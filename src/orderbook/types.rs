// OrderBook 数据结构定义
// OrderBook Data Structure Definitions

use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use utoipa::ToSchema;

/// OrderBook 头部元数据
/// OrderBook header metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookHeader {
    /// 版本号(用于未来升级,当前版本: 1)
    /// Version number (for future upgrades, current version: 1)
    pub version: u8,

    /// 订单类型(1=做多/down, 2=做空/up)
    /// Order type (1=long/down, 2=short/up)
    pub order_type: u8,

    /// 协议管理员(有权执行扩容/缩容)
    /// Authority (has permission to resize)
    pub authority: String,

    /// 订单 ID 计数器(用于分配唯一订单 ID,单调递增)
    /// Order ID counter (for assigning unique order IDs, monotonically increasing)
    pub order_id_counter: u64,

    /// 账本创建时间戳(Unix timestamp,秒)
    /// Created timestamp (Unix timestamp, seconds)
    pub created_at: u32,

    /// 最后修改时间戳(Unix timestamp,秒)
    /// Last modified timestamp (Unix timestamp, seconds)
    pub last_modified: u32,

    /// 总容量(最大槽位数限制)
    /// Total capacity (maximum slot count limit)
    pub total_capacity: u32,

    /// 链表头索引(第一个订单)
    /// Head index (first order)
    pub head: u16,

    /// 链表尾索引(最后一个订单)
    /// Tail index (last order)
    pub tail: u16,

    /// 当前订单总数(也是下一个插入的索引)
    /// Current order count (also the next insert index)
    pub total: u16,
}

impl OrderBookHeader {
    /// 当前版本号
    /// Current version number
    pub const CURRENT_VERSION: u8 = 1;

    /// 最大容量(考虑到千万级订单,这里设置为 u16::MAX)
    /// Maximum capacity (considering tens of millions of orders, set to u16::MAX)
    pub const MAX_CAPACITY: u32 = u16::MAX as u32; // 65535

    /// 创建新的 OrderBook header
    /// Create new OrderBook header
    pub fn new(order_type: u8, authority: String) -> Self {
        let now = chrono::Utc::now().timestamp() as u32;
        Self {
            version: Self::CURRENT_VERSION,
            order_type,
            authority,
            order_id_counter: 0,
            created_at: now,
            last_modified: now,
            total_capacity: 0,
            head: u16::MAX, // 空链表 / Empty linked list
            tail: u16::MAX, // 空链表 / Empty linked list
            total: 0,
        }
    }

    /// 序列化为字节
    /// Serialize to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// 从字节反序列化
    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

/// 保证金订单槽位结构
/// Margin order slot structure
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MarginOrder {
    // ========== 32-byte 对齐字段 (Pubkey) ==========
    /// 开仓用户
    /// User who opened the position
    pub user: String,

    // ========== 16-byte 对齐字段 (u128) ==========
    /// 锁定流动池区间开始价 (Q64.64 格式)
    /// Locked LP range start price (Q64.64 format)
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub lock_lp_start_price: u128,

    /// 锁定流动池区间结束价 (Q64.64 格式)
    /// Locked LP range end price (Q64.64 format)
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub lock_lp_end_price: u128,

    /// 开仓价 (Q64.64 格式,开仓时设置,永远不会再变)
    /// Open price (Q64.64 format, set at opening, never changes)
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub open_price: u128,

    // ========== 8-byte 对齐字段 (u64) ==========
    /// 订单唯一标识符(全局递增,由 OrderBook.order_id_counter 分配)
    /// Order unique identifier (globally increasing, assigned by OrderBook.order_id_counter)
    pub order_id: u64,

    /// 锁定流动池区间 SOL 数量 (精确值,lamports)
    /// Locked LP range SOL amount (exact value, lamports)
    pub lock_lp_sol_amount: u64,

    /// 锁定流动池区间 Token 数量 (精确值,最小单位)
    /// Locked LP range Token amount (exact value, smallest unit)
    pub lock_lp_token_amount: u64,

    /// 到后个节点,流动池区间 SOL 数量
    /// Next node LP range SOL amount
    pub next_lp_sol_amount: u64,

    /// 到后个节点,流动池区间 Token 数量
    /// Next node LP range Token amount
    pub next_lp_token_amount: u64,

    /// 初始保证金 SOL 数量 (主要作为记录用,不参与计算)
    /// Initial margin SOL amount (mainly for record, not used in calculations)
    pub margin_init_sol_amount: u64,

    /// 保证金 SOL 数量
    /// Margin SOL amount
    pub margin_sol_amount: u64,

    /// 贷款数量: 如果是做多则借出 SOL,如果是做空则借出 Token
    /// Borrow amount: SOL for long, Token for short
    pub borrow_amount: u64,

    /// 当前持仓币的数量 (做空时是 SOL,做多时是 Token)
    /// Current position asset amount (SOL for short, Token for long)
    /// 注意: 做多时这值完全等于 lock_lp_token_amount
    /// Note: For long positions, this equals lock_lp_token_amount
    pub position_asset_amount: u64,

    /// 已实现的 SOL 利润
    /// Realized SOL profit
    pub realized_sol_amount: u64,

    // ========== 4-byte 对齐字段 (u32) ==========
    /// 订单版本号(每次更新时递增)
    /// Order version number (incremented on each update)
    pub version: u32,

    /// 订单开始时间戳 (Unix timestamp, 秒)
    /// Order start timestamp (Unix timestamp, seconds)
    pub start_time: u32,

    /// 贷款到期时间戳 (Unix timestamp, 秒),到期后可被任何用户平仓
    /// Loan expiry timestamp (Unix timestamp, seconds), can be closed by anyone after expiry
    pub end_time: u32,

    // ========== 2-byte 对齐字段 (u16) ==========
    /// 指向下一个订单的槽位索引
    /// Next order slot index
    pub next_order: u16,

    /// 指向上一个订单的槽位索引
    /// Previous order slot index
    pub prev_order: u16,

    /// 保证金交易手续费 (基点, bps)
    /// Margin trading fee (basis points, bps)
    /// 需要记录,因为手续费基数可能变化(但同一订单不能变化)
    /// Need to record as fee basis may change (but cannot change for same order)
    /// 例如: 50 = 0.5%, 100 = 1%
    /// Example: 50 = 0.5%, 100 = 1%
    pub borrow_fee: u16,

    // ========== 1-byte 对齐字段 (u8) ==========
    /// 订单类型: 1=做多(Down方向) 2=做空(Up方向)
    /// Order type: 1=Long(Down direction) 2=Short(Up direction)
    pub order_type: u8,
}

impl MarginOrder {
    /// 序列化为字节
    /// Serialize to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// 从字节反序列化
    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

/// 订单更新数据(只包含可更新的字段)
/// Order update data (only updatable fields)
/// 不可更新字段: user, order_id, start_time, order_type, next_order, prev_order (链表指针由系统管理)
/// Non-updatable fields: user, order_id, start_time, order_type, next_order, prev_order (managed by system)
#[derive(Clone, Copy, Default, Debug)]
pub struct MarginOrderUpdateData {
    pub lock_lp_start_price: Option<u128>,
    pub lock_lp_end_price: Option<u128>,
    pub lock_lp_sol_amount: Option<u64>,
    pub lock_lp_token_amount: Option<u64>,
    pub next_lp_sol_amount: Option<u64>,
    pub next_lp_token_amount: Option<u64>,
    pub end_time: Option<u32>,
    pub margin_init_sol_amount: Option<u64>,
    pub margin_sol_amount: Option<u64>,
    pub borrow_amount: Option<u64>,
    pub position_asset_amount: Option<u64>,
    pub borrow_fee: Option<u16>,
    pub open_price: Option<u128>,
    pub realized_sol_amount: Option<u64>,
}

/// 遍历结果
/// Traversal result
#[derive(Debug, Clone, Copy)]
pub struct TraversalResult {
    /// 本次处理的订单数量
    /// Number of orders processed in this traversal
    pub processed: u32,

    /// 下一个待处理的索引(u16::MAX 表示已完成)
    /// Next index to process (u16::MAX indicates completion)
    pub next: u16,

    /// 是否已遍历完成
    /// Whether traversal is complete
    pub done: bool,
}

// ==================== 已关闭订单相关数据结构 / Closed Order Related Structures ====================

/// 已关闭订单快照 - 完整数据
/// Closed order snapshot - complete data
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ClosedOrderRecord {
    /// Token mint 地址
    /// Token mint address
    pub mint: String,

    /// 订单方向: "up"(做空) 或 "dn"(做多)
    /// Order direction: "up"(short) or "dn"(long)
    pub direction: String,

    /// 订单完整快照(删除时保存)
    /// Complete order snapshot (saved at deletion)
    pub order: MarginOrder,

    /// 关闭时的额外信息
    /// Additional close-time information
    pub close_info: CloseInfo,
}

/// 订单关闭信息
/// Order close information
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CloseInfo {
    /// 关闭时间戳(Unix timestamp)
    /// Close timestamp (Unix timestamp)
    pub close_timestamp: u32,

    /// 关闭时的价格(u128, 9位小数精度)
    /// Close price (u128, 9 decimal precision)
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub close_price: u128,

    /// 关闭原因 / Close reason:
    /// - 1: 用户主动平仓 / User initiated close
    /// - 2: 强制清算 / Forced liquidation
    /// - 3: 到期自动平仓 / Expired auto-close
    /// - 4: 爆仓清算 / Margin call liquidation
    pub close_reason: u8,

    /// 最终盈亏(SOL,带符号)
    /// Final PnL (SOL, signed)
    /// - 正数: 盈利 / Positive: profit
    /// - 负数: 亏损 / Negative: loss
    pub final_pnl_sol: i64,

    /// 借款费用总计(SOL)
    /// Total borrow fee (SOL)
    pub total_borrow_fee_sol: u64,

    /// 持仓时长(秒)
    /// Position duration (seconds)
    pub position_duration_sec: u32,
}

/// 关闭原因枚举
/// Close reason enum
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum CloseReason {
    /// 用户主动平仓 / User initiated
    UserInitiated = 1,
    /// 强制清算 / Forced liquidation
    ForcedLiquidation = 2,
    /// 到期自动平仓 / Expired
    Expired = 3,
    /// 爆仓清算 / Margin call
    MarginCall = 4,
}
