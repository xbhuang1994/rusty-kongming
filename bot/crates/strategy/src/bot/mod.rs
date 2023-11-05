use anyhow::{Result, anyhow};
use artemis_core::{collectors::block_collector::NewBlock, types::Strategy};
use async_trait::async_trait;
use cfmms::pool::Pool;
use cfmms::pool::Pool::{UniswapV2, UniswapV3};
use colored::Colorize;
use ethers::{
    providers::Middleware,
    types::{Transaction, Address, U256, H256}
};
use foundry_evm::executor::fork::{BlockchainDb, BlockchainDbMeta, SharedBackend};
use foundry_evm::revm::primitives::{B160, U256 as rU256};
use foundry_evm::revm::new;
use log::{error, info};
use tokio::task::JoinError;
use std::{collections::{BTreeSet, LinkedList, HashMap}, sync::{Arc,Mutex}, time, thread};
use tokio::{runtime, sync::broadcast::Sender};

use crate::types::{CalculateMaxFeeResult, IngredientsBundleResult};
use crate::{
    constants::WETH_ADDRESS,
    log_error, log_info_cyan, log_new_block_info, log_not_sandwichable, log_opportunity,
    managers::{
        block_manager::BlockManager, pool_manager::PoolManager,
        sando_state_manager::SandoStateManager, sando_recipe_manager::SandoRecipeManager,
    },
    simulator::{huff_sando::create_recipe, lil_router::find_optimal_input},
    simulator::{huff_sando_reverse::create_recipe_reverse, lil_router_reverse::find_optimal_input_reverse},
    simulator::huff_sando_huge::create_recipe_huge,
    types::{
        Action, BlockInfo, Event, RawIngredients, SandoRecipe, StratConfig, SandwichSwapType,
        calculate_bribe_for_max_fee
    },
    helpers::{calculate_inventory_for_debug, get_start_token_info, calculate_token_decimals}
};
use uuid::Uuid;

pub struct SandoBot<M> {
    /// Ethers client
    provider: Arc<M>,
    /// Keeps track of onchain pools
    pool_manager: PoolManager<M>,
    /// Block manager
    block_manager: BlockManager,
    /// Keeps track of weth inventory & token dust
    sando_state_manager: SandoStateManager,
    /// Keeps pendding sandoRecipes
    sando_recipe_manager: SandoRecipeManager,
    
    /// Auto process txs
    event_tx_runtime: Option<tokio::runtime::Runtime>,
    event_tx_list: Arc<Mutex<LinkedList<Transaction>>>,
    event_tx_sender: Arc<Mutex<Option<Sender<Event>>>>,

    /// Auto process newblock
    event_block_runtime: Option<tokio::runtime::Runtime>,
    event_block_list: Arc<Mutex<LinkedList<NewBlock>>>,

    /// Auto process action
    action_list: Arc<Mutex<LinkedList<Action>>>,
    action_runtime: Option<tokio::runtime::Runtime>,
    action_sender: Arc<Mutex<Option<Sender<Action>>>>,

    /// Auto process huge sandwich
    huge_task_list: Arc<Mutex<LinkedList<(HashMap<Pool, Vec<SandoRecipe>>, NewBlock)>>>,
    huge_task_runtime: Option<tokio::runtime::Runtime>,

    huge_mixed_task_list: Arc<Mutex<LinkedList<(HashMap<Pool, Vec<SandoRecipe>>, NewBlock)>>>,
    huge_mixed_task_runtime: Option<tokio::runtime::Runtime>,

    huge_overlay_task_list: Arc<Mutex<LinkedList<(HashMap<Pool, Vec<SandoRecipe>>, HashMap<Pool, Vec<SandoRecipe>>, NewBlock)>>>,
    huge_overlay_task_runtime: Option<tokio::runtime::Runtime>,

    processed_tx_map: Mutex<HashMap<H256, i64>>,
}

impl<M: Middleware + 'static> SandoBot<M> {
    /// Create a new instance
    pub fn new(client: Arc<M>, config: &StratConfig, need_runtime: bool) -> Self {
        Self {
            pool_manager: PoolManager::new(client.clone()),
            provider: client,
            block_manager: BlockManager::new(),
            sando_state_manager: SandoStateManager::new(
                config.sando_address,
                config.searcher_signer.clone(),
                config.sando_inception_block,
            ),
            sando_recipe_manager: SandoRecipeManager::new(),
            event_tx_runtime: if need_runtime {Some(runtime::Builder::new_multi_thread().worker_threads(32).enable_all().enable_time().build().unwrap())} else {None},
            event_tx_list: Arc::new(Mutex::new(LinkedList::new())),
            event_tx_sender: Arc::new(Mutex::new(None)),
            event_block_runtime: if need_runtime {Some(runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap())} else {None},
            event_block_list: Arc::new(Mutex::new(LinkedList::new())),
            action_list: Arc::new(Mutex::new(LinkedList::new())),
            action_runtime: if need_runtime {Some(runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap())} else {None},
            action_sender: Arc::new(Mutex::new(None)),
            processed_tx_map: Mutex::new(HashMap::new()),
            
            huge_task_list: Arc::new(Mutex::new(LinkedList::new())),
            huge_task_runtime: if need_runtime {Some(runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap())} else {None},
            huge_mixed_task_list: Arc::new(Mutex::new(LinkedList::new())),
            huge_mixed_task_runtime: if need_runtime {Some(runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap())} else {None},
            huge_overlay_task_list: Arc::new(Mutex::new(LinkedList::new())),
            huge_overlay_task_runtime: if need_runtime {Some(runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap())} else {None},
        }
    }

    fn fliter_recipes_by_swap_type(&self, recipes_map: &HashMap<Pool, Vec<SandoRecipe>>, swap_type: &SandwichSwapType) -> HashMap<Pool, Vec<SandoRecipe>> {

        let mut filtered_map: HashMap<Pool, Vec<SandoRecipe>> = HashMap::new();
        let pools: Vec<Pool> = recipes_map.iter().map(|(p, _)| p).cloned().collect();
        for pool in pools.iter() {
            let recipes = recipes_map.get(pool).unwrap();
            let filtered_recipes: Vec<SandoRecipe> = recipes.iter().filter(|r| r.get_swap_type() == *swap_type).cloned().collect();
            if filtered_recipes.len() > 0 {
                filtered_map.insert(*pool, filtered_recipes);
            }
        }

        filtered_map
    }

    fn sort_tx_by_none_group_from(&self, txs: &Vec<Transaction>) -> Vec<Transaction> {

        let mut group_from: HashMap<Address, Vec<Transaction>> = HashMap::new();
        for tx in txs {
            if group_from.contains_key(&tx.from) {
                group_from.get_mut(&tx.from).unwrap().push(tx.clone());
            } else {
                let v = vec![tx.clone()];
                group_from.insert(tx.from.clone(), v);
            }
        }
        let mut result = vec![];
        for (from, v) in group_from.iter_mut() {
            let before_len = v.len();
            v.sort_by_key(|t| t.nonce);
            v.dedup_by_key(|t| t.nonce);
            let after_len = v.len();
            if after_len != before_len {
                info!("delete duplicate tx nonce from {:?}", from);
            }
            v.iter().for_each(|t| result.push(t.clone()));
        }

        result
    }

    async fn find_same_swaptype_sandwichable_parallel(&'static self, recipes_map: &mut HashMap<Pool, Vec<SandoRecipe>>, target_block: BlockInfo)
        -> Vec<SandoRecipe> {

        let mut handlers = vec![];
        for (target_pool, recipes) in recipes_map.iter_mut() {
            if recipes.is_empty() {
                continue;
            }
            recipes.sort_by_key(|r| r.get_revenue());
            recipes.reverse();

            let start_end_token = recipes[0].get_start_end_token();
            let intermediary_token = recipes[0].get_intermediary_token();
            let swap_type = recipes[0].get_swap_type();

            let mut meats: Vec<Transaction> = recipes.iter()
                .flat_map(|recipe| recipe.get_meats().clone())
                .collect();

            if meats.len() > 1 {
                // delete duplicate tx with same hash
                meats.sort_by_key(|meat| meat.hash);
                meats.dedup_by_key(|meat| meat.hash);
            }
            if meats.len() > 1 {
                // sort by nonce and group by 'from'
                meats = self.sort_tx_by_none_group_from(&meats);
            }

            let mut head_txs: Vec<Transaction> = recipes.iter()
                .flat_map(|recipe| recipe.get_head_txs().clone())
                .collect();

            if head_txs.len() > 1 {
                // delete duplicate tx with same hash
                head_txs.sort_by_key(|recipe| recipe.hash);
                head_txs.dedup_by_key(|recipe| recipe.hash);
            }
            if head_txs.len() > 1 {
                // sort by nonce and group by 'from'
                head_txs = self.sort_tx_by_none_group_from(&head_txs);
            }

            let ingredients = RawIngredients::new(
                head_txs,
                meats,
                start_end_token,
                intermediary_token,
                *target_pool,
            );

            let handler = tokio::spawn(self.is_sandwichable(
                ingredients, target_block.clone(), swap_type, true, false
            ));
            handlers.push(handler);
        }
        let handler_results = futures::future::try_join_all(handlers).await;
        let optimal_recipes = match handler_results {
            Ok(recipe_results) => {
                let mut recipes: Vec<SandoRecipe> = vec![];
                for result in recipe_results {
                    match result {
                        Ok(recipe) => {
                            recipes.push(recipe.clone());
                        },
                        Err(_) => {}
                    }
                }
                recipes
            },
            Err(e) => {
                error!("One of the tasks panicked: {}", e);
                vec![]
            }
        };
        optimal_recipes
    }

    pub async fn make_huge_recpie(
        &self,
        final_recipes: &Vec<SandoRecipe>,
        target_block: BlockInfo,
        need_recheck_revenue: bool,
    ) -> Result<SandoRecipe> {

        let mut head_txs: Vec<Transaction> = Vec::new();
        let mut frontrun_data = Vec::new();
        let mut backrun_data = Vec::new();
        let mut meats: Vec<Transaction> = Vec::new();
        let mut meats_access_list: HashMap<H256, Vec<(B160, Vec<rU256>)>> = HashMap::new();
        let mut sando_weth_balance = U256::zero();
        let mut sando_tokens_balance: HashMap<Address, U256> = HashMap::new();
        let uuid = format!("{}", Uuid::new_v4());
        let mut log_swap_pair: Vec<String> = vec![];
        for recipe in final_recipes {
            let max_fee_result = calculate_bribe_for_max_fee(
                recipe.get_revenue(),
                recipe.get_frontrun_gas_used(),
                recipe.get_backrun_gas_used(),
                target_block.base_fee_per_gas,
                false
            );

            match max_fee_result {
                Ok((result, _)) => {
                    if !need_recheck_revenue || CalculateMaxFeeResult::RevenueOverBaseFee == result {
                        match recipe.get_frontrun_data() {
                            Some(data) => {
                                head_txs.extend(recipe.get_head_txs().clone());
                                frontrun_data.extend(data.clone());
                                backrun_data.extend(recipe.get_backrun().data.clone());
                                meats.extend(recipe.get_meats().clone());
                                for meat in recipe.get_meats() {
                                    meats_access_list.insert(meat.hash, recipe.get_frontrun().access_list.clone());
                                }
        
                                log_swap_pair.push(
                                    format!("{:?}->{:?}", recipe.get_start_end_token(), recipe.get_intermediary_token())
                                );
        
                                // set sando token balance for recipe creation
                                if recipe.get_start_end_token() == *WETH_ADDRESS {
                                    sando_weth_balance += recipe.get_frontrun_optimal_in() * 2;
        
                                    #[cfg(feature = "debug")]
                                    {
                                        // add some buffer, test if REVERT occur in backrun
                                        let balance = U256::from(10000u128).checked_mul(U256::from(1e18 as u128)).unwrap_or_default();
                                        info!("[make_huge_recpie] reset other token {:?} balance {:?}", recipe.get_intermediary_token(), balance);
                                        sando_tokens_balance.insert(recipe.get_intermediary_token().clone(), balance);
        
                                    }
        
                                } else {
                                    let mut balance = recipe.get_frontrun_optimal_in();
                                    if sando_tokens_balance.contains_key(&recipe.get_start_end_token()) {
                                        balance += *sando_tokens_balance.get(&recipe.get_start_end_token()).unwrap();
                                    }
                                    sando_tokens_balance.insert(recipe.get_start_end_token().clone(), balance);
                                }
                            },
                            None => {}
                        }
                    }
                },
                Err(e) => {
                    error!("calculating {:?} max fee error {}", recipe.get_uuid(), e);
                }
            }
        }
        if meats.is_empty() {
            return Err(anyhow!("no matched meat for making huge recipe"));
        }

        if meats.len() > 1 {
            // delete duplicate tx with same hash
            meats.sort_by_key(|meat| meat.hash);
            meats.dedup_by_key(|meat| meat.hash);
        }
        if meats.len() > 1 {
            // sort by nonce and group by 'from'
            meats = self.sort_tx_by_none_group_from(&meats);
        }
        
        if head_txs.len() > 1 {
            // delete duplicate tx with same hash
            head_txs.sort_by_key(|recipe| recipe.hash);
            head_txs.dedup_by_key(|recipe| recipe.hash);
        }
        if head_txs.len() > 1 {
            // sort by nonce group by 'from'
            head_txs = self.sort_tx_by_none_group_from(&head_txs);
        }

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

        info!("[make_huge_recpie] before create recipe uuid={:?} swap_pairs={:?}",
            uuid, log_swap_pair.join("|")
        );

        return create_recipe_huge(
            &target_block,
            frontrun_data.into(),
            backrun_data.into(),
            head_txs,
            meats,
            &meats_access_list,
            sando_weth_balance,
            sando_tokens_balance,
            self.sando_state_manager.get_searcher_address(),
            self.sando_state_manager.get_sando_address(),
            shared_backend.clone(),
            SandwichSwapType::Forward.clone(),
            uuid
        );
    }

    /// mixed all swap types and low revenue recipes
    async fn is_sandwichable_huge_overlay(&'static self,
        recipes_map: &mut HashMap<Pool, Vec<SandoRecipe>>,
        low_revenue_recipes_map: &mut HashMap<Pool, Vec<SandoRecipe>>,
        target_block: BlockInfo) -> Result<Vec<SandoRecipe>> {
        let swap_types = vec![SandwichSwapType::Forward, SandwichSwapType::Reverse];
        let mut optimal_recipes = vec![];
        for swap_type in swap_types.iter() {
            let mut filtered_recipes_map = self.fliter_recipes_by_swap_type(&recipes_map, swap_type);

            optimal_recipes.extend(self.find_same_swaptype_sandwichable_parallel(&mut filtered_recipes_map, target_block).await);
        }
        info!("[sandwich_huge_overlay] optimal recipes size {:?}", optimal_recipes.len());

        let mut low_revenue_recipes = vec![];
        for swap_type in swap_types.iter() {
            let mut filtered_low_recipes_map = self.fliter_recipes_by_swap_type(&low_revenue_recipes_map, swap_type);

            low_revenue_recipes.extend(self.find_same_swaptype_sandwichable_parallel(&mut filtered_low_recipes_map, target_block).await);
        }
        
        info!("[sandwich_huge_overlay] low revenue recipes size {:?}", low_revenue_recipes.len());
        if low_revenue_recipes.len() == 0 {
            return Ok(vec![]);
        }

        let mut optimal_forward_pools = vec![];
        let mut optimal_final_recipes = vec![];
        optimal_recipes.iter().filter(|r| r.get_swap_type() == SandwichSwapType::Forward)
            .for_each(|r| {
                optimal_forward_pools.push(r.get_target_pool().unwrap().clone());
                optimal_final_recipes.push(r.clone());
            }
        );

        let mut low_forward_pools = vec![];
        let mut low_final_recipes = vec![];
        low_revenue_recipes.iter().filter(|r| r.get_swap_type() == SandwichSwapType::Forward)
            .for_each(|r| {
                low_forward_pools.push(r.get_target_pool().unwrap().clone());
                low_final_recipes.push(r.clone());
            }
        );
        low_revenue_recipes.iter().filter(|r| r.get_swap_type() == SandwichSwapType::Reverse)
            .for_each(|r| {
                if !optimal_forward_pools.contains(&r.get_target_pool().unwrap())
                    && !low_forward_pools.contains(&r.get_target_pool().unwrap()){
                    low_final_recipes.push(r.clone());
                }
            }
        );
        if low_final_recipes.len() == 0 {
            info!("[sandwich_huge_overlay] low revenue final recipes is empty");
            return Ok(vec![]);
        }

        let mut huge_recipes = vec![];
        // check many low revenue recipes are sandwichable
        if low_final_recipes.len() > 1 {
            info!("[sandwich_huge_overlay] make huge recipe with all {:?} low revenues", low_final_recipes.len());
            let huge_recipe_result = self.make_huge_recpie(&low_final_recipes, target_block.clone(), false).await;
            match huge_recipe_result {
                Ok(recipe) => {
                    huge_recipes.push(recipe);
                },
                Err(e) => {
                    info!("[sandwich_huge_overlay] make huge recipe all low revenue error: {:?}", e);
                }
            }
        }
    
        optimal_recipes.iter().filter(|r| r.get_swap_type() == SandwichSwapType::Reverse)
            .for_each(|r| {
                if !optimal_forward_pools.contains(&r.get_target_pool().unwrap())
                    && !low_forward_pools.contains(&r.get_target_pool().unwrap()) {
                    optimal_final_recipes.push(r.clone());
                }
            }
        );
        info!("[sandwich_huge_overlay] fianl optimal recipes size {:?}, final low revenue recipes size {:?}",
            optimal_final_recipes.len(), low_final_recipes.len());
        
        if optimal_final_recipes.len() > 0 {

            let huge_recipe_result = self.make_huge_recpie_with_highest_profit(
                &optimal_final_recipes,
                &low_final_recipes,
                target_block.clone()).await;
            
            match huge_recipe_result {
                Some(recipe) => {
                    huge_recipes.push(recipe);
                },
                None => {
                    info!("[sandwich_huge_overlay] make huge overlay recipe contain low revenue fail");
                }
            }
        }

        Ok(huge_recipes)
    }

    pub async fn make_huge_recpie_with_highest_profit(
        &self,
        profitable_recipes: &Vec<SandoRecipe>,
        unprofitable_recipes: &Vec<SandoRecipe>,
        target_block: BlockInfo,
    ) -> Option<SandoRecipe> {

        let mut profix_max = U256::zero();
        let mut highest_profit_recipe: Option<SandoRecipe> = None;
        let mut total_final_recipes = profitable_recipes.clone();
        let huge_recipe_result = self.make_huge_recpie(&total_final_recipes, target_block.clone(), false).await;
        match huge_recipe_result {
            Ok(current_huge_recipe) => {
                match current_huge_recipe.clone().to_fb_bundle(
                    self.sando_state_manager.get_sando_address(),
                    self.sando_state_manager.get_searcher_signer(),
                    false,
                    self.provider.clone(),
                    true,
                    false,
                    false,
                    false,
                ).await {
                    Ok((_, _, current_profit_max)) => {
                        profix_max = current_profit_max;
                        highest_profit_recipe = Some(current_huge_recipe);
                    },
                    Err(e) => {
                        info!("[make_huge_recpie_with_highest_profit] cannot to fb bundle with optimal recipes {:?}", e);
                        return None;
                    }
                }
            },
            Err(e) => {
                info!("[make_huge_recpie_with_highest_profit] make huge recipe error with optimal recipes {:?}", e);
                return None;
            }
        }

        for recipe in unprofitable_recipes {
            let uuid = recipe.get_uuid();
            total_final_recipes.push(recipe.clone());
            let huge_recipe_result = self.make_huge_recpie(&total_final_recipes, target_block.clone(), false).await;
            match huge_recipe_result {
                Ok(current_huge_recipe) => {
                    match current_huge_recipe.clone().to_fb_bundle(
                        self.sando_state_manager.get_sando_address(),
                        self.sando_state_manager.get_searcher_signer(),
                        false,
                        self.provider.clone(),
                        true,
                        false,
                        false,
                        false,
                    ).await {
                        Ok((_, _, current_profit_max)) => {
                            if current_profit_max > profix_max {
                                highest_profit_recipe = Some(current_huge_recipe);
                            } else {
                                total_final_recipes.retain(|r| r.get_uuid() != uuid);
                            }
                        },
                        Err(e) => {
                            return None;
                        }
                    }
                },
                Err(e) => {
                    info!("[make_huge_recpie_with_highest_profit] make huge recipe contain low revenue error: {:?}", e);
                    return None;
                }
            }
        }

        if total_final_recipes.len() > profitable_recipes.len() {
            return highest_profit_recipe;
        } else {
            info!("[make_huge_recpie_with_highest_profit] no low recipe can be bundle");
            return None;
        }
    }
    
    /// recheck the pendding-recipes are sandwichable at new block and remake huge sandwich
    /// mixed all swap types
    async fn is_sandwichable_huge_mixed(&'static self, recipes_map: &mut HashMap<Pool, Vec<SandoRecipe>>, target_block: BlockInfo) -> Result<Vec<SandoRecipe>> {
        
        let swap_types = vec![SandwichSwapType::Forward, SandwichSwapType::Reverse];
        let mut optimal_recipes = vec![];
        for swap_type in swap_types.iter() {
            let mut filtered_recipes_map = self.fliter_recipes_by_swap_type(&recipes_map, swap_type);

            optimal_recipes.extend(self.find_same_swaptype_sandwichable_parallel(&mut filtered_recipes_map, target_block).await);
        }
        
        if optimal_recipes.len() == 0 {
            return Ok(vec![]);
        }
        info!("[sandwich_huge_mixed] optimal recipes size {:?}", optimal_recipes.len());
        let mut forward_pools = vec![];
        let mut optimal_final_recipes = vec![];
        optimal_recipes.iter().filter(|r| r.get_swap_type() == SandwichSwapType::Forward)
            .for_each(|r| {
                forward_pools.push(r.get_target_pool().unwrap().clone());
                optimal_final_recipes.push(r.clone());
            });
        let optimal_reverse_recipes: Vec<SandoRecipe> = optimal_recipes.iter().filter(|r| r.get_swap_type() == SandwichSwapType::Reverse).cloned().collect();
        let final_recipes_len = optimal_final_recipes.len();
        if optimal_final_recipes.is_empty() || optimal_reverse_recipes.is_empty() {
            info!("[sandwich_huge_mixed] one swap_type huge mixed is empty");
            return Ok(vec![]);
        }

        for recipe in optimal_reverse_recipes.iter() {
            if !forward_pools.contains(&recipe.get_target_pool().unwrap()) {
                optimal_final_recipes.push(recipe.clone());
            }
        }
        if optimal_final_recipes.len() == final_recipes_len {
            info!("[sandwich_huge_mixed] no matched reverse recipe");
            return Ok(vec![]);
        }

        let huge_recipe = self.make_huge_recpie(&optimal_final_recipes, target_block.clone(), true).await?;

        /*
        let mut head_txs: Vec<Transaction> = Vec::new();
        let mut frontrun_data = Vec::new();
        let mut backrun_data = Vec::new();
        let mut meats: Vec<Transaction> = Vec::new();
        let mut sando_weth_balance = U256::zero();
        let mut sando_tokens_balance: HashMap<Address, U256> = HashMap::new();
        for recipe in optimal_final_recipes {
            let max_fee_result = calculate_bribe_for_max_fee(
                recipe.get_revenue(),
                recipe.get_frontrun_gas_used(),
                recipe.get_backrun_gas_used(),
                target_block.base_fee_per_gas,
                false
            );
            match max_fee_result {
                Ok((result, _)) => {
                    match result {
                        CalculateMaxFeeResult::RevenueOverBaseFee => {
                            match recipe.get_frontrun_data() {
                                Some(data) => {
                                    head_txs.extend(recipe.get_head_txs().clone());
                                    frontrun_data.extend(data.clone());
                                    backrun_data.extend(recipe.get_backrun().data.clone());
                                    meats.extend(recipe.get_meats().clone());
        
                                    // set sando token balance for recipe creation
                                    if recipe.get_start_end_token() == *WETH_ADDRESS {
                                        sando_weth_balance += recipe.get_frontrun_optimal_in() * 2;
                                    } else {
                                        let mut balance = recipe.get_frontrun_optimal_in();
                                        if sando_tokens_balance.contains_key(&recipe.get_start_end_token()) {
                                            balance += *sando_tokens_balance.get(&recipe.get_start_end_token()).unwrap();
                                        }
                                        sando_tokens_balance.insert(recipe.get_start_end_token().clone(), balance);
                                    }
                                },
                                None => {}
                            }
                        },
                        _ => {}
                    }
                },
                Err(e) => {
                    error!("calculating {:?} max fee error {}", recipe.get_uuid(), e);
                }
            }
        }
        if meats.is_empty() {
            info!("[sandwich_huge_mixed] no matched meat for recipe");
            return Ok(vec![]);
        }

        if meats.len() > 1 {
            // delete duplicate tx with same hash
            meats.sort_by_key(|meat| meat.hash);
            meats.dedup_by_key(|meat| meat.hash);
        }
        if meats.len() > 1 {
            // sort by nonce and group by 'from'
            meats = self.sort_tx_by_none_group_from(&meats);
        }
        
        if head_txs.len() > 1 {
            // delete duplicate tx with same hash
            head_txs.sort_by_key(|recipe| recipe.hash);
            head_txs.dedup_by_key(|recipe| recipe.hash);
        }
        if head_txs.len() > 1 {
            // sort by nonce group by 'from'
            head_txs = self.sort_tx_by_none_group_from(&head_txs);
        }

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

        let huge_recipe = create_recipe_huge(
            &target_block,
            frontrun_data.into(),
            backrun_data.into(),
            head_txs,
            meats,
            sando_weth_balance,
            sando_tokens_balance,
            self.sando_state_manager.get_searcher_address(),
            self.sando_state_manager.get_sando_address(),
            shared_backend.clone(),
            SandwichSwapType::Forward.clone(),
            format!("{}", Uuid::new_v4())
        )?;
        */

        Ok(vec![huge_recipe])
    }

    /// recheck the pendding-recipes are sandwichable at new block and remake huge sandwich
    /// group by swap type
    async fn is_sandwichable_huge(&'static self, recipes_map: &mut HashMap<Pool, Vec<SandoRecipe>>, target_block: BlockInfo) -> Result<Vec<SandoRecipe>> {

        let swap_types = vec![SandwichSwapType::Forward, SandwichSwapType::Reverse];
        let mut huge_recipes = vec![];

        for swap_type in swap_types.iter() {

            let mut filtered_recipes_map = self.fliter_recipes_by_swap_type(&recipes_map, swap_type);
            let optimal_recipes = self.find_same_swaptype_sandwichable_parallel(&mut filtered_recipes_map, target_block.clone()).await;
            if optimal_recipes.len() == 0 {
                continue;
            }
            info!("[sandwich_huge] optimal recipes size {:?} swap type {:?}", optimal_recipes.len(), swap_type);
            let huge_recipe = self.make_huge_recpie(&optimal_recipes, target_block.clone(), true).await?;

            // let mut head_txs: Vec<Transaction> = Vec::new();
            // let mut frontrun_data = Vec::new();
            // let mut backrun_data = Vec::new();
            // let mut meats: Vec<Transaction> = Vec::new();
            // let mut sando_weth_balance = U256::zero();
            // let mut sando_tokens_balance: HashMap<Address, U256> = HashMap::new();
            // for recipe in optimal_recipes {
            //     let max_fee_result = calculate_bribe_for_max_fee(
            //         recipe.get_revenue(),
            //         recipe.get_frontrun_gas_used(),
            //         recipe.get_backrun_gas_used(),
            //         target_block.base_fee_per_gas,
            //         false
            //     );
            //     match max_fee_result {
            //         Ok((result, _)) => {
            //             match result {
            //                 CalculateMaxFeeResult::RevenueOverBaseFee => {
            //                     match recipe.get_frontrun_data() {
            //                         Some(data) => {
            //                             head_txs.extend(recipe.get_head_txs().clone());
            //                             frontrun_data.extend(data.clone());
            //                             backrun_data.extend(recipe.get_backrun().data.clone());
            //                             meats.extend(recipe.get_meats().clone());
        
            //                             // set sando token balance for recipe creation
            //                             if recipe.get_start_end_token() == *WETH_ADDRESS {
            //                                 sando_weth_balance += recipe.get_frontrun_optimal_in() * 2;
                                            
            //                                 #[cfg(feature = "debug")]
            //                                 {
            //                                     // add some buffer, test if REVERT occur in backrun
            //                                     let balance = U256::from(10000u128).checked_mul(U256::from(1e18 as u128)).unwrap_or_default();
            //                                     info!("[sandwich_huge] reset other token {:?} balance {:?}", recipe.get_intermediary_token(), balance);
            //                                     sando_tokens_balance.insert(recipe.get_intermediary_token().clone(), balance);
        
            //                                 }
            //                             } else {
            //                                 let mut balance = recipe.get_frontrun_optimal_in();
            //                                 if sando_tokens_balance.contains_key(&recipe.get_start_end_token()) {
            //                                     balance += *sando_tokens_balance.get(&recipe.get_start_end_token()).unwrap();
            //                                 }
            //                                 sando_tokens_balance.insert(recipe.get_start_end_token().clone(), balance);
            //                             }
            //                         },
            //                         None => {}
            //                     }
            //                 },
            //                 _ => {}
            //             }
            //         },
            //         Err(e) => {
            //             error!("calculating {:?} max fee error {}", recipe.get_uuid(), e);
            //         }
            //     }
            // }
            // if meats.is_empty() {
            //     continue;
            // }

            // if meats.len() > 1 {
            //     // delete duplicate tx with same hash
            //     meats.sort_by_key(|meat| meat.hash);
            //     meats.dedup_by_key(|meat| meat.hash);
            // }
            // if meats.len() > 1 {
            //     // sort by nonce and group by 'from'
            //     meats = self.sort_tx_by_none_group_from(&meats);
            // }
            
            // if head_txs.len() > 1 {
            //     // delete duplicate tx with same hash
            //     head_txs.sort_by_key(|recipe| recipe.hash);
            //     head_txs.dedup_by_key(|recipe| recipe.hash);
            // }
            // if head_txs.len() > 1 {
            //     // sort by nonce group by 'from'
            //     head_txs = self.sort_tx_by_none_group_from(&head_txs);
            // }

            // let shared_backend = SharedBackend::spawn_backend_thread(
            //     self.provider.clone(),
            //     BlockchainDb::new(
            //         BlockchainDbMeta {
            //             cfg_env: Default::default(),
            //             block_env: Default::default(),
            //             hosts: BTreeSet::from(["".to_string()]),
            //         },
            //         None,
            //     ), /* default because not accounting for this atm */
            //     Some((target_block.number - 1).into()),
            // );

            // let huge_recipe = create_recipe_huge(
            //     &target_block,
            //     frontrun_data.into(),
            //     backrun_data.into(),
            //     head_txs,
            //     meats,
            //     sando_weth_balance,
            //     sando_tokens_balance,
            //     self.sando_state_manager.get_searcher_address(),
            //     self.sando_state_manager.get_sando_address(),
            //     shared_backend.clone(),
            //     swap_type.clone(),
            //     format!("{}", Uuid::new_v4())
            // )?;

            info!("[sandwich_huge] make sandwich huge {:?}", huge_recipe.get_uuid());
            huge_recipes.push(huge_recipe);
        }

        Ok(huge_recipes)
    }

    /// Main logic for the strategy
    /// Checks if the passed `RawIngredients` is sandwichable
    pub async fn is_sandwichable(
        &self,
        ingredients: RawIngredients,
        target_block: BlockInfo,
        swap_type: SandwichSwapType,
        for_huge: bool,
        check_nonce_valid: bool,
    ) -> Result<SandoRecipe> {
        // check if meat nonce is valid
        if check_nonce_valid {
            for meat in ingredients.get_meats_ref() {
                let tx_nonce = self.provider.get_transaction_count(meat.from, None).await?;
                if tx_nonce != meat.nonce {
                    return Err(anyhow!("meat nonce {:?} not match tx nonce {:?} from {:?}", tx_nonce, meat.nonce, meat.from));
                }
            }
        }
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
        let (token_symbol, token_decimals, token_inventory) = if cfg!(feature = "debug") {
            // spoof weth balance when the debug feature is active
            // (*crate::constants::WETH_FUND_AMT).into()
            calculate_inventory_for_debug(&ingredients)
        } else {
            let (symbol, decimals) = get_start_token_info(&ingredients);
            let inventory = 
            if swap_type == SandwichSwapType::Forward {
                self.sando_state_manager.get_weth_inventory()
            } else {
                self.sando_state_manager.get_token_inventory(
                    ingredients.get_start_end_token(),
                    self.provider.clone()
                ).await
            };
            (symbol, decimals, inventory)
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
                    shared_backend.clone(),
                )?;
            },
            SandwichSwapType::Reverse => {
                let (_optimal_input, other_diff_max) = find_optimal_input_reverse(
                    &ingredients,
                    &target_block,
                    token_inventory,
                    shared_backend.clone(),
                )
                .await?;
                optimal_input = _optimal_input;

                recipe = create_recipe_reverse(
                    &ingredients,
                    &target_block,
                    optimal_input,
                    token_inventory,
                    U256::from(1e18 as u128),
                    other_diff_max,
                    self.sando_state_manager.get_searcher_address(),
                    self.sando_state_manager.get_sando_address(),
                    shared_backend.clone(),
                )?
            },
        };
        
        log_opportunity!(
            for_huge,
            ingredients.get_uuid(),
            swap_type,
            token_symbol,
            ingredients.print_head_txs(),
            ingredients.print_meats(),
            optimal_input.as_u128() as f64 / calculate_token_decimals(token_decimals) as f64,
            recipe.get_revenue().as_u128() as f64 / 1e18,
            recipe.get_frontrun_gas_used(),
            recipe.get_backrun_gas_used() 
        );

        Ok(recipe)
    }


    pub async fn start_auto_process(&'static self, tx_processor_num: i32, block_process_num: i32, action_process_num: i32, huge_process_num: i32) -> Result<()> {

        match &self.event_tx_runtime {
            Some(rt) => {
                for _index in 0..tx_processor_num {
                    rt.spawn(async move {
                        let mut _count = 0;
                        loop {
                            match self.pop_event_tx().await {
                                Some(event) => {
                                    // #[cfg(feature = "debug")]
                                    // {
                                    //     info!("bot running: event tx processor {_index} process_event");
                                    // }
                                    match self.process_event_tx(event).await {
                                        Ok(_) => {},
                                        Err(e) => error!("bot running event tx processor {_index} error {}", e)
                                    }
                                },
                                None => {
                                    tokio::time::sleep(time::Duration::from_millis(10)).await;
                                },
                            }
                        }
                    });
                }
                info!("start {:?} event tx auto processors", tx_processor_num);
            },
            None => {
                return Err(anyhow!("event tx runtime is none"));
            }
        }


        match &self.event_block_runtime {
            Some(rt) => {
                for _index in 0..block_process_num {
                    rt.spawn(async move {
                        loop {
                            match self.pop_event_block().await {
                                Some(event) => {
                                    let _ = self.process_event_block(event).await;
                                },
                                None => {
                                    tokio::time::sleep(time::Duration::from_millis(100)).await;
                                }
                            }
                        }
                    });
                }
                info!("start {:?} event block auto processors", block_process_num);
            },
            None => {
                return Err(anyhow!("event block runtime is none"));
            }
        }


        match &self.action_runtime {
            Some(rt) => {
                for _index in 0..action_process_num {
                    rt.spawn(async move {
                        loop {
                            let action_sender = self.get_action_sender().await;
                            match action_sender {
                                Some(_) => {},
                                None => {
                                    tokio::time::sleep(time::Duration::from_millis(10)).await;
                                    continue;
                                }
                            }
                            match self.pop_action().await {
                                Some(action) => {
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
                info!("start {:?} action auto processors", action_process_num);
            },
            None => {
                return Err(anyhow!("action runtime is none"));
            }
        }
        

        match &self.huge_task_runtime {
            Some(rt) => {
                for _index in 0..huge_process_num {
                    rt.spawn(async move {
                        loop {
                            match self.pop_huge_task().await {
                                Some((recipes_map, new_block)) => {
        
                                    let new_block_info = BlockInfo{
                                        number: new_block.number,
                                        base_fee_per_gas: new_block.base_fee_per_gas,
                                        timestamp: new_block.timestamp,
                                        gas_used: Some(new_block.gas_used),
                                        gas_limit: Some(new_block.gas_limit),
                                    };
                                    let target_block = new_block_info.get_next_block();
        
                                    match self.is_sandwichable_huge(&mut recipes_map.clone(), target_block).await {
        
                                        Ok(huge_recipes) => {
        
                                            let mut bundles = vec![];
                                            for huge in huge_recipes {
                                                match huge.to_fb_bundle(
                                                    self.sando_state_manager.get_sando_address(),
                                                    self.sando_state_manager.get_searcher_signer(),
                                                    false,
                                                    self.provider.clone(),
                                                    true,
                                                    false,
                                                    false,
                                                    true,
                                                ).await {
                                                    Ok((_, bundle_option, _profit_max)) => {
                                                        match bundle_option {
                                                            Some(bundle) => {
                                                                bundles.push(bundle);
                                                            },
                                                            None => {}
                                                        }
                                                    },
                                                    Err(e) => {
                                                        error!("fail make huge sandwich error:{}", e)
                                                    }
                                                }
                                            }
                                            if bundles.len() > 0 {
                                                self.push_action(Action::SubmitToFlashbots(bundles)).await.unwrap();
                                            }
                                        },
                                        Err(e) => {
                                            error!("process huge sandwich error: {}", e);
                                        }
                                    }
                                },
                                None => {
                                    tokio::time::sleep(time::Duration::from_millis(50)).await;
                                }
                            }
                        }
                    });
                }
                info!("start {:?} huge auto processors", huge_process_num);
            },
            None => {
                return Err(anyhow!("huge task runtime is none"));
            }
        }
        

        match &self.huge_mixed_task_runtime {
            Some(rt) => {
                for _index in 0..huge_process_num {
                    rt.spawn(async move {
                        loop {
                            match self.pop_huge_mixed_task().await {
                                Some((recipes_map, new_block)) => {
        
                                    let new_block_info = BlockInfo{
                                        number: new_block.number,
                                        base_fee_per_gas: new_block.base_fee_per_gas,
                                        timestamp: new_block.timestamp,
                                        gas_used: Some(new_block.gas_used),
                                        gas_limit: Some(new_block.gas_limit),
                                    };
                                    let target_block = new_block_info.get_next_block();
        
                                    match self.is_sandwichable_huge_mixed(&mut recipes_map.clone(), target_block).await {
        
                                        Ok(huge_recipes) => {
        
                                            let mut bundles = vec![];
                                            for huge in huge_recipes {
                                                match huge.to_fb_bundle(
                                                    self.sando_state_manager.get_sando_address(),
                                                    self.sando_state_manager.get_searcher_signer(),
                                                    false,
                                                    self.provider.clone(),
                                                    true,
                                                    true,
                                                    false,
                                                    true,
                                                ).await {
                                                    Ok((_, bundle_option, _profit_max)) => {
                                                        match bundle_option {
                                                            Some(bundle) => {
                                                                bundles.push(bundle);
                                                            },
                                                            None => {}
                                                        }
                                                    },
                                                    Err(e) => {
                                                        error!("fail make huge mixed sandwich error:{}", e)
                                                    }
                                                }
                                            }
                                            if bundles.len() > 0 {
                                                self.push_action(Action::SubmitToFlashbots(bundles)).await.unwrap();
                                            }
                                        },
                                        Err(e) => {
                                            error!("process huge mixed sandwich error: {}", e);
                                        }
                                    }
                                },
                                None => {
                                }
                            }
                        }
                    });
                }
                info!("start {:?} huge mixed auto processors", huge_process_num);
            },
            None => {
                return Err(anyhow!("huge mixed task runtime is none"));
            }
        }
        

        match &self.huge_overlay_task_runtime {
            Some(rt) => {
                for _index in 0..huge_process_num {
                    rt.spawn(async move {
                        loop {
                            match self.pop_huge_overlay_task().await {
                                Some((pendding_recipes_map, low_revenue_recipes_map, new_block)) => {
                                    if low_revenue_recipes_map.len() == 0 {
                                        info!("overlay low revenue recipes is empty");
                                        tokio::time::sleep(time::Duration::from_millis(50)).await;
                                        continue;
                                    }
                                    let new_block_info = BlockInfo{
                                        number: new_block.number,
                                        base_fee_per_gas: new_block.base_fee_per_gas,
                                        timestamp: new_block.timestamp,
                                        gas_used: Some(new_block.gas_used),
                                        gas_limit: Some(new_block.gas_limit),
                                    };
                                    let target_block = new_block_info.get_next_block();
                                    match self.is_sandwichable_huge_overlay(&mut pendding_recipes_map.clone(), &mut low_revenue_recipes_map.clone(), target_block).await {
        
                                        Ok(huge_recipes) => {
        
                                            let mut bundles = vec![];
                                            for huge in huge_recipes {
                                                match huge.to_fb_bundle(
                                                    self.sando_state_manager.get_sando_address(),
                                                    self.sando_state_manager.get_searcher_signer(),
                                                    false,
                                                    self.provider.clone(),
                                                    true,
                                                    false,
                                                    true,
                                                    true,
                                                ).await {
                                                    Ok((_, bundle_option, _profit_max)) => {
                                                        match bundle_option {
                                                            Some(bundle) => {
                                                                bundles.push(bundle);
                                                            },
                                                            None => {}
                                                        }
                                                    },
                                                    Err(e) => {
                                                        error!("fail make huge overlay sandwich error:{}", e)
                                                    }
                                                }
                                            }
                                            if bundles.len() > 0 {
                                                self.push_action(Action::SubmitToFlashbots(bundles)).await.unwrap();
                                            }
                                        },
                                        Err(e) => {
                                            error!("process huge overlay sandwich error: {}", e);
                                        }
                                    }
                                },
                                None => {
                                    tokio::time::sleep(time::Duration::from_millis(50)).await;
                                }
                            }
                        }
                    });
                }
                info!("start {:?} huge overlay auto processors", huge_process_num);
            },
            None => {
                return Err(anyhow!("huge overlay task runtime is none"));
            }
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

        let mut locker = self.event_tx_sender.lock().unwrap();
        if locker.is_none() {
            *locker = Some(sender);
        }
        Ok(())
    }

    async fn push_event(&self, event: Event) -> Result<()> {
        match event {
            Event::NewBlock(block) => {
                let mut list_block = self.event_block_list.lock().unwrap();
                list_block.push_back(block);
            },
            Event::NewTransaction(tx) => {
                let mut list_tx = self.event_tx_list.lock().unwrap();
                let mut tx = tx.clone();
                // tx.from is 'zero' receive from WS, so reset it
                match tx.recover_from_mut(){
                    Ok(_) => {
                        list_tx.push_back(tx);
                    },
                    Err(e) => error!("failed to recover from victim tx: {}", e),
                }
            },
        }

        Ok(())
    }

}

impl<M: Middleware + 'static> SandoBot<M> {

    async fn push_action(&self, action: Action) -> Result<()> {
        let mut list_action = self.action_list.lock().unwrap();
        list_action.push_back(action);
        Ok(())
    }

    /// Process incoming events of transaction
    async fn process_event_tx(&self, event: Transaction) -> Result<()> {
        // info!("proc tx {:?}", event.hash);
        if let Some(action) = self.process_new_tx(event).await {
           self.push_action(action).await?;
        }

        Ok(())
    }

    /// Process incoming events of newblock
    async fn process_event_block(&self, event: NewBlock) -> Result<()> {
        // info!("proc newblock {:?}", event.number);
        self.process_new_block(event.clone()).await.unwrap();

        // sleep 10.5 seconds wait for refresh pendding recepies, then make huge bundle
        // info!("before process pendding recipes");
        tokio::time::sleep(time::Duration::from_millis(10_500)).await;
        self.process_pendding_recipes(event.clone()).await.unwrap();
        Ok(())
    }

    async fn process_pendding_recipes(&self, event: NewBlock) -> Result<()> {
        let pendding_recipes_group = self.sando_recipe_manager.get_all_pendding_recipes(false);
        info!("start process pendding recipes {:?} groups by pool with simple strategy", pendding_recipes_group.len());
        self.push_huge_task(pendding_recipes_group, event.clone()).await.unwrap();
        
        let pendding_recipes_group = self.sando_recipe_manager.get_all_pendding_recipes(false);
        info!("start process pendding recipes {:?} groups by pool with mixed strategy", pendding_recipes_group.len());
        self.push_huge_mixed_task(pendding_recipes_group, event.clone()).await.unwrap();
        
        let pendding_recipes_group = self.sando_recipe_manager.get_all_pendding_recipes(false);
        let low_revenue_recipes_group = self.sando_recipe_manager.get_all_low_revenue_recipes(false);
        info!("start process pendding recipes {:?} groups and low revenue recipes {:?} groups by pool with overlay strategy",
            pendding_recipes_group.len(), low_revenue_recipes_group.len());
        self.push_huge_overlay_task(pendding_recipes_group, low_revenue_recipes_group, event.clone()).await.unwrap();
        
        Ok(())
    }

    async fn pop_huge_overlay_task(&self) ->
        Option<(HashMap<Pool, Vec<SandoRecipe>>, HashMap<Pool, Vec<SandoRecipe>>, NewBlock)> {
        let mut task_list = self.huge_overlay_task_list.lock().unwrap();
        if task_list.len() > 0 {
            task_list.pop_front()
        } else {
            None
        }
    }

    async fn push_huge_overlay_task(&self,
        recipes_maps: HashMap<Pool, Vec<SandoRecipe>>,
        low_revenue_recipes_maps: HashMap<Pool, Vec<SandoRecipe>>,
        new_block: NewBlock
    ) -> Result<()> {
        let mut task_list = self.huge_overlay_task_list.lock().unwrap();
        task_list.push_back((recipes_maps, low_revenue_recipes_maps, new_block));
        Ok(())
    }

    async fn pop_huge_task(&self) -> Option<(HashMap<Pool, Vec<SandoRecipe>>, NewBlock)> {
        let mut task_list = self.huge_task_list.lock().unwrap();
        if task_list.len() > 0 {
            task_list.pop_front()
        } else {
            None
        }
    }

    async fn push_huge_task(&self, recipes_maps: HashMap<Pool, Vec<SandoRecipe>>, new_block: NewBlock) -> Result<()> {
        let mut task_list = self.huge_task_list.lock().unwrap();
        task_list.push_back((recipes_maps, new_block));
        Ok(())
    }

    async fn pop_huge_mixed_task(&self) -> Option<(HashMap<Pool, Vec<SandoRecipe>>, NewBlock)> {
        let mut task_list = self.huge_mixed_task_list.lock().unwrap();
        if task_list.len() > 0 {
            task_list.pop_front()
        } else {
            None
        }
    }

    async fn push_huge_mixed_task(&self, recipes_maps: HashMap<Pool, Vec<SandoRecipe>>, new_block: NewBlock) -> Result<()> {
        let mut task_list = self.huge_mixed_task_list.lock().unwrap();
        task_list.push_back((recipes_maps, new_block));
        Ok(())
    }

    async fn get_action_sender(&self) -> Option<Sender<Action>> {
    
        let locker = self.action_sender.lock().unwrap();
        return locker.clone();
    }

    async fn get_event_sender(&self) -> Option<Sender<Event>> {
    
        let locker = self.event_tx_sender.lock().unwrap();
        return locker.clone();
    }

    async fn pop_event_tx(&self) -> Option<Transaction> {
        let mut list_tx = self.event_tx_list.lock().unwrap();
        if !list_tx.is_empty() {
            list_tx.pop_front()
        } else {
            None
        }
    }

    async fn pop_event_block(&self) -> Option<NewBlock> {
        let mut list_block = self.event_block_list.lock().unwrap();
        if !list_block.is_empty() {
            list_block.pop_front()
        } else {
            None
        }
    }

    async fn pop_action(&self) -> Option<Action> {
        let mut list_action = self.action_list.lock().unwrap();
        if !list_action.is_empty() {
            list_action.pop_front()
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
                let mut block_txs: Vec<Transaction> = Vec::new();
                for mut tx in block.transactions {
                    match tx.recover_from_mut(){
                        Ok(_) => {
                            block_txs.push(tx.clone());
                        },
                        Err(e) => error!("failed to recover from block tx: {}", e),
                    }
                }
                self.pool_manager.update_block_info(&block_txs);
                self.sando_state_manager.update_block_info(&block_txs);
                self.sando_recipe_manager.update_pendding_recipe(&block_txs);
                self.sando_recipe_manager.update_low_revenue_recipe(&block_txs);
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
                            Ok(_) => {/*info!("resent low tx {:?}", hash);*/},
                            Err(e) => error!("error resending low tx {:?}: {}", hash, e),
                        }
                    }
                }
            },
            None => {}
        }
        
        Ok(())
    }

    fn check_tx_processed(&self, tx_hash: H256) -> bool {

        let mut hist_txs = self.processed_tx_map.lock().unwrap();
        let mut processed = false;
        let now_ts = chrono::Local::now().timestamp();
        if hist_txs.contains_key(&tx_hash.clone()) {
            processed = true;
            let hist_ts = hist_txs.get(&tx_hash.clone()).unwrap_or(&0);
            if now_ts - hist_ts > 7200 {
                // cache transactions for 3 hours
                hist_txs.remove(&tx_hash.clone());
                info!("remove tx:{:?}", tx_hash);
            }
        } else {
            hist_txs.insert(tx_hash, now_ts);
        }

        processed
    }

    /// Process new txs as they come in
    #[allow(unused_mut)]
    async fn process_new_tx(& self, victim_tx: Transaction) -> Option<Action> {
        // setup variables for processing tx
        let next_block = self.block_manager.get_next_block();
        let latest_block = self.block_manager.get_latest_block();

        // ignore txs that we can't include in next block
        // enhancement: simulate all txs regardless, store result, and use result when tx can included
        if victim_tx.max_fee_per_gas.unwrap_or_default() < next_block.base_fee_per_gas || victim_tx.max_fee_per_gas.unwrap_or_default() < latest_block.base_fee_per_gas {
            // log_info_cyan!("{:?} mf<nbf", victim_tx.hash);
            self.sando_state_manager.append_low_tx(&victim_tx);
            return None;
        }

        if self.sando_state_manager.check_approve_by_signature(&victim_tx) {
            // log_info_cyan!("{:?} is approve tx", victim_tx.hash);
            return None;
        }

        if self.sando_state_manager.check_liquidity_by_signature(&victim_tx) {
            // todo! 
            // log_info_cyan!("{:?} is liquidity tx", victim_tx.hash);
            return None;
        }

        // check if tx had been processed
        if self.check_tx_processed(victim_tx.hash) {
            // info!("{:?} had processed", victim_tx.hash);
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
            for pool in touched_pools {
                let (head_hashs, head_txs) = self.sando_state_manager.get_head_txs(&victim_tx.from, pool.address(), SandwichSwapType::Forward);
                log_info_cyan!("process sandwich {:?} from {:?} nonce {:?} pool {:?} head_txs {:?}", victim_tx.hash, victim_tx.from, victim_tx.nonce, pool.address(), head_hashs.join(","));
                
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
                    head_txs,
                    vec![victim_tx.clone()],
                    start_end_token,
                    intermediary_token,
                    pool,
                );

                match self.is_sandwichable(
                        ingredients,
                        next_block.clone(),
                        SandwichSwapType::Forward,
                        false,
                        true
                    ).await {
                    Ok(s) => {
                        let mut cloned_recipe = s.clone();
                        match s.to_fb_bundle(
                                self.sando_state_manager.get_sando_address(),
                                self.sando_state_manager.get_searcher_signer(),
                                false,
                                self.provider.clone(),
                                false,
                                false,
                                false,
                                true,
                            )
                            .await
                        {
                            Ok((result, bundle_option, profit_max)) => {
                                match result {
                                    IngredientsBundleResult::ExpectedProfitIsPositive => {
                                        match bundle_option {
                                            Some(bundle) => {
                                                cloned_recipe.set_profit_max(profit_max);
                                                sando_bundles.push(bundle);
                                                self.sando_recipe_manager.push_pendding_recipe(cloned_recipe);
                                            },
                                            None => {}
                                        };
                                    },
                                    IngredientsBundleResult::RevenueBelowBaseFee | IngredientsBundleResult::ExpectedProfitIsNegtive => {
                                        self.sando_recipe_manager.push_low_revenue_recipe(cloned_recipe);
                                    },
                                }
                            },
                            Err(e) => {
                                log_not_sandwichable!("{:?}", e);
                                continue;
                            }
                        };
                    },
                    Err(e) => {
                        log_not_sandwichable!("{:?} {:?}", victim_tx.hash, e)
                    }
                };
            }
        }

        if !touched_pools_reverse.is_empty() {
            for pool in touched_pools_reverse {
                let (head_hashs, head_txs) = self.sando_state_manager.get_head_txs(&victim_tx.from, pool.address(), SandwichSwapType::Reverse);
                log_info_cyan!("process reverse sandwich {:?} from {:?} nonce {:?} pool {:?} head_txs {:?}", victim_tx.hash, victim_tx.from, victim_tx.nonce, pool.address(), head_hashs.join(","));
                
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
                    head_txs,
                    vec![victim_tx.clone()],
                    start_end_token,
                    intermediary_token,
                    pool,
                );

                match self.is_sandwichable(
                        ingredients,
                        next_block.clone(),
                        SandwichSwapType::Reverse,
                        false,
                        true
                    ).await {
                    Ok(s) => {
                        let mut cloned_recipe = s.clone();
                        match s.to_fb_bundle(
                                self.sando_state_manager.get_sando_address(),
                                self.sando_state_manager.get_searcher_signer(),
                                false,
                                self.provider.clone(),
                                false,
                                false,
                                false,
                                true,
                            )
                            .await
                        {
                            Ok((result, bundle_option, profit_max)) => {
                                match result {
                                    IngredientsBundleResult::ExpectedProfitIsPositive => {
                                        match bundle_option {
                                            Some(bundle) => {
                                                cloned_recipe.set_profit_max(profit_max);
                                                sando_bundles.push(bundle);
                                                self.sando_recipe_manager.push_pendding_recipe(cloned_recipe);
                                            },
                                            None => {}
                                        };
                                    },
                                    IngredientsBundleResult::RevenueBelowBaseFee | IngredientsBundleResult::ExpectedProfitIsNegtive => {
                                        self.sando_recipe_manager.push_low_revenue_recipe(cloned_recipe);
                                    },
                                }
                            },
                            Err(e) => {
                                log_not_sandwichable!("{:?}", e);
                                continue;
                            }
                        };
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
