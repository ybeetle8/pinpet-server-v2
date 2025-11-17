use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::util::{ok_result, ApiResult};

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

/// 创建数据库路由
pub fn routes() -> Router<std::sync::Arc<crate::db::RocksDbStorage>> {
    Router::new()
        .route("/db/put", post(db_put))
        .route("/db/get", post(db_get))
        .route("/db/delete", post(db_delete))
        .route("/db/stats", get(db_stats))
}
