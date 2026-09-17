//! CSPRNG（docs/01 设计原则二：MK 由 CSPRNG 随机生成，不依赖密码派生）。
use rand::rngs::OsRng;
use rand::RngCore;

/// 密码学安全随机字节。
pub fn random_bytes(len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    OsRng.fill_bytes(&mut buf);
    buf
}

/// 16B Argon2id salt（docs/05-01 §3.2：每次设置随机）。
pub fn random_salt() -> [u8; 16] {
    let mut s = [0u8; 16];
    OsRng.fill_bytes(&mut s);
    s
}

/// 32B 密钥（MK / KEK_bio / FSKey）。
pub fn random_key() -> [u8; 32] {
    let mut k = [0u8; 32];
    OsRng.fill_bytes(&mut k);
    k
}
