use axum::{routing::get, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

use crate::util::{ok_result, ApiResult};

/// Health check 响应数据
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(
    title = "HealthResponse",
    description = "健康检查响应数据",
    example = json!({
        "status": "ok",
        "version": "0.1.0"
    })
)]
pub struct HealthResponse {
    /// 服务状态
    #[schema(example = "ok")]
    pub status: String,

    /// 服务版本
    #[schema(example = "0.1.0")]
    pub version: String,
}

/// Health check 接口
#[utoipa::path(
    get,
    path = "/health",
    tag = "system",
    summary = "健康检查",
    description = "检查服务是否正常运行",
    responses(
        (status = 200, description = "服务正常",
         body = crate::docs::ApiResponse<HealthResponse>),
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
pub async fn health() -> ApiResult {
    let response = HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    Ok(ok_result(Ok(response)))
}

/// 创建健康检查路由
pub fn routes() -> Router {
    Router::new().route("/health", get(health))
}
