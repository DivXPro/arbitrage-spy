use anyhow::Result;
use bigdecimal::BigDecimal;
use num_traits::Zero;
use rand;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

/// 将字符串转换为 BigDecimal
pub fn str_to_bigdecimal(s: &str) -> Result<BigDecimal> {
    BigDecimal::from_str(s).map_err(|e| anyhow::anyhow!("Failed to parse BigDecimal: {}", e))
}

/// 将 Wei 转换为 Ether
pub fn wei_to_ether(wei: &BigDecimal) -> BigDecimal {
    let ether_divisor = BigDecimal::from_str("1000000000000000000").unwrap(); // 10^18
    wei / ether_divisor
}

/// 将 Ether 转换为 Wei
pub fn ether_to_wei(ether: &BigDecimal) -> BigDecimal {
    let ether_multiplier = BigDecimal::from_str("1000000000000000000").unwrap(); // 10^18
    ether * ether_multiplier
}

/// 根据代币精度调整数量
pub fn adjust_for_decimals(amount: &BigDecimal, decimals: u8) -> BigDecimal {
    let divisor = BigDecimal::from(10u64.pow(decimals as u32));
    amount / divisor
}

/// 计算价格影响
pub fn calculate_price_impact(amount_in: &BigDecimal, reserve_in: &BigDecimal, reserve_out: &BigDecimal) -> f64 {
    if reserve_in.is_zero() || reserve_out.is_zero() {
        return 100.0; // 100% 价格影响表示无流动性
    }
    
    let amount_in_with_fee = amount_in * BigDecimal::from_str("0.997").unwrap(); // 假设 0.3% 手续费
    let numerator = &amount_in_with_fee * reserve_out;
    let denominator = reserve_in + &amount_in_with_fee;
    let amount_out = &numerator / &denominator;
    
    let price_before = reserve_out / reserve_in;
    let price_after = (reserve_out - &amount_out) / (reserve_in + amount_in);
    
    let price_impact = (&price_before - &price_after) / &price_before;
    
    // 转换为百分比
    price_impact.to_string().parse::<f64>().unwrap_or(0.0) * 100.0
}

/// 计算 Uniswap V2 风格的输出数量
pub fn calculate_amount_out(
    amount_in: &BigDecimal,
    reserve_in: &BigDecimal,
    reserve_out: &BigDecimal,
    fee_percentage: f64,
) -> BigDecimal {
    if reserve_in.is_zero() || reserve_out.is_zero() {
        return BigDecimal::from(0);
    }
    
    let fee_multiplier = BigDecimal::from_str(&(1.0 - fee_percentage).to_string()).unwrap();
    let amount_in_with_fee = amount_in * fee_multiplier;
    let numerator = &amount_in_with_fee * reserve_out;
    let denominator = reserve_in + &amount_in_with_fee;
    
    &numerator / &denominator
}

/// 计算所需的输入数量
pub fn calculate_amount_in(
    amount_out: &BigDecimal,
    reserve_in: &BigDecimal,
    reserve_out: &BigDecimal,
    fee_percentage: f64,
) -> BigDecimal {
    if reserve_in.is_zero() || reserve_out.is_zero() || amount_out >= reserve_out {
        return BigDecimal::from(0);
    }
    
    let fee_multiplier = BigDecimal::from_str(&(1.0 - fee_percentage).to_string()).unwrap();
    let numerator = reserve_in * amount_out;
    let denominator = (reserve_out - amount_out) * &fee_multiplier;
    
    &numerator / &denominator
}

/// 生成唯一 ID
pub fn generate_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let random_suffix: u32 = rand::random();
    format!("{}-{}", timestamp, random_suffix)
}

/// 计算百分比差异
pub fn calculate_percentage_difference(price1: &BigDecimal, price2: &BigDecimal) -> f64 {
    if price1.is_zero() {
        return 0.0;
    }
    
    let diff = (price2 - price1).abs();
    let percentage = (&diff / price1) * BigDecimal::from(100);
    
    percentage.to_string().parse::<f64>().unwrap_or(0.0)
}

/// 验证以太坊地址格式
pub fn is_valid_ethereum_address(address: &str) -> bool {
    if !address.starts_with("0x") {
        return false;
    }
    
    if address.len() != 42 {
        return false;
    }
    
    address[2..].chars().all(|c| c.is_ascii_hexdigit())
}

/// 格式化大数字为可读字符串
pub fn format_big_number(number: &BigDecimal, decimals: usize) -> String {
    let rounded = number.with_scale(decimals as i64);
    rounded.to_string()
}

/// 将带小数点的reserve字符串转换为整数型字符串
/// 将浮点数转换为uint格式，移除小数点但保留所有数字
/// 例如: "123.45" -> "12345", "0.001" -> "1", "1000" -> "1000"
pub fn convert_decimal_to_integer_string(decimal_str: &str) -> Result<String> {
    if decimal_str.is_empty() {
        return Ok("0".to_string());
    }
    
    // 移除前导和尾随空格
    let trimmed = decimal_str.trim();
    
    // 如果是"0"或"0.0"等，直接返回"0"
    if let Ok(val) = trimmed.parse::<f64>() {
        if val == 0.0 {
            return Ok("0".to_string());
        }
    }
    
    // 分割整数部分和小数部分
    let parts: Vec<&str> = trimmed.split('.').collect();
    let integer_part = parts[0];
    let decimal_part = if parts.len() > 1 { parts[1] } else { "" };
    
    // 处理整数部分：移除前导零
    let mut integer_clean = integer_part.trim_start_matches('0');
    if integer_clean.is_empty() || integer_clean == "-" {
        integer_clean = "0";
    }
    
    // 处理小数部分：移除尾随零
     let decimal_clean = decimal_part.trim_end_matches('0');
     
     // 组合结果
     let mut result = String::new();
     
     // 添加整数部分
     if integer_clean == "0" && !decimal_clean.is_empty() {
         // 如果整数部分是0且有小数部分，不添加0
     } else {
         result.push_str(integer_clean);
     }
     
     // 添加小数部分（移除前导零）
     if !decimal_clean.is_empty() {
         let decimal_no_leading_zeros = decimal_clean.trim_start_matches('0');
         if !decimal_no_leading_zeros.is_empty() {
             result.push_str(decimal_no_leading_zeros);
         }
     }
    
    // 最终检查：如果结果为空，返回"0"
    if result.is_empty() {
        return Ok("0".to_string());
    }
    
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_wei_to_ether() {
        let wei = BigDecimal::from_str("1000000000000000000").unwrap();
        let ether = wei_to_ether(&wei);
        assert_eq!(ether, BigDecimal::from(1));
    }
    
    #[test]
    fn test_calculate_amount_out() {
        let amount_in = BigDecimal::from(1000);
        let reserve_in = BigDecimal::from(10000);
        let reserve_out = BigDecimal::from(20000);
        let fee = 0.003; // 0.3%
        
        let amount_out = calculate_amount_out(&amount_in, &reserve_in, &reserve_out, fee);
        assert!(amount_out > BigDecimal::from(0));
    }
    
    #[test]
    fn test_ethereum_address_validation() {
        assert!(is_valid_ethereum_address("0x742d35Cc6634C0532925a3b8D4C9db4C4C4C4C4C"));
        assert!(!is_valid_ethereum_address("0x742d35Cc6634C0532925a3b8D4C9db4C4C4C4C4"));
        assert!(!is_valid_ethereum_address("742d35Cc6634C0532925a3b8D4C9db4C4C4C4C4C"));
    }
    
    #[test]
    fn test_convert_decimal_to_integer_string() {
        // 测试基本的小数转换 - 保留所有数字
        assert_eq!(convert_decimal_to_integer_string("123.456").unwrap(), "123456");
        assert_eq!(convert_decimal_to_integer_string("123.45").unwrap(), "12345");
        assert_eq!(convert_decimal_to_integer_string("123.33").unwrap(), "12333");
        
        // 测试整数
        assert_eq!(convert_decimal_to_integer_string("1000").unwrap(), "1000");
        assert_eq!(convert_decimal_to_integer_string("1000.0").unwrap(), "1000");
        
        // 测试小数
        assert_eq!(convert_decimal_to_integer_string("0.999").unwrap(), "999");
        assert_eq!(convert_decimal_to_integer_string("0.001").unwrap(), "1");
        assert_eq!(convert_decimal_to_integer_string("0.123").unwrap(), "123");
        
        // 测试大数字
        assert_eq!(convert_decimal_to_integer_string("1234567890123456789.123").unwrap(), "1234567890123456789123");
        
        // 测试边界情况
        assert_eq!(convert_decimal_to_integer_string("0").unwrap(), "0");
        assert_eq!(convert_decimal_to_integer_string("0.0").unwrap(), "0");
        assert_eq!(convert_decimal_to_integer_string("0.000").unwrap(), "0");
        assert_eq!(convert_decimal_to_integer_string("").unwrap(), "0");
        
        // 测试前导零
        assert_eq!(convert_decimal_to_integer_string("000123.456").unwrap(), "123456");
        assert_eq!(convert_decimal_to_integer_string("0.00123").unwrap(), "123");
        
        // 测试尾随零
        assert_eq!(convert_decimal_to_integer_string("123.4500").unwrap(), "12345");
        assert_eq!(convert_decimal_to_integer_string("123.000").unwrap(), "123");
        
        // 测试整数（无小数点）
        assert_eq!(convert_decimal_to_integer_string("12345").unwrap(), "12345");
        
        // 测试带空格
        assert_eq!(convert_decimal_to_integer_string(" 123.45 ").unwrap(), "12345");
    }
}