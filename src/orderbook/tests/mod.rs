// OrderBook 测试模块
// OrderBook Test Module

use crate::orderbook::{MarginOrder, OrderBookDBManager, OrderBookHeader};
use rocksdb::{Options, DB};
use std::sync::Arc;
use uuid::Uuid;

/// 创建临时测试数据库
/// Create temporary test database
pub fn create_test_db() -> (Arc<DB>, String) {
    let temp_dir = std::env::temp_dir().join(format!("orderbook_test_{}", Uuid::new_v4()));
    let mut opts = Options::default();
    opts.create_if_missing(true);
    let db = DB::open(&opts, &temp_dir).expect("Failed to open test DB");
    (Arc::new(db), temp_dir.to_string_lossy().to_string())
}

/// 清理临时测试数据库
/// Clean up temporary test database
pub fn cleanup_test_db(path: &str) {
    let _ = std::fs::remove_dir_all(path);
}

/// 创建测试用 OrderBook 管理器
/// Create test OrderBook manager
pub fn create_test_manager() -> (OrderBookDBManager, String) {
    let (db, temp_path) = create_test_db();
    let mint = "EPjFWaLb3crLvQQf89kiNqEX5jg5Kv431J06Y1AD3ic".to_string();
    let direction = "dn".to_string();
    let manager = OrderBookDBManager::new(db, mint, direction);
    (manager, temp_path)
}

/// 创建测试订单
/// Create test order
pub fn create_test_order(user: &str, price: u128) -> MarginOrder {
    MarginOrder {
        user: user.to_string(),
        lock_lp_start_price: price,
        lock_lp_end_price: price + 100000,
        open_price: price + 50000,
        order_id: 0, // 会被自动分配 / Will be auto-assigned
        lock_lp_sol_amount: 1000000000,
        lock_lp_token_amount: 5000000000,
        next_lp_sol_amount: 0,
        next_lp_token_amount: 0,
        margin_init_sol_amount: 100000000,
        margin_sol_amount: 100000000,
        borrow_amount: 900000000,
        position_asset_amount: 5000000000,
        realized_sol_amount: 0,
        version: 0, // 会被自动设置 / Will be auto-set
        start_time: 1735660800,
        end_time: 1735747200,
        next_order: u16::MAX,
        prev_order: u16::MAX,
        borrow_fee: 50,
        order_type: 1,
    }
}

mod insert_test;
mod delete_test;
mod update_test;
mod traverse_test;
mod stress_test;
mod bug_verification_test;
