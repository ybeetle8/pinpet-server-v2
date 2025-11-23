use rust_decimal::Decimal;
use rust_decimal::prelude::*;

// Decimal::from(u128) 转换本身最大只能处理 79228162514264337593543950335 的数字

// /// SOL计算使用的精度因子 (10^9)
// pub const SOL_PRECISION_FACTOR: u64 = 1_000_000_000;

// /// Token计算使用的精度因子 (10^6)
// pub const TOKEN_PRECISION_FACTOR: u64 = 1_000_000;

// /// 价格计算使用的精度因子 (10^28)
// pub const PRICE_PRECISION_FACTOR: u128 = 10_000_000_000_000_000_000_000_000_000;

/// 手续费计算使用的分母 (10^5)
pub const FEE_DENOMINATOR: u64 = 100_000;

/// 最大手续费率（10%）
pub const MAX_FEE_RATE: u16 = 10_000;

// /// 最小价格变因子 （单笔交易中用的，未来需要思考后决定）
// pub const MIN_PRICE_SPAN: u64 = 100;

/// 取整偏差因子 - 用于让取整更严格地对用户不利，保护流动池
//pub const ROUNDING_BIAS_FACTOR: u64 = 0;

/// 调试开关：是否打印取整偏差
//pub const DEBUG_PRINT_ROUNDING_DEVIATION: bool = false;

/// 传统AMM交易模型结构体
pub struct CurveAMM;

impl CurveAMM {

    pub const INITIAL_SOL_RESERVE_DECIMAL: Decimal = Decimal::from_parts(30, 0, 0, false, 0);
    pub const INITIAL_TOKEN_RESERVE_DECIMAL: Decimal = Decimal::from_parts(1073000000, 0, 0, false, 0);
    //pub const INITIAL_K_DECIMAL: Decimal = Decimal::from_parts(2125228928, 7, 0, false, 0);
    /// 可以出现的最小价格，低于这个价格，可能溢出
    pub const INITIAL_MIN_PRICE_DECIMAL: Decimal = Decimal::from_parts(1, 0, 0, false, 9);

    /// 精度因子的Decimal表示 = 1000000000000000
    //pub const PRICE_PRECISION_FACTOR_DECIMAL: Decimal = Decimal::from_parts(2764472320, 232830, 0, false, 0);
    /// 精度因子的Decimal表示 = 10^28
    //pub const PRICE_PRECISION_FACTOR_DECIMAL: Decimal = Decimal::from_parts(268435456, 1042612833, 542101086, false, 0);
    // /// 精度因子的Decimal表示 = 10^24
    // pub const PRICE_PRECISION_FACTOR_DECIMAL: Decimal = Decimal::from_parts(2701131776, 466537709, 54210, false, 0);
    /// 精度因子的Decimal表示 = 10^26
    pub const PRICE_PRECISION_FACTOR_DECIMAL: Decimal = Decimal::from_parts(3825205248, 3704098002, 5421010, false, 0);

    /// Token精度因子的Decimal表示 = 1000000
    pub const TOKEN_PRECISION_FACTOR_DECIMAL: Decimal = Decimal::from_parts(1000000, 0, 0, false, 0);
    
    /// SOL精度因子的Decimal表示 = 1000000000
    pub const SOL_PRECISION_FACTOR_DECIMAL: Decimal = Decimal::from_parts(1000000000, 0, 0, false, 0);

    /// u64 的极大值 用来分配无限流动性等
    pub const MAX_U64: u64 = 3046744073709551614;

    /// AMM价格计算上限 - 防止 Decimal 运算溢出/panic
    pub const PRICE_CALCULATION_LIMIT: u128 = 50_000_000_000_000_000_000_000_000_000;


    /// 将u128价格转换为Decimal
    #[inline(always)]
    pub fn u128_to_decimal(price: u128) -> Option<Decimal> {
        // 检查价格是否在安全范围内，防止后续计算溢出
        if price > Self::PRICE_CALCULATION_LIMIT {
            return None;
        }
        
        let price_decimal = Decimal::from(price);
        price_decimal.checked_div(Self::PRICE_PRECISION_FACTOR_DECIMAL)
    }
    
    /// 将Decimal价格转换为u128，向下取整
    #[inline(always)]
    pub fn decimal_to_u128(price: Decimal) -> Option<u128> {
        let scaled: Decimal = price.checked_mul(Self::PRICE_PRECISION_FACTOR_DECIMAL)?;
        let result = scaled.floor().to_u128()?;
        
        // 检查结果是否超过安全限制
        if result > Self::PRICE_CALCULATION_LIMIT {
            return None;
        }
        
        Some(result)
    }
    
    
    /// 将Decimal token数量转换为u64，使用6位精度，向下取整后减去偏差因子
    #[inline(always)]
    pub fn token_decimal_to_u64(amount: Decimal) -> Option<u64> {
        let scaled = amount.checked_mul(Self::TOKEN_PRECISION_FACTOR_DECIMAL)?;
        let base_result = scaled.floor().to_u64()?;
        // 向下取整后减去偏差因子，检查溢出
        Some(base_result)
    }
    
    /// 将Decimal token数量转换为u64，使用6位精度，向上取整后加上偏差因子
    #[inline(always)]
    pub fn token_decimal_to_u64_ceil(amount: Decimal) -> Option<u64> {
        let scaled = amount.checked_mul(Self::TOKEN_PRECISION_FACTOR_DECIMAL)?;
        let base_result = scaled.ceil().to_u64()?;
        // 向上取整后加上偏差因子
        Some(base_result)
    }
    
    /// 将Decimal token数量转换为u64，使用6位精度，四舍五入
    #[inline(always)]
    pub fn token_decimal_to_u64_rounded(amount: Decimal) -> Option<u64> {
        let scaled = amount.checked_mul(Self::TOKEN_PRECISION_FACTOR_DECIMAL)?;
        scaled.round().to_u64()
    }
    
    /// 将Decimal SOL数量转换为u64，使用9位精度，向下取整后减去偏差因子
    #[inline(always)]
    pub fn sol_decimal_to_u64(amount: Decimal) -> Option<u64> {
        let scaled = amount.checked_mul(Self::SOL_PRECISION_FACTOR_DECIMAL)?;
        let base_result = scaled.floor().to_u64()?;
        // 向下取整后减去偏差因子，检查溢出
        Some(base_result)
    }
    
    /// 将Decimal SOL数量转换为u64，使用9位精度，向上取整后加上偏差因子
    #[inline(always)]
    pub fn sol_decimal_to_u64_ceil(amount: Decimal) -> Option<u64> {
        let scaled = amount.checked_mul(Self::SOL_PRECISION_FACTOR_DECIMAL)?;
        let base_result = scaled.ceil().to_u64()?;
        // 向上取整后加上偏差因子
        Some(base_result)
    }
    
    /// 将Decimal SOL数量转换为u64，使用9位精度，四舍五入
    #[inline(always)]
    pub fn sol_decimal_to_u64_rounded(amount: Decimal) -> Option<u64> {
        let scaled = amount.checked_mul(Self::SOL_PRECISION_FACTOR_DECIMAL)?;
        scaled.round().to_u64()
    }
    
    /// 将u64 token数量转换为Decimal，使用6位精度
    #[inline(always)]
    pub fn u64_to_token_decimal(amount: u64) -> Option<Decimal> {
        let amount_decimal = Decimal::from(amount);
        amount_decimal.checked_div(Self::TOKEN_PRECISION_FACTOR_DECIMAL)
    }
    
    /// 将u64 SOL数量转换为Decimal，使用9位精度
    #[inline(always)]
    pub fn u64_to_sol_decimal(amount: u64) -> Option<Decimal> {
        let amount_decimal = Decimal::from(amount);
        amount_decimal.checked_div(Self::SOL_PRECISION_FACTOR_DECIMAL)
    }

    /// 计算初始k值
    /// 
    /// # 返回值
    /// * `Decimal` - 初始储备量的乘积k值
    #[inline(always)]
    pub fn calculate_initial_k() -> Decimal {
        Self::INITIAL_SOL_RESERVE_DECIMAL * Self::INITIAL_TOKEN_RESERVE_DECIMAL
        //Self::INITIAL_K_DECIMAL
    }

    /// 获取初始价格（1个token兑换的SOL数量）
    /// 
    /// # 返回值
    /// * `Option<u128>` - 以u128表示的初始价格，如果计算失败则返回None
    /// 初始价格: 279,589,934,762,348,555,452
    /// 10倍价格: 2,795,899,347,623,485,554,520
    /// 100倍价格: 27,958,993,476,234,855,545,200
    /// 1000倍价格: 279,589,934,762,348,555,452,000
    #[inline(always)]
    pub fn get_initial_price() -> Option<u128> {
        // 计算初始价格 = 初始SOL储备 / 初始Token储备
        let initial_price = Self::INITIAL_SOL_RESERVE_DECIMAL.checked_div(Self::INITIAL_TOKEN_RESERVE_DECIMAL)?;
        
        // 转换为u128格式
        Self::decimal_to_u128(initial_price)
    }

    /// 计算从低价到高价购买token需要的SOL和获得的token数量
    /// 
    /// # 参数
    /// * `start_low_price` - 开始价格（较低）
    /// * `end_high_price` - 目标价格（较高）
    /// 
    /// # 返回值
    /// * `Option<(u64, u64)>` - 成功则返回Some((需要投入的SOL数量, 能获得的token数量))，失败则返回None
    /// SOL数量以9位精度表示，四舍五入；token数量以6位精度表示，四舍五入
    pub fn buy_from_price_to_price(
        start_low_price: u128,
        end_high_price: u128,
    ) -> Option<(u64, u64)> {
        // 转换为Decimal进行计算
        let start_price_dec = Self::u128_to_decimal(start_low_price)?;
        let end_price_dec = Self::u128_to_decimal(end_high_price)?;
        
        // 确保起始价格低于结束价格
        if start_price_dec >= end_price_dec {
            return None;
        }
        
        // 使用初始k值
        let k = Self::calculate_initial_k();
        
        // 计算起始和结束状态的储备量
        let (start_sol_reserve, start_token_reserve) = Self::calculate_reserves_by_price(start_price_dec, k)?;
        let (end_sol_reserve, end_token_reserve) = Self::calculate_reserves_by_price(end_price_dec, k)?;
        
        // 计算需要投入的SOL数量（SOL储备的增加量）
        let sol_input_amount = end_sol_reserve.checked_sub(start_sol_reserve)?;
        
        // 计算能获得的token数量（token储备的减少量）
        let token_output_amount = start_token_reserve.checked_sub(end_token_reserve)?;
        
        // 检查计算结果是否有效
        if sol_input_amount <= Decimal::ZERO || token_output_amount <= Decimal::ZERO {
            return None;
        }
        
        // 转换回u64
        // SOL使用9位精度四舍五入，token使用6位精度四舍五入
        let sol_amount_u64 = Self::sol_decimal_to_u64_rounded(sol_input_amount)?;
        let token_amount_u64 = Self::token_decimal_to_u64_rounded(token_output_amount)?;
        
        
        Some((sol_amount_u64, token_amount_u64))
    }
    
    /// 计算从高价到低价出售token能获得的SOL数量
    /// 
    /// # 参数
    /// * `start_high_price` - 开始价格（较高）
    /// * `end_low_price` - 目标价格（较低）
    /// 
    /// # 返回值
    /// * `Option<(u64, u64)>` - 成功则返回Some((需要出售的token数量, 获得的SOL数量))，失败则返回None
    /// token数量以6位精度表示，四舍五入；SOL数量以9位精度表示，四舍五入 
    pub fn sell_from_price_to_price(
        start_high_price: u128,
        end_low_price: u128,
    ) -> Option<(u64, u64)> {
        // 转换为Decimal进行计算
        let start_price_dec = Self::u128_to_decimal(start_high_price)?;
        let end_price_dec = Self::u128_to_decimal(end_low_price)?;
        
        // 确保起始价格高于结束价格
        if start_price_dec <= end_price_dec {
            return None;
        }
        
        // 使用初始k值
        let k = Self::calculate_initial_k();
        
        // 计算起始和结束状态的储备量
        let (start_sol_reserve, start_token_reserve) = Self::calculate_reserves_by_price(start_price_dec, k)?;
        let (end_sol_reserve, end_token_reserve) = Self::calculate_reserves_by_price(end_price_dec, k)?;
        
        // 计算需要出售的token数量（token储备的增加量）
        let token_input_amount = end_token_reserve.checked_sub(start_token_reserve)?;
        
        // 计算能获得的SOL数量（SOL储备的减少量）
        let sol_output_amount = start_sol_reserve.checked_sub(end_sol_reserve)?;
        
        // 检查计算结果是否有效
        if token_input_amount <= Decimal::ZERO || sol_output_amount <= Decimal::ZERO {
            return None;
        }
        
        // 转换回u64
        // token使用6位精度四舍五入，SOL使用9位精度四舍五入
        let token_amount_u64 = Self::token_decimal_to_u64_rounded(token_input_amount)?;
        let sol_amount_u64 = Self::sol_decimal_to_u64_rounded(sol_output_amount)?;
        
        // // 调试输出：打印取整偏差
        // if DEBUG_PRINT_ROUNDING_DEVIATION {
        //     let token_u64_as_decimal = Self::u64_to_token_decimal(token_amount_u64)?;
        //     let sol_u64_as_decimal = Self::u64_to_sol_decimal(sol_amount_u64)?;
            
        //     let token_deviation = token_u64_as_decimal.checked_sub(token_input_amount)?;
        //     let sol_deviation = sol_u64_as_decimal.checked_sub(sol_output_amount)?;
            
        //     println!("=== sell_from_price_to_price 取整偏差 ===");
        //     println!("Token 原始值: {}", token_input_amount);
        //     println!("Token u64值: {} ({})", token_amount_u64, token_u64_as_decimal);
        //     println!("Token 偏差: {}", token_deviation);
        //     println!("SOL 原始值: {}", sol_output_amount);
        //     println!("SOL u64值: {} ({})", sol_amount_u64, sol_u64_as_decimal);
        //     println!("SOL 偏差: {}", sol_deviation);
        //     println!("=====================================");
        // }
        
        Some((token_amount_u64, sol_amount_u64))
    }

    /// 根据价格计算储备量 (u64接口)
    /// 
    /// # 参数
    /// * `price` - 价格(u128格式，1 token 换多少 SOL)
    /// 
    /// # 返回值
    /// * `Option<(u64, u64)>` - 成功则返回Some((SOL储备, token储备))，失败则返回None
    /// SOL和token数量都使用四舍五入转换
    pub fn price_to_reserves(price: u128) -> Option<(u64, u64)> {
        // 1. 将 u128 价格转换为 Decimal
        let price_decimal = Self::u128_to_decimal(price)?;
        
        // 2. 获取 k 值（使用 calculate_initial_k）
        let k = Self::calculate_initial_k();
        
        // 3. 调用现有函数计算储备量
        let (sol_reserve_decimal, token_reserve_decimal) = Self::calculate_reserves_by_price(price_decimal, k)?;
        
        // 4. 转换 Decimal 结果为 u64
        let sol_reserve_u64 = Self::sol_decimal_to_u64_rounded(sol_reserve_decimal)?;
        let token_reserve_u64 = Self::token_decimal_to_u64_rounded(token_reserve_decimal)?;
        
        Some((sol_reserve_u64, token_reserve_u64))
    }

    /// 给定价格，计算储备量
    /// 
    /// # 参数
    /// * `price` - 价格是以  1 token 换多少 sol 来设定的 
    /// * `k` - 常量乘积
    /// 
    /// # 返回值
    /// * `Option<(Decimal, Decimal)>` - 成功则返回Some((SOL储备, token储备))，失败则返回None
    pub fn calculate_reserves_by_price(price: Decimal, k: Decimal) -> Option<(Decimal, Decimal)> {
        // 检查输入参数是否有效
        if price <= Decimal::ZERO || k <= Decimal::ZERO {
            return None;
        }

        // 最小价格判断，防溢出
        if price < Self::INITIAL_MIN_PRICE_DECIMAL {
            return None;
        }

        // 根据AMM公式: k = sol_reserve * token_reserve
        // 且 price = sol_reserve / token_reserve
        // 可得: sol_reserve = price * token_reserve
        // 代入k公式: k = price * token_reserve^2
        // 因此: token_reserve = sqrt(k / price)
        // sol_reserve = sqrt(k * price)

        // 计算 k / price - 使用checked_div防止溢出
        let k_div_price = k.checked_div(price)?;
        
        // 计算 token_reserve = sqrt(k / price) - 使用 rust_decimal 内置的 sqrt 方法
        let token_reserve = k_div_price.sqrt()?;

        // 计算 sol_reserve = price * token_reserve - 使用checked_mul防止溢出
        let sol_reserve = price.checked_mul(token_reserve)?;

        Some((sol_reserve, token_reserve))
    }





    
    /// 基于起始价格和SOL输入量计算token输出量和结束价格
    /// 
    /// # 参数
    /// * `start_low_price` - 开始价格（较低）
    /// * `sol_input_amount` - 买入用的SOL数量
    /// 
    /// # 返回值
    /// * `Option<(u128, u64)>` - 成功则返回Some((交易完成后的价格, 得到的token数量))，失败则返回None
    /// 价格向下取整，token数量四舍五入
    pub fn buy_from_price_with_sol_input(
        start_low_price: u128,
        sol_input_amount: u64,
    ) -> Option<(u128, u64)> {
        // 转换为Decimal进行计算
        let start_price_dec = Self::u128_to_decimal(start_low_price)?;
        let sol_input_dec = Self::u64_to_sol_decimal(sol_input_amount)?;
        
        // 检查输入参数是否有效
        if start_price_dec <= Decimal::ZERO || sol_input_dec <= Decimal::ZERO {
            return None;
        }
        
        // 使用初始k值
        let k = Self::calculate_initial_k();
        
        // 计算起始状态的储备量
        let (start_sol_reserve, start_token_reserve) = Self::calculate_reserves_by_price(start_price_dec, k)?;
        
        // 计算结束状态的SOL储备量 - 使用checked_add防止溢出
        let end_sol_reserve = start_sol_reserve.checked_add(sol_input_dec)?;
        
        // 根据AMM公式计算结束状态的token储备量 - 使用checked_div防止溢出
        let end_token_reserve = k.checked_div(end_sol_reserve)?;
        
        // 计算token输出量 - 使用checked_sub防止溢出
        let token_output_amount = start_token_reserve.checked_sub(end_token_reserve)?;
        
        // 计算结束价格 - 使用checked_div防止溢出
        let end_price = end_sol_reserve.checked_div(end_token_reserve)?;
        
        // 检查计算结果是否有效
        if token_output_amount <= Decimal::ZERO || end_price <= Decimal::ZERO {
            return None;
        }
        
        // 转换回相应类型，按要求取整
        let end_price_u128 = Self::decimal_to_u128(end_price)?; // 价格向下取整
        let token_amount_u64 = Self::token_decimal_to_u64_rounded(token_output_amount)?; // token四舍五入
        
        Some((end_price_u128, token_amount_u64))
    }

    /// 基于起始价格和token输入量计算SOL输出量和结束价格
    /// 
    /// # 参数
    /// * `start_high_price` - 开始价格（较高）
    /// * `token_input_amount` - 卖出的token数量 
    /// 
    /// # 返回值
    /// * `Option<(u128, u64)>` - 成功则返回Some((交易完成后的价格, 得到的SOL数量))，失败则返回None
    /// 价格向下取整，SOL数量四舍五入
    pub fn sell_from_price_with_token_input(
        start_high_price: u128,
        token_input_amount: u64,
    ) -> Option<(u128, u64)> {
        // 转换为Decimal进行计算
        let start_price_dec = Self::u128_to_decimal(start_high_price)?;
        let token_input_dec = Self::u64_to_token_decimal(token_input_amount)?;
        
        // 检查输入参数是否有效
        if start_price_dec <= Decimal::ZERO || token_input_dec <= Decimal::ZERO {
            return None;
        }
        
        // 使用初始k值
        let k = Self::calculate_initial_k();
        
        // 计算起始状态的储备量
        let (start_sol_reserve, start_token_reserve) = Self::calculate_reserves_by_price(start_price_dec, k)?;
        
        // 计算结束状态的token储备量 - 使用checked_add防止溢出
        let end_token_reserve = start_token_reserve.checked_add(token_input_dec)?;
        
        // 根据AMM公式计算结束状态的SOL储备量 - 使用checked_div防止溢出
        let end_sol_reserve = k.checked_div(end_token_reserve)?;
        
        // 计算SOL输出量 - 使用checked_sub防止溢出
        let sol_output_amount = start_sol_reserve.checked_sub(end_sol_reserve)?;
        
        // 计算结束价格 - 使用checked_div防止溢出
        let end_price = end_sol_reserve.checked_div(end_token_reserve)?;
        
        // 检查计算结果是否有效
        if sol_output_amount <= Decimal::ZERO || end_price <= Decimal::ZERO {
            return None;
        }
        
        // 转换回相应类型，按要求取整
        let end_price_u128 = Self::decimal_to_u128(end_price)?; // 价格向下取整
        let sol_amount_u64 = Self::sol_decimal_to_u64_rounded(sol_output_amount)?; // SOL四舍五入
        
        Some((end_price_u128, sol_amount_u64))
    }

    /// 基于起始价格和期望token输出量计算需要的SOL输入量和结束价格
    /// 
    /// # 参数
    /// * `start_low_price` - 开始价格（较低）
    /// * `token_output_amount` - 希望得到的token数量
    /// 
    /// # 返回值
    /// * `Option<(u128, u64)>` - 成功则返回Some((交易完成后的价格, 需要付出的SOL数量))，失败则返回None
    /// 价格向下取整，SOL数量四舍五入
    pub fn buy_from_price_with_token_output(
        start_low_price: u128,
        token_output_amount: u64,
    ) -> Option<(u128, u64)> {
        // 转换为Decimal进行计算
        let start_price_dec = Self::u128_to_decimal(start_low_price)?;
        let token_output_dec = Self::u64_to_token_decimal(token_output_amount)?;
        
        // 检查输入参数是否有效
        if start_price_dec <= Decimal::ZERO || token_output_dec <= Decimal::ZERO {
            return None;
        }
        
        // 使用初始k值
        let k = Self::calculate_initial_k();
        
        // 计算起始状态的储备量
        let (start_sol_reserve, start_token_reserve) = Self::calculate_reserves_by_price(start_price_dec, k)?;
        
        // 计算结束状态的token储备量 - 使用checked_sub防止溢出
        let end_token_reserve = start_token_reserve.checked_sub(token_output_dec)?;
        
        // 检查token储备量是否足够
        if end_token_reserve <= Decimal::ZERO {
            return None;
        }
        
        // 根据AMM公式计算结束状态的SOL储备量 - 使用checked_div防止溢出
        let end_sol_reserve = k.checked_div(end_token_reserve)?;
        
        // 计算需要的SOL输入量 - 使用checked_sub防止溢出
        let sol_input_amount = end_sol_reserve.checked_sub(start_sol_reserve)?;
        
        // 计算结束价格 - 使用checked_div防止溢出
        let end_price = end_sol_reserve.checked_div(end_token_reserve)?;
        
        // 检查计算结果是否有效
        if sol_input_amount <= Decimal::ZERO || end_price <= Decimal::ZERO {
            return None;
        }
        
        // 转换回相应类型，按要求取整
        let end_price_u128 = Self::decimal_to_u128(end_price)?; // 价格向下取整
        let sol_amount_u64 = Self::sol_decimal_to_u64_rounded(sol_input_amount)?; // SOL四舍五入
        
        Some((end_price_u128, sol_amount_u64))
    }

    /// 基于起始价格和期望SOL输出量计算需要的token输入量和结束价格
    /// 
    /// # 参数
    /// * `start_high_price` - 开始价格（较高）
    /// * `sol_output_amount` - 希望得到的SOL数量
    /// 
    /// # 返回值
    /// * `Option<(u128, u64)>` - 成功则返回Some((交易完成后的价格, 需要付出的token数量))，失败则返回None
    /// 价格向下取整，token数量四舍五入
    pub fn sell_from_price_with_sol_output(
        start_high_price: u128,
        sol_output_amount: u64,
    ) -> Option<(u128, u64)> {
        // 转换为Decimal进行计算
        let start_price_dec = Self::u128_to_decimal(start_high_price)?;
        let sol_output_dec = Self::u64_to_sol_decimal(sol_output_amount)?;
        
        // 检查输入参数是否有效
        if start_price_dec <= Decimal::ZERO || sol_output_dec <= Decimal::ZERO {
            return None;
        }
        
        // 使用初始k值
        let k = Self::calculate_initial_k();
        
        // 计算起始状态的储备量
        let (start_sol_reserve, start_token_reserve) = Self::calculate_reserves_by_price(start_price_dec, k)?;
        
        // 计算结束状态的SOL储备量 - 使用checked_sub防止溢出
        let end_sol_reserve = start_sol_reserve.checked_sub(sol_output_dec)?;
        
        // 检查SOL储备量是否足够
        if end_sol_reserve <= Decimal::ZERO {
            return None;
        }
        
        // 根据AMM公式计算结束状态的token储备量 - 使用checked_div防止溢出
        let end_token_reserve = k.checked_div(end_sol_reserve)?;
        
        // 计算需要的token输入量 - 使用checked_sub防止溢出
        let token_input_amount = end_token_reserve.checked_sub(start_token_reserve)?;
        
        // 计算结束价格 - 使用checked_div防止溢出
        let end_price = end_sol_reserve.checked_div(end_token_reserve)?;
        
        // 检查计算结果是否有效
        if token_input_amount <= Decimal::ZERO || end_price <= Decimal::ZERO {
            return None;
        }
        
        // 转换回相应类型，按要求取整
        let end_price_u128 = Self::decimal_to_u128(end_price)?; // 价格向下取整
        let token_amount_u64 = Self::token_decimal_to_u64_rounded(token_input_amount)?; // token四舍五入
        
        Some((end_price_u128, token_amount_u64))
    }

    /// 计算扣除手续费后的剩余金额
    ///
    /// # 参数
    /// * `amount` - 原始金额
    /// * `fee` - 手续费率，以FEE_DENOMINATOR为分母表示
    ///           例如：1000表示1%的手续费 (1000/100000)
    ///                2000表示2%的手续费 (2000/100000)
    ///
    /// # 返回值
    /// * `Option<u64>` - 成功则返回Some(扣除手续费后的剩余金额)，失败则返回None
    /// 手续费计算采用向下取整方式，即对用户最有利的计算方式
    ///
    /// # 可能的失败原因
    /// * fee大于MAX_FEE_RATE（手续费率超过10%）
    /// * 计算过程中出现溢出
    #[inline(always)]
    pub fn calculate_amount_after_fee(amount: u64, fee: u16) -> Option<u64> {
        // 检查手续费率是否有效（必须小于等于10%）
        if fee > MAX_FEE_RATE {
            return None;
        }
        
        // 计算手续费金额：amount * fee / FEE_DENOMINATOR
        // 先将fee转换为u64以防止溢出
        let fee_u64 = u64::from(fee);
        
        // 计算手续费金额，使用checked_mul和checked_div防止溢出
        let fee_amount = amount.checked_mul(fee_u64)?.checked_div(FEE_DENOMINATOR)?;
        
        // 计算扣除手续费后的剩余金额，使用checked_sub防止溢出
        let amount_after_fee = amount.checked_sub(fee_amount)?;
        
        Some(amount_after_fee)
    }

    /// 计算加上手续费后的总金额
    ///
    /// # 参数
    /// * `sol_amount` - 净金额（基础金额）
    /// * `fee` - 手续费率，以FEE_DENOMINATOR为分母表示
    ///           例如：1000表示1%的手续费 (1000/100000)
    ///                2000表示2%的手续费 (2000/100000)
    ///
    /// # 返回值
    /// * `Option<u64>` - 成功则返回Some(加上手续费后的总金额)，失败则返回None
    /// 总金额计算采用向上取整方式
    ///
    /// # 计算公式
    /// total_amount = sol_amount + (sol_amount * fee / FEE_DENOMINATOR)
    /// 即：total_amount = sol_amount * (FEE_DENOMINATOR + fee) / FEE_DENOMINATOR
    ///
    /// # 可能的失败原因
    /// * fee大于MAX_FEE_RATE（手续费率超过10%）
    /// * 计算过程中出现溢出
    #[inline(always)]
    pub fn calculate_total_amount_with_fee(sol_amount: u64, fee: u16) -> Option<u64> {
        // 检查手续费率是否有效（必须小于等于10%）
        if fee > MAX_FEE_RATE {
            return None;
        }
        
        // 将fee转换为u64以防止溢出
        let fee_u64 = u64::from(fee);
        
        // 计算分子：net_amount * (FEE_DENOMINATOR + fee)
        let numerator = sol_amount
            .checked_mul(FEE_DENOMINATOR.checked_add(fee_u64)?)?;
        
        // 使用向上取整除法：(numerator + FEE_DENOMINATOR - 1) / FEE_DENOMINATOR
        let total_amount = numerator
            .checked_add(FEE_DENOMINATOR)?
            .checked_sub(1)?
            .checked_div(FEE_DENOMINATOR)?;
        
        Some(total_amount)
    }

}