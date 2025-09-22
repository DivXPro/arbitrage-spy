use std::path;
use std::sync::Arc;

use anyhow::Result;
use log::{info, warn, debug};
use crate::core::exchange_graph::ExchangeGraph;
use crate::data::database::Database;
use crate::data::pair_manager::{PairManager, PairData};
use crate::data::blockchain_client::{BlockchainClient, NetworkConfig};
use crate::data::uniswap_v3_client::UniswapV3Client;
use crate::config::{dex_types};
use ethers::prelude::*;

/// 套利交易相关功能
pub struct ArbitrageTrade;

impl ArbitrageTrade {
    /// 通过链上数据更新PairData的最新信息
    pub async fn update_pairs_from_blockchain(pairs: &mut [PairData]) -> Result<()> {
        info!("开始从链上更新 {} 个交易对的数据...", pairs.len());
        
        // 创建区块链客户端
        let blockchain_client = Arc::new(BlockchainClient::ethereum().await?);
        let v3_client = UniswapV3Client::new(blockchain_client.clone())?;
        
        let mut updated_count = 0;
        let mut error_count = 0;
        
        for pair in pairs.iter_mut() {
            // 只更新V3交易对
            if pair.dex != dex_types::UNISWAP_V3 {
                debug!("跳过非V3交易对: {} ({})", pair.id, pair.dex);
                continue;
            }
            
            match Self::update_single_pair_from_blockchain(pair, &v3_client).await {
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
    async fn update_single_pair_from_blockchain(
        pair: &mut PairData, 
        v3_client: &UniswapV3Client
    ) -> Result<()> {
        // 解析池地址
        let pool_address = pair.id.parse::<Address>()
            .map_err(|e| anyhow::anyhow!("无效的池地址 {}: {}", pair.id, e))?;
        
        // 获取池的最新信息
        let pool_info = v3_client.get_pool_info(pool_address).await?;
        
        // 更新PairData中的相关字段
        let sqrt_price_value = pool_info.sqrt_price_x96.clone();
        let tick_value = pool_info.tick;
        let liquidity_value = pool_info.liquidity.clone();
        
        pair.sqrt_price = Some(sqrt_price_value.clone());
        pair.tick = Some(tick_value.to_string());
        
        // 计算并更新流动性相关数据
        // 注意：这里简化处理，实际应该根据sqrt_price和liquidity计算储备量
        if let Ok(liquidity_f64) = liquidity_value.parse::<f64>() {
            // 简化的流动性转换为USD价值
            let estimated_usd_value = liquidity_f64 / 1e18 * 2000.0; // 假设平均代币价格
            pair.reserve_usd = estimated_usd_value.to_string();
        }
        
        debug!("更新交易对 {} 的链上数据: sqrt_price={}, tick={}, liquidity={}", 
               pair.id, 
               sqrt_price_value, 
               tick_value, 
               liquidity_value);
        
        Ok(())
    }

    /// 从数据库读取V3交易对并构建ExchangeGraph
    pub async fn build_v3_exchange_graph(database: &Database) -> Result<ExchangeGraph> {
        info!("开始从数据库读取V3交易对数据...");
        
        // 创建新的ExchangeGraph实例
        let mut graph = ExchangeGraph::new();
        
        // 读取V3交易对数据
        let pair_manager = PairManager::new(database);
        let mut v3_pairs = pair_manager.load_pairs_by_filter(
            None,  // network
            Some(dex_types::UNISWAP_V3),  // dex_type
            None,  // limit
        )?;
        
        if v3_pairs.is_empty() {
            warn!("数据库中没有找到V3交易对数据");
            return Ok(graph);
        }
        
        info!("从数据库读取到 {} 个V3交易对", v3_pairs.len());
        
        // 通过链上方法更新PairData的最新数据
        info!("正在从链上更新交易对数据...");
        Self::update_pairs_from_blockchain(&mut v3_pairs).await?;
        
        // 使用ExchangeGraph的from_pair_data方法构建图
        graph.from_pair_data(&v3_pairs)?;
        
        info!("V3交易对图构建完成，代币数量: {}, 边数量: {}", 
              graph.tokens.len(), 
              graph.adjacency_list.values().map(|edges| edges.len()).sum::<usize>());
        
        // 测试套利路径查找功能（使用合理的参数）
        let paths = graph.find_arbitrage_paths("USDT", 4, 0.01); // 最小盈利阈值1%

        Ok(graph)
    }

    /// 获取图的统计信息
    pub fn get_graph_stats(graph: &ExchangeGraph) -> (usize, usize) {
        graph.get_stats()
    }

    /// 检查图中是否存在直接路径
    pub fn has_direct_path(graph: &ExchangeGraph, from_token: &str, to_token: &str) -> bool {
        graph.has_direct_path(from_token, to_token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::pair_manager::{PairData, TokenInfo};
    use std::str::FromStr;
    use bigdecimal::BigDecimal;

    #[test]
    fn test_arbitrage_trade_creation() {
        // 这里可以添加测试用例
        // 由于需要数据库连接，实际测试需要mock数据库
        assert!(true);
    }

    #[test]
    fn test_graph_stats() {
        let graph = ExchangeGraph::new();
        let (tokens, edges) = ArbitrageTrade::get_graph_stats(&graph);
        assert_eq!(tokens, 0);
        assert_eq!(edges, 0);
    }

    #[test]
    fn test_has_direct_path() {
        let graph = ExchangeGraph::new();
        assert!(!ArbitrageTrade::has_direct_path(&graph, "USDT", "USDC"));
    }
}