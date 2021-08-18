use crate::msg::ButtcoinDistributorResponseStatus::Success;
use crate::msg::{
    ButtcoinDistributorHandleAnswer, ButtcoinDistributorHandleMsg, ButtcoinDistributorQueryAnswer,
    ButtcoinDistributorQueryMsg, InitMsg, YieldOptimizerReceiveMsg,
};
use crate::state::{config, config_read, SecretContract, State};
use cosmwasm_std::{
    to_binary, Api, Binary, Env, Extern, HandleResponse, HumanAddr, InitResponse, Querier,
    StdError, StdResult, Storage, Uint128,
};
use secret_toolkit::snip20;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let state = State {
        buttcoin: msg.buttcoin.clone(),
        end_block: msg.end_block,
        last_update_block: msg.starting_block,
        receivable_smart_contract: None,
        release_per_block: msg.release_per_block,
        starting_block: msg.starting_block,
        viewing_key: msg.viewing_key.clone(),
    };

    config(&mut deps.storage).save(&state)?;

    let messages = vec![snip20::set_viewing_key_msg(
        msg.viewing_key,
        None,
        1,
        msg.buttcoin.contract_hash,
        msg.buttcoin.address,
    )?];

    Ok(InitResponse {
        messages,
        log: vec![],
    })
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: ButtcoinDistributorHandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        ButtcoinDistributorHandleMsg::SetReceivableSmartContract {
            receivable_smart_contract,
        } => set_receivable_smart_contract(deps, env, receivable_smart_contract),
        ButtcoinDistributorHandleMsg::ClaimButtcoin { hook } => claim_buttcoin(deps, env, hook),
    }
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: ButtcoinDistributorQueryMsg,
) -> StdResult<Binary> {
    match msg {
        ButtcoinDistributorQueryMsg::Config {} => to_binary(&query_config(deps)?),
        ButtcoinDistributorQueryMsg::Pending { block } => {
            to_binary(&query_pending_rewards(deps, block)?)
        }
    }
}

fn query_config<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<ButtcoinDistributorQueryAnswer> {
    let state: State = config_read(&deps.storage).load()?;

    Ok(ButtcoinDistributorQueryAnswer::Config {
        buttcoin: state.buttcoin,
        end_block: state.end_block,
        last_update_block: state.last_update_block,
        receivable_smart_contract: state.receivable_smart_contract,
        release_per_block: state.release_per_block,
        starting_block: state.starting_block,
        viewing_key: state.viewing_key,
    })
}

fn query_pending_rewards<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    block: u64,
) -> StdResult<ButtcoinDistributorQueryAnswer> {
    let state = config_read(&deps.storage).load()?;
    let amount = get_receivable_contract_rewards(block, state);

    Ok(ButtcoinDistributorQueryAnswer::Pending {
        amount: Uint128(amount),
    })
}

fn get_receivable_contract_rewards(block: u64, state: State) -> u128 {
    if block > state.last_update_block {
        let block = if block > state.end_block {
            state.end_block
        } else {
            block
        };

        (block - state.last_update_block) as u128 * state.release_per_block.u128()
    } else {
        0
    }
}

fn claim_buttcoin<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    hook: Option<Binary>,
) -> StdResult<HandleResponse> {
    let mut state = config_read(&deps.storage).load()?;
    let mut rewards = 0;

    if state.receivable_smart_contract.is_some() {
        if state.clone().receivable_smart_contract.unwrap().address == env.message.sender {
            rewards = get_receivable_contract_rewards(env.block.height, state.clone());
            state.last_update_block = env.block.height;
            config(&mut deps.storage).save(&state)?;
        }
    }

    Ok(HandleResponse {
        messages: vec![snip20::send_msg(
            env.message.sender.clone(),
            Uint128(rewards),
            Some(to_binary(&YieldOptimizerReceiveMsg::DepositButtcoin {
                hook: hook,
            })?),
            None,
            1,
            state.buttcoin.contract_hash.clone(),
            state.buttcoin.address,
        )?],
        log: vec![],
        data: Some(to_binary(
            &ButtcoinDistributorHandleAnswer::ClaimButtcoin { status: Success },
        )?),
    })
}

fn set_receivable_smart_contract<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    receivable_smart_contract: SecretContract,
) -> StdResult<HandleResponse> {
    let mut state = config_read(&deps.storage).load()?;
    if state.receivable_smart_contract.is_some() {
        return Err(StdError::generic_err(format!(
            "Receivable smart contract can only be set once!"
        )));
    }

    state.receivable_smart_contract = Some(receivable_smart_contract);
    config(&mut deps.storage).save(&state)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(
            &ButtcoinDistributorHandleAnswer::SetReceivableSmartContract { status: Success },
        )?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SecretContract;
    use cosmwasm_std::from_binary;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, MockApi, MockQuerier, MockStorage};

    // === CONSTANTS ===
    pub const MOCK_SMART_CONTRACT_INITIALIZER: &str = "smart_contract_initializer";

    // === HELPERS ===
    fn init_helper() -> (
        StdResult<InitResponse>,
        Extern<MockStorage, MockApi, MockQuerier>,
    ) {
        let env = mock_env(MOCK_SMART_CONTRACT_INITIALIZER, &[]);
        let mut deps = mock_dependencies(20, &[]);
        let msg = InitMsg {
            buttcoin: mock_buttcoin(),
            end_block: 12,
            release_per_block: Uint128(34),
            starting_block: 11,
            viewing_key: mock_viewing_key(),
        };
        (init(&mut deps, env.clone(), msg), deps)
    }

    fn mock_buttcoin() -> SecretContract {
        SecretContract {
            address: HumanAddr::from("buttcoincontractaddress"),
            contract_hash: "buttcoincontracthash".to_string(),
        }
    }

    fn mock_viewing_key() -> String {
        "viewing_key".to_string()
    }

    fn mock_yield_optimizer_smart_contract() -> SecretContract {
        SecretContract {
            address: HumanAddr::from("yieldoptimizersmartcontractaddress"),
            contract_hash: "yieldoptimizersmartcontracthash".to_string(),
        }
    }

    // === QUERY ===

    #[test]
    fn test_query_config() {
        let (_init_result, deps) = init_helper();
        let res =
            from_binary(&query(&deps, ButtcoinDistributorQueryMsg::Config {}).unwrap()).unwrap();
        match res {
            ButtcoinDistributorQueryAnswer::Config {
                buttcoin,
                end_block,
                last_update_block,
                receivable_smart_contract,
                release_per_block,
                starting_block,
                viewing_key,
            } => {
                assert_eq!(buttcoin, mock_buttcoin());
                assert_eq!(end_block, 12);
                assert_eq!(last_update_block, 11);
                assert_eq!(receivable_smart_contract, None);
                assert_eq!(release_per_block, Uint128(34));
                assert_eq!(starting_block, 11);
                assert_eq!(viewing_key, mock_viewing_key());
            }
            _ => panic!("unexpected error"),
        }
    }

    // #[test]
    // fn test_query_pending_rewards() {
    //     let (_init_result, mut deps) = init_helper();
    //     let env = mock_env(MOCK_SMART_CONTRACT_INITIALIZER, &[]);

    //     // = When contract has no weight
    //     // = * It returns 0
    //     let res = from_binary(
    //         &query(
    //             &deps,
    //             ButtcoinDistributorQueryMsg::Pending {
    //                 block: 1,
    //                 receivable_contract_address: HumanAddr::from(MOCK_SMART_CONTRACT_INITIALIZER),
    //             },
    //         )
    //         .unwrap(),
    //     )
    //     .unwrap();
    //     match res {
    //         ButtcoinDistributorQueryAnswer::Pending { amount } => {
    //             assert_eq!(amount, Uint128(0));
    //         }
    //         _ => panic!("unexpected error"),
    //     }

    //     // = When contract has weight
    //     let handle_msg = ButtcoinDistributorHandleMsg::SetWeights {
    //         weights: vec![WeightInfo {
    //             address: mock_yield_optimizer_smart_contract().address,
    //             hash: mock_yield_optimizer_smart_contract().contract_hash,
    //             weight: 123,
    //         }],
    //     };
    //     handle(&mut deps, mock_env(MOCK_SMART_CONTRACT_INITIALIZER, &[]), handle_msg).unwrap();

    //     // == When there is no schedule
    //     // == * It returns 0
    //     let res = from_binary(
    //         &query(
    //             &deps,
    //             ButtcoinDistributorQueryMsg::Pending {
    //                 block: 1,
    //                 receivable_contract_address: HumanAddr::from(MOCK_SMART_CONTRACT_INITIALIZER),
    //             },
    //         )
    //         .unwrap(),
    //     )
    //     .unwrap();
    //     match res {
    //         ButtcoinDistributorQueryAnswer::Pending { amount } => {
    //             assert_eq!(amount, Uint128(0));
    //         }
    //         _ => panic!("unexpected error"),
    //     }

    //     // === When there is a schedule
    //     let new_release_schedule: Schedule = vec![
    //         ScheduleUnit {
    //             end_block: env.block.height + 3000,
    //             release_per_block: Uint128(3000),
    //         },
    //         ScheduleUnit {
    //             end_block: env.block.height + 6000,
    //             release_per_block: Uint128(6000),
    //         },
    //     ];

    //     let handle_msg = ButtcoinDistributorHandleMsg::SetSchedule {
    //         schedule: new_release_schedule.clone(),
    //     };
    //     handle(&mut deps, mock_env(MOCK_SMART_CONTRACT_INITIALIZER, &[]), handle_msg).unwrap();
    //     // ==== * When block specified is before weight was added
    //     // ==== * It returns 0
    //     let res = from_binary(
    //         &query(
    //             &deps,
    //             ButtcoinDistributorQueryMsg::Pending {
    //                 block: 1,
    //                 receivable_contract_address: mock_yield_optimizer_smart_contract().address,
    //             },
    //         )
    //         .unwrap(),
    //     )
    //     .unwrap();
    //     match res {
    //         ButtcoinDistributorQueryAnswer::Pending { amount } => {
    //             assert_eq!(amount, Uint128(0));
    //         }
    //         _ => panic!("unexpected error"),
    //     }
    //     // ==== * When block specified is one after when weight was added
    //     // ==== * It returns the correct amount
    //     let res = from_binary(
    //         &query(
    //             &deps,
    //             ButtcoinDistributorQueryMsg::Pending {
    //                 block: env.block.height + 1,
    //                 receivable_contract_address: mock_yield_optimizer_smart_contract().address,
    //             },
    //         )
    //         .unwrap(),
    //     )
    //     .unwrap();
    //     match res {
    //         ButtcoinDistributorQueryAnswer::Pending { amount } => {
    //             assert_eq!(amount, Uint128(3_000));
    //         }
    //         _ => panic!("unexpected error"),
    //     }
    // }

    // === HANDLE ===

    // #[test]
    // fn test_handle_claim_buttcoin() {
    //     let (_init_result, mut deps) = init_helper();
    //     let env = mock_env("admin", &[]);

    //     // = When there is no schedule
    //     // = * It returns a send_msg with 0 amount and a hook
    //     let handle_msg = ButtcoinDistributorHandleMsg::ClaimButtcoin { hook: None };
    //     let handle_result = handle(
    //         &mut deps,
    //         mock_env(mock_buttcoin().address, &[]),
    //         handle_msg,
    //     );
    //     let handle_result_unwrapped = handle_result.unwrap();
    //     assert_eq!(
    //         handle_result_unwrapped.messages,
    //         vec![snip20::send_msg(
    //             mock_buttcoin().address.clone(),
    //             Uint128(0),
    //             Some(
    //                 to_binary(&YieldOptimizerReceiveMsg::DepositButtcoin { hook: None },).unwrap()
    //             ),
    //             None,
    //             1,
    //             mock_buttcoin().contract_hash,
    //             mock_buttcoin().address,
    //         )
    //         .unwrap(),]
    //     );
    //     assert_eq!(
    //         handle_result_unwrapped.log,
    //         vec![log("claim_buttcoin", mock_buttcoin().address.0)]
    //     );
    //     let handle_result_data: ButtcoinDistributorHandleAnswer =
    //         from_binary(&handle_result_unwrapped.data.unwrap()).unwrap();
    //     assert_eq!(
    //         to_binary(&handle_result_data).unwrap(),
    //         to_binary(&ButtcoinDistributorHandleAnswer::ClaimButtcoin { status: Success }).unwrap()
    //     );

    //     // = When there is a schedule
    //     let new_release_schedule: Schedule = vec![
    //         ScheduleUnit {
    //             end_block: env.block.height + 3000,
    //             release_per_block: Uint128(3000),
    //         },
    //         ScheduleUnit {
    //             end_block: env.block.height + 6000,
    //             release_per_block: Uint128(6000),
    //         },
    //     ];
    //     let handle_msg = ButtcoinDistributorHandleMsg::SetSchedule {
    //         schedule: new_release_schedule.clone(),
    //     };
    //     handle(&mut deps, mock_env(MOCK_SMART_CONTRACT_INITIALIZER, &[]), handle_msg).unwrap();

    //     // == When the address has weight
    //     let handle_msg = ButtcoinDistributorHandleMsg::SetWeights {
    //         weights: vec![WeightInfo {
    //             address: mock_yield_optimizer_smart_contract().address,
    //             hash: mock_yield_optimizer_smart_contract().contract_hash,
    //             weight: 123,
    //         }],
    //     };
    //     handle(&mut deps, mock_env(MOCK_SMART_CONTRACT_INITIALIZER, &[]), handle_msg).unwrap();

    //     // == * It returns a send_msg with with the right amount, to the correct address with the correct hook msg
    //     let mut env = mock_env(mock_yield_optimizer_smart_contract().address, &[]);
    //     env.block.height += 1;
    //     let hook = YieldOptimizerDepositButtcoinHookMsg::ContinueDepositAfterButtcoinClaimed {
    //         depositer: mock_buttcoin().address,
    //         incentivized_token_amount: Uint128(789),
    //     };
    //     let handle_msg = ButtcoinDistributorHandleMsg::ClaimButtcoin {
    //         hook: Some(to_binary(&hook).unwrap()),
    //     };
    //     let handle_result = handle(&mut deps, env, handle_msg);
    //     let handle_result_unwrapped = handle_result.unwrap();
    //     assert_eq!(
    //         handle_result_unwrapped.messages,
    //         vec![snip20::send_msg(
    //             mock_yield_optimizer_smart_contract().address,
    //             Uint128(3000),
    //             Some(
    //                 to_binary(&YieldOptimizerReceiveMsg::DepositButtcoin {
    //                     hook: Some(to_binary(&hook).unwrap())
    //                 },)
    //                 .unwrap()
    //             ),
    //             None,
    //             1,
    //             mock_buttcoin().contract_hash,
    //             mock_buttcoin().address,
    //         )
    //         .unwrap(),]
    //     );
    // }
}
