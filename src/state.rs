use cosmwasm_std::{HumanAddr, Storage};
use cosmwasm_storage::{singleton, singleton_read, ReadonlySingleton, Singleton};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const CONFIG_KEY: &[u8] = b"config";

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone, JsonSchema)]
pub struct SecretContract {
    pub address: HumanAddr,
    pub contract_hash: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub buttcoin: SecretContract,
    pub contract_address: HumanAddr,
    // e.g. SEFI staking contract
    pub farm_contract: SecretContract,
    pub profit_sharing_contract: SecretContract,
    // incentivized_token and reward_token will be the same in this contract
    pub token: SecretContract,
    pub shares_token: SecretContract,
    pub admin: HumanAddr,
    // Need this for contract to view its own balance of SNIP-20 tokens
    pub viewing_key: String,
    pub stopped: bool,
}

pub fn config<S: Storage>(storage: &mut S) -> Singleton<S, State> {
    singleton(storage, CONFIG_KEY)
}

pub fn config_read<S: Storage>(storage: &S) -> ReadonlySingleton<S, State> {
    singleton_read(storage, CONFIG_KEY)
}
