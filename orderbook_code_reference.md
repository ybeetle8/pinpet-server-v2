# OrderBook 代码引用汇总

## 关键代码位置速查表

### 1. 核心数据结构定义

#### OrderBookHeader
**文件**: `/home/ybeetle/ybeetle8/pinpet-server-v2/src/orderbook/types.rs:10-91`
```rust
pub struct OrderBookHeader {
    pub version: u8,
    pub order_type: u8,
    pub authority: String,
    pub order_id_counter: u64,
    pub created_at: u32,
    pub last_modified: u32,
    pub total_capacity: u32,
    pub head: u16,
    pub tail: u16,
    pub total: u16,
}
```
- 关键方法: `new()`, `to_bytes()`, `from_bytes()`
- MAX_CAPACITY: 65535 (u16::MAX)

#### MarginOrder
**文件**: `/home/ybeetle/ybeetle8/pinpet-server-v2/src/orderbook/types.rs:93-231`
```rust
pub struct MarginOrder {
    pub user: String,
    pub lock_lp_start_price: u128,
    pub lock_lp_end_price: u128,
    pub open_price: u128,
    pub order_id: u64,
    pub lock_lp_sol_amount: u64,
    pub lock_lp_token_amount: u64,
    pub next_lp_sol_amount: u64,
    pub next_lp_token_amount: u64,
    pub margin_init_sol_amount: u64,
    pub margin_sol_amount: u64,
    pub borrow_amount: u64,
    pub position_asset_amount: u64,
    pub realized_sol_amount: u64,
    pub version: u32,
    pub start_time: u32,
    pub end_time: u32,
    pub next_order: u16,
    pub prev_order: u16,
    pub borrow_fee: u16,
    pub order_type: u8,
}
```

#### MarginOrderUpdateData
**文件**: `/home/ybeetle/ybeetle8/pinpet-server-v2/src/orderbook/types.rs:237-253`
- 所有字段都是 `Option<T>`, 仅用于选择性更新

---

### 2. OrderBookDBManager 核心方法

**文件**: `/home/ybeetle/ybeetle8/pinpet-server-v2/src/orderbook/manager.rs`

#### 初始化
```rust
pub fn new(db: Arc<DB>, mint: String, direction: String) -> Self  // L45
pub fn initialize(&self, authority: String) -> Result<()>  // L169
```

#### 插入操作
```rust
pub fn insert_after(&self, after_index: u16, order_data: &MarginOrder) -> Result<(u16, u64)>  // L325
pub fn insert_before(&self, before_index: u16, order_data: &MarginOrder) -> Result<(u16, u64)>  // L502
```

**关键步骤** (insert_after):
1. 获取操作锁: `let _lock = self.operation_lock.lock().unwrap();` (L327)
2. 加载 header 和验证容量 (L329-342)
3. 处理空链表 (L350-400)
4. 使用 WriteBatch 构建批量操作 (L346)
5. 原子提交: `self.db.write(batch)?;` (L482)

#### 删除操作
```rust
pub fn batch_remove_by_indices_unsafe(
    &self,
    indices: &[u16],
    close_reason: u8,
    previous_price: u128,
) -> Result<()>  // L641

pub fn batch_remove_by_indices_unsafe_with_info(
    &self,
    indices: &[u16],
    close_reason: u8,
    previous_price: u128,
) -> Result<Vec<RemovedOrderInfo>>  // L995
```

**关键步骤** (batch_remove_by_indices_unsafe):
1. 获取操作锁: `let _lock = self.operation_lock.lock().unwrap();` (L654)
2. 克隆、去重、降序排序: (L656-660)
3. HashMap 缓存构建: (L698-710) **← BUG 所在位置**
4. 循环删除 (L715-899):
   - 从链表中摘除节点
   - 删除 ID 映射和用户索引
   - 移动末尾节点到删除位置
   - virtual_tail 前移
5. 更新 tail 指针修复 (L907-957)
6. 更新活跃索引列表 (L962-971)
7. 原子提交: `self.db.write(batch)?;` (L975)

**关键缓存代码** (L698-710):
```rust
use std::collections::HashMap;
let mut order_cache: HashMap<u16, MarginOrder> = HashMap::new();

let get_order_cached = |cache: &HashMap<u16, MarginOrder>, index: u16| -> Result<MarginOrder> {
    if let Some(order) = cache.get(&index) {
        Ok(order.clone())
    } else {
        self.get_order(index)
    }
};
```
**问题**: 缓存中的订单可能已被移动,再次读取会导致重复移动

#### 更新操作
```rust
pub fn update_order(
    &self,
    update_index: u16,
    order_id: u64,
    update_data: &MarginOrderUpdateData,
) -> Result<()>  // L1314
```
**关键问题**: **不获取操作锁**，允许并发执行

#### 查询操作
```rust
pub fn get_order(&self, index: u16) -> Result<MarginOrder>  // L251
pub fn get_order_by_id(&self, order_id: u64) -> Result<MarginOrder>  // L273
pub fn load_header(&self) -> Result<OrderBookHeader>  // L219
pub fn load_active_indices(&self) -> Result<Vec<u16>>  // L290
pub fn get_all_active_orders(&self) -> Result<Vec<(u16, MarginOrder)>>  // L301
pub fn traverse<F>(&self, start: u16, limit: u32, mut callback: F) -> Result<TraversalResult>  // L1418
```

#### 已关闭订单
```rust
fn build_close_record(
    &self,
    order: &MarginOrder,
    close_timestamp: u32,
    close_price: u128,
    close_reason: u8,
) -> Result<ClosedOrderRecord>  // L1550
```

---

### 3. 并发控制相关

#### 操作锁定义
**文件**: `/home/ybeetle/ybeetle8/pinpet-server-v2/src/orderbook/manager.rs:37-40`
```rust
/// Operation lock - ensures insert and delete operations don't execute concurrently
operation_lock: Mutex<()>,
```

#### 序列化事件处理
**文件**: `/home/ybeetle/ybeetle8/pinpet-server-v2/src/solana/storage_handler.rs:82-183`

关键注释 (L77-80):
```rust
// ⚠️  重要: 先处理订单操作,再更新价格
// ⚠️  Important: Process order operations BEFORE updating price
// 这样可以确保在删除订单时获取的是上一次的价格,而不是当前事件的价格
// This ensures we get the previous price when deleting orders, not the current event's price
```

spawn_blocking 中的序列化执行 (L85-183):
- 第一步: 处理所有 OrderBook 操作 (插入/删除/更新)
- 第二步: 更新价格

---

### 4. 已知 Bug 的验证

#### 被忽略的 Bug 验证测试
**文件**: `/home/ybeetle/ybeetle8/pinpet-server-v2/src/orderbook/tests/delete_test.rs:18-54`
```rust
#[test]
#[ignore] // TODO: 修复批量删除的链表指针更新问题 / Fix batch delete linked list pointer update issue
fn test_delete_single_order_from_middle() {
    // 删除中间的两个订单 (index=2, index=3)
    let delete_indices = vec![2, 3];
    let result = orderbook.batch_remove_by_indices_unsafe(delete_indices).unwrap();
    
    // 验证...
}
```

#### Bug 验证测试套件
**文件**: `/home/ybeetle/ybeetle8/pinpet-server-v2/src/orderbook/tests/bug_verification_test.rs`

测试列表:
1. `test_bug_1_writebatch_pointer_conflict()` (L22) - WriteBatch 指针冲突
2. `test_bug_multiple_middle_deletions()` (L256) - 多个中间节点删除
3. `test_bug_sequential_deletions()` (L401) - 连续删除测试
4. `test_bug_tail_pointer_tracking()` (L472) - Tail 指针追踪
5. `test_bug_head_pointer_move()` (L584) - Head 节点移动
6. `test_bug_head_is_tail_move()` (L672) - 两节点链表删除

---

### 5. 数据库存储层

#### OrderBookStorage
**文件**: `/home/ybeetle/ybeetle8/pinpet-server-v2/src/db/orderbook_storage.rs`

```rust
pub struct OrderBookStorage {
    db: Arc<DB>,
    managers: Arc<RwLock<HashMap<String, Arc<OrderBookDBManager>>>>,
}

impl OrderBookStorage {
    pub fn new(config: &OrderBookDbConfig, db_path: &str) -> Result<Self>  // L30
    pub fn get_or_create_manager(
        &self,
        mint: String,
        direction: String,
    ) -> Result<Arc<OrderBookDBManager>>  // L83
}
```

**关键点**: 双重检查锁定 (DCL) 模式 (L90-106)
```rust
// 第一次检查（读锁）- 快速路径 / First check (read lock) - fast path
{
    let managers = self.managers.read().unwrap();
    if let Some(manager) = managers.get(&key) {
        return Ok(manager.clone());
    }
}

// 获取写锁并进行第二次检查 / Acquire write lock and double check
let mut managers = self.managers.write().unwrap();
if let Some(manager) = managers.get(&key) {
    return Ok(manager.clone());
}

// 创建新的 manager
let manager = Arc::new(OrderBookDBManager::new(...));
managers.insert(key, manager.clone());
```

---

### 6. 事件处理

#### LongShort 事件处理
**文件**: `/home/ybeetle/ybeetle8/pinpet-server-v2/src/solana/storage_handler.rs:250-418`

关键部分:
- 确定方向 (L256-265)
- 构造 MarginOrder (L277-299)
- 确定插入位置 (L305-323)
- 插入订单 (L333-340)
- 处理清算 (L357-415)

**清算删除代码** (L388-392):
```rust
let removed_orders = liquidate_manager.batch_remove_by_indices_unsafe_with_info(
    &event.liquidate_indices,
    2, // ForcedLiquidation
    previous_price,
)?;
```

---

### 7. 用户查询

#### UserOrderQueryService
**文件**: `/home/ybeetle/ybeetle8/pinpet-server-v2/src/orderbook/user_query.rs`

关键方法 (L35-150):
```rust
pub fn query_user_active_orders(
    &self,
    user: &str,
    mint_filter: Option<&str>,
    direction_filter: Option<&str>,
    page: u32,
    page_size: u32,
) -> Result<(u32, Vec<(String, String, u16, MarginOrder)>)>
```

**快照使用** (L43-45):
```rust
// ⭐ 创建快照 - 所有读操作在这个时刻的一致性视图上进行
let snapshot = self.db.snapshot();
```

---

### 8. 测试基础设施

#### 测试辅助函数
**文件**: `/home/ybeetle/ybeetle8/pinpet-server-v2/src/orderbook/tests/mod.rs`

```rust
pub fn create_test_db() -> (Arc<DB>, String)  // L11
pub fn create_test_manager() -> (OrderBookDBManager, String)  // L27
pub fn create_test_order(user: &str, price: u128) -> MarginOrder  // L37
pub fn cleanup_test_db(path: &str)  // L21
```

---

## 关键代码片段

### 操作锁的获取和释放
```rust
let _lock = self.operation_lock.lock().unwrap();
// 在作用域结束时自动释放锁
```

### WriteBatch 原子操作
```rust
let mut batch = WriteBatch::default();
batch.put(key.as_bytes(), &value);
batch.delete(key.as_bytes());
self.db.write(batch)?;  // 原子提交
```

### RocksDB 快照查询
```rust
let snapshot = self.db.snapshot();
let bytes = snapshot.get(key.as_bytes())?;
```

### 序列化事件处理模式
```rust
tokio::task::spawn_blocking(move || {
    // OrderBook 操作(获取锁)
    // 价格更新(无锁)
    Ok(results)
}).await??
```

---

## 文档引用

- **OrderBook 说明**: `/home/ybeetle/ybeetle8/pinpet-server-v2/docs/OrderBook说明.md`
- **OrderBook Bug 分析**: `/home/ybeetle/ybeetle8/pinpet-server-v2/orderbook_bug_analysis.md`
- **小概率误删分析**: `/home/ybeetle/ybeetle8/pinpet-server-v2/notes/orderbook小概率误删.md`
- **OrderBook 更新流程**: `/home/ybeetle/ybeetle8/pinpet-server-v2/notes/OrderBook更新流程分析.md`

