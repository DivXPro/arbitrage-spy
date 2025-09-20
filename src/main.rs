use anyhow::Result;

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

#[tokio::main]
async fn main() -> Result<()> {
    // 加载 .env 文件
    dotenv::dotenv().ok();

    // 解析命令行参数
    let matches = CliApp::build_cli().get_matches();

    // 创建CLI应用程序实例并运行（日志初始化在CLI模块中根据命令类型进行）
    let app = CliApp::new().await?;
    app.run(matches).await?;

    Ok(())
}
