use crate::models::BotError;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey};
use std::collections::BTreeMap;
use std::time::Duration;

const DEFAULT_WINDOW: i64 = 5000; // 默认 5000ms 窗口 [cite: 15]

#[derive(Clone)]
pub struct Authenticator {
    api_key: String, // Base64 编码的公钥
    signing_key: SigningKey,
}

impl Authenticator {
    pub fn new() -> Result<Self, BotError> {
        let api_key = std::env::var("BP_API_KEY")?;
        let api_secret = std::env::var("BP_API_SECRET")?;

        // 解码 Base64 编码的私钥
        let secret_bytes = BASE64
            .decode(api_secret)
            .map_err(|e| BotError::AuthError(format!("Failed to decode API secret: {}", e)))?;

        // 从 32 字节私钥创建 SigningKey
        let signing_key = SigningKey::from_bytes(
            secret_bytes.as_slice().try_into().map_err(|e| {
                BotError::AuthError(format!("Invalid private key length: {}", e))
            })?,
        );

        Ok(Self {
            api_key,
            signing_key,
        })
    }

    fn get_timestamp() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

pub fn generate_rest_headers(
        &self,
        instruction: &str,
        body: &str,
    ) -> Result<reqwest::header::HeaderMap, BotError> {
        let timestamp = Self::get_timestamp();
        let window = DEFAULT_WINDOW;

        // 1. 将 body JSON 字符串解析为 BTreeMap 以自动按字母排序
        let body_params: BTreeMap<String, serde_json::Value> = serde_json::from_str(body)?;
        
        // 2. 将 BTreeMap 转换为 query 字符串
        //    *** 这是修复错误的关键部分 ***
        let body_query: String = body_params
            .into_iter()
            .map(|(k, v)| {
                // 正确处理不同
                let value_str = match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Bool(b) => b.to_string(), // "true" 或 "false"
                    serde_json::Value::Number(n) => n.to_string(),
                    _ => "".to_string(), // 忽略 Null, Array, Object
                };
                format!("{}={}", k, value_str)
            })
            .collect::<Vec<_>>()
            .join("&");
            
        // 3. 构建完整的可签名负载
        let signable_payload = format!(
            "instruction={}&{}&timestamp={}&window={}",
            instruction, body_query, timestamp, window
        );

        // 4. 签名
        let signature: Signature = self.signing_key.sign(signable_payload.as_bytes());

        // 5. Base64 编码签名
        let signature_b64 = BASE64.encode(signature.to_bytes());

        // 6. 创建 Headers
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "X-API-Key",
            reqwest::header::HeaderValue::from_str(&self.api_key)
                .map_err(|e| BotError::AuthError(e.to_string()))?,
        );
        headers.insert(
            "X-Timestamp",
            reqwest::header::HeaderValue::from(timestamp),
        );
        headers.insert(
            "X-Window", 
            reqwest::header::HeaderValue::from(window)
        );
        headers.insert(
            "X-Signature",
            reqwest::header::HeaderValue::from_str(&signature_b64)
                .map_err(|e| BotError::AuthError(e.to_string()))?,
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json; charset=utf-8"),
        );

        Ok(headers)
    }

    /// 为 WebSocket 订阅生成签名
    pub fn generate_ws_signature(&self) -> Result<Vec<String>, BotError> {
        let timestamp = Self::get_timestamp();
        let window = DEFAULT_WINDOW;

        // WS 订阅的指令是 "subscribe" [cite: 199]
        let signable_payload = format!(
            "instruction=subscribe&timestamp={}&window={}",
            timestamp, window
        );

        // 签名
        let signature: Signature = self.signing_key.sign(signable_payload.as_bytes());
        let signature_b64 = BASE64.encode(signature.to_bytes());

        // 格式: ["<api_key_b64>", "<signature_b64>", "<timestamp_str>", "<window_str>"] [cite: 3272]
        Ok(vec![
            self.api_key.clone(),
            signature_b64,
            timestamp.to_string(),
            window.to_string(),
        ])
    }
}