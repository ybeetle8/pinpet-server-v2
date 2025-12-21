# OrderBook 小概率误删问题排查 (GPT)

## 背景
- 实际运行中观测到: 大量交易(>10万笔)后偶尔会出现“原本应该删除 `order_id=100`，结果被删的是 `order_id=55`”的错误。
- 该系统的订单簿实现位于 `src/orderbook`, RocksDB 中每个槽位以物理索引(`index`)标识，并维护双向链表(`prev_order`/`next_order`)来表示实际顺序。
- 合约事件(`BuySellEvent`/`LongShortEvent` 等)给出的 `liquidate_indices` 也是基于槽位索引。任何索引和真实订单之间的偏差都会直接导致误删。

## 关键链路概览
- 批量删除的核心逻辑在 `src/orderbook/manager.rs:715-973` (`batch_remove_by_indices_unsafe`)，通过“将尾部槽位搬到被删位置”的方式保持数组紧凑。
- 对外暴露给事件处理器的接口是 `batch_remove_by_indices_unsafe_with_info` (`src/orderbook/manager.rs:995-1065`)，先读出待删订单信息，再调用上面的批量删除函数。
- 事件处理器 `StorageEventHandler` (`src/solana/storage_handler.rs:43-160, 320-420`) 会在 `tokio::task::spawn_blocking` 中并行处理多个链上事件，并在 `handle_*_event` 中调用 `batch_remove_by_indices_unsafe_with_info` 来完成实际清算。

## 多角度分析

### 1. 物理索引在删除时会被重排
- 在 `batch_remove_by_indices_unsafe` 中，删除任何 `remove_index` 且 `remove_index < virtual_tail` 时，会把当前“末尾槽位”(初始为 `old_total-1`)搬到该位置(`src/orderbook/manager.rs:819-894`)。
- 这意味着**删除过程中任意时刻，未删除订单的索引都可能被重写**。如果有人在删除开始前记住了“第 i 个槽位是哪张订单”，而删除期间又插入/删除了别的订单，那么删除结束时“第 i 个槽位”指向的对象很可能已经变化。
- 因此，只要“记下索引”和“真正执行删除”之间存在时间窗口，就有可能依据旧索引删到新对象。

### 2. `batch_remove_by_indices_unsafe_with_info` 的 TOCTOU 竞态
- 该函数为了返回 `RemovedOrderInfo`，分两步执行：
  1. 获取 `operation_lock`，通过 `get_order(index)` 将待删订单信息推入 `removed_orders`，然后**立即释放锁**(`src/orderbook/manager.rs:1007-1058`)；
  2. 在锁已释放的情况下再调用 `batch_remove_by_indices_unsafe` (`src/orderbook/manager.rs:1060-1062`)，该函数内部会重新尝试获取同一把锁然后真正删除。
- 这是一种典型的 TOCTOU（time-of-check vs time-of-use）模式：在步骤(1)和步骤(2)之间，其他线程可以拿到锁，对同一 `OrderBookDBManager` 做插入或删除。尤其是**另一个清算线程也调用 `batch_remove_by_indices_unsafe_with_info`** 时，会出现如下时序：
  1. 线程A锁定->读取索引A->释放锁；
  2. 线程B锁定->读取索引B->释放锁；
  3. 线程B重新加锁并删除索引B，此时 `batch_remove_by_indices_unsafe` 会重排剩余槽位；
  4. 线程A此时才重新加锁并删除“索引A”，但槽位A已经指向其它订单，于是删错人。
- 由于 `RemovedOrderInfo` 是在步骤(1)构造的，即使实际删掉了别的订单，返回值仍然告诉上层“我删了 order_id=100”，造成排查困难且会传播错误数据（下游 `LiquidateEvent` 也会携带错误订单快照）。这完全符合“概率很低，但一旦碰上就删错”的现象。

### 3. 触发条件在当前架构下随时存在
- `StorageEventHandler::handle_event` 每次被调用都会 `spawn_blocking` 一个新任务去处理该事件(`src/solana/storage_handler.rs:82-137`)，没有任何串行化或分片锁。因此多个链上事件可以同时执行。
- 在这些任务里，`handle_long_short_event`、`handle_buy_sell_event`、`handle_full_close_event` 等都会调用 `batch_remove_by_indices_unsafe_with_info` (`src/solana/storage_handler.rs:320-420` 等位置)。由于所有方向(`up/dn`)共享对应 `OrderBookDBManager` 的 `operation_lock`，一旦两个事件命中了同一个 `(mint, direction)`，上面的 TOCTOU 竞态就可被放大。
- 触发概率低的原因在于：需要“两个(或以上)清算事件几乎同时击中同一个 orderbook”；在交易很少的情况下不容易碰到，但在 10 万级高频交易场景中概率就不再可以忽略。

### 4. 其他排除项
- `active_indices`、`order_id_map` 等辅助索引在每次删除中都会被更新为 `[0..new_total)`（`src/orderbook/manager.rs:964-972`），本身不会造成误删，但它们依赖于“删除期间没有额外的结构性修改”这一前提，同样容易被上面的竞态破坏。
- RocksDB 本身的写入是通过 `WriteBatch` 原子提交的，不太可能在提交层面出现“写到一半”的问题，因此重心仍在并发逻辑而非存储。

## 结论与建议
1. **必须消除 `batch_remove_by_indices_unsafe_with_info` 的双阶段锁。**
   - 方案A：让 `batch_remove_by_indices_unsafe` 支持可选地回传删除信息，然后 `with_info` 直接在一次锁保护里完成“读取 + 删除 + 收集”。
   - 方案B：拆出一个 `fn batch_remove_by_indices_locked(&mut self, ...)`，要求调用方在进入前已持有锁；`with_info` 获取锁后调用该内部函数，全程不释放锁。
2. **在问题修复前，务必加上监控/日志**：
   - 在 `batch_remove_by_indices_unsafe_with_info` 里记录“真正删除掉的 order_id 列表”和“返回的 RemovedOrderInfo 中的 order_id 列表”，一旦不一致立即告警，可辅助验证竞态是否正在发生。
3. **如果需要临时缓解，可串行化同一 orderbook 的事件处理。**
   - 例如在 `StorageEventHandler` 层按 `(mint, direction)` 维护一个异步队列/锁，将所有对同一本订单簿的操作排队执行，直到底层 `with_info` 修复完成。
4. **编写并行测试复现**：用两个线程同时对同一个 `OrderBookDBManager` 调用 `batch_remove_by_indices_unsafe_with_info` (例如分别删除 `[0]` 和 `[1]`) ，在 `cargo test` 中加入断言，应该可以在较短时间内出现“返回的 order_id 与实际删除不一致”的情况，从而锁定问题。

只要上述竞态存在，少量并发就足以触发“错删”这一低概率问题。建议优先修复 `with_info` 的锁覆盖范围，再考虑是否需要在上层增加额外的串行化保护。
