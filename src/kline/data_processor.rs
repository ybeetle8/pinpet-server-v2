// K线数据处理器 / K-line data processor
use crate::kline::types::{EventHistoryResponse, EventUpdateMessage, KlineHistoryResponse, KlineRealtimeData};
use crate::solana::PinpetEvent;
use anyhow::Result;
use chrono::Utc;
use std::sync::Arc;

/// 价格精度常量 (26位小数) / Precision constant for u128 to f64 conversion (26 decimal places)
pub const PRICE_PRECISION: u128 = 10_u128.pow(26);

/// K线数据处理器 / K-line data processor
pub struct KlineDataProcessor {
    event_storage: Arc<crate::db::EventStorage>,
}

impl KlineDataProcessor {
    /// 创建新的K线数据处理器 / Create new K-line data processor
    pub fn new(event_storage: Arc<crate::db::EventStorage>) -> Self {
        Self { event_storage }
    }

    /// 将u128价格转换为f64 / Convert u128 price to f64 with precision handling
    /// 价格存储为u128类型，精度为10^26，需要除以PRICE_PRECISION转换为f64
    /// Price is stored as u128 with 26 decimal places precision, needs to be divided by PRICE_PRECISION to convert to f64
    pub fn convert_price_to_f64(price_u128: u128) -> f64 {
        // 将u128转换为f64并除以精度常量 / Convert u128 to f64 and divide by precision constant
        // Since u128 has 26 decimal places, we divide by 10^26
        // But f64 has limited precision, so we might lose some accuracy
        let price_f64 = price_u128 as f64 / PRICE_PRECISION as f64;

        // 四舍五入到合理精度(12位小数)以避免浮点噪声 / Round to reasonable precision (12 decimal places) to avoid floating point noise
        (price_f64 * 1e12).round() / 1e12
    }

    /// 从事件提取价格数据 / Extract price from event
    pub fn extract_price_from_event(event: &PinpetEvent) -> Option<f64> {
        match event {
            PinpetEvent::TokenCreated(e) => {
                // Convert u128 to f64 with precision handling
                // 将u128价格转换为f64, 保留精度 / Convert u128 price to f64 with precision
                Some(Self::convert_price_to_f64(e.latest_price))
            }
            PinpetEvent::BuySell(e) => Some(Self::convert_price_to_f64(e.latest_price)),
            PinpetEvent::LongShort(e) => Some(Self::convert_price_to_f64(e.latest_price)),
            PinpetEvent::FullClose(e) => Some(Self::convert_price_to_f64(e.latest_price)),
            PinpetEvent::PartialClose(e) => Some(Self::convert_price_to_f64(e.latest_price)),
            _ => None,
        }
    }

    /// 从事件获取mint地址 / Get mint address from event
    pub fn get_mint_from_event(event: &PinpetEvent) -> String {
        match event {
            PinpetEvent::TokenCreated(e) => e.mint_account.clone(),
            PinpetEvent::BuySell(e) => e.mint_account.clone(),
            PinpetEvent::LongShort(e) => e.mint_account.clone(),
            PinpetEvent::FullClose(e) => e.mint_account.clone(),
            PinpetEvent::PartialClose(e) => e.mint_account.clone(),
            PinpetEvent::MilestoneDiscount(e) => e.mint_account.clone(),
        }
    }

    /// 获取事件类型名称 / Get event type name
    pub fn get_event_type_name(event: &PinpetEvent) -> String {
        match event {
            PinpetEvent::TokenCreated(_) => "TokenCreated".to_string(),
            PinpetEvent::BuySell(_) => "BuySell".to_string(),
            PinpetEvent::LongShort(_) => "LongShort".to_string(),
            PinpetEvent::FullClose(_) => "FullClose".to_string(),
            PinpetEvent::PartialClose(_) => "PartialClose".to_string(),
            PinpetEvent::MilestoneDiscount(_) => "MilestoneDiscount".to_string(),
        }
    }

    /// 获取历史K线数据 / Get historical K-line data
    /// 从数据库查询已聚合的K线数据 / Query aggregated K-line data from database
    pub async fn get_kline_history(
        &self,
        symbol: &str,
        interval: &str,
        limit: usize,
    ) -> Result<KlineHistoryResponse> {
        use crate::kline::types::KlineQuery;

        // 构建查询参数 / Build query parameters
        let query = KlineQuery {
            mint_account: symbol.to_string(),
            interval: interval.to_string(),
            page: Some(1),
            limit: Some(limit),
            order_by: Some("time_desc".to_string()), // 时间倒序（最新的在前）/ Time descending (newest first)
        };

        // 查询K线数据 / Query K-line data
        let response = self.event_storage.query_kline_data(query).await?;

        // 转换KlineData为KlineRealtimeData / Convert KlineData to KlineRealtimeData
        let data: Vec<KlineRealtimeData> = response
            .klines
            .into_iter()
            .map(|kline| KlineRealtimeData {
                time: kline.time,
                open: kline.open,
                high: kline.high,
                low: kline.low,
                close: kline.close,
                volume: kline.volume,
                is_final: kline.is_final,
                update_type: if kline.is_final {
                    "final".to_string()
                } else {
                    "realtime".to_string()
                },
                update_count: kline.update_count,
            })
            .collect();

        Ok(KlineHistoryResponse {
            symbol: symbol.to_string(),
            interval: interval.to_string(),
            data,
            has_more: response.has_next,
            total_count: response.total,
        })
    }

    /// 获取历史交易事件 / Get historical events
    pub async fn get_event_history(
        &self,
        symbol: &str,
        limit: usize,
    ) -> Result<EventHistoryResponse> {
        // 从数据库查询事件（降序，最新的在前）/ Query events from database (descending, newest first)
        let events = self
            .event_storage
            .query_by_mint(symbol, Some(limit), false)  // false = 降序 / descending
            .await?;

        let data: Vec<EventUpdateMessage> = events
            .into_iter()
            .map(|event| EventUpdateMessage {
                symbol: symbol.to_string(),
                event_type: Self::get_event_type_name(&event),
                event_data: event,
                timestamp: Utc::now().timestamp_millis() as u64,
            })
            .collect();

        let total_count = data.len();

        Ok(EventHistoryResponse {
            symbol: symbol.to_string(),
            data,
            has_more: false,
            total_count,
        })
    }

    /// 将价格转换为K线数据 (用于实时推送) / Convert price to K-line data (for real-time push)
    pub fn price_to_kline_data(&self, price: f64, timestamp: u64) -> KlineRealtimeData {
        KlineRealtimeData {
            time: timestamp,
            open: price,
            high: price,
            low: price,
            close: price,
            volume: 0.0, // Volume暂时为0 / Volume is 0 for now
            is_final: false,
            update_type: "realtime".to_string(),
            update_count: 1,
        }
    }
}
