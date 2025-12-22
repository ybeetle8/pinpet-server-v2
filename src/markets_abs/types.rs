// MarketsAbs 统计类型定义 / MarketsAbs Statistics Type Definitions
use bloomfilter::Bloom;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// 复用 volume 模块的 Period 类型
pub use crate::volume::Period;

/// 绝对钱包数统计数据 (内部存储) / Absolute Wallet Count Data (Internal Storage)
///
/// 全局去重，一个钱包在所有时间内只计数一次
/// Global deduplication, one wallet counted only once across all time
#[derive(Debug, Clone)]
pub struct MarketsAbsData {
    /// 累计钱包数量 (近似值, 误差 <10%) / Cumulative wallet count (approximate, error <10%)
    pub count: u64,
    /// 累计事件数量 / Total event count
    pub event_count: u64,
    /// 首次出现时间 / First seen timestamp
    pub first_seen: u64,
    /// 最后更新时间 / Last update timestamp
    pub last_update: u64,
    /// 全局 Bloom Filter 用于去重 / Global Bloom Filter for deduplication
    pub bloom_filter: Bloom<String>,
}

impl MarketsAbsData {
    /// 创建新的绝对钱包数统计数据 / Create new markets abs data
    ///
    /// # 参数 / Parameters
    /// * `expected_items` - 预期元素数量 / Expected number of items
    /// * `fp_rate` - 误报率 / False positive rate (e.g., 0.01 for 1%)
    pub fn new(expected_items: usize, fp_rate: f64, timestamp: u64) -> Self {
        Self {
            count: 0,
            event_count: 0,
            first_seen: timestamp,
            last_update: timestamp,
            bloom_filter: Bloom::new_for_fp_rate(expected_items, fp_rate)
                .expect("Failed to create Bloom filter"),
        }
    }

    /// 使用默认参数创建 / Create with default parameters
    /// 预期 20,000 个钱包, 1% 误报率
    /// Expected 20,000 wallets, 1% false positive rate
    pub fn default_config(timestamp: u64) -> Self {
        Self::new(20_000, 0.01, timestamp)
    }

    /// 尝试添加钱包 / Try to add wallet
    /// 返回是否为新钱包 / Returns true if wallet is new
    pub fn add_wallet(&mut self, wallet: &str, timestamp: u64) -> bool {
        let wallet_string = wallet.to_string();
        if self.bloom_filter.check(&wallet_string) {
            // 可能已存在 (或误报) / Possibly exists (or false positive)
            self.event_count += 1;
            self.last_update = timestamp;
            false
        } else {
            // 确定是新钱包 / Definitely new wallet
            self.bloom_filter.set(&wallet_string);
            self.count += 1;
            self.event_count += 1;
            self.last_update = timestamp;
            true
        }
    }
}

// 用于序列化的辅助结构 / Helper structure for serialization
#[derive(Serialize, Deserialize)]
pub(crate) struct MarketsAbsDataSerde {
    count: u64,
    event_count: u64,
    first_seen: u64,
    last_update: u64,
    bloom_bytes: Vec<u8>,  // Bloom Filter 的字节序列化
}

impl From<&MarketsAbsData> for MarketsAbsDataSerde {
    fn from(data: &MarketsAbsData) -> Self {
        Self {
            count: data.count,
            event_count: data.event_count,
            first_seen: data.first_seen,
            last_update: data.last_update,
            bloom_bytes: data.bloom_filter.to_bytes(),
        }
    }
}

impl TryFrom<MarketsAbsDataSerde> for MarketsAbsData {
    type Error = &'static str;

    fn try_from(serde_data: MarketsAbsDataSerde) -> Result<Self, Self::Error> {
        let bloom_filter = Bloom::from_bytes(serde_data.bloom_bytes)?;

        Ok(Self {
            count: serde_data.count,
            event_count: serde_data.event_count,
            first_seen: serde_data.first_seen,
            last_update: serde_data.last_update,
            bloom_filter,
        })
    }
}

/// 时间周期索引数据 / Period Index Data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PeriodIndexData {
    /// 截至该时间桶的累计钱包数 / Cumulative count up to this time bucket
    pub cumulative_count: u64,
    /// 最后更新时间 / Last update timestamp
    pub last_update: u64,
}

/// 单个币种的绝对钱包数查询响应 / Single Token MarketsAbs Query Response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenMarketsAbsResponse {
    /// 币种 mint 地址 / Token mint address
    pub mint: String,
    /// 时间周期 / Time period
    pub period: Period,
    /// 截至该时间桶的累计钱包数 / Cumulative wallet count up to this time bucket
    pub cumulative_count: u64,
    /// 累计事件数量 / Total event count
    pub event_count: u64,
    /// 首次出现时间 / First seen timestamp
    pub first_seen: u64,
    /// 最后更新时间 / Last update timestamp
    pub last_update: u64,
}

/// Top 绝对钱包数查询响应项 / Top MarketsAbs Query Response Item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TopMarketsAbsItem {
    /// 币种 mint 地址 / Token mint address
    pub mint: String,
    /// 截至该时间桶的累计钱包数 / Cumulative wallet count
    pub cumulative_count: u64,
    /// 累计事件数量 / Total event count
    pub event_count: u64,
    /// 首次出现时间 / First seen timestamp
    pub first_seen: u64,
    /// 最后更新时间 / Last update timestamp
    pub last_update: u64,
}

/// Top 绝对钱包数查询响应 / Top MarketsAbs Query Response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TopMarketsAbsResponse {
    /// 时间周期 / Time period
    pub period: Period,
    /// 时间桶 / Time bucket
    pub time_bucket: u64,
    /// Top 列表 / Top list
    pub items: Vec<TopMarketsAbsItem>,
}
