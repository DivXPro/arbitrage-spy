use anyhow::Result;
use bigdecimal::{BigDecimal, FromPrimitive, Zero};
use std::str::FromStr;
use crate::store::pair_manager::PairData;

use crate::config::protocol_types;

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
    /// 从储备量计算调整后的价格（考虑小数位差异）
    pub fn calculate_price_with_reserve(
        reserve0: &str,
        reserve1: &str,
        token0_decimals: u32,
        token1_decimals: u32,
    ) -> Result<BigDecimal> {
        // 先计算原始价格
        let raw_price = Self::calculate_raw_price_with_reserve(reserve0, reserve1)?;
        
        // 使用统一的小数位调整方法
        let adjusted_price = Self::adjust_price_for_decimals(raw_price, token0_decimals, token1_decimals);
        
        Ok(adjusted_price)
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
    /// 从储备量计算原始价格（不进行小数位调整）
    pub fn calculate_raw_price_with_reserve(
        reserve0: &str,
        reserve1: &str,
    ) -> Result<BigDecimal> {
        let reserve0_bd = BigDecimal::from_str(reserve0)
            .map_err(|e| anyhow::anyhow!("Invalid reserve0: {}", e))?;
        let reserve1_bd = BigDecimal::from_str(reserve1)
            .map_err(|e| anyhow::anyhow!("Invalid reserve1: {}", e))?;
        
        if reserve0_bd.is_zero() {
            return Err(anyhow::anyhow!("Reserve0 is zero, cannot calculate price"));
        }
        
        // 计算原始价格 (token1/token0)，不进行小数位调整
        let raw_price = &reserve1_bd / &reserve0_bd;
        
        // 添加价格合理性检查
        Self::validate_price(&raw_price)?;
        
        Ok(raw_price)
    }

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
    pub fn calculate_raw_price_from_sqrt_price(sqrt_price_x96: &str) -> Result<BigDecimal> {
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
        
        // 价格 = sqrt_price^2 (原始价格，不进行小数位调整)
        let price_raw = &sqrt_price_real * &sqrt_price_real;
        
        // 添加价格合理性检查
        Self::validate_price(&price_raw)?;
        
        Ok(price_raw)
    }

    /// 从 sqrt_price 计算调整后的价格（考虑小数位差异）
    /// 
    /// # 参数
    /// * `sqrt_price_x96` - sqrt(price) * 2^96 格式的价格
    /// * `token0_decimals` - token0 的小数位数
    /// * `token1_decimals` - token1 的小数位数
    /// 
    /// # 返回
    /// 调整后的 token1/token0 价格
    pub fn calculate_price_from_sqrt_price(
        sqrt_price_x96: &str,
        token0_decimals: u32,
        token1_decimals: u32,
    ) -> Result<BigDecimal> {
        // 先获取原始价格
        let price_raw = Self::calculate_raw_price_from_sqrt_price(sqrt_price_x96)?;
        
        // 使用统一的小数位调整方法
        let adjusted_price = Self::adjust_price_for_decimals(price_raw, token0_decimals, token1_decimals);
        
        Ok(adjusted_price)
    }
    
    /// 根据token小数位差异调整价格
    /// 
    /// # 参数
    /// * `raw_price` - 原始价格
    /// * `token0_decimals` - token0的小数位数
    /// * `token1_decimals` - token1的小数位数
    /// 
    /// # 返回
    /// 调整后的价格
    fn adjust_price_for_decimals(
        raw_price: BigDecimal,
        token0_decimals: u32,
        token1_decimals: u32,
    ) -> BigDecimal {
        let decimals_diff = token0_decimals as i32 - token1_decimals as i32;
        
        if decimals_diff == 0 {
            return raw_price;
        }
        
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
        
        let result = if decimals_diff > 0 {
            raw_price * adjustment
        } else {
            raw_price / adjustment
        };
        
        // 规范化精度以保持向后兼容性
        // 移除尾随的零，使输出格式与原方法一致
        result.normalized()
    }

    /// 验证价格是否在合理范围内
    /// 
    /// # 参数
    /// * `price` - 要验证的价格
    /// 
    /// # 返回
    /// 如果价格合理则返回Ok，否则返回错误
    fn validate_price(price: &BigDecimal) -> Result<()> {
        // 定义合理的价格范围
        let min_price = BigDecimal::from_str("1e-18")?; // 最小价格：1e-18
        let max_price = BigDecimal::from_str("1e18")?;  // 最大价格：1e18
        
        if price < &min_price {
            return Err(anyhow::anyhow!(
                "Price too small: {} (min: {}). This may indicate incorrect sqrt_price or token decimals.",
                price, min_price
            ));
        }
        
        if price > &max_price {
            return Err(anyhow::anyhow!(
                "Price too large: {} (max: {}). This may indicate incorrect sqrt_price or token decimals.",
                price, max_price
            ));
        }
        
        Ok(())
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
    pub fn calculate_raw_price_from_tick(tick: &str) -> Result<BigDecimal> {
        let tick_value = tick.parse::<i32>()
            .map_err(|e| anyhow::anyhow!("Invalid tick value: {}", e))?;
        
        // 价格 = 1.0001^tick (原始价格，不进行小数位调整)
        let price_raw = Self::TICK_BASE.powi(tick_value);
        
        // 转换为 BigDecimal
        let price_bd = BigDecimal::from_f64(price_raw)
            .ok_or_else(|| anyhow::anyhow!("Failed to convert price to BigDecimal"))?;
        
        // 添加价格合理性检查
        Self::validate_price(&price_bd)?;
        
        Ok(price_bd)
    }

    /// 从 tick 计算调整后的价格（考虑小数位差异）
    /// 
    /// # 参数
    /// * `tick` - tick 值
    /// * `token0_decimals` - token0 的小数位数
    /// * `token1_decimals` - token1 的小数位数
    /// 
    /// # 返回
    /// 调整后的 token1/token0 价格
    pub fn calculate_price_from_tick(
        tick: &str,
        token0_decimals: u32,
        token1_decimals: u32,
    ) -> Result<BigDecimal> {
        // 先获取原始价格
        let price_raw = Self::calculate_raw_price_from_tick(tick)?;
        
        // 使用统一的小数位调整方法
        let adjusted_price = Self::adjust_price_for_decimals(price_raw, token0_decimals, token1_decimals);
        
        Ok(adjusted_price)
    }
    
    /// 计算 V3 价格（基础版本，使用最基本的参数）
    /// 
    /// # 参数
    /// * `sqrt_price` - 可选的 sqrt_price_x96 字符串
    /// * `tick` - 可选的 tick 字符串
    /// * `token0_decimals` - token0 的小数位数

    
    /// 从 PairData 自动计算价格（根据 protocol_type 选择 V2 或 V3 计算方式）
    /// 
    /// # 参数
    /// * `pair` - 包含价格信息的 PairData
    /// 
    /// # 返回
    /// token1/token0 的价格
    pub fn calculate_price_from_pair(pair: &PairData) -> Result<BigDecimal> {
        let token0_decimals = pair.token0.decimals.parse::<u32>()
            .map_err(|e| anyhow::anyhow!("Invalid token0 decimals: {}", e))?;
        let token1_decimals = pair.token1.decimals.parse::<u32>()
            .map_err(|e| anyhow::anyhow!("Invalid token1 decimals: {}", e))?;

        // 根据 protocol_type 选择计算方式
        if pair.protocol_type == protocol_types::AMM_V3 {
            // 使用 V3 计算方式 - 使用调整后价格方法
            // 优先使用 sqrt_price
            if let Some(sqrt_price_str) = &pair.sqrt_price {
                if !sqrt_price_str.is_empty() && sqrt_price_str != "0" {
                    return Self::calculate_price_from_sqrt_price(sqrt_price_str, token0_decimals, token1_decimals);
                }
            }
            
            // fallback 到 tick
            if let Some(tick_str) = &pair.tick {
                if !tick_str.is_empty() {
                    return Self::calculate_price_from_tick(tick_str, token0_decimals, token1_decimals);
                }
            }
            
            Err(anyhow::anyhow!("No valid V3 price data (sqrt_price or tick) found"))
        } else {
            // 使用 V2 计算方式（默认）
            if Self::has_valid_reserves(pair) {
                Self::calculate_price_with_reserve(
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
    use crate::store::pair_manager::TokenInfo;
    use crate::config::dex_types;
    
    #[test]
    fn test_calculate_price_with_reserve() {
        // 测试独立的 calculate_price_with_reserve 方法
        let reserve0 = "1000000000000000000000"; // 1000 WETH (18 decimals)
        let reserve1 = "2000000000000"; // 2,000,000 USDT (6 decimals)
        let token0_decimals = 18;
        let token1_decimals = 6;
        
        let price = PriceCalculator::calculate_price_with_reserve(
            reserve0,
            reserve1,
            token0_decimals,
            token1_decimals,
        ).unwrap();
        // 预期价格: 2,000,000 USDT / 1000 WETH = 2000 USDT per WETH
        assert_eq!(price.to_string(), "2000");
    }
    
    #[test]
    fn test_calculate_raw_price_with_reserve() {
        // 测试原始价格计算（不进行小数位调整）
        let reserve0 = "1000000000000000000000"; // 1000 WETH (原始值)
        let reserve1 = "2000000000000"; // 2,000,000 USDT (原始值)
        
        let raw_price = PriceCalculator::calculate_raw_price_with_reserve(reserve0, reserve1).unwrap();
        
        // 原始价格应该是 reserve1/reserve0 = 2000000000000/1000000000000000000000 = 0.000000002
        let expected = BigDecimal::from_str("0.000000002").unwrap();
        assert_eq!(raw_price, expected);
        
        // 测试零储备量的错误情况
        let result = PriceCalculator::calculate_raw_price_with_reserve("0", reserve1);
        assert!(result.is_err());
        
        // 测试无效输入
        let result = PriceCalculator::calculate_raw_price_with_reserve("invalid", reserve1);
        assert!(result.is_err());
    }

    #[test]
    fn test_calculate_price_from_pair_data() {
        let pair = PairData {
            id: "test".to_string(),
            network: "ethereum".to_string(),
            dex: dex_types::UNISWAP_V2.to_string(),
            protocol_type: protocol_types::AMM_V2.to_string(),
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
        
        let price = PriceCalculator::calculate_price_with_reserve(
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
            dex: dex_types::UNISWAP_V2.to_string(),
            protocol_type: protocol_types::AMM_V2.to_string(),
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
        let result = PriceCalculator::calculate_raw_price_from_sqrt_price(sqrt_price_x96);
        assert!(result.is_ok());
        
        let price = result.unwrap();
        // 验证价格约为1（相同小数位数的代币对）
        assert!(price > BigDecimal::from_str("0.9").unwrap() && price < BigDecimal::from_str("1.1").unwrap());
        
        // 测试调整后价格
        let adjusted_result = PriceCalculator::calculate_price_from_sqrt_price(sqrt_price_x96, 18, 18);
        assert!(adjusted_result.is_ok());
        
        let adjusted_price = adjusted_result.unwrap();
        assert!(adjusted_price > BigDecimal::from_str("0.9").unwrap() && adjusted_price < BigDecimal::from_str("1.1").unwrap());
    }

    #[test]
    fn test_calculate_price_from_tick() {
        // 测试从 tick 计算价格
        // tick = 0 对应价格为 1
        let tick = "0";
        let result = PriceCalculator::calculate_raw_price_from_tick(tick);
        assert!(result.is_ok());
        
        let price = result.unwrap();
        // 验证价格约为1（tick=0时价格为1）
        assert!(price > BigDecimal::from_str("0.9").unwrap() && price < BigDecimal::from_str("1.1").unwrap());
        
        // 测试调整后价格
        let adjusted_result = PriceCalculator::calculate_price_from_tick(tick, 18, 18);
        assert!(adjusted_result.is_ok());
        
        let adjusted_price = adjusted_result.unwrap();
        assert!(adjusted_price > BigDecimal::from_str("0.9").unwrap() && adjusted_price < BigDecimal::from_str("1.1").unwrap());
    }


    
    #[test]
    fn test_calculate_price_from_pair_v2() {
        let pair = PairData {
            id: "test".to_string(),
            network: "ethereum".to_string(),
            dex: dex_types::UNISWAP_V2.to_string(),
            protocol_type: protocol_types::AMM_V2.to_string(),
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
            dex: dex_types::UNISWAP_V3.to_string(),
            protocol_type: protocol_types::AMM_V3.to_string(),
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
            sqrt_price: Some("79228162514264337593543950336".to_string()), // Q96
            tick: Some("0".to_string()),
        };

        let result = PriceCalculator::calculate_price_from_pair(&pair);
        assert!(result.is_ok());
        
        let price = result.unwrap();
        // 验证价格约为1（相同小数位数的代币对）
        assert!(price > BigDecimal::from_str("0.9").unwrap() && price < BigDecimal::from_str("1.1").unwrap());
    }



    #[test]
    fn test_official_uniswap_v3_examples() {
        // 测试1: sqrt_price计算验证（原始价格）
        // 来源: https://blog.uniswap.org/uniswap-v3-math-primer
        let sqrt_price_x96 = "2018382873588440326581633304624437";
        let usdc_decimals = 6;  // token0
        let weth_decimals = 18; // token1
        
        // 测试原始价格计算
        let raw_price = PriceCalculator::calculate_raw_price_from_sqrt_price(sqrt_price_x96).unwrap();
        assert!(raw_price > BigDecimal::from(0), "Raw price should be positive");
        
        // 测试调整后价格计算
        let adjusted_price = PriceCalculator::calculate_price_from_sqrt_price(sqrt_price_x96, usdc_decimals, weth_decimals).unwrap();
        
        // 根据小数位差异，调整后价格应该是原始价格除以10^12
        // 期望的调整后价格约为 0.000649004842701370...
        let expected = BigDecimal::from_str("0.000649004842701370").unwrap();
        let diff_percent = ((&adjusted_price - &expected).abs() / &expected) * BigDecimal::from(100);
        
        // 允许0.01%的误差
        assert!(diff_percent < BigDecimal::from_str("0.01").unwrap(), 
                "Adjusted price calculation error too large: {}%, calculated: {}, expected: {}", 
                diff_percent, adjusted_price, expected);
        
        // 验证原始价格（不考虑小数位调整）
        // 官方结果约为 649004842.70137（这是原始价格）
        let expected_raw = BigDecimal::from_str("649004842.70137").unwrap();
        let diff_percent_raw = ((&raw_price - &expected_raw).abs() / &expected_raw) * BigDecimal::from(100);
        
        // 允许0.01%的误差
        assert!(diff_percent_raw < BigDecimal::from_str("0.01").unwrap(), 
                "Raw price calculation error too large: {}%, calculated: {}, expected: {}", 
                diff_percent_raw, raw_price, expected_raw);

        // 测试2: tick计算验证（原始价格）
        // 使用与sqrt_price对应的tick值（约202920）
        let tick = "202920";
        let raw_price_tick = PriceCalculator::calculate_raw_price_from_tick(tick).unwrap();
        
        // 1.0001^202920 ≈ 649027383.8115474（这是原始价格，应该接近sqrt_price计算的结果）
        let expected_tick = BigDecimal::from_str("649027383.8115474").unwrap();
        let diff_percent_tick = ((&raw_price_tick - &expected_tick).abs() / &expected_tick) * BigDecimal::from(100);
        
        // 允许0.1%的误差（tick计算使用f64可能有精度损失）
        assert!(diff_percent_tick < BigDecimal::from_str("0.1").unwrap(),
                "Raw tick price calculation error too large: {}%, calculated: {}, expected: {}",
                diff_percent_tick, raw_price_tick, expected_tick);

        // 测试3: 基础验证 - tick=0应该等于1（原始价格）
        let price_tick_0 = PriceCalculator::calculate_raw_price_from_tick("0").unwrap();
        assert!((&price_tick_0 - BigDecimal::from(1)).abs() < BigDecimal::from_str("0.0001").unwrap(),
                "tick=0 should equal 1.0, got: {}", price_tick_0);

        // 测试4: sqrt_price=Q96应该等于1（原始价格）
        let price_sqrt_q96 = PriceCalculator::calculate_raw_price_from_sqrt_price("79228162514264337593543950336").unwrap();
        assert!((&price_sqrt_q96 - BigDecimal::from(1)).abs() < BigDecimal::from_str("0.0001").unwrap(),
                "sqrtPriceX96=Q96 should equal 1.0, got: {}", price_sqrt_q96);
    }

    #[test]
    fn test_tick_boundary_validation() {
        // 测试合理范围内的tick值
        let large_positive_tick = "100000";
        let large_negative_tick = "-100000";
        
        // 大的正tick应该能正常计算
        let large_positive_price = PriceCalculator::calculate_raw_price_from_tick(large_positive_tick);
        assert!(large_positive_price.is_ok(), "Large positive tick calculation failed");
        
        // 大的负tick应该能正常计算
        let large_negative_price = PriceCalculator::calculate_raw_price_from_tick(large_negative_tick);
        assert!(large_negative_price.is_ok(), "Large negative tick calculation failed");
        
        // 验证价格范围合理性
        if let Ok(price) = large_positive_price {
            assert!(price > BigDecimal::from(1), "Large positive tick should produce price > 1");
            assert!(price < BigDecimal::from_str("1e50").unwrap(), "Large positive tick price should be finite");
        }
        
        if let Ok(price) = large_negative_price {
            assert!(price < BigDecimal::from(1), "Large negative tick should produce price < 1");
            assert!(price > BigDecimal::from_str("1e-50").unwrap(), "Large negative tick price should be finite");
        }
        
        // 测试极端值（可能失败）
        let extreme_tick = "500000";
        let extreme_result = PriceCalculator::calculate_raw_price_from_tick(extreme_tick);
        
        // 如果极端值计算成功，价格应该非常大
        if let Ok(price) = extreme_result {
            assert!(price > BigDecimal::from_str("1e100").unwrap(), "Extreme tick should produce very large price");
        }
        // 如果失败也是可以接受的，因为超出了合理范围
    }
}