//! backtest.rs — 用真实 BTC 历史数据回测多个候选信号的方向准确率。
//!
//! 诚实声明：
//!   * 我们没有 Polymarket 的历史盘口价，无法还原"当时买入价"。因此回测严谨
//!     衡量的是【信号方向准确率】——预测每个 5 分钟窗口最终涨/跌是否正确。
//!   * 这是有没有 edge 的根本前提：方向准确率若过不了盈亏平衡线，谈定价/PnL
//!     就没意义。
//!   * PnL 列用"假设以 0.50 中性价买入"估算（avg = 准确率 - 0.50），这是
//!     **乐观上界**——真实市场会随动量调价、还有点差，实际只会更差。
//!   * 标准误 SE ≈ 0.5/sqrt(N)。准确率落在 50% ± 2·SE 内 = 与随机无显著差异。
//!
//! 无前视偏差：决策只用窗口起点后 ENTRY_OFFSET 分钟为止的数据；真值用窗口结算价。

use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq)]
enum Dir {
    Up,
    Down,
}

struct SignalResult {
    name: &'static str,
    trades: usize,
    correct: usize,
}

impl SignalResult {
    fn new(name: &'static str) -> Self {
        SignalResult { name, trades: 0, correct: 0 }
    }
    fn accuracy(&self) -> f64 {
        if self.trades == 0 { 0.0 } else { self.correct as f64 / self.trades as f64 }
    }
}

// ── 历史数据抓取（Coinbase 1 分钟蜡烛，分页） ────────────────────────────────

async fn fetch_history(days: i64) -> Result<Vec<(i64, f64)>, String> {
    let client = reqwest::Client::builder()
        .user_agent("jybot-backtest/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    let now = (now / 60) * 60;
    let start_target = now - days * 86400;

    let mut all: Vec<(i64, f64)> = Vec::new();
    let mut cursor = now;
    let step = 300 * 60; // 每请求 300 根 1m = 5 小时

    while cursor > start_target {
        let end = cursor;
        let start = (cursor - step).max(start_target);
        let url = format!(
            "https://api.exchange.coinbase.com/products/BTC-USD/candles?granularity=60&start={}&end={}",
            iso(start), iso(end)
        );
        match client.get(&url).send().await {
            Ok(resp) => {
                if resp.status().as_u16() == 429 {
                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                    continue;
                }
                let rows: Vec<Vec<f64>> = match resp.json().await {
                    Ok(v) => v,
                    Err(_) => Vec::new(),
                };
                for r in rows {
                    if r.len() >= 5 {
                        all.push((r[0] as i64, r[4])); // [time, low, high, open, close, vol]
                    }
                }
            }
            Err(e) => return Err(format!("请求失败: {e}")),
        }
        cursor -= step;
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }

    all.sort_by_key(|x| x.0);
    all.dedup_by_key(|x| x.0);
    Ok(all)
}

fn iso(ts: i64) -> String {
    // 简易 UTC ISO8601（够 Coinbase 解析）
    let days = ts / 86400;
    let rem = ts % 86400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // 从 1970-01-01 起算日期
    let mut y = 1970i64;
    let mut d = days;
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
        let yd = if leap { 366 } else { 365 };
        if d < yd { break; }
        d -= yd;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
    let mdays = [31, if leap {29} else {28}, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut mo = 0usize;
    while d >= mdays[mo] { d -= mdays[mo]; mo += 1; }
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo + 1, d + 1, h, m, s)
}

// ── 信号（只看决策点 ie 及之前；无前视） ─────────────────────────────────────
// closes: 全量 1m 收盘序列；i0: 窗口起点；ie: 决策点(ie>=i0)。

fn ret_avg(closes: &[f64], end: usize, k: usize) -> f64 {
    if end < k { return 0.0; }
    let mut s = 0.0;
    let mut n = 0;
    for i in (end.saturating_sub(k) + 1)..=end {
        if i == 0 { continue; }
        let prev = closes[i - 1];
        if prev != 0.0 { s += (closes[i] - prev) / prev; n += 1; }
    }
    if n == 0 { 0.0 } else { s / n as f64 }
}

fn ma(closes: &[f64], end: usize, k: usize) -> f64 {
    let start = end.saturating_sub(k - 1);
    let slice = &closes[start..=end];
    slice.iter().sum::<f64>() / slice.len() as f64
}

/// 现有动量信号（与 signal.rs 同口径）
fn sig_momentum(closes: &[f64], i0: usize, ie: usize) -> Option<Dir> {
    let p0 = closes[i0];
    let pn = closes[ie];
    if p0 == 0.0 { return None; }
    let drift = (pn - p0) / p0;
    let mom = ret_avg(closes, ie, 3);
    let logit = 3.0 * (drift / 0.0015) + 2.0 * (mom / 0.0008);
    Some(if logit >= 0.0 { Dir::Up } else { Dir::Down })
}

/// 反转：与动量相反
fn sig_reversal(closes: &[f64], i0: usize, ie: usize) -> Option<Dir> {
    sig_momentum(closes, i0, ie).map(|d| if d == Dir::Up { Dir::Down } else { Dir::Up })
}

/// 动量+阈值：动量太弱就不交易
fn sig_mom_threshold(closes: &[f64], _i0: usize, ie: usize) -> Option<Dir> {
    let mom = ret_avg(closes, ie, 3);
    if mom.abs() < 0.0003 { return None; }
    Some(if mom > 0.0 { Dir::Up } else { Dir::Down })
}

/// 均线交叉：短(3) vs 长(10)
fn sig_ma_cross(closes: &[f64], _i0: usize, ie: usize) -> Option<Dir> {
    if ie < 10 { return None; }
    let fast = ma(closes, ie, 3);
    let slow = ma(closes, ie, 10);
    Some(if fast >= slow { Dir::Up } else { Dir::Down })
}

/// 对照组：永远买涨（应 ≈ 上涨窗口占比，验证回测无偏）
fn sig_always_up(_closes: &[f64], _i0: usize, _ie: usize) -> Option<Dir> {
    Some(Dir::Up)
}

type SigFn = fn(&[f64], usize, usize) -> Option<Dir>;

// ── 入口 ─────────────────────────────────────────────────────────────────────

pub fn run(days: i64, entry_offset_min: usize) -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    println!("==============================================================");
    println!("  回测：BTC 5 分钟 UP/DOWN 方向信号  ({days} 天历史)");
    println!("  决策点 = 窗口开盘后 {entry_offset_min} 分钟；真值 = 结算价 vs 起点价");
    println!("==============================================================");
    println!("  抓取 Coinbase 1m 历史中（分页，请稍候）...");

    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let data = match rt.block_on(fetch_history(days)) {
        Ok(d) => d,
        Err(e) => {
            println!("  [!] 抓取失败: {e}");
            return Ok(());
        }
    };
    if data.len() < 100 {
        println!("  [!] 数据不足（{} 根），换个时间或加大 --days", data.len());
        return Ok(());
    }
    let times: Vec<i64> = data.iter().map(|x| x.0).collect();
    let closes: Vec<f64> = data.iter().map(|x| x.1).collect();
    println!("  得到 {} 根 1m 蜡烛。", closes.len());

    // 索引：ts -> idx，便于按时间定位
    use std::collections::HashMap;
    let idx: HashMap<i64, usize> = times.iter().enumerate().map(|(i, &t)| (t, i)).collect();

    let signals: Vec<(&'static str, SigFn)> = vec![
        ("momentum (现有)", sig_momentum),
        ("reversal 反转", sig_reversal),
        ("mom+阈值", sig_mom_threshold),
        ("MA 交叉(3/10)", sig_ma_cross),
        ("always-up 对照", sig_always_up),
    ];
    let mut results: Vec<SignalResult> = signals.iter().map(|(n, _)| SignalResult::new(n)).collect();

    let mut windows = 0usize;
    let mut up_windows = 0usize;

    // 遍历对齐到 300s 的窗口起点
    for (&t0, &i0) in idx.iter() {
        if t0 % 300 != 0 { continue; }
        let settle_t = t0 + 300;
        let ie_t = t0 + (entry_offset_min as i64) * 60;
        let (i_settle, ie) = match (idx.get(&settle_t), idx.get(&ie_t)) {
            (Some(&s), Some(&e)) => (s, e),
            _ => continue,
        };
        if ie < i0 || i_settle <= i0 { continue; }

        let truth = if closes[i_settle] > closes[i0] {
            Dir::Up
        } else if closes[i_settle] < closes[i0] {
            Dir::Down
        } else {
            continue; // 平局不计
        };
        windows += 1;
        if truth == Dir::Up { up_windows += 1; }

        for (si, (_, f)) in signals.iter().enumerate() {
            if let Some(pred) = f(&closes, i0, ie) {
                results[si].trades += 1;
                if pred == truth {
                    results[si].correct += 1;
                }
            }
        }
    }

    if windows == 0 {
        println!("  [!] 没有可用窗口（数据可能不连续）。");
        return Ok(());
    }

    let se = 0.5 / (windows as f64).sqrt() * 100.0;
    println!("--------------------------------------------------------------");
    println!("  有效窗口: {windows}   上涨占比: {:.1}%   单次标准误 SE≈{:.2}%",
             up_windows as f64 / windows as f64 * 100.0, se);
    println!("  盈亏平衡(0.5价,0手续费): 50.0%   显著优于随机需 > {:.1}%", 50.0 + 2.0 * se);
    println!("--------------------------------------------------------------");
    println!("  {:<18} {:>8} {:>9} {:>14}", "信号", "交易数", "方向准确率", "平均PnL(0.5假设)");
    for r in &results {
        let acc = r.accuracy() * 100.0;
        let pnl = r.accuracy() - 0.5; // 每份(0.5平价)
        println!("  {:<18} {:>8} {:>8.1}% {:>+13.4}", r.name, r.trades, acc, pnl);
    }
    println!("--------------------------------------------------------------");
    println!("  解读：准确率落在 50%±{:.1}% 内 = 与随机无显著差异（没有可利用 edge）。", 2.0 * se);
    println!("        'always-up' 应 ≈ 上涨占比，用于验证回测无偏。");
    println!("        ⚠ 陷阱：--entry-min 越大准确率越高，那是【已实现走势泄漏】不是预测力。");
    println!("           看真实预测力请用 --entry-min 0（窗口起点决策，drift=0）。");
    println!("        ⚠ PnL 是 0.5 平价乐观上界；真实开盘后市场价已反映 drift，edge 只会更小/为负。");
    println!("==============================================================");
    Ok(())
}
