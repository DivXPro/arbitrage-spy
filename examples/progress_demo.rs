use arbitrage_spy::core::exchange_graph::{ExchangeGraph, ExchangeEdge};
use bigdecimal::BigDecimal;
use std::str::FromStr;

fn main() {
    println!("🚀 套利路径搜索进度演示");
    println!("{}", "=".repeat(50));

    // 创建更多的测试交易对
    let test_pairs = vec![
        // USDT相关
        ("USDT", "ETH", "uniswap", "0.0004878", "5000000"),
        ("ETH", "USDT", "sushiswap", "2050.0", "5000000"),
        
        // ETH相关
        ("ETH", "USDC", "sushiswap", "2050.0", "2000000"),
        ("USDC", "ETH", "uniswap", "0.0004878", "2000000"),
        
        // USDC相关
        ("USDC", "USDT", "curve", "1.002", "10000000"),
        ("USDT", "USDC", "curve", "0.998", "10000000"),
        
        // BTC相关
        ("USDT", "WBTC", "binance", "0.000015", "3000000"),
        ("WBTC", "USDT", "binance", "66500.0", "3000000"),
        ("ETH", "WBTC", "uniswap", "0.031", "1500000"),
        ("WBTC", "ETH", "uniswap", "32.2", "1500000"),
        
        // DAI相关
        ("USDT", "DAI", "compound", "1.001", "8000000"),
        ("DAI", "USDT", "compound", "0.999", "8000000"),
        ("USDC", "DAI", "aave", "1.0005", "6000000"),
        ("DAI", "USDC", "aave", "0.9995", "6000000"),
        
        // LINK相关
        ("USDT", "LINK", "chainlink", "0.065", "1000000"),
        ("LINK", "USDT", "chainlink", "15.4", "1000000"),
        ("ETH", "LINK", "uniswap", "0.133", "800000"),
        ("LINK", "ETH", "uniswap", "7.52", "800000"),
        
        // UNI相关
        ("USDT", "UNI", "uniswap", "0.125", "500000"),
        ("UNI", "USDT", "uniswap", "8.0", "500000"),
        ("ETH", "UNI", "uniswap", "0.256", "400000"),
        ("UNI", "ETH", "uniswap", "3.9", "400000"),
    ];

    // 构建交换图
    let mut graph = ExchangeGraph::new();
    
    println!("📊 构建交换图...");
    for (from, to, exchange, rate, liquidity) in test_pairs {
        let edge = ExchangeEdge {
            pair_id: format!("{}-{}", from, to),
            from_token: from.to_string(),
            to_token: to.to_string(),
            dex: exchange.to_string(),
            exchange_rate: BigDecimal::from_str(rate).unwrap(),
            liquidity: BigDecimal::from_str(liquidity).unwrap(),
            gas_cost: BigDecimal::from_str("0.008").unwrap(),
            slippage: 0.001,
            fee_percentage: 0.003,
        };
        graph.add_edge(edge);
    }

    let (token_count, edge_count) = graph.get_stats();
    println!("✅ 图构建完成: {} 个代币节点, {} 条交易边", token_count, edge_count);
    println!();

    // 寻找套利路径 - 使用较低的盈利阈值以找到更多路径
    println!("🔍 开始寻找套利路径...");
    let arbitrage_paths = graph.find_arbitrage_paths("USDT", 4, 0.0001); // 0.01% 盈利阈值

    println!();
    println!("📈 搜索结果汇总:");
    println!("总共找到 {} 条套利路径", arbitrage_paths.len());
    
    if !arbitrage_paths.is_empty() {
        println!();
        println!("🏆 前5条最佳套利路径:");
        for (i, path) in arbitrage_paths.iter().take(5).enumerate() {
            println!("{}. {} (盈利率: {:.4}%)", 
                     i + 1, 
                     path.format_path_chain(), 
                     path.profit_rate * 100.0);
        }
        
        if arbitrage_paths.len() > 5 {
            println!("... 还有 {} 条其他路径", arbitrage_paths.len() - 5);
        }
    }

    println!();
    println!("🎉 进度演示完成!");
}