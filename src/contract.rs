use crate::authorize::authorize;
use crate::msg::ButtcoinDistributorResponseStatus::Success;
use crate::msg::{
    ButtcoinDistributorHandleAnswer, ButtcoinDistributorHandleMsg, ButtcoinDistributorQueryAnswer,
    ButtcoinDistributorQueryMsg, InitMsg, YieldOptimizerReceiveMsg,
};
use crate::state::{
    config, config_read, sort_schedule, ReceivableContractSettings, Schedule, State, WeightInfo,
};
use cosmwasm_std::{
    log, to_binary, Api, Binary, Env, Extern, HandleResponse, HumanAddr, InitResponse, Querier,
    StdResult, Storage, Uint128,
};
use secret_toolkit::snip20;
use secret_toolkit::storage::{TypedStore, TypedStoreMut};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let mut release_schedule = msg.release_schedule;
    sort_schedule(&mut release_schedule);

    let state = State {
        admin: env.message.sender,
        buttcoin: msg.buttcoin.clone(),
        total_weight: 0,
        release_schedule: release_schedule,
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
        ButtcoinDistributorHandleMsg::ClaimButtcoin { hook } => claim_buttcoin(deps, env, hook),
        ButtcoinDistributorHandleMsg::SetWeights { weights } => set_weights(deps, env, weights),
        ButtcoinDistributorHandleMsg::SetSchedule { schedule } => set_schedule(deps, env, schedule),
        ButtcoinDistributorHandleMsg::ChangeAdmin { addr } => change_admin(deps, env, addr),
    }
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: ButtcoinDistributorQueryMsg,
) -> StdResult<Binary> {
    match msg {
        ButtcoinDistributorQueryMsg::Config {} => to_binary(&query_public_config(deps)?),
        ButtcoinDistributorQueryMsg::ReceivableContractWeight { addr } => {
            to_binary(&query_receivable_contract_weight(deps, addr)?)
        }
        ButtcoinDistributorQueryMsg::Pending {
            receivable_contract_address,
            block,
        } => to_binary(&query_pending_rewards(
            deps,
            receivable_contract_address,
            block,
        )?),
    }
}

fn query_public_config<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<ButtcoinDistributorQueryAnswer> {
    let state: State = config_read(&deps.storage).load()?;

    Ok(ButtcoinDistributorQueryAnswer::Config {
        admin: state.admin,
        buttcoin: state.buttcoin,
        schedule: state.release_schedule,
        total_weight: state.total_weight,
        viewing_key: state.viewing_key,
    })
}

fn query_receivable_contract_weight<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    receivable_contract_address: HumanAddr,
) -> StdResult<ButtcoinDistributorQueryAnswer> {
    let receivable_contract = TypedStore::attach(&deps.storage)
        .load(receivable_contract_address.0.as_bytes())
        .unwrap_or(ReceivableContractSettings {
            weight: 0,
            last_update_block: 0,
        });

    Ok(ButtcoinDistributorQueryAnswer::ReceivableContractWeight {
        weight: receivable_contract.weight,
    })
}

fn query_pending_rewards<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    receivable_contract_addr: HumanAddr,
    block: u64,
) -> StdResult<ButtcoinDistributorQueryAnswer> {
    let state = config_read(&deps.storage).load()?;
    let receivable_contract = TypedStore::attach(&deps.storage)
        .load(receivable_contract_addr.0.as_bytes())
        .unwrap_or(ReceivableContractSettings {
            weight: 0,
            last_update_block: block,
        });

    let amount = get_receivable_contract_rewards(
        block,
        state.total_weight,
        &state.release_schedule,
        receivable_contract,
    );

    Ok(ButtcoinDistributorQueryAnswer::Pending {
        amount: Uint128(amount),
    })
}

fn get_receivable_contract_rewards(
    block: u64,
    total_weight: u64,
    schedule: &Schedule,
    receivable_contract_settings: ReceivableContractSettings,
) -> u128 {
    if total_weight > 0 {
        let mut last_update_block = receivable_contract_settings.last_update_block;
        if block > last_update_block {
            let mut multiplier = 0;
            // Going serially assuming that schedule is not a big vector
            for u in schedule.to_owned() {
                if last_update_block < u.end_block {
                    if block > u.end_block {
                        multiplier +=
                            (u.end_block - last_update_block) as u128 * u.release_per_block.u128();
                        last_update_block = u.end_block;
                    } else {
                        multiplier +=
                            (block - last_update_block) as u128 * u.release_per_block.u128();
                        // last_update_block = current_block;
                        break; // No need to go further up the schedule
                    }
                }
            }

            (multiplier * receivable_contract_settings.weight as u128) / total_weight as u128
        } else {
            0
        }
    } else {
        0
    }
}

fn set_schedule<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    schedule: Schedule,
) -> StdResult<HandleResponse> {
    let mut st = config(&mut deps.storage);
    let mut state = st.load()?;
    authorize(state.admin.clone(), env.message.sender)?;

    let mut s = schedule;
    sort_schedule(&mut s);
    state.release_schedule = s;
    st.save(&state)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&ButtcoinDistributorHandleAnswer::SetSchedule {
            status: Success,
        })?),
    })
}

fn set_weights<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    weights: Vec<WeightInfo>,
) -> StdResult<HandleResponse> {
    let mut state = config_read(&deps.storage).load()?;
    authorize(state.admin.clone(), env.message.sender)?;

    let mut messages = vec![];
    let mut logs = vec![];
    let mut new_weight_counter = 0;
    let mut old_weight_counter = 0;

    // Update reward contracts one by one
    for to_update in weights {
        let mut rs = TypedStoreMut::attach(&mut deps.storage);
        let mut receivable_contract_settings = rs
            .load(to_update.address.clone().0.as_bytes())
            .unwrap_or(ReceivableContractSettings {
                weight: 0,
                last_update_block: env.block.height,
            });

        // There is no need to update a receivable_contract twice in a block, and there is no need to update a receivable_contract
        // that had 0 weight until now
        if receivable_contract_settings.last_update_block < env.block.height
            && receivable_contract_settings.weight > 0
        {
            // Calc amount to send to receivable contract
            let rewards = get_receivable_contract_rewards(
                env.block.height,
                state.total_weight,
                &state.release_schedule,
                receivable_contract_settings.clone(),
            );
            messages.push(snip20::send_msg(
                to_update.address.clone(),
                Uint128(rewards),
                Some(to_binary(&YieldOptimizerReceiveMsg::DepositButtcoin {
                    hook: None,
                })?),
                None,
                1,
                state.buttcoin.contract_hash.clone(),
                state.buttcoin.address.clone(),
            )?);
        }

        let old_weight = receivable_contract_settings.weight;
        let new_weight = to_update.weight;

        // Set new weight and update total counter
        receivable_contract_settings.weight = new_weight;
        receivable_contract_settings.last_update_block = env.block.height;
        rs.store(
            to_update.address.0.as_bytes(),
            &receivable_contract_settings,
        )?;

        // Update counters to batch update after the loop
        new_weight_counter += new_weight;
        old_weight_counter += old_weight;

        logs.push(log("weight_update", to_update.address.0))
    }

    state.total_weight = state.total_weight - old_weight_counter + new_weight_counter;
    config(&mut deps.storage).save(&state)?;

    Ok(HandleResponse {
        messages,
        log: logs,
        data: Some(to_binary(&ButtcoinDistributorHandleAnswer::SetWeights {
            status: Success,
        })?),
    })
}

fn claim_buttcoin<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    hook: Option<Binary>,
) -> StdResult<HandleResponse> {
    let state = config_read(&deps.storage).load()?;

    let mut rs = TypedStoreMut::attach(&mut deps.storage);
    let mut receivable_contract_settings =
        rs.load(env.message.sender.0.as_bytes())
            .unwrap_or(ReceivableContractSettings {
                weight: 0,
                last_update_block: env.block.height,
            });

    let mut rewards = 0;
    let mut messages = vec![];
    if receivable_contract_settings.last_update_block < env.block.height
        && receivable_contract_settings.weight > 0
    {
        // Calc amount to minYieldOptimizerHandleMsg for this receivable contract and push to messages
        rewards = get_receivable_contract_rewards(
            env.block.height,
            state.total_weight,
            &state.release_schedule,
            receivable_contract_settings.clone(),
        );

        receivable_contract_settings.last_update_block = env.block.height;
        rs.store(
            env.message.sender.0.as_bytes(),
            &receivable_contract_settings,
        )?;
    }

    messages.push(snip20::send_msg(
        env.message.sender.clone(),
        Uint128(rewards),
        Some(to_binary(&YieldOptimizerReceiveMsg::DepositButtcoin {
            hook: hook,
        })?),
        None,
        1,
        state.buttcoin.contract_hash.clone(),
        state.buttcoin.address,
    )?);

    Ok(HandleResponse {
        messages,
        log: vec![log("claim_buttcoin", env.message.sender.0)],
        data: Some(to_binary(
            &ButtcoinDistributorHandleAnswer::ClaimButtcoin { status: Success },
        )?),
    })
}

fn change_admin<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    admin_addr: HumanAddr,
) -> StdResult<HandleResponse> {
    let mut state = config_read(&deps.storage).load()?;
    authorize(state.admin, env.message.sender)?;

    state.admin = admin_addr;
    config(&mut deps.storage).save(&state)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&ButtcoinDistributorHandleAnswer::ChangeAdmin {
            status: Success,
        })?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg::YieldOptimizerDepositButtcoinHookMsg;
    use crate::state::{ScheduleUnit, SecretContract};
    use cosmwasm_std::testing::{mock_dependencies, mock_env, MockApi, MockQuerier, MockStorage};
    use cosmwasm_std::{from_binary, StdError};

    // === CONSTANTS ===
    pub const MOCK_ADMIN: &str = "admin";

    // === HELPERS ===
    fn init_helper() -> (
        StdResult<InitResponse>,
        Extern<MockStorage, MockApi, MockQuerier>,
    ) {
        let env = mock_env(MOCK_ADMIN, &[]);
        let release_schedule: Schedule = vec![
            ScheduleUnit {
                end_block: env.block.height + 1000,
                release_per_block: Uint128(4000),
            },
            ScheduleUnit {
                end_block: env.block.height + 2000,
                release_per_block: Uint128(3000),
            },
        ];
        let mut deps = mock_dependencies(20, &[]);
        let msg = InitMsg {
            buttcoin: mock_buttcoin(),
            release_schedule,
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
    fn test_query_public_config() {
        let (_init_result, deps) = init_helper();
        let env = mock_env(MOCK_ADMIN, &[]);
        let res =
            from_binary(&query(&deps, ButtcoinDistributorQueryMsg::Config {}).unwrap()).unwrap();
        match res {
            ButtcoinDistributorQueryAnswer::Config {
                admin,
                buttcoin,
                schedule,
                total_weight,
                viewing_key,
            } => {
                assert_eq!(admin, HumanAddr(MOCK_ADMIN.to_string()));
                assert_eq!(buttcoin, mock_buttcoin());
                assert_eq!(
                    schedule,
                    vec![
                        ScheduleUnit {
                            end_block: env.block.height + 1000,
                            release_per_block: Uint128(4000),
                        },
                        ScheduleUnit {
                            end_block: env.block.height + 2000,
                            release_per_block: Uint128(3000),
                        },
                    ]
                );
                assert_eq!(total_weight, 0);
                assert_eq!(viewing_key, mock_viewing_key());
            }
            _ => panic!("unexpected error"),
        }
    }

    #[test]
    fn test_query_receivable_contract_weight() {
        let (_init_result, deps) = init_helper();
        let _env = mock_env(MOCK_ADMIN, &[]);

        // = When provided address has no weight
        // = * It returns a weight of zero
        let res = from_binary(
            &query(
                &deps,
                ButtcoinDistributorQueryMsg::ReceivableContractWeight {
                    addr: HumanAddr::from(MOCK_ADMIN),
                },
            )
            .unwrap(),
        )
        .unwrap();
        match res {
            ButtcoinDistributorQueryAnswer::ReceivableContractWeight { weight } => {
                assert_eq!(weight, 0);
            }
            _ => panic!("unexpected error"),
        }

        // = When provided address has weight
        // = * It returns the weight for that address
        // This is tested in #test_set_weights
    }

    #[test]
    fn test_query_pending_rewards() {
        let (_init_result, mut deps) = init_helper();
        let env = mock_env(MOCK_ADMIN, &[]);

        // = When contract has no weight
        // = * It returns 0
        let res = from_binary(
            &query(
                &deps,
                ButtcoinDistributorQueryMsg::Pending {
                    block: 1,
                    receivable_contract_address: HumanAddr::from(MOCK_ADMIN),
                },
            )
            .unwrap(),
        )
        .unwrap();
        match res {
            ButtcoinDistributorQueryAnswer::Pending { amount } => {
                assert_eq!(amount, Uint128(0));
            }
            _ => panic!("unexpected error"),
        }

        // = When contract has weight
        let handle_msg = ButtcoinDistributorHandleMsg::SetWeights {
            weights: vec![WeightInfo {
                address: mock_yield_optimizer_smart_contract().address,
                hash: mock_yield_optimizer_smart_contract().contract_hash,
                weight: 123,
            }],
        };
        handle(&mut deps, mock_env(MOCK_ADMIN, &[]), handle_msg).unwrap();

        // == When there is no schedule
        // == * It returns 0
        let res = from_binary(
            &query(
                &deps,
                ButtcoinDistributorQueryMsg::Pending {
                    block: 1,
                    receivable_contract_address: HumanAddr::from(MOCK_ADMIN),
                },
            )
            .unwrap(),
        )
        .unwrap();
        match res {
            ButtcoinDistributorQueryAnswer::Pending { amount } => {
                assert_eq!(amount, Uint128(0));
            }
            _ => panic!("unexpected error"),
        }

        // === When there is a schedule
        let new_release_schedule: Schedule = vec![
            ScheduleUnit {
                end_block: env.block.height + 3000,
                release_per_block: Uint128(3000),
            },
            ScheduleUnit {
                end_block: env.block.height + 6000,
                release_per_block: Uint128(6000),
            },
        ];

        let handle_msg = ButtcoinDistributorHandleMsg::SetSchedule {
            schedule: new_release_schedule.clone(),
        };
        handle(&mut deps, mock_env(MOCK_ADMIN, &[]), handle_msg).unwrap();
        // ==== * When block specified is before weight was added
        // ==== * It returns 0
        let res = from_binary(
            &query(
                &deps,
                ButtcoinDistributorQueryMsg::Pending {
                    block: 1,
                    receivable_contract_address: mock_yield_optimizer_smart_contract().address,
                },
            )
            .unwrap(),
        )
        .unwrap();
        match res {
            ButtcoinDistributorQueryAnswer::Pending { amount } => {
                assert_eq!(amount, Uint128(0));
            }
            _ => panic!("unexpected error"),
        }
        // ==== * When block specified is one after when weight was added
        // ==== * It returns the correct amount
        let res = from_binary(
            &query(
                &deps,
                ButtcoinDistributorQueryMsg::Pending {
                    block: env.block.height + 1,
                    receivable_contract_address: mock_yield_optimizer_smart_contract().address,
                },
            )
            .unwrap(),
        )
        .unwrap();
        match res {
            ButtcoinDistributorQueryAnswer::Pending { amount } => {
                assert_eq!(amount, Uint128(3_000));
            }
            _ => panic!("unexpected error"),
        }
    }

    // === HANDLE ===

    #[test]
    fn test_handle_change_admin() {
        let (init_result, mut deps) = init_helper();

        assert!(
            init_result.is_ok(),
            "Init failed: {}",
            init_result.err().unwrap()
        );

        let handle_msg = ButtcoinDistributorHandleMsg::ChangeAdmin {
            addr: HumanAddr("bob".to_string()),
        };
        let handle_result = handle(&mut deps, mock_env(MOCK_ADMIN, &[]), handle_msg);
        assert!(
            handle_result.is_ok(),
            "handle() failed: {}",
            handle_result.err().unwrap()
        );

        let res =
            from_binary(&query(&deps, ButtcoinDistributorQueryMsg::Config {}).unwrap()).unwrap();
        match res {
            ButtcoinDistributorQueryAnswer::Config { admin, .. } => {
                assert_eq!(admin, HumanAddr("bob".to_string()))
            }
            _ => panic!("unexpected error"),
        }
    }

    #[test]
    fn test_handle_claim_buttcoin() {
        let (_init_result, mut deps) = init_helper();
        let env = mock_env("admin", &[]);

        // = When there is no schedule
        // = * It returns a send_msg with 0 amount and a hook
        let handle_msg = ButtcoinDistributorHandleMsg::ClaimButtcoin { hook: None };
        let handle_result = handle(
            &mut deps,
            mock_env(mock_buttcoin().address, &[]),
            handle_msg,
        );
        let handle_result_unwrapped = handle_result.unwrap();
        assert_eq!(
            handle_result_unwrapped.messages,
            vec![snip20::send_msg(
                mock_buttcoin().address.clone(),
                Uint128(0),
                Some(
                    to_binary(&YieldOptimizerReceiveMsg::DepositButtcoin { hook: None },).unwrap()
                ),
                None,
                1,
                mock_buttcoin().contract_hash,
                mock_buttcoin().address,
            )
            .unwrap(),]
        );
        assert_eq!(
            handle_result_unwrapped.log,
            vec![log("claim_buttcoin", mock_buttcoin().address.0)]
        );
        let handle_result_data: ButtcoinDistributorHandleAnswer =
            from_binary(&handle_result_unwrapped.data.unwrap()).unwrap();
        assert_eq!(
            to_binary(&handle_result_data).unwrap(),
            to_binary(&ButtcoinDistributorHandleAnswer::ClaimButtcoin { status: Success }).unwrap()
        );

        // = When there is a schedule
        let new_release_schedule: Schedule = vec![
            ScheduleUnit {
                end_block: env.block.height + 3000,
                release_per_block: Uint128(3000),
            },
            ScheduleUnit {
                end_block: env.block.height + 6000,
                release_per_block: Uint128(6000),
            },
        ];
        let handle_msg = ButtcoinDistributorHandleMsg::SetSchedule {
            schedule: new_release_schedule.clone(),
        };
        handle(&mut deps, mock_env(MOCK_ADMIN, &[]), handle_msg).unwrap();

        // == When the address has weight
        let handle_msg = ButtcoinDistributorHandleMsg::SetWeights {
            weights: vec![WeightInfo {
                address: mock_yield_optimizer_smart_contract().address,
                hash: mock_yield_optimizer_smart_contract().contract_hash,
                weight: 123,
            }],
        };
        handle(&mut deps, mock_env(MOCK_ADMIN, &[]), handle_msg).unwrap();

        // == * It returns a send_msg with with the right amount, to the correct address with the correct hook msg
        let mut env = mock_env(mock_yield_optimizer_smart_contract().address, &[]);
        env.block.height += 1;
        let hook = YieldOptimizerDepositButtcoinHookMsg::ContinueDepositAfterButtcoinClaimed {
            depositer: mock_buttcoin().address,
            incentivized_token_amount: Uint128(789),
        };
        let handle_msg = ButtcoinDistributorHandleMsg::ClaimButtcoin {
            hook: Some(to_binary(&hook).unwrap()),
        };
        let handle_result = handle(&mut deps, env, handle_msg);
        let handle_result_unwrapped = handle_result.unwrap();
        assert_eq!(
            handle_result_unwrapped.messages,
            vec![snip20::send_msg(
                mock_yield_optimizer_smart_contract().address,
                Uint128(3000),
                Some(
                    to_binary(&YieldOptimizerReceiveMsg::DepositButtcoin {
                        hook: Some(to_binary(&hook).unwrap())
                    },)
                    .unwrap()
                ),
                None,
                1,
                mock_buttcoin().contract_hash,
                mock_buttcoin().address,
            )
            .unwrap(),]
        );
    }

    #[test]
    fn test_set_schedule() {
        let (init_result, mut deps) = init_helper();
        let env = mock_env("non-admin", &[]);

        assert!(
            init_result.is_ok(),
            "Init failed: {}",
            init_result.err().unwrap()
        );

        let new_release_schedule: Schedule = vec![
            ScheduleUnit {
                end_block: env.block.height + 3000,
                release_per_block: Uint128(3000),
            },
            ScheduleUnit {
                end_block: env.block.height + 6000,
                release_per_block: Uint128(6000),
            },
        ];

        let handle_msg = ButtcoinDistributorHandleMsg::SetSchedule {
            schedule: new_release_schedule.clone(),
        };

        // When function is called by a non-admin
        let handle_result = handle(&mut deps, mock_env("non-admin", &[]), handle_msg.clone());
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::Unauthorized { backtrace: None }
        );

        // When function is called by an admin
        let handle_result = handle(&mut deps, mock_env(MOCK_ADMIN, &[]), handle_msg);
        handle_result.unwrap();

        let res =
            from_binary(&query(&deps, ButtcoinDistributorQueryMsg::Config {}).unwrap()).unwrap();
        match res {
            ButtcoinDistributorQueryAnswer::Config { schedule, .. } => {
                assert_eq!(schedule, new_release_schedule);
            }
            _ => panic!("unexpected error"),
        }
    }

    #[test]
    fn test_set_weights() {
        let (init_result, mut deps) = init_helper();

        assert!(
            init_result.is_ok(),
            "Init failed: {}",
            init_result.err().unwrap()
        );

        let handle_msg = ButtcoinDistributorHandleMsg::SetWeights {
            weights: vec![WeightInfo {
                address: mock_yield_optimizer_smart_contract().address,
                hash: mock_yield_optimizer_smart_contract().contract_hash,
                weight: 123,
            }],
        };

        // When function is called by a non-admin
        let handle_result = handle(&mut deps, mock_env("non-admin", &[]), handle_msg.clone());
        assert_eq!(
            handle_result.unwrap_err(),
            StdError::Unauthorized { backtrace: None }
        );

        // When function is called by an admin
        let handle_result = handle(&mut deps, mock_env(MOCK_ADMIN, &[]), handle_msg);
        handle_result.unwrap();
        let res = from_binary(
            &query(
                &deps,
                ButtcoinDistributorQueryMsg::ReceivableContractWeight {
                    addr: mock_yield_optimizer_smart_contract().address,
                },
            )
            .unwrap(),
        )
        .unwrap();
        match res {
            ButtcoinDistributorQueryAnswer::ReceivableContractWeight { weight } => {
                assert_eq!(weight, 123)
            }
            _ => panic!("unexpected error"),
        }
    }
}
