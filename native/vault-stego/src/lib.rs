//! LSB 隐写嵌入 / 提取 / 容量估算（P5-3 实现，设计见 docs/05-05）。
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub use vault_crypto;

/// 骨架自检：确认依赖边可用。
pub fn skeleton_ready() -> bool {
    vault_crypto::skeleton_ready()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependency_edges_work() {
        assert!(skeleton_ready());
    }
}
