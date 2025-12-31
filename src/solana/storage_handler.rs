// 存储事件处理器 - 将事件存储到RocksDB / Storage event handler - store events to RocksDB
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use crate::db::{EventStorage, TokenStorage, OrderBookStorage};
use crate::orderbook::MarginOrder;
use crate::volume::VolumeStorage;
use crate::change::ChangeStorage;
use crate::markets::MarketsStorage;
use crate::markets_abs::MarketsAbsStorage;
use super::events::PinpetEvent;
use super::listener::EventHandler;
use rust_decimal::Decimal;
use rust_decimal::prelude::*;

/// 存储事件处理器 - 将接收到的事件存储到RocksDB / Storage event handler - stores received events to RocksDB
#[derive(Clone)]
pub struct StorageEventHandler {
    event_storage: Arc<EventStorage>,
    token_storage: Arc<TokenStorage>,
    orderbook_storage: Arc<OrderBookStorage>,
    volume_storage: Arc<VolumeStorage>,
    change_storage: Arc<ChangeStorage>,
    markets_storage: Arc<MarketsStorage>,
    markets_abs_storage: Arc<MarketsAbsStorage>,
    sol_price_service: Arc<crate::price::SolPriceService>,
    kline_socket_service: Option<Arc<crate::kline::KlineSocketService>>,
    sync_monitor: Option<Arc<crate::orderbook_sync::OrderBookSyncMonitor>>,
}

impl StorageEventHandler {
    /// 创建新的存储事件处理器 / Create new storage event handler
    pub fn new(
        event_storage: Arc<EventStorage>,
        token_storage: Arc<TokenStorage>,
        orderbook_storage: Arc<OrderBookStorage>,
        volume_storage: Arc<VolumeStorage>,
        change_storage: Arc<ChangeStorage>,
        markets_storage: Arc<MarketsStorage>,
        markets_abs_storage: Arc<MarketsAbsStorage>,
        sol_price_service: Arc<crate::price::SolPriceService>,
    ) -> Self {
        Self {
            event_storage,
            token_storage,
            orderbook_storage,
            volume_storage,
            change_storage,
            markets_storage,
            markets_abs_storage,
            sol_price_service,
            kline_socket_service: None,
            sync_monitor: None,
        }
    }

    /// 设置 K线 Socket 服务 (用于推送 LiquidateEvent)
    /// Set K-line socket service (for pushing LiquidateEvent)
    pub fn set_kline_socket_service(&mut self, service: Arc<crate::kline::KlineSocketService>) {
        self.kline_socket_service = Some(service);
    }

    /// 设置 OrderBook 同步监控器 / Set OrderBook sync monitor
    pub fn set_sync_monitor(&mut self, monitor: Arc<crate::orderbook_sync::OrderBookSyncMonitor>) {
        self.sync_monitor = Some(monitor);
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

        // 🔧 P0 修复: 串行化处理 OrderBook 操作和价格更新，避免竞态条件
        // 🔧 P0 Fix: Serialize OrderBook operations and price updates to avoid race conditions
        // 在单个 spawn_blocking 任务中按顺序执行所有操作
        // Execute all operations sequentially in a single spawn_blocking task
        let this = self.clone();
        let event_for_processing = event.clone();
        let token_storage = self.token_storage.clone();

        let liquidate_events = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<PinpetEvent>> {
            let mut additional_events = Vec::new();

            // ====== 第一步：处理订单操作 / Step 1: Process order operations ======

            // 如果是 LongShortEvent，插入到 OrderBook / If LongShortEvent, insert to OrderBook
            if let PinpetEvent::LongShort(ref ls_event) = event_for_processing {
                match this.handle_long_short_event(ls_event) {
                    Ok(events) => additional_events.extend(events),
                    Err(e) => {
                        error!("❌ 处理 LongShortEvent 失败 / Failed to handle LongShortEvent: {}", e);
                        // 继续存储事件，不因 OrderBook 失败而中断 / Continue storing event, don't fail due to OrderBook error
                    }
                }
            }

            // 如果是 BuySellEvent，处理清算 / If BuySellEvent, handle liquidations
            if let PinpetEvent::BuySell(ref bs_event) = event_for_processing {
                match this.handle_buy_sell_event(bs_event) {
                    Ok(events) => additional_events.extend(events),
                    Err(e) => {
                        error!("❌ 处理 BuySellEvent 清算失败 / Failed to handle BuySellEvent liquidations: {}", e);
                        // 继续存储事件，不因 OrderBook 失败而中断 / Continue storing event, don't fail due to OrderBook error
                    }
                }
            }

            // 如果是 FullCloseEvent，处理清算 / If FullCloseEvent, handle liquidations
            if let PinpetEvent::FullClose(ref fc_event) = event_for_processing {
                match this.handle_full_close_event(fc_event) {
                    Ok(events) => additional_events.extend(events),
                    Err(e) => {
                        error!("❌ 处理 FullCloseEvent 清算失败 / Failed to handle FullCloseEvent liquidations: {}", e);
                        // 继续存储事件，不因 OrderBook 失败而中断 / Continue storing event, don't fail due to OrderBook error
                    }
                }
            }

            // 如果是 PartialCloseEvent，处理更新和清算 / If PartialCloseEvent, handle update and liquidations
            if let PinpetEvent::PartialClose(ref pc_event) = event_for_processing {
                match this.handle_partial_close_event(pc_event) {
                    Ok(events) => additional_events.extend(events),
                    Err(e) => {
                        error!("❌ 处理 PartialCloseEvent 更新和清算失败 / Failed to handle PartialCloseEvent update and liquidations: {}", e);
                        // 继续存储事件，不因 OrderBook 失败而中断 / Continue storing event, don't fail due to OrderBook error
                    }
                }
            }

            // ====== 第二步：更新统计信息（必须在更新价格之前）/ Step 2: Update statistics (must be before price update) ======
            // 重要：必须在更新价格之前调用，这样 get_previous_price 才能获取到上一个事件的价格
            // Important: Must be called before price update so get_previous_price can get the previous event's price
            this.update_volume_statistics(&event_for_processing)?;
            this.update_change_statistics(&event_for_processing)?;
            this.update_markets_statistics(&event_for_processing)?;
            this.update_markets_abs_statistics(&event_for_processing)?;

            // ====== 第三步：更新价格（在同一个任务中串行执行）/ Step 3: Update price (execute serially in same task) ======

            // 更新Token的latest_price（所有带latest_price的事件）/ Update token's latest_price (all events with latest_price)
            match &event_for_processing {
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

            Ok(additional_events)
        }).await??;

        // 🔧 P1 修复: 批量存储主事件和清算事件,避免签名映射被覆盖
        // 🔧 P1 Fix: Batch store main event and liquidate events to avoid sig_map overwrite

        // 提取 mint 地址用于更新同步监控器 / Extract mint address for sync monitor
        let mint = match &event {
            PinpetEvent::TokenCreated(e) => e.mint_account.clone(),
            PinpetEvent::BuySell(e) => e.mint_account.clone(),
            PinpetEvent::LongShort(e) => e.mint_account.clone(),
            PinpetEvent::FullClose(e) => e.mint_account.clone(),
            PinpetEvent::PartialClose(e) => e.mint_account.clone(),
            PinpetEvent::MilestoneDiscount(e) => e.mint_account.clone(),
            PinpetEvent::Liquidate(e) => e.mint_account.clone(),
        };

        // 合并主事件和清算事件 / Merge main event and liquidate events
        let mut all_events = vec![event];
        let liquidate_events_count = liquidate_events.len();
        all_events.extend(liquidate_events.iter().cloned());

        // 一次性存储所有事件到数据库 / Store all events to database at once
        match self.event_storage.store_events(&signature, all_events).await {
            Ok(_) => {
                let total_count = 1 + liquidate_events_count;
                info!("✅ 批量存储{}个事件成功 / Batch stored {} events successfully: signature={}, main=1, liquidate={}",
                      total_count, total_count, &signature[..8], liquidate_events_count);
            }
            Err(e) => {
                error!("❌ 批量存储事件失败 / Failed to batch store events: {}", e);
                return Err(e);
            }
        }

        // 更新同步监控器的事件时间 / Update sync monitor's event time
        if let Some(ref sync_monitor) = self.sync_monitor {
            sync_monitor.update_event_time(&mint).await;
        }

        // 推送清算事件到 Socket.IO (如果服务可用) / Push liquidate events to Socket.IO (if service available)
        if !liquidate_events.is_empty() {
            if let Some(ref kline_service) = self.kline_socket_service {
                for liquidate_event in liquidate_events {
                    info!("📡 推送 LiquidateEvent 到 Socket.IO / Pushing LiquidateEvent to Socket.IO");
                    if let Err(err) = kline_service.broadcast_event_update(&liquidate_event).await {
                        error!("❌ 推送 LiquidateEvent 失败 / Failed to broadcast LiquidateEvent: {}", err);
                        // 不中断主流程 / Don't interrupt main flow
                    }
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
            // 🔧 P0 修复: 获取当前数据库中的价格，即事件发生前的价格
            // 🔧 P0 Fix: Get price from database, which is the price before event
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

        // 0. 在更新前先加载当前订单,用于记录半平仓历史
        // 0. Load current order before update for recording partial close history
        let current_order = manager.get_order(event.order_index)?;

        // 验证 order_id 是否匹配 / Verify order_id matches
        if current_order.order_id != event.order_id {
            return Err(anyhow::anyhow!(
                "Order ID mismatch: current={}, event={}",
                current_order.order_id, event.order_id
            ));
        }

        // ⭐ 核心修改：基于 lock_lp_token_amount 比例计算新保证金
        // ⭐ Core change: Calculate new margin based on lock_lp_token_amount ratio
        // 合约中的 event.margin_sol_amount 永远是初始值，不能直接使用
        // event.margin_sol_amount in contract is always the initial value, cannot be used directly

        let new_margin_sol_amount = Self::calculate_margin_by_token_ratio(
            current_order.margin_sol_amount,       // 旧保证金 / Old margin
            current_order.lock_lp_token_amount,    // 旧持仓 token / Old token position
            event.lock_lp_token_amount,            // 新持仓 token (来自事件) / New token position (from event)
        ).ok_or_else(|| anyhow::anyhow!(
            "Failed to calculate new margin_sol_amount for order_id={}, order_index={}",
            event.order_id, event.order_index
        ))?;

        info!(
            "📊 半平仓保证金计算 / Partial close margin calculation:
             - 订单 / Order: order_id={}, index={}
             - 旧保证金 / Old margin: {} lamports
             - 旧持仓 / Old token: {}
             - 新持仓 / New token: {}
             - 新保证金 / New margin: {} lamports",
            event.order_id,
            event.order_index,
            current_order.margin_sol_amount,
            current_order.lock_lp_token_amount,
            event.lock_lp_token_amount,
            new_margin_sol_amount
        );

        // 计算本次半平仓产生的利润 / Calculate profit from this partial close
        // realized_sol_amount_delta = 事件的 realized_sol_amount - 当前仓位的 realized_sol_amount
        // realized_sol_amount_delta = event's realized_sol_amount - current position's realized_sol_amount
        let realized_sol_amount_delta = event.realized_sol_amount.saturating_sub(current_order.realized_sol_amount);

        info!(
            "📊 半平仓利润计算 / Partial close profit calculation: event_realized={}, current_realized={}, delta={}",
            event.realized_sol_amount, current_order.realized_sol_amount, realized_sol_amount_delta
        );

        // 构建"被平掉部分"的订单记录 / Build order record for "closed portion"
        // 这个记录代表被平掉的那部分仓位 / This record represents the closed portion of the position
        let closed_portion_order = MarginOrder {
            user: current_order.user.clone(),
            lock_lp_start_price: current_order.lock_lp_start_price,
            lock_lp_end_price: current_order.lock_lp_end_price,
            open_price: current_order.open_price,
            order_id: current_order.order_id,
            // 被平掉的部分数量 = 原数量 - 新数量
            // Closed portion amount = old amount - new amount
            lock_lp_sol_amount: current_order.lock_lp_sol_amount.saturating_sub(event.lock_lp_sol_amount),
            lock_lp_token_amount: current_order.lock_lp_token_amount.saturating_sub(event.lock_lp_token_amount),
            next_lp_sol_amount: current_order.next_lp_sol_amount,
            next_lp_token_amount: current_order.next_lp_token_amount,
            // ⭐ margin_init_sol_amount 永远不变 (保持原始值)
            // ⭐ margin_init_sol_amount never changes (keep original value)
            margin_init_sol_amount: current_order.margin_init_sol_amount,
            // ⭐ 修改: 被平掉部分的保证金 = 原保证金 - 新计算的保证金
            // ⭐ Change: Closed portion margin = old margin - newly calculated margin
            margin_sol_amount: current_order.margin_sol_amount.saturating_sub(new_margin_sol_amount),
            // 被平掉部分的借款 = 原借款 - 新借款
            // Closed portion borrow = old borrow - new borrow
            borrow_amount: current_order.borrow_amount.saturating_sub(event.borrow_amount),
            // 被平掉部分的持仓 = 原持仓 - 新持仓
            // Closed portion position = old position - new position
            position_asset_amount: current_order.position_asset_amount.saturating_sub(event.position_asset_amount),
            // 本次半平仓产生的利润 / Profit from this partial close
            realized_sol_amount: realized_sol_amount_delta,
            version: current_order.version,
            start_time: current_order.start_time,
            end_time: current_order.end_time,
            next_order: current_order.next_order,
            prev_order: current_order.prev_order,
            borrow_fee: current_order.borrow_fee,
            order_type: current_order.order_type,
        };

        // 保存半平仓历史记录 / Save partial close history record
        // 使用 close_reason = 4 (用户主动半平仓 / User initiated partial close)
        // 使用最新价格 (即当前事件的价格) / Use latest price (current event's price)
        self.save_partial_close_record(
            &event.mint_account,
            direction,
            &closed_portion_order,
            event.timestamp.timestamp() as u32,
            event.latest_price,
        )?;

        info!(
            "✅ 半平仓历史记录已保存 / Partial close history record saved: order_id={}, delta_realized={}",
            event.order_id, realized_sol_amount_delta
        );

        // 1. 更新订单 / Update the order
        use crate::orderbook::MarginOrderUpdateData;
        let update_data = MarginOrderUpdateData {
            lock_lp_start_price: Some(event.lock_lp_start_price),
            lock_lp_end_price: Some(event.lock_lp_end_price),
            lock_lp_sol_amount: Some(event.lock_lp_sol_amount),
            lock_lp_token_amount: Some(event.lock_lp_token_amount),
            next_lp_sol_amount: None,  // 不更新 / Don't update
            next_lp_token_amount: None,  // 不更新 / Don't update
            end_time: Some(event.end_time),
            // ⭐ margin_init_sol_amount 永远不更新
            // ⭐ margin_init_sol_amount never updates
            margin_init_sol_amount: None,
            // ⭐ 核心修改: 使用计算后的新保证金，而不是事件中的值
            // ⭐ Core change: Use calculated new margin, not the value from event
            margin_sol_amount: Some(new_margin_sol_amount),
            borrow_amount: Some(event.borrow_amount),
            position_asset_amount: Some(event.position_asset_amount),
            borrow_fee: Some(event.borrow_fee),
            open_price: None,  // 不更新 / Don't update
            realized_sol_amount: Some(event.realized_sol_amount),
        };

        manager.update_order(event.order_index, event.order_id, &update_data)?;

        info!(
            "✅ PartialCloseEvent 订单更新完成 / PartialCloseEvent order update completed: order_id={}, order_index={}, new_margin={}",
            event.order_id, event.order_index, new_margin_sol_amount
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

    /// 基于 lock_lp_token_amount 的比例计算新的 margin_sol_amount
    /// Calculate new margin_sol_amount based on lock_lp_token_amount ratio
    ///
    /// # 参数 / Parameters
    /// * `old_margin` - 当前保证金 / Current margin SOL amount
    /// * `old_token` - 旧的持仓 token 数量 / Old lock_lp_token_amount
    /// * `new_token` - 新的持仓 token 数量 / New lock_lp_token_amount
    ///
    /// # 返回值 / Returns
    /// 计算后的新保证金,如果计算失败返回 None
    /// Calculated new margin, or None if calculation fails
    ///
    /// # 算法 / Algorithm
    /// ```text
    /// 新保证金 = 原保证金 × (新token数量 / 旧token数量)
    /// new_margin = old_margin × (new_token / old_token)
    /// ```
    ///
    /// # 特殊情况处理 / Edge Cases
    /// - 如果 old_token = 0: 返回 None (避免除零)
    /// - 如果 new_token = 0: 返回 0 (仓位全平)
    /// - 如果 new_token > old_token: 返回 None (异常情况,仓位不应增加)
    /// - 如果计算结果 > u64::MAX: 返回 None (溢出)
    fn calculate_margin_by_token_ratio(
        old_margin: u64,
        old_token: u64,
        new_token: u64,
    ) -> Option<u64> {
        // 特殊情况1: 旧持仓为0,无法计算比例
        if old_token == 0 {
            warn!("⚠️  无法计算保证金比例: old_token = 0");
            return None;
        }

        // 特殊情况2: 新持仓为0,保证金也为0
        if new_token == 0 {
            return Some(0);
        }

        // 特殊情况3: 新持仓大于旧持仓,异常情况
        if new_token > old_token {
            warn!(
                "⚠️  异常: 新持仓 ({}) 大于旧持仓 ({}), 半平仓不应增加仓位",
                new_token, old_token
            );
            return None;
        }

        // 转换为 Decimal 进行高精度计算 (使用默认28位精度)
        let old_margin_dec = Decimal::from(old_margin);
        let old_token_dec = Decimal::from(old_token);
        let new_token_dec = Decimal::from(new_token);

        // 计算比例: ratio = new_token / old_token
        let ratio = new_token_dec / old_token_dec;

        // 计算新保证金: new_margin = old_margin × ratio
        let new_margin_dec = old_margin_dec * ratio;

        // 舍入到最接近的整数 (使用银行家舍入)
        let new_margin_rounded = new_margin_dec.round();

        // 转换回 u64
        match new_margin_rounded.to_u64() {
            Some(value) => {
                info!(
                    "💰 保证金按比例计算 / Margin calculated by ratio: old_margin={}, old_token={}, new_token={}, ratio={}, new_margin={}",
                    old_margin, old_token, new_token, ratio, value
                );
                Some(value)
            }
            None => {
                error!(
                    "❌ 保证金计算溢出 / Margin calculation overflow: old_margin={}, ratio={}, result={}",
                    old_margin, ratio, new_margin_rounded
                );
                None
            }
        }
    }

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

    /// 保存半平仓历史记录
    /// Save partial close history record
    ///
    /// # 参数 / Parameters
    /// * `mint` - Token mint 地址 / Token mint address
    /// * `direction` - 订单方向 / Order direction
    /// * `closed_portion` - 被平掉部分的订单数据 / Closed portion order data
    /// * `close_timestamp` - 平仓时间戳 / Close timestamp
    /// * `close_price` - 平仓价格 / Close price
    fn save_partial_close_record(
        &self,
        mint: &str,
        direction: &str,
        closed_portion: &MarginOrder,
        close_timestamp: u32,
        close_price: u128,
    ) -> anyhow::Result<()> {
        use crate::orderbook::types::{ClosedOrderRecord, CloseInfo};

        // 构建关闭信息 / Build close info
        let close_info = CloseInfo {
            close_timestamp,
            close_price,
            close_reason: 4, // 用户主动半平仓 / User initiated partial close
        };

        // 构建已关闭订单记录 / Build closed order record
        let close_record = ClosedOrderRecord {
            mint: mint.to_string(),
            direction: direction.to_string(),
            order: closed_portion.clone(),
            close_info,
        };

        // 生成键 / Generate key
        // 键格式: orderbook_user_closed:{user}:{close_timestamp:010}:{mint}:{direction}:{order_id:020}
        // Key format: orderbook_user_closed:{user}:{close_timestamp:010}:{mint}:{direction}:{order_id:020}
        let close_key = format!(
            "orderbook_user_closed:{}:{:010}:{}:{}:{:020}",
            closed_portion.user,
            close_timestamp,
            mint,
            direction,
            closed_portion.order_id
        );

        // 序列化并保存到数据库 / Serialize and save to database
        let value = serde_json::to_vec(&close_record)?;
        self.orderbook_storage.db().put(close_key.as_bytes(), value)?;

        info!(
            "📝 半平仓记录已保存 / Partial close record saved: key={}",
            close_key
        );

        Ok(())
    }

    /// 更新交易额统计 / Update volume statistics
    ///
    /// # 参数 / Parameters
    /// * `event` - 事件 / Event
    fn update_volume_statistics(&self, event: &PinpetEvent) -> anyhow::Result<()> {
        use crate::curve_amm::CurveAMM;

        // 提取事件信息 / Extract event info
        let (mint, price_after, timestamp) = match event {
            PinpetEvent::TokenCreated(e) => (&e.mint_account, e.latest_price, e.timestamp.timestamp() as u64),
            PinpetEvent::BuySell(e) => (&e.mint_account, e.latest_price, e.timestamp.timestamp() as u64),
            PinpetEvent::LongShort(e) => (&e.mint_account, e.latest_price, e.timestamp.timestamp() as u64),
            PinpetEvent::FullClose(e) => (&e.mint_account, e.latest_price, e.timestamp.timestamp() as u64),
            PinpetEvent::PartialClose(e) => (&e.mint_account, e.latest_price, e.timestamp.timestamp() as u64),
            PinpetEvent::MilestoneDiscount(_) | PinpetEvent::Liquidate(_) => {
                // 这两个事件不包含价格变动,不更新交易额 / These events don't contain price changes, skip volume update
                return Ok(());
            }
        };

        // 获取变动前的价格 / Get price before change
        let price_before = if matches!(event, PinpetEvent::TokenCreated(_)) {
            // TokenCreated 事件使用初始价格 / TokenCreated event uses initial price
            CurveAMM::get_initial_price()
                .ok_or_else(|| anyhow::anyhow!("Failed to get initial price"))?
        } else {
            // 其他事件从数据库获取上一次的价格 / Other events get previous price from database
            self.get_previous_price(mint)?
        };

        // 获取 SOL/USD 汇率 / Get SOL/USD exchange rate
        let sol_price_usd = self.sol_price_service.get_price_sync();

        debug!(
            "💰 Volume计算参数 / Volume calc params: mint={}, price_before={}, price_after={}, sol_usd={:.2}",
            &mint[..8.min(mint.len())],
            price_before,
            price_after,
            sol_price_usd
        );

        // 更新交易额 / Update volume
        if let Err(e) = self.volume_storage.update_volume(
            mint,
            price_before,
            price_after,
            sol_price_usd,
            timestamp,
        ) {
            error!("❌ 更新交易额失败 / Failed to update volume: mint={}, error={}",
                   &mint[..8.min(mint.len())], e);
            // 不中断主流程 / Don't interrupt main flow
        }

        Ok(())
    }

    /// 更新涨跌幅统计 / Update change statistics
    ///
    /// # 参数 / Parameters
    /// * `event` - 事件 / Event
    fn update_change_statistics(&self, event: &PinpetEvent) -> anyhow::Result<()> {
        use crate::curve_amm::CurveAMM;

        // 提取事件信息 / Extract event info
        let (mint, price_after, timestamp) = match event {
            PinpetEvent::TokenCreated(e) => (&e.mint_account, e.latest_price, e.timestamp.timestamp() as u64),
            PinpetEvent::BuySell(e) => (&e.mint_account, e.latest_price, e.timestamp.timestamp() as u64),
            PinpetEvent::LongShort(e) => (&e.mint_account, e.latest_price, e.timestamp.timestamp() as u64),
            PinpetEvent::FullClose(e) => (&e.mint_account, e.latest_price, e.timestamp.timestamp() as u64),
            PinpetEvent::PartialClose(e) => (&e.mint_account, e.latest_price, e.timestamp.timestamp() as u64),
            PinpetEvent::MilestoneDiscount(_) | PinpetEvent::Liquidate(_) => {
                // 这两个事件不包含价格变动,不更新涨跌幅 / These events don't contain price changes, skip change update
                return Ok(());
            }
        };

        // 获取 SOL/USD 汇率 / Get SOL/USD exchange rate
        let sol_price_usd = self.sol_price_service.get_price_sync();

        // 将价格从 lamports 转换为 USD / Convert price from lamports to USD
        // price 单位是 lamports, 需要转换为 SOL 再转换为 USD
        // price is in lamports, need to convert to SOL then to USD
        let (sol_reserve, _token_reserve) = CurveAMM::price_to_reserves(price_after)
            .ok_or_else(|| anyhow::anyhow!("Failed to calculate reserves"))?;

        // SOL 储备单位是 lamports (1 SOL = 10^9 lamports)
        // SOL reserve is in lamports (1 SOL = 10^9 lamports)
        let sol_amount = sol_reserve as f64 / 1_000_000_000.0;
        let price_usd = sol_amount * sol_price_usd;

        debug!(
            "📊 Change计算参数 / Change calc params: mint={}, price_usd=${:.9}, timestamp={}",
            &mint[..8.min(mint.len())],
            price_usd,
            timestamp
        );

        // 更新涨跌幅 / Update change
        if let Err(e) = self.change_storage.update_change(
            mint,
            price_usd,
            timestamp,
        ) {
            error!("❌ 更新涨跌幅失败 / Failed to update change: mint={}, error={}",
                   &mint[..8.min(mint.len())], e);
            // 不中断主流程 / Don't interrupt main flow
        }

        Ok(())
    }

    /// 更新钱包数统计 / Update markets statistics
    ///
    /// # 参数 / Parameters
    /// * `event` - 事件 / Event
    fn update_markets_statistics(&self, event: &PinpetEvent) -> anyhow::Result<()> {
        // 提取事件信息 / Extract event info
        let (mint, wallet, timestamp) = match event {
            PinpetEvent::TokenCreated(_) => {
                // TokenCreated 不涉及用户钱包,跳过
                // TokenCreated doesn't involve user wallet, skip
                return Ok(());
            }
            PinpetEvent::BuySell(e) => (&e.mint_account, &e.payer, e.timestamp.timestamp() as u64),
            PinpetEvent::LongShort(e) => (&e.mint_account, &e.payer, e.timestamp.timestamp() as u64),
            PinpetEvent::FullClose(e) => (&e.mint_account, &e.payer, e.timestamp.timestamp() as u64),
            PinpetEvent::PartialClose(e) => (&e.mint_account, &e.payer, e.timestamp.timestamp() as u64),
            PinpetEvent::MilestoneDiscount(_) | PinpetEvent::Liquidate(_) => {
                // 这两个事件不统计钱包数
                // These events don't count towards markets
                return Ok(());
            }
        };

        debug!(
            "👛 Markets统计参数 / Markets calc params: mint={}, wallet={}, timestamp={}",
            &mint[..8.min(mint.len())],
            &wallet[..8.min(wallet.len())],
            timestamp
        );

        // 更新钱包数 / Update markets
        if let Err(e) = self.markets_storage.update_markets(mint, wallet, timestamp) {
            error!(
                "❌ 更新钱包数失败 / Failed to update markets: mint={}, error={}",
                &mint[..8.min(mint.len())],
                e
            );
            // 不中断主流程 / Don't interrupt main flow
        }

        Ok(())
    }

    /// 更新绝对钱包数统计 / Update markets abs statistics
    ///
    /// # 参数 / Parameters
    /// * `event` - 事件 / Event
    fn update_markets_abs_statistics(&self, event: &PinpetEvent) -> anyhow::Result<()> {
        // 提取事件信息 / Extract event info
        let (mint, wallet, timestamp) = match event {
            PinpetEvent::TokenCreated(_) => {
                // TokenCreated 不涉及用户钱包,跳过
                // TokenCreated doesn't involve user wallet, skip
                return Ok(());
            }
            PinpetEvent::BuySell(e) => (&e.mint_account, &e.payer, e.timestamp.timestamp() as u64),
            PinpetEvent::LongShort(e) => (&e.mint_account, &e.payer, e.timestamp.timestamp() as u64),
            PinpetEvent::FullClose(e) => (&e.mint_account, &e.payer, e.timestamp.timestamp() as u64),
            PinpetEvent::PartialClose(e) => (&e.mint_account, &e.payer, e.timestamp.timestamp() as u64),
            PinpetEvent::MilestoneDiscount(_) | PinpetEvent::Liquidate(_) => {
                // 这两个事件不统计钱包数
                // These events don't count towards markets abs
                return Ok(());
            }
        };

        debug!(
            "🌐 MarketsAbs统计参数 / MarketsAbs calc params: mint={}, wallet={}, timestamp={}",
            &mint[..8.min(mint.len())],
            &wallet[..8.min(wallet.len())],
            timestamp
        );

        // 更新绝对钱包数 / Update markets abs
        if let Err(e) = self.markets_abs_storage.update_markets_abs(mint, wallet, timestamp) {
            error!(
                "❌ 更新绝对钱包数失败 / Failed to update markets abs: mint={}, error={}",
                &mint[..8.min(mint.len())],
                e
            );
            // 不中断主流程 / Don't interrupt main flow
        }

        Ok(())
    }
}

/// 处理包含多个事件的交易 / Process transactions containing multiple events
#[allow(dead_code)]
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
#[allow(dead_code)]
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
// ==================== 测试模块 / Test Module ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_margin_by_token_ratio_normal() {
        // 正常场景: 平50% / Normal case: close 50%
        let old_margin = 100_000_000_000_u64; // 100 SOL
        let old_token = 1_000_000_000_u64;
        let new_token = 500_000_000_u64;

        let result = StorageEventHandler::calculate_margin_by_token_ratio(
            old_margin, old_token, new_token
        );

        assert_eq!(result, Some(50_000_000_000)); // 50 SOL
    }

    #[test]
    fn test_calculate_margin_by_token_ratio_precision() {
        // 高精度测试: 复杂比例 / High precision test: complex ratio
        let old_margin = 100_000_000_000_u64; // 100 SOL
        let old_token = 1_000_000_u64;
        let new_token = 666_667_u64; // 保留 66.6667%

        let result = StorageEventHandler::calculate_margin_by_token_ratio(
            old_margin, old_token, new_token
        );

        // 期望: 66.6667 SOL ≈ 66_666_700_000 lamports
        assert!(result.is_some());
        let value = result.unwrap();
        // 允许一定的舍入误差
        assert!(value >= 66_666_000_000 && value <= 66_667_000_000);
    }

    #[test]
    fn test_calculate_margin_by_token_ratio_zero_old_token() {
        // 边界: 旧持仓为0 / Edge case: old token = 0
        let result = StorageEventHandler::calculate_margin_by_token_ratio(
            100_000_000_000, 0, 50_000_000
        );
        assert_eq!(result, None);
    }

    #[test]
    fn test_calculate_margin_by_token_ratio_zero_new_token() {
        // 边界: 新持仓为0 (全平) / Edge case: new token = 0 (full close)
        let result = StorageEventHandler::calculate_margin_by_token_ratio(
            100_000_000_000, 1_000_000, 0
        );
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_calculate_margin_by_token_ratio_increase() {
        // 异常: 新持仓大于旧持仓 / Abnormal: new token > old token
        let result = StorageEventHandler::calculate_margin_by_token_ratio(
            100_000_000_000, 500_000, 1_000_000
        );
        assert_eq!(result, None);
    }

    #[test]
    fn test_multiple_partial_close_sequence() {
        // 模拟多次半平仓 / Simulate multiple partial closes
        let mut margin = 100_000_000_000_u64; // 100 SOL
        let mut token = 1_000_000_000_u64;

        // 第一次平50% / First close: 50%
        let new_token_1 = 500_000_000;
        margin = StorageEventHandler::calculate_margin_by_token_ratio(
            margin, token, new_token_1
        ).unwrap();
        assert_eq!(margin, 50_000_000_000); // 50 SOL
        token = new_token_1;

        // 第二次再平50% / Second close: 50% again
        let new_token_2 = 250_000_000;
        margin = StorageEventHandler::calculate_margin_by_token_ratio(
            margin, token, new_token_2
        ).unwrap();
        assert_eq!(margin, 25_000_000_000); // 25 SOL
        token = new_token_2;

        // 第三次再平50% / Third close: 50% again
        let new_token_3 = 125_000_000;
        margin = StorageEventHandler::calculate_margin_by_token_ratio(
            margin, token, new_token_3
        ).unwrap();
        assert_eq!(margin, 12_500_000_000); // 12.5 SOL
        token = new_token_3;

        // 第四次平光 / Fourth close: close all
        margin = StorageEventHandler::calculate_margin_by_token_ratio(
            margin, token, 0
        ).unwrap();
        assert_eq!(margin, 0);
    }
}
