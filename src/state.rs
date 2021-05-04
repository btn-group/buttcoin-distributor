use crate::msg::SecretContract;
use cosmwasm_std::HumanAddr;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct Config {
    // e.g. SEFI staking contract
    pub farm_contract: SecretContract,
    // incentivized_token and reward_token will be the same in this contract
    pub token: SecretContract,
    pub shares_token: SecretContract,
    pub admin: HumanAddr,
    // Need this for contract to view its own balance of SNIP-20 tokens
    pub viewing_key: String,
    pub stopped: bool,
}
