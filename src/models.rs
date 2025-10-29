use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- WebSocket 消息结构 ---

#[derive(Deserialize, Debug, Clone)]
pub struct WsMessage<T> {
    pub stream: String,
    pub data: T,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BookTickerData {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "a")]
    pub best_ask_price: String,
    #[serde(rename = "A")]
    pub best_ask_qty: String,
    #[serde(rename = "b")]
    pub best_bid_price: String,
    #[serde(rename = "B")]
    pub best_bid_qty: String,
    #[serde(rename = "T")]
    pub engine_timestamp: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct OrderUpdateData {
    #[serde(rename = "e")] // 事件类型 (e.g., "orderAccepted", "orderFill") [cite: 3273]
    pub event_type: String,
    #[serde(rename = "s")] // 交易对
    pub symbol: String,
    #[serde(rename = "i")] // 订单 ID [cite: 3276]
    pub order_id: String,
    #[serde(rename = "X")] // 订单状态 (e.g., "New", "Filled", "Cancelled") [cite: 3276]
    pub order_status: String,
    #[serde(rename = "l")] // 本次成交数量 (仅 "orderFill" 事件) [cite: 3277]
    pub fill_quantity: Option<String>,
    #[serde(rename = "z")] // 累计成交数量 (仅 "orderFill" 事件) [cite: 3277]
    pub executed_quantity: Option<String>,
}

// --- REST API 结构 ---

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OrderRequest<'a> {
    pub symbol: &'a str,
    pub side: &'a str,
    pub order_type: &'a str,
    pub quantity: &'a str,
    pub price: &'a str,
    pub post_only: bool,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OrderResponse {
    pub id: String,
    pub status: String,
    pub symbol: String,
    pub side: String,
    pub quantity: String,
    pub executed_quantity: String,
}

// --- 错误处理 ---

#[derive(Debug, thiserror::Error)]
pub enum BotError {
    #[error("WebSocket error: {0}")]
    WsError(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("REST API error: {0}")]
    RestError(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Auth error: {0}")]
    AuthError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Env var error: {0}")]
    EnvError(#[from] std::env::VarError),
    #[error("Strategy error: {0}")]
    StrategyError(String),
}


//... (文件末尾)

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAccountRequest<'a> {
    pub leverage_limit: &'a str, // [cite: 859]
}