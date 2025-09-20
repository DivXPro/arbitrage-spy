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
use crate::core::types::{Pool, Price, Token, TokenPair};
use crate::utils::{adjust_for_decimals, str_to_bigdecimal};

pub struct CurveProvider {
    config: DexConfig,
    client: Client,
    web3_provider: Arc<Provider<Http>>,
}

#[derive(Debug, Deserialize)]
struct CurvePoolsResponse {
    success: bool,
    data: CurvePoolsData,
}

#[derive(Debug, Deserialize)]
struct CurvePoolsData {
    #[serde(rename = "poolData")]
    pool_data: Vec<CurvePool>,
}

#[derive(Debug, Deserialize)]
struct CurvePool {
    id: String,
    address: String,
    name: String,
    #[serde(rename = "assetTypeName")]
    asset_type_name: String,
    coins: Vec<CurveCoin>,
    #[serde(rename = "totalSupply")]
    total_supply: String,
    #[serde(rename = "usdTotal")]
    usd_total: f64,
    #[serde(rename = "volumeUSD")]
    volume_usd: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct CurveCoin {
    address: String,
    symbol: String,
    decimals: u8,
    #[serde(rename = "usdPrice")]
    usd_price: Option<f64>,
    #[serde(rename = "poolBalance")]
    pool_balance: String,
}



impl CurveProvider {
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
    
    async fn fetch_pools_from_api(&self) -> Result<Vec<CurvePool>> {
        let response = self.client
            .get(&self.config.api_url)
            .send()
            .await?
            .json::<CurvePoolsResponse>()
            .await?;
            
        if !response.success {
            return Err(anyhow!("Curve API returned unsuccessful response"));
        }
        
        Ok(response.data.pool_data)
    }
    

    
    fn convert_curve_pool_to_pools(&self, curve_pool: CurvePool) -> Result<Vec<Pool>> {
        let mut pools = Vec::new();
        
        // Curve 池通常包含多个代币，我们需要为每对代币创建一个池
        for i in 0..curve_pool.coins.len() {
            for j in (i + 1)..curve_pool.coins.len() {
                let coin_a = &curve_pool.coins[i];
                let coin_b = &curve_pool.coins[j];
                
                let token_a = Token::new(
                    coin_a.address.clone(),
                    coin_a.symbol.clone(),
                    coin_a.symbol.clone(), // Curve API 不提供完整名称
                    coin_a.decimals,
                    self.config.chain_id,
                );
                
                let token_b = Token::new(
                    coin_b.address.clone(),
                    coin_b.symbol.clone(),
                    coin_b.symbol.clone(),
                    coin_b.decimals,
                    self.config.chain_id,
                );
                
                let token_pair = TokenPair::new(token_a, token_b);
                
                let reserve_a = str_to_bigdecimal(&coin_a.pool_balance)?;
                let reserve_b = str_to_bigdecimal(&coin_b.pool_balance)?;
                let total_liquidity = BigDecimal::from_str(&curve_pool.usd_total.to_string())?;
                let volume_24h = BigDecimal::from_str(&curve_pool.volume_usd.unwrap_or(0.0).to_string())?;
                
                let token_a_decimals = token_pair.token_a.decimals;
                let token_b_decimals = token_pair.token_b.decimals;
                
                let pool = Pool {
                    id: format!("{}-{}-{}", curve_pool.id, i, j),
                    dex: self.name().to_string(),
                    token_pair,
                    reserve_a: adjust_for_decimals(&reserve_a, token_a_decimals),
                    reserve_b: adjust_for_decimals(&reserve_b, token_b_decimals),
                    fee_percentage: self.get_fee_percentage(),
                    total_liquidity: total_liquidity.clone(),
                    volume_24h: volume_24h.clone(),
                    last_updated: Utc::now(),
                };
                
                pools.push(pool);
            }
        }
        
        Ok(pools)
    }
}

#[async_trait]
impl DexProvider for CurveProvider {
    fn name(&self) -> &str {
        &self.config.name
    }
    
    fn chain_id(&self) -> u64 {
        self.config.chain_id
    }
    
    async fn get_pools(&self) -> Result<Vec<Pool>> {
        let curve_pools = self.fetch_pools_from_api().await?;
        
        let mut all_pools = Vec::new();
        for curve_pool in curve_pools {
            // 只处理有足够流动性的池
            if curve_pool.usd_total > 10000.0 {
                match self.convert_curve_pool_to_pools(curve_pool) {
                    Ok(mut pools) => all_pools.append(&mut pools),
                    Err(e) => log::warn!("Failed to convert Curve pool: {}", e),
                }
            }
        }
        
        Ok(all_pools)
    }
    
    async fn get_price(&self, token_pair: &TokenPair) -> Result<Option<Price>> {
        // Curve 的价格获取比较复杂，因为它使用不同的定价机制
        // 这里提供一个简化的实现
        let pools = self.get_pools().await?;
        
        for pool in pools {
            if pool.token_pair == *token_pair {
                // 使用储备比率计算价格
                if !pool.reserve_b.is_zero() {
                    let price = &pool.reserve_a / &pool.reserve_b;
                    
                    return Ok(Some(Price {
                        token_pair: token_pair.clone(),
                        price,
                        liquidity: pool.total_liquidity,
                        dex: self.name().to_string(),
                        timestamp: Utc::now(),
                        block_number: None,
                    }));
                }
            }
        }
        
        Ok(None)
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
        let pools = self.get_pools().await?;
        
        for pool in pools {
            if pool.id == pool_id {
                return Ok(Some(pool));
            }
        }
        
        Ok(None)
    }
    
    async fn health_check(&self) -> Result<bool> {
        // 检查区块链连接是否正常
        match self.web3_provider.get_block_number().await {
            Ok(_) => {
                log::info!("Curve: 区块链连接健康检查通过");
                Ok(true)
            }
            Err(e) => {
                log::error!("Curve: 区块链连接健康检查失败: {}", e);
                Ok(false)
            }
        }
    }
    
    fn get_fee_percentage(&self) -> f64 {
        0.0004 // Curve 的典型费率是 0.04%
    }
}