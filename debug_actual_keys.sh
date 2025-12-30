#!/bin/bash
# 调试脚本: 打印实际存储的键

cat << 'EOF' > /tmp/check_keys.rs
use rocksdb::DB;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <db_path>", args[0]);
        return;
    }

    let db_path = &args[1];
    let db = DB::open_for_read_only(&Default::default(), db_path, false)
        .expect("Failed to open DB");

    let prefix = "orderbook_user_closed:GKApmS6rzjjj1StwkWWuoXUGPjz7r8owSn8sV47pLzZF:";

    println!("=== 实际存储的键 ===");
    let iter = db.prefix_iterator(prefix.as_bytes());

    for (i, item) in iter.enumerate() {
        if i >= 10 { break; }  // 只显示前10条

        let (key, _value) = item.unwrap();
        let key_str = String::from_utf8_lossy(&key);

        if !key_str.starts_with(prefix) {
            break;
        }

        // 解析键: orderbook_user_closed:{user}:{inverted_ts}:{mint}:{direction}:{order_id}
        let parts: Vec<&str> = key_str.split(':').collect();
        if parts.len() >= 6 {
            let inverted_ts = parts[2];
            let mint_short = &parts[3][..8.min(parts[3].len())];
            let direction = parts[4];
            let order_id = parts[5];

            // 计算原始时间戳
            if let Ok(inv_ts) = inverted_ts.parse::<u32>() {
                let original_ts = u32::MAX - inv_ts;
                println!("[{}] inv_ts={}, original={}, mint={}, dir={}, order_id={}",
                    i, inverted_ts, original_ts, mint_short, direction, order_id);
            }
        }
    }
}
EOF

rustc --edition 2021 /tmp/check_keys.rs \
  --extern rocksdb=$(ls target/debug/deps/librocksdb-*.rlib | head -1) \
  -L target/debug/deps \
  -o /tmp/check_keys && \
echo "编译成功! 请手动运行:" && \
echo "/tmp/check_keys /path/to/rocksdb"
