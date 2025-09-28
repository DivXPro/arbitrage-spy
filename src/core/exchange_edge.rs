use std::str::FromStr;
use bigdecimal::{BigDecimal, FromPrimitive, Zero, ToPrimitive};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use log::{info, warn};
use crate::store::pair_manager::PairData;
use crate::price_calculator::PriceCalculator;

/// 交换边结构体，表示两个代币之间的交换关系
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeEdge {
    pub pair_id: String,            // 交易对唯一标识（引用Graph中的PairData）
    pub from_token_id: String,      // 源代币ID（用于图构建）
    pub to_token_id: String,        // 目标代币ID（用于图构建）
    pub from_token: String,         // 源代币符号（用于显示）
    pub to_token: String,           // 目标代币符号（用于显示）
    pub dex: String,                // 去中心化交易所名称
    pub exchange_rate: BigDecimal,  // 汇率 (to_token/from_token)
    pub liquidity: BigDecimal,      // 流动性
    pub gas_cost: BigDecimal,       // Gas成本估算
    pub slippage: f64,              // 预期滑点
    pub fee_percentage: f64,        // 交易费用百分比
}

impl ExchangeEdge {
    /// 从PairData创建单向ExchangeEdge
    /// 
    /// # 参数
    /// * `pair` - 交易对数据
    /// * `from_token_id` - 源代币ID
    /// * `to_token_id` - 目标代币ID
    /// * `from_token` - 源代币符号
    /// * `to_token` - 目标代币符号
    /// * `exchange_rate` - 汇率 (to_token/from_token)
    /// 
    /// # 返回
    /// * `Result<ExchangeEdge>` - 创建的交换边或错误
    pub fn from_pair_data(
        pair: &PairData,
        from_token_id: String,
        to_token_id: String,
        from_token: String,
        to_token: String,
        exchange_rate: BigDecimal,
    ) -> Result<Self> {
        // 验证输入参数
        if from_token.is_empty() || to_token.is_empty() {
            return Err(anyhow!("代币符号不能为空"));
        }
        
        if from_token == to_token {
            return Err(anyhow!("源代币和目标代币不能相同"));
        }
        
        if exchange_rate <= BigDecimal::zero() {
            return Err(anyhow!("汇率必须大于零"));
        }
        
        // 计算流动性（使用reserve_usd作为流动性指标）
        let liquidity = BigDecimal::from_str(&pair.reserve_usd)
            .map_err(|_| anyhow!("无效的流动性数据: {}", pair.reserve_usd))?;
        
        // 估算Gas成本
        let gas_cost = Self::estimate_gas_cost(&pair.dex);
        
        // 估算滑点
        let slippage = Self::estimate_slippage(&liquidity);
        
        // 获取交易费用
        let fee_percentage = Self::get_dex_fee_percentage(&pair.dex);
        
        Ok(ExchangeEdge {
            pair_id: pair.id.clone(),
            from_token_id,
            to_token_id,
            from_token,
            to_token,
            dex: pair.dex.clone(),
            exchange_rate,
            liquidity,
            gas_cost,
            slippage,
            fee_percentage,
        })
    }
    
    /// 从PairData创建双向ExchangeEdge
    /// 
    /// # 参数
    /// * `pair` - 交易对数据
    /// 
    /// # 返回
    /// * `Result<(ExchangeEdge, ExchangeEdge)>` - 双向交换边或错误
    pub fn create_bidirectional_edges(pair: &PairData) -> Result<(ExchangeEdge, ExchangeEdge)> {
        // 使用PriceCalculator计算价格
        let price_1_per_0 = PriceCalculator::calculate_price_from_pair(pair)
            .map_err(|e| anyhow!("价格计算失败: {}", e))?;
        
        if price_1_per_0.is_zero() {
            return Err(anyhow!("计算出的价格为零"));
        }
        
        // 计算反向价格
        let price_0_per_1 = BigDecimal::from(1) / &price_1_per_0;
        
        // 创建 token0 -> token1 的边
        let edge_0_to_1 = Self::from_pair_data(
            pair,
            pair.token0.id.clone(),
            pair.token1.id.clone(),
            pair.token0.symbol.clone(),
            pair.token1.symbol.clone(),
            price_1_per_0,
        )?;
        
        // 创建 token1 -> token0 的边
        let edge_1_to_0 = Self::from_pair_data(
            pair,
            pair.token1.id.clone(),
            pair.token0.id.clone(),
            pair.token1.symbol.clone(),
            pair.token0.symbol.clone(),
            price_0_per_1,
        )?;
        
        Ok((edge_0_to_1, edge_1_to_0))
    }
    
    /// 从PairData批量创建ExchangeEdge
    /// 
    /// # 参数
    /// * `pairs` - 交易对数据列表
    /// 
    /// # 返回
    /// * `Result<Vec<ExchangeEdge>>` - 所有创建的交换边或错误
    pub fn from_pair_data_batch(pairs: &[PairData]) -> Result<Vec<ExchangeEdge>> {
        let mut edges = Vec::new();
        let mut error_count = 0;
        
        for pair in pairs {
            match Self::create_bidirectional_edges(pair) {
                Ok((edge1, edge2)) => {
                    edges.push(edge1);
                    edges.push(edge2);
                }
                Err(e) => {
                    warn!("跳过交易对 {}: {}", pair.id, e);
                    error_count += 1;
                }
            }
        }
        
        info!("批量创建ExchangeEdge完成，成功: {}, 失败: {}", 
              edges.len() / 2, error_count);
        
        Ok(edges)
    }
    
    /// 验证ExchangeEdge的有效性
    pub fn validate(&self) -> Result<()> {
        if self.pair_id.is_empty() {
            return Err(anyhow!("交易对ID不能为空"));
        }
        
        if self.from_token.is_empty() || self.to_token.is_empty() {
            return Err(anyhow!("代币符号不能为空"));
        }
        
        if self.from_token == self.to_token {
            return Err(anyhow!("源代币和目标代币不能相同"));
        }
        
        if self.exchange_rate <= BigDecimal::zero() {
            return Err(anyhow!("汇率必须大于零"));
        }
        
        if self.liquidity < BigDecimal::zero() {
            return Err(anyhow!("流动性不能为负数"));
        }
        
        if self.gas_cost < BigDecimal::zero() {
            return Err(anyhow!("Gas成本不能为负数"));
        }
        
        if self.slippage < 0.0 || self.slippage > 1.0 {
            return Err(anyhow!("滑点必须在0-1之间"));
        }
        
        if self.fee_percentage < 0.0 || self.fee_percentage > 1.0 {
            return Err(anyhow!("交易费用百分比必须在0-1之间"));
        }
        
        Ok(())
    }
    
    /// 计算考虑费用和滑点后的实际汇率
    pub fn effective_exchange_rate(&self) -> BigDecimal {
        let fee_multiplier = BigDecimal::from_f64(1.0 - self.fee_percentage)
            .unwrap_or_else(|| BigDecimal::from(1));
        let slippage_multiplier = BigDecimal::from_f64(1.0 - self.slippage)
            .unwrap_or_else(|| BigDecimal::from(1));
        
        &self.exchange_rate * &fee_multiplier * &slippage_multiplier
    }
    
    /// 获取交换边的显示信息
    pub fn display_info(&self) -> String {
        format!(
            "{} -> {} (DEX: {}, Rate: {:.6}, Liquidity: ${:.0})",
            self.from_token,
            self.to_token,
            self.dex,
            self.exchange_rate,
            self.liquidity
        )
    }

    /// 估算不同DEX的Gas成本
    pub fn estimate_gas_cost(dex_name: &str) -> BigDecimal {
        // 基于不同DEX的历史数据估算Gas成本
        let base_gas_cost = match dex_name.to_lowercase().as_str() {
            "uniswap_v2" | "sushiswap" | "pancakeswap" => BigDecimal::from_str("150000").unwrap(),
            "uniswap_v3" => BigDecimal::from_str("180000").unwrap(),
            "balancer" => BigDecimal::from_str("200000").unwrap(),
            "curve" => BigDecimal::from_str("220000").unwrap(),
            _ => BigDecimal::from_str("150000").unwrap(), // 默认值
        };
        
        // 假设Gas价格为20 Gwei，ETH价格为$2000
        let gas_price_gwei = BigDecimal::from_str("20").unwrap();
        let eth_price_usd = BigDecimal::from_str("2000").unwrap();
        let gwei_to_eth = BigDecimal::from_str("0.000000001").unwrap();
        
        &base_gas_cost * &gas_price_gwei * &gwei_to_eth * &eth_price_usd
    }
    
    /// 基于流动性估算滑点
    pub fn estimate_slippage(liquidity: &BigDecimal) -> f64 {
        // 基于流动性估算滑点，流动性越高滑点越低
        let liquidity_f64 = liquidity.to_f64().unwrap_or(0.0);
        
        if liquidity_f64 >= 10_000_000.0 {
            0.001 // 0.1% 滑点
        } else if liquidity_f64 >= 1_000_000.0 {
            0.003 // 0.3% 滑点
        } else if liquidity_f64 >= 100_000.0 {
            0.005 // 0.5% 滑点
        } else if liquidity_f64 >= 10_000.0 {
            0.01  // 1% 滑点
        } else {
            0.03  // 3% 滑点
        }
    }
    
    /// 获取不同DEX的交易费用百分比
    pub fn get_dex_fee_percentage(dex_name: &str) -> f64 {
        match dex_name.to_lowercase().as_str() {
            "uniswap_v2" | "sushiswap" => 0.003,  // 0.3%
            "uniswap_v3" => 0.0005,               // 0.05% (可变，这里用最低费率)
            "pancakeswap" => 0.0025,              // 0.25%
            "balancer" => 0.001,                  // 0.1% (可变)
            "curve" => 0.0004,                    // 0.04%
            _ => 0.003,                           // 默认0.3%
        }
    }
}