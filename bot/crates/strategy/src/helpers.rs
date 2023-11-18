use anyhow::{anyhow, Result};
use ethers::{
    signers::{LocalWallet, Signer},
    types::{
        transaction::{
            eip2718::TypedTransaction,
            eip2930::{AccessList, AccessListItem},
        },
        Transaction,
        BigEndianHash, Bytes, Eip1559TransactionRequest, H256,
        U256,
    },
};
use foundry_evm::{
    executor::{rU256, B160},
    utils::{b160_to_h160, h160_to_b160, ru256_to_u256, u256_to_ru256},
};
use crate::types::RawIngredients;
use crate::constants::{WETH_ADDRESS, FUND_OTHER_AMT_BASE};

pub fn tx_logic_max_fee_per_gas(victim_tx: &Transaction) -> U256 {

    let max_fee_per_gas = victim_tx.max_fee_per_gas.unwrap_or_default();
    let max_priority_fee_per_gas = victim_tx.max_priority_fee_per_gas.unwrap_or_default();
    let gas_price = victim_tx.gas_price.unwrap_or_default();

    let mut max_fee = U256::zero();
    if !max_fee_per_gas.is_zero() {
        max_fee = max_fee_per_gas;
    } else if !max_priority_fee_per_gas.is_zero() {
        max_fee = max_priority_fee_per_gas;
    } else if !gas_price.is_zero() {
        max_fee = gas_price;
    }

    max_fee
}

/// Sign eip1559 transactions
pub async fn sign_eip1559(
    tx: Eip1559TransactionRequest,
    signer_wallet: &LocalWallet,
) -> Result<Bytes> {
    let tx_typed = TypedTransaction::Eip1559(tx);
    let signed_frontrun_tx_sig = signer_wallet
        .sign_transaction(&tx_typed)
        .await
        .map_err(|e| anyhow!("Failed to sign eip1559 request: {:?}", e))?;

    Ok(tx_typed.rlp_signed(&signed_frontrun_tx_sig))
}

/// convert revm access list to ethers access list
pub fn access_list_to_ethers(access_list: Vec<(B160, Vec<rU256>)>) -> AccessList {
    AccessList::from(
        access_list
            .into_iter()
            .map(|(address, slots)| AccessListItem {
                address: b160_to_h160(address),
                storage_keys: slots
                    .into_iter()
                    .map(|y| H256::from_uint(&ru256_to_u256(y)))
                    .collect(),
            })
            .collect::<Vec<AccessListItem>>(),
    )
}

/// convert ethers access list to revm access list
pub fn access_list_to_revm(access_list: AccessList) -> Vec<(B160, Vec<rU256>)> {
    access_list
        .0
        .into_iter()
        .map(|x| {
            (
                h160_to_b160(x.address),
                x.storage_keys
                    .into_iter()
                    .map(|y| u256_to_ru256(y.0.into()))
                    .collect(),
            )
        })
        .collect()
}

/// get inventory for token when debug
pub fn calculate_inventory_for_debug(
        ingredients: &RawIngredients,
    ) -> (String, u32, U256) {
    if ingredients.get_start_end_token() == *WETH_ADDRESS {
        (String::from("wETH"), 18, (*crate::constants::WETH_FUND_AMT).into())
    } else {
        if ingredients.get_credit_helper_ref().token_can_swap(ingredients.get_start_end_token()) {
            let (symbol, decimals) = ingredients.get_credit_helper_ref()
                .get_token_info(
                    ingredients.get_start_end_token()
                );
            if decimals > 0 {
                let token_decimals = calculate_token_decimals(decimals);
                let inventory = U256::from(token_decimals).checked_mul(U256::from(FUND_OTHER_AMT_BASE)).unwrap_or_default();
                return (symbol, decimals, inventory);
            }
        }
        (String::from("zUnknown"), 1, U256::zero())
    }
}

pub fn calculate_token_decimals(decimals: u32) -> u128 {
    U256::pow(U256::from(10), U256::from(decimals)).as_u128()
}

/// get token symbol & decimal
pub fn get_start_token_info(
        ingredients: &RawIngredients,
    ) -> (String, u32) {
    if ingredients.get_start_end_token() == *WETH_ADDRESS {
        (String::from("wETH"), 18)
    } else {
        if ingredients.get_credit_helper_ref().token_can_swap(ingredients.get_start_end_token()) {
            ingredients.get_credit_helper_ref()
                .get_token_info(
                    ingredients.get_start_end_token()
            )
        } else {
            (String::from("zUnknown"), 1)
        }
    }
}

//
// -- Logging Macros --
//
#[macro_export]
macro_rules! log_info_cyan {
    ($($arg:tt)*) => {
        info!("{}", format_args!($($arg)*).to_string().cyan());
    };
}

#[macro_export]
macro_rules! log_not_sandwichable {
    ($($arg:tt)*) => {
        info!("{}", format_args!($($arg)*).to_string().yellow())
    };
}

#[macro_export]
macro_rules! log_bundle {
    ($is_huge:expr, $is_mixed:expr, $is_overlay:expr, $uuid:expr, $swap_type:expr, $head_txs:expr, $meats:expr, $block_number:expr, $revenue:expr, $frontrun_gas_used:expr, $backrun_gas_used:expr, $base_fee:expr, $max_fee:expr, $profit_min:expr, $profit_max:expr) => {
        info!("{}", format!("[BUILT BUNDLE]"));
        info!(
            "{}",
            format!("is_huge: {}, is_mixed: {}, is_overlay: {}, uuid: {}",
                $is_huge.to_string(),
                $is_mixed.to_string(),
                $is_overlay.to_string(),
                $uuid.to_string()
            )
        );
        info!(
            "{}",
            format!("swap type: {}", $swap_type.to_string())
        );
        info!(
            "{}",
            format!("head_txs: {}", $head_txs.to_string())
        );
        info!(
            "{}",
            format!("meats: {}", $meats.to_string())
        );
        info!(
            "{}",
            format!(
                "taget_block_number: {} ",
                $block_number.to_string()
            )
        );
        info!(
            "{}",
            format!(
                "revenue      : {} wETH",
                $revenue.to_string()
            )
        );
        info!(
            "{}",
            format!(
                "frontrun_gas_used: {} ",
                $frontrun_gas_used.to_string()
            )
        );
        info!(
            "{}",
            format!(
                "backrun_gas_used: {} ",
                $backrun_gas_used.to_string()
            )
        );
        info!(
            "{}",
            format!(
                "base_fee: {}, max_fee: {} ",
                $base_fee.to_string(),
                $max_fee.to_string()
            )
        );
        info!(
            "{}",
            format!(
                "expect profit: [{} ~ {}] ",
                $profit_min.to_string(),
                $profit_max.to_string()
            )
        );
    };
}

#[macro_export]
macro_rules! log_opportunity {
    ($for_huge:expr, $uuid:expr, $swap_type:expr, $token_symbol:expr, $head_txs:expr, $meats:expr, $optimal_input:expr, $revenue:expr,$frontrun_gas_used:expr,$backrun_gas_used:expr) => {{
        
        info!("{}", format!("[OPPORTUNITY DETECTED]"));
        info!(
            "{}",
            format!("for_huge: {}, uuid: {}",
                $for_huge.to_string(),
                $uuid.to_string()
            )
        );
        info!(
            "{}",
            format!("swap type: {}", $swap_type.to_string())
        );
        info!(
            "{}",
            format!("head_txs: {}", $head_txs.to_string())
        );
        info!(
            "{}",
            format!("meats: {}", $meats.to_string())
        );
        info!(
            "{}",
            format!(
                "optimal_input: {} {}",
                $optimal_input.to_string(),
                $token_symbol.to_string()
            )
        );
        info!(
            "{}",
            format!(
                "revenue      : {} wETH",
                $revenue.to_string()
            )
        );
        info!(
            "{}",
            format!(
                "frontrun_gas_used: {} ",
                $frontrun_gas_used.to_string()
            )
        );
        info!(
            "{}",
            format!(
                "backrun_gas_used: {} ",
                $backrun_gas_used.to_string()
            )
        );
    }};
}

// #[macro_export]
// macro_rules! log_opportunity {
//     ($for_huge:expr, $uuid:expr, $swap_type:expr, $head_txs:expr, $meats:expr, $optimal_input:expr, $revenue:expr,$frontrun_gas_used:expr,$backrun_gas_used:expr) => {{
        
//         info!("{}", format!("[OPPORTUNITY DETECTED]"));
//         info!(
//             "{}",
//             format!("for_huge: {}, uuid: {}",
//                 $for_huge.to_string().green().on_black(),
//                 $uuid.to_string().green().on_black()
//             )
//         );
//         info!(
//             "{}",
//             format!("swap type: {}", $swap_type.to_string().green().on_black()).bold()
//         );
//         info!(
//             "{}",
//             format!("head_txs: {}", $head_txs.to_string().green().on_black()).bold()
//         );
//         info!(
//             "{}",
//             format!("meats: {}", $meats.to_string().green().on_black()).bold()
//         );
//         info!(
//             "{}",
//             format!(
//                 "optimal_input: {} wETH/Other",
//                 $optimal_input.to_string().green().on_black()
//             )
//             .bold()
//         );
//         info!(
//             "{}",
//             format!(
//                 "revenue      : {} wETH",
//                 $revenue.to_string().green().on_black()
//             )
//             .bold()
//         );
//         info!(
//             "{}",
//             format!(
//                 "frontrun_gas_used: {} ",
//                 $frontrun_gas_used.to_string().green().on_black()
//             )
//             .bold()
//         );
//         info!(
//             "{}",
//             format!(
//                 "backrun_gas_used: {} ",
//                 $backrun_gas_used.to_string().green().on_black()
//             )
//             .bold()
//         );
//     }};
// }

#[macro_export]
macro_rules! startup_info_log {
    ($($arg:tt)*) => {
        info!("{}", format_args!($($arg)*).to_string().on_black().yellow().bold());
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        error!("{}", format_args!($($arg)*).to_string().red());
    };
}

#[macro_export]
macro_rules! log_new_block_info {
    ($new_block:expr) => {
        log::info!(
            "{}",
            format!(
                "\nFound New Block\nLatest Block: (number:{:?}, timestamp:{:?}, basefee:{:?})",
                $new_block.number, $new_block.timestamp, $new_block.base_fee_per_gas,
            )
            .bright_purple()
            .on_black()
        );
    };
}
