// K线数据处理器 / K-line data processor
use crate::kline::types::{EventHistoryResponse, EventUpdateMessage, KlineHistoryResponse, KlineRealtimeData};
use crate::solana::PinpetEvent;
use anyhow::Result;
use chrono::Utc;
use std::sync::Arc;

/// K线数据处理器 / K-line data processor
pub struct KlineDataProcessor {
    event_storage: Arc<crate::db::EventStorage>,
}

impl KlineDataProcessor {
    /// 创建新的K线数据处理器 / Create new K-line data processor
    pub fn new(event_storage: Arc<crate::db::EventStorage>) -> Self {
        Self { event_storage }
    }

    /// 从事件提取价格数据 / Extract price from event
    pub fn extract_price_from_event(event: &PinpetEvent) -> Option<f64> {
        match event {
            PinpetEvent::TokenCreated(e) => {
                // Convert u128 to f64 with precision handling
                // 将u128价格转换为f64, 保留精度 / Convert u128 price to f64 with precision
                Some(e.latest_price as f64)
            }
            PinpetEvent::BuySell(e) => Some(e.latest_price as f64),
            PinpetEvent::LongShort(e) => Some(e.latest_price as f64),
            PinpetEvent::FullClose(e) => Some(e.latest_price as f64),
            PinpetEvent::PartialClose(e) => Some(e.latest_price as f64),
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
    /// Note: 新项目暂时返回空数据,因为还没有实现K线聚合存储 / Returns empty for now as K-line aggregation storage is not implemented yet
    pub async fn get_kline_history(
        &self,
        _symbol: &str,
        _interval: &str,
        _limit: usize,
    ) -> Result<KlineHistoryResponse> {
        // TODO: 实现真正的K线历史数据查询 / TODO: Implement real K-line history query
        // 现在返回空数据 / Return empty data for now
        Ok(KlineHistoryResponse {
            symbol: _symbol.to_string(),
            interval: _interval.to_string(),
            data: Vec::new(),
            has_more: false,
            total_count: 0,
        })
    }

    /// 获取历史交易事件 / Get historical events
    pub async fn get_event_history(
        &self,
        symbol: &str,
        limit: usize,
    ) -> Result<EventHistoryResponse> {
        // 从数据库查询事件 / Query events from database
        let events = self
            .event_storage
            .query_by_mint(symbol, Some(limit))
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
