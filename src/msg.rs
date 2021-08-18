use crate::state::SecretContract;
use cosmwasm_std::{Binary, HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub buttcoin: SecretContract,
    pub end_block: u64,
    pub starting_block: u64,
    pub release_per_block: Uint128,
    pub viewing_key: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ButtcoinDistributorHandleMsg {
    ClaimButtcoin {
        hook: Option<Binary>,
    },
    SetReceivableSmartContract {
        receivable_smart_contract: SecretContract,
    },
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ButtcoinDistributorHandleAnswer {
    ClaimButtcoin {
        status: ButtcoinDistributorResponseStatus,
    },
    SetReceivableSmartContract {
        status: ButtcoinDistributorResponseStatus,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ButtcoinDistributorQueryMsg {
    Config {},
    Pending { block: u64 },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ButtcoinDistributorQueryAnswer {
    Config {
        buttcoin: SecretContract,
        end_block: u64,
        last_update_block: u64,
        receivable_smart_contract: Option<SecretContract>,
        release_per_block: Uint128,
        starting_block: u64,
        viewing_key: String,
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
pub enum YieldOptimizerDepositButtcoinHookMsg {
    ContinueDepositAfterButtcoinClaimed {
        depositer: HumanAddr,
        incentivized_token_amount: Uint128,
    },
    ContinueWithdrawalAfterButtcoinClaimed {
        withdrawer: HumanAddr,
        shares_amount: Uint128,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum YieldOptimizerReceiveMsg {
    DepositButtcoin { hook: Option<Binary> },
}
