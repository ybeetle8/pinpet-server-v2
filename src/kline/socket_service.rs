// K线实时推送 Socket.IO 服务 / K-line real-time push Socket.IO service
// 基于 SocketIoxide 0.17 实现 / Based on SocketIoxide 0.17

use crate::kline::{
    data_processor::KlineDataProcessor,
    subscription::SubscriptionManager,
    types::*,
};
use crate::db::EventStorage;
use crate::price::SolPriceService;
use crate::solana::PinpetEvent;
use anyhow::Result;
use chrono::Utc;
use socketioxide::extract::{Data, SocketRef};
use socketioxide::SocketIo;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// K线Socket服务 / K-line Socket service
pub struct KlineSocketService {
    socketio: SocketIo,                                      // Socket.IO实例 / Socket.IO instance
    event_storage: Arc<EventStorage>,                        // 事件存储 / Event storage
    subscriptions: Arc<RwLock<SubscriptionManager>>,         // 订阅管理器 / Subscription manager
    data_processor: Arc<KlineDataProcessor>,                 // 数据处理器 / Data processor
    #[allow(dead_code)]
    config: KlineConfig,                                     // 配置 / Configuration
}

impl KlineSocketService {
    /// 创建新的Socket服务并返回服务实例和Layer / Create new Socket service and return (Service, Layer)
    pub fn new(
        event_storage: Arc<EventStorage>,
        price_service: Arc<SolPriceService>,
        config: KlineConfig,
    ) -> Result<(Self, socketioxide::layer::SocketIoLayer)> {
        // 创建 SocketIoxide 实例 / Create SocketIoxide instance
        let (layer, io) = SocketIo::builder()
            .ping_interval(std::time::Duration::from_secs(config.ping_interval_secs))
            .ping_timeout(std::time::Duration::from_secs(config.ping_timeout_secs))
            .max_payload(1024 * 1024) // 1MB 最大负载 / 1MB max payload
            .build_layer();

        let data_processor = Arc::new(KlineDataProcessor::new(event_storage.clone(), price_service));

        let service = Self {
            socketio: io,
            event_storage: event_storage.clone(),
            subscriptions: Arc::new(RwLock::new(SubscriptionManager::new(
                config.max_subscriptions_per_client,
            ))),
            data_processor,
            config,
        };

        Ok((service, layer))
    }

    /// 设置Socket事件处理器 / Setup Socket event handlers
    pub fn setup_socket_handlers(&self) {
        let subscriptions = Arc::clone(&self.subscriptions);
        let event_storage = Arc::clone(&self.event_storage);
        let data_processor = Arc::clone(&self.data_processor);

        // 设置默认命名空间（避免default namespace not found错误）/ Setup default namespace (avoid default namespace not found error)
        self.socketio.ns("/", |_socket: SocketRef| {
            // 默认命名空间不做任何处理，只是为了避免错误 / Default namespace does nothing, just to avoid errors
        });

        // K线命名空间 - 合并所有事件处理器到一个命名空间 / K-line namespace - merge all event handlers into one namespace
        self.socketio.ns("/kline", {
            let subscriptions = subscriptions.clone();
            let _event_storage = event_storage.clone();
            let data_processor = data_processor.clone();

            move |socket: SocketRef| {
                info!("🔌 New client connected to /kline: {}", socket.id);

                // 保存 socket_id 用于后续使用 / Save socket_id for later use
                let socket_id = socket.id.to_string();

                // 注册客户端连接 / Register client connection
                {
                    let subscriptions = subscriptions.clone();
                    let socket_id_clone = socket_id.clone();
                    tokio::spawn(async move {
                        let mut manager = subscriptions.write().await;
                        manager.add_connection(socket_id_clone);
                    });
                }

                // 发送连接成功消息 / Send connection success message
                let welcome_msg = serde_json::json!({
                    "client_id": socket_id,
                    "server_time": Utc::now().timestamp(),
                    "supported_symbols": [],
                    "supported_intervals": ["s1", "s30", "m5"]
                });

                if let Err(e) = socket.emit("connection_success", &welcome_msg) {
                    warn!("Failed to send welcome message: {}", e);
                }

                // 订阅事件处理器 / Subscribe event handler
                socket.on("subscribe", {
                    let subscriptions = subscriptions.clone();
                    let data_processor = data_processor.clone();

                    move |socket: SocketRef, Data(data): Data<SubscribeRequest>| {
                        let subscriptions = subscriptions.clone();
                        let data_processor = data_processor.clone();

                        tokio::spawn(async move {
                            info!(
                                "📊 Subscribe request from {}: {} {}",
                                socket.id, data.symbol, data.interval
                            );

                            // 更新客户端活动 / Update client activity
                            {
                                let mut manager = subscriptions.write().await;
                                manager.update_activity(&socket.id.to_string());
                            }

                            // 验证订阅请求 / Validate subscribe request
                            if let Err(e) = validate_subscribe_request(&data) {
                                let _ = socket.emit(
                                    "error",
                                    &serde_json::json!({
                                        "code": 1001,
                                        "message": e.to_string()
                                    }),
                                );
                                return;
                            }

                            // 添加订阅 / Add subscription
                            {
                                let mut manager = subscriptions.write().await;
                                if let Err(e) = manager.add_subscription(
                                    &socket.id.to_string(),
                                    &data.symbol,
                                    &data.interval,
                                ) {
                                    let _ = socket.emit(
                                        "error",
                                        &serde_json::json!({
                                            "code": 1002,
                                            "message": e.to_string()
                                        }),
                                    );
                                    return;
                                }

                                // 更新活动时间 / Update activity time
                                manager.update_activity(&socket.id.to_string());
                            }

                            // 加入对应的房间 / Join corresponding room
                            let room_name = format!("kline:{}:{}", data.symbol, data.interval);
                            info!("🏠 Client {} joining room: {}", socket.id, room_name);
                            socket.join(room_name.clone());

                            // 检查订阅者状态 / Check subscriber status
                            {
                                let manager = subscriptions.read().await;
                                let subscribers =
                                    manager.get_subscribers(&data.symbol, &data.interval);
                                info!(
                                    "📈 Current subscribers for {}:{}: {:?}",
                                    data.symbol, data.interval, subscribers
                                );
                                info!("📋 Total active connections: {}", manager.connections.len());
                            }

                            // 推送历史K线数据 / Push historical K-line data
                            // 使用客户端传入的limit，默认100，最大1000
                            // Use client-provided limit, default 100, max 1000
                            let history_limit = data.limit.unwrap_or(100).min(1000);
                            if let Ok(history) = data_processor
                                .get_kline_history(&data.symbol, &data.interval, history_limit)
                                .await
                            {
                                if let Err(e) = socket.emit("history_data", &history) {
                                    warn!("Failed to send history data: {}", e);
                                } else {
                                    // 更新历史数据发送计数 / Update history data sent count
                                    {
                                        let mut manager = subscriptions.write().await;
                                        manager.increment_history_data_sent(&socket.id.to_string());
                                    }
                                }
                            }

                            // 推送历史交易事件数据 (300条) / Push historical event data (300 records)
                            info!("📡 Sending historical event data for mint: {}", data.symbol);
                            if let Ok(event_history) = data_processor
                                .get_event_history(&data.symbol, 300)
                                .await
                            {
                                if let Err(e) = socket.emit("history_event_data", &event_history) {
                                    warn!("Failed to send history event data: {}", e);
                                } else {
                                    info!(
                                        "✅ Successfully sent {} historical events for mint: {}",
                                        event_history.data.len(),
                                        data.symbol
                                    );
                                    // 更新历史数据发送计数 / Update history data sent count
                                    {
                                        let mut manager = subscriptions.write().await;
                                        manager.increment_history_data_sent(&socket.id.to_string());
                                    }
                                }
                            } else {
                                warn!("❌ Failed to get historical event data for mint: {}", data.symbol);
                            }

                            // 确认订阅成功 / Confirm subscription success
                            let _ = socket.emit(
                                "subscription_confirmed",
                                &serde_json::json!({
                                    "symbol": data.symbol,
                                    "interval": data.interval,
                                    "subscription_id": data.subscription_id,
                                    "success": true,
                                    "message": "订阅成功 / Subscription successful"
                                }),
                            );
                        });
                    }
                });

                // 取消订阅事件处理器 / Unsubscribe event handler
                socket.on("unsubscribe", {
                    let subscriptions = subscriptions.clone();

                    move |socket: SocketRef, Data(data): Data<UnsubscribeRequest>| {
                        let subscriptions = subscriptions.clone();

                        tokio::spawn(async move {
                            info!(
                                "🚫 Unsubscribe request from {}: {} {}",
                                socket.id, data.symbol, data.interval
                            );

                            // 移除订阅 / Remove subscription
                            {
                                let mut manager = subscriptions.write().await;
                                manager.remove_subscription(
                                    &socket.id.to_string(),
                                    &data.symbol,
                                    &data.interval,
                                );
                                manager.update_activity(&socket.id.to_string());
                            }

                            // 离开对应的房间 / Leave corresponding room
                            let room_name = format!("kline:{}:{}", data.symbol, data.interval);
                            socket.leave(room_name);

                            // 确认取消订阅 / Confirm unsubscribe success
                            let _ = socket.emit(
                                "unsubscribe_confirmed",
                                &serde_json::json!({
                                    "symbol": data.symbol,
                                    "interval": data.interval,
                                    "subscription_id": data.subscription_id,
                                    "success": true,
                                    "message": "取消订阅成功 / Unsubscribe successful"
                                }),
                            );
                        });
                    }
                });

                // 历史数据事件处理器 / History event handler
                socket.on("history", {
                    let data_processor = data_processor.clone();
                    let subscriptions = subscriptions.clone();

                    move |socket: SocketRef, Data(data): Data<HistoryRequest>| {
                        let data_processor = data_processor.clone();
                        let subscriptions = subscriptions.clone();

                        tokio::spawn(async move {
                            info!(
                                "📈 History request from {}: {} {}",
                                socket.id, data.symbol, data.interval
                            );

                            // 更新活动时间 / Update activity time
                            {
                                let mut manager = subscriptions.write().await;
                                manager.update_activity(&socket.id.to_string());
                            }

                            match data_processor
                                .get_kline_history(
                                    &data.symbol,
                                    &data.interval,
                                    data.limit.unwrap_or(100),
                                )
                                .await
                            {
                                Ok(history) => {
                                    if let Err(e) = socket.emit("history_data", &history) {
                                        warn!("Failed to send history data: {}", e);
                                    } else {
                                        // 更新历史数据发送计数 / Update history data sent count
                                        {
                                            let mut manager = subscriptions.write().await;
                                            manager.increment_history_data_sent(&socket.id.to_string());
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = socket.emit(
                                        "error",
                                        &serde_json::json!({
                                            "code": 1003,
                                            "message": e.to_string()
                                        }),
                                    );
                                }
                            }
                        });
                    }
                });

                // 连接断开事件处理器 / Disconnect event handler
                socket.on_disconnect({
                    let subscriptions = subscriptions.clone();

                    move |socket: SocketRef| {
                        let subscriptions = subscriptions.clone();

                        tokio::spawn(async move {
                            info!("🔌 Client disconnected: {}", socket.id);

                            // 清理客户端连接 / Clean up client connection
                            let mut manager = subscriptions.write().await;
                            manager.remove_client(&socket.id.to_string());
                        });
                    }
                });
            }
        });
    }

    /// 广播K线更新到订阅者 / Broadcast K-line update to subscribers
    pub async fn broadcast_kline_update(
        &self,
        mint_account: &str,
        interval: &str,
        kline_data: &KlineRealtimeData,
    ) -> Result<()> {
        let room_name = format!("kline:{}:{}", mint_account, interval);

        let update_message = KlineUpdateMessage {
            symbol: mint_account.to_string(),
            interval: interval.to_string(),
            subscription_id: None,
            data: kline_data.clone(),
            timestamp: Utc::now().timestamp_millis() as u64,
        };

        info!("📡 Broadcasting kline update to room: {}", room_name);
        debug!(
            "📊 Update message: time={}, open={}, high={}, low={}, close={}, volume={}, is_final={}, update_count={}",
            update_message.data.time,
            update_message.data.open,
            update_message.data.high,
            update_message.data.low,
            update_message.data.close,
            update_message.data.volume,
            update_message.data.is_final,
            update_message.data.update_count
        );

        // 在发送前检查房间中的实际连接 / Check actual connections in room before sending
        {
            let manager = self.subscriptions.read().await;
            let subscribers = manager.get_subscribers(mint_account, interval);
            info!(
                "📋 Room {} has {} subscribers: {:?}",
                room_name,
                subscribers.len(),
                subscribers
            );
        }

        // 发送到 /kline 命名空间的房间 / Send to /kline namespace room
        let result = self
            .socketio
            .of("/kline")
            .ok_or_else(|| anyhow::anyhow!("Namespace /kline not found"))?
            .to(room_name.clone())
            .emit("kline_data", &update_message)
            .await;

        match result {
            Ok(_) => {
                info!(
                    "✅ Successfully broadcasted kline update to room {}",
                    room_name
                );

                // 更新所有订阅了该房间的客户端的 kline_data 发送计数 / Update kline_data sent count for all clients in room
                {
                    let mut manager = self.subscriptions.write().await;
                    let subscribers = manager.get_subscribers(mint_account, interval);
                    for socket_id in subscribers {
                        manager.increment_kline_data_sent(&socket_id);
                    }
                }
            }
            Err(e) => {
                warn!("❌ Failed to broadcast to room {}: {}", room_name, e);
            }
        }

        Ok(())
    }

    /// 广播交易事件到订阅者 / Broadcast event update to subscribers
    pub async fn broadcast_event_update(&self, event: &PinpetEvent) -> Result<()> {
        let mint_account = KlineDataProcessor::get_mint_from_event(event);
        info!("📡 Broadcasting event update for mint: {}", mint_account);

        let event_type_name = KlineDataProcessor::get_event_type_name(event);
        let event_message = EventUpdateMessage {
            symbol: mint_account.clone(),
            event_type: event_type_name,
            event_data: event.clone(),
            timestamp: Utc::now().timestamp_millis() as u64,
        };

        // 使用相同的间隔广播到所有可能的间隔 / Use same intervals as K-line push - broadcast to all possible intervals
        let intervals = ["s1", "s30", "m5"];
        let mut broadcast_count = 0;

        for interval in intervals {
            let room_name = format!("kline:{}:{}", mint_account, interval);

            let result = self
                .socketio
                .of("/kline")
                .ok_or_else(|| anyhow::anyhow!("Namespace /kline not found"))?
                .to(room_name.clone())
                .emit("event_data", &event_message)
                .await;

            match result {
                Ok(_) => {
                    info!("✅ Successfully broadcasted event to room {}", room_name);
                    broadcast_count += 1;
                }
                Err(e) => {
                    warn!("❌ Failed to broadcast event to room {}: {}", room_name, e);
                }
            }
        }

        info!(
            "📡 Event broadcast completed for mint: {}, sent to {} rooms",
            mint_account, broadcast_count
        );
        Ok(())
    }

    /// 获取服务统计信息 / Get service statistics
    #[allow(dead_code)]
    pub async fn get_service_stats(&self) -> serde_json::Value {
        let manager = self.subscriptions.read().await;

        serde_json::json!({
            "active_connections": manager.connections.len(),
            "total_subscriptions": manager.client_subscriptions.values().map(|s| s.len()).sum::<usize>(),
            "monitored_mints": manager.mint_subscribers.len(),
            "config": {
                "max_subscriptions_per_client": self.config.max_subscriptions_per_client,
                "ping_interval": self.config.ping_interval_secs,
                "ping_timeout": self.config.ping_timeout_secs
            }
        })
    }

    /// 获取详细的订阅状态和通讯统计 / Get detailed subscription status and communication statistics
    #[allow(dead_code)]
    pub async fn get_subscription_details(&self) -> serde_json::Value {
        let manager = self.subscriptions.read().await;
        let now = Instant::now();

        let mut client_details = Vec::new();

        for (socket_id, client) in &manager.connections {
            let subscriptions: Vec<String> = client.subscriptions.iter().cloned().collect();
            let connection_duration = now.duration_since(client.connection_time).as_secs();
            let last_activity_ago = now.duration_since(client.last_activity).as_secs();

            client_details.push(serde_json::json!({
                "socket_id": socket_id,
                "subscriptions": subscriptions,
                "subscription_count": client.subscription_count,
                "connection_duration_seconds": connection_duration,
                "last_activity_seconds_ago": last_activity_ago,
                "message_stats": {
                    "kline_data_sent": client.kline_data_sent_count,
                    "history_data_sent": client.history_data_sent_count,
                    "total_messages_sent": client.total_messages_sent
                }
            }));
        }

        let mut room_details = Vec::new();

        for (mint, intervals) in &manager.mint_subscribers {
            for (interval, subscribers) in intervals {
                let room_name = format!("kline:{}:{}", mint, interval);
                room_details.push(serde_json::json!({
                    "room_name": room_name,
                    "mint": mint,
                    "interval": interval,
                    "subscriber_count": subscribers.len(),
                    "subscribers": subscribers.iter().cloned().collect::<Vec<String>>()
                }));
            }
        }

        serde_json::json!({
            "timestamp": Utc::now().timestamp(),
            "total_connections": manager.connections.len(),
            "total_rooms": room_details.len(),
            "clients": client_details,
            "rooms": room_details
        })
    }
}

/// 验证订阅请求 / Validate subscribe request
fn validate_subscribe_request(req: &SubscribeRequest) -> Result<()> {
    // 验证时间间隔 / Validate interval
    if !["s1", "s30", "m5"].contains(&req.interval.as_str()) {
        return Err(anyhow::anyhow!(
            "Invalid interval: {}, must be one of: s1, s30, m5",
            req.interval
        ));
    }

    // 验证symbol格式（基本的Solana地址格式检查）/ Validate symbol format (basic Solana address format check)
    if req.symbol.len() < 32 || req.symbol.len() > 44 {
        return Err(anyhow::anyhow!("Invalid symbol format"));
    }

    Ok(())
}
