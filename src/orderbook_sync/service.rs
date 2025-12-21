// OrderBook 同步服务 / OrderBook sync service
use anyhow::Result;
use std::sync::Arc;
use tracing::{info, warn, error};
use serde::{Serialize, Deserialize};

use crate::config::OrderBookSyncConfig;
use crate::db::OrderBookStorage;
use crate::solana::orderbook_reader::{OrderBookReader, ChainOrderBookHeader};
use crate::solana::orderbook_comparator::{OrderBookComparator, ComparisonResult, OrderBookComparison};
use crate::orderbook::MarginOrder;

/// 同步结果 / Sync result
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        }
    }

    /// 同步指定 mint 的 OrderBook / Sync OrderBook for specified mint
    pub async fn sync_orderbook(&self, mint: &str) -> Result<SyncResult> {
        let mint_short = if mint.len() > 8 { &mint[..8] } else { mint };
        info!("📊 开始同步 OrderBook / Starting OrderBook sync: mint={}", mint_short);

        // 1. 执行对比 / Execute comparison
        let comparison = self.comparator.compare(mint).await?;

        // 2. 如果完全匹配，无需同步 / If fully matched, no need to sync
        if comparison.fully_matched {
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

        // 获取链上完整数据 / Get complete chain data
        let (chain_header, chain_orders) = self.reader
            .get_orderbook_from_chain(mint, direction)
            .await?;

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