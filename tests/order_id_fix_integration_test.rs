// order_id 修复集成测试
// order_id Fix Integration Test
//
// 这是一个独立的集成测试,不依赖其他测试文件
// This is a standalone integration test, independent of other test files

use pinpet_server_v2::orderbook::{MarginOrder, OrderBookDBManager, OrderBookError};
use rocksdb::{Options, DB};
use std::sync::Arc;
use uuid::Uuid;

/// 创建临时测试数据库
fn create_test_db() -> (Arc<DB>, String) {
    let temp_dir = std::env::temp_dir().join(format!("orderbook_test_{}", Uuid::new_v4()));
    let mut opts = Options::default();
    opts.create_if_missing(true);
    let db = DB::open(&opts, &temp_dir).expect("Failed to open test DB");
    (Arc::new(db), temp_dir.to_string_lossy().to_string())
}

/// 清理临时测试数据库
fn cleanup_test_db(path: &str) {
    let _ = std::fs::remove_dir_all(path);
}

/// 创建测试用 OrderBook 管理器
fn create_test_manager() -> (OrderBookDBManager, String) {
    let (db, temp_path) = create_test_db();
    let mint = "EPjFWaLb3crLvQQf89kiNqEX5jg5Kv431J06Y1AD3ic".to_string();
    let direction = "dn".to_string();
    let manager = OrderBookDBManager::new(db, mint, direction);
    (manager, temp_path)
}

/// 创建带指定 order_id 的测试订单
fn create_order_with_id(user: &str, order_id: u64, price: u128) -> MarginOrder {
    MarginOrder {
        user: user.to_string(),
        lock_lp_start_price: price,
        lock_lp_end_price: price + 100000,
        open_price: price + 50000,
        order_id,
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

#[test]
fn test_insert_with_event_order_id() {
    let (manager, temp_path) = create_test_manager();
    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    let order = create_order_with_id("UserA", 100, 1000000);
    let (index, assigned_order_id) = manager.insert_after(u16::MAX, &order).unwrap();

    assert_eq!(assigned_order_id, 100);
    assert_eq!(index, 0);

    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 101);

    let stored_order = manager.get_order(0).unwrap();
    assert_eq!(stored_order.order_id, 100);

    cleanup_test_db(&temp_path);
    println!("✅ test_insert_with_event_order_id passed");
}

#[test]
fn test_insert_with_zero_order_id_should_fail() {
    let (manager, temp_path) = create_test_manager();
    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    let order = create_order_with_id("UserA", 0, 1000000);
    let result = manager.insert_after(u16::MAX, &order);

    assert!(result.is_err());
    match result.unwrap_err() {
        OrderBookError::InvalidOrderId(msg) => {
            assert!(msg.contains("cannot be 0"));
        }
        _ => panic!("expected InvalidOrderId error"),
    }

    let header = manager.load_header().unwrap();
    assert_eq!(header.total, 0);

    cleanup_test_db(&temp_path);
    println!("✅ test_insert_with_zero_order_id_should_fail passed");
}

#[test]
fn test_insert_multiple_orders_with_non_sequential_ids() {
    let (manager, temp_path) = create_test_manager();
    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    let order1 = create_order_with_id("UserA", 100, 1000000);
    let (index1, id1) = manager.insert_after(u16::MAX, &order1).unwrap();
    assert_eq!(id1, 100);
    assert_eq!(index1, 0);

    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 101);

    let order2 = create_order_with_id("UserB", 200, 2000000);
    let (index2, id2) = manager.insert_after(0, &order2).unwrap();
    assert_eq!(id2, 200);
    assert_eq!(index2, 1);

    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 201);

    let order3 = create_order_with_id("UserC", 150, 1500000);
    let (index3, id3) = manager.insert_after(1, &order3).unwrap();
    assert_eq!(id3, 150);
    assert_eq!(index3, 2);

    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 201);
    assert_eq!(header.total, 3);

    assert_eq!(manager.get_order(0).unwrap().order_id, 100);
    assert_eq!(manager.get_order(1).unwrap().order_id, 200);
    assert_eq!(manager.get_order(2).unwrap().order_id, 150);

    cleanup_test_db(&temp_path);
    println!("✅ test_insert_multiple_orders_with_non_sequential_ids passed");
}

#[test]
fn test_order_id_counter_is_metadata_only() {
    let (manager, temp_path) = create_test_manager();
    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    let order1 = create_order_with_id("UserA", 1000, 1000000);
    manager.insert_after(u16::MAX, &order1).unwrap();
    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 1001);

    let order2 = create_order_with_id("UserB", 500, 2000000);
    manager.insert_after(0, &order2).unwrap();
    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 1001);

    let order3 = create_order_with_id("UserC", 2000, 3000000);
    manager.insert_after(1, &order3).unwrap();
    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 2001);

    assert_eq!(manager.get_order(0).unwrap().order_id, 1000);
    assert_eq!(manager.get_order(1).unwrap().order_id, 500);
    assert_eq!(manager.get_order(2).unwrap().order_id, 2000);

    cleanup_test_db(&temp_path);
    println!("✅ test_order_id_counter_is_metadata_only passed");
}

#[test]
fn test_insert_before_with_event_order_id() {
    let (manager, temp_path) = create_test_manager();
    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    let order1 = create_order_with_id("UserA", 100, 1000000);
    manager.insert_after(u16::MAX, &order1).unwrap();

    let order2 = create_order_with_id("UserB", 50, 2000000);
    let (index, assigned_id) = manager.insert_before(0, &order2).unwrap();

    assert_eq!(assigned_id, 50);
    assert_eq!(index, 1);

    let stored_order = manager.get_order(1).unwrap();
    assert_eq!(stored_order.order_id, 50);

    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 101);

    cleanup_test_db(&temp_path);
    println!("✅ test_insert_before_with_event_order_id passed");
}

#[test]
fn test_insert_before_with_zero_order_id_should_fail() {
    let (manager, temp_path) = create_test_manager();
    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    let order1 = create_order_with_id("UserA", 100, 1000000);
    manager.insert_after(u16::MAX, &order1).unwrap();

    let order2 = create_order_with_id("UserB", 0, 2000000);
    let result = manager.insert_before(0, &order2);

    assert!(result.is_err());
    match result.unwrap_err() {
        OrderBookError::InvalidOrderId(_) => {}
        _ => panic!("expected InvalidOrderId error"),
    }

    let header = manager.load_header().unwrap();
    assert_eq!(header.total, 1);

    cleanup_test_db(&temp_path);
    println!("✅ test_insert_before_with_zero_order_id_should_fail passed");
}

#[test]
fn test_large_scale_random_order_ids() {
    let (manager, temp_path) = create_test_manager();
    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    let order_ids = vec![10, 50, 30, 100, 25, 200, 75, 150, 5, 300];
    let mut max_order_id = 0u64;

    for (i, &order_id) in order_ids.iter().enumerate() {
        let order = create_order_with_id(&format!("User{}", i), order_id, 1000000 + i as u128 * 100000);

        if i == 0 {
            manager.insert_after(u16::MAX, &order).unwrap();
        } else {
            manager.insert_after((i - 1) as u16, &order).unwrap();
        }

        if order_id > max_order_id {
            max_order_id = order_id;
        }

        let header = manager.load_header().unwrap();
        assert_eq!(header.order_id_counter, max_order_id + 1);
    }

    for (i, &expected_id) in order_ids.iter().enumerate() {
        let order = manager.get_order(i as u16).unwrap();
        assert_eq!(order.order_id, expected_id);
    }

    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 301);
    assert_eq!(header.total, 10);

    cleanup_test_db(&temp_path);
    println!("✅ test_large_scale_random_order_ids passed");
}
