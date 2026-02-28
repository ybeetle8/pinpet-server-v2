#!/bin/bash

# Nginx 安装和配置脚本 for Ubuntu 24
# Nginx Installation and Configuration Script for Ubuntu 24
# 用途 Purpose: 配置 api.pinpet.fun 反向代理到 localhost:3000

set -e

# 颜色定义 Color definitions
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 配置变量 Configuration variables
DOMAIN="api.pinpet.fun"
BACKEND_URL="http://localhost:3000"
CERT_KEY="/root/pinpet-server-main/cer/pinpet.key"
CERT_PEM="/root/pinpet-server-main/cer/pinpet.pem"
NGINX_CONF="/etc/nginx/sites-available/${DOMAIN}"
NGINX_ENABLED="/etc/nginx/sites-enabled/${DOMAIN}"

# 打印信息函数 Print info function
print_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

# 检查是否为 root 用户 Check if running as root
check_root() {
    if [ "$EUID" -ne 0 ]; then
        print_error "请使用 root 权限运行此脚本 Please run as root"
        exit 1
    fi
}

# 检查证书文件 Check certificate files
check_certificates() {
    print_info "检查证书文件 Checking certificate files..."

    if [ ! -f "$CERT_KEY" ]; then
        print_error "证书密钥文件不存在 Certificate key file not found: $CERT_KEY"
        exit 1
    fi

    if [ ! -f "$CERT_PEM" ]; then
        print_error "证书文件不存在 Certificate file not found: $CERT_PEM"
        exit 1
    fi

    print_info "证书文件检查通过 Certificate files verified"
}

# 创建单文件 Nginx 配置 Create single-file nginx.conf
create_single_nginx_conf() {
    print_info "创建单文件 Nginx 配置 Creating single-file nginx.conf..."

    cat > /etc/nginx/nginx.conf << 'SINGLECONF'
user www-data;
worker_processes auto;
pid /run/nginx.pid;
error_log /var/log/nginx/error.log;

events {
    worker_connections 768;
}

http {
    # 基础设置 Basic Settings
    sendfile on;
    tcp_nopush on;
    types_hash_max_size 2048;
    default_type application/octet-stream;

    # 内联 MIME 类型 Inline MIME types
    types {
        text/html                             html htm shtml;
        text/css                              css;
        text/xml                              xml;
        application/javascript                js;
        application/json                      json;
        image/gif                             gif;
        image/jpeg                            jpeg jpg;
        image/png                             png;
        image/svg+xml                         svg svgz;
        image/webp                            webp;
        application/pdf                       pdf;
        application/octet-stream              bin exe dll;
    }

    # SSL 设置 SSL Settings
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_prefer_server_ciphers on;

    # 日志设置 Logging Settings
    access_log /var/log/nginx/access.log;

    # Gzip 压缩 Gzip Compression
    gzip on;
    gzip_vary on;
    gzip_types text/plain text/css application/json application/javascript text/xml application/xml;

    # WebSocket 连接升级映射 WebSocket Connection Upgrade Mapping
    map $http_upgrade $connection_upgrade {
        default upgrade;
        '' close;
    }

    # HTTP 服务器 HTTP Server
    server {
        listen 80;
        listen [::]:80;
        server_name api.pinpet.fun;

        # 日志配置 Log Configuration
        access_log /var/log/nginx/api.pinpet.fun.http.access.log;
        error_log /var/log/nginx/api.pinpet.fun.http.error.log;

        # 客户端上传大小限制 Client Upload Size Limit
        client_max_body_size 100M;

        # Socket.IO 专用配置 Socket.IO Specific Configuration
        location /socket.io/ {
            proxy_pass http://localhost:3000;
            proxy_http_version 1.1;

            # WebSocket 升级头（必需）WebSocket Upgrade Headers (Required)
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection "upgrade";

            # 代理头设置 Proxy Headers
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
            proxy_set_header X-Forwarded-Host $host;
            proxy_set_header X-Forwarded-Port $server_port;

            # Socket.IO 长连接超时设置 Socket.IO Long Connection Timeout
            proxy_connect_timeout 7d;
            proxy_send_timeout 7d;
            proxy_read_timeout 7d;

            # 禁用缓冲和缓存以支持实时通信 Disable buffering and caching for real-time communication
            proxy_buffering off;
            proxy_cache off;
        }

        # K线 WebSocket 配置 Kline WebSocket Configuration
        location /kline {
            proxy_pass http://localhost:3000;
            proxy_http_version 1.1;

            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection "upgrade";
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;

            proxy_connect_timeout 7d;
            proxy_send_timeout 7d;
            proxy_read_timeout 7d;
            proxy_buffering off;
        }

        # 通用 WebSocket 路径 General WebSocket Path
        location /ws {
            proxy_pass http://localhost:3000;
            proxy_http_version 1.1;

            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection "upgrade";
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;

            proxy_connect_timeout 7d;
            proxy_send_timeout 7d;
            proxy_read_timeout 7d;
            proxy_buffering off;
        }

        # 反向代理配置 Reverse Proxy Configuration
        location / {
            proxy_pass http://localhost:3000;
            proxy_http_version 1.1;

            # WebSocket 支持 WebSocket Support
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection $connection_upgrade;

            # 代理头设置 Proxy Headers
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
            proxy_set_header X-Forwarded-Host $host;
            proxy_set_header X-Forwarded-Port $server_port;

            # 超时设置 Timeout Settings
            proxy_connect_timeout 60s;
            proxy_send_timeout 60s;
            proxy_read_timeout 60s;

            # 缓冲设置 Buffer Settings
            proxy_buffering off;
            proxy_request_buffering off;
        }
    }

    # HTTPS 服务器 HTTPS Server
    server {
        listen 443 ssl http2;
        listen [::]:443 ssl http2;
        server_name api.pinpet.fun;

        # SSL 证书配置 SSL Certificate Configuration
        ssl_certificate /root/pinpet-server-main/cer/pinpet.pem;
        ssl_certificate_key /root/pinpet-server-main/cer/pinpet.key;

        # SSL 安全配置 SSL Security Configuration
        ssl_protocols TLSv1.2 TLSv1.3;
        ssl_ciphers HIGH:!aNULL:!MD5;
        ssl_prefer_server_ciphers on;
        ssl_session_cache shared:SSL:10m;
        ssl_session_timeout 10m;

        # 日志配置 Log Configuration
        access_log /var/log/nginx/api.pinpet.fun.access.log;
        error_log /var/log/nginx/api.pinpet.fun.error.log;

        # 客户端上传大小限制 Client Upload Size Limit
        client_max_body_size 100M;

        # Socket.IO 专用配置 Socket.IO Specific Configuration
        location /socket.io/ {
            proxy_pass http://localhost:3000;
            proxy_http_version 1.1;

            # WebSocket 升级头（必需）WebSocket Upgrade Headers (Required)
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection "upgrade";

            # 代理头设置 Proxy Headers
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
            proxy_set_header X-Forwarded-Host $host;
            proxy_set_header X-Forwarded-Port $server_port;

            # Socket.IO 长连接超时设置 Socket.IO Long Connection Timeout
            proxy_connect_timeout 7d;
            proxy_send_timeout 7d;
            proxy_read_timeout 7d;

            # 禁用缓冲和缓存以支持实时通信 Disable buffering and caching for real-time communication
            proxy_buffering off;
            proxy_cache off;
        }

        # K线 WebSocket 配置 Kline WebSocket Configuration
        location /kline {
            proxy_pass http://localhost:3000;
            proxy_http_version 1.1;

            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection "upgrade";
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;

            proxy_connect_timeout 7d;
            proxy_send_timeout 7d;
            proxy_read_timeout 7d;
            proxy_buffering off;
        }

        # 通用 WebSocket 路径 General WebSocket Path
        location /ws {
            proxy_pass http://localhost:3000;
            proxy_http_version 1.1;

            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection "upgrade";
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;

            proxy_connect_timeout 7d;
            proxy_send_timeout 7d;
            proxy_read_timeout 7d;
            proxy_buffering off;
        }

        # 反向代理配置 Reverse Proxy Configuration
        location / {
            proxy_pass http://localhost:3000;
            proxy_http_version 1.1;

            # WebSocket 支持 WebSocket Support
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection $connection_upgrade;

            # 代理头设置 Proxy Headers
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
            proxy_set_header X-Forwarded-Host $host;
            proxy_set_header X-Forwarded-Port $server_port;

            # 超时设置 Timeout Settings
            proxy_connect_timeout 60s;
            proxy_send_timeout 60s;
            proxy_read_timeout 60s;

            # 缓冲设置 Buffer Settings
            proxy_buffering off;
            proxy_request_buffering off;
        }
    }
}
SINGLECONF

    print_info "单文件配置已创建 Single-file config created"
}

# 安装 Nginx Install Nginx
install_nginx() {
    print_info "开始安装 Nginx Starting Nginx installation..."

    # 更新包列表 Update package list
    print_info "更新包列表 Updating package list..."
    apt update

    # 安装 Nginx Install Nginx
    print_info "安装 Nginx Installing Nginx..."
    apt install -y nginx

    # 检查主配置文件是否存在 Check if main config exists
    if [ ! -f /etc/nginx/nginx.conf ]; then
        print_warning "主配置文件不存在，重新安装 Nginx Main config missing, reinstalling..."
        apt reinstall -y nginx-common nginx
    fi

    # 禁用默认站点 Disable default site
    print_info "禁用默认站点 Disabling default site..."
    if [ -L /etc/nginx/sites-enabled/default ]; then
        rm /etc/nginx/sites-enabled/default
    fi

    print_info "Nginx 安装完成 Nginx installation completed"
}

# 配置 Nginx Configure Nginx
configure_nginx() {
    print_info "配置 Nginx Configuring Nginx..."

    # 创建单文件配置 Create single-file configuration
    create_single_nginx_conf

    print_info "配置已完成 Configuration completed"
}

# 测试并重载 Nginx Test and reload Nginx
reload_nginx() {
    print_info "测试 Nginx 配置 Testing Nginx configuration..."

    if nginx -t; then
        print_info "配置测试通过 Configuration test passed"
        print_info "重载 Nginx Reloading Nginx..."
        systemctl reload nginx
        print_info "Nginx 重载完成 Nginx reloaded successfully"
    else
        print_error "配置测试失败 Configuration test failed"
        exit 1
    fi
}

# 显示状态 Show status
show_status() {
    print_info "Nginx 状态 Nginx Status:"
    systemctl status nginx --no-pager

    echo ""
    print_info "监听端口 Listening Ports:"
    ss -tlnp | grep nginx || netstat -tlnp | grep nginx

    echo ""
    print_info "配置信息 Configuration Info:"
    echo "  域名 Domain: $DOMAIN"
    echo "  后端 Backend: $BACKEND_URL"
    echo "  HTTP 端口 HTTP Port: 80"
    echo "  HTTPS 端口 HTTPS Port: 443"
}

# 安装主函数 Main installation function
install() {
    print_info "=========================================="
    print_info "开始安装和配置 Nginx"
    print_info "Starting Nginx Installation and Configuration"
    print_info "=========================================="

    check_root
    check_certificates

    # 检查 Nginx 是否已安装
    # Check if Nginx is already installed
    if command -v nginx &> /dev/null; then
        print_warning "Nginx 已安装 Nginx is already installed"
        read -p "是否重新配置? Reconfigure? (y/n): " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            print_info "取消操作 Operation cancelled"
            exit 0
        fi
    else
        install_nginx
    fi

    configure_nginx
    reload_nginx

    print_info "=========================================="
    print_info "安装完成 Installation Completed!"
    print_info "=========================================="

    show_status

    echo ""
    print_info "访问地址 Access URLs:"
    echo "  https://api.pinpet.fun"
    echo "  http://api.pinpet.fun (自动重定向到 HTTPS auto-redirect to HTTPS)"
}

# 卸载函数 Uninstall function
uninstall() {
    print_info "=========================================="
    print_info "开始卸载 Nginx"
    print_info "Starting Nginx Uninstallation"
    print_info "=========================================="

    check_root

    read -p "确认卸载 Nginx? Confirm uninstall Nginx? (y/n): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        print_info "取消卸载 Uninstall cancelled"
        exit 0
    fi

    # 停止 Nginx Stop Nginx
    print_info "停止 Nginx 服务 Stopping Nginx service..."
    systemctl stop nginx || true
    systemctl disable nginx || true

    # 删除配置文件 Remove configuration files
    print_info "删除配置文件 Removing configuration files..."
    rm -f "$NGINX_ENABLED"
    rm -f "$NGINX_CONF"

    # 卸载 Nginx Uninstall Nginx
    print_info "卸载 Nginx Uninstalling Nginx..."
    apt remove -y nginx nginx-common nginx-core
    apt autoremove -y

    # 可选：删除配置目录 Optional: Remove configuration directory
    read -p "是否删除所有 Nginx 配置? Remove all Nginx configurations? (y/n): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -rf /etc/nginx
        rm -rf /var/log/nginx
        print_info "已删除所有配置 All configurations removed"
    fi

    print_info "=========================================="
    print_info "卸载完成 Uninstallation Completed!"
    print_info "=========================================="
}

# 主菜单 Main menu
show_menu() {
    echo ""
    echo "=========================================="
    echo "Nginx 安装配置脚本"
    echo "Nginx Installation Script"
    echo "=========================================="
    echo "1. 安装/配置 Nginx (Install/Configure Nginx)"
    echo "2. 卸载 Nginx (Uninstall Nginx)"
    echo "3. 查看状态 (Show Status)"
    echo "4. 重载配置 (Reload Configuration)"
    echo "5. 退出 (Exit)"
    echo "=========================================="
    read -p "请选择 Please select [1-5]: " choice

    case $choice in
        1)
            install
            ;;
        2)
            uninstall
            ;;
        3)
            check_root
            show_status
            ;;
        4)
            check_root
            reload_nginx
            show_status
            ;;
        5)
            print_info "退出 Exit"
            exit 0
            ;;
        *)
            print_error "无效选择 Invalid choice"
            show_menu
            ;;
    esac
}

# 脚本入口 Script entry point
if [ $# -eq 0 ]; then
    show_menu
else
    case "$1" in
        install)
            install
            ;;
        uninstall)
            uninstall
            ;;
        status)
            check_root
            show_status
            ;;
        reload)
            check_root
            reload_nginx
            ;;
        *)
            echo "用法 Usage: $0 {install|uninstall|status|reload}"
            echo "或直接运行显示菜单 Or run without arguments to show menu"
            exit 1
            ;;
    esac
fi
