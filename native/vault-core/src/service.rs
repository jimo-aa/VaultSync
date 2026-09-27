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
    /// 需要恢复（P7-3，码 11）：轮换恢复文件缺失 / 校验不过
    NeedsRecovery(&'static str),
    /// 维护态（P7-3，码 10）：业务写冻结
    Maintenance,
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
            // P8-10：隐写开关从 settings.enc 自读（不再依赖 UI 每次解锁后同步）
            let data_dir = {
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("vault");
                path.parent()
                    .unwrap_or(Path::new("."))
                    .join(format!("{stem}.data"))
            };
            let stego_from_settings = crate::settings::Settings::load(&data_dir, &mk).stego_enabled;
            let s = Session {
                mk: Zeroizing::new(mk),
                keys: Zeroizing::new(keys),
                vault_path: path.to_path_buf(),
                disguise,
                vault: std::sync::Arc::new(std::sync::Mutex::new(None)),
                p2p: std::sync::Mutex::new(None),
                audit: std::sync::Mutex::new(None),
                stego_enabled: std::sync::atomic::AtomicBool::new(stego_from_settings),
                readonly: std::sync::atomic::AtomicBool::new(false),
                maintenance: std::sync::atomic::AtomicBool::new(false),
                sync_ctx: std::sync::Mutex::new(vault_p2p::policy::SyncContext::default()),
                lease: std::sync::Mutex::new(None),
            };
            // P7-3：头部停在轮换态 → 会话进维护态（FFI 层随即自动续做）
            if ks.rotation_state != crate::keystore::ROTATION_IDLE {
                s.enter_maintenance();
            }
            Ok(s)
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
                maintenance: std::sync::atomic::AtomicBool::new(false),
                sync_ctx: std::sync::Mutex::new(vault_p2p::policy::SyncContext::default()),
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
    rotate_mk_begin(session, password, store).map(|rewritten| (true, rewritten))
}

/// P7-3 轮换会话 · begin：确认密码 → VSRR → state=1 → 数据面重加密 →
/// 重包装从属密钥 + 头部提交（state=2）→ 删 VSRR。任一步崩溃，重启后
/// `rotate_mk_resume` 从断点续做（state 是唯一真值）。
///
/// 维护态语义：state != 0 期间业务写冻结（码 10）；擦除/销毁不受冻结。
/// 无已配对设备时传播为空集 → state 直接归 0；有对端时停在 2 等 P7-4 传播。
pub fn rotate_mk_begin(
    session: &Session,
    password: &str,
    store: Option<&dyn SecureStore>,
) -> Result<usize, CoreError> {
    if session.disguise {
        return Err(CoreError::Internal("disguise session cannot rotate MK"));
    }
    if session.is_readonly() {
        return Err(CoreError::Internal("lease readonly")); // → 9
    }
    let data_dir = session.data_dir();
    let mut ks = Keystore::load(&session.vault_path).map_err(CoreError::Format)?;
    if ks.rotation_state != crate::keystore::ROTATION_IDLE {
        return Err(CoreError::Maintenance); // 已在轮换 → 10
    }
    // 强确认：必须持有当前 MK 的会话 + 正确的主密码（重新包装 MK' 用）
    let kek = argon2id_derive(password, &ks.salt, &ks.argon).map_err(CoreError::Internal)?;
    match unwrap_mk(&kek, &ks.mk_wrap_pwd) {
        Some(mk) if mk == *session.mk => {}
        _ => return Err(CoreError::WrongPassword(0)),
    }
    session.audit("security", "rotation.begin mk");

    // ③ VSRR（写入失败 → 码 3，不进入轮换，头部仍 idle）
    let rotation_id: [u8; 16] = vault_crypto::random_bytes(16)
        .as_slice()
        .try_into()
        .map_err(|_| CoreError::Internal("rand"))?;
    let mk_new = random_key();
    crate::rotation::write_vsrr(&data_dir, &rotation_id, &mk_new, &session.mk)
        .map_err(CoreError::Io)?;

    // ④ 头部置 state=1 + rotation_id（先于任何数据面变更；tmp+rename 提交点）
    ks.rotation_state = crate::keystore::ROTATION_IN_PROGRESS;
    ks.rotation_id = rotation_id;
    ks.save().map_err(CoreError::Io)?;
    session.enter_maintenance();
    session.audit(
        "security",
        &format!(
            "rotation.state=1 id={}",
            vault_crypto::hex_encode(&rotation_id)
        ),
    );

    // ⑤ 数据面重加密（可中断：崩溃后 resume 以 VSRR 恢复 MK' 续做）
    let rewritten = match session.with_vault(|v| v.rekey_all(&mk_new).map_err(CoreError::Internal))
    {
        Ok(n) => n,
        Err(e) => {
            // 状态保持 state=1 + VSRR：resume 续做（rekey_all 以旧索引键批次
            // 提交，中断即回滚到一致旧状态，见 vault.rs）
            eprintln!("vsync rotation data-face interrupted: {:?}; resumable", e);
            return Err(e);
        }
    };

    // ⑥⑦ 从属密钥仅重包装 + 控制面提交（state=2）
    finish_rotation(session, &mut ks, &mk_new, rotation_id, password, store)?;

    // 传播：无已配对设备 → 空集即完成，state 归 0；有对端 → kind=3 入队（P7-4）
    if rotation_propagation_pending(session) {
        let propagate = data_dir.join("rotation.propagate");
        if let Ok(blob) = std::fs::read(&propagate) {
            let hex = vault_crypto::hex_encode(&blob);
            if let Ok(engine) = crate::p2p_service::p2p_engine(session) {
                match engine.push_rotation(None, &vault_crypto::hex_encode(&rotation_id), &hex) {
                    Ok(v) => session.audit(
                        "security",
                        &format!("rotation.propagate queued={}", v["queued"]),
                    ),
                    Err(e) => {
                        session.audit("security", &format!("rotation.propagate queue failed: {e}"))
                    }
                }
            }
        }
        session.audit("security", "rotation.state=2 pending_ack");
    } else {
        finish_rotation_idle(session, &data_dir, rotation_id);
    }
    Ok(rewritten)
}

/// ⑥⑦ 共用：从属密钥重包装 + 新 salt/verifier + MK' 重包装 + 头部 state=2 提交 + 删 VSRR。
fn finish_rotation(
    session: &Session,
    ks: &mut Keystore,
    mk_new: &[u8; KEY_LEN],
    rotation_id: [u8; 16],
    password: &str,
    store: Option<&dyn SecureStore>,
) -> Result<(), CoreError> {
    let (rewraps, _) = Keystore::wrap_derived_keys(mk_new, Some(session.keys.to_array()));
    ks.derived_keys = Some(rewraps);
    session.audit("security", "dk.rewrap keyIds=[1,2,3,4,5,6,7,8]");

    // 主密码不随轮换改变（F-07 零影响）：新 salt 派生 KEK' 重包装 MK'。
    // 偏差（记 LOG）：resume 需要密码参数——规范签名无参，但 KEK 只能由
    // 密码派生，无参 resume 在密码学上无法完成 mk_wrap_pwd 的重包装。
    ks.verifier = verifier_of(mk_new);
    ks.salt = random_salt();
    let kek_new = argon2id_derive(password, &ks.salt, &ks.argon).map_err(CoreError::Internal)?;
    ks.mk_wrap_pwd = wrap_mk(mk_new, &kek_new)?;

    if ks.mk_wrap_bio.is_some() {
        match store {
            Some(st) => match kek_bio::load(st, &session.vault_path) {
                Ok(Some(kek_bio_key)) => {
                    ks.mk_wrap_bio = Some(wrap_mk(mk_new, &kek_bio_key)?);
                }
                _ => ks.mk_wrap_bio = None,
            },
            None => ks.mk_wrap_bio = None,
        }
    }
    ks.rotation_state = crate::keystore::ROTATION_COMPLETED_PENDING_ACK;
    ks.rotation_id = rotation_id;
    ks.save().map_err(CoreError::Io)?;
    // P7-4：VSRR 转为传播载体（新 MK' 的包装原样投递给对端）；对端 ACK 齐
    // （或无对端）后由 finish_rotation_idle 删除
    let vsrr = crate::rotation::vsrr_path(&session.data_dir());
    let propagate = session.data_dir().join("rotation.propagate");
    if vsrr.exists() {
        std::fs::rename(&vsrr, &propagate)
            .map_err(|_| CoreError::Io("cannot keep propagate blob"))?;
    }
    Ok(())
}

/// state=2 → 0：传播完成（空集或全部 acked 后由 P7-4 调用）。
fn finish_rotation_idle(session: &Session, data_dir: &std::path::Path, rotation_id: [u8; 16]) {
    if let Ok(mut ks) = Keystore::load(&session.vault_path) {
        if ks.rotation_id == rotation_id {
            ks.rotation_state = crate::keystore::ROTATION_IDLE;
            ks.rotation_id = [0u8; 16];
            if ks.save().is_ok() {
                session.exit_maintenance();
                session.audit("security", "rotation.state=0 propagated");
                let _ = std::fs::remove_file(crate::rotation::vsrr_path(data_dir));
                let _ = std::fs::remove_file(data_dir.join("rotation.propagate"));
            }
        }
    }
}

/// 是否存在已配对设备（P7-3 范围内的传播判定；逐设备 ack 归 P7-4）。
fn rotation_propagation_pending(session: &Session) -> bool {
    session
        .p2p
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|e| e.has_paired_peers()))
        .unwrap_or(false)
}

/// P7-3 轮换会话 · resume：状态机恢复（不需要密码：MK' 由 VSRR + 会话内旧 MK 恢复）。
/// 返回重写的文件数；state=0 幂等成功；state=1 缺 VSRR/校验不过 → NeedsRecovery（11）。
pub fn rotate_mk_resume(session: &Session, password: &str) -> Result<usize, CoreError> {
    let data_dir = session.data_dir();
    let mut ks = Keystore::load(&session.vault_path).map_err(CoreError::Format)?;
    match ks.rotation_state {
        crate::keystore::ROTATION_IDLE => {
            // state=0：VSRR 是孤儿（state 是唯一真值）
            if crate::rotation::remove_orphan_vsrr(&data_dir) {
                session.audit("diagnostic", "rotation.orphanRecoverRemoved");
            }
            session.exit_maintenance();
            Ok(0)
        }
        crate::keystore::ROTATION_IN_PROGRESS => {
            let v = crate::rotation::read_vsrr(&data_dir).map_err(|e| match e {
                crate::rotation::VsrrError::Missing => CoreError::NeedsRecovery("vsrr missing"),
                crate::rotation::VsrrError::Corrupt(m) => CoreError::NeedsRecovery(m),
            })?;
            if v.rotation_id != ks.rotation_id {
                return Err(CoreError::NeedsRecovery("vsrr rotation_id mismatch"));
            }
            let mk_new = v
                .unwrap_mk(&session.mk)
                .map_err(|_| CoreError::NeedsRecovery("vsrr mk unwrap failed"))?;
            session.audit("security", "rotation.resume");
            // 数据面续做（幂等：中断发生在批次提交前即整体回滚；已提交项重跑等价）
            let rewritten =
                session.with_vault(|v| v.rekey_all(&mk_new).map_err(CoreError::Internal))?;
            // 生物重包装走下一次解锁的降级路径（resume 无密码上下文）
            let rid = ks.rotation_id;
            finish_rotation(session, &mut ks, &mk_new, rid, password, None)?;
            if rotation_propagation_pending(session) {
                session.audit("security", "rotation.state=2 pending_ack");
            } else {
                finish_rotation_idle(session, &data_dir, ks.rotation_id);
            }
            Ok(rewritten)
        }
        crate::keystore::ROTATION_COMPLETED_PENDING_ACK => {
            // VSRR 残留：删除；等待 P7-4 传播（P7-3 内无对端即归 idle）
            let _ = std::fs::remove_file(crate::rotation::vsrr_path(&data_dir));
            if !rotation_propagation_pending(session) {
                let rid = ks.rotation_id;
                finish_rotation_idle(session, &data_dir, rid);
            }
            Ok(0)
        }
        _ => Err(CoreError::Internal("unknown rotation_state")),
    }
}

/// P7-3 轮换会话 · status：JSON 载荷（字段对齐 03 §4.10）。
pub fn rotate_mk_status(session: &Session) -> Result<serde_json::Value, CoreError> {
    let ks = Keystore::load(&session.vault_path).map_err(CoreError::Format)?;
    let state = match ks.rotation_state {
        crate::keystore::ROTATION_IDLE => "idle",
        crate::keystore::ROTATION_IN_PROGRESS => "in_progress",
        _ => "pending_ack",
    };
    Ok(serde_json::json!({
        "schema": 1,
        "rotationState": state,
        "rotationId": vault_crypto::hex_encode(&ks.rotation_id),
        "phase": match ks.rotation_state {
            crate::keystore::ROTATION_IDLE => "done",
            crate::keystore::ROTATION_IN_PROGRESS => "rekey",
            _ => "propagate",
        },
        "interrupted": ks.rotation_state == crate::keystore::ROTATION_IN_PROGRESS,
        "autoResumeExhausted": false,
        "maintenance": session.is_maintenance(),
        "propagation": [],
    }))
}

/// P7-4：消费对端轮换通知——用本端密码完成与 notice 等价的轮换
/// （notice 的 ct 由两端相同的旧 MK 包装；本端解锁后以 VSRR 同构造落盘，
/// 复用 resume 路径完成数据面重加密与头部提交）。
pub fn consume_rotation_notice(session: &Session, password: &str) -> Result<usize, CoreError> {
    let data_dir = session.data_dir();
    let inbound = data_dir.join("rotation.inbound");
    if !inbound.exists() {
        return Ok(0);
    }
    let raw = std::fs::read(&inbound).map_err(|_| CoreError::Io("cannot read rotation inbound"))?;
    let v: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|_| CoreError::Format("rotation inbound parse"))?;
    let rotation_id_hex = v["rotationId"]
        .as_str()
        .ok_or(CoreError::Format("inbound id"))?;
    let ct = v["ct"].as_str().ok_or(CoreError::Format("inbound ct"))?;
    let id_bytes =
        vault_crypto::hex_decode(rotation_id_hex).ok_or(CoreError::Format("inbound id hex"))?;
    if id_bytes.len() != 16 {
        return Err(CoreError::Format("inbound id len"));
    }
    let mut rotation_id = [0u8; 16];
    rotation_id.copy_from_slice(&id_bytes);
    let blob = vault_crypto::hex_decode(ct).ok_or(CoreError::Format("inbound ct hex"))?;
    let mut ks = Keystore::load(&session.vault_path).map_err(CoreError::Format)?;
    if ks.rotation_state != crate::keystore::ROTATION_IDLE {
        return Ok(0); // 本端自有轮换进行中：通知保留，稍后处理
    }
    // 用 notice 的 ct 解出 MK'（解封密钥 = HKDF(本端旧 MK,"rotation-recover")）；
    // 解封失败 = 两端旧 MK 不同 → 如实拒绝（不动任何状态）
    let wrap_key = vault_crypto::kdf::hkdf_sha256_derive(&*session.mk, b"rotation-recover");
    let mk_new_vec = vault_crypto::aead_decrypt(&wrap_key, &blob)
        .ok_or(CoreError::Format("rotation notice mk unwrap failed"))?;
    let mk_new: [u8; 32] = mk_new_vec
        .as_slice()
        .try_into()
        .map_err(|_| CoreError::Format("rotation mk len"))?;
    // 走标准轮换会话：写 VSRR → state=1 → resume 续做（数据面+头部提交）
    crate::rotation::write_vsrr(&data_dir, &rotation_id, &mk_new, &session.mk)
        .map_err(CoreError::Io)?;
    ks.rotation_state = crate::keystore::ROTATION_IN_PROGRESS;
    ks.rotation_id = rotation_id;
    ks.save().map_err(CoreError::Io)?;
    session.enter_maintenance();
    session.audit("security", "rotation.inbound accepted");
    let n = rotate_mk_resume(session, password)?;
    let _ = std::fs::remove_file(&inbound);
    Ok(n)
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

    /// P7-2 证据：rotation_chain::audit_chain_continues_after_mk_rotation
    /// 链键（从属密钥 6）与 MK 解耦：轮换前后审计链逐条衔接。
    #[test]
    fn audit_chain_continues_after_mk_rotation() {
        let (_d, p) = tmp_vault("chain");
        create_fast(&p, "pw-1234");
        let s = unlock(&p, "pw-1234", false).expect("unlock");
        s.audit("session", "e1");
        s.audit("vault", "e2");
        let head_before = s
            .with_audit(|log| (log.len(), log.head_hash()))
            .expect("audit");
        let _ = head_before;
        assert_eq!(head_before.0, 2);

        rotate_mk(&s, "pw-1234", None).expect("rotate");
        // 轮换后同会话继续追加：同一条链
        s.audit("vault", "e3-after-rotation");
        let v = s
            .with_audit(|log| {
                let verdict = log.verify();
                let entries = log.entries().to_vec();
                (verdict, entries)
            })
            .expect("audit open");
        assert!(v.0.ok, "chain must stay intact: {:?}", v.0.reason);
        // 轮换事件本身也写入同一条链（begin/state=1/dk.rewrap/state=0）→ 条目数 > 3
        assert!(v.1.len() > 3, "rotation events should be in the same chain");
        // 全链逐条衔接（含跨轮换点）
        for w in v.1.windows(2) {
            assert_eq!(
                w[1].prev_hash, w[0].hash,
                "chain continues across mk rotation"
            );
        }
        // 新会话（新 MK 解锁）读同一条链
        drop(s);
        let s2 = unlock(&p, "pw-1234", false).expect("unlock after rotate");
        let check = s2
            .with_audit(|log| (log.len(), log.verify().ok))
            .expect("audit open");
        assert!(check.0 > 3);
        assert!(check.1);
    }

    /// P7-3 证据：rotation_session::resume_after_kill
    /// 数据面中断（state=1 + VSRR）→ resume 续做 → 新 MK 解锁、文件可读。
    #[test]
    fn rotation_session_resume_after_kill() {
        let (_d, p) = tmp_vault("rsess");
        create_fast(&p, "pw-1234");
        let s = unlock(&p, "pw-1234", false).expect("unlock");
        // 放一个文件（数据面）
        let src = {
            let d = p.parent().unwrap().join("src.txt");
            std::fs::write(&d, b"rotation payload").expect("src");
            d
        };
        let file_id = s
            .with_vault(|v| v.import_file(&src, 0).map_err(CoreError::Internal))
            .expect("import");

        // 模拟 I3：begin 的 ④⑤ 之间被杀 —— 手工构造 state=1 + VSRR 现场
        let rotation_id = [0xABu8; 16];
        let mk_new = random_key();
        crate::rotation::write_vsrr(s.data_dir().as_path(), &rotation_id, &mk_new, &s.mk)
            .expect("vsrr");
        let mut ks = Keystore::load(&p).expect("ks");
        ks.rotation_state = crate::keystore::ROTATION_IN_PROGRESS;
        ks.rotation_id = rotation_id;
        ks.save().expect("save");
        s.enter_maintenance();

        // resume：不需要 begin 上下文，密码重包装 MK'
        let n = rotate_mk_resume(&s, "pw-1234").expect("resume");
        let _ = n;
        assert!(
            !s.is_maintenance(),
            "无对端（传播空集）→ 轮换完成后应回到 Active"
        );
        drop(s);

        // 新 MK 生效：同一密码解锁，文件仍可读，状态 idle
        let s2 = unlock(&p, "pw-1234", false).expect("unlock with same password");
        let st = rotate_mk_status(&s2).expect("status");
        assert_eq!(st["rotationState"], "idle");
        let out = p.parent().unwrap().join("after-rotate.bin");
        s2.with_vault(|v| v.export_file(file_id, &out).map_err(CoreError::Internal))
            .expect("export after resume");
        assert_eq!(std::fs::read(&out).expect("read"), b"rotation payload");

        // 错误密码 → 拒绝
        drop(s2);
        assert!(unlock(&p, "wrong-pw", false).is_err());
    }

    /// P7-3：VSRR 校验失败 → 11（NeedsRecovery），不猜测。
    #[test]
    fn rotation_resume_corrupt_vsrr_is_needs_recovery() {
        let (_d, p) = tmp_vault("rvsrr");
        create_fast(&p, "pw-1234");
        let s = unlock(&p, "pw-1234", false).expect("unlock");
        let rotation_id = [0x11u8; 16];
        crate::rotation::write_vsrr(s.data_dir().as_path(), &rotation_id, &random_key(), &s.mk)
            .expect("vsrr");
        // 篡改 digest 区
        let pp = crate::rotation::vsrr_path(s.data_dir().as_path());
        let mut raw = std::fs::read(&pp).expect("read");
        let last = raw.len() - 1;
        raw[last] ^= 0x01;
        std::fs::write(&pp, &raw).expect("write");
        let mut ks = Keystore::load(&p).expect("ks");
        ks.rotation_state = crate::keystore::ROTATION_IN_PROGRESS;
        ks.rotation_id = rotation_id;
        ks.save().expect("save");

        match rotate_mk_resume(&s, "pw-1234") {
            Err(CoreError::NeedsRecovery(_)) => {}
            other => panic!("expected NeedsRecovery, got {:?}", other.map(|_| ())),
        }
        // state=0：孤儿 VSRR 清理 + 幂等成功
        let mut ks2 = Keystore::load(&p).expect("ks");
        ks2.rotation_state = crate::keystore::ROTATION_IDLE;
        ks2.save().expect("save");
        assert!(crate::rotation::vsrr_path(s.data_dir().as_path()).exists());
        rotate_mk_resume(&s, "pw-1234").expect("idle resume");
        assert!(!crate::rotation::vsrr_path(s.data_dir().as_path()).exists());
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
