//! service.rs — 后台常驻服务管理（启动/停止/重启/状态/日志）。
//!
//! 设计：
//!   * 「启动服务」= spawn 一个脱离终端的后台子进程（jybot-rs --simulation/--live），
//!     stdout/stderr 重定向到 logs/service.log，PID 写入 jybot.pid。
//!   * 面板退出后服务继续运行（常驻）。
//!   * 跨平台：Windows 用 DETACHED_PROCESS 标志；Unix 直接 spawn（不随父进程退出而被杀）。
//!   * 停止/状态通过 PID 文件 + 系统命令（taskkill / kill）实现，无需额外依赖。

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const PID_FILE: &str = "jybot.pid";
const LOG_FILE: &str = "logs/service.log";

pub enum Status {
    Stopped,
    Running { pid: u32, mode: String },
}

fn pid_path() -> PathBuf {
    PathBuf::from(PID_FILE)
}

pub fn log_path() -> PathBuf {
    PathBuf::from(LOG_FILE)
}

/// 读取 PID 文件 -> (pid, mode)
fn read_pidfile() -> Option<(u32, String)> {
    let txt = fs::read_to_string(pid_path()).ok()?;
    let mut it = txt.trim().splitn(2, char::is_whitespace);
    let pid: u32 = it.next()?.trim().parse().ok()?;
    let mode = it.next().unwrap_or("simulation").trim().to_string();
    Some((pid, mode))
}

#[cfg(windows)]
fn is_alive(pid: u32) -> bool {
    let out = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()),
        Err(_) => false,
    }
}

#[cfg(unix)]
fn is_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 当前服务状态。
pub fn status() -> Status {
    if let Some((pid, mode)) = read_pidfile() {
        if is_alive(pid) {
            return Status::Running { pid, mode };
        }
        // 进程已不在 -> 清理陈旧 PID 文件
        let _ = fs::remove_file(pid_path());
    }
    Status::Stopped
}

pub fn is_running() -> bool {
    matches!(status(), Status::Running { .. })
}

/// 启动后台服务。mode_flag 形如 "--simulation" / "--live"。
/// assume_yes：实盘时跳过子进程的交互确认（面板已确认）。
pub fn start(mode_flag: &str, assume_yes: bool) -> std::io::Result<u32> {
    if let Status::Running { pid, .. } = status() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("服务已在运行 (PID {pid})"),
        ));
    }

    fs::create_dir_all("logs").ok();
    // 每次启动覆盖日志，避免无限增长
    let log = fs::File::create(log_path())?;
    let log_err = log.try_clone()?;

    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.arg(mode_flag);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::from(log));
    cmd.stderr(Stdio::from(log_err));
    if assume_yes {
        cmd.env("JY_ASSUME_YES", "1");
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS(0x08) | CREATE_NEW_PROCESS_GROUP(0x200)
        cmd.creation_flags(0x0000_0008 | 0x0000_0200);
    }

    let child = cmd.spawn()?;
    let pid = child.id();
    // 不 wait、不 kill_on_drop —— 子进程脱离父进程常驻
    std::mem::forget(child);

    let mode = mode_flag.trim_start_matches("--").to_string();
    fs::write(pid_path(), format!("{pid} {mode}\n"))?;
    Ok(pid)
}

/// 停止后台服务。
pub fn stop() -> std::io::Result<bool> {
    let (pid, _) = match read_pidfile() {
        Some(v) => v,
        None => return Ok(false),
    };
    kill(pid);
    let _ = fs::remove_file(pid_path());
    Ok(true)
}

#[cfg(windows)]
fn kill(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(unix)]
fn kill(pid: u32) {
    let _ = Command::new("kill").arg(pid.to_string()).status();
    std::thread::sleep(std::time::Duration::from_millis(500));
    if is_alive(pid) {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
    }
}

/// 读取日志末尾 n 行。
pub fn tail_log(n: usize) -> String {
    match fs::read_to_string(log_path()) {
        Ok(txt) => {
            let lines: Vec<&str> = txt.lines().collect();
            let start = lines.len().saturating_sub(n);
            lines[start..].join("\n")
        }
        Err(_) => "(暂无日志，启动服务后生成 logs/service.log)".to_string(),
    }
}
