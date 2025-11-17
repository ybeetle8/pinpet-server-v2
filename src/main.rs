mod config;
mod db;
mod docs;
mod router;
mod util;

use axum::Router;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pinpet_server_v2=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("启动 Pinpet Server v2...");

    // 加载配置
    let config = match config::Config::new() {
        Ok(config) => config,
        Err(e) => {
            tracing::error!("❌ 配置加载失败: {}", e);
            std::process::exit(1);
        }
    };
    tracing::info!("✅ 配置加载成功");

    // 初始化 RocksDB
    let db_storage = match db::RocksDbStorage::new(&config) {
        Ok(storage) => Arc::new(storage),
        Err(e) => {
            tracing::error!("❌ RocksDB 初始化失败: {}", e);
            std::process::exit(1);
        }
    };
    tracing::info!("✅ RocksDB 初始化成功");

    // 创建 CORS 层
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // 创建路由
    let api_router = router::create_router(db_storage);

    // 创建 Swagger UI
    let swagger_ui = SwaggerUi::new("/swagger-ui")
        .url("/api-docs/openapi.json", docs::ApiDoc::openapi());

    // 组合所有路由
    let app = Router::new()
        .merge(swagger_ui)
        .merge(api_router)
        .layer(cors);

    // 绑定地址
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

    tracing::info!("服务器启动成功！");
    tracing::info!("访问 http://localhost:{}/health 测试接口", config.server.port);
    tracing::info!("访问 http://localhost:{}/swagger-ui 查看 API 文档", config.server.port);
    tracing::info!("访问 http://localhost:{}/db/* 测试数据库接口", config.server.port);

    // 启动服务器
    axum::serve(listener, app).await.unwrap();
}
