# Arbitrage Spy - 区块链套利监控工具

一个用 Rust 编写的高性能区块链套利机会监控工具，支持多个主流 DEX 平台的实时价格监控和套利机会分析。

## 功能特性

- 🔍 **多 DEX 支持**: 支持 Uniswap V2、SushiSwap、PancakeSwap、Curve 和 Balancer
- ⚡ **实时监控**: 异步并发获取多个 DEX 的实时价格数据
- 📊 **智能分析**: 自动识别和分析套利机会，计算利润率和置信度
- 🎯 **精准过滤**: 可配置的最小利润阈值和风险参数
- 📈 **流动性评估**: 考虑流动性深度，避免滑点风险
- ⛽ **Gas 成本估算**: 实时 Gas 价格监控和成本计算
- 🔧 **灵活配置**: 支持自定义 DEX 配置和监控参数

## 支持的 DEX 平台

| DEX | 网络 | 状态 |
|-----|------|------|
| Uniswap V2 | Ethereum | ✅ |
| SushiSwap | Ethereum | ✅ |
| PancakeSwap | BSC | ✅ |
| Curve | Ethereum | ✅ |
| Balancer | Ethereum | ✅ |

## 快速开始

### 环境要求

- Rust 1.70+
- Cargo
- 网络连接（用于访问各 DEX 的 API）

### 安装

```bash
# 克隆项目
git clone https://github.com/your-username/arbitrage-spy.git
cd arbitrage-spy

# 复制环境变量配置文件
cp .env.example .env

# 安装依赖
cargo build
```

### 配置

项目支持通过 `.env` 文件进行配置：

```bash
# 编辑配置文件
vim .env
```

主要配置项：
- `RUST_LOG`: 日志级别 (debug, info, warn, error)
- `SCAN_INTERVAL_SECONDS`: 扫描间隔（秒）
- `MIN_PROFIT_THRESHOLD`: 最小利润阈值
- `MAX_GAS_PRICE`: 最大 Gas 价格

### 构建

```bash
# 开发构建
cargo build

# 发布构建
cargo build --release
```

### 运行

```bash
# 使用 .env 文件配置运行
cargo run

# 或者手动指定环境变量
RUST_LOG=info cargo run
```

### 配置

项目使用内置的默认配置，包含主流代币对和合理的监控参数。你可以通过修改 `src/config.rs` 来自定义配置：

```rust
// 监控配置
pub struct MonitoringConfig {
    pub scan_interval_seconds: u64,    // 扫描间隔（秒）
    pub max_concurrent_requests: usize, // 最大并发请求数
    pub request_timeout_seconds: u64,   // 请求超时时间
}

// 套利配置
pub struct ArbitrageConfig {
    pub min_profit_threshold: f64,      // 最小利润阈值（%）
    pub max_slippage: f64,             // 最大滑点（%）
    pub min_liquidity: f64,            // 最小流动性要求
}
```

## 使用示例

### 基本监控

```rust
use arbitrage_spy::monitor::ArbitrageMonitor;
use arbitrage_spy::config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    env_logger::init();
    
    // 加载配置
    let config = Config::default();
    
    // 创建监控器
    let mut monitor = ArbitrageMonitor::new(config).await?;
    
    // 扫描套利机会
    let opportunities = monitor.scan_opportunities().await?;
    
    // 打印结果
    for opportunity in opportunities {
        println!("发现套利机会: {} -> {}", 
                opportunity.buy_dex, 
                opportunity.sell_dex);
        println!("利润率: {:.2}%", opportunity.profit_percentage);
        println!("置信度: {:.1}", opportunity.confidence_score);
        println!("---");
    }
    
    Ok(())
}
```

### 持续监控

```rust
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::default();
    let mut monitor = ArbitrageMonitor::new(config.clone()).await?;
    
    loop {
        match monitor.scan_opportunities().await {
            Ok(opportunities) => {
                if !opportunities.is_empty() {
                    println!("发现 {} 个套利机会", opportunities.len());
                    for opp in opportunities.iter().take(3) {
                        println!("  {} -> {}: {:.2}%", 
                                opp.buy_dex, 
                                opp.sell_dex, 
                                opp.profit_percentage);
                    }
                }
            }
            Err(e) => {
                eprintln!("扫描错误: {}", e);
            }
        }
        
        sleep(Duration::from_secs(config.monitoring.scan_interval_seconds)).await;
    }
}
```

## 项目结构

```
src/
├── main.rs              # 主程序入口
├── config.rs            # 配置管理
├── types.rs             # 数据类型定义
├── utils.rs             # 工具函数
├── monitor.rs           # 套利监控核心逻辑
└── dex/                 # DEX 接口实现
    ├── mod.rs           # DEX 抽象接口
    ├── uniswap.rs       # Uniswap V2 实现
    ├── sushiswap.rs     # SushiSwap 实现
    ├── pancakeswap.rs   # PancakeSwap 实现
    ├── curve.rs         # Curve 实现
    └── balancer.rs      # Balancer 实现
```

## 核心概念

### 套利机会

套利机会由以下要素组成：
- **代币对**: 要交易的代币组合
- **买入 DEX**: 价格较低的交易所
- **卖出 DEX**: 价格较高的交易所
- **利润率**: 价格差异百分比
- **流动性**: 可用的交易深度
- **置信度**: 基于多个因素的风险评估

### 置信度计算

置信度分数（0-100）基于以下因素：
- 利润率大小（40%权重）
- 流动性深度（30%权重）
- 价格稳定性（30%权重）

### 风险控制

- **最小利润阈值**: 过滤低利润机会
- **流动性检查**: 确保有足够的交易深度
- **Gas 成本估算**: 考虑交易成本
- **滑点保护**: 避免大额交易的价格冲击

## 性能优化

- **异步并发**: 同时查询多个 DEX，提高效率
- **连接池**: 复用 HTTP 连接，减少延迟
- **缓存机制**: 避免重复的 API 调用
- **批量查询**: 一次获取多个代币对的价格

## 注意事项

⚠️ **风险提示**:
- 本工具仅用于监控和分析，不提供自动交易功能
- 套利交易存在风险，包括但不限于滑点、Gas 费用波动、MEV 竞争等
- 请在充分了解风险的情况下进行实际交易
- 建议在测试网络上验证策略后再进行主网操作

## 开发计划

- [ ] 添加更多 DEX 支持（1inch、0x 等）
- [ ] 实现 WebSocket 实时数据流
- [ ] 添加 MEV 保护机制
- [ ] 支持跨链套利监控
- [ ] 集成 Telegram/Discord 通知
- [ ] 添加历史数据分析
- [ ] 实现策略回测功能

## 贡献

欢迎提交 Issue 和 Pull Request！

## 许可证

MIT License - 详见 [LICENSE](LICENSE) 文件

## 免责声明

本软件仅供学习和研究使用。使用本软件进行实际交易的任何损失，开发者不承担责任。请在使用前充分了解相关风险。