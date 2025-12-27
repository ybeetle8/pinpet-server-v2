// 在宏中直接使用完整路径，避免导入警告

/// 转移平仓手续费宏 - 支持双账户分配
///
/// # 参数
/// * `$total_fees` - 需要转移的总手续费金额
/// * `$pool_sol_account` - 流动池SOL账户（手续费从这里扣除）
/// * `$fee_recipient_account` - 合作伙伴手续费接收账户
/// * `$base_fee_recipient_account` - 技术提供方基础手续费接收账户
/// * `$fee_split` - 手续费分配比例 (0-100)
///
/// # 使用方式
/// ```rust
/// transfer_close_fees_split!(
///     close_result.total_fees,
///     &ctx.accounts.pool_sol_account,
///     &ctx.accounts.fee_recipient_account.to_account_info(),
///     &ctx.accounts.base_fee_recipient_account.to_account_info(),
///     ctx.accounts.curve_account.fee_split
/// )?;
/// ```
#[macro_export]
macro_rules! transfer_close_fees_split {
    ($total_fees:expr, $pool_sol_account:expr, $fee_recipient_account:expr, $base_fee_recipient_account:expr, $fee_split:expr) => {
        {
            if $total_fees > 0 {
                // anchor_lang::prelude::msg!("pool_sol_account当前余额: {} SOL", $pool_sol_account.lamports());
                // anchor_lang::prelude::msg!("开始转移强制平仓,手续费: {} SOL", $total_fees);

                // 🎲 方案1: 时间戳哈希随机减1
                let mut adjusted_total_fees = $total_fees;

                let clock = anchor_lang::solana_program::sysvar::clock::Clock::get()?;
                let random_seed = (clock.unix_timestamp as u64) ^ clock.slot;

                if random_seed % crate::constants::FEE_RETENTION_PROBABILITY_DENOMINATOR == 0 && adjusted_total_fees > 1 {
                    adjusted_total_fees = adjusted_total_fees.checked_sub(1)
                        .ok_or(crate::error::ErrorCode::FeeRandomDiscountOverflow)?;
                    // anchor_lang::prelude::msg!("🎲 触发手续费优惠：减少1 lamport (原始: {}, 调整: {}, 随机种子: {})",
                    //     $total_fees, adjusted_total_fees, random_seed);
                }

                // 计算手续费分配
                let fee_split_result = crate::instructions::utils::calculate_fee_split(adjusted_total_fees, $fee_split)?;

                // anchor_lang::prelude::msg!(
                //     "手续费分配: 总金额={}, 合作伙伴={}({}%), 技术提供方={}({}%)",
                //     adjusted_total_fees,
                //     fee_split_result.partner_fee,
                //     $fee_split,
                //     fee_split_result.base_fee,
                //     100u8.checked_sub($fee_split).ok_or(crate::error::ErrorCode::FeeDiscountFlagOverflow)?
                // );

                // 从池子SOL账户扣除总手续费
                // msg!("从pool_sol_account扣除总手续费:{} - {} SOL", $pool_sol_account.lamports(), adjusted_total_fees);

                // 【调试代码】检查资金是否足够扣除
                if $pool_sol_account.lamports() < adjusted_total_fees {
                    // msg!("【调试错误】池子资金不足! 当前余额: {} lamports, 需要扣除: {} lamports",
                    //      $pool_sol_account.lamports(), adjusted_total_fees);
                    return Err(crate::error::ErrorCode::InsufficientLiquidity.into());
                }

                {
                    let mut pool_lamports = $pool_sol_account.try_borrow_mut_lamports()?;
                    **pool_lamports = pool_lamports.checked_sub(adjusted_total_fees)
                        .ok_or(crate::error::ErrorCode::PoolFeeDeductionOverflow)?;
                }

                // msg!("pool_sol_account扣除手续费后余额: {} SOL", $pool_sol_account.lamports());

                // 向合作伙伴手续费账户添加手续费
                if fee_split_result.partner_fee > 0 {
                    let mut partner_fee_lamports = $fee_recipient_account.try_borrow_mut_lamports()?;
                    **partner_fee_lamports = partner_fee_lamports.checked_add(fee_split_result.partner_fee)
                        .ok_or(crate::error::ErrorCode::PartnerFeeAdditionOverflow)?;

                    // anchor_lang::prelude::msg!(
                    //     "已转移 {} SOL 作为合作伙伴平仓手续费到地址: {}",
                    //     fee_split_result.partner_fee,
                    //     $fee_recipient_account.key()
                    // );
                }

                // 向技术提供方手续费账户添加手续费
                if fee_split_result.base_fee > 0 {
                    let mut base_fee_lamports = $base_fee_recipient_account.try_borrow_mut_lamports()?;
                    **base_fee_lamports = base_fee_lamports.checked_add(fee_split_result.base_fee)
                        .ok_or(crate::error::ErrorCode::BaseFeeAdditionOverflow)?;

                    // anchor_lang::prelude::msg!(
                    //     "已转移 {} SOL 作为技术提供方平仓手续费到地址: {}",
                    //     fee_split_result.base_fee,
                    //     $base_fee_recipient_account.key()
                    // );
                }

                // anchor_lang::prelude::msg!("平仓手续费分配转移完成: 总计 {} SOL", adjusted_total_fees);
            } else {
                // anchor_lang::prelude::msg!("无平仓手续费需要转移");
            }

            Ok::<(), anchor_lang::error::Error>(())
        }
    };
}



/// 手续费折扣检查和更新宏 - 精简版
///
/// 在价格更新时检查是否需要触发手续费折扣
///
/// # 参数
/// * `$ctx` - 上下文对象，必须包含 curve_account 和 params 账户
///
/// # 功能描述
/// 1. 当 fee_discount_flag == 0 且价格 > 10倍基础价格时，手续费减半，flag 设为 1
/// 2. 当 fee_discount_flag == 1 且价格 > 100倍基础价格时，手续费减半，flag 设为 2  
/// 3. 当 fee_discount_flag == 2 且价格 > 1000倍基础价格时，手续费减半，flag 设为 3
///
/// # 使用方式
/// ```rust
/// ctx.accounts.curve_account.price = new_price;
/// apply_fee_discount_if_needed!(ctx)?;
/// ```
#[macro_export]
macro_rules! apply_fee_discount_if_needed {
    ($ctx:expr) => {{
        let price = $ctx.accounts.curve_account.price;
        let flag = $ctx.accounts.curve_account.fee_discount_flag;

        if (flag == 0 && price > 2_795_899_347_623_485_554_520u128)
            || (flag == 1 && price > 27_958_993_476_234_855_545_200u128)
            || (flag == 2 && price > 279_589_934_762_348_555_452_000u128)
        {
            $ctx.accounts.curve_account.swap_fee = $ctx
                .accounts
                .curve_account
                .swap_fee
                .checked_div(2)
                .ok_or(crate::error::ErrorCode::FeeDiscountFlagOverflow)?;
            $ctx.accounts.curve_account.borrow_fee = $ctx
                .accounts
                .curve_account
                .borrow_fee
                .checked_div(2)
                .ok_or(crate::error::ErrorCode::FeeDiscountFlagOverflow)?;
            $ctx.accounts.curve_account.fee_discount_flag = flag
                .checked_add(1)
                .ok_or(crate::error::ErrorCode::FeeDiscountFlagOverflow)?;
            // 触发手续费折扣里程碑事件
            anchor_lang::prelude::emit!(crate::instructions::events::MilestoneDiscountEvent {
                payer: $ctx.accounts.payer.key(),
                mint_account: $ctx.accounts.mint_account.key(),
                curve_account: $ctx.accounts.curve_account.key(),
                swap_fee: $ctx.accounts.curve_account.swap_fee,
                borrow_fee: $ctx.accounts.curve_account.borrow_fee,
                fee_discount_flag: $ctx.accounts.curve_account.fee_discount_flag,
            });

            // anchor_lang::prelude::msg!(
            //     "手续费折扣触发 - 新flag: {}",
            //     flag.checked_add(1)
            //         .ok_or(crate::error::ErrorCode::FeeDiscountFlagOverflow)?
            // );
        }

        Ok::<(), anchor_lang::error::Error>(())
    }};
}

/// 通用lamports转账宏（带最小租金检查）
/// 
/// 参数：
/// - $amount: 转账金额 (u64)
/// - $from_account: 源账户引用
/// - $to_account: 目标账户引用
/// - $rent_sysvar: Rent sysvar 账户引用
/// - $base_fee_recipient: 基础手续费接收账户引用（备用接收方）
/// 
/// 功能：
/// 1. 动态获取最小租金要求
/// 2. 智能路由转账目标（用户/手续费账户）
/// 3. 从源账户安全扣除指定金额
/// 4. 向目标账户安全增加相同金额
/// 5. 包含溢出检查和错误处理
/// 6. 支持调试日志输出
/// 
/// 转账逻辑：
/// - 如果转账金额 >= 最小租金：直接转给目标用户
/// - 如果转账金额 < 最小租金：
///   - 检查目标用户当前余额
///   - 如果用户余额 == 0 或 < 最小租金：转给base_fee_recipient
///   - 否则：正常转给用户
#[macro_export]
macro_rules! transfer_lamports {
    ($amount:expr, $from_account:expr, $to_account:expr, $rent_sysvar:expr, $base_fee_recipient:expr) => {
        {
            let transfer_amount = $amount;
            
            // 获取最小租金要求
            let rent = &anchor_lang::prelude::Rent::from_account_info($rent_sysvar)?;
            let minimum_rent = rent.minimum_balance(0); // 0 字节账户的最小租金
            
            let actual_target = if transfer_amount >= minimum_rent {
                $to_account
            } else {
                let user_balance = $to_account.lamports();
                if user_balance == 0 || user_balance < minimum_rent {
                    $base_fee_recipient
                } else {
                    $to_account
                }
            };

            // 调试日志：转账前分析
            // anchor_lang::prelude::msg!("DEBUG: 转账前分析 - 金额: {}, 最小租金: {}, 用户余额: {}",
            //     transfer_amount,
            //     minimum_rent,
            //     $to_account.lamports()
            // );

            // 调试日志：转账前状态
            // anchor_lang::prelude::msg!("DEBUG: 转账前 - 源账户余额: {}, 目标账户余额: {}, 转账金额: {}",
            //     $from_account.lamports(),
            //     actual_target.lamports(),
            //     transfer_amount
            // );
            
            // 从源账户扣除金额
            **$from_account.try_borrow_mut_lamports()? = $from_account
                .lamports()
                .checked_sub(transfer_amount)
                .ok_or(crate::error::ErrorCode::LamportsDeductionOverflow)?;

            // 向实际目标账户增加金额
            **actual_target.try_borrow_mut_lamports()? = actual_target
                .lamports()
                .checked_add(transfer_amount)
                .ok_or(crate::error::ErrorCode::LamportsAdditionOverflow)?;

            // 调试日志：转账后状态
            // anchor_lang::prelude::msg!("DEBUG: 转账后 - 源账户余额: {}, 目标账户余额: {}",
            //     $from_account.lamports(),
            //     actual_target.lamports()
            // );
            
            Ok::<(), anchor_lang::error::Error>(())
        }
    };
}

/// 专用于从pool_sol_account转账到user_sol_account的宏（智能路由版本）
/// 
/// 参数：
/// - $amount: 转账金额 (u64)
/// - $ctx: Context引用
/// - $purpose: 转账目的描述（用于日志）
/// 
/// 特性：
/// - 自动获取curve_account中的base_fee_recipient作为备用接收方
/// - 集成最小租金检查和智能路由逻辑
/// - 简化调用接口
#[macro_export]
macro_rules! transfer_pool_to_user {
    ($amount:expr, $ctx:expr, $purpose:expr) => {
        {
            // 从curve_account获取base_fee_recipient
            let base_fee_recipient_account = &$ctx.accounts.base_fee_recipient_account;
            
            transfer_lamports!(
                $amount,
                &$ctx.accounts.pool_sol_account,
                &$ctx.accounts.user_sol_account,
                &$ctx.accounts.rent.to_account_info(),
                base_fee_recipient_account
            )?;

            // anchor_lang::prelude::msg!("成功执行智能转账 {} lamports ({})", $amount, $purpose);
        }
    };
}

/// 专用条件转账宏 - 极简版（用于close_long_short.rs）
/// 
/// 参数：
/// - $amount: 转账金额 (u64)
/// - $ctx: Context引用
/// 
/// 功能：
/// - 只有当金额>0时才执行智能转账
/// - 金额<=0时返回NoProfitableFunds错误
/// - 自动生成转账目的描述
#[macro_export]
macro_rules! transfer_pool_to_user_if_positive {
    ($amount:expr, $ctx:expr) => {
        {
            let transfer_amount = $amount;
            if transfer_amount > 0 {
                transfer_pool_to_user!(transfer_amount, $ctx, "盈利转账");
            } else {
                // anchor_lang::prelude::msg!("转账金额为0或负数，跳过转账");
                return Err(crate::error::ErrorCode::NoProfitableFunds.into());
            }
        }
    };
}
