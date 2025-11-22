// Token查询路由处理器 / Token query route handlers
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

use crate::db::TokenStorage;
use crate::util::CommonResult;

/// Token查询的共享状态 / Shared state for token queries 
#[derive(Clone)]
pub struct TokenState {
    pub token_storage: Arc<TokenStorage>,
}

/// 根据mint查询Token参数 / Get token by mint parameters
#[derive(Debug, Deserialize, IntoParams)]
pub struct GetTokenByMintParams {
    /// Token mint地址 / Token mint address
    pub mint: String,
}

/// 根据symbol查询Token列表参数 / Get tokens by symbol parameters
#[derive(Debug, Deserialize, IntoParams)]
pub struct GetTokensBySymbolParams {
    /// Token符号 / Token symbol
    pub symbol: String,
    /// 每页数量(默认20,最大100) / Items per page (default 20, max 100)
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// 游标(用于分页) / Cursor (for pagination)
    pub cursor: Option<String>,
}

/// 获取最新Token列表参数 / Get latest tokens parameters
#[derive(Debug, Deserialize, IntoParams)]
pub struct GetLatestTokensParams {
    /// 每页数量(默认20,最大100) / Items per page (default 20, max 100)
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// 查询此时间戳之前的tokens / Get tokens before this timestamp
    pub before_timestamp: Option<i64>,
}

/// 按slot范围查询Token参数 / Get tokens by slot range parameters
#[derive(Debug, Deserialize, IntoParams)]
pub struct GetTokensBySlotRangeParams {
    /// 起始slot / Start slot
    pub start_slot: u64,
    /// 结束slot / End slot
    pub end_slot: u64,
}

/// Token列表响应 / Token list response
#[derive(Debug, Serialize, ToSchema)]
pub struct TokenListResponse {
    /// Token列表 / Token list
    pub tokens: Vec<crate::db::TokenDetail>,
    /// 总数 / Total count
    pub total: usize,
    /// 下一页游标(如果有) / Next cursor (if exists)
    pub next_cursor: Option<String>,
}

fn default_limit() -> usize {
    20
}

/// 根据mint查询Token详情
/// Get token detail by mint address
#[utoipa::path(
    get,
    path = "/api/tokens/mint/{mint}",
    params(
        ("mint" = String, Path, description = "Token mint地址 / Token mint address")
    ),
    responses(
        (status = 200, description = "成功返回Token详情 / Successfully returned token detail"),
        (status = 404, description = "Token未找到 / Token not found"),
        (status = 500, description = "服务器内部错误 / Internal server error")
    ),
    tag = "tokens"
)]
pub async fn get_token_by_mint(
    State(state): State<TokenState>,
    Path(mint): Path<String>,
) -> impl IntoResponse {
    match state.token_storage.get_token_by_mint(&mint) {
        Ok(Some(token)) => Ok(Json(CommonResult::ok(token))),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            format!("Token not found: {}", mint),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to query token: {}", e),
        )),
    }
}

/// 根据symbol查询Token列表
/// Get tokens by symbol
#[utoipa::path(
    get,
    path = "/api/tokens/symbol",
    params(
        ("symbol" = String, Query, description = "Token符号 / Token symbol"),
        ("limit" = Option<usize>, Query, description = "每页数量(默认20,最大100) / Items per page (default 20, max 100)"),
        ("cursor" = Option<String>, Query, description = "游标(用于分页) / Cursor (for pagination)")
    ),
    responses(
        (status = 200, description = "成功返回Token列表 / Successfully returned token list"),
        (status = 400, description = "无效的参数 / Invalid parameters"),
        (status = 500, description = "服务器内部错误 / Internal server error")
    ),
    tag = "tokens"
)]
pub async fn get_tokens_by_symbol(
    State(state): State<TokenState>,
    Query(params): Query<GetTokensBySymbolParams>,
) -> impl IntoResponse {
    // 限制最大每页数量 / Limit max items per page
    let limit = params.limit.min(100);

    match state
        .token_storage
        .get_tokens_by_symbol(&params.symbol, limit, params.cursor)
    {
        Ok(tokens) => {
            let total = tokens.len();
            let next_cursor = if total >= limit {
                tokens.last().map(|t| {
                    format!(
                        "token_symbol:{}:{}",
                        params.symbol.to_uppercase(),
                        t.mint_account
                    )
                })
            } else {
                None
            };

            Ok(Json(CommonResult::ok(TokenListResponse {
                tokens,
                total,
                next_cursor,
            })))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to query tokens by symbol: {}", e),
        )),
    }
}

/// 获取最新创建的Token列表
/// Get latest created tokens
#[utoipa::path(
    get,
    path = "/api/tokens/latest",
    params(
        ("limit" = Option<usize>, Query, description = "每页数量(默认20,最大100) / Items per page (default 20, max 100)"),
        ("before_timestamp" = Option<i64>, Query, description = "查询此时间戳之前的tokens / Get tokens before this timestamp")
    ),
    responses(
        (status = 200, description = "成功返回最新Token列表 / Successfully returned latest tokens"),
        (status = 400, description = "无效的参数 / Invalid parameters"),
        (status = 500, description = "服务器内部错误 / Internal server error")
    ),
    tag = "tokens"
)]
pub async fn get_latest_tokens(
    State(state): State<TokenState>,
    Query(params): Query<GetLatestTokensParams>,
) -> impl IntoResponse {
    // 限制最大每页数量 / Limit max items per page
    let limit = params.limit.min(100);

    match state
        .token_storage
        .get_latest_tokens(limit, params.before_timestamp)
    {
        Ok(tokens) => {
            let total = tokens.len();

            // 计算下一页游标 / Calculate next cursor
            // 如果返回了完整的一页，使用最后一个token的created_at作为游标
            // If a full page is returned, use the last token's created_at as cursor
            let next_cursor = if total >= limit {
                tokens.last().map(|t| t.created_at.to_string())
            } else {
                // 如果少于limit，说明已经是最后一页 / Less than limit means last page
                None
            };

            Ok(Json(CommonResult::ok(TokenListResponse {
                tokens,
                total,
                next_cursor,
            })))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to query latest tokens: {}", e),
        )),
    }
}

/// 按slot范围查询Token
/// Get tokens by slot range
#[utoipa::path(
    get,
    path = "/api/tokens/slot-range",
    params(
        ("start_slot" = u64, Query, description = "起始slot / Start slot"),
        ("end_slot" = u64, Query, description = "结束slot / End slot")
    ),
    responses(
        (status = 200, description = "成功返回slot范围内的Token列表 / Successfully returned tokens in slot range"),
        (status = 400, description = "无效的参数 / Invalid parameters"),
        (status = 500, description = "服务器内部错误 / Internal server error")
    ),
    tag = "tokens"
)]
pub async fn get_tokens_by_slot_range(
    State(state): State<TokenState>,
    Query(params): Query<GetTokensBySlotRangeParams>,
) -> impl IntoResponse {
    if params.start_slot > params.end_slot {
        return Err((
            StatusCode::BAD_REQUEST,
            "start_slot must be less than or equal to end_slot".to_string(),
        ));
    }

    match state
        .token_storage
        .get_tokens_by_slot_range(params.start_slot, params.end_slot)
    {
        Ok(tokens) => {
            let total = tokens.len();
            Ok(Json(CommonResult::ok(TokenListResponse {
                tokens,
                total,
                next_cursor: None,
            })))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to query tokens by slot range: {}", e),
        )),
    }
}

/// 获取Token统计信息
/// Get token statistics
#[utoipa::path(
    get,
    path = "/api/tokens/stats",
    responses(
        (status = 200, description = "成功返回Token统计信息 / Successfully returned token statistics"),
        (status = 500, description = "服务器内部错误 / Internal server error")
    ),
    tag = "tokens"
)]
pub async fn get_token_stats(
    State(state): State<TokenState>,
) -> impl IntoResponse {
    match state.token_storage.get_token_count() {
        Ok(count) => Ok(Json(CommonResult::ok(TokenStatsResponse {
            total_tokens: count,
        }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get token stats: {}", e),
        )),
    }
}

/// Token统计响应 / Token statistics response
#[derive(Debug, Serialize, ToSchema)]
pub struct TokenStatsResponse {
    /// Token总数 / Total tokens
    pub total_tokens: u64,
}

/// 创建Token相关路由 / Create token related routes
pub fn routes() -> Router<TokenState> {
    Router::new()
        .route("/api/tokens/mint/:mint", get(get_token_by_mint))
        .route("/api/tokens/symbol", get(get_tokens_by_symbol))
        .route("/api/tokens/latest", get(get_latest_tokens))
        .route("/api/tokens/slot-range", get(get_tokens_by_slot_range))
        .route("/api/tokens/stats", get(get_token_stats))
}
