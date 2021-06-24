use cosmwasm_std::{HumanAddr, Storage, Uint128};
use cosmwasm_storage::{singleton, singleton_read, ReadonlySingleton, Singleton};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub static CONFIG_KEY: &[u8] = b"config";

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone, JsonSchema)]
pub struct SecretContract {
    pub address: HumanAddr,
    pub contract_hash: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SpySettings {
    pub weight: u64,
    pub last_update_block: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Copy)]
pub struct ScheduleUnit {
    pub end_block: u64,
    pub mint_per_block: Uint128,
}

pub type Schedule = Vec<ScheduleUnit>;

pub fn sort_schedule(s: &mut Schedule) {
    s.sort_by(|&s1, &s2| s1.end_block.cmp(&s2.end_block))
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub admin: HumanAddr,
    pub buttcoin: SecretContract,
    pub total_weight: u64,
    pub minting_schedule: Schedule,
}

pub fn config<S: Storage>(storage: &mut S) -> Singleton<S, State> {
    singleton(storage, CONFIG_KEY)
}

pub fn config_read<S: Storage>(storage: &S) -> ReadonlySingleton<S, State> {
    singleton_read(storage, CONFIG_KEY)
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone, JsonSchema)]
pub struct TokenInfo {
    pub name: String,
    pub symbol: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct WeightInfo {
    pub address: HumanAddr,
    pub hash: String,
    pub weight: u64,
}
