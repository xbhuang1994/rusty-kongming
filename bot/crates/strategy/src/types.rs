use std::sync::Arc;
use std::fmt;

use anyhow::ensure;
use anyhow::{anyhow, Result};
use artemis_core::{
    collectors::block_collector::NewBlock, executors::flashbots_executor::FlashbotsBundle,
};
use cfmms::pool::Pool;
use ethers::providers::Middleware;
use ethers::signers::LocalWallet;
use ethers::signers::Signer;
use ethers::types::{
    Address, Block, Bytes, Eip1559TransactionRequest, Transaction, H256, U256, U64,
};

use ethers_flashbots::BundleRequest;
use foundry_evm::executor::TxEnv;

use crate::constants::DUST_OVERPAY;
use crate::helpers::access_list_to_ethers;
use crate::helpers::sign_eip1559;
use crate::simulator::credit::CreditHelper;
use crate::log_bundle;
use log::info;
use colored::Colorize;
use uuid::Uuid;

/// Core Event enum for current strategy
#[derive(Debug, Clone)]
pub enum Event {
    NewBlock(NewBlock),
    NewTransaction(Transaction),
}

/// Core Action enum for current strategy
#[derive(Debug, Clone)]
pub enum Action {
    SubmitToFlashbots(FlashbotsBundle),
}

/// sandwich direction type
#[derive(Debug, Clone, PartialEq)]
pub enum SandwichSwapType {
    Forward,
    Reverse
}

impl fmt::Display for SandwichSwapType {

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == SandwichSwapType::Forward {
            write!(f, "FORWARD")
        } else {
            write!(f, "REVERSE")
        }
    }
}

/// Configuration for variables needed for sandwiches
#[derive(Debug, Clone)]
pub struct StratConfig {
    pub sando_address: Address,
    pub sando_inception_block: U64,
    pub searcher_signer: LocalWallet,
}

/// Information on potential sandwichable opportunity
#[derive(Clone)]
pub struct RawIngredients {
    head_txs: Vec<Transaction>,
    /// Victim tx/s to be used in sandwich
    meats: Vec<Transaction>,
    /// Which token do start and end sandwich with
    start_end_token: Address,
    /// Which token do we hold for duration of sandwich
    intermediary_token: Address,
    /// Which pool are we targetting
    target_pool: Pool,
    credit_helper: CreditHelper,
    uuid: String,
}

impl RawIngredients {
    pub fn new(
        head_txs: Vec<Transaction>,
        meats: Vec<Transaction>,
        start_end_token: Address,
        intermediary_token: Address,
        target_pool: Pool,
    ) -> Self {
        Self {
            head_txs: head_txs,
            meats,
            start_end_token,
            intermediary_token,
            target_pool,
            credit_helper: CreditHelper::new(),
            uuid: format!("{}", Uuid::new_v4()),
        }
    }

    pub fn get_uuid(&self) -> String {
        self.uuid.clone()
    }

    pub fn get_start_end_token(&self) -> Address {
        self.start_end_token
    }

    pub fn get_intermediary_token(&self) -> Address {
        self.intermediary_token
    }

    pub fn get_meats_ref(&self) -> &Vec<Transaction> {
        &self.meats
    }
    pub fn get_head_txs_ref(&self) -> &Vec<Transaction> {
        &self.head_txs
    }

    pub fn get_target_pool(&self) -> Pool {
        self.target_pool
    }

    pub fn get_credit_helper_ref(&self) -> &CreditHelper {
        &self.credit_helper
    }

    // Used for logging
    pub fn print_meats(&self) -> String {
        let mut s = String::new();
        s.push('[');
        for (i, x) in self.meats.iter().enumerate() {
            s.push_str(&format!("{:?}", x.hash));
            if i != self.meats.len() - 1 {
                s.push_str(",");
            }
        }
        s.push(']');
        s
    }

    // Used for logging
    pub fn print_head_txs(&self) -> String {
        let mut s = String::new();
        s.push('[');
        for (i, x) in self.head_txs.iter().enumerate() {
            s.push_str(&format!("{:?}", x.hash));
            if i != self.head_txs.len() - 1 {
                s.push_str(",");
            }
        }
        s.push(']');
        s
    }
}

#[derive(Default, Clone, Copy)]
pub struct BlockInfo {
    pub number: U64,
    pub base_fee_per_gas: U256,
    pub timestamp: U256,
    // These are optional because we don't know these values for `next_block`
    pub gas_used: Option<U256>,
    pub gas_limit: Option<U256>,
}

impl BlockInfo {
    /// Returns block info for next block
    pub fn get_next_block(&self) -> BlockInfo {
        BlockInfo {
            number: self.number + 1,
            base_fee_per_gas: calculate_next_block_base_fee(&self),
            timestamp: self.timestamp + 12,
            gas_used: None,
            gas_limit: None,
        }
    }
}

impl TryFrom<Block<H256>> for BlockInfo {
    type Error = anyhow::Error;

    fn try_from(value: Block<H256>) -> std::result::Result<Self, Self::Error> {
        Ok(BlockInfo {
            number: value.number.ok_or(anyhow!(
                "could not parse block.number when setting up `block_manager`"
            ))?,
            gas_used: Some(value.gas_used),
            gas_limit: Some(value.gas_limit),
            base_fee_per_gas: value.base_fee_per_gas.ok_or(anyhow!(
                "could not parse base fee when setting up `block_manager`"
            ))?,
            timestamp: value.timestamp,
        })
    }
}

impl From<NewBlock> for BlockInfo {
    fn from(value: NewBlock) -> Self {
        Self {
            number: value.number,
            base_fee_per_gas: value.base_fee_per_gas,
            timestamp: value.timestamp,
            gas_used: Some(value.gas_used),
            gas_limit: Some(value.gas_limit),
        }
    }
}

/// Calculate the next block base fee
// based on math provided here: https://ethereum.stackexchange.com/questions/107173/how-is-the-base-fee-per-gas-computed-for-a-new-block
fn calculate_next_block_base_fee(block: &BlockInfo) -> U256 {
    // Get the block base fee per gas
    let current_base_fee_per_gas = block.base_fee_per_gas;

    let current_gas_used = block
        .gas_used
        .expect("can't calculate base fee from unmined block \"next_block\"");

    let current_gas_target = block
        .gas_limit
        .expect("can't calculate base fee from unmined block \"next_block\"")
        / 2;

    if current_gas_used == current_gas_target {
        current_base_fee_per_gas
    } else if current_gas_used > current_gas_target {
        let gas_used_delta = current_gas_used - current_gas_target;
        let base_fee_per_gas_delta =
            current_base_fee_per_gas * gas_used_delta / current_gas_target / 8;

        return current_base_fee_per_gas + base_fee_per_gas_delta;
    } else {
        let gas_used_delta = current_gas_target - current_gas_used;
        let base_fee_per_gas_delta =
            current_base_fee_per_gas * gas_used_delta / current_gas_target / 8;

        return current_base_fee_per_gas - base_fee_per_gas_delta;
    }
}

pub fn calculate_bribe_for_max_fee(revenue: U256,
    frontrun_gas_used: u64,
    backrun_gas_used: u64,
    base_fee_per_gas: U256,
    has_dust: bool) -> Result<U256> {

    // calc bribe (bribes paid in backrun)
    let revenue_minus_frontrun_tx_fee = revenue
        .checked_sub(U256::from(frontrun_gas_used) * base_fee_per_gas)
        .ok_or_else(|| {
        anyhow!("[FAILED TO CREATE BUNDLE] revenue doesn't cover frontrun basefee")
        })?;

    // eat a loss (overpay) to get dust onto the sando contract (more: https://twitter.com/libevm/status/1474870661373779969)
    let bribe_amount = if !has_dust {
        revenue_minus_frontrun_tx_fee + *DUST_OVERPAY
    } else {
        // bribe away 99.9999999% of revenue lmeow
        revenue_minus_frontrun_tx_fee * 999999999 / 1000000000
    };

    let max_fee = bribe_amount / backrun_gas_used;

    ensure!(
        max_fee >= base_fee_per_gas,
        format!("[FAILED TO CREATE BUNDLE] backrun maxfee {:?} less than basefee {:?}", max_fee, base_fee_per_gas)
    );

    let effective_miner_tip = max_fee.checked_sub(base_fee_per_gas);

    ensure!(
        !effective_miner_tip.is_none(),
        "[FAILED TO CREATE BUNDLE] negative miner tip"
    );

    Ok(max_fee)
}

/// All details for capturing a sando opp
#[derive(Clone)]
pub struct SandoRecipe {
    head_txs: Vec<Transaction>,
    frontrun: TxEnv,
    frontrun_gas_used: u64,
    meats: Vec<Transaction>,
    backrun: TxEnv,
    backrun_gas_used: u64,
    revenue: U256,
    target_block: BlockInfo,
    swap_type: SandwichSwapType,
    target_pool: Option<Pool>,
    profit_max: U256,
    uuid: String,
    start_end_token: Address,
    intermediary_token: Address,
    frontrun_data: Option<Vec<u8>>,
}

impl SandoRecipe {
    pub fn new(
        head_txs: Vec<Transaction>,
        frontrun: TxEnv,
        frontrun_gas_used: u64,
        meats: Vec<Transaction>,
        backrun: TxEnv,
        backrun_gas_used: u64,
        revenue: U256,
        target_block: BlockInfo,
        swap_type: SandwichSwapType,
        target_pool: Option<Pool>,
        uuid: String,
        start_end_token: Address,
        intermediary_token: Address,
        frontrun_data: Option<Vec<u8>>,
    ) -> Self {
        Self {
            head_txs,
            frontrun,
            frontrun_gas_used,
            meats,
            backrun,
            backrun_gas_used,
            revenue,
            target_block,
            swap_type,
            target_pool: target_pool,
            uuid: uuid,
            profit_max: U256::from(0),
            start_end_token: start_end_token,
            intermediary_token: intermediary_token,
            frontrun_data: frontrun_data,
        }
    }

    pub fn get_frontrun(&self) -> &TxEnv {
        &self.frontrun
    }
    pub fn get_backrun(&self) -> &TxEnv {
        &self.backrun
    }

    pub fn get_revenue(&self) -> U256 {
        self.revenue
    }
    pub fn get_frontrun_gas_used(&self) -> u64 {
        self.frontrun_gas_used
    }
    pub fn get_backrun_gas_used(&self) -> u64 {
        self.backrun_gas_used
    }

    pub fn set_profit_max(&mut self, profit: U256) {
        self.profit_max = profit;
    }

    pub fn get_profit_max(&self) -> U256 {
        self.profit_max
    }

    pub fn get_uuid(&self) -> String {
        self.uuid.clone()
    }

    pub fn get_swap_type(&self) -> SandwichSwapType {
        self.swap_type.clone()
    }

    pub fn get_target_pool(&self) -> Option<Pool> {
        self.target_pool
    }

    pub fn get_meats(&self) -> &Vec<Transaction> {
        &self.meats
    }

    pub fn get_head_txs(&self) -> &Vec<Transaction> {
        &self.head_txs
    }

    pub fn set_head_txs(&mut self, head_txs: Vec<Transaction>) {
        self.head_txs = head_txs;
    }

    pub fn get_start_end_token(&self) -> Address {
        self.start_end_token.clone()
    }

    pub fn get_intermediary_token(&self) -> Address {
        self.intermediary_token.clone()
    }

    pub fn get_frontrun_data(&self) -> Option<Vec<u8>> {
        self.frontrun_data.clone()
    }

    /// turn recipe into a signed bundle that can be sumbitted to flashbots
    pub async fn to_fb_bundle<M: Middleware>(
        self,
        sando_address: Address,
        searcher: &LocalWallet,
        has_dust: bool,
        provider: Arc<M>,
        is_huge: bool,
    ) -> Result<(BundleRequest, U256)> {
        let tx_nonce = provider
            .get_transaction_count(searcher.address(), None)
            .await
            .map_err(|e| anyhow!("FAILED TO CREATE BUNDLE: Failed to get nonce {:?}", e))?;

        // info!("bundle nonce start from {:?}", tx_nonce);
        let mut head_hashs: Vec<String> = vec![];
        let mut signed_head_txs: Vec<Bytes> = vec![];
        self.head_txs.into_iter().for_each(|head| {
                head_hashs.push(format!("{:?}", head.hash));
                signed_head_txs.push(head.rlp());
            }
        );
        // let signed_head_txs: Vec<Bytes> = self.head_txs.into_iter().map(|head| head.rlp()).collect();

        let frontrun_tx = Eip1559TransactionRequest {
            to: Some(sando_address.into()),
            gas: Some((U256::from(self.frontrun_gas_used) * 10) / 7),
            value: Some(self.frontrun.value.into()),
            data: Some(self.frontrun.data.into()),
            nonce: Some(tx_nonce),
            access_list: access_list_to_ethers(self.frontrun.access_list),
            max_fee_per_gas: Some(self.target_block.base_fee_per_gas.into()),
            ..Default::default()
        };
        let signed_frontrun = sign_eip1559(frontrun_tx, &searcher).await?;

        let mut meat_hashs: Vec<String> = vec![];
        let mut signed_meat_txs: Vec<Bytes> = vec![];
        self.meats.into_iter().for_each(|meat| {
                meat_hashs.push(format!("{:?}", meat.hash));
                signed_meat_txs.push(meat.rlp());
            }
        );
        // let signed_meat_txs: Vec<Bytes> = self.meats.into_iter().map(|meat| meat.rlp()).collect();

        let max_fee_result = calculate_bribe_for_max_fee(
            self.revenue,
            self.frontrun_gas_used,
            self.backrun_gas_used,
            self.target_block.base_fee_per_gas,
            has_dust
        );
        ensure!(
            max_fee_result.is_ok(),
            max_fee_result.err().unwrap()
        );
        let max_fee = max_fee_result.unwrap();

        let backrun_tx = Eip1559TransactionRequest {
            to: Some(sando_address.into()),
            gas: Some((U256::from(self.backrun_gas_used) * 10) / 7),
            value: Some(self.backrun.value.into()),
            data: Some(self.backrun.data.into()),
            nonce: Some(tx_nonce + 1),
            access_list: access_list_to_ethers(self.backrun.access_list),
            max_priority_fee_per_gas: Some(max_fee),
            max_fee_per_gas: Some(max_fee),
            ..Default::default()
        };
        let signed_backrun = sign_eip1559(backrun_tx, &searcher).await?;

        // construct bundle
        let mut bundled_transactions: Vec<Bytes> = vec![];
        if signed_head_txs.len() > 0 {
            bundled_transactions.append(&mut signed_head_txs.clone());
        }
        bundled_transactions.push(signed_frontrun);
        bundled_transactions.append(&mut signed_meat_txs.clone());
        bundled_transactions.push(signed_backrun);

        let mut bundle_request = BundleRequest::new();
        for tx in bundled_transactions {
            bundle_request = bundle_request.push_transaction(tx);
        }

        bundle_request = bundle_request
            .set_block(self.target_block.number)
            .set_simulation_block(self.target_block.number - 1)
            .set_simulation_timestamp(self.target_block.timestamp.as_u64());

        let profit_min = self
            .revenue
            .checked_sub(
                (U256::from(self.frontrun_gas_used) * self.target_block.base_fee_per_gas)
                    + (U256::from(self.backrun_gas_used) * max_fee),
            )
            .unwrap_or_default();

        let profit_max = self
            .revenue
            .checked_sub(
                (U256::from(self.frontrun_gas_used) * self.target_block.base_fee_per_gas)
                    + (U256::from(self.backrun_gas_used) * self.target_block.base_fee_per_gas),
            )
            .unwrap_or_default();

        log_bundle!(
            is_huge,
            self.uuid,
            self.swap_type,
            head_hashs.join(","),
            meat_hashs.join(","),
            self.target_block.number,
            self.revenue,
            self.frontrun_gas_used,
            self.backrun_gas_used,
            profit_min,
            profit_max
        );

        Ok((bundle_request, profit_max))
    }
}
