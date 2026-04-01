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
    println!("🔍 检查用户订单数据: {}", user);
    println!("前缀: {}", prefix);
    println!();

    let mut count = 0;
    let mut empty_id_map_count = 0;
    let mut empty_slot_count = 0;
    let mut invalid_json_count = 0;

    let iter = db.iterator(IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward));

    for item in iter {
        let (key, _value) = item.expect("Iterator error");
        let key_str = String::from_utf8_lossy(&key);

        if !key_str.starts_with(&prefix) {
            break;
        }

        count += 1;
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("📌 记录 #{}", count);
        println!("键: {}", key_str);

        // 解析键
        let parts: Vec<&str> = key_str.split(':').collect();
        if parts.len() != 6 {
            println!("❌ 键格式错误，应该有6个部分，实际有 {} 个", parts.len());
            continue;
        }

        let mint = parts[2];
        let direction = parts[3];
        let order_id: u64 = match parts[5].parse() {
            Ok(id) => id,
            Err(e) => {
                println!("❌ 无法解析 order_id: {}", e);
                continue;
            }
        };

        println!("  mint: {}", mint);
        println!("  direction: {}", direction);
        println!("  order_id: {}", order_id);

        // 检查 id_map
        let id_key = format!("orderbook_id_map:{}:{}:{:010}", mint, direction, order_id);
        println!("\n🔍 检查 id_map: {}", id_key);

        match db.get(id_key.as_bytes()) {
            Ok(Some(index_bytes)) => {
                println!("  ✅ id_map 存在");
                println!("  字节长度: {}", index_bytes.len());
                println!("  原始字节: {:?}", &index_bytes[..index_bytes.len().min(20)]);

                if index_bytes.is_empty() {
                    println!("  ❌ 数据为空！");
                    empty_id_map_count += 1;
                    continue;
                }

                match serde_json::from_slice::<u16>(&index_bytes) {
                    Ok(index) => {
                        println!("  ✅ 反序列化成功: index={}", index);

                        // 检查 slot
                        let slot_key = format!("orderbook_slot:{}:{}:{:05}", mint, direction, index);
                        println!("\n🔍 检查 slot: {}", slot_key);

                        match db.get(slot_key.as_bytes()) {
                            Ok(Some(order_bytes)) => {
                                println!("  ✅ slot 数据存在");
                                println!("  字节长度: {}", order_bytes.len());

                                if order_bytes.is_empty() {
                                    println!("  ❌ 数据为空！");
                                    empty_slot_count += 1;
                                    continue;
                                }

                                // 显示前50字节
                                let preview_len = order_bytes.len().min(50);
                                println!("  前{}字节: {:?}", preview_len, &order_bytes[..preview_len]);
                                println!("  ASCII预览: {}", String::from_utf8_lossy(&order_bytes[..preview_len]));

                                // 尝试 JSON 反序列化
                                match serde_json::from_slice::<serde_json::Value>(&order_bytes) {
                                    Ok(_) => println!("  ✅ JSON 反序列化成功"),
                                    Err(e) => {
                                        println!("  ❌ JSON 反序列化失败: {}", e);
                                        invalid_json_count += 1;
                                    }
                                }
                            }
                            Ok(None) => println!("  ❌ slot 数据不存在"),
                            Err(e) => println!("  ❌ 读取 slot 失败: {}", e),
                        }
                    }
                    Err(e) => {
                        println!("  ❌ 反序列化失败: {}", e);
                        invalid_json_count += 1;
                    }
                }
            }
            Ok(None) => println!("  ❌ id_map 不存在"),
            Err(e) => println!("  ❌ 读取 id_map 失败: {}", e),
        }

        println!();

        if count >= 5 {
            println!("... (只显示前5条)");
            break;
        }
    }

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("\n📊 统计结果:");
    println!("  总记录数: {}", count);
    println!("  空 id_map: {}", empty_id_map_count);
    println!("  空 slot: {}", empty_slot_count);
    println!("  JSON 反序列化失败: {}", invalid_json_count);
}
