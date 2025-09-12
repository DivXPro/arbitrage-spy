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
    #[serde(rename = "reserve0")]
    pub reserve0: String,
    #[serde(rename = "reserve1")]
    pub reserve1: String,
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
struct GraphQLV3Response {
    data: Option<PoolsData>,
    errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PairsData {
    pairs: Vec<PairData>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PoolsData {
    pools: Vec<PoolData>,
}

// Uniswap V3 Pool data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolData {
    pub id: String,
    pub token0: TokenInfo,
    pub token1: TokenInfo,
    #[serde(rename = "volumeUSD")]
    pub volume_usd: String,
    #[serde(rename = "totalValueLockedUSD")]
    pub total_value_locked_usd: String,
    #[serde(rename = "txCount")]
    pub tx_count: String,
    #[serde(rename = "totalValueLockedToken0")]
    pub total_value_locked_token0: String,
    #[serde(rename = "totalValueLockedToken1")]
    pub total_value_locked_token1: String,
    #[serde(rename = "feeTier")]
    pub fee_tier: String,
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
    uniswap_v3_subgraph_id: String,
}

// Convert V3 Pool to V2 PairData format for compatibility
impl From<PoolData> for PairData {
    fn from(pool: PoolData) -> Self {
        PairData {
            id: pool.id,
            network: "ethereum".to_string(),
            dex_type: "uniswap_v3".to_string(),
            token0: pool.token0,
            token1: pool.token1,
            volume_usd: pool.volume_usd,
            reserve_usd: pool.total_value_locked_usd,
            tx_count: pool.tx_count,
            reserve0: pool.total_value_locked_token0,
            reserve1: pool.total_value_locked_token1,
        }
    }
}

impl TheGraphClient {
    pub fn new() -> Self {
        let api_key = env::var("THEGRAPH_API_KEY").ok();
        let base_url = env::var("THEGRAPH_BASE_URL").unwrap_or_else(|_| "https://gateway.thegraph.com/api".to_string());
        let uniswap_v2_subgraph_id = env::var("UNISWAP_V2_SUBGRAPH_ID").unwrap_or_else(|_| "A3Np3RQbaBA6oKJgiwDJeo5T3zrYfGHPWFYayMwtNDum".to_string());
        let uniswap_v3_subgraph_id = env::var("UNISWAP_V3_SUBGRAPH_ID").unwrap_or_else(|_| "5zvR82QoaXYFyDEKLZ9t6v9adgnptxYpKpSbxtgVENFV".to_string());

        Self {
            client: reqwest::Client::new(),
            api_key,
            base_url,
            uniswap_v2_subgraph_id,
            uniswap_v3_subgraph_id,
        }
    }

    /// Fetch V3 pools by token from TheGraph API
    async fn fetch_v3_pools_by_token_from_graph(&self, token_address: &str, limit: i32) -> Result<Vec<PoolData>> {
        let query = r#"
            query GetPoolsByToken($token: String!, $first: Int!) {
                pools(
                    first: $first,
                    orderBy: totalValueLockedUSD,
                    orderDirection: desc,
                    where: {
                        and: [
                            {
                                or: [
                                    { token0: $token },
                                    { token1: $token }
                                ]
                            },
                            { totalValueLockedUSD_gt: "1000" }
                        ]
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
                    totalValueLockedUSD
                    txCount
                    totalValueLockedToken0
                    totalValueLockedToken1
                    feeTier
                }
            }
        "#;

        let variables = serde_json::json!({
            "token": token_address.to_lowercase(),
            "first": limit
        });

        let request = GraphQLRequest {
            query: query.to_string(),
            variables,
        };

        let url = format!(
            "{}/subgraphs/id/{}",
            self.base_url, self.uniswap_v3_subgraph_id
        );
        
        let mut request_builder = self.client.post(&url).json(&request);
        
        // Add Bearer token if available
        if let Some(ref api_key) = self.api_key {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", api_key));
        }
        
        let response = request_builder
            .send()
            .await?
            .json::<GraphQLV3Response>()
            .await?;

        if let Some(errors) = response.errors {
            return Err(anyhow!("GraphQL errors: {:?}", errors));
        }

        let pools = response
            .data
            .ok_or_else(|| anyhow!("No data in response"))?
            .pools;

        Ok(pools)
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

    /// Get pairs by token address
    /// Get V3 pools by token address
    pub async fn get_v3_pools_by_token(&self, token_address: &str, limit: i32) -> Result<Vec<PairData>> {
        match self.fetch_v3_pools_by_token_from_graph(token_address, limit).await {
            Ok(pools) if !pools.is_empty() => {
                // Convert V3 pools to PairData format
                let pairs: Vec<PairData> = pools.into_iter().map(|pool| pool.into()).collect();
                Ok(pairs)
            }
            _ => {
                log::warn!("Uniswap V3 TheGraph API 不可用，token: {}", token_address);
                Ok(vec![])
            }
        }
    }

    pub async fn get_pairs_by_token(&self, token_address: &str, limit: i32) -> Result<Vec<PairData>> {
        let query = r#"
            query GetPairsByToken($token: String!, $first: Int!) {
                pairs(
                    first: $first,
                    orderBy: volumeUSD,
                    orderDirection: desc,
                    where: {
                        or: [
                            { token0: $token, reserveUSD_gt: "1000" },
                            { token1: $token, reserveUSD_gt: "1000" }
                        ]
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
                    reserve0
                    reserve1
                }
            }
        "#;

        let variables = serde_json::json!({
            "token": token_address.to_lowercase(),
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

        Ok(filtered_pairs)
    }

}

#[cfg(test)]
mod tests {
    use super::*;

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
                reserve0: "1000000000000000000000".to_string(),
                reserve1: "2000000000".to_string(),
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
                reserve0: "800000000000000000000".to_string(),
                reserve1: "1000000000000000000000000".to_string(),
            },
        ];

        let filtered = client.filter_stablecoins(test_pairs);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].token1.symbol, "UNI");
    }
}
