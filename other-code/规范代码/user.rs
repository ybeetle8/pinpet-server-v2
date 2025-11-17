use axum::extract::State;
use axum::routing::get;
use axum::{Extension, Router};
use framework_common::util::result::ApiResult;
use framework_common::util::result::common_result::ok_result;
use framework_starter_web::auth::LoginUser;
use sea_orm::DbConn;

use crate::framework::config::server_state::ServerState;
use crate::response::user::UserInfoResponse;
use crate::services::user::user_service;

pub fn routes() -> Router<ServerState> { Router::new().route("/user/get_info", get(get_info)).route("/user/test", get(test)) }

#[utoipa::path(
    get,
    path = "/user/test",
    tag = "user",
    summary = "测试接口",
    description = "用户模块测试接口，返回空数据。需要 JWT 认证。",
    responses(
        (status = 200, description = "测试请求成功", body = crate::docs::EmptyResponse),
        (status = 401, description = "未授权，需要有效的 JWT token"),
        (status = 500, description = "服务器内部错误")
    ),
    security(
        ("Authorization" = [])
    )
)]
pub async fn test(State(conn): State<DbConn>) -> ApiResult { Ok(ok_result(user_service::get_list(&conn).await)) }

#[utoipa::path(
    get,
    path = "/user/get_info",
    tag = "user",
    summary = "获取用户信息",
    description = "获取当前用户的详细信息，包括钱包地址、构建状态等。需要 JWT 认证。",
    responses(
        (status = 200, description = "获取用户信息成功", body = crate::docs::ApiResponse<UserInfoResponse>),
        (status = 401, description = "未授权", body = crate::docs::ErrorApiResponse,
            example = json!({"code": 401, "msg": "Unauthorized", "data": null})
        ),
        (status = 500, description = "服务器内部错误", body = crate::docs::ErrorApiResponse,
            example = json!({"code": 500, "msg": "Internal Server Error", "data": null})
        )
    ),
    security(
        ("Authorization" = [])
    )
)]
pub async fn get_info(State(state): State<ServerState>, Extension(user): Extension<LoginUser>) -> ApiResult {
    Ok(ok_result(user_service::get_user_info(user.address(), user.lang(), state.db(), state.i18n()).await))
}
