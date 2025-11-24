use serde::Deserialize;
use anyhow::Result;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub solana: SolanaConfig,
    pub ipfs: IpfsConfig,
    #[serde(default)]
    pub kline: KlineServiceConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub rocksdb_path: String,
    /// OrderBook 专用数据库路径 / OrderBook dedicated database path
    pub orderbook_db_path: String,
    #[serde(default = "default_orderbook_max_limit")]
    pub orderbook_max_limit: usize,  // OrderBook查询最大返回数量 / OrderBook query max limit
    /// OrderBook 数据库性能配置 / OrderBook database performance config
    #[serde(default)]
    pub orderbook_db: OrderBookDbConfig,
}

/// OrderBook 数据库性能配置 / OrderBook database performance configuration
#[derive(Debug, Deserialize, Clone)]
pub struct OrderBookDbConfig {
    /// 单个写缓冲区大小(MB) / Single write buffer size (MB)
    #[serde(default = "default_write_buffer_size")]
    pub write_buffer_size_mb: usize,
    /// 最大写缓冲区数量 / Max number of write buffers
    #[serde(default = "default_max_write_buffer_number")]
    pub max_write_buffer_number: i32,
    /// 是否启用 fsync / Enable fsync
    #[serde(default = "default_use_fsync")]
    pub use_fsync: bool,
    /// 是否启用偏执检查 / Enable paranoid checks
    #[serde(default = "default_paranoid_checks")]
    pub paranoid_checks: bool,
    /// 最大后台任务数 / Max background jobs
    #[serde(default = "default_max_background_jobs")]
    pub max_background_jobs: i32,
}

impl Default for OrderBookDbConfig {
    fn default() -> Self {
        Self {
            write_buffer_size_mb: 256,
            max_write_buffer_number: 4,
            use_fsync: false,
            paranoid_checks: false,
            max_background_jobs: 8,
        }
    }
}

fn default_write_buffer_size() -> usize {
    256
}

fn default_max_write_buffer_number() -> i32 {
    4
}

fn default_use_fsync() -> bool {
    false
}

fn default_paranoid_checks() -> bool {
    false
}

fn default_max_background_jobs() -> i32 {
    8
}

fn default_orderbook_max_limit() -> usize {
    60000
}

#[derive(Debug, Deserialize, Clone)]
pub struct SolanaConfig {
    pub rpc_url: String,                    // Solana RPC URL
    pub ws_url: String,                     // Solana WebSocket URL
    pub program_id: String,                 // 程序ID / Program ID
    pub enable_event_listener: bool,        // 是否启用事件监听 / Enable event listener
    pub commitment: String,                 // 承诺级别 / Commitment level: processed/confirmed/finalized
    pub reconnect_interval: u64,            // 重连间隔(秒) / Reconnect interval (seconds)
    pub max_reconnect_attempts: u32,        // 最大重连次数 / Max reconnect attempts
    pub event_buffer_size: usize,           // 事件缓冲区大小 / Event buffer size
    pub event_batch_size: usize,            // 事件批处理大小 / Event batch size
    pub ping_interval_seconds: u64,         // WebSocket ping间隔(秒) / WebSocket ping interval
    pub process_failed_transactions: bool,  // 是否处理失败的交易 / Process failed transactions
    pub enable_raw_message_logging: bool,   // 是否记录原始消息 / Enable raw message logging
}

#[derive(Debug, Deserialize, Clone)]
pub struct IpfsConfig {
    pub gateway_url: String,                // IPFS网关URL / IPFS gateway URL
    pub request_timeout_seconds: u64,       // 请求超时时间(秒) / Request timeout (seconds)
    pub max_retries: u32,                   // 最大重试次数 / Max retries
    pub retry_delay_seconds: u64,           // 重试延迟(秒) / Retry delay (seconds)
}

#[derive(Debug, Deserialize, Clone)]
pub struct KlineServiceConfig {
    #[serde(default = "default_kline_enable")]
    pub enable_kline_service: bool,         // 是否启用K线服务 / Enable K-line service
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout_secs: u64,       // 连接超时时间(秒) / Connection timeout (seconds)
    #[serde(default = "default_max_subscriptions")]
    pub max_subscriptions_per_client: usize, // 每客户端最大订阅数 / Max subscriptions per client
    #[serde(default = "default_history_limit")]
    pub history_data_limit: usize,          // 历史数据默认条数 / History data default limit
    #[serde(default = "default_ping_interval")]
    pub ping_interval_secs: u64,            // 心跳间隔(秒) / Ping interval (seconds)
    #[serde(default = "default_ping_timeout")]
    pub ping_timeout_secs: u64,             // 心跳超时(秒) / Ping timeout (seconds)
}

impl Default for KlineServiceConfig {
    fn default() -> Self {
        Self {
            enable_kline_service: true,
            connection_timeout_secs: 60,
            max_subscriptions_per_client: 100,
            history_data_limit: 100,
            ping_interval_secs: 25,
            ping_timeout_secs: 60,
        }
    }
}

fn default_kline_enable() -> bool {
    true
}

fn default_connection_timeout() -> u64 {
    60
}

fn default_max_subscriptions() -> usize {
    100
}

fn default_history_limit() -> usize {
    100
}

fn default_ping_interval() -> u64 {
    25
}

fn default_ping_timeout() -> u64 {
    60
}

impl Config {
    pub fn new() -> Result<Self> {
        let settings = config::Config::builder()
            .add_source(config::File::with_name("config"))
            .add_source(config::Environment::with_prefix("APP"))
            .build()?;

        let config: Config = settings.try_deserialize()?;
        Ok(config)
    }
}
