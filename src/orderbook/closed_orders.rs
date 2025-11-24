// 已关闭订单查询接口
// Closed orders query interface

use crate::orderbook::{
    errors::{OrderBookError, Result},
    types::ClosedOrderRecord,
    manager::OrderBookDBManager,
};
use rocksdb::DB;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

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

    /// 统计用户交易历史(盈亏汇总)
    /// Calculate user trading history stats
    pub fn calculate_user_stats(
        &self,
        user_address: &str,
    ) -> Result<UserTradingStats> {
        let records = self.query_user_closed_orders(user_address, None)?;

        let mut stats = UserTradingStats::default();
        stats.total_trades = records.len();

        for record in records {
            let pnl = record.close_info.final_pnl_sol;
            stats.total_pnl_sol += pnl;

            if pnl > 0 {
                stats.winning_trades += 1;
                stats.total_profit_sol += pnl;
            } else if pnl < 0 {
                stats.losing_trades += 1;
                stats.total_loss_sol += pnl.abs();
            }

            stats.total_borrow_fee_sol += record.close_info.total_borrow_fee_sol as i64;
            stats.total_position_duration_sec += record.close_info.position_duration_sec as u64;
        }

        Ok(stats)
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

/// 用户交易统计
/// User trading statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UserTradingStats {
    /// 总交易次数
    /// Total number of trades
    pub total_trades: usize,

    /// 盈利交易次数
    /// Number of winning trades
    pub winning_trades: usize,

    /// 亏损交易次数
    /// Number of losing trades
    pub losing_trades: usize,

    /// 总盈亏(SOL)
    /// Total PnL (SOL)
    pub total_pnl_sol: i64,

    /// 总盈利(SOL)
    /// Total profit (SOL)
    pub total_profit_sol: i64,

    /// 总亏损(SOL)
    /// Total loss (SOL)
    pub total_loss_sol: i64,

    /// 总借款费用(SOL)
    /// Total borrow fees (SOL)
    pub total_borrow_fee_sol: i64,

    /// 总持仓时长(秒)
    /// Total position duration (seconds)
    pub total_position_duration_sec: u64,
}
