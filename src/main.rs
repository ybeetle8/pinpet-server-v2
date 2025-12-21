mod config;
mod db;
mod docs;
mod kline;
mod orderbook;
mod price;
mod router;
mod solana;
mod util;

use axum::Router;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, fmt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[tokio::main]
async fn main() {
    // 初始化日志 / Initialize logging
    // 创建日志目录(如果不存在) / Create logs directory if it doesn't exist
    std::fs::create_dir_all("logs").expect("无法创建 logs 目录 / Cannot create logs directory");

    // 配置文件日志输出 / Configure file logging
    let file_appender = tracing_appender::rolling::daily("logs", "pinpet-server.log");
    let (non_blocking_file, _guard) = tracing_appender::non_blocking(file_appender);

    // 配置控制台日志输出 / Configure console logging
    let (non_blocking_stdout, _guard2) = tracing_appender::non_blocking(std::io::stdout());

    // 环境过滤器 / Environment filter
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "pinpet_server_v2=debug,tower_http=debug".into());

    // 初始化订阅器,同时输出到文件和控制台 / Initialize subscriber with both file and console output
    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_writer(non_blocking_file)
                .with_ansi(false) // 文件输出不使用颜色 / No colors for file output
        )
        .with(
            fmt::layer()
                .with_writer(non_blocking_stdout)
                .with_ansi(true) // 控制台输出使用颜色 / Colors for console output
        )
        .init();

    tracing::info!("启动 Pinpet Server v2...");
    tracing::info!("📝 日志输出到: logs/pinpet-server.log.* / Logging to: logs/pinpet-server.log.*");

    // 加载配置
    let config = match config::Config::new() {
        Ok(config) => Arc::new(config),
        Err(e) => {
            tracing::error!("❌ 配置加载失败: {}", e);
            std::process::exit(1);
        }
    };
    tracing::info!("✅ 配置加载成功");

    // 初始化 RocksDB
    let mut db_storage = match db::RocksDbStorage::new(&config) {
        Ok(storage) => storage,
        Err(e) => {
            tracing::error!("❌ RocksDB 初始化失败: {}", e);
            std::process::exit(1);
        }
    };
    tracing::info!("✅ RocksDB 初始化成功");

    // 初始化 OrderBook 专用数据库 / Initialize OrderBook dedicated database
    let orderbook_storage = match db::OrderBookStorage::new(
        &config.database.orderbook_db,
        &config.database.orderbook_db_path,
    ) {
        Ok(storage) => Arc::new(storage),
        Err(e) => {
            tracing::error!("❌ OrderBook 数据库初始化失败 / Failed to initialize OrderBook database: {}", e);
            std::process::exit(1);
        }
    };
    tracing::info!("✅ OrderBook 数据库初始化成功 / OrderBook database initialized successfully");

    // 创建 Solana 客户端 / Create Solana client
    let solana_client = match solana::SolanaClient::new(config.solana.rpc_url.clone()) {
        Ok(client) => client,
        Err(e) => {
            tracing::error!("❌ Solana 客户端创建失败 / Failed to create Solana client: {}", e);
            std::process::exit(1);
        }
    };
    tracing::info!("✅ Solana 客户端创建成功 / Solana client created successfully");

    // 初始化 SOL 价格服务 / Initialize SOL price service
    tracing::info!("🚀 初始化 SOL 价格服务 / Initializing SOL price service");
    let price_service = Arc::new(price::SolPriceService::new());

    // 启动定时更新任务 / Start periodic update task
    price_service.clone().start_periodic_update();
    tracing::info!("✅ SOL 价格服务初始化成功 / SOL price service initialized successfully");

    // 设置价格服务到 db_storage (必须在创建 EventStorage 之前) / Set price service to db_storage (must be before creating EventStorage)
    db_storage.set_price_service(price_service.clone());

    // 现在将 db_storage 转为 Arc / Now convert db_storage to Arc
    let db_storage = Arc::new(db_storage);

    // 初始化 K线推送服务 (如果启用) / Initialize K-line WebSocket service (if enabled)
    let (kline_socket_service, socketio_layer) = if config.kline.enable_kline_service {
        tracing::info!("🚀 初始化 K线 WebSocket 服务 / Initializing K-line WebSocket service");

        // 创建K线配置 / Create K-line config
        let kline_config = kline::KlineConfig {
            max_subscriptions_per_client: config.kline.max_subscriptions_per_client,
            ping_interval_secs: config.kline.ping_interval_secs,
            ping_timeout_secs: config.kline.ping_timeout_secs,
        };

        // 创建事件存储实例 (用于K线服务查询历史数据) / Create event storage instance (for K-line service to query history)
        let event_storage_for_kline = match db_storage.create_event_storage() {
            Ok(storage) => Arc::new(storage),
            Err(e) => {
                tracing::error!("❌ 事件存储创建失败(K线) / Failed to create event storage (K-line): {}", e);
                std::process::exit(1);
            }
        };

        // 创建K线推送服务 / Create K-line socket service
        let (kline_service, layer) = match kline::KlineSocketService::new(
            event_storage_for_kline,
            kline_config,
        ) {
            Ok((service, layer)) => (Arc::new(service), Some(layer)),
            Err(e) => {
                tracing::error!("❌ K线 Socket 服务创建失败 / Failed to create K-line socket service: {}", e);
                std::process::exit(1);
            }
        };

        // 设置事件处理器 / Setup event handlers
        kline_service.setup_socket_handlers();

        tracing::info!("✅ K线 WebSocket 服务初始化成功 / K-line WebSocket service initialized");
        (Some(kline_service), layer)
    } else {
        tracing::info!("ℹ️ K线 WebSocket 服务已禁用 / K-line WebSocket service disabled");
        (None, None)
    };

    // 初始化事件队列存储（用于持久化） / Initialize event queue storage (for persistence)
    let event_queue_storage = if config.solana.enable_event_listener {
        match db::EventQueueStorage::new("./data/event_queue") {
            Ok(storage) => Some(Arc::new(storage)),
            Err(e) => {
                tracing::error!("❌ 事件队列存储初始化失败 / Failed to initialize event queue storage: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    // 初始化 Solana 事件监听器 / Initialize Solana event listener
    if config.solana.enable_event_listener {
        tracing::info!("🚀 初始化 Solana 事件监听器 / Initializing Solana event listener");

        // 使用已创建的 Solana 客户端 / Use already created Solana client
        let solana_client_arc = Arc::new(solana_client.clone());

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
        let mut storage_handler = solana::StorageEventHandler::new(
            event_storage.clone(),  // 克隆一份供storage_handler使用 / Clone for storage_handler
            token_storage.clone(),
            orderbook_storage.clone(),
        );

        // 如果启用了K线服务,设置到 StorageEventHandler 中用于推送 LiquidateEvent
        // If K-line service is enabled, set it in StorageEventHandler for pushing LiquidateEvent
        if let Some(ref kline_service) = kline_socket_service {
            storage_handler.set_kline_socket_service(kline_service.clone());
        }

        let storage_handler = Arc::new(storage_handler);

        // 如果启用了K线服务,创建K线事件处理器包装器 / If K-line service is enabled, create K-line event handler wrapper
        let event_handler: Arc<dyn solana::EventHandler> = if let Some(ref kline_service) = kline_socket_service {
            // 创建K线事件处理器,包装StorageEventHandler / Create K-line event handler wrapping StorageEventHandler
            Arc::new(kline::KlineEventHandler::new(
                storage_handler,
                kline_service.clone(),
                event_storage,  // 传入event_storage用于读取K线数据 / Pass event_storage for reading K-line data
                price_service.clone(),  // 传入price_service用于SOL->USD转换 / Pass price_service for SOL->USD conversion
            ))
        } else {
            // 不使用K线服务,直接使用 StorageEventHandler / Without K-line service, use StorageEventHandler directly
            storage_handler
        };

        // 创建事件监听器管理器 / Create event listener manager
        let mut listener_manager = solana::EventListenerManager::new();

        // 使用之前创建的事件队列存储 / Use previously created event queue storage
        let event_queue = event_queue_storage.as_ref().unwrap().clone();

        if let Err(e) = listener_manager.initialize(
            config.solana.clone(),
            solana_client_arc,
            event_handler,
            event_queue,
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

        // 启动事件队列清理任务 / Start event queue cleanup task
        if let Some(ref event_queue) = event_queue_storage {
            let queue_for_cleanup = event_queue.clone();
            tokio::spawn(async move {
                tracing::info!("🧹 启动事件队列清理任务 / Starting event queue cleanup task");
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600)); // 每小时执行一次 / Run every hour

                loop {
                    interval.tick().await;

                    // 清理24小时前的已完成事件 / Clean up completed events older than 24 hours
                    let older_than_ms = 24 * 60 * 60 * 1000; // 24 hours in milliseconds
                    match queue_for_cleanup.cleanup_completed(older_than_ms).await {
                        Ok(count) => {
                            if count > 0 {
                                tracing::info!("🧹 清理了 {} 个过期事件 / Cleaned up {} expired events", count, count);
                            }
                        }
                        Err(e) => {
                            tracing::error!("清理事件队列失败 / Failed to cleanup event queue: {}", e);
                        }
                    }

                    // 打印队列健康统计 / Print queue health statistics
                    if let Ok((pending, processing, dead, total)) = queue_for_cleanup.get_statistics().await {
                        tracing::info!("📊 事件队列统计 / Event queue stats: 待处理/pending={}, 处理中/processing={}, 死信/dead={}, 总计/total={}",
                                      pending, processing, dead, total);

                        if dead > 100 {
                            tracing::warn!("⚠️ 死信队列过大，请检查处理逻辑 / Dead letter queue too large, please check processing logic");
                        }
                    }
                }
            });
        }
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

    // 创建 K线 存储实例 (用于API查询) / Create K-line storage instance (for API queries)
    let kline_storage_for_api = match db_storage.create_kline_storage() {
        Ok(storage) => Arc::new(storage),
        Err(e) => {
            tracing::error!("❌ K线存储创建失败(API) / Failed to create K-line storage (API): {}", e);
            std::process::exit(1);
        }
    };

    // 创建路由
    let api_router = router::create_router(
        db_storage,
        token_storage_for_api,
        orderbook_storage.clone(),
        kline_storage_for_api,
        price_service.clone(),
        config.clone(),
        solana_client.clone(),
    );

    // 创建 Swagger UI
    let swagger_ui = SwaggerUi::new("/swagger-ui")
        .url("/api-docs/openapi.json", docs::ApiDoc::openapi());

    // 组合所有路由 / Combine all routes
    let app = if let Some(layer) = socketio_layer {
        // 如果有Socket.IO层,添加到路由 / If Socket.IO layer exists, add to router
        Router::new()
            .merge(swagger_ui)
            .merge(api_router)
            .layer(cors)
            .layer(layer)
    } else {
        // 没有Socket.IO层 / No Socket.IO layer
        Router::new()
            .merge(swagger_ui)
            .merge(api_router)
            .layer(cors)
    };

    // 绑定地址
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

    tracing::info!("服务器启动成功！");
    tracing::info!("访问 http://localhost:{}/health 测试接口", config.server.port);
    tracing::info!("访问 http://localhost:{}/swagger-ui 查看 API 文档", config.server.port);
    tracing::info!("访问 http://localhost:{}/db/* 测试数据库接口", config.server.port);

    if config.kline.enable_kline_service {
        tracing::info!("📊 K线 WebSocket 服务:");
        tracing::info!("  WS   ws://{}:{}/kline - 实时K线数据订阅 / Real-time K-line data subscription", config.server.host, config.server.port);
        tracing::info!("  事件 / Events: subscribe, unsubscribe, history, kline_data, event_data");
        tracing::info!("  支持间隔 / Supported intervals: s1, s30, m5");
    }

    // 启动服务器
    axum::serve(listener, app).await.unwrap();
}
