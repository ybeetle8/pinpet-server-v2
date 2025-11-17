// 事件监听器模块 / Event listener module
use super::client::SolanaClient;
use super::events::{EventParser, PinpetEvent};
use crate::config::SolanaConfig;
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rand;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{interval, sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use chrono;

/// 事件监听器trait / Event listener trait
#[async_trait]
pub trait EventListener {
    async fn start(&mut self) -> anyhow::Result<()>;
    #[allow(dead_code)]
    async fn stop(&mut self) -> anyhow::Result<()>;
    fn is_running(&self) -> bool;
}

/// 事件处理器trait / Event handler trait
#[async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle_event(&self, event: PinpetEvent) -> anyhow::Result<()>;

    /// 向下转型支持trait对象 / Downcast support for trait objects
    fn as_any(&self) -> &dyn std::any::Any;
}

/// 默认事件处理器 - 简单打印事件 / Default event handler - simply print events
pub struct DefaultEventHandler;

#[async_trait]
impl EventHandler for DefaultEventHandler {
    async fn handle_event(&self, event: PinpetEvent) -> anyhow::Result<()> {
        match event {
            PinpetEvent::TokenCreated(e) => {
                info!(
                    "🪙 Token创建事件 / Token creation event: {} 创建了代币 / created token {}",
                    e.payer, e.mint_account
                );
                info!("   - 代币名称 / Token name: {}", e.name);
                info!("   - 代币符号 / Token symbol: {}", e.symbol);
                info!("   - Curve账户 / Curve account: {}", e.curve_account);
                info!("   - 最新价格 / Latest price: {}", e.latest_price);
                info!("   - 交易签名 / Transaction signature: {}", e.signature);
                info!("   - 区块高度 / Block height: {}", e.slot);
            }
            PinpetEvent::BuySell(e) => {
                let action = if e.is_buy { "买入/bought" } else { "卖出/sold" };
                info!(
                    "💰 交易事件 / Trade event: {} {} 代币 / token {} (代币数量/token amount: {}, SOL数量/SOL amount: {})",
                    e.payer, action, e.mint_account, e.token_amount, e.sol_amount
                );
                info!("   - 最新价格 / Latest price: {}", e.latest_price);
                info!("   - 清算索引 / Liquidation indices: {:?}", e.liquidate_indices);
                info!("   - 交易签名 / Transaction signature: {}", e.signature);
                info!("   - 区块高度 / Block height: {}", e.slot);
            }
            PinpetEvent::LongShort(e) => {
                let direction = if e.order_type == 1 { "做多/long" } else { "做空/short" };
                info!(
                    "📈 多空事件 / Long/Short event: {} {} 代币 / on token {} (订单ID / order ID: {})",
                    e.payer, direction, e.mint_account, e.order_id
                );
                info!("   - 用户 / User: {}", e.user);
                info!("   - 保证金SOL数量 / Margin SOL amount: {}", e.margin_sol_amount);
                info!("   - 借款数量 / Borrow amount: {}", e.borrow_amount);
                info!("   - 锁定LP开始价 / Lock LP start price: {}", e.lock_lp_start_price);
                info!("   - 锁定LP结束价 / Lock LP end price: {}", e.lock_lp_end_price);
                info!("   - 开始时间 / Start time: {}", e.start_time);
                info!("   - 结束时间 / End time: {}", e.end_time);
                info!("   - 清算索引 / Liquidation indices: {:?}", e.liquidate_indices);
                info!("   - 交易签名 / Transaction signature: {}", e.signature);
                info!("   - 区块高度 / Block height: {}", e.slot);
            }
            PinpetEvent::FullClose(e) => {
                let direction = if e.is_close_long { "多单/long" } else { "空单/short" };
                info!(
                    "🔒 全平仓事件 / Full close event: {} 平仓了 / closed {} 订单 / order {} 代币 / on token {} (收益/profit: {})",
                    e.payer, direction, e.order_id, e.mint_account, e.user_close_profit
                );
                info!("   - 最终代币数量 / Final token amount: {}", e.final_token_amount);
                info!("   - 最终SOL数量 / Final SOL amount: {}", e.final_sol_amount);
                info!("   - 最新价格 / Latest price: {}", e.latest_price);
                info!("   - 清算索引 / Liquidation indices: {:?}", e.liquidate_indices);
                info!("   - 交易签名 / Transaction signature: {}", e.signature);
                info!("   - 区块高度 / Block height: {}", e.slot);
            }
            PinpetEvent::PartialClose(e) => {
                let direction = if e.is_close_long { "多单/long" } else { "空单/short" };
                info!("🔓 部分平仓事件 / Partial close event: {} 部分平仓了 / partially closed {} 订单 / order {} 代币 / on token {} (收益/profit: {})",
                      e.payer, direction, e.order_id, e.mint_account, e.user_close_profit);
                info!("   - 最终代币数量 / Final token amount: {}", e.final_token_amount);
                info!("   - 最终SOL数量 / Final SOL amount: {}", e.final_sol_amount);
                info!("   - 最新价格 / Latest price: {}", e.latest_price);
                info!("   - 剩余持仓 / Remaining position: {}", e.position_asset_amount);
                info!("   - 清算索引 / Liquidation indices: {:?}", e.liquidate_indices);
                info!("   - 交易签名 / Transaction signature: {}", e.signature);
                info!("   - 区块高度 / Block height: {}", e.slot);
            }
            PinpetEvent::MilestoneDiscount(e) => {
                info!(
                    "💲 里程碑折扣事件 / Milestone discount event: {} 更新了代币 / updated fees for token {}",
                    e.payer, e.mint_account
                );
                info!("   - 现货交易费 / Swap fee: {}", e.swap_fee);
                info!("   - 保证金交易费 / Borrow fee: {}", e.borrow_fee);
                info!(
                    "   - 费用折扣标志 / Fee discount flag: {} (0: 原价/original, 1: 5折/50%, 2: 2.5折/25%, 3: 1.25折/12.5%)",
                    e.fee_discount_flag
                );
                info!("   - 交易签名 / Transaction signature: {}", e.signature);
                info!("   - 区块高度 / Block height: {}", e.slot);
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// 改进的Solana事件监听器，具有强大的重连功能 / Improved Solana event listener with robust reconnection
pub struct SolanaEventListener {
    config: SolanaConfig,
    client: Arc<SolanaClient>,
    event_parser: EventParser,
    event_handler: Arc<dyn EventHandler>,
    // 使用广播通道避免"通道已关闭"错误 / Use broadcast channel to avoid "channel closed" errors
    event_broadcaster: broadcast::Sender<PinpetEvent>,
    connection_state: Arc<tokio::sync::RwLock<ConnectionState>>,
    reconnect_attempts: Arc<tokio::sync::RwLock<u32>>,
    should_stop: Arc<tokio::sync::RwLock<bool>>,
    processed_signatures: Arc<tokio::sync::RwLock<HashSet<String>>>,
    is_running: bool,
}

impl SolanaEventListener {
    /// 记录原始Solana消息到单独文件用于调试 / Log raw Solana message to separate file for debugging
    async fn log_raw_message(message: &str, config: &SolanaConfig) {
        if !config.enable_raw_message_logging {
            return;
        }

        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f UTC");
        let log_line = format!("[{}] {}\n", timestamp, message);

        // 创建logs目录如果不存在 / Create logs directory if it doesn't exist
        if let Err(e) = tokio::fs::create_dir_all("logs").await {
            warn!("创建logs目录失败 / Failed to create logs directory: {}", e);
            return;
        }

        // 追加到原始消息日志文件 / Append to raw messages log file
        match OpenOptions::new()
            .create(true)
            .append(true)
            .open("logs/solana_raw_messages.log")
            .await
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(log_line.as_bytes()).await {
                    warn!("写入原始消息到日志文件失败 / Failed to write raw message to log file: {}", e);
                }
            }
            Err(e) => {
                warn!("打开原始消息日志文件失败 / Failed to open raw messages log file: {}", e);
            }
        }
    }

    /// 创建新的事件监听器 / Create new event listener
    pub fn new(
        config: SolanaConfig,
        client: Arc<SolanaClient>,
        event_handler: Arc<dyn EventHandler>,
    ) -> anyhow::Result<Self> {
        let event_parser = EventParser::new(&config.program_id)?;
        let (event_broadcaster, _) = broadcast::channel(1000);

        Ok(Self {
            config,
            client,
            event_parser,
            event_handler,
            event_broadcaster,
            connection_state: Arc::new(tokio::sync::RwLock::new(ConnectionState::Disconnected)),
            reconnect_attempts: Arc::new(tokio::sync::RwLock::new(0)),
            should_stop: Arc::new(tokio::sync::RwLock::new(false)),
            processed_signatures: Arc::new(tokio::sync::RwLock::new(HashSet::new())),
            is_running: false,
        })
    }

    /// 使用广播通道启动事件处理器 / Start event processor using broadcast channel
    async fn start_event_processor(&self) -> anyhow::Result<()> {
        let mut event_receiver = self.event_broadcaster.subscribe();
        let handler = Arc::clone(&self.event_handler);
        let should_stop = Arc::clone(&self.should_stop);

        tokio::spawn(async move {
            info!("🎯 事件处理器启动，使用广播通道 / Event processor started with broadcast channel");

            loop {
                tokio::select! {
                    event_result = event_receiver.recv() => {
                        match event_result {
                            Ok(event) => {
                                if let Err(e) = handler.handle_event(event).await {
                                    error!("处理事件失败 / Failed to process event: {}", e);
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                                warn!("事件处理器延迟，跳过了{}个事件 / Event processor lagged, skipped {} events", skipped, skipped);
                                continue;
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                info!("事件广播器关闭，停止处理器 / Event broadcaster closed, stopping processor");
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        if *should_stop.read().await {
                            info!("事件处理器收到停止信号 / Event processor received stop signal");
                            break;
                        }
                    }
                }
            }

            info!("🎯 事件处理器停止 / Event processor stopped");
        });

        Ok(())
    }

    /// 带自动重连的主连接循环 / Main connection loop with automatic reconnection
    async fn connection_loop(&self) -> anyhow::Result<()> {
        let config = self.config.clone();
        let client = Arc::clone(&self.client);
        let event_parser = self.event_parser.clone();
        let event_broadcaster = self.event_broadcaster.clone();
        let connection_state = Arc::clone(&self.connection_state);
        let reconnect_attempts = Arc::clone(&self.reconnect_attempts);
        let should_stop = Arc::clone(&self.should_stop);
        let processed_signatures = Arc::clone(&self.processed_signatures);

        tokio::spawn(async move {
            info!("🔄 启动连接循环 / Starting connection loop");

            loop {
                // 检查是否应该停止 / Check if we should stop
                if *should_stop.read().await {
                    info!("连接循环收到停止信号 / Connection loop received stop signal");
                    break;
                }

                *connection_state.write().await = ConnectionState::Connecting;
                info!("🔌 尝试连接WebSocket / Attempting to connect to WebSocket: {}", config.ws_url);

                match Self::connect_and_listen(
                    &config,
                    &client,
                    &event_parser,
                    &event_broadcaster,
                    &connection_state,
                    &should_stop,
                    &processed_signatures,
                )
                .await
                {
                    Ok(()) => {
                        info!("✅ WebSocket连接正常完成 / WebSocket connection completed normally");
                        *reconnect_attempts.write().await = 0;
                    }
                    Err(e) => {
                        error!("❌ WebSocket连接失败 / WebSocket connection failed: {}", e);
                        let mut attempts = reconnect_attempts.write().await;
                        *attempts += 1;

                        if *attempts > config.max_reconnect_attempts {
                            error!(
                                "❌ 超过最大重连次数 / Max reconnection attempts ({}) exceeded",
                                config.max_reconnect_attempts
                            );
                            *connection_state.write().await = ConnectionState::Disconnected;
                            break;
                        }

                        *connection_state.write().await = ConnectionState::Reconnecting;

                        // 指数退避与抖动 / Exponential backoff with jitter
                        let base_delay = config.reconnect_interval;
                        let exponential_delay =
                            std::cmp::min(base_delay * 2_u64.pow((*attempts - 1).min(5)), 60);
                        let jitter = (rand::random::<f64>() * 2.0) as u64;
                        let delay = exponential_delay + jitter;

                        warn!(
                            "🔄 重连尝试 / Reconnection attempt {} of {} in {} seconds",
                            *attempts, config.max_reconnect_attempts, delay
                        );

                        drop(attempts);
                        sleep(Duration::from_secs(delay)).await;
                    }
                }
            }

            *connection_state.write().await = ConnectionState::Disconnected;
            info!("🔄 连接循环结束 / Connection loop ended");
        });

        Ok(())
    }

    /// 连接并监听WebSocket / Connect and listen to WebSocket
    async fn connect_and_listen(
        config: &SolanaConfig,
        client: &Arc<SolanaClient>,
        event_parser: &EventParser,
        event_broadcaster: &broadcast::Sender<PinpetEvent>,
        connection_state: &Arc<tokio::sync::RwLock<ConnectionState>>,
        should_stop: &Arc<tokio::sync::RwLock<bool>>,
        processed_signatures: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    ) -> anyhow::Result<()> {
        let (ws_stream, _) = connect_async(&config.ws_url).await?;
        info!("🔗 WebSocket连接成功 / WebSocket connected successfully");

        *connection_state.write().await = ConnectionState::Connected;

        let (mut write, mut read) = ws_stream.split();

        // 订阅程序日志 / Subscribe to program logs
        let subscribe_request = json!({
            "jsonrpc": "2.0",
            "id": Uuid::new_v4().to_string(),
            "method": "logsSubscribe",
            "params": [
                {
                    "mentions": [config.program_id]
                },
                {
                    "commitment": config.commitment
                }
            ]
        });

        let subscribe_msg = Message::Text(subscribe_request.to_string());
        write.send(subscribe_msg).await?;
        info!("📡 订阅程序日志 / Subscribed to program logs: {}", config.program_id);

        // 用于ping和其他操作的共享写入器 / Shared writer for ping and other operations
        let shared_writer = Arc::new(Mutex::new(write));
        let (ping_stop_sender, mut ping_stop_receiver) = mpsc::unbounded_channel::<()>();

        // 启动ping任务 / Start ping task
        let ping_writer = Arc::clone(&shared_writer);
        let ping_should_stop = Arc::clone(should_stop);
        let ping_config = config.clone();
        tokio::spawn(async move {
            info!(
                "💓 启动ping任务(每{}秒) / Starting ping task (every {} seconds)",
                ping_config.ping_interval_seconds, ping_config.ping_interval_seconds
            );
            let mut ping_interval =
                interval(Duration::from_secs(ping_config.ping_interval_seconds));
            ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            let mut consecutive_failures = 0u32;
            const MAX_PING_FAILURES: u32 = 3;

            loop {
                tokio::select! {
                    _ = ping_interval.tick() => {
                        if *ping_should_stop.read().await {
                            break;
                        }

                        let mut writer = ping_writer.lock().await;
                        match writer.send(Message::Ping(vec![])).await {
                            Ok(()) => {
                                consecutive_failures = 0;
                                debug!("💓 Ping发送成功 / Ping sent successfully");
                            }
                            Err(e) => {
                                consecutive_failures += 1;
                                warn!("💓 Ping失败 / Ping failed ({}): {}", consecutive_failures, e);

                                if consecutive_failures >= MAX_PING_FAILURES {
                                    error!("💓 太多ping失败，连接可能已断开 / Too many ping failures, connection seems dead");
                                    break;
                                }
                            }
                        }
                    }
                    _ = ping_stop_receiver.recv() => {
                        info!("💓 Ping任务收到停止信号 / Ping task received stop signal");
                        break;
                    }
                }
            }
            info!("💓 Ping任务停止 / Ping task stopped");
        });

        // 消息处理循环 / Message handling loop
        let event_broadcaster_clone = event_broadcaster.clone();
        let event_parser_clone = event_parser.clone();
        let client_clone = Arc::clone(client);
        let processed_signatures_clone = Arc::clone(processed_signatures);
        let should_stop_clone = Arc::clone(should_stop);

        info!("🎧 开始监听WebSocket消息 / Starting to listen for WebSocket messages");
        while let Some(msg) = read.next().await {
            // 检查停止信号 / Check stop signal
            if *should_stop_clone.read().await {
                info!("消息监听器收到停止信号 / Message listener received stop signal");
                break;
            }

            match msg {
                Ok(Message::Text(text)) => {
                    debug!("📨 收到文本消息 / Received text message");

                    // 记录原始消息如果启用 / Log raw message if enabled
                    Self::log_raw_message(&text, config).await;

                    if let Err(e) = Self::handle_websocket_message(
                        &text,
                        &event_parser_clone,
                        &event_broadcaster_clone,
                        &client_clone,
                        &processed_signatures_clone,
                        config,
                    )
                    .await
                    {
                        error!("处理WebSocket消息失败 / Failed to process WebSocket message: {}", e);
                    }
                }
                Ok(Message::Close(_)) => {
                    warn!("🎧 WebSocket连接被服务器关闭 / WebSocket connection closed by server");
                    break;
                }
                Ok(Message::Ping(data)) => {
                    debug!("🏓 收到ping，响应pong / Received ping, responding with pong");
                    let mut writer = shared_writer.lock().await;
                    if let Err(e) = writer.send(Message::Pong(data)).await {
                        warn!("发送pong失败 / Failed to send pong: {}", e);
                        break;
                    }
                }
                Ok(Message::Pong(_)) => {
                    debug!("🏓 收到pong - 连接活跃 / Received pong - connection alive");
                }
                Err(e) => {
                    error!("🎧 WebSocket错误 / WebSocket error: {}", e);
                    break;
                }
                _ => {
                    debug!("收到其他消息类型 / Received other message type");
                }
            }
        }

        // 停止ping任务 / Stop ping task
        let _ = ping_stop_sender.send(());
        warn!("🎧 WebSocket消息监听器结束 / WebSocket message listener ended");

        Ok(())
    }

    /// 处理WebSocket消息 / Handle WebSocket messages
    async fn handle_websocket_message(
        message: &str,
        event_parser: &EventParser,
        event_broadcaster: &broadcast::Sender<PinpetEvent>,
        client: &Arc<SolanaClient>,
        processed_signatures: &Arc<tokio::sync::RwLock<HashSet<String>>>,
        config: &SolanaConfig,
    ) -> anyhow::Result<()> {
        debug!("📨 处理WebSocket消息 / Processing WebSocket message");

        let json_msg: Value = serde_json::from_str(message)?;

        // 检查订阅确认 / Check subscription confirmation
        if let Some(result) = json_msg.get("result") {
            if json_msg.get("params").is_none() {
                info!("✅ 订阅确认 / Subscription confirmed: ID = {}", result);
                return Ok(());
            }
        }

        // 处理日志通知 / Handle log notifications
        if let Some(params) = json_msg.get("params") {
            if let Some(result) = params.get("result") {
                let slot = result
                    .get("context")
                    .and_then(|ctx| ctx.get("slot"))
                    .and_then(|s| s.as_u64())
                    .unwrap_or(0);

                if let Some(value) = result.get("value") {
                    let signature = match value.get("signature").and_then(|s| s.as_str()) {
                        Some(sig) => sig,
                        None => {
                            warn!("消息中没有签名 / No signature found in message");
                            return Ok(());
                        }
                    };

                    // 检查交易成功 / Check transaction success
                    let transaction_error = value.get("err");
                    let is_transaction_success =
                        transaction_error.is_none() || transaction_error == Some(&Value::Null);

                    if !is_transaction_success {
                        if let Some(error_detail) = transaction_error {
                            debug!(
                                "❌ 交易{}失败，错误: {} / Transaction {} failed with error: {}",
                                signature, error_detail, signature, error_detail
                            );
                        } else {
                            debug!("❌ 交易{}失败，未知错误 / Transaction {} failed with unknown error", signature, signature);
                        }

                        // 跳过失败的交易除非明确配置处理它们 / Skip failed transactions unless configured
                        if !config.process_failed_transactions {
                            debug!("⏭️ 跳过失败交易{} (process_failed_transactions=false) / Skipping failed transaction {} (process_failed_transactions=false)", signature, signature);
                            return Ok(());
                        } else {
                            debug!("🔄 处理失败交易{} (process_failed_transactions=true) / Processing failed transaction {} (process_failed_transactions=true)", signature, signature);
                        }
                    }

                    // 检查是否已处理 / Check if already processed
                    {
                        let mut processed = processed_signatures.write().await;
                        if processed.contains(signature) {
                            debug!("签名{}已处理 / Signature {} already processed", signature, signature);
                            return Ok(());
                        }
                        processed.insert(signature.to_string());
                    }

                    // 处理日志 / Process logs
                    if let Some(logs_array) = value.get("logs").and_then(|l| l.as_array()) {
                        let logs: Vec<String> = logs_array
                            .iter()
                            .filter_map(|l| l.as_str())
                            .map(|s| s.to_string())
                            .collect();

                        let mut all_events = Vec::new();

                        // 从日志解析事件 / Parse events from logs
                        match event_parser.parse_events_with_call_stack(&logs, signature, slot) {
                            Ok(events) => {
                                all_events.extend(events);
                            }
                            Err(e) => {
                                debug!("从日志解析事件失败 / Failed to parse events from logs: {}", e);
                            }
                        }

                        // 如果需要处理CPI调用 / Handle CPI calls if needed
                        let has_cpi = logs.iter().any(|log| {
                            log.contains("invoke [2]")
                                || log.contains("invoke [3]")
                                || log.contains("invoke [4]")
                        });

                        if has_cpi {
                            info!("检测到CPI调用，获取完整交易详情 / Detected CPI calls, fetching full transaction details");

                            match client.get_transaction_with_logs(signature).await {
                                Ok(tx_details) => {
                                    if let Some(meta) =
                                        tx_details.get("meta").and_then(|m| m.as_object())
                                    {
                                        if let Some(full_logs) =
                                            meta.get("logMessages").and_then(|l| l.as_array())
                                        {
                                            let full_log_strings: Vec<String> = full_logs
                                                .iter()
                                                .filter_map(|l| l.as_str())
                                                .map(|s| s.to_string())
                                                .collect();

                                            match event_parser.parse_events_with_call_stack(
                                                &full_log_strings,
                                                signature,
                                                slot,
                                            ) {
                                                Ok(events) => {
                                                    for event in events {
                                                        if !Self::event_exists_in_list(
                                                            &all_events,
                                                            &event,
                                                        ) {
                                                            all_events.push(event);
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    error!("解析完整交易事件失败 / Failed to parse full transaction events: {}", e);
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!("获取交易详情失败 / Failed to get transaction details: {}", e);
                                }
                            }
                        }

                        // 广播事件 / Broadcast events
                        if !all_events.is_empty() {
                            info!(
                                "✅ 广播{}个事件，交易 / Broadcasting {} events for transaction {}",
                                all_events.len(), all_events.len(),
                                signature
                            );

                            for event in all_events {
                                if let Err(e) = event_broadcaster.send(event) {
                                    error!("广播事件失败 / Failed to broadcast event: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn event_exists_in_list(events: &[PinpetEvent], new_event: &PinpetEvent) -> bool {
        events.iter().any(|e| Self::events_are_equal(e, new_event))
    }

    fn events_are_equal(e1: &PinpetEvent, e2: &PinpetEvent) -> bool {
        use PinpetEvent::*;
        match (e1, e2) {
            (TokenCreated(a), TokenCreated(b)) => a.signature == b.signature,
            (BuySell(a), BuySell(b)) => a.signature == b.signature,
            (LongShort(a), LongShort(b)) => {
                a.signature == b.signature && a.order_id == b.order_id
            }
            (PartialClose(a), PartialClose(b)) => {
                a.signature == b.signature && a.order_id == b.order_id
            }
            (FullClose(a), FullClose(b)) => {
                a.signature == b.signature && a.order_id == b.order_id
            }
            (MilestoneDiscount(a), MilestoneDiscount(b)) => a.signature == b.signature,
            _ => false,
        }
    }

    #[allow(dead_code)]
    pub async fn get_connection_health(&self) -> serde_json::Value {
        let processed_count = self.processed_signatures.read().await.len();
        let current_attempts = *self.reconnect_attempts.read().await;
        let connection_state = self.connection_state.read().await.clone();

        serde_json::json!({
            "is_running": self.is_running,
            "connection_state": format!("{:?}", connection_state),
            "reconnect_attempts": current_attempts,
            "max_reconnect_attempts": self.config.max_reconnect_attempts,
            "should_stop": *self.should_stop.read().await,
            "ws_url": self.config.ws_url,
            "program_id": self.config.program_id,
            "processed_signatures_count": processed_count,
            "ping_interval_seconds": self.config.ping_interval_seconds
        })
    }
}

#[async_trait]
impl EventListener for SolanaEventListener {
    async fn start(&mut self) -> anyhow::Result<()> {
        if self.is_running {
            warn!("事件监听器已在运行 / Event listener is already running");
            return Ok(());
        }

        info!("🚀 启动改进的Solana事件监听器 / Starting improved Solana event listener");

        // 重置停止信号 / Reset stop signal
        *self.should_stop.write().await = false;

        // 检查RPC连接 / Check RPC connection
        if !self.client.check_connection().await? {
            return Err(anyhow::anyhow!("无法连接到Solana RPC / Cannot connect to Solana RPC"));
        }

        // 启动事件处理器 / Start event processor
        self.start_event_processor().await?;

        // 启动连接循环 / Start connection loop
        self.connection_loop().await?;

        self.is_running = true;
        info!("✅ 改进的Solana事件监听器启动成功 / Improved Solana event listener started successfully");

        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        if !self.is_running {
            warn!("事件监听器未在运行 / Event listener is not running");
            return Ok(());
        }

        info!("🛑 停止改进的Solana事件监听器 / Stopping improved Solana event listener");

        // 设置停止信号 / Set stop signal
        *self.should_stop.write().await = true;

        // 允许一些时间优雅关闭 / Allow some time for graceful shutdown
        sleep(Duration::from_secs(2)).await;

        self.is_running = false;
        info!("✅ 改进的Solana事件监听器停止成功 / Improved Solana event listener stopped successfully");

        Ok(())
    }

    fn is_running(&self) -> bool {
        self.is_running
    }
}

pub struct EventListenerManager {
    listener: Option<SolanaEventListener>,
}

impl EventListenerManager {
    pub fn new() -> Self {
        Self { listener: None }
    }

    pub fn initialize(
        &mut self,
        config: SolanaConfig,
        client: Arc<SolanaClient>,
        event_handler: Arc<dyn EventHandler>,
    ) -> anyhow::Result<()> {
        self.listener = Some(SolanaEventListener::new(config, client, event_handler)?);

        Ok(())
    }

    pub async fn start(&mut self) -> anyhow::Result<()> {
        if let Some(listener) = &mut self.listener {
            listener.start().await
        } else {
            Err(anyhow::anyhow!("事件监听器未初始化 / Event listener not initialized"))
        }
    }

    #[allow(dead_code)]
    pub async fn stop(&mut self) -> anyhow::Result<()> {
        if let Some(listener) = &mut self.listener {
            listener.stop().await
        } else {
            Ok(())
        }
    }

    pub fn is_running(&self) -> bool {
        self.listener.as_ref().map_or(false, |l| l.is_running())
    }

    #[allow(dead_code)]
    pub async fn get_connection_health(&self) -> Option<serde_json::Value> {
        if let Some(listener) = &self.listener {
            Some(listener.get_connection_health().await)
        } else {
            None
        }
    }
}