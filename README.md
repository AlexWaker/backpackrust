## backpackrust

一个使用 Rust 编写的 Backpack Exchange 示例交易机器人/客户端，用于backpack刷分：
- 该程序用于且仅用于Backpack刷分！本策略几乎不会有任何盈利！我也不相信散户个人量化能长期赚到钱！
- 逻辑非常简单：以当前卖一价格开空，等订单完成立刻以买一价格开多单，基本上都会成交
- 为啥这么简单的策略也写脚本？你试了就知道了，量化开单挂单的速度远远超过人手，而且对行情的反应也更快

当前默认交易标的为 `SOL_USDC_PERP`，程序会：
1) 启动即将账户杠杆设置为 1.0（失败则退出）；
2) 订阅 `bookTicker.SOL_USDC_PERP` 与私有 `account.orderUpdate`；
3) 等到第一条行情后，以卖一价下「限价 PostOnly 空单」2 SOL；
4) 等该空单完全成交后，再以买一价下「限价 PostOnly 多单」2 SOL；
5) 打印关键日志并结束。

警告：该仓库仅用于学习与集成示例，请在测试环境或极小仓位下使用。策略逻辑非常基础，不构成任何投资建议。

## 目录结构

```
backpackrust/
	├─ Cargo.toml            # 依赖与编译配置（edition=2024）
	└─ src/
		 ├─ main.rs            # 程序入口：加载环境、初始化组件、并发启动 WS 和策略
		 ├─ auth.rs            # Ed25519 签名；生成 REST/WS 所需签名与头部
		 ├─ rest.rs            # REST 客户端：设置杠杆、下单
		 ├─ ws.rs              # WebSocket 客户端：订阅 bookTicker 与 account.orderUpdate
		 ├─ models.rs          # 数据结构与错误类型
		 └─ strategy.rs        # 简单示例策略：先开空再开多
```

## 环境要求

- Rust 工具链（建议使用最新 stable，支持 Rust 2024 edition）
- Linux 运行时依赖（默认使用 OpenSSL）：
	- Debian/Ubuntu：需要 `pkg-config` 与 `libssl-dev`

如果你不想安装系统 OpenSSL，可改用 rustls，见下方「不使用 OpenSSL 的可选方案」。

## 安装与构建

在 Debian/Ubuntu 上，先安装依赖：

```bash
sudo apt-get update
sudo apt-get install -y pkg-config libssl-dev
```

构建：

```bash
cargo build
```

运行：

```bash
cargo run
```

首次运行前请先准备 `.env`（见下章）。

### 不使用 OpenSSL 的可选方案（rustls）

如果希望避免系统 OpenSSL 依赖，可在 `Cargo.toml` 中改为 rustls：

```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
tokio-tungstenite = { version = "0.23", features = ["rustls-tls-native-roots"] }
```

然后重新 `cargo build` 即可。

## 配置（.env）

项目使用以下环境变量（在根目录创建 `.env` 文件）：

```dotenv
# Base64 编码的公钥（交易所提供的 API Key）
BP_API_KEY=你的API_KEY_BASE64

# Base64 编码的 32 字节 Ed25519 私钥（交易所提供的 API Secret）
# 注意：必须是 32 字节原始私钥的 Base64 编码
BP_API_SECRET=你的API_SECRET_BASE64
```

加载逻辑位于 `main.rs` 中：程序启动会自动 `dotenv::dotenv().ok()`。

## 运行流程与策略说明

- 程序启动：
	- 加载环境变量，初始化 `Authenticator` 与 `ApiClient`；
	- 通过 REST 将账户杠杆设置为 1.0（`PATCH /api/v1/account`），失败则直接退出；
	- 并发启动 WS 订阅与策略任务，通过 `watch`/`broadcast` 通道通信。

- WebSocket：
	- 连接 `wss://ws.backpack.exchange/`；
	- 订阅公共频道：`bookTicker.SOL_USDC_PERP`；
	- 订阅私有频道：`account.orderUpdate`，需要签名参数 `[apiKeyB64, signatureB64, timestamp, window]`；
	- 将最新的 `bookTicker` 推送写入 `watch` 通道，将订单变更广播到 `broadcast` 通道。

- 策略任务（示例）：
	- 等第一条 `bookTicker` 到达；
	- 以卖一价下空单（限价、PostOnly、数量 `2.0`）；
	- 若未立即完全成交，则通过 `orderUpdate` 监听直到状态为 `Filled`；
	- 再以买一价下多单（限价、PostOnly、数量 `2.0`），打印订单 ID 后结束。

提示：默认交易标的常量 `MARKET_SYMBOL` 在 `ws.rs` 与 `strategy.rs` 中各定义了一份（均为 `SOL_USDC_PERP`）。若需更改标的，请在两处同步修改。

## REST/WS 交互要点

### REST

- Base URL: `https://api.backpack.exchange`
- 设置杠杆（`PATCH /api/v1/account`）：
	- instruction: `accountUpdate`
	- Body（JSON）：`{ "leverageLimit": "1.0" }`

- 下单（`POST /api/v1/order`）：
	- instruction: `orderExecute`
	- Body（JSON）：
		```json
		{
			"symbol": "SOL_USDC_PERP",
			"side": "Bid" | "Ask",
			"orderType": "Limit",
			"quantity": "2.0",
			"price": "价格字符串",
			"postOnly": true
		}
		```

- 签名头（`auth.rs::generate_rest_headers`）：
	- 计算字符串：`instruction=...&<body_kv_sorted>&timestamp=...&window=...`
	- 使用 Ed25519 对上述字符串签名，Base64 编码为 `X-Signature`
	- 附带头：`X-API-Key`, `X-Timestamp`, `X-Window`, `Content-Type`

### WebSocket

- URL: `wss://ws.backpack.exchange/`
- 公共订阅：`{"method":"SUBSCRIBE","params":["bookTicker.SOL_USDC_PERP"]}`
- 私有订阅：`{"method":"SUBSCRIBE","params":["account.orderUpdate"],"signature":[apiKeyB64, signatureB64, timestamp, window]}`
- WS 签名字符串：`instruction=subscribe&timestamp=...&window=...`

## 常见问题（FAQ）

- 报错「OpenSSL not found / cannot find -lssl」
	- 请先安装 `pkg-config libssl-dev`，或改用上文的 rustls 方案。

- 认证失败（401/403）或签名不通过
	- 请检查 `.env` 中的 `BP_API_KEY/BP_API_SECRET` 是否为 Base64 编码；
	- `BP_API_SECRET` 必须是 32 字节 Ed25519 私钥的 Base64 编码；
	- 机器时间必须准确（`X-Timestamp` 与 `X-Window` 受限）。

- 订阅不到私有订单流
	- 请确认 WS 订阅时携带的 `signature` 数组顺序与内容正确；
	- 确认账户、API 权限与环境（实盘/测试）一致。

## 开发与调试

- 日志：当前主要使用 `println!` 输出，你可以引入 `tracing` 进行结构化日志。
- 并发：WS 与策略分别在 Tokio 任务中运行，通过 `watch` 与 `broadcast` 传递数据。
- 代码入口：`src/main.rs`

## 许可

本仓库未包含 LICENSE 文件，如需开源发布请补充相应许可证。

## 免责声明

本项目仅为示例代码，不保证适用于任何实盘环境。数字资产交易存在高风险，请自行评估并承担后果。
