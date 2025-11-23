use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("计算溢出错误")]
    ArithmeticOverflow,

    // ==================== 交易计算溢出错误 ====================
    #[msg("买入交易计算溢出")]
    BuyCalculationOverflow,

    #[msg("买入手续费计算溢出")]
    BuyFeeCalculationOverflow,

    #[msg("卖出交易计算溢出")]
    SellCalculationOverflow,

    #[msg("卖出手续费计算溢出")]
    SellFeeCalculationOverflow,

    // ==================== 保证金交易溢出错误 ====================
    #[msg("做多保证金计算溢出")]
    LongMarginCalculationOverflow,

    #[msg("做多借款计算溢出")]
    LongBorrowCalculationOverflow,

    #[msg("做多手续费计算溢出")]
    LongFeeCalculationOverflow,

    #[msg("做多价格计算溢出")]
    LongPriceCalculationOverflow,

    #[msg("做空保证金计算溢出")]
    ShortMarginCalculationOverflow,

    #[msg("做空借款计算溢出")]
    ShortBorrowCalculationOverflow,

    #[msg("做空手续费计算溢出")]
    ShortFeeCalculationOverflow,

    #[msg("做空价格计算溢出")]
    ShortPriceCalculationOverflow,

    // ==================== 平仓操作溢出错误 ====================
    #[msg("平多仓利润计算溢出")]
    CloseLongProfitOverflow,

    #[msg("平多仓还款计算溢出")]
    CloseLongRepaymentOverflow,

    #[msg("平多仓剩余计算溢出")]
    CloseLongRemainingOverflow,

    #[msg("平多仓手续费溢出")]
    CloseLongFeeOverflow,

    #[msg("平空仓利润计算溢出")]
    CloseShortProfitOverflow,

    #[msg("平空仓还款计算溢出")]
    CloseShortRepaymentOverflow,

    #[msg("平空仓剩余计算溢出")]
    CloseShortRemainingOverflow,

    #[msg("平空仓手续费溢出")]
    CloseShortFeeOverflow,

    // ==================== 手续费管理溢出错误 ====================
    #[msg("手续费分配计算溢出")]
    FeeSplitCalculationOverflow,

    #[msg("手续费累加溢出")]
    FeeAccumulationOverflow,

    #[msg("合作伙伴手续费增加溢出")]
    PartnerFeeAdditionOverflow,

    #[msg("基础手续费增加溢出")]
    BaseFeeAdditionOverflow,

    #[msg("资金池手续费扣除溢出")]
    PoolFeeDeductionOverflow,

    #[msg("手续费随机优惠计算溢出")]
    FeeRandomDiscountOverflow,

    // ==================== 流动性管理溢出错误 ====================
    #[msg("SOL储备增加溢出")]
    SolReserveAdditionOverflow,

    #[msg("SOL储备扣除溢出")]
    SolReserveDeductionOverflow,

    #[msg("代币储备增加溢出")]
    TokenReserveAdditionOverflow,

    // ==================== 转账操作溢出错误 ====================
    #[msg("Lamports增加溢出")]
    LamportsAdditionOverflow,

    #[msg("Lamports扣除溢出")]
    LamportsDeductionOverflow,

    // ==================== 时间与计数器溢出错误 ====================
    #[msg("到期时间计算溢出")]
    DeadlineCalculationOverflow,

    #[msg("手续费优惠标志计算溢出")]
    FeeDiscountFlagOverflow,

    #[msg("未授权的操作")]
    Unauthorized,

    #[msg("初始化时所有参数都是必需的")]
    RequiredParameter,

    #[msg("曲线计算错误")]
    CurveCalculationError,

    #[msg("初始价格计算失败")]
    InitialPriceCalculationError,

    #[msg("储备量重算失败（买入后）")]
    BuyReserveRecalculationError,

    #[msg("储备量重算失败（卖出后）")]
    SellReserveRecalculationError,

    #[msg("含手续费总额计算失败")]
    TotalAmountWithFeeError,

    #[msg("扣费后金额计算失败")]
    AmountAfterFeeError,

    #[msg("买入价格区间计算失败")]
    BuyPriceRangeCalculationError,

    #[msg("卖出价格区间计算失败")]
    SellPriceRangeCalculationError,

    #[msg("剩余区间交易计算失败")]
    RemainingRangeCalculationError,

    #[msg("全区间交易计算失败")]
    FullRangeCalculationError,

    #[msg("曲线函数返回None：buy_from_price_with_token_output")]
    BuyFromPriceWithTokenNoneError,

    #[msg("曲线函数返回None：sell_from_price_with_token_input")]
    SellFromPriceWithTokenNoneError,

    #[msg("用户设置的最大SOL可使用金额不足")]
    ExceedsMaxSolAmount,

    #[msg("获得的SOL数量不足")]
    InsufficientSolOutput,

    #[msg("平仓收益不足以偿还借款")]
    InsufficientRepayment,

    #[msg("借款请求超过可用储备")]
    InsufficientBorrowingReserve,

    #[msg("实际卖出的代币数量不足")]
    InsufficientTokenSale,

    #[msg("当前订单可提供的流动性不足")]
    InsufficientLiquidity,

    #[msg("市场流动性不足，即使清算所有止损订单也无法满足交易需求")]
    InsufficientMarketLiquidity,

    #[msg("保证金交易时,区间计算误差值太大")]
    TokenAmountDifferenceOutOfRange,

    #[msg("借款金额与锁定代币数量不匹配")]
    BorrowAmountMismatch,

    #[msg("平仓手续费计算错误")]
    CloseFeeCalculationError,

    #[msg("保证金不足")]
    InsufficientMargin,

    #[msg("保证金低于最小限制")]
    InsufficientMinimumMargin,

    #[msg("账户所有者不正确")]
    InvalidAccountOwner,

    #[msg("卖出数量超过订单持有的代币数量")]
    SellAmountExceedsOrderAmount,

    #[msg("未超时订单必须由开仓者平仓")]
    OrderNotExpiredMustCloseByOwner,

    #[msg("结算地址必须是开仓地址")]
    SettlementAddressMustBeOwnerAddress,

    #[msg("买入数量超过订单持有的代币数量")]
    BuyAmountExceedsOrderAmount,

    #[msg("交易数量低于最小限制")]
    InsufficientTradeAmount,

    #[msg("交易冷却期未结束，请稍后再试")]
    TradeCooldownNotExpired,

    #[msg("卖出数量超过批准额度，请先调用approval函数")]
    ExceedApprovalAmount,

    #[msg("Sell交易需要先调用approval或buy函数初始化冷却PDA")]
    CooldownNotInitialized,

    #[msg("代币余额不为0，无法关闭冷却PDA")]
    CannotCloseCooldownWithBalance,

    #[msg("传入的 cooldown PDA 地址不正确")]
    InvalidCooldownPDA,

    #[msg("剩余代币数量低于最小交易限制")]
    RemainingTokenAmountTooSmall,

    #[msg("价格计算错误")]
    PriceCalculationError,

    #[msg("手续费接收账户地址不匹配")]
    InvalidFeeRecipientAccount,

    #[msg("订单mint地址与curve账户mint不匹配")]
    InvalidOrderMintAddress,

    #[msg("手续费分配比例必须在0-100之间")]
    InvalidFeePercentage,


    #[msg("止损价格不满足最小间隔要求")]
    InvalidStopLossPrice,

    #[msg("无盈利资金可转移")]
    NoProfitableFunds,

    #[msg("池子资金不足")]
    InsufficientPoolFunds,

    // ==================== OrderBook Manager 错误 ====================
    #[msg("数学运算溢出")]
    OrderBookManagerOverflow,

    #[msg("无效的槽位索引")]
    OrderBookManagerInvalidSlotIndex,

    #[msg("无效的账户数据")]
    OrderBookManagerInvalidAccountData,

    #[msg("新容量超过最大限制")]
    OrderBookManagerExceedsMaxCapacity,

    #[msg("账户大小超过 10MB 限制")]
    OrderBookManagerExceedsAccountSizeLimit,

    #[msg("订单 ID 不匹配")]
    OrderBookManagerOrderIdMismatch,

    #[msg("订单簿为空")]
    OrderBookManagerEmptyOrderBook,

    #[msg("账户不可写")]
    OrderBookManagerAccountNotWritable,

    #[msg("账户未达到 rent-exempt")]
    OrderBookManagerNotRentExempt,

    #[msg("租金余额无效")]
    OrderBookManagerInvalidRentBalance,

    #[msg("余额不足")]
    OrderBookManagerInsufficientFunds,

    #[msg("无效的账户所有者")]
    OrderBookManagerInvalidAccountOwner,

    #[msg("数据访问越界")]
    OrderBookManagerDataOutOfBounds,

    // ==================== 做多/做空订单插入错误 ====================
    #[msg("无法找到合适的插入位置，所有候选位置均因价格区间重叠而失败")]
    NoValidInsertPosition,

    #[msg("close_insert_indices 数组不能为空")]
    EmptyCloseInsertIndices,

    #[msg("close_insert_indices 数组元素数量不能超过 20 个")]
    TooManyCloseInsertIndices,

    #[msg("未找到指定的平仓订单")]
    CloseOrderNotFound,

    #[msg("链表删除计数异常：删除前后计数不一致")]
    LinkedListDeleteCountMismatch,
}
