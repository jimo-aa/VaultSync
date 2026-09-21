//! 保险箱头部（keystore）二进制格式。
//!
//! 版本字段遵循 docs/v2.0/05-01 §3.4（VSVB v3）：
//! ```text
//! 固定区 96B:
//!   magic "VSVB" | format_ver u16 | kdf_ver u16 | cipher_suite u16 | chunk_cfg u16 | flags u16
//!   rotation_state u8 | reserved0 u8 | rotation_id 16B
//!   argon: m_kib u32 | t u32 | p u32 | salt 16B
//!   verifier 32B（HKDF(MK,"verifier")）
//!   derived_key_count u16（=8）| reserved1 u16
//! 从属密钥包装区 64B×N（flags.bit1）:
//!   每条 key_id u16 | wrap_alg u16 | nonce 12B | ct 48B（32B 密钥 + GCM tag）
//! MK_wrap_pwd 60B（nonce 12B ‖ ct 48B）
//! [flags.bit0] MK_wrap_bio 60B
//! ```
//! 写入永远用 v3（flags.bit1 必置位，否则按规范是自相矛盾头部）；
//! 读取兼容 v2（从属密钥按 §3.2 冻结公式在内存求得，`derived_keys_legacy=true`，
//! 首次写头部升 v3 并写入包装区）；读取拒绝更高 format_ver（向前拒绝）。
//!
//! 从属密钥取值由 CSPRNG 生成一次即固定，绑定关系 = 包装密钥
//! `DWK = HKDF(MK,"dk-wrap")`（不是取值 = HKDF(MK,label)）——
//! 这是「MK 轮换仅重包装」在数学上成立的前提（05-01 §3.3）。
use std::path::{Path, PathBuf};

use vault_crypto::aead::AES_GCM_NONCE_LEN;
use vault_crypto::kdf::Argon2Params;
use vault_crypto::KEY_LEN;

pub const MAGIC: &[u8; 4] = b"VSVB";

/// 引擎写入的最高格式版本（docs/v2.0/05-01 §3.4：当前 = 3）。
pub const FORMAT_VER: u16 = 3;
/// v2（V1.0）格式版本，兼容读。
pub const FORMAT_VER_V2: u16 = 2;
pub const KDF_VER: u16 = 1;
/// cipher_suite 1 = AES-256-GCM 经典（2 = 混合 KEM，P7-8）
pub const CIPHER_SUITE: u16 = 1;

pub const FLAGS_BIO_BOUND: u16 = 0x0001;
pub const FLAGS_DERIVED_KEYS: u16 = 0x0002;

/// 轮换状态（05-01 §3.6）：0 idle / 1 in_progress / 2 completed_pending_ack。
pub const ROTATION_IDLE: u8 = 0;
#[allow(dead_code)] // P7-3 轮换会话接线
pub const ROTATION_IN_PROGRESS: u8 = 1;
pub const ROTATION_COMPLETED_PENDING_ACK: u8 = 2;

/// 单条从属密钥包装：key_id u16 ‖ wrap_alg u16 ‖ nonce 12B ‖ ct 48B。
pub const DERIVED_KEY_WRAP_LEN: usize = 2 + 2 + AES_GCM_NONCE_LEN + KEY_LEN + 16;
/// wrap_alg = 1：AES-GCM under DWK（2–65535 保留给外部包装）。
pub const WRAP_ALG_DWK: u16 = 1;
/// 从属密钥条数（key_id 1–8；9–255 保留）。
pub const DERIVED_KEY_COUNT: usize = 8;

/// key_id 枚举（docs/v2.0/05-01 §3.2）。
pub mod key_id {
    #![allow(dead_code)] // P7-3 起逐位接线；先按规范冻结枚举
    pub const INDEX: u16 = 1;
    pub const SEARCH: u16 = 2;
    pub const SHARE: u16 = 3;
    pub const ORDERS: u16 = 4;
    pub const STEGO: u16 = 5;
    pub const AUDIT_CHAIN: u16 = 6;
    pub const AUDIT_EXPORT: u16 = 7;
    pub const DISCOVERY_SALT: u16 = 8;
}

/// v3 固定区大小。
pub const HEADER_V3_FIXED_LEN: usize = 4 + 2 * 5 + 1 + 1 + 16 + 4 * 3 + 16 + 32 + 2 + 2; // = 96

/// 一条从属密钥的包装记录。
#[derive(Clone, Debug)]
pub struct DerivedKeyWrap {
    pub key_id: u16,
    pub wrap_alg: u16,
    /// nonce 12B ‖ ct 48B
    pub blob: Vec<u8>,
}

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
    /// 轮换状态机（v3）。
    pub rotation_state: u8,
    /// 本轮轮换标识（rotation_state != 0 时有效，否则全 0）。
    pub rotation_id: [u8; 16],
    /// 从属密钥包装区（v3 必有；v2 兼容读时为 None → 按冻结公式内存求得）。
    pub derived_keys: Option<Vec<DerivedKeyWrap>>,
}

/// 兼容读结果元信息（诊断用）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadInfo {
    pub format_ver: u16,
    /// 从属密钥是否来自 V1.0 冻结公式的内存求值（v2 兼容读）。
    pub derived_keys_legacy: bool,
}

impl Default for ReadInfo {
    fn default() -> Self {
        ReadInfo {
            format_ver: FORMAT_VER,
            derived_keys_legacy: false,
        }
    }
}

fn try_into_arr<const N: usize>(s: &[u8]) -> Result<&[u8; N], &'static str> {
    s.try_into().map_err(|_| "truncated keystore header")
}

impl Keystore {
    pub fn bio_bound(&self) -> bool {
        self.mk_wrap_bio.is_some()
    }

    /// v1.0 冻结公式：8 条从属密钥的兼容取值（05-01 §3.2 表）。
    /// 仅用于 v2 兼容读；写入 v3 时按此值进包装区（derivedKeysLegacy=true）。
    pub fn legacy_derived_keys(mk: &[u8; KEY_LEN]) -> [[u8; KEY_LEN]; DERIVED_KEY_COUNT] {
        let d = |info: &[u8]| *vault_crypto::kdf::hkdf_sha256_derive(mk, info);
        [
            d(b"vault-index"),   // 1 索引
            d(b"search"),        // 2 搜索
            d(b"search"),        // 3 分享（V1.0 复用搜索密钥）
            d(b"p2p-orders"),    // 4 订单
            d(b"stego-payload"), // 5 隐写载荷
            d(b"audit-log"),     // 6 链键
            d(b"audit-export"),  // 7 审计导出
            d(b"p2p-ident"),     // 8 发现指纹盐
        ]
    }

    /// 用 DWK 解封包装区 → 8 条从属密钥（按下标 key_id-1）。
    /// 缺失 / wrap_alg 未知 / 解封失败 → Err（**禁止**静默回退派生，05-01 §3.5）。
    pub fn unwrap_derived_keys(
        &self,
        mk: &[u8; KEY_LEN],
    ) -> Result<[[u8; KEY_LEN]; DERIVED_KEY_COUNT], &'static str> {
        let wraps = self
            .derived_keys
            .as_ref()
            .ok_or("derived key region missing (v3 header must carry it)")?;
        let dwk = vault_crypto::kdf::hkdf_sha256_derive(mk, b"dk-wrap");
        let mut out = [[0u8; KEY_LEN]; DERIVED_KEY_COUNT];
        let mut seen = [false; DERIVED_KEY_COUNT];
        for w in wraps {
            if !(1..=8).contains(&w.key_id) || w.key_id as usize > DERIVED_KEY_COUNT {
                return Err("derived key_id out of range");
            }
            if w.wrap_alg != WRAP_ALG_DWK {
                return Err("unknown derived key wrap_alg");
            }
            if w.blob.len() != AES_GCM_NONCE_LEN + KEY_LEN + 16 {
                return Err("derived key wrap truncated");
            }
            let pt = vault_crypto::aead::aead_decrypt(&dwk, &w.blob)
                .ok_or("derived key unwrap failed (wrong mk or tampered)")?;
            let key: [u8; KEY_LEN] = pt.try_into().map_err(|_| "derived key length")?;
            out[w.key_id as usize - 1] = key;
            seen[w.key_id as usize - 1] = true;
        }
        if seen.iter().any(|s| !s) {
            return Err("derived key region incomplete (missing key_id)");
        }
        Ok(out)
    }

    /// 把 8 条从属密钥用 `HKDF(MK,"dk-wrap")` 包装成头部包装区。
    /// `keys = None` 表示生成全新 CSPRNG 取值（首启）；Some = 既有取值（v2 升级 / 重包装）。
    pub fn wrap_derived_keys(
        mk: &[u8; KEY_LEN],
        keys: Option<[[u8; KEY_LEN]; DERIVED_KEY_COUNT]>,
    ) -> (Vec<DerivedKeyWrap>, [[u8; KEY_LEN]; DERIVED_KEY_COUNT]) {
        let keys = keys.unwrap_or_else(|| core::array::from_fn(|_| vault_crypto::random_key()));
        let dwk = vault_crypto::kdf::hkdf_sha256_derive(mk, b"dk-wrap");
        let wraps = (0..DERIVED_KEY_COUNT)
            .map(|i| {
                let mut nonce = [0u8; AES_GCM_NONCE_LEN];
                nonce.copy_from_slice(&vault_crypto::random_bytes(AES_GCM_NONCE_LEN));
                let blob = match vault_crypto::aead::aead_encrypt(&dwk, &nonce, &keys[i]) {
                    Ok(b) => b,
                    // AEAD 对 32B 明文 + 随机 nonce 加密不会失败（key/nonce 长度由类型保证）
                    Err(_) => unreachable!("aead encrypt infallible"),
                };
                DerivedKeyWrap {
                    key_id: i as u16 + 1,
                    wrap_alg: WRAP_ALG_DWK,
                    blob,
                }
            })
            .collect();
        (wraps, keys)
    }

    /// 序列化为磁盘格式。写入永远使用 FORMAT_VER（v3），flags.bit1 必置位。
    ///
    /// # Panics
    /// `derived_keys` 为 None（v3 头部必须有从属密钥包装区）——这是构造期
    /// 不变量，调用方全部经 `wrap_derived_keys` 构造；测试可直接构造。
    pub fn to_bytes(&self) -> Vec<u8> {
        let wraps = match self.derived_keys.as_ref() {
            Some(w) => w,
            None => panic!("v3 header requires derived key region"),
        };
        let mut b =
            Vec::with_capacity(HEADER_V3_FIXED_LEN + wraps.len() * DERIVED_KEY_WRAP_LEN + 120);
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&FORMAT_VER.to_le_bytes());
        b.extend_from_slice(&KDF_VER.to_le_bytes());
        b.extend_from_slice(&CIPHER_SUITE.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes()); // chunk_cfg：P2 启用
        let mut flags = FLAGS_DERIVED_KEYS;
        if self.bio_bound() {
            flags |= FLAGS_BIO_BOUND;
        }
        b.extend_from_slice(&flags.to_le_bytes());
        b.push(self.rotation_state);
        b.push(0u8); // reserved0
        b.extend_from_slice(&self.rotation_id);
        b.extend_from_slice(&self.argon.m_kib.to_le_bytes());
        b.extend_from_slice(&self.argon.t.to_le_bytes());
        b.extend_from_slice(&self.argon.p.to_le_bytes());
        b.extend_from_slice(&self.salt);
        b.extend_from_slice(&self.verifier);
        b.extend_from_slice(&(wraps.len() as u16).to_le_bytes()); // derived_key_count
        b.extend_from_slice(&0u16.to_le_bytes()); // reserved1
        for w in wraps {
            b.extend_from_slice(&w.key_id.to_le_bytes());
            b.extend_from_slice(&w.wrap_alg.to_le_bytes());
            b.extend_from_slice(&w.blob);
        }
        b.extend_from_slice(&self.mk_wrap_pwd);
        if let Some(bio) = &self.mk_wrap_bio {
            b.extend_from_slice(bio);
        }
        b
    }

    /// 反序列化：v3 主路径 + v2 兼容读（四情形，05-01 §3.5）。
    /// 返回 (Keystore, ReadInfo)。
    pub fn from_bytes_with_info(
        path: impl Into<PathBuf>,
        data: &[u8],
    ) -> Result<(Self, ReadInfo), &'static str> {
        let need = |n: usize, r: usize| -> Result<(), &'static str> {
            if data.len() < r + n {
                Err("truncated keystore header")
            } else {
                Ok(())
            }
        };
        need(14, 0)?;
        if &data[0..4] != MAGIC {
            return Err("bad magic: not a VaultSync vault");
        }
        let format_ver = u16::from_le_bytes(*try_into_arr::<2>(&data[4..6])?);
        if format_ver > FORMAT_VER {
            return Err("vault format is newer than this engine; upgrade required");
        }
        if format_ver == FORMAT_VER_V2 {
            return Self::parse_v2(path, data);
        }
        Self::parse_v3(path, data, format_ver)
    }

    pub fn from_bytes(path: impl Into<PathBuf>, data: &[u8]) -> Result<Self, &'static str> {
        Self::from_bytes_with_info(path, data).map(|(k, _)| k)
    }

    /// v2 兼容读：无从属密钥区 / rotation 字段；derived_keys 置 None，
    /// 调用方用 `legacy_derived_keys(mk)` 内存求值（ReadInfo.derived_keys_legacy）。
    fn parse_v2(path: impl Into<PathBuf>, data: &[u8]) -> Result<(Self, ReadInfo), &'static str> {
        let mut r = 4usize;
        let need = |n: usize, r: usize| -> Result<(), &'static str> {
            if data.len() < r + n {
                Err("truncated keystore header")
            } else {
                Ok(())
            }
        };
        need(10, r)?;
        let _kdf_ver = u16::from_le_bytes(*try_into_arr::<2>(&data[r..r + 2])?);
        let _cipher = u16::from_le_bytes(*try_into_arr::<2>(&data[r + 2..r + 4])?);
        let _chunk = u16::from_le_bytes(*try_into_arr::<2>(&data[r + 4..r + 6])?);
        let flags = u16::from_le_bytes(*try_into_arr::<2>(&data[r + 6..r + 8])?);
        r += 10;
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
            Some(data[r..r + AES_GCM_NONCE_LEN + KEY_LEN + 16].to_vec())
        } else {
            None
        };
        Ok((
            Self {
                path: path.into(),
                argon,
                salt,
                verifier,
                mk_wrap_pwd,
                mk_wrap_bio,
                rotation_state: ROTATION_IDLE,
                rotation_id: [0u8; 16],
                derived_keys: None,
            },
            ReadInfo {
                format_ver: FORMAT_VER_V2,
                derived_keys_legacy: true,
            },
        ))
    }

    /// v3 主路径。flags.bit1 未置位 = 自相矛盾头部 → 6（禁止静默回退派生）。
    fn parse_v3(
        path: impl Into<PathBuf>,
        data: &[u8],
        format_ver: u16,
    ) -> Result<(Self, ReadInfo), &'static str> {
        let need = |n: usize, r: usize| -> Result<(), &'static str> {
            if data.len() < r + n {
                Err("truncated keystore header")
            } else {
                Ok(())
            }
        };
        let _ = format_ver;
        // flags @12..14（规范 §3.4）：bit1 从属密钥区；bit2–15 保留必须 0
        let flags = u16::from_le_bytes(*try_into_arr::<2>(&data[12..14])?);
        if flags & FLAGS_DERIVED_KEYS == 0 {
            return Err("v3 header without derived key region (refusing silent fallback)");
        }
        if flags & !0x0003 != 0 {
            return Err("v3 header has reserved flag bits set");
        }
        let rotation_state = data[14];
        if rotation_state > ROTATION_COMPLETED_PENDING_ACK {
            return Err("unknown rotation_state");
        }
        let rotation_id: [u8; 16] = *try_into_arr::<16>(&data[16..32])?;
        need(HEADER_V3_FIXED_LEN - 32, 32)?;
        let argon = Argon2Params {
            m_kib: u32::from_le_bytes(*try_into_arr::<4>(&data[32..36])?),
            t: u32::from_le_bytes(*try_into_arr::<4>(&data[36..40])?),
            p: u32::from_le_bytes(*try_into_arr::<4>(&data[40..44])?),
        };
        let salt: [u8; 16] = *try_into_arr::<16>(&data[44..60])?;
        let verifier: [u8; KEY_LEN] = *try_into_arr::<32>(&data[60..92])?;
        let count = u16::from_le_bytes(*try_into_arr::<2>(&data[92..94])?) as usize;
        // reserved1 @94..96 必须 0
        if u16::from_le_bytes(*try_into_arr::<2>(&data[94..96])?) != 0 {
            return Err("reserved1 must be zero");
        }
        let mut r = HEADER_V3_FIXED_LEN;
        let mut wraps = Vec::with_capacity(count);
        for _ in 0..count {
            need(DERIVED_KEY_WRAP_LEN, r)?;
            let key_id = u16::from_le_bytes(*try_into_arr::<2>(&data[r..r + 2])?);
            let wrap_alg = u16::from_le_bytes(*try_into_arr::<2>(&data[r + 2..r + 4])?);
            let blob = data[r + 4..r + DERIVED_KEY_WRAP_LEN].to_vec();
            wraps.push(DerivedKeyWrap {
                key_id,
                wrap_alg,
                blob,
            });
            r += DERIVED_KEY_WRAP_LEN;
        }
        // key_id 完整性：1–8 各出现且仅一次（缺失 → 6，诊断列缺失项）
        let mut have = [false; DERIVED_KEY_COUNT];
        for w in &wraps {
            if !(1..=8).contains(&w.key_id) {
                return Err("derived key_id out of range");
            }
            if have[w.key_id as usize - 1] {
                return Err("duplicate derived key_id");
            }
            have[w.key_id as usize - 1] = true;
        }
        if have.iter().any(|h| !h) {
            return Err("derived key region incomplete (missing key_id)");
        }
        need(AES_GCM_NONCE_LEN + KEY_LEN + 16, r)?;
        let mk_wrap_pwd = data[r..r + AES_GCM_NONCE_LEN + KEY_LEN + 16].to_vec();
        r += AES_GCM_NONCE_LEN + KEY_LEN + 16;
        let mk_wrap_bio = if flags & FLAGS_BIO_BOUND != 0 {
            need(AES_GCM_NONCE_LEN + KEY_LEN + 16, r)?;
            Some(data[r..r + AES_GCM_NONCE_LEN + KEY_LEN + 16].to_vec())
        } else {
            None
        };
        Ok((
            Self {
                path: path.into(),
                argon,
                salt,
                verifier,
                mk_wrap_pwd,
                mk_wrap_bio,
                rotation_state,
                rotation_id,
                derived_keys: Some(wraps),
            },
            ReadInfo {
                format_ver: FORMAT_VER,
                derived_keys_legacy: false,
            },
        ))
    }

    pub fn load(path: &Path) -> Result<Self, &'static str> {
        let data = std::fs::read(path).map_err(|_| "cannot read vault file")?;
        Self::from_bytes(path, &data)
    }

    /// 读取并返回兼容读元信息（P7-7 迁移工具判定 derivedKeysLegacy 用）。
    #[allow(dead_code)]
    pub fn load_with_info(path: &Path) -> Result<(Self, ReadInfo), &'static str> {
        let data = std::fs::read(path).map_err(|_| "cannot read vault file")?;
        Self::from_bytes_with_info(path, &data)
    }

    /// 原子写：tmp → fsync → rename。
    ///
    /// 保险箱头部是**唯一**能解封 MK 的地方，直接覆盖写一旦中途断电就会留下半截头部
    /// （旧密码、新密码都打不开）。MK 全库轮换会两次经过这里，故必须是原子的。
    pub fn save(&self) -> Result<(), &'static str> {
        let tmp = self.path.with_extension("vsvb.tmp");
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&tmp).map_err(|_| "cannot write vault file")?;
            f.write_all(&self.to_bytes())
                .map_err(|_| "cannot write vault file")?;
            f.sync_all().map_err(|_| "cannot sync vault file")?;
        }
        std::fs::rename(&tmp, &self.path).map_err(|_| "cannot commit vault file")
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
        let (wraps, _) = Keystore::wrap_derived_keys(&[9u8; KEY_LEN], None);
        Keystore {
            path: PathBuf::from("/tmp/x.vsvb"),
            argon: Argon2Params::for_test(),
            salt: [1; 16],
            verifier: [2; 32],
            mk_wrap_pwd: vec![3; AES_GCM_NONCE_LEN + KEY_LEN + 16],
            mk_wrap_bio: Some(vec![4; AES_GCM_NONCE_LEN + KEY_LEN + 16]),
            rotation_state: ROTATION_IDLE,
            rotation_id: [0u8; 16],
            derived_keys: Some(wraps),
        }
    }

    /// P7-1 证据：keystore::v3_roundtrip
    #[test]
    fn v3_roundtrip() {
        let mk = [7u8; KEY_LEN];
        let (wraps, keys) = Keystore::wrap_derived_keys(&mk, None);
        let mut ks = sample();
        ks.derived_keys = Some(wraps);
        let bytes = ks.to_bytes();
        // 布局断言：固定区 96B、format_ver=3、flags.bit1、derived_key_count=8
        assert_eq!(&bytes[0..4], b"VSVB");
        assert_eq!(u16::from_le_bytes(bytes[4..6].try_into().unwrap()), 3);
        assert_eq!(
            u16::from_le_bytes(bytes[12..14].try_into().unwrap()) & FLAGS_DERIVED_KEYS,
            FLAGS_DERIVED_KEYS
        );
        assert_eq!(u16::from_le_bytes(bytes[92..94].try_into().unwrap()), 8);
        let (ks2, info) = Keystore::from_bytes_with_info("/tmp/x", &bytes).unwrap();
        assert!(!info.derived_keys_legacy);
        assert_eq!(ks2.rotation_state, ROTATION_IDLE);
        let unwrapped = ks2.unwrap_derived_keys(&mk).unwrap();
        assert_eq!(unwrapped, keys);
        // 错误 MK → 解封失败（不回退派生）
        assert!(ks2.unwrap_derived_keys(&[8u8; KEY_LEN]).is_err());
    }

    /// P7-1 证据：keystore::missing_key_id_refuses_silent_fallback
    #[test]
    fn missing_key_id_refuses_silent_fallback() {
        let mut ks = sample();
        // v3 且 flags.bit1=0 → 自相矛盾头部 → 6
        let bytes = ks.to_bytes();
        let mut b = bytes.clone();
        let f = u16::from_le_bytes(b[12..14].try_into().unwrap()) & !FLAGS_DERIVED_KEYS;
        b[12..14].copy_from_slice(&f.to_le_bytes());
        let err = Keystore::from_bytes("/tmp/x", &b).unwrap_err();
        assert!(err.contains("refusing silent fallback"), "{err}");
        // 缺一条 key_id → 6
        ks.derived_keys.as_mut().unwrap().pop();
        let err2 = Keystore::from_bytes("/tmp/x", &ks.to_bytes()).unwrap_err();
        assert!(err2.contains("missing key_id"), "{err2}");
    }

    /// P7-1 证据：keystore::read_v2_header（v2 兼容读 + 首写升级）
    #[test]
    fn read_v2_header() {
        let mk = [5u8; KEY_LEN];
        // 手工封一个 v2 头部（旧布局：4 + 5×u16 + argon + salt + verifier + pwd + bio）
        let mut b = Vec::new();
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&FORMAT_VER_V2.to_le_bytes());
        b.extend_from_slice(&KDF_VER.to_le_bytes());
        b.extend_from_slice(&CIPHER_SUITE.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&FLAGS_BIO_BOUND.to_le_bytes());
        b.extend_from_slice(&Argon2Params::for_test().m_kib.to_le_bytes());
        b.extend_from_slice(&Argon2Params::for_test().t.to_le_bytes());
        b.extend_from_slice(&Argon2Params::for_test().p.to_le_bytes());
        b.extend_from_slice(&[1u8; 16]);
        b.extend_from_slice(&[2u8; 32]);
        b.extend_from_slice(&[3u8; 60]);
        b.extend_from_slice(&[4u8; 60]);
        let (ks, info) = Keystore::from_bytes_with_info("/tmp/x", &b).unwrap();
        assert_eq!(info.format_ver, FORMAT_VER_V2);
        assert!(info.derived_keys_legacy);
        assert!(ks.derived_keys.is_none());
        // 冻结公式内存求值：与 V1.0 派生一致
        let legacy = Keystore::legacy_derived_keys(&mk);
        assert_eq!(
            legacy[0],
            *vault_crypto::kdf::hkdf_sha256_derive(&mk, b"vault-index")
        );
        assert_eq!(legacy[2], legacy[1], "分享复用搜索密钥（V1.0 冻结值）");
        // 首写升级：以冻结值进包装区，解封回同值（存量数据无需重写）
        let (wraps, keys) = Keystore::wrap_derived_keys(&mk, Some(legacy));
        assert_eq!(keys, legacy);
        let upgraded = Keystore {
            derived_keys: Some(wraps),
            ..ks
        };
        let (rt, info2) = Keystore::from_bytes_with_info("/tmp/x", &upgraded.to_bytes()).unwrap();
        assert!(!info2.derived_keys_legacy);
        assert_eq!(rt.unwrap_derived_keys(&mk).unwrap(), legacy);
    }

    #[test]
    fn rejects_higher_format_version() {
        let mut b = sample().to_bytes();
        b[4] = u16::to_le_bytes(FORMAT_VER + 1)[0];
        assert!(Keystore::from_bytes("/tmp/x", &b).is_err());
    }

    #[test]
    fn rejects_bad_magic_and_truncated() {
        assert!(Keystore::from_bytes("/tmp/x", b"XXXX....").is_err());
        assert!(Keystore::from_bytes("/tmp/x", b"VSVB").is_err());
    }

    #[test]
    fn rotation_fields_roundtrip() {
        let mut ks = sample();
        ks.rotation_state = ROTATION_IN_PROGRESS;
        ks.rotation_id = [0xAB; 16];
        let ks2 = Keystore::from_bytes("/tmp/x", &ks.to_bytes()).unwrap();
        assert_eq!(ks2.rotation_state, ROTATION_IN_PROGRESS);
        assert_eq!(ks2.rotation_id, [0xAB; 16]);
        let mut ks3 = sample();
        ks3.rotation_state = 9;
        let raw = ks3.to_bytes();
        assert!(Keystore::from_bytes("/tmp/x", &raw).is_err());
    }
}
