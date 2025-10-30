use crate::models::{BookTickerData, BotError, OrderUpdateData, WsMessage};
use crate::auth::Authenticator;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, watch};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

const WS_URL: &str = "wss://ws.backpack.exchange/"; // [cite: 8]
const MARKET_SYMBOL: &str = "APT_USDC_PERP";

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub async fn run_websocket(
    authenticator: Authenticator,
    price_tx: watch::Sender<Option<BookTickerData>>,
    order_tx: broadcast::Sender<OrderUpdateData>,
) -> anyhow::Result<()> {
    println!("Connecting to WebSocket at {}...", WS_URL);
    let (mut ws_stream, _) = connect_async(WS_URL).await?;
    println!("WebSocket connected.");

    // 1. 订阅公共市场数据
    let ticker_subscription = json!({
        "method": "SUBSCRIBE",
        "params": [format!("bookTicker.{}", MARKET_SYMBOL)] // 
    });
    ws_stream
        .send(ticker_subscription.to_string().into())
        .await?;
    println!("Subscribed to {} book ticker.", MARKET_SYMBOL);

    // 2. 订阅私有订单更新
    let signature = authenticator.generate_ws_signature()?;
    let order_subscription = json!({
        "method": "SUBSCRIBE",
        "params": ["account.orderUpdate"], // 
        "signature": signature // [cite: 3271, 3272]
    });
    ws_stream
        .send(order_subscription.to_string().into())
        .await?;
    println!("Subscribed to account order updates.");

    // 3. 消息处理循环
    while let Some(msg) = ws_stream.next().await {
        let msg = match msg {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("WebSocket error: {}", e);
                break;
            }
        };

        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            // 尝试解析 BookTicker
            if let Ok(ticker_msg) = serde_json::from_str::<WsMessage<BookTickerData>>(&text) {
                if ticker_msg.stream.starts_with("bookTicker") {
                    // 发送最新价格
                    price_tx.send(Some(ticker_msg.data))?;
                }
            }
            // 尝试解析 OrderUpdate
            else if let Ok(order_msg) = serde_json::from_str::<WsMessage<OrderUpdateData>>(&text) {
                if order_msg.stream.starts_with("account.orderUpdate") {
                    // 广播订单更新
                    order_tx.send(order_msg.data)?;
                }
            } else {
                // println!("Received unhandled WS message: {}", text);
            }
        }
    }

    Ok(())
}