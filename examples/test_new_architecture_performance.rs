use arbitrage_spy::core::exchange_graph::{ExchangeGraph, ExchangeEdge};
use arbitrage_spy::data::pair_manager::{PairData, TokenInfo};
use std::time::Instant;
use std::sync::Arc;

fn create_test_pair(id: &str, token0: &str, token1: &str, reserve0: &str, reserve1: &str) -> PairData {
    PairData {
        id: id.to_string(),
        network: "ethereum".to_string(),
        dex: "uniswap_v2".to_string(),
        protocol_type: "v2".to_string(),
        token0: TokenInfo {
            id: format!("{}_token0", id),
            symbol: token0.to_string(),
            name: format!("{} Token", token0),
            decimals: "18".to_string(),
        },
        token1: TokenInfo {
            id: format!("{}_token1", id),
            symbol: token1.to_string(),
            name: format!("{} Token", token1),
            decimals: "18".to_string(),
        },
        volume_usd: "100000".to_string(),
        reserve_usd: "1000000".to_string(),
        tx_count: "1000".to_string(),
        reserve0: reserve0.to_string(),
        reserve1: reserve1.to_string(),
        fee_tier: "3000".to_string(),
        sqrt_price: None,
        tick: None,
    }
}

#[tokio::main]
async fn main() {
    println!("🚀 测试新架构的性能改进");
    
    // 创建大量测试数据
    let mut pairs = Vec::new();
    let tokens = ["WETH", "USDC", "USDT", "DAI", "WBTC", "UNI", "LINK", "AAVE", "COMP", "MKR"];
    
    println!("📊 创建测试数据...");
    let mut pair_id = 0;
    for i in 0..tokens.len() {
        for j in (i+1)..tokens.len() {
            for dex_variant in 0..3 { // 每个代币对在3个不同DEX上
                pair_id += 1;
                let pair = create_test_pair(
                    &format!("pair_{}_{}", pair_id, dex_variant),
                    tokens[i],
                    tokens[j],
                    "1000000000000000000000",
                    "2000000000000000000000"
                );
                pairs.push(pair);
            }
        }
    }
    
    println!("✅ 创建了 {} 个交易对", pairs.len());
    
    // 测试图构建性能
    println!("\n🔧 测试图构建性能...");
    let start = Instant::now();
    let mut graph = ExchangeGraph::new();
    graph.from_pair_data(&pairs, None).await.expect("构建图失败");
    let build_time = start.elapsed();
    
    let (token_count, edge_count) = graph.get_stats();
    println!("✅ 图构建完成:");
    println!("   - 代币数量: {}", token_count);
    println!("   - 边数量: {}", edge_count);
    println!("   - 交易对数量: {}", pairs.len());
    println!("   - 构建时间: {:?}", build_time);
    
    // 测试PairData访问性能
    println!("\n🔍 测试PairData访问性能...");
    let start = Instant::now();
    let mut found_count = 0;
    for pair in &pairs {
        if let Some(_pair_data) = graph.get_pair_data(&pair.id) {
            found_count += 1;
        }
    }
    let access_time = start.elapsed();
    println!("✅ PairData访问测试:");
    println!("   - 查找次数: {}", pairs.len());
    println!("   - 找到数量: {}", found_count);
    println!("   - 平均访问时间: {:?}", access_time / pairs.len() as u32);
    
    // 测试更新性能
    println!("\n🔄 测试更新性能...");
    let start = Instant::now();
    let mut updated_pairs = pairs.clone();
    for pair in &mut updated_pairs {
        pair.reserve0 = "1500000000000000000000".to_string();
        pair.reserve1 = "2500000000000000000000".to_string();
    }
    
    for pair in &updated_pairs {
        graph.update_pair_data(pair).expect("更新失败");
    }
    let update_time = start.elapsed();
    
    println!("✅ 批量更新测试:");
    println!("   - 更新数量: {}", updated_pairs.len());
    println!("   - 总更新时间: {:?}", update_time);
    println!("   - 平均更新时间: {:?}", update_time / updated_pairs.len() as u32);
    
    // 测试套利路径查找性能
    println!("\n🎯 测试套利路径查找性能...");
    let start = Instant::now();
    let paths = graph.find_arbitrage_paths("WETH", 3, 0.01);
    let search_time = start.elapsed();
    
    println!("✅ 套利路径查找测试:");
    println!("   - 找到路径数量: {}", paths.len());
    println!("   - 搜索时间: {:?}", search_time);
    
    if !paths.is_empty() {
        println!("   - 最佳路径盈利率: {:.2}%", paths[0].profit_rate);
        println!("   - 最佳路径详情: {}", paths[0].get_summary());
    }
    
    // 内存使用情况估算
    println!("\n💾 内存使用情况分析:");
    let pair_data_size = std::mem::size_of::<PairData>();
    let edge_size = std::mem::size_of::<ExchangeEdge>();
    let arc_size = std::mem::size_of::<Arc<PairData>>();
    
    println!("   - 单个PairData大小: {} bytes", pair_data_size);
    println!("   - 单个ExchangeEdge大小: {} bytes", edge_size);
    println!("   - 单个Arc<PairData>大小: {} bytes", arc_size);
    
    let estimated_pair_memory = pairs.len() * pair_data_size;
    let estimated_edge_memory = edge_count * edge_size;
    let estimated_total = estimated_pair_memory + estimated_edge_memory;
    
    println!("   - 估算PairData内存: {} KB", estimated_pair_memory / 1024);
    println!("   - 估算Edge内存: {} KB", estimated_edge_memory / 1024);
    println!("   - 估算总内存: {} KB", estimated_total / 1024);
    
    println!("\n🎉 新架构性能测试完成!");
    println!("📈 主要优势:");
    println!("   ✓ PairData统一管理，避免重复存储");
    println!("   ✓ ExchangeEdge轻量化，只保留引用");
    println!("   ✓ 内存使用更高效");
    println!("   ✓ 数据一致性更好");
}