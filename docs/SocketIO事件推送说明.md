# Socket.IO 事件推送说明 / Socket.IO Event Push Documentation

## 概述 / Overview

本文档详细说明了 Pinpet Server 的 Socket.IO 实时推送服务，包括连接方式、事件类型、请求/响应格式等。

This document provides detailed information about the Pinpet Server Socket.IO real-time push service, including connection methods, event types, request/response formats, etc.

---

## 连接信息 / Connection Information

### 命名空间 / Namespace
- **K线和交易事件推送**: `/kline`
- **K-line and Trading Events Push**: `/kline`

### 连接 URL / Connection URL
```
ws://<server_host>:<port>/socket.io/?EIO=4&transport=websocket
```

示例 / Example:
```javascript
import { io } from 'socket.io-client';

const socket = io('http://localhost:3000/kline', {
  transports: ['websocket'],
  reconnection: true,
  reconnectionDelay: 1000,
  reconnectionAttempts: 5
});
```

---

## 客户端接收的事件 / Events Received by Client

### 1. `connection_success` - 连接成功 / Connection Success

**触发时机 / Trigger**: 客户端成功连接到服务器时

**字段说明 / Fields**:

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `client_id` | String | 客户端唯一ID / Client unique ID |
| `server_time` | i64 | 服务器时间戳(秒) / Server timestamp (seconds) |
| `supported_symbols` | Array | 支持的交易对列表 / Supported symbols list |
| `supported_intervals` | Array&lt;String&gt; | 支持的K线间隔 / Supported K-line intervals: `["s1", "s30", "m5"]` |

**示例 / Example**:
```json
{
  "client_id": "abc123xyz",
  "server_time": 1703001234,
  "supported_symbols": [],
  "supported_intervals": ["s1", "s30", "m5"]
}
```

---

### 2. `subscription_confirmed` - 订阅确认 / Subscription Confirmed

**触发时机 / Trigger**: 订阅成功后

**字段说明 / Fields**:

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `symbol` | String | Mint账户地址 / Mint account address |
| `interval` | String | K线间隔 / K-line interval: `s1`, `s30`, `m5` |
| `subscription_id` | String \| null | 客户端订阅ID(可选) / Client subscription ID (optional) |
| `success` | Boolean | 是否成功 / Success status |
| `message` | String | 确认消息 / Confirmation message |

**示例 / Example**:
```json
{
  "symbol": "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm",
  "interval": "s1",
  "subscription_id": "sub_001",
  "success": true,
  "message": "订阅成功 / Subscription successful"
}
```

---

### 3. `history_data` - 历史K线数据 / Historical K-line Data

**触发时机 / Trigger**: 订阅成功后立即推送，或手动请求历史数据时

**字段说明 / Fields**:

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `symbol` | String | Mint地址 / Mint address |
| `interval` | String | 时间间隔 / Time interval |
| `data` | Array&lt;KlineRealtimeData&gt; | K线数据列表(最多100条) / K-line data list (max 100) |
| `has_more` | Boolean | 是否有更多数据 / Has more data |
| `total_count` | usize | 总数量 / Total count |

**KlineRealtimeData 结构 / KlineRealtimeData Structure**:

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `time` | u64 | Unix时间戳(秒) / Unix timestamp (seconds) |
| `open` | f64 | 开盘价(USD) / Open price (USD) |
| `high` | f64 | 最高价(USD) / High price (USD) |
| `low` | f64 | 最低价(USD) / Low price (USD) |
| `close` | f64 | 收盘价(USD) / Close price (USD) |
| `volume` | f64 | 成交量 / Volume |
| `is_final` | Boolean | 是否为最终K线 / Is final K-line |
| `update_type` | String | 更新类型: `"realtime"` 或 `"final"` / Update type |
| `update_count` | u32 | 更新次数 / Update count |

**示例 / Example**:
```json
{
  "symbol": "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm",
  "interval": "s1",
  "data": [
    {
      "time": 1703001234,
      "open": 0.000123456,
      "high": 0.000125000,
      "low": 0.000122000,
      "close": 0.000124500,
      "volume": 1000.0,
      "is_final": false,
      "update_type": "realtime",
      "update_count": 5
    }
  ],
  "has_more": true,
  "total_count": 1000
}
```

---

### 4. `history_event_data` - 历史交易事件数据 / Historical Event Data

**触发时机 / Trigger**: 订阅成功后立即推送(最多300条)

**字段说明 / Fields**:

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `symbol` | String | Mint地址 / Mint address |
| `data` | Array&lt;EventUpdateMessage&gt; | 事件数据列表(最多300条) / Event data list (max 300) |
| `has_more` | Boolean | 是否有更多数据 / Has more data |
| `total_count` | usize | 总数量 / Total count |

**EventUpdateMessage 结构 / EventUpdateMessage Structure**:

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `symbol` | String | Mint地址 / Mint address |
| `event_type` | String | 事件类型名称 / Event type name |
| `event_data` | PinpetEvent | 完整事件数据(详见下方) / Complete event data (see below) |
| `timestamp` | u64 | 推送时间戳(毫秒) / Push timestamp (milliseconds) |

---

### 5. `kline_data` - 实时K线更新 / Real-time K-line Update

**触发时机 / Trigger**: 价格变动时实时推送

**字段说明 / Fields**:

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `symbol` | String | Mint地址 / Mint address |
| `interval` | String | 时间间隔 / Time interval |
| `subscription_id` | String \| null | 客户端订阅ID / Client subscription ID |
| `data` | KlineRealtimeData | K线数据 / K-line data |
| `timestamp` | u64 | 推送时间戳(毫秒) / Push timestamp (milliseconds) |

**示例 / Example**:
```json
{
  "symbol": "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm",
  "interval": "s1",
  "subscription_id": null,
  "data": {
    "time": 1703001234,
    "open": 0.000123456,
    "high": 0.000125000,
    "low": 0.000122000,
    "close": 0.000124500,
    "volume": 1000.0,
    "is_final": false,
    "update_type": "realtime",
    "update_count": 10
  },
  "timestamp": 1703001234567
}
```

---

### 6. `event_data` - 实时交易事件 / Real-time Trading Event

**触发时机 / Trigger**: 链上交易发生时实时推送

**字段说明 / Fields**:

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `symbol` | String | Mint地址 / Mint address |
| `event_type` | String | 事件类型 / Event type: `TokenCreated`, `BuySell`, `LongShort`, `FullClose`, `PartialClose`, `MilestoneDiscount`, `Liquidate` |
| `event_data` | PinpetEvent | 完整事件数据 / Complete event data |
| `timestamp` | u64 | 推送时间戳(毫秒) / Push timestamp (milliseconds) |

---

### 7. `unsubscribe_confirmed` - 取消订阅确认 / Unsubscribe Confirmed

**触发时机 / Trigger**: 取消订阅成功后

**字段说明 / Fields**:

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `symbol` | String | Mint地址 / Mint address |
| `interval` | String | 时间间隔 / Time interval |
| `subscription_id` | String \| null | 客户端订阅ID / Client subscription ID |
| `success` | Boolean | 是否成功 / Success status |
| `message` | String | 确认消息 / Confirmation message |

---

### 8. `error` - 错误消息 / Error Message

**触发时机 / Trigger**: 发生错误时

**字段说明 / Fields**:

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `code` | i32 | 错误码 / Error code |
| `message` | String | 错误信息 / Error message |

**错误码说明 / Error Codes**:

| 错误码 Code | 说明 Description |
|------------|------------------|
| 1001 | 无效的订阅请求(参数错误) / Invalid subscription request (parameter error) |
| 1002 | 订阅失败(超过最大订阅数) / Subscription failed (max subscriptions exceeded) |
| 1003 | 获取历史数据失败 / Failed to get historical data |

---

## 客户端发送的事件 / Events Sent by Client

### 1. `subscribe` - 订阅K线数据 / Subscribe to K-line Data

**请求参数 / Request Parameters**:

| 字段 Field | 类型 Type | 必填 Required | 说明 Description |
|-----------|----------|--------------|------------------|
| `symbol` | String | 是 Yes | Mint账户地址(32-44字符) / Mint account address (32-44 chars) |
| `interval` | String | 是 Yes | K线间隔: `s1`, `s30`, `m5` / K-line interval |
| `subscription_id` | String | 否 No | 客户端订阅ID(可选) / Client subscription ID (optional) |

**示例 / Example**:
```javascript
socket.emit('subscribe', {
  symbol: 'EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm',
  interval: 's1',
  subscription_id: 'sub_001'
});
```

**响应 / Response**:
- 成功时触发 `subscription_confirmed` 事件
- 失败时触发 `error` 事件
- 订阅成功后会立即收到 `history_data` (K线历史数据) 和 `history_event_data` (交易事件历史数据)

---

### 2. `unsubscribe` - 取消订阅 / Unsubscribe

**请求参数 / Request Parameters**:

| 字段 Field | 类型 Type | 必填 Required | 说明 Description |
|-----------|----------|--------------|------------------|
| `symbol` | String | 是 Yes | Mint地址 / Mint address |
| `interval` | String | 是 Yes | 时间间隔 / Time interval |
| `subscription_id` | String | 否 No | 客户端订阅ID / Client subscription ID |

**示例 / Example**:
```javascript
socket.emit('unsubscribe', {
  symbol: 'EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm',
  interval: 's1',
  subscription_id: 'sub_001'
});
```

**响应 / Response**:
- 成功时触发 `unsubscribe_confirmed` 事件

---

### 3. `history` - 请求历史K线数据 / Request Historical K-line Data

**请求参数 / Request Parameters**:

| 字段 Field | 类型 Type | 必填 Required | 说明 Description |
|-----------|----------|--------------|------------------|
| `symbol` | String | 是 Yes | Mint地址 / Mint address |
| `interval` | String | 是 Yes | 时间间隔 / Time interval |
| `limit` | usize | 否 No | 返回数量限制(默认100) / Return limit (default 100) |

**示例 / Example**:
```javascript
socket.emit('history', {
  symbol: 'EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm',
  interval: 's1',
  limit: 50
});
```

**响应 / Response**:
- 成功时触发 `history_data` 事件
- 失败时触发 `error` 事件

---

## 事件数据结构 / Event Data Structures

### PinpetEvent 类型 / PinpetEvent Types

#### 1. TokenCreated - 代币创建事件 / Token Creation Event

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `payer` | String | 支付者地址 / Payer address |
| `mint_account` | String | Mint账户地址 / Mint account address |
| `curve_account` | String | 曲线账户地址 / Curve account address |
| `pool_token_account` | String | 流动池Token账户 / Pool token account |
| `pool_sol_account` | String | 流动池SOL账户 / Pool SOL account |
| `fee_recipient` | String | 手续费接收地址 / Fee recipient address |
| `base_fee_recipient` | String | 基础手续费接收账户 / Base fee recipient account |
| `params_account` | String | 参数账户PDA地址 / Params account PDA |
| `swap_fee` | u16 | 现货交易手续费 / Spot trading fee |
| `borrow_fee` | u16 | 保证金交易手续费 / Margin trading fee |
| `fee_discount_flag` | u8 | 手续费折扣标志: 0=原价, 1=5折, 2=2.5折, 3=1.25折 / Fee discount flag |
| `name` | String | 代币名称 / Token name |
| `symbol` | String | 代币符号 / Token symbol |
| `uri` | String | 元数据URI / Metadata URI |
| `up_orderbook` | String | 做空订单账本PDA / Short orderbook PDA |
| `down_orderbook` | String | 做多订单账本PDA / Long orderbook PDA |
| `latest_price` | String | 最新价格(SOL) / Latest price (SOL) |
| `latest_price_usd` | String \| null | 最新价格(USD,整数字符串,精度10^23) / Latest price (USD, integer string, precision 10^23) |
| `timestamp` | String | ISO 8601时间戳 / ISO 8601 timestamp |
| `signature` | String | 交易签名 / Transaction signature |
| `slot` | u64 | 区块槽位 / Block slot |

---

#### 2. BuySell - 买卖交易事件 / Buy/Sell Event

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `payer` | String | 支付者地址 / Payer address |
| `mint_account` | String | Mint地址 / Mint address |
| `is_buy` | Boolean | 是否为买入 / Is buy (true) or sell (false) |
| `token_amount` | String | Token数量 / Token amount |
| `sol_amount` | String | SOL数量 / SOL amount |
| `latest_price` | String | 最新价格(SOL) / Latest price (SOL) |
| `latest_price_usd` | String \| null | 最新价格(USD,整数字符串,精度10^23) / Latest price (USD, integer string, precision 10^23) |
| `liquidate_indices` | Array&lt;u16&gt; | 清算订单索引列表 / Liquidation order indices |
| `timestamp` | String | ISO 8601时间戳 / ISO 8601 timestamp |
| `signature` | String | 交易签名 / Transaction signature |
| `slot` | u64 | 区块槽位 / Block slot |

---

#### 3. LongShort - 做多做空事件 / Long/Short Event

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `payer` | String | 开仓用户地址 / User address |
| `mint_account` | String | Mint地址 / Mint address |
| `order_id` | u64 | 订单唯一编号 / Unique order ID |
| `order_index` | u16 | 订单在账本中的索引 / Order index in orderbook |
| `latest_price` | String | 最新价格(SOL) / Latest price (SOL) |
| `latest_price_usd` | String \| null | 最新价格(USD,整数字符串,精度10^23) / Latest price (USD, integer string, precision 10^23) |
| `open_price` | String | 开仓价格(SOL) / Open price (SOL) |
| `order_type` | u8 | 订单类型: 1=做多, 2=做空 / Order type: 1=long, 2=short |
| `lock_lp_start_price` | String | 锁定LP区间开始价 / LP lock range start price |
| `lock_lp_end_price` | String | 锁定LP区间结束价 / LP lock range end price |
| `lock_lp_sol_amount` | String | 锁定LP的SOL数量 / Locked LP SOL amount |
| `lock_lp_token_amount` | String | 锁定LP的Token数量 / Locked LP token amount |
| `start_time` | i64 | 订单开始时间(秒) / Order start time (seconds) |
| `end_time` | i64 | 贷款到期时间(秒) / Loan expiry time (seconds) |
| `margin_sol_amount` | String | 保证金SOL数量 / Margin SOL amount |
| `borrow_amount` | String | 贷款数量 / Borrowed amount |
| `position_asset_amount` | String | 当前持仓数量 / Current position amount |
| `borrow_fee` | u16 | 保证金交易手续费 / Margin trading fee |
| `liquidate_indices` | Array&lt;u16&gt; | 清算订单索引列表 / Liquidation order indices |
| `timestamp` | String | ISO 8601时间戳 / ISO 8601 timestamp |
| `signature` | String | 交易签名 / Transaction signature |
| `slot` | u64 | 区块槽位 / Block slot |

---

#### 4. FullClose - 全平仓事件 / Full Close Event

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `payer` | String | 操作者地址 / Operator address |
| `user_sol_account` | String | 用户SOL账户 / User SOL account |
| `mint_account` | String | Mint地址 / Mint address |
| `is_close_long` | Boolean | 是否为平多 / Is closing long position |
| `final_token_amount` | String | 最终Token数量 / Final token amount |
| `final_sol_amount` | String | 最终SOL数量 / Final SOL amount |
| `user_close_profit` | String | 用户平仓利润(SOL) / User closing profit (SOL) |
| `latest_price` | String | 最新价格(SOL) / Latest price (SOL) |
| `latest_price_usd` | String \| null | 最新价格(USD,整数字符串,精度10^23) / Latest price (USD, integer string, precision 10^23) |
| `order_id` | u64 | 订单ID / Order ID |
| `order_index` | u16 | 订单索引 / Order index |
| `liquidate_indices` | Array&lt;u16&gt; | 清算订单索引列表 / Liquidation indices |
| `timestamp` | String | ISO 8601时间戳 / ISO 8601 timestamp |
| `signature` | String | 交易签名 / Transaction signature |
| `slot` | u64 | 区块槽位 / Block slot |

---

#### 5. PartialClose - 部分平仓事件 / Partial Close Event

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `payer` | String | 操作者地址 / Operator address |
| `user_sol_account` | String | 用户SOL账户 / User SOL account |
| `mint_account` | String | Mint地址 / Mint address |
| `is_close_long` | Boolean | 是否为平多 / Is closing long position |
| `final_token_amount` | String | 最终Token数量 / Final token amount |
| `final_sol_amount` | String | 最终SOL数量 / Final SOL amount |
| `user_close_profit` | String | 用户平仓利润(SOL) / User closing profit (SOL) |
| `latest_price` | String | 最新价格(SOL) / Latest price (SOL) |
| `latest_price_usd` | String \| null | 最新价格(USD,整数字符串,精度10^23) / Latest price (USD, integer string, precision 10^23) |
| `order_id` | u64 | 订单ID / Order ID |
| `order_index` | u16 | 订单索引 / Order index |
| `order_type` | u8 | 订单类型: 1=做多, 2=做空 / Order type: 1=long, 2=short |
| `user` | String | 开仓用户 / User who opened position |
| `lock_lp_start_price` | String | 锁定LP区间开始价 / LP lock range start price |
| `lock_lp_end_price` | String | 锁定LP区间结束价 / LP lock range end price |
| `lock_lp_sol_amount` | String | 锁定LP的SOL数量 / Locked LP SOL amount |
| `lock_lp_token_amount` | String | 锁定LP的Token数量 / Locked LP token amount |
| `start_time` | i64 | 订单开始时间(秒) / Order start time (seconds) |
| `end_time` | i64 | 贷款到期时间(秒) / Loan expiry time (seconds) |
| `margin_sol_amount` | String | 保证金SOL数量 / Margin SOL amount |
| `borrow_amount` | String | 贷款数量 / Borrowed amount |
| `position_asset_amount` | String | 当前持仓数量 / Current position amount |
| `borrow_fee` | u16 | 保证金交易手续费 / Margin trading fee |
| `realized_sol_amount` | String | 实现盈亏(SOL) / Realized P&L (SOL) |
| `liquidate_indices` | Array&lt;u16&gt; | 清算订单索引列表 / Liquidation indices |
| `timestamp` | String | ISO 8601时间戳 / ISO 8601 timestamp |
| `signature` | String | 交易签名 / Transaction signature |
| `slot` | u64 | 区块槽位 / Block slot |

---

#### 6. MilestoneDiscount - 里程碑折扣事件 / Milestone Discount Event

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `payer` | String | 支付者地址 / Payer address |
| `mint_account` | String | Mint地址 / Mint address |
| `curve_account` | String | 曲线账户地址 / Curve account address |
| `swap_fee` | u16 | 现货交易手续费 / Spot trading fee |
| `borrow_fee` | u16 | 保证金交易手续费 / Margin trading fee |
| `fee_discount_flag` | u8 | 手续费折扣标志 / Fee discount flag |
| `timestamp` | String | ISO 8601时间戳 / ISO 8601 timestamp |
| `signature` | String | 交易签名 / Transaction signature |
| `slot` | u64 | 区块槽位 / Block slot |

---

#### 7. Liquidate - 清算事件 / Liquidation Event

**说明 / Note**: 这是服务端合成的事件,不是链上事件 / This is a server-side synthetic event, not an on-chain event

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `payer` | String | 清算触发者 / Liquidation initiator |
| `user_sol_account` | String | 被清算用户SOL账户 / Liquidated user's SOL account |
| `mint_account` | String | Mint地址 / Mint address |
| `is_close_long` | Boolean | 是否为平多 / Is closing long (true) or short (false) |
| `final_token_amount` | String | 最终Token数量 / Final token amount |
| `final_sol_amount` | String | 最终SOL数量 / Final SOL amount |
| `order_index` | u16 | 订单索引 / Order index |
| `timestamp` | String | ISO 8601时间戳 / ISO 8601 timestamp |
| `signature` | String | 触发清算的交易签名 / Transaction signature that triggered liquidation |
| `slot` | u64 | 区块槽位 / Block slot |

---

## 使用示例 / Usage Examples

### JavaScript/TypeScript 完整示例 / Full Example

```javascript
import { io } from 'socket.io-client';

// 连接到K线服务 / Connect to K-line service
const socket = io('http://localhost:3000/kline', {
  transports: ['websocket'],
  reconnection: true,
  reconnectionDelay: 1000,
  reconnectionAttempts: 5
});

// 连接成功 / Connection success
socket.on('connection_success', (data) => {
  console.log('Connected:', data);
  console.log('Client ID:', data.client_id);
  console.log('Supported intervals:', data.supported_intervals);

  // 订阅K线数据 / Subscribe to K-line data
  socket.emit('subscribe', {
    symbol: 'EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm',
    interval: 's1',
    subscription_id: 'my_subscription_001'
  });
});

// 订阅确认 / Subscription confirmed
socket.on('subscription_confirmed', (data) => {
  console.log('Subscription confirmed:', data);
});

// 接收历史K线数据 / Receive historical K-line data
socket.on('history_data', (data) => {
  console.log('Historical K-line data:', data);
  console.log('Total count:', data.total_count);
  console.log('K-lines:', data.data.length);
});

// 接收历史交易事件 / Receive historical event data
socket.on('history_event_data', (data) => {
  console.log('Historical event data:', data);
  console.log('Total events:', data.total_count);
  data.data.forEach((event) => {
    console.log(`Event: ${event.event_type}`, event.event_data);
  });
});

// 实时K线更新 / Real-time K-line update
socket.on('kline_data', (update) => {
  console.log('K-line update:', update);
  console.log('Price:', update.data.close);
  console.log('Is final:', update.data.is_final);
});

// 实时交易事件 / Real-time trading event
socket.on('event_data', (event) => {
  console.log('New event:', event.event_type);

  switch(event.event_type) {
    case 'TokenCreated':
      console.log('New token created:', event.event_data);
      break;
    case 'BuySell':
      console.log('Buy/Sell event:', event.event_data);
      break;
    case 'LongShort':
      console.log('Long/Short event:', event.event_data);
      break;
    case 'Liquidate':
      console.log('Liquidation event:', event.event_data);
      break;
  }
});

// 错误处理 / Error handling
socket.on('error', (error) => {
  console.error('Error:', error);
  console.error('Error code:', error.code);
  console.error('Error message:', error.message);
});

// 连接错误 / Connection error
socket.on('connect_error', (error) => {
  console.error('Connection error:', error);
});

// 断开连接 / Disconnected
socket.on('disconnect', (reason) => {
  console.log('Disconnected:', reason);
});

// 取消订阅示例 / Unsubscribe example
function unsubscribe() {
  socket.emit('unsubscribe', {
    symbol: 'EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm',
    interval: 's1',
    subscription_id: 'my_subscription_001'
  });
}

// 请求历史数据 / Request historical data
function requestHistory() {
  socket.emit('history', {
    symbol: 'EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm',
    interval: 's1',
    limit: 50
  });
}
```

---

## 配置说明 / Configuration

### 服务端配置 / Server Configuration

| 配置项 Config | 默认值 Default | 说明 Description |
|--------------|----------------|------------------|
| `max_subscriptions_per_client` | 100 | 每个客户端最大订阅数 / Max subscriptions per client |
| `ping_interval_secs` | 25 | 心跳间隔(秒) / Ping interval (seconds) |
| `ping_timeout_secs` | 60 | 心跳超时(秒) / Ping timeout (seconds) |
| `max_payload` | 1MB | 最大负载大小 / Max payload size |

### 支持的时间间隔 / Supported Intervals

| 间隔 Interval | 说明 Description | 对齐方式 Alignment |
|--------------|------------------|--------------------|
| `s1` | 1秒 / 1 second | 无对齐 / No alignment |
| `s30` | 30秒 / 30 seconds | 30秒边界对齐 / Aligned to 30-second boundary |
| `m5` | 5分钟 / 5 minutes | 5分钟边界对齐 / Aligned to 5-minute boundary |

---

## 房间机制 / Room Mechanism

服务器使用房间(Room)机制来管理订阅:
The server uses a room mechanism to manage subscriptions:

- 房间命名格式 / Room naming format: `kline:{symbol}:{interval}`
- 示例 / Example: `kline:EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm:s1`
- 订阅时会自动加入对应房间 / Automatically joins corresponding room on subscribe
- 取消订阅时会自动离开房间 / Automatically leaves room on unsubscribe
- 断开连接时会自动清理所有订阅 / Automatically cleans up all subscriptions on disconnect

---

## 注意事项 / Notes

1. **价格单位 / Price Units**:
   - `latest_price`: 原始价格,单位为 SOL (u128) / Raw price in SOL (u128)
   - `latest_price_usd`: USD 价格(整数字符串,精度 10^23),由服务端根据 SOL/USD 汇率计算 / USD price (integer string, precision 10^23), calculated by server based on SOL/USD rate
   - K线中的价格均为 USD / K-line prices are in USD

2. **订阅限制 / Subscription Limits**:
   - 每个客户端最多订阅 100 个交易对 / Max 100 subscriptions per client
   - 超过限制会返回错误码 1002 / Returns error code 1002 if limit exceeded

3. **历史数据 / Historical Data**:
   - K线历史数据默认返回 100 条 / K-line history returns 100 records by default
   - 交易事件历史数据返回 300 条 / Event history returns 300 records
   - 可通过 `history` 事件请求更多数据 / Can request more data via `history` event

4. **实时更新频率 / Real-time Update Frequency**:
   - K线数据: 价格变动时实时推送 / K-line: pushed on price changes
   - 交易事件: 链上交易发生时实时推送 / Events: pushed when on-chain transactions occur

5. **断线重连 / Reconnection**:
   - 建议启用自动重连 / Auto-reconnection is recommended
   - 重连后需要重新订阅 / Need to re-subscribe after reconnection

---

## 测试工具 / Testing Tools

项目提供了测试脚本:
The project provides test scripts:

- **K线测试** / **K-line test**: `test/test_auto_kline.js`
- **事件测试** / **Event test**: `test/test_auto_event.js`

运行测试 / Run tests:
```bash
node test/test_auto_kline.js
node test/test_auto_event.js
```

---

## 相关文件 / Related Files

- 服务实现 / Service implementation: [src/kline/socket_service.rs](../src/kline/socket_service.rs)
- 事件处理 / Event handler: [src/kline/event_handler.rs](../src/kline/event_handler.rs)
- 类型定义 / Type definitions: [src/kline/types.rs](../src/kline/types.rs)
- 事件定义 / Event definitions: [src/solana/events.rs](../src/solana/events.rs)
