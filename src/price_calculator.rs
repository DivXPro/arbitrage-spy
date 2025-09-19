use anyhow::Result;
use bigdecimal::{BigDecimal, FromPrimitive, Zero};
use std::str::FromStr;
use crate::thegraph::{PairData};

/// 价格计算工具
pub struct PriceCalculator;

impl PriceCalculator {
    /// Uniswap V3 Q64.96 格式的常量 (2^96)
    const Q96: &'static str = "79228162514264337593543950336";
    
    /// Uniswap V3 tick 基数 (1.0001)
    const TICK_BASE: f64 = 1.0001;
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
    
    /// 格式化价格为显示字符串
    pub fn format_price(price: &BigDecimal) -> String {
        format!("${:.6}", price)
    }
    
    /// 调整BigDecimal的小数位数
    fn adjust_for_decimals(value: &BigDecimal, decimals: u32) -> BigDecimal {
        // 安全地计算 10^decimals，避免整数溢出
        let divisor = if decimals <= 18 {
            // 对于常见的小数位数，使用预计算的值
            BigDecimal::from_u64(10_u64.pow(decimals))
                .unwrap_or_else(|| BigDecimal::from(1))
        } else {
            // 对于极大的小数位数，使用BigDecimal的乘法
            let ten = BigDecimal::from(10);
            let mut result = BigDecimal::from(1);
            for _ in 0..decimals {
                result = result * &ten;
            }
            result
        };
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
    
    /// 从 Uniswap V3 的 sqrt_price 计算实际价格
    /// 
    /// # 参数
    /// * `sqrt_price_x96` - Q64.96 格式的价格平方根字符串
    /// * `token0_decimals` - token0的小数位数
    /// * `token1_decimals` - token1的小数位数
    /// 
    /// # 返回
    /// token1/token0 的价格
    pub fn calculate_price_from_sqrt_price(
        sqrt_price_x96: &str,
        token0_decimals: u32,
        token1_decimals: u32,
    ) -> Result<BigDecimal> {
        let sqrt_price_bd = BigDecimal::from_str(sqrt_price_x96)
            .map_err(|e| anyhow::anyhow!("Invalid sqrt_price: {}", e))?;
        
        if sqrt_price_bd.is_zero() {
            return Err(anyhow::anyhow!("sqrt_price is zero, cannot calculate price"));
        }
        
        // sqrt_price 是 Q64.96 格式，需要除以 2^96
        let q96 = BigDecimal::from_str(Self::Q96)
            .map_err(|e| anyhow::anyhow!("Invalid Q96 constant: {}", e))?;
        
        // 计算实际的 sqrt_price
        let sqrt_price_real = &sqrt_price_bd / &q96;
        
        // 价格 = sqrt_price^2
        let price_raw = &sqrt_price_real * &sqrt_price_real;
        
        // 调整小数位数差异
        // 对于 WETH(18)/USDT(6) 对，价格应该是 USDT/WETH
        // 需要将价格乘以 10^(token0_decimals - token1_decimals) = 10^(18-6) = 10^12
        let decimals_diff = token0_decimals as i32 - token1_decimals as i32;
        let price = if decimals_diff != 0 {
            let abs_diff = decimals_diff.abs() as u32;
            
            // 安全地计算 10^abs_diff，避免整数溢出
            let adjustment = if abs_diff <= 18 {
                // 对于常见的小数位数差异，使用预计算的值
                BigDecimal::from_u64(10_u64.pow(abs_diff))
                    .unwrap_or_else(|| BigDecimal::from(1))
            } else {
                // 对于极大的差异，使用BigDecimal的字符串构造
                let ten = BigDecimal::from(10);
                let mut result = BigDecimal::from(1);
                for _ in 0..abs_diff {
                    result = result * &ten;
                }
                result
            };
            
            if decimals_diff > 0 {
                price_raw * adjustment
            } else {
                price_raw / adjustment
            }
        } else {
            price_raw
        };
        
        Ok(price)
    }
    
    /// 从 Uniswap V3 的 tick 计算实际价格
    /// 
    /// # 参数
    /// * `tick` - tick 值字符串
    /// * `token0_decimals` - token0的小数位数
    /// * `token1_decimals` - token1的小数位数
    /// 
    /// # 返回
    /// token1/token0 的价格
    pub fn calculate_price_from_tick(
        tick: &str,
        token0_decimals: u32,
        token1_decimals: u32,
    ) -> Result<BigDecimal> {
        let tick_value = tick.parse::<i32>()
            .map_err(|e| anyhow::anyhow!("Invalid tick value: {}", e))?;
        
        // 价格 = 1.0001^tick
        let price_raw = Self::TICK_BASE.powi(tick_value);
        
        // 转换为 BigDecimal
        let price_bd = BigDecimal::from_f64(price_raw)
            .ok_or_else(|| anyhow::anyhow!("Failed to convert price to BigDecimal"))?;
        
        // 调整小数位数差异
        let decimals_diff = token0_decimals as i32 - token1_decimals as i32;
        let price = if decimals_diff != 0 {
            let abs_diff = decimals_diff.abs() as u32;
            
            // 安全地计算 10^abs_diff，避免整数溢出
            let adjustment = if abs_diff <= 18 {
                // 对于常见的小数位数差异，使用预计算的值
                BigDecimal::from_u64(10_u64.pow(abs_diff))
                    .unwrap_or_else(|| BigDecimal::from(1))
            } else {
                // 对于极大的差异，使用BigDecimal的字符串构造
                let ten = BigDecimal::from(10);
                let mut result = BigDecimal::from(1);
                for _ in 0..abs_diff {
                    result = result * &ten;
                }
                result
            };
            
            if decimals_diff > 0 {
                price_bd * adjustment
            } else {
                price_bd / adjustment
            }
        } else {
            price_bd
        };
        
        Ok(price)
    }
    
    /// 从 PairData 计算 V3 价格（优先使用 sqrt_price，fallback 到 tick）
    /// 
    /// # 参数
    /// * `pair` - 包含 V3 价格信息的 PairData
    /// 
    /// # 返回
    /// token1/token0 的价格
    pub fn calculate_v3_price(pair: &PairData) -> Result<BigDecimal> {
        let token0_decimals = pair.token0.decimals.parse::<u32>()
            .map_err(|e| anyhow::anyhow!("Invalid token0 decimals: {}", e))?;
        let token1_decimals = pair.token1.decimals.parse::<u32>()
            .map_err(|e| anyhow::anyhow!("Invalid token1 decimals: {}", e))?;
        
        // 优先使用 sqrt_price
        if let Some(sqrt_price) = &pair.sqrt_price {
            if !sqrt_price.is_empty() && sqrt_price != "0" {
                return Self::calculate_price_from_sqrt_price(sqrt_price, token0_decimals, token1_decimals);
            }
        }
        
        // fallback 到 tick
        if let Some(tick) = &pair.tick {
            if !tick.is_empty() {
                return Self::calculate_price_from_tick(tick, token0_decimals, token1_decimals);
            }
        }
        
        Err(anyhow::anyhow!("No valid V3 price data (sqrt_price or tick) found"))
    }
    
    /// 从 PairData 自动计算价格（根据 protocol_type 选择 V2 或 V3 计算方式）
    /// 
    /// # 参数
    /// * `pair` - 包含价格信息的 PairData
    /// 
    /// # 返回
    /// token1/token0 的价格
    pub fn calculate_price_from_pair(pair: &PairData) -> Result<BigDecimal> {
        // 根据 protocol_type 选择计算方式
        if pair.protocol_type == "amm_v3" {
            // 使用 V3 计算方式
            Self::calculate_v3_price(pair)
        } else {
            // 使用 V2 计算方式（默认）
            if Self::has_valid_reserves(pair) {
                let token0_decimals = pair.token0.decimals.parse::<u32>()
                    .map_err(|e| anyhow::anyhow!("Invalid token0 decimals: {}", e))?;
                let token1_decimals = pair.token1.decimals.parse::<u32>()
                    .map_err(|e| anyhow::anyhow!("Invalid token1 decimals: {}", e))?;
                
                Self::calculate_price_with_decimals(
                    &pair.reserve0,
                    &pair.reserve1,
                    token0_decimals,
                    token1_decimals,
                )
            } else {
                Err(anyhow::anyhow!("Invalid reserves for V2 price calculation"))
            }
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
    fn test_calculate_price_from_pair_data() {
        let pair = PairData {
            id: "test".to_string(),
            network: "ethereum".to_string(),
            dex_type: "uniswap_v2".to_string(),
            protocol_type: "amm_v2".to_string(),
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
            sqrt_price: None,
            tick: None,
        };
        
        let price = PriceCalculator::calculate_price_with_decimals(
            &pair.reserve0, 
            &pair.reserve1, 
            18, // WETH decimals
            6   // USDT decimals
        ).unwrap();
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
            protocol_type: "amm_v2".to_string(),
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
            sqrt_price: None,
            tick: None,
        };
        
        assert!(PriceCalculator::has_valid_reserves(&valid_pair));
        
        let invalid_pair = PairData {
            reserve0: "0".to_string(),
            reserve1: "1000".to_string(),
            ..valid_pair
        };
        
        assert!(!PriceCalculator::has_valid_reserves(&invalid_pair));
    }
    
    #[test]
    fn test_calculate_price_from_sqrt_price() {
        // 测试基本的sqrt_price计算功能
        // 使用一个简单的测试值
        let sqrt_price_x96 = "79228162514264337593543950336"; // 这是 Q96 = 2^96，对应价格为1
        let result = PriceCalculator::calculate_price_from_sqrt_price(sqrt_price_x96, 18, 18);
        assert!(result.is_ok());
        
        let price = result.unwrap();
        println!("Calculated price from sqrt_price: {}", price);
        // 验证价格约为1（相同小数位数的代币对）
        assert!(price > BigDecimal::from_str("0.9").unwrap() && price < BigDecimal::from_str("1.1").unwrap());
    }

    #[test]
    fn test_calculate_price_from_tick() {
        // 测试从 tick 计算价格
        // tick = 0 对应价格为 1
        let tick = "0";
        let result = PriceCalculator::calculate_price_from_tick(tick, 18, 18);
        assert!(result.is_ok());
        
        let price = result.unwrap();
        println!("Calculated price from tick: {}", price);
        // 验证价格约为1（tick=0时价格为1）
        assert!(price > BigDecimal::from_str("0.9").unwrap() && price < BigDecimal::from_str("1.1").unwrap());
    }

    #[test]
    fn test_calculate_v3_price_with_sqrt_price() {
        let pair = PairData {
            id: "test".to_string(),
            network: "ethereum".to_string(),
            dex_type: "UNI_V3".to_string(),
            protocol_type: "amm_v3".to_string(),
            token0: TokenInfo {
                 id: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(), // WETH
                 symbol: "WETH".to_string(),
                 name: "Wrapped Ether".to_string(),
                 decimals: "18".to_string(),
             },
             token1: TokenInfo {
                 id: "0xA0b86a33E6441E6C7D3E4C7C5C6C8C8C8C8C8C8C".to_string(), // 另一个18位小数的代币
                 symbol: "TOKEN1".to_string(),
                 name: "Test Token 1".to_string(),
                 decimals: "18".to_string(),
             },
            volume_usd: "1000000".to_string(),
            reserve_usd: "5000000".to_string(),
            tx_count: "1000".to_string(),
            reserve0: "0".to_string(),
            reserve1: "0".to_string(),
            fee_tier: "3000".to_string(),
             sqrt_price: Some("79228162514264337593543950336".to_string()), // Q96, 对应价格1
             tick: Some("0".to_string()),
        };

        let result = PriceCalculator::calculate_v3_price(&pair);
        assert!(result.is_ok());
        
        let price = result.unwrap();
         // 验证价格约为1（相同小数位数的代币对）
         assert!(price > BigDecimal::from_str("0.9").unwrap() && price < BigDecimal::from_str("1.1").unwrap());
    }

    #[test]
    fn test_calculate_v3_price_with_tick_fallback() {
        let pair = PairData {
            id: "test".to_string(),
            network: "ethereum".to_string(),
            dex_type: "UNI_V3".to_string(),
            protocol_type: "amm_v3".to_string(),
            token0: TokenInfo {
                 id: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(), // WETH
                 symbol: "WETH".to_string(),
                 name: "Wrapped Ether".to_string(),
                 decimals: "18".to_string(),
             },
             token1: TokenInfo {
                 id: "0xA0b86a33E6441E6C7D3E4C7C5C6C8C8C8C8C8C8C".to_string(), // 另一个18位小数的代币
                 symbol: "TOKEN1".to_string(),
                 name: "Test Token 1".to_string(),
                 decimals: "18".to_string(),
             },
            volume_usd: "1000000".to_string(),
            reserve_usd: "5000000".to_string(),
            tx_count: "1000".to_string(),
            reserve0: "0".to_string(),
            reserve1: "0".to_string(),
            fee_tier: "3000".to_string(),
             sqrt_price: None,
             tick: Some("0".to_string()),
        };

        let result = PriceCalculator::calculate_v3_price(&pair);
        assert!(result.is_ok());
        
        let price = result.unwrap();
         // 验证价格约为1（相同小数位数的代币对）
         assert!(price > BigDecimal::from_str("0.9").unwrap() && price < BigDecimal::from_str("1.1").unwrap());
    }

    #[test]
    fn test_calculate_v3_price_no_data() {
        let pair = PairData {
            id: "test".to_string(),
            network: "ethereum".to_string(),
            dex_type: "UNI_V3".to_string(),
            protocol_type: "amm_v3".to_string(),
            token0: TokenInfo {
                id: "token0".to_string(),
                symbol: "TOKEN0".to_string(),
                name: "Token 0".to_string(),
                decimals: "18".to_string(),
            },
            token1: TokenInfo {
                id: "token1".to_string(),
                symbol: "TOKEN1".to_string(),
                name: "Token 1".to_string(),
                decimals: "6".to_string(),
            },
            volume_usd: "1000000".to_string(),
            reserve_usd: "5000000".to_string(),
            tx_count: "1000".to_string(),
            reserve0: "1000000000000000000".to_string(),
            reserve1: "2000000000".to_string(),
            fee_tier: "3000".to_string(),
            sqrt_price: None,
            tick: None,
        };

        let result = PriceCalculator::calculate_v3_price(&pair);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_calculate_price_from_pair_v2() {
        let pair = PairData {
            id: "test".to_string(),
            network: "ethereum".to_string(),
            dex_type: "uniswap_v2".to_string(),
            protocol_type: "amm_v2".to_string(),
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
            volume_usd: "100000".to_string(),
            reserve_usd: "4000".to_string(),
            tx_count: "50".to_string(),
            reserve0: "1000000000000000000000".to_string(), // 1000 WETH (18 decimals)
            reserve1: "2000000000000".to_string(), // 2,000,000 USDT (6 decimals)
            fee_tier: "3000".to_string(),
            sqrt_price: None,
            tick: None,
        };
        
        let result = PriceCalculator::calculate_price_from_pair(&pair);
        assert!(result.is_ok());
        let price = result.unwrap();
        assert_eq!(price.to_string(), "2000");
    }
    
    #[test]
    fn test_calculate_price_from_pair_v3() {
        let pair = PairData {
            id: "test".to_string(),
            network: "ethereum".to_string(),
            dex_type: "UNI_V3".to_string(),
            protocol_type: "amm_v3".to_string(),
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
            volume_usd: "500000".to_string(),
            reserve_usd: "1000000".to_string(),
            tx_count: "100".to_string(),
            reserve0: "0".to_string(),
            reserve1: "0".to_string(),
            fee_tier: "3000".to_string(),
            sqrt_price: Some("79228162514264337593543950336".to_string()), // Q96
            tick: None,
        };
        
        let result = PriceCalculator::calculate_price_from_pair(&pair);
        assert!(result.is_ok());
        // 只验证V3计算方式被正确调用，不验证具体数值
    }
}