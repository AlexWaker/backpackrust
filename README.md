## backpackrust

一个使用 Rust 编写的 Backpack Exchange 示例交易机器人/客户端，用于backpack刷分：
- 一个非常简单的网格策略，很鲁棒，纯刷分就够用，一天24小时连轴转不断线无压力
- 几秒钟（时间可以自己设置）就在买一卖一同时挂空和多，最多180条open orders（交易所限制是200条，这个可以自己设置）
- 根根据我亏钱经验：刷大交易对（BTC SOL这种）基本能做到不亏钱，甚至能赚回手续费。刷小币（高波动）亏得很惨！我刷APT和SOL能赚点，但是刷ZRO亏死了。
- 建议用日本服务器运行，一是防止出现网络问题，二是backpack服务器在日本，用日本服务器可以减少地理延时

Backpack交易费率：
![手续费](./img/4807241eed41c00df501defbc287b36e.jpg)


警告：该仓库仅用于刷分，请在测试环境或极小仓位下使用。策略逻辑非常基础，不构成任何投资建议。

## 安装与构建

请先安装Rust最新版

构建：

```bash
cargo build
```

运行：

```bash
cargo run
```

首次运行前请先准备 `.env`。

## 配置（.env）

参数基本上都在配置里，env.example里很详细，可以照着配置

请去交易所注册API：https://support.backpack.exchange/support-docs/cn/jiao-yi-suo-1/zhang-hu-gong-neng/sheng-cheng-backpack-jiao-yi-suo-api-mi-yao

