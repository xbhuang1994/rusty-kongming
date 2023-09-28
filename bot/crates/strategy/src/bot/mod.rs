use anyhow::Result;
use artemis_core::{collectors::block_collector::NewBlock, types::Strategy};
use async_trait::async_trait;
use cfmms::pool::Pool::{UniswapV2, UniswapV3};
use colored::Colorize;
use ethers::{
    providers::Middleware,
    types::{Transaction, U256}
};
use foundry_evm::executor::fork::{BlockchainDb, BlockchainDbMeta, SharedBackend};
use log::{error, info};
use std::{collections::{BTreeSet, LinkedList}, sync::{Arc,Mutex}, time, thread};
use tokio::{runtime, sync::broadcast::Sender};

use crate::{
    constants::WETH_ADDRESS,
    log_error, log_info_cyan, log_new_block_info, log_not_sandwichable, log_opportunity,
    managers::{
        block_manager::BlockManager, pool_manager::PoolManager,
        sando_state_manager::SandoStateManager,
    },
    simulator::{huff_sando::create_recipe, lil_router::find_optimal_input},
    simulator::{huff_sando_reverse::create_recipe_reverse, lil_router_reverse::find_optimal_input_reverse},
    types::{Action, BlockInfo, Event, RawIngredients, SandoRecipe, StratConfig, SandwichSwapType},
    helpers::calculate_inventory_for_debug,
};

pub struct SandoBot<M> {
    /// Ethers client
    provider: Arc<M>,
    /// Keeps track of onchain pools
    pool_manager: PoolManager<M>,
    /// Block manager
    block_manager: BlockManager,
    /// Keeps track of weth inventory & token dust
    sando_state_manager: SandoStateManager,
    
    event_runtime: tokio::runtime::Runtime,
    event_list: Arc<Mutex<LinkedList<Event>>>,
    event_sender: Arc<Mutex<Option<Sender<Event>>>>,
    action_list: Arc<Mutex<LinkedList<Action>>>,
    action_runtime: tokio::runtime::Runtime,
    action_sender: Arc<Mutex<Option<Sender<Action>>>>,
}

impl<M: Middleware + 'static> SandoBot<M> {
    /// Create a new instance
    pub fn new(client: Arc<M>, config: &StratConfig) -> Self {
        Self {
            pool_manager: PoolManager::new(client.clone()),
            provider: client,
            block_manager: BlockManager::new(),
            sando_state_manager: SandoStateManager::new(
                config.sando_address,
                config.searcher_signer.clone(),
                config.sando_inception_block,
            ),
            event_runtime: runtime::Builder::new_multi_thread().worker_threads(8).enable_all().build().unwrap(),
            event_list: Arc::new(Mutex::new(LinkedList::new())),
            event_sender: Arc::new(Mutex::new(None)),
            action_list: Arc::new(Mutex::new(LinkedList::new())),
            action_runtime: runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap(),
            action_sender: Arc::new(Mutex::new(None)),
        }
    }

    /// Main logic for the strategy
    /// Checks if the passed `RawIngredients` is sandwichable
    pub async fn is_sandwichable(
        &self,
        ingredients: RawIngredients,
        target_block: BlockInfo,
        swap_type: SandwichSwapType,
    ) -> Result<SandoRecipe> {
        // setup shared backend
        let shared_backend = SharedBackend::spawn_backend_thread(
            self.provider.clone(),
            BlockchainDb::new(
                BlockchainDbMeta {
                    cfg_env: Default::default(),
                    block_env: Default::default(),
                    hosts: BTreeSet::from(["".to_string()]),
                },
                None,
            ), /* default because not accounting for this atm */
            Some((target_block.number - 1).into()),
        );

        // enhancement: should set another inventory when reverse
        let token_inventory = if cfg!(feature = "debug") {
            // spoof weth balance when the debug feature is active
            // (*crate::constants::WETH_FUND_AMT).into()
            calculate_inventory_for_debug(&ingredients)
        } else {
            if swap_type == SandwichSwapType::Forward {
                self.sando_state_manager.get_weth_inventory()
            } else {
                self.sando_state_manager.get_token_inventory(
                    ingredients.get_start_end_token(),
                    self.provider.clone()
                ).await
            }
        };

        let optimal_input;
        let recipe;

        match swap_type {
            SandwichSwapType::Forward => {
                optimal_input = find_optimal_input(
                    &ingredients,
                    &target_block,
                    token_inventory,
                    shared_backend.clone()
                )
                .await?;
    
                recipe = create_recipe(
                    &ingredients,
                    &target_block,
                    optimal_input,
                    token_inventory,
                    self.sando_state_manager.get_searcher_address(),
                    self.sando_state_manager.get_sando_address(),
                    shared_backend,
                )?;
            },
            SandwichSwapType::Reverse => {
                optimal_input = find_optimal_input_reverse(
                    &ingredients,
                    &target_block,
                    token_inventory,
                    shared_backend.clone(),
                )
                .await?;

                recipe = create_recipe_reverse(
                    &ingredients,
                    &target_block,
                    optimal_input,
                    token_inventory,
                    self.sando_state_manager.get_searcher_address(),
                    self.sando_state_manager.get_sando_address(),
                    shared_backend,
                )?
            },
        };
        
        log_opportunity!(
            swap_type,
            ingredients.print_meats(),
            optimal_input.as_u128() as f64 / 1e18,
            recipe.get_revenue().as_u128() as f64 / 1e18,
            recipe.get_frontrun_gas_used(),
            recipe.get_backrun_gas_used()    
            
        );

        Ok(recipe)
    }


    pub async fn start_auto_process(&'static self, event_processor_num: i32, action_process_num: i32) -> Result<()> {

        #[cfg(feature = "debug")]
        {
            println!("bot start: event_processor_num={event_processor_num}, action_process_num={action_process_num}");
        }
        for _index in 0..event_processor_num {
            self.event_runtime.spawn(async move {
                loop {
                    match self.pop_event().await {
                        Some(event) => {
                            // #[cfg(feature = "debug")]
                            // {
                            //     println!("bot running: event processor {_index} process_event");
                            // }
                            let _ = self.process_event(event).await;
                        },
                        None => {
                            thread::sleep(time::Duration::from_millis(10));
                        },
                    }
                }
            });
        }
        log_info_cyan!("start {:?} event auto processors", event_processor_num);

        for _index in 0..action_process_num {
            self.action_runtime.spawn(async move {
                loop {
                    let action_sender = self.get_action_sender().await;
                    match action_sender {
                        Some(_) => {},
                        None => {
                            thread::sleep(time::Duration::from_millis(10));
                            continue;
                        }
                    }
                    match self.pop_action().await {
                        Some(action) => {
                            #[cfg(feature = "debug")]
                            {
                                println!("bot running: action processor {_index} process_event");
                            }
                            match action_sender.unwrap().send(action) {
                                Ok(_) => {},
                                Err(e) => error!("error sending action: {}", e),
                            }
                        },
                        None => {
                            thread::sleep(time::Duration::from_millis(10));
                        }
                    }
                }
            });
        }
        Ok(())
    }
    
}

#[async_trait]
impl<M: Middleware + 'static> Strategy<Event, Action> for SandoBot<M> {
    /// Setup by getting all pools to monitor for swaps
    async fn sync_state(&self) -> Result<()> {
        self.pool_manager.setup().await?;
        self.sando_state_manager
            .setup(self.provider.clone())
            .await?;
        self.block_manager.setup(self.provider.clone()).await?;
        Ok(())
    }

    async fn set_action_sender(&self, sender: Sender<Action>) -> Result<()> {

        let mut locker = self.action_sender.lock().unwrap();
        if locker.is_none() {
            *locker = Some(sender);
        }
        Ok(())
    }

    async fn set_event_sender(&self, sender: Sender<Event>) -> Result<()> {

        let mut locker = self.event_sender.lock().unwrap();
        if locker.is_none() {
            *locker = Some(sender);
        }
        Ok(())
    }

    async fn push_event(&self, event: Event) -> Result<()> {
        let mut locked_list = self.event_list.lock().unwrap();
        locked_list.push_back(event);
        Ok(())
    }

}

impl<M: Middleware + 'static> SandoBot<M> {

    async fn push_action(&self, action: Action) -> Result<()> {
        let mut locked_list = self.action_list.lock().unwrap();
        locked_list.push_back(action);
        Ok(())
    }

    /// Process incoming events
    async fn process_event(&self, event: Event) -> Result<()> {
        match event {
            Event::NewBlock(block) => self.process_new_block(block).await.unwrap(),
            Event::NewTransaction(tx) => {
                if let Some(action) = self.process_new_tx(tx).await {
                   self.push_action(action).await?;
                }
            }
        }

        Ok(())
    }

    async fn get_action_sender(&self) -> Option<Sender<Action>> {
    
        let locker = self.action_sender.lock().unwrap();
        return locker.clone();
    }

    async fn get_event_sender(&self) -> Option<Sender<Event>> {
    
        let locker = self.event_sender.lock().unwrap();
        return locker.clone();
    }

    async fn pop_event(&self) -> Option<Event> {
        let mut locked_list = self.event_list.lock().unwrap();
        if !locked_list.is_empty() {
            locked_list.pop_front()
        } else {
            None
        }
    }

    async fn pop_action(&self) -> Option<Action> {
        let mut locked_list = self.action_list.lock().unwrap();
        if !locked_list.is_empty() {
            locked_list.pop_front()
        } else {
            None
        }
    }

    /// Process new blocks as they come in
    async fn process_new_block(&self, event: NewBlock) -> Result<()> {
        log_new_block_info!(event);
        let base_fee_per_gas = event.base_fee_per_gas;
        self.update_block_info(event).await.unwrap();
        self.resend_low_txs(base_fee_per_gas).await.unwrap();
        
        Ok(())
    }

    async fn update_block_info(&self, new_block: NewBlock) -> Result<()> {

        let new_block_number = new_block.number;
        self.block_manager.update_block_info(new_block);
        match self.provider.get_block_with_txs(new_block_number).await? {
            Some(block) =>{
                self.pool_manager.update_block_info(&block);
                self.sando_state_manager.update_block_info(&block);
            },
            None =>{
                log_error!("Block not found");
            }
        }
        Ok(())
    }

    async fn resend_low_txs(&self, base_fee_per_gas: U256) -> Result<()> {

        match self.get_event_sender().await {
            Some(sender) => {
                let low_txs = self.sando_state_manager.get_low_txs(base_fee_per_gas);
                if !low_txs.is_empty() {
                    for tx in low_txs {
                        let hash = tx.hash;
                        match sender.send(Event::NewTransaction(tx)) {
                            Ok(_) => info!("resend low tx {}", hash),
                            Err(e) => error!("error resending low tx {}: {}", hash, e),
                        }
                    }
                }
            },
            None => {}
        }
        
        Ok(())
    }

    /// Process new txs as they come in
    #[allow(unused_mut)]
    async fn process_new_tx(& self, victim_tx: Transaction) -> Option<Action> {
        // setup variables for processing tx
        let next_block = self.block_manager.get_next_block();
        let latest_block = self.block_manager.get_latest_block();

        // ignore txs that we can't include in next block
        // enhancement: simulate all txs regardless, store result, and use result when tx can included
        if victim_tx.max_fee_per_gas.unwrap_or_default() < next_block.base_fee_per_gas {
            log_info_cyan!("{:?} mf<nbf", victim_tx.hash);
            self.sando_state_manager.append_low_tx(&victim_tx);
            return None;
        }

        if self.sando_state_manager.check_sig_id(&victim_tx) {
            // log_info_cyan!("{:?} approve", victim_tx.hash);
            return None;
        }
        
        
        // check if tx is a swap
        let (touched_pools, touched_pools_reverse) = self
            .pool_manager
            .get_touched_sandwichable_pools(
                &victim_tx,
                latest_block.number.into(),
                self.provider.clone(),
            )
            .await
            .map_err(|e| {
                log_error!("Failed to get touched sandwichable pools: {}", e);
                e
            })
            .ok()?;
        
        // no touched pools = no sandwich opps
        let mut sando_bundles = vec![];
        if !touched_pools.is_empty() {
            log_info_cyan!("process sandwich={:?}", victim_tx.hash);
            for pool in touched_pools {
                let (token_a, token_b) = match pool {
                    UniswapV2(p) => (p.token_a, p.token_b),
                    UniswapV3(p) => (p.token_a, p.token_b),
                };

                if token_a != *WETH_ADDRESS && token_b != *WETH_ADDRESS {
                    // contract can only sandwich weth pools
                    continue;
                }

                // token that we use as frontrun input and backrun output
                let start_end_token = *WETH_ADDRESS;

                // token that we use as frontrun output and backrun input
                let intermediary_token = if token_a == start_end_token {
                    token_b
                } else {
                    token_a
                };
                
                let ingredients = RawIngredients::new(
                    vec![],
                    vec![victim_tx.clone()],
                    start_end_token,
                    intermediary_token,
                    pool,
                );

                match self.is_sandwichable(ingredients, next_block.clone(), SandwichSwapType::Forward).await {
                    Ok(s) => {
                        let _bundle = match s
                            .to_fb_bundle(
                                self.sando_state_manager.get_sando_address(),
                                self.sando_state_manager.get_searcher_signer(),
                                false,
                                self.provider.clone(),
                            )
                            .await
                        {
                            Ok(b) => b,
                            Err(e) => {
                                log_not_sandwichable!("{:?}", e);
                                continue;
                            }
                        };

                        #[cfg(not(feature = "debug"))]
                        {
                            sando_bundles.push(_bundle);
                        }
                    }
                    Err(e) => {
                        log_not_sandwichable!("{:?} {:?}", victim_tx.hash, e)
                    }
                };
            }
        }

        if !touched_pools_reverse.is_empty() {
            log_info_cyan!("process reverse_sandwich={:?}", victim_tx.hash);
            for pool in touched_pools_reverse {
                let (token_a, token_b) = match pool {
                    UniswapV2(p) => (p.token_a, p.token_b),
                    UniswapV3(p) => (p.token_a, p.token_b),
                };

                if token_a != *WETH_ADDRESS && token_b != *WETH_ADDRESS {
                    // contract can only sandwich weth pools
                    continue;
                }

                // token that we use as frontrun output and backrun input
                let intermediary_token = *WETH_ADDRESS;

                // token that we use as frontrun input and backrun output
                let start_end_token = if token_a == intermediary_token {
                    token_b
                } else {
                    token_a
                };

                let ingredients = RawIngredients::new(
                    vec![],
                    vec![victim_tx.clone()],
                    start_end_token,
                    intermediary_token,
                    pool,
                );

                match self.is_sandwichable(ingredients, next_block.clone(), SandwichSwapType::Reverse).await {
                    Ok(s) => {
                        let _bundle = match s
                            .to_fb_bundle(
                                self.sando_state_manager.get_sando_address(),
                                self.sando_state_manager.get_searcher_signer(),
                                false,
                                self.provider.clone(),
                            )
                            .await
                        {
                            Ok(b) => b,
                            Err(e) => {
                                log_not_sandwichable!("{:?}", e);
                                continue;
                            }
                        };

                        #[cfg(not(feature = "debug"))]
                        {
                            sando_bundles.push(_bundle);
                        }
                    }
                    Err(e) => {
                        log_not_sandwichable!("{:?} {:?}", victim_tx.hash, e)
                    }
                };
            }
        }
        
        if sando_bundles.len() > 0 {
            return Some(Action::SubmitToFlashbots(sando_bundles));
        } else {
            // info!("{:?}", victim_tx.hash);
            return None;
        }
    }
}
