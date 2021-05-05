// APPLY performance_fee of 3% (300/10_000) when calculating fees etc
// Do we need governanceRecoverUnsupported in Secret Network?
// Can you send unsupported coins in Secret Network or does the regeister stuff in init prevent that?
use crate::msg::{
    StakingHandleAnswer, StakingHandleMsg, StakingInitMsg, StakingQueryAnswer, StakingQueryMsg,
    StakingReceiveMsg, StakingResponseStatus::Success,
};
use cosmwasm_std::{
    from_binary, to_binary, Api, Binary, CosmosMsg, Env, Extern, HandleResponse, HumanAddr,
    InitResponse, Querier, QueryRequest, StdError, StdResult, Storage, Uint128, WasmMsg, WasmQuery,
};

use secret_toolkit::snip20;
use secret_toolkit::storage::{TypedStore, TypedStoreMut};
use secret_toolkit::utils::{pad_handle_result, pad_query_result};

use crate::constants::*;
use crate::state::Config;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: StakingInitMsg,
) -> StdResult<InitResponse> {
    let mut config_store = TypedStoreMut::attach(&mut deps.storage);
    config_store.store(
        CONFIG_KEY,
        &Config {
            farm_contract: msg.farm_contract,
            token: msg.inc_token.clone(),
            shares_token: msg.shares_token.clone(),
            admin: env.message.sender.clone(),
            viewing_key: msg.viewing_key.clone(),
            stopped: false,
        },
    )?;

    // https://github.com/enigmampc/secret-toolkit/tree/master/packages/snip20
    // Register this contract to be able to receive the incentivized token
    // Enable this contract to see it's incentivized token details via viewing key
    let messages = vec![
        snip20::register_receive_msg(
            env.contract_code_hash,
            None,
            1,
            msg.inc_token.contract_hash.clone(),
            msg.inc_token.address.clone(),
        )?,
        snip20::set_viewing_key_msg(
            msg.viewing_key,
            None,
            RESPONSE_BLOCK_SIZE,
            msg.inc_token.contract_hash,
            msg.inc_token.address,
        )?,
    ];

    Ok(InitResponse {
        messages,
        log: vec![],
    })
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: StakingQueryMsg,
) -> StdResult<Binary> {
    let response = match msg {
        StakingQueryMsg::ContractStatus {} => query_contract_status(deps),
        StakingQueryMsg::Token {} => query_token(deps),
        _ => Err(StdError::generic_err("Unavailable or unknown action")),
    };

    pad_query_result(response, RESPONSE_BLOCK_SIZE)
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: StakingHandleMsg,
) -> StdResult<HandleResponse> {
    let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY)?;
    if config.stopped {
        return match msg {
            StakingHandleMsg::ResumeContract {} => resume_contract(deps, env),
            _ => Err(StdError::generic_err(
                "this contract is stopped and this action is not allowed",
            )),
        };
    }

    let response = match msg {
        StakingHandleMsg::Receive {
            from, amount, msg, ..
        } => receive(deps, env, from, amount.u128(), msg),
        StakingHandleMsg::Redeem { amount } => withdraw(deps, env, amount),
        StakingHandleMsg::StopContract {} => stop_contract(deps, env),
        _ => Err(StdError::generic_err("Unavailable or unknown action")),
    };

    pad_handle_result(response, RESPONSE_BLOCK_SIZE)
}

// This is called from the snip-20 SEFI contract
// It's more like a after receive callback
fn deposit<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    _from: HumanAddr,
    amount: u128,
) -> StdResult<HandleResponse> {
    // Ensure that the sent tokens are from an expected contract address
    let config_store = TypedStoreMut::attach(&mut deps.storage);
    let config: Config = config_store.load(CONFIG_KEY)?;
    if env.message.sender != config.token.address {
        return Err(StdError::generic_err(format!(
            "This token is not supported. Supported: {}, given: {}",
            config.token.address, env.message.sender
        )));
    }
    let total_shares: u128 = total_supply_of_shares_token(&deps.querier, config.clone())
        .unwrap()
        .u128();
    let _shares_for_this_deposit: u128 = if total_shares == 0 {
        amount
    } else {
        let balance_of_pool_before_deposit =
            balance_of_pool(&deps.querier, env.clone(), config.clone()).unwrap() - amount;
        // 4. Shares for this deposit
        amount * total_shares / balance_of_pool_before_deposit
    };
    deposit_into_farm_contract(deps, env.clone())?;
    let mut messages: Vec<CosmosMsg> = vec![];
    messages.push(secret_toolkit::snip20::mint_msg(
        _from,
        Uint128(_shares_for_this_deposit),
        None,
        RESPONSE_BLOCK_SIZE,
        config.shares_token.contract_hash,
        config.shares_token.address,
    )?);

    Ok(HandleResponse {
        messages,
        log: vec![],
        data: None,
    })
}

fn deposit_into_farm_contract<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let config = TypedStore::<Config, S>::attach(&deps.storage).load(CONFIG_KEY)?;
    let mut messages: Vec<CosmosMsg> = vec![];
    let balance_of_this_contract: u128 =
        balance_of_this_contract(&deps.querier, env.clone(), config.clone()).unwrap();
    messages.push(secret_toolkit::snip20::send_msg(
        config.farm_contract.address.clone(),
        Uint128(balance_of_this_contract),
        Some(to_binary(&StakingHandleMsg::Receive {
            sender: env.contract.address.clone(),
            from: env.contract.address.clone(),
            amount: Uint128(balance_of_this_contract),
            msg: to_binary(&StakingReceiveMsg::Deposit {})?,
        })?),
        None,
        RESPONSE_BLOCK_SIZE,
        config.token.contract_hash.clone(),
        config.token.address.clone(),
    )?);

    // At this point the reward will be in the account and the performance fee will be sent to admin
    let commission: u128 =
        unclaimed_rewards(&deps.querier, env.clone(), config.clone()).unwrap() * 500 / 10_000;
    messages.push(secret_toolkit::snip20::transfer_msg(
        config.admin,
        Uint128(commission),
        None,
        RESPONSE_BLOCK_SIZE,
        config.token.contract_hash,
        config.token.address,
    )?);

    Ok(HandleResponse {
        messages,
        log: vec![],
        data: None,
    })
}

fn enforce_admin(config: Config, env: Env) -> StdResult<()> {
    if config.admin != env.message.sender {
        return Err(StdError::generic_err(format!(
            "not an admin: {}",
            env.message.sender
        )));
    }

    Ok(())
}

fn query_contract_status<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<Binary> {
    let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;

    to_binary(&StakingQueryAnswer::ContractStatus {
        stopped: config.stopped,
    })
}

fn query_token<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<Binary> {
    let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;

    to_binary(&StakingQueryAnswer::Token {
        token: config.token,
    })
}

fn receive<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    from: HumanAddr,
    amount: u128,
    msg: Binary,
) -> StdResult<HandleResponse> {
    let msg: StakingReceiveMsg = from_binary(&msg)?;

    match msg {
        StakingReceiveMsg::Deposit {} => deposit(deps, env, from, amount),
    }
}

//
fn resume_contract<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let mut config_store = TypedStoreMut::attach(&mut deps.storage);
    let mut config: Config = config_store.load(CONFIG_KEY)?;

    enforce_admin(config.clone(), env)?;

    config.stopped = false;
    config_store.store(CONFIG_KEY, &config)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&StakingHandleAnswer::ResumeContract {
            status: Success,
        })?),
    })
}

fn stop_contract<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let mut config_store = TypedStoreMut::attach(&mut deps.storage);
    let mut config: Config = config_store.load(CONFIG_KEY)?;

    enforce_admin(config.clone(), env)?;

    config.stopped = true;
    config_store.store(CONFIG_KEY, &config)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&StakingHandleAnswer::StopContract {
            status: Success,
        })?),
    })
}

//As such, we provide the Querier with read-only access to the state snapshot right before execution of the current CosmWasm message. Since we take a snapshot and both the executing contract and the queried contract have read-only access to the data before the contract execution, this is still safe with Rust's borrowing rules (as a placeholder for secure design). The current contract only writes to a cache, which is flushed afterwards on success.
fn balance_of_pool<Q: Querier>(querier: &Q, env: Env, config: Config) -> StdResult<u128> {
    // 1. Get unclaimed rewards in third party contract
    let unclaimed_rewards: u128 = querier
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: config.farm_contract.address.clone(),
            callback_code_hash: config.farm_contract.contract_hash.clone(),
            msg: to_binary(&StakingQueryMsg::Rewards {
                address: env.contract.address.clone(),
                key: config.viewing_key.clone(),
                height: env.block.height,
            })?,
        }))
        .map_err(|err| StdError::generic_err(format!("Got an error from query: {:?}", err)))?;

    // 2. Get total locked in third party
    let total_locked_in_farm_contract: u128 =
        total_locked_in_farm_contract(querier, env.clone(), config.clone()).unwrap();
    // 3. Get balance of this contract - the new amount?
    // DO I need the response_block_size_here? I don't really care who sees the balance etc
    // I want people to see everything so that they can check everything is right
    let balance_of_this_contract: u128 =
        balance_of_this_contract(querier, env.clone(), config.clone()).unwrap();
    Ok(unclaimed_rewards + total_locked_in_farm_contract + balance_of_this_contract)
}

fn unclaimed_rewards<Q: Querier>(querier: &Q, env: Env, config: Config) -> StdResult<u128> {
    querier
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: config.farm_contract.address.clone(),
            callback_code_hash: config.farm_contract.contract_hash.clone(),
            msg: to_binary(&StakingQueryMsg::Rewards {
                address: env.contract.address.clone(),
                key: config.viewing_key.clone(),
                height: env.block.height,
            })?,
        }))
        .map_err(|err| StdError::generic_err(format!("Got an error from query: {:?}", err)))?
}

fn balance_of_this_contract<Q: Querier>(querier: &Q, env: Env, config: Config) -> StdResult<u128> {
    Ok((snip20::balance_query(
        querier,
        env.contract.address.clone(),
        config.viewing_key,
        RESPONSE_BLOCK_SIZE,
        env.contract_code_hash.clone(),
        env.contract.address.clone(),
    )?)
    .amount
    .u128())
}

fn total_locked_in_farm_contract<Q: Querier>(
    querier: &Q,
    env: Env,
    config: Config,
) -> StdResult<u128> {
    let amount: u128 = querier
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: config.farm_contract.address,
            callback_code_hash: config.farm_contract.contract_hash,
            msg: to_binary(&StakingQueryMsg::Balance {
                address: env.contract.address.clone(),
                key: config.viewing_key.clone(),
            })?,
        }))
        .map_err(|err| StdError::generic_err(format!("Got an error from query: {:?}", err)))?;

    Ok(amount)
}

fn total_supply_of_shares_token<Q: Querier>(querier: &Q, config: Config) -> StdResult<Uint128> {
    let amount = (secret_toolkit::snip20::token_info_query(
        querier,
        RESPONSE_BLOCK_SIZE,
        config.shares_token.contract_hash,
        config.shares_token.address,
    )?)
    .total_supply
    .unwrap();

    Ok(amount)
}

fn withdraw<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    amount_of_shares: Uint128,
) -> StdResult<HandleResponse> {
    let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY)?;
    // 1. Burn the tokens from the user
    let mut messages: Vec<CosmosMsg> = vec![snip20::burn_from_msg(
        env.message.sender.clone(),
        amount_of_shares,
        None,
        RESPONSE_BLOCK_SIZE,
        config.shares_token.contract_hash.clone(),
        config.shares_token.address.clone(),
    )?];
    let total_shares: u128 = total_supply_of_shares_token(&deps.querier, config.clone())
        .unwrap()
        .u128();
    // 2. At this point we need to figure out how much of the token to withdraw from farm contract
    let amount_of_token: u128 = balance_of_pool(&deps.querier, env.clone(), config.clone())
        .unwrap()
        * amount_of_shares.u128()
        / total_shares;
    let total_locked_in_farm_contract: u128 =
        total_locked_in_farm_contract(&deps.querier, env.clone(), config.clone()).unwrap();
    let amount_to_withdraw_from_farm_contract: u128 =
        if amount_of_token > total_locked_in_farm_contract {
            total_locked_in_farm_contract
        } else {
            amount_of_token
        };
    messages.push(
        WasmMsg::Execute {
            contract_addr: config.farm_contract.address.clone(),
            callback_code_hash: config.farm_contract.contract_hash.clone(),
            msg: to_binary(&StakingHandleMsg::Redeem {
                amount: Uint128(amount_to_withdraw_from_farm_contract),
            })?,
            send: vec![],
        }
        .into(),
    );
    // 3. At this point we've got the withdrawal from the farm address and the unclaimed reward so it's time to transfer the token back to the user
    messages.push(secret_toolkit::snip20::transfer_msg(
        env.message.sender.clone(),
        Uint128(amount_of_token),
        None,
        RESPONSE_BLOCK_SIZE,
        config.token.contract_hash.clone(),
        config.token.address.clone(),
    )?);

    // 4. Commission from claimed rewards
    // At this point the reward will be in the account and the performance fee will be sent to admin
    let commission: u128 =
        unclaimed_rewards(&deps.querier, env.clone(), config.clone()).unwrap() * 500 / 10_000;
    messages.push(secret_toolkit::snip20::transfer_msg(
        config.admin,
        Uint128(commission),
        None,
        RESPONSE_BLOCK_SIZE,
        config.token.contract_hash,
        config.token.address,
    )?);

    Ok(HandleResponse {
        messages,
        log: vec![],
        data: Some(to_binary(&StakingHandleAnswer::Redeem { status: Success })?),
    })
}
