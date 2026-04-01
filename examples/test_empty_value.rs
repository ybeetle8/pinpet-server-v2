use rocksdb::DB;

fn main() {
    let db_path = "./data/orderbook.db";
    let db = DB::open_default(db_path).expect("Failed to open DB");

    // 测试查找不存在的键
    let test_keys = vec![
        "orderbook_id_map:test:up:0000000001",
        "orderbook_slot:test:up:00001",
    ];

    for key in test_keys {
        println!("🔍 测试键: {}", key);
        match db.get(key.as_bytes()) {
            Ok(Some(bytes)) => {
                println!("  ✅ 找到数据, 长度={}", bytes.len());
                if bytes.is_empty() {
                    println!("  ⚠️  数据为空!");
                    println!("  尝试反序列化空数据...");
                    match serde_json::from_slice::<u16>(&bytes) {
                        Ok(v) => println!("  结果: {}", v),
                        Err(e) => println!("  ❌ 错误: {}", e),
                    }
                }
            }
            Ok(None) => println!("  ℹ️  键不存在"),
            Err(e) => println!("  ❌ 读取错误: {}", e),
        }
        println!();
    }

    // 测试空字节数组的反序列化
    println!("🧪 测试空字节数组反序列化:");
    let empty: &[u8] = &[];
    match serde_json::from_slice::<u16>(empty) {
        Ok(v) => println!("  结果: {}", v),
        Err(e) => println!("  ❌ 错误: {}", e),
    }
}
