use anyhow::{Result, anyhow};
use once_cell::sync::OnceCell;
use ethers::types::U256;
use ethers::utils::parse_ether;
use ethers::prelude::{rand::Rng, *};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::fmt;

use crate::constants::{
    BRIBE_AMOUNT_PER_DUST, BEIBE_RATIO_BP,
    RATIO_FLOAT_BP, CAN_BUNDLE_FLOOR_BALANCE
};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum BribeStrategy {

    Ratio,
}
impl fmt::Display for BribeStrategy {

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == BribeStrategy::Ratio {
            write!(f, "Ratio")
        } else {
            write!(f, "Ratio")
        }
    }
}


#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum BribeStatus {

    Fixed,
    Float,
}

impl fmt::Display for BribeStatus {

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == BribeStatus::Fixed {
            write!(f, "Fixed")
        } else {
            write!(f, "Float")
        }
    }
}


#[derive(Debug, Deserialize, Serialize)]
pub struct DynamicConfig {
    bribe_strategy: BribeStrategy,
    bribe_status: BribeStatus,
    baseamount_per_dust: f32,  // use when token balance is zero
    ratio_basepoint: i64,  // Base Point, use when strategy is Ratio
    ratio_floatpoint: i64,   // Float point, use when strategy is Ratio and status is Float
    bundle_floor_balance: f32,  // When searcher‘s balance is below this one, pause make bundle
}


pub static DYNAMIC_CONFIG: OnceCell<Mutex<DynamicConfig>> = OnceCell::new();


pub fn init_config() {

    let config = DynamicConfig {
        bribe_strategy: BribeStrategy::Ratio,
        bribe_status: BribeStatus::Float,
        baseamount_per_dust: BRIBE_AMOUNT_PER_DUST,
        ratio_basepoint: BEIBE_RATIO_BP,
        ratio_floatpoint: RATIO_FLOAT_BP,
        bundle_floor_balance: CAN_BUNDLE_FLOOR_BALANCE,
    };
    let _ = DYNAMIC_CONFIG.set(Mutex::new(config));
}


pub fn get_all_config() -> DynamicConfig {

    let config = DYNAMIC_CONFIG.get().unwrap().lock().unwrap();
    DynamicConfig { 
        bribe_strategy: config.bribe_strategy.clone(),
        bribe_status: config.bribe_status.clone(),
        baseamount_per_dust: config.baseamount_per_dust.clone(),
        ratio_basepoint: config.ratio_basepoint.clone(),
        ratio_floatpoint: config.ratio_floatpoint.clone(),
        bundle_floor_balance: config.bundle_floor_balance.clone(),
    }
}


pub fn get_config(key: String) -> String {

    let config = DYNAMIC_CONFIG.get().unwrap().lock().unwrap();
    if "bribe_strategy" == key {
        return format!("{}", config.bribe_strategy);
    } else if "bribe_status" == key {
        return format!("{}", config.bribe_status);
    } else if "baseamount_per_dust" == key {
        return config.baseamount_per_dust.to_string();
    } else if "ratio_basepoint" == key {
        return config.ratio_basepoint.to_string();
    } else if "ratio_floatpoint" == key {
        return config.ratio_floatpoint.to_string();
    } else if "bundle_floor_balance" == key {
        return config.bundle_floor_balance.to_string();
    } else {
        return String::from("_");
    }
}

fn string_to_strategy(strategy: &str) -> Option<BribeStrategy> {

    match strategy {
        "ratio" | "Ratio" => {
            Some(BribeStrategy::Ratio)
        },
        _ => {
            None
        }
    }
}


fn string_to_status(status: &str) -> Option<BribeStatus> {

    match status {
        "fixed" | "Fixed" => {
            Some(BribeStatus::Fixed)
        },
        "float" | "Float" => {
            Some(BribeStatus::Float)
        },
        _ => {
            None
        }
    }
}


pub fn set_config(key: String, value: String) -> Result<()> {

    let mut config = DYNAMIC_CONFIG.get().unwrap().lock().unwrap();
    if "bribe_strategy" == key {
        match string_to_strategy(&value) {
            Some(s) => {
                config.bribe_strategy = s;
            },
            None => {}
        }
    } else if "bribe_status" == key {
        match string_to_status(&value) {
            Some(s) => {
                config.bribe_status = s;
            },
            None => {}
        }
    } else if "baseamount_per_dust" == key {
        config.baseamount_per_dust = value.parse().unwrap();
    } else if "ratio_basepoint" == key {
        let base_point = i64::from_str_radix(&value, 10).unwrap();
        config.ratio_basepoint = base_point;
    } else if "ratio_floatpoint" == key {
        let float_point = i64::from_str_radix(&value, 10).unwrap();
        config.ratio_floatpoint = float_point;
    } else if "bundle_floor_balance" == key {
        config.bundle_floor_balance = value.parse().unwrap();
    }

    Ok(())
}


pub fn is_searcher_balance_over_floor_required(searcher_weth_balance: U256) -> bool {

    let config = get_all_config();
    let floor_balance = parse_ether(config.bundle_floor_balance.to_string()).unwrap();
    return searcher_weth_balance > floor_balance;
}


pub fn calculate_runtime_bribe_amount_u128(revenue: u128, without_dust_token_num: u64) -> Result<u128> {

    let revenue_minus_front_gas = U256::from(revenue);
    let result = calculate_runtime_bribe_amount(revenue_minus_front_gas, without_dust_token_num);
    match result {
        Ok(bribe) => {
            return Ok(bribe.as_u128());
        },
        Err(e) => {
            return Result::Err(anyhow!(e.to_string()));
        }
    }
}


pub fn calculate_runtime_bribe_amount(revenue_minus_front_gas: U256, without_dust_token_num: u64) -> Result<U256> {

    let mut bribe_amount = revenue_minus_front_gas.clone();

    let config = get_all_config();
    if without_dust_token_num > 0 {
        // eat a loss (overpay) to get dust onto the sando contract (more: https://twitter.com/libevm/status/1474870661373779969)    
        let dust_amount = config.baseamount_per_dust * without_dust_token_num as f32;
        bribe_amount = revenue_minus_front_gas + parse_ether(dust_amount.to_string()).unwrap();
    } else {
        match config.bribe_strategy {
            BribeStrategy::Ratio => {
                match config.bribe_status {
                    BribeStatus::Fixed => {
                        bribe_amount = revenue_minus_front_gas * config.ratio_basepoint / 1_000_000_000
                    },
                    BribeStatus::Float => {
                        let mut rng = rand::thread_rng();
                        let floatpoint = config.ratio_basepoint + rng.gen_range(0..config.ratio_floatpoint);
                        bribe_amount = (revenue_minus_front_gas * floatpoint) / 1_000_000_000
                    }
                }
            },
            _ => {
                return Err(anyhow!("bribe strategy is none"));
            }
        }
    }

    Ok(bribe_amount)
}