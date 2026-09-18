//! P2P 设备配对与增量同步（P3，协议见 docs/05-03、docs/05-04、docs/08）。
//!
//! 中继只转发密文：本 crate 对外的一切网络载荷必须是密文块（加密块=同步块=传输块）。
//! 模块：`identity` 设备身份 / `channel` Noise 加密信道 / `proto` 信令 /
//! `peers` 对端登记表 / `engine` 同步引擎编排。
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod channel;
pub mod engine;
pub mod identity;
pub mod peers;
pub mod proto;

pub use engine::P2pEngine;
pub use identity::Identity;

pub use vault_audit;
pub use vault_crypto;

/// 引擎自检：确认依赖边与身份/信道可用。
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
