// MarketsAbs 统计路由 / MarketsAbs Statistics Routes
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::IntoParams;

use crate::markets_abs::{
    MarketsAbsStorage, Period, TokenMarketsAbsResponse, TopMarketsAbsResponse,
};
use crate::util::{CommonResult, EmptyData};

/// MarketsAbs 路由状态 / MarketsAbs router state
#[derive(Clone)]
pub struct MarketsAbsState {
    pub markets_abs_storage: Arc<MarketsAbsStorage>,
}

/// 创建 MarketsAbs 路由 / Create markets abs routes
pub fn create_markets_abs_routes(markets_abs_storage: Arc<MarketsAbsStorage>) -> Router {
    let state = MarketsAbsState {
        markets_abs_storage,
    };

    Router::new()
        .route("/markets-abs/token/:mint", get(get_token_markets_abs))
        .route("/markets-abs/top", get(get_top_markets_abs))
        .with_state(state)
}

/// 单个币种绝对钱包数查询参数 / Token markets abs query parameters
#[derive(Debug, Deserialize, IntoParams)]
pub struct TokenMarketsAbsQuery {
    /// 时间周期 / Time period
    /// 可选值: 1m, 5m, 15m, 1h, 4h, 24h / Options: 1m, 5m, 15m, 1h, 4h, 24h
    #[param(example = "1h")]
    period: String,

    /// 时间桶(可选) / Time bucket (optional)
    /// Unix 时间戳,如果不提供则使用当前时间对齐后的时间桶
    /// Unix timestamp, if not provided, uses current time aligned to period
    #[param(example = 1703001600)]
    time_bucket: Option<u64>,
}

/// Top 绝对钱包数查询参数 / Top markets abs query parameters
#[derive(Debug, Deserialize, IntoParams)]
pub struct TopMarketsAbsQuery {
    /// 时间周期 / Time period
    /// 可选值: 1m, 5m, 15m, 1h, 4h, 24h / Options: 1m, 5m, 15m, 1h, 4h, 24h
    #[param(example = "1h")]
    period: String,

    /// 时间桶(可选) / Time bucket (optional)
    /// Unix 时间戳,如果不提供则使用当前时间对齐后的时间桶
    /// Unix timestamp, if not provided, uses current time aligned to period
    #[param(example = 1703001600)]
    time_bucket: Option<u64>,

    /// 返回数量限制 / Limit of results
    /// 默认为 100 / Default: 100
    #[param(example = 100)]
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    100
}

/// 查询单个币种的绝对钱包数 / Query absolute wallet count for a single token
///
/// # 中文说明 / Chinese Description
/// 查询指定币种从创世到指定时间点的累计唯一钱包数量（全局去重）
///
/// # English Description
/// Query cumulative unique wallet count for a specified token from genesis to specified time (global deduplication)
#[utoipa::path(
    get,
    path = "/markets-abs/token/{mint}",
    tag = "Statistics / 统计数据",
    params(
        ("mint" = String, Path, description = "Token mint 地址 / Token mint address", example = "4k3Dz2sV7C4YNP8pZdxU3LqRJpMf9gQ8tWxKvU2nEFGH"),
        TokenMarketsAbsQuery
    ),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = CommonResult<TokenMarketsAbsResponse>),
        (status = 400, description = "参数错误 / Invalid parameters", body = CommonResult<EmptyData>),
        (status = 500, description = "服务器错误 / Server error", body = CommonResult<EmptyData>)
    )
)]
pub async fn get_token_markets_abs(
    State(state): State<MarketsAbsState>,
    Path(mint): Path<String>,
    Query(query): Query<TokenMarketsAbsQuery>,
) -> impl IntoResponse {
    // 解析周期 / Parse period
    let period = match parse_period(&query.period) {
        Ok(p) => p,
        Err(e) => {
            return CommonResult::<TokenMarketsAbsResponse>::error(400, format!("Invalid period: {}", e))
                .into_response();
        }
    };

    // 查询绝对钱包数 / Query markets abs
    match state
        .markets_abs_storage
        .get_token_markets_abs(&mint, period, query.time_bucket)
    {
        Ok(result) => CommonResult::ok(result).into_response(),
        Err(e) => CommonResult::<TokenMarketsAbsResponse>::error(
            500,
            format!("Failed to query markets abs: {}", e),
        )
        .into_response(),
    }
}

/// 查询 Top N 绝对钱包数币种 / Query Top N tokens by absolute wallet count
///
/// # 中文说明 / Chinese Description
/// 查询从创世到指定时间点累计钱包数最多的前 N 个币种（全局去重）
///
/// # English Description
/// Query top N tokens with highest cumulative wallet count from genesis to specified time (global deduplication)
#[utoipa::path(
    get,
    path = "/markets-abs/top",
    tag = "Statistics / 统计数据",
    params(TopMarketsAbsQuery),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = CommonResult<TopMarketsAbsResponse>),
        (status = 400, description = "参数错误 / Invalid parameters", body = CommonResult<EmptyData>),
        (status = 500, description = "服务器错误 / Server error", body = CommonResult<EmptyData>)
    )
)]
pub async fn get_top_markets_abs(
    State(state): State<MarketsAbsState>,
    Query(query): Query<TopMarketsAbsQuery>,
) -> impl IntoResponse {
    // 解析周期 / Parse period
    let period = match parse_period(&query.period) {
        Ok(p) => p,
        Err(e) => {
            return CommonResult::<TopMarketsAbsResponse>::error(
                400,
                format!("Invalid period: {}", e),
            )
            .into_response();
        }
    };

    // 限制最大返回数量 / Limit maximum results
    let limit = query.limit.min(1000);

    // 查询 Top 绝对钱包数 / Query top markets abs
    match state
        .markets_abs_storage
        .get_top_markets_abs(period, query.time_bucket, limit)
    {
        Ok(result) => CommonResult::ok(result).into_response(),
        Err(e) => CommonResult::<TopMarketsAbsResponse>::error(
            500,
            format!("Failed to query top markets abs: {}", e),
        )
        .into_response(),
    }
}

/// 解析周期字符串 / Parse period string
fn parse_period(s: &str) -> Result<Period, String> {
    match s {
        "1m" => Ok(Period::OneMinute),
        "5m" => Ok(Period::FiveMinutes),
        "15m" => Ok(Period::FifteenMinutes),
        "1h" => Ok(Period::OneHour),
        "4h" => Ok(Period::FourHours),
        "24h" => Ok(Period::TwentyFourHours),
        _ => Err(format!(
            "Invalid period '{}', must be one of: 1m, 5m, 15m, 1h, 4h, 24h",
            s
        )),
    }
}
