// OrderBook 用户查询服务
// OrderBook User Query Service

use crate::orderbook::{MarginOrder, Result, OrderBookError};
use rocksdb::DB;
use std::sync::Arc;

/// 用户活跃订单查询服务
/// User active orders query service
pub struct UserOrderQueryService {
    db: Arc<DB>,
}

impl UserOrderQueryService {
    /// 创建新的查询服务
    /// Create new query service
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    /// 查询用户的所有活跃订单
    /// Query user's all active orders
    ///
    /// # 参数 / Parameters
    /// * `user` - 用户地址
    /// * `mint_filter` - 可选的 mint 过滤
    /// * `direction_filter` - 可选的方向过滤
    /// * `page` - 页码 (从 1 开始)
    /// * `page_size` - 每页数量
    ///
    /// # 返回值 / Returns
    /// (总数, 订单列表)
    /// (total count, order list)
    pub fn query_user_active_orders(
        &self,
        user: &str,
        mint_filter: Option<&str>,
        direction_filter: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<(u32, Vec<(String, String, MarginOrder)>)> {
        // 1. 构建前缀键
        // 1. Build prefix key
        let prefix = if let Some(mint) = mint_filter {
            if let Some(direction) = direction_filter {
                // 精确到方向: orderbook_user:{user}:{mint}:{direction}:
                format!("orderbook_user:{}:{}:{}:", user, mint, direction)
            } else {
                // 精确到 mint: orderbook_user:{user}:{mint}:
                format!("orderbook_user:{}:{}:", user, mint)
            }
        } else {
            // 只过滤用户: orderbook_user:{user}:
            format!("orderbook_user:{}:", user)
        };

        // 2. 前缀扫描,收集所有匹配的键
        // 2. Prefix scan, collect all matching keys
        let mut all_keys = Vec::new();
        let iter = self.db.prefix_iterator(prefix.as_bytes());
        for item in iter {
            let (key, _value) = item?;
            let key_str = String::from_utf8_lossy(&key).to_string();

            // 验证键是否还在前缀范围内
            // Verify key is still within prefix range
            if !key_str.starts_with(&prefix) {
                break;
            }

            all_keys.push(key_str);
        }

        let total = all_keys.len() as u32;

        // 3. 分页
        // 3. Pagination
        let skip = ((page - 1) * page_size) as usize;
        let take = page_size as usize;
        let page_keys: Vec<_> = all_keys.into_iter().skip(skip).take(take).collect();

        // 4. 解析键并查询订单数据
        // 4. Parse keys and query order data
        let mut orders = Vec::new();
        for key in page_keys {
            // 解析键: orderbook_user:{user}:{mint}:{direction}:{start_time}:{order_id}
            // Parse key: orderbook_user:{user}:{mint}:{direction}:{start_time}:{order_id}
            let parts: Vec<&str> = key.split(':').collect();
            if parts.len() != 6 {
                continue; // 跳过格式错误的键 / Skip malformed keys
            }

            let mint = parts[2];
            let direction = parts[3];
            let order_id_str = parts[5];

            // 解析 order_id
            // Parse order_id
            let order_id: u64 = order_id_str.parse().map_err(|_| {
                OrderBookError::InvalidAccountData(format!("Invalid order_id: {}", order_id_str))
            })?;

            // 查询 index
            // Query index
            let id_key = format!("orderbook_id_map:{}:{}:{:010}", mint, direction, order_id);
            let index_bytes = match self.db.get(id_key.as_bytes())? {
                Some(bytes) => bytes,
                None => continue, // 订单可能已被删除,跳过 / Order may be deleted, skip
            };
            let index: u16 = serde_json::from_slice(&index_bytes)?;

            // 查询订单数据
            // Query order data
            let slot_key = format!("orderbook_slot:{}:{}:{:05}", mint, direction, index);
            let order_bytes = match self.db.get(slot_key.as_bytes())? {
                Some(bytes) => bytes,
                None => continue, // 订单可能已被删除,跳过 / Order may be deleted, skip
            };
            let order = MarginOrder::from_bytes(&order_bytes)?;

            orders.push((mint.to_string(), direction.to_string(), order));
        }

        Ok((total, orders))
    }
}
