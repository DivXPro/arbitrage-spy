use arbitrage_spy::core::exchange_graph::ExchangeGraph;
use arbitrage_spy::core::exchange_edge::ExchangeEdge;
use arbitrage_spy::event_listener::EventListener;
use bigdecimal::BigDecimal;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

/// 创建模拟交换边的辅助函数
fn create_mock_edge(
    from: &str,
    to: &str,
    rate: &str,
    liquidity: &str,
    dex: &str,
    pair_id: &str,
) -> ExchangeEdge {
    ExchangeEdge {
        from_token: from.to_string(),
        to_token: to.to_string(),
        exchange_rate: BigDecimal::from_str(rate).unwrap(),
        liquidity: BigDecimal::from_str(liquidity).unwrap(),
        dex: dex.to_string(),
        pair_id: pair_id.to_string(),
        gas_cost: BigDecimal::from_str("0").unwrap(), // Mock gas cost
        slippage: 0.001,                               // Mock slippage
        fee_percentage: 0.003,                         // Mock fee
    }
}

/// 测试多币种套利路径检测（8个币种）
#[tokio::test]
async fn test_triangular_arbitrage_detection() {
    // 1. Setup: Create a graph with multiple arbitrage opportunities across 8 tokens
    let event_listener = Arc::new(Mutex::new(EventListener::new_without_connection(10)));
    let mut graph = ExchangeGraph::new(event_listener, None).unwrap();

    // Define 8 tokens
    let weth = "WETH".to_string();
    let usdc = "USDC".to_string();
    let dai = "DAI".to_string();
    let usdt = "USDT".to_string();
    let wbtc = "WBTC".to_string();
    let uni = "UNI".to_string();
    let link = "LINK".to_string();
    let aave = "AAVE".to_string();

    // === 主要三角套利路径: WETH -> USDC -> DAI -> WETH ===
    // WETH -> USDC (Rate: 1 WETH = 3000 USDC)
    graph.add_edge(create_mock_edge(&weth, &usdc, "3000", "2000000", "Uniswap_V2", "weth_usdc"));
    // USDC -> DAI (Rate: 1 USDC = 1.01 DAI)
    graph.add_edge(create_mock_edge(&usdc, &dai, "1.01", "1500000", "Uniswap_V2", "usdc_dai"));
    // DAI -> WETH (Rate: 1 DAI = 0.000345 WETH) - 盈利路径
    graph.add_edge(create_mock_edge(&dai, &weth, "0.000345", "1000000", "Uniswap_V2", "dai_weth"));

    // === 稳定币套利路径: USDC -> USDT -> DAI -> USDC ===
    // USDC -> USDT (Rate: 1 USDC = 1.002 USDT)
    graph.add_edge(create_mock_edge(&usdc, &usdt, "1.002", "3000000", "Sushiswap", "usdc_usdt"));
    // USDT -> DAI (Rate: 1 USDT = 1.001 DAI)
    graph.add_edge(create_mock_edge(&usdt, &dai, "1.001", "2500000", "Curve", "usdt_dai"));
    // DAI -> USDC (Rate: 1 DAI = 0.995 USDC) - 盈利路径
    graph.add_edge(create_mock_edge(&dai, &usdc, "0.995", "2000000", "Uniswap_V3", "dai_usdc"));

    // === 比特币路径: WETH -> WBTC -> USDC -> WETH ===
    // WETH -> WBTC (Rate: 1 WETH = 0.055 WBTC)
    graph.add_edge(create_mock_edge(&weth, &wbtc, "0.055", "500000", "Uniswap_V2", "weth_wbtc"));
    // WBTC -> USDC (Rate: 1 WBTC = 55000 USDC)
    graph.add_edge(create_mock_edge(&wbtc, &usdc, "55000", "800000", "Sushiswap", "wbtc_usdc"));
    // USDC -> WETH (Rate: 1 USDC = 0.000335 WETH) - 盈利路径
    graph.add_edge(create_mock_edge(&usdc, &weth, "0.000335", "1800000", "Uniswap_V3", "usdc_weth"));

    // === 治理代币路径: WETH -> UNI -> USDC -> WETH ===
    // WETH -> UNI (Rate: 1 WETH = 400 UNI)
    graph.add_edge(create_mock_edge(&weth, &uni, "400", "600000", "Uniswap_V2", "weth_uni"));
    // UNI -> USDC (Rate: 1 UNI = 7.6 USDC)
    graph.add_edge(create_mock_edge(&uni, &usdc, "7.6", "700000", "Sushiswap", "uni_usdc"));

    // === DeFi代币路径: USDC -> LINK -> AAVE -> USDC ===
    // USDC -> LINK (Rate: 1 USDC = 0.065 LINK)
    graph.add_edge(create_mock_edge(&usdc, &link, "0.065", "900000", "Uniswap_V2", "usdc_link"));
    // LINK -> AAVE (Rate: 1 LINK = 0.18 AAVE)
    graph.add_edge(create_mock_edge(&link, &aave, "0.18", "400000", "Sushiswap", "link_aave"));
    // AAVE -> USDC (Rate: 1 AAVE = 87 USDC) - 盈利路径
    graph.add_edge(create_mock_edge(&aave, &usdc, "87", "350000", "Uniswap_V3", "aave_usdc"));

    // === 额外的交叉连接（增加网络复杂性）===
    // WBTC -> WETH
    graph.add_edge(create_mock_edge(&wbtc, &weth, "18.2", "400000", "Uniswap_V2", "wbtc_weth"));
    // USDT -> USDC
    graph.add_edge(create_mock_edge(&usdt, &usdc, "0.998", "2800000", "Curve", "usdt_usdc"));
    // UNI -> WETH
    graph.add_edge(create_mock_edge(&uni, &weth, "0.0025", "500000", "Uniswap_V2", "uni_weth"));
    // LINK -> USDC
    graph.add_edge(create_mock_edge(&link, &usdc, "15.4", "600000", "Uniswap_V3", "link_usdc"));
    // AAVE -> WETH
    graph.add_edge(create_mock_edge(&aave, &weth, "0.029", "300000", "Sushiswap", "aave_weth"));
    // DAI -> USDT
    graph.add_edge(create_mock_edge(&dai, &usdt, "1.003", "1200000", "Curve", "dai_usdt"));

    // === 反向边（非盈利，增加真实性）===
    graph.add_edge(create_mock_edge(&usdc, &weth, "0.00033", "1500000", "Uniswap_V2", "usdc_weth_rev"));
    graph.add_edge(create_mock_edge(&dai, &usdc, "0.99", "1800000", "Uniswap_V2", "dai_usdc_rev"));
    graph.add_edge(create_mock_edge(&weth, &dai, "2900", "1000000", "Uniswap_V2", "weth_dai_rev"));
    graph.add_edge(create_mock_edge(&usdt, &usdc, "0.999", "2500000", "Sushiswap", "usdt_usdc_rev"));
    graph.add_edge(create_mock_edge(&wbtc, &weth, "18.1", "400000", "Sushiswap", "wbtc_weth_rev"));

    // 2. Act: Find arbitrage paths starting from WETH with increased depth
    let paths = graph.find_arbitrage_paths(&weth, 4, 0.01);

    // 3. Assert: Check if multiple profitable paths were found
    assert!(paths.len() >= 2, "Should find at least 2 profitable paths with 8 tokens, found {}", paths.len());

    // Verify the main triangular arbitrage path exists
    let main_path = paths.iter().find(|p| 
        p.format_path_chain().contains("WETH -> USDC") && 
        p.format_path_chain().contains("DAI") &&
        p.format_path_chain().ends_with("WETH(Uniswap_V2)")
    );
    assert!(main_path.is_some(), "Should find the main WETH->USDC->DAI->WETH path");

    // Verify profit calculations for the best path
    let best_path = &paths[0];
    let expected_min_profit_rate = 0.015; // 1.5%
    assert!(
        best_path.net_profit_rate > expected_min_profit_rate,
        "Expected net profit rate to be over 1.5%, but got {}",
        best_path.net_profit_rate
    );

    // Test different starting tokens to ensure network connectivity
    let usdc_paths = graph.find_arbitrage_paths(&usdc, 4, 0.01);
    assert!(usdc_paths.len() >= 1, "Should find profitable paths starting from USDC");

    let dai_paths = graph.find_arbitrage_paths(&dai, 4, 0.01);
    assert!(dai_paths.len() >= 1, "Should find profitable paths starting from DAI");

    println!("Found {} arbitrage paths starting from WETH", paths.len());
    println!("Found {} arbitrage paths starting from USDC", usdc_paths.len());
    println!("Found {} arbitrage paths starting from DAI", dai_paths.len());
    
    // Print the top 3 paths for verification
    for (i, path) in paths.iter().take(3).enumerate() {
        println!("Path {}: {} (Net profit rate: {:.4}%)", 
                i + 1, 
                path.format_path_chain(), 
                path.net_profit_rate * 100.0);
    }
}

/// 测试无套利机会的场景
#[tokio::test]
async fn test_no_arbitrage_opportunity() {
    // Test case where no profitable arbitrage exists
    let event_listener = Arc::new(Mutex::new(EventListener::new_without_connection(10)));
    let mut graph = ExchangeGraph::new(event_listener, None).unwrap();

    // Create edges that don't form a profitable cycle
    graph.add_edge(create_mock_edge(
        "WETH",
        "USDC",
        "3000",
        "1000000",
        "Uniswap_V2",
        "pair1",
    ));
    graph.add_edge(create_mock_edge(
        "USDC",
        "DAI",
        "1.0",
        "1000000",
        "Uniswap_V2",
        "pair2",
    ));
    // This rate makes the cycle unprofitable (3000 * 1.0 * 0.0003 = 0.9)
    graph.add_edge(create_mock_edge(
        "DAI",
        "WETH",
        "0.0003",
        "1000000",
        "Uniswap_V2",
        "pair3",
    ));

    let paths = graph.find_arbitrage_paths("WETH", 3, 0.01);
    assert_eq!(paths.len(), 0, "Should find no profitable paths");
}

/// 测试搜索深度限制对套利路径发现的影响
#[tokio::test]
async fn test_search_depth_limitation() {
    // Test case where arbitrage exists but search depth is too shallow
    let event_listener = Arc::new(Mutex::new(EventListener::new_without_connection(10)));
    let mut graph = ExchangeGraph::new(event_listener, None).unwrap();

    // Create a 4-step arbitrage cycle
    graph.add_edge(create_mock_edge("A", "B", "2.0", "1000000", "DEX1", "pair1"));
    graph.add_edge(create_mock_edge("B", "C", "2.0", "1000000", "DEX2", "pair2"));
    graph.add_edge(create_mock_edge("C", "D", "2.0", "1000000", "DEX3", "pair3"));
    graph.add_edge(create_mock_edge("D", "A", "0.2", "1000000", "DEX4", "pair4")); // 2*2*2*0.2 = 3.2 > 1

    // Search with depth 3 should find no paths
    let paths_shallow = graph.find_arbitrage_paths("A", 3, 0.01);
    assert_eq!(paths_shallow.len(), 0, "Should find no paths with insufficient depth");

    // Search with depth 4 should find the path
    let paths_deep = graph.find_arbitrage_paths("A", 4, 0.01);
    assert!(paths_deep.len() > 0, "Should find paths with sufficient depth");
}

/// 测试多DEX环境下的套利路径检测
#[tokio::test]
async fn test_multi_dex_arbitrage() {
    let event_listener = Arc::new(Mutex::new(EventListener::new_without_connection(10)));
    let mut graph = ExchangeGraph::new(event_listener, None).unwrap();

    // Create a simple 2-step arbitrage opportunity across different DEXs
    // WETH -> USDC on Uniswap V2
    graph.add_edge(create_mock_edge(
        "WETH",
        "USDC",
        "3000",
        "1000000",
        "Uniswap_V2",
        "uni_v2_pair1",
    ));
    
    // USDC -> WETH on Sushiswap (better rate, creating arbitrage opportunity)
    // Rate: 1 USDC = 0.00035 WETH (1/2857), making cycle profitable: 3000 * 0.00035 = 1.05
    graph.add_edge(create_mock_edge(
        "USDC",
        "WETH",
        "0.00035", // 1/2857
        "1000000",
        "Sushiswap",
        "sushi_pair1",
    ));

    let paths = graph.find_arbitrage_paths("WETH", 3, 0.01);
    
    // Should find the cross-DEX arbitrage opportunity
    assert!(paths.len() > 0, "Should find cross-DEX arbitrage opportunity");
    
    if !paths.is_empty() {
        let best_path = &paths[0];
        // Verify the path uses different DEXs
        assert!(best_path.format_path_chain().contains("Uniswap_V2"));
        assert!(best_path.format_path_chain().contains("Sushiswap"));
    }
}

/// 测试复杂的四角套利路径
#[tokio::test]
async fn test_quadrilateral_arbitrage() {
    let event_listener = Arc::new(Mutex::new(EventListener::new_without_connection(10)));
    let mut graph = ExchangeGraph::new(event_listener, None).unwrap();

    // Create a 4-token arbitrage cycle: WETH -> USDC -> DAI -> USDT -> WETH
    graph.add_edge(create_mock_edge("WETH", "USDC", "3000", "1000000", "Uniswap_V2", "pair1"));
    graph.add_edge(create_mock_edge("USDC", "DAI", "1.02", "1000000", "Uniswap_V2", "pair2"));
    graph.add_edge(create_mock_edge("DAI", "USDT", "1.01", "1000000", "Uniswap_V2", "pair3"));
    graph.add_edge(create_mock_edge("USDT", "WETH", "0.000345", "1000000", "Uniswap_V2", "pair4"));
    // Total rate: 3000 * 1.02 * 1.01 * 0.000345 = 1.063 (profitable)

    let paths = graph.find_arbitrage_paths("WETH", 4, 0.01);
    
    assert!(paths.len() > 0, "Should find quadrilateral arbitrage path");
    
    if !paths.is_empty() {
        let best_path = &paths[0];
        // Verify it's a 4-step path (4 tokens, so 4 arrows in the chain)
        let path_chain = best_path.format_path_chain();
        let arrow_count = path_chain.matches(" -> ").count();
        assert_eq!(arrow_count, 4, "Should be a 4-step path (4 arrows for cycle)");
    }
}

/// 测试利润率阈值过滤功能
#[tokio::test]
async fn test_profit_threshold_filtering() {
    let event_listener = Arc::new(Mutex::new(EventListener::new_without_connection(10)));
    let mut graph = ExchangeGraph::new(event_listener, None).unwrap();

    // Create a high-profit arbitrage opportunity
    graph.add_edge(create_mock_edge("WETH", "USDC", "3000", "1000000", "Uniswap_V2", "pair1"));
    graph.add_edge(create_mock_edge("USDC", "WETH", "0.00036", "1000000", "Uniswap_V2", "pair2"));
    // Gross profit rate: (3000 * 0.00036) - 1 = 0.08 = 8%
    // Net profit will be lower due to fees but should still be significant

    // With very high threshold (10%), should find no paths
    let paths_high_threshold = graph.find_arbitrage_paths("WETH", 3, 0.10);
    assert_eq!(paths_high_threshold.len(), 0, "Should find no paths with very high threshold");

    // With reasonable threshold (2%), should find the path
    let paths_low_threshold = graph.find_arbitrage_paths("WETH", 3, 0.02);
    assert!(paths_low_threshold.len() > 0, "Should find paths with reasonable threshold");
}

/// 测试空图的行为
#[tokio::test]
async fn test_empty_graph_behavior() {
    let event_listener = Arc::new(Mutex::new(EventListener::new_without_connection(10)));
    let graph = ExchangeGraph::new(event_listener, None).unwrap();
    
    // Empty graph should return no paths
    let paths = graph.find_arbitrage_paths("WETH", 3, 0.01);
    assert_eq!(paths.len(), 0, "Empty graph should have no arbitrage paths");
}

/// 测试单边图的行为
#[tokio::test]
async fn test_single_edge_graph() {
    let event_listener = Arc::new(Mutex::new(EventListener::new_without_connection(10)));
    let mut graph = ExchangeGraph::new(event_listener, None).unwrap();
    
    // Add only one edge
    graph.add_edge(create_mock_edge("WETH", "USDC", "3000", "1000000", "Uniswap_V2", "pair1"));
    
    // Single edge should not create arbitrage opportunity
    let paths = graph.find_arbitrage_paths("WETH", 3, 0.01);
    assert_eq!(paths.len(), 0, "Single edge should not create arbitrage opportunity");
}

/// 测试create_mock_edge辅助函数
#[test]
fn test_create_mock_edge() {
    // Test the helper function for creating mock exchange edges
    let edge = create_mock_edge("WETH", "USDC", "3000", "1000000", "Uniswap_V2", "test_pair");
    
    assert_eq!(edge.from_token, "WETH");
    assert_eq!(edge.to_token, "USDC");
    assert_eq!(edge.exchange_rate, BigDecimal::from_str("3000").unwrap());
    assert_eq!(edge.liquidity, BigDecimal::from_str("1000000").unwrap());
    assert_eq!(edge.dex, "Uniswap_V2");
    assert_eq!(edge.pair_id, "test_pair");
    assert_eq!(edge.slippage, 0.001);
    assert_eq!(edge.fee_percentage, 0.003);
}

/// 测试ExchangeEdge的直接创建
#[test]
fn test_exchange_edge_creation() {
    // Test direct creation of ExchangeEdge
    let edge = ExchangeEdge {
        from_token: "ETH".to_string(),
        to_token: "BTC".to_string(),
        exchange_rate: BigDecimal::from_str("15.5").unwrap(),
        liquidity: BigDecimal::from_str("500000").unwrap(),
        dex: "TestDEX".to_string(),
        pair_id: "eth_btc_pair".to_string(),
        gas_cost: BigDecimal::from_str("0.01").unwrap(),
        slippage: 0.002,
        fee_percentage: 0.0025,
    };

    assert_eq!(edge.from_token, "ETH");
    assert_eq!(edge.to_token, "BTC");
    assert_eq!(edge.exchange_rate, BigDecimal::from_str("15.5").unwrap());
    assert_eq!(edge.liquidity, BigDecimal::from_str("500000").unwrap());
    assert_eq!(edge.dex, "TestDEX");
    assert_eq!(edge.pair_id, "eth_btc_pair");
    assert_eq!(edge.gas_cost, BigDecimal::from_str("0.01").unwrap());
    assert_eq!(edge.slippage, 0.002);
    assert_eq!(edge.fee_percentage, 0.0025);
}