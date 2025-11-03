use crate::models::{BotError, OrderRequest, OrderResponse, UpdateAccountRequest, CancelRequest, PositionResponse};
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

pub async fn get_positions(&self) -> Result<Vec<PositionResponse>, BotError> {
        let instruction = "positionQueryAll"; // "positionQueryAll" 用于获取所有仓位
        let endpoint = "/api/v1/positions";   // 端点是 /api/v1/positions
        let url = format!("{}{}", API_BASE_URL, endpoint);

        // [关键] GET 请求没有 body，但签名需要一个 "body"
        // 对于没有参数的 GET 请求，我们签名一个空字符串 ""
        let body_str = "";

        // 生成签名头
        let headers = self
            .authenticator
            .generate_rest_headers(instruction, &body_str)?;

        // 发送 GET 请求
        let response = self
            .client
            .get(&url) // 这是一个 GET 请求
            .headers(headers)
            // .body() -- GET 请求没有 body
            .send()
            .await?;

        // 错误处理（与您现有的代码保持一致）
        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(BotError::StrategyError(format!(
                "Failed to get positions: {}", // 自定义错误信息
                error_text
            )));
        }

        // 解析返回的仓位数组
        let positions_response: Vec<PositionResponse> = response.json().await?;
        Ok(positions_response)
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
pub async fn cancel_order(
        &self,
        order_id: &str,
        symbol: &str,
    ) -> Result<OrderResponse, BotError> {
        let instruction = "orderCancel"; // [cite: 2834]
        let endpoint = "/api/v1/order"; // [cite: 2876]
        let url = format!("{}{}", API_BASE_URL, endpoint);

        let request_body = CancelRequest { symbol, order_id };
        let body_str = serde_json::to_string(&request_body)?;

        let headers = self
            .authenticator
            .generate_rest_headers(instruction, &body_str)?;

        // 根据文档, 撤单是一个 DELETE 请求，但它仍然需要一个 body [cite: 2854, 2876]
        let response = self
            .client
            .delete(&url)
            .headers(headers)
            .body(body_str)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            // 如果订单已经被成交或不存在，交易所可能会报错，我们需要优雅地处理
            // 但在这里，我们暂时先将其视为一个通用错误
            return Err(BotError::StrategyError(format!(
                "Failed to cancel order: {}",
                error_text
            )));
        }

        let order_response: OrderResponse = response.json().await?;
        Ok(order_response)
    }
}