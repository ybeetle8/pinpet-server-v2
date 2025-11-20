// OrderBook 存储模块 / OrderBook storage module
use anyhow::Result;
use rocksdb::{WriteBatch, IteratorMode, Direction, DB};
use serde::{Serialize, Deserialize};
use serde_with::{serde_as, DisplayFromStr};
use std::sync::Arc;
use tracing::info;
use utoipa::ToSchema;

/// 订单完整数据结构 / Complete order data structure
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct OrderData {
    // ==================== 标识信息 / Identification ====================
    /// Slot number - 8 bytes
    pub slot: u64,

    /// 订单唯一标识符（在 mint + direction 范围内唯一） - 8 bytes
    /// Order unique identifier (unique within mint + direction)
    pub order_id: u64,

    /// 开仓用户 - 32 bytes
    /// User who opened the position
    pub user: String,

    // ==================== 价格信息 / Price Information ====================
    /// 锁定流动池区间开始价 (Q64.64 格式) - 16 bytes
    /// Locked LP range start price (Q64.64 format)
    #[serde_as(as = "DisplayFromStr")]
    pub lock_lp_start_price: u128,

    /// 锁定流动池区间结束价 (Q64.64 格式) - 16 bytes
    /// Locked LP range end price (Q64.64 format)
    #[serde_as(as = "DisplayFromStr")]
    pub lock_lp_end_price: u128,

    /// 开仓价 (Q64.64 格式，开仓时设置，永远不会再变) - 16 bytes
    /// Open price (Q64.64 format, set at opening, never changes)
    #[serde_as(as = "DisplayFromStr")]
    pub open_price: u128,

    // ==================== 数量信息 / Amount Information ====================
    /// 锁定流动池区间 SOL 数量 (精确值，lamports) - 8 bytes
    /// Locked LP range SOL amount (exact value, lamports)
    pub lock_lp_sol_amount: u64,

    /// 锁定流动池区间 Token 数量 (精确值，最小单位) - 8 bytes
    /// Locked LP range Token amount (exact value, smallest unit)
    pub lock_lp_token_amount: u64,

    /// 初始保证金 SOL 数量 (主要作为记录用，不参与计算) - 8 bytes
    /// Initial margin SOL amount (mainly for record, not used in calculations)
    pub margin_init_sol_amount: u64,

    /// 保证金 SOL 数量 - 8 bytes
    /// Margin SOL amount
    pub margin_sol_amount: u64,

    /// 贷款数量：如果是做多则借出 SOL，如果是做空则借出 Token - 8 bytes
    /// Borrow amount: SOL for long, Token for short
    pub borrow_amount: u64,

    /// 当前持仓币的数量 (做空时是 SOL，做多时是 Token) - 8 bytes
    /// Current position asset amount (SOL for short, Token for long)
    /// 注意：做多时这值完全等于 lock_lp_token_amount
    /// Note: For long positions, this equals lock_lp_token_amount
    pub position_asset_amount: u64,

    /// 已实现的 SOL 利润 - 8 bytes
    /// Realized SOL profit
    pub realized_sol_amount: u64,

    // ==================== 时间信息 / Time Information ====================
    /// 订单开始时间戳 (Unix timestamp, 秒) - 4 bytes
    /// Order start timestamp (Unix timestamp, seconds)
    pub start_time: u32,

    /// 贷款到期时间戳 (Unix timestamp, 秒)，到期后可被任何用户平仓 - 4 bytes
    /// Loan expiry timestamp (Unix timestamp, seconds), can be closed by anyone after expiry
    pub end_time: u32,

    // ==================== 其他信息 / Other Information ====================
    /// 保证金交易手续费 (基点, bps) - 2 bytes
    /// Margin trading fee (basis points, bps)
    /// 需要记录，因为手续费基数可能变化（但同一订单不能变化）
    /// Need to record as fee basis may change (but cannot change for same order)
    /// 例如: 50 = 0.5%, 100 = 1%
    /// Example: 50 = 0.5%, 100 = 1%
    pub borrow_fee: u16,

    /// 订单类型: 1=做多(Down方向) 2=做空(Up方向) - 1 byte
    /// Order type: 1=Long(Down direction) 2=Short(Up direction)
    pub order_type: u8,

    // ==================== 状态信息（仅已关闭订单） / State Information (closed orders only) ====================
    /// 关闭时间戳 (仅已关闭订单) - 4 bytes
    /// Close timestamp (closed orders only)
    pub close_time: Option<u32>,

    /// 关闭类型: 0=未关闭, 1=正常平仓, 2=强制平仓 - 1 byte
    /// Close type: 0=Not closed, 1=Normal close, 2=Force liquidation
    pub close_type: u8,
}

impl OrderData {
    /// 获取订单方向编码 / Get order direction code
    pub fn direction(&self) -> &'static str {
        match self.order_type {
            1 => "dn",  // 做多 Long
            2 => "up",  // 做空 Short
            _ => "dn",  // 默认 Default
        }
    }

    /// 序列化为字节 / Serialize to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// 从字节反序列化 / Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

/// OrderBook 存储服务 / OrderBook storage service
pub struct OrderBookStorage {
    db: Arc<DB>,
}

impl OrderBookStorage {
    /// 创建新的 OrderBook 存储服务 / Create new OrderBook storage service
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    // ==================== 激活订单操作 / Active Order Operations ====================

    /// 添加新的激活订单 / Add new active order
    pub async fn add_active_order(&self, mint: &str, order: &OrderData) -> Result<()> {
        let mut batch = WriteBatch::default();
        let dir = order.direction();

        // 1. 主存储 / Primary storage
        let main_key = format!(
            "active_order:{}:{}:{:010}:{:010}",
            mint, dir, order.slot, order.order_id
        );
        batch.put(main_key.as_bytes(), &order.to_bytes()?);

        // 2. 用户索引 / User index
        let user_idx_key = format!(
            "active_user:{}:{}:{}:{:010}:{:010}",
            order.user, mint, dir, order.slot, order.order_id
        );
        batch.put(user_idx_key.as_bytes(), b"");

        // 3. 订单ID映射 / Order ID mapping
        let id_map_key = format!(
            "active_id:{}:{}:{:010}",
            mint, dir, order.order_id
        );
        let slot_str = format!("{:010}", order.slot);
        batch.put(id_map_key.as_bytes(), slot_str.as_bytes());

        self.db.write(batch)?;
        info!(
            "添加激活订单 / Added active order: mint={}, dir={}, order_id={}",
            mint, dir, order.order_id
        );
        Ok(())
    }

    /// 查询 mint + direction 的所有激活订单 / Query all active orders for mint + direction
    /// 返回 (mint, order) 元组列表，按 slot 降序排序 / Returns (mint, order) tuples, sorted by slot descending
    pub async fn get_active_orders_by_mint(
        &self,
        mint: &str,
        direction: &str,
        limit: Option<usize>,
    ) -> Result<Vec<(String, OrderData)>> {
        let prefix = format!("active_order:{}:{}:", mint, direction);
        let mut orders = self.scan_orders_with_mint(&prefix, limit).await?;

        // 按 slot 降序排序 / Sort by slot descending
        orders.sort_by(|a, b| b.1.slot.cmp(&a.1.slot));

        Ok(orders)
    }

    /// 查询用户在指定 mint 上的所有激活订单 / Query all active orders for user on specific mint
    /// 返回 (mint, order) 元组列表，按 slot 降序排序 / Returns (mint, order) tuples, sorted by slot descending
    pub async fn get_active_orders_by_user_mint(
        &self,
        user: &str,
        mint: Option<&str>,
        direction: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<(String, OrderData)>> {
        let prefix = match (mint, direction) {
            (Some(m), Some(d)) => format!("active_user:{}:{}:{}:", user, m, d),
            (Some(m), None) => format!("active_user:{}:{}:", user, m),
            (None, _) => format!("active_user:{}:", user),
        };

        // 扫描索引，获取订单引用 / Scan index, get order references
        let order_keys = self.scan_index_keys(&prefix, limit).await?;

        // 根据引用获取完整订单数据 / Get complete order data from references
        let mut orders = Vec::new();
        for key in order_keys {
            if let Some((mint, order)) = self.get_order_from_user_index(&key).await? {
                orders.push((mint, order));
            }
        }

        // 按 slot 降序排序 / Sort by slot descending
        orders.sort_by(|a, b| b.1.slot.cmp(&a.1.slot));

        Ok(orders)
    }

    /// 通过 order_id 获取激活订单 / Get active order by order_id
    /// 返回 (mint, order) 元组 / Returns (mint, order) tuple
    pub async fn get_active_order_by_id(
        &self,
        mint: &str,
        direction: &str,
        order_id: u64,
    ) -> Result<Option<(String, OrderData)>> {
        // 1. 通过 ID 映射获取 slot / Get slot through ID mapping
        let id_map_key = format!("active_id:{}:{}:{:010}", mint, direction, order_id);

        let slot_bytes = match self.db.get(id_map_key.as_bytes())? {
            Some(bytes) => bytes,
            None => return Ok(None),
        };

        let slot_str = String::from_utf8_lossy(&slot_bytes);
        let slot: u64 = slot_str.parse()?;

        // 2. 构建主键获取完整订单 / Build main key to get complete order
        let main_key = format!(
            "active_order:{}:{}:{:010}:{:010}",
            mint, direction, slot, order_id
        );

        match self.db.get(main_key.as_bytes())? {
            Some(data) => Ok(Some((mint.to_string(), OrderData::from_bytes(&data)?))),
            None => Ok(None),
        }
    }

    // ==================== 订单关闭操作 / Order Close Operations ====================

    /// 关闭订单（从激活状态移动到关闭状态）/ Close order (move from active to closed state)
    pub async fn close_order(
        &self,
        mint: &str,
        order_id: u64,
        close_time: u32,
        close_type: u8,
    ) -> Result<()> {
        // 1. 先尝试从 up 方向获取 / Try to get from up direction first
        let mut order = self.get_active_order_by_id(mint, "up", order_id).await?;
        let mut dir = "up";

        // 2. 如果 up 方向没有，从 dn 方向获取 / If not in up, get from dn direction
        if order.is_none() {
            order = self.get_active_order_by_id(mint, "dn", order_id).await?;
            dir = "dn";
        }

        let (mint_str, mut order_data) = match order {
            Some((m, o)) => (m, o),
            None => {
                return Err(anyhow::anyhow!(
                    "订单不存在 / Order not found: mint={}, order_id={}",
                    mint, order_id
                ));
            }
        };

        // 3. 更新订单状态 / Update order state
        order_data.close_time = Some(close_time);
        order_data.close_type = close_type;

        let mut batch = WriteBatch::default();

        // 4. 删除激活订单的所有键 / Delete all keys for active order
        self.delete_active_order_keys(&mut batch, &mint_str, dir, &order_data);

        // 5. 添加已关闭订单的所有键 / Add all keys for closed order
        self.add_closed_order_keys(&mut batch, &mint_str, &order_data)?;

        // 6. 原子提交 / Atomic commit
        self.db.write(batch)?;

        info!(
            "订单已关闭 / Order closed: mint={}, order_id={}, close_type={}",
            mint, order_id, close_type
        );
        Ok(())
    }

    /// 删除激活订单的所有键 / Delete all keys for active order
    fn delete_active_order_keys(
        &self,
        batch: &mut WriteBatch,
        mint: &str,
        dir: &str,
        order: &OrderData,
    ) {
        // 主存储 / Primary storage
        let main_key = format!(
            "active_order:{}:{}:{:010}:{:010}",
            mint, dir, order.slot, order.order_id
        );
        batch.delete(main_key.as_bytes());

        // 用户索引 / User index
        let user_idx_key = format!(
            "active_user:{}:{}:{}:{:010}:{:010}",
            order.user, mint, dir, order.slot, order.order_id
        );
        batch.delete(user_idx_key.as_bytes());

        // 订单ID映射 / Order ID mapping
        let id_map_key = format!("active_id:{}:{}:{:010}", mint, dir, order.order_id);
        batch.delete(id_map_key.as_bytes());
    }

    /// 添加已关闭订单的所有键 / Add all keys for closed order
    fn add_closed_order_keys(
        &self,
        batch: &mut WriteBatch,
        mint: &str,
        order: &OrderData,
    ) -> Result<()> {
        let dir = order.direction();
        let close_time = order.close_time.unwrap_or(0);

        // 主存储 / Primary storage
        let main_key = format!(
            "closed_order:{}:{:010}:{}:{}:{:010}",
            order.user, close_time, mint, dir, order.order_id
        );
        batch.put(main_key.as_bytes(), &order.to_bytes()?);

        Ok(())
    }

    // ==================== 已关闭订单查询 / Closed Order Queries ====================

    /// 查询用户的已关闭订单 / Query user's closed orders
    /// 返回 (mint, order) 元组列表，按 slot 降序排序 / Returns (mint, order) tuples, sorted by slot descending
    pub async fn get_closed_orders_by_user(
        &self,
        user: &str,
        start_time: Option<u32>,
        end_time: Option<u32>,
        limit: Option<usize>,
    ) -> Result<Vec<(String, OrderData)>> {
        let start_prefix = match start_time {
            Some(t) => format!("closed_order:{}:{:010}:", user, t),
            None => format!("closed_order:{}:0000000000:", user),
        };

        let end_prefix = match end_time {
            Some(t) => format!("closed_order:{}:{:010}:", user, t),
            None => format!("closed_order:{}:9999999999:", user),
        };

        let mut orders = Vec::new();
        let iter = self.db.iterator(IteratorMode::From(
            start_prefix.as_bytes(),
            Direction::Forward,
        ));

        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key).to_string();

            // 检查是否超出范围 / Check if exceeds range
            if key_str > end_prefix {
                break;
            }

            // 检查是否仍在用户范围内 / Check if still within user range
            if !key_str.starts_with(&format!("closed_order:{}:", user)) {
                break;
            }

            // 解析键提取 mint / Parse key to extract mint
            // 键格式: closed_order:{user}:{close_time:010}:{mint}:{dir}:{order_id:010}
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 4 {
                let mint = parts[3].to_string();

                // 解析订单数据 / Parse order data
                if let Ok(order) = OrderData::from_bytes(&value) {
                    orders.push((mint, order));

                    if let Some(limit) = limit {
                        if orders.len() >= limit {
                            break;
                        }
                    }
                }
            }
        }

        // 按 slot 降序排序 / Sort by slot descending
        orders.sort_by(|a, b| b.1.slot.cmp(&a.1.slot));

        Ok(orders)
    }

    // ==================== 辅助方法 / Helper Methods ====================

    /// 扫描并返回订单列表（带 mint）/ Scan and return order list with mint
    async fn scan_orders_with_mint(&self, prefix: &str, limit: Option<usize>) -> Result<Vec<(String, OrderData)>> {
        let mut orders = Vec::new();
        let iter = self.db.iterator(IteratorMode::From(
            prefix.as_bytes(),
            Direction::Forward,
        ));

        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key).to_string();

            if !key_str.starts_with(prefix) {
                break;
            }

            // 解析键提取 mint / Parse key to extract mint
            // 键格式: active_order:{mint}:{dir}:{slot:010}:{order_id:010}
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 2 {
                let mint = parts[1].to_string();

                if let Ok(order) = OrderData::from_bytes(&value) {
                    orders.push((mint, order));

                    if let Some(limit) = limit {
                        if orders.len() >= limit {
                            break;
                        }
                    }
                }
            }
        }

        Ok(orders)
    }

    /// 扫描索引键 / Scan index keys
    async fn scan_index_keys(&self, prefix: &str, limit: Option<usize>) -> Result<Vec<String>> {
        let mut keys = Vec::new();
        let iter = self.db.iterator(IteratorMode::From(
            prefix.as_bytes(),
            Direction::Forward,
        ));

        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key).to_string();

            if !key_str.starts_with(prefix) {
                break;
            }

            keys.push(key_str);

            if let Some(limit) = limit {
                if keys.len() >= limit {
                    break;
                }
            }
        }

        Ok(keys)
    }

    /// 从用户索引获取订单（带 mint）/ Get order from user index with mint
    async fn get_order_from_user_index(&self, index_key: &str) -> Result<Option<(String, OrderData)>> {
        // 解析: active_user:{user}:{mint}:{dir}:{slot:010}:{order_id:010}
        // Parse: active_user:{user}:{mint}:{dir}:{slot:010}:{order_id:010}
        let parts: Vec<&str> = index_key.split(':').collect();
        if parts.len() < 6 {
            return Ok(None);
        }

        let mint = parts[2].to_string();
        let dir = parts[3];
        let slot = parts[4];
        let order_id = parts[5];

        let main_key = format!("active_order:{}:{}:{}:{}", mint, dir, slot, order_id);

        match self.db.get(main_key.as_bytes())? {
            Some(data) => Ok(Some((mint, OrderData::from_bytes(&data)?))),
            None => Ok(None),
        }
    }
}
