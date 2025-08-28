use anyhow::Result;
use clap::{Arg, Command};
use log::{error, info};
use std::time::Duration;
use tokio::time;

mod config;
mod database;
mod dex;
mod monitor;
mod thegraph;
mod token;
mod types;
mod utils;

use config::Config;
use database::Database;
use monitor::ArbitrageMonitor;
use thegraph::TheGraphClient;
use token::TokenManager;

// 命令行参数常量
const UPDATE_TOKENS_ARG: &str = "update";

#[tokio::main]
async fn main() -> Result<()> {
    // 加载 .env 文件
    dotenv::dotenv().ok();

    // 初始化日志系统
    env_logger::init();

    // 解析命令行参数
    let matches = Command::new("arbitrage-spy")
        .version("1.0")
        .about("区块链套利监控系统")
        .arg(
            Arg::new(UPDATE_TOKENS_ARG)
                .long(UPDATE_TOKENS_ARG)
                .help("更新 token 数据")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    info!("启动区块链套利监控系统...");

    // 加载配置
    let config = Config::load()?;
    info!("配置加载完成");

    // 初始化数据库
    info!("初始化数据库...");
    let database = Database::new(Some("data/tokens.db"))?;
    info!("数据库初始化完成");

    // 检查是否只需要更新 token
    if matches.get_flag(UPDATE_TOKENS_ARG) {
        info!("执行 token 更新命令...");
        update_tokens(&database).await?;
        info!("Token 更新完成");
        return Ok(());
    }

    // 正常启动模式 - 初始化完整的监控系统
    info!("启动完整监控系统...");

    // 初始化 Token 管理器
    let token_manager = TokenManager::new(Some("data/tokens.json".to_string()));

    // 获取并缓存 token 列表
    info!("获取 token 列表...");
    let token_list = token_manager.update_tokens(None).await?;
    info!("获取到 {} 个 token", token_list.tokens.len());

    // 保存到数据库
    database.save_tokens(&token_list.tokens)?;
    info!("Token 数据已保存到数据库");

    // 显示数据库统计
    let (total_tokens, last_update) = database.get_stats()?;
    info!(
        "数据库统计: 总计 {} 个 token，最后更新: {}",
        total_tokens, last_update
    );

    // 初始化套利监控器
    info!("初始化套利监控器...");
    let mut monitor = ArbitrageMonitor::new(config).await?;
    monitor.start_scan().await;

    // 开始监控
    info!("开始监控套利机会...");
    // monitor.start_monitoring().await?;
    info!("监控系统已启动");

    Ok(())
}

/// 独立的 token 更新功能
async fn update_tokens(database: &Database) -> Result<()> {
    info!("开始更新 token 数据...");

    // 初始化 Token 管理器
    let token_manager = TokenManager::new(Some("data/tokens.json".to_string()));

    // 获取 token 列表
    info!("从 CoinGecko API 获取 token 列表...");
    let token_list = token_manager.update_tokens(None).await?;
    info!("获取到 {} 个 token", token_list.tokens.len());

    // 保存到数据库
    database.save_tokens(&token_list.tokens)?;
    info!("Token 数据已保存到数据库");

    // 显示更新后的统计
    let (total_tokens, last_update) = database.get_stats()?;
    info!(
        "更新完成 - 总计 {} 个 token，最后更新: {}",
        total_tokens, last_update
    );

    // 获取 Uniswap V2 交易对
    info!("从 TheGraph 获取 Uniswap V2 交易对...");
    let graph_client = TheGraphClient::new();
    match graph_client.get_top_pairs(100).await {
        Ok(pairs) => {
            info!("成功获取到 {} 个 Uniswap V2 交易对", pairs.len());
            // 显示前几个交易对的信息
            for (i, pair) in pairs.iter().take(3).enumerate() {
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
        }
        Err(e) => {
            error!("获取 Uniswap V2 交易对失败: {}", e);
        }
    }

    Ok(())
}
