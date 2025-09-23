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


use crate::data::pair_manager::PairData;
use crate::config::{protocol_types};
use serde_json;

// EventType 枚举已移除，现在直接使用 JSON 格式的原始事件数据

/// 原始事件数据，由 EventListener 发送，业务模块处理
#[derive(Debug, Clone)]
pub struct RawEventData {
    /// 事件类型
    pub event_type: String,
    /// 合约地址
    pub contract_address: String,
    /// 合约信息（协议类型、DEX等）
    pub contract_info: Option<ContractMetadata>,
    /// 原始事件数据
    pub raw_data: serde_json::Value,
    /// 事件时间戳
    pub timestamp: u64,
}

/// 合约元数据
#[derive(Debug, Clone)]
pub struct ContractMetadata {
    pub protocol_type: String,
    pub dex: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ContractInfo {
    pub address: H160,
    pub protocol_type: String, // protocol_types::AMM_V2 or protocol_types::AMM_V3
    pub dex: String,      // dex_types::UNISWAP_V2, dex_types::UNISWAP_V3, etc.
}

pub struct EventListener {
    sender: mpsc::Sender<RawEventData>,
    provider: Option<Arc<Provider<ethers::providers::Ws>>>,
    contracts: HashMap<String, ContractInfo>,
}

impl EventListener {
    pub async fn new(
        sender: mpsc::Sender<RawEventData>,
    ) -> Self {
        info!("正在创建EventListener实例...");
        
        // 尝试连接到以太坊WebSocket
        let provider = Self::try_connect_to_ethereum().await;
        
        // 初始化空的合约映射，稍后通过方法添加
        let contracts = HashMap::new();
        info!("EventListener实例创建完成，等待添加合约监听");

        Self {
            sender,
            provider,
            contracts,
        }
    }

    /// 从PairData批量添加要监听的合约
    pub fn add_pair(&mut self, pair: PairData) -> Result<()> {
        let pair_name = format!("{}-{}", pair.token0.symbol, pair.token1.symbol);
        if let Ok(address) = pair.id.parse::<H160>() {
            let contract_info = ContractInfo {
                address,
                protocol_type: pair.protocol_type.clone(),
                dex: pair.dex.clone(),
            };
            info!("已添加交易对合约监听: {} -> {} ({})", pair_name, pair.id, pair.protocol_type);
            self.contracts.insert(pair_name, contract_info);
        } else {
            warn!("无效的交易对地址: {}", pair.id);
        }
        Ok(())
    }

    pub fn add_pairs(&mut self, pairs: Vec<PairData>) -> Result<()> {
        for pair in pairs {
            self.add_pair(pair)?;
        }
        Ok(())
    }
    
    /// 添加要监听的DEX合约地址
    pub fn add_contract(&mut self, name: String, address: &str, protocol_type: String, dex_type: String) -> Result<()> {
        let parsed_address: H160 = address.parse()
            .map_err(|e| anyhow::anyhow!("无效的合约地址 {}: {}", address, e))?;
        
        let contract_info = ContractInfo {
            address: parsed_address,
            protocol_type: protocol_type.clone(),
            dex: dex_type.clone(),
        };
        
        self.contracts.insert(name.clone(), contract_info);
        info!("已添加合约监听: {} -> {} ({})", name, address, protocol_type);
        Ok(())
    }
    
    /// 批量添加合约地址（需要指定协议类型）
    pub fn add_contracts(&mut self, contracts: HashMap<String, (String, String, String)>) -> Result<()> {
        for (name, (address, protocol_type, dex_type)) in contracts {
            self.add_contract(name, &address, protocol_type, dex_type)?;
        }
        Ok(())
    }

    async fn try_connect_to_ethereum() -> Option<Arc<Provider<ethers::providers::Ws>>> {
        use tokio::time::{timeout, Duration};
        
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
            info!("尝试连接到WebSocket节点: {}", wss_url);
            
            // 为连接添加5秒超时
            let connect_result = timeout(Duration::from_secs(5), async {
                Provider::<ethers::providers::Ws>::connect(&wss_url).await
            }).await;
            
            match connect_result {
                Ok(Ok(provider)) => {
                    // 为测试连接添加3秒超时
                    let test_result = timeout(Duration::from_secs(3), async {
                        provider.get_block_number().await
                    }).await;
                    
                    match test_result {
                        Ok(Ok(_)) => {
                            info!("成功连接到以太坊WebSocket节点: {}", wss_url);
                            return Some(Arc::new(provider));
                        }
                        Ok(Err(e)) => {
                            warn!("WebSocket连接测试失败 {}: {}", wss_url, e);
                        }
                        Err(_) => {
                            warn!("WebSocket连接测试超时: {}", wss_url);
                        }
                    }
                }
                Ok(Err(e)) => {
                    warn!("WebSocket连接失败 {}: {}", wss_url, e);
                }
                Err(_) => {
                    warn!("WebSocket连接超时: {}", wss_url);
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
        
        // 根据合约类型启动相应的监听器
        if !v2_contracts.is_empty() && !v3_contracts.is_empty() {
            // 同时监听V2和V3
            tokio::select! {
                _ = Self::listen_v2_swap_events(v2_contracts, provider.clone(), sender.clone()) => {
                    error!("V2 Swap事件监听意外停止");
                }
                _ = Self::listen_v3_swap_events(v3_contracts, provider.clone(), sender.clone()) => {
                    error!("V3 Swap事件监听意外停止");
                }
            }
        } else if !v2_contracts.is_empty() {
            // 只监听V2
            if let Err(e) = Self::listen_v2_swap_events(v2_contracts, provider.clone(), sender.clone()).await {
                error!("V2 Swap事件监听失败: {}", e);
            }
        } else if !v3_contracts.is_empty() {
            // 只监听V3
            if let Err(e) = Self::listen_v3_swap_events(v3_contracts, provider.clone(), sender.clone()).await {
                error!("V3 Swap事件监听失败: {}", e);
            }
        } else {
            warn!("没有任何合约需要监听，事件监听器将保持运行但不监听任何事件");
            // 保持运行，等待可能的关闭信号
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        }
        
        info!("事件监听器已停止");
        Ok(())
    }
    
    // V2 Swap事件监听
    async fn listen_v2_swap_events(
        contracts: HashMap<String, ContractInfo>,
        provider: Arc<Provider<ethers::providers::Ws>>,
        sender: mpsc::Sender<RawEventData>,
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
            if let Err(e) = Self::process_v2_swap_event(&log, &contracts, &sender).await {
                error!("处理V2 Swap事件失败: {}", e);
            }
        }
        
        Ok(())
    }
    
    // V3 Swap事件监听
    async fn listen_v3_swap_events(
        contracts: HashMap<String, ContractInfo>,
        provider: Arc<Provider<ethers::providers::Ws>>,
        sender: mpsc::Sender<RawEventData>,
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
            if let Err(e) = Self::process_v3_swap_event(&log, &contracts, &sender).await {
                error!("处理V3 Swap事件失败: {}", e);
            }
        }
        
        Ok(())
    }


    
    async fn process_v2_swap_event(
        log: &Log,
        contracts: &HashMap<String, ContractInfo>,
        msg_sender: &mpsc::Sender<RawEventData>,
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
            
            // 创建原始事件数据
            let event_data = serde_json::json!({
                "event_type": "V2SwapEvent",
                "pair_address": format!("{:?}", log.address),
                "sender": format!("{:?}", sender_addr),
                "amount0_in": amount0_in.to_string(),
                "amount1_in": amount1_in.to_string(),
                "amount0_out": amount0_out.to_string(),
                "amount1_out": amount1_out.to_string(),
                "to": format!("{:?}", to),
                "block_number": log.block_number.map(|n| n.as_u64()),
                "transaction_hash": format!("{:?}", log.transaction_hash),
            });
            
            // 获取合约信息
            let contract_info = contracts.values().find(|info| info.address == log.address);
            
            // 发送原始事件数据
            Self::send_raw_event("V2SwapEvent", log.address, contract_info, event_data, msg_sender).await
         } else {
             warn!("V2 Swap事件数据格式不正确: topics={}, data_len={}", log.topics.len(), log.data.len());
             Ok(())
         }
     }
     
     async fn process_v3_swap_event(
          log: &Log,
          contracts: &HashMap<String, ContractInfo>,
          msg_sender: &mpsc::Sender<RawEventData>,
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
             
             // 创建原始事件数据
             let event_data = serde_json::json!({
                 "event_type": "V3SwapEvent",
                 "pair_address": format!("{:?}", log.address),
                 "sender": format!("{:?}", sender_addr),
                 "recipient": format!("{:?}", recipient),
                 "amount0": amount0.to_string(),
                 "amount1": amount1.to_string(),
                 "sqrt_price_x96": sqrt_price_x96.to_string(),
                 "liquidity": liquidity,
                 "tick": tick,
                 "block_number": log.block_number.map(|n| n.as_u64()),
                 "transaction_hash": format!("{:?}", log.transaction_hash),
             });
             
             // 获取合约信息
             let contract_info = contracts.values().find(|info| info.address == log.address);
             
             // 发送原始事件数据
             Self::send_raw_event("V3SwapEvent", log.address, contract_info, event_data, msg_sender).await
         } else {
             warn!("V3 Swap事件数据格式不正确: topics={}, data_len={}", log.topics.len(), log.data.len());
             Ok(())
         }
     }
     
     // 发送原始事件数据
     async fn send_raw_event(
         event_type: &str,
         contract_address: H160,
         contract_info: Option<&ContractInfo>,
         event_data: serde_json::Value,
         msg_sender: &mpsc::Sender<RawEventData>,
     ) -> Result<()> {
         let contract_metadata = contract_info.map(|info| ContractMetadata {
             protocol_type: info.protocol_type.clone(),
             dex: info.dex.clone(),
             name: format!("{:?}", contract_address), // 可以改为更友好的名称
         });

         let raw_event = RawEventData {
             event_type: event_type.to_string(),
             contract_address: format!("{:?}", contract_address),
             contract_info: contract_metadata,
             raw_data: event_data,
             timestamp: std::time::SystemTime::now()
                 .duration_since(std::time::UNIX_EPOCH)
                 .unwrap_or_default()
                 .as_secs(),
         };
         
         if let Err(e) = msg_sender.send(raw_event).await {
             error!("发送原始事件数据失败: {}", e);
         } else {
             debug!("已发送 {} 事件的原始数据", event_type);
         }
         
         Ok(())
     }

    pub async fn shutdown(&self) -> Result<()> {
        info!("正在关闭事件监听器...");
        // EventListener 现在只发送原始事件数据，关闭逻辑由调用方处理
        Ok(())
    }
}