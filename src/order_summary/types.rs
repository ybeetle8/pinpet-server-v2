// 订单汇总统计类型定义 / Order Summary Statistics Type Definitions

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 订单汇总统计数据（持久化到 RocksDB）
/// Order summary statistics (persisted to RocksDB)
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct OrderSummaryData {
    /// SUM(margin_sol_amount), lamports 原始值
    /// SUM(margin_sol_amount), raw lamports value
    pub total_margin_sol: u64,
    /// SUM(lock_lp_token_amount), lamports 原始值
    /// SUM(lock_lp_token_amount), raw lamports value
    pub total_lock_lp_token: u64,
    /// SUM(borrow_amount), lamports 原始值
    /// SUM(borrow_amount), raw lamports value
    pub total_borrow: u64,
    /// SUM(position_asset_amount), lamports 原始值（仅做空有意义）
    /// SUM(position_asset_amount), raw lamports value (only meaningful for short)
    pub total_position_asset: u64,
    /// 最后更新时间戳 / Last update timestamp
    pub last_update: i64,
}

/// 半平仓差值 / Partial close delta
/// 记录半平仓中每个字段的变化量
/// Records the change in each field during partial close
#[derive(Debug, Clone, Default)]
pub struct PartialCloseDelta {
    pub margin_sol: u64,
    pub lock_lp_token: u64,
    pub borrow: u64,
    pub position_asset: u64,
}

/// 重建结果 / Rebuild result
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RebuildResult {
    pub mint: String,
    pub long: OrderSummaryData,
    pub short: OrderSummaryData,
}
