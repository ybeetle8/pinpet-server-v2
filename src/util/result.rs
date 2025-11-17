use axum::{
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use tracing::error;

/// API 统一响应结果类型
pub type ApiResult = Result<Response, ApiError>;

/// 统一响应格式
#[derive(Debug, Serialize, Deserialize)]
pub struct CommonResult<T: Serialize> {
    /// 响应状态码
    pub code: u32,
    /// 响应消息
    pub msg: String,
    /// 响应数据（成功时包含数据，失败时为 None）
    pub data: Option<T>,
}

impl<T: Serialize> CommonResult<T> {
    /// 自定义响应
    pub fn default(code: u32, msg: String, data: Option<T>) -> Self {
        CommonResult { code, msg, data }
    }

    /// 成功响应（带数据）
    pub fn ok(data: T) -> Self {
        Self::default(200, "success".to_string(), Some(data))
    }

    /// 错误响应（无数据）
    pub fn error(code: u32, msg: String) -> Self {
        Self::default(code, msg, None)
    }

    /// 直接返回错误响应
    pub fn error_response(code: u32, msg: String) -> Response {
        Self::error(code, msg).into_response()
    }
}

impl<T: Serialize> IntoResponse for CommonResult<T> {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

/// 处理 Result 类型，自动将 Result<T, ApiError> 转换为响应
pub fn ok_result<T: Serialize>(result: Result<T, ApiError>) -> Response {
    match result {
        Ok(data) => CommonResult::ok(data).into_response(),
        Err(err) => err.into_response(),
    }
}

/// API 错误枚举
#[derive(Debug)]
pub enum ApiError {
    /// 直接返回的响应
    Response(Response),
    /// Anyhow 错误
    AnyhowError(anyhow::Error),
    /// 请求错误
    BadRequest(String),
    /// 请求参数错误
    RequestParamError(String),
    /// 未授权
    Unauthorized(String),
    /// 资源不存在
    NotFound(String),
    /// 业务错误
    BusinessError(String),
    /// 内部错误
    InternalError(String),
}

impl ApiError {
    /// 判断是否为业务错误（不需要打印堆栈）
    pub fn is_business_error(&self) -> bool {
        matches!(
            self,
            ApiError::Unauthorized(_)
                | ApiError::BadRequest(_)
                | ApiError::NotFound(_)
                | ApiError::BusinessError(_)
        )
    }
}

impl Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Response(_) => write!(f, "Response"),
            ApiError::AnyhowError(e) => write!(f, "Anyhow错误: {}", e),
            ApiError::BadRequest(e) => write!(f, "请求错误: {}", e),
            ApiError::RequestParamError(e) => write!(f, "参数错误: {}", e),
            ApiError::Unauthorized(e) => write!(f, "未授权: {}", e),
            ApiError::NotFound(e) => write!(f, "未找到: {}", e),
            ApiError::BusinessError(e) => write!(f, "业务错误: {}", e),
            ApiError::InternalError(e) => write!(f, "内部错误: {}", e),
        }
    }
}

impl std::error::Error for ApiError {}

/// Anyhow 错误转换
impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        error!("Anyhow错误: {:#}", e);
        Self::AnyhowError(e)
    }
}

/// 构建错误响应的辅助函数
fn error(code: u32, msg: String) -> Response {
    CommonResult::<()>::error(code, msg).into_response()
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // 记录系统错误的详细信息
        if !self.is_business_error() {
            error!("系统错误: {:?}", self);
        }

        match self {
            Self::Response(resp) => resp,
            Self::NotFound(e) => error(404, format!("404错误：{}", e)),
            Self::Unauthorized(e) => error(401, e),
            Self::BadRequest(e) => error(400, format!("请求错误：{}", e)),
            Self::RequestParamError(e) => error(400, format!("参数错误：{}", e)),
            Self::InternalError(e) => error(500, format!("内部错误：{}", e)),
            Self::BusinessError(e) => error(500, format!("业务错误：{}", e)),
            Self::AnyhowError(e) => error(500, e.to_string()),
        }
    }
}
