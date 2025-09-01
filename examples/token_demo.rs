//! Token管理器演示程序
//! 
//! 这个示例展示如何使用TokenManager来获取和管理token数据

use anyhow::Result;
use arbitrage_spy::token::TokenManager;
use arbitrage_spy::database::Database;
use log::info;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    env_logger::init();
    
    info!("Token管理器演示程序启动");
    
    // 创建数据库和token管理器
    let database = Database::new(Some("demo_tokens.db"))?;
    let token_manager = TokenManager::new(&database);
    
    // 获取token列表（使用缓存数据以避免API限制）
    info!("正在获取token列表...");
    match token_manager.get_tokens(Some(50)).await {
        Ok(token_list) => {
            info!("成功获取 {} 个token", token_list.total_count);
            info!("最后更新时间: {}", token_list.last_updated);
            
            // 显示前10个token
            info!("\n前10个token:");
            for (i, token) in token_list.tokens.iter().take(10).enumerate() {
                info!(
                    "{}. {} ({}) - ${:.2} (排名: {})",
                    i + 1,
                    token.name,
                    token.symbol.to_uppercase(),
                    token.current_price.unwrap_or(0.0),
                    token.market_cap_rank.map(|r| r.to_string()).unwrap_or("N/A".to_string())
                );
                
                // 显示以太坊合约地址
                if let Some(Some(eth_address)) = token.platforms.get("ethereum") {
                    info!("   以太坊地址: {}", eth_address);
                }
            }
            
            // 演示按符号查找token
            info!("\n按符号查找token:");
            let symbols = ["ETH", "USDC", "USDT", "WBTC"];
            for symbol in &symbols {
                match token_manager.get_token_by_symbol(symbol).await? {
                    Some(token) => {
                        info!(
                            "找到 {}: {} - ${:.2}",
                            symbol,
                            token.name,
                            token.current_price.unwrap_or(0.0)
                        );
                    }
                    None => {
                        info!("未找到符号为 {} 的token", symbol);
                    }
                }
            }
            
            // 演示获取top tokens
            info!("\n市值前5的token:");
            let top_tokens = token_manager.get_top_tokens(5).await?;
            for (i, token) in top_tokens.iter().enumerate() {
                info!(
                    "{}. {} ({}) - 市值: ${:.0}M",
                    i + 1,
                    token.name,
                    token.symbol.to_uppercase(),
                    token.market_cap.unwrap_or(0.0) / 1_000_000.0
                );
            }
        }
        Err(e) => {
            eprintln!("获取token列表失败: {}", e);
        }
    }
    
    info!("演示程序结束");
    Ok(())
}