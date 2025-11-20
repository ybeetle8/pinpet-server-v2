use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{OpenApi, ToSchema};

/// API 统一响应格式（用于 Swagger 文档）
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(title = "ApiResponse", description = "API 统一响应格式")]
pub struct ApiResponse<T>
where
    T: ToSchema + Serialize,
{
    /// 响应状态码：200=成功，其他=错误
    #[schema(example = 200)]
    pub code: u32,

    /// 响应消息
    #[schema(example = "success")]
    pub msg: String,

    /// 响应数据，成功时包含具体数据，失败时为 null
    pub data: Option<T>,
}

/// 空数据响应格式（用于 Swagger 文档）
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(
    title = "EmptyResponse",
    description = "空数据响应格式（操作成功但无返回数据）",
    example = json!({
        "code": 200,
        "msg": "success",
        "data": null
    })
)]
pub struct EmptyResponse {
    #[schema(example = 200)]
    pub code: u32,

    #[schema(example = "success")]
    pub msg: String,

    pub data: Option<()>,
}

/// 错误响应格式（用于 Swagger 文档）
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(title = "ErrorApiResponse", description = "错误响应格式")]
pub struct ErrorApiResponse {
    /// 响应状态码：非200表示错误
    pub code: u32,

    /// 错误消息
    pub msg: String,

    /// 错误时数据为空
    pub data: Option<Value>,
}

/// OpenAPI 文档配置
#[derive(OpenApi)]
#[openapi(
    paths(
        // 路由函数列表
        crate::router::health::health,
        crate::router::db::db_put,
        crate::router::db::db_get,
        crate::router::db::db_delete,
        crate::router::db::db_stats,
        crate::router::db::db_event_stats,
        crate::router::db::query_events_by_mint,
        crate::router::db::query_events_by_user,
        crate::router::db::query_events_by_signature,
        // OrderBook 路由
        crate::router::orderbook::get_active_orders_by_mint,
        crate::router::orderbook::get_active_orders_by_user,
        crate::router::orderbook::get_active_order_by_id,
        crate::router::orderbook::get_closed_orders_by_user,
    ),
    components(
        schemas(
            // 响应结构体列表
            crate::router::health::HealthResponse,
            crate::router::db::DbRequest,
            crate::router::db::DbResponse,
            crate::router::db::SortOrder,
            crate::router::db::PaginatedEvents,
            crate::router::db::EventList,
            crate::db::DatabaseStats,
            crate::db::event_storage::IndexCounts,
            crate::solana::events::PinpetEvent,
            crate::solana::events::TokenCreatedEvent,
            crate::solana::events::BuySellEvent,
            crate::solana::events::LongShortEvent,
            crate::solana::events::FullCloseEvent,
            crate::solana::events::PartialCloseEvent,
            crate::solana::events::MilestoneDiscountEvent,
            // OrderBook 结构体
            crate::db::OrderData,
            crate::router::orderbook::OrderListResponse,
            EmptyResponse,
            ErrorApiResponse,
        )
    ),
    tags(
        (name = "system", description = "系统相关接口 / System related APIs"),
        (name = "database", description = "数据库相关接口 / Database related APIs"),
        (name = "events", description = "事件查询接口 / Event query APIs"),
        (name = "OrderBook", description = "OrderBook 订单簿接口 / OrderBook APIs"),
    ),
    info(
        title = "Pinpet Server API",
        version = "0.1.0",
        description = "Pinpet Server API 文档 / Pinpet Server API Documentation"
    )
)]
pub struct ApiDoc;
