use anyhow::Result;
use arbitrage_spy::database::Database;
use arbitrage_spy::thegraph::TheGraphClient;
use log::info;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    env_logger::init();

    info!("测试交易对数据库功能");

    // 初始化数据库
    let database = Database::new(Some("data/test_pairs.db"))?;
    info!("数据库初始化完成");

    // 获取交易对数据
    info!("从 TheGraph 获取交易对数据...");
    let graph_client = TheGraphClient::new();
    let pairs = graph_client.get_top_pairs(10).await?;
    info!("获取到 {} 个交易对", pairs.len());

    // 保存到数据库
    info!("保存交易对到数据库...");
    database.save_pairs(&pairs)?;
    info!("交易对数据已保存到数据库");

    // 从数据库加载交易对
    info!("从数据库加载交易对...");
    let loaded_pairs = database.load_pairs()?;
    info!("从数据库加载了 {} 个交易对", loaded_pairs.len());

    // 显示前几个交易对
    for (i, pair) in loaded_pairs.iter().take(3).enumerate() {
        info!(
            "交易对 {}: {}/{} - 地址: {} - 24h交易量: ${} - 流动性: ${}",
            i + 1,
            pair.token0.symbol,
            pair.token1.symbol,
            pair.id,
            pair.volume_usd,
            pair.reserve_usd
        );
    }

    info!("测试完成！");
    Ok(())
}