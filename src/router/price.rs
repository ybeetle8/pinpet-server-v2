use axum::{extract::State, routing::get, Json, Router};
use std::sync::Arc;

use crate::price::{SolPrice, SolPriceService};
use crate::util::result::{ApiError, CommonResult};

/// 价格路由状态 / Price router state
#[derive(Clone)]
pub struct PriceState {
    pub price_service: Arc<SolPriceService>,
}

/// 获取 SOL 当前价格 / Get current SOL price
///
/// 返回 SOL 的当前美元价格和最后更新时间
/// Returns the current USD price of SOL and last update time
#[utoipa::path(
    get,
    path = "/price/sol",
    tag = "价格查询 / Price Query",
    responses(
        (status = 200, description = "成功获取价格 / Successfully fetched price", body = SolPrice),
        (status = 500, description = "价格数据不可用 / Price data unavailable")
    )
)]
async fn get_sol_price(
    State(state): State<PriceState>,
) -> Result<Json<CommonResult<SolPrice>>, ApiError> {
    match state.price_service.get_price().await {
        Some(price) => {
            tracing::debug!("📊 返回 SOL 价格: ${} / Returning SOL price: ${}", price.price, price.price);
            Ok(Json(CommonResult::ok(price)))
        }
        None => {
            tracing::warn!("⚠️ 价格数据尚未可用 / Price data not yet available");
            Err(ApiError::InternalError(
                "价格数据尚未可用,请稍后重试 / Price data not yet available, please try again later".to_string()
            ))
        }
    }
}

/// 创建价格相关路由 / Create price related routes
pub fn routes() -> Router<PriceState> {
    Router::new()
        .route("/price/sol", get(get_sol_price))
}
