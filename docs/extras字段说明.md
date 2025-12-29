# extras 字段说明文档 / Extras Field Documentation

## 概述 / Overview

`extras` 是 `TokenDetail` 数据结构中的扩展字段,用于存储 Token 的24小时统计数据。该字段为动态的键值对结构(`HashMap<String, Value>`),包含交易量、价格变化、市场活跃度等实时统计信息。

`extras` is an extension field in the `TokenDetail` data structure, used to store 24-hour statistics for tokens. This field is a dynamic key-value structure (`HashMap<String, Value>`) containing real-time statistics like volume, price changes, and market activity.

---

## 数据来源 / Data Source

`extras` 字段的数据来自三个独立的统计模块:

The data in `extras` field comes from three independent statistics modules:

- **Volume 交易量模块** (`VolumeStorage`) - 统计24小时交易量 / 24h trading volume statistics
- **Change 涨跌幅模块** (`ChangeStorage`) - 统计24小时价格变化 / 24h price change statistics
- **MarketsAbs 市场活跃度模块** (`MarketsAbsStorage`) - 统计24小时绝对钱包数 / 24h absolute markets statistics

数据通过 `enrich_token_with_stats()` 函数异步附加到 Token 详情中。

Data is asynchronously enriched to token details via the `enrich_token_with_stats()` function.

**代码位置** / Code Location: [src/router/token.rs:950-1005](src/router/token.rs#L950-L1005)

---

## 字段详情 / Field Details

### 1. 交易量相关字段 / Volume Related Fields

#### 1.1 `volume_24h`
- **类型** / Type: `String` (数值字符串 / numeric string)
- **含义** / Meaning: 24小时内的总交易量(SOL计价) / Total trading volume in 24 hours (denominated in SOL)
- **数据源** / Data Source: `VolumeStorage::get_token_volume()`
- **示例值** / Example: `"767.4"` (表示767.4 SOL的交易量)
- **说明** / Description:
  - 该值是累计的买入和卖出交易量总和
  - This value is the cumulative sum of buy and sell volumes
  - 使用字符串格式避免精度损失
  - String format used to avoid precision loss

#### 1.2 `volume_event_count`
- **类型** / Type: `Number` (整数 / integer)
- **含义** / Meaning: 24小时内的交易事件总次数 / Total number of trading events in 24 hours
- **数据源** / Data Source: `VolumeData.event_count`
- **示例值** / Example: `4` (表示发生了4次交易)
- **说明** / Description:
  - 统计24小时内所有买卖交易的事件数量
  - Counts all buy/sell trading events within 24 hours
  - 可用于判断交易活跃程度
  - Can be used to determine trading activity level

#### 1.3 `volume_last_update`
- **类型** / Type: `Number` (Unix时间戳,秒 / Unix timestamp, seconds)
- **含义** / Meaning: 交易量数据的最后更新时间 / Last update time of volume data
- **数据源** / Data Source: `VolumeData.last_update`
- **示例值** / Example: `1766998899` (对应 2025-12-29 某时刻)
- **说明** / Description:
  - 记录最后一次交易事件的发生时间
  - Records the time of the last trading event
  - 用于判断数据新鲜度
  - Used to determine data freshness

---

### 2. 价格变化相关字段 / Price Change Related Fields

#### 2.1 `change_percent_24h`
- **类型** / Type: `String` (百分比字符串 / percentage string)
- **含义** / Meaning: 24小时价格涨跌幅百分比 / 24-hour price change percentage
- **数据源** / Data Source: `ChangeStorage::get_token_change()`
- **示例值** / Example: `"-0.10940064077517694"` (表示下跌0.109%)
- **说明** / Description:
  - 计算公式: `((收盘价 - 开盘价) / 开盘价) × 100`
  - Calculation: `((close_price - open_price) / open_price) × 100`
  - 正数表示上涨,负数表示下跌
  - Positive = gain, negative = loss
  - 高精度浮点数字符串
  - High-precision floating-point string

#### 2.2 `change_open_price`
- **类型** / Type: `String` (价格字符串 / price string)
- **含义** / Meaning: 24小时统计周期的开盘价 / Opening price of 24-hour period
- **数据源** / Data Source: `ChangeData.open_price`
- **示例值** / Example: `"3839.1"` (表示开盘价3839.1 SOL)
- **说明** / Description:
  - 24小时前的价格快照
  - Price snapshot from 24 hours ago
  - 用于计算涨跌幅
  - Used to calculate price change percentage

#### 2.3 `change_close_price`
- **类型** / Type: `String` (价格字符串 / price string)
- **含义** / Meaning: 24小时统计周期的收盘价(最新价格) / Closing price of 24-hour period (latest price)
- **数据源** / Data Source: `ChangeData.close_price`
- **示例值** / Example: `"3834.9"` (表示最新价格3834.9 SOL)
- **说明** / Description:
  - 当前最新的交易价格
  - Current latest trading price
  - 配合开盘价计算涨跌幅
  - Used with open_price to calculate change percentage

---

### 3. 市场活跃度相关字段 / Market Activity Related Fields

#### 3.1 `markets_abs_cumulative`
- **类型** / Type: `Number` (整数 / integer)
- **含义** / Meaning: 24小时内参与交易的绝对钱包数(去重) / Absolute number of unique wallets trading in 24 hours
- **数据源** / Data Source: `MarketsAbsStorage::get_token_markets_abs()`
- **示例值** / Example: `1` (表示有1个独立钱包参与了交易)
- **说明** / Description:
  - 统计24小时内所有参与买卖的唯一钱包地址数量
  - Counts unique wallet addresses participating in buy/sell within 24 hours
  - 去重统计,同一钱包只计算一次
  - Deduplicated count, each wallet counted once
  - 用于衡量市场参与度和热度
  - Used to measure market participation and popularity
  - **"Hottest" 排序依据** / Hottest sort criterion

#### 3.2 `markets_abs_first_seen`
- **类型** / Type: `Number` (Unix时间戳,秒 / Unix timestamp, seconds)
- **含义** / Meaning: 第一个钱包参与交易的时间 / Time when the first wallet participated in trading
- **数据源** / Data Source: `MarketsAbsData.first_seen`
- **示例值** / Example: `1766998590` (对应 2025-12-29 某时刻)
- **说明** / Description:
  - 记录该Token在24小时统计窗口内首次被交易的时间
  - Records the first trading time within the 24-hour statistics window
  - 用于判断Token的活跃起始时间
  - Used to determine when the token became active

---

## 数据附加机制 / Data Enrichment Mechanism

### 何时附加 extras 数据? / When is extras data enriched?

根据不同的API端点有不同的行为:

Different API endpoints have different behaviors:

1. **单个Token查询** (`/api/tokens/mint/{mint}`)
   - 默认: **不附加** extras 数据 / Default: **NO** extras data
   - 条件附加: 当 `include_stats=true` 时附加 / Conditional: when `include_stats=true`
   - **代码位置** / Code: [src/router/token.rs:189-191](src/router/token.rs#L189-L191)

2. **Token列表查询** (`/api/tokens/list`)
   - **所有排序模式都附加** extras 数据 / **ALL** sort modes enrich extras data
   - 包括: `all`, `latest`, `liquid`, `rising`, `hottest`
   - **代码位置** / Code:
     - Latest/All: [src/router/token.rs:737-739](src/router/token.rs#L737-L739)
     - Liquid: [src/router/token.rs:809-811](src/router/token.rs#L809-L811)
     - Rising: [src/router/token.rs:872-874](src/router/token.rs#L872-L874)
     - Hottest: [src/router/token.rs:935-937](src/router/token.rs#L935-L937)

3. **其他查询** (symbol/latest/slot-range/search)
   - **不附加** extras 数据 / **NO** extras data

### 并发查询机制 / Concurrent Query Mechanism

为了优化性能,`extras` 数据通过 Tokio 并发查询:

For performance optimization, `extras` data is queried concurrently via Tokio:

```rust
let (volume_result, change_result, markets_abs_result) = tokio::join!(
    query_volume_stats(state, mint, period),
    query_change_stats(state, mint, period),
    query_markets_abs_stats(state, mint, period)
);
```

**代码位置** / Code: [src/router/token.rs:956-960](src/router/token.rs#L956-L960)

---

## 排序功能与 extras 字段的关系 / Sorting and extras Fields

### 排序类型 / Sort Types

`/api/tokens/list` 接口支持以下排序方式:

The `/api/tokens/list` endpoint supports the following sort methods:

1. **latest/all** - 按创建时间降序 / By creation time DESC
   - 依赖: 无 / Dependency: None
   - 数据源: Token的 `created_at` 字段 / Token's `created_at` field

2. **liquid** - 按24小时交易量降序 / By 24h volume DESC
   - 依赖: `volume_24h` (来自 extras) / Depends on: `volume_24h` (from extras)
   - 数据源: `VolumeStorage::get_top_volume()` / Data source: `VolumeStorage::get_top_volume()`

3. **rising** - 按24小时涨幅降序 / By 24h gain DESC
   - 依赖: `change_percent_24h` (来自 extras) / Depends on: `change_percent_24h` (from extras)
   - 数据源: `ChangeStorage::get_top_change()` / Data source: `ChangeStorage::get_top_change()`

4. **hottest** - 按24小时绝对钱包数降序 / By 24h absolute markets DESC
   - 依赖: `markets_abs_cumulative` (来自 extras) / Depends on: `markets_abs_cumulative` (from extras)
   - 数据源: `MarketsAbsStorage::get_top_markets_abs()` / Data source: `MarketsAbsStorage::get_top_markets_abs()`

---

## 示例数据 / Sample Data

```json
{
  "extras": {
    "volume_24h": "767.4",
    "volume_event_count": 4,
    "volume_last_update": 1766998899,
    "change_percent_24h": "-0.10940064077517694",
    "change_open_price": "3839.1",
    "change_close_price": "3834.9",
    "markets_abs_cumulative": 1,
    "markets_abs_first_seen": 1766998590
  }
}
```

### 字段解读 / Field Interpretation

- 该Token在24小时内:
  - 总交易量为 767.4 SOL
  - 发生了 4 次交易事件
  - 价格下跌 0.109% (从3839.1降至3834.9)
  - 有 1 个独立钱包参与交易
  - 最后交易时间: 1766998899
  - 首次交易时间: 1766998590

- This token in 24 hours:
  - Total volume: 767.4 SOL
  - 4 trading events occurred
  - Price dropped 0.109% (from 3839.1 to 3834.9)
  - 1 unique wallet participated
  - Last trade time: 1766998899
  - First trade time: 1766998590

---

## 数据缺失处理 / Missing Data Handling

如果某个统计模块查询失败,对应的 `extras` 字段将不会被添加,但不影响其他字段:

If a statistics module query fails, the corresponding `extras` field will not be added, but won't affect other fields:

```rust
// 查询失败时记录警告,返回 None / Log warning on failure, return None
if let Some(volume_data) = volume_result {
    token.extras.insert("volume_24h", serde_json::json!(volume_data.volume.to_string()));
    // ...
}
// 其他字段继续处理 / Other fields continue processing
```

**代码位置** / Code: [src/router/token.rs:963-1004](src/router/token.rs#L963-L1004)

---

## 统计周期 / Statistics Period

所有 `extras` 字段默认统计周期为 **24小时** (`Period::TwentyFourHours`)

All `extras` fields default to a **24-hour** statistics period (`Period::TwentyFourHours`)

**代码位置** / Code: [src/router/token.rs:953](src/router/token.rs#L953)

---

## 性能考虑 / Performance Considerations

### 缓存机制 / Caching Mechanism

`/api/tokens/list` 接口对结果进行了缓存:

The `/api/tokens/list` endpoint caches results:

- **缓存键** / Cache Key: `{sort_by}:{limit}`
- **默认TTL** / Default TTL: 可配置,通常为60秒 / Configurable, typically 60 seconds
- **缓存存储** / Cache Storage: 内存HashMap / In-memory HashMap

**代码位置** / Code: [src/router/token.rs:339-341](src/router/token.rs#L339-L341)

### 并发优化 / Concurrency Optimization

三个统计模块的查询并发执行,减少总响应时间:

Three statistics modules query concurrently to reduce total response time:

- 串行查询总耗时: ~3 × T
- 并发查询总耗时: ~max(T)

---

## 存储位置 / Storage Location

`extras` 字段存储在 **事件数据库** (`./data/event`) 中:

The `extras` field is stored in the **event database** (`./data/event`):

- **键格式** / Key Format: `token:{mint}`
- **值格式** / Value Format: JSON (包含 `extras` 字段 / including `extras` field)
- **数据库类型** / Database Type: RocksDB

**相关文档** / Related Docs: [docs/mint列表键值对说明.md](./mint列表键值对说明.md)

---

## API 使用示例 / API Usage Examples

### 示例1: 获取单个Token(不含统计数据) / Example 1: Get Single Token (No Stats)

```bash
GET /api/tokens/mint/Eyr1aggRfAq4z9XnrAstTbYxodmCFWmwuEXr38GRFXeW
```

**响应** / Response: `extras` 为空对象 `{}` / `extras` is empty object `{}`

---

### 示例2: 获取单个Token(含统计数据) / Example 2: Get Single Token (With Stats)

```bash
GET /api/tokens/mint/Eyr1aggRfAq4z9XnrAstTbYxodmCFWmwuEXr38GRFXeW?include_stats=true
```

**响应** / Response: `extras` 包含完整的8个统计字段 / `extras` contains all 8 statistics fields

---

### 示例3: 获取交易量排行榜 / Example 3: Get Volume Ranking

```bash
GET /api/tokens/list?sort_by=liquid&limit=20
```

**响应** / Response:
- 返回20个Token,按 `volume_24h` 降序排列 / Returns 20 tokens sorted by `volume_24h` DESC
- 每个Token的 `extras` 包含完整统计数据 / Each token's `extras` contains full statistics

---

### 示例4: 获取热度排行榜 / Example 4: Get Hottest Ranking

```bash
GET /api/tokens/list?sort_by=hottest&limit=50
```

**响应** / Response:
- 返回50个Token,按 `markets_abs_cumulative` 降序排列 / Returns 50 tokens sorted by `markets_abs_cumulative` DESC
- 每个Token的 `extras` 包含完整统计数据 / Each token's `extras` contains full statistics

---

## 相关模块 / Related Modules

- **Volume模块** / Volume Module: `src/volume/`
- **Change模块** / Change Module: `src/change/`
- **MarketsAbs模块** / MarketsAbs Module: `src/markets_abs/`
- **Token路由** / Token Router: `src/router/token.rs`
- **Token存储** / Token Storage: `src/db/token_storage.rs`

---

## 更新日志 / Changelog

- **2025-12-29**: 初始文档创建 / Initial documentation created
- 基于提交: `b6c7689` / Based on commit: `b6c7689`
