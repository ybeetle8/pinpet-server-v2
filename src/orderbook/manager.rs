// OrderBook 数据库管理器 - 核心实现
// OrderBook Database Manager - Core Implementation

use crate::orderbook::{
    errors::{OrderBookError, Result},
    types::{MarginOrder, MarginOrderUpdateData, OrderBookHeader, TraversalResult},
};
use rocksdb::{WriteBatch, DB};
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

/// OrderBook 数据库管理器
/// OrderBook database manager
pub struct OrderBookDBManager {
    /// RocksDB 实例 (共享)
    /// RocksDB instance (shared)
    db: Arc<DB>,

    /// 关联的 mint 地址
    /// Associated mint address
    mint: String,

    /// 订单方向: "up"(做空) 或 "dn"(做多)
    /// Order direction: "up"(short) or "dn"(long)
    direction: String,

    /// 操作锁 - 确保插入和删除操作不会并发执行
    /// Operation lock - ensures insert and delete operations don't execute concurrently
    operation_lock: Mutex<()>,
}

impl OrderBookDBManager {
    /// 创建新的 OrderBookDBManager
    /// Create new OrderBookDBManager
    pub fn new(db: Arc<DB>, mint: String, direction: String) -> Self {
        Self {
            db,
            mint,
            direction,
            operation_lock: Mutex::new(()),
        }
    }

    // ==================== 键生成辅助函数 / Key Generation Helpers ====================

    /// 生成 header 键
    /// Generate header key
    fn header_key(&self) -> String {
        format!("orderbook_header:{}:{}", self.mint, self.direction)
    }

    /// 生成订单槽位键
    /// Generate order slot key
    fn slot_key(&self, index: u16) -> String {
        format!(
            "orderbook_slot:{}:{}:{:05}",
            self.mint, self.direction, index
        )
    }

    /// 生成订单ID映射键
    /// Generate order ID mapping key
    fn id_map_key(&self, order_id: u64) -> String {
        format!(
            "orderbook_id_map:{}:{}:{:010}",
            self.mint, self.direction, order_id
        )
    }

    /// 生成活跃索引列表键
    /// Generate active indices list key
    fn active_indices_key(&self) -> String {
        format!("orderbook_active_indices:{}:{}", self.mint, self.direction)
    }

    // ==================== 初始化 / Initialization ====================

    /// 初始化 OrderBook(如果不存在)
    /// Initialize OrderBook (if not exists)
    pub fn initialize(&self, authority: String) -> Result<()> {
        let header_key = self.header_key();

        // 检查是否已存在
        // Check if already exists
        if self.db.get(header_key.as_bytes())?.is_some() {
            warn!(
                "OrderBook already exists: {}:{}",
                self.mint, self.direction
            );
            return Err(OrderBookError::AlreadyExists {
                mint: self.mint.clone(),
                direction: self.direction.clone(),
            });
        }

        // 创建新的 header
        // Create new header
        let order_type = match self.direction.as_str() {
            "dn" => 1,
            "up" => 2,
            _ => {
                return Err(OrderBookError::InvalidDirection(self.direction.clone()));
            }
        };

        let header = OrderBookHeader::new(order_type, authority);

        // 写入数据库
        // Write to database
        self.db.put(header_key.as_bytes(), &header.to_bytes()?)?;

        // 初始化活跃索引列表为空
        // Initialize active indices list as empty
        let active_key = self.active_indices_key();
        let empty_vec: Vec<u16> = vec![];
        self.db
            .put(active_key.as_bytes(), &serde_json::to_vec(&empty_vec)?)?;

        info!(
            "✅ OrderBook initialized: {}:{}",
            self.mint, self.direction
        );
        Ok(())
    }

    // ==================== Header 操作 / Header Operations ====================

    /// 加载 OrderBook header
    /// Load OrderBook header
    pub fn load_header(&self) -> Result<OrderBookHeader> {
        let key = self.header_key();
        match self.db.get(key.as_bytes())? {
            Some(data) => Ok(OrderBookHeader::from_bytes(&data)?),
            None => Err(OrderBookError::NotFound {
                mint: self.mint.clone(),
                direction: self.direction.clone(),
            }),
        }
    }

    /// 更新 OrderBook header
    /// Update OrderBook header
    fn save_header(&self, header: &OrderBookHeader) -> Result<()> {
        let key = self.header_key();
        self.db.put(key.as_bytes(), &header.to_bytes()?)?;
        Ok(())
    }

    /// 在 WriteBatch 中保存 header
    /// Save header in WriteBatch
    fn save_header_batch(&self, batch: &mut WriteBatch, header: &OrderBookHeader) -> Result<()> {
        let key = self.header_key();
        batch.put(key.as_bytes(), &header.to_bytes()?);
        Ok(())
    }

    // ==================== 订单查询操作 / Order Query Operations ====================

    /// 获取指定索引的订单(不可变引用)
    /// Get order at specified index (immutable reference)
    pub fn get_order(&self, index: u16) -> Result<MarginOrder> {
        let header = self.load_header()?;

        // 验证索引范围
        // Validate index range
        if (index as u32) >= header.total_capacity {
            return Err(OrderBookError::InvalidSlotIndex {
                index,
                total: header.total,
            });
        }

        let key = self.slot_key(index);
        match self.db.get(key.as_bytes())? {
            Some(data) => Ok(MarginOrder::from_bytes(&data)?),
            None => Err(OrderBookError::OrderNotFound(index)),
        }
    }

    /// 通过 order_id 获取订单
    /// Get order by order_id
    pub fn get_order_by_id(&self, order_id: u64) -> Result<MarginOrder> {
        // 1. 通过 ID 映射获取 index
        // 1. Get index through ID mapping
        let id_key = self.id_map_key(order_id);
        let index_bytes = self
            .db
            .get(id_key.as_bytes())?
            .ok_or(OrderBookError::OrderIdNotFound(order_id))?;
        let index: u16 = serde_json::from_slice(&index_bytes)?;

        // 2. 获取订单
        // 2. Get order
        self.get_order(index)
    }

    /// 加载活跃索引列表
    /// Load active indices list
    pub fn load_active_indices(&self) -> Result<Vec<u16>> {
        let key = self.active_indices_key();
        match self.db.get(key.as_bytes())? {
            Some(data) => Ok(serde_json::from_slice(&data)?),
            None => Ok(vec![]),
        }
    }

    /// 获取所有活跃订单
    /// Get all active orders
    pub fn get_all_active_orders(&self) -> Result<Vec<(u16, MarginOrder)>> {
        let indices = self.load_active_indices()?;
        let mut orders = Vec::with_capacity(indices.len());

        for index in indices {
            let order = self.get_order(index)?;
            orders.push((index, order));
        }

        Ok(orders)
    }

    // ==================== 插入操作 / Insert Operations ====================

    /// 在指定节点之后插入订单
    /// Insert order after specified node
    ///
    /// # 参数 / Parameters
    /// * `after_index` - 在此索引之后插入(u16::MAX 表示插入到头部)
    /// * `order_data` - 要插入的订单数据
    ///
    /// # 返回值 / Returns
    /// 返回 (插入的订单索引, 订单ID)
    /// Returns (inserted order index, order ID)
    pub fn insert_after(&self, after_index: u16, order_data: &MarginOrder) -> Result<(u16, u64)> {
        // 获取操作锁 / Acquire operation lock
        let _lock = self.operation_lock.lock().unwrap();

        let mut header = self.load_header()?;
        let old_total = header.total;
        let current_order_id = header.order_id_counter;

        // ✅ 验证容量
        // ✅ Validate capacity
        let new_total = old_total
            .checked_add(1)
            .ok_or_else(|| OrderBookError::Overflow("total overflow".to_string()))?;
        if new_total as u32 > OrderBookHeader::MAX_CAPACITY {
            return Err(OrderBookError::ExceedsMaxCapacity {
                max: OrderBookHeader::MAX_CAPACITY,
            });
        }

        // 使用 WriteBatch 保证原子性
        // Use WriteBatch to ensure atomicity
        let mut batch = WriteBatch::default();

        // 处理空链表: 插入第一个节点
        // Handle empty linked list: insert first node
        if old_total == 0 {
            let mut new_order = order_data.clone();
            new_order.order_id = current_order_id;
            new_order.prev_order = u16::MAX;
            new_order.next_order = u16::MAX;
            new_order.version = 1;

            // 写入订单
            // Write order
            let slot_key = self.slot_key(0);
            batch.put(slot_key.as_bytes(), &new_order.to_bytes()?);

            // 更新 ID 映射
            // Update ID mapping
            let id_key = self.id_map_key(current_order_id);
            batch.put(id_key.as_bytes(), &serde_json::to_vec(&0u16)?);

            // 更新活跃索引列表
            // Update active indices list
            let active_key = self.active_indices_key();
            batch.put(active_key.as_bytes(), &serde_json::to_vec(&vec![0u16])?);

            // 更新 header
            // Update header
            header.head = 0;
            header.tail = 0;
            header.total = 1;
            header.total_capacity = 1;
            header.order_id_counter = current_order_id + 1;
            header.last_modified = chrono::Utc::now().timestamp() as u32;
            self.save_header_batch(&mut batch, &header)?;

            // 原子提交
            // Atomic commit
            self.db.write(batch)?;

            info!(
                "✅ Inserted first order: index=0, order_id={}",
                current_order_id
            );
            return Ok((0, current_order_id));
        }

        // 验证索引
        // Validate index
        if after_index >= old_total {
            return Err(OrderBookError::InvalidSlotIndex {
                index: after_index,
                total: old_total,
            });
        }

        // 读取 after_index 节点信息
        // Read after_index node info
        let after_order = self.get_order(after_index)?;
        let old_next = after_order.next_order;

        // 创建新订单
        // Create new order
        let mut new_order = order_data.clone();
        new_order.order_id = current_order_id;
        new_order.prev_order = after_index;
        new_order.next_order = old_next;
        new_order.version = 1;

        // 写入新订单
        // Write new order
        let new_slot_key = self.slot_key(old_total);
        batch.put(new_slot_key.as_bytes(), &new_order.to_bytes()?);

        // 更新 ID 映射
        // Update ID mapping
        let id_key = self.id_map_key(current_order_id);
        batch.put(id_key.as_bytes(), &serde_json::to_vec(&old_total)?);

        // 更新 after_index 节点
        // Update after_index node
        let mut updated_after = after_order.clone();
        updated_after.next_order = old_total;
        updated_after.version += 1;
        let after_slot_key = self.slot_key(after_index);
        batch.put(after_slot_key.as_bytes(), &updated_after.to_bytes()?);

        // 如果不是在尾节点后插入,更新 old_next 节点
        // If not inserting after tail node, update old_next node
        if old_next != u16::MAX {
            let mut old_next_order = self.get_order(old_next)?;
            old_next_order.prev_order = old_total;
            old_next_order.version += 1;
            let old_next_key = self.slot_key(old_next);
            batch.put(old_next_key.as_bytes(), &old_next_order.to_bytes()?);
        } else {
            // 更新尾节点
            // Update tail node
            header.tail = old_total;
        }

        // 更新活跃索引列表
        // Update active indices list
        let mut active_indices = self.load_active_indices()?;
        active_indices.push(old_total);
        let active_key = self.active_indices_key();
        batch.put(active_key.as_bytes(), &serde_json::to_vec(&active_indices)?);

        // 更新 header
        // Update header
        header.total = new_total;
        header.total_capacity = new_total as u32;
        header.order_id_counter = current_order_id + 1;
        header.last_modified = chrono::Utc::now().timestamp() as u32;
        self.save_header_batch(&mut batch, &header)?;

        // 原子提交
        // Atomic commit
        self.db.write(batch)?;

        info!(
            "✅ Inserted order: index={}, order_id={}",
            old_total, current_order_id
        );
        Ok((old_total, current_order_id))
    }

    /// 在指定节点之前插入订单
    /// Insert order before specified node
    ///
    /// # 参数 / Parameters
    /// * `before_index` - 在此索引之前插入
    /// * `order_data` - 要插入的订单数据
    ///
    /// # 返回值 / Returns
    /// 返回 (插入的订单索引, 订单ID)
    /// Returns (inserted order index, order ID)
    pub fn insert_before(
        &self,
        before_index: u16,
        order_data: &MarginOrder,
    ) -> Result<(u16, u64)> {
        // 获取操作锁 / Acquire operation lock
        let _lock = self.operation_lock.lock().unwrap();

        let mut header = self.load_header()?;
        let old_total = header.total;
        let current_order_id = header.order_id_counter;

        // ✅ 验证容量
        // ✅ Validate capacity
        let new_total = old_total
            .checked_add(1)
            .ok_or_else(|| OrderBookError::Overflow("total overflow".to_string()))?;
        if new_total as u32 > OrderBookHeader::MAX_CAPACITY {
            return Err(OrderBookError::ExceedsMaxCapacity {
                max: OrderBookHeader::MAX_CAPACITY,
            });
        }

        // 使用 WriteBatch 保证原子性
        // Use WriteBatch to ensure atomicity
        let mut batch = WriteBatch::default();

        // 处理空链表
        // Handle empty linked list
        if old_total == 0 {
            return self.insert_after(u16::MAX, order_data);
        }

        // 验证索引
        // Validate index
        if before_index >= old_total {
            return Err(OrderBookError::InvalidSlotIndex {
                index: before_index,
                total: old_total,
            });
        }

        // 读取 before_index 节点信息
        // Read before_index node info
        let before_order = self.get_order(before_index)?;
        let old_prev = before_order.prev_order;

        // 创建新订单
        // Create new order
        let mut new_order = order_data.clone();
        new_order.order_id = current_order_id;
        new_order.prev_order = old_prev;
        new_order.next_order = before_index;
        new_order.version = 1;

        // 写入新订单
        // Write new order
        let new_slot_key = self.slot_key(old_total);
        batch.put(new_slot_key.as_bytes(), &new_order.to_bytes()?);

        // 更新 ID 映射
        // Update ID mapping
        let id_key = self.id_map_key(current_order_id);
        batch.put(id_key.as_bytes(), &serde_json::to_vec(&old_total)?);

        // 更新 before_index 节点
        // Update before_index node
        let mut updated_before = before_order.clone();
        updated_before.prev_order = old_total;
        updated_before.version += 1;
        let before_slot_key = self.slot_key(before_index);
        batch.put(before_slot_key.as_bytes(), &updated_before.to_bytes()?);

        // 如果不是在头节点前插入,更新 old_prev 节点
        // If not inserting before head node, update old_prev node
        if old_prev != u16::MAX {
            let mut old_prev_order = self.get_order(old_prev)?;
            old_prev_order.next_order = old_total;
            old_prev_order.version += 1;
            let old_prev_key = self.slot_key(old_prev);
            batch.put(old_prev_key.as_bytes(), &old_prev_order.to_bytes()?);
        } else {
            // 更新头节点
            // Update head node
            header.head = old_total;
        }

        // 更新活跃索引列表
        // Update active indices list
        let mut active_indices = self.load_active_indices()?;
        active_indices.push(old_total);
        let active_key = self.active_indices_key();
        batch.put(active_key.as_bytes(), &serde_json::to_vec(&active_indices)?);

        // 更新 header
        // Update header
        header.total = new_total;
        header.total_capacity = new_total as u32;
        header.order_id_counter = current_order_id + 1;
        header.last_modified = chrono::Utc::now().timestamp() as u32;
        self.save_header_batch(&mut batch, &header)?;

        // 原子提交
        // Atomic commit
        self.db.write(batch)?;

        info!(
            "✅ Inserted order before: index={}, order_id={}",
            old_total, current_order_id
        );
        Ok((old_total, current_order_id))
    }

    // ==================== 删除操作 / Delete Operations ====================

    /// 批量删除订单(按索引,无安全验证)
    /// Batch remove orders by indices (unsafe, no verification)
    ///
    /// # 警告 / Warning
    /// ⚠️ 此函数不验证 order_id,调用者必须确保索引都有效
    /// This function does not verify order_id, caller must ensure all indices are valid
    ///
    /// # 参数 / Parameters
    /// * `indices` - 待删除的索引切片(可乱序、可重复)
    ///
    /// # 返回值 / Returns
    /// 成功返回 Ok(())
    /// Returns Ok(()) on success
    pub fn batch_remove_by_indices_unsafe(&self, indices: &[u16]) -> Result<()> {
        // 0. 处理空数组
        // 0. Handle empty array
        if indices.is_empty() {
            return Ok(());
        }

        // 获取操作锁 / Acquire operation lock
        let _lock = self.operation_lock.lock().unwrap();

        // 1. 克隆、去重并降序排序索引
        // 1. Clone, deduplicate and sort indices in descending order
        let mut sorted_indices = indices.to_vec();
        sorted_indices.sort_unstable_by(|a, b| b.cmp(a)); // 降序 / Descending
        sorted_indices.dedup();

        // 2. 读取初始状态
        // 2. Read initial state
        let mut header = self.load_header()?;
        let old_total = header.total;

        // 验证链表非空
        // Verify linked list is not empty
        if old_total == 0 {
            return Err(OrderBookError::EmptyOrderBook);
        }

        // 验证所有索引都在范围内
        // Verify all indices are within range
        for &index in &sorted_indices {
            if index >= old_total {
                return Err(OrderBookError::InvalidSlotIndex {
                    index,
                    total: old_total,
                });
            }
        }

        let delete_count = sorted_indices.len() as u16;

        // 检查是否删除全部
        // Check if deleting all
        if delete_count >= old_total {
            return self.batch_remove_all();
        }

        // 3. 使用 WriteBatch 和本地缓存
        // 3. Use WriteBatch with local cache
        let mut batch = WriteBatch::default();
        let mut virtual_tail = old_total - 1;

        // ✅ Bug #1 修复: 使用 HashMap 缓存已修改的节点
        // ✅ Bug #1 Fix: Use HashMap to cache modified nodes
        use std::collections::HashMap;
        let mut order_cache: HashMap<u16, MarginOrder> = HashMap::new();

        // 辅助函数: 从缓存或数据库读取订单
        // Helper function: Get order from cache or database
        let get_order_cached = |cache: &HashMap<u16, MarginOrder>, index: u16| -> Result<MarginOrder> {
            if let Some(order) = cache.get(&index) {
                Ok(order.clone())
            } else {
                self.get_order(index)
            }
        };

        for &remove_index in &sorted_indices {
            // 3.1 读取被删除节点
            // 3.1 Read node to be deleted
            let removed_order = get_order_cached(&order_cache, remove_index)?;
            let removed_prev = removed_order.prev_order;
            let removed_next = removed_order.next_order;
            let removed_order_id = removed_order.order_id;

            // 3.2 从链表中摘除该节点 (使用缓存版本)
            // 3.2 Unlink node from linked list (using cached version)
            // 处理前驱节点
            // Handle predecessor node
            if removed_prev != u16::MAX {
                let mut prev_order = get_order_cached(&order_cache, removed_prev)?;
                prev_order.next_order = removed_next;
                prev_order.version += 1;

                // 更新到缓存
                // Update to cache
                order_cache.insert(removed_prev, prev_order.clone());

                let prev_key = self.slot_key(removed_prev);
                batch.put(prev_key.as_bytes(), &prev_order.to_bytes()?);
            } else {
                // 删除的是头节点,更新 head
                // Deleting head node, update head
                header.head = removed_next;
            }

            // 处理后继节点
            // Handle successor node
            if removed_next != u16::MAX {
                let mut next_order = get_order_cached(&order_cache, removed_next)?;
                next_order.prev_order = removed_prev;
                next_order.version += 1;

                // 更新到缓存
                // Update to cache
                order_cache.insert(removed_next, next_order.clone());

                let next_key = self.slot_key(removed_next);
                batch.put(next_key.as_bytes(), &next_order.to_bytes()?);
            } else {
                // 删除的是尾节点,更新 tail
                // Deleting tail node, update tail
                header.tail = removed_prev;

                // 修复: 删除尾节点时,更新前驱节点的 next_order 为 u16::MAX
                // Fix: When deleting tail node, update predecessor's next_order to u16::MAX
                if removed_prev != u16::MAX {
                    let mut prev_order = get_order_cached(&order_cache, removed_prev)?;
                    prev_order.next_order = u16::MAX;
                    prev_order.version += 1;

                    // 更新到缓存
                    // Update to cache
                    order_cache.insert(removed_prev, prev_order.clone());

                    let prev_key = self.slot_key(removed_prev);
                    batch.put(prev_key.as_bytes(), &prev_order.to_bytes()?);
                }
            }

            // 3.3 删除订单槽位
            // 3.3 Delete order slot
            let slot_key = self.slot_key(remove_index);
            batch.delete(slot_key.as_bytes());

            // 3.4 删除 ID 映射
            // 3.4 Delete ID mapping
            let id_key = self.id_map_key(removed_order_id);
            batch.delete(id_key.as_bytes());

            // 3.5 移动末尾节点到被删除位置(如果不是删除末尾)
            // 3.5 Move tail node to deleted position (if not deleting tail)
            if remove_index < virtual_tail {
                // 读取末尾节点数据 (使用缓存)
                // Read tail node data (using cache)
                let tail_order = get_order_cached(&order_cache, virtual_tail)?;
                let tail_prev = tail_order.prev_order;
                let tail_next = tail_order.next_order;
                let tail_order_id = tail_order.order_id;

                // 复制到目标位置
                // Copy to target position
                let mut target_order = tail_order.clone();
                target_order.version += 1;

                // 更新到缓存 (新位置)
                // Update to cache (new position)
                order_cache.insert(remove_index, target_order.clone());

                let target_key = self.slot_key(remove_index);
                batch.put(target_key.as_bytes(), &target_order.to_bytes()?);

                // 更新 ID 映射
                // Update ID mapping
                let id_key = self.id_map_key(tail_order_id);
                batch.put(id_key.as_bytes(), &serde_json::to_vec(&remove_index)?);

                // 删除原位置
                // Delete original position
                let tail_key = self.slot_key(virtual_tail);
                batch.delete(tail_key.as_bytes());

                // 从缓存中移除旧位置
                // Remove old position from cache
                order_cache.remove(&virtual_tail);

                // 更新前驱节点的 next_order (使用缓存)
                // Update predecessor's next_order (using cache)
                if tail_prev != u16::MAX {
                    let mut prev_order = get_order_cached(&order_cache, tail_prev)?;
                    prev_order.next_order = remove_index;
                    prev_order.version += 1;

                    // 更新到缓存
                    // Update to cache
                    order_cache.insert(tail_prev, prev_order.clone());

                    let prev_key = self.slot_key(tail_prev);
                    batch.put(prev_key.as_bytes(), &prev_order.to_bytes()?);
                }

                // 更新后继节点的 prev_order (使用缓存)
                // Update successor's prev_order (using cache)
                if tail_next != u16::MAX {
                    let mut next_order = get_order_cached(&order_cache, tail_next)?;
                    next_order.prev_order = remove_index;
                    next_order.version += 1;

                    // 更新到缓存
                    // Update to cache
                    order_cache.insert(tail_next, next_order.clone());

                    let next_key = self.slot_key(tail_next);
                    batch.put(next_key.as_bytes(), &next_order.to_bytes()?);
                }
            }

            // 3.6 虚拟末尾前移
            // 3.6 Move virtual tail forward
            virtual_tail -= 1;
        }

        // 3.7 更新 OrderBook 头部
        // 3.7 Update OrderBook header
        let new_total = old_total - delete_count;
        header.total = new_total;
        header.total_capacity = new_total as u32;

        // ✅ Bug #2 修复: 更新 tail 指针
        // ✅ Bug #2 Fix: Update tail pointer
        // 如果 tail 超出新的范围,需要找到新的尾节点
        // If tail exceeds new range, need to find new tail node
        if header.tail >= new_total {
            // 从头遍历找到真正的尾节点
            // Traverse from head to find the real tail node
            if new_total > 0 {
                let mut current = header.head;
                loop {
                    if current >= new_total {
                        // tail 无效,设置为第一个有效节点作为临时值
                        // tail is invalid, set to first valid node as temporary value
                        header.tail = if header.head < new_total { header.head } else { u16::MAX };
                        break;
                    }

                    let order = get_order_cached(&order_cache, current)?;
                    if order.next_order == u16::MAX || order.next_order >= new_total {
                        // 找到尾节点
                        // Found tail node
                        header.tail = current;

                        // 如果新尾节点的 next 不是 MAX,需要修正
                        // If new tail node's next is not MAX, need to fix it
                        if order.next_order != u16::MAX {
                            let mut new_tail_order = order.clone();
                            new_tail_order.next_order = u16::MAX;
                            new_tail_order.version += 1;

                            // 更新到缓存和批量操作
                            // Update to cache and batch
                            order_cache.insert(current, new_tail_order.clone());
                            let tail_key = self.slot_key(current);
                            batch.put(tail_key.as_bytes(), &new_tail_order.to_bytes()?);
                        }
                        break;
                    }
                    current = order.next_order;

                    // 防止无限循环
                    // Prevent infinite loop
                    if current == header.head {
                        header.tail = u16::MAX;
                        break;
                    }
                }
            } else {
                header.tail = u16::MAX;
            }
        }

        header.last_modified = chrono::Utc::now().timestamp() as u32;
        self.save_header_batch(&mut batch, &header)?;

        // 3.8 更新活跃索引列表
        // 3.8 Update active indices list
        // ✅ Bug #3 修复: 删除和移动操作后,active_indices 应该是 [0..new_total)
        // ✅ Bug #3 Fix: After delete and move operations, active_indices should be [0..new_total)
        // 因为我们使用了移动末尾节点的策略,删除后所有有效索引都是连续的
        // Since we use move-tail strategy, all valid indices are consecutive after deletion
        let active_indices: Vec<u16> = (0..new_total).collect();

        let active_key = self.active_indices_key();
        batch.put(active_key.as_bytes(), &serde_json::to_vec(&active_indices)?);

        // 4. 原子提交
        // 4. Atomic commit
        self.db.write(batch)?;

        info!("✅ Batch removed {} orders", delete_count);
        Ok(())
    }

    /// 内部辅助函数: 从链表中摘除节点
    /// Internal helper: Unlink node from linked list
    fn unlink_node_internal(
        &self,
        batch: &mut WriteBatch,
        header: &mut OrderBookHeader,
        removed_prev: u16,
        removed_next: u16,
    ) -> Result<()> {
        // 处理前驱节点
        // Handle predecessor node
        if removed_prev != u16::MAX {
            let mut prev_order = self.get_order(removed_prev)?;
            prev_order.next_order = removed_next;
            prev_order.version += 1;
            let prev_key = self.slot_key(removed_prev);
            batch.put(prev_key.as_bytes(), &prev_order.to_bytes()?);
        } else {
            // 删除的是头节点,更新 head
            // Deleting head node, update head
            header.head = removed_next;
        }

        // 处理后继节点
        // Handle successor node
        if removed_next != u16::MAX {
            let mut next_order = self.get_order(removed_next)?;
            next_order.prev_order = removed_prev;
            next_order.version += 1;
            let next_key = self.slot_key(removed_next);
            batch.put(next_key.as_bytes(), &next_order.to_bytes()?);
        } else {
            // 删除的是尾节点,更新 tail
            // Deleting tail node, update tail
            header.tail = removed_prev;

            // ✅ 修复: 删除尾节点时,更新前驱节点的 next_order 为 u16::MAX
            // ✅ Fix: When deleting tail node, update predecessor's next_order to u16::MAX
            if removed_prev != u16::MAX {
                let mut prev_order = self.get_order(removed_prev)?;
                prev_order.next_order = u16::MAX;
                prev_order.version += 1;
                let prev_key = self.slot_key(removed_prev);
                batch.put(prev_key.as_bytes(), &prev_order.to_bytes()?);
            }
        }

        Ok(())
    }

    /// 内部辅助函数: 将末尾节点移动到指定索引
    /// Internal helper: Move tail node to specified index
    fn move_tail_to_index_internal(
        &self,
        batch: &mut WriteBatch,
        tail_index: u16,
        target_index: u16,
    ) -> Result<()> {
        // 读取末尾节点数据
        // Read tail node data
        let tail_order = self.get_order(tail_index)?;
        let tail_prev = tail_order.prev_order;
        let tail_next = tail_order.next_order;
        let tail_order_id = tail_order.order_id;

        // 复制到目标位置
        // Copy to target position
        let mut target_order = tail_order.clone();
        target_order.version += 1;
        let target_key = self.slot_key(target_index);
        batch.put(target_key.as_bytes(), &target_order.to_bytes()?);

        // 更新 ID 映射
        // Update ID mapping
        let id_key = self.id_map_key(tail_order_id);
        batch.put(id_key.as_bytes(), &serde_json::to_vec(&target_index)?);

        // 删除原位置
        // Delete original position
        let tail_key = self.slot_key(tail_index);
        batch.delete(tail_key.as_bytes());

        // 更新前驱节点的 next_order
        // Update predecessor's next_order
        if tail_prev != u16::MAX {
            let mut prev_order = self.get_order(tail_prev)?;
            prev_order.next_order = target_index;
            prev_order.version += 1;
            let prev_key = self.slot_key(tail_prev);
            batch.put(prev_key.as_bytes(), &prev_order.to_bytes()?);
        }

        // 更新后继节点的 prev_order
        // Update successor's prev_order
        if tail_next != u16::MAX {
            let mut next_order = self.get_order(tail_next)?;
            next_order.prev_order = target_index;
            next_order.version += 1;
            let next_key = self.slot_key(tail_next);
            batch.put(next_key.as_bytes(), &next_order.to_bytes()?);
        }

        Ok(())
    }

    /// 内部辅助函数: 删除全部订单
    /// Internal helper: Remove all orders
    fn batch_remove_all(&self) -> Result<()> {
        let mut batch = WriteBatch::default();

        // 删除所有订单槽位和 ID 映射
        // Delete all order slots and ID mappings
        let active_indices = self.load_active_indices()?;
        for index in active_indices {
            let order = self.get_order(index)?;

            // 删除槽位
            // Delete slot
            let slot_key = self.slot_key(index);
            batch.delete(slot_key.as_bytes());

            // 删除 ID 映射
            // Delete ID mapping
            let id_key = self.id_map_key(order.order_id);
            batch.delete(id_key.as_bytes());
        }

        // 重置 header
        // Reset header
        let mut header = self.load_header()?;
        header.head = u16::MAX;
        header.tail = u16::MAX;
        header.total = 0;
        header.total_capacity = 0;
        header.last_modified = chrono::Utc::now().timestamp() as u32;
        self.save_header_batch(&mut batch, &header)?;

        // 清空活跃索引列表
        // Clear active indices list
        let active_key = self.active_indices_key();
        batch.put(
            active_key.as_bytes(),
            &serde_json::to_vec(&Vec::<u16>::new())?,
        );

        // 原子提交
        // Atomic commit
        self.db.write(batch)?;

        info!("✅ Removed all orders");
        Ok(())
    }

    // ==================== 更新操作 / Update Operations ====================

    /// 更新指定索引的订单(需要 order_id 双重验证)
    /// Update order at specified index (requires order_id verification)
    ///
    /// # 参数 / Parameters
    /// * `update_index` - 要更新的订单索引
    /// * `order_id` - 订单 ID(用于验证)
    /// * `update_data` - 更新数据(只包含可更新的字段)
    ///
    /// # 返回值 / Returns
    /// 成功返回 Ok(())
    /// Returns Ok(()) on success
    pub fn update_order(
        &self,
        update_index: u16,
        order_id: u64,
        update_data: &MarginOrderUpdateData,
    ) -> Result<()> {
        // 1. 读取并验证
        // 1. Read and validate
        let header = self.load_header()?;

        // 验证索引范围
        // Validate index range
        if update_index >= header.total {
            return Err(OrderBookError::InvalidSlotIndex {
                index: update_index,
                total: header.total,
            });
        }

        // 读取订单并验证 order_id
        // Read order and validate order_id
        let mut order = self.get_order(update_index)?;
        if order.order_id != order_id {
            return Err(OrderBookError::OrderIdMismatch {
                expected: order_id,
                actual: order.order_id,
            });
        }

        // 2. 应用更新(只更新非 None 的字段)
        // 2. Apply updates (only update non-None fields)
        if let Some(lock_lp_start_price) = update_data.lock_lp_start_price {
            order.lock_lp_start_price = lock_lp_start_price;
        }
        if let Some(lock_lp_end_price) = update_data.lock_lp_end_price {
            order.lock_lp_end_price = lock_lp_end_price;
        }
        if let Some(lock_lp_sol_amount) = update_data.lock_lp_sol_amount {
            order.lock_lp_sol_amount = lock_lp_sol_amount;
        }
        if let Some(lock_lp_token_amount) = update_data.lock_lp_token_amount {
            order.lock_lp_token_amount = lock_lp_token_amount;
        }
        if let Some(next_lp_sol_amount) = update_data.next_lp_sol_amount {
            order.next_lp_sol_amount = next_lp_sol_amount;
        }
        if let Some(next_lp_token_amount) = update_data.next_lp_token_amount {
            order.next_lp_token_amount = next_lp_token_amount;
        }
        if let Some(end_time) = update_data.end_time {
            order.end_time = end_time;
        }
        if let Some(margin_init_sol_amount) = update_data.margin_init_sol_amount {
            order.margin_init_sol_amount = margin_init_sol_amount;
        }
        if let Some(margin_sol_amount) = update_data.margin_sol_amount {
            order.margin_sol_amount = margin_sol_amount;
        }
        if let Some(borrow_amount) = update_data.borrow_amount {
            order.borrow_amount = borrow_amount;
        }
        if let Some(position_asset_amount) = update_data.position_asset_amount {
            order.position_asset_amount = position_asset_amount;
        }
        if let Some(borrow_fee) = update_data.borrow_fee {
            order.borrow_fee = borrow_fee;
        }
        if let Some(open_price) = update_data.open_price {
            order.open_price = open_price;
        }
        if let Some(realized_sol_amount) = update_data.realized_sol_amount {
            order.realized_sol_amount = realized_sol_amount;
        }

        // 更新版本号
        // Update version number
        order.version += 1;

        // 3. 写回数据库
        // 3. Write back to database
        let slot_key = self.slot_key(update_index);
        self.db.put(slot_key.as_bytes(), &order.to_bytes()?)?;

        info!(
            "✅ Updated order: index={}, order_id={}",
            update_index, order_id
        );
        Ok(())
    }

    // ==================== 遍历操作 / Traverse Operations ====================

    /// 遍历订单(不可变,支持批量和续传)
    /// Traverse orders (immutable, with pagination and resume support)
    ///
    /// # 参数 / Parameters
    /// * `start` - 起始索引(u16::MAX = 从 head 开始)
    /// * `limit` - 最多处理数量(0 = 无限制)
    /// * `callback` - 回调函数 fn(index, order) -> Result<bool>
    ///   - 返回 true: 继续
    ///   - 返回 false: 中断
    ///
    /// # 返回值 / Returns
    /// TraversalResult { processed, next, done }
    pub fn traverse<F>(&self, start: u16, limit: u32, mut callback: F) -> Result<TraversalResult>
    where
        F: FnMut(u16, &MarginOrder) -> Result<bool>,
    {
        let header = self.load_header()?;

        // 确定起始位置
        // Determine starting position
        let mut current = if start == u16::MAX {
            header.head
        } else {
            start
        };

        // 空链表或无效起始
        // Empty linked list or invalid start
        if current == u16::MAX {
            return Ok(TraversalResult {
                processed: 0,
                next: u16::MAX,
                done: true,
            });
        }

        let mut count = 0;

        loop {
            // 验证索引有效性
            // Validate index validity
            if current >= header.total {
                return Err(OrderBookError::TraversalInvalidIndex(current));
            }

            // 读取订单
            // Read order
            let order = self.get_order(current)?;

            // 执行回调
            // Execute callback
            let should_continue = callback(current, &order)?;
            count += 1;

            // 用户主动中断
            // User actively interrupted
            if !should_continue {
                return Ok(TraversalResult {
                    processed: count,
                    next: order.next_order,
                    done: false,
                });
            }

            // 达到限制
            // Reached limit
            if limit > 0 && count >= limit {
                return Ok(TraversalResult {
                    processed: count,
                    next: order.next_order,
                    done: order.next_order == u16::MAX,
                });
            }

            // 到达尾部
            // Reached tail
            if order.next_order == u16::MAX {
                return Ok(TraversalResult {
                    processed: count,
                    next: u16::MAX,
                    done: true,
                });
            }

            current = order.next_order;
        }
    }

    /// 获取指定插入位置的前后邻居节点索引
    /// Get insert neighbors for specified position
    ///
    /// # 参数 / Parameters
    /// * `insert_pos` - 插入位置标识:
    ///   - `u16::MAX`: 插入到链表头部(成为新的 head)
    ///   - 有效索引: 在该索引节点之后插入
    ///
    /// # 返回值 / Returns
    /// `(prev_index: Option<u16>, next_index: Option<u16>)`
    pub fn get_insert_neighbors(&self, insert_pos: u16) -> Result<(Option<u16>, Option<u16>)> {
        let header = self.load_header()?;

        // 情况 1: 空链表
        // Case 1: Empty linked list
        if header.total == 0 {
            return Ok((None, None));
        }

        // 情况 2: 插入到头部
        // Case 2: Insert at head
        if insert_pos == u16::MAX {
            let head_idx = header.head;
            if head_idx == u16::MAX {
                return Err(OrderBookError::InvalidAccountData(
                    "head is MAX but total > 0".to_string(),
                ));
            }
            return Ok((None, Some(head_idx)));
        }

        // 情况 3 & 4: 插入到指定节点之后
        // Case 3 & 4: Insert after specified node
        if insert_pos >= header.total {
            return Err(OrderBookError::InvalidSlotIndex {
                index: insert_pos,
                total: header.total,
            });
        }

        let node = self.get_order(insert_pos)?;

        let next_idx = if node.next_order == u16::MAX {
            None // 插入到尾部 / Insert at tail
        } else {
            Some(node.next_order) // 插入到中间 / Insert in middle
        };

        Ok((Some(insert_pos), next_idx))
    }
}
