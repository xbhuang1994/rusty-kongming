use std::ops::{Div, Mul};

use ethers::prelude::*;
use revm::primitives::{ExecutionResult, Output, TransactTo, B160 as rAddress, U256 as rU256, ruint::Uint};

use crate::simulate::{
    attach_braindance_module, braindance_address, braindance_controller_address,
    braindance_starting_balance, setup_block_state, braindance_starting_balance_other,
};

use crate::prelude::PoolVariant;

use crate::prelude::sandwich_types::RawIngredients;
use crate::prelude::fork_factory::ForkFactory;
use crate::types::{BlockInfo, SimulationError};
use crate::prelude::fork_db::ForkDB;
use crate::utils::tx_builder::{self, braindance};

// Roided implementation of https://research.ijcaonline.org/volume65/number14/pxc3886165.pdf
// splits range in more intervals, search intervals concurrently, compare, repeat till termination
//
// Arguments:
// * `&ingredients`: holds onchain information about opportunity
// * `lower_bound`: lower bound of search interval
// * `upper_bound`: upper bound of search interval, normally equal to sandwich balance
// * `next_block`: holds information about next block
// * `fork_factory`: used to create new forked evm instances for simulations
//
// Returns:
// Ok(U256): optimal amount in, if no errors during calculation
// Err(SimulationError): if error during calculation
pub async fn juiced_quadratic_search(
    ingredients: &RawIngredients,
    mut lower_bound: U256,
    mut upper_bound: U256,
    next_block: &BlockInfo,
    mut fork_factory: &mut ForkFactory,
) -> Result<U256, SimulationError> {
    //
    //            [EXAMPLE WITH 10 BOUND INTERVALS]
    //
    //     (first)              (mid)               (last)
    //        ▼                   ▼                   ▼
    //        +---+---+---+---+---+---+---+---+---+---+
    //        |   |   |   |   |   |   |   |   |   |   |
    //        +---+---+---+---+---+---+---+---+---+---+
    //        ▲   ▲   ▲   ▲   ▲   ▲   ▲   ▲   ▲   ▲   ▲
    //        0   1   2   3   4   5   6   7   8   9   X
    //
    //  * [0, X] = search range
    //  * Find revenue at each interval
    //  * Find index of interval with highest revenue
    //  * Search again with bounds set to adjacent index of highest
    //

    attach_braindance_module(&mut fork_factory, ingredients.clone(), false);

    #[cfg(test)]
    {
        // if running test, setup contract sandwich to allow for backtest
        // can also inject new sandwich code for testing
        crate::prelude::inject_sando(
            &mut fork_factory,
            upper_bound,
            false,
            ingredients.clone());
    }

    // setup values for search termination
    let base = U256::from(1000000u64);
    let tolerance = U256::from(1u64);

    let tolerance = (tolerance * ((upper_bound + lower_bound) / 2)) / base;

    // initialize variables for search
    let left_interval_lower = |i: usize, intervals: &Vec<U256>| intervals[i - 1].clone() + 1;
    let right_interval_upper = |i: usize, intervals: &Vec<U256>| intervals[i + 1].clone() - 1;
    let should_loop_terminate = |lower_bound: U256, upper_bound: U256| -> bool {
        let search_range = match upper_bound.checked_sub(lower_bound) {
            Some(range) => range,
            None => return true,
        };
        // produces negative result
        if lower_bound > upper_bound {
            return true;
        }
        // tolerance condition not met
        if search_range < tolerance {
            return true;
        }
        false
    };
    let mut highest_sando_input = U256::zero();
    let number_of_intervals = 15;
    let mut counter = 0;

    // continue search until termination condition is met (no point seraching down to closest wei)
    loop {
        counter += 1;
        if should_loop_terminate(lower_bound, upper_bound) {
            break;
        }

        // split search range into intervals
        let mut intervals = Vec::new();
        for i in 0..=number_of_intervals {
            intervals.push(lower_bound + (((upper_bound - lower_bound) * i) / number_of_intervals));
        }

        // calculate revenue at each interval concurrently
        let mut revenues = Vec::new();
        for bound in &intervals {
            if bound.is_zero() {
                continue;
            }
            let sim = tokio::task::spawn(evaluate_sandwich_revenue(
                *bound,
                ingredients.clone(),
                next_block.clone(),
                fork_factory.new_sandbox_fork(),
            ));
            revenues.push(sim);
        }

        let revenues = futures::future::join_all(revenues).await;

        let revenues = revenues
            .into_iter()
            .map(|r| r.unwrap().unwrap_or_default())
            .collect::<Vec<_>>();

        // find interval that produces highest revenue
        let (highest_revenue_index, _highest_revenue) = revenues
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.cmp(&b))
            .unwrap();

        highest_sando_input = intervals[highest_revenue_index];

        // enhancement: find better way to increase finding opps incase of all rev=0
        if revenues[highest_revenue_index] == U256::zero() {
            // most likely there is no sandwich possibility
            if counter == 10 {
                return Ok(U256::zero());
            }
            // no revenue found, most likely small optimal so decrease range
            upper_bound = intervals[intervals.len() / 3] - 1;
            continue;
        }

        // if highest revenue is produced at last interval (upper bound stays fixed)
        if highest_revenue_index == intervals.len() - 1 {
            lower_bound = left_interval_lower(highest_revenue_index, &intervals);
            continue;
        }

        // if highest revenue is produced at first interval (lower bound stays fixed)
        if highest_revenue_index == 0 {
            upper_bound = right_interval_upper(highest_revenue_index, &intervals);
            continue;
        }

        // set bounds to intervals adjacent to highest revenue index and search again
        lower_bound = left_interval_lower(highest_revenue_index, &intervals);
        upper_bound = right_interval_upper(highest_revenue_index, &intervals);
    }

    Ok(highest_sando_input)
}

async fn evalute_sandwich_backrun_in(
    frontrun_in: U256,
    ingredients: RawIngredients,
    next_block: BlockInfo,
    fork_db: ForkDB,
) -> Result<U256, SimulationError> {

    let mut evm = revm::EVM::new();
    let fork_db = fork_db.clone();
    evm.database(fork_db);
    setup_block_state(&mut evm, &next_block);

    let pool_variant = ingredients.target_pool.pool_variant;

    let (mut startend_token, mut intermediary_token) = (ingredients.intermediary_token, ingredients.startend_token);

    /*
     by FRONTRUN TRANSACTION
     */
    let frontrun_data = match pool_variant {
        PoolVariant::UniswapV2 => braindance::build_swap_v2_data(
            frontrun_in,
            ingredients.target_pool.address,
            startend_token,
            intermediary_token,
        ),
        PoolVariant::UniswapV3 => braindance::build_swap_v3_data(
            frontrun_in.as_u128().into(),
            ingredients.target_pool.address,
            startend_token,
            intermediary_token,
        ),
    };

    evm.env.tx.caller = braindance_controller_address();
    evm.env.tx.transact_to = TransactTo::Call(braindance_address().0.into());
    evm.env.tx.data = frontrun_data.0;
    evm.env.tx.gas_limit = 700000;
    evm.env.tx.gas_price = next_block.base_fee.into();
    evm.env.tx.value = rU256::ZERO;

    let result = match evm.transact_commit() {
        Ok(result) => result,
        Err(e) => return Err(SimulationError::FrontrunEvmError(e)),
    };
    let output = match result {
        ExecutionResult::Success { output, .. } => match output {
            Output::Call(o) => o,
            Output::Create(o, _) => o,
        },
        ExecutionResult::Revert { output, .. } => {
            #[cfg(test)]
            {
                println!("ErrorRevert:{:?}", output);
            }
            return Err(SimulationError::FrontrunReverted(output))
        }
        ExecutionResult::Halt { reason, .. } => {
            #[cfg(test)]
            {
                println!("ErrorHalt:{:?}", reason);
            }
            return Err(SimulationError::FrontrunHalted(reason))
        }
    };
    let (_frontrun_out, intermediary_balance) = match pool_variant {
        PoolVariant::UniswapV2 => {
            match tx_builder::braindance::decode_swap_v2_result(output.into()) {
                Ok(output) => output,
                Err(e) => return Err(SimulationError::FailedToDecodeOutput(e)),
            }
        }
        PoolVariant::UniswapV3 => {
            match tx_builder::braindance::decode_swap_v3_result(output.into()) {
                Ok(output) => output,
                Err(e) => return Err(SimulationError::FailedToDecodeOutput(e)),
            }
        }
    };
    // #[cfg(test)]
    // {
    //     println!("evalute_sandwich_backrun_in:_frontrun_out={:?}, intermediary_balance={:?}", _frontrun_out, intermediary_balance);
    // }

    Ok(intermediary_balance)
}

/// Sandwich simulation using BrainDance contract (modified router contract)
///
/// Arguments:
/// * `frontrun_in`: amount of to frontrun with
/// * `ingredients`: ingredients of the sandwich
/// * `next_block`: block info of the next block
/// * `fork_db`: database instance used for evm simulations
async fn evaluate_sandwich_revenue(
    frontrun_in: U256,
    ingredients: RawIngredients,
    next_block: BlockInfo,
    fork_db: ForkDB,
) -> Result<U256, SimulationError> {

    // evalute to get back_in firstly
    let ingredients_result = ingredients.clone();
    let intermediary_balance = evalute_sandwich_backrun_in(
        frontrun_in, ingredients_result, next_block.clone(), fork_db.clone()).await?;
    if intermediary_balance.is_zero() {
        return Err(SimulationError::ZeroOptimal());
    }

    // amount of weth increase
    let intermediary_increase = intermediary_balance.checked_sub(braindance_starting_balance()).unwrap_or_default();
    let min_revenue_threshold = U256::from(10000);
    let max_backrun_in = intermediary_increase.checked_sub(min_revenue_threshold).unwrap_or_default();
    // min_backrun_in is 75%
    let min_backrun_in = intermediary_increase.mul(U256::from(75)).div(U256::from(100));

    let mut revenue = U256::zero();
    let mut last_amount_in = max_backrun_in.clone();
    let mut is_last_too_many = false;
    let mut current_round = 1;
    let mut low_amount_in = min_backrun_in.clone();
    // let mut high_amount_in = max_backrun_in.clone().mul(U256::from(101)).div(U256::from(100));
    let mut high_amount_in = max_backrun_in.clone();

    let (startend_token, intermediary_token) = (ingredients.intermediary_token, ingredients.startend_token);
    let mut min_amount_in = U256::zero();
    let mut low_high_range = U256::zero();
    let mut max_other_balance = U256::zero();

    loop {
        let (can_continue, current_amount_in) = calculate_weth_input_amount(
            low_amount_in,
            high_amount_in,
            last_amount_in,
            is_last_too_many,
            current_round);
        
        if min_amount_in == U256::zero() || (can_continue && current_amount_in < min_amount_in) {
            min_amount_in = current_amount_in;
        }
        
        if !can_continue {
            revenue = U256::zero();
            break;
        }

        let mut evm = revm::EVM::new();
        let fork_db = fork_db.clone();
        evm.database(fork_db);
        setup_block_state(&mut evm, &next_block);
    
        let pool_variant = ingredients.target_pool.pool_variant;
    
        /*´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
        /*                    FRONTRUN TRANSACTION                    */
        /*.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
        let frontrun_data = match pool_variant {
            PoolVariant::UniswapV2 => braindance::build_swap_v2_data(
                frontrun_in,
                ingredients.target_pool.address,
                startend_token,
                intermediary_token,
            ),
            PoolVariant::UniswapV3 => braindance::build_swap_v3_data(
                frontrun_in.as_u128().into(),
                ingredients.target_pool.address,
                startend_token,
                intermediary_token,
            ),
        };
    
        evm.env.tx.caller = braindance_controller_address();
        evm.env.tx.transact_to = TransactTo::Call(braindance_address().0.into());
        evm.env.tx.data = frontrun_data.0;
        evm.env.tx.gas_limit = 700000;
        evm.env.tx.gas_price = next_block.base_fee.into();
        evm.env.tx.value = rU256::ZERO;
    
        let result = match evm.transact_commit() {
            Ok(result) => result,
            Err(e) => return Err(SimulationError::FrontrunEvmError(e)),
        };
        let output = match result {
            ExecutionResult::Success { output, .. } => match output {
                Output::Call(o) => o,
                Output::Create(o, _) => o,
            },
            ExecutionResult::Revert { output, .. } => {
                #[cfg(test)]
                {
                    println!("ErrorRevert:{:?}", output);
                }
                return Err(SimulationError::FrontrunReverted(output))
            }
            ExecutionResult::Halt { reason, .. } => {
                #[cfg(test)]
                {
                    println!("ErrorHalt:{:?}", reason);
                }
                return Err(SimulationError::FrontrunHalted(reason))
            }
        };
        let (_frontrun_out, _intermediary_balance) = match pool_variant {
            PoolVariant::UniswapV2 => {
                match tx_builder::braindance::decode_swap_v2_result(output.into()) {
                    Ok(output) => output,
                    Err(e) => return Err(SimulationError::FailedToDecodeOutput(e)),
                }
            }
            PoolVariant::UniswapV3 => {
                match tx_builder::braindance::decode_swap_v3_result(output.into()) {
                    Ok(output) => output,
                    Err(e) => return Err(SimulationError::FailedToDecodeOutput(e)),
                }
            }
        };

        /*´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
        /*                     MEAT TRANSACTION/s                     */
        /*.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
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
                    evm.env.tx.gas_price = meat.gas_price.unwrap_or_default().into();
                }
            }

            let _res = evm.transact_commit();
        }

        /*´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
        /*                    BACKRUN TRANSACTION                     */
        /*.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
        let backrun_data = match pool_variant {
            PoolVariant::UniswapV2 => braindance::build_swap_v2_data(
                current_amount_in,
                ingredients.target_pool.address,
                intermediary_token,
                startend_token,
            ),
            PoolVariant::UniswapV3 => braindance::build_swap_v3_data(
                current_amount_in.as_u128().into(),
                ingredients.target_pool.address,
                intermediary_token,
                startend_token,
            ),
        };

        evm.env.tx.caller = braindance_controller_address();
        evm.env.tx.transact_to = TransactTo::Call(braindance_address().0.into());
        evm.env.tx.data = backrun_data.0;
        evm.env.tx.gas_limit = 700000;
        evm.env.tx.gas_price = next_block.base_fee.into();
        // evm.env.tx.gas_priority_fee = Option::from(Uint::from(710000));
        evm.env.tx.value = rU256::ZERO;
        evm.env.tx.nonce = None;

        let result = match evm.transact_commit() {
            Ok(result) => result,
            Err(e) => {
                #[cfg(test)]
                {
                    println!("EVMError:{:?}", e);
                }
                return Err(SimulationError::BackrunEvmError(e));
            },
        };
        let output = match result {
            ExecutionResult::Success { output, .. } => match output {
                Output::Call(o) => o,
                Output::Create(o, _) => o,
            },
            ExecutionResult::Revert { output, .. } => {
                #[cfg(test)]
                {
                    println!("Revert:{:X?}", output);
                }
                return Err(SimulationError::BackrunReverted(output))
            }
            ExecutionResult::Halt { reason, .. } => {
                #[cfg(test)]
                {
                    println!("Halt:{:?}", reason);
                }
                return Err(SimulationError::BackrunHalted(reason));
            },
        };
        let (_amount_other_out, post_other_balance) = match pool_variant {
            PoolVariant::UniswapV2 => {
                match tx_builder::braindance::decode_swap_v2_result(output.into()) {
                    Ok(output) => output,
                    Err(e) => {
                        #[cfg(test)]
                        {
                            println!("Err:{:?}", e);
                        }
                        return Err(SimulationError::FailedToDecodeOutput(e));
                    },
                }
            }
            PoolVariant::UniswapV3 => {
                match tx_builder::braindance::decode_swap_v3_result(output.into()) {
                    Ok(output) => output,
                    Err(e) => {
                        #[cfg(test)]
                        {
                            println!("Err:{:?}", e);
                        }
                        return Err(SimulationError::FailedToDecodeOutput(e));
                    },
                }
            }
        };

        let current_post_other_balance = post_other_balance.clone();
        if current_post_other_balance > max_other_balance {
            max_other_balance = post_other_balance.clone();
        }

        // println!("010:current_round={:?}, low={:?}, high={:?}, can_continue={:?}, intermediary_increase={:?},
        //     current_amount_in={:?}, last_amount_in={:?}, _amount_other_out={:?}, current_post_other_balance={:?}",
        //     current_round, low_amount_in, high_amount_in, can_continue, intermediary_increase,
        //     current_amount_in, last_amount_in, amount_other_out, current_post_other_balance);

        last_amount_in = current_amount_in.clone();
        current_round = current_round + 1;
        low_high_range = high_amount_in - low_amount_in;
        if current_post_other_balance == braindance_starting_balance_other()
            || low_high_range <= U256::from(100000) {
            revenue = intermediary_increase.checked_sub(current_amount_in).unwrap_or_default();
            break;
        } else if current_post_other_balance > braindance_starting_balance_other() {
            // buy more, reduce weth input and retry
            is_last_too_many = true;
            high_amount_in = last_amount_in
        } else {
            // by less, increase weth input and retry
            is_last_too_many = false;
            low_amount_in = last_amount_in

        }
    }

    #[cfg(test)]
    {
        println!("started_token={:?},intermediary_token={:?},frontrun_in={:?},intermediary_balance={:?},
            min_mount_in={:?},max_other_balance={:?},low_high_range={:?},round={:?},revenue={:?}",
            startend_token, intermediary_token, frontrun_in, intermediary_balance, min_amount_in,
            max_other_balance, low_high_range, current_round, revenue);
    }

    Ok(revenue)
}

fn calculate_weth_input_amount(low_amount_in: U256, high_amount_in: U256, last_amount_in: U256, is_last_too_many: bool, current_round: i32)
    -> (bool, U256) {
    if current_round == 1 {
        return (true, high_amount_in)
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
            return (true, last_amount_in - (high_amount_in - low_amount_in) / 2)
        }
    } else {
        if current_round == 2 {
            return (false, U256::zero())
        } else {
            // increase weth input amount
            if high_amount_in - low_amount_in == U256::from(1) {
                return (true, last_amount_in + 1)
            } else {
                return (true, last_amount_in + (high_amount_in - low_amount_in) / 2)
            }
        }
    }
}