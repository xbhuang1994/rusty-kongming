use anvil::eth::util::get_precompiles_for;
use anyhow::{anyhow, Result};
use cfmms::pool::Pool::{UniswapV2, UniswapV3};
use ethers::abi::Address;
use ethers::types::U256;
use foundry_evm::executor::TxEnv;
use foundry_evm::executor::{
    fork::SharedBackend, inspector::AccessListTracer, ExecutionResult, TransactTo,
};
use foundry_evm::revm::{
    db::CacheDB,
    primitives::{Address as rAddress, U256 as rU256},
    EVM,
};

use crate::helpers::access_list_to_revm;
use crate::simulator::setup_block_state;
use crate::tx_utils::huff_sando_interface::common::five_byte_encoder::FiveByteMetaData;
use crate::tx_utils::huff_sando_interface::common::limit_block_height;
use crate::tx_utils::huff_sando_interface::{
    v2::{v2_create_frontrun_payload_multi,v2_create_backrun_payload_multi},
    v3::{v3_create_backrun_payload_multi, v3_create_frontrun_payload_multi},
};
use crate::types::{BlockInfo, RawIngredients, SandoRecipe};

use super::{
    salmonella_inspector::{IsSandoSafu, SalmonellaInspectoooor},
    huff_helper::{get_erc20_balance, v2_get_amount_out, inject_huff_sando},
    binary_search_weth_input,
};

use crate::constants::MIN_REVENUE_THRESHOLD;

fn pre_get_intermediary_balance(
    ingredients: &RawIngredients,
    next_block: &BlockInfo,
    optimal_in: U256,
    sando_start_bal: U256,
    searcher: Address,
    sando_address: Address,
    shared_backend: SharedBackend,
) -> Result<U256> {

    #[allow(unused_mut)]
    let mut fork_db = CacheDB::new(shared_backend);

    #[cfg(feature = "debug")]
    {
        inject_huff_sando(
            &mut fork_db,
            sando_address.0.into(),
            searcher.0.into(),
            sando_start_bal,
        );

        // as start_end token is not WETH, credit xxxx tokens for use
        let credit_helper_ref = ingredients.get_credit_helper_ref();
        credit_helper_ref.credit_token_amount(
            ingredients.get_start_end_token().clone(),
            &mut fork_db,
            sando_address.0.into(),
            sando_start_bal,
        );
    }
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
                evm.env.tx.gas_priority_fee = head_tx.max_priority_fee_per_gas.map(|mpf| mpf.into());
                evm.env.tx.gas_price = head_tx.max_fee_per_gas.unwrap_or_default().into();
            }
            None => {
                // legacy tx
                evm.env.tx.gas_price = head_tx.gas_price.unwrap_or_default().into();
            }
        }

        let _res = evm.transact_commit();
    }

    // *´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
    // *                    FRONTRUN TRANSACTION                    */
    // *.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
    // encode frontrun_in before passing to sandwich contract
    // let frontrun_in = WethEncoder::decode(WethEncoder::encode(optimal_in));

    // keep some dust
    let frontrun_in = match ingredients.get_target_pool() {
        UniswapV2(_) => {
            let mut frontrun_in_encoded = FiveByteMetaData::encode(optimal_in, 1);
            frontrun_in_encoded.decrement_four_bytes();
            frontrun_in_encoded.decode()
        }
        UniswapV3(_) => {
            let frontrun_in_encoded = FiveByteMetaData::encode(optimal_in, 1);
            frontrun_in_encoded.decode()
        }
    };

    // caluclate frontrun_out using encoded frontrun_in
    let frontrun_out = match ingredients.get_target_pool() {
        UniswapV2(p) => {
            evm.env.tx.gas_price = next_block.base_fee_per_gas.into();
            evm.env.tx.gas_limit = 700000;
            evm.env.tx.value = rU256::ZERO;
            let out = v2_get_amount_out(frontrun_in, p, false, &mut evm)?;
            out
        }
        UniswapV3(_p) => U256::zero(), // we don't need to know backrun out for v3
    };

    // create tx.data and tx.value for frontrun_in
    let (frontrun_data, frontrun_value) = match ingredients.get_target_pool() {
        UniswapV2(p) => v2_create_backrun_payload_multi(
            p,
            ingredients.get_start_end_token(),
            frontrun_in,
            frontrun_out),
        UniswapV3(p) => (
            v3_create_backrun_payload_multi(
                p,
                ingredients.get_start_end_token(),
                frontrun_in),
            U256::zero(),
        ),
    };

    // setup evm for frontrun transaction
    let mut frontrun_tx_env = TxEnv {
        caller: searcher.0.into(),
        gas_limit: 700000,
        gas_price: next_block.base_fee_per_gas.into(),
        gas_priority_fee: None,
        transact_to: TransactTo::Call(sando_address.0.into()),
        value: frontrun_value.into(),
        data: limit_block_height(frontrun_data, next_block.number).into(),
        chain_id: None,
        nonce: None,
        access_list: Default::default(),
    };
    evm.env.tx = frontrun_tx_env.clone();


    // get access list
    let mut access_list_inspector = AccessListTracer::new(
        Default::default(),
        searcher,
        sando_address,
        get_precompiles_for(evm.env.cfg.spec_id),
    );
    evm.inspect_ref(&mut access_list_inspector)
        .map_err(|e| anyhow!("[EVM ERROR] frontrun: {:?}", (e)))?;
    let frontrun_access_list = access_list_inspector.access_list();

    frontrun_tx_env.access_list = access_list_to_revm(frontrun_access_list);
    evm.env.tx = frontrun_tx_env.clone();

    // run again but now with access list (so that we get accurate gas used)
    // run with a salmonella inspector to flag `suspicious` opcodes
    let mut salmonella_inspector = SalmonellaInspectoooor::new();
    let frontrun_result = match evm.inspect_commit(&mut salmonella_inspector) {
        Ok(result) => result,
        Err(e) => return Err(anyhow!("[huffsando: EVM ERROR] frontrun: {:?}", e)),
    };
    match frontrun_result {
        ExecutionResult::Success { .. } => { /* continue operation */ }
        ExecutionResult::Revert { output, .. } => {
            return Err(anyhow!("[huffsando: REVERT] frontrun: {:?}", output));
        }
        ExecutionResult::Halt { reason, .. } => {
            return Err(anyhow!("[huffsando: HALT] frontrun: {:?}", reason));
        }
    };
    match salmonella_inspector.is_sando_safu() {
        IsSandoSafu::Safu => { /* continue operation */ }
        IsSandoSafu::NotSafu(not_safu_opcodes) => {
            return Err(anyhow!(
                "[huffsando: FrontrunNotSafu] {:?}",
                not_safu_opcodes
            ))
        }
    }

    let intermediary_balance = get_erc20_balance(ingredients.get_intermediary_token(), sando_address, next_block, &mut evm)?;
    Ok(intermediary_balance)
}

/// finds if sandwich is profitable + salmonella free
pub fn create_recipe_reverse(
    ingredients: &RawIngredients,
    next_block: &BlockInfo,
    optimal_in: U256,
    sando_start_bal: U256,
    searcher: Address,
    sando_address: Address,
    shared_backend: SharedBackend,
) -> Result<SandoRecipe> {
    
    let intermediary_balance = pre_get_intermediary_balance(
        ingredients,
        &next_block.clone(),
        optimal_in,
        sando_start_bal,
        searcher,
        sando_address,
        shared_backend.clone()
    )?;
    if intermediary_balance.is_zero() {
        return Err(anyhow!("[huffsando: PRE] ZeroBalance: {:?}", "intermediary_balance=0"));
    }

    // let credit_helper_ref = ingredients.get_credit_helper_ref();
    // let other_start_balance = credit_helper_ref.base_to_amount(
    //     startend_token, &(LIL_ROUTER_OTHER_AMT_BASE.to_string()));
    let other_start_balance = sando_start_bal.clone();

    // amount of weth increase
    // let weth_start_balance = U256::from(eth_to_wei(LIL_ROUTER_WETH_AMT_BASE));
    let weth_start_balance = sando_start_bal.clone();
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
        let (can_continue, current_amount_in) = binary_search_weth_input(
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

        #[allow(unused_mut)]
        let mut fork_db = CacheDB::new(shared_backend.clone());

        #[cfg(feature = "debug")]
        {
            inject_huff_sando(
                &mut fork_db,
                sando_address.0.into(),
                searcher.0.into(),
                sando_start_bal,
            );

            // as start_end token is not WETH, credit xxxx tokens for use
            let credit_helper_ref = ingredients.get_credit_helper_ref();
            credit_helper_ref.credit_token_amount(
                ingredients.get_start_end_token().clone(),
                &mut fork_db,
                sando_address.0.into(),
                sando_start_bal,
            );
        }
        let mut evm = EVM::new();
        evm.database(fork_db);
        let next_block = next_block.clone();
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
                    evm.env.tx.gas_priority_fee = head_tx.max_priority_fee_per_gas.map(|mpf| mpf.into());
                    evm.env.tx.gas_price = head_tx.max_fee_per_gas.unwrap_or_default().into();
                }
                None => {
                    // legacy tx
                    evm.env.tx.gas_price = head_tx.gas_price.unwrap_or_default().into();
                }
            }

            let _res = evm.transact_commit();
        }

        // *´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
        // *                    FRONTRUN TRANSACTION                    */
        // *.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
        // encode frontrun_in before passing to sandwich contract
        // let frontrun_in = WethEncoder::decode(WethEncoder::encode(optimal_in));
        // keep some dust
        let frontrun_in = match ingredients.get_target_pool() {
            UniswapV2(_) => {
                let mut frontrun_in_encoded = FiveByteMetaData::encode(optimal_in, 1);
                frontrun_in_encoded.decrement_four_bytes();
                frontrun_in_encoded.decode()
            }
            UniswapV3(_) => {
                let frontrun_in_encoded = FiveByteMetaData::encode(optimal_in, 1);
                frontrun_in_encoded.decode()
            }
        };

        // caluclate frontrun_out using encoded frontrun_in
        let frontrun_out = match ingredients.get_target_pool() {
            UniswapV2(p) => {
                evm.env.tx.gas_price = next_block.base_fee_per_gas.into();
                evm.env.tx.gas_limit = 700000;
                evm.env.tx.value = rU256::ZERO;
                let out = v2_get_amount_out(frontrun_in, p, false, &mut evm)?;
                out
            }
            UniswapV3(_p) => U256::zero(), // we don't need to know backrun out for v3
        };

        // create tx.data and tx.value for frontrun_in
        let (frontrun_data, frontrun_value) = match ingredients.get_target_pool() {
            UniswapV2(p) => v2_create_backrun_payload_multi(
                p,
                ingredients.get_start_end_token(),
                frontrun_in,
                frontrun_out),
            UniswapV3(p) => (
                v3_create_backrun_payload_multi(
                    p,
                    ingredients.get_start_end_token(),
                    frontrun_in),
                U256::zero(),
            ),
        };

        // setup evm for frontrun transaction
        let mut frontrun_tx_env = TxEnv {
            caller: searcher.0.into(),
            gas_limit: 700000,
            gas_price: next_block.base_fee_per_gas.into(),
            gas_priority_fee: None,
            transact_to: TransactTo::Call(sando_address.0.into()),
            value: frontrun_value.into(),
            data: limit_block_height(frontrun_data, next_block.number).into(),
            chain_id: None,
            nonce: None,
            access_list: Default::default(),
        };
        evm.env.tx = frontrun_tx_env.clone();

        // get access list
        let mut access_list_inspector = AccessListTracer::new(
            Default::default(),
            searcher,
            sando_address,
            get_precompiles_for(evm.env.cfg.spec_id),
        );
        evm.inspect_ref(&mut access_list_inspector)
            .map_err(|e| anyhow!("[EVM ERROR] frontrun: {:?}", (e)))?;
        let frontrun_access_list = access_list_inspector.access_list();

        frontrun_tx_env.access_list = access_list_to_revm(frontrun_access_list);
        evm.env.tx = frontrun_tx_env.clone();

        // run again but now with access list (so that we get accurate gas used)
        // run with a salmonella inspector to flag `suspicious` opcodes
        let mut salmonella_inspector = SalmonellaInspectoooor::new();
        let frontrun_result = match evm.inspect_commit(&mut salmonella_inspector) {
            Ok(result) => result,
            Err(e) => return Err(anyhow!("[huffsando: EVM ERROR] frontrun: {:?}", e)),
        };
        match frontrun_result {
            ExecutionResult::Success { .. } => { /* continue operation */ }
            ExecutionResult::Revert { output, .. } => {
                return Err(anyhow!("[huffsando: REVERT] frontrun: {:?}", output));
            }
            ExecutionResult::Halt { reason, .. } => {
                return Err(anyhow!("[huffsando: HALT] frontrun: {:?}", reason));
            }
        };
        match salmonella_inspector.is_sando_safu() {
            IsSandoSafu::Safu => { /* continue operation */ }
            IsSandoSafu::NotSafu(not_safu_opcodes) => {
                return Err(anyhow!(
                    "[huffsando: FrontrunNotSafu] {:?}",
                    not_safu_opcodes
                ))
            }
        }

        let frontrun_gas_used = frontrun_result.gas_used();

        // *´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
        // *                     MEAT TRANSACTION/s                     */
        // *.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
        let mut is_meat_good = Vec::new();
        for meat in ingredients.get_meats_ref().iter() {
            evm.env.tx.caller = rAddress::from_slice(&meat.from.0);
            evm.env.tx.transact_to =
                TransactTo::Call(rAddress::from_slice(&meat.to.unwrap_or_default().0));
            evm.env.tx.data = meat.input.0.clone();
            evm.env.tx.value = meat.value.into();
            evm.env.tx.chain_id = meat.chain_id.map(|id| id.as_u64());
            //evm.env.tx.nonce = Some(meat.nonce.as_u64());
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
                Err(e) => return Err(anyhow!("[huffsando: EVM ERROR] meat: {:?}", e)),
            };
            match res.is_success() {
                true => is_meat_good.push(true),
                false => is_meat_good.push(false),
            }
        }
        // *´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
        // *                    BACKRUN TRANSACTION                     */
        // *.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
        // encode backrun_in before passing to sandwich contract
        let _backrun_token_in = ingredients.get_intermediary_token();
        let backrun_token_out = ingredients.get_start_end_token();

        let backrun_in = FiveByteMetaData::encode(current_amount_in, 0).decode();
        // caluclate backrun_out using encoded backrun_in
        let backrun_out = match ingredients.get_target_pool() {
            UniswapV2(p) => {
                let out = v2_get_amount_out(backrun_in, p, true, &mut evm)?;
                out
            }
            UniswapV3(_) => U256::zero(),
        };
    
        // create tx.data and tx.value for backrun_in
        let (backrun_data, backrun_value) = match ingredients.get_target_pool() {
            UniswapV2(p) => v2_create_frontrun_payload_multi(
                p,
                backrun_token_out,
                backrun_in,
                backrun_out
            ),
            UniswapV3(p) => v3_create_frontrun_payload_multi(
                p,
                backrun_token_out,
                backrun_in,
            ),
        };

        // setup evm for backrun transaction
        let mut backrun_tx_env = TxEnv {
            caller: searcher.0.into(),
            gas_limit: 700000,
            gas_price: next_block.base_fee_per_gas.into(),
            gas_priority_fee: None,
            transact_to: TransactTo::Call(sando_address.0.into()),
            value: backrun_value.into(),
            data: backrun_data.clone().into(),
            chain_id: None,
            nonce: None,
            access_list: Default::default(),
        };
        evm.env.tx = backrun_tx_env.clone();

        // create access list
        let mut access_list_inspector = AccessListTracer::new(
            Default::default(),
            searcher,
            sando_address,
            get_precompiles_for(evm.env.cfg.spec_id),
        );
        evm.inspect_ref(&mut access_list_inspector)
            .map_err(|e| anyhow!("[huffsando: EVM ERROR] frontrun: {:?}", e))
            .unwrap();
        let backrun_access_list = access_list_inspector.access_list();
        backrun_tx_env.access_list = access_list_to_revm(backrun_access_list);
        evm.env.tx = backrun_tx_env.clone();

        // run again but now with access list (so that we get accurate gas used)
        // run with a salmonella inspector to flag `suspicious` opcodes
        let mut salmonella_inspector = SalmonellaInspectoooor::new();
        let backrun_result = match evm.inspect_commit(&mut salmonella_inspector) {
            Ok(result) => result,
            Err(e) => return Err(anyhow!("[huffsando: EVM ERROR] backrun: {:?}", e)),
        };
        match backrun_result {
            ExecutionResult::Success { .. } => { /* continue */ }
            ExecutionResult::Revert { output, .. } => {
                return Err(anyhow!("[huffsando: REVERT] backrun: {:?}", output));
            }
            ExecutionResult::Halt { reason, .. } => {
                return Err(anyhow!("[huffsando: HALT] backrun: {:?}", reason))
            }
        };
        match salmonella_inspector.is_sando_safu() {
            IsSandoSafu::Safu => { /* continue operation */ }
            IsSandoSafu::NotSafu(not_safu_opcodes) => {
                return Err(anyhow!(
                    "[huffsando: BACKRUN_NOT_SAFU] bad_opcodes->{:?}",
                    not_safu_opcodes
                ))
            }
        }

        let backrun_gas_used = backrun_result.gas_used();

        let post_other_balance = get_erc20_balance(_backrun_token_in, sando_address, &next_block, &mut evm)?;
        let current_post_other_balance = post_other_balance.clone();
        if current_post_other_balance > max_other_balance {
            max_other_balance = post_other_balance.clone();
        }

        println!("010:current_round={:?}, low={:?}, high={:?}, can_continue={:?}, other_frontrun_in={:?} intermediary_increase={:?},
            current_amount_in={:?}, last_amount_in={:?}, other_start_balance={:?}, current_post_other_balance={:?}",
            current_round, low_amount_in, high_amount_in, can_continue, frontrun_in, intermediary_increase,
            current_amount_in, last_amount_in, other_start_balance, current_post_other_balance);

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

        if !revenue.is_zero() {
            // *´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
            // *                      GENERATE REPORTS                      */
            // *.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/

            // filter only passing meat txs
            let good_meats_only = ingredients
                .get_meats_ref()
                .iter()
                .zip(is_meat_good.iter())
                .filter(|&(_, &b)| b)
                .map(|(s, _)| s.to_owned())
                .collect();

            return Ok(SandoRecipe::new(
                ingredients.get_head_txs_ref().clone(),
                frontrun_tx_env,
                frontrun_gas_used,
                good_meats_only,
                backrun_tx_env,
                backrun_gas_used,
                revenue,
                next_block,
            ))
        }
    }

    return Err(anyhow!("[huffsando: ZeroRevene]"))
}