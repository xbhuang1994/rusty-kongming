use anyhow::Result;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::fmt;
use crate::constants::{DUST_OVERPAY_AMOUNT, DUST_OVERPAY_PB};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum BribeStrategy {

    Amount,
    Ratio,
}
impl fmt::Display for BribeStrategy {

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == BribeStrategy::Amount {
            write!(f, "Amount")
        } else {
            write!(f, "Ratio")
        }
    }
}


#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum BribeStatus {

    Stable,
    Range,
}

impl fmt::Display for BribeStatus {

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == BribeStatus::Stable {
            write!(f, "Stable")
        } else {
            write!(f, "Range")
        }
    }
}


#[derive(Debug, Deserialize, Serialize)]
pub struct DynamicConfig {
    bribe_strategy: BribeStrategy,
    bribe_status: BribeStatus,
    bribe_amount: String,  // use when strategy is amount
    bribe_basepoint: i32,  // Base Point, use when strategy is ratio
}


pub static DYNAMIC_CONFIG: OnceCell<Mutex<DynamicConfig>> = OnceCell::new();


pub fn init_config() {

    let config = DynamicConfig {
        bribe_strategy: BribeStrategy::Amount,
        bribe_status: BribeStatus::Stable,
        bribe_amount: String::from(DUST_OVERPAY_AMOUNT),
        bribe_basepoint: DUST_OVERPAY_PB,
    };
    let _ = DYNAMIC_CONFIG.set(Mutex::new(config));
}


pub fn get_all_config() -> DynamicConfig {

    let config = DYNAMIC_CONFIG.get().unwrap().lock().unwrap();
    DynamicConfig { 
        bribe_strategy: config.bribe_strategy.clone(),
        bribe_status: config.bribe_status.clone(),
        bribe_amount: config.bribe_amount.clone(),
        bribe_basepoint: config.bribe_basepoint.clone(),
    }
}


pub fn get_config(key: String) -> String {

    let config = DYNAMIC_CONFIG.get().unwrap().lock().unwrap();
    if "bribe_strategy" == key {
        return format!("{}", config.bribe_strategy);
    } else if "bribe_status" == key {
        return format!("{}", config.bribe_status);
    } else if "bribe_amount" == key {
        return config.bribe_amount.clone();
    } else if "bribe_basepoint" == key {
        return config.bribe_basepoint.to_string();
    } else {
        return String::from("_");
    }
}

fn string_to_strategy(strategy: &str) -> Option<BribeStrategy> {

    match strategy {
        "amount" | "Amount" => {
            Some(BribeStrategy::Amount)
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
        "stable" | "Stable" => {
            Some(BribeStatus::Stable)
        },
        "range" | "Range" => {
            Some(BribeStatus::Range)
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
    } else if "bribe_amount" == key {
        config.bribe_amount = value
    } else if "bribe_basepoint" == key {
        let base_point = i32::from_str_radix(&value, 10).unwrap();
        config.bribe_basepoint = base_point;
    }

    Ok(())
}


pub fn adjust_bribe_amount(amount: String, increase: bool) -> String {

    let mut amount: f32 = amount.parse().unwrap();
    if increase {
        amount += 0.0000001f32;
    } else {
        if amount > 0f32 {
            amount -= 0.0000001f32;
        }
    }
    amount.to_string()
}


pub fn adjust_bribe_basepoint(basepoint: i32, increase: bool) -> i32 {

    let mut basepoint = basepoint;
    if increase {
        if basepoint < 1000 {
            basepoint += 1;
        }
    } else {
        if basepoint > 0 {
            basepoint -= 1;
        }
    }
    basepoint
}

pub fn get_runtime_bribe(revenue: u128) -> String {

    // todo
    return String::from("");
}