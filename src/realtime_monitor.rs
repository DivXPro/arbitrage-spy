use anyhow::Result;
use log::{info, error};
use tokio::sync::mpsc;

use crate::config::{dex_types, Config};
use crate::data::database::Database;
use crate::event_listener::EventListener;
use crate::log_adapter::LogAdapter;
use crate::table_display::{DisplayMessage, TableDisplay, PairDisplay, PairDisplayConverter};
use crate::data::pair_manager::PairData;

pub struct RealTimeMonitor {
    config: Config,
    database: Database,
}

impl RealTimeMonitor {
    pub async fn new(config: Config, database: Database) -> Result<Self> {
        Ok(Self {
            config,
            database,
        })
    }
    
    pub async fn start_monitoring(self, count: usize) -> Result<()> {
        info!("启动模块化实时监控系统");
        
        // 切换到表格模式，让日志显示在表格的日志区域
        LogAdapter::switch_to_table();
        info!("已切换到表格日志模式");
        
        // 创建消息通道
        let (sender, receiver) = mpsc::channel::<DisplayMessage>(100);
        info!("消息通道创建完成");
        
        // 准备初始数据
        info!("正在获取初始交易对数据");
        let pair_manager = crate::data::pair_manager::PairManager::new(&self.database);
        
        match pair_manager.load_pairs_by_value(None, Some(dex_types::UNISWAP_V3), Some(count.min(100))) {
            Ok(initial_pairs) => {
                info!("获取到 {} 个初始交易对", initial_pairs.len());
                
                match self.convert_pairs_to_display(&initial_pairs) {
                    Ok(initial_data) => {
                        info!("初始数据转换完成");
                        
                        // 继续后续逻辑
                        self.start_display_and_listener(sender, receiver, initial_data, initial_pairs, count).await
                    }
                    Err(e) => {
                        error!("数据转换失败: {}", e);
                        return Err(e);
                    }
                }
            }
            Err(e) => {
                error!("获取初始数据失败: {}", e);
                return Err(e);
            }
        }
    }
    
    async fn start_display_and_listener(
        self,
        sender: mpsc::Sender<DisplayMessage>,
        receiver: mpsc::Receiver<DisplayMessage>,
        initial_data: Vec<PairDisplay>,
        initial_pairs: Vec<PairData>,
        count: usize,
    ) -> Result<()> {
        
        // 创建表格显示模块
        info!("正在创建表格显示模块");
        match TableDisplay::new(receiver, initial_data) {
            Ok(mut table_display) => {
                info!("表格显示模块创建完成");
                
                // 创建事件监听模块，传递初始交易对数据
                info!("正在创建事件监听模块");
                let mut event_listener = EventListener::new(
                    self.database.clone(),
                    sender,
                    count,
                    initial_pairs,
                ).await;
                info!("事件监听模块创建完成");
                
                // 启动两个模块
                info!("正在启动表格显示模块");
                let display_handle = tokio::spawn(async move {
                    if let Err(e) = table_display.start_display().await {
                        error!("表格显示模块错误: {}", e);
                    }
                });
                
                info!("正在启动事件监听模块");
                let listener_handle = tokio::spawn(async move {
                    if let Err(e) = event_listener.start_listening().await {
                        error!("事件监听模块错误: {}", e);
                    }
                });
                
                info!("实时监控系统已启动，两个模块正在运行");
                
                // 等待任一模块完成（通常是用户按Ctrl+C退出）
                tokio::select! {
                    _ = display_handle => {
                        info!("表格显示模块已退出");
                    }
                    _ = listener_handle => {
                        info!("事件监听模块已退出");
                    }
                }
                
                // 表格显示结束后，切换回终端模式
                LogAdapter::switch_to_terminal();
                info!("已切换回终端日志模式");
                info!("实时监控系统已停止");
                Ok(())
            }
            Err(e) => {
                error!("创建表格显示模块失败: {}", e);
                Err(e)
            }
        }
    }


    /// 将 PairData 转换为 PairDisplay
    fn convert_pairs_to_display(&self, pairs: &[PairData]) -> Result<Vec<PairDisplay>> {
        // 使用统一的转换工具
        PairDisplayConverter::convert_list(pairs)
    }
}