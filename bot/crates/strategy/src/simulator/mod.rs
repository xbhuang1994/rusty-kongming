pub mod credit;
pub mod huff_helper;
pub mod huff_sando;
pub mod huff_sando_reverse;
pub(crate) mod lil_router;
pub(crate) mod lil_router_reverse;
pub(crate) mod salmonella_inspector;

use foundry_evm::{
    executor::fork::SharedBackend,
    revm::{db::CacheDB, primitives::U256 as rU256, EVM},
};

use crate::{
    constants::{COINBASE, ONE_ETHER_IN_WEI},
    types::BlockInfo,
};
use ethers::types::U256;

fn setup_block_state(evm: &mut EVM<CacheDB<SharedBackend>>, next_block: &BlockInfo) {
    evm.env.block.number = rU256::from(next_block.number.as_u64());
    evm.env.block.timestamp = next_block.timestamp.into();
    evm.env.block.basefee = next_block.base_fee_per_gas.into();
    // use something other than default
    evm.env.block.coinbase = *COINBASE;
}

pub fn eth_to_wei(amt: u128) -> rU256 {
    rU256::from(amt).checked_mul(*ONE_ETHER_IN_WEI).unwrap()
}

/// balance difference that can calculate the revenue
fn is_balance_diff_for_revenue(start_balance: U256, end_balance: U256) -> bool {
    let mut diff = U256::zero();
    if end_balance >= start_balance {
        diff = end_balance.checked_sub(start_balance).unwrap_or_default();
    }
    let min_diff = start_balance.checked_div(U256::from(10000)).unwrap_or_default();
    return diff > U256::zero() && diff <= min_diff;
}

/// backrun_in difference that can calculate the revenue
fn backrun_in_diff_for_revenue(backrun_in: U256) -> U256 {
    backrun_in.checked_div(U256::from(10000)).unwrap_or_default()
}

fn binary_search_weth_input(low_amount_in: U256, high_amount_in: U256, last_amount_in: U256, is_last_too_many: bool, current_round: i32)
    -> (bool, U256) {
    if current_round == 1 {
        return (true, high_amount_in);
    } else if current_round > 20 {
        return (false, U256::zero());
    }

    if low_amount_in >= high_amount_in {
        return (false, U256::zero());
    }

    if is_last_too_many {
        // reduce weth input amount
        if high_amount_in - low_amount_in == U256::from(1) {
            return (true, last_amount_in - 1);
        } else {
            let range = (high_amount_in - low_amount_in) / 2;
            if last_amount_in > range {
                return (true, last_amount_in - range);
            } else {
                return (false, U256::zero());
            }
        }
    } else {
        if current_round == 2 {
            return (false, U256::zero());
        } else {
            // increase weth input amount
            if high_amount_in - low_amount_in == U256::from(1) {
                return (true, last_amount_in + 1);
            } else {
                return (true, last_amount_in + (high_amount_in - low_amount_in) / 2);
            }
        }
    }
}