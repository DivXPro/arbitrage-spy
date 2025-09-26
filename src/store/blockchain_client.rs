use anyhow::{anyhow, Result};
use ethers::prelude::*;
use std::sync::Arc;
use std::time::Duration;
use std::env;
use log::{info, warn, error};

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
                vec!["https://eth-mainnet.g.alchemy.com/v2/VOgKLbFkWm760hlVNXCHq9RDKtFWebaG".to_string()]
            }
        };
        
        // 确保至少有一个 RPC URL
        let final_urls = if rpc_urls.is_empty() {
            vec!["https://eth-mainnet.g.alchemy.com/v2/VOgKLbFkWm760hlVNXCHq9RDKtFWebaG".to_string()]
        } else {
            rpc_urls
        };
        
        Self {
            name: "Ethereum Mainnet".to_string(),
            chain_id: 1,
            rpc_urls: final_urls,
            timeout_seconds: 60, // 增加超时时间到60秒
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
        // 创建自定义的HTTP客户端，配置更长的超时时间和连接设置
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .connect_timeout(Duration::from_secs(45)) // 增加连接超时
            .tcp_keepalive(Duration::from_secs(60))
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(10)
            .user_agent("arbitrage-spy/1.0")
            .build()?;
        
        // 使用自定义HTTP客户端创建Provider
        let url: reqwest::Url = rpc_url.parse()?;
        let http = Http::new_with_client(url, client);
        let provider = Provider::new(http)
            .interval(Duration::from_millis(500u64)); // 增加间隔时间

        // 重试机制：最多重试3次
        let mut last_error = None;
        for attempt in 1..=3 {
            info!("🔄 尝试连接 (第{}/3次): {}", attempt, rpc_url);
            info!("Timeout is {}", config.timeout_seconds);
            
            // 测试连接 - 使用更长的超时时间
            let block_result = tokio::time::timeout(
                Duration::from_secs(config.timeout_seconds),
                provider.get_block_number()
            ).await;

            let block_number = match block_result {
                Ok(Ok(block)) => block,
                Ok(Err(e)) => {
                    warn!("⚠️  第{}次尝试获取区块号失败: {}", attempt, e);
                    last_error = Some(anyhow!("获取区块号失败: {}", e));
                    if attempt < 3 {
                        tokio::time::sleep(Duration::from_secs(2)).await; // 等待2秒后重试
                        continue;
                    } else {
                        return Err(last_error.unwrap());
                    }
                }
                Err(_) => {
                    warn!("⚠️  第{}次尝试超时", attempt);
                    last_error = Some(anyhow!("连接超时"));
                    if attempt < 3 {
                        tokio::time::sleep(Duration::from_secs(2)).await; // 等待2秒后重试
                        continue;
                    } else {
                        return Err(last_error.unwrap());
                    }
                }
            };

            // 验证链ID
            let chain_result = tokio::time::timeout(
                Duration::from_secs(config.timeout_seconds),
                provider.get_chainid()
            ).await;

            let chain_id = match chain_result {
                Ok(Ok(id)) => id,
                Ok(Err(e)) => {
                    warn!("⚠️  第{}次尝试获取链ID失败: {}", attempt, e);
                    last_error = Some(anyhow!("获取链ID失败: {}", e));
                    if attempt < 3 {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    } else {
                        return Err(last_error.unwrap());
                    }
                }
                Err(_) => {
                    warn!("⚠️  第{}次尝试获取链ID超时", attempt);
                    last_error = Some(anyhow!("获取链ID超时"));
                    if attempt < 3 {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    } else {
                        return Err(last_error.unwrap());
                    }
                }
            };

            if chain_id.as_u64() != config.chain_id {
                return Err(anyhow!(
                    "链ID不匹配: 期望 {}, 实际 {}", 
                    config.chain_id, 
                    chain_id.as_u64()
                ));
            }

            info!("✅ 连接成功! 区块高度: {}, 链ID: {}", block_number, chain_id);
            return Ok(provider);
        }

        Err(last_error.unwrap_or_else(|| anyhow!("连接失败")))
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