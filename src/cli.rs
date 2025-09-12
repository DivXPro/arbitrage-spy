use anyhow::Result;
use clap::{Arg, Command, ArgMatches};
use log::{error, info};

use crate::config::Config;
use crate::database::Database;
use crate::monitor::ArbitrageMonitor;
use crate::pairs::PairManager;
use crate::realtime_monitor::RealTimeMonitor;
use crate::thegraph::TheGraphClient;
use crate::token::TokenManager;

// 命令行参数常量
const UPDATE_TOKENS_ARG: &str = "update";
const UPDATE_PAIRS_ARG: &str = "update-pairs";
const MONITOR_ARG: &str = "monitor";

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
            .arg(
                Arg::new(UPDATE_PAIRS_ARG)
                    .long(UPDATE_PAIRS_ARG)
                    .help("更新交易对数据")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                Arg::new(MONITOR_ARG)
                    .long(MONITOR_ARG)
                    .short('m')
                    .help("启动实时监控模式")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                Arg::new("count")
                    .long("count")
                    .short('c')
                    .help("显示的交易对数量 (默认: 10)")
                    .value_name("NUMBER")
                    .default_value("10")
                    .requires(MONITOR_ARG),
            )
            .arg(
                Arg::new("interval")
                    .long("interval")
                    .short('i')
                    .help("更新间隔秒数 (默认: 1)")
                    .value_name("SECONDS")
                    .default_value("1")
                    .requires(MONITOR_ARG),
            )

    }

    /// 运行CLI应用程序
    pub async fn run(&self, matches: ArgMatches) -> Result<()> {
        // 检查是否只需要更新 token
        if matches.get_flag(UPDATE_TOKENS_ARG) {
            info!("执行 token 更新命令...");
            self.update_data().await?;
            return Ok(());
        }

        // 检查是否只需要更新交易对
        if matches.get_flag(UPDATE_PAIRS_ARG) {
            info!("执行交易对更新命令...");
            self.update_pairs().await?;
            return Ok(());
        }

        // 检查是否启动实时监控模式
        if matches.get_flag(MONITOR_ARG) {
            let count: usize = matches.get_one::<String>("count")
                .unwrap()
                .parse()
                .unwrap_or(10);
            let interval: u64 = matches.get_one::<String>("interval")
                .unwrap()
                .parse()
                .unwrap_or(1);
            
            info!("启动实时监控模式...");
            self.start_realtime_monitor(count, interval).await?;
            return Ok(());
        }



        // 正常启动模式 - 初始化完整的监控系统
        info!("启动完整监控系统...");
        self.start_monitoring().await?;

        Ok(())
    }

    /// 启动实时监控模式
    async fn start_realtime_monitor(&self, count: usize, interval: u64) -> Result<()> {
        println!("正在启动实时监控...");
        
        // 创建实时监控器
        let monitor = RealTimeMonitor::new(self.config.clone(), self.database.clone()).await?;
        
        // 开始监控
        monitor.start_monitoring(count, interval).await?;
        
        Ok(())
    }



    /// 启动完整的监控系统
    async fn start_monitoring(&self) -> Result<()> {
        // 初始化 Token 管理器
        let token_manager = TokenManager::new(&self.database);

        // 获取并缓存 token 列表
        info!("获取 token 列表...");
        let token_list = token_manager.get_tokens(None).await?;
        info!("获取到 {} 个 token", token_list.tokens.len());

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

        // 获取 token 列表 (限制为前100个以避免长时间等待)
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

        // 通过遍历 token 表中的每个 token，查询 TheGraph 相关的交易对来更新数据
        info!("遍历 token 表，从 TheGraph 获取相关交易对...");
        let token_manager = TokenManager::new(&self.database);
        let pair_manager = PairManager::new(&self.database);
        let graph_client = TheGraphClient::new();
        
        // 只获取 market_cap_rank 前100的币种
        match token_manager.get_tokens(Some(100)).await {
            Ok(token_list) => {
                info!("从数据库获取到 {} 个 token", token_list.tokens.len());
                let mut total_pairs_saved = 0;
                
                for (index, token) in token_list.tokens.iter().enumerate() {
                    // 需要从 token 的 platforms 中获取以太坊地址
                    if let Some(ethereum_address) = token.platforms.get("ethereum").and_then(|addr| addr.as_ref()) {
                        info!("[{}/{}] 正在查询 token {} ({}) 的相关交易对...", 
                             index + 1, token_list.tokens.len(), token.symbol, ethereum_address);
                        
                        let mut all_pairs = Vec::new();
                             
                        // 从 TheGraph 查询该 token 相关的 V2 交易对
                        // match graph_client.get_pairs_by_token(ethereum_address, 25).await {
                        //     Ok(v2_pairs) => {
                        //         if !v2_pairs.is_empty() {
                        //             info!("Token {} 从 Uniswap V2 获取到 {} 个相关交易对", 
                        //                  token.symbol, v2_pairs.len());
                        //             all_pairs.extend(v2_pairs);
                        //         }
                        //     }
                        //     Err(e) => {
                        //         error!("从 TheGraph 查询 token {} 的 V2 交易对失败: {}", token.symbol, e);
                        //     }
                        // }
                        
                        // 从 TheGraph 查询该 token 相关的 V3 pools
                        match graph_client.get_v3_pools_by_token(ethereum_address, 25).await {
                            Ok(v3_pairs) => {
                                if !v3_pairs.is_empty() {
                                    info!("Token {} 从 Uniswap V3 获取到 {} 个相关交易对", 
                                         token.symbol, v3_pairs.len());
                                    all_pairs.extend(v3_pairs);
                                }
                            }
                            Err(e) => {
                                error!("从 TheGraph 查询 token {} 的 V3 交易对失败: {}", token.symbol, e);
                            }
                        }
                        
                        // 保存所有交易对到数据库
                        if !all_pairs.is_empty() {
                            info!("Token {} 总共获取到 {} 个交易对 (V2 + V3)", 
                                 token.symbol, all_pairs.len());
                            
                            if let Err(e) = pair_manager.save_pairs(&all_pairs) {
                                error!("保存 token {} 的交易对到数据库失败: {}", token.symbol, e);
                            } else {
                                total_pairs_saved += all_pairs.len();
                                info!("Token {} 的 {} 个交易对已保存到数据库", token.symbol, all_pairs.len());
                            }
                        } else {
                            info!("Token {} 未找到相关交易对", token.symbol);
                        }
                        
                        // 添加延迟以避免请求过于频繁
                        if index < token_list.tokens.len() - 1 {
                            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                        }
                    } else {
                        info!("Token {} 没有以太坊地址，跳过", token.symbol);
                    }
                }
                
                info!("更新完成！总共保存了 {} 个交易对到数据库", total_pairs_saved);
            }
            Err(e) => {
                error!("从数据库获取 token 列表失败: {}", e);
            }
        }

        Ok(())
    }
}