// 订单清算模块 / Order liquidation module
use anyhow::{Result, Context};
use rocksdb::WriteBatch;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, error, warn};

use crate::db::{OrderBookStorage, OrderData};
use super::events::{BuySellEvent, LongShortEvent, FullCloseEvent, PartialCloseEvent};

/// 获取清算方向 / Get liquidation direction
/// 返回 "up" 或 "dn"
/// Returns "up" or "dn"
pub fn get_liquidation_direction_for_buysell(event: &BuySellEvent) -> &'static str {
    // is_buy=true 删 up 方向的订单
    // is_buy=true liquidates up direction orders
    if event.is_buy {
        "up"
    } else {
        "dn"
    }
}

/// 获取 LongShort 事件的清算方向 / Get liquidation direction for LongShort event
pub fn get_liquidation_direction_for_longshort(event: &LongShortEvent) -> &'static str {
    // order_type=1 删 up 方向的订单
    // order_type=2 删 dn 方向的订单
    // order_type=1 liquidates up direction orders
    // order_type=2 liquidates dn direction orders
    match event.order_type {
        1 => "up",
        2 => "dn",
        _ => "dn", // 默认 / default
    }
}

/// 获取 FullClose 事件的清算方向 / Get liquidation direction for FullClose event
pub fn get_liquidation_direction_for_fullclose(event: &FullCloseEvent) -> &'static str {
    // is_close_long=true 删 dn 方向的订单
    // is_close_long=false 删 up 方向的订单
    // is_close_long=true liquidates dn direction orders
    // is_close_long=false liquidates up direction orders
    if event.is_close_long {
        "dn"
    } else {
        "up"
    }
}

/// 获取 PartialClose 事件的清算方向 / Get liquidation direction for PartialClose event
pub fn get_liquidation_direction_for_partialclose(event: &PartialCloseEvent) -> &'static str {
    // is_close_long=true 删 dn 方向的订单
    // is_close_long=false 删 up 方向的订单
    // is_close_long=true liquidates dn direction orders
    // is_close_long=false liquidates up direction orders
    if event.is_close_long {
        "dn"
    } else {
        "up"
    }
}

/// 订单排序：按价格排序
/// Order sorting: by price
/// up 方向：lock_lp_start_price 从小到大
/// dn 方向：lock_lp_start_price 从大到小
/// up direction: lock_lp_start_price ascending
/// dn direction: lock_lp_start_price descending
fn sort_orders_by_price(orders: &mut Vec<(String, OrderData)>, direction: &str) {
    if direction == "up" {
        // up 方向：从小到大 / up direction: ascending
        orders.sort_by(|a, b| a.1.lock_lp_start_price.cmp(&b.1.lock_lp_start_price));
    } else {
        // dn 方向：从大到小 / dn direction: descending
        orders.sort_by(|a, b| b.1.lock_lp_start_price.cmp(&a.1.lock_lp_start_price));
    }
}

/// 清算处理器 / Liquidation processor
pub struct LiquidationProcessor {
    orderbook_storage: Arc<OrderBookStorage>,
}

impl LiquidationProcessor {
    /// 创建新的清算处理器 / Create new liquidation processor
    pub fn new(orderbook_storage: Arc<OrderBookStorage>) -> Self {
        Self { orderbook_storage }
    }

    /// 处理清算（事务内完成）/ Process liquidation (within transaction)
    ///
    /// 步骤：
    /// 1. 查询 active_order:{mint}:{dir} 的所有订单
    /// 2. 排序：up 按 lock_lp_start_price 升序，dn 按降序
    /// 3. 验证 liquidate_indices 的有效性
    /// 4. 在一个事务中删除指定的订单
    ///
    /// Steps:
    /// 1. Query all orders from active_order:{mint}:{dir}
    /// 2. Sort: up by lock_lp_start_price ascending, dn descending
    /// 3. Validate liquidate_indices
    /// 4. Delete specified orders in one transaction
    pub async fn process_liquidation(
        &self,
        mint: &str,
        direction: &str,
        liquidate_indices: &[u16],
    ) -> Result<()> {
        if liquidate_indices.is_empty() {
            return Ok(());
        }

        info!(
            "开始清算 / Starting liquidation: mint={}, dir={}, indices={:?}",
            mint, direction, liquidate_indices
        );

        // 1. 查询所有激活订单 / Query all active orders
        let mut orders = self
            .orderbook_storage
            .get_active_orders_by_mint(mint, direction, None)
            .await
            .context("查询激活订单失败 / Failed to query active orders")?;

        // 2. 排序 / Sort
        sort_orders_by_price(&mut orders, direction);

        // 3. 验证索引 / Validate indices
        let max_index = orders.len();
        for &idx in liquidate_indices {
            if idx as usize >= max_index {
                error!(
                    "❌ 清算索引无效 / Invalid liquidation index: idx={}, max={}, mint={}, dir={}",
                    idx, max_index, mint, direction
                );
                return Err(anyhow::anyhow!(
                    "清算索引超出范围 / Liquidation index out of range: idx={}, max={}",
                    idx, max_index
                ));
            }
        }

        // 4. 对 indices 从大到小排序（避免索引错位）/ Sort indices descending (avoid index shift)
        let mut sorted_indices: Vec<u16> = liquidate_indices.to_vec();
        sorted_indices.sort_by(|a, b| b.cmp(a));

        // 5. 获取当前时间戳作为关闭时间 / Get current timestamp as close time
        let close_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;

        // 6. 在一个事务中执行所有删除操作 / Execute all deletions in one transaction
        let db = self.orderbook_storage.get_db();
        let mut batch = WriteBatch::default();

        for &idx in &sorted_indices {
            let (mint_str, mut order) = orders[idx as usize].clone();

            // 设置关闭信息 / Set close information
            order.close_time = Some(close_time);
            order.close_type = 2; // 2=强制平仓 / 2=Force liquidation

            info!(
                "清算订单 / Liquidating order: idx={}, order_id={}, user={}, lock_lp_start_price={}",
                idx, order.order_id, order.user, order.lock_lp_start_price
            );

            // 删除激活订单的所有键 / Delete all keys for active order
            self.delete_active_order_keys(&mut batch, &mint_str, direction, &order);

            // 添加已关闭订单的所有键 / Add all keys for closed order
            self.add_closed_order_keys(&mut batch, &mint_str, &order)?;
        }

        // 7. 原子提交 / Atomic commit
        db.write(batch)
            .context("清算事务提交失败 / Liquidation transaction commit failed")?;

        info!(
            "✅ 清算完成 / Liquidation completed: mint={}, dir={}, count={}",
            mint, direction, sorted_indices.len()
        );

        Ok(())
    }

    /// 删除激活订单的所有键 / Delete all keys for active order
    fn delete_active_order_keys(
        &self,
        batch: &mut WriteBatch,
        mint: &str,
        dir: &str,
        order: &OrderData,
    ) {
        // 主存储 / Primary storage
        let main_key = format!(
            "active_order:{}:{}:{:010}:{:010}",
            mint, dir, order.slot, order.order_id
        );
        batch.delete(main_key.as_bytes());

        // 用户索引 / User index
        let user_idx_key = format!(
            "active_user:{}:{}:{}:{:010}:{:010}",
            order.user, mint, dir, order.slot, order.order_id
        );
        batch.delete(user_idx_key.as_bytes());

        // 订单ID映射 / Order ID mapping
        let id_map_key = format!("active_id:{}:{}:{:010}", mint, dir, order.order_id);
        batch.delete(id_map_key.as_bytes());
    }

    /// 添加已关闭订单的所有键 / Add all keys for closed order
    fn add_closed_order_keys(
        &self,
        batch: &mut WriteBatch,
        mint: &str,
        order: &OrderData,
    ) -> Result<()> {
        let dir = order.direction();
        let close_time = order.close_time.unwrap_or(0);

        // 主存储 / Primary storage
        let main_key = format!(
            "closed_order:{}:{:010}:{}:{}:{:010}",
            order.user, close_time, mint, dir, order.order_id
        );
        batch.put(main_key.as_bytes(), &order.to_bytes()?);

        Ok(())
    }
}
