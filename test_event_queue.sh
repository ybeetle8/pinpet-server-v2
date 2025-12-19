#!/bin/bash

echo "======================================"
echo "测试事件队列功能 / Testing Event Queue"
echo "======================================"

# 清理旧数据 / Clean old data
echo "1. 清理旧数据 / Cleaning old data..."
rm -rf ./data/event_queue

# 启动服务器 / Start server
echo "2. 启动服务器 / Starting server..."
timeout 10 cargo run 2>&1 | tee server.log &
SERVER_PID=$!

# 等待服务器启动 / Wait for server to start
echo "3. 等待服务器启动 / Waiting for server to start..."
sleep 5

# 检查日志 / Check logs
echo "4. 检查日志 / Checking logs..."
echo ""

# 检查是否有锁错误 / Check for lock errors
if grep -q "LOCK: No locks available" server.log; then
    echo "❌ 发现锁错误 / Lock error found!"
    grep "LOCK" server.log
else
    echo "✅ 没有锁错误 / No lock errors!"
fi

# 检查是否成功初始化 / Check successful initialization
if grep -q "使用无界通道和持久化队列" server.log; then
    echo "✅ 事件队列初始化成功 / Event queue initialized successfully!"
else
    echo "❌ 事件队列初始化失败 / Event queue initialization failed!"
fi

# 检查是否启动了清理任务 / Check cleanup task
if grep -q "启动事件队列清理任务" server.log; then
    echo "✅ 清理任务启动成功 / Cleanup task started successfully!"
else
    echo "⚠️ 清理任务未启动 / Cleanup task not started!"
fi

echo ""
echo "======================================"
echo "服务器日志摘要 / Server Log Summary:"
echo "======================================"
grep -E "(事件队列|Event queue|无界通道|unbounded channel|持久化|persistence)" server.log | head -10

# 停止服务器 / Stop server
echo ""
echo "5. 停止服务器 / Stopping server..."
kill $SERVER_PID 2>/dev/null

# 清理日志文件 / Clean log file
rm -f server.log

echo ""
echo "测试完成 / Test completed!"