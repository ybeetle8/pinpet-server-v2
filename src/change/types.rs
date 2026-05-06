// 涨跌幅统计类型定义 / Change Statistics Type Definitions
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

// 复用 Volume 模块的 Period 枚举
// Reuse Period enum from Volume module
pub use crate::volume::Period;

/// 涨跌幅数据 / Change Data
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChangeData {
    /// 开盘价 (USD) / Open price in USD
    pub open_price: f64,
    /// 收盘价 (USD) / Close price in USD
    pub close_price: f64,
    /// 涨跌幅百分比 / Change percentage
    pub change_percent: f64,
    /// 首个事件时间 / First event timestamp
    pub first_event_time: u64,
    /// 最后事件时间 / Last event timestamp
    pub last_event_time: u64,
}

impl ChangeData {
    /// 创建新的涨跌幅数据 / Create new change data
    pub fn new(open_price: f64, timestamp: u64) -> Self {
        Self {
            open_price,
            close_price: open_price,
            change_percent: 0.0,
            first_event_time: timestamp,
            last_event_time: timestamp,
        }
    }

    /// 更新收盘价并重新计算涨跌幅 / Update close price and recalculate change percent
    pub fn update_close_price(&mut self, close_price: f64, timestamp: u64) {
        self.close_price = close_price;
        self.last_event_time = timestamp;

        // 计算涨跌幅: (close - open) / open * 100
        // Calculate change percent: (close - open) / open * 100
        if self.open_price > 0.0 {
            self.change_percent = ((self.close_price - self.open_price) / self.open_price) * 100.0;
        } else {
            self.change_percent = 0.0;
        }
    }
}

/// 单个币种的涨跌幅查询响应 / Single Token Change Query Response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenChangeResponse {
    /// 币种 mint 地址 / Token mint address
    pub mint: String,
    /// 时间周期 / Time period
    pub period: Period,
    /// 时间桶 / Time bucket
    pub time_bucket: u64,
    /// 涨跌幅数据 / Change data
    #[serde(flatten)]
    pub data: ChangeData,
}

/// Top 涨跌幅查询响应项 / Top Change Query Response Item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TopChangeItem {
    /// 币种 mint 地址 / Token mint address
    pub mint: String,
    /// 开盘价 (USD) / Open price in USD
    pub open_price: f64,
    /// 收盘价 (USD) / Close price in USD
    pub close_price: f64,
    /// 涨跌幅百分比 / Change percentage
    pub change_percent: f64,
    /// 首个事件时间 / First event timestamp
    pub first_event_time: u64,
    /// 最后事件时间 / Last event timestamp
    pub last_event_time: u64,
}

/// 涨跌幅方向 / Change Direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChangeDirection {
    /// 涨幅榜 (正向排序,从高到低) / Gainers (positive, descending)
    Gain,
    /// 跌幅榜 (负向排序,从低到高) / Losers (negative, ascending)
    Loss,
}

/// Top 涨跌幅查询响应 / Top Change Query Response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TopChangeResponse {
    /// 时间周期 / Time period
    pub period: Period,
    /// 时间桶 / Time bucket
    pub time_bucket: u64,
    /// 查询方向 / Query direction
    pub direction: ChangeDirection,
    /// Top 列表 / Top list
    pub items: Vec<TopChangeItem>,
}

// ============================================================================
// 滚动24小时窗口数据结构 / Rolling 24h Window Data Structures
// ============================================================================

/// 单个小时槽位的价格数据 / Single hour slot price data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSlot {
    /// 该小时第一个事件的价格 / First event price in this hour
    pub open_price: f64,
    /// 该小时最后一个事件的价格 / Last event price in this hour
    pub close_price: f64,
    /// 该小时第一个事件时间 / First event timestamp in this hour
    pub first_event_time: u64,
    /// 该小时最后一个事件时间 / Last event timestamp in this hour
    pub last_event_time: u64,
}

/// 滚动24h涨跌幅数据 / Rolling 24h change data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollingChangeData {
    /// 各小时槽位，key = 1h 对齐的 timestamp / Hour slots, key = 1h-aligned timestamp
    /// 最多 25 个槽位 / Max 25 slots
    pub slots: HashMap<u64, ChangeSlot>,
    /// 最后更新时间 / Last update timestamp
    pub last_update: u64,
}

impl RollingChangeData {
    /// 创建空的滚动数据 / Create empty rolling data
    pub fn new() -> Self {
        Self {
            slots: HashMap::new(),
            last_update: 0,
        }
    }
}

impl Default for RollingChangeData {
    fn default() -> Self {
        Self::new()
    }
}

/// 滚动24h涨跌幅查询响应 / Rolling 24h change query response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RollingChangeResponse {
    /// 币种 mint 地址 / Token mint address
    pub mint: String,
    /// 开盘价 / Open price
    pub open_price: f64,
    /// 收盘价 / Close price
    pub close_price: f64,
    /// 涨跌幅百分比 / Change percentage
    pub change_percent: f64,
    /// 最后更新时间 / Last update timestamp
    pub last_update: u64,
}

impl RollingChangeResponse {
    /// 创建空响应 / Create empty response
    pub fn empty(mint: &str) -> Self {
        Self {
            mint: mint.to_string(),
            open_price: 0.0,
            close_price: 0.0,
            change_percent: 0.0,
            last_update: 0,
        }
    }
}

/// Top 滚动涨跌幅查询响应 / Top rolling change query response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TopRollingChangeResponse {
    /// Top 列表 / Top list
    pub items: Vec<TopChangeItem>,
}
