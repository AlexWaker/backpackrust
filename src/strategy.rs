use crate::models::{BookTickerData, OrderRequest, OrderUpdateData, PositionResponse}; // [修改] 导入 PositionResponse
use crate::rest::ApiClient;
use std::time::Duration;
use tokio::sync::{broadcast, watch};

// 策略超参数（硬编码）
const MARKET_SYMBOL: &str = "APT_USDC_PERP";
const ORDER_QUANTITY: &str = "100.0";
const SHORT_ORDER_TIMEOUT_SECS: u64 = 5; // 卖一挂空 5 秒超时
const LONG_ORDER_TIMEOUT_SECS: u64 = 5;  // 买一挂多 5 秒超时
const SIDE_INTERVAL_SECS: u64 = 120;     // 两方向间等待 120 秒

/// 一个辅助函数，用于等待并获取一次有效的价格更新
async fn wait_for_price(
    price_rx: &mut watch::Receiver<Option<BookTickerData>>,
) -> anyhow::Result<BookTickerData> {
    loop {
        price_rx.changed().await?;
        if let Some(ref ticker) = *price_rx.borrow() {
            return Ok(ticker.clone());
        }
    }
}

/// 判断数量字符串是否为非零（兼容 "0", "0.0", "0.000000" 等表示）
fn is_nonzero_qty(q: &str) -> bool {
    match q.trim().parse::<f64>() {
        Ok(v) => v.abs() > 1e-12,
        Err(_) => false,
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum FillStatus {
    Pending,
    PartiallyFilled,
    FullyFilled,
}

pub async fn run_strategy(
    api_client: ApiClient,
    mut price_rx: watch::Receiver<Option<BookTickerData>>,
    order_tx: broadcast::Sender<OrderUpdateData>,
) -> anyhow::Result<()> {
    println!("[STRATEGY] Waiting for the first price update...");
    wait_for_price(&mut price_rx).await?;
    println!("[STRATEGY] Got initial price. Starting main strategy loop.");

    loop {
        // ##################################################################
        // #                       1. 空单阶段 (开仓)
        // ##################################################################
        println!("[STRATEGY] --- Begin SHORT cycle ---");

        let mut short_order_filled = false;
        while !short_order_filled {
            let ticker = wait_for_price(&mut price_rx).await?;
            let short_price = &ticker.best_ask_price;
            let short_order_req = OrderRequest {
                symbol: MARKET_SYMBOL,
                side: "Ask",
                order_type: "Limit",
                quantity: ORDER_QUANTITY,
                price: short_price,
                post_only: true,
                reduce_only: None, // 开仓
            };

            println!("[STRATEGY] Placing SHORT order at {}...", short_price);
            let short_order_resp = api_client.place_order(&short_order_req).await?;
            let short_order_id = short_order_resp.id.clone();

            if short_order_resp.status == "Filled" {
                println!("[STRATEGY] SHORT order ({}) filled immediately.", short_order_id);
                short_order_filled = true;
                continue; 
            }

            println!(
                "[STRATEGY] Waiting for SHORT order ({}) to fill ({}s timeout rule)...",
                short_order_id, SHORT_ORDER_TIMEOUT_SECS
            );
            
            let mut order_listener = order_tx.subscribe();
            let mut status = FillStatus::Pending;
            let timer = tokio::time::sleep(Duration::from_secs(SHORT_ORDER_TIMEOUT_SECS));
            tokio::pin!(timer);

            let mut break_select_loop = false;
            while !break_select_loop {
                tokio::select! {
                    biased; 
                    Ok(update) = order_listener.recv() => {
                        if update.order_id != short_order_id { continue; }
                        if update.order_status == "Filled" {
                            status = FillStatus::FullyFilled;
                            break_select_loop = true;
                        } 
                        else if update.order_status == "PartiallyFilled" {
                            status = FillStatus::PartiallyFilled;
                        }
                    }
                    _ = &mut timer, if !timer.is_elapsed() => {
                        break_select_loop = true; // 5秒到了，跳出 select
                    }
                }
            } // 结束 select 循环

            // --- [空单开仓逻辑] (您的策略) ---
            match status {
                FillStatus::FullyFilled => {
                    println!("[STRATEGY] SHORT Order fully filled. Proceeding to 120s wait.");
                    short_order_filled = true; // 退出 while 循环
                }
                FillStatus::PartiallyFilled => {
                    println!("[STRATEGY] SHORT Order partially filled. Cancelling remainder and proceeding.");
                    if let Err(e) = api_client.cancel_order(&short_order_id, MARKET_SYMBOL).await {
                        eprintln!("[STRATEGY] Failed to cancel partial SHORT order: {}. Proceeding anyway...", e);
                    }
                    short_order_filled = true; // 退出 while 循环
                }
                FillStatus::Pending => {
                    println!("[STRATEGY] SHORT Order pending. Cancelling and re-placing.");
                    if let Err(e) = api_client.cancel_order(&short_order_id, MARKET_SYMBOL).await {
                        eprintln!("[STRATEGY] Failed to cancel pending SHORT order: {}. Will retry placement.", e);
                    }
                    // `short_order_filled` 保持 false, "while" 循环将继续
                }
            }
        } // 结束空单循环 (while !short_order_filled)

        // --- 等待 120 秒 ---
        println!(
            "[STRATEGY] SHORT phase complete. Sleeping {}s before LONG (Close)...",
            SIDE_INTERVAL_SECS
        );
        tokio::time::sleep(Duration::from_secs(SIDE_INTERVAL_SECS)).await;

        // ##################################################################
        // #                  2. 多单 (平仓) 阶段
        // ##################################################################
        println!("[STRATEGY] --- Begin LONG (Close Short) cycle ---");

        let mut long_order_closed = false;
        while !long_order_closed { 
            // 先查询当前是否仍有空头仓位，并获取需要平掉的数量
            let mut close_qty: Option<String> = None;
            match api_client.get_positions().await {
                Ok(positions) => {
                    if let Some(pos) = positions.iter().find(|p| p.symbol == MARKET_SYMBOL && p.side == "Ask") {
                        if is_nonzero_qty(&pos.quantity) {
                            close_qty = Some(pos.quantity.clone());
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[STRATEGY] Warning: failed to query positions before close: {}. Fallback to configured ORDER_QUANTITY.", e);
                    close_qty = Some(ORDER_QUANTITY.to_string());
                }
            }

            // 如果没有空头仓位可平，直接结束本次平仓阶段
            let Some(close_qty_str) = close_qty else {
                println!("[STRATEGY] No short position detected. Close phase is a no-op.");
                long_order_closed = true;
                continue;
            };

            let ticker = wait_for_price(&mut price_rx).await?;
            let long_price = &ticker.best_bid_price;
            // 使用实际空头仓位数量进行 reduce_only 平仓，避免因超量被交易所拒绝
            let long_order_req = OrderRequest {
                symbol: MARKET_SYMBOL,
                side: "Bid",
                order_type: "Limit",
                quantity: &close_qty_str,
                price: long_price,
                post_only: true,
                reduce_only: Some(true), // [关键] 平仓
            };

            println!("[STRATEGY] Placing LONG (Close) order at {}...", long_price);
            let long_order_resp = api_client.place_order(&long_order_req).await?;
            let long_order_id = long_order_resp.id.clone();

            if long_order_resp.status == "Filled" {
                println!("[STRATEGY] LONG (Close) order ({}) filled immediately.", long_order_id);
                long_order_closed = true;
                continue; 
            }

            println!(
                "[STRATEGY] Waiting for LONG (Close) order ({}) to fill ({}s timeout rule)...",
                long_order_id, LONG_ORDER_TIMEOUT_SECS
            );

            let mut order_listener = order_tx.subscribe();
            let mut status = FillStatus::Pending;
            let timer = tokio::time::sleep(Duration::from_secs(LONG_ORDER_TIMEOUT_SECS));
            tokio::pin!(timer);

            let mut break_select_loop = false;
            while !break_select_loop {
                tokio::select! {
                    biased;
                    Ok(update) = order_listener.recv() => {
                        if update.order_id != long_order_id { continue; }
                        if update.order_status == "Filled" {
                            status = FillStatus::FullyFilled;
                            break_select_loop = true;
                        } 
                        else if update.order_status == "PartiallyFilled" {
                            status = FillStatus::PartiallyFilled;
                        }
                    }
                    _ = &mut timer, if !timer.is_elapsed() => {
                        break_select_loop = true; // 5秒到了，跳出 select
                    }
                }
            } // 结束 select 循环

            // --- [!!! 您的全新平仓逻辑 !!!] ---
            match status {
                FillStatus::FullyFilled => {
                    // 情况1: 5秒内或5秒时，完全成交
                    println!("[STRATEGY] LONG (Close) Order fully filled.");
                    long_order_closed = true; // 退出 "while !long_order_closed" 循环
                }
                
                FillStatus::PartiallyFilled => {
                    // 情况2: 5秒到时，部分成交
                    println!("[STRATEGY] LONG (Close) Order partially filled. Cancelling remainder and CHECKING POSITION...");
                    
                    if let Err(e) = api_client.cancel_order(&long_order_id, MARKET_SYMBOL).await {
                         eprintln!("[STRATEGY] Failed to cancel partial LONG order: {}. Checking position anyway...", e);
                    }
                    
                    // [核心新逻辑] 检查仓位
                    match api_client.get_positions().await {
                        Ok(positions) => {
                            // 检查是否仍然持有此交易对的空单
                            let has_short = positions.iter().any(|p| {
                                p.symbol == MARKET_SYMBOL && p.side == "Ask" && is_nonzero_qty(&p.quantity)
                            });

                            if has_short {
                                // 2a: "如果存在" (空单还在)
                                println!("[STRATEGY] Short position still exists. Re-placing close order.");
                                // `long_order_closed` 保持 false, "while" 循环将继续
                            } else {
                                // 2b: "如果不存在" (仓位是 "Bid", "0.0", 或未找到)
                                println!("[STRATEGY] Position is flat. Close task is complete.");
                                long_order_closed = true; // 退出 "while" 循环
                            }
                        }
                        Err(e) => {
                            // 无法获取仓位，停止策略
                            anyhow::bail!("CRITICAL: Failed to get positions: {}. Stopping strategy.", e);
                        }
                    }
                }

                FillStatus::Pending => {
                    // 情况3: 5秒到时，完全未成交
                    println!("[STRATEGY] LONG (Close) Order pending. Cancelling and re-placing.");
                    if let Err(e) = api_client.cancel_order(&long_order_id, MARKET_SYMBOL).await {
                        eprintln!("[STRATEGY] Failed to cancel pending LONG order: {}. Will retry placement.", e);
                    }
                    // `long_order_closed` 保持 false, "while" 循环将继续
                }
            }
        } // 结束多单循环 (while !long_order_closed)

        // --- 平仓完成，等待 120 秒 ---
        println!(
            "[STRATEGY] LONG (Close) phase complete. Sleeping {}s before next SHORT...",
            SIDE_INTERVAL_SECS
        );
        tokio::time::sleep(Duration::from_secs(SIDE_INTERVAL_SECS)).await;
    } // 结束主循环 (loop)
}