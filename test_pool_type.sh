#!/bin/bash
# 测试脚本：检查数据库中的高级版币种

echo "=== 检查数据库中所有币种的 pool_type ==="
echo ""

# 使用 Rust 代码直接读取 RocksDB
cat > /tmp/check_db.rs << 'EOF'
use rocksdb::DB;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct TokenDetail {
    pub pool_type: Option<u8>,
    pub mint_account: Option<String>,
    pub symbol: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = DB::open_default("/home/ybeetle/ybeetle8/pinpet-server-v2/data/event")?;

    println!("扫描所有 token: 前缀的数据...");
    let prefix = b"token:";
    let iter = db.prefix_iterator(prefix);

    let mut count = 0;
    let mut advanced_count = 0;

    for item in iter {
        let (key, value) = item?;
        let key_str = String::from_utf8_lossy(&key);

        if !key_str.starts_with("token:") {
            break;
        }

        if let Ok(detail) = serde_json::from_slice::<serde_json::Value>(&value) {
            count += 1;
            if let Some(pool_type) = detail.get("pool_type") {
                if pool_type.as_u64() == Some(1) {
                    advanced_count += 1;
                    println!("找到高级版币种: mint={}, symbol={}, pool_type=1",
                        detail.get("mint_account").and_then(|v| v.as_str()).unwrap_or("unknown"),
                        detail.get("symbol").and_then(|v| v.as_str()).unwrap_or("unknown"));
                }
            }
        }
    }

    println!("\n总计: {} 个币种", count);
    println!("高级版币种: {} 个", advanced_count);

    Ok(())
}
EOF

echo "创建临时检查程序..."
cd /tmp
cargo init --bin check_db_temp 2>/dev/null
cd check_db_temp
echo '[dependencies]
rocksdb = "0.22"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"' > Cargo.toml
mv /tmp/check_db.rs src/main.rs
cargo run 2>&1
