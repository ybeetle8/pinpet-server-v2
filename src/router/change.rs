// 涨跌幅路由 / Change Statistics Routes
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::IntoParams;

use crate::change::{ChangeStorage, Direction, Period, TopChangeResponse, TokenChangeResponse};
use crate::util::{CommonResult, EmptyData};

/// 涨跌幅路由状态 / Change router state
#[derive(Clone)]
pub struct ChangeState {
    pub change_storage: Arc<ChangeStorage>,
}

/// 创建涨跌幅路由 / Create change routes
pub fn create_change_routes(change_storage: Arc<ChangeStorage>) -> Router {
    let state = ChangeState { change_storage };

    Router::new()
        .route("/change/token/:mint", get(get_token_change))
        .route("/change/top", get(get_top_change))
        .with_state(state)
}

/// 单个币种涨跌幅查询参数 / Token change query parameters
#[derive(Debug, Deserialize, IntoParams)]
pub struct TokenChangeQuery {
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

/// Top 涨跌幅查询参数 / Top change query parameters
#[derive(Debug, Deserialize, IntoParams)]
pub struct TopChangeQuery {
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

    /// 查询方向 / Query direction
    /// gain: 涨幅榜 (从高到低), loss: 跌幅榜 (从低到高)
    /// gain: gainers (high to low), loss: losers (low to high)
    #[param(example = "gain")]
    #[serde(default = "default_direction")]
    direction: String,
}

fn default_limit() -> usize {
    100
}

fn default_direction() -> String {
    "gain".to_string()
}

/// 查询单个币种的涨跌幅 / Query change for a single token
///
/// # 中文说明 / Chinese Description
/// 查询指定币种在特定时间周期内的涨跌幅统计信息
///
/// # English Description
/// Query change statistics for a specified token in a specific time period
#[utoipa::path(
    get,
    path = "/change/token/{mint}",
    tag = "Volume Statistics / 交易额统计",
    params(
        ("mint" = String, Path, description = "Token mint 地址 / Token mint address", example = "4k3Dz2sV7C4YNP8pZdxU3LqRJpMf9gQ8tWxKvU2nEFGH"),
        TokenChangeQuery
    ),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = CommonResult<TokenChangeResponse>),
        (status = 400, description = "参数错误 / Invalid parameters", body = CommonResult<EmptyData>),
        (status = 500, description = "服务器错误 / Server error", body = CommonResult<EmptyData>)
    )
)]
pub async fn get_token_change(
    State(state): State<ChangeState>,
    Path(mint): Path<String>,
    Query(query): Query<TokenChangeQuery>,
) -> impl IntoResponse {
    // 解析周期 / Parse period
    let period = match parse_period(&query.period) {
        Ok(p) => p,
        Err(e) => {
            return CommonResult::<TokenChangeResponse>::error(400, format!("Invalid period: {}", e))
                .into_response();
        }
    };

    // 查询涨跌幅 / Query change
    match state
        .change_storage
        .get_token_change(&mint, period, query.time_bucket)
    {
        Ok(result) => CommonResult::ok(result).into_response(),
        Err(e) => {
            CommonResult::<TokenChangeResponse>::error(500, format!("Failed to query change: {}", e))
                .into_response()
        }
    }
}

/// 查询 Top N 涨跌幅币种 / Query Top N tokens by change
///
/// # 中文说明 / Chinese Description
/// 查询指定时间周期内涨跌幅最高或最低的前 N 个币种
///
/// # English Description
/// Query top N tokens with highest or lowest change in a specific time period
#[utoipa::path(
    get,
    path = "/change/top",
    tag = "Volume Statistics / 交易额统计",
    params(TopChangeQuery),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = CommonResult<TopChangeResponse>),
        (status = 400, description = "参数错误 / Invalid parameters", body = CommonResult<EmptyData>),
        (status = 500, description = "服务器错误 / Server error", body = CommonResult<EmptyData>)
    )
)]
pub async fn get_top_change(
    State(state): State<ChangeState>,
    Query(query): Query<TopChangeQuery>,
) -> impl IntoResponse {
    // 解析周期 / Parse period
    let period = match parse_period(&query.period) {
        Ok(p) => p,
        Err(e) => {
            return CommonResult::<TopChangeResponse>::error(400, format!("Invalid period: {}", e))
                .into_response();
        }
    };

    // 解析方向 / Parse direction
    let direction = match parse_direction(&query.direction) {
        Ok(d) => d,
        Err(e) => {
            return CommonResult::<TopChangeResponse>::error(400, format!("Invalid direction: {}", e))
                .into_response();
        }
    };

    // 限制最大返回数量 / Limit maximum results
    let limit = query.limit.min(1000);

    // 查询 Top 涨跌幅 / Query top change
    match state
        .change_storage
        .get_top_change(period, query.time_bucket, limit, direction)
    {
        Ok(result) => CommonResult::ok(result).into_response(),
        Err(e) => {
            CommonResult::<TopChangeResponse>::error(500, format!("Failed to query top change: {}", e))
                .into_response()
        }
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

/// 解析方向字符串 / Parse direction string
fn parse_direction(s: &str) -> Result<Direction, String> {
    match s {
        "gain" => Ok(Direction::Gain),
        "loss" => Ok(Direction::Loss),
        _ => Err(format!(
            "Invalid direction '{}', must be one of: gain, loss",
            s
        )),
    }
}
