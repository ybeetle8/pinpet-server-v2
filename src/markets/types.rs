// Markets 统计类型定义 / Markets Statistics Type Definitions
use bloomfilter::Bloom;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// 复用 volume 模块的 Period 类型
pub use crate::volume::Period;

/// 钱包数统计数据 (内部存储) / Wallet Count Data (Internal Storage)
#[derive(Debug, Clone)]
pub struct MarketsData {
    /// 钱包数量 (近似值, 误差 ~1%) / Wallet count (approximate, ~1% error)
    pub count: u64,
    /// 事件数量 / Event count
    pub event_count: u64,
    /// 最后更新时间 / Last update timestamp
    pub last_update: u64,
    /// Bloom Filter 用于去重 / Bloom Filter for deduplication
    pub bloom_filter: Bloom<String>,
}

impl MarketsData {
    /// 创建新的钱包数统计数据 / Create new markets data
    ///
    /// # 参数 / Parameters
    /// * `expected_items` - 预期元素数量 / Expected number of items
    /// * `fp_rate` - 误报率 / False positive rate (e.g., 0.01 for 1%)
    pub fn new(expected_items: usize, fp_rate: f64) -> Self {
        Self {
            count: 0,
            event_count: 0,
            last_update: 0,
            bloom_filter: Bloom::new_for_fp_rate(expected_items, fp_rate)
                .expect("Failed to create Bloom filter"),
        }
    }

    /// 使用默认参数创建 / Create with default parameters
    /// 预期 10,000 个钱包, 1% 误报率
    /// Expected 10,000 wallets, 1% false positive rate
    pub fn default_config() -> Self {
        Self::new(10_000, 0.01)
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

impl Default for MarketsData {
    fn default() -> Self {
        Self::default_config()
    }
}

// 用于序列化的辅助结构 / Helper structure for serialization
#[derive(Serialize, Deserialize)]
pub(crate) struct MarketsDataSerde {
    count: u64,
    event_count: u64,
    last_update: u64,
    bloom_bytes: Vec<u8>,  // Bloom Filter 的字节序列化
}

impl From<&MarketsData> for MarketsDataSerde {
    fn from(data: &MarketsData) -> Self {
        Self {
            count: data.count,
            event_count: data.event_count,
            last_update: data.last_update,
            bloom_bytes: data.bloom_filter.to_bytes(),
        }
    }
}

impl TryFrom<MarketsDataSerde> for MarketsData {
    type Error = &'static str;

    fn try_from(serde_data: MarketsDataSerde) -> Result<Self, Self::Error> {
        let bloom_filter = Bloom::from_bytes(serde_data.bloom_bytes)?;

        Ok(Self {
            count: serde_data.count,
            event_count: serde_data.event_count,
            last_update: serde_data.last_update,
            bloom_filter,
        })
    }
}

/// 单个币种的钱包数查询响应 / Single Token Markets Query Response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenMarketsResponse {
    /// 币种 mint 地址 / Token mint address
    pub mint: String,
    /// 时间周期 / Time period
    pub period: Period,
    /// 钱包数量 (近似值, 误差 ~1%) / Wallet count (approximate, ~1% error)
    pub count: u64,
    /// 事件数量 / Event count
    pub event_count: u64,
    /// 最后更新时间 / Last update timestamp
    pub last_update: u64,
}

/// Top 钱包数查询响应项 / Top Markets Query Response Item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TopMarketsItem {
    /// 币种 mint 地址 / Token mint address
    pub mint: String,
    /// 钱包数量 (近似值, 误差 ~1%) / Wallet count (approximate, ~1% error)
    pub count: u64,
    /// 事件数量 / Event count
    pub event_count: u64,
    /// 最后更新时间 / Last update timestamp
    pub last_update: u64,
}

/// Top 钱包数查询响应 / Top Markets Query Response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TopMarketsResponse {
    /// 时间周期 / Time period
    pub period: Period,
    /// 时间桶 / Time bucket
    pub time_bucket: u64,
    /// Top 列表 / Top list
    pub items: Vec<TopMarketsItem>,
}
