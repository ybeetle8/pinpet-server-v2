// Per-mint 事件路由器 / Per-mint event router
// 确保同一 mint 的事件按顺序串行执行，不同 mint 之间并行执行
// Ensures events for the same mint are executed sequentially, while different mints run in parallel

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{info, error};

use async_trait::async_trait;
use super::events::PinpetEvent;
use super::liquidation::LiquidationProcessor;
use super::storage_handler::StorageEventHandler;
use super::listener::EventHandler;

/// Per-mint 事件处理任务 / Per-mint event processing task
struct MintEventTask {
    mint: String,
    receiver: mpsc::UnboundedReceiver<PinpetEvent>,
    liquidation_processor: Arc<LiquidationProcessor>,
    storage_handler: Arc<StorageEventHandler>,
}

impl MintEventTask {
    /// 运行事件处理任务 / Run event processing task
    async fn run(mut self) {
        info!("🚀 启动 mint 事件处理任务 / Started mint event processing task: mint={}", self.mint);

        while let Some(event) = self.receiver.recv().await {
            if let Err(e) = self.process_event(event).await {
                error!(
                    "❌ 处理事件失败 / Failed to process event: mint={}, error={}",
                    self.mint, e
                );
            }
        }

        info!("🛑 停止 mint 事件处理任务 / Stopped mint event processing task: mint={}", self.mint);
    }

    /// 处理单个事件 / Process single event
    async fn process_event(&self, event: PinpetEvent) -> anyhow::Result<()> {
        let event_type = match &event {
            PinpetEvent::TokenCreated(_) => "TokenCreated",
            PinpetEvent::BuySell(_) => "BuySell",
            PinpetEvent::LongShort(_) => "LongShort",
            PinpetEvent::FullClose(_) => "FullClose",
            PinpetEvent::PartialClose(_) => "PartialClose",
            PinpetEvent::MilestoneDiscount(_) => "MilestoneDiscount",
        };

        info!(
            "处理事件 / Processing event: mint={}, type={}",
            self.mint, event_type
        );

        // 先处理清算逻辑 / Process liquidation first
        match &event {
            PinpetEvent::BuySell(e) => {
                self.process_liquidation_for_buysell(e).await?;
            }
            PinpetEvent::LongShort(e) => {
                self.process_liquidation_for_longshort(e).await?;
            }
            PinpetEvent::FullClose(e) => {
                self.process_liquidation_for_fullclose(e).await?;
            }
            PinpetEvent::PartialClose(e) => {
                self.process_liquidation_for_partialclose(e).await?;
            }
            _ => {
                // 其他事件类型不需要清算 / Other event types don't need liquidation
            }
        }

        // 然后存储事件（包括 LongShort 插入和 PartialClose 更新）
        // Then store event (including LongShort insert and PartialClose update)
        self.storage_handler.handle_event(event).await?;

        Ok(())
    }

    /// 处理 BuySell 事件的清算 / Process liquidation for BuySell event
    async fn process_liquidation_for_buysell(
        &self,
        event: &super::events::BuySellEvent,
    ) -> anyhow::Result<()> {
        if event.liquidate_indices.is_empty() {
            return Ok(());
        }

        let direction = super::liquidation::get_liquidation_direction_for_buysell(event);

        info!(
            "BuySell 事件清算 / BuySell liquidation: mint={}, dir={}, indices={:?}",
            event.mint_account, direction, event.liquidate_indices
        );

        self.liquidation_processor
            .process_liquidation(&event.mint_account, direction, &event.liquidate_indices)
            .await
    }

    /// 处理 LongShort 事件的清算 / Process liquidation for LongShort event
    async fn process_liquidation_for_longshort(
        &self,
        event: &super::events::LongShortEvent,
    ) -> anyhow::Result<()> {
        if event.liquidate_indices.is_empty() {
            return Ok(());
        }

        let direction = super::liquidation::get_liquidation_direction_for_longshort(event);

        info!(
            "LongShort 事件清算 / LongShort liquidation: mint={}, dir={}, indices={:?}",
            event.mint_account, direction, event.liquidate_indices
        );

        self.liquidation_processor
            .process_liquidation(&event.mint_account, direction, &event.liquidate_indices)
            .await
    }

    /// 处理 FullClose 事件的清算 / Process liquidation for FullClose event
    async fn process_liquidation_for_fullclose(
        &self,
        event: &super::events::FullCloseEvent,
    ) -> anyhow::Result<()> {
        if event.liquidate_indices.is_empty() {
            return Ok(());
        }

        info!(
            "FullClose 事件清算 / FullClose liquidation: mint={}, order_id={}, indices={:?}",
            event.mint_account, event.order_id, event.liquidate_indices
        );

        // 使用专门的 FullClose 清算处理，会根据 order_id 和 user_sol_account 判断 close_type
        // Use specialized FullClose liquidation handler, which determines close_type based on order_id and user_sol_account
        self.liquidation_processor
            .process_fullclose_liquidation(event)
            .await
    }

    /// 处理 PartialClose 事件的清算 / Process liquidation for PartialClose event
    async fn process_liquidation_for_partialclose(
        &self,
        event: &super::events::PartialCloseEvent,
    ) -> anyhow::Result<()> {
        if event.liquidate_indices.is_empty() {
            return Ok(());
        }

        let direction = super::liquidation::get_liquidation_direction_for_partialclose(event);

        info!(
            "PartialClose 事件清算 / PartialClose liquidation: mint={}, dir={}, indices={:?}",
            event.mint_account, direction, event.liquidate_indices
        );

        self.liquidation_processor
            .process_liquidation(&event.mint_account, direction, &event.liquidate_indices)
            .await
    }
}

/// Mint 事件路由器 / Mint event router
/// 维护 per-mint 事件队列，确保同一 mint 的事件串行执行
/// Maintains per-mint event queues, ensures events for the same mint are executed sequentially
pub struct MintEventRouter {
    senders: Arc<Mutex<HashMap<String, mpsc::UnboundedSender<PinpetEvent>>>>,
    liquidation_processor: Arc<LiquidationProcessor>,
    storage_handler: Arc<StorageEventHandler>,
}

impl MintEventRouter {
    /// 创建新的 mint 事件路由器 / Create new mint event router
    pub fn new(
        liquidation_processor: Arc<LiquidationProcessor>,
        storage_handler: Arc<StorageEventHandler>,
    ) -> Self {
        Self {
            senders: Arc::new(Mutex::new(HashMap::new())),
            liquidation_processor,
            storage_handler,
        }
    }

    /// 路由事件到对应的 mint 处理任务 / Route event to corresponding mint processing task
    pub async fn route_event(&self, event: PinpetEvent) -> anyhow::Result<()> {
        // 提取 mint_account / Extract mint_account
        let mint = match &event {
            PinpetEvent::TokenCreated(e) => e.mint_account.clone(),
            PinpetEvent::BuySell(e) => e.mint_account.clone(),
            PinpetEvent::LongShort(e) => e.mint_account.clone(),
            PinpetEvent::FullClose(e) => e.mint_account.clone(),
            PinpetEvent::PartialClose(e) => e.mint_account.clone(),
            PinpetEvent::MilestoneDiscount(e) => e.mint_account.clone(),
        };

        let mut senders = self.senders.lock().await;

        // 获取或创建对应 mint 的 sender / Get or create sender for the mint
        let sender = if let Some(sender) = senders.get(&mint) {
            sender.clone()
        } else {
            // 创建新的 channel 和处理任务 / Create new channel and processing task
            let (tx, rx) = mpsc::unbounded_channel();

            let task = MintEventTask {
                mint: mint.clone(),
                receiver: rx,
                liquidation_processor: self.liquidation_processor.clone(),
                storage_handler: self.storage_handler.clone(),
            };

            // 启动异步任务 / Start async task
            tokio::spawn(async move {
                task.run().await;
            });

            senders.insert(mint.clone(), tx.clone());
            info!("✨ 创建新的 mint 事件处理任务 / Created new mint event processing task: mint={}", mint);

            tx
        };

        // 发送事件到对应的处理任务 / Send event to corresponding processing task
        sender.send(event).map_err(|e| {
            error!("❌ 发送事件失败 / Failed to send event: mint={}, error={}", mint, e);
            anyhow::anyhow!("发送事件失败 / Failed to send event: {}", e)
        })?;

        Ok(())
    }

    /// 获取当前活跃的 mint 数量 / Get current number of active mints
    pub async fn active_mints_count(&self) -> usize {
        self.senders.lock().await.len()
    }
}

/// EventHandler 实现 / EventHandler implementation
/// 将 MintEventRouter 适配为 EventHandler trait
/// Adapts MintEventRouter to EventHandler trait
#[async_trait]
impl EventHandler for MintEventRouter {
    async fn handle_event(&self, event: PinpetEvent) -> anyhow::Result<()> {
        self.route_event(event).await
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
