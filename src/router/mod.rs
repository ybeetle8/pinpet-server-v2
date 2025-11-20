pub mod db;
pub mod health;
pub mod orderbook;

use axum::Router;
use std::sync::Arc;

/// 创建所有路由
pub fn create_router(
    db: Arc<crate::db::RocksDbStorage>,
    orderbook_storage: Arc<crate::db::OrderBookStorage>,
) -> Router {
    // 创建 OrderBook 状态
    let orderbook_state = orderbook::OrderBookState {
        orderbook_storage: orderbook_storage.clone(),
    };

    Router::new()
        .merge(health::routes())
        .merge(db::routes().with_state(db))
        .merge(orderbook::routes().with_state(orderbook_state))
}
