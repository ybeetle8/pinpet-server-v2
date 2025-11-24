pub mod db;
pub mod health;
pub mod token;

use axum::Router;
use std::sync::Arc;

/// 创建所有路由
pub fn create_router(
    db: Arc<crate::db::RocksDbStorage>,
    token_storage: Arc<crate::db::TokenStorage>,
) -> Router {
    // 创建 Token 状态
    let token_state = token::TokenState {
        token_storage: token_storage.clone(),
    };

    Router::new()
        .merge(health::routes())
        .merge(db::routes().with_state(db))
        .merge(token::routes().with_state(token_state))
}
