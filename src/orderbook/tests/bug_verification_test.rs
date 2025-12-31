// OrderBook Bug 验证测试
// OrderBook Bug Verification Tests
//
// 此文件专门用于验证文档中提到的已知 Bug
// This file is dedicated to verifying the known bugs mentioned in the documentation
//
// 参考文档: notes/OrderBook链表中间删除操作流程分析.md
// Reference: notes/OrderBook链表中间删除操作流程分析.md

use super::*;

/// Bug #1: WriteBatch 指针冲突 - 删除中间节点后链表指针不一致
/// Bug #1: WriteBatch pointer conflict - Linked list pointers inconsistent after deleting middle node
///
/// 测试场景:
/// Test scenario:
/// 创建链表: [A] ←→ [B] ←→ [C] ←→ [D] ←→ [E] ←→ [F] ←→ [G]
/// 删除 index=2 (Order C)
/// 期望: 链表正确重连,所有指针一致
/// Expected: Linked list correctly reconnected, all pointers consistent
#[test]
fn test_bug_1_writebatch_pointer_conflict() {
    let (manager, temp_path) = create_test_manager();
    manager
        .initialize("test_authority".to_string())
        .expect("Failed to initialize");

    println!("\n=== Bug #1 验证: WriteBatch 指针冲突 ===\n");

    // 1. 创建 7 个订单
    // 1. Create 7 orders
    let users = vec!["A", "B", "C", "D", "E", "F", "G"];
    let mut order_ids = vec![];

    for (i, user) in users.iter().enumerate() {
        let order = create_test_order(user, 1000000 + (i as u128 * 100000));
        let (index, order_id) = manager
            .insert_after(if i == 0 { u16::MAX } else { (i - 1) as u16 }, &order)
            .expect("Failed to insert");
        order_ids.push(order_id);
        println!("✅ 插入订单 {}: index={}, order_id={}", user, index, order_id);
    }

    // 2. 验证初始链表结构
    // 2. Verify initial linked list structure
    println!("\n--- 初始链表结构 ---");
    let header = manager.load_header().expect("Failed to load header");
    println!("Header: head={}, tail={}, total={}", header.head, header.tail, header.total);

    for i in 0..7 {
        let order = manager.get_order(i).expect("Failed to get order");
        println!(
            "index={}: user={}, prev={}, next={}, order_id={}",
            i,
            order.user,
            if order.prev_order == u16::MAX {
                "MAX".to_string()
            } else {
                order.prev_order.to_string()
            },
            if order.next_order == u16::MAX {
                "MAX".to_string()
            } else {
                order.next_order.to_string()
            },
            order.order_id
        );
    }

    // 3. 删除中间节点 index=2 (Order C)
    // 3. Delete middle node index=2 (Order C)
    println!("\n--- 删除 index=2 (Order C) ---");
    let result = manager.batch_remove_by_indices_unsafe(&[2], 1, 0, 0);

    match result {
        Ok(_) => println!("✅ 删除操作完成"),
        Err(e) => {
            println!("❌ 删除操作失败: {:?}", e);
            cleanup_test_db(&temp_path);
            panic!("Delete operation failed");
        }
    }

    // 4. 验证删除后的 Header
    // 4. Verify header after deletion
    println!("\n--- 删除后的 Header ---");
    let header_after = manager.load_header().expect("Failed to load header");
    println!(
        "Header: head={}, tail={}, total={}",
        header_after.head, header_after.tail, header_after.total
    );

    // Bug #2 验证: tail 指针是否正确
    // Bug #2 verification: Is tail pointer correct?
    println!("\n🔍 Bug #2 检查: tail 指针验证");
    if header_after.tail >= header_after.total {
        println!("❌ BUG 确认: tail={} 超出范围 (total={})", header_after.tail, header_after.total);
    } else {
        println!("✅ tail 指针在有效范围内: tail={}", header_after.tail);
    }

    // 5. 验证删除后的链表结构
    // 5. Verify linked list structure after deletion
    println!("\n--- 删除后的链表结构 ---");
    for i in 0..header_after.total {
        match manager.get_order(i) {
            Ok(order) => {
                println!(
                    "index={}: user={}, prev={}, next={}, order_id={}",
                    i,
                    order.user,
                    if order.prev_order == u16::MAX {
                        "MAX".to_string()
                    } else {
                        order.prev_order.to_string()
                    },
                    if order.next_order == u16::MAX {
                        "MAX".to_string()
                    } else {
                        order.next_order.to_string()
                    },
                    order.order_id
                );

                // Bug #1 验证: 检查指针是否有效
                // Bug #1 verification: Check if pointers are valid
                if order.prev_order != u16::MAX && order.prev_order >= header_after.total {
                    println!(
                        "❌ BUG 确认: index={} 的 prev_order={} 超出范围 (total={})",
                        i, order.prev_order, header_after.total
                    );
                }
                if order.next_order != u16::MAX && order.next_order >= header_after.total {
                    println!(
                        "❌ BUG 确认: index={} 的 next_order={} 超出范围 (total={})",
                        i, order.next_order, header_after.total
                    );
                }
            }
            Err(e) => {
                println!("❌ 读取 index={} 失败: {:?}", i, e);
            }
        }
    }

    // 6. 验证链表双向连接的一致性
    // 6. Verify linked list bidirectional connection consistency
    println!("\n--- 链表连接一致性验证 ---");
    let mut inconsistency_found = false;

    for i in 0..header_after.total {
        let order = manager.get_order(i).expect("Failed to get order");

        // 验证前驱节点的 next 是否指向当前节点
        // Verify if predecessor's next points to current node
        if order.prev_order != u16::MAX {
            if let Ok(prev_order) = manager.get_order(order.prev_order) {
                if prev_order.next_order != i {
                    println!(
                        "❌ BUG 确认: 指针不一致! index={} 的前驱 index={} 的 next={} (应该是 {})",
                        i, order.prev_order, prev_order.next_order, i
                    );
                    inconsistency_found = true;
                }
            }
        }

        // 验证后继节点的 prev 是否指向当前节点
        // Verify if successor's prev points to current node
        if order.next_order != u16::MAX {
            if let Ok(next_order) = manager.get_order(order.next_order) {
                if next_order.prev_order != i {
                    println!(
                        "❌ BUG 确认: 指针不一致! index={} 的后继 index={} 的 prev={} (应该是 {})",
                        i, order.next_order, next_order.prev_order, i
                    );
                    inconsistency_found = true;
                }
            }
        }
    }

    if !inconsistency_found {
        println!("✅ 所有链表指针一致");
    }

    // 7. 验证活跃索引列表
    // 7. Verify active indices list
    println!("\n--- Bug #3 检查: 活跃索引列表验证 ---");
    let active_indices = manager
        .load_active_indices()
        .expect("Failed to load active indices");
    println!("活跃索引: {:?}", active_indices);

    // Bug #3 验证: 检查是否包含无效索引
    // Bug #3 verification: Check if contains invalid indices
    let mut invalid_indices = vec![];
    for &idx in &active_indices {
        if idx >= header_after.total {
            invalid_indices.push(idx);
        }
    }

    if !invalid_indices.is_empty() {
        println!(
            "❌ BUG 确认: active_indices 包含无效索引: {:?} (total={})",
            invalid_indices, header_after.total
        );
    } else {
        println!("✅ active_indices 中所有索引都有效");
    }

    // 8. 尝试遍历整个链表
    // 8. Try to traverse entire linked list
    println!("\n--- 链表遍历测试 ---");
    let traverse_result = manager.traverse(u16::MAX, 0, |idx, order| {
        println!("遍历: index={}, user={}", idx, order.user);
        Ok(true)
    });

    match traverse_result {
        Ok(result) => {
            println!("✅ 遍历完成: processed={}, done={}", result.processed, result.done);
            if result.processed != header_after.total as u32 {
                println!(
                    "⚠️ 警告: 遍历数量 {} 与 total {} 不匹配",
                    result.processed, header_after.total
                );
            }
        }
        Err(e) => {
            println!("❌ BUG 确认: 遍历失败: {:?}", e);
        }
    }

    // 9. 总结
    // 9. Summary
    println!("\n=== Bug 验证总结 ===");
    println!("Bug #1 (指针冲突): {}", if inconsistency_found { "❌ 存在" } else { "✅ 未发现" });
    println!("Bug #2 (tail 错误): {}", if header_after.tail >= header_after.total { "❌ 存在" } else { "✅ 未发现" });
    println!("Bug #3 (无效索引): {}", if !invalid_indices.is_empty() { "❌ 存在" } else { "✅ 未发现" });

    cleanup_test_db(&temp_path);
}

/// Bug 验证 #2: 删除多个中间节点
/// Bug verification #2: Delete multiple middle nodes
///
/// 测试场景:
/// Test scenario:
/// 创建链表: [0] ←→ [1] ←→ [2] ←→ [3] ←→ [4] ←→ [5] ←→ [6] ←→ [7] ←→ [8] ←→ [9]
/// 删除 indices=[2, 5, 7]
/// 验证链表完整性
/// Verify linked list integrity
#[test]
fn test_bug_multiple_middle_deletions() {
    let (manager, temp_path) = create_test_manager();
    manager
        .initialize("test_authority".to_string())
        .expect("Failed to initialize");

    println!("\n=== Bug 验证: 删除多个中间节点 ===\n");

    // 1. 创建 10 个订单
    // 1. Create 10 orders
    for i in 0..10 {
        let order = create_test_order(&format!("User_{}", i), 1000000 + (i as u128 * 100000));
        let (index, order_id) = manager
            .insert_after(if i == 0 { u16::MAX } else { i - 1 }, &order)
            .expect("Failed to insert");
        println!("✅ 插入订单 {}: index={}, order_id={}", i, index, order_id);
    }

    // 2. 删除多个中间节点
    // 2. Delete multiple middle nodes
    let delete_indices = vec![2, 5, 7];
    println!("\n--- 删除 indices={:?} ---", delete_indices);

    let result = manager.batch_remove_by_indices_unsafe(&delete_indices, 1, 0, 0);

    match result {
        Ok(_) => println!("✅ 删除操作完成"),
        Err(e) => {
            println!("❌ 删除操作失败: {:?}", e);
            cleanup_test_db(&temp_path);
            panic!("Delete operation failed");
        }
    }

    // 3. 验证 Header
    // 3. Verify Header
    let header = manager.load_header().expect("Failed to load header");
    println!(
        "\nHeader: head={}, tail={}, total={}",
        header.head, header.tail, header.total
    );

    // 4. 检查所有节点
    // 4. Check all nodes
    println!("\n--- 剩余节点检查 ---");
    let mut pointer_errors = 0;
    let mut range_errors = 0;

    for i in 0..header.total {
        match manager.get_order(i) {
            Ok(order) => {
                println!(
                    "index={}: user={}, prev={}, next={}",
                    i,
                    order.user,
                    if order.prev_order == u16::MAX {
                        "MAX".to_string()
                    } else {
                        order.prev_order.to_string()
                    },
                    if order.next_order == u16::MAX {
                        "MAX".to_string()
                    } else {
                        order.next_order.to_string()
                    }
                );

                // 检查指针范围
                // Check pointer ranges
                if order.prev_order != u16::MAX && order.prev_order >= header.total {
                    range_errors += 1;
                    println!("  ❌ prev_order 超出范围!");
                }
                if order.next_order != u16::MAX && order.next_order >= header.total {
                    range_errors += 1;
                    println!("  ❌ next_order 超出范围!");
                }

                // 检查指针一致性
                // Check pointer consistency
                if order.prev_order != u16::MAX {
                    if let Ok(prev) = manager.get_order(order.prev_order) {
                        if prev.next_order != i {
                            pointer_errors += 1;
                            println!("  ❌ 前驱指针不一致!");
                        }
                    }
                }

                if order.next_order != u16::MAX {
                    if let Ok(next) = manager.get_order(order.next_order) {
                        if next.prev_order != i {
                            pointer_errors += 1;
                            println!("  ❌ 后继指针不一致!");
                        }
                    }
                }
            }
            Err(e) => {
                println!("❌ 读取 index={} 失败: {:?}", i, e);
            }
        }
    }

    // 5. 尝试遍历
    // 5. Try traversal
    println!("\n--- 遍历测试 ---");
    let traverse_result = manager.traverse(u16::MAX, 0, |idx, order| {
        println!("  -> index={}, user={}", idx, order.user);
        Ok(true)
    });

    let traverse_success = match traverse_result {
        Ok(result) => {
            println!("✅ 遍历成功: processed={}", result.processed);
            result.processed == header.total as u32
        }
        Err(e) => {
            println!("❌ 遍历失败: {:?}", e);
            false
        }
    };

    // 6. 总结
    // 6. Summary
    println!("\n=== 验证结果 ===");
    println!("指针范围错误: {}", range_errors);
    println!("指针一致性错误: {}", pointer_errors);
    println!("遍历测试: {}", if traverse_success { "✅ 通过" } else { "❌ 失败" });

    if range_errors > 0 || pointer_errors > 0 || !traverse_success {
        println!("\n❌ 发现 BUG!");
    } else {
        println!("\n✅ 未发现 BUG (或者测试场景未触发)");
    }

    cleanup_test_db(&temp_path);
}

/// Bug 验证 #3: 连续删除测试
/// Bug verification #3: Sequential deletion test
///
/// 从头到尾连续删除中间节点,观察累积错误
/// Sequentially delete middle nodes from head to tail, observe accumulated errors
#[test]
fn test_bug_sequential_deletions() {
    let (manager, temp_path) = create_test_manager();
    manager
        .initialize("test_authority".to_string())
        .expect("Failed to initialize");

    println!("\n=== Bug 验证: 连续删除测试 ===\n");

    // 1. 创建 8 个订单
    // 1. Create 8 orders
    for i in 0..8 {
        let order = create_test_order(&format!("Order_{}", i), 1000000 + (i as u128 * 100000));
        manager
            .insert_after(if i == 0 { u16::MAX } else { i - 1 }, &order)
            .expect("Failed to insert");
    }

    println!("✅ 初始链表: 8 个订单 (index 0-7)\n");

    // 2. 连续删除中间节点
    // 2. Sequentially delete middle nodes
    let delete_sequence = vec![
        vec![3],    // 删除 index=3
        vec![2],    // 删除 index=2 (现在的 index=2 是原来的 index=7)
        vec![1],    // 删除 index=1
    ];

    for (round, indices) in delete_sequence.iter().enumerate() {
        println!("--- Round {}: 删除 {:?} ---", round + 1, indices);

        let result = manager.batch_remove_by_indices_unsafe(indices, 1, 0, 0);

        match result {
            Ok(_) => {
                let header = manager.load_header().expect("Failed to load header");
                println!("✅ 删除成功, total={}", header.total);

                // 尝试遍历
                let traverse_result = manager.traverse(u16::MAX, 0, |idx, order| {
                    print!("{} ", order.user);
                    Ok(true)
                });

                match traverse_result {
                    Ok(result) => {
                        println!("\n✅ 遍历成功: {} 个节点", result.processed);
                    }
                    Err(e) => {
                        println!("\n❌ 遍历失败: {:?}", e);
                        println!("💥 BUG 触发在 Round {}", round + 1);
                        break;
                    }
                }
            }
            Err(e) => {
                println!("❌ 删除失败: {:?}", e);
                break;
            }
        }
        println!();
    }

    cleanup_test_db(&temp_path);
}

/// Bug 验证 #4: tail 节点追踪测试
/// Bug verification #4: Tail node tracking test
///
/// 专门测试 tail 指针在各种删除场景下是否正确更新
/// Specifically test if tail pointer is correctly updated in various deletion scenarios
#[test]
fn test_bug_tail_pointer_tracking() {
    let (manager, temp_path) = create_test_manager();
    manager
        .initialize("test_authority".to_string())
        .expect("Failed to initialize");

    println!("\n=== Bug 验证: Tail 指针追踪 ===\n");

    // 1. 创建 5 个订单
    // 1. Create 5 orders
    for i in 0..5 {
        let order = create_test_order(&format!("Node_{}", i), 1000000 + (i as u128 * 100000));
        manager
            .insert_after(if i == 0 { u16::MAX } else { i - 1 }, &order)
            .expect("Failed to insert");
    }

    let initial_header = manager.load_header().expect("Failed to load header");
    println!("初始状态: head={}, tail={}, total={}",
             initial_header.head, initial_header.tail, initial_header.total);

    // 2. 测试场景 1: 删除中间节点
    // 2. Test scenario 1: Delete middle node
    println!("\n--- 场景 1: 删除中间节点 index=2 ---");
    manager.batch_remove_by_indices_unsafe(&[2], 1, 0, 0).expect("Failed to delete");

    let header1 = manager.load_header().expect("Failed to load header");
    println!("删除后: head={}, tail={}, total={}", header1.head, header1.tail, header1.total);

    // 验证 tail 是否有效
    // Verify if tail is valid
    if header1.tail >= header1.total {
        println!("❌ BUG: tail={} 超出范围 (total={})", header1.tail, header1.total);
    } else {
        // 验证 tail 节点是否真的是尾节点
        // Verify if tail node is really the tail node
        match manager.get_order(header1.tail) {
            Ok(tail_order) => {
                println!("tail 节点: user={}, next={}",
                         tail_order.user,
                         if tail_order.next_order == u16::MAX { "MAX" } else { &tail_order.next_order.to_string() });

                if tail_order.next_order != u16::MAX {
                    println!("❌ BUG: tail 节点的 next 不是 MAX!");
                } else {
                    println!("✅ tail 节点正确");
                }
            }
            Err(e) => {
                println!("❌ BUG: 无法读取 tail 节点: {:?}", e);
            }
        }
    }

    // 3. 测试场景 2: 从尾部回溯
    // 3. Test scenario 2: Backtrack from tail
    println!("\n--- 场景 2: 从尾部回溯到头部 ---");
    if header1.tail < header1.total {
        let mut current = header1.tail;
        let mut path = vec![];
        let mut visited = std::collections::HashSet::new();

        loop {
            if visited.contains(&current) {
                println!("❌ BUG: 检测到循环引用!");
                break;
            }
            visited.insert(current);

            match manager.get_order(current) {
                Ok(order) => {
                    path.push(order.user.clone());

                    if order.prev_order == u16::MAX {
                        println!("✅ 回溯路径: {:?}", path.iter().rev().collect::<Vec<_>>());

                        if current != header1.head {
                            println!("❌ BUG: 回溯到的头节点 {} 与 header.head={} 不一致",
                                     current, header1.head);
                        }
                        break;
                    }

                    if order.prev_order >= header1.total {
                        println!("❌ BUG: prev_order={} 超出范围!", order.prev_order);
                        break;
                    }

                    current = order.prev_order;
                }
                Err(e) => {
                    println!("❌ BUG: 回溯失败: {:?}", e);
                    break;
                }
            }

            if path.len() > header1.total as usize {
                println!("❌ BUG: 回溯路径过长,可能存在循环!");
                break;
            }
        }
    }

    cleanup_test_db(&temp_path);
}

/// Bug 验证 #5: head 节点移动测试
/// Bug verification #5: Head node move test
///
/// 专门测试当 head 节点被移动到其他位置时,header.head 是否正确更新
/// Specifically test if header.head is correctly updated when head node is moved to another position
#[test]
fn test_bug_head_pointer_move() {
    let (manager, temp_path) = create_test_manager();
    manager
        .initialize("test_authority".to_string())
        .expect("Failed to initialize");

    println!("\n=== Bug 验证 #5: Head 节点移动测试 ===\n");

    // 1. 创建 4 个订单: [0] -> [1] -> [2] -> [3]
    // 1. Create 4 orders: [0] -> [1] -> [2] -> [3]
    for i in 0..4 {
        let order = create_test_order(&format!("Node_{}", i), 1000000 + (i as u128 * 100000));
        manager
            .insert_after(if i == 0 { u16::MAX } else { i - 1 }, &order)
            .expect("Failed to insert");
    }

    let initial_header = manager.load_header().expect("Failed to load header");
    println!("初始状态: head={}, tail={}, total={}",
             initial_header.head, initial_header.tail, initial_header.total);
    assert_eq!(initial_header.head, 0);
    assert_eq!(initial_header.tail, 3);
    assert_eq!(initial_header.total, 4);

    // 2. 删除 index=0 (head 节点)
    // 2. Delete index=0 (head node)
    // 这会导致:
    // - index=3 (原 tail) 被移动到 index=0
    // - header.head 应该更新为指向原来 index=1 的节点
    // - header.tail 应该更新为 index=0 (原 index=3 移动到此)
    println!("\n--- 场景: 删除 head 节点 index=0 ---");
    manager.batch_remove_by_indices_unsafe(&[0], 1, 0, 0).expect("Failed to delete");

    let header_after = manager.load_header().expect("Failed to load header");
    println!("删除后: head={}, tail={}, total={}",
             header_after.head, header_after.tail, header_after.total);

    // 3. 验证遍历是否成功
    // 3. Verify traversal succeeds
    println!("\n--- 验证遍历 ---");
    let mut visited_orders = Vec::new();
    let traverse_result = manager.traverse(u16::MAX, 0, |idx, order| {
        visited_orders.push((idx, order.user.clone()));
        println!("  index={}, user={}", idx, order.user);
        Ok(true)
    });

    match traverse_result {
        Ok(result) => {
            println!("✅ 遍历成功: {} 个节点 (预期 3 个)", result.processed);
            assert_eq!(result.processed, 3, "应该遍历 3 个节点");
            assert!(result.done, "遍历应该完成");
        }
        Err(e) => {
            panic!("❌ BUG 触发: 遍历失败 - {:?}", e);
        }
    }

    // 4. 验证 head 和 tail 指针有效性
    // 4. Verify head and tail pointers validity
    assert!(header_after.head < header_after.total,
            "head={} 应该小于 total={}", header_after.head, header_after.total);
    assert!(header_after.tail < header_after.total,
            "tail={} 应该小于 total={}", header_after.tail, header_after.total);

    // 5. 验证 head 节点的 prev 是 MAX
    // 5. Verify head node's prev is MAX
    let head_order = manager.get_order(header_after.head).expect("Failed to get head order");
    assert_eq!(head_order.prev_order, u16::MAX,
               "head 节点的 prev_order 应该是 MAX, 实际是 {}", head_order.prev_order);

    // 6. 验证 tail 节点的 next 是 MAX
    // 6. Verify tail node's next is MAX
    let tail_order = manager.get_order(header_after.tail).expect("Failed to get tail order");
    assert_eq!(tail_order.next_order, u16::MAX,
               "tail 节点的 next_order 应该是 MAX, 实际是 {}", tail_order.next_order);

    println!("\n✅ 所有验证通过!");

    cleanup_test_db(&temp_path);
}

/// Bug 验证 #6: tail 节点是 head 时的移动测试
/// Bug verification #6: Move test when tail node is head
///
/// 测试当链表只有一个节点被移动时,head 和 tail 是否都正确更新
/// Test if both head and tail are correctly updated when the only node is moved
#[test]
fn test_bug_head_is_tail_move() {
    let (manager, temp_path) = create_test_manager();
    manager
        .initialize("test_authority".to_string())
        .expect("Failed to initialize");

    println!("\n=== Bug 验证 #6: 两节点链表删除头节点 ===\n");

    // 1. 创建 2 个订单: [0] -> [1]
    // 1. Create 2 orders: [0] -> [1]
    for i in 0..2 {
        let order = create_test_order(&format!("Node_{}", i), 1000000 + (i as u128 * 100000));
        manager
            .insert_after(if i == 0 { u16::MAX } else { i - 1 }, &order)
            .expect("Failed to insert");
    }

    let initial_header = manager.load_header().expect("Failed to load header");
    println!("初始状态: head={}, tail={}, total={}",
             initial_header.head, initial_header.tail, initial_header.total);

    // 2. 删除 index=0 (head 节点)
    // 2. Delete index=0 (head node)
    // index=1 会被移动到 index=0,然后它既是 head 也是 tail
    println!("\n--- 场景: 删除 head 节点 ---");
    manager.batch_remove_by_indices_unsafe(&[0], 1, 0, 0).expect("Failed to delete");

    let header_after = manager.load_header().expect("Failed to load header");
    println!("删除后: head={}, tail={}, total={}",
             header_after.head, header_after.tail, header_after.total);

    // 3. 验证遍历
    // 3. Verify traversal
    let mut count = 0;
    let traverse_result = manager.traverse(u16::MAX, 0, |idx, order| {
        println!("  index={}, user={}", idx, order.user);
        count += 1;
        Ok(true)
    });

    match traverse_result {
        Ok(result) => {
            println!("✅ 遍历成功: {} 个节点 (预期 1 个)", result.processed);
            assert_eq!(result.processed, 1, "应该遍历 1 个节点");
        }
        Err(e) => {
            panic!("❌ BUG 触发: 遍历失败 - {:?}", e);
        }
    }

    // 4. head 和 tail 应该相同且有效
    // 4. head and tail should be same and valid
    assert_eq!(header_after.head, header_after.tail,
               "只有一个节点时 head 和 tail 应该相同");
    assert!(header_after.head < header_after.total,
            "head 应该有效");

    println!("\n✅ 所有验证通过!");

    cleanup_test_db(&temp_path);
}
