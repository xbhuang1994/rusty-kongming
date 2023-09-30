use std::sync::Mutex;
use ethers::types::Transaction;
use crate::types::SandoRecipe;
use log::info;
use cfmms::pool::Pool::{UniswapV2, UniswapV3};

pub struct SandoRecipeManager {

    pendding_recipes: Mutex<Vec<SandoRecipe>>,

}

impl SandoRecipeManager {
    
    pub fn new() -> Self {
        Self {
            pendding_recipes: Default::default(),
        }
    }

    pub fn push_pendding_recipe(&self, recipe: SandoRecipe) {
        let mut pendding = self.pendding_recipes.lock().unwrap();
        let uuid = recipe.get_uuid();
        pendding.push(recipe);
        info!("pendding recipes after push {:?} length is {:?}", uuid, pendding.len());
    }

    /// remove recipes has same hash with 'tx',
    /// and remove recipes has same 'from' and smaller nonce with 'tx'.
    fn remove_pendding_recipe(&self, tx: &Transaction) {

        let mut pendding = self.pendding_recipes.lock().unwrap();
        let len_before = pendding.len();
        if len_before > 0 {
            pendding.retain(|r| { 
                let meats = r.get_meats();
                for meat in meats {
                    if meat.hash == tx.hash {
                        return false;
                    } else if meat.from == tx.from && meat.nonce <= tx.nonce {
                        return false;
                    }
                }
                let heads = r.get_head_txs();
                for head in heads {
                    if head.hash == tx.hash {
                        return false;
                    } else if head.from == tx.from && head.nonce <= tx.nonce {
                        return false;
                    }
                }
                return true;
            });
        }
        let len_after = pendding.len();
        info!("pendding recipes remove with tx {:?} from {:?} nonce {:?}, before len {:?} after len {:?}",
            tx.hash, tx.from, tx.nonce, len_before, len_after);
    }

    pub fn update_pendding_recipe(&self, block_txs: &Vec<Transaction>) {

        for tx in block_txs {
            self.remove_pendding_recipe(tx);
        }
    }

    /// get and remove recipes with UniswapV2
    pub fn find_pendding_recipes_pool_usv2(&self) -> Vec<SandoRecipe> {

        let mut pendding = self.pendding_recipes.lock().unwrap();
        let found_recipes: Vec<SandoRecipe> = pendding.iter().filter(
            |r| match r.get_target_pool() {
                UniswapV2(_) => true,
                _ => false
            }
        ).cloned().collect();
        if found_recipes.len() > 0 {
            let uuids: Vec<String> = found_recipes.iter().map(|s|s.get_uuid()).collect();
            pendding.retain(|s| !uuids.contains(&s.get_uuid()));
            info!("pendding recipes after find pool_usv2 length is {:?}", pendding.len());
        }
        found_recipes
    }

    /// get and remove recipes with UniswapV3
    pub fn find_pendding_recipes_pool_usv3(&self) -> Vec<SandoRecipe> {

        let mut pendding = self.pendding_recipes.lock().unwrap();
        let found_recipes: Vec<SandoRecipe> = pendding.iter().filter(
            |r| match r.get_target_pool() {
                UniswapV3(_) => true,
                _ => false
            }
        ).cloned().collect();
        if found_recipes.len() > 0 {
            let uuids: Vec<String> = found_recipes.iter().map(|s|s.get_uuid()).collect();
            pendding.retain(|s| !uuids.contains(&s.get_uuid()));
            info!("pendding recipes after find pool_usv3 length is {:?}", pendding.len());
        }
        found_recipes
    }

}