//! VaultSync 分段持久化内核（docs/v2.0/02 §四）。
//!
//! 本 crate 是「单端不会写坏」的机制层：VSSG v1 不可变段、单写者租约、
//! 恢复日志与六步提交协议。它只认 `(record_op, key, record_seq, value)`
//! 记录模型，不含任何业务语义（索引 / 审计只是 value 的解释规则）。
//!
//! 安全边界：明文与密钥只在本层与 `vault-crypto` 之间流转；段密钥一律
//! `HKDF(主密钥, "seg/<namespace>")` 现用现派生，不落盘、不入日志。
#![deny(warnings)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![forbid(unsafe_code)]

pub mod crash;
pub mod error;
pub mod journal;
pub mod lease;
pub mod segment;
pub mod store;

pub use error::StoreError;
pub use journal::{Journal, JournalEntry, JournalEntryKind, RecoveryOutcome};
pub use lease::{LeaseState, VaultLease};
pub use segment::{Record, RecordOp, SegmentKind, VSSG_FORMAT_VER};
pub use store::{OpenMode, VaultStore};

/// 命名空间：决定段密钥派生的用途串（`seg/<namespace>`）。
/// `index` = 索引（VSIX v3）、`audit` = 审计（VSAU v1）、`state` = 恢复日志 / 清单 / 设置。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Namespace {
    Index,
    Audit,
    State,
}

impl Namespace {
    pub fn as_str(self) -> &'static str {
        match self {
            Namespace::Index => "index",
            Namespace::Audit => "audit",
            Namespace::State => "state",
        }
    }

    /// 段密钥用途串（docs/v2.0/02 §4.2 第 2 步）。
    pub fn purpose(self) -> String {
        format!("seg/{}", self.as_str())
    }
}
