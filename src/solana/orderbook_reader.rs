// 链上 OrderBook 读取器 / On-chain OrderBook reader
use anyhow::{Result, Context};
use base64::{Engine, engine::general_purpose};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use solana_sdk::{pubkey::Pubkey, bs58};
use std::str::FromStr;
use tracing::{debug, error, info};

use crate::solana::client::SolanaClient;
use crate::orderbook::MarginOrder;

// OrderBook Header 结构 (与合约保持一致) / OrderBook Header structure (consistent with contract)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainOrderBookHeader {
    pub version: u8,
    pub order_type: u8,
    pub bump: u8,
    pub _padding1: [u8; 5],
    pub authority: Pubkey,
    pub order_id_counter: u64,
    pub created_at: u32,
    pub last_modified: u32,
    pub total_capacity: u32,
    pub head: u16,
    pub tail: u16,
    pub total: u16,
    pub _padding2: u16,
    pub reserved: [u8; 32],
}

impl ChainOrderBookHeader {
    pub const SIZE: usize = 104;
}

/// 链上 OrderBook 读取器 / On-chain OrderBook reader
pub struct OrderBookReader {
    client: SolanaClient,
    program_id: Pubkey,
}

impl OrderBookReader {
    /// 创建新的 OrderBook 读取器 / Create new OrderBook reader
    pub fn new(client: SolanaClient, program_id: String) -> Result<Self> {
        let program_id = Pubkey::from_str(&program_id)
            .context("无效的 program ID / Invalid program ID")?;

        Ok(Self {
            client,
            program_id,
        })
    }

    /// 计算 OrderBook PDA 地址 / Calculate OrderBook PDA address
    pub fn get_orderbook_pda(&self, mint: &str, direction: &str) -> Result<Pubkey> {
        let mint_pubkey = Pubkey::from_str(mint)
            .context("无效的 mint 地址 / Invalid mint address")?;

        let seed_prefix: &[u8] = if direction == "up" {
            b"up_orderbook"
        } else {
            b"down_orderbook"
        };

        let (pda, _bump) = Pubkey::find_program_address(
            &[seed_prefix, mint_pubkey.as_ref()],
            &self.program_id,
        );

        debug!(
            "计算 PDA 地址 / Calculated PDA address: mint={}, direction={}, pda={}",
            mint, direction, pda
        );

        Ok(pda)
    }

    /// 从链上获取 OrderBook 数据 / Get OrderBook data from chain
    pub async fn get_orderbook_from_chain(
        &self,
        mint: &str,
        direction: &str,
    ) -> Result<(ChainOrderBookHeader, Vec<(u16, MarginOrder)>)> {
        info!(
            "从链上查询 OrderBook / Querying OrderBook from chain: mint={}, direction={}",
            &mint[..8.min(mint.len())], direction
        );

        // 计算 PDA 地址 / Calculate PDA address
        let pda = self.get_orderbook_pda(mint, direction)?;

        // 获取账户数据 / Get account data
        let account_data = self.get_account_data(&pda).await?;

        // 解析数据 / Parse data
        self.parse_orderbook_data(&account_data)
    }

    /// 获取账户数据 / Get account data
    async fn get_account_data(&self, address: &Pubkey) -> Result<Vec<u8>> {
        debug!("获取账户数据 / Getting account data: {}", address);

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountInfo",
            "params": [
                address.to_string(),
                {
                    "encoding": "base64",
                    "commitment": "confirmed"
                }
            ]
        });

        let response = self.client.client
            .post(&self.client.rpc_url)
            .json(&request)
            .send()
            .await
            .context("发送 RPC 请求失败 / Failed to send RPC request")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "RPC 请求失败，状态码 / RPC request failed with status: {}",
                response.status()
            ));
        }

        let body: Value = response.json().await
            .context("解析响应失败 / Failed to parse response")?;

        if let Some(error) = body.get("error") {
            return Err(anyhow::anyhow!(
                "RPC 错误 / RPC error: {:?}",
                error
            ));
        }

        let result = body.get("result")
            .context("响应缺少 result 字段 / Response missing result field")?;

        if result.is_null() {
            return Err(anyhow::anyhow!(
                "账户不存在 / Account does not exist"
            ));
        }

        let value = result.get("value")
            .context("响应缺少 value 字段 / Response missing value field")?;

        if value.is_null() {
            return Err(anyhow::anyhow!(
                "账户数据为空 / Account data is null"
            ));
        }

        let data_array = value.get("data")
            .and_then(|d| d.as_array())
            .context("无效的数据格式 / Invalid data format")?;

        if data_array.is_empty() {
            return Err(anyhow::anyhow!(
                "账户数据数组为空 / Account data array is empty"
            ));
        }

        let base64_data = data_array[0].as_str()
            .context("数据不是字符串 / Data is not a string")?;

        let decoded = general_purpose::STANDARD.decode(base64_data)
            .context("Base64 解码失败 / Base64 decode failed")?;

        // Anchor 账户有 8 字节的鉴别器 / Anchor accounts have 8-byte discriminator
        if decoded.len() < 8 {
            return Err(anyhow::anyhow!(
                "账户数据太短 / Account data too short"
            ));
        }

        Ok(decoded)
    }

    /// 解析 OrderBook 数据 / Parse OrderBook data
    fn parse_orderbook_data(&self, data: &[u8]) -> Result<(ChainOrderBookHeader, Vec<(u16, MarginOrder)>)> {
        // 跳过 8 字节的 Anchor 鉴别器 / Skip 8-byte Anchor discriminator
        let data = &data[8..];

        if data.len() < ChainOrderBookHeader::SIZE {
            return Err(anyhow::anyhow!(
                "数据长度不足以包含 header / Data too short for header"
            ));
        }

        // 解析 Header / Parse header
        let header = self.parse_header(&data[..ChainOrderBookHeader::SIZE])?;

        // 解析订单数据 / Parse order data
        let orders = self.parse_orders(&data[ChainOrderBookHeader::SIZE..], &header)?;

        Ok((header, orders))
    }

    /// 解析 Header / Parse header
    fn parse_header(&self, data: &[u8]) -> Result<ChainOrderBookHeader> {
        if data.len() != ChainOrderBookHeader::SIZE {
            return Err(anyhow::anyhow!(
                "Header 数据长度错误 / Invalid header data length"
            ));
        }

        let mut cursor = 0;

        let version = data[cursor];
        cursor += 1;

        let order_type = data[cursor];
        cursor += 1;

        let bump = data[cursor];
        cursor += 1;

        let mut _padding1 = [0u8; 5];
        _padding1.copy_from_slice(&data[cursor..cursor + 5]);
        cursor += 5;

        let authority = Pubkey::new_from_array(
            data[cursor..cursor + 32].try_into()
                .context("解析 authority 失败 / Failed to parse authority")?
        );
        cursor += 32;

        let order_id_counter = u64::from_le_bytes(
            data[cursor..cursor + 8].try_into()
                .context("解析 order_id_counter 失败 / Failed to parse order_id_counter")?
        );
        cursor += 8;

        let created_at = u32::from_le_bytes(
            data[cursor..cursor + 4].try_into()
                .context("解析 created_at 失败 / Failed to parse created_at")?
        );
        cursor += 4;

        let last_modified = u32::from_le_bytes(
            data[cursor..cursor + 4].try_into()
                .context("解析 last_modified 失败 / Failed to parse last_modified")?
        );
        cursor += 4;

        let total_capacity = u32::from_le_bytes(
            data[cursor..cursor + 4].try_into()
                .context("解析 total_capacity 失败 / Failed to parse total_capacity")?
        );
        cursor += 4;

        let head = u16::from_le_bytes(
            data[cursor..cursor + 2].try_into()
                .context("解析 head 失败 / Failed to parse head")?
        );
        cursor += 2;

        let tail = u16::from_le_bytes(
            data[cursor..cursor + 2].try_into()
                .context("解析 tail 失败 / Failed to parse tail")?
        );
        cursor += 2;

        let total = u16::from_le_bytes(
            data[cursor..cursor + 2].try_into()
                .context("解析 total 失败 / Failed to parse total")?
        );
        cursor += 2;

        let _padding2 = u16::from_le_bytes(
            data[cursor..cursor + 2].try_into()
                .context("解析 _padding2 失败 / Failed to parse _padding2")?
        );
        cursor += 2;

        let mut reserved = [0u8; 32];
        reserved.copy_from_slice(&data[cursor..cursor + 32]);

        Ok(ChainOrderBookHeader {
            version,
            order_type,
            bump,
            _padding1,
            authority,
            order_id_counter,
            created_at,
            last_modified,
            total_capacity,
            head,
            tail,
            total,
            _padding2,
            reserved,
        })
    }

    /// 解析订单数据 / Parse order data
    fn parse_orders(&self, data: &[u8], header: &ChainOrderBookHeader) -> Result<Vec<(u16, MarginOrder)>> {
        const ORDER_SIZE: usize = 192;

        let mut orders = Vec::new();

        if header.total == 0 || header.head == u16::MAX {
            return Ok(orders);
        }

        // 计算可用的订单槽位数 / Calculate available order slots
        let available_slots = data.len() / ORDER_SIZE;

        if available_slots < header.total_capacity as usize {
            debug!(
                "警告：数据长度不足，期望 {} 个槽位，实际 {} 个 / Warning: Insufficient data, expected {} slots, got {}",
                header.total_capacity, available_slots, header.total_capacity, available_slots
            );
        }

        // 遍历链表结构 / Traverse linked list structure
        let mut current = header.head;
        let mut visited_count = 0;
        let max_iterations = header.total as usize * 2; // 防止无限循环 / Prevent infinite loop

        while current != u16::MAX && visited_count < max_iterations {
            if current as usize >= available_slots {
                error!(
                    "索引越界：current={}, available_slots={} / Index out of bounds: current={}, available_slots={}",
                    current, available_slots, current, available_slots
                );
                break;
            }

            let start = current as usize * ORDER_SIZE;
            let end = start + ORDER_SIZE;

            if end > data.len() {
                error!(
                    "数据不足：需要 {} 字节，实际 {} 字节 / Insufficient data: need {} bytes, got {} bytes",
                    end, data.len(), end, data.len()
                );
                break;
            }

            let order_data = &data[start..end];
            match self.parse_single_order(order_data) {
                Ok(order) => {
                    orders.push((current, order.clone()));
                    current = order.next_order;
                    visited_count += 1;
                }
                Err(e) => {
                    error!("解析订单失败 / Failed to parse order at index {}: {}", current, e);
                    break;
                }
            }

            if visited_count >= header.total as usize {
                break;
            }
        }

        info!(
            "从链上读取 {} 个订单 / Read {} orders from chain (header.total={})",
            orders.len(), orders.len(), header.total
        );

        Ok(orders)
    }

    /// 解析单个订单 / Parse single order
    fn parse_single_order(&self, data: &[u8]) -> Result<MarginOrder> {
        if data.len() != 192 {
            return Err(anyhow::anyhow!(
                "订单数据长度错误：期望 192 字节，实际 {} 字节 / Invalid order data length: expected 192 bytes, got {}",
                data.len(), data.len()
            ));
        }

        let mut cursor = 0;

        // user: Pubkey (32 bytes)
        let user = data[cursor..cursor + 32].try_into()
            .map(|bytes: [u8; 32]| bs58::encode(bytes).into_string())
            .context("解析 user 失败 / Failed to parse user")?;
        cursor += 32;

        // lock_lp_start_price: u128 (16 bytes)
        let lock_lp_start_price = u128::from_le_bytes(
            data[cursor..cursor + 16].try_into()
                .context("解析 lock_lp_start_price 失败 / Failed to parse lock_lp_start_price")?
        );
        cursor += 16;

        // lock_lp_end_price: u128 (16 bytes)
        let lock_lp_end_price = u128::from_le_bytes(
            data[cursor..cursor + 16].try_into()
                .context("解析 lock_lp_end_price 失败 / Failed to parse lock_lp_end_price")?
        );
        cursor += 16;

        // open_price: u128 (16 bytes)
        let open_price = u128::from_le_bytes(
            data[cursor..cursor + 16].try_into()
                .context("解析 open_price 失败 / Failed to parse open_price")?
        );
        cursor += 16;

        // order_id: u64 (8 bytes)
        let order_id = u64::from_le_bytes(
            data[cursor..cursor + 8].try_into()
                .context("解析 order_id 失败 / Failed to parse order_id")?
        );
        cursor += 8;

        // lock_lp_sol_amount: u64 (8 bytes)
        let lock_lp_sol_amount = u64::from_le_bytes(
            data[cursor..cursor + 8].try_into()
                .context("解析 lock_lp_sol_amount 失败 / Failed to parse lock_lp_sol_amount")?
        );
        cursor += 8;

        // lock_lp_token_amount: u64 (8 bytes)
        let lock_lp_token_amount = u64::from_le_bytes(
            data[cursor..cursor + 8].try_into()
                .context("解析 lock_lp_token_amount 失败 / Failed to parse lock_lp_token_amount")?
        );
        cursor += 8;

        // next_lp_sol_amount: u64 (8 bytes)
        let next_lp_sol_amount = u64::from_le_bytes(
            data[cursor..cursor + 8].try_into()
                .context("解析 next_lp_sol_amount 失败 / Failed to parse next_lp_sol_amount")?
        );
        cursor += 8;

        // next_lp_token_amount: u64 (8 bytes)
        let next_lp_token_amount = u64::from_le_bytes(
            data[cursor..cursor + 8].try_into()
                .context("解析 next_lp_token_amount 失败 / Failed to parse next_lp_token_amount")?
        );
        cursor += 8;

        // margin_init_sol_amount: u64 (8 bytes)
        let margin_init_sol_amount = u64::from_le_bytes(
            data[cursor..cursor + 8].try_into()
                .context("解析 margin_init_sol_amount 失败 / Failed to parse margin_init_sol_amount")?
        );
        cursor += 8;

        // margin_sol_amount: u64 (8 bytes)
        let margin_sol_amount = u64::from_le_bytes(
            data[cursor..cursor + 8].try_into()
                .context("解析 margin_sol_amount 失败 / Failed to parse margin_sol_amount")?
        );
        cursor += 8;

        // borrow_amount: u64 (8 bytes)
        let borrow_amount = u64::from_le_bytes(
            data[cursor..cursor + 8].try_into()
                .context("解析 borrow_amount 失败 / Failed to parse borrow_amount")?
        );
        cursor += 8;

        // position_asset_amount: u64 (8 bytes)
        let position_asset_amount = u64::from_le_bytes(
            data[cursor..cursor + 8].try_into()
                .context("解析 position_asset_amount 失败 / Failed to parse position_asset_amount")?
        );
        cursor += 8;

        // realized_sol_amount: u64 (8 bytes)
        let realized_sol_amount = u64::from_le_bytes(
            data[cursor..cursor + 8].try_into()
                .context("解析 realized_sol_amount 失败 / Failed to parse realized_sol_amount")?
        );
        cursor += 8;

        // version: u32 (4 bytes)
        let version = u32::from_le_bytes(
            data[cursor..cursor + 4].try_into()
                .context("解析 version 失败 / Failed to parse version")?
        );
        cursor += 4;

        // start_time: u32 (4 bytes)
        let start_time = u32::from_le_bytes(
            data[cursor..cursor + 4].try_into()
                .context("解析 start_time 失败 / Failed to parse start_time")?
        );
        cursor += 4;

        // end_time: u32 (4 bytes)
        let end_time = u32::from_le_bytes(
            data[cursor..cursor + 4].try_into()
                .context("解析 end_time 失败 / Failed to parse end_time")?
        );
        cursor += 4;

        // next_order: u16 (2 bytes)
        let next_order = u16::from_le_bytes(
            data[cursor..cursor + 2].try_into()
                .context("解析 next_order 失败 / Failed to parse next_order")?
        );
        cursor += 2;

        // prev_order: u16 (2 bytes)
        let prev_order = u16::from_le_bytes(
            data[cursor..cursor + 2].try_into()
                .context("解析 prev_order 失败 / Failed to parse prev_order")?
        );
        cursor += 2;

        // borrow_fee: u16 (2 bytes)
        let borrow_fee = u16::from_le_bytes(
            data[cursor..cursor + 2].try_into()
                .context("解析 borrow_fee 失败 / Failed to parse borrow_fee")?
        );
        cursor += 2;

        // order_type: u8 (1 byte)
        let order_type = data[cursor];

        Ok(MarginOrder {
            user,
            lock_lp_start_price,
            lock_lp_end_price,
            open_price,
            order_id,
            lock_lp_sol_amount,
            lock_lp_token_amount,
            next_lp_sol_amount,
            next_lp_token_amount,
            margin_init_sol_amount,
            margin_sol_amount,
            borrow_amount,
            position_asset_amount,
            realized_sol_amount,
            version,
            start_time,
            end_time,
            next_order,
            prev_order,
            borrow_fee,
            order_type,
        })
    }
}

