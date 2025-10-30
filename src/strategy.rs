use crate::models::{BookTickerData, OrderRequest, OrderUpdateData};
use crate::rest::ApiClient;
use tokio::sync::{broadcast, watch};

const MARKET_SYMBOL: &str = "APT_USDC_PERP";
const ORDER_QUANTITY: &str = "100.0"; // 2 SOL

pub async fn run_strategy(
    api_client: ApiClient,
    mut price_rx: watch::Receiver<Option<BookTickerData>>,
    order_tx: broadcast::Sender<OrderUpdateData>,
) -> anyhow::Result<()> {
    
    // --- 步骤 1: 等待第一次价格更新 ---
    println!("[STRATEGY] Waiting for the first price update...");
    price_rx.changed().await?;
    
    let ticker = match *price_rx.borrow() {
        Some(ref ticker) => ticker.clone(),
        None => {
            anyhow::bail!("[STRATEGY] Price receiver dropped before first price.");
        }
    };

    println!(
        "[STRATEGY] Got initial price: Bid = {}, Ask = {}",
        ticker.best_bid_price, ticker.best_ask_price
    );

    // --- 步骤 2: 以卖一价开空单 ---
    let short_price = &ticker.best_ask_price;
    let short_order_req = OrderRequest {
        symbol: MARKET_SYMBOL,
        side: "Ask", // [cite: 2720] (卖出/开空)
        order_type: "Limit", // [cite: 2715]
        quantity: ORDER_QUANTITY,
        price: short_price,
        post_only: true, // [cite: 2720]
    };

    println!("[STRATEGY] Placing SHORT order at {}...", short_price);
    let short_order_resp = api_client.place_order(&short_order_req).await?;
    let short_order_id = short_order_resp.id.clone();
    println!("[STRATEGY] SHORT order placed. ID: {}", short_order_id);

    // --- 步骤 3: 监控空单直到完全成交 ---
    if short_order_resp.status == "Filled" {
        println!("[STRATEGY] SHORT order filled immediately.");
    } else {
        println!("[STRATEGY] Waiting for SHORT order ({}) to be filled...", short_order_id);
        let mut order_listener = order_tx.subscribe();
        
        loop {
            let update = order_listener.recv().await?;
            // 检查是否是我们的订单 [cite: 3276] 并且状态是否为 "Filled" [cite: 3276]
            if update.order_id == short_order_id && update.order_status == "Filled" {
                println!("[STRATEGY] SHORT order fill confirmed via WS.");
                break;
            }
        }
    }

    // --- 步骤 4: 以买一价开多单 ---
    // 获取最新的价格
    let current_ticker = match *price_rx.borrow() {
        Some(ref ticker) => ticker.clone(),
        None => anyhow::bail!("[STRATEGY] Price receiver dropped."),
    };
    let long_price = &current_ticker.best_bid_price;

    let long_order_req = OrderRequest {
        symbol: MARKET_SYMBOL,
        side: "Bid", // [cite: 2720] (买入/开多)
        order_type: "Limit",
        quantity: ORDER_QUANTITY,
        price: long_price,
        post_only: true,
    };

    println!("[STRATEGY] Placing LONG order at {}...", long_price);
    let long_order_resp = api_client.place_order(&long_order_req).await?;
    println!("[STRATEGY] LONG order placed. ID: {}", long_order_resp.id);
    
    println!("[STRATEGY] Strategy complete.");
    Ok(())
}