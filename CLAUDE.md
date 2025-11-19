# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述
pinpet Server 是一个基于 Rust 开发的服务端应用程序，主要用于监听和处理 Solana 区块链上的 pinpet 合约事件，并提供相关的 API 服务。该项目整合了 Solana 区块链数据，使用 RocksDB 进行本地数据存储，并通过 RESTful API 为前端应用提供数据访问接口。

## 构建和测试命令

### 文档路径
  无特殊规定的话都放到 notes 目录, 用 md 方式编写, 用中文取文件名.

## 开发规范
  注释写中英文双语的.
  swagger 文档也写个中英文双语的.
  notes/代码接口规范.md

## 项目架构说明

### 整体架构
事件驱动的 Solana 区块链监控服务：
- WebSocket 监听区块链事件
- RocksDB 持久化存储
- Axum REST API 对外服务
- OpenAPI/Swagger 文档

### 代码文件说明

#### 入口
- `src/main.rs` - 应用启动：初始化日志、配置、数据库，启动事件监听和 HTTP 服务

#### 配置模块 (src/config.rs)
- 配置管理：环境变量和 TOML 文件加载

#### 数据库层 (src/db/)
- `mod.rs` - 模块导出
- `storage.rs` - RocksDB 初始化与性能调优
- `event_storage.rs` - 事件存储：复合键索引 (slot/mint/signature)
- `errors.rs` - 数据库错误类型

#### Solana 集成层 (src/solana/)
- `mod.rs` - 模块导出
- `client.rs` - Solana RPC 客户端
- `events.rs` - 事件类型定义：TokenCreated、BuySell、LongShort 等
- `listener.rs` - WebSocket 监听器：连接、解析、分发事件
- `storage_handler.rs` - 事件处理器：接收事件并存入 RocksDB

#### API 路由层 (src/router/)
- `mod.rs` - 路由组装
- `health.rs` - 健康检查端点
- `db.rs` - 数据库 CRUD 端点

#### 工具模块 (src/util/)
- `mod.rs` / `result.rs` - 通用响应类型

#### 文档模块 (src/docs/)
- `mod.rs` - OpenAPI 文档定义