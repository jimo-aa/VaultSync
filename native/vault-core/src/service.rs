//! 密钥封装体系业务逻辑（P1-2/P1-3/P1-4）。
//!
//! 职责：MK 生成与 wrapped 副本管理、双路径解封（密码 / 生物识别）、
//! 零重加密改密码、伪装空间（独立保险箱文件，F-06）。
//! 本层之上的 FFI 层只见不透明会话句柄；MK 不跨界（docs/01 红线）。
use std::path::Path;

use vault_crypto::aead::{aead_decrypt, aead_encrypt, AES_GCM_NONCE_LEN};
use vault_crypto::kdf::{argon2id_derive, hkdf_sha256_derive, Argon2Params};
use vault_crypto::random::{random_bytes, random_key, random_salt};
use vault_crypto::KEY_LEN;
use zeroize::Zeroizing;

use crate::keystore::Keystore;
use crate::platform_store::{kek_bio, SecureStore};
use crate::session::{guard_check, guard_fail, guard_reset, GuardVerdict, Session};

#[derive(Debug, PartialEq, Eq)]
pub enum CoreError {
    /// 密码错误；u64 = 新增冷却毫秒（0 = 未达阈值）
    WrongPassword(u64),
    /// 防护冷却中；u64 = 剩余毫秒
    Cooldown(u64),
    BioUnavailable,
    BioNotBound,
    Io(&'static str),
    Format(&'static str),
    Internal(&'static str),
}

const VERIFIER_INFO: &[u8] = b"verifier";

fn verifier_of(mk: &[u8; KEY_LEN]) -> [u8; KEY_LEN] {
    *hkdf_sha256_derive(mk, VERIFIER_INFO)
}

fn wrap_mk(mk: &[u8; KEY_LEN], kek: &[u8; KEY_LEN]) -> Result<Vec<u8>, CoreError> {
    let nonce: [u8; AES_GCM_NONCE_LEN] = random_bytes(AES_GCM_NONCE_LEN)
        .as_slice()
        .try_into()
        .map_err(|_| CoreError::Internal("nonce len"))?;
    aead_encrypt(kek, &nonce, mk).map_err(|_| CoreError::Internal("wrap failed"))
}

fn unwrap_mk(kek: &[u8; KEY_LEN], blob: &[u8]) -> Option<[u8; KEY_LEN]> {
    let pt = aead_decrypt(kek, blob)?;
    pt.try_into().ok()
}

/// 创建保险箱（F-01）：生成 MK、写 wrapped 主副本；可同时绑定生物识别（F-03）。
pub fn create_vault(
    path: &Path,
    password: &str,
    bind_bio: bool,
    store: Option<&dyn SecureStore>,
) -> Result<(), CoreError> {
    let mk = random_key();
    let salt = random_salt();
    let argon = Argon2Params::DESKTOP;
    let kek = argon2id_derive(password, &salt, &argon).map_err(CoreError::Internal)?;
    let mk_wrap_pwd = wrap_mk(&mk, &kek)?;

    let mut ks = Keystore {
        path: path.to_path_buf(),
        argon,
        salt,
        verifier: verifier_of(&mk),
        mk_wrap_pwd,
        mk_wrap_bio: None,
    };

    if bind_bio {
        let store = store.ok_or(CoreError::BioUnavailable)?;
        bind_bio_locked(&mut ks, &mk, store)?;
    }
    ks.save().map_err(CoreError::Io)
}

/// 为 keystore 生成 KEK_bio、追加 wrapped 副本并落盘（不重复 save 供调用方合并写）。
fn bind_bio_locked(
    ks: &mut Keystore,
    mk: &[u8; KEY_LEN],
    store: &dyn SecureStore,
) -> Result<(), CoreError> {
    let kek_bio_key = random_key();
    kek_bio::store(store, &ks.path, &kek_bio_key).map_err(|_| CoreError::BioUnavailable)?;
    ks.mk_wrap_bio = Some(wrap_mk(mk, &kek_bio_key)?);
    Ok(())
}

/// 绑定 / 重新绑定生物识别（F-03）。会话必须为主空间。
pub fn bind_biometric(session: &Session, store: &dyn SecureStore) -> Result<(), CoreError> {
    if session.disguise {
        return Err(CoreError::Internal("disguise session cannot bind bio"));
    }
    let mut ks = Keystore::load(&session.vault_path).map_err(CoreError::Format)?;
    bind_bio_locked(&mut ks, &session.mk, store)?;
    ks.save().map_err(CoreError::Io)
}

/// 解除生物识别绑定：删除安全存储中的 KEK_bio 与 wrapped 副本。
pub fn unbind_biometric(session: &Session, store: &dyn SecureStore) -> Result<(), CoreError> {
    if session.disguise {
        return Err(CoreError::Internal("disguise session cannot unbind bio"));
    }
    let mut ks = Keystore::load(&session.vault_path).map_err(CoreError::Format)?;
    kek_bio::remove(store, &ks.path).map_err(|_| CoreError::BioUnavailable)?;
    ks.mk_wrap_bio = None;
    ks.save().map_err(CoreError::Io)
}

/// 主密码解锁（F-02）。错误密码计入暴力破解防护域（按父目录共享，见 session.rs）。
pub fn unlock(path: &Path, password: &str, disguise: bool) -> Result<Session, CoreError> {
    if let GuardVerdict::Cooldown(ms) = guard_check(path) {
        return Err(CoreError::Cooldown(ms));
    }
    let ks = Keystore::load(path).map_err(|e| match e {
        "cannot read vault file" => CoreError::Io(e),
        other => CoreError::Format(other),
    })?;
    let kek = argon2id_derive(password, &ks.salt, &ks.argon).map_err(CoreError::Internal)?;
    match unwrap_mk(&kek, &ks.mk_wrap_pwd) {
        Some(mk) if verifier_of(&mk) == ks.verifier => {
            guard_reset(path);
            Ok(Session {
                mk: Zeroizing::new(mk),
                vault_path: path.to_path_buf(),
                disguise,
            })
        }
        Some(_) => Err(CoreError::Internal(
            "MK verifier mismatch: wrapped copy tampered",
        )),
        None => {
            let wait = guard_fail(path);
            Err(CoreError::WrongPassword(wait))
        }
    }
}

/// 生物识别解锁（F-04）。安全存储不可用 → BioUnavailable（降级，主密码路径不受影响）。
pub fn unlock_biometric(path: &Path, store: &dyn SecureStore) -> Result<Session, CoreError> {
    if let GuardVerdict::Cooldown(ms) = guard_check(path) {
        return Err(CoreError::Cooldown(ms));
    }
    let ks = Keystore::load(path).map_err(CoreError::Format)?;
    let bio_blob = ks.mk_wrap_bio.as_deref().ok_or(CoreError::BioNotBound)?;
    let kek = kek_bio::load(store, path)
        .map_err(|_| CoreError::BioUnavailable)?
        .ok_or(CoreError::BioNotBound)?;
    match unwrap_mk(&kek, bio_blob) {
        Some(mk) if verifier_of(&mk) == ks.verifier => {
            guard_reset(path);
            Ok(Session {
                mk: Zeroizing::new(mk),
                vault_path: path.to_path_buf(),
                disguise: false,
            })
        }
        Some(_) => Err(CoreError::Internal(
            "MK verifier mismatch: wrapped copy tampered",
        )),
        None => {
            // 安全区重置 / 副本失效：标记失效但不影响主密码路径（docs/05-01 §六）
            let wait = guard_fail(path);
            Err(CoreError::WrongPassword(wait))
        }
    }
}

/// 修改主密码（F-07）：仅重新生成 MK_wrap_pwd，MK 不变 → 全部密文零重加密。
pub fn change_password(
    session: &Session,
    old_password: &str,
    new_password: &str,
) -> Result<(), CoreError> {
    if session.disguise {
        return Err(CoreError::Internal(
            "disguise session cannot change password",
        ));
    }
    let mut ks = Keystore::load(&session.vault_path).map_err(CoreError::Format)?;
    // 验证旧密码（不消耗防护计数：需已有会话）
    let kek_old =
        argon2id_derive(old_password, &ks.salt, &ks.argon).map_err(CoreError::Internal)?;
    match unwrap_mk(&kek_old, &ks.mk_wrap_pwd) {
        Some(mk) if mk == *session.mk => {}
        _ => return Err(CoreError::WrongPassword(0)),
    }
    // 新 salt + 新 KEK 重包装；MK 与全部文件密钥不变
    ks.salt = random_salt();
    let kek_new =
        argon2id_derive(new_password, &ks.salt, &ks.argon).map_err(CoreError::Internal)?;
    ks.mk_wrap_pwd = wrap_mk(&session.mk, &kek_new)?;
    // verifier 仅依赖 MK，不变；参数表保持（跨设备一致性）
    ks.save().map_err(CoreError::Io)
}

/// 查询保险箱文件是否存在（首次运行判定）。
pub fn vault_exists(path: &Path) -> bool {
    Keystore::exists(path)
}

/// 当前冷却剩余毫秒；无冷却返回 None。
pub fn cooldown_remaining_ms(path: &Path) -> Option<u64> {
    match guard_check(path) {
        GuardVerdict::Cooldown(ms) => Some(ms),
        GuardVerdict::Allowed => None,
    }
}

/// 保险箱是否已绑定生物识别路径。
pub fn bio_bound(path: &Path) -> Result<bool, CoreError> {
    let ks = Keystore::load(path).map_err(CoreError::Format)?;
    Ok(ks.bio_bound())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::platform_store::mock::{MockStore, UnavailableStore};

    fn tmp_vault(tag: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join(format!("{tag}.vsvb"));
        (dir, p)
    }

    // Argon2 DESKTOP 参数下每次派生 ~1s，业务流测试统一用低成本参数。
    // 注：create_vault 固定 DESKTOP；此处的流测试直接构造 Keystore 绕过。
    fn create_fast(path: &Path, pwd: &str) {
        let mk = random_key();
        let salt = random_salt();
        let argon = Argon2Params::for_test();
        let kek = argon2id_derive(pwd, &salt, &argon).expect("derive");
        let ks = Keystore {
            path: path.to_path_buf(),
            argon,
            salt,
            verifier: verifier_of(&mk),
            mk_wrap_pwd: wrap_mk(&mk, &kek).expect("wrap"),
            mk_wrap_bio: None,
        };
        ks.save().expect("save");
    }

    #[test]
    fn create_unlock_roundtrip() {
        let (_d, p) = tmp_vault("roundtrip");
        create_vault(&p, "s3cret-pw", false, None).expect("create");
        assert!(vault_exists(&p));
        let s = unlock(&p, "s3cret-pw", false).expect("unlock");
        assert!(!s.disguise);
    }

    #[test]
    fn wrong_password_counts_and_unlock_resets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join("primary.vsvb");
        let p2 = dir.path().join("disguise.vsvb");
        create_fast(&p, "right");
        assert_eq!(unlock(&p, "nope1", false), Err(CoreError::WrongPassword(0)));
        assert_eq!(unlock(&p, "nope2", false), Err(CoreError::WrongPassword(0)));
        assert_eq!(
            unlock(&p, "nope3", false),
            Err(CoreError::WrongPassword(30_000))
        );
        // 第 4 次直接进冷却
        assert!(matches!(
            unlock(&p, "nope4", false),
            Err(CoreError::Cooldown(_))
        ));
        // 伪空间文件同域共享冷却（父目录相同）
        assert!(matches!(
            unlock(&p2, "x", false),
            Err(CoreError::Cooldown(_))
        ));
    }

    #[test]
    fn change_password_zero_reencrypt() {
        let (_d, p) = tmp_vault("chg");
        create_fast(&p, "old-pw");
        let s = unlock(&p, "old-pw", false).expect("unlock");
        let mk_before = *s.mk;

        change_password(&s, "old-pw", "new-pw").expect("change");

        // MK 不变（零重加密断言）
        let s2 = unlock(&p, "new-pw", false).expect("unlock new");
        assert_eq!(*s2.mk, mk_before);
        // 旧密码立即失效
        assert!(matches!(
            unlock(&p, "old-pw", false),
            Err(CoreError::WrongPassword(_))
        ));
    }

    #[test]
    fn change_password_requires_old() {
        let (_d, p) = tmp_vault("chg-old");
        create_fast(&p, "old-pw");
        let s = unlock(&p, "old-pw", false).expect("unlock");
        assert_eq!(
            change_password(&s, "bad-old", "new"),
            Err(CoreError::WrongPassword(0))
        );
    }

    #[test]
    fn bio_bind_unlock_and_unbind() {
        let (_d, p) = tmp_vault("bio");
        create_fast(&p, "pw");
        let store = MockStore::default();
        let s = unlock(&p, "pw", false).expect("unlock");

        // 未绑定时解锁 → BioNotBound
        assert_eq!(unlock_biometric(&p, &store), Err(CoreError::BioNotBound));

        bind_biometric(&s, &store).expect("bind");
        let bio = unlock_biometric(&p, &store).expect("bio unlock");
        assert_eq!(*bio.mk, *s.mk);

        // 解绑后回到 BioNotBound，主密码不受影响
        unbind_biometric(&bio, &store).expect("unbind");
        assert_eq!(unlock_biometric(&p, &store), Err(CoreError::BioNotBound));
        assert!(unlock(&p, "pw", false).is_ok());
    }

    #[test]
    fn bio_store_unavailable_degrades_gracefully() {
        let (_d, p) = tmp_vault("bio-degrade");
        create_fast(&p, "pw");
        let s = unlock(&p, "pw", false).expect("unlock");
        let dead = UnavailableStore;
        // 绑定失败 = 平台不支持，不产生 wrapped 副本
        assert!(matches!(
            bind_biometric(&s, &dead),
            Err(CoreError::BioUnavailable)
        ));
        // 主密码路径完全不受影响
        assert!(unlock(&p, "pw", false).is_ok());
    }

    #[test]
    fn disguised_session_cannot_mutate_vault() {
        let (_d, p) = tmp_vault("dummy");
        create_fast(&p, "pw");
        let s = unlock(&p, "pw", true).expect("unlock");
        assert!(s.disguise);
        assert_eq!(
            change_password(&s, "pw", "x").unwrap_err(),
            CoreError::Internal("disguise session cannot change password")
        );
    }

    #[test]
    fn tampered_wrap_detected_by_verifier() {
        let (_d, p) = tmp_vault("tamper");
        create_fast(&p, "pw");
        // 篡改密文位 → GCM 解封失败（WrongPassword 路径，非 verifier）
        let mut raw = std::fs::read(&p).expect("read");
        let off = raw.len() - 60; // wrap blob 位于头部尾部
        raw[off] ^= 0xff;
        std::fs::write(&p, raw).expect("write");
        assert!(matches!(
            unlock(&p, "pw", false),
            Err(CoreError::WrongPassword(_))
        ));
    }
}
