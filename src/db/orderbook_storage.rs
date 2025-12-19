// OrderBook 专用数据库管理器 / OrderBook dedicated database manager
use anyhow::Result;
use rocksdb::{Options, DB};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{info, warn};

use crate::config::OrderBookDbConfig;
use crate::orderbook::OrderBookDBManager;

/// OrderBook 存储管理器 / OrderBook storage manager 
/// 负责初始化独立的 OrderBook 数据库,并为每个 (mint, direction) 创建管理器
/// Responsible for initializing independent OrderBook database and creating managers for each (mint, direction)
pub struct OrderBookStorage {
    /// 独立的 RocksDB 实例 / Independent RocksDB instance
    db: Arc<DB>,

    /// 缓存已创建的 OrderBook 管理器 / Cache of created OrderBook managers
    /// Key: "mint:direction" (例如 "EPjFWdd5A....:up" 或 "EPjFWdd5A....:dn")
    /// Key: "mint:direction" (e.g., "EPjFWdd5A....:up" or "EPjFWdd5A....:dn")
    managers: Arc<RwLock<HashMap<String, Arc<OrderBookDBManager>>>>,
}

impl OrderBookStorage {
    /// 创建新的 OrderBook 存储实例 / Create new OrderBook storage instance
    ///
    /// # 参数 / Parameters
    /// * `config` - OrderBook 数据库性能配置 / OrderBook database performance config
    /// * `db_path` - 数据库路径 / Database path
    pub fn new(config: &OrderBookDbConfig, db_path: &str) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // 应用配置参数 / Apply configuration parameters
        opts.set_write_buffer_size(config.write_buffer_size_mb * 1024 * 1024);
        opts.set_max_write_buffer_number(config.max_write_buffer_number);
        opts.set_use_fsync(config.use_fsync);
        opts.set_paranoid_checks(config.paranoid_checks);
        opts.set_max_background_jobs(config.max_background_jobs);

        // 优化内存分配 / Optimize memory allocation
        opts.set_allow_concurrent_memtable_write(true);
        opts.set_enable_write_thread_adaptive_yield(true);

        // 压缩策略 / Compression strategy
        opts.set_compression_type(rocksdb::DBCompressionType::None);
        opts.set_compression_per_level(&[
            rocksdb::DBCompressionType::None,   // L0: 无压缩 / No compression
            rocksdb::DBCompressionType::Snappy, // L1: 轻压缩 / Light compression
            rocksdb::DBCompressionType::Lz4,    // L2+: 轻压缩 / Light compression
        ]);

        let db = DB::open(&opts, db_path)?;

        info!(
            "🗄️ OrderBook RocksDB initialized successfully / OrderBook RocksDB 初始化成功, path: {}",
            db_path
        );
        info!(
            "📊 OrderBook DB config: write_buffer={}MB, max_buffers={}, fsync={}, paranoid_checks={}, bg_jobs={}",
            config.write_buffer_size_mb,
            config.max_write_buffer_number,
            config.use_fsync,
            config.paranoid_checks,
            config.max_background_jobs
        );

        Ok(Self {
            db: Arc::new(db),
            managers: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// 获取或创建 OrderBook 管理器 / Get or create OrderBook manager
    ///
    /// # 参数 / Parameters
    /// * `mint` - Token mint 地址 / Token mint address
    /// * `direction` - 订单方向: "up"(做空) 或 "dn"(做多) / Order direction: "up"(short) or "dn"(long)
    ///
    /// # 返回值 / Returns
    /// 返回对应的 OrderBookDBManager 实例 / Returns corresponding OrderBookDBManager instance
    pub fn get_or_create_manager(
        &self,
        mint: String,
        direction: String,
    ) -> Result<Arc<OrderBookDBManager>> {
        let key = format!("{}:{}", mint, direction);

        // 第一次检查（读锁）- 快速路径 / First check (read lock) - fast path
        {
            let managers = self.managers.read().unwrap();
            if let Some(manager) = managers.get(&key) {
                return Ok(manager.clone());
            }
        }

        // 获取写锁并进行第二次检查 / Acquire write lock and double check
        let mut managers = self.managers.write().unwrap();

        // 🔧 P0 修复: 双重检查锁定防止竞态条件
        // 🔧 P0 Fix: Double-checked locking to prevent race condition
        // 第二次检查 - 防止其他线程已经创建了管理器 / Second check - prevent other thread already created
        if let Some(manager) = managers.get(&key) {
            return Ok(manager.clone());
        }

        // 创建新的 manager / Create new manager
        info!(
            "📝 Creating new OrderBook manager / 创建新的 OrderBook 管理器: mint={}, direction={}",
            &mint[..8], direction
        );

        let manager = Arc::new(OrderBookDBManager::new(
            self.db.clone(),
            mint.clone(),
            direction.clone(),
        ));

        // 初始化 OrderBook (如果不存在) / Initialize OrderBook (if not exists)
        // 使用 "system" 作为 authority
        match manager.initialize("system".to_string()) {
            Ok(_) => {
                info!(
                    "✅ OrderBook initialized / OrderBook 已初始化: {}:{}",
                    &mint[..8], direction
                );
            }
            Err(e) => {
                // 如果已存在,忽略错误 / Ignore error if already exists
                if e.to_string().contains("already exists") {
                    info!(
                        "ℹ️ OrderBook already exists / OrderBook 已存在: {}:{}",
                        &mint[..8], direction
                    );
                } else {
                    warn!(
                        "⚠️ OrderBook initialization warning / OrderBook 初始化警告: {}:{} - {}",
                        &mint[..8], direction, e
                    );
                }
            }
        }

        // 缓存 manager - 在持有写锁时直接插入 / Cache manager - insert while holding write lock
        managers.insert(key, manager.clone());

        Ok(manager)
    }

    /// 获取数据库统计信息 / Get database statistics
    #[allow(dead_code)]
    pub fn get_stats(&self) -> Result<String> {
        let stats = self.db.property_value("rocksdb.stats")?;
        Ok(stats.unwrap_or_else(|| "No stats available".to_string()))
    }

    /// 获取所有已缓存的 OrderBook 管理器数量 / Get count of cached OrderBook managers
    #[allow(dead_code)]
    pub fn get_manager_count(&self) -> usize {
        self.managers.read().unwrap().len()
    }

    /// 获取底层 RocksDB 实例 / Get underlying RocksDB instance
    pub fn db(&self) -> Arc<DB> {
        self.db.clone()
    }
}
