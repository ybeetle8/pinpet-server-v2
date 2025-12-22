// OrderBook 对比器 / OrderBook comparator
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, warn};
use utoipa::ToSchema;

use crate::orderbook::MarginOrder;
use crate::solana::orderbook_reader::OrderBookReader;

/// 订单对比结果 / Order comparison result
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderComparison {
    /// 订单ID / Order ID
    pub order_id: u64,

    /// 索引位置 / Index position
    pub index: u16,

    /// 字段差异列表 / Field differences list
    pub differences: Vec<FieldDifference>,

    /// 差异来源: "chain_only", "db_only", "mismatch"
    /// Difference source: "chain_only", "db_only", "mismatch"
    pub diff_type: String,
}

/// 字段差异 / Field difference
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FieldDifference {
    /// 字段名 / Field name
    pub field: String,

    /// 链上值 / Chain value
    pub chain_value: String,

    /// 数据库值 / Database value
    pub db_value: String,
}

/// OrderBook 对比结果 / OrderBook comparison result
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderBookComparison {
    /// 方向: "up" 或 "dn" / Direction: "up" or "dn"
    pub direction: String,

    /// 链上总数 / Chain total count
    pub chain_total: u16,

    /// 数据库总数 / Database total count
    pub db_total: u16,

    /// 总数是否匹配 / Whether total count matches
    pub total_match: bool,

    /// 有差异的订单列表 / Orders with differences
    pub order_differences: Vec<OrderComparison>,

    /// 仅存在于链上的订单 / Orders only on chain
    pub chain_only_orders: Vec<OrderSummary>,

    /// 仅存在于数据库的订单 / Orders only in database
    pub db_only_orders: Vec<OrderSummary>,

    /// 匹配的订单数量 / Number of matching orders
    pub matching_orders: u32,

    /// 不匹配的订单数量 / Number of mismatching orders
    pub mismatching_orders: u32,
}

/// 订单摘要 / Order summary
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderSummary {
    pub order_id: u64,
    pub index: u16,
    pub user: String,
}

/// 完整对比结果 / Complete comparison result
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ComparisonResult {
    /// Token mint 地址 / Token mint address
    pub mint: String,

    /// 对比时间戳 / Comparison timestamp
    pub timestamp: u64,

    /// 做空订单对比结果 / Short order comparison result
    pub up_orderbook: OrderBookComparison,

    /// 做多订单对比结果 / Long order comparison result
    pub down_orderbook: OrderBookComparison,

    /// 总体是否完全匹配 / Overall match status
    pub fully_matched: bool,

    /// 错误信息（如果有）/ Error messages (if any)
    pub errors: Vec<String>,
}

/// OrderBook 对比器 / OrderBook comparator
pub struct OrderBookComparator {
    reader: OrderBookReader,
    orderbook_storage: std::sync::Arc<crate::db::OrderBookStorage>,
}

impl OrderBookComparator {
    /// 创建新的对比器 / Create new comparator
    pub fn new(
        reader: OrderBookReader,
        orderbook_storage: std::sync::Arc<crate::db::OrderBookStorage>,
    ) -> Self {
        Self {
            reader,
            orderbook_storage,
        }
    }

    /// 执行对比 / Execute comparison
    pub async fn compare(&self, mint: &str) -> Result<ComparisonResult> {
        info!("开始对比 OrderBook / Starting OrderBook comparison: mint={}", &mint[..8.min(mint.len())]);

        let mut errors = Vec::new();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        // 对比 up (做空) orderbook
        let up_comparison = match self.compare_single_orderbook(mint, "up").await {
            Ok(result) => result,
            Err(e) => {
                let error_msg = format!("Failed to compare up orderbook: {}", e);
                warn!("{}", error_msg);
                errors.push(error_msg);
                OrderBookComparison {
                    direction: "up".to_string(),
                    chain_total: 0,
                    db_total: 0,
                    total_match: false,
                    order_differences: vec![],
                    chain_only_orders: vec![],
                    db_only_orders: vec![],
                    matching_orders: 0,
                    mismatching_orders: 0,
                }
            }
        };

        // 对比 down (做多) orderbook
        let down_comparison = match self.compare_single_orderbook(mint, "dn").await {
            Ok(result) => result,
            Err(e) => {
                let error_msg = format!("Failed to compare down orderbook: {}", e);
                warn!("{}", error_msg);
                errors.push(error_msg);
                OrderBookComparison {
                    direction: "dn".to_string(),
                    chain_total: 0,
                    db_total: 0,
                    total_match: false,
                    order_differences: vec![],
                    chain_only_orders: vec![],
                    db_only_orders: vec![],
                    matching_orders: 0,
                    mismatching_orders: 0,
                }
            }
        };

        let fully_matched = errors.is_empty()
            && up_comparison.total_match
            && up_comparison.order_differences.is_empty()
            && up_comparison.chain_only_orders.is_empty()
            && up_comparison.db_only_orders.is_empty()
            && down_comparison.total_match
            && down_comparison.order_differences.is_empty()
            && down_comparison.chain_only_orders.is_empty()
            && down_comparison.db_only_orders.is_empty();

        // 添加详细的调试日志 / Add detailed debug logging
        debug!(
            "fully_matched 判定详情 / fully_matched decision details: \
             mint={}, errors_empty={}, \
             up[total_match={}, diff_empty={}, chain_only_empty={} (count={}), db_only_empty={} (count={})], \
             down[total_match={}, diff_empty={}, chain_only_empty={} (count={}), db_only_empty={} (count={})], \
             => fully_matched={}",
            &mint[..8.min(mint.len())],
            errors.is_empty(),
            up_comparison.total_match,
            up_comparison.order_differences.is_empty(),
            up_comparison.chain_only_orders.is_empty(),
            up_comparison.chain_only_orders.len(),
            up_comparison.db_only_orders.is_empty(),
            up_comparison.db_only_orders.len(),
            down_comparison.total_match,
            down_comparison.order_differences.is_empty(),
            down_comparison.chain_only_orders.is_empty(),
            down_comparison.chain_only_orders.len(),
            down_comparison.db_only_orders.is_empty(),
            down_comparison.db_only_orders.len(),
            fully_matched
        );

        // 如果 fully_matched 与实际差异不符，记录警告
        // If fully_matched doesn't match actual differences, log warning
        let has_actual_diff = !up_comparison.chain_only_orders.is_empty()
            || !up_comparison.db_only_orders.is_empty()
            || !up_comparison.order_differences.is_empty()
            || !down_comparison.chain_only_orders.is_empty()
            || !down_comparison.db_only_orders.is_empty()
            || !down_comparison.order_differences.is_empty();

        if fully_matched && has_actual_diff {
            warn!(
                "⚠️ 逻辑错误检测 / Logic error detected: fully_matched=true 但存在实际差异 / but has actual differences! \
                 mint={}, up_chain_only={}, up_db_only={}, down_chain_only={}, down_db_only={}",
                &mint[..8.min(mint.len())],
                up_comparison.chain_only_orders.len(),
                up_comparison.db_only_orders.len(),
                down_comparison.chain_only_orders.len(),
                down_comparison.db_only_orders.len()
            );
        }

        Ok(ComparisonResult {
            mint: mint.to_string(),
            timestamp,
            up_orderbook: up_comparison,
            down_orderbook: down_comparison,
            fully_matched,
            errors,
        })
    }

    /// 对比单个 OrderBook / Compare single OrderBook
    async fn compare_single_orderbook(&self, mint: &str, direction: &str) -> Result<OrderBookComparison> {
        debug!("对比 {} OrderBook / Comparing {} OrderBook", direction, direction);

        // 1. 获取链上数据 / Get chain data
        let (chain_header, chain_orders) = self.reader.get_orderbook_from_chain(mint, direction).await
            .context("获取链上数据失败 / Failed to get chain data")?;

        // 2. 获取数据库数据 / Get database data
        let manager = self.orderbook_storage.get_or_create_manager(mint.to_string(), direction.to_string())?;

        // 获取数据库 header
        let db_header = match manager.load_header() {
            Ok(h) => h,
            Err(_) => {
                // 数据库中没有数据
                info!("数据库中没有 {} OrderBook / No {} OrderBook in database", direction, direction);
                return Ok(OrderBookComparison {
                    direction: direction.to_string(),
                    chain_total: chain_header.total,
                    db_total: 0,
                    total_match: chain_header.total == 0,
                    order_differences: vec![],
                    chain_only_orders: chain_orders.into_iter().map(|(idx, order)| OrderSummary {
                        order_id: order.order_id,
                        index: idx,
                        user: order.user.clone(),
                    }).collect(),
                    db_only_orders: vec![],
                    matching_orders: 0,
                    mismatching_orders: 0,
                });
            }
        };

        // 获取数据库订单
        let mut db_orders = HashMap::new();
        if db_header.total > 0 {
            // 使用 traverse 遍历所有订单
            manager.traverse(
                u16::MAX, // 从头开始
                0,        // 不限制数量
                |index, order| {
                    db_orders.insert(order.order_id, (index, order.clone()));
                    Ok(true) // 继续遍历
                },
            )?;
        }

        // 3. 构建链上订单映射 / Build chain order map
        let mut chain_order_map: HashMap<u64, (u16, MarginOrder)> = HashMap::new();
        for (index, order) in chain_orders.iter() {
            chain_order_map.insert(order.order_id, (*index, order.clone()));
        }

        // 4. 对比订单 / Compare orders
        let mut order_differences = Vec::new();
        let mut matching_orders = 0u32;
        let mut chain_only_orders = Vec::new();
        let mut db_only_orders = Vec::new();

        // 检查链上的每个订单 / Check each chain order
        for (order_id, (chain_index, chain_order)) in chain_order_map.iter() {
            if let Some((db_index, db_order)) = db_orders.get(order_id) {
                // 订单同时存在，对比字段 / Order exists in both, compare fields
                let differences = self.compare_order_fields(chain_order, db_order, *chain_index, *db_index);

                if !differences.is_empty() {
                    order_differences.push(OrderComparison {
                        order_id: *order_id,
                        index: *chain_index,
                        differences,
                        diff_type: "mismatch".to_string(),
                    });
                } else {
                    matching_orders += 1;
                }
            } else {
                // 仅在链上存在 / Only exists on chain
                chain_only_orders.push(OrderSummary {
                    order_id: *order_id,
                    index: *chain_index,
                    user: chain_order.user.clone(),
                });
            }
        }

        // 检查仅在数据库中的订单 / Check database-only orders
        for (order_id, (db_index, db_order)) in db_orders.iter() {
            if !chain_order_map.contains_key(order_id) {
                db_only_orders.push(OrderSummary {
                    order_id: *order_id,
                    index: *db_index,
                    user: db_order.user.clone(),
                });
            }
        }

        let total_match = chain_header.total == db_header.total;
        let mismatching_orders = order_differences.len() as u32;

        Ok(OrderBookComparison {
            direction: direction.to_string(),
            chain_total: chain_header.total,
            db_total: db_header.total,
            total_match,
            order_differences,
            chain_only_orders,
            db_only_orders,
            matching_orders,
            mismatching_orders,
        })
    }

    /// 对比订单字段 / Compare order fields
    fn compare_order_fields(
        &self,
        chain_order: &MarginOrder,
        db_order: &MarginOrder,
        chain_index: u16,
        db_index: u16,
    ) -> Vec<FieldDifference> {
        let mut differences = Vec::new();

        // 对比 index
        if chain_index != db_index {
            differences.push(FieldDifference {
                field: "index".to_string(),
                chain_value: chain_index.to_string(),
                db_value: db_index.to_string(),
            });
        }

        // 对比 order_type
        if chain_order.order_type != db_order.order_type {
            differences.push(FieldDifference {
                field: "order_type".to_string(),
                chain_value: chain_order.order_type.to_string(),
                db_value: db_order.order_type.to_string(),
            });
        }

        // 对比 order_id (通常应该相同，但为了完整性还是检查)
        if chain_order.order_id != db_order.order_id {
            differences.push(FieldDifference {
                field: "order_id".to_string(),
                chain_value: chain_order.order_id.to_string(),
                db_value: db_order.order_id.to_string(),
            });
        }

        // 对比 lock_lp_start_price
        if chain_order.lock_lp_start_price != db_order.lock_lp_start_price {
            differences.push(FieldDifference {
                field: "lock_lp_start_price".to_string(),
                chain_value: chain_order.lock_lp_start_price.to_string(),
                db_value: db_order.lock_lp_start_price.to_string(),
            });
        }

        // 对比 lock_lp_end_price
        if chain_order.lock_lp_end_price != db_order.lock_lp_end_price {
            differences.push(FieldDifference {
                field: "lock_lp_end_price".to_string(),
                chain_value: chain_order.lock_lp_end_price.to_string(),
                db_value: db_order.lock_lp_end_price.to_string(),
            });
        }

        // 对比 lock_lp_sol_amount
        if chain_order.lock_lp_sol_amount != db_order.lock_lp_sol_amount {
            differences.push(FieldDifference {
                field: "lock_lp_sol_amount".to_string(),
                chain_value: chain_order.lock_lp_sol_amount.to_string(),
                db_value: db_order.lock_lp_sol_amount.to_string(),
            });
        }

        // 对比 lock_lp_token_amount
        if chain_order.lock_lp_token_amount != db_order.lock_lp_token_amount {
            differences.push(FieldDifference {
                field: "lock_lp_token_amount".to_string(),
                chain_value: chain_order.lock_lp_token_amount.to_string(),
                db_value: db_order.lock_lp_token_amount.to_string(),
            });
        }

        differences
    }
}