//! 加密保险箱：容器格式、加密索引、CDC 分块、安全擦除（docs/05-02）。
//!
//! - `container`  每文件密文容器（VSEF v2：Header + 加密元数据 + CDC 块数据区）
//! - `index`      加密索引（VSIX v2：文件夹树/标签/倒排搜索，整体 AEAD 落盘）
//! - `vault`      编排：导入/导出/擦除/分享（FSK/FSKey 派生链在此落地）
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![deny(warnings)]

pub mod container;
pub mod index;
pub mod vault;

pub use vault_audit;
pub use vault_crypto;

/// 引擎自检：确认依赖边可用（crypto / audit 已接入）。
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
