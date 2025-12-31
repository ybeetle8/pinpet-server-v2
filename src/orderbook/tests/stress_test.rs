// 压力测试 - 5000个订单
// Stress Test - 5000 Orders

use super::*;
use std::time::Instant;

#[test]
#[ignore] // 默认忽略,需要时手动运行: cargo test stress_test -- --ignored
fn test_insert_5000_orders() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    let start = Instant::now();

    // 插入5000个订单
    for i in 0..5000 {
        let order = create_test_order(&format!("User{}", i), (i as u128 + 1) * 1000000);
        if i == 0 {
            manager.insert_after(u16::MAX, &order).unwrap();
        } else {
            manager.insert_after((i - 1) as u16, &order).unwrap();
        }

        if (i + 1) % 1000 == 0 {
            println!("✅ Inserted {} orders", i + 1);
        }
    }

    let elapsed = start.elapsed();
    println!(
        "✅ Inserted 5000 orders in {:.2}s ({:.2} orders/sec)",
        elapsed.as_secs_f64(),
        5000.0 / elapsed.as_secs_f64()
    );

    // 验证 header
    let header = manager.load_header().unwrap();
    assert_eq!(header.total, 5000);
    assert_eq!(header.head, 0);
    assert_eq!(header.tail, 4999);

    cleanup_test_db(&temp_path);
}

#[test]
#[ignore]
fn test_traverse_5000_orders() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入5000个订单
    for i in 0..5000 {
        let order = create_test_order(&format!("User{}", i), (i as u128 + 1) * 1000000);
        if i == 0 {
            manager.insert_after(u16::MAX, &order).unwrap();
        } else {
            manager.insert_after((i - 1) as u16, &order).unwrap();
        }
    }

    let start = Instant::now();

    // 遍历所有订单
    let mut count = 0;
    let result = manager
        .traverse(u16::MAX, 0, |_index, _order| {
            count += 1;
            Ok(true)
        })
        .unwrap();

    let elapsed = start.elapsed();
    println!(
        "✅ Traversed 5000 orders in {:.2}ms ({:.2} orders/ms)",
        elapsed.as_millis(),
        5000.0 / elapsed.as_millis() as f64
    );

    assert_eq!(count, 5000);
    assert_eq!(result.processed, 5000);
    assert_eq!(result.done, true);

    cleanup_test_db(&temp_path);
}

#[test]
#[ignore]
fn test_batch_delete_1000_orders_from_5000() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入5000个订单
    for i in 0..5000 {
        let order = create_test_order(&format!("User{}", i), (i as u128 + 1) * 1000000);
        if i == 0 {
            manager.insert_after(u16::MAX, &order).unwrap();
        } else {
            manager.insert_after((i - 1) as u16, &order).unwrap();
        }
    }

    // 准备要删除的索引: 每隔5个删除一个 (0, 5, 10, 15, ...)
    let to_delete: Vec<u16> = (0..5000).filter(|i| i % 5 == 0).collect();
    println!("准备删除 {} 个订单", to_delete.len());

    let start = Instant::now();

    // 批量删除
    manager.batch_remove_by_indices_unsafe(&to_delete, 1, 0, 0).unwrap();

    let elapsed = start.elapsed();
    println!(
        "✅ Deleted {} orders in {:.2}s ({:.2} orders/sec)",
        to_delete.len(),
        elapsed.as_secs_f64(),
        to_delete.len() as f64 / elapsed.as_secs_f64()
    );

    // 验证 header
    let header = manager.load_header().unwrap();
    assert_eq!(header.total, 5000 - to_delete.len() as u16);

    cleanup_test_db(&temp_path);
}

#[test]
#[ignore]
fn test_update_1000_orders_from_5000() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    // 插入5000个订单
    for i in 0..5000 {
        let order = create_test_order(&format!("User{}", i), (i as u128 + 1) * 1000000);
        if i == 0 {
            manager.insert_after(u16::MAX, &order).unwrap();
        } else {
            manager.insert_after((i - 1) as u16, &order).unwrap();
        }
    }

    use crate::orderbook::MarginOrderUpdateData;

    let start = Instant::now();

    // 更新前1000个订单
    for i in 0..1000 {
        let update_data = MarginOrderUpdateData {
            margin_sol_amount: Some(90000000),
            realized_sol_amount: Some(5000000),
            ..Default::default()
        };

        manager.update_order(i as u16, i, &update_data).unwrap();
    }

    let elapsed = start.elapsed();
    println!(
        "✅ Updated 1000 orders in {:.2}s ({:.2} orders/sec)",
        elapsed.as_secs_f64(),
        1000.0 / elapsed.as_secs_f64()
    );

    // 验证更新
    for i in 0..1000 {
        let order = manager.get_order(i as u16).unwrap();
        assert_eq!(order.margin_sol_amount, 90000000);
        assert_eq!(order.realized_sol_amount, 5000000);
    }

    cleanup_test_db(&temp_path);
}

#[test]
#[ignore]
fn test_mixed_operations_stress() {
    let (manager, temp_path) = create_test_manager();

    let authority = "9B5X6wrjJVcXHnbPfZ8wP4k5m9n2q1r7t3u2v5w8x1y".to_string();
    manager.initialize(authority).unwrap();

    use crate::orderbook::MarginOrderUpdateData;

    let start = Instant::now();

    // 1. 插入1000个订单
    println!("📝 步骤1: 插入1000个订单...");
    for i in 0..1000 {
        let order = create_test_order(&format!("User{}", i), (i as u128 + 1) * 1000000);
        if i == 0 {
            manager.insert_after(u16::MAX, &order).unwrap();
        } else {
            manager.insert_after((i - 1) as u16, &order).unwrap();
        }
    }

    // 2. 更新前500个
    println!("📝 步骤2: 更新前500个订单...");
    for i in 0..500 {
        let update_data = MarginOrderUpdateData {
            margin_sol_amount: Some(90000000),
            ..Default::default()
        };
        manager.update_order(i as u16, i, &update_data).unwrap();
    }

    // 3. 删除每隔10个的订单
    println!("📝 步骤3: 删除100个订单...");
    let to_delete: Vec<u16> = (0..1000).filter(|i| i % 10 == 0).collect();
    manager.batch_remove_by_indices_unsafe(&to_delete, 1, 0, 0).unwrap();

    // 4. 再插入500个
    println!("📝 步骤4: 再插入500个订单...");
    for i in 0..500 {
        let order = create_test_order(&format!("NewUser{}", i), (i as u128 + 1) * 2000000);
        let header = manager.load_header().unwrap();
        if header.total == 0 {
            manager.insert_after(u16::MAX, &order).unwrap();
        } else {
            manager.insert_after(header.tail, &order).unwrap();
        }
    }

    // 5. 遍历所有订单
    println!("📝 步骤5: 遍历所有订单...");
    let mut count = 0;
    manager
        .traverse(u16::MAX, 0, |_index, _order| {
            count += 1;
            Ok(true)
        })
        .unwrap();

    let elapsed = start.elapsed();
    println!("✅ 混合操作完成! 耗时: {:.2}s", elapsed.as_secs_f64());
    println!("   - 最终订单数: {}", count);

    // 验证最终状态
    let header = manager.load_header().unwrap();
    assert_eq!(header.total, count as u16);

    cleanup_test_db(&temp_path);
}
