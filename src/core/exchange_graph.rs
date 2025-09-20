use std::collections::{HashMap, HashSet};
use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive};
use anyhow::{Result};
use serde::{Deserialize, Serialize};
use log::{info, warn, debug};
use chrono::{DateTime, Utc};
use crate::core::types::{TokenPair, Price};

/// 图中的边，表示一次代币交换
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageEdge {
    pub from_token: String,         // 源代币符号
    pub to_token: String,           // 目标代币符号
    pub dex: String,                // 去中心化交易所名称
    pub exchange_rate: BigDecimal,  // 汇率 (to_token/from_token)
    pub liquidity: BigDecimal,      // 流动性
    pub gas_cost: BigDecimal,       // Gas成本估算
    pub slippage: f64,              // 预期滑点
    pub fee_percentage: f64,        // 交易费用百分比
}

/// 价格图，用于存储所有代币间的交换关系
pub struct ExchangeGraph {
    /// 邻接表：token -> [(to_token, edge)]
    pub adjacency_list: HashMap<String, Vec<ArbitrageEdge>>, // 代币交换关系的邻接表
    pub tokens: HashSet<String>,                             // 所有代币符号的集合
    pub last_updated: DateTime<Utc>,         // 最后更新时间
}

impl ExchangeGraph {
    pub fn new() -> Self {
        Self {
            adjacency_list: HashMap::new(),
            tokens: HashSet::new(),
            last_updated: Utc::now(),
        }
    }

    /// 从DEX价格数据构建图
    pub fn build_from_dex_data(&mut self, dex_data: &HashMap<String, HashMap<TokenPair, Price>>) -> Result<()> {
        info!("开始构建价格图，DEX数量: {}", dex_data.len());
        
        // 清空现有数据
        self.adjacency_list.clear();
        self.tokens.clear();
        
        let mut edge_count = 0;
        
        for (dex_name, prices) in dex_data {
            debug!("处理DEX: {}, 价格数据数量: {}", dex_name, prices.len());
            
            for (token_pair, price) in prices {
                // 验证价格数据
                if price.price <= BigDecimal::from(0) || price.liquidity <= BigDecimal::from(0) {
                    warn!("跳过无效价格数据: {} on {}", 
                          format!("{}/{}", token_pair.token_a.symbol, token_pair.token_b.symbol), 
                          dex_name);
                    continue;
                }

                // 添加正向边 (token_a -> token_b)
                let forward_edge = ArbitrageEdge {
                    from_token: token_pair.token_a.symbol.clone(),
                    to_token: token_pair.token_b.symbol.clone(),
                    dex: dex_name.clone(),
                    exchange_rate: price.price.clone(),
                    liquidity: price.liquidity.clone(),
                    gas_cost: Self::estimate_gas_cost(dex_name),
                    slippage: Self::estimate_slippage(&price.liquidity),
                    fee_percentage: Self::get_dex_fee_percentage(dex_name),
                };

                // 添加反向边 (token_b -> token_a)
                let reverse_rate = BigDecimal::from(1) / &price.price;
                let reverse_edge = ArbitrageEdge {
                    from_token: token_pair.token_b.symbol.clone(),
                    to_token: token_pair.token_a.symbol.clone(),
                    dex: dex_name.clone(),
                    exchange_rate: reverse_rate,
                    liquidity: price.liquidity.clone(),
                    gas_cost: Self::estimate_gas_cost(dex_name),
                    slippage: Self::estimate_slippage(&price.liquidity),
                    fee_percentage: Self::get_dex_fee_percentage(dex_name),
                };

                self.add_edge(forward_edge);
                self.add_edge(reverse_edge);
                edge_count += 2;
            }
        }

        self.last_updated = Utc::now();
        info!("价格图构建完成，代币数量: {}, 边数量: {}", self.tokens.len(), edge_count);
        
        Ok(())
    }

    pub fn add_edge(&mut self, edge: ArbitrageEdge) {
        self.tokens.insert(edge.from_token.clone());
        self.tokens.insert(edge.to_token.clone());
        
        self.adjacency_list
            .entry(edge.from_token.clone())
            .or_insert_with(Vec::new)
            .push(edge);
    }

    /// 根据DEX类型估算Gas成本
    pub fn estimate_gas_cost(dex_name: &str) -> BigDecimal {
        match dex_name.to_lowercase().as_str() {
            name if name.contains("uniswap") && name.contains("v2") => BigDecimal::from_f64(0.003).unwrap_or_default(),
            name if name.contains("uniswap") && name.contains("v3") => BigDecimal::from_f64(0.005).unwrap_or_default(),
            name if name.contains("sushiswap") => BigDecimal::from_f64(0.003).unwrap_or_default(),
            name if name.contains("curve") => BigDecimal::from_f64(0.004).unwrap_or_default(),
            name if name.contains("balancer") => BigDecimal::from_f64(0.006).unwrap_or_default(),
            name if name.contains("pancakeswap") => BigDecimal::from_f64(0.002).unwrap_or_default(),
            _ => BigDecimal::from_f64(0.003).unwrap_or_default(), // 默认值
        }
    }

    /// 根据流动性估算滑点
    pub fn estimate_slippage(liquidity: &BigDecimal) -> f64 {
        let liquidity_f64 = liquidity.to_f64().unwrap_or(0.0);
        
        if liquidity_f64 > 10_000_000.0 {
            0.0005 // 0.05% - 超高流动性
        } else if liquidity_f64 > 1_000_000.0 {
            0.001  // 0.1% - 高流动性
        } else if liquidity_f64 > 100_000.0 {
            0.005  // 0.5% - 中等流动性
        } else if liquidity_f64 > 10_000.0 {
            0.01   // 1% - 低流动性
        } else {
            0.03   // 3% - 极低流动性
        }
    }

    /// 获取DEX的交易费用百分比
    pub fn get_dex_fee_percentage(dex_name: &str) -> f64 {
        match dex_name.to_lowercase().as_str() {
            name if name.contains("uniswap") && name.contains("v2") => 0.003, // 0.3%
            name if name.contains("uniswap") && name.contains("v3") => 0.003, // 0.3% (可变)
            name if name.contains("sushiswap") => 0.003, // 0.3%
            name if name.contains("curve") => 0.0004,    // 0.04%
            name if name.contains("balancer") => 0.001,  // 0.1% (可变)
            name if name.contains("pancakeswap") => 0.0025, // 0.25%
            _ => 0.003, // 默认0.3%
        }
    }

    /// 获取指定代币的所有出边
    pub fn get_edges_from(&self, token: &str) -> Option<&Vec<ArbitrageEdge>> {
        self.adjacency_list.get(token)
    }

    /// 检查两个代币之间是否存在直接连接
    pub fn has_direct_path(&self, from_token: &str, to_token: &str) -> bool {
        if let Some(edges) = self.adjacency_list.get(from_token) {
            edges.iter().any(|edge| edge.to_token == to_token)
        } else {
            false
        }
    }

    /// 获取图的统计信息
    pub fn get_stats(&self) -> (usize, usize) {
        let token_count = self.tokens.len();
        let edge_count = self.adjacency_list.values().map(|edges| edges.len()).sum();
        (token_count, edge_count)
    }

    /// 清空图数据
    pub fn clear(&mut self) {
        self.adjacency_list.clear();
        self.tokens.clear();
        self.last_updated = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::Token;
    use std::str::FromStr;

    #[test]
    fn test_price_graph_creation() {
        let graph = ExchangeGraph::new();
        assert_eq!(graph.tokens.len(), 0);
        assert_eq!(graph.adjacency_list.len(), 0);
    }

    #[test]
    fn test_gas_cost_estimation() {
        assert_eq!(ExchangeGraph::estimate_gas_cost("uniswap_v2"), BigDecimal::from_f64(0.003).unwrap());
        assert_eq!(ExchangeGraph::estimate_gas_cost("curve"), BigDecimal::from_f64(0.004).unwrap());
    }

    #[test]
    fn test_slippage_estimation() {
        assert_eq!(ExchangeGraph::estimate_slippage(&BigDecimal::from(20_000_000)), 0.0005);
        assert_eq!(ExchangeGraph::estimate_slippage(&BigDecimal::from(5_000)), 0.03);
    }
}