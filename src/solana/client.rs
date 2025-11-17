// Solana客户端模块 / Solana client module
use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tracing::{debug, error, info};

/// Solana RPC客户端 / Solana RPC client
#[derive(Clone)]
pub struct SolanaClient {
    rpc_url: String,
    client: Client,
}

impl SolanaClient {
    /// 创建新的Solana客户端 / Create new Solana client
    pub fn new(rpc_url: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self { rpc_url, client })
    }

    /// 检查RPC连接 / Check RPC connection
    pub async fn check_connection(&self) -> Result<bool> {
        info!("检查Solana RPC连接 / Checking Solana RPC connection: {}", self.rpc_url);

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getHealth"
        });

        match self.client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    let body: Value = response.json().await?;
                    if body.get("result").is_some() {
                        info!("✅ Solana RPC连接正常 / Solana RPC connection is healthy");
                        Ok(true)
                    } else if let Some(error) = body.get("error") {
                        error!("Solana RPC返回错误 / Solana RPC returned error: {:?}", error);
                        Ok(false)
                    } else {
                        Ok(true)
                    }
                } else {
                    error!("Solana RPC响应状态码错误 / Solana RPC response status code error: {}", response.status());
                    Ok(false)
                }
            }
            Err(e) => {
                error!("无法连接到Solana RPC / Cannot connect to Solana RPC: {}", e);
                Ok(false)
            }
        }
    }

    /// 获取交易详情和日志 / Get transaction with logs
    pub async fn get_transaction_with_logs(&self, signature: &str) -> Result<Value> {
        debug!("获取交易详情 / Getting transaction details: {}", signature);

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTransaction",
            "params": [
                signature,
                {
                    "encoding": "json",
                    "commitment": "confirmed",
                    "maxSupportedTransactionVersion": 0
                }
            ]
        });

        let response = self.client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "RPC请求失败，状态码 / RPC request failed with status: {}",
                response.status()
            ));
        }

        let body: Value = response.json().await?;

        if let Some(error) = body.get("error") {
            return Err(anyhow::anyhow!(
                "RPC错误 / RPC error: {:?}",
                error
            ));
        }

        body.get("result")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("响应中没有result字段 / No result field in response"))
    }

    /// 获取最新区块高度 / Get slot
    pub async fn get_slot(&self) -> Result<u64> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSlot",
            "params": [{
                "commitment": "processed"
            }]
        });

        let response = self.client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await?;

        let body: Value = response.json().await?;

        body.get("result")
            .and_then(|r| r.as_u64())
            .ok_or_else(|| anyhow::anyhow!("无法获取slot / Failed to get slot"))
    }

    /// 获取程序账户 / Get program accounts
    pub async fn get_program_accounts(&self, program_id: &str) -> Result<Vec<ProgramAccount>> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getProgramAccounts",
            "params": [
                program_id,
                {
                    "encoding": "base64",
                    "commitment": "confirmed"
                }
            ]
        });

        let response = self.client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await?;

        let body: Value = response.json().await?;

        if let Some(result) = body.get("result") {
            let accounts: Vec<ProgramAccount> = serde_json::from_value(result.clone())?;
            Ok(accounts)
        } else {
            Ok(Vec::new())
        }
    }
}

/// 程序账户数据结构 / Program account data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramAccount {
    pub pubkey: String,
    pub account: AccountData,
}

/// 账户数据结构 / Account data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountData {
    pub data: Vec<String>,
    pub executable: bool,
    pub lamports: u64,
    pub owner: String,
    #[serde(rename = "rentEpoch")]
    pub rent_epoch: u64,
}