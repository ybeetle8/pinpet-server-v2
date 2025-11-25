// OrderBook 错误类型定义
// OrderBook Error Types

use thiserror::Error;

/// OrderBook 错误类型
/// OrderBook error types
#[derive(Error, Debug)]
pub enum OrderBookError {
    /// RocksDB 错误 / RocksDB error
    #[error("RocksDB error: {0}")]
    RocksDb(#[from] rocksdb::Error),

    /// 序列化/反序列化错误 / Serialization/Deserialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// OrderBook 不存在 / OrderBook not found
    #[error("OrderBook not found: {mint}:{direction}")]
    NotFound { mint: String, direction: String },

    /// OrderBook 已存在 / OrderBook already exists
    #[error("OrderBook already exists: {mint}:{direction}")]
    AlreadyExists { mint: String, direction: String },

    /// 无效的槽位索引 / Invalid slot index
    #[error("Invalid slot index: {index}, total: {total}")]
    InvalidSlotIndex { index: u16, total: u16 },

    /// 订单未找到 / Order not found
    #[error("Order not found at index: {0}")]
    OrderNotFound(u16),

    /// 订单ID未找到 / Order ID not found
    #[error("Order ID not found: {0}")]
    OrderIdNotFound(u64),

    /// 订单ID不匹配 / Order ID mismatch
    #[error("Order ID mismatch: expected {expected}, got {actual}")]
    OrderIdMismatch { expected: u64, actual: u64 },

    /// 超出最大容量 / Exceeds max capacity
    #[error("Exceeds max capacity: {max}")]
    ExceedsMaxCapacity { max: u32 },

    /// 空订单簿 / Empty order book
    #[error("Empty order book")]
    EmptyOrderBook,

    /// 无效的方向 / Invalid direction
    #[error("Invalid direction: {0}, expected 'up' or 'dn'")]
    InvalidDirection(String),

    /// 无效的账户数据 / Invalid account data
    #[error("Invalid account data: {0}")]
    InvalidAccountData(String),

    /// 数据越界 / Data out of bounds
    #[error("Data out of bounds: {0}")]
    DataOutOfBounds(String),

    /// 溢出错误 / Overflow error
    #[error("Overflow error: {0}")]
    Overflow(String),

    /// 遍历过程中的索引无效 / Invalid index during traversal
    #[error("Invalid slot index during traversal: {0}")]
    TraversalInvalidIndex(u16),

    /// order_id 无效 (必须从事件中提供) / Invalid order_id (must be provided from event)
    #[error("Invalid order_id: {0}")]
    InvalidOrderId(String),

    /// 通用错误 / Generic error
    #[error("{0}")]
    Generic(String),
}

/// Result 类型别名 / Result type alias
pub type Result<T> = std::result::Result<T, OrderBookError>;
