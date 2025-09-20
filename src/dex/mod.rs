pub mod uniswap;
pub mod sushiswap;
pub mod pancakeswap;
pub mod curve;
pub mod balancer;

use anyhow::Result;
use async_trait::async_trait;
use crate::core::types::{Pool, Price, TokenPair};
use std::collections::HashMap;

#[async_trait]
pub trait DexProvider {
    /// 获取 DEX 名称
    fn name(&self) -> &str;
    
    /// 获取支持的链 ID
    fn chain_id(&self) -> u64;
    
    /// 获取所有流动性池
    async fn get_pools(&self) -> Result<Vec<Pool>>;
    
    /// 获取指定代币对的价格
    async fn get_price(&self, token_pair: &TokenPair) -> Result<Option<Price>>;
    
    /// 获取多个代币对的价格
    async fn get_prices(&self, token_pairs: &[TokenPair]) -> Result<HashMap<TokenPair, Price>>;
    
    /// 获取指定池的详细信息
    async fn get_pool_info(&self, pool_id: &str) -> Result<Option<Pool>>;
    
    /// 检查 DEX 是否健康（API 可用）
    async fn health_check(&self) -> Result<bool>;
    
    /// 获取费率信息
    fn get_fee_percentage(&self) -> f64;
}

pub struct DexManager {
    providers: HashMap<String, Box<dyn DexProvider + Send + Sync>>,
}

impl DexManager {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }
    
    pub fn add_provider(&mut self, provider: Box<dyn DexProvider + Send + Sync>) {
        let name = provider.name().to_string();
        self.providers.insert(name, provider);
    }
    
    pub fn get_provider(&self, name: &str) -> Option<&Box<dyn DexProvider + Send + Sync>> {
        self.providers.get(name)
    }
    
    pub fn get_all_providers(&self) -> &HashMap<String, Box<dyn DexProvider + Send + Sync>> {
        &self.providers
    }
    
    pub async fn get_all_pools(&self) -> Result<HashMap<String, Vec<Pool>>> {
        let mut all_pools = HashMap::new();
        
        for (name, provider) in &self.providers {
            match provider.get_pools().await {
                Ok(pools) => {
                    all_pools.insert(name.clone(), pools);
                }
                Err(e) => {
                    log::warn!("Failed to get pools from {}: {}", name, e);
                }
            }
        }
        
        Ok(all_pools)
    }
    
    pub async fn get_prices_from_all_dexes(&self, token_pairs: &[TokenPair]) -> Result<HashMap<String, HashMap<TokenPair, Price>>> {
        let mut all_prices = HashMap::new();
        
        for (name, provider) in &self.providers {
            match provider.get_prices(token_pairs).await {
                Ok(prices) => {
                    all_prices.insert(name.clone(), prices);
                }
                Err(e) => {
                    log::warn!("Failed to get prices from {}: {}", name, e);
                }
            }
        }
        
        Ok(all_prices)
    }
}