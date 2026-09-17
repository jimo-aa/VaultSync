//! 加密保险箱：容器格式、加密索引、CDC 分块、安全擦除（P2 实现）。
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub use vault_audit;
pub use vault_crypto;

/// 骨架自检：确认依赖边可用（crypto / audit 已接入）。
pub fn self_check() -> bool {
    vault_crypto::self_check() && vault_audit::self_check()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependency_edges_work() {
        assert!(self_check());
    }
}
