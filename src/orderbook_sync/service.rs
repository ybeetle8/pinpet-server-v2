// OrderBook 同步服务 / OrderBook sync service
use anyhow::Result;
use std::sync::Arc;
use tracing::{info, warn, error};
use serde::{Serialize, Deserialize};
use utoipa::ToSchema;
use chrono::{DateTime, Utc};

use crate::config::OrderBookSyncConfig;
use crate::db::OrderBookStorage;
use crate::solana::orderbook_reader::OrderBookReader;
use crate::solana::orderbook_comparator::{OrderBookComparator, ComparisonResult, OrderBookComparison};
use super::monitor::EventTimeMap;

/// 同步结果 / Sync result
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SyncResult {
    /// Token mint 地址 / Token mint address
    pub mint: String,
    /// 是否完全匹配 / Fully matched
    pub fully_matched: bool,
    /// 修复的记录数 / Repaired records count
    pub repaired_count: usize,
    /// 错误列表 / Error list
    pub errors: Vec<String>,
}

/// OrderBook 同步服务 / OrderBook sync service
pub struct OrderBookSyncService {
    /// OrderBook 读取器 / OrderBook reader
    reader: OrderBookReader,
    /// OrderBook 对比器 / OrderBook comparator
    comparator: OrderBookComparator,
    /// OrderBook 存储 / OrderBook storage
    orderbook_storage: Arc<OrderBookStorage>,
    /// 配置 / Configuration
    config: OrderBookSyncConfig,
    /// 事件时间映射(由 Monitor 设置) / Event time map (set by Monitor)
    event_time_map: std::sync::RwLock<Option<EventTimeMap>>,
}

impl OrderBookSyncService {
    /// 创建新的同步服务 / Create new sync service
    pub fn new(
        reader: OrderBookReader,
        comparator: OrderBookComparator,
        orderbook_storage: Arc<OrderBookStorage>,
        config: OrderBookSyncConfig,
    ) -> Self {
        Self {
            reader,
            comparator,
            orderbook_storage,
            config,
            event_time_map: std::sync::RwLock::new(None),
        }
    }

    /// 设置事件时间映射(由 Monitor 调用) / Set event time map (called by Monitor)
    pub fn set_event_time_map(&self, map: EventTimeMap) {
        *self.event_time_map.write().unwrap() = Some(map);
    }

    /// 检查指定 mint 在给定时间之后是否有新事件到达
    /// Check if new events arrived for specified mint after the given time
    async fn has_new_events_since(&self, mint: &str, since: DateTime<Utc>) -> bool {
        let map = {
            let guard = self.event_time_map.read().unwrap();
            guard.clone()
        };
        if let Some(map) = map {
            let events = map.read().await;
            if let Some(info) = events.get(mint) {
                return info.last_event_time > since;
            }
        }
        false
    }

    /// 同步指定 mint 的 OrderBook / Sync OrderBook for specified mint
    pub async fn sync_orderbook(&self, mint: &str) -> Result<SyncResult> {
        let mint_short = if mint.len() > 8 { &mint[..8] } else { mint };
        info!("📊 开始同步 OrderBook / Starting OrderBook sync: mint={}", mint_short);

        // 1. 执行对比 / Execute comparison
        let comparison = self.comparator.compare(mint).await?;

        // 2. 检查是否需要同步（双重检查：fully_matched 标志 + 实际差异检查）
        // Check if sync is needed (double check: fully_matched flag + actual differences check)
        let has_actual_diff = self.has_actual_differences(&comparison);

        if comparison.fully_matched && !has_actual_diff {
            info!(
                "✅ OrderBook 完全匹配，无需同步 / OrderBook fully matched, no sync needed: mint={}",
                mint_short
            );
            return Ok(SyncResult {
                mint: mint.to_string(),
                fully_matched: true,
                repaired_count: 0,
                errors: vec![],
            });
        }

        // 如果 fully_matched 与实际差异不一致，记录警告并继续同步
        // If fully_matched doesn't match actual differences, log warning and proceed with sync
        if comparison.fully_matched && has_actual_diff {
            warn!(
                "⚠️ 检测到不一致 / Inconsistency detected: fully_matched=true 但存在实际差异 / but actual differences found! \
                 mint={}, 继续执行同步 / Proceeding with sync",
                mint_short
            );
        }

        // 3. 发现差异，执行修复 / Found differences, execute repair
        let repaired_count = self.count_differences(&comparison);
        warn!(
            "⚠️ 发现 OrderBook 差异 / Found OrderBook differences: mint={}, count={}",
            mint_short, repaired_count
        );

        if self.config.auto_repair {
            match self.repair_differences(mint, &comparison).await {
                Ok(_) => {
                    info!(
                        "✅ OrderBook 差异已修复 / OrderBook differences repaired: mint={}, count={}",
                        mint_short, repaired_count
                    );
                }
                Err(e) => {
                    error!(
                        "❌ OrderBook 修复失败 / OrderBook repair failed: mint={}, error={}",
                        mint_short, e
                    );
                    return Ok(SyncResult {
                        mint: mint.to_string(),
                        fully_matched: false,
                        repaired_count: 0,
                        errors: vec![format!("Repair failed: {}", e)],
                    });
                }
            }
        } else {
            warn!(
                "⚠️ 发现 OrderBook 差异但未启用自动修复 / Found differences but auto-repair disabled: mint={}",
                mint_short
            );
        }

        Ok(SyncResult {
            mint: mint.to_string(),
            fully_matched: false,
            repaired_count,
            errors: comparison.errors,
        })
    }

    /// 修复差异 / Repair differences
    async fn repair_differences(&self, mint: &str, comparison: &ComparisonResult) -> Result<()> {
        let mint_short = if mint.len() > 8 { &mint[..8] } else { mint };

        // 处理 up (做空) OrderBook / Handle up (short) OrderBook
        if self.needs_repair(&comparison.up_orderbook) {
            info!("🔧 修复 up OrderBook / Repairing up OrderBook: mint={}", mint_short);
            self.repair_single_orderbook(mint, "up").await?;
        }

        // 处理 down (做多) OrderBook / Handle down (long) OrderBook
        if self.needs_repair(&comparison.down_orderbook) {
            info!("🔧 修复 down OrderBook / Repairing down OrderBook: mint={}", mint_short);
            self.repair_single_orderbook(mint, "dn").await?;
        }

        Ok(())
    }

    /// 检查是否存在实际差异 / Check if actual differences exist
    fn has_actual_differences(&self, comparison: &ComparisonResult) -> bool {
        let up_has_diff = !comparison.up_orderbook.order_differences.is_empty()
            || !comparison.up_orderbook.chain_only_orders.is_empty()
            || !comparison.up_orderbook.db_only_orders.is_empty();

        let down_has_diff = !comparison.down_orderbook.order_differences.is_empty()
            || !comparison.down_orderbook.chain_only_orders.is_empty()
            || !comparison.down_orderbook.db_only_orders.is_empty();

        up_has_diff || down_has_diff
    }

    /// 判断是否需要修复 / Check if repair is needed
    fn needs_repair(&self, comparison: &OrderBookComparison) -> bool {
        !comparison.order_differences.is_empty()
            || !comparison.chain_only_orders.is_empty()
            || !comparison.db_only_orders.is_empty()
    }

    /// 修复单个 OrderBook / Repair single OrderBook
    async fn repair_single_orderbook(
        &self,
        mint: &str,
        direction: &str,
    ) -> Result<()> {
        let mint_short = if mint.len() > 8 { &mint[..8] } else { mint };

        // 🔧 记录读取链上数据前的时间,用于检测竞态
        // 🔧 Record time before reading chain data, used to detect race condition
        let read_start = Utc::now();

        // 获取链上完整数据 / Get complete chain data
        let (chain_header, chain_orders) = self.reader
            .get_orderbook_from_chain(mint, direction)
            .await?;

        // 🔧 检查读取链上数据期间是否有新事件到达
        // 如果有,说明本地数据已变化,链上快照可能已过时,放弃本次 rebuild
        // 🔧 Check if new events arrived during chain data reading
        // If so, local data has changed, chain snapshot may be stale, abort this rebuild
        if self.has_new_events_since(mint, read_start).await {
            warn!(
                "⚠️ rebuild 期间检测到新事件,放弃本次修复 / New events detected during rebuild, aborting repair: mint={}, direction={}",
                mint_short, direction
            );
            return Ok(());
        }

        // 获取管理器 / Get manager
        let manager = self.orderbook_storage
            .get_or_create_manager(mint.to_string(), direction.to_string())?;

        // 清空本地数据并重建 / Clear local data and rebuild
        manager.rebuild_from_chain_data(chain_header, chain_orders)?;

        info!(
            "✅ OrderBook 修复完成 / OrderBook repair completed: mint={}, direction={}",
            mint_short, direction
        );

        Ok(())
    }

    /// 统计差异数量 / Count differences
    fn count_differences(&self, comparison: &ComparisonResult) -> usize {
        comparison.up_orderbook.order_differences.len()
            + comparison.up_orderbook.chain_only_orders.len()
            + comparison.up_orderbook.db_only_orders.len()
            + comparison.down_orderbook.order_differences.len()
            + comparison.down_orderbook.chain_only_orders.len()
            + comparison.down_orderbook.db_only_orders.len()
    }

    /// 强制同步 OrderBook（跳过 fully_matched 检查）/ Force sync OrderBook (skip fully_matched check)
    ///
    /// 无论 fully_matched 标志如何，都会检查实际差异并执行修复
    /// Check actual differences and repair regardless of fully_matched flag
    pub async fn force_sync_orderbook(&self, mint: &str) -> Result<SyncResult> {
        let mint_short = if mint.len() > 8 { &mint[..8] } else { mint };
        info!("🔄 强制同步 OrderBook / Force sync OrderBook: mint={}", mint_short);

        // 1. 执行对比 / Execute comparison
        let comparison = self.comparator.compare(mint).await?;

        // 2. 检查实际差异（忽略 fully_matched 标志）
        // Check actual differences (ignore fully_matched flag)
        let has_actual_diff = self.has_actual_differences(&comparison);

        if !has_actual_diff {
            info!(
                "✅ OrderBook 无实际差异，无需同步 / No actual differences, no sync needed: mint={}",
                mint_short
            );
            return Ok(SyncResult {
                mint: mint.to_string(),
                fully_matched: comparison.fully_matched,
                repaired_count: 0,
                errors: comparison.errors,
            });
        }

        // 3. 发现差异，执行修复 / Found differences, execute repair
        let repaired_count = self.count_differences(&comparison);
        warn!(
            "⚠️ 发现 OrderBook 差异（强制同步）/ Found OrderBook differences (force sync): mint={}, count={}",
            mint_short, repaired_count
        );

        if self.config.auto_repair {
            match self.repair_differences(mint, &comparison).await {
                Ok(_) => {
                    info!(
                        "✅ OrderBook 差异已修复（强制同步）/ OrderBook differences repaired (force sync): mint={}, count={}",
                        mint_short, repaired_count
                    );
                }
                Err(e) => {
                    error!(
                        "❌ OrderBook 修复失败（强制同步）/ OrderBook repair failed (force sync): mint={}, error={}",
                        mint_short, e
                    );
                    return Ok(SyncResult {
                        mint: mint.to_string(),
                        fully_matched: false,
                        repaired_count: 0,
                        errors: vec![format!("Repair failed: {}", e)],
                    });
                }
            }
        } else {
            warn!(
                "⚠️ 发现 OrderBook 差异但未启用自动修复（强制同步）/ Found differences but auto-repair disabled (force sync): mint={}",
                mint_short
            );
        }

        Ok(SyncResult {
            mint: mint.to_string(),
            fully_matched: comparison.fully_matched,
            repaired_count,
            errors: comparison.errors,
        })
    }

    /// 同步所有 OrderBook（批量同步，用于初始化或定期检查）/ Sync all OrderBooks
    #[allow(dead_code)]
    pub async fn sync_all_orderbooks(&self, mints: Vec<String>) -> Vec<SyncResult> {
        let mut results = Vec::new();

        for mint in mints {
            match self.sync_orderbook(&mint).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    error!(
                        "❌ 同步 OrderBook 失败 / Failed to sync OrderBook: mint={}, error={}",
                        &mint[..8.min(mint.len())], e
                    );
                    results.push(SyncResult {
                        mint,
                        fully_matched: false,
                        repaired_count: 0,
                        errors: vec![e.to_string()],
                    });
                }
            }
        }

        results
    }
}