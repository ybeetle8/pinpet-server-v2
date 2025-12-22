// Markets 统计路由 / Markets Statistics Routes
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::IntoParams;

use crate::markets::{MarketsStorage, Period, TokenMarketsResponse, TopMarketsResponse};
use crate::util::{CommonResult, EmptyData};

/// Markets 路由状态 / Markets router state
#[derive(Clone)]
pub struct MarketsState {
    pub markets_storage: Arc<MarketsStorage>,
}

/// 创建 Markets 路由 / Create markets routes
pub fn create_markets_routes(markets_storage: Arc<MarketsStorage>) -> Router {
    let state = MarketsState { markets_storage };

    Router::new()
        .route("/markets/token/:mint", get(get_token_markets))
        .route("/markets/top", get(get_top_markets))
        .with_state(state)
}

/// 单个币种钱包数查询参数 / Token markets query parameters
#[derive(Debug, Deserialize, IntoParams)]
pub struct TokenMarketsQuery {
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

/// Top 钱包数查询参数 / Top markets query parameters
#[derive(Debug, Deserialize, IntoParams)]
pub struct TopMarketsQuery {
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

/// 查询单个币种的钱包数 / Query markets for a single token
///
/// # 中文说明 / Chinese Description
/// 查询指定币种在特定时间周期内参与交易的唯一钱包数量
///
/// # English Description
/// Query unique wallet count for a specified token in a specific time period
#[utoipa::path(
    get,
    path = "/markets/token/{mint}",
    tag = "Statistics / 统计数据",
    params(
        ("mint" = String, Path, description = "Token mint 地址 / Token mint address", example = "4k3Dz2sV7C4YNP8pZdxU3LqRJpMf9gQ8tWxKvU2nEFGH"),
        TokenMarketsQuery
    ),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = CommonResult<TokenMarketsResponse>),
        (status = 400, description = "参数错误 / Invalid parameters", body = CommonResult<EmptyData>),
        (status = 500, description = "服务器错误 / Server error", body = CommonResult<EmptyData>)
    )
)]
pub async fn get_token_markets(
    State(state): State<MarketsState>,
    Path(mint): Path<String>,
    Query(query): Query<TokenMarketsQuery>,
) -> impl IntoResponse {
    // 解析周期 / Parse period
    let period = match parse_period(&query.period) {
        Ok(p) => p,
        Err(e) => {
            return CommonResult::<TokenMarketsResponse>::error(
                400,
                format!("Invalid period: {}", e)
            ).into_response();
        }
    };

    // 查询钱包数 / Query markets
    match state
        .markets_storage
        .get_token_markets(&mint, period, query.time_bucket)
    {
        Ok(result) => CommonResult::ok(result).into_response(),
        Err(e) => CommonResult::<TokenMarketsResponse>::error(
            500,
            format!("Failed to query markets: {}", e)
        ).into_response(),
    }
}

/// 查询 Top N 钱包数币种 / Query Top N tokens by wallet count
///
/// # 中文说明 / Chinese Description
/// 查询指定时间周期内参与钱包数最多的前 N 个币种
///
/// # English Description
/// Query top N tokens with highest wallet count in a specific time period
#[utoipa::path(
    get,
    path = "/markets/top",
    tag = "Statistics / 统计数据",
    params(TopMarketsQuery),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = CommonResult<TopMarketsResponse>),
        (status = 400, description = "参数错误 / Invalid parameters", body = CommonResult<EmptyData>),
        (status = 500, description = "服务器错误 / Server error", body = CommonResult<EmptyData>)
    )
)]
pub async fn get_top_markets(
    State(state): State<MarketsState>,
    Query(query): Query<TopMarketsQuery>,
) -> impl IntoResponse {
    // 解析周期 / Parse period
    let period = match parse_period(&query.period) {
        Ok(p) => p,
        Err(e) => {
            return CommonResult::<TopMarketsResponse>::error(
                400,
                format!("Invalid period: {}", e)
            ).into_response();
        }
    };

    // 限制最大返回数量 / Limit maximum results
    let limit = query.limit.min(1000);

    // 查询 Top 钱包数 / Query top markets
    match state
        .markets_storage
        .get_top_markets(period, query.time_bucket, limit)
    {
        Ok(result) => CommonResult::ok(result).into_response(),
        Err(e) => CommonResult::<TopMarketsResponse>::error(
            500,
            format!("Failed to query top markets: {}", e)
        ).into_response(),
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
