// OrderBook 同步监控器 / OrderBook sync monitor
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use chrono::{DateTime, Utc};
use tracing::{info, warn, error};

use crate::config::OrderBookSyncConfig;
use super::service::OrderBookSyncService;

/// 最后事件信息 / Last event information
#[derive(Clone, Debug)]
pub struct LastEventInfo {
    /// 最后收到事件的时间 / Last event received time
    pub last_event_time: DateTime<Utc>,
    /// 是否正在同步 / Is syncing
    pub is_syncing: bool,
    /// 上次同步时间 / Last sync time
    pub last_sync_time: Option<DateTime<Utc>>,
}

/// 事件时间追踪器,在 Monitor 和 Service 之间共享
/// Event time tracker, shared between Monitor and Service
pub type EventTimeMap = Arc<RwLock<HashMap<String, LastEventInfo>>>;

/// OrderBook 同步监控器 / OrderBook sync monitor
pub struct OrderBookSyncMonitor {
    /// 配置 / Configuration
    config: OrderBookSyncConfig,
    /// mint -> LastEventInfo 映射 / mint -> LastEventInfo mapping
    last_events: EventTimeMap,
    /// 同步服务 / Sync service
    sync_service: Arc<OrderBookSyncService>,
    /// 是否运行中 / Is running
    is_running: Arc<RwLock<bool>>,
}

impl OrderBookSyncMonitor {
    /// 创建新的监控器 / Create new monitor
    pub fn new(
        config: OrderBookSyncConfig,
        sync_service: Arc<OrderBookSyncService>,
    ) -> Self {
        let last_events: EventTimeMap = Arc::new(RwLock::new(HashMap::new()));
        // 把事件时间映射共享给 SyncService,用于 rebuild 前检查
        // Share event time map with SyncService for pre-rebuild check
        sync_service.set_event_time_map(last_events.clone());
        Self {
            config,
            last_events,
            sync_service,
            is_running: Arc::new(RwLock::new(false)),
        }
    }

    /// 启动监控 / Start monitoring
    pub async fn start(&self) {
        info!("🚀 启动 OrderBook 同步监控器 / Starting OrderBook sync monitor");

        *self.is_running.write().await = true;

        let mut interval_timer = interval(Duration::from_secs(self.config.check_interval_seconds));

        loop {
            interval_timer.tick().await;

            if !*self.is_running.read().await {
                info!("⛔ OrderBook 同步监控器停止 / OrderBook sync monitor stopped");
                break;
            }

            self.check_and_sync().await;
        }
    }

    /// 停止监控 / Stop monitoring
    #[allow(dead_code)]
    pub async fn stop(&self) {
        info!("正在停止 OrderBook 同步监控器 / Stopping OrderBook sync monitor");
        *self.is_running.write().await = false;
    }

    /// 检查并触发同步 / Check and trigger sync
    async fn check_and_sync(&self) {
        let now = Utc::now();
        let threshold_secs = self.config.idle_threshold_seconds as i64;

        // 收集需要同步的 mint
        let mints_to_sync: Vec<String> = {
            let events = self.last_events.read().await;
            events
                .iter()
                .filter_map(|(mint, info)| {
                    // 检查条件 / Check conditions:
                    // 1. 未在同步中 / Not syncing
                    // 2. 超过阈值时间没有事件 / No event for threshold time
                    // 3. 上次同步后又过了阈值时间（避免重复同步）/ Threshold time passed since last sync
                    if !info.is_syncing {
                        let idle_seconds = now.signed_duration_since(info.last_event_time).num_seconds();

                        // 检查是否超过空闲阈值
                        if idle_seconds > threshold_secs {
                            // 检查是否需要再次同步（避免频繁同步）
                            if let Some(last_sync) = info.last_sync_time {
                                let since_last_sync = now.signed_duration_since(last_sync).num_seconds();
                                if since_last_sync > threshold_secs {
                                    Some(mint.clone())
                                } else {
                                    None
                                }
                            } else {
                                // 从未同步过
                                Some(mint.clone())
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect()
        };

        // 触发同步任务
        for mint in mints_to_sync {
            self.trigger_sync(mint).await;
        }
    }

    /// 触发同步任务 / Trigger sync task
    async fn trigger_sync(&self, mint: String) {
        // 标记为正在同步 / Mark as syncing
        {
            let mut events = self.last_events.write().await;
            if let Some(info) = events.get_mut(&mint) {
                if info.is_syncing {
                    return; // 已在同步中，跳过
                }
                info.is_syncing = true;
            } else {
                return; // mint 不存在，跳过
            }
        }

        let sync_service = self.sync_service.clone();
        let last_events = self.last_events.clone();
        let mint_short = if mint.len() > 8 { &mint[..8] } else { &mint };
        let mint_short = mint_short.to_string();

        // 异步执行同步
        tokio::spawn(async move {
            info!("🔄 触发 OrderBook 同步 / Triggering OrderBook sync: mint={}", mint_short);

            match sync_service.sync_orderbook(&mint).await {
                Ok(result) => {
                    if !result.fully_matched {
                        warn!(
                            "⚠️ OrderBook 同步发现差异并已修复 / OrderBook sync found differences and repaired: mint={}, repaired_count={}",
                            mint_short, result.repaired_count
                        );
                    } else {
                        info!(
                            "✅ OrderBook 同步验证通过 / OrderBook sync validation passed: mint={}",
                            mint_short
                        );
                    }
                }
                Err(e) => {
                    error!(
                        "❌ OrderBook 同步失败 / OrderBook sync failed: mint={}, error={}",
                        mint_short, e
                    );
                }
            }

            // 更新同步状态 / Update sync status
            let mut events = last_events.write().await;
            if let Some(info) = events.get_mut(&mint) {
                info.is_syncing = false;
                info.last_sync_time = Some(Utc::now());
            }
        });
    }

    /// 更新事件时间（由事件处理器调用）/ Update event time (called by event handler)
    pub async fn update_event_time(&self, mint: &str) {
        let mut events = self.last_events.write().await;

        if let Some(info) = events.get_mut(mint) {
            // 更新已存在的记录
            info.last_event_time = Utc::now();
            // 重要：清除上次同步时间，这样在新的空闲期会重新触发对比
            // Important: Clear last sync time, so comparison will be triggered in new idle period
            info.last_sync_time = None;
        } else {
            // 插入新记录
            events.insert(
                mint.to_string(),
                LastEventInfo {
                    last_event_time: Utc::now(),
                    is_syncing: false,
                    last_sync_time: None,
                },
            );
        }
    }

    /// 获取监控状态（用于调试）/ Get monitor status (for debugging)
    #[allow(dead_code)]
    pub async fn get_status(&self) -> HashMap<String, (DateTime<Utc>, bool, Option<DateTime<Utc>>)> {
        let events = self.last_events.read().await;
        events
            .iter()
            .map(|(mint, info)| {
                (
                    mint.clone(),
                    (info.last_event_time, info.is_syncing, info.last_sync_time),
                )
            })
            .collect()
    }
}