// 导入所需的模块和依赖项
use {
    // 导入常量
    crate::constants::MIN_TRADE_TOKEN_AMOUNT,
    // 导入错误处理
    crate::error::ErrorCode,
    // 导入参数结构和账户结构
    crate::instructions::contexts::TradeBuySell,
    crate::instructions::events::BuySellEvent,
    crate::context_validator::validate_trade_buy_sell_context,
    crate::instructions::utils::calculate_fee_split,
    crate::instructions::trade_engine::{buy_amounts, sell_amounts},
    // 导入转移平仓手续费宏
    crate::transfer_close_fees_split,
    // 导入 CurveAMM 结构体
    crate::curve::curve_amm::CurveAMM,
    // 导入 Anchor 框架的基础组件
    anchor_lang::prelude::*,
};

// 现货买入交易指令的处理函数
pub fn buy_trade(
    ctx: Context<TradeBuySell>,
    buy_token_amount: u64, // 买入的token数量
    max_sol_amount: u64,   // 愿意给出的最大sol数量
) -> Result<()> {
    // 在这里实现现货买入交易的逻辑
    // msg!("处理现货买入交易");

    // 验证交易上下文
    validate_trade_buy_sell_context(&ctx)?;

    // ========== 新增: 验证冷却时间 ==========
    // 如果PDA已存在(last_trade_time > 0)，验证冷却时间
    if ctx.accounts.cooldown.last_trade_time > 0 {
        crate::instructions::validate_trade_cooldown(&ctx.accounts.cooldown)?;
    }

    // 验证交易量是否满足最小交易量
    if buy_token_amount < MIN_TRADE_TOKEN_AMOUNT {
        return Err(ErrorCode::InsufficientTradeAmount.into());
    }

    // // // 打印lp_pairs内容
    // // for (index, pair) in lp_pairs.iter().enumerate() {
    // //     msg!("LP配对 {}: sol_amount={}, token_amount={}", index, pair.sol_amount, pair.token_amount);
    // // }

    // 打印其他函数参数值
    // msg!("buy_trade: buy_token_amount={}", buy_token_amount);
    // msg!("buy_trade: max_sol_amount={}", max_sol_amount);

    // // 准备可变订单数组（使用宏生成）
    // //let mut orders_mut = orders_mut!(ctx);
    // let mut orders_mut_box = Box::new(orders_mut!(ctx));
    // let mut orders_mut = &mut *orders_mut_box;


    // // 创建不可变引用用于计算（使用宏生成）
    // let orders_for_calc = orders_calc!(orders_mut);

    // 提前读取 swap_fee，避免借用冲突
    let swap_fee = ctx.accounts.curve_account.swap_fee;

    // 调用辅助函数计算交易数量
    let calc_result = buy_amounts(
        &mut ctx.accounts.curve_account,
        &ctx.accounts.up_orderbook.to_account_info(),
        None, // pass_order 参数，目前传入空值
        buy_token_amount,
        max_sol_amount,
        swap_fee,
    )?;

    // 强平手续费
    let forced_liquidation_total_fees = calc_result.liquidate_fee_sol;
    
    // 执行SOL转账 - 从用户转到池子
    let cpi_context = CpiContext::new(
        ctx.accounts.system_program.to_account_info(),
        anchor_lang::system_program::Transfer {
            from: ctx.accounts.payer.to_account_info(),
            to: ctx.accounts.pool_sol_account.to_account_info(),
        },
    );
    anchor_lang::system_program::transfer(cpi_context, calc_result.required_sol)?;

    // 执行代币转账 - 从池子转到用户
    let seeds = &[
        b"borrowing_curve",
        ctx.accounts.mint_account.to_account_info().key.as_ref(),
        &[ctx.bumps.curve_account],
    ];
    let signer = &[&seeds[..]];

    let token_transfer_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        anchor_spl::token::Transfer {
            from: ctx.accounts.pool_token_account.to_account_info(),
            to: ctx.accounts.user_token_account.to_account_info(),
            authority: ctx.accounts.curve_account.to_account_info(),
        },
        signer,
    );
    anchor_spl::token::transfer(token_transfer_ctx, calc_result.output_token)?;

    // 转移手续费 - 使用新的分配逻辑
    if calc_result.fee_sol > 0 {
        // 计算手续费分配
        let fee_split_result = calculate_fee_split(calc_result.fee_sol, ctx.accounts.curve_account.fee_split)?;

        // 转账给合作伙伴
        if fee_split_result.partner_fee > 0 {
            let partner_fee_transfer_ctx = CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.payer.to_account_info(),
                    to: ctx.accounts.fee_recipient_account.to_account_info(),
                },
            );
            anchor_lang::system_program::transfer(partner_fee_transfer_ctx, fee_split_result.partner_fee)?;
        }

        // 转账给技术提供方
        if fee_split_result.base_fee > 0 {
            let base_fee_transfer_ctx = CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.payer.to_account_info(),
                    to: ctx.accounts.base_fee_recipient_account.to_account_info(),
                },
            );
            anchor_lang::system_program::transfer(base_fee_transfer_ctx, fee_split_result.base_fee)?;
        }
    }

    // 更新价格
    ctx.accounts.curve_account.price = calc_result.target_price;
    //msg!("价格已更新: {}", ctx.accounts.curve_account.price);
    
    // 检查是否需要应用手续费折扣
    crate::apply_fee_discount_if_needed!(ctx)?;

    // msg!("用户获得 {} 代币", calc_result.output_token);
    // msg!("交易完成!");

    // 转移强制平仓手续费
    let fee_recipient_info = ctx.accounts.fee_recipient_account.to_account_info();
    let base_fee_recipient_info = ctx.accounts.base_fee_recipient_account.to_account_info();
    // msg!("强平手续费:forced_liquidation_total_fees={}",forced_liquidation_total_fees);
    transfer_close_fees_split!(
        forced_liquidation_total_fees,
        &ctx.accounts.pool_sol_account,
        &fee_recipient_info,
        &base_fee_recipient_info,
        ctx.accounts.curve_account.fee_split
    )?;

    // ========== 在所有其他操作完成后，关闭平仓后的订单PDA账户并退还租金 ==========
    // 批量删除已平仓的订单
    if !calc_result.liquidate_indices.is_empty() {
        // msg!("批量删除 {} 个已平仓订单", calc_result.liquidate_indices.len());
        crate::instructions::orderbook_manager::OrderBookManager::batch_remove_by_indices_unsafe(
            &ctx.accounts.up_orderbook.to_account_info(),
            &calc_result.liquidate_indices,
            &ctx.accounts.payer.to_account_info(),
            &ctx.accounts.system_program.to_account_info(),
        )?;
        // msg!("批量删除订单完成");
    }

    // 重新计算流动池储备量
    if let Some((sol_reserve, token_reserve)) = CurveAMM::price_to_reserves(ctx.accounts.curve_account.price) {
        ctx.accounts.curve_account.lp_sol_reserve = sol_reserve;
        ctx.accounts.curve_account.lp_token_reserve = token_reserve;
        // msg!("流动池储备量已根据价格重新计算: SOL={}, Token={}", sol_reserve, token_reserve);
    } else {
        return Err(ErrorCode::BuyReserveRecalculationError.into());
    }

    // ========== 交易成功后，更新冷却记录 ==========
    ctx.accounts.user_token_account.reload()?;
    let new_token_balance = ctx.accounts.user_token_account.amount;

    crate::instructions::update_cooldown_record(
        &mut ctx.accounts.cooldown,
        new_token_balance,
        ctx.bumps.cooldown,
    )?;

    // 触发买入交易事件
    emit!(BuySellEvent {
        payer: ctx.accounts.payer.key(),
        mint_account: ctx.accounts.mint_account.key(),
        is_buy: true,
        token_amount: calc_result.output_token,
        sol_amount: calc_result.required_sol,
        latest_price: calc_result.target_price,
        liquidate_indices: calc_result.liquidate_indices.clone(),
    });

    // 返回成功结果
    Ok(())
}

// 现货卖出交易指令的处理函数
pub fn sell_trade(
    ctx: Context<TradeBuySell>,
    sell_token_amount: u64, // 希望卖出的token数量
    min_sol_output: u64,    // 卖出后最少得到的sol数量
) -> Result<()> {
    // 在这里实现现货卖出交易的逻辑
    // msg!("处理现货卖出交易");

    // 验证交易上下文
    validate_trade_buy_sell_context(&ctx)?;

    // ========== 新增: Sell必须要求PDA已存在 ==========
    if ctx.accounts.cooldown.last_trade_time == 0 {
        // msg!("错误: 冷却PDA未初始化，请先调用buy或approval函数");
        return Err(ErrorCode::CooldownNotInitialized.into());
    }

    // ========== 验证冷却时间 ==========
    crate::instructions::validate_trade_cooldown(&ctx.accounts.cooldown)?;

    // ========== 验证卖出数量不超过批准额度 ==========
    if sell_token_amount > ctx.accounts.cooldown.approval_token_amount {
        // msg!(
        //     "卖出数量({}) 超过批准额度({})",
        //     sell_token_amount,
        //     ctx.accounts.cooldown.approval_token_amount
        // );
        return Err(ErrorCode::ExceedApprovalAmount.into());
    }

    // 验证交易量是否满足最小交易量
    if sell_token_amount < MIN_TRADE_TOKEN_AMOUNT {
        return Err(ErrorCode::InsufficientTradeAmount.into());
    }

    // 提前读取 swap_fee，避免借用冲突
    let swap_fee = ctx.accounts.curve_account.swap_fee;

    // 调用辅助函数计算交易数量
    let calc_result = sell_amounts(
        &mut ctx.accounts.curve_account,
        &ctx.accounts.down_orderbook.to_account_info(),
        None, // pass_order 参数，目前传入空值
        sell_token_amount,
        min_sol_output,
        swap_fee,
    )?;

    // 强制平仓手续费
    let forced_liquidation_total_fees = calc_result.liquidate_fee_sol;

    // 执行代币转移 - 从用户代币账户转移到池子代币账户
    let token_transfer_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        anchor_spl::token::Transfer {
            from: ctx.accounts.user_token_account.to_account_info(),
            to: ctx.accounts.pool_token_account.to_account_info(),
            authority: ctx.accounts.payer.to_account_info(), // 用户作为签名者授权转移
        },
    );
    anchor_spl::token::transfer(token_transfer_ctx, calc_result.sell_token)?;

    // 计算包含手续费的总SOL金额
    let total_sol_with_fee = calc_result
        .output_sol
        .checked_add(calc_result.fee_sol)
        .ok_or(ErrorCode::SellCalculationOverflow)?;

    // 执行SOL转账 - 从池子转到用户（包含手续费的总金额）
    let mut pool_lamports = ctx.accounts.pool_sol_account.try_borrow_mut_lamports()?;
    **pool_lamports = pool_lamports
        .checked_sub(total_sol_with_fee)
        .ok_or(ErrorCode::SolReserveDeductionOverflow)?;
    drop(pool_lamports);
    //msg!("已从池子扣除 {} SOL", total_sol_with_fee);

    // 执行SOL转账 - 为用户加上资金
    let mut payer_lamports = ctx.accounts.payer.try_borrow_mut_lamports()?;
    **payer_lamports = payer_lamports
        .checked_add(calc_result.output_sol)
        .ok_or(ErrorCode::LamportsAdditionOverflow)?;
    drop(payer_lamports);

    // 转移手续费 - 使用新的分配逻辑
    //msg!("========== 开始转移手续费 ==========");
    if calc_result.fee_sol > 0 {
        // 计算手续费分配
        let fee_split_result = calculate_fee_split(calc_result.fee_sol, ctx.accounts.curve_account.fee_split)?;

        // 转移合作伙伴手续费
        if fee_split_result.partner_fee > 0 {
            let mut partner_fee_lamports = ctx
                .accounts
                .fee_recipient_account
                .try_borrow_mut_lamports()?;
            **partner_fee_lamports = partner_fee_lamports
                .checked_add(fee_split_result.partner_fee)
                .ok_or(ErrorCode::PartnerFeeAdditionOverflow)?;
            drop(partner_fee_lamports);
        }

        // 转移技术提供方手续费
        if fee_split_result.base_fee > 0 {
            // msg!(
            //     "技术提供方手续费转移前余额: {}",
            //     ctx.accounts.base_fee_recipient_account.lamports()
            // );

            let mut base_fee_lamports = ctx
                .accounts
                .base_fee_recipient_account
                .try_borrow_mut_lamports()?;
            **base_fee_lamports = base_fee_lamports
                .checked_add(fee_split_result.base_fee)
                .ok_or(ErrorCode::BaseFeeAdditionOverflow)?;
            drop(base_fee_lamports);

        }
    } else {
        //msg!("无需转移手续费");
    }

    // 更新价格 (卖出后价格下降)
    ctx.accounts.curve_account.price = calc_result.target_price;
    //msg!("价格已更新: {}", ctx.accounts.curve_account.price);
    
    // 检查是否需要应用手续费折扣
    crate::apply_fee_discount_if_needed!(ctx)?;

    // 转移强制平仓手续费
    let fee_recipient_info = ctx.accounts.fee_recipient_account.to_account_info();
    let base_fee_recipient_info = ctx.accounts.base_fee_recipient_account.to_account_info();
    transfer_close_fees_split!(
        forced_liquidation_total_fees,
        &ctx.accounts.pool_sol_account,
        &fee_recipient_info,
        &base_fee_recipient_info,
        ctx.accounts.curve_account.fee_split
    )?;

    // ========== 在所有其他操作完成后，关闭平仓后的订单PDA账户并退还租金 ==========
    // 批量删除已平仓的订单
    if !calc_result.liquidate_indices.is_empty() {
        // msg!("批量删除 {} 个已平仓订单", calc_result.liquidate_indices.len());
        crate::instructions::orderbook_manager::OrderBookManager::batch_remove_by_indices_unsafe(
            &ctx.accounts.down_orderbook.to_account_info(),
            &calc_result.liquidate_indices,
            &ctx.accounts.payer.to_account_info(),
            &ctx.accounts.system_program.to_account_info(),
        )?;
        // msg!("批量删除订单完成");
    }

    // 重新计算流动池储备量
    if let Some((sol_reserve, token_reserve)) = CurveAMM::price_to_reserves(ctx.accounts.curve_account.price) {
        ctx.accounts.curve_account.lp_sol_reserve = sol_reserve;
        ctx.accounts.curve_account.lp_token_reserve = token_reserve;
        //msg!("流动池储备量已根据价格重新计算: SOL={}, Token={}", sol_reserve, token_reserve);
    } else {
        return Err(ErrorCode::SellReserveRecalculationError.into());
    }

    // ========== 交易成功后，检查是否需要回收PDA ==========
    ctx.accounts.user_token_account.reload()?;
    let new_token_balance = ctx.accounts.user_token_account.amount;

    if new_token_balance == 0 {
        // 用户卖出了所有代币，回收PDA释放租金
        // msg!("检测到代币余额为0，回收TradeCooldown PDA并释放租金");

        // 关闭PDA账户，租金返还给payer
        let cooldown_account_info = ctx.accounts.cooldown.to_account_info();
        let payer_account_info = ctx.accounts.payer.to_account_info();

        let cooldown_lamports = cooldown_account_info.lamports();

        **cooldown_account_info.lamports.borrow_mut() = 0;
        **payer_account_info.lamports.borrow_mut() = payer_account_info
            .lamports()
            .checked_add(cooldown_lamports)
            .ok_or(ErrorCode::LamportsAdditionOverflow)?;

        // msg!("PDA回收成功，租金({} lamports)已返还给用户", cooldown_lamports);
    } else {
        // 仍有代币余额，正常更新冷却记录
        crate::instructions::update_cooldown_record(
            &mut ctx.accounts.cooldown,
            new_token_balance,
            ctx.bumps.cooldown,
        )?;
    }

    // 触发卖出交易事件
    emit!(BuySellEvent {
        payer: ctx.accounts.payer.key(),
        mint_account: ctx.accounts.mint_account.key(),
        is_buy: false,
        token_amount: calc_result.sell_token,
        sol_amount: calc_result.output_sol,
        latest_price: calc_result.target_price,
        liquidate_indices: calc_result.liquidate_indices.clone(),
    });


    Ok(())
}
