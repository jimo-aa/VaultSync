//! VaultSync 核心引擎 —— 唯一 FFI 门面（docs/01 §三）。
//!
//! Dart 侧仅允许经本 crate 的 C ABI 与引擎交互；任何密钥/明文处理逻辑
//! 不得上移到 Dart 层。
//! - `keystore`  保险箱头部格式（VSVB v2，docs/07 版本化）
//! - `service`   密钥封装体系业务逻辑（P1-2/3/4）
//! - `session`   会话与暴力破解防护（P1-5）
//! - `platform_store` 平台安全存储（KEK_bio 托管，P1-3）
//! - `p2p_service` P2P 同步服务编排（P3）
//! - `ffi`       C ABI 导出
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![deny(warnings)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

mod ffi;
mod keystore;
mod p2p_service;
mod platform_store;
mod service;
mod session;

/// 引擎自检：确认全部子 crate 依赖边可用。
pub fn self_check() -> bool {
    vault_crypto::self_check()
        && vault_audit::self_check()
        && vault_vault::self_check()
        && vault_p2p::self_check()
        && vault_stego::self_check()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_crates_linked() {
        assert!(self_check());
    }
}
