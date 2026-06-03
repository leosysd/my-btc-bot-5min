//! panel.rs — 交互式管理面板（仿「JY Bot 管理菜单」风格）。
//!
//! 纯 Rust、阻塞式终端菜单。集成：配置编辑(.env)、查看配置、测试 API、
//! 交易统计、后台服务启/停/重启、实时日志、切换 DRY_RUN、调策略参数、
//! 清空模拟数据、更新程序。
//!
//! 安全：编辑 .env 前自动备份；密钥显示打码；把 DRY_RUN 关掉 / LIVE_TRADING
//! 打开等危险变更需输入大写 YES。

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

use crate::{config, service, state};

// ── 字段元数据 ───────────────────────────────────────────────────────────────

enum Kind {
    Bool,
    Int,
    Float,
    Str,
    Enum(&'static [&'static str]),
}

struct Fld {
    key: &'static str,
    kind: Kind,
    secret: bool,
    desc: &'static str,
}

fn f(key: &'static str, kind: Kind, secret: bool, desc: &'static str) -> Fld {
    Fld { key, kind, secret, desc }
}

fn groups() -> Vec<(&'static str, Vec<Fld>)> {
    vec![
        ("市场周期", vec![f("MARKET_INTERVAL", Kind::Enum(&["5m", "15m"]), false, "交易周期")]),
        ("安全开关", vec![
            f("DRY_RUN", Kind::Bool, false, "true=纸面模拟(安全)"),
            f("LIVE_TRADING", Kind::Bool, false, "true=允许实盘(危险)"),
        ]),
        ("钱包 / API", vec![
            f("POLYMARKET_PRIVATE_KEY", Kind::Str, true, "签名私钥"),
            f("POLYMARKET_API_KEY", Kind::Str, true, "CLOB API key"),
            f("POLYMARKET_API_SECRET", Kind::Str, true, "CLOB API secret"),
            f("POLYMARKET_API_PASSPHRASE", Kind::Str, true, "CLOB API passphrase"),
            f("PM_FUNDER_ADDRESS", Kind::Str, false, "持有USDC的地址"),
            f("POLYMARKET_SIG_TYPE", Kind::Enum(&["0", "1", "2"]), false, "0=EOA 1=PROXY 2=SAFE"),
            f("CHAIN_ID", Kind::Int, false, "链ID(137)"),
        ]),
        ("网络", vec![
            f("RPC_URL", Kind::Str, false, "Polygon RPC"),
            f("WSS_URL", Kind::Str, false, "可选 websocket"),
            f("MARKET_WS_URL", Kind::Str, false, "盘口 WS 地址"),
            f("WS_STALENESS_SEC", Kind::Int, false, "WS过期阈值(秒)"),
            f("PYTHON_BIN", Kind::Str, false, "实盘下单的Python"),
        ]),
        ("下单设置", param_fields()),
        ("入场过滤", vec![
            f("MIN_ENTRY_PRICE", Kind::Float, false, "最低入场价"),
            f("MAX_ENTRY_PRICE", Kind::Float, false, "最高入场价"),
            f("MAX_SPREAD_PCT", Kind::Float, false, "最大点差占比"),
            f("LATE_ENTRY_CUTOFF_SEC", Kind::Int, false, "距结算<此值不进场"),
            f("EARLY_ENTRY_CUTOFF_SEC", Kind::Int, false, "开盘后多少秒才进场"),
            f("MIN_ML_EDGE", Kind::Float, false, "最小模型优势"),
        ]),
        ("离场设置", vec![
            f("TAKE_PROFIT_PCT", Kind::Float, false, "止盈比例"),
            f("ENABLE_STOP_LOSS", Kind::Bool, false, "是否止损"),
            f("STOP_LOSS_PCT", Kind::Float, false, "止损比例"),
        ]),
        ("运行 / 信号", vec![
            f("POLL_INTERVAL_SEC", Kind::Float, false, "轮询间隔(秒)"),
            f("SIGNAL_LOOKBACK_MIN", Kind::Int, false, "信号回看分钟"),
            f("PRICE_FEED", Kind::Enum(&["coinbase", "binance"]), false, "行情源"),
        ]),
    ]
}

/// 「调策略参数」用的精简字段集。
fn param_fields() -> Vec<Fld> {
    vec![
        f("FIXED_SHARES", Kind::Float, false, "每单固定份额"),
        f("ORDER_TYPE", Kind::Enum(&["FOK", "FAK", "GTC"]), false, "订单类型"),
        f("SLIPPAGE_BPS", Kind::Float, false, "滑点(100=1%)"),
        f("MAX_POSITION_USDC", Kind::Float, false, "单仓名义上限"),
        f("MAX_TRADES_PER_MARKET", Kind::Int, false, "每市场最多下单"),
        f("MIN_ML_EDGE", Kind::Float, false, "最小模型优势"),
        f("TAKE_PROFIT_PCT", Kind::Float, false, "止盈比例"),
        f("ENABLE_STOP_LOSS", Kind::Bool, false, "是否止损"),
        f("STOP_LOSS_PCT", Kind::Float, false, "止损比例"),
    ]
}

// ── 终端 IO ──────────────────────────────────────────────────────────────────

fn input(prompt: &str) -> String {
    print!("{prompt}");
    io::stdout().flush().ok();
    let mut s = String::new();
    if io::stdin().read_line(&mut s).unwrap_or(0) == 0 {
        return "0".to_string(); // EOF -> 当作退出，避免死循环
    }
    s.trim().to_string()
}

fn pause() {
    let _ = input("\n按回车继续...");
}

fn hr() {
    println!("============================================================");
}

// ── .env 读写 ────────────────────────────────────────────────────────────────

fn env_path() -> String {
    config::default_env_path()
}

fn ensure_env() {
    let p = env_path();
    if !Path::new(&p).exists() {
        if Path::new(".env.example").exists() {
            let _ = fs::copy(".env.example", &p);
            println!("[i] 未找到 .env，已从 .env.example 生成。");
        } else {
            let _ = fs::write(&p, "");
        }
    }
}

fn read_pairs() -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Ok(txt) = fs::read_to_string(env_path()) {
        for raw in txt.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || !line.contains('=') {
                continue;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            if let Some((k, v)) = line.split_once('=') {
                let mut v = v.trim().to_string();
                if let Some(stripped) = v.strip_prefix('"') {
                    v = stripped.split('"').next().unwrap_or("").to_string();
                } else if let Some(stripped) = v.strip_prefix('\'') {
                    v = stripped.split('\'').next().unwrap_or("").to_string();
                } else if let Some(idx) = v.find(" #") {
                    v = v[..idx].trim().to_string();
                }
                out.push((k.trim().to_string(), v));
            }
        }
    }
    out
}

fn get_val(key: &str) -> String {
    read_pairs().into_iter().find(|(k, _)| k == key).map(|(_, v)| v).unwrap_or_default()
}

fn backup_env() -> Option<String> {
    let p = env_path();
    if !Path::new(&p).exists() {
        return None;
    }
    // 时间戳（用 SystemTime 秒，避免引入 chrono）
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dst = format!("{p}.backup-{ts}");
    fs::copy(&p, &dst).ok().map(|_| dst)
}

fn set_val(key: &str, val: &str) -> Option<String> {
    let backup = backup_env();
    let p = env_path();
    let mut lines: Vec<String> = fs::read_to_string(&p)
        .map(|t| t.lines().map(|s| s.to_string()).collect())
        .unwrap_or_default();
    let mut replaced = false;
    for line in lines.iter_mut() {
        let trimmed = line.trim_start();
        let body = trimmed.strip_prefix("export ").unwrap_or(trimmed);
        if body.starts_with(&format!("{key}=")) {
            *line = format!("{key}={val}");
            replaced = true;
            break;
        }
    }
    if !replaced {
        lines.push(format!("{key}={val}"));
    }
    let _ = fs::write(&p, lines.join("\n") + "\n");
    backup
}

// ── 显示 / 校验 ──────────────────────────────────────────────────────────────

fn mask(secret: bool, val: &str) -> String {
    let v = val.trim();
    if v.is_empty() || v == "0x..." || v == "..." {
        return "(未设置)".to_string();
    }
    if secret {
        if v.len() <= 8 {
            return "****".to_string();
        }
        return format!("{}****{}", &v[..4], &v[v.len() - 4..]);
    }
    v.to_string()
}

fn validate(kind: &Kind, raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    match kind {
        Kind::Bool => match raw.to_lowercase().as_str() {
            "1" | "true" | "yes" | "y" | "on" => Ok("true".to_string()),
            "0" | "false" | "no" | "n" | "off" => Ok("false".to_string()),
            _ => Err("请输入 true/false".to_string()),
        },
        Kind::Int => raw.parse::<i64>().map(|v| v.to_string()).map_err(|_| "请输入整数".to_string()),
        Kind::Float => raw.parse::<f64>().map(|v| v.to_string()).map_err(|_| "请输入数字".to_string()),
        Kind::Enum(choices) => {
            if choices.contains(&raw) {
                Ok(raw.to_string())
            } else {
                Err(format!("只能是: {}", choices.join(", ")))
            }
        }
        Kind::Str => Ok(raw.to_string()),
    }
}

fn danger_ok(key: &str, val: &str) -> bool {
    let risky = (key == "LIVE_TRADING" && val == "true") || (key == "DRY_RUN" && val == "false");
    if !risky {
        return true;
    }
    println!("\n!!! 危险变更：这会让机器人更接近真实下单 !!!");
    println!("    {key} = {val}");
    println!("    真钱下单需 DRY_RUN=false 且 LIVE_TRADING=true。");
    input("    确认请输入大写 YES: ") == "YES"
}

fn edit_one(fl: &Fld) {
    let cur = get_val(fl.key);
    hr();
    println!("  修改: {}", fl.key);
    println!("  说明: {}", fl.desc);
    println!("  当前: {}", mask(fl.secret, &cur));
    match &fl.kind {
        Kind::Enum(c) => println!("  可选: {}", c.join(", ")),
        Kind::Bool => println!("  可选: true / false"),
        _ => {}
    }
    let raw = input("  输入新值（直接回车=取消）: ");
    if raw.is_empty() || raw == "0" {
        println!("  未修改。");
        return;
    }
    match validate(&fl.kind, &raw) {
        Err(e) => {
            println!("  [!] 校验失败: {e}");
        }
        Ok(v) => {
            if !danger_ok(fl.key, &v) {
                println!("  已取消（未保存）。");
                return;
            }
            let backup = set_val(fl.key, &v);
            println!("  [OK] 已保存 {} = {}", fl.key, mask(fl.secret, &v));
            if let Some(b) = backup {
                println!("       备份: {b}");
            }
        }
    }
}

/// 列出字段并循环编辑。
fn edit_fields(title: &str, fields: &[Fld]) {
    loop {
        hr();
        println!("  {title}");
        hr();
        for (i, fl) in fields.iter().enumerate() {
            println!("  {:>2}) {:<26} = {}", i + 1, fl.key, mask(fl.secret, &get_val(fl.key)));
            println!("      {}", fl.desc);
        }
        println!("   0) 返回");
        let c = input("选择要修改的项: ");
        if c == "0" || c.is_empty() {
            return;
        }
        if let Ok(n) = c.parse::<usize>() {
            if n >= 1 && n <= fields.len() {
                edit_one(&fields[n - 1]);
                pause();
            }
        }
    }
}

fn config_menu() {
    let gs = groups();
    loop {
        hr();
        println!("  初始化 / 修改配置（{}）", env_path());
        hr();
        for (i, (name, _)) in gs.iter().enumerate() {
            println!("  {}) {}", i + 1, name);
        }
        println!("  0) 返回");
        let c = input("选择分组: ");
        if c == "0" || c.is_empty() {
            return;
        }
        if let Ok(n) = c.parse::<usize>() {
            if n >= 1 && n <= gs.len() {
                let (name, fields) = &gs[n - 1];
                edit_fields(name, fields);
            }
        }
    }
}

// ── 服务相关动作 ─────────────────────────────────────────────────────────────

fn start_service() {
    if service::is_running() {
        println!("  服务已在运行。");
        return;
    }
    let cfg = match config::load(None) {
        Ok(c) => c,
        Err(e) => {
            println!("  [!] 配置加载失败: {e}");
            return;
        }
    };
    let (flag, assume_yes) = if cfg.can_trade_live() {
        // 实盘：面板二次确认
        println!("\n!!! 即将以【实盘 LIVE】常驻后台启动 —— 真实资金风险 !!!");
        println!("    DRY_RUN=false, LIVE_TRADING=true 已满足。");
        if input("    确认实盘请输入大写 YES: ") != "YES" {
            println!("  已取消。");
            return;
        }
        let missing = cfg.missing_credentials();
        if !missing.is_empty() {
            println!("  [!] 缺少凭证，无法实盘: {}", missing.join(", "));
            return;
        }
        ("--live", true)
    } else {
        ("--simulation", false)
    };
    match service::start(flag, assume_yes) {
        Ok(pid) => println!("  [OK] 服务已后台启动 (PID {pid}, {})", flag.trim_start_matches("--")),
        Err(e) => println!("  [!] 启动失败: {e}"),
    }
}

fn stop_service() {
    match service::stop() {
        Ok(true) => println!("  [OK] 已发送停止信号。"),
        Ok(false) => println!("  服务未在运行。"),
        Err(e) => println!("  [!] 停止失败: {e}"),
    }
}

fn toggle_dry_run() {
    let cur = get_val("DRY_RUN");
    let now_dry = !matches!(cur.to_lowercase().as_str(), "false" | "0" | "no");
    let new_val = if now_dry { "false" } else { "true" };
    println!("  当前 DRY_RUN={cur} -> 切换为 DRY_RUN={new_val}");
    if !danger_ok("DRY_RUN", new_val) {
        println!("  已取消。");
        return;
    }
    set_val("DRY_RUN", new_val);
    println!("  [OK] DRY_RUN={new_val}");
    if new_val == "false" {
        println!("  提示：实盘还需 LIVE_TRADING=true，且服务以 --live 启动。");
    }
    if service::is_running() {
        println!("  注意：服务正在运行，需『重启服务』才生效。");
    }
}

fn clear_sim_data() {
    let cfg = config::load(None).ok();
    let path = cfg
        .map(|c| c.paper_trades_path.display().to_string())
        .unwrap_or_else(|| "paper_trades.json".to_string());
    println!("  将删除模拟交易记录: {path}");
    if input("  确认清空请输入 y: ").to_lowercase() != "y" {
        println!("  已取消。");
        return;
    }
    let _ = fs::remove_file(&path);
    println!("  [OK] 已清空。");
}

fn test_api() {
    let cfg = match config::load(None) {
        Ok(c) => c,
        Err(e) => {
            println!("  [!] 配置加载失败: {e}");
            return;
        }
    };
    println!("  测试中（Gamma 市场发现 / 行情源={}）...", cfg.price_feed);
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(r) => r,
        Err(e) => {
            println!("  [!] 运行时创建失败: {e}");
            return;
        }
    };
    rt.block_on(async {
        let clob = crate::clob::ClobClient::new(
            &cfg.clob_api_url,
            &cfg.gamma_api_url,
            &cfg.slug_prefix(),
            cfg.interval_seconds,
        );
        let ms = clob.upcoming_markets(3).await;
        if ms.is_empty() {
            println!("  [FAIL] Gamma 未返回市场（网络/代理？）");
        } else {
            println!("  [OK] Gamma 发现 {} 个市场，最近: {}", ms.len(), ms[0].slug);
            println!("       盘口/价格 WS 在『启动服务』后实时连接（见实时日志）。");
        }
    });
}

fn update_program() {
    println!("  从 GitHub 拉取最新版本并重新编译...");
    println!("  $ git pull");
    let _ = Command::new("git").arg("pull").status();
    println!("  $ cargo build --release");
    let _ = Command::new("cargo").args(["build", "--release"]).status();
    println!("  完成。若服务在运行，请『重启服务』以加载新版本。");
}

fn view_config() {
    match config::load(None) {
        Ok(cfg) => {
            hr();
            println!("  当前配置 (.env)");
            hr();
            println!("{}", config::describe(&cfg));
        }
        Err(e) => println!("  [!] {e}"),
    }
}

fn view_stats() {
    match config::load(None) {
        Ok(cfg) => {
            let log = state::TradeLog::load(&cfg.paper_trades_path);
            let s = log.summary();
            hr();
            println!("  交易统计表");
            hr();
            println!("  总交易:   {}", s.trades);
            println!("  已结算:   {}", s.settled);
            println!("  胜 / 负:  {} / {}", s.wins, s.losses);
            println!("  胜率:     {:.1}%", s.win_rate);
            println!("  累计 PnL: ${:+.4}", s.pnl_usd);
        }
        Err(e) => println!("  [!] {e}"),
    }
}

fn view_logs() {
    hr();
    println!("  实时日志（末尾 40 行，{}）", service::log_path().display());
    hr();
    println!("{}", service::tail_log(40));
}

// ── 主面板 ───────────────────────────────────────────────────────────────────

fn header() {
    let (svc, extra) = match service::status() {
        service::Status::Running { pid, mode } => ("运行中", format!("PID {pid} / {mode}")),
        service::Status::Stopped => ("已停止", String::new()),
    };
    let dry = get_val("DRY_RUN");
    let live = get_val("LIVE_TRADING");
    let dry_label = if dry.eq_ignore_ascii_case("false") && live.eq_ignore_ascii_case("true") {
        "实盘"
    } else {
        "模拟"
    };
    hr();
    println!("   ☰  JY Bot 管理菜单 (jybot-rs)");
    hr();
    if extra.is_empty() {
        println!("  服务: {svc}    入场: 动量    DRY_RUN: {dry_label}");
    } else {
        println!("  服务: {svc} ({extra})   入场: 动量   DRY_RUN: {dry_label}");
    }
    println!("------------------------------------------------------------");
}

pub fn run() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    ensure_env();
    loop {
        header();
        println!("  1. 初始化 / 修改配置");
        println!("  2. 查看当前配置");
        println!("  3. 测试 API 连接");
        println!("  4. 交易统计表");
        println!("  5. 启动服务（后台常驻）");
        println!("  6. 停止服务");
        println!("  7. 重启服务");
        println!("  8. 查看实时日志");
        println!("  9. 切换 DRY_RUN 模式");
        println!(" 10. 调策略参数");
        println!(" 11. 清空模拟数据");
        println!(" 12. 更新程序（从 GitHub 拉取最新版本）");
        println!("  0. 退出");
        let c = input("? 选择操作: ");
        println!();
        match c.as_str() {
            "1" => config_menu(),
            "2" => { view_config(); pause(); }
            "3" => { test_api(); pause(); }
            "4" => { view_stats(); pause(); }
            "5" => { start_service(); pause(); }
            "6" => { stop_service(); pause(); }
            "7" => {
                stop_service();
                std::thread::sleep(std::time::Duration::from_millis(800));
                start_service();
                pause();
            }
            "8" => { view_logs(); pause(); }
            "9" => { toggle_dry_run(); pause(); }
            "10" => edit_fields("调策略参数", &param_fields()),
            "11" => { clear_sim_data(); pause(); }
            "12" => { update_program(); pause(); }
            "0" | "q" => {
                println!("再见。");
                return Ok(());
            }
            _ => {}
        }
    }
}
