use std::collections::{HashMap, HashSet};
use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive, Zero};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use log::{info, warn, debug};
use chrono::{DateTime, Utc};
use crate::core::types::{TokenPair, Price};
use crate::data::pair_manager::PairData;
use crate::price_calculator::PriceCalculator;
use crate::config::protocol_types;

/// 图中的边，表示一次代币交换
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeEdge {
    pub pair_id: String,            // 交易对唯一标识
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
    pub adjacency_list: HashMap<String, Vec<ExchangeEdge>>, // 代币交换关系的邻接表
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

    pub fn add_edge(&mut self, edge: ExchangeEdge) {
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
    pub fn get_edges_from(&self, token: &str) -> Option<&Vec<ExchangeEdge>> {
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

    /// 从PairData数据构建图
    pub fn from_pair_data(&mut self, pair_data: &[PairData]) -> Result<()> {
        info!("开始从PairData构建价格图，交易对数量: {}", pair_data.len());
        
        // 清空现有数据
        self.adjacency_list.clear();
        self.tokens.clear();
        
        let mut edge_count = 0;
        
        for pair in pair_data {
            // 验证交易对数据
            if let Err(e) = self.validate_pair_data(pair) {
                warn!("跳过无效交易对数据 {}: {}", pair.id, e);
                continue;
            }

            // 使用PriceCalculator根据协议类型计算价格
            let price_1_per_0 = match PriceCalculator::calculate_price_from_pair(pair) {
                Ok(price) => price,
                Err(e) => {
                    warn!("跳过价格计算失败的交易对 {}: {}", pair.id, e);
                    continue;
                }
            };
            
            // 计算反向价格 (token0/token1)
            let price_0_per_1 = if price_1_per_0.is_zero() {
                warn!("跳过零价格的交易对: {}", pair.id);
                continue;
            } else {
                BigDecimal::from(1) / &price_1_per_0
            };
            
            debug!("交易对 {} ({}) 价格计算: {} {} = 1 {}, 1 {} = {} {}", 
                   pair.id, 
                   pair.protocol_type,
                   price_1_per_0, 
                   pair.token1.symbol, 
                   pair.token0.symbol,
                   pair.token0.symbol,
                   price_0_per_1,
                   pair.token1.symbol);

            // 使用reserveUSD作为流动性指标
            let liquidity = pair.reserve_usd.parse::<f64>()
                .map_err(|_| anyhow!("无效的reserveUSD: {}", pair.reserve_usd))?;
            let liquidity_bd = BigDecimal::from_f64(liquidity)
                .ok_or_else(|| anyhow!("无法转换流动性为BigDecimal"))?;

            // 创建正向边 (token0 -> token1)
            let forward_edge = ExchangeEdge {
                pair_id: pair.id.clone(),
                from_token: pair.token0.symbol.clone(),
                to_token: pair.token1.symbol.clone(),
                dex: pair.dex.clone(),
                exchange_rate: price_1_per_0,
                liquidity: liquidity_bd.clone(),
                gas_cost: Self::estimate_gas_cost(&pair.dex),
                slippage: Self::estimate_slippage(&liquidity_bd),
                fee_percentage: Self::get_dex_fee_percentage(&pair.dex),
            };

            // 创建反向边 (token1 -> token0)
            let reverse_edge = ExchangeEdge {
                pair_id: pair.id.clone(),
                from_token: pair.token1.symbol.clone(),
                to_token: pair.token0.symbol.clone(),
                dex: pair.dex.clone(),
                exchange_rate: price_0_per_1,
                liquidity: liquidity_bd.clone(),
                gas_cost: Self::estimate_gas_cost(&pair.dex),
                slippage: Self::estimate_slippage(&liquidity_bd),
                fee_percentage: Self::get_dex_fee_percentage(&pair.dex),
            };

            self.add_edge(forward_edge);
            self.add_edge(reverse_edge);
            edge_count += 2;
        }

        self.last_updated = Utc::now();
        info!("从PairData构建价格图完成，代币数量: {}, 边数量: {}", self.tokens.len(), edge_count);
        
        Ok(())
    }

    /// 更新单个交易对的数据
    /// 如果交易对已存在，会移除旧的边并添加新的边
    /// 直接更新交易对数据，优先更新现有边而不是删除重建
    /// 如果交易对不存在，会添加新的边
    pub fn update_pair_data(&mut self, pair: &PairData) -> Result<()> {
        info!("更新交易对数据: {} ({} <-> {})", pair.id, pair.token0.symbol, pair.token1.symbol);
        
        // 验证交易对数据
        self.validate_pair_data(pair)?;

        // 使用PriceCalculator根据协议类型计算价格
        let price_1_per_0 = PriceCalculator::calculate_price_from_pair(pair)
            .map_err(|e| anyhow!("价格计算失败: {}", e))?;
        
        // 计算反向价格 (token0/token1)
        if price_1_per_0.is_zero() {
            return Err(anyhow!("价格为零，无法更新交易对: {}", pair.id));
        }
        
        let price_0_per_1 = BigDecimal::from(1) / &price_1_per_0;
        
        debug!("交易对 {} ({}) 价格更新: {} {} = 1 {}, 1 {} = {} {}", 
               pair.id, 
               pair.protocol_type,
               price_1_per_0, 
               pair.token1.symbol, 
               pair.token0.symbol,
               pair.token0.symbol,
               price_0_per_1,
               pair.token1.symbol);

        // 使用reserveUSD作为流动性指标
        let liquidity = pair.reserve_usd.parse::<f64>()
            .map_err(|_| anyhow!("无效的reserveUSD: {}", pair.reserve_usd))?;
        let liquidity_bd = BigDecimal::from_f64(liquidity)
            .ok_or_else(|| anyhow!("无法转换流动性为BigDecimal"))?;

        // 尝试直接更新现有边，如果不存在则添加新边
        let updated_forward = self.update_existing_edge(
            &pair.token0.symbol, 
            &pair.token1.symbol, 
            &pair.id,
            price_1_per_0.clone(),
            liquidity_bd.clone()
        );

        let updated_reverse = self.update_existing_edge(
            &pair.token1.symbol, 
            &pair.token0.symbol, 
            &pair.id,
            price_0_per_1.clone(),
            liquidity_bd.clone()
        );

        // 如果没有找到现有边，则添加新边
        if !updated_forward {
            let forward_edge = ExchangeEdge {
                pair_id: pair.id.clone(),
                from_token: pair.token0.symbol.clone(),
                to_token: pair.token1.symbol.clone(),
                dex: pair.dex.clone(),
                exchange_rate: price_1_per_0,
                liquidity: liquidity_bd.clone(),
                gas_cost: Self::estimate_gas_cost(&pair.dex),
                slippage: Self::estimate_slippage(&liquidity_bd),
                fee_percentage: Self::get_dex_fee_percentage(&pair.dex),
            };
            self.add_edge(forward_edge);
        }

        if !updated_reverse {
            let reverse_edge = ExchangeEdge {
                pair_id: pair.id.clone(),
                from_token: pair.token1.symbol.clone(),
                to_token: pair.token0.symbol.clone(),
                dex: pair.dex.clone(),
                exchange_rate: price_0_per_1,
                liquidity: liquidity_bd.clone(),
                gas_cost: Self::estimate_gas_cost(&pair.dex),
                slippage: Self::estimate_slippage(&liquidity_bd),
                fee_percentage: Self::get_dex_fee_percentage(&pair.dex),
            };
            self.add_edge(reverse_edge);
        }

        self.last_updated = Utc::now();
        info!("交易对 {} 更新完成", pair.id);
        
        Ok(())
    }

    /// 直接更新现有边的数据，避免删除重建
    /// 返回true如果找到并更新了边，false如果边不存在
    fn update_existing_edge(
        &mut self, 
        from_token: &str, 
        to_token: &str, 
        pair_id: &str,
        new_rate: BigDecimal,
        new_liquidity: BigDecimal
    ) -> bool {
        if let Some(edges) = self.adjacency_list.get_mut(from_token) {
            for edge in edges.iter_mut() {
                if edge.to_token == to_token && edge.pair_id == pair_id {
                    // 直接更新边的数据
                    edge.exchange_rate = new_rate;
                    edge.liquidity = new_liquidity.clone();
                    edge.gas_cost = Self::estimate_gas_cost(&edge.dex);
                    edge.slippage = Self::estimate_slippage(&new_liquidity);
                    edge.fee_percentage = Self::get_dex_fee_percentage(&edge.dex);
                    
                    debug!("直接更新边: {} -> {} ({}), 新汇率: {}", 
                           from_token, to_token, edge.dex, edge.exchange_rate);
                    return true;
                }
            }
        }
        false
    }

    /// 移除指定交易对的边
    fn remove_pair_edges(&mut self, pair_id: &str) {
        // 移除所有具有指定pair_id的边
        for edges in self.adjacency_list.values_mut() {
            edges.retain(|edge| edge.pair_id != pair_id);
        }
        
        // 清理空的邻接表和tokens
        let mut tokens_to_remove = Vec::new();
        for (token, edges) in &self.adjacency_list {
            if edges.is_empty() {
                tokens_to_remove.push(token.clone());
            }
        }
        
        for token in tokens_to_remove {
            self.adjacency_list.remove(&token);
            self.tokens.remove(&token);
        }
    }

    /// 移除指定的交易对
    pub fn remove_pair(&mut self, pair_id: &str) -> Result<()> {
        info!("移除交易对: {}", pair_id);
        
        self.remove_pair_edges(pair_id);
        self.last_updated = Utc::now();
        
        info!("交易对移除完成");
        Ok(())
    }

    /// 批量更新多个交易对
    pub fn update_multiple_pairs(&mut self, pairs: &[PairData]) -> Result<()> {
        info!("批量更新 {} 个交易对", pairs.len());
        
        let mut success_count = 0;
        let mut error_count = 0;
        
        for pair in pairs {
            match self.update_pair_data(pair) {
                Ok(_) => success_count += 1,
                Err(e) => {
                    warn!("更新交易对 {} 失败: {}", pair.id, e);
                    error_count += 1;
                }
            }
        }
        
        info!("批量更新完成: 成功 {}, 失败 {}", success_count, error_count);
        
        if error_count > 0 {
            warn!("部分交易对更新失败，请检查日志");
        }
        
        Ok(())
    }

    /// 验证PairData数据的有效性
    fn validate_pair_data(&self, pair: &PairData) -> Result<()> {
        if pair.id.is_empty() {
            return Err(anyhow!("交易对ID不能为空"));
        }
        
        if pair.token0.symbol.is_empty() || pair.token1.symbol.is_empty() {
            return Err(anyhow!("代币符号不能为空"));
        }
        
        if pair.token0.symbol == pair.token1.symbol {
            return Err(anyhow!("代币符号不能相同"));
        }
        
        if pair.dex.is_empty() {
            return Err(anyhow!("DEX名称不能为空"));
        }
        
        // 验证token decimals
        if pair.token0.decimals.parse::<u32>().is_err() {
            return Err(anyhow!("无效的token0 decimals格式"));
        }
        
        if pair.token1.decimals.parse::<u32>().is_err() {
            return Err(anyhow!("无效的token1 decimals格式"));
        }
        
        if pair.reserve_usd.parse::<f64>().is_err() {
            return Err(anyhow!("无效的reserveUSD格式"));
        }
        
        // 根据协议类型进行不同的验证
        if pair.protocol_type == protocol_types::AMM_V3 {
            // V3协议验证：需要sqrt_price或tick数据
            let has_sqrt_price = pair.sqrt_price.as_ref()
                .map(|s| !s.is_empty() && s != "0")
                .unwrap_or(false);
            
            let has_tick = pair.tick.as_ref()
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            
            if !has_sqrt_price && !has_tick {
                return Err(anyhow!("V3协议需要sqrt_price或tick数据"));
            }
        } else {
            // V2协议验证：需要有效的储备量数据
            if pair.reserve0.parse::<f64>().is_err() {
                return Err(anyhow!("无效的reserve0格式"));
            }
            
            if pair.reserve1.parse::<f64>().is_err() {
                return Err(anyhow!("无效的reserve1格式"));
            }
            
            let reserve0 = pair.reserve0.parse::<f64>().unwrap();
            let reserve1 = pair.reserve1.parse::<f64>().unwrap();
            
            if reserve0 <= 0.0 || reserve1 <= 0.0 {
                return Err(anyhow!("V2协议储备量必须大于0"));
            }
        }
        
        Ok(())
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
    use crate::data::pair_manager::TokenInfo;
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

    #[test]
    fn test_build_from_pair_data_v2() {
        let mut graph = ExchangeGraph::new();
        
        let pair_data = vec![
            PairData {
                id: "test_pair_v2".to_string(),
                network: "ethereum".to_string(),
                dex: "uniswap_v2".to_string(),
                protocol_type: protocol_types::AMM_V2.to_string(),
                token0: TokenInfo {
                    id: "0x1".to_string(),
                    symbol: "WETH".to_string(),
                    name: "Wrapped Ether".to_string(),
                    decimals: "18".to_string(),
                },
                token1: TokenInfo {
                    id: "0x2".to_string(),
                    symbol: "USDC".to_string(),
                    name: "USD Coin".to_string(),
                    decimals: "6".to_string(),
                },
                volume_usd: "1000000".to_string(),
                reserve_usd: "5000000".to_string(),
                tx_count: "1000".to_string(),
                reserve0: "1000000000000000000000".to_string(), // 1000 WETH (18 decimals)
                reserve1: "2000000000".to_string(), // 2000 USDC (6 decimals)
                fee_tier: "3000".to_string(),
                sqrt_price: None,
                tick: None,
            }
        ];
        
        let result = graph.from_pair_data(&pair_data);
        assert!(result.is_ok());
        
        // 验证图构建结果
        assert_eq!(graph.tokens.len(), 2);
        assert!(graph.tokens.contains("WETH"));
        assert!(graph.tokens.contains("USDC"));
        
        // 验证边的存在
        let weth_edges = graph.get_edges_from("WETH");
        assert!(weth_edges.is_some());
        assert_eq!(weth_edges.unwrap().len(), 1);
        
        let usdc_edges = graph.get_edges_from("USDC");
        assert!(usdc_edges.is_some());
        assert_eq!(usdc_edges.unwrap().len(), 1);
        
        // 验证汇率计算 (V2: reserve1/reserve0 adjusted for decimals)
        let weth_to_usdc = &weth_edges.unwrap()[0];
        assert_eq!(weth_to_usdc.to_token, "USDC");
        // 预期价格应该是 2000 USDC / 1000 WETH = 2 USDC per WETH
        assert_eq!(weth_to_usdc.exchange_rate, BigDecimal::from_str("2").unwrap());
    }

    #[test]
    fn test_build_from_pair_data_v3() {
        let mut graph = ExchangeGraph::new();
        
        let pair_data = vec![
            PairData {
                id: "test_pair_v3".to_string(),
                network: "ethereum".to_string(),
                dex: "uniswap_v3".to_string(),
                protocol_type: protocol_types::AMM_V3.to_string(),
                token0: TokenInfo {
                    id: "0x1".to_string(),
                    symbol: "WETH".to_string(),
                    name: "Wrapped Ether".to_string(),
                    decimals: "18".to_string(),
                },
                token1: TokenInfo {
                    id: "0x2".to_string(),
                    symbol: "USDC".to_string(),
                    name: "USD Coin".to_string(),
                    decimals: "6".to_string(),
                },
                volume_usd: "1000000".to_string(),
                reserve_usd: "5000000".to_string(),
                tx_count: "1000".to_string(),
                reserve0: "0".to_string(), // V3 不使用储备量
                reserve1: "0".to_string(),
                fee_tier: "3000".to_string(),
                sqrt_price: Some("79228162514264337593543950336".to_string()), // Q96 format, price = 1
                tick: None,
            }
        ];
        
        let result = graph.from_pair_data(&pair_data);
        assert!(result.is_ok());
        
        // 验证图构建结果
        assert_eq!(graph.tokens.len(), 2);
        assert!(graph.tokens.contains("WETH"));
        assert!(graph.tokens.contains("USDC"));
        
        // 验证边的存在
        let weth_edges = graph.get_edges_from("WETH");
        assert!(weth_edges.is_some());
        assert_eq!(weth_edges.unwrap().len(), 1);
        
        let usdc_edges = graph.get_edges_from("USDC");
        assert!(usdc_edges.is_some());
        assert_eq!(usdc_edges.unwrap().len(), 1);
        
        // 验证V3价格计算
        let weth_to_usdc = &weth_edges.unwrap()[0];
        assert_eq!(weth_to_usdc.to_token, "USDC");
        // sqrt_price = 2^96 表示价格为1，但需要考虑decimals差异
        // 实际价格应该是 1 * 10^(6-18) = 1e-12
        assert!(weth_to_usdc.exchange_rate > BigDecimal::from(0));
    }

    #[test]
    fn test_validate_pair_data_v2() {
        let graph = ExchangeGraph::new();
        
        // 有效的V2数据
        let valid_v2_pair = PairData {
            id: "valid_v2".to_string(),
            network: "ethereum".to_string(),
            dex: "uniswap_v2".to_string(),
            protocol_type: protocol_types::AMM_V2.to_string(),
            token0: TokenInfo {
                id: "0x1".to_string(),
                symbol: "WETH".to_string(),
                name: "Wrapped Ether".to_string(),
                decimals: "18".to_string(),
            },
            token1: TokenInfo {
                id: "0x2".to_string(),
                symbol: "USDC".to_string(),
                name: "USD Coin".to_string(),
                decimals: "6".to_string(),
            },
            volume_usd: "1000000".to_string(),
            reserve_usd: "5000000".to_string(),
            tx_count: "1000".to_string(),
            reserve0: "1000000000000000000000".to_string(),
            reserve1: "2000000000".to_string(),
            fee_tier: "3000".to_string(),
            sqrt_price: None,
            tick: None,
        };
        
        assert!(graph.validate_pair_data(&valid_v2_pair).is_ok());
        
        // 无效的V2数据（零储备量）
        let mut invalid_v2_pair = valid_v2_pair.clone();
        invalid_v2_pair.reserve0 = "0".to_string();
        
        assert!(graph.validate_pair_data(&invalid_v2_pair).is_err());
    }

    #[test]
    fn test_validate_pair_data_v3() {
        let graph = ExchangeGraph::new();
        
        // 有效的V3数据（有sqrt_price）
        let valid_v3_pair = PairData {
            id: "valid_v3".to_string(),
            network: "ethereum".to_string(),
            dex: "uniswap_v3".to_string(),
            protocol_type: protocol_types::AMM_V3.to_string(),
            token0: TokenInfo {
                id: "0x1".to_string(),
                symbol: "WETH".to_string(),
                name: "Wrapped Ether".to_string(),
                decimals: "18".to_string(),
            },
            token1: TokenInfo {
                id: "0x2".to_string(),
                symbol: "USDC".to_string(),
                name: "USD Coin".to_string(),
                decimals: "6".to_string(),
            },
            volume_usd: "1000000".to_string(),
            reserve_usd: "5000000".to_string(),
            tx_count: "1000".to_string(),
            reserve0: "0".to_string(),
            reserve1: "0".to_string(),
            fee_tier: "3000".to_string(),
            sqrt_price: Some("79228162514264337593543950336".to_string()),
            tick: None,
        };
        
        assert!(graph.validate_pair_data(&valid_v3_pair).is_ok());
        
        // 无效的V3数据（没有sqrt_price和tick）
        let mut invalid_v3_pair = valid_v3_pair.clone();
        invalid_v3_pair.sqrt_price = None;
        invalid_v3_pair.tick = None;
        
        assert!(graph.validate_pair_data(&invalid_v3_pair).is_err());
        
        // 有效的V3数据（有tick）
        let mut valid_v3_with_tick = invalid_v3_pair.clone();
        valid_v3_with_tick.tick = Some("1000".to_string());
        
        assert!(graph.validate_pair_data(&valid_v3_with_tick).is_ok());
    }

    #[test]
    fn test_update_pair_data() {
        let mut graph = ExchangeGraph::new();
        
        // 创建初始交易对数据
        let initial_pair = PairData {
            id: "test_pair_update".to_string(),
            network: "ethereum".to_string(),
            dex: "uniswap_v2".to_string(),
            protocol_type: protocol_types::AMM_V2.to_string(),
            token0: TokenInfo {
                id: "0x1".to_string(),
                symbol: "WETH".to_string(),
                name: "Wrapped Ether".to_string(),
                decimals: "18".to_string(),
            },
            token1: TokenInfo {
                id: "0x2".to_string(),
                symbol: "USDC".to_string(),
                name: "USD Coin".to_string(),
                decimals: "6".to_string(),
            },
            volume_usd: "1000000".to_string(),
            reserve_usd: "5000000".to_string(),
            tx_count: "1000".to_string(),
            reserve0: "1000000000000000000000".to_string(), // 1000 WETH
            reserve1: "2000000000".to_string(), // 2000 USDC
            fee_tier: "3000".to_string(),
            sqrt_price: None,
            tick: None,
        };
        
        // 添加初始交易对
        let result = graph.update_pair_data(&initial_pair);
        assert!(result.is_ok());
        
        // 验证初始状态
        assert_eq!(graph.tokens.len(), 2);
        assert!(graph.tokens.contains("WETH"));
        assert!(graph.tokens.contains("USDC"));
        
        let weth_edges = graph.get_edges_from("WETH").unwrap();
        assert_eq!(weth_edges.len(), 1);
        let initial_rate = weth_edges[0].exchange_rate.clone();
        
        // 创建更新后的交易对数据（价格变化）
        let mut updated_pair = initial_pair.clone();
        updated_pair.reserve0 = "1000000000000000000000".to_string(); // 1000 WETH
        updated_pair.reserve1 = "3000000000".to_string(); // 3000 USDC (价格从2变为3)
        updated_pair.reserve_usd = "6000000".to_string(); // 流动性增加
        
        // 更新交易对
        let result = graph.update_pair_data(&updated_pair);
        assert!(result.is_ok());
        
        // 验证更新后状态
        assert_eq!(graph.tokens.len(), 2); // 代币数量不变
        
        let updated_weth_edges = graph.get_edges_from("WETH").unwrap();
        assert_eq!(updated_weth_edges.len(), 1);
        let updated_rate = &updated_weth_edges[0].exchange_rate;
        
        // 验证价格已更新
        assert_ne!(initial_rate, *updated_rate);
        assert_eq!(*updated_rate, BigDecimal::from_str("3").unwrap());
        
        // 验证流动性已更新
        assert_eq!(updated_weth_edges[0].liquidity, BigDecimal::from_str("6000000").unwrap());
    }

    #[test]
    fn test_remove_pair() {
        let mut graph = ExchangeGraph::new();
        
        // 添加两个交易对
        let pair1 = PairData {
            id: "pair1".to_string(),
            network: "ethereum".to_string(),
            dex: "uniswap_v2".to_string(),
            protocol_type: protocol_types::AMM_V2.to_string(),
            token0: TokenInfo {
                id: "0x1".to_string(),
                symbol: "WETH".to_string(),
                name: "Wrapped Ether".to_string(),
                decimals: "18".to_string(),
            },
            token1: TokenInfo {
                id: "0x2".to_string(),
                symbol: "USDC".to_string(),
                name: "USD Coin".to_string(),
                decimals: "6".to_string(),
            },
            volume_usd: "1000000".to_string(),
            reserve_usd: "5000000".to_string(),
            tx_count: "1000".to_string(),
            reserve0: "1000000000000000000000".to_string(),
            reserve1: "2000000000".to_string(),
            fee_tier: "3000".to_string(),
            sqrt_price: None,
            tick: None,
        };
        
        let pair2 = PairData {
            id: "pair2".to_string(),
            network: "ethereum".to_string(),
            dex: "sushiswap".to_string(),
            protocol_type: protocol_types::AMM_V2.to_string(),
            token0: TokenInfo {
                id: "0x1".to_string(),
                symbol: "WETH".to_string(),
                name: "Wrapped Ether".to_string(),
                decimals: "18".to_string(),
            },
            token1: TokenInfo {
                id: "0x3".to_string(),
                symbol: "DAI".to_string(),
                name: "Dai Stablecoin".to_string(),
                decimals: "18".to_string(),
            },
            volume_usd: "500000".to_string(),
            reserve_usd: "2500000".to_string(),
            tx_count: "500".to_string(),
            reserve0: "1000000000000000000000".to_string(),
            reserve1: "2000000000000000000000000".to_string(),
            fee_tier: "3000".to_string(),
            sqrt_price: None,
            tick: None,
        };
        
        // 添加两个交易对
        assert!(graph.update_pair_data(&pair1).is_ok());
        assert!(graph.update_pair_data(&pair2).is_ok());
        
        // 验证初始状态
        assert_eq!(graph.tokens.len(), 3); // WETH, USDC, DAI
        let weth_edges = graph.get_edges_from("WETH").unwrap();
        assert_eq!(weth_edges.len(), 2); // WETH -> USDC, WETH -> DAI
        
        // 移除第一个交易对
        let result = graph.remove_pair("pair1");
        assert!(result.is_ok());
        
        // 验证移除后状态
        assert_eq!(graph.tokens.len(), 2); // WETH, DAI (USDC被移除)
        assert!(!graph.tokens.contains("USDC"));
        
        let weth_edges_after = graph.get_edges_from("WETH").unwrap();
        assert_eq!(weth_edges_after.len(), 1); // 只剩 WETH -> DAI
        assert_eq!(weth_edges_after[0].to_token, "DAI");
        
        // 验证USDC相关的边都被移除
        assert!(graph.get_edges_from("USDC").is_none());
    }

    #[test]
    fn test_update_multiple_pairs() {
        let mut graph = ExchangeGraph::new();
        
        let pairs = vec![
            PairData {
                id: "pair1".to_string(),
                network: "ethereum".to_string(),
                dex: "uniswap_v2".to_string(),
                protocol_type: protocol_types::AMM_V2.to_string(),
                token0: TokenInfo {
                    id: "0x1".to_string(),
                    symbol: "WETH".to_string(),
                    name: "Wrapped Ether".to_string(),
                    decimals: "18".to_string(),
                },
                token1: TokenInfo {
                    id: "0x2".to_string(),
                    symbol: "USDC".to_string(),
                    name: "USD Coin".to_string(),
                    decimals: "6".to_string(),
                },
                volume_usd: "1000000".to_string(),
                reserve_usd: "5000000".to_string(),
                tx_count: "1000".to_string(),
                reserve0: "1000000000000000000000".to_string(),
                reserve1: "2000000000".to_string(),
                fee_tier: "3000".to_string(),
                sqrt_price: None,
                tick: None,
            },
            PairData {
                id: "pair2".to_string(),
                network: "ethereum".to_string(),
                dex: "uniswap_v3".to_string(),
                protocol_type: protocol_types::AMM_V3.to_string(),
                token0: TokenInfo {
                    id: "0x2".to_string(),
                    symbol: "USDC".to_string(),
                    name: "USD Coin".to_string(),
                    decimals: "6".to_string(),
                },
                token1: TokenInfo {
                    id: "0x3".to_string(),
                    symbol: "DAI".to_string(),
                    name: "Dai Stablecoin".to_string(),
                    decimals: "18".to_string(),
                },
                volume_usd: "500000".to_string(),
                reserve_usd: "2500000".to_string(),
                tx_count: "500".to_string(),
                reserve0: "0".to_string(),
                reserve1: "0".to_string(),
                fee_tier: "500".to_string(),
                sqrt_price: Some("79228162514264337593543950336".to_string()),
                tick: None,
            },
        ];
        
        // 批量更新
        let result = graph.update_multiple_pairs(&pairs);
        assert!(result.is_ok());
        
        // 验证结果
        assert_eq!(graph.tokens.len(), 3); // WETH, USDC, DAI
        assert!(graph.tokens.contains("WETH"));
        assert!(graph.tokens.contains("USDC"));
        assert!(graph.tokens.contains("DAI"));
        
        // 验证边的存在
        assert!(graph.get_edges_from("WETH").is_some());
        assert!(graph.get_edges_from("USDC").is_some());
        assert!(graph.get_edges_from("DAI").is_some());
        
        let usdc_edges = graph.get_edges_from("USDC").unwrap();
        assert_eq!(usdc_edges.len(), 2); // USDC -> WETH, USDC -> DAI
    }

    #[test]
    fn test_direct_update_vs_rebuild() {
        let mut graph = ExchangeGraph::new();
        
        // 创建初始交易对数据
        let initial_pair = PairData {
            id: "test_pair_1".to_string(),
            token0: TokenInfo {
                id: "token0_id".to_string(),
                symbol: "USDC".to_string(),
                name: "USD Coin".to_string(),
                decimals: "6".to_string(),
            },
            token1: TokenInfo {
                id: "token1_id".to_string(),
                symbol: "WETH".to_string(),
                name: "Wrapped Ether".to_string(),
                decimals: "18".to_string(),
            },
            reserve0: "1000000000000".to_string(), // 1M USDC
            reserve1: "500000000000000000000".to_string(), // 500 WETH
            reserve_usd: "2000000.0".to_string(),
            dex: "uniswap_v2".to_string(),
            protocol_type: "v2".to_string(),
            network: "ethereum".to_string(),
            volume_usd: "100000.0".to_string(),
            tx_count: "1000".to_string(),
            fee_tier: "3000".to_string(),
            sqrt_price: None,
            tick: None,
        };

        // 首次添加交易对
        graph.update_pair_data(&initial_pair).unwrap();
        
        // 验证初始状态
        let initial_rate = {
            let initial_edges = graph.get_edges_from("USDC").unwrap();
            assert_eq!(initial_edges.len(), 1);
            initial_edges[0].exchange_rate.clone()
        };
        
        // 创建更新后的交易对数据（价格变化）
        let updated_pair = PairData {
            reserve0: "1200000000000".to_string(), // 1.2M USDC
            reserve1: "400000000000000000000".to_string(), // 400 WETH
            reserve_usd: "2400000.0".to_string(),
            ..initial_pair.clone()
        };

        // 使用直接更新方法
        graph.update_pair_data(&updated_pair).unwrap();
        
        // 验证更新后的状态
        let updated_edges = graph.get_edges_from("USDC").unwrap();
        assert_eq!(updated_edges.len(), 1); // 应该还是只有一条边
        let updated_rate = &updated_edges[0].exchange_rate;
        
        // 价格应该发生变化
        assert_ne!(&initial_rate, updated_rate);
        
        // 验证反向边也被正确更新
        let reverse_edges = graph.get_edges_from("WETH").unwrap();
        assert_eq!(reverse_edges.len(), 1);
        
        // 验证流动性也被更新
        assert_eq!(updated_edges[0].liquidity.to_string(), "2400000");
    }

    #[test]
    fn test_update_pair_with_invalid_data() {
        let mut graph = ExchangeGraph::new();
        
        // 创建无效的交易对数据（零储备量的V2）
        let invalid_pair = PairData {
            id: "invalid_pair".to_string(),
            network: "ethereum".to_string(),
            dex: "uniswap_v2".to_string(),
            protocol_type: protocol_types::AMM_V2.to_string(),
            token0: TokenInfo {
                id: "0x1".to_string(),
                symbol: "WETH".to_string(),
                name: "Wrapped Ether".to_string(),
                decimals: "18".to_string(),
            },
            token1: TokenInfo {
                id: "0x2".to_string(),
                symbol: "USDC".to_string(),
                name: "USD Coin".to_string(),
                decimals: "6".to_string(),
            },
            volume_usd: "1000000".to_string(),
            reserve_usd: "5000000".to_string(),
            tx_count: "1000".to_string(),
            reserve0: "0".to_string(), // 无效：V2协议储备量为零
            reserve1: "2000000000".to_string(),
            fee_tier: "3000".to_string(),
            sqrt_price: None,
            tick: None,
        };
        
        // 尝试更新无效数据
        let result = graph.update_pair_data(&invalid_pair);
        assert!(result.is_err());
        
        // 验证图没有被修改
        assert_eq!(graph.tokens.len(), 0);
        assert!(graph.adjacency_list.is_empty());
    }
}