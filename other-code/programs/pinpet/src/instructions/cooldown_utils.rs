use anchor_lang::prelude::*;
use crate::error::ErrorCode;
use crate::constants::TRADE_COOLDOWN_SECONDS;
use crate::instructions::pdas::TradeCooldown;

/// 验证交易冷却时间
pub fn validate_trade_cooldown(cooldown: &TradeCooldown) -> Result<()> {
    let current_time = Clock::get()?.unix_timestamp as u32;
    let elapsed_time = current_time.saturating_sub(cooldown.last_trade_time);

    if elapsed_time < TRADE_COOLDOWN_SECONDS {
        // msg!(
        //     "冷却时间未到: 已过{}秒, 需要{}秒",
        //     elapsed_time,
        //     TRADE_COOLDOWN_SECONDS
        // );
        return Err(ErrorCode::TradeCooldownNotExpired.into());
    }

    Ok(())
}

/// 更新冷却时间记录
pub fn update_cooldown_record(
    cooldown: &mut Account<TradeCooldown>,
    new_token_amount: u64,
    bump: u8,
) -> Result<()> {
    let current_time = Clock::get()?.unix_timestamp as u32;

    cooldown.last_trade_time = current_time;
    cooldown.approval_token_amount = new_token_amount;
    cooldown.bump = bump;

    // msg!("冷却记录已更新: time={}, amount={}", current_time, new_token_amount);
    Ok(())
}
