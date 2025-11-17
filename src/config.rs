use serde::Deserialize;
use anyhow::Result;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub solana: SolanaConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub rocksdb_path: String,
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
