# Mint列表键值对说明 / Mint List Key-Value Storage Documentation

## 数据库位置 / Database Location

**事件数据库路径 / Event Database Path:**
- 默认路径 / Default path: `./data/event`
- 配置项 / Config key: `database.rocksdb_path`
- 存储内容 / Storage content: Token元数据、创建信息、Symbol索引、创建者索引等

**注意 / Note:** 本文档描述的所有Token/Mint键值结构均存储在事件数据库中，与统计数据库（`./data/stats`）和OrderBook数据库（`./data/orderbook`）分离。

---

## 一、键值存储结构 / Key-Value Storage Structure

### 1. 主存储 / Main Storage

#### 1.1 代币详情主键 / Token Detail Primary Key
```
键 Key:    token:{mint}
值 Value:  TokenDetail (JSON)
```

**说明 / Description:**
- 存储完整的代币详情信息，包括创建者、账户地址、手续费配置、元数据等
- Store complete token details including creator, account addresses, fee configuration, metadata, etc.

**示例 / Example:**
```
键: token:So11111111111111111111111111111111111111112
值: {
  "payer": "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU",
  "mint_account": "So11111111111111111111111111111111111111112",
  "curve_account": "...",
  "pool_token_account": "...",
  "pool_sol_account": "...",
  "name": "Wrapped SOL",
  "symbol": "SOL",
  "uri": "ipfs://...",
  "latest_price": "1000000000",
  "created_at": 1735660800,
  "created_slot": 123456789,
  ...
}
```

---

### 2. 索引存储 / Index Storage

#### 2.1 Symbol索引 / Symbol Index
```
键 Key:    token_symbol:{SYMBOL}:{mint}
值 Value:  "" (空值)
```

**说明 / Description:**
- 按代币符号（symbol）建立索引，支持重名代币
- Index by token symbol, supports duplicate symbol names
- Symbol统一大写存储
- Symbol stored in uppercase

**示例 / Example:**
```
键: token_symbol:PEPE:So11111111111111111111111111111111111111112
键: token_symbol:PEPE:8xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAbc
```

**用途 / Usage:**
- 按symbol查询代币列表
- Query tokens by symbol
- 支持同名代币并存
- Support multiple tokens with same symbol

---

#### 2.2 创建时间索引 / Creation Time Index
```
键 Key:    token_created:{timestamp:010}:{mint}
值 Value:  "" (空值)
```

**说明 / Description:**
- 按创建时间戳建立索引，用于时间范围查询和新币榜排序
- Index by creation timestamp for time-range queries and new token ranking
- 时间戳为10位Unix时间戳（秒级），左补零
- Timestamp is 10-digit Unix timestamp (seconds), zero-padded

**示例 / Example:**
```
键: token_created:1735660800:So11111111111111111111111111111111111111112
键: token_created:1735660801:8xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAbc
```

**用途 / Usage:**
- 获取最新创建的代币（新币榜）
- Get latest created tokens (new token list)
- 按时间倒序排列
- Sort by time in descending order

---

#### 2.3 Slot索引 / Slot Index
```
键 Key:    token_slot:{slot:010}:{mint}
值 Value:  "" (空值)
```

**说明 / Description:**
- 按区块链slot建立索引，用于按slot范围查询
- Index by blockchain slot for slot-range queries
- Slot为10位数字，左补零
- Slot is 10-digit number, zero-padded

**示例 / Example:**
```
键: token_slot:0123456789:So11111111111111111111111111111111111111112
键: token_slot:0123456790:8xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAbc
```

**用途 / Usage:**
- 按slot范围查询代币
- Query tokens by slot range
- 区块链数据同步和回溯
- Blockchain data sync and backtracking

---

#### 2.4 创建者索引 / Creator Index
```
键 Key:    token_payer:{payer}:{timestamp:010}:{mint}
值 Value:  "" (空值)
```

**说明 / Description:**
- 按创建者地址建立索引，用于查询某用户创建的所有代币
- Index by creator address to query all tokens created by a user
- 包含时间戳以支持按创建时间排序
- Include timestamp to support sorting by creation time

**示例 / Example:**
```
键: token_payer:7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU:1735660800:So11111111111111111111111111111111111111112
```

**用途 / Usage:**
- 查询用户创建的所有代币
- Query all tokens created by a user
- 创建者行为分析
- Creator behavior analysis

---

## 二、数据字段说明 / Data Fields Description

### TokenDetail 主要字段 / TokenDetail Main Fields

#### 基础信息 / Basic Information
| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `payer` | String | 创建者地址 / Creator address |
| `mint_account` | String | 代币mint地址（44字符） / Token mint address (44 chars) |
| `curve_account` | String | 曲线账户地址 / Curve account address |
| `pool_token_account` | String | 池子代币账户 / Pool token account |
| `pool_sol_account` | String | 池子SOL账户 / Pool SOL account |
| `fee_recipient` | String | 手续费接收地址 / Fee recipient address |
| `base_fee_recipient` | String | 基础手续费接收地址 / Base fee recipient address |
| `params_account` | String | 参数账户PDA地址 / Params account PDA address |
| `up_orderbook` | String | 做空订单簿地址 / Short orderbook address |
| `down_orderbook` | String | 做多订单簿地址 / Long orderbook address |

#### 手续费信息 / Fee Information
| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `swap_fee` | u16 | 现货交易手续费（基点） / Spot trading fee (basis points) |
| `borrow_fee` | u16 | 保证金交易手续费（基点） / Margin trading fee (basis points) |
| `fee_discount_flag` | u8 | 手续费折扣标志 / Fee discount flag |

#### 元数据 / Metadata
| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `name` | String | 代币名称 / Token name |
| `symbol` | String | 代币符号 / Token symbol |
| `uri` | String | 元数据URI (IPFS) / Metadata URI (IPFS) |
| `uri_data` | Object | IPFS解析后的扩展元数据 / IPFS parsed extended metadata |

#### 价格信息 / Price Information
| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `latest_price` | String | 最新价格（u128字符串） / Latest price (u128 as string) |

#### 时间信息 / Time Information
| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `created_at` | i64 | 创建时间Unix时间戳（秒） / Creation Unix timestamp (seconds) |
| `created_slot` | u64 | 创建时的区块slot / Creation block slot |
| `updated_at` | i64 | 最后更新时间 / Last update timestamp |

#### 扩展字段 / Extension Fields
| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `stats` | Object | 代币统计信息（预留） / Token statistics (reserved) |
| `extras` | Map | 自定义扩展字段 / Custom extension fields |

---

### TokenUriData 字段 / TokenUriData Fields

从IPFS URI解析获取的扩展元数据 / Extended metadata parsed from IPFS URI:

| 字段 Field | 类型 Type | 说明 Description |
|-----------|----------|------------------|
| `name` | String | 展示名称 / Display name |
| `symbol` | String | 展示符号 / Display symbol |
| `description` | String | 代币描述 / Token description |
| `image` | String | 代币图片URI / Token image URI |
| `show_name` | bool | 是否显示名称 / Show name flag |
| `created_on` | String | 创建日期 / Creation date |
| `twitter` | String | Twitter链接 / Twitter link |
| `website` | String | 官网链接 / Website link |
| `telegram` | String | Telegram链接 / Telegram link |

---

## 三、键格式规范 / Key Format Specification

### 格式约定 / Format Convention

| 占位符 Placeholder | 格式 Format | 说明 Description |
|-------------------|-------------|------------------|
| `{mint}` | 完整字符串 | 44字符Solana公钥地址 / 44-char Solana pubkey |
| `{symbol}` | 大写字符串 | 代币符号，统一大写 / Token symbol, uppercase |
| `{payer}` | 完整字符串 | 44字符创建者地址 / 44-char creator address |
| `{timestamp:010}` | 10位数字 | Unix时间戳，左补零 / Unix timestamp, zero-padded |
| `{slot:010}` | 10位数字 | 区块slot，左补零 / Block slot, zero-padded |

### 命名空间前缀 / Namespace Prefix

| 前缀 Prefix | 用途 Purpose |
|------------|--------------|
| `token:` | 代币主数据 / Token main data |
| `token_symbol:` | Symbol索引 / Symbol index |
| `token_created:` | 创建时间索引 / Creation time index |
| `token_slot:` | Slot索引 / Slot index |
| `token_payer:` | 创建者索引 / Creator index |

---

## 四、数据写入流程 / Data Write Flow

### 原子写入操作 / Atomic Write Operation

1. **创建WriteBatch** / Create WriteBatch
2. **写入主数据** / Write main data
   - `token:{mint}` → TokenDetail JSON
3. **创建索引** / Create indexes
   - `token_symbol:{SYMBOL}:{mint}` → ""
   - `token_created:{timestamp:010}:{mint}` → ""
   - `token_slot:{slot:010}:{mint}` → ""
   - `token_payer:{payer}:{timestamp:010}:{mint}` → ""
4. **原子提交** / Atomic commit

**特点 / Features:**
- 使用RocksDB WriteBatch确保原子性
- Use RocksDB WriteBatch to ensure atomicity
- 所有操作要么全部成功，要么全部失败
- All operations either all succeed or all fail

---

## 五、查询场景 / Query Scenarios

### 5.1 按mint查询单个代币 / Query Single Token by Mint
```
键: token:{mint}
```
直接查询，O(1)时间复杂度 / Direct query, O(1) time complexity

---

### 5.2 按symbol查询代币列表 / Query Token List by Symbol
```
前缀: token_symbol:{SYMBOL}:
迭代: 正向迭代 Forward iteration
限制: limit参数控制返回数量
```
支持分页游标 / Support pagination cursor

---

### 5.3 获取最新创建代币 / Get Latest Created Tokens
```
前缀: token_created:
迭代: 反向迭代 Reverse iteration (从大到小)
限制: limit参数控制返回数量
```
按创建时间倒序返回 / Return in descending order by creation time

---

### 5.4 按slot范围查询 / Query by Slot Range
```
起始键: token_slot:{start_slot:010}:
结束键: token_slot:{end_slot:010}:
迭代: 正向迭代 Forward iteration
```
用于区块链数据同步 / Used for blockchain data sync

---

### 5.5 查询用户创建的代币 / Query User's Created Tokens
```
前缀: token_payer:{payer}:
迭代: 正向或反向 Forward or reverse iteration
```
按创建时间排序 / Sort by creation time

---

## 六、数据一致性保证 / Data Consistency Guarantee

### 原子操作 / Atomic Operations
- 使用WriteBatch确保主数据和索引同时写入
- Use WriteBatch to ensure main data and indexes are written together
- 避免部分写入导致的数据不一致
- Avoid data inconsistency caused by partial writes

### 索引完整性 / Index Integrity
- 每个代币写入时创建所有必需索引
- Create all required indexes when writing each token
- 索引键包含mint地址，避免重复
- Index keys include mint address to avoid duplication

---

## 七、性能特点 / Performance Characteristics

### 存储效率 / Storage Efficiency
- 索引使用空值，节省存储空间
- Indexes use empty values to save storage space
- 主数据JSON序列化，平衡可读性和效率
- Main data JSON serialized, balancing readability and efficiency

### 查询性能 / Query Performance
- 直接查询: O(1) / Direct query: O(1)
- 前缀扫描: O(log N + M) / Prefix scan: O(log N + M)
  - N为总键数 / N is total key count
  - M为匹配结果数 / M is matched result count

### 索引维护 / Index Maintenance
- 写入时创建索引，无需后台维护
- Create indexes on write, no background maintenance needed
- 索引自动排序，支持高效范围查询
- Indexes auto-sorted, support efficient range queries

---

## 八、扩展性设计 / Scalability Design

### 预留扩展字段 / Reserved Extension Fields
- `stats`: 统计信息（市值、交易量等）
- `stats`: Statistics (market cap, volume, etc.)
- `extras`: 自定义扩展字段
- `extras`: Custom extension fields

### 索引可扩展性 / Index Extensibility
- 可按需添加新索引（如价格、流动性）
- Can add new indexes as needed (e.g., price, liquidity)
- 索引前缀命名规范，避免冲突
- Index prefix naming convention to avoid conflicts

---

## 九、注意事项 / Important Notes

### 数据类型转换 / Data Type Conversion
- 大数值（u128）存储为字符串，避免精度损失
- Large numbers (u128) stored as strings to avoid precision loss
- 时间戳使用i64 Unix时间戳（秒级）
- Timestamp uses i64 Unix timestamp (seconds)

### IPFS数据获取 / IPFS Data Fetching
- URI数据异步获取，可能为空
- URI data fetched asynchronously, may be empty
- 支持重试机制，提高成功率
- Retry mechanism supported to improve success rate
- 失败时不影响主数据写入
- Failure does not affect main data write

### 索引命名规范 / Index Naming Convention
- 统一使用下划线分隔符
- Uniformly use underscore separator
- 前缀标识索引类型和用途
- Prefix identifies index type and purpose
- 避免与其他模块冲突
- Avoid conflicts with other modules
