use anyhow::Result;
use log::info;

mod cli;
mod config;
mod database;
mod dex;
mod monitor;
mod pairs;
mod thegraph;
mod token;
mod types;
mod utils;

use cli::CliApp;

#[tokio::main]
async fn main() -> Result<()> {
    // 加载 .env 文件
    dotenv::dotenv().ok();

    // 初始化日志系统
    env_logger::init();

    info!("启动区块链套利监控系统...");

    // 解析命令行参数
    let matches = CliApp::build_cli().get_matches();

    // 创建CLI应用程序实例
    let app = CliApp::new().await?;

    // 运行应用程序
    app.run(matches).await?;

    Ok(())
}
