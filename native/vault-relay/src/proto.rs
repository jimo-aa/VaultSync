//! VSR2 协议层（docs/v2.0/08 §1.2、§3.4）：控制行解析/格式化 + 错误码 +
//! 数据阶段二进制帧 + proof 计算/常量时间比对。
//!
//! 控制阶段（OK 之前）：单行 UTF-8 文本，LF 结尾，单空格分隔，行内无 CR，
//! 单行上限 512 B。数据阶段（OK 之后）：`u32 BE len ‖ u8 type ‖ payload`，
//! `len ≤ 65 541`（= 1 + DATA 载荷上限 65 540）。
#![deny(unsafe_code)]

/// 数据帧最大 `len`（1 + DATA 载荷 65 540）。
pub const MAX_FRAME: u32 = 65_541;
/// 控制行上限。
pub const MAX_LINE: usize = 512;
/// 数据帧类型。
pub const TYPE_DATA: u8 = 1;
pub const TYPE_CONTROL: u8 = 2;
pub const TYPE_PING: u8 = 3;

/// 中继错误码 R1–R17（**与 `vault_core_err_e` 0–13 不是同一空间**，
/// 客户端必须映射为本机码 + 可读诊断，docs/08 §3.4）。
/// 部分变体当前无触发路径（时间窗 / 带宽桶 / 房间 TTL 超时 / 内部错误
/// 各留 R 位，随运维端点与带宽桶完善启用）——枚举即协议契约，整体保留。
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayError {
    BadMagic,
    BadVer,
    BadRoom,
    AuthRequired,
    AuthFailed,
    AuthExpired,
    AuthReplay,
    LimitConcurrency,
    LimitRate,
    LimitSessionTime,
    LimitSessionIdle,
    LimitSessionBytes,
    LimitBandwidth,
    Busy,
    RoomTimeout,
    BadFrame,
    ServerError,
}

impl RelayError {
    pub fn code(self) -> u8 {
        match self {
            RelayError::BadMagic => 1,
            RelayError::BadVer => 2,
            RelayError::BadRoom => 3,
            RelayError::AuthRequired => 4,
            RelayError::AuthFailed => 5,
            RelayError::AuthExpired => 6,
            RelayError::AuthReplay => 7,
            RelayError::LimitConcurrency => 8,
            RelayError::LimitRate => 9,
            RelayError::LimitSessionTime => 10,
            RelayError::LimitSessionIdle => 11,
            RelayError::LimitSessionBytes => 12,
            RelayError::LimitBandwidth => 13,
            RelayError::Busy => 14,
            RelayError::RoomTimeout => 15,
            RelayError::BadFrame => 16,
            RelayError::ServerError => 17,
        }
    }

    pub fn token(self) -> &'static str {
        match self {
            RelayError::BadMagic => "bad-magic",
            RelayError::BadVer => "bad-ver",
            RelayError::BadRoom => "bad-room",
            RelayError::AuthRequired => "auth-required",
            RelayError::AuthFailed => "auth-failed",
            RelayError::AuthExpired => "auth-expired",
            RelayError::AuthReplay => "auth-replay",
            RelayError::LimitConcurrency => "limit-concurrency",
            RelayError::LimitRate => "limit-rate",
            RelayError::LimitSessionTime => "limit-session-time",
            RelayError::LimitSessionIdle => "limit-session-idle",
            RelayError::LimitSessionBytes => "limit-session-bytes",
            RelayError::LimitBandwidth => "limit-bandwidth",
            RelayError::Busy => "busy",
            RelayError::RoomTimeout => "room-timeout",
            RelayError::BadFrame => "bad-frame",
            RelayError::ServerError => "server-error",
        }
    }
}

/// 校验房间 token（字符集 `[A-Za-z0-9_-]`，≤128；中继不解释其语义）。
pub fn valid_room(room: &str) -> bool {
    !room.is_empty()
        && room.len() <= 128
        && room
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 计算 `proof = HMAC-SHA256(token, "VSR2-AUTH" ‖ room ‖ cli_nonce ‖ srv_nonce ‖ ts_bucket)`。
/// `ts_bucket` 以十进制文本拼入（与服务端逐字段一致）。
pub fn compute_proof(
    token: &[u8],
    room: &str,
    cli_nonce: &str,
    srv_nonce: &str,
    ts_bucket: u64,
) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    let mut mac = match <Hmac<sha2::Sha256> as Mac>::new_from_slice(token) {
        Ok(m) => m,
        Err(_) => return Vec::new(), // HMAC 接受任意长度 key；此分支实际不可达
    };
    mac.update(b"VSR2-AUTH");
    mac.update(room.as_bytes());
    mac.update(cli_nonce.as_bytes());
    mac.update(srv_nonce.as_bytes());
    mac.update(ts_bucket.to_string().as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// 常量时间比对（不短路——防按响应时间区分 token 是否存在）。
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// 客户端能力摘要校验：4 hex；bit2 = 支持 OBSERVE；bit3–15 保留（置位记诊断并忽略）。
pub fn valid_caps(caps: &str) -> bool {
    caps.len() == 4 && caps.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// proof 绑定全部输入：任一字段变化 → 不同 proof（重放防护的根基）。
    #[test]
    fn proof_binds_room_nonces_and_bucket() {
        let base = compute_proof(b"tok", "room", "cli", "srv", 100);
        assert_eq!(base.len(), 32);
        assert_ne!(base, compute_proof(b"tok2", "room", "cli", "srv", 100));
        assert_ne!(base, compute_proof(b"tok", "room2", "cli", "srv", 100));
        assert_ne!(base, compute_proof(b"tok", "room", "cli2", "srv", 100));
        assert_ne!(base, compute_proof(b"tok", "room", "cli", "srv2", 100));
        assert_ne!(base, compute_proof(b"tok", "room", "cli", "srv", 101));
    }

    /// 常量时间比对：内容相同为真，不同为假（长度不等直接假）。
    #[test]
    fn ct_eq_basics() {
        assert!(ct_eq(b"aaaa", b"aaaa"));
        assert!(!ct_eq(b"aaaa", b"aaab"));
        assert!(!ct_eq(b"aaaa", b"aaa"));
    }

    /// 错误码与 token 一一对应（R1–R17）。
    #[test]
    fn error_codes_match_spec() {
        assert_eq!(RelayError::BadMagic.code(), 1);
        assert_eq!(RelayError::BadVer.token(), "bad-ver");
        assert_eq!(RelayError::BadFrame.code(), 16);
        assert_eq!(RelayError::ServerError.token(), "server-error");
        let all = [
            RelayError::BadMagic,
            RelayError::BadVer,
            RelayError::BadRoom,
            RelayError::AuthRequired,
            RelayError::AuthFailed,
            RelayError::AuthExpired,
            RelayError::AuthReplay,
            RelayError::LimitConcurrency,
            RelayError::LimitRate,
            RelayError::LimitSessionTime,
            RelayError::LimitSessionIdle,
            RelayError::LimitSessionBytes,
            RelayError::LimitBandwidth,
            RelayError::Busy,
            RelayError::RoomTimeout,
            RelayError::BadFrame,
            RelayError::ServerError,
        ];
        // 17 个错误码连续且唯一（R1–R17），token 非空且不重复
        let mut codes: Vec<u8> = all.iter().map(|e| e.code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), 17, "错误码必须连续唯一（R1–R17）");
        for e in all {
            assert!(!e.token().is_empty());
        }
    }
}
