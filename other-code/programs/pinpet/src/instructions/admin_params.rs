// 导入必要的模块
use {
    anchor_lang::prelude::*,
    crate::instructions::contexts::{UpdateAdmin, CreateParams, UpdateParams},
    crate::error::ErrorCode,
};



// 更新 Admin 的处理函数，支持初始化和更新
pub fn update_admin(
    ctx: Context<UpdateAdmin>,
    default_swap_fee: Option<u16>,
    default_borrow_fee: Option<u16>,
    default_borrow_duration: Option<u32>,
    base_fee_recipient: Option<Pubkey>,
    default_fee_split: Option<u8>,
    new_admin: Option<Pubkey>,
) -> Result<()> {
    let admin_account = &mut ctx.accounts.admin_account;
    
    // 判断是否为新账户：检查admin是否为默认值（最可靠的方法）
    let is_new_account = admin_account.admin == Pubkey::default();
    
    // 如果不是新账户，进行双重权限验证
    if !is_new_account {
        // 第一重验证：基本权限检查
        if admin_account.admin != ctx.accounts.admin.key() {
            // msg!("Unauthorized: Caller is not the stored admin. Expected: {}, Got: {}",
            //      admin_account.admin, ctx.accounts.admin.key());
            return Err(ErrorCode::Unauthorized.into());
        }
        
        // 第二重验证：确保admin账户字段不是默认值（防止意外重置）
        if admin_account.admin == Pubkey::default() {
            // msg!("Invalid state: Admin account admin field is default value");
            return Err(ErrorCode::Unauthorized.into());
        }

        // msg!("Admin update verification passed for existing account: {}", ctx.accounts.admin.key());
    } else {
        // msg!("Initializing new admin account for: {}", ctx.accounts.admin.key());
    }
    
    // 如果是新账户，初始化bump值
    if is_new_account {
        admin_account.bump = ctx.bumps.admin_account;
    }

    // 设置或更新默认交易费率
    if let Some(fee) = default_swap_fee {
        admin_account.default_swap_fee = fee;
        // msg!("{} default swap fee: {}", if is_new_account { "Set" } else { "Updated" }, fee);
    } else if is_new_account {
        // 新账户必须设置初始值
        return Err(ErrorCode::RequiredParameter.into());
    }
    
    // 设置或更新默认借贷费率
    if let Some(fee) = default_borrow_fee {
        admin_account.default_borrow_fee = fee;
        // msg!("{} default borrow fee: {}", if is_new_account { "Set" } else { "Updated" }, fee);
    } else if is_new_account {
        // 新账户必须设置初始值
        return Err(ErrorCode::RequiredParameter.into());
    }
    
    // 设置或更新默认借贷时长
    if let Some(duration) = default_borrow_duration {
        admin_account.default_borrow_duration = duration;
        // msg!("{} default borrow duration (seconds): {}", if is_new_account { "Set" } else { "Updated" }, duration);
    } else if is_new_account {
        // 新账户必须设置初始值
        return Err(ErrorCode::RequiredParameter.into());
    }

    // 设置或更新基础手续费接收地址
    if let Some(recipient) = base_fee_recipient {
        admin_account.base_fee_recipient = recipient;
        // msg!("{} base fee recipient: {}", if is_new_account { "Set" } else { "Updated" }, recipient);
    } else if is_new_account {
        // 新账户如果未设置，使用默认值（当前管理员地址）
        admin_account.base_fee_recipient = ctx.accounts.admin.key();
        // msg!("Set default base fee recipient: {}", ctx.accounts.admin.key());
    }

    // 设置或更新默认手续费分配比例
    if let Some(split) = default_fee_split {
        if split > 100 {
            return Err(ErrorCode::InvalidFeePercentage.into());
        }
        admin_account.default_fee_split = split;
        // msg!("{} default fee split: {}%", if is_new_account { "Set" } else { "Updated" }, split);
    } else if is_new_account {
        // 新账户设置默认值80%
        admin_account.default_fee_split = 80;
        // msg!("Set default fee split: 80%");
    }
    
    // 设置或更新超级管理员账户
    if let Some(admin_key) = new_admin {
        admin_account.admin = admin_key;
        // msg!("{} admin: {}", if is_new_account { "Set" } else { "Updated" }, admin_key);
    } else if is_new_account {
        // 新账户设置当前签名者为管理员
        admin_account.admin = ctx.accounts.admin.key();
        // msg!("Set admin: {}", ctx.accounts.admin.key());
    }

    // msg!("Admin {} successfully!", if is_new_account { "initialized" } else { "updated" });
    
    Ok(())
}

// 创建合作伙伴参数的处理函数
pub fn create_params(
    ctx: Context<CreateParams>,
) -> Result<()> {
    let admin_account = &ctx.accounts.admin_account;
    let params = &mut ctx.accounts.params;
    let partner = &ctx.accounts.partner;
    
    // 初始化 bump 值
    params.bump = ctx.bumps.params;
    
    // 从 Admin 账户复制默认值
    params.base_swap_fee = admin_account.default_swap_fee;
    params.base_borrow_fee = admin_account.default_borrow_fee;
    params.base_borrow_duration = admin_account.default_borrow_duration;
    params.base_fee_recipient = admin_account.base_fee_recipient;
    params.fee_recipient = partner.key(); // 合作伙伴手续费接收账户设为调用者
    params.fee_split = admin_account.default_fee_split;

    // msg!("Created params for partner: {}", partner.key());
    // msg!("Base swap fee: {}", params.base_swap_fee);
    // msg!("Base borrow fee: {}", params.base_borrow_fee);
    // msg!("Base borrow duration: {}", params.base_borrow_duration);
    // msg!("Base fee recipient: {}", params.base_fee_recipient);
    // msg!("Partner fee recipient: {}", params.fee_recipient);
    // msg!("Fee split: {}%", params.fee_split);
    
    Ok(())
}

// 更新合作伙伴参数的处理函数（只有超级管理员可以调用）
pub fn update_params(
    ctx: Context<UpdateParams>,
    partner_pubkey: Pubkey,
    base_swap_fee: Option<u16>,
    base_borrow_fee: Option<u16>,
    base_borrow_duration: Option<u32>,
    base_fee_recipient: Option<Pubkey>,
    fee_split: Option<u8>,
) -> Result<()> {
    // 双重验证：确保调用者确实是超级管理员
    let admin_account = &ctx.accounts.admin_account;
    let admin_signer = &ctx.accounts.admin;
    
    // 第一重验证：检查admin账户是否已初始化
    if admin_account.admin == Pubkey::default() {
        // msg!("Admin account not properly initialized");
        return Err(ErrorCode::Unauthorized.into());
    }

    // 第二重验证：确认调用者就是存储的管理员
    if admin_account.admin != admin_signer.key() {
        // msg!("Caller is not the stored admin. Expected: {}, Got: {}",
        //      admin_account.admin, admin_signer.key());
        return Err(ErrorCode::Unauthorized.into());
    }

    // msg!("Admin verification passed for: {}", admin_signer.key());
    
    let params = &mut ctx.accounts.params;
    
    // 设置或更新交易费率
    if let Some(fee) = base_swap_fee {
        params.base_swap_fee = fee;
        // msg!("Updated base swap fee: {}", fee);
    }

    // 设置或更新借贷费率
    if let Some(fee) = base_borrow_fee {
        params.base_borrow_fee = fee;
        // msg!("Updated base borrow fee: {}", fee);
    }

    // 设置或更新借贷时长
    if let Some(duration) = base_borrow_duration {
        params.base_borrow_duration = duration;
        // msg!("Updated base borrow duration (seconds): {}", duration);
    }

    // 设置或更新基础手续费接收地址
    if let Some(recipient) = base_fee_recipient {
        params.base_fee_recipient = recipient;
        // msg!("Updated base fee recipient: {}", recipient);
    }

    // 设置或更新手续费分配比例
    if let Some(split) = fee_split {
        if split > 100 {
            return Err(ErrorCode::InvalidFeePercentage.into());
        }
        params.fee_split = split;
        // msg!("Updated fee split: {}%", split);
    }

    // msg!("Parameters updated successfully for partner: {}", partner_pubkey);
    
    Ok(())
} 