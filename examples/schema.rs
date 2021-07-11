use cosmwasm_schema::{export_schema, remove_schemas, schema_for};
use cw_buttcoin_distributor::msg::{
    ButtcoinDistributorHandleAnswer, ButtcoinDistributorHandleMsg, ButtcoinDistributorQueryAnswer,
    ButtcoinDistributorQueryMsg, InitMsg,
};
use std::env::current_dir;
use std::fs::create_dir_all;

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();
    export_schema(&schema_for!(ButtcoinDistributorHandleAnswer), &out_dir);
    export_schema(&schema_for!(ButtcoinDistributorHandleMsg), &out_dir);
    export_schema(&schema_for!(ButtcoinDistributorQueryAnswer), &out_dir);
    export_schema(&schema_for!(ButtcoinDistributorQueryMsg), &out_dir);
    export_schema(&schema_for!(InitMsg), &out_dir);
}
