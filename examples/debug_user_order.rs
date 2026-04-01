use rocksdb::{DB, IteratorMode};
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    let user = if args.len() > 1 {
        &args[1]
    } else {
        "F5eLfL7qeEZhvbbXT1pmkiLN1opRFcQSDyaDaRSQ2ehD"
    };

    let db_path = "./data/orderbook.db";
    let db = DB::open_default(db_path).expect("Failed to open DB");

    let prefix = format!("orderbook_user:{}:", user);
    println!("🔍 扫描前缀: {}", prefix);
    println!();

    let mut count = 0;
    let iter = db.iterator(IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward));

    for item in iter {
        let (key, value) = item.expect("Iterator error");
        let key_str = String::from_utf8_lossy(&key);

        if !key_str.starts_with(&prefix) {
            break;
        }

        count += 1;
        println!("📌 键 #{}: {}", count, key_str);

        // 解析键
        let parts: Vec<&str> = key_str.split(':').collect();
        if parts.len() == 6 {
            println!("   - mint: {}", parts[2]);
            println!("   - direction: {}", parts[3]);
            println!("   - start_time: {}", parts[4]);
            println!("   - order_id: {}", parts[5]);
        }

        // 尝试获取对应的订单数据
        if parts.len() == 6 {
            let mint = parts[2];
            let direction = parts[3];
            let order_id: u64 = parts[5].parse().unwrap_or(0);

            // 查询 id_map
            let id_key = format!("orderbook_id_map:{}:{}:{:010}", mint, direction, order_id);
            match db.get(id_key.as_bytes()) {
                Ok(Some(index_bytes)) => {
                    match serde_json::from_slice::<u16>(&index_bytes) {
                        Ok(index) => {
                            println!("   ✅ id_map 存在, index={}", index);

                            // 查询 slot 数据
                            let slot_key = format!("orderbook_slot:{}:{}:{:05}", mint, direction, index);
                            match db.get(slot_key.as_bytes()) {
                                Ok(Some(order_bytes)) => {
                                    println!("   ✅ slot 数据存在, 大小={} bytes", order_bytes.len());

                                    // 显示原始数据的前100字节
                                    let preview = if order_bytes.len() > 100 {
                                        &order_bytes[..100]
                                    } else {
                                        &order_bytes[..]
                                    };
                                    println!("   📄 数据预览: {}", String::from_utf8_lossy(preview));

                                    // 尝试反序列化
                                    match serde_json::from_slice::<serde_json::Value>(&order_bytes) {
                                        Ok(_) => println!("   ✅ JSON 反序列化成功"),
                                        Err(e) => println!("   ❌ JSON 反序列化失败: {}", e),
                                    }
                                }
                                Ok(None) => println!("   ❌ slot 数据不存在"),
                                Err(e) => println!("   ❌ 读取 slot 失败: {}", e),
                            }
                        }
                        Err(e) => println!("   ❌ 解析 index 失败: {}", e),
                    }
                }
                Ok(None) => println!("   ❌ id_map 不存在"),
                Err(e) => println!("   ❌ 读取 id_map 失败: {}", e),
            }
        }

        println!();

        if count >= 3 {
            println!("... (只显示前3条)");
            break;
        }
    }

    println!("总共找到 {} 条记录", count);
}
