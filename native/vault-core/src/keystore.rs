//! 保险箱头部（keystore）二进制格式。
//!
//! 版本字段遵循 docs/07 §4.1：
//! ```text
//! magic "VSVB" | format_ver u16 | kdf_ver u16 | cipher_suite u16 | chunk_cfg u16 | flags u16
//! argon: m_kib u32 | t u32 | p u32 | salt 16B
//! verifier 32B（HKDF(MK, "verifier")，防 MK 副本被篡改调包）
//! MK_wrap_pwd: nonce 12B ‖ ct 48B（MK 32B + GCM tag 16B）
//! [flags.bit0] MK_wrap_bio: nonce 12B ‖ ct 48B
//! ```
//! 写入永远用引擎支持的最高格式版本；读取拒绝更高的 format_ver（向前拒绝），
//! 兼容"只增字段"式的次版本演进：未知尾部字节原样忽略。
use std::path::{Path, PathBuf};

use vault_crypto::aead::AES_GCM_NONCE_LEN;
use vault_crypto::kdf::Argon2Params;
use vault_crypto::KEY_LEN;

pub const MAGIC: &[u8; 4] = b"VSVB";

/// 引擎写入的最高格式版本（docs/07：当前 = 2）。
pub const FORMAT_VER: u16 = 2;
pub const KDF_VER: u16 = 1;
/// cipher_suite 1 = AES-256-GCM（docs/07 §4.1）
pub const CIPHER_SUITE: u16 = 1;

/// 定长切片转换：长度不符时返回格式错误（代替 expect，见 deny(expect_used)）。
fn try_into_arr<const N: usize>(s: &[u8]) -> Result<&[u8; N], &'static str> {
    s.try_into().map_err(|_| "truncated keystore header")
}

pub const FLAGS_BIO_BOUND: u16 = 0x0001;

/// 头部固定区大小（不含 MK_wrap_bio 可变区）。
pub const HEADER_LEN: usize = 4 + 2 * 5 + 4 * 3 + 16 + 32 + (AES_GCM_NONCE_LEN + KEY_LEN + 16);

#[derive(Clone, Debug)]
pub struct Keystore {
    pub path: PathBuf,
    pub argon: Argon2Params,
    pub salt: [u8; 16],
    /// HKDF(MK, "verifier")，解封后校验，防 wrapped 副本被调包。
    pub verifier: [u8; KEY_LEN],
    /// nonce ‖ ct‖tag
    pub mk_wrap_pwd: Vec<u8>,
    /// Some(nonce ‖ ct‖tag)：已绑定生物识别
    pub mk_wrap_bio: Option<Vec<u8>>,
}

impl Keystore {
    pub fn bio_bound(&self) -> bool {
        self.mk_wrap_bio.is_some()
    }

    /// 序列化为磁盘格式。写入永远使用 FORMAT_VER。
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(HEADER_LEN + 64);
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&FORMAT_VER.to_le_bytes());
        b.extend_from_slice(&KDF_VER.to_le_bytes());
        b.extend_from_slice(&CIPHER_SUITE.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes()); // chunk_cfg：P2 启用
        b.extend_from_slice(&(if self.bio_bound() { FLAGS_BIO_BOUND } else { 0 }).to_le_bytes());
        b.extend_from_slice(&self.argon.m_kib.to_le_bytes());
        b.extend_from_slice(&self.argon.t.to_le_bytes());
        b.extend_from_slice(&self.argon.p.to_le_bytes());
        b.extend_from_slice(&self.salt);
        b.extend_from_slice(&self.verifier);
        b.extend_from_slice(&self.mk_wrap_pwd);
        if let Some(bio) = &self.mk_wrap_bio {
            b.extend_from_slice(bio);
        }
        b
    }

    /// 反序列化：向前拒绝更高 format_ver；未知尾部字节忽略（次版本兼容）。
    pub fn from_bytes(path: impl Into<PathBuf>, data: &[u8]) -> Result<Self, &'static str> {
        let mut r = 0usize;
        let need = |n: usize, r: usize| -> Result<(), &'static str> {
            if data.len() < r + n {
                Err("truncated keystore header")
            } else {
                Ok(())
            }
        };
        need(4, r)?;
        if &data[r..r + 4] != MAGIC {
            return Err("bad magic: not a VaultSync vault");
        }
        r += 4;
        need(10, r)?;
        let format_ver = u16::from_le_bytes(*try_into_arr::<2>(&data[r..r + 2])?);
        let _kdf_ver = u16::from_le_bytes(*try_into_arr::<2>(&data[r + 2..r + 4])?);
        let _cipher = u16::from_le_bytes(*try_into_arr::<2>(&data[r + 4..r + 6])?);
        let _chunk = u16::from_le_bytes(*try_into_arr::<2>(&data[r + 6..r + 8])?);
        let flags = u16::from_le_bytes(*try_into_arr::<2>(&data[r + 8..r + 10])?);
        r += 10;
        if format_ver > FORMAT_VER {
            // 向前拒绝：引擎过旧，提示升级（docs/07 §4.1）
            return Err("vault format is newer than this engine; upgrade required");
        }
        need(12, r)?;
        let argon = Argon2Params {
            m_kib: u32::from_le_bytes(*try_into_arr::<4>(&data[r..r + 4])?),
            t: u32::from_le_bytes(*try_into_arr::<4>(&data[r + 4..r + 8])?),
            p: u32::from_le_bytes(*try_into_arr::<4>(&data[r + 8..r + 12])?),
        };
        r += 12;
        need(16 + 32 + AES_GCM_NONCE_LEN + KEY_LEN + 16, r)?;
        let salt: [u8; 16] = *try_into_arr::<16>(&data[r..r + 16])?;
        r += 16;
        let verifier: [u8; KEY_LEN] = *try_into_arr::<32>(&data[r..r + 32])?;
        r += 32;
        let mk_wrap_pwd = data[r..r + AES_GCM_NONCE_LEN + KEY_LEN + 16].to_vec();
        r += AES_GCM_NONCE_LEN + KEY_LEN + 16;
        let mk_wrap_bio = if flags & FLAGS_BIO_BOUND != 0 {
            need(AES_GCM_NONCE_LEN + KEY_LEN + 16, r)?;
            let blob = data[r..r + AES_GCM_NONCE_LEN + KEY_LEN + 16].to_vec();
            Some(blob)
        } else {
            None
        };
        Ok(Self {
            path: path.into(),
            argon,
            salt,
            verifier,
            mk_wrap_pwd,
            mk_wrap_bio,
        })
    }

    pub fn load(path: &Path) -> Result<Self, &'static str> {
        let data = std::fs::read(path).map_err(|_| "cannot read vault file")?;
        Self::from_bytes(path, &data)
    }

    pub fn save(&self) -> Result<(), &'static str> {
        std::fs::write(&self.path, self.to_bytes()).map_err(|_| "cannot write vault file")
    }

    /// keystore 文件是否已存在。
    pub fn exists(path: &Path) -> bool {
        path.is_file()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn sample() -> Keystore {
        Keystore {
            path: PathBuf::from("/tmp/x.vsvb"),
            argon: Argon2Params::for_test(),
            salt: [1; 16],
            verifier: [2; 32],
            mk_wrap_pwd: vec![3; AES_GCM_NONCE_LEN + KEY_LEN + 16],
            mk_wrap_bio: Some(vec![4; AES_GCM_NONCE_LEN + KEY_LEN + 16]),
        }
    }

    #[test]
    fn roundtrip_preserves_all_fields() {
        let ks = sample();
        let ks2 = match Keystore::from_bytes("/tmp/x.vsvb", &ks.to_bytes()) {
            Ok(k) => k,
            Err(e) => panic!("parse failed: {e}"),
        };
        assert!(ks2.bio_bound());
        assert_eq!(ks2.salt, ks.salt);
        assert_eq!(ks2.verifier, ks.verifier);
        assert_eq!(ks2.mk_wrap_pwd, ks.mk_wrap_pwd);
        assert_eq!(ks2.mk_wrap_bio, ks.mk_wrap_bio);
        assert_eq!(ks2.argon, ks.argon);
    }

    #[test]
    fn rejects_higher_format_version() {
        let mut b = sample().to_bytes();
        // format_ver 位于偏移 4（LE）
        b[4] = u16::to_le_bytes(FORMAT_VER + 1)[0];
        assert!(Keystore::from_bytes("/tmp/x", &b).is_err());
    }

    #[test]
    fn rejects_bad_magic_and_truncated() {
        assert!(Keystore::from_bytes("/tmp/x", b"XXXX....").is_err());
        assert!(Keystore::from_bytes("/tmp/x", b"VSVB").is_err());
    }
}
