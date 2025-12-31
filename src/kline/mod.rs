// K线模块 / K-line module
// 提供实时K线数据推送和历史数据查询功能 / Provides real-time K-line data push and historical data query functionality

pub mod cache;
pub mod cleanup_task;
pub mod data_processor;
pub mod event_handler;
pub mod socket_service;
pub mod subscription;
pub mod types;

// 重新导出常用类型 / Re-export commonly used types
pub use cache::KlineCache;
pub use cleanup_task::{start_cleanup_task, CleanupConfig};
pub use event_handler::KlineEventHandler;
pub use socket_service::KlineSocketService;
pub use types::KlineConfig;
