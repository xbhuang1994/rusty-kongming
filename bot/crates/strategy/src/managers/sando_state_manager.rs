use anyhow::{anyhow, Result};
use colored::Colorize;
use ethers::{
    providers::Middleware,
    signers::{LocalWallet, Signer},
    types::{Address, BlockNumber, Filter, U256, U64, H256, H160, Transaction},
};
use log::info;
use std::sync::{Arc, Mutex, RwLock};

use crate::{
    abi::Erc20,
    constants::{ERC20_TRANSFER_EVENT_SIG, WETH_ADDRESS},
    startup_info_log, types::SandwichSwapType,
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
    // use map for dunplicate transaction
    approve_txs: Mutex<HashMap<H256, Transaction>>,
    // use vec for sort by timestamp
    approve_txs_vec: Mutex<Vec<H256>>,
    low_txs: Mutex<HashMap<H256, Transaction>>,
    low_txs_vec: Mutex<Vec<H256>>,
    liquidity_txs: Mutex<HashMap<H256, Transaction>>,
    liquidity_txs_vec: Mutex<Vec<H256>>,
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
            approve_txs_vec: Default::default(),
            low_txs: Default::default(),
            low_txs_vec: Default::default(),
            liquidity_txs: Default::default(),
            liquidity_txs_vec: Default::default(),
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

    pub fn check_approve_by_signature(&self, tx: &Transaction) -> bool{
        let sig_approve = ethers::utils::id("approve(address,uint256)");
        if tx.input.0.starts_with(&sig_approve) {
            self.append_approve_tx(&tx.clone());
            // info!("tx approve {:?} to {:?} input: {:?}", &tx.hash, &tx.to.unwrap_or_default(), hex::encode(&tx.input.0));
            true
        } else {
            false
        }
    }

    pub fn check_liquidity_by_signature(&self, tx: &Transaction) -> bool {
        // todo
        let sig_transfer = ethers::utils::id("transfer(address,uint256)");
        let sig_transfer_from = ethers::utils::id("transferFrom(address,address,uint256)");
        if tx.input.0.starts_with(&sig_transfer) {
            self.append_liquidity_tx(tx);
            // info!("tx transfer {:?} to {:?} input: {:?}", &tx.hash, &tx.to.unwrap_or_default(), hex::encode(&tx.input.0));
            true
        } else if tx.input.0.starts_with(&sig_transfer_from) {
            self.append_liquidity_tx(tx);
            // info!("tx transfer_from {:?} to {:?} input: {:?}", &tx.hash, &tx.to.unwrap_or_default(), hex::encode(&tx.input.0));
            true
        } else {
            false
        }
    }

    pub fn update_block_info(&self, block_txs: &Vec<Transaction>) {
        for tx in block_txs {
            // info!("remove_approve_tx by from {:?}", tx.from);
            self.remove_approve_tx(tx);
            self.remove_liquidity_tx(tx);
        }
    }
    pub fn append_low_tx(&self, tx: &Transaction) {
        //if approve txs count is more than 10000, remove the oldest one
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

    fn append_liquidity_tx(&self, tx: &Transaction) {
        //if liquidity txs count is more than 10000, remove the oldest one
        let mut map_liquidity_txs = self.liquidity_txs.lock().unwrap();
        let mut list_liquidity_txs = self.liquidity_txs_vec.lock().unwrap();

        if !map_liquidity_txs.contains_key(&tx.hash) {
            if list_liquidity_txs.len() > MAX_TRANSACTION_COUNT {
                let oldest = list_liquidity_txs.remove(0);
                map_liquidity_txs.remove(&oldest).unwrap();
                // info!("liquidity_tx vec overflow {:?} remove {:?}", MAX_TRANSACTION_COUNT, oldest);
                // info!("after remove liquidity_tx map size {:?} vec size {:?}", map_liquidity_txs.len(), list_liquidity_txs.len());
            }

            map_liquidity_txs.insert(tx.hash.clone(), tx.clone());
            list_liquidity_txs.push(tx.hash.clone());
        } else {
            // info!("exists {:?} liquidity_tx map size {:?} vec size {:?}", tx.hash, map_liquidity_txs.len(), list_liquidity_txs.len());
        }
    }


    fn append_approve_tx(&self, tx: &Transaction) {
        //if low txs count is more than 10000, remove the oldest one
        let mut map_approve_txs = self.approve_txs.lock().unwrap();
        let mut list_approve_txs = self.approve_txs_vec.lock().unwrap();

        if !map_approve_txs.contains_key(&tx.hash) {
            if list_approve_txs.len() > MAX_TRANSACTION_COUNT {
                let oldest = list_approve_txs.remove(0);
                map_approve_txs.remove(&oldest).unwrap();
                // info!("approve_tx vec overflow {:?} remove {:?}", MAX_TRANSACTION_COUNT, oldest);
                // info!("after remove approve_tx map size {:?} vec size {:?}", map_approve_txs.len(), list_approve_txs.len());
            }

            map_approve_txs.insert(tx.hash.clone(), tx.clone());
            list_approve_txs.push(tx.hash.clone());
        } else {
            // info!("exists {:?} approve_tx map size {:?} vec size {:?}", tx.hash, map_approve_txs.len(), list_approve_txs.len());
        }
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
    fn get_approve_txs(&self,from: &Address) -> Vec<Transaction> {
        let map_approve_txs = self.approve_txs.lock().unwrap();
        let mut result: Vec<Transaction> = map_approve_txs.iter().filter(|(_, tx)| tx.from == *from).map(|(_, tx)| tx).cloned().collect();
        if result.len() > 1 {
            // sort by noce
            result.sort_by_key(|t| t.nonce);
        }
        return result;
    }
    
    fn remove_approve_tx(&self, tx: &Transaction) {
        let mut map_approve_txs = self.approve_txs.lock().unwrap();
        let mut list_appprove_txs = self.approve_txs_vec.lock().unwrap();
        let remove_hash: Vec<H256> = map_approve_txs.iter().filter(|(_, t)| tx.hash == t.hash || tx.from == t.from && tx.nonce >= t.nonce).map(|(hash, _)| hash).cloned().collect();
        if remove_hash.len() > 0 {
            map_approve_txs.retain(|hash, _| remove_hash.contains(hash));
            list_appprove_txs.retain(|hash| remove_hash.contains(hash));
            // info!("after remove approve map size={:?}, approve vec size={:?}", map_approve_txs.len(), list_appprove_txs.len());
        }
    }

    /// get liquidity txs with same pool
    /// input pool_address
    /// return Vec<Transaction>
    fn get_liquidity_txs(&self, pool_address: H160) -> Vec<Transaction> {
        let map_liquidity_txs = self.liquidity_txs.lock().unwrap();
        let mut result: Vec<Transaction> = map_liquidity_txs.iter().filter(|(_, tx)| tx.to.unwrap_or_default() == pool_address).map(|(_, tx)| tx).cloned().collect();
        if result.len() > 1 {
            // sort by noce
            result.sort_by_key(|t| t.nonce);
        }
        return result;
    }
    
    fn remove_liquidity_tx(&self, tx: &Transaction) {
        let mut map_liquidity_txs = self.liquidity_txs.lock().unwrap();
        let mut list_liquidity_txs = self.liquidity_txs_vec.lock().unwrap();
        let remove_hash: Vec<H256> = map_liquidity_txs.iter().filter(|(_, t)| tx.hash == t.hash || tx.from == t.from && tx.nonce >= t.nonce).map(|(hash, _)| hash).cloned().collect();
        if remove_hash.len() > 0 {
            map_liquidity_txs.retain(|hash, _| remove_hash.contains(hash));
            list_liquidity_txs.retain(|hash| remove_hash.contains(hash));
            // info!("after remove liquidity map size={:?}, liquidity vec size={:?}", map_liquidity_txs.len(), list_liquidity_txs.len());
        }
    }

    pub fn get_head_txs(&self, from: &Address, pool_address: H160, swap_type: SandwichSwapType) -> (Vec<String>, Vec<Transaction>) {

        let mut head_txs = self.get_liquidity_txs(pool_address);
        if swap_type == SandwichSwapType::Reverse {
            let approve_txs = self.get_approve_txs(from);
            if approve_txs.len() > 0 {
                head_txs.extend(approve_txs);
            }
            if head_txs.len() > 1 {
                head_txs.sort_by_key(|t| t.nonce);
            }
        }
        let head_hashs = head_txs.iter().map(|t| format!("{:?}", t.hash)).collect();
        (head_hashs, head_txs)
    }

}
