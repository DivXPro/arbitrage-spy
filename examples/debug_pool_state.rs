use arbitrage_spy::data::{BlockchainClient, UniswapV3Client};
use ethers::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Uniswap V3 池状态方法详细诊断");
    println!("================================");

    // 初始化客户端
    let blockchain_client = Arc::new(BlockchainClient::ethereum().await?);
    let v3_client = UniswapV3Client::new(blockchain_client.clone())?;

    // 测试多个不同的池
    let test_pools = vec![
        ("USDC/WETH 0.05%", "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640"),
        ("WETH/USDT 0.05%", "0x11b815efb8f581194ae79006d24e0d814b7697f6"),
        ("DAI/USDC 0.01%", "0x5777d92f208679db4b9778590fa3cab3ac9e2168"),
        ("WBTC/WETH 0.3%", "0xcbcdf9626bc03e24f779434178a73a0b4bad62ed"),
        ("USDC/USDT 0.01%", "0x3416cf6c708da44db2624d63ea0aaef7113527c6"),
    ];

    for (name, address_str) in test_pools {
        println!("\n📊 测试池: {} - {}", name, address_str);
        println!("----------------------------------------");
        
        let pool_address: Address = address_str.parse()?;
        
        // 检查合约是否存在
        match blockchain_client.provider().get_code(pool_address, None).await {
            Ok(code) => {
                if code.is_empty() {
                    println!("❌ 合约不存在");
                    continue;
                } else {
                    println!("✅ 合约存在 (代码长度: {} bytes)", code.len());
                }
            }
            Err(e) => {
                println!("❌ 检查合约失败: {}", e);
                continue;
            }
        }

        // 测试基本方法
        test_basic_methods(&v3_client, pool_address).await;
        
        // 测试状态方法
        test_state_methods(&v3_client, pool_address).await;
    }

    println!("\n🏁 诊断完成");
    Ok(())
}

async fn test_basic_methods(v3_client: &UniswapV3Client, pool_address: Address) {
    println!("\n🔧 基本方法测试:");
    
    let basic_methods = vec!["token0()", "token1()", "fee()"];
    
    for method in basic_methods {
        match v3_client.test_call_pool_method(pool_address, method).await {
            Ok(_) => println!("   ✅ {} 成功", method),
            Err(e) => println!("   ❌ {} 失败: {}", method, e),
        }
    }
}

async fn test_state_methods(v3_client: &UniswapV3Client, pool_address: Address) {
    println!("\n📊 状态方法测试:");
    
    let state_methods = vec![
        "slot0()",
        "liquidity()",
        "feeGrowthGlobal0X128()",
        "feeGrowthGlobal1X128()",
    ];
    
    for method in state_methods {
        match v3_client.test_call_pool_method(pool_address, method).await {
            Ok(result) => {
                println!("   ✅ {} 成功 (数据长度: {} bytes)", method, result.len());
                if method == "liquidity()" && result.len() >= 16 {
                    // 尝试解析liquidity值
                    let liquidity_bytes = &result[result.len()-16..];
                    println!("      💧 Liquidity 数据: {:?}", liquidity_bytes);
                }
            },
            Err(e) => println!("   ❌ {} 失败: {}", method, e),
        }
    }
}