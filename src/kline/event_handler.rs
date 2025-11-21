// K线事件处理器 - 包装现有事件处理器并添加K线推送功能
// K-line event handler - Wraps existing event handler and adds K-line push functionality

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
}

impl KlineEventHandler {
    /// 创建新的K线事件处理器 / Create new K-line event handler
    pub fn new(
        inner: Arc<dyn EventHandler>,
        kline_service: Arc<KlineSocketService>,
    ) -> Self {
        Self {
            inner,
            kline_service,
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
        if let Some(price) = KlineDataProcessor::extract_price_from_event(&event) {
            let mint = KlineDataProcessor::get_mint_from_event(&event);
            let timestamp = Utc::now().timestamp() as u64;

            // 为每个支持的时间间隔生成K线数据 / Generate K-line data for each supported interval
            let intervals = ["s1", "s30", "m5"];
            for interval in intervals {
                // 生成K线数据 (简化版,直接使用KlineRealtimeData)
                // Generate K-line data (simplified, use KlineRealtimeData directly)
                let kline_data = crate::kline::types::KlineRealtimeData {
                    time: timestamp,
                    open: price,
                    high: price,
                    low: price,
                    close: price,
                    volume: 0.0,
                    is_final: false,
                    update_type: "realtime".to_string(),
                    update_count: 1,
                };

                // 广播K线更新 / Broadcast K-line update
                info!(
                    "广播K线更新 / Broadcasting K-line update: mint={}, interval={}, price={}",
                    mint, interval, price
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
