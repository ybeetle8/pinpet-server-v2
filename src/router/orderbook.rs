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
use crate::router::db::SortOrder;
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
    pub max_limit: usize,  // 最大返回数量限制 / Max result limit
}

fn default_page() -> u32 { 1 }
fn default_page_size() -> u32 { 20 }

/// 查询激活订单的请求参数（按 mint + direction）/ Query parameters for active orders (by mint + direction)
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ActiveOrdersByMintQuery {
    /// 页码（从1开始）/ Page number (starts from 1)
    #[param(example = 1, minimum = 1)]
    #[serde(default = "default_page")]
    pub page: u32,
    /// 每页数量 / Page size
    #[param(example = 20, minimum = 1)]
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    /// 排序方向: asc=升序(slot从小到大), desc=降序(slot从大到小)，默认降序 / Sort order: asc=ascending(slot low to high), desc=descending(slot high to low), default desc
    #[param(example = "desc")]
    #[serde(default)]
    pub sort: SortOrder,
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
    /// 页码（从1开始）/ Page number (starts from 1)
    #[param(example = 1, minimum = 1)]
    #[serde(default = "default_page")]
    pub page: u32,
    /// 每页数量 / Page size
    #[param(example = 20, minimum = 1)]
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    /// 排序方向: asc=升序(slot从小到大), desc=降序(slot从大到小)，默认降序 / Sort order: asc=ascending(slot low to high), desc=descending(slot high to low), default desc
    #[param(example = "desc")]
    #[serde(default)]
    pub sort: SortOrder,
}

/// 查询已关闭订单的请求参数 / Query parameters for closed orders
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ClosedOrdersQuery {
    /// 页码（从1开始）/ Page number (starts from 1)
    #[param(example = 1, minimum = 1)]
    #[serde(default = "default_page")]
    pub page: u32,
    /// 每页数量 / Page size
    #[param(example = 20, minimum = 1)]
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    /// 排序方向: asc=升序(close_time从小到大), desc=降序(close_time从大到小)，默认降序 / Sort order: asc=ascending(close_time low to high), desc=descending(close_time high to low), default desc
    #[param(example = "desc")]
    #[serde(default)]
    pub sort: SortOrder,
}

/// 带 mint 的订单数据 / Order data with mint
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct OrderDataWithMint {
    /// 代币地址 / Token mint address
    #[schema(example = "So11111111111111111111111111111111111111112")]
    pub mint: String,
    /// 订单数据 / Order data
    #[serde(flatten)]
    pub order: OrderData,
}

/// 订单列表响应 / Order list response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OrderListResponse {
    /// 订单列表 / Order list
    pub orders: Vec<OrderDataWithMint>,
    /// 订单数量 / Order count
    pub count: usize,
}

/// 分页订单响应 / Paginated order response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PaginatedOrdersResponse {
    /// 订单列表 / Order list
    pub orders: Vec<OrderDataWithMint>,
    /// 总数量 / Total count
    #[schema(example = 100)]
    pub total: u64,
    /// 当前页码 / Current page
    #[schema(example = 1)]
    pub page: u32,
    /// 每页数量 / Page size
    #[schema(example = 20)]
    pub page_size: u32,
    /// 总页数 / Total pages
    #[schema(example = 5)]
    pub total_pages: u32,
}

/// 按 mint + direction 查询激活订单 / Query active orders by mint + direction
///
/// 查询指定代币和方向的所有激活订单，支持按 slot 排序（默认降序）。
/// Query all active orders for specified token and direction, supports sorting by slot (default descending).
#[utoipa::path(
    get,
    path = "/api/orderbook/active/{mint}/{direction}",
    params(
        ("mint" = String, Path, description = "代币地址 / Token address", example = "So11111111111111111111111111111111111111112"),
        ("direction" = String, Path, description = "订单方向: up=做空订单簿, dn=做多订单簿 / Order direction: up=short orderbook, dn=long orderbook", example = "up"),
        ActiveOrdersByMintQuery
    ),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = PaginatedOrdersResponse),
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
            Json(ApiResponse::<PaginatedOrdersResponse>::error(
                400,
                "无效的方向参数，必须是 'up' 或 'dn' / Invalid direction parameter, must be 'up' or 'dn'".to_string(),
            )),
        )
            .into_response();
    }

    // 限制 page_size 不超过 max_limit
    let page_size = params.page_size.min(state.max_limit as u32);

    match state
        .orderbook_storage
        .get_active_orders_by_mint(&mint, &direction, Some(state.max_limit))
        .await
    {
        Ok(all_orders) => {
            // 转换为 OrderDataWithMint
            let mut orders_with_mint: Vec<OrderDataWithMint> = all_orders
                .into_iter()
                .map(|(mint, order)| OrderDataWithMint { mint, order })
                .collect();

            // 按 slot 排序 / Sort by slot
            match params.sort {
                SortOrder::Asc => {
                    // 升序：slot 从小到大 / Ascending: slot from low to high
                    orders_with_mint.sort_by(|a, b| a.order.slot.cmp(&b.order.slot));
                },
                SortOrder::Desc => {
                    // 降序：slot 从大到小 / Descending: slot from high to low
                    orders_with_mint.sort_by(|a, b| b.order.slot.cmp(&a.order.slot));
                }
            }

            // 计算分页
            let total = orders_with_mint.len() as u64;
            let total_pages = ((total as f64) / (page_size as f64)).ceil() as u32;
            let start = ((params.page - 1) * page_size) as usize;
            let end = (start + page_size as usize).min(orders_with_mint.len());

            let page_orders = orders_with_mint[start..end].to_vec();

            (
                StatusCode::OK,
                Json(ApiResponse::ok(PaginatedOrdersResponse {
                    orders: page_orders,
                    total,
                    page: params.page,
                    page_size,
                    total_pages,
                })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<PaginatedOrdersResponse>::error(
                500,
                format!("查询失败 / Query failed: {}", e)
            )),
        )
            .into_response(),
    }
}

/// 按 user 查询激活订单 / Query active orders by user
///
/// 查询指定用户的所有激活订单，可选择性过滤 mint 和方向，支持按 slot 排序（默认降序）。
/// Query all active orders for specified user, with optional mint and direction filters, supports sorting by slot (default descending).
#[utoipa::path(
    get,
    path = "/api/orderbook/active/user/{user}",
    params(
        ("user" = String, Path, description = "用户地址 / User address", example = "User111111111111111111111111111111111111111"),
        ActiveOrdersByUserQuery
    ),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = PaginatedOrdersResponse),
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
                Json(ApiResponse::<PaginatedOrdersResponse>::error(
                    400,
                    "无效的方向参数，必须是 'up' 或 'dn' / Invalid direction parameter, must be 'up' or 'dn'".to_string(),
                )),
            )
                .into_response();
        }
    }

    // 限制 page_size 不超过 max_limit
    let page_size = params.page_size.min(state.max_limit as u32);

    match state
        .orderbook_storage
        .get_active_orders_by_user_mint(
            &user,
            params.mint.as_deref(),
            params.direction.as_deref(),
            Some(state.max_limit),
        )
        .await
    {
        Ok(all_orders) => {
            // 转换为 OrderDataWithMint
            let mut orders_with_mint: Vec<OrderDataWithMint> = all_orders
                .into_iter()
                .map(|(mint, order)| OrderDataWithMint { mint, order })
                .collect();

            // 按 slot 排序 / Sort by slot
            match params.sort {
                SortOrder::Asc => {
                    // 升序：slot 从小到大 / Ascending: slot from low to high
                    orders_with_mint.sort_by(|a, b| a.order.slot.cmp(&b.order.slot));
                },
                SortOrder::Desc => {
                    // 降序：slot 从大到小 / Descending: slot from high to low
                    orders_with_mint.sort_by(|a, b| b.order.slot.cmp(&a.order.slot));
                }
            }

            // 计算分页
            let total = orders_with_mint.len() as u64;
            let total_pages = ((total as f64) / (page_size as f64)).ceil() as u32;
            let start = ((params.page - 1) * page_size) as usize;
            let end = (start + page_size as usize).min(orders_with_mint.len());

            let page_orders = orders_with_mint[start..end].to_vec();

            (
                StatusCode::OK,
                Json(ApiResponse::ok(PaginatedOrdersResponse {
                    orders: page_orders,
                    total,
                    page: params.page,
                    page_size,
                    total_pages,
                })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<PaginatedOrdersResponse>::error(
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
        (status = 200, description = "查询成功 / Query successful", body = OrderDataWithMint),
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
            Json(ApiResponse::<OrderDataWithMint>::error(
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
        Ok(Some((mint, order))) => {
            let order_with_mint = OrderDataWithMint { mint, order };
            (StatusCode::OK, Json(ApiResponse::ok(order_with_mint))).into_response()
        },
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<OrderDataWithMint>::error(
                404,
                "订单不存在 / Order not found".to_string(),
            )),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<OrderDataWithMint>::error(
                500,
                format!("查询失败 / Query failed: {}", e)
            )),
        )
            .into_response(),
    }
}

/// 按 user 查询已关闭订单 / Query closed orders by user
///
/// 查询指定用户的已关闭订单，支持按 close_time 排序（默认降序）。
/// Query closed orders for specified user, supports sorting by close_time (default descending).
#[utoipa::path(
    get,
    path = "/api/orderbook/closed/user/{user}",
    params(
        ("user" = String, Path, description = "用户地址 / User address", example = "User111111111111111111111111111111111111111"),
        ClosedOrdersQuery
    ),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = PaginatedOrdersResponse),
        (status = 500, description = "服务器错误 / Server error")
    ),
    tag = "OrderBook"
)]
pub async fn get_closed_orders_by_user(
    State(state): State<OrderBookState>,
    Path(user): Path<String>,
    Query(params): Query<ClosedOrdersQuery>,
) -> impl IntoResponse {
    // 限制 page_size 不超过 max_limit
    let page_size = params.page_size.min(state.max_limit as u32);

    match state
        .orderbook_storage
        .get_closed_orders_by_user(&user, None, None, Some(state.max_limit))
        .await
    {
        Ok(all_orders) => {
            // 转换为 OrderDataWithMint
            let mut orders_with_mint: Vec<OrderDataWithMint> = all_orders
                .into_iter()
                .map(|(mint, order)| OrderDataWithMint { mint, order })
                .collect();

            // 按 close_time 排序 / Sort by close_time
            match params.sort {
                SortOrder::Asc => {
                    // 升序：close_time 从小到大 / Ascending: close_time from low to high
                    orders_with_mint.sort_by(|a, b| {
                        let time_a = a.order.close_time.unwrap_or(0);
                        let time_b = b.order.close_time.unwrap_or(0);
                        time_a.cmp(&time_b)
                    });
                },
                SortOrder::Desc => {
                    // 降序：close_time 从大到小 / Descending: close_time from high to low
                    orders_with_mint.sort_by(|a, b| {
                        let time_a = a.order.close_time.unwrap_or(0);
                        let time_b = b.order.close_time.unwrap_or(0);
                        time_b.cmp(&time_a)
                    });
                }
            }

            // 计算分页
            let total = orders_with_mint.len() as u64;
            let total_pages = ((total as f64) / (page_size as f64)).ceil() as u32;
            let start = ((params.page - 1) * page_size) as usize;
            let end = (start + page_size as usize).min(orders_with_mint.len());

            let page_orders = orders_with_mint[start..end].to_vec();

            (
                StatusCode::OK,
                Json(ApiResponse::ok(PaginatedOrdersResponse {
                    orders: page_orders,
                    total,
                    page: params.page,
                    page_size,
                    total_pages,
                })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<PaginatedOrdersResponse>::error(
                500,
                format!("查询失败 / Query failed: {}", e)
            )),
        )
            .into_response(),
    }
}
