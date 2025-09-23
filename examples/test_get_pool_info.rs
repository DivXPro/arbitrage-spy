use arbitrage_spy::store::{BlockchainClient, UniswapV3Client};
use ethers::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    println!("🔍 测试原始 get_pool_info 方法");
    println!("================================");

    // 初始化客户端
    let blockchain_client = Arc::new(BlockchainClient::ethereum().await?);
    let v3_client = UniswapV3Client::new(blockchain_client.clone())?;

    // 测试工作正常的池
    let working_pools = vec![
        ("USDC/WETH 0.05%", "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640"),
        ("DAI/USDC 0.01%", "0x5777d92f208679db4b9778590fa3cab3ac9e2168"),
        ("WBTC/WETH 0.3%", "0xcbcdf9626bc03e24f779434178a73a0b4bad62ed"),
    ];

    for (name, address_str) in working_pools {
        println!("\n📊 测试池: {} - {}", name, address_str);
        println!("----------------------------------------");
        
        let pool_address: Address = address_str.parse()?;
        
        match v3_client.get_pool_info(pool_address).await {
            Ok(pool_info) => {
                println!("✅ get_pool_info 成功！");
                println!("   📋 池地址: {}", pool_info.pool_address);
                println!("   🪙 Token0: {} ({})", 
                    pool_info.token0, 
                    pool_info.token0_symbol.as_deref().unwrap_or("未知")
                );
                println!("   🪙 Token1: {} ({})", 
                    pool_info.token1, 
                    pool_info.token1_symbol.as_deref().unwrap_or("未知")
                );
                println!("   💰 手续费: {} ({}%)", 
                    pool_info.fee, 
                    pool_info.fee as f64 / 10000.0
                );
                println!("   💧 流动性: {}", pool_info.liquidity);
                println!("   📈 SqrtPriceX96: {}", pool_info.sqrt_price_x96);
                println!("   🎯 Tick: {}", pool_info.tick);
                println!("   📊 FeeGrowthGlobal0: {}", pool_info.fee_growth_global0_x128);
                println!("   📊 FeeGrowthGlobal1: {}", pool_info.fee_growth_global1_x128);
            }
            Err(e) => {
                println!("❌ get_pool_info 失败: {}", e);
            }
        }
    }

    // 测试有问题的池
    println!("\n📊 测试有问题的池: WETH/USDT 0.05% - 0x11b815efb8f581194ae79006d24e0d814b7697f6");
    println!("----------------------------------------");
    
    let problem_pool: Address = "0x11b815efb8f581194ae79006d24e0d814b7697f6".parse()?;
    
    match v3_client.get_pool_info(problem_pool).await {
        Ok(pool_info) => {
            println!("✅ get_pool_info 成功！");
            println!("   📋 池地址: {}", pool_info.pool_address);
            println!("   🪙 Token0: {} ({})", 
                pool_info.token0, 
                pool_info.token0_symbol.as_deref().unwrap_or("未知")
            );
            println!("   🪙 Token1: {} ({})", 
                pool_info.token1, 
                pool_info.token1_symbol.as_deref().unwrap_or("未知")
            );
            println!("   💰 手续费: {} ({}%)", 
                pool_info.fee, 
                pool_info.fee as f64 / 10000.0
            );
            println!("   💧 流动性: {}", pool_info.liquidity);
            println!("   📈 SqrtPriceX96: {}", pool_info.sqrt_price_x96);
            println!("   🎯 Tick: {}", pool_info.tick);
            println!("   📊 FeeGrowthGlobal0: {}", pool_info.fee_growth_global0_x128);
            println!("   📊 FeeGrowthGlobal1: {}", pool_info.fee_growth_global1_x128);
        }
        Err(e) => {
            println!("❌ get_pool_info 失败: {}", e);
        }
    }

    println!("\n🏁 测试完成");
    Ok(())
}