use crate::msg::ButtcoinDistributorResponseStatus::Success;
use crate::msg::{
    ButtcoinDistributorHandleAnswer, ButtcoinDistributorHandleMsg, ButtcoinDistributorQueryAnswer,
    ButtcoinDistributorQueryMsg, InitMsg, LPStakingHandleMsg,
};
use crate::state::{
    config, config_read, sort_schedule, ReceivableContractSettings, Schedule, SecretContract,
    State, WeightInfo,
};
use cosmwasm_std::{
    log, to_binary, Api, Binary, Env, Extern, HandleResponse, HumanAddr, InitResponse, Querier,
    StdError, StdResult, Storage, Uint128,
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

    let buttcoin = SecretContract {
        address: HumanAddr::from("secret1yxcexylwyxlq58umhgsjgstgcg2a0ytfy4d9lt"),
        contract_hash: "F8B27343FF08290827560A1BA358EECE600C9EA7F403B02684AD87AE7AF0F288"
            .to_string(),
    };
    // We are going to publicly expose the viewing key for this contract because
    // the transfers are only between the admin to this contract and from this contract to
    // sub contracts such as yield optimizers.
    // None of this information will expose user's transactions etc.
    let viewing_key = "api_key_ButtcoinDistributor=".to_string();
    let state = State {
        admin: env.message.sender,
        buttcoin: buttcoin.clone(),
        total_weight: 0,
        release_schedule: release_schedule,
        viewing_key: viewing_key.clone(),
    };

    config(&mut deps.storage).save(&state)?;

    let messages = vec![snip20::set_viewing_key_msg(
        viewing_key,
        None,
        1,
        buttcoin.contract_hash,
        buttcoin.address,
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
        ButtcoinDistributorHandleMsg::ClaimButtcoin {
            receivable_contract_address,
            hook,
        } => claim_buttcoin(deps, env, receivable_contract_address, hook),
        ButtcoinDistributorHandleMsg::SetWeights { weights } => set_weights(deps, env, weights),
        ButtcoinDistributorHandleMsg::SetSchedule { schedule } => set_schedule(deps, env, schedule),
        ButtcoinDistributorHandleMsg::ChangeAdmin { addr } => change_admin(deps, env, addr),
    }
}

fn set_schedule<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    schedule: Schedule,
) -> StdResult<HandleResponse> {
    let mut st = config(&mut deps.storage);
    let mut state = st.load()?;

    enforce_admin(state.clone(), env)?;

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

    enforce_admin(state.clone(), env.clone())?;

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
                Some(to_binary(&LPStakingHandleMsg::ButtcoinClaimedCallback {
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
    receivable_contract_address: HumanAddr,
    hook: Option<Binary>,
) -> StdResult<HandleResponse> {
    let state = config_read(&deps.storage).load()?;

    let mut rs = TypedStoreMut::attach(&mut deps.storage);
    let mut receivable_contract_settings = rs
        .load(receivable_contract_address.0.as_bytes())
        .unwrap_or(ReceivableContractSettings {
            weight: 0,
            last_update_block: env.block.height,
        });

    let mut rewards = 0;
    let mut messages = vec![];
    if receivable_contract_settings.last_update_block < env.block.height
        && receivable_contract_settings.weight > 0
    {
        // Calc amount to minLPStakingHandleMsg for this receivable contract and push to messages
        rewards = get_receivable_contract_rewards(
            env.block.height,
            state.total_weight,
            &state.release_schedule,
            receivable_contract_settings.clone(),
        );

        receivable_contract_settings.last_update_block = env.block.height;
        rs.store(
            receivable_contract_address.0.as_bytes(),
            &receivable_contract_settings,
        )?;
    }

    messages.push(snip20::send_msg(
        receivable_contract_address.clone(),
        Uint128(rewards),
        Some(to_binary(&LPStakingHandleMsg::ButtcoinClaimedCallback {
            hook,
        })?),
        None,
        1,
        state.buttcoin.contract_hash.clone(),
        state.buttcoin.address,
    )?);

    Ok(HandleResponse {
        messages,
        log: vec![log("claim_buttcoin", receivable_contract_address.0)],
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

    enforce_admin(state.clone(), env)?;

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
    current_block: u64,
    total_weight: u64,
    schedule: &Schedule,
    receivable_contract_settings: ReceivableContractSettings,
) -> u128 {
    let mut last_update_block = receivable_contract_settings.last_update_block;

    let mut multiplier = 0;
    // Going serially assuming that schedule is not a big vector
    for u in schedule.to_owned() {
        if last_update_block < u.end_block {
            if current_block > u.end_block {
                multiplier +=
                    (u.end_block - last_update_block) as u128 * u.release_per_block.u128();
                last_update_block = u.end_block;
            } else {
                multiplier +=
                    (current_block - last_update_block) as u128 * u.release_per_block.u128();
                // last_update_block = current_block;
                break; // No need to go further up the schedule
            }
        }
    }

    (multiplier * receivable_contract_settings.weight as u128) / total_weight as u128
}

fn enforce_admin(config: State, env: Env) -> StdResult<()> {
    if config.admin != env.message.sender {
        return Err(StdError::Unauthorized { backtrace: None });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ScheduleUnit;
    use cosmwasm_std::from_binary;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, MockApi, MockQuerier, MockStorage};

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
        let msg = InitMsg { release_schedule };
        (init(&mut deps, env.clone(), msg), deps)
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
                assert_eq!(
                    buttcoin,
                    SecretContract {
                        address: HumanAddr::from("secret1yxcexylwyxlq58umhgsjgstgcg2a0ytfy4d9lt"),
                        contract_hash:
                            "F8B27343FF08290827560A1BA358EECE600C9EA7F403B02684AD87AE7AF0F288"
                                .to_string()
                    }
                );
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
                assert_eq!(viewing_key, "api_key_ButtcoinDistributor=".to_string());
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
                address: HumanAddr::from("sefistakingoptimizeraddress"),
                hash: "sefistakingoptimizerhash".to_string(),
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
                    addr: HumanAddr::from("sefistakingoptimizeraddress"),
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
