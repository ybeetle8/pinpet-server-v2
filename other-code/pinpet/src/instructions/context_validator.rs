// 上下文验证模块 - 用于验证交易上下文中的账户约束
use {
    crate::error::ErrorCode,
    crate::instructions::pdas::{BorrowingBondingCurve},
    crate::instructions::contexts::{TradeBuySell, TradeClose, TradeLongShort},
    anchor_lang::prelude::*,
};

/// 验证手续费接收账户地址是否匹配
///
/// # 参数
/// * `curve_account` - 借贷流动池账户
/// * `fee_recipient_account` - 合作伙伴手续费接收账户
/// * `base_fee_recipient_account` - 技术提供方基础手续费接收账户
///
/// # 返回值
/// * `Result<()>` - 验证成功返回Ok，失败返回错误
pub fn validate_fee_recipient_accounts(
    curve_account: &Account<BorrowingBondingCurve>,
    fee_recipient_account: &UncheckedAccount,
    base_fee_recipient_account: &UncheckedAccount,
) -> Result<()> {
    // 验证合作伙伴手续费接收账户
    require!(
        curve_account.fee_recipient == fee_recipient_account.key(),
        ErrorCode::InvalidFeeRecipientAccount
    );

    // 验证技术提供方基础手续费接收账户
    require!(
        curve_account.base_fee_recipient == base_fee_recipient_account.key(),
        ErrorCode::InvalidFeeRecipientAccount
    );

    Ok(())
}



/// 验证TradeLongShort上下文中的账户约束
///
/// # 参数
/// * `ctx` - TradeLongShort上下文
///
/// # 返回值
/// * `Result<()>` - 验证成功返回Ok，失败返回错误
pub fn validate_trade_long_short_context(ctx: &Context<TradeLongShort>) -> Result<()> {
    // 验证手续费接收账户
    validate_fee_recipient_accounts(
        &ctx.accounts.curve_account,
        &ctx.accounts.fee_recipient_account,
        &ctx.accounts.base_fee_recipient_account,
    )?;


    // PDA账户所有权验证
    require!(
        ctx.accounts.up_orderbook.to_account_info().owner == ctx.program_id,
        ErrorCode::InvalidAccountOwner
    );
    require!(
        ctx.accounts.down_orderbook.to_account_info().owner == ctx.program_id,
        ErrorCode::InvalidAccountOwner
    );

    // 订单簿地址验证
    require!(
        ctx.accounts.curve_account.up_orderbook == ctx.accounts.up_orderbook.key(),
        ErrorCode::InvalidOrderMintAddress
    );
    require!(
        ctx.accounts.curve_account.down_orderbook == ctx.accounts.down_orderbook.key(),
        ErrorCode::InvalidOrderMintAddress
    );

    // 代币账户所有权验证
    require!(
        ctx.accounts.pool_token_account.owner == ctx.accounts.curve_account.key(),
        ErrorCode::InvalidAccountOwner
    );


    Ok(())
}

/// 验证TradeBuySell上下文中的账户约束
///
/// # 参数
/// * `ctx` - TradeBuySell上下文
///
/// # 返回值
/// * `Result<()>` - 验证成功返回Ok，失败返回错误
pub fn validate_trade_buy_sell_context(ctx: &Context<TradeBuySell>) -> Result<()> {
    // 1. 验证手续费接收账户
    validate_fee_recipient_accounts(
        &ctx.accounts.curve_account,
        &ctx.accounts.fee_recipient_account,
        &ctx.accounts.base_fee_recipient_account,
    )?;

    // PDA账户所有权验证
    require!(
        ctx.accounts.up_orderbook.to_account_info().owner == ctx.program_id,
        ErrorCode::InvalidAccountOwner
    );
    require!(
        ctx.accounts.down_orderbook.to_account_info().owner == ctx.program_id,
        ErrorCode::InvalidAccountOwner
    );

    // 代币账户所有权验证
    require!(
        ctx.accounts.pool_token_account.owner == ctx.accounts.curve_account.key(),
        ErrorCode::InvalidAccountOwner
    );
    require!(
        ctx.accounts.user_token_account.owner == ctx.accounts.payer.key(),
        ErrorCode::InvalidAccountOwner
    );

    // 订单簿地址验证
    require!(
        ctx.accounts.curve_account.up_orderbook == ctx.accounts.up_orderbook.key(),
        ErrorCode::InvalidOrderMintAddress
    );
    require!(
        ctx.accounts.curve_account.down_orderbook == ctx.accounts.down_orderbook.key(),
        ErrorCode::InvalidOrderMintAddress
    );


    Ok(())
}

/// 验证TradeClose上下文中的账户约束
///
/// # 参数
/// * `ctx` - TradeClose上下文
///
/// # 返回值
/// * `Result<()>` - 验证成功返回Ok，失败返回错误
pub fn validate_trade_close_context(ctx: &Context<TradeClose>) -> Result<()> {
    // 验证手续费接收账户
    validate_fee_recipient_accounts(
        &ctx.accounts.curve_account,
        &ctx.accounts.fee_recipient_account,
        &ctx.accounts.base_fee_recipient_account,
    )?;

    // PDA账户所有权验证
    require!(
        ctx.accounts.up_orderbook.to_account_info().owner == ctx.program_id,
        ErrorCode::InvalidAccountOwner
    );
    require!(
        ctx.accounts.down_orderbook.to_account_info().owner == ctx.program_id,
        ErrorCode::InvalidAccountOwner
    );

    // 代币账户所有权验证
    require!(
        ctx.accounts.pool_token_account.owner == ctx.accounts.curve_account.key(),
        ErrorCode::InvalidAccountOwner
    );

    // 订单簿地址验证
    require!(
        ctx.accounts.curve_account.up_orderbook == ctx.accounts.up_orderbook.key(),
        ErrorCode::InvalidOrderMintAddress
    );
    require!(
        ctx.accounts.curve_account.down_orderbook == ctx.accounts.down_orderbook.key(),
        ErrorCode::InvalidOrderMintAddress
    );



    Ok(())
}
