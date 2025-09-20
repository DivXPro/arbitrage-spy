use anyhow::{anyhow, Result};
use async_trait::async_trait;
use bigdecimal::BigDecimal;
use chrono::Utc;
use ethers::{
    abi::Abi,
    contract::Contract,
    providers::{Http, Provider, Middleware},
    types::{Address, U256},
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use crate::config::DexConfig;
use crate::dex::DexProvider;
use crate::core::types::{Pool, Price, Token, TokenPair};
use crate::utils::{adjust_for_decimals, str_to_bigdecimal};

pub struct PancakeSwapProvider {
    config: DexConfig,
    client: Client,
    web3_provider: Arc<Provider<Http>>,
}



impl PancakeSwapProvider {
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
        // PancakeSwap V2 Factory 合约地址 (BSC)
        let factory_address: Address = "0xcA143Ce32Fe78f1f7019d7d551a6402fC5350c73".parse()?;
        
        // PancakeSwap V2 Factory ABI (简化版，只包含getPair函数)
        let factory_abi: Abi = serde_json::from_str(r#"[
            {
                "constant": true,
                "inputs": [
                    {"name": "tokenA", "type": "address"},
                    {"name": "tokenB", "type": "address"}
                ],
                "name": "getPair",
                "outputs": [{"name": "pair", "type": "address"}],
                "type": "function"
            }
        ]"#)?;
        
        // Pair合约ABI (简化版，只包含getReserves函数)
        let pair_abi: Abi = serde_json::from_str(r#"[
            {
                "constant": true,
                "inputs": [],
                "name": "getReserves",
                "outputs": [
                    {"name": "reserve0", "type": "uint112"},
                    {"name": "reserve1", "type": "uint112"},
                    {"name": "blockTimestampLast", "type": "uint32"}
                ],
                "type": "function"
            }
        ]"#)?;
        
        let token0_address: Address = token_pair.token_a.address.parse()?;
        let token1_address: Address = token_pair.token_b.address.parse()?;
        
        // 创建Factory合约实例
        let factory_contract = Contract::new(factory_address, factory_abi, self.web3_provider.clone());
        
        // 调用getPair获取配对合约地址
        let pair_address: Address = factory_contract
            .method::<_, Address>("getPair", (token0_address, token1_address))?
            .call()
            .await?;
        
        // 检查配对是否存在
        if pair_address == Address::zero() {
            log::warn!("PancakeSwap: 代币对 {}/{} 不存在流动性池", token_pair.token_a.symbol, token_pair.token_b.symbol);
            return Ok(None);
        }
        
        // 创建Pair合约实例
        let pair_contract = Contract::new(pair_address, pair_abi, self.web3_provider.clone());
        
        // 调用getReserves获取储备量
        let (reserve0, reserve1, _): (U256, U256, u32) = pair_contract
            .method::<_, (U256, U256, u32)>("getReserves", ())?
            .call()
            .await?;
        
        if reserve0.is_zero() || reserve1.is_zero() {
            log::warn!("PancakeSwap: 代币对 {}/{} 储备量为零", token_pair.token_a.symbol, token_pair.token_b.symbol);
            return Ok(None);
        }
        
        // 转换为BigDecimal并调整小数位
        let reserve0_decimal = BigDecimal::from_str(&reserve0.to_string())?;
        let reserve1_decimal = BigDecimal::from_str(&reserve1.to_string())?;
        
        let adjusted_reserve0 = adjust_for_decimals(&reserve0_decimal, token_pair.token_a.decimals);
        let adjusted_reserve1 = adjust_for_decimals(&reserve1_decimal, token_pair.token_b.decimals);
        
        // 计算价格 (token1/token0)
        let price_value = &adjusted_reserve1 / &adjusted_reserve0;
        
        let price = Price {
            token_pair: token_pair.clone(),
            price: price_value,
            liquidity: &adjusted_reserve0 + &adjusted_reserve1,
            dex: "PancakeSwap".to_string(),
            timestamp: Utc::now(),
            block_number: None,
        };
        
        log::debug!("PancakeSwap: 从区块链获取价格 {}/{} = {}", 
                   token_pair.token_a.symbol, token_pair.token_b.symbol, price.price);
        
        Ok(Some(price))
    }
    

    

}

#[async_trait]
impl DexProvider for PancakeSwapProvider {
    fn name(&self) -> &str {
        &self.config.name
    }
    
    fn chain_id(&self) -> u64 {
        self.config.chain_id
    }
    
    async fn get_pools(&self) -> Result<Vec<Pool>> {
        // PancakeSwap pools are now fetched on-demand via get_price_from_blockchain
        // This method returns an empty list as pools are discovered dynamically
        log::info!("PancakeSwap: Pool discovery is now done on-demand via blockchain queries");
        Ok(Vec::new())
    }
    
    async fn get_price(&self, token_pair: &TokenPair) -> Result<Option<Price>> {
        self.get_price_from_blockchain(token_pair).await
    }
    
    async fn get_prices(&self, token_pairs: &[TokenPair]) -> Result<HashMap<TokenPair, Price>> {
        let mut prices = HashMap::new();
        
        for (i, token_pair) in token_pairs.iter().enumerate() {
            log::debug!("PancakeSwap: 获取价格 ({}/{}) {}/{}", 
                       i + 1, token_pairs.len(), 
                       token_pair.token_a.symbol, token_pair.token_b.symbol);
            
            match self.get_price_from_blockchain(token_pair).await {
                Ok(Some(price)) => {
                    log::info!("PancakeSwap: 成功获取价格 {}/{} = {}", 
                              token_pair.token_a.symbol, token_pair.token_b.symbol, price.price);
                    prices.insert(token_pair.clone(), price);
                }
                Ok(None) => {
                    log::warn!("PancakeSwap: 代币对 {}/{} 无价格数据", 
                              token_pair.token_a.symbol, token_pair.token_b.symbol);
                }
                Err(e) => {
                    log::error!("PancakeSwap: 获取价格失败 {}/{}: {}", 
                               token_pair.token_a.symbol, token_pair.token_b.symbol, e);
                }
            }
            
            // 添加延迟以避免过于频繁的请求
            if i < token_pairs.len() - 1 {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        
        log::info!("PancakeSwap: 完成价格获取，成功获取 {}/{} 个代币对的价格", 
                  prices.len(), token_pairs.len());
        
        Ok(prices)
    }
    
    async fn get_pool_info(&self, pool_id: &str) -> Result<Option<Pool>> {
        // Pool info is now retrieved on-demand via blockchain queries
        // This method returns None as pool discovery is done dynamically
        log::info!("PancakeSwap: Pool info for {} is now retrieved on-demand via blockchain queries", pool_id);
        Ok(None)
    }
    
    async fn health_check(&self) -> Result<bool> {
        // 检查区块链连接是否正常
        match self.web3_provider.get_block_number().await {
            Ok(_) => {
                log::info!("PancakeSwap: 区块链连接健康检查通过");
                Ok(true)
            }
            Err(e) => {
                log::error!("PancakeSwap: 区块链连接健康检查失败: {}", e);
                Ok(false)
            }
        }
    }
    
    fn get_fee_percentage(&self) -> f64 {
        0.0025 // PancakeSwap 的标准费率是 0.25%
    }
}