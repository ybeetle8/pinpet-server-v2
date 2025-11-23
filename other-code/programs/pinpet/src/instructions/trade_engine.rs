// 交易计算模块 - 包含买入和卖出交易的计算逻辑
use {
    // 导入常量
    crate::constants::{MAX_TOKEN_DIFFERENCE},
    // 导入曲线计算模块
    crate::curve::curve_amm::CurveAMM,
    // 导入错误处理
    crate::error::ErrorCode,
    // 导入订单管理器和链表方向枚举
    crate::instructions::orderbook_manager::OrderBookManager,
    // 导入参数结构和账户结构
    crate::instructions::pdas::BorrowingBondingCurve,
    // 导入 Anchor 框架的基础组件
    anchor_lang::prelude::*,
};

/// 买入交易计算结果 z
#[derive(Debug)]
pub struct BuyAmountsResult {
    /// 需要投入的SOL数量 不包含手续费
    pub required_sol: u64,
    /// 获得的代币数量
    pub output_token: u64,
    /// 交易后的价格
    pub target_price: u128,
    /// 交易手续费用 sol收取
    pub fee_sol: u64,
    /// 强制平仓时产生的手续费 (这是需要从总账里转到手续费地址的)
    pub liquidate_fee_sol: u64,
    /// 需要清算的订单索引列表
    pub liquidate_indices: Vec<u16>,
}

/// 卖出交易计算结果
#[derive(Debug)]
pub struct SellAmountsResult {
    /// 卖出的代币数量
    pub sell_token: u64,
    /// 获得的SOL数量 已扣除手续费
    pub output_sol: u64,
    /// 交易后的价格
    pub target_price: u128,
    /// 交易手续费用 sol收取
    pub fee_sol: u64,
    /// 强制平仓时产生的手续费 (这是需要从总账里转到手续费地址的)
    pub liquidate_fee_sol: u64,
    /// 需要清算的订单索引列表
    pub liquidate_indices: Vec<u16>,
}

/// 计算买入时所需的SOL数量和可获得的token数量，并处理订单逻辑, 注意: 这里为平仓定单归还了借币池
///
/// # 参数
/// * `curve_account` - 曲线账户，包含价格、储备等信息
/// * `up_orderbook` - 做空订单簿账户（买入时处理）
/// * `payer` - 支付账户（用于支付租金或接收退款）
/// * `system_program` - 系统程序
/// * `pass_order_id` - 需要跳过不止损的订单ID，None表示不跳过任何订单 (切记只有在平仓操作时才会有值)
/// * `output_token_amount` - 期望获得的token数量
/// * `input_sol_max` - 最大允许投入的SOL数量
/// * `fee` - 手续费率
///
/// # 返回值
/// * `Result<BuyAmountsResult>` - 成功则返回包含交易详情的结构体
pub fn buy_amounts<'info>(
    curve_account: &mut Account<'info, BorrowingBondingCurve>,
    up_orderbook: &AccountInfo<'info>,
    pass_order_id: Option<u64>,
    output_token_amount: u64,
    input_sol_max: u64,
    fee: u16,
) -> Result<BuyAmountsResult> {
    // msg!("=== buy_amounts 开始计算 ===");

    // 当前价格
    let current_price = curve_account.price;
    // msg!("当前价格: {}", current_price);

    // 1. 借用 up_orderbook 数据并获取头部订单信息
    let data = up_orderbook.data.borrow();
    let orderbook = OrderBookManager::load_orderbook_header(&data)?;
    let head_index = orderbook.head;
    let total = orderbook.total;

    // 2. 检查是否有需要处理的做空订单
    // 条件: 头节点为空, 或只有一个节点且该节点需要跳过
    let no_orders_to_process = head_index == u16::MAX || {
        // 检查是否只有一个节点且需要跳过
        if total == 1 {
            let head_order = OrderBookManager::get_order(&data, head_index)?;
            if let Some(pass_id) = pass_order_id {
                head_order.order_id == pass_id
            } else {
                false
            }
        } else {
            false
        }
    };

    if no_orders_to_process {
        // msg!("分支1 up_orderbook 为空或只有需跳过的订单，没有做空订单需要处理");

        // 1. 计算不含手续费的交易数据
        let calc_result =
            CurveAMM::buy_from_price_with_token_output(current_price, output_token_amount)
                .ok_or(ErrorCode::BuyFromPriceWithTokenNoneError)?;

        let (target_price, required_sol) = calc_result;

        // msg!("初始计算结果:");
        // msg!("不含手续费的SOL数量: {}", required_sol);
        // msg!("能获得的token数量: {}", output_token_amount);
        // msg!("交易后的价格: {}", target_price);

        // 2. 计算加上手续费后的总金额
        // msg!("开始计算手续费...");
        // msg!("手续费率: {}", fee);

        let total_sol_with_fee = CurveAMM::calculate_total_amount_with_fee(required_sol, fee)
            .ok_or(ErrorCode::TotalAmountWithFeeError)?;

        // msg!("加上手续费后的总SOL数量: {}", total_sol_with_fee);

        // 3. 计算手续费金额
        let fee_sol = total_sol_with_fee
            .checked_sub(required_sol)
            .ok_or(ErrorCode::BuyFeeCalculationOverflow)?;

        // msg!("手续费金额: {} SOL", fee_sol);

        // 4. 验证交易条件 - 检查含手续费的SOL输入是否满足最大限制
        require!(
            total_sol_with_fee <= input_sol_max,
            ErrorCode::ExceedsMaxSolAmount
        );

        // msg!("交易条件验证通过，可以执行交易");

        // 5. 返回结果 - 空订单簿场景不需要平仓
        return Ok(BuyAmountsResult {
            required_sol, // 不含手续费的净SOL
            output_token: output_token_amount,
            target_price,
            fee_sol,
            liquidate_fee_sol: 0,          // 没有强制平仓，手续费为0
            liquidate_indices: Vec::new(), // 没有需要清算的订单
        });
    } else {
        // msg!(
            //     "分支2：要处理订单的逻辑  up_orderbook 头部索引: {}",
            //     head_index
        // );

        // 3. 获取头部订单
        let head_order = OrderBookManager::get_order(&data, head_index)?;

        // 4. 获取头部订单的止损价格
        let head_lock_start_price = head_order.lock_lp_start_price;

        // 5. 计算从当前价格到头部订单止损价格的交易数据
        let head_range_result =
            CurveAMM::buy_from_price_to_price(current_price, head_lock_start_price)
                .ok_or(ErrorCode::BuyPriceRangeCalculationError)?;

        let (head_required_sol, head_available_token) = head_range_result;

        // 6. 打印头部订单分析数据
        // msg!("--- 头部订单分析 ---");
        // msg!(
            //     "头部订单止损价格 (lock_lp_start_price): {}",
            //     head_lock_start_price
        // );
        // msg!("到达止损价格需要的SOL: {}", head_required_sol);
        // msg!("到达止损价格可获得的Token: {}", head_available_token);
        // msg!("-------------------");

        if head_available_token >= output_token_amount {
            // msg!("buy分支2-1, 未触发止损完成交易");

            // 1. 计算不含手续费的交易数据
            let calc_result =
                CurveAMM::buy_from_price_with_token_output(current_price, output_token_amount)
                    .ok_or(ErrorCode::BuyFromPriceWithTokenNoneError)?;

            let (target_price, required_sol) = calc_result;

            let total_sol_with_fee = CurveAMM::calculate_total_amount_with_fee(required_sol, fee)
                .ok_or(ErrorCode::TotalAmountWithFeeError)?;

            // msg!("加上手续费后的总SOL数量: {}", total_sol_with_fee);

            // 3. 计算手续费金额
            let fee_sol = total_sol_with_fee
                .checked_sub(required_sol)
                .ok_or(ErrorCode::BuyFeeCalculationOverflow)?;

            // msg!("手续费金额: {} SOL", fee_sol);

            // 4. 验证交易条件 - 检查含手续费的SOL输入是否满足最大限制
            require!(
                total_sol_with_fee <= input_sol_max,
                ErrorCode::ExceedsMaxSolAmount
            );

            // msg!("交易条件验证通过，可以执行交易");

            // 5. 返回结果 - 未触发止损场景不需要平仓
            return Ok(BuyAmountsResult {
                required_sol, // 不含手续费的净SOL
                output_token: output_token_amount,
                target_price,
                fee_sol,
                liquidate_fee_sol: 0,          // 没有强制平仓，手续费为0
                liquidate_indices: Vec::new(), // 没有需要清算的订单
            });
        } else {
            // msg!("分支2-2, 止损被触发");

            // 7. 使用 traverse 遍历 up_orderbook 中的所有订单
            // msg!("开始遍历 up_orderbook 中的订单...");

            let mut iteration_count = 0;
            let mut total_token_amount = head_available_token;
            // 止损单的索引列表 (顺序是乱的哦)
            let mut liquidate_indices = Vec::new();
            // 止损单的总手续费
            let mut total_liquidate_fee_sol = 0u64;

            // 计算止损区间的token数量
            let mut stop_loss_token_amount = 0u64;
            // 计算止损区间的sol数量
            let mut stop_loss_sol_amount = 0u64;
            // 存储最终的买入结果
            let mut buy_result: Option<BuyAmountsResult> = None;

            let traversal_result = OrderBookManager::traverse(
                &data,
                head_index, // 从头部开始
                0,          // limit=0 表示无限制遍历
                |current_index, current_order| {
                    iteration_count += 1;

                    let mut is_pass = false;
                    // 检查是否是需要跳过的订单
                    if let Some(pass_id) = pass_order_id {
                        if current_order.order_id == pass_id {
                            // msg!("  -> 有跳过订单{}被跳过", current_index);
                            is_pass = true;
                        }
                    }

                    // 打印当前订单信息
                    // msg!("第 {} 个订单: 索引 {}", iteration_count, current_index);
                    // msg!("  用户地址: {}", current_order.user);
                    // msg!("  止损价格: {}", current_order.lock_lp_start_price);
                    // msg!("  借入数量: {}", current_order.borrow_amount);
                    // msg!("  持仓数量: {}", current_order.position_asset_amount);
                    //记录上个流动区间
                    let previous_available_token = total_token_amount;

                    if !is_pass {
                        //累加止损后的损区间的token数量
                        stop_loss_token_amount = stop_loss_token_amount
                            .checked_add(current_order.lock_lp_token_amount)
                            .ok_or(ErrorCode::BuyCalculationOverflow)?;
                        //累加止损区间的sol数量
                        stop_loss_sol_amount = stop_loss_sol_amount
                            .checked_add(current_order.lock_lp_sol_amount)
                            .ok_or(ErrorCode::BuyCalculationOverflow)?;
                        // 累加token 流动区间
                        total_token_amount = total_token_amount
                            .checked_add(current_order.next_lp_token_amount)
                            .ok_or(ErrorCode::BuyCalculationOverflow)?;
                    } else {
                        // 跳过订单 要累加锁定流动性 + 累加token 流动区间
                        total_token_amount = total_token_amount
                            .checked_add(current_order.lock_lp_token_amount)
                            .ok_or(ErrorCode::BuyCalculationOverflow)?
                            .checked_add(current_order.next_lp_token_amount)
                            .ok_or(ErrorCode::BuyCalculationOverflow)?;
                    }

                    // 判断流动性是否充足
                    if total_token_amount >= output_token_amount {
                        // msg!("流动性充足，可以进行止损交易");

                        // // TODO: 在这里添加止损交易逻辑
                        // // 将当前索引记录到 Vec 中
                        // liquidate_indices.push(current_index);
                        // msg!("  -> 记录到止损列表，索引: {}", current_index);

                        // 计算剩余区间需要购买的代币数量
                        let remaining_token_amount = output_token_amount
                            .checked_sub(previous_available_token)
                            .ok_or(ErrorCode::BuyCalculationOverflow)?;

                        let remaining_calc_result = if !is_pass {
                            // 正常情况 调用 CurveAMM::buy_from_price_with_token_output 计算剩余区间的交易
                            CurveAMM::buy_from_price_with_token_output(
                                current_order.lock_lp_end_price,
                                remaining_token_amount,
                            )
                        } else {
                            // 跳过订单, 要从开始价算
                            CurveAMM::buy_from_price_with_token_output(
                                current_order.lock_lp_end_price,
                                remaining_token_amount,
                            )
                        };

                        let remaining_end_price = match remaining_calc_result {
                            Some((remaining_end_price, _remaining_sol_cost)) => remaining_end_price,
                            None => {
                                // msg!("错误：剩余区间交易计算失败");
                                return Err(ErrorCode::RemainingRangeCalculationError.into());
                            }
                        };

                        // 计算总的价格 的流动性数据
                        let total_range_result =
                            CurveAMM::buy_from_price_to_price(current_price, remaining_end_price);
                        let (mut total_sol_required, total_token_available) =
                            match total_range_result {
                                Some((total_sol_required, total_token_available)) => {
                                    (total_sol_required, total_token_available)
                                }
                                None => {
                                    // msg!("错误：全区间交易计算失败");
                                    return Err(ErrorCode::FullRangeCalculationError.into());
                                }
                            };

                        // 全区间需要投入的SOL数量: total_sol_required 也需要减去区间中的sol 的数量(就是order.lock_lp_sol_amount) 的数量
                        total_sol_required = total_sol_required
                            .checked_sub(stop_loss_sol_amount)
                            .ok_or(ErrorCode::BuyCalculationOverflow)?;

                        // 计算减去止损区间后的token数量
                        let remaining_token_after_stop_loss = total_token_available
                            .checked_sub(stop_loss_token_amount)
                            .ok_or(ErrorCode::BuyCalculationOverflow)?;

                        // 比较token的往返偏差
                        let token_difference =
                            if remaining_token_after_stop_loss >= output_token_amount {
                                remaining_token_after_stop_loss - output_token_amount
                            } else {
                                output_token_amount - remaining_token_after_stop_loss
                            };

                        // 验证token数量差值是否在允许范围内(0-MAX_TOKEN_DIFFERENCE)
                        if token_difference > MAX_TOKEN_DIFFERENCE {
                            // msg!(
                            //     "错误：剩余token数量与目标数量的差值 {} 不在允许范围内(0-{})",
                            //     token_difference,
                            //     MAX_TOKEN_DIFFERENCE
                            // );
                            return Err(ErrorCode::TokenAmountDifferenceOutOfRange.into());
                        }

                        let total_sol_with_fee =
                            CurveAMM::calculate_total_amount_with_fee(total_sol_required, fee)
                                .ok_or(ErrorCode::TotalAmountWithFeeError)?;

                        //msg!("加上手续费后的总SOL数量: {}", total_sol_with_fee);

                        // 计算手续费金额
                        let fee_sol = total_sol_with_fee
                            .checked_sub(total_sol_required)
                            .ok_or(ErrorCode::BuyFeeCalculationOverflow)?;

                        // 验证交易条件
                        require!(
                            total_sol_with_fee <= input_sol_max,
                            ErrorCode::ExceedsMaxSolAmount
                        );

                        //msg!("交易条件验证通过，可以执行交易");
                        // msg!("---------------------------------calculate_buy_amounts 完成计算---------------------------------");

                        // 存储结果到外部变量
                        buy_result = Some(BuyAmountsResult {
                            required_sol: total_sol_required,
                            output_token: output_token_amount,
                            target_price: remaining_end_price,
                            fee_sol,
                            liquidate_fee_sol: 0,
                            liquidate_indices: Vec::new(),
                        });
                    }

                    // --这里是每次遍历都必须执行的代码--

                    // 流动性不足  但也要加到止损, 因为止损区间被跨越也要止损
                    // 非跳过节点才加到止损列表
                    if is_pass == false {
                        liquidate_indices.push(current_index);
                        // msg!("  -> 记录到止损列表，索引: {}", current_index);

                        // 判断 borrow_amount 必须等于 lock_lp_token_amount
                        if current_order.borrow_amount != current_order.lock_lp_token_amount {
                            return Err(ErrorCode::BorrowAmountMismatch.into());
                        }

                        // 归还借币池 - borrow_amount 加到 curve_account.borrow_token_reserve
                        //msg!("L199: borrow_token_reserve={}, borrow_amount={}", curve_account.borrow_token_reserve, borrow_amount);
                        curve_account.borrow_token_reserve = curve_account
                            .borrow_token_reserve
                            .checked_add(current_order.borrow_amount)
                            .ok_or(ErrorCode::TokenReserveAdditionOverflow)?;

                        // 计算加了手续费后的值
                        let cost_sol_with_fee = CurveAMM::calculate_total_amount_with_fee(
                            current_order.lock_lp_sol_amount,
                            current_order.borrow_fee,
                        )
                        .ok_or(ErrorCode::CloseFeeCalculationError)?;

                        // 需打到手续费账户的sol资金
                        let fee_amount = cost_sol_with_fee
                            .checked_sub(current_order.lock_lp_sol_amount)
                            .ok_or(ErrorCode::CloseShortFeeOverflow)?;

                        // 累计手续费
                        total_liquidate_fee_sol = total_liquidate_fee_sol
                            .checked_add(fee_amount)
                            .ok_or(ErrorCode::FeeAccumulationOverflow)?;
                    }

                    // 如果 buy_result 有值说明已完成计算
                    if let Some(_) = buy_result {
                        Ok(false) // 停止遍历
                    } else {
                        Ok(true) // 继续遍历
                    }
                },
            )?;

            // msg!("订单遍历完成，共处理 {} 个订单", traversal_result.processed);
            // msg!("需要止损的订单数量: {}", liquidate_indices.len());

            // 打印所有需要止损的订单索引
            if !liquidate_indices.is_empty() {
                // msg!("止损订单索引列表: {:?}", liquidate_indices);
            }

            // 遍历结束后，检查并返回结果
            if let Some(mut result) = buy_result {
                // msg!("=== buy_amounts 计算完成，返回结果 ===");
                // 把 total_liquidate_fee_sol 加到 buy_result
                result.liquidate_fee_sol = total_liquidate_fee_sol;
                result.liquidate_indices = liquidate_indices.clone();
                return Ok(result);
            }
        }
    }

    Err(ErrorCode::InsufficientMarketLiquidity.into())
}

/// 计算卖出时所需的token数量和可获得的SOL数量，并处理订单逻辑 注意: 这里为平仓定单归还了借币池
///
/// # 参数
/// * `curve_account` - 曲线账户，包含价格、储备等信息
/// * `down_orderbook` - 做多订单簿账户
/// * `payer` - 支付账户（用于支付租金或接收退款）
/// * `system_program` - 系统程序
/// * `pass_order_id` - 需要跳过不止损的订单ID，None表示不跳过任何订单  (切记只有在平仓操作时才可以使用)
/// * `input_token_amount` - 要卖出的token数量
/// * `output_sol_min` - 最小期望获得的SOL数量
///
/// # 返回值
/// * `Result<SellAmountsResult>` - 成功则返回包含交易详情的结构体
pub fn sell_amounts<'info>(
    curve_account: &mut Account<'info, BorrowingBondingCurve>,
    down_orderbook: &AccountInfo<'info>,
    pass_order_id: Option<u64>,
    input_token_amount: u64,
    output_sol_min: u64,
    fee: u16,
) -> Result<SellAmountsResult> {
    // 当前价格
    let current_price = curve_account.price;
    // msg!("当前价格: {}", current_price);

    // 1. 借用 down_orderbook 数据并获取头部订单信息
    let data = down_orderbook.data.borrow();
    let orderbook = OrderBookManager::load_orderbook_header(&data)?;
    let head_index = orderbook.head;
    let total = orderbook.total;

    // 2. 检查是否有需要处理的做多订单
    // 条件: 头节点为空, 或只有一个节点且该节点需要跳过
    let no_orders_to_process = head_index == u16::MAX || {
        // 检查是否只有一个节点且需要跳过
        if total == 1 {
            let head_order = OrderBookManager::get_order(&data, head_index)?;
            if let Some(pass_id) = pass_order_id {
                head_order.order_id == pass_id
            } else {
                false
            }
        } else {
            false
        }
    };

    if no_orders_to_process {
        // msg!("分支1 down_orderbook 为空或只有需跳过的订单，没有做多订单需要处理");

        // 调用CurveAMM的sell_from_price_with_token_input函数计算可获得SOL数量和目标价格
        let calc_result =
            CurveAMM::sell_from_price_with_token_input(current_price, input_token_amount)
                .ok_or(ErrorCode::SellFromPriceWithTokenNoneError)?;

        let (target_price, output_sol) = calc_result;

        // msg!("初始计算结果:");
        // msg!("卖出的token数量: {}", input_token_amount);
        // msg!("不含手续费的SOL数量: {}", output_sol);
        // msg!("交易后的价格: {}", target_price);

        // 计算扣除手续费后的SOL数量
        let sol_after_fee = CurveAMM::calculate_amount_after_fee(output_sol, fee)
            .ok_or(ErrorCode::AmountAfterFeeError)?;

        // msg!("开始计算手续费...");
        // msg!("手续费率: {}", fee);

        // 计算手续费金额
        let fee_sol = output_sol
            .checked_sub(sol_after_fee)
            .ok_or(ErrorCode::SellFeeCalculationOverflow)?;

        // msg!("扣除手续费后的SOL数量: {}", sol_after_fee);
        // msg!("手续费金额: {} SOL", fee_sol);

        // 验证交易条件 - 检查SOL输出是否满足最小要求
        require!(
            sol_after_fee >= output_sol_min,
            ErrorCode::InsufficientSolOutput
        );

        // msg!("交易条件验证通过，可以执行交易");

        // 返回结果 - 空订单簿场景不需要平仓
        return Ok(SellAmountsResult {
            sell_token: input_token_amount,
            output_sol: sol_after_fee, // 扣除手续费后的净SOL
            target_price,
            fee_sol,
            liquidate_fee_sol: 0,          // 没有强制平仓，手续费为0
            liquidate_indices: Vec::new(), // 没有需要清算的订单
        });
    } else {
        // msg!(
            //     "分支2：要处理订单的逻辑  up_orderbook 头部索引: {}",
            //     head_index
        // );

        // 3. 获取头部订单
        let head_order = OrderBookManager::get_order(&data, head_index)?;

        // 4. 获取头部订单的止损价格
        let head_lock_start_price = head_order.lock_lp_start_price;

        // 5. 计算从当前价格到头部订单止损价格的交易数据
        // let head_range_result =
        //     CurveAMM::buy_from_price_to_price(current_price, head_lock_start_price)
        //         .ok_or(ErrorCode::BuyPriceRangeCalculationError)?;

        // 计算新的价格区间
        let head_range_result =
            CurveAMM::sell_from_price_to_price(current_price, head_lock_start_price)
                .ok_or(ErrorCode::SellPriceRangeCalculationError)?;

        //let (required_token, available_sol) = price_range_result;

        let (head_available_token, head_required_sol) = head_range_result;

        // 6. 打印头部订单分析数据
        // msg!("--- 头部订单分析 ---");
        // msg!(
            //     "头部订单区间价格 (lock_lp_start_price,lock_lp_end_price): {} {}",
            //     head_lock_start_price,
            //     head_order.lock_lp_end_price
        // );
        // msg!(
            //     "到达止损价格需要的SOL: head_required_sol={}",
            //     head_required_sol
        // );
        // msg!(
        //     "到达止损价格可获得的Token: head_available_token={}",
        //     head_available_token
        // );
        // msg!("-------------------");

        if head_available_token >= input_token_amount {
            // msg!("sell分支2-1, 未触发止损完成交易");
            // 调用CurveAMM的sell_from_price_with_token_input函数计算可获得SOL数量和目标价格
            let calc_result =
                CurveAMM::sell_from_price_with_token_input(current_price, input_token_amount)
                    .ok_or(ErrorCode::SellFromPriceWithTokenNoneError)?;

            let (target_price, output_sol) = calc_result;

            // 计算扣除手续费后的SOL数量
            let sol_after_fee = CurveAMM::calculate_amount_after_fee(output_sol, fee)
                .ok_or(ErrorCode::AmountAfterFeeError)?;

            // 计算手续费金额
            let fee_sol = output_sol
                .checked_sub(sol_after_fee)
                .ok_or(ErrorCode::SellFeeCalculationOverflow)?;

            // 验证交易条件 - 检查SOL输出是否满足最小要求
            require!(
                sol_after_fee >= output_sol_min,
                ErrorCode::InsufficientSolOutput
            );

            // msg!("交易条件验证通过，可以执行交易");

            // 返回结果 - 空订单簿场景不需要平仓
            return Ok(SellAmountsResult {
                sell_token: input_token_amount,
                output_sol: sol_after_fee, // 扣除手续费后的净SOL
                target_price,
                fee_sol,
                liquidate_fee_sol: 0,          // 没有强制平仓，手续费为0
                liquidate_indices: Vec::new(), // 没有需要清算的订单
            });
        } else {
            // msg!("分支2-2, 止损被触发");

            // 7. 使用 traverse 遍历 down_orderbook 中的所有订单

            let mut iteration_count = 0;
            let mut total_token_amount = head_available_token;
            // 止损单的索引列表 (顺序是乱的哦)
            let mut liquidate_indices = Vec::new();
            // 止损单的总手续费
            let mut total_liquidate_fee_sol = 0u64;

            // 计算止损区间的token数量
            let mut stop_loss_token_amount = 0u64;
            // 计算止损区间的sol数量
            let mut stop_loss_sol_amount = 0u64;
            // 存储最终的卖出结果
            let mut sell_result: Option<SellAmountsResult> = None;

            // msg!(
            //     "开始遍历 down_orderbook 中的订单... 累计total_token_amount={}",
            //     total_token_amount
            // );

            let traversal_result = OrderBookManager::traverse(
                &data,
                head_index, // 从头部开始
                0,          // limit=0 表示无限制遍历
                |current_index, current_order| {
                    iteration_count += 1;

                    // 检查是否是需要跳过的订单
                    let mut is_pass = false;
                    if let Some(pass_id) = pass_order_id {
                        if current_order.order_id == pass_id {
                            // msg!("  -> 有跳过订单{}被跳过", current_index);
                            is_pass = true;
                        }
                    }

                    // 打印当前订单信息
                    // msg!("第 {} 个订单: 索引 {}", iteration_count, current_index);
                    // msg!("  用户地址: {}", current_order.user);
                    // msg!("  止损价格: {}", current_order.lock_lp_start_price);
                    // msg!("  借入数量: {}", current_order.borrow_amount);
                    // msg!("  持仓数量: {}", current_order.position_asset_amount);

                    //记录上个流动区间
                    let previous_available_token = total_token_amount;

                    if !is_pass {
                        // 正常情况下是进这里, 是跳过订单不能累加止损
                        //累加止损后的损区间的token数量
                        stop_loss_token_amount = stop_loss_token_amount
                            .checked_add(current_order.lock_lp_token_amount)
                            .ok_or(ErrorCode::BuyCalculationOverflow)?;
                        //累加止损区间的sol数量
                        stop_loss_sol_amount = stop_loss_sol_amount
                            .checked_add(current_order.lock_lp_sol_amount)
                            .ok_or(ErrorCode::BuyCalculationOverflow)?;
                        // 累加token 流动区间
                        total_token_amount = total_token_amount
                            .checked_add(current_order.next_lp_token_amount)
                            .ok_or(ErrorCode::BuyCalculationOverflow)?;
                    } else {
                        // 跳过订单 要累加锁定流动性 + 累加token 流动区间
                        total_token_amount = total_token_amount
                            .checked_add(current_order.lock_lp_token_amount)
                            .ok_or(ErrorCode::BuyCalculationOverflow)?
                            .checked_add(current_order.next_lp_token_amount)
                            .ok_or(ErrorCode::BuyCalculationOverflow)?;
                    }

                    // msg!("累计流动性: total_token_amount= {}", total_token_amount);

                    // 判断流动性是否充足
                    if total_token_amount >= input_token_amount {
                        // msg!("流动性充足，可以进行止损交易");

                        // 计算剩余区间需要卖出的代币数量
                        let remaining_token_amount = input_token_amount
                            .checked_sub(previous_available_token)
                            .ok_or(ErrorCode::SellCalculationOverflow)?;
                        // msg!("上个流动区间 previous_available_token={},  剩余区间需要卖出的代币数量: remaining_token_amount={}",previous_available_token, remaining_token_amount);

                        let remaining_calc_result = if !is_pass {
                            // 正常情况 调用 CurveAMM::sell_from_price_with_token_input 计算剩余区间的交易
                            CurveAMM::sell_from_price_with_token_input(
                                current_order.lock_lp_end_price,
                                remaining_token_amount,
                            )
                        } else {
                            // 跳过订单, 要从开始价算
                            CurveAMM::sell_from_price_with_token_input(
                                current_order.lock_lp_start_price,
                                remaining_token_amount,
                            )
                        };

                        let remaining_end_price = match remaining_calc_result {
                            Some((remaining_end_price, _remaining_sol_gained)) => {
                                // msg!("剩余区间交易计算成功:");
                                // msg!("  剩余区交易完成后的价格: {}", remaining_end_price);
                                // msg!("  剩余区能获得的SOL数量: {}", _remaining_sol_gained);
                                remaining_end_price
                            }
                            None => {
                                // msg!("错误：剩余区间交易计算失败");
                                return Err(ErrorCode::RemainingRangeCalculationError.into());
                            }
                        };

                        // msg!(
                        //     "计算全区间交易: 从价格 {} 到价格 {}",
                        //     current_price,
                        //     remaining_end_price
                        // );

                        let total_range_result: Option<(u64, u64)> =
                            CurveAMM::sell_from_price_to_price(current_price, remaining_end_price);
                        let (total_token_required, mut total_sol_available) =
                            match total_range_result {
                                Some((total_token_required, total_sol_available)) => {
                                    (total_token_required, total_sol_available)
                                }
                                None => {
                                    // msg!("错误：全区间交易计算失败");
                                    return Err(ErrorCode::FullRangeCalculationError.into());
                                }
                            };

                        //msg!("  全区间需要卖出的代币数量: {} 全区间能获得的SOL数量: {}", total_token_required,total_sol_available);

                        // msg!(
                        //     "全区间 total_sol_available={}, 止损区间 stop_loss_sol_amount={} ",
                        //     total_sol_available,
                        //     stop_loss_sol_amount
                        // );

                        // 全区间能获得的SOL数量: total_sol_available 也需要减去区间中的sol 的数量(就是order.lock_lp_sol_amount) 的数量
                        total_sol_available = total_sol_available
                            .checked_sub(stop_loss_sol_amount)
                            .ok_or(ErrorCode::SellCalculationOverflow)?;

                        //
                        let remaining_token_after_stop_loss = total_token_required
                            .checked_sub(stop_loss_token_amount)
                            .ok_or(ErrorCode::SellCalculationOverflow)?;

                        // 比较token的往返偏差
                        let token_difference =
                            if remaining_token_after_stop_loss >= input_token_amount {
                                // msg!(
                                //     "remaining_token_after_stop_loss {} - input_token_amount {}",
                                //     remaining_token_after_stop_loss,
                                //     input_token_amount
                                // );
                                remaining_token_after_stop_loss - input_token_amount
                            } else {
                                // msg!(
                                //     "input_token_amount {} - remaining_token_after_stop_loss {}",
                                //     input_token_amount,
                                //     remaining_token_after_stop_loss
                                // );
                                input_token_amount - remaining_token_after_stop_loss
                            };

                        // 验证token数量差值是否在允许范围内(0-MAX_TOKEN_DIFFERENCE)
                        if token_difference > MAX_TOKEN_DIFFERENCE {
                            // msg!("========== Token差值超出范围错误(stop_loss分支) ==========");
                            // msg!("token_difference: {}", token_difference);
                            // msg!("MAX_TOKEN_DIFFERENCE: {}", MAX_TOKEN_DIFFERENCE);
                            // msg!(
                            //     "remaining_token_after_stop_loss: {}",
                            //     remaining_token_after_stop_loss
                            // );
                            // msg!("input_token_amount: {}", input_token_amount);
                            // msg!("total_token_required: {}", total_token_required);
                            // msg!("stop_loss_token_amount: {}", stop_loss_token_amount);
                            // msg!("current_price: {}", current_price);
                            // msg!("remaining_end_price: {}", remaining_end_price);
                            // msg!(
                            //     "total_sol_available (扣除止损sol后): {}",
                            //     total_sol_available
                            // );
                            // msg!("stop_loss_sol_amount: {}", stop_loss_sol_amount);
                            // msg!("==========================================");
                            return Err(ErrorCode::TokenAmountDifferenceOutOfRange.into());
                        }

                        // msg!("token数量差值检查通过，差值为: {}", token_difference);

                        //if remaining_token_after_stop_loss >= input_token_amount {
                        // msg!("token数量足够，可以进行交易");

                        // 计算扣除手续费后的SOL数量
                        let sol_after_fee =
                            CurveAMM::calculate_amount_after_fee(total_sol_available, fee)
                                .ok_or(ErrorCode::AmountAfterFeeError)?;

                        // 计算手续费金额
                        let fee_sol = total_sol_available
                            .checked_sub(sol_after_fee)
                            .ok_or(ErrorCode::SellFeeCalculationOverflow)?;

                        // 检查SOL数量是否满足最小要求
                        require!(
                            sol_after_fee >= output_sol_min,
                            ErrorCode::InsufficientSolOutput
                        );

                        // 存储结果到外部变量
                        sell_result = Some(SellAmountsResult {
                            sell_token: input_token_amount,
                            output_sol: sol_after_fee,
                            target_price: remaining_end_price,
                            fee_sol,
                            liquidate_fee_sol: 0,
                            liquidate_indices: Vec::new(),
                        });
                    }

                    // --这里是每次遍历都必须执行的代码--

                    // 流动性不足  但也要加到止损, 因为止损区间被跨越也要止损
                    if is_pass == false {
                        liquidate_indices.push(current_index);
                        // msg!("  -> 记录到止损列表，索引: {}", current_index);

                        // // 判断 borrow_amount 必须等于 lock_lp_token_amount
                        // if current_order.borrow_amount != current_order.lock_lp_token_amount {
                        //     msg!(
                        //         "错误: 订单{}借款金额({})与锁定代币数量({})不匹配",
                        //         current_index,
                        //         current_order.borrow_amount,
                        //         current_order.lock_lp_token_amount
                        //     );
                        //     return Err(ErrorCode::BorrowAmountMismatch.into());
                        // }

                        // 归还借币池 - borrow_amount 加到 curve_account.borrow_sol_reserve
                        curve_account.borrow_sol_reserve = curve_account
                            .borrow_sol_reserve
                            .checked_add(current_order.borrow_amount)
                            .ok_or(ErrorCode::SolReserveAdditionOverflow)?;

                        // 计算扣了手续费后的值
                        let cost_sol_after_fee = CurveAMM::calculate_amount_after_fee(
                            current_order.lock_lp_sol_amount,
                            current_order.borrow_fee,
                        )
                        .ok_or(ErrorCode::CloseFeeCalculationError)?;

                        // 清算时,需打到手续费账户的sol资金
                        let liquidate_fee_amount = current_order
                            .lock_lp_sol_amount
                            .checked_sub(cost_sol_after_fee)
                            .ok_or(ErrorCode::CloseLongFeeOverflow)?;

                        // 累计手续费
                        total_liquidate_fee_sol = total_liquidate_fee_sol
                            .checked_add(liquidate_fee_amount)
                            .ok_or(ErrorCode::FeeAccumulationOverflow)?;
                    }

                    // 如果 buy_result 有值说明已完成计算
                    if let Some(_) = sell_result {
                        Ok(false) // 停止遍历
                    } else {
                        Ok(true) // 继续遍历
                    }
                },
            )?;

            // msg!("订单遍历完成，共处理 {} 个订单", traversal_result.processed);
            // msg!("需要止损的订单数量: {}", liquidate_indices.len());

            // 打印所有需要止损的订单索引
            if !liquidate_indices.is_empty() {
                // msg!("止损订单索引列表: {:?}", liquidate_indices);
            }

            // 遍历结束后，检查并返回结果
            if let Some(mut result) = sell_result {
                // msg!("=== sell_amounts 计算完成，返回结果 ===");
                // 返回前需要添加 清算数据
                result.liquidate_fee_sol = total_liquidate_fee_sol;
                result.liquidate_indices = liquidate_indices;
                return Ok(result);
            }
        }
    }

    Err(ErrorCode::InsufficientMarketLiquidity.into())
}
