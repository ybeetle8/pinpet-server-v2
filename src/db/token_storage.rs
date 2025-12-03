// Token存储模块 - 代币列表键值存储系统
// Token storage module - Token list key-value storage system

use crate::config::Config;

use crate::solana::events::TokenCreatedEvent;
use anyhow::Result;
use chrono::Utc;
use rocksdb::{WriteBatch, DB};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use utoipa::ToSchema;

/// Token详情数据结构 / Token detail data structure
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenDetail {
    // ===== TokenCreatedEvent 原始字段 / Original Fields =====
    pub payer: String,                      // 创建者地址 / Creator address
    pub mint_account: String,               // 代币mint地址 / Token mint address
    pub curve_account: String,              // 曲线账户地址 / Curve account address
    pub pool_token_account: String,         // 池子代币账户 / Pool token account
    pub pool_sol_account: String,           // 池子SOL账户 / Pool SOL account
    pub fee_recipient: String,              // 手续费接收地址 / Fee recipient address
    pub base_fee_recipient: String,         // 基础手续费接收地址 / Base fee recipient address
    pub params_account: String,             // 参数账户PDA地址 / Params account PDA address
    pub swap_fee: u16,                      // 现货交易手续费 / Spot trading fee
    pub borrow_fee: u16,                    // 保证金交易手续费 / Margin trading fee
    pub fee_discount_flag: u8,              // 手续费折扣标志 / Fee discount flag
    pub name: String,                       // 代币名称 / Token name
    pub symbol: String,                     // 代币符号 / Token symbol
    pub uri: String,                        // 元数据URI / Metadata URI
    pub up_orderbook: String,               // 做空订单簿地址 / Short orderbook address
    pub down_orderbook: String,             // 做多订单簿地址 / Long orderbook address
    pub latest_price: String,               // 最新价格 / Latest price (u128 as string)

    // ===== 时间戳信息 / Timestamp Info =====
    pub created_at: i64,                    // 创建时间Unix时间戳 / Creation Unix timestamp
    pub created_slot: u64,                  // 创建时的slot / Creation slot
    pub updated_at: i64,                    // 最后更新时间 / Last update timestamp

    // ===== URI 解析数据 / URI Parsed Data =====
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri_data: Option<TokenUriData>,     // IPFS解析后的元数据 / IPFS parsed metadata

    // ===== 统计数据 / Statistics =====
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<TokenStats>,          // 代币统计信息 / Token statistics

    // ===== 扩展字段 / Extension Fields =====
    #[serde(default)]
    pub extras: HashMap<String, Value>,     // 预留扩展字段 / Reserved extension fields
}

/// Token URI 元数据 / Token URI metadata
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenUriData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,               // 展示名称 / Display name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,             // 展示符号 / Display symbol
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,        // 描述信息 / Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,              // 图片URI / Image URI
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_name: Option<bool>,            // 是否显示名称 / Show name flag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_on: Option<String>,         // 创建日期 / Creation date
    #[serde(skip_serializing_if = "Option::is_none")]
    pub twitter: Option<String>,            // Twitter链接 / Twitter link
    #[serde(skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,            // 官网链接 / Website link
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telegram: Option<String>,           // Telegram链接 / Telegram link
}

/// Token 统计数据 / Token statistics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenStats {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_cap: Option<String>,         // 市值 / Market cap (u128 as string)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_24h: Option<String>,         // 24小时交易量 / 24h volume (u128 as string)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holders: Option<u64>,               // 持币地址数 / Holders count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_change_24h: Option<f64>,      // 24小时价格变化 / 24h price change
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liquidity: Option<String>,          // 流动性 / Liquidity (u128 as string)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_supply: Option<String>,       // 总供应量 / Total supply (u128 as string)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub circulating_supply: Option<String>, // 流通供应量 / Circulating supply (u128 as string)
}

/// Token存储管理器 / Token storage manager
pub struct TokenStorage {
    db: Arc<DB>,
    config: Config,
    http_client: reqwest::Client,
}

impl TokenStorage {
    /// 创建新的Token存储管理器 / Create new token storage manager
    pub fn new(db: Arc<DB>, config: Config) -> Result<Self> {
        let timeout = Duration::from_secs(config.ipfs.request_timeout_seconds);
        let http_client = reqwest::Client::builder()
            .timeout(timeout)
            .build()?;

        Ok(Self {
            db,
            config,
            http_client,
        })
    }

    /// 从TokenCreatedEvent保存Token / Save token from TokenCreatedEvent
    pub async fn save_token_from_event(&self, event: &TokenCreatedEvent) -> Result<()> {
        info!(
            "保存Token详情 / Saving token detail: mint={}, symbol={}",
            event.mint_account, event.symbol
        );

        let now = Utc::now().timestamp();

        // 构建TokenDetail / Build TokenDetail
        let mut detail = TokenDetail {
            payer: event.payer.clone(),
            mint_account: event.mint_account.clone(),
            curve_account: event.curve_account.clone(),
            pool_token_account: event.pool_token_account.clone(),
            pool_sol_account: event.pool_sol_account.clone(),
            fee_recipient: event.fee_recipient.clone(),
            base_fee_recipient: event.base_fee_recipient.clone(),
            params_account: event.params_account.clone(),
            swap_fee: event.swap_fee,
            borrow_fee: event.borrow_fee,
            fee_discount_flag: event.fee_discount_flag,
            name: event.name.clone(),
            symbol: event.symbol.clone(),
            uri: event.uri.clone(),
            up_orderbook: event.up_orderbook.clone(),
            down_orderbook: event.down_orderbook.clone(),
            latest_price: event.latest_price.to_string(),
            created_at: event.timestamp.timestamp(),
            created_slot: event.slot,
            updated_at: now,
            uri_data: None,
            stats: None,
            extras: HashMap::new(),
        };

        // 异步获取URI数据 / Fetch URI data asynchronously
        if !event.uri.is_empty() {
            debug!(
                "开始获取IPFS元数据 / Starting IPFS metadata fetch: uri={}",
                event.uri
            );
            if let Some(uri_data) = self.fetch_token_uri_data(&event.uri).await {
                detail.uri_data = Some(uri_data);
                debug!("成功获取IPFS元数据 / Successfully fetched IPFS metadata");
            } else {
                warn!(
                    "无法获取IPFS元数据 / Failed to fetch IPFS metadata: uri={}",
                    event.uri
                );
            }
        }

        // 🔧 P0 修复: 原子写入所有索引使用 spawn_blocking / P0 Fix: Atomic write all indexes using spawn_blocking
        let db = Arc::clone(&self.db);
        let detail_clone = detail.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut batch = WriteBatch::default();

            // 1. 主存储 / Main storage: token:{mint}
            let main_key = format!("token:{}", detail_clone.mint_account);
            let value = serde_json::to_vec(&detail_clone)?;
            batch.put(main_key.as_bytes(), &value);

            // 2. Symbol索引 / Symbol index: token_symbol:{SYMBOL}:{mint}
            let symbol_key = format!(
                "token_symbol:{}:{}",
                detail_clone.symbol.to_uppercase(),
                detail_clone.mint_account
            );
            batch.put(symbol_key.as_bytes(), b"");

            // 3. 创建时间索引 / Creation time index: token_created:{timestamp:010}:{mint}
            let time_key = format!(
                "token_created:{:010}:{}",
                detail_clone.created_at, detail_clone.mint_account
            );
            batch.put(time_key.as_bytes(), b"");

            // 4. Slot索引 / Slot index: token_slot:{slot:010}:{mint}
            let slot_key = format!(
                "token_slot:{:010}:{}",
                detail_clone.created_slot, detail_clone.mint_account
            );
            batch.put(slot_key.as_bytes(), b"");

            // 5. 创建者索引 / Creator index: token_payer:{payer}:{timestamp:010}:{mint}
            let payer_key = format!(
                "token_payer:{}:{:010}:{}",
                detail_clone.payer, detail_clone.created_at, detail_clone.mint_account
            );
            batch.put(payer_key.as_bytes(), b"");

            // 原子提交 / Atomic commit
            db.write(batch)?;
            Ok(())
        }).await??;

        info!(
            "Token详情保存成功 / Token detail saved successfully: mint={}",
            event.mint_account
        );
        Ok(())
    }

    /// 使用WriteBatch原子保存Token及其索引 / Save token with indexes atomically using WriteBatch
    fn save_token_with_indexes(&self, detail: &TokenDetail) -> Result<()> {
        let mut batch = WriteBatch::default();

        // 1. 主存储 / Main storage: token:{mint}
        let main_key = format!("token:{}", detail.mint_account);
        let value = serde_json::to_vec(detail)?;
        batch.put(main_key.as_bytes(), &value);

        // 2. Symbol索引 / Symbol index: token_symbol:{SYMBOL}:{mint}
        let symbol_key = format!(
            "token_symbol:{}:{}",
            detail.symbol.to_uppercase(),
            detail.mint_account
        );
        batch.put(symbol_key.as_bytes(), b"");

        // 3. 创建时间索引 / Creation time index: token_created:{timestamp:010}:{mint}
        let time_key = format!(
            "token_created:{:010}:{}",
            detail.created_at, detail.mint_account
        );
        batch.put(time_key.as_bytes(), b"");

        // 4. Slot索引 / Slot index: token_slot:{slot:010}:{mint}
        let slot_key = format!(
            "token_slot:{:010}:{}",
            detail.created_slot, detail.mint_account
        );
        batch.put(slot_key.as_bytes(), b"");

        // 5. 创建者索引（可选）/ Creator index (optional): token_payer:{payer}:{timestamp:010}:{mint}
        let payer_key = format!(
            "token_payer:{}:{:010}:{}",
            detail.payer, detail.created_at, detail.mint_account
        );
        batch.put(payer_key.as_bytes(), b"");

        // 🔧 P0 修复: 原子提交使用 spawn_blocking / P0 Fix: Atomic commit using spawn_blocking
        // 注意: 这是同步函数,但在 save_token_from_event (异步) 调用链中被 spawn_blocking 包装
        // Note: This is a sync function, but wrapped in spawn_blocking via save_token_from_event (async) call chain
        self.db.write(batch)?;

        debug!(
            "Token及索引已原子写入 / Token and indexes written atomically: mint={}",
            detail.mint_account
        );
        Ok(())
    }

    /// 根据mint获取Token详情 / Get token by mint
    pub fn get_token_by_mint(&self, mint: &str) -> Result<Option<TokenDetail>> {
        let key = format!("token:{}", mint);
        match self.db.get(key.as_bytes())? {
            Some(data) => {
                let detail: TokenDetail = serde_json::from_slice(&data)?;
                Ok(Some(detail))
            }
            None => Ok(None),
        }
    }

    /// 根据symbol查询Token列表 / Get tokens by symbol
    pub fn get_tokens_by_symbol(
        &self,
        symbol: &str,
        limit: usize,
        cursor: Option<String>,
    ) -> Result<Vec<TokenDetail>> {
        let prefix = format!("token_symbol:{}:", symbol.to_uppercase());
        let start_key = if let Some(c) = cursor {
            c
        } else {
            prefix.clone()
        };

        let iter = self.db.iterator(rocksdb::IteratorMode::From(
            start_key.as_bytes(),
            rocksdb::Direction::Forward,
        ));

        let mut tokens = Vec::new();
        let mut count = 0;

        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);

            if !key_str.starts_with(&prefix) {
                break;
            }

            if count >= limit {
                break;
            }

            // 从key中提取mint地址 / Extract mint from key
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 3 {
                let mint = parts[2];
                if let Ok(Some(detail)) = self.get_token_by_mint(mint) {
                    tokens.push(detail);
                    count += 1;
                }
            }
        }

        Ok(tokens)
    }

    /// 获取最新Token列表 / Get latest tokens
    pub fn get_latest_tokens(
        &self,
        limit: usize,
        before_timestamp: Option<i64>,
    ) -> Result<Vec<TokenDetail>> {
        let prefix = "token_created:";
        let start_key = if let Some(ts) = before_timestamp {
            format!("token_created:{:010}:", ts)
        } else {
            // 使用一个很大的时间戳从最新开始 / Use a large timestamp to start from latest
            format!("token_created:{:010}:", i64::MAX)
        };

        let iter = self.db.iterator(rocksdb::IteratorMode::From(
            start_key.as_bytes(),
            rocksdb::Direction::Reverse,
        ));

        let mut tokens = Vec::new();
        let mut count = 0;

        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);

            if !key_str.starts_with(prefix) {
                break;
            }

            if count >= limit {
                break;
            }

            // 从key中提取mint地址 / Extract mint from key
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 3 {
                let mint = parts[2];
                if let Ok(Some(detail)) = self.get_token_by_mint(mint) {
                    tokens.push(detail);
                    count += 1;
                }
            }
        }

        Ok(tokens)
    }

    /// 按slot范围查询Token / Get tokens by slot range
    pub fn get_tokens_by_slot_range(
        &self,
        start_slot: u64,
        end_slot: u64,
    ) -> Result<Vec<TokenDetail>> {
        let start = format!("token_slot:{:010}:", start_slot);
        let end = format!("token_slot:{:010}:", end_slot + 1);

        let iter = self.db.iterator(rocksdb::IteratorMode::From(
            start.as_bytes(),
            rocksdb::Direction::Forward,
        ));

        let mut tokens = Vec::new();

        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);

            if key_str.as_ref() >= end.as_str() {
                break;
            }

            if !key_str.starts_with("token_slot:") {
                continue;
            }

            // 从key中提取mint地址 / Extract mint from key
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 3 {
                let mint = parts[2];
                if let Ok(Some(detail)) = self.get_token_by_mint(mint) {
                    tokens.push(detail);
                }
            }
        }

        Ok(tokens)
    }

    /// 批量获取Token详情 / Batch get tokens
    pub fn batch_get_tokens(&self, mints: Vec<String>) -> Result<Vec<TokenDetail>> {
        let mut tokens = Vec::new();
        for mint in mints {
            if let Ok(Some(detail)) = self.get_token_by_mint(&mint) {
                tokens.push(detail);
            }
        }
        Ok(tokens)
    }

    /// 从IPFS URI提取hash / Extract IPFS hash from URI
    fn extract_ipfs_hash(uri: &str) -> Option<String> {
        if uri.starts_with("ipfs://") {
            let hash = &uri[7..];
            let end_pos = hash.find('?').unwrap_or(hash.len());
            Some(hash[..end_pos].to_string())
        } else if uri.contains("/ipfs/") {
            if let Some(start) = uri.find("/ipfs/") {
                let hash = &uri[start + 6..];
                let end_pos = hash.find('?').unwrap_or(hash.len());
                Some(hash[..end_pos].to_string())
            } else {
                None
            }
        } else {
            None
        }
    }

    /// 从IPFS获取Token元数据（带重试逻辑）/ Fetch token metadata from IPFS with retry logic
    async fn fetch_token_uri_data(&self, uri: &str) -> Option<TokenUriData> {
        let ipfs_hash = Self::extract_ipfs_hash(uri)?;
        let ipfs_url = format!("{}{}", self.config.ipfs.gateway_url, ipfs_hash);

        debug!("从IPFS获取Token元数据 / Fetching token metadata from IPFS: {}", ipfs_url);

        for attempt in 1..=self.config.ipfs.max_retries {
            match self.http_client.get(&ipfs_url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<TokenUriData>().await {
                            Ok(uri_data) => {
                                debug!(
                                    "成功获取Token元数据 / Successfully fetched token metadata: uri={}",
                                    uri
                                );
                                return Some(uri_data);
                            }
                            Err(e) => {
                                warn!(
                                    "解析IPFS JSON失败 (尝试 {}/{}) / Failed to parse IPFS JSON (attempt {}/{}): {}",
                                    attempt, self.config.ipfs.max_retries, attempt, self.config.ipfs.max_retries, e
                                );
                            }
                        }
                    } else {
                        warn!(
                            "IPFS网关HTTP错误 (尝试 {}/{}) / IPFS gateway HTTP error (attempt {}/{}): {}",
                            attempt,
                            self.config.ipfs.max_retries,
                            attempt,
                            self.config.ipfs.max_retries,
                            response.status()
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        "从IPFS获取数据网络错误 (尝试 {}/{}) / Network error fetching from IPFS (attempt {}/{}): {}",
                        attempt, self.config.ipfs.max_retries, attempt, self.config.ipfs.max_retries, e
                    );
                }
            }

            // 重试前休眠（除了最后一次尝试）/ Sleep before retry (except last attempt)
            if attempt < self.config.ipfs.max_retries {
                sleep(Duration::from_secs(self.config.ipfs.retry_delay_seconds)).await;
            }
        }

        error!(
            "在{}次尝试后无法获取Token元数据 / Failed to fetch token metadata after {} attempts: uri={}",
            self.config.ipfs.max_retries, self.config.ipfs.max_retries, uri
        );
        None
    }

    /// 获取Token总数统计 / Get token count statistics
    pub fn get_token_count(&self) -> Result<u64> {
        let prefix = "token:";
        let mut count = 0u64;

        let iter = self.db.iterator(rocksdb::IteratorMode::From(
            prefix.as_bytes(),
            rocksdb::Direction::Forward,
        ));

        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);

            if !key_str.starts_with(prefix) {
                break;
            }

            count += 1;
        }

        Ok(count)
    }

    /// 更新Token的latest_price / Update token's latest_price
    pub fn update_token_price(&self, mint: &str, latest_price: u128) -> Result<()> {
        let key = format!("token:{}", mint);

        // 读取现有Token详情 / Read existing token detail
        match self.db.get(key.as_bytes())? {
            Some(data) => {
                let mut detail: TokenDetail = serde_json::from_slice(&data)?;

                // 更新价格和时间戳 / Update price and timestamp
                detail.latest_price = latest_price.to_string();
                detail.updated_at = Utc::now().timestamp();

                // 写回数据库 / Write back to database
                let value = serde_json::to_vec(&detail)?;
                self.db.put(key.as_bytes(), &value)?;

                debug!(
                    "Token价格已更新 / Token price updated: mint={}, latest_price={}",
                    mint, latest_price
                );
                Ok(())
            }
            None => {
                warn!(
                    "无法更新价格，Token不存在 / Cannot update price, token not found: mint={}",
                    mint
                );
                // 不抛出错误，因为可能事件到达顺序不同 / Don't throw error, events may arrive out of order
                Ok(())
            }
        }
    }

    /// 更新Token的费率字段 / Update token's fee fields
    pub fn update_token_fees(
        &self,
        mint: &str,
        swap_fee: u16,
        borrow_fee: u16,
        fee_discount_flag: u8,
    ) -> Result<()> {
        let key = format!("token:{}", mint);

        // 读取现有Token详情 / Read existing token detail
        match self.db.get(key.as_bytes())? {
            Some(data) => {
                let mut detail: TokenDetail = serde_json::from_slice(&data)?;

                // 更新费率字段和时间戳 / Update fee fields and timestamp
                detail.swap_fee = swap_fee;
                detail.borrow_fee = borrow_fee;
                detail.fee_discount_flag = fee_discount_flag;
                detail.updated_at = Utc::now().timestamp();

                // 写回数据库 / Write back to database
                let value = serde_json::to_vec(&detail)?;
                self.db.put(key.as_bytes(), &value)?;

                debug!(
                    "Token费率已更新 / Token fees updated: mint={}, swap_fee={}, borrow_fee={}, fee_discount_flag={}",
                    mint, swap_fee, borrow_fee, fee_discount_flag
                );
                Ok(())
            }
            None => {
                warn!(
                    "无法更新费率，Token不存在 / Cannot update fees, token not found: mint={}",
                    mint
                );
                // 不抛出错误，因为可能事件到达顺序不同 / Don't throw error, events may arrive out of order
                Ok(())
            }
        }
    }
}
