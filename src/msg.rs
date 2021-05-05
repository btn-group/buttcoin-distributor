use cosmwasm_std::{Binary, HumanAddr, Uint128};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone, JsonSchema)]
pub struct SecretContract {
    pub address: HumanAddr,
    pub contract_hash: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StakingHandleMsg {
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
    ChangeAdmin {
        address: HumanAddr,
    },

    // Master callbacks
    NotifyAllocation {
        amount: Uint128,
        hook: Option<Binary>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StakingInitMsg {
    pub farm_contract: SecretContract,
    pub inc_token: SecretContract,
    pub shares_token: SecretContract,
    pub viewing_key: String,
    pub prng_seed: Binary,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StakingHandleAnswer {
    Redeem { status: StakingResponseStatus },
    StopContract { status: StakingResponseStatus },
    ResumeContract { status: StakingResponseStatus },
    ChangeAdmin { status: StakingResponseStatus },
    SetDeadline { status: StakingResponseStatus },
    ClaimRewardPool { status: StakingResponseStatus },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StakingReceiveMsg {
    Deposit {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StakingHookMsg {
    Deposit {
        from: HumanAddr,
        amount: Uint128,
    },
    Redeem {
        to: HumanAddr,
        amount: Option<Uint128>,
    },
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StakingReceiveAnswer {
    Deposit { status: StakingResponseStatus },
    DepositRewards { status: StakingResponseStatus },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StakingQueryMsg {
    TokenInfo {},
    ContractStatus {},
    Token {},

    // Authenticated
    Rewards {
        address: HumanAddr,
        key: String,
        height: u64,
    },
    Balance {
        address: HumanAddr,
        key: String,
    },
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StakingQueryAnswer {
    Rewards { rewards: Uint128 },
    Balance { amount: Uint128 },
    ContractStatus { stopped: bool },
    Token { token: SecretContract },
    QueryError { msg: String },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StakingResponseStatus {
    Success,
    Failure,
}
