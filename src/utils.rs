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
}