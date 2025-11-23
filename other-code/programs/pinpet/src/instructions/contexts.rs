// 指令上下文 - Instruction Contexts
use {
    anchor_lang::prelude::*,
    anchor_spl::token::{Mint, Token, TokenAccount},
    anchor_spl::associated_token::AssociatedToken,
    anchor_spl::metadata::Metadata,
    // 导入 Metaplex Token Metadata v5.1.1 的核心组件
    mpl_token_metadata::accounts::Metadata as MetaplexMetadata,

    crate::error::ErrorCode,
    super::pdas::{Admin, Params, BorrowingBondingCurve, TradeCooldown},
    super::structs::OrderBook,
};

// Admin 账户更新所需的账户结构，支持初始化和更新
#[derive(Accounts)]
pub struct UpdateAdmin<'info> {
    // 超级管理员账户，必须是交易的签名者
    #[account(mut)]
    pub admin: Signer<'info>,

    // Admin 账户 - 使用PDA派生，全网唯一
    #[account(
        init_if_needed,
        payer = admin,
        space = 8 + Admin::INIT_SPACE,
        seeds = [b"admin"],
        bump,
    )]
    pub admin_account: Account<'info, Admin>,

    // Solana系统程序
    pub system_program: Program<'info, System>,
}

// 合作伙伴参数创建所需的账户结构
#[derive(Accounts)]
pub struct CreateParams<'info> {
    // 合作伙伴账户，必须是交易的签名者
    #[account(mut)]
    pub partner: Signer<'info>,

    // Admin 账户 - 用于获取默认值
    #[account(
        seeds = [b"admin"],
        bump,
    )]
    pub admin_account: Account<'info, Admin>,

    // 合作伙伴参数账户 - 使用合作伙伴地址作为种子，确保一个地址只能创建一个
    #[account(
        init,
        payer = partner,
        space = 8 + Params::INIT_SPACE,
        seeds = [b"params", partner.key().as_ref()],
        bump,
    )]
    pub params: Account<'info, Params>,

    // Solana系统程序
    pub system_program: Program<'info, System>,
}

// 合作伙伴参数更新所需的账户结构
#[derive(Accounts)]
#[instruction(partner_pubkey: Pubkey)]
pub struct UpdateParams<'info> {
    // 超级管理员账户，必须是交易的签名者
    #[account(mut)]
    pub admin: Signer<'info>,

    // Admin 账户 - 用于验证权限
    #[account(
        seeds = [b"admin"],
        bump,
        constraint = admin_account.admin == admin.key() @ ErrorCode::Unauthorized
    )]
    pub admin_account: Account<'info, Admin>,

    // 合作伙伴参数账户 - 要更新的账户
    #[account(
        mut,
        seeds = [b"params", partner_pubkey.as_ref()],
        bump,
    )]
    pub params: Account<'info, Params>,

    // Solana系统程序
    pub system_program: Program<'info, System>,
}

// 定义创建基本代币所需的账户结构
#[derive(Accounts)]
pub struct CreateToken<'info> {
    // 支付交易费用的账户，必须是签名者
    #[account(mut)]
    pub payer: Signer<'info>,

    // 代币铸造账户 - 存储代币的基本信息
    #[account(
        // 初始化账户
        init,
        // 支付账户
        payer = payer,
        // 设置代币精度
        mint::decimals = 6,
        // 设置铸造权限拥有者为curve_account(PDA)
        mint::authority = curve_account.key(),
        // 设置冻结权限拥有者为curve_account(PDA)
        mint::freeze_authority = curve_account.key(),
    )]
    pub mint_account: Box<Account<'info, Mint>>,

    // 借贷流动池账户 - 与mint_account一一对应
    #[account(
        init,
        payer = payer,
        space = 8 + BorrowingBondingCurve::INIT_SPACE,
        seeds = [b"borrowing_curve", mint_account.key().as_ref()],
        bump,
    )]
    pub curve_account: Account<'info, BorrowingBondingCurve>,

    // 流动池代币账户
    #[account(
        init,
        payer = payer,
        token::mint = mint_account,
        token::authority = curve_account,
        seeds = [b"pool_token", mint_account.key().as_ref()],
        bump
    )]
    pub pool_token_account: Box<Account<'info, TokenAccount>>,

    // 流动池SOL账户 - 改为系统程序账户，用于存储原生SOL
    #[account(
        init,
        payer = payer,
        space = 0,
        seeds = [b"pool_sol", mint_account.key().as_ref()],
        bump
    )]
    pub pool_sol_account: AccountInfo<'info>,

    // 做空订单账本 (Up方向)
    #[account(
        init,
        seeds = [b"up_orderbook", mint_account.key().as_ref()],
        payer = payer,
        space = 8 + OrderBook::HEADER_SIZE, // 只分配头部空间，不包含任何槽位
        bump,
    )]
    pub up_orderbook: AccountLoader<'info, OrderBook>,

    // 做多订单账本 (Down方向)
    #[account(
        init,
        seeds = [b"down_orderbook", mint_account.key().as_ref()],
        payer = payer,
        space = 8 + OrderBook::HEADER_SIZE, // 只分配头部空间，不包含任何槽位
        bump,
    )]
    pub down_orderbook: AccountLoader<'info, OrderBook>,

    #[account(
        mut,
        address = MetaplexMetadata::find_pda(&mint_account.key()).0
    )]
    // CHECK: PDA 地址通过 address 约束验证，使用 Metaplex 官方 find_pda 方法
    // Metaplex 元数据账户（使用简洁的 PDA 验证）
    pub metadata: UncheckedAccount<'info>,
    // Metaplex Token Metadata 程序（使用 Anchor 类型安全验证）
    pub metadata_program: Program<'info, Metadata>,

    // 合作伙伴参数账户 - 调用者直接传入正确的PDA地址
    pub params: Account<'info, Params>,

    // SPL代币程序 - 处理代币操作的系统程序
    pub token_program: Program<'info, Token>,
    // Solana系统程序 - 处理基础系统操作
    pub system_program: Program<'info, System>,
    // 租金系统变量 - 用于计算账户所需的租金豁免
    pub rent: Sysvar<'info, Rent>,
}

// 定义交易的上下文结构(保证金交易用这个)
#[derive(Accounts)]
pub struct TradeLongShort<'info> {
    // 支付交易费用的账户，必须是签名者
    #[account(mut)]
    pub payer: Signer<'info>,

    // 代币铸造账户 - 存储代币的基本信息
    #[account(mut)]
    pub mint_account: Box<Account<'info, Mint>>,

    // 借贷流动池账户 - 与mint_account一一对应
    #[account(
        mut,
        seeds = [b"borrowing_curve", mint_account.key().as_ref()],
        bump,
    )]
    pub curve_account: Account<'info, BorrowingBondingCurve>,

    // 流动池代币账户
    #[account(
        mut,
        seeds = [b"pool_token", mint_account.key().as_ref()],
        bump
    )]
    pub pool_token_account: Box<Account<'info, TokenAccount>>,

    // 流动池SOL账户 - 改为系统程序账户
    #[account(
        mut,
        seeds = [b"pool_sol", mint_account.key().as_ref()],
        bump
    )]
    pub pool_sol_account: AccountInfo<'info>,


    // SPL代币程序 - 处理代币操作的系统程序
    pub token_program: Program<'info, Token>,
    // Solana系统程序 - 处理基础系统操作
    pub system_program: Program<'info, System>,
    // 租金系统变量 - 用于计算账户所需的租金豁免
    pub rent: Sysvar<'info, Rent>,

    // 合作伙伴手续费接收账户
    /// CHECK: 合作伙伴手续费接收账户，地址在运行时从curve_account.fee_recipient验证
    #[account(mut)]
    pub fee_recipient_account: UncheckedAccount<'info>,

    // 技术提供方基础手续费接收账户
    /// CHECK: 基础手续费接收账户，地址在运行时从curve_account.base_fee_recipient验证
    #[account(mut)]
    pub base_fee_recipient_account: UncheckedAccount<'info>,

    // 做空订单账本 (Up方向) - 引用创币时创建的PDA
    #[account(
        mut,
        seeds = [b"up_orderbook", mint_account.key().as_ref()],
        bump,
    )]
    pub up_orderbook: AccountLoader<'info, OrderBook>,

    // 做多订单账本 (Down方向) - 引用创币时创建的PDA
    #[account(
        mut,
        seeds = [b"down_orderbook", mint_account.key().as_ref()],
        bump,
    )]
    pub down_orderbook: AccountLoader<'info, OrderBook>,

}

// 定义交易的上下文结构(买卖交易用这个)
#[derive(Accounts)]
pub struct TradeBuySell<'info> {
    // 支付交易费用的账户，必须是签名者
    #[account(mut)]
    pub payer: Signer<'info>,

    // 代币铸造账户 - 存储代币的基本信息
    #[account(mut)]
    pub mint_account: Box<Account<'info, Mint>>,

    // 借贷流动池账户 - 与mint_account一一对应
    #[account(
        mut,
        seeds = [b"borrowing_curve", mint_account.key().as_ref()],
        bump,
    )]
    pub curve_account: Account<'info, BorrowingBondingCurve>,

    // 流动池代币账户
    #[account(
        mut,
        seeds = [b"pool_token", mint_account.key().as_ref()],
        bump
    )]
    pub pool_token_account: Box<Account<'info, TokenAccount>>,

    // 流动池SOL账户 - 改为系统程序账户
    #[account(
        mut,
        seeds = [b"pool_sol", mint_account.key().as_ref()],
        bump
    )]
    pub pool_sol_account: AccountInfo<'info>,

    // 做空订单账本 (Up方向) - 引用创币时创建的PDA
    #[account(
        mut,
        seeds = [b"up_orderbook", mint_account.key().as_ref()],
        bump,
    )]
    pub up_orderbook: AccountLoader<'info, OrderBook>,

    // 做多订单账本 (Down方向) - 引用创币时创建的PDA
    #[account(
        mut,
        seeds = [b"down_orderbook", mint_account.key().as_ref()],
        bump,
    )]
    pub down_orderbook: AccountLoader<'info, OrderBook>,

    // 用户代币账户 - 用于接收买入的代币
    #[account(
        mut,
        associated_token::mint = mint_account,
        associated_token::authority = payer,
    )]
    pub user_token_account: Box<Account<'info, TokenAccount>>,

    // SPL代币程序 - 处理代币操作的系统程序
    pub token_program: Program<'info, Token>,
    // Solana系统程序 - 处理基础系统操作
    pub system_program: Program<'info, System>,
    // 租金系统变量 - 用于计算账户所需的租金豁免
    pub rent: Sysvar<'info, Rent>,
    // Associated Token Program - 处理关联代币账户
    pub associated_token_program: Program<'info, AssociatedToken>,

    // 合作伙伴手续费接收账户
    /// CHECK: 合作伙伴手续费接收账户，地址在运行时从curve_account.fee_recipient验证
    #[account(mut)]
    pub fee_recipient_account: UncheckedAccount<'info>,

    // 技术提供方基础手续费接收账户
    /// CHECK: 基础手续费接收账户，地址在运行时从curve_account.base_fee_recipient验证
    #[account(mut)]
    pub base_fee_recipient_account: UncheckedAccount<'info>,

    /// 交易冷却时间PDA
    /// 首次交易时自动创建，后续交易验证冷却时间
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + TradeCooldown::INIT_SPACE,
        seeds = [
            b"trade_cooldown",
            mint_account.key().as_ref(),
            payer.key().as_ref()
        ],
        bump,
    )]
    pub cooldown: Account<'info, TradeCooldown>,

}

// 定义交易的上下文结构(主动平仓用这个)
#[derive(Accounts)]
#[instruction(unique_seed: u64)]
pub struct TradeClose<'info> {
    // 支付交易费用的账户，必须是签名者
    #[account(mut)]
    pub payer: Signer<'info>,

    // 代币铸造账户 - 存储代币的基本信息
    #[account(mut)]
    pub mint_account: Box<Account<'info, Mint>>,

    // 借贷流动池账户 - 与mint_account一一对应
    #[account(
        mut,
        seeds = [b"borrowing_curve", mint_account.key().as_ref()],
        bump,
    )]
    pub curve_account: Account<'info, BorrowingBondingCurve>,

    // 流动池代币账户
    #[account(
        mut,
        seeds = [b"pool_token", mint_account.key().as_ref()],
        bump
    )]
    pub pool_token_account: Box<Account<'info, TokenAccount>>,

    // 流动池SOL账户 - 改为系统程序账户
    #[account(
        mut,
        seeds = [b"pool_sol", mint_account.key().as_ref()],
        bump
    )]
    pub pool_sol_account: AccountInfo<'info>,

    // close_order 的开仓用户SOL账户 - 用于接收SOL（保证金返还或平仓收益）
    /// CHECK: 用户SOL账户，地址在运行时从close_order.user验证
    #[account(mut)]
    pub user_sol_account: UncheckedAccount<'info>,

    // SPL代币程序 - 处理代币操作的系统程序
    pub token_program: Program<'info, Token>,
    // Solana系统程序 - 处理基础系统操作
    pub system_program: Program<'info, System>,
    // 租金系统变量 - 用于计算账户所需的租金豁免
    pub rent: Sysvar<'info, Rent>,

    // 合作伙伴手续费接收账户
    /// CHECK: 合作伙伴手续费接收账户，地址在运行时从curve_account.fee_recipient验证
    #[account(mut)]
    pub fee_recipient_account: UncheckedAccount<'info>,

    // 技术提供方基础手续费接收账户
    /// CHECK: 基础手续费接收账户，地址在运行时从curve_account.base_fee_recipient验证
    #[account(mut)]
    pub base_fee_recipient_account: UncheckedAccount<'info>,

    // 做空订单账本 (Up方向) - 引用创币时创建的PDA
    #[account(
        mut,
        seeds = [b"up_orderbook", mint_account.key().as_ref()],
        bump,
    )]
    pub up_orderbook: AccountLoader<'info, OrderBook>,

    // 做多订单账本 (Down方向) - 引用创币时创建的PDA
    #[account(
        mut,
        seeds = [b"down_orderbook", mint_account.key().as_ref()],
        bump,
    )]
    pub down_orderbook: AccountLoader<'info, OrderBook>,

}

// 手动关闭 TradeCooldown PDA 的上下文结构
#[derive(Accounts)]
pub struct CloseCooldown<'info> {
    /// 发起关闭请求的用户(必须是PDA的owner)
    #[account(mut)]
    pub payer: Signer<'info>,

    /// 代币mint地址
    pub mint_account: Box<Account<'info, Mint>>,

    /// 用户的token账户(用于验证余额)
    #[account(
        associated_token::mint = mint_account,
        associated_token::authority = payer,
    )]
    pub user_token_account: Box<Account<'info, TokenAccount>>,

    /// 要关闭的TradeCooldown PDA
    #[account(
        mut,
        close = payer,  // 租金返还给payer
        seeds = [
            b"trade_cooldown",
            mint_account.key().as_ref(),
            payer.key().as_ref()
        ],
        bump = cooldown.bump,
    )]
    pub cooldown: Account<'info, TradeCooldown>,

    // SPL代币程序
    pub token_program: Program<'info, Token>,
    // AssociatedToken程序
    pub associated_token_program: Program<'info, AssociatedToken>,
    // 系统程序
    pub system_program: Program<'info, System>,
}

