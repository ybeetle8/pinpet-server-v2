// 交易额路由 / Volume Statistics Routes
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::IntoParams;

use crate::util::{CommonResult, EmptyData};
use crate::volume::{Period, TopVolumeResponse, TokenVolumeResponse, VolumeStorage};

/// 交易额路由状态 / Volume router state
#[derive(Clone)]
pub struct VolumeState {
    pub volume_storage: Arc<VolumeStorage>,
}

/// 创建交易额路由 / Create volume routes
pub fn create_volume_routes(volume_storage: Arc<VolumeStorage>) -> Router {
    let state = VolumeState { volume_storage };

    Router::new()
        .route("/volume/token/:mint", get(get_token_volume))
        .route("/volume/top", get(get_top_volume))
        .with_state(state)
}

/// 单个币种交易额查询参数 / Token volume query parameters
#[derive(Debug, Deserialize, IntoParams)]
pub struct TokenVolumeQuery {
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

/// Top 交易额查询参数 / Top volume query parameters
#[derive(Debug, Deserialize, IntoParams)]
pub struct TopVolumeQuery {
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

/// 查询单个币种的交易额 / Query volume for a single token
///
/// # 中文说明 / Chinese Description
/// 查询指定币种在特定时间周期内的交易额统计信息
///
/// # English Description
/// Query volume statistics for a specified token in a specific time period
#[utoipa::path(
    get,
    path = "/volume/token/{mint}",
    tag = "Volume Statistics / 交易额统计",
    params(
        ("mint" = String, Path, description = "Token mint 地址 / Token mint address", example = "4k3Dz2sV7C4YNP8pZdxU3LqRJpMf9gQ8tWxKvU2nEFGH"),
        TokenVolumeQuery
    ),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = CommonResult<TokenVolumeResponse>),
        (status = 400, description = "参数错误 / Invalid parameters", body = CommonResult<EmptyData>),
        (status = 500, description = "服务器错误 / Server error", body = CommonResult<EmptyData>)
    )
)]
pub async fn get_token_volume(
    State(state): State<VolumeState>,
    Path(mint): Path<String>,
    Query(query): Query<TokenVolumeQuery>,
) -> impl IntoResponse {
    // 解析周期 / Parse period
    let period = match parse_period(&query.period) {
        Ok(p) => p,
        Err(e) => {
            return CommonResult::<TokenVolumeResponse>::error(
                400,
                format!("Invalid period: {}", e)
            ).into_response();
        }
    };

    // 查询交易额 / Query volume
    match state
        .volume_storage
        .get_token_volume(&mint, period, query.time_bucket)
    {
        Ok(result) => CommonResult::ok(result).into_response(),
        Err(e) => CommonResult::<TokenVolumeResponse>::error(
            500,
            format!("Failed to query volume: {}", e)
        ).into_response(),
    }
}

/// 查询 Top N 交易额币种 / Query Top N tokens by volume
///
/// # 中文说明 / Chinese Description
/// 查询指定时间周期内交易额最高的前 N 个币种
///
/// # English Description
/// Query top N tokens with highest volume in a specific time period
#[utoipa::path(
    get,
    path = "/volume/top",
    tag = "Volume Statistics / 交易额统计",
    params(TopVolumeQuery),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = CommonResult<TopVolumeResponse>),
        (status = 400, description = "参数错误 / Invalid parameters", body = CommonResult<EmptyData>),
        (status = 500, description = "服务器错误 / Server error", body = CommonResult<EmptyData>)
    )
)]
pub async fn get_top_volume(
    State(state): State<VolumeState>,
    Query(query): Query<TopVolumeQuery>,
) -> impl IntoResponse {
    // 解析周期 / Parse period
    let period = match parse_period(&query.period) {
        Ok(p) => p,
        Err(e) => {
            return CommonResult::<TopVolumeResponse>::error(
                400,
                format!("Invalid period: {}", e)
            ).into_response();
        }
    };

    // 限制最大返回数量 / Limit maximum results
    let limit = query.limit.min(1000);

    // 查询 Top 交易额 / Query top volume
    match state
        .volume_storage
        .get_top_volume(period, query.time_bucket, limit)
    {
        Ok(result) => CommonResult::ok(result).into_response(),
        Err(e) => CommonResult::<TopVolumeResponse>::error(
            500,
            format!("Failed to query top volume: {}", e)
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
