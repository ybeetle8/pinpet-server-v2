#!/bin/bash
# 检查用户订单数据的脚本

USER="F5eLfL7qeEZhvbbXT1pmkiLN1opRFcQSDyaDaRSQ2ehD"

echo "=== 检查用户前缀键 ==="
cargo run --release --bin rocksdb_inspector -- scan "orderbook_user:${USER}:" 10

echo ""
echo "=== 检查第一个键的详细数据 ==="
cargo run --release --bin rocksdb_inspector -- get-first "orderbook_user:${USER}:"
