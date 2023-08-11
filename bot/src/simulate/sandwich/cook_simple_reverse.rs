use std::ops::{Mul, Div};

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

use super::sandwich_helper_reverse::juiced_quadratic_search;


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
    //     false,
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


fn prepare_backrun_amount_in(
    frontrun_in: U256,
    ingredients: &RawIngredients,
    next_block: &BlockInfo,
    sandwich_maker: &SandwichMaker,
    fork_db: ForkDB,
) -> Result<U256, SimulationError> {
    // setup evm simulation
    let mut evm = revm::EVM::new();
    evm.database(fork_db);
    setup_block_state(&mut evm, &next_block);

    let searcher = dotenv::get_searcher_wallet().address();
    let sandwich_contract = dotenv::get_sandwich_contract_address();
    let pool_variant = ingredients.target_pool.pool_variant;

    let (startend_token, intermediary_token) = (ingredients.intermediary_token, ingredients.startend_token);

    let weth_balance_start = get_balance_of_evm(
        intermediary_token,
        sandwich_contract,
        next_block,
        &mut evm,
    )?;

    // by frontrun transaction
    let frontrun_in = match pool_variant {
        PoolVariant::UniswapV2 => {
            tx_builder::v2::encode_intermediary_with_dust(frontrun_in, false, startend_token)
        }
        PoolVariant::UniswapV3 => tx_builder::v3::encode_intermediary_token(frontrun_in),
    };

    // caluclate frontrun_out using encoded frontrun_in
    let frontrun_out = match pool_variant {
        PoolVariant::UniswapV2 => {
            let target_pool = ingredients.target_pool.address;
            evm.env.tx.gas_price = next_block.base_fee.into();
            evm.env.tx.gas_limit = 700000;
            evm.env.tx.value = rU256::ZERO;
            let amount_out =
                get_amount_out_evm(
                    frontrun_in,
                    target_pool,
                    startend_token, 
                    intermediary_token,
                    &mut evm
                )?;
            tx_builder::v2::encode_weth(amount_out)
        }
        PoolVariant::UniswapV3 => U256::zero(),
    };

    let (frontrun_data, frontrun_value) = match pool_variant {
        PoolVariant::UniswapV2 => sandwich_maker.v2.create_payload_weth_is_output(
            frontrun_in,
            frontrun_out,
            startend_token,
            ingredients.target_pool,
        ),
        PoolVariant::UniswapV3 => (
            sandwich_maker.v3.create_payload_weth_is_output(
                frontrun_in.as_u128().into(),
                startend_token,
                intermediary_token,
                ingredients.target_pool,
            ),
            U256::zero(),
        ),
    };

    // setup evm for frontrun transaction
    evm.env.tx.caller = searcher.0.into();
    evm.env.tx.transact_to = TransactTo::Call(sandwich_contract.0.into());
    evm.env.tx.data = frontrun_data.clone().into();
    evm.env.tx.value = frontrun_value.into();
    evm.env.tx.gas_limit = 700000;
    evm.env.tx.gas_price = next_block.base_fee.into();

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

    let weth_balance_end = get_balance_of_evm(
        intermediary_token,
        sandwich_contract,
        next_block,
        &mut evm,
    )?;

    let amount_in = weth_balance_end.checked_sub(weth_balance_start).unwrap_or_default();

    Ok(amount_in)
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
    // calculate backrun_amount_in
    let intermediary_increase: U256 = prepare_backrun_amount_in(
        frontrun_in,
        ingredients,
        next_block,
        sandwich_maker,
        fork_db.clone())?;
    
    if intermediary_increase.is_zero() {
        return Err(SimulationError::ZeroOptimal())
    }

    // amount of weth increase
    let min_revenue_threshold = U256::from(10000);
    let max_backrun_in = intermediary_increase.checked_sub(min_revenue_threshold).unwrap_or_default();
    // min_backrun_in is 75%
    let min_backrun_in = intermediary_increase.mul(U256::from(75)).div(U256::from(100));

    let mut revenue = U256::zero();
    let mut last_amount_in = max_backrun_in.clone();
    let mut is_last_too_many = false;
    let mut current_round = 1;
    let mut low_amount_in = min_backrun_in.clone();
    // let mut high_amount_in = max_backrun_in.clone().mul(U256::from(101)).div(U256::from(100))
    let mut high_amount_in = max_backrun_in.clone() * 2;  // modify by wang

    let mut min_amount_in = U256::zero();
    let mut low_high_range = U256::zero();
    // let mut max_other_balance = U256::zero();
    let mut max_backrun_out = U256::zero();
    let mut backrun_in_at_max = U256::zero();

    let (startend_token, intermediary_token) = (ingredients.intermediary_token, ingredients.startend_token);
 
    // loop for find optimal sandwich
    loop {
        let (can_continue, current_amount_in) = calculate_weth_input_amount(
            low_amount_in,
            high_amount_in,
            last_amount_in,
            is_last_too_many,
            current_round,
        );
        
        if min_amount_in == U256::zero() || (can_continue && current_amount_in < min_amount_in) {
            min_amount_in = current_amount_in;
        }
        
        if !can_continue {
            revenue = U256::zero();
            break;
        }

        // setup evm simulation
        let mut evm = revm::EVM::new();
        evm.database(fork_db.clone());
        setup_block_state(&mut evm, &next_block);

        let searcher = dotenv::get_searcher_wallet().address();
        let sandwich_contract = dotenv::get_sandwich_contract_address();
        let pool_variant = ingredients.target_pool.pool_variant;

        #[cfg(test)]
        {
            println!("001:startend_token={:?},intermediary_token={:?},frontrun_in={:?}", startend_token, intermediary_token, frontrun_in);
        }

        let sandwich_start_weth_balance = get_balance_of_evm(
            intermediary_token,
            sandwich_contract,
            next_block,
            &mut evm,
        )?;

        let sandwich_start_other_balance = get_balance_of_evm(
            startend_token, 
            sandwich_contract, 
            next_block, 
            &mut evm)?;

        // *´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
        // *                    FRONTRUN TRANSACTION                    */
        // *.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
        //
        // encode frontrun_in before passing to sandwich contract
        let frontrun_in = match pool_variant {
            PoolVariant::UniswapV2 => {
                tx_builder::v2::encode_intermediary_with_dust(frontrun_in, false, startend_token)
            }
            PoolVariant::UniswapV3 => tx_builder::v3::encode_intermediary_token(frontrun_in),
        };

        // caluclate frontrun_out using encoded frontrun_in
        let frontrun_out = match pool_variant {
            PoolVariant::UniswapV2 => {
                let target_pool = ingredients.target_pool.address;
                evm.env.tx.gas_price = next_block.base_fee.into();
                evm.env.tx.gas_limit = 700000;
                evm.env.tx.value = rU256::ZERO;
                let amount_out =
                    get_amount_out_evm(
                        frontrun_in,
                        target_pool,
                        startend_token, 
                        intermediary_token,
                        &mut evm
                    )?;
                tx_builder::v2::encode_weth(amount_out)
            }
            PoolVariant::UniswapV3 => U256::zero(),
        };

        #[cfg(test)]
        {
            println!("002:sandwich_start_weth_balance={:?}, sandwich_start_other_balance={:?}, frontrun_in={:?}, frontrun_out={:?}",
                sandwich_start_weth_balance, sandwich_start_other_balance, frontrun_in, frontrun_out);
        }

        // create tx.data and tx.value for backrun_in
        let (frontrun_data, frontrun_value) = match pool_variant {
            PoolVariant::UniswapV2 => sandwich_maker.v2.create_payload_weth_is_output(
                frontrun_in,
                frontrun_out,
                startend_token,
                ingredients.target_pool,
            ),
            PoolVariant::UniswapV3 => (
                sandwich_maker.v3.create_payload_weth_is_output(
                    frontrun_in.as_u128().into(),
                    startend_token,
                    intermediary_token,
                    ingredients.target_pool,
                ),
                U256::zero(),
            ),
        };

        // setup evm for frontrun transaction
        evm.env.tx.caller = searcher.0.into();
        evm.env.tx.transact_to = TransactTo::Call(sandwich_contract.0.into());
        evm.env.tx.data = frontrun_data.clone().into();
        evm.env.tx.value = frontrun_value.into();
        evm.env.tx.gas_limit = 700000;
        evm.env.tx.gas_price = next_block.base_fee.into();

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

        #[cfg(test)]
        {
            println!("003:gas={:?}", frontrun_gas_used);
        }

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
        
        let backrun_in = current_amount_in;
        let backrun_in = match pool_variant {
            PoolVariant::UniswapV2 => tx_builder::v2::encode_weth(backrun_in),
            PoolVariant::UniswapV3 => tx_builder::v3::encode_weth(backrun_in),
        };

        // caluclate backrun_out using encoded backrun_in
        let backrun_out = match pool_variant {
            PoolVariant::UniswapV2 => {
                let target_pool = ingredients.target_pool.address;
                evm.env.tx.gas_price = next_block.base_fee.into();
                evm.env.tx.gas_limit = 700000;
                evm.env.tx.value = rU256::ZERO;
                let amount_out = get_amount_out_evm(
                    backrun_in,
                    target_pool,
                    intermediary_token,
                    startend_token,
                    &mut evm,
                )?;
                // tx_builder::v2::encode_weth(amount_out)
                tx_builder::v2::decode_intermediary(amount_out, true, startend_token)
            }
            PoolVariant::UniswapV3 => U256::zero(),
        };

        if backrun_out > max_backrun_out {
            max_backrun_out = backrun_out;
            backrun_in_at_max = backrun_in;
        }

        #[cfg(test)]
        {
            let sandwich_backrun_weth_balance = get_balance_of_evm(
                intermediary_token,
                sandwich_contract,
                next_block,
                &mut evm,
            )?;
            let sandwich_backrun_other_balance = get_balance_of_evm(
                startend_token,
                sandwich_contract,
                next_block,
                &mut evm,
            )?;
            println!("004:backrun_weth_balance={:?},backrun_other_balance={:?},max_backrun_out={:?},
                backrun_in_at_max={:?}, max_backrun_in={:?},round={:?},backrun_in={:?},backrun_out={:?}",
                sandwich_backrun_weth_balance, sandwich_backrun_other_balance, max_backrun_out,
                backrun_in_at_max, max_backrun_in, current_round, backrun_in, backrun_out);
        }

        // create tx.data and tx.value for frontrun_in
        let (backrun_data, backrun_value) = match pool_variant {
            PoolVariant::UniswapV2 => sandwich_maker.v2.create_payload_weth_is_input(
                backrun_in.into(),
                backrun_out.into(),
                // U256::zero(),
                // U256::zero(),
                startend_token,
                ingredients.target_pool,
                next_block.number,
            ),
            PoolVariant::UniswapV3 => sandwich_maker.v3.create_payload_weth_is_input(
                backrun_out.as_u128().into(),
                intermediary_token,
                startend_token,
                ingredients.target_pool,
                next_block.number,
            ),
        };

        println!("backrun_data={:X?}", backrun_data.clone());
        println!("backrun_value={:?}", backrun_value);

        // setup evm for backrun transaction
        evm.env.tx.caller = searcher.0.into();
        evm.env.tx.transact_to = TransactTo::Call(sandwich_contract.0.into());
        evm.env.tx.data = backrun_data.clone().into();
        evm.env.tx.value = U256::zero().into();
        evm.env.tx.gas_limit = 700000;
        evm.env.tx.gas_price = next_block.base_fee.into();
        evm.env.tx.access_list = Vec::default();

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
            Err(e) => {
                #[cfg(test)]
                {
                    println!("EVMError:error={:?}", e);
                }
                return Err(SimulationError::BackrunEvmError(e))
            }
        };
        let mut has_error = false;
        match backrun_result {
            ExecutionResult::Success { .. } => { /* continue */ }
            ExecutionResult::Revert { output, .. } => {
                #[cfg(test)]
                {
                    println!("ExecutionResult::Revert:output={:?}", output);
                }
                has_error = true;
                // return Err(SimulationError::BackrunReverted(output))
            }
            ExecutionResult::Halt { reason, .. } => {
                #[cfg(test)]
                {
                    println!("ExecutionResult::Halt:output={:?}", reason);
                }
                has_error = true;
                // return Err(SimulationError::BackrunHalted(reason))
            }
        };
        match salmonella_inspector.is_sando_safu() {
            IsSandoSafu::Safu => { /* continue operation */ }
            IsSandoSafu::NotSafu(not_safu_opcodes) => {
                #[cfg(test)]
                {
                    println!("IsSandoSafu::NotSafu:not_safu_opcodes={:?}", not_safu_opcodes);
                }
                has_error = true;
                // return Err(SimulationError::BackrunNotSafu(not_safu_opcodes))
            }
        }

        // let backrun_gas_used = backrun_result.gas_used();
        let backrun_gas_used = 10000;
        #[cfg(test)]
        {
            println!("005:backrun_gas_used={:?}", backrun_gas_used);
        }

        let sandwich_final_other_balance = get_balance_of_evm(
            startend_token,
            sandwich_contract,
            next_block,
            &mut evm,
        )?;
        let sandwich_final_weth_balance = get_balance_of_evm(
            intermediary_token,
            sandwich_contract,
            next_block,
            &mut evm,
        )?;

        low_high_range = high_amount_in - low_amount_in;
        revenue = sandwich_final_weth_balance.checked_sub(sandwich_start_weth_balance).unwrap_or_default();

        last_amount_in = current_amount_in.clone();
        current_round = current_round + 1;

        let mut should_report = false;
        if sandwich_final_other_balance == sandwich_start_other_balance
            || low_high_range <= U256::from(100000) {
            should_report = true;
        } else if has_error || sandwich_final_other_balance > sandwich_start_other_balance {
            // buy more, reduce weth input and retry
            is_last_too_many = true;
            high_amount_in = last_amount_in;
            continue;
        } else {
            // by less, increase weth input and retry
            is_last_too_many = false;
            low_amount_in = last_amount_in;
            continue;
        }

        should_report = true;
        if should_report && !has_error{

            #[cfg(test)]
            {
                println!("final: final_weth_balance={:?}, final_other_balance={:?}, start_weth_balance={:?}, 
                    revenue={:?}, round={:?}, low={:?}, high={:?}, range={:?}",
                    sandwich_final_weth_balance, sandwich_final_other_balance, sandwich_start_weth_balance, revenue,
                    current_round, low_amount_in, high_amount_in, low_high_range);
            }

            // *´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
            // *                      GENERATE REPORTS                      */
            // *.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/

            if !revenue.is_zero() {
                // filter only passing meat txs
                let good_meats_only = ingredients
                    .meats
                    .iter()
                    .zip(is_meat_good.iter())
                    .filter(|&(_, &b)| b)
                    .map(|(s, _)| s.to_owned())
                    .collect();

                return Ok(OptimalRecipe::new(
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
                ));
            }
        }
    }
    return Err(SimulationError::ZeroOptimal());
}


fn calculate_weth_input_amount(low_amount_in: U256, high_amount_in: U256, last_amount_in: U256, is_last_too_many: bool, current_round: i32)
    -> (bool, U256) {
    if current_round == 1 {
        return (true, high_amount_in - 50000)
    } else if current_round > 10 {
        return (false, U256::zero())
    }

    if low_amount_in >= high_amount_in {
        return (false, U256::zero())
    }

    if is_last_too_many {
        // reduce weth input amount
        if high_amount_in - low_amount_in == U256::from(1) {
            return (true, last_amount_in - 1)
        } else {
            println!("last_amount_in={:?},low={:?},high={:?}", last_amount_in, low_amount_in, high_amount_in);
            let half_range = (high_amount_in - low_amount_in) / 2;
            if last_amount_in > half_range {
                return (true, last_amount_in - half_range)
            } else {
                return (false, U256::zero())
            }
        }
    } else {
        if current_round == 2 {
            return (false, U256::zero())
        } else {
            // increase weth input amount
            if high_amount_in - low_amount_in == U256::from(1) {
                return (true, last_amount_in + 1)
            } else {
                return (true, last_amount_in - (high_amount_in - low_amount_in) / 2)
            }
        }
    }
}