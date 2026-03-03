use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::Duration;
use base64::{Engine as _, engine::general_purpose};

/// Raydium SOL/USDT CLMM 池地址 / Raydium SOL/USDT CLMM pool address
const SOL_USDT_POOL: &str = "3nMFwZXwY1s1M5s8vYAHqd4wGs4iSxXE4LRoUMMYqEgF";

/// Wrapped SOL Mint 地址 / Wrapped SOL Mint address
const WRAPPED_SOL_MINT: &str = "So11111111111111111111111111111111111111112";

/// USDT Mint 地址 / USDT Mint address
const USDT_MINT: &str = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";

/// SOL 价格信息 / SOL price information
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SolPrice {
    /// 当前价格(USD) / Current price (USD)
    #[schema(example = 139.41)]
    pub price: f64,

    /// 最后更新时间 / Last update time
    #[schema(value_type = String, example = "2025-12-12T10:30:00Z")]
    pub last_updated: DateTime<Utc>,
}

/// SOL 价格服务 / SOL price service
#[derive(Clone)]
pub struct SolPriceService {
    /// 缓存的价格信息 / Cached price information
    price: Arc<RwLock<Option<SolPrice>>>,

    /// RPC 客户端 / RPC client
    client: reqwest::Client,

    /// RPC URL
    rpc_url: String,
}

impl SolPriceService {
    /// 创建新的价格服务 / Create a new price service
    pub fn new(rpc_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("无法创建 HTTP 客户端 / Failed to create HTTP client");

        Self {
            price: Arc::new(RwLock::new(None)),
            client,
            rpc_url,
        }
    }

    /// 从 u8 数组读取 Pubkey (32 bytes) / Read Pubkey from u8 array (32 bytes)
    fn extract_pubkey(data: &[u8], offset: usize) -> anyhow::Result<Pubkey> {
        if offset + 32 > data.len() {
            anyhow::bail!("数据越界 / Data out of bounds");
        }
        let bytes: [u8; 32] = data[offset..offset + 32]
            .try_into()
            .map_err(|_| anyhow::anyhow!("无法转换为 32 字节数组 / Failed to convert to 32-byte array"))?;
        Ok(Pubkey::new_from_array(bytes))
    }

    /// 从 u8 数组读取 u128 (小端序) / Read u128 from u8 array (little-endian)
    fn read_u128_le(data: &[u8], offset: usize) -> anyhow::Result<u128> {
        if offset + 16 > data.len() {
            anyhow::bail!("数据越界 / Data out of bounds");
        }
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&data[offset..offset + 16]);
        Ok(u128::from_le_bytes(bytes))
    }

    /// 从 sqrtPriceX64 计算实际价格 / Calculate price from sqrtPriceX64
    ///
    /// sqrtPriceX64 = sqrt(price) * 2^64
    /// price = (sqrtPriceX64 / 2^64)^2
    /// price = amount_token_1 / amount_token_0
    fn calculate_price_from_sqrt_price_x64(
        sqrt_price_x64: u128,
        decimals0: u8,
        decimals1: u8,
    ) -> f64 {
        // Q64.64 格式 / Q64.64 format
        let q64 = 2u128.pow(64);

        // 转换为浮点数 / Convert to float
        let sqrt_price = sqrt_price_x64 as f64 / q64 as f64;

        // price = sqrtPrice^2
        let mut price = sqrt_price * sqrt_price;

        // 调整小数位差异 / Adjust decimal difference
        let decimal_adjustment = 10f64.powi(decimals0 as i32 - decimals1 as i32);
        price *= decimal_adjustment;

        price
    }

    /// 获取 Token 账户余额 / Get token account balance
    async fn get_token_account_balance(&self, account: &Pubkey) -> anyhow::Result<f64> {
        #[derive(Serialize)]
        struct RpcRequest {
            jsonrpc: String,
            id: u64,
            method: String,
            params: Vec<serde_json::Value>,
        }

        #[derive(Deserialize)]
        struct RpcResponse {
            result: TokenBalanceResult,
        }

        #[derive(Deserialize)]
        struct TokenBalanceResult {
            value: TokenBalanceValue,
        }

        #[derive(Deserialize)]
        struct TokenBalanceValue {
            #[serde(rename = "uiAmount")]
            ui_amount: Option<f64>,
        }

        let request = RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "getTokenAccountBalance".to_string(),
            params: vec![serde_json::Value::String(account.to_string())],
        };

        let response = self
            .client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await?;

        let rpc_response: RpcResponse = response.json().await?;
        rpc_response
            .result
            .value
            .ui_amount
            .ok_or_else(|| anyhow::anyhow!("无法获取余额 / Failed to get balance"))
    }

    /// 获取账户信息 / Get account info
    async fn get_account_info(&self, pubkey: &Pubkey) -> anyhow::Result<Vec<u8>> {
        #[derive(Serialize)]
        struct RpcRequest {
            jsonrpc: String,
            id: u64,
            method: String,
            params: Vec<serde_json::Value>,
        }

        #[derive(Deserialize)]
        struct RpcResponse {
            result: AccountResult,
        }

        #[derive(Deserialize)]
        struct AccountResult {
            value: Option<AccountValue>,
        }

        #[derive(Deserialize)]
        struct AccountValue {
            data: (String, String), // (base64 data, encoding)
        }

        let request = RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "getAccountInfo".to_string(),
            params: vec![
                serde_json::Value::String(pubkey.to_string()),
                serde_json::json!({
                    "encoding": "base64",
                    "commitment": "confirmed"
                }),
            ],
        };

        let response = self
            .client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await?;

        let rpc_response: RpcResponse = response.json().await?;
        let account_value = rpc_response
            .result
            .value
            .ok_or_else(|| anyhow::anyhow!("账户不存在 / Account does not exist"))?;

        let data = general_purpose::STANDARD.decode(&account_value.data.0)?;
        Ok(data)
    }

    /// 从链上获取 SOL 价格 (Raydium CLMM) / Fetch SOL price from chain (Raydium CLMM)
    pub async fn fetch_price_from_chain(&self) -> anyhow::Result<f64> {
        tracing::info!("🔍 开始从 Raydium CLMM 池获取 SOL 价格... / Starting to fetch SOL price from Raydium CLMM pool...");

        // 1. 获取池账户信息 / Get pool account info
        let pool_pubkey = Pubkey::from_str(SOL_USDT_POOL)?;
        tracing::debug!("📖 [1/4] 读取池账户 {} / Reading pool account {}", SOL_USDT_POOL, SOL_USDT_POOL);
        let pool_data = self.get_account_info(&pool_pubkey).await?;
        tracing::debug!("      ✓ 完成,数据长度: {} bytes / Done, data length: {} bytes", pool_data.len(), pool_data.len());

        // 2. 解析池账户结构 / Parse pool account structure
        tracing::debug!("🔬 [2/4] 解析账户结构 / Parsing account structure");

        // 根据 Raydium CLMM 池账户结构解析 / Parse according to Raydium CLMM pool account structure
        // offset 73: mint0 (32 bytes)
        // offset 105: mint1 (32 bytes)
        // offset 233: decimals0 (1 byte)
        // offset 234: decimals1 (1 byte)
        // offset 253: sqrtPriceX64 (16 bytes, u128)
        let mint0 = Self::extract_pubkey(&pool_data, 73)?;
        let mint1 = Self::extract_pubkey(&pool_data, 105)?;
        let decimals0 = pool_data[233];
        let decimals1 = pool_data[234];
        let sqrt_price_x64 = Self::read_u128_le(&pool_data, 253)?;

        tracing::debug!("      Mint 0: {}", mint0);
        tracing::debug!("      Mint 1: {}", mint1);
        tracing::debug!("      Decimals: {}, {}", decimals0, decimals1);
        tracing::debug!("      sqrtPriceX64: {}", sqrt_price_x64);
        tracing::debug!("      ✓ 完成 / Done");

        // 3. 计算价格 / Calculate price
        tracing::debug!("💰 [3/4] 计算价格 / Calculating price");

        let price = Self::calculate_price_from_sqrt_price_x64(sqrt_price_x64, decimals0, decimals1);
        tracing::debug!("      原始价格 (Token1/Token0): {:.6}", price);

        // 确定哪个是 SOL，哪个是 USDT / Determine which is SOL and which is USDT
        let sol_price = if mint0.to_string() == WRAPPED_SOL_MINT && mint1.to_string() == USDT_MINT {
            tracing::debug!("      识别: SOL 是 Token0, USDT 是 Token1 / Identified: SOL is Token0, USDT is Token1");
            price
        } else if mint1.to_string() == WRAPPED_SOL_MINT && mint0.to_string() == USDT_MINT {
            tracing::debug!("      识别: USDT 是 Token0, SOL 是 Token1 / Identified: USDT is Token0, SOL is Token1");
            1.0 / price
        } else {
            anyhow::bail!("无法识别 SOL/USDT 池 / Cannot identify SOL/USDT pool");
        };

        tracing::debug!("      ✓ 完成 / Done");

        // 4. 获取储备量（用于日志输出）/ Get reserves (for logging)
        tracing::debug!("📊 [4/4] 获取储备量 / Getting reserves");

        // offset 137: vault0 (32 bytes)
        // offset 169: vault1 (32 bytes)
        let vault0 = Self::extract_pubkey(&pool_data, 137)?;
        let vault1 = Self::extract_pubkey(&pool_data, 169)?;

        let vault0_balance = self.get_token_account_balance(&vault0).await?;
        let vault1_balance = self.get_token_account_balance(&vault1).await?;

        tracing::debug!("      SOL 储备: {:.2} / SOL reserve: {:.2}", vault0_balance, vault0_balance);
        tracing::debug!("      USDT 储备: {:.2} / USDT reserve: {:.2}", vault1_balance, vault1_balance);
        tracing::debug!("      ✓ 完成 / Done");

        // 输出最终结果 / Output final result
        tracing::info!("✅ SOL 价格: ${:.4} USDT / SOL price: ${:.4} USDT", sol_price, sol_price);
        tracing::info!("📊 流动性: ${:.2} / Liquidity: ${:.2}", sol_price * vault0_balance * 2.0, sol_price * vault0_balance * 2.0);

        Ok(sol_price)
    }

    /// 更新价格 (从链上获取) / Update price (fetch from chain)
    pub async fn update_price(&self) {
        let price = match self.fetch_price_from_chain().await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("❌ 从链上获取价格失败: {} / Failed to fetch price from chain: {}", e, e);
                return;
            }
        };

        // 更新缓存 / Update cache
        let sol_price = SolPrice {
            price,
            last_updated: Utc::now(),
        };

        let mut price_lock = self.price.write().await;
        *price_lock = Some(sol_price);

        tracing::debug!("💾 价格已更新到缓存 / Price updated in cache");
    }

    /// 初始化价格 (启动时同步获取,失败则 panic) / Initialize price (sync fetch on startup, panic on failure)
    pub async fn initialize_price(&self) {
        tracing::info!("🚀 初始化 SOL 价格服务... / Initializing SOL price service...");

        let max_retries = 3;
        let mut retry_count = 0;

        loop {
            match self.fetch_price_from_chain().await {
                Ok(price) => {
                    // 更新缓存 / Update cache
                    let sol_price = SolPrice {
                        price,
                        last_updated: Utc::now(),
                    };

                    let mut price_lock = self.price.write().await;
                    *price_lock = Some(sol_price);

                    tracing::info!("✅ SOL 价格初始化成功: ${:.4} / SOL price initialized successfully: ${:.4}", price, price);
                    return;
                }
                Err(e) => {
                    retry_count += 1;
                    if retry_count >= max_retries {
                        tracing::error!(
                            "❌ SOL 价格初始化失败,已重试 {} 次,程序无法启动 / SOL price initialization failed after {} retries, program cannot start",
                            max_retries, max_retries
                        );
                        panic!("无法获取 SOL 价格,程序终止 / Failed to get SOL price, program terminated: {}", e);
                    }
                    tracing::warn!(
                        "⚠️ 获取价格失败 ({}/{}): {}, 5秒后重试... / Failed to fetch price ({}/{}): {}, retrying in 5s...",
                        retry_count, max_retries, e, retry_count, max_retries, e
                    );
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    /// 获取当前价格 / Get current price
    pub async fn get_price(&self) -> Option<SolPrice> {
        let price_lock = self.price.read().await;
        price_lock.clone()
    }

    /// 同步获取当前价格(仅返回价格值) / Get current price synchronously (returns price value only)
    pub fn get_price_sync(&self) -> f64 {
        // 使用 try_read 避免阻塞 / Use try_read to avoid blocking
        if let Ok(price_lock) = self.price.try_read() {
            if let Some(ref price_info) = *price_lock {
                return price_info.price;
            }
        }
        // 如果无法获取或没有缓存,返回默认值 / Return default if unable to get or no cache
        140.0 // 默认值 / Default value
    }

    /// 启动定时更新任务 / Start periodic update task
    pub fn start_periodic_update(self: Arc<Self>) {
        tokio::spawn(async move {
            // 每 5 分钟更新一次 / Update every 5 minutes
            let mut interval = tokio::time::interval(Duration::from_secs(300));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                interval.tick().await;
                tracing::debug!("🔄 开始定期 SOL 价格更新... / Starting periodic SOL price update...");
                self.update_price().await;
            }
        });

        tracing::info!("✅ SOL 价格定时更新任务已启动 (每5分钟) / SOL price periodic update task started (every 5 minutes)");
    }
}
