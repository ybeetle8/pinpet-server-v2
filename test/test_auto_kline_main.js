#!/usr/bin/env node

// 监听指定 mint 的 K线数据脚本
// Monitor kline data for a specific mint

const { io } = require('socket.io-client');

// 配置
const SERVER_URL = 'http://47.109.157.92:3000';
const INTERVAL = 's1';
const TARGET_MINT = '31i9zwjYiKnuwTsnrE8Zq2XM7hbY3RozF5LDnH5zePin';

let currentMint = null;
let socket = null;

console.log('🚀 启动指定 mint 监听...');
console.log(`📍 服务器地址: ${SERVER_URL}`);
console.log(`🎯 目标 mint: ${TARGET_MINT}`);
console.log(`⏰ 监听间隔: ${INTERVAL}`);

// 连接 WebSocket 并监听行情
function connectAndSubscribe(mint) {
    console.log(`\n🔌 连接 WebSocket 并监听 ${mint} 的行情...`);
    
    // 创建 Socket.IO 客户端 - 连接到 /kline 命名空间
    socket = io(`${SERVER_URL}/kline`, {
        transports: ['websocket', 'polling'],
        timeout: 20000,
        reconnection: true,
        reconnectionAttempts: 10,
        reconnectionDelay: 2000,
    });

    // 连接事件监听
    socket.on('connect', () => {
        console.log('✅ WebSocket 连接成功');
        console.log(`🔌 Socket ID: ${socket.id}`);
        
        // 等待连接稳定后订阅
        setTimeout(() => {
            subscribeKline(mint);
        }, 1000);
    });

    socket.on('disconnect', (reason) => {
        console.log(`❌ 连接断开: ${reason}`);
        console.log('🔄 等待重连...');
    });

    socket.on('connect_error', (error) => {
        console.log(`💥 连接错误: ${error.message}`);
    });

    // 接收服务器消息
    socket.on('connection_success', (data) => {
        console.log('🎉 收到连接成功消息:', JSON.stringify(data, null, 2));
    });

    socket.on('subscription_confirmed', (data) => {
        console.log('✅ 订阅确认:', JSON.stringify(data, null, 2));
    });

    socket.on('history_data', (data) => {
        if (data.interval === INTERVAL) {
            console.log(`📈 历史数据:`, {
                symbol: data.symbol,
                interval: data.interval,
                dataPoints: data.data.length,
                hasMore: data.has_more,
                totalCount: data.total_count
            });
            
            // 逐条打印每条K线
            data.data.forEach((kline, idx) => {
                const time = new Date(kline.time * 1000).toISOString();
                console.log(`   K线 #${idx + 1}: time=${time}, open=${kline.open}, high=${kline.high}, low=${kline.low}, close=${kline.close}, volume=${kline.volume}`);
            });
        }
    });

    socket.on('kline_data', (data) => {
        console.log(`🔔 收到K线数据 (原始):`, {
            interval: data.interval,
            expected: INTERVAL,
            symbol: data.symbol,
            timestamp: data.timestamp,
            dataSize: JSON.stringify(data).length
        });
        
        if (data.interval === INTERVAL) {
            const klineTime = new Date(data.data.time * 1000);
            console.log(`📊 实时K线更新:`, data);
        } else {
            console.log(`⚠️ 收到其他间隔的K线数据: ${data.interval}, 期望: ${INTERVAL}`);
        }
    });

    socket.on('error', (error) => {
        console.log('❌ 错误消息:', JSON.stringify(error, null, 2));
    });

    // 监听直接测试事件
    socket.on('direct_kline_test', (data) => {
        console.log('🧪 收到直接测试消息:', {
            interval: data.interval,
            symbol: data.symbol,
            timestamp: new Date(data.timestamp).toISOString()
        });
    });

    // 捕获所有事件
    socket.onAny((eventName, ...args) => {
        console.log(`🎯 收到事件: ${eventName}`, {
            eventName,
            argsCount: args.length,
            firstArg: args[0] ? JSON.stringify(args[0]).substring(0, 200) + '...' : 'no args'
        });
    });
}

// 订阅 K线数据
function subscribeKline(mint) {
    console.log(`\n📊 订阅 ${mint} 的 ${INTERVAL} K线数据...`);
    socket.emit('subscribe', {
        symbol: mint,
        interval: INTERVAL,
        subscription_id: `auto_monitor_${Date.now()}`,
        limit: 105
    });
}

// 主函数
function main() {
    currentMint = TARGET_MINT;
    connectAndSubscribe(TARGET_MINT);
}

// 错误处理
process.on('unhandledRejection', (reason, promise) => {
    console.log('Unhandled Rejection at:', promise, 'reason:', reason);
});

process.on('uncaughtException', (error) => {
    console.log('Uncaught Exception:', error);
    process.exit(1);
});

// 优雅退出
process.on('SIGINT', () => {
    console.log('\n👋 收到退出信号，正在断开连接...');
    if (socket) {
        socket.disconnect();
    }
    process.exit(0);
});

console.log('\n📋 功能说明:');
console.log('  - 监听指定 mint 地址的 K线数据');
console.log('  - 连接到 WebSocket 服务器');
console.log('  - 订阅该 mint 的 K线数据');
console.log('  - 持续接收并显示实时更新');
console.log('  - 按 Ctrl+C 退出监听\n');

// 启动程序
main();