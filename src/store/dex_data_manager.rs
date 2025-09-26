use anyhow::Result;
use ethers::prelude::*;
use std::sync::Arc;
use log::{info, warn};
use serde::{Deserialize, Serialize};

use super::blockchain_client::{BlockchainClient, NetworkConfig};
use super::uniswap_v2_client::{UniswapV2Client};
use super::uniswap_v3_client::{UniswapV3Client};

/// DEX类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DexType {
    UniswapV2,
    UniswapV3,
    SushiSwap,
    PancakeSwap,
}

impl DexType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DexType::UniswapV2 => "Uniswap V2",
            DexType::UniswapV3 => "Uniswap V3",
            DexType::SushiSwap => "SushiSwap",
            DexType::PancakeSwap => "PancakeSwap",
        }
    }
}

/// 交易对信息（统一格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairInfo {
    pub dex_type: DexType,
    pub pair_address: String,
    pub token0: String,
    pub token1: String,
    pub token0_symbol: Option<String>,
    pub token1_symbol: Option<String>,
    pub token0_decimals: Option<u8>,
    pub token1_decimals: Option<u8>,
    pub fee: Option<u32>, // V3专用
    pub reserves0: Option<String>, // V2专用
    pub reserves1: Option<String>, // V2专用
    pub sqrt_price_x96: Option<String>, // V3专用
    pub tick: Option<i32>, // V3专用
    pub liquidity: Option<String>,
    pub total_supply: Option<String>, // V2专用
}

/// DEX工厂信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexFactoryInfo {
    pub dex_type: DexType,
    pub factory_address: String,
    pub additional_info: serde_json::Value,
}

/// DEX数据管理器
pub struct DexDataManager {
    blockchain_client: Arc<BlockchainClient>,
    v2_client: Option<UniswapV2Client>,
    v3_client: Option<UniswapV3Client>,
    enabled_dex_types: Vec<DexType>,
}

impl DexDataManager {
    /// 创建新的DEX数据管理器
    pub async fn new(network_config: NetworkConfig, enabled_dex_types: Vec<DexType>) -> Result<Self> {
        info!("初始化DEX数据管理器，网络: {}", network_config.name);
        
        let blockchain_client = Arc::new(BlockchainClient::new(network_config).await?);
        
        // 根据启用的DEX类型初始化客户端
        let v2_client = if enabled_dex_types.contains(&DexType::UniswapV2) || 
                           enabled_dex_types.contains(&DexType::SushiSwap) {
            Some(UniswapV2Client::new(blockchain_client.clone())?)
        } else {
            None
        };

        let v3_client = if enabled_dex_types.contains(&DexType::UniswapV3) {
            Some(UniswapV3Client::new(blockchain_client.clone())?)
        } else {
            None
        };

        Ok(Self {
            blockchain_client,
            v2_client,
            v3_client,
            enabled_dex_types,
        })
    }

    /// 创建以太坊主网管理器（启用所有DEX）
    pub async fn ethereum_all_dex() -> Result<Self> {
        Self::new(
            NetworkConfig::ethereum_mainnet(),
            vec![DexType::UniswapV2, DexType::UniswapV3, DexType::SushiSwap]
        ).await
    }

    /// 获取所有启用的DEX工厂信息
    pub async fn get_all_factory_info(&self) -> Result<Vec<DexFactoryInfo>> {
        info!("获取所有启用DEX的工厂信息");
        let mut factory_infos = Vec::new();

        // V2工厂信息
        if let Some(ref v2_client) = self.v2_client {
            for dex_type in &self.enabled_dex_types {
                if matches!(dex_type, DexType::UniswapV2 | DexType::SushiSwap) {
                    match v2_client.get_factory_info().await {
                        Ok(info) => {
                            let factory_info = DexFactoryInfo {
                                dex_type: dex_type.clone(),
                                factory_address: info.factory_address.clone(),
                                additional_info: serde_json::to_value(&info)?,
                            };
                            factory_infos.push(factory_info);
                        }
                        Err(e) => {
                            warn!("获取 {} 工厂信息失败: {}", dex_type.as_str(), e);
                        }
                    }
                }
            }
        }

        // V3工厂信息
        if let Some(ref v3_client) = self.v3_client {
            if self.enabled_dex_types.contains(&DexType::UniswapV3) {
                match v3_client.get_factory_info().await {
                    Ok(info) => {
                        let factory_info = DexFactoryInfo {
                            dex_type: DexType::UniswapV3,
                            factory_address: info.factory_address.clone(),
                            additional_info: serde_json::to_value(&info)?,
                        };
                        factory_infos.push(factory_info);
                    }
                    Err(e) => {
                        warn!("获取 Uniswap V3 工厂信息失败: {}", e);
                    }
                }
            }
        }

        info!("成功获取 {} 个DEX工厂信息", factory_infos.len());
        Ok(factory_infos)
    }

    /// 获取V2交易对信息（批量）
    pub async fn get_v2_pairs_batch(&self, start_index: u64, count: u64) -> Result<Vec<PairInfo>> {
        if let Some(ref v2_client) = self.v2_client {
            info!("获取V2交易对信息: 从索引 {} 开始，获取 {} 个", start_index, count);
            
            let v2_pairs = v2_client.get_pairs_batch(start_index, count).await?;
            let pairs: Vec<PairInfo> = v2_pairs.into_iter().map(|pair| {
                PairInfo {
                    dex_type: DexType::UniswapV2,
                    pair_address: pair.pair_address,
                    token0: pair.token0,
                    token1: pair.token1,
                    token0_symbol: pair.token0_symbol,
                    token1_symbol: pair.token1_symbol,
                    token0_decimals: pair.token0_decimals,
                    token1_decimals: pair.token1_decimals,
                    fee: None,
                    reserves0: Some(pair.reserves0),
                    reserves1: Some(pair.reserves1),
                    sqrt_price_x96: None,
                    tick: None,
                    liquidity: None,
                    total_supply: Some(pair.total_supply),
                }
            }).collect();

            info!("成功获取 {} 个V2交易对", pairs.len());
            Ok(pairs)
        } else {
            Err(anyhow::anyhow!("V2客户端未初始化"))
        }
    }

    /// 查找V3池信息（指定代币对）
    pub async fn find_v3_pools(&self, token_pairs: &[(Address, Address)]) -> Result<Vec<PairInfo>> {
        if let Some(ref v3_client) = self.v3_client {
            info!("查找 {} 个代币对的V3池", token_pairs.len());
            
            let v3_pools = v3_client.find_pools_batch(token_pairs).await?;
            let pairs: Vec<PairInfo> = v3_pools.into_iter().map(|pool| {
                PairInfo {
                    dex_type: DexType::UniswapV3,
                    pair_address: pool.pool_address,
                    token0: pool.token0,
                    token1: pool.token1,
                    token0_symbol: pool.token0_symbol,
                    token1_symbol: pool.token1_symbol,
                    token0_decimals: pool.token0_decimals,
                    token1_decimals: pool.token1_decimals,
                    fee: Some(pool.fee),
                    reserves0: None,
                    reserves1: None,
                    sqrt_price_x96: Some(pool.sqrt_price_x96),
                    tick: Some(pool.tick),
                    liquidity: Some(pool.liquidity),
                    total_supply: None,
                }
            }).collect();

            info!("成功找到 {} 个V3池", pairs.len());
            Ok(pairs)
        } else {
            Err(anyhow::anyhow!("V3客户端未初始化"))
        }
    }

    /// 查找指定代币对在所有启用DEX中的交易对/池
    pub async fn find_all_pairs_for_tokens(&self, token_a: Address, token_b: Address) -> Result<Vec<PairInfo>> {
        info!("查找代币对 {:?} - {:?} 在所有DEX中的交易对", token_a, token_b);
        
        let mut all_pairs = Vec::new();

        // V2交易对
        if let Some(ref v2_client) = self.v2_client {
            match v2_client.get_pair_address(token_a, token_b).await {
                Ok(Some(pair_address)) => {
                    match v2_client.get_pair_info(pair_address).await {
                        Ok(pair_info) => {
                            let pair = PairInfo {
                                dex_type: DexType::UniswapV2,
                                pair_address: pair_info.pair_address,
                                token0: pair_info.token0,
                                token1: pair_info.token1,
                                token0_symbol: pair_info.token0_symbol,
                                token1_symbol: pair_info.token1_symbol,
                                token0_decimals: pair_info.token0_decimals,
                                token1_decimals: pair_info.token1_decimals,
                                fee: None,
                                reserves0: Some(pair_info.reserves0),
                                reserves1: Some(pair_info.reserves1),
                                sqrt_price_x96: None,
                                tick: None,
                                liquidity: None,
                                total_supply: Some(pair_info.total_supply),
                            };
                            all_pairs.push(pair);
                        }
                        Err(e) => {
                            warn!("获取V2 Pair信息失败: {}", e);
                        }
                    }
                }
                Ok(None) => {
                    info!("V2中未找到该代币对");
                }
                Err(e) => {
                    warn!("查找V2 Pair失败: {}", e);
                }
            }
        }

        // V3池
        if let Some(ref v3_client) = self.v3_client {
            match v3_client.find_pools(token_a, token_b).await {
                Ok(pools) => {
                    for pool in pools {
                        let pair = PairInfo {
                            dex_type: DexType::UniswapV3,
                            pair_address: pool.pool_address,
                            token0: pool.token0,
                            token1: pool.token1,
                            token0_symbol: pool.token0_symbol,
                            token1_symbol: pool.token1_symbol,
                            token0_decimals: pool.token0_decimals,
                            token1_decimals: pool.token1_decimals,
                            fee: Some(pool.fee),
                            reserves0: None,
                            reserves1: None,
                            sqrt_price_x96: Some(pool.sqrt_price_x96),
                            tick: Some(pool.tick),
                            liquidity: Some(pool.liquidity),
                            total_supply: None,
                        };
                        all_pairs.push(pair);
                    }
                }
                Err(e) => {
                    warn!("查找V3池失败: {}", e);
                }
            }
        }

        info!("总共找到 {} 个交易对/池", all_pairs.len());
        Ok(all_pairs)
    }

    /// 健康检查
    pub async fn health_check(&self) -> Result<bool> {
        self.blockchain_client.health_check().await
    }

    /// 获取网络信息
    pub fn get_network_info(&self) -> &NetworkConfig {
        self.blockchain_client.network_config()
    }

    /// 获取启用的DEX类型
    pub fn get_enabled_dex_types(&self) -> &[DexType] {
        &self.enabled_dex_types
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dex_manager_creation() {
        let manager = DexDataManager::ethereum_all_dex().await;
        assert!(manager.is_ok());
        
        let manager = manager.unwrap();
        assert!(manager.health_check().await.unwrap());
    }

    #[tokio::test]
    async fn test_get_factory_info() {
        let manager = DexDataManager::ethereum_all_dex().await.unwrap();
        let factory_infos = manager.get_all_factory_info().await;
        
        assert!(factory_infos.is_ok());
        let infos = factory_infos.unwrap();
        assert!(!infos.is_empty());
    }

    #[tokio::test]
    async fn test_find_eth_usdc_pairs() {
        let manager = DexDataManager::ethereum_all_dex().await.unwrap();
        
        // WETH和USDC地址
        let weth = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".parse().unwrap();
        let usdc = "0xA0b86a33E6441b8C4505B4afDcA7FBf0251f7046".parse().unwrap();
        
        let pairs = manager.find_all_pairs_for_tokens(weth, usdc).await;
        assert!(pairs.is_ok());
        
        let pairs = pairs.unwrap();
        assert!(!pairs.is_empty());
    }
}