pub mod db;
pub mod health;
pub mod orderbook;
pub mod orderbook_history;
pub mod token;

use axum::Router;
use std::sync::Arc;

/// 创建所有路由
pub fn create_router(
    db: Arc<crate::db::RocksDbStorage>,
    token_storage: Arc<crate::db::TokenStorage>,
    orderbook_storage: Arc<crate::db::OrderBookStorage>,
) -> Router {
    // 创建 Token 状态
    let token_state = token::TokenState {
        token_storage: token_storage.clone(),
    };

    Router::new()
        .merge(health::routes())
        .merge(db::routes().with_state(db))
        .merge(token::routes().with_state(token_state))
        .merge(orderbook::routes().with_state(orderbook_storage.clone()))
        .merge(orderbook_history::routes().with_state(orderbook_storage))
}
