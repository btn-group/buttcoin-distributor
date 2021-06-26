use cosmwasm_schema::{export_schema, remove_schemas, schema_for};
use cw_buttcoin_distributor::msg::{
    ButtcoinDistributorHandleMsg, InitMsg, LPStakingHandleMsg, QueryAnswer, QueryMsg,
};
use std::env::current_dir;
use std::fs::create_dir_all;

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InitMsg), &out_dir);
    export_schema(&schema_for!(ButtcoinDistributorHandleMsg), &out_dir);
    export_schema(&schema_for!(LPStakingHandleMsg), &out_dir);
    export_schema(&schema_for!(QueryAnswer), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
}
