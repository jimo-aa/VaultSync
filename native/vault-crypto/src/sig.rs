//! Ed25519 签名校验（通用入口）。
//!
//! 与 `vault-p2p` 的设备身份签名同族，但**与 P2P 无关**：用于校验
//! 「更新清单」这类由发布方离线签名、客户端只做验签的产物（P5-7 分发通道）。
//! 本模块**只做验签**：签名私钥不出现在引擎里（发布方离线持有）。
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

/// 校验 `msg` 的 hex 签名是否由 `public_hex` 对应的私钥签发。
///
/// 任一输入格式非法（hex、长度、点不在曲线上）一律返回 `false`——调用方
/// 必须把它当作「不可信」，不要区分「格式错」与「签名错」。
pub fn verify_hex(public_hex: &str, msg: &[u8], sig_hex: &str) -> bool {
    let pk = match crate::hex_decode(public_hex) {
        Some(v) => v,
        None => return false,
    };
    let pk_arr: [u8; 32] = match pk.as_slice().try_into() {
        Ok(a) => a,
        Err(_) => return false,
    };
    let sig = match crate::hex_decode(sig_hex) {
        Some(v) => v,
        None => return false,
    };
    let sig = match Signature::from_slice(&sig) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let vk = match VerifyingKey::from_bytes(&pk_arr) {
        Ok(v) => v,
        Err(_) => return false,
    };
    vk.verify(msg, &sig).is_ok()
}

/// 计算文件的 SHA-256（hex）：更新包完整性校验用（清单里带 sha256）。
pub fn sha256_file_hex(path: &std::path::Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(crate::hex_encode(&hasher.finalize()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn keypair() -> (SigningKey, String) {
        // 用固定种子保证测试确定性
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let pk = sk.verifying_key().to_bytes();
        (sk, crate::hex_encode(&pk))
    }

    #[test]
    fn verify_accepts_valid_and_rejects_tampered() {
        let (sk, pk_hex) = keypair();
        let msg = b"{\"version\":\"0.6.0\",\"sha256\":\"ab\"}";
        let sig_hex = crate::hex_encode(&sk.sign(msg).to_bytes());
        assert!(verify_hex(&pk_hex, msg, &sig_hex));
        // 消息被改
        assert!(!verify_hex(&pk_hex, b"{\"version\":\"9.9.9\"}", &sig_hex));
        // 换公钥
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let other_pk = crate::hex_encode(&other.verifying_key().to_bytes());
        assert!(!verify_hex(&other_pk, msg, &sig_hex));
        // 非法输入一律 false（不 panic）
        assert!(!verify_hex("zz", msg, &sig_hex));
        assert!(!verify_hex(&pk_hex, msg, "00"));
        assert!(!verify_hex(&pk_hex, msg, &"0".repeat(128)));
    }

    #[test]
    fn sha256_file_matches_known_vector() {
        let dir = std::env::temp_dir().join(format!("vsync_sig_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let f = dir.join("a.bin");
        std::fs::write(&f, b"abc").expect("write");
        // FIPS 180-4 空输入与 "abc" 的已知向量
        assert_eq!(
            sha256_file_hex(&f).expect("hash"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
