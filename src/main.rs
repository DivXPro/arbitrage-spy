use anyhow::Result;
use log::info;

mod cli;
mod config;
mod database;
mod dex;
mod event_listener;
mod monitor;
mod pairs;
mod price_calculator;
mod realtime_monitor;
mod table_display;
mod thegraph;
mod token;
mod types;
mod utils;

use cli::CliApp;

#[tokio::main]
async fn main() -> Result<()> {
    // 加载 .env 文件
    dotenv::dotenv().ok();

    // 初始化标准日志系统
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    info!("启动区块链套利监控系统...");

    // 解析命令行参数
    let matches = CliApp::build_cli().get_matches();

    // 创建CLI应用程序实例
    let app = CliApp::new().await?;

    // 运行应用程序
    app.run(matches).await?;

    Ok(())
}
