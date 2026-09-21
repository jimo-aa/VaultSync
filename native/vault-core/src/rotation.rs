//! 轮换会话（P7-3，docs/v2.0/05-01 §3.6 / §4.6、07 §三）。
//!
//! 状态机：`rotation_state`（VSVB v3 头部）0 idle / 1 in_progress /
//! 2 completed_pending_ack。**state 是唯一真值，VSRR 只是辅助凭据**：
//! - state=0：VSRR 一律视为垃圾（丢弃，诊断 `rotation.orphanRecoverRemoved`）；
//! - state=1：缺 VSRR / digest 不符 / rotation_id 不一致 → 码 11（不猜测）；
//! - state=2：VSRR 是残留（删除即可）。
//!
//! 恢复文件 `rotation.recover`（"VSRR"，114B 定长）：
//! `magic 4B | ver u16 | rotation_id 16B | nonce 12B | ct 48B | digest 32B`，
//! ct = 新 MK'（32B+tag）被 `HKDF(MK_old,"rotation-recover")` 包装。
#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use vault_crypto::aead::AES_GCM_NONCE_LEN;
use vault_crypto::{aead_decrypt, aead_encrypt, hash_sha256_bytes, random_bytes, KEY_LEN};

pub const VSRR_MAGIC: &[u8; 4] = b"VSRR";
pub const VSRR_LEN: usize = 114;
pub const ROTATION_RECOVER_INFO: &[u8] = b"rotation-recover";

/// 自动续做上限：同一 rotation_id 最多自动续做 3 次，第 4 次需显式确认
/// （`autoResumeExhausted=true`）。
pub const AUTO_RESUME_LIMIT: u32 = 3;

#[derive(Debug)]
pub enum VsrrError {
    Missing,
    Corrupt(&'static str),
}

pub struct Vsrr {
    pub rotation_id: [u8; 16],
    pub nonce: [u8; 12],
    /// 新 MK' 的包装（48B = 32B + tag；nonce 单独存字段）
    pub blob: [u8; 48],
}

impl Vsrr {
    /// 用旧 MK 解封得 MK'；解封失败（两端旧 MK 不同 / 篡改）→ Err。
    pub fn unwrap_mk(&self, mk_old: &[u8; KEY_LEN]) -> Result<[u8; KEY_LEN], &'static str> {
        let wrap_key = vault_crypto::kdf::hkdf_sha256_derive(mk_old, ROTATION_RECOVER_INFO);
        let mut input = Vec::with_capacity(60);
        input.extend_from_slice(&self.nonce);
        input.extend_from_slice(&self.blob);
        aead_decrypt(&wrap_key, &input)
            .as_deref()
            .and_then(|p| p.try_into().ok())
            .ok_or("vsrr mk unwrap failed")
    }
}

pub fn vsrr_path(data_dir: &Path) -> PathBuf {
    data_dir.join("rotation.recover")
}

/// 写 VSRR（tmp+rename 原子；写入失败 = begin 以码 3 失败，不进入轮换）。
pub fn write_vsrr(
    data_dir: &Path,
    rotation_id: &[u8; 16],
    mk_new: &[u8; KEY_LEN],
    mk_old: &[u8; KEY_LEN],
) -> Result<(), &'static str> {
    let wrap_key = vault_crypto::kdf::hkdf_sha256_derive(mk_old, ROTATION_RECOVER_INFO);
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&random_bytes(12));
    // 规范：ct 字段 48B（32B 密钥 + 16B tag），nonce 单独存 → 剥离返回值的 nonce 前缀
    let full = aead_encrypt(&wrap_key, &nonce, mk_new).map_err(|_| "vsrr encrypt failed")?;
    let blob: [u8; 48] = full[AES_GCM_NONCE_LEN..]
        .try_into()
        .map_err(|_| "vsrr encrypt failed")?;

    let mut b = Vec::with_capacity(VSRR_LEN);
    b.extend_from_slice(VSRR_MAGIC);
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(rotation_id);
    b.extend_from_slice(&nonce);
    b.extend_from_slice(&blob);
    b.extend_from_slice(&hash_sha256_bytes(&b));
    assert_eq!(b.len(), VSRR_LEN);

    let path = vsrr_path(data_dir);
    std::fs::create_dir_all(data_dir).map_err(|_| "cannot create data dir")?;
    let tmp = data_dir.join("rotation.recover.tmp");
    std::fs::write(&tmp, &b).map_err(|_| "cannot write rotation recover")?;
    if let Ok(f) = std::fs::File::open(&tmp) {
        let _ = f.sync_all();
    }
    std::fs::rename(&tmp, &path).map_err(|_| "cannot commit rotation recover")
}

/// 读 VSRR；digest / rotation_id 不匹配 → Corrupt（调用方映射码 11）。
pub fn read_vsrr(data_dir: &Path) -> Result<Vsrr, VsrrError> {
    let raw = std::fs::read(vsrr_path(data_dir)).map_err(|_| VsrrError::Missing)?;
    if raw.len() != VSRR_LEN || &raw[0..4] != VSRR_MAGIC {
        return Err(VsrrError::Corrupt("bad vsrr magic or length"));
    }
    if u16::from_le_bytes(
        raw[4..6]
            .try_into()
            .map_err(|_| VsrrError::Corrupt("truncated"))?,
    ) != 1
    {
        return Err(VsrrError::Corrupt("vsrr ver unknown"));
    }
    let digest_at = VSRR_LEN - 32;
    let expected: [u8; 32] = raw[digest_at..]
        .try_into()
        .map_err(|_| VsrrError::Corrupt("truncated"))?;
    if hash_sha256_bytes(&raw[..digest_at]) != expected {
        return Err(VsrrError::Corrupt("vsrr digest mismatch"));
    }
    Ok(Vsrr {
        rotation_id: raw[6..22]
            .try_into()
            .map_err(|_| VsrrError::Corrupt("truncated"))?,
        nonce: raw[22..34]
            .try_into()
            .map_err(|_| VsrrError::Corrupt("truncated"))?,
        blob: raw[34..82]
            .try_into()
            .map_err(|_| VsrrError::Corrupt("truncated"))?,
    })
}

/// state=0 时的孤儿 VSRR 清理（state 是唯一真值）。
pub fn remove_orphan_vsrr(data_dir: &Path) -> bool {
    let p = vsrr_path(data_dir);
    if p.exists() {
        let _ = std::fs::remove_file(&p);
        return true;
    }
    false
}
