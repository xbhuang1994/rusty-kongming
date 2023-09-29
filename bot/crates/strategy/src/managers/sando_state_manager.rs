use anyhow::{anyhow, Result};
use colored::Colorize;
use ethers::{
    providers::Middleware,
    signers::{LocalWallet, Signer},
    types::{Address, BlockNumber, Filter, U256, U64, H256, Transaction, Block},
};
use log::info;
use std::sync::{Arc, Mutex, RwLock};

use crate::{
    abi::Erc20,
    constants::{ERC20_TRANSFER_EVENT_SIG, WETH_ADDRESS},
    startup_info_log,
};
use std::collections::HashMap;

// max transaction count
const MAX_TRANSACTION_COUNT: usize = 10000;
pub struct SandoStateManager {
    sando_contract: Address,
    sando_inception_block: U64,
    searcher_signer: LocalWallet,
    weth_inventory: RwLock<U256>,
    token_dust: Mutex<Vec<Address>>,
    approve_txs: Mutex<Vec<Transaction>>,
    low_txs: Mutex<HashMap<H256, Transaction>>,
    low_txs_vec: Mutex<Vec<H256>>,
    token_inventory_map: Arc<Mutex<HashMap<Address, U256>>>,
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
            low_txs_vec: Default::default(),
            token_inventory_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn setup<M: Middleware + 'static>(&self, provider: Arc<M>) -> Result<()> {
        // find weth inventory
        let weth = Erc20::new(*WETH_ADDRESS, provider.clone());
        let weth_balance = weth.balance_of(self.sando_contract).call().await?;
        startup_info_log!("weth inventory   : {}", weth_balance);

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

        let mut locked_weth_inventory = self.weth_inventory.write().unwrap();
        *locked_weth_inventory = weth_balance;
        
        let mut locked_token_dust = self.token_dust.lock().unwrap();
        for log in token_dust {
            locked_token_dust.push(log);
        }

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
        let locked_weth_inventory = self.weth_inventory.read().unwrap();
        return *locked_weth_inventory;
    }

    fn get_inventory_from_map(&self, token: Address) -> Option<U256> {
        let locked_map = self.token_inventory_map.lock().unwrap();
        if locked_map.contains_key(&token.clone()) {
            Some(locked_map[&token.clone()])
        } else {
            None
        }
    }

    pub async fn get_token_inventory<M: Middleware + 'static>(&self, token: Address, provider: Arc<M>) -> U256 {

        match self.get_inventory_from_map(token) {
            Some(inventory) => {
                inventory
            },
            None => {
                let other = Erc20::new(token, provider.clone());
                let other_balance = other.balance_of(self.sando_contract).call().await.unwrap();
                // should get locker after call.await
                let mut locked_map = self.token_inventory_map.lock().unwrap();
                locked_map.insert(token.clone(), other_balance.clone());
                startup_info_log!("get token{} inventory   : {}", token, other_balance.clone());
                other_balance
            }
        }
    }

    pub fn check_sig_id(&self, tx: &Transaction) -> bool{
        let sig_approve = ethers::utils::id("approve(address,uint256)");
        if tx.input.0.starts_with(&sig_approve) {
            self.append_approve_tx(&tx.clone());
            true
        } else {
            false
        }
    }

    pub fn update_block_info(&self, block: &Block<Transaction>) {
        for tx in &block.transactions {
            self.remove_approve_tx(tx);
        }
    }
    pub fn append_low_tx(&self, tx: &Transaction) {
        //if low txs count is more than 10000, remove the oldest one
        let mut map_low_txs = self.low_txs.lock().unwrap();
        let mut list_low_txs = self.low_txs_vec.lock().unwrap();
        
        if !map_low_txs.contains_key(&tx.hash) {
            if list_low_txs.len() > MAX_TRANSACTION_COUNT {
                let oldest = list_low_txs.remove(0);
                map_low_txs.remove(&oldest).unwrap();
                // info!("low_tx vec overflow {:?} remove {:?}", MAX_TRANSACTION_COUNT, oldest);
                // info!("after remove map size {:?} vec size {:?}", map_low_txs.len(), list_low_txs.len());
            }
            
            map_low_txs.insert(tx.hash.clone(), tx.clone());
            list_low_txs.push(tx.hash.clone());
        } else {
            // info!("exists {:?} map size {:?} vec size {:?}", tx.hash, map_low_txs.len(), list_low_txs.len());
        }
    }

    fn append_approve_tx(&self, tx: &Transaction) {
        //if low txs count is more than 10000, remove the oldest one
        let mut list_approve_txs = self.approve_txs.lock().unwrap();
        if list_approve_txs.len() > MAX_TRANSACTION_COUNT {
            info!("approve_tx vec overflow");
            list_approve_txs.remove(0);
        }
        list_approve_txs.push(tx.clone());
        info!("approve_tx size {:?}", list_approve_txs.len());
    }
    
    pub fn get_low_txs(&self,base_fee_per_gas:U256) -> Vec<Transaction> {
        //get low txs by max_fee_per_gas > base_fee_per_gas
        let mut map_low_txs = self.low_txs.lock().unwrap();
        let mut list_low_txs = self.low_txs_vec.lock().unwrap();
        let result: Vec<Transaction> = map_low_txs.iter().filter(|(_, tx)| tx.max_fee_per_gas.unwrap_or_default() > base_fee_per_gas).map(|(_, tx)| tx).cloned().collect();
        
        // info!("before low map size={:?}, low vec size={:?}, result size={:?}", map_low_txs.len(), list_low_txs.len(), result.len());
        if result.len() > 0 {
            let result_hash: Vec<H256> = result.iter().map(|tx|{tx.hash}).collect();
            map_low_txs.retain(|hash, _| !result_hash.contains(hash));
            list_low_txs.retain(|hash| !result_hash.contains(hash));

            // info!("after low map size={:?}, low vec size={:?}", map_low_txs.len(), list_low_txs.len());
        }
        return result;
    }
    
    /// get approve txs by tx.from
    /// input Address
    /// return Vec<Transaction>
    pub fn get_approve_txs(&self,from: &Address) -> Vec<Transaction> {
        let locked_vec = self.approve_txs.lock().unwrap();
        locked_vec.iter().filter(|tx| tx.from == *from).cloned().collect()
    }
    
    fn remove_approve_tx(&self, tx: &Transaction) {
        let mut locked_vec = self.approve_txs.lock().unwrap();
        locked_vec.retain(|t| !(tx.from == t.from && tx.nonce >= t.nonce))
    }

}
