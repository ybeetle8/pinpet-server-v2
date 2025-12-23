# Stats 键值存储说明 / Stats Key-Value Storage Specification

## 概述 / Overview
本文档详细说明了 pinpet-server-v2 中统计数据（Stats）模块的键值存储结构。Stats 模块使用 RocksDB 存储四个核心统计指标的数据及其排序索引。

## 数据库位置 / Database Location

**统计数据库路径 / Statistics Database Path:**
- 默认路径 / Default path: `./data/stats`
- 配置项 / Config key: `database.stats_db_path`
- 存储内容 / Storage content: 四大核心统计指标（Volume、Markets、MarketsAbs、Change）及其排序索引

**注意 / Note:** 本文档描述的所有统计键值结构均存储在统计数据库中，与事件数据库（`./data/event`）和OrderBook数据库（`./data/orderbook`）分离。

---

## 时间周期定义 / Time Period Definition

所有统计指标都支持以下时间周期：
- `1m` - 1分钟
- `5m` - 5分钟
- `15m` - 15分钟
- `1h` - 1小时
- `4h` - 4小时
- `24h` - 24小时

时间戳对齐规则：时间桶（time_bucket）总是对齐到周期边界。例如，1m 对齐到分钟整点，1h 对齐到小时整点。

---

## 核心键值结构 / Core Key-Value Structure

### 1. Volume (交易额)

#### 主数据键 / Main Data Key
```
键格式: vol:{period}:{mint}:{time_bucket:020}
值格式: JSON (VolumeData)
```

**键组成说明：**
- `vol` - 固定前缀，标识交易额数据
- `period` - 时间周期（1m/5m/15m/1h/4h/24h）
- `mint` - Token 的 Mint 地址
- `time_bucket` - 对齐后的 Unix 时间戳（20位零填充）

**值字段（VolumeData）：**
```json
{
  "volume": 12345.67,      // 交易额（USD）
  "event_count": 45,       // 事件数量
  "last_update": 1703001645 // 最后更新时间戳
}
```

#### 排序索引键 / Ranking Index Key
```
键格式: vol_rank:{period}:{time_bucket:020}:{volume_encoded:020}:{mint}
值格式: 空（仅利用键排序）
```

**交易额编码规则：**
- 20位零填充整数
- 单位：美分（cents）
- 示例：$1,234.56 → 00000000000000123456

---

### 2. Markets (周期内钱包数)

#### 主数据键 / Main Data Key
```
键格式: markets:{period}:{mint}:{time_bucket:020}
值格式: Bincode 序列化 (MarketsDataSerde)
```

**值字段（MarketsData）：**
- 使用 Bloom Filter 存储钱包去重信息
- `count` - 唯一钱包数量
- `event_count` - 事件总数
- `last_update` - 最后更新时间戳

**Bloom Filter 配置：**
- 容量：100,000 个元素
- 错误率：1%
- 空间效率：相比 HashSet 节省约 90% 空间

#### 排序索引键 / Ranking Index Key
```
键格式: markets_rank:{period}:{time_bucket:020}:{count:010}:{mint}
值格式: 空（仅利用键排序）
```

**钱包数编码规则：**
- 10位零填充整数
- 示例：12345 → 0000012345

---

### 3. MarketsAbs (绝对钱包数)

#### 全局主数据键 / Global Main Data Key
```
键格式: markets_abs:{mint}
值格式: Bincode 序列化 (MarketsAbsDataSerde)
```

**特点：**
- 不分时间周期，全局唯一
- 记录从创世到现在所有参与过的钱包
- 使用全局 Bloom Filter 去重

**值字段（MarketsAbsData）：**
- `count` - 全局唯一钱包总数
- `event_count` - 总事件数
- `first_seen` - 首次记录时间
- `last_update` - 最后更新时间
- Bloom Filter - 全局钱包去重

#### 时间周期索引键 / Period Index Key
```
键格式: markets_abs_period:{period}:{mint}:{time_bucket:020}
值格式: Bincode 序列化 (PeriodIndexData)
```

**值字段（PeriodIndexData）：**
```json
{
  "cumulative_count": 5678,  // 截至当前时间桶的累计钱包数
  "last_update": 1703001645   // 最后更新时间戳
}
```

#### 排序索引键 / Ranking Index Key
```
键格式: markets_abs_rank:{period}:{time_bucket:020}:{count:010}:{mint}
值格式: 空（仅利用键排序）
```

---

### 4. Change (涨跌幅)

#### 主数据键 / Main Data Key
```
键格式: change:{period}:{mint}:{time_bucket:020}
值格式: JSON (ChangeData)
```

**值字段（ChangeData）：**
```json
{
  "open_price": 0.000123,      // 开盘价（USD）
  "close_price": 0.000145,     // 收盘价（USD）
  "change_percent": 17.89,     // 涨跌百分比
  "first_event_time": 1703001600, // 首次事件时间
  "last_event_time": 1703001659   // 最后事件时间
}
```

#### 排序索引键 / Ranking Index Key
```
键格式: change_rank:{period}:{time_bucket:020}:{change_percent_encoded:+012}:{mint}
值格式: 空（仅利用键排序）
```

**涨跌幅编码规则：**
- 格式：符号(1位) + 数值(11位)
- 数值 = 涨跌幅 × 100，保留2位小数
- 示例：
  - +123.45% → +0000012345
  - -67.89% → -0000006789
  - +5.67% → +0000000567

**编码特性：**
- 字典序 = 数值序：+500 > +123 > +5 > -30 > -67
- 支持正向迭代（涨幅榜）
- 支持反向迭代（跌幅榜）

---

## 查询模式 / Query Patterns

### 单币种查询 / Single Token Query
通过前缀扫描实现高效查询：
- Volume: `vol:{period}:{mint}:`
- Markets: `markets:{period}:{mint}:`
- MarketsAbs: `markets_abs:{mint}`
- Change: `change:{period}:{mint}:`

### Top K 查询 / Top K Query
通过排序索引实现 O(K) 复杂度查询：

**涨幅榜 Top K：**
- 前缀：`change_rank:{period}:{time_bucket}:+`
- 方向：正向迭代
- 限制：取前 K 个

**跌幅榜 Top K：**
- 前缀：`change_rank:{period}:{time_bucket}:-`
- 方向：反向迭代（从小到大）
- 限制：取前 K 个

**交易量榜 Top K：**
- 前缀：`vol_rank:{period}:{time_bucket}:`
- 方向：反向迭代（从大到小）
- 限制：取前 K 个

**活跃度榜 Top K：**
- 前缀：`markets_rank:{period}:{time_bucket}:`
- 方向：反向迭代（从大到小）
- 限制：取前 K 个

---

## 更新策略 / Update Strategy

### 原子更新 / Atomic Updates
使用 WriteBatch 保证数据一致性：
1. 更新主数据
2. 删除旧排序索引
3. 插入新排序索引
4. 批量提交

### 索引维护 / Index Maintenance
每个事件需要更新：
- 6个周期 × 4个指标 = 24条主数据记录
- 6个周期 × 4个指标 = 24条排序索引（删除旧值+插入新值）
- 总计：约 48-72 条记录操作/事件

---

## 存储优化 / Storage Optimization

### Bloom Filter 使用
Markets 和 MarketsAbs 使用 Bloom Filter 代替 HashSet：
- 空间节省：约 90%
- 误报率：1%
- 适用场景：只需要判断钱包是否存在，不需要遍历

### 数据压缩
- L0-L2：LZ4 轻量压缩
- L3+：Zstd 高压缩比
- 预期压缩率：50-70%

### 键长度优化
- 使用固定长度编码确保正确排序
- Mint 地址放在键的末尾，便于前缀扫描

---

## 性能特性 / Performance Characteristics

| 操作类型 | 时间复杂度 | 说明 |
|---------|-----------|------|
| 单币查询 | O(1) | 直接键查找 |
| 时间范围查询 | O(T) | T为时间点数量 |
| Top K 查询（有索引）| O(K) | 利用排序索引 |
| Top K 查询（无索引）| O(N) | N为全部币种数量 |
| 更新操作 | O(1) | 批量原子更新 |

**性能优势：**
- Top K 查询性能提升：约 100 倍（N=10000, K=100）
- 单币查询：亚毫秒级响应
- 批量更新：支持 1000+ TPS

---

## 数据保留策略 / Data Retention Policy

| 周期 | 保留时间 | 说明 |
|------|---------|------|
| 1m | 7天 | 高频数据，短期保留 |
| 5m | 30天 | 中频数据 |
| 15m | 90天 | 中频数据 |
| 1h | 180天 | 低频数据 |
| 4h | 365天 | 低频数据 |
| 24h | 永久 | 日线数据，永久保留 |

清理策略：
- 定时任务每日凌晨执行
- 按时间桶边界批量删除
- 同时清理主数据和索引

---

## 注意事项 / Notes

1. **Mint 地址可能包含冒号**：解析键时需要正确处理
2. **Bloom Filter 误报**：Markets 统计可能略高于实际值（约1%）
3. **索引一致性**：必须使用 WriteBatch 保证主数据和索引同步更新
4. **时间对齐**：所有时间戳必须对齐到周期边界
5. **编码格式**：数值编码必须固定长度，确保字典序正确

---

**文档版本**: v1.0
**最后更新**: 2025-12-23