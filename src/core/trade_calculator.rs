use bigdecimal::{BigDecimal, Zero, FromPrimitive, ToPrimitive};
use anyhow::{Result, anyhow};
use super::exchange_edge::ExchangeEdge;

/// 统一的交易计算器
/// 
/// 这个模块提供了所有套利相关计算的统一接口，确保在整个系统中
/// 使用一致的计算逻辑，避免不同模块间的计算差异。
pub struct TradeCalculator;

impl TradeCalculator {
    /// 计算单次交易后的金额
    /// 
    /// 这是核心的计算方法，所有其他计算都基于这个方法。
    /// 计算顺序：输入金额 -> 应用汇率 -> 扣除费用 -> 考虑滑点
    /// 
    /// # 参数
    /// * `input_amount` - 输入金额
    /// * `edge` - 交易边信息
    /// 
    /// # 返回
    /// 交易后的输出金额
    pub fn calculate_trade_output(input_amount: &BigDecimal, edge: &ExchangeEdge) -> BigDecimal {
        if input_amount.is_zero() {
            return BigDecimal::zero();
        }

        // 1. 应用汇率进行基础转换
        let mut output_amount = input_amount * &edge.exchange_rate;

        // 2. 扣除交易费用 (fee_percentage 已经是小数形式，如 0.003 表示 0.3%)
        let fee_amount = &output_amount * BigDecimal::from_f64(edge.fee_percentage).unwrap_or_default();
        output_amount = output_amount - fee_amount;

        // 3. 考虑滑点影响 (slippage 也是小数形式)
        let slippage_impact = &output_amount * BigDecimal::from_f64(edge.slippage).unwrap_or_default();
        output_amount = output_amount - slippage_impact;

        // 4. 确保金额不为负数
        if output_amount < BigDecimal::zero() {
            BigDecimal::zero()
        } else {
            output_amount
        }
    }

    /// 计算路径的最终金额
    /// 
    /// # 参数
    /// * `initial_amount` - 初始投入金额
    /// * `path` - 交易路径（边的序列）
    /// 
    /// # 返回
    /// 路径执行后的最终金额
    pub fn calculate_path_output(initial_amount: &BigDecimal, path: &[ExchangeEdge]) -> BigDecimal {
        let mut current_amount = initial_amount.clone();
        
        for edge in path {
            current_amount = Self::calculate_trade_output(&current_amount, edge);
            
            // 如果任何一步的输出为零，整个路径失败
            if current_amount.is_zero() {
                return BigDecimal::zero();
            }
        }
        
        current_amount
    }

    /// 计算路径的盈利指标
    /// 
    /// # 参数
    /// * `initial_amount` - 初始投入金额
    /// * `path` - 交易路径
    /// 
    /// # 返回
    /// (最终金额, 绝对盈利, 盈利率)
    pub fn calculate_path_profit(initial_amount: &BigDecimal, path: &[ExchangeEdge]) -> (BigDecimal, BigDecimal, f64) {
        let final_amount = Self::calculate_path_output(initial_amount, path);
        let profit = &final_amount - initial_amount;
        let profit_rate = if !initial_amount.is_zero() {
            profit.to_f64().unwrap_or(0.0) / initial_amount.to_f64().unwrap_or(1.0)
        } else {
            0.0
        };
        
        (final_amount, profit, profit_rate)
    }

    /// 计算路径的总交易费用
    /// 
    /// # 参数
    /// * `initial_amount` - 初始投入金额
    /// * `path` - 交易路径
    /// 
    /// # 返回
    /// 总交易费用（以初始代币计价）
    pub fn calculate_total_fees(initial_amount: &BigDecimal, path: &[ExchangeEdge]) -> BigDecimal {
        let mut current_amount = initial_amount.clone();
        let mut total_fees = BigDecimal::zero();
        
        for edge in path {
            // 计算当前步骤的输出金额（应用汇率）
            let output_before_fee = &current_amount * &edge.exchange_rate;
            
            // 计算费用
            let fee_amount = &output_before_fee * BigDecimal::from_f64(edge.fee_percentage).unwrap_or_default();
            
            // 将费用转换回初始代币计价（简化处理，实际应该考虑汇率链）
            total_fees += &fee_amount;
            
            // 更新当前金额
            current_amount = Self::calculate_trade_output(&current_amount, edge);
        }
        
        total_fees
    }

    /// 计算路径的总Gas成本
    /// 
    /// # 参数
    /// * `path` - 交易路径
    /// 
    /// # 返回
    /// 总Gas成本
    pub fn calculate_total_gas_cost(path: &[ExchangeEdge]) -> BigDecimal {
        path.iter().map(|edge| &edge.gas_cost).sum()
    }

    /// 计算净盈利（扣除Gas和费用）
    /// 
    /// # 参数
    /// * `initial_amount` - 初始投入金额
    /// * `path` - 交易路径
    /// 
    /// # 返回
    /// (净盈利, 净盈利率)
    pub fn calculate_net_profit(initial_amount: &BigDecimal, path: &[ExchangeEdge]) -> (BigDecimal, f64) {
        let (final_amount, gross_profit, _) = Self::calculate_path_profit(initial_amount, path);
        let total_gas_cost = Self::calculate_total_gas_cost(path);
        
        // 注意：这里简化处理，假设Gas成本以相同代币计价
        // 实际应用中可能需要转换Gas成本到初始代币
        let net_profit = gross_profit - total_gas_cost;
        let net_profit_rate = if !initial_amount.is_zero() {
            net_profit.to_f64().unwrap_or(0.0) / initial_amount.to_f64().unwrap_or(1.0)
        } else {
            0.0
        };
        
        (net_profit, net_profit_rate)
    }

    /// 验证交易边的有效性
    /// 
    /// # 参数
    /// * `edge` - 要验证的交易边
    /// 
    /// # 返回
    /// 验证结果
    pub fn validate_edge(edge: &ExchangeEdge) -> Result<()> {
        if edge.exchange_rate <= BigDecimal::zero() {
            return Err(anyhow!("汇率必须大于0: {}", edge.exchange_rate));
        }
        
        if edge.fee_percentage < 0.0 || edge.fee_percentage > 1.0 {
            return Err(anyhow!("费用百分比必须在0-1之间: {}", edge.fee_percentage));
        }
        
        if edge.slippage < 0.0 || edge.slippage > 1.0 {
            return Err(anyhow!("滑点必须在0-1之间: {}", edge.slippage));
        }
        
        if edge.liquidity < BigDecimal::zero() {
            return Err(anyhow!("流动性不能为负数: {}", edge.liquidity));
        }
        
        Ok(())
    }

    /// 验证交易路径的有效性
    /// 
    /// # 参数
    /// * `path` - 要验证的交易路径
    /// 
    /// # 返回
    /// 验证结果
    pub fn validate_path(path: &[ExchangeEdge]) -> Result<()> {
        if path.is_empty() {
            return Err(anyhow!("交易路径不能为空"));
        }

        if path.len() < 2 {
            return Err(anyhow!("套利路径至少需要2条边"));
        }

        // 检查路径是否形成闭环
        let start_token = &path[0].from_token;
        let end_token = &path.last().unwrap().to_token;
        if start_token != end_token {
            return Err(anyhow!("套利路径必须形成闭环，起始代币: {}, 结束代币: {}", start_token, end_token));
        }

        // 检查路径连续性
        for i in 0..path.len() - 1 {
            if path[i].to_token != path[i + 1].from_token {
                return Err(anyhow!(
                    "路径在第{}步不连续: {} -> {} 与 {} -> {}",
                    i + 1,
                    path[i].from_token,
                    path[i].to_token,
                    path[i + 1].from_token,
                    path[i + 1].to_token
                ));
            }
        }

        // 验证每条边的有效性
        for (i, edge) in path.iter().enumerate() {
            if let Err(e) = Self::validate_edge(edge) {
                return Err(anyhow!("路径第{}条边验证失败: {}", i + 1, e));
            }
        }

        Ok(())
    }

    /// 计算有效汇率（考虑费用和滑点）
    /// 
    /// # 参数
    /// * `edge` - 交易边
    /// 
    /// # 返回
    /// 有效汇率
    pub fn calculate_effective_rate(edge: &ExchangeEdge) -> BigDecimal {
        let fee_multiplier = BigDecimal::from_f64(1.0 - edge.fee_percentage)
            .unwrap_or_else(|| BigDecimal::from(1));
        let slippage_multiplier = BigDecimal::from_f64(1.0 - edge.slippage)
            .unwrap_or_else(|| BigDecimal::from(1));
        
        &edge.exchange_rate * &fee_multiplier * &slippage_multiplier
    }

    /// 计算路径风险评分
    /// 
    /// 风险评分基于以下因素：
    /// - 流动性风险：流动性越低，风险越高
    /// - 滑点风险：滑点越高，风险越高  
    /// - DEX风险：不同DEX的可靠性不同
    /// - 路径长度风险：路径越长，风险越高
    /// 
    /// # 参数
    /// * `path` - 套利路径
    /// 
    /// # 返回
    /// 风险评分 (0-100，100为最高风险)
    pub fn calculate_path_risk(path: &[ExchangeEdge]) -> f64 {
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
    /// 
    /// 执行时间基于以下因素：
    /// - 基础执行时间：每个交易约15秒（以太坊区块时间）
    /// - DEX复杂度：不同DEX的执行时间差异
    /// 
    /// # 参数
    /// * `path` - 套利路径
    /// 
    /// # 返回
    /// 预估执行时间（秒）
    pub fn estimate_execution_time(path: &[ExchangeEdge]) -> f64 {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn create_test_edge(rate: &str, fee: f64, slippage: f64) -> ExchangeEdge {
        ExchangeEdge {
            from_token: "A".to_string(),
            to_token: "B".to_string(),
            from_token_id: "a_id".to_string(),
            to_token_id: "b_id".to_string(),
            exchange_rate: BigDecimal::from_str(rate).unwrap(),
            liquidity: BigDecimal::from_str("1000000").unwrap(),
            dex: "TestDEX".to_string(),
            pair_id: "test_pair".to_string(),
            gas_cost: BigDecimal::from_str("0.01").unwrap(),
            slippage,
            fee_percentage: fee,
        }
    }

    #[test]
    fn test_calculate_trade_output() {
        let edge = create_test_edge("2.0", 0.003, 0.001); // 2x汇率, 0.3%费用, 0.1%滑点
        let input = BigDecimal::from_str("100").unwrap();
        
        let output = TradeCalculator::calculate_trade_output(&input, &edge);
        
        // 预期计算: 100 * 2.0 = 200
        // 扣除费用: 200 * (1 - 0.003) = 199.4
        // 扣除滑点: 199.4 * (1 - 0.001) = 199.2006
        let expected = BigDecimal::from_str("199.2006").unwrap();
        
        assert!((output - expected).abs() < BigDecimal::from_str("0.0001").unwrap());
    }

    #[test]
    fn test_path_calculation() {
        let edge1 = create_test_edge("2.0", 0.003, 0.001);
        let mut edge2 = create_test_edge("0.6", 0.003, 0.001);
        edge2.from_token = "B".to_string();
        edge2.to_token = "A".to_string();
        
        let path = vec![edge1, edge2];
        let initial = BigDecimal::from_str("100").unwrap();
        
        let (final_amount, profit, profit_rate) = TradeCalculator::calculate_path_profit(&initial, &path);
        
        // 应该有一定的盈利（或亏损）
        println!("Initial: {}, Final: {}, Profit: {}, Rate: {}", initial, final_amount, profit, profit_rate);
        
        assert!(final_amount > BigDecimal::zero());
    }

    #[test]
    fn test_validation() {
        let valid_edge = create_test_edge("2.0", 0.003, 0.001);
        assert!(TradeCalculator::validate_edge(&valid_edge).is_ok());
        
        let invalid_edge = create_test_edge("0", 0.003, 0.001); // 无效汇率
        assert!(TradeCalculator::validate_edge(&invalid_edge).is_err());
    }
}