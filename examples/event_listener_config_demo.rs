//! EventListener动态合约配置示例
//! 
//! 这个示例展示了如何使用EventListener的动态合约配置功能

use arbitrage_spy::event_listener::EventListener;
use arbitrage_spy::database::Database;
use arbitrage_spy::table_display::DisplayMessage;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    env_logger::init();
    
    // 创建数据库连接
    let database = Database::new(Some(":memory:"))?;
    
    // 创建消息通道
    let (sender, _receiver) = mpsc::channel::<DisplayMessage>(100);
    
    // 创建EventListener实例
    let mut event_listener = EventListener::new(
        database,
        sender,
        10,
        Duration::from_secs(5),
    ).await;
    
    println!("=== EventListener动态合约配置示例 ===");
    
    // 1. 查看默认加载的合约
    println!("\n1. 默认加载的合约地址:");
    for (name, address) in event_listener.get_contracts() {
        println!("   {} -> {:?}", name, address);
    }
    
    // 2. 添加新的合约地址
    println!("\n2. 添加新的合约地址:");
    event_listener.add_contract(
        "CustomDEX".to_string(),
        "0x1234567890123456789012345678901234567890"
    )?;
    
    // 3. 批量添加合约
    println!("\n3. 批量添加合约:");
    let mut new_contracts = HashMap::new();
    new_contracts.insert("TestDEX1".to_string(), "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd".to_string());
    new_contracts.insert("TestDEX2".to_string(), "0x1111111111111111111111111111111111111111".to_string());
    event_listener.add_contracts(new_contracts)?;
    
    // 4. 查看更新后的合约列表
    println!("\n4. 更新后的合约列表:");
    for (name, address) in event_listener.get_contracts() {
        println!("   {} -> {:?}", name, address);
    }
    
    // 6. 移除合约
    println!("\n6. 移除合约:");
    let removed = event_listener.remove_contract("TestDEX1");
    println!("   移除TestDEX1: {}", removed);
    
    // 7. 查看最终的合约列表
    println!("\n7. 最终的合约列表:");
    for (name, address) in event_listener.get_contracts() {
        println!("   {} -> {:?}", name, address);
    }
    
    // 8. 清空所有合约并重新加载默认合约
    println!("\n8. 清空并重新加载默认合约:");
    event_listener.clear_contracts();
    
    println!("\n9. 重新加载后的合约列表:");
    for (name, address) in event_listener.get_contracts() {
        println!("   {} -> {:?}", name, address);
    }
    
    println!("\n=== 示例完成 ===");
    
    Ok(())
}