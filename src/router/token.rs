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
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use utoipa::{IntoParams, ToSchema};

use crate::db::TokenStorage;
use crate::util::CommonResult;
use crate::volume::{Period, VolumeStorage};
use crate::change::ChangeStorage;
use crate::fee::FeeStorage;
use crate::markets_abs::MarketsAbsStorage;
use crate::order_summary::{OrderSummaryStorage, OrderSummaryData};

/// Token列表缓存项 / Token list cache item
#[derive(Debug, Clone)]
pub struct TokenListCacheItem {
    /// 缓存的响应数据 / Cached response data
    pub data: TokenListResponse,
    /// 缓存时间 / Cache timestamp
    pub cached_at: SystemTime,
}

/// Token列表缓存 / Token list cache
/// 为每个 sort_by + limit 组合缓存结果
/// Cache results for each sort_by + limit combination
type TokenListCache = Arc<RwLock<std::collections::HashMap<String, TokenListCacheItem>>>;

/// Token查询的共享状态 / Shared state for token queries
#[derive(Clone)]
pub struct TokenState {
    pub token_storage: Arc<TokenStorage>,
    pub price_service: Arc<crate::price::SolPriceService>,

    // 新增统计存储依赖 / Added statistics storage dependencies
    pub volume_storage: Arc<VolumeStorage>,
    pub change_storage: Arc<ChangeStorage>,
    pub markets_abs_storage: Arc<MarketsAbsStorage>,

    // 缓存配置和存储 / Cache configuration and storage
    pub cache_ttl_secs: u64,
    pub list_cache: TokenListCache,

    // 订单汇总存储 / Order summary storage
    pub order_summary_storage: Arc<OrderSummaryStorage>,

    // 手续费存储 / Fee storage
    pub fee_storage: Arc<FeeStorage>,

    // Mint地址屏蔽服务 / Mint address blocking service
    pub blocked_mints_service: Arc<crate::blocked_mints::BlockedMintsService>,
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
    /// 全部(按创建时间降序) / All (by creation time desc)
    All,
    /// 最新(按创建时间降序) / Latest (by creation time desc)
    Latest,
    /// 24小时交易量排序 / Sort by 24h volume
    Liquid,
    /// 24小时涨幅排序 / Sort by 24h gain
    Rising,
    /// 24小时绝对钱包数排序 / Sort by 24h absolute markets
    Hottest,
}

impl Default for SortBy {
    fn default() -> Self {
        SortBy::Latest
    }
}

/// 获取Token列表参数(支持多种排序) / Get token list parameters (with multiple sort options)
#[derive(Debug, Deserialize, IntoParams)]
pub struct GetTokenListParams {
    /// 排序方式 / Sort order
    /// - all: 全部(按创建时间降序) / All (by creation time desc)
    /// - latest: 最新(按创建时间降序,默认) / Latest (by creation time desc, default)
    /// - liquid: 24小时交易量排序 / 24h volume (desc)
    /// - rising: 24小时涨幅排序 / 24h gain (desc)
    /// - hottest: 24小时绝对钱包数排序 / 24h absolute markets (desc)
    #[serde(default)]
    pub sort_by: SortBy,
    /// 每页数量(默认20,最大1000) / Items per page (default 20, max 1000)
    #[serde(default = "default_limit")]
    pub limit: usize,
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
#[derive(Debug, Clone, Serialize, ToSchema)]
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

/// 根据mint查询Token详情参数 / Get token by mint parameters
#[derive(Debug, Deserialize, IntoParams)]
pub struct GetTokenByMintParams {
    /// 是否包含24小时统计数据 / Include 24h statistics data
    /// 包含: volume, change, markets_abs
    /// Includes: volume, change, markets_abs
    #[serde(default)]
    pub include_stats: bool,
}

/// 根据mint查询Token详情
/// Get token detail by mint address
#[utoipa::path(
    get,
    path = "/api/tokens/mint/{mint}",
    params(
        ("mint" = String, Path, description = "Token mint地址 / Token mint address"),
        ("include_stats" = Option<bool>, Query, description = "是否包含24小时统计数据(volume/change/markets_abs) / Include 24h statistics (volume/change/markets_abs). 默认: false / Default: false")
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
    Query(params): Query<GetTokenByMintParams>,
) -> impl IntoResponse {
    // 1. 查询 token 基础数据 / Query token base data
    let mut token = match state.token_storage.get_token_by_mint(&mint) {
        Ok(Some(t)) => t,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                format!("Token not found: {}", mint),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query token: {}", e),
            ))
        }
    };

    // 2. 获取SOL价格并计算价格信息 / Get SOL price and calculate price info
    let sol_price = state.price_service.get_price_sync();
    enrich_token_with_prices(&mut token, sol_price);

    // 3. 如果需要统计数据,附加到 extras / If stats needed, enrich to extras
    if params.include_stats {
        enrich_token_with_stats(&state, &mut token).await;
    }

    Ok(Json(CommonResult::ok(token)))
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
        Ok(mut tokens) => {
            // 获取SOL价格并计算价格信息 / Get SOL price and calculate price info
            let sol_price = state.price_service.get_price_sync();
            enrich_tokens_with_prices(&mut tokens, sol_price);

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
        Ok(mut tokens) => {
            // 过滤屏蔽的mint / Filter blocked mints
            let blocked_service = state.blocked_mints_service.clone();
            let mut filtered_tokens = Vec::new();
            for token in tokens {
                if !blocked_service.is_blocked(&token.mint_account).await {
                    filtered_tokens.push(token);
                }
            }
            tokens = filtered_tokens;

            // 获取SOL价格并计算价格信息 / Get SOL price and calculate price info
            let sol_price = state.price_service.get_price_sync();
            enrich_tokens_with_prices(&mut tokens, sol_price);

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
/// **缓存策略 / Cache Strategy:** 所有请求都会被缓存(可配置TTL),避免频繁查询
/// All requests are cached (configurable TTL) to avoid frequent queries
#[utoipa::path(
    get,
    path = "/api/tokens/list",
    params(
        ("sort_by" = Option<SortBy>, Query, description = "排序方式 / Sort order:\n- all: 全部(按创建时间降序) / All (by creation time desc)\n- latest: 最新(按创建时间降序,默认) / Latest (by creation time desc, default)\n- liquid: 24小时交易量排序 / 24h volume (desc)\n- rising: 24小时涨幅排序 / 24h gain (desc)\n- hottest: 24小时绝对钱包数排序 / 24h absolute markets (desc)"),
        ("limit" = Option<usize>, Query, description = "每页数量(默认20,最大1000) / Items per page (default 20, max 1000)")
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
    let limit = params.limit.min(1000);

    // 1. 检查缓存 / Check cache
    if let Some(cached) = get_cached_response(&state, &params.sort_by, limit).await {
        return Ok(Json(CommonResult::ok(cached)));
    }

    // 2. 根据 sort_by 调用不同的处理函数 / Call different handler based on sort_by
    let result = match params.sort_by {
        SortBy::All | SortBy::Latest => {
            // 按创建时间降序(最新优先) / Sort by creation time descending (newest first)
            handle_local_tokens(&state, limit).await
        }
        SortBy::Liquid => {
            // 24小时交易量排序 / Sort by 24h volume
            handle_liquid_tokens(&state, limit).await
        }
        SortBy::Rising => {
            // 24小时涨幅排序 / Sort by 24h gain
            handle_rising_tokens(&state, limit).await
        }
        SortBy::Hottest => {
            // 24小时绝对钱包数排序 / Sort by 24h absolute markets
            handle_hottest_tokens(&state, limit).await
        }
    };

    match result {
        Ok(response) => {
            // 3. 更新缓存 / Update cache
            set_cache(&state, &params.sort_by, limit, response.clone()).await;

            Ok(Json(CommonResult::ok(response)))
        }
        Err(err) => Err(err),
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
        Ok(mut tokens) => {
            // 获取SOL价格并计算价格信息 / Get SOL price and calculate price info
            let sol_price = state.price_service.get_price_sync();
            enrich_tokens_with_prices(&mut tokens, sol_price);

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

    let mut mc = "0.00".to_string();
    let mut usd_price = "0".to_string();

    // 计算 mc 和 usd_price / Calculate mc and usd_price
    if let Ok(price_u128) = detail.latest_price.parse::<u128>() {
        // ✅ 使用 u128_to_decimal 检查价格是否在安全范围内
        if let Some(normalized_price) = crate::curve_amm::CurveAMM::u128_to_decimal(price_u128) {
            if let Some(sol_price_decimal) = Decimal::from_f64(sol_price) {
                // 1. 计算 mc (市值,美元,格式化) / Calculate mc (market cap, USD, formatted)
                // 使用 detail 的动态 initial_virtual_token 参数 / Use dynamic initial_virtual_token from detail
                if let Some(token_reserve_decimal) = crate::curve_amm::CurveAMM::u64_to_token_decimal(detail.initial_virtual_token) {
                    if let Some(token_value) = normalized_price.checked_mul(token_reserve_decimal) {
                        if let Some(mc_decimal) = token_value.checked_mul(sol_price_decimal) {
                            mc = format!("{:.2}", mc_decimal);
                        } else {
                            tracing::warn!("MC 计算溢出 / MC calculation overflow for token: {}", detail.mint_account);
                            mc = "overflow".to_string();
                        }
                    } else {
                        tracing::warn!("Token value 计算溢出 / Token value calculation overflow for token: {}", detail.mint_account);
                        mc = "overflow".to_string();
                    }
                } else {
                    tracing::warn!("Token reserve decimal 转换失败 / Token reserve decimal conversion failed for token: {}", detail.mint_account);
                    mc = "error".to_string();
                }

                // 2. 计算 usd_price (Token美元价格,大整数,保持10^23精度) / Calculate usd_price (Token USD price, big integer, 10^23 precision)
                let price_decimal = Decimal::from(price_u128);
                if let Some(usd_price_decimal) = price_decimal.checked_mul(sol_price_decimal) {
                    // 转换为字符串(整数形式,去掉小数部分) / Convert to string (integer form, remove decimal part)
                    usd_price = usd_price_decimal.trunc().to_string();
                } else {
                    tracing::warn!("USD price 计算溢出 / USD price calculation overflow for token: {}", detail.mint_account);
                    usd_price = "overflow".to_string();
                }
            }
        } else {
            tracing::warn!(
                "价格超出安全范围 / Price exceeds safe limit: token={}, price={}",
                detail.mint_account, price_u128
            );
            mc = "too_high".to_string();
            usd_price = "too_high".to_string();
        }
    }

    TokenSearchResult {
        mint_account: detail.mint_account.clone(),
        symbol: detail.symbol.clone(),
        name: detail.name.clone(),
        image: detail.uri_data.as_ref().and_then(|d| d.image.clone()),
        created_at: detail.created_at,
        mc,
        latest_price: detail.latest_price.clone(),
        usd_price,
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
    /// 市值(美元,格式化,2位小数) / Market cap (USD, formatted, 2 decimals)
    pub mc: String,
    /// 最新价格(SOL计,大整数,10^23精度) / Latest price (SOL, big integer, 10^23 precision)
    pub latest_price: String,
    /// Token美元价格(大整数,10^23精度) / Token USD price (big integer, 10^23 precision)
    pub usd_price: String,
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
                    token: None,
                    tokens: Some(vec![search_result]),
                    total: Some(1),
                })))
            }
            Ok(None) => {
                // 未找到也返回空数组，而不是404错误 / Return empty array instead of 404
                Ok(Json(CommonResult::ok(SearchResponse {
                    search_type: "mint".to_string(),
                    query: params.q,
                    token: None,
                    tokens: Some(Vec::new()),
                    total: Some(0),
                })))
            }
            Err(e) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query token by mint: {}", e),
            )),
        }
    }
}

// ============================================================================
// 辅助函数 / Helper Functions
// ============================================================================

/// 为单个Token计算价格信息(mc和usd_price) / Calculate price info for a single token
///
/// # 参数 / Parameters
/// * `token` - 要计算的Token / Token to calculate for
/// * `sol_price` - 当前SOL的美元价格 / Current SOL price in USD
fn enrich_token_with_prices(token: &mut crate::db::TokenDetail, sol_price: f64) {
    use rust_decimal::Decimal;
    use rust_decimal::prelude::FromPrimitive;

    // 解析 latest_price / Parse latest_price
    if let Ok(price_u128) = token.latest_price.parse::<u128>() {
        // ✅ 使用 u128_to_decimal 检查价格是否在安全范围内
        if let Some(normalized_price) = crate::curve_amm::CurveAMM::u128_to_decimal(price_u128) {
            if let Some(sol_price_decimal) = Decimal::from_f64(sol_price) {
                // 1. 计算 mc (市值,美元,格式化) / Calculate mc (market cap, USD, formatted)
                // 使用 token 的动态 initial_virtual_token 参数 / Use dynamic initial_virtual_token from token
                if let Some(token_reserve_decimal) = crate::curve_amm::CurveAMM::u64_to_token_decimal(token.initial_virtual_token) {
                    if let Some(token_value) = normalized_price.checked_mul(token_reserve_decimal) {
                        if let Some(mc_decimal) = token_value.checked_mul(sol_price_decimal) {
                            token.mc = Some(format!("{:.2}", mc_decimal));
                        } else {
                            tracing::warn!("MC 计算溢出 / MC calculation overflow for token: {}", token.mint_account);
                            token.mc = Some("overflow".to_string());
                        }
                    } else {
                        tracing::warn!("Token value 计算溢出 / Token value calculation overflow for token: {}", token.mint_account);
                        token.mc = Some("overflow".to_string());
                    }
                } else {
                    tracing::warn!("Token reserve decimal 转换失败 / Token reserve decimal conversion failed for token: {}", token.mint_account);
                    token.mc = Some("error".to_string());
                }

                // 2. 计算 usd_price (Token美元价格,大整数,保持10^23精度) / Calculate usd_price (Token USD price, big integer, 10^23 precision)
                let price_decimal = Decimal::from(price_u128);
                if let Some(usd_price_decimal) = price_decimal.checked_mul(sol_price_decimal) {
                    // 转换为字符串(整数形式,去掉小数部分) / Convert to string (integer form, remove decimal part)
                    token.usd_price = Some(usd_price_decimal.trunc().to_string());
                } else {
                    tracing::warn!("USD price 计算溢出 / USD price calculation overflow for token: {}", token.mint_account);
                    token.usd_price = Some("overflow".to_string());
                }
            }
        } else {
            tracing::warn!(
                "价格超出安全范围 / Price exceeds safe limit: token={}, price={}",
                token.mint_account, price_u128
            );
            token.mc = Some("too_high".to_string());
            token.usd_price = Some("too_high".to_string());
        }
    }
}

/// 为Token列表批量计算价格信息 / Calculate price info for token list in batch
///
/// # 参数 / Parameters
/// * `tokens` - Token列表 / Token list
/// * `sol_price` - 当前SOL的美元价格 / Current SOL price in USD
fn enrich_tokens_with_prices(tokens: &mut [crate::db::TokenDetail], sol_price: f64) {
    for token in tokens.iter_mut() {
        enrich_token_with_prices(token, sol_price);
    }
}

/// 生成缓存键 / Generate cache key
fn make_cache_key(sort_by: &SortBy, limit: usize) -> String {
    format!("{}:{}", serde_json::to_string(sort_by).unwrap_or_default(), limit)
}

/// 检查缓存是否有效 / Check if cache is valid
async fn get_cached_response(
    state: &TokenState,
    sort_by: &SortBy,
    limit: usize,
) -> Option<TokenListResponse> {
    let cache_key = make_cache_key(sort_by, limit);
    let cache = state.list_cache.read().await;

    if let Some(item) = cache.get(&cache_key) {
        let elapsed = SystemTime::now()
            .duration_since(item.cached_at)
            .unwrap_or(Duration::from_secs(u64::MAX));

        if elapsed.as_secs() < state.cache_ttl_secs {
            return Some(item.data.clone());
        }
    }

    None
}

/// 更新缓存 / Update cache
async fn set_cache(
    state: &TokenState,
    sort_by: &SortBy,
    limit: usize,
    data: TokenListResponse,
) {
    let cache_key = make_cache_key(sort_by, limit);
    let mut cache = state.list_cache.write().await;

    cache.insert(
        cache_key,
        TokenListCacheItem {
            data,
            cached_at: SystemTime::now(),
        },
    );
}

/// 处理本地tokens (all/latest 模式) / Handle local tokens (all/latest mode)
async fn handle_local_tokens(
    state: &TokenState,
    limit: usize,
) -> Result<TokenListResponse, (StatusCode, String)> {
    match state.token_storage.get_latest_tokens(limit, None) {
        Ok(mut tokens) => {
            // 过滤屏蔽的mint / Filter blocked mints
            let blocked_service = state.blocked_mints_service.clone();
            let mut filtered_tokens = Vec::new();
            for token in tokens {
                if !blocked_service.is_blocked(&token.mint_account).await {
                    filtered_tokens.push(token);
                }
            }
            tokens = filtered_tokens;

            // 获取SOL价格并计算价格信息 / Get SOL price and calculate price info
            let sol_price = state.price_service.get_price_sync();
            enrich_tokens_with_prices(&mut tokens, sol_price);

            // 附加完整的统计数据到 extras / Enrich all tokens with complete stats
            for token in &mut tokens {
                enrich_token_with_stats(state, token).await;
            }

            let total = tokens.len();
            Ok(TokenListResponse {
                tokens,
                total,
                next_cursor: None,
            })
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to query token list: {}", e),
        )),
    }
}

/// 处理liquid tokens (24h交易量排序,滚动窗口) / Handle liquid tokens (24h volume sort, rolling window)
async fn handle_liquid_tokens(
    state: &TokenState,
    limit: usize,
) -> Result<TokenListResponse, (StatusCode, String)> {
    // 1. 获取 Top 滚动24h Volume 列表 / Get top rolling 24h volume list
    let volume_result = state
        .volume_storage
        .get_top_rolling_volume(limit)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get rolling volume data: {}", e),
            )
        })?;

    // 2. 提取 mint 列表 / Extract mint list
    let mints: Vec<String> = volume_result.items.iter().map(|item| item.mint.clone()).collect();

    if mints.is_empty() {
        return Ok(TokenListResponse {
            tokens: Vec::new(),
            total: 0,
            next_cursor: None,
        });
    }

    // 3. 批量查询 token 详情 / Batch query token details
    let mut tokens = state
        .token_storage
        .get_tokens_by_mints(&mints)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to batch query tokens: {}", e),
            )
        })?;

    // 4. 过滤屏蔽的mint / Filter blocked mints
    let blocked_service = state.blocked_mints_service.clone();
    let mut filtered_tokens = Vec::new();
    for token in tokens {
        if !blocked_service.is_blocked(&token.mint_account).await {
            filtered_tokens.push(token);
        }
    }
    tokens = filtered_tokens;

    // 5. 获取SOL价格并计算价格信息 / Get SOL price and calculate price info
    let sol_price = state.price_service.get_price_sync();
    enrich_tokens_with_prices(&mut tokens, sol_price);

    // 6. 按原始顺序排序 (volume 从高到低) / Sort by original order (volume desc)
    // RocksDB 返回的可能是无序的,需要根据 mints 顺序重新排列
    // RocksDB might return unordered, need to reorder by mints
    let mint_index: std::collections::HashMap<String, usize> = mints
        .iter()
        .enumerate()
        .map(|(i, m)| (m.clone(), i))
        .collect();

    tokens.sort_by_key(|t| mint_index.get(&t.mint_account).copied().unwrap_or(usize::MAX));

    // 6. 附加完整的统计数据到 extras / Enrich all tokens with complete stats
    for token in &mut tokens {
        enrich_token_with_stats(state, token).await;
    }

    Ok(TokenListResponse {
        total: tokens.len(),
        tokens,
        next_cursor: None,
    })
}

/// 处理rising tokens (24h涨幅排序,滚动窗口) / Handle rising tokens (24h gain sort, rolling window)
async fn handle_rising_tokens(
    state: &TokenState,
    limit: usize,
) -> Result<TokenListResponse, (StatusCode, String)> {
    // 1. 获取 Top 滚动24h涨幅列表 / Get top rolling 24h change list
    let change_result = state
        .change_storage
        .get_top_rolling_change(limit)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get rolling change data: {}", e),
            )
        })?;

    // 2. 提取 mint 列表 / Extract mint list
    let mints: Vec<String> = change_result.items.iter().map(|item| item.mint.clone()).collect();

    if mints.is_empty() {
        return Ok(TokenListResponse {
            tokens: Vec::new(),
            total: 0,
            next_cursor: None,
        });
    }

    // 3. 批量查询 token 详情 / Batch query token details
    let mut tokens = state
        .token_storage
        .get_tokens_by_mints(&mints)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to batch query tokens: {}", e),
            )
        })?;

    // 4. 过滤屏蔽的mint / Filter blocked mints
    let blocked_service = state.blocked_mints_service.clone();
    let mut filtered_tokens = Vec::new();
    for token in tokens {
        if !blocked_service.is_blocked(&token.mint_account).await {
            filtered_tokens.push(token);
        }
    }
    tokens = filtered_tokens;

    // 5. 获取SOL价格并计算价格信息 / Get SOL price and calculate price info
    let sol_price = state.price_service.get_price_sync();
    enrich_tokens_with_prices(&mut tokens, sol_price);

    // 6. 按原始顺序排序 / Sort by original order
    let mint_index: std::collections::HashMap<String, usize> = mints
        .iter()
        .enumerate()
        .map(|(i, m)| (m.clone(), i))
        .collect();

    tokens.sort_by_key(|t| mint_index.get(&t.mint_account).copied().unwrap_or(usize::MAX));

    // 7. 附加完整的统计数据到 extras / Enrich all tokens with complete stats
    for token in &mut tokens {
        enrich_token_with_stats(state, token).await;
    }

    Ok(TokenListResponse {
        total: tokens.len(),
        tokens,
        next_cursor: None,
    })
}

/// 处理hottest tokens (24h绝对钱包数排序) / Handle hottest tokens (24h absolute markets sort)
async fn handle_hottest_tokens(
    state: &TokenState,
    limit: usize,
) -> Result<TokenListResponse, (StatusCode, String)> {
    // 1. 获取 Top MarketsAbs 列表 / Get top markets abs list
    let markets_result = state
        .markets_abs_storage
        .get_top_markets_abs(Period::TwentyFourHours, None, limit)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get markets abs data: {}", e),
            )
        })?;

    // 2. 提取 mint 列表 / Extract mint list
    let mints: Vec<String> = markets_result.items.iter().map(|item| item.mint.clone()).collect();

    if mints.is_empty() {
        return Ok(TokenListResponse {
            tokens: Vec::new(),
            total: 0,
            next_cursor: None,
        });
    }

    // 3. 批量查询 token 详情 / Batch query token details
    let mut tokens = state
        .token_storage
        .get_tokens_by_mints(&mints)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to batch query tokens: {}", e),
            )
        })?;

    // 4. 过滤屏蔽的mint / Filter blocked mints
    let blocked_service = state.blocked_mints_service.clone();
    let mut filtered_tokens = Vec::new();
    for token in tokens {
        if !blocked_service.is_blocked(&token.mint_account).await {
            filtered_tokens.push(token);
        }
    }
    tokens = filtered_tokens;

    // 5. 获取SOL价格并计算价格信息 / Get SOL price and calculate price info
    let sol_price = state.price_service.get_price_sync();
    enrich_tokens_with_prices(&mut tokens, sol_price);

    // 6. 按原始顺序排序 / Sort by original order
    let mint_index: std::collections::HashMap<String, usize> = mints
        .iter()
        .enumerate()
        .map(|(i, m)| (m.clone(), i))
        .collect();

    tokens.sort_by_key(|t| mint_index.get(&t.mint_account).copied().unwrap_or(usize::MAX));

    // 7. 附加完整的统计数据到 extras / Enrich all tokens with complete stats
    for token in &mut tokens {
        enrich_token_with_stats(state, token).await;
    }

    Ok(TokenListResponse {
        total: tokens.len(),
        tokens,
        next_cursor: None,
    })
}

// ============================================================================
// Token详情统计数据附加 / Token Detail Statistics Enrichment
// ============================================================================

/// 附加24小时统计数据到 token.extras / Enrich token with 24h statistics
async fn enrich_token_with_stats(state: &TokenState, token: &mut crate::db::TokenDetail) {
    let mint = &token.mint_account;
    let period = Period::TwentyFourHours;

    // 并发查询所有统计数据 / Query all stats concurrently
    // Volume 和 Change 使用滚动24h查询（替代固定桶）/ Volume and Change use rolling 24h queries (replacing fixed buckets)
    let (volume_result, change_result, markets_abs_result, order_summary_result, fee_24h_result, fee_all_result) = tokio::join!(
        query_rolling_volume_stats(state, mint),
        query_rolling_change_stats(state, mint),
        query_markets_abs_stats(state, mint, period),
        query_order_summary(state, mint),
        query_fee_24h(state, mint),
        query_fee_all(state, mint),
    );

    // 附加 Volume 数据 (滚动24h) / Attach volume data (rolling 24h)
    if let Some(volume_data) = volume_result {
        token.extras.insert(
            "volume_24h".to_string(),
            serde_json::json!(volume_data.volume.to_string()),
        );
        token.extras.insert(
            "volume_event_count".to_string(),
            serde_json::json!(volume_data.event_count),
        );
        token.extras.insert(
            "volume_last_update".to_string(),
            serde_json::json!(volume_data.last_update),
        );
    }

    // 附加 Change 数据 (滚动24h) / Attach change data (rolling 24h)
    if let Some(change_data) = change_result {
        token.extras.insert(
            "change_percent_24h".to_string(),
            serde_json::json!(change_data.change_percent.to_string()),
        );
        token.extras.insert(
            "change_open_price".to_string(),
            serde_json::json!(change_data.open_price.to_string()),
        );
        token.extras.insert(
            "change_close_price".to_string(),
            serde_json::json!(change_data.close_price.to_string()),
        );
    }

    // 附加 MarketsAbs 数据 / Attach markets abs data
    if let Some(markets_abs_data) = markets_abs_result {
        token.extras.insert(
            "markets_abs_cumulative".to_string(),
            serde_json::json!(markets_abs_data.cumulative_count),
        );
        token.extras.insert(
            "markets_abs_first_seen".to_string(),
            serde_json::json!(markets_abs_data.first_seen),
        );
    }

    // 附加订单汇总数据 / Attach order summary data
    if let Some((long_data, short_data)) = order_summary_result {
        // 做多 / Long
        token.extras.insert("long_margin_sol".into(), serde_json::json!(long_data.total_margin_sol.to_string()));
        token.extras.insert("long_token_locked".into(), serde_json::json!(long_data.total_lock_lp_token.to_string()));
        token.extras.insert("long_sol_borrowed".into(), serde_json::json!(long_data.total_borrow.to_string()));
        // 做空 / Short
        token.extras.insert("short_margin_sol".into(), serde_json::json!(short_data.total_margin_sol.to_string()));
        token.extras.insert("short_sol_locked".into(), serde_json::json!(short_data.total_position_asset.to_string()));
        token.extras.insert("short_token_borrowed".into(), serde_json::json!(short_data.total_borrow.to_string()));
    }

    // 附加24h手续费数据 / Attach 24h fee data
    if let Some(fee_data) = fee_24h_result {
        token.extras.insert("fee_swap_24h".into(), serde_json::json!(fee_data.swap_fee_total.to_string()));
        token.extras.insert("fee_borrow_24h".into(), serde_json::json!(fee_data.borrow_fee_total.to_string()));
        token.extras.insert("fee_liquidate_24h".into(), serde_json::json!(fee_data.liquidate_fee_total.to_string()));
        token.extras.insert("fee_total_24h".into(), serde_json::json!(fee_data.total_fee.to_string()));
        token.extras.insert("fee_event_count_24h".into(), serde_json::json!(fee_data.event_count));
    }

    // 附加全量累计手续费 / Attach all-time accumulated fee
    if let Some(fee_all_data) = fee_all_result {
        token.extras.insert("fee_total_all".into(), serde_json::json!(fee_all_data.total_fee.to_string()));
    }
}

/// 查询滚动24h Volume 统计 / Query rolling 24h volume statistics
async fn query_rolling_volume_stats(
    state: &TokenState,
    mint: &str,
) -> Option<crate::volume::RollingVolumeResponse> {
    use tracing::warn;

    match state.volume_storage.get_rolling_volume(mint) {
        Ok(resp) => {
            if resp.volume > 0.0 { Some(resp) } else { None }
        }
        Err(e) => {
            warn!("Failed to get rolling volume stats for {}: {}", mint, e);
            None
        }
    }
}

/// 查询滚动24h Change 统计 / Query rolling 24h change statistics
async fn query_rolling_change_stats(
    state: &TokenState,
    mint: &str,
) -> Option<crate::change::RollingChangeResponse> {
    use tracing::warn;

    match state.change_storage.get_rolling_change(mint) {
        Ok(resp) => {
            if resp.last_update > 0 { Some(resp) } else { None }
        }
        Err(e) => {
            warn!("Failed to get rolling change stats for {}: {}", mint, e);
            None
        }
    }
}

/// 查询 MarketsAbs 统计 / Query markets abs statistics
async fn query_markets_abs_stats(
    state: &TokenState,
    mint: &str,
    period: Period,
) -> Option<MarketsAbsStatsData> {
    use tracing::warn;

    match state.markets_abs_storage.get_token_markets_abs(mint, period, None) {
        Ok(resp) => Some(MarketsAbsStatsData {
            cumulative_count: resp.cumulative_count,
            first_seen: resp.first_seen,
        }),
        Err(e) => {
            warn!("Failed to get markets abs stats for {}: {}", mint, e);
            None
        }
    }
}

/// 查询订单汇总 / Query order summary
async fn query_order_summary(
    state: &TokenState,
    mint: &str,
) -> Option<(OrderSummaryData, OrderSummaryData)> {
    let storage = state.order_summary_storage.clone();
    let mint = mint.to_string();

    tokio::task::spawn_blocking(move || {
        let long_data = storage.get(&mint, "dn").ok()?;
        let short_data = storage.get(&mint, "up").ok()?;
        Some((long_data, short_data))
    })
    .await
    .ok()?
}

/// 查询24h手续费统计 / Query 24h fee statistics
async fn query_fee_24h(
    state: &TokenState,
    mint: &str,
) -> Option<crate::fee::FeeData> {
    use tracing::warn;

    match state.fee_storage.get_fee_24h(mint) {
        Ok(data) => {
            if data.total_fee > 0 { Some(data) } else { None }
        }
        Err(e) => {
            warn!("Failed to get 24h fee stats for {}: {}", mint, e);
            None
        }
    }
}

/// 查询全量累计手续费 / Query all-time accumulated fee
async fn query_fee_all(
    state: &TokenState,
    mint: &str,
) -> Option<crate::fee::FeeData> {
    use tracing::warn;

    match state.fee_storage.get_fee_all(mint) {
        Ok(data) => {
            if data.total_fee > 0 { Some(data) } else { None }
        }
        Err(e) => {
            warn!("Failed to get all-time fee stats for {}: {}", mint, e);
            None
        }
    }
}

/// MarketsAbs 统计数据辅助结构 / MarketsAbs statistics helper struct
struct MarketsAbsStatsData {
    cumulative_count: u64,
    first_seen: u64,
}

// ============================================================================
// 路由定义 / Route Definitions
// ============================================================================

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
