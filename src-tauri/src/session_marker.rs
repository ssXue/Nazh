//! 会话守护：通过全局 panic 捕获 + 启动标记检测应用异常退出。
//!
//! 启动时安装 panic hook，若发生 panic 则写入崩溃报告（含调用栈）。
//! 同时写入启动标记作为兜底，检测 SIGKILL / 断电等非 panic 异常终止。
//! 正常退出时清理标记；下次启动时检查残留文件判断上次退出方式。

use std::backtrace::Backtrace;
use std::panic::{self, PanicHookInfo};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const MARKER_FILE: &str = ".session.marker";
const CRASH_FILE: &str = ".session.crash";

/// 上次会话异常退出的类型。
#[derive(Debug)]
pub(crate) enum SessionAnomaly {
    /// 上次会话因 panic 崩溃。
    Panicked(CrashReport),
    /// 上次会话被外部终止（SIGKILL / 断电 / 强制退出等）。
    KilledExternally { pid: u32, started_at_ms: u64 },
}

/// Panic 崩溃报告。
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CrashReport {
    /// panic 消息。
    pub(crate) message: String,
    /// 源码位置 `file:line:column`。
    pub(crate) location: Option<String>,
    /// 调用栈。
    pub(crate) backtrace: String,
    /// panic 发生的 Unix 时间戳（毫秒）。
    pub(crate) panicked_at_ms: u64,
}

#[derive(Serialize, Deserialize)]
struct SessionMarker {
    pid: u32,
    started_at_ms: u64,
}

/// 全局存储 `data_dir`，供 panic hook 写入崩溃报告。
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 初始化会话守护：检测上次异常、写入本次标记、安装 panic hook。
///
/// 返回 `Some(anomaly)` 表示检测到上次会话异常退出。
/// 应在 Tauri `setup` 阶段调用，且仅调用一次。
pub(crate) fn init(data_dir: &Path) -> Option<SessionAnomaly> {
    let _ = DATA_DIR.set(data_dir.to_path_buf());

    let anomaly = detect_anomaly(data_dir);

    write_marker(data_dir);

    // 链式安装 panic hook：先写崩溃报告，再调用原有 hook（保留 Tauri 默认处理）
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        write_crash_report(info);
        previous_hook(info);
    }));

    anomaly
}

/// 正常退出时清理启动标记。
pub(crate) fn clean_shutdown(data_dir: &Path) {
    let _ = std::fs::remove_file(data_dir.join(MARKER_FILE));
}

fn detect_anomaly(data_dir: &Path) -> Option<SessionAnomaly> {
    let marker_path = data_dir.join(MARKER_FILE);
    let crash_path = data_dir.join(CRASH_FILE);

    // 优先检查崩溃报告（panic 导致的异常退出）
    if let Ok(content) = std::fs::read_to_string(&crash_path)
        && let Ok(report) = serde_json::from_str::<CrashReport>(&content)
    {
        let _ = std::fs::remove_file(&crash_path);
        let _ = std::fs::remove_file(&marker_path);
        return Some(SessionAnomaly::Panicked(report));
    }

    // 其次检查残留标记（非 panic 异常：SIGKILL / 断电 / 强制退出）
    if let Ok(content) = std::fs::read_to_string(&marker_path)
        && let Ok(marker) = serde_json::from_str::<SessionMarker>(&content)
        && marker.pid != std::process::id()
    {
        let _ = std::fs::remove_file(&marker_path);
        return Some(SessionAnomaly::KilledExternally {
            pid: marker.pid,
            started_at_ms: marker.started_at_ms,
        });
    }

    None
}

fn write_marker(data_dir: &Path) {
    let marker = SessionMarker {
        pid: std::process::id(),
        started_at_ms: now_ms(),
    };
    if let Ok(content) = serde_json::to_string(&marker) {
        let _ = std::fs::write(data_dir.join(MARKER_FILE), content);
    }
}

fn write_crash_report(info: &PanicHookInfo) {
    let Some(data_dir) = DATA_DIR.get() else {
        return;
    };

    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "未知 panic".to_string());

    let location = info
        .location()
        .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()));

    let backtrace = Backtrace::force_capture().to_string();

    let report = CrashReport {
        message,
        location,
        backtrace,
        panicked_at_ms: now_ms(),
    };

    tracing::error!(
        message = %report.message,
        location = ?report.location,
        "应用发生 panic，正在写入崩溃报告"
    );

    if let Ok(content) = serde_json::to_string(&report) {
        let _ = std::fs::write(data_dir.join(CRASH_FILE), content);
    }
}

#[allow(clippy::cast_possible_truncation)]
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
