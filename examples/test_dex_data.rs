use arbitrage_spy::store::{DexDataManager, DexType, NetworkConfig};
use ethers::types::Address;
use std::str::FromStr;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 开始测试DEX数据获取功能...\n");

    // 初始化DexDataManager，启用所有DEX类型
    let mut dex_manager = DexDataManager::ethereum_all_dex().await?;

    // 测试代币地址 (USDC/WETH)
    let usdc_address = Address::from_str("0xA0b86a33E6441c8C4505B4afDcA7FBf0251f7046")?;
    let weth_address = Address::from_str("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2")?;

    println!("📊 测试代币地址:");
    println!("USDC: {}", usdc_address);
    println!("WETH: {}\n", weth_address);

    // 测试V2功能
    println!("🔄 测试Uniswap V2功能...");
    test_v2_functionality(&mut dex_manager, usdc_address, weth_address).await?;

    // 测试V3功能
    println!("\n🔄 测试Uniswap V3功能...");
    test_v3_functionality(&mut dex_manager, usdc_address, weth_address).await?;

    println!("\n✅ 所有测试完成!");
    Ok(())
}

async fn test_v2_functionality(
    dex_manager: &mut DexDataManager,
    token0: Address,
    token1: Address,
) -> Result<(), Box<dyn std::error::Error>> {
    // 获取所有Factory信息
    match dex_manager.get_all_factory_info().await {
        Ok(factory_infos) => {
            for factory_info in factory_infos {
                if factory_info.dex_type == DexType::UniswapV2 {
                    println!("✅ V2 Factory信息:");
                    println!("   地址: {}", factory_info.factory_address);
                    println!("   类型: {:?}", factory_info.dex_type);
                }
            }
        }
        Err(e) => println!("❌ 获取Factory信息失败: {}", e),
    }

    // 获取V2交易对信息（批量获取前10个）
    match dex_manager.get_v2_pairs_batch(0, 10).await {
        Ok(pairs) => {
            println!("✅ 获取到 {} 个V2交易对:", pairs.len());
            for (i, pair) in pairs.iter().take(3).enumerate() {
                println!("   交易对 {}: {}", i + 1, pair.pair_address);
                println!("     Token0: {}", pair.token0);
                println!("     Token1: {}", pair.token1);
                if let (Some(r0), Some(r1)) = (&pair.reserves0, &pair.reserves1) {
                    println!("     Reserve0: {}", r0);
                    println!("     Reserve1: {}", r1);
                }
            }
        }
        Err(e) => println!("❌ 获取V2交易对信息失败: {}", e),
    }

    Ok(())
}

async fn test_v3_functionality(
    dex_manager: &mut DexDataManager,
    token0: Address,
    token1: Address,
) -> Result<(), Box<dyn std::error::Error>> {
    // 获取所有Factory信息
    match dex_manager.get_all_factory_info().await {
        Ok(factory_infos) => {
            for factory_info in factory_infos {
                if factory_info.dex_type == DexType::UniswapV3 {
                    println!("✅ V3 Factory信息:");
                    println!("   地址: {}", factory_info.factory_address);
                    println!("   类型: {:?}", factory_info.dex_type);
                }
            }
        }
        Err(e) => println!("❌ 获取Factory信息失败: {}", e),
    }

    // 查找V3池
    let token_pairs = vec![(token0, token1)];
    match dex_manager.find_v3_pools(&token_pairs).await {
        Ok(pools) => {
            if pools.is_empty() {
                println!("⚠️  未找到V3池");
            } else {
                println!("✅ 找到 {} 个V3池:", pools.len());
                for (i, pool) in pools.iter().enumerate() {
                    println!("   池 {}: {}", i + 1, pool.pair_address);
                    if let Some(fee) = pool.fee {
                        println!("     手续费: {}", fee);
                    }
                    if let Some(price) = &pool.sqrt_price_x96 {
                        println!("     当前价格: {}", price);
                    }
                    if let Some(liquidity) = &pool.liquidity {
                        println!("     流动性: {}", liquidity);
                    }
                    if let Some(tick) = pool.tick {
                        println!("     当前tick: {}", tick);
                    }
                }
            }
        }
        Err(e) => println!("❌ 查找V3池失败: {}", e),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dex_manager_initialization() {
        let result = DexDataManager::ethereum_all_dex().await;
        assert!(result.is_ok(), "DexDataManager初始化应该成功");
    }

    #[tokio::test]
    async fn test_network_configs() {
        // 测试不同网络配置
        let ethereum_config = NetworkConfig::ethereum();
        let bsc_config = NetworkConfig::bsc();
        let polygon_config = NetworkConfig::polygon();

        assert_eq!(ethereum_config.chain_id, 1);
        assert_eq!(bsc_config.chain_id, 56);
        assert_eq!(polygon_config.chain_id, 137);

        println!("✅ 网络配置测试通过");
    }

    #[tokio::test]
    async fn test_address_parsing() {
        let usdc_str = "0xA0b86a33E6441c8C06DD2b7c94b7E6E42342f8e";
        let result = Address::from_str(usdc_str);
        assert!(result.is_ok(), "地址解析应该成功");
        println!("✅ 地址解析测试通过");
    }
}