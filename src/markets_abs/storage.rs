// MarketsAbs 统计存储模块 / MarketsAbs Statistics Storage Module
use super::types::{
    MarketsAbsData, Period, PeriodIndexData, TokenMarketsAbsResponse, TopMarketsAbsItem,
    TopMarketsAbsResponse,
};
use anyhow::{Context, Result};
use rocksdb::DB;
use std::sync::Arc;
use tracing::{debug, info};

/// 绝对钱包数统计存储 / MarketsAbs Storage
pub struct MarketsAbsStorage {
    db: Arc<DB>,
}

impl MarketsAbsStorage {
    /// 创建新的绝对钱包数统计存储 / Create new markets abs storage
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    /// 更新绝对钱包数统计 / Update markets abs statistics
    ///
    /// # 参数 / Parameters
    /// * `mint` - Token mint 地址 / Token mint address
    /// * `wallet` - 钱包地址 / Wallet address
    /// * `timestamp` - 事件时间戳 / Event timestamp
    pub fn update_markets_abs(
        &self,
        mint: &str,
        wallet: &str,
        timestamp: u64,
    ) -> Result<()> {
        debug!(
            "📊 更新绝对钱包数统计 / Updating markets abs: mint={}, wallet={}",
            &mint[..8.min(mint.len())],
            &wallet[..8.min(wallet.len())]
        );

        // 1. 读取或创建全局数据 (包含全局 Bloom Filter)
        // Read or create global data (with global Bloom Filter)
        let main_key = format!("markets_abs:{}", mint);
        let mut global_data = match self.db.get(main_key.as_bytes())? {
            Some(bytes) => {
                let serde_data: super::types::MarketsAbsDataSerde =
                    bincode::deserialize(&bytes).context("Failed to deserialize markets abs data")?;
                serde_data
                    .try_into()
                    .map_err(|e| anyhow::anyhow!("Failed to convert markets abs data: {}", e))?
            }
            None => MarketsAbsData::default_config(timestamp),
        };

        // 2. 检查并添加钱包到全局 Bloom Filter
        // Check and add wallet to global Bloom Filter
        let is_new = global_data.add_wallet(wallet, timestamp);

        if is_new {
            debug!(
                "✅ 新钱包 (全局) / New wallet (global): mint={}, wallet={}, new_count={}",
                &mint[..8.min(mint.len())],
                &wallet[..8.min(wallet.len())],
                global_data.count
            );

            // 3. 保存全局数据 / Save global data
            let serde_data = super::types::MarketsAbsDataSerde::from(&global_data);
            let value =
                bincode::serialize(&serde_data).context("Failed to serialize markets abs data")?;
            self.db.put(main_key.as_bytes(), value)?;

            // 4. 更新所有时间周期的索引 / Update all time period indexes
            for period in Period::all() {
                let time_bucket = period.align_timestamp(timestamp);
                self.update_period_index(mint, period, time_bucket, &global_data)?;
                self.update_markets_abs_rank(mint, period, time_bucket, &global_data)?;
            }
        } else {
            debug!(
                "🔄 已存在钱包 (或误报) / Existing wallet (or FP): mint={}, wallet={}, count={}",
                &mint[..8.min(mint.len())],
                &wallet[..8.min(wallet.len())],
                global_data.count
            );

            // 即使不是新钱包，也要更新全局数据的 event_count 和 last_update
            // Update global data even if not a new wallet
            let serde_data = super::types::MarketsAbsDataSerde::from(&global_data);
            let value =
                bincode::serialize(&serde_data).context("Failed to serialize markets abs data")?;
            self.db.put(main_key.as_bytes(), value)?;
        }

        Ok(())
    }

    /// 更新单个时间周期的索引 / Update index for a single period
    fn update_period_index(
        &self,
        mint: &str,
        period: Period,
        time_bucket: u64,
        global_data: &MarketsAbsData,
    ) -> Result<()> {
        // 键格式: markets_abs_period:{period}:{mint}:{time_bucket:020}
        let key = format!(
            "markets_abs_period:{}:{}:{:020}",
            period.as_str(),
            mint,
            time_bucket
        );

        // 值: 截至该时间桶的累计钱包数
        let index_data = PeriodIndexData {
            cumulative_count: global_data.count,
            last_update: global_data.last_update,
        };

        let value = bincode::serialize(&index_data).context("Failed to serialize period index")?;
        self.db.put(key.as_bytes(), value)?;

        Ok(())
    }

    /// 更新绝对钱包数排序索引 / Update markets abs ranking index
    fn update_markets_abs_rank(
        &self,
        mint: &str,
        period: Period,
        time_bucket: u64,
        global_data: &MarketsAbsData,
    ) -> Result<()> {
        // 1. 删除旧的排序索引 (如果存在) / Delete old ranking index (if exists)
        let prefix = format!(
            "markets_abs_rank:{}:{:020}:",
            period.as_str(),
            time_bucket
        );
        let mut old_key_to_delete: Option<Vec<u8>> = None;

        let iter = self.db.prefix_iterator(prefix.as_bytes());
        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            // 检查是否是该 mint 的索引
            if key_str.ends_with(&format!(":{}", mint)) {
                old_key_to_delete = Some(key.to_vec());
                break;
            }
        }

        if let Some(old_key) = old_key_to_delete {
            self.db.delete(&old_key)?;
        }

        // 2. 插入新的排序索引 / Insert new ranking index
        // 累计钱包数编码为 10 位零填充整数
        // Cumulative count encoded as 10-digit zero-padded integer
        let new_key = format!(
            "markets_abs_rank:{}:{:020}:{:010}:{}",
            period.as_str(),
            time_bucket,
            global_data.count,
            mint
        );

        // 值可以为空,我们只需要键的排序 / Value can be empty, we only need key ordering
        self.db.put(new_key.as_bytes(), b"")?;

        debug!(
            "🔄 更新绝对排序索引 / Updated abs ranking index: period={}, mint={}, count={}",
            period.as_str(),
            &mint[..8.min(mint.len())],
            global_data.count
        );

        Ok(())
    }

    /// 查询单个币种的绝对钱包数 / Query markets abs for a single token
    ///
    /// # 参数 / Parameters
    /// * `mint` - Token mint 地址 / Token mint address
    /// * `period` - 时间周期 / Time period
    /// * `time_bucket` - 时间桶 (可选,默认为当前时间对齐后的时间桶) / Time bucket (optional)
    pub fn get_token_markets_abs(
        &self,
        mint: &str,
        period: Period,
        time_bucket: Option<u64>,
    ) -> Result<TokenMarketsAbsResponse> {
        // 如果没有提供时间桶,使用当前时间对齐后的时间桶
        let time_bucket = time_bucket.unwrap_or_else(|| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            period.align_timestamp(now)
        });

        // 1. 读取全局数据 / Read global data
        let main_key = format!("markets_abs:{}", mint);
        let global_data: MarketsAbsData = match self.db.get(main_key.as_bytes())? {
            Some(bytes) => {
                let serde_data: super::types::MarketsAbsDataSerde =
                    bincode::deserialize(&bytes).context("Failed to deserialize markets abs data")?;
                serde_data.try_into().map_err(|e| {
                    anyhow::anyhow!("Failed to convert markets abs data: {}", e)
                })?
            }
            None => {
                // 没有数据，返回空结果
                return Ok(TokenMarketsAbsResponse {
                    mint: mint.to_string(),
                    period,
                    cumulative_count: 0,
                    event_count: 0,
                    first_seen: 0,
                    last_update: 0,
                });
            }
        };

        // 2. 读取周期索引 (如果存在) / Read period index (if exists)
        let index_key = format!(
            "markets_abs_period:{}:{}:{:020}",
            period.as_str(),
            mint,
            time_bucket
        );

        let cumulative_count = match self.db.get(index_key.as_bytes())? {
            Some(bytes) => {
                let index_data: PeriodIndexData =
                    bincode::deserialize(&bytes).context("Failed to deserialize period index")?;
                index_data.cumulative_count
            }
            None => {
                // 如果没有周期索引，使用全局数据的 count
                global_data.count
            }
        };

        Ok(TokenMarketsAbsResponse {
            mint: mint.to_string(),
            period,
            cumulative_count,
            event_count: global_data.event_count,
            first_seen: global_data.first_seen,
            last_update: global_data.last_update,
        })
    }

    /// 查询 Top N 绝对钱包数币种 / Query Top N tokens by absolute wallet count
    ///
    /// # 参数 / Parameters
    /// * `period` - 时间周期 / Time period
    /// * `time_bucket` - 时间桶 (可选) / Time bucket (optional)
    /// * `limit` - 返回数量限制 / Limit of results to return
    pub fn get_top_markets_abs(
        &self,
        period: Period,
        time_bucket: Option<u64>,
        limit: usize,
    ) -> Result<TopMarketsAbsResponse> {
        // 如果没有提供时间桶,使用当前时间对齐后的时间桶
        let time_bucket = time_bucket.unwrap_or_else(|| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            period.align_timestamp(now)
        });

        // 键前缀: markets_abs_rank:{period}:{time_bucket:020}:
        let prefix = format!(
            "markets_abs_rank:{}:{:020}:",
            period.as_str(),
            time_bucket
        );

        debug!(
            "🔍 查询绝对排序索引 / Querying abs ranking index: prefix={}",
            prefix
        );

        // 先收集所有匹配的项,然后按累计钱包数排序 / Collect items and sort by count
        let mut items = Vec::new();
        let iter = self.db.prefix_iterator(prefix.as_bytes());

        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);

            // 检查是否仍在前缀范围内
            if !key_str.starts_with(&prefix) {
                break;
            }

            // 解析键提取 mint
            // 键格式: markets_abs_rank:{period}:{time_bucket:020}:{count:010}:{mint}
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() < 5 {
                continue;
            }

            let mint = parts[4..].join(":"); // mint 可能包含冒号

            // 读取全局数据 / Read global data
            let main_key = format!("markets_abs:{}", mint);
            if let Some(bytes) = self.db.get(main_key.as_bytes())? {
                if let Ok(serde_data) =
                    bincode::deserialize::<super::types::MarketsAbsDataSerde>(&bytes)
                {
                    if let Ok(data) = MarketsAbsData::try_from(serde_data) {
                        items.push(TopMarketsAbsItem {
                            mint: mint.to_string(),
                            cumulative_count: data.count,
                            event_count: data.event_count,
                            first_seen: data.first_seen,
                            last_update: data.last_update,
                        });
                    }
                }
            }
        }

        // 如果没有找到排序索引,尝试直接扫描主数据
        if items.is_empty() {
            debug!(
                "⚠️  未找到排序索引,直接扫描主数据 / No ranking index, scanning main data"
            );

            let main_prefix = "markets_abs:".to_string();
            let main_iter = self.db.prefix_iterator(main_prefix.as_bytes());

            for item in main_iter {
                let (key, value) = item?;
                let key_str = String::from_utf8_lossy(&key);

                if !key_str.starts_with(&main_prefix) {
                    break;
                }

                // 解析键
                let mint = key_str.strip_prefix("markets_abs:").unwrap_or("");

                if let Ok(serde_data) =
                    bincode::deserialize::<super::types::MarketsAbsDataSerde>(&value)
                {
                    if let Ok(data) = MarketsAbsData::try_from(serde_data) {
                        if data.count > 0 {
                            items.push(TopMarketsAbsItem {
                                mint: mint.to_string(),
                                cumulative_count: data.count,
                                event_count: data.event_count,
                                first_seen: data.first_seen,
                                last_update: data.last_update,
                            });
                        }
                    }
                }
            }
        }

        // 按累计钱包数从大到小排序
        items.sort_by(|a, b| b.cumulative_count.cmp(&a.cumulative_count));

        // 限制返回数量
        items.truncate(limit);

        info!(
            "📈 查询 Top {} 绝对钱包数 / Queried Top {} markets abs: period={}, time_bucket={}, found={}",
            limit,
            limit,
            period.as_str(),
            time_bucket,
            items.len()
        );

        Ok(TopMarketsAbsResponse {
            period,
            time_bucket,
            items,
        })
    }
}
