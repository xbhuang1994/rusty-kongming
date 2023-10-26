use anyhow::{anyhow, Result};
use cfmms::{
    /* checkpoint::sync_pools_from_checkpoint, */
    dex::{Dex, DexVariant},
    pool::Pool,
    /* sync::sync_pairs, */
};
use colored::Colorize;
use dashmap::DashMap;
use ethers::{
    abi,
    providers::Middleware,
    types::{Address, BlockNumber, Diff, TraceType, Transaction, H160, H256, U256},
};
use log::info;
use std::{path::Path, str::FromStr, sync::Arc};

use crate::{constants::WETH_ADDRESS, startup_info_log};

pub struct TouchedTxs {
    from_weth: Vec<Transaction>,
    to_weth: Vec<Transaction>,
}
impl TouchedTxs {
    pub fn new() -> Self {
        Self {
            from_weth: vec![],
            to_weth: vec![],
        }
    }

    pub fn remove_transaction(&mut self, tx: &Transaction) {
        self.from_weth.retain(|t| !(tx.from == t.from && tx.nonce >= t.nonce));
        self.to_weth.retain(|t| !(tx.from == t.from && tx.nonce >= t.nonce));
    }
    pub fn append_transaction(&mut self, tx: &Transaction, from_weth: bool) {
        self.remove_transaction(&tx);
        if from_weth {
            self.from_weth.push(tx.clone());
        } else {
            self.to_weth.push(tx.clone());
        }
    }
}

pub(crate) struct PoolManager<M> {
    /// Provider
    provider: Arc<M>,
    /// Sandwichable pools
    pools: DashMap<Address, Pool>,
    /// Which dexes to monitor
    dexes: Vec<Dex>,
    /// Cache for touched pools
    mem_touched_pools: DashMap<Address, TouchedTxs>,
}

impl<M: Middleware + 'static> PoolManager<M> {
    /// Gets state of all pools
    pub async fn setup(&self) -> Result<()> {
        let checkpoint_path = ".cfmms-checkpoint.json";

        let checkpoint_exists = Path::new(checkpoint_path).exists();

        let pools = if checkpoint_exists {
            let (_, pools) =
                sync_pools_from_checkpoint_with_throttle(checkpoint_path, 1, 0, self.provider.clone()).await?;
            pools
        } else {
            sync_pairs(
                self.dexes.clone(),
                self.provider.clone(),
                Some(checkpoint_path),
            )
            .await?
        };

        for pool in pools {
            self.pools.insert(pool.address(), pool);
        }

        startup_info_log!("pools synced: {}", self.pools.len());

        Ok(())
    }

    /// Return a tx's touched pools
    // enhancement: record stable coin pairs to sandwich as well here
    pub async fn get_touched_sandwichable_pools(
        &self,
        victim_tx: &Transaction,
        latest_block: BlockNumber,
        provider: Arc<M>,
    ) -> Result<(Vec<Pool>, Vec<Pool>)> {
        // get victim tx state diffs
        let state_diffs = provider
            .trace_call(victim_tx, vec![TraceType::StateDiff], Some(latest_block))
            .await?
            .state_diff
            .ok_or(anyhow!("not sandwichable, no state diffs produced"))?
            .0;

        // capture all addresses that have a state change and are also a `WETH` pool
        let touched_pools: Vec<Pool> = state_diffs
            .keys()
            .filter_map(|e| self.pools.get(e).map(|p| (*p.value()).clone()))
            .filter(|e| match e {
                Pool::UniswapV2(p) => vec![p.token_a, p.token_b].contains(&WETH_ADDRESS),
                Pool::UniswapV3(p) => vec![p.token_a, p.token_b].contains(&WETH_ADDRESS),
            })
            .collect();

        // nothing to sandwich
        if touched_pools.is_empty() {
            return Ok((vec![], vec![]));
        }

        // find trade direction
        let weth_state_diff = &state_diffs
            .get(&WETH_ADDRESS)
            .ok_or(anyhow!("Missing WETH state diffs"))?
            .storage;

        let mut sandwichable_pools = vec![];
        let mut sandwichable_pools_reverse = vec![];

        for pool in touched_pools {
            // find pool mapping location on WETH contract
            let storage_key = H256::from(ethers::utils::keccak256(abi::encode(&[
                abi::Token::Address(pool.address()),
                abi::Token::Uint(U256::from(3)), // WETH balanceOf mapping is at index 3
            ])));

            // in reality we also want to check stable coin pools
            if let Some(Diff::Changed(c)) = weth_state_diff.get(&storage_key) {
                let from = U256::from(c.from.to_fixed_bytes());
                let to = U256::from(c.to.to_fixed_bytes());
                let pool_address = pool.address().clone();
                if !self.mem_touched_pools.contains_key(&pool_address) {
                    self.mem_touched_pools.insert(pool_address, TouchedTxs::new());
                }
                match self.mem_touched_pools.get_mut(&pool_address){
                    Some(mut mem_touched_pool) => {
                        let touched_tx = victim_tx.clone(); 
                        mem_touched_pool.append_transaction(&touched_tx, to > from);
                    },
                    None => {
                        return Err(anyhow!("Pool {} not found", pool_address))
                    }
                }
                
                // right now bot can only sandwich `weth->token` trades
                // enhancement: add support for `token->weth` trades (using longtail or flashswaps sandos)
                if to > from {
                    sandwichable_pools.push(pool);
                } else {
                    sandwichable_pools_reverse.push(pool);
                }
            }
        }

        Ok((sandwichable_pools, sandwichable_pools_reverse))
    }

    pub fn new(provider: Arc<M>) -> Self {
        let dexes_data = [
            (
                // Uniswap v2
                "0x5C69bEe701ef814a2B6a3EDD4B1652CB9cc5aA6f",
                DexVariant::UniswapV2,
                10000835u64,
            ),
            (
                // Sushiswap
                "0xC0AEe478e3658e2610c5F7A4A2E1777cE9e4f2Ac",
                DexVariant::UniswapV2,
                10794229u64,
            ),
            (
                // Crypto.com swap
                "0x9DEB29c9a4c7A88a3C0257393b7f3335338D9A9D",
                DexVariant::UniswapV2,
                10828414u64,
            ),
            (
                // Convergence swap
                "0x4eef5746ED22A2fD368629C1852365bf5dcb79f1",
                DexVariant::UniswapV2,
                12385067u64,
            ),
            (
                // Pancakeswap
                "0x1097053Fd2ea711dad45caCcc45EfF7548fCB362",
                DexVariant::UniswapV2,
                15614590u64,
            ),
            (
                // ShibaSwap
                "0x115934131916C8b277DD010Ee02de363c09d037c",
                DexVariant::UniswapV2,
                12771526u64,
            ),
            (
                // Saitaswap
                "0x35113a300ca0D7621374890ABFEAC30E88f214b1",
                DexVariant::UniswapV2,
                15210780u64,
            ),
            (
                // Uniswap v3
                "0x1F98431c8aD98523631AE4a59f267346ea31F984",
                DexVariant::UniswapV3,
                12369621u64,
            ),
        ];

        let dexes = dexes_data
            .into_iter()
            .map(|(address, variant, number)| {
                Dex::new(H160::from_str(address).unwrap(), variant, number, Some(300))
            })
            .collect();

        Self {
            pools: DashMap::new(),
            provider,
            dexes,
            mem_touched_pools: DashMap::new(),
        }
    }

    pub fn update_block_info(&self, block_txs: &Vec<Transaction>) {
        for tx in block_txs {
            // info!("remove_mem_tounced_pool_tx by from {:?}", tx.from);
            self.remove_mem_touched_pool_tx(tx);
        }
    }

    fn remove_mem_touched_pool_tx(&self, tx: &Transaction) {
        self.mem_touched_pools
            .iter_mut()
            .for_each(|mut r| r.remove_transaction(&tx));
    }
}


/** Below functions copy from cfmms::sync.rs and cfmms::checkpoint.rs */
//Get all pairs from last synced block and sync reserve values for each Dex in the `dexes` vec.
async fn sync_pools_from_checkpoint_with_throttle<M: 'static + Middleware>(
    path_to_checkpoint: &str,
    step: usize,
    requests_per_second_limit: usize,
    middleware: Arc<M>,
) -> Result<(Vec<Dex>, Vec<Pool>), cfmms::errors::CFMMError<M>> {
    let current_block = middleware
        .get_block_number()
        .await
        .map_err(cfmms::errors::CFMMError::MiddlewareError)?;

    let request_throttle = Arc::new(std::sync::Mutex::new(cfmms::throttle::RequestThrottle::new(requests_per_second_limit)));
    //Initialize multi progress bar
    let multi_progress_bar = indicatif::MultiProgress::new();

    //Read in checkpoint
    let (dexes, pools, checkpoint_block_number) = cfmms::checkpoint::deconstruct_checkpoint(path_to_checkpoint);

    //Sort all of the pools from the checkpoint into uniswapv2 and uniswapv3 pools so we can sync them concurrently
    let (uinswap_v2_pools, uniswap_v3_pools) = cfmms::checkpoint::sort_pool_variants(pools);

    let mut aggregated_pools = vec![];
    let mut handles = vec![];

    //Sync all uniswap v2 pools from checkpoint
    if !uinswap_v2_pools.is_empty() {
        handles.push(
            cfmms::checkpoint::batch_sync_pools_from_checkpoint(
                uinswap_v2_pools,
                DexVariant::UniswapV2,
                multi_progress_bar.add(indicatif::ProgressBar::new(0)),
                request_throttle.clone(),
                middleware.clone(),
            )
            .await,
        );
    }

    //Sync all uniswap v3 pools from checkpoint
    if !uniswap_v3_pools.is_empty() {
        handles.push(
            cfmms::checkpoint::batch_sync_pools_from_checkpoint(
                uniswap_v3_pools,
                DexVariant::UniswapV3,
                multi_progress_bar.add(indicatif::ProgressBar::new(0)),
                request_throttle.clone(),
                middleware.clone(),
            )
            .await,
        );
    }

    //Sync all pools from the since synced block
    handles.extend(
        cfmms::checkpoint::get_new_pools_from_range(
            dexes.clone(),
            checkpoint_block_number,
            current_block.into(),
            step,
            request_throttle,
            multi_progress_bar,
            middleware.clone(),
        )
        .await,
    );

    for handle in handles {
        match handle.await {
            Ok(sync_result) => {
                match sync_result {
                    Ok(pools) => {
                        aggregated_pools.extend(pools);
                    },
                    Err(e) => {
                        info!("sync pool error: {:?}", e);
                    },
                }
            },
            Err(err) => {
                {
                    if err.is_panic() {
                        // Resume the panic on the main task
                        std::panic::resume_unwind(err.into_panic());
                    }
                }
            }
        }
    }

    //update the sync checkpoint
    cfmms::checkpoint::construct_checkpoint(
        dexes.clone(),
        &aggregated_pools,
        current_block.as_u64(),
        path_to_checkpoint,
    );

    Ok((dexes, aggregated_pools))
}

//Get all pairs and sync reserve values for each Dex in the `dexes` vec.
async fn sync_pairs<M: 'static + Middleware>(
    dexes: Vec<Dex>,
    middleware: Arc<M>,
    checkpoint_path: Option<&str>,
) -> Result<Vec<Pool>, cfmms::errors::CFMMError<M>> {
    //Sync pairs with throttle but set the requests per second limit to 0, disabling the throttle.
    sync_pairs_with_throttle(dexes, 1, middleware, 0, checkpoint_path).await
}

//Get all pairs and sync reserve values for each Dex in the `dexes` vec.
async fn sync_pairs_with_throttle<M: 'static + Middleware>(
    dexes: Vec<Dex>,
    step: usize, //TODO: Add docs on step. Step is the block range used to get all pools from a dex if syncing from event logs
    middleware: Arc<M>,
    requests_per_second_limit: usize,
    checkpoint_path: Option<&str>,
) -> Result<Vec<Pool>, cfmms::errors::CFMMError<M>> {
    let current_block = middleware
        .get_block_number()
        .await
        .map_err(cfmms::errors::CFMMError::MiddlewareError)?;

    //Initialize a new request throttle
    let request_throttle = Arc::new(std::sync::Mutex::new(cfmms::throttle::RequestThrottle::new(requests_per_second_limit)));

    //Aggregate the populated pools from each thread
    let mut aggregated_pools: Vec<Pool> = vec![];
    let mut handles = vec![];

    //Initialize multi progress bar
    let multi_progress_bar = indicatif::MultiProgress::new();

    //For each dex supplied, get all pair created events and get reserve values
    for dex in dexes.clone() {
        let middleware = middleware.clone();
        let request_throttle = request_throttle.clone();
        let progress_bar = multi_progress_bar.add(indicatif::ProgressBar::new(0));

        //Spawn a new thread to get all pools and sync data for each dex
        handles.push(tokio::spawn(async move {
            progress_bar.set_style(
                indicatif::ProgressStyle::with_template("{msg} {bar:40.cyan/blue} {pos:>7}/{len:7}")
                    .expect("Error when setting progress bar style")
                    .progress_chars("##-"),
            );

            //Get all of the pools from the dex
            progress_bar.set_message(format!("Getting all pools from: {}", dex.factory_address()));

            let mut pools = dex
                .get_all_pools(
                    request_throttle.clone(),
                    step,
                    progress_bar.clone(),
                    middleware.clone(),
                )
                .await?;

            progress_bar.reset();
            progress_bar.set_style(
                indicatif::ProgressStyle::with_template("{msg} {bar:40.cyan/blue} {pos:>7}/{len:7}")
                    .expect("Error when setting progress bar style")
                    .progress_chars("##-"),
            );

            //Get all of the pool data and sync the pool
            progress_bar.set_message(format!(
                "Getting all pool data for: {}",
                dex.factory_address()
            ));
            progress_bar.set_length(pools.len() as u64);

            dex.get_all_pool_data(
                &mut pools,
                request_throttle.clone(),
                progress_bar.clone(),
                middleware.clone(),
            )
            .await?;

            //Clean empty pools
            pools = cfmms::sync::remove_empty_pools(pools);

            Ok::<_, cfmms::errors::CFMMError<M>>(pools)
        }));
    }

    for handle in handles {
        match handle.await {
            Ok(sync_result) => {
                match sync_result {
                    Ok(pools) => {
                        aggregated_pools.extend(pools);
                    },
                    Err(e) => {
                        info!("sync pool error: {:?}", e);
                    },
                }
            },
            Err(err) => {
                {
                    if err.is_panic() {
                        // Resume the panic on the main task
                        std::panic::resume_unwind(err.into_panic());
                    }
                }
            }
        }
    }

    //Save a checkpoint if a path is provided
    if checkpoint_path.is_some() {
        let checkpoint_path = checkpoint_path.unwrap();

        cfmms::checkpoint::construct_checkpoint(
            dexes,
            &aggregated_pools,
            current_block.as_u64(),
            checkpoint_path,
        )
    }

    //Return the populated aggregated pools vec
    Ok(aggregated_pools)
}