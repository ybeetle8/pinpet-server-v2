#!/bin/bash
# 检查特定用户的 orderbook_slot 数据

USER="F5eLfL7qeEZhvbbXT1pmkiLN1opRFcQSDyaDaRSQ2ehD"
MINT="31i9zwjYiKnuwTsnrE8Zq2XM7hbY3RozF5LDnH5zePin"

echo "=== 检查用户索引键 ==="
cargo run --example scan_all_prefixes 2>/dev/null | grep "orderbook_user:$USER" | head -5

echo ""
echo "=== 检查对应的 slot 数据 ==="
# 从用户索引中提取 order_id，然后查 id_map，再查 slot
cargo run --example check_user_data "$USER" 2>/dev/null | head -30
