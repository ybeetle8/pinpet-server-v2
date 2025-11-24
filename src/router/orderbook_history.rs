// OrderBook 用户交易历史查询接口
// OrderBook User Trading History Query Endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};
use utoipa::{IntoParams, ToSchema};

use crate::db::OrderBookStorage;
use crate::orderbook::closed_orders::{ClosedOrdersQuery, UserTradingStats};
use crate::orderbook::types::ClosedOrderRecord;
use crate::util::result::CommonResult;

/// 创建 OrderBook History 路由 / Create OrderBook History routes
pub fn routes() -> Router<Arc<OrderBookStorage>> {
    Router::new()
        .route(
            "/api/orderbook/user/:user_address/history",
            get(get_user_history),
        )
        .route(
            "/api/orderbook/user/:user_address/stats",
            get(get_user_stats),
        )
}

/// 查询参数 - 分页
/// Query parameters - Pagination
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
pub struct HistoryQueryParams {
    /// 页码(从1开始)
    /// Page number (starting from 1)
    #[serde(default = "default_page")]
    pub page: usize,

    /// 每页大小(默认20,最大100)
    /// Page size (default 20, max 100)
    #[serde(default = "default_page_size")]
    pub page_size: usize,

    /// 可选: 按 mint 过滤
    /// Optional: filter by mint
    pub mint: Option<String>,

    /// 可选: 按方向过滤 ("up" 或 "dn")
    /// Optional: filter by direction ("up" or "dn")
    pub direction: Option<String>,

    /// 可选: 开始时间戳
    /// Optional: start timestamp
    pub start_time: Option<u32>,

    /// 可选: 结束时间戳
    /// Optional: end timestamp
    pub end_time: Option<u32>,
}

fn default_page() -> usize {
    1
}

fn default_page_size() -> usize {
    20
}

/// 响应数据 - 已关闭订单列表
/// Response data - Closed orders list
#[derive(Debug, Serialize, ToSchema)]
pub struct ClosedOrdersResponse {
    /// 总数量
    /// Total count
    pub total: usize,

    /// 当前页码
    /// Current page
    pub page: usize,

    /// 每页大小
    /// Page size
    pub page_size: usize,

    /// 订单记录列表
    /// Order records list
    pub records: Vec<ClosedOrderRecord>,
}

/// 响应数据 - 用户交易统计
/// Response data - User trading stats
#[derive(Debug, Serialize, ToSchema)]
pub struct StatsResponse {
    /// 用户地址
    /// User address
    pub user_address: String,

    /// 统计数据
    /// Statistics data
    pub stats: UserTradingStats,
}

// ==================== API 端点 / API Endpoints ====================

/// 查询用户交易历史(已关闭订单)
/// Query user trading history (closed orders)
///
/// # 中文说明 / Chinese Description
/// 查询指定用户的所有已关闭订单,支持分页和过滤
///
/// # English Description
/// Query all closed orders for specified user, with pagination and filtering support
#[utoipa::path(
    get,
    path = "/api/orderbook/user/{user_address}/history",
    params(
        ("user_address" = String, Path, description = "用户 Solana 地址 / User Solana address"),
        HistoryQueryParams
    ),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = ClosedOrdersResponse),
        (status = 400, description = "参数错误 / Invalid parameters"),
        (status = 500, description = "服务器错误 / Server error")
    ),
    tag = "OrderBook"
)]
pub async fn get_user_history(
    Path(user_address): Path<String>,
    Query(params): Query<HistoryQueryParams>,
    State(orderbook_storage): State<Arc<OrderBookStorage>>,
) -> impl IntoResponse {
    info!(
        "📊 查询用户交易历史 / Query user history: user={}, mint={:?}, direction={:?}, page={}, page_size={}",
        &user_address[..8.min(user_address.len())],
        params.mint.as_ref().map(|s| &s[..8.min(s.len())]),
        params.direction,
        params.page,
        params.page_size
    );

    // 验证参数 / Validate parameters
    let page_size = params.page_size.min(100).max(1);
    let page = params.page.max(1);

    // 验证 direction 参数 / Validate direction parameter
    if let Some(ref direction) = params.direction {
        if direction != "up" && direction != "dn" {
            error!("❌ 无效的 direction 参数 / Invalid direction parameter: {}", direction);
            return (
                StatusCode::BAD_REQUEST,
                Json(CommonResult::<()>::error(
                    400,
                    format!("Invalid direction: {}, expected 'up' or 'dn'", direction),
                )),
            )
                .into_response();
        }
    }

    // 创建查询实例 / Create query instance
    let query = ClosedOrdersQuery::new(orderbook_storage.db());

    // 执行查询 / Execute query
    let records = match query.query_user_closed_orders(&user_address, None) {
        Ok(r) => r,
        Err(e) => {
            error!("❌ 查询失败 / Query failed: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(CommonResult::<()>::error(500, e.to_string())),
            )
                .into_response();
        }
    };

    // 应用过滤器 / Apply filters
    let mut filtered = records;

    // 按 mint 过滤 / Filter by mint
    // TODO: 需要在 ClosedOrderRecord 中添加 mint 字段或从键中解析
    // if let Some(mint) = params.mint {
    //     filtered = filtered.into_iter().filter(|r| ...).collect();
    // }

    // 按 direction 过滤 / Filter by direction
    // TODO: 需要在 ClosedOrderRecord 中添加 direction 字段或从键中解析
    // if let Some(direction) = params.direction {
    //     filtered = filtered.into_iter().filter(|r| ...).collect();
    // }

    // 按时间范围过滤 / Filter by time range
    if let (Some(start), Some(end)) = (params.start_time, params.end_time) {
        filtered = filtered
            .into_iter()
            .filter(|r| {
                let ts = r.close_info.close_timestamp;
                ts >= start && ts <= end
            })
            .collect();
    }

    // 分页 / Pagination
    let total = filtered.len();
    let start_idx = (page - 1) * page_size;

    let page_records: Vec<ClosedOrderRecord> = filtered
        .into_iter()
        .skip(start_idx)
        .take(page_size)
        .collect();

    let response = ClosedOrdersResponse {
        total,
        page,
        page_size,
        records: page_records,
    };

    info!(
        "✅ 查询成功 / Query successful: user={}, total={}, returned={}",
        &user_address[..8.min(user_address.len())],
        total,
        response.records.len()
    );

    (StatusCode::OK, Json(CommonResult::ok(response))).into_response()
}

/// 查询用户交易统计
/// Query user trading statistics
///
/// # 中文说明 / Chinese Description
/// 统计用户所有已关闭订单的盈亏、胜率等信息
///
/// # English Description
/// Calculate PnL, win rate and other stats from all closed orders
#[utoipa::path(
    get,
    path = "/api/orderbook/user/{user_address}/stats",
    params(
        ("user_address" = String, Path, description = "用户 Solana 地址 / User Solana address")
    ),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = StatsResponse),
        (status = 500, description = "服务器错误 / Server error")
    ),
    tag = "OrderBook"
)]
pub async fn get_user_stats(
    Path(user_address): Path<String>,
    State(orderbook_storage): State<Arc<OrderBookStorage>>,
) -> impl IntoResponse {
    info!(
        "📈 查询用户交易统计 / Query user stats: user={}",
        &user_address[..8.min(user_address.len())]
    );

    let query = ClosedOrdersQuery::new(orderbook_storage.db());

    let stats = match query.calculate_user_stats(&user_address) {
        Ok(s) => s,
        Err(e) => {
            error!("❌ 统计失败 / Stats calculation failed: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(CommonResult::<()>::error(500, e.to_string())),
            )
                .into_response();
        }
    };

    let response = StatsResponse {
        user_address: user_address.clone(),
        stats,
    };

    info!(
        "✅ 统计成功 / Stats calculation successful: user={}, total_trades={}",
        &user_address[..8.min(user_address.len())],
        response.stats.total_trades
    );

    (StatusCode::OK, Json(CommonResult::ok(response))).into_response()
}
