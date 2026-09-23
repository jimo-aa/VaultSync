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

    /// 配对短码（P8-4，docs/05-03 §3.3）：6 位十进制
    /// `HKDF(SHA256(pk_a‖pk_b‖role), "pair-code")[0..4] % 1_000_000`。
    ///
    /// 公钥对按字典序规范化（双端算得同值）；`role` 固定为配对会话语义
    /// （`vsync-pair`，双方对称）——比对一致即证明双方看到的公钥完全一致
    /// （中间人无法让两侧算出同一个码）。有效期 120 s 由调用方管理。
    pub fn match_code(pk_a_hex: &str, pk_b_hex: &str) -> Option<String> {
        let mut a = hex_decode(pk_a_hex)?;
        let mut b = hex_decode(pk_b_hex)?;
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        let mut pre = a;
        pre.extend_from_slice(&b);
        let digest = hash_sha256(&pre);
        let digest_raw = hex_decode(&digest)?;
        let key = vault_crypto::kdf::hkdf_sha256_derive(
            &<[u8; 32]>::try_from(digest_raw).ok()?,
            b"vsync-pair",
        );
        let v = u32::from_be_bytes([key[0], key[1], key[2], key[3]]) as u64 % 1_000_000;
        Some(format!("{v:06}"))
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

    /// P8-4 证据：短码双端独立计算一致（公钥顺序无关）、格式 6 位十进制、
    /// 不同公钥对不同短码（MITM 无法让两侧算出同码）。
    #[test]
    fn match_code_is_symmetric_and_deterministic() {
        let a = Identity::from_seed(&[1u8; 32]);
        let b = Identity::from_seed(&[2u8; 32]);
        let c = Identity::from_seed(&[3u8; 32]);
        let ab = Identity::match_code(&a.public_hex(), &b.public_hex()).unwrap();
        let ba = Identity::match_code(&b.public_hex(), &a.public_hex()).unwrap();
        assert_eq!(ab, ba, "公钥顺序无关（规范化后双端同码）");
        assert_eq!(ab.len(), 6, "6 位十进制");
        assert!(ab.chars().all(|ch| ch.is_ascii_digit()));
        let ac = Identity::match_code(&a.public_hex(), &c.public_hex()).unwrap();
        assert_ne!(ab, ac, "不同公钥对不同短码");
        // 非法公钥 hex → None
        assert!(Identity::match_code("zz", &b.public_hex()).is_none());
    }
}
