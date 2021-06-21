// APPLY performance_fee of 3% (300/10_000) when calculating fees etc
// Do we need governanceRecoverUnsupported in Secret Network?
// Can you send unsupported coins in Secret Network or does the regeister stuff in init prevent that?
use crate::msg::ResponseStatus::Success;
use crate::msg::{HandleAnswer, HandleMsg, InitMsg, QueryAnswer, QueryMsg, ReceiveMsg};
use crate::state::{config, config_read, SecretContract, State};
use cosmwasm_std::{
    from_binary, to_binary, Api, Binary, CosmosMsg, Env, Extern, HandleResponse, HumanAddr,
    InitResponse, Querier, QueryRequest, QueryResult, StdError, StdResult, Storage, Uint128,
    WasmQuery,
};
use secret_toolkit::snip20;
use secret_toolkit::utils::pad_handle_result;

// === CONSTANTS ===
pub const RESPONSE_BLOCK_SIZE: usize = 256;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let state: State = State {
        admin: env.message.sender.clone(),
        contract_address: env.contract.address,
        farm_contract: msg.farm_contract,
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
        // HandleMsg::Redeem { amount } => withdraw(deps, env, amount),
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
    // // 1. Calculate the user's share of the pool

    // // 2. Mint tokens for the user
    // messages.push(secret_toolkit::snip20::mint_msg(
    //     from,
    //     Uint128(_shares_for_this_deposit),
    //     None,
    //     RESPONSE_BLOCK_SIZE,
    //     state.shares_token.contract_hash,
    //     state.shares_token.address,
    // )?);

    // let total_shares: u128 = query_balance(deps, state.shares_token)
    //     .unwrap()
    //     .u128();
    // let _shares_for_this_deposit: u128 = if total_shares == 0 {
    //     amount
    // } else {
    //     let balance_of_pool_before_deposit =
    //         balance_of_pool(&deps.querier, env.clone(), state.clone()).unwrap() - amount;
    //     // 4. Shares for this deposit
    //     amount * total_shares / balance_of_pool_before_deposit
    // };
    // deposit_into_farm_contract(deps, env.clone())?;

    Ok(HandleResponse {
        messages,
        log: vec![],
        data: None,
    })
}

// fn deposit_into_farm_contract<S: Storage, A: Api, Q: Querier>(
//     deps: &mut Extern<S, A, Q>,
//     env: Env,
// ) -> StdResult<HandleResponse> {
//     let state: State = config_read(&deps.storage).load()?;
//     let mut messages: Vec<CosmosMsg> = vec![];
//     let balance_of_this_contract: u128 =
//         balance_of_this_contract(&deps.querier, env.clone(), state.clone()).unwrap();
//     messages.push(secret_toolkit::snip20::send_msg(
//         state.farm_contract.address.clone(),
//         Uint128(balance_of_this_contract),
//         Some(to_binary(&HandleMsg::Receive {
//             sender: env.contract.address.clone(),
//             from: env.contract.address.clone(),
//             amount: Uint128(balance_of_this_contract),
//             msg: to_binary(&ReceiveMsg::Deposit {})?,
//         })?),
//         None,
//         RESPONSE_BLOCK_SIZE,
//         state.token.contract_hash.clone(),
//         state.token.address.clone(),
//     )?);

//     // At this point the reward will be in the account and the performance fee will be sent to admin
//     let commission: u128 =
//         unclaimed_rewards(&deps.querier, env.clone(), state.clone()).unwrap() * 500 / 10_000;
//     messages.push(secret_toolkit::snip20::transfer_msg(
//         state.admin,
//         Uint128(commission),
//         None,
//         RESPONSE_BLOCK_SIZE,
//         state.token.contract_hash,
//         state.token.address,
//     )?);

//     Ok(HandleResponse {
//         messages,
//         log: vec![],
//         data: None,
//     })
// }

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

// //As such, we provide the Querier with read-only access to the state snapshot right before execution of the current CosmWasm message. Since we take a snapshot and both the executing contract and the queried contract have read-only access to the data before the contract execution, this is still safe with Rust's borrowing rules (as a placeholder for secure design). The current contract only writes to a cache, which is flushed afterwards on success.
// fn balance_of_pool<Q: Querier>(querier: &Q, env: Env, state: State) -> StdResult<u128> {
//     // 1. Get unclaimed rewards in third party contract
//     let unclaimed_rewards: u128 = querier
//         .query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: state.farm_contract.address.clone(),
//             callback_code_hash: state.farm_contract.contract_hash.clone(),
//             msg: to_binary(&QueryMsg::Rewards {
//                 address: env.contract.address.clone(),
//                 key: state.viewing_key.clone(),
//                 height: env.block.height,
//             })?,
//         }))
//         .map_err(|err| StdError::generic_err(format!("Got an error from query: {:?}", err)))?;

//     // 2. Get total locked in third party
//     let total_locked_in_farm_contract: u128 =
//         total_locked_in_farm_contract(querier, env.clone(), state.clone()).unwrap();
//     // 3. Get balance of this contract - the new amount?
//     // DO I need the response_block_size_here? I don't really care who sees the balance etc
//     // I want people to see everything so that they can check everything is right
//     let balance_of_this_contract: u128 =
//         balance_of_this_contract(querier, env.clone(), state.clone()).unwrap();
//     Ok(unclaimed_rewards + total_locked_in_farm_contract + balance_of_this_contract)
// }

// fn unclaimed_rewards<Q: Querier>(querier: &Q, env: Env, state: State) -> StdResult<u128> {
//     querier
//         .query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: state.farm_contract.address.clone(),
//             callback_code_hash: state.farm_contract.contract_hash.clone(),
//             msg: to_binary(&QueryMsg::Rewards {
//                 address: env.contract.address.clone(),
//                 key: state.viewing_key.clone(),
//                 height: env.block.height,
//             })?,
//         }))
//         .map_err(|err| StdError::generic_err(format!("Got an error from query: {:?}", err)))?
// }

fn balance_of_this_contract<Q: Querier>(querier: &Q, env: Env, state: State) -> StdResult<u128> {
    Ok((snip20::balance_query(
        querier,
        env.contract.address.clone(),
        state.viewing_key,
        RESPONSE_BLOCK_SIZE,
        env.contract_code_hash.clone(),
        env.contract.address.clone(),
    )?)
    .amount
    .u128())
}

// fn total_locked_in_farm_contract<Q: Querier>(
//     querier: &Q,
//     env: Env,
//     state: State,
// ) -> StdResult<u128> {
//     let amount: u128 = querier
//         .query(&QueryRequest::Wasm(WasmQuery::Smart {
//             contract_addr: state.farm_contract.address,
//             callback_code_hash: state.farm_contract.contract_hash,
//             msg: to_binary(&QueryMsg::Balance {
//                 address: env.contract.address.clone(),
//                 key: state.viewing_key.clone(),
//             })?,
//         }))
//         .map_err(|err| StdError::generic_err(format!("Got an error from query: {:?}", err)))?;

//     Ok(amount)
// }

// fn withdraw<S: Storage, A: Api, Q: Querier>(
//     deps: &mut Extern<S, A, Q>,
//     env: Env,
//     amount_of_shares: Uint128,
// ) -> StdResult<HandleResponse> {
//     let state: State = config_read(&deps.storage).load()?;
//     // 1. Burn the tokens from the user
//     let mut messages: Vec<CosmosMsg> = vec![snip20::burn_from_msg(
//         env.message.sender.clone(),
//         amount_of_shares,
//         None,
//         RESPONSE_BLOCK_SIZE,
//         state.shares_token.contract_hash.clone(),
//         state.shares_token.address.clone(),
//     )?];
//     let total_shares: u128 = query_balance(deps, state.shares_token)
//         .unwrap()
//         .u128();
//     // 2. At this point we need to figure out how much of the token to withdraw from farm contract
//     let amount_of_token: u128 = balance_of_pool(&deps.querier, env.clone(), state.clone()).unwrap()
//         * amount_of_shares.u128()
//         / total_shares;
//     let total_locked_in_farm_contract: u128 =
//         total_locked_in_farm_contract(&deps.querier, env.clone(), state.clone()).unwrap();
//     let amount_to_withdraw_from_farm_contract: u128 =
//         if amount_of_token > total_locked_in_farm_contract {
//             total_locked_in_farm_contract
//         } else {
//             amount_of_token
//         };
//     messages.push(
//         WasmMsg::Execute {
//             contract_addr: state.farm_contract.address.clone(),
//             callback_code_hash: state.farm_contract.contract_hash.clone(),
//             msg: to_binary(&HandleMsg::Redeem {
//                 amount: Uint128(amount_to_withdraw_from_farm_contract),
//             })?,
//             send: vec![],
//         }
//         .into(),
//     );
//     // 3. At this point we've got the withdrawal from the farm address and the unclaimed reward so it's time to transfer the token back to the user
//     messages.push(secret_toolkit::snip20::transfer_msg(
//         env.message.sender.clone(),
//         Uint128(amount_of_token),
//         None,
//         RESPONSE_BLOCK_SIZE,
//         state.token.contract_hash.clone(),
//         state.token.address.clone(),
//     )?);

//     // 4. Commission from claimed rewards
//     // At this point the reward will be in the account and the performance fee will be sent to admin
//     let commission: u128 =
//         unclaimed_rewards(&deps.querier, env.clone(), state.clone()).unwrap() * 500 / 10_000;
//     messages.push(secret_toolkit::snip20::transfer_msg(
//         state.admin,
//         Uint128(commission),
//         None,
//         RESPONSE_BLOCK_SIZE,
//         state.token.contract_hash,
//         state.token.address,
//     )?);

//     Ok(HandleResponse {
//         messages,
//         log: vec![],
//         data: Some(to_binary(&HandleAnswer::Redeem { status: Success })?),
//     })
// }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg::ResponseStatus;
    use cosmwasm_std::testing::*;
    use cosmwasm_std::QueryResponse;
    use std::any::Any;

    //=== HELPER FUNCTIONS ===

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

    // Init tests

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

    // Handle tests

    // #[test]
    // fn test_handle_transfer() {
    //     // Initialize
    //     let (init_result, mut deps) = init_helper();
    //     assert!(
    //         init_result.is_ok(),
    //         "Init failed: {}",
    //         init_result.err().unwrap()
    //     );

    //     // Set bob as minter
    //     let handle_msg = HandleMsg::SetMinters {
    //         minters: vec![HumanAddr("bob".to_string())],
    //         padding: None,
    //     };
    //     let _handle_result = handle(&mut deps, mock_env("admin", &[]), handle_msg);

    //     // Try mint to bob
    //     let mint_amount: u128 = 5000;
    //     let handle_msg = HandleMsg::Mint {
    //         recipient: HumanAddr("bob".to_string()),
    //         amount: Uint128(mint_amount),
    //         padding: None,
    //     };
    //     let _handle_result = handle(&mut deps, mock_env("bob", &[]), handle_msg);

    //     // Transfer from bob to alice
    //     let handle_msg = HandleMsg::Transfer {
    //         recipient: HumanAddr("alice".to_string()),
    //         amount: Uint128(1000),
    //         padding: None,
    //     };
    //     let handle_result = handle(&mut deps, mock_env("bob", &[]), handle_msg);
    //     let result = handle_result.unwrap();
    //     assert!(ensure_success(result));

    //     // Check bob and alice's balances are correct after transfer
    //     let bob_canonical = deps
    //         .api
    //         .canonical_address(&HumanAddr("bob".to_string()))
    //         .unwrap();
    //     let alice_canonical = deps
    //         .api
    //         .canonical_address(&HumanAddr("alice".to_string()))
    //         .unwrap();
    //     let balances = ReadonlyBalances::from_storage(&deps.storage);
    //     assert_eq!(5000 - 1000, balances.account_amount(&bob_canonical));
    //     assert_eq!(1000, balances.account_amount(&alice_canonical));

    //     // Try to transfer more than alice's balance to bob
    //     let handle_msg = HandleMsg::Transfer {
    //         recipient: HumanAddr("alice".to_string()),
    //         amount: Uint128(10000),
    //         padding: None,
    //     };
    //     let handle_result = handle(&mut deps, mock_env("bob", &[]), handle_msg);
    //     let error = extract_error_msg(handle_result);
    //     assert!(error.contains("insufficient funds"));
    // }

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

    //=== QUERY TESTS ===
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
