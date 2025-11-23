// 其他结构体 - Other Structures
use anchor_lang::prelude::*;

// ============================================================================
// OrderBook 账户数据结构
// ============================================================================

/// 订单账本 Header 结构
/// 用于 up_orderbook 和 down_orderbook
#[account(zero_copy(unsafe))]
#[repr(C)]
pub struct OrderBook {
    /// 版本号（用于未来升级，当前版本：1） - 1 byte
    pub version: u8,

    /// 订单类型（1=做多/down, 2=做空/up） - 1 byte
    pub order_type: u8,

    /// PDA bump - 1 byte
    pub bump: u8,

    /// 保留字段（对齐到 8 字节边界） - 5 bytes
    pub _padding1: [u8; 5],

    /// 协议管理员（有权执行扩容/缩容） - 32 bytes
    pub authority: Pubkey,

    /// 订单 ID 计数器（用于分配唯一订单 ID，单调递增） - 8 bytes
    pub order_id_counter: u64,

    /// 账本创建时间戳（Unix timestamp，秒） - 4 bytes
    pub created_at: u32,

    /// 最后修改时间戳（Unix timestamp，秒） - 4 bytes
    pub last_modified: u32,

    /// 总容量（最大槽位数限制） - 4 bytes
    pub total_capacity: u32,

    /// 链表头索引（第一个订单） - 2 bytes
    pub head: u16,

    /// 链表尾索引（最后一个订单） - 2 bytes
    pub tail: u16,

    /// 当前订单总数（也是下一个插入的索引） - 2 bytes
    pub total: u16,

    /// 保留字段（对齐到 8 字节边界） - 2 bytes
    pub _padding2: u16,

    /// 预留字段（用于未来扩展，如 migration_pda 等） - 32 bytes
    pub reserved: [u8; 32],

    // 注意: 动态订单槽位数组通过 realloc 调整
    // 实际存储: MarginOrder[total_capacity]
    // 通过内存偏移访问，不在这里声明
}

impl OrderBook {
    /// 当前版本号
    pub const CURRENT_VERSION: u8 = 1;

    /// Header 大小: (1+1+1+5) + 32 + 8 + (4+4) + (4+4+4+4) + 32 = 104 bytes
    pub const HEADER_SIZE: usize = std::mem::size_of::<OrderBook>();

    /// 最大容量 (10MB / 192 bytes ≈ 54,612, 保守取 54,000)
    pub const MAX_CAPACITY: u32 = 52_000;

    /// 计算账户总大小
    pub fn account_size(capacity: u32) -> usize {
        8 + Self::HEADER_SIZE + (capacity as usize) * MarginOrder::SIZE
    }
}

/// 保证金订单槽位结构
/// 使用 zero-copy 和 bytemuck 进行高效内存操作
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MarginOrder {
    // ========== 32-byte 对齐字段 (Pubkey) ==========
    /// 开仓用户 - 32 bytes
    pub user: Pubkey,

    // ========== 16-byte 对齐字段 (u128) ==========
    /// 锁定流动池区间开始价 (Q64.64 格式) - 16 bytes
    pub lock_lp_start_price: u128,

    /// 锁定流动池区间结束价 (Q64.64 格式) - 16 bytes
    pub lock_lp_end_price: u128,

    /// 开仓价 (Q64.64 格式，开仓时设置，永远不会再变) - 16 bytes
    pub open_price: u128,

    // ========== 8-byte 对齐字段 (u64) ==========
    /// 订单唯一标识符（全局递增，由 OrderBook.order_id_counter 分配） - 8 bytes
    pub order_id: u64,

    /// 锁定流动池区间 SOL 数量 (精确值，lamports) - 8 bytes
    pub lock_lp_sol_amount: u64,

    /// 锁定流动池区间 Token 数量 (精确值，最小单位) - 8 bytes
    pub lock_lp_token_amount: u64,

    /// 到后个节点,流动池区间 SOL 数量 - 8 bytes
    pub next_lp_sol_amount: u64,

    /// 到后个节点,流动池区间 Token 数量 - 8 bytes
    pub next_lp_token_amount: u64,

    /// 初始保证金 SOL 数量 (主要作为记录用，不参与计算) - 8 bytes
    pub margin_init_sol_amount: u64,

    /// 保证金 SOL 数量 - 8 bytes
    pub margin_sol_amount: u64,

    /// 贷款数量: 如果是做多则借出 SOL，如果是做空则借出 Token - 8 bytes
    pub borrow_amount: u64,

    /// 当前持仓币的数量 (做空时是 SOL，做多时是 Token) - 8 bytes
    /// 注意: 做多时这值完全等于 lock_lp_token_amount
    pub position_asset_amount: u64,

    /// 已实现的 SOL 利润 - 8 bytes
    pub realized_sol_amount: u64,

    // ========== 4-byte 对齐字段 (u32) ==========
    /// 订单版本号（每次更新时递增） - 4 bytes
    pub version: u32,

    /// 订单开始时间戳 (Unix timestamp, 秒) - 4 bytes
    pub start_time: u32,

    /// 贷款到期时间戳 (Unix timestamp, 秒)，到期后可被任何用户平仓 - 4 bytes
    pub end_time: u32,

    // ========== 2-byte 对齐字段 (u16) ==========
    /// 指向下一个订单的槽位索引 - 2 bytes
    pub next_order: u16,

    /// 指向上一个订单的槽位索引 - 2 bytes
    pub prev_order: u16,

    /// 保证金交易手续费 (基点, bps) - 2 bytes
    /// 需要记录，因为手续费基数可能变化（但同一订单不能变化）
    /// 例如: 50 = 0.5%, 100 = 1%
    pub borrow_fee: u16,

    // ========== 1-byte 对齐字段 (u8) ==========
    /// 订单类型: 1=做多(Down方向) 2=做空(Up方向) - 1 byte
    pub order_type: u8,

    /// 保留字段（对齐到结构体 32-byte 边界，bytemuck::Pod 要求无 padding） - 13 bytes
    pub _padding: [u8; 13],
}


impl MarginOrder {
    /// 槽位大小: 使用 size_of 自动计算
    pub const SIZE: usize = std::mem::size_of::<MarginOrder>();

    // /// 检查订单是否过期（可被任何用户平仓）
    // pub fn is_expired(&self, current_timestamp: u32) -> bool {
    //     self.end_time > 0 && current_timestamp >= self.end_time
    // }

    // /// 重置槽位为空闲状态
    // pub fn reset(&mut self) {
    //     *self = Self::zeroed();
    //     self.next_order = u16::MAX;
    //     self.prev_order = u16::MAX;
    // }

    // /// 初始化为空闲槽位
    // pub fn init_free() -> Self {
    //     let mut slot = Self::zeroed();
    //     slot.next_order = u16::MAX;
    //     slot.prev_order = u16::MAX;
    //     slot
    // }
}
