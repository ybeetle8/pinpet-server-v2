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
    #[allow(dead_code)]
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
                // 按 mint 过滤
                // Filter by mint
                let mint_match = record.mint == mint;

                // 如果指定了 direction,则同时按 direction 过滤
                // If direction is specified, also filter by direction
                let direction_match = direction
                    .map(|d| record.direction == d)
                    .unwrap_or(true);

                mint_match && direction_match
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
    #[allow(dead_code)]
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

            // 从键中解析 mint 和 direction
            // Parse mint and direction from key
            // 键格式: orderbook_user_closed:{user}:{timestamp}:{mint}:{direction}:{order_id}
            // Key format: orderbook_user_closed:{user}:{timestamp}:{mint}:{direction}:{order_id}
            let key_str = String::from_utf8_lossy(&key);

            // DEBUG: 打印键和时间戳信息 / Print key and timestamp info
            tracing::debug!("🔍 扫描键 / Scanning key: {}", key_str);

            let parts: Vec<&str> = key_str.split(':').collect();

            let (mint, direction) = if parts.len() >= 5 {
                // DEBUG: 解析并打印时间戳信息 / Parse and print timestamp info
                if let Ok(inverted_ts) = parts[2].parse::<u32>() {
                    let original_ts = u32::MAX - inverted_ts;
                    tracing::debug!(
                        "   inverted_ts={}, original_ts={}, mint={}, dir={}",
                        inverted_ts, original_ts, &parts[3][..8.min(parts[3].len())], parts[4]
                    );
                }

                (parts[3].to_string(), parts[4].to_string())
            } else {
                // 如果键格式不正确,使用默认值
                // If key format is incorrect, use default values
                ("unknown".to_string(), "unknown".to_string())
            };

            // 反序列化值
            // Deserialize value
            let mut record: ClosedOrderRecord = serde_json::from_slice(&value)?;

            // 设置 mint 和 direction
            // Set mint and direction
            record.mint = mint;
            record.direction = direction;

            records.push(record);

            // 达到限制则停止
            // Stop if reached limit
            if let Some(limit) = limit {
                if records.len() >= limit {
                    break;
                }
            }
        }

        // ✅ 时间戳反转设计已修复 / Timestamp inversion design fixed
        //
        // 根本原因: PartialClose 保存时没有使用反转时间戳,导致数据库键混乱
        // Root cause: PartialClose was not using inverted timestamps, causing DB key disorder
        //
        // 修复方案: 统一使用 OrderBookDBManager::closed_order_key() 函数保存所有 closed orders
        // Fix: Unified use of OrderBookDBManager::closed_order_key() for all closed orders
        //
        // 现在所有数据都正确使用反转时间戳,RocksDB prefix_iterator 自动按时间降序返回
        // Now all data uses inverted timestamps correctly, RocksDB prefix_iterator returns in descending order automatically
        //
        // 注意: 旧数据(修复前)仍然是错误的,需要数据迁移或清除重建
        // Note: Old data (before fix) is still incorrect, requires migration or rebuild

        Ok(records)
    }
}
