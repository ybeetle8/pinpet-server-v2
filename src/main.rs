mod config;
mod db;
mod docs;
mod router;
mod solana;
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

    // 创建 OrderBook 存储实例（无论事件监听器是否启用都需要）
    // Create OrderBook storage instance (needed regardless of event listener status)
    let orderbook_storage = match db_storage.create_orderbook_storage() {
        Ok(storage) => Arc::new(storage),
        Err(e) => {
            tracing::error!("❌ OrderBook 存储创建失败 / Failed to create OrderBook storage: {}", e);
            std::process::exit(1);
        }
    };
    tracing::info!("✅ OrderBook 存储初始化成功");

    // 初始化 Solana 事件监听器 / Initialize Solana event listener
    if config.solana.enable_event_listener {
        tracing::info!("🚀 初始化 Solana 事件监听器 / Initializing Solana event listener");

        // 创建 Solana 客户端 / Create Solana client
        let solana_client = match solana::SolanaClient::new(config.solana.rpc_url.clone()) {
            Ok(client) => Arc::new(client),
            Err(e) => {
                tracing::error!("❌ Solana 客户端创建失败 / Failed to create Solana client: {}", e);
                std::process::exit(1);
            }
        };

        // 创建事件存储实例 / Create event storage instance
        let event_storage = match db_storage.create_event_storage() {
            Ok(storage) => Arc::new(storage),
            Err(e) => {
                tracing::error!("❌ 事件存储创建失败 / Failed to create event storage: {}", e);
                std::process::exit(1);
            }
        };

        // 创建 Token 存储实例 / Create token storage instance
        let token_storage = match db_storage.create_token_storage() {
            Ok(storage) => Arc::new(storage),
            Err(e) => {
                tracing::error!("❌ Token 存储创建失败 / Failed to create Token storage: {}", e);
                std::process::exit(1);
            }
        };

        // 创建存储事件处理器 / Create storage event handler
        let storage_handler = Arc::new(solana::StorageEventHandler::new(
            event_storage,
            orderbook_storage.clone(),
            token_storage.clone(),
        ));

        // 创建清算处理器 / Create liquidation processor
        let liquidation_processor = Arc::new(solana::LiquidationProcessor::new(orderbook_storage.clone()));

        // 创建 MintEventRouter 作为事件处理器 / Create MintEventRouter as event handler
        let event_handler = Arc::new(solana::MintEventRouter::new(
            liquidation_processor,
            storage_handler,
        ));

        // 创建事件监听器管理器 / Create event listener manager
        let mut listener_manager = solana::EventListenerManager::new();

        if let Err(e) = listener_manager.initialize(
            config.solana.clone(),
            solana_client,
            event_handler,
        ) {
            tracing::error!("❌ 事件监听器初始化失败 / Failed to initialize event listener: {}", e);
            std::process::exit(1);
        }

        // 在后台启动事件监听器 / Start event listener in background
        tokio::spawn(async move {
            if let Err(e) = listener_manager.start().await {
                tracing::error!("❌ 事件监听器启动失败 / Failed to start event listener: {}", e);
            }
        });

        tracing::info!("✅ Solana 事件监听器已启动 / Solana event listener started");
    } else {
        tracing::info!("⏭️ Solana 事件监听器已禁用 / Solana event listener disabled");
    }

    // 创建 CORS 层
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // 创建 Token 存储实例 (用于API查询) / Create token storage instance (for API queries)
    let token_storage_for_api = match db_storage.create_token_storage() {
        Ok(storage) => Arc::new(storage),
        Err(e) => {
            tracing::error!("❌ Token 存储创建失败(API) / Failed to create Token storage (API): {}", e);
            std::process::exit(1);
        }
    };

    // 创建路由
    let api_router = router::create_router(
        db_storage,
        orderbook_storage,
        token_storage_for_api,
        config.database.orderbook_max_limit
    );

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
