use anyhow::{anyhow, Result};
use cfmms::pool::Pool::{UniswapV2, UniswapV3};
use ethers::{abi, types::U256};
use foundry_evm::{
    executor::{fork::SharedBackend, Bytecode, ExecutionResult, Output, TransactTo},
    revm::{
        db::CacheDB,
        primitives::{keccak256, AccountInfo, Address as rAddress, U256 as rU256},
        EVM,
    },
};

use crate::{
    constants::{
        LIL_ROUTER_ADDRESS, LIL_ROUTER_CODE, LIL_ROUTER_CONTROLLER, WETH_ADDRESS, WETH_FUND_AMT,
        LIL_ROUTER_WETH_AMT_BASE, LIL_ROUTER_OTHER_AMT_BASE, MIN_REVENUE_THRESHOLD
    },
    tx_utils::lil_router_interface::{
        build_swap_v2_data, build_swap_v3_data, decode_swap_v2_result, decode_swap_v3_result,
    },
    types::{BlockInfo, RawIngredients},
};

use super::{eth_to_wei, setup_block_state};

// Juiced implementation of https://research.ijcaonline.org/volume65/number14/pxc3886165.pdf
// splits range in more intervals, search intervals concurrently, compare, repeat till termination
pub async fn find_optimal_input_reverse(
    ingredients: &RawIngredients,
    target_block: &BlockInfo,
    weth_inventory: U256,
    shared_backend: SharedBackend,
) -> Result<U256> {
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

    // setup values for search termination
    let base = U256::from(1000000u64);
    let tolerance = U256::from(1u64);

    let mut lower_bound = U256::zero();
    let mut upper_bound = weth_inventory;

    let tolerance = (tolerance * ((upper_bound + lower_bound) / rU256::from(2))) / base;

    // initialize variables for search
    let l_interval_lower = |i: usize, intervals: &Vec<U256>| intervals[i - 1].clone() + 1;
    let r_interval_upper = |i: usize, intervals: &Vec<U256>| {
        intervals[i + 1]
            .clone()
            .checked_sub(1.into())
            .ok_or(anyhow!("r_interval - 1 underflowed"))
    };
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
            let diff = upper_bound
                .checked_sub(lower_bound)
                .ok_or(anyhow!("upper_bound - lower_bound resulted in underflow"))?;

            let fraction = diff * i;
            let divisor = U256::from(number_of_intervals);
            let interval = lower_bound + (fraction / divisor);

            intervals.push(interval);
        }

        // calculate revenue at each interval concurrently
        let mut revenues = Vec::new();
        for bound in &intervals {
            let sim = tokio::task::spawn(evaluate_sandwich_revenue(
                *bound,
                target_block.clone(),
                shared_backend.clone(),
                ingredients.clone(),
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
            upper_bound = intervals[intervals.len() / 3]
                .checked_sub(1.into())
                .ok_or(anyhow!("intervals[intervals.len()/3] - 1 underflowed"))?;
            continue;
        }

        // if highest revenue is produced at last interval (upper bound stays fixed)
        if highest_revenue_index == intervals.len() - 1 {
            lower_bound = l_interval_lower(highest_revenue_index, &intervals);
            continue;
        }

        // if highest revenue is produced at first interval (lower bound stays fixed)
        if highest_revenue_index == 0 {
            upper_bound = r_interval_upper(highest_revenue_index, &intervals)?;
            continue;
        }

        // set bounds to intervals adjacent to highest revenue index and search again
        lower_bound = l_interval_lower(highest_revenue_index, &intervals);
        upper_bound = r_interval_upper(highest_revenue_index, &intervals)?;
    }

    Ok(highest_sando_input)
}

async fn pre_evalute_for_backrun_in(
    frontrun_in: U256,
    next_block: BlockInfo,
    shared_backend: SharedBackend,
    ingredients: &mut RawIngredients,
) -> Result<U256> {
    let mut fork_db = CacheDB::new(shared_backend);
    inject_lil_router_code(&mut fork_db, ingredients);

    let mut evm = EVM::new();
    evm.database(fork_db);
    setup_block_state(&mut evm, &next_block);

    /*´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
    /*                     HEAD TRANSACTION/s                     */
    /*.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
    for head_tx in ingredients.get_head_txs_ref().iter() {
        evm.env.tx.caller = rAddress::from_slice(&head_tx.from.0);
        evm.env.tx.transact_to =
            TransactTo::Call(rAddress::from_slice(&head_tx.to.unwrap_or_default().0));
        evm.env.tx.data = head_tx.input.0.clone();
        evm.env.tx.value = head_tx.value.into();
        evm.env.tx.chain_id = head_tx.chain_id.map(|id| id.as_u64());
        // evm.env.tx.nonce = Some(meat.nonce.as_u64()); /** ignore nonce check for now **/
        evm.env.tx.gas_limit = head_tx.gas.as_u64();
        match head_tx.transaction_type {
            Some(ethers::types::U64([0])) => {
                // legacy tx
                evm.env.tx.gas_price = head_tx.gas_price.unwrap_or_default().into();
            }
            Some(_) => {
                // type 2 tx
                evm.env.tx.gas_priority_fee =
                    head_tx.max_priority_fee_per_gas.map(|mpf| mpf.into());
                evm.env.tx.gas_price = head_tx.max_fee_per_gas.unwrap_or_default().into();
            }
            None => {
                // legacy tx
                evm.env.tx.gas_price = head_tx.gas_price.unwrap_or_default().into();
            }
        }

        let _res = evm.transact_commit();
    }

    /*´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
    /*                    FRONTRUN TRANSACTION                    */
    /*.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
    let frontrun_data = match ingredients.get_target_pool() {
        // frontrun and backrun should be swapped
        UniswapV2(pool) => build_swap_v2_data(frontrun_in, pool, false),
        UniswapV3(pool) => build_swap_v3_data(frontrun_in.as_u128().into(), pool, false),
    };

    evm.env.tx.caller = *LIL_ROUTER_CONTROLLER;
    evm.env.tx.transact_to = TransactTo::Call(*LIL_ROUTER_ADDRESS);
    evm.env.tx.data = frontrun_data.0;
    evm.env.tx.gas_limit = 700000;
    evm.env.tx.gas_price = next_block.base_fee_per_gas.into();
    evm.env.tx.value = rU256::ZERO;

    let result = match evm.transact_commit() {
        Ok(result) => result,
        Err(e) => return Err(anyhow!("[lilRouter: EVM ERROR] frontrun: {:?}", e)),
    };
    let output = match result {
        ExecutionResult::Success { output, .. } => match output {
            Output::Call(o) => o,
            Output::Create(o, _) => o,
        },
        ExecutionResult::Revert { output, .. } => {
            return Err(anyhow!("[lilRouter: REVERT] frontrun: {:?}", output))
        }
        ExecutionResult::Halt { reason, .. } => {
            return Err(anyhow!("[lilRouter: HALT] frontrun: {:?}", reason))
        }
    };
    let (_frontrun_out, intermediary_balance) = match ingredients.get_target_pool() {
        UniswapV2(_) => match decode_swap_v2_result(output.into()) {
            Ok(output) => output,
            Err(e) => {
                return Err(anyhow!(
                    "[lilRouter: FailedToDecodeOutput] frontrun: {:?}",
                    e
                ))
            }
        },
        UniswapV3(_) => match decode_swap_v3_result(output.into()) {
            Ok(output) => output,
            Err(e) => return Err(anyhow!("lilRouter: FailedToDecodeOutput: {:?}", e)),
        },
    };

    Ok(intermediary_balance)
}

async fn evaluate_sandwich_revenue(
    frontrun_in: U256,
    next_block: BlockInfo,
    shared_backend: SharedBackend,
    ingredients: RawIngredients,
) -> Result<U256> {

    // evalute to get back_in firstly
    let ingredients_result = &mut ingredients.clone();
    let intermediary_balance = pre_evalute_for_backrun_in(
        frontrun_in,
        next_block,
        shared_backend.clone(),
        ingredients_result).await?;
    if intermediary_balance.is_zero() {
        return Err(anyhow!("[lilRouter: HALT] ZeroOptimal: {:?}", "intermediary_balance=0"));
    }

    let (startend_token, _intermediary_token) = (ingredients.get_start_end_token(), ingredients.get_intermediary_token());
    let credit_helper_ref = ingredients.get_credit_helper_ref();
    let other_start_balance = credit_helper_ref.base_to_amount(
        startend_token, &(LIL_ROUTER_OTHER_AMT_BASE.to_string()));

    // amount of weth increase
    let weth_start_balance = U256::from(eth_to_wei(LIL_ROUTER_WETH_AMT_BASE));
    let intermediary_increase = intermediary_balance.checked_sub(weth_start_balance).unwrap_or_default();
    let max_backrun_in = intermediary_increase.checked_sub(*MIN_REVENUE_THRESHOLD).unwrap_or_default();
    // min_backrun_in is 75%
    let min_backrun_in = intermediary_increase.checked_mul(U256::from(75)).unwrap().checked_div(U256::from(100)).unwrap();

    let mut revenue = U256::zero();
    let mut last_amount_in = max_backrun_in.clone();
    let mut is_last_too_many = false;
    let mut current_round = 1;
    let mut low_amount_in = min_backrun_in.clone();
    let mut high_amount_in = max_backrun_in.clone();

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

        let mut fork_db = CacheDB::new(shared_backend.clone());

        inject_lil_router_code(&mut fork_db, ingredients_result);

        let mut evm = EVM::new();
        evm.database(fork_db);
        setup_block_state(&mut evm, &next_block);

        /*´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
        /*                     HEAD TRANSACTION/s                     */
        /*.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
        for head_tx in ingredients.get_head_txs_ref().iter() {
            evm.env.tx.caller = rAddress::from_slice(&head_tx.from.0);
            evm.env.tx.transact_to =
                TransactTo::Call(rAddress::from_slice(&head_tx.to.unwrap_or_default().0));
            evm.env.tx.data = head_tx.input.0.clone();
            evm.env.tx.value = head_tx.value.into();
            evm.env.tx.chain_id = head_tx.chain_id.map(|id| id.as_u64());
            // evm.env.tx.nonce = Some(meat.nonce.as_u64()); /** ignore nonce check for now **/
            evm.env.tx.gas_limit = head_tx.gas.as_u64();
            match head_tx.transaction_type {
                Some(ethers::types::U64([0])) => {
                    // legacy tx
                    evm.env.tx.gas_price = head_tx.gas_price.unwrap_or_default().into();
                }
                Some(_) => {
                    // type 2 tx
                    evm.env.tx.gas_priority_fee =
                        head_tx.max_priority_fee_per_gas.map(|mpf| mpf.into());
                    evm.env.tx.gas_price = head_tx.max_fee_per_gas.unwrap_or_default().into();
                }
                None => {
                    // legacy tx
                    evm.env.tx.gas_price = head_tx.gas_price.unwrap_or_default().into();
                }
            }

            let _res = evm.transact_commit();
        }

        /*´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
        /*                    FRONTRUN TRANSACTION                    */
        /*.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
        let frontrun_data = match ingredients.get_target_pool() {
            UniswapV2(pool) => build_swap_v2_data(frontrun_in, pool, false),
            UniswapV3(pool) => build_swap_v3_data(frontrun_in.as_u128().into(), pool, false),
        };

        evm.env.tx.caller = *LIL_ROUTER_CONTROLLER;
        evm.env.tx.transact_to = TransactTo::Call(*LIL_ROUTER_ADDRESS);
        evm.env.tx.data = frontrun_data.0;
        evm.env.tx.gas_limit = 700000;
        evm.env.tx.gas_price = next_block.base_fee_per_gas.into();
        evm.env.tx.value = rU256::ZERO;

        let result = match evm.transact_commit() {
            Ok(result) => result,
            Err(e) => return Err(anyhow!("[lilRouter: EVM ERROR] frontrun: {:?}", e)),
        };
        let output = match result {
            ExecutionResult::Success { output, .. } => match output {
                Output::Call(o) => o,
                Output::Create(o, _) => o,
            },
            ExecutionResult::Revert { output, .. } => {
                return Err(anyhow!("[lilRouter: REVERT] frontrun: {:?}", output))
            }
            ExecutionResult::Halt { reason, .. } => {
                return Err(anyhow!("[lilRouter: HALT] frontrun: {:?}", reason))
            }
        };
        let (_frontrun_out, _intermediary_balance) = match ingredients.get_target_pool() {
            UniswapV2(_) => match decode_swap_v2_result(output.into()) {
                Ok(output) => output,
                Err(e) => {
                    return Err(anyhow!(
                        "[lilRouter: FailedToDecodeOutput] frontrun: {:?}",
                        e
                    ))
                }
            },
            UniswapV3(_) => match decode_swap_v3_result(output.into()) {
                Ok(output) => output,
                Err(e) => return Err(anyhow!("lilRouter: FailedToDecodeOutput: {:?}", e)),
            },
        };

        /*´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
        /*                     MEAT TRANSACTION/s                     */
        /*.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
        for meat in ingredients.get_meats_ref().iter() {
            evm.env.tx.caller = rAddress::from_slice(&meat.from.0);
            evm.env.tx.transact_to =
                TransactTo::Call(rAddress::from_slice(&meat.to.unwrap_or_default().0));
            evm.env.tx.data = meat.input.0.clone();
            evm.env.tx.value = meat.value.into();
            evm.env.tx.chain_id = meat.chain_id.map(|id| id.as_u64());
            // evm.env.tx.nonce = Some(meat.nonce.as_u64()); /** ignore nonce check for now **/
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
        let backrun_data = match ingredients.get_target_pool() {
            UniswapV2(pool) => build_swap_v2_data(current_amount_in, pool, true),
            UniswapV3(pool) => build_swap_v3_data(current_amount_in.as_u128().into(), pool, true),
        };

        evm.env.tx.caller = *LIL_ROUTER_CONTROLLER;
        evm.env.tx.transact_to = TransactTo::Call(*LIL_ROUTER_ADDRESS);
        evm.env.tx.data = backrun_data.0;
        evm.env.tx.gas_limit = 700000;
        evm.env.tx.gas_price = next_block.base_fee_per_gas.into();
        evm.env.tx.value = rU256::ZERO;

        let result = match evm.transact_commit() {
            Ok(result) => result,
            Err(e) => return Err(anyhow!("[lilRouter: EVM ERROR] backrun: {:?}", e)),
        };
        let output = match result {
            ExecutionResult::Success { output, .. } => match output {
                Output::Call(o) => o,
                Output::Create(o, _) => o,
            },
            ExecutionResult::Revert { output, .. } => {
                return Err(anyhow!("[lilRouter: REVERT] backrun: {:?}", output))
            }
            ExecutionResult::Halt { reason, .. } => {
                return Err(anyhow!("[lilRouter: HALT] backrun: {:?}", reason))
            }
        };
        let (_amount_other_out, post_other_balance) = match ingredients.get_target_pool() {
            UniswapV2(_) => match decode_swap_v2_result(output.into()) {
                Ok(output) => output,
                Err(e) => return Err(anyhow!("[lilRouter: FailedToDecodeOutput] {:?}", e)),
            },
            UniswapV3(_) => match decode_swap_v3_result(output.into()) {
                Ok(output) => output,
                Err(e) => return Err(anyhow!("[lilRouter: FailedToDecodeOutput] {:?}", e)),
            },
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
        if current_post_other_balance == other_start_balance
            || low_high_range <= U256::from(100000) {
            revenue = intermediary_increase.checked_sub(current_amount_in).unwrap_or_default();
            break;
        } else if current_post_other_balance > other_start_balance {
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
            startend_token, _intermediary_token, frontrun_in, intermediary_balance, min_amount_in,
            max_other_balance, low_high_range, current_round, revenue);
    }

    Ok(revenue)
}

/// Inserts custom minimal router contract into evm instance for simulations
fn inject_lil_router_code(
    db: &mut CacheDB<SharedBackend>, 
    ingredients: &mut RawIngredients,
) {
    // insert lilRouter bytecode
    let lil_router_info = AccountInfo::new(
        rU256::ZERO,
        0,
        Bytecode::new_raw((*LIL_ROUTER_CODE.0).into()),
    );
    db.insert_account_info(*LIL_ROUTER_ADDRESS, lil_router_info);

    // insert and fund lilRouter controller (so we can spoof)
    let controller_info = AccountInfo::new(*WETH_FUND_AMT, 0, Bytecode::default());
    db.insert_account_info(*LIL_ROUTER_CONTROLLER, controller_info);

    // fund lilRouter with 200 weth
    let slot = keccak256(&abi::encode(&[
        abi::Token::Address((*LIL_ROUTER_ADDRESS).into()),
        abi::Token::Uint(U256::from(3)),
    ]));

    db.insert_account_storage(
        (*WETH_ADDRESS).into(),
        slot.into(),
        eth_to_wei(LIL_ROUTER_WETH_AMT_BASE))
        .unwrap();


    // as start_end token is not WETH, credit xxxx tokens for use
    let credit_helper_ref = ingredients.get_credit_helper_ref();
    credit_helper_ref.credit_token_from_base(
        ingredients.get_start_end_token().clone(),
        db,
        (*LIL_ROUTER_ADDRESS).into(),
        &(LIL_ROUTER_OTHER_AMT_BASE.to_string()),
    );
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
            let range = (high_amount_in - low_amount_in) / 2;
            if last_amount_in > range {
                return (true, last_amount_in - range);
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
                return (true, last_amount_in + (high_amount_in - low_amount_in) / 2)
            }
        }
    }
}