use crate::msg::{HandleAnswer, HandleMsg, InitMsg, LPStakingHandleMsg, QueryAnswer, QueryMsg};
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
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::UpdateAllocation {
            receivable_contract_address,
            hook,
        } => update_allocation(deps, env, receivable_contract_address, hook),
        HandleMsg::SetWeights { weights } => set_weights(deps, env, weights),
        HandleMsg::SetSchedule { schedule } => set_schedule(deps, env, schedule),
        HandleMsg::ChangeAdmin { addr } => change_admin(deps, env, addr),
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
        data: Some(to_binary(&HandleAnswer::Success)?),
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
                Some(to_binary(&LPStakingHandleMsg::NotifyAllocation {
                    amount: Uint128(rewards),
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
        data: Some(to_binary(&HandleAnswer::Success)?),
    })
}

fn update_allocation<S: Storage, A: Api, Q: Querier>(
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
        Some(to_binary(&LPStakingHandleMsg::NotifyAllocation {
            amount: Uint128(rewards),
            hook,
        })?),
        None,
        1,
        state.buttcoin.contract_hash.clone(),
        state.buttcoin.address,
    )?);

    Ok(HandleResponse {
        messages,
        log: vec![log("update_allocation", receivable_contract_address.0)],
        data: Some(to_binary(&HandleAnswer::Success)?),
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
        data: Some(to_binary(&HandleAnswer::Success)?),
    })
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_public_config(deps)?),
        QueryMsg::ReceivableContractWeight { addr } => {
            to_binary(&query_receivable_contract_weight(deps, addr)?)
        }
        QueryMsg::Pending {
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
) -> StdResult<QueryAnswer> {
    let state: State = config_read(&deps.storage).load()?;

    Ok(QueryAnswer::Config {
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
) -> StdResult<QueryAnswer> {
    let receivable_contract = TypedStore::attach(&deps.storage)
        .load(receivable_contract_address.0.as_bytes())
        .unwrap_or(ReceivableContractSettings {
            weight: 0,
            last_update_block: 0,
        });

    Ok(QueryAnswer::ReceivableContractWeight {
        weight: receivable_contract.weight,
    })
}

fn query_pending_rewards<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    receivable_contract_addr: HumanAddr,
    block: u64,
) -> StdResult<QueryAnswer> {
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

    Ok(QueryAnswer::Pending {
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
        return Err(StdError::generic_err(format!(
            "not an admin: {}",
            env.message.sender
        )));
    }

    Ok(())
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use cosmwasm_std::testing::{mock_dependencies, mock_env};
//     use cosmwasm_std::{coins, from_binary, StdError};
// }
