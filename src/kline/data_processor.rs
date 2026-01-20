// K线数据处理器 / K-line data processor
use crate::kline::types::{EventHistoryResponse, EventUpdateMessage, KlineHistoryResponse, KlineRealtimeData};
use crate::price::SolPriceService;
use crate::solana::PinpetEvent;
use anyhow::Result;
use std::sync::Arc;

/// 价格精度常量 (23位小数) / Precision constant for u128 to f64 conversion (23 decimal places)
pub const PRICE_PRECISION: u128 = 10_u128.pow(23);

/// K线数据处理器 / K-line data processor
pub struct KlineDataProcessor {
    event_storage: Arc<crate::db::EventStorage>,
    price_service: Arc<SolPriceService>,
}

impl KlineDataProcessor {
    /// 创建新的K线数据处理器 / Create new K-line data processor
    pub fn new(event_storage: Arc<crate::db::EventStorage>, price_service: Arc<SolPriceService>) -> Self {
        Self { event_storage, price_service }
    }

    /// 将u128价格转换为f64 / Convert u128 price to f64 with precision handling
    /// 价格存储为u128类型，精度为10^23，需要除以PRICE_PRECISION转换为f64
    /// Price is stored as u128 with 23 decimal places precision, needs to be divided by PRICE_PRECISION to convert to f64
    pub fn convert_price_to_f64(price_u128: u128) -> f64 {
        // 将u128转换为f64并除以精度常量 / Convert u128 to f64 and divide by precision constant
        // Since u128 has 23 decimal places, we divide by 10^23
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
            PinpetEvent::Liquidate(e) => e.mint_account.clone(),
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
            PinpetEvent::Liquidate(_) => "Liquidate".to_string(),
        }
    }

    /// 从事件获取时间戳 / Get timestamp from event
    pub fn get_event_timestamp(event: &PinpetEvent) -> chrono::DateTime<chrono::Utc> {
        match event {
            PinpetEvent::TokenCreated(e) => e.timestamp,
            PinpetEvent::BuySell(e) => e.timestamp,
            PinpetEvent::LongShort(e) => e.timestamp,
            PinpetEvent::FullClose(e) => e.timestamp,
            PinpetEvent::PartialClose(e) => e.timestamp,
            PinpetEvent::MilestoneDiscount(e) => e.timestamp,
            PinpetEvent::Liquidate(e) => e.timestamp,
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

    /// 填充事件的USD价格 / Fill USD price for event
    /// 将SOL价格转换为USD价格(整数字符串,精度10^23) / Convert SOL price to USD price (integer string, precision 10^23)
    fn fill_usd_price(&self, event: &mut PinpetEvent) {
        // 获取 SOL 价格 (get_price_sync 返回 f64, 有默认值 140.0)
        // Get SOL price (get_price_sync returns f64, with default value 140.0)
        let sol_price_usd = self.price_service.get_price_sync();

        // 提取 latest_price 并转换为 USD (整数格式,精度 10^23)
        if let Some(price_in_sol) = Self::extract_price_from_event(event) {
            // price_in_sol 是 f64, sol_price_usd 是 f64
            // 为了得到精度 10^23 的整数价格:
            // 1. 计算浮点数价格: price_in_sol * sol_price_usd
            // 2. 乘以 10^23 得到整数
            let price_usd_float = price_in_sol * sol_price_usd;
            let precision = 1e23; // 10^23
            let price_usd_integer = (price_usd_float * precision) as u128;

            // 转换为字符串
            let price_usd_str = price_usd_integer.to_string();

            // 填充对应事件的 latest_price_usd 字段
            match event {
                PinpetEvent::TokenCreated(e) => e.latest_price_usd = Some(price_usd_str),
                PinpetEvent::BuySell(e) => e.latest_price_usd = Some(price_usd_str),
                PinpetEvent::LongShort(e) => e.latest_price_usd = Some(price_usd_str),
                PinpetEvent::FullClose(e) => e.latest_price_usd = Some(price_usd_str),
                PinpetEvent::PartialClose(e) => e.latest_price_usd = Some(price_usd_str),
                _ => {}
            }
        }
    }

    /// 获取历史交易事件 / Get historical events
    pub async fn get_event_history(
        &self,
        symbol: &str,
        limit: usize,
    ) -> Result<EventHistoryResponse> {
        // 从数据库查询事件（降序，最新的在前）/ Query events from database (descending, newest first)
        let mut events = self
            .event_storage
            .query_by_mint(symbol, Some(limit), false)  // false = 降序 / descending
            .await?;

        // 填充每个事件的USD价格 / Fill USD price for each event
        for event in &mut events {
            self.fill_usd_price(event);
        }

        let data: Vec<EventUpdateMessage> = events
            .into_iter()
            .map(|event| {
                // 从事件本身获取时间戳并转换为毫秒级数字 / Get timestamp from event and convert to milliseconds
                let event_timestamp_ms = Self::get_event_timestamp(&event).timestamp_millis() as u64;
                EventUpdateMessage {
                    symbol: symbol.to_string(),
                    event_type: Self::get_event_type_name(&event),
                    event_data: event,
                    timestamp: event_timestamp_ms,
                }
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
    #[allow(dead_code)]
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
