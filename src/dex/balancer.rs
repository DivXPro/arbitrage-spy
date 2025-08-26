use anyhow::{anyhow, Result};
use async_trait::async_trait;
use bigdecimal::BigDecimal;
use chrono::Utc;
use ethers::{
    providers::{Http, Provider, Middleware},
};
use num_traits::Zero;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use crate::config::DexConfig;
use crate::dex::DexProvider;
use crate::types::{Pool, Price, Token, TokenPair};
use crate::utils::{adjust_for_decimals, str_to_bigdecimal};

pub struct BalancerProvider {
    config: DexConfig,
    client: Client,
    web3_provider: Arc<Provider<Http>>,
}



impl BalancerProvider {
    pub fn new(config: DexConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        
        // 创建Web3提供者
        let provider = Provider::<Http>::try_from(&config.api_url)
            .expect("Failed to create Web3 provider");
        let web3_provider = Arc::new(provider);
            
        Self { config, client, web3_provider }
    }
    
    async fn get_price_from_blockchain(&self, token_pair: &TokenPair) -> Result<Option<Price>> {
        // For Balancer, we'll use a simplified approach to get price from the most liquid pool
        // In a real implementation, you would interact with Balancer Vault contract
        // For now, we'll return None to indicate price not available from blockchain
        log::warn!("Balancer blockchain price fetching not yet implemented for pair: {} - {}", 
                  token_pair.token_a.symbol, token_pair.token_b.symbol);
        Ok(None)
    }
    

    

}

#[async_trait]
impl DexProvider for BalancerProvider {
    fn name(&self) -> &str {
        &self.config.name
    }
    
    fn chain_id(&self) -> u64 {
        self.config.chain_id
    }
    
    async fn get_pools(&self) -> Result<Vec<Pool>> {
        // Balancer pools are now fetched on-demand via get_price_from_blockchain
        Ok(Vec::new())
    }
    
    async fn get_price(&self, token_pair: &TokenPair) -> Result<Option<Price>> {
        self.get_price_from_blockchain(token_pair).await
    }
    
    async fn get_prices(&self, token_pairs: &[TokenPair]) -> Result<HashMap<TokenPair, Price>> {
        let mut prices = HashMap::new();
        
        for token_pair in token_pairs {
            if let Ok(Some(price)) = self.get_price(token_pair).await {
                prices.insert(token_pair.clone(), price);
            }
            
            // 添加速率限制
            tokio::time::sleep(Duration::from_millis(self.config.rate_limit_ms)).await;
        }
        
        Ok(prices)
    }
    
    async fn get_pool_info(&self, pool_id: &str) -> Result<Option<Pool>> {
        // Pool info is now fetched on-demand via blockchain
        Ok(None)
    }
    
    async fn health_check(&self) -> Result<bool> {
        match self.web3_provider.get_block_number().await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
    
    fn get_fee_percentage(&self) -> f64 {
        0.003 // Balancer 的默认费率，实际费率可能因池而异
    }
}