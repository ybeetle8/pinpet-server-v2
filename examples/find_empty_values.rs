use rocksdb::{DB, IteratorMode};

fn main() {
    let db_path = "./data/orderbook.db";
    let db = DB::open_default(db_path).expect("Failed to open DB");

    println!("🔍 扫描所有 orderbook_id_map 键，查找空值...");
    println!();

    let prefix = "orderbook_id_map:";
    let mut count = 0;
    let mut empty_count = 0;

    let iter = db.iterator(IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward));

    for item in iter {
        let (key, value) = item.expect("Iterator error");
        let key_str = String::from_utf8_lossy(&key);

        if !key_str.starts_with(prefix) {
            break;
        }

        count += 1;

        if value.is_empty() {
            empty_count += 1;
            println!("❌ 发现空值: {}", key_str);

            if empty_count >= 5 {
                println!("... (只显示前5个空值)");
                break;
            }
        }

        if count >= 1000 && empty_count == 0 {
            println!("✅ 已检查 {} 条记录，未发现空值", count);
            break;
        }
    }

    println!();
    println!("总共检查: {} 条记录", count);
    println!("发现空值: {} 条", empty_count);
}
