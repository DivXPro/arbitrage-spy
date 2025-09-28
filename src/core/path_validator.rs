use anyhow::{Result, anyhow};
use bigdecimal::{BigDecimal, ToPrimitive};
use std::str::FromStr;
use super::exchange_edge::ExchangeEdge;
use super::trade_calculator::TradeCalculator;

/// 路径数据验证器
/// 
/// 提供全面的路径数据合理性检查，包括汇率、费用、滑点等关键指标的验证
pub struct PathValidator;

impl PathValidator {
    /// 验证单条交换边的数据合理性
    /// 
    /// # 参数
    /// * `edge` - 要验证的交换边
    /// 
    /// # 返回
    /// 验证结果和警告信息
    pub fn validate_edge_comprehensive(edge: &ExchangeEdge) -> Result<Vec<String>> {
        let mut warnings = Vec::new();
        
        // 基础验证
        TradeCalculator::validate_edge(edge)?;
        
        // 汇率合理性检查
        Self::validate_exchange_rate(edge, &mut warnings)?;
        
        // 费用合理性检查
        Self::validate_fees(edge, &mut warnings);
        
        // 滑点合理性检查
        Self::validate_slippage(edge, &mut warnings);
        
        // 流动性合理性检查
        Self::validate_liquidity(edge, &mut warnings);
        
        // Gas成本合理性检查
        Self::validate_gas_cost(edge, &mut warnings);
        
        Ok(warnings)
    }
    
    /// 验证汇率的合理性
    fn validate_exchange_rate(edge: &ExchangeEdge, warnings: &mut Vec<String>) -> Result<()> {
        let rate = &edge.exchange_rate;
        
        // 检查汇率是否在合理范围内
        let min_rate = BigDecimal::from_str("0.000000000000000001").unwrap();
        let max_rate = BigDecimal::from_str("1000000000000000000").unwrap();
        
        if rate < &min_rate {
            return Err(anyhow!("汇率过低，可能存在数据错误: {} ({}->{})", 
                              rate, edge.from_token, edge.to_token));
        }
        
        if rate > &max_rate {
            return Err(anyhow!("汇率过高，可能存在数据错误: {} ({}->{})", 
                              rate, edge.from_token, edge.to_token));
        }
        
        // 检查是否为异常的汇率值
        let rate_f64 = rate.to_f64().unwrap_or(0.0);
        
        // 检查极端小数位数（可能的精度问题）
        let rate_str = rate.to_string();
        if rate_str.contains('.') {
            let decimal_places = rate_str.split('.').nth(1).unwrap_or("").len();
            if decimal_places > 18 {
                warnings.push(format!("汇率精度过高 ({} 位小数): {} ({}->{})", 
                                     decimal_places, rate, edge.from_token, edge.to_token));
            }
        }
        
        // 检查是否为常见的异常值
        if rate_f64 == 1.0 && edge.from_token != edge.to_token {
            warnings.push(format!("汇率为1.0，请确认是否正确: {} ({}->{})", 
                                 rate, edge.from_token, edge.to_token));
        }
        
        Ok(())
    }
    
    /// 验证费用的合理性
    fn validate_fees(edge: &ExchangeEdge, warnings: &mut Vec<String>) {
        let fee = edge.fee_percentage;
        
        // 检查费用是否在常见范围内
        if fee > 0.01 { // 超过1%
            warnings.push(format!("交易费用较高: {:.2}% ({}->{})", 
                                 fee * 100.0, edge.from_token, edge.to_token));
        }
        
        // 检查不同DEX的费用是否合理
        let expected_fee = match edge.dex.to_lowercase().as_str() {
            "uniswap_v2" | "sushiswap" => 0.003,
            "uniswap_v3" => 0.0005,
            "pancakeswap" => 0.0025,
            "balancer" => 0.001,
            "curve" => 0.0004,
            _ => return, // 未知DEX，跳过检查
        };
        
        let fee_diff = (fee - expected_fee).abs();
        if fee_diff > 0.001 { // 差异超过0.1%
            warnings.push(format!("{}的费用({:.2}%)与预期({:.2}%)差异较大", 
                                 edge.dex, fee * 100.0, expected_fee * 100.0));
        }
    }
    
    /// 验证滑点的合理性
    fn validate_slippage(edge: &ExchangeEdge, warnings: &mut Vec<String>) {
        let slippage = edge.slippage;
        
        // 检查滑点是否过高
        if slippage > 0.05 { // 超过5%
            warnings.push(format!("滑点过高: {:.2}% ({}->{})", 
                                 slippage * 100.0, edge.from_token, edge.to_token));
        }
        
        // 根据流动性检查滑点是否合理
        let liquidity = edge.liquidity.to_f64().unwrap_or(0.0);
        let expected_slippage = if liquidity >= 10_000_000.0 {
            0.001
        } else if liquidity >= 1_000_000.0 {
            0.003
        } else if liquidity >= 100_000.0 {
            0.005
        } else if liquidity >= 10_000.0 {
            0.01
        } else {
            0.03
        };
        
        if slippage > expected_slippage * 2.0 {
            warnings.push(format!("滑点({:.2}%)相对于流动性(${:.0})过高", 
                                 slippage * 100.0, liquidity));
        }
    }
    
    /// 验证流动性的合理性
    fn validate_liquidity(edge: &ExchangeEdge, warnings: &mut Vec<String>) {
        let liquidity = edge.liquidity.to_f64().unwrap_or(0.0);
        
        // 检查流动性是否过低
        if liquidity < 1000.0 {
            warnings.push(format!("流动性过低: ${:.0} ({}->{})", 
                                 liquidity, edge.from_token, edge.to_token));
        }
        
        // 检查流动性是否异常高（可能的数据错误）
        if liquidity > 1_000_000_000.0 { // 超过10亿美元
            warnings.push(format!("流动性异常高: ${:.0} ({}->{})", 
                                 liquidity, edge.from_token, edge.to_token));
        }
    }
    
    /// 验证Gas成本的合理性
    fn validate_gas_cost(edge: &ExchangeEdge, warnings: &mut Vec<String>) {
        let gas_cost = edge.gas_cost.to_f64().unwrap_or(0.0);
        
        // 检查Gas成本是否在合理范围内
        if gas_cost < 0.001 { // 低于0.001 USD
            warnings.push(format!("Gas成本过低: ${:.6} ({}->{})", 
                                 gas_cost, edge.from_token, edge.to_token));
        }
        
        if gas_cost > 100.0 { // 超过100 USD
            warnings.push(format!("Gas成本过高: ${:.2} ({}->{})", 
                                 gas_cost, edge.from_token, edge.to_token));
        }
    }
    
    /// 验证整个路径的合理性
    /// 
    /// # 参数
    /// * `path` - 要验证的交易路径
    /// 
    /// # 返回
    /// 验证结果和警告信息
    pub fn validate_path_comprehensive(path: &[ExchangeEdge]) -> Result<Vec<String>> {
        let mut all_warnings = Vec::new();
        
        // 基础路径验证
        TradeCalculator::validate_path(path)?;
        
        // 验证每条边
        for (i, edge) in path.iter().enumerate() {
            match Self::validate_edge_comprehensive(edge) {
                Ok(warnings) => {
                    for warning in warnings {
                        all_warnings.push(format!("边{}: {}", i + 1, warning));
                    }
                }
                Err(e) => {
                    return Err(anyhow!("边{}验证失败: {}", i + 1, e));
                }
            }
        }
        
        // 路径级别的检查
        Self::validate_path_economics(path, &mut all_warnings)?;
        
        Ok(all_warnings)
    }
    
    /// 验证路径的经济合理性
    fn validate_path_economics(path: &[ExchangeEdge], warnings: &mut Vec<String>) -> Result<()> {
        let initial_amount = BigDecimal::from(1000); // 使用1000作为测试金额
        
        // 计算路径盈利性
        let (final_amount, profit, profit_rate) = TradeCalculator::calculate_path_profit(&initial_amount, path);
        
        // 计算总费用
        let total_gas_cost = TradeCalculator::calculate_total_gas_cost(path);
        let total_fees = TradeCalculator::calculate_total_fees(&initial_amount, path);
        
        // 检查是否有明显的套利机会（可能的数据错误）
        if profit_rate > 0.1 { // 超过10%的盈利率
            warnings.push(format!("路径盈利率异常高: {:.2}%，请检查数据准确性", profit_rate * 100.0));
        }
        
        // 检查是否亏损过大
        if profit_rate < -0.5 { // 亏损超过50%
            warnings.push(format!("路径亏损过大: {:.2}%", profit_rate * 100.0));
        }
        
        // 检查总费用是否合理
        let total_cost_rate = (total_gas_cost + total_fees).to_f64().unwrap_or(0.0) / initial_amount.to_f64().unwrap_or(1.0);
        if total_cost_rate > 0.05 { // 总费用超过5%
            warnings.push(format!("总交易成本过高: {:.2}%", total_cost_rate * 100.0));
        }
        
        // 检查路径长度
        if path.len() > 5 {
            warnings.push(format!("路径过长({} 步)，可能增加执行风险", path.len()));
        }
        
        // 检查是否涉及多个DEX（跨DEX套利的额外风险）
        let dexes: std::collections::HashSet<String> = path.iter().map(|e| e.dex.clone()).collect();
        if dexes.len() > 2 {
            warnings.push(format!("路径涉及{}个DEX，执行复杂度较高", dexes.len()));
        }
        
        Ok(())
    }
    
    /// 生成路径验证报告
    /// 
    /// # 参数
    /// * `path` - 要分析的交易路径
    /// 
    /// # 返回
    /// 详细的验证报告
    pub fn generate_validation_report(path: &[ExchangeEdge]) -> String {
        let mut report = String::new();
        
        report.push_str("=== 路径验证报告 ===\n");
        
        // 基本信息
        report.push_str(&format!("路径长度: {} 步\n", path.len()));
        
        let tokens: Vec<String> = path.iter().map(|e| e.from_token.clone()).collect();
        let mut token_chain = tokens.clone();
        if let Some(last_edge) = path.last() {
            token_chain.push(last_edge.to_token.clone());
        }
        report.push_str(&format!("代币链: {}\n", token_chain.join(" -> ")));
        
        let dexes: std::collections::HashSet<String> = path.iter().map(|e| e.dex.clone()).collect();
        report.push_str(&format!("涉及DEX: {}\n", dexes.into_iter().collect::<Vec<_>>().join(", ")));
        
        // 验证结果
        match Self::validate_path_comprehensive(path) {
            Ok(warnings) => {
                if warnings.is_empty() {
                    report.push_str("✅ 路径验证通过，未发现问题\n");
                } else {
                    report.push_str(&format!("⚠️  发现 {} 个警告:\n", warnings.len()));
                    for warning in warnings {
                        report.push_str(&format!("  - {}\n", warning));
                    }
                }
            }
            Err(e) => {
                report.push_str(&format!("❌ 路径验证失败: {}\n", e));
            }
        }
        
        // 经济分析
        let initial_amount = BigDecimal::from(1000);
        let (final_amount, profit, profit_rate) = TradeCalculator::calculate_path_profit(&initial_amount, path);
        
        report.push_str("\n=== 经济分析 ===\n");
        report.push_str(&format!("测试金额: {} {}\n", initial_amount, path[0].from_token));
        report.push_str(&format!("最终金额: {:.6} {}\n", final_amount, path[0].from_token));
        report.push_str(&format!("绝对盈利: {:.6} {}\n", profit, path[0].from_token));
        report.push_str(&format!("盈利率: {:.4}%\n", profit_rate * 100.0));
        
        let total_gas_cost = TradeCalculator::calculate_total_gas_cost(path);
        report.push_str(&format!("总Gas成本: ${:.6}\n", total_gas_cost));
        
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn create_test_edge(from: &str, to: &str, rate: &str, fee: f64, slippage: f64, liquidity: &str, dex: &str) -> ExchangeEdge {
        ExchangeEdge {
            pair_id: format!("{}-{}", from, to),
            from_token: from.to_string(),
            to_token: to.to_string(),
            from_token_id: format!("{}_id", from.to_lowercase()),
            to_token_id: format!("{}_id", to.to_lowercase()),
            dex: dex.to_string(),
            exchange_rate: BigDecimal::from_str(rate).unwrap(),
            liquidity: BigDecimal::from_str(liquidity).unwrap(),
            gas_cost: BigDecimal::from_str("0.01").unwrap(),
            slippage,
            fee_percentage: fee,
        }
    }

    #[test]
    fn test_comprehensive_validation() {
        let edge = create_test_edge("USDC", "ETH", "0.0005", 0.003, 0.001, "1000000", "Uniswap");
        let warnings = PathValidator::validate_edge_comprehensive(&edge).unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_validation_report() {
        let path = vec![
            create_test_edge("USDC", "ETH", "0.0005", 0.003, 0.001, "1000000", "Uniswap"),
            create_test_edge("ETH", "USDC", "2000", 0.003, 0.001, "1000000", "Sushiswap"),
        ];
        
        let report = PathValidator::generate_validation_report(&path);
        println!("{}", report);
        assert!(report.contains("路径验证报告"));
    }
}