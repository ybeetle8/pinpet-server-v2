// Debug 路由模块 / Debug route module 
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};
use utoipa::{IntoParams, ToSchema};

use crate::config::Config;
use crate::db::{OrderBookStorage, TokenStorage};
use crate::orderbook::MarginOrder;
use crate::orderbook_sync::{OrderBookSyncService, SyncResult};
use crate::order_summary::{OrderSummaryStorage, RebuildResult};
use crate::solana::{OrderBookReader, OrderBookComparator, ComparisonResult, SolanaClient};
use crate::util::result::CommonResult;

/// Debug 状态 / Debug state
#[derive(Clone)]
pub struct DebugState {
    pub config: Arc<Config>,
    pub solana_client: SolanaClient,
    pub orderbook_storage: Arc<OrderBookStorage>,
    pub sync_service: Option<Arc<OrderBookSyncService>>,
    pub order_summary_storage: Arc<OrderSummaryStorage>,
    pub token_storage: Arc<TokenStorage>,
}

/// 创建 Debug 路由 / Create debug routes
pub fn routes() -> Router<DebugState> {
    Router::new()
        .route("/api/debug/orderbook/:mint/:direction/chain", get(query_orderbook_from_chain))
        .route("/api/debug/orderbook/:mint/compare", get(compare_orderbook))
        .route("/api/debug/orderbook/:mint/sync", post(trigger_manual_sync))
        .route("/api/debug/order-summary/rebuild", post(rebuild_order_summary))
}

/// OrderBook 查询参数 / OrderBook query parameters
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct ChainOrderBookQueryParams {
    /// 页码(从 1 开始) / Page number (starting from 1)
    #[serde(default = "default_page")]
    pub page: usize,

    /// 每页数量(默认 100) / Page size (default 100)
    #[serde(default = "default_page_size")]
    pub page_size: usize,
}

fn default_page() -> usize {
    1
}

fn default_page_size() -> usize {
    100
}

/// 同步参数 / Sync parameters
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct SyncParams {
    /// 是否强制同步（忽略 fully_matched 检查）/ Force sync (ignore fully_matched check)
    #[serde(default)]
    pub force: bool,
}

/// 链上 OrderBook Header 信息 / On-chain OrderBook Header info
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChainOrderBookHeaderInfo {
    /// 版本号 / Version number
    pub version: u8,

    /// 订单类型(1=做多/down, 2=做空/up) / Order type (1=long/down, 2=short/up)
    pub order_type: u8,

    /// 协议管理员 / Authority
    pub authority: String,

    /// 订单 ID 计数器 / Order ID counter
    pub order_id_counter: u64,

    /// 账本创建时间戳(Unix timestamp,秒) / Created timestamp (Unix timestamp, seconds)
    pub created_at: i64,

    /// 最后修改时间戳(Unix timestamp,秒) / Last modified timestamp (Unix timestamp, seconds)
    pub last_modified: i64,

    /// 总容量(最大槽位数限制) / Total capacity (maximum slot count limit)
    pub total_capacity: u32,

    /// 链表头索引(第一个订单) / Head index (first order)
    pub head: u16,

    /// 链表尾索引(最后一个订单) / Tail index (last order)
    pub tail: u16,

    /// 当前订单总数 / Current order count
    pub total: u16,

    /// PDA bump / PDA bump
    pub bump: u8,
}

/// 链上 OrderBook 订单详情(包含索引) / On-chain OrderBook order detail (with index)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChainOrderBookOrderDetail {
    /// 订单在链表中的索引 / Order index in the linked list
    pub index: u16,

    /// 订单数据 / Order data
    #[serde(flatten)]
    pub order: MarginOrder,
}

/// 链上 OrderBook 查询响应 / On-chain OrderBook query response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ChainOrderBookQueryResponse {
    /// 数据来源 / Data source
    pub data_source: String,

    /// OrderBook PDA 地址 / OrderBook PDA address
    pub pda_address: String,

    /// OrderBook Header 信息 / OrderBook header info
    pub header: ChainOrderBookHeaderInfo,

    /// 订单列表 / Order list
    pub orders: Vec<ChainOrderBookOrderDetail>,

    /// 总订单数 / Total order count
    pub total_count: u16,

    /// 当前页返回的订单数 / Returned order count in current page
    pub returned_count: usize,

    /// 当前页码 / Current page
    pub page: usize,

    /// 每页数量 / Page size
    pub page_size: usize,

    /// 总页数 / Total pages
    pub total_pages: usize,
}

/// 从链上查询 OrderBook 数据 / Query OrderBook data from chain
///
/// 直接从 Solana 链上读取 OrderBook PDA 数据
/// Read OrderBook PDA data directly from Solana chain
///
/// # 参数 / Parameters
/// - `mint`: Token mint 地址 / Token mint address
/// - `direction`: 订单方向,可选值: "up"(做空) 或 "dn"(做多) / Order direction: "up"(short) or "dn"(long)
/// - `page`: 页码(从 1 开始,默认 1) / Page number (starting from 1, default 1)
/// - `page_size`: 每页数量(默认 100) / Page size (default 100)
///
/// # 返回值 / Returns
/// 返回从链上读取的 OrderBook header 信息和订单列表
/// Returns OrderBook header info and order list read from chain
#[utoipa::path(
    get,
    path = "/api/debug/orderbook/{mint}/{direction}/chain",
    params(
        ("mint" = String, Path, description = "Token mint 地址 / Token mint address"),
        ("direction" = String, Path, description = "订单方向: up(做空) 或 dn(做多) / Order direction: up(short) or dn(long)"),
        ChainOrderBookQueryParams
    ),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = ChainOrderBookQueryResponse),
        (status = 404, description = "OrderBook 不存在 / OrderBook not found"),
        (status = 500, description = "服务器错误 / Server error")
    ),
    tag = "Debug"
)]
pub async fn query_orderbook_from_chain(
    Path((mint, direction)): Path<(String, String)>,
    Query(params): Query<ChainOrderBookQueryParams>,
    State(state): State<DebugState>,
) -> Result<Json<CommonResult<ChainOrderBookQueryResponse>>, (StatusCode, String)> {
    info!(
        "📊 [DEBUG] 从链上查询 OrderBook / Query OrderBook from chain: mint={}, direction={}, page={}, page_size={}",
        &mint[..8.min(mint.len())], direction, params.page, params.page_size
    );

    // 验证 direction 参数 / Validate direction parameter
    if direction != "up" && direction != "dn" {
        error!("❌ 无效的 direction 参数 / Invalid direction parameter: {}", direction);
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Invalid direction: {}, expected 'up' or 'dn'", direction),
        ));
    }

    // 验证分页参数 / Validate pagination parameters
    let page = if params.page < 1 { 1 } else { params.page };
    let page_size = if params.page_size < 1 {
        100
    } else {
        params.page_size.min(1000)
    };

    // 从配置获取 program ID / Get program ID from config
    let program_id = state.config.solana.program_id.clone();

    // 创建 OrderBook 读取器 / Create OrderBook reader
    let reader = match OrderBookReader::new(state.solana_client.clone(), program_id) {
        Ok(r) => r,
        Err(e) => {
            error!("❌ 创建 OrderBook 读取器失败 / Failed to create OrderBook reader: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create OrderBook reader: {}", e),
            ));
        }
    };

    // 获取 PDA 地址 / Get PDA address
    let pda_address = match reader.get_orderbook_pda(&mint, &direction) {
        Ok(pda) => pda.to_string(),
        Err(e) => {
            error!("❌ 计算 PDA 地址失败 / Failed to calculate PDA address: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to calculate PDA address: {}", e),
            ));
        }
    };

    // 从链上获取数据 / Get data from chain
    let (header, orders) = match reader.get_orderbook_from_chain(&mint, &direction).await {
        Ok(data) => data,
        Err(e) => {
            error!("❌ 从链上获取 OrderBook 失败 / Failed to get OrderBook from chain: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get OrderBook from chain: {}", e),
            ));
        }
    };

    // 构造 header 响应 / Construct header response
    let header_info = ChainOrderBookHeaderInfo {
        version: header.version,
        order_type: header.order_type,
        authority: header.authority.to_string(),
        order_id_counter: header.order_id_counter,
        created_at: header.created_at,
        last_modified: header.last_modified,
        total_capacity: header.total_capacity,
        head: header.head,
        tail: header.tail,
        total: header.total,
        bump: header.bump,
    };

    // 计算分页 / Calculate pagination
    let total_count = orders.len() as u16;
    let total_pages = if total_count == 0 {
        0
    } else {
        ((total_count as usize + page_size - 1) / page_size).max(1)
    };

    // 分页处理订单 / Paginate orders
    let skip = (page - 1) * page_size;
    let paged_orders: Vec<ChainOrderBookOrderDetail> = orders
        .into_iter()
        .skip(skip)
        .take(page_size)
        .map(|(index, order)| ChainOrderBookOrderDetail {
            index,
            order,
        })
        .collect();

    let returned_count = paged_orders.len();

    info!(
        "✅ [DEBUG] 从链上查询成功 / Query from chain successful: mint={}, direction={}, total={}, returned={}, page={}/{}",
        &mint[..8.min(mint.len())], direction, total_count, returned_count, page, total_pages
    );

    Ok(Json(CommonResult::ok(ChainOrderBookQueryResponse {
        data_source: "chain".to_string(),
        pda_address,
        header: header_info,
        orders: paged_orders,
        total_count,
        returned_count,
        page,
        page_size,
        total_pages,
    })))
}

/// 对比链上和数据库的 OrderBook / Compare OrderBook between chain and database
///
/// 对比 mint 对应的 up 和 dn 两个方向的 OrderBook
/// Compare both up and dn OrderBooks for the given mint
///
/// # 参数 / Parameters
/// - `mint`: Token mint 地址 / Token mint address
///
/// # 返回值 / Returns
/// 返回详细的对比结果，包括差异字段
/// Returns detailed comparison result including field differences
#[utoipa::path(
    get,
    path = "/api/debug/orderbook/{mint}/compare",
    params(
        ("mint" = String, Path, description = "Token mint 地址 / Token mint address"),
    ),
    responses(
        (status = 200, description = "对比成功 / Comparison successful", body = ComparisonResult),
        (status = 500, description = "服务器错误 / Server error")
    ),
    tag = "Debug"
)]
pub async fn compare_orderbook(
    Path(mint): Path<String>,
    State(state): State<DebugState>,
) -> Result<Json<CommonResult<ComparisonResult>>, (StatusCode, String)> {
    info!(
        "📊 [DEBUG] 对比 OrderBook / Comparing OrderBook: mint={}",
        &mint[..8.min(mint.len())]
    );

    // 从配置获取 program ID / Get program ID from config
    let program_id = state.config.solana.program_id.clone();

    // 创建 OrderBook 读取器 / Create OrderBook reader
    let reader = match OrderBookReader::new(state.solana_client.clone(), program_id) {
        Ok(r) => r,
        Err(e) => {
            error!("❌ 创建 OrderBook 读取器失败 / Failed to create OrderBook reader: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create OrderBook reader: {}", e),
            ));
        }
    };

    // 创建对比器 / Create comparator
    let comparator = OrderBookComparator::new(reader, state.orderbook_storage.clone());

    // 执行对比 / Execute comparison
    let comparison_result = match comparator.compare(&mint).await {
        Ok(result) => result,
        Err(e) => {
            error!("❌ 对比失败 / Comparison failed: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Comparison failed: {}", e),
            ));
        }
    };

    // 输出对比结果摘要 / Output comparison summary
    if comparison_result.fully_matched {
        info!(
            "✅ [DEBUG] OrderBook 完全匹配 / OrderBook fully matched: mint={}",
            &mint[..8.min(mint.len())]
        );
    } else {
        warn!(
            "⚠️ [DEBUG] OrderBook 存在差异 / OrderBook has differences: mint={}",
            &mint[..8.min(mint.len())]
        );

        // 输出差异摘要 / Output difference summary
        info!(
            "📊 UP OrderBook: chain_total={}, db_total={}, matching={}, mismatching={}, chain_only={}, db_only={}",
            comparison_result.up_orderbook.chain_total,
            comparison_result.up_orderbook.db_total,
            comparison_result.up_orderbook.matching_orders,
            comparison_result.up_orderbook.mismatching_orders,
            comparison_result.up_orderbook.chain_only_orders.len(),
            comparison_result.up_orderbook.db_only_orders.len()
        );

        info!(
            "📊 DOWN OrderBook: chain_total={}, db_total={}, matching={}, mismatching={}, chain_only={}, db_only={}",
            comparison_result.down_orderbook.chain_total,
            comparison_result.down_orderbook.db_total,
            comparison_result.down_orderbook.matching_orders,
            comparison_result.down_orderbook.mismatching_orders,
            comparison_result.down_orderbook.chain_only_orders.len(),
            comparison_result.down_orderbook.db_only_orders.len()
        );
    }

    Ok(Json(CommonResult::ok(comparison_result)))
}

/// 手动触发 OrderBook 同步 / Manually trigger OrderBook sync
///
/// 立即对指定 mint 的 OrderBook 进行同步和修复
/// Immediately sync and repair OrderBook for the specified mint
///
/// # 参数 / Parameters
/// - `mint`: Token mint 地址 / Token mint address
/// - `force`: 是否强制同步（忽略 fully_matched 检查）/ Force sync (ignore fully_matched check)
///
/// # 返回值 / Returns
/// 返回同步结果，包括是否完全匹配、修复的记录数等
/// Returns sync result including whether fully matched, repaired count, etc.
#[utoipa::path(
    post,
    path = "/api/debug/orderbook/{mint}/sync",
    params(
        ("mint" = String, Path, description = "Token mint 地址 / Token mint address"),
        SyncParams
    ),
    responses(
        (status = 200, description = "同步成功 / Sync successful", body = SyncResult),
        (status = 503, description = "同步服务未启用 / Sync service not enabled"),
        (status = 500, description = "服务器错误 / Server error")
    ),
    tag = "Debug"
)]
pub async fn trigger_manual_sync(
    Path(mint): Path<String>,
    Query(params): Query<SyncParams>,
    State(state): State<DebugState>,
) -> Result<Json<CommonResult<SyncResult>>, (StatusCode, String)> {
    info!(
        "🔄 [DEBUG] 手动触发同步 / Manual sync triggered: mint={}, force={}",
        &mint[..8.min(mint.len())],
        params.force
    );

    // 检查同步服务是否可用 / Check if sync service is available
    let sync_service = match state.sync_service {
        Some(ref service) => service,
        None => {
            error!("❌ 同步服务未启用 / Sync service not enabled");
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "Sync service is not enabled. Please enable orderbook_sync in config.toml".to_string(),
            ));
        }
    };

    // 执行同步 / Execute sync (根据 force 参数选择不同的同步方法)
    let result = if params.force {
        match sync_service.force_sync_orderbook(&mint).await {
            Ok(result) => result,
            Err(e) => {
                error!("❌ 强制同步失败 / Force sync failed: {}", e);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Force sync failed: {}", e),
                ));
            }
        }
    } else {
        match sync_service.sync_orderbook(&mint).await {
            Ok(result) => result,
            Err(e) => {
                error!("❌ 同步失败 / Sync failed: {}", e);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Sync failed: {}", e),
                ));
            }
        }
    };

    // 输出同步结果 / Output sync result
    if result.fully_matched {
        info!(
            "✅ [DEBUG] 同步完成，数据完全匹配 / Sync completed, data fully matched: mint={}",
            &mint[..8.min(mint.len())]
        );
    } else {
        warn!(
            "⚠️ [DEBUG] 同步完成，发现并修复了 {} 处差异 / Sync completed, found and repaired {} differences: mint={}",
            result.repaired_count, result.repaired_count, &mint[..8.min(mint.len())]
        );
    }

    if !result.errors.is_empty() {
        warn!(
            "⚠️ [DEBUG] 同步过程中出现错误 / Errors during sync: {:?}",
            result.errors
        );
    }

    Ok(Json(CommonResult::ok(result)))
}

/// 订单汇总重建参数 / Order summary rebuild parameters
#[derive(Debug, Deserialize, IntoParams)]
pub struct RebuildOrderSummaryParams {
    /// 可选, 不传则重建所有 mint / Optional, rebuild all mints if not provided
    pub mint: Option<String>,
}

/// 订单汇总重建响应 / Order summary rebuild response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RebuildOrderSummaryResponse {
    /// 重建的 mint 数量 / Number of rebuilt mints
    pub rebuilt_count: usize,
    /// 重建结果 / Rebuild results
    pub results: Vec<RebuildResult>,
}

/// 重建订单汇总数据 / Rebuild order summary data
///
/// 从 OrderBook 遍历重建指定 mint 或所有 mint 的订单汇总统计
/// Rebuild order summary statistics from OrderBook traversal for specific or all mints
///
/// # 参数 / Parameters
/// - `mint`: 可选的 Token mint 地址,不传则重建所有 / Optional Token mint address, rebuild all if not provided
///
/// # 返回值 / Returns
/// 返回重建结果 / Returns rebuild results
#[utoipa::path(
    post,
    path = "/api/debug/order-summary/rebuild",
    params(RebuildOrderSummaryParams),
    responses(
        (status = 200, description = "重建成功 / Rebuild successful", body = RebuildOrderSummaryResponse),
        (status = 500, description = "服务器错误 / Server error")
    ),
    tag = "Debug"
)]
pub async fn rebuild_order_summary(
    State(state): State<DebugState>,
    Query(params): Query<RebuildOrderSummaryParams>,
) -> Result<Json<CommonResult<RebuildOrderSummaryResponse>>, (StatusCode, String)> {
    info!(
        "🔄 [DEBUG] 重建订单汇总 / Rebuilding order summary: mint={:?}",
        params.mint
    );

    let order_summary_storage = state.order_summary_storage.clone();
    let orderbook_storage = state.orderbook_storage.clone();
    let token_storage = state.token_storage.clone();
    let mint_filter = params.mint.clone();

    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<RebuildOrderSummaryResponse> {
        let mints = if let Some(mint) = mint_filter {
            vec![mint]
        } else {
            // 获取所有 mint 地址 / Get all mint addresses
            token_storage.get_all_mint_addresses()?
        };

        let mut results = Vec::new();
        for mint in &mints {
            let long_data = order_summary_storage.rebuild_from_orderbook(mint, "dn", &orderbook_storage)?;
            let short_data = order_summary_storage.rebuild_from_orderbook(mint, "up", &orderbook_storage)?;
            results.push(RebuildResult {
                mint: mint.clone(),
                long: long_data,
                short: short_data,
            });
        }

        Ok(RebuildOrderSummaryResponse {
            rebuilt_count: results.len(),
            results,
        })
    })
    .await
    .map_err(|e| {
        error!("❌ 重建订单汇总失败 / Failed to rebuild order summary: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Rebuild failed: {}", e))
    })?
    .map_err(|e| {
        error!("❌ 重建订单汇总失败 / Failed to rebuild order summary: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Rebuild failed: {}", e))
    })?;

    info!(
        "✅ [DEBUG] 订单汇总重建完成 / Order summary rebuild completed: rebuilt_count={}",
        result.rebuilt_count
    );

    Ok(Json(CommonResult::ok(result)))
}