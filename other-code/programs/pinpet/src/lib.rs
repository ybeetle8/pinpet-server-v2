
use anchor_lang::prelude::*;
// 导入指令模块
mod instructions;
use instructions::*;

// 导入曲线模块
pub mod curve;
//pub mod utils;
pub mod constants;
pub mod error;
pub mod types;

declare_id!("HNaandW3U5sVTsoJaGx61UmX9Siupa6difFY9qRAPXyw");

#[program]
pub mod pinpet {
    use super::*;

    // 初始化函数
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        // msg!("Greetings from: {:?}", ctx.program_id);
        Ok(())
    }

    // 更新 Admin 配置指令
    pub fn update_admin(
        ctx: Context<UpdateAdmin>,
        default_swap_fee: Option<u16>,
        default_borrow_fee: Option<u16>,
        default_borrow_duration: Option<u32>,
        base_fee_recipient: Option<Pubkey>,
        default_fee_split: Option<u8>,
        new_admin: Option<Pubkey>,
    ) -> Result<()> {
        instructions::admin_params::update_admin(
            ctx,
            default_swap_fee,
            default_borrow_fee,
            default_borrow_duration,
            base_fee_recipient,
            default_fee_split,
            new_admin,
        )
    }

    // 创建合作伙伴参数指令
    pub fn create_params(ctx: Context<CreateParams>) -> Result<()> {
        instructions::admin_params::create_params(ctx)
    }

    // 更新合作伙伴参数指令（只有超级管理员可以调用）
    pub fn update_params(
        ctx: Context<UpdateParams>,
        partner_pubkey: Pubkey,
        base_swap_fee: Option<u16>,
        base_borrow_fee: Option<u16>,
        base_borrow_duration: Option<u32>,
        base_fee_recipient: Option<Pubkey>,
        fee_split: Option<u8>,
    ) -> Result<()> {
        instructions::admin_params::update_params(
            ctx,
            partner_pubkey,
            base_swap_fee,
            base_borrow_fee,
            base_borrow_duration,
            base_fee_recipient,
            fee_split,
        )
    }

    // 创建基本代币指令
    pub fn create_token(
        ctx: Context<CreateToken>,
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        instructions::create_token::create_token(ctx, name, symbol, uri)
    }

    // 创建基本代币指令的别名（保持向后兼容）
    pub fn create(
        ctx: Context<CreateToken>,
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        instructions::create_token::create_token(ctx, name, symbol, uri)
    }

    // 保证金做多交易指令
    pub fn long(
        ctx: Context<TradeLongShort>,
        buy_token_amount: u64,          // 希望买入的token数量
        max_sol_amount: u64,            // 愿意给出的最大sol数量
        margin_sol: u64,                // 保证金数量 (SOL)
        close_price: u128,              // 平仓价格
        close_insert_indices: Vec<u16>, // 开仓时插入订单簿的位置索引
    ) -> Result<()> {
        instructions::long_short::long_trade(
            ctx,
            buy_token_amount,
            max_sol_amount,
            margin_sol,
            close_price,
            close_insert_indices,
        )
    }

    // 保证金做空交易指令
    pub fn short(
        ctx: Context<TradeLongShort>,
        borrow_sell_token_amount: u64,  // 希望卖出的token数量
        min_sol_output: u64,            // 卖出后最少得到的sol数量
        margin_sol: u64,                // 保证金数量 (SOL)
        close_price: u128,              // 平仓价格
        close_insert_indices: Vec<u16>, // 开仓时插入订单簿的位置索引
    ) -> Result<()> {
        instructions::long_short::short_trade(
            ctx,
            borrow_sell_token_amount,
            min_sol_output,
            margin_sol,
            close_price,
            close_insert_indices,
        )
    }

    // 现货买入交易指令
    pub fn buy(
        ctx: Context<TradeBuySell>,
        buy_token_amount: u64, // 希望买入的token数量
        max_sol_amount: u64,   // 愿意给出的最大sol数量
    ) -> Result<()> {
        instructions::buy_sell::buy_trade(ctx, buy_token_amount, max_sol_amount)
    }

    // 现货卖出交易指令
    pub fn sell(
        ctx: Context<TradeBuySell>,
        sell_token_amount: u64, // 希望卖出的token数量
        min_sol_output: u64,    // 卖出后最少得到的sol数量
    ) -> Result<()> {
        instructions::buy_sell::sell_trade(ctx, sell_token_amount, min_sol_output)
    }

    /// 批准当前token余额用于交易
    /// 用于以下场景:
    /// 1. 从其他地址转入token后想要立即交易
    /// 2. 重新激活冷却PDA
    pub fn approve_trade(ctx: Context<TradeBuySell>) -> Result<()> {
        // msg!("批准token用于交易");

        // ========== 如果PDA已存在，验证冷却时间 ==========
        if ctx.accounts.cooldown.last_trade_time > 0 {
            instructions::validate_trade_cooldown(&ctx.accounts.cooldown)?;
        }

        // ========== 更新批准额度为当前余额 ==========
        let current_token_balance = ctx.accounts.user_token_account.amount;

        instructions::update_cooldown_record(
            &mut ctx.accounts.cooldown,
            current_token_balance,
            ctx.bumps.cooldown,
        )?;

        // msg!("批准成功: approval_token_amount = {}", current_token_balance);

        Ok(())
    }

    /// 手动关闭TradeCooldown PDA并回收租金
    ///
    /// 使用条件:
    /// 1. 只能关闭自己的PDA(通过seeds验证)
    ///
    /// 使用场景:
    /// - 用户想要回收租金
    /// - 清理不再使用的PDA
    /// - 管理员批量清理过期PDA
    ///
    /// 注意:
    /// - 无需验证代币余额，关闭后可通过approve_trade重新创建
    /// - PDA关闭后，下次buy或approve会自动重新创建
    pub fn close_trade_cooldown(ctx: Context<CloseCooldown>) -> Result<()> {
        // msg!("手动关闭TradeCooldown PDA");

        let token_balance = ctx.accounts.user_token_account.amount;
        // msg!("当前代币余额: {}, PDA关闭成功，租金返还给用户", token_balance);

        // PDA会自动关闭(通过close = payer约束)
        Ok(())
    }

    // 平仓做多交易指令
    pub fn close_long(
        ctx: Context<TradeClose>,
        sell_token_amount: u64,        // 希望卖出的token数量
        min_sol_output: u64,           // 卖出后最少得到的sol数量
        close_order_id: u64,           // 订单的唯一编号
        close_order_indices: Vec<u16>, // 平仓时订的位置索引 (可以有多个,万一订单被移动了,就会自动找第二位置)
    ) -> Result<()> {
        instructions::close_long_short::close_long_trade(
            ctx,
            sell_token_amount,
            min_sol_output,
            close_order_id,
            close_order_indices
        )
    }

    // 平仓做空交易指令
    pub fn close_short(
        ctx: Context<TradeClose>,
        buy_token_amount: u64, // 买入的token数量
        max_sol_amount: u64,   // 愿意给出的最大sol数量
        close_order_id: u64,           // 订单的唯一编号
        close_order_indices: Vec<u16>, // 平仓时订的位置索引 (可以有多个,万一订单被移动了,就会自动找第二位置)

    ) -> Result<()> {
        instructions::close_long_short::close_short_trade(
            ctx,
            buy_token_amount,
            max_sol_amount,
            close_order_id,
            close_order_indices
        )
    }
}

#[derive(Accounts)]
pub struct Initialize {} // 初始化
