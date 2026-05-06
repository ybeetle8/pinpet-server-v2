// 涨跌幅统计存储模块 / Change Statistics Storage Module
use super::types::{
    ChangeData, ChangeDirection, ChangeSlot, Period, RollingChangeData, RollingChangeResponse,
    TokenChangeResponse, TopChangeItem, TopChangeResponse, TopRollingChangeResponse,
};
use anyhow::{Context, Result};
use rocksdb::{DB, WriteBatch};
use std::collections::BTreeMap;
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
            if period == Period::TwentyFourHours {
                continue; // 24h 不再写固定桶，由滚动窗口替代 / 24h no longer uses fixed buckets, replaced by rolling window
            }
            let time_bucket = period.align_timestamp(timestamp);
            self.update_period_change(mint, period, time_bucket, price, timestamp)?;

            // 同时更新排序索引 / Also update ranking index
            self.update_change_rank(mint, period, time_bucket)?;
        }

        // 写入滚动24h数据（替代原来的 24h 固定桶）/ Write rolling 24h data (replaces 24h fixed bucket)
        self.update_rolling_change(mint, price, timestamp)?;

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

    // ============================================================================
    // 滚动24小时窗口方法 / Rolling 24h Window Methods
    // ============================================================================

    /// 更新滚动24h涨跌幅 / Update rolling 24h change
    pub fn update_rolling_change(&self, mint: &str, price: f64, timestamp: u64) -> Result<()> {
        let key = format!("change_rolling:{}", mint);
        let hour_bucket = (timestamp / 3600) * 3600;
        let cutoff = timestamp.saturating_sub(86400);

        // 1. 读取现有数据 / Read existing data
        let mut data = match self.db.get(key.as_bytes())? {
            Some(bytes) => serde_json::from_slice::<RollingChangeData>(&bytes)
                .context("Failed to deserialize rolling change data")?,
            None => RollingChangeData::new(),
        };

        // 2. 清理过期槽位 / Clean expired slots
        data.slots.retain(|&k, _| k + 3600 > cutoff);

        // 3. 更新当前小时槽位 / Update current hour slot
        let slot = data.slots.entry(hour_bucket).or_insert_with(|| ChangeSlot {
            open_price: price,
            close_price: price,
            first_event_time: timestamp,
            last_event_time: timestamp,
        });
        slot.close_price = price;
        slot.last_event_time = timestamp;

        data.last_update = timestamp;

        // 4. 计算滚动涨跌幅 / Calculate rolling change percent
        let (open_price, close_price) = Self::calc_rolling_change(&data);
        let change_percent = if open_price > 0.0 {
            ((close_price - open_price) / open_price) * 100.0
        } else {
            0.0
        };

        // 5. 原子写入 / Atomic write
        let mut batch = WriteBatch::default();
        let value = serde_json::to_vec(&data).context("Failed to serialize rolling change data")?;
        batch.put(key.as_bytes(), &value);

        self.batch_update_rolling_change_rank(&mut batch, mint, change_percent)?;

        self.db.write(batch)?;

        debug!(
            "✅ 更新滚动24h涨跌幅 / Updated rolling 24h change: mint={}, change={:.2}%, slots={}",
            &mint[..8.min(mint.len())],
            change_percent,
            data.slots.len()
        );

        Ok(())
    }

    /// 从滚动数据中计算开盘价和收盘价 / Calculate open and close price from rolling data
    pub fn calc_rolling_change(data: &RollingChangeData) -> (f64, f64) {
        if data.slots.is_empty() {
            return (0.0, 0.0);
        }
        let earliest = data.slots.keys().min().unwrap();
        let open_price = data.slots[earliest].open_price;

        let latest = data.slots.keys().max().unwrap();
        let close_price = data.slots[latest].close_price;

        (open_price, close_price)
    }

    /// 批量更新滚动涨跌幅排序索引 / Batch update rolling change ranking index
    fn batch_update_rolling_change_rank(&self, batch: &mut WriteBatch, mint: &str, change_percent: f64) -> Result<()> {
        // 1. 通过反向索引精确删除旧 rank key / Delete old rank key via reverse index
        let rev_key = format!("change_rank_rolling_rev:{}", mint);
        if let Some(old_rank_key_bytes) = self.db.get(rev_key.as_bytes())? {
            batch.delete(&old_rank_key_bytes);
        }

        // 2. 插入新 rank key / Insert new rank key
        let change_encoded = Self::encode_change_percent(change_percent);
        let new_rank_key = format!("change_rank_rolling:{}:{}", change_encoded, mint);
        batch.put(new_rank_key.as_bytes(), b"");

        // 3. 更新反向索引 / Update reverse index
        batch.put(rev_key.as_bytes(), new_rank_key.as_bytes());

        Ok(())
    }

    /// 查询单个 mint 的滚动24h涨跌幅 / Query rolling 24h change for a single mint
    pub fn get_rolling_change(&self, mint: &str) -> Result<RollingChangeResponse> {
        let key = format!("change_rolling:{}", mint);

        let data = match self.db.get(key.as_bytes())? {
            Some(bytes) => serde_json::from_slice::<RollingChangeData>(&bytes)
                .context("Failed to deserialize rolling change data")?,
            None => return Ok(RollingChangeResponse::empty(mint)),
        };

        // 查询时过滤过期槽位 / Filter expired slots on query
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let cutoff = now.saturating_sub(86400);

        let valid_slots: BTreeMap<u64, &ChangeSlot> = data.slots.iter()
            .filter(|(&k, _)| k + 3600 > cutoff)
            .map(|(&k, v)| (k, v))
            .collect();

        if valid_slots.is_empty() {
            return Ok(RollingChangeResponse::empty(mint));
        }

        let open_price = valid_slots.values().next().unwrap().open_price;
        let close_price = valid_slots.values().last().unwrap().close_price;
        let change_percent = if open_price > 0.0 {
            ((close_price - open_price) / open_price) * 100.0
        } else {
            0.0
        };

        Ok(RollingChangeResponse {
            mint: mint.to_string(),
            open_price,
            close_price,
            change_percent,
            last_update: data.last_update,
        })
    }

    /// 查询 Top N 滚动24h涨幅 / Query top N rolling 24h change (gainers)
    pub fn get_top_rolling_change(&self, limit: usize) -> Result<TopRollingChangeResponse> {
        // 遍历 change_rank_rolling: 前缀中以 '+' 开头的键（涨幅）
        // Iterate change_rank_rolling: prefix with '+' keys (gainers)
        let prefix = "change_rank_rolling:+";

        let mut rank_keys: Vec<(String, String)> = Vec::new(); // (rank_key, mint)
        let iter = self.db.prefix_iterator(prefix.as_bytes());

        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);

            if !key_str.starts_with(prefix) {
                break;
            }

            // 解析: change_rank_rolling:{change_encoded}:{mint}
            // Parse: change_rank_rolling:{change_encoded}:{mint}
            // change_encoded 是 12 字符（+0000012345）/ change_encoded is 12 chars
            let rest = &key_str["change_rank_rolling:".len()..];
            if rest.len() > 12 {
                let mint = rest[13..].to_string(); // skip encoded (12 chars) + ':' (1 char)
                rank_keys.push((key_str.to_string(), mint));
            }
        }

        // 从后往前取（涨幅大的在后面）/ Take from end (larger gains at the end)
        let mut items = Vec::new();
        for (_rank_key, mint) in rank_keys.iter().rev() {
            // 二次验证 / Secondary validation
            let rolling_resp = self.get_rolling_change(mint)?;
            if rolling_resp.change_percent > 0.0 {
                items.push(TopChangeItem {
                    mint: mint.clone(),
                    open_price: rolling_resp.open_price,
                    close_price: rolling_resp.close_price,
                    change_percent: rolling_resp.change_percent,
                    first_event_time: 0,
                    last_event_time: rolling_resp.last_update,
                });
            }

            if items.len() >= limit {
                break;
            }
        }

        info!(
            "📈 查询 Top {} 滚动24h涨幅 / Queried Top {} rolling 24h change: found={}",
            limit, limit, items.len()
        );

        Ok(TopRollingChangeResponse { items })
    }

    // ============================================================================
    // 启动时重建/刷新 / Startup Rebuild/Refresh
    // ============================================================================

    /// 检查是否需要从1h桶重建滚动数据 / Check if rebuild from 1h buckets is needed
    pub fn rebuild_rolling_data_if_needed(&self) -> Result<()> {
        let prefix = b"change_rolling:";
        let has_rolling = self.db.prefix_iterator(prefix)
            .next()
            .and_then(|r| r.ok())
            .map(|(k, _)| k.starts_with(prefix))
            .unwrap_or(false);

        if !has_rolling {
            info!("首次升级，从1h桶重建滚动24h涨跌幅数据... / First upgrade, rebuilding rolling 24h change data from 1h buckets...");
            self.rebuild_rolling_change_data()?;
        }
        Ok(())
    }

    /// 从1h桶重建滚动24h涨跌幅数据 / Rebuild rolling 24h change data from 1h buckets
    fn rebuild_rolling_change_data(&self) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let cutoff = now.saturating_sub(86400);

        // 在内存中按 mint 归集 / Aggregate by mint in memory
        let mut change_map: std::collections::HashMap<String, RollingChangeData> = std::collections::HashMap::new();

        let prefix = b"change:1h:";
        let iter = self.db.prefix_iterator(prefix);

        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if !key_str.starts_with("change:1h:") { break; }

            // 解析: change:1h:{mint}:{time_bucket:020}
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() < 4 { continue; }

            let time_bucket: u64 = match parts.last().unwrap().parse() {
                Ok(v) => v,
                Err(_) => continue,
            };

            // 只取最近24小时的桶 / Only take buckets from last 24h
            if time_bucket + 3600 <= cutoff { continue; }

            let mint = parts[2..parts.len()-1].join(":");
            let change_data: ChangeData = match serde_json::from_slice(&value) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let rolling = change_map.entry(mint).or_insert_with(RollingChangeData::new);
            rolling.slots.insert(time_bucket, ChangeSlot {
                open_price: change_data.open_price,
                close_price: change_data.close_price,
                first_event_time: change_data.first_event_time,
                last_event_time: change_data.last_event_time,
            });
            rolling.last_update = rolling.last_update.max(change_data.last_event_time);
        }

        // 批量写入 / Batch write
        let mut batch = WriteBatch::default();
        let mut count = 0u64;

        for (mint, data) in &change_map {
            let key = format!("change_rolling:{}", mint);
            let value = serde_json::to_vec(data)?;
            batch.put(key.as_bytes(), &value);

            // 计算涨跌幅并写排序索引 / Calculate change percent and write ranking index
            let (open, close) = Self::calc_rolling_change(data);
            let change_pct = if open > 0.0 { ((close - open) / open) * 100.0 } else { 0.0 };
            let encoded = Self::encode_change_percent(change_pct);
            let rank_key = format!("change_rank_rolling:{}:{}", encoded, mint);
            batch.put(rank_key.as_bytes(), b"");
            batch.put(format!("change_rank_rolling_rev:{}", mint).as_bytes(), rank_key.as_bytes());

            count += 1;
        }

        self.db.write(batch)?;
        info!("滚动24h涨跌幅数据重建完成，共 {} 个 mint / Rolling 24h change data rebuild complete, {} mints", count, count);
        Ok(())
    }

    /// 启动时刷新过期的滚动数据 / Refresh expired rolling data on startup
    pub fn refresh_rolling_data_on_startup(&self) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let cutoff = now.saturating_sub(86400);

        let mut batch = WriteBatch::default();
        let mut refreshed = 0u64;

        let prefix = b"change_rolling:";
        let iter = self.db.prefix_iterator(prefix);

        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if !key_str.starts_with("change_rolling:") { break; }

            let mut data: RollingChangeData = match serde_json::from_slice(&value) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let before = data.slots.len();
            data.slots.retain(|&k, _| k + 3600 > cutoff);

            if data.slots.len() != before {
                let value = serde_json::to_vec(&data)?;
                batch.put(&*key, &value);

                // 同步更新排序索引 / Sync update ranking index
                let mint = key_str.strip_prefix("change_rolling:").unwrap();

                // 删除旧 rank key / Delete old rank key
                let rev_key = format!("change_rank_rolling_rev:{}", mint);
                if let Some(old_rank_key_bytes) = self.db.get(rev_key.as_bytes())? {
                    batch.delete(&old_rank_key_bytes);
                }

                // 计算新涨跌幅并插入新 rank key / Calculate new change and insert new rank key
                let (open, close) = Self::calc_rolling_change(&data);
                let change_pct = if open > 0.0 { ((close - open) / open) * 100.0 } else { 0.0 };
                let encoded = Self::encode_change_percent(change_pct);
                let new_rank_key = format!("change_rank_rolling:{}:{}", encoded, mint);
                batch.put(new_rank_key.as_bytes(), b"");
                batch.put(rev_key.as_bytes(), new_rank_key.as_bytes());

                refreshed += 1;
            }
        }

        self.db.write(batch)?;
        if refreshed > 0 {
            info!("启动时刷新了 {} 个 mint 的滚动24h涨跌幅数据 / Refreshed rolling 24h change data for {} mints on startup", refreshed, refreshed);
        }
        Ok(())
    }
}
