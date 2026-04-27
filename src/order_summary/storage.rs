// 订单汇总统计存储模块 / Order Summary Statistics Storage Module

use super::types::OrderSummaryData;
use anyhow::Result;
use chrono::Utc;
use rocksdb::DB;
use std::sync::Arc;
use tracing::warn;

/// 订单汇总存储 / Order Summary Storage
pub struct OrderSummaryStorage {
    db: Arc<DB>,
}

impl OrderSummaryStorage {
    /// 创建新的订单汇总存储 / Create new order summary storage
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    /// 生成 key / Generate key
    fn key(mint: &str, direction: &str) -> String {
        format!("order_summary:{}:{}", mint, direction)
    }

    /// 读取汇总 / Read summary (不存在则返回默认零值 / returns default zero if not exists)
    pub fn get(&self, mint: &str, direction: &str) -> Result<OrderSummaryData> {
        let key = Self::key(mint, direction);
        match self.db.get(key.as_bytes())? {
            Some(value) => {
                let data: OrderSummaryData = serde_json::from_slice(&value)?;
                Ok(data)
            }
            None => Ok(OrderSummaryData::default()),
        }
    }

    /// 写入汇总 / Write summary
    pub fn put(&self, mint: &str, direction: &str, data: &OrderSummaryData) -> Result<()> {
        let key = Self::key(mint, direction);
        let value = serde_json::to_vec(data)?;
        self.db.put(key.as_bytes(), value)?;
        Ok(())
    }

    /// 增加 (新订单插入时) / Add (on new order insert)
    pub fn add_order(
        &self,
        mint: &str,
        direction: &str,
        margin_sol: u64,
        lock_lp_token: u64,
        borrow: u64,
        position_asset: u64,
    ) -> Result<()> {
        let mut data = self.get(mint, direction)?;
        data.total_margin_sol = data.total_margin_sol.saturating_add(margin_sol);
        data.total_lock_lp_token = data.total_lock_lp_token.saturating_add(lock_lp_token);
        data.total_borrow = data.total_borrow.saturating_add(borrow);
        data.total_position_asset = data.total_position_asset.saturating_add(position_asset);
        data.last_update = Utc::now().timestamp();
        self.put(mint, direction, &data)
    }

    /// 减去 (订单删除/缩小时) / Subtract (on order remove/shrink)
    pub fn sub_order(
        &self,
        mint: &str,
        direction: &str,
        margin_sol: u64,
        lock_lp_token: u64,
        borrow: u64,
        position_asset: u64,
    ) -> Result<()> {
        let mut data = self.get(mint, direction)?;
        let old_margin = data.total_margin_sol;
        let old_token = data.total_lock_lp_token;
        let old_borrow = data.total_borrow;
        let old_position = data.total_position_asset;

        data.total_margin_sol = data.total_margin_sol.saturating_sub(margin_sol);
        data.total_lock_lp_token = data.total_lock_lp_token.saturating_sub(lock_lp_token);
        data.total_borrow = data.total_borrow.saturating_sub(borrow);
        data.total_position_asset = data.total_position_asset.saturating_sub(position_asset);
        data.last_update = Utc::now().timestamp();

        // 如果减到0以下,记录警告日志,提示可能需要重建
        // If subtracted below 0, log warning suggesting rebuild may be needed
        if margin_sol > old_margin || lock_lp_token > old_token || borrow > old_borrow || position_asset > old_position {
            warn!(
                "⚠️ 订单汇总减法下溢 / Order summary subtraction underflow: mint={}, direction={}, \
                 margin_sol: {}->{}, lock_lp_token: {}->{}, borrow: {}->{}, position_asset: {}->{}. \
                 建议重建 / Suggest rebuild.",
                mint, direction,
                old_margin, data.total_margin_sol,
                old_token, data.total_lock_lp_token,
                old_borrow, data.total_borrow,
                old_position, data.total_position_asset,
            );
        }

        self.put(mint, direction, &data)
    }

    /// 从 OrderBook 遍历重建 / Rebuild from OrderBook traversal
    pub fn rebuild_from_orderbook(
        &self,
        mint: &str,
        direction: &str,
        orderbook_storage: &crate::db::OrderBookStorage,
    ) -> Result<OrderSummaryData> {
        let manager = orderbook_storage
            .get_or_create_manager(mint.to_string(), direction.to_string())?;

        let mut data = OrderSummaryData::default();

        manager.traverse(
            u16::MAX,
            0,
            |_index, order| {
                data.total_margin_sol = data.total_margin_sol.saturating_add(order.margin_sol_amount);
                data.total_lock_lp_token = data.total_lock_lp_token.saturating_add(order.lock_lp_token_amount);
                data.total_borrow = data.total_borrow.saturating_add(order.borrow_amount);
                data.total_position_asset = data.total_position_asset.saturating_add(order.position_asset_amount);
                Ok(true)
            },
        )?;

        data.last_update = Utc::now().timestamp();
        self.put(mint, direction, &data)?;
        Ok(data)
    }
}
