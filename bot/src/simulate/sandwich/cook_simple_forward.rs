use ethers::prelude::*;
use revm::primitives::{ExecutionResult, TransactTo, B160 as rAddress, U256 as rU256};

use crate::prelude::access_list::AccessListInspector;
use crate::prelude::fork_db::ForkDB;
use crate::prelude::fork_factory::ForkFactory;
use crate::prelude::is_sando_safu::{IsSandoSafu, SalmonellaInspectoooor};
use crate::prelude::sandwich_types::RawIngredients;
use crate::prelude::{
    convert_access_list, get_amount_out_evm, get_balance_of_evm,
    setup_block_state, PoolVariant
};
use crate::types::sandwich_types::OptimalRecipe;
use crate::types::{BlockInfo, SimulationError};
use crate::utils::tx_builder::{self, SandwichMaker};
use crate::utils::dotenv;

use super::sandwich_helper::juiced_quadratic_search;

// Calculate amount in that produces highest revenue and performs honeypot checks
//
// Arguments:
// `&ingredients`: holds onchain information about opportunity
// `sandwich_balance`: balance of sandwich contract
// `&next_block`: holds information about next block
// `&mut fork_factory`: used to create new forked evm instances for simulations
// `sandwich_maker`: handles encoding of transaction for sandwich contract
//
// Returns:
// Ok(OptimalRecipe) if no errors during calculation
// Err(SimulationError) if error during calculation
pub async fn create_optimal_sandwich(
    ingredients: &RawIngredients,
    sandwich_balance: U256,
    next_block: &BlockInfo,
    fork_factory: &mut ForkFactory,
    sandwich_maker: &SandwichMaker,
) -> Result<OptimalRecipe, SimulationError> {

    // find_token_slot_value(
    //     ingredients,
    //     next_block,
    //     fork_factory,
    //     sandwich_maker,
    //     true,
    // ).unwrap();

    let optimal = juiced_quadratic_search(
        ingredients,
        U256::zero(),
        sandwich_balance,
        next_block,
        fork_factory,
    )
    .await?;

    #[cfg(test)]
    {
        println!("Optimal amount in: {}", optimal);
    }

    if optimal.is_zero() {
        return Err(SimulationError::ZeroOptimal());
    }

    sanity_check(
        sandwich_balance,
        optimal,
        ingredients,
        next_block,
        sandwich_maker,
        fork_factory.new_sandbox_fork(),
    )
}

// Perform simulation using sandwich contract and check for salmonella
//
// Arguments:
// `sandwich_start_balance`: amount of token held by sandwich contract
// `frontrun_in`: amount to use as frontrun
// `ingredients`: holds information about opportunity
// `next_block`: holds information about next block
// `sandwich_maker`: handles encoding of transaction for sandwich contract
// `fork_db`: fork db used for evm simulations
//
// Returns:
// Ok(OptimalRecipe): params to pass to sandwich contract to capture opportunity
// Err(SimulationError): error encountered during simulation
fn sanity_check(
    _: U256,
    frontrun_in: U256,
    ingredients: &RawIngredients,
    next_block: &BlockInfo,
    sandwich_maker: &SandwichMaker,
    fork_db: ForkDB,
) -> Result<OptimalRecipe, SimulationError> {
    // setup evm simulation
    let mut evm = revm::EVM::new();
    evm.database(fork_db);
    setup_block_state(&mut evm, &next_block);

    let searcher = dotenv::get_searcher_wallet().address();
    let sandwich_contract = dotenv::get_sandwich_contract_address();
    let pool_variant = ingredients.target_pool.pool_variant;

    let sandwich_start_balance = get_balance_of_evm(
        ingredients.startend_token,
        sandwich_contract,
        next_block,
        &mut evm,
    )?;

    // *´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
    // *                    FRONTRUN TRANSACTION                    */
    // *.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
    //
    // encode frontrun_in before passing to sandwich contract
    let frontrun_in = match pool_variant {
        PoolVariant::UniswapV2 => tx_builder::v2::encode_weth(frontrun_in),
        PoolVariant::UniswapV3 => tx_builder::v3::encode_weth(frontrun_in),
    };

    // caluclate frontrun_out using encoded frontrun_in
    let frontrun_out = match pool_variant {
        PoolVariant::UniswapV2 => {
            let target_pool = ingredients.target_pool.address;
            let token_in = ingredients.startend_token;
            let token_out = ingredients.intermediary_token;
            evm.env.tx.gas_price = next_block.base_fee.into();
            evm.env.tx.gas_limit = 700000;
            evm.env.tx.value = rU256::ZERO;
            let amount_out =
                get_amount_out_evm(frontrun_in, target_pool, token_in, token_out, &mut evm)?;
            tx_builder::v2::decode_intermediary(amount_out, true, token_out)
        }
        PoolVariant::UniswapV3 => U256::zero(),
    };

    // create tx.data and tx.value for frontrun_in
    let (frontrun_data, frontrun_value) = match pool_variant {
        PoolVariant::UniswapV2 => sandwich_maker.v2.create_payload_weth_is_input(
            frontrun_in,
            frontrun_out,
            ingredients.intermediary_token,
            ingredients.target_pool,
            next_block.number,
        ),
        PoolVariant::UniswapV3 => sandwich_maker.v3.create_payload_weth_is_input(
            frontrun_in.as_u128().into(),
            ingredients.startend_token,
            ingredients.intermediary_token,
            ingredients.target_pool,
            next_block.number,
        ),
    };

    // setup evm for frontrun transaction
    evm.env.tx.caller = searcher.0.into();
    evm.env.tx.transact_to = TransactTo::Call(sandwich_contract.0.into());
    evm.env.tx.data = frontrun_data.clone().into();
    evm.env.tx.value = frontrun_value.into();
    evm.env.tx.gas_limit = 700000;
    evm.env.tx.gas_price = next_block.base_fee.into();
    evm.env.tx.access_list = Vec::default();

    // get access list
    let mut access_list_inspector = AccessListInspector::new(searcher, sandwich_contract);
    evm.inspect_ref(&mut access_list_inspector)
        .map_err(|e| SimulationError::FrontrunEvmError(e))
        .unwrap();
    let frontrun_access_list = access_list_inspector.into_access_list();
    evm.env.tx.access_list = frontrun_access_list.clone();

    // run again but now with access list (so that we get accurate gas used)
    // run with a salmonella inspector to flag `suspicious` opcodes
    let mut salmonella_inspector = SalmonellaInspectoooor::new();
    let frontrun_result = match evm.inspect_commit(&mut salmonella_inspector) {
        Ok(result) => result,
        Err(e) => return Err(SimulationError::FrontrunEvmError(e)),
    };
    match frontrun_result {
        ExecutionResult::Success { .. } => { /* continue operation */ }
        ExecutionResult::Revert { output, .. } => {
            return Err(SimulationError::FrontrunReverted(output))
        }
        ExecutionResult::Halt { reason, .. } => {
            return Err(SimulationError::FrontrunHalted(reason))
        }
    };
    match salmonella_inspector.is_sando_safu() {
        IsSandoSafu::Safu => { /* continue operation */ }
        IsSandoSafu::NotSafu(not_safu_opcodes) => {
            return Err(SimulationError::FrontrunNotSafu(not_safu_opcodes))
        }
    }

    let frontrun_gas_used = frontrun_result.gas_used();

    // *´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
    // *                     MEAT TRANSACTION/s                     */
    // *.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
    let mut is_meat_good = Vec::new();
    for meat in ingredients.meats.iter() {
        evm.env.tx.caller = rAddress::from_slice(&meat.from.0);
        evm.env.tx.transact_to =
            TransactTo::Call(rAddress::from_slice(&meat.to.unwrap_or_default().0));
        evm.env.tx.data = meat.input.0.clone();
        evm.env.tx.value = meat.value.into();
        evm.env.tx.chain_id = meat.chain_id.map(|id| id.as_u64());
        evm.env.tx.nonce = Some(meat.nonce.as_u64());
        evm.env.tx.gas_limit = meat.gas.as_u64();
        match meat.transaction_type {
            Some(ethers::types::U64([0])) => {
                // legacy tx
                evm.env.tx.gas_price = meat.gas_price.unwrap_or_default().into();
            }
            Some(_) => {
                // type 2 tx
                evm.env.tx.gas_priority_fee = meat.max_priority_fee_per_gas.map(|mpf| mpf.into());
                evm.env.tx.gas_price = meat.max_fee_per_gas.unwrap_or_default().into();
            }
            None => {
                // legacy tx
                evm.env.tx.gas_price = meat.gas_price.unwrap().into();
            }
        }

        // keep track of which meat transactions are successful to filter reverted meats at end
        // remove reverted meats because mempool tx/s gas costs are accounted for by fb
        let res = match evm.transact_commit() {
            Ok(result) => result,
            Err(e) => return Err(SimulationError::EvmError(e)),
        };
        match res.is_success() {
            true => is_meat_good.push(true),
            false => is_meat_good.push(false),
        }
    }
    // clean nonce with meat transactions
    evm.env.tx.nonce = None;

    // *´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
    // *                    BACKRUN TRANSACTION                     */
    // *.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
    //
    // encode backrun_in before passing to sandwich contract
    let token_in = ingredients.intermediary_token;
    let token_out = ingredients.startend_token;
    let balance = get_balance_of_evm(token_in, sandwich_contract, next_block, &mut evm)?;
    let backrun_in = match pool_variant {
        PoolVariant::UniswapV2 => {
            tx_builder::v2::encode_intermediary_with_dust(balance, false, token_in)
        }
        PoolVariant::UniswapV3 => tx_builder::v3::encode_intermediary_token(balance),
    };

    // caluclate backrun_out using encoded backrun_in
    let backrun_out = match pool_variant {
        PoolVariant::UniswapV2 => {
            let target_pool = ingredients.target_pool.address;
            let out = get_amount_out_evm(backrun_in, target_pool, token_in, token_out, &mut evm)?;
            tx_builder::v2::encode_weth(out)
        }
        PoolVariant::UniswapV3 => U256::zero(),
    };

    // create tx.data and tx.value for backrun_in
    let (backrun_data, backrun_value) = match pool_variant {
        PoolVariant::UniswapV2 => sandwich_maker.v2.create_payload_weth_is_output(
            backrun_in,
            backrun_out,
            ingredients.intermediary_token,
            ingredients.target_pool,
        ),
        PoolVariant::UniswapV3 => (
            sandwich_maker.v3.create_payload_weth_is_output(
                backrun_in.as_u128().into(),
                ingredients.intermediary_token,
                ingredients.startend_token,
                ingredients.target_pool,
            ),
            U256::zero(),
        ),
    };

    // setup evm for backrun transaction
    evm.env.tx.caller = searcher.0.into();
    evm.env.tx.transact_to = TransactTo::Call(sandwich_contract.0.into());
    evm.env.tx.data = backrun_data.clone().into();
    evm.env.tx.gas_limit = 700000;
    evm.env.tx.gas_price = next_block.base_fee.into();
    evm.env.tx.value = backrun_value.into();

    // create access list
    let mut access_list_inspector = AccessListInspector::new(searcher, sandwich_contract);
    evm.inspect_ref(&mut access_list_inspector)
        .map_err(|e| SimulationError::FrontrunEvmError(e))
        .unwrap();
    let backrun_access_list = access_list_inspector.into_access_list();
    evm.env.tx.access_list = backrun_access_list.clone();

    // run again but now with access list (so that we get accurate gas used)
    // run with a salmonella inspector to flag `suspicious` opcodes
    let mut salmonella_inspector = SalmonellaInspectoooor::new();
    let backrun_result = match evm.inspect_commit(&mut salmonella_inspector) {
        Ok(result) => result,
        Err(e) => return Err(SimulationError::BackrunEvmError(e)),
    };
    match backrun_result {
        ExecutionResult::Success { .. } => { /* continue */ }
        ExecutionResult::Revert { output, .. } => {
            return Err(SimulationError::BackrunReverted(output))
        }
        ExecutionResult::Halt { reason, .. } => return Err(SimulationError::BackrunHalted(reason)),
    };
    match salmonella_inspector.is_sando_safu() {
        IsSandoSafu::Safu => { /* continue operation */ }
        IsSandoSafu::NotSafu(not_safu_opcodes) => {
            return Err(SimulationError::BackrunNotSafu(not_safu_opcodes))
        }
    }

    let backrun_gas_used = backrun_result.gas_used();

    // *´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
    // *                      GENERATE REPORTS                      */
    // *.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
    //
    // caluclate revenue from balance change
    let post_sandwich_balance = get_balance_of_evm(
        ingredients.startend_token,
        sandwich_contract,
        next_block,
        &mut evm,
    )?;
    let revenue = post_sandwich_balance
        .checked_sub(sandwich_start_balance)
        .unwrap_or_default();

    // filter only passing meat txs
    let good_meats_only = ingredients
        .meats
        .iter()
        .zip(is_meat_good.iter())
        .filter(|&(_, &b)| b)
        .map(|(s, _)| s.to_owned())
        .collect();

    Ok(OptimalRecipe::new(
        frontrun_data.into(),
        frontrun_value,
        frontrun_gas_used,
        convert_access_list(frontrun_access_list),
        backrun_data.into(),
        backrun_value,
        backrun_gas_used,
        convert_access_list(backrun_access_list),
        good_meats_only,
        revenue,
        ingredients.target_pool,
        ingredients.state_diffs.clone(),
    ))
}
