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
pub enum QueryAnswer {
    Rewards { rewards: Uint128 },
    Balance { amount: Uint128 },
    ContractStatus { stopped: bool },
    Token { token: SecretContract },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    Success,
}
