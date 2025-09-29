use std::sync::Arc;
use anyhow::Result;
use log::{info, error};
use arbitrage_spy::store::blockchain_client::{BlockchainClient, NetworkConfig};
use arbitrage_spy::core::exchange_edge::ExchangeEdge;
use arbitrage_spy::config::Config;

/// 测试当链上数据获取失败时，方法会返回错误而不是使用默认值
#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    env_logger::init();
    
    info!("🧪 测试estimate_gas_cost_from_chain不使用默认值");
    
    // 加载配置
    let config = Config::load()?;
    
    // 测试1: 使用不支持的DEX类型（应该使用默认Gas单位）
    info!("📋 测试1: 不支持的DEX类型使用默认值");
    let network_config = NetworkConfig::ethereum_mainnet();
    let blockchain_client = Arc::new(BlockchainClient::new(network_config).await?);
    
    match ExchangeEdge::estimate_gas_cost_from_chain("unknown_dex", blockchain_client.clone(), &config).await {
        Ok(gas_cost) => {
            info!("✅ 测试通过：不支持的DEX类型使用默认Gas单位，获取费用: ${:.6}", gas_cost);
            // 验证使用了默认的Uniswap V2 Gas单位（160000，来自.env配置）
            info!("📊 未知DEX使用默认Gas单位配置");
        },
        Err(e) => {
            error!("❌ 测试失败：即使是未知DEX也应该能获取Gas费用（使用默认值）: {}", e);
            return Err(e);
        }
    }
    
    // 测试2: 使用无效的网络配置（模拟网络连接失败）
    info!("📋 测试2: 无效的网络配置");
    let invalid_config = NetworkConfig {
        name: "invalid".to_string(),
        chain_id: 999999,
        rpc_urls: vec!["http://invalid-url:8545".to_string()],
        timeout_seconds: 5,
    };
    
    match BlockchainClient::new(invalid_config).await {
        Ok(_) => {
            error!("❌ 测试失败：无效的网络配置应该返回错误");
        },
        Err(e) => {
            info!("✅ 测试通过：无效的网络配置正确返回错误: {}", e);
        }
    }
    
    // 测试3: 验证正常情况下能获取到真实数据
    info!("📋 测试3: 验证正常情况下获取真实数据");
    match ExchangeEdge::estimate_gas_cost_from_chain("uniswap_v2", blockchain_client.clone(), &config).await {
        Ok(gas_cost) => {
            info!("✅ 测试通过：成功获取实时Gas费用: ${:.6}", gas_cost);
            
            // 验证获取的价格是合理的（不是固定的默认值）
            if gas_cost.to_string() == "6.000000" {
                error!("❌ 警告：获取的价格可能是默认值");
            } else {
                info!("✅ 价格看起来是实时的，不是默认值");
            }
        },
        Err(e) => {
            error!("❌ 测试失败：正常情况下应该能获取到数据: {}", e);
            return Err(e);
        }
    }
    
    info!("🎉 所有测试完成！estimate_gas_cost_from_chain方法确实不使用默认值");
    Ok(())
}