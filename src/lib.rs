//! Arbitrage Spy - 区块链套利监控系统
//! 
//! 这个库提供了监控多个DEX平台套利机会的功能，支持从区块链直接获取价格数据。
//! 现在支持传统套利和多跳链式套利两种模式。

pub mod arbitrage_chain;
pub mod cli;
pub mod config;
pub mod database;
pub mod dex;
pub mod event_listener;
pub mod log_adapter;
pub mod pair_manager;
pub mod price_calculator;
pub mod realtime_monitor;
pub mod table_display;
pub mod thegraph;
pub mod token_manager;
pub mod types;
pub mod utils;

// 重新导出常用类型
pub use arbitrage_chain::{ArbitrageChain, ArbitrageChainFinder, PriceGraph};
pub use config::Config;
pub use database::Database;
pub use pair_manager::PairManager;
pub use token_manager::{Token, TokenList, TokenManager};
pub use types::{ArbitrageOpportunity, Pool, Token as TypesToken, TokenPair};