use anyhow::Result;
use clap::{Arg, Command, ArgMatches};
use log::{error, info};

use crate::config::Config;
use crate::database::Database;
use crate::monitor::ArbitrageMonitor;
use crate::thegraph::TheGraphClient;
use crate::token::TokenManager;

// 命令行参数常量
const UPDATE_TOKENS_ARG: &str = "update";

/// CLI应用程序结构
pub struct CliApp {
    config: Config,
    database: Database,
}

impl CliApp {
    /// 创建新的CLI应用程序实例
    pub async fn new() -> Result<Self> {
        // 加载配置
        let config = Config::load()?;
        info!("配置加载完成");

        // 初始化数据库
        info!("初始化数据库...");
        let database = Database::new(Some("data/tokens.db"))?;
        info!("数据库初始化完成");

        Ok(Self { config, database })
    }

    /// 构建命令行参数解析器
    pub fn build_cli() -> Command {
        Command::new("arbitrage-spy")
            .version("1.0")
            .about("区块链套利监控系统")
            .arg(
                Arg::new(UPDATE_TOKENS_ARG)
                    .long(UPDATE_TOKENS_ARG)
                    .help("更新 token 数据")
                    .action(clap::ArgAction::SetTrue),
            )
    }

    /// 运行CLI应用程序
    pub async fn run(&self, matches: ArgMatches) -> Result<()> {
        // 检查是否只需要更新 token
        if matches.get_flag(UPDATE_TOKENS_ARG) {
            info!("执行 token 更新命令...");
            self.update_data().await?;
            info!("Token 更新完成");
            return Ok(());
        }

        // 正常启动模式 - 初始化完整的监控系统
        info!("启动完整监控系统...");
        self.start_monitoring().await?;

        Ok(())
    }

    /// 启动完整的监控系统
    async fn start_monitoring(&self) -> Result<()> {
        // 初始化 Token 管理器
        let token_manager = TokenManager::new(&self.database);

        // 获取并缓存 token 列表
        info!("获取 token 列表...");
        let token_list = token_manager.fetch_tokens(None).await?;
        info!("获取到 {} 个 token", token_list.tokens.len());

        // 保存到数据库
        self.database.save_tokens(&token_list.tokens)?;
        info!("Token 数据已保存到数据库");

        // 显示数据库统计
        let (total_tokens, last_update) = self.database.get_stats()?;
        info!(
            "数据库统计: 总计 {} 个 token，最后更新: {}",
            total_tokens, last_update
        );

        // 初始化套利监控器
        info!("初始化套利监控器...");
        let mut monitor = ArbitrageMonitor::new(self.config.clone()).await?;
        monitor.start_scan().await;

        // 开始监控
        info!("开始监控套利机会...");
        // monitor.start_monitoring().await?;
        info!("监控系统已启动");

        Ok(())
    }

    async fn update_data(&self) -> Result<()> {
        self.update_tokens().await?;
        self.update_pairs().await?;
        Ok(())
    }

    /// 独立的 token 更新功能
    async fn update_tokens(&self) -> Result<()> {
        info!("开始更新 token 数据...");

        // 初始化 Token 管理器
        let token_manager = TokenManager::new(&self.database);

        // 获取 token 列表
        info!("从 CoinGecko API 获取 token 列表...");
        let token_list = token_manager.fetch_tokens(None).await?;
        info!("获取到 {} 个 token", token_list.tokens.len());

        // 保存到数据库
        self.database.save_tokens(&token_list.tokens)?;
        info!("Token 数据已保存到数据库");

        // 显示更新后的统计
        let (total_tokens, last_update) = self.database.get_stats()?;
        info!(
            "更新完成 - 总计 {} 个 token，最后更新: {}",
            total_tokens, last_update
        );

        Ok(())
    }

    /// 独立的 pairs 更新功能
    async fn update_pairs(&self) -> Result<()> {
        info!("开始更新 pairs 数据...");

        // 获取 Uniswap V2 交易对
        info!("从 TheGraph 获取 Uniswap V2 交易对...");
        let graph_client = TheGraphClient::new();
        match graph_client.get_top_pairs(100).await {
            Ok(pairs) => {
                info!("成功获取到 {} 个 Uniswap V2 交易对", pairs.len());
                
                // 保存交易对到数据库
                if let Err(e) = self.database.save_pairs(&pairs) {
                    error!("保存交易对到数据库失败: {}", e);
                } else {
                    info!("交易对数据已保存到数据库");
                }
            }
            Err(e) => {
                error!("获取 Uniswap V2 交易对失败: {}", e);
            }
        }

        Ok(())
    }
}