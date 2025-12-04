// 事件定义和解析模块 / Event definition and parsing module
use base64::engine::Engine;
use borsh::BorshDeserialize;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use solana_sdk::pubkey::Pubkey;
use tracing::{debug, warn};
use utoipa::ToSchema;

/// 事件判别器 - 来自IDL文件的正确判别器 / Event discriminators - correct discriminators from IDL file
pub const TOKEN_CREATED_EVENT_DISCRIMINATOR: [u8; 8] = [96, 122, 113, 138, 50, 227, 149, 57];
pub const BUY_SELL_EVENT_DISCRIMINATOR: [u8; 8] = [98, 208, 120, 60, 93, 32, 19, 180];
pub const LONG_SHORT_EVENT_DISCRIMINATOR: [u8; 8] = [27, 69, 20, 116, 58, 250, 95, 220];
pub const FULL_CLOSE_EVENT_DISCRIMINATOR: [u8; 8] = [22, 244, 113, 245, 154, 168, 109, 139];
pub const PARTIAL_CLOSE_EVENT_DISCRIMINATOR: [u8; 8] = [133, 94, 3, 222, 24, 68, 69, 155];
pub const MILESTONE_DISCOUNT_EVENT_DISCRIMINATOR: [u8; 8] = [130, 232, 11, 37, 34, 185, 136, 128];

/// 所有Pinpet事件的统一枚举 / Unified enum for all Pinpet events
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "event_type")]
pub enum PinpetEvent {
    TokenCreated(TokenCreatedEvent),
    BuySell(BuySellEvent),
    LongShort(LongShortEvent),
    FullClose(FullCloseEvent),
    PartialClose(PartialCloseEvent),
    MilestoneDiscount(MilestoneDiscountEvent),
    Liquidate(LiquidateEvent),
}

/// 创建基本代币事件 / Token creation event
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenCreatedEvent {
    pub payer: String,
    pub mint_account: String,
    pub curve_account: String,
    pub pool_token_account: String,
    pub pool_sol_account: String,
    pub fee_recipient: String,
    pub base_fee_recipient: String,      // 基础手续费接收账户 / Base fee recipient account
    pub params_account: String,          // 合作伙伴参数账户PDA地址 / Partner params account PDA
    pub swap_fee: u16,                   // 现货交易手续费 / Spot trading fee
    pub borrow_fee: u16,                 // 保证金交易手续费 / Margin trading fee
    pub fee_discount_flag: u8,           // 手续费折扣标志 / Fee discount flag: 0:原价/original 1:5折/50% 2:2.5折/25% 3:1.25折/12.5%
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub up_orderbook: String,           // 做空订单账本 (Up方向) PDA地址 / Short orderbook (Up direction) PDA
    pub down_orderbook: String,         // 做多订单账本 (Down方向) PDA地址 / Long orderbook (Down direction) PDA
    #[serde_as(as = "DisplayFromStr")]
    pub latest_price: u128,              // 最新的价格 / Latest price
    #[schema(value_type = String)]
    pub timestamp: DateTime<Utc>,
    pub signature: String,
    pub slot: u64,
}

/// 买卖交易事件 / Buy/Sell event
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BuySellEvent {
    pub payer: String,
    pub mint_account: String,
    pub is_buy: bool,
    pub token_amount: u64,               // 最终买入或卖出的token数量 / Final token amount bought/sold
    pub sol_amount: u64,                 // 最终花费或得到的sol数量 / Final SOL amount spent/received
    #[serde_as(as = "DisplayFromStr")]
    pub latest_price: u128,              // 最新的价格 / Latest price
    pub liquidate_indices: Vec<u16>,    // 需要清算的订单索引列表 / Liquidation order indices (indices, not order IDs!)
    #[schema(value_type = String)]
    pub timestamp: DateTime<Utc>,
    pub signature: String,
    pub slot: u64,
}

/// 保证金做多做空交易事件 / Long/Short margin trading event
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LongShortEvent {
    pub payer: String,                   // 开仓用户 (payer 就是 user) / User who opened position (payer is the user)
    pub mint_account: String,
    pub order_id: u64,                   // 开仓的订单的唯一编号 / Unique order ID
    pub order_index: u16,                // 开仓的订单在订单账本中的索引 / Order index in the orderbook
    #[serde_as(as = "DisplayFromStr")]
    pub latest_price: u128,              // 最新的价格 / Latest price
    #[serde_as(as = "DisplayFromStr")]
    pub open_price: u128,                // 开仓价格 / Open price
    pub order_type: u8,                  // 订单类型 / Order type: 1:做多/long 2:做空/short
    #[serde_as(as = "DisplayFromStr")]
    pub lock_lp_start_price: u128,       // 锁定流动池区间开始价 / LP lock range start price
    #[serde_as(as = "DisplayFromStr")]
    pub lock_lp_end_price: u128,         // 锁定流动池区间结束价 / LP lock range end price
    pub lock_lp_sol_amount: u64,         // 锁定流动池区间sol数量 / Locked LP SOL amount
    pub lock_lp_token_amount: u64,       // 锁定流动池区间token数量 / Locked LP token amount
    pub start_time: u32,                 // 订单开始时间戳(秒) / Order start timestamp (seconds)
    pub end_time: u32,                   // 贷款到期时间戳(秒) / Loan expiry timestamp (seconds)
    pub margin_sol_amount: u64,          // 保证金SOL数量 / Margin SOL amount
    pub borrow_amount: u64,              // 贷款数量 / Borrowed amount
    pub position_asset_amount: u64,      // 当前持仓币的数量 / Current position asset amount
    pub borrow_fee: u16,                 // 保证金交易手续费 / Margin trading fee
    pub liquidate_indices: Vec<u16>,    // 需要清算的订单索引列表 / Liquidation order indices
    #[schema(value_type = String)]
    pub timestamp: DateTime<Utc>,
    pub signature: String,
    pub slot: u64,
}

/// 全平仓事件 / Full close event
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FullCloseEvent {
    pub payer: String,
    pub user_sol_account: String,        // close_order的开仓用户SOL账户 / User's SOL account
    pub mint_account: String,
    pub is_close_long: bool,             // 是否为平多 / Is closing long position
    pub final_token_amount: u64,         // 最终买入或卖出的token数量 / Final token amount
    pub final_sol_amount: u64,           // 最终花费或得到的sol数量 / Final SOL amount
    pub user_close_profit: u64,          // 用户平仓收入的sol数量 / User's closing profit in SOL
    #[serde_as(as = "DisplayFromStr")]
    pub latest_price: u128,              // 最新的价格 / Latest price
    pub order_id: u64,                   // 平仓订单的唯一编号 / Unique order ID
    pub order_index: u16,                // 平仓订单的索引 / Order index in the orderbook
    pub liquidate_indices: Vec<u16>,    // 需要清算的订单索引列表 / Liquidation indices
    #[schema(value_type = String)]
    pub timestamp: DateTime<Utc>,
    pub signature: String,
    pub slot: u64,
}

/// 部分平仓事件 / Partial close event
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PartialCloseEvent {
    pub payer: String,
    pub user_sol_account: String,        // close_order的开仓用户SOL账户 / User's SOL account
    pub mint_account: String,
    pub is_close_long: bool,             // 是否为平多 / Is closing long position
    pub final_token_amount: u64,         // 最终买入或卖出的token数量 / Final token amount
    pub final_sol_amount: u64,           // 最终花费或得到的sol数量 / Final SOL amount
    pub user_close_profit: u64,          // 用户平仓收入的sol数量 / User's closing profit
    #[serde_as(as = "DisplayFromStr")]
    pub latest_price: u128,              // 最新的价格 / Latest price
    pub order_id: u64,                   // 平仓订单的唯一编号 / Order ID
    pub order_index: u16,                // 开仓的订单在订单账本中的索引 / Order index in the orderbook
    // 部分平仓订单的参数(修改后的值) / Partial close order parameters (modified values)
    pub order_type: u8,                  // 订单类型 / Order type: 1:做多/long 2:做空/short
    pub user: String,                    // 开仓用户 / User who opened position
    #[serde_as(as = "DisplayFromStr")]
    pub lock_lp_start_price: u128,       // 锁定流动池区间开始价 / LP lock range start price
    #[serde_as(as = "DisplayFromStr")]
    pub lock_lp_end_price: u128,         // 锁定流动池区间结束价 / LP lock range end price
    pub lock_lp_sol_amount: u64,         // 锁定流动池区间sol数量 / Locked LP SOL amount
    pub lock_lp_token_amount: u64,       // 锁定流动池区间token数量 / Locked LP token amount
    pub start_time: u32,                 // 订单开始时间戳(秒) / Order start timestamp
    pub end_time: u32,                   // 贷款到期时间戳(秒) / Loan expiry timestamp
    pub margin_sol_amount: u64,          // 保证金SOL数量 / Margin SOL amount
    pub borrow_amount: u64,              // 贷款数量 / Borrowed amount
    pub position_asset_amount: u64,      // 当前持仓数量 / Current position amount
    pub borrow_fee: u16,                 // 保证金交易手续费 / Margin trading fee
    pub realized_sol_amount: u64,        // 实现盈亏的sol数量 / Realized P&L in SOL
    pub liquidate_indices: Vec<u16>,    // 需要清算的订单索引列表 / Liquidation indices
    #[schema(value_type = String)]
    pub timestamp: DateTime<Utc>,
    pub signature: String,
    pub slot: u64,
}

/// 交易里程碑折扣事件 / Milestone discount event
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MilestoneDiscountEvent {
    pub payer: String,
    pub mint_account: String,
    pub curve_account: String,
    pub swap_fee: u16,                   // 现货交易手续费 / Spot trading fee
    pub borrow_fee: u16,                 // 保证金交易手续费 / Margin trading fee
    pub fee_discount_flag: u8,           // 手续费折扣标志 / Fee discount flag
    #[schema(value_type = String)]
    pub timestamp: DateTime<Utc>,
    pub signature: String,
    pub slot: u64,
}

/// 事件解析器 / Event parser
#[derive(Clone)]
pub struct EventParser {
    #[allow(dead_code)]
    pub program_id: Pubkey,
}

impl EventParser {
    /// 创建新的事件解析器 / Create new event parser
    pub fn new(program_id: &str) -> anyhow::Result<Self> {
        let program_id = program_id.parse::<Pubkey>()?;
        Ok(Self { program_id })
    }

    /// 使用调用栈跟踪解析事件以捕获CPI事件 / Parse events with call stack tracking to capture CPI events
    pub fn parse_events_with_call_stack(
        &self,
        logs: &[String],
        signature: &str,
        slot: u64,
    ) -> anyhow::Result<Vec<PinpetEvent>> {
        let mut events = Vec::new();
        let mut program_stack = Vec::new();
        let mut in_target_program = false;

        debug!("开始调用栈解析，共{}行日志 / Starting call stack parsing for {} log lines", logs.len(), logs.len());

        for (i, log) in logs.iter().enumerate() {
            debug!("处理日志[{}] / Processing log[{}]: {}", i, i, log);

            // 跟踪程序调用 / Track program invocations
            if log.contains(" invoke [") {
                // 从日志中提取程序ID / Extract program ID from log
                if let Some(program_id) = Self::extract_program_id_from_log(log) {
                    program_stack.push(program_id.clone());
                    debug!(
                        "程序{}进入栈(深度:{}) / Program {} entered stack (depth: {})",
                        program_id,
                        program_stack.len(),
                        program_id,
                        program_stack.len()
                    );

                    // 检查目标程序是否在栈中 / Check if target program is in stack
                    if program_id == self.program_id.to_string() {
                        in_target_program = true;
                        debug!("目标程序{}现在激活 / Target program {} is now active", self.program_id, self.program_id);
                    }
                }
            } else if log.contains(" success") || log.contains(" failed") {
                // 程序退出 - 从栈中弹出 / Program exit - pop from stack
                if let Some(exited_program) = program_stack.pop() {
                    debug!(
                        "程序{}退出栈(剩余深度:{}) / Program {} exited stack (remaining depth: {})",
                        exited_program,
                        program_stack.len(),
                        exited_program,
                        program_stack.len()
                    );

                    // 检查是否仍在目标程序上下文中 / Check if still in target program context
                    in_target_program = program_stack
                        .iter()
                        .any(|p| p == &self.program_id.to_string());
                    if !in_target_program {
                        debug!("目标程序{}不再激活 / Target program {} is no longer active", self.program_id, self.program_id);
                    }
                }
            }

            // 在目标程序上下文中解析"Program data:"日志 / Parse "Program data:" logs in target program context
            if in_target_program && log.starts_with("Program data:") {
                debug!("在目标程序上下文中找到Program data，位于日志[{}] / Found Program data in target program context at log[{}]", i, i);

                if let Some(data_part) = log.strip_prefix("Program data: ") {
                    let data_part = data_part.trim();

                    // Base64解码 / Base64 decode
                    match base64::engine::general_purpose::STANDARD.decode(data_part) {
                        Ok(data) => {
                            debug!("成功解码Base64数据，长度:{} / Successfully decoded Base64 data, length: {}", data.len(), data.len());

                            // 从数据解析事件 / Parse event from data
                            match self.parse_event_data(&data, signature, slot) {
                                Ok(Some(event)) => {
                                    debug!(
                                        "成功从CPI上下文解析事件 / Successfully parsed event from CPI context: {:?}",
                                        event
                                    );
                                    events.push(event);
                                }
                                Ok(None) => {
                                    debug!("数据不匹配任何事件判别器 / Data didn't match any event discriminator");
                                }
                                Err(e) => {
                                    warn!("解析事件数据失败 / Failed to parse event data: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Base64解码失败 / Base64 decoding failed: {}", e);
                        }
                    }
                }
            }
        }

        debug!("调用栈解析完成。找到{}个事件 / Call stack parsing complete. Found {} events", events.len(), events.len());
        Ok(events)
    }

    /// 从invoke日志行提取程序ID / Extract program ID from invoke log line
    fn extract_program_id_from_log(log: &str) -> Option<String> {
        // 日志格式 / Log format: "Program <pubkey> invoke [depth]"
        if let Some(start) = log.find("Program ") {
            let after_program = &log[start + 8..];
            if let Some(end) = after_program.find(" invoke") {
                return Some(after_program[..end].to_string());
            }
        }
        None
    }

    /// 解析事件数据 / Parse event data
    fn parse_event_data(
        &self,
        data: &[u8],
        signature: &str,
        slot: u64,
    ) -> anyhow::Result<Option<PinpetEvent>> {
        if data.len() < 8 {
            return Ok(None);
        }

        let discriminator: [u8; 8] = data[0..8].try_into()?;
        let event_data = &data[8..];
        let timestamp = Utc::now();

        match discriminator {
            TOKEN_CREATED_EVENT_DISCRIMINATOR => {
                debug!("解析TokenCreated事件 / Parsing TokenCreated event");
                let event = TokenCreatedRaw::try_from_slice(event_data)?;
                Ok(Some(PinpetEvent::TokenCreated(TokenCreatedEvent {
                    payer: event.payer.to_string(),
                    mint_account: event.mint_account.to_string(),
                    curve_account: event.curve_account.to_string(),
                    pool_token_account: event.pool_token_account.to_string(),
                    pool_sol_account: event.pool_sol_account.to_string(),
                    fee_recipient: event.fee_recipient.to_string(),
                    base_fee_recipient: event.base_fee_recipient.to_string(),
                    params_account: event.params_account.to_string(),
                    swap_fee: event.swap_fee,
                    borrow_fee: event.borrow_fee,
                    fee_discount_flag: event.fee_discount_flag,
                    name: event.name,
                    symbol: event.symbol,
                    uri: event.uri,
                    up_orderbook: event.up_orderbook.to_string(),
                    down_orderbook: event.down_orderbook.to_string(),
                    latest_price: event.latest_price,
                    timestamp,
                    signature: signature.to_string(),
                    slot,
                })))
            }
            BUY_SELL_EVENT_DISCRIMINATOR => {
                debug!("解析BuySell事件 / Parsing BuySell event, data_len={}", event_data.len());
                let event = BuySellRaw::try_from_slice(event_data)
                    .map_err(|e| anyhow::anyhow!("BuySell解析失败: {}, data_len={}", e, event_data.len()))?;
                Ok(Some(PinpetEvent::BuySell(BuySellEvent {
                    payer: event.payer.to_string(),
                    mint_account: event.mint_account.to_string(),
                    is_buy: event.is_buy,
                    token_amount: event.token_amount,
                    sol_amount: event.sol_amount,
                    latest_price: event.latest_price,
                    liquidate_indices: event.liquidate_indices,
                    timestamp,
                    signature: signature.to_string(),
                    slot,
                })))
            }
            LONG_SHORT_EVENT_DISCRIMINATOR => {
                debug!("解析LongShort事件 / Parsing LongShort event, data_len={}", event_data.len());
                let event = LongShortRaw::try_from_slice(event_data)
                    .map_err(|e| anyhow::anyhow!("LongShort解析失败: {}, data_len={}", e, event_data.len()))?;
                Ok(Some(PinpetEvent::LongShort(LongShortEvent {
                    payer: event.payer.to_string(),
                    mint_account: event.mint_account.to_string(),
                    order_id: event.order_id,
                    order_index: event.order_index,
                    latest_price: event.latest_price,
                    open_price: event.open_price,
                    order_type: event.order_type,
                    lock_lp_start_price: event.lock_lp_start_price,
                    lock_lp_end_price: event.lock_lp_end_price,
                    lock_lp_sol_amount: event.lock_lp_sol_amount,
                    lock_lp_token_amount: event.lock_lp_token_amount,
                    start_time: event.start_time,
                    end_time: event.end_time,
                    margin_sol_amount: event.margin_sol_amount,
                    borrow_amount: event.borrow_amount,
                    position_asset_amount: event.position_asset_amount,
                    borrow_fee: event.borrow_fee,
                    liquidate_indices: event.liquidate_indices,
                    timestamp,
                    signature: signature.to_string(),
                    slot,
                })))
            }
            FULL_CLOSE_EVENT_DISCRIMINATOR => {
                debug!("解析FullClose事件 / Parsing FullClose event, data_len={}", event_data.len());
                let event = FullCloseRaw::try_from_slice(event_data)
                    .map_err(|e| anyhow::anyhow!("FullClose解析失败: {}, data_len={}", e, event_data.len()))?;
                Ok(Some(PinpetEvent::FullClose(FullCloseEvent {
                    payer: event.payer.to_string(),
                    user_sol_account: event.user_sol_account.to_string(),
                    mint_account: event.mint_account.to_string(),
                    is_close_long: event.is_close_long,
                    final_token_amount: event.final_token_amount,
                    final_sol_amount: event.final_sol_amount,
                    user_close_profit: event.user_close_profit,
                    latest_price: event.latest_price,
                    order_id: event.order_id,
                    order_index: event.order_index,
                    liquidate_indices: event.liquidate_indices,
                    timestamp,
                    signature: signature.to_string(),
                    slot,
                })))
            }
            PARTIAL_CLOSE_EVENT_DISCRIMINATOR => {
                debug!("解析PartialClose事件 / Parsing PartialClose event, data_len={}", event_data.len());
                let event = PartialCloseRaw::try_from_slice(event_data)
                    .map_err(|e| anyhow::anyhow!("PartialClose解析失败: {}, data_len={}", e, event_data.len()))?;
                Ok(Some(PinpetEvent::PartialClose(PartialCloseEvent {
                    payer: event.payer.to_string(),
                    user_sol_account: event.user_sol_account.to_string(),
                    mint_account: event.mint_account.to_string(),
                    is_close_long: event.is_close_long,
                    final_token_amount: event.final_token_amount,
                    final_sol_amount: event.final_sol_amount,
                    user_close_profit: event.user_close_profit,
                    latest_price: event.latest_price,
                    order_id: event.order_id,
                    order_index: event.order_index,
                    order_type: event.order_type,
                    user: event.user.to_string(),
                    lock_lp_start_price: event.lock_lp_start_price,
                    lock_lp_end_price: event.lock_lp_end_price,
                    lock_lp_sol_amount: event.lock_lp_sol_amount,
                    lock_lp_token_amount: event.lock_lp_token_amount,
                    start_time: event.start_time,
                    end_time: event.end_time,
                    margin_sol_amount: event.margin_sol_amount,
                    borrow_amount: event.borrow_amount,
                    position_asset_amount: event.position_asset_amount,
                    borrow_fee: event.borrow_fee,
                    realized_sol_amount: event.realized_sol_amount,
                    liquidate_indices: event.liquidate_indices,
                    timestamp,
                    signature: signature.to_string(),
                    slot,
                })))
            }
            MILESTONE_DISCOUNT_EVENT_DISCRIMINATOR => {
                debug!("解析MilestoneDiscount事件 / Parsing MilestoneDiscount event");
                let event = MilestoneDiscountRaw::try_from_slice(event_data)?;
                Ok(Some(PinpetEvent::MilestoneDiscount(MilestoneDiscountEvent {
                    payer: event.payer.to_string(),
                    mint_account: event.mint_account.to_string(),
                    curve_account: event.curve_account.to_string(),
                    swap_fee: event.swap_fee,
                    borrow_fee: event.borrow_fee,
                    fee_discount_flag: event.fee_discount_flag,
                    timestamp,
                    signature: signature.to_string(),
                    slot,
                })))
            }
            _ => {
                debug!("未知事件判别器 / Unknown event discriminator: {:?}", discriminator);
                Ok(None)
            }
        }
    }
}

// Borsh反序列化的原始结构 / Raw structures for Borsh deserialization
#[derive(BorshDeserialize)]
struct TokenCreatedRaw {
    payer: Pubkey,
    mint_account: Pubkey,
    curve_account: Pubkey,
    pool_token_account: Pubkey,
    pool_sol_account: Pubkey,
    fee_recipient: Pubkey,
    base_fee_recipient: Pubkey,
    params_account: Pubkey,
    swap_fee: u16,
    borrow_fee: u16,
    fee_discount_flag: u8,
    name: String,
    symbol: String,
    uri: String,
    up_orderbook: Pubkey,
    down_orderbook: Pubkey,
    latest_price: u128,
}

#[derive(BorshDeserialize)]
struct BuySellRaw {
    payer: Pubkey,
    mint_account: Pubkey,
    is_buy: bool,
    token_amount: u64,
    sol_amount: u64,
    latest_price: u128,
    liquidate_indices: Vec<u16>,
}

#[derive(BorshDeserialize)]
struct LongShortRaw {
    payer: Pubkey,
    mint_account: Pubkey,
    order_id: u64,
    order_index: u16,
    latest_price: u128,
    open_price: u128,
    order_type: u8,
    lock_lp_start_price: u128,
    lock_lp_end_price: u128,
    lock_lp_sol_amount: u64,
    lock_lp_token_amount: u64,
    start_time: u32,
    end_time: u32,
    margin_sol_amount: u64,
    borrow_amount: u64,
    position_asset_amount: u64,
    borrow_fee: u16,
    liquidate_indices: Vec<u16>,
}

#[derive(BorshDeserialize)]
struct FullCloseRaw {
    payer: Pubkey,
    user_sol_account: Pubkey,
    mint_account: Pubkey,
    is_close_long: bool,
    final_token_amount: u64,
    final_sol_amount: u64,
    user_close_profit: u64,
    latest_price: u128,
    order_id: u64,
    order_index: u16,  // 添加缺失的字段 / Add missing field
    liquidate_indices: Vec<u16>,
}

#[derive(BorshDeserialize)]
struct PartialCloseRaw {
    payer: Pubkey,
    user_sol_account: Pubkey,
    mint_account: Pubkey,
    is_close_long: bool,
    final_token_amount: u64,
    final_sol_amount: u64,
    user_close_profit: u64,
    latest_price: u128,
    order_id: u64,
    order_index: u16,
    order_type: u8,
    user: Pubkey,
    lock_lp_start_price: u128,
    lock_lp_end_price: u128,
    lock_lp_sol_amount: u64,
    lock_lp_token_amount: u64,
    start_time: u32,
    end_time: u32,
    margin_sol_amount: u64,
    borrow_amount: u64,
    position_asset_amount: u64,
    borrow_fee: u16,
    realized_sol_amount: u64,
    liquidate_indices: Vec<u16>,
}

#[derive(BorshDeserialize)]
struct MilestoneDiscountRaw {
    payer: Pubkey,
    mint_account: Pubkey,
    curve_account: Pubkey,
    swap_fee: u16,
    borrow_fee: u16,
    fee_discount_flag: u8,
}

/// 清算事件 (服务端合成事件) / Liquidation event (server-side synthetic event)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LiquidateEvent {
    pub payer: String,                    // 清算触发者 / Liquidation initiator
    pub user_sol_account: String,         // 被清算用户SOL账户 / Liquidated user's SOL account
    pub mint_account: String,             // 代币mint地址 / Token mint address
    pub is_close_long: bool,              // 是否为平多 / Is closing long (true) or short (false)
    pub final_token_amount: u64,         // 最终token数量 / Final token amount (u64)
    pub final_sol_amount: u64,           // 最终SOL数量 / Final SOL amount (u64, lamports)
    pub order_index: u16,                // 订单索引 / Order index (u16)
    #[schema(value_type = String)]
    pub timestamp: DateTime<Utc>,        // ISO 8601格式时间戳 / ISO 8601 timestamp
    pub signature: String,               // 触发清算的交易签名 / Transaction signature that triggered liquidation
    pub slot: u64,                       // 区块槽位 / Block slot
}