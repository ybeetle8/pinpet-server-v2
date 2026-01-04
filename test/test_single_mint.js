#!/usr/bin/env node

// 单个 mint 事件监听测试脚本
// Test script for monitoring events of a single mint

const { io } = require('socket.io-client');

// 配置 / Configuration
const SERVER_URL = 'http://192.168.18.5:3000';
const MINT_ADDRESS = 'J6Z2XDGrGwkkxNpStA6RrGm5DbQVnt2AfWR1tfqodPet'; // 在这里填写要监听的 mint 地址 / Fill in the mint address to monitor here
const INTERVAL = 's30'; // K线间隔 / K-line interval

let socket = null;

console.log('🚀 启动单个 mint 事件监听...');
console.log('   Starting single mint event monitoring...');
console.log(`📍 服务器地址 / Server URL: ${SERVER_URL}`);
console.log(`🎯 监听 Mint / Monitoring Mint: ${MINT_ADDRESS}`);
console.log(`⏰ 间隔 / Interval: ${INTERVAL}`);
console.log('');

// 连接 WebSocket 并监听事件
// Connect to WebSocket and listen for events
function connectAndSubscribe() {
    console.log(`🔌 正在连接 WebSocket...`);
    console.log(`   Connecting to WebSocket...`);

    // 创建 Socket.IO 客户端 - 连接到 /kline 命名空间
    // Create Socket.IO client - connect to /kline namespace
    socket = io(`${SERVER_URL}/kline`, {
        transports: ['websocket', 'polling'],
        timeout: 20000,
        reconnection: true,
        reconnectionAttempts: 10,
        reconnectionDelay: 2000,
    });

    // 连接事件监听 / Connection event listeners
    socket.on('connect', () => {
        console.log('✅ WebSocket 连接成功');
        console.log('   WebSocket connected successfully');
        console.log(`🔌 Socket ID: ${socket.id}`);
        console.log('');

        // 等待连接稳定后订阅 / Subscribe after connection is stable
        setTimeout(() => {
            subscribeEvents();
        }, 1000);
    });

    socket.on('disconnect', (reason) => {
        console.log(`❌ 连接断开 / Disconnected: ${reason}`);
        console.log('🔄 等待重连... / Waiting for reconnection...');
    });

    socket.on('connect_error', (error) => {
        console.log(`💥 连接错误 / Connection error: ${error.message}`);
    });

    // 捕获所有 socket.io 事件并打印
    // Capture and print all socket.io events
    socket.onAny((eventName, ...args) => {
        console.log('\n' + '='.repeat(80));
        console.log(`📨 收到事件 / Received Event: ${eventName}`);
        console.log('   时间 / Time:', new Date().toISOString());
        console.log('   参数数量 / Args count:', args.length);
        console.log('   事件数据 / Event Data:');
        console.log(JSON.stringify(args, null, 2));
        console.log('='.repeat(80));
    });
}

// 订阅事件数据
// Subscribe to event data
function subscribeEvents() {
    console.log(`📊 订阅 ${MINT_ADDRESS} 的事件数据...`);
    console.log(`   Subscribing to event data for ${MINT_ADDRESS}...`);
    console.log('');

    socket.emit('subscribe', {
        symbol: MINT_ADDRESS,
        interval: INTERVAL,
        subscription_id: `single_mint_monitor_${Date.now()}`
    });

    console.log('✅ 订阅请求已发送，等待接收数据...');
    console.log('   Subscription request sent, waiting for data...');
    console.log('');
}

// 错误处理 / Error handling
process.on('unhandledRejection', (reason, promise) => {
    console.log('Unhandled Rejection at:', promise, 'reason:', reason);
});

process.on('uncaughtException', (error) => {
    console.log('Uncaught Exception:', error);
    process.exit(1);
});

// 优雅退出 / Graceful exit
process.on('SIGINT', () => {
    console.log('\n👋 收到退出信号，正在断开连接...');
    console.log('   Exit signal received, disconnecting...');
    if (socket) {
        socket.disconnect();
    }
    process.exit(0);
});

console.log('📋 功能说明 / Features:');
console.log('  - 监听指定 mint 地址的所有事件');
console.log('    Monitor all events for the specified mint address');
console.log('  - 打印所有收到的 socket.io 事件及其完整数据');
console.log('    Print all received socket.io events with complete data');
console.log('  - 按 Ctrl+C 退出监听');
console.log('    Press Ctrl+C to exit');
console.log('');

// 验证 mint 地址是否已配置
// Verify if mint address is configured
if (MINT_ADDRESS === 'YOUR_MINT_ADDRESS_HERE') {
    console.error('❌ 错误: 请先在文件开头配置 MINT_ADDRESS 常量!');
    console.error('   Error: Please configure MINT_ADDRESS constant at the beginning of the file!');
    process.exit(1);
}

// 启动程序 / Start the program
connectAndSubscribe();
