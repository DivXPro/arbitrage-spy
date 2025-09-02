use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::env;

// Manual GraphQL query structure for Uniswap V2 pairs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairData {
    pub id: String,
    #[serde(default = "default_network")]
    pub network: String,
    #[serde(default = "default_dex_type")]
    pub dex_type: String,
    pub token0: TokenInfo,
    pub token1: TokenInfo,
    #[serde(rename = "volumeUSD")]
    pub volume_usd: String,
    #[serde(rename = "reserveUSD")]
    pub reserve_usd: String,
    #[serde(rename = "txCount")]
    pub tx_count: String,
}

fn default_network() -> String {
    "ethereum".to_string()
}

fn default_dex_type() -> String {
    "uniswap_v2".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub id: String,
    pub symbol: String,
    pub name: String,
    pub decimals: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GraphQLResponse {
    data: Option<PairsData>,
    errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PairsData {
    pairs: Vec<PairData>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GraphQLError {
    message: String,
}

#[derive(Debug, Serialize)]
struct GraphQLRequest {
    query: String,
    variables: serde_json::Value,
}

pub struct TheGraphClient {
    client: reqwest::Client,
    api_key: Option<String>,
    base_url: String,
    uniswap_v2_subgraph_id: String,
}

impl TheGraphClient {
    pub fn new() -> Self {
        let api_key = env::var("THEGRAPH_API_KEY").ok();
        let base_url = env::var("THEGRAPH_BASE_URL").unwrap_or_else(|_| "https://gateway.thegraph.com/api".to_string());
        let uniswap_v2_subgraph_id = env::var("UNISWAP_V2_SUBGRAPH_ID").unwrap_or_else(|_| "A3Np3RQbaBA6oKJgiwDJeo5T3zrYfGHPWFYayMwtNDum".to_string());

        Self {
            client: reqwest::Client::new(),
            api_key,
            base_url,
            uniswap_v2_subgraph_id,
        }
    }

    /// Get top Uniswap V2 pairs excluding stablecoins
    pub async fn get_top_pairs(&self, limit: i32) -> Result<Vec<PairData>> {
        // Try to fetch from TheGraph first, fallback to demo data if failed
        match self.fetch_pairs_from_graph(limit).await {
            Ok(pairs) if !pairs.is_empty() => Ok(pairs),
            _ => {
                log::warn!("TheGraph API 不可用，使用演示数据");
                Ok(self.get_demo_pairs(limit))
            }
        }
    }

    /// Fetch pairs from TheGraph API
    async fn fetch_pairs_from_graph(&self, limit: i32) -> Result<Vec<PairData>> {
        let query = r#"
            query GetTopPairs($first: Int!) {
                pairs(
                    first: $first,
                    orderBy: volumeUSD,
                    orderDirection: desc,
                    where: {
                        reserveUSD_gt: "10000"
                    }
                ) {
                    id
                    token0 {
                        id
                        symbol
                        name
                        decimals
                    }
                    token1 {
                        id
                        symbol
                        name
                        decimals
                    }
                    volumeUSD
                    reserveUSD
                    txCount
                }
            }
        "#;

        let variables = serde_json::json!({
            "first": limit
        });

        let request = GraphQLRequest {
            query: query.to_string(),
            variables,
        };

        let url = format!(
            "{}/subgraphs/id/{}",
            self.base_url, self.uniswap_v2_subgraph_id
        );
        
        let mut request_builder = self.client.post(&url).json(&request);
        
        // Add Bearer token if available
        if let Some(ref api_key) = self.api_key {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", api_key));
        }
        
        let response = request_builder
            .send()
            .await?
            .json::<GraphQLResponse>()
            .await?;

        if let Some(errors) = response.errors {
            return Err(anyhow!("GraphQL errors: {:?}", errors));
        }

        let pairs = response
            .data
            .ok_or_else(|| anyhow!("No data in response"))?
            .pairs;

        // Filter out stablecoins
        let filtered_pairs = self.filter_stablecoins(pairs);

        // Limit to requested number
        Ok(filtered_pairs.into_iter().take(limit as usize).collect())
    }

    /// Filter out stablecoin pairs
    fn filter_stablecoins(&self, pairs: Vec<PairData>) -> Vec<PairData> {
        let stablecoins: HashSet<&str> = [
            "USDT", "USDC", "DAI", "BUSD", "TUSD", "USDP", "FRAX", "LUSD", "MIM", "USDD", "GUSD",
            "SUSD", "USDN", "USTC", "HUSD", "DUSD", "OUSD", "USDK", "USDS", "USDX", "USDR", "USDB",
            "USDE", "USDF", "USDH", "USDJ", "USDL", "USDM", "USDO", "USDQ", "USDT", "USDV", "USDW",
            "USDY", "USDZ", "VAI", "VUSD", "YUSD", "ZUSD", "CUSD", "EURC", "EURS", "EURT", "JEUR",
            "AGEUR", "CEUR", "EUROC",
        ]
        .iter()
        .copied()
        .collect();

        pairs
            .into_iter()
            .filter(|pair| {
                !stablecoins.contains(pair.token0.symbol.as_str())
                    && !stablecoins.contains(pair.token1.symbol.as_str())
            })
            .collect()
    }

    /// Get pair information by address
    pub async fn get_pair_by_address(&self, pair_address: &str) -> Result<Option<PairData>> {
        let query = r#"
            query GetPairByAddress($id: ID!) {
                pair(id: $id) {
                    id
                    token0 {
                        id
                        symbol
                        name
                        decimals
                    }
                    token1 {
                        id
                        symbol
                        name
                        decimals
                    }
                    volumeUSD
                    reserveUSD
                    txCount
                }
            }
        "#;

        let variables = serde_json::json!({
            "id": pair_address.to_lowercase()
        });

        let request = GraphQLRequest {
            query: query.to_string(),
            variables,
        };

        let url = format!(
            "{}/{}",
            self.base_url, self.uniswap_v2_subgraph_id
        );
        
        let mut request_builder = self.client.post(&url).json(&request);
        
        // Add Bearer token if available
        if let Some(ref api_key) = self.api_key {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", api_key));
        }
        
        let response = request_builder
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        if let Some(errors) = response.get("errors") {
            return Err(anyhow!("GraphQL errors: {:?}", errors));
        }

        let pair_data = response
            .get("data")
            .and_then(|data| data.get("pair"))
            .and_then(|pair| serde_json::from_value(pair.clone()).ok());

        Ok(pair_data)
    }

    /// Generate demo pairs for testing when TheGraph API is unavailable
    fn get_demo_pairs(&self, limit: i32) -> Vec<PairData> {
        let demo_pairs = vec![
            PairData {
                id: "0xa478c2975ab1ea89e8196811f51a7b7ade33eb11".to_string(),
                network: "ethereum".to_string(),
                dex_type: "uniswap_v2".to_string(),
                token0: TokenInfo {
                    id: "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2".to_string(),
                    symbol: "WETH".to_string(),
                    name: "Wrapped Ether".to_string(),
                    decimals: "18".to_string(),
                },
                token1: TokenInfo {
                    id: "0xdac17f958d2ee523a2206206994597c13d831ec7".to_string(),
                    symbol: "USDT".to_string(),
                    name: "Tether USD".to_string(),
                    decimals: "6".to_string(),
                },
                volume_usd: "50000000".to_string(),
                reserve_usd: "200000000".to_string(),
                tx_count: "5000".to_string(),
            },
            PairData {
                id: "0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc".to_string(),
                network: "ethereum".to_string(),
                dex_type: "uniswap_v2".to_string(),
                token0: TokenInfo {
                    id: "0xa0b86a33e6180d93c6e6b3d3d4dae2c6b5b8b8b8".to_string(),
                    symbol: "WETH".to_string(),
                    name: "Wrapped Ether".to_string(),
                    decimals: "18".to_string(),
                },
                token1: TokenInfo {
                    id: "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984".to_string(),
                    symbol: "UNI".to_string(),
                    name: "Uniswap".to_string(),
                    decimals: "18".to_string(),
                },
                volume_usd: "30000000".to_string(),
                reserve_usd: "150000000".to_string(),
                tx_count: "3000".to_string(),
            },
            PairData {
                id: "0xd3d2e2692501a5c9ca623199d38826e513033a17".to_string(),
                network: "ethereum".to_string(),
                dex_type: "uniswap_v2".to_string(),
                token0: TokenInfo {
                    id: "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2".to_string(),
                    symbol: "WETH".to_string(),
                    name: "Wrapped Ether".to_string(),
                    decimals: "18".to_string(),
                },
                token1: TokenInfo {
                    id: "0x514910771af9ca656af840dff83e8264ecf986ca".to_string(),
                    symbol: "LINK".to_string(),
                    name: "ChainLink Token".to_string(),
                    decimals: "18".to_string(),
                },
                volume_usd: "25000000".to_string(),
                reserve_usd: "120000000".to_string(),
                tx_count: "2500".to_string(),
            },
        ];

        // Filter out stablecoins and limit results
        let filtered = self.filter_stablecoins(demo_pairs);
        filtered.into_iter().take(limit as usize).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_top_pairs() {
        let client = TheGraphClient::new();
        let result = client.get_top_pairs(10).await;

        match result {
            Ok(pairs) => {
                println!("Found {} pairs", pairs.len());
                for pair in pairs.iter().take(3) {
                    println!(
                        "Pair: {} - {}/{}",
                        pair.id, pair.token0.symbol, pair.token1.symbol
                    );
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
    }

    #[test]
    fn test_stablecoin_filter() {
        let client = TheGraphClient::new();
        let test_pairs = vec![
            PairData {
                id: "0x1".to_string(),
                network: "ethereum".to_string(),
                dex_type: "uniswap_v2".to_string(),
                token0: TokenInfo {
                    id: "0xa".to_string(),
                    symbol: "WETH".to_string(),
                    name: "Wrapped Ether".to_string(),
                    decimals: "18".to_string(),
                },
                token1: TokenInfo {
                    id: "0xb".to_string(),
                    symbol: "USDC".to_string(),
                    name: "USD Coin".to_string(),
                    decimals: "6".to_string(),
                },
                volume_usd: "1000000".to_string(),
                reserve_usd: "5000000".to_string(),
                tx_count: "1000".to_string(),
            },
            PairData {
                id: "0x2".to_string(),
                network: "ethereum".to_string(),
                dex_type: "uniswap_v2".to_string(),
                token0: TokenInfo {
                    id: "0xc".to_string(),
                    symbol: "WETH".to_string(),
                    name: "Wrapped Ether".to_string(),
                    decimals: "18".to_string(),
                },
                token1: TokenInfo {
                    id: "0xd".to_string(),
                    symbol: "UNI".to_string(),
                    name: "Uniswap".to_string(),
                    decimals: "18".to_string(),
                },
                volume_usd: "800000".to_string(),
                reserve_usd: "4000000".to_string(),
                tx_count: "800".to_string(),
            },
        ];

        let filtered = client.filter_stablecoins(test_pairs);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].token1.symbol, "UNI");
    }
}
