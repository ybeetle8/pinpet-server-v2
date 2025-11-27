// K线事件处理器 - 包装现有事件处理器并添加K线推送功能
// K-line event handler - Wraps existing event handler and adds K-line push functionality

use crate::db::EventStorage;
use crate::kline::{data_processor::KlineDataProcessor, socket_service::KlineSocketService};
use crate::solana::{EventHandler, PinpetEvent};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// K线事件处理器 - 装饰器模式包装EventHandler
/// K-line event handler - Decorator pattern wrapping EventHandler
pub struct KlineEventHandler {
    inner: Arc<dyn EventHandler>,           // 内部事件处理器 / Inner event handler
    kline_service: Arc<KlineSocketService>, // K线推送服务 / K-line push service
    event_storage: Arc<EventStorage>,       // 事件存储(用于读取K线数据) / Event storage (for reading K-line data)
}

impl KlineEventHandler {
    /// 创建新的K线事件处理器 / Create new K-line event handler
    pub fn new(
        inner: Arc<dyn EventHandler>,
        kline_service: Arc<KlineSocketService>,
        event_storage: Arc<EventStorage>,
    ) -> Self {
        Self {
            inner,
            kline_service,
            event_storage,
        }
    }

    /// 计算时间桶 / Calculate time bucket for different intervals
    /// 返回对齐后的时间戳 / Returns the aligned timestamp for the time bucket
    fn calculate_time_bucket(timestamp: u64, interval: &str) -> u64 {
        match interval {
            "s1" => timestamp,                    // 1秒间隔-不需要对齐 / 1-second intervals - no alignment needed
            "s30" => (timestamp / 30) * 30,       // 30秒边界对齐 / align to 30-second boundary
            "m5" => (timestamp / 300) * 300,      // 5分钟边界对齐 / align to 5-minute boundary
            _ => timestamp,                        // 默认1秒 / default to 1-second
        }
    }
}

#[async_trait]
impl EventHandler for KlineEventHandler {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn handle_event(&self, event: PinpetEvent) -> Result<()> {
        debug!("K线事件处理器收到事件 / K-line event handler received event: {:?}", event);

        // 1. 首先调用内部事件处理器 (保存到数据库等)
        // 1. First call inner event handler (save to database, etc.)
        if let Err(e) = self.inner.handle_event(event.clone()).await {
            warn!(
                "内部事件处理器失败 / Inner event handler failed: {}",
                e
            );
            // 即使内部处理失败,也继续进行K线推送 / Continue with K-line push even if inner handler fails
        }

        // 2. 广播交易事件 (所有事件都推送)
        // 2. Broadcast trading event (all events are pushed)
        info!("广播交易事件 / Broadcasting trading event");
        if let Err(e) = self.kline_service.broadcast_event_update(&event).await {
            warn!("广播交易事件失败 / Failed to broadcast event update: {}", e);
        }

        // 3. 如果事件包含价格数据,生成并广播K线更新
        // 3. If event contains price data, generate and broadcast K-line update
        if let Some(current_price) = KlineDataProcessor::extract_price_from_event(&event) {
            let mint = KlineDataProcessor::get_mint_from_event(&event);
            let timestamp = Utc::now().timestamp() as u64;

            // 为每个支持的时间间隔生成K线数据 / Generate K-line data for each supported interval
            let intervals = ["s1", "s30", "m5"];
            for interval in intervals {
                // 计算对齐后的时间桶 / Calculate aligned time bucket
                let aligned_time = Self::calculate_time_bucket(timestamp, interval);

                // 从数据库读取当前时间桶的K线数据 / Read K-line data from database for current time bucket
                let kline_data = match self.event_storage.get_kline_data(&mint, interval, aligned_time).await {
                    Ok(Some(existing_kline)) => {
                        // 数据库中已有K线数据,更新high/low,close为当前价格 / K-line exists in DB, update high/low, close to current price
                        crate::kline::types::KlineRealtimeData {
                            time: aligned_time,
                            open: existing_kline.open,              // ✅ 保持原有的开盘价 / Keep original open price
                            high: existing_kline.high.max(current_price),  // ✅ 更新最高价 / Update high price
                            low: existing_kline.low.min(current_price),    // ✅ 更新最低价 / Update low price
                            close: current_price,                   // ✅ 当前价格作为收盘价 / Current price as close
                            volume: existing_kline.volume,
                            is_final: false,
                            update_type: "realtime".to_string(),
                            update_count: existing_kline.update_count + 1,
                        }
                    }
                    Ok(None) => {
                        // 数据库中没有该时间桶的K线,创建新K线(open/high/low/close都是当前价格) / No K-line in DB, create new one
                        crate::kline::types::KlineRealtimeData {
                            time: aligned_time,
                            open: current_price,
                            high: current_price,
                            low: current_price,
                            close: current_price,
                            volume: 0.0,
                            is_final: false,
                            update_type: "realtime".to_string(),
                            update_count: 1,
                        }
                    }
                    Err(e) => {
                        warn!("从数据库读取K线数据失败 / Failed to read K-line from DB: {}, 使用当前价格 / using current price", e);
                        // 出错时回退到使用当前价格 / Fallback to current price on error
                        crate::kline::types::KlineRealtimeData {
                            time: aligned_time,
                            open: current_price,
                            high: current_price,
                            low: current_price,
                            close: current_price,
                            volume: 0.0,
                            is_final: false,
                            update_type: "realtime".to_string(),
                            update_count: 1,
                        }
                    }
                };

                // 广播K线更新 / Broadcast K-line update
                info!(
                    "广播K线更新 / Broadcasting K-line update: mint={}, interval={}, time={} (原始={}), open={}, high={}, low={}, close={}",
                    mint, interval, aligned_time, timestamp, kline_data.open, kline_data.high, kline_data.low, kline_data.close
                );

                if let Err(e) = self
                    .kline_service
                    .broadcast_kline_update(&mint, interval, &kline_data)
                    .await
                {
                    warn!(
                        "广播K线更新失败 / Failed to broadcast K-line update for {}:{}: {}",
                        mint, interval, e
                    );
                }
            }
        } else {
            debug!(
                "事件不包含价格数据,跳过K线推送 / Event does not contain price data, skipping K-line push"
            );
        }

        Ok(())
    }
}
