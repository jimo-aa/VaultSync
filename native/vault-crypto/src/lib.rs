//! 密钥与密码学原语（P1 实现）。
//!
//! 安全红线：本 crate 是全系统唯一允许实现加解密/密钥派生逻辑的底层模块；
//! 上层 crate 只能通过本 crate 暴露的 API 触碰密钥材料（docs/01、docs/06）。
#![deny(clippy::unwrap_used, clippy::expect_used)]

/// 骨架自检：P0 阶段占位，P1 引入 AES-256-GCM 等真实原语后移除。
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
