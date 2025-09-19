use anyhow::Result;
use log::info;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::database::Database;
use crate::event_listener::EventListener;
use crate::table_display::{DisplayMessage, TableDisplay, PairDisplay, PairDisplayConverter};
use crate::thegraph::PairData;

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
        println!("启动模块化实时监控系统...");
        
        // 创建消息通道
        let (sender, receiver) = mpsc::channel::<DisplayMessage>(100);
        println!("消息通道创建完成");
        
        // 准备初始数据
        println!("正在获取初始交易对数据...");
        let pair_manager = crate::pairs::PairManager::new(&self.database);
        let initial_pairs = pair_manager.load_pairs_by_filter(None, Some("UNI_V3"), Some(count.min(10)))?;
        println!("获取到 {} 个初始交易对", initial_pairs.len());
        let initial_data = self.convert_pairs_to_display(&initial_pairs)?;
        println!("初始数据转换完成");
        
        // 创建表格显示模块
        println!("正在创建表格显示模块...");
        let mut table_display = TableDisplay::new(receiver, initial_data)?;
        println!("表格显示模块创建完成");
        
        // 创建事件监听模块，传递初始交易对数据
        println!("正在创建事件监听模块...");
        let mut event_listener = EventListener::new(
            self.database.clone(),
            sender,
            count,
            initial_pairs,
        ).await;
        println!("事件监听模块创建完成");
        
        // 启动两个模块
        println!("正在启动表格显示模块...");
        let display_handle = tokio::spawn(async move {
            if let Err(e) = table_display.start_display().await {
                println!("表格显示模块错误: {}", e);
            }
        });
        
        println!("正在启动事件监听模块...");
        let listener_handle = tokio::spawn(async move {
            if let Err(e) = event_listener.start_listening().await {
                println!("事件监听模块错误: {}", e);
            }
        });
        
        println!("实时监控系统已启动，两个模块正在运行...");
        
        // 等待任一模块完成（通常是用户按Ctrl+C退出）
        tokio::select! {
            _ = display_handle => {
                info!("表格显示模块已退出");
            }
            _ = listener_handle => {
                info!("事件监听模块已退出");
            }
        }
        
        info!("实时监控系统已停止");
        Ok(())
    }


    /// 将 PairData 转换为 PairDisplay
    fn convert_pairs_to_display(&self, pairs: &[PairData]) -> Result<Vec<PairDisplay>> {
        // 使用统一的转换工具
        PairDisplayConverter::convert_list(pairs)
    }
}