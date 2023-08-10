use ethers::{
    abi,
    types::{Address, H160, U256},
    utils::parse_units,
};
use foundry_evm::{executor::fork::SharedBackend, revm::db::CacheDB};
use std::{collections::HashMap, str::FromStr};
use toml::Value;

#[derive(Debug, Clone, Default)]
struct SlotIndex {
    decimals: u32,
    index: i64,
}
impl SlotIndex {
    pub fn new(decimals: u32, index: i64) -> Self {
        Self { decimals, index }
    }
}
#[derive(Debug, Clone)]
pub struct CreditHelper {
    slot_index_map: HashMap<Address, SlotIndex>,
}

impl CreditHelper {
    // not so elegeant but create sim env from state diffs
    pub fn new() -> Self {
        // load slot index map
        let toml_str = std::fs::read_to_string("Slot.toml").expect("Unable to read config file");
        let parsed: Value = toml::from_str(&toml_str).expect("Failed to parse TOML");
        let mut slot_index_map: HashMap<Address, SlotIndex> = HashMap::new();
        for (key, value) in parsed.as_table().unwrap() {
            let index = value["index"].as_integer().unwrap();
            let decimals = value["decimals"].as_integer().unwrap() as u32;

            let slot_index = SlotIndex::new(decimals, index);
            let address = H160::from_str(key).expect("Invalid input token address");
            slot_index_map.insert(address, slot_index);
        }

        Self { slot_index_map }
    }

    pub fn credit_token(
        &self,
        input_token: Address,
        fork_db: &mut CacheDB<SharedBackend>,
        credit_addr: Address,
        amount: &str,
    ) {
        let slot_item: &SlotIndex = &self.slot_index_map[&input_token.clone()];
        // give sandwich contract some weth for swap
        let slot: U256 = ethers::utils::keccak256(abi::encode(&[
            abi::Token::Address(credit_addr.0.into()),
            abi::Token::Uint(U256::from(slot_item.index)),
        ]))
        .into();
        // update changes
        let credit_balance: U256 = parse_units(amount, slot_item.decimals).unwrap().into();
        fork_db
            .insert_account_storage(input_token.0.into(), slot.into(), credit_balance.into())
            .unwrap();
    }
}
