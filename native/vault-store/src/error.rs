//! 错误类型。映射约定见 docs/v2.0/01 §6.7：`LeaseBusy` → 9、
//! `Maintenance` → 10、`NeedsRecovery` → 11、`ReadOnly` → 9。

#[derive(Debug)]
pub enum StoreError {
    /// 磁盘 IO 失败（FFI 码 3）。
    Io(std::io::Error),
    /// 结构可解析但版本 / 魔数不符（FFI 码 6）。
    Format(&'static str),
    /// CRC / digest / AEAD tag 校验失败（FFI 码 6，触发恢复规则）。
    Corrupt(&'static str),
    /// 单写者租约被他进程持有（FFI 码 9，只读降级）。
    LeaseBusy,
    /// 维护态：业务写入冻结（FFI 码 10）。
    Maintenance,
    /// 恢复日志无法自动回滚 / 前滚，需要显式恢复（FFI 码 11）。
    NeedsRecovery(&'static str),
    /// 只读实例上执行了写操作（FFI 码 9）。
    ReadOnly,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "io error: {e}"),
            StoreError::Format(m) => write!(f, "format error: {m}"),
            StoreError::Corrupt(m) => write!(f, "corrupt: {m}"),
            StoreError::LeaseBusy => write!(f, "lease held by another process"),
            StoreError::Maintenance => write!(f, "vault in maintenance"),
            StoreError::NeedsRecovery(m) => write!(f, "recovery required: {m}"),
            StoreError::ReadOnly => write!(f, "vault opened read-only"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}
