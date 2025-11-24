// 存储事件处理器 - 将事件存储到RocksDB / Storage event handler - store events to RocksDB
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{info, error};
use crate::db::{EventStorage, TokenStorage};
use super::events::PinpetEvent;
use super::listener::EventHandler;

/// 存储事件处理器 - 将接收到的事件存储到RocksDB / Storage event handler - stores received events to RocksDB
pub struct StorageEventHandler {
    event_storage: Arc<EventStorage>,
    token_storage: Arc<TokenStorage>,
}

impl StorageEventHandler {
    /// 创建新的存储事件处理器 / Create new storage event handler
    pub fn new(
        event_storage: Arc<EventStorage>,
        token_storage: Arc<TokenStorage>,
    ) -> Self {
        Self {
            event_storage,
            token_storage,
        }
    }
}

#[async_trait]
impl EventHandler for StorageEventHandler {
    async fn handle_event(&self, event: PinpetEvent) -> anyhow::Result<()> {
        // 提取签名和事件基本信息 / Extract signature and basic event info
        let signature = match &event {
            PinpetEvent::TokenCreated(e) => e.signature.clone(),
            PinpetEvent::BuySell(e) => e.signature.clone(),
            PinpetEvent::LongShort(e) => e.signature.clone(),
            PinpetEvent::FullClose(e) => e.signature.clone(),
            PinpetEvent::PartialClose(e) => e.signature.clone(),
            PinpetEvent::MilestoneDiscount(e) => e.signature.clone(),
        };

        // 获取事件类型 / Get event type
        let event_type = match &event {
            PinpetEvent::TokenCreated(_) => "TokenCreated",
            PinpetEvent::BuySell(_) => "BuySell",
            PinpetEvent::LongShort(_) => "LongShort",
            PinpetEvent::FullClose(_) => "FullClose",
            PinpetEvent::PartialClose(_) => "PartialClose",
            PinpetEvent::MilestoneDiscount(_) => "MilestoneDiscount",
        };

        info!("📝 存储事件 / Storing event: 类型/type={}, 签名/signature={}",
              event_type, &signature[..8]);

        // 如果是 TokenCreatedEvent，同时存储到 TokenStorage / If TokenCreatedEvent, also store to TokenStorage
        if let PinpetEvent::TokenCreated(ref tc_event) = event {
            if let Err(e) = self.store_token_created(tc_event).await {
                error!("❌ 存储 TokenCreatedEvent 到 TokenStorage 失败 / Failed to store TokenCreatedEvent to TokenStorage: {}", e);
                // 继续存储事件，不因 TokenStorage 失败而中断 / Continue storing event, don't fail due to TokenStorage error
            }
        }

        // 目前我们一次只处理一个事件，但store_events支持批量存储
        // Currently we process one event at a time, but store_events supports batch storage
        let events = vec![event];

        // 存储事件到数据库 / Store event to database
        match self.event_storage.store_events(&signature, events).await {
            Ok(_) => {
                info!("✅ 事件存储成功 / Event stored successfully: {}", &signature[..8]);
                Ok(())
            }
            Err(e) => {
                error!("❌ 事件存储失败 / Failed to store event: {}", e);
                Err(e)
            }
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl StorageEventHandler {
    /// 将 TokenCreatedEvent 存储到 TokenStorage / Store TokenCreatedEvent to TokenStorage
    async fn store_token_created(
        &self,
        event: &super::events::TokenCreatedEvent,
    ) -> anyhow::Result<()> {
        info!(
            "🪙 处理TokenCreated事件 / Processing TokenCreated event: mint={}, symbol={}",
            event.mint_account, event.symbol
        );

        // 异步保存token（包括IPFS元数据获取）/ Save token asynchronously (including IPFS metadata fetch)
        self.token_storage.save_token_from_event(event).await?;

        info!(
            "✅ TokenCreatedEvent 已存储到 TokenStorage / TokenCreatedEvent stored to TokenStorage: mint={}",
            event.mint_account
        );

        Ok(())
    }
}

/// 处理包含多个事件的交易 / Process transactions containing multiple events
pub async fn process_transaction_events(
    event_storage: &EventStorage,
    signature: &str,
    events: Vec<PinpetEvent>,
) -> anyhow::Result<()> {
    if events.is_empty() {
        return Ok(());
    }

    info!("📦 批量存储{}个事件，签名: {} / Batch storing {} events for signature: {}",
          events.len(), &signature[..8], events.len(), &signature[..8]);

    // 存储所有事件 / Store all events
    event_storage.store_events(signature, events).await?;

    Ok(())
}

/// 处理包含强平的BuySell事件 / Process BuySell events with force liquidations
pub async fn process_buy_sell_with_liquidations(
    event_storage: &EventStorage,
    buy_sell_event: PinpetEvent,
    force_liquidate_events: Vec<PinpetEvent>,
) -> anyhow::Result<()> {
    // 获取签名 / Get signature
    let signature = if let PinpetEvent::BuySell(ref e) = buy_sell_event {
        e.signature.clone()
    } else {
        return Err(anyhow::anyhow!("Expected BuySell event"));
    };

    // 合并所有事件 / Merge all events
    let mut all_events = vec![buy_sell_event];
    all_events.extend(force_liquidate_events);

    info!("🔄 处理BuySell事件及{}个强平事件，签名: {} / Processing BuySell event with {} force liquidations, signature: {}",
          all_events.len() - 1, &signature[..8], all_events.len() - 1, &signature[..8]);

    // 批量存储 / Batch store
    event_storage.store_events(&signature, all_events).await?;

    Ok(())
}