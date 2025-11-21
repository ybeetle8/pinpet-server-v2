// Token路由定义 / Token routes definition
use axum::{routing::get, Router};
use super::token::TokenState;

/// 创建Token相关路由 / Create token related routes
pub fn routes() -> Router<TokenState> {
    Router::new()
        .route("/api/tokens/mint/:mint", get(super::token::get_token_by_mint))
        .route("/api/tokens/symbol", get(super::token::get_tokens_by_symbol))
        .route("/api/tokens/latest", get(super::token::get_latest_tokens))
        .route("/api/tokens/slot-range", get(super::token::get_tokens_by_slot_range))
        .route("/api/tokens/stats", get(super::token::get_token_stats))
}
