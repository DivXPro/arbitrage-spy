use std::sync::Arc;
use anyhow::Result;
use log::{info, error};
use num_traits::{Zero, ToPrimitive};
use arbitrage_spy::store::blockchain_client::{BlockchainClient, NetworkConfig};
use arbitrage_spy::core::exchange_edge::ExchangeEdge;
use arbitrage_spy::config::Config;

/// 演示如何使用链上实时Gas费用获取功能
#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    env_logger::init();
    
    info!("🚀 开始演示链上Gas费用获取功能");
    
    // 创建区块链客户端
    let network_config = NetworkConfig::ethereum_mainnet();
    let blockchain_client = match BlockchainClient::new(network_config).await {
        Ok(client) => Arc::new(client),
        Err(e) => {
            error!("❌ 创建区块链客户端失败: {}", e);
            return Err(e);
        }
    };
    
    info!("✅ 区块链客户端创建成功");
    
    // 加载配置
    let config = Config::load()?;
    info!("✅ 配置加载成功");
    
    // 打印Gas配置信息
    info!("📊 Gas配置信息:");
    info!("  Uniswap V2: {} units", config.gas_config.uniswap_v2_gas_units);
    info!("  Uniswap V3: {} units", config.gas_config.uniswap_v3_gas_units);
    info!("  SushiSwap: {} units", config.gas_config.sushiswap_gas_units);
    info!("  PancakeSwap: {} units", config.gas_config.pancakeswap_gas_units);
    info!("  Balancer: {} units", config.gas_config.balancer_gas_units);
    info!("  Curve: {} units", config.gas_config.curve_gas_units);
    
    // 测试不同DEX的Gas费用获取
    let dex_list = vec![
        "uniswap_v2",
        "uniswap_v3", 
        "sushiswap",
        "curve",
        "balancer"
    ];
    
    for dex_name in dex_list {
        info!("📊 获取 {} 的实时Gas费用...", dex_name);
        
        match ExchangeEdge::estimate_gas_cost_from_chain(dex_name, blockchain_client.clone(), &config).await {
            Ok(gas_cost) => {
                info!("✅ {} 实时Gas费用: ${:.6}", dex_name, gas_cost);
                
                // 对比静态估算
                let static_cost = ExchangeEdge::estimate_gas_cost(dex_name);
                info!("📈 {} 静态估算费用: ${:.6}", dex_name, static_cost);
                
                let difference = &gas_cost - &static_cost;
                let percentage = if static_cost.is_zero() {
                    0.0
                } else {
                    (difference.clone() / static_cost.clone()).to_f64().unwrap_or(0.0) * 100.0
                };
                
                info!("📊 {} 差异: ${:.6} ({:.2}%)", dex_name, difference, percentage);
            }
            Err(e) => {
                error!("❌ 获取 {} Gas费用失败: {}", dex_name, e);
            }
        }
        
        println!(); // 添加空行分隔
    }
    
    info!("🎉 Gas费用获取演示完成");
    Ok(())
}