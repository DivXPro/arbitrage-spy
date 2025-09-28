use bigdecimal::{BigDecimal, ToPrimitive};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use super::exchange_edge::ExchangeEdge;
use super::trade_calculator::TradeCalculator;

/// 套利路径结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitragePath {
    pub edges: Vec<ExchangeEdge>,           // 路径中的所有边
    pub initial_amount: BigDecimal,         // 初始投入金额
    pub final_amount: BigDecimal,           // 最终获得金额
    pub profit: BigDecimal,                 // 绝对盈利
    pub profit_rate: f64,                   // 盈利率（百分比）
    pub net_profit: BigDecimal,             // 净盈利（扣除费用后）
    pub net_profit_rate: f64,               // 净盈利率（百分比）
    pub total_gas_cost: BigDecimal,         // 总Gas成本
    pub total_fee_cost: BigDecimal,         // 总交易费用
    pub risk_score: f64,                    // 风险评分（0-100）
    pub estimated_execution_time: f64,      // 预估执行时间（秒）
}

impl ArbitragePath {
    /// 验证套利路径的有效性
    pub fn validate(&self) -> Result<()> {
        // 使用统一的验证逻辑
        TradeCalculator::validate_path(&self.edges)?;

        // 检查盈利率是否合理
        if self.profit_rate < -1.0 || self.profit_rate > 10.0 {
            return Err(anyhow!("盈利率异常: {}%", self.profit_rate * 100.0));
        }

        Ok(())
    }

    /// 重新计算路径的盈利指标
    pub fn recalculate_metrics(&mut self, initial_amount: &BigDecimal) -> Result<()> {
        self.initial_amount = initial_amount.clone();
        
        // 使用统一计算器计算路径盈利指标
        let (final_amount, profit, profit_rate) = TradeCalculator::calculate_path_profit(initial_amount, &self.edges);
        self.final_amount = final_amount;
        self.profit = profit;
        self.profit_rate = profit_rate;

        // 计算成本
        self.total_gas_cost = TradeCalculator::calculate_total_gas_cost(&self.edges);
        self.total_fee_cost = TradeCalculator::calculate_total_fees(initial_amount, &self.edges);

        // 计算净盈利
        let (net_profit, net_profit_rate) = TradeCalculator::calculate_net_profit(initial_amount, &self.edges);
        self.net_profit = net_profit;
        self.net_profit_rate = net_profit_rate;

        Ok(())
    }



    /// 获取路径摘要信息
    pub fn get_summary(&self) -> String {
        format!(
            "套利路径: {} -> 盈利率: {:.2}% -> 净盈利率: {:.2}% -> 风险评分: {:.1} -> 预估执行时间: {:.1}s",
            self.format_path_chain(),
            self.profit_rate * 100.0,
            self.net_profit_rate * 100.0,
            self.risk_score,
            self.estimated_execution_time
        )
    }

    /// 检查路径是否可执行
    pub fn is_executable(&self, min_liquidity_per_step: f64) -> bool {
        for edge in &self.edges {
            if edge.liquidity.to_f64().unwrap_or(0.0) < min_liquidity_per_step {
                return false;
            }
        }
        true
    }

    /// 格式化路径链
    pub fn format_path_chain(&self) -> String {
        if self.edges.is_empty() {
            return "空路径".to_string();
        }

        let mut chain = vec![self.edges[0].from_token.clone()];
        for edge in &self.edges {
            chain.push(format!("{}({})", edge.to_token, edge.pair_id));
        }
        chain.join(" -> ")
    }

    /// 获取涉及的DEX列表
    pub fn get_involved_dexes(&self) -> Vec<String> {
        self.edges.iter()
            .map(|edge| edge.dex.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect()
    }

    /// 获取涉及的代币列表
    pub fn get_involved_tokens(&self) -> Vec<String> {
        let mut tokens = std::collections::HashSet::new();
        for edge in &self.edges {
            tokens.insert(edge.from_token.clone());
            tokens.insert(edge.to_token.clone());
        }
        tokens.into_iter().collect()
    }

    /// 计算年化收益率
    pub fn calculate_apy(&self, executions_per_year: f64) -> f64 {
        if executions_per_year <= 0.0 {
            return 0.0;
        }
        self.net_profit_rate * executions_per_year * 100.0
    }
}