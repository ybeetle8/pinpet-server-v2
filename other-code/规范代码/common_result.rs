use std::string::ToString;

use axum::Json;
use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::util::result::ApiError;

#[derive(Debug, Serialize, Deserialize)]
pub struct CommonResult<T: Serialize> {
    pub code: u32,
    pub msg: String,
    pub data: Option<T>,
}

const SUCCESS_CODE: u32 = 200;
const SUCCESS: &str = "success";

/// API 统一返回结果类型
pub type ApiResult<T = Response<Body>> = Result<T, ApiError>;

impl<T: Serialize> CommonResult<T> {
    pub fn default(code: u32, msg: String, data: Option<T>) -> Self { CommonResult { code, msg, data } }

    pub fn ok(data: T) -> Self { Self::default(SUCCESS_CODE, SUCCESS.to_string(), Some(data)) }

    pub fn error(code: u32, msg: String) -> Self { Self::default(code, msg, None) }

    pub fn error_response(code: u32, msg: String) -> Response { Self::error(code, msg).into_response() }
}

impl<T: Serialize> IntoResponse for CommonResult<T> {
    fn into_response(self) -> Response { (StatusCode::OK, Json(self)).into_response() }
}

pub type ErrorResult = CommonResult<()>;

pub fn ok_result<T: Serialize>(result: Result<T, ApiError>) -> Response {
    match result {
        Ok(data) => CommonResult::ok(data).into_response(),
        Err(err) => err.into_response(),
    }
}

/// 通用的成功响应函数 - 可以处理任何可序列化的数据
pub fn success_result<T: Serialize>(data: Option<T>) -> Response {
    match data {
        Some(d) => CommonResult::ok(d).into_response(),
        None => CommonResult::<T>::default(SUCCESS_CODE, SUCCESS.to_string(), None).into_response(),
    }
}
