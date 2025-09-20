use arbitrage_spy::arbitrage_chain::{PriceGraph, ArbitrageChainFinder, ArbitrageEdge};
use bigdecimal::{BigDecimal, FromPrimitive};
use std::str::FromStr;

/// 集成测试：基本的套利链发现功能
#[test]
fn test_complete_arbitrage_discovery_flow() {
    // 创建一个模拟的多DEX环境
    let mut graph = PriceGraph::new();
    
    // 添加一些基本的交易对
    let eth_usdc = ArbitrageEdge {
        from_token: "ETH".to_string(),
        to_token: "USDC".to_string(),
        dex: "uniswap_v2".to_string(),
        exchange_rate: BigDecimal::from_str("2000.0").unwrap(),
        liquidity: BigDecimal::from_str("1000000.0").unwrap(),
        gas_cost: BigDecimal::from_str("0.01").unwrap(),
        slippage: 0.003,
        fee_percentage: 0.003,
    };
    
    let usdc_eth = ArbitrageEdge {
        from_token: "USDC".to_string(),
        to_token: "ETH".to_string(),
        dex: "sushiswap".to_string(),
        exchange_rate: BigDecimal::from_str("0.0005").unwrap(),
        liquidity: BigDecimal::from_str("500000.0").unwrap(),
        gas_cost: BigDecimal::from_str("0.012").unwrap(),
        slippage: 0.005,
        fee_percentage: 0.0025,
    };
    
    graph.add_edge(eth_usdc);
    graph.add_edge(usdc_eth);
    
    // 创建套利链查找器
    let finder = ArbitrageChainFinder::new(
        3,    // max_hops
        0.1,  // min_profit_percentage (0.1%)
        0.1,  // max_slippage (10%)
        1000.0, // min_liquidity
        1.0,  // max_risk_score
    );
    
    // 测试基本功能
    let result = finder.find_arbitrage_chains(&graph, "ETH");
    assert!(result.is_ok(), "套利链查找应该成功");
    
    let chains = result.unwrap();
    println!("图统计: {:?}", graph.get_stats());
    println!("找到的套利链数量: {}", chains.len());
    
    // 验证图的基本结构
    let (token_count, edge_count) = graph.get_stats();
    assert_eq!(token_count, 2, "应该有2个代币");
    assert_eq!(edge_count, 2, "应该有2条边");
    
    // 验证查找器的配置
    assert_eq!(finder.max_hops(), 3);
    assert_eq!(finder.min_profit_percentage(), 0.1);
    
    println!("基本套利链发现功能测试通过");
}

/// 测试多代币环境下的复杂套利场景
#[test]
fn test_multi_token_arbitrage_scenarios() {
    let mut graph = PriceGraph::new();
    
    // 创建一个包含多个代币的复杂网络
    let tokens = vec!["ETH", "USDC", "USDT", "DAI", "WBTC"];
    
    // 添加各种代币对的交易路径
    for (i, from_token) in tokens.iter().enumerate() {
        for (j, to_token) in tokens.iter().enumerate() {
            if i != j {
                // 为每个代币对创建多个DEX的路径
                let dexes = vec!["uniswap_v2", "sushiswap", "curve"];
                
                for (k, dex) in dexes.iter().enumerate() {
                    let base_rate = if from_token == &"ETH" && to_token == &"USDC" {
                        2000.0
                    } else if from_token == &"USDC" && to_token == &"ETH" {
                        0.0005
                    } else if from_token == &"WBTC" && to_token == &"ETH" {
                        15.0
                    } else if from_token == &"ETH" && to_token == &"WBTC" {
                        0.066
                    } else {
                        1.0 + (k as f64 * 0.001) // 轻微的价格差异
                    };
                    
                    let edge = ArbitrageEdge {
                        from_token: from_token.to_string(),
                        to_token: to_token.to_string(),
                        dex: dex.to_string(),
                        exchange_rate: BigDecimal::from_f64(base_rate * (1.0 + k as f64 * 0.002)).unwrap(),
                        liquidity: BigDecimal::from_f64(100000.0 * (1.0 + k as f64)).unwrap(),
                        gas_cost: BigDecimal::from_f64(0.01 + k as f64 * 0.002).unwrap(),
                        slippage: 0.003 + k as f64 * 0.001,
                        fee_percentage: 0.003 + k as f64 * 0.0005,
                    };
                    
                    graph.add_edge(edge);
                }
            }
        }
    }
    
    // 使用优化版本的查找器
    let finder = ArbitrageChainFinder::new_optimized(
        4,    // max_hops
        0.3,  // min_profit_percentage
        0.02, // max_slippage
        5000.0, // min_liquidity
        0.9,  // max_risk_score
        10,   // max_chains_per_token
        0.001, // min_amount_threshold
    );
    
    // 测试从不同代币开始的套利发现
    for start_token in &tokens {
        let result = finder.find_arbitrage_chains(&graph, start_token);
        assert!(result.is_ok(), "从 {} 开始的套利链查找应该成功", start_token);
        
        let chains = result.unwrap();
        println!("从 {} 开始找到 {} 个套利机会", start_token, chains.len());
        
        // 验证所有链都是有效的
        for chain in &chains {
            assert_eq!(chain.start_token, *start_token);
            // 验证最后一跳回到起始代币
            if let Some(last_hop) = chain.hops.last() {
                assert_eq!(last_hop.edge.to_token, *start_token);
            }
            assert!(chain.profit_percentage >= 0.3);
        }
    }
}

/// 测试边界条件和错误处理
#[test]
fn test_edge_cases_and_error_handling() {
    let mut graph = PriceGraph::new();
    let finder = ArbitrageChainFinder::new(3, 1.0, 0.05, 1000.0, 0.8);
    
    // 测试空图
    let result = finder.find_arbitrage_chains(&graph, "ETH");
    assert!(result.is_err(), "空图中查找不存在的代币应该返回错误");
    
    // 测试不存在的代币
    let edge = ArbitrageEdge {
        from_token: "TOKEN_A".to_string(),
        to_token: "TOKEN_B".to_string(),
        dex: "test_dex".to_string(),
        exchange_rate: BigDecimal::from_str("1.0").unwrap(),
        liquidity: BigDecimal::from_str("1000.0").unwrap(),
        gas_cost: BigDecimal::from_str("0.01").unwrap(),
        slippage: 0.01,
        fee_percentage: 0.003,
    };
    graph.add_edge(edge);
    
    let result = finder.find_arbitrage_chains(&graph, "NONEXISTENT_TOKEN");
    assert!(result.is_err(), "不存在的代币应该返回错误");
    
    // 测试高滑点场景
    let high_slippage_edge = ArbitrageEdge {
        from_token: "ETH".to_string(),
        to_token: "USDC".to_string(),
        dex: "high_slippage_dex".to_string(),
        exchange_rate: BigDecimal::from_str("2000.0").unwrap(),
        liquidity: BigDecimal::from_str("100.0").unwrap(), // 低流动性导致高滑点
        gas_cost: BigDecimal::from_str("0.01").unwrap(),
        slippage: 0.15, // 15% 滑点
        fee_percentage: 0.003,
    };
    graph.add_edge(high_slippage_edge);
    
    let strict_finder = ArbitrageChainFinder::new(3, 1.0, 0.05, 1000.0, 0.8);
    let result = strict_finder.find_arbitrage_chains(&graph, "ETH");
    assert!(result.is_ok());
    
    // 高滑点的边应该被过滤掉
    let chains = result.unwrap();
    for chain in &chains {
        for hop in &chain.hops {
            assert!(hop.edge.slippage <= 0.05, "所有边的滑点都应该在限制范围内");
        }
    }
}

/// 性能基准测试
#[test]
fn test_performance_benchmark() {
    use std::time::Instant;
    
    let mut graph = PriceGraph::new();
    
    // 创建一个大型图进行性能测试
    let tokens: Vec<String> = (0..20).map(|i| format!("TOKEN_{}", i)).collect();
    let dexes = vec!["uniswap_v2", "sushiswap", "curve", "balancer", "pancakeswap"];
    
    // 添加大量边
    for from_token in &tokens {
        for to_token in &tokens {
            if from_token != to_token {
                for dex in &dexes {
                    let edge = ArbitrageEdge {
                        from_token: from_token.clone(),
                        to_token: to_token.clone(),
                        dex: dex.to_string(),
                        exchange_rate: BigDecimal::from_f64(1.0 + rand::random::<f64>() * 0.1).unwrap(),
                        liquidity: BigDecimal::from_f64(10000.0 + rand::random::<f64>() * 90000.0).unwrap(),
                        gas_cost: BigDecimal::from_f64(0.005 + rand::random::<f64>() * 0.01).unwrap(),
                        slippage: 0.001 + rand::random::<f64>() * 0.01,
                        fee_percentage: 0.001 + rand::random::<f64>() * 0.005,
                    };
                    graph.add_edge(edge);
                }
            }
        }
    }
    
    let (token_count, edge_count) = graph.get_stats();
    println!("创建了包含 {} 个代币和 {} 条边的测试图", token_count, edge_count);
    
    // 测试标准版本的性能
    let standard_finder = ArbitrageChainFinder::new(3, 0.5, 0.02, 1000.0, 0.8);
    let start = Instant::now();
    let result = standard_finder.find_arbitrage_chains(&graph, "TOKEN_0");
    let standard_duration = start.elapsed();
    
    assert!(result.is_ok());
    println!("标准版本搜索耗时: {:?}", standard_duration);
    
    // 测试优化版本的性能
    let optimized_finder = ArbitrageChainFinder::new_optimized(
        3, 0.5, 0.02, 1000.0, 0.8, 10, 0.001
    );
    let start = Instant::now();
    let result = optimized_finder.find_arbitrage_chains(&graph, "TOKEN_0");
    let optimized_duration = start.elapsed();
    
    assert!(result.is_ok());
    println!("优化版本搜索耗时: {:?}", optimized_duration);
    
    // 优化版本应该更快或至少不慢太多
    assert!(optimized_duration <= standard_duration * 2, 
           "优化版本不应该比标准版本慢太多");
    
    // 确保在合理时间内完成
    assert!(optimized_duration.as_secs() < 5, 
           "搜索时间应该在5秒内完成");
}