use axum::Router;
use framework_starter_web::SecurityAddon;
use framework_starter_web::swagger::create_swagger_router_dynamic;
use framework_starter_web::swagger::swagger_config::ApiDocConfig;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::openapi::{ContactBuilder, Info, LicenseBuilder, ServerBuilder};
use utoipa::{Modify, OpenApi, ToSchema};

use crate::framework::config::server_config::ServerConfig;
use crate::request::auth::AuthRequest;
use crate::response::user::user_info_response::UserInfoResponse;
use crate::response::wallet::asset_response::AssetsResponse;
use crate::router::api::{auth, user, wallet};
use crate::router::rpc::rpc::{self, RpcRequest, RpcResponse};

/// 通用 API 响应结构体（用于 Swagger 文档）
/// 这个结构体和 framework_common::CommonResult 完全一样，但实现了 ToSchema
/// 注意：由于 utoipa 的限制，泛型结构体无法自动继承 T 类型的 example
/// 所以需要在每个接口的 responses 中手动指定 example
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(title = "ApiResponse", description = "API 统一响应格式")]
pub struct ApiResponse<T>
where T: ToSchema + Serialize
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
/// 空响应类型
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
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
    /// 响应状态码
    #[schema(example = 200)]
    pub code: u32,
    /// 响应消息
    #[schema(example = "success")]
    pub msg: String,
    /// 空数据
    pub data: Option<()>,
}

// 专门的 Token 响应结构体，避免 ApiResponse<String> 的解析问题
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(title = "TokenApiResponse", description = "Token 响应格式")]
pub struct TokenApiResponse {
    /// 响应状态码：200=成功
    #[schema(example = 200)]
    pub code: u32,
    /// 响应消息
    #[schema(example = "success")]
    pub msg: String,
    /// JWT Token
    #[schema(example = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...")]
    pub data: Option<String>,
}

// 专门的错误响应结构体，避免 ApiResponse<String> 的解析问题
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

/// API 模块文档配置
#[derive(OpenApi)]
#[openapi(
    paths(
        // 认证相关
        auth::auth,
        // 用户相关
        user::test,
        user::get_info,
        // 资产中心
        wallet::get_assets,
    ),
    components(
        schemas(
            // 请求结构体
            AuthRequest,
            // 基础数据结构体
            UserInfoResponse,
            AssetsResponse,
            // 专门的响应结构体（用于 Swagger 文档）
            ErrorApiResponse,
            EmptyResponse,
        )
    ),
    // SecurityAddon 将在运行时手动应用
    tags(
        (name = "auth", description = "认证相关接口"),
        (name = "user", description = "用户相关接口"),
        (name = "wallet", description = "资产相关接口"),
    ),
    // 不设置全局安全要求，让每个接口自己决定是否需要认证
    // 这样可以避免不需要认证的接口（如登录接口）也被要求提供 Authorization 参数

    // info 和 servers 将通过 ApiDocInfoAddon 动态设置
)]
pub struct ApiDoc;

/// RPC 模块文档配置
#[derive(OpenApi)]
#[openapi(
    paths(
        // RPC 相关的路径
        rpc::get_info,
    ),
    components(
        schemas(
            // RPC 相关的请求/响应结构体
            RpcRequest,
            RpcResponse,
        )
    ),
    modifiers(&SECURITY_ADDON),
    tags(
        (name = "rpc", description = "RPC 相关接口"),
    )
    // info 和 servers 将通过 ApiDocInfoAddon 动态设置
)]
pub struct RpcDoc;

// 创建默认的 SecurityAddon 实例
static SECURITY_ADDON: SecurityAddon = SecurityAddon::default();

/// API 文档信息配置修改器
pub struct ApiDocInfoAddon {
    pub config: ApiDocConfig,
    // pub title_suffix: String, // 用于区分 REST API 和 RPC API
}

impl Modify for ApiDocInfoAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let title = self.config.title();
        let mut info = Info::new(title, self.config.version());

        info.description = Some(self.config.description().to_string());

        info.contact = Some(ContactBuilder::new().name(Some(self.config.author())).email(Some(self.config.email())).build());

        info.license = Some(LicenseBuilder::new().name(self.config.license_name()).url(Some(self.config.license_url())).build());

        openapi.info = info;
        openapi.servers =
            Some(vec![ServerBuilder::new().url(self.config.host()).description(Some(self.config.memo().to_string())).build()]);
    }
}

/// 创建带配置的 REST API OpenAPI 文档
pub fn create_rest_api_openapi(config: &ServerConfig) -> utoipa::openapi::OpenApi {
    let mut openapi = ApiDoc::openapi();

    // 创建带有认证白名单的 SecurityAddon
    // 从配置文件中读取白名单路径
    let security_addon = SecurityAddon::default_with_auth_whitelist(config.swagger().auth_whitelist().clone());
    security_addon.modify(&mut openapi);

    // 应用配置修改器
    let config_modifier = ApiDocInfoAddon {
        config: config.swagger().clone(),
        // title_suffix: "REST API".to_string(),
    };
    config_modifier.modify(&mut openapi);

    openapi
}

/// 创建带配置的 RPC API OpenAPI 文档
pub fn create_rpc_api_openapi(config: &ServerConfig) -> utoipa::openapi::OpenApi {
    let mut openapi = RpcDoc::openapi();

    // 应用配置修改器
    let config_modifier = ApiDocInfoAddon {
        config: config.swagger().clone(),
        // title_suffix: "RPC API".to_string(),
    };
    config_modifier.modify(&mut openapi);

    openapi
}

/// 创建本服务的 Swagger UI 路由（带配置）
pub fn create_api_service_swagger_router<S>(config: &ServerConfig) -> Router<S>
where S: Clone + Send + Sync + 'static {
    // 从配置文件中读取 API 分组名称
    let rest_api_name = config.swagger().rest_api_group_name().clone();
    let rpc_api_name = config.swagger().rpc_api_group_name().clone();

    // 使用 web 模块的通用函数，传入配置好的 OpenAPI 文档
    create_swagger_router_dynamic(vec![
        (rest_api_name, "/api-docs/api/openapi.json".to_string(), create_rest_api_openapi(config)),
        (rpc_api_name, "/api-docs/rpc/openapi.json".to_string(), create_rpc_api_openapi(config)),
    ])
}
