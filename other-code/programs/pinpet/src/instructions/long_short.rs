// 导入所需的模块和依赖项
use {
    // 导入常量
    crate::constants::{MIN_MARGIN_SOL_AMOUNT, MIN_STOP_LOSS_PERCENT, MIN_TRADE_TOKEN_AMOUNT, MAX_CLOSE_INSERT_INDICES},
    // 导入曲线计算模块
    crate::curve::curve_amm::CurveAMM,
    // 导入错误处理
    crate::error::ErrorCode,
    // 导入上下文验证函数
    crate::instructions::context_validator::validate_trade_long_short_context,
    crate::instructions::contexts::TradeLongShort,
    crate::instructions::events::LongShortEvent,
    // 导入订单簿管理器
    crate::instructions::orderbook_manager::OrderBookManager,
    // 导入转移平仓手续费宏
    crate::transfer_close_fees_split,
    crate::instructions::utils::calculate_fee_split,

    crate::instructions::trade_engine::{buy_amounts, sell_amounts},
    // 导入 Anchor 框架的基础组件
    anchor_lang::prelude::*,
};

// 保证金做多交易指令的处理函数
pub fn long_trade(
    ctx: Context<TradeLongShort>,
    buy_token_amount: u64,          // 希望买入的token数量 (确定值)
    max_sol_amount: u64,            // 愿意给出的最大sol数量 (实际有可能会少点)
    margin_sol_max: u64,            // 最大保证金数量 (SOL)
    close_price: u128,              // 平仓价格
    close_insert_indices: Vec<u16>, // 平仓时插入订单簿的位置索引 u16::MAX代表插入到最前面 (可以有多个,是防止我们需要的位置刚好被删除,就会自动找第二位置)
) -> Result<()> {
    // 输出函数名和关键参数
    // msg!("-处理保证金做多交易-");

    validate_trade_long_short_context(&ctx)?;


    // 验证 close_insert_indices 数量
    if close_insert_indices.is_empty() {
        return Err(ErrorCode::EmptyCloseInsertIndices.into());
    }
    if close_insert_indices.len() > MAX_CLOSE_INSERT_INDICES {
        return Err(ErrorCode::TooManyCloseInsertIndices.into());
    }

    // 打印一下 close_insert_indices 方便调试
    // msg!("close_insert_indices: {:#?}", close_insert_indices);

    // 验证交易量是否满足最小交易量
    if buy_token_amount < MIN_TRADE_TOKEN_AMOUNT {
        return Err(ErrorCode::InsufficientTradeAmount.into());
    }

    // 验证止损价格：做多时止损价必须低于当前价格的97%（减去3%）
    let current_price = ctx.accounts.curve_account.price;
    let min_stop_price = current_price
        .checked_mul(
            100u128
                .checked_sub(MIN_STOP_LOSS_PERCENT as u128)
                .ok_or(ErrorCode::LongPriceCalculationOverflow)?,
        )
        .ok_or(ErrorCode::LongPriceCalculationOverflow)?
        .checked_div(100)
        .ok_or(ErrorCode::LongPriceCalculationOverflow)?;

    if close_price >= min_stop_price {
        // msg!(
        //     "止损价格验证失败: 当前价格={}, 最小止损价格={}, 输入止损价格={}",
        //     current_price,
        //     min_stop_price,
        //     close_price
        // );
        return Err(ErrorCode::InvalidStopLossPrice.into());
    }

    // 当前手续费(现货与保证金交易是不一样的)
    let fee = ctx.accounts.curve_account.borrow_fee;

    // 调用辅助函数计算交易数量
    let calc_buy_result = buy_amounts(
        &mut ctx.accounts.curve_account,
        &ctx.accounts.up_orderbook.to_account_info(),
        None, // pass_order 参数，目前传入空值
        buy_token_amount,
        max_sol_amount,
        fee,
    )?;

    // 实际需要借出的SOL数量
    let required_borrow_sol = calc_buy_result.required_sol;

    // 打印计算结果
    // msg!(
    //     "买入计算结果: 需要借出SOL={}, 获得代币={}",
    //     required_borrow_sol,
    //     calc_buy_result.output_token
    // );

    // 检查 borrow_sol 是否大于 BorrowingBondingCurve的 borrow_sol_reserve
    if required_borrow_sol > ctx.accounts.curve_account.borrow_sol_reserve {
        let _diff = required_borrow_sol
            .checked_sub(ctx.accounts.curve_account.borrow_sol_reserve)
            .unwrap_or(0);
        // msg!(
        //     "错误: 借款请求超过可用储备! 请求金额={}, 可用储备={}, 差额={}",
        //     required_borrow_sol,
        //     ctx.accounts.curve_account.borrow_sol_reserve,
        //     diff
        // );
        return Err(ErrorCode::InsufficientBorrowingReserve.into());
    }

    // 计算未来平仓时可能获得的SOL数量
    let future_sell_result = CurveAMM::sell_from_price_with_token_input(
        close_price,                  // 以平仓价格为起点
        calc_buy_result.output_token, // 卖出刚买入的所有代币
    );

    // 如果计算失败，直接返回错误
    let (close_end_price, close_output_sol) =
        future_sell_result.ok_or(ErrorCode::LongPriceCalculationOverflow)?;

    // 计算平仓时扣除手续费后的剩余金额
    let close_output_sol_after_fee = CurveAMM::calculate_amount_after_fee(close_output_sol, fee)
        .ok_or(ErrorCode::LongFeeCalculationOverflow)?;

    // 当下手续费计算 - 使用买入交易产生的实际手续费
    let fee_sol = calc_buy_result.fee_sol;

    let close_required_tokens = calc_buy_result.output_token;

    // 计算精确的,真实收的保证金的数量  等于   买币的总花费 (SOL) - 强制平仓时扣除手续费后的剩余金额
    let real_margin_sol = calc_buy_result
        .required_sol
        // .checked_add(calc_buy_result.fee_sol)
        // .ok_or(ErrorCode::LongMarginCalculationOverflow)?
        .checked_sub(close_output_sol_after_fee)
        .ok_or(ErrorCode::LongMarginCalculationOverflow)?;

    // msg!(
    //     "保证金计算详情: required_sol={}, close_output_sol_after_fee={}, real_margin_sol={}, MIN_MARGIN={}",
    //     calc_buy_result.required_sol,
    //     close_output_sol_after_fee,
    //     real_margin_sol,
    //     MIN_MARGIN_SOL_AMOUNT
    // );

    // 检查保证金是否满足最小限制
    if real_margin_sol < MIN_MARGIN_SOL_AMOUNT {
        // msg!(
        //     "错误: 保证金不足! real_margin_sol={} < MIN_MARGIN_SOL_AMOUNT={}",
        //     real_margin_sol,
        //     MIN_MARGIN_SOL_AMOUNT
        // );
        return Err(ErrorCode::InsufficientMinimumMargin.into());
    }


    // msg!("real_margin_sol={},margin_sol_max={}",real_margin_sol,margin_sol_max);
    // 计算输入的保证金是否足够
    if real_margin_sol > margin_sol_max {
        return Err(ErrorCode::InsufficientMargin.into());
    }

    // 再次验证, 要确保,保证金+ 扣除手续费后的平仓收益大于借款金额
    let total_repayment = real_margin_sol
        .checked_add(close_output_sol_after_fee)
        .ok_or(ErrorCode::LongMarginCalculationOverflow)?;

    if total_repayment < required_borrow_sol {
        // 如果收益不足以偿还借款，返回错误
        return Err(ErrorCode::InsufficientRepayment.into());
    }

    let forced_liquidation_total_fees = calc_buy_result.liquidate_fee_sol;

    // 生成新的 MarginOrder 定单, 并插入到  down_orderbook 中去

    // 获取当前时间戳和计算到期时间
    let now = Clock::get()?.unix_timestamp as u32;
    let deadline = now
        .checked_add(ctx.accounts.curve_account.borrow_duration as u32)
        .ok_or(ErrorCode::DeadlineCalculationOverflow)?;

    // 创建新的 MarginOrder 实例
    let new_margin_order = crate::instructions::structs::MarginOrder {
        // ========== 32-byte 对齐字段 (Pubkey) ==========
        // 用户公钥
        user: ctx.accounts.payer.key(),

        // ========== 16-byte 对齐字段 (u128) ==========
        // 锁定流动池区间开始价格
        lock_lp_start_price: close_price,
        // 锁定流动池区间结束价格
        lock_lp_end_price: close_end_price,
        // 开仓价格
        open_price: ctx.accounts.curve_account.price,

        // ========== 8-byte 对齐字段 (u64) ==========
        // 订单ID (暂时设置为0，插入时由订单簿分配)
        order_id: 0,
        // 锁定的SOL数量
        lock_lp_sol_amount: close_output_sol,
        // 锁定的Token数量
        lock_lp_token_amount: close_required_tokens,
        // 到下个节点的流动池区间 SOL 数量 (初始化时设为0)
        next_lp_sol_amount: 0,
        // 到下个节点的流动池区间 Token 数量 (初始化时设为0)
        next_lp_token_amount: 0,
        // 初始保证金SOL数量
        margin_init_sol_amount: real_margin_sol,
        // 当前保证金SOL数量
        margin_sol_amount: real_margin_sol,
        // 借款数量（做多借SOL）
        borrow_amount: required_borrow_sol,
        // 持仓资产数量（做多持有Token）
        position_asset_amount: close_required_tokens,
        // 已实现的SOL利润 (初始为0)
        realized_sol_amount: 0,

        // ========== 4-byte 对齐字段 (u32) ==========
        // 订单版本号 (初始为1)
        version: 1,
        // 开始时间
        start_time: now,
        // 到期时间
        end_time: deadline,

        // ========== 2-byte 对齐字段 (u16) ==========
        // 下一个订单索引 (初始化时设为u16::MAX，插入时更新)
        next_order: u16::MAX,
        // 上一个订单索引 (初始化时设为u16::MAX，插入时更新)
        prev_order: u16::MAX,
        // 借款手续费
        borrow_fee: fee,

        // ========== 1-byte 对齐字段 (u8) ==========
        // 订单类型: 1=做多(Down方向)
        order_type: 1,
        // 保留字段（对齐到结构体 32-byte 边界）
        _padding: [0; 13],
    };
    // msg!(
    //     "生成新的做多订单: 用户={}, 保证金={}, 借款={}, 持仓Token={}, 开仓价={}, 止损价={}",
    //     new_margin_order.user,
    //     new_margin_order.margin_sol_amount,
    //     new_margin_order.borrow_amount,
    //     new_margin_order.position_asset_amount,
    //     new_margin_order.open_price,
    //     new_margin_order.lock_lp_start_price
    // );

    // 遍历 close_insert_indices，找到一个正确的位置后, 尝试插入订单
    // msg!(
    //     "开始遍历插入位置索引，共 {} 个候选位置",
    //     close_insert_indices.len()
    // );
    // 读取 down 上的头信息 load_orderbook_header
    let down_orderbook_info = ctx.accounts.down_orderbook.to_account_info();
    let down_orderbook_header_total = {
        let down_orderbook_data = down_orderbook_info.data.borrow();
        let header = OrderBookManager::load_orderbook_header(&down_orderbook_data)?;
        header.total
    }; // down_orderbook_data 在这里自动 drop，释放借用

    // msg!("down_orderbook总订单数: {}", down_orderbook_header_total);

    // 插入成功标记
    let mut insert_ok = false;
    // 保存实际分配的 order_id
    let mut actual_order_id: u64 = 0;

    for (idx, &insert_index) in close_insert_indices.iter().enumerate() {
        // msg!(
        //     "尝试第 {} 个插入位置: index={}, 订单类型={}, 止损价={}",
        //     idx + 1,
        //     insert_index,
        //     new_margin_order.order_type,
        //     new_margin_order.lock_lp_start_price
        // );
        if (insert_index != u16::MAX && insert_index >= down_orderbook_header_total) {
            // msg!("插入位置超出总订单数，跳过");
            continue;
        }
        // 查找插入位置前后的订单（每次循环重新借用）
        let neighbors_result = {
            let down_orderbook_data = down_orderbook_info.data.borrow();
            OrderBookManager::get_insert_neighbors(&down_orderbook_data, insert_index)?
        }; // down_orderbook_data 在这里自动 drop

        // 识别并打印返回结果的各种可能，并执行相应的插入操作
        match neighbors_result {
            (None, None) => {
                // 情况1: 空订单簿
                // msg!("情况1: 订单簿为空 - prev=None, next=None, 将作为第一个订单插入");

                // 设置 next_lp_sol_amount 和 next_lp_token_amount 为 MAX_U64
                // 因为后面是无限空间
                let mut order_to_insert = new_margin_order.clone();
                order_to_insert.next_lp_sol_amount = CurveAMM::MAX_U64;
                order_to_insert.next_lp_token_amount = CurveAMM::MAX_U64;

                // 直接插入到空链表中（insert_after会处理空链表情况，使用0作为索引）
                // 对于空链表，insert_after 内部会自动创建第一个节点
                let (_insert_index, returned_order_id) = OrderBookManager::insert_after(
                    &ctx.accounts.down_orderbook.to_account_info(),
                    0, // 空链表时这个值不重要，函数内部会处理
                    &order_to_insert,
                    &ctx.accounts.payer.to_account_info(),
                    &ctx.accounts.system_program.to_account_info(),
                )?;

                actual_order_id = returned_order_id;
                // msg!("成功插入订单到空订单簿, order_id={}", actual_order_id);
                insert_ok = true;
                break; // 插入成功，退出循环
            }
            (None, Some(next_idx)) => {
                // 情况2: 插入到头部 (insert_pos == u16::MAX)
                // msg!(
                //     "情况2: 插入到链表头部 - prev=None, next={}, 将成为新的头节点",
                //     next_idx
                // );

                // 获取next节点的数据，检查价格区间是否重叠
                let next_order = {
                    let down_orderbook_data = down_orderbook_info.data.borrow();
                    *OrderBookManager::get_order(&down_orderbook_data, next_idx)?
                }; // 复制订单数据，然后释放借用

                // down_orderbook上：lock_lp_start_price > lock_lp_end_price（价格下跌）
                // 新订单的end_price 必须 >= next订单的start_price（新订单在更高价格）
                // 检查是否有重叠：new.end >= next.start && new.start <= next.end
                let has_overlap = new_margin_order.lock_lp_end_price
                    <= next_order.lock_lp_start_price;

                if has_overlap {
                    // msg!(
                    //     "价格区间重叠检测失败1: new[{}, {}] 与 next[{}, {}] 存在重叠",
                    //     new_margin_order.lock_lp_start_price,
                    //     new_margin_order.lock_lp_end_price,
                    //     next_order.lock_lp_start_price,
                    //     next_order.lock_lp_end_price
                    // );
                    continue; // 重叠，尝试下一个插入位置
                }

                // 计算新订单到next节点的区间流动性
                // 使用 sell_from_price_to_price 计算从 new.lock_lp_end_price 到 next.lock_lp_start_price
                let (next_lp_token, next_lp_sol) = CurveAMM::sell_from_price_to_price(
                    new_margin_order.lock_lp_end_price,
                    next_order.lock_lp_start_price,
                )
                .ok_or(ErrorCode::LongPriceCalculationOverflow)?;

                // 创建要插入的订单副本并设置流动性
                let mut order_to_insert = new_margin_order.clone();
                order_to_insert.next_lp_sol_amount = next_lp_sol;
                order_to_insert.next_lp_token_amount = next_lp_token;

                // msg!(
                //     "计算得到区间流动性: next_lp_sol_amount={}, next_lp_token_amount={}",
                //     next_lp_sol,
                //     next_lp_token
                // );

                // 插入到next节点之前
                let (_insert_index, returned_order_id) = OrderBookManager::insert_before(
                    &ctx.accounts.down_orderbook.to_account_info(),
                    next_idx,
                    &order_to_insert,
                    &ctx.accounts.payer.to_account_info(),
                    &ctx.accounts.system_program.to_account_info(),
                )?;

                actual_order_id = returned_order_id;
                // msg!("成功插入订单到链表头部, order_id={}", actual_order_id);
                insert_ok = true;
                break; // 插入成功，退出循环
            }
            (Some(prev_idx), None) => {
                // 情况3: 插入到尾部
                // msg!(
                //     "情况3: 插入到链表尾部 - prev={}, next=None, 将成为新的尾节点",
                //     prev_idx
                // );

                // 获取prev节点的数据，检查价格区间是否重叠
                let prev_order = {
                    let down_orderbook_data = down_orderbook_info.data.borrow();
                    *OrderBookManager::get_order(&down_orderbook_data, prev_idx)?
                }; // 复制订单数据，然后释放借用

                // down_orderbook上：新订单的start_price 必须 < prev订单的end_price（新订单在更低价格）
                // 检查是否有重叠
                let has_overlap = new_margin_order.lock_lp_start_price
                    >= prev_order.lock_lp_end_price;

                if has_overlap {
                    // msg!(
                    //     "价格区间重叠检测失败2: new[{}, {}] 与 prev[{}, {}] 存在重叠",
                    //     new_margin_order.lock_lp_start_price,
                    //     new_margin_order.lock_lp_end_price,
                    //     prev_order.lock_lp_start_price,
                    //     prev_order.lock_lp_end_price
                    // );
                    continue; // 重叠，尝试下一个插入位置
                }


                // 计算prev节点到新订单的区间流动性
                // 使用 sell_from_price_to_price 计算从 prev.lock_lp_end_price 到 new.lock_lp_start_price
                let (prev_next_lp_token, prev_next_lp_sol) = CurveAMM::sell_from_price_to_price(
                    prev_order.lock_lp_end_price,
                    new_margin_order.lock_lp_start_price,
                )
                .ok_or(ErrorCode::LongPriceCalculationOverflow)?;

                // msg!(
                //     "计算prev到new的区间流动性: prev_next_lp_sol_amount={}, prev_next_lp_token_amount={}",
                //     prev_next_lp_sol,
                //     prev_next_lp_token
                // );

                // 更新prev节点的 next_lp_sol_amount 和 next_lp_token_amount
                OrderBookManager::update_order(
                    &ctx.accounts.down_orderbook.to_account_info(),
                    prev_idx,
                    prev_order.order_id,
                    &crate::instructions::orderbook_manager::MarginOrderUpdateData {
                        next_lp_sol_amount: Some(prev_next_lp_sol),
                        next_lp_token_amount: Some(prev_next_lp_token),
                        lock_lp_start_price: None,
                        lock_lp_end_price: None,
                        lock_lp_sol_amount: None,
                        lock_lp_token_amount: None,
                        end_time: None,
                        margin_init_sol_amount: None,
                        margin_sol_amount: None,
                        borrow_amount: None,
                        position_asset_amount: None,
                        borrow_fee: None,
                        open_price: None,
                        realized_sol_amount: None,
                    },
                )?;

                // msg!("已更新prev节点的区间流动性");

                // 设置新订单的 next_lp_sol_amount 和 next_lp_token_amount 为 MAX_U64
                // 因为后面是无限空间
                let mut order_to_insert = new_margin_order.clone();
                order_to_insert.next_lp_sol_amount = CurveAMM::MAX_U64;
                order_to_insert.next_lp_token_amount = CurveAMM::MAX_U64;

                // 插入到prev节点之后
                let (_insert_index, returned_order_id) = OrderBookManager::insert_after(
                    &ctx.accounts.down_orderbook.to_account_info(),
                    prev_idx,
                    &order_to_insert,
                    &ctx.accounts.payer.to_account_info(),
                    &ctx.accounts.system_program.to_account_info(),
                )?;

                actual_order_id = returned_order_id;
                // msg!("成功插入订单到链表尾部, order_id={}", actual_order_id);
                insert_ok = true;
                break; // 插入成功，退出循环
            }
            (Some(prev_idx), Some(next_idx)) => {
                // 情况4: 插入到中间
                // msg!(
                //     "情况4: 插入到链表中间 - prev={}, next={}, 将插入到两个订单之间",
                //     prev_idx,
                //     next_idx
                // );

                // 获取prev和next节点的数据，检查价格区间是否重叠
                let (prev_order, next_order) = {
                    let down_orderbook_data = down_orderbook_info.data.borrow();
                    let prev = *OrderBookManager::get_order(&down_orderbook_data, prev_idx)?;
                    let next = *OrderBookManager::get_order(&down_orderbook_data, next_idx)?;
                    (prev, next)
                }; // 复制订单数据，然后释放借用

                // 检查与prev节点是否重叠
                let has_overlap_with_prev = new_margin_order.lock_lp_start_price
                    >= prev_order.lock_lp_end_price;

                // 检查与next节点是否重叠
                let has_overlap_with_next = new_margin_order.lock_lp_end_price
                    <= next_order.lock_lp_start_price;

                if has_overlap_with_prev || has_overlap_with_next {
                    // msg!(
                    //     "价格区间重叠检测失败3: new[{}, {}], prev[{}, {}], next[{}, {}]",
                    //     new_margin_order.lock_lp_start_price,
                    //     new_margin_order.lock_lp_end_price,
                    //     prev_order.lock_lp_start_price,
                    //     prev_order.lock_lp_end_price,
                    //     next_order.lock_lp_start_price,
                    //     next_order.lock_lp_end_price
                    // );
                    continue; // 重叠，尝试下一个插入位置
                }

                // 计算prev节点到新订单的区间流动性
                // 使用 sell_from_price_to_price 计算从 prev.lock_lp_end_price 到 new.lock_lp_start_price
                let (prev_next_lp_token, prev_next_lp_sol) = CurveAMM::sell_from_price_to_price(
                    prev_order.lock_lp_end_price,
                    new_margin_order.lock_lp_start_price,
                )
                .ok_or(ErrorCode::LongPriceCalculationOverflow)?;

                // msg!(
                //     "计算prev到new的区间流动性: prev_next_lp_sol_amount={}, prev_next_lp_token_amount={}",
                //     prev_next_lp_sol,
                //     prev_next_lp_token
                // );

                // 更新prev节点的 next_lp_sol_amount 和 next_lp_token_amount
                OrderBookManager::update_order(
                    &ctx.accounts.down_orderbook.to_account_info(),
                    prev_idx,
                    prev_order.order_id,
                    &crate::instructions::orderbook_manager::MarginOrderUpdateData {
                        next_lp_sol_amount: Some(prev_next_lp_sol),
                        next_lp_token_amount: Some(prev_next_lp_token),
                        lock_lp_start_price: None,
                        lock_lp_end_price: None,
                        lock_lp_sol_amount: None,
                        lock_lp_token_amount: None,
                        end_time: None,
                        margin_init_sol_amount: None,
                        margin_sol_amount: None,
                        borrow_amount: None,
                        position_asset_amount: None,
                        borrow_fee: None,
                        open_price: None,
                        realized_sol_amount: None,
                    },
                )?;

                // msg!("已更新prev节点的区间流动性");

                // 计算新订单到next节点的区间流动性
                // 使用 sell_from_price_to_price 计算从 new.lock_lp_end_price 到 next.lock_lp_start_price
                let (new_next_lp_token, new_next_lp_sol) = CurveAMM::sell_from_price_to_price(
                    new_margin_order.lock_lp_end_price,
                    next_order.lock_lp_start_price,
                )
                .ok_or(ErrorCode::LongPriceCalculationOverflow)?;

                // msg!(
                //     "计算new到next的区间流动性: new_next_lp_sol_amount={}, new_next_lp_token_amount={}",
                //     new_next_lp_sol,
                //     new_next_lp_token
                // );

                // 创建要插入的订单副本并设置流动性
                let mut order_to_insert = new_margin_order.clone();
                order_to_insert.next_lp_sol_amount = new_next_lp_sol;
                order_to_insert.next_lp_token_amount = new_next_lp_token;

                // 插入到prev节点之后（也就是next节点之前）
                let (_insert_index, returned_order_id) = OrderBookManager::insert_after(
                    &ctx.accounts.down_orderbook.to_account_info(),
                    prev_idx,
                    &order_to_insert,
                    &ctx.accounts.payer.to_account_info(),
                    &ctx.accounts.system_program.to_account_info(),
                )?;

                actual_order_id = returned_order_id;
                // msg!("成功插入订单到链表中间, order_id={}", actual_order_id);
                insert_ok = true;
                break; // 插入成功，退出循环
            }
        }
    }

    // 检查是否成功插入订单
    if !insert_ok {
        // msg!(
        //     "所有 {} 个候选插入位置均失败，无法找到合适的插入位置",
        //     close_insert_indices.len()
        // );
        return Err(ErrorCode::NoValidInsertPosition.into());
    }

    // 更新curve_account 借贷池的数据
    ctx.accounts.curve_account.borrow_sol_reserve = ctx
        .accounts
        .curve_account
        .borrow_sol_reserve
        .checked_sub(new_margin_order.borrow_amount)
        .ok_or(ErrorCode::LongBorrowCalculationOverflow)?;
    ctx.accounts.curve_account.price = calc_buy_result.target_price;

    // 检查是否需要应用手续费折扣
    crate::apply_fee_discount_if_needed!(ctx)?;

    // 进行保证金转账
    let margin_sol_amount = new_margin_order.margin_sol_amount;
    if margin_sol_amount > 0 {
        // 使用系统程序转账SOL
        anchor_lang::solana_program::program::invoke(
            &anchor_lang::solana_program::system_instruction::transfer(
                &ctx.accounts.payer.key(),
                &ctx.accounts.pool_sol_account.key(),
                margin_sol_amount,
            ),
            &[
                ctx.accounts.payer.to_account_info(),
                ctx.accounts.pool_sol_account.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;
    } else {
        //msg!("保证金0, 不需要转账");
    }

    // 手续费转账 - 使用新的分配逻辑
    if fee_sol > 0 {
        // 计算手续费分配
        let fee_split_result = calculate_fee_split(fee_sol, ctx.accounts.curve_account.fee_split)?;
        // 转账给合作伙伴
        if fee_split_result.partner_fee > 0 {
            let partner_fee_transfer_ctx = CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.payer.to_account_info(),
                    to: ctx.accounts.fee_recipient_account.to_account_info(),
                },
            );
            anchor_lang::system_program::transfer(
                partner_fee_transfer_ctx,
                fee_split_result.partner_fee,
            )?;
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
            anchor_lang::system_program::transfer(
                base_fee_transfer_ctx,
                fee_split_result.base_fee,
            )?;
        }
    } else {
        //msg!("手续费为0, 无需转账");
    }

    // 批量删除已平仓的订单
    if !calc_buy_result.liquidate_indices.is_empty() {
        // 获取删除前的链表总数
        let orderbook_account_info = ctx.accounts.up_orderbook.to_account_info();
        let orderbook_data = orderbook_account_info.try_borrow_data()?;
        let header_before = crate::instructions::orderbook_manager::OrderBookManager::load_orderbook_header(&orderbook_data)?;
        let total_before = header_before.total;
        drop(orderbook_data); // 释放借用，避免冲突

        let delete_count = calc_buy_result.liquidate_indices.len();
        // msg!("批量删除前: 链表总数={}, 待删除数量={}", total_before, delete_count);

        // 执行批量删除
        crate::instructions::orderbook_manager::OrderBookManager::batch_remove_by_indices_unsafe(
            &orderbook_account_info,
            &calc_buy_result.liquidate_indices,
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

    // 重新计算流动池储备量
    if let Some((sol_reserve, token_reserve)) =
        CurveAMM::price_to_reserves(ctx.accounts.curve_account.price)
    {
        ctx.accounts.curve_account.lp_sol_reserve = sol_reserve;
        ctx.accounts.curve_account.lp_token_reserve = token_reserve;
        //msg!("流动池储备量已根据价格重新计算: SOL={}, Token={}", sol_reserve, token_reserve);
    } else {
        return Err(ErrorCode::CurveCalculationError.into());
    }

    // 发出保证金做多交易事件
    emit!(LongShortEvent {
        payer: ctx.accounts.payer.key(),
        order_id: actual_order_id,
        mint_account: ctx.accounts.mint_account.key(),
        latest_price: ctx.accounts.curve_account.price,
        open_price: new_margin_order.open_price,
        order_type: new_margin_order.order_type,
        lock_lp_start_price: new_margin_order.lock_lp_start_price,
        lock_lp_end_price: new_margin_order.lock_lp_end_price,
        lock_lp_sol_amount: new_margin_order.lock_lp_sol_amount,
        lock_lp_token_amount: new_margin_order.lock_lp_token_amount,
        start_time: new_margin_order.start_time,
        end_time: new_margin_order.end_time,
        margin_sol_amount: new_margin_order.margin_sol_amount,
        borrow_amount: new_margin_order.borrow_amount,
        position_asset_amount: new_margin_order.position_asset_amount,
        borrow_fee: new_margin_order.borrow_fee,
        liquidate_indices: calc_buy_result.liquidate_indices.clone(),
    });

    // msg!("订单插入处理完成");

    // 返回成功结果
    Ok(())
}

pub fn short_trade(
    ctx: Context<TradeLongShort>,
    borrow_sell_token_amount: u64, // 希望借出并马上卖掉的token数量
    min_sol_output: u64,           // 卖出后最少得到的sol数量
    margin_sol_max: u64,           // 最大保证金数量 (SOL)
    close_price: u128,             // 平仓价格
    close_insert_indices: Vec<u16>, // 平仓时插入订单簿的位置索引 (可以有多个,是防止我们需要的位置刚好被删除,就会自动找第二位置)
) -> Result<()> {
    // msg!("-处理保证金做空交易-");

    validate_trade_long_short_context(&ctx)?;

    // 验证 close_insert_indices 数量
    if close_insert_indices.is_empty() {
        return Err(ErrorCode::EmptyCloseInsertIndices.into());
    }
    if close_insert_indices.len() > MAX_CLOSE_INSERT_INDICES {
        return Err(ErrorCode::TooManyCloseInsertIndices.into());
    }

    // 验证交易量是否满足最小交易量
    if borrow_sell_token_amount < MIN_TRADE_TOKEN_AMOUNT {
        return Err(ErrorCode::InsufficientTradeAmount.into());
    }

    // 验证止损价格：做空时止损价必须高于当前价格的103%（加上3%）
    let current_price = ctx.accounts.curve_account.price;
    let max_stop_price = current_price
        .checked_mul(
            100u128
                .checked_add(MIN_STOP_LOSS_PERCENT as u128)
                .ok_or(ErrorCode::ShortPriceCalculationOverflow)?,
        )
        .ok_or(ErrorCode::ShortPriceCalculationOverflow)?
        .checked_div(100)
        .ok_or(ErrorCode::ShortPriceCalculationOverflow)?;

    if close_price <= max_stop_price {
        return Err(ErrorCode::InvalidStopLossPrice.into());
    }

    // 当前手续费 基值
    let fee = ctx.accounts.curve_account.borrow_fee;
    // 检查 borrow_sell_token_amount 是否大于 BorrowingBondingCurve的 borrow_token_reserve
    if borrow_sell_token_amount > ctx.accounts.curve_account.borrow_token_reserve {
        let _diff = borrow_sell_token_amount
            .checked_sub(ctx.accounts.curve_account.borrow_token_reserve)
            .unwrap_or(0);
        return Err(ErrorCode::InsufficientBorrowingReserve.into());
    }

    // 调用辅助函数计算交易数量
    let calc_sell_result = sell_amounts(
        &mut ctx.accounts.curve_account,
        &ctx.accounts.down_orderbook.to_account_info(),
        None, // pass_order 参数，目前传入空值
        borrow_sell_token_amount,
        min_sol_output,
        fee,
    )?;

    // 1. 确保得到的 token 等于 borrow_sell_token_amount
    if calc_sell_result.sell_token != borrow_sell_token_amount {
        let _diff = borrow_sell_token_amount
            .checked_sub(calc_sell_result.sell_token)
            .unwrap_or(0);
        return Err(ErrorCode::InsufficientTokenSale.into());
    }
    // 2. 确保得到的 sol 大于或等于 min_sol_output
    if calc_sell_result.output_sol < min_sol_output {
        let _diff = min_sol_output
            .checked_sub(calc_sell_result.output_sol)
            .unwrap_or(0);
        return Err(ErrorCode::InsufficientSolOutput.into());
    }

    // 计算未来平仓时需要的SOL数量（买回代币的成本）
    let future_buy_result = CurveAMM::buy_from_price_with_token_output(
        close_price,                 // 以平仓价格为起点
        calc_sell_result.sell_token, // 买回刚卖出的所有代币
    );

    // 如果计算失败，直接返回错误
    let (close_end_price, close_cost_sol) =
        future_buy_result.ok_or(ErrorCode::CurveCalculationError)?;

    // 计算加上手续费用后的实际需要SOL
    let close_buy_sol_with_fee = CurveAMM::calculate_total_amount_with_fee(close_cost_sol, fee)
        .ok_or(ErrorCode::ShortFeeCalculationOverflow)?;

    // 计算真实保证金 ( 未来平仓时,含手结的费用 - 曾经卖币赚得到的sol(除开手续费后))
    let real_margin_sol = close_buy_sol_with_fee
        .checked_sub(calc_sell_result.output_sol)
        .ok_or(ErrorCode::ShortMarginCalculationOverflow)?
        .checked_sub(calc_sell_result.fee_sol)
        .ok_or(ErrorCode::ShortMarginCalculationOverflow)?;

    // msg!(
    //     "做空保证金计算: close_buy_sol_with_fee={}, output_sol={}, fee_sol={}, real_margin_sol={}, MIN_MARGIN={}",
    //     close_buy_sol_with_fee,
    //     calc_sell_result.output_sol,
    //     calc_sell_result.fee_sol,
    //     real_margin_sol,
    //     MIN_MARGIN_SOL_AMOUNT
    // );

    // 检查保证金是否满足最小限制
    if real_margin_sol < MIN_MARGIN_SOL_AMOUNT {
        // msg!(
        //     "错误: 做空保证金不足! real_margin_sol={} < MIN_MARGIN_SOL_AMOUNT={}",
        //     real_margin_sol,
        //     MIN_MARGIN_SOL_AMOUNT
        // );
        return Err(ErrorCode::InsufficientMinimumMargin.into());
    }
    // 检查保证金是否足
    if real_margin_sol > margin_sol_max {
        // msg!(
        //     "错误: 输入保证金不足! real_margin_sol={} > margin_sol_max={}",
        //     real_margin_sol,
        //     margin_sol_max
        // );
        return Err(ErrorCode::InsufficientMargin.into());
    }

    // 强制平仓手续费
    let forced_liquidation_total_fees = calc_sell_result.liquidate_fee_sol;

    // 生成新的 MarginOrder 定单, 并插入到 up_orderbook 中去

    // 获取当前时间戳和计算到期时间
    let now = Clock::get()?.unix_timestamp as u32;
    let deadline = now
        .checked_add(ctx.accounts.curve_account.borrow_duration as u32)
        .ok_or(ErrorCode::DeadlineCalculationOverflow)?;

    // msg!("borrow_sell_token_amount = {}", borrow_sell_token_amount);
    // 创建新的 MarginOrder 实例
    let new_margin_order = crate::instructions::structs::MarginOrder {
        // ========== 32-byte 对齐字段 (Pubkey) ==========
        // 用户公钥
        user: ctx.accounts.payer.key(),

        // ========== 16-byte 对齐字段 (u128) ==========
        // 锁定流动池区间开始价格
        lock_lp_start_price: close_price,
        // 锁定流动池区间结束价格
        lock_lp_end_price: close_end_price,
        // 开仓价格
        open_price: ctx.accounts.curve_account.price,

        // ========== 8-byte 对齐字段 (u64) ==========
        // 订单ID (暂时设置为0，插入时由订单簿分配)
        order_id: 0,
        // 锁定的SOL数量
        lock_lp_sol_amount: close_cost_sol,
        // 锁定的Token数量
        lock_lp_token_amount: calc_sell_result.sell_token,
        // 到下个节点的流动池区间 SOL 数量 (初始化时设为0)
        next_lp_sol_amount: 0,
        // 到下个节点的流动池区间 Token 数量 (初始化时设为0)
        next_lp_token_amount: 0,
        // 初始保证金SOL数量
        margin_init_sol_amount: real_margin_sol,
        // 当前保证金SOL数量
        margin_sol_amount: real_margin_sol,
        // 借款数量（做空借Token）
        borrow_amount: borrow_sell_token_amount,
        // 持仓资产数量（做空持有SOL）
        position_asset_amount: calc_sell_result.output_sol,
        // 已实现的SOL利润 (初始为0)
        realized_sol_amount: 0,

        // ========== 4-byte 对齐字段 (u32) ==========
        // 订单版本号 (初始为1)
        version: 1,
        // 开始时间
        start_time: now,
        // 到期时间
        end_time: deadline,

        // ========== 2-byte 对齐字段 (u16) ==========
        // 下一个订单索引 (初始化时设为u16::MAX，插入时更新)
        next_order: u16::MAX,
        // 上一个订单索引 (初始化时设为u16::MAX，插入时更新)
        prev_order: u16::MAX,
        // 借款手续费
        borrow_fee: fee,

        // ========== 1-byte 对齐字段 (u8) ==========
        // 订单类型: 2=做空(Up方向)
        order_type: 2,
        // 保留字段（对齐到结构体 32-byte 边界）
        _padding: [0; 13],
    };
    // msg!(
    //     "生成新的做空订单: 用户={}, 保证金={}, 借款Token={}, 持仓SOL={}, 开仓价={}, 止损价={}",
    //     new_margin_order.user,
    //     new_margin_order.margin_sol_amount,
    //     new_margin_order.borrow_amount,
    //     new_margin_order.position_asset_amount,
    //     new_margin_order.open_price,
    //     new_margin_order.lock_lp_start_price
    // );

    // 遍历 close_insert_indices，找到一个正确的位置后, 尝试插入订单
    // msg!(
    //     "开始遍历插入位置索引，共 {} 个候选位置",
    //     close_insert_indices.len()
    // );
    // 读取 up 上的头信息 load_orderbook_header
    let up_orderbook_info = ctx.accounts.up_orderbook.to_account_info();
    let up_orderbook_header_total = {
        let up_orderbook_data = up_orderbook_info.data.borrow();
        let header = OrderBookManager::load_orderbook_header(&up_orderbook_data)?;
        header.total
    }; // up_orderbook_data 在这里自动 drop，释放借用

    // msg!("up_orderbook总订单数: {}", up_orderbook_header_total);

    // 插入成功标记
    let mut insert_ok = false;
    // 保存实际分配的 order_id
    let mut actual_order_id: u64 = 0;

    for (idx, &insert_index) in close_insert_indices.iter().enumerate() {
        // msg!(
        //     "尝试第 {} 个插入位置: index={}, 订单类型={}, 止损价={}",
        //     idx + 1,
        //     insert_index,
        //     new_margin_order.order_type,
        //     new_margin_order.lock_lp_start_price
        // );
        if (insert_index != u16::MAX && insert_index >= up_orderbook_header_total) {
            // msg!("插入位置超出总订单数，跳过");
            continue;
        }
        // 查找插入位置前后的订单（每次循环重新借用）
        let neighbors_result = {
            let up_orderbook_data = up_orderbook_info.data.borrow();
            OrderBookManager::get_insert_neighbors(&up_orderbook_data, insert_index)?
        }; // up_orderbook_data 在这里自动 drop

        // 识别并打印返回结果的各种可能，并执行相应的插入操作
        match neighbors_result {
            (None, None) => {
                // 情况1: 空订单簿
                // msg!("情况1: 订单簿为空 - prev=None, next=None, 将作为第一个订单插入");

                // 设置 next_lp_sol_amount 和 next_lp_token_amount 为 MAX_U64
                // 因为后面是无限空间
                let mut order_to_insert = new_margin_order.clone();
                order_to_insert.next_lp_sol_amount = CurveAMM::MAX_U64;
                order_to_insert.next_lp_token_amount = CurveAMM::MAX_U64;

                // 直接插入到空链表中（insert_after会处理空链表情况，使用0作为索引）
                // 对于空链表，insert_after 内部会自动创建第一个节点
                let (_insert_index, returned_order_id) = OrderBookManager::insert_after(
                    &ctx.accounts.up_orderbook.to_account_info(),
                    0, // 空链表时这个值不重要，函数内部会处理
                    &order_to_insert,
                    &ctx.accounts.payer.to_account_info(),
                    &ctx.accounts.system_program.to_account_info(),
                )?;

                actual_order_id = returned_order_id;
                // msg!("成功插入订单到空订单簿, order_id={}", actual_order_id);
                insert_ok = true;
                break; // 插入成功，退出循环
            }
            (None, Some(next_idx)) => {
                // 情况2: 插入到头部 (insert_pos == u16::MAX)
                // msg!(
                //     "情况2: 插入到链表头部 - prev=None, next={}, 将成为新的头节点",
                //     next_idx
                // );

                // 获取next节点的数据，检查价格区间是否重叠
                let next_order = {
                    let up_orderbook_data = up_orderbook_info.data.borrow();
                    *OrderBookManager::get_order(&up_orderbook_data, next_idx)?
                }; // 复制订单数据，然后释放借用

                // up_orderbook上：lock_lp_start_price < lock_lp_end_price（价格上涨）
                // 新订单的end_price 必须 <= next订单的start_price（新订单在更低价格）
                // 检查是否有重叠：new.end > next.start 或 new.start > next.end
                let has_overlap = new_margin_order.lock_lp_end_price
                    > next_order.lock_lp_start_price
                    || new_margin_order.lock_lp_start_price > next_order.lock_lp_end_price;

                if has_overlap {
                    // msg!(
                    //     "价格区间重叠检测失败4: new[{}, {}] 与 next[{}, {}] 存在重叠",
                    //     new_margin_order.lock_lp_start_price,
                    //     new_margin_order.lock_lp_end_price,
                    //     next_order.lock_lp_start_price,
                    //     next_order.lock_lp_end_price
                    // );
                    continue; // 重叠，尝试下一个插入位置
                }

                // 检查连续性：new.lock_lp_end_price 应该 < next.lock_lp_start_price
                if new_margin_order.lock_lp_end_price >= next_order.lock_lp_start_price {
                    // msg!(
                    //     "价格区间连续性检测失败: new.end_price({}) >= next.start_price({})",
                    //     new_margin_order.lock_lp_end_price,
                    //     next_order.lock_lp_start_price
                    // );
                    continue; // 不连续，尝试下一个插入位置
                }

                // msg!(
                //     "价格区间验证通过3: new[{}, {}] 与 next[{}, {}] 无重叠且连续",
                //     new_margin_order.lock_lp_start_price,
                //     new_margin_order.lock_lp_end_price,
                //     next_order.lock_lp_start_price,
                //     next_order.lock_lp_end_price
                // );

                // 计算新订单到next节点的区间流动性
                // 使用 buy_from_price_to_price 计算从 new.lock_lp_end_price 到 next.lock_lp_start_price
                let (next_lp_sol, next_lp_token) = CurveAMM::buy_from_price_to_price(
                    new_margin_order.lock_lp_end_price,
                    next_order.lock_lp_start_price,
                )
                .ok_or(ErrorCode::ShortPriceCalculationOverflow)?;

                // 创建要插入的订单副本并设置流动性
                let mut order_to_insert = new_margin_order.clone();
                order_to_insert.next_lp_sol_amount = next_lp_sol;
                order_to_insert.next_lp_token_amount = next_lp_token;

                // msg!(
                //     "计算得到区间流动性: next_lp_sol_amount={}, next_lp_token_amount={}",
                //     next_lp_sol,
                //     next_lp_token
                // );

                // 插入到next节点之前
                let (_insert_index, returned_order_id) = OrderBookManager::insert_before(
                    &ctx.accounts.up_orderbook.to_account_info(),
                    next_idx,
                    &order_to_insert,
                    &ctx.accounts.payer.to_account_info(),
                    &ctx.accounts.system_program.to_account_info(),
                )?;

                actual_order_id = returned_order_id;
                // msg!("成功插入订单到链表头部, order_id={}", actual_order_id);
                insert_ok = true;
                break; // 插入成功，退出循环
            }
            (Some(prev_idx), None) => {
                // 情况3: 插入到尾部
                // msg!(
                //     "情况3: 插入到链表尾部 - prev={}, next=None, 将成为新的尾节点",
                //     prev_idx
                // );

                // 获取prev节点的数据，检查价格区间是否重叠
                let prev_order = {
                    let up_orderbook_data = up_orderbook_info.data.borrow();
                    *OrderBookManager::get_order(&up_orderbook_data, prev_idx)?
                }; // 复制订单数据，然后释放借用

                // up_orderbook上：新订单的start_price 必须 > prev订单的end_price（新订单在更高价格）
                // 检查是否有重叠：new.start < prev.end 或 new.end < prev.start
                let has_overlap = new_margin_order.lock_lp_start_price
                    < prev_order.lock_lp_end_price
                    || new_margin_order.lock_lp_end_price < prev_order.lock_lp_start_price;

                if has_overlap {
                    // msg!(
                    //     "价格区间重叠检测失败5: new[{}, {}] 与 prev[{}, {}] 存在重叠",
                    //     new_margin_order.lock_lp_start_price,
                    //     new_margin_order.lock_lp_end_price,
                    //     prev_order.lock_lp_start_price,
                    //     prev_order.lock_lp_end_price
                    // );
                    continue; // 重叠，尝试下一个插入位置
                }

                // 检查连续性：prev.lock_lp_end_price 应该 < new.lock_lp_start_price
                if prev_order.lock_lp_end_price >= new_margin_order.lock_lp_start_price {
                    // msg!(
                    //     "价格区间连续性检测失败: prev.end_price({}) >= new.start_price({})",
                    //     prev_order.lock_lp_end_price,
                    //     new_margin_order.lock_lp_start_price
                    // );
                    continue; // 不连续，尝试下一个插入位置
                }

                // msg!(
                //     "价格区间验证通过4: new[{}, {}] 与 prev[{}, {}] 无重叠且连续",
                //     new_margin_order.lock_lp_start_price,
                //     new_margin_order.lock_lp_end_price,
                //     prev_order.lock_lp_start_price,
                //     prev_order.lock_lp_end_price
                // );

                // 计算prev节点到新订单的区间流动性
                // 使用 buy_from_price_to_price 计算从 prev.lock_lp_end_price 到 new.lock_lp_start_price
                let (prev_next_lp_sol, prev_next_lp_token) = CurveAMM::buy_from_price_to_price(
                    prev_order.lock_lp_end_price,
                    new_margin_order.lock_lp_start_price,
                )
                .ok_or(ErrorCode::ShortPriceCalculationOverflow)?;

                // msg!(
                //     "计算prev到new的区间流动性: prev_next_lp_sol_amount={}, prev_next_lp_token_amount={}",
                //     prev_next_lp_sol,
                //     prev_next_lp_token
                // );

                // 更新prev节点的 next_lp_sol_amount 和 next_lp_token_amount
                OrderBookManager::update_order(
                    &ctx.accounts.up_orderbook.to_account_info(),
                    prev_idx,
                    prev_order.order_id,
                    &crate::instructions::orderbook_manager::MarginOrderUpdateData {
                        next_lp_sol_amount: Some(prev_next_lp_sol),
                        next_lp_token_amount: Some(prev_next_lp_token),
                        lock_lp_start_price: None,
                        lock_lp_end_price: None,
                        lock_lp_sol_amount: None,
                        lock_lp_token_amount: None,
                        end_time: None,
                        margin_init_sol_amount: None,
                        margin_sol_amount: None,
                        borrow_amount: None,
                        position_asset_amount: None,
                        borrow_fee: None,
                        open_price: None,
                        realized_sol_amount: None,
                    },
                )?;

                // msg!("已更新prev节点的区间流动性");

                // 设置新订单的 next_lp_sol_amount 和 next_lp_token_amount 为 MAX_U64
                // 因为后面是无限空间
                let mut order_to_insert = new_margin_order.clone();
                order_to_insert.next_lp_sol_amount = CurveAMM::MAX_U64;
                order_to_insert.next_lp_token_amount = CurveAMM::MAX_U64;

                // 插入到prev节点之后
                let (_insert_index, returned_order_id) = OrderBookManager::insert_after(
                    &ctx.accounts.up_orderbook.to_account_info(),
                    prev_idx,
                    &order_to_insert,
                    &ctx.accounts.payer.to_account_info(),
                    &ctx.accounts.system_program.to_account_info(),
                )?;

                actual_order_id = returned_order_id;
                // msg!("成功插入订单到链表尾部, order_id={}", actual_order_id);
                insert_ok = true;
                break; // 插入成功，退出循环
            }
            (Some(prev_idx), Some(next_idx)) => {
                // 情况4: 插入到中间
                // msg!(
                //     "情况4: 插入到链表中间 - prev={}, next={}, 将插入到两个订单之间",
                //     prev_idx,
                //     next_idx
                // );

                // 获取prev和next节点的数据，检查价格区间是否重叠
                let (prev_order, next_order) = {
                    let up_orderbook_data = up_orderbook_info.data.borrow();
                    let prev = *OrderBookManager::get_order(&up_orderbook_data, prev_idx)?;
                    let next = *OrderBookManager::get_order(&up_orderbook_data, next_idx)?;
                    (prev, next)
                }; // 复制订单数据，然后释放借用

                // 检查与prev节点是否重叠
                // up_orderbook上：new.start 必须 > prev.end
                let has_overlap_with_prev = new_margin_order.lock_lp_start_price
                    < prev_order.lock_lp_end_price
                    || new_margin_order.lock_lp_end_price < prev_order.lock_lp_start_price;

                // 检查与next节点是否重叠
                // up_orderbook上：new.end 必须 <= next.start
                let has_overlap_with_next = new_margin_order.lock_lp_end_price
                    > next_order.lock_lp_start_price
                    || new_margin_order.lock_lp_start_price > next_order.lock_lp_end_price;

                if has_overlap_with_prev || has_overlap_with_next {
                    // msg!(
                    //     "价格区间重叠检测失败6: new[{}, {}], prev[{}, {}], next[{}, {}]",
                    //     new_margin_order.lock_lp_start_price,
                    //     new_margin_order.lock_lp_end_price,
                    //     prev_order.lock_lp_start_price,
                    //     prev_order.lock_lp_end_price,
                    //     next_order.lock_lp_start_price,
                    //     next_order.lock_lp_end_price
                    // );
                    continue; // 重叠，尝试下一个插入位置
                }

                // 检查连续性：new.lock_lp_end_price 应该 < next.lock_lp_start_price
                if new_margin_order.lock_lp_end_price >= next_order.lock_lp_start_price {
                    // msg!(
                    //     "价格区间连续性检测失败: new.end_price({}) >= next.start_price({})",
                    //     new_margin_order.lock_lp_end_price,
                    //     next_order.lock_lp_start_price
                    // );
                    continue; // 不连续，尝试下一个插入位置
                }

                // 检查连续性：prev.lock_lp_end_price 应该 < new.lock_lp_start_price
                if prev_order.lock_lp_end_price >= new_margin_order.lock_lp_start_price {
                    // msg!(
                    //     "价格区间连续性检测失败: prev.end_price({}) >= new.start_price({})",
                    //     prev_order.lock_lp_end_price,
                    //     new_margin_order.lock_lp_start_price
                    // );
                    continue; // 不连续，尝试下一个插入位置
                }

                // msg!(
                //     "价格区间验证通过5: new[{}, {}] 与 prev[{}, {}] 和 next[{}, {}] 均无重叠且连续",
                //     new_margin_order.lock_lp_start_price,
                //     new_margin_order.lock_lp_end_price,
                //     prev_order.lock_lp_start_price,
                //     prev_order.lock_lp_end_price,
                //     next_order.lock_lp_start_price,
                //     next_order.lock_lp_end_price
                // );

                // 计算prev节点到新订单的区间流动性
                // 使用 buy_from_price_to_price 计算从 prev.lock_lp_end_price 到 new.lock_lp_start_price
                let (prev_next_lp_sol, prev_next_lp_token) = CurveAMM::buy_from_price_to_price(
                    prev_order.lock_lp_end_price,
                    new_margin_order.lock_lp_start_price,
                )
                .ok_or(ErrorCode::ShortPriceCalculationOverflow)?;

                // msg!(
                //     "计算prev到new的区间流动性: prev_next_lp_sol_amount={}, prev_next_lp_token_amount={}",
                //     prev_next_lp_sol,
                //     prev_next_lp_token
                // );

                // 更新prev节点的 next_lp_sol_amount 和 next_lp_token_amount
                OrderBookManager::update_order(
                    &ctx.accounts.up_orderbook.to_account_info(),
                    prev_idx,
                    prev_order.order_id,
                    &crate::instructions::orderbook_manager::MarginOrderUpdateData {
                        next_lp_sol_amount: Some(prev_next_lp_sol),
                        next_lp_token_amount: Some(prev_next_lp_token),
                        lock_lp_start_price: None,
                        lock_lp_end_price: None,
                        lock_lp_sol_amount: None,
                        lock_lp_token_amount: None,
                        end_time: None,
                        margin_init_sol_amount: None,
                        margin_sol_amount: None,
                        borrow_amount: None,
                        position_asset_amount: None,
                        borrow_fee: None,
                        open_price: None,
                        realized_sol_amount: None,
                    },
                )?;

                // msg!("已更新prev节点的区间流动性");

                // 计算新订单到next节点的区间流动性
                // 使用 buy_from_price_to_price 计算从 new.lock_lp_end_price 到 next.lock_lp_start_price
                let (new_next_lp_sol, new_next_lp_token) = CurveAMM::buy_from_price_to_price(
                    new_margin_order.lock_lp_end_price,
                    next_order.lock_lp_start_price,
                )
                .ok_or(ErrorCode::ShortPriceCalculationOverflow)?;

                // msg!(
                //     "计算new到next的区间流动性: new_next_lp_sol_amount={}, new_next_lp_token_amount={}",
                //     new_next_lp_sol,
                //     new_next_lp_token
                // );

                // 创建要插入的订单副本并设置流动性
                let mut order_to_insert = new_margin_order.clone();
                order_to_insert.next_lp_sol_amount = new_next_lp_sol;
                order_to_insert.next_lp_token_amount = new_next_lp_token;

                // 插入到prev节点之后（也就是next节点之前）
                let (_insert_index, returned_order_id) = OrderBookManager::insert_after(
                    &ctx.accounts.up_orderbook.to_account_info(),
                    prev_idx,
                    &order_to_insert,
                    &ctx.accounts.payer.to_account_info(),
                    &ctx.accounts.system_program.to_account_info(),
                )?;

                actual_order_id = returned_order_id;
                // msg!("成功插入订单到链表中间, order_id={}", actual_order_id);
                insert_ok = true;
                break; // 插入成功，退出循环
            }
        }
    }

    // 检查是否成功插入订单
    if !insert_ok {
        // msg!(
        //     "所有 {} 个候选插入位置均失败，无法找到合适的插入位置",
        //     close_insert_indices.len()
        // );
        return Err(ErrorCode::NoValidInsertPosition.into());
    }

    // 更新curve_account的数据
    ctx.accounts.curve_account.borrow_token_reserve = ctx
        .accounts
        .curve_account
        .borrow_token_reserve
        .checked_sub(new_margin_order.borrow_amount)
        .ok_or(ErrorCode::ShortBorrowCalculationOverflow)?;

    ctx.accounts.curve_account.price = calc_sell_result.target_price;

    // 检查是否需要应用手续费折扣
    crate::apply_fee_discount_if_needed!(ctx)?;

    // 进行保证金转账
    let margin_sol_amount = new_margin_order.margin_sol_amount;

    // 使用系统程序转账SOL
    anchor_lang::solana_program::program::invoke(
        &anchor_lang::solana_program::system_instruction::transfer(
            &ctx.accounts.payer.key(),
            &ctx.accounts.pool_sol_account.key(),
            margin_sol_amount,
        ),
        &[
            ctx.accounts.payer.to_account_info(),
            ctx.accounts.pool_sol_account.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
    )?;

    // 手续费转账 - 使用新的分配逻辑
    let fee_sol = calc_sell_result.fee_sol;
    if fee_sol > 0 {
        // 计算手续费分配
        let fee_split_result = calculate_fee_split(fee_sol, ctx.accounts.curve_account.fee_split)?;

        // 转账给合作伙伴
        if fee_split_result.partner_fee > 0 {
            let partner_fee_transfer_ctx = CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.payer.to_account_info(),
                    to: ctx.accounts.fee_recipient_account.to_account_info(),
                },
            );
            anchor_lang::system_program::transfer(
                partner_fee_transfer_ctx,
                fee_split_result.partner_fee,
            )?;
            // msg!(
            //     "已转移 {} SOL 作为合作伙伴手续费到地址: {}",
            //     fee_split_result.partner_fee,
            //     ctx.accounts.fee_recipient_account.key()
            // );
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
            anchor_lang::system_program::transfer(
                base_fee_transfer_ctx,
                fee_split_result.base_fee,
            )?;
        }
    } else {
        //msg!("手续费为0, 无需转账");
    }

    // 批量删除已平仓的订单
    if !calc_sell_result.liquidate_indices.is_empty() {
        // 获取删除前的链表总数
        let orderbook_account_info = ctx.accounts.down_orderbook.to_account_info();
        let orderbook_data = orderbook_account_info.try_borrow_data()?;
        let header_before = crate::instructions::orderbook_manager::OrderBookManager::load_orderbook_header(&orderbook_data)?;
        let total_before = header_before.total;
        drop(orderbook_data); // 释放借用，避免冲突

        let delete_count = calc_sell_result.liquidate_indices.len();
        // msg!("批量删除前: 链表总数={}, 待删除数量={}", total_before, delete_count);

        // 执行批量删除
        crate::instructions::orderbook_manager::OrderBookManager::batch_remove_by_indices_unsafe(
            &orderbook_account_info,
            &calc_sell_result.liquidate_indices,
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


    // 重新计算流动池储备量
    if let Some((sol_reserve, token_reserve)) = CurveAMM::price_to_reserves(ctx.accounts.curve_account.price) {
        ctx.accounts.curve_account.lp_sol_reserve = sol_reserve;
        ctx.accounts.curve_account.lp_token_reserve = token_reserve;
        // msg!("流动池储备量已根据价格重新计算: SOL={}, Token={}", sol_reserve, token_reserve);
    } else {
        return Err(ErrorCode::CurveCalculationError.into());
    }


    // 发出保证金做空交易事件
    emit!(LongShortEvent {
        payer: ctx.accounts.payer.key(),
        order_id: actual_order_id,
        mint_account: ctx.accounts.mint_account.key(),
        latest_price: ctx.accounts.curve_account.price,
        open_price: new_margin_order.open_price,
        order_type: new_margin_order.order_type,
        lock_lp_start_price: new_margin_order.lock_lp_start_price,
        lock_lp_end_price: new_margin_order.lock_lp_end_price,
        lock_lp_sol_amount: new_margin_order.lock_lp_sol_amount,
        lock_lp_token_amount: new_margin_order.lock_lp_token_amount,
        start_time: new_margin_order.start_time,
        end_time: new_margin_order.end_time,
        margin_sol_amount: new_margin_order.margin_sol_amount,
        borrow_amount: new_margin_order.borrow_amount,
        position_asset_amount: new_margin_order.position_asset_amount,
        borrow_fee: new_margin_order.borrow_fee,
        liquidate_indices: calc_sell_result.liquidate_indices.clone(),
    });




    // msg!("订单插入处理完成");

    // 返回成功结果
    Ok(())
}
