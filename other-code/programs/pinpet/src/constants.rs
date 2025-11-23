// 订单相关常量


// 交易冷却时间 秒, 为防止夹子机器人, 或是恶意攻击清算交易 而设置
pub const TRADE_COOLDOWN_SECONDS : u32 = 2; // 秒

// 最小交易token数量 防止交易量过小
pub const MIN_TRADE_TOKEN_AMOUNT: u64 = 100_000;  // 0.1 token (降低以支持测试)

// long short 最小交易等值sol数量 保证金交易不能太小,而且同时要满足 MIN_TRADE_TOKEN_AMOUNT 的条件
pub const MIN_MARGIN_SOL_AMOUNT: u64 = 2_000_000; // 0.002 sol

// Token数量差值检查相关常量
// 允许的最大token数量差值
pub const MAX_TOKEN_DIFFERENCE: u64 = 20;

/// 最小止损百分比，避免过小止损引发计算问题
pub const MIN_STOP_LOSS_PERCENT: u16 = 3; // 3%

/// 手续费保留概率分母，决定减少手续费的触发概率 百分比
pub const FEE_RETENTION_PROBABILITY_DENOMINATOR: u64 = 20;

/// 平仓/开仓时插入索引的最大数量
pub const MAX_CLOSE_INSERT_INDICES: usize = 21;


// 报废代码: 

