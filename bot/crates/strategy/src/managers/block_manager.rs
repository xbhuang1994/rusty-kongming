use anyhow::{anyhow, Result};
use ethers::{providers::Middleware, types::BlockNumber};
use log::info;
use std::sync::Arc;
use std::sync::Mutex;

use colored::Colorize;

use crate::{startup_info_log, types::BlockInfo};

pub struct BlockManager {
    latest_block: Mutex<Vec<BlockInfo>>,
    next_block: Mutex<Vec<BlockInfo>>,
}

impl BlockManager {
    pub fn new() -> Self {
        Self {
            latest_block: Mutex::new(vec![]),
            next_block: Mutex::new(vec![]),
        }
    }

    pub async fn setup<M: Middleware + 'static>(&self, provider: Arc<M>) -> Result<()> {
        let latest_block = provider
            .get_block(BlockNumber::Latest)
            .await
            .map_err(|_| anyhow!("Failed to get current block"))?
            .ok_or(anyhow!("Failed to get current block"))?;

        let latest_block: BlockInfo = latest_block.try_into()?;
        self.update_block_info(latest_block);

        startup_info_log!("latest block synced: {}", latest_block.number);
        Ok(())
    }

    /// Return info for the next block
    pub fn get_next_block(&self) -> BlockInfo {
        let locked = self.next_block.lock().unwrap();
        if locked.is_empty() {
            BlockInfo::default()
        } else {
            locked[0]
        }
    }

    /// Return info for the next block
    pub fn get_latest_block(&self) -> BlockInfo {
        let locked = self.latest_block.lock().unwrap();
        if locked.is_empty() {
            BlockInfo::default()
        } else {
            locked[0]
        }
    }

    /// Updates internal state with the latest mined block and next block
    pub fn update_block_info<T: Into<BlockInfo>>(&self, latest_block: T) {
        let latest_block: BlockInfo = latest_block.into();

        self.latest_block.lock().unwrap()[0] = latest_block;
        self.next_block.lock().unwrap()[0] = latest_block.get_next_block();
    }
}
