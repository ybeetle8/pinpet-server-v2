// K线查询路由处理器 / K-line query route handlers
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::IntoParams;

use crate::db::KlineStorage;
use crate::kline::types::{KlineQuery, KlineQueryResponse};
use crate::util::CommonResult;

/// K线查询的共享状态 / Shared state for K-line queries
#[derive(Clone)]
pub struct KlineState {
    pub kline_storage: Arc<KlineStorage>,
}

/// K线查询参数 / K-line query parameters
#[derive(Debug, Deserialize, IntoParams)]
pub struct GetKlineParams {
    /// 代币mint地址 / Token mint address
    ///
    /// 示例 / Example: `7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU`
    pub mint: String,

    /// 时间间隔 / Time interval
    ///
    /// 支持的间隔 / Supported intervals:
    /// - `s1`: 1秒K线 / 1-second K-line
    /// - `s30`: 30秒K线 / 30-second K-line
    /// - `m5`: 5分钟K线 / 5-minute K-line
    pub interval: String,

    /// 页码(从1开始,默认1) / Page number (starts from 1, default 1)
    #[serde(default = "default_page")]
    pub page: usize,

    /// 每页数量(默认500,最大10000) / Items per page (default 500, max 10000)
    #[serde(default = "default_limit")]
    pub limit: usize,

    /// 排序方式 / Sort order
    ///
    /// 支持的排序 / Supported sort orders:
    /// - `time_desc`: 时间倒序(最新的在前,默认) / Time descending (newest first, default)
    /// - `time_asc`: 时间正序(最早的在前) / Time ascending (oldest first)
    #[serde(default = "default_order")]
    pub order: String,
}

/// 默认页码 / Default page number
fn default_page() -> usize {
    1
}

/// 默认每页数量 / Default items per page
fn default_limit() -> usize {
    500
}

/// 默认排序方式 / Default sort order
fn default_order() -> String {
    "time_desc".to_string()
}

/// 查询K线数据 / Query K-line data
///
/// 根据mint地址和时间间隔查询K线数据,支持分页和排序。
/// Query K-line data by mint address and time interval, supports pagination and sorting.
#[utoipa::path(
    get,
    path = "/api/kline",
    params(GetKlineParams),
    responses(
        (status = 200, description = "查询成功 / Query successful"),
        (status = 400, description = "参数错误 / Invalid parameters"),
        (status = 500, description = "服务器内部错误 / Internal server error")
    ),
    tag = "K线查询 / K-line Query"
)]
async fn get_kline(
    State(state): State<KlineState>,
    Query(params): Query<GetKlineParams>,
) -> impl IntoResponse {
    // 验证间隔参数 / Validate interval parameter
    if !matches!(params.interval.as_str(), "s1" | "s30" | "m5") {
        return (
            StatusCode::BAD_REQUEST,
            Json(CommonResult::<KlineQueryResponse>::error(
                400,
                "Invalid interval, must be one of: s1, s30, m5".to_string(),
            )),
        )
            .into_response();
    }

    // 验证排序参数 / Validate order parameter
    if !matches!(params.order.as_str(), "time_desc" | "time_asc") {
        return (
            StatusCode::BAD_REQUEST,
            Json(CommonResult::<KlineQueryResponse>::error(
                400,
                "Invalid order, must be one of: time_desc, time_asc".to_string(),
            )),
        )
            .into_response();
    }

    // 验证分页参数 / Validate pagination parameters
    if params.page < 1 {
        return (
            StatusCode::BAD_REQUEST,
            Json(CommonResult::<KlineQueryResponse>::error(
                400,
                "Page number must be greater than 0".to_string(),
            )),
        )
            .into_response();
    }

    // 限制每页最大数量 / Limit max items per page
    let limit = params.limit.min(10000);

    if limit == 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(CommonResult::<KlineQueryResponse>::error(
                400,
                "Limit must be greater than 0".to_string(),
            )),
        )
            .into_response();
    }

    // 构建查询参数 / Build query parameters
    let query = KlineQuery {
        mint_account: params.mint.clone(),
        interval: params.interval.clone(),
        page: Some(params.page),
        limit: Some(limit),
        order_by: Some(params.order.clone()),
    };

    // 执行查询 / Execute query
    match state.kline_storage.query_kline_data(query).await {
        Ok(response) => (
            StatusCode::OK,
            Json(CommonResult::ok(response)),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(CommonResult::<KlineQueryResponse>::error(
                500,
                format!("Failed to query K-line data: {}", e)
            )),
        )
            .into_response(),
    }
}

/// 创建K线路由 / Create K-line routes
pub fn routes() -> Router<KlineState> {
    Router::new()
        .route("/api/kline", get(get_kline))
}
