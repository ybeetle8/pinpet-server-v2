// 存储事件处理器 - 将事件存储到RocksDB / Storage event handler - store events to RocksDB
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{info, error};
use crate::db::{EventStorage, TokenStorage, OrderBookStorage};
use crate::orderbook::MarginOrder;
use super::events::PinpetEvent;
use super::listener::EventHandler;

/// 存储事件处理器 - 将接收到的事件存储到RocksDB / Storage event handler - stores received events to RocksDB
pub struct StorageEventHandler {
    event_storage: Arc<EventStorage>,
    token_storage: Arc<TokenStorage>,
    orderbook_storage: Arc<OrderBookStorage>,
}

impl StorageEventHandler {
    /// 创建新的存储事件处理器 / Create new storage event handler
    pub fn new(
        event_storage: Arc<EventStorage>,
        token_storage: Arc<TokenStorage>,
        orderbook_storage: Arc<OrderBookStorage>,
    ) -> Self {
        Self {
            event_storage,
            token_storage,
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

        // 如果是 TokenCreatedEvent，同时存储到 TokenStorage / If TokenCreatedEvent, also store to TokenStorage
        if let PinpetEvent::TokenCreated(ref tc_event) = event {
            if let Err(e) = self.store_token_created(tc_event).await {
                error!("❌ 存储 TokenCreatedEvent 到 TokenStorage 失败 / Failed to store TokenCreatedEvent to TokenStorage: {}", e);
                // 继续存储事件，不因 TokenStorage 失败而中断 / Continue storing event, don't fail due to TokenStorage error
            }
        }

        // 如果是 LongShortEvent，插入到 OrderBook / If LongShortEvent, insert to OrderBook
        if let PinpetEvent::LongShort(ref ls_event) = event {
            if let Err(e) = self.handle_long_short_event(ls_event) {
                error!("❌ 处理 LongShortEvent 失败 / Failed to handle LongShortEvent: {}", e);
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

    /// 处理 LongShortEvent 并插入到 OrderBook / Handle LongShortEvent and insert to OrderBook
    fn handle_long_short_event(
        &self,
        event: &super::events::LongShortEvent,
    ) -> anyhow::Result<()> {
        // 1. 确定方向 / Determine direction
        // order_type: 1=做多/long/dn, 2=做空/short/up
        let direction = match event.order_type {
            1 => "dn",  // 做多 / Long
            2 => "up",  // 做空 / Short
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid order_type: {}, expected 1 (long/dn) or 2 (short/up)",
                    event.order_type
                ));
            }
        };

        info!(
            "📊 处理 LongShortEvent / Processing LongShortEvent: mint={}, direction={}, order_id={}, payer={}",
            &event.mint_account[..8], direction, event.order_id, &event.payer[..8]
        );

        // 2. 获取或创建 OrderBook 管理器 / Get or create OrderBook manager
        let manager = self.orderbook_storage
            .get_or_create_manager(event.mint_account.clone(), direction.to_string())?;

        // 3. 构造 MarginOrder / Construct MarginOrder
        let order = MarginOrder {
            user: event.payer.clone(),
            lock_lp_start_price: event.lock_lp_start_price,
            lock_lp_end_price: event.lock_lp_end_price,
            open_price: event.open_price,
            order_id: 0,  // 将由 manager 分配 / Will be assigned by manager
            lock_lp_sol_amount: event.lock_lp_sol_amount,
            lock_lp_token_amount: event.lock_lp_token_amount,
            next_lp_sol_amount: 0,  // 初始值 / Initial value
            next_lp_token_amount: 0,  // 初始值 / Initial value
            margin_init_sol_amount: event.margin_sol_amount,  // ⭐ 初始保证金 / Initial margin
            margin_sol_amount: event.margin_sol_amount,       // ⭐ 当前保证金 / Current margin
            borrow_amount: event.borrow_amount,
            position_asset_amount: event.position_asset_amount,
            realized_sol_amount: 0,  // 初始值 / Initial value
            version: 0,  // 将由 manager 设置 / Will be set by manager
            start_time: event.start_time,
            end_time: event.end_time,
            next_order: u16::MAX,  // 将由 manager 设置 / Will be set by manager
            prev_order: u16::MAX,  // 将由 manager 设置 / Will be set by manager
            borrow_fee: event.borrow_fee,
            order_type: event.order_type,
        };

        // 4. 确定插入位置 / Determine insert position
        // 根据 order_index 确定插入位置 / Determine insert position based on order_index
        // 如果 order_index 是 0 且链表为空,则插入头部 / If order_index is 0 and list is empty, insert at head
        // 否则,根据 order_index 插入 / Otherwise, insert based on order_index
        let header = manager.load_header()?;
        let insert_pos = if header.total == 0 {
            // 空链表,插入头部 / Empty list, insert at head
            u16::MAX
        } else {
            // 根据 order_index 确定插入位置 / Determine insert position based on order_index
            // 注意: order_index 是在链表中的索引,直接使用 / Note: order_index is the index in the list, use directly
            if event.order_index == 0 {
                // 插入到头部之前 / Insert before head
                u16::MAX
            } else if event.order_index >= header.total {
                // 插入到尾部 / Insert at tail
                header.tail
            } else {
                // 插入到指定位置之前 / Insert before specified position
                // 我们需要找到 order_index - 1 的位置 / We need to find the position at order_index - 1
                event.order_index.saturating_sub(1)
            }
        };

        info!(
            "📍 插入位置 / Insert position: insert_pos={}, header.total={}, order_index={}",
            if insert_pos == u16::MAX { "HEAD".to_string() } else { insert_pos.to_string() },
            header.total,
            event.order_index
        );

        // 5. 插入订单 / Insert order
        let (index, assigned_order_id) = if insert_pos == u16::MAX || header.total == 0 {
            // 插入到头部或空链表 / Insert at head or empty list
            // 使用 insert_after(u16::MAX, ...) 会在头部插入 / Using insert_after(u16::MAX, ...) inserts at head
            manager.insert_after(u16::MAX, &order)?
        } else {
            // 插入到指定位置之后 / Insert after specified position
            manager.insert_after(insert_pos, &order)?
        };

        info!(
            "✅ 订单已插入 OrderBook / Order inserted to OrderBook: mint={}, direction={}, index={}, assigned_order_id={}, event_order_id={}",
            &event.mint_account[..8], direction, index, assigned_order_id, event.order_id
        );

        // 验证: 检查分配的 order_id 是否与事件中的 order_id 一致 / Verify: Check if assigned order_id matches event order_id
        if assigned_order_id != event.order_id {
            error!(
                "⚠️ 警告: 分配的 order_id 与事件中的不一致 / Warning: Assigned order_id mismatch: assigned={}, event={}",
                assigned_order_id, event.order_id
            );
        }

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