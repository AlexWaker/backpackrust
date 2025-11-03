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

    // 设置账户杠杆为固定值 1.0（硬编码，不从 .env 读取）
    println!("Setting account leverage to 1.0...");
    if let Err(e) = api_client.set_leverage("1.0").await {
        eprintln!("CRITICAL: Failed to set leverage to 1.0x: {}. Exiting.", e);
        // 如果无法设置杠杆，程序应该退出以保证安全
        return Err(e.into());
    }
    println!("Account leverage successfully set to 1.0.");
// 4. 创建 channels 用于 WS 和 Strategy 间通信
    let (price_tx, price_rx) = watch::channel::<Option<BookTickerData>>(None);
    let (order_tx, _) = broadcast::channel::<OrderUpdateData>(32);

    // --- ↓↓↓ 这是你需要的修复 ↓↓↓ ---
    // 创建一个虚拟接收者，以防止通道在没有监听者时关闭。
    // 只要 main 函数在 select! 处阻塞，_dummy_order_rx 就会保持存活。
    let _dummy_order_rx = order_tx.subscribe();
    // --- ↑↑↑ 修复结束 ↑↑↑ ---

    // 5. 启动 WebSocket 任务
    let ws_task = tokio::spawn(ws::run_websocket(
        authenticator,
        price_tx,
        order_tx.clone(), // ws_task 获得一个 sender
    ));

    // 6. 启动策略任务
    let strategy_task = tokio::spawn(strategy::run_strategy(
        api_client,
        price_rx,
        order_tx, // strategy_task 获得另一个 sender (用于 subscribe)
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