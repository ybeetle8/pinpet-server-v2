# K线键值存储说明 / K-line Key-Value Storage Documentation

## 文档概述 / Document Overview

本文档说明pinpet-server-v2项目中K线数据在RocksDB中的键值存储设计方案。

This document describes the K-line data key-value storage design in RocksDB for the pinpet-server-v2 project.

## 数据库位置 / Database Location

**统计数据库路径 / Statistics Database Path:**
- 默认路径 / Default path: `./data/stats`
- 配置项 / Config key: `database.stats_db_path`
- 存储内容 / Storage content: K线数据、交易额统计、涨跌幅统计、钱包数统计等

**注意 / Note:** 本文档描述的所有K线键值结构均存储在统计数据库中，与事件数据库（`./data/event`）和OrderBook数据库（`./data/orderbook`）分离。

---

## 1. 存储架构 / Storage Architecture

### 1.1 设计原则 / Design Principles

- **时间序列优化** / Time-series optimization: 键设计支持高效的时间范围查询
- **多维度索引** / Multi-dimensional indexing: 支持按mint、interval、时间等维度查询
- **存储效率** / Storage efficiency: 合理的键长度和数据组织
- **查询性能** / Query performance: 前缀扫描和范围查询优化

### 1.2 数据流向 / Data Flow

```
区块链事件 → 事件处理器 → K线生成器 → RocksDB存储
Blockchain Event → Event Handler → K-line Generator → RocksDB Storage
```

---

## 2. 键空间设计 / Key Space Design

### 2.1 键前缀规范 / Key Prefix Specification

所有K线相关的键使用统一前缀以便于管理和隔离:

All K-line related keys use unified prefixes for management and isolation:

| 前缀 Prefix | 用途 Purpose | 示例 Example |
|------------|-------------|--------------|
| `kline:` | K线主数据存储 / K-line main data storage | `kline:mint:interval:timestamp` |
| `kline_index:` | K线索引数据 / K-line index data | `kline_index:mint:interval` |
| `kline_meta:` | K线元数据 / K-line metadata | `kline_meta:mint:interval:stats` |

---

## 3. 主键设计 / Primary Key Design

### 3.1 K线数据主键 / K-line Data Primary Key

#### 键格式 / Key Format:
```
kline:{mint}:{interval}:{timestamp}
```

#### 字段说明 / Field Description:

| 字段 Field | 类型 Type | 说明 Description | 示例 Example |
|-----------|----------|-----------------|--------------|
| `mint` | String | 代币mint地址 / Token mint address | `7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU` |
| `interval` | String | 时间间隔 / Time interval | `s1`, `s30`, `m5`, `h1`, `d1` |
| `timestamp` | u64 (10位零填充) | Unix时间戳(秒) / Unix timestamp (seconds) | `0001732185600` (补零到10位) |

#### 完整示例 / Complete Example:
```
kline:7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU:s30:0001732185600
```

#### 时间戳格式说明 / Timestamp Format:
- 使用10位数字,不足前补0 / Use 10 digits, pad with leading zeros
- 保证字典序和时间序一致 / Ensure lexicographic order matches chronological order
- 示例: `1732185600` → `0001732185600`

---

### 3.2 支持的时间间隔 / Supported Time Intervals

| 间隔标识 Interval | 含义 Meaning | 时间长度 Duration | 时间桶对齐 Alignment |
|------------------|-------------|------------------|---------------------|
| `s1` | 1秒K线 / 1-second K-line | 1秒 / 1 second | 不对齐 / No alignment |
| `s30` | 30秒K线 / 30-second K-line | 30秒 / 30 seconds | 30秒边界 / 30s boundary |
| `m5` | 5分钟K线 / 5-minute K-line | 300秒 / 300 seconds | 5分钟边界 / 5min boundary |
| `h1` | 1小时K线 / 1-hour K-line | 3600秒 / 3600 seconds | 整点对齐 / Hour boundary |
| `d1` | 1天K线 / 1-day K-line | 86400秒 / 86400 seconds | UTC 0点对齐 / UTC midnight |

---

## 4. 索引键设计 / Index Key Design

### 4.1 Mint索引 / Mint Index

用于快速列出某个mint的所有时间间隔:

Used to quickly list all intervals for a mint:

#### 键格式 / Key Format:
```
kline_index:mint:{mint}:{interval}
```

#### 值 / Value:
```
空字符串 (仅键存在即可) / Empty string (key existence only)
```

#### 用途 / Purpose:
- 查询某个mint支持的所有interval
- Query all intervals supported by a mint

---

### 4.2 时间范围索引 / Time Range Index

#### 键格式 / Key Format:
```
kline_index:time:{interval}:{timestamp}:{mint}
```

#### 用途 / Purpose:
- 按时间范围查询所有mint的K线
- Query K-lines for all mints within a time range
- 跨mint的时间序列分析
- Cross-mint time series analysis

---

## 5. 元数据键设计 / Metadata Key Design

### 5.1 统计元数据 / Statistics Metadata

#### 键格式 / Key Format:
```
kline_meta:{mint}:{interval}:stats
```

#### 存储内容 / Stored Content:
- 首条K线时间戳 / First K-line timestamp
- 最后一条K线时间戳 / Last K-line timestamp
- K线总数量 / Total K-line count
- 最后更新时间 / Last update time

---

### 5.2 配置元数据 / Configuration Metadata

#### 键格式 / Key Format:
```
kline_meta:config:{key}
```

#### 存储内容 / Stored Content:
- 全局K线配置信息 / Global K-line configuration
- 数据保留策略 / Data retention policy
- 聚合规则 / Aggregation rules

---

## 6. 查询模式 / Query Patterns

### 6.1 单点查询 / Point Query

查询特定mint在特定时间的K线:

Query K-line for specific mint at specific time:

```
键: kline:{mint}:{interval}:{timestamp}
Key: kline:{mint}:{interval}:{timestamp}
```

---

### 6.2 范围查询 / Range Query

查询特定mint在时间范围内的K线:

Query K-lines for specific mint within time range:

```
起始键: kline:{mint}:{interval}:{start_timestamp}
结束键: kline:{mint}:{interval}:{end_timestamp}
Start key: kline:{mint}:{interval}:{start_timestamp}
End key: kline:{mint}:{interval}:{end_timestamp}
```

---

### 6.3 前缀查询 / Prefix Query

查询某个mint所有interval的K线:

Query all interval K-lines for a mint:

```
前缀: kline:{mint}:
Prefix: kline:{mint}:
```

---

### 6.4 倒序查询 / Reverse Query

获取最新的N条K线数据:

Get the latest N K-line records:

```
前缀: kline:{mint}:{interval}:
方向: 逆向迭代 (从大到小)
Prefix: kline:{mint}:{interval}:
Direction: Reverse iteration (from large to small)
```

---

## 7. 键空间管理 / Key Space Management

### 7.1 键长度估算 / Key Length Estimation

| 组成部分 Component | 长度 Length | 示例 Example |
|-------------------|------------|--------------|
| 前缀 Prefix | 6字节 / 6 bytes | `kline:` |
| Mint地址 Address | 43-44字节 / 43-44 bytes | `7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU` |
| 间隔 Interval | 2-3字节 / 2-3 bytes | `s30` |
| 时间戳 Timestamp | 10字节 / 10 bytes | `0001732185600` |
| 分隔符 Separators | 3字节 / 3 bytes | `:` × 3 |
| **总计 Total** | **64-66字节** | **64-66 bytes** |

---

### 7.2 键命名规范 / Key Naming Convention

1. **使用小写字母** / Use lowercase letters
2. **使用冒号分隔** / Use colon as separator
3. **时间戳零填充** / Pad timestamps with zeros
4. **避免特殊字符** / Avoid special characters
5. **保持一致性** / Maintain consistency

---

## 8. 数据保留策略 / Data Retention Policy

### 8.1 按时间间隔的保留策略 / Retention by Interval

| 间隔 Interval | 建议保留时长 Recommended Retention | 说明 Description |
|--------------|-----------------------------------|-----------------|
| `s1` | 24小时 / 24 hours | 高频数据,保留时间较短 / High-frequency data, shorter retention |
| `s30` | 7天 / 7 days | 中频数据,适中保留 / Medium-frequency data, moderate retention |
| `m5` | 30天 / 30 days | 低频数据,较长保留 / Low-frequency data, longer retention |
| `h1` | 90天 / 90 days | 小时线,每天最多24条 / Hourly data, max 24 records per day |
| `d1` | 永久 / Permanent | 日线,每天最多1条 / Daily data, max 1 record per day |

### 8.2 清理机制 / Cleanup Mechanism

#### 键格式示例 / Key Format Example:
```
# 清理s1间隔超过24小时的数据
# Clean up s1 interval data older than 24 hours
kline:{mint}:s1:{timestamp < now - 86400}
```

---

## 9. 性能优化建议 / Performance Optimization

### 9.1 RocksDB配置 / RocksDB Configuration

- **Bloom过滤器** / Bloom filters: 启用以加速键查找
- **块缓存** / Block cache: 适当增大以提高读性能
- **压缩算法** / Compression: 使用LZ4或Snappy
- **写缓冲** / Write buffer: 合理配置以平衡内存和性能

### 9.2 批量操作 / Batch Operations

- 使用WriteBatch进行批量写入
- Use WriteBatch for bulk writes
- 避免频繁的小批量写入
- Avoid frequent small writes
- 定期批量清理过期数据
- Periodically batch-clean expired data

---

## 10. 扩展性考虑 / Scalability Considerations

### 10.1 分片策略 / Sharding Strategy

如果单个RocksDB实例无法满足性能需求,可以考虑:

If a single RocksDB instance cannot meet performance requirements, consider:

1. **按mint分片** / Shard by mint
   - 不同mint存储在不同数据库实例
   - Different mints stored in different database instances

2. **按时间分片** / Shard by time
   - 不同时间段存储在不同数据库
   - Different time periods stored in different databases

3. **按interval分片** / Shard by interval
   - 不同时间间隔独立存储
   - Different intervals stored independently

---

## 11. 监控指标 / Monitoring Metrics

### 11.1 存储指标 / Storage Metrics

- **总键数量** / Total key count
- **存储空间使用** / Storage space usage
- **每个mint的K线数量** / K-line count per mint
- **每个interval的数据量** / Data volume per interval

### 11.2 性能指标 / Performance Metrics

- **读取延迟** / Read latency
- **写入延迟** / Write latency
- **查询吞吐量** / Query throughput
- **批量操作耗时** / Batch operation duration

---

## 12. 故障恢复 / Disaster Recovery

### 12.1 备份策略 / Backup Strategy

- **全量备份** / Full backup: 定期创建完整数据快照
- **增量备份** / Incremental backup: 基于时间戳的增量备份
- **备份验证** / Backup validation: 定期验证备份完整性

### 12.2 恢复流程 / Recovery Process

1. 停止写入服务 / Stop write service
2. 从备份恢复数据 / Restore data from backup
3. 验证数据完整性 / Verify data integrity
4. 重启服务 / Restart service

---

## 13. 最佳实践 / Best Practices

### 13.1 键设计 / Key Design

- ✅ 使用固定长度的时间戳 / Use fixed-length timestamps
- ✅ 保持键的可读性 / Maintain key readability
- ✅ 使用一致的分隔符 / Use consistent separators
- ✅ 避免键冲突 / Avoid key collisions

### 13.2 数据管理 / Data Management

- ✅ 定期清理过期数据 / Regularly clean expired data
- ✅ 监控存储空间使用 / Monitor storage space usage
- ✅ 实施合理的保留策略 / Implement reasonable retention policies
- ✅ 记录数据变更日志 / Log data change operations

### 13.3 查询优化 / Query Optimization

- ✅ 利用键前缀进行范围查询 / Use key prefixes for range queries
- ✅ 避免全表扫描 / Avoid full table scans
- ✅ 使用迭代器进行大量数据遍历 / Use iterators for large data traversal
- ✅ 缓存热点数据 / Cache hot data

---

## 14. 相关文档 / Related Documents

- [数据库存储架构](./数据库存储架构.md)
- [事件存储说明](./事件存储说明.md)
- [Token存储说明](./Token存储说明.md)
- [K线推送服务使用说明](../notes/kline_socket_service使用说明.md)

---

## 15. 版本历史 / Version History

| 版本 Version | 日期 Date | 说明 Description |
|-------------|----------|-----------------|
| 1.0 | 2025-11-21 | 初始版本 / Initial version |
| 1.1 | 2026-04-28 | 新增h1(小时线)和d1(日线)周期 / Add h1 (hourly) and d1 (daily) intervals |

---

**文档维护者 / Document Maintainer**: pinpet-server-v2 开发团队 / Development Team

**最后更新 / Last Updated**: 2026-04-28
