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
    pub price_service: Arc<crate::price::SolPriceService>,
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

/// 排序方式枚举 / Sort order enum
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortBy {
    /// 按热度排序 / Sort by hottest (based on activity and recency)
    Hot,
    /// 按创建时间降序排序(最新优先) / Sort by creation time descending (newest first)
    Created,
    /// 按创建时间升序排序(最早优先) / Sort by creation time ascending (oldest first)
    Ascending,
}

impl Default for SortBy {
    fn default() -> Self {
        SortBy::Created
    }
}

/// 获取Token列表参数(支持多种排序) / Get token list parameters (with multiple sort options)
#[derive(Debug, Deserialize, IntoParams)]
pub struct GetTokenListParams {
    /// 排序方式 / Sort order
    /// - hot: 按热度排序 / Sort by hottest
    /// - created: 按创建时间降序(最新优先) / Sort by creation time descending (newest first)
    /// - ascending: 按创建时间升序(最早优先) / Sort by creation time ascending (oldest first)
    #[serde(default)]
    pub sort_by: SortBy,
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

/// 获取Token列表(支持多种排序方式)
/// Get token list with multiple sort options
///
/// **注意 / Note:** 当前所有排序方式暂时返回相同结果(按创建时间降序)，未来会实现不同的排序逻辑
/// Currently all sort options return the same result (sorted by creation time descending), different sorting logic will be implemented in the future
#[utoipa::path(
    get,
    path = "/api/tokens/list",
    params(
        ("sort_by" = Option<SortBy>, Query, description = "排序方式 / Sort order: hot(按热度), created(按创建时间降序,默认), ascending(按创建时间升序) | Sort options: hot(by hottest), created(by creation time desc, default), ascending(by creation time asc). **未来会实现不同排序逻辑 / Different sorting logic will be implemented in future**"),
        ("limit" = Option<usize>, Query, description = "每页数量(默认20,最大100) / Items per page (default 20, max 100)"),
        ("before_timestamp" = Option<i64>, Query, description = "查询此时间戳之前的tokens / Get tokens before this timestamp")
    ),
    responses(
        (status = 200, description = "成功返回Token列表 / Successfully returned token list"),
        (status = 400, description = "无效的参数 / Invalid parameters"),
        (status = 500, description = "服务器内部错误 / Internal server error")
    ),
    tag = "tokens"
)]
pub async fn get_token_list(
    State(state): State<TokenState>,
    Query(params): Query<GetTokenListParams>,
) -> impl IntoResponse {
    // 限制最大每页数量 / Limit max items per page
    let limit = params.limit.min(100);

    // TODO: 未来根据不同的 sort_by 实现不同的排序逻辑
    // TODO: Implement different sorting logic based on sort_by in the future
    // 当前暂时统一使用按创建时间降序
    // Currently using creation time descending for all options
    match params.sort_by {
        SortBy::Hot => {
            // TODO: 实现热度排序逻辑 / Implement hotness sorting logic
            // 暂时使用创建时间降序 / Temporarily use creation time descending
        }
        SortBy::Created => {
            // 按创建时间降序(最新优先) / Sort by creation time descending (newest first)
        }
        SortBy::Ascending => {
            // TODO: 实现创建时间升序排序 / Implement creation time ascending sorting
            // 暂时使用创建时间降序 / Temporarily use creation time descending
        }
    }

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
            format!("Failed to query token list: {}", e),
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

/// 将TokenDetail转换为TokenSearchResult / Convert TokenDetail to TokenSearchResult
fn to_search_result(detail: &crate::db::TokenDetail, sol_price: f64) -> TokenSearchResult {
    use rust_decimal::Decimal;
    use rust_decimal::prelude::FromPrimitive;
    use std::str::FromStr;

    // 计算市值 mc = (latest_price / PRICE_PRECISION) * INITIAL_TOKEN_RESERVE * sol_price
    // Calculate market cap: mc = (latest_price / PRICE_PRECISION) * INITIAL_TOKEN_RESERVE * sol_price
    let mc = if let Ok(price_u128) = detail.latest_price.parse::<u128>() {
        // 将 u128 转为 Decimal
        if let Ok(price_decimal) = Decimal::from_str(&price_u128.to_string()) {
            // latest_price / PRICE_PRECISION_FACTOR
            let normalized_price = price_decimal / crate::curve_amm::CurveAMM::PRICE_PRECISION_FACTOR_DECIMAL;

            // * INITIAL_TOKEN_RESERVE
            let token_value = normalized_price * crate::curve_amm::CurveAMM::INITIAL_TOKEN_RESERVE_DECIMAL;

            // * sol_price (转为 Decimal)
            if let Some(sol_price_decimal) = Decimal::from_f64(sol_price) {
                let mc_decimal = token_value * sol_price_decimal;
                // 保留2位小数 / Round to 2 decimal places
                format!("{:.2}", mc_decimal)
            } else {
                "0.00".to_string()
            }
        } else {
            "0.00".to_string()
        }
    } else {
        "0.00".to_string()
    };

    TokenSearchResult {
        mint_account: detail.mint_account.clone(),
        symbol: detail.symbol.clone(),
        name: detail.name.clone(),
        image: detail.uri_data.as_ref().and_then(|d| d.image.clone()),
        created_at: detail.created_at,
        latest_price: detail.latest_price.clone(),
        mc,
    }
}

/// Token搜索参数 / Token search parameters
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct SearchTokensParams {
    /// 搜索关键词(Symbol或Mint地址) / Search keyword (symbol or mint address)
    /// - 长度 < 10: 按Symbol搜索 / Length < 10: search by symbol
    /// - 长度 >= 10: 按Mint搜索 / Length >= 10: search by mint
    pub q: String,
    /// 返回数量(仅Symbol搜索,默认20,最大100) / Return count (symbol search only, default 20, max 100)
    #[serde(default = "default_limit")]
    pub limit: usize,
}

/// Token搜索结果(简化版) / Token search result (simplified)
#[derive(Debug, Serialize, ToSchema)]
pub struct TokenSearchResult {
    /// Token mint地址 / Token mint address
    pub mint_account: String,
    /// Token符号 / Token symbol
    pub symbol: String,
    /// Token名称 / Token name
    pub name: String,
    /// Token图片URI / Token image URI
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// 创建时间Unix时间戳 / Creation Unix timestamp
    pub created_at: i64,
    /// 最新价格 / Latest price
    pub latest_price: String,
    /// 市值(美元) / Market cap (USD)
    pub mc: String,
}

/// Token搜索响应 / Token search response
#[derive(Debug, Serialize, ToSchema)]
pub struct SearchResponse {
    /// 搜索类型 / Search type: "symbol" or "mint"
    pub search_type: String,
    /// 搜索关键词 / Search query
    pub query: String,
    /// Mint搜索结果(单个) / Mint search result (single)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<TokenSearchResult>,
    /// Symbol搜索结果(列表) / Symbol search results (list)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<Vec<TokenSearchResult>>,
    /// 结果总数 / Total count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,
}

/// 搜索Token(统一接口,自动识别Symbol/Mint)
/// Search tokens (unified interface, auto-detect symbol/mint)
#[utoipa::path(
    get,
    path = "/api/tokens/search",
    params(
        ("q" = String, Query, description = "搜索关键词(Symbol或Mint地址) / Search keyword (symbol or mint address). 长度<10按Symbol搜索,>=10按Mint搜索 / Length<10 search by symbol, >=10 search by mint"),
        ("limit" = Option<usize>, Query, description = "返回数量(仅Symbol搜索,默认20,最大100) / Return count (symbol search only, default 20, max 100)")
    ),
    responses(
        (status = 200, description = "成功返回搜索结果 / Successfully returned search results"),
        (status = 404, description = "Token未找到 / Token not found"),
        (status = 400, description = "无效的参数 / Invalid parameters"),
        (status = 500, description = "服务器内部错误 / Internal server error")
    ),
    tag = "tokens"
)]
pub async fn search_tokens(
    State(state): State<TokenState>,
    Query(params): Query<SearchTokensParams>,
) -> impl IntoResponse {
    // 验证参数 / Validate parameters
    if params.q.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Search query cannot be empty".to_string(),
        ));
    }

    // 获取当前 SOL 价格 / Get current SOL price
    let sol_price = state.price_service.get_price_sync();

    // 根据输入长度判断搜索类型 / Determine search type by input length
    if params.q.len() < 10 {
        // Symbol搜索 / Symbol search
        let limit = params.limit.min(100);

        match state
            .token_storage
            .search_tokens_by_symbol(&params.q, limit)
        {
            Ok(tokens) => {
                let total = tokens.len();
                let search_results: Vec<TokenSearchResult> =
                    tokens.iter().map(|t| to_search_result(t, sol_price)).collect();

                Ok(Json(CommonResult::ok(SearchResponse {
                    search_type: "symbol".to_string(),
                    query: params.q,
                    token: None,
                    tokens: Some(search_results),
                    total: Some(total),
                })))
            }
            Err(e) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to search tokens by symbol: {}", e),
            )),
        }
    } else {
        // Mint搜索 / Mint search
        match state.token_storage.get_token_by_mint(&params.q) {
            Ok(Some(token)) => {
                let search_result = to_search_result(&token, sol_price);

                Ok(Json(CommonResult::ok(SearchResponse {
                    search_type: "mint".to_string(),
                    query: params.q,
                    token: Some(search_result),
                    tokens: None,
                    total: None,
                })))
            }
            Ok(None) => Err((
                StatusCode::NOT_FOUND,
                format!("Token not found: {}", params.q),
            )),
            Err(e) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query token by mint: {}", e),
            )),
        }
    }
}

/// 创建Token相关路由 / Create token related routes
pub fn routes() -> Router<TokenState> {
    Router::new()
        .route("/api/tokens/mint/:mint", get(get_token_by_mint))
        .route("/api/tokens/symbol", get(get_tokens_by_symbol))
        .route("/api/tokens/latest", get(get_latest_tokens))
        .route("/api/tokens/list", get(get_token_list))
        .route("/api/tokens/slot-range", get(get_tokens_by_slot_range))
        .route("/api/tokens/stats", get(get_token_stats))
        .route("/api/tokens/search", get(search_tokens))
}
