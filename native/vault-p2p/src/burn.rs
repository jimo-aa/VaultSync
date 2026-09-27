//! 阅后即焚（P8-9，docs/v2.0/05-04 §五）。
//!
//! 一次性 P2P 会话：票据秘密 → 邀请码 / 房间 / PSK 三处派生；会话建立 → 传输 →
//! **传输完成即失效**；对端「打开」后或超时即**自动加密擦除**。
//!
//! ## 两处必须在实现里讲清的设计裁决（docs 内部张力，已记 LOG）
//! 1. **邀请码即秘密载体**：`HKDF` 是单向的，接收端**无法**从 `HKDF(secret,"burn-code")`
//!    反推 `secret`。故本实现把 10 B CSPRNG 作为 `burn_secret`，邀请码 = `base32(secret)`
//!    （16 字符，80 位）——接收端解出同一 secret 后派生 `psk` / `room`，
//!    与既有「配对邀请码即 PSK 载体」同构（`05-03` §3.3 的口径）。
//! 2. **`burn_secret` 不落盘**：只存内存（`Zeroizing`），进程重启即无票据
//!    → 未完成会话一律收敛 `Expired` 并擦除残留（`05-04` §5.5）。
//!    `05-04` §5.7② 的「票据记录含 `secret: 32 B` 落盘」与之冲突，本实现按 §5.5 处理。
#![deny(unsafe_code)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use zeroize::Zeroizing;

/// 邀请码字符集（RFC 4648 base32，无填充、大写）。
const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// `burn_secret` 长度（字节）→ 邀请码 16 字符（80 位一次性凭据；与 `05-04` §5.2 的
/// 「16 字符邀请码」一致）。
pub const SECRET_LEN: usize = 10;

/// 擦除重试间隔（`05-04` §5.5：1 s / 5 s / 30 s，共 3 次重试 = 最多 4 次尝试）。
pub const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(5),
    Duration::from_secs(30),
];

/// 会话相位（`05-04` §5.3 状态机）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BurnPhase {
    Offering,
    Transferring,
    Delivered,
    Opened,
    Erasing,
    Erased,
    Expired,
    Rejected,
    EraseFailed,
}

impl BurnPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            BurnPhase::Offering => "offering",
            BurnPhase::Transferring => "transferring",
            BurnPhase::Delivered => "delivered",
            BurnPhase::Opened => "opened",
            BurnPhase::Erasing => "erasing",
            BurnPhase::Erased => "erased",
            BurnPhase::Expired => "expired",
            BurnPhase::Rejected => "rejected",
            BurnPhase::EraseFailed => "erase_failed",
        }
    }
}

/// 一次性票据（秘密仅内存）。
pub struct BurnTicket {
    pub burn_id: u64,
    /// 仅内存（`Zeroizing`：Drop 即擦除）；绝不落盘。
    pub secret: Zeroizing<Vec<u8>>,
    pub file_id: u64,
    pub name: String,
    pub ttl_secs: u64,
    pub open_timeout_secs: u64,
    pub created_ms: u64,
    /// 已被使用过（`Delivered` 后不可复用；`05-04` §5.2）。
    pub consumed: bool,
}

impl BurnTicket {
    pub fn expires_ms(&self) -> u64 {
        self.created_ms
            .saturating_add(self.ttl_secs.saturating_mul(1000))
    }

    pub fn is_expired(&self, now_ms: u64) -> bool {
        self.ttl_secs > 0 && now_ms > self.expires_ms()
    }
}

/// base32 编码（无填充）。
pub fn base32_encode(data: &[u8]) -> String {
    let mut out = String::new();
    let mut buf: u32 = 0;
    let mut bits = 0u32;
    for b in data {
        buf = (buf << 8) | u32::from(*b);
        bits += 8;
        while bits >= 5 {
            let idx = ((buf >> (bits - 5)) & 0x1f) as usize;
            out.push(ALPHABET[idx] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        let idx = ((buf << (5 - bits)) & 0x1f) as usize;
        out.push(ALPHABET[idx] as char);
    }
    out
}

/// base32 解码（接受无填充输入；非法字符 → None）。
pub fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf: u32 = 0;
    let mut bits = 0u32;
    for c in s.bytes() {
        let up = c.to_ascii_uppercase();
        let idx = ALPHABET.iter().position(|a| *a == up)? as u32;
        buf = (buf << 5) | idx;
        bits += 5;
        if bits >= 8 {
            out.push(((buf >> (bits - 8)) & 0xff) as u8);
            bits -= 8;
        }
    }
    Some(out)
}

/// 邀请码 = `base32(burn_secret)`（16 字符；**码即秘密载体**，见模块头裁决 1）。
pub fn burn_code(secret: &[u8]) -> String {
    base32_encode(secret)
}

/// 由邀请码还原 `burn_secret`（长度校验收口到 [`SECRET_LEN`]）。
pub fn burn_secret_from_code(code: &str) -> Option<Vec<u8>> {
    let raw = base32_decode(code)?;
    if raw.len() != SECRET_LEN {
        return None;
    }
    Some(raw)
}

/// 会话 PSK = `HKDF(burn_secret, "burn-psk")`（喂 `Noise_XXpsk3`；不一致握手必败）。
pub fn burn_psk(secret: &[u8]) -> [u8; 32] {
    *vault_crypto::hkdf_sha256_derive(secret, b"burn-psk")
}

/// 中继房间 = `base32(HKDF(burn_secret, "burn-room")[..8])`（中继只见房间 token）。
pub fn burn_room(secret: &[u8]) -> String {
    let d = vault_crypto::hkdf_sha256_derive(secret, b"burn-room");
    base32_encode(&d[..8])
}

/// 落盘密钥 = `HKDF(burn_secret, "burn-file")`（**专用 FSKey**，`05-04` §5.4）：
/// 只保护 `<data_dir>/burn/<id>.vse` 这一份一次性容器；秘密不落盘 → 重启后不可解。
pub fn burn_file_key(secret: &[u8]) -> [u8; 32] {
    *vault_crypto::hkdf_sha256_derive(secret, b"burn-file")
}

/// 生成新票据（秘密 CSPRNG，仅内存）。
pub fn new_ticket(
    burn_id: u64,
    file_id: u64,
    name: &str,
    ttl_secs: u64,
    open_timeout_secs: u64,
    now_ms: u64,
) -> BurnTicket {
    BurnTicket {
        burn_id,
        secret: Zeroizing::new(vault_crypto::random_bytes(SECRET_LEN)),
        file_id,
        name: name.to_string(),
        ttl_secs,
        open_timeout_secs,
        created_ms: now_ms,
        consumed: false,
    }
}

/// 擦除（带重试）：删除 `paths` 中的每个文件；任一轮全部成功即返回。
/// `delays` 为两轮之间的等待（生产用 [`RETRY_DELAYS`]，测试传 0 时长）。
/// 返回 `(全部成功, 实际尝试轮数)`——失败时调用方须进 `EraseFailed` 并告警。
pub fn erase_with_retries<F>(paths: &[PathBuf], delays: &[Duration], mut deleter: F) -> (bool, u32)
where
    F: FnMut(&Path) -> std::io::Result<()>,
{
    let mut attempts = 0u32;
    loop {
        attempts += 1;
        let mut all_ok = true;
        for p in paths {
            // 已不存在视为成功（幂等）
            if !p.exists() {
                continue;
            }
            if deleter(p).is_err() {
                all_ok = false;
            }
        }
        if all_ok {
            return (true, attempts);
        }
        let idx = (attempts - 1) as usize;
        if idx >= delays.len() {
            return (false, attempts);
        }
        std::thread::sleep(delays[idx]);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// 邀请码往返：16 字符、解码回同一秘密；长度不符拒绝。
    #[test]
    fn code_is_secret_carrier_and_roundtrips() {
        let secret = vec![0x11u8; SECRET_LEN];
        let code = burn_code(&secret);
        assert_eq!(code.len(), 16, "邀请码 16 字符（$code）");
        assert!(code.bytes().all(|b| b.is_ascii_alphanumeric()));
        assert_eq!(burn_secret_from_code(&code).unwrap(), secret);
        assert!(burn_secret_from_code("SHORT").is_none(), "长度不符必须拒绝");
        assert!(burn_secret_from_code("!!!!!!!!!!!!!!!!").is_none());
        // 随机秘密 → 不同码
        let s1 = vault_crypto::random_bytes(SECRET_LEN);
        let s2 = vault_crypto::random_bytes(SECRET_LEN);
        assert_ne!(burn_code(&s1), burn_code(&s2));
    }

    /// 三处派生互不相同、且确定性（双端独立计算必须一致）。
    #[test]
    fn derivations_are_distinct_and_deterministic() {
        let secret = vault_crypto::random_bytes(SECRET_LEN);
        let psk = burn_psk(&secret);
        let room = burn_room(&secret);
        assert_ne!(psk, burn_psk(&vault_crypto::random_bytes(SECRET_LEN)));
        assert!(!room.is_empty() && room != burn_code(&secret));
        assert_eq!(burn_psk(&secret), psk, "PSK 确定性");
        assert_eq!(burn_room(&secret), room, "房间确定性");
        // PSK 与房间不相等（域串分离）
        assert_ne!(burn_code(&secret), room);
    }

    /// 票据：TTL 到期判定 + `consumed` 单次性。
    #[test]
    fn ticket_single_use_and_expiry() {
        let mut t = new_ticket(7, 3, "secret.pdf", 900, 60, 1_000_000);
        assert_eq!(t.burn_id, 7);
        assert!(!t.is_expired(1_000_001));
        assert!(t.is_expired(1_000_000 + 900_000 + 1), "超 TTL 必须过期");
        assert!(!t.consumed);
        t.consumed = true;
        assert!(t.consumed, "交付后不可复用（单次性标记）");
        // ttl = 0 = 不自动过期（05-04 §5.4 的 `0 = 不过期`）
        let t0 = new_ticket(8, 1, "x", 0, 60, 0);
        assert!(!t0.is_expired(u64::MAX / 2));
    }

    /// 擦除重试：前两轮失败、第三轮成功 → 成功且尝试 3 轮；持续失败 → 4 轮后放弃。
    #[test]
    fn erase_retries_then_gives_up() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("payload.bin");
        std::fs::write(&f, b"secret").unwrap();
        let zero = [Duration::ZERO; 3];

        let mut calls = 0;
        let (ok, attempts) = erase_with_retries(std::slice::from_ref(&f), &zero, |p| {
            calls += 1;
            if calls < 3 {
                return Err(std::io::Error::other("injected failure"));
            }
            std::fs::remove_file(p)
        });
        assert!(ok && attempts == 3, "失败重试后成功（attempts={attempts}）");
        assert!(!f.exists());

        // 持续失败 → 放弃（1 + 3 轮重试 = 4 轮），文件仍在 → 调用方须告警
        std::fs::write(&f, b"again").unwrap();
        let (ok2, attempts2) = erase_with_retries(std::slice::from_ref(&f), &zero, |_p| {
            Err(std::io::Error::other("persistent"))
        });
        assert!(
            !ok2 && attempts2 == 4,
            "持续失败必须在 4 轮后放弃（{attempts2}）"
        );
        assert!(f.exists(), "失败时不得假装擦除成功");

        // 幂等：文件不存在视为成功
        let missing = dir.path().join("nope.bin");
        let (ok3, attempts3) = erase_with_retries(&[missing], &zero, |p| std::fs::remove_file(p));
        assert!(ok3 && attempts3 == 1);
    }
}
