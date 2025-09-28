use anyhow::Result;
use clap::{Arg, Command, ArgMatches};
use log::{error, info};

use crate::config::Config;
use crate::core::ArbitrageTrade;
use crate::store::database::Database;
use crate::store::pair_manager::PairManager;
use crate::realtime_monitor::RealTimeMonitor;
use crate::store::thegraph::TheGraphClient;
use crate::store::token_manager::TokenManager;
use crate::log_adapter::LogAdapter;

// 命令行参数常量
const UPDATE_TOKENS_ARG: &str = "update";
const UPDATE_PAIRS_ARG: &str = "update-pairs";
const MONITOR_ARG: &str = "monitor";
const GRAPH_ARG: &str = "graph";

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
            // 数据更新相关命令
            .args(Self::build_update_args())
            // 监控相关命令
            .args(Self::build_monitor_args())
            // 图构建相关命令
            .args(Self::build_graph_args())
    }

    /// 构建数据更新相关的命令参数
    fn build_update_args() -> Vec<Arg> {
        vec![
            Arg::new(UPDATE_TOKENS_ARG)
                .long(UPDATE_TOKENS_ARG)
                .help("更新 token 数据")
                .action(clap::ArgAction::SetTrue),
            
            Arg::new(UPDATE_PAIRS_ARG)
                .long(UPDATE_PAIRS_ARG)
                .help("更新交易对数据")
                .action(clap::ArgAction::SetTrue),
        ]
    }

    /// 构建监控相关的命令参数
    fn build_monitor_args() -> Vec<Arg> {
        vec![
            Arg::new(MONITOR_ARG)
                .long(MONITOR_ARG)
                .short('m')
                .help("启动实时监控模式")
                .action(clap::ArgAction::SetTrue),
            
            Arg::new("count")
                .long("count")
                .short('c')
                .help("显示的交易对数量 (默认: 100)")
                .value_name("NUMBER")
                .default_value("100")
                .requires(MONITOR_ARG),
            
            Arg::new("interval")
                .long("interval")
                .short('i')
                .help("更新间隔秒数 (默认: 1)")
                .value_name("SECONDS")
                .default_value("1")
                .requires(MONITOR_ARG),
        ]
    }

    /// 构建图构建相关的命令参数
    fn build_graph_args() -> Vec<Arg> {
        vec![
            Arg::new(GRAPH_ARG)
                .long(GRAPH_ARG)
                .short('g')
                .help("从数据库读取交易对并生成ExchangeGraph")
                .action(clap::ArgAction::SetTrue),
            
            Arg::new("protocol")
                .long("protocol")
                .short('p')
                .help("指定协议类型 (v2, v3, all)")
                .value_name("PROTOCOL")
                .default_value("v3")
                .requires(GRAPH_ARG),
            
            Arg::new("network")
                .long("network")
                .short('n')
                .help("指定网络")
                .value_name("NETWORK")
                .requires(GRAPH_ARG),
        ]
    }

    /// 运行CLI应用程序
    pub async fn run(&self, matches: ArgMatches) -> Result<()> {
        // 根据命令类型初始化相应的日志系统
        if matches.get_flag(MONITOR_ARG) {
            // 监控模式：使用TUI表格显示日志
            LogAdapter::init_for_monitor().expect("Failed to initialize log adapter for monitor mode");
        } else {
            // Update命令：使用终端显示日志
            LogAdapter::init_for_terminal().expect("Failed to initialize log adapter for terminal mode");
        }

        info!("启动区块链套利监控系统...");

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

        // 检查是否执行graph命令
        if matches.get_flag(GRAPH_ARG) {
            let protocol = matches.get_one::<String>("protocol").unwrap();
            let network = matches.get_one::<String>("network");
            
            info!("执行graph命令...");
            self.build_exchange_graph(protocol, network).await?;
            return Ok(());
        }

        // 检查是否启动实时监控模式
        if matches.get_flag(MONITOR_ARG) {
            let count: usize = matches.get_one::<String>("count")
                .unwrap()
                .parse()
                .unwrap_or(100);
            
            info!("启动实时监控模式...");
            self.start_realtime_monitor(count).await?;
            return Ok(());
        }

        Ok(())
    }

    /// 启动实时监控模式
    async fn start_realtime_monitor(&self, count: usize) -> Result<()> {
        info!("正在启动实时监控...");
        
        // 创建实时监控器
        let monitor = RealTimeMonitor::new(self.config.clone(), self.database.clone()).await?;
        
        // 开始监控
        monitor.start_monitoring(count).await?;
        
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

    /// 构建ExchangeGraph
    async fn build_exchange_graph(&self, protocol: &str, network: Option<&String>) -> Result<()> {
        info!("开始构建ExchangeGraph，协议: {}, 网络: {:?}", protocol, network);

        let graph = match (protocol, network) {
            ("v3", None) => {
                info!("构建V3协议的ExchangeGraph...");
                ArbitrageTrade::build_v3_exchange_graph(&self.database).await?
            },
            _ => {
                error!("不支持的协议类型: {}", protocol);
                return Ok(());
            }
        };

        let (token_count, edge_count) = ArbitrageTrade::get_graph_stats(&graph);
        let last_updated = {
            let graph_guard = graph.lock().unwrap();
            graph_guard.last_updated
        };
        info!("ExchangeGraph构建完成！");
        info!("统计信息:");
        info!("  - 代币数量: {}", token_count);
        info!("  - 边数量: {}", edge_count);
        info!("  - 最后更新时间: {}", last_updated);

        // 等待后台任务执行，观察链上数据更新过程
        info!("等待后台任务执行链上数据更新...");
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        
        // 再次检查统计信息，看是否有更新
        let (updated_token_count, updated_edge_count) = ArbitrageTrade::get_graph_stats(&graph);
        let updated_last_updated = {
            let graph_guard = graph.lock().unwrap();
            graph_guard.last_updated
        };
        
        info!("后台任务执行后的统计信息:");
        info!("  - 代币数量: {}", updated_token_count);
        info!("  - 边数量: {}", updated_edge_count);
        info!("  - 最后更新时间: {}", updated_last_updated);

        // 执行套利路径查找（不要求利润率，路径节点在4个以内）
        info!("开始执行套利路径查找...");
        self.find_arbitrage_paths_in_graph(&graph).await?;

        Ok(())
    }

    /// 在交换图中查找套利路径
    async fn find_arbitrage_paths_in_graph(&self, graph: &std::sync::Arc<std::sync::Mutex<crate::core::exchange_graph::ExchangeGraph>>) -> Result<()> {
        info!("🔍 开始套利路径搜索分析...");
        
        // 获取图中的代币列表
        let tokens: Vec<String> = {
            let graph_guard = graph.lock().unwrap();
            graph_guard.tokens.iter().cloned().collect()
        };
        
        if tokens.is_empty() {
            info!("⚠️  图中没有代币，跳过套利路径搜索");
            return Ok(());
        }
        
        info!("📊 图中共有 {} 个代币", tokens.len());
        
        // 选择前几个代币进行套利路径搜索（避免搜索时间过长）
        let search_tokens: Vec<String> = vec![String::from("0xdac17f958d2ee523a2206206994597c13d831ec7")];

        info!("🎯 将对以下代币进行套利路径搜索: {:?}", search_tokens);
        
        let mut total_paths_found = 0;
        let max_depth = 4; // 路径节点在4个以内
        let min_profit_threshold = -100.0; // 不要求利润率（使用负数阈值）
        
        for token in &search_tokens {
            info!("🔍 搜索从 {} 开始的套利路径...", token);
            
            let paths = {
                let graph_guard = graph.lock().unwrap();
                graph_guard.find_arbitrage_paths(token, max_depth, min_profit_threshold)
            };
            
            let path_count = paths.len();
            total_paths_found += path_count;
            
            info!("📈 从 {} 开始找到 {} 条路径", token, path_count);
            
            // 显示前3条最佳路径的详细信息
            for (i, path) in paths.iter().take(3).enumerate() {
                info!("  路径 {}: {} (净利润率: {:.4}%)", 
                     i + 1, 
                     path.format_path_chain(), 
                     path.net_profit_rate * 100.0);
            }
            
            if path_count > 3 {
                info!("  ... 还有 {} 条其他路径", path_count - 3);
            }
            
            // 添加短暂延迟，避免过度占用资源
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        
        info!("✅ 套利路径搜索完成！");
        info!("📊 总计找到 {} 条套利路径", total_paths_found);
        
        if total_paths_found > 0 {
            info!("💡 提示: 这些路径包括盈利和亏损的所有闭环路径");
        } else {
            info!("⚠️  未找到任何套利路径，可能是图中缺少足够的连接");
        }
        
        Ok(())
    }
}