//! ML-KEM-768（FIPS 203）密钥封装原语（P7-8，docs/v2.0/05-04 §4.6、docs/v2.0/09）。
//!
//! 选型与风险登记（R2，docs/v2.0/11）：采用 RustCrypto `ml-kem`（纯 Rust，与 aes-gcm /
//! argon2 同族维护），**该实现尚未经过独立审计**。处置约束（缺一不可）：
//! 1. 仅作信道握手增强（`cipher_suite=2`），协商成功才启用，永不作默认；
//! 2. 初始化失败一律 fail-open 回落 suite 1，且降级必须可见（信道 note + 引擎日志）；
//! 3. 与 X25519 混合成双成份信道：会话数据机密性 = max(经典， 后量子)。
//!
//! 解封装密钥由 32B 身份种子**确定性展开**（与 X25519 静态密钥同法，identity.rs），
//! 不落盘、每次握手重派生；封装公钥/密文走线上帧，共享秘密绝不出本层接口之外的
//! 明文日志（返回值仅供调用方混入密钥派生）。
use ml_kem::{
    kem::{Decapsulate, Encapsulate, Key, KeyExport, TryKeyInit},
    DecapsulationKey, EncapsulationKey, MlKem768, Seed,
};
use rand_core::{Infallible, TryCryptoRng, TryRng};

use crate::hash_sha256_bytes;
use crate::random::random_bytes;

/// 封装公钥（线上帧）长度。
pub const MLKEM768_EK_LEN: usize = 1184;
/// 密文（线上帧）长度。
pub const MLKEM768_CT_LEN: usize = 1088;
/// 共享秘密长度。
pub const MLKEM768_SS_LEN: usize = 32;

type Ek768 = EncapsulationKey<MlKem768>;
type EkBytes768 = Key<Ek768>;

/// 解封装密钥句柄（内存持有，Drop 即失效；zeroize feature 启用后材料擦除）。
pub struct MlKemSk(DecapsulationKey<MlKem768>);

/// 由 32B 身份种子确定性展开 ML-KEM-768 解封装密钥。
/// 同一种子必得同一密钥对（冒烟与 pq 测试断言）；失败仅因参数集常量不符（不可达防御）。
pub fn mlkem768_sk_from_seed(seed: &[u8; 32]) -> Option<MlKemSk> {
    let d = hash_sha256_bytes(format!("vsync-mlkem-d:{}", crate::hex_encode(seed)).as_bytes());
    let z = hash_sha256_bytes(format!("vsync-mlkem-z:{}", crate::hex_encode(seed)).as_bytes());
    let mut s64 = [0u8; 64];
    s64[..32].copy_from_slice(&d);
    s64[32..].copy_from_slice(&z);
    Some(MlKemSk(DecapsulationKey::<MlKem768>::from_seed(
        Seed::from(s64),
    )))
}

impl MlKemSk {
    /// 封装公钥字节（响应端在协商帧中发给对端）。
    /// 长度不变式：`Key<Ek768>` 即 `Array<u8, U1184>`，与本 crate 常量恒等。
    pub fn ek_bytes(&self) -> [u8; MLKEM768_EK_LEN] {
        let ek = self.0.encapsulation_key();
        let arr: EkBytes768 = ek.to_bytes();
        let mut out = [0u8; MLKEM768_EK_LEN];
        out.copy_from_slice(arr.as_slice());
        out
    }

    /// 解封装对端密文 → 共享秘密。长度不符返回 None（fail-open 判据）。
    pub fn decapsulate(&self, ct: &[u8]) -> Option<[u8; MLKEM768_SS_LEN]> {
        let arr = <[u8; MLKEM768_CT_LEN]>::try_from(ct).ok()?;
        let ss = self
            .0
            .decapsulate(&ml_kem::Ciphertext::<MlKem768>::from(arr));
        let mut out = [0u8; MLKEM768_SS_LEN];
        out.copy_from_slice(ss.as_slice());
        Some(out)
    }
}

/// 用对端封装公钥封装 → (密文, 共享秘密)。公钥长度或格式不符返回 None。
/// 封装随机性取自本 crate 的 CSPRNG（OsRng 桥），与全库随机源同源。
pub fn mlkem768_encapsulate(ek: &[u8]) -> Option<([u8; MLKEM768_CT_LEN], [u8; MLKEM768_SS_LEN])> {
    if ek.len() != MLKEM768_EK_LEN {
        return None;
    }
    let arr = EkBytes768::from(<[u8; MLKEM768_EK_LEN]>::try_from(ek).ok()?);
    let parsed = <Ek768 as TryKeyInit>::new(&arr).ok()?;
    let mut rng = OsBridge;
    let (ct, ss) = parsed.encapsulate_with_rng(&mut rng);
    let mut ct_out = [0u8; MLKEM768_CT_LEN];
    ct_out.copy_from_slice(ct.as_slice());
    let mut ss_out = [0u8; MLKEM768_SS_LEN];
    ss_out.copy_from_slice(ss.as_slice());
    Some((ct_out, ss_out))
}

/// vault-crypto CSPRNG → rand_core 0.10 的桥（封装内部随机数）。
struct OsBridge;

impl TryRng for OsBridge {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Infallible> {
        let b = random_bytes(4);
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn try_next_u64(&mut self) -> Result<u64, Infallible> {
        let b = random_bytes(8);
        let mut a = [0u8; 8];
        a.copy_from_slice(&b);
        Ok(u64::from_le_bytes(a))
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Infallible> {
        let b = random_bytes(dst.len());
        dst.copy_from_slice(&b);
        Ok(())
    }
}

impl TryCryptoRng for OsBridge {}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_keygen_from_seed() {
        let sk1 = mlkem768_sk_from_seed(&[7u8; 32]).unwrap();
        let sk2 = mlkem768_sk_from_seed(&[7u8; 32]).unwrap();
        assert_eq!(sk1.ek_bytes(), sk2.ek_bytes(), "同种子必得同一密钥对");
        let sk3 = mlkem768_sk_from_seed(&[8u8; 32]).unwrap();
        assert_ne!(sk1.ek_bytes(), sk3.ek_bytes());
    }

    #[test]
    fn encdec_roundtrip_and_lengths() {
        let sk = mlkem768_sk_from_seed(&[1u8; 32]).unwrap();
        let ek = sk.ek_bytes();
        assert_eq!(ek.len(), MLKEM768_EK_LEN);
        let (ct, ss1) = mlkem768_encapsulate(&ek).unwrap();
        assert_eq!(ct.len(), MLKEM768_CT_LEN);
        let ss2 = sk.decapsulate(&ct).unwrap();
        assert_eq!(ss1, ss2);
        assert_eq!(ss1.len(), MLKEM768_SS_LEN);
        // 隐式拒绝：被篡改密文解出一个伪秘密而不是报错（FIPS 203 语义），但两端不匹配
        let mut bad = ct;
        bad[0] ^= 0x01;
        let ss3 = sk.decapsulate(&bad).unwrap();
        assert_ne!(ss1, ss3, "篡改密文不得得到同一秘密");
    }

    #[test]
    fn bad_lengths_are_rejected() {
        let sk = mlkem768_sk_from_seed(&[2u8; 32]).unwrap();
        assert!(mlkem768_encapsulate(&[0u8; 16]).is_none());
        assert!(sk.decapsulate(&[0u8; 16]).is_none());
    }
}
