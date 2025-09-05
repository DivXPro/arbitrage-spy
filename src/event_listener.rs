use anyhow::Result;
use log::{error, info, debug, warn};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time;
use ethers::{
    prelude::*,
    providers::{Provider, StreamExt},
    types::{Filter, Log, H160, U256},
};
use std::sync::Arc;
use std::collections::HashMap;
use std::env;

use crate::database::Database;
use crate::price_calculator::PriceCalculator;
use crate::table_display::{DisplayMessage, PairDisplay};
use crate::thegraph::PairData;


#[derive(Debug, Clone)]
pub enum EventType {
    SwapEvent {
        pair_address: H160,
        token0_amount: U256,
        token1_amount: U256,
        price: f64,
    },
    MintEvent {
        pair_address: H160,
        liquidity_added: U256,
    },
    BurnEvent {
        pair_address: H160,
        liquidity_removed: U256,
    },
    PairCreated {
        pair_address: H160,
        token0: H160,
        token1: H160,
    },
}

pub struct EventListener {
    database: Database,
    sender: mpsc::Sender<DisplayMessage>,
    count: usize,
    interval: Duration,
    provider: Option<Arc<Provider<ethers::providers::Ws>>>,
    contracts: HashMap<String, H160>,
}

impl EventListener {
    pub async fn new(
        database: Database,
        sender: mpsc::Sender<DisplayMessage>,
        count: usize,
        interval: Duration,
        initial_pairs: Vec<PairData>,
    ) -> Self {
        // 尝试连接到以太坊节点
        let provider = Self::try_connect_to_ethereum().await;
        
        // 从初始交易对数据中提取合约地址
        let mut contracts = HashMap::new();
        for pair in &initial_pairs {
            let pair_name = format!("{}-{}", pair.token0.symbol, pair.token1.symbol);
            if let Ok(address) = pair.id.parse::<H160>() {
                info!("已添加交易对合约监听: {} -> {}", pair_name, pair.id);
                contracts.insert(pair_name, address);
            } else {
                warn!("无效的交易对地址: {}", pair.id);
            }
        }
        
        let event_listener = Self {
            database,
            sender,
            count,
            interval,
            provider,
            contracts,
        };
        
        event_listener
    }

    
    /// 添加要监听的DEX合约地址
    pub fn add_contract(&mut self, name: String, address: &str) -> Result<()> {
        let parsed_address: H160 = address.parse()
            .map_err(|e| anyhow::anyhow!("无效的合约地址 {}: {}", address, e))?;
        
        self.contracts.insert(name.clone(), parsed_address);
        info!("已添加合约监听: {} -> {}", name, address);
        Ok(())
    }
    
    /// 移除DEX合约地址
    pub fn remove_contract(&mut self, name: &str) -> bool {
        if let Some(address) = self.contracts.remove(name) {
            info!("已移除合约监听: {} -> {:?}", name, address);
            true
        } else {
            warn!("未找到要移除的合约: {}", name);
            false
        }
    }
    
    /// 批量添加合约地址
    pub fn add_contracts(&mut self, contracts: HashMap<String, String>) -> Result<()> {
        for (name, address) in contracts {
            self.add_contract(name, &address)?;
        }
        Ok(())
    }
    
    /// 获取当前监听的所有合约地址
    pub fn get_contracts(&self) -> &HashMap<String, H160> {
        &self.contracts
    }
    
    /// 清空所有合约地址
    pub fn clear_contracts(&mut self) {
        let count = self.contracts.len();
        self.contracts.clear();
        info!("已清空所有合约地址，共移除 {} 个合约", count);
    }

    async fn try_connect_to_ethereum() -> Option<Arc<Provider<ethers::providers::Ws>>> {
        // 从环境变量读取WebSocket端点
        let wss_urls = match env::var("WSS_URLS") {
            Ok(urls_str) => {
                urls_str.split(',').map(|s| s.trim().to_string()).collect::<Vec<String>>()
            },
            Err(_) => {
                warn!("未找到环境变量 WSS_URLS，使用默认WebSocket端点");
                vec![
                    "wss://mainnet.infura.io/ws/v3/".to_string(),
                ]
            }
        };
        
        for wss_url in wss_urls {
            match Provider::<ethers::providers::Ws>::connect(&wss_url).await {
                Ok(provider) => {
                    // 测试连接
                    if let Ok(_) = provider.get_block_number().await {
                        info!("成功连接到以太坊WebSocket节点: {}", wss_url);
                        return Some(Arc::new(provider));
                    }
                }
                Err(e) => {
                    info!("WebSocket连接失败 {}: {}", wss_url, e);
                }
            }
        }
        
        warn!("无法连接到任何以太坊WebSocket节点");
        None
    }
    
    pub async fn start_listening(&mut self) -> Result<()> {
        info!("启动区块链事件监听器...");
        
        let provider = self.provider.as_ref().unwrap().clone();
        
        // 检查是否有配置的合约地址
        if self.contracts.is_empty() {
            warn!("没有配置任何合约地址，加载默认合约");
            // 这里不能调用self的方法，因为self已经被借用了
            // 所以我们直接在这里处理
        }
        
        // 获取所有配置的合约地址
        let contract_addresses: Vec<H160> = self.contracts.values().cloned().collect();
        
        // 创建事件过滤器监听Swap事件，指定合约地址
        let swap_filter = Filter::new()
            .event("Swap(address,uint256,uint256,uint256,uint256,address)")
            .address(contract_addresses.clone())
            .from_block(BlockNumber::Latest);
        
        info!("开始监听区块链事件，监听 {} 个合约地址...", contract_addresses.len());
        for (name, address) in &self.contracts {
            info!("监听合约: {} -> {:?}", name, address);
        }
        
        // 启动事件监听循环
        let sender = self.sender.clone();
        let database = self.database.clone();
        let count = self.count;
        let interval = self.interval;
        
        tokio::select! {
            _ = Self::listen_swap_events_static(provider.clone(), swap_filter, sender.clone(), database.clone(), count) => {
                error!("Swap事件监听意外停止");
            }
            _ = Self::periodic_data_refresh_static(sender.clone(), database.clone(), count, interval) => {
                error!("定期数据刷新意外停止");
            }
        }
        
        info!("事件监听器已停止");
        Ok(())
    }
    
    async fn listen_swap_events_static(
        provider: Arc<Provider<ethers::providers::Ws>>, 
        filter: Filter,
        sender: mpsc::Sender<DisplayMessage>,
        database: Database,
        count: usize
    ) -> Result<()> {
        info!("开始监听Swap事件...");
        
        // 使用WebSocket实时事件流
        let mut stream = provider.subscribe_logs(&filter).await?;
        
        info!("WebSocket事件流已建立，等待Swap事件...");
        
        while let Some(log) = stream.next().await {
             info!("检测到实时Swap事件，合约地址: {:?}", log.address);
             
             // 处理事件
             if let Err(e) = Self::process_swap_event(&log, &database).await {
                 error!("处理Swap事件失败: {}", e);
                 continue;
             }
             
             // 获取更新后的数据并发送
             match Self::fetch_and_process_data_static(&database, count).await {
                 Ok(pairs) => {
                     if !pairs.is_empty() {
                         info!("Swap事件触发数据更新，共 {} 个交易对:", pairs.len());
                         for (i, pair) in pairs.iter().enumerate().take(5) {
                             info!("  {}. {} ({}) - 价格: {} - 流动性: {}", 
                                 i + 1, pair.pair, pair.dex, pair.price, pair.liquidity);
                         }
                         if pairs.len() > 5 {
                             info!("  ... 还有 {} 个交易对", pairs.len() - 5);
                         }
                         
                         if let Err(e) = sender.send(DisplayMessage::UpdateData(pairs)).await {
                             error!("发送Swap事件更新失败: {}", e);
                             break;
                         }
                         debug!("实时Swap事件触发的数据更新已推送");
                     }
                 }
                 Err(e) => {
                     error!("获取Swap事件后的数据失败: {}", e);
                 }
             }
         }
        
        warn!("WebSocket事件流已结束");
        Ok(())
    }
    
    async fn process_swap_event(log: &Log, database: &Database) -> Result<()> {
        // 解析Swap事件的具体数据
        // 这里可以根据不同DEX的Swap事件格式进行解析
        debug!("处理来自合约 {:?} 的Swap事件", log.address);
        
        // TODO: 实现具体的事件解析逻辑
        // 1. 解析事件参数 (token amounts, addresses等)
        // 2. 计算价格变化
        // 3. 更新数据库中的价格信息
        
        Ok(())
    }

    async fn periodic_data_refresh_static(
        sender: mpsc::Sender<DisplayMessage>,
        database: Database,
        count: usize,
        interval: Duration
    ) -> Result<()> {
        info!("启动定期数据刷新...");
        
        let mut interval_timer = time::interval(interval);
        
        loop {
            interval_timer.tick().await;
            
            match Self::fetch_and_process_data_static(&database, count).await {
                Ok(pairs) => {
                    if !pairs.is_empty() {
                        if let Err(e) = sender.send(DisplayMessage::UpdateData(pairs)).await {
                            error!("发送定期刷新数据失败: {}", e);
                            break;
                        }
                        debug!("定期数据刷新完成");
                    }
                }
                Err(e) => {
                    error!("定期数据刷新失败: {}", e);
                }
            }
        }
        
        Ok(())
    }
    
    async fn fetch_and_process_data_static(database: &Database, count: usize) -> Result<Vec<PairDisplay>> {
        // 从数据库获取最新的交易对数据
        let pairs = database.get_top_pairs(count)?;
        
        // 转换为显示格式
        let display_pairs: Vec<PairDisplay> = pairs
            .into_iter()
            .enumerate()
            .map(|(index, pair)| {
                PairDisplay {
                    rank: index + 1,
                    pair: format!("{}/{}", pair.token0.symbol, pair.token1.symbol),
                    dex: pair.dex_type.clone(),
                    price: match PriceCalculator::calculate_price(&pair.reserve0, &pair.reserve1) {
                        Ok(price) => PriceCalculator::format_price(&price),
                        Err(_) => "_".to_string(),
                    },
                    liquidity: format!("${:.0}", pair.reserve_usd.parse::<f64>().unwrap_or(0.0)),
                    last_update: chrono::Utc::now().format("%H:%M:%S").to_string(),
                }
            })
            .collect();
        
        Ok(display_pairs)
    }
    
    async fn fetch_and_process_data(&self) -> Result<Vec<PairDisplay>> {
        // 从数据库获取最新的交易对数据
        let pairs = self.database.get_top_pairs(self.count)?;
        
        // 转换为显示格式
        let display_pairs: Vec<PairDisplay> = pairs
            .into_iter()
            .enumerate()
            .map(|(index, pair)| {
                PairDisplay {
                    rank: index + 1,
                    pair: format!("{}/{}", pair.token0.symbol, pair.token1.symbol),
                    dex: pair.dex_type.clone(),
                    price: match PriceCalculator::calculate_price(&pair.reserve0, &pair.reserve1) {
                        Ok(price) => PriceCalculator::format_price(&price),
                        Err(_) => "_".to_string(),
                    },
                    liquidity: format!("${:.0}", pair.reserve_usd.parse::<f64>().unwrap_or(0.0)),
                    last_update: chrono::Utc::now().format("%H:%M:%S").to_string(),
                }
            })
            .collect();
        
        Ok(display_pairs)
    }
    
    async fn handle_price_update_event(&self) -> Result<()> {
        // 获取最新数据并推送更新
        let pairs = self.fetch_and_process_data().await?;
        if !pairs.is_empty() {
            self.sender.send(DisplayMessage::UpdateData(pairs)).await
                .map_err(|e| anyhow::anyhow!("发送价格更新消息失败: {}", e))?;
        }
        Ok(())
    }
    
    async fn handle_liquidity_change_event(&self) -> Result<()> {
        // 获取最新数据并推送更新
        let pairs = self.fetch_and_process_data().await?;
        if !pairs.is_empty() {
            self.sender.send(DisplayMessage::UpdateData(pairs)).await
                .map_err(|e| anyhow::anyhow!("发送流动性更新消息失败: {}", e))?;
        }
        Ok(())
    }
    
    async fn handle_new_pair_event(&self) -> Result<()> {
        // 获取最新数据并推送更新
        let pairs = self.fetch_and_process_data().await?;
        if !pairs.is_empty() {
            self.sender.send(DisplayMessage::UpdateData(pairs)).await
                .map_err(|e| anyhow::anyhow!("发送新交易对消息失败: {}", e))?;
        }
        Ok(())
    }
    
    pub async fn shutdown(&self) -> Result<()> {
        info!("正在关闭事件监听器...");
        self.sender.send(DisplayMessage::Shutdown).await
            .map_err(|e| anyhow::anyhow!("发送关闭消息失败: {}", e))?;
        Ok(())
    }
}