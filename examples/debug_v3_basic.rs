use anyhow::Result;
use ethers::prelude::*;
use std::sync::Arc;
use arbitrage_spy::data::{BlockchainClient, NetworkConfig, UniswapV3Client};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("🔍 Uniswap V3 基本池信息诊断工具");
    println!("================================");
    
    // 初始化客户端
    let blockchain_client = Arc::new(BlockchainClient::ethereum().await?);
    let v3_client = UniswapV3Client::new(blockchain_client.clone())?;
    
    // 测试池地址 - 使用真实的V3池地址
    let test_pools = vec![
        "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640", // USDC/WETH 0.05%
        "0x11b815efb8f581194ae79006d24e0d814b7697f6", // WETH/USDT 0.05%
        "0x5777d92f208679db4b9778590fa3cab3ac9e2168", // DAI/USDC 0.01%
    ];
    
    for (i, pool_addr_str) in test_pools.iter().enumerate() {
        println!("\n📊 测试池 {} - {}", i + 1, pool_addr_str);
        println!("----------------------------------------");
        
        let pool_address: Address = pool_addr_str.parse()?;
        
        // 步骤1: 检查合约地址
        println!("1️⃣ 检查合约地址...");
        match blockchain_client.provider().get_code(pool_address, None).await {
            Ok(code) => {
                if code.is_empty() {
                    println!("   ❌ 地址无合约代码");
                    continue;
                } else {
                    println!("   ✅ 合约存在 (代码长度: {} bytes)", code.len());
                }
            }
            Err(e) => {
                println!("   ❌ 获取合约代码失败: {}", e);
                continue;
            }
        }
        
        // 步骤2: 测试 token0() 方法
        println!("2️⃣ 测试 token0() 方法...");
        match test_method_call(&v3_client, pool_address, "token0()").await {
            Ok(result) => {
                println!("   ✅ token0() 成功");
                if result.len() >= 20 {
                    let token0_addr = Address::from_slice(&result[0..20]);
                    println!("   📍 Token0 地址: {:?}", token0_addr);
                }
            }
            Err(e) => {
                println!("   ❌ token0() 失败: {}", e);
                continue;
            }
        }
        
        // 步骤3: 测试 token1() 方法
        println!("3️⃣ 测试 token1() 方法...");
        match test_method_call(&v3_client, pool_address, "token1()").await {
            Ok(result) => {
                println!("   ✅ token1() 成功");
                if result.len() >= 20 {
                    let token1_addr = Address::from_slice(&result[0..20]);
                    println!("   📍 Token1 地址: {:?}", token1_addr);
                }
            }
            Err(e) => {
                println!("   ❌ token1() 失败: {}", e);
                continue;
            }
        }
        
        // 步骤4: 测试 fee() 方法
        println!("4️⃣ 测试 fee() 方法...");
        match test_method_call(&v3_client, pool_address, "fee()").await {
            Ok(result) => {
                println!("   ✅ fee() 成功");
                if result.len() >= 4 {
                    let fee = u32::from_be_bytes([result[0], result[1], result[2], result[3]]);
                    println!("   💰 手续费: {} ({}%)", fee, fee as f64 / 10000.0);
                }
            }
            Err(e) => {
                println!("   ❌ fee() 失败: {}", e);
                continue;
            }
        }
        
        // 步骤5: 测试 slot0() 方法
        println!("5️⃣ 测试 slot0() 方法...");
        match test_method_call(&v3_client, pool_address, "slot0()").await {
            Ok(result) => {
                println!("   ✅ slot0() 成功");
                println!("   📊 返回数据长度: {} bytes", result.len());
                if result.len() >= 32 {
                    let sqrt_price_bytes = &result[0..32];
                    println!("   💹 SqrtPriceX96 数据: {:?}", sqrt_price_bytes);
                }
            }
            Err(e) => {
                println!("   ❌ slot0() 失败: {}", e);
                continue;
            }
        }
        
        // 步骤6: 尝试简化的池信息获取
        println!("6️⃣ 测试简化池信息获取...");
        match get_basic_pool_info(&v3_client, pool_address).await {
            Ok(info) => {
                println!("   ✅ 简化池信息获取成功");
                println!("   📋 池地址: {}", info.0);
                println!("   🪙 Token0: {}", info.1);
                println!("   🪙 Token1: {}", info.2);
                println!("   💰 手续费: {}", info.3);
            }
            Err(e) => {
                println!("   ❌ 简化池信息获取失败: {}", e);
            }
        }
    }
    
    println!("\n🏁 诊断完成");
    Ok(())
}

async fn test_method_call(
    v3_client: &UniswapV3Client,
    pool_address: Address,
    method_sig: &str,
) -> Result<Bytes> {
    // 使用公共测试方法
    v3_client.test_call_pool_method(pool_address, method_sig).await
}

async fn get_basic_pool_info(
    v3_client: &UniswapV3Client,
    pool_address: Address,
) -> Result<(String, String, String, u32)> {
    // 获取token0
    let token0_result = v3_client.test_call_pool_method(pool_address, "token0()").await?;
    let token0_addr = if token0_result.len() >= 20 {
        Address::from_slice(&token0_result[0..20])
    } else {
        return Err(anyhow::anyhow!("无效的token0数据"));
    };
    
    // 获取token1
    let token1_result = v3_client.test_call_pool_method(pool_address, "token1()").await?;
    let token1_addr = if token1_result.len() >= 20 {
        Address::from_slice(&token1_result[0..20])
    } else {
        return Err(anyhow::anyhow!("无效的token1数据"));
    };
    
    // 获取fee
    let fee_result = v3_client.test_call_pool_method(pool_address, "fee()").await?;
    let fee = if fee_result.len() >= 4 {
        u32::from_be_bytes([fee_result[0], fee_result[1], fee_result[2], fee_result[3]])
    } else {
        return Err(anyhow::anyhow!("无效的fee数据"));
    };
    
    Ok((
        format!("{:?}", pool_address),
        format!("{:?}", token0_addr),
        format!("{:?}", token1_addr),
        fee,
    ))
}