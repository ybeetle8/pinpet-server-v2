pub mod db;
pub mod health;
pub mod orderbook;
pub mod token;

use axum::Router;
use std::sync::Arc;

/// 创建所有路由
pub fn create_router(
    db: Arc<crate::db::RocksDbStorage>,
    orderbook_storage: Arc<crate::db::OrderBookStorage>,
    token_storage: Arc<crate::db::TokenStorage>,
    orderbook_max_limit: usize,
) -> Router {
    // 创建 OrderBook 状态
    let orderbook_state = orderbook::OrderBookState {
        orderbook_storage: orderbook_storage.clone(),
        max_limit: orderbook_max_limit,
    };

    // 创建 Token 状态
    let token_state = token::TokenState {
        token_storage: token_storage.clone(),
    };

    Router::new()
        .merge(health::routes())
        .merge(db::routes().with_state(db))
        .merge(orderbook::routes().with_state(orderbook_state))
        .merge(token::routes().with_state(token_state))
}
