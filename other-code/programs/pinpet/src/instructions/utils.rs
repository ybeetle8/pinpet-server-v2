use anchor_lang::prelude::*;
use crate::error::ErrorCode;




/// 手续费分配计算结果
#[derive(Debug)]
pub struct FeeSplitResult {
    /// 合作伙伴手续费金额
    pub partner_fee: u64,
    /// 技术提供方基础手续费金额  
    pub base_fee: u64,
}

/// 根据手续费分配比例计算各方应得的手续费
///
/// # 参数
/// * `total_fee` - 总手续费金额
/// * `fee_split` - 手续费分配比例 (0-100)，表示合作伙伴所得百分比
///
/// # 返回值
/// * `FeeSplitResult` - 包含各方手续费金额的结构体
///
/// # 示例
/// ```
/// let result = calculate_fee_split(1000, 80);
/// // 合作伙伴得到 800，技术提供方得到 200
/// assert_eq!(result.partner_fee, 800);
/// assert_eq!(result.base_fee, 200);
/// ```
pub fn calculate_fee_split(total_fee: u64, fee_split: u8) -> Result<FeeSplitResult> {
    // 验证 fee_split 范围
    if fee_split > 100 {
        return Err(ErrorCode::InvalidFeePercentage.into());
    }

    // 处理 total_fee 为 0 的特殊情况
    if total_fee == 0 {
        return Ok(FeeSplitResult {
            partner_fee: 0,
            base_fee: 0,
        });
    }

    // 计算合作伙伴手续费 (按百分比)
    let partner_fee = if fee_split == 0 {
        0
    } else {
        // 严格检查：确保除数不为 0
        const DIVISOR: u64 = 100;
        if DIVISOR == 0 {
            return Err(ErrorCode::FeeSplitCalculationOverflow.into());
        }

        total_fee
            .checked_mul(fee_split as u64)
            .ok_or(ErrorCode::FeeSplitCalculationOverflow)?
            .checked_div(DIVISOR)
            .ok_or(ErrorCode::FeeSplitCalculationOverflow)?
    };

    // 技术提供方手续费 = 总手续费 - 合作伙伴手续费
    let base_fee = total_fee
        .checked_sub(partner_fee)
        .ok_or(ErrorCode::FeeSplitCalculationOverflow)?;

    Ok(FeeSplitResult {
        partner_fee,
        base_fee,
    })
}


