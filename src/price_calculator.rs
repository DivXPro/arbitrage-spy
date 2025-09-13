use anyhow::Result;
use bigdecimal::{BigDecimal, FromPrimitive, Zero};
use std::str::FromStr;
use crate::thegraph::{PairData};

/// 价格计算工具
pub struct PriceCalculator;

impl PriceCalculator {
    /// 从储备量计算token0/token1的价格
    /// 
    /// # 参数
    /// * `reserve0` - token0的储备量字符串（可以是带小数点的形式）
    /// * `reserve1` - token1的储备量字符串（可以是带小数点的形式）
    /// * `token0_decimals` - token0的小数位数，默认为0（用于带小数点的字符串）
    /// * `token1_decimals` - token1的小数位数，默认为0（用于带小数点的字符串）
    pub fn calculate_price_with_decimals(
        reserve0: &str,
        reserve1: &str,
        token0_decimals: u32,
        token1_decimals: u32,
    ) -> Result<BigDecimal> {
        let reserve0_bd = BigDecimal::from_str(reserve0)
            .map_err(|e| anyhow::anyhow!("Invalid reserve0: {}", e))?;
        let reserve1_bd = BigDecimal::from_str(reserve1)
            .map_err(|e| anyhow::anyhow!("Invalid reserve1: {}", e))?;
        
        if reserve0_bd.is_zero() {
            return Err(anyhow::anyhow!("Reserve0 is zero, cannot calculate price"));
        }
        
        // 调整小数位数
        let adjusted_reserve0 = Self::adjust_for_decimals(&reserve0_bd, token0_decimals);
        let adjusted_reserve1 = Self::adjust_for_decimals(&reserve1_bd, token1_decimals);
        
        // 计算价格 (token1/token0)
        let price = &adjusted_reserve1 / &adjusted_reserve0;
        
        Ok(price)
    }
    
    /// 从两个储备量计算token0/token1的价格（简化方法，默认小数位数为0）
    /// 适用于带小数点的储备量字符串
    /// 
    /// # 参数
    /// * `reserve0` - token0的储备量字符串（带小数点形式）
    /// * `reserve1` - token1的储备量字符串（带小数点形式）
    pub fn calculate_price(reserve0: &str, reserve1: &str) -> Result<BigDecimal> {
        // 使用默认小数位数0，适用于带小数点的字符串
        Self::calculate_price_with_decimals(reserve0, reserve1, 0, 0)
    }

    
    /// 格式化价格为显示字符串
    pub fn format_price(price: &BigDecimal) -> String {
        format!("${:.6}", price)
    }
    
    /// 调整BigDecimal的小数位数
    fn adjust_for_decimals(value: &BigDecimal, decimals: u32) -> BigDecimal {
        let divisor = BigDecimal::from_u64(10_u64.pow(decimals))
            .unwrap_or_else(|| BigDecimal::from(1));
        value / divisor
    }
    
    /// 检查是否为有效的储备量数据
    pub fn has_valid_reserves(pair: &PairData) -> bool {
        if let (Ok(reserve0), Ok(reserve1)) = (
            BigDecimal::from_str(&pair.reserve0),
            BigDecimal::from_str(&pair.reserve1)
        ) {
            !reserve0.is_zero() && !reserve1.is_zero()
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thegraph::TokenInfo;
    
    #[test]
    fn test_calculate_price_with_decimals() {
        // 测试独立的 calculate_price_with_decimals 方法
        let reserve0 = "1000000000000000000000"; // 1000 WETH (18 decimals)
        let reserve1 = "2000000000000"; // 2,000,000 USDT (6 decimals)
        let token0_decimals = 18;
        let token1_decimals = 6;
        
        let price = PriceCalculator::calculate_price_with_decimals(
            reserve0,
            reserve1,
            token0_decimals,
            token1_decimals,
        ).unwrap();
        // 预期价格: 2,000,000 USDT / 1000 WETH = 2000 USDT per WETH
        assert_eq!(price.to_string(), "2000");
    }
    
    #[test]
    fn test_calculate_price() {
        // 测试带小数点的储备量字符串
        let reserve0 = "1000.0"; // 1000 WETH
        let reserve1 = "2000000.0"; // 2,000,000 USDT
        
        let price = PriceCalculator::calculate_price(reserve0, reserve1).unwrap();
        // 预期价格: 2,000,000 USDT / 1000 WETH = 2000 USDT per WETH
        assert_eq!(price.to_string(), "2000");
    }
    
    #[test]
    fn test_calculate_price_from_pair_data() {
        let pair = PairData {
            id: "test".to_string(),
            network: "ethereum".to_string(),
            dex_type: "uniswap_v2".to_string(),
            token0: TokenInfo {
                id: "token0".to_string(),
                symbol: "WETH".to_string(),
                name: "Wrapped Ether".to_string(),
                decimals: "18".to_string(),
            },
            token1: TokenInfo {
                id: "token1".to_string(),
                symbol: "USDT".to_string(),
                name: "Tether USD".to_string(),
                decimals: "6".to_string(),
            },
            volume_usd: "1000000".to_string(),
            reserve_usd: "5000000".to_string(),
            tx_count: "1000".to_string(),
            reserve0: "1000000000000000000000".to_string(), // 1000 WETH (18 decimals)
            reserve1: "2000000000000".to_string(), // 2,000,000 USDT (6 decimals)
            fee_tier: "3000".to_string(),
        };
        
        let price = PriceCalculator::calculate_price(&pair.reserve0, &pair.reserve1).unwrap();
        // 预期价格: 2,000,000 USDT / 1000 WETH = 2000 USDT per WETH
        assert_eq!(price.to_string(), "2000");
    }
    
    #[test]
    fn test_format_price() {
        let price = BigDecimal::from_str("2000.123456789").unwrap();
        let formatted = PriceCalculator::format_price(&price);
        assert_eq!(formatted, "$2000.123457");
    }
    
    #[test]
    fn test_has_valid_reserves() {
        let valid_pair = PairData {
            id: "test".to_string(),
            network: "ethereum".to_string(),
            dex_type: "uniswap_v2".to_string(),
            token0: TokenInfo {
                id: "token0".to_string(),
                symbol: "WETH".to_string(),
                name: "Wrapped Ether".to_string(),
                decimals: "18".to_string(),
            },
            token1: TokenInfo {
                id: "token1".to_string(),
                symbol: "USDT".to_string(),
                name: "Tether USD".to_string(),
                decimals: "6".to_string(),
            },
            volume_usd: "1000000".to_string(),
            reserve_usd: "5000000".to_string(),
            tx_count: "1000".to_string(),
            reserve0: "1000000000000000000000".to_string(),
            reserve1: "2000000000000".to_string(),
            fee_tier: "3000".to_string(),
        };
        
        assert!(PriceCalculator::has_valid_reserves(&valid_pair));
        
        let invalid_pair = PairData {
            reserve0: "0".to_string(),
            reserve1: "1000".to_string(),
            ..valid_pair
        };
        
        assert!(!PriceCalculator::has_valid_reserves(&invalid_pair));
    }
}