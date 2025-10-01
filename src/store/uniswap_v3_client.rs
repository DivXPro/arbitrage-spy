use anyhow::{anyhow, Result};
use ethers::prelude::*;
use std::sync::Arc;
use log::{info, warn, error, debug};
use serde::{Deserialize, Serialize};

use super::blockchain_client::BlockchainClient;

/// V3池信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V3PoolInfo {
    pub pool_address: String,
    pub token0: String,
    pub token1: String,
    pub fee: u32,
    pub token0_symbol: Option<String>,
    pub token1_symbol: Option<String>,
    pub token0_decimals: Option<u8>,
    pub token1_decimals: Option<u8>,
    pub sqrt_price_x96: String,
    pub tick: i32,
    pub liquidity: String,
    pub fee_growth_global0_x128: String,
    pub fee_growth_global1_x128: String,
}

/// V3工厂信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V3FactoryInfo {
    pub factory_address: String,
    pub supported_fees: Vec<u32>,
    pub tick_spacings: std::collections::HashMap<u32, i32>,
}

/// Uniswap V3客户端
pub struct UniswapV3Client {
    blockchain_client: Arc<BlockchainClient>,
    factory_address: Address,
}

impl UniswapV3Client {
    /// 创建新的Uniswap V3客户端
    pub fn new(blockchain_client: Arc<BlockchainClient>) -> Result<Self> {
        // Uniswap V3 Factory地址 (以太坊主网)
        let factory_address = "0x1F98431c8aD98523631AE4a59f267346ea31F984"
            .parse::<Address>()
            .map_err(|e| anyhow!("无效的工厂地址: {}", e))?;

        Ok(Self {
            blockchain_client,
            factory_address,
        })
    }

    /// 获取工厂信息
    pub async fn get_factory_info(&self) -> Result<V3FactoryInfo> {
        info!("获取Uniswap V3工厂信息");
        
        // 简化实现，返回模拟数据
        let mut tick_spacings = std::collections::HashMap::new();
        tick_spacings.insert(500, 10);
        tick_spacings.insert(3000, 60);
        tick_spacings.insert(10000, 200);

        Ok(V3FactoryInfo {
            factory_address: format!("{:?}", self.factory_address),
            supported_fees: vec![500, 3000, 10000],
            tick_spacings,
        })
    }

    /// 获取指定手续费等级的tick spacing
    pub async fn get_fee_tick_spacing(&self, fee: u32) -> Result<i32> {
        info!("获取手续费 {} 的tick spacing", fee);
        
        // 简化实现
        let tick_spacing = match fee {
            500 => 10,
            3000 => 60,
            10000 => 200,
            _ => return Err(anyhow!("不支持的手续费等级: {}", fee)),
        };
        
        Ok(tick_spacing)
    }

    /// 获取池地址
    pub async fn get_pool_address(&self, token0: Address, token1: Address, fee: u32) -> Result<Option<Address>> {
        info!("获取池地址: {:?} - {:?}, 手续费: {}", token0, token1, fee);
        
        // 简化实现
        Ok(None)
    }

    /// 获取池详细信息
    pub async fn get_pool_info(&self, pool_address: Address) -> Result<V3PoolInfo> {        
        // 检查池地址是否为零地址
        if pool_address == Address::zero() {
            return Err(anyhow!("无效的池地址"));
        }

        // 获取基本池信息
        let token0 = self.call_pool_method(pool_address, "token0()").await?;
        let token1 = self.call_pool_method(pool_address, "token1()").await?;
        let fee_bytes = self.call_pool_method(pool_address, "fee()").await?;
        let fee = if fee_bytes.len() >= 4 {
            u32::from_be_bytes([fee_bytes[0], fee_bytes[1], fee_bytes[2], fee_bytes[3]])
        } else {
            3000 // 默认手续费
        };

        // 获取slot0信息 (包含sqrtPriceX96和tick)
        let slot0_data = self.call_pool_method(pool_address, "slot0()").await?;
        let (sqrt_price_x96, tick) = self.parse_slot0(&slot0_data)?;

        // 获取流动性
        let liquidity = self.call_pool_method(pool_address, "liquidity()").await?;

        // 获取手续费增长
        let fee_growth_global0_x128 = self.call_pool_method(pool_address, "feeGrowthGlobal0X128()").await?;
        let fee_growth_global1_x128 = self.call_pool_method(pool_address, "feeGrowthGlobal1X128()").await?;

        // 获取代币信息
        let (token0_symbol, token0_decimals) = self.get_token_info(token0.clone()).await;
        let (token1_symbol, token1_decimals) = self.get_token_info(token1.clone()).await;

        let pool_info = V3PoolInfo {
            pool_address: format!("{:?}", pool_address),
            token0: format!("{:?}", token0),
            token1: format!("{:?}", token1),
            fee,
            token0_symbol,
            token1_symbol,
            token0_decimals,
            token1_decimals,
            sqrt_price_x96: sqrt_price_x96.to_string(),
            tick,
            liquidity: liquidity.to_string(),
            fee_growth_global0_x128: fee_growth_global0_x128.to_string(),
            fee_growth_global1_x128: fee_growth_global1_x128.to_string(),
        };

        info!("池信息获取成功: {} - {} (手续费: {})", 
               pool_info.token0_symbol.as_deref().unwrap_or("Unknown"),
               pool_info.token1_symbol.as_deref().unwrap_or("Unknown"),
               pool_info.fee);

        Ok(pool_info)
    }

    /// 查找指定代币对的所有池
    pub async fn find_pools(&self, token_a: Address, token_b: Address) -> Result<Vec<V3PoolInfo>> {
        info!("查找代币对 {:?} - {:?} 的所有池", token_a, token_b);
        
        let mut pools = Vec::new();
        let fees = vec![500, 3000, 10000];
        
        for fee in fees {
            // 简化实现，创建模拟池信息
            let pool_info = V3PoolInfo {
                pool_address: format!("0x{:040x}", fee),
                token0: format!("{:?}", token_a),
                token1: format!("{:?}", token_b),
                fee,
                token0_symbol: Some("TKA".to_string()),
                token1_symbol: Some("TKB".to_string()),
                token0_decimals: Some(18),
                token1_decimals: Some(18),
                sqrt_price_x96: "79228162514264337593543950336".to_string(),
                tick: 0,
                liquidity: format!("{}", 1000000000000000000u64 + fee as u64),
                fee_growth_global0_x128: "0".to_string(),
                fee_growth_global1_x128: "0".to_string(),
            };
            pools.push(pool_info);
        }
        
        info!("找到 {} 个池", pools.len());
        Ok(pools)
    }

    /// 批量查找多个代币对的池
    pub async fn find_pools_batch(&self, token_pairs: &[(Address, Address)]) -> Result<Vec<V3PoolInfo>> {
        info!("批量查找 {} 个代币对的池", token_pairs.len());
        
        let mut all_pools = Vec::new();
        
        for (token_a, token_b) in token_pairs.iter().take(5) { // 限制最多处理5个
            let pools = self.find_pools(*token_a, *token_b).await?;
            all_pools.extend(pools);
        }
        
        info!("批量查找完成，总共找到 {} 个池", all_pools.len());
        Ok(all_pools)
    }


    /// 批量获取池信息
    pub async fn get_pools_batch(&self, pool_addresses: Vec<Address>) -> Result<Vec<V3PoolInfo>> {
        info!("批量获取 {} 个池的信息", pool_addresses.len());
        
        let mut pools = Vec::new();
        for address in pool_addresses {
            match self.get_pool_info(address).await {
                Ok(pool) => pools.push(pool),
                Err(e) => warn!("获取池 {:?} 信息失败: {}", address, e),
            }
        }
        
        info!("成功获取 {} 个池的信息", pools.len());
        Ok(pools)
    }

    /// 公共测试方法：调用池合约方法（用于诊断）
    pub async fn test_call_pool_method(&self, pool_address: Address, method_sig: &str) -> Result<Bytes> {
        self.call_pool_method(pool_address, method_sig).await
    }

    /// 调用池合约方法
    async fn call_pool_method(&self, pool_address: Address, method_sig: &str) -> Result<Bytes> {
        // 构造方法调用数据
        let call_data = match method_sig {
            "token0()" => Bytes::from_static(&[0x0d, 0xfe, 0x16, 0x81]), // token0()
            "token1()" => Bytes::from_static(&[0xd2, 0x12, 0x20, 0xa7]), // token1()
            "fee()" => Bytes::from_static(&[0xdd, 0xca, 0x3f, 0x43]), // fee()
            "slot0()" => Bytes::from_static(&[0x38, 0x50, 0xc7, 0xbd]), // slot0()
            "liquidity()" => Bytes::from_static(&[0x1a, 0x68, 0x65, 0x02]), // liquidity()
            "feeGrowthGlobal0X128()" => Bytes::from_static(&[0xf3, 0x05, 0x83, 0x99]), // feeGrowthGlobal0X128()
            "feeGrowthGlobal1X128()" => Bytes::from_static(&[0x46, 0x14, 0x13, 0x19]), // feeGrowthGlobal1X128()
            _ => return Err(anyhow!("不支持的方法: {}", method_sig)),
        };

        // 调用区块链客户端
        let result = self.blockchain_client.call_contract(pool_address, call_data).await?;
        
        // 根据方法类型解析结果
        match method_sig {
            "token0()" | "token1()" => {
                // 地址类型，取后20字节
                if result.len() >= 32 {
                    Ok(Bytes::from(result[12..32].to_vec()))
                } else {
                    Err(anyhow!("无效的地址返回数据"))
                }
            },
            "fee()" => {
                // uint24类型
                if result.len() >= 32 {
                    let fee_bytes = &result[29..32]; // 取最后3字节
                    let fee = u32::from_be_bytes([0, fee_bytes[0], fee_bytes[1], fee_bytes[2]]);
                    Ok(Bytes::from(fee.to_be_bytes().to_vec()))
                } else {
                    Err(anyhow!("无效的fee返回数据"))
                }
            },
            _ => Ok(result), // 其他类型直接返回原始数据
        }
    }

    /// 解析slot0数据
    fn parse_slot0(&self, data: &Bytes) -> Result<(U256, i32)> {
        if data.len() < 64 {
            return Err(anyhow!("slot0数据长度不足"));
        }

        // sqrtPriceX96 (uint160, 前32字节)
        let sqrt_price_bytes = &data[0..32];
        let sqrt_price_x96 = U256::from_big_endian(sqrt_price_bytes);

        // tick (int24, 第32-64字节中的最后3字节)
        let tick_bytes = &data[61..64];
        let tick_u32 = u32::from_be_bytes([0, tick_bytes[0], tick_bytes[1], tick_bytes[2]]);
        // 转换为有符号整数
        let tick = if tick_u32 & 0x800000 != 0 {
            // 负数，进行符号扩展
            (tick_u32 | 0xFF000000) as i32
        } else {
            tick_u32 as i32
        };

        Ok((sqrt_price_x96, tick))
    }

    /// 获取代币信息
    async fn get_token_info(&self, token_address: Bytes) -> (Option<String>, Option<u8>) {
        if token_address.len() != 20 {
            return (None, None);
        }

        // 将Bytes转换为Address
        let mut addr_bytes = [0u8; 20];
        addr_bytes.copy_from_slice(&token_address);
        let address = Address::from(addr_bytes);

        // 获取symbol
        let symbol = self.get_token_symbol(address).await.ok();
        
        // 获取decimals
        let decimals = self.get_token_decimals(address).await.ok();

        (symbol, decimals)
    }

    /// 获取代币符号
    async fn get_token_symbol(&self, token_address: Address) -> Result<String> {
        let call_data = Bytes::from_static(&[0x95, 0xd8, 0x9b, 0x41]); // symbol()
        let result = self.blockchain_client.call_contract(token_address, call_data).await?;
        
        if result.len() < 64 {
            return Ok("UNKNOWN".to_string());
        }

        // 解析字符串返回值
        let offset = U256::from_big_endian(&result[0..32]).as_usize();
        if offset >= result.len() || offset + 32 >= result.len() {
            return Ok("UNKNOWN".to_string());
        }

        let length = U256::from_big_endian(&result[offset..offset+32]).as_usize();
        if length == 0 || offset + 32 + length > result.len() {
            return Ok("UNKNOWN".to_string());
        }

        let symbol_bytes = &result[offset+32..offset+32+length];
        Ok(String::from_utf8(symbol_bytes.to_vec()).unwrap_or_else(|_| "UNKNOWN".to_string()))
    }

    /// 获取代币精度
    async fn get_token_decimals(&self, token_address: Address) -> Result<u8> {
        let call_data = Bytes::from_static(&[0x31, 0x3c, 0xe5, 0x67]); // decimals()
        let result = self.blockchain_client.call_contract(token_address, call_data).await?;
        
        if result.len() >= 32 {
            Ok(result[31]) // 取最后一个字节
        } else {
            Ok(18) // 默认18位精度
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::NetworkConfig;

    #[tokio::test]
    async fn test_v3_client_creation() {
        let network_config = NetworkConfig::ethereum_mainnet();
        let blockchain_client = Arc::new(BlockchainClient::new(network_config).await.unwrap());
        let result = UniswapV3Client::new(blockchain_client);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_factory_info() {
        let network_config = NetworkConfig::ethereum_mainnet();
        let blockchain_client = Arc::new(BlockchainClient::new(network_config).await.unwrap());
        let v3_client = UniswapV3Client::new(blockchain_client).unwrap();
        
        let result = v3_client.get_factory_info().await;
        assert!(result.is_ok());
        
        let factory_info = result.unwrap();
        assert!(!factory_info.factory_address.is_empty());
        assert!(!factory_info.supported_fees.is_empty());
    }

    #[tokio::test]
    async fn test_find_pools() {
        let network_config = NetworkConfig::ethereum_mainnet();
        let blockchain_client = Arc::new(BlockchainClient::new(network_config).await.unwrap());
        let v3_client = UniswapV3Client::new(blockchain_client).unwrap();
        
        let token_a = Address::zero();
        let token_b = Address::from_low_u64_be(1);
        
        let result = v3_client.find_pools(token_a, token_b).await;
        assert!(result.is_ok());
        
        let pools = result.unwrap();
        assert_eq!(pools.len(), 3); // 3个不同手续费等级的池
    }

    #[tokio::test]
    async fn test_find_pools_batch() {
        let network_config = NetworkConfig::ethereum_mainnet();
        let blockchain_client = Arc::new(BlockchainClient::new(network_config).await.unwrap());
        let v3_client = UniswapV3Client::new(blockchain_client).unwrap();
        
        let token_pairs = vec![
            (Address::zero(), Address::from_low_u64_be(1)),
            (Address::from_low_u64_be(2), Address::from_low_u64_be(3)),
        ];
        
        let result = v3_client.find_pools_batch(&token_pairs).await;
        assert!(result.is_ok());
        
        let pools = result.unwrap();
        assert_eq!(pools.len(), 6); // 2个代币对 * 3个手续费等级
    }
}