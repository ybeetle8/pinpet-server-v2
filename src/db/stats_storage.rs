// 统计数据库存储模块 / Statistics Database Storage Module
use anyhow::Result;
use rocksdb::{Options, DB};
use std::sync::Arc;
use tracing::info;

use crate::config::Config;

/// 统计数据库存储 / Statistics Database Storage
///
/// 专门用于存储币种统计数据（Volume、Markets、Change 等）
/// Dedicated storage for token statistics (Volume, Markets, Change, etc.)
pub struct StatsStorage {
    pub(crate) db: Arc<DB>,
    config: Config,
}

impl StatsStorage {
    /// 创建新的统计数据库实例 / Create new stats database instance
    ///
    /// 使用针对统计数据优化的 RocksDB 配置
    /// Uses RocksDB configuration optimized for statistics data
    pub fn new(config: &Config) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // 1. 统计数据库特定配置 / Stats DB specific configuration
        // 相比事件数据库，更注重读性能和空间效率
        // Compared to event DB, focus more on read performance and space efficiency

        // 写缓冲：适中大小（256MB），兼顾写入速度和内存占用
        // Write buffer: Medium size (256MB), balance write speed and memory usage
        opts.set_write_buffer_size(256 * 1024 * 1024);
        opts.set_max_write_buffer_number(4);
        opts.set_min_write_buffer_number_to_merge(2);
        opts.set_db_write_buffer_size(1024 * 1024 * 1024); // 1GB total

        // 2. 压缩策略：积极压缩节省空间 / Compression: Aggressive to save space
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        opts.set_compression_per_level(&[
            rocksdb::DBCompressionType::None,   // L0: 无压缩
            rocksdb::DBCompressionType::Lz4,    // L1: 轻量压缩
            rocksdb::DBCompressionType::Lz4,    // L2: 轻量压缩
            rocksdb::DBCompressionType::Zstd,   // L3+: 高压缩比
            rocksdb::DBCompressionType::Zstd,
            rocksdb::DBCompressionType::Zstd,
            rocksdb::DBCompressionType::Zstd,
        ]);

        // 3. Compaction：较为积极（优化读性能）/ Compaction: More aggressive (optimize read)
        opts.set_level_zero_file_num_compaction_trigger(4);  // 4 files → compact
        opts.set_level_zero_slowdown_writes_trigger(20);     // 20 files → slowdown
        opts.set_level_zero_stop_writes_trigger(36);         // 36 files → stop

        // 4. 文件大小：适中（便于清理和 Compaction）/ File size: Medium (easy cleanup)
        opts.set_target_file_size_base(256 * 1024 * 1024); // 256MB
        opts.set_max_bytes_for_level_base(1024 * 1024 * 1024); // 1GB
        opts.set_max_bytes_for_level_multiplier(10.0);
        opts.set_num_levels(7);

        // 5. 并发配置 / Concurrency configuration
        opts.set_max_background_jobs(8);
        opts.set_max_subcompactions(4);

        // 6. 启用 Bloom Filter 提升读性能 / Enable Bloom Filter for read performance
        let mut block_opts = rocksdb::BlockBasedOptions::default();
        block_opts.set_bloom_filter(10.0, false); // 10 bits per key
        block_opts.set_block_cache(&rocksdb::Cache::new_lru_cache(512 * 1024 * 1024)); // 512MB cache
        opts.set_block_based_table_factory(&block_opts);

        // 7. 适中的 WAL 配置 / Medium WAL configuration
        opts.set_max_total_wal_size(512 * 1024 * 1024); // 512MB WAL

        // 8. 其他优化 / Other optimizations
        opts.set_allow_concurrent_memtable_write(true);
        opts.set_enable_write_thread_adaptive_yield(true);
        opts.set_max_open_files(1000); // 限制打开文件数

        // 获取数据库路径 / Get database path
        let stats_db_path = config.database.stats_db_path
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("./data/stats");

        let db = DB::open(&opts, stats_db_path)?;

        info!(
            "📊 Stats RocksDB initialized successfully, path: {}",
            stats_db_path
        );

        Ok(Self {
            db: Arc::new(db),
            config: config.clone(),
        })
    }

    /// 获取数据库引用 / Get database reference
    pub fn db(&self) -> Arc<DB> {
        Arc::clone(&self.db)
    }

    /// 创建 Volume 存储实例 / Create Volume storage instance
    pub fn create_volume_storage(&self) -> crate::volume::VolumeStorage {
        crate::volume::VolumeStorage::new(Arc::clone(&self.db))
    }

    /// 创建 Token 存储实例 / Create Token storage instance
    pub fn create_token_storage(&self) -> Result<crate::db::TokenStorage> {
        crate::db::TokenStorage::new(Arc::clone(&self.db), self.config.clone())
    }

    /// 创建 K线 存储实例 / Create K-line storage instance
    pub fn create_kline_storage(&self) -> crate::db::KlineStorage {
        crate::db::KlineStorage::new(Arc::clone(&self.db))
    }

    /// 获取数据库统计信息 / Get database statistics
    pub fn get_stats(&self) -> Result<String> {
        let stats = self.db.property_value("rocksdb.stats")?;
        Ok(stats.unwrap_or_else(|| "No stats available".to_string()))
    }
}
