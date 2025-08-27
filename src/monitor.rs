use anyhow::Result;
use bigdecimal::BigDecimal;
use chrono::Utc;
use log::{error, info, warn};
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
use tabled::{settings::Style, Table};
use tokio::time;

use crate::config::Config;
use crate::dex::balancer::BalancerProvider;
use crate::dex::curve::CurveProvider;
use crate::dex::pancakeswap::PancakeSwapProvider;
use crate::dex::sushiswap::SushiSwapProvider;
use crate::dex::uniswap::UniswapProvider;
use crate::dex::{DexManager, DexProvider};
use crate::types::{ArbitrageOpportunity, GasPrice, Price, Token, TokenPair};
use crate::utils::{calculate_percentage_difference, generate_id};

pub struct ArbitrageMonitor {
    config: Config,
    dex_manager: DexManager,
}

impl ArbitrageMonitor {
    pub async fn new(config: Config) -> Result<Self> {
        let mut dex_manager = DexManager::new();

        // 初始化所有启用的 DEX 提供者
        for (dex_name, dex_config) in &config.dex_configs {
            if !dex_config.enabled {
                continue;
            }

            info!("正在初始化 DEX 提供者: {}", dex_name);
            let provider: Box<dyn DexProvider + Send + Sync> = match dex_name.as_str() {
                "uniswap_v2" => {
                    info!("创建 Uniswap V2 提供者");
                    Box::new(UniswapProvider::new(dex_config.clone()))
                }
                "sushiswap" => Box::new(SushiSwapProvider::new(dex_config.clone())),
                "pancakeswap" => Box::new(PancakeSwapProvider::new(dex_config.clone())),
                "curve" => Box::new(CurveProvider::new(dex_config.clone())),
                "balancer" => Box::new(BalancerProvider::new(dex_config.clone())),
                _ => {
                    warn!("Unknown DEX provider: {}", dex_name);
                    continue;
                }
            };

            // 健康检查
            match provider.health_check().await {
                Ok(true) => {
                    info!("DEX provider {} is healthy", dex_name);
                    dex_manager.add_provider(provider);
                }
                Ok(false) => {
                    warn!("DEX provider {} failed health check", dex_name);
                }
                Err(e) => {
                    error!("Error checking health of DEX provider {}: {}", dex_name, e);
                }
            }
        }

        Ok(Self {
            config,
            dex_manager,
        })
    }

    pub async fn start_scan(&mut self) {
        // 启动监控循环
        let mut interval = time::interval(Duration::from_secs(10));

        loop {
            interval.tick().await;

            match self.scan_opportunities().await {
                Ok(opportunities) => {
                    if !opportunities.is_empty() {
                        info!("发现 {} 个套利机会", opportunities.len());
                        for opportunity in opportunities {
                            info!("套利机会: {:?}", opportunity);
                        }
                    }
                }
                Err(e) => {
                    error!("扫描套利机会时出错: {}", e);
                }
            }
        }
    }

    pub async fn scan_opportunities(&mut self) -> Result<Vec<ArbitrageOpportunity>> {
        info!("开始扫描套利机会...");

        // 创建要监控的代币对
        let token_pairs = self.create_token_pairs();
        info!("监控 {} 个代币对", token_pairs.len());

        // 从所有 DEX 获取价格
        let all_prices: HashMap<String, HashMap<TokenPair, Price>> = self
            .dex_manager
            .get_prices_from_all_dexes(&token_pairs)
            .await?;

        // 分析套利机会
        let opportunities = self.analyze_arbitrage_opportunities(all_prices).await?;

        info!("发现 {} 个潜在套利机会", opportunities.len());

        let mut display_opportunities = if opportunities.is_empty() {
            // 演示模式：如果没有找到真实机会，显示模拟数据
            info!("演示模式：显示模拟套利机会");
            self.create_demo_opportunities()
        } else {
            opportunities.clone()
        };

        // 按利润率降序排序
        display_opportunities.sort_by(|a, b| {
            b.profit_percentage
                .partial_cmp(&a.profit_percentage)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 始终显示表格
        self.display_opportunities_table(&display_opportunities);

        Ok(opportunities)
    }

    /// 以表格形式显示套利机会
    fn display_opportunities_table(&self, opportunities: &[ArbitrageOpportunity]) {
        println!("\n🔍 发现的套利机会:");
        println!("{}", "=".repeat(120));

        let table = Table::new(opportunities).with(Style::rounded()).to_string();

        println!("{}", table);
        println!("{}", "=".repeat(120));
        println!();
    }

    /// 创建演示套利机会数据
    fn create_demo_opportunities(&self) -> Vec<ArbitrageOpportunity> {
        use crate::types::Token;

        let eth = Token {
            address: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
            symbol: "ETH".to_string(),
            name: "Ethereum".to_string(),
            decimals: 18,
            chain_id: 1,
        };

        let usdc = Token {
            address: "0xA0b86a33E6441b8C4505E2E0c41416c5c5E0E8E8".to_string(),
            symbol: "USDC".to_string(),
            name: "USD Coin".to_string(),
            decimals: 6,
            chain_id: 1,
        };

        let token_pair = TokenPair {
            token_a: eth,
            token_b: usdc,
        };

        vec![
            ArbitrageOpportunity {
                id: "demo_001".to_string(),
                token_pair: token_pair.clone(),
                buy_dex: "Uniswap V2".to_string(),
                sell_dex: "SushiSwap".to_string(),
                buy_price: BigDecimal::from_str("2450.123456").unwrap(),
                sell_price: BigDecimal::from_str("2465.789012").unwrap(),
                profit_percentage: 0.64,
                estimated_profit: BigDecimal::from_str("15.665556").unwrap(),
                liquidity: BigDecimal::from_str("1000000").unwrap(),
                gas_cost_estimate: BigDecimal::from_str("0.005").unwrap(),
                confidence_score: 0.85,
                timestamp: Utc::now(),
            },
            ArbitrageOpportunity {
                id: "demo_002".to_string(),
                token_pair: token_pair.clone(),
                buy_dex: "PancakeSwap".to_string(),
                sell_dex: "Curve".to_string(),
                buy_price: BigDecimal::from_str("2448.567890").unwrap(),
                sell_price: BigDecimal::from_str("2470.123456").unwrap(),
                profit_percentage: 0.88,
                estimated_profit: BigDecimal::from_str("21.555566").unwrap(),
                liquidity: BigDecimal::from_str("2500000").unwrap(),
                gas_cost_estimate: BigDecimal::from_str("0.008").unwrap(),
                confidence_score: 0.92,
                timestamp: Utc::now(),
            },
        ]
    }

    fn create_token_pairs(&self) -> Vec<TokenPair> {
        let mut token_pairs = Vec::new();

        // 创建常见的代币
        let tokens = vec![
            Token::new(
                "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
                "WETH".to_string(),
                "Wrapped Ether".to_string(),
                18,
                1,
            ),
            Token::new(
                "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
                "USDT".to_string(),
                "Tether USD".to_string(),
                6,
                1,
            ),
            Token::new(
                "0xA0b86a33E6441b8C4505B6c8C8f6e6b8C8f6e6b8".to_string(),
                "USDC".to_string(),
                "USD Coin".to_string(),
                6,
                1,
            ),
            Token::new(
                "0x6B175474E89094C44Da98b954EedeAC495271d0F".to_string(),
                "DAI".to_string(),
                "Dai Stablecoin".to_string(),
                18,
                1,
            ),
        ];

        // 创建所有可能的代币对组合
        for i in 0..tokens.len() {
            for j in (i + 1)..tokens.len() {
                let token_pair = TokenPair::new(tokens[i].clone(), tokens[j].clone());
                token_pairs.push(token_pair);
            }
        }

        token_pairs
    }

    async fn analyze_arbitrage_opportunities(
        &self,
        all_prices: HashMap<String, HashMap<TokenPair, Price>>,
    ) -> Result<Vec<ArbitrageOpportunity>> {
        let mut opportunities = Vec::new();

        // 为每个代币对分析不同 DEX 之间的价格差异
        let mut token_pair_prices: HashMap<TokenPair, Vec<(String, Price)>> = HashMap::new();

        // 整理价格数据
        for (dex_name, prices) in all_prices {
            for (token_pair, price) in prices {
                token_pair_prices
                    .entry(token_pair)
                    .or_insert_with(Vec::new)
                    .push((dex_name.clone(), price));
            }
        }

        // 分析每个代币对的套利机会
        for (token_pair, dex_prices) in token_pair_prices {
            if dex_prices.len() < 2 {
                continue; // 需要至少两个 DEX 的价格才能进行套利
            }

            // 找到最低价和最高价
            let mut min_price_dex = &dex_prices[0];
            let mut max_price_dex = &dex_prices[0];

            for dex_price in &dex_prices {
                if dex_price.1.price < min_price_dex.1.price {
                    min_price_dex = dex_price;
                }
                if dex_price.1.price > max_price_dex.1.price {
                    max_price_dex = dex_price;
                }
            }

            // 计算价格差异百分比
            let price_diff_percentage =
                calculate_percentage_difference(&min_price_dex.1.price, &max_price_dex.1.price);

            // 创建套利机会（无论利润大小都添加到列表中）
            let opportunity = self
                .create_arbitrage_opportunity(
                    token_pair,
                    min_price_dex,
                    max_price_dex,
                    price_diff_percentage,
                )
                .await?;

            opportunities.push(opportunity);
        }

        // 按利润百分比排序
        opportunities.sort_by(|a, b| {
            b.profit_percentage
                .partial_cmp(&a.profit_percentage)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(opportunities)
    }

    async fn create_arbitrage_opportunity(
        &self,
        token_pair: TokenPair,
        buy_dex: &(String, Price),
        sell_dex: &(String, Price),
        profit_percentage: f64,
    ) -> Result<ArbitrageOpportunity> {
        // 估算可用流动性（取较小的那个）
        let available_liquidity = if buy_dex.1.liquidity < sell_dex.1.liquidity {
            buy_dex.1.liquidity.clone()
        } else {
            sell_dex.1.liquidity.clone()
        };

        // 估算利润（简化计算）
        let price_diff = &sell_dex.1.price - &buy_dex.1.price;
        let estimated_profit = &price_diff * &available_liquidity * BigDecimal::from_str("0.1")?; // 假设使用 10% 的流动性

        // 估算 Gas 成本（简化）
        let gas_cost_estimate = BigDecimal::from_str("0.01")?; // 假设 0.01 ETH 的 Gas 成本

        // 计算置信度分数
        let confidence_score =
            self.calculate_confidence_score(&buy_dex.1, &sell_dex.1, profit_percentage);

        Ok(ArbitrageOpportunity {
            id: generate_id(),
            token_pair,
            buy_dex: buy_dex.0.clone(),
            sell_dex: sell_dex.0.clone(),
            buy_price: buy_dex.1.price.clone(),
            sell_price: sell_dex.1.price.clone(),
            profit_percentage,
            estimated_profit,
            liquidity: available_liquidity,
            gas_cost_estimate,
            timestamp: Utc::now(),
            confidence_score,
        })
    }

    fn calculate_confidence_score(
        &self,
        buy_price: &Price,
        sell_price: &Price,
        profit_percentage: f64,
    ) -> f64 {
        let mut score = 0.0;

        // 基于利润百分比的分数（0-40分）
        score += (profit_percentage * 10.0).min(40.0);

        // 基于流动性的分数（0-30分）
        let min_liquidity = if buy_price.liquidity < sell_price.liquidity {
            &buy_price.liquidity
        } else {
            &sell_price.liquidity
        };

        let liquidity_score = if *min_liquidity > BigDecimal::from(100000) {
            30.0
        } else if *min_liquidity > BigDecimal::from(50000) {
            20.0
        } else if *min_liquidity > BigDecimal::from(10000) {
            10.0
        } else {
            5.0
        };

        score += liquidity_score;

        // 基于价格稳定性的分数（0-30分）
        // 这里简化为固定分数，实际应该基于历史价格波动
        score += 20.0;

        // 确保分数在 0-100 范围内
        score.min(100.0).max(0.0)
    }

    pub async fn get_gas_price(&self) -> Result<GasPrice> {
        // 这里应该从 Gas 价格 API 获取实时数据
        // 简化实现，返回固定值
        Ok(GasPrice {
            standard: BigDecimal::from_str("20")?, // 20 Gwei
            fast: BigDecimal::from_str("30")?,     // 30 Gwei
            instant: BigDecimal::from_str("50")?,  // 50 Gwei
            timestamp: Utc::now(),
        })
    }
}
