use axum::response::{IntoResponse, Response};
use framework_common::util::result::CommonResult;
use framework_starter_sea::entity::qt_user;
use getset::{Getters, Setters};
use sea_orm::sqlx::types::chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::json;
use typed_builder::TypedBuilder;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, Getters, Setters, TypedBuilder, ToSchema)]
#[getset(set = "pub", get = "pub")]
#[schema(
    title = "UserInfoResponse",
    description = "用户信息响应数据结构",
    example = json!({
        "wallet": "0x1234567890abcdef",
        "build_status": true,
        "created_time": "2024-01-01T00:00:00"
    })
)]
pub struct UserInfoResponse {
    /// 用户钱包地址，用户的以太坊钱包地址
    #[schema(example = "0x1234567890abcdef")]
    wallet: String,
    /// 构建状态，用户的构建状态，true 表示已构建，false 表示未构建
    #[schema(example = true)]
    build_status: bool,
    /// 创建时间，用户账户创建时间
    #[schema(example = "2024-01-01T00:00:00", value_type = String)]
    created_time: NaiveDateTime,
}

impl Default for UserInfoResponse {
    fn default() -> Self {
        UserInfoResponse::builder().wallet(String::new()).build_status(false).created_time(NaiveDateTime::default()).build()
    }
}


impl From<qt_user::Model> for UserInfoResponse {
    fn from(user_info: qt_user::Model) -> Self {
        user_info.create_time.time();
        UserInfoResponse::builder().wallet(user_info.wallet_address).build_status(true).created_time(user_info.create_time).build()
    }
}
