# jybot-rs — Polymarket BTC 5 分钟 UP/DOWN 交易机器人（Rust）

纯 **Rust + tokio** 实现的 Polymarket 比特币 5 分钟「涨/跌」自动交易机器人。
行情/盘口判断全部走 **WebSocket 长连接、事件驱动**；REST 仅作兜底；真实下单复用官方 `py-clob-client`。

参考 [leosysd/JY_RUST](https://github.com/leosysd/JY_RUST) 的 Rust + WebSocket 架构。

- ✅ 默认 **dry-run（纸面模拟）**，绝不默认真钱下单
- ✅ Polymarket 盘口 WS + BTC 价格 WS，盘口/价格一更新立刻决策
- ✅ 动态发现 BTC 5 分钟市场（不写死 token_id）；REST 断线/过期兜底
- ✅ 固定份额限价单，FOK/FAK/GTC
- ✅ 三重安全锁：`--live` + `DRY_RUN=false` + `LIVE_TRADING=true`
- ✅ 只改 `.env`，不改代码

---

## 架构

```
                 ┌─────────────── tokio 异步运行时 ───────────────┐
 Polymarket 盘口 WS ─┐                                            │
 (market_ws.rs)      ├─► 事件驱动主循环 (main.rs: tokio::select!) │
 BTC 价格 WS ────────┘        │                                   │
 (price_ws.rs)                ▼                                   │
                        策略决策 (strategy.rs)                     │
                          │        │                              │
                   信号(signal.rs) 执行(executor.rs)              │
                                       │                          │
                          DRY-RUN 模拟撮合 / LIVE→Python 下单      │
                 └──────────────────────────────────────────────┘
   REST 兜底 (clob.rs)：WS 无数据或过期(WS_STALENESS_SEC)时回退
```

| 文件 | 职责 |
| --- | --- |
| `src/main.rs` | 入口、CLI、安全门、tokio 事件驱动主循环 |
| `src/config.rs` | 读 `.env`、安全锁 |
| `src/clob.rs` | 市场动态发现、REST 盘口兜底、结算查询、盘口缓存 |
| `src/market_ws.rs` | Polymarket 盘口 WS（订阅/增量/重连/事件通知） |
| `src/price_ws.rs` | BTC 价格 WS（Binance 镜像 aggTrade）+ REST 兜底 |
| `src/signal.rs` | 漂移+动量 → P(涨) 信号 |
| `src/strategy.rs` | 入场过滤、持仓管理、TP/SL、结算 |
| `src/executor.rs` | 固定份额限价单；DRY 模拟 / LIVE 调 Python |
| `src/state.rs` | 持仓 + 纸面记录 + 胜率/PnL |
| `src/panel.rs` | 交互式管理面板（菜单：配置/服务/统计/日志…） |
| `src/service.rs` | 后台常驻服务（启/停/重启/状态/日志，PID 文件） |
| `scripts/place_order.py` | LIVE 真实下单（官方 py-clob-client，自包含） |

---

## 安装（拉 GitHub 预编译二进制，VPS 免装 Rust / 免编译）

二进制由 **GitHub Actions 云端编译**并发布到 **Releases**。VPS 直接拉现成的，
小内存机器也不会因编译 OOM。

### 方式一：一行 curl 远程安装（最简，无需 git）

```bash
curl -fsSL https://raw.githubusercontent.com/leosysd/my-btc-bot-5min/main/get.sh | bash
```

下载预编译二进制 + `.env.example` + `place_order.py` → 生成 `.env` → 装全局命令 `jybot`。
默认装到 `~/jybot-rs`（可 `JYBOT_DIR=/opt/jybot-rs` 覆盖）。装好直接敲 **`jybot`** 进面板。

### 方式二：git clone + install.sh

```bash
git clone https://github.com/leosysd/my-btc-bot-5min.git jybot-rs
cd jybot-rs
bash install.sh
```

两种方式都**优先下载预编译二进制**（拉不到才回退装 Rust 编译），并安装全局命令 `jybot`。

> **更新**：面板选 **「12. 更新程序」** —— 优先**下载最新预编译二进制**（秒级、免编译）
> → 自动重启服务；拉不到才回退源码编译。你的 `.env` 永远不会被覆盖。

### 发布新版本（维护者）

```bash
git tag v0.1.1 && git push origin v0.1.1   # 打 tag 即触发云端编译 + 发布 Release
```
CI 配置见 `.github/workflows/release.yml`（Linux + Windows 二进制）。

---

## 系统级配置（全局唯一）

机器人的配置/状态/日志都集中在**一个固定的全局目录**（默认 `~/.jybot`，可用
环境变量 `JYBOT_HOME` 覆盖）。无论你从哪个目录、用什么方式启动，读写的都是
**同一份**——你在面板里改的就是整台机器人的唯一设置，不会因为目录不同而分裂。

```
~/.jybot/
  .env                 ← 唯一配置（面板 1/10 项编辑的就是它）
  jybot.pid            后台服务 PID
  logs/service.log     服务日志（面板第 8 项查看）
  paper_trades.json    模拟交易记录（面板第 4 项统计）
  scripts/place_order.py  实盘下单助手
  bin/jybot-rs         二进制（面板第 12 项更新时替换此文件）
```

全局命令 `jybot` 会自动锁定该目录，所以**任意目录敲 `jybot` 都是同一套配置/服务**。

---

## 手动开始

```bash
# 0) 装 Rust 工具链（一次性）: https://rustup.rs
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 1) 配置（你之后只改这个文件）
cp .env.example .env
nano .env

# 2) 编译
cargo build --release        # 产物: target/release/jybot-rs

# 3) 推荐：打开管理面板（菜单式，新手首选）
./target/release/jybot-rs                  # 无参数 = 管理面板

# 或直接用命令行：
./target/release/jybot-rs --test-mode     # 有界纸面演示（前台）
./target/release/jybot-rs --simulation    # WS 事件驱动纸面模拟（前台）
./target/release/jybot-rs check           # 检查配置
./target/release/jybot-rs stats           # 胜率 / PnL / 交易统计
# 可选: --interval 5m|15m
```

## 管理面板（菜单）

`./target/release/jybot-rs`（不带参数）打开交互式管理菜单，顶部实时显示
**服务状态 / 入场策略 / DRY_RUN 模式**：

```
============================================================
   ☰  JY Bot 管理菜单 (jybot-rs)
============================================================
  服务: 运行中 (PID 48140 / simulation)   入场: 动量   DRY_RUN: 模拟
------------------------------------------------------------
  1. 初始化 / 修改配置        7. 重启服务
  2. 查看当前配置            8. 查看实时日志
  3. 测试 API 连接          9. 切换 DRY_RUN 模式
  4. 交易统计表            10. 调策略参数
  5. 启动服务（后台常驻）   11. 清空模拟数据
  6. 停止服务              12. 更新程序（git pull + 编译）
  0. 退出
```

- 改 `.env` 前自动备份；私钥/密钥显示打码；把 `DRY_RUN=false`/`LIVE_TRADING=true`
  等危险开关打开需输入大写 `YES`。
- 「启动服务」= **后台常驻**：面板退出后机器人继续跑；可随时停止/重启/看状态。

## 后台常驻服务（命令行）

不进面板也能直接管理后台服务：

```bash
./target/release/jybot-rs start      # 后台常驻启动（按 .env 决定 模拟/实盘）
./target/release/jybot-rs status     # 运行中(PID)/已停止
./target/release/jybot-rs logs       # 查看服务日志(logs/service.log)
./target/release/jybot-rs restart    # 重启
./target/release/jybot-rs stop       # 停止
```

> 一键安装：`bash install.sh`（装 Rust、编译、生成 .env、校验配置）。

---

## 实盘

真钱下单需 **三把锁同时满足**：
1. 启动加 `--live`
2. `.env` `DRY_RUN=false`
3. `.env` `LIVE_TRADING=true`

任一不满足即纸面模拟。`--live` 还会要求终端输入大写 `YES`；安全锁未打开时 `--live` 直接退出（码 2）。

Rust 负责全部行情/盘口/策略（WebSocket）。真实提交订单时调用
`scripts/place_order.py`（官方 `py-clob-client` 完成 EIP-712 签名与提交）：

```bash
pip install py-clob-client     # 实盘才需要，装在 PYTHON_BIN 指定的解释器里
# .env 填好 5 个凭证 + DRY_RUN=false + LIVE_TRADING=true
./target/release/jybot-rs --live
```

---

## 配置（`.env`）

所有参数都在 `.env`，改完重启即可，无需改代码。完整说明见 `.env.example`。常用项：

| 变量 | 默认 | 说明 |
| --- | --- | --- |
| `MARKET_INTERVAL` | `5m` | `5m` / `15m` |
| `DRY_RUN` / `LIVE_TRADING` | `true` / `false` | 安全双锁 |
| `FIXED_SHARES` | `5` | 每单固定份额 |
| `ORDER_TYPE` | `FOK` | FOK/FAK/GTC |
| `SLIPPAGE_BPS` | `100` | 滑点(100=1%) |
| `MAX_POSITION_USDC` | `5` | 单仓名义上限 |
| `MIN_ENTRY_PRICE`/`MAX_ENTRY_PRICE` | `0.25`/`0.75` | 入场价区间 |
| `MIN_ML_EDGE` | `0.10` | 最小模型优势 |
| `TAKE_PROFIT_PCT` | `0.40` | 止盈 |
| `MARKET_WS_URL` | `wss://ws-subscriptions-clob.polymarket.com/ws/market` | 盘口 WS |
| `WS_STALENESS_SEC` | `8` | WS 过期→回退 REST |
| `PYTHON_BIN` | `python3` | 实盘下单的 Python |

---

## 部署（systemd）

```bash
cargo build --release
sudo cp jy-bot.service /etc/systemd/system/
sudo nano /etc/systemd/system/jy-bot.service   # 改 User / WorkingDirectory / ExecStart
sudo systemctl daemon-reload
sudo systemctl enable --now jy-bot
journalctl -u jy-bot -f
```

---

## 免责声明

加密货币与预测市场交易存在**重大亏损风险**。本软件仅用于学习研究，作者不对任何资金损失负责。请先用模拟模式、小额资金，仅用你能承受全部损失的资金交易。
