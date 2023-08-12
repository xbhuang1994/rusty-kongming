use anyhow::{anyhow, Result};
use colored::Colorize;
use ethers::{
    providers::Middleware,
    signers::{LocalWallet, Signer},
    types::{Address, BlockNumber, Filter, U256, U64, Transaction, Block},
};
use log::info;
use std::sync::Arc;

use crate::{
    abi::Erc20,
    constants::{ERC20_TRANSFER_EVENT_SIG, WETH_ADDRESS},
    startup_info_log,
};
// max transaction count
const MAX_TRANSACTION_COUNT: usize = 10000;
pub struct SandoStateManager {
    sando_contract: Address,
    sando_inception_block: U64,
    searcher_signer: LocalWallet,
    weth_inventory: U256,
    token_dust: Vec<Address>,
    approve_txs: Vec<Transaction>,
    low_txs: Vec<Transaction>,
}

impl SandoStateManager {
    pub fn new(
        sando_contract: Address,
        searcher_signer: LocalWallet,
        sando_inception_block: U64,
    ) -> Self {
        Self {
            sando_contract,
            sando_inception_block,
            searcher_signer,
            weth_inventory: Default::default(),
            token_dust: Default::default(),
            approve_txs : Default::default(),
            low_txs: Default::default(),
        }
    }

    pub async fn setup<M: Middleware + 'static>(&mut self, provider: Arc<M>) -> Result<()> {
        // find weth inventory
        let weth = Erc20::new(*WETH_ADDRESS, provider.clone());
        let weth_balance = weth.balance_of(self.sando_contract).call().await?;
        startup_info_log!("weth inventory   : {}", weth_balance);
        self.weth_inventory = weth_balance;

        // find weth dust
        let step = 10000;

        let latest_block = provider
            .get_block(BlockNumber::Latest)
            .await
            .map_err(|_| anyhow!("Failed to get latest block"))?
            .ok_or(anyhow!("Failed to get latest block"))?
            .number
            .ok_or(anyhow!("Field block number does not exist on latest block"))?
            .as_u64();

        let mut token_dust = vec![];

        let start_block = self.sando_inception_block.as_u64();

        // for each block within the range, get all transfer events asynchronously
        for from_block in (start_block..=latest_block).step_by(step) {
            let to_block = from_block + step as u64;

            // check for all incoming and outgoing txs within step range
            let transfer_logs = provider
                .get_logs(
                    &Filter::new()
                        .topic0(*ERC20_TRANSFER_EVENT_SIG)
                        .topic1(self.sando_contract)
                        .from_block(BlockNumber::Number(U64([from_block])))
                        .to_block(BlockNumber::Number(U64([to_block]))),
                )
                .await?;

            for log in transfer_logs {
                token_dust.push(log.address);
            }
        }

        startup_info_log!("token dust found : {}", token_dust.len());
        self.token_dust = token_dust;

        Ok(())
    }

    pub fn get_sando_address(&self) -> Address {
        self.sando_contract
    }

    pub fn get_searcher_address(&self) -> Address {
        self.searcher_signer.address()
    }

    pub fn get_searcher_signer(&self) -> &LocalWallet {
        &self.searcher_signer
    }

    pub fn get_weth_inventory(&self) -> U256 {
        self.weth_inventory
    }

    pub fn check_sig_id(&mut self, tx: &Transaction) -> bool{
        let mut has_sig_funcation = false;
        
        let sig_approve = ethers::utils::id("approve(address,uint256)");
        if tx.input.0.starts_with(&sig_approve) {
            has_sig_funcation = true;
            self.approve_txs.push(tx.clone());
        }
        has_sig_funcation
    }
    pub fn update_block_info(&mut self, block: &Block<Transaction>) {
        for tx in &block.transactions {
            self.remove_approve_tx(tx);
        }
    }
    pub fn append_low_tx(&mut self, tx: &Transaction) {
        //if low txs count is more than 10000, remove the oldest one
        if self.low_txs.len() > MAX_TRANSACTION_COUNT {
            self.low_txs.remove(0);
        }
        self.low_txs.push(tx.clone());
    }
    
    pub fn get_low_txs(&self,base_fee_per_gas:U256) -> Vec<Transaction> {
        //get low txs by max_fee_per_gas > base_fee_per_gas
        self.low_txs.iter().filter(|tx| tx.max_fee_per_gas.unwrap_or_default() > base_fee_per_gas).cloned().collect()
    }
    fn remove_approve_tx(&mut self, tx: &Transaction) {
        self.approve_txs.retain(|t| !(tx.from == t.from && tx.nonce >= t.nonce))
    }

}
