use ethers::{
    abi,
    types::{Address, H160, U256},
    utils::parse_units,
};
use foundry_evm::{executor::fork::SharedBackend, revm::db::CacheDB};
use lazy_static::lazy_static;
use std::{collections::HashMap, env, str::FromStr};
use toml::Value;
#[derive(Debug, Clone, Default)]
struct SlotIndex {
    symbol: String,
    decimals: u32,
    index: i64,
}
impl SlotIndex {
    pub fn new(symbol: String, decimals: u32, index: i64) -> Self {
        Self { symbol, decimals, index }
    }
}
#[derive(Debug, Clone)]
pub struct CreditHelper {
    slot_index_map: HashMap<Address, SlotIndex>,
}

impl CreditHelper {
    // not so elegeant but create sim env from state diffs
    pub fn new() -> Self {
        lazy_static! {
            static ref SLOT_INDEX_MAP: HashMap<Address, SlotIndex> = {
                log::info!("current dir {:?}", env::current_dir().unwrap());
                let toml_str =
                    std::fs::read_to_string("Slot.toml").expect("Unable to read config file");
                let parsed: Value = toml::from_str(&toml_str).expect("Failed to parse TOML");
                let mut slot_index_map: HashMap<Address, SlotIndex> = HashMap::new();
                for (key, value) in parsed.as_table().unwrap() {
                    let index = value["index"].as_integer().unwrap();
                    let decimals = value["decimals"].as_integer().unwrap() as u32;
                    let symbol = String::from(value["symbol"].as_str().unwrap());

                    let slot_index = SlotIndex::new(symbol, decimals, index);
                    let address = H160::from_str(key).expect("Invalid input token address");
                    slot_index_map.insert(address, slot_index);
                }
                slot_index_map
            };
        }

        Self {
            slot_index_map: SLOT_INDEX_MAP.clone(),
        }
    }

    pub fn credit_token_from_base(
        &self,
        input_token: Address,
        fork_db: &mut CacheDB<SharedBackend>,
        credit_addr: Address,
        base_amount: &str,
    ) {
        //log::info!("current dir {:?}", &self.slot_index_map);
        let slot_item: &SlotIndex = &self.slot_index_map[&input_token.clone()];
        // give sandwich contract some weth for swap
        let slot: U256 = ethers::utils::keccak256(abi::encode(&[
            abi::Token::Address(credit_addr.0.into()),
            abi::Token::Uint(U256::from(slot_item.index)),
        ]))
        .into();
        // update changes
        let credit_balance: U256 = parse_units(base_amount, slot_item.decimals).unwrap().into();
        fork_db
            .insert_account_storage(input_token.0.into(), slot.into(), credit_balance.into())
            .unwrap();
    }

    pub fn credit_multi_tokens_balance(
        &self,
        token_amouts: &HashMap<Address, U256>, 
        fork_db: &mut CacheDB<SharedBackend>,
        sando_address: Address
    ) {
        if token_amouts.len() > 0 {
            for (input_token, amount) in token_amouts.iter() {
                self.credit_token_balance(*input_token, fork_db, sando_address, *amount);
            }
        }
    }

    pub fn credit_token_balance(
        &self,
        input_token: Address,
        fork_db: &mut CacheDB<SharedBackend>,
        sando_address: Address,
        amount: U256,
    ) {
        if &self.slot_index_map.contains_key(&input_token.clone()) {

            let slot_item: &SlotIndex = &self.slot_index_map[&input_token.clone()];
            // give sandwich contract some weth for swap
            let slot: U256 = ethers::utils::keccak256(abi::encode(&[
                abi::Token::Address(sando_address.0.into()),
                abi::Token::Uint(U256::from(slot_item.index)),
            ]))
            .into();
            // update changes
            let credit_balance = amount;
            fork_db
                .insert_account_storage(input_token.0.into(), slot.into(), credit_balance.into())
                .unwrap();
        }
    }

    pub fn base_to_amount(&self,
        input_token: Address,
        amount: &str,) -> U256 {
        
        let slot_item: &SlotIndex = &self.slot_index_map[&input_token.clone()];
        parse_units(amount, slot_item.decimals).unwrap().into() 
    }

    pub fn token_can_swap(&self, input_token: Address) -> bool {
        self.slot_index_map.contains_key(&input_token.clone())
    }

    pub fn get_token_info(&self, input_token: Address) -> (String, u32) {
        if self.slot_index_map.contains_key(&input_token) {
            (self.slot_index_map[&input_token].symbol.clone(), self.slot_index_map[&input_token].decimals)
        } else {
            (String::from("zUnknow"), 1)
        }
    }
}
