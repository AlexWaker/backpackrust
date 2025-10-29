use crate::models::{BotError, OrderRequest, OrderResponse, UpdateAccountRequest};
use crate::auth::Authenticator;
use reqwest::Client;

const API_BASE_URL: &str = "https://api.backpack.exchange"; // [cite: 8]

#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    authenticator: Authenticator,
}

impl ApiClient {
    pub fn new(authenticator: Authenticator) -> Self {
        Self {
            client: Client::new(),
            authenticator,
        }
    }

    pub async fn place_order(&self, order: &OrderRequest<'_>) -> Result<OrderResponse, BotError> {
        let instruction = "orderExecute"; // [cite: 2674]
        let endpoint = "/api/v1/order"; // [cite: 2750]
        let url = format!("{}{}", API_BASE_URL, endpoint);

        // 序列化 body 以用于签名和请求
        let body_str = serde_json::to_string(order)?;

        // 生成签名头
        let headers = self
            .authenticator
            .generate_rest_headers(instruction, &body_str)?;

        // 发送请求
        let response = self
            .client
            .post(&url)
            .headers(headers)
            .body(body_str)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(BotError::StrategyError(format!(
                "Order placement failed: {}",
                error_text
            )));
        }

        let order_response: OrderResponse = response.json().await?;
        Ok(order_response)
    }

    pub async fn set_leverage(&self, leverage: &str) -> Result<(), BotError> {
        let instruction = "accountUpdate"; // [cite: 841]
        let endpoint = "/api/v1/account"; // [cite: 876]
        let url = format!("{}{}", API_BASE_URL, endpoint);

        let request_body = UpdateAccountRequest {
            leverage_limit: leverage, // [cite: 859, 866]
        };
        
        let body_str = serde_json::to_string(&request_body)?;

        let headers = self
            .authenticator
            .generate_rest_headers(instruction, &body_str)?;

        // 注意: 这是 PATCH 请求 [cite: 876]
        let response = self
            .client
            .patch(&url)
            .headers(headers)
            .body(body_str)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(BotError::StrategyError(format!(
                "Failed to set leverage: {}",
                error_text
            )));
        }

        Ok(())
    }
}