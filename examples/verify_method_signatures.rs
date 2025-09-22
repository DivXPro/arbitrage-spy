use ethers::prelude::*;
use ethers::utils::keccak256;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("验证 Uniswap V3 池合约方法签名");
    println!("=====================================");
    
    // 计算各种方法的签名
    let methods = vec![
        "slot0()",
        "token0()",
        "token1()",
        "fee()",
        "liquidity()",
        "feeGrowthGlobal0X128()",
        "feeGrowthGlobal1X128()",
        "tickSpacing()",
        "maxLiquidityPerTick()",
        "protocolFees()",
    ];
    
    for method in methods {
        let hash = keccak256(method.as_bytes());
        let signature = format!("0x{:02x}{:02x}{:02x}{:02x}", hash[0], hash[1], hash[2], hash[3]);
        println!("{:<25} -> {}", method, signature);
    }
    
    println!("\n当前代码中使用的签名:");
    println!("liquidity()              -> 0x128acb08");
    println!("feeGrowthGlobal0X128()   -> 0xf3058b14");
    println!("feeGrowthGlobal1X128()   -> 0x46141f19");
    
    Ok(())
}