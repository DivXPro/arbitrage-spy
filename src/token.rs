use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::time::{sleep, Duration};
use rusqlite::{Connection};

/// Token information from CoinGecko API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub id: String,
    pub symbol: String,
    pub name: String,
    pub platforms: HashMap<String, Option<String>>, // platform -> contract address
    pub market_cap_rank: Option<u32>,
    pub current_price: Option<f64>,
    pub market_cap: Option<f64>,
    pub total_volume: Option<f64>,
    pub price_change_percentage_24h: Option<f64>,
}

/// Token list with metadata
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenList {
    pub tokens: Vec<Token>,
    pub last_updated: DateTime<Utc>,
    pub total_count: usize,
}

/// CoinGecko API response for coins list
#[derive(Debug, Deserialize)]
struct CoinGeckoToken {
    id: String,
    symbol: String,
    name: String,
    platforms: HashMap<String, Option<String>>,
}

/// CoinGecko API response for market data
#[derive(Debug, Deserialize)]
struct CoinGeckoMarketData {
    id: String,
    symbol: String,
    name: String,
    current_price: Option<f64>,
    market_cap: Option<f64>,
    market_cap_rank: Option<u32>,
    total_volume: Option<f64>,
    price_change_percentage_24h: Option<f64>,
}

/// Token manager for fetching and caching token data
pub struct TokenManager {
    client: Client,
    api_base_url: String,
    api_key: Option<String>,
    db_path: Option<String>,
}

impl TokenManager {
    /// Create a new TokenManager instance
    pub fn new(db_path: Option<String>) -> Self {
        let api_key = std::env::var("COINGECKO_API_KEY").ok();

        Self {
            client: Client::new(),
            api_base_url: "https://api.coingecko.com/api/v3".to_string(),
            api_key,
            db_path,
        }
    }


    /// Fetch token list from CoinGecko API
    pub async fn fetch_tokens(&self, limit: Option<usize>) -> Result<TokenList> {
        log::info!("Fetching token list from CoinGecko API...");

        // First, get the basic coin list with platform information
        let coins_url = format!("{}/coins/list?include_platform=true", self.api_base_url);
        let mut request = self
            .client
            .get(&coins_url)
            .header("User-Agent", "arbitrage-spy/0.1.0");

        // Add API key header if available
        if let Some(ref api_key) = self.api_key {
            request = request.header("x-cg-demo-api-key", api_key);
        }

        let coins_response = request.send().await?;

        if !coins_response.status().is_success() {
            return Err(anyhow!(
                "Failed to fetch coins list: {}",
                coins_response.status()
            ));
        }

        let coins: Vec<CoinGeckoToken> = coins_response.json().await?;
        log::info!("Fetched {} coins from CoinGecko", coins.len());

        // Filter coins that have Ethereum platform addresses
        let mut ethereum_tokens: Vec<_> = coins
            .into_iter()
            .filter(|coin| {
                coin.platforms.get("ethereum").is_some()
                    && coin.platforms.get("ethereum").unwrap().is_some()
            })
            .collect();
        
        // Apply limit if specified
        if let Some(limit_value) = limit {
            ethereum_tokens.truncate(limit_value);
        }

        log::info!("Found {} Ethereum tokens", ethereum_tokens.len());

        // Get market data for these tokens
        let mut tokens = self.fetch_market_data(ethereum_tokens).await?;

        // Sort by market cap rank (if available)
        tokens.sort_by(|a, b| match (a.market_cap_rank, b.market_cap_rank) {
            (Some(rank_a), Some(rank_b)) => rank_a.cmp(&rank_b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.symbol.cmp(&b.symbol),
        });

        let token_list = TokenList {
            total_count: tokens.len(),
            tokens,
            last_updated: Utc::now(),
        };

        log::info!("Successfully fetched {} tokens", token_list.total_count);
        Ok(token_list)
    }

    /// Fetch market data for a list of tokens
    async fn fetch_market_data(&self, ethereum_tokens: Vec<CoinGeckoToken>) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        let batch_size = 100;

        for chunk in ethereum_tokens.chunks(batch_size) {
            let ids: Vec<String> = chunk.iter().map(|t| t.id.clone()).collect();
            let ids_str = ids.join(",");

            let market_url = format!(
                "{}/coins/markets?vs_currency=usd&ids={}&order=market_cap_desc&per_page={}&page=1&sparkline=false",
                self.api_base_url, ids_str, batch_size
            );

            let mut market_request = self
                .client
                .get(&market_url)
                .header("User-Agent", "arbitrage-spy/0.1.0");

            // Add API key header if available
            if let Some(ref api_key) = self.api_key {
                market_request = market_request.header("x-cg-demo-api-key", api_key);
            }

            let market_response = market_request.send().await?;

            if market_response.status().is_success() {
                let market_data: Vec<CoinGeckoMarketData> = market_response.json().await?;

                // Merge coin info with market data
                for coin in chunk {
                    let market_info = market_data.iter().find(|m| m.id == coin.id);

                    let token = Token {
                        id: coin.id.clone(),
                        symbol: coin.symbol.clone(),
                        name: coin.name.clone(),
                        platforms: coin.platforms.clone(),
                        market_cap_rank: market_info.and_then(|m| m.market_cap_rank),
                        current_price: market_info.and_then(|m| m.current_price),
                        market_cap: market_info.and_then(|m| m.market_cap),
                        total_volume: market_info.and_then(|m| m.total_volume),
                        price_change_percentage_24h: market_info
                            .and_then(|m| m.price_change_percentage_24h),
                    };

                    tokens.push(token);
                }
            } else {
                log::warn!(
                    "Failed to fetch market data for batch: {}",
                    market_response.status()
                );

                // Add tokens without market data
                for coin in chunk {
                    let token = Token {
                        id: coin.id.clone(),
                        symbol: coin.symbol.clone(),
                        name: coin.name.clone(),
                        platforms: coin.platforms.clone(),
                        market_cap_rank: None,
                        current_price: None,
                        market_cap: None,
                        total_volume: None,
                        price_change_percentage_24h: None,
                    };

                    tokens.push(token);
                }
            }

            // Rate limiting: wait between requests (30 requests per minute max)
            if chunk.len() == batch_size {
                sleep(Duration::from_millis(2000)).await;
            }
        }

        Ok(tokens)
    }

    /// Get token list from database
    pub async fn get_tokens(&self, limit: Option<usize>) -> Result<TokenList> {
        // Require database path to be configured
        let db_path = self.db_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Database path not configured. Use new_with_db() to initialize TokenManager with database support."))?;
        
        // Load from database
        match self.load_from_database(db_path, limit) {
            Ok(token_list) => {
                log::info!("Loaded {} tokens from database", token_list.tokens.len());
                return Ok(token_list);
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to load tokens from database: {}", e));
            }
        }
    }

    /// Load token list from database
    fn load_from_database(&self, db_path: &str, limit: Option<usize>) -> Result<TokenList> {
        let conn = Connection::open(db_path)?;
        
        let (query, params_vec): (&str, Vec<rusqlite::types::Value>) = if let Some(limit_val) = limit {
            (
                "SELECT id, symbol, name, market_cap_rank, current_price, market_cap, total_volume, price_change_percentage_24h, platforms FROM tokens ORDER BY market_cap_rank ASC LIMIT ?1",
                vec![rusqlite::types::Value::Integer(limit_val as i64)]
            )
        } else {
            (
                "SELECT id, symbol, name, market_cap_rank, current_price, market_cap, total_volume, price_change_percentage_24h, platforms FROM tokens ORDER BY market_cap_rank ASC",
                vec![]
            )
        };
        
        let mut stmt = conn.prepare(query)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params_vec), |row| {
            let platforms_json: String = row.get(8)?;
            let platforms: HashMap<String, Option<String>> = 
                serde_json::from_str(&platforms_json).unwrap_or_default();
            
            Ok(Token {
                id: row.get(0)?,
                symbol: row.get(1)?,
                name: row.get(2)?,
                platforms,
                market_cap_rank: row.get(3)?,
                current_price: row.get(4)?,
                market_cap: row.get(5)?,
                total_volume: row.get(6)?,
                price_change_percentage_24h: row.get(7)?,
            })
        })?;
        
        let mut tokens = Vec::new();
        for token_result in rows {
            tokens.push(token_result?);
        }
        
        let total_count = tokens.len();
        
        Ok(TokenList {
            tokens,
            last_updated: Utc::now(),
            total_count,
        })
    }

    /// Get token by symbol
    pub async fn get_token_by_symbol(&self, symbol: &str) -> Result<Option<Token>> {
        let token_list = self.get_tokens(None).await?;
        Ok(token_list
            .tokens
            .into_iter()
            .find(|t| t.symbol.to_lowercase() == symbol.to_lowercase()))
    }

    /// Get token by contract address
    pub async fn get_token_by_address(&self, address: &str) -> Result<Option<Token>> {
        let token_list = self.get_tokens(None).await?;
        Ok(token_list.tokens.into_iter().find(|t| {
            t.platforms
                .values()
                .any(|addr| addr.as_ref().map(|a| a.to_lowercase()) == Some(address.to_lowercase()))
        }))
    }

    /// Get top tokens by market cap
    pub async fn get_top_tokens(&self, limit: usize) -> Result<Vec<Token>> {
        let token_list = self.get_tokens(None).await?;
        Ok(token_list.tokens.into_iter().take(limit).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_serialization() {
        let token = Token {
            id: "ethereum".to_string(),
            symbol: "ETH".to_string(),
            name: "Ethereum".to_string(),
            platforms: HashMap::new(),
            market_cap_rank: Some(2),
            current_price: Some(2000.0),
            market_cap: Some(240000000000.0),
            total_volume: Some(10000000000.0),
            price_change_percentage_24h: Some(5.2),
        };

        let json = serde_json::to_string(&token).unwrap();
        let deserialized: Token = serde_json::from_str(&json).unwrap();

        assert_eq!(token.id, deserialized.id);
        assert_eq!(token.symbol, deserialized.symbol);
    }
}
