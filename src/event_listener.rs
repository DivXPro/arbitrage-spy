use anyhow::Result;
use log::{error, info, debug, warn};
use tokio::sync::mpsc;
use ethers::{
    prelude::*,
    providers::{Provider, StreamExt},
    types::{Filter, Log, H160, U256, I256},
};
use std::sync::Arc;
use std::collections::HashMap;
use std::env;

use crate::database::Database;
use crate::price_calculator::PriceCalculator;
use crate::table_display::{DisplayMessage, PairDisplay, PairDisplayConverter};
use crate::thegraph::PairData;
use crate::config::{protocol_types, dex_types};
use chrono;

#[derive(Debug, Clone)]
pub enum EventType {
    V2SwapEvent {
        pair_address: H160,
        sender: H160,
        amount0_in: U256,
        amount1_in: U256,
        amount0_out: U256,
        amount1_out: U256,
        to: H160,
    },
    V3SwapEvent {
        pair_address: H160,
        sender: H160,
        recipient: H160,
        amount0: I256,
        amount1: I256,
        sqrt_price_x96: U256,
        liquidity: u128,
        tick: i32,
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

#[derive(Debug, Clone)]
pub struct ContractInfo {
    pub address: H160,
    pub protocol_type: String, // protocol_types::AMM_V2 or protocol_types::AMM_V3
    pub dex_type: String,      // dex_types::UNISWAP_V2, dex_types::UNISWAP_V3, etc.
}

pub struct EventListener {
    database: Database,
    sender: mpsc::Sender<DisplayMessage>,
    count: usize,
    provider: Option<Arc<Provider<ethers::providers::Ws>>>,
    contracts: HashMap<String, ContractInfo>,
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
        
        // 从初始交易对数据中提取合约地址和协议信息
        let mut contracts: HashMap<String, ContractInfo> = HashMap::new();
        info!("开始从 {} 个初始交易对中提取合约地址", initial_pairs.len());
        for pair in &initial_pairs {
            let pair_name = format!("{}-{}", pair.token0.symbol, pair.token1.symbol);
            if let Ok(address) = pair.id.parse::<H160>() {
                let contract_info = ContractInfo {
                    address,
                    protocol_type: pair.protocol_type.clone(),
                    dex_type: pair.dex_type.clone(),
                };
                info!("已添加交易对合约监听: {} -> {} ({})", 
                      pair_name, pair.id, pair.protocol_type);
                contracts.insert(pair_name, contract_info);
            } else {
                warn!("无效的交易对地址: {}", pair.id);
            }
        }
        info!("合约地址提取完成，共添加 {} 个合约", contracts.len());
        

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
    pub fn add_contract(&mut self, name: String, address: &str, protocol_type: String, dex_type: String) -> Result<()> {
        let parsed_address: H160 = address.parse()
            .map_err(|e| anyhow::anyhow!("无效的合约地址 {}: {}", address, e))?;
        
        let contract_info = ContractInfo {
            address: parsed_address,
            protocol_type: protocol_type.clone(),
            dex_type: dex_type.clone(),
        };
        
        self.contracts.insert(name.clone(), contract_info);
        info!("已添加合约监听: {} -> {} ({})", name, address, protocol_type);
        Ok(())
    }
    
    /// 移除DEX合约地址
    pub fn remove_contract(&mut self, name: &str) -> bool {
        if let Some(contract_info) = self.contracts.remove(name) {
            info!("已移除合约监听: {} -> {:?} ({})", name, contract_info.address, contract_info.protocol_type);
            true
        } else {
            warn!("未找到要移除的合约: {}", name);
            false
        }
    }
    
    /// 批量添加合约地址（需要指定协议类型）
    pub fn add_contracts(&mut self, contracts: HashMap<String, (String, String, String)>) -> Result<()> {
        for (name, (address, protocol_type, dex_type)) in contracts {
            self.add_contract(name, &address, protocol_type, dex_type)?;
        }
        Ok(())
    }
    
    /// 获取所有合约信息
    pub fn get_contracts(&self) -> &HashMap<String, ContractInfo> {
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
        
        // 检查WebSocket连接状态
        if self.provider.is_none() {
            error!("WebSocket连接未建立，无法启动事件监听");
            return Ok(());
        }
        
        let provider = self.provider.as_ref().unwrap().clone();
        
        // 检查是否有配置的合约地址
        info!("检查合约地址配置: 当前有 {} 个合约", self.contracts.len());
        if self.contracts.is_empty() {
            warn!("没有配置任何合约地址，事件监听器将退出");
            return Ok(());
        }
        
        // 分离v2和v3合约
        let mut v2_contracts = HashMap::new();
        let mut v3_contracts = HashMap::new();
        
        for (name, contract_info) in &self.contracts {
            if contract_info.protocol_type == protocol_types::AMM_V2 {
                v2_contracts.insert(name.clone(), contract_info.clone());
            } else if contract_info.protocol_type == protocol_types::AMM_V3 {
                v3_contracts.insert(name.clone(), contract_info.clone());
            }
        }
        
        info!("分离合约: V2={} 个, V3={} 个", v2_contracts.len(), v3_contracts.len());
        
        // 启动事件监听循环
        let sender = self.sender.clone();
        let pairs = self.pairs.clone();
        
        tokio::select! {
            _ = Self::listen_v2_swap_events(v2_contracts, provider.clone(), sender.clone(), pairs.clone()) => {
                error!("V2 Swap事件监听意外停止");
            }
            _ = Self::listen_v3_swap_events(v3_contracts, provider.clone(), sender.clone(), pairs.clone()) => {
                error!("V3 Swap事件监听意外停止");
            }
        }
        
        info!("事件监听器已停止");
        Ok(())
    }
    
    // V2 Swap事件监听
    async fn listen_v2_swap_events(
        contracts: HashMap<String, ContractInfo>,
        provider: Arc<Provider<ethers::providers::Ws>>,
        sender: mpsc::Sender<DisplayMessage>,
        pairs: Vec<PairData>,
    ) -> Result<()> {
        if contracts.is_empty() {
            info!("没有V2合约需要监听");
            return Ok(());
        }
        
        let contract_addresses: Vec<H160> = contracts.values().map(|c| c.address).collect();
        
        // V2 Swap事件签名: Swap(address,uint256,uint256,uint256,uint256,address)
        let v2_filter = Filter::new()
            .event("Swap(address,uint256,uint256,uint256,uint256,address)")
            .address(contract_addresses.clone())
            .from_block(BlockNumber::Latest);
        
        info!("开始监听V2 Swap事件，监听 {} 个合约...", contract_addresses.len());
        for (name, contract_info) in &contracts {
            info!("V2合约: {} -> {:?}", name, contract_info.address);
        }
        
        let mut stream = provider.subscribe_logs(&v2_filter).await?;
        
        while let Some(log) = stream.next().await {
            if let Err(e) = Self::process_v2_swap_event(&log, &contracts, &sender, &pairs).await {
                error!("处理V2 Swap事件失败: {}", e);
            }
        }
        
        Ok(())
    }
    
    // V3 Swap事件监听
    async fn listen_v3_swap_events(
        contracts: HashMap<String, ContractInfo>,
        provider: Arc<Provider<ethers::providers::Ws>>,
        sender: mpsc::Sender<DisplayMessage>,
        pairs: Vec<PairData>,
    ) -> Result<()> {
        if contracts.is_empty() {
            info!("没有V3合约需要监听");
            return Ok(());
        }
        
        let contract_addresses: Vec<H160> = contracts.values().map(|c| c.address).collect();
        
        // V3 Swap事件签名: Swap(address,address,int256,int256,uint160,uint128,int24)
        let v3_filter = Filter::new()
            .event("Swap(address,address,int256,int256,uint160,uint128,int24)")
            .address(contract_addresses.clone())
            .from_block(BlockNumber::Latest);
        
        info!("开始监听V3 Swap事件，监听 {} 个合约...", contract_addresses.len());
        for (name, contract_info) in &contracts {
            info!("V3合约: {} -> {:?}", name, contract_info.address);
        }
        
        let mut stream = provider.subscribe_logs(&v3_filter).await?;
        
        while let Some(log) = stream.next().await {
            if let Err(e) = Self::process_v3_swap_event(&log, &contracts, &sender, &pairs).await {
                error!("处理V3 Swap事件失败: {}", e);
            }
        }
        
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
    
    async fn process_v2_swap_event(
        log: &Log,
        contracts: &HashMap<String, ContractInfo>,
        msg_sender: &mpsc::Sender<DisplayMessage>,
        pairs: &Vec<PairData>,
    ) -> Result<()> {
        let contract_name = contracts.iter()
            .find(|(_, contract_info)| contract_info.address == log.address)
            .map(|(name, _)| name.clone())
            .unwrap_or_else(|| format!("{:?}", log.address));
        
        debug!("处理V2 Swap事件，合约: {}", contract_name);
        
        // V2 Swap事件结构: Swap(address indexed sender, uint amount0In, uint amount1In, uint amount0Out, uint amount1Out, address indexed to)
        if log.topics.len() >= 3 && log.data.len() >= 128 {
            // 解析V2事件数据
            let sender_addr = H160::from(log.topics[1]);
            let to = H160::from(log.topics[2]);
            
            // 解析数据字段 (每个uint256占32字节)
            let amount0_in = U256::from_big_endian(&log.data[0..32]);
            let amount1_in = U256::from_big_endian(&log.data[32..64]);
            let amount0_out = U256::from_big_endian(&log.data[64..96]);
            let amount1_out = U256::from_big_endian(&log.data[96..128]);
            
            info!("V2 Swap: sender={:?}, to={:?}, amount0In={}, amount1In={}, amount0Out={}, amount1Out={}", 
                  sender_addr, to, amount0_in, amount1_in, amount0_out, amount1_out);
            
            // 创建V2SwapEvent
            let swap_event = EventType::V2SwapEvent {
                pair_address: log.address,
                sender: sender_addr,
                amount0_in,
                amount1_in,
                amount0_out,
                amount1_out,
                to,
            };
            
            // 处理事件并发送更新
             Self::handle_swap_event_update(swap_event, msg_sender, pairs).await
         } else {
             warn!("V2 Swap事件数据格式不正确: topics={}, data_len={}", log.topics.len(), log.data.len());
             Ok(())
         }
     }
     
     async fn process_v3_swap_event(
          log: &Log,
          contracts: &HashMap<String, ContractInfo>,
          msg_sender: &mpsc::Sender<DisplayMessage>,
          pairs: &Vec<PairData>,
      ) -> Result<()> {
         let contract_name = contracts.iter()
             .find(|(_, contract_info)| contract_info.address == log.address)
             .map(|(name, _)| name.clone())
             .unwrap_or_else(|| format!("{:?}", log.address));
         
         debug!("处理V3 Swap事件，合约: {}", contract_name);
         
         // V3 Swap事件结构: Swap(address indexed sender, address indexed recipient, int256 amount0, int256 amount1, uint160 sqrtPriceX96, uint128 liquidity, int24 tick)
         if log.topics.len() >= 3 && log.data.len() >= 160 {
             // 解析V3事件数据
             let sender_addr = H160::from(log.topics[1]);
             let recipient = H160::from(log.topics[2]);
             
             // 解析数据字段
             let amount0 = I256::from_raw(U256::from_big_endian(&log.data[0..32]));
             let amount1 = I256::from_raw(U256::from_big_endian(&log.data[32..64]));
             let sqrt_price_x96 = U256::from_big_endian(&log.data[64..96]);
             let liquidity = u128::from_be_bytes({
                 let mut bytes = [0u8; 16];
                 bytes.copy_from_slice(&log.data[96..112]);
                 bytes
             });
             let tick = i32::from_be_bytes({
                 let mut bytes = [0u8; 4];
                 bytes.copy_from_slice(&log.data[156..160]);
                 bytes
             });
             
             info!("V3 Swap: sender={:?}, recipient={:?}, amount0={}, amount1={}, sqrtPriceX96={}, liquidity={}, tick={}", 
                   sender_addr, recipient, amount0, amount1, sqrt_price_x96, liquidity, tick);
             
             // 创建V3SwapEvent
             let swap_event = EventType::V3SwapEvent {
                 pair_address: log.address,
                 sender: sender_addr,
                 recipient,
                 amount0,
                 amount1,
                 sqrt_price_x96,
                 liquidity,
                 tick,
             };
             
             // 处理事件并发送更新
              Self::handle_swap_event_update(swap_event, msg_sender, pairs).await
         } else {
             warn!("V3 Swap事件数据格式不正确: topics={}, data_len={}", log.topics.len(), log.data.len());
             Ok(())
         }
     }
     
     // 通用的事件更新处理方法
     async fn handle_swap_event_update(
         swap_event: EventType,
         msg_sender: &mpsc::Sender<DisplayMessage>,
         pairs: &Vec<PairData>,
     ) -> Result<()> {
         // 根据事件类型获取交易对地址
         let pair_address = match &swap_event {
             EventType::V2SwapEvent { pair_address, .. } => *pair_address,
             EventType::V3SwapEvent { pair_address, .. } => *pair_address,
             _ => return Ok(()),
         };
         
         // 查找对应的交易对数据
         if let Some((index, pair)) = pairs.iter().enumerate()
             .find(|(_, pair)| {
                 if let Ok(addr) = pair.id.parse::<H160>() {
                     addr == pair_address
                 } else {
                     false
                 }
             }) {
             
             let pair_name = format!("{}/{}", pair.token0.symbol, pair.token1.symbol);
             debug!("找到匹配的交易对: {} (索引: {})", pair_name, index);
             
             // 将 PairData 转换为 PairDisplay
             let pair_display = PairDisplayConverter::convert_for_event(pair, index + 1);
             
             // 发送局部更新消息
             let message = DisplayMessage::PartialUpdate {
                 index,
                 data: pair_display,
             };
             
             if let Err(e) = msg_sender.send(message).await {
                 error!("发送局部更新消息失败: {}", e);
             } else {
                 debug!("已发送交易对 {} 的局部更新", pair_name);
             }
         } else {
             debug!("未找到匹配的交易对，地址: {:?}", pair_address);
         }
         
         Ok(())
     }

    async fn process_swap_event(
        log: &Log, 
        sender: &mpsc::Sender<DisplayMessage>, 
        contracts: &HashMap<String, H160>,
        pairs: Vec<PairData>,
    ) -> Result<()> {
        Self::process_swap_event_common(log, sender, &HashMap::new(), &pairs).await
    }
    
    async fn process_swap_event_common(
        log: &Log, 
        sender: &mpsc::Sender<DisplayMessage>, 
        contracts: &HashMap<String, ContractInfo>,
        pairs: &Vec<PairData>,
    ) -> Result<()> {
        // 查找合约名称
        let contract_name = contracts.iter()
            .find(|(_, contract_info)| contract_info.address == log.address)
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
        let pair_manager = crate::pairs::PairManager::new(&database);
        let pairs = pair_manager.load_pairs_by_filter(None, None, Some(count))?;
        
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