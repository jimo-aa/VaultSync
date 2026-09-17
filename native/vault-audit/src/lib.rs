//! 链式哈希审计日志（docs/01 设计原则五：可审计）。
//!
//! 全部敏感操作必须经本 crate 记录，链式哈希保证事后可追溯且不可篡改。
#![deny(clippy::unwrap_used, clippy::expect_used)]

/// 骨架自检：P0 阶段占位。
pub fn skeleton_ready() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_ready_is_true() {
        assert!(skeleton_ready());
    }
}
