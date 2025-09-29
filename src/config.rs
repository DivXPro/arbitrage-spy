use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;

// 协议类型常量
pub mod protocol_types {
    pub const AMM_V2: &str = "amm_v2";
    pub const AMM_V3: &str = "amm_v3";
}

// DEX类型常量
pub mod dex_types {
    pub const UNISWAP_V2: &str = "UNI_V2";
    pub const UNISWAP_V3: &str = "UNI_V3";
    pub const SUSHISWAP: &str = "sushiswap";
    pub const PANCAKESWAP: &str = "pancakeswap";
    pub const CURVE: &str = "curve";
    pub const BALANCER: &str = "balancer";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub dex_configs: HashMap<String, DexConfig>,
    pub monitoring: MonitoringConfig,
    pub arbitrage: ArbitrageConfig,
    pub gas_config: GasConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexConfig {
    pub name: String,
    pub enabled: bool,
    pub api_url: String,
    pub chain_id: u64,
    pub factory_address: Option<String>,
    pub router_address: Option<String>,
    pub subgraph_url: Option<String>,
    pub rate_limit_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub scan_interval_seconds: u64,
    pub max_concurrent_requests: usize,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageConfig {
    pub min_profit_threshold: f64,
    pub max_gas_price_gwei: f64,
    pub slippage_tolerance: f64,
    pub tokens_to_monitor: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasConfig {
    pub uniswap_v2_gas_units: u64,
    pub uniswap_v3_gas_units: u64,
    pub sushiswap_gas_units: u64,
    pub pancakeswap_gas_units: u64,
    pub balancer_gas_units: u64,
    pub curve_gas_units: u64,
}

impl Config {
    pub fn load() -> Result<Self> {
        // 加载.env文件
        dotenv::dotenv().ok();
        // 默认配置
        let mut dex_configs = HashMap::new();
        
        // Uniswap V2 配置
        dex_configs.insert("uniswap_v2".to_string(), DexConfig {
            name: "Uniswap V2".to_string(),
            enabled: true,
            api_url: "https://eth.llamarpc.com".to_string(), // 使用公共 RPC 端点
            chain_id: 1,
            factory_address: Some("0x5C69bEe701ef814a2B6a3EDD4B1652CB9cc5aA6f".to_string()),
            router_address: Some("0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D".to_string()),
            subgraph_url: Some("https://api.thegraph.com/subgraphs/name/uniswap/uniswap-v2".to_string()),
            rate_limit_ms: 1000,
        });
        
        // SushiSwap 配置
        dex_configs.insert("sushiswap".to_string(), DexConfig {
            name: "SushiSwap".to_string(),
            enabled: true,
            api_url: "https://eth.llamarpc.com".to_string(),
            chain_id: 1,
            factory_address: Some("0xC0AEe478e3658e2610c5F7A4A2E1777cE9e4f2Ac".to_string()),
            router_address: Some("0xd9e1cE17f2641f24aE83637ab66a2cca9C378B9F".to_string()),
            subgraph_url: Some("https://api.thegraph.com/subgraphs/name/sushiswap/exchange".to_string()),
            rate_limit_ms: 1000,
        });
        
        // PancakeSwap 配置 (BSC)
        dex_configs.insert("pancakeswap".to_string(), DexConfig {
            name: "PancakeSwap".to_string(),
            enabled: true,
            api_url: "https://bsc-dataseed1.binance.org".to_string(),
            chain_id: 56,
            factory_address: Some("0xcA143Ce32Fe78f1f7019d7d551a6402fC5350c73".to_string()),
            router_address: Some("0x10ED43C718714eb63d5aA57B78B54704E256024E".to_string()),
            subgraph_url: Some("https://api.thegraph.com/subgraphs/name/pancakeswap/exchange".to_string()),
            rate_limit_ms: 1000,
        });
        
        // Curve 配置
        dex_configs.insert("curve".to_string(), DexConfig {
            name: "Curve".to_string(),
            enabled: true,
            api_url: "https://eth.llamarpc.com".to_string(),
            chain_id: 1,
            factory_address: None,
            router_address: None,
            subgraph_url: Some("https://api.thegraph.com/subgraphs/name/curvefi/curve".to_string()),
            rate_limit_ms: 2000,
        });
        
        // Balancer 配置
        dex_configs.insert("balancer".to_string(), DexConfig {
            name: "Balancer".to_string(),
            enabled: true,
            api_url: "https://eth.llamarpc.com".to_string(),
            chain_id: 1,
            factory_address: Some("0xBA12222222228d8Ba445958a75a0704d566BF2C8".to_string()),
            router_address: None,
            subgraph_url: Some("https://api.thegraph.com/subgraphs/name/balancer-labs/balancer-v2".to_string()),
            rate_limit_ms: 1500,
        });
        
        Ok(Config {
            dex_configs,
            monitoring: MonitoringConfig {
                scan_interval_seconds: 10,
                max_concurrent_requests: 10,
                timeout_seconds: 30,
            },
            arbitrage: ArbitrageConfig {
                min_profit_threshold: 0.01, // 1% 最小利润
                max_gas_price_gwei: 100.0,
                slippage_tolerance: 0.005, // 0.5% 滑点容忍度
                tokens_to_monitor: vec![
                    "0xA0b86a33E6441b8C4505B6c8C8f6e6b8C8f6e6b8".to_string(), // WETH
                    "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(), // USDT
                    "0xA0b73E1Ff0B80914AB6fe0444E65848C4C34450b".to_string(), // USDC
                    "0x6B175474E89094C44Da98b954EedeAC495271d0F".to_string(), // DAI
                ],
            },
            gas_config: GasConfig {
                uniswap_v2_gas_units: env::var("UNISWAP_V2_GAS_UNITS")
                    .unwrap_or_else(|_| "150000".to_string())
                    .parse()
                    .unwrap_or(150000),
                uniswap_v3_gas_units: env::var("UNISWAP_V3_GAS_UNITS")
                    .unwrap_or_else(|_| "180000".to_string())
                    .parse()
                    .unwrap_or(180000),
                sushiswap_gas_units: env::var("SUSHISWAP_GAS_UNITS")
                    .unwrap_or_else(|_| "150000".to_string())
                    .parse()
                    .unwrap_or(150000),
                pancakeswap_gas_units: env::var("PANCAKESWAP_GAS_UNITS")
                    .unwrap_or_else(|_| "150000".to_string())
                    .parse()
                    .unwrap_or(150000),
                balancer_gas_units: env::var("BALANCER_GAS_UNITS")
                    .unwrap_or_else(|_| "200000".to_string())
                    .parse()
                    .unwrap_or(200000),
                curve_gas_units: env::var("CURVE_GAS_UNITS")
                    .unwrap_or_else(|_| "220000".to_string())
                    .parse()
                    .unwrap_or(220000),
            },
        })
    }
    
    pub fn get_enabled_dexes(&self) -> Vec<&DexConfig> {
        self.dex_configs
            .values()
            .filter(|config| config.enabled)
            .collect()
    }
}

impl GasConfig {
    /// 根据DEX名称获取Gas单位
    pub fn get_gas_units(&self, dex_name: &str) -> u64 {
        match dex_name.to_lowercase().as_str() {
            "uniswap_v2" => self.uniswap_v2_gas_units,
            "uniswap_v3" => self.uniswap_v3_gas_units,
            "sushiswap" => self.sushiswap_gas_units,
            "pancakeswap" => self.pancakeswap_gas_units,
            "balancer" => self.balancer_gas_units,
            "curve" => self.curve_gas_units,
            _ => self.uniswap_v2_gas_units, // 默认使用Uniswap V2的值
        }
    }
}