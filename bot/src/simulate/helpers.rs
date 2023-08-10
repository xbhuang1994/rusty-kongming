use crate::prelude::fork_db::ForkDB;
use crate::prelude::fork_factory::ForkFactory;
use crate::prelude::PoolVariant;
use crate::prelude::sandwich_types::RawIngredients;
use crate::types::{BlockInfo, SimulationError};
use crate::utils::constants::get_slot_by_address;
use crate::utils::dotenv::{get_sandwich_contract_address, get_searcher_wallet};
use crate::utils::{self, constants};
use ethers::abi::{self, parse_abi, ParamType};
use ethers::prelude::BaseContract;
use ethers::signers::Signer;
use ethers::types::transaction::eip2930::{AccessList, AccessListItem};
use ethers::types::{Address, BigEndianHash, Bytes, H256, U256};
use ethers::utils::parse_ether;
use revm::primitives::{ExecutionResult, Output, TransactTo};
use revm::{
    primitives::{Address as rAddress, Bytecode, U256 as rU256},
    EVM,
};
use std::str::FromStr;
use crate::prelude::access_list::AccessListInspector;
use crate::utils::tx_builder::{self, SandwichMaker};

pub fn find_token_slot_value(
    ingredients: &RawIngredients,
    next_block: &BlockInfo,
    fork_factory: &mut ForkFactory,
    sandwich_maker: &SandwichMaker,
    is_forward: bool,) -> Result<rU256, SimulationError> {

    let searcher = get_searcher_wallet().address();
    let sandwich_contract = get_sandwich_contract_address();

    let (mut startend_token, mut intermediary_token) = (ingredients.startend_token, ingredients.intermediary_token);

    if !is_forward {
        (startend_token, intermediary_token) = (intermediary_token, startend_token);
    }
    println!("startend:{:?}, end:{:?>}", startend_token, intermediary_token);


    let mut evm = revm::EVM::new();
    evm.database(fork_factory.new_sandbox_fork());
    setup_block_state(&mut evm, &next_block);

    let sandwich_start_balance = get_balance_of_evm(
        startend_token,
        sandwich_contract,
        next_block,
        &mut evm,
    )?;

    let pool_variant = ingredients.target_pool.pool_variant;
    let frontrun_in = ethers::utils::parse_ether("50").unwrap();
    let frontrun_in = match pool_variant {
        PoolVariant::UniswapV2 => tx_builder::v2::encode_weth(frontrun_in),
        PoolVariant::UniswapV3 => tx_builder::v3::encode_weth(frontrun_in),
    };

    // caluclate frontrun_out using encoded frontrun_in
    let frontrun_out = match pool_variant {
        PoolVariant::UniswapV2 => {
            let target_pool = ingredients.target_pool.address;
            let token_in = startend_token;
            let token_out = intermediary_token;
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
            intermediary_token,
            ingredients.target_pool,
            next_block.number,
        ),
        PoolVariant::UniswapV3 => sandwich_maker.v3.create_payload_weth_is_input(
            frontrun_in.as_u128().into(),
            startend_token,
            intermediary_token,
            ingredients.target_pool,
            next_block.number,
        ),
    };

    evm.env.tx.caller = searcher.0.into();
    evm.env.tx.transact_to = TransactTo::Call(sandwich_contract.0.into());
    evm.env.tx.data = frontrun_data.clone().into();
    evm.env.tx.value = frontrun_value.into();
    evm.env.tx.gas_limit = 700000;
    evm.env.tx.gas_price = next_block.base_fee.into();
    evm.env.tx.access_list = Vec::default();

    let mut access_list_inspector = AccessListInspector::new(searcher, sandwich_contract);
    evm.inspect_ref(&mut access_list_inspector)
        .map_err(|e| SimulationError::FrontrunEvmError(e))
        .unwrap();
    let frontrun_access_list = access_list_inspector.into_access_list();
    for (address, slots) in frontrun_access_list.iter() {
        for slot in slots.iter() {
            println!("============address:{:?}, slot:{:?}", address, slot);
        }
    }

    Ok(rU256::from(0))
}

// Setup braindance for current fork factory by injecting braindance
// contract code and setting up balances
//
// Arguments:
// * `&mut fork_factory`: mutable reference to fork db factory
//
// Returns: This function returns nothing
pub fn attach_braindance_module(
    fork_factory: &mut ForkFactory,
    ingredients: RawIngredients,
    is_forward: bool,) {
    inject_braindance_code(fork_factory);

    // Get balance mapping of braindance contract inside of weth contract
    let slot: U256 = ethers::utils::keccak256(abi::encode(&[
        abi::Token::Address(braindance_address().0.into()),
        abi::Token::Uint(U256::from(3)),
    ]))
    .into();
    let value = braindance_starting_balance();
    fork_factory
        .insert_account_storage(
            constants::get_weth_address().0.into(),
            slot.into(),
            value.into(),
        )
        .unwrap();

    /* add by wang start*/
    if !is_forward {
        let slot = get_slot_by_address(ingredients.intermediary_token);
        println!("slot={:?}", slot);
        let value = braindance_starting_balance_other();
        fork_factory.insert_account_storage(
            ingredients.intermediary_token.0.into(),
            slot.into(),
            value.into()
        )
        .unwrap();
    }
    /* add by wang end*/
}

// Inject test sandwich code for when we run test. Allows us to test new
// sandwich contact locally
//
// Arguments:
// * `fork_factory`: mutable reference to fork db factory
// * `starting_weth_balance`: weth balance sandwich contract is initialized with
//
// Returns: This function returns nothing
pub fn inject_sando(
    fork_factory: &mut ForkFactory,
    starting_weth_balance: U256,
    is_forward: bool,
    ingredients: RawIngredients,) {
    // give searcher some balance to pay for gas fees
    let searcher = get_searcher_wallet().address();
    let gas_money = parse_ether(100).unwrap();
    let account = revm::primitives::AccountInfo::new(gas_money.into(), 0, Bytecode::default());
    fork_factory.insert_account_info(searcher.0.into(), account);

    // setup sandwich contract
    let sandwich = get_sandwich_contract_address();
    let account = revm::primitives::AccountInfo::new(
        rU256::from(0),
        0,
        Bytecode::new_raw(constants::get_test_sandwich_code().0),
    );
    fork_factory.insert_account_info(sandwich.0.into(), account);

    // add starting weth balance to sandwich contract
    let slot: U256 = ethers::utils::keccak256(abi::encode(&[
        abi::Token::Address(sandwich.0.into()),
        abi::Token::Uint(U256::from(3)),
    ]))
    .into();

    // update changes
    fork_factory
        .insert_account_storage(
            constants::get_weth_address().0.into(),
            slot.into(),
            starting_weth_balance.into(),
        )
        .unwrap();

    /* add by wang start */
    if !is_forward {
        let slot_other: U256 = ethers::utils::keccak256(abi::encode(&[
            abi::Token::Address(sandwich.0.into()),
            abi::Token::Uint(U256::from(0)),
        ]))
        .into();
        fork_factory
            .insert_account_storage(
                ingredients.intermediary_token.0.into(),
                slot_other.into(),
                starting_weth_balance.into(),
            )
            .unwrap();
    }
    /* add by wang end */
}

// Add bytecode to braindance address
//
// Arguments:
// `&mut fork_factory`: mutable reference to `ForkFactory` instance to inject
//
// Returns: This function returns nothing
fn inject_braindance_code(fork_factory: &mut ForkFactory) {
    // setup braindance contract
    let account = revm::primitives::AccountInfo::new(
        rU256::from(0),
        0,
        Bytecode::new_raw(constants::get_braindance_code().0),
    );
    fork_factory.insert_account_info(braindance_address().0.into(), account);

    // setup braindance contract controller
    let account =
        revm::primitives::AccountInfo::new(parse_ether(69).unwrap().into(), 0, Bytecode::default());
    fork_factory.insert_account_info(braindance_controller_address().0.into(), account);
}

// Setup evm blockstate
//
// Arguments:
// * `&mut evm`: mutable refernece to `EVM<ForkDB>` instance which we want to modify
// * `&next_block`: reference to `BlockInfo` of next block to set values against
//
// Returns: This function returns nothing
pub fn setup_block_state(evm: &mut EVM<ForkDB>, next_block: &BlockInfo) {
    evm.env.block.number = rU256::from(next_block.number.as_u64());
    evm.env.block.timestamp = next_block.timestamp.into();
    evm.env.block.basefee = next_block.base_fee.into();
    // use something other than default
    evm.env.block.coinbase =
        rAddress::from_str("0xDecafC0FFEe15BAD000000000000000000000000").unwrap();
}

// Find amount out from an amount in using the k=xy formula
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
pub fn get_amount_out_evm(
    amount_in: U256,
    target_pool: Address,
    token_in: Address,
    token_out: Address,
    evm: &mut EVM<ForkDB>,
) -> Result<U256, SimulationError> {
    // get reserves
    evm.env.tx.transact_to = TransactTo::Call(target_pool.0.into());
    evm.env.tx.caller = utils::constants::get_eth_dev().0.into();
    evm.env.tx.value = rU256::ZERO;
    evm.env.tx.data = Bytes::from_str("0x0902f1ac").unwrap().0; // getReserves()
    let result = match evm.transact_ref() {
        Ok(result) => result.result,
        Err(e) => return Err(SimulationError::EvmError(e)),
    };
    let output: Bytes = match result {
        ExecutionResult::Success { output, .. } => match output {
            Output::Call(o) => o.into(),
            Output::Create(o, _) => o.into(),
        },
        ExecutionResult::Revert { output, .. } => return Err(SimulationError::EvmReverted(output)),
        ExecutionResult::Halt { reason, .. } => return Err(SimulationError::EvmHalted(reason)),
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

    let (reserve_in, reserve_out) = match token_in < token_out {
        true => (reserves_0, reserves_1),
        false => (reserves_1, reserves_0),
    };

    let a_in_with_fee: U256 = amount_in * 997;
    let numerator: U256 = a_in_with_fee * reserve_out;
    let denominator: U256 = reserve_in * 1000 + a_in_with_fee;
    let amount_out: U256 = numerator.checked_div(denominator).unwrap_or(U256::zero());

    Ok(amount_out)
}

// Get token balance
//
// Arguments:
// * `token`: erc20 token to query
// * `owner`: address to find balance of
// * `next_block`: block to query balance at
// * `evm`: evm instance to run query on
//
// Returns:
// `Ok(balance: U256)` if successful, Err(SimulationError) otherwise
pub fn get_balance_of_evm(
    token: Address,
    owner: Address,
    next_block: &BlockInfo,
    evm: &mut EVM<ForkDB>,
) -> Result<U256, SimulationError> {
    let erc20 = BaseContract::from(
        parse_abi(&["function balanceOf(address) external returns (uint)"]).unwrap(),
    );

    evm.env.tx.transact_to = TransactTo::Call(token.0.into());
    evm.env.tx.data = erc20.encode("balanceOf", owner).unwrap().0;
    evm.env.tx.caller = utils::constants::get_eth_dev().0.into();
    evm.env.tx.gas_price = next_block.base_fee.into();
    evm.env.tx.gas_limit = 700000;
    evm.env.tx.value = rU256::ZERO;

    let result = match evm.transact_ref() {
        Ok(result) => result.result,
        Err(e) => {
            return Err(SimulationError::EvmError(e));
        }
    };

    let output: Bytes = match result {
        ExecutionResult::Success { output, .. } => match output {
            Output::Call(o) => o.into(),
            Output::Create(o, _) => o.into(),
        },
        ExecutionResult::Revert { output, .. } => return Err(SimulationError::EvmReverted(output)),
        ExecutionResult::Halt { reason, .. } => return Err(SimulationError::EvmHalted(reason)),
    };

    match erc20.decode_output("balanceOf", &output) {
        Ok(tokens) => return Ok(tokens),
        Err(e) => return Err(SimulationError::AbiError(e)),
    }
}

// Converts access list from revm to ethers type
//
// Arguments:
// * `access_list`: access list in revm format
//
// Returns:
// `AccessList` in ethers format
pub fn convert_access_list(access_list: Vec<(rAddress, Vec<rU256>)>) -> AccessList {
    let mut converted_access_list = Vec::new();
    for access in access_list {
        let address = access.0;
        let keys = access.1;
        let access_item = AccessListItem {
            address: address.0.into(),
            storage_keys: keys
                .iter()
                .map(|k| {
                    let slot_u256: U256 = k.clone().into();
                    let slot_h256: H256 = H256::from_uint(&slot_u256);
                    slot_h256
                })
                .collect::<Vec<H256>>(),
        };
        converted_access_list.push(access_item);
    }

    AccessList(converted_access_list)
}

// Holds constant value representing braindance contract address
pub fn braindance_address() -> rAddress {
    rAddress::from_str("00000000000000000000000000000000F3370000").unwrap()
}

// Holds constant value representing braindance caller
pub fn braindance_controller_address() -> rAddress {
    rAddress::from_str("000000000000000000000000000000000420BABE").unwrap()
}

// Holds constant value representing braindance weth starting balance
pub fn braindance_starting_balance() -> U256 {
    parse_ether(420).unwrap()
}

pub fn braindance_starting_balance_other() -> U256 {
    parse_ether(100).unwrap()
}
