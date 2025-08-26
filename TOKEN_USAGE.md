# Token管理功能使用指南

本项目集成了从CoinGecko API获取token数据的功能，支持JSON格式存储和自动更新。

## 功能特性

- 🔄 **自动数据更新**: 程序启动时自动从CoinGecko API获取最新token数据
- 💾 **本地缓存**: 支持JSON格式本地存储，避免频繁API调用
- 🔍 **智能查询**: 支持按符号、合约地址查找token
- 📊 **市值排序**: 自动按市值排名排序token列表
- ⚡ **速率限制**: 内置API调用速率限制，避免触发限制
- 🌐 **多链支持**: 支持以太坊等多个区块链平台的token地址

## 使用方法

### 1. 程序启动时自动更新

程序启动时会自动初始化token数据：

```bash
cargo run
```

启动日志示例：
```
[INFO] 启动区块链套利监控系统...
[INFO] 配置加载完成
[INFO] 初始化token数据...
[INFO] 成功加载 500 个token
[INFO] 监控器初始化完成
```

### 2. 运行演示程序

查看token功能演示：

```bash
RUST_LOG=info cargo run --example token_demo
```

### 3. 在代码中使用TokenManager

```rust
use arbitrage_spy::token::TokenManager;

#[tokio::main]
async fn main() -> Result<()> {
    // 创建token管理器
    let token_manager = TokenManager::new(Some("data/tokens.json".to_string()));
    
    // 获取token列表（优先使用缓存）
    let token_list = token_manager.get_tokens(false, Some(100)).await?;
    println!("加载了 {} 个token", token_list.total_count);
    
    // 按符号查找token
    if let Some(token) = token_manager.get_token_by_symbol("USDC").await? {
        println!("找到USDC: {}", token.name);
        if let Some(Some(eth_address)) = token.platforms.get("ethereum") {
            println!("以太坊地址: {}", eth_address);
        }
    }
    
    // 按合约地址查找token
    let address = "0xa0b86a33e6c8b4c4c6e8b4c4c6e8b4c4c6e8b4c4";
    if let Some(token) = token_manager.get_token_by_address(address).await? {
        println!("找到token: {} ({})", token.name, token.symbol);
    }
    
    // 获取市值前10的token
    let top_tokens = token_manager.get_top_tokens(10).await?;
    for (i, token) in top_tokens.iter().enumerate() {
        println!(
            "{}. {} ({}) - ${:.2}",
            i + 1,
            token.name,
            token.symbol.to_uppercase(),
            token.current_price.unwrap_or(0.0)
        );
    }
    
    Ok(())
}
```

## Token数据结构

### Token结构

```rust
pub struct Token {
    pub id: String,                    // CoinGecko ID
    pub symbol: String,                // 代币符号
    pub name: String,                  // 代币名称
    pub platforms: HashMap<String, Option<String>>, // 平台->合约地址映射
    pub market_cap_rank: Option<u32>,  // 市值排名
    pub current_price: Option<f64>,    // 当前价格(USD)
    pub market_cap: Option<f64>,       // 市值
    pub total_volume: Option<f64>,     // 24h交易量
    pub price_change_percentage_24h: Option<f64>, // 24h价格变化百分比
}
```

### TokenList结构

```rust
pub struct TokenList {
    pub tokens: Vec<Token>,            // token列表
    pub last_updated: DateTime<Utc>,   // 最后更新时间
    pub total_count: usize,            // token总数
}
```

## 配置选项

### API Key配置

- **环境变量**: `COINGECKO_API_KEY`
- **在 `.env` 文件中设置**: `COINGECKO_API_KEY=your_api_key_here`
- **免费版API有速率限制**，建议申请API key获得更高请求限制
- **如果未设置API key**，将使用免费版API

### 缓存设置

- **缓存文件**: 默认存储在 `data/tokens.json`
- **缓存有效期**: 1小时，超过后自动更新
- **强制更新**: 可通过 `force_update` 参数强制从API获取

### API限制

- **默认获取数量**: 500个token（可配置）
- **批次大小**: 100个token/批次
- **请求间隔**: 1秒/批次
- **用户代理**: `arbitrage-spy/0.1.0`

## 数据文件示例

生成的JSON文件结构：

```json
{
  "tokens": [
    {
      "id": "ethereum",
      "symbol": "eth",
      "name": "Ethereum",
      "platforms": {
        "ethereum": null
      },
      "market_cap_rank": 2,
      "current_price": 2500.0,
      "market_cap": 300000000000.0,
      "total_volume": 15000000000.0,
      "price_change_percentage_24h": 2.5
    }
  ],
  "last_updated": "2024-01-01T12:00:00Z",
  "total_count": 500
}
```

## 错误处理

程序具有完善的错误处理机制：

- **API失败**: 如果API调用失败，会记录错误但不会中断程序运行
- **网络问题**: 支持重试机制和降级处理
- **缓存损坏**: 自动重新获取数据
- **权限问题**: 提供清晰的错误信息

## 性能优化

- **增量更新**: 只获取必要的数据
- **并发控制**: 避免过多并发请求
- **内存管理**: 合理的数据结构设计
- **缓存策略**: 智能的缓存失效机制

## 注意事项

1. **API限制**: CoinGecko免费API有调用频率限制，请合理使用
2. **数据准确性**: 价格数据仅供参考，实际交易请以交易所数据为准
3. **网络依赖**: 首次运行需要网络连接获取数据
4. **存储空间**: token数据文件可能较大，请确保有足够存储空间

## 故障排除

### 常见问题

**Q: API调用失败怎么办？**
A: 检查网络连接，确认CoinGecko API可访问。程序会自动重试并使用缓存数据。

**Q: 找不到某个token？**
A: 确认token符号正确，或者增加获取的token数量限制。

**Q: 数据更新太慢？**
A: 可以减少获取的token数量，或者调整批次大小。

**Q: 缓存文件损坏？**
A: 删除缓存文件，程序会自动重新获取数据。

```bash
rm data/tokens.json
cargo run
```