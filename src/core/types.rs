use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tabled::Tabled;

fn display_price(price: &BigDecimal) -> String {
    format!("{:.6}", price)
}

fn display_token_pair(token_pair: &TokenPair) -> String {
    format!("{}/{}", token_pair.token_a.symbol, token_pair.token_b.symbol)
}

#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct ArbitrageOpportunity {
    #[tabled(rename = "ID")]
    pub id: String,
    #[tabled(rename = "代币对", display_with = "display_token_pair")]
    pub token_pair: TokenPair,
    #[tabled(rename = "买入DEX")]
    pub buy_dex: String,
    #[tabled(rename = "卖出DEX")]
    pub sell_dex: String,
    #[tabled(rename = "买入价格", display_with = "display_price")]
    pub buy_price: BigDecimal,
    #[tabled(rename = "卖出价格", display_with = "display_price")]
    pub sell_price: BigDecimal,
    #[tabled(rename = "利润率%")]
    pub profit_percentage: f64,
    #[tabled(rename = "预估利润", display_with = "display_price")]
    pub estimated_profit: BigDecimal,
    #[tabled(skip)]
    pub liquidity: BigDecimal,
    #[tabled(skip)]
    pub gas_cost_estimate: BigDecimal,
    #[tabled(skip)]
    pub timestamp: DateTime<Utc>,
    #[tabled(rename = "置信度")]
    pub confidence_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TokenPair {
    pub token_a: Token,
    pub token_b: Token,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Token {
    pub address: String,
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    pub chain_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Price {
    pub token_pair: TokenPair,
    pub price: BigDecimal,
    pub liquidity: BigDecimal,
    pub dex: String,
    pub timestamp: DateTime<Utc>,
    pub block_number: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pool {
    pub id: String,
    pub dex: String,
    pub token_pair: TokenPair,
    pub reserve_a: BigDecimal,
    pub reserve_b: BigDecimal,
    pub fee_percentage: f64,
    pub total_liquidity: BigDecimal,
    pub volume_24h: BigDecimal,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexData {
    pub dex_name: String,
    pub pools: Vec<Pool>,
    pub prices: HashMap<TokenPair, Price>,
    pub last_sync: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketData {
    pub dex_data: HashMap<String, DexData>,
    pub arbitrage_opportunities: Vec<ArbitrageOpportunity>,
    pub last_scan: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasPrice {
    pub standard: BigDecimal,
    pub fast: BigDecimal,
    pub instant: BigDecimal,
    pub timestamp: DateTime<Utc>,
}

impl TokenPair {
    pub fn new(token_a: Token, token_b: Token) -> Self {
        // 确保 token 顺序一致性（按地址排序）
        if token_a.address < token_b.address {
            Self { token_a, token_b }
        } else {
            Self {
                token_a: token_b,
                token_b: token_a,
            }
        }
    }
    
    pub fn reverse(&self) -> Self {
        Self {
            token_a: self.token_b.clone(),
            token_b: self.token_a.clone(),
        }
    }
}

impl Token {
    pub fn new(address: String, symbol: String, name: String, decimals: u8, chain_id: u64) -> Self {
        Self {
            address: address.to_lowercase(),
            symbol,
            name,
            decimals,
            chain_id,
        }
    }
}

impl ArbitrageOpportunity {
    pub fn calculate_profit_after_gas(&self, gas_price: &BigDecimal) -> BigDecimal {
        &self.estimated_profit - (&self.gas_cost_estimate * gas_price)
    }
    
    pub fn is_profitable_after_gas(&self, gas_price: &BigDecimal, min_profit: &BigDecimal) -> bool {
        self.calculate_profit_after_gas(gas_price) > *min_profit
    }
}