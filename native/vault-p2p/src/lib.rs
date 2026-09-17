//! P2P 设备配对与增量同步（P3 实现，协议见 docs/08）。
//!
//! 中继只转发密文：本 crate 对外的一切网络载荷必须是密文块（加密块=同步块=传输块）。
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub use vault_audit;
pub use vault_crypto;

/// 骨架自检：确认依赖边可用。
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
