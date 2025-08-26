use crate::token::{Token, TokenList};
use anyhow::Result;
use log::info;
use rusqlite::{params, Connection};
use std::path::Path;

pub struct Database {
    conn: Connection,
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

        let db = Database { conn };
        db.init_tables()?;
        Ok(db)
    }

    /// 初始化数据库表
    fn init_tables(&self) -> Result<()> {
        // 创建tokens表
        self.conn.execute(
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
        self.conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS token_updates (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp INTEGER NOT NULL,
                token_count INTEGER NOT NULL
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
        let tx = self.conn.unchecked_transaction()?;

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
    pub fn load_tokens(&self) -> Result<Vec<Token>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, symbol, name, market_cap_rank, current_price,
                   market_cap, total_volume, price_change_percentage_24h, platforms
            FROM tokens
            ORDER BY market_cap_rank ASC
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
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
        let mut stmt = self.conn.prepare(
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

    /// 获取数据库统计信息
    pub fn get_stats(&self) -> Result<(usize, chrono::DateTime<chrono::Utc>)> {
        // 获取token数量
        let mut stmt = self.conn.prepare("SELECT COUNT(*) FROM tokens")?;
        let count: i64 = stmt.query_row([], |row| row.get(0))?;

        // 获取最后更新时间
        let mut update_stmt = self
            .conn
            .prepare("SELECT timestamp FROM token_updates ORDER BY timestamp DESC LIMIT 1")?;

        let timestamp: i64 = update_stmt.query_row([], |row| row.get(0))?;
        let last_update =
            chrono::DateTime::from_timestamp(timestamp, 0).unwrap_or_else(chrono::Utc::now);

        Ok((count as usize, last_update))
    }
}
