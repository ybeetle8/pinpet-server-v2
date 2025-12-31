// K线事件处理器 - 包装现有事件处理器并添加K线推送功能
// K-line event handler - Wraps existing event handler and adds K-line push functionality

use crate::db::EventStorage;
use crate::kline::{cache::KlineCache, data_processor::KlineDataProcessor, socket_service::KlineSocketService};
use crate::price::SolPriceService;
use crate::solana::{EventHandler, PinpetEvent};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

/// K线事件处理器 - 装饰器模式包装EventHandler
/// K-line event handler - Decorator pattern wrapping EventHandler
pub struct KlineEventHandler {
    inner: Arc<dyn EventHandler>,           // 内部事件处理器 / Inner event handler
    kline_service: Arc<KlineSocketService>, // K线推送服务 / K-line push service
    kline_cache: Arc<KlineCache>,           // K线缓存 / K-line cache
    event_storage: Arc<EventStorage>,       // 事件存储(用于降级读取) / Event storage (for fallback reads)
    price_service: Arc<SolPriceService>,    // SOL价格服务(用于SOL->USD转换) / SOL price service (for SOL->USD conversion)
}

impl KlineEventHandler {
    /// 创建新的K线事件处理器 / Create new K-line event handler
    pub fn new(
        inner: Arc<dyn EventHandler>,
        kline_service: Arc<KlineSocketService>,
        kline_cache: Arc<KlineCache>,
        event_storage: Arc<EventStorage>,
        price_service: Arc<SolPriceService>,
    ) -> Self {
        Self {
            inner,
            kline_service,
            kline_cache,
            event_storage,
            price_service,
        }
    }

    /// 填充事件的 USD 价格 / Fill USD price for the event
    async fn fill_usd_price(&self, event: &mut PinpetEvent) -> Result<()> {
        // 获取 SOL 价格
        let sol_price_usd = match self.price_service.get_price().await {
            Some(sol_price) => sol_price.price,
            None => {
                return Err(anyhow::anyhow!("SOL价格未就绪 / SOL price not ready"));
            }
        };

        // 提取 latest_price 并转换为 USD
        if let Some(price_in_sol) = KlineDataProcessor::extract_price_from_event(event) {
            let price_usd = price_in_sol * sol_price_usd;

            // 填充对应事件的 latest_price_usd 字段
            match event {
                PinpetEvent::TokenCreated(e) => e.latest_price_usd = Some(price_usd),
                PinpetEvent::BuySell(e) => e.latest_price_usd = Some(price_usd),
                PinpetEvent::LongShort(e) => e.latest_price_usd = Some(price_usd),
                PinpetEvent::FullClose(e) => e.latest_price_usd = Some(price_usd),
                PinpetEvent::PartialClose(e) => e.latest_price_usd = Some(price_usd),
                _ => {}
            }

            debug!(
                "填充USD价格 / Filled USD price: {} SOL × {} USD/SOL = {} USD",
                price_in_sol, sol_price_usd, price_usd
            );
        }

        Ok(())
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

    /// 提取事件时间戳 / Extract event timestamp
    /// 统一使用事件的区块链时间戳,避免时间桶不一致 / Use event's blockchain timestamp consistently to avoid time bucket mismatch
    fn extract_event_timestamp(event: &PinpetEvent) -> u64 {
        match event {
            PinpetEvent::TokenCreated(e) => e.timestamp.timestamp() as u64,
            PinpetEvent::BuySell(e) => e.timestamp.timestamp() as u64,
            PinpetEvent::LongShort(e) => e.timestamp.timestamp() as u64,
            PinpetEvent::FullClose(e) => e.timestamp.timestamp() as u64,
            PinpetEvent::PartialClose(e) => e.timestamp.timestamp() as u64,
            _ => chrono::Utc::now().timestamp() as u64, // 其他事件使用当前时间 / Use current time for other events
        }
    }
}

#[async_trait]
impl EventHandler for KlineEventHandler {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn handle_event(&self, event: PinpetEvent) -> Result<()> {
        let start_time = Instant::now();
        debug!("K线事件处理器收到事件 / K-line event handler received event: {:?}", event);

        // 🚀 方案E: 异步推送 + 内存缓存 (高性能方案)
        // 🚀 Solution E: Async push + Memory cache (High-performance solution)

        // 1. 立即从缓存获取/计算K线数据并推送 (超快!)
        // 1. Immediately get/calculate K-line data from cache and push (ultra-fast!)
        if let Some(price_in_sol) = KlineDataProcessor::extract_price_from_event(&event) {
            // 获取SOL价格(USD) / Get SOL price (USD)
            let sol_price_usd = match self.price_service.get_price().await {
                Some(sol_price) => sol_price.price,
                None => {
                    warn!("⚠️ SOL价格未就绪,跳过K线推送 / SOL price not ready, skipping K-line push");
                    return Ok(());
                }
            };

            // 将SOL价格转换为USD价格 / Convert SOL price to USD price
            let current_price = price_in_sol * sol_price_usd;

            debug!(
                "价格转换 / Price conversion: {} SOL × {} USD/SOL = {} USD",
                price_in_sol, sol_price_usd, current_price
            );

            let mint = KlineDataProcessor::get_mint_from_event(&event);
            // ✅ 使用事件时间戳,避免时间桶不一致 / Use event timestamp to avoid time bucket mismatch
            let timestamp = Self::extract_event_timestamp(&event);

            // 为每个支持的时间间隔生成K线数据 / Generate K-line data for each supported interval
            let intervals = ["s1", "s30", "m5"];
            for interval in intervals {
                // 计算对齐后的时间桶 / Calculate aligned time bucket
                let aligned_time = Self::calculate_time_bucket(timestamp, interval);

                // 🔥 从缓存获取或创建K线 (1-2ms, 无DB读取!) / Get or create K-line from cache (1-2ms, no DB read!)
                let kline_storage_data = self.kline_cache
                    .get_or_create(&mint, interval, aligned_time, current_price, timestamp)
                    .await;

                // 转换为实时K线数据格式 / Convert to realtime K-line data format
                let kline_data = crate::kline::types::KlineRealtimeData {
                    time: kline_storage_data.time,
                    open: kline_storage_data.open,
                    high: kline_storage_data.high,
                    low: kline_storage_data.low,
                    close: kline_storage_data.close,
                    volume: kline_storage_data.volume,
                    is_final: kline_storage_data.is_final,
                    update_type: "realtime".to_string(),
                    update_count: kline_storage_data.update_count,
                };

                // 🚀 立即推送K线更新 (10-30ms) / Immediately broadcast K-line update (10-30ms)
                let push_start = Instant::now();
                if let Err(e) = self
                    .kline_service
                    .broadcast_kline_update(&mint, interval, &kline_data)
                    .await
                {
                    warn!(
                        "广播K线更新失败 / Failed to broadcast K-line update for {}:{}: {}",
                        mint, interval, e
                    );
                } else {
                    let push_duration = push_start.elapsed().as_millis();
                    info!(
                        "📡 K线推送完成 / K-line push completed: mint={}, interval={}, time={}, OHLC=[{},{},{},{}], count={}, push_time={}ms",
                        mint, interval, aligned_time, kline_data.open, kline_data.high, kline_data.low, kline_data.close, kline_data.update_count, push_duration
                    );
                }
            }
        }

        // 2. 填充USD价格并广播交易事件 (10-20ms) / Fill USD price and broadcast trading event (10-20ms)
        let mut event_with_usd = event.clone();
        if let Err(e) = self.fill_usd_price(&mut event_with_usd).await {
            warn!("填充USD价格失败 / Failed to fill USD price: {}", e);
        }

        if let Err(e) = self.kline_service.broadcast_event_update(&event_with_usd).await {
            warn!("广播交易事件失败 / Failed to broadcast event update: {}", e);
        }

        // 3. 后台异步存储 (不等待,不阻塞) / Background async storage (no wait, no blocking)
        let inner = Arc::clone(&self.inner);
        let event_clone = event.clone();
        tokio::spawn(async move {
            if let Err(e) = inner.handle_event(event_clone).await {
                warn!("后台存储失败 / Background storage failed: {}", e);
            }
        });

        let total_duration = start_time.elapsed().as_millis();
        debug!(
            "⚡ 事件处理完成 / Event processing completed: total_time={}ms",
            total_duration
        );

        Ok(())
    }
}
