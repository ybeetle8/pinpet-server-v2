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
    #[serde(default)]
    pub orderbook_sync: OrderBookSyncConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// Token列表缓存TTL(秒) / Token list cache TTL (seconds)
    /// 默认30秒,可避免频繁查询 / Default 30 seconds to avoid frequent queries
    #[serde(default = "default_token_list_cache_ttl")]
    pub token_list_cache_ttl_secs: u64,
}

fn default_token_list_cache_ttl() -> u64 {
    30
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub rocksdb_path: String,
    /// OrderBook 专用数据库路径 / OrderBook dedicated database path
    pub orderbook_db_path: String,
    /// 统计数据库路径 / Statistics database path
    #[serde(default = "default_stats_db_path")]
    pub stats_db_path: Option<String>,
    /// OrderBook 数据库性能配置 / OrderBook database performance config
    #[serde(default)]
    pub orderbook_db: OrderBookDbConfig,
}

fn default_stats_db_path() -> Option<String> {
    Some("./data/stats".to_string())
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

#[derive(Debug, Deserialize, Clone)]
pub struct SolanaConfig {
    pub rpc_url: String,                    // Solana RPC URL
    pub ws_url: String,                     // Solana WebSocket URL
    pub program_id: String,                 // 程序ID / Program ID
    pub enable_event_listener: bool,        // 是否启用事件监听 / Enable event listener
    pub commitment: String,                 // 承诺级别 / Commitment level: processed/confirmed/finalized
    pub max_reconnect_attempts: u32,        // 最大重连次数 / Max reconnect attempts
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
    #[allow(dead_code)]
    pub connection_timeout_secs: u64,       // 连接超时时间(秒) / Connection timeout (seconds)
    #[serde(default = "default_max_subscriptions")]
    pub max_subscriptions_per_client: usize, // 每客户端最大订阅数 / Max subscriptions per client
    #[serde(default = "default_history_limit")]
    #[allow(dead_code)]
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

/// OrderBook 同步配置 / OrderBook sync configuration
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct OrderBookSyncConfig {
    /// 是否启用延迟对比同步 / Enable delayed comparison sync
    #[serde(default = "default_orderbook_sync_enabled")]
    pub enabled: bool,
    /// 触发对比的空闲时间阈值（秒）/ Idle threshold for triggering comparison (seconds)
    #[serde(default = "default_idle_threshold_seconds")]
    pub idle_threshold_seconds: u64,
    /// 检查间隔（秒）/ Check interval (seconds)
    #[serde(default = "default_check_interval_seconds")]
    pub check_interval_seconds: u64,
    /// 最大并发同步任务数 / Maximum concurrent sync tasks
    #[serde(default = "default_max_concurrent_syncs")]
    pub max_concurrent_syncs: usize,
    /// 是否自动修复差异 / Auto repair differences
    #[serde(default = "default_auto_repair")]
    pub auto_repair: bool,
    /// 同步失败重试次数 / Retry count on sync failure
    #[serde(default = "default_retry_count")]
    pub retry_count: u32,
    /// 重试延迟（秒）/ Retry delay (seconds)
    #[serde(default = "default_retry_delay_seconds")]
    pub retry_delay_seconds: u64,
}

impl Default for OrderBookSyncConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            idle_threshold_seconds: 30,
            check_interval_seconds: 5,
            max_concurrent_syncs: 3,
            auto_repair: true,
            retry_count: 3,
            retry_delay_seconds: 5,
        }
    }
}

fn default_orderbook_sync_enabled() -> bool {
    true
}

fn default_idle_threshold_seconds() -> u64 {
    30
}

fn default_check_interval_seconds() -> u64 {
    5
}

fn default_max_concurrent_syncs() -> usize {
    3
}

fn default_auto_repair() -> bool {
    true
}

fn default_retry_count() -> u32 {
    3
}

fn default_retry_delay_seconds() -> u64 {
    5
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
