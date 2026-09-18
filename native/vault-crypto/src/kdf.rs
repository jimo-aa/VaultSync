//! KDF：Argon2id（KEK_pwd 派生）与 HKDF-SHA256（FSK / verifier 派生）。
//! 参数与语义见 docs/05-01 §3.2、docs/README 术语表。
use argon2::{Algorithm, Argon2, ParamsBuilder, Version};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::KEY_LEN;

type HmacSha256 = Hmac<Sha256>;

/// Argon2id 参数（docs/05-01 §3.2，桌面首发基线：64 MiB / t=3 / p=2）。
/// 参数随保险箱头部明文区持久化，保证跨设备可校验同一密码。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Argon2Params {
    /// 内存 KiB（桌面 65536 = 64 MiB，移动端降级 16384）
    pub m_kib: u32,
    /// 迭代次数（3）
    pub t: u32,
    /// 并行度（桌面 2，移动端 1）
    pub p: u32,
}

impl Argon2Params {
    pub const DESKTOP: Self = Self {
        m_kib: 64 * 1024,
        t: 3,
        p: 2,
    };
    pub const MOBILE: Self = Self {
        m_kib: 16 * 1024,
        t: 3,
        p: 1,
    };

    /// 低成本参数：仅限测试使用，生产路径禁止。
    pub fn for_test() -> Self {
        Self {
            m_kib: 8 * 1024,
            t: 1,
            p: 1,
        }
    }
}

/// Argon2id 派生 32B KEK_pwd。返回值经 Zeroizing 包装，作用域结束即擦除。
pub fn argon2id_derive(
    password: &str,
    salt: &[u8; 16],
    params: &Argon2Params,
) -> Result<Zeroizing<[u8; KEY_LEN]>, &'static str> {
    // builder 形式与 Params::new 等价（m/t/p/输出长度），语义不变
    let a2params = ParamsBuilder::new()
        .m_cost(params.m_kib)
        .t_cost(params.t)
        .p_cost(params.p)
        .output_len(KEY_LEN)
        .build()
        .map_err(|_| "invalid argon2 params")?;
    let a2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, a2params);
    let mut okm = Zeroizing::new([0u8; KEY_LEN]);
    a2.hash_password_into(password.as_bytes(), salt, okm.as_mut())
        .map_err(|_| "argon2id derive failed")?;
    Ok(okm)
}

/// RFC 5869 第一步：PRK = HMAC-SHA256(salt, IKM)。
fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(salt)
        .unwrap_or_else(|_| panic!("hmac accepts any key length"));
    mac.update(ikm);
    finalize_32(&mut mac)
}

/// RFC 5869 第二步：OKM = HMAC 迭代（T1 = HMAC(PRK, info‖0x01)，Ti = HMAC(PRK, T(i-1)‖info‖i)）。
/// 输出长度以 32B 为上限，迭代次数恒为 1，越界在类型上不可表达。
fn hkdf_okm_32(prk: &[u8; 32], info: &[u8]) -> [u8; 32] {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(prk)
        .unwrap_or_else(|_| panic!("hmac accepts any key length"));
    mac.update(info);
    mac.update(&[0x01]);
    finalize_32(&mut mac)
}

fn finalize_32(mac: &mut HmacSha256) -> [u8; 32] {
    let out = mac.clone().finalize().into_bytes();
    out.as_slice()
        .try_into()
        .unwrap_or_else(|_| panic!("sha256 output is 32 bytes"))
}

/// HKDF-SHA256（RFC 5869）完整派生，输出 32B。
/// 用途：MK → FSK（info = 上下文串）、MK → verifier（info = "verifier"）。
/// 正确性由下方 RFC 5869 官方测试向量（A.1/A.2 案例）断言。
pub fn hkdf_sha256_extract_expand(
    ikm: &[u8],
    salt: &[u8],
    info: &[u8],
) -> Zeroizing<[u8; KEY_LEN]> {
    let prk = hkdf_extract(salt, ikm);
    Zeroizing::new(hkdf_okm_32(&prk, info))
}

/// 便捷形式：固定 salt 的 HKDF 派生（docs/05-01 §3.1 verifier 的用法）。
pub fn hkdf_sha256_derive(ikm: &[u8], info: &[u8]) -> Zeroizing<[u8; KEY_LEN]> {
    hkdf_sha256_extract_expand(ikm, b"VaultSync-HKDF-Salt-v1", info)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 5869 Appendix A. Test Case 1（SHA-256）
    #[test]
    fn rfc5869_test_vector_1() {
        let ikm = [0x0b_u8; 22];
        let salt = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
        ];
        let info = [
            0xf0_u8, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9,
        ];
        let okm = hkdf_sha256_extract_expand(&ikm, &salt, &info);
        let expected: [u8; 32] = [
            0x3c, 0xb2, 0x5f, 0x25, 0xfa, 0xac, 0xd5, 0x7a, 0x90, 0x43, 0x4f, 0x64, 0xd0, 0x36,
            0x2f, 0x2a, 0x2d, 0x2d, 0x0a, 0x90, 0xcf, 0x1a, 0x5a, 0x4c, 0x5d, 0xb0, 0x2d, 0x56,
            0xec, 0xc4, 0xc5, 0xbf,
        ];
        assert_eq!(okm.as_ref(), &expected);
    }

    /// RFC 5869 Test Case 3（零长度 salt / info）
    #[test]
    fn rfc5869_test_vector_3() {
        let ikm = [0x0b_u8; 22];
        let okm = hkdf_sha256_extract_expand(&ikm, &[], &[]);
        let expected: [u8; 32] = [
            0x8d, 0xa4, 0xe7, 0x75, 0xa5, 0x63, 0xc1, 0x8f, 0x71, 0x5f, 0x80, 0x2a, 0x06, 0x3c,
            0x5a, 0x31, 0xb8, 0xa1, 0x1f, 0x5c, 0x5e, 0xe1, 0x87, 0x9e, 0xc3, 0x45, 0x4e, 0x5f,
            0x3c, 0x73, 0x8d, 0x2d,
        ];
        assert_eq!(okm.as_ref(), &expected);
    }

    #[test]
    fn hkdf_context_separation() {
        let ikm = [1u8; 32];
        let fsk = hkdf_sha256_derive(&ikm, b"folder/2026");
        let fsk2 = hkdf_sha256_derive(&ikm, b"folder/2026");
        let other = hkdf_sha256_derive(&ikm, b"folder/2027");
        assert_eq!(fsk.as_ref(), fsk2.as_ref());
        assert_ne!(fsk.as_ref(), other.as_ref());
    }

    #[test]
    fn argon2id_deterministic() {
        let salt = [7u8; 16];
        let derive = |pwd: &str| argon2id_derive(pwd, &salt, &Argon2Params::for_test());
        let a = match derive("correct horse") {
            Ok(k) => k,
            Err(e) => panic!("derive failed: {e}"),
        };
        let b = match derive("correct horse") {
            Ok(k) => k,
            Err(e) => panic!("derive failed: {e}"),
        };
        assert_eq!(a.as_ref(), b.as_ref());
        let c = match derive("wrong") {
            Ok(k) => k,
            Err(e) => panic!("derive failed: {e}"),
        };
        assert_ne!(a.as_ref(), c.as_ref());
    }
}
