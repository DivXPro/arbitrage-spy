use std::collections::{HashMap, HashSet};
use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use log::{info, warn, debug};
use crate::types::{TokenPair, Price};

#[cfg(test)]
use std::str::FromStr;

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

/// 套利路径中的一跳
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageHop {
    pub edge: ArbitrageEdge,        // 交换边信息
    pub amount_in: BigDecimal,      // 输入金额
    pub amount_out: BigDecimal,     // 输出金额
    pub cumulative_gas: BigDecimal, // 累计Gas费用
    pub cumulative_fees: BigDecimal,// 累计交易费用
}

/// 完整的套利链
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageChain {
    pub start_token: String,            // 起始代币符号
    pub hops: Vec<ArbitrageHop>,        // 套利路径中的所有跳跃
    pub initial_amount: BigDecimal,     // 初始投入金额
    pub final_amount: BigDecimal,       // 最终获得金额
    pub total_profit: BigDecimal,       // 总利润
    pub total_gas_cost: BigDecimal,     // 总Gas成本
    pub total_fees: BigDecimal,         // 总交易费用
    pub net_profit: BigDecimal,         // 净利润（扣除Gas和费用）
    pub profit_percentage: f64,         // 利润百分比
    pub risk_score: f64,                // 风险评分（0-1）
    pub execution_time_estimate: u64,   // 预估执行时间(秒)
}

/// 价格图，用于存储所有代币间的交换关系
pub struct PriceGraph {
    /// 邻接表：token -> [(to_token, edge)]
    pub adjacency_list: HashMap<String, Vec<ArbitrageEdge>>, // 代币交换关系的邻接表
    pub tokens: HashSet<String>,                             // 所有代币符号的集合
    pub last_updated: chrono::DateTime<chrono::Utc>,         // 最后更新时间
}

impl PriceGraph {
    pub fn new() -> Self {
        Self {
            adjacency_list: HashMap::new(),
            tokens: HashSet::new(),
            last_updated: chrono::Utc::now(),
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

        self.last_updated = chrono::Utc::now();
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
    fn estimate_gas_cost(dex_name: &str) -> BigDecimal {
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
    fn estimate_slippage(liquidity: &BigDecimal) -> f64 {
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
    fn get_dex_fee_percentage(dex_name: &str) -> f64 {
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
        self.last_updated = chrono::Utc::now();
    }
}

/// 套利链查找器
pub struct ArbitrageChainFinder {
    max_hops: usize,                      // 最大跳跃次数
    min_profit_percentage: f64,           // 最小利润百分比
    max_slippage: f64,                    // 最大滑点
    min_liquidity: BigDecimal,            // 最小流动性要求
    max_risk_score: f64,                  // 最大风险评分
    // 性能优化字段
    max_chains_per_token: usize,          // 每个代币最多返回的链数
    min_amount_threshold: BigDecimal,     // 最小金额阈值，低于此值停止搜索
    enable_early_pruning: bool,           // 是否启用早期剪枝
}

impl ArbitrageChainFinder {
    pub fn new(
        max_hops: usize, 
        min_profit_percentage: f64, 
        max_slippage: f64,
        min_liquidity: f64,
        max_risk_score: f64,
    ) -> Self {
        Self {
            max_hops,
            min_profit_percentage,
            max_slippage,
            min_liquidity: BigDecimal::from_f64(min_liquidity).unwrap_or_default(),
            max_risk_score,
            // 默认性能优化设置
            max_chains_per_token: 10,
            min_amount_threshold: BigDecimal::from_f64(0.001).unwrap_or_default(),
            enable_early_pruning: true,
        }
    }

    /// 创建高性能配置的查找器
    pub fn new_optimized(
        max_hops: usize, 
        min_profit_percentage: f64, 
        max_slippage: f64,
        min_liquidity: f64,
        max_risk_score: f64,
        max_chains_per_token: usize,
        min_amount_threshold: f64,
    ) -> Self {
        Self {
            max_hops,
            min_profit_percentage,
            max_slippage,
            min_liquidity: BigDecimal::from_f64(min_liquidity).unwrap_or_default(),
            max_risk_score,
            max_chains_per_token,
            min_amount_threshold: BigDecimal::from_f64(min_amount_threshold).unwrap_or_default(),
            enable_early_pruning: true,
        }
    }

    /// 获取最大跳数
    pub fn max_hops(&self) -> usize {
        self.max_hops
    }

    /// 获取最小利润百分比
    pub fn min_profit_percentage(&self) -> f64 {
        self.min_profit_percentage
    }

    /// 寻找从指定代币开始的所有套利链
    pub fn find_arbitrage_chains(&self, graph: &PriceGraph, start_token: &str) -> Result<Vec<ArbitrageChain>> {
        info!("开始寻找从 {} 开始的套利链", start_token);
        
        if !graph.tokens.contains(start_token) {
            return Err(anyhow!("起始代币 {} 在价格图中不存在", start_token));
        }

        let mut chains = Vec::new();
        let mut visited = HashSet::new();
        let mut current_path = Vec::new();
        let initial_amount = BigDecimal::from(1); // 从1个单位开始

        // 性能优化：预先计算最佳边以减少搜索空间
        let sorted_edges = if self.enable_early_pruning {
            self.get_sorted_edges_from_token(graph, start_token)
        } else {
            None
        };

        self.dfs_search(
            graph,
            start_token,
            start_token,
            initial_amount,
            &mut visited,
            &mut current_path,
            &mut chains,
            0,
        )?;

        // 按净利润排序并限制结果数量
        chains.sort_by(|a, b| {
            b.net_profit.partial_cmp(&a.net_profit)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 限制返回的链数量以提高性能
        if chains.len() > self.max_chains_per_token {
            chains.truncate(self.max_chains_per_token);
        }

        info!("找到 {} 条套利链", chains.len());
        Ok(chains)
    }

    /// 获取从指定代币出发的边，按潜在收益排序
    fn get_sorted_edges_from_token(&self, graph: &PriceGraph, token: &str) -> Option<Vec<String>> {
        graph.get_edges_from(token).map(|edges| {
            let mut edge_scores: Vec<(String, f64)> = edges.iter()
                .map(|edge| {
                    // 简单的启发式评分：汇率 * 流动性 / (滑点 + 费用)
                    let rate_score = edge.exchange_rate.to_f64().unwrap_or(0.0);
                    let liquidity_score = edge.liquidity.to_f64().unwrap_or(0.0).log10().max(0.0);
                    let penalty = edge.slippage + edge.fee_percentage + 0.001; // 避免除零
                    let score = (rate_score * liquidity_score) / penalty;
                    (edge.to_token.clone(), score)
                })
                .collect();
            
            edge_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            edge_scores.into_iter().map(|(token, _)| token).collect()
        })
    }

    fn dfs_search(
        &self,
        graph: &PriceGraph,
        current_token: &str,
        start_token: &str,
        current_amount: BigDecimal,
        visited: &mut HashSet<String>,
        current_path: &mut Vec<ArbitrageHop>,
        chains: &mut Vec<ArbitrageChain>,
        depth: usize,
    ) -> Result<()> {
        // 早期剪枝：如果当前金额太小，停止搜索
        if self.enable_early_pruning && current_amount < self.min_amount_threshold {
            return Ok(());
        }

        // 早期剪枝：如果已经找到足够多的链，停止搜索
        if self.enable_early_pruning && chains.len() >= self.max_chains_per_token * 2 {
            return Ok(());
        }

        // 如果回到起始代币且路径长度 > 1，检查是否有利润
        if current_token == start_token && depth > 1 {
            let chain = self.build_arbitrage_chain(start_token, current_path, &current_amount)?;
            if chain.profit_percentage >= self.min_profit_percentage && 
               chain.risk_score <= self.max_risk_score {
                chains.push(chain);
            }
            return Ok(());
        }

        // 如果达到最大跳数，停止搜索
        if depth >= self.max_hops {
            return Ok(());
        }

        // 获取当前代币的所有出边
        if let Some(edges) = graph.get_edges_from(current_token) {
            // 性能优化：对边进行排序，优先处理高质量的边
            let mut sorted_edges: Vec<&ArbitrageEdge> = edges.iter().collect();
            if self.enable_early_pruning {
                sorted_edges.sort_by(|a, b| {
                    // 综合评分：汇率 * 流动性权重 - 风险权重
                    let score_a = self.calculate_edge_priority_score(a);
                    let score_b = self.calculate_edge_priority_score(b);
                    score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
                });
            }

            for edge in sorted_edges {
                // 避免立即回头（除非是完成循环）
                if depth > 0 && edge.to_token == start_token && depth < 2 {
                    continue;
                }

                // 检查各种约束条件
                if !self.is_edge_acceptable(edge, visited, start_token, depth) {
                    continue;
                }

                // 计算交换后的数量
                let amount_out = self.calculate_amount_after_swap(&current_amount, edge)?;
                
                // 创建跳跃
                let hop = ArbitrageHop {
                    edge: edge.clone(),
                    amount_in: current_amount.clone(),
                    amount_out: amount_out.clone(),
                    cumulative_gas: self.calculate_cumulative_gas(current_path, &edge.gas_cost),
                    cumulative_fees: self.calculate_cumulative_fees(current_path, &current_amount, edge),
                };

                // 添加到路径
                current_path.push(hop);
                visited.insert(current_token.to_string());

                // 递归搜索
                self.dfs_search(
                    graph,
                    &edge.to_token,
                    start_token,
                    amount_out,
                    visited,
                    current_path,
                    chains,
                    depth + 1,
                )?;

                // 回溯
                current_path.pop();
                if edge.to_token != start_token {
                    visited.remove(current_token);
                }
            }
        }

        Ok(())
    }

    fn is_edge_acceptable(
        &self, 
        edge: &ArbitrageEdge, 
        visited: &HashSet<String>, 
        start_token: &str, 
        depth: usize
    ) -> bool {
        // 检查滑点
        if edge.slippage > self.max_slippage {
            return false;
        }

        // 检查流动性
        if edge.liquidity < self.min_liquidity {
            return false;
        }

        // 避免重复访问（除非是回到起始点）
        if visited.contains(&edge.to_token) && edge.to_token != start_token {
            return false;
        }

        true
    }

    fn calculate_amount_after_swap(&self, amount_in: &BigDecimal, edge: &ArbitrageEdge) -> Result<BigDecimal> {
        // 应用交易费用
        let amount_after_fee = amount_in * (BigDecimal::from(1) - BigDecimal::from_f64(edge.fee_percentage).unwrap_or_default());
        
        // 应用汇率
        let amount_before_slippage = amount_after_fee * &edge.exchange_rate;
        
        // 应用滑点
        let slippage_factor = BigDecimal::from(1) - BigDecimal::from_f64(edge.slippage).unwrap_or_default();
        let amount_out = amount_before_slippage * slippage_factor;
        
        Ok(amount_out)
    }

    fn calculate_cumulative_gas(&self, current_path: &[ArbitrageHop], additional_gas: &BigDecimal) -> BigDecimal {
        let current_gas = current_path.iter()
            .map(|hop| &hop.cumulative_gas)
            .fold(BigDecimal::from(0), |acc, gas| acc + gas);
        current_gas + additional_gas
    }

    fn calculate_cumulative_fees(&self, current_path: &[ArbitrageHop], amount_in: &BigDecimal, edge: &ArbitrageEdge) -> BigDecimal {
        let current_fees = current_path.iter()
            .map(|hop| &hop.cumulative_fees)
            .fold(BigDecimal::from(0), |acc, fees| acc + fees);
        
        let current_fee = amount_in * BigDecimal::from_f64(edge.fee_percentage).unwrap_or_default();
        current_fees + current_fee
    }

    fn build_arbitrage_chain(&self, start_token: &str, path: &[ArbitrageHop], final_amount: &BigDecimal) -> Result<ArbitrageChain> {
        let initial_amount = BigDecimal::from(1);
        let total_profit = final_amount - &initial_amount;
        let total_gas_cost = path.last().map(|hop| hop.cumulative_gas.clone()).unwrap_or_default();
        let total_fees = path.last().map(|hop| hop.cumulative_fees.clone()).unwrap_or_default();
        let net_profit = &total_profit - &total_gas_cost;
        let profit_percentage = (net_profit.clone() / &initial_amount).to_f64().unwrap_or(0.0) * 100.0;

        // 计算风险评分和执行时间
        let risk_score = self.calculate_risk_score(path);
        let execution_time_estimate = self.estimate_execution_time(path);

        Ok(ArbitrageChain {
            start_token: start_token.to_string(),
            hops: path.to_vec(),
            initial_amount,
            final_amount: final_amount.clone(),
            total_profit,
            total_gas_cost,
            total_fees,
            net_profit,
            profit_percentage,
            risk_score,
            execution_time_estimate,
        })
    }

    fn calculate_risk_score(&self, path: &[ArbitrageHop]) -> f64 {
        let mut risk_score = 0.0;
        
        // 路径长度风险 (每跳增加10%风险)
        risk_score += path.len() as f64 * 0.1;
        
        // 流动性风险
        let default_liquidity = BigDecimal::from(0);
        let min_liquidity = path.iter()
            .map(|hop| &hop.edge.liquidity)
            .min()
            .unwrap_or(&default_liquidity);
        
        let min_liquidity_f64 = min_liquidity.to_f64().unwrap_or(0.0);
        if min_liquidity_f64 < 100_000.0 {
            risk_score += 0.3;
        } else if min_liquidity_f64 < 1_000_000.0 {
            risk_score += 0.1;
        }
        
        // 滑点风险
        let total_slippage: f64 = path.iter()
            .map(|hop| hop.edge.slippage)
            .sum();
        risk_score += total_slippage * 5.0;
        
        // DEX多样性风险 (使用相同DEX增加风险)
        let unique_dexes: HashSet<_> = path.iter().map(|hop| &hop.edge.dex).collect();
        if unique_dexes.len() < path.len() {
            risk_score += 0.2;
        }
        
        // 限制在0-1之间
        risk_score.min(1.0)
    }

    fn estimate_execution_time(&self, path: &[ArbitrageHop]) -> u64 {
        // 基础时间：每跳15秒
        let base_time = path.len() as u64 * 15;
        
        // 根据DEX类型调整
        let dex_adjustment: u64 = path.iter()
            .map(|hop| match hop.edge.dex.to_lowercase().as_str() {
                name if name.contains("uniswap") => 10,
                name if name.contains("curve") => 20,
                name if name.contains("balancer") => 25,
                _ => 15,
            })
            .sum();
        
        base_time + dex_adjustment
    }

    /// 计算边的优先级评分，用于排序优化
    fn calculate_edge_priority_score(&self, edge: &ArbitrageEdge) -> f64 {
        // 汇率权重 (40%)
        let rate_score = edge.exchange_rate.to_f64().unwrap_or(0.0) * 0.4;
        
        // 流动性权重 (30%) - 使用对数缩放
        let liquidity_raw = edge.liquidity.to_f64().unwrap_or(1.0);
        let liquidity_score = if liquidity_raw > 0.0 {
            liquidity_raw.log10().max(0.0) * 0.3
        } else {
            0.0
        };
        
        // 费用惩罚 (15%)
        let fee_penalty = edge.fee_percentage * 0.15;
        
        // 滑点惩罚 (10%)
        let slippage_penalty = edge.slippage * 0.1;
        
        // Gas成本惩罚 (5%)
        let gas_penalty = edge.gas_cost.to_f64().unwrap_or(0.0) * 0.05;
        
        // 综合评分：收益 - 成本
        rate_score + liquidity_score - fee_penalty - slippage_penalty - gas_penalty
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Token;

    #[test]
    fn test_price_graph_creation() {
        let mut graph = PriceGraph::new();
        assert_eq!(graph.tokens.len(), 0);
        assert_eq!(graph.adjacency_list.len(), 0);
    }

    #[test]
    fn test_arbitrage_chain_finder_creation() {
        let finder = ArbitrageChainFinder::new(3, 1.0, 0.01, 100000.0, 0.8);
        assert_eq!(finder.max_hops(), 3);
        assert_eq!(finder.min_profit_percentage(), 1.0);
    }

    #[test]
    fn test_gas_cost_estimation() {
        assert_eq!(PriceGraph::estimate_gas_cost("Uniswap V2"), BigDecimal::from_f64(0.003).unwrap_or_default());
        assert_eq!(PriceGraph::estimate_gas_cost("Uniswap V3"), BigDecimal::from_f64(0.005).unwrap_or_default());
        assert_eq!(PriceGraph::estimate_gas_cost("Curve"), BigDecimal::from_f64(0.004).unwrap_or_default());
    }

    #[test]
    fn test_slippage_estimation() {
        assert_eq!(PriceGraph::estimate_slippage(&BigDecimal::from(20_000_000)), 0.0005);
        assert_eq!(PriceGraph::estimate_slippage(&BigDecimal::from(500_000)), 0.005);
        assert_eq!(PriceGraph::estimate_slippage(&BigDecimal::from(5_000)), 0.03);
    }

    #[test]
    fn test_optimized_finder_creation() {
        let finder = ArbitrageChainFinder::new_optimized(
            3,      // max_hops
            1.0,    // min_profit_percentage
            0.05,   // max_slippage
            1000.0, // min_liquidity
            0.8,    // max_risk_score
            5,      // max_chains_per_token
            0.01,   // min_amount_threshold
        );
        
        assert_eq!(finder.max_hops(), 3);
        assert_eq!(finder.min_profit_percentage(), 1.0);
        assert_eq!(finder.max_chains_per_token, 5);
        assert!(finder.enable_early_pruning);
    }

    #[test]
    fn test_edge_priority_scoring() {
        let finder = ArbitrageChainFinder::new(3, 1.0, 0.05, 1000.0, 0.8);
        
        // 创建高质量边
        let high_quality_edge = ArbitrageEdge {
            from_token: "ETH".to_string(),
            to_token: "USDC".to_string(),
            dex: "uniswap_v2".to_string(),
            exchange_rate: BigDecimal::from_str("2000.0").unwrap(),
            liquidity: BigDecimal::from_str("1000000.0").unwrap(),
            gas_cost: BigDecimal::from_str("0.01").unwrap(),
            slippage: 0.01,
            fee_percentage: 0.003,
        };
        
        // 创建低质量边
        let low_quality_edge = ArbitrageEdge {
            from_token: "ETH".to_string(),
            to_token: "USDC".to_string(),
            dex: "uniswap_v2".to_string(),
            exchange_rate: BigDecimal::from_str("100.0").unwrap(),
            liquidity: BigDecimal::from_str("1000.0").unwrap(),
            gas_cost: BigDecimal::from_str("0.1").unwrap(),
            slippage: 0.1,
            fee_percentage: 0.01,
        };
        
        let high_score = finder.calculate_edge_priority_score(&high_quality_edge);
        let low_score = finder.calculate_edge_priority_score(&low_quality_edge);
        
        assert!(high_score > low_score, "高质量边应该有更高的评分");
    }

    #[test]
    fn test_performance_with_large_graph() {
        use std::time::Instant;
        
        let mut graph = PriceGraph::new();
        
        // 创建一个较大的测试图
        let tokens = vec!["ETH", "USDC", "USDT", "DAI", "WBTC", "LINK", "UNI", "AAVE"];
        
        // 添加所有可能的边
        for (i, from_token) in tokens.iter().enumerate() {
            for (j, to_token) in tokens.iter().enumerate() {
                if i != j {
                    let edge = ArbitrageEdge {
                        from_token: from_token.to_string(),
                        to_token: to_token.to_string(),
                        dex: "uniswap_v2".to_string(),
                        exchange_rate: BigDecimal::from_str("1.1").unwrap(),
                        liquidity: BigDecimal::from_str("100000.0").unwrap(),
                        gas_cost: BigDecimal::from_str("0.01").unwrap(),
                        slippage: 0.01,
                        fee_percentage: 0.003,
                    };
                    graph.add_edge(edge);
                }
            }
        }
        
        // 测试优化版本的性能
        let optimized_finder = ArbitrageChainFinder::new_optimized(
            3, 0.5, 0.05, 1000.0, 0.8, 5, 0.001
        );
        
        let start = Instant::now();
        let result = optimized_finder.find_arbitrage_chains(&graph, "ETH");
        let duration = start.elapsed();
        
        assert!(result.is_ok());
        println!("优化版本搜索耗时: {:?}", duration);
        
        // 确保在合理时间内完成（应该在1秒内）
        assert!(duration.as_secs() < 1, "搜索时间过长: {:?}", duration);
    }
}