// 存储事件处理器 - 将事件存储到RocksDB / Storage event handler - store events to RocksDB
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{info, error, warn};
use crate::db::{EventStorage, TokenStorage, OrderBookStorage};
use crate::orderbook::MarginOrder;
use super::events::PinpetEvent;
use super::listener::EventHandler;

/// 存储事件处理器 - 将接收到的事件存储到RocksDB / Storage event handler - stores received events to RocksDB
#[derive(Clone)]
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
            PinpetEvent::Liquidate(e) => e.signature.clone(),
        };

        // 获取事件类型 / Get event type
        let event_type = match &event {
            PinpetEvent::TokenCreated(_) => "TokenCreated",
            PinpetEvent::BuySell(_) => "BuySell",
            PinpetEvent::LongShort(_) => "LongShort",
            PinpetEvent::FullClose(_) => "FullClose",
            PinpetEvent::PartialClose(_) => "PartialClose",
            PinpetEvent::MilestoneDiscount(_) => "MilestoneDiscount",
            PinpetEvent::Liquidate(_) => "Liquidate",
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

        // ⚠️  重要: 先处理订单操作,再更新价格
        // ⚠️  Important: Process order operations BEFORE updating price
        // 这样可以确保在删除订单时获取的是上一次的价格,而不是当前事件的价格
        // This ensures we get the previous price when deleting orders, not the current event's price

        // 🔧 P0 修复: 使用 spawn_blocking 包装所有同步 OrderBook 操作
        // 🔧 P0 Fix: Use spawn_blocking to wrap all synchronous OrderBook operations
        // 🔧 返回生成的 LiquidateEvent 列表 / Return generated LiquidateEvent list
        let this = self.clone();
        let event_for_blocking = event.clone();
        let liquidate_events = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<PinpetEvent>> {
            let mut additional_events = Vec::new();

            // 如果是 LongShortEvent，插入到 OrderBook / If LongShortEvent, insert to OrderBook
            if let PinpetEvent::LongShort(ref ls_event) = event_for_blocking {
                match this.handle_long_short_event(ls_event) {
                    Ok(events) => additional_events.extend(events),
                    Err(e) => {
                        error!("❌ 处理 LongShortEvent 失败 / Failed to handle LongShortEvent: {}", e);
                        // 继续存储事件，不因 OrderBook 失败而中断 / Continue storing event, don't fail due to OrderBook error
                    }
                }
            }

            // 如果是 BuySellEvent，处理清算 / If BuySellEvent, handle liquidations
            if let PinpetEvent::BuySell(ref bs_event) = event_for_blocking {
                match this.handle_buy_sell_event(bs_event) {
                    Ok(events) => additional_events.extend(events),
                    Err(e) => {
                        error!("❌ 处理 BuySellEvent 清算失败 / Failed to handle BuySellEvent liquidations: {}", e);
                        // 继续存储事件，不因 OrderBook 失败而中断 / Continue storing event, don't fail due to OrderBook error
                    }
                }
            }

            // 如果是 FullCloseEvent，处理清算 / If FullCloseEvent, handle liquidations
            if let PinpetEvent::FullClose(ref fc_event) = event_for_blocking {
                match this.handle_full_close_event(fc_event) {
                    Ok(events) => additional_events.extend(events),
                    Err(e) => {
                        error!("❌ 处理 FullCloseEvent 清算失败 / Failed to handle FullCloseEvent liquidations: {}", e);
                        // 继续存储事件，不因 OrderBook 失败而中断 / Continue storing event, don't fail due to OrderBook error
                    }
                }
            }

            // 如果是 PartialCloseEvent，处理更新和清算 / If PartialCloseEvent, handle update and liquidations
            if let PinpetEvent::PartialClose(ref pc_event) = event_for_blocking {
                match this.handle_partial_close_event(pc_event) {
                    Ok(events) => additional_events.extend(events),
                    Err(e) => {
                        error!("❌ 处理 PartialCloseEvent 更新和清算失败 / Failed to handle PartialCloseEvent update and liquidations: {}", e);
                        // 继续存储事件，不因 OrderBook 失败而中断 / Continue storing event, don't fail due to OrderBook error
                    }
                }
            }

            Ok(additional_events)
        }).await??;

        // 更新Token的latest_price（所有带latest_price的事件）/ Update token's latest_price (all events with latest_price)
        // 🔧 P0 修复: 使用 spawn_blocking 包装 TokenStorage 的同步写操作
        // 🔧 P0 Fix: Use spawn_blocking to wrap synchronous TokenStorage write operations
        let token_storage = self.token_storage.clone();
        let event_for_token = event.clone();
        tokio::task::spawn_blocking(move || {
            match &event_for_token {
                PinpetEvent::TokenCreated(_e) => {
                    // TokenCreated已经在store_token_created中设置了初始价格 / Initial price already set in store_token_created
                }
                PinpetEvent::BuySell(e) => {
                    if let Err(err) = token_storage.update_token_price(&e.mint_account, e.latest_price) {
                        error!("❌ 更新Token价格失败 (BuySell) / Failed to update token price (BuySell): {}", err);
                    }
                }
                PinpetEvent::LongShort(e) => {
                    if let Err(err) = token_storage.update_token_price(&e.mint_account, e.latest_price) {
                        error!("❌ 更新Token价格失败 (LongShort) / Failed to update token price (LongShort): {}", err);
                    }
                }
                PinpetEvent::FullClose(e) => {
                    if let Err(err) = token_storage.update_token_price(&e.mint_account, e.latest_price) {
                        error!("❌ 更新Token价格失败 (FullClose) / Failed to update token price (FullClose): {}", err);
                    }
                }
                PinpetEvent::PartialClose(e) => {
                    if let Err(err) = token_storage.update_token_price(&e.mint_account, e.latest_price) {
                        error!("❌ 更新Token价格失败 (PartialClose) / Failed to update token price (PartialClose): {}", err);
                    }
                }
                PinpetEvent::MilestoneDiscount(e) => {
                    // MilestoneDiscount 更新费率字段 / Update fee fields
                    if let Err(err) = token_storage.update_token_fees(
                        &e.mint_account,
                        e.swap_fee,
                        e.borrow_fee,
                        e.fee_discount_flag,
                    ) {
                        error!("❌ 更新Token费率失败 (MilestoneDiscount) / Failed to update token fees (MilestoneDiscount): {}", err);
                    }
                }
                PinpetEvent::Liquidate(_e) => {
                    // LiquidateEvent 不包含 latest_price,无需更新 / LiquidateEvent doesn't contain latest_price, no update needed
                }
            }
        }).await?;

        // 目前我们一次只处理一个事件，但store_events支持批量存储
        // Currently we process one event at a time, but store_events supports batch storage
        let events = vec![event];

        // 存储主事件到数据库 / Store main event to database
        match self.event_storage.store_events(&signature, events).await {
            Ok(_) => {
                info!("✅ 事件存储成功 / Event stored successfully: {}", &signature[..8]);
            }
            Err(e) => {
                error!("❌ 事件存储失败 / Failed to store event: {}", e);
                return Err(e);
            }
        }

        // 存储额外生成的 LiquidateEvent / Store additional generated LiquidateEvents
        if !liquidate_events.is_empty() {
            info!("📦 存储{}个额外的清算事件 / Storing {} additional liquidate events",
                  liquidate_events.len(), liquidate_events.len());
            for liquidate_event in liquidate_events {
                // 先提取signature,避免借用检查问题 / Extract signature first to avoid borrow checker issues
                let sig = match &liquidate_event {
                    PinpetEvent::Liquidate(e) => e.signature.clone(),
                    _ => continue, // 不应该发生 / Should not happen
                };
                if let Err(err) = self.event_storage.store_events(&sig, vec![liquidate_event]).await {
                    error!("❌ 存储 LiquidateEvent 失败 / Failed to store LiquidateEvent: {}", err);
                    // 不中断主流程，记录错误继续 / Don't interrupt main flow, log error and continue
                }
            }
        }

        Ok(())
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
    ) -> anyhow::Result<Vec<PinpetEvent>> {
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

        // 处理清算 / Handle liquidations
        let mut liquidate_events = Vec::new();
        if !event.liquidate_indices.is_empty() {
            info!(
                "🔥 处理 LongShortEvent 清算 / Processing LongShortEvent liquidations: count={}",
                event.liquidate_indices.len()
            );

            // LongShortEvent 的清算方向 / LongShortEvent liquidation direction
            // order_type=1 (做多/long) 删 up 方向的订单 / order_type=1 (long) deletes up direction orders
            // order_type=2 (做空/short) 删 dn 方向的订单 / order_type=2 (short) deletes dn direction orders
            let liquidate_direction = match event.order_type {
                1 => "up",  // 做多时清算做空订单 / When going long, liquidate short orders
                2 => "dn",  // 做空时清算做多订单 / When going short, liquidate long orders
                _ => {
                    return Err(anyhow::anyhow!(
                        "Invalid order_type for liquidation: {}, expected 1 or 2",
                        event.order_type
                    ));
                }
            };

            // ✅ 先获取平仓前的价格(上一次记录的价格)
            // ✅ First get the previous price (last recorded price before this event)
            let previous_price = self.get_previous_price(&event.mint_account)?;

            let liquidate_manager = self.orderbook_storage
                .get_or_create_manager(event.mint_account.clone(), liquidate_direction.to_string())?;

            // 强制清算,使用 CloseReason::ForcedLiquidation (2)
            // Forced liquidation, use CloseReason::ForcedLiquidation (2)
            let removed_orders = liquidate_manager.batch_remove_by_indices_unsafe_with_info(
                &event.liquidate_indices,
                2, // ForcedLiquidation
                previous_price,
            )?;

            // 为每个被删除的订单创建 LiquidateEvent / Create LiquidateEvent for each removed order
            for removed_order in removed_orders {
                let liquidate_event = PinpetEvent::Liquidate(super::events::LiquidateEvent {
                    payer: event.payer.clone(),
                    user_sol_account: removed_order.user,
                    mint_account: event.mint_account.clone(),
                    is_close_long: liquidate_direction == "dn",
                    final_token_amount: removed_order.position_asset_amount,
                    final_sol_amount: removed_order.margin_sol_amount,
                    order_index: removed_order.index,
                    timestamp: event.timestamp,
                    signature: event.signature.clone(),
                    slot: event.slot,
                });
                liquidate_events.push(liquidate_event);
            }

            info!(
                "✅ LongShortEvent 清算完成 / LongShortEvent liquidations completed: direction={}, count={}, generated {} LiquidateEvents",
                liquidate_direction, event.liquidate_indices.len(), liquidate_events.len()
            );
        }

        Ok(liquidate_events)
    }

    /// 处理 BuySellEvent 的清算 / Handle BuySellEvent liquidations
    /// 返回生成的 LiquidateEvent 列表 / Returns generated LiquidateEvent list
    fn handle_buy_sell_event(
        &self,
        event: &super::events::BuySellEvent,
    ) -> anyhow::Result<Vec<PinpetEvent>> {
        // 检查是否有需要清算的订单 / Check if there are orders to liquidate
        if event.liquidate_indices.is_empty() {
            return Ok(Vec::new());
        }

        // 确定清算的方向 / Determine liquidation direction
        // is_buy=true 删 up 方向的订单 / is_buy=true deletes up direction orders
        // is_buy=false 删 dn 方向的订单 / is_buy=false deletes dn direction orders
        let direction = if event.is_buy { "up" } else { "dn" };

        info!(
            "🔥 处理 BuySellEvent 清算 / Processing BuySellEvent liquidations: mint={}, direction={}, count={}",
            &event.mint_account[..8], direction, event.liquidate_indices.len()
        );

        // ✅ 先获取平仓前的价格(上一次记录的价格)
        // ✅ First get the previous price (last recorded price before this event)
        let previous_price = self.get_previous_price(&event.mint_account)?;

        // 获取 OrderBook 管理器 / Get OrderBook manager
        let manager = self.orderbook_storage
            .get_or_create_manager(event.mint_account.clone(), direction.to_string())?;

        // 批量删除订单并获取被删除订单信息 / Batch remove orders and get removed order info
        // 强制清算,使用 CloseReason::ForcedLiquidation (2)
        // Forced liquidation, use CloseReason::ForcedLiquidation (2)
        let removed_orders = manager.batch_remove_by_indices_unsafe_with_info(
            &event.liquidate_indices,
            2, // ForcedLiquidation
            previous_price,
        )?;

        // 为每个被删除的订单创建 LiquidateEvent / Create LiquidateEvent for each removed order
        let mut liquidate_events = Vec::new();
        for removed_order in removed_orders {
            let liquidate_event = PinpetEvent::Liquidate(super::events::LiquidateEvent {
                payer: event.payer.clone(),
                user_sol_account: removed_order.user,
                mint_account: event.mint_account.clone(),
                is_close_long: direction == "dn",
                final_token_amount: removed_order.position_asset_amount,
                final_sol_amount: removed_order.margin_sol_amount,
                order_index: removed_order.index,
                timestamp: event.timestamp,
                signature: event.signature.clone(),
                slot: event.slot,
            });
            liquidate_events.push(liquidate_event);
        }

        info!(
            "✅ BuySellEvent 清算完成 / BuySellEvent liquidations completed: mint={}, direction={}, count={}, generated {} LiquidateEvents",
            &event.mint_account[..8], direction, event.liquidate_indices.len(), liquidate_events.len()
        );

        Ok(liquidate_events)
    }

    /// 处理 FullCloseEvent 的清算 / Handle FullCloseEvent liquidations
    /// 返回生成的 LiquidateEvent 列表 / Returns generated LiquidateEvent list
    fn handle_full_close_event(
        &self,
        event: &super::events::FullCloseEvent,
    ) -> anyhow::Result<Vec<PinpetEvent>> {
        // 检查是否有需要清算的订单 / Check if there are orders to liquidate
        if event.liquidate_indices.is_empty() {
            return Ok(Vec::new());
        }

        // 确定清算的方向 / Determine liquidation direction
        // is_close_long=true 删 dn 方向的订单 / is_close_long=true deletes dn direction orders
        // is_close_long=false 删 up 方向的订单 / is_close_long=false deletes up direction orders
        let direction = if event.is_close_long { "dn" } else { "up" };

        info!(
            "🔥 处理 FullCloseEvent 清算 / Processing FullCloseEvent liquidations: mint={}, direction={}, count={}",
            &event.mint_account[..8], direction, event.liquidate_indices.len()
        );

        // ✅ 先获取平仓前的价格(上一次记录的价格)
        // ✅ First get the previous price (last recorded price before this event)
        let previous_price = self.get_previous_price(&event.mint_account)?;

        // 获取 OrderBook 管理器 / Get OrderBook manager
        let manager = self.orderbook_storage
            .get_or_create_manager(event.mint_account.clone(), direction.to_string())?;

        // 批量删除订单并获取被删除订单信息 / Batch remove orders and get removed order info
        // 用户主动平仓,使用 CloseReason::UserInitiated (1)
        // User initiated close, use CloseReason::UserInitiated (1)
        let removed_orders = manager.batch_remove_by_indices_unsafe_with_info(
            &event.liquidate_indices,
            1, // UserInitiated
            previous_price,
        )?;

        // 为每个被删除的订单创建 LiquidateEvent / Create LiquidateEvent for each removed order
        let mut liquidate_events = Vec::new();
        for removed_order in removed_orders {
            let liquidate_event = PinpetEvent::Liquidate(super::events::LiquidateEvent {
                payer: event.payer.clone(),
                user_sol_account: removed_order.user,
                mint_account: event.mint_account.clone(),
                is_close_long: direction == "dn",
                final_token_amount: removed_order.position_asset_amount,
                final_sol_amount: removed_order.margin_sol_amount,
                order_index: removed_order.index,
                timestamp: event.timestamp,
                signature: event.signature.clone(),
                slot: event.slot,
            });
            liquidate_events.push(liquidate_event);
        }

        info!(
            "✅ FullCloseEvent 清算完成 / FullCloseEvent liquidations completed: mint={}, direction={}, count={}, generated {} LiquidateEvents",
            &event.mint_account[..8], direction, event.liquidate_indices.len(), liquidate_events.len()
        );

        Ok(liquidate_events)
    }

    /// 处理 PartialCloseEvent 的更新和清算 / Handle PartialCloseEvent update and liquidations
    /// 返回生成的 LiquidateEvent 列表 / Returns generated LiquidateEvent list
    fn handle_partial_close_event(
        &self,
        event: &super::events::PartialCloseEvent,
    ) -> anyhow::Result<Vec<PinpetEvent>> {
        // 确定更新和清算的方向 / Determine update and liquidation direction
        // is_close_long=true 更新 dn 方向的订单 / is_close_long=true updates dn direction orders
        // is_close_long=false 更新 up 方向的订单 / is_close_long=false updates up direction orders
        let direction = if event.is_close_long { "dn" } else { "up" };

        info!(
            "🔄 处理 PartialCloseEvent / Processing PartialCloseEvent: mint={}, direction={}, order_id={}, order_index={}",
            &event.mint_account[..8], direction, event.order_id, event.order_index
        );

        // 获取 OrderBook 管理器 / Get OrderBook manager
        let manager = self.orderbook_storage
            .get_or_create_manager(event.mint_account.clone(), direction.to_string())?;

        // 1. 先更新订单 / First update the order
        use crate::orderbook::MarginOrderUpdateData;
        let update_data = MarginOrderUpdateData {
            lock_lp_start_price: Some(event.lock_lp_start_price),
            lock_lp_end_price: Some(event.lock_lp_end_price),
            lock_lp_sol_amount: Some(event.lock_lp_sol_amount),
            lock_lp_token_amount: Some(event.lock_lp_token_amount),
            next_lp_sol_amount: None,  // 不更新 / Don't update
            next_lp_token_amount: None,  // 不更新 / Don't update
            end_time: Some(event.end_time),
            margin_init_sol_amount: None,  // 不更新 / Don't update
            margin_sol_amount: Some(event.margin_sol_amount),
            borrow_amount: Some(event.borrow_amount),
            position_asset_amount: Some(event.position_asset_amount),
            borrow_fee: Some(event.borrow_fee),
            open_price: None,  // 不更新 / Don't update
            realized_sol_amount: Some(event.realized_sol_amount),
        };

        manager.update_order(event.order_index, event.order_id, &update_data)?;

        info!(
            "✅ PartialCloseEvent 订单更新完成 / PartialCloseEvent order update completed: order_id={}, order_index={}",
            event.order_id, event.order_index
        );

        // 2. 再删除清算的订单 / Then delete liquidated orders
        let mut liquidate_events = Vec::new();
        if !event.liquidate_indices.is_empty() {
            info!(
                "🔥 处理 PartialCloseEvent 清算 / Processing PartialCloseEvent liquidations: count={}",
                event.liquidate_indices.len()
            );

            // ✅ 先获取平仓前的价格(上一次记录的价格)
            // ✅ First get the previous price (last recorded price before this event)
            let previous_price = self.get_previous_price(&event.mint_account)?;

            // 强制清算,使用 CloseReason::ForcedLiquidation (2)
            // Forced liquidation, use CloseReason::ForcedLiquidation (2)
            let removed_orders = manager.batch_remove_by_indices_unsafe_with_info(
                &event.liquidate_indices,
                2, // ForcedLiquidation
                previous_price,
            )?;

            // 为每个被删除的订单创建 LiquidateEvent / Create LiquidateEvent for each removed order
            for removed_order in removed_orders {
                let liquidate_event = PinpetEvent::Liquidate(super::events::LiquidateEvent {
                    payer: event.payer.clone(),
                    user_sol_account: removed_order.user,
                    mint_account: event.mint_account.clone(),
                    is_close_long: direction == "dn",
                    final_token_amount: removed_order.position_asset_amount,
                    final_sol_amount: removed_order.margin_sol_amount,
                    order_index: removed_order.index,
                    timestamp: event.timestamp,
                    signature: event.signature.clone(),
                    slot: event.slot,
                });
                liquidate_events.push(liquidate_event);
            }

            info!(
                "✅ PartialCloseEvent 清算完成 / PartialCloseEvent liquidations completed: count={}, generated {} LiquidateEvents",
                event.liquidate_indices.len(), liquidate_events.len()
            );
        }

        Ok(liquidate_events)
    }

    // ==================== 辅助方法 / Helper Methods ====================

    /// 获取平仓前的价格(从 TokenStorage 获取上一次记录的价格)
    /// Get previous price before close (last recorded price from TokenStorage)
    ///
    /// # 参数 / Parameters
    /// * `mint` - Token mint 地址 / Token mint address
    ///
    /// # 返回值 / Returns
    /// 返回上一次记录的价格,如果不存在则返回 0
    /// Returns last recorded price, or 0 if not found
    fn get_previous_price(&self, mint: &str) -> anyhow::Result<u128> {
        match self.token_storage.get_token_by_mint(mint) {
            Ok(Some(token)) => {
                // 将 String 类型的 latest_price 转换为 u128
                // Convert String latest_price to u128
                token.latest_price.parse::<u128>()
                    .map_err(|e| anyhow::anyhow!("Failed to parse latest_price: {}", e))
            }
            Ok(None) => {
                warn!("⚠️  Token not found in storage: {}, using price 0", &mint[..8.min(mint.len())]);
                Ok(0)
            }
            Err(e) => {
                error!("❌ Failed to get token from storage: {}", e);
                Err(anyhow::anyhow!("Failed to get token: {}", e))
            }
        }
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