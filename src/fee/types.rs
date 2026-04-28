// 手续费统计类型定义 / Fee Statistics Type Definitions
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 手续费累计统计数据
/// Accumulated fee statistics data
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct FeeData {
    /// 现货交易手续费累计 (lamports)
    /// Accumulated spot trading fees (lamports)
    pub swap_fee_total: u64,

    /// 保证金交易手续费累计 (lamports)
    /// Accumulated margin trading fees (lamports)
    pub borrow_fee_total: u64,

    /// 强制平仓手续费累计 (lamports)
    /// Accumulated liquidation fees (lamports)
    pub liquidate_fee_total: u64,

    /// 总手续费累计 (lamports) = swap + borrow + liquidate
    /// Total accumulated fees (lamports)
    pub total_fee: u64,

    /// 事件计数
    /// Event count
    pub event_count: u64,

    /// 最后更新时间戳
    /// Last update timestamp
    pub last_update: u64,
}

impl FeeData {
    /// 创建新的手续费数据 / Create new fee data
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加现货手续费 / Add swap fee
    pub fn add_swap_fee(&mut self, fee_lamports: u64, timestamp: u64) {
        self.swap_fee_total = self.swap_fee_total.saturating_add(fee_lamports);
        self.total_fee = self.total_fee.saturating_add(fee_lamports);
        self.event_count += 1;
        self.last_update = timestamp;
    }

    /// 添加保证金手续费 / Add borrow fee
    pub fn add_borrow_fee(&mut self, fee_lamports: u64, timestamp: u64) {
        self.borrow_fee_total = self.borrow_fee_total.saturating_add(fee_lamports);
        self.total_fee = self.total_fee.saturating_add(fee_lamports);
        self.event_count += 1;
        self.last_update = timestamp;
    }

    /// 添加强制平仓手续费 / Add liquidation fee
    pub fn add_liquidate_fee(&mut self, fee_lamports: u64, timestamp: u64) {
        self.liquidate_fee_total = self.liquidate_fee_total.saturating_add(fee_lamports);
        self.total_fee = self.total_fee.saturating_add(fee_lamports);
        self.event_count += 1;
        self.last_update = timestamp;
    }
}

/// 手续费类型枚举 / Fee type enum
#[derive(Debug, Clone, Copy)]
pub enum FeeType {
    /// 现货交易手续费 / Spot trading fee
    Swap,
    /// 保证金交易手续费 / Margin trading fee
    Borrow,
    /// 强制平仓手续费 / Liquidation fee
    Liquidate,
}
