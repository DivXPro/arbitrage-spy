use crate::token::{Token, TokenList, TokenManager};
use crate::thegraph::PairData;
use crate::utils::convert_decimal_to_integer_string;
use anyhow::Result;
use log::info;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// 创建新的数据库实例
    pub fn new(db_path: Option<&str>) -> Result<Self> {
        let conn = match db_path {
            Some(path) => {
                // 确保目录存在
                if let Some(parent) = Path::new(path).parent() {
                    std::fs::create_dir_all(parent)?;
                }
                Connection::open(path)?
            }
            None => {
                let conn = Connection::open(":memory:")?;
                // 启用外键约束
                conn.execute("PRAGMA foreign_keys = ON", [])?;
                conn
            }
        };

        let db = Database { conn: Arc::new(Mutex::new(conn)) };
        db.init_tables()?;
        Ok(db)
    }

    /// 初始化数据库表
    fn init_tables(&self) -> Result<()> {
        // 创建tokens表
        self.conn.lock().unwrap().execute(
            r#"
            CREATE TABLE IF NOT EXISTS tokens (
                id TEXT PRIMARY KEY,
                symbol TEXT NOT NULL,
                name TEXT NOT NULL,
                ethereum_address TEXT,
                market_cap_rank INTEGER,
                current_price REAL,
                market_cap REAL,
                total_volume REAL,
                price_change_percentage_24h REAL,
                platforms TEXT, -- JSON string of platforms
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
            [],
        )?;

        // 创建token_updates表用于记录更新历史
        self.conn.lock().unwrap().execute(
            r#"
            CREATE TABLE IF NOT EXISTS token_updates (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp INTEGER NOT NULL,
                token_count INTEGER NOT NULL
            )
            "#,
            [],
        )?;

        // 创建pairs表用于存储交易对数据
        self.conn.lock().unwrap().execute(
            r#"
            CREATE TABLE IF NOT EXISTS pairs (
                id TEXT PRIMARY KEY,
                network TEXT NOT NULL DEFAULT 'ethereum',
                dex_type TEXT NOT NULL DEFAULT 'uniswap_v2',
                protocol_type TEXT NOT NULL DEFAULT 'amm_v2',
                token0_id TEXT NOT NULL,
                token0_symbol TEXT NOT NULL,
                token0_name TEXT NOT NULL,
                token0_decimals TEXT NOT NULL,
                token1_id TEXT NOT NULL,
                token1_symbol TEXT NOT NULL,
                token1_name TEXT NOT NULL,
                token1_decimals TEXT NOT NULL,
                volume_usd TEXT NOT NULL,
                reserve_usd TEXT NOT NULL,
                tx_count TEXT NOT NULL,
                reserve0 TEXT NOT NULL DEFAULT '0',
                reserve1 TEXT NOT NULL DEFAULT '0',
                fee_tier TEXT NOT NULL DEFAULT '3000',
                sqrt_price TEXT,
                tick TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
            [],
        )?;

        info!("数据库表初始化完成");
        Ok(())
    }

    /// 保存token列表到数据库
    pub fn save_tokens(&self, tokens: &[Token]) -> Result<()> {
        let tokens_len = tokens.len();
        // 开始事务
        let binding = self.conn.lock().unwrap();
        let tx = binding.unchecked_transaction()?;

        // 插入或更新tokens
        {
            let mut stmt = tx.prepare(
                r#"
                INSERT OR REPLACE INTO tokens (
                    id, symbol, name, market_cap_rank, current_price,
                    market_cap, total_volume, price_change_percentage_24h, platforms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
            )?;

            for token in tokens {
                let platforms_json =
                    serde_json::to_string(&token.platforms).unwrap_or_else(|_| "{}".to_string());

                stmt.execute(params![
                    &token.id,
                    &token.symbol,
                    &token.name,
                    &token.market_cap_rank,
                    &token.current_price,
                    &token.market_cap,
                    &token.total_volume,
                    &token.price_change_percentage_24h,
                    &platforms_json,
                ])?;
            }
        }

        // 记录更新时间
        {
            let mut update_stmt =
                tx.prepare("INSERT INTO token_updates (timestamp, token_count) VALUES (?1, ?2)")?;

            let timestamp = chrono::Utc::now().timestamp();
            update_stmt.execute(params![&timestamp, &(tokens_len as i64)])?;
        }

        tx.commit()?;

        info!("成功保存 {} 个token到数据库", tokens_len);
        Ok(())
    }

    /// 从数据库加载token列表
    pub fn load_tokens(&self, limit: Option<usize>) -> Result<Vec<Token>> {
        let (query, params_vec): (&str, Vec<rusqlite::types::Value>) = if let Some(limit_val) = limit {
            (
                "SELECT id, symbol, name, market_cap_rank, current_price, market_cap, total_volume, price_change_percentage_24h, platforms FROM tokens WHERE market_cap_rank IS NOT NULL ORDER BY market_cap_rank ASC LIMIT ?1",
                vec![rusqlite::types::Value::Integer(limit_val as i64)]
            )
        } else {
            (
                "SELECT id, symbol, name, market_cap_rank, current_price, market_cap, total_volume, price_change_percentage_24h, platforms FROM tokens WHERE market_cap_rank IS NOT NULL ORDER BY market_cap_rank ASC",
                vec![]
            )
        };
        
        let binding = self.conn.lock().unwrap();
        let mut stmt = binding.prepare(query)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params_vec), |row| {
            let platforms_json: String = row.get(8)?;
            let platforms: std::collections::HashMap<String, Option<String>> =
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

        Ok(tokens)
    }

    /// 根据符号查找token
    pub fn find_token_by_symbol(&self, symbol: &str) -> Result<Option<Token>> {
        let binding = self.conn.lock().unwrap();
        let mut stmt = binding.prepare(
            r#"
            SELECT id, symbol, name, market_cap_rank, current_price,
                   market_cap, total_volume, price_change_percentage_24h, platforms
            FROM tokens
            WHERE UPPER(symbol) = UPPER(?1)
            LIMIT 1
            "#,
        )?;

        let mut rows = stmt.query_map([symbol], |row| {
            let platforms_json: String = row.get(8)?;
            let platforms: std::collections::HashMap<String, Option<String>> =
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

        match rows.next() {
            Some(token) => Ok(Some(token?)),
            None => Ok(None),
        }
    }

    /// 根据地址查找token - 直接数据库操作
    pub fn find_token_by_address(&self, address: &str) -> Result<Option<Token>> {
        let binding = self.conn.lock().unwrap();
        let mut stmt = binding.prepare(
            r#"
            SELECT id, symbol, name, market_cap_rank, current_price,
                   market_cap, total_volume, price_change_percentage_24h, platforms
            FROM tokens
            WHERE platforms LIKE '%' || ?1 || '%'
            LIMIT 1
            "#,
        )?;

        let mut rows = stmt.query_map([address], |row| {
            let platforms_json: String = row.get(8)?;
            let platforms: std::collections::HashMap<String, Option<String>> =
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

        match rows.next() {
            Some(token) => Ok(Some(token?)),
            None => Ok(None),
        }
    }

    /// 获取token统计信息 - 直接数据库操作
    pub fn get_token_stats(&self) -> Result<(usize, chrono::DateTime<chrono::Utc>)> {
        // 获取token数量
        let binding = self.conn.lock().unwrap();
        let mut stmt = binding.prepare("SELECT COUNT(*) FROM tokens")?;
        let count: i64 = stmt.query_row([], |row| row.get(0))?;

        // 获取最后更新时间
        let binding2 = self.conn.lock().unwrap();
        let mut update_stmt = binding2
            .prepare("SELECT timestamp FROM token_updates ORDER BY timestamp DESC LIMIT 1")?;

        let timestamp: i64 = update_stmt.query_row([], |row| row.get(0))?;
        let last_update =
            chrono::DateTime::from_timestamp(timestamp, 0).unwrap_or_else(chrono::Utc::now);

        Ok((count as usize, last_update))
    }

    /// 获取数据库统计信息
    pub fn get_stats(&self) -> Result<(usize, chrono::DateTime<chrono::Utc>)> {
        // 获取token数量
        let count = {
            let binding = self.conn.lock().unwrap();
            let mut stmt = binding.prepare("SELECT COUNT(*) FROM tokens")?;
            stmt.query_row([], |row| row.get(0))?
        };

        // 获取最后更新时间
        let last_update = {
            let binding = self.conn.lock().unwrap();
            let mut update_stmt = binding
                .prepare("SELECT timestamp FROM token_updates ORDER BY timestamp DESC LIMIT 1")?;
    
            let timestamp: i64 = update_stmt.query_row([], |row| row.get(0))?;
            chrono::DateTime::from_timestamp(timestamp, 0).unwrap_or_else(chrono::Utc::now)
        };

        Ok((count, last_update))
    }

    /// 保存交易对列表到数据库 - 直接数据库操作
    pub fn save_pairs(&self, pairs: &[PairData]) -> Result<()> {
        let pairs_len = pairs.len();
        // 开始事务
        let binding = self.conn.lock().unwrap();
        let tx = binding.unchecked_transaction()?;

        // 插入或更新pairs
        {
            let mut stmt = tx.prepare(
                r#"
                INSERT OR REPLACE INTO pairs (
                id, network, dex_type, protocol_type,
                token0_id, token0_symbol, token0_name, token0_decimals,
                token1_id, token1_symbol, token1_name, token1_decimals,
                volume_usd, reserve_usd, tx_count, reserve0, reserve1, fee_tier, sqrt_price, tick
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
                "#,
            )?;

            for pair in pairs {
                // 将reserve字段从带小数点的字符串转换为整数型字符串
                let reserve_usd_int = convert_decimal_to_integer_string(&pair.reserve_usd)
                    .unwrap_or_else(|_| "0".to_string());
                let reserve0_int = convert_decimal_to_integer_string(&pair.reserve0)
                    .unwrap_or_else(|_| "0".to_string());
                let reserve1_int = convert_decimal_to_integer_string(&pair.reserve1)
                    .unwrap_or_else(|_| "0".to_string());
                
                stmt.execute(params![
                    &pair.id,
                    &pair.network,
                    &pair.dex_type,
                    "amm_v2", // default protocol_type
                    &pair.token0.id,
                    &pair.token0.symbol,
                    &pair.token0.name,
                    &pair.token0.decimals,
                    &pair.token1.id,
                    &pair.token1.symbol,
                    &pair.token1.name,
                    &pair.token1.decimals,
                    &pair.volume_usd,
                    &reserve_usd_int,
                    &pair.tx_count,
                    &reserve0_int,
                    &reserve1_int,
                    &pair.fee_tier,
                    &pair.sqrt_price,
                    &pair.tick,
                 ])?;
            }
        }

        // 提交事务
        tx.commit()?;
        info!("Saved {} pairs to database", pairs_len);
        Ok(())
    }

    /// 从数据库加载交易对列表 - 直接数据库操作
    pub fn load_pairs(&self) -> Result<Vec<PairData>> {
        use crate::thegraph::TokenInfo;
        
        let binding = self.conn.lock().unwrap();
        let mut stmt = binding.prepare(
            r#"
            SELECT id, network, dex_type, protocol_type, token0_id, token0_symbol, token0_name, token0_decimals,
                   token1_id, token1_symbol, token1_name, token1_decimals,
                   volume_usd, reserve_usd, tx_count, reserve0, reserve1, fee_tier, sqrt_price, tick
            FROM pairs
            "#,
        )?;

        let pair_iter = stmt.query_map([], |row| {
            Ok(PairData {
                id: row.get(0)?,
                network: row.get(1)?,
                dex_type: row.get(2)?,
                protocol_type: row.get(3)?,
                token0: TokenInfo {
                    id: row.get(4)?,
                    symbol: row.get(5)?,
                    name: row.get(6)?,
                    decimals: row.get(7)?,
                },
                token1: TokenInfo {
                    id: row.get(8)?,
                    symbol: row.get(9)?,
                    name: row.get(10)?,
                    decimals: row.get(11)?,
                },
                volume_usd: row.get(12)?,
                reserve_usd: row.get(13)?,
                tx_count: row.get(14)?,
                reserve0: row.get(15)?,
                reserve1: row.get(16)?,
                fee_tier: row.get(17)?,
                sqrt_price: row.get(18)?,
                tick: row.get(19)?,
            })
        })?;

        let mut pairs = Vec::new();
        for pair in pair_iter {
            pairs.push(pair?);
        }

        Ok(pairs)
    }

    /// 根据网络和DEX类型筛选交易对 - 直接数据库操作
    pub fn load_pairs_by_filter(
        &self,
        network: Option<&str>,
        dex_type: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<PairData>> {
        use crate::thegraph::TokenInfo;
        
        let mut query = String::from(
            r#"
            SELECT id, network, dex_type, protocol_type, token0_id, token0_symbol, token0_name, token0_decimals,
                   token1_id, token1_symbol, token1_name, token1_decimals,
                   volume_usd, reserve_usd, tx_count, reserve0, reserve1, fee_tier, sqrt_price, tick
            FROM pairs
            "#,
        );

        let mut conditions = Vec::new();
        let mut params_vec = Vec::new();

        if let Some(net) = network {
            conditions.push("network = ?");
            params_vec.push(net);
        }

        if let Some(dex) = dex_type {
            conditions.push("dex_type = ?");
            params_vec.push(dex);
        }

        if !conditions.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&conditions.join(" AND "));
        }

        if let Some(lim) = limit {
            query.push_str(&format!(" LIMIT {}", lim));
        }

        let binding = self.conn.lock().unwrap();
        let mut stmt = binding.prepare(&query)?;
        let params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        
        let pair_iter = stmt.query_map(params.as_slice(), |row| {
            Ok(PairData {
                id: row.get(0)?,
                network: row.get(1)?,
                dex_type: row.get(2)?,
                protocol_type: row.get(3)?,
                token0: TokenInfo {
                    id: row.get(4)?,
                    symbol: row.get(5)?,
                    name: row.get(6)?,
                    decimals: row.get(7)?,
                },
                token1: TokenInfo {
                    id: row.get(8)?,
                    symbol: row.get(9)?,
                    name: row.get(10)?,
                    decimals: row.get(11)?,
                },
                volume_usd: row.get(12)?,
                reserve_usd: row.get(13)?,
                tx_count: row.get(14)?,
                reserve0: row.get(15)?,
                reserve1: row.get(16)?,
                fee_tier: row.get(17)?,
                sqrt_price: row.get(18)?,
                tick: row.get(19)?,
            })
        })?;

        let mut pairs = Vec::new();
        for pair in pair_iter {
            pairs.push(pair?);
        }

        Ok(pairs)
    }

    /// 根据交易对ID查找特定交易对 - 直接数据库操作
    pub fn find_pair_by_id(&self, pair_id: &str) -> Result<Option<PairData>> {
        use crate::thegraph::TokenInfo;
        
        let binding = self.conn.lock().unwrap();
        let mut stmt = binding.prepare(
            r#"
            SELECT id, network, dex_type, protocol_type, token0_id, token0_symbol, token0_name, token0_decimals,
                   token1_id, token1_symbol, token1_name, token1_decimals,
                   volume_usd, reserve_usd, tx_count, reserve0, reserve1, fee_tier, sqrt_price, tick
            FROM pairs
            WHERE id = ?
            "#,
        )?;

        let mut pair_iter = stmt.query_map([pair_id], |row| {
            Ok(PairData {
                id: row.get(0)?,
                network: row.get(1)?,
                dex_type: row.get(2)?,
                protocol_type: row.get(3)?,
                token0: TokenInfo {
                    id: row.get(4)?,
                    symbol: row.get(5)?,
                    name: row.get(6)?,
                    decimals: row.get(7)?,
                },
                token1: TokenInfo {
                    id: row.get(8)?,
                    symbol: row.get(9)?,
                    name: row.get(10)?,
                    decimals: row.get(11)?,
                },
                volume_usd: row.get(12)?,
                reserve_usd: row.get(13)?,
                tx_count: row.get(14)?,
                reserve0: row.get(15)?,
                reserve1: row.get(16)?,
                fee_tier: row.get(17)?,
                sqrt_price: row.get(18)?,
                tick: row.get(19)?,
            })
        })?;

        match pair_iter.next() {
            Some(pair) => Ok(Some(pair?)),
            None => Ok(None),
        }
    }

    /// 获取交易对统计信息 - 直接数据库操作
    pub fn get_pairs_stats(&self) -> Result<(usize, f64, f64)> {
        let binding = self.conn.lock().unwrap();
        let mut stmt = binding.prepare(
            r#"
            SELECT COUNT(*) as count,
                   AVG(CAST(volume_usd AS REAL)) as avg_volume,
                   AVG(CAST(reserve_usd AS REAL)) as avg_reserve
            FROM pairs
            "#,
        )?;

        let mut rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)? as usize,
                row.get::<_, f64>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })?;

        match rows.next() {
            Some(row) => Ok(row?),
            None => Ok((0, 0.0, 0.0)),
        }
    }


}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thegraph::{PairData, TokenInfo};

    #[test]
    fn test_save_pairs_with_decimal_reserves() {
        // 创建临时数据库
        let db = Database::new(Some(":memory:")).unwrap();
        
        // 创建测试数据，包含带小数点的reserve字段
        let test_pair = PairData {
            id: "test_pair_1".to_string(),
            network: "ethereum".to_string(),
            dex_type: "uniswap_v2".to_string(),
            protocol_type: "amm_v2".to_string(),
            token0: TokenInfo {
                id: "token0_id".to_string(),
                symbol: "TOKEN0".to_string(),
                name: "Token 0".to_string(),
                decimals: "18".to_string(),
            },
            token1: TokenInfo {
                id: "token1_id".to_string(),
                symbol: "TOKEN1".to_string(),
                name: "Token 1".to_string(),
                decimals: "6".to_string(),
            },
            volume_usd: "1000000.5".to_string(),
            reserve_usd: "5000000.123456".to_string(), // 带小数点
            tx_count: "100".to_string(),
            reserve0: "1234567.890123".to_string(), // 带小数点
            reserve1: "9876543.210987".to_string(), // 带小数点
            fee_tier: "3000".to_string(),
            sqrt_price: None,
            tick: None,
        };

        // 保存数据
        db.save_pairs(&[test_pair]).unwrap();

        // 验证数据是否正确保存（reserve字段应该被转换为整数）
        let saved_pairs = db.load_pairs().unwrap();
        assert_eq!(saved_pairs.len(), 1);
        
        let saved_pair = &saved_pairs[0];
        assert_eq!(saved_pair.id, "test_pair_1");
        assert_eq!(saved_pair.reserve_usd, "5000000123456"); // 保留所有数字
        assert_eq!(saved_pair.reserve0, "1234567890123"); // 保留所有数字
        assert_eq!(saved_pair.reserve1, "9876543210987"); // 保留所有数字
    }
}
