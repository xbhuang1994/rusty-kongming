use std::{sync::{Mutex,RwLock}, collections::HashMap};
use ethers::types::Transaction;
use crate::types::SandoRecipe;
use log::info;
use cfmms::pool::Pool;

pub struct SandoRecipeManager {

    pendding_recipes: Mutex<HashMap<Pool, RwLock<Vec<SandoRecipe>>>>,
}

impl SandoRecipeManager {
    
    pub fn new() -> Self {
        Self {
            pendding_recipes: Default::default(),
        }
    }

    pub fn push_pendding_recipe(&self, recipe: SandoRecipe) {
        let pool = recipe.get_target_pool().unwrap();
        let uuid = recipe.get_uuid();
        let mut map = self.pendding_recipes.lock().unwrap();
        let mut len = 1;
        if let Some(recipe_vec) = map.get(&pool) {
            let mut writer = recipe_vec.write().unwrap();
            writer.push(recipe);
            len = writer.len();
        } else {
            let new_vec = RwLock::new(vec![recipe]);
            map.insert(pool, new_vec);
        }
        info!("pendding recipes after push {:?} pool length is {:?}", uuid, len);
    }

    /// remove recipes has same hash with 'tx',
    /// and remove recipes has same 'from' and smaller nonce with 'tx'.
    fn remove_pendding_recipe(&self, tx: &Transaction) {

        let mut map = self.pendding_recipes.lock().unwrap();
        
        for (_pool, recipes) in map.iter_mut() {
            let mut pendding = recipes.write().unwrap();
            let _len_before = pendding.len();
            pendding.retain_mut(|recipe| { 
                let meats = recipe.get_meats();
                for meat in meats {
                    if meat.hash == tx.hash {
                        return false;
                    } else if meat.from == tx.from && meat.nonce <= tx.nonce {
                        return false;
                    }
                }

                // remove head_tx of committed before
                let mut head_txs = recipe.get_head_txs().clone();
                head_txs.retain(|head| 
                    !(head.hash == tx.hash || head.from == tx.from && head.nonce <= tx.nonce)
                );
                recipe.set_head_txs(head_txs);

                return true;
            });
            let _len_after = pendding.len();
            // info!("pendding recipes remove with tx {:?} from {:?} nonce {:?}, before len {:?} after len {:?}", tx.hash, tx.from, tx.nonce, _len_before, _len_after);
        }
    }

    pub fn update_pendding_recipe(&self, block_txs: &Vec<Transaction>) {

        for tx in block_txs {
            self.remove_pendding_recipe(tx);
        }
    }

    /// get all repices group by pool
    pub fn get_all_pendding_recipes(&self, clear_map: bool) -> HashMap<Pool, Vec<SandoRecipe>> {

        let mut map = self.pendding_recipes.lock().unwrap();
        let mut result: HashMap<Pool, Vec<SandoRecipe>> = HashMap::new();
        for (k, v) in map.iter() {
            let reader = v.read().unwrap();
            let vec = (*reader).clone();
            result.insert(k.clone(), vec);
        }
        if clear_map {
            map.clear();
        }
        return result;
    }

    // /// get and remove recipes with UniswapV2
    // pub fn get_pendding_recipes_pool_usv2(&self) -> Vec<SandoRecipe> {

    //     let mut pendding = self.pendding_recipes.lock().unwrap();
    //     let found_recipes: Vec<SandoRecipe> = pendding.iter().filter(
    //         |r| match r.get_target_pool() {
    //             UniswapV2(_) => true,
    //             _ => false
    //         }
    //     ).cloned().collect();
    //     if found_recipes.len() > 0 {
    //         let uuids: Vec<String> = found_recipes.iter().map(|s|s.get_uuid()).collect();
    //         pendding.retain(|s| !uuids.contains(&s.get_uuid()));
    //         // info!("pendding recipes after get pool_usv2 length is {:?}", pendding.len());
    //     }
    //     found_recipes
    // }

    // /// get and remove recipes with UniswapV3
    // pub fn get_pendding_recipes_pool_usv3(&self) -> Vec<SandoRecipe> {

    //     let mut pendding = self.pendding_recipes.lock().unwrap();
    //     let found_recipes: Vec<SandoRecipe> = pendding.iter().filter(
    //         |r| match r.get_target_pool() {
    //             UniswapV3(_) => true,
    //             _ => false
    //         }
    //     ).cloned().collect();
    //     if found_recipes.len() > 0 {
    //         let uuids: Vec<String> = found_recipes.iter().map(|s|s.get_uuid()).collect();
    //         pendding.retain(|s| !uuids.contains(&s.get_uuid()));
    //         // info!("pendding recipes after get pool_usv3 length is {:?}", pendding.len());
    //     }
    //     found_recipes
    // }

}