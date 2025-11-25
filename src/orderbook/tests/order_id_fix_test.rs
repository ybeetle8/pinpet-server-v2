// order_id 修复验证测试
// order_id Fix Verification Tests
//
// 测试目标: 验证 order_id 修复方案的正确性
// Test Goal: Verify the correctness of order_id fix solution
//
// 核心原则: 服务端不生成 order_id,所有 order_id 必须来自事件
// Core Principle: Server does not generate order_id, all order_id must come from events

use super::*;
use crate::orderbook::errors::OrderBookError;

/// 辅助函数: 创建带指定 order_id 的测试订单
/// Helper function: Create test order with specified order_id
fn create_order_with_id(user: &str, order_id: u64, price: u128) -> MarginOrder {
    let mut order = create_test_order(user, price);
    order.order_id = order_id;
    order
}

// ==================== 测试 1: 使用事件中的 order_id ====================
// ==================== Test 1: Use order_id from event ====================

#[test]
fn test_insert_with_event_order_id() {
    // 测试: 插入订单时使用事件中的 order_id
    // Test: Insert order using order_id from event
    let (manager, temp_path) = create_test_manager();
    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 创建订单,order_id = 100 (模拟来自事件)
    // Create order with order_id = 100 (simulating from event)
    let order = create_order_with_id("UserA", 100, 1000000);

    // 插入订单
    // Insert order
    let (index, assigned_order_id) = manager.insert_after(u16::MAX, &order).unwrap();

    // 验证: 返回的 order_id 应该是事件中的值 (100)
    // Verify: returned order_id should be the value from event (100)
    assert_eq!(assigned_order_id, 100, "assigned order_id should match event order_id");
    assert_eq!(index, 0, "first order should have index 0");

    // 验证: order_id_counter 应该被更新为 101
    // Verify: order_id_counter should be updated to 101
    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 101, "order_id_counter should be 101");

    // 验证: 存储的订单 order_id 应该是 100
    // Verify: stored order's order_id should be 100
    let stored_order = manager.get_order(0).unwrap();
    assert_eq!(stored_order.order_id, 100, "stored order_id should be 100");

    cleanup_test_db(&temp_path);
    println!("✅ test_insert_with_event_order_id passed");
}

// ==================== 测试 2: order_id 为 0 时应该报错 ====================
// ==================== Test 2: order_id = 0 should fail ====================

#[test]
fn test_insert_with_zero_order_id_should_fail() {
    // 测试: order_id = 0 应该返回错误
    // Test: order_id = 0 should return error
    let (manager, temp_path) = create_test_manager();
    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 创建订单,order_id = 0 (无效)
    // Create order with order_id = 0 (invalid)
    let order = create_order_with_id("UserA", 0, 1000000);

    // 插入应该失败
    // Insert should fail
    let result = manager.insert_after(u16::MAX, &order);

    // 验证: 应该返回 InvalidOrderId 错误
    // Verify: should return InvalidOrderId error
    assert!(result.is_err(), "insert with order_id = 0 should fail");
    match result.unwrap_err() {
        OrderBookError::InvalidOrderId(msg) => {
            assert!(msg.contains("cannot be 0"), "error message should mention 'cannot be 0'");
        }
        _ => panic!("expected InvalidOrderId error"),
    }

    // 验证: OrderBook 应该仍然为空
    // Verify: OrderBook should still be empty
    let header = manager.load_header().unwrap();
    assert_eq!(header.total, 0, "OrderBook should still be empty");

    cleanup_test_db(&temp_path);
    println!("✅ test_insert_with_zero_order_id_should_fail passed");
}

// ==================== 测试 3: 插入多个非连续 order_id ====================
// ==================== Test 3: Insert multiple non-sequential order_ids ====================

#[test]
fn test_insert_multiple_orders_with_non_sequential_ids() {
    // 测试: 插入 order_id = 100, 200, 150 (非连续)
    // Test: Insert order_id = 100, 200, 150 (non-sequential)
    let (manager, temp_path) = create_test_manager();
    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入 order_id = 100
    // Insert order_id = 100
    let order1 = create_order_with_id("UserA", 100, 1000000);
    let (index1, id1) = manager.insert_after(u16::MAX, &order1).unwrap();
    assert_eq!(id1, 100);
    assert_eq!(index1, 0);

    // 验证 counter = 101
    // Verify counter = 101
    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 101);

    // 插入 order_id = 200 (跳过了 101-199)
    // Insert order_id = 200 (skipped 101-199)
    let order2 = create_order_with_id("UserB", 200, 2000000);
    let (index2, id2) = manager.insert_after(0, &order2).unwrap();
    assert_eq!(id2, 200);
    assert_eq!(index2, 1);

    // 验证 counter = 201 (更新为最大值 + 1)
    // Verify counter = 201 (updated to max + 1)
    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 201);

    // 插入 order_id = 150 (在之前的范围内,但比 counter 小)
    // Insert order_id = 150 (within previous range, but less than counter)
    let order3 = create_order_with_id("UserC", 150, 1500000);
    let (index3, id3) = manager.insert_after(1, &order3).unwrap();
    assert_eq!(id3, 150);
    assert_eq!(index3, 2);

    // 验证 counter 保持 201 (因为 150 < 201)
    // Verify counter remains 201 (because 150 < 201)
    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 201);
    assert_eq!(header.total, 3);

    // 验证所有订单的 order_id 都正确存储
    // Verify all orders' order_id are correctly stored
    assert_eq!(manager.get_order(0).unwrap().order_id, 100);
    assert_eq!(manager.get_order(1).unwrap().order_id, 200);
    assert_eq!(manager.get_order(2).unwrap().order_id, 150);

    cleanup_test_db(&temp_path);
    println!("✅ test_insert_multiple_orders_with_non_sequential_ids passed");
}

// ==================== 测试 4: order_id_counter 只记录不生成 ====================
// ==================== Test 4: order_id_counter only records, not generates ====================

#[test]
fn test_order_id_counter_is_metadata_only() {
    // 测试: order_id_counter 只用于记录,不用于生成新 order_id
    // Test: order_id_counter is only for recording, not for generating new order_id
    let (manager, temp_path) = create_test_manager();
    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 第一个订单: order_id = 1000
    // First order: order_id = 1000
    let order1 = create_order_with_id("UserA", 1000, 1000000);
    manager.insert_after(u16::MAX, &order1).unwrap();

    // counter 应该是 1001
    // counter should be 1001
    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 1001);

    // 第二个订单: order_id = 500 (小于 counter)
    // Second order: order_id = 500 (less than counter)
    let order2 = create_order_with_id("UserB", 500, 2000000);
    manager.insert_after(0, &order2).unwrap();

    // counter 应该保持 1001 (不变)
    // counter should remain 1001 (unchanged)
    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 1001);

    // 第三个订单: order_id = 2000 (大于 counter)
    // Third order: order_id = 2000 (greater than counter)
    let order3 = create_order_with_id("UserC", 2000, 3000000);
    manager.insert_after(1, &order3).unwrap();

    // counter 应该更新为 2001
    // counter should update to 2001
    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 2001);

    // 验证所有 order_id 都是我们指定的,不是 counter 生成的
    // Verify all order_id are as we specified, not generated by counter
    assert_eq!(manager.get_order(0).unwrap().order_id, 1000);
    assert_eq!(manager.get_order(1).unwrap().order_id, 500);
    assert_eq!(manager.get_order(2).unwrap().order_id, 2000);

    cleanup_test_db(&temp_path);
    println!("✅ test_order_id_counter_is_metadata_only passed");
}

// ==================== 测试 5: insert_before 也使用事件 order_id ====================
// ==================== Test 5: insert_before also uses event order_id ====================

#[test]
fn test_insert_before_with_event_order_id() {
    // 测试: insert_before 方法也应该使用事件中的 order_id
    // Test: insert_before method should also use order_id from event
    let (manager, temp_path) = create_test_manager();
    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入第一个订单: order_id = 100
    // Insert first order: order_id = 100
    let order1 = create_order_with_id("UserA", 100, 1000000);
    manager.insert_after(u16::MAX, &order1).unwrap();

    // 在第一个订单之前插入: order_id = 50
    // Insert before first order: order_id = 50
    let order2 = create_order_with_id("UserB", 50, 2000000);
    let (index, assigned_id) = manager.insert_before(0, &order2).unwrap();

    // 验证返回的 order_id 是 50
    // Verify returned order_id is 50
    assert_eq!(assigned_id, 50);
    assert_eq!(index, 1);

    // 验证存储的 order_id 是 50
    // Verify stored order_id is 50
    let stored_order = manager.get_order(1).unwrap();
    assert_eq!(stored_order.order_id, 50);

    // 验证 counter 保持 101 (因为 50 < 101)
    // Verify counter remains 101 (because 50 < 101)
    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 101);

    cleanup_test_db(&temp_path);
    println!("✅ test_insert_before_with_event_order_id passed");
}

// ==================== 测试 6: insert_before 的 order_id = 0 也应报错 ====================
// ==================== Test 6: insert_before with order_id = 0 should also fail ====================

#[test]
fn test_insert_before_with_zero_order_id_should_fail() {
    // 测试: insert_before 方法对 order_id = 0 也应该报错
    // Test: insert_before method should also fail for order_id = 0
    let (manager, temp_path) = create_test_manager();
    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 先插入一个有效订单
    // First insert a valid order
    let order1 = create_order_with_id("UserA", 100, 1000000);
    manager.insert_after(u16::MAX, &order1).unwrap();

    // 尝试插入 order_id = 0 的订单
    // Try to insert order with order_id = 0
    let order2 = create_order_with_id("UserB", 0, 2000000);
    let result = manager.insert_before(0, &order2);

    // 验证应该失败
    // Verify should fail
    assert!(result.is_err(), "insert_before with order_id = 0 should fail");
    match result.unwrap_err() {
        OrderBookError::InvalidOrderId(_) => {}
        _ => panic!("expected InvalidOrderId error"),
    }

    // 验证 OrderBook 仍然只有 1 个订单
    // Verify OrderBook still has only 1 order
    let header = manager.load_header().unwrap();
    assert_eq!(header.total, 1);

    cleanup_test_db(&temp_path);
    println!("✅ test_insert_before_with_zero_order_id_should_fail passed");
}

// ==================== 测试 7: 大量随机 order_id 测试 ====================
// ==================== Test 7: Massive random order_id test ====================

#[test]
fn test_large_scale_random_order_ids() {
    // 测试: 插入大量随机 order_id,验证 counter 始终是最大值 + 1
    // Test: Insert many random order_ids, verify counter is always max + 1
    let (manager, temp_path) = create_test_manager();
    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 模拟的 order_id 序列 (非连续)
    // Simulated order_id sequence (non-sequential)
    let order_ids = vec![10, 50, 30, 100, 25, 200, 75, 150, 5, 300];
    let mut max_order_id = 0u64;

    for (i, &order_id) in order_ids.iter().enumerate() {
        let order = create_order_with_id(&format!("User{}", i), order_id, 1000000 + i as u128 * 100000);

        if i == 0 {
            manager.insert_after(u16::MAX, &order).unwrap();
        } else {
            manager.insert_after((i - 1) as u16, &order).unwrap();
        }

        // 更新最大值
        // Update max value
        if order_id > max_order_id {
            max_order_id = order_id;
        }

        // 验证 counter = max + 1
        // Verify counter = max + 1
        let header = manager.load_header().unwrap();
        assert_eq!(
            header.order_id_counter,
            max_order_id + 1,
            "counter should always be max_order_id + 1"
        );
    }

    // 验证所有订单的 order_id 都正确
    // Verify all orders' order_id are correct
    for (i, &expected_id) in order_ids.iter().enumerate() {
        let order = manager.get_order(i as u16).unwrap();
        assert_eq!(order.order_id, expected_id, "order {} should have order_id {}", i, expected_id);
    }

    // 最终 counter 应该是 301 (max 300 + 1)
    // Final counter should be 301 (max 300 + 1)
    let header = manager.load_header().unwrap();
    assert_eq!(header.order_id_counter, 301);
    assert_eq!(header.total, 10);

    cleanup_test_db(&temp_path);
    println!("✅ test_large_scale_random_order_ids passed");
}

// ==================== 测试 8: 验证与旧版本的不兼容性 ====================
// ==================== Test 8: Verify incompatibility with old version ====================

#[test]
fn test_old_behavior_no_longer_works() {
    // 测试: 验证旧的行为(order_id = 0)不再有效
    // Test: Verify old behavior (order_id = 0) no longer works
    let (manager, temp_path) = create_test_manager();
    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 旧代码会创建 order_id = 0 的订单
    // Old code would create orders with order_id = 0
    let old_style_order = create_test_order("UserA", 1000000); // order_id = 0

    // 现在应该失败
    // Should fail now
    let result = manager.insert_after(u16::MAX, &old_style_order);
    assert!(result.is_err(), "old-style order (order_id=0) should be rejected");

    cleanup_test_db(&temp_path);
    println!("✅ test_old_behavior_no_longer_works passed");
}
