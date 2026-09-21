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
use zeroize::{Zeroize, Zeroizing};

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

    let (derived_wraps, derived_keys) = Keystore::wrap_derived_keys(&mk, None);
    let mut ks = Keystore {
        path: path.to_path_buf(),
        argon,
        salt,
        verifier: verifier_of(&mk),
        mk_wrap_pwd,
        mk_wrap_bio: None,
        rotation_state: crate::keystore::ROTATION_IDLE,
        rotation_id: [0u8; 16],
        derived_keys: Some(derived_wraps),
    };
    // 首启从属密钥明文随栈帧结束离作用域；这里显式零化（数组是 Copy，不能 drop）
    {
        let mut dk = derived_keys;
        dk.zeroize();
    }

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
    let mut ks = Keystore::load(path).map_err(|e| match e {
        "cannot read vault file" => CoreError::Io(e),
        other => CoreError::Format(other),
    })?;
    let kek = argon2id_derive(password, &ks.salt, &ks.argon).map_err(CoreError::Internal)?;
    match unwrap_mk(&kek, &ks.mk_wrap_pwd) {
        Some(mk) if verifier_of(&mk) == ks.verifier => {
            guard_reset(path);
            let keys = open_derived_keys(&mut ks, &mk).map_err(CoreError::Internal)?;
            Ok(Session {
                mk: Zeroizing::new(mk),
                keys: Zeroizing::new(keys),
                vault_path: path.to_path_buf(),
                disguise,
                vault: std::sync::Arc::new(std::sync::Mutex::new(None)),
                p2p: std::sync::Mutex::new(None),
                audit: std::sync::Mutex::new(None),
                stego_enabled: std::sync::atomic::AtomicBool::new(false),
                readonly: std::sync::atomic::AtomicBool::new(false),
                lease: std::sync::Mutex::new(None),
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

/// 解封从属密钥束（P7-1/P7-2）：v3 头部从包装区解封；v2 头部按冻结公式
/// 内存求得（`derivedKeysLegacy=true`）并**顺手升级**为 v3 包装区（尽力而为，
/// 失败不阻塞解锁——下次写头部时会再次尝试）。
fn open_derived_keys(
    ks: &mut Keystore,
    mk: &[u8; KEY_LEN],
) -> Result<crate::session::VaultKeys, &'static str> {
    let array = match ks.derived_keys.as_ref() {
        Some(_) => ks.unwrap_derived_keys(mk)?,
        None => {
            let legacy = Keystore::legacy_derived_keys(mk);
            // 升级：以冻结值进包装区（存量数据无需重写）
            let (wraps, _) = Keystore::wrap_derived_keys(mk, Some(legacy));
            ks.derived_keys = Some(wraps);
            if let Err(e) = ks.save() {
                eprintln!("vsync keystore v3 upgrade deferred: {e}");
            }
            legacy
        }
    };
    Ok(crate::session::VaultKeys::from_array(array))
}

/// 生物识别解锁（F-04）。安全存储不可用 → BioUnavailable（降级，主密码路径不受影响）。
pub fn unlock_biometric(path: &Path, store: &dyn SecureStore) -> Result<Session, CoreError> {
    if let GuardVerdict::Cooldown(ms) = guard_check(path) {
        return Err(CoreError::Cooldown(ms));
    }
    let mut ks = Keystore::load(path).map_err(CoreError::Format)?;
    let bio_blob = ks.mk_wrap_bio.as_deref().ok_or(CoreError::BioNotBound)?;
    let kek = kek_bio::load(store, path)
        .map_err(|_| CoreError::BioUnavailable)?
        .ok_or(CoreError::BioNotBound)?;
    match unwrap_mk(&kek, bio_blob) {
        Some(mk) if verifier_of(&mk) == ks.verifier => {
            guard_reset(path);
            let keys = open_derived_keys(&mut ks, &mk).map_err(CoreError::Internal)?;
            Ok(Session {
                mk: Zeroizing::new(mk),
                keys: Zeroizing::new(keys),
                vault_path: path.to_path_buf(),
                disguise: false,
                vault: std::sync::Arc::new(std::sync::Mutex::new(None)),
                p2p: std::sync::Mutex::new(None),
                audit: std::sync::Mutex::new(None),
                stego_enabled: std::sync::atomic::AtomicBool::new(false),
                readonly: std::sync::atomic::AtomicBool::new(false),
                lease: std::sync::Mutex::new(None),
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

/// MK 全库轮换（docs/07 §三 在单机范围内的可用部分）。
///
/// 流程：校验密码 → 生成 MK'（CSPRNG，永不派生自密码）→ 全库重加密
/// （索引键 / 各文件夹 FSK' / 每文件 FSKey'，由 `vault-vault::rekey_all` 保证
/// 容器暂存与索引原子提交）→ 新 salt 下用 KEK_pwd 与（若已绑定）原 KEK_bio
/// 重新包装 MK' → 原子覆盖保险箱头部。
///
/// 返回 `(生物识别副本是否一并换过, 重写的文件数)`。
///
/// **调用方责任**：本函数**不更新** `session.mk`（会话内的 MK 副本仍是旧的），
/// 成功后必须立即 `vault_core_lock` 并用主密码重新解锁；重新解锁前不要继续使用该会话。
///
/// 已知窗口：`rekey_all` 完成后到头部落盘之间存在极短窗口（一次原子 rename），
/// 期间断电会留下「数据已用 MK'、头部仍包旧 MK」的不可解状态。彻底消除需要
/// 在头部加「轮换进行中」标志位与恢复路径（记入 LOG 待办）。
///
/// 副作用（docs/07 未写但必须说明）：索引倒排的搜索令牌与阅后即焚分享令牌都由 MK 派生，
/// 轮换后前者随新 MK 重建、后者无法迁移故一并作废。
pub fn rotate_mk(
    session: &Session,
    password: &str,
    store: Option<&dyn SecureStore>,
) -> Result<(bool, usize), CoreError> {
    if session.disguise {
        return Err(CoreError::Internal("disguise session cannot rotate MK"));
    }
    let mut ks = Keystore::load(&session.vault_path).map_err(CoreError::Format)?;
    // 强确认：必须持有当前 MK 的会话 + 正确的主密码（用于重新包装 MK'）
    let kek = argon2id_derive(password, &ks.salt, &ks.argon).map_err(CoreError::Internal)?;
    match unwrap_mk(&kek, &ks.mk_wrap_pwd) {
        Some(mk) if mk == *session.mk => {}
        _ => return Err(CoreError::WrongPassword(0)),
    }

    // 先记审计：此刻旧 MK 仍有效，这条会随旧链一起被归档（持有旧 MK 者可事后核对）
    session.audit("security", "MK rotation started (whole-vault rekey)");

    let mk_new = random_key();
    // 数据面：索引 + 全部容器（含每文件新 FSKey 与各文件夹新 FSK 覆盖）
    let rewritten = session.with_vault(|v| v.rekey_all(&mk_new).map_err(CoreError::Internal))?;

    // 数据面已换钥：此后**任何**失败都必须把数据面滚回旧 MK。否则「数据用 MK'、头部仍包旧 MK」
    // 会让保险箱彻底打不开（冒烟测试第一版就命中了这个状态，故此处必须回滚）。
    let complete = (|| -> Result<(usize, bool), CoreError> {
        // P7-2：审计链键（从属密钥 6）与 MK 解耦，轮换不再触碰审计载体；
        // 旧载体存在性仅作诊断保留（legacyChains 语义归迁移工具 P7-7）。
        let data_dir = session.data_dir();
        let _audit_moved = session.with_audit(|log| log.len()).unwrap_or(0);

        // 控制面：新 salt 下重新包装 MK'
        // **verifier 必须同步更新**：它是 HKDF(MK,"verifier")，锁死在头部，
        // 不同步会让下一次解锁判定为「wrapped 副本被调包」而拒绝解封（冒烟抓到过）。
        ks.verifier = verifier_of(&mk_new);
        ks.salt = random_salt();
        let kek_new =
            argon2id_derive(password, &ks.salt, &ks.argon).map_err(CoreError::Internal)?;
        ks.mk_wrap_pwd = wrap_mk(&mk_new, &kek_new)?;

        let mut bio_rewrapped = false;
        if ks.mk_wrap_bio.is_some() {
            match store {
                Some(st) => match kek_bio::load(st, &session.vault_path) {
                    // 安全区里的 KEK_bio 可读 → 无需再次刷指纹即可包装 MK'
                    Ok(Some(kek_bio_key)) => {
                        ks.mk_wrap_bio = Some(wrap_mk(&mk_new, &kek_bio_key)?);
                        bio_rewrapped = true;
                    }
                    // 安全区不可用 / 无后端：主密码路径不受影响，生物路径失效
                    _ => ks.mk_wrap_bio = None,
                },
                None => ks.mk_wrap_bio = None,
            }
        }

        // P7-2：从属密钥**仅重包装**（8×64B，取值不变）——索引/搜索/分享/订单/
        // 隐写/链键/审计导出/发现盐全部不动；这是「轮换是廉价事务」的机制基础。
        let (rewraps, _) = Keystore::wrap_derived_keys(&mk_new, Some(session.keys.to_array()));
        ks.derived_keys = Some(rewraps);
        session.audit("security", "dk.rewrap keyIds=[1,2,3,4,5,6,7,8]");

        ks.save().map_err(CoreError::Io)?;
        let legacy_chain_present =
            usize::from(data_dir.join("audit.enc").exists() || data_dir.join("audit").is_dir());
        Ok((legacy_chain_present, bio_rewrapped))
    })();

    let (audit_moved, bio_rewrapped) = match complete {
        Ok(v) => v,
        Err(e) => {
            // 回滚数据面到旧 MK。审计文件此时尚未改名（改名在 ks.save() 之后），
            // 因此回滚不需要额外处理审计。
            if session
                .with_vault(|v| v.rekey_all(&session.mk).map_err(CoreError::Internal))
                .is_err()
            {
                return Err(CoreError::Internal(
                    "MK rotation failed and rollback failed: vault requires the new MK",
                ));
            }
            return Err(e);
        }
    };
    // P7-2：轮换完成事件随解耦后的链键写入**同一条**审计链（不再断链）。
    let _ = (audit_moved, bio_rewrapped, rewritten);
    Ok((bio_rewrapped, rewritten))
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

/// 加密擦除本机数据（P5-5 紧急销毁 / 远程销毁执行体，docs/05-06 §五）。
///
/// `secure=true` 时先对保险箱头部与数据目录内的每个文件做**单次覆写**再删除
/// （介质兜底语义；SSD 上单次覆写不等于物理擦除，文档如实标注）。
/// 销毁的是「本机全部可解封材料」：保险箱头部（wrapped MK 副本 + salt）
/// 与数据目录（密文容器 / 加密索引 / P2P 身份 / 审计日志）。
pub(crate) fn wipe_local(vault_path: &Path, secure: bool) -> Result<(), String> {
    let stem = vault_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("bad vault path")?;
    let data_dir = vault_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!("{stem}.data"));

    let mut targets: Vec<std::path::PathBuf> = Vec::new();
    if vault_path.exists() {
        targets.push(vault_path.to_path_buf());
    }
    collect_files(&data_dir, &mut targets, 0);

    if secure {
        for f in &targets {
            overwrite_once(f);
        }
    }

    let mut errs = Vec::new();
    for f in &targets {
        if f == vault_path {
            continue;
        }
        if std::fs::remove_file(f).is_err() && f.exists() {
            errs.push("file");
        }
    }
    if std::fs::remove_file(vault_path).is_err() && vault_path.exists() {
        errs.push("vault file");
    }
    if std::fs::remove_dir_all(&data_dir).is_err() && data_dir.exists() {
        errs.push("data dir");
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(format!("wipe failed: {}", errs.join(", ")))
    }
}

/// 递归收集数据目录内的常规文件（深度上限防环）。
fn collect_files(dir: &Path, out: &mut Vec<std::path::PathBuf>, depth: u32) {
    if depth > 8 || !dir.is_dir() {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out, depth + 1);
        } else {
            out.push(path);
        }
    }
}

/// 单次覆写（64 KiB 零块）；失败静默（尽力而为，随后仍会删除）。
fn overwrite_once(path: &Path) {
    use std::io::{Seek, Write};
    let mut f = match std::fs::OpenOptions::new().write(true).open(path) {
        Ok(f) => f,
        Err(_) => return,
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    if f.seek(std::io::SeekFrom::Start(0)).is_err() {
        return;
    }
    let zeros = vec![0u8; 64 * 1024];
    let mut left = len;
    while left > 0 {
        let n = zeros.len().min(left as usize);
        if f.write_all(&zeros[..n]).is_err() {
            return;
        }
        left -= n as u64;
    }
    f.sync_all().ok();
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
        let (derived_wraps, _) = Keystore::wrap_derived_keys(&mk, None);
        let ks = Keystore {
            path: path.to_path_buf(),
            argon,
            salt,
            verifier: verifier_of(&mk),
            mk_wrap_pwd: wrap_mk(&mk, &kek).expect("wrap"),
            mk_wrap_bio: None,
            rotation_state: crate::keystore::ROTATION_IDLE,
            rotation_id: [0u8; 16],
            derived_keys: Some(derived_wraps),
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
        assert!(matches!(
            unlock(&p, "nope1", false),
            Err(CoreError::WrongPassword(_))
        ));
        assert!(matches!(
            unlock(&p, "nope2", false),
            Err(CoreError::WrongPassword(_))
        ));
        assert!(matches!(
            unlock(&p, "nope3", false),
            Err(CoreError::WrongPassword(_))
        ));
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
        assert!(matches!(
            unlock_biometric(&p, &store),
            Err(CoreError::BioNotBound)
        ));

        bind_biometric(&s, &store).expect("bind");
        let bio = unlock_biometric(&p, &store).expect("bio unlock");
        assert_eq!(*bio.mk, *s.mk);

        // 解绑后回到 BioNotBound，主密码不受影响
        unbind_biometric(&bio, &store).expect("unbind");
        assert!(matches!(
            unlock_biometric(&p, &store),
            Err(CoreError::BioNotBound)
        ));
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
