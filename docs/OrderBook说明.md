# OrderBook 说明文档

## 概述

本文档说明 SpinPet 协议的 OrderBook（订单簿）机制，以及后端服务器如何通过监听链上事件来维护本地订单簿数据，实现快速查询和交易。

## 核心架构

### 1. 链上订单簿结构

每个代币 mint 对应两个独立的订单簿：
- **up_orderbook**: 做空订单簿（Up方向，价格上涨时亏损）
- **down_orderbook**: 做多订单簿（Down方向，价格下跌时亏损）

这两个订单簿都是链上 PDA 账户，使用链表结构存储订单数据。

### 2. 数据结构

#### OrderBook Header（104 字节）
```rust
struct OrderBook {
    version: u8,              // 版本号
    order_type: u8,          // 1=做多, 2=做空
    authority: Pubkey,       // 管理员
    order_id_counter: u64,   // 订单ID计数器（自增）
    total_capacity: u32,     // 总容量
    head: u16,              // 链表头索引
    tail: u16,              // 链表尾索引
    total: u16,             // 当前订单总数
    // ... 其他字段
}
```

#### MarginOrder 订单数据（192 字节）
```rust
struct MarginOrder {
    user: Pubkey,                    // 开仓用户
    order_id: u64,                   // 唯一订单ID
    order_type: u8,                  // 1=做多, 2=做空

    // 价格信息
    open_price: u128,               // 开仓价格
    lock_lp_start_price: u128,      // 锁定区间起始价
    lock_lp_end_price: u128,        // 锁定区间结束价

    // 仓位信息
    margin_sol_amount: u64,         // 保证金数量
    borrow_amount: u64,             // 借款数量
    position_asset_amount: u64,     // 持仓数量

    // 时间信息
    start_time: u32,                // 开仓时间
    end_time: u32,                  // 到期时间

    // 链表指针
    next_order: u16,                // 下一个订单索引
    prev_order: u16,                // 上一个订单索引
}
```

## 事件系统

### 1. 关键事件类型

#### TokenCreatedEvent - 代币创建
```rust
TokenCreatedEvent {
    mint_account: Pubkey,       // 代币地址
    up_orderbook: Pubkey,      // 做空订单簿PDA
    down_orderbook: Pubkey,    // 做多订单簿PDA
    latest_price: u128,        // 初始价格
}
```

#### LongShortEvent - 开仓事件
```rust
LongShortEvent {
    mint_account: Pubkey,
    order_id: u64,             // 新订单ID
    order_type: u8,            // 1=做多, 2=做空
    user: Pubkey,
    lock_lp_start_price: u128,
    lock_lp_end_price: u128,
    margin_sol_amount: u64,
    borrow_amount: u64,
    position_asset_amount: u64,
    latest_price: u128,
    liquidate_indices: Vec<u16>, // 需要清算的订单索引
}
```

#### FullCloseEvent - 全部平仓
```rust
FullCloseEvent {
    mint_account: Pubkey,
    order_id: u64,             // 被平仓的订单ID
    is_close_long: bool,       // true=平多, false=平空
    latest_price: u128,
    liquidate_indices: Vec<u16>, // 包含被平仓订单自身的索引
}
```

#### PartialCloseEvent - 部分平仓
```rust
PartialCloseEvent {
    mint_account: Pubkey,
    order_id: u64,
    liquidate_id: u16,         // 订单在链表中的索引
    // ... 更新后的订单参数
    liquidate_indices: Vec<u16>,
}
```

#### BuySellEvent - 现货交易
```rust
BuySellEvent {
    mint_account: Pubkey,
    is_buy: bool,
    latest_price: u128,
    liquidate_indices: Vec<u16>, // 触发清算的订单索引
}
```

### 2. 重要字段说明

- **order_id**: 全局唯一的订单标识符，由链上自增生成
- **liquidate_indices**: 需要清算的订单索引数组（注意：是索引不是order_id）
- **latest_price**: 交易后的最新价格，用于判断是否触发清算

## 后端实现方案

### 1. 数据库设计（RocksDB）

建议使用以下 key 结构：

```
# OrderBook 元数据
orderbook:{mint}:up:meta    -> {head, tail, total, order_id_counter}
orderbook:{mint}:down:meta  -> {head, tail, total, order_id_counter}

# 订单数据（按索引存储）
orderbook:{mint}:up:order:{index}   -> MarginOrder
orderbook:{mint}:down:order:{index} -> MarginOrder

# 订单ID到索引的映射
orderbook:{mint}:up:id:{order_id}   -> index
orderbook:{mint}:down:id:{order_id} -> index

# 用户订单索引
orderbook:{mint}:user:{user}:orders -> [order_ids]
```

### 2. 事件处理流程

#### 监听链上事件
```javascript
// 伪代码
async function listenToEvents() {
    const connection = new Connection(RPC_URL);

    // 订阅程序日志
    connection.onLogs(PROGRAM_ID, (logs) => {
        const events = parseEvents(logs);
        for (const event of events) {
            await processEvent(event);
        }
    });
}
```

#### 处理不同事件

##### 1. TokenCreatedEvent 处理
```javascript
async function handleTokenCreated(event) {
    const { mint_account, up_orderbook, down_orderbook, latest_price } = event;

    // 初始化两个订单簿
    await db.put(`orderbook:${mint_account}:up:meta`, {
        head: 0xFFFF,  // u16::MAX 表示空链表
        tail: 0xFFFF,
        total: 0,
        order_id_counter: 1
    });

    await db.put(`orderbook:${mint_account}:down:meta`, {
        head: 0xFFFF,
        tail: 0xFFFF,
        total: 0,
        order_id_counter: 1
    });

    // 记录最新价格
    await db.put(`price:${mint_account}:latest`, latest_price);
}
```

##### 2. LongShortEvent 处理（开仓）
```javascript
async function handleLongShort(event) {
    const { mint_account, order_id, order_type, liquidate_indices } = event;

    // 先处理清算
    for (const index of liquidate_indices) {
        await removeOrderByIndex(mint_account, order_type, index);
    }

    // 确定订单簿类型
    const bookType = order_type === 1 ? 'down' : 'up';

    // 获取元数据
    const meta = await db.get(`orderbook:${mint_account}:${bookType}:meta`);

    // 新订单索引 = total（在清算后的值）
    const newIndex = meta.total;

    // 创建订单对象
    const order = {
        user: event.user,
        order_id: event.order_id,
        order_type: event.order_type,
        lock_lp_start_price: event.lock_lp_start_price,
        lock_lp_end_price: event.lock_lp_end_price,
        margin_sol_amount: event.margin_sol_amount,
        borrow_amount: event.borrow_amount,
        position_asset_amount: event.position_asset_amount,
        // ... 其他字段
        prev_order: meta.tail,  // 链接到当前尾部
        next_order: 0xFFFF,     // 作为新尾部
    };

    // 更新前一个订单的 next_order
    if (meta.tail !== 0xFFFF) {
        const prevOrder = await db.get(`orderbook:${mint_account}:${bookType}:order:${meta.tail}`);
        prevOrder.next_order = newIndex;
        await db.put(`orderbook:${mint_account}:${bookType}:order:${meta.tail}`, prevOrder);
    }

    // 存储新订单
    await db.put(`orderbook:${mint_account}:${bookType}:order:${newIndex}`, order);
    await db.put(`orderbook:${mint_account}:${bookType}:id:${order_id}`, newIndex);

    // 更新元数据
    meta.tail = newIndex;
    if (meta.head === 0xFFFF) {
        meta.head = newIndex;  // 第一个订单
    }
    meta.total++;
    meta.order_id_counter = order_id + 1;

    await db.put(`orderbook:${mint_account}:${bookType}:meta`, meta);
}
```

##### 3. FullCloseEvent 处理（全部平仓）
```javascript
async function handleFullClose(event) {
    const { mint_account, order_id, is_close_long, liquidate_indices } = event;

    // 确定订单簿类型
    const bookType = is_close_long ? 'down' : 'up';

    // liquidate_indices 包含被平仓的订单自身
    for (const index of liquidate_indices) {
        await removeOrderByIndex(mint_account, bookType, index);
    }
}
```

##### 4. 清算订单函数
```javascript
async function removeOrderByIndex(mint_account, bookType, index) {
    const meta = await db.get(`orderbook:${mint_account}:${bookType}:meta`);
    const order = await db.get(`orderbook:${mint_account}:${bookType}:order:${index}`);

    if (!order) return;

    // 更新链表指针
    if (order.prev_order !== 0xFFFF) {
        const prevOrder = await db.get(`orderbook:${mint_account}:${bookType}:order:${order.prev_order}`);
        prevOrder.next_order = order.next_order;
        await db.put(`orderbook:${mint_account}:${bookType}:order:${order.prev_order}`, prevOrder);
    } else {
        // 删除的是头节点
        meta.head = order.next_order;
    }

    if (order.next_order !== 0xFFFF) {
        const nextOrder = await db.get(`orderbook:${mint_account}:${bookType}:order:${order.next_order}`);
        nextOrder.prev_order = order.prev_order;
        await db.put(`orderbook:${mint_account}:${bookType}:order:${order.next_order}`, nextOrder);
    } else {
        // 删除的是尾节点
        meta.tail = order.prev_order;
    }

    // 如果不是最后一个订单，需要移动最后一个订单到被删除的位置
    const lastIndex = meta.total - 1;
    if (index < lastIndex) {
        const lastOrder = await db.get(`orderbook:${mint_account}:${bookType}:order:${lastIndex}`);

        // 更新链表指针
        if (lastOrder.prev_order !== 0xFFFF) {
            const prevOrder = await db.get(`orderbook:${mint_account}:${bookType}:order:${lastOrder.prev_order}`);
            prevOrder.next_order = index;
            await db.put(`orderbook:${mint_account}:${bookType}:order:${lastOrder.prev_order}`, prevOrder);
        }

        if (lastOrder.next_order !== 0xFFFF) {
            const nextOrder = await db.get(`orderbook:${mint_account}:${bookType}:order:${lastOrder.next_order}`);
            nextOrder.prev_order = index;
            await db.put(`orderbook:${mint_account}:${bookType}:order:${lastOrder.next_order}`, nextOrder);
        }

        // 移动到新位置
        await db.put(`orderbook:${mint_account}:${bookType}:order:${index}`, lastOrder);
        await db.put(`orderbook:${mint_account}:${bookType}:id:${lastOrder.order_id}`, index);

        // 更新 head/tail
        if (meta.head === lastIndex) meta.head = index;
        if (meta.tail === lastIndex) meta.tail = index;
    }

    // 删除原位置数据
    await db.delete(`orderbook:${mint_account}:${bookType}:order:${lastIndex}`);
    await db.delete(`orderbook:${mint_account}:${bookType}:id:${order.order_id}`);

    // 更新元数据
    meta.total--;
    await db.put(`orderbook:${mint_account}:${bookType}:meta`, meta);
}
```

### 3. 查询接口

#### 获取订单簿快照
```javascript
async function getOrderBook(mint_account, bookType, limit = 100) {
    const meta = await db.get(`orderbook:${mint_account}:${bookType}:meta`);
    const orders = [];

    let current = meta.head;
    let count = 0;

    while (current !== 0xFFFF && count < limit) {
        const order = await db.get(`orderbook:${mint_account}:${bookType}:order:${current}`);
        orders.push(order);
        current = order.next_order;
        count++;
    }

    return {
        total: meta.total,
        orders: orders
    };
}
```

#### 获取特定订单
```javascript
async function getOrderById(mint_account, order_id) {
    // 先查找 up_orderbook
    const upIndex = await db.get(`orderbook:${mint_account}:up:id:${order_id}`);
    if (upIndex !== null) {
        return await db.get(`orderbook:${mint_account}:up:order:${upIndex}`);
    }

    // 再查找 down_orderbook
    const downIndex = await db.get(`orderbook:${mint_account}:down:id:${order_id}`);
    if (downIndex !== null) {
        return await db.get(`orderbook:${mint_account}:down:order:${downIndex}`);
    }

    return null;
}
```

## 注意事项

1. **索引 vs ID**
   - `order_id`: 全局唯一标识符，永不重复
   - `index`: 订单在数组中的位置，会因为删除操作而变化
   - `liquidate_indices` 返回的是索引，不是 order_id

2. **链表维护**
   - 删除订单时需要正确更新前后节点的指针
   - 删除中间节点后，最后一个节点会移动到被删除的位置（压缩数组）

3. **事件顺序**
   - 必须按照链上事件的顺序处理，不能并发处理
   - 每个事件的 `liquidate_indices` 必须先处理，再处理新增操作

4. **价格更新**
   - 每个交易事件都会返回 `latest_price`
   - 需要实时更新并用于判断订单是否需要清算

5. **数据一致性**
   - 建议定期与链上数据对账
   - 可以通过读取链上 PDA 账户来验证本地数据的正确性

## 性能优化建议

1. **批量处理**
   - 将多个事件的数据库操作批量提交
   - 使用 RocksDB 的 WriteBatch 功能

2. **缓存策略**
   - 热点订单簿数据保持在内存中
   - 使用 LRU 缓存管理内存使用

3. **索引优化**
   - 为用户订单建立独立索引
   - 为价格区间建立索引，便于快速查找需要清算的订单

4. **并发控制**
   - 不同 mint 的订单簿可以并发处理
   - 同一个 mint 的事件必须串行处理