// 交易额统计存储模块 / Volume Statistics Storage Module
use super::types::{Period, TopVolumeItem, TopVolumeResponse, TokenVolumeResponse, VolumeData};
use crate::curve_amm::CurveAMM;
use anyhow::{Context, Result};
use rocksdb::DB;
use std::sync::Arc;
use tracing::{debug, info};

/// 交易额存储 / Volume Storage
pub struct VolumeStorage {
    db: Arc<DB>,
}

impl VolumeStorage {
    /// 创建新的交易额存储 / Create new volume storage
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    /// 计算并更新交易额 / Calculate and update volume
    ///
    /// # 参数 / Parameters
    /// * `mint` - Token mint 地址 / Token mint address
    /// * `price_before` - 变动前的价格 / Price before change
    /// * `price_after` - 变动后的价格 / Price after change
    /// * `sol_price_usd` - SOL/USD 汇率 / SOL to USD exchange rate
    /// * `timestamp` - 事件时间戳 / Event timestamp
    pub fn update_volume(
        &self,
        mint: &str,
        price_before: u128,
        price_after: u128,
        sol_price_usd: f64,
        timestamp: u64,
    ) -> Result<()> {
        // 1. 计算 SOL 储备变化 / Calculate SOL reserve change
        let (sol_reserve_before, _) = CurveAMM::price_to_reserves(price_before)
            .context("Failed to calculate reserves before")?;
        let (sol_reserve_after, _) = CurveAMM::price_to_reserves(price_after)
            .context("Failed to calculate reserves after")?;

        // 2. 计算绝对值差额 / Calculate absolute difference
        let sol_reserve_delta = if sol_reserve_after >= sol_reserve_before {
            sol_reserve_after - sol_reserve_before
        } else {
            sol_reserve_before - sol_reserve_after
        };

        // 3. 转换为美元 / Convert to USD
        // SOL 储备单位是 lamports (1 SOL = 10^9 lamports)
        let sol_amount = sol_reserve_delta as f64 / 1_000_000_000.0;
        let volume_usd = sol_amount * sol_price_usd;

        debug!(
            "📊 计算交易额 / Calculated volume: mint={}, price_before={}, price_after={}, sol_delta={} lamports, volume=${:.2}",
            &mint[..8.min(mint.len())],
            price_before,
            price_after,
            sol_reserve_delta,
            volume_usd
        );

        // 4. 更新所有时间周期 / Update all time periods
        for period in Period::all() {
            let time_bucket = period.align_timestamp(timestamp);
            self.update_period_volume(mint, period, time_bucket, volume_usd, timestamp)?;

            // 同时更新排序索引 / Also update ranking index
            self.update_volume_rank(mint, period, time_bucket)?;
        }

        Ok(())
    }

    /// 更新单个时间周期的交易额 / Update volume for a single period
    fn update_period_volume(
        &self,
        mint: &str,
        period: Period,
        time_bucket: u64,
        volume_usd: f64,
        timestamp: u64,
    ) -> Result<()> {
        // 键格式: vol:{period}:{mint}:{time_bucket:020}
        // Key format: vol:{period}:{mint}:{time_bucket:020}
        let key = format!("vol:{}:{}:{:020}", period.as_str(), mint, time_bucket);

        // 读取现有数据 / Read existing data
        let mut data = match self.db.get(key.as_bytes())? {
            Some(bytes) => serde_json::from_slice::<VolumeData>(&bytes)
                .context("Failed to deserialize volume data")?,
            None => VolumeData::new(),
        };

        // 更新数据 / Update data
        data.add_volume(volume_usd, timestamp);

        // 保存数据 / Save data
        let value = serde_json::to_vec(&data).context("Failed to serialize volume data")?;
        self.db.put(key.as_bytes(), value)?;

        debug!(
            "✅ 更新周期交易额 / Updated period volume: period={}, mint={}, time_bucket={}, volume=${:.2}",
            period.as_str(),
            &mint[..8.min(mint.len())],
            time_bucket,
            data.volume
        );

        Ok(())
    }

    /// 更新交易额排序索引 / Update volume ranking index
    fn update_volume_rank(&self, mint: &str, period: Period, time_bucket: u64) -> Result<()> {
        // 1. 读取当前交易额 / Read current volume
        let main_key = format!("vol:{}:{}:{:020}", period.as_str(), mint, time_bucket);
        let data = match self.db.get(main_key.as_bytes())? {
            Some(bytes) => serde_json::from_slice::<VolumeData>(&bytes)?,
            None => return Ok(()), // 没有数据，跳过
        };

        // 2. 删除旧的排序索引 (如果存在) / Delete old ranking index (if exists)
        // 我们需要扫描找到旧的索引键并删除它
        // 键格式: vol_rank:{period}:{time_bucket:020}:{volume_encoded:020}:{mint}
        let prefix = format!("vol_rank:{}:{:020}:", period.as_str(), time_bucket);
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
        // 交易额编码为 20 位零填充整数 (美分单位)
        // Volume encoded as 20-digit zero-padded integer (in cents)
        let volume_cents = (data.volume * 100.0).round() as u64;
        let new_key = format!(
            "vol_rank:{}:{:020}:{:020}:{}",
            period.as_str(),
            time_bucket,
            volume_cents,
            mint
        );

        // 值可以为空，我们只需要键的排序 / Value can be empty, we only need key ordering
        self.db.put(new_key.as_bytes(), b"")?;

        debug!(
            "🔄 更新排序索引 / Updated ranking index: period={}, mint={}, volume=${:.2}",
            period.as_str(),
            &mint[..8.min(mint.len())],
            data.volume
        );

        Ok(())
    }

    /// 查询单个币种的交易额 / Query volume for a single token
    ///
    /// # 参数 / Parameters
    /// * `mint` - Token mint 地址 / Token mint address
    /// * `period` - 时间周期 / Time period
    /// * `time_bucket` - 时间桶 (可选，默认为当前时间对齐后的时间桶) / Time bucket (optional, defaults to current time aligned)
    pub fn get_token_volume(
        &self,
        mint: &str,
        period: Period,
        time_bucket: Option<u64>,
    ) -> Result<TokenVolumeResponse> {
        // 如果没有提供时间桶，使用当前时间对齐后的时间桶
        // If no time bucket provided, use current time aligned
        let time_bucket = time_bucket.unwrap_or_else(|| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            period.align_timestamp(now)
        });

        // 键格式: vol:{period}:{mint}:{time_bucket:020}
        let key = format!("vol:{}:{}:{:020}", period.as_str(), mint, time_bucket);

        // 读取数据 / Read data
        let data = match self.db.get(key.as_bytes())? {
            Some(bytes) => serde_json::from_slice::<VolumeData>(&bytes)
                .context("Failed to deserialize volume data")?,
            None => VolumeData::new(), // 没有数据返回空数据
        };

        Ok(TokenVolumeResponse {
            mint: mint.to_string(),
            period,
            data,
        })
    }

    /// 查询 Top N 交易额币种 / Query Top N tokens by volume
    ///
    /// # 参数 / Parameters
    /// * `period` - 时间周期 / Time period
    /// * `time_bucket` - 时间桶 (可选，默认为当前时间对齐后的时间桶) / Time bucket (optional, defaults to current time aligned)
    /// * `limit` - 返回数量限制 / Limit of results to return
    pub fn get_top_volume(
        &self,
        period: Period,
        time_bucket: Option<u64>,
        limit: usize,
    ) -> Result<TopVolumeResponse> {
        // 如果没有提供时间桶，使用当前时间对齐后的时间桶
        let time_bucket = time_bucket.unwrap_or_else(|| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            period.align_timestamp(now)
        });

        // 键前缀: vol_rank:{period}:{time_bucket:020}:
        let prefix = format!("vol_rank:{}:{:020}:", period.as_str(), time_bucket);

        // 反向迭代，从大到小 / Reverse iteration, from large to small
        let mut items = Vec::new();
        let mut iter = self.db.prefix_iterator(prefix.as_bytes());
        iter.set_mode(rocksdb::IteratorMode::End);

        let mut count = 0;
        while let Some(Ok((key, _))) = iter.next() {
            let key_str = String::from_utf8_lossy(&key);

            // 检查是否仍在前缀范围内 / Check if still in prefix range
            if !key_str.starts_with(&prefix) {
                break;
            }

            // 解析键提取 mint / Parse key to extract mint
            // 键格式: vol_rank:{period}:{time_bucket:020}:{volume_encoded:020}:{mint}
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() < 5 {
                continue;
            }

            let mint = parts[4..].join(":"); // mint 可能包含冒号

            // 读取主数据 / Read main data
            let main_key = format!("vol:{}:{}:{:020}", period.as_str(), mint, time_bucket);
            if let Some(bytes) = self.db.get(main_key.as_bytes())? {
                if let Ok(data) = serde_json::from_slice::<VolumeData>(&bytes) {
                    items.push(TopVolumeItem {
                        mint: mint.to_string(),
                        volume: data.volume,
                        event_count: data.event_count,
                        last_update: data.last_update,
                    });

                    count += 1;
                    if count >= limit {
                        break;
                    }
                }
            }
        }

        info!(
            "📈 查询 Top {} 交易额 / Queried Top {} volume: period={}, time_bucket={}, found={}",
            limit,
            limit,
            period.as_str(),
            time_bucket,
            items.len()
        );

        Ok(TopVolumeResponse {
            period,
            time_bucket,
            items,
        })
    }
}
