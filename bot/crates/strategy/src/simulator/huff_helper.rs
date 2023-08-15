
use cfmms::pool::UniswapV2Pool;
use ethers::abi::{self, parse_abi, Address, ParamType};
use ethers::prelude::BaseContract;
use ethers::types::{Bytes, U256};
use crate::constants::{GET_RESERVES_SIG, SUGAR_DADDY, WETH_ADDRESS, LIL_ROUTER_WETH_AMT_BASE};
use foundry_evm::executor::{
    fork::SharedBackend, ExecutionResult, Output, TransactTo,
};
use anyhow::{anyhow, Result};
use crate::types::BlockInfo;
use foundry_evm::revm::{
    db::CacheDB,
    primitives::U256 as rU256,
    EVM,
};

/// Get the balance of a token in an evm (account for tax)
pub fn get_erc20_balance(
    token: Address,
    owner: Address,
    block: &BlockInfo,
    evm: &mut EVM<CacheDB<SharedBackend>>,
) -> Result<U256> {
    let erc20 = BaseContract::from(
        parse_abi(&["function balanceOf(address) external returns (uint)"]).unwrap(),
    );

    evm.env.tx.transact_to = TransactTo::Call(token.0.into());
    evm.env.tx.data = erc20.encode("balanceOf", owner).unwrap().0;
    evm.env.tx.caller = (*SUGAR_DADDY).into(); // spoof addy with a lot of eth
    evm.env.tx.nonce = None;
    evm.env.tx.gas_price = block.base_fee_per_gas.into();
    evm.env.tx.gas_limit = 700000;
    evm.env.tx.value = rU256::ZERO;

    let result = match evm.transact_ref() {
        Ok(result) => result.result,
        Err(e) => {
            return Err(anyhow!("[get_erc20_balance: EVMError] {:?}", e));
        }
    };

    let output: Bytes = match result {
        ExecutionResult::Success { output, .. } => match output {
            Output::Call(o) => o.into(),
            Output::Create(o, _) => o.into(),
        },
        ExecutionResult::Revert { output, .. } => {
            return Err(anyhow!("[get_erc20_balance: Revert] {:?}", output))
        }
        ExecutionResult::Halt { reason, .. } => {
            return Err(anyhow!("[get_erc20_balance: Halt] {:?}", reason))
        }
    };

    match erc20.decode_output("balanceOf", &output) {
        Ok(tokens) => return Ok(tokens),
        Err(e) => return Err(anyhow!("[get_erc20_balance: ABI Error] {:?}", e)),
    }
}

// Find amount out from an amount in using the k=xy formula
// note: reserve values taken from evm
// note: assuming fee is set to 3% for all pools (not case irl)
//
// Arguments:
// * `amount_in`: amount of token in
// * `target_pool`: address of pool
// * `token_in`: address of token in
// * `token_out`: address of token out
// * `evm`: mutable reference to evm used for query
//
// Returns:
// Ok(U256): amount out
// Err(SimulationError): if error during caluclation
pub fn v2_get_amount_out(
    amount_in: U256,
    target_pool: UniswapV2Pool,
    is_frontrun: bool,
    evm: &mut EVM<CacheDB<SharedBackend>>,
) -> Result<U256> {
    // get reserves
    evm.env.tx.transact_to = TransactTo::Call(target_pool.address().0.into());
    evm.env.tx.caller = (*SUGAR_DADDY).0.into(); // spoof weth address for its ether
    evm.env.tx.value = rU256::ZERO;
    evm.env.tx.data = (*GET_RESERVES_SIG).0.clone(); // getReserves()
    evm.env.tx.nonce = None;
    let result = match evm.transact_ref() {
        Ok(result) => result.result,
        Err(e) => return Err(anyhow!("[get_amount_out_evm: EVM ERROR] {:?}", e)),
    };
    let output: Bytes = match result {
        ExecutionResult::Success { output, .. } => match output {
            Output::Call(o) => o.into(),
            Output::Create(o, _) => o.into(),
        },
        ExecutionResult::Revert { output, .. } => {
            return Err(anyhow!("[get_amount_out_evm: EVM REVERTED] {:?}", output))
        }
        ExecutionResult::Halt { reason, .. } => {
            return Err(anyhow!("[get_amount_out_evm: EVM HALT] {:?}", reason))
        }
    };

    let tokens = abi::decode(
        &vec![
            ParamType::Uint(128),
            ParamType::Uint(128),
            ParamType::Uint(32),
        ],
        &output,
    )
    .unwrap();

    let reserves_0 = tokens[0].clone().into_uint().unwrap();
    let reserves_1 = tokens[1].clone().into_uint().unwrap();

    let other_token = [target_pool.token_a, target_pool.token_b]
        .into_iter()
        .find(|&t| t != *WETH_ADDRESS)
        .unwrap();

    let (input_token, output_token) = if is_frontrun {
        // if frontrun we trade WETH -> TOKEN
        (*WETH_ADDRESS, other_token)
    } else {
        // if backrun we trade TOKEN -> WETH
        (other_token, *WETH_ADDRESS)
    };

    let (reserve_in, reserve_out) = match input_token < output_token {
        true => (reserves_0, reserves_1),
        false => (reserves_1, reserves_0),
    };

    let a_in_with_fee: U256 = amount_in * 997;
    let numerator: U256 = a_in_with_fee * reserve_out;
    let denominator: U256 = reserve_in * 1000 + a_in_with_fee;
    let amount_out: U256 = numerator.checked_div(denominator).unwrap_or(U256::zero());

    Ok(amount_out)
}

// #[cfg(feature = "debug")]
pub fn inject_huff_sando(
    db: &mut CacheDB<SharedBackend>,
    huff_sando_addy: foundry_evm::executor::B160,
    searcher: foundry_evm::executor::B160,
    sando_start_bal: U256,
) {
    // compile huff contract
    let git_root = std::str::from_utf8(
        &std::process::Command::new("git")
            .arg("rev-parse")
            .arg("--show-toplevel")
            .output()
            .expect("Failed to execute git command")
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    let mut contract_dir = std::path::PathBuf::from(git_root);
    contract_dir.push("contract/src");

    let output = std::process::Command::new("huffc")
        .arg("--bin-runtime")
        .arg("sando.huff")
        .current_dir(contract_dir)
        .output()
        .expect("Failed to compile huff sando contract");

    assert!(output.status.success(), "Command execution failed");

    let huff_sando_code = std::str::from_utf8(&output.stdout).unwrap();
    let huff_sando_code = <Bytes as std::str::FromStr>::from_str(huff_sando_code).unwrap();

    //// insert huff sando bytecode
    let huff_sando_info = foundry_evm::revm::primitives::AccountInfo::new(
        rU256::ZERO,
        0,
        foundry_evm::executor::Bytecode::new_raw(huff_sando_code.0),
    );

    db.insert_account_info(huff_sando_addy, huff_sando_info);

    // insert and fund lilRouter controller (so we can spoof)
    let searcher_info = foundry_evm::revm::primitives::AccountInfo::new(
        crate::simulator::eth_to_wei(LIL_ROUTER_WETH_AMT_BASE),
        0,
        foundry_evm::executor::Bytecode::default(),
    );
    db.insert_account_info(searcher, searcher_info);

    // fund huff sando with xxx weth
    let slot = foundry_evm::revm::primitives::keccak256(&abi::encode(&[
        abi::Token::Address(huff_sando_addy.0.into()),
        abi::Token::Uint(U256::from(3)),
    ]));

    db.insert_account_storage((*WETH_ADDRESS).into(), slot.into(), sando_start_bal.into())
        .unwrap();
}