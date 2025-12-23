# OrderBook 键值对说明

## 数据库位置 / Database Location

**OrderBook数据库路径 / OrderBook Database Path:**
- 默认路径 / Default path: `./data/orderbook`
- 配置项 / Config key: `database.orderbook_db_path`
- 存储内容 / Storage content: 保证金订单数据、订单簿快照、订单ID映射等

**注意 / Note:** 本文档描述的所有OrderBook键值结构均存储在专用的OrderBook数据库中，与事件数据库（`./data/event`）和统计数据库（`./data/stats`）分离。

**性能优化配置 / Performance Configuration:**
- 写缓冲区大小 / Write buffer size: 256 MB (可配置 / configurable)
- 最大写缓冲区数量 / Max write buffers: 4 (可配置 / configurable)
- 后台任务数 / Background jobs: 8 (可配置 / configurable)

---

## 概述
OrderBook 使用 RocksDB 存储，采用多种键值对类型来管理保证金订单数据。所有键值对设计支持高效的查询、更新和删除操作。

## 键值对类型

### 1. OrderBook Header（账本头部）
**键格式：** `orderbook_header:{mint}:{direction}`

**值类型：** `OrderBookHeader` 结构体（JSON 序列化）

**功能：** 存储 OrderBook 的元数据，包括版本、总数、头尾索引、订单 ID 计数器等。

**示例：**
- `orderbook_header:So11111111111111111111111111111111111111112:up`
- `orderbook_header:EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v:dn`

---

### 2. Order Slot（订单槽位）
**键格式：** `orderbook_slot:{mint}:{direction}:{index:05}`

**值类型：** `MarginOrder` 结构体（JSON 序列化）

**功能：** 存储具体的订单数据，包括用户、价格、保证金、仓位等完整信息。索引为 5 位零填充数字。

**示例：**
- `orderbook_slot:So11111111111111111111111111111111111111112:up:00000`
- `orderbook_slot:EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v:dn:00123`

---

### 3. Order ID Mapping（订单 ID 映射）
**键格式：** `orderbook_id_map:{mint}:{direction}:{order_id:010}`

**值类型：** `u16` 索引（JSON 序列化）

**功能：** 通过唯一的 order_id 快速定位订单在链表中的槽位索引。order_id 为 10 位零填充数字。

**示例：**
- `orderbook_id_map:So11111111111111111111111111111111111111112:up:0000000001` → `0`
- `orderbook_id_map:EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v:dn:0000001234` → `123`

---

### 4. Active Indices List（活跃索引列表）
**键格式：** `orderbook_active_indices:{mint}:{direction}`

**值类型：** `Vec<u16>` 数组（JSON 序列化）

**功能：** 维护当前所有活跃订单的索引列表，用于快速获取所有活跃订单，避免遍历整个槽位空间。

**示例：**
- `orderbook_active_indices:So11111111111111111111111111111111111111112:up` → `[0, 1, 2, 3, 4]`

---

### 5. User Active Order Index（用户活跃订单索引）
**键格式：** `orderbook_user:{user}:{mint}:{direction}:{start_time:010}:{order_id:020}`

**值类型：** 空字节（仅作索引用）

**功能：** 按用户维度索引活跃订单，支持前缀扫描查询用户的所有活跃订单，支持按 mint、direction 过滤。

**特点：**
- `start_time` 为 10 位零填充，确保时间顺序
- `order_id` 为 20 位零填充，确保唯一性
- 值为空，仅键存在即表示订单活跃

**示例：**
- `orderbook_user:7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU:So11111111111111111111111111111111111111112:up:0001733232000:00000000000000000001`

---

### 6. User Closed Order（用户已关闭订单）
**键格式：** `orderbook_user_closed:{user}:{close_timestamp:010}:{mint}:{direction}:{order_id:020}`

**值类型：** `ClosedOrderRecord` 结构体（JSON 序列化）

**功能：** 存储用户的历史已关闭订单，包括订单完整快照和关闭信息（关闭时间、价格、原因）。

**特点：**
- 按关闭时间倒序排列（通过 `close_timestamp` 实现）
- 支持前缀扫描查询用户所有历史订单
- 可按 mint、direction、时间范围过滤

**关闭原因：**
- `1`: 用户主动平仓
- `2`: 强制清算
- `3`: 到期自动平仓
- `4`: 爆仓清算

**示例：**
- `orderbook_user_closed:7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU:0001733232000:So11111111111111111111111111111111111111112:up:00000000000000000001`

---

## 查询模式

### 通过 Order ID 查询订单
1. 通过 `orderbook_id_map` 获取 index
2. 通过 `orderbook_slot` 获取订单数据

### 查询用户活跃订单
1. 前缀扫描 `orderbook_user:{user}:` 获取所有键
2. 解析键得到 order_id
3. 通过 order_id 查询完整订单数据

### 查询用户历史订单
1. 前缀扫描 `orderbook_user_closed:{user}:` 获取所有记录
2. 可选按 mint、direction、时间范围过滤
3. 直接返回完整的 ClosedOrderRecord

### 获取所有活跃订单
1. 读取 `orderbook_active_indices` 获取活跃索引列表
2. 批量读取对应的 `orderbook_slot` 数据

---

## 原子性保证

所有涉及多个键值对的操作（插入、删除、更新）均使用 `WriteBatch` 实现原子性提交，确保数据一致性。

---

## 性能优化

- **前缀扫描优化**：用户索引键设计支持高效的前缀扫描
- **时间排序**：通过零填充的时间戳实现自然排序
- **活跃索引缓存**：`active_indices` 避免全表扫描
- **快照隔离**：查询使用 RocksDB Snapshot 保证一致性视图
