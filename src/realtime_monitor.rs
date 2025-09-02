use anyhow::Result;
use log::info;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::database::Database;
use crate::event_listener::EventListener;
use crate::table_display::{DisplayMessage, TableDisplay};

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
    
    pub async fn start_monitoring(self, count: usize, interval: u64) -> Result<()> {
        info!("启动模块化实时监控系统...");
        
        // 创建消息通道
        let (sender, receiver) = mpsc::channel::<DisplayMessage>(100);
        
        // 创建表格显示模块
        let mut table_display = TableDisplay::new(receiver)?;
        
        // 创建事件监听模块
        let mut event_listener = EventListener::new(
            self.database.clone(),
            sender,
            count,
            Duration::from_secs(interval),
        ).await;
        
        // 启动两个模块
        let display_handle = tokio::spawn(async move {
            if let Err(e) = table_display.start_display().await {
                log::error!("表格显示模块错误: {}", e);
            }
        });
        
        let listener_handle = tokio::spawn(async move {
            if let Err(e) = event_listener.start_listening().await {
                log::error!("事件监听模块错误: {}", e);
            }
        });
        
        info!("实时监控系统已启动，两个模块正在运行...");
        
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

}