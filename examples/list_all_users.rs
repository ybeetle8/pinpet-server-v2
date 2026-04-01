use rocksdb::{DB, IteratorMode};

fn main() {
    let db_path = "./data/orderbook.db";
    let db = DB::open_default(db_path).expect("Failed to open DB");

    let prefix = "orderbook_user:";
    println!("🔍 扫描所有用户订单键...");
    println!();

    let mut count = 0;
    let iter = db.iterator(IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward));

    for item in iter {
        let (key, _) = item.expect("Iterator error");
        let key_str = String::from_utf8_lossy(&key);

        if !key_str.starts_with(prefix) {
            break;
        }

        count += 1;
        println!("📌 键 #{}: {}", count, key_str);

        if count >= 10 {
            println!("... (只显示前10条)");
            break;
        }
    }

    println!();
    println!("总共找到 {} 条用户订单记录", count);
}
