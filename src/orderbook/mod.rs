// OrderBook 模块 - 基于 RocksDB 的链表式订单簿管理
// OrderBook Module - RocksDB-based linked list order book management

pub mod errors;
pub mod manager;
pub mod types;

// Re-export main types
// 重导出主要类型
pub use errors::{OrderBookError, Result};
pub use manager::OrderBookDBManager;
pub use types::{MarginOrder, MarginOrderUpdateData, OrderBookHeader, TraversalResult};

#[cfg(test)]
mod tests;
