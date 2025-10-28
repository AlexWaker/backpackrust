use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey};
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{env, time::Duration};
use dotenvy;

// --- 配置 ---
const API_BASE_URL: &str = "https://api.backpack.exchange";
const SYMBOL: &str = "SOL_USDC_PERP"; // 交易对
const ORDER_QUANTITY: &str = "2.0"; // 订单数量 (2个SOL)
const POLLING_INTERVAL_SECONDS: u64 = 1; // 轮询间隔时间 (5秒)
// ---

// ----------------------------------------------------
// 结构体定义 (用于 API 响应)
// ----------------------------------------------------

// 深度 API 响应
#[derive(Debug, Deserialize)]
struct DepthResponse {
    pub asks: Vec<[String; 2]>, // [price, quantity] [cite: 514]
    pub bids: Vec<[String; 2]>, // [price, quantity] [cite: 516]
}

// 订单执行 API 响应
#[derive(Debug, Deserialize)]
struct OrderExecuteResponse {
    id: String, // 订单 ID [cite: 2798]
    status: String,
}

// 订单历史 API 响应
#[derive(Clone, Debug, Deserialize)]
struct OrderHistoryResponse {
    id: String,
    status: String, // "Filled", "Cancelled", "New", "PartiallyFilled" 等 [cite: 2259, 2299]
    symbol: String,
    side: String,
    #[serde(rename = "executedQuantity")]
    executed_quantity: String, // [cite: 2250]
    quantity: String, // [cite: 2256]
}


// ----------------------------------------------------
// 辅助函数 (签名)
// ----------------------------------------------------

/// 辅助函数：生成认证所需的 HTTP 头部 (逻辑不变)
fn generate_auth_headers(
    api_key_b64: &str,
    api_secret_b64: &str,
    instruction: &str,
    params_str: &str, // 已按字母顺序排列的查询/请求体字符串 [cite: 19]
    timestamp: i64,
    window: i64,
) -> Result<HeaderMap, Box<dyn std::error::Error>> {
    let secret_key_bytes = STANDARD.decode(api_secret_b64)?;
    let signing_key = SigningKey::from_bytes(&secret_key_bytes.try_into().map_err(|_| "私钥格式错误")?);

    // 构造签名字符串 [cite: 24, 53]
    let mut signable_string = format!("instruction={}", instruction);
    if !params_str.is_empty() {
        signable_string.push('&');
        signable_string.push_str(params_str);
    }
    signable_string.push_str(&format!("&timestamp={}&window={}", timestamp, window));

    println!("(Debug) 待签名字符串: {}", signable_string);

    let signature = signing_key.sign(signable_string.as_bytes());
    let signature_b64 = STANDARD.encode(signature.to_bytes());

    let mut headers = HeaderMap::new();
    headers.insert("X-API-Key", HeaderValue::from_str(api_key_b64)?); // [cite: 16]
    headers.insert("X-Signature", HeaderValue::from_str(&signature_b64)?); // [cite: 17]
    headers.insert("X-Timestamp", HeaderValue::from_str(&timestamp.to_string())?); // [cite: 14]
    headers.insert("X-Window", HeaderValue::from_str(&window.to_string())?); // [cite: 15]
    headers.insert("Content-Type", HeaderValue::from_static("application/json; charset=utf-8"));

    Ok(headers)
}

// ----------------------------------------------------
// 核心业务逻辑函数
// ----------------------------------------------------

/// 步骤 1 & 4: 获取市场价格 (买一/卖一)
async fn get_market_prices(client: &reqwest::Client) -> Result<(f64, f64), Box<dyn std::error::Error>> {
    println!("正在获取 {} 的市场深度...", SYMBOL);
    let url = format!("{}/api/v1/depth", API_BASE_URL); // [cite: 503]

    let response = client
        .get(url)
        .query(&[("symbol", SYMBOL)]) // [cite: 494]
        .timeout(Duration::from_secs(10))
        .send()
        .await?;

    // 按之前的实现：先检查状态码，再直接解析 JSON
    let status = response.status();
    if !status.is_success() {
        return Err(format!("获取深度失败: {}", response.text().await?).into());
    }
    let depth: DepthResponse = response.json().await?;

    // 卖一取 asks 的第一个价位；买一取 bids 的最后一个价位
    let best_ask = depth
        .asks
        .first()
        .ok_or("订单簿中没有卖单 (Ask) 数据")?
        [0]
        .parse::<f64>()?;

    let best_bid = depth
        .bids
        .last()
        .ok_or("订单簿中没有买单 (Bid) 数据")?
        [0]
        .parse::<f64>()?;
    
    println!("获取成功: 卖一价 (Ask): {}, 买一价 (Bid): {}", best_ask, best_bid);
    Ok((best_ask, best_bid))
}

/// 步骤 2 & 5: 执行订单 (开仓/平仓)
async fn execute_order(
    client: &reqwest::Client,
    api_key: &str,
    api_secret: &str,
    side: &str, // "Ask" (开空) 或 "Bid" (平仓) [cite: 2720]
    price: f64,
) -> Result<String, Box<dyn std::error::Error>> {
    println!("--- 正在执行 {} 订单 @ {:.4} (PostOnly) ---", side, price);
    
    let url = format!("{}/api/v1/order", API_BASE_URL); // [cite: 2750]
    let instruction = "orderExecute"; // [cite: 2674]
    
    // 构造订单请求体
    // (注意：已移除止盈止损字段)
    let request_body = serde_json::json!({
        "symbol": SYMBOL, // [cite: 2720]
        "side": side, // [cite: 2720]
        "orderType": "Limit", // [cite: 2715]
        "quantity": ORDER_QUANTITY, // [cite: 2720]
        "price": format!("{:.4}", price), // [cite: 2720]
        "postOnly": true // [cite: 2720]
    });
    
    let json_body = request_body.to_string();

    // 构造签名字符串 (按字母顺序) [cite: 19]
    // 字段: orderType, postOnly, price, quantity, side, symbol
    let params_str = format!(
        "orderType=Limit&postOnly=true&price={:.4}&quantity={}&side={}&symbol={}",
        price, ORDER_QUANTITY, side, SYMBOL
    );
    
    let timestamp = Utc::now().timestamp_millis();
    let window = 5000;

    let headers = generate_auth_headers(
        api_key,
        api_secret,
        instruction,
        &params_str,
        timestamp,
        window,
    )?;

    let response = client
        .post(url)
        .headers(headers)
        .body(json_body)
        .timeout(Duration::from_secs(10))
        .send()
        .await?;

    let status = response.status();
    let response_text = response.text().await?;

    if !status.is_success() {
        println!("!! 订单执行失败 (可能是 PostOnly 被拒绝): {}", status);
        return Err(format!("订单失败: {}", response_text).into());
    }
    
    println!("订单初步接受成功。");
    let order_response: OrderExecuteResponse = serde_json::from_str(&response_text)?;
    println!("订单 ID: {}", order_response.id);
    
    Ok(order_response.id)
}

/// 步骤 3: 轮询订单是否完全成交
async fn poll_order_filled(
    client: &reqwest::Client,
    api_key: &str,
    api_secret: &str,
    order_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("--- 正在轮询订单 {} (每 {} 秒) ---", order_id, POLLING_INTERVAL_SECONDS);
    
    loop {
        // 1. 等待 5 秒
        tokio::time::sleep(Duration::from_secs(POLLING_INTERVAL_SECONDS)).await;
        println!("正在查询订单 {} 的状态...", order_id);

        // 2. 准备查询订单历史
        let instruction = "orderHistoryQueryAll"; // [cite: 2179]
        let url = format!("{}/wapi/v1/history/orders", API_BASE_URL); // 
        
        // 签名字符串 (按字母顺序)
        let params_str = format!("orderId={}", order_id); // [cite: 2185]
        let timestamp = Utc::now().timestamp_millis();
        let window = 5000;

        let headers = generate_auth_headers(
            api_key,
            api_secret,
            instruction,
            &params_str,
            timestamp,
            window,
        )?;

        // 3. 发送 GET 请求
        let response = client
            .get(url)
            .query(&[("orderId", order_id)]) // [cite: 2185]
            .headers(headers)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;

        if !response.status().is_success() {
            println!("!! 查询订单历史失败: {}. 5秒后重试...", response.status());
            continue;
        }

        let history: Vec<OrderHistoryResponse> = response.json().await?;

        // 4. 检查状态
        if let Some(order) = history.first() {
            println!("当前订单状态: {}", order.status);
            
            // "Filled" 是 API 文档中代表完全成交的状态 
            if order.status == "Filled" {
                println!("*** 订单 {} 已完全成交! ({} {} @ {}) ***",
                    order.id, order.side, order.quantity, order.symbol
                );
                break Ok(());
            } else if order.status == "Cancelled" || order.status == "Expired" {
                // 如果订单因为 postOnly 被取消或失效
                return Err(format!("!! 订单 {} 状态为 {}，策略终止。", order_id, order.status).into());
            } else {
                println!("订单未成交 (状态: {}). 5秒后重试...", order.status);
            }
        } else {
            println!("!! 在历史记录中未找到订单 {}. 5秒后重试...", order_id);
        }
    }
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ---- 1. 加载环境变量并创建 Client ----
    dotenvy::dotenv().ok();
    let api_key = env::var("BP_API_KEY")
        .map_err(|_| "环境变量 BP_API_KEY 未设置。")?;
    let api_secret = env::var("BP_API_SECRET")
        .map_err(|_| "环境变量 BP_API_SECRET 未设置。")?;

    let client = reqwest::Client::new();
    println!("策略启动: {} | 数量: {}", SYMBOL, ORDER_QUANTITY);

    // ---- 2. 步骤 1: 获取价格 ----
    let (best_ask, _) = get_market_prices(&client).await?;
    
    // ---- 3. 步骤 2: 执行开空单 ----
    let short_order_id = match execute_order(&client, &api_key, &api_secret, "Ask", best_ask).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("!! 开空单失败: {}", e);
            return Err(e);
        }
    };

    // ---- 4. 步骤 3: 轮询开空单 ----
    if let Err(e) = poll_order_filled(&client, &api_key, &api_secret, &short_order_id).await {
        eprintln!("!! 轮询开空单时出错: {}", e);
        return Err(e);
    }
    
    // ---- 5. 步骤 4: 获取新价格 (用于平仓) ----
    // 停顿一小会，防止 API 速率限制
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (_, best_bid) = get_market_prices(&client).await?;

    // ---- 6. 步骤 5: 执行平仓 (开多单) ----
    match execute_order(&client, &api_key, &api_secret, "Bid", best_bid).await {
        Ok(long_order_id) => {
            println!("*** 策略执行完毕: 平仓单 {} 已提交。***", long_order_id);
        }
        Err(e) => {
            eprintln!("!! 平仓单执行失败: {}", e);
            return Err(e);
        }
    };

    Ok(())
}