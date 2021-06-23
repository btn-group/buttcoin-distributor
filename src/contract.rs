use crate::msg::ResponseStatus::Success;
use crate::msg::{
    HandleAnswer, HandleMsg, InitMsg, LPStakingHandleMsg, LPStakingQueryMsg, LPStakingReceiveMsg,
    LPStakingRewardsResponse, QueryAnswer, QueryMsg, ReceiveMsg,
};
use crate::state::{config, config_read, SecretContract, State};
use cosmwasm_std::{
    from_binary, to_binary, Api, Binary, CosmosMsg, Env, Extern, HandleResponse, HumanAddr,
    InitResponse, Querier, QueryResult, StdError, StdResult, Storage, Uint128,
};
use secret_toolkit::snip20;
use secret_toolkit::utils::{pad_handle_result, HandleCallback, Query};

// === CONSTANTS ===
pub const RESPONSE_BLOCK_SIZE: usize = 256;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let buttcoin = SecretContract {
        address: HumanAddr::from("secret1yxcexylwyxlq58umhgsjgstgcg2a0ytfy4d9lt"),
        contract_hash: "F8B27343FF08290827560A1BA358EECE600C9EA7F403B02684AD87AE7AF0F288"
            .to_string(),
    };

    let state: State = State {
        admin: env.message.sender.clone(),
        buttcoin: buttcoin,
        contract_address: env.contract.address,
        farm_contract: msg.farm_contract,
        profit_sharing_contract: msg.profit_sharing_contract,
        token: msg.token.clone(),
        shares_token: msg.shares_token.clone(),
        viewing_key: msg.viewing_key.clone(),
        stopped: false,
    };

    config(&mut deps.storage).save(&state)?;

    // https://github.com/enigmampc/secret-toolkit/tree/master/packages/snip20
    let messages = vec![
        snip20::register_receive_msg(
            env.contract_code_hash,
            None,
            1,
            msg.token.contract_hash.clone(),
            msg.token.address.clone(),
        )?,
        snip20::set_viewing_key_msg(
            msg.viewing_key,
            None,
            RESPONSE_BLOCK_SIZE,
            msg.token.contract_hash,
            msg.token.address,
        )?,
    ];

    Ok(InitResponse {
        messages,
        log: vec![],
    })
}

pub fn query<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>, msg: QueryMsg) -> QueryResult {
    match msg {
        QueryMsg::Config {} => query_public_config(deps),
        QueryMsg::Balance { token } => query_balance(deps, token),
    }
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    let state: State = config_read(&deps.storage).load()?;
    if state.stopped {
        return match msg {
            HandleMsg::ResumeContract {} => resume_contract(deps, env),
            _ => Err(StdError::generic_err(
                "this contract is stopped and this action is not allowed",
            )),
        };
    }

    let response = match msg {
        HandleMsg::Receive {
            from, amount, msg, ..
        } => receive(deps, env, from, amount.u128(), msg),
        HandleMsg::StopContract {} => stop_contract(deps, env),
        _ => Err(StdError::generic_err("Unavailable or unknown action")),
    };

    pad_handle_result(response, RESPONSE_BLOCK_SIZE)
}

fn deposit<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    from: HumanAddr,
    amount: u128,
) -> StdResult<HandleResponse> {
    let mut messages: Vec<CosmosMsg> = vec![];
    let state: State = config_read(&deps.storage).load()?;

    // 1. Calculate balance
    let unstaked_balance_of_contract: u128 =
        match from_binary(&query_balance(deps, state.token.clone()).unwrap()).unwrap() {
            QueryAnswer::Balance { amount } => amount.u128(),
            _ => panic!("Unexpected result from handle"),
        };
    let balance_in_farm_contract: u128 =
        match from_binary(&query_balance(deps, state.farm_contract.clone()).unwrap()).unwrap() {
            QueryAnswer::Balance { amount } => amount.u128(),
            _ => panic!("Unexpected result from handle"),
        };
    let rewards_response: LPStakingRewardsResponse =
        from_binary(&query_rewards(&deps.querier, env, state.clone()).unwrap()).unwrap();
    let unclaimed_rewards_of_contract = rewards_response.rewards.rewards.u128();
    let balance_of_pool: u128 =
        unstaked_balance_of_contract + balance_in_farm_contract + unclaimed_rewards_of_contract;

    // 2. Calculate shares to give to user
    let total_shares: u128 =
        match from_binary(&query_balance(deps, state.shares_token.clone()).unwrap()).unwrap() {
            QueryAnswer::Balance { amount } => amount.u128(),
            _ => panic!("Unexpected result from handle"),
        };
    let shares_for_this_deposit: u128 = if total_shares == 0 {
        amount
    } else {
        let balance_of_pool_before_deposit = balance_of_pool - amount;
        amount / balance_of_pool_before_deposit * total_shares
    };

    // 3. Mint tokens for the user
    messages.push(secret_toolkit::snip20::mint_msg(
        from,
        Uint128(shares_for_this_deposit),
        None,
        RESPONSE_BLOCK_SIZE,
        state.shares_token.contract_hash,
        state.shares_token.address,
    )?);

    // 4. Send unstaked balance to farm contract (receive rewards at the same time)
    messages.push(secret_toolkit::snip20::send_msg(
        state.farm_contract.address.clone(),
        Uint128(unstaked_balance_of_contract),
        Some(to_binary(&LPStakingReceiveMsg::Deposit {})?),
        None,
        RESPONSE_BLOCK_SIZE,
        state.token.contract_hash.clone(),
        state.token.address.clone(),
    )?);

    // 5. Calculate fees and send to profit sharing contract
    let fee: u128 = unclaimed_rewards_of_contract * 500 / 10_000;
    messages.push(secret_toolkit::snip20::transfer_msg(
        state.profit_sharing_contract.address.clone(),
        Uint128(fee),
        None,
        RESPONSE_BLOCK_SIZE,
        state.token.contract_hash,
        state.token.address,
    )?);

    Ok(HandleResponse {
        messages,
        log: vec![],
        data: None,
    })
}

fn enforce_admin(state: State, env: Env) -> StdResult<()> {
    if state.admin != env.message.sender {
        return Err(StdError::generic_err(format!(
            "not an admin: {}",
            env.message.sender
        )));
    }

    Ok(())
}

fn query_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    token: SecretContract,
) -> QueryResult {
    let state: State = config_read(&deps.storage).load()?;
    let balance = snip20::balance_query(
        &deps.querier,
        state.contract_address,
        state.viewing_key,
        RESPONSE_BLOCK_SIZE,
        token.contract_hash,
        token.address,
    )?;
    to_binary(&QueryAnswer::Balance {
        amount: balance.amount,
    })
}

fn query_public_config<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> QueryResult {
    let state: State = config_read(&deps.storage).load()?;

    to_binary(&QueryAnswer::Config {
        admin: state.admin,
        farm_contract: state.farm_contract,
        shares_token: state.shares_token,
        stopped: state.stopped,
        token: state.token,
    })
}

fn receive<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    from: HumanAddr,
    amount: u128,
    msg: Binary,
) -> StdResult<HandleResponse> {
    let msg: ReceiveMsg = from_binary(&msg)?;

    match msg {
        ReceiveMsg::Deposit {} => deposit(deps, env, from, amount),
        ReceiveMsg::Withdraw {} => withdraw(deps, env, from, amount),
    }
}

fn resume_contract<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let mut state: State = config_read(&deps.storage).load()?;

    enforce_admin(state.clone(), env)?;

    state.stopped = false;
    config(&mut deps.storage).save(&state)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&HandleAnswer::ResumeContract {
            status: Success,
        })?),
    })
}

fn stop_contract<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let mut state: State = config_read(&deps.storage).load()?;

    enforce_admin(state.clone(), env)?;

    state.stopped = true;
    config(&mut deps.storage).save(&state)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&HandleAnswer::StopContract { status: Success })?),
    })
}

fn query_rewards<Q: Querier>(querier: &Q, env: Env, state: State) -> QueryResult {
    let rewards_query_msg = LPStakingQueryMsg::Rewards {
        address: state.contract_address,
        key: state.viewing_key,
        height: env.block.height,
    };
    let rewards_response: LPStakingRewardsResponse = rewards_query_msg.query(
        querier,
        state.farm_contract.contract_hash,
        state.farm_contract.address,
    )?;

    to_binary(&rewards_response)
}

fn withdraw<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    from: HumanAddr,
    amount_of_shares: u128,
) -> StdResult<HandleResponse> {
    let state: State = config_read(&deps.storage).load()?;

    // 1. Calculate balance
    let unstaked_balance_of_contract: u128 =
        match from_binary(&query_balance(deps, state.token.clone()).unwrap()).unwrap() {
            QueryAnswer::Balance { amount } => amount.u128(),
            _ => panic!("Unexpected result from handle"),
        };
    let balance_in_farm_contract: u128 =
        match from_binary(&query_balance(deps, state.farm_contract.clone()).unwrap()).unwrap() {
            QueryAnswer::Balance { amount } => amount.u128(),
            _ => panic!("Unexpected result from handle"),
        };
    let rewards_response: LPStakingRewardsResponse =
        from_binary(&query_rewards(&deps.querier, env, state.clone()).unwrap()).unwrap();
    let unclaimed_rewards_of_contract = rewards_response.rewards.rewards.u128();
    let balance_of_pool: u128 =
        unstaked_balance_of_contract + balance_in_farm_contract + unclaimed_rewards_of_contract;

    // 2. Calculate shares before send
    let total_shares: u128 =
        match from_binary(&query_balance(deps, state.shares_token.clone()).unwrap()).unwrap() {
            QueryAnswer::Balance { amount } => amount.u128(),
            _ => panic!("Unexpected result from handle"),
        };
    let total_shares_before_send: u128 = total_shares + amount_of_shares;

    // 3. Burn the tokens from the user
    let mut messages: Vec<CosmosMsg> = vec![snip20::burn_msg(
        Uint128(amount_of_shares),
        None,
        RESPONSE_BLOCK_SIZE,
        state.shares_token.contract_hash.clone(),
        state.shares_token.address.clone(),
    )?];

    // 4. Calculate amount of token to withdraw from farm contract and to send
    let amount_of_token: u128 = balance_of_pool * amount_of_shares / total_shares_before_send;
    let amount_to_withdraw_from_farm_contract: u128 = if amount_of_token > balance_in_farm_contract
    {
        balance_in_farm_contract
    } else {
        amount_of_token
    };
    let redeem_msg = LPStakingHandleMsg::Redeem {
        amount: Some(Uint128(amount_to_withdraw_from_farm_contract)),
    };
    let cosmos_msg = redeem_msg.to_cosmos_msg(
        state.farm_contract.contract_hash.clone(),
        state.farm_contract.address.clone(),
        None,
    )?;

    // 5. Withdraw from farm contract
    messages.push(cosmos_msg);

    // 6. At this point we've got the withdrawal from the farm address and the unclaimed reward so it's time to transfer the token back to the user
    messages.push(secret_toolkit::snip20::transfer_msg(
        from,
        Uint128(amount_of_token),
        None,
        RESPONSE_BLOCK_SIZE,
        state.token.contract_hash.clone(),
        state.token.address.clone(),
    )?);

    // 7. Commission from claimed rewards
    // At this point the reward will be in the account and the performance fee will be sent to reward contract
    let fee: u128 = unclaimed_rewards_of_contract * 500 / 10_000;
    messages.push(secret_toolkit::snip20::transfer_msg(
        state.admin,
        Uint128(fee),
        None,
        RESPONSE_BLOCK_SIZE,
        state.token.contract_hash,
        state.token.address,
    )?);

    Ok(HandleResponse {
        messages,
        log: vec![],
        data: Some(to_binary(&HandleAnswer::Redeem { status: Success })?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg::ResponseStatus;
    use cosmwasm_std::testing::*;
    use cosmwasm_std::QueryResponse;
    use std::any::Any;

    // === HELPER FUNCTIONS ===

    fn init_helper() -> (
        StdResult<InitResponse>,
        Extern<MockStorage, MockApi, MockQuerier>,
    ) {
        let mut deps = mock_dependencies(20, &[]);
        let env = mock_env("admin", &[]);

        let init_msg = InitMsg {
            farm_contract: SecretContract {
                address: HumanAddr("farm-contract-address".to_string()),
                contract_hash: "farm-contract-hash".to_string(),
            },
            profit_sharing_contract: SecretContract {
                address: HumanAddr("profit-sharing-contract-address".to_string()),
                contract_hash: "profit-sharing-contract-hash".to_string(),
            },
            token: SecretContract {
                address: HumanAddr("token-contract-address".to_string()),
                contract_hash: "token-contract-hash".to_string(),
            },
            shares_token: SecretContract {
                address: HumanAddr("shares-token-contract-address".to_string()),
                contract_hash: "shares-token-contract-hash".to_string(),
            },
            viewing_key: "btn-viewing-key".to_string(),
        };

        (init(&mut deps, env, init_msg), deps)
    }

    fn extract_error_msg<T: Any>(error: StdResult<T>) -> String {
        match error {
            Ok(response) => {
                let bin_err = (&response as &dyn Any)
                    .downcast_ref::<QueryResponse>()
                    .expect("An error was expected, but no error could be extracted");
                match from_binary(bin_err).unwrap() {
                    QueryAnswer::ViewingKeyError { msg } => msg,
                    _ => panic!("Unexpected query answer"),
                }
            }
            Err(err) => match err {
                StdError::GenericErr { msg, .. } => msg,
                _ => panic!("Unexpected result from init"),
            },
        }
    }

    fn ensure_success(handle_result: HandleResponse) -> bool {
        let handle_result: HandleAnswer = from_binary(&handle_result.data.unwrap()).unwrap();

        match handle_result {
            HandleAnswer::ResumeContract { status } | HandleAnswer::StopContract { status } => {
                matches!(status, ResponseStatus::Success { .. })
            }
            _ => panic!("HandleAnswer not supported for success extraction"),
        }
    }

    // === INIT TESTS ===

    #[test]
    fn test_init_sanity() {
        let (init_result, deps) = init_helper();
        let state: State = config_read(&deps.storage).load().unwrap();
        let env = mock_env("admin", &[]);

        assert_eq!(
            init_result.unwrap(),
            InitResponse {
                messages: vec![
                    snip20::register_receive_msg(
                        env.contract_code_hash,
                        None,
                        1,
                        state.token.contract_hash.clone(),
                        state.token.address.clone(),
                    )
                    .unwrap(),
                    snip20::set_viewing_key_msg(
                        state.viewing_key,
                        None,
                        RESPONSE_BLOCK_SIZE,
                        state.token.contract_hash,
                        state.token.address,
                    )
                    .unwrap(),
                ],
                log: vec![],
            },
        );

        assert_eq!(
            state.farm_contract.address,
            HumanAddr("farm-contract-address".to_string())
        );
        assert_eq!(
            state.farm_contract.contract_hash,
            "farm-contract-hash".to_string()
        );
    }

    // === HANDLE TESTS ===

    #[test]
    fn test_handle_admin_commands() {
        let admin_err = "not an admin".to_string();
        // Init
        let (init_result, mut deps) = init_helper();
        assert!(
            init_result.is_ok(),
            "Init failed: {}",
            init_result.err().unwrap()
        );

        // Stop Contract
        let stop_contract_msg = HandleMsg::StopContract {};
        // When user is not an admin
        let handle_result = handle(
            &mut deps,
            mock_env("not_admin", &[]),
            stop_contract_msg.clone(),
        );
        let error = extract_error_msg(handle_result);
        assert!(error.contains(&admin_err.clone()));
        // When user is an admin
        let handle_result = handle(&mut deps, mock_env("admin", &[]), stop_contract_msg);
        let result = handle_result.unwrap();
        assert!(ensure_success(result));

        // Resune Contract
        let resume_contract_msg = HandleMsg::ResumeContract {};
        // When user is not an admin
        let handle_result = handle(
            &mut deps,
            mock_env("not_admin", &[]),
            resume_contract_msg.clone(),
        );
        let error = extract_error_msg(handle_result);
        assert!(error.contains(&admin_err.clone()));
        // When user is an admin
        let handle_result = handle(&mut deps, mock_env("admin", &[]), resume_contract_msg);
        let result = handle_result.unwrap();
        assert!(ensure_success(result));
    }

    // === QUERY TESTS ===

    #[test]
    fn test_query_config() {
        let (_init_result, deps) = init_helper();
        let state: State = config_read(&deps.storage).load().unwrap();
        let query_result = query(&deps, QueryMsg::Config {}).unwrap();
        let query_answer: QueryAnswer = from_binary(&query_result).unwrap();
        match query_answer {
            QueryAnswer::Config {
                admin,
                stopped,
                farm_contract,
                shares_token,
                token,
            } => {
                assert_eq!(admin, state.admin);
                assert_eq!(stopped, state.stopped);
                assert_eq!(farm_contract, state.farm_contract);
                assert_eq!(shares_token, state.shares_token);
                assert_eq!(token, state.token);
            }
            _ => panic!("unexpected"),
        }
    }
}
