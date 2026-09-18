//! 设备身份密钥对（Ed25519，docs/05-03 §三：双向认证 + 指纹核验）。
//!
//! 私钥不出引擎；跨 FFI 只暴露 device_id / 指纹 / 公钥。
//! Noise 信道用 X25519 静态密钥，由 Ed25519 种子确定性换算（同一身份两用）。
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use vault_crypto::{hash_sha256, hex_decode, hex_encode, random_bytes};

pub const SEED_LEN: usize = 32;

pub struct Identity {
    sk: SigningKey,
}

impl Identity {
    /// CSPRNG 生成新身份。
    pub fn generate() -> Self {
        let seed = random_bytes(SEED_LEN);
        let mut arr = [0u8; SEED_LEN];
        arr.copy_from_slice(&seed[..SEED_LEN]);
        Self::from_seed(&arr)
    }

    pub fn from_seed(seed: &[u8; SEED_LEN]) -> Self {
        Self {
            sk: SigningKey::from_bytes(seed),
        }
    }

    /// 私钥种子（用于加密落盘；绝不跨 FFI）。
    pub fn seed(&self) -> [u8; SEED_LEN] {
        self.sk.to_bytes()
    }

    /// Ed25519 公钥 hex。
    pub fn public_hex(&self) -> String {
        hex_encode(&self.sk.verifying_key().to_bytes())
    }

    /// 设备 ID：sha256(pubkey) 前 8 字节 hex，前缀 `vd-`。
    pub fn device_id(&self) -> String {
        Self::device_id_of(&self.public_hex()).unwrap_or_default()
    }

    /// 由任意 Ed25519 公钥 hex 推导设备 ID（核验对端声明用）。
    pub fn device_id_of(pub_hex: &str) -> Option<String> {
        let pk = hex_decode(pub_hex)?;
        Some(format!("vd-{}", &hash_sha256(&pk)[..16]))
    }

    /// 由公钥 hex 推导人工核验指纹（对端登记表展示用）。
    pub fn fingerprint_of(pub_hex: &str) -> String {
        match hex_decode(pub_hex) {
            Some(pk) => {
                let h = &hash_sha256(&pk)[..16];
                h.as_bytes()
                    .chunks(4)
                    .map(|c| std::str::from_utf8(c).unwrap_or("?"))
                    .collect::<Vec<_>>()
                    .join(" ")
            }
            None => String::new(),
        }
    }

    /// 人工核验指纹：sha256(pubkey) 前 8 字节，按 4 字符分组（双方 UI 比对，docs/05-03 §3.2）。
    pub fn fingerprint(&self) -> String {
        let h = &hash_sha256(&self.sk.verifying_key().to_bytes())[..16];
        h.as_bytes()
            .chunks(4)
            .map(|c| std::str::from_utf8(c).unwrap_or("?"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn sign(&self, msg: &[u8]) -> String {
        hex_encode(&self.sk.sign(msg).to_bytes())
    }

    /// 公钥 hex + 签名 hex 验签。
    pub fn verify(pub_hex: &str, msg: &[u8], sig_hex: &str) -> bool {
        let (Some(pk), Some(sig)) = (hex_decode(pub_hex), hex_decode(sig_hex)) else {
            return false;
        };
        let Ok(pk_arr) = <[u8; 32]>::try_from(pk.as_slice()) else {
            return false;
        };
        let (Ok(vk), Ok(sig)) = (
            VerifyingKey::from_bytes(&pk_arr),
            Signature::from_slice(&sig),
        ) else {
            return false;
        };
        vk.verify(msg, &sig).is_ok()
    }

    /// Noise 静态私钥：由 Ed25519 种子确定性派生的独立 X25519 密钥
    ///（ed25519 标量与 X25519 钳位规则不同，不得直接复用）。
    pub(crate) fn x25519_priv(&self) -> [u8; 32] {
        let h = hash_sha256(format!("vsync-x25519:{}", hex_encode(&self.seed())).as_bytes());
        let mut k = [0u8; 32];
        for (i, c) in h.as_bytes().chunks(2).enumerate() {
            let hi = (c[0] as char).to_digit(16).unwrap_or(0) as u8;
            let lo = (c[1] as char).to_digit(16).unwrap_or(0) as u8;
            k[i] = (hi << 4) | lo;
        }
        k
    }

    /// 本机 X25519 静态公钥（Hello 中携带并对握手哈希签名，绑定到 Ed25519 身份）。
    pub fn x25519_pub(&self) -> [u8; 32] {
        let secret = x25519_dalek::StaticSecret::from(self.x25519_priv());
        x25519_dalek::PublicKey::from(&secret).to_bytes()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_keys_and_fingerprint() {
        let seed = [7u8; 32];
        let a = Identity::from_seed(&seed);
        let b = Identity::from_seed(&seed);
        assert_eq!(a.public_hex(), b.public_hex());
        assert_eq!(a.device_id(), b.device_id());
        assert_eq!(a.fingerprint().len(), 19); // 16 hex + 3 空格
        assert!(a.device_id().starts_with("vd-"));
    }

    #[test]
    fn sign_verify_roundtrip_and_tamper() {
        let id = Identity::generate();
        let sig = id.sign(b"hello");
        assert!(Identity::verify(&id.public_hex(), b"hello", &sig));
        assert!(!Identity::verify(&id.public_hex(), b"hell0", &sig));
        let other = Identity::generate();
        assert!(!Identity::verify(&other.public_hex(), b"hello", &sig));
    }

    #[test]
    fn x25519_derivation_is_deterministic() {
        let a = Identity::from_seed(&[9u8; 32]);
        let b = Identity::from_seed(&[9u8; 32]);
        assert_eq!(a.x25519_priv(), b.x25519_priv());
        assert_eq!(a.x25519_pub(), b.x25519_pub());
        let c = Identity::generate();
        assert_ne!(a.x25519_pub(), c.x25519_pub());
    }
}
