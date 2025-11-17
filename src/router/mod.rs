pub mod db;
pub mod health;

use axum::Router;
use std::sync::Arc;

/// 创建所有路由
pub fn create_router(db: Arc<crate::db::RocksDbStorage>) -> Router {
    Router::new()
        .merge(health::routes())
        .merge(db::routes().with_state(db))
}
