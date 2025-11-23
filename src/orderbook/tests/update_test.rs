// 更新操作测试
// Update Operation Tests

use super::*;
use crate::orderbook::MarginOrderUpdateData;

#[test]
fn test_update_single_field() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入订单
    let order1 = create_test_order("UserA", 1000000);
    let (index, order_id) = manager.insert_after(u16::MAX, &order1).unwrap();

    // 更新保证金
    let update_data = MarginOrderUpdateData {
        margin_sol_amount: Some(90000000),
        ..Default::default()
    };

    manager.update_order(index, order_id, &update_data).unwrap();

    // 验证更新
    let updated_order = manager.get_order(index).unwrap();
    assert_eq!(updated_order.margin_sol_amount, 90000000);
    assert_eq!(updated_order.version, 2); // 版本递增

    cleanup_test_db(&temp_path);
}

#[test]
fn test_update_multiple_fields() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入订单
    let order1 = create_test_order("UserA", 1000000);
    let (index, order_id) = manager.insert_after(u16::MAX, &order1).unwrap();

    // 更新多个字段
    let update_data = MarginOrderUpdateData {
        margin_sol_amount: Some(90000000),
        realized_sol_amount: Some(5000000),
        position_asset_amount: Some(4500000000),
        ..Default::default()
    };

    manager.update_order(index, order_id, &update_data).unwrap();

    // 验证更新
    let updated_order = manager.get_order(index).unwrap();
    assert_eq!(updated_order.margin_sol_amount, 90000000);
    assert_eq!(updated_order.realized_sol_amount, 5000000);
    assert_eq!(updated_order.position_asset_amount, 4500000000);
    assert_eq!(updated_order.version, 2);

    cleanup_test_db(&temp_path);
}

#[test]
fn test_update_with_wrong_order_id() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入订单
    let order1 = create_test_order("UserA", 1000000);
    let (index, _order_id) = manager.insert_after(u16::MAX, &order1).unwrap();

    // 使用错误的 order_id 更新
    let update_data = MarginOrderUpdateData {
        margin_sol_amount: Some(90000000),
        ..Default::default()
    };

    let result = manager.update_order(index, 999, &update_data);
    assert!(result.is_err());

    cleanup_test_db(&temp_path);
}

#[test]
fn test_update_preserves_immutable_fields() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入订单
    let order1 = create_test_order("UserA", 1000000);
    let (index, order_id) = manager.insert_after(u16::MAX, &order1).unwrap();

    // 保存原始值
    let original_order = manager.get_order(index).unwrap();
    let original_user = original_order.user.clone();
    let original_order_id = original_order.order_id;
    let original_start_time = original_order.start_time;
    let original_order_type = original_order.order_type;

    // 更新可变字段
    let update_data = MarginOrderUpdateData {
        margin_sol_amount: Some(90000000),
        ..Default::default()
    };

    manager.update_order(index, order_id, &update_data).unwrap();

    // 验证不可变字段没有改变
    let updated_order = manager.get_order(index).unwrap();
    assert_eq!(updated_order.user, original_user);
    assert_eq!(updated_order.order_id, original_order_id);
    assert_eq!(updated_order.start_time, original_start_time);
    assert_eq!(updated_order.order_type, original_order_type);

    cleanup_test_db(&temp_path);
}

#[test]
fn test_multiple_updates_increment_version() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入订单
    let order1 = create_test_order("UserA", 1000000);
    let (index, order_id) = manager.insert_after(u16::MAX, &order1).unwrap();

    // 第一次更新
    let update_data1 = MarginOrderUpdateData {
        margin_sol_amount: Some(90000000),
        ..Default::default()
    };
    manager.update_order(index, order_id, &update_data1).unwrap();

    let order = manager.get_order(index).unwrap();
    assert_eq!(order.version, 2);

    // 第二次更新
    let update_data2 = MarginOrderUpdateData {
        realized_sol_amount: Some(5000000),
        ..Default::default()
    };
    manager.update_order(index, order_id, &update_data2).unwrap();

    let order = manager.get_order(index).unwrap();
    assert_eq!(order.version, 3);

    // 第三次更新
    let update_data3 = MarginOrderUpdateData {
        position_asset_amount: Some(4500000000),
        ..Default::default()
    };
    manager.update_order(index, order_id, &update_data3).unwrap();

    let order = manager.get_order(index).unwrap();
    assert_eq!(order.version, 4);

    cleanup_test_db(&temp_path);
}
