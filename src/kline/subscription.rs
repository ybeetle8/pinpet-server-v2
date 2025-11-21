// 订阅管理器 / Subscription manager
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// 客户端连接信息 / Client connection information
#[derive(Debug, Clone)]
pub struct ClientConnection {
    pub socket_id: String,               // Socket ID
    pub subscriptions: HashSet<String>,  // "mint:interval" 格式的订阅键 / Subscription keys in "mint:interval" format
    pub last_activity: Instant,          // 最后活动时间 / Last activity time
    pub connection_time: Instant,        // 连接建立时间 / Connection establishment time
    pub subscription_count: usize,       // 当前订阅数量 / Current subscription count
    pub user_agent: Option<String>,      // 客户端信息 / Client user agent
    pub kline_data_sent_count: u64,      // kline_data 发送次数 / kline_data sent count
    pub history_data_sent_count: u64,    // history_data 发送次数 / history_data sent count
    pub total_messages_sent: u64,        // 总消息发送次数 / Total messages sent count
}

/// 订阅管理器 / Subscription manager
#[derive(Debug)]
pub struct SubscriptionManager {
    // 连接映射: SocketId -> 客户端信息 / Connection mapping: SocketId -> Client info
    pub connections: HashMap<String, ClientConnection>,

    // 订阅索引: mint_account -> interval -> SocketId集合 / Subscription index: mint -> interval -> SocketId set
    pub mint_subscribers: HashMap<String, HashMap<String, HashSet<String>>>,

    // 反向索引: SocketId -> 订阅键集合 (用于快速清理) / Reverse index: SocketId -> subscription keys (for fast cleanup)
    pub client_subscriptions: HashMap<String, HashSet<String>>,

    // 最大订阅数限制 / Max subscriptions limit
    pub max_subscriptions_per_client: usize,
}

impl SubscriptionManager {
    /// 创建新的订阅管理器 / Create new subscription manager
    pub fn new(max_subscriptions_per_client: usize) -> Self {
        Self {
            connections: HashMap::new(),
            mint_subscribers: HashMap::new(),
            client_subscriptions: HashMap::new(),
            max_subscriptions_per_client,
        }
    }

    /// 添加客户端连接 / Add client connection
    pub fn add_connection(&mut self, socket_id: String) {
        self.connections.insert(
            socket_id.clone(),
            ClientConnection {
                socket_id,
                subscriptions: HashSet::new(),
                last_activity: Instant::now(),
                connection_time: Instant::now(),
                subscription_count: 0,
                user_agent: None,
                kline_data_sent_count: 0,
                history_data_sent_count: 0,
                total_messages_sent: 0,
            },
        );
    }

    /// 添加订阅 / Add subscription
    pub fn add_subscription(&mut self, socket_id: &str, mint: &str, interval: &str) -> Result<()> {
        // 检查客户端是否存在 / Check if client exists
        let client = self
            .connections
            .get_mut(socket_id)
            .ok_or_else(|| anyhow::anyhow!("Client not found"))?;

        // 检查订阅数量限制 / Check subscription limit
        if client.subscription_count >= self.max_subscriptions_per_client {
            return Err(anyhow::anyhow!("Subscription limit exceeded"));
        }

        let subscription_key = format!("{}:{}", mint, interval);

        // 添加到客户端订阅列表 / Add to client subscription list
        if client.subscriptions.insert(subscription_key.clone()) {
            client.subscription_count += 1;

            // 添加到全局索引 / Add to global index
            self.mint_subscribers
                .entry(mint.to_string())
                .or_default()
                .entry(interval.to_string())
                .or_default()
                .insert(socket_id.to_string());

            // 添加到反向索引 / Add to reverse index
            self.client_subscriptions
                .entry(socket_id.to_string())
                .or_default()
                .insert(subscription_key);
        }

        Ok(())
    }

    /// 移除订阅 / Remove subscription
    pub fn remove_subscription(&mut self, socket_id: &str, mint: &str, interval: &str) {
        let subscription_key = format!("{}:{}", mint, interval);

        // 从客户端订阅列表移除 / Remove from client subscription list
        if let Some(client) = self.connections.get_mut(socket_id) {
            if client.subscriptions.remove(&subscription_key) {
                client.subscription_count = client.subscription_count.saturating_sub(1);
            }
        }

        // 从全局索引移除 / Remove from global index
        if let Some(interval_map) = self.mint_subscribers.get_mut(mint) {
            if let Some(client_set) = interval_map.get_mut(interval) {
                client_set.remove(socket_id);

                if client_set.is_empty() {
                    interval_map.remove(interval);
                }
            }

            if interval_map.is_empty() {
                self.mint_subscribers.remove(mint);
            }
        }

        // 从反向索引移除 / Remove from reverse index
        if let Some(subscriptions) = self.client_subscriptions.get_mut(socket_id) {
            subscriptions.remove(&subscription_key);
        }
    }

    /// 获取订阅者列表 / Get subscribers list
    pub fn get_subscribers(&self, mint: &str, interval: &str) -> Vec<String> {
        self.mint_subscribers
            .get(mint)
            .and_then(|interval_map| interval_map.get(interval))
            .map(|client_set| client_set.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// 移除客户端 / Remove client
    pub fn remove_client(&mut self, socket_id: &str) {
        // 获取该客户端的所有订阅 / Get all subscriptions of this client
        if let Some(subscriptions) = self.client_subscriptions.remove(socket_id) {
            for subscription_key in subscriptions {
                let parts: Vec<&str> = subscription_key.split(':').collect();
                if parts.len() == 2 {
                    let (mint, interval) = (parts[0], parts[1]);
                    self.remove_subscription(socket_id, mint, interval);
                }
            }
        }

        // 移除连接记录 / Remove connection record
        self.connections.remove(socket_id);
    }

    /// 更新活动时间 / Update activity time
    pub fn update_activity(&mut self, socket_id: &str) {
        if let Some(client) = self.connections.get_mut(socket_id) {
            client.last_activity = Instant::now();
        }
    }

    /// 增加K线数据发送计数 / Increment kline data sent count
    pub fn increment_kline_data_sent(&mut self, socket_id: &str) {
        if let Some(client) = self.connections.get_mut(socket_id) {
            client.kline_data_sent_count += 1;
            client.total_messages_sent += 1;
        }
    }

    /// 增加历史数据发送计数 / Increment history data sent count
    pub fn increment_history_data_sent(&mut self, socket_id: &str) {
        if let Some(client) = self.connections.get_mut(socket_id) {
            client.history_data_sent_count += 1;
            client.total_messages_sent += 1;
        }
    }

    /// 获取超时的客户端 / Get timeout clients
    pub fn get_timeout_clients(&self, timeout_duration: std::time::Duration) -> Vec<String> {
        let now = Instant::now();
        self.connections
            .iter()
            .filter(|(_, conn)| now.duration_since(conn.last_activity) > timeout_duration)
            .map(|(id, _)| id.clone())
            .collect()
    }
}
