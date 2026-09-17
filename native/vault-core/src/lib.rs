//! VaultSync 核心引擎 —— 唯一 FFI 门面（docs/01 §三）。
//!
//! Dart 侧仅允许经本 crate 的 C ABI 与引擎交互；任何密钥/明文处理逻辑
//! 不得上移到 Dart 层。P0 阶段只暴露引擎自检接口（P0-4）。

mod ffi;

/// 引擎自检：确认全部子 crate 依赖边可用。
pub fn self_check() -> bool {
    vault_crypto::skeleton_ready()
        && vault_audit::skeleton_ready()
        && vault_vault::skeleton_ready()
        && vault_p2p::skeleton_ready()
        && vault_stego::skeleton_ready()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_crates_linked() {
        assert!(self_check());
    }
}
