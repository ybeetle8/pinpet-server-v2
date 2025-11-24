use anyhow::Result;
use rocksdb::{Options, DB};
use std::sync::Arc;
use tracing::info;

use crate::config::{Config, OrderBookDbConfig};

/// RocksDB 存储服务
pub struct RocksDbStorage {
    pub(crate) db: Arc<DB>,
    config: Config,
}

impl RocksDbStorage {
    /// 创建新的 RocksDB 存储实例 (照抄老项目配置) 
    pub fn new(config: &Config) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // 1. Maximize memory usage - reduce flush frequency
        opts.set_write_buffer_size(512 * 1024 * 1024); // 512MB single buffer
        opts.set_max_write_buffer_number(8); // 8 buffers = 4GB memory
        opts.set_min_write_buffer_number_to_merge(1); // Single buffer can flush
        opts.set_db_write_buffer_size(4096 * 1024 * 1024); // 4GB total write buffer

        // 2. Progressive compression (balance performance and space)
        opts.set_compression_type(rocksdb::DBCompressionType::None);
        opts.set_compression_per_level(&[
            rocksdb::DBCompressionType::None,   // L0: No compression (latest data, frequent writes)
            rocksdb::DBCompressionType::None,   // L1: No compression (frequent writes)
            rocksdb::DBCompressionType::Snappy, // L2: Light compression
            rocksdb::DBCompressionType::Lz4,    // L3: Light compression
            rocksdb::DBCompressionType::Zstd,   // L4: Medium compression
            rocksdb::DBCompressionType::Zstd,   // L5: Medium compression
            rocksdb::DBCompressionType::Zstd,   // L6: Medium compression
        ]);

        // 3. Greatly delay compaction triggers - almost no compaction
        opts.set_level_zero_file_num_compaction_trigger(50); // 50 L0 files before compaction
        opts.set_level_zero_slowdown_writes_trigger(100); // 100 files before slowdown
        opts.set_level_zero_stop_writes_trigger(200); // 200 files before stop

        // 4. Ultra-large file sizes - reduce file count
        opts.set_target_file_size_base(1024 * 1024 * 1024); // 1GB file size
        opts.set_max_bytes_for_level_base(10 * 1024 * 1024 * 1024); // 10GB L1 size
        opts.set_max_bytes_for_level_multiplier(10.0); // 10x growth per level
        opts.set_num_levels(7);

        // 5. Maximize concurrency
        opts.set_max_background_jobs(16); // 16 background tasks
        opts.set_max_subcompactions(8); // 8 sub-compaction tasks

        // 6. Ultimate filesystem optimization
        opts.set_use_fsync(false); // Disable fsync
        opts.set_bytes_per_sync(0); // Disable periodic sync
        opts.set_wal_bytes_per_sync(0); // Disable WAL sync

        // 7. WAL ultimate optimization
        opts.set_max_total_wal_size(2048 * 1024 * 1024); // 2GB WAL

        // 8. Disable all statistics and checks
        opts.set_stats_dump_period_sec(0); // Disable stats
        opts.set_stats_persist_period_sec(0); // Disable stats persistence
        opts.set_paranoid_checks(false); // Disable paranoid checks

        // 9. Memory table optimization
        opts.set_allow_concurrent_memtable_write(true); // Concurrent memtable writes
        opts.set_enable_write_thread_adaptive_yield(true); // Adaptive yield
        opts.set_max_open_files(-1); // Unlimited open files

        // 10. Optimize memory allocation
        opts.set_arena_block_size(64 * 1024 * 1024); // 64MB arena blocks

        let db = DB::open(&opts, &config.database.rocksdb_path)?;

        info!(
            "🗄️ RocksDB initialized successfully, path: {}",
            config.database.rocksdb_path
        );

        Ok(Self {
            db: Arc::new(db),
            config: config.clone(),
        })
    }

    /// 写入键值对
    pub fn put(&self, key: &str, value: &str) -> Result<()> {
        self.db.put(key.as_bytes(), value.as_bytes())?;
        Ok(())
    }

    /// 读取值
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        match self.db.get(key.as_bytes())? {
            Some(value) => Ok(Some(String::from_utf8(value)?)),
            None => Ok(None),
        }
    }

    /// 删除键
    pub fn delete(&self, key: &str) -> Result<()> {
        self.db.delete(key.as_bytes())?;
        Ok(())
    }

    /// 获取数据库统计信息
    pub fn get_stats(&self) -> Result<String> {
        let stats = self.db.property_value("rocksdb.stats")?;
        Ok(stats.unwrap_or_else(|| "No stats available".to_string()))
    }

    /// 创建事件存储实例 / Create event storage instance
    pub fn create_event_storage(&self) -> Result<crate::db::EventStorage> {
        crate::db::EventStorage::new(Arc::clone(&self.db))
    }

    /// 创建 OrderBook 存储实例 / Create OrderBook storage instance
    /// @deprecated 此方法使用共享 DB，推荐使用 create_orderbook_db 创建独立实例
    /// @deprecated This method uses shared DB, recommend using create_orderbook_db for independent instance
    pub fn create_orderbook_storage(&self) -> Result<crate::db::OrderBookStorage> {
        Ok(crate::db::OrderBookStorage::new(Arc::clone(&self.db)))
    }

    /// 创建独立的 OrderBook 数据库实例 / Create independent OrderBook database instance
    ///
    /// 用于将 OrderBook 数据与其他数据物理隔离
    /// Used to physically isolate OrderBook data from other data
    pub fn create_orderbook_db(config: &Config) -> Result<Arc<DB>> {
        let db_config = &config.database.orderbook_db;
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // 1. 内存配置 - 根据配置项设置 / Memory config - set according to config
        opts.set_write_buffer_size(db_config.write_buffer_size_mb * 1024 * 1024);
        opts.set_max_write_buffer_number(db_config.max_write_buffer_number);
        opts.set_min_write_buffer_number_to_merge(1);
        opts.set_db_write_buffer_size(
            db_config.write_buffer_size_mb * db_config.max_write_buffer_number as usize * 1024 * 1024
        );

        // 2. 渐进式压缩 / Progressive compression
        opts.set_compression_type(rocksdb::DBCompressionType::None);
        opts.set_compression_per_level(&[
            rocksdb::DBCompressionType::None,   // L0: No compression
            rocksdb::DBCompressionType::None,   // L1: No compression
            rocksdb::DBCompressionType::Snappy, // L2: Light compression
            rocksdb::DBCompressionType::Lz4,    // L3: Light compression
            rocksdb::DBCompressionType::Zstd,   // L4: Medium compression
            rocksdb::DBCompressionType::Zstd,   // L5: Medium compression
            rocksdb::DBCompressionType::Zstd,   // L6: Medium compression
        ]);

        // 3. Compaction 触发器 / Compaction triggers
        opts.set_level_zero_file_num_compaction_trigger(50);
        opts.set_level_zero_slowdown_writes_trigger(100);
        opts.set_level_zero_stop_writes_trigger(200);

        // 4. 文件大小配置 / File size config
        opts.set_target_file_size_base(512 * 1024 * 1024); // 512MB
        opts.set_max_bytes_for_level_base(5 * 1024 * 1024 * 1024); // 5GB
        opts.set_max_bytes_for_level_multiplier(10.0);
        opts.set_num_levels(7);

        // 5. 并发配置 / Concurrency config
        opts.set_max_background_jobs(db_config.max_background_jobs);
        opts.set_max_subcompactions(4);

        // 6. 文件系统优化 / Filesystem optimization
        opts.set_use_fsync(db_config.use_fsync);
        if !db_config.use_fsync {
            opts.set_bytes_per_sync(0);
            opts.set_wal_bytes_per_sync(0);
        }

        // 7. WAL 配置 / WAL config
        opts.set_max_total_wal_size(1024 * 1024 * 1024); // 1GB WAL

        // 8. 统计和检查 / Statistics and checks
        opts.set_stats_dump_period_sec(0);
        opts.set_stats_persist_period_sec(0);
        opts.set_paranoid_checks(db_config.paranoid_checks);

        // 9. 内存表优化 / Memtable optimization
        opts.set_allow_concurrent_memtable_write(true);
        opts.set_enable_write_thread_adaptive_yield(true);
        opts.set_max_open_files(-1);

        // 10. 内存分配优化 / Memory allocation optimization
        opts.set_arena_block_size(32 * 1024 * 1024); // 32MB arena blocks

        let db = DB::open(&opts, &config.database.orderbook_db_path)?;

        info!(
            "🗄️ OrderBook RocksDB initialized successfully, path: {}",
            config.database.orderbook_db_path
        );

        Ok(Arc::new(db))
    }

    /// 创建 Token 存储实例 / Create Token storage instance
    pub fn create_token_storage(&self) -> Result<crate::db::TokenStorage> {
        crate::db::TokenStorage::new(Arc::clone(&self.db), self.config.clone())
    }
}
