// 导入所需的模块和依赖项
use {
    // 导入曲线AMM模块
    crate::curve::curve_amm::CurveAMM,
    // 导入参数结构和账户结构
    crate::instructions::contexts::CreateToken,
    crate::instructions::events::TokenCreatedEvent,
    // 导入 Anchor 框架的基础组件
    anchor_lang::prelude::*,
    // 导入系统程序相关功能
    anchor_lang::solana_program::{program::invoke, system_instruction},
    // 导入 Anchor 对 SPL 代币标准的支持
    anchor_spl::token::{
        mint_to, set_authority, spl_token::instruction::AuthorityType, MintTo, SetAuthority,
    },
    // 导入 Metaplex Token Metadata 的核心组件
    mpl_token_metadata::instructions::{CreateMetadataAccountV3, CreateMetadataAccountV3InstructionArgs},
    mpl_token_metadata::types::DataV2,
};

// 创建基本代币指令的处理函数
pub fn create_token(
    ctx: Context<CreateToken>,
    // 代币名称
    name: String,
    // 代币符号
    symbol: String,
    // 代币元数据URI
    uri: String,
) -> Result<()> {
    // 输出日志消息，在交易日志中可见
    // msg!("正在创建基本代币");
    // msg!("名称: {}", name);
    // msg!("符号: {}", symbol);
    // msg!("URI: {}", uri);

    // 输出铸币账户地址
    // msg!("铸币账户地址: {}", ctx.accounts.mint_account.key());
    // msg!("铸币权限: {}", ctx.accounts.curve_account.key());

    // 获取借贷流动池账户地址用于日志输出
    let curve_address = ctx.accounts.curve_account.key();

    // 创建一个新的BorrowingBondingCurve账户，用于管理资金池
    // 初始化所有值为0
    ctx.accounts.curve_account.lp_token_reserve = 0;
    ctx.accounts.curve_account.lp_sol_reserve = 0;
    ctx.accounts.curve_account.price = 0;
    ctx.accounts.curve_account.borrow_token_reserve = 0;
    ctx.accounts.curve_account.borrow_sol_reserve = 0;

    // 从参数账户读取费率设置
    ctx.accounts.curve_account.swap_fee = ctx.accounts.params.base_swap_fee;
    ctx.accounts.curve_account.borrow_fee = ctx.accounts.params.base_borrow_fee;
    ctx.accounts.curve_account.fee_discount_flag = 0; // 原价
    ctx.accounts.curve_account.borrow_duration = ctx.accounts.params.base_borrow_duration;
    //ctx.accounts.curve_account.borrow_start_price_deviation = ctx.accounts.params.base_borrow_start_price_deviation;
    // 设置手续费接收账户
    ctx.accounts.curve_account.base_fee_recipient = ctx.accounts.params.base_fee_recipient;
    ctx.accounts.curve_account.fee_recipient = ctx.accounts.params.fee_recipient;
    ctx.accounts.curve_account.fee_split = ctx.accounts.params.fee_split;

    ctx.accounts.curve_account.mint = ctx.accounts.mint_account.key();

    // msg!("借贷流动池账户地址: {}", curve_address);
    // msg!("设置交换费率: {}", ctx.accounts.curve_account.swap_fee);
    // msg!("设置借贷费率: {}", ctx.accounts.curve_account.borrow_fee);
    // msg!(
    //     "设置手续费接收账户: {}",
    //     ctx.accounts.curve_account.fee_recipient
    // );

    // 获取curve_account的bump值
    let (_, curve_bump) = Pubkey::find_program_address(
        &[b"borrowing_curve", ctx.accounts.mint_account.key().as_ref()],
        &ctx.program_id,
    );

    // 初始化为低池值 代币为1073个 SOL为0.03个 价格为 28
    // 这里只能手续费 直接设置成大家都有的 否则谁来换?
    ctx.accounts.curve_account.lp_token_reserve = 1073000000000000;
    ctx.accounts.curve_account.mint = ctx.accounts.mint_account.key();

    // 设置新的订单账本地址
    ctx.accounts.curve_account.up_orderbook = ctx.accounts.up_orderbook.key();
    ctx.accounts.curve_account.down_orderbook = ctx.accounts.down_orderbook.key();

    // 初始化 up_orderbook (做空订单账本)
    {
        let mut up_orderbook_data = ctx.accounts.up_orderbook.load_init()?;
        up_orderbook_data.version = crate::instructions::structs::OrderBook::CURRENT_VERSION;
        up_orderbook_data.order_type = 2; // 2=做空/Up
        up_orderbook_data.bump = ctx.bumps.up_orderbook;
        up_orderbook_data.authority = ctx.accounts.payer.key();
        up_orderbook_data.order_id_counter = 0;
        up_orderbook_data.created_at = Clock::get()?.unix_timestamp as u32;
        up_orderbook_data.last_modified = Clock::get()?.unix_timestamp as u32;
        up_orderbook_data.total_capacity = 0;
        up_orderbook_data.head = u16::MAX; // 空链表
        up_orderbook_data.tail = u16::MAX; // 空链表
        up_orderbook_data.total = 0;
    }

    // 初始化 down_orderbook (做多订单账本)
    {
        let mut down_orderbook_data = ctx.accounts.down_orderbook.load_init()?;
        down_orderbook_data.version = crate::instructions::structs::OrderBook::CURRENT_VERSION;
        down_orderbook_data.order_type = 1; // 1=做多/Down
        down_orderbook_data.bump = ctx.bumps.down_orderbook;
        down_orderbook_data.authority = ctx.accounts.payer.key();
        down_orderbook_data.order_id_counter = 0;
        down_orderbook_data.created_at = Clock::get()?.unix_timestamp as u32;
        down_orderbook_data.last_modified = Clock::get()?.unix_timestamp as u32;
        down_orderbook_data.total_capacity = 0;
        down_orderbook_data.head = u16::MAX; // 空链表
        down_orderbook_data.tail = u16::MAX; // 空链表
        down_orderbook_data.total = 0;
    }

    // // 旧的链表头(保留待删)
    // ctx.accounts.curve_account.up_head = None;
    // ctx.accounts.curve_account.down_head = None;

    // 铸币到流动池代币账户
    // msg!("铸币到流动池代币账户: {} 个代币", 1073000000000000i64);
    mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.mint_account.to_account_info(),
                to: ctx.accounts.pool_token_account.to_account_info(),
                authority: ctx.accounts.curve_account.to_account_info(),
            },
            &[&[
                b"borrowing_curve",
                ctx.accounts.mint_account.key().as_ref(),
                &[curve_bump],
            ]],
        ),
        1073000000000000, // 铸造1,073,000,000个代币
    )?;

    // 更新流动池代币余额
    ctx.accounts.curve_account.lp_token_reserve = 1073000000000000;

    // // 给流动池sol账户转入 0.03 SOL
    // invoke(
    //     &system_instruction::transfer(
    //         ctx.accounts.payer.key,
    //         ctx.accounts.pool_sol_account.key,
    //         10000000000, // 0.03 SOL
    //     ),
    //     &[
    //         ctx.accounts.payer.to_account_info().clone(),
    //         ctx.accounts.pool_sol_account.clone(),
    //         ctx.accounts.system_program.to_account_info().clone(),
    //     ],
    // )?;
    //ctx.accounts.curve_account.lp_sol_reserve = 30000000000;

    // 铸币到借贷池代币账户
    // msg!("铸币给借贷: {} 个代币", 536500000000000i64);
    mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.mint_account.to_account_info(),
                to: ctx.accounts.pool_token_account.to_account_info(),
                authority: ctx.accounts.curve_account.to_account_info(),
            },
            &[&[
                b"borrowing_curve",
                ctx.accounts.mint_account.key().as_ref(),
                &[curve_bump],
            ]],
        ),
        536500000000000, // 铸造536500000000000个代币
    )?;

    // 更新借贷池代币余额
    ctx.accounts.curve_account.borrow_token_reserve = 536500000000000;
    // 更新虚拟借贷池sol的数量 1千万个sol永远用不完
    ctx.accounts.curve_account.borrow_sol_reserve = 10000000000000000;
    // 更新虚拟流动池sol的数量
    ctx.accounts.curve_account.lp_sol_reserve = 30000000000;

    // 计算并设置精确的初始价格
    let initial_price = CurveAMM::get_initial_price()
        .ok_or(error!(crate::error::ErrorCode::InitialPriceCalculationError))?;
    ctx.accounts.curve_account.price = initial_price;

    // msg!("计算出的精确初始价格: {}", initial_price);

    // 检查是否需要应用手续费折扣
    crate::apply_fee_discount_if_needed!(ctx)?;

    // 从 payer 转账 10000 lamports 到 pool_sol_account
    // msg!("正在从 payer 转账 10000 lamports 到 pool_sol_account 目的是为了防止亿分之几的可能性sol不足");
    invoke(
        &system_instruction::transfer(
            ctx.accounts.payer.key,
            ctx.accounts.pool_sol_account.key,
            10000, // 10000 lamports  //  目的是为了防止 亿分之几的可能性sol不足
            //109120  // 方便调试用 109120 上线前改回10000  
        ),
        &[
            ctx.accounts.payer.to_account_info().clone(),
            ctx.accounts.pool_sol_account.clone(),
            ctx.accounts.system_program.to_account_info().clone(),
        ],
    )?;
    // msg!("成功转账 10000 lamports 到 pool_sol_account");
    // msg!(
    //     "完成创币后 pool_sol_account 池子SOL余额: {}",
    //     ctx.accounts.pool_sol_account.lamports()
    // );

    // ======================================================================
    // 创建 Metaplex Token Metadata (必须在销毁铸币权限之前完成)
    // ======================================================================
    // msg!("正在创建 Metaplex Token Metadata...");

    // 构建 CreateMetadataAccountV3 指令结构
    let metadata_instruction = CreateMetadataAccountV3 {
        metadata: ctx.accounts.metadata.key(),
        mint: ctx.accounts.mint_account.key(),
        mint_authority: ctx.accounts.curve_account.key(),
        payer: ctx.accounts.payer.key(),
        update_authority: (ctx.accounts.curve_account.key(), true),
        system_program: ctx.accounts.system_program.key(),
        rent: Some(ctx.accounts.rent.key()),
    };

    // 构建 DataV2 结构 - 包含代币的所有元数据信息
    let data = DataV2 {
        name: name.clone(),
        symbol: symbol.clone(),
        uri: uri.clone(),
        seller_fee_basis_points: 0,
        creators: Some(vec![
            mpl_token_metadata::types::Creator {
                address: ctx.accounts.payer.key(),
                verified: false,
                share: 100,
            }
        ]),
        collection: None,
        uses: None,
    };

    // 构建 CreateMetadataAccountV3InstructionArgs - 指令参数
    let args = CreateMetadataAccountV3InstructionArgs {
        data,
        is_mutable: true,     // 允许更新元数据
        collection_details: None,
    };

    // 生成最终的 Solana 指令
    let create_metadata_ix = metadata_instruction.instruction(args);

    // 执行跨程序调用（CPI）创建元数据，使用curve_account作为签名权限
    anchor_lang::solana_program::program::invoke_signed(
        &create_metadata_ix,
        &[
            ctx.accounts.metadata.to_account_info(),
            ctx.accounts.mint_account.to_account_info(),
            ctx.accounts.curve_account.to_account_info(),  // 使用curve_account作为mint_authority
            ctx.accounts.payer.to_account_info(),
            ctx.accounts.curve_account.to_account_info(),  // 使用curve_account作为update_authority
            ctx.accounts.system_program.to_account_info(),
            ctx.accounts.rent.to_account_info(),
        ],
        &[&[
            b"borrowing_curve",
            ctx.accounts.mint_account.key().as_ref(),
            &[curve_bump],
        ]],
    )?;

    // msg!("✅ Metaplex Token Metadata 创建成功！");
    // msg!("🏷️  代币名称: {}", name);
    // msg!("🔖 代币符号: {}", symbol);
    // msg!("🔗 元数据 URI: {}", uri);

    // // 输出成功日志
    // msg!("基本代币及借贷流动池创建成功！");

    // // 永久销毁铸币权限，确保代币不可再增发
    // msg!("正在销毁铸币权限，使代币不可再增发...");
    set_authority(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            SetAuthority {
                account_or_mint: ctx.accounts.mint_account.to_account_info(),
                current_authority: ctx.accounts.curve_account.to_account_info(),
            },
            &[&[
                b"borrowing_curve",
                ctx.accounts.mint_account.key().as_ref(),
                &[curve_bump],
            ]],
        ),
        AuthorityType::MintTokens,
        None, // 设置为None意味着永久删除铸币权限
    )?;

    // msg!("铸币权限已永久销毁，代币供应量已固定！");

    // // 永久销毁冻结权限，确保代币不能被冻结
    // msg!("正在销毁冻结权限，使代币不能被冻结...");
    set_authority(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            SetAuthority {
                account_or_mint: ctx.accounts.mint_account.to_account_info(),
                current_authority: ctx.accounts.curve_account.to_account_info(),
            },
            &[&[
                b"borrowing_curve",
                ctx.accounts.mint_account.key().as_ref(),
                &[curve_bump],
            ]],
        ),
        AuthorityType::FreezeAccount,
        None, // 设置为None意味着永久删除冻结权限
    )?;

    // msg!("冻结权限已永久销毁，代币不能被冻结！");

    // // 打印 curve_account 的所有参数信息
    // msg!("========== create_basic_token curve_account 全部参数信息 ==========");
    // msg!("lp_token_reserve (代币在流动池中的数量): {}", ctx.accounts.curve_account.lp_token_reserve);
    // msg!("lp_sol_reserve (SOL在流动池中的数量): {}", ctx.accounts.curve_account.lp_sol_reserve);
    // msg!("price (当前价格): {}", ctx.accounts.curve_account.price);
    // msg!("borrow_token_reserve (代币在虚拟借贷池中的数量): {}", ctx.accounts.curve_account.borrow_token_reserve);
    // msg!("borrow_sol_reserve (SOL在虚拟借贷池中的数量): {}", ctx.accounts.curve_account.borrow_sol_reserve);
    // msg!("swap_fee (现货交易手续费): {}", ctx.accounts.curve_account.swap_fee);
    // msg!("borrow_fee (保证金交易手续费): {}", ctx.accounts.curve_account.borrow_fee);
    // msg!("fee_discount_flag (手续费折扣标志): {}", ctx.accounts.curve_account.fee_discount_flag);
    // msg!("base_fee_recipient (技术提供方基础手续费接收账户): {}", ctx.accounts.curve_account.base_fee_recipient);
    // msg!("fee_recipient (合作伙伴手续费接收账户): {}", ctx.accounts.curve_account.fee_recipient);
    // msg!("fee_split (手续费分配比例): {}", ctx.accounts.curve_account.fee_split);
    // msg!("borrow_duration (贷款时长秒): {}", ctx.accounts.curve_account.borrow_duration);
    // msg!("mint (关联的代币铸造账户): {}", ctx.accounts.curve_account.mint);
    // msg!("up_head (做空订单的链表头部): {:?}", ctx.accounts.curve_account.up_head);
    // msg!("down_head (做多订单的链表头部): {:?}", ctx.accounts.curve_account.down_head);
    // msg!("bump (PDA bump): {}", ctx.accounts.curve_account.bump);
    // msg!("===============================================");


    // 发出代币创建事件
    emit!(TokenCreatedEvent {
        payer: ctx.accounts.payer.key(),
        mint_account: ctx.accounts.mint_account.key(),
        curve_account: curve_address,
        pool_token_account: ctx.accounts.pool_token_account.key(),
        pool_sol_account: ctx.accounts.pool_sol_account.key(),
        fee_recipient: ctx.accounts.curve_account.fee_recipient,
        base_fee_recipient: ctx.accounts.params.base_fee_recipient, // 添加基础手续费接收账户
        params_account: ctx.accounts.params.key(),                  // 添加合作伙伴参数账户PDA地址
        swap_fee: ctx.accounts.curve_account.swap_fee,              // 添加现货交易手续费
        borrow_fee: ctx.accounts.curve_account.borrow_fee,          // 添加保证金交易手续费
        fee_discount_flag: ctx.accounts.curve_account.fee_discount_flag, // 添加手续费折扣标志
        name: name.clone(),
        symbol: symbol.clone(),
        uri: uri.clone(),
        up_orderbook: ctx.accounts.up_orderbook.key(),              // 添加做空订单账本地址
        down_orderbook: ctx.accounts.down_orderbook.key(),          // 添加做多订单账本地址
        latest_price: ctx.accounts.curve_account.price,             // 添加最新价格
    });

    // 返回成功结果
    Ok(())
}
