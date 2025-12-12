use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::Duration;

/// Binance API 响应结构 / Binance API response structure
#[derive(Debug, Deserialize)]
struct BinancePrice {
    price: String,
}
/// CoinGecko API 响应结构 / CoinGecko API response structure
#[derive(Debug, Deserialize)]
struct CoinGeckoResponse {
    solana: CoinGeckoPrice,
}

#[derive(Debug, Deserialize)]
struct CoinGeckoPrice {
    usd: f64,
}

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

    /// HTTP 客户端 / HTTP client
    client: reqwest::Client,
}

impl SolPriceService {
    /// 创建新的价格服务 / Create a new price service
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .expect("无法创建 HTTP 客户端 / Failed to create HTTP client");

        Self {
            price: Arc::new(RwLock::new(None)),
            client,
        }
    }

    /// 从 Binance 获取价格 / Fetch price from Binance
    async fn fetch_from_binance(&self) -> anyhow::Result<f64> {
        let url = "https://api.binance.com/api/v3/ticker/price?symbol=SOLUSDT";

        let response = self.client.get(url).send().await?;
        let binance_price: BinancePrice = response.json().await?;
        let price = binance_price.price.parse::<f64>()?;

        tracing::info!("✅ 从 Binance 获取 SOL 价格: ${} / Fetched SOL price from Binance: ${}", price, price);
        Ok(price)
    }

    /// 从 CoinGecko 获取价格 / Fetch price from CoinGecko
    async fn fetch_from_coingecko(&self) -> anyhow::Result<f64> {
        let url = "https://api.coingecko.com/api/v3/simple/price?ids=solana&vs_currencies=usd";

        let response = self.client.get(url).send().await?;
        let coingecko_response: CoinGeckoResponse = response.json().await?;
        let price = coingecko_response.solana.usd;

        tracing::info!("✅ 从 CoinGecko 获取 SOL 价格: ${} / Fetched SOL price from CoinGecko: ${}", price, price);
        Ok(price)
    }

    /// 更新价格 (Binance 优先,失败则使用 CoinGecko) / Update price (Binance first, fallback to CoinGecko)
    pub async fn update_price(&self) {
        // 尝试从 Binance 获取 / Try to fetch from Binance
        let price = match self.fetch_from_binance().await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("⚠️ Binance 获取失败: {}, 尝试 CoinGecko... / Failed to fetch from Binance: {}, trying CoinGecko...", e, e);

                // 回退到 CoinGecko / Fallback to CoinGecko
                match self.fetch_from_coingecko().await {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!("❌ 两个 API 都失败了,保留旧价格 / Both APIs failed, keeping old price: {}", e);
                        return;
                    }
                }
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

    /// 获取当前价格 / Get current price
    pub async fn get_price(&self) -> Option<SolPrice> {
        let price_lock = self.price.read().await;
        price_lock.clone()
    }

    /// 启动定时更新任务 / Start periodic update task
    pub fn start_periodic_update(self: Arc<Self>) {
        tokio::spawn(async move {
            // 立即执行第一次查询 / Execute first query immediately
            tracing::info!("🚀 开始首次 SOL 价格查询... / Starting first SOL price query...");
            self.update_price().await;

            // 每 3 分钟更新一次 / Update every 3 minutes
            let mut interval = tokio::time::interval(Duration::from_secs(180));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                interval.tick().await;
                tracing::debug!("🔄 开始定期 SOL 价格更新... / Starting periodic SOL price update...");
                self.update_price().await;
            }
        });

        tracing::info!("✅ SOL 价格定时更新任务已启动 (每3分钟) / SOL price periodic update task started (every 3 minutes)");
    }
}
