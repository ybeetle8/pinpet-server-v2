// K线数据存储模块 / K-line data storage module
use anyhow::Result;
use chrono::{DateTime, Utc};
use rocksdb::{Direction, IteratorMode, DB};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::kline::types::{KlineData, KlineQuery, KlineQueryResponse};

/// K线时间间隔常量 / K-line interval constants
pub const KLINE_INTERVAL_1S: &str = "s1";   // 1秒 / 1 second
pub const KLINE_INTERVAL_30S: &str = "s30"; // 30秒 / 30 seconds
pub const KLINE_INTERVAL_5M: &str = "m5";   // 5分钟 / 5 minutes

/// 价格精度常量(26位小数) / Precision constant for u128 to f64 conversion (26 decimal places)
pub const PRICE_PRECISION: u128 = 10_u128.pow(26);

/// K线存储服务 / K-line storage service
pub struct KlineStorage {
    db: Arc<DB>,
}

impl KlineStorage {
    /// 创建新的K线存储服务 / Create new K-line storage service
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    /// 生成K线键 / Generate K-line key
    /// 格式: interval:mint_account:timestamp(20位零填充)
    /// Format: interval:mint_account:timestamp(20-digit zero-padded)
    fn generate_kline_key(&self, interval: &str, mint_account: &str, timestamp: u64) -> String {
        format!("{}:{}:{:020}", interval, mint_account, timestamp)
    }

    /// 将u128价格转换为f64 / Convert u128 price to f64 with precision handling
    fn convert_price_to_f64(&self, price_u128: u128) -> f64 {
        let price_f64 = price_u128 as f64 / PRICE_PRECISION as f64;
        // 四舍五入到12位小数以避免浮点噪声 / Round to 12 decimal places to avoid floating point noise
        (price_f64 * 1e12).round() / 1e12
    }

    /// 计算时间桶 / Calculate time bucket for different intervals
    /// 返回对齐后的时间戳 / Returns the aligned timestamp for the time bucket
    fn calculate_time_bucket(&self, timestamp: u64, interval: &str) -> u64 {
        match interval {
            KLINE_INTERVAL_1S => timestamp,        // 1秒间隔-不需要对齐 / 1-second intervals - no alignment needed
            KLINE_INTERVAL_30S => (timestamp / 30) * 30,  // 30秒边界对齐 / align to 30-second boundary
            KLINE_INTERVAL_5M => (timestamp / 300) * 300, // 5分钟边界对齐 / align to 5-minute boundary
            _ => timestamp,  // 默认1秒 / default to 1-second
        }
    }

    /// 获取上一个K线的收盘价 / Get previous K-line close price
    /// 用于维持价格连续性,避免K线之间的价格gap / Used to maintain price continuity and avoid gaps between K-lines
    fn get_previous_kline_close_price(
        &self,
        interval: &str,
        mint_account: &str,
        current_time_bucket: u64,
    ) -> Option<f64> {
        // 构建前缀键 / Build prefix key for the specific mint and interval
        let prefix = format!("{}:{}:", interval, mint_account);

        // 从头开始迭代找到当前时间桶之前的最新K线 / Iterate from the beginning to find the latest kline before current_time_bucket
        let iter = self
            .db
            .iterator(IteratorMode::From(prefix.as_bytes(), Direction::Forward));
        let mut latest_close_price = None;

        for item in iter {
            if let Ok((key, value)) = item {
                let key_str = String::from_utf8_lossy(&key);

                // 检查是否仍匹配前缀 / Check if still matches prefix
                if !key_str.starts_with(&prefix) {
                    break;
                }

                // 从键中提取时间戳 / Extract timestamp from key format: "interval:mint_account:timestamp"
                if let Some(timestamp_str) = key_str.split(':').nth(2) {
                    if let Ok(timestamp) = timestamp_str.parse::<u64>() {
                        // 只考虑当前时间桶之前的K线 / Only consider klines before the current time bucket
                        if timestamp < current_time_bucket {
                            // 解析K线数据获取收盘价 / Parse kline data to get close price
                            if let Ok(kline_data) = serde_json::from_slice::<KlineData>(&value) {
                                latest_close_price = Some(kline_data.close);
                            }
                        } else {
                            // 已经到达或超过当前时间桶,停止迭代 / Reached or exceeded current time bucket, stop iteration
                            break;
                        }
                    }
                }
            }
        }

        latest_close_price
    }

    /// 处理K线数据 / Process K-line data
    /// 当有新的价格事件时调用,更新或创建对应时间间隔的K线数据
    /// Called when there's a new price event, updates or creates K-line data for corresponding intervals
    pub async fn process_kline_data(
        &self,
        mint_account: &str,
        latest_price: u128,
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        let price = self.convert_price_to_f64(latest_price);
        let unix_timestamp = timestamp.timestamp() as u64;

        let intervals = [KLINE_INTERVAL_1S, KLINE_INTERVAL_30S, KLINE_INTERVAL_5M];

        for interval in intervals {
            let time_bucket = self.calculate_time_bucket(unix_timestamp, interval);
            let kline_key = self.generate_kline_key(interval, mint_account, time_bucket);

            // 尝试获取现有的K线数据 / Try to get existing kline data
            let kline_data = match self.db.get(kline_key.as_bytes())? {
                Some(data) => {
                    match serde_json::from_slice::<KlineData>(&data) {
                        Ok(mut existing_kline) => {
                            // 更新现有K线数据(同一时间桶) / Update existing kline data (same time bucket)
                            existing_kline.high = existing_kline.high.max(price);
                            existing_kline.low = existing_kline.low.min(price);
                            existing_kline.close = price;
                            existing_kline.update_count += 1;
                            existing_kline.is_final = false; // 标记为非最终状态,因为正在更新 / Mark as not final since it's being updated
                            existing_kline
                        }
                        Err(e) => {
                            warn!(
                                "Failed to parse existing kline data: {}, creating new one",
                                e
                            );
                            // 解析失败时创建新K线数据 / Create new kline data if parsing fails
                            // 获取上一个K线的收盘价以避免gap / Get previous kline close price to avoid gaps
                            let open_price = self
                                .get_previous_kline_close_price(interval, mint_account, time_bucket)
                                .unwrap_or(price); // 如果没有找到上一个K线,使用当前价格 / Use current price if no previous kline found

                            KlineData {
                                time: time_bucket,
                                open: open_price,
                                high: price,
                                low: price,
                                close: price,
                                volume: 0.0, // Volume按要求为0 / Volume is 0 as requested
                                is_final: false,
                                update_count: 1,
                            }
                        }
                    }
                }
                None => {
                    // 为不同时间桶创建新K线数据 / Create new kline data for different time bucket
                    // 获取上一个K线的收盘价以保持价格连续性,避免gap / Get previous kline close price to maintain price continuity and avoid gaps
                    let open_price = self
                        .get_previous_kline_close_price(interval, mint_account, time_bucket)
                        .unwrap_or(price); // 如果没有找到上一个K线(首个K线),使用当前价格 / Use current price if no previous kline found (first kline)

                    KlineData {
                        time: time_bucket,
                        open: open_price,
                        high: price,
                        low: price,
                        close: price,
                        volume: 0.0, // Volume按要求为0 / Volume is 0 as requested
                        is_final: false,
                        update_count: 1,
                    }
                }
            };

            // 存储更新后的K线数据 / Store updated kline data
            let value = serde_json::to_vec(&kline_data)?;
            self.db.put(kline_key.as_bytes(), &value)?;

            debug!(
                "💹 Kline data updated for interval {}, mint: {}, time: {}, open: {}, close: {}",
                interval, mint_account, time_bucket, kline_data.open, price
            );
        }

        Ok(())
    }

    /// 查询K线数据 / Query K-line data
    pub async fn query_kline_data(&self, query: KlineQuery) -> Result<KlineQueryResponse> {
        let mint_account = &query.mint_account;
        let interval = &query.interval;
        let page = query.page.unwrap_or(1);
        let limit = query.limit.unwrap_or(50);
        let order_by = query.order_by.unwrap_or_else(|| "time_desc".to_string());

        // 验证时间间隔 / Validate interval
        if !matches!(interval.as_str(), "s1" | "s30" | "m5") {
            return Err(anyhow::anyhow!(
                "Invalid interval: {}, must be one of: s1, s30, m5",
                interval
            ));
        }

        debug!(
            "🔍 Querying kline data, mint: {}, interval: {}, page: {}, limit: {}, order: {}",
            mint_account, interval, page, limit, order_by
        );

        // 构建特定mint和interval的前缀键 / Build prefix key for the specific mint and interval
        let prefix = format!("{}:{}:", interval, mint_account);

        // 收集所有匹配的K线数据 / Collect all matching kline data
        let mut all_klines = Vec::new();

        let iter = self
            .db
            .iterator(IteratorMode::From(prefix.as_bytes(), Direction::Forward));

        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);

            // 检查是否仍匹配前缀 / Check if still matches prefix
            if !key_str.starts_with(&prefix) {
                break;
            }

            // 解析K线数据 / Parse kline data
            match serde_json::from_slice::<KlineData>(&value) {
                Ok(kline_data) => all_klines.push(kline_data),
                Err(e) => {
                    warn!("❌ Failed to parse kline data: {}, key: {}", e, key_str);
                    continue;
                }
            }
        }

        // 按时间排序 / Sort by time
        match order_by.as_str() {
            "time_asc" => {
                all_klines.sort_by(|a, b| a.time.cmp(&b.time));
            }
            "time_desc" => {
                all_klines.sort_by(|a, b| b.time.cmp(&a.time));
            }
            _ => {
                // 默认按时间倒序(最新的在前) / Default sort by time descending (newest first)
                all_klines.sort_by(|a, b| b.time.cmp(&a.time));
            }
        }

        let total = all_klines.len();
        let offset = (page - 1) * limit;
        let has_prev = page > 1;
        let has_next = offset + limit < total;

        // 分页 / Pagination
        let klines = all_klines
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();

        debug!(
            "🔍 Retrieved {} klines for mint: {}, interval: {}",
            klines.len(),
            mint_account,
            interval
        );

        Ok(KlineQueryResponse {
            klines,
            total,
            page,
            limit,
            has_next,
            has_prev,
            interval: interval.clone(),
            mint_account: mint_account.clone(),
        })
    }
}
