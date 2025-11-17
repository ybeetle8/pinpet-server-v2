use axum::body::Body;
use axum::extract::rejection::JsonRejection;
use axum::response::{IntoResponse, Response};
use deadpool_redis::{CreatePoolError, PoolError};
use sea_orm::DbErr;
use std::fmt;
use tracing::error;

use crate::util::result::ErrorResult;

#[derive(Debug)]
pub enum ApiError {
    Response(Response),
    AnyhowError(anyhow::Error),
    DbError(DbErr),
    RedisError(PoolError),
    RedisCreatePoolError(CreatePoolError),
    BadRequest(String),
    RequestParamError(String),
    JsonRejection(JsonRejection),
    InternalError(std::io::Error),
    Unauthorized(String),
    NotFound(String),
    BusinessError(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Response(_) => write!(f, "Response error"),
            Self::AnyhowError(e) => write!(f, "Anyhow error: {}", e),
            Self::DbError(e) => write!(f, "Database error: {}", e),
            Self::RedisError(e) => write!(f, "Redis error: {}", e),
            Self::RedisCreatePoolError(e) => write!(f, "Redis pool creation error: {}", e),
            Self::BadRequest(e) => write!(f, "Bad request: {}", e),
            Self::RequestParamError(e) => write!(f, "Request parameter error: {}", e),
            Self::JsonRejection(e) => write!(f, "JSON rejection: {}", e.body_text()),
            Self::InternalError(e) => write!(f, "Internal error: {}", e),
            Self::Unauthorized(e) => write!(f, "Unauthorized: {}", e),
            Self::NotFound(e) => write!(f, "Not found: {}", e),
            Self::BusinessError(e) => write!(f, "Business error: {}", e),
        }
    }
}

impl ApiError {
    /// 业务错误（不需要堆栈跟踪的错误）
    pub fn is_business_error(&self) -> bool {
        matches!(self, ApiError::Unauthorized(_) | ApiError::BadRequest(_) | ApiError::NotFound(_) | ApiError::BusinessError(_))
    }

    /// 系统错误
    pub fn is_system_error(&self) -> bool { !self.is_business_error() }
}

impl From<DbErr> for ApiError {
    fn from(e: DbErr) -> Self {
        error!("数据库错误: {:?}", e);
        Self::DbError(e.into())
    }
}
impl From<PoolError> for ApiError {
    fn from(e: PoolError) -> Self {
        error!("Redis错误: {:?}", e);
        Self::RedisError(e)
    }
}

impl From<CreatePoolError> for ApiError {
    fn from(e: CreatePoolError) -> Self {
        error!("Redis创建连接池错误: {:?}", e);
        Self::RedisCreatePoolError(e)
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        // 使用 {:#} 格式化以显示完整的错误链和堆栈信息
        error!("anyhow错误：{:#}", e);
        Self::AnyhowError(e)
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        error!("IO错误：{:?}", e);
        Self::InternalError(e)
    }
}

impl From<JsonRejection> for ApiError {
    fn from(e: JsonRejection) -> Self {
        error!("Json参数格式错误：{:?}", e);
        Self::JsonRejection(e)
    }
}

fn error(code: u32, message: String) -> Response<Body> { ErrorResult::error_response(code, message) }

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Response(resp) => resp,
            Self::NotFound(e) => error(404, format!("404错误：{e}")),
            Self::JsonRejection(e) => error(400, e.body_text()),
            Self::Unauthorized(e) => error(401, e),
            Self::BadRequest(e) => error(400, format!("请求错误：{e}")),
            Self::RequestParamError(e) => error(400, format!("参数错误：{e}")),
            Self::InternalError(e) => error(500, format!("内部错误：{e}")),
            Self::DbError(e) => error(500, format!("数据库错误：{e}")),
            Self::RedisError(e) => error(500, format!("Redis错误：{e}")),
            Self::RedisCreatePoolError(e) => error(500, format!("Redis Pool错误：{e}")),
            Self::BusinessError(e) => error(500, format!("业务错误：{e}")),
            Self::AnyhowError(e) => error(500, e.to_string()),
            // 未知错误
            #[allow(unreachable_patterns)]
            _ => {
                error!("未知异常：[{}:{}]", file!(), line!());
                error(500, "未知错误".to_string())
            }
        }
    }
}
