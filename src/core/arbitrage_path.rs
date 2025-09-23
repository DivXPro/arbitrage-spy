use std::collections::HashSet;
use bigdecimal::{BigDecimal, Zero, FromPrimitive, ToPrimitive};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use log::{info, warn};
use super::exchange_edge::ExchangeEdge;

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
        if self.edges.is_empty() {
            return Err(anyhow!("套利路径不能为空"));
        }

        if self.edges.len() < 2 {
            return Err(anyhow!("套利路径至少需要2条边"));
        }

        // 检查路径是否形成闭环
        let start_token = &self.edges[0].from_token;
        let end_token = &self.edges.last().unwrap().to_token;
        if start_token != end_token {
            return Err(anyhow!("套利路径必须形成闭环，起始代币: {}, 结束代币: {}", start_token, end_token));
        }

        // 检查路径连续性
        for i in 0..self.edges.len() - 1 {
            if self.edges[i].to_token != self.edges[i + 1].from_token {
                return Err(anyhow!(
                    "路径在第{}步不连续: {} -> {} 与 {} -> {}",
                    i + 1,
                    self.edges[i].from_token,
                    self.edges[i].to_token,
                    self.edges[i + 1].from_token,
                    self.edges[i + 1].to_token
                ));
            }
        }

        // 验证每条边的有效性
        for (i, edge) in self.edges.iter().enumerate() {
            if let Err(e) = edge.validate() {
                return Err(anyhow!("路径第{}条边验证失败: {}", i + 1, e));
            }
        }

        // 检查盈利率是否合理
        if self.profit_rate < -1.0 || self.profit_rate > 10.0 {
            return Err(anyhow!("盈利率异常: {}%", self.profit_rate * 100.0));
        }

        Ok(())
    }

    /// 重新计算路径的盈利指标
    pub fn recalculate_metrics(&mut self, initial_amount: &BigDecimal) -> Result<()> {
        self.initial_amount = initial_amount.clone();
        
        // 重新计算最终金额
        let mut current_amount = initial_amount.clone();
        for edge in &self.edges {
            current_amount = self.calculate_amount_after_trade(&current_amount, edge);
        }
        self.final_amount = current_amount;

        // 重新计算盈利
        self.profit = &self.final_amount - &self.initial_amount;
        self.profit_rate = self.profit.to_f64().unwrap_or(0.0) / self.initial_amount.to_f64().unwrap_or(1.0);

        // 重新计算成本
        self.total_gas_cost = self.edges.iter().map(|edge| &edge.gas_cost).sum();
        self.total_fee_cost = self.calculate_total_fees(&self.initial_amount);

        // 重新计算净盈利
        self.net_profit = &self.profit - &self.total_gas_cost - &self.total_fee_cost;
        self.net_profit_rate = self.net_profit.to_f64().unwrap_or(0.0) / self.initial_amount.to_f64().unwrap_or(1.0);

        Ok(())
    }

    /// 计算交易后的金额
    fn calculate_amount_after_trade(&self, input_amount: &BigDecimal, edge: &ExchangeEdge) -> BigDecimal {
        // 计算交易费用
        let fee_rate = BigDecimal::from_f64(edge.fee_percentage / 100.0).unwrap_or_default();
        let fee_amount = input_amount * &fee_rate;
        let amount_after_fee = input_amount - &fee_amount;

        // 应用汇率
        let output_amount = &amount_after_fee * &edge.exchange_rate;

        // 考虑滑点影响
        if edge.slippage > 0.0 {
            let slippage_factor = BigDecimal::from_f64(1.0 - edge.slippage / 100.0).unwrap_or_default();
            output_amount * slippage_factor
        } else {
            output_amount
        }
    }

    /// 计算总交易费用
    fn calculate_total_fees(&self, initial_amount: &BigDecimal) -> BigDecimal {
        let mut current_amount = initial_amount.clone();
        let mut total_fees = BigDecimal::from(0);

        for edge in &self.edges {
            let fee_rate = BigDecimal::from_f64(edge.fee_percentage / 100.0).unwrap_or_default();
            let fee_amount = &current_amount * &fee_rate;
            total_fees += &fee_amount;
            current_amount = self.calculate_amount_after_trade(&current_amount, edge);
        }

        total_fees
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
            chain.push(format!("{}({})", edge.to_token, edge.dex));
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