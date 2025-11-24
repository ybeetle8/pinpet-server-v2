// OrderBook 查询接口 / OrderBook query endpoints
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};
use utoipa::{IntoParams, ToSchema};

use crate::db::OrderBookStorage;
use crate::orderbook::MarginOrder;
use crate::util::result::CommonResult;

/// 创建 OrderBook 路由 / Create OrderBook routes
pub fn routes() -> Router<Arc<OrderBookStorage>> {
    Router::new()
        .route("/api/orderbook/:mint/:direction", get(query_orderbook))
}

/// OrderBook 查询参数 / OrderBook query parameters
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct OrderBookQueryParams {
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

/// OrderBook Header 简化信息 / OrderBook Header simplified info
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderBookHeaderInfo {
    /// 版本号 / Version number
    pub version: u8,

    /// 订单类型(1=做多/down, 2=做空/up) / Order type (1=long/down, 2=short/up)
    pub order_type: u8,

    /// 协议管理员 / Authority
    pub authority: String,

    /// 订单 ID 计数器 / Order ID counter
    pub order_id_counter: u64,

    /// 账本创建时间戳(Unix timestamp,秒) / Created timestamp (Unix timestamp, seconds)
    pub created_at: u32,

    /// 最后修改时间戳(Unix timestamp,秒) / Last modified timestamp (Unix timestamp, seconds)
    pub last_modified: u32,

    /// 总容量(最大槽位数限制) / Total capacity (maximum slot count limit)
    pub total_capacity: u32,

    /// 链表头索引(第一个订单) / Head index (first order)
    pub head: u16,

    /// 链表尾索引(最后一个订单) / Tail index (last order)
    pub tail: u16,

    /// 当前订单总数 / Current order count
    pub total: u16,
}

/// OrderBook 订单详情(包含索引) / OrderBook order detail (with index)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderBookOrderDetail {
    /// 订单在链表中的索引 / Order index in the linked list
    pub index: u16,

    /// 订单数据 / Order data
    #[serde(flatten)]
    pub order: MarginOrder,
}

/// OrderBook 查询响应 / OrderBook query response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OrderBookQueryResponse {
    /// OrderBook Header 信息 / OrderBook header info
    pub header: OrderBookHeaderInfo,

    /// 订单列表 / Order list
    pub orders: Vec<OrderBookOrderDetail>,

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

/// 查询 OrderBook 数据 / Query OrderBook data
///
/// 根据 mint 地址和方向查询 OrderBook 中的所有订单
/// Query all orders in OrderBook by mint address and direction
///
/// # 参数 / Parameters
/// - `mint`: Token mint 地址 / Token mint address
/// - `direction`: 订单方向,可选值: "up"(做空) 或 "dn"(做多) / Order direction: "up"(short) or "dn"(long)
/// - `page`: 页码(从 1 开始,默认 1) / Page number (starting from 1, default 1)
/// - `page_size`: 每页数量(默认 100,最大受配置限制) / Page size (default 100, max limited by config)
///
/// # 返回值 / Returns
/// 返回 OrderBook header 信息和订单列表
/// Returns OrderBook header info and order list
#[utoipa::path(
    get,
    path = "/api/orderbook/{mint}/{direction}",
    params(
        ("mint" = String, Path, description = "Token mint 地址 / Token mint address"),
        ("direction" = String, Path, description = "订单方向: up(做空) 或 dn(做多) / Order direction: up(short) or dn(long)"),
        OrderBookQueryParams
    ),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = OrderBookQueryResponse),
        (status = 404, description = "OrderBook 不存在 / OrderBook not found"),
        (status = 500, description = "服务器错误 / Server error")
    ),
    tag = "OrderBook"
)]
pub async fn query_orderbook(
    Path((mint, direction)): Path<(String, String)>,
    Query(params): Query<OrderBookQueryParams>,
    State(orderbook_storage): State<Arc<OrderBookStorage>>,
) -> Result<Json<CommonResult<OrderBookQueryResponse>>, (StatusCode, String)> {
    info!(
        "📊 查询 OrderBook / Query OrderBook: mint={}, direction={}, page={}, page_size={}",
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
        params.page_size
    };

    // 获取 OrderBook 管理器 / Get OrderBook manager
    let manager = match orderbook_storage.get_or_create_manager(mint.clone(), direction.clone()) {
        Ok(m) => m,
        Err(e) => {
            error!("❌ 获取 OrderBook 管理器失败 / Failed to get OrderBook manager: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get OrderBook manager: {}", e),
            ));
        }
    };

    // 加载 OrderBook header / Load OrderBook header
    let header = match manager.load_header() {
        Ok(h) => h,
        Err(e) => {
            error!("❌ 加载 OrderBook header 失败 / Failed to load OrderBook header: {}", e);
            return Err((
                StatusCode::NOT_FOUND,
                format!("OrderBook not found: {}:{}", mint, direction),
            ));
        }
    };

    // 构造 header 响应 / Construct header response
    let header_info = OrderBookHeaderInfo {
        version: header.version,
        order_type: header.order_type,
        authority: header.authority.clone(),
        order_id_counter: header.order_id_counter,
        created_at: header.created_at,
        last_modified: header.last_modified,
        total_capacity: header.total_capacity,
        head: header.head,
        tail: header.tail,
        total: header.total,
    };

    // 计算分页 / Calculate pagination
    let total_count = header.total;
    let total_pages = if total_count == 0 {
        0
    } else {
        ((total_count as usize + page_size - 1) / page_size).max(1)
    };

    // 如果链表为空,直接返回 / If linked list is empty, return directly
    if total_count == 0 {
        info!("ℹ️ OrderBook 为空 / OrderBook is empty");
        return Ok(Json(CommonResult::ok(OrderBookQueryResponse {
            header: header_info,
            orders: vec![],
            total_count: 0,
            returned_count: 0,
            page,
            page_size,
            total_pages: 0,
        })));
    }

    // 计算起始位置 / Calculate start position
    let skip = (page - 1) * page_size;

    // 收集订单 / Collect orders
    let mut orders = Vec::new();
    let mut current_index = 0;
    let mut collected = 0;

    // 使用 traverse 方法遍历链表 / Use traverse method to iterate linked list
    let traverse_result = manager.traverse(
        u16::MAX, // 从 head 开始 / Start from head
        0,        // 不限制遍历数量 / No limit
        |index, order| {
            // 跳过前面的记录 / Skip previous records
            if current_index < skip {
                current_index += 1;
                return Ok(true); // 继续遍历 / Continue
            }

            // 已收集足够的记录 / Collected enough records
            if collected >= page_size {
                return Ok(false); // 停止遍历 / Stop
            }

            // 收集当前记录 / Collect current record
            orders.push(OrderBookOrderDetail {
                index,
                order: order.clone(),
            });
            collected += 1;
            current_index += 1;

            Ok(true) // 继续遍历 / Continue
        },
    );

    // 检查遍历结果 / Check traverse result
    if let Err(e) = traverse_result {
        error!("❌ 遍历 OrderBook 失败 / Failed to traverse OrderBook: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to traverse OrderBook: {}", e),
        ));
    }

    let returned_count = orders.len();

    info!(
        "✅ 查询成功 / Query successful: mint={}, direction={}, total={}, returned={}, page={}/{}",
        &mint[..8.min(mint.len())], direction, total_count, returned_count, page, total_pages
    );

    Ok(Json(CommonResult::ok(OrderBookQueryResponse {
        header: header_info,
        orders,
        total_count,
        returned_count,
        page,
        page_size,
        total_pages,
    })))
}
