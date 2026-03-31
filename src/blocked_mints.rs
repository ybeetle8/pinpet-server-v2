// Mint地址屏蔽服务 / Mint address blocking service
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// 屏蔽配置 / Blocked configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedMintsConfig {
    pub version: String,
    pub last_updated: String,
    pub blocked_mints: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// 屏蔽服务 / Blocking service
#[derive(Clone)]
pub struct BlockedMintsService {
    blocked_set: Arc<RwLock<HashSet<String>>>,
    config_path: PathBuf,
    last_modified: Arc<RwLock<Option<SystemTime>>>,
}

impl BlockedMintsService {
    /// 创建新的屏蔽服务 / Create new blocking service
    pub fn new<P: AsRef<Path>>(config_path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = config_path.as_ref().to_path_buf();

        // 加载配置 / Load config
        let blocked_set = Self::load_config(&config_path)?;
        let last_modified = Self::get_file_modified_time(&config_path);

        info!("BlockedMintsService initialized with {} blocked mints", blocked_set.len());

        Ok(Self {
            blocked_set: Arc::new(RwLock::new(blocked_set)),
            config_path,
            last_modified: Arc::new(RwLock::new(last_modified)),
        })
    }

    /// 检查mint是否被屏蔽 / Check if mint is blocked
    pub async fn is_blocked(&self, mint: &str) -> bool {
        self.blocked_set.read().await.contains(mint)
    }

    /// 启动自动刷新任务 / Start auto-refresh task
    pub fn start_auto_refresh(self, interval: Duration) {
        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            loop {
                interval_timer.tick().await;
                if let Err(e) = self.check_and_reload().await {
                    error!("Failed to reload blocked mints config: {}", e);
                }
            }
        });
    }

    /// 检查并重新加载配置 / Check and reload config
    async fn check_and_reload(&self) -> Result<(), Box<dyn std::error::Error>> {
        let current_modified = Self::get_file_modified_time(&self.config_path);
        let last_modified = *self.last_modified.read().await;

        if current_modified != last_modified {
            info!("Detected config file change, reloading blocked mints");
            let new_set = Self::load_config(&self.config_path)?;
            *self.blocked_set.write().await = new_set.clone();
            *self.last_modified.write().await = current_modified;
            info!("Reloaded {} blocked mints", new_set.len());
        }

        Ok(())
    }

    /// 加载配置文件 / Load config file
    fn load_config(path: &Path) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
        if !path.exists() {
            warn!("Blocked mints config file not found: {:?}, using empty list", path);
            return Ok(HashSet::new());
        }

        let content = fs::read_to_string(path)?;
        let config: BlockedMintsConfig = serde_json::from_str(&content)?;
        Ok(config.blocked_mints.into_iter().collect())
    }

    /// 获取文件修改时间 / Get file modification time
    fn get_file_modified_time(path: &Path) -> Option<SystemTime> {
        fs::metadata(path).ok()?.modified().ok()
    }
}
