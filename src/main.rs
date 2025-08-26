use anyhow::Result;
use log::{error, info};
use std::time::Duration;
use tokio::time;

mod config;
mod database;
mod dex;
mod monitor;
mod token;
mod types;
mod utils;

use config::Config;
use database::Database;
use monitor::ArbitrageMonitor;
use token::TokenManager;

#[tokio::main]
async fn main() -> Result<()> {
    // 加载 .env 文件
    dotenv::dotenv().ok();

    // 初始化日志系统
    env_logger::init();

    info!("启动区块链套利监控系统...");

    // 加载配置
    let config = Config::load()?;
    info!("配置加载完成");

    // 初始化数据库
    info!("初始化数据库...");
    let database = Database::new(Some("tokens.db"))?;
    info!("数据库初始化完成");

    // 初始化token管理器并更新token数据
    info!("初始化token数据...");
    let token_manager = TokenManager::new(Some("data/tokens.json".to_string()));
    match token_manager.update_tokens(None).await {
        Ok(token_list) => {
            info!("成功加载 {} 个token", token_list.total_count);

            // 将token数据保存到数据库
            match database.save_tokens(&token_list.tokens) {
                Ok(_) => {
                    info!("token数据已成功保存到数据库");

                    // 验证数据库中的数据
                    match database.get_stats() {
                        Ok((count, last_update)) => {
                            info!(
                                "数据库统计: {} 个token，最后更新时间: {}",
                                count, last_update
                            );
                        }
                        Err(e) => {
                            error!("获取数据库统计失败: {}", e);
                        }
                    }

                    // 测试查找功能
                    if let Ok(Some(btc_token)) = database.find_token_by_symbol("BTC") {
                        info!("找到BTC token: {} - {}", btc_token.name, btc_token.symbol);
                    }
                }
                Err(e) => {
                    error!("保存token数据到数据库失败: {}", e);
                }
            }
        }
        Err(e) => {
            error!("加载token数据失败: {}", e);
            info!("将使用空的token列表继续运行");
        }
    }

    Ok(())
    // 创建监控器
    // let mut monitor = ArbitrageMonitor::new(config).await?;
    // info!("监控器初始化完成");

    // 启动监控循环
    // let mut interval = time::interval(Duration::from_secs(10));

    // loop {
    //     interval.tick().await;

    //     match monitor.scan_opportunities().await {
    //         Ok(opportunities) => {
    //             if !opportunities.is_empty() {
    //                 info!("发现 {} 个套利机会", opportunities.len());
    //                 for opportunity in opportunities {
    //                     info!("套利机会: {:?}", opportunity);
    //                 }
    //             }
    //         }
    //         Err(e) => {
    //             error!("扫描套利机会时出错: {}", e);
    //         }
    //     }
    // }
}
