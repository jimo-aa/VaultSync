//! 崩溃注入（P6-8）：`VS_ABORT_POINT` 环境变量指定的中断点处 `abort()`。
//!
//! 用法：测试 / 外部驱动进程设置 `VS_ABORT_POINT=<point>` 后调用引擎，
//! 引擎走到同名中断点即硬崩溃（不是 panic，模拟真实掉电）。
//! 中断点清单见 `store.rs` 的 `abort_point!` 调用位置。

/// 命中注入点则硬崩溃；返回表示未命中，正常继续。
pub fn abort_point(point: &str) {
    if let Ok(target) = std::env::var("VS_ABORT_POINT") {
        if target == point {
            // 刷掉缓冲再崩溃，保证 stderr 消息可见
            use std::io::Write;
            let _ = std::io::stderr().flush();
            std::process::abort();
        }
    }
}

/// 命中注入点则直接返回错误（「软崩溃」：进程活着但操作中断，用于
/// 无法在单进程内 abort 的路径，如 FFI 冒烟）。
pub fn soft_abort_point(point: &str) -> Option<crate::error::StoreError> {
    if let Ok(target) = std::env::var("VS_ABORT_POINT") {
        if target == point {
            return Some(crate::error::StoreError::Io(std::io::Error::other(
                format!("injected abort at {point}"),
            )));
        }
    }
    None
}
