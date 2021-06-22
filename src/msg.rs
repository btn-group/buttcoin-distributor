use crate::contract::RESPONSE_BLOCK_SIZE;
use crate::state::SecretContract;
use cosmwasm_std::{Binary, HumanAddr, Uint128};
use schemars::JsonSchema;
use secret_toolkit::utils::Query;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LPStakingQueryMsg {
    Rewards {
        address: HumanAddr,
        key: String,
        height: u64,
    },
}
impl Query for LPStakingQueryMsg {
    const BLOCK_SIZE: usize = RESPONSE_BLOCK_SIZE;
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
pub enum LPStakingReceiveMsg {
    Deposit {},
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
pub struct LPStakingRewardsResponse {
    pub rewards: Rewards,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
pub struct Rewards {
    pub rewards: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    Redeem {
        amount: Uint128,
    },

    // Registered commands
    Receive {
        sender: HumanAddr,
        from: HumanAddr,
        amount: Uint128,
        msg: Binary,
    },

    // Admin commands
    StopContract {},
    ResumeContract {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub farm_contract: SecretContract,
    pub profit_sharing_contract: SecretContract,
    pub token: SecretContract,
    pub shares_token: SecretContract,
    pub viewing_key: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum HandleAnswer {
    Redeem { status: ResponseStatus },
    StopContract { status: ResponseStatus },
    ResumeContract { status: ResponseStatus },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReceiveMsg {
    Deposit {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Balance { token: SecretContract },
    Config {},
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryAnswer {
    Config {
        admin: HumanAddr,
        stopped: bool,
        farm_contract: SecretContract,
        shares_token: SecretContract,
        token: SecretContract,
    },

    // Authenticated
    Balance {
        amount: Uint128,
    },
    ViewingKeyError {
        msg: String,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    Success,
}
