// 存储事件处理器 - 将事件存储到RocksDB / Storage event handler - store events to RocksDB
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{info, error};
use crate::db::{EventStorage, OrderBookStorage, OrderData};
use super::events::PinpetEvent;
use super::listener::EventHandler;

/// 存储事件处理器 - 将接收到的事件存储到RocksDB / Storage event handler - stores received events to RocksDB
pub struct StorageEventHandler {
    event_storage: Arc<EventStorage>,
    orderbook_storage: Arc<OrderBookStorage>,
}

impl StorageEventHandler {
    /// 创建新的存储事件处理器 / Create new storage event handler
    pub fn new(event_storage: Arc<EventStorage>, orderbook_storage: Arc<OrderBookStorage>) -> Self {
        Self {
            event_storage,
            orderbook_storage,
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

        // 如果是 LongShortEvent，同时存储到 OrderBook / If LongShortEvent, also store to OrderBook
        if let PinpetEvent::LongShort(ref ls_event) = event {
            if let Err(e) = self.store_long_short_to_orderbook(ls_event).await {
                error!("❌ 存储 LongShortEvent 到 OrderBook 失败 / Failed to store LongShortEvent to OrderBook: {}", e);
                // 继续存储事件，不因 OrderBook 失败而中断 / Continue storing event, don't fail due to OrderBook error
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
    /// 将 LongShortEvent 转换并存储到 OrderBook / Convert and store LongShortEvent to OrderBook
    async fn store_long_short_to_orderbook(
        &self,
        event: &super::events::LongShortEvent,
    ) -> anyhow::Result<()> {
        // 将 LongShortEvent 转换为 OrderData / Convert LongShortEvent to OrderData
        let order = OrderData {
            slot: event.slot,
            order_id: event.order_id,
            user: event.user.clone(),
            lock_lp_start_price: event.lock_lp_start_price,
            lock_lp_end_price: event.lock_lp_end_price,
            open_price: event.open_price,
            lock_lp_sol_amount: event.lock_lp_sol_amount,
            lock_lp_token_amount: event.lock_lp_token_amount,
            margin_init_sol_amount: 0,  // 填0 / Fill with 0
            margin_sol_amount: event.margin_sol_amount,
            borrow_amount: event.borrow_amount,
            position_asset_amount: event.position_asset_amount,
            realized_sol_amount: 0,  // 填0 / Fill with 0
            start_time: event.start_time,
            end_time: event.end_time,
            borrow_fee: event.borrow_fee,
            order_type: event.order_type,
            close_time: None,
            close_type: 0,
        };

        // 存储到 OrderBook / Store to OrderBook
        self.orderbook_storage
            .add_active_order(&event.mint_account, &order)
            .await?;

        info!(
            "✅ LongShortEvent 已存储到 OrderBook / LongShortEvent stored to OrderBook: mint={}, order_id={}",
            event.mint_account, event.order_id
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