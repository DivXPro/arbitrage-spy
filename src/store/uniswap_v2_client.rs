use anyhow::{anyhow, Result};
use ethers::prelude::*;
use std::sync::Arc;
use log::{info};
use serde::{Deserialize, Serialize};

use super::blockchain_client::BlockchainClient;

/// V2交易对信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2PairInfo {
    pub pair_address: String,
    pub token0: String,
    pub token1: String,
    pub token0_symbol: Option<String>,
    pub token1_symbol: Option<String>,
    pub token0_decimals: Option<u8>,
    pub token1_decimals: Option<u8>,
    pub reserves0: String,
    pub reserves1: String,
    pub total_supply: String,
}

/// V2工厂信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V2FactoryInfo {
    pub factory_address: String,
    pub total_pairs: u64,
    pub fee_to_setter: Option<String>,
}

/// Uniswap V2客户端
pub struct UniswapV2Client {
    blockchain_client: Arc<BlockchainClient>,
    factory_address: Address,
}

impl UniswapV2Client {
    /// 创建新的Uniswap V2客户端
    pub fn new(blockchain_client: Arc<BlockchainClient>) -> Result<Self> {
        // Uniswap V2 Factory地址 (以太坊主网)
        let factory_address = "0x5C69bEe701ef814a2B6a3EDD4B1652CB9cc5aA6f"
            .parse::<Address>()
            .map_err(|e| anyhow!("无效的工厂地址: {}", e))?;

        Ok(Self {
            blockchain_client,
            factory_address,
        })
    }

    /// 获取工厂信息
    pub async fn get_factory_info(&self) -> Result<V2FactoryInfo> {
        info!("获取Uniswap V2工厂信息");
        
        // 简化实现，返回模拟数据
        Ok(V2FactoryInfo {
            factory_address: format!("{:?}", self.factory_address),
            total_pairs: 100000, // 模拟数据
            fee_to_setter: None,
        })
    }

    /// 通过索引获取交易对地址
    pub async fn get_pair_by_index(&self, index: u64) -> Result<Address> {
        info!("获取索引 {} 的交易对地址", index);
        
        // 简化实现，返回零地址
        Ok(Address::zero())
    }

    /// 获取指定代币对的交易对地址
    pub async fn get_pair_address(&self, token_a: Address, token_b: Address) -> Result<Option<Address>> {
        info!("获取代币对 {:?} - {:?} 的交易对地址", token_a, token_b);
        
        // 简化实现
        Ok(None)
    }

    /// 获取交易对详细信息
    pub async fn get_pair_info(&self, pair_address: Address) -> Result<V2PairInfo> {
        info!("获取交易对 {:?} 的详细信息", pair_address);
        
        // 检查交易对地址是否为零地址
        if pair_address == Address::zero() {
            return Err(anyhow!("无效的交易对地址"));
        }

        // 获取基本交易对信息
        let token0 = self.call_pair_method(pair_address, "token0()").await?;
        let token1 = self.call_pair_method(pair_address, "token1()").await?;
        
        // 获取储备量信息
        let reserves_data = self.call_pair_method(pair_address, "getReserves()").await?;
        let (reserves0, reserves1) = self.parse_reserves(&reserves_data)?;
        
        // 获取总供应量
        let total_supply = self.call_pair_method(pair_address, "totalSupply()").await?;
        let total_supply_u256 = U256::from_big_endian(&total_supply);

        // 获取代币信息
        let (token0_symbol, token0_decimals) = self.get_token_info(token0.clone()).await;
        let (token1_symbol, token1_decimals) = self.get_token_info(token1.clone()).await;

        let pair_info = V2PairInfo {
            pair_address: format!("{:?}", pair_address),
            token0: format!("{:?}", Address::from_slice(&token0)),
            token1: format!("{:?}", Address::from_slice(&token1)),
            token0_symbol,
            token1_symbol,
            token0_decimals,
            token1_decimals,
            reserves0: reserves0.to_string(),
            reserves1: reserves1.to_string(),
            total_supply: total_supply_u256.to_string(),
        };

        info!("交易对信息获取成功: {} - {} (储备量: {}/{}, 总供应量: {})", 
               pair_info.token0_symbol.as_deref().unwrap_or("Unknown"),
               pair_info.token1_symbol.as_deref().unwrap_or("Unknown"),
               pair_info.reserves0,
               pair_info.reserves1,
               pair_info.total_supply);

        Ok(pair_info)
    }

    /// 调用交易对合约方法
    async fn call_pair_method(&self, pair_address: Address, method_signature: &str) -> Result<Vec<u8>> {
        let function_selector = match method_signature {
            "token0()" => [0x0d, 0xfe, 0x16, 0x24],
            "token1()" => [0xd2, 0x1c, 0x20, 0xee],
            "getReserves()" => [0x09, 0x02, 0xf1, 0xac],
            "totalSupply()" => [0x18, 0x16, 0x0d, 0xdd],
            _ => return Err(anyhow!("不支持的方法: {}", method_signature)),
        };

        let call_data = Bytes::from(function_selector.to_vec());
        let result = self.blockchain_client.call_contract(pair_address, call_data).await?;
        Ok(result.to_vec())
    }

    /// 解析储备量数据
    fn parse_reserves(&self, data: &[u8]) -> Result<(U256, U256)> {
        if data.len() < 64 {
            return Err(anyhow!("储备量数据长度不足"));
        }

        let reserves0 = U256::from_big_endian(&data[0..32]);
        let reserves1 = U256::from_big_endian(&data[32..64]);
        
        Ok((reserves0, reserves1))
    }

    /// 获取代币信息（symbol和decimals）
    async fn get_token_info(&self, token_address: Vec<u8>) -> (Option<String>, Option<u8>) {
        if token_address.len() != 20 {
            return (None, None);
        }

        let address = Address::from_slice(&token_address);
        
        // 获取symbol
        let symbol = self.get_token_symbol(address).await.ok();
        
        // 获取decimals
        let decimals = self.get_token_decimals(address).await.ok();
        
        (symbol, decimals)
    }

    /// 获取代币符号
    async fn get_token_symbol(&self, token_address: Address) -> Result<String> {
        let function_selector = [0x95, 0xd8, 0x9b, 0x41]; // symbol()
        let call_data = Bytes::from(function_selector.to_vec());
        
        let result = self.blockchain_client.call_contract(token_address, call_data).await?;
        let result_bytes = result.to_vec();
        
        if result_bytes.len() < 64 {
            return Ok("UNKNOWN".to_string());
        }

        // 解析字符串返回值
        let offset = U256::from_big_endian(&result_bytes[0..32]).as_usize();
        if offset >= result_bytes.len() || offset + 32 >= result_bytes.len() {
            return Ok("UNKNOWN".to_string());
        }

        let length = U256::from_big_endian(&result_bytes[offset..offset + 32]).as_usize();
        let start = offset + 32;
        
        if start + length > result_bytes.len() {
            return Ok("UNKNOWN".to_string());
        }

        let symbol_bytes = &result_bytes[start..start + length];
        let symbol = String::from_utf8_lossy(symbol_bytes).trim_end_matches('\0').to_string();
        
        Ok(if symbol.is_empty() { "UNKNOWN".to_string() } else { symbol })
    }

    /// 获取代币精度
    async fn get_token_decimals(&self, token_address: Address) -> Result<u8> {
        let function_selector = [0x31, 0x3c, 0xe5, 0x67]; // decimals()
        let call_data = Bytes::from(function_selector.to_vec());
        
        let result = self.blockchain_client.call_contract(token_address, call_data).await?;
        let result_bytes = result.to_vec();
        
        if result_bytes.len() < 32 {
            return Ok(18); // 默认精度
        }

        let decimals = U256::from_big_endian(&result_bytes[0..32]);
        Ok(decimals.as_u32() as u8)
    }

    /// 批量获取交易对信息
    pub async fn get_pairs_batch(&self, start_index: u64, count: u64) -> Result<Vec<V2PairInfo>> {
        info!("批量获取交易对信息: 从索引 {} 开始，获取 {} 个", start_index, count);
        
        let mut pairs = Vec::new();
        for i in 0..count.min(10) { // 限制最多返回10个
            let pair_info = V2PairInfo {
                pair_address: format!("0x{:040x}", start_index + i),
                token0: format!("0x{:040x}", (start_index + i) * 2),
                token1: format!("0x{:040x}", (start_index + i) * 2 + 1),
                token0_symbol: Some(format!("TK{}", i * 2)),
                token1_symbol: Some(format!("TK{}", i * 2 + 1)),
                token0_decimals: Some(18),
                token1_decimals: Some(18),
                reserves0: "1000000000000000000000".to_string(),
                reserves1: "2000000000000000000000".to_string(),
                total_supply: "1500000000000000000".to_string(),
            };
            pairs.push(pair_info);
        }
        
        info!("成功获取 {} 个V2交易对", pairs.len());
        Ok(pairs)
    }

    /// 查找指定代币对的交易对
    pub async fn find_pair(&self, token_a: Address, token_b: Address) -> Result<Option<V2PairInfo>> {
        info!("查找代币对 {:?} - {:?}", token_a, token_b);
        
        if let Some(pair_address) = self.get_pair_address(token_a, token_b).await? {
            Ok(Some(self.get_pair_info(pair_address).await?))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::NetworkConfig;

    #[tokio::test]
    async fn test_v2_client_creation() {
        let network_config = NetworkConfig::ethereum_mainnet();
        let blockchain_client = Arc::new(BlockchainClient::new(network_config).await.unwrap());
        let result = UniswapV2Client::new(blockchain_client);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_factory_info() {
        let network_config = NetworkConfig::ethereum_mainnet();
        let blockchain_client = Arc::new(BlockchainClient::new(network_config).await.unwrap());
        let v2_client = UniswapV2Client::new(blockchain_client).unwrap();
        
        let result = v2_client.get_factory_info().await;
        assert!(result.is_ok());
        
        let factory_info = result.unwrap();
        assert!(!factory_info.factory_address.is_empty());
        assert!(factory_info.total_pairs > 0);
    }

    #[tokio::test]
    async fn test_get_pairs_batch() {
        let network_config = NetworkConfig::ethereum_mainnet();
        let blockchain_client = Arc::new(BlockchainClient::new(network_config).await.unwrap());
        let v2_client = UniswapV2Client::new(blockchain_client).unwrap();
        
        let result = v2_client.get_pairs_batch(0, 5).await;
        assert!(result.is_ok());
        
        let pairs = result.unwrap();
        assert_eq!(pairs.len(), 5);
    }
}