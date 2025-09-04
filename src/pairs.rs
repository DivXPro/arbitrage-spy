use anyhow::Result;
use crate::thegraph::PairData;
use crate::database::Database;

/// 交易对管理器 - 负责业务逻辑
pub struct PairManager {
    database: Database,
}

impl PairManager {
    /// 创建新的交易对管理器
    pub fn new(database: &Database) -> Self {
        Self { database: database.clone() }
    }

    /// 保存交易对列表到数据库 - 业务逻辑
    pub fn save_pairs(&self, pairs: &[PairData]) -> Result<()> {
        // 业务逻辑：数据验证
        self.validate_pairs(pairs)?;
        
        // 业务逻辑：数据预处理（这里直接使用原始数据）
        // 如果需要预处理，可以在这里添加逻辑
        
        // 调用数据库层的方法
        self.database.save_pairs(pairs)
    }

    /// 从数据库加载交易对列表 - 业务逻辑
    pub fn load_pairs(&self) -> Result<Vec<PairData>> {
        // 调用数据库层的方法
        let pairs = self.database.load_pairs()?;
        
        // 业务逻辑：数据后处理
        Ok(self.postprocess_pairs(pairs))
    }

    /// 根据网络和DEX类型筛选交易对 - 业务逻辑
    pub fn load_pairs_by_filter(
        &self,
        network: Option<&str>,
        dex_type: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<PairData>> {
        // 业务逻辑：参数验证
        self.validate_filter_params(network, dex_type, limit)?;
        
        // 调用数据库层的方法
        let pairs = self.database.load_pairs_by_filter(network, dex_type, limit)?;
        
        // 业务逻辑：结果处理
        Ok(self.postprocess_pairs(pairs))
    }

    /// 根据交易对ID查找特定交易对 - 业务逻辑
    pub fn find_pair_by_id(&self, pair_id: &str) -> Result<Option<PairData>> {
        // 业务逻辑：参数验证
        if pair_id.is_empty() {
            return Err(anyhow::anyhow!("Pair ID cannot be empty"));
        }
        
        // 调用数据库层的方法
        let pair = self.database.find_pair_by_id(pair_id)?;
        
        // 业务逻辑：结果处理
        Ok(pair.map(|p| self.postprocess_pair(p)))
    }

    /// 获取交易对统计信息 - 业务逻辑
    pub fn get_pairs_stats(&self) -> Result<(usize, f64, f64)> {
        // 调用数据库层的方法
        let stats = self.database.get_pairs_stats()?;
        
        // 业务逻辑：统计数据处理
        Ok(self.process_stats(stats))
    }

    /// 验证交易对数据
    fn validate_pairs(&self, pairs: &[PairData]) -> Result<()> {
        for pair in pairs {
            if pair.id.is_empty() {
                return Err(anyhow::anyhow!("Pair ID cannot be empty"));
            }
            if pair.token0.symbol.is_empty() || pair.token1.symbol.is_empty() {
                return Err(anyhow::anyhow!("Token symbols cannot be empty"));
            }
            if pair.network.is_empty() {
                return Err(anyhow::anyhow!("Network cannot be empty"));
            }
            if pair.dex_type.is_empty() {
                return Err(anyhow::anyhow!("DEX type cannot be empty"));
            }
        }
        Ok(())
    }

    /// 验证筛选参数
    fn validate_filter_params(
        &self,
        _network: Option<&str>,
        _dex_type: Option<&str>,
        limit: Option<usize>,
    ) -> Result<()> {
        if let Some(lim) = limit {
            if lim == 0 {
                return Err(anyhow::anyhow!("Limit must be greater than 0"));
            }
            if lim > 10000 {
                return Err(anyhow::anyhow!("Limit cannot exceed 10000"));
            }
        }
        Ok(())
    }

    /// 预处理交易对数据（暂时未使用，因为PairData没有实现Clone）
    #[allow(dead_code)]
    fn _preprocess_pairs(&self, _pairs: &[PairData]) {
        // 这里可以添加数据清洗、格式化等逻辑
        // 由于PairData没有实现Clone，暂时不返回新的Vec
    }

    /// 后处理交易对数据
    fn postprocess_pairs(&self, pairs: Vec<PairData>) -> Vec<PairData> {
        // 这里可以添加数据转换、排序等逻辑
        pairs
    }

    /// 后处理单个交易对数据
    fn postprocess_pair(&self, pair: PairData) -> PairData {
        // 这里可以添加单个交易对的数据处理逻辑
        pair
    }

    /// 处理统计数据
    fn process_stats(&self, stats: (usize, f64, f64)) -> (usize, f64, f64) {
        // 这里可以添加统计数据的处理逻辑，比如四舍五入等
        let (count, avg_volume, avg_reserve) = stats;
        (count, (avg_volume * 100.0).round() / 100.0, (avg_reserve * 100.0).round() / 100.0)
    }

    /// 获取支持的网络列表 - 业务方法
    pub fn get_supported_networks(&self) -> Vec<&'static str> {
        vec!["ethereum", "polygon", "bsc", "arbitrum"]
    }

    /// 获取支持的DEX类型列表 - 业务方法
    pub fn get_supported_dex_types(&self) -> Vec<&'static str> {
        vec!["uniswap_v2", "uniswap_v3", "sushiswap", "pancakeswap"]
    }

    /// 检查网络是否支持 - 业务方法
    pub fn is_network_supported(&self, network: &str) -> bool {
        self.get_supported_networks().contains(&network)
    }

    /// 检查DEX类型是否支持 - 业务方法
    pub fn is_dex_type_supported(&self, dex_type: &str) -> bool {
        self.get_supported_dex_types().contains(&dex_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thegraph::TokenInfo;

    #[test]
    fn test_pair_manager_creation() {
        let database = Database::new(Some("test_pairs.db")).unwrap();
        let _manager = PairManager::new(&database);
        // PairManager现在包含Database实例
    }

    #[test]
    fn test_validate_pairs() {
        let database = Database::new(Some("test_pairs.db")).unwrap();
        let manager = PairManager::new(&database);
        
        // 测试有效数据
        let valid_pairs = vec![get_demo_pair()];
        assert!(manager.validate_pairs(&valid_pairs).is_ok());
        
        // 测试无效数据 - 空ID
        let mut invalid_pair = get_demo_pair();
        invalid_pair.id = String::new();
        let invalid_pairs = vec![invalid_pair];
        assert!(manager.validate_pairs(&invalid_pairs).is_err());
    }

    #[test]
    fn test_validate_filter_params() {
        let database = Database::new(Some("test_pairs.db")).unwrap();
        let manager = PairManager::new(&database);
        
        // 测试有效参数
        assert!(manager.validate_filter_params(Some("ethereum"), Some("uniswap_v2"), Some(100)).is_ok());
        
        // 测试无效参数 - limit为0
        assert!(manager.validate_filter_params(None, None, Some(0)).is_err());
        
        // 测试无效参数 - limit过大
        assert!(manager.validate_filter_params(None, None, Some(20000)).is_err());
    }

    #[test]
    fn test_supported_networks_and_dex_types() {
        let database = Database::new(Some("test_pairs.db")).unwrap();
        let manager = PairManager::new(&database);
        
        assert!(manager.is_network_supported("ethereum"));
        assert!(!manager.is_network_supported("unknown"));
        
        assert!(manager.is_dex_type_supported("uniswap_v2"));
        assert!(!manager.is_dex_type_supported("unknown"));
    }

    #[test]
    fn test_process_stats() {
        let database = Database::new(Some("test_pairs.db")).unwrap();
        let manager = PairManager::new(&database);
        let stats = (100, 123.456789, 987.654321);
        let processed = manager.process_stats(stats);
        
        assert_eq!(processed.0, 100);
        assert_eq!(processed.1, 123.46);
        assert_eq!(processed.2, 987.65);
    }

    fn get_demo_pair() -> PairData {
        PairData {
            id: "0x123".to_string(),
            network: "ethereum".to_string(),
            dex_type: "uniswap_v2".to_string(),
            token0: TokenInfo {
                id: "0xA0b86a33E6441E6C7D3E4C2C4C6C6C6C6C6C6C6C".to_string(),
                symbol: "USDC".to_string(),
                name: "USD Coin".to_string(),
                decimals: "6".to_string(),
            },
            token1: TokenInfo {
                id: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
                symbol: "WETH".to_string(),
                name: "Wrapped Ether".to_string(),
                decimals: "18".to_string(),
            },
            volume_usd: "1000000".to_string(),
            reserve_usd: "5000000".to_string(),
            tx_count: "1000".to_string(),
            reserve0: "1000000".to_string(),
            reserve1: "5000000".to_string(),
        }
    }
}