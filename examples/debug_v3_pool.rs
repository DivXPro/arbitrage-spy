use arbitrage_spy::data::{
    blockchain_client::{BlockchainClient, NetworkConfig},
    uniswap_v3_client::UniswapV3Client,
};
use ethers::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Uniswap V3 池诊断工具");
    println!("========================");
    
    // 初始化区块链客户端
    let config = NetworkConfig::ethereum_mainnet();
    let blockchain_client = Arc::new(BlockchainClient::new(config).await?);
    println!("✅ 区块链客户端初始化成功");
    
    // 创建V3客户端
    let v3_client = UniswapV3Client::new(blockchain_client.clone())?;
    println!("✅ V3客户端创建成功");
    
    // 测试已知的真实V3池地址
    let known_pools = vec![
        // USDC/WETH 0.05% - 这是一个非常活跃的池
        ("USDC/WETH 0.05%", "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640"),
        // WETH/USDT 0.05% 
        ("WETH/USDT 0.05%", "0x11b815efb8f581194ae79006d24e0d814b7697f6"),
        // DAI/USDC 0.01%
        ("DAI/USDC 0.01%", "0x5777d92f208679db4b9778590fa3cab3ac9e2168"),
    ];
    
    for (name, address_str) in known_pools.iter() {
        println!("\n--- 诊断池: {} ---", name);
        
        match address_str.parse::<Address>() {
            Ok(pool_address) => {
                println!("📍 池地址: {:?}", pool_address);
                
                // 步骤1: 检查地址是否为合约
                println!("🔍 步骤1: 检查是否为合约地址...");
                match blockchain_client.provider().get_code(pool_address, None).await {
                    Ok(code) => {
                        if code.is_empty() {
                            println!("❌ 地址 {} 不是合约地址", address_str);
                            continue;
                        } else {
                            println!("✅ 确认为合约地址 (代码长度: {} 字节)", code.len());
                        }
                    },
                    Err(e) => {
                        println!("❌ 无法获取合约代码: {}", e);
                        continue;
                    }
                }
                
                // 步骤2: 尝试调用最简单的方法 - token0()
                println!("🔍 步骤2: 测试 token0() 方法调用...");
                let token0_call_data = ethers::utils::hex::decode("0dfe1681")?; // token0()
                match blockchain_client.call_contract(pool_address, token0_call_data.into()).await {
                    Ok(result) => {
                        println!("✅ token0() 调用成功 (返回数据长度: {} 字节)", result.len());
                        if result.len() >= 32 {
                            let token0_bytes = &result[12..32];
                            let token0_address = Address::from_slice(token0_bytes);
                            println!("   Token0 地址: {:?}", token0_address);
                        }
                    },
                    Err(e) => {
                        println!("❌ token0() 调用失败: {}", e);
                        continue;
                    }
                }
                
                // 步骤3: 尝试调用 fee() 方法
                println!("🔍 步骤3: 测试 fee() 方法调用...");
                let fee_call_data = ethers::utils::hex::decode("ddca3f43")?; // fee()
                match blockchain_client.call_contract(pool_address, fee_call_data.into()).await {
                    Ok(result) => {
                        println!("✅ fee() 调用成功 (返回数据长度: {} 字节)", result.len());
                        if result.len() >= 32 {
                            let fee_bytes = &result[29..32];
                            let fee = u32::from_be_bytes([0, fee_bytes[0], fee_bytes[1], fee_bytes[2]]);
                            println!("   手续费: {}", fee);
                        }
                    },
                    Err(e) => {
                        println!("❌ fee() 调用失败: {}", e);
                        continue;
                    }
                }
                
                // 步骤4: 尝试完整的 get_pool_info 调用
                println!("🔍 步骤4: 测试完整的 get_pool_info 调用...");
                match v3_client.get_pool_info(pool_address).await {
                    Ok(pool_info) => {
                        println!("✅ 完整池信息获取成功:");
                        println!("   池地址: {}", pool_info.pool_address);
                        println!("   Token0: {}", pool_info.token0);
                        println!("   Token1: {}", pool_info.token1);
                        println!("   手续费: {}", pool_info.fee);
                        println!("   Token0符号: {:?}", pool_info.token0_symbol);
                        println!("   Token1符号: {:?}", pool_info.token1_symbol);
                        println!("   流动性: {}", pool_info.liquidity);
                    },
                    Err(e) => {
                        println!("❌ 完整池信息获取失败: {}", e);
                    }
                }
                
                println!("✅ 池 {} 诊断完成", name);
            },
            Err(e) => {
                println!("❌ 无效的地址格式: {}", e);
            }
        }
    }
    
    println!("\n🎯 诊断总结:");
    println!("1. 如果所有步骤都失败，可能是网络连接问题");
    println!("2. 如果步骤1失败，说明地址不是合约");
    println!("3. 如果步骤2-3失败，说明不是V3池合约");
    println!("4. 如果步骤4失败，说明我们的实现有问题");
    
    Ok(())
}