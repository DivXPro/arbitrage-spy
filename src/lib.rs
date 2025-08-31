//! Arbitrage Spy - 区块链套利监控系统
//! 
//! 这个库提供了监控多个DEX平台套利机会的功能，支持从区块链直接获取价格数据。

pub mod config;
pub mod database;
pub mod dex;
pub mod monitor;
pub mod token;
pub mod types;
pub mod utils;

// 重新导出常用类型
pub use config::Config;
pub use database::Database;
pub use monitor::ArbitrageMonitor;
pub use token::{Token, TokenList, TokenManager};
pub use types::{ArbitrageOpportunity, Pool, Token as TypesToken, TokenPair};