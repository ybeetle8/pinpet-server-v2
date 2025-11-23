// 导入必要的模块
use anchor_lang::prelude::*;

// -----------------------------事件------------------------------ 

// 定义创建基本代币事件
#[event]
pub struct TokenCreatedEvent {
    pub payer: Pubkey,
    pub mint_account: Pubkey,
    pub curve_account: Pubkey,
    pub pool_token_account: Pubkey,
    pub pool_sol_account: Pubkey,
    pub fee_recipient: Pubkey,
    pub base_fee_recipient: Pubkey,        // 基础手续费接收账户
    pub params_account: Pubkey,            // 合作伙伴参数账户PDA地址
    pub swap_fee: u16,                     // 现货交易手续费
    pub borrow_fee: u16,                   // 保证金交易手续费
    pub fee_discount_flag: u8,             // 手续费折扣标志 0: 原价 1: 5折 2: 2.5折  3: 1.25折
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub up_orderbook: Pubkey,        // 做空订单账本 (Up方向)  PDA地址
    pub down_orderbook: Pubkey,        // 做多订单账本 (Down方向) PDA地址
    pub latest_price: u128,                 // 最新的价格
}

// 定义买卖交易事件
#[event]
pub struct BuySellEvent {
    pub payer: Pubkey,
    pub mint_account: Pubkey,
    pub is_buy: bool,
    pub token_amount: u64,    // 最终买入或卖出的token数量
    pub sol_amount: u64,      // 最终花费或得到的sol数量
    pub latest_price: u128,   // 最新的价格
    pub liquidate_indices: Vec<u16>,       // 需要清算的订单索引列表 (只是索引不是订单id哦!)
}

// 定义保证金做多做空交易事件
#[event]
pub struct LongShortEvent {
    pub payer: Pubkey,
    pub mint_account: Pubkey,
    pub order_id: u64,                  // 开仓的订单的唯一编号
    pub latest_price: u128,                 // 最新的价格
    pub open_price: u128,                   // 开仓价格
    pub order_type: u8,                     // 订单类型 1: 做多 2: 做空
    pub lock_lp_start_price: u128,          // 锁定流动池区间开始价
    pub lock_lp_end_price: u128,            // 锁定流动池区间结束价
    pub lock_lp_sol_amount: u64,            // 锁定流动池区间sol数量
    pub lock_lp_token_amount: u64,          // 锁定流动池区间token数量
    pub start_time: u32,                    // 订单开始时间戳(秒)
    pub end_time: u32,                      // 贷款到期时间戳(秒)
    pub margin_sol_amount: u64,             // 保证金SOL数量
    pub borrow_amount: u64,                 // 贷款数量
    pub position_asset_amount: u64,         // 当前持仓币的数量
    pub borrow_fee: u16,                    // 保证金交易手续费
    pub liquidate_indices: Vec<u16>,        // 需要清算的订单索引列表 (只是索引不是订单id哦!)
}



// 定义全平仓事件
#[event]
pub struct FullCloseEvent {
    pub payer: Pubkey,
    pub user_sol_account: Pubkey,           // close_order 的开仓用户SOL账户
    pub mint_account: Pubkey,
    pub is_close_long: bool,                // 是否为平多
    pub final_token_amount: u64,            // 最终买入或卖出的token数量
    pub final_sol_amount: u64,              // 最终花费或得到的sol数量
    pub user_close_profit: u64,             // 用户平仓收入的sol数量
    pub latest_price: u128,                 // 最新的价格
    pub order_id: u64,                      // 平仓订单订单的唯一编号
    pub liquidate_indices: Vec<u16>,        // 需要清算的订单索引列表 这里包括平仓订单自已 (只是索引不是订单id哦!)
}

// 定义部分平仓事件
#[event]
pub struct PartialCloseEvent {
    pub payer: Pubkey,
    pub user_sol_account: Pubkey,           // close_order 的开仓用户SOL账户
    pub mint_account: Pubkey,
    pub is_close_long: bool,                // 是否为平多
    pub final_token_amount: u64,            // 最终买入或卖出的token数量
    pub final_sol_amount: u64,              // 最终花费或得到的sol数量
    pub user_close_profit: u64,             // 用户平仓收入的sol数量
    pub latest_price: u128,                 // 最新的价格
    pub order_id: u64,                      // 平仓订单订单的唯一编号
    // 部分平仓订单的参数(修改后的值)
    pub order_type: u8,                     // 订单类型 1: 做多 2: 做空
    pub user: Pubkey,                       // 开仓用户
    pub lock_lp_start_price: u128,          // 锁定流动池区间开始价
    pub lock_lp_end_price: u128,            // 锁定流动池区间结束价
    pub lock_lp_sol_amount: u64,            // 锁定流动池区间sol数量
    pub lock_lp_token_amount: u64,          // 锁定流动池区间token数量
    pub start_time: u32,                    // 订单开始时间戳(秒)
    pub end_time: u32,                      // 贷款到期时间戳(秒)
    //pub margin_init_sol_amount: u64,        // 初始保证金SOL数量 
    pub margin_sol_amount: u64,             // 保证金SOL数量
    pub borrow_amount: u64,                 // 贷款数量
    pub position_asset_amount: u64,         // 当前持仓数量
    pub borrow_fee: u16,                    // 保证金交易手续费
    pub realized_sol_amount: u64,           // 实现盈亏的sol数量
    pub liquidate_indices: Vec<u16>,        // 需要清算的订单索引列表 (只是索引不是订单id哦!)
}

// 交易里程碑折扣 事件
#[event]
pub struct MilestoneDiscountEvent {
    pub payer: Pubkey,
    pub mint_account: Pubkey,
    pub curve_account: Pubkey,
    pub swap_fee: u16,                     // 现货交易手续费
    pub borrow_fee: u16,                   // 保证金交易手续费
    pub fee_discount_flag: u8,             // 手续费折扣标志 0: 原价 1: 5折 2: 2.5折  3: 1.25折
}