//! paths.rs — 全局唯一的「系统级」数据目录。
//!
//! 机器人的配置/状态/日志/持仓记录都集中在一个固定目录（默认 `~/.jybot`，
//! 可用环境变量 `JYBOT_HOME` 覆盖）。所有组件（面板、后台服务、命令行）都
//! 通过这里的**绝对路径**访问，因此无论从哪个目录、用什么方式启动，读写的
//! 都是**同一份**配置与状态——你在面板里改的就是整台机器人的系统级设置。
//!
//!   ~/.jybot/.env                 唯一配置（面板编辑的就是它）
//!   ~/.jybot/jybot.pid            后台服务 PID
//!   ~/.jybot/logs/service.log     服务日志
//!   ~/.jybot/paper_trades.json    模拟交易记录
//!   ~/.jybot/scripts/place_order.py  实盘下单助手
//!   ~/.jybot/bin/jybot-rs         二进制（更新时替换此文件）

use std::path::PathBuf;

/// 全局数据根目录。优先 `JYBOT_HOME`，否则 `~/.jybot`。
pub fn home() -> PathBuf {
    if let Ok(p) = std::env::var("JYBOT_HOME") {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    let base = std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())
        .unwrap_or_else(|| ".".to_string());
    PathBuf::from(base).join(".jybot")
}

/// 创建必要的子目录，返回 home。
pub fn ensure() -> PathBuf {
    let h = home();
    let _ = std::fs::create_dir_all(h.join("logs"));
    let _ = std::fs::create_dir_all(h.join("scripts"));
    let _ = std::fs::create_dir_all(h.join("bin"));
    h
}

pub fn env_file() -> PathBuf {
    home().join(".env")
}

pub fn pid_file() -> PathBuf {
    home().join("jybot.pid")
}

pub fn log_file() -> PathBuf {
    home().join("logs").join("service.log")
}

pub fn paper_trades() -> PathBuf {
    home().join("paper_trades.json")
}

pub fn place_order_script() -> PathBuf {
    home().join("scripts").join("place_order.py")
}

/// 二进制自身路径（面板「更新程序」会替换它）。
pub fn binary() -> PathBuf {
    let name = if cfg!(target_os = "windows") { "jybot-rs.exe" } else { "jybot-rs" };
    home().join("bin").join(name)
}
