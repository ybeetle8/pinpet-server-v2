// 测试 RocksDB prefix_iterator 的排序行为
// Test RocksDB prefix_iterator ordering behavior

use rocksdb::DB;
use std::path::Path;

fn main() {
    // 创建临时数据库
    let db_path = "/tmp/test_prefix_order";
    let _ = std::fs::remove_dir_all(db_path);
    let db = DB::open_default(db_path).unwrap();

    // 插入测试数据
    let test_data = vec![
        ("test:2527896597:a", "oldest"),  // inverted from 1767070698
        ("test:2527890516:b", "newest"),  // inverted from 1767076779
        ("test:2527892369:c", "middle1"), // inverted from 1767074926
        ("test:2527892397:d", "middle2"), // inverted from 1767074898
        ("test:2527893081:e", "middle3"), // inverted from 1767074214
    ];

    for (key, value) in &test_data {
        db.put(key.as_bytes(), value.as_bytes()).unwrap();
    }

    println!("=== 测试 prefix_iterator 的顺序 ===\n");
    println!("插入的键(按 inverted_ts 大小):");
    for (key, value) in &test_data {
        println!("  {} = {}", key, value);
    }

    println!("\n=== prefix_iterator 返回顺序 ===");
    let mut results = Vec::new();
    let iter = db.prefix_iterator("test:".as_bytes());
    for (i, item) in iter.enumerate() {
        let (key, value) = item.unwrap();
        let key_str = String::from_utf8_lossy(&key);
        let value_str = String::from_utf8_lossy(&value);
        println!("  [{}] {} = {}", i, key_str, value_str);
        results.push((key_str.to_string(), value_str.to_string()));
    }

    println!("\n=== 结论 ===");
    if results[0].1 == "newest" {
        println!("❌ prefix_iterator 返回降序 (从大到小)");
        println!("   这意味着反转时间戳设计是错误的!");
    } else if results[0].1 == "oldest" {
        println!("✅ prefix_iterator 返回升序 (从小到大)");
        println!("   反转时间戳设计正确,但需要在返回前 reverse()");
    }

    // 清理
    drop(db);
    let _ = std::fs::remove_dir_all(db_path);
}
