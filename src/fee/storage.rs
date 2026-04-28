// 手续费统计存储模块 / Fee Statistics Storage Module
use super::types::{FeeData, FeeType};
use crate::volume::Period;
use anyhow::{Context, Result};
use rocksdb::DB;
use std::sync::Arc;
use tracing::debug;

/// 手续费存储 / Fee Storage
pub struct FeeStorage {
    db: Arc<DB>,
}

impl FeeStorage {
    /// 创建新的手续费存储 / Create new fee storage
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    /// 更新手续费统计 / Update fee statistics
    ///
    /// # 参数 / Parameters
    /// * `mint` - Token mint 地址 / Token mint address
    /// * `fee_lamports` - 手续费金额 (lamports) / Fee amount (lamports)
    /// * `fee_type` - 手续费类型 / Fee type
    /// * `timestamp` - 事件时间戳 / Event timestamp
    pub fn update_fee(
        &self,
        mint: &str,
        fee_lamports: u64,
        fee_type: FeeType,
        timestamp: u64,
    ) -> Result<()> {
        if fee_lamports == 0 {
            return Ok(());
        }

        // 更新所有时间周期 / Update all time periods
        for period in Period::all() {
            let time_bucket = period.align_timestamp(timestamp);
            self.update_period_fee(mint, period, time_bucket, fee_lamports, fee_type, timestamp)?;

            // 同时更新排序索引 / Also update ranking index
            self.update_fee_rank(mint, period, time_bucket)?;
        }

        // 更新全量累计 / Update all-time accumulation
        self.update_all_time_fee(mint, fee_lamports, fee_type, timestamp)?;

        Ok(())
    }

    /// 更新单个时间周期的手续费 / Update fee for a single period
    fn update_period_fee(
        &self,
        mint: &str,
        period: Period,
        time_bucket: u64,
        fee_lamports: u64,
        fee_type: FeeType,
        timestamp: u64,
    ) -> Result<()> {
        // 键格式: fee:{period}:{mint}:{time_bucket:020}
        // Key format: fee:{period}:{mint}:{time_bucket:020}
        let key = format!("fee:{}:{}:{:020}", period.as_str(), mint, time_bucket);

        // 读取现有数据 / Read existing data
        let mut data = match self.db.get(key.as_bytes())? {
            Some(bytes) => serde_json::from_slice::<FeeData>(&bytes)
                .context("Failed to deserialize fee data")?,
            None => FeeData::new(),
        };

        // 根据类型累加 / Accumulate by type
        match fee_type {
            FeeType::Swap => data.add_swap_fee(fee_lamports, timestamp),
            FeeType::Borrow => data.add_borrow_fee(fee_lamports, timestamp),
            FeeType::Liquidate => data.add_liquidate_fee(fee_lamports, timestamp),
        }

        // 保存数据 / Save data
        let value = serde_json::to_vec(&data).context("Failed to serialize fee data")?;
        self.db.put(key.as_bytes(), value)?;

        debug!(
            "Updated period fee: period={}, mint={}, fee={} lamports, type={:?}, total={}",
            period.as_str(),
            &mint[..8.min(mint.len())],
            fee_lamports,
            fee_type,
            data.total_fee
        );

        Ok(())
    }

    /// 更新手续费排序索引 / Update fee ranking index
    fn update_fee_rank(&self, mint: &str, period: Period, time_bucket: u64) -> Result<()> {
        // 1. 读取当前手续费 / Read current fee
        let main_key = format!("fee:{}:{}:{:020}", period.as_str(), mint, time_bucket);
        let data = match self.db.get(main_key.as_bytes())? {
            Some(bytes) => serde_json::from_slice::<FeeData>(&bytes)?,
            None => return Ok(()),
        };

        // 2. 删除旧的排序索引 / Delete old ranking index
        // 键格式: fee_rank:{period}:{time_bucket:020}:{total_fee:020}:{mint}
        let prefix = format!("fee_rank:{}:{:020}:", period.as_str(), time_bucket);
        let mut old_key_to_delete: Option<Vec<u8>> = None;

        let iter = self.db.prefix_iterator(prefix.as_bytes());
        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if !key_str.starts_with(&prefix) {
                break;
            }
            if key_str.ends_with(&format!(":{}", mint)) {
                old_key_to_delete = Some(key.to_vec());
                break;
            }
        }

        if let Some(old_key) = old_key_to_delete {
            self.db.delete(&old_key)?;
        }

        // 3. 插入新的排序索引 / Insert new ranking index
        let new_key = format!(
            "fee_rank:{}:{:020}:{:020}:{}",
            period.as_str(),
            time_bucket,
            data.total_fee,
            mint
        );
        self.db.put(new_key.as_bytes(), b"")?;

        Ok(())
    }

    /// 更新全量累计手续费 / Update all-time accumulated fee
    fn update_all_time_fee(
        &self,
        mint: &str,
        fee_lamports: u64,
        fee_type: FeeType,
        timestamp: u64,
    ) -> Result<()> {
        // 键格式: fee_all:{mint}
        let key = format!("fee_all:{}", mint);

        // 读取现有数据 / Read existing data
        let mut data = match self.db.get(key.as_bytes())? {
            Some(bytes) => serde_json::from_slice::<FeeData>(&bytes)
                .context("Failed to deserialize all-time fee data")?,
            None => FeeData::new(),
        };

        // 根据类型累加 / Accumulate by type
        match fee_type {
            FeeType::Swap => data.add_swap_fee(fee_lamports, timestamp),
            FeeType::Borrow => data.add_borrow_fee(fee_lamports, timestamp),
            FeeType::Liquidate => data.add_liquidate_fee(fee_lamports, timestamp),
        }

        // 保存数据 / Save data
        let value = serde_json::to_vec(&data).context("Failed to serialize all-time fee data")?;
        self.db.put(key.as_bytes(), value)?;

        Ok(())
    }

    /// 查询24小时手续费 / Query 24h fee
    pub fn get_fee_24h(&self, mint: &str) -> Result<FeeData> {
        let period = Period::TwentyFourHours;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let time_bucket = period.align_timestamp(now);

        let key = format!("fee:{}:{}:{:020}", period.as_str(), mint, time_bucket);
        match self.db.get(key.as_bytes())? {
            Some(bytes) => serde_json::from_slice::<FeeData>(&bytes)
                .context("Failed to deserialize fee data"),
            None => Ok(FeeData::new()),
        }
    }

    /// 查询全量累计手续费 / Query all-time accumulated fee
    pub fn get_fee_all(&self, mint: &str) -> Result<FeeData> {
        let key = format!("fee_all:{}", mint);
        match self.db.get(key.as_bytes())? {
            Some(bytes) => serde_json::from_slice::<FeeData>(&bytes)
                .context("Failed to deserialize all-time fee data"),
            None => Ok(FeeData::new()),
        }
    }
}
