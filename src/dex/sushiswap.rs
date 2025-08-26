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
use crate::types::{Pool, Price, Token, TokenPair};
use crate::utils::{adjust_for_decimals, str_to_bigdecimal};

pub struct SushiSwapProvider {
    config: DexConfig,
    client: Client,
    web3_provider: Arc<Provider<Http>>,
}





impl SushiSwapProvider {
    pub fn new(config: DexConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        
        // 创建 Web3 提供者
        let web3_provider = match Provider::<Http>::try_from(&config.api_url) {
            Ok(provider) => Arc::new(provider),
            Err(e) => {
                log::error!("Failed to create Web3 provider for SushiSwap: {}", e);
                panic!("Failed to create Web3 provider: {}", e);
            }
        };
            
        Self { config, client, web3_provider }
    }
    
    async fn get_price_from_blockchain(&self, token_pair: &TokenPair) -> Result<Option<Price>> {
        // 获取工厂合约地址
        let factory_address = self.config.factory_address.as_ref()
            .ok_or_else(|| anyhow!("Factory address not configured"))?;
        
        let factory_addr = Address::from_str(factory_address)
            .map_err(|e| anyhow!("Invalid factory address: {}", e))?;
        
        // SushiSwap V2 Factory ABI (与Uniswap V2兼容)
        let factory_abi = r#"[
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
        ]"#;
        
        let factory_abi: Abi = serde_json::from_str(factory_abi)
            .map_err(|e| anyhow!("Invalid factory ABI: {}", e))?;
        
        let factory_contract = Contract::new(
            factory_addr,
            factory_abi,
            self.web3_provider.clone()
        );
        
        // 获取代币地址
        let token_a = Address::from_str(&token_pair.token_a.address)
            .map_err(|e| anyhow!("Invalid token A address: {}", e))?;
        let token_b = Address::from_str(&token_pair.token_b.address)
            .map_err(|e| anyhow!("Invalid token B address: {}", e))?;
        
        // 调用 getPair 方法获取配对地址
        let pair_address: Address = factory_contract
            .method::<_, Address>("getPair", (token_a, token_b))?
            .call()
            .await
            .map_err(|e| anyhow!("Failed to get pair address: {}", e))?;
        
        // 检查配对是否存在
        if pair_address == Address::zero() {
            return Ok(None);
        }
        
        // SushiSwap V2 Pair ABI (与Uniswap V2兼容)
        let pair_abi = r#"[
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
            },
            {
                "constant": true,
                "inputs": [],
                "name": "token0",
                "outputs": [{"name": "", "type": "address"}],
                "type": "function"
            }
        ]"#;
        
        let pair_abi: Abi = serde_json::from_str(pair_abi)
            .map_err(|e| anyhow!("Invalid pair ABI: {}", e))?;
        
        let pair_contract = Contract::new(
            pair_address,
            pair_abi,
            self.web3_provider.clone()
        );
        
        // 获取 token0 地址以确定储备量顺序
        let token0: Address = pair_contract
            .method::<_, Address>("token0", ())?
            .call()
            .await
            .map_err(|e| anyhow!("Failed to get token0: {}", e))?;
        
        // 获取储备量
        let (reserve0, reserve1, _): (U256, U256, u32) = pair_contract
            .method::<_, (U256, U256, u32)>("getReserves", ())?
            .call()
            .await
            .map_err(|e| anyhow!("Failed to get reserves: {}", e))?;
        
        // 确定哪个储备量对应哪个代币
        let (reserve_a, reserve_b) = if token0 == token_a {
            (reserve0, reserve1)
        } else {
            (reserve1, reserve0)
        };
        
        // 计算价格 (token_b / token_a)
        if reserve_a.is_zero() {
            return Ok(None);
        }
        
        let price_raw = reserve_b.as_u128() as f64 / reserve_a.as_u128() as f64;
        
        // 调整小数位数
        let decimals_diff = token_pair.token_a.decimals as i32 - token_pair.token_b.decimals as i32;
        let adjusted_price = if decimals_diff != 0 {
            price_raw * 10f64.powi(decimals_diff)
        } else {
            price_raw
        };
        
        let price = str_to_bigdecimal(&adjusted_price.to_string())?;
        
        Ok(Some(Price {
            token_pair: token_pair.clone(),
            price,
            liquidity: BigDecimal::from(0),
            dex: self.name().to_string(),
            timestamp: Utc::now(),
            block_number: None,
        }))
    }
    


}

#[async_trait]
impl DexProvider for SushiSwapProvider {
    fn name(&self) -> &str {
        &self.config.name
    }
    
    fn chain_id(&self) -> u64 {
        self.config.chain_id
    }
    
    async fn get_pools(&self) -> Result<Vec<Pool>> {
        // SushiSwap pools are now fetched on-demand via get_price_from_blockchain
        // This method returns an empty list as pools are discovered dynamically
        log::info!("SushiSwap: Pool discovery is now done on-demand via blockchain queries");
        Ok(Vec::new())
    }
    
    async fn get_price(&self, token_pair: &TokenPair) -> Result<Option<Price>> {
        // 直接调用区块链价格获取方法
        self.get_price_from_blockchain(token_pair).await
    }
    
    async fn get_prices(&self, token_pairs: &[TokenPair]) -> Result<HashMap<TokenPair, Price>> {
        let mut prices: HashMap<TokenPair, Price> = HashMap::new();
        
        log::info!("SushiSwap: 开始从区块链获取 {} 个代币对的价格", token_pairs.len());
        
        // 从区块链直接获取价格
        for token_pair in token_pairs {
            log::info!("SushiSwap: 正在获取代币对 {}/{} 的价格", token_pair.token_a.symbol, token_pair.token_b.symbol);
            match self.get_price_from_blockchain(token_pair).await {
                Ok(Some(price)) => {
                    log::info!("SushiSwap: 成功获取价格 {} for {}/{}", price.price, token_pair.token_a.symbol, token_pair.token_b.symbol);
                    prices.insert(token_pair.clone(), price);
                }
                Ok(None) => {
                    log::warn!("SushiSwap: 代币对 {}/{} 不存在流动性池", token_pair.token_a.symbol, token_pair.token_b.symbol);
                }
                Err(e) => {
                    log::error!("SushiSwap: 获取代币对 {}/{} 价格失败: {}", token_pair.token_a.symbol, token_pair.token_b.symbol, e);
                }
            }
            
            // 添加速率限制
            tokio::time::sleep(Duration::from_millis(self.config.rate_limit_ms)).await;
        }
        
        log::info!("SushiSwap: 成功获取 {} 个代币对的价格", prices.len());
        Ok(prices)
    }
    
    async fn get_pool_info(&self, pool_id: &str) -> Result<Option<Pool>> {
        // Pool info is now retrieved on-demand via blockchain queries
        // This method returns None as pool discovery is done dynamically
        log::info!("SushiSwap: Pool info for {} is now retrieved on-demand via blockchain queries", pool_id);
        Ok(None)
    }
    
    async fn health_check(&self) -> Result<bool> {
        // 检查区块链连接是否正常
        match self.web3_provider.get_block_number().await {
            Ok(_) => {
                log::info!("SushiSwap: 区块链连接健康检查通过");
                Ok(true)
            }
            Err(e) => {
                log::error!("SushiSwap: 区块链连接健康检查失败: {}", e);
                Ok(false)
            }
        }
    }
    
    fn get_fee_percentage(&self) -> f64 {
        0.003 // SushiSwap 的标准费率是 0.3%
    }
}