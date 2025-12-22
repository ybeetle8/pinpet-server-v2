// Markets 统计存储模块 / Markets Statistics Storage Module
use super::types::{MarketsData, Period, TokenMarketsResponse, TopMarketsItem, TopMarketsResponse};
use anyhow::{Context, Result};
use rocksdb::DB;
use std::sync::Arc;
use tracing::{debug, info};

/// 钱包数统计存储 / Markets Storage
pub struct MarketsStorage {
    db: Arc<DB>,
}

impl MarketsStorage {
    /// 创建新的钱包数统计存储 / Create new markets storage
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    /// 更新钱包数统计 / Update markets statistics
    ///
    /// # 参数 / Parameters
    /// * `mint` - Token mint 地址 / Token mint address
    /// * `wallet` - 钱包地址 / Wallet address
    /// * `timestamp` - 事件时间戳 / Event timestamp
    pub fn update_markets(
        &self,
        mint: &str,
        wallet: &str,
        timestamp: u64,
    ) -> Result<()> {
        debug!(
            "📊 更新钱包数统计 / Updating markets: mint={}, wallet={}",
            &mint[..8.min(mint.len())],
            &wallet[..8.min(wallet.len())]
        );

        // 更新所有时间周期 / Update all time periods
        for period in Period::all() {
            let time_bucket = period.align_timestamp(timestamp);
            self.update_period_markets(mint, wallet, period, time_bucket, timestamp)?;

            // 同时更新排序索引 / Also update ranking index
            self.update_markets_rank(mint, period, time_bucket)?;
        }

        Ok(())
    }

    /// 更新单个时间周期的钱包数 / Update markets for a single period
    fn update_period_markets(
        &self,
        mint: &str,
        wallet: &str,
        period: Period,
        time_bucket: u64,
        timestamp: u64,
    ) -> Result<()> {
        // 键格式: markets:{period}:{mint}:{time_bucket:020}
        let main_key = format!("markets:{}:{}:{:020}", period.as_str(), mint, time_bucket);

        // 读取现有数据 (包含 Bloom Filter) / Read existing data (with Bloom Filter)
        let mut data = match self.db.get(main_key.as_bytes())? {
            Some(bytes) => {
                let serde_data: super::types::MarketsDataSerde = bincode::deserialize(&bytes)
                    .context("Failed to deserialize markets data")?;
                serde_data.try_into()
                    .map_err(|e| anyhow::anyhow!("Failed to convert markets data: {}", e))?
            }
            None => MarketsData::default_config(), // 使用默认配置创建新的 Bloom Filter
        };

        // 使用 Bloom Filter 检查并添加钱包 / Check and add wallet using Bloom Filter
        let is_new = data.add_wallet(wallet, timestamp);

        if is_new {
            debug!(
                "✅ 新钱包 / New wallet: period={}, mint={}, wallet={}, new_count={}",
                period.as_str(),
                &mint[..8.min(mint.len())],
                &wallet[..8.min(wallet.len())],
                data.count
            );
        } else {
            debug!(
                "🔄 已存在钱包 (或误报) / Existing wallet (or FP): period={}, mint={}, wallet={}, count={}",
                period.as_str(),
                &mint[..8.min(mint.len())],
                &wallet[..8.min(wallet.len())],
                data.count
            );
        }

        // 保存数据 (使用 bincode 序列化,更紧凑) / Save data (using bincode, more compact)
        let serde_data = super::types::MarketsDataSerde::from(&data);
        let value = bincode::serialize(&serde_data)
            .context("Failed to serialize markets data")?;
        self.db.put(main_key.as_bytes(), value)?;

        Ok(())
    }

    /// 更新钱包数排序索引 / Update markets ranking index
    fn update_markets_rank(&self, mint: &str, period: Period, time_bucket: u64) -> Result<()> {
        // 1. 读取当前钱包数 / Read current wallet count
        let main_key = format!("markets:{}:{}:{:020}", period.as_str(), mint, time_bucket);
        let data: MarketsData = match self.db.get(main_key.as_bytes())? {
            Some(bytes) => {
                let serde_data: super::types::MarketsDataSerde = bincode::deserialize(&bytes)?;
                serde_data.try_into()
                    .map_err(|e: &str| anyhow::anyhow!("Failed to convert: {}", e))?
            }
            None => return Ok(()), // 没有数据,跳过
        };

        // 2. 删除旧的排序索引 (如果存在) / Delete old ranking index (if exists)
        let prefix = format!("markets_rank:{}:{:020}:", period.as_str(), time_bucket);
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

        // 3. 插入新的排序索引 / Insert new ranking index
        // 钱包数编码为 10 位零填充整数
        // Wallet count encoded as 10-digit zero-padded integer
        let new_key = format!(
            "markets_rank:{}:{:020}:{:010}:{}",
            period.as_str(),
            time_bucket,
            data.count,
            mint
        );

        // 值可以为空,我们只需要键的排序 / Value can be empty, we only need key ordering
        self.db.put(new_key.as_bytes(), b"")?;

        debug!(
            "🔄 更新排序索引 / Updated ranking index: period={}, mint={}, count={}",
            period.as_str(),
            &mint[..8.min(mint.len())],
            data.count
        );

        Ok(())
    }

    /// 查询单个币种的钱包数 / Query markets for a single token
    ///
    /// # 参数 / Parameters
    /// * `mint` - Token mint 地址 / Token mint address
    /// * `period` - 时间周期 / Time period
    /// * `time_bucket` - 时间桶 (可选,默认为当前时间对齐后的时间桶) / Time bucket (optional)
    pub fn get_token_markets(
        &self,
        mint: &str,
        period: Period,
        time_bucket: Option<u64>,
    ) -> Result<TokenMarketsResponse> {
        // 如果没有提供时间桶,使用当前时间对齐后的时间桶
        let time_bucket = time_bucket.unwrap_or_else(|| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            period.align_timestamp(now)
        });

        // 键格式: markets:{period}:{mint}:{time_bucket:020}
        let key = format!("markets:{}:{}:{:020}", period.as_str(), mint, time_bucket);

        // 读取数据 / Read data
        let data = match self.db.get(key.as_bytes())? {
            Some(bytes) => {
                let serde_data: super::types::MarketsDataSerde = bincode::deserialize(&bytes)
                    .context("Failed to deserialize markets data")?;
                serde_data.try_into()
                    .map_err(|e| anyhow::anyhow!("Failed to convert markets data: {}", e))?
            }
            None => MarketsData::default_config(), // 没有数据返回空数据
        };

        Ok(TokenMarketsResponse {
            mint: mint.to_string(),
            period,
            count: data.count,
            event_count: data.event_count,
            last_update: data.last_update,
        })
    }

    /// 查询 Top N 钱包数币种 / Query Top N tokens by wallet count
    ///
    /// # 参数 / Parameters
    /// * `period` - 时间周期 / Time period
    /// * `time_bucket` - 时间桶 (可选) / Time bucket (optional)
    /// * `limit` - 返回数量限制 / Limit of results to return
    pub fn get_top_markets(
        &self,
        period: Period,
        time_bucket: Option<u64>,
        limit: usize,
    ) -> Result<TopMarketsResponse> {
        // 如果没有提供时间桶,使用当前时间对齐后的时间桶
        let time_bucket = time_bucket.unwrap_or_else(|| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            period.align_timestamp(now)
        });

        // 键前缀: markets_rank:{period}:{time_bucket:020}:
        let prefix = format!("markets_rank:{}:{:020}:", period.as_str(), time_bucket);

        debug!(
            "🔍 查询排序索引 / Querying ranking index: prefix={}",
            prefix
        );

        // 先收集所有匹配的项,然后按钱包数排序 / Collect items and sort by count
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
            // 键格式: markets_rank:{period}:{time_bucket:020}:{count:010}:{mint}
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() < 5 {
                continue;
            }

            let mint = parts[4..].join(":"); // mint 可能包含冒号

            // 读取主数据 / Read main data
            let main_key = format!("markets:{}:{}:{:020}", period.as_str(), mint, time_bucket);
            if let Some(bytes) = self.db.get(main_key.as_bytes())? {
                if let Ok(serde_data) = bincode::deserialize::<super::types::MarketsDataSerde>(&bytes) {
                    if let Ok(data) = MarketsData::try_from(serde_data) {
                        items.push(TopMarketsItem {
                            mint: mint.to_string(),
                            count: data.count,
                            event_count: data.event_count,
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

            let markets_prefix = format!("markets:{}:", period.as_str());
            let markets_iter = self.db.prefix_iterator(markets_prefix.as_bytes());

            for item in markets_iter {
                let (key, value) = item?;
                let key_str = String::from_utf8_lossy(&key);

                if !key_str.starts_with(&markets_prefix) {
                    break;
                }

                // 解析键
                let parts: Vec<&str> = key_str.split(':').collect();
                if parts.len() < 4 {
                    continue;
                }

                // 检查 time_bucket
                if let Ok(tb) = parts[parts.len() - 1].parse::<u64>() {
                    if tb != time_bucket {
                        continue;
                    }

                    let mint = parts[2..parts.len() - 1].join(":");

                    if let Ok(serde_data) = bincode::deserialize::<super::types::MarketsDataSerde>(&value) {
                        if let Ok(data) = MarketsData::try_from(serde_data) {
                            if data.count > 0 {
                                items.push(TopMarketsItem {
                                    mint,
                                    count: data.count,
                                    event_count: data.event_count,
                                    last_update: data.last_update,
                                });
                            }
                        }
                    }
                }
            }
        }

        // 按钱包数从大到小排序
        items.sort_by(|a, b| b.count.cmp(&a.count));

        // 限制返回数量
        items.truncate(limit);

        info!(
            "📈 查询 Top {} 钱包数 / Queried Top {} markets: period={}, time_bucket={}, found={}",
            limit,
            limit,
            period.as_str(),
            time_bucket,
            items.len()
        );

        Ok(TopMarketsResponse {
            period,
            time_bucket,
            items,
        })
    }
}
