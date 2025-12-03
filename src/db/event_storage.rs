// 事件存储模块 - 增强型复合键实施方案 / Event storage module - Enhanced composite key implementation
use anyhow::Result;
use rocksdb::{WriteBatch, IteratorMode, Direction, DB};
use serde::{Serialize, Deserialize};
use utoipa::ToSchema;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

use crate::solana::events::PinpetEvent;
use crate::router::db::PaginatedEvents;

/// 事件引用结构 - 用于索引 / Event reference structure - for indexing
#[derive(Serialize, Deserialize, Debug, Clone)]
struct EventRef {
    slot: u64,
    mint: String,
    sig8: String,
    event_type: String,
    idx: u32,
}

/// 签名引用结构 - 用于签名映射 / Signature reference structure - for signature mapping
#[derive(Serialize, Deserialize, Debug, Clone)]
struct SignatureRef {
    slot: u64,
    mint: String,
    event_type: String,
    idx: u32,
}

/// 事件存储服务 / Event storage service
pub struct EventStorage {
    db: Arc<DB>,
    kline_storage: crate::db::KlineStorage,
}

impl EventStorage {
    /// 创建新的事件存储服务 / Create new event storage service
    pub fn new(db: Arc<DB>) -> Result<Self> {
        let kline_storage = crate::db::KlineStorage::new(Arc::clone(&db));
        Ok(Self { db, kline_storage })
    }

    /// 生成8位短签名 / Generate 8-character short signature
    fn get_sig8(signature: &str) -> String {
        signature.chars().take(8).collect()
    }

    /// 获取事件类型编码 / Get event type code
    fn get_event_type_code(event: &PinpetEvent) -> &'static str {
        match event {
            PinpetEvent::TokenCreated(_) => "tc",
            PinpetEvent::BuySell(_) => "bs",
            PinpetEvent::LongShort(_) => "ls",
            PinpetEvent::FullClose(_) => "fc",
            PinpetEvent::PartialClose(_) => "pc",
            PinpetEvent::MilestoneDiscount(_) => "md",
        }
    }

    /// 提取事件的基础信息 / Extract basic event information
    fn extract_event_info(event: &PinpetEvent) -> (String, u64, String, Option<String>) {
        match event {
            PinpetEvent::TokenCreated(e) => {
                (e.mint_account.clone(), e.slot, e.signature.clone(), Some(e.payer.clone()))
            },
            PinpetEvent::BuySell(e) => {
                (e.mint_account.clone(), e.slot, e.signature.clone(), Some(e.payer.clone()))
            },
            PinpetEvent::LongShort(e) => {
                (e.mint_account.clone(), e.slot, e.signature.clone(), Some(e.payer.clone()))
            },
            PinpetEvent::FullClose(e) => {
                // 使用 user_sol_account 作为用户索引，因为 payer 可能是清算机器人而非订单所有者
                // Use user_sol_account for user index, as payer may be liquidator bot instead of order owner
                (e.mint_account.clone(), e.slot, e.signature.clone(), Some(e.user_sol_account.clone()))
            },
            PinpetEvent::PartialClose(e) => {
                // 使用 user_sol_account 作为用户索引，因为 payer 可能是清算机器人而非订单所有者
                // Use user_sol_account for user index, as payer may be liquidator bot instead of order owner
                (e.mint_account.clone(), e.slot, e.signature.clone(), Some(e.user_sol_account.clone()))
            },
            PinpetEvent::MilestoneDiscount(e) => {
                (e.mint_account.clone(), e.slot, e.signature.clone(), Some(e.payer.clone()))
            },
        }
    }

    /// 存储多个事件（同一签名）/ Store multiple events (same signature)
    pub async fn store_events(&self, signature: &str, events: Vec<PinpetEvent>) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        let sig8 = Self::get_sig8(signature);
        let mut batch = WriteBatch::default();
        let mut sig_refs = Vec::new();
        let mut slot_refs: HashMap<u64, Vec<EventRef>> = HashMap::new();

        // 按事件类型分组计数 / Group count by event type
        let mut type_counters: HashMap<String, u32> = HashMap::new();

        let events_len = events.len();  // 保存长度以供后面使用 / Save length for later use

        for event in &events {
            let event_type = Self::get_event_type_code(event).to_string();
            let (mint, slot, _, user) = Self::extract_event_info(event);

            // 获取或递增索引 / Get or increment index
            let idx = type_counters
                .entry(event_type.clone())
                .and_modify(|e| *e += 1)
                .or_insert(1);

            let idx_str = format!("{:03}", idx);
            let slot_str = format!("{:010}", slot);

            // 1. 存储主事件数据 / Store main event data
            let event_key = format!("event:{}:{}:{}:{}:{}",
                                   slot_str, mint, sig8, event_type, idx_str);
            let event_data = serde_json::to_vec(event)?;
            batch.put(event_key.as_bytes(), &event_data);

            // 2. 创建mint索引 / Create mint index
            let mint_idx = format!("idx_mint:{}:{}:{}:{}:{}",
                                  mint, slot_str, sig8, event_type, idx_str);
            batch.put(mint_idx.as_bytes(), b"");

            // 3. 创建user索引（如果有user）/ Create user index (if user exists)
            if let Some(user) = user {
                let user_idx = format!("idx_user:{}:{}:{}:{}:{}:{}",
                                      user, slot_str, mint, sig8, event_type, idx_str);
                batch.put(user_idx.as_bytes(), b"");
            }

            // 4. 收集签名引用 / Collect signature references
            sig_refs.push(SignatureRef {
                slot,
                mint: mint.clone(),
                event_type: event_type.clone(),
                idx: *idx,
            });

            // 5. 收集slot引用 / Collect slot references
            slot_refs.entry(slot).or_insert_with(Vec::new).push(EventRef {
                slot,
                mint: mint.clone(),
                sig8: sig8.clone(),
                event_type: event_type.clone(),
                idx: *idx,
            });
        }

        // 6. 存储签名映射 / Store signature mapping
        let sig_map_key = format!("sig_map:{}", signature);
        let sig_map_data = serde_json::to_vec(&sig_refs)?;
        batch.put(sig_map_key.as_bytes(), &sig_map_data);

        // 7. 更新slot批量索引 / Update slot batch index
        for (slot, refs) in slot_refs {
            self.update_slot_batch(&mut batch, slot, refs)?;
        }

        // 8. 原子提交所有更改 / Atomically commit all changes
        // 🔧 P0 修复: 使用 spawn_blocking 避免阻塞 Tokio 运行时 / P0 Fix: Use spawn_blocking to avoid blocking Tokio runtime
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            db.write(batch)
        }).await??;

        info!("成功存储 {} 个事件，签名: {} / Successfully stored {} events, signature: {}",
              events_len, signature, events_len, signature);

        // 9. 处理K线数据（对于包含价格信息的事件）/ Process K-line data (for events with price info)
        // 在事件存储完成后异步处理,避免阻塞 / Process asynchronously after event storage to avoid blocking
        for event in &events {
            match event {
                PinpetEvent::TokenCreated(e) => {
                    if let Err(err) = self.kline_storage
                        .process_kline_data(&e.mint_account, e.latest_price, e.timestamp)
                        .await
                    {
                        tracing::error!("❌ Failed to process kline data for TokenCreated event: {}", err);
                    }
                }
                PinpetEvent::BuySell(e) => {
                    if let Err(err) = self.kline_storage
                        .process_kline_data(&e.mint_account, e.latest_price, e.timestamp)
                        .await
                    {
                        tracing::error!("❌ Failed to process kline data for BuySell event: {}", err);
                    }
                }
                PinpetEvent::LongShort(e) => {
                    if let Err(err) = self.kline_storage
                        .process_kline_data(&e.mint_account, e.latest_price, e.timestamp)
                        .await
                    {
                        tracing::error!("❌ Failed to process kline data for LongShort event: {}", err);
                    }
                }
                PinpetEvent::FullClose(e) => {
                    if let Err(err) = self.kline_storage
                        .process_kline_data(&e.mint_account, e.latest_price, e.timestamp)
                        .await
                    {
                        tracing::error!("❌ Failed to process kline data for FullClose event: {}", err);
                    }
                }
                PinpetEvent::PartialClose(e) => {
                    if let Err(err) = self.kline_storage
                        .process_kline_data(&e.mint_account, e.latest_price, e.timestamp)
                        .await
                    {
                        tracing::error!("❌ Failed to process kline data for PartialClose event: {}", err);
                    }
                }
                _ => {
                    // 其他事件(仅MilestoneDiscount)不包含latest_price,无需处理K线 / Other events (only MilestoneDiscount) don't have latest_price, no kline processing needed
                }
            }
        }

        Ok(())
    }

    /// 更新slot批量索引 / Update slot batch index
    fn update_slot_batch(&self, batch: &mut WriteBatch, slot: u64, new_refs: Vec<EventRef>) -> Result<()> {
        let slot_key = format!("slot_batch:{:010}", slot);

        // 读取现有数据 / Read existing data
        let mut existing_refs = if let Ok(Some(data)) = self.db.get(slot_key.as_bytes()) {
            serde_json::from_slice::<Vec<EventRef>>(&data).unwrap_or_default()
        } else {
            Vec::new()
        };

        // 合并新数据 / Merge new data
        existing_refs.extend(new_refs);

        // 写入更新后的数据 / Write updated data
        let updated_data = serde_json::to_vec(&existing_refs)?;
        batch.put(slot_key.as_bytes(), &updated_data);

        Ok(())
    }

    /// 按mint_account查询事件 / Query events by mint_account
    /// 默认返回按slot降序排列的事件（最新的在前） / Returns events sorted by slot in descending order (newest first) by default
    pub async fn query_by_mint(&self, mint: &str, limit: Option<usize>, ascending: bool) -> Result<Vec<PinpetEvent>> {
        let prefix = format!("idx_mint:{}:", mint);
        let mut all_keys: Vec<String> = Vec::new();

        // 收集所有匹配的索引键 / Collect all matching index keys
        let iter = self.db.iterator(IteratorMode::From(
            prefix.as_bytes(),
            Direction::Forward
        ));

        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key).to_string();

            // 检查是否仍在prefix范围内 / Check if still within prefix range
            if !key_str.starts_with(&prefix) {
                break;
            }

            all_keys.push(key_str);
        }

        // 按slot排序（从键中提取slot）/ Sort by slot (extract slot from key)
        // idx_mint:{mint}:{slot:010}:{sig8}:{type}:{idx3}
        all_keys.sort_by(|a, b| {
            let slot_a = a.split(':').nth(2).unwrap_or("0");
            let slot_b = b.split(':').nth(2).unwrap_or("0");
            if ascending {
                slot_a.cmp(slot_b)  // 升序 / ascending
            } else {
                slot_b.cmp(slot_a)  // 降序 / descending
            }
        });

        // 应用limit限制 / Apply limit
        let keys_to_process = if let Some(limit) = limit {
            all_keys.iter().take(limit).collect::<Vec<_>>()
        } else {
            all_keys.iter().collect::<Vec<_>>()
        };

        // 获取事件数据 / Get event data
        let mut events = Vec::new();
        for key_str in keys_to_process {
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 6 {
                let slot = parts[2];
                let sig8 = parts[3];
                let event_type = parts[4];
                let idx = parts[5];

                let event_key = format!("event:{}:{}:{}:{}:{}",
                                       slot, mint, sig8, event_type, idx);

                if let Ok(Some(data)) = self.db.get(event_key.as_bytes()) {
                    if let Ok(event) = serde_json::from_slice::<PinpetEvent>(&data) {
                        events.push(event);
                    }
                }
            }
        }

        Ok(events)
    }

    /// 按signature查询所有相关事件 / Query all related events by signature
    pub async fn query_by_signature(&self, signature: &str) -> Result<Vec<PinpetEvent>> {
        let sig_map_key = format!("sig_map:{}", signature);

        // 获取签名映射 / Get signature mapping
        if let Ok(Some(data)) = self.db.get(sig_map_key.as_bytes()) {
            let sig_refs: Vec<SignatureRef> = serde_json::from_slice(&data)?;
            let sig8 = Self::get_sig8(signature);
            let mut events = Vec::new();

            for sig_ref in sig_refs {
                let event_key = format!("event:{:010}:{}:{}:{}:{:03}",
                                       sig_ref.slot, sig_ref.mint, sig8,
                                       sig_ref.event_type, sig_ref.idx);

                if let Ok(Some(data)) = self.db.get(event_key.as_bytes()) {
                    if let Ok(event) = serde_json::from_slice::<PinpetEvent>(&data) {
                        events.push(event);
                    }
                }
            }

            Ok(events)
        } else {
            Ok(Vec::new())
        }
    }

    /// 按user查询事件 / Query events by user
    pub async fn query_by_user(&self, user: &str, mint: Option<&str>, limit: Option<usize>) -> Result<Vec<PinpetEvent>> {
        let prefix = match mint {
            Some(m) => format!("idx_user:{}:{}:", user, m),
            None => format!("idx_user:{}:", user),
        };

        let mut events = Vec::new();
        let iter = self.db.iterator(IteratorMode::From(
            prefix.as_bytes(),
            Direction::Forward
        ));

        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);

            if !key_str.starts_with(&prefix) {
                break;
            }

            // 解析索引键 / Parse index key
            // idx_user:{user}:{slot:010}:{mint}:{sig8}:{type}:{idx3}
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 7 {
                let slot = parts[2];
                let mint = parts[3];
                let sig8 = parts[4];
                let event_type = parts[5];
                let idx = parts[6];

                let event_key = format!("event:{}:{}:{}:{}:{}",
                                       slot, mint, sig8, event_type, idx);

                if let Ok(Some(data)) = self.db.get(event_key.as_bytes()) {
                    if let Ok(event) = serde_json::from_slice::<PinpetEvent>(&data) {
                        events.push(event);

                        if let Some(limit) = limit {
                            if events.len() >= limit {
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok(events)
    }

    /// 按slot查询事件 / Query events by slot
    pub async fn query_by_slot(&self, slot: u64) -> Result<Vec<PinpetEvent>> {
        let slot_key = format!("slot_batch:{:010}", slot);

        if let Ok(Some(data)) = self.db.get(slot_key.as_bytes()) {
            let refs: Vec<EventRef> = serde_json::from_slice(&data)?;
            let mut events = Vec::new();

            for event_ref in refs {
                let event_key = format!("event:{:010}:{}:{}:{}:{:03}",
                                       event_ref.slot, event_ref.mint, event_ref.sig8,
                                       event_ref.event_type, event_ref.idx);

                if let Ok(Some(data)) = self.db.get(event_key.as_bytes()) {
                    if let Ok(event) = serde_json::from_slice::<PinpetEvent>(&data) {
                        events.push(event);
                    }
                }
            }

            Ok(events)
        } else {
            Ok(Vec::new())
        }
    }

    /// 按slot范围查询事件 / Query events by slot range
    pub async fn query_by_slot_range(&self, start_slot: u64, end_slot: u64) -> Result<Vec<PinpetEvent>> {
        let mut all_events = Vec::new();

        for slot in start_slot..=end_slot {
            let events = self.query_by_slot(slot).await?;
            all_events.extend(events);
        }

        // 按slot排序 / Sort by slot
        all_events.sort_by_key(|e| {
            let (_, slot, _, _) = Self::extract_event_info(e);
            slot
        });

        Ok(all_events)
    }

    /// 按mint_account查询事件（分页）/ Query events by mint_account (paginated)
    pub async fn query_by_mint_paginated(
        &self,
        mint: &str,
        page: u32,
        page_size: u32,
        ascending: bool,
    ) -> Result<PaginatedEvents> {
        let prefix = format!("idx_mint:{}:", mint);
        let mut all_keys: Vec<String> = Vec::new();

        // 收集所有匹配的索引键 / Collect all matching index keys
        let iter = self.db.iterator(IteratorMode::From(
            prefix.as_bytes(),
            Direction::Forward
        ));

        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key).to_string();

            // 检查是否仍在prefix范围内 / Check if still within prefix range
            if !key_str.starts_with(&prefix) {
                break;
            }

            all_keys.push(key_str);
        }

        // 按slot排序（从键中提取slot）/ Sort by slot (extract slot from key)
        // idx_mint:{mint}:{slot:010}:{sig8}:{type}:{idx3}
        all_keys.sort_by(|a, b| {
            let slot_a = a.split(':').nth(2).unwrap_or("0");
            let slot_b = b.split(':').nth(2).unwrap_or("0");
            if ascending {
                slot_a.cmp(slot_b)
            } else {
                slot_b.cmp(slot_a)
            }
        });

        let total = all_keys.len() as u64;
        let total_pages = ((total as f64) / (page_size as f64)).ceil() as u32;

        // 计算分页偏移 / Calculate pagination offset
        let start = ((page - 1) * page_size) as usize;
        let end = (start + page_size as usize).min(all_keys.len());

        // 获取当前页的事件 / Get events for current page
        let mut events = Vec::new();
        for key_str in all_keys.get(start..end).unwrap_or(&[]) {
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 6 {
                let slot = parts[2];
                let sig8 = parts[3];
                let event_type = parts[4];
                let idx = parts[5];

                let event_key = format!("event:{}:{}:{}:{}:{}",
                                       slot, mint, sig8, event_type, idx);

                if let Ok(Some(data)) = self.db.get(event_key.as_bytes()) {
                    if let Ok(event) = serde_json::from_slice::<PinpetEvent>(&data) {
                        events.push(event);
                    }
                }
            }
        }

        Ok(PaginatedEvents {
            events,
            total,
            page,
            page_size,
            total_pages,
        })
    }

    /// 按user查询事件（分页）/ Query events by user (paginated)
    pub async fn query_by_user_paginated(
        &self,
        user: &str,
        mint: Option<&str>,
        page: u32,
        page_size: u32,
        ascending: bool,
    ) -> Result<PaginatedEvents> {
        let prefix = format!("idx_user:{}:", user);
        let mut all_keys: Vec<String> = Vec::new();

        // 收集所有匹配的索引键 / Collect all matching index keys
        let iter = self.db.iterator(IteratorMode::From(
            prefix.as_bytes(),
            Direction::Forward
        ));

        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key).to_string();

            // 检查是否仍在prefix范围内 / Check if still within prefix range
            if !key_str.starts_with(&prefix) {
                break;
            }

            // 如果指定了mint，进行过滤 / Filter by mint if specified
            // idx_user:{user}:{slot:010}:{mint}:{sig8}:{type}:{idx3}
            if let Some(filter_mint) = mint {
                let parts: Vec<&str> = key_str.split(':').collect();
                if parts.len() >= 4 && parts[3] != filter_mint {
                    continue;
                }
            }

            all_keys.push(key_str);
        }

        // 按slot排序（从键中提取slot）/ Sort by slot (extract slot from key)
        // idx_user:{user}:{slot:010}:{mint}:{sig8}:{type}:{idx3}
        all_keys.sort_by(|a, b| {
            let slot_a = a.split(':').nth(2).unwrap_or("0");
            let slot_b = b.split(':').nth(2).unwrap_or("0");
            if ascending {
                slot_a.cmp(slot_b)
            } else {
                slot_b.cmp(slot_a)
            }
        });

        let total = all_keys.len() as u64;
        let total_pages = ((total as f64) / (page_size as f64)).ceil() as u32;

        // 计算分页偏移 / Calculate pagination offset
        let start = ((page - 1) * page_size) as usize;
        let end = (start + page_size as usize).min(all_keys.len());

        // 获取当前页的事件 / Get events for current page
        let mut events = Vec::new();
        for key_str in all_keys.get(start..end).unwrap_or(&[]) {
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 7 {
                let slot = parts[2];
                let mint = parts[3];
                let sig8 = parts[4];
                let event_type = parts[5];
                let idx = parts[6];

                let event_key = format!("event:{}:{}:{}:{}:{}",
                                       slot, mint, sig8, event_type, idx);

                if let Ok(Some(data)) = self.db.get(event_key.as_bytes()) {
                    if let Ok(event) = serde_json::from_slice::<PinpetEvent>(&data) {
                        events.push(event);
                    }
                }
            }
        }

        Ok(PaginatedEvents {
            events,
            total,
            page,
            page_size,
            total_pages,
        })
    }

    /// 获取数据库中的总键值对数量 / Get total key-value count in database
    pub fn get_total_key_count(&self) -> Result<u64> {
        let mut count = 0u64;
        let iter = self.db.iterator(IteratorMode::Start);

        for item in iter {
            if item.is_ok() {
                count += 1;
            }
        }

        Ok(count)
    }

    /// 获取数据库的估计大小（字节）/ Get estimated database size in bytes
    pub fn get_estimated_db_size(&self) -> Result<u64> {
        let mut total_size = 0u64;

        // 获取各种数据库属性来估算大小 / Get various database properties to estimate size
        if let Ok(Some(value)) = self.db.property_value("rocksdb.estimate-live-data-size") {
            if let Ok(size) = value.parse::<u64>() {
                total_size = size;
            }
        }

        // 如果无法获取live-data-size，尝试其他属性 / If can't get live-data-size, try other properties
        if total_size == 0 {
            if let Ok(Some(value)) = self.db.property_value("rocksdb.total-sst-files-size") {
                if let Ok(size) = value.parse::<u64>() {
                    total_size = size;
                }
            }
        }

        Ok(total_size)
    }

    /// 获取数据库统计信息 / Get database statistics
    pub fn get_db_stats(&self) -> Result<DatabaseStats> {
        let key_count = self.get_total_key_count()?;
        let db_size_bytes = self.get_estimated_db_size()?;

        // 统计各类型的事件数量和键值总大小 / Count events by type and total KV size
        let mut event_counts = HashMap::new();
        let mut mint_count = 0;
        let mut user_count = 0;
        let mut signature_count = 0;
        let mut slot_count = 0;
        let mut total_kv_size: u64 = 0;

        let iter = self.db.iterator(IteratorMode::Start);
        for item in iter {
            if let Ok((key, value)) = item {
                // 累加键值大小 / Accumulate key-value size
                total_kv_size += key.len() as u64 + value.len() as u64;

                let key_str = String::from_utf8_lossy(&key);

                if key_str.starts_with("event:") {
                    // 解析事件类型 / Parse event type
                    let parts: Vec<&str> = key_str.split(':').collect();
                    if parts.len() >= 5 {
                        let event_type = parts[4];
                        *event_counts.entry(event_type.to_string()).or_insert(0) += 1;
                    }
                } else if key_str.starts_with("idx_mint:") {
                    mint_count += 1;
                } else if key_str.starts_with("idx_user:") {
                    user_count += 1;
                } else if key_str.starts_with("sig_map:") {
                    signature_count += 1;
                } else if key_str.starts_with("slot_batch:") {
                    slot_count += 1;
                }
            }
        }

        Ok(DatabaseStats {
            total_keys: key_count,
            total_kv_size_bytes: total_kv_size,
            total_kv_size_mb: total_kv_size as f64 / (1024.0 * 1024.0),
            database_size_bytes: db_size_bytes,
            database_size_mb: db_size_bytes as f64 / (1024.0 * 1024.0),
            event_counts,
            index_counts: IndexCounts {
                mint_indices: mint_count,
                user_indices: user_count,
                signature_mappings: signature_count,
                slot_batches: slot_count,
            },
        })
    }

    /// 查询K线数据 / Query K-line data
    /// 代理到KlineStorage的查询方法 / Delegate to KlineStorage's query method
    pub async fn query_kline_data(&self, query: crate::kline::types::KlineQuery) -> Result<crate::kline::types::KlineQueryResponse> {
        self.kline_storage.query_kline_data(query).await
    }

    /// 获取特定时间桶的K线数据 / Get K-line data for specific time bucket
    /// 用于实时推送时读取数据库中的K线数据 / Used for reading K-line data from DB during real-time push
    pub async fn get_kline_data(
        &self,
        mint: &str,
        interval: &str,
        time: u64,
    ) -> Result<Option<crate::kline::types::KlineData>> {
        self.kline_storage.get_kline_by_time(mint, interval, time).await
    }
}

/// 数据库统计信息 / Database statistics
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(title = "DatabaseStats", description = "数据库键值统计信息")]
pub struct DatabaseStats {
    /// 总键数量 / Total number of keys
    #[schema(example = 10000)]
    pub total_keys: u64,
    /// 键值总大小（字节）/ Total size of all keys and values (bytes)
    #[schema(example = 2097152)]
    pub total_kv_size_bytes: u64,
    /// 键值总大小（MB）/ Total size of all keys and values (MB)
    #[schema(example = 2.0)]
    pub total_kv_size_mb: f64,
    /// 数据库文件大小（字节）/ Database file size (bytes)
    #[schema(example = 1048576)]
    pub database_size_bytes: u64,
    /// 数据库文件大小（MB）/ Database file size (MB)
    #[schema(example = 1.0)]
    pub database_size_mb: f64,
    /// 各事件类型计数 / Event counts by type
    pub event_counts: HashMap<String, u64>,
    /// 索引计数 / Index counts
    pub index_counts: IndexCounts,
}

/// 索引计数 / Index counts
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(title = "IndexCounts", description = "索引计数统计")]
pub struct IndexCounts {
    #[schema(example = 100)]
    pub mint_indices: u64,
    #[schema(example = 200)]
    pub user_indices: u64,
    #[schema(example = 50)]
    pub signature_mappings: u64,
    #[schema(example = 30)]
    pub slot_batches: u64,
}