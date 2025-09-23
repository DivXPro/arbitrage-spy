use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive, Zero};
use anyhow::{Result, anyhow};
use log::{info, warn, debug};
use chrono::{DateTime, Utc};
use tokio::sync::mpsc;
use crate::store::pair_manager::PairData;
use crate::price_calculator::PriceCalculator;
use crate::config::protocol_types;
use crate::event_listener::{EventListener, RawEventData};
use super::exchange_edge::ExchangeEdge;
use super::arbitrage_path::ArbitragePath;

/// 价格图，用于存储所有代币间的交换关系
pub struct ExchangeGraph {
    /// 统一管理所有交易对数据
    pub event_listener: Arc<Mutex<EventListener>>,
    pub pairs: HashMap<String, Arc<PairData>>,              // pair_id -> PairData
    /// 邻接表：token -> [(to_token, edge)]
    pub adjacency_list: HashMap<String, Vec<ExchangeEdge>>, // 代币交换关系的邻接表
    pub tokens: HashSet<String>,                             // 所有代币符号的集合
    pub last_updated: DateTime<Utc>,         // 最后更新时间
    /// EventListener的sender，用于发送交易对信息
    pub event_sender: Option<mpsc::Sender<RawEventData>>,
}

impl ExchangeGraph {
    pub fn new(event_listener: Arc<Mutex<EventListener>>) -> Self {
        Self {
            event_listener,
            pairs: HashMap::new(),
            adjacency_list: HashMap::new(),
            tokens: HashSet::new(),
            last_updated: Utc::now(),
            event_sender: None,
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

    /// 通过pair_id获取PairData
    pub fn get_pair_data(&self, pair_id: &str) -> Option<Arc<PairData>> {
        self.pairs.get(pair_id).cloned()
    }

    /// 从PairData数据构建图
    /// 添加单个交易对数据到图中
    /// 返回 (是否成功添加, 添加的边数量)
    fn add_pair(&mut self, pair: &PairData) -> (bool, usize) {
        // 验证交易对数据
        if let Err(e) = self.validate_pair_data(pair) {
            warn!("跳过无效交易对数据 {}: {}", pair.id, e);
            return (false, 0);
        }

        // 使用ExchangeEdge的create_bidirectional_edges方法创建双向边
        match ExchangeEdge::create_bidirectional_edges(pair) {
            Ok((forward_edge, reverse_edge)) => {
                debug!("交易对 {} ({}) 成功创建双向边: {} -> {} (汇率: {}), {} -> {} (汇率: {})", 
                       pair.id, 
                       pair.protocol_type,
                       forward_edge.from_token,
                       forward_edge.to_token,
                       forward_edge.exchange_rate,
                       reverse_edge.from_token,
                       reverse_edge.to_token,
                       reverse_edge.exchange_rate);

                // 验证边的有效性
                if let Err(e) = forward_edge.validate() {
                    warn!("跳过无效的正向边 {}: {}", pair.id, e);
                    return (false, 0);
                }
                
                if let Err(e) = reverse_edge.validate() {
                    warn!("跳过无效的反向边 {}: {}", pair.id, e);
                    return (false, 0);
                }

                // 添加边到图中
                self.add_edge(forward_edge);
                self.add_edge(reverse_edge);
                
                // 存储 pair 数据
                self.pairs.insert(pair.id.clone(), Arc::new(pair.clone()));
                
                if let Ok(mut listener) = self.event_listener.lock() {
                    let _ = listener.add_pair(pair.clone());
                }
                
                (true, 2)
            }
            Err(e) => {
                warn!("跳过创建边失败的交易对 {} => {} : {}", pair.token0.symbol, pair.token1.symbol, e);
                (false, 0)
            }
        }
    }

    pub async fn from_pair_data(
        &mut self, 
        pair_data: &[PairData],
    ) -> Result<()> {
        info!("开始从PairData构建价格图，交易对数量: {}", pair_data.len());
        
        // 清空现有数据
        self.adjacency_list.clear();
        self.tokens.clear();
        self.pairs.clear();
        
        let mut edge_count = 0;
        let mut skipped_count = 0;
        
        for pair in pair_data {
            let (success, added_edges) = self.add_pair(pair);
            if success {
                edge_count += added_edges;
            } else {
                skipped_count += 1;
            }
        }

        self.last_updated = Utc::now();
        info!("从PairData构建价格图完成，代币数量: {}, 边数量: {}, 跳过: {}", 
              self.tokens.len(), edge_count, skipped_count);
        
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
        
        // 验证反向价格是否合理
        let min_price = BigDecimal::from_str("1e-18").unwrap();
        let max_price = BigDecimal::from_str("1e18").unwrap();
        
        if price_0_per_1 < min_price || price_0_per_1 > max_price {
            return Err(anyhow!("异常反向价格的交易对 {}: 原价格={}, 反向价格={}", 
                              pair.id, price_1_per_0, price_0_per_1));
        }
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
        
        // 验证反向价格是否合理
        let min_price = BigDecimal::from_str("1e-18").unwrap();
        let max_price = BigDecimal::from_str("1e18").unwrap();
        
        if price_0_per_1 < min_price || price_0_per_1 > max_price {
            return Err(anyhow!("异常反向价格的交易对 {}: 原价格={}, 反向价格={}", 
                              pair.id, price_1_per_0, price_0_per_1));
        }
        
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

        // 更新pairs字段中的PairData
        self.pairs.insert(pair.id.clone(), Arc::new(pair.clone()));
        
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
        
        // 移除边
        self.remove_pair_edges(pair_id);
        
        // 从pairs字段中移除PairData
        self.pairs.remove(pair_id);
        
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
        self.pairs.clear();
        self.adjacency_list.clear();
        self.tokens.clear();
    }

    /// 寻找套利路径
    /// 
    /// # 参数
    /// * `start_token` - 起始代币符号
    /// * `max_depth` - 最大路径深度（节点数）
    /// * `min_profit_threshold` - 最小盈利阈值（百分比，如0.01表示1%）
    /// 
    /// # 返回
    /// * `Vec<ArbitragePath>` - 找到的套利路径列表，按盈利率排序
    pub fn find_arbitrage_paths(
        &self,
        start_token: &str,
        max_depth: usize,
        min_profit_threshold: f64,
    ) -> Vec<ArbitragePath> {
        if max_depth < 3 {
            warn!("套利路径至少需要3个节点，当前设置: {}", max_depth);
            return Vec::new();
        }

        if !self.tokens.contains(start_token) {
            warn!("起始代币 {} 不存在于图中", start_token);
            return Vec::new();
        }

        let mut arbitrage_paths = Vec::new();
        let mut visited = HashSet::new();
        let mut current_path = Vec::new();

        info!("开始寻找从 {} 出发的套利路径，最大深度: {}, 最小盈利阈值: {}%", 
              start_token, max_depth, min_profit_threshold * 100.0);
        
        println!("🔍 正在搜索套利路径...");

        // 使用深度优先搜索寻找套利路径
        self.dfs_arbitrage_paths(
            start_token,
            start_token,
            BigDecimal::from(1), // 初始金额为1
            max_depth,
            min_profit_threshold,
            &mut visited,
            &mut current_path,
            &mut arbitrage_paths,
        );

        // 按盈利率降序排序
        arbitrage_paths.sort_by(|a, b| b.profit_rate.partial_cmp(&a.profit_rate).unwrap_or(std::cmp::Ordering::Equal));

        println!("✅ 搜索完成！总共找到 {} 条套利路径", arbitrage_paths.len());
        info!("找到 {} 条套利路径", arbitrage_paths.len());
        arbitrage_paths
    }

    /// 深度优先搜索套利路径
    fn dfs_arbitrage_paths(
        &self,
        current_token: &str,
        start_token: &str,
        current_amount: BigDecimal,
        remaining_depth: usize,
        min_profit_threshold: f64,
        visited: &mut HashSet<String>,
        current_path: &mut Vec<ExchangeEdge>,
        arbitrage_paths: &mut Vec<ArbitragePath>,
    ) {
        // 如果已经访问过当前代币（除了起始代币），跳过以避免无限循环
        if visited.contains(current_token) && current_token != start_token {
            return;
        }

        // 如果路径长度达到3且回到起始代币，检查是否有套利机会
        if current_path.len() >= 2 && current_token == start_token {
            if let Some(arbitrage_path) = self.evaluate_arbitrage_path(
                current_path,
                &current_amount,
                min_profit_threshold,
            ) {
                // 先获取路径信息用于显示
                let path_chain = arbitrage_path.format_path_chain();
                let profit_rate = arbitrage_path.profit_rate;
                
                arbitrage_paths.push(arbitrage_path);
                
                // 每找到10条路径就显示一次进度
                let path_count = arbitrage_paths.len();
                if path_count % 10 == 0 {
                    println!("已找到 {} 条套利路径...", path_count);
                } else if path_count <= 5 {
                    // 前5条路径每找到一条就显示
                    println!("💰 发现第 {} 条套利路径: {} (盈利率: {:.2}%)", 
                             path_count, 
                             path_chain,
                             profit_rate * 100.0);
                }
            }
            return;
        }

        // 如果达到最大深度，停止搜索
        if remaining_depth == 0 {
            return;
        }

        // 标记当前代币为已访问
        if current_token != start_token {
            visited.insert(current_token.to_string());
        }

        // 探索从当前代币出发的所有边
        if let Some(edges) = self.adjacency_list.get(current_token) {
            for edge in edges {
                // 计算通过这条边后的金额
                let next_amount = self.calculate_amount_after_trade(&current_amount, edge);
                
                // 添加边到当前路径
                current_path.push(edge.clone());

                // 递归搜索
                self.dfs_arbitrage_paths(
                    &edge.to_token,
                    start_token,
                    next_amount,
                    remaining_depth - 1,
                    min_profit_threshold,
                    visited,
                    current_path,
                    arbitrage_paths,
                );

                // 回溯：移除边
                current_path.pop();
            }
        }

        // 回溯：移除访问标记
        if current_token != start_token {
            visited.remove(current_token);
        }
    }

    /// 评估套利路径的盈利性
    fn evaluate_arbitrage_path(
        &self,
        path: &[ExchangeEdge],
        final_amount: &BigDecimal,
        min_profit_threshold: f64,
    ) -> Option<ArbitragePath> {
        if path.is_empty() {
            return None;
        }

        let initial_amount = BigDecimal::from(1);
        let profit = final_amount - &initial_amount;
        let profit_rate = profit.to_f64().unwrap_or(0.0);

        // 检查是否满足最小盈利阈值
        if profit_rate < min_profit_threshold {
            return None;
        }

        // 计算总的Gas成本
        let total_gas_cost: BigDecimal = path.iter()
            .map(|edge| &edge.gas_cost)
            .sum();

        // 计算总的交易费用
        let total_fee_cost = self.calculate_total_fees(path, &initial_amount);

        // 计算净盈利（扣除Gas和手续费）
        let net_profit = &profit - &total_gas_cost - &total_fee_cost;
        let net_profit_rate = net_profit.to_f64().unwrap_or(0.0);

        // 检查净盈利是否仍然满足阈值
        if net_profit_rate < min_profit_threshold {
            return None;
        }

        // 计算路径风险评分
        let risk_score = self.calculate_path_risk(path);

        Some(ArbitragePath {
            edges: path.to_vec(),
            initial_amount: initial_amount.clone(),
            final_amount: final_amount.clone(),
            profit: profit.clone(),
            profit_rate,
            net_profit: net_profit.clone(),
            net_profit_rate,
            total_gas_cost: total_gas_cost.clone(),
            total_fee_cost: total_fee_cost.clone(),
            risk_score,
            estimated_execution_time: self.estimate_execution_time(path),
        })
    }

    /// 计算交易后的金额（考虑汇率、滑点和手续费）
    fn calculate_amount_after_trade(&self, input_amount: &BigDecimal, edge: &ExchangeEdge) -> BigDecimal {
        // 基础汇率计算
        let mut output_amount = input_amount * &edge.exchange_rate;

        // 扣除交易手续费
        let fee_amount = &output_amount * BigDecimal::from_f64(edge.fee_percentage / 100.0).unwrap_or_default();
        output_amount = output_amount - fee_amount;

        // 考虑滑点影响
        let slippage_impact = &output_amount * BigDecimal::from_f64(edge.slippage / 100.0).unwrap_or_default();
        output_amount = output_amount - slippage_impact;

        // 确保金额不为负数
        if output_amount < BigDecimal::zero() {
            BigDecimal::zero()
        } else {
            output_amount
        }
    }

    /// 计算路径的总手续费
    fn calculate_total_fees(&self, path: &[ExchangeEdge], initial_amount: &BigDecimal) -> BigDecimal {
        let mut current_amount = initial_amount.clone();
        let mut total_fees = BigDecimal::zero();

        for edge in path {
            let trade_amount = &current_amount * &edge.exchange_rate;
            let fee = &trade_amount * BigDecimal::from_f64(edge.fee_percentage / 100.0).unwrap_or_default();
            total_fees = total_fees + fee;
            current_amount = self.calculate_amount_after_trade(&current_amount, edge);
        }

        total_fees
    }

    /// 计算路径风险评分（0-100，越低越好）
    fn calculate_path_risk(&self, path: &[ExchangeEdge]) -> f64 {
        let mut risk_score = 0.0;

        for edge in path {
            // 流动性风险：流动性越低，风险越高
            let liquidity_usd = edge.liquidity.to_f64().unwrap_or(0.0);
            let liquidity_risk = if liquidity_usd > 1_000_000.0 {
                0.0
            } else if liquidity_usd > 100_000.0 {
                10.0
            } else if liquidity_usd > 10_000.0 {
                25.0
            } else {
                50.0
            };

            // 滑点风险
            let slippage_risk = edge.slippage * 10.0; // 滑点越高，风险越高

            // DEX风险（不同DEX的可靠性不同）
            let dex_risk = match edge.dex.as_str() {
                "uniswap_v2" | "uniswap_v3" => 0.0,
                "sushiswap" | "pancakeswap" => 5.0,
                _ => 15.0,
            };

            risk_score += liquidity_risk + slippage_risk + dex_risk;
        }

        // 路径长度风险：路径越长，风险越高
        let path_length_risk = (path.len() as f64 - 2.0) * 5.0;
        risk_score += path_length_risk;

        risk_score.min(100.0) // 最大风险评分为100
    }

    /// 估算路径执行时间（秒）
    fn estimate_execution_time(&self, path: &[ExchangeEdge]) -> f64 {
        // 基础执行时间：每个交易约15秒（以太坊区块时间）
        let base_time = path.len() as f64 * 15.0;

        // 不同DEX的执行时间差异
        let dex_time_factor: f64 = path.iter()
            .map(|edge| match edge.dex.as_str() {
                "uniswap_v2" => 1.0,
                "uniswap_v3" => 1.2, // V3稍微复杂一些
                "sushiswap" => 1.1,
                _ => 1.5,
            })
            .sum::<f64>() / path.len() as f64;

        base_time * dex_time_factor
    }

    /// 寻找最优套利路径（限制返回数量）
    pub fn find_best_arbitrage_paths(
        &self,
        start_token: &str,
        max_depth: usize,
        min_profit_threshold: f64,
        max_results: usize,
    ) -> Vec<ArbitragePath> {
        let mut paths = self.find_arbitrage_paths(start_token, max_depth, min_profit_threshold);
        
        // 按综合评分排序（考虑盈利率和风险）
        paths.sort_by(|a, b| {
            let score_a = a.net_profit_rate - (a.risk_score / 1000.0); // 风险权重
            let score_b = b.net_profit_rate - (b.risk_score / 1000.0);
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        paths.into_iter().take(max_results).collect()
    }

    /// 获取所有可能的套利起始代币
    pub fn get_arbitrage_candidates(&self, min_liquidity: f64) -> Vec<String> {
        let mut candidates = Vec::new();

        for token in &self.tokens {
            if let Some(edges) = self.adjacency_list.get(token) {
                // 检查是否有足够的流动性和连接
                let total_liquidity: f64 = edges.iter()
                    .map(|edge| edge.liquidity.to_f64().unwrap_or(0.0))
                    .sum();

                if total_liquidity >= min_liquidity && edges.len() >= 2 {
                    candidates.push(token.clone());
                }
            }
        }

        // 按流动性排序
        candidates.sort_by(|a, b| {
            let liquidity_a: f64 = self.adjacency_list.get(a)
                .map(|edges| edges.iter().map(|e| e.liquidity.to_f64().unwrap_or(0.0)).sum())
                .unwrap_or(0.0);
            let liquidity_b: f64 = self.adjacency_list.get(b)
                .map(|edges| edges.iter().map(|e| e.liquidity.to_f64().unwrap_or(0.0)).sum())
                .unwrap_or(0.0);
            liquidity_b.partial_cmp(&liquidity_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        candidates
     }

    /// 高级套利路径搜索（支持多种优化策略）
    pub fn find_optimized_arbitrage_paths(
        &self,
        start_token: &str,
        max_depth: usize,
        min_profit_threshold: f64,
        optimization_strategy: OptimizationStrategy,
        max_results: usize,
    ) -> Vec<ArbitragePath> {
        let mut paths = self.find_arbitrage_paths(start_token, max_depth, min_profit_threshold);
        
        // 应用优化策略排序
        match optimization_strategy {
            OptimizationStrategy::MaxProfit => {
                paths.sort_by(|a, b| b.net_profit_rate.partial_cmp(&a.net_profit_rate).unwrap_or(std::cmp::Ordering::Equal));
            },
            OptimizationStrategy::MinRisk => {
                paths.sort_by(|a, b| a.risk_score.partial_cmp(&b.risk_score).unwrap_or(std::cmp::Ordering::Equal));
            },
            OptimizationStrategy::Balanced => {
                paths.sort_by(|a, b| {
                    let score_a = a.net_profit_rate - (a.risk_score / 100.0);
                    let score_b = b.net_profit_rate - (b.risk_score / 100.0);
                    score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
                });
            },
            OptimizationStrategy::FastExecution => {
                paths.sort_by(|a, b| a.estimated_execution_time.partial_cmp(&b.estimated_execution_time).unwrap_or(std::cmp::Ordering::Equal));
            },
            OptimizationStrategy::HighLiquidity => {
                paths.sort_by(|a, b| {
                    let liquidity_a: f64 = a.edges.iter().map(|e| e.liquidity.to_f64().unwrap_or(0.0)).sum();
                    let liquidity_b: f64 = b.edges.iter().map(|e| e.liquidity.to_f64().unwrap_or(0.0)).sum();
                    liquidity_b.partial_cmp(&liquidity_a).unwrap_or(std::cmp::Ordering::Equal)
                });
            },
        }

        paths.into_iter().take(max_results).collect()
    }

    /// 批量分析多个代币的套利机会
    pub fn analyze_multiple_tokens_arbitrage(
        &self,
        tokens: &[String],
        max_depth: usize,
        min_profit_threshold: f64,
        max_results_per_token: usize,
    ) -> HashMap<String, Vec<ArbitragePath>> {
        let mut results = HashMap::new();

        for token in tokens {
            if self.tokens.contains(token) {
                let paths = self.find_best_arbitrage_paths(
                    token,
                    max_depth,
                    min_profit_threshold,
                    max_results_per_token,
                );
                if !paths.is_empty() {
                    results.insert(token.clone(), paths);
                }
            }
        }

        info!("分析了{}个代币，找到{}个有套利机会的代币", tokens.len(), results.len());
        results
    }

    /// 寻找跨DEX套利机会（专门寻找涉及不同DEX的路径）
    pub fn find_cross_dex_arbitrage(
        &self,
        start_token: &str,
        max_depth: usize,
        min_profit_threshold: f64,
        max_results: usize,
    ) -> Vec<ArbitragePath> {
        let all_paths = self.find_arbitrage_paths(start_token, max_depth, min_profit_threshold);
        
        // 过滤出跨DEX的路径
        let cross_dex_paths: Vec<ArbitragePath> = all_paths.into_iter()
            .filter(|path| {
                let dexes: HashSet<String> = path.edges.iter()
                    .map(|edge| edge.dex.clone())
                    .collect();
                dexes.len() > 1 // 涉及多个DEX
            })
            .collect();

        // 按净盈利率排序
        let mut sorted_paths = cross_dex_paths;
        sorted_paths.sort_by(|a, b| b.net_profit_rate.partial_cmp(&a.net_profit_rate).unwrap_or(std::cmp::Ordering::Equal));

        sorted_paths.into_iter().take(max_results).collect()
    }

    /// 寻找三角套利机会（3步路径）
    pub fn find_triangular_arbitrage(
        &self,
        start_token: &str,
        min_profit_threshold: f64,
        max_results: usize,
    ) -> Vec<ArbitragePath> {
        self.find_optimized_arbitrage_paths(
            start_token,
            3, // 固定为3步
            min_profit_threshold,
            OptimizationStrategy::MaxProfit,
            max_results,
        ).into_iter()
        .filter(|path| path.edges.len() == 3) // 确保是三角套利
        .collect()
    }

    /// 实时监控套利机会（模拟实时更新）
    pub fn monitor_arbitrage_opportunities(
        &self,
        monitored_tokens: &[String],
        max_depth: usize,
        min_profit_threshold: f64,
    ) -> ArbitrageMonitorResult {
        let mut total_opportunities = 0;
        let mut best_opportunity: Option<ArbitragePath> = None;
        let mut opportunities_by_token = HashMap::new();

        for token in monitored_tokens {
            let paths = self.find_best_arbitrage_paths(token, max_depth, min_profit_threshold, 5);
            
            if !paths.is_empty() {
                total_opportunities += paths.len();
                opportunities_by_token.insert(token.clone(), paths.len());

                // 更新最佳机会
                if let Some(best_path) = paths.first() {
                    if best_opportunity.is_none() || 
                       best_path.net_profit_rate > best_opportunity.as_ref().unwrap().net_profit_rate {
                        best_opportunity = Some(best_path.clone());
                    }
                }
            }
        }

        ArbitrageMonitorResult {
            timestamp: Utc::now(),
            total_opportunities,
            best_opportunity,
            opportunities_by_token,
            monitored_tokens: monitored_tokens.to_vec(),
        }
    }

    /// 计算路径组合的风险分散效果
    pub fn analyze_portfolio_risk(
        &self,
        paths: &[ArbitragePath],
    ) -> PortfolioRiskAnalysis {
        if paths.is_empty() {
            return PortfolioRiskAnalysis::default();
        }

        // 计算平均风险评分
        let avg_risk_score = paths.iter()
            .map(|p| p.risk_score)
            .sum::<f64>() / paths.len() as f64;

        // 计算风险分散度（涉及的DEX和代币数量）
        let all_dexes: HashSet<String> = paths.iter()
            .flat_map(|p| p.get_involved_dexes())
            .collect();
        
        let all_tokens: HashSet<String> = paths.iter()
            .flat_map(|p| p.get_involved_tokens())
            .collect();

        // 计算相关性（简化版本：基于共同涉及的代币）
        let mut correlation_matrix = Vec::new();
        for i in 0..paths.len() {
            let mut row = Vec::new();
            for j in 0..paths.len() {
                let tokens_i: HashSet<String> = paths[i].get_involved_tokens().into_iter().collect();
                let tokens_j: HashSet<String> = paths[j].get_involved_tokens().into_iter().collect();
                let intersection = tokens_i.intersection(&tokens_j).count();
                let union = tokens_i.union(&tokens_j).count();
                let correlation = if union > 0 { intersection as f64 / union as f64 } else { 0.0 };
                row.push(correlation);
            }
            correlation_matrix.push(row);
        }

        // 计算平均相关性（排除对角线）
        let mut total_correlation = 0.0;
        let mut count = 0;
        for i in 0..correlation_matrix.len() {
            for j in 0..correlation_matrix[i].len() {
                if i != j {
                    total_correlation += correlation_matrix[i][j];
                    count += 1;
                }
            }
        }
        let avg_correlation = if count > 0 { total_correlation / count as f64 } else { 0.0 };

        PortfolioRiskAnalysis {
            avg_risk_score,
            diversification_score: (all_dexes.len() + all_tokens.len()) as f64,
            avg_correlation,
            dex_count: all_dexes.len(),
            token_count: all_tokens.len(),
            path_count: paths.len(),
        }
    }

    /// 获取市场深度分析
    pub fn get_market_depth_analysis(&self, token: &str) -> Option<MarketDepthAnalysis> {
        if let Some(edges) = self.adjacency_list.get(token) {
            let total_liquidity: f64 = edges.iter()
                .map(|edge| edge.liquidity.to_f64().unwrap_or(0.0))
                .sum();

            let avg_liquidity = total_liquidity / edges.len() as f64;
            
            let connected_tokens = edges.len();
            
            // 计算流动性分布
            let mut liquidity_values: Vec<f64> = edges.iter()
                .map(|edge| edge.liquidity.to_f64().unwrap_or(0.0))
                .collect();
            liquidity_values.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

            let max_liquidity = liquidity_values.first().copied().unwrap_or(0.0);
            let min_liquidity = liquidity_values.last().copied().unwrap_or(0.0);

            // 计算涉及的DEX
            let involved_dexes: HashSet<String> = edges.iter()
                .map(|edge| edge.dex.clone())
                .collect();

            Some(MarketDepthAnalysis {
                token: token.to_string(),
                total_liquidity,
                avg_liquidity,
                max_liquidity,
                min_liquidity,
                connected_tokens,
                involved_dexes: involved_dexes.into_iter().collect(),
                liquidity_distribution: liquidity_values,
            })
        } else {
            None
        }
    }
}

/// 优化策略枚举
#[derive(Debug, Clone, Copy)]
pub enum OptimizationStrategy {
    MaxProfit,      // 最大化盈利
    MinRisk,        // 最小化风险
    Balanced,       // 平衡盈利和风险
    FastExecution,  // 最快执行
    HighLiquidity,  // 高流动性优先
}

/// 套利监控结果
#[derive(Debug, Clone)]
pub struct ArbitrageMonitorResult {
    pub timestamp: DateTime<Utc>,
    pub total_opportunities: usize,
    pub best_opportunity: Option<ArbitragePath>,
    pub opportunities_by_token: HashMap<String, usize>,
    pub monitored_tokens: Vec<String>,
}

/// 投资组合风险分析
#[derive(Debug, Clone, Default)]
pub struct PortfolioRiskAnalysis {
    pub avg_risk_score: f64,
    pub diversification_score: f64,
    pub avg_correlation: f64,
    pub dex_count: usize,
    pub token_count: usize,
    pub path_count: usize,
}

/// 市场深度分析
#[derive(Debug, Clone)]
pub struct MarketDepthAnalysis {
    pub token: String,
    pub total_liquidity: f64,
    pub avg_liquidity: f64,
    pub max_liquidity: f64,
    pub min_liquidity: f64,
    pub connected_tokens: usize,
    pub involved_dexes: Vec<String>,
    pub liquidity_distribution: Vec<f64>,
}
