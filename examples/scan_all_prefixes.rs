use rocksdb::{DB, IteratorMode};

fn main() {
    let db_path = "./data/orderbook.db";
    let db = DB::open_default(db_path).expect("Failed to open DB");

    // 查找所有不同的前缀
    let prefixes = vec![
        "orderbook_header:",
        "orderbook_slot:",
        "orderbook_id_map:",
        "orderbook_user:",
        "orderbook_closed:",
    ];

    for prefix in prefixes {
        println!("🔍 前缀: {}", prefix);
        let mut count = 0;
        let iter = db.iterator(IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward));

        for item in iter {
            let (key, _) = item.expect("Iterator error");
            let key_str = String::from_utf8_lossy(&key);

            if !key_str.starts_with(prefix) {
                break;
            }

            count += 1;
            if count <= 3 {
                println!("  - {}", key_str);
            }
        }

        println!("  总计: {} 条记录\n", count);
    }
}
