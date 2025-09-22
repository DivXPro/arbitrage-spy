use arbitrage_spy::core::exchange_graph::{ExchangeGraph, ExchangeEdge};
use bigdecimal::BigDecimal;
use std::str::FromStr;

fn main() {
    println!("🚀 大规模套利路径搜索进度演示");
    println!("{}", "=".repeat(60));

    // 创建大量的测试交易对，模拟真实的DeFi环境
    let test_pairs = vec![
        // 主要稳定币对
        ("USDT", "USDC", "curve", "1.001", "50000000"),
        ("USDC", "USDT", "curve", "0.999", "50000000"),
        ("USDT", "DAI", "compound", "1.0005", "30000000"),
        ("DAI", "USDT", "compound", "0.9995", "30000000"),
        ("USDC", "DAI", "aave", "1.0002", "25000000"),
        ("DAI", "USDC", "aave", "0.9998", "25000000"),
        
        // ETH相关对
        ("USDT", "ETH", "uniswap", "0.0005", "20000000"),
        ("ETH", "USDT", "uniswap", "2000.0", "20000000"),
        ("USDC", "ETH", "sushiswap", "0.00049", "18000000"),
        ("ETH", "USDC", "sushiswap", "2040.0", "18000000"),
        ("DAI", "ETH", "balancer", "0.000495", "15000000"),
        ("ETH", "DAI", "balancer", "2020.0", "15000000"),
        
        // BTC相关对
        ("USDT", "WBTC", "binance", "0.000015", "10000000"),
        ("WBTC", "USDT", "binance", "66000.0", "10000000"),
        ("USDC", "WBTC", "coinbase", "0.0000148", "8000000"),
        ("WBTC", "USDC", "coinbase", "67500.0", "8000000"),
        ("ETH", "WBTC", "uniswap", "0.031", "5000000"),
        ("WBTC", "ETH", "uniswap", "32.0", "5000000"),
        
        // DeFi代币
        ("USDT", "LINK", "chainlink", "0.065", "3000000"),
        ("LINK", "USDT", "chainlink", "15.3", "3000000"),
        ("ETH", "LINK", "uniswap", "0.13", "2500000"),
        ("LINK", "ETH", "uniswap", "7.6", "2500000"),
        ("USDC", "LINK", "sushiswap", "0.0648", "2000000"),
        ("LINK", "USDC", "sushiswap", "15.4", "2000000"),
        
        ("USDT", "UNI", "uniswap", "0.124", "2000000"),
        ("UNI", "USDT", "uniswap", "8.05", "2000000"),
        ("ETH", "UNI", "uniswap", "0.248", "1800000"),
        ("UNI", "ETH", "uniswap", "4.02", "1800000"),
        ("USDC", "UNI", "sushiswap", "0.123", "1500000"),
        ("UNI", "USDC", "sushiswap", "8.1", "1500000"),
        
        ("USDT", "AAVE", "aave", "0.0105", "1500000"),
        ("AAVE", "USDT", "aave", "95.0", "1500000"),
        ("ETH", "AAVE", "uniswap", "0.021", "1200000"),
        ("AAVE", "ETH", "uniswap", "47.5", "1200000"),
        ("USDC", "AAVE", "sushiswap", "0.0104", "1000000"),
        ("AAVE", "USDC", "sushiswap", "96.0", "1000000"),
        
        // 更多小币种
        ("USDT", "COMP", "compound", "0.0185", "800000"),
        ("COMP", "USDT", "compound", "54.0", "800000"),
        ("ETH", "COMP", "uniswap", "0.037", "600000"),
        ("COMP", "ETH", "uniswap", "27.0", "600000"),
        
        ("USDT", "MKR", "maker", "0.00065", "500000"),
        ("MKR", "USDT", "maker", "1540.0", "500000"),
        ("ETH", "MKR", "uniswap", "0.0013", "400000"),
        ("MKR", "ETH", "uniswap", "770.0", "400000"),
        
        // 跨链代币
        ("USDT", "MATIC", "polygon", "0.9", "2000000"),
        ("MATIC", "USDT", "polygon", "1.11", "2000000"),
        ("ETH", "MATIC", "uniswap", "1.8", "1500000"),
        ("MATIC", "ETH", "uniswap", "0.555", "1500000"),
        
        ("USDT", "AVAX", "avalanche", "0.032", "1000000"),
        ("AVAX", "USDT", "avalanche", "31.2", "1000000"),
        ("ETH", "AVAX", "uniswap", "0.064", "800000"),
        ("AVAX", "ETH", "uniswap", "15.6", "800000"),
    ];

    // 构建交换图
    let mut graph = ExchangeGraph::new();
    
    println!("📊 构建大规模交换图...");
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
    println!("✅ 大规模图构建完成: {} 个代币节点, {} 条交易边", token_count, edge_count);
    println!();

    // 寻找套利路径 - 使用非常低的盈利阈值以找到大量路径
    println!("🔍 开始大规模套利路径搜索...");
    println!("⚙️  搜索参数: 起始代币=USDT, 最大深度=4, 盈利阈值=0.001%");
    println!();
    
    let start_time = std::time::Instant::now();
    let arbitrage_paths = graph.find_arbitrage_paths("USDT", 4, 0.00001); // 0.001% 盈利阈值
    let search_duration = start_time.elapsed();

    println!();
    println!("📊 大规模搜索结果汇总:");
    println!("总搜索时间: {:?}", search_duration);
    println!("总共找到 {} 条套利路径", arbitrage_paths.len());
    
    if !arbitrage_paths.is_empty() {
        println!();
        println!("🏆 前10条最佳套利路径:");
        for (i, path) in arbitrage_paths.iter().take(10).enumerate() {
            println!("{}. {} (盈利率: {:.4}%)", 
                     i + 1, 
                     path.format_path_chain(), 
                     path.profit_rate * 100.0);
        }
        
        if arbitrage_paths.len() > 10 {
            println!("... 还有 {} 条其他路径", arbitrage_paths.len() - 10);
        }
        
        // 统计不同类型的路径
        println!();
        println!("📈 路径类型分析:");
        let mut dex_count = std::collections::HashMap::new();
        for path in &arbitrage_paths {
            let dexes: std::collections::HashSet<String> = path.edges.iter()
                .map(|edge| edge.dex.clone())
                .collect();
            let dex_type = if dexes.len() == 1 {
                "单DEX套利"
            } else {
                "跨DEX套利"
            };
            *dex_count.entry(dex_type).or_insert(0) += 1;
        }
        
        for (dex_type, count) in dex_count {
            println!("   - {}: {} 条路径", dex_type, count);
        }
    }

    println!();
    println!("🎉 大规模进度演示完成!");
    println!("💡 进度显示功能说明:");
    println!("   - 前5条路径会逐一显示详细信息");
    println!("   - 之后每找到10条路径显示一次进度");
    println!("   - 搜索完成后显示总结信息");
}