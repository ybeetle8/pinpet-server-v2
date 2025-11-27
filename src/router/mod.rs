pub mod db;
pub mod health;
pub mod kline;
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
    kline_storage: Arc<crate::db::KlineStorage>,
) -> Router {
    // 创建 Token 状态
    let token_state = token::TokenState {
        token_storage: token_storage.clone(),
    };

    // 创建 K线 状态
    let kline_state = kline::KlineState {
        kline_storage: kline_storage.clone(),
    };

    Router::new()
        .merge(health::routes())
        .merge(db::routes().with_state(db))
        .merge(token::routes().with_state(token_state))
        .merge(orderbook::routes().with_state(orderbook_storage.clone()))
        .merge(orderbook_history::routes().with_state(orderbook_storage))
        .merge(kline::routes().with_state(kline_state))
}
