use anyhow::{Result, anyhow};
use once_cell::sync::OnceCell;
use ethers::types::U256;
use ethers::utils::parse_ether;
use ethers::prelude::{rand::Rng, *};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::fmt;

use crate::constants::{BRIBE_OVERPAY_AMOUNT, BEIBE_RATIO_BP, OVERPAY_FLOAT_AMOUNT, RATIO_FLOAT_BP};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum BribeStrategy {

    Overpay,
    Ratio,
}
impl fmt::Display for BribeStrategy {

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == BribeStrategy::Overpay {
            write!(f, "Overpay")
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
    overpay_baseamount: f32,  // use when strategy is Overpay
    ratio_basepoint: i64,  // Base Point, use when strategy is Ratio
    overpay_floatmount: f32,  // Flaot mount, use when strategy is Overpay and status is Float
    ratio_floatpoint: i64,   // Float point, use when strategy is Ratio and status is Float

}


pub static DYNAMIC_CONFIG: OnceCell<Mutex<DynamicConfig>> = OnceCell::new();


pub fn init_config() {

    let config = DynamicConfig {
        bribe_strategy: BribeStrategy::Overpay,
        bribe_status: BribeStatus::Fixed,
        overpay_baseamount: BRIBE_OVERPAY_AMOUNT,
        ratio_basepoint: BEIBE_RATIO_BP,
        overpay_floatmount: OVERPAY_FLOAT_AMOUNT,
        ratio_floatpoint: RATIO_FLOAT_BP,
    };
    let _ = DYNAMIC_CONFIG.set(Mutex::new(config));
}


pub fn get_all_config() -> DynamicConfig {

    let config = DYNAMIC_CONFIG.get().unwrap().lock().unwrap();
    DynamicConfig { 
        bribe_strategy: config.bribe_strategy.clone(),
        bribe_status: config.bribe_status.clone(),
        overpay_baseamount: config.overpay_baseamount.clone(),
        ratio_basepoint: config.ratio_basepoint.clone(),
        overpay_floatmount: config.overpay_floatmount.clone(),
        ratio_floatpoint: config.ratio_floatpoint.clone(),
    }
}


pub fn get_config(key: String) -> String {

    let config = DYNAMIC_CONFIG.get().unwrap().lock().unwrap();
    if "bribe_strategy" == key {
        return format!("{}", config.bribe_strategy);
    } else if "bribe_status" == key {
        return format!("{}", config.bribe_status);
    } else if "overpay_baseamount" == key {
        return config.overpay_baseamount.to_string();
    } else if "ratio_basepoint" == key {
        return config.ratio_basepoint.to_string();
    } else if "overpay_floatmount" == key {
        return config.overpay_floatmount.to_string();
    } else if "ratio_floatpoint" == key {
        return config.ratio_floatpoint.to_string();
    } else {
        return String::from("_");
    }
}

fn string_to_strategy(strategy: &str) -> Option<BribeStrategy> {

    match strategy {
        "overpay" | "Overpay" => {
            Some(BribeStrategy::Overpay)
        },
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
    } else if "overpay_baseamount" == key {
        config.overpay_baseamount = value.parse().unwrap();
    } else if "ratio_basepoint" == key {
        let base_point = i64::from_str_radix(&value, 10).unwrap();
        config.ratio_basepoint = base_point;
    } else if "overpay_floatmount" == key {
        config.overpay_floatmount = value.parse().unwrap();
    } else if "ratio_floatpoint" == key {
        let float_point = i64::from_str_radix(&value, 10).unwrap();
        config.ratio_floatpoint = float_point;
    }

    Ok(())
}


pub fn calculate_runtime_bribe_amount_u128(revenue: u128) -> Result<u128> {

    let revenue_minus_front_gas = U256::from(revenue);
    let result = calculate_runtime_bribe_amount(revenue_minus_front_gas);
    match result {
        Ok(bribe) => {
            return Ok(bribe.as_u128());
        },
        Err(e) => {
            return Result::Err(anyhow!(e.to_string()));
        }
    }
}


pub fn calculate_runtime_bribe_amount(revenue_minus_front_gas: U256) -> Result<U256> {

    let mut bribe_amount = revenue_minus_front_gas.clone();
    let config = get_all_config();
    match config.bribe_strategy {
        BribeStrategy::Overpay => {
            match config.bribe_status {
                BribeStatus::Fixed => {
                    let baseamount = config.overpay_baseamount.to_string();
                    bribe_amount = revenue_minus_front_gas + parse_ether(baseamount).unwrap();
                },
                BribeStatus::Float => {
                    let mut rng = rand::thread_rng();
                    let floatamount = rng.gen_range(0.0..config.overpay_floatmount) + config.overpay_baseamount;
                    bribe_amount = revenue_minus_front_gas + parse_ether(floatamount.to_string()).unwrap();
                },
                _ => {
                    return Err(anyhow!("bribe status is none"));
                }
            }
        },
        BribeStrategy::Ratio => {
            match config.bribe_status {
                BribeStatus::Fixed => {
                    bribe_amount = revenue_minus_front_gas * config.ratio_basepoint / 1_000_000_000
                },
                BribeStatus::Float => {
                    let mut rng = rand::thread_rng();
                    let floatpoint = config.ratio_basepoint + rng.gen_range(0..config.ratio_floatpoint);
                    bribe_amount = (revenue_minus_front_gas * floatpoint) / 1_000_000_000
                },
                _ => {
                    return Err(anyhow!("bribe status is none"));
                }
            }
        },
        _ => {
            return Err(anyhow!("bribe strategy is none"));
        }
    }

    Ok(bribe_amount)
}