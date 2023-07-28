use ethers::prelude::*;

use crate::utils;
use crate::prelude::fork_factory::ForkFactory;
use crate::prelude::sandwich_types::RawIngredients;
use crate::types::sandwich_types::OptimalRecipe;
use crate::types::{BlockInfo, SimulationError};
use crate::utils::tx_builder::SandwichMaker;
use crate::runner::state::*;
use crate::runner::bundle_sender;
use crate::runner::bundle_sender::BundleSender;
use super::{cook_simple_forward, cook_simple_reverse};
use log;
use colored::Colorize;
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn create_sandwich_by_cooks(
    ingredients: &RawIngredients,
    sandwich_balance: U256,
    next_block: &BlockInfo,
    fork_factory: &mut ForkFactory,
    sandwich_maker: &SandwichMaker,
    victim_hash: H256,
    bundle_sender: Arc<RwLock<BundleSender>>,
    sandwich_state: Arc<BotState>,
) -> Result<String, String> {
    let ingredients_simple_forward = ingredients.clone();
    let next_block_simple_forward = next_block.clone();
    let mut fork_factory_simple_forward = fork_factory.clone();
    let sandwich_maker_simple_forward = sandwich_maker.clone();
    let bundle_sender_simple_forward = bundle_sender.clone();
    let sandwich_state_simple_forward = sandwich_state.clone();
    tokio::spawn(async move {
        make_simple_forward(
            &ingredients_simple_forward,
            sandwich_balance.clone(),
            &next_block_simple_forward,
            &mut fork_factory_simple_forward,
            &sandwich_maker_simple_forward,
            victim_hash,
            bundle_sender_simple_forward,
            sandwich_state_simple_forward
        ).await;
    });

    let ingredients_simple_reverse = ingredients.clone();
    let next_block_simple_reverse = next_block.clone();
    let mut fork_factory_simple_reverse = fork_factory.clone();
    let sandwich_maker_simple_reverse = sandwich_maker.clone();
    let bundle_sender_simple_reverse = bundle_sender.clone();
    let sandwich_state_simple_reverse = sandwich_state.clone();
    tokio::spawn(async move {
        make_simple_reverse(
            &ingredients_simple_reverse,
            sandwich_balance.clone(),
            &next_block_simple_reverse,
            &mut fork_factory_simple_reverse,
            &sandwich_maker_simple_reverse,
            victim_hash,
            bundle_sender_simple_reverse,
            sandwich_state_simple_reverse,
        ).await;
    });

    return Ok("".to_string());
}

async fn make_simple_forward(
    ingredients: &RawIngredients,
    sandwich_balance: U256,
    next_block: &BlockInfo,
    fork_factory: &mut ForkFactory,
    sandwich_maker: &SandwichMaker,
    victim_hash: H256,
    bundle_sender: Arc<RwLock<BundleSender>>,
    sandwich_state: Arc<BotState>,
) {

    let optimal_sandwich =
        cook_simple_forward::create_optimal_sandwich(
            ingredients,
            sandwich_balance,
            next_block,
            fork_factory,
            sandwich_maker
        ).await;

    do_send_bundle(
        optimal_sandwich,
        victim_hash,
        sandwich_state,
        sandwich_maker,
        bundle_sender,
        next_block,
    ).await;
}

async fn make_simple_reverse(
    ingredients: &RawIngredients,
    sandwich_balance: U256,
    next_block: &BlockInfo,
    fork_factory: &mut ForkFactory,
    sandwich_maker: &SandwichMaker,
    victim_hash: H256,
    bundle_sender: Arc<RwLock<BundleSender>>,
    sandwich_state: Arc<BotState>,
) {

    let optimal_sandwich =
        cook_simple_reverse::create_optimal_sandwich(
            ingredients,
            sandwich_balance,
            next_block,
            fork_factory,
            sandwich_maker
        ).await;
    
    do_send_bundle(
        optimal_sandwich,
        victim_hash,
        sandwich_state,
        sandwich_maker,
        bundle_sender,
        next_block,
    ).await;
}

async fn do_send_bundle(
    optimal_sandwich: Result<OptimalRecipe, SimulationError>,
    victim_hash: H256,
    sandwich_state: Arc<BotState>,
    sandwich_maker: &SandwichMaker,
    bundle_sender: Arc<RwLock<BundleSender>>,
    next_block: &BlockInfo,
) {
    let mut optimal_sandwich = match optimal_sandwich {
        Ok(optimal) => optimal,
        Err(e) => {
            log::info!(
                "{}",
                format!("{:?} sim failed due to {:?}", &victim_hash, e).yellow()
            );
            return;
        }
    };

    // check if has dust
    let other_token = if optimal_sandwich.target_pool.token_0
        != utils::constants::get_weth_address()
    {
        optimal_sandwich.target_pool.token_0
    } else {
        optimal_sandwich.target_pool.token_1
    };

    if sandwich_state.has_dust(&other_token).await {
        optimal_sandwich.has_dust = true;
    }

    // spawn thread to send tx to builders
    let optimal_sandwich = optimal_sandwich.clone();
    let optimal_sandwich_two = optimal_sandwich.clone();
    let sandwich_maker = Arc::new(sandwich_maker.clone());
    let target_block = BlockInfo::new(next_block.number, next_block.timestamp, next_block.base_fee);

    if optimal_sandwich.revenue > U256::zero() {
        tokio::spawn(async move {
            match bundle_sender::send_bundle(
                &optimal_sandwich,
                target_block,
                sandwich_maker,
                sandwich_state.clone(),
            )
            .await
            {
                Ok(_) => { /* all reporting already done inside of send_bundle */ }
                Err(e) => {
                    log::info!(
                        "{}",
                        format!(
                            "{:?} failed to send bundle, due to {:?}",
                            optimal_sandwich.print_meats(),
                            e
                        )
                        .bright_magenta()
                    );
                }
            };
        });
    }

    // spawn thread to add tx for mega sandwich calculation
    let bundle_sender = bundle_sender.clone();
    tokio::spawn(async move {
        bundle_sender
            .write()
            .await
            .add_recipe(optimal_sandwich_two)
            .await;
    });
}