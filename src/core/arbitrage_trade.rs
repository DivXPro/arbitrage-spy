use std::sync::{Arc, Mutex};

use anyhow::Result;
use log::{info, warn, debug};
use crate::core::exchange_graph::ExchangeGraph;
use crate::store::database::Database;
use crate::store::pair_manager::{PairManager, PairData};
use crate::store::blockchain_client::{BlockchainClient};
use crate::store::uniswap_v3_client::UniswapV3Client;
use crate::config::{dex_types};
use crate::event_listener::EventListener;
use ethers::prelude::*;

/// 套利交易相关功能
pub struct ArbitrageTrade {
    pub event_listener: EventListener,
}

impl ArbitrageTrade {
    pub fn new(event_listener: EventListener) -> Self {
        Self { event_listener }
    }
    
    /// 通过链上数据更新PairData的最新信息
    pub async fn update_pairs_from_blockchain(
        pairs: &[PairData],
        graph: &Arc<Mutex<ExchangeGraph>>
    ) -> Result<()> {
        info!("开始从链上更新 {} 个交易对的数据...", pairs.len());
        
        // 创建区块链客户端
        let blockchain_client = Arc::new(BlockchainClient::ethereum().await?);
        let v3_client = UniswapV3Client::new(blockchain_client.clone())?;
        
        let mut updated_count = 0;
        let mut error_count = 0;
        
        for pair in pairs.iter() {
            // 只更新V3交易对
            if pair.dex != dex_types::UNISWAP_V3 {
                debug!("跳过非V3交易对: {} ({})", pair.id, pair.dex);
                continue;
            }
            
            match Self::update_pair_from_blockchain(pair, graph, &v3_client).await {
                Ok(_) => {
                    updated_count += 1;
                    debug!("成功更新交易对: {} ({}/{})", 
                           pair.id, pair.token0.symbol, pair.token1.symbol);
                }
                Err(e) => {
                    error_count += 1;
                    warn!("更新交易对 {} 失败: {}", pair.id, e);
                }
            }
        }
        
        info!("链上数据更新完成: 成功 {}, 失败 {}", updated_count, error_count);
        Ok(())
    }
    
    /// 更新单个交易对的链上数据
    async fn update_pair_from_blockchain(
        pair: &PairData,
        graph: &Arc<Mutex<ExchangeGraph>>,
        v3_client: &UniswapV3Client
    ) -> Result<()> {
        // 解析池地址
        let pool_address = pair.id.parse::<Address>()
            .map_err(|e| anyhow::anyhow!("无效的池地址 {}: {}", pair.id, e))?;
        
        // 获取池的最新信息
        let pool_info = v3_client.get_pool_info(pool_address).await?;
        
        // 创建更新后的PairData
        let mut updated_pair = pair.clone();
        let sqrt_price_value = pool_info.sqrt_price_x96.clone();
        let tick_value = pool_info.tick;
        let liquidity_value = pool_info.liquidity.clone();
        
        updated_pair.sqrt_price = Some(sqrt_price_value.clone());
        updated_pair.tick = Some(tick_value.to_string());
        
        // 计算并更新流动性相关数据
        // 注意：这里简化处理，实际应该根据sqrt_price和liquidity计算储备量
        if let Ok(liquidity_f64) = liquidity_value.parse::<f64>() {
            // 简化的流动性转换为USD价值
            let estimated_usd_value = liquidity_f64 / 1e18 * 2000.0; // 假设平均代币价格
            updated_pair.reserve_usd = estimated_usd_value.to_string();
        }
        
        debug!("更新交易对 {} 的链上数据: sqrt_price={}, tick={}, liquidity={}", 
               pair.id, 
               sqrt_price_value, 
               tick_value, 
               liquidity_value);
        
        // 使用 ExchangeGraph 的 update_pair_data 方法更新数据
        {
            let mut graph_guard = graph.lock().unwrap();
            graph_guard.update_pair_data_once(&updated_pair)?;
        }
        
        Ok(())
    }

    /// 从数据库读取V3交易对并构建ExchangeGraph
    pub async fn build_v3_exchange_graph(database: &Database) -> Result<Arc<Mutex<ExchangeGraph>>> {
        info!("开始从数据库读取V3交易对数据...");

        // 读取V3交易对数据
        let pair_manager = PairManager::new(database);
        let v3_pairs = pair_manager.load_pairs_by_filter(
            None,  // network
            Some(dex_types::UNISWAP_V3),  // dex_type
            None,  // limit
        )?;
        
        // 创建EventListener实例
        let event_listener = EventListener::new().await;
        
        if v3_pairs.is_empty() {
            warn!("数据库中没有找到V3交易对数据");
            let graph = Arc::new(Mutex::new(ExchangeGraph::new(Arc::new(Mutex::new(event_listener)), None)?));
            return Ok(graph);
        }
        
        info!("从数据库读取到 {} 个V3交易对", v3_pairs.len());
        
        // 使用ExchangeGraph的new构造函数创建图，直接传入pairs参数
        let exchange_graph = ExchangeGraph::new(Arc::new(Mutex::new(event_listener)), Some(&v3_pairs))?;
        let graph = Arc::new(Mutex::new(exchange_graph));
        
        // 启动事件订阅
        {
            let mut graph_guard = graph.lock().unwrap();
            graph_guard.start_subscription(Arc::clone(&graph))?;
        }
        
        // 启动后台任务更新链上数据，不阻塞主流程
        let graph_clone = Arc::clone(&graph);
        let pairs_clone = v3_pairs.clone();
        tokio::spawn(async move {
            info!("开始后台更新链上交易对数据...");
            if let Err(e) = Self::update_pairs_from_blockchain(&pairs_clone, &graph_clone).await {
                warn!("后台更新链上数据失败: {}", e);
            } else {
                info!("后台更新链上数据完成");
            }
        });
        
        // 获取统计信息
        let (token_count, edge_count) = {
            let graph_guard = graph.lock().unwrap();
            (graph_guard.tokens.len(), 
             graph_guard.adjacency_list.values().map(|edges| edges.len()).sum::<usize>())
        };
        
        info!("V3交易对图构建完成，代币数量: {}, 边数量: {}", token_count, edge_count);
        
        // 测试套利路径查找功能（使用合理的参数）
        let paths = {
            let graph_guard = graph.lock().unwrap();
            graph_guard.find_arbitrage_paths("USDT", 4, 0.01) // 最小盈利阈值1%
        };

        Ok(graph)
    }

    /// 获取图的统计信息
    pub fn get_graph_stats(graph: &Arc<Mutex<ExchangeGraph>>) -> (usize, usize) {
        let graph_guard = graph.lock().unwrap();
        graph_guard.get_stats()
    }

    /// 检查图中是否存在直接路径
    pub fn has_direct_path(graph: &Arc<Mutex<ExchangeGraph>>, from_token: &str, to_token: &str) -> bool {
        let graph_guard = graph.lock().unwrap();
        graph_guard.has_direct_path(from_token, to_token)
    }
}
