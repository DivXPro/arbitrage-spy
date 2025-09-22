use arbitrage_spy::core::exchange_graph::ExchangeEdge;
use arbitrage_spy::data::pair_manager::{PairData, TokenInfo};
use arbitrage_spy::config::protocol_types;
use bigdecimal::BigDecimal;
use std::str::FromStr;

fn main() {
    println!("ExchangeEdge 使用示例");
    println!("===================");

    // 创建示例交易对数据
    let pair_data = create_sample_pair();
    
    // 示例1: 使用 from_pair_data 创建单个边
    println!("\n1. 创建单个交易边:");
    let exchange_rate = BigDecimal::from_str("2000.0").unwrap();
    match ExchangeEdge::from_pair_data(
        &pair_data,
        "WETH".to_string(),
        "USDC".to_string(),
        exchange_rate,
    ) {
        Ok(edge) => {
            println!("✓ 成功创建交易边:");
            println!("  {}", edge.display_info());
            println!("  有效汇率: {}", edge.effective_exchange_rate());
        }
        Err(e) => println!("✗ 创建失败: {}", e),
    }

    // 示例2: 使用 create_bidirectional_edges 创建双向边
    println!("\n2. 创建双向交易边:");
    match ExchangeEdge::create_bidirectional_edges(&pair_data) {
        Ok((edge1, edge2)) => {
            println!("✓ 成功创建双向边:");
            println!("  边1: {}", edge1.display_info());
            println!("  边2: {}", edge2.display_info());
            
            // 验证汇率互为倒数
            let product = &edge1.exchange_rate * &edge2.exchange_rate;
            println!("  汇率乘积 (应接近1): {}", product);
        }
        Err(e) => println!("✗ 创建失败: {}", e),
    }

    // 示例3: 批量处理多个交易对
    println!("\n3. 批量处理交易对:");
    let pairs = vec![
        create_sample_pair(),
        create_another_sample_pair(),
    ];
    
    match ExchangeEdge::from_pair_data_batch(&pairs) {
        Ok(edges) => {
            println!("✓ 成功创建 {} 条边:", edges.len());
            for (i, edge) in edges.iter().enumerate() {
                println!("  边{}: {} -> {} (汇率: {})", 
                    i + 1, 
                    edge.from_token, 
                    edge.to_token, 
                    edge.exchange_rate
                );
            }
        }
        Err(e) => println!("✗ 批量处理失败: {}", e),
    }

    // 示例4: 验证边的有效性
    println!("\n4. 验证边的有效性:");
    let exchange_rate = BigDecimal::from_str("1500.0").unwrap();
    match ExchangeEdge::from_pair_data(
        &pair_data,
        "WETH".to_string(),
        "USDC".to_string(),
        exchange_rate,
    ) {
        Ok(edge) => {
            match edge.validate() {
                Ok(_) => println!("✓ 边验证通过"),
                Err(e) => println!("✗ 边验证失败: {}", e),
            }
        }
        Err(e) => println!("✗ 创建边失败: {}", e),
    }
}

fn create_sample_pair() -> PairData {
    PairData {
        id: "sample_pair_1".to_string(),
        network: "ethereum".to_string(),
        dex: "uniswap_v2".to_string(),
        protocol_type: protocol_types::AMM_V2.to_string(),
        token0: TokenInfo {
            id: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
            symbol: "WETH".to_string(),
            name: "Wrapped Ether".to_string(),
            decimals: "18".to_string(),
        },
        token1: TokenInfo {
            id: "0xA0b86a33E6441b8C4505B8C4505B8C4505B8C4505".to_string(),
            symbol: "USDC".to_string(),
            name: "USD Coin".to_string(),
            decimals: "6".to_string(),
        },
        volume_usd: "5000000".to_string(),
        reserve_usd: "10000000".to_string(),
        tx_count: "2500".to_string(),
        reserve0: "2500000000000000000000".to_string(), // 2500 WETH
        reserve1: "5000000000000".to_string(), // 5,000,000 USDC
        fee_tier: "3000".to_string(),
        sqrt_price: None,
        tick: None,
    }
}

fn create_another_sample_pair() -> PairData {
    PairData {
        id: "sample_pair_2".to_string(),
        network: "ethereum".to_string(),
        dex: "uniswap_v3".to_string(),
        protocol_type: protocol_types::AMM_V3.to_string(),
        token0: TokenInfo {
            id: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
            symbol: "WETH".to_string(),
            name: "Wrapped Ether".to_string(),
            decimals: "18".to_string(),
        },
        token1: TokenInfo {
            id: "0x6B175474E89094C44Da98b954EedeAC495271d0F".to_string(),
            symbol: "DAI".to_string(),
            name: "Dai Stablecoin".to_string(),
            decimals: "18".to_string(),
        },
        volume_usd: "3000000".to_string(),
        reserve_usd: "8000000".to_string(),
        tx_count: "1800".to_string(),
        reserve0: "2000000000000000000000".to_string(), // 2000 WETH
        reserve1: "4000000000000000000000000".to_string(), // 4,000,000 DAI
        fee_tier: "3000".to_string(),
        sqrt_price: Some("1771845812700903892492222464".to_string()),
        tick: Some("201077".to_string()),
    }
}