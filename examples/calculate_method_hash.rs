use ethers::utils::{keccak256, hex};

fn main() {
    let methods = vec![
        "token0()",
        "token1()",
        "fee()",
        "slot0()",
        "liquidity()",
        "feeGrowthGlobal0X128()",
        "feeGrowthGlobal1X128()",
    ];

    println!("🔍 Uniswap V3 方法签名哈希计算");
    println!("================================");

    for method in methods {
        let hash = keccak256(method.as_bytes());
        let selector = &hash[0..4];
        println!("{}:", method);
        println!("  完整哈希: 0x{}", hex::encode(hash));
        println!("  选择器:   0x{}", hex::encode(selector));
        println!("  字节数组: {:?}", selector);
        println!();
    }
}