// 事件持久化队列 - 防止事件丢失 / Event persistence queue - prevent event loss
use crate::solana::events::PinpetEvent;
use anyhow::Result;
use rocksdb::{DB, WriteBatch, Options, IteratorMode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn, error};

/// 队列事件包装器 / Queue event wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedEvent {
    /// 事件ID（时间戳+序号） / Event ID (timestamp + sequence)
    pub id: String,
    /// 事件数据 / Event data
    pub event: PinpetEvent,
    /// 入队时间戳（毫秒） / Enqueue timestamp (milliseconds)
    pub enqueued_at: u64,
    /// 处理状态：pending/processing/completed/failed / Processing status
    pub status: String,
    /// 重试次数 / Retry count
    pub retry_count: u32,
    /// 最后处理时间 / Last processing time
    pub last_processed_at: Option<u64>,
    /// 错误信息 / Error message
    pub error: Option<String>,
}

/// 事件队列存储管理器 / Event queue storage manager
pub struct EventQueueStorage {
    /// RocksDB 实例 / RocksDB instance
    db: Arc<DB>,
    /// 序列号计数器 / Sequence counter
    sequence: Arc<std::sync::atomic::AtomicU64>,
}

impl EventQueueStorage {
    /// 创建新的事件队列存储 / Create new event queue storage
    pub fn new(path: &str) -> Result<Self> {
        info!("初始化事件队列存储 / Initializing event queue storage at: {}", path);

        // 创建目录 / Create directory
        std::fs::create_dir_all(path)?;

        // 配置 RocksDB / Configure RocksDB
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        opts.set_write_buffer_size(64 * 1024 * 1024); // 64MB
        opts.set_max_write_buffer_number(3);
        opts.set_target_file_size_base(64 * 1024 * 1024);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);

        let db = Arc::new(DB::open(&opts, path)?);

        // 初始化序列号 / Initialize sequence number
        let sequence = Arc::new(std::sync::atomic::AtomicU64::new(0));

        Ok(Self { db, sequence })
    }

    /// 生成事件ID / Generate event ID
    fn generate_event_id(&self) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let seq = self.sequence.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        format!("{:016x}:{:08x}", timestamp, seq)
    }

    /// 入队事件 / Enqueue event
    pub async fn enqueue(&self, event: PinpetEvent) -> Result<String> {
        let id = self.generate_event_id();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let queued_event = QueuedEvent {
            id: id.clone(),
            event,
            enqueued_at: timestamp,
            status: "pending".to_string(),
            retry_count: 0,
            last_processed_at: None,
            error: None,
        };

        // 存储到数据库 / Store to database
        let key = format!("queue:{}", id);
        let value = serde_json::to_vec(&queued_event)?;
        self.db.put(key.as_bytes(), &value)?;

        // 添加到待处理索引 / Add to pending index
        let pending_key = format!("pending:{}", id);
        self.db.put(pending_key.as_bytes(), b"")?;

        Ok(id)
    }

    /// 批量入队 / Batch enqueue
    #[allow(dead_code)]
    pub async fn enqueue_batch(&self, events: Vec<PinpetEvent>) -> Result<Vec<String>> {
        let mut batch = WriteBatch::default();
        let mut ids = Vec::new();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        for event in events {
            let id = self.generate_event_id();
            let queued_event = QueuedEvent {
                id: id.clone(),
                event,
                enqueued_at: timestamp,
                status: "pending".to_string(),
                retry_count: 0,
                last_processed_at: None,
                error: None,
            };

            // 添加到批处理 / Add to batch
            let key = format!("queue:{}", id);
            let value = serde_json::to_vec(&queued_event)?;
            batch.put(key.as_bytes(), &value);

            // 添加到待处理索引 / Add to pending index
            let pending_key = format!("pending:{}", id);
            batch.put(pending_key.as_bytes(), b"");

            ids.push(id);
        }

        // 原子提交 / Atomic commit
        self.db.write(batch)?;

        Ok(ids)
    }

    /// 获取待处理事件（FIFO） / Get pending events (FIFO)
    pub async fn get_pending_events(&self, limit: usize) -> Result<Vec<QueuedEvent>> {
        let mut events = Vec::new();
        let prefix = b"pending:";

        // 扫描待处理索引 / Scan pending index
        let iter = self.db.iterator(IteratorMode::From(prefix, rocksdb::Direction::Forward));

        for item in iter {
            let (key, _) = item?;
            // 检查前缀 / Check prefix
            if !key.starts_with(prefix) {
                break;
            }

            // 提取事件ID / Extract event ID
            let key_str = String::from_utf8_lossy(&key);
            let id = key_str.trim_start_matches("pending:");

            // 获取事件数据 / Get event data
            let queue_key = format!("queue:{}", id);
            if let Some(value) = self.db.get(queue_key.as_bytes())? {
                if let Ok(queued_event) = serde_json::from_slice::<QueuedEvent>(&value) {
                    events.push(queued_event);

                    if events.len() >= limit {
                        break;
                    }
                }
            }
        }

        Ok(events)
    }

    /// 标记事件为处理中 / Mark event as processing
    pub async fn mark_processing(&self, id: &str) -> Result<()> {
        let queue_key = format!("queue:{}", id);

        // 获取事件 / Get event
        if let Some(value) = self.db.get(queue_key.as_bytes())? {
            if let Ok(mut queued_event) = serde_json::from_slice::<QueuedEvent>(&value) {
                // 更新状态 / Update status
                queued_event.status = "processing".to_string();
                queued_event.last_processed_at = Some(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64
                );

                // 保存更新 / Save update
                let value = serde_json::to_vec(&queued_event)?;
                self.db.put(queue_key.as_bytes(), &value)?;

                // 从待处理索引移除 / Remove from pending index
                let pending_key = format!("pending:{}", id);
                self.db.delete(pending_key.as_bytes())?;

                // 添加到处理中索引 / Add to processing index
                let processing_key = format!("processing:{}", id);
                self.db.put(processing_key.as_bytes(), b"")?;
            }
        }

        Ok(())
    }

    /// 标记事件为完成 / Mark event as completed
    pub async fn mark_completed(&self, id: &str) -> Result<()> {
        let mut batch = WriteBatch::default();

        // 从处理中索引移除 / Remove from processing index
        let processing_key = format!("processing:{}", id);
        batch.delete(processing_key.as_bytes());

        // 删除事件数据（已处理完成） / Delete event data (completed)
        let queue_key = format!("queue:{}", id);
        batch.delete(queue_key.as_bytes());

        // 原子提交 / Atomic commit
        self.db.write(batch)?;

        Ok(())
    }

    /// 标记事件为失败（可重试） / Mark event as failed (retryable)
    pub async fn mark_failed(&self, id: &str, error: String, max_retries: u32) -> Result<bool> {
        let queue_key = format!("queue:{}", id);

        // 获取事件 / Get event
        if let Some(value) = self.db.get(queue_key.as_bytes())? {
            if let Ok(mut queued_event) = serde_json::from_slice::<QueuedEvent>(&value) {
                queued_event.retry_count += 1;
                queued_event.error = Some(error);

                let mut batch = WriteBatch::default();

                // 从处理中索引移除 / Remove from processing index
                let processing_key = format!("processing:{}", id);
                batch.delete(processing_key.as_bytes());

                if queued_event.retry_count < max_retries {
                    // 可以重试，放回待处理队列 / Can retry, put back to pending
                    queued_event.status = "pending".to_string();
                    let value = serde_json::to_vec(&queued_event)?;
                    batch.put(queue_key.as_bytes(), &value);

                    // 添加回待处理索引 / Add back to pending index
                    let pending_key = format!("pending:{}", id);
                    batch.put(pending_key.as_bytes(), b"");

                    self.db.write(batch)?;
                    return Ok(true); // 将重试 / Will retry
                } else {
                    // 超过最大重试次数，标记为死信 / Exceeded max retries, mark as dead letter
                    queued_event.status = "dead".to_string();
                    let value = serde_json::to_vec(&queued_event)?;
                    batch.put(queue_key.as_bytes(), &value);

                    // 添加到死信索引 / Add to dead letter index
                    let dead_key = format!("dead:{}", id);
                    batch.put(dead_key.as_bytes(), b"");

                    self.db.write(batch)?;

                    error!("事件 {} 超过最大重试次数，已移入死信队列 / Event {} exceeded max retries, moved to dead letter queue", id, id);
                    return Ok(false); // 不再重试 / No more retries
                }
            }
        }

        Ok(false)
    }

    /// 恢复处理中的事件（服务重启时） / Recover processing events (on service restart)
    pub async fn recover_processing_events(&self) -> Result<u32> {
        let mut count = 0;
        let mut batch = WriteBatch::default();
        let prefix = b"processing:";

        // 扫描处理中索引 / Scan processing index
        let iter = self.db.iterator(IteratorMode::From(prefix, rocksdb::Direction::Forward));

        for item in iter {
            let (key, _) = item?;
            // 检查前缀 / Check prefix
            if !key.starts_with(prefix) {
                break;
            }

            // 提取事件ID / Extract event ID
            let key_str = String::from_utf8_lossy(&key);
            let id = key_str.trim_start_matches("processing:");

            // 移回待处理队列 / Move back to pending queue
            batch.delete(&key);
            let pending_key = format!("pending:{}", id);
            batch.put(pending_key.as_bytes(), b"");

            // 更新事件状态 / Update event status
            let queue_key = format!("queue:{}", id);
            if let Some(value) = self.db.get(queue_key.as_bytes())? {
                if let Ok(mut queued_event) = serde_json::from_slice::<QueuedEvent>(&value) {
                    queued_event.status = "pending".to_string();
                    let value = serde_json::to_vec(&queued_event)?;
                    batch.put(queue_key.as_bytes(), &value);
                }
            }

            count += 1;
        }

        if count > 0 {
            self.db.write(batch)?;
            warn!("恢复了 {} 个处理中的事件到待处理队列 / Recovered {} processing events to pending queue", count, count);
        }

        Ok(count)
    }

    /// 获取队列统计信息 / Get queue statistics
    pub async fn get_statistics(&self) -> Result<(u32, u32, u32, u32)> {
        let mut pending_count = 0;
        let mut processing_count = 0;
        let mut dead_count = 0;
        let mut total_count = 0;

        // 统计待处理 / Count pending
        let prefix = b"pending:";
        for item in self.db.iterator(IteratorMode::From(prefix, rocksdb::Direction::Forward)) {
            if let Ok((key, _)) = item {
                if !key.starts_with(prefix) {
                    break;
                }
                pending_count += 1;
            }
        }

        // 统计处理中 / Count processing
        let prefix = b"processing:";
        for item in self.db.iterator(IteratorMode::From(prefix, rocksdb::Direction::Forward)) {
            if let Ok((key, _)) = item {
                if !key.starts_with(prefix) {
                    break;
                }
                processing_count += 1;
            }
        }

        // 统计死信 / Count dead letters
        let prefix = b"dead:";
        for item in self.db.iterator(IteratorMode::From(prefix, rocksdb::Direction::Forward)) {
            if let Ok((key, _)) = item {
                if !key.starts_with(prefix) {
                    break;
                }
                dead_count += 1;
            }
        }

        // 统计总数 / Count total
        let prefix = b"queue:";
        for item in self.db.iterator(IteratorMode::From(prefix, rocksdb::Direction::Forward)) {
            if let Ok((key, _)) = item {
                if !key.starts_with(prefix) {
                    break;
                }
                total_count += 1;
            }
        }

        Ok((pending_count, processing_count, dead_count, total_count))
    }

    /// 清理已完成的事件（定期维护） / Clean completed events (periodic maintenance)
    pub async fn cleanup_completed(&self, _older_than_ms: u64) -> Result<u32> {
        // 由于已完成的事件会立即删除，这里主要是安全检查和清理死信队列
        // Since completed events are deleted immediately, this is mainly a safety check and dead letter cleanup

        let count = 0;

        // 未来可以在这里添加死信队列的清理逻辑
        // Future: can add dead letter queue cleanup logic here

        Ok(count)
    }
}