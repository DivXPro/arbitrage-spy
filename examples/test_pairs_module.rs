//! 测试独立的pairs模块功能

use anyhow::Result;
use arbitrage_spy::database::Database;
use arbitrage_spy::pairs::PairManager;
use arbitrage_spy::thegraph::{PairData, TheGraphClient, TokenInfo};
use log::info;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    info!("开始测试独立的pairs模块功能");

    // 初始化数据库
    let database = Database::new(Some("test_pairs_module.db"))?;
    info!("数据库初始化完成");

    // 创建PairManager实例
    let pair_manager = PairManager::new(&database);
    info!("PairManager创建完成");

    // 获取演示数据
    let thegraph_client = TheGraphClient::new();
    let pairs = match thegraph_client.get_top_pairs(2).await {
        Ok(pairs) => {
            info!("从TheGraph获取到 {} 个交易对", pairs.len());
            pairs
        }
        Err(e) => {
            info!("TheGraph API不可用: {}, 使用演示数据", e);
            vec![
                PairData {
                    id: "demo_pair_1".to_string(),
                    network: "ethereum".to_string(),
                    dex_type: "uniswap_v2".to_string(),
                    token0: TokenInfo {
                        id: "0xA0b86a33E6441E6C7D3E4C7C5C6C7D8E9F0A1B2C".to_string(),
                        symbol: "USDC".to_string(),
                        name: "USD Coin".to_string(),
                        decimals: "6".to_string(),
                    },
                    token1: TokenInfo {
                        id: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
                        symbol: "WETH".to_string(),
                        name: "Wrapped Ether".to_string(),
                        decimals: "18".to_string(),
                    },
                    volume_usd: "1000000".to_string(),
                    reserve_usd: "5000000".to_string(),
                    tx_count: "1000".to_string(),
                    reserve0: "1000000".to_string(),
                    reserve1: "5000000".to_string(),
                },
                PairData {
                    id: "demo_pair_2".to_string(),
                    network: "ethereum".to_string(),
                    dex_type: "uniswap_v2".to_string(),
                    token0: TokenInfo {
                        id: "0x6B175474E89094C44Da98b954EedeAC495271d0F".to_string(),
                        symbol: "DAI".to_string(),
                        name: "Dai Stablecoin".to_string(),
                        decimals: "18".to_string(),
                    },
                    token1: TokenInfo {
                        id: "0xA0b86a33E6441E6C7D3E4C7C5C6C7D8E9F0A1B2C".to_string(),
                        symbol: "USDC".to_string(),
                        name: "USD Coin".to_string(),
                        decimals: "6".to_string(),
                    },
                    volume_usd: "800000".to_string(),
                    reserve_usd: "3000000".to_string(),
                    tx_count: "800".to_string(),
                    reserve0: "1000000".to_string(),
                    reserve1: "5000000".to_string(),
                },
            ]
        }
    };

    // 使用Database的委托方法保存交易对
    database.save_pairs(&pairs)?;
    info!("通过Database委托方法保存了 {} 个交易对", pairs.len());

    // 使用Database的委托方法加载交易对
    let loaded_pairs = database.load_pairs()?;
    info!("通过Database委托方法加载了 {} 个交易对", loaded_pairs.len());

    // 测试新的筛选功能
    let ethereum_pairs = database.load_pairs_by_filter(Some("ethereum"), None, Some(10))?;
    info!("筛选到 {} 个以太坊网络的交易对", ethereum_pairs.len());

    let uniswap_pairs = database.load_pairs_by_filter(None, Some("uniswap_v2"), Some(10))?;
    info!("筛选到 {} 个Uniswap V2的交易对", uniswap_pairs.len());

    // 测试根据ID查找
    if let Some(first_pair) = loaded_pairs.first() {
        let found_pair = database.find_pair_by_id(&first_pair.id)?;
        match found_pair {
            Some(pair) => info!("成功找到交易对: {} - {}/{}", pair.id, pair.token0.symbol, pair.token1.symbol),
            None => info!("未找到指定的交易对"),
        }
    }

    // 获取统计信息
    let (count, total_volume, total_reserve) = database.get_pairs_stats()?;
    info!("交易对统计: 总数={}, 总交易量=${:.2}, 总储备=${:.2}", count, total_volume, total_reserve);

    // 直接使用PairManager测试
    info!("\n=== 直接使用PairManager测试 ===");
    
    // 创建测试数据
    let test_pairs = vec![
        PairData {
            id: "test_pair_1".to_string(),
            network: "polygon".to_string(),
            dex_type: "sushiswap".to_string(),
            token0: TokenInfo {
                id: "0x123".to_string(),
                symbol: "MATIC".to_string(),
                name: "Polygon".to_string(),
                decimals: "18".to_string(),
            },
            token1: TokenInfo {
                id: "0x456".to_string(),
                symbol: "USDC".to_string(),
                name: "USD Coin".to_string(),
                decimals: "6".to_string(),
            },
            volume_usd: "500000".to_string(),
            reserve_usd: "2000000".to_string(),
            tx_count: "500".to_string(),
            reserve0: "1000000".to_string(),
            reserve1: "5000000".to_string(),
        },
    ];

    // 注意：这里我们无法直接访问database.conn，因为它是私有的
    // 所以我们通过Database的方法来测试
    database.save_pairs(&test_pairs)?;
    info!("通过Database保存了 {} 个测试交易对", test_pairs.len());

    // 测试筛选功能
    let polygon_pairs = database.load_pairs_by_filter(Some("polygon"), None, None)?;
    info!("筛选到 {} 个Polygon网络的交易对", polygon_pairs.len());

    let sushiswap_pairs = database.load_pairs_by_filter(None, Some("sushiswap"), None)?;
    info!("筛选到 {} 个SushiSwap的交易对", sushiswap_pairs.len());

    info!("pairs模块功能测试完成!");
    Ok(())
}