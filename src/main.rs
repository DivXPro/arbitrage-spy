use anyhow::Result;
use log::info;

mod cli;
mod config;
mod core;
mod data;
mod dex;
mod event_listener;
mod log_adapter;
mod price_calculator;
mod realtime_monitor;
mod table_display;
mod utils;

use cli::CliApp;
use log_adapter::LogAdapter;

#[tokio::main]
async fn main() -> Result<()> {
    // 加载 .env 文件
    dotenv::dotenv().ok();

    // 初始化日志适配器系统（默认为终端模式）
    LogAdapter::init().expect("Failed to initialize log adapter");

    info!("启动区块链套利监控系统...");

    // 解析命令行参数
    let matches = CliApp::build_cli().get_matches();

    // 创建CLI应用程序实例
    let app = CliApp::new().await?;

    // 运行应用程序
    app.run(matches).await?;

    Ok(())
}
