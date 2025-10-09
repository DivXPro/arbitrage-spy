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
        
        // 创建区块链客户端，使用重试机制避免DNS错误
        let blockchain_client: Arc<BlockchainClient> = match Self::create_blockchain_client_with_retry().await {
            Ok(client) => Arc::new(client),
            Err(e) => {
                warn!("无法创建区块链客户端，跳过链上数据更新: {}", e);
                return Ok(());
            }
        };
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
        
        // 创建EventListener实例（不立即连接WebSocket以避免DNS错误）
        let event_listener = EventListener::new_without_connection(10);
        
        if v3_pairs.is_empty() {
            warn!("数据库中没有找到V3交易对数据");
            let graph = Arc::new(Mutex::new(ExchangeGraph::new(Arc::new(Mutex::new(event_listener)), None)?));
            return Ok(graph);
        }
        
        info!("从数据库读取到 {} 个V3交易对", v3_pairs.len());
        
        // 使用ExchangeGraph的new构造函数创建图，直接传入pairs参数
        let exchange_graph = ExchangeGraph::new(Arc::new(Mutex::new(event_listener)), Some(&v3_pairs))?;
        let graph = Arc::new(Mutex::new(exchange_graph));

        // 延迟启动后台任务更新链上数据，避免与主线程的DNS解析冲突
        let graph_clone = Arc::clone(&graph);
        let pairs_clone = v3_pairs.clone();
        tokio::spawn(async move {
            // 等待5秒，让主线程完成图构建
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            
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
        
        Ok(graph)
    }

    /// 按Token集合读取V3交易对并构建ExchangeGraph
    pub async fn build_v3_exchange_graph_for_tokens(
        database: &Database,
        tokens: &[String],
        limit: Option<usize>,
    ) -> Result<Arc<Mutex<ExchangeGraph>>> {
        info!("开始按地址集合从数据库读取V3交易对数据...");

        // 参数校验
        if tokens.is_empty() {
            warn!("地址集合为空，返回空图");
            let event_listener = EventListener::new_without_connection(10);
            let graph = Arc::new(Mutex::new(ExchangeGraph::new(Arc::new(Mutex::new(event_listener)), None)?));
            return Ok(graph);
        }

        // 读取V3交易对数据（按地址集合筛选）
        let pair_manager = PairManager::new(database);
        let v3_pairs = pair_manager.load_pairs_by_tokens_addresses(
            None,  // network
            Some(dex_types::UNISWAP_V3),  // dex_type
            tokens,
            limit,
        )?;
        
        // 创建EventListener实例（不立即连接WebSocket以避免DNS错误）
        let event_listener = EventListener::new_without_connection(10);
        
        if v3_pairs.is_empty() {
            warn!("数据库中未找到匹配的V3交易对");
            let graph = Arc::new(Mutex::new(ExchangeGraph::new(Arc::new(Mutex::new(event_listener)), None)?));
            return Ok(graph);
        }
        
        info!("按地址集合读取到 {} 个V3交易对", v3_pairs.len());
        
        // 使用ExchangeGraph的new构造函数创建图，直接传入pairs参数
        let exchange_graph = ExchangeGraph::new(Arc::new(Mutex::new(event_listener)), Some(&v3_pairs))?;
        let graph = Arc::new(Mutex::new(exchange_graph));

        // 延迟启动后台任务更新链上数据，避免与主线程的DNS解析冲突
        let graph_clone = Arc::clone(&graph);
        let pairs_clone = v3_pairs.clone();
        tokio::spawn(async move {
            // 等待5秒，让主线程完成图构建
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            
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
        
        info!("按地址集合的V3交易对图构建完成，代币数量: {}, 边数量: {}", token_count, edge_count);
        
        Ok(graph)
    }

    /// 按交易对ID集合读取V3交易对并构建ExchangeGraph
    pub async fn build_v3_exchange_graph_for_pair_ids(
        database: &Database,
        pair_ids: &[String],
        limit: Option<usize>,
    ) -> Result<Arc<Mutex<ExchangeGraph>>> {
        info!("开始按交易对ID集合从数据库读取V3交易对数据...");

        // 参数校验
        if pair_ids.is_empty() {
            warn!("交易对ID集合为空，返回空图");
            let event_listener = EventListener::new_without_connection(10);
            let graph = Arc::new(Mutex::new(ExchangeGraph::new(Arc::new(Mutex::new(event_listener)), None)?));
            return Ok(graph);
        }

        // 读取V3交易对数据（按ID集合筛选）
        let pair_manager = PairManager::new(database);
        let v3_pairs = pair_manager.load_pairs_by_ids(
            None,  // network
            Some(dex_types::UNISWAP_V3),  // dex_type
            pair_ids,
            limit,
        )?;
        
        // 创建EventListener实例（不立即连接WebSocket以避免DNS错误）
        let event_listener = EventListener::new_without_connection(10);
        
        if v3_pairs.is_empty() {
            warn!("数据库中未找到匹配的V3交易对");
            let graph = Arc::new(Mutex::new(ExchangeGraph::new(Arc::new(Mutex::new(event_listener)), None)?));
            return Ok(graph);
        }
        
        info!("按交易对ID集合读取到 {} 个V3交易对", v3_pairs.len());
        
        // 使用ExchangeGraph的new构造函数创建图，直接传入pairs参数
        let exchange_graph = ExchangeGraph::new(Arc::new(Mutex::new(event_listener)), Some(&v3_pairs))?;
        let graph = Arc::new(Mutex::new(exchange_graph));

        // 延迟启动后台任务更新链上数据，避免与主线程的DNS解析冲突
        let graph_clone = Arc::clone(&graph);
        let pairs_clone = v3_pairs.clone();
        tokio::spawn(async move {
            // 等待5秒，让主线程完成图构建
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            
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
        
        info!("按交易对ID集合的V3交易对图构建完成，代币数量: {}, 边数量: {}", token_count, edge_count);
        
        Ok(graph)
    }

    /// 创建区块链客户端，带重试机制避免DNS冲突
    async fn create_blockchain_client_with_retry() -> Result<BlockchainClient> {
        let mut last_error = None;
        
        // 重试3次，每次间隔2秒
        for attempt in 1..=3 {
            info!("尝试创建区块链客户端 (第{}/3次)...", attempt);
            
            match BlockchainClient::ethereum().await {
                Ok(client) => {
                    info!("成功创建区块链客户端");
                    return Ok(client);
                }
                Err(e) => {
                    warn!("第{}次创建区块链客户端失败: {}", attempt, e);
                    last_error = Some(e);
                    
                    if attempt < 3 {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                }
            }
        }
        
        Err(last_error.unwrap())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Database, PairManager, PairData, TokenInfo};
    use crate::config::{dex_types, protocol_types};

    fn make_pair(
        id: &str,
        token0_id: &str,
        token0_symbol: &str,
        token1_id: &str,
        token1_symbol: &str,
    ) -> PairData {
        PairData {
            id: id.to_string(),
            network: "ethereum".to_string(),
            dex: dex_types::UNISWAP_V3.to_string(),
            protocol_type: protocol_types::AMM_V3.to_string(),
            token0: TokenInfo {
                id: token0_id.to_string(),
                symbol: token0_symbol.to_string(),
                name: token0_symbol.to_string(),
                decimals: "18".to_string(),
            },
            token1: TokenInfo {
                id: token1_id.to_string(),
                symbol: token1_symbol.to_string(),
                name: token1_symbol.to_string(),
                decimals: "6".to_string(),
            },
            volume_usd: "1000000.0".to_string(),
            reserve_usd: "500000.0".to_string(),
            tx_count: "1000".to_string(),
            reserve0: "1000.0".to_string(),
            reserve1: "3000000.0".to_string(),
            fee_tier: "500".to_string(),
            sqrt_price: Some("0".to_string()),
            tick: Some("0".to_string()),
        }
    }

    #[tokio::test]
    async fn test_build_v3_graph_for_addresses() {
        // 使用内存数据库，自动初始化表
        let database = Database::new(None).expect("Failed to create in-memory DB");
        let manager = PairManager::new(&database);

        // 构造两条 V3 交易对（双向边预期共 4 条）
        let p1 = make_pair("pair-1", "0xWETH", "WETH", "0xUSDC", "USDC");
        let p2 = make_pair("pair-2", "0xUSDC", "USDC", "0xDAI", "DAI");
        manager.save_pairs(&[p1.clone(), p2.clone()]).expect("save_pairs failed");

        // 以地址集合（此处即 token id 集合）构建图
        let tokens = vec!["0xWETH".to_string(), "0xUSDC".to_string(), "0xDAI".to_string()];
        let graph = ArbitrageTrade::build_v3_exchange_graph_for_tokens(&database, &tokens, None)
            .await
            .expect("build_v3_exchange_graph_for_addresses failed");

        // 验证图的统计信息
        let (token_count, edge_count) = ArbitrageTrade::get_graph_stats(&graph);
        assert_eq!(token_count, 3, "Token count should be 3");
        assert_eq!(edge_count, 4, "Edge count should be 4 (two pairs, bidirectional edges)");

        // 验证存在直接路径
        assert!(ArbitrageTrade::has_direct_path(&graph, "0xWETH", "0xUSDC"));
        assert!(ArbitrageTrade::has_direct_path(&graph, "0xUSDC", "0xWETH"));
        assert!(ArbitrageTrade::has_direct_path(&graph, "0xUSDC", "0xDAI"));
        assert!(ArbitrageTrade::has_direct_path(&graph, "0xDAI", "0xUSDC"));
    }

    #[tokio::test]
    async fn test_build_v3_graph_for_pair_ids() {
        // 使用内存数据库，自动初始化表
        let database = Database::new(None).expect("Failed to create in-memory DB");
        let manager = PairManager::new(&database);

        // 构造两条 V3 交易对
        let p1 = make_pair("pair-1", "0xWETH", "WETH", "0xUSDC", "USDC");
        let p2 = make_pair("pair-2", "0xUSDC", "USDC", "0xDAI", "DAI");
        manager.save_pairs(&[p1.clone(), p2.clone()]).expect("save_pairs failed");

        // 以交易对ID集合构建图
        let pair_ids = vec!["pair-1".to_string(), "pair-2".to_string()];
        let graph = ArbitrageTrade::build_v3_exchange_graph_for_pair_ids(&database, &pair_ids, None)
            .await
            .expect("build_v3_exchange_graph_for_pair_ids failed");

        // 验证图的统计信息
        let (token_count, edge_count) = ArbitrageTrade::get_graph_stats(&graph);
        assert_eq!(token_count, 3, "Token count should be 3");
        assert_eq!(edge_count, 4, "Edge count should be 4 (two pairs, bidirectional edges)");

        // 验证存在直接路径
        assert!(ArbitrageTrade::has_direct_path(&graph, "0xWETH", "0xUSDC"));
        assert!(ArbitrageTrade::has_direct_path(&graph, "0xUSDC", "0xWETH"));
        assert!(ArbitrageTrade::has_direct_path(&graph, "0xUSDC", "0xDAI"));
        assert!(ArbitrageTrade::has_direct_path(&graph, "0xDAI", "0xUSDC"));
    }
}
