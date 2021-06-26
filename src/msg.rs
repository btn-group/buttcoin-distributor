use crate::state::{Schedule, SecretContract, WeightInfo};
use cosmwasm_std::{Binary, HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LPStakingHandleMsg {
    // Master callbacks
    NotifyAllocation {
        amount: Uint128,
        hook: Option<Binary>,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum LPStakingResponseStatus {
    Success,
    Failure,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MasterInitMsg {
    pub release_schedule: Schedule,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MasterHandleMsg {
    UpdateAllocation {
        receivable_contract_address: HumanAddr,
        receivable_contract_hash: String,
        hook: Option<Binary>,
    },

    // Admin commands
    SetWeights {
        weights: Vec<WeightInfo>,
    },
    SetSchedule {
        schedule: Schedule,
    },
    ChangeAdmin {
        addr: HumanAddr,
    },
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MasterHandleAnswer {
    Success,
    Failure,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MasterQueryMsg {
    Config {},
    ReceivableContractWeight {
        addr: HumanAddr,
    },
    Pending {
        receivable_contract_address: HumanAddr,
        block: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MasterQueryAnswer {
    Config {
        admin: HumanAddr,
        buttcoin: SecretContract,
        schedule: Schedule,
        total_weight: u64,
        viewing_key: String,
    },
    ReceivableContractWeight {
        weight: u64,
    },
    Pending {
        amount: Uint128,
    },
}
