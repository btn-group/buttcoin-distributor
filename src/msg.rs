use crate::state::{Schedule, SecretContract, WeightInfo};
use cosmwasm_std::{Binary, HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub buttcoin: SecretContract,
    pub release_schedule: Schedule,
    pub viewing_key: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ButtcoinDistributorHandleMsg {
    ClaimButtcoin {
        receivable_contract_address: HumanAddr,
        hook: Option<Binary>,
    },
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
pub enum ButtcoinDistributorHandleAnswer {
    ChangeAdmin {
        status: ButtcoinDistributorResponseStatus,
    },
    ClaimButtcoin {
        status: ButtcoinDistributorResponseStatus,
    },
    SetWeights {
        status: ButtcoinDistributorResponseStatus,
    },
    SetSchedule {
        status: ButtcoinDistributorResponseStatus,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ButtcoinDistributorQueryMsg {
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
pub enum ButtcoinDistributorQueryAnswer {
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

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ButtcoinDistributorResponseStatus {
    Success,
    Failure,
}

// === YieldOptimizer ===

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum YieldOptimizerHandleMsg {
    ButtcoinClaimedCallback { hook: Option<Binary> },
}
