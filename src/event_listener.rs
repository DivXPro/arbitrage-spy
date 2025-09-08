use anyhow::Result;
use log::{error, info, debug, warn};
use tokio::sync::mpsc;
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
use crate::table_display::{DisplayMessage, PairDisplay, PairDisplayConverter};
use crate::thegraph::PairData;
use chrono;

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
    provider: Option<Arc<Provider<ethers::providers::Ws>>>,
    contracts: HashMap<String, H160>,
    pairs: Vec<PairData>,
}

impl EventListener {
    pub async fn new(
        database: Database,
        sender: mpsc::Sender<DisplayMessage>,
        count: usize,
        initial_pairs: Vec<PairData>,
    ) -> Self {
        // 尝试连接到以太坊节点
        let provider = Self::try_connect_to_ethereum().await;
        
        // 从初始交易对数据中提取合约地址
        let mut contracts: HashMap<String, H160> = HashMap::new();
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
            database: database.clone(),
            sender,
            count,
            provider,
            contracts,
            pairs: initial_pairs,
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
        
        tokio::select! {
            _ = Self::listen_swap_events_static(self.contracts.clone(), provider.clone(), swap_filter, sender.clone(), self.pairs.clone()) => {
                error!("Swap事件监听意外停止");
            }
        }
        
        info!("事件监听器已停止");
        Ok(())
    }
    
    async fn listen_swap_events_static(
        contracts: HashMap<String, H160>,
        provider: Arc<Provider<ethers::providers::Ws>>, 
        filter: Filter,
        sender: mpsc::Sender<DisplayMessage>,
        pairs: Vec<PairData>
    ) -> Result<()> {
        info!("开始监听Swap事件...");
        
        // 使用WebSocket实时事件流
        let mut stream = provider.subscribe_logs(&filter).await?;
        
        info!("WebSocket事件流已建立，等待Swap事件...");
        
        while let Some(log) = stream.next().await {
            let contract_name = contracts.iter().find(|(_, addr)| **addr == log.address)
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| format!("{:?}", log.address));
            info!("检测到实时Swap事件，合约: {}", contract_name);
             
             // 处理事件并获取局部更新数据
             match Self::process_swap_event(&log, &sender, &contracts, pairs.clone()).await {
                 Ok(_) => {
                     info!("成功处理Swap事件并发送局部更新");
                     debug!("实时Swap事件触发的数据更新已推送");
                 }
                 Err(e) => {
                     error!("处理Swap事件失败: {}", e);
                     continue;
                 }
             }
         }
        
        warn!("WebSocket事件流已结束");
        Ok(())
    }
    
    async fn process_swap_event(
        log: &Log, 
        sender: &mpsc::Sender<DisplayMessage>, 
        contracts: &HashMap<String, H160>,
        pairs: Vec<PairData>,
    ) -> Result<()> {
        // 查找合约名称
        let contract_name = contracts.iter()
            .find(|(_, addr)| **addr == log.address)
            .map(|(name, _)| name.clone())
            .unwrap_or_else(|| format!("{:?}", log.address));
        
        // 解析Swap事件的具体数据
        debug!("处理来自合约 {} 的Swap事件", contract_name);
                
        // 查找与事件相关的交易对索引
         if let Some((index, pair)) = pairs.iter().enumerate()
             .find(|(_, pair)| {
                 // 这里需要根据实际情况匹配交易对
                 // 可以通过合约地址或其他标识符来匹配
                 if let Ok(pair_address) = pair.id.parse::<H160>() {
                     pair_address == log.address
                 } else {
                     false
                 }
             }) {
             
             let pair_name = format!("{}/{}", pair.token0.symbol, pair.token1.symbol);
             
             // 将 PairData 转换为 PairDisplay（使用统一的转换工具）
             let updated_pair_display = PairDisplayConverter::convert_for_event(pair, index + 1);
             
             // 发送局部更新消息
             if let Err(e) = sender.send(DisplayMessage::PartialUpdate { 
                 index, 
                 data: updated_pair_display 
             }).await {
                 error!("发送局部更新失败: {}", e);
             } else {
                 info!("已发送交易对 {} 的局部更新 (索引: {})", pair_name, index);
             }
        } else {
            // 如果找不到对应的交易对，记录警告
            warn!("未找到与合约地址 {:?} 对应的交易对", log.address);
        }
        
        Ok(())
    }

    async fn fetch_and_process_data_static(database: &Database, count: usize) -> Result<Vec<PairDisplay>> {
        // 从数据库获取最新的交易对数据
        let pairs = database.get_top_pairs(count)?;
        
        // 转换为显示格式（使用统一的转换工具）
        let display_pairs = PairDisplayConverter::convert_owned(pairs)?;
        
        Ok(display_pairs)
    }
    
    async fn fetch_and_process_data(&self) -> Result<Vec<PairDisplay>> {
        Self::fetch_and_process_data_static(&self.database, self.count).await
    }
    
    /// 更新缓存数据
    async fn update_cache(&self) -> Result<()> {
        // 这个方法暂时不需要实现，因为我们直接使用pairs字段
        Ok(())
    }
    
    async fn handle_price_update_event(&self) -> Result<()> {
        // 获取最新数据并推送更新
        let pairs = self.fetch_and_process_data().await?;
        if !pairs.is_empty() {
            self.sender.send(DisplayMessage::FullUpdate(pairs)).await
                .map_err(|e| anyhow::anyhow!("发送价格更新消息失败: {}", e))?;
        }
        Ok(())
    }
    
    async fn handle_liquidity_change_event(&self) -> Result<()> {
        // 获取最新数据并推送更新
        let pairs = self.fetch_and_process_data().await?;
        if !pairs.is_empty() {
            self.sender.send(DisplayMessage::FullUpdate(pairs)).await
                .map_err(|e| anyhow::anyhow!("发送流动性更新消息失败: {}", e))?;
        }
        Ok(())
    }
    
    async fn handle_new_pair_event(&self) -> Result<()> {
        // 获取最新数据并推送更新
        let pairs = self.fetch_and_process_data().await?;
        if !pairs.is_empty() {
            self.sender.send(DisplayMessage::FullUpdate(pairs)).await
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