// 插入操作测试
// Insert Operation Tests

use super::*;

#[test]
fn test_initialize_orderbook() {
    let (manager, temp_path) = create_test_manager();

    // 初始化 OrderBook
    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority.clone()).unwrap();

    // 验证 header
    let header = manager.load_header().unwrap();
    assert_eq!(header.version, OrderBookHeader::CURRENT_VERSION);
    assert_eq!(header.order_type, 1); // dn = 做多
    assert_eq!(header.authority, authority);
    assert_eq!(header.total, 0);
    assert_eq!(header.head, u16::MAX);
    assert_eq!(header.tail, u16::MAX);

    cleanup_test_db(&temp_path);
}

#[test]
fn test_insert_first_order() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入第一个订单
    let order1 = create_test_order("UserA", 1000000);
    let (index, order_id) = manager.insert_after(u16::MAX, &order1).unwrap();

    assert_eq!(index, 0);
    assert_eq!(order_id, 0);

    // 验证 header
    let header = manager.load_header().unwrap();
    assert_eq!(header.total, 1);
    assert_eq!(header.head, 0);
    assert_eq!(header.tail, 0);
    assert_eq!(header.order_id_counter, 1);

    // 验证订单
    let saved_order = manager.get_order(0).unwrap();
    assert_eq!(saved_order.user, "UserA");
    assert_eq!(saved_order.order_id, 0);
    assert_eq!(saved_order.prev_order, u16::MAX);
    assert_eq!(saved_order.next_order, u16::MAX);
    assert_eq!(saved_order.version, 1);

    cleanup_test_db(&temp_path);
}

#[test]
fn test_insert_after_tail() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入第一个订单
    let order1 = create_test_order("UserA", 1000000);
    let (index1, _order_id1) = manager.insert_after(u16::MAX, &order1).unwrap();

    // 插入第二个订单(在尾部之后)
    let order2 = create_test_order("UserB", 2000000);
    let (index2, order_id2) = manager.insert_after(index1, &order2).unwrap();

    assert_eq!(index2, 1);
    assert_eq!(order_id2, 1);

    // 验证 header
    let header = manager.load_header().unwrap();
    assert_eq!(header.total, 2);
    assert_eq!(header.head, 0);
    assert_eq!(header.tail, 1);

    // 验证第一个订单
    let saved_order1 = manager.get_order(0).unwrap();
    assert_eq!(saved_order1.prev_order, u16::MAX);
    assert_eq!(saved_order1.next_order, 1);

    // 验证第二个订单
    let saved_order2 = manager.get_order(1).unwrap();
    assert_eq!(saved_order2.prev_order, 0);
    assert_eq!(saved_order2.next_order, u16::MAX);

    cleanup_test_db(&temp_path);
}

#[test]
fn test_insert_before_head() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入第一个订单
    let order1 = create_test_order("UserA", 1000000);
    let (index1, _) = manager.insert_after(u16::MAX, &order1).unwrap();

    // 在头部之前插入
    let order2 = create_test_order("UserB", 500000);
    let (index2, order_id2) = manager.insert_before(index1, &order2).unwrap();

    assert_eq!(index2, 1);
    assert_eq!(order_id2, 1);

    // 验证 header
    let header = manager.load_header().unwrap();
    assert_eq!(header.total, 2);
    assert_eq!(header.head, 1); // 新的头部
    assert_eq!(header.tail, 0);

    // 验证新头部
    let saved_order2 = manager.get_order(1).unwrap();
    assert_eq!(saved_order2.prev_order, u16::MAX);
    assert_eq!(saved_order2.next_order, 0);

    // 验证原头部
    let saved_order1 = manager.get_order(0).unwrap();
    assert_eq!(saved_order1.prev_order, 1);
    assert_eq!(saved_order1.next_order, u16::MAX);

    cleanup_test_db(&temp_path);
}

#[test]
fn test_insert_multiple_orders() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入5个订单
    for i in 0..5 {
        let order = create_test_order(&format!("User{}", i), (i as u128 + 1) * 1000000);
        if i == 0 {
            manager.insert_after(u16::MAX, &order).unwrap();
        } else {
            manager.insert_after((i - 1) as u16, &order).unwrap();
        }
    }

    // 验证 header
    let header = manager.load_header().unwrap();
    assert_eq!(header.total, 5);
    assert_eq!(header.head, 0);
    assert_eq!(header.tail, 4);

    // 验证链表完整性
    let mut current = 0;
    for i in 0..5 {
        let order = manager.get_order(current).unwrap();
        assert_eq!(order.user, format!("User{}", i));

        if i == 0 {
            assert_eq!(order.prev_order, u16::MAX);
        } else {
            assert_eq!(order.prev_order, current - 1);
        }

        if i == 4 {
            assert_eq!(order.next_order, u16::MAX);
        } else {
            assert_eq!(order.next_order, current + 1);
        }

        current += 1;
    }

    cleanup_test_db(&temp_path);
}
