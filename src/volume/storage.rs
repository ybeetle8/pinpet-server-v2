// 交易额统计存储模块 / Volume Statistics Storage Module
use super::types::{Period, TopVolumeItem, TopVolumeResponse, TopRollingVolumeResponse, TokenVolumeResponse, VolumeData, VolumeSlot, RollingVolumeData, RollingVolumeResponse};
use crate::curve_amm::CurveAMM;
use anyhow::{Context, Result};
use rocksdb::{DB, WriteBatch};
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
    /// * `initial_virtual_sol` - 初始虚拟SOL储备量 / Initial virtual SOL reserve
    /// * `initial_virtual_token` - 初始虚拟Token储备量 / Initial virtual Token reserve
    /// * `price_before` - 变动前的价格 / Price before change
    /// * `price_after` - 变动后的价格 / Price after change
    /// * `sol_price_usd` - SOL/USD 汇率 / SOL to USD exchange rate
    /// * `timestamp` - 事件时间戳 / Event timestamp
    pub fn update_volume(
        &self,
        mint: &str,
        initial_virtual_sol: u64,
        initial_virtual_token: u64,
        price_before: u128,
        price_after: u128,
        sol_price_usd: f64,
        timestamp: u64,
    ) -> Result<()> {
        // 1. 计算 SOL 储备变化,使用动态池子参数 / Calculate SOL reserve change using dynamic pool parameters
        let (sol_reserve_before, _) = CurveAMM::price_to_reserves(initial_virtual_sol, initial_virtual_token, price_before)
            .context("Failed to calculate reserves before")?;
        let (sol_reserve_after, _) = CurveAMM::price_to_reserves(initial_virtual_sol, initial_virtual_token, price_after)
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
            if period == Period::TwentyFourHours {
                continue; // 24h 不再写固定桶，由滚动窗口替代 / 24h no longer uses fixed buckets, replaced by rolling window
            }
            let time_bucket = period.align_timestamp(timestamp);
            self.update_period_volume(mint, period, time_bucket, volume_usd, timestamp)?;

            // 同时更新排序索引 / Also update ranking index
            self.update_volume_rank(mint, period, time_bucket)?;
        }

        // 写入滚动24h数据（替代原来的 24h 固定桶）/ Write rolling 24h data (replaces 24h fixed bucket)
        self.update_rolling_volume(mint, volume_usd, timestamp)?;

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

        debug!(
            "🔍 查询排序索引 / Querying ranking index: prefix={}",
            prefix
        );

        // 先收集所有匹配的项，然后按交易额排序 / First collect all matching items, then sort by volume
        let mut items = Vec::new();
        let iter = self.db.prefix_iterator(prefix.as_bytes());

        for item in iter {
            let (key, _) = item?;
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
                }
            }
        }

        // 如果没有找到排序索引，尝试直接扫描主数据 / If no ranking index found, try scanning main data
        if items.is_empty() {
            debug!(
                "⚠️  未找到排序索引，直接扫描主数据 / No ranking index found, scanning main data"
            );

            // 直接扫描 vol: 前缀的所有数据 / Scan all data with vol: prefix
            let vol_prefix = format!("vol:{}:", period.as_str());
            let vol_iter = self.db.prefix_iterator(vol_prefix.as_bytes());

            for item in vol_iter {
                let (key, value) = item?;
                let key_str = String::from_utf8_lossy(&key);

                // 检查前缀 / Check prefix
                if !key_str.starts_with(&vol_prefix) {
                    break;
                }

                // 解析键提取 mint 和 time_bucket
                // 键格式: vol:{period}:{mint}:{time_bucket:020}
                let parts: Vec<&str> = key_str.split(':').collect();
                if parts.len() < 4 {
                    continue;
                }

                // 检查 time_bucket 是否匹配
                if let Ok(tb) = parts[parts.len() - 1].parse::<u64>() {
                    if tb != time_bucket {
                        continue;
                    }

                    // 提取 mint (可能包含冒号，所以需要 join)
                    let mint = parts[2..parts.len() - 1].join(":");

                    // 解析数据
                    if let Ok(data) = serde_json::from_slice::<VolumeData>(&value) {
                        if data.volume > 0.0 {
                            // 只包含有交易额的 token
                            items.push(TopVolumeItem {
                                mint,
                                volume: data.volume,
                                event_count: data.event_count,
                                last_update: data.last_update,
                            });
                        }
                    }
                }
            }
        }

        // 按交易额从大到小排序 / Sort by volume in descending order
        items.sort_by(|a, b| b.volume.partial_cmp(&a.volume).unwrap_or(std::cmp::Ordering::Equal));

        // 限制返回数量 / Limit results
        items.truncate(limit);

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

    // ============================================================================
    // 滚动24小时窗口方法 / Rolling 24h Window Methods
    // ============================================================================

    /// 更新滚动24h交易额 / Update rolling 24h volume
    pub fn update_rolling_volume(&self, mint: &str, volume_usd: f64, timestamp: u64) -> Result<()> {
        let key = format!("vol_rolling:{}", mint);
        let hour_bucket = (timestamp / 3600) * 3600;
        let cutoff = timestamp.saturating_sub(86400);

        // 1. 读取现有数据 / Read existing data
        let mut data = match self.db.get(key.as_bytes())? {
            Some(bytes) => serde_json::from_slice::<RollingVolumeData>(&bytes)
                .context("Failed to deserialize rolling volume data")?,
            None => RollingVolumeData::new(),
        };

        // 2. 清理过期槽位 / Clean expired slots
        data.slots.retain(|&k, _| k + 3600 > cutoff);

        // 3. 更新当前小时槽位 / Update current hour slot
        let slot = data.slots.entry(hour_bucket).or_insert(VolumeSlot { volume: 0.0, event_count: 0 });
        slot.volume += volume_usd;
        slot.event_count += 1;

        // 4. 从 slots 重算聚合缓存（安全，最多25次加法）/ Recalculate aggregates from slots (safe, max 25 additions)
        data.total_volume = data.slots.values().map(|s| s.volume).sum();
        data.total_event_count = data.slots.values().map(|s| s.event_count).sum();
        data.last_update = timestamp;

        // 5. 原子写入：主数据 + 排序索引 / Atomic write: main data + ranking index
        let mut batch = WriteBatch::default();
        let value = serde_json::to_vec(&data).context("Failed to serialize rolling volume data")?;
        batch.put(key.as_bytes(), &value);

        // 6. 更新排序索引 / Update ranking index
        self.batch_update_rolling_volume_rank(&mut batch, mint, &data)?;

        self.db.write(batch)?;

        debug!(
            "✅ 更新滚动24h交易额 / Updated rolling 24h volume: mint={}, volume=${:.2}, slots={}",
            &mint[..8.min(mint.len())],
            data.total_volume,
            data.slots.len()
        );

        Ok(())
    }

    /// 批量更新滚动交易额排序索引 / Batch update rolling volume ranking index
    fn batch_update_rolling_volume_rank(&self, batch: &mut WriteBatch, mint: &str, data: &RollingVolumeData) -> Result<()> {
        // 1. 通过反向索引精确删除旧 rank key / Delete old rank key via reverse index
        let rev_key = format!("vol_rank_rolling_rev:{}", mint);
        if let Some(old_rank_key_bytes) = self.db.get(rev_key.as_bytes())? {
            batch.delete(&old_rank_key_bytes);
        }

        // 2. 插入新 rank key / Insert new rank key
        let volume_cents = (data.total_volume * 100.0).round() as u64;
        let new_rank_key = format!("vol_rank_rolling:{:020}:{}", volume_cents, mint);
        batch.put(new_rank_key.as_bytes(), b"");

        // 3. 更新反向索引 / Update reverse index
        batch.put(rev_key.as_bytes(), new_rank_key.as_bytes());

        Ok(())
    }

    /// 查询单个 mint 的滚动24h交易额 / Query rolling 24h volume for a single mint
    pub fn get_rolling_volume(&self, mint: &str) -> Result<RollingVolumeResponse> {
        let key = format!("vol_rolling:{}", mint);

        let data = match self.db.get(key.as_bytes())? {
            Some(bytes) => serde_json::from_slice::<RollingVolumeData>(&bytes)
                .context("Failed to deserialize rolling volume data")?,
            None => return Ok(RollingVolumeResponse::empty(mint)),
        };

        // 查询时清理过期槽位（应对冷门 mint 长时间无写入）/ Filter expired slots on query (for cold mints)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let cutoff = now.saturating_sub(86400);

        let mut valid_volume = 0.0;
        let mut valid_count = 0u64;
        for (&k, slot) in &data.slots {
            if k + 3600 > cutoff {
                valid_volume += slot.volume;
                valid_count += slot.event_count;
            }
        }

        Ok(RollingVolumeResponse {
            mint: mint.to_string(),
            volume: valid_volume,
            event_count: valid_count,
            last_update: data.last_update,
        })
    }

    /// 查询 Top N 滚动24h交易额 / Query top N rolling 24h volume
    pub fn get_top_rolling_volume(&self, limit: usize) -> Result<TopRollingVolumeResponse> {
        // 反向遍历 vol_rank_rolling: 前缀（键是字典序，volume_cents 大的排后面）
        // Reverse iterate vol_rank_rolling: prefix (keys are lexicographic, larger volume_cents at end)
        let prefix = "vol_rank_rolling:";

        // 收集所有 rank keys / Collect all rank keys
        let mut rank_keys: Vec<(String, String)> = Vec::new(); // (rank_key, mint)
        let iter = self.db.prefix_iterator(prefix.as_bytes());

        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);

            if !key_str.starts_with(prefix) {
                break;
            }

            // 解析: vol_rank_rolling:{volume_cents:020}:{mint}
            // Parse: vol_rank_rolling:{volume_cents:020}:{mint}
            let rest = &key_str[prefix.len()..];
            if let Some(colon_pos) = rest.find(':') {
                let mint = rest[colon_pos + 1..].to_string();
                rank_keys.push((key_str.to_string(), mint));
            }
        }

        // 从后往前取（volume 大的在后面）/ Take from end (larger volumes are at the end)
        let mut items = Vec::new();
        for (_rank_key, mint) in rank_keys.iter().rev() {
            // 二次验证：过滤掉已过期的 / Secondary validation: filter out expired
            let rolling_resp = self.get_rolling_volume(mint)?;
            if rolling_resp.volume > 0.0 {
                items.push(TopVolumeItem {
                    mint: mint.clone(),
                    volume: rolling_resp.volume,
                    event_count: rolling_resp.event_count,
                    last_update: rolling_resp.last_update,
                });
            }

            if items.len() >= limit {
                break;
            }
        }

        info!(
            "📈 查询 Top {} 滚动24h交易额 / Queried Top {} rolling 24h volume: found={}",
            limit, limit, items.len()
        );

        Ok(TopRollingVolumeResponse { items })
    }

    // ============================================================================
    // 启动时重建/刷新 / Startup Rebuild/Refresh
    // ============================================================================

    /// 检查是否需要从1h桶重建滚动数据 / Check if rebuild from 1h buckets is needed
    pub fn rebuild_rolling_data_if_needed(&self) -> Result<()> {
        let prefix = b"vol_rolling:";
        let has_rolling = self.db.prefix_iterator(prefix)
            .next()
            .and_then(|r| r.ok())
            .map(|(k, _)| k.starts_with(prefix))
            .unwrap_or(false);

        if !has_rolling {
            info!("首次升级，从1h桶重建滚动24h交易额数据... / First upgrade, rebuilding rolling 24h volume data from 1h buckets...");
            self.rebuild_rolling_volume_data()?;
        }
        Ok(())
    }

    /// 从1h桶重建滚动24h交易额数据 / Rebuild rolling 24h volume data from 1h buckets
    fn rebuild_rolling_volume_data(&self) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let cutoff = now.saturating_sub(86400);

        // 在内存中按 mint 归集 / Aggregate by mint in memory
        let mut vol_map: std::collections::HashMap<String, RollingVolumeData> = std::collections::HashMap::new();

        let prefix = b"vol:1h:";
        let iter = self.db.prefix_iterator(prefix);

        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if !key_str.starts_with("vol:1h:") { break; }

            // 解析: vol:1h:{mint}:{time_bucket:020}
            // Parse: vol:1h:{mint}:{time_bucket:020}
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() < 4 { continue; }

            let time_bucket: u64 = match parts.last().unwrap().parse() {
                Ok(v) => v,
                Err(_) => continue,
            };

            // 只取最近24小时的桶 / Only take buckets from last 24h
            if time_bucket + 3600 <= cutoff { continue; }

            let mint = parts[2..parts.len()-1].join(":");
            let vol_data: VolumeData = match serde_json::from_slice(&value) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let rolling = vol_map.entry(mint).or_insert_with(RollingVolumeData::new);
            rolling.slots.insert(time_bucket, VolumeSlot {
                volume: vol_data.volume,
                event_count: vol_data.event_count,
            });
            rolling.last_update = rolling.last_update.max(vol_data.last_update);
        }

        // 批量写入 / Batch write
        let mut batch = WriteBatch::default();
        let mut count = 0u64;

        for (mint, mut data) in vol_map {
            // 重算聚合 / Recalculate aggregates
            data.total_volume = data.slots.values().map(|s| s.volume).sum();
            data.total_event_count = data.slots.values().map(|s| s.event_count).sum();

            let key = format!("vol_rolling:{}", mint);
            let value = serde_json::to_vec(&data)?;
            batch.put(key.as_bytes(), &value);

            // 排序索引 + 反向索引 / Ranking index + reverse index
            let volume_cents = (data.total_volume * 100.0).round() as u64;
            let rank_key = format!("vol_rank_rolling:{:020}:{}", volume_cents, mint);
            batch.put(rank_key.as_bytes(), b"");
            batch.put(format!("vol_rank_rolling_rev:{}", mint).as_bytes(), rank_key.as_bytes());

            count += 1;
        }

        self.db.write(batch)?;
        info!("滚动24h交易额数据重建完成，共 {} 个 mint / Rolling 24h volume data rebuild complete, {} mints", count, count);
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

        let prefix = b"vol_rolling:";
        let iter = self.db.prefix_iterator(prefix);

        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if !key_str.starts_with("vol_rolling:") { break; }

            let mut data: RollingVolumeData = match serde_json::from_slice(&value) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let before = data.slots.len();
            data.slots.retain(|&k, _| k + 3600 > cutoff);

            if data.slots.len() != before {
                data.total_volume = data.slots.values().map(|s| s.volume).sum();
                data.total_event_count = data.slots.values().map(|s| s.event_count).sum();

                let value = serde_json::to_vec(&data)?;
                batch.put(&*key, &value);

                // 同步更新排序索引 / Sync update ranking index
                let mint = key_str.strip_prefix("vol_rolling:").unwrap();

                // 删除旧 rank key / Delete old rank key
                let rev_key = format!("vol_rank_rolling_rev:{}", mint);
                if let Some(old_rank_key_bytes) = self.db.get(rev_key.as_bytes())? {
                    batch.delete(&old_rank_key_bytes);
                }

                // 插入新 rank key / Insert new rank key
                let volume_cents = (data.total_volume * 100.0).round() as u64;
                let new_rank_key = format!("vol_rank_rolling:{:020}:{}", volume_cents, mint);
                batch.put(new_rank_key.as_bytes(), b"");
                batch.put(rev_key.as_bytes(), new_rank_key.as_bytes());

                refreshed += 1;
            }
        }

        self.db.write(batch)?;
        if refreshed > 0 {
            info!("启动时刷新了 {} 个 mint 的滚动24h交易额数据 / Refreshed rolling 24h volume data for {} mints on startup", refreshed, refreshed);
        }
        Ok(())
    }
}
