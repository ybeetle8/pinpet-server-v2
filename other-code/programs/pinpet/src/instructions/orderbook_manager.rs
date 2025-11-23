use anchor_lang::prelude::*;
use crate::error::ErrorCode;
use crate::instructions::structs::{MarginOrder, OrderBook};

/// OrderBook 链表操作管理器
pub struct OrderBookManager;

impl OrderBookManager {
    /// 辅助函数：调整账户大小
    ///
    /// # 参数
    /// * `account` - 要调整大小的账户
    /// * `new_size` - 新的账户大小（字节）
    ///
    /// # 返回值
    /// 成功返回 Ok(())，失败返回相应错误
    fn resize_account<'a>(
        account: &AccountInfo<'a>,
        new_size: usize,
    ) -> Result<()> {
        account.realloc(new_size, false)?;
        Ok(())
    }

    /// 验证是否可以增加订单（内部辅助函数）
    ///
    /// # 参数
    /// * `current_total` - 当前订单总数
    /// * `new_total` - 插入后的新订单总数
    ///
    /// # 返回值
    /// 成功返回 Ok(()), 失败返回相应错误
    fn check_capacity_before_insert(
        current_total: u16,
        new_total: u16,
    ) -> Result<()> {
        // 1. 验证 new_total 大于 current_total (防止逻辑错误)
        require!(
            new_total > current_total,
            ErrorCode::OrderBookManagerInvalidAccountData
        );

        // 2. 验证不超过业务逻辑上限 MAX_CAPACITY
        require!(
            new_total as u32 <= OrderBook::MAX_CAPACITY,
            ErrorCode::OrderBookManagerExceedsMaxCapacity
        );

        // 3. 验证账户大小不超过 Solana 10MB 限制
        let new_size = OrderBook::account_size(new_total as u32);
        require!(
            new_size <= 10 * 1024 * 1024,  // 10MB
            ErrorCode::OrderBookManagerExceedsAccountSizeLimit
        );

        Ok(())
    }

    /// 获取指定索引的订单槽位（不可变引用）
    ///
    /// # 参数
    /// * `orderbook_data` - OrderBook 账户的原始数据（由调用者从 AccountInfo 借用）
    ///   - 使用方式: `let data = account.data.borrow(); OrderBookManager::get_order(&data, index)`
    ///   - 优点: 调用者控制借用生命周期，避免客户端传递错误数据
    /// * `index` - 要访问的订单槽位索引（0-based）
    ///
    /// # 返回值
    /// 返回指定索引位置的 MarginOrder 不可变引用
    pub fn get_order(orderbook_data: &[u8], index: u16) -> Result<&MarginOrder> {
        // 加载并验证 OrderBook 头部
        let orderbook = Self::load_orderbook_header(orderbook_data)?;

        // 验证索引范围
        require!(
            (index as u32) < orderbook.total_capacity,
            ErrorCode::OrderBookManagerInvalidSlotIndex
        );

        // 计算订单槽位的偏移量并验证边界
        let offset = 8 + OrderBook::HEADER_SIZE + (index as usize) * MarginOrder::SIZE;
        require!(
            offset + MarginOrder::SIZE <= orderbook_data.len(),
            ErrorCode::OrderBookManagerInvalidSlotIndex
        );

        // 零拷贝解析订单数据
        let slice = &orderbook_data[offset..offset + MarginOrder::SIZE];
        Ok(bytemuck::from_bytes(slice))
    }

    /// 获取指定索引的订单槽位（可变引用）
    ///
    /// # 参数
    /// * `orderbook_data` - OrderBook 账户的原始数据（由调用者从 AccountInfo 借用）
    ///   - 使用方式: `let mut data = account.data.borrow_mut(); OrderBookManager::get_order_mut(&mut data, index)`
    /// * `index` - 要访问的订单槽位索引（0-based）
    ///
    /// # 返回值
    /// 返回指定索引位置的 MarginOrder 可变引用
    ///
    /// # 安全性
    /// 此方法为私有方法，只在 OrderBookManager 内部使用，防止外部代码直接修改订单数据破坏链表结构
    fn get_order_mut(orderbook_data: &mut [u8], index: u16) -> Result<&mut MarginOrder> {
        // 加载并验证 OrderBook 头部
        let orderbook = Self::load_orderbook_header(orderbook_data)?;

        // 验证索引范围
        require!(
            (index as u32) < orderbook.total_capacity,
            ErrorCode::OrderBookManagerInvalidSlotIndex
        );

        // 计算订单槽位的偏移量并验证边界
        let offset = 8 + OrderBook::HEADER_SIZE + (index as usize) * MarginOrder::SIZE;
        require!(
            offset + MarginOrder::SIZE <= orderbook_data.len(),
            ErrorCode::OrderBookManagerInvalidSlotIndex
        );

        // 零拷贝解析订单数据
        let slice = &mut orderbook_data[offset..offset + MarginOrder::SIZE];
        Ok(bytemuck::from_bytes_mut(slice))
    }

    /// 从原始字节数据加载 OrderBook 头部（只读访问）
    ///
    /// # 参数
    /// * `data` - OrderBook 账户的原始数据
    ///
    /// # 返回值
    /// 返回 OrderBook 头部的不可变引用
    ///
    /// # 使用示例
    /// ```ignore
    /// let data = orderbook_account.data.borrow();
    /// let header = OrderBookManager::load_orderbook_header(&data)?;
    /// let total = header.total;
    /// let head = header.head;
    /// ```
    pub fn load_orderbook_header(data: &[u8]) -> Result<&OrderBook> {
        require!(
            data.len() >= 8 + OrderBook::HEADER_SIZE,
            ErrorCode::OrderBookManagerInvalidAccountData
        );
        let header_slice = &data[8..8 + OrderBook::HEADER_SIZE];
        Ok(bytemuck::from_bytes(header_slice))
    }

    /// 从原始字节数据加载 OrderBook 头部（可变引用）
    ///
    /// # 安全性
    /// 此方法为私有方法，只在 OrderBookManager 内部使用，防止外部代码直接修改 header 字段
    /// （如 total, total_capacity, order_id_counter 等）导致状态不一致
    fn load_orderbook_header_mut(data: &mut [u8]) -> Result<&mut OrderBook> {
        require!(
            data.len() >= 8 + OrderBook::HEADER_SIZE,
            ErrorCode::OrderBookManagerInvalidAccountData
        );
        let header_slice = &mut data[8..8 + OrderBook::HEADER_SIZE];
        Ok(bytemuck::from_bytes_mut(header_slice))
    }

    /// 安全地获取指定索引的订单（不可变引用）- 用于删除操作
    ///
    /// 与 get_order 的区别：
    /// - 不检查 total_capacity，只检查当前数据长度
    /// - 用于删除操作中，避免访问已缩小账户后的越界索引
    fn get_order_safe(data: &[u8], index: u16) -> Result<&MarginOrder> {
        let offset = 8 + OrderBook::HEADER_SIZE + (index as usize) * MarginOrder::SIZE;
        let end_offset = offset + MarginOrder::SIZE;

        // 验证索引范围 - 基于当前数据长度
        require!(
            end_offset <= data.len(),
            ErrorCode::OrderBookManagerDataOutOfBounds
        );

        let slice = &data[offset..end_offset];
        Ok(bytemuck::from_bytes(slice))
    }

    /// 安全地获取指定索引的订单（可变引用）- 用于删除操作
    ///
    /// 与 get_order_mut 的区别：
    /// - 不检查 total_capacity，只检查当前数据长度
    /// - 用于删除操作中，避免访问已缩小账户后的越界索引
    fn get_order_mut_safe(data: &mut [u8], index: u16) -> Result<&mut MarginOrder> {
        let offset = 8 + OrderBook::HEADER_SIZE + (index as usize) * MarginOrder::SIZE;
        let end_offset = offset + MarginOrder::SIZE;

        // 验证索引范围 - 基于当前数据长度
        require!(
            end_offset <= data.len(),
            ErrorCode::OrderBookManagerDataOutOfBounds
        );

        let slice = &mut data[offset..end_offset];
        Ok(bytemuck::from_bytes_mut(slice))
    }

    /// 在指定节点之后插入订单
    ///
    /// # 参数
    /// * `orderbook_account` - OrderBook 账户信息
    /// * `after_index` - 在此索引之后插入
    /// * `order_data` - 要插入的订单数据
    /// * `payer` - 支付账户信息
    /// * `system_program` - 系统程序
    ///
    /// # 返回值
    /// 返回 (插入的订单索引, 订单ID)
    pub fn insert_after<'a>(
        orderbook_account: &AccountInfo<'a>,
        after_index: u16,
        order_data: &MarginOrder,
        payer: &AccountInfo<'a>,
        system_program: &AccountInfo<'a>,
    ) -> Result<(u16, u64)> {
        // 读取当前头部信息并验证容量
        let (old_total, current_order_id) = {
            let data = orderbook_account.data.borrow();
            let orderbook = Self::load_orderbook_header(&data)?;
            let old_total = orderbook.total;
            let current_order_id = orderbook.order_id_counter;

            // ✅ 验证容量：确保插入后不超过限制
            let new_total = old_total.checked_add(1)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;
            Self::check_capacity_before_insert(old_total, new_total)?;

            (old_total, current_order_id)
        };

        // 处理空链表：插入第一个节点
        if old_total == 0 {
            // 计算新的账户大小
            let new_size = OrderBook::account_size(1 as u32);

            // Resize 扩容
            Self::resize_account(orderbook_account, new_size)?;

            // 支付租金
            let rent = Rent::get()?;
            let new_minimum_balance = rent.minimum_balance(new_size);
            let lamports_diff = new_minimum_balance.saturating_sub(orderbook_account.lamports());

            if lamports_diff > 0 {
                anchor_lang::system_program::transfer(
                    CpiContext::new(
                        system_program.clone(),
                        anchor_lang::system_program::Transfer {
                            from: payer.clone(),
                            to: orderbook_account.clone(),
                        },
                    ),
                    lamports_diff,
                )?;
            }

            // 获取可变引用
            let mut data = orderbook_account.data.borrow_mut();

            // 更新头部
            let orderbook_mut = Self::load_orderbook_header_mut(&mut data)?;
            orderbook_mut.head = 0;
            orderbook_mut.tail = 0;
            orderbook_mut.total = 1;
            orderbook_mut.total_capacity = 1;
            orderbook_mut.order_id_counter = current_order_id.checked_add(1)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;

            // 写入新订单
            let offset = 8 + OrderBook::HEADER_SIZE;
            let slice = &mut data[offset..offset + MarginOrder::SIZE];
            let new_order: &mut MarginOrder = bytemuck::from_bytes_mut(slice);
            *new_order = *order_data;
            new_order.order_id = current_order_id;
            new_order.prev_order = u16::MAX;
            new_order.next_order = u16::MAX;
            new_order.version = 1;

            return Ok((0, current_order_id));
        }

        // 验证索引
        require!(after_index < old_total, ErrorCode::OrderBookManagerInvalidSlotIndex);

        // 读取 after_index 节点信息
        let old_next = {
            let data = orderbook_account.data.borrow();
            let after_node = Self::get_order(&data, after_index)?;
            after_node.next_order
        };

        // 如果在尾节点后插入，内联处理
        if old_next == u16::MAX {
            // 计算新的账户大小
            let new_size = OrderBook::account_size(old_total as u32 + 1);

            // Resize 扩容
            Self::resize_account(orderbook_account, new_size)?;

            // 支付租金
            let rent = Rent::get()?;
            let new_minimum_balance = rent.minimum_balance(new_size);
            let lamports_diff = new_minimum_balance.saturating_sub(orderbook_account.lamports());

            if lamports_diff > 0 {
                anchor_lang::system_program::transfer(
                    CpiContext::new(
                        system_program.clone(),
                        anchor_lang::system_program::Transfer {
                            from: payer.clone(),
                            to: orderbook_account.clone(),
                        },
                    ),
                    lamports_diff,
                )?;
            }

            // 获取可变引用
            let mut data = orderbook_account.data.borrow_mut();

            // 修改旧尾节点
            let after_offset = 8 + OrderBook::HEADER_SIZE + (after_index as usize) * MarginOrder::SIZE;
            let after_slice = &mut data[after_offset..after_offset + MarginOrder::SIZE];
            let after_order: &mut MarginOrder = bytemuck::from_bytes_mut(after_slice);
            after_order.next_order = old_total;
            after_order.version = after_order.version.checked_add(1)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;

            // 写入新订单
            let new_offset = 8 + OrderBook::HEADER_SIZE + (old_total as usize) * MarginOrder::SIZE;
            let new_slice = &mut data[new_offset..new_offset + MarginOrder::SIZE];
            let new_order: &mut MarginOrder = bytemuck::from_bytes_mut(new_slice);
            *new_order = *order_data;
            new_order.order_id = current_order_id;
            new_order.prev_order = after_index;
            new_order.next_order = u16::MAX;
            new_order.version = 1;

            // 更新头部
            let orderbook_mut = Self::load_orderbook_header_mut(&mut data)?;
            orderbook_mut.tail = old_total;
            orderbook_mut.total = old_total.checked_add(1)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;
            orderbook_mut.total_capacity = (old_total.checked_add(1)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?) as u32;
            orderbook_mut.order_id_counter = current_order_id.checked_add(1)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;

            return Ok((old_total, current_order_id));
        }

        // 计算新的账户大小
        let new_size = OrderBook::account_size(old_total as u32 + 1);

        // Resize 扩容
        Self::resize_account(orderbook_account, new_size)?;

        // 支付租金
        let rent = Rent::get()?;
        let new_minimum_balance = rent.minimum_balance(new_size);
        let lamports_diff = new_minimum_balance.saturating_sub(orderbook_account.lamports());

        if lamports_diff > 0 {
            anchor_lang::system_program::transfer(
                CpiContext::new(
                    system_program.clone(),
                    anchor_lang::system_program::Transfer {
                        from: payer.clone(),
                        to: orderbook_account.clone(),
                    },
                ),
                lamports_diff,
            )?;
        }

        // 获取可变引用并写入数据
        let mut data = orderbook_account.data.borrow_mut();

        // 修改 after_index 节点
        let after_offset = 8 + OrderBook::HEADER_SIZE + (after_index as usize) * MarginOrder::SIZE;
        let after_slice = &mut data[after_offset..after_offset + MarginOrder::SIZE];
        let after_order: &mut MarginOrder = bytemuck::from_bytes_mut(after_slice);
        after_order.next_order = old_total;
        after_order.version = after_order.version.checked_add(1)
            .ok_or(ErrorCode::OrderBookManagerOverflow)?;

        // 修改 old_next 节点
        let old_next_offset = 8 + OrderBook::HEADER_SIZE + (old_next as usize) * MarginOrder::SIZE;
        let old_next_slice = &mut data[old_next_offset..old_next_offset + MarginOrder::SIZE];
        let old_next_order: &mut MarginOrder = bytemuck::from_bytes_mut(old_next_slice);
        old_next_order.prev_order = old_total;
        old_next_order.version = old_next_order.version.checked_add(1)
            .ok_or(ErrorCode::OrderBookManagerOverflow)?;

        // 写入新订单
        let new_offset = 8 + OrderBook::HEADER_SIZE + (old_total as usize) * MarginOrder::SIZE;
        let new_slice = &mut data[new_offset..new_offset + MarginOrder::SIZE];
        let new_order: &mut MarginOrder = bytemuck::from_bytes_mut(new_slice);
        *new_order = *order_data;
        new_order.order_id = current_order_id;
        new_order.prev_order = after_index;
        new_order.next_order = old_next;
        new_order.version = 1;

        // 更新头部
        let orderbook_mut = Self::load_orderbook_header_mut(&mut data)?;
        orderbook_mut.total = old_total.checked_add(1)
            .ok_or(ErrorCode::OrderBookManagerOverflow)?;
        orderbook_mut.total_capacity = (old_total.checked_add(1)
            .ok_or(ErrorCode::OrderBookManagerOverflow)?) as u32;
        orderbook_mut.order_id_counter = current_order_id.checked_add(1)
            .ok_or(ErrorCode::OrderBookManagerOverflow)?;

        Ok((old_total, current_order_id))
    }

    /// 在指定节点之前插入订单
    ///
    /// # 参数
    /// * `orderbook_account` - OrderBook 账户信息
    /// * `before_index` - 在此索引之前插入
    /// * `order_data` - 要插入的订单数据
    /// * `payer` - 支付账户信息
    /// * `system_program` - 系统程序
    ///
    /// # 返回值
    /// 返回 (插入的订单索引, 订单ID)
    pub fn insert_before<'a>(
        orderbook_account: &AccountInfo<'a>,
        before_index: u16,
        order_data: &MarginOrder,
        payer: &AccountInfo<'a>,
        system_program: &AccountInfo<'a>,
    ) -> Result<(u16, u64)> {
        // 读取当前头部信息并验证容量
        let (old_total, current_order_id) = {
            let data = orderbook_account.data.borrow();
            let orderbook = Self::load_orderbook_header(&data)?;
            let old_total = orderbook.total;
            let current_order_id = orderbook.order_id_counter;

            // ✅ 验证容量：确保插入后不超过限制
            let new_total = old_total.checked_add(1)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;
            Self::check_capacity_before_insert(old_total, new_total)?;

            (old_total, current_order_id)
        };

        // 处理空链表：插入第一个节点
        if old_total == 0 {
            // 计算新的账户大小
            let new_size = OrderBook::account_size(1 as u32);

            // Resize 扩容
            Self::resize_account(orderbook_account, new_size)?;

            // 支付租金
            let rent = Rent::get()?;
            let new_minimum_balance = rent.minimum_balance(new_size);
            let lamports_diff = new_minimum_balance.saturating_sub(orderbook_account.lamports());

            if lamports_diff > 0 {
                anchor_lang::system_program::transfer(
                    CpiContext::new(
                        system_program.clone(),
                        anchor_lang::system_program::Transfer {
                            from: payer.clone(),
                            to: orderbook_account.clone(),
                        },
                    ),
                    lamports_diff,
                )?;
            }

            // 获取可变引用
            let mut data = orderbook_account.data.borrow_mut();

            // 更新头部
            let orderbook_mut = Self::load_orderbook_header_mut(&mut data)?;
            orderbook_mut.head = 0;
            orderbook_mut.tail = 0;
            orderbook_mut.total = 1;
            orderbook_mut.total_capacity = 1;
            orderbook_mut.order_id_counter = current_order_id.checked_add(1)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;

            // 写入新订单
            let offset = 8 + OrderBook::HEADER_SIZE;
            let slice = &mut data[offset..offset + MarginOrder::SIZE];
            let new_order: &mut MarginOrder = bytemuck::from_bytes_mut(slice);
            *new_order = *order_data;
            new_order.order_id = current_order_id;
            new_order.prev_order = u16::MAX;
            new_order.next_order = u16::MAX;
            new_order.version = 1;

            return Ok((0, current_order_id));
        }

        // 验证索引
        require!(before_index < old_total, ErrorCode::OrderBookManagerInvalidSlotIndex);

        // 读取 before_index 节点信息
        let old_prev = {
            let data = orderbook_account.data.borrow();
            let before_node = Self::get_order(&data, before_index)?;
            before_node.prev_order
        };

        // 如果在头节点前插入，内联处理
        if old_prev == u16::MAX {
            // 计算新的账户大小
            let new_size = OrderBook::account_size(old_total as u32 + 1);

            // Resize 扩容
            Self::resize_account(orderbook_account, new_size)?;

            // 支付租金
            let rent = Rent::get()?;
            let new_minimum_balance = rent.minimum_balance(new_size);
            let lamports_diff = new_minimum_balance.saturating_sub(orderbook_account.lamports());

            if lamports_diff > 0 {
                anchor_lang::system_program::transfer(
                    CpiContext::new(
                        system_program.clone(),
                        anchor_lang::system_program::Transfer {
                            from: payer.clone(),
                            to: orderbook_account.clone(),
                        },
                    ),
                    lamports_diff,
                )?;
            }

            // 获取可变引用
            let mut data = orderbook_account.data.borrow_mut();

            // 修改旧头节点
            let before_offset = 8 + OrderBook::HEADER_SIZE + (before_index as usize) * MarginOrder::SIZE;
            let before_slice = &mut data[before_offset..before_offset + MarginOrder::SIZE];
            let before_order: &mut MarginOrder = bytemuck::from_bytes_mut(before_slice);
            before_order.prev_order = old_total;
            before_order.version = before_order.version.checked_add(1)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;

            // 写入新订单
            let new_offset = 8 + OrderBook::HEADER_SIZE + (old_total as usize) * MarginOrder::SIZE;
            let new_slice = &mut data[new_offset..new_offset + MarginOrder::SIZE];
            let new_order: &mut MarginOrder = bytemuck::from_bytes_mut(new_slice);
            *new_order = *order_data;
            new_order.order_id = current_order_id;
            new_order.prev_order = u16::MAX;
            new_order.next_order = before_index;
            new_order.version = 1;

            // 更新头部
            let orderbook_mut = Self::load_orderbook_header_mut(&mut data)?;
            orderbook_mut.head = old_total;
            orderbook_mut.total = old_total.checked_add(1)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;
            orderbook_mut.total_capacity = (old_total.checked_add(1)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?) as u32;
            orderbook_mut.order_id_counter = current_order_id.checked_add(1)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;

            return Ok((old_total, current_order_id));
        }

        // 计算新的账户大小
        let new_size = OrderBook::account_size(old_total as u32 + 1);

        // Resize 扩容
        Self::resize_account(orderbook_account, new_size)?;

        // 支付租金
        let rent = Rent::get()?;
        let new_minimum_balance = rent.minimum_balance(new_size);
        let lamports_diff = new_minimum_balance.saturating_sub(orderbook_account.lamports());

        if lamports_diff > 0 {
            anchor_lang::system_program::transfer(
                CpiContext::new(
                    system_program.clone(),
                    anchor_lang::system_program::Transfer {
                        from: payer.clone(),
                        to: orderbook_account.clone(),
                    },
                ),
                lamports_diff,
            )?;
        }

        // 获取可变引用并写入数据
        let mut data = orderbook_account.data.borrow_mut();

        // 修改 before_index 节点
        let before_offset = 8 + OrderBook::HEADER_SIZE + (before_index as usize) * MarginOrder::SIZE;
        let before_slice = &mut data[before_offset..before_offset + MarginOrder::SIZE];
        let before_order: &mut MarginOrder = bytemuck::from_bytes_mut(before_slice);
        before_order.prev_order = old_total;
        before_order.version = before_order.version.checked_add(1)
            .ok_or(ErrorCode::OrderBookManagerOverflow)?;

        // 修改 old_prev 节点
        let old_prev_offset = 8 + OrderBook::HEADER_SIZE + (old_prev as usize) * MarginOrder::SIZE;
        let old_prev_slice = &mut data[old_prev_offset..old_prev_offset + MarginOrder::SIZE];
        let old_prev_order: &mut MarginOrder = bytemuck::from_bytes_mut(old_prev_slice);
        old_prev_order.next_order = old_total;
        old_prev_order.version = old_prev_order.version.checked_add(1)
            .ok_or(ErrorCode::OrderBookManagerOverflow)?;

        // 写入新订单
        let new_offset = 8 + OrderBook::HEADER_SIZE + (old_total as usize) * MarginOrder::SIZE;
        let new_slice = &mut data[new_offset..new_offset + MarginOrder::SIZE];
        let new_order: &mut MarginOrder = bytemuck::from_bytes_mut(new_slice);
        *new_order = *order_data;
        new_order.order_id = current_order_id;
        new_order.prev_order = old_prev;
        new_order.next_order = before_index;
        new_order.version = 1;

        // 更新头部
        let orderbook_mut = Self::load_orderbook_header_mut(&mut data)?;
        orderbook_mut.total = old_total.checked_add(1)
            .ok_or(ErrorCode::OrderBookManagerOverflow)?;
        orderbook_mut.total_capacity = (old_total.checked_add(1)
            .ok_or(ErrorCode::OrderBookManagerOverflow)?) as u32;
        orderbook_mut.order_id_counter = current_order_id.checked_add(1)
            .ok_or(ErrorCode::OrderBookManagerOverflow)?;

        Ok((old_total, current_order_id))
    }

    /// 遍历订单（不可变，支持批量和续传）
    ///
    /// # 参数
    /// * `orderbook_data` - OrderBook 账户数据
    /// * `start` - 起始索引（u16::MAX = 从 head 开始）
    /// * `limit` - 最多处理数量（0 = 无限制）
    /// * `callback` - 回调函数 fn(index, order) -> Result<bool>
    ///   - 返回 true: 继续
    ///   - 返回 false: 中断
    ///
    /// # 返回值
    /// TraversalResult { processed, next, done }
    pub fn traverse<F>(
        orderbook_data: &[u8],
        start: u16,
        limit: u32,
        mut callback: F,
    ) -> Result<TraversalResult>
    where
        F: FnMut(u16, &MarginOrder) -> Result<bool>,
    {
        let orderbook = Self::load_orderbook_header(orderbook_data)?;

        // 确定起始位置
        let mut current = if start == u16::MAX {
            orderbook.head
        } else {
            start
        };

        // 空链表或无效起始
        if current == u16::MAX {
            return Ok(TraversalResult {
                processed: 0,
                next: u16::MAX,
                done: true,
            });
        }

        let mut count = 0;

        loop {
            // 验证索引有效性
            require!(current < orderbook.total, ErrorCode::OrderBookManagerInvalidSlotIndex);

            // 读取订单
            let order = Self::get_order(orderbook_data, current)?;

            // 执行回调
            let should_continue = callback(current, order)?;
            count += 1;

            // 用户主动中断
            if !should_continue {
                return Ok(TraversalResult {
                    processed: count,
                    next: order.next_order,
                    done: false,
                });
            }

            // 达到限制
            if limit > 0 && count >= limit {
                return Ok(TraversalResult {
                    processed: count,
                    next: order.next_order,
                    done: order.next_order == u16::MAX,
                });
            }

            // 到达尾部
            if order.next_order == u16::MAX {
                return Ok(TraversalResult {
                    processed: count,
                    next: u16::MAX,
                    done: true,
                });
            }

            current = order.next_order;
        }
    }


    /// 更新指定索引的订单（需要 order_id 双重验证）
    ///
    /// # 参数
    /// * `orderbook_account` - OrderBook 账户信息
    /// * `update_index` - 要更新的订单索引
    /// * `order_id` - 订单 ID（用于验证）
    /// * `update_data` - 更新数据（只包含可更新的字段）
    ///
    /// # 不可更新字段
    /// - user: 开仓用户
    /// - order_id: 订单唯一标识符
    /// - start_time: 订单开始时间戳
    /// - order_type: 订单类型（做多/做空）
    ///
    /// # 返回值
    /// 成功返回 Ok(())
    pub fn update_order<'a>(
        orderbook_account: &AccountInfo<'a>,
        update_index: u16,
        order_id: u64,
        update_data: &MarginOrderUpdateData,
    ) -> Result<()> {
        // 1. 读取并验证
        let data = orderbook_account.data.borrow();
        let orderbook = Self::load_orderbook_header(&data)?;

        // 验证索引范围
        require!(update_index < orderbook.total, ErrorCode::OrderBookManagerInvalidSlotIndex);

        // 读取订单并验证 order_id
        let order = Self::get_order(&data, update_index)?;
        require!(order.order_id == order_id, ErrorCode::OrderBookManagerOrderIdMismatch);

        drop(data);

        // 2. 获取可变引用并执行更新
        let mut data = orderbook_account.data.borrow_mut();
        let order_mut = Self::get_order_mut(&mut data, update_index)?;

        // 3. 应用更新（只更新非 None 的字段）
        // 注意：next_order 和 prev_order 已从 MarginOrderUpdateData 中移除
        // 链表指针由系统内部管理，不允许外部修改，以防止链表结构被破坏
        if let Some(lock_lp_start_price) = update_data.lock_lp_start_price {
            order_mut.lock_lp_start_price = lock_lp_start_price;
        }
        if let Some(lock_lp_end_price) = update_data.lock_lp_end_price {
            order_mut.lock_lp_end_price = lock_lp_end_price;
        }
        if let Some(lock_lp_sol_amount) = update_data.lock_lp_sol_amount {
            order_mut.lock_lp_sol_amount = lock_lp_sol_amount;
        }
        if let Some(lock_lp_token_amount) = update_data.lock_lp_token_amount {
            order_mut.lock_lp_token_amount = lock_lp_token_amount;
        }
        if let Some(next_lp_sol_amount) = update_data.next_lp_sol_amount {
            order_mut.next_lp_sol_amount = next_lp_sol_amount;
        }
        if let Some(next_lp_token_amount) = update_data.next_lp_token_amount {
            order_mut.next_lp_token_amount = next_lp_token_amount;
        }
        if let Some(end_time) = update_data.end_time {
            order_mut.end_time = end_time;
        }
        if let Some(margin_init_sol_amount) = update_data.margin_init_sol_amount {
            order_mut.margin_init_sol_amount = margin_init_sol_amount;
        }
        if let Some(margin_sol_amount) = update_data.margin_sol_amount {
            order_mut.margin_sol_amount = margin_sol_amount;
        }
        if let Some(borrow_amount) = update_data.borrow_amount {
            order_mut.borrow_amount = borrow_amount;
        }
        if let Some(position_asset_amount) = update_data.position_asset_amount {
            order_mut.position_asset_amount = position_asset_amount;
        }
        if let Some(borrow_fee) = update_data.borrow_fee {
            order_mut.borrow_fee = borrow_fee;
        }
        if let Some(open_price) = update_data.open_price {
            order_mut.open_price = open_price;
        }
        if let Some(realized_sol_amount) = update_data.realized_sol_amount {
            order_mut.realized_sol_amount = realized_sol_amount;
        }

        // 更新版本号
        order_mut.version = order_mut.version.checked_add(1)
            .ok_or(ErrorCode::OrderBookManagerOverflow)?;

        Ok(())
    }


    /// 获取指定插入位置的前后邻居节点索引
    ///
    /// # 功能说明
    /// 用于在插入新节点前,确定新节点应该连接到哪两个现有节点之间
    ///
    /// # 参数
    /// * `orderbook_data` - OrderBook 账户数据
    /// * `insert_pos` - 插入位置标识:
    ///   - `u16::MAX`: 插入到链表头部(成为新的 head)
    ///   - 有效索引: 在该索引节点之后插入
    ///
    /// # 返回值
    /// `(prev_index: Option<u16>, next_index: Option<u16>)`
    /// - `prev_index`: 新节点的前驱索引
    ///   - `None` 表示新节点将成为头部(没有前驱)
    ///   - `Some(i)` 表示新节点将插入到索引 i 之后
    /// - `next_index`: 新节点的后继索引
    ///   - `None` 表示新节点将成为尾部(没有后继)
    ///   - `Some(i)` 表示新节点的下一个是索引 i
    ///
    /// # 边界情况
    /// 1. 空链表 (total=0): 返回 `(None, None)`
    /// 2. 插入到头部 (insert_pos=u16::MAX): 返回 `(None, Some(head))`
    /// 3. 插入到尾部 (insert_pos=tail): 返回 `(Some(tail), None)`
    /// 4. 插入到中间 (insert_pos=i): 返回 `(Some(i), Some(i.next_order))`
    ///
    /// # 使用示例
    /// ```ignore
    /// // 获取插入位置的邻居
    /// let (prev_idx, next_idx) = OrderBookManager::get_insert_neighbors(&data, insert_pos)?;
    ///
    /// // 使用返回值调用 insert_after 或 insert_before
    /// if let Some(prev) = prev_idx {
    ///     OrderBookManager::insert_after(account, prev, order_data, ...)?;
    /// }
    /// ```
    pub fn get_insert_neighbors(
        orderbook_data: &[u8],
        insert_pos: u16,
    ) -> Result<(Option<u16>, Option<u16>)> {
        // 加载 OrderBook 头部
        let orderbook = Self::load_orderbook_header(orderbook_data)?;

        // 情况 1: 空链表
        if orderbook.total == 0 {
            return Ok((None, None));
        }

        // 情况 2: 插入到头部 (insert_pos == u16::MAX)
        if insert_pos == u16::MAX {
            let head_idx = orderbook.head;

            // 头部应该有效 (因为 total > 0)
            if head_idx == u16::MAX {
                // 理论上不应该出现这种情况,如果出现说明数据不一致
                return err!(ErrorCode::OrderBookManagerInvalidAccountData);
            }

            return Ok((None, Some(head_idx)));
        }

        // 情况 3 & 4: 插入到指定节点之后
        // 验证索引有效性
        require!(
            insert_pos < orderbook.total,
            ErrorCode::OrderBookManagerInvalidSlotIndex
        );

        // 读取指定节点
        let node = Self::get_order(orderbook_data, insert_pos)?;

        // 确定后继节点
        let next_idx = if node.next_order == u16::MAX {
            None  // 插入到尾部
        } else {
            Some(node.next_order)  // 插入到中间
        };

        Ok((Some(insert_pos), next_idx))
    }

    /// 批量删除订单(按索引,无安全验证)
    ///
    /// # 警告
    /// ⚠️ 此函数不验证 order_id,调用者必须确保:
    /// 1. 所有索引都有效且在范围内
    /// 2. 索引对应的订单确实应该被删除
    /// 3. 不会因误删导致业务逻辑错误
    ///
    /// # 参数
    /// * `orderbook_account` - OrderBook 账户
    /// * `indices` - 待删除的索引切片(可乱序、可重复，函数内部会去重并降序排序)
    /// * `payer` - 接收返还租金的账户
    /// * `system_program` - 系统程序
    ///
    /// # 性能优化
    /// - 自动去重和降序排序(内部克隆一份处理,不影响原数组)
    /// - 只执行一次 resize 操作
    /// - 批量处理链表操作
    /// - 不返回删除的订单数据,避免不必要的内存分配
    ///
    /// # 返回值
    /// 成功返回 Ok(())
    ///
    /// # 示例
    /// ```ignore
    /// let indices = vec![5, 2, 8, 2]; // 乱序+重复
    /// OrderBookManager::batch_remove_by_indices_unsafe(
    ///     &orderbook,
    ///     &indices, // 函数内部会处理为 [8, 5, 2]
    ///     &payer,
    ///     &system_program,
    /// )?;
    /// // 删除成功
    /// ```
    pub fn batch_remove_by_indices_unsafe<'a>(
        orderbook_account: &AccountInfo<'a>,
        indices: &[u16],
        payer: &AccountInfo<'a>,
        _system_program: &AccountInfo<'a>,
    ) -> Result<()> {
        // 0. 处理空数组的情况
        if indices.is_empty() {
            return Ok(());
        }

        // 1. 克隆、去重并降序排序索引
        let mut sorted_indices = indices.to_vec();
        sorted_indices.sort_unstable_by(|a, b| b.cmp(a)); // 降序排序
        sorted_indices.dedup(); // 去重

        // 2. 读取初始状态
        let (old_total, old_head, old_tail) = {
            let data = orderbook_account.data.borrow();
            let orderbook = Self::load_orderbook_header(&data)?;

            // 验证链表非空
            require!(orderbook.total > 0, ErrorCode::OrderBookManagerEmptyOrderBook);

            (orderbook.total, orderbook.head, orderbook.tail)
        };

        // 验证所有索引都在有效范围内
        for &index in &sorted_indices {
            require!(
                index < old_total,
                ErrorCode::OrderBookManagerInvalidSlotIndex
            );
        }

        let delete_count = sorted_indices.len() as u16;

        // 检查是否删除全部
        if delete_count >= old_total {
            // 全部删除的特殊情况
            return Self::batch_remove_all(
                orderbook_account,
                payer,
                old_total,
            );
        }

        // 3. 批量处理删除操作(不 resize)
        {
            let mut data = orderbook_account.data.borrow_mut();
            let mut virtual_tail = old_total.checked_sub(1)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;

            for &remove_index in &sorted_indices {
                // 3.1 读取被删除节点的链表指针
                let (removed_prev, removed_next) = {
                    let removed_order = Self::get_order_safe(&data, remove_index)?;
                    (removed_order.prev_order, removed_order.next_order)
                };

                // 3.2 从链表中摘除该节点
                Self::unlink_node_internal(
                    &mut data,
                    remove_index,
                    removed_prev,
                    removed_next,
                    old_head,
                    old_tail,
                )?;

                // 3.3 移动末尾节点到被删除位置(如果不是删除末尾)
                if remove_index < virtual_tail {
                    Self::move_tail_to_index_internal(&mut data, virtual_tail, remove_index)?;
                }

                // 3.4 虚拟末尾前移
                virtual_tail = virtual_tail.checked_sub(1)
                    .ok_or(ErrorCode::OrderBookManagerOverflow)?;
            }

            // 3.5 更新 OrderBook 头部
            let orderbook_mut = Self::load_orderbook_header_mut(&mut data)?;
            orderbook_mut.total = old_total.checked_sub(delete_count)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;
            orderbook_mut.total_capacity = (old_total.checked_sub(delete_count)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?) as u32;
        }

        // 4. 一次性 resize + 返还租金
        Self::final_resize_and_refund(
            orderbook_account,
            payer,
            old_total,
            delete_count,
        )?;

        Ok(())
    }

    /// 内部辅助函数: 从链表中摘除节点
    fn unlink_node_internal(
        data: &mut [u8],
        _remove_index: u16,
        removed_prev: u16,
        removed_next: u16,
        _old_head: u16,
        _old_tail: u16,
    ) -> Result<()> {
        // 处理前驱节点
        if removed_prev != u16::MAX {
            let prev_order = Self::get_order_mut_safe(data, removed_prev)?;
            prev_order.next_order = removed_next;
            prev_order.version = prev_order.version.checked_add(1)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;
        } else {
            // 删除的是头节点，更新 head
            let orderbook_mut = Self::load_orderbook_header_mut(data)?;
            orderbook_mut.head = removed_next;
        }

        // 处理后继节点
        if removed_next != u16::MAX {
            let next_order = Self::get_order_mut_safe(data, removed_next)?;
            next_order.prev_order = removed_prev;
            next_order.version = next_order.version.checked_add(1)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;
        } else {
            // 删除的是尾节点，更新 tail
            let orderbook_mut = Self::load_orderbook_header_mut(data)?;
            orderbook_mut.tail = removed_prev;

            // ✅ 修复: 删除尾节点时,更新前驱节点的 next_order 为 u16::MAX
            // 批量删除场景: 删除尾节点后,前驱节点成为新的尾节点,其 next_order 应该为 u16::MAX
            if removed_prev != u16::MAX {
                let prev_order = Self::get_order_mut_safe(data, removed_prev)?;
                prev_order.next_order = u16::MAX;
                prev_order.version = prev_order.version.checked_add(1)
                    .ok_or(ErrorCode::OrderBookManagerOverflow)?;
            }
        }

        Ok(())
    }

    /// 内部辅助函数: 将末尾节点移动到指定索引
    fn move_tail_to_index_internal(
        data: &mut [u8],
        tail_index: u16,
        target_index: u16,
    ) -> Result<()> {
        // 读取末尾节点数据
        let tail_order = *Self::get_order_safe(data, tail_index)?;
        let tail_prev = tail_order.prev_order;
        let tail_next = tail_order.next_order;

        // 复制到目标位置
        let target_order = Self::get_order_mut_safe(data, target_index)?;
        *target_order = tail_order;
        target_order.version = target_order.version.checked_add(1)
            .ok_or(ErrorCode::OrderBookManagerOverflow)?;

        // 更新前驱节点的 next_order
        if tail_prev != u16::MAX {
            let prev_order = Self::get_order_mut_safe(data, tail_prev)?;
            prev_order.next_order = target_index;
            prev_order.version = prev_order.version.checked_add(1)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;
        } else {
            // 移动的是头节点
            let orderbook_mut = Self::load_orderbook_header_mut(data)?;
            orderbook_mut.head = target_index;
        }

        // 更新后继节点的 prev_order
        if tail_next != u16::MAX {
            let next_order = Self::get_order_mut_safe(data, tail_next)?;
            next_order.prev_order = target_index;
            next_order.version = next_order.version.checked_add(1)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;
        } else {
            // 移动的是尾节点
            let orderbook_mut = Self::load_orderbook_header_mut(data)?;
            orderbook_mut.tail = target_index;
        }

        Ok(())
    }

    /// 内部辅助函数: 批量删除全部订单
    fn batch_remove_all<'a>(
        orderbook_account: &AccountInfo<'a>,
        payer: &AccountInfo<'a>,
        old_total: u16,
    ) -> Result<()> {
        // 更新头部为空链表
        {
            let mut data = orderbook_account.data.borrow_mut();
            let orderbook_mut = Self::load_orderbook_header_mut(&mut data)?;
            orderbook_mut.head = u16::MAX;
            orderbook_mut.tail = u16::MAX;
            orderbook_mut.total = 0;
            orderbook_mut.total_capacity = 0;
        }

        // Resize 并返还租金
        Self::final_resize_and_refund(
            orderbook_account,
            payer,
            old_total,
            old_total,
        )?;

        Ok(())
    }

    /// 内部辅助函数: 最终 resize 并返还租金
    fn final_resize_and_refund<'a>(
        orderbook_account: &AccountInfo<'a>,
        payer: &AccountInfo<'a>,
        old_total: u16,
        delete_count: u16,
    ) -> Result<()> {
        // 安全检查
        require!(payer.is_writable, ErrorCode::OrderBookManagerAccountNotWritable);
        require!(orderbook_account.is_writable, ErrorCode::OrderBookManagerAccountNotWritable);
        require!(
            orderbook_account.owner == &crate::ID,
            ErrorCode::OrderBookManagerInvalidAccountOwner
        );

        let new_total = old_total.checked_sub(delete_count)
            .ok_or(ErrorCode::OrderBookManagerOverflow)?;
        let new_size = OrderBook::account_size(new_total as u32);
        let rent = Rent::get()?;
        let new_minimum_balance = rent.minimum_balance(new_size);
        let old_lamports = orderbook_account.lamports();

        // 如果有多余租金，返还给 payer
        if old_lamports > new_minimum_balance {
            let refund = old_lamports.checked_sub(new_minimum_balance)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;

            // 验证 payer 账户有足够空间接收退款
            require!(
                payer.lamports().checked_add(refund).is_some(),
                ErrorCode::OrderBookManagerOverflow
            );

            // 验证 orderbook 账户有足够余额
            require!(
                orderbook_account.lamports() >= refund,
                ErrorCode::OrderBookManagerInsufficientFunds
            );

            // 执行转账
            let new_orderbook_lamports = orderbook_account.lamports()
                .checked_sub(refund)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;
            let new_payer_lamports = payer.lamports()
                .checked_add(refund)
                .ok_or(ErrorCode::OrderBookManagerOverflow)?;

            **orderbook_account.lamports.borrow_mut() = new_orderbook_lamports;
            **payer.lamports.borrow_mut() = new_payer_lamports;

            // 验证退款后状态
            require!(
                orderbook_account.lamports() >= new_minimum_balance,
                ErrorCode::OrderBookManagerNotRentExempt
            );
            require!(
                orderbook_account.lamports() == new_minimum_balance,
                ErrorCode::OrderBookManagerInvalidRentBalance
            );
        }

        // Resize 缩小空间
        Self::resize_account(orderbook_account, new_size)?;

        Ok(())
    }

}


/// 遍历结果 
#[derive(Debug, Clone, Copy)]
pub struct TraversalResult {
    /// 本次处理的订单数量
    pub processed: u32,

    /// 下一个待处理的索引（u16::MAX 表示已完成）
    #[allow(dead_code)]
    pub next: u16,

    /// 是否已遍历完成
    #[allow(dead_code)]
    pub done: bool,
}

/// 订单更新数据（只包含可更新的字段）
/// 不可更新字段：user, order_id, start_time, order_type, next_order, prev_order (链表指针由系统管理)
#[derive(Clone, Copy, Default)]
pub struct MarginOrderUpdateData {
    pub lock_lp_start_price: Option<u128>,
    pub lock_lp_end_price: Option<u128>,
    pub lock_lp_sol_amount: Option<u64>,
    pub lock_lp_token_amount: Option<u64>,
    pub next_lp_sol_amount: Option<u64>,
    pub next_lp_token_amount: Option<u64>,
    pub end_time: Option<u32>,
    pub margin_init_sol_amount: Option<u64>,
    pub margin_sol_amount: Option<u64>,
    pub borrow_amount: Option<u64>,
    pub position_asset_amount: Option<u64>,
    pub borrow_fee: Option<u16>,
    pub open_price: Option<u128>,
    pub realized_sol_amount: Option<u64>,
}
