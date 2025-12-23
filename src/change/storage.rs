// 涨跌幅统计存储模块 / Change Statistics Storage Module
use super::types::{
    ChangeData, ChangeDirection, Period, TokenChangeResponse, TopChangeItem, TopChangeResponse,
};
use anyhow::{Context, Result};
use rocksdb::DB;
use std::sync::Arc;
use tracing::{debug, info};

/// 涨跌幅存储 / Change Storage
pub struct ChangeStorage {
    db: Arc<DB>,
}

impl ChangeStorage {
    /// 创建新的涨跌幅存储 / Create new change storage
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    /// 更新涨跌幅数据 / Update change data
    ///
    /// # 参数 / Parameters
    /// * `mint` - Token mint 地址 / Token mint address
    /// * `price` - 当前价格 (USD) / Current price in USD
    /// * `timestamp` - 事件时间戳 / Event timestamp
    pub fn update_change(&self, mint: &str, price: f64, timestamp: u64) -> Result<()> {
        debug!(
            "📊 更新涨跌幅 / Updating change: mint={}, price=${:.9}, timestamp={}",
            &mint[..8.min(mint.len())],
            price,
            timestamp
        );

        // 更新所有时间周期 / Update all time periods
        for period in Period::all() {
            let time_bucket = period.align_timestamp(timestamp);
            self.update_period_change(mint, period, time_bucket, price, timestamp)?;

            // 同时更新排序索引 / Also update ranking index
            self.update_change_rank(mint, period, time_bucket)?;
        }

        Ok(())
    }

    /// 更新单个时间周期的涨跌幅 / Update change for a single period
    fn update_period_change(
        &self,
        mint: &str,
        period: Period,
        time_bucket: u64,
        price: f64,
        timestamp: u64,
    ) -> Result<()> {
        // 键格式: change:{period}:{mint}:{time_bucket:020}
        // Key format: change:{period}:{mint}:{time_bucket:020}
        let key = format!("change:{}:{}:{:020}", period.as_str(), mint, time_bucket);

        // 读取现有数据 / Read existing data
        let mut data = match self.db.get(key.as_bytes())? {
            Some(bytes) => serde_json::from_slice::<ChangeData>(&bytes)
                .context("Failed to deserialize change data")?,
            None => {
                // 首次记录,设置 open_price / First record, set open_price
                ChangeData::new(price, timestamp)
            }
        };

        // 更新 close_price 并重新计算涨跌幅 / Update close_price and recalculate change
        data.update_close_price(price, timestamp);

        // 保存数据 / Save data
        let value = serde_json::to_vec(&data).context("Failed to serialize change data")?;
        self.db.put(key.as_bytes(), value)?;

        debug!(
            "✅ 更新周期涨跌幅 / Updated period change: period={}, mint={}, time_bucket={}, change={:.2}%",
            period.as_str(),
            &mint[..8.min(mint.len())],
            time_bucket,
            data.change_percent
        );

        Ok(())
    }

    /// 更新涨跌幅排序索引 / Update change ranking index
    fn update_change_rank(&self, mint: &str, period: Period, time_bucket: u64) -> Result<()> {
        // 1. 读取当前涨跌幅 / Read current change
        let main_key = format!("change:{}:{}:{:020}", period.as_str(), mint, time_bucket);
        let data = match self.db.get(main_key.as_bytes())? {
            Some(bytes) => serde_json::from_slice::<ChangeData>(&bytes)?,
            None => return Ok(()), // 没有数据,跳过 / No data, skip
        };

        // 2. 删除旧的排序索引 (如果存在) / Delete old ranking index (if exists)
        // 键格式: change_rank:{period}:{time_bucket:020}:{change_percent_encoded:+012}:{mint}
        let prefix = format!("change_rank:{}:{:020}:", period.as_str(), time_bucket);
        let mut old_key_to_delete: Option<Vec<u8>> = None;

        let iter = self.db.prefix_iterator(prefix.as_bytes());
        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);

            // 检查前缀范围 / Check prefix range
            if !key_str.starts_with(&prefix) {
                break;
            }

            // 检查是否是该 mint 的索引 / Check if it's the index for this mint
            if key_str.ends_with(&format!(":{}", mint)) {
                old_key_to_delete = Some(key.to_vec());
                break;
            }
        }

        if let Some(old_key) = old_key_to_delete {
            self.db.delete(&old_key)?;
        }

        // 3. 插入新的排序索引 / Insert new ranking index
        // 涨跌幅编码: 符号(1位) + 数值(11位)
        // Change encoding: sign(1) + value(11)
        // 例: +123.45% -> +0000012345, -67.89% -> -0000006789
        let change_encoded = Self::encode_change_percent(data.change_percent);
        let new_key = format!(
            "change_rank:{}:{:020}:{}:{}",
            period.as_str(),
            time_bucket,
            change_encoded,
            mint
        );

        // 值可以为空,我们只需要键的排序 / Value can be empty, we only need key ordering
        self.db.put(new_key.as_bytes(), b"")?;

        debug!(
            "🔄 更新涨跌幅排序索引 / Updated change ranking index: period={}, mint={}, change={:.2}%",
            period.as_str(),
            &mint[..8.min(mint.len())],
            data.change_percent
        );

        Ok(())
    }

    /// 编码涨跌幅百分比为固定长度字符串 / Encode change percent to fixed-length string
    ///
    /// # 格式 / Format
    /// 符号(1位) + 数值(11位,零填充)
    /// Sign(1) + Value(11, zero-padded)
    ///
    /// # 示例 / Examples
    /// * +123.45% -> "+0000012345"
    /// * -67.89%  -> "-0000006789"
    /// * +5.67%   -> "+0000000567"
    fn encode_change_percent(change_percent: f64) -> String {
        // 将百分比乘以 100,保留 2 位小数
        // Multiply percentage by 100, keep 2 decimal places
        let value = (change_percent.abs() * 100.0).round() as i64;
        let sign = if change_percent >= 0.0 { '+' } else { '-' };

        format!("{}{:011}", sign, value)
    }

    /// 解码涨跌幅字符串为百分比 / Decode change string to percentage
    #[allow(dead_code)]
    fn decode_change_percent(encoded: &str) -> Result<f64> {
        if encoded.len() != 12 {
            anyhow::bail!("Invalid encoded change percent length");
        }

        let sign = &encoded[0..1];
        let value_str = &encoded[1..];
        let value = value_str
            .parse::<i64>()
            .context("Failed to parse change value")?;

        let change = value as f64 / 100.0;
        Ok(if sign == "+" { change } else { -change })
    }

    /// 查询单个币种的涨跌幅 / Query change for a single token
    ///
    /// # 参数 / Parameters
    /// * `mint` - Token mint 地址 / Token mint address
    /// * `period` - 时间周期 / Time period
    /// * `time_bucket` - 时间桶 (可选,默认为当前时间对齐后的时间桶) / Time bucket (optional)
    pub fn get_token_change(
        &self,
        mint: &str,
        period: Period,
        time_bucket: Option<u64>,
    ) -> Result<TokenChangeResponse> {
        // 如果没有提供时间桶,使用当前时间对齐后的时间桶
        // If no time bucket provided, use current time aligned
        let time_bucket = time_bucket.unwrap_or_else(|| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            period.align_timestamp(now)
        });

        // 键格式: change:{period}:{mint}:{time_bucket:020}
        let key = format!("change:{}:{}:{:020}", period.as_str(), mint, time_bucket);

        // 读取数据 / Read data
        let data = match self.db.get(key.as_bytes())? {
            Some(bytes) => serde_json::from_slice::<ChangeData>(&bytes)
                .context("Failed to deserialize change data")?,
            None => {
                // 没有数据返回零值 / No data, return zero values
                ChangeData {
                    open_price: 0.0,
                    close_price: 0.0,
                    change_percent: 0.0,
                    first_event_time: 0,
                    last_event_time: 0,
                }
            }
        };

        Ok(TokenChangeResponse {
            mint: mint.to_string(),
            period,
            time_bucket,
            data,
        })
    }

    /// 查询 Top N 涨跌幅币种 / Query Top N tokens by change
    ///
    /// # 参数 / Parameters
    /// * `period` - 时间周期 / Time period
    /// * `time_bucket` - 时间桶 (可选,默认为当前时间对齐后的时间桶) / Time bucket (optional)
    /// * `direction` - 查询方向 (涨幅或跌幅) / Query direction (gain or loss)
    /// * `limit` - 返回数量限制 / Limit of results to return
    pub fn get_top_change(
        &self,
        period: Period,
        time_bucket: Option<u64>,
        direction: ChangeDirection,
        limit: usize,
    ) -> Result<TopChangeResponse> {
        // 如果没有提供时间桶,使用当前时间对齐后的时间桶
        let time_bucket = time_bucket.unwrap_or_else(|| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            period.align_timestamp(now)
        });

        // 键前缀: change_rank:{period}:{time_bucket:020}:
        let prefix = format!("change_rank:{}:{:020}:", period.as_str(), time_bucket);

        debug!(
            "🔍 查询涨跌幅排序索引 / Querying change ranking index: prefix={}, direction={:?}",
            prefix, direction
        );

        let mut items = Vec::new();

        match direction {
            ChangeDirection::Gain => {
                // 涨幅榜: 从最大正值开始,正向迭代
                // Gainers: Start from max positive, iterate forward
                // 查找所有以 '+' 开头的键
                let gain_prefix = format!("{}+", prefix);
                let iter = self.db.prefix_iterator(gain_prefix.as_bytes());

                for item in iter {
                    let (key, _) = item?;
                    let key_str = String::from_utf8_lossy(&key);

                    // 检查是否仍在前缀范围内
                    if !key_str.starts_with(&gain_prefix) {
                        break;
                    }

                    // 解析键提取 mint
                    if let Some(mint) = Self::extract_mint_from_rank_key(&key_str) {
                        // 读取主数据
                        if let Some(change_item) = self.read_change_data(mint, period, time_bucket)? {
                            items.push(change_item);
                            if items.len() >= limit {
                                break;
                            }
                        }
                    }
                }

                // 按涨跌幅从大到小排序 (降序)
                items.sort_by(|a, b| {
                    b.change_percent
                        .partial_cmp(&a.change_percent)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            ChangeDirection::Loss => {
                // 跌幅榜: 从最小负值开始,反向迭代
                // Losers: Start from min negative, iterate backward
                // 我们需要收集所有负值,然后排序
                let loss_prefix = format!("{}-", prefix);
                let iter = self.db.prefix_iterator(loss_prefix.as_bytes());

                for item in iter {
                    let (key, _) = item?;
                    let key_str = String::from_utf8_lossy(&key);

                    // 检查是否仍在前缀范围内
                    if !key_str.starts_with(&loss_prefix) {
                        break;
                    }

                    // 解析键提取 mint
                    if let Some(mint) = Self::extract_mint_from_rank_key(&key_str) {
                        // 读取主数据
                        if let Some(change_item) = self.read_change_data(mint, period, time_bucket)? {
                            items.push(change_item);
                        }
                    }
                }

                // 按涨跌幅从小到大排序 (升序,负数越小排越前)
                items.sort_by(|a, b| {
                    a.change_percent
                        .partial_cmp(&b.change_percent)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                // 限制返回数量
                items.truncate(limit);
            }
        }

        info!(
            "📈 查询 Top {} {:?} / Queried Top {} {:?}: period={}, time_bucket={}, found={}",
            limit,
            direction,
            limit,
            direction,
            period.as_str(),
            time_bucket,
            items.len()
        );

        Ok(TopChangeResponse {
            period,
            time_bucket,
            direction,
            items,
        })
    }

    /// 从排序索引键中提取 mint 地址 / Extract mint address from ranking key
    /// 键格式: change_rank:{period}:{time_bucket:020}:{change_percent_encoded:+012}:{mint}
    fn extract_mint_from_rank_key(key_str: &str) -> Option<String> {
        let parts: Vec<&str> = key_str.split(':').collect();
        if parts.len() >= 5 {
            // mint 可能包含冒号,所以需要 join
            Some(parts[4..].join(":"))
        } else {
            None
        }
    }

    /// 读取涨跌幅主数据 / Read change main data
    fn read_change_data(
        &self,
        mint: String,
        period: Period,
        time_bucket: u64,
    ) -> Result<Option<TopChangeItem>> {
        let main_key = format!("change:{}:{}:{:020}", period.as_str(), mint, time_bucket);

        if let Some(bytes) = self.db.get(main_key.as_bytes())? {
            if let Ok(data) = serde_json::from_slice::<ChangeData>(&bytes) {
                return Ok(Some(TopChangeItem {
                    mint,
                    open_price: data.open_price,
                    close_price: data.close_price,
                    change_percent: data.change_percent,
                    first_event_time: data.first_event_time,
                    last_event_time: data.last_event_time,
                }));
            }
        }

        Ok(None)
    }
}
