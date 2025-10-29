mod auth;
mod models;
mod rest;
mod strategy;
mod ws;

use auth::Authenticator;
use models::{BookTickerData, OrderUpdateData};
use rest::ApiClient;
use tokio::sync::{broadcast, watch};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 加载 .env 文件
    dotenv::dotenv().ok();
    println!("Environment variables loaded.");

    // 2. 初始化认证器
    let authenticator = Authenticator::new()?;
    println!("Authenticator initialized.");

    // 3. 初始化 REST API 客户端
    let api_client = ApiClient::new(authenticator.clone());
    println!("API Client initialized.");

    // 4. 创建 channels 用于 WS 和 Strategy 间通信
    // watch 用于 "最新价格"
    let (price_tx, price_rx) = watch::channel::<Option<BookTickerData>>(None);
    // broadcast 用于 "订单更新"
    let (order_tx, _) = broadcast::channel::<OrderUpdateData>(32);

    // 5. 启动 WebSocket 任务
    let ws_task = tokio::spawn(ws::run_websocket(
        authenticator,
        price_tx,
        order_tx.clone(),
    ));

    // 6. 启动策略任务
    let strategy_task = tokio::spawn(strategy::run_strategy(
        api_client,
        price_rx,
        order_tx,
    ));

    // 7. 等待任务完成
    tokio::select! {
        result = ws_task => {
            eprintln!("WebSocket task exited: {:?}", result);
        }
        result = strategy_task => {
            match result {
                Ok(Ok(_)) => println!("Strategy task completed successfully."),
                Ok(Err(e)) => eprintln!("Strategy task failed: {}", e),
                Err(e) => eprintln!("Strategy task panicked: {}", e),
            }
        }
    }

    Ok(())
}