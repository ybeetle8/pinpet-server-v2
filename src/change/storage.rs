// 涨跌幅统计存储模块 / Change Statistics Storage Module
use super::types::{
    ChangeData, Direction, Period, TopChangeItem, TopChangeResponse, TokenChangeResponse,
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

    /// 计算并更新涨跌幅 / Calculate and update change
    ///
    /// # 参数 / Parameters
    /// * `mint` - Token mint 地址 / Token mint address
    /// * `price_before` - 变动前的价格 / Price before change
    /// * `price_after` - 变动后的价格 / Price after change
    /// * `timestamp` - 事件时间戳 / Event timestamp
    pub fn update_change(
        &self,
        mint: &str,
        price_before: u128,
        price_after: u128,
        timestamp: u64,
    ) -> Result<()> {
        // 将价格转换为 USD (价格单位是 lamports per token)
        // Convert price to USD (price unit is lamports per token)
        // 这里需要根据实际的价格单位进行转换
        // 假设 price 已经是以 SOL 为单位的价格，需要乘以 SOL/USD 汇率
        // 为了简化，我们先直接使用 price_after 作为 USD 价格
        // TODO: 需要传入 SOL/USD 汇率进行转换

        // 暂时使用 price_after 作为价格（后续需要根据实际情况调整）
        // Temporarily use price_after as price (needs adjustment based on actual situation)
        let price_usd = Self::price_to_usd(price_after);

        debug!(
            "📊 更新涨跌幅 / Update change: mint={}, price_before={}, price_after={}, price_usd=${:.8}",
            &mint[..8.min(mint.len())],
            price_before,
            price_after,
            price_usd
        );

        // 更新所有时间周期 / Update all time periods
        for period in Period::all() {
            let time_bucket = period.align_timestamp(timestamp);
            self.update_period_change(mint, period, time_bucket, price_usd, timestamp)?;

            // 同时更新排序索引 / Also update ranking index
            self.update_change_rank(mint, period, time_bucket)?;
        }

        Ok(())
    }

    /// 将价格转换为 USD / Convert price to USD
    ///
    /// 注意：这是一个临时实现，实际需要根据价格单位和汇率进行转换
    /// Note: This is a temporary implementation, actual conversion needs price unit and exchange rate
    fn price_to_usd(price: u128) -> f64 {
        // 假设价格单位是 lamports per token
        // 1 SOL = 10^9 lamports
        // 这里需要乘以 SOL/USD 汇率
        // TODO: 传入 SOL/USD 汇率
        let sol_price = price as f64 / 1_000_000_000.0;

        // 临时假设 SOL = $100 (实际应该从参数传入)
        // Temporarily assume SOL = $100 (should be passed as parameter)
        let sol_usd_rate = 100.0;

        sol_price * sol_usd_rate
    }

    /// 更新单个时间周期的涨跌幅 / Update change for a single period
    fn update_period_change(
        &self,
        mint: &str,
        period: Period,
        time_bucket: u64,
        price_usd: f64,
        timestamp: u64,
    ) -> Result<()> {
        // 键格式: change:{period}:{mint}:{time_bucket:020}
        // Key format: change:{period}:{mint}:{time_bucket:020}
        let key = format!("change:{}:{}:{:020}", period.as_str(), mint, time_bucket);

        // 读取现有数据 / Read existing data
        let mut data = match self.db.get(key.as_bytes())? {
            Some(bytes) => serde_json::from_slice::<ChangeData>(&bytes)
                .context("Failed to deserialize change data")?,
            None => ChangeData::new(),
        };

        // 更新价格数据 / Update price data
        data.update_price(price_usd, timestamp);

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
            None => return Ok(()), // 没有数据，跳过 / No data, skip
        };

        // 2. 删除旧的排序索引 (如果存在) / Delete old ranking index (if exists)
        // 键格式: change_rank:{period}:{time_bucket:020}:{change_percent_encoded:+012}:{mint}
        let prefix = format!("change_rank:{}:{:020}:", period.as_str(), time_bucket);
        let mut old_key_to_delete: Option<Vec<u8>> = None;

        let iter = self.db.prefix_iterator(prefix.as_bytes());
        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            // 检查是否是该 mint 的索引 / Check if this is the mint's index
            if key_str.ends_with(&format!(":{}", mint)) {
                old_key_to_delete = Some(key.to_vec());
                break;
            }
        }

        if let Some(old_key) = old_key_to_delete {
            self.db.delete(&old_key)?;
        }

        // 3. 插入新的排序索引 / Insert new ranking index
        // 涨跌幅编码为带符号的 12 位整数
        // Change percentage encoded as signed 12-digit integer
        // 格式: 符号(1位) + 数值(11位)
        // Format: sign(1) + value(11)
        // 数值 = abs(涨跌幅 × 100)，保留2位小数
        // Value = abs(change_percent × 100), 2 decimal places
        let change_encoded = Self::encode_change_percent(data.change_percent);
        let new_key = format!(
            "change_rank:{}:{:020}:{}:{}",
            period.as_str(),
            time_bucket,
            change_encoded,
            mint
        );

        // 值可以为空，我们只需要键的排序 / Value can be empty, we only need key ordering
        self.db.put(new_key.as_bytes(), b"")?;

        debug!(
            "🔄 更新排序索引 / Updated ranking index: period={}, mint={}, change={:.2}%, encoded={}",
            period.as_str(),
            &mint[..8.min(mint.len())],
            data.change_percent,
            change_encoded
        );

        Ok(())
    }

    /// 编码涨跌幅百分比 / Encode change percentage
    ///
    /// 格式: 符号(1位) + 数值(11位)
    /// Format: sign(1) + value(11)
    ///
    /// 示例 / Examples:
    /// - +123.45% → +0000012345
    /// - -67.89%  → -0000006789
    /// - +5000.0% → +0000500000
    /// - 0.00%    → +0000000000
    fn encode_change_percent(change_percent: f64) -> String {
        let sign = if change_percent >= 0.0 { '+' } else { '-' };
        let value = (change_percent.abs() * 100.0).round() as u64;
        format!("{}{:011}", sign, value)
    }

    /// 查询单个币种的涨跌幅 / Query change for a single token
    ///
    /// # 参数 / Parameters
    /// * `mint` - Token mint 地址 / Token mint address
    /// * `period` - 时间周期 / Time period
    /// * `time_bucket` - 时间桶 (可选，默认为当前时间对齐后的时间桶) / Time bucket (optional, defaults to current time aligned)
    pub fn get_token_change(
        &self,
        mint: &str,
        period: Period,
        time_bucket: Option<u64>,
    ) -> Result<TokenChangeResponse> {
        // 如果没有提供时间桶，使用当前时间对齐后的时间桶
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
            None => ChangeData::new(), // 没有数据返回空数据 / No data, return empty
        };

        Ok(TokenChangeResponse {
            mint: mint.to_string(),
            period,
            data,
        })
    }

    /// 查询 Top N 涨跌幅币种 / Query Top N tokens by change
    ///
    /// # 参数 / Parameters
    /// * `period` - 时间周期 / Time period
    /// * `time_bucket` - 时间桶 (可选，默认为当前时间对齐后的时间桶) / Time bucket (optional, defaults to current time aligned)
    /// * `limit` - 返回数量限制 / Limit of results to return
    /// * `direction` - 查询方向 (涨幅或跌幅) / Query direction (gain or loss)
    pub fn get_top_change(
        &self,
        period: Period,
        time_bucket: Option<u64>,
        limit: usize,
        direction: Direction,
    ) -> Result<TopChangeResponse> {
        // 如果没有提供时间桶，使用当前时间对齐后的时间桶
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
            "🔍 查询排序索引 / Querying ranking index: prefix={}, direction={:?}",
            prefix, direction
        );

        let mut items = Vec::new();

        match direction {
            Direction::Gain => {
                // 涨幅榜：从高到低，正向迭代，只取正数
                // Gainers: high to low, forward iteration, only positive
                let iter = self.db.prefix_iterator(prefix.as_bytes());
                for item in iter {
                    let (key, _) = item?;
                    let key_str = String::from_utf8_lossy(&key);

                    if !key_str.starts_with(&prefix) {
                        break;
                    }

                    // 解析键提取符号和 mint
                    // Parse key to extract sign and mint
                    let parts: Vec<&str> = key_str.split(':').collect();
                    if parts.len() < 5 {
                        continue;
                    }

                    let change_encoded = parts[3];
                    let mint = parts[4..].join(":");

                    // 只取正数 / Only take positive numbers
                    if !change_encoded.starts_with('+') {
                        continue;
                    }

                    // 读取主数据 / Read main data
                    if let Some(item) = self.read_change_item(&mint, period, time_bucket)? {
                        items.push(item);
                        if items.len() >= limit {
                            break;
                        }
                    }
                }

                // 按涨跌幅从高到低排序（防止索引不准确）
                // Sort by change_percent descending (in case index is inaccurate)
                items.sort_by(|a, b| {
                    b.change_percent
                        .partial_cmp(&a.change_percent)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            Direction::Loss => {
                // 跌幅榜：从低到高，需要收集所有负数然后排序
                // Losers: low to high, need to collect all negative numbers then sort
                let prefix_with_minus = format!("{}-", prefix);
                let iter = self.db.prefix_iterator(prefix_with_minus.as_bytes());

                for item in iter {
                    let (key, _) = item?;
                    let key_str = String::from_utf8_lossy(&key);

                    if !key_str.starts_with(&prefix) {
                        break;
                    }

                    // 解析键
                    let parts: Vec<&str> = key_str.split(':').collect();
                    if parts.len() < 5 {
                        continue;
                    }

                    let change_encoded = parts[3];
                    let mint = parts[4..].join(":");

                    // 只取负数 / Only take negative numbers
                    if !change_encoded.starts_with('-') {
                        continue;
                    }

                    // 读取主数据
                    if let Some(item) = self.read_change_item(&mint, period, time_bucket)? {
                        items.push(item);
                    }
                }

                // 按涨跌幅从低到高排序
                // Sort by change_percent ascending
                items.sort_by(|a, b| {
                    a.change_percent
                        .partial_cmp(&b.change_percent)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                // 限制返回数量
                items.truncate(limit);
            }
        }

        // 如果没有找到排序索引，尝试直接扫描主数据
        // If no ranking index found, try scanning main data
        if items.is_empty() {
            debug!(
                "⚠️  未找到排序索引，直接扫描主数据 / No ranking index found, scanning main data"
            );
            items = self.scan_main_data_for_top(period, time_bucket, limit, direction)?;
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

    /// 读取涨跌幅数据项 / Read change data item
    fn read_change_item(
        &self,
        mint: &str,
        period: Period,
        time_bucket: u64,
    ) -> Result<Option<TopChangeItem>> {
        let main_key = format!("change:{}:{}:{:020}", period.as_str(), mint, time_bucket);
        if let Some(bytes) = self.db.get(main_key.as_bytes())? {
            if let Ok(data) = serde_json::from_slice::<ChangeData>(&bytes) {
                return Ok(Some(TopChangeItem {
                    mint: mint.to_string(),
                    open_price: data.open_price,
                    close_price: data.close_price,
                    high_price: data.high_price,
                    low_price: data.low_price,
                    change_percent: data.change_percent,
                    first_event_time: data.first_event_time,
                    last_event_time: data.last_event_time,
                }));
            }
        }
        Ok(None)
    }

    /// 直接扫描主数据查找 Top N / Scan main data directly for Top N
    fn scan_main_data_for_top(
        &self,
        period: Period,
        time_bucket: u64,
        limit: usize,
        direction: Direction,
    ) -> Result<Vec<TopChangeItem>> {
        let mut items = Vec::new();
        let change_prefix = format!("change:{}:", period.as_str());
        let iter = self.db.prefix_iterator(change_prefix.as_bytes());

        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);

            if !key_str.starts_with(&change_prefix) {
                break;
            }

            // 解析键提取 mint 和 time_bucket
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() < 4 {
                continue;
            }

            // 检查 time_bucket 是否匹配
            if let Ok(tb) = parts[parts.len() - 1].parse::<u64>() {
                if tb != time_bucket {
                    continue;
                }

                let mint = parts[2..parts.len() - 1].join(":");

                // 解析数据
                if let Ok(data) = serde_json::from_slice::<ChangeData>(&value) {
                    // 根据方向过滤
                    match direction {
                        Direction::Gain if data.change_percent > 0.0 => {
                            items.push(TopChangeItem {
                                mint,
                                open_price: data.open_price,
                                close_price: data.close_price,
                                high_price: data.high_price,
                                low_price: data.low_price,
                                change_percent: data.change_percent,
                                first_event_time: data.first_event_time,
                                last_event_time: data.last_event_time,
                            });
                        }
                        Direction::Loss if data.change_percent < 0.0 => {
                            items.push(TopChangeItem {
                                mint,
                                open_price: data.open_price,
                                close_price: data.close_price,
                                high_price: data.high_price,
                                low_price: data.low_price,
                                change_percent: data.change_percent,
                                first_event_time: data.first_event_time,
                                last_event_time: data.last_event_time,
                            });
                        }
                        _ => {}
                    }
                }
            }
        }

        // 排序
        match direction {
            Direction::Gain => {
                items.sort_by(|a, b| {
                    b.change_percent
                        .partial_cmp(&a.change_percent)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            Direction::Loss => {
                items.sort_by(|a, b| {
                    a.change_percent
                        .partial_cmp(&b.change_percent)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }

        items.truncate(limit);
        Ok(items)
    }
}
