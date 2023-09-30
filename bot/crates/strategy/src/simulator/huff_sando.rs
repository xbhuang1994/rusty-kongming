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
use crate::types::{BlockInfo, RawIngredients, SandoRecipe, SandwichSwapType};

use super::salmonella_inspector::{IsSandoSafu, SalmonellaInspectoooor};

use super::huff_helper::{get_erc20_balance, v2_get_amount_out, inject_huff_sando};

/// finds if sandwich is profitable + salmonella free
pub fn create_recipe(
    ingredients: &RawIngredients,
    next_block: &BlockInfo,
    optimal_in: U256,
    sando_start_bal: U256,
    searcher: Address,
    sando_address: Address,
    shared_backend: SharedBackend,
) -> Result<SandoRecipe> {

    if optimal_in.is_zero() {
        return Err(anyhow!("[huffsando: ZeroOtimal]"))
    }

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
    let frontrun_in = FiveByteMetaData::encode(optimal_in, 0).decode();
    // caluclate frontrun_out using encoded frontrun_in
    let frontrun_out = match ingredients.get_target_pool() {
        UniswapV2(p) => {
            evm.env.tx.gas_price = next_block.base_fee_per_gas.into();
            evm.env.tx.gas_limit = 700000;
            evm.env.tx.value = rU256::ZERO;
            v2_get_amount_out(frontrun_in, p, true, &mut evm)?
        }
        UniswapV3(_) => U256::zero(),
    };

    // create tx.data and tx.value for frontrun_in
    let (frontrun_data, frontrun_value) = match ingredients.get_target_pool() {
        UniswapV2(p) => v2_create_frontrun_payload_multi(
            p,
            ingredients.get_intermediary_token(),
            frontrun_in,
            frontrun_out
        ),
        UniswapV3(p) => v3_create_frontrun_payload_multi(
            p,
            ingredients.get_intermediary_token(),
            frontrun_in,
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
    let backrun_token_in = ingredients.get_intermediary_token();
    let backrun_token_out = ingredients.get_start_end_token();

    // keep some dust
    let backrun_in = get_erc20_balance(backrun_token_in, sando_address, next_block, &mut evm)?;
    let backrun_in = match ingredients.get_target_pool() {
        UniswapV2(_) => {
            let mut backrun_in_encoded = FiveByteMetaData::encode(backrun_in, 1);
            backrun_in_encoded.decrement_four_bytes();
            backrun_in_encoded.decode()
        }
        UniswapV3(_) => {
            let backrun_in_encoded = FiveByteMetaData::encode(backrun_in, 1);
            backrun_in_encoded.decode()
        }
    };

    // caluclate backrun_out using encoded backrun_in
    let backrun_out = match ingredients.get_target_pool() {
        UniswapV2(p) => {
            let out = v2_get_amount_out(backrun_in, p, false, &mut evm)?;
            out
        }
        UniswapV3(_p) => U256::zero(), // we don't need to know backrun out for v3
    };
    
    // create tx.data and tx.value for backrun_in
    let (backrun_data, backrun_value) = match ingredients.get_target_pool() {
        UniswapV2(p) => v2_create_backrun_payload_multi(p, backrun_token_in, backrun_in, backrun_out),
        UniswapV3(p) => (
            v3_create_backrun_payload_multi(p, backrun_token_in, backrun_in),
            U256::zero(),
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

    // *´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
    // *                      GENERATE REPORTS                      */
    // *.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
    // caluclate revenue from balance change
    let post_sando_bal = get_erc20_balance(backrun_token_out, sando_address, next_block, &mut evm)?;
    
    let revenue = post_sando_bal
        .checked_sub(sando_start_bal)
        .unwrap_or_default();

    // filter only passing meat txs
    let good_meats_only = ingredients
        .get_meats_ref()
        .iter()
        .zip(is_meat_good.iter())
        .filter(|&(_, &b)| b)
        .map(|(s, _)| s.to_owned())
        .collect();

    Ok(SandoRecipe::new(
        ingredients.get_head_txs_ref().clone(),
        frontrun_tx_env,
        frontrun_gas_used,
        good_meats_only,
        backrun_tx_env,
        backrun_gas_used,
        revenue,
        *next_block,
        SandwichSwapType::Forward
    ))
}
