// Data layer modules
pub mod database;
pub mod pair_manager;
pub mod token_manager;
pub mod thegraph;

// Blockchain data modules
pub mod blockchain_client;
pub mod uniswap_v2_client;
pub mod uniswap_v3_client;
pub mod dex_data_manager;

// Re-export commonly used types and structs
pub use database::Database;
pub use pair_manager::{PairData, PairManager, TokenInfo};
pub use token_manager::{Token, TokenList, TokenManager};
pub use thegraph::{PoolData, TheGraphClient};

// Re-export blockchain data types
pub use blockchain_client::{BlockchainClient, NetworkConfig};
pub use uniswap_v2_client::{UniswapV2Client, V2PairInfo, V2FactoryInfo};
pub use uniswap_v3_client::{UniswapV3Client, V3PoolInfo, V3FactoryInfo};
pub use dex_data_manager::{DexDataManager, DexType, PairInfo, DexFactoryInfo};