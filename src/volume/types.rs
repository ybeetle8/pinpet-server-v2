// 交易额统计类型定义 / Volume Statistics Type Definitions
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 时间周期枚举 / Time Period Enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum Period {
    #[serde(rename = "1m")]
    OneMinute,
    #[serde(rename = "5m")]
    FiveMinutes,
    #[serde(rename = "15m")]
    FifteenMinutes,
    #[serde(rename = "1h")]
    OneHour,
    #[serde(rename = "4h")]
    FourHours,
    #[serde(rename = "24h")]
    TwentyFourHours,
}

impl Period {
    /// 获取所有周期 / Get all periods
    pub fn all() -> [Period; 6] {
        [
            Period::OneMinute,
            Period::FiveMinutes,
            Period::FifteenMinutes,
            Period::OneHour,
            Period::FourHours,
            Period::TwentyFourHours,
        ]
    }

    /// 转换为字符串 / Convert to string
    pub fn as_str(&self) -> &'static str {
        match self {
            Period::OneMinute => "1m",
            Period::FiveMinutes => "5m",
            Period::FifteenMinutes => "15m",
            Period::OneHour => "1h",
            Period::FourHours => "4h",
            Period::TwentyFourHours => "24h",
        }
    }

    /// 获取周期的秒数 / Get period duration in seconds
    pub fn seconds(&self) -> u64 {
        match self {
            Period::OneMinute => 60,
            Period::FiveMinutes => 300,
            Period::FifteenMinutes => 900,
            Period::OneHour => 3600,
            Period::FourHours => 14400,
            Period::TwentyFourHours => 86400,
        }
    }

    /// 对齐时间戳到周期边界 / Align timestamp to period boundary
    pub fn align_timestamp(&self, timestamp: u64) -> u64 {
        let seconds = self.seconds();
        (timestamp / seconds) * seconds
    }
}

/// 交易额数据 / Volume Data
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VolumeData {
    /// 交易额 (USD) / Volume in USD
    pub volume: f64,
    /// 事件数量 / Event count
    pub event_count: u64,
    /// 最后更新时间 / Last update timestamp
    pub last_update: u64,
}

impl VolumeData {
    /// 创建新的交易额数据 / Create new volume data
    pub fn new() -> Self {
        Self {
            volume: 0.0,
            event_count: 0,
            last_update: 0,
        }
    }

    /// 添加交易额 / Add volume
    pub fn add_volume(&mut self, volume_usd: f64, timestamp: u64) {
        self.volume += volume_usd;
        self.event_count += 1;
        self.last_update = timestamp;
    }
}

impl Default for VolumeData {
    fn default() -> Self {
        Self::new()
    }
}

/// 单个币种的交易额查询响应 / Single Token Volume Query Response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenVolumeResponse {
    /// 币种 mint 地址 / Token mint address
    pub mint: String,
    /// 时间周期 / Time period
    pub period: Period,
    /// 交易额数据 / Volume data
    #[serde(flatten)]
    pub data: VolumeData,
}

/// Top 交易额查询响应项 / Top Volume Query Response Item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TopVolumeItem {
    /// 币种 mint 地址 / Token mint address
    pub mint: String,
    /// 交易额 (USD) / Volume in USD
    pub volume: f64,
    /// 事件数量 / Event count
    pub event_count: u64,
    /// 最后更新时间 / Last update timestamp
    pub last_update: u64,
}

/// Top 交易额查询响应 / Top Volume Query Response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TopVolumeResponse {
    /// 时间周期 / Time period
    pub period: Period,
    /// 时间桶 / Time bucket
    pub time_bucket: u64,
    /// Top 列表 / Top list
    pub items: Vec<TopVolumeItem>,
}
