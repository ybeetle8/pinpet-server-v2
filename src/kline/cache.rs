// K线内存缓存模块 / K-line memory cache module
// 用于高性能实时K线推送 / For high-performance real-time K-line push

use crate::kline::types::KlineData;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// K线缓存键 / K-line cache key
/// 格式: (mint_account, interval, time_bucket) / Format: (mint_account, interval, time_bucket)
type CacheKey = (String, String, u64);

/// K线内存缓存 / K-line memory cache
/// 缓存最近的K线数据,避免频繁读取数据库 / Cache recent K-line data to avoid frequent DB reads
pub struct KlineCache {
    /// 缓存存储 / Cache storage
    cache: Arc<RwLock<HashMap<CacheKey, KlineData>>>,
    /// 最大缓存条目数 / Maximum cache entries
    max_entries: usize,
    /// 缓存过期时间(秒) / Cache expiration time (seconds)
    expiration_secs: u64,
}

impl KlineCache {
    /// 创建新的K线缓存 / Create new K-line cache
    pub fn new(max_entries: usize, expiration_secs: u64) -> Self {
        info!(
            "📦 初始化K线缓存 / Initializing K-line cache: max_entries={}, expiration_secs={}",
            max_entries, expiration_secs
        );

        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
            expiration_secs,
        }
    }

    /// 获取或创建K线数据 / Get or create K-line data
    /// 如果缓存中存在则更新,否则创建新K线 / Update if exists in cache, otherwise create new
    pub async fn get_or_create(
        &self,
        mint: &str,
        interval: &str,
        time_bucket: u64,
        current_price: f64,
        event_timestamp: u64,
    ) -> KlineData {
        let key = (mint.to_string(), interval.to_string(), time_bucket);

        let mut cache = self.cache.write().await;

        // 检查缓存中是否存在该时间桶的K线 / Check if K-line exists in cache for this time bucket
        if let Some(existing_kline) = cache.get_mut(&key) {
            // 检查事件时间戳,拒绝旧事件覆盖 / Check event timestamp, reject old events
            if event_timestamp < existing_kline.last_event_timestamp {
                debug!(
                    "⚠️ 拒绝乱序事件覆盖 / Rejecting out-of-order event: mint={}, interval={}, time={}, event_ts={}, existing_ts={}",
                    mint, interval, time_bucket, event_timestamp, existing_kline.last_event_timestamp
                );
                return existing_kline.clone();
            }

            // 更新现有K线的high/low/close / Update existing K-line high/low/close
            existing_kline.high = existing_kline.high.max(current_price);
            existing_kline.low = existing_kline.low.min(current_price);
            existing_kline.close = current_price;
            existing_kline.update_count += 1;
            existing_kline.is_final = false;
            existing_kline.last_event_timestamp = event_timestamp;

            debug!(
                "📊 缓存命中,更新K线 / Cache hit, K-line updated: mint={}, interval={}, time={}, OHLC=[{},{},{},{}], count={}, event_ts={}",
                mint, interval, time_bucket, existing_kline.open, existing_kline.high, existing_kline.low, existing_kline.close, existing_kline.update_count, event_timestamp
            );

            return existing_kline.clone();
        }

        // 缓存未命中,创建新K线 / Cache miss, create new K-line
        // 尝试从上一个时间桶获取收盘价作为开盘价 / Try to get close price from previous time bucket as open price
        let open_price = self.get_previous_close_from_cache(&cache, mint, interval, time_bucket)
            .unwrap_or(current_price);

        let new_kline = KlineData {
            time: time_bucket,
            open: open_price,
            high: current_price,
            low: current_price,
            close: current_price,
            volume: 0.0,
            is_final: false,
            update_count: 1,
            last_event_timestamp: event_timestamp,
        };

        debug!(
            "🆕 缓存未命中,创建新K线 / Cache miss, new K-line created: mint={}, interval={}, time={}, OHLC=[{},{},{},{}], event_ts={}",
            mint, interval, time_bucket, new_kline.open, new_kline.high, new_kline.low, new_kline.close, event_timestamp
        );

        // 插入缓存 / Insert into cache
        cache.insert(key, new_kline.clone());

        // 检查缓存大小,必要时清理 / Check cache size, cleanup if necessary
        if cache.len() > self.max_entries {
            debug!(
                "⚠️ 缓存超过最大限制 / Cache exceeded max entries: {} > {}",
                cache.len(),
                self.max_entries
            );
            drop(cache); // 释放写锁 / Release write lock
            self.cleanup_old_entries_internal().await;
        }

        new_kline
    }

    /// 从缓存中获取上一个K线的收盘价 / Get previous K-line close price from cache
    /// 保持价格连续性,避免gap / Maintain price continuity and avoid gaps
    fn get_previous_close_from_cache(
        &self,
        cache: &HashMap<CacheKey, KlineData>,
        mint: &str,
        interval: &str,
        current_time_bucket: u64,
    ) -> Option<f64> {
        // 查找该mint和interval下所有早于当前时间桶的K线 / Find all K-lines before current time bucket for this mint and interval
        let mut previous_klines: Vec<(&CacheKey, &KlineData)> = cache
            .iter()
            .filter(|((m, i, t), _)| {
                m == mint && i == interval && *t < current_time_bucket
            })
            .collect();

        // 按时间降序排序,获取最近的一个 / Sort by time descending, get the most recent one
        previous_klines.sort_by(|a, b| b.0.2.cmp(&a.0.2));

        // 返回最近K线的收盘价 / Return close price of most recent K-line
        previous_klines.first().map(|(_, kline)| kline.close)
    }

    /// 更新缓存中的K线数据 / Update K-line data in cache
    /// 用于从数据库加载数据后同步到缓存 / Used to sync data to cache after loading from DB
    pub async fn update(&self, mint: &str, interval: &str, kline: KlineData) {
        let key = (mint.to_string(), interval.to_string(), kline.time);
        let time = kline.time; // 保存time用于日志 / Save time for logging
        let mut cache = self.cache.write().await;
        cache.insert(key, kline);

        debug!(
            "🔄 缓存更新 / Cache updated: mint={}, interval={}, time={}",
            mint, interval, time
        );
    }

    /// 清理过期的缓存条目 / Cleanup expired cache entries
    /// 定期调用以释放内存 / Call periodically to free memory
    pub async fn cleanup_old_entries(&self) {
        self.cleanup_old_entries_internal().await;
    }

    /// 内部清理实现 / Internal cleanup implementation
    async fn cleanup_old_entries_internal(&self) {
        let now = Utc::now().timestamp() as u64;
        let mut cache = self.cache.write().await;

        let initial_size = cache.len();

        // 保留未过期的条目 / Retain non-expired entries
        cache.retain(|(_mint, _interval, time), _| {
            now - time < self.expiration_secs
        });

        let removed = initial_size - cache.len();
        if removed > 0 {
            info!(
                "🧹 缓存清理完成 / Cache cleanup completed: removed={}, remaining={}",
                removed,
                cache.len()
            );
        }
    }

    /// 获取缓存统计信息 / Get cache statistics
    pub async fn get_stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        let total_entries = cache.len();

        // 按interval统计 / Count by interval
        let mut by_interval: HashMap<String, usize> = HashMap::new();
        for ((_mint, interval, _time), _) in cache.iter() {
            *by_interval.entry(interval.clone()).or_insert(0) += 1;
        }

        CacheStats {
            total_entries,
            max_entries: self.max_entries,
            by_interval,
            expiration_secs: self.expiration_secs,
        }
    }

    /// 清空缓存 / Clear cache
    /// 用于测试或重置 / Used for testing or reset
    #[allow(dead_code)]
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        info!("🗑️ 缓存已清空 / Cache cleared");
    }
}

/// 缓存统计信息 / Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,      // 总条目数 / Total entries
    pub max_entries: usize,         // 最大条目数 / Max entries
    pub by_interval: HashMap<String, usize>, // 按interval统计 / Count by interval
    pub expiration_secs: u64,       // 过期时间 / Expiration time
}

impl Default for KlineCache {
    fn default() -> Self {
        // 默认配置: 最多10000条,保留10分钟 / Default config: max 10000 entries, retain 10 minutes
        Self::new(10000, 600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_kline_cache_get_or_create() {
        let cache = KlineCache::new(1000, 600);

        // 第一次创建 / First creation
        let kline1 = cache.get_or_create("mint1", "s30", 1000, 100.0, 1000).await;
        assert_eq!(kline1.open, 100.0);
        assert_eq!(kline1.high, 100.0);
        assert_eq!(kline1.low, 100.0);
        assert_eq!(kline1.close, 100.0);
        assert_eq!(kline1.update_count, 1);
        assert_eq!(kline1.last_event_timestamp, 1000);

        // 同一时间桶更新 / Update in same time bucket
        let kline2 = cache.get_or_create("mint1", "s30", 1000, 110.0, 1001).await;
        assert_eq!(kline2.open, 100.0);  // open不变 / open unchanged
        assert_eq!(kline2.high, 110.0);  // high更新 / high updated
        assert_eq!(kline2.low, 100.0);
        assert_eq!(kline2.close, 110.0); // close更新 / close updated
        assert_eq!(kline2.update_count, 2);
        assert_eq!(kline2.last_event_timestamp, 1001);

        // 乱序事件应该被拒绝 / Out-of-order events should be rejected
        let kline_old = cache.get_or_create("mint1", "s30", 1000, 95.0, 999).await;
        assert_eq!(kline_old.close, 110.0); // close不变,旧事件被拒绝 / close unchanged, old event rejected
        assert_eq!(kline_old.update_count, 2); // 更新次数不变 / count unchanged

        // 新时间桶,open使用上一个K线的close / New time bucket, open uses previous close
        let kline3 = cache.get_or_create("mint1", "s30", 1030, 105.0, 1030).await;
        assert_eq!(kline3.open, 110.0);  // 上一个K线的close / Previous close
        assert_eq!(kline3.close, 105.0);
        assert_eq!(kline3.update_count, 1);
        assert_eq!(kline3.last_event_timestamp, 1030);
    }

    #[tokio::test]
    async fn test_cache_cleanup() {
        let cache = KlineCache::new(1000, 1); // 1秒过期 / 1 second expiration

        // 创建一些K线 / Create some K-lines
        cache.get_or_create("mint1", "s30", 1000, 100.0, 1000).await;
        cache.get_or_create("mint2", "s30", 2000, 200.0, 2000).await;

        let stats1 = cache.get_stats().await;
        assert_eq!(stats1.total_entries, 2);

        // 等待过期 / Wait for expiration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // 清理 / Cleanup
        cache.cleanup_old_entries().await;

        let stats2 = cache.get_stats().await;
        assert_eq!(stats2.total_entries, 0);
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = KlineCache::new(1000, 600);

        cache.get_or_create("mint1", "s1", 1000, 100.0, 1000).await;
        cache.get_or_create("mint1", "s30", 1000, 100.0, 1000).await;
        cache.get_or_create("mint1", "m5", 1000, 100.0, 1000).await;
        cache.get_or_create("mint2", "s1", 1000, 200.0, 1000).await;

        let stats = cache.get_stats().await;
        assert_eq!(stats.total_entries, 4);
        assert_eq!(*stats.by_interval.get("s1").unwrap(), 2);
        assert_eq!(*stats.by_interval.get("s30").unwrap(), 1);
        assert_eq!(*stats.by_interval.get("m5").unwrap(), 1);
    }
}
