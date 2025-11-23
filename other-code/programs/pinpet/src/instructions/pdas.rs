// PDA 派生地址 - PDA Accounts
use {
    anchor_lang::prelude::*,
};

// Admin 主要是用来分配超级管理员的
#[account]
#[derive(InitSpace)]
pub struct Admin {
    #[max_len(2)]
    pub default_swap_fee: u16,         // 默认 现货交易手续费swap_fee
    #[max_len(2)]
    pub default_borrow_fee: u16,       // 默认 保证金交易手续费borrow_fee
    #[max_len(4)]
    pub default_borrow_duration: u32,  // 默认借贷最长时时 单位秒
    #[max_len(32)]
    pub base_fee_recipient: Pubkey,      // 基础手续费接收账户
    #[max_len(1)]
    pub default_fee_split: u8,  // 默认的 手续费分配比例值为(0-100), 示例:  80: 运营商拿80% ,技术提供方拿20%
    #[max_len(32)]
    pub admin: Pubkey,              // 超级管理员账户
    #[max_len(1)]
    pub bump: u8,                   // PDA bump
}

// 交易配置数据,
#[account]
#[derive(InitSpace)]
pub struct Params {
    #[max_len(2)]
    pub base_swap_fee: u16,         // 默认 现货交易手续费swap_fee
    #[max_len(2)]
    pub base_borrow_fee: u16,       // 默认 保证金交易手续费borrow_fee
    #[max_len(4)]
    pub base_borrow_duration: u32,  // 默认借贷最长时时 单位秒
    #[max_len(32)]
    pub base_fee_recipient: Pubkey,      // 基础手续费接收账户
    #[max_len(32)]
    pub fee_recipient: Pubkey,      // 合作伙伴手续费接收账户
    #[max_len(1)]
    pub fee_split: u8,  // 手续费分配比例值为(0-100), 示例:  80: 合作伙伴拿80% ,基础手续费方拿20%
    #[max_len(1)]
    pub bump: u8,                   // PDA bump
}

// 定义借贷流动池账户结构
#[account]
#[derive(InitSpace)]
pub struct BorrowingBondingCurve {
    #[max_len(8)]
    pub lp_token_reserve: u64, // 代币在流动池中的数量
    #[max_len(8)]
    pub lp_sol_reserve: u64, // SOL在流动池中的数量
    #[max_len(16)]
    pub price: u128, // 当前价格
    #[max_len(8)]
    pub borrow_token_reserve: u64, // 代币在虚拟借贷池中的数量
    #[max_len(8)]
    pub borrow_sol_reserve: u64, // SOL在虚拟借贷池中的数量
    #[max_len(2)]
    pub swap_fee: u16, // 现货交易手续费（比例乘以10000，所以0.99 = 9900）
    #[max_len(2)]
    pub borrow_fee: u16, // 保证金交易手续费（比例乘以10000，所以0.9975 = 9975）
    #[max_len(1)]
    pub fee_discount_flag: u8, // 手续费折扣标志 0: 原价 1: 5折 2: 2.5折  3: 1.25折
    #[max_len(32)]
    pub base_fee_recipient: Pubkey,      // 技术提供方基础手续费接收账户
    #[max_len(32)]
    pub fee_recipient: Pubkey,      // 合作伙伴手续费接收账户 partner_fee_recipient
    #[max_len(1)]
    pub fee_split: u8,  // 手续费分配比例值为(0-100), 示例:  80: 合作伙伴拿80% ,技术提供方拿20%
    #[max_len(4)]
    pub borrow_duration: u32, // 贷款时长(秒)
    #[max_len(32)]
    pub mint: Pubkey, // 关联的代币铸造账户
    #[max_len(32)]
    pub up_orderbook: Pubkey,  // 做空订单账本地址 (新)
    #[max_len(32)]
    pub down_orderbook: Pubkey,  // 做多订单账本地址 (新)
    #[max_len(1)]
    pub bump: u8, // PDA bump
}

/// 交易冷却时间PDA
/// 用于防止高频交易和转账绕过攻击
#[account]
#[derive(InitSpace)]
pub struct TradeCooldown {
    /// 最近一次交易的时间戳 (Unix timestamp, 秒) - 4 bytes
    #[max_len(4)]
    pub last_trade_time: u32,

    /// 允许交易的token数量 (上次交易后的余额快照) - 8 bytes
    /// 用于防止用户通过转账绕过冷却时间
    #[max_len(8)]
    pub approval_token_amount: u64,

    /// PDA bump - 1 byte
    #[max_len(1)]
    pub bump: u8,
}
