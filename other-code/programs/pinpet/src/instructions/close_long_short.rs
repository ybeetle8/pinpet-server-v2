// 导入所需的模块和依赖项
use {
    // 导入常量
    crate::constants::{MIN_TRADE_TOKEN_AMOUNT, MAX_CLOSE_INSERT_INDICES, TRADE_COOLDOWN_SECONDS},
    // 导入曲线AMM计算模块
    crate::curve::curve_amm::CurveAMM,
    // 导入错误处理
    crate::error::ErrorCode,
    crate::instructions::trade_engine::{buy_amounts, sell_amounts},

    crate::instructions::contexts::TradeClose,
    // 导入上下文验证函数
    crate::instructions::context_validator::validate_trade_close_context,
    crate::instructions::events::{FullCloseEvent, PartialCloseEvent},

    // 导入订单簿管理器
    crate::instructions::orderbook_manager::{OrderBookManager, MarginOrderUpdateData},
    // 导入转移平仓手续费宏
    crate::transfer_close_fees_split,
    // 导入转账宏
    crate::transfer_pool_to_user_if_positive,
    crate::transfer_pool_to_user,
    crate::transfer_lamports,
    // 导入 Anchor 框架的基础组件
    anchor_lang::prelude::*,
};

// 平仓做多的订单 - 本质上是卖出
pub fn close_long_trade(
    ctx: Context<TradeClose>,
    sell_token_amount: u64, // 希望卖出的token数量(非确定值可能有千万分之几的偏差)
    min_sol_output: u64,    // 卖出后最少得到的sol数量
    close_order_id: u64,    // 订单的唯一编号
    close_order_indices: Vec<u16>, // 平仓时订的位置索引 (可以有多个,万一订单被移动了,就会自动找第二位置)
) -> Result<()> {

    // msg!("-处理平仓做多交易-");
    // msg!("订单 order ID: {}, 候选索引数量: {}", close_order_id, close_order_indices.len());

    validate_trade_close_context(&ctx)?;



    // 验证 close_order_indices 数量
    if close_order_indices.is_empty() {
        return Err(ErrorCode::EmptyCloseInsertIndices.into());
    }
    if close_order_indices.len() > MAX_CLOSE_INSERT_INDICES {
        return Err(ErrorCode::TooManyCloseInsertIndices.into());
    }

    // 获取 down_orderbook 账户信息
    let down_orderbook_info = ctx.accounts.down_orderbook.to_account_info();

    // TODO: 临时调试代码，以后删除 - 打印 orderbook 总订单数量
    {
        let orderbook_data = down_orderbook_info.data.borrow();
        let orderbook = OrderBookManager::load_orderbook_header(&orderbook_data)?;
        // msg!("[调试] down_orderbook 总订单数量: {}", orderbook.total);
        drop(orderbook_data);
    }

    // 定义找到的订单变量
    let mut close_margin_order: Option<crate::instructions::structs::MarginOrder> = None;
    // 找到的订单的索引
    let mut close_margin_index: u16 = u16::MAX; 

    // 遍历 close_order_indices 查找匹配的订单
    for (attempt_idx, &current_index) in close_order_indices.iter().enumerate() {
        // msg!("尝试第 {} 个索引: {}", attempt_idx + 1, current_index);

        // 借用 orderbook 数据
        let orderbook_data = down_orderbook_info.data.borrow();

        // 获取当前索引的订单
        let current_order_result = OrderBookManager::get_order(&orderbook_data, current_index);

        if current_order_result.is_err() {
            // msg!("  索引 {} 无效，跳过", current_index);
            continue;
        }

        let current_order = current_order_result.unwrap();

        // 检查当前节点的 order_id
        if current_order.order_id == close_order_id {
            // msg!("  ✓ 在当前节点找到匹配订单 (index={})", current_index);
            close_margin_order = Some(*current_order);
            close_margin_index = current_index;
            break;
        }

        // 检查前一个节点 (prev_order)
        if current_order.prev_order != u16::MAX {
            let prev_order_result = OrderBookManager::get_order(&orderbook_data, current_order.prev_order);

            if let Ok(prev_order) = prev_order_result {
                if prev_order.order_id == close_order_id {
                    // msg!("  ✓ 在前驱节点找到匹配订单 (index={})", current_order.prev_order);
                    close_margin_order = Some(*prev_order);
                    close_margin_index = current_order.prev_order;
                    break;
                }
            }
        }

        // 检查后一个节点 (next_order)
        if current_order.next_order != u16::MAX {
            let next_order_result = OrderBookManager::get_order(&orderbook_data, current_order.next_order);

            if let Ok(next_order) = next_order_result {
                if next_order.order_id == close_order_id {
                    // msg!("  ✓ 在后继节点找到匹配订单 (index={})", current_order.next_order);
                    close_margin_order = Some(*next_order);
                    close_margin_index = current_order.next_order;
                    break;
                }
            }
        }

        // msg!("  ✗ 索引 {} 及其前后节点均未找到匹配订单", current_index);
    }

    // 如果遍历完所有索引都没找到，返回错误
    let close_margin_order = match close_margin_order {
        Some(order) => order,
        None => {
            // msg!("错误: 遍历完所有 {} 个索引后仍未找到订单 ID {}", close_order_indices.len(), close_order_id);
            return Err(ErrorCode::CloseOrderNotFound.into());
        }
    };

    // msg!("成功找到订单: index={}, user={}, margin_sol_amount={}, borrow_amount={}",
    //     close_margin_index,
    //     close_margin_order.user,
    //     close_margin_order.margin_sol_amount,
    //     close_margin_order.borrow_amount
    // );

    // 1. 首先检查冷却时间
    let current_timestamp = Clock::get()?.unix_timestamp as u32;
    let time_elapsed = current_timestamp
        .checked_sub(close_margin_order.start_time)
        .ok_or(ErrorCode::ArithmeticOverflow)?;

    if time_elapsed < TRADE_COOLDOWN_SECONDS {
        return Err(ErrorCode::TradeCooldownNotExpired.into());
    }

    // 2. 检查当前时间戳是否大于等于订单结束时间
    if current_timestamp < close_margin_order.end_time {
        // 订单未超时，检查是否是开仓者本人进行平仓
        if ctx.accounts.payer.key() != close_margin_order.user {
            return Err(ErrorCode::OrderNotExpiredMustCloseByOwner.into());
        }
    } else {
        //msg!("订单已超时，任何人都可以平仓");
    }

    // 3. 检查结算地址必须是开仓地址
    if ctx.accounts.user_sol_account.key() != close_margin_order.user {
        return Err(ErrorCode::SettlementAddressMustBeOwnerAddress.into());
    }


    // 验证卖出数量不能超过订单持有的代币数量
    if sell_token_amount > close_margin_order.lock_lp_token_amount {
        return Err(ErrorCode::SellAmountExceedsOrderAmount.into());
    }

    if sell_token_amount != close_margin_order.lock_lp_token_amount {
        // 如果不是平光,就要 验证交易量是否满足最小交易量的2倍
        if sell_token_amount < MIN_TRADE_TOKEN_AMOUNT * 2 {
            return Err(ErrorCode::InsufficientTradeAmount.into());
        }

        // 检查部分平仓后剩余的代币数量是否小于最小交易量，防止溢出
        let remaining_token_amount = close_margin_order
            .lock_lp_token_amount
            .checked_sub(sell_token_amount)
            .ok_or(ErrorCode::CloseLongRemainingOverflow)?;

        if remaining_token_amount < MIN_TRADE_TOKEN_AMOUNT {
            return Err(ErrorCode::RemainingTokenAmountTooSmall.into());
        }
    }

    // 调用辅助函数计算交易数量
    let mut calc_result = sell_amounts(
        &mut ctx.accounts.curve_account,
        &ctx.accounts.down_orderbook.to_account_info(),
        Some(close_margin_order.order_id), // 传入当前要平仓的订单的key
        sell_token_amount,
        min_sol_output,
        close_margin_order.borrow_fee
    )?;

    // 强制平仓手续费 也可加上 主动平仓手续费
    let mut forced_liquidation_total_fees = calc_result.liquidate_fee_sol;
    // 把 交易手续费用 加到 强制平仓手续费 - 自己的交易要自己出手续费
    forced_liquidation_total_fees = forced_liquidation_total_fees
        .checked_add(calc_result.fee_sol)
        .ok_or(ErrorCode::CloseLongFeeOverflow)?;


    let mut is_close_pda = false;
    if sell_token_amount == close_margin_order.lock_lp_token_amount {
        // 是平光了
        is_close_pda = true; // 设置关pda标记
        // msg!(
        //     "---------平光了---------- 卖出数量: sell_token_amount={}",
        //     sell_token_amount
        // );

        //归还借币池
        ctx.accounts.curve_account.borrow_sol_reserve = ctx
            .accounts
            .curve_account
            .borrow_sol_reserve
            .checked_add(close_margin_order.borrow_amount)
            .ok_or(ErrorCode::CloseLongRepaymentOverflow)?;

        let profit_sol = calc_result
            .output_sol
            .checked_add(close_margin_order.margin_sol_amount)
            .ok_or(ErrorCode::CloseLongProfitOverflow)?
            .checked_sub(close_margin_order.borrow_amount)
            .ok_or(ErrorCode::CloseLongProfitOverflow)?;

        // 6. 把 盈利资金 sol转到, user_sol_account 账户
        transfer_pool_to_user_if_positive!(profit_sol, ctx);

        // 更新价格
        ctx.accounts.curve_account.price = calc_result.target_price;
        //msg!("价格已更新为: {}", ctx.accounts.curve_account.price);

        // 删除节点前，获取前后节点信息并重新计算流动性（删除后索引会变化）
        let prev_index = close_margin_order.prev_order;
        let next_index = close_margin_order.next_order;

        // 在删除节点前重新计算前节点的 next_lp_sol_amount 和 next_lp_token_amount
        // 有4种场景需要处理
        if prev_index != u16::MAX && next_index != u16::MAX {
            // 场景4: 前后都有节点 - 需要重新计算前节点到后节点之间的流动性
            // msg!("场景4: 删除中间节点，重新计算前节点(index={})到后节点(index={})的流动性", prev_index, next_index);

            let down_orderbook_info = ctx.accounts.down_orderbook.to_account_info();
            let orderbook_data = down_orderbook_info.data.borrow();

            // 获取前节点和后节点的价格及 order_id
            let prev_order = OrderBookManager::get_order(&orderbook_data, prev_index)?;
            let prev_order_id = prev_order.order_id;
            let prev_lock_lp_end_price = prev_order.lock_lp_end_price;

            let next_order = OrderBookManager::get_order(&orderbook_data, next_index)?;
            let next_lock_lp_start_price = next_order.lock_lp_start_price;

            drop(orderbook_data);

            // 使用 sell_from_price_to_price 计算新的流动性
            let (new_token_amount, new_sol_amount) = CurveAMM::sell_from_price_to_price(
                prev_lock_lp_end_price,
                next_lock_lp_start_price,
            ).ok_or(ErrorCode::CloseLongRemainingOverflow)?;

            // 使用 update_order 方法更新前节点
            let update_data = MarginOrderUpdateData {
                next_lp_sol_amount: Some(new_sol_amount),
                next_lp_token_amount: Some(new_token_amount),
                ..Default::default()
            };

            OrderBookManager::update_order(
                &down_orderbook_info,
                prev_index,
                prev_order_id,
                &update_data,
            )?;

            // msg!("前节点流动性已更新1: next_lp_sol={}, next_lp_token={}", new_sol_amount, new_token_amount);

        } else if prev_index != u16::MAX && next_index == u16::MAX {
            // 场景3: 只有前节点，没有后节点 - 前节点变成尾节点，设置无限流动性
            // msg!("场景3: 删除尾节点，前节点(index={})变为新尾节点，设置无限流动性", prev_index);

            let down_orderbook_info = ctx.accounts.down_orderbook.to_account_info();
            let orderbook_data = down_orderbook_info.data.borrow();

            let prev_order = OrderBookManager::get_order(&orderbook_data, prev_index)?;
            let prev_order_id = prev_order.order_id;

            drop(orderbook_data);

            // 使用 update_order 方法更新前节点
            let update_data = MarginOrderUpdateData {
                next_lp_sol_amount: Some(CurveAMM::MAX_U64),
                next_lp_token_amount: Some(CurveAMM::MAX_U64),
                ..Default::default()
            };

            OrderBookManager::update_order(
                &down_orderbook_info,
                prev_index,
                prev_order_id,
                &update_data,
            )?;

            // msg!("前节点流动性已设置为无限: next_lp_sol={}, next_lp_token={}", CurveAMM::MAX_U64, CurveAMM::MAX_U64);

        } else if prev_index == u16::MAX {
            // 场景2: 没有前节点 - 删除的是头节点，不需要处理
            // msg!("场景2: 删除头节点，无需更新流动性");
        } else {
            // 场景1: 前后都没节点 - 链表为空，不需要处理
            // msg!("场景1: 链表已空，无需更新流动性");
        }

        // 将当前订单索引加入清算列表，统一批量删除
        calc_result.liquidate_indices.push(close_margin_index);
        // msg!("全平仓：添加当前订单到清算列表 index={}, order_id={}", close_margin_index, close_order_id);

        // 检查是否需要应用手续费折扣
        crate::apply_fee_discount_if_needed!(ctx)?;

        // 7.5. 发射全平仓事件
        emit!(FullCloseEvent {
            payer: ctx.accounts.payer.key(),
            user_sol_account: ctx.accounts.user_sol_account.key(),
            mint_account: ctx.accounts.mint_account.key(),
            is_close_long: true,
            final_token_amount: sell_token_amount,
            final_sol_amount: calc_result.output_sol,
            user_close_profit: profit_sol,
            latest_price: calc_result.target_price,
            order_id: close_margin_order.order_id,
            order_index: close_margin_index,
            liquidate_indices: calc_result.liquidate_indices.clone(),
        });

        // 不在这里关,移到函数最后了 8. 关闭平仓订单的PDA账户并退还租金
    } else {
        // 是部份平仓
        // msg!("---------B部份平仓----------  ");

        // 剩余 token 资产
        let remaining_position_asset_amount = close_margin_order
            .position_asset_amount
            .checked_sub(sell_token_amount)
            .ok_or(ErrorCode::CloseLongRemainingOverflow)?;

        //msg!("剩余 token 资产: {}",remaining_position_asset_amount);

        // 下面这步应该有问题, 不应是从 lock_lp_start_price 价开始,应该是先卖出 sell_token_amount 后的价格开始
        let close_sell_result = CurveAMM::sell_from_price_with_token_input(
            close_margin_order.lock_lp_start_price, // 以平仓价格为起点
            sell_token_amount,           // 卖出剩下的代币
        );
        let (close_end_price, _close_output_sol) =
            close_sell_result.ok_or(ErrorCode::CloseLongRemainingOverflow)?;

        
        // 下面这步应该有问题, 不应是从 lock_lp_start_price 价开始,应该是先卖出 sell_token_amount 后的价格开始
        // 重新 计算未来平仓时可能获得的SOL数量
        let remaining_sell_result = CurveAMM::sell_from_price_with_token_input(
            close_end_price, // 以平仓价格为起点
            remaining_position_asset_amount,           // 卖出剩下的代币
        );

        // 如果计算失败，直接返回错误
        let (_remaining_end_price, remaining_output_sol) =
            remaining_sell_result.ok_or(ErrorCode::CloseLongRemainingOverflow)?;

        // 余下部份扣除手续费后能得到的sol 数量
        let remaining_output_sol_after_fee = CurveAMM::calculate_amount_after_fee(
            remaining_output_sol,
            close_margin_order.borrow_fee,
        )
        .ok_or(ErrorCode::CloseLongFeeOverflow)?;
        
        let profit_portion = calc_result
            .output_sol // 这次赚的sol
            .checked_add(close_margin_order.margin_sol_amount) // +保证金 sol
            .ok_or(ErrorCode::CloseLongProfitOverflow)?
            .checked_add(remaining_output_sol_after_fee) // +剩余部份可赚的sol
            .ok_or(ErrorCode::CloseLongProfitOverflow)?
            .checked_sub(close_margin_order.borrow_amount) // -减去借的sol
            .ok_or(ErrorCode::CloseLongProfitOverflow)?;

        //msg!("@@ 部份平仓盈利 sol :{}", profit_portion);

        // 可用来还款的资金
        let close_borrow_sol_amount = calc_result
            .output_sol // 这次卖token 赚的钱
            .checked_sub(profit_portion) // -减去 部份平仓盈利
            .ok_or(ErrorCode::CloseLongRepaymentOverflow)?;

        //msg!("@@ 可用来还款的资金 sol :{}", close_borrow_sol_amount);

        //归还借币池
        ctx.accounts.curve_account.borrow_sol_reserve = ctx
            .accounts
            .curve_account
            .borrow_sol_reserve
            .checked_add(close_borrow_sol_amount)
            .ok_or(ErrorCode::CloseLongRepaymentOverflow)?;

        // 下面有 margin_sol_amount 需要重新计算


        // 计算更新后的值
        let new_borrow_amount = close_margin_order
            .borrow_amount
            .checked_sub(close_borrow_sol_amount)
            .ok_or(ErrorCode::CloseLongRepaymentOverflow)?;

        let new_realized_sol_amount = close_margin_order
            .realized_sol_amount
            .checked_add(profit_portion)
            .ok_or(ErrorCode::CloseLongProfitOverflow)?;

        //msg!("@@ 还 sol 后,还借:{} sol", new_borrow_amount);


        // 使用 update_order 方法更新订单
        let update_data = MarginOrderUpdateData {
            lock_lp_start_price: Some(close_end_price), // 平仓卖掉后的新起点
            lock_lp_token_amount: Some(remaining_position_asset_amount),
            lock_lp_sol_amount: Some(remaining_output_sol),
            position_asset_amount: Some(remaining_position_asset_amount),
            borrow_amount: Some(new_borrow_amount), // 还钱 sol 单位
            realized_sol_amount: Some(new_realized_sol_amount), // 累加已实现盈利
            //margin_sol_amount: None, // 不更新
            ..Default::default()
        };

        OrderBookManager::update_order(
            &ctx.accounts.down_orderbook.to_account_info(),
            close_margin_index,
            close_order_id,
            &update_data,
        )?;

        // 如果有前节点,那就需要对前节点的 next_lp_sol_amount next_lp_token_amount 进行重新计算
        let prev_index = close_margin_order.prev_order;

        if prev_index != u16::MAX {
            // 有前节点，需要重新计算前节点到当前订单之间的流动性
            // msg!("部分平仓: 重新计算前节点(index={})的流动性", prev_index);

            let down_orderbook_info = ctx.accounts.down_orderbook.to_account_info();
            let orderbook_data = down_orderbook_info.data.borrow();

            // 获取前节点的数据
            let prev_order = OrderBookManager::get_order(&orderbook_data, prev_index)?;
            let prev_order_id = prev_order.order_id;
            let prev_lock_lp_end_price = prev_order.lock_lp_end_price;

            drop(orderbook_data);

            // 使用 sell_from_price_to_price 计算前节点到当前订单(更新后)之间的流动性
            // 前节点的结束价格 -> 当前订单的新起点价格(close_end_price，即部分平仓后的价格)
            // 注意: 不能使用 close_margin_order.lock_lp_start_price，因为它还是旧值
            let (new_next_token_amount, new_next_sol_amount) = CurveAMM::sell_from_price_to_price(
                prev_lock_lp_end_price,  // 前节点的结束价格
                close_end_price,          // 当前订单更新后的起点价格(部分平仓后的新价格)
            ).ok_or(ErrorCode::CloseLongRemainingOverflow)?;

            // 更新前节点的 next_lp_sol_amount 和 next_lp_token_amount
            let prev_update_data = MarginOrderUpdateData {
                next_lp_sol_amount: Some(new_next_sol_amount),
                next_lp_token_amount: Some(new_next_token_amount),
                ..Default::default()
            };

            OrderBookManager::update_order(
                &down_orderbook_info,
                prev_index,
                prev_order_id,
                &prev_update_data,
            )?;

            // msg!("前节点流动性已更新2: next_lp_sol={}, next_lp_token={}", new_next_sol_amount, new_next_token_amount);
        } else {
            // 没有前节点(当前订单是链表头部)，不需要处理
            // msg!("部分平仓: 当前订单是链表头部，无需更新前节点流动性");
        }

        //msg!("========== 做多订单价格区间调整完成 ==========")

        // 将 profit_portion 资金 sol 转到 user_sol_account 账户
        transfer_pool_to_user_if_positive!(profit_portion, ctx);

        // 更新价格
        ctx.accounts.curve_account.price = calc_result.target_price;
        //msg!("价格已更新为: {}", ctx.accounts.curve_account.price);

        // 检查是否需要应用手续费折扣
        crate::apply_fee_discount_if_needed!(ctx)?;

        // 发射部分平仓事件
        emit!(PartialCloseEvent {
            payer: ctx.accounts.payer.key(),
            user_sol_account: ctx.accounts.user_sol_account.key(),
            mint_account: ctx.accounts.mint_account.key(),
            is_close_long: true,
            final_token_amount: sell_token_amount,
            final_sol_amount: calc_result.output_sol,
            user_close_profit: profit_portion,
            latest_price: calc_result.target_price,
            order_id: close_margin_order.order_id,  // 使用订单的 user 字段作为 PDA 地址
            order_index: close_margin_index,
            // 部分平仓订单的参数(修改后的值) - 使用更新后的值
            order_type: close_margin_order.order_type,
            user: close_margin_order.user,
            lock_lp_start_price: close_end_price,  // 使用更新后的值
            lock_lp_end_price: close_margin_order.lock_lp_end_price,
            lock_lp_sol_amount: remaining_output_sol,  // 使用更新后的值
            lock_lp_token_amount: remaining_position_asset_amount,  // 使用更新后的值
            start_time: close_margin_order.start_time,
            end_time: close_margin_order.end_time,
            margin_sol_amount: close_margin_order.margin_sol_amount,
            borrow_amount: new_borrow_amount,  // 使用更新后的值
            position_asset_amount: remaining_position_asset_amount,  // 使用更新后的值
            borrow_fee: close_margin_order.borrow_fee,
            realized_sol_amount: new_realized_sol_amount,  // 使用更新后的值
            liquidate_indices: calc_result.liquidate_indices.clone(),
        });

        //msg!("部分平仓操作完成");
    }

    // 重新计算流动池储备量
    if let Some((sol_reserve, token_reserve)) =
        CurveAMM::price_to_reserves(ctx.accounts.curve_account.price)
    {
        ctx.accounts.curve_account.lp_sol_reserve = sol_reserve;
        ctx.accounts.curve_account.lp_token_reserve = token_reserve;
    } else {
        return Err(ErrorCode::CurveCalculationError.into());
    }

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

    if is_close_pda {
        // 如果是全平仓, 必须关闭平仓订单的PDA账户并退还租金 
        // 这里上面已经退租金了 (要看看会不会出问题)
        //close_one_order_pda!(close_margin_order, ctx.accounts.payer)?;
    }
    
    // ========== 在所有其他操作完成后，关闭平仓后的订单PDA账户并退还租金 ==========
    // 批量删除已平仓的订单
    if !calc_result.liquidate_indices.is_empty() {
        // 获取删除前的链表总数
        let orderbook_account_info = ctx.accounts.down_orderbook.to_account_info();
        let orderbook_data = orderbook_account_info.try_borrow_data()?;
        let header_before = crate::instructions::orderbook_manager::OrderBookManager::load_orderbook_header(&orderbook_data)?;
        let total_before = header_before.total;
        drop(orderbook_data); // 释放借用，避免冲突

        let delete_count = calc_result.liquidate_indices.len();
        // msg!("批量删除前: 链表总数={}, 待删除数量={}", total_before, delete_count);

        // 执行批量删除
        crate::instructions::orderbook_manager::OrderBookManager::batch_remove_by_indices_unsafe(
            &orderbook_account_info,
            &calc_result.liquidate_indices,
            &ctx.accounts.payer.to_account_info(),
            &ctx.accounts.system_program.to_account_info(),
        )?;

        // 获取删除后的链表总数
        let orderbook_data = orderbook_account_info.try_borrow_data()?;
        let header_after = crate::instructions::orderbook_manager::OrderBookManager::load_orderbook_header(&orderbook_data)?;
        let total_after = header_after.total;
        drop(orderbook_data);

        let expected_total = total_before.checked_sub(delete_count as u16)
            .ok_or(ErrorCode::ArithmeticOverflow)?;

        // msg!("批量删除后: 链表总数={}, 期望总数={}", total_after, expected_total);

        // 验证删除后的总数是否正确
        if total_after != expected_total {
            // msg!("❌ 链表删除计数异常! 删除前={}, 删除数={}, 期望剩余={}, 实际剩余={}, 差值={}",
            //     total_before, delete_count, expected_total, total_after,
            //     if total_after > expected_total {
            //         total_after - expected_total
            //     } else {
            //         expected_total - total_after
            //     }
            // );
            return err!(ErrorCode::LinkedListDeleteCountMismatch);
        }

        // msg!("✓ 批量删除订单完成，计数验证通过");
    }

    // TODO: 后续的平仓逻辑将在这里实现

    Ok(())
}

// 平仓做空的订单 - 本质上是买入
pub fn close_short_trade(
    ctx: Context<TradeClose>,
    buy_token_amount: u64, // 买入的token数量
    max_sol_amount: u64,   // 愿意给出的最大sol数量
    close_order_id: u64,   // 订单的唯一编号
    close_order_indices: Vec<u16>, // 平仓时订的位置索引 (可以有多个,万一订单被移动了,就会自动找第二位置)
) -> Result<()> {

    // msg!("-处理平仓做空交易-");
    // msg!("订单 order ID: {}, 候选索引数量: {}", close_order_id, close_order_indices.len());

    validate_trade_close_context(&ctx)?;

    // 验证 close_order_indices 数量
    if close_order_indices.is_empty() {
        return Err(ErrorCode::EmptyCloseInsertIndices.into());
    }
    if close_order_indices.len() > MAX_CLOSE_INSERT_INDICES {
        return Err(ErrorCode::TooManyCloseInsertIndices.into());
    }

    // 获取 up_orderbook 账户信息（做空订单在 up_orderbook 中）
    let up_orderbook_info = ctx.accounts.up_orderbook.to_account_info();

    // TODO: 临时调试代码，以后删除 - 打印 orderbook 总订单数量
    {
        let orderbook_data = up_orderbook_info.data.borrow();
        let orderbook = OrderBookManager::load_orderbook_header(&orderbook_data)?;
        // msg!("[调试] up_orderbook 总订单数量: {}", orderbook.total);
        drop(orderbook_data);
    }

    // 定义找到的订单变量
    let mut close_margin_order: Option<crate::instructions::structs::MarginOrder> = None;
    // 找到的订单的索引
    let mut close_margin_index: u16 = u16::MAX;

    // 遍历 close_order_indices 查找匹配的订单
    for (attempt_idx, &current_index) in close_order_indices.iter().enumerate() {
        // msg!("尝试第 {} 个索引: {}", attempt_idx + 1, current_index);

        // 借用 orderbook 数据
        let orderbook_data = up_orderbook_info.data.borrow();

        // 获取当前索引的订单
        let current_order_result = OrderBookManager::get_order(&orderbook_data, current_index);

        if current_order_result.is_err() {
            // msg!("  索引 {} 无效，跳过", current_index);
            continue;
        }

        let current_order = current_order_result.unwrap();

        // 检查当前节点的 order_id
        if current_order.order_id == close_order_id {
            // msg!("  ✓ 在当前节点找到匹配订单 (index={})", current_index);
            close_margin_order = Some(*current_order);
            close_margin_index = current_index;
            break;
        }

        // 检查前一个节点 (prev_order)
        if current_order.prev_order != u16::MAX {
            let prev_order_result = OrderBookManager::get_order(&orderbook_data, current_order.prev_order);

            if let Ok(prev_order) = prev_order_result {
                if prev_order.order_id == close_order_id {
                    // msg!("  ✓ 在前驱节点找到匹配订单 (index={})", current_order.prev_order);
                    close_margin_order = Some(*prev_order);
                    close_margin_index = current_order.prev_order;
                    break;
                }
            }
        }

        // 检查后一个节点 (next_order)
        if current_order.next_order != u16::MAX {
            let next_order_result = OrderBookManager::get_order(&orderbook_data, current_order.next_order);

            if let Ok(next_order) = next_order_result {
                if next_order.order_id == close_order_id {
                    // msg!("  ✓ 在后继节点找到匹配订单 (index={})", current_order.next_order);
                    close_margin_order = Some(*next_order);
                    close_margin_index = current_order.next_order;
                    break;
                }
            }
        }

        // msg!("  ✗ 索引 {} 及其前后节点均未找到匹配订单", current_index);
    }

    // 如果遍历完所有索引都没找到，返回错误
    let close_margin_order = match close_margin_order {
        Some(order) => order,
        None => {
            // msg!("错误: 遍历完所有 {} 个索引后仍未找到订单 ID {}", close_order_indices.len(), close_order_id);
            return Err(ErrorCode::CloseOrderNotFound.into());
        }
    };

    // msg!("成功找到订单: index={}, user={}, margin_sol_amount={}, borrow_amount={}",
    //     close_margin_index,
    //     close_margin_order.user,
    //     close_margin_order.margin_sol_amount,
    //     close_margin_order.borrow_amount
    // );

    // 1. 首先检查冷却时间
    let current_timestamp = Clock::get()?.unix_timestamp as u32;
    let time_elapsed = current_timestamp
        .checked_sub(close_margin_order.start_time)
        .ok_or(ErrorCode::ArithmeticOverflow)?;

    if time_elapsed < TRADE_COOLDOWN_SECONDS {
        return Err(ErrorCode::TradeCooldownNotExpired.into());
    }

    // 2. 检查当前时间戳是否大于等于订单结束时间
    // msg!(
    //     "当前时间戳: {}, 订单结束时间: {}",
    //     current_timestamp,
    //     close_margin_order.end_time
    // );

    if current_timestamp < close_margin_order.end_time {
        // 订单未超时，检查是否是开仓者本人进行平仓
        if ctx.accounts.payer.key() != close_margin_order.user {
            // msg!(
            //     "订单未超时，只能由开仓者平仓。开仓者: {}, 当前操作者: {}",
            //     ctx.accounts.close_order.user,
            //     ctx.accounts.payer.key()
            // );
            return Err(ErrorCode::OrderNotExpiredMustCloseByOwner.into());
        }
        //msg!("订单未超时，但由开仓者本人平仓，验证通过");
    } else {
        //msg!("订单已超时，任何人都可以平仓");
    }

    // 3. 检查结算地址必须是开仓地址
    if ctx.accounts.user_sol_account.key() != close_margin_order.user {
        // msg!(
        //     "结算地址错误。开仓地址: {}, 结算地址: {}",
        //     ctx.accounts.close_order.user,
        //     ctx.accounts.user_sol_account.key()
        // );
        return Err(ErrorCode::SettlementAddressMustBeOwnerAddress.into());
    }
    //msg!("结算地址验证通过");

    // 验证买入数量不能超过订单持有的代币数量
    if buy_token_amount > close_margin_order.lock_lp_token_amount {
        // msg!(
        //     "买入数量 {} 超过订单持有的代币数量 {}",
        //     buy_token_amount,
        //     ctx.accounts.close_order.lock_lp_token_amount
        // );
        return Err(ErrorCode::BuyAmountExceedsOrderAmount.into());
    }


    if buy_token_amount != close_margin_order.lock_lp_token_amount {
        // 如果不是平光,就要 验证交易量是否满足最小交易量的2倍
        if buy_token_amount < MIN_TRADE_TOKEN_AMOUNT * 2 {
            return Err(ErrorCode::InsufficientTradeAmount.into());
        }
        // 检查部分平仓后剩余的代币数量是否小于最小交易量，防止溢出
        let remaining_token_amount = close_margin_order
            .lock_lp_token_amount
            .checked_sub(buy_token_amount)
            .ok_or(ErrorCode::CloseShortRemainingOverflow)?;

        if remaining_token_amount < MIN_TRADE_TOKEN_AMOUNT {
            return Err(ErrorCode::RemainingTokenAmountTooSmall.into());
        }
    }

    // 调用辅助函数计算交易数量
    let mut calc_result = buy_amounts(
        &mut ctx.accounts.curve_account,
        &ctx.accounts.up_orderbook.to_account_info(),
        Some(close_margin_order.order_id), // 传入当前要平仓的订单的key
        buy_token_amount,
        max_sol_amount,
        close_margin_order.borrow_fee,
    )?;

    // 强制平仓手续费
    let mut forced_liquidation_total_fees = calc_result.liquidate_fee_sol;
    // 把 交易手续费用 加到 强制平仓手续费
    forced_liquidation_total_fees = forced_liquidation_total_fees
        .checked_add(calc_result.fee_sol)
        .ok_or(ErrorCode::CloseShortFeeOverflow)?;

    //msg!("交易条件验证通过，执行平仓做空交易");
    let mut is_close_pda = false;

    if buy_token_amount == close_margin_order.lock_lp_token_amount {
        // msg!("--------空单--全平仓-------------");
        is_close_pda = true; //关pda 标记

        // 计算关闭区间的流动性
        let (_close_end_price, close_reduced_sol) = CurveAMM::buy_from_price_with_token_output(
            close_margin_order.lock_lp_start_price,
            buy_token_amount,
        )
        .ok_or(ErrorCode::PriceCalculationError)?;

        // 买关闭区间 的token 需要的sol含手续费
        let close_reduced_sol_with_fee = CurveAMM::calculate_total_amount_with_fee(
            close_reduced_sol,
            close_margin_order.borrow_fee
        )
        .ok_or(ErrorCode::CloseShortFeeOverflow)?;

        let profit_sol =  close_reduced_sol_with_fee  - calc_result.required_sol - calc_result.fee_sol ;


        // 3. 归还借币池 向 curve_account.borrow_token_reserve 加上 close_order.borrow_amount
        ctx.accounts.curve_account.borrow_token_reserve = ctx
            .accounts
            .curve_account
            .borrow_token_reserve
            .checked_add(close_margin_order.borrow_amount)
            .ok_or(ErrorCode::CloseShortRepaymentOverflow)?;

        // 把 盈利资金 sol转到, user_sol_account 账户
        transfer_pool_to_user_if_positive!(profit_sol, ctx);

        // 更新价格
        ctx.accounts.curve_account.price = calc_result.target_price;

        // 删除以前要对, 上下节点的  next_lp_sol_amount next_lp_token_amount 进行重新计算

        // 删除节点前，获取前后节点信息并重新计算流动性（删除后索引会变化）
        let prev_index = close_margin_order.prev_order;
        let next_index = close_margin_order.next_order;

        // 在删除节点前重新计算前节点的 next_lp_sol_amount 和 next_lp_token_amount
        // 有4种场景需要处理
        if prev_index != u16::MAX && next_index != u16::MAX {
            // 场景4: 前后都有节点 - 需要重新计算前节点到后节点之间的流动性
            // msg!("场景4: 删除中间节点，重新计算前节点(index={})到后节点(index={})的流动性", prev_index, next_index);

            let up_orderbook_info = ctx.accounts.up_orderbook.to_account_info();
            let orderbook_data = up_orderbook_info.data.borrow();

            // 获取前节点和后节点的价格及 order_id
            let prev_order = OrderBookManager::get_order(&orderbook_data, prev_index)?;
            let prev_order_id = prev_order.order_id;
            let prev_lock_lp_end_price = prev_order.lock_lp_end_price;

            let next_order = OrderBookManager::get_order(&orderbook_data, next_index)?;
            let next_lock_lp_start_price = next_order.lock_lp_start_price;

            drop(orderbook_data);

            // 使用 buy_from_price_to_price 计算新的流动性
            // up_orderbook 按价格从低到高排列，平仓是买入操作
            let (new_sol_amount, new_token_amount) = CurveAMM::buy_from_price_to_price(
                prev_lock_lp_end_price,
                next_lock_lp_start_price,
            ).ok_or(ErrorCode::CloseShortRemainingOverflow)?;

            // 使用 update_order 方法更新前节点
            let update_data = MarginOrderUpdateData {
                next_lp_sol_amount: Some(new_sol_amount),
                next_lp_token_amount: Some(new_token_amount),
                ..Default::default()
            };

            OrderBookManager::update_order(
                &up_orderbook_info,
                prev_index,
                prev_order_id,
                &update_data,
            )?;

            // msg!("前节点流动性已更新3: next_lp_sol={}, next_lp_token={}", new_sol_amount, new_token_amount);

        } else if prev_index != u16::MAX && next_index == u16::MAX {
            // 场景3: 只有前节点，没有后节点 - 前节点变成尾节点，设置无限流动性
            // msg!("场景3: 删除尾节点，前节点(index={})变为新尾节点，设置无限流动性", prev_index);

            let up_orderbook_info = ctx.accounts.up_orderbook.to_account_info();
            let orderbook_data = up_orderbook_info.data.borrow();

            let prev_order = OrderBookManager::get_order(&orderbook_data, prev_index)?;
            let prev_order_id = prev_order.order_id;

            drop(orderbook_data);

            // 使用 update_order 方法更新前节点
            let update_data = MarginOrderUpdateData {
                next_lp_sol_amount: Some(CurveAMM::MAX_U64),
                next_lp_token_amount: Some(CurveAMM::MAX_U64),
                ..Default::default()
            };

            OrderBookManager::update_order(
                &up_orderbook_info,
                prev_index,
                prev_order_id,
                &update_data,
            )?;

            // msg!("前节点流动性已设置为无限: next_lp_sol={}, next_lp_token={}", CurveAMM::MAX_U64, CurveAMM::MAX_U64);

        } else if prev_index == u16::MAX {
            // 场景2: 没有前节点 - 删除的是头节点，不需要处理
            // msg!("场景2: 删除头节点，无需更新流动性");
        } else {
            // 场景1: 前后都没节点 - 链表为空，不需要处理
            // msg!("场景1: 链表已空，无需更新流动性");
        }

        // 将当前订单索引加入清算列表，统一批量删除
        calc_result.liquidate_indices.push(close_margin_index);
        // msg!("全平仓：添加当前订单到清算列表 index={}, order_id={}", close_margin_index, close_order_id);


        // 检查是否需要应用手续费折扣
        crate::apply_fee_discount_if_needed!(ctx)?;

        // 发射全平仓事件
        emit!(FullCloseEvent {
            payer: ctx.accounts.payer.key(),
            user_sol_account: ctx.accounts.user_sol_account.key(),
            mint_account: ctx.accounts.mint_account.key(),
            is_close_long: false,
            final_token_amount: buy_token_amount,
            final_sol_amount: calc_result.required_sol,
            user_close_profit: profit_sol,
            latest_price: calc_result.target_price,
            order_id: close_margin_order.order_id,
            order_index: close_margin_index,
            liquidate_indices: calc_result.liquidate_indices.clone(),
        });

        // 不在这里关,移到函数最后了  8. 关闭平仓订单的PDA账户并退还租金
    } else {
        // msg!("------空单---部分平仓-------------");

        // 剩余 未还的token
        let remaining_token_borrow_amount = close_margin_order
            .borrow_amount
            .checked_sub(buy_token_amount)
            .ok_or(ErrorCode::CloseShortRemainingOverflow)?;

        // 计算关闭区间的流动性 
        let (close_end_price, close_reduced_sol) = CurveAMM::buy_from_price_with_token_output(
            close_margin_order.lock_lp_start_price,
            buy_token_amount,
        )
        .ok_or(ErrorCode::PriceCalculationError)?;

        // 买关闭区间 的token 需要的sol含手续费
        let close_reduced_sol_with_fee = CurveAMM::calculate_total_amount_with_fee(
            close_reduced_sol,
            close_margin_order.borrow_fee
        )
        .ok_or(ErrorCode::CloseShortFeeOverflow)?;

        // 计算剩余区间的流动性
        let (_remaining_close_end_price, remaining_close_reduced_sol) = CurveAMM::buy_from_price_with_token_output(
            close_end_price,
            remaining_token_borrow_amount, // 用 剩余 未还的token 来计算
        )
        .ok_or(ErrorCode::PriceCalculationError)?;

        // 买回剩余的token 需要的sol含手续费
        let remaining_close_reduced_sol_with_fee = CurveAMM::calculate_total_amount_with_fee(
            remaining_close_reduced_sol,
            close_margin_order.borrow_fee
        )
        .ok_or(ErrorCode::CloseShortFeeOverflow)?;

        // 买回剩余区间所用的手续费
        let _remaining_close_reduced_sol_fee = remaining_close_reduced_sol_with_fee
            .checked_sub(remaining_close_reduced_sol)  // -减 这次买回花的sol
            .ok_or(ErrorCode::CloseShortFeeOverflow)?;


        let profit_portion =  close_reduced_sol_with_fee  - calc_result.required_sol - calc_result.fee_sol ;
        //msg!("@@ 部份盈利资金 profit_portion :{}, ",profit_portion);

        //归还借币池
        ctx.accounts.curve_account.borrow_token_reserve = ctx
            .accounts
            .curve_account
            .borrow_token_reserve
            .checked_add(buy_token_amount)
            .ok_or(ErrorCode::CloseShortRepaymentOverflow)?;

        // 这两个值
        // close_margin_order.position_asset_amount
        // close_margin_order.margin_sol_amount
        // 说明, 这2值加起来 始终等于, remaining_close_reduced_sol_with_fee  未来买回需要的sol(加了手续费)
        let current_total = close_margin_order.position_asset_amount
            .checked_add(close_margin_order.margin_sol_amount)
            .ok_or(ErrorCode::CloseShortRemainingOverflow)?;

        // 计算调整后的 position_asset_amount 和 margin_sol_amount
        let (new_position_asset_amount, new_margin_sol_amount) = if current_total > remaining_close_reduced_sol_with_fee {
            let excess = current_total
                .checked_sub(remaining_close_reduced_sol_with_fee)
                .ok_or(ErrorCode::CloseShortRemainingOverflow)?;
            if close_margin_order.position_asset_amount >= excess {
                (
                    close_margin_order.position_asset_amount
                        .checked_sub(excess)
                        .ok_or(ErrorCode::CloseShortRemainingOverflow)?,
                    close_margin_order.margin_sol_amount
                )
            } else {
                (0, remaining_close_reduced_sol_with_fee)
            }
        } else {
            (close_margin_order.position_asset_amount, close_margin_order.margin_sol_amount)
        };

        // 验证最终结果
        let final_total = new_position_asset_amount
            .checked_add(new_margin_sol_amount)
            .ok_or(ErrorCode::CloseShortRemainingOverflow)?;

        // 确保最终总和等于目标值
        if final_total != remaining_close_reduced_sol_with_fee {
            // msg!("错误：最终分配不匹配目标值！差额={}",
            //     final_total.abs_diff(remaining_close_reduced_sol_with_fee));
            return Err(ErrorCode::CloseShortRemainingOverflow.into());
        }

        //msg!("========== B部分平仓盈亏计算完成 ==========");

        // 计算新的已实现盈利
        let new_realized_sol_amount = close_margin_order
            .realized_sol_amount
            .checked_add(profit_portion)
            .ok_or(ErrorCode::CloseShortProfitOverflow)?; // 累加已实现盈利 sol

        // 使用 update_order 方法一次性更新订单的所有字段
        let update_data = MarginOrderUpdateData {
            lock_lp_start_price: Some(close_end_price),
            lock_lp_token_amount: Some(remaining_token_borrow_amount),
            lock_lp_sol_amount: Some(remaining_close_reduced_sol),
            borrow_amount: Some(remaining_token_borrow_amount),
            position_asset_amount: Some(new_position_asset_amount),
            margin_sol_amount: Some(new_margin_sol_amount),
            realized_sol_amount: Some(new_realized_sol_amount),
            ..Default::default()
        };

        OrderBookManager::update_order(
            &ctx.accounts.up_orderbook.to_account_info(),
            close_margin_index,
            close_order_id,
            &update_data,
        )?;

        // 如果有前节点,那就需要对前节点的   next_lp_sol_amount next_lp_token_amount 进行重新计算
        let prev_index = close_margin_order.prev_order;

        if prev_index != u16::MAX {
            // 有前节点，需要重新计算前节点到当前订单之间的流动性
            // msg!("部分平仓: 重新计算前节点(index={})的流动性", prev_index);

            let up_orderbook_info = ctx.accounts.up_orderbook.to_account_info();
            let orderbook_data = up_orderbook_info.data.borrow();

            // 获取前节点的数据
            let prev_order = OrderBookManager::get_order(&orderbook_data, prev_index)?;
            let prev_order_id = prev_order.order_id;
            let prev_lock_lp_end_price = prev_order.lock_lp_end_price;

            drop(orderbook_data);

            // 使用 buy_from_price_to_price 计算前节点到当前订单(更新后)之间的流动性
            // 前节点的结束价格 -> close_end_price（当前订单部分平仓后的新起点价格）
            // 注意: 不能使用 close_margin_order.lock_lp_start_price，因为它还是旧值
            // close_end_price 是当前订单更新后的 lock_lp_start_price
            let (new_next_sol_amount, new_next_token_amount) = CurveAMM::buy_from_price_to_price(
                prev_lock_lp_end_price,  // 前节点的结束价格
                close_end_price,          // 当前订单更新后的起点价格(部分平仓后的新价格)
            ).ok_or(ErrorCode::CloseShortRemainingOverflow)?;

            // 更新前节点的 next_lp_sol_amount 和 next_lp_token_amount
            let prev_update_data = MarginOrderUpdateData {
                next_lp_sol_amount: Some(new_next_sol_amount),
                next_lp_token_amount: Some(new_next_token_amount),
                ..Default::default()
            };

            OrderBookManager::update_order(
                &up_orderbook_info,
                prev_index,
                prev_order_id,
                &prev_update_data,
            )?;

            // msg!("前节点流动性已更新4: next_lp_sol={}, next_lp_token={}", new_next_sol_amount, new_next_token_amount);
        } else {
            // 没有前节点(当前订单是链表头部)，不需要处理
            // msg!("部分平仓: 当前订单是链表头部，无需更新前节点流动性");
        }

        // 把 盈利资金 sol转到, user_sol_account 账户
        transfer_pool_to_user_if_positive!(profit_portion, ctx);

        // 更新价格
        ctx.accounts.curve_account.price = calc_result.target_price;
        //msg!("价格已更新为: {}", ctx.accounts.curve_account.price);

        // 检查是否需要应用手续费折扣
        crate::apply_fee_discount_if_needed!(ctx)?;

        // 发射部分平仓事件
        emit!(PartialCloseEvent {
            payer: ctx.accounts.payer.key(),
            user_sol_account: ctx.accounts.user_sol_account.key(),
            mint_account: ctx.accounts.mint_account.key(),
            is_close_long: false,
            final_token_amount: buy_token_amount,
            final_sol_amount: calc_result.required_sol,
            user_close_profit: profit_portion,
            latest_price: calc_result.target_price,
            order_id: close_margin_order.order_id,
            order_index: close_margin_index,
            // 部分平仓订单的参数(修改后的值) - 使用更新后的值
            order_type: close_margin_order.order_type,
            user: close_margin_order.user,
            lock_lp_start_price: close_end_price,  // 使用更新后的值
            lock_lp_end_price: close_margin_order.lock_lp_end_price,
            lock_lp_sol_amount: remaining_close_reduced_sol,  // 使用更新后的值
            lock_lp_token_amount: remaining_token_borrow_amount,  // 使用更新后的值
            start_time: close_margin_order.start_time,
            end_time: close_margin_order.end_time,
            //margin_init_sol_amount: close_margin_order.margin_init_sol_amount,
            margin_sol_amount: new_margin_sol_amount,  // 使用更新后的值
            borrow_amount: remaining_token_borrow_amount,  // 使用更新后的值
            position_asset_amount: new_position_asset_amount,  // 使用更新后的值
            borrow_fee: close_margin_order.borrow_fee,
            realized_sol_amount: new_realized_sol_amount,  // 使用更新后的值
            liquidate_indices: calc_result.liquidate_indices.clone(),
        });
        // msg!("空单部分平仓操作完成");
    }


    // 重新计算流动池储备量
    if let Some((sol_reserve, token_reserve)) =
        CurveAMM::price_to_reserves(ctx.accounts.curve_account.price)
    {
        ctx.accounts.curve_account.lp_sol_reserve = sol_reserve;
        ctx.accounts.curve_account.lp_token_reserve = token_reserve;
    } else {
        return Err(ErrorCode::CurveCalculationError.into());
    }

    // 【调试代码】检查资金是否足够扣除
    if ctx.accounts.pool_sol_account.lamports() < forced_liquidation_total_fees {
        return Err(ErrorCode::InsufficientPoolFunds.into());
    }
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

    if is_close_pda {
        // 如果是全平仓, 必须关闭平仓订单的PDA账户并退还租金
        // 现在没关pda 了
    }

    // ========== 在所有其他操作完成后，关闭平仓后的订单PDA账户并退还租金 ==========
    // 批量删除已平仓的订单
    if !calc_result.liquidate_indices.is_empty() {
        // 获取删除前的链表总数
        let orderbook_account_info = ctx.accounts.up_orderbook.to_account_info();
        let orderbook_data = orderbook_account_info.try_borrow_data()?;
        let header_before = crate::instructions::orderbook_manager::OrderBookManager::load_orderbook_header(&orderbook_data)?;
        let total_before = header_before.total;
        drop(orderbook_data); // 释放借用，避免冲突

        let delete_count = calc_result.liquidate_indices.len();
        // msg!("批量删除前: 链表总数={}, 待删除数量={}", total_before, delete_count);

        // 执行批量删除
        crate::instructions::orderbook_manager::OrderBookManager::batch_remove_by_indices_unsafe(
            &orderbook_account_info,
            &calc_result.liquidate_indices,
            &ctx.accounts.payer.to_account_info(),
            &ctx.accounts.system_program.to_account_info(),
        )?;

        // 获取删除后的链表总数
        let orderbook_data = orderbook_account_info.try_borrow_data()?;
        let header_after = crate::instructions::orderbook_manager::OrderBookManager::load_orderbook_header(&orderbook_data)?;
        let total_after = header_after.total;
        drop(orderbook_data);

        let expected_total = total_before.checked_sub(delete_count as u16)
            .ok_or(ErrorCode::ArithmeticOverflow)?;

        // msg!("批量删除后: 链表总数={}, 期望总数={}", total_after, expected_total);

        // 验证删除后的总数是否正确
        if total_after != expected_total {
            // msg!("❌ 链表删除计数异常! 删除前={}, 删除数={}, 期望剩余={}, 实际剩余={}, 差值={}",
            //     total_before, delete_count, expected_total, total_after,
            //     if total_after > expected_total {
            //         total_after - expected_total
            //     } else {
            //         expected_total - total_after
            //     }
            // );
            return err!(ErrorCode::LinkedListDeleteCountMismatch);
        }

        // msg!("✓ 批量删除订单完成，计数验证通过");
    }


    Ok(())
}
