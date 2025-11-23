use pinpet_server_v2::orderbook::{MarginOrder, OrderBookDBManager, MarginOrderUpdateData};
use rocksdb::{Options, DB};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化 RocksDB
    println!("📝 步骤1: 初始化 RocksDB...");
    let mut opts = Options::default();
    opts.create_if_missing(true);
    let db_path = "/tmp/orderbook_manual_test";
    let db = DB::open(&opts, db_path)?;
    let db = Arc::new(db);
    println!("✅ RocksDB 已初始化: {}", db_path);

    // 2. 创建 OrderBookDBManager
    println!("\n📝 步骤2: 创建 OrderBookDBManager...");
    let mint = "EPjFWaLb3crLvQQf89kiNqEX5jg5Kv431J06Y1AD3ic".to_string();
    let direction = "dn".to_string();
    let manager = OrderBookDBManager::new(db.clone(), mint.clone(), direction.clone());
    println!("✅ OrderBookDBManager 已创建: {}:{}", mint, direction);

    // 3. 初始化 OrderBook
    println!("\n📝 步骤3: 初始化 OrderBook...");
    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority)?;
    let header = manager.load_header()?;
    println!("✅ OrderBook 已初始化:");
    println!("   - version: {}", header.version);
    println!("   - order_type: {}", header.order_type);
    println!("   - total: {}", header.total);

    // 4. 插入测试订单
    println!("\n📝 步骤4: 插入测试订单...");
    for i in 0..10 {
        let order = create_test_order(&format!("User{}", i), (i as u128 + 1) * 1000000);
        let (index, order_id) = if i == 0 {
            manager.insert_after(u16::MAX, &order)?
        } else {
            manager.insert_after((i - 1) as u16, &order)?
        };
        println!("   ✅ 插入订单 {}: index={}, order_id={}", i, index, order_id);
    }

    let header = manager.load_header()?;
    println!("✅ 已插入 {} 个订单", header.total);

    // 5. 遍历订单
    println!("\n📝 步骤5: 遍历所有订单...");
    let result = manager.traverse(u16::MAX, 0, |index, order| {
        println!(
            "   订单[{}]: user={}, order_id={}, price={}",
            index, order.user, order.order_id, order.lock_lp_start_price
        );
        Ok(true)
    })?;
    println!("✅ 遍历完成,处理了 {} 个订单", result.processed);

    // 6. 更新订单
    println!("\n📝 步骤6: 更新订单...");
    let update_data = MarginOrderUpdateData {
        margin_sol_amount: Some(90000000),
        realized_sol_amount: Some(5000000),
        ..Default::default()
    };
    manager.update_order(0, 0, &update_data)?;
    let updated_order = manager.get_order(0)?;
    println!("✅ 订单已更新:");
    println!("   - margin_sol_amount: {}", updated_order.margin_sol_amount);
    println!("   - realized_sol_amount: {}", updated_order.realized_sol_amount);
    println!("   - version: {}", updated_order.version);

    // 7. 删除订单 (只删除尾部,避免已知问题)
    println!("\n📝 步骤7: 删除尾部订单...");
    let header = manager.load_header()?;
    let tail = header.tail;
    manager.batch_remove_by_indices_unsafe(&[tail])?;
    let header = manager.load_header()?;
    println!("✅ 已删除尾部订单,剩余: {}", header.total);

    // 8. 通过 order_id 查询
    println!("\n📝 步骤8: 通过 order_id 查询订单...");
    let order = manager.get_order_by_id(3)?;
    println!("✅ 查询到订单:");
    println!("   - user: {}", order.user);
    println!("   - order_id: {}", order.order_id);
    println!("   - price: {}", order.lock_lp_start_price);

    // 9. 获取所有活跃订单
    println!("\n📝 步骤9: 获取所有活跃订单...");
    let active_orders = manager.get_all_active_orders()?;
    println!("✅ 活跃订单数: {}", active_orders.len());
    for (index, order) in active_orders.iter().take(3) {
        println!("   - index={}, user={}, order_id={}", index, order.user, order.order_id);
    }

    // 10. 清理
    println!("\n📝 步骤10: 清理测试数据...");
    drop(db);
    std::fs::remove_dir_all(db_path)?;
    println!("✅ 测试完成!");

    Ok(())
}

fn create_test_order(user: &str, price: u128) -> MarginOrder {
    MarginOrder {
        user: user.to_string(),
        lock_lp_start_price: price,
        lock_lp_end_price: price + 100000,
        open_price: price + 50000,
        order_id: 0,
        lock_lp_sol_amount: 1000000000,
        lock_lp_token_amount: 5000000000,
        next_lp_sol_amount: 0,
        next_lp_token_amount: 0,
        margin_init_sol_amount: 100000000,
        margin_sol_amount: 100000000,
        borrow_amount: 900000000,
        position_asset_amount: 5000000000,
        realized_sol_amount: 0,
        version: 0,
        start_time: 1735660800,
        end_time: 1735747200,
        next_order: u16::MAX,
        prev_order: u16::MAX,
        borrow_fee: 50,
        order_type: 1,
    }
}
