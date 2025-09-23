use arbitrage_spy::store::{
    blockchain_client::{BlockchainClient, NetworkConfig},
    uniswap_v3_client::UniswapV3Client,
};
use ethers::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Uniswap V3 方法级诊断工具");
    println!("============================");
    
    // 初始化区块链客户端
    let config = NetworkConfig::ethereum_mainnet();
    let blockchain_client = Arc::new(BlockchainClient::new(config).await?);
    println!("✅ 区块链客户端初始化成功");
    
    // 使用一个已知的有效池地址
    let pool_address = "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640".parse::<Address>()?; // USDC/WETH 0.05%
    println!("📍 测试池地址: {:?}", pool_address);
    
    // 测试每个方法调用
    let methods = vec![
        ("token0()", "0dfe1681"),
        ("token1()", "d21c20ee"),
        ("fee()", "ddca3f43"),
        ("slot0()", "3850c7bd"),
        ("liquidity()", "128acb08"),
        ("feeGrowthGlobal0X128()", "f3058b14"),
        ("feeGrowthGlobal1X128()", "46141f19"),
    ];
    
    for (method_name, method_sig) in methods.iter() {
        println!("\n--- 测试方法: {} ---", method_name);
        
        let call_data = ethers::utils::hex::decode(method_sig)?;
        match blockchain_client.call_contract(pool_address, call_data.into()).await {
            Ok(result) => {
                println!("✅ {} 调用成功", method_name);
                println!("   返回数据长度: {} 字节", result.len());
                println!("   返回数据 (hex): {:?}", result);
                
                // 根据方法类型解析数据
                match *method_name {
                    "token0()" | "token1()" => {
                        if result.len() >= 32 {
                            let token_bytes = &result[12..32];
                            let token_address = Address::from_slice(token_bytes);
                            println!("   解析地址: {:?}", token_address);
                        }
                    },
                    "fee()" => {
                        if result.len() >= 32 {
                            let fee_bytes = &result[29..32];
                            let fee = u32::from_be_bytes([0, fee_bytes[0], fee_bytes[1], fee_bytes[2]]);
                            println!("   解析手续费: {}", fee);
                        }
                    },
                    "slot0()" => {
                        if result.len() >= 32 {
                            println!("   slot0 数据可用，长度正确");
                            // 尝试解析前32字节作为sqrtPriceX96
                            let sqrt_price_bytes = &result[0..32];
                            println!("   sqrtPriceX96 bytes: {:?}", sqrt_price_bytes);
                        }
                    },
                    "liquidity()" => {
                        if result.len() >= 32 {
                            println!("   流动性数据可用");
                        }
                    },
                    _ => {
                        println!("   原始数据返回");
                    }
                }
            },
            Err(e) => {
                println!("❌ {} 调用失败: {}", method_name, e);
                
                // 如果这个方法失败了，这可能就是导致get_pool_info失败的原因
                if method_name == &"slot0()" {
                    println!("⚠️  slot0() 方法失败可能是主要原因！");
                }
            }
        }
    }
    
    println!("\n🔍 现在测试 get_token_info 相关方法...");
    
    // 首先获取token0地址
    let token0_call_data = ethers::utils::hex::decode("0dfe1681")?; // token0()
    if let Ok(result) = blockchain_client.call_contract(pool_address, token0_call_data.into()).await {
        if result.len() >= 32 {
            let token0_bytes = &result[12..32];
            let token0_address = Address::from_slice(token0_bytes);
            println!("Token0 地址: {:?}", token0_address);
            
            // 测试token symbol调用
            println!("\n--- 测试 Token0 symbol() ---");
            let symbol_call_data = ethers::utils::hex::decode("95d89b41")?; // symbol()
            match blockchain_client.call_contract(token0_address, symbol_call_data.into()).await {
                Ok(symbol_result) => {
                    println!("✅ Token0 symbol() 调用成功");
                    println!("   返回数据长度: {} 字节", symbol_result.len());
                    println!("   返回数据 (hex): {:?}", symbol_result);
                },
                Err(e) => {
                    println!("❌ Token0 symbol() 调用失败: {}", e);
                    println!("⚠️  这可能是导致 get_pool_info 失败的原因！");
                }
            }
            
            // 测试token decimals调用
            println!("\n--- 测试 Token0 decimals() ---");
            let decimals_call_data = ethers::utils::hex::decode("313ce567")?; // decimals()
            match blockchain_client.call_contract(token0_address, decimals_call_data.into()).await {
                Ok(decimals_result) => {
                    println!("✅ Token0 decimals() 调用成功");
                    println!("   返回数据长度: {} 字节", decimals_result.len());
                    println!("   返回数据 (hex): {:?}", decimals_result);
                },
                Err(e) => {
                    println!("❌ Token0 decimals() 调用失败: {}", e);
                    println!("⚠️  这可能是导致 get_pool_info 失败的原因！");
                }
            }
        }
    }
    
    println!("\n🎯 诊断完成！");
    println!("如果某个方法调用失败，那就是导致 get_pool_info 失败的根本原因。");
    
    Ok(())
}