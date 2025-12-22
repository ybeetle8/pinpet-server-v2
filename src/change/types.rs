// 涨跌幅统计类型定义 / Change Statistics Type Definitions
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// 复用 volume 模块的 Period 定义
// Reuse Period definition from volume module
pub use crate::volume::Period;

/// 涨跌幅数据 / Change Data
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChangeData {
    /// 开盘价 (USD) / Open price in USD
    pub open_price: f64,
    /// 收盘价 (USD) / Close price in USD
    pub close_price: f64,
    /// 最高价 (USD) / Highest price in USD
    pub high_price: f64,
    /// 最低价 (USD) / Lowest price in USD
    pub low_price: f64,
    /// 涨跌幅百分比 / Change percentage
    pub change_percent: f64,
    /// 第一个事件时间 / First event timestamp
    pub first_event_time: u64,
    /// 最后一个事件时间 / Last event timestamp
    pub last_event_time: u64,
}

impl ChangeData {
    /// 创建新的涨跌幅数据 / Create new change data
    pub fn new() -> Self {
        Self {
            open_price: 0.0,
            close_price: 0.0,
            high_price: 0.0,
            low_price: 0.0,
            change_percent: 0.0,
            first_event_time: 0,
            last_event_time: 0,
        }
    }

    /// 更新价格数据 / Update price data
    ///
    /// # 参数 / Parameters
    /// * `price` - 当前价格 (USD) / Current price in USD
    /// * `timestamp` - 事件时间戳 / Event timestamp
    pub fn update_price(&mut self, price: f64, timestamp: u64) {
        // 如果是第一个事件，初始化所有价格 / If first event, initialize all prices
        if self.first_event_time == 0 {
            self.open_price = price;
            self.close_price = price;
            self.high_price = price;
            self.low_price = price;
            self.first_event_time = timestamp;
            self.last_event_time = timestamp;
        } else {
            // 更新收盘价（假设当前是最后一个事件）/ Update close price (assume current is last)
            self.close_price = price;

            // 更新最高价和最低价 / Update high and low prices
            if price > self.high_price {
                self.high_price = price;
            }
            if price < self.low_price {
                self.low_price = price;
            }

            // 更新最后事件时间 / Update last event time
            self.last_event_time = timestamp;
        }

        // 重新计算涨跌幅 / Recalculate change percentage
        self.recalculate_change();
    }

    /// 重新计算涨跌幅 / Recalculate change percentage
    fn recalculate_change(&mut self) {
        if self.open_price > 0.0 {
            self.change_percent = ((self.close_price - self.open_price) / self.open_price) * 100.0;
        } else {
            self.change_percent = 0.0;
        }
    }
}

impl Default for ChangeData {
    fn default() -> Self {
        Self::new()
    }
}

/// 单个币种的涨跌幅查询响应 / Single Token Change Query Response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenChangeResponse {
    /// 币种 mint 地址 / Token mint address
    pub mint: String,
    /// 时间周期 / Time period
    pub period: Period,
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
    /// 最高价 (USD) / Highest price in USD
    pub high_price: f64,
    /// 最低价 (USD) / Lowest price in USD
    pub low_price: f64,
    /// 涨跌幅百分比 / Change percentage
    pub change_percent: f64,
    /// 第一个事件时间 / First event timestamp
    pub first_event_time: u64,
    /// 最后一个事件时间 / Last event timestamp
    pub last_event_time: u64,
}

/// Top 涨跌幅查询方向 / Top Change Query Direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// 涨幅榜 (从高到低) / Gainers (high to low)
    Gain,
    /// 跌幅榜 (从低到高) / Losers (low to high)
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
    pub direction: Direction,
    /// Top 列表 / Top list
    pub items: Vec<TopChangeItem>,
}
