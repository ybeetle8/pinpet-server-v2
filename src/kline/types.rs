// K线数据类型定义 / K-line data type definitions
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 实时K线数据结构 / Real-time K-line data structure
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct KlineRealtimeData {
    pub time: u64,           // Unix时间戳(秒) / Unix timestamp (seconds)
    pub open: f64,           // 开盘价 / Open price
    pub high: f64,           // 最高价 / High price
    pub low: f64,            // 最低价 / Low price
    pub close: f64,          // 收盘价(当前价格) / Close price (current price)
    pub volume: f64,         // 成交量 / Volume
    pub is_final: bool,      // 是否为最终K线 / Is final K-line
    pub update_type: String, // "realtime" | "final" 更新类型 / Update type
    pub update_count: u32,   // 更新次数 / Update count
}

/// 实时K线推送消息 / Real-time K-line push message
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct KlineUpdateMessage {
    pub symbol: String,                  // mint_account mint地址 / mint address
    pub interval: String,                // s1, s30, m5 时间间隔 / time interval
    pub subscription_id: Option<String>, // 客户端订阅ID / Client subscription ID
    pub data: KlineRealtimeData,         // K线数据 / K-line data
    pub timestamp: u64,                  // 推送时间戳(毫秒) / Push timestamp (ms)
}

/// 历史K线数据响应 / Historical K-line data response
#[derive(Debug, Serialize, ToSchema)]
pub struct KlineHistoryResponse {
    pub symbol: String,              // mint地址 / mint address
    pub interval: String,            // 时间间隔 / time interval
    pub data: Vec<KlineRealtimeData>, // K线数据列表 / K-line data list
    pub has_more: bool,              // 是否有更多数据 / Has more data
    pub total_count: usize,          // 总数量 / Total count
}

/// 交易事件推送消息 / Event update message
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EventUpdateMessage {
    pub symbol: String,                       // mint_account mint地址 / mint address
    pub event_type: String,                   // 事件类型名称 / Event type name
    pub event_data: crate::solana::PinpetEvent, // 完整事件数据 / Complete event data
    pub timestamp: u64,                       // 推送时间戳(毫秒) / Push timestamp (ms)
}

/// 历史交易事件响应 / Historical event response
#[derive(Debug, Serialize, ToSchema)]
pub struct EventHistoryResponse {
    pub symbol: String,                  // mint地址 / mint address
    pub data: Vec<EventUpdateMessage>,   // 事件数据列表 / Event data list
    pub has_more: bool,                  // 是否有更多数据 / Has more data
    pub total_count: usize,              // 总数量 / Total count
}

/// Socket.IO订阅请求 / Socket.IO subscribe request
#[derive(Debug, Deserialize)]
pub struct SubscribeRequest {
    pub symbol: String,                  // mint_account mint地址 / mint address
    pub interval: String,                // s1, s30, m5 时间间隔 / time interval
    pub subscription_id: Option<String>, // 客户端订阅ID / Client subscription ID
}

/// Socket.IO取消订阅请求 / Socket.IO unsubscribe request
#[derive(Debug, Deserialize)]
pub struct UnsubscribeRequest {
    pub symbol: String,                  // mint地址 / mint address
    pub interval: String,                // 时间间隔 / time interval
    pub subscription_id: Option<String>, // 客户端订阅ID / Client subscription ID
}

/// Socket.IO历史数据请求 / Socket.IO history request
#[derive(Debug, Deserialize)]
pub struct HistoryRequest {
    pub symbol: String,        // mint地址 / mint address
    pub interval: String,      // 时间间隔 / time interval
    pub limit: Option<usize>,  // 返回数量限制 / Return limit
    pub from: Option<u64>,     // 开始时间戳(秒) / Start timestamp (seconds)
}

/// K线配置 / K-line configuration
#[derive(Debug, Clone)]
pub struct KlineConfig {
    pub connection_timeout_secs: u64,        // 连接超时时间(秒) / Connection timeout (seconds)
    pub max_subscriptions_per_client: usize, // 每客户端最大订阅数 / Max subscriptions per client
    pub history_data_limit: usize,           // 历史数据默认条数 / History data default limit
    pub ping_interval_secs: u64,             // 心跳间隔(秒) / Ping interval (seconds)
    pub ping_timeout_secs: u64,              // 心跳超时(秒) / Ping timeout (seconds)
}

impl Default for KlineConfig {
    fn default() -> Self {
        Self {
            connection_timeout_secs: 60,
            max_subscriptions_per_client: 100,
            history_data_limit: 100,
            ping_interval_secs: 25,
            ping_timeout_secs: 60,
        }
    }
}
