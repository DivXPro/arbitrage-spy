use anyhow::Result;
use log::{info, error};
use std::time::Duration;
use tokio::time;

mod config;
mod dex;
mod monitor;
mod types;
mod utils;

use config::Config;
use monitor::ArbitrageMonitor;

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
    
    // 创建监控器
    let mut monitor = ArbitrageMonitor::new(config).await?;
    info!("监控器初始化完成");
    
    // 启动监控循环
    let mut interval = time::interval(Duration::from_secs(10));
    
    loop {
        interval.tick().await;
        
        match monitor.scan_opportunities().await {
            Ok(opportunities) => {
                if !opportunities.is_empty() {
                    info!("发现 {} 个套利机会", opportunities.len());
                    for opportunity in opportunities {
                        info!("套利机会: {:?}", opportunity);
                    }
                }
            }
            Err(e) => {
                error!("扫描套利机会时出错: {}", e);
            }
        }
    }
}