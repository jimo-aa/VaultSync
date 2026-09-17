//! 密钥与密码学原语。
//!
//! 安全红线：本 crate 是全系统唯一允许实现加解密/密钥派生逻辑的底层模块；
//! 上层 crate 只能通过本 crate 暴露的 API 触碰密钥材料（docs/01、docs/06）。
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![deny(warnings)]

pub mod aead;
pub mod kdf;
pub mod random;

pub use aead::{
    aead_decrypt, aead_decrypt_with_aad, aead_encrypt, aead_encrypt_with_aad, AES_GCM_NONCE_LEN,
    AES_GCM_TAG_LEN,
};
pub use kdf::{argon2id_derive, hkdf_sha256_derive, hkdf_sha256_extract_expand, Argon2Params};
pub use random::{random_bytes, random_key, random_salt};

/// AES-256-GCM 密钥长度（docs/05-01：KEK_pwd / KEK_bio / MK 均 32B）。
pub const KEY_LEN: usize = 32;

/// SHA-256 增量哈希（整文件终验用，docs/05-02 §3.3）。封装 sha2，避免上层直接依赖。
pub struct Sha256 {
    inner: sha2::Sha256,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    pub fn new() -> Self {
        Self {
            inner: <sha2::Sha256 as sha2::Digest>::new(),
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        sha2::Digest::update(&mut self.inner, data);
    }

    /// 完成并输出十六进制摘要。
    pub fn finalize_hex(self) -> String {
        hex_encode(&sha2::Digest::finalize(self.inner))
    }
}

/// 一次性 SHA-256（十六进制）。
pub fn hash_sha256(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize_hex()
}

/// HMAC-SHA256（十六进制）。用于搜索令牌的确定性伪随机化（SSE-lite，docs/05-02 §4.4）。
pub fn hmac_sha256_hex(key: &[u8], data: &[u8]) -> String {
    let mut mac = <hmac::Hmac<sha2::Sha256> as hmac::Mac>::new_from_slice(key)
        .unwrap_or_else(|_| panic!("hmac accepts any key length"));
    hmac::Mac::update(&mut mac, data);
    hex_encode(&hmac::Mac::finalize(mac).into_bytes())
}

/// 引擎自检：跑一次 AEAD 加解密往返，确认密码学栈可用。
pub fn self_check() -> bool {
    let key = random::random_key();
    let nonce = [0x42u8; 12];
    match aead::aead_encrypt(&key, &nonce, b"self-check") {
        Ok(blob) => aead::aead_decrypt(&key, &blob).as_deref() == Some(b"self-check".as_slice()),
        Err(_) => false,
    }
}

/// 十六进制编码（用于把二进制密钥材料存入平台安全存储等字符串载体）。
pub fn hex_encode(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 2);
    for b in data {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// 十六进制解码；长度为奇数或含非 hex 字符时返回 None。
pub fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_ready_is_true() {
        assert!(random_bytes(8).len() == 8);
    }

    #[test]
    fn hex_roundtrip() {
        let data = [0u8, 0x0f, 0xa5, 0xff];
        let hex = hex_encode(&data);
        assert_eq!(hex, "000fa5ff");
        assert_eq!(hex_decode(&hex).as_deref(), Some(data.as_slice()));
        assert_eq!(hex_decode("0f5"), None);
        assert_eq!(hex_decode("zz"), None);
    }
}
