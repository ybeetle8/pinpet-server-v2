use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::util::{ok_result, ApiResult};
use crate::db::DatabaseStats;
use crate::solana::events::PinpetEvent;

/// 数据库操作请求
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(title = "DbRequest", description = "数据库操作请求")]
pub struct DbRequest {
    /// 键
    #[schema(example = "test_key")]
    pub key: String,

    /// 值 (可选，用于写入操作)
    #[schema(example = "test_value")]
    pub value: Option<String>,
}

/// 数据库响应
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(title = "DbResponse", description = "数据库响应")]
pub struct DbResponse {
    /// 键
    #[schema(example = "test_key")]
    pub key: String,

    /// 值
    #[schema(example = "test_value")]
    pub value: Option<String>,
}

/// 排序方向 / Sort order
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq)]
pub enum SortOrder {
    /// 升序（slot从小到大）/ Ascending (slot from low to high)
    #[serde(rename = "asc")]
    Asc,
    /// 降序（slot从大到小）/ Descending (slot from high to low)
    #[serde(rename = "desc")]
    Desc,
}

impl Default for SortOrder {
    fn default() -> Self {
        SortOrder::Desc
    }
}

/// 按 Mint 查询请求参数 / Query by mint request parameters
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct QueryByMintParams {
    /// 代币 mint account 地址 / Token mint account address
    #[param(example = "So11111111111111111111111111111111111111112")]
    pub mint: String,
    /// 页码（从1开始）/ Page number (starts from 1)
    #[param(example = 1, minimum = 1)]
    #[serde(default = "default_page")]
    pub page: u32,
    /// 每页数量 / Page size
    #[param(example = 20, minimum = 1, maximum = 100)]
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    /// 排序方向 / Sort order
    #[param(example = "desc")]
    #[serde(default)]
    pub sort: SortOrder,
}

/// 按 User 查询请求参数 / Query by user request parameters
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct QueryByUserParams {
    /// 用户钱包地址 / User wallet address
    #[param(example = "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU")]
    pub user: String,
    /// 可选的 mint 过滤 / Optional mint filter
    #[param(example = "So11111111111111111111111111111111111111112")]
    pub mint: Option<String>,
    /// 页码（从1开始）/ Page number (starts from 1)
    #[param(example = 1, minimum = 1)]
    #[serde(default = "default_page")]
    pub page: u32,
    /// 每页数量 / Page size
    #[param(example = 20, minimum = 1, maximum = 100)]
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    /// 排序方向 / Sort order
    #[param(example = "desc")]
    #[serde(default)]
    pub sort: SortOrder,
}

/// 按 Signature 查询请求参数 / Query by signature request parameters
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct QueryBySignatureParams {
    /// 交易签名 / Transaction signature
    #[param(example = "5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW")]
    pub signature: String,
}

/// 分页事件响应 / Paginated event response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(title = "PaginatedEvents", description = "分页事件响应")]
pub struct PaginatedEvents {
    /// 事件列表 / Event list
    pub events: Vec<PinpetEvent>,
    /// 总数量 / Total count
    #[schema(example = 100)]
    pub total: u64,
    /// 当前页码 / Current page
    #[schema(example = 1)]
    pub page: u32,
    /// 每页数量 / Page size
    #[schema(example = 20)]
    pub page_size: u32,
    /// 总页数 / Total pages
    #[schema(example = 5)]
    pub total_pages: u32,
}

/// 事件列表响应 / Event list response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(title = "EventList", description = "事件列表响应")]
pub struct EventList {
    /// 事件列表 / Event list
    pub events: Vec<PinpetEvent>,
}

fn default_page() -> u32 { 1 }
fn default_page_size() -> u32 { 20 }

/// 写入数据到 RocksDB
#[utoipa::path(
    post,
    path = "/db/put",
    tag = "database",
    summary = "写入数据",
    description = "向 RocksDB 写入键值对",
    request_body = DbRequest,
    responses(
        (status = 200, description = "写入成功",
         body = crate::docs::ApiResponse<DbResponse>),
        (status = 500, description = "服务器内部错误",
         body = crate::docs::ErrorApiResponse,
         example = json!({
             "code": 500,
             "msg": "Internal Server Error",
             "data": null
         })
        )
    )
)]
pub async fn db_put(
    State(db): State<std::sync::Arc<crate::db::RocksDbStorage>>,
    Json(req): Json<DbRequest>,
) -> ApiResult {
    let result = db.put(&req.key, req.value.as_deref().unwrap_or(""));

    match result {
        Ok(_) => Ok(ok_result::<DbResponse>(Ok(DbResponse {
            key: req.key.clone(),
            value: req.value,
        }))),
        Err(e) => Ok(ok_result::<DbResponse>(Err(
            crate::util::result::ApiError::InternalError(e.to_string()),
        ))),
    }
}

/// 从 RocksDB 读取数据
#[utoipa::path(
    post,
    path = "/db/get",
    tag = "database",
    summary = "读取数据",
    description = "从 RocksDB 读取键对应的值",
    request_body = DbRequest,
    responses(
        (status = 200, description = "读取成功",
         body = crate::docs::ApiResponse<DbResponse>),
        (status = 404, description = "未找到",
         body = crate::docs::ErrorApiResponse,
         example = json!({
             "code": 404,
             "msg": "Key not found",
             "data": null
         })
        ),
        (status = 500, description = "服务器内部错误",
         body = crate::docs::ErrorApiResponse)
    )
)]
pub async fn db_get(
    State(db): State<std::sync::Arc<crate::db::RocksDbStorage>>,
    Json(req): Json<DbRequest>,
) -> ApiResult {
    let result = db.get(&req.key);

    match result {
        Ok(value) => Ok(ok_result::<DbResponse>(Ok(DbResponse {
            key: req.key.clone(),
            value,
        }))),
        Err(e) => Ok(ok_result::<DbResponse>(Err(
            crate::util::result::ApiError::InternalError(e.to_string()),
        ))),
    }
}

/// 从 RocksDB 删除数据
#[utoipa::path(
    post,
    path = "/db/delete",
    tag = "database",
    summary = "删除数据",
    description = "从 RocksDB 删除键值对",
    request_body = DbRequest,
    responses(
        (status = 200, description = "删除成功",
         body = crate::docs::ApiResponse<DbResponse>),
        (status = 500, description = "服务器内部错误",
         body = crate::docs::ErrorApiResponse)
    )
)]
pub async fn db_delete(
    State(db): State<std::sync::Arc<crate::db::RocksDbStorage>>,
    Json(req): Json<DbRequest>,
) -> ApiResult {
    let result = db.delete(&req.key);

    match result {
        Ok(_) => Ok(ok_result::<DbResponse>(Ok(DbResponse {
            key: req.key.clone(),
            value: None,
        }))),
        Err(e) => Ok(ok_result::<DbResponse>(Err(
            crate::util::result::ApiError::InternalError(e.to_string()),
        ))),
    }
}

/// 获取 RocksDB 统计信息
#[utoipa::path(
    get,
    path = "/db/stats",
    tag = "database",
    summary = "获取统计信息",
    description = "获取 RocksDB 的统计信息",
    responses(
        (status = 200, description = "获取成功",
         body = crate::docs::ApiResponse<String>),
        (status = 500, description = "服务器内部错误",
         body = crate::docs::ErrorApiResponse)
    )
)]
pub async fn db_stats(State(db): State<std::sync::Arc<crate::db::RocksDbStorage>>) -> ApiResult {
    let result = db.get_stats();

    match result {
        Ok(stats) => Ok(ok_result::<String>(Ok(stats))),
        Err(e) => Ok(ok_result::<String>(Err(
            crate::util::result::ApiError::InternalError(e.to_string()),
        ))),
    }
}

/// 获取数据库键值统计信息 - 调试接口 / Get database key-value statistics - debug interface
#[utoipa::path(
    get,
    path = "/db/event_stats",
    tag = "database",
    summary = "获取数据库键值统计信息",
    description = "获取 RocksDB 中所有键值对的数量和大小统计（调试功能）",
    responses(
        (status = 200, description = "获取成功",
         body = crate::docs::ApiResponse<DatabaseStats>),
        (status = 500, description = "服务器内部错误",
         body = crate::docs::ErrorApiResponse)
    )
)]
pub async fn db_event_stats(State(db): State<std::sync::Arc<crate::db::RocksDbStorage>>) -> ApiResult {
    // 创建事件存储实例 / Create event storage instance
    let event_storage = match db.create_event_storage() {
        Ok(storage) => storage,
        Err(e) => {
            return Ok(ok_result::<DatabaseStats>(Err(
                crate::util::result::ApiError::InternalError(
                    format!("创建事件存储失败 / Failed to create event storage: {}", e)
                ),
            )))
        }
    };

    // 获取数据库统计信息 / Get database statistics
    let result = event_storage.get_db_stats();

    match result {
        Ok(stats) => Ok(ok_result::<DatabaseStats>(Ok(stats))),
        Err(e) => Ok(ok_result::<DatabaseStats>(Err(
            crate::util::result::ApiError::InternalError(e.to_string()),
        ))),
    }
}

/// 按 Mint 查询事件 / Query events by mint
#[utoipa::path(
    get,
    path = "/db/events/by_mint",
    tag = "events",
    summary = "按 Mint 查询事件",
    description = "按代币 mint account 地址查询事件，支持分页和排序",
    params(QueryByMintParams),
    responses(
        (status = 200, description = "查询成功",
         body = crate::docs::ApiResponse<PaginatedEvents>),
        (status = 500, description = "服务器内部错误",
         body = crate::docs::ErrorApiResponse)
    )
)]
pub async fn query_events_by_mint(
    State(db): State<std::sync::Arc<crate::db::RocksDbStorage>>,
    Query(params): Query<QueryByMintParams>,
) -> ApiResult {
    // 创建事件存储实例 / Create event storage instance
    let event_storage = match db.create_event_storage() {
        Ok(storage) => storage,
        Err(e) => {
            return Ok(ok_result::<PaginatedEvents>(Err(
                crate::util::result::ApiError::InternalError(
                    format!("创建事件存储失败 / Failed to create event storage: {}", e)
                ),
            )))
        }
    };

    // 查询事件 / Query events
    let result = event_storage.query_by_mint_paginated(
        &params.mint,
        params.page,
        params.page_size,
        params.sort == SortOrder::Asc,
    ).await;

    match result {
        Ok(paginated) => Ok(ok_result::<PaginatedEvents>(Ok(paginated))),
        Err(e) => Ok(ok_result::<PaginatedEvents>(Err(
            crate::util::result::ApiError::InternalError(e.to_string()),
        ))),
    }
}

/// 按 User 查询事件 / Query events by user
#[utoipa::path(
    get,
    path = "/db/events/by_user",
    tag = "events",
    summary = "按 User 查询事件",
    description = "按用户钱包地址查询事件，可选 mint 过滤，支持分页和排序",
    params(QueryByUserParams),
    responses(
        (status = 200, description = "查询成功",
         body = crate::docs::ApiResponse<PaginatedEvents>),
        (status = 500, description = "服务器内部错误",
         body = crate::docs::ErrorApiResponse)
    )
)]
pub async fn query_events_by_user(
    State(db): State<std::sync::Arc<crate::db::RocksDbStorage>>,
    Query(params): Query<QueryByUserParams>,
) -> ApiResult {
    // 创建事件存储实例 / Create event storage instance
    let event_storage = match db.create_event_storage() {
        Ok(storage) => storage,
        Err(e) => {
            return Ok(ok_result::<PaginatedEvents>(Err(
                crate::util::result::ApiError::InternalError(
                    format!("创建事件存储失败 / Failed to create event storage: {}", e)
                ),
            )))
        }
    };

    // 查询事件 / Query events
    let result = event_storage.query_by_user_paginated(
        &params.user,
        params.mint.as_deref(),
        params.page,
        params.page_size,
        params.sort == SortOrder::Asc,
    ).await;

    match result {
        Ok(paginated) => Ok(ok_result::<PaginatedEvents>(Ok(paginated))),
        Err(e) => Ok(ok_result::<PaginatedEvents>(Err(
            crate::util::result::ApiError::InternalError(e.to_string()),
        ))),
    }
}

/// 按 Signature 查询事件 / Query events by signature
#[utoipa::path(
    get,
    path = "/db/events/by_signature",
    tag = "events",
    summary = "按 Signature 查询事件",
    description = "按交易签名查询该交易产生的所有事件",
    params(QueryBySignatureParams),
    responses(
        (status = 200, description = "查询成功",
         body = crate::docs::ApiResponse<EventList>),
        (status = 500, description = "服务器内部错误",
         body = crate::docs::ErrorApiResponse)
    )
)]
pub async fn query_events_by_signature(
    State(db): State<std::sync::Arc<crate::db::RocksDbStorage>>,
    Query(params): Query<QueryBySignatureParams>,
) -> ApiResult {
    // 创建事件存储实例 / Create event storage instance
    let event_storage = match db.create_event_storage() {
        Ok(storage) => storage,
        Err(e) => {
            return Ok(ok_result::<EventList>(Err(
                crate::util::result::ApiError::InternalError(
                    format!("创建事件存储失败 / Failed to create event storage: {}", e)
                ),
            )))
        }
    };

    // 查询事件 / Query events
    let result = event_storage.query_by_signature(&params.signature).await;

    match result {
        Ok(events) => Ok(ok_result::<EventList>(Ok(EventList { events }))),
        Err(e) => Ok(ok_result::<EventList>(Err(
            crate::util::result::ApiError::InternalError(e.to_string()),
        ))),
    }
}

/// 创建数据库路由
pub fn routes() -> Router<std::sync::Arc<crate::db::RocksDbStorage>> {
    Router::new()
        .route("/db/put", post(db_put))
        .route("/db/get", post(db_get))
        .route("/db/delete", post(db_delete))
        .route("/db/stats", get(db_stats))
        .route("/db/event_stats", get(db_event_stats))
        .route("/db/events/by_mint", get(query_events_by_mint))
        .route("/db/events/by_user", get(query_events_by_user))
        .route("/db/events/by_signature", get(query_events_by_signature))
}
