pub mod health;

use axum::Router;

/// 创建所有路由
pub fn create_router() -> Router {
    Router::new().merge(health::routes())
}
