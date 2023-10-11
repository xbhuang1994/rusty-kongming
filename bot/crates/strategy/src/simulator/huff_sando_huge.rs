use anvil::eth::util::get_precompiles_for;
use anyhow::{anyhow, Result};
use ethers::abi::Address;
use ethers::types::{U256, Transaction};
use foundry_evm::executor::TxEnv;
use foundry_evm::executor::{
    fork::SharedBackend, inspector::AccessListTracer, ExecutionResult, TransactTo,
};
use foundry_evm::revm::{
    db::CacheDB,
    primitives::Address as rAddress,
    EVM,
};
use std::collections::HashMap;

use crate::helpers::access_list_to_revm;
use crate::simulator::setup_block_state;
use crate::tx_utils::huff_sando_interface::common::limit_block_height;
use crate::types::{BlockInfo, SandoRecipe, SandwichSwapType};
use crate::constants::WETH_ADDRESS;

use super::salmonella_inspector::{IsSandoSafu, SalmonellaInspectoooor};

use super::huff_helper::{get_erc20_balance, inject_huff_sando};
use crate::simulator::credit::CreditHelper;

/// finds if sandwich is profitable + salmonella free
pub fn create_recipe_huge(
    next_block: &BlockInfo,
    frontrun_data: Vec<u8>,
    backrun_data: Vec<u8>,
    head_txs: Vec<Transaction>,
    meats: Vec<Transaction>,
    start_sando_weth_balance: U256,
    start_sando_tokens_balance: HashMap<Address, U256>,
    searcher: Address,
    sando_address: Address,
    shared_backend: SharedBackend,
    swap_type: SandwichSwapType,
    uuid: String,
) -> Result<SandoRecipe> {

    let mut fork_db = CacheDB::new(shared_backend);

    inject_huff_sando(
        &mut fork_db,
        sando_address.0.into(),
        searcher.0.into(),
        start_sando_weth_balance,
    );

    // huge sandwich may contain many other tokens, set their balance in sando
    let credit_helper_ref = CreditHelper::new();
    credit_helper_ref.credit_multi_tokens_balance(
        &start_sando_tokens_balance,
        &mut fork_db,
        sando_address.0.into()
    );
    
    let mut evm = EVM::new();
    evm.database(fork_db);
    setup_block_state(&mut evm, &next_block);

    let frontrun_value = U256::zero();
    let backrun_value = U256::zero();

    /*´:°•.°+.*•´.*:˚.°*.˚•´.°:°•.°•.*•´.*:˚.°*.˚•´.°:°•.°+.*•´.*:*/
    /*                     HEAD TRANSACTION/s                     */
    /*.•°:°.´+˚.*°.˚:*.´•*.+°.•°:´*.´•*.•°.•°:°.´:•˚°.*°.˚:*.´+°.•*/
    for head_tx in head_txs.iter() {
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

    // setup evm for frontrun transaction
    let mut frontrun_tx_env = TxEnv {
        caller: searcher.0.into(),
        gas_limit: 700000,
        gas_price: next_block.base_fee_per_gas.into(),
        gas_priority_fee: None,
        transact_to: TransactTo::Call(sando_address.0.into()),
        value: frontrun_value.into(),
        data: limit_block_height(frontrun_data.into(), next_block.number).into(),
        // data: frontrun_data.clone().into(),
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
    for meat in meats.iter() {
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
            return Err(anyhow!("[huffsando: HALT] backrun: {:?}", reason));
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
    let post_sando_weth_balance = get_erc20_balance(*WETH_ADDRESS, sando_address, next_block, &mut evm)?;
    
    let revenue = post_sando_weth_balance
        .checked_sub(start_sando_weth_balance)
        .unwrap_or_default();

    // filter only passing meat txs
    let good_meats_only = meats
        .iter()
        .zip(is_meat_good.iter())
        .filter(|&(_, &b)| b)
        .map(|(s, _)| s.to_owned())
        .collect();

    Ok(SandoRecipe::new(
        head_txs.clone(),
        frontrun_tx_env,
        frontrun_gas_used,
        good_meats_only,
        backrun_tx_env,
        backrun_gas_used,
        revenue,
        *next_block,
        swap_type,
        None,
        uuid,
        Default::default(),
        Default::default(),
        None,
        U256::zero(),
    ))
}
