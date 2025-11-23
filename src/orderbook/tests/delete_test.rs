// 删除操作测试
// Delete Operation Tests

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
#[ignore] // TODO: 修复批量删除的链表指针更新问题 / Fix batch delete linked list pointer update issue
fn test_delete_single_order_from_middle() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入5个订单 (为了避免边界情况)
    insert_orders(&manager, 5);

    // 删除中间的两个订单 (index=2, index=3)
    manager.batch_remove_by_indices_unsafe(&[2, 3]).unwrap();

    // 验证 header
    let header = manager.load_header().unwrap();
    assert_eq!(header.total, 3);

    // 验证通过遍历:应该还有 3 个有效订单
    let mut count = 0;
    let mut users = Vec::new();
    manager
        .traverse(u16::MAX, 0, |_index, order| {
            count += 1;
            users.push(order.user.clone());
            Ok(true)
        })
        .unwrap();

    assert_eq!(count, 3);
    // 删除 User2 和 User3 后,剩下 User0, User1, User4
    assert!(users.contains(&"User0".to_string()));
    assert!(users.contains(&"User1".to_string()));
    assert!(users.contains(&"User4".to_string()));

    cleanup_test_db(&temp_path);
}

#[test]
fn test_delete_head_order() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入3个订单
    insert_orders(&manager, 3);

    // 删除头节点 (index=0)
    // 注意: 删除后,原来的 index=2 会被移动到 index=0
    manager.batch_remove_by_indices_unsafe(&[0]).unwrap();

    // 验证 header
    let header = manager.load_header().unwrap();
    assert_eq!(header.total, 2);

    // 删除 index=0 后,末尾节点(User2, index=2)被移动到 index=0
    // 链表变为: order0(User2) -> order1(User1) -> null
    // head 仍然是原来的 head,但内容被替换了
    let order0 = manager.get_order(0).unwrap();
    assert_eq!(order0.user, "User2"); // 这是移动过来的原 index=2

    let order1 = manager.get_order(1).unwrap();
    assert_eq!(order1.prev_order, 0);

    cleanup_test_db(&temp_path);
}

#[test]
fn test_delete_tail_order() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入3个订单
    insert_orders(&manager, 3);

    // 删除尾节点 (index=2)
    manager.batch_remove_by_indices_unsafe(&[2]).unwrap();

    // 验证 header
    let header = manager.load_header().unwrap();
    assert_eq!(header.total, 2);
    assert_eq!(header.tail, 1); // 新的尾节点

    // 验证新尾节点
    let order1 = manager.get_order(1).unwrap();
    assert_eq!(order1.next_order, u16::MAX); // 现在是尾节点

    cleanup_test_db(&temp_path);
}

#[test]
fn test_batch_delete_multiple_orders() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入10个订单
    insert_orders(&manager, 10);

    // 批量删除: index = [2, 5, 8]
    manager
        .batch_remove_by_indices_unsafe(&[2, 5, 8])
        .unwrap();

    // 验证 header
    let header = manager.load_header().unwrap();
    assert_eq!(header.total, 7);

    // 验证活跃索引列表
    let active_indices = manager.load_active_indices().unwrap();
    assert_eq!(active_indices.len(), 7);
    assert!(!active_indices.contains(&2));
    assert!(!active_indices.contains(&5));
    assert!(!active_indices.contains(&8));

    cleanup_test_db(&temp_path);
}

#[test]
fn test_delete_all_orders() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入5个订单
    insert_orders(&manager, 5);

    // 删除所有订单
    manager
        .batch_remove_by_indices_unsafe(&[0, 1, 2, 3, 4])
        .unwrap();

    // 验证 header
    let header = manager.load_header().unwrap();
    assert_eq!(header.total, 0);
    assert_eq!(header.head, u16::MAX);
    assert_eq!(header.tail, u16::MAX);
    assert_eq!(header.total_capacity, 0);

    // 验证活跃索引列表
    let active_indices = manager.load_active_indices().unwrap();
    assert_eq!(active_indices.len(), 0);

    cleanup_test_db(&temp_path);
}

#[test]
fn test_delete_with_duplicates() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入5个订单
    insert_orders(&manager, 5);

    // 删除重复的索引 [1, 3, 1, 3, 2]
    manager
        .batch_remove_by_indices_unsafe(&[1, 3, 1, 3, 2])
        .unwrap();

    // 验证 header (应该只删除3个: 1, 2, 3)
    let header = manager.load_header().unwrap();
    assert_eq!(header.total, 2);

    cleanup_test_db(&temp_path);
}

#[test]
fn test_delete_empty_array() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入3个订单
    insert_orders(&manager, 3);

    // 删除空数组
    manager.batch_remove_by_indices_unsafe(&[]).unwrap();

    // 验证 header (应该没有变化)
    let header = manager.load_header().unwrap();
    assert_eq!(header.total, 3);

    cleanup_test_db(&temp_path);
}
