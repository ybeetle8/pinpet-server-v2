// 事件存储错误定义 / Event storage error definitions
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EventStorageError {
    #[error("数据库错误 / Database error: {0}")]
    DatabaseError(#[from] rocksdb::Error),

    #[error("序列化错误 / Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("事件不存在 / Event not found: signature={signature}, type={event_type}, idx={idx}")]
    EventNotFound {
        signature: String,
        event_type: String,
        idx: u32,
    },

    #[error("索引损坏 / Index corrupted: {0}")]
    IndexCorrupted(String),

    #[error("无效的事件类型 / Invalid event type: {0}")]
    InvalidEventType(String),

    #[error("UTF-8转换错误 / UTF-8 conversion error: {0}")]
    Utf8Error(#[from] std::string::FromUtf8Error),

    #[error("通用错误 / General error: {0}")]
    GeneralError(String),
}