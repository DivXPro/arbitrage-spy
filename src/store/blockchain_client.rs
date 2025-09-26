use anyhow::{anyhow, Result};
use ethers::prelude::*;
use ethers::abi::{Tokenize, Detokenize};
use std::sync::Arc;
use std::time::Duration;
use std::env;
use log::{info, warn, error};
use serde::{Deserialize, Serialize};

/// 区块链网络配置
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub name: String,
    pub chain_id: u64,
    pub rpc_urls: Vec<String>,
    pub timeout_seconds: u64,
}

impl NetworkConfig {
    /// 以太坊主网配置
    pub fn ethereum_mainnet() -> Self {
        // 从环境变量读取 RPC_URL，支持多个地址用分号分割
        let rpc_urls = match env::var("RPC_URL") {
            Ok(urls_str) => {
                // 按分号分割，过滤空字符串，并去除首尾空格
                urls_str
                    .split(';')
                    .map(|url| url.trim().to_string())
                    .filter(|url| !url.is_empty())
                    .collect::<Vec<String>>()
            }
            Err(_) => {
                // 如果环境变量不存在，使用默认值
                vec!["https://eth-mainnet.g.alchemy.com/v2/your_key".to_string()]
            }
        };
        
        // 确保至少有一个 RPC URL
        let final_urls = if rpc_urls.is_empty() {
            vec!["https://eth-mainnet.g.alchemy.com/v2/your_key".to_string()]
        } else {
            rpc_urls
        };
        
        Self {
            name: "Ethereum Mainnet".to_string(),
            chain_id: 1,
            rpc_urls: final_urls,
            timeout_seconds: 30,
        }
    }

    /// BSC主网配置
    pub fn bsc_mainnet() -> Self {
        Self {
            name: "BSC Mainnet".to_string(),
            chain_id: 56,
            rpc_urls: vec![
                "https://bsc-dataseed.binance.org".to_string(),
                "https://rpc.ankr.com/bsc".to_string(),
                "https://bsc.publicnode.com".to_string(),
            ],
            timeout_seconds: 30,
        }
    }

    /// Polygon主网配置
    pub fn polygon_mainnet() -> Self {
        Self {
            name: "Polygon Mainnet".to_string(),
            chain_id: 137,
            rpc_urls: vec![
                "https://polygon-rpc.com".to_string(),
                "https://rpc.ankr.com/polygon".to_string(),
                "https://polygon.publicnode.com".to_string(),
            ],
            timeout_seconds: 30,
        }
    }
}

/// 区块链客户端管理器
pub struct BlockchainClient {
    provider: Arc<Provider<Http>>,
    network_config: NetworkConfig,
}

impl BlockchainClient {
    /// 创建新的区块链客户端
    pub async fn new(network_config: NetworkConfig) -> Result<Self> {
        let provider = Self::create_provider(&network_config).await?;
        
        Ok(Self {
            provider: Arc::new(provider),
            network_config,
        })
    }

    /// 创建以太坊主网客户端
    pub async fn ethereum() -> Result<Self> {
        Self::new(NetworkConfig::ethereum_mainnet()).await
    }

    /// 创建BSC主网客户端
    pub async fn bsc() -> Result<Self> {
        Self::new(NetworkConfig::bsc_mainnet()).await
    }

    /// 创建Polygon主网客户端
    pub async fn polygon() -> Result<Self> {
        Self::new(NetworkConfig::polygon_mainnet()).await
    }

    /// 创建Provider，尝试多个RPC端点
    async fn create_provider(config: &NetworkConfig) -> Result<Provider<Http>> {
        info!("尝试连接到 {} 网络...", config.name);

        for (index, rpc_url) in config.rpc_urls.iter().enumerate() {
            info!("🔄 尝试连接RPC端点 ({}/{}): {}", index + 1, config.rpc_urls.len(), rpc_url);
            
            match Self::test_rpc_connection(rpc_url, config).await {
                Ok(provider) => {
                    info!("✅ 成功连接到: {}", rpc_url);
                    return Ok(provider);
                }
                Err(e) => {
                    warn!("❌ 连接失败 {}: {}", rpc_url, e);
                    continue;
                }
            }
        }

        Err(anyhow!("无法连接到任何 {} RPC端点", config.name))
    }

    /// 测试RPC连接
    async fn test_rpc_connection(rpc_url: &str, config: &NetworkConfig) -> Result<Provider<Http>> {
        let provider = Provider::<Http>::try_from(rpc_url)?
            .interval(Duration::from_millis(100u64));

        // 测试连接
        let block_number = tokio::time::timeout(
            Duration::from_secs(config.timeout_seconds),
            provider.get_block_number()
        ).await??;

        // 验证链ID
        let chain_id = tokio::time::timeout(
            Duration::from_secs(config.timeout_seconds),
            provider.get_chainid()
        ).await??;

        if chain_id.as_u64() != config.chain_id {
            return Err(anyhow!(
                "链ID不匹配: 期望 {}, 实际 {}", 
                config.chain_id, 
                chain_id.as_u64()
            ));
        }

        info!("📊 当前区块高度: {}, 链ID: {}", block_number, chain_id);
        Ok(provider)
    }

    /// 获取Provider引用
    pub fn provider(&self) -> Arc<Provider<Http>> {
        self.provider.clone()
    }

    /// 获取网络配置
    pub fn network_config(&self) -> &NetworkConfig {
        &self.network_config
    }

    /// 健康检查
    pub async fn health_check(&self) -> Result<bool> {
        match tokio::time::timeout(
            Duration::from_secs(self.network_config.timeout_seconds),
            self.provider.get_block_number()
        ).await {
            Ok(Ok(_)) => {
                info!("{}: 区块链连接健康检查通过", self.network_config.name);
                Ok(true)
            }
            Ok(Err(e)) => {
                error!("{}: 区块链连接健康检查失败: {}", self.network_config.name, e);
                Ok(false)
            }
            Err(_) => {
                error!("{}: 区块链连接超时", self.network_config.name);
                Ok(false)
            }
        }
    }

    /// 获取当前区块号
    pub async fn get_block_number(&self) -> Result<U64> {
        Ok(self.provider.get_block_number().await?)
    }

    /// 获取链ID
    pub async fn get_chain_id(&self) -> Result<U256> {
        Ok(self.provider.get_chainid().await?)
    }

    /// 调用合约只读方法
    pub async fn call_contract(
        &self,
        contract_address: Address,
        function_data: Bytes,
    ) -> Result<Bytes> {
        let call_request = TransactionRequest::new()
            .to(contract_address)
            .data(function_data);

        let result = self.provider.call(&call_request.into(), None).await?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ethereum_connection() {
        let client = BlockchainClient::ethereum().await;
        assert!(client.is_ok());
        
        if let Ok(client) = client {
            let health = client.health_check().await;
            assert!(health.is_ok());
        }
    }
}