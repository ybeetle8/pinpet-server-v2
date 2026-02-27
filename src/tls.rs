use anyhow::{Context, Result};
use rustls_pemfile::{certs, pkcs8_private_keys};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use tokio_rustls::rustls::{Certificate, PrivateKey, ServerConfig};

/// 加载 TLS 配置 / Load TLS configuration
///
/// 从指定的证书和私钥文件加载 TLS 配置
/// Load TLS configuration from specified certificate and private key files
pub fn load_tls_config(cert_path: &str, key_path: &str) -> Result<Arc<ServerConfig>> {
    // 检查文件是否存在 / Check if files exist
    if !Path::new(cert_path).exists() {
        anyhow::bail!("证书文件不存在 / Certificate file not found: {}", cert_path);
    }

    if !Path::new(key_path).exists() {
        anyhow::bail!("私钥文件不存在 / Private key file not found: {}", key_path);
    }

    // 加载证书 / Load certificates
    let cert_file = File::open(cert_path)
        .context(format!("无法打开证书文件 / Failed to open certificate file: {}", cert_path))?;
    let mut cert_reader = BufReader::new(cert_file);
    let cert_chain: Vec<Certificate> = certs(&mut cert_reader)
        .context("解析证书失败 / Failed to parse certificates")?
        .into_iter()
        .map(Certificate)
        .collect();

    if cert_chain.is_empty() {
        anyhow::bail!("证书文件为空 / Certificate file is empty");
    }

    // 加载私钥 / Load private key
    let key_file = File::open(key_path)
        .context(format!("无法打开私钥文件 / Failed to open private key file: {}", key_path))?;
    let mut key_reader = BufReader::new(key_file);
    let mut keys = pkcs8_private_keys(&mut key_reader)
        .context("解析私钥失败 / Failed to parse private key")?;

    if keys.is_empty() {
        anyhow::bail!("私钥文件为空或格式不正确 / Private key file is empty or invalid format");
    }

    let private_key = PrivateKey(keys.remove(0));

    // 创建 TLS 配置 / Create TLS configuration
    let config = ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .context("创建 TLS 配置失败 / Failed to create TLS configuration")?;

    tracing::info!("✅ TLS 配置加载成功 / TLS configuration loaded successfully");
    tracing::info!("   证书路径 / Certificate: {}", cert_path);
    tracing::info!("   私钥路径 / Private key: {}", key_path);

    Ok(Arc::new(config))
}
