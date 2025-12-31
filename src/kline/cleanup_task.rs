// K线缓存清理任务 / K-line cache cleanup task
// 定期清理过期的缓存条目以释放内存 / Periodically cleanup expired cache entries to free memory

use crate::kline::KlineCache;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

/// 缓存清理任务配置 / Cache cleanup task configuration
#[derive(Debug, Clone)]
pub struct CleanupConfig {
    /// 清理间隔(秒) / Cleanup interval (seconds)
    pub interval_secs: u64,
    /// 是否启用 / Whether enabled
    pub enabled: bool,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            interval_secs: 300, // 默认每5分钟清理一次 / Default cleanup every 5 minutes
            enabled: true,
        }
    }
}

/// 启动缓存清理任务 / Start cache cleanup task
/// 返回任务句柄,可用于取消任务 / Returns task handle for cancellation
pub fn start_cleanup_task(
    kline_cache: Arc<KlineCache>,
    config: CleanupConfig,
) -> tokio::task::JoinHandle<()> {
    info!(
        "🧹 启动K线缓存清理任务 / Starting K-line cache cleanup task: interval={}s, enabled={}",
        config.interval_secs, config.enabled
    );

    tokio::spawn(async move {
        if !config.enabled {
            info!("缓存清理任务已禁用 / Cache cleanup task disabled");
            return;
        }

        let mut interval = tokio::time::interval(Duration::from_secs(config.interval_secs));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            // 执行清理 / Perform cleanup
            info!("🧹 执行K线缓存清理 / Performing K-line cache cleanup");
            let stats_before = kline_cache.get_stats().await;

            kline_cache.cleanup_old_entries().await;

            let stats_after = kline_cache.get_stats().await;
            let removed = stats_before.total_entries.saturating_sub(stats_after.total_entries);

            info!(
                "✅ 缓存清理完成 / Cache cleanup completed: before={}, after={}, removed={}",
                stats_before.total_entries, stats_after.total_entries, removed
            );

            // 记录详细统计 / Log detailed statistics
            if !stats_after.by_interval.is_empty() {
                info!(
                    "📊 缓存统计 / Cache stats: s1={}, s30={}, m5={}",
                    stats_after.by_interval.get("s1").unwrap_or(&0),
                    stats_after.by_interval.get("s30").unwrap_or(&0),
                    stats_after.by_interval.get("m5").unwrap_or(&0)
                );
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kline::KlineCache;

    #[tokio::test]
    async fn test_cleanup_task() {
        let cache = Arc::new(KlineCache::new(1000, 1)); // 1秒过期

        // 添加一些数据
        cache.get_or_create("mint1", "s30", 1000, 100.0).await;
        cache.get_or_create("mint2", "s30", 2000, 200.0).await;

        let stats1 = cache.get_stats().await;
        assert_eq!(stats1.total_entries, 2);

        // 启动清理任务(每1秒清理一次)
        let config = CleanupConfig {
            interval_secs: 1,
            enabled: true,
        };
        let _handle = start_cleanup_task(Arc::clone(&cache), config);

        // 等待过期和清理
        tokio::time::sleep(Duration::from_secs(3)).await;

        let stats2 = cache.get_stats().await;
        assert_eq!(stats2.total_entries, 0);
    }
}
