pub mod change;
pub mod db;
pub mod debug;
pub mod health;
pub mod kline;
pub mod orderbook;
pub mod orderbook_history;
pub mod price;
pub mod token;
pub mod volume;

use axum::Router;
use std::sync::Arc;

/// 创建所有路由
pub fn create_router(
    db: Arc<crate::db::RocksDbStorage>,
    token_storage: Arc<crate::db::TokenStorage>,
    orderbook_storage: Arc<crate::db::OrderBookStorage>,
    kline_storage: Arc<crate::db::KlineStorage>,
    volume_storage: Arc<crate::volume::VolumeStorage>,
    change_storage: Arc<crate::change::ChangeStorage>,
    price_service: Arc<crate::price::SolPriceService>,
    config: Arc<crate::config::Config>,
    solana_client: crate::solana::SolanaClient,
    sync_service: Option<Arc<crate::orderbook_sync::OrderBookSyncService>>,
) -> Router {
    // 创建 Token 状态
    let token_state = token::TokenState {
        token_storage: token_storage.clone(),
    };

    // 创建 K线 状态
    let kline_state = kline::KlineState {
        kline_storage: kline_storage.clone(),
    };

    // 创建价格状态 / Create price state
    let price_state = price::PriceState {
        price_service: price_service.clone(),
    };

    // 创建 Debug 状态 / Create debug state
    let debug_state = debug::DebugState {
        config,
        solana_client,
        orderbook_storage: orderbook_storage.clone(),
        sync_service,
    };

    Router::new()
        .merge(health::routes())
        .merge(db::routes().with_state(db))
        .merge(token::routes().with_state(token_state))
        .merge(orderbook::routes().with_state(orderbook_storage.clone()))
        .merge(orderbook_history::routes().with_state(orderbook_storage))
        .merge(kline::routes().with_state(kline_state))
        .merge(price::routes().with_state(price_state))
        .merge(volume::create_volume_routes(volume_storage))
        .merge(change::create_change_routes(change_storage))
        .merge(debug::routes().with_state(debug_state))
}
