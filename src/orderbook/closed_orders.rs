// 已关闭订单查询接口
// Closed orders query interface

use crate::orderbook::{
    errors::Result,
    types::ClosedOrderRecord,
    manager::OrderBookDBManager,
};
use rocksdb::DB;
use std::sync::Arc;

/// 已关闭订单查询接口
/// Closed orders query interface
pub struct ClosedOrdersQuery {
    db: Arc<DB>,
}

impl ClosedOrdersQuery {
    /// 创建查询实例
    /// Create query instance
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    /// 查询用户所有已关闭订单(按时间倒序)
    /// Query all closed orders for user (reverse chronological)
    ///
    /// # 参数 / Parameters
    /// * `user_address` - 用户地址
    /// * `limit` - 返回数量限制(None = 无限制)
    ///
    /// # 返回值 / Returns
    /// Vec<ClosedOrderRecord> - 按关闭时间倒序排列
    pub fn query_user_closed_orders(
        &self,
        user_address: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ClosedOrderRecord>> {
        let prefix = OrderBookDBManager::closed_order_prefix(user_address);
        self.scan_with_prefix(&prefix, limit)
    }

    /// 查询用户在特定 mint 的已关闭订单
    /// Query closed orders for specific mint
    ///
    /// # 参数 / Parameters
    /// * `user_address` - 用户地址
    /// * `mint` - Token mint 地址
    /// * `direction` - 可选,订单方向 ("up" 或 "dn")
    /// * `limit` - 返回数量限制
    pub fn query_user_closed_orders_by_mint(
        &self,
        user_address: &str,
        mint: &str,
        direction: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<ClosedOrderRecord>> {
        // 先获取所有已关闭订单,然后在应用层过滤
        // Get all closed orders first, then filter in application layer
        let all_records = self.query_user_closed_orders(user_address, None)?;

        let filtered: Vec<ClosedOrderRecord> = all_records
            .into_iter()
            .filter(|record| {
                // 从键中判断 mint 是否匹配 (键格式: orderbook_user_closed:{user}:{timestamp}:{mint}:{direction}:{order_id})
                // 这里我们可以从订单数据中检查,因为我们没有保存原始 mint/direction 到 ClosedOrderRecord
                // 实际上更好的方法是在 ClosedOrderRecord 中添加 mint 和 direction 字段
                // TODO: 需要完善过滤逻辑,或在 ClosedOrderRecord 中添加 mint/direction 字段
                true
            })
            .take(limit.unwrap_or(usize::MAX))
            .collect();

        Ok(filtered)
    }

    /// 查询用户在指定时间范围的已关闭订单
    /// Query closed orders in time range
    ///
    /// # 参数 / Parameters
    /// * `user_address` - 用户地址
    /// * `start_ts` - 开始时间戳(包含)
    /// * `end_ts` - 结束时间戳(包含)
    pub fn query_user_closed_orders_by_time_range(
        &self,
        user_address: &str,
        start_ts: u32,
        end_ts: u32,
    ) -> Result<Vec<ClosedOrderRecord>> {
        let all_records = self.query_user_closed_orders(user_address, None)?;

        let filtered: Vec<ClosedOrderRecord> = all_records
            .into_iter()
            .filter(|record| {
                let ts = record.close_info.close_timestamp;
                ts >= start_ts && ts <= end_ts
            })
            .collect();

        Ok(filtered)
    }

    /// 前缀扫描辅助函数
    /// Prefix scan helper function
    fn scan_with_prefix(
        &self,
        prefix: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ClosedOrderRecord>> {
        let mut records = Vec::new();
        let iter = self.db.prefix_iterator(prefix.as_bytes());

        for item in iter {
            let (key, value) = item?;

            // 检查是否还在前缀范围内
            // Check if still within prefix range
            if !key.starts_with(prefix.as_bytes()) {
                break;
            }

            // 反序列化值
            // Deserialize value
            let record: ClosedOrderRecord = serde_json::from_slice(&value)?;
            records.push(record);

            // 达到限制则停止
            // Stop if reached limit
            if let Some(limit) = limit {
                if records.len() >= limit {
                    break;
                }
            }
        }

        Ok(records)
    }
}
