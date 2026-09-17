//! AES-256-GCM 认证加密（docs/05-01：AEAD(KEK, MK)，cipher_suite = 1）。
use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};

use crate::KEY_LEN;

pub const AES_GCM_NONCE_LEN: usize = 12;
pub const AES_GCM_TAG_LEN: usize = 16;

/// 加密：返回 nonce ‖ ciphertext ‖ tag。
/// `nonce` 必须为 12B；同一 key 下 nonce 绝不复用（由调用方保证随机生成）。
pub fn aead_encrypt(
    key: &[u8; KEY_LEN],
    nonce: &[u8; AES_GCM_NONCE_LEN],
    plaintext: &[u8],
) -> Result<Vec<u8>, &'static str> {
    aead_encrypt_with_aad(key, nonce, plaintext, &[])
}

/// 带 AAD 加密（docs/05-02：块密文以块序号为 AAD，防块重排/移植）。
/// 返回 nonce ‖ ciphertext ‖ tag。
pub fn aead_encrypt_with_aad(
    key: &[u8; KEY_LEN],
    nonce: &[u8; AES_GCM_NONCE_LEN],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, &'static str> {
    let cipher = Aes256Gcm::new(key.into());
    let ct = cipher
        .encrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| "aes-gcm encrypt failed")?;
    let mut out = Vec::with_capacity(AES_GCM_NONCE_LEN + ct.len());
    out.extend_from_slice(nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// 解密：输入 nonce ‖ ciphertext ‖ tag；tag 校验失败（密钥错误 / 篡改）返回 None。
pub fn aead_decrypt(key: &[u8; KEY_LEN], blob: &[u8]) -> Option<Vec<u8>> {
    aead_decrypt_with_aad(key, blob, &[])
}

/// 带 AAD 解密；tag 校验失败（密钥错误 / 篡改 / AAD 不符）返回 None。
pub fn aead_decrypt_with_aad(key: &[u8; KEY_LEN], blob: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
    if blob.len() < AES_GCM_NONCE_LEN + AES_GCM_TAG_LEN {
        return None;
    }
    let (nonce, ct) = blob.split_at(AES_GCM_NONCE_LEN);
    let cipher = Aes256Gcm::new(key.into());
    cipher
        .decrypt(Nonce::from_slice(nonce), Payload { msg: ct, aad })
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::random::{random_bytes, random_key};

    #[test]
    fn roundtrip_and_tamper() {
        let key = random_key();
        let mut nonce = [0u8; AES_GCM_NONCE_LEN];
        nonce.copy_from_slice(&random_bytes(AES_GCM_NONCE_LEN));
        let pt = b"master key material 32 bytes!!!!";
        let blob = match aead_encrypt(&key, &nonce, pt) {
            Ok(b) => b,
            Err(e) => panic!("encrypt failed: {e}"),
        };
        assert_eq!(blob.len(), AES_GCM_NONCE_LEN + pt.len() + AES_GCM_TAG_LEN);
        assert_eq!(aead_decrypt(&key, &blob).as_deref(), Some(&pt[..]));

        // 篡改密文任一字节 → GCM tag 校验失败
        let mut tampered = blob.clone();
        tampered[AES_GCM_NONCE_LEN] ^= 1;
        assert_eq!(aead_decrypt(&key, &tampered), None);

        // 错误密钥 → None
        assert_eq!(aead_decrypt(&random_key(), &blob), None);
    }

    #[test]
    fn too_short_blob_rejected() {
        let key = random_key();
        assert_eq!(aead_decrypt(&key, &[0u8; 20]), None);
    }
}
