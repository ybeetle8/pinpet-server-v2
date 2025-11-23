// 遍历操作测试
// Traverse Operation Tests

use super::*;

/// 辅助函数: 插入多个订单
fn insert_orders(manager: &OrderBookDBManager, count: usize) {
    for i in 0..count {
        let order = create_test_order(&format!("User{}", i), (i as u128 + 1) * 1000000);
        if i == 0 {
            manager.insert_after(u16::MAX, &order).unwrap();
        } else {
            manager.insert_after((i - 1) as u16, &order).unwrap();
        }
    }
}

#[test]
fn test_traverse_all_orders() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入5个订单
    insert_orders(&manager, 5);

    // 遍历所有订单
    let mut count = 0;
    let result = manager
        .traverse(u16::MAX, 0, |index, order| {
            assert_eq!(order.user, format!("User{}", index));
            count += 1;
            Ok(true)
        })
        .unwrap();

    assert_eq!(count, 5);
    assert_eq!(result.processed, 5);
    assert_eq!(result.done, true);

    cleanup_test_db(&temp_path);
}

#[test]
fn test_traverse_with_limit() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入10个订单
    insert_orders(&manager, 10);

    // 只遍历前3个
    let result = manager
        .traverse(u16::MAX, 3, |_index, _order| Ok(true))
        .unwrap();

    assert_eq!(result.processed, 3);
    assert_eq!(result.done, false);
    assert_eq!(result.next, 3); // 下一个待处理的索引

    cleanup_test_db(&temp_path);
}

#[test]
fn test_traverse_resume() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入10个订单
    insert_orders(&manager, 10);

    // 第一次遍历: 处理前5个
    let result1 = manager
        .traverse(u16::MAX, 5, |_index, _order| Ok(true))
        .unwrap();

    assert_eq!(result1.processed, 5);
    assert_eq!(result1.next, 5);

    // 第二次遍历: 从上次停止的地方继续
    let result2 = manager
        .traverse(result1.next, 0, |_index, _order| Ok(true))
        .unwrap();

    assert_eq!(result2.processed, 5);
    assert_eq!(result2.done, true);

    cleanup_test_db(&temp_path);
}

#[test]
fn test_traverse_early_stop() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入10个订单
    insert_orders(&manager, 10);

    // 遍历,遇到 User3 时停止
    let mut count = 0;
    let result = manager
        .traverse(u16::MAX, 0, |_index, order| {
            count += 1;
            if order.user == "User3" {
                Ok(false) // 停止遍历
            } else {
                Ok(true)
            }
        })
        .unwrap();

    assert_eq!(count, 4); // 遍历了 User0, User1, User2, User3
    assert_eq!(result.processed, 4);
    assert_eq!(result.done, false);

    cleanup_test_db(&temp_path);
}

#[test]
fn test_traverse_empty_orderbook() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 不插入任何订单,直接遍历
    let result = manager
        .traverse(u16::MAX, 0, |_index, _order| Ok(true))
        .unwrap();

    assert_eq!(result.processed, 0);
    assert_eq!(result.done, true);

    cleanup_test_db(&temp_path);
}

#[test]
fn test_get_insert_neighbors_empty() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 空链表
    let (prev, next) = manager.get_insert_neighbors(u16::MAX).unwrap();
    assert_eq!(prev, None);
    assert_eq!(next, None);

    cleanup_test_db(&temp_path);
}

#[test]
fn test_get_insert_neighbors_at_head() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入3个订单
    insert_orders(&manager, 3);

    // 插入到头部
    let (prev, next) = manager.get_insert_neighbors(u16::MAX).unwrap();
    assert_eq!(prev, None);
    assert_eq!(next, Some(0)); // 当前头部是0

    cleanup_test_db(&temp_path);
}

#[test]
fn test_get_insert_neighbors_at_tail() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入3个订单
    insert_orders(&manager, 3);

    // 插入到尾部之后 (在 index=2 之后)
    let (prev, next) = manager.get_insert_neighbors(2).unwrap();
    assert_eq!(prev, Some(2));
    assert_eq!(next, None); // 当前尾部是2

    cleanup_test_db(&temp_path);
}

#[test]
fn test_get_insert_neighbors_in_middle() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入3个订单
    insert_orders(&manager, 3);

    // 插入到中间 (在 index=1 之后)
    let (prev, next) = manager.get_insert_neighbors(1).unwrap();
    assert_eq!(prev, Some(1));
    assert_eq!(next, Some(2)); // index=1 的下一个是 index=2

    cleanup_test_db(&temp_path);
}

#[test]
fn test_get_all_active_orders() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入5个订单
    insert_orders(&manager, 5);

    // 获取所有活跃订单
    let active_orders = manager.get_all_active_orders().unwrap();
    assert_eq!(active_orders.len(), 5);

    // 验证每个订单
    for (index, order) in active_orders {
        assert_eq!(order.user, format!("User{}", index));
    }

    cleanup_test_db(&temp_path);
}

#[test]
fn test_get_order_by_id() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入5个订单
    insert_orders(&manager, 5);

    // 通过 order_id 获取订单
    for i in 0..5 {
        let order = manager.get_order_by_id(i).unwrap();
        assert_eq!(order.order_id, i);
        assert_eq!(order.user, format!("User{}", i));
    }

    cleanup_test_db(&temp_path);
}
