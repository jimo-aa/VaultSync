//! 加密保险箱：容器格式、加密索引、CDC 分块、安全擦除（P2 实现）。
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub use vault_audit;
pub use vault_crypto;

/// 骨架自检：确认依赖边可用（crypto / audit 已接入）。
pub fn skeleton_ready() -> bool {
    vault_crypto::skeleton_ready() && vault_audit::skeleton_ready()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependency_edges_work() {
        assert!(skeleton_ready());
    }
}
