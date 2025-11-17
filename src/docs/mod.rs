use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{OpenApi, ToSchema};

/// API 统一响应格式（用于 Swagger 文档）
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(title = "ApiResponse", description = "API 统一响应格式")]
pub struct ApiResponse<T>
where
    T: ToSchema + Serialize,
{
    /// 响应状态码：200=成功，其他=错误
    #[schema(example = 200)]
    pub code: u32,

    /// 响应消息
    #[schema(example = "success")]
    pub msg: String,

    /// 响应数据，成功时包含具体数据，失败时为 null
    pub data: Option<T>,
}

/// 空数据响应格式（用于 Swagger 文档）
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(
    title = "EmptyResponse",
    description = "空数据响应格式（操作成功但无返回数据）",
    example = json!({
        "code": 200,
        "msg": "success",
        "data": null
    })
)]
pub struct EmptyResponse {
    #[schema(example = 200)]
    pub code: u32,

    #[schema(example = "success")]
    pub msg: String,

    pub data: Option<()>,
}

/// 错误响应格式（用于 Swagger 文档）
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(title = "ErrorApiResponse", description = "错误响应格式")]
pub struct ErrorApiResponse {
    /// 响应状态码：非200表示错误
    pub code: u32,

    /// 错误消息
    pub msg: String,

    /// 错误时数据为空
    pub data: Option<Value>,
}

/// OpenAPI 文档配置
#[derive(OpenApi)]
#[openapi(
    paths(
        // 路由函数列表
        crate::router::health::health,
        crate::router::db::db_put,
        crate::router::db::db_get,
        crate::router::db::db_delete,
        crate::router::db::db_stats,
    ),
    components(
        schemas(
            // 响应结构体列表
            crate::router::health::HealthResponse,
            crate::router::db::DbRequest,
            crate::router::db::DbResponse,
            EmptyResponse,
            ErrorApiResponse,
        )
    ),
    tags(
        (name = "system", description = "系统相关接口"),
        (name = "database", description = "数据库相关接口"),
    ),
    info(
        title = "Pinpet Server API",
        version = "0.1.0",
        description = "Pinpet Server API 文档"
    )
)]
pub struct ApiDoc;
