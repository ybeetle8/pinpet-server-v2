// OrderBook API 路由 / OrderBook API routes
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};

use crate::db::{OrderBookStorage, OrderData};
use crate::util::result::CommonResult as ApiResponse;

/// 创建 OrderBook 路由 / Create OrderBook routes
pub fn routes() -> Router<OrderBookState> {
    Router::new()
        .route(
            "/api/orderbook/active/:mint/:direction",
            get(get_active_orders_by_mint),
        )
        .route(
            "/api/orderbook/active/user/:user",
            get(get_active_orders_by_user),
        )
        .route(
            "/api/orderbook/active/order/:mint/:direction/:order_id",
            get(get_active_order_by_id),
        )
        .route(
            "/api/orderbook/closed/user/:user",
            get(get_closed_orders_by_user),
        )
}

/// OrderBook 路由状态 / OrderBook route state
#[derive(Clone)]
pub struct OrderBookState {
    pub orderbook_storage: Arc<OrderBookStorage>,
}

/// 查询激活订单的请求参数（按 mint + direction）/ Query parameters for active orders (by mint + direction)
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ActiveOrdersByMintQuery {
    /// 结果数量限制 / Result limit
    #[param(example = 100)]
    pub limit: Option<usize>,
}

/// 查询用户激活订单的请求参数 / Query parameters for user's active orders
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ActiveOrdersByUserQuery {
    /// Mint 地址（可选）/ Mint address (optional)
    pub mint: Option<String>,
    /// 订单方向: up=做空, dn=做多（可选）/ Order direction: up=short, dn=long (optional)
    #[param(example = "up")]
    pub direction: Option<String>,
    /// 结果数量限制 / Result limit
    #[param(example = 100)]
    pub limit: Option<usize>,
}

/// 查询已关闭订单的请求参数 / Query parameters for closed orders
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ClosedOrdersQuery {
    /// 开始时间戳（Unix秒）/ Start timestamp (Unix seconds)
    pub start_time: Option<u32>,
    /// 结束时间戳（Unix秒）/ End timestamp (Unix seconds)
    pub end_time: Option<u32>,
    /// 结果数量限制 / Result limit
    #[param(example = 100)]
    pub limit: Option<usize>,
}

/// 订单列表响应 / Order list response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OrderListResponse {
    /// 订单列表 / Order list
    pub orders: Vec<OrderData>,
    /// 订单数量 / Order count
    pub count: usize,
}

/// 按 mint + direction 查询激活订单 / Query active orders by mint + direction
///
/// 查询指定代币和方向的所有激活订单。
/// Query all active orders for specified token and direction.
#[utoipa::path(
    get,
    path = "/api/orderbook/active/{mint}/{direction}",
    params(
        ("mint" = String, Path, description = "代币地址 / Token address", example = "So11111111111111111111111111111111111111112"),
        ("direction" = String, Path, description = "订单方向: up=做空订单簿, dn=做多订单簿 / Order direction: up=short orderbook, dn=long orderbook", example = "up"),
        ActiveOrdersByMintQuery
    ),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = OrderListResponse),
        (status = 500, description = "服务器错误 / Server error")
    ),
    tag = "OrderBook"
)]
pub async fn get_active_orders_by_mint(
    State(state): State<OrderBookState>,
    Path((mint, direction)): Path<(String, String)>,
    Query(params): Query<ActiveOrdersByMintQuery>,
) -> impl IntoResponse {
    // 验证方向参数 / Validate direction parameter
    if direction != "up" && direction != "dn" {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<OrderListResponse>::error(
                400,
                "无效的方向参数，必须是 'up' 或 'dn' / Invalid direction parameter, must be 'up' or 'dn'".to_string(),
            )),
        )
            .into_response();
    }

    match state
        .orderbook_storage
        .get_active_orders_by_mint(&mint, &direction, params.limit)
        .await
    {
        Ok(orders) => {
            let count = orders.len();
            (
                StatusCode::OK,
                Json(ApiResponse::ok(OrderListResponse { orders, count })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<OrderListResponse>::error(
                500,
                format!("查询失败 / Query failed: {}", e)
            )),
        )
            .into_response(),
    }
}

/// 按 user 查询激活订单 / Query active orders by user
///
/// 查询指定用户的所有激活订单，可选择性过滤 mint 和方向。
/// Query all active orders for specified user, with optional mint and direction filters.
#[utoipa::path(
    get,
    path = "/api/orderbook/active/user/{user}",
    params(
        ("user" = String, Path, description = "用户地址 / User address", example = "User111111111111111111111111111111111111111"),
        ActiveOrdersByUserQuery
    ),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = OrderListResponse),
        (status = 500, description = "服务器错误 / Server error")
    ),
    tag = "OrderBook"
)]
pub async fn get_active_orders_by_user(
    State(state): State<OrderBookState>,
    Path(user): Path<String>,
    Query(params): Query<ActiveOrdersByUserQuery>,
) -> impl IntoResponse {
    // 验证方向参数（如果提供）/ Validate direction parameter (if provided)
    if let Some(ref dir) = params.direction {
        if dir != "up" && dir != "dn" {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<OrderListResponse>::error(
                    400,
                    "无效的方向参数，必须是 'up' 或 'dn' / Invalid direction parameter, must be 'up' or 'dn'".to_string(),
                )),
            )
                .into_response();
        }
    }

    match state
        .orderbook_storage
        .get_active_orders_by_user_mint(
            &user,
            params.mint.as_deref(),
            params.direction.as_deref(),
            params.limit,
        )
        .await
    {
        Ok(orders) => {
            let count = orders.len();
            (
                StatusCode::OK,
                Json(ApiResponse::ok(OrderListResponse { orders, count })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<OrderListResponse>::error(
                500,
                format!("查询失败 / Query failed: {}", e)
            )),
        )
            .into_response(),
    }
}

/// 按 order_id 查询单个激活订单 / Query single active order by order_id
///
/// 通过订单ID查询单个激活订单。
/// Query a single active order by order ID.
#[utoipa::path(
    get,
    path = "/api/orderbook/active/order/{mint}/{direction}/{order_id}",
    params(
        ("mint" = String, Path, description = "代币地址 / Token address", example = "So11111111111111111111111111111111111111112"),
        ("direction" = String, Path, description = "订单方向: up=做空, dn=做多 / Order direction: up=short, dn=long", example = "up"),
        ("order_id" = u64, Path, description = "订单ID / Order ID", example = 1)
    ),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = OrderData),
        (status = 404, description = "订单不存在 / Order not found"),
        (status = 500, description = "服务器错误 / Server error")
    ),
    tag = "OrderBook"
)]
pub async fn get_active_order_by_id(
    State(state): State<OrderBookState>,
    Path((mint, direction, order_id)): Path<(String, String, u64)>,
) -> impl IntoResponse {
    // 验证方向参数 / Validate direction parameter
    if direction != "up" && direction != "dn" {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<OrderData>::error(
                400,
                "无效的方向参数，必须是 'up' 或 'dn' / Invalid direction parameter, must be 'up' or 'dn'".to_string(),
            )),
        )
            .into_response();
    }

    match state
        .orderbook_storage
        .get_active_order_by_id(&mint, &direction, order_id)
        .await
    {
        Ok(Some(order)) => (StatusCode::OK, Json(ApiResponse::ok(order))).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<OrderData>::error(
                404,
                "订单不存在 / Order not found".to_string(),
            )),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<OrderData>::error(
                500,
                format!("查询失败 / Query failed: {}", e)
            )),
        )
            .into_response(),
    }
}

/// 按 user 查询已关闭订单 / Query closed orders by user
///
/// 查询指定用户的已关闭订单，可按时间范围过滤。
/// Query closed orders for specified user, with optional time range filter.
#[utoipa::path(
    get,
    path = "/api/orderbook/closed/user/{user}",
    params(
        ("user" = String, Path, description = "用户地址 / User address", example = "User111111111111111111111111111111111111111"),
        ClosedOrdersQuery
    ),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = OrderListResponse),
        (status = 500, description = "服务器错误 / Server error")
    ),
    tag = "OrderBook"
)]
pub async fn get_closed_orders_by_user(
    State(state): State<OrderBookState>,
    Path(user): Path<String>,
    Query(params): Query<ClosedOrdersQuery>,
) -> impl IntoResponse {
    match state
        .orderbook_storage
        .get_closed_orders_by_user(&user, params.start_time, params.end_time, params.limit)
        .await
    {
        Ok(orders) => {
            let count = orders.len();
            (
                StatusCode::OK,
                Json(ApiResponse::ok(OrderListResponse { orders, count })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<OrderListResponse>::error(
                500,
                format!("查询失败 / Query failed: {}", e)
            )),
        )
            .into_response(),
    }
}
