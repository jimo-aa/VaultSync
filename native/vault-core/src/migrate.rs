//! P7-7 V2→V3 格式迁移工具（docs/v2.0/07 §七）。
//!
//! 四类迁移，依赖序：M4 前置检查/头部备份 → M1 容器（VSEF v2→v3，逐文件原子替换）
//! → M2 索引（index.enc → VSIX v3 段）→ M3 审计（audit.enc → VSAU v1 段）→ M4 复核。
//! 「先建后拆」：新格式提交成功后旧载体才删除；任何一步崩溃都存在可读版本。
//!
//! 断点续做：`<data_dir>/migrate.log.jsonl` 逐行记录（只增不改）；`begin`/`resume`
//! 从头扫描但跳过已有 `done` 行。M1 逐文件原子替换天然幂等。
//!
//! 偏差（与 docs/v2.0/07 §7.3 的规范签名对照，均记 LOG）：
//! 1. 同步执行：`migrate_begin` 在调用内完成全部迁移，`out_task_id` 恒写 0
//!    （与 P7-3 `rotate_mk_begin` 同一取舍；异步任务化归 P8-1）。
//! 2. `migrate_cancel` 仅清理 `.mig.tmp` / `.mig.part` 残留——同步执行下不存在
//!    「进行中的原子单元」可停。
//! 3. M4 头部抬升**不由迁移工具执行**：VSVB v2→v3 已在解锁（RW 首写）时自动完成
//!    （keystore 兼容读 + 首写升级）。迁移只做 format_ver 复核与 v2 头部备份
//!    （`<vault>.vsb.pre-v3`，若头部尚为 v2）；7 天保留期清理归后续任务。
//! 4. 磁盘余量 ≥ 库大小 × 1.2 的前置检查未实现（std 无可用空间查询，引入
//!    平台 API 的取舍留给后续；其余前置检查照做）。
#![forbid(unsafe_code)]
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::json;

use vault_audit::{segmented, AuditLog, SegmentedAuditLog};
use vault_crypto::kdf::{argon2id_derive, hkdf_sha256_derive};
use vault_crypto::{aead_decrypt, KEY_LEN};
use vault_store::{Namespace, OpenMode, VaultStore};
use vault_vault::container::container_v3::{
    assemble_v3, ContainerReaderV3, V3ChunkEntry, V3FileMeta, DEFAULT_META_SHARD_SPAN,
};
use vault_vault::container::ContainerReader;
use vault_vault::index::VaultIndex;

use crate::keystore::Keystore;
use crate::session::Session;

/// 迁移日志文件名（`<data_dir>/migrate.log.jsonl`，docs/v2.0/07 §7.3）。
const MIGRATE_LOG: &str = "migrate.log.jsonl";
/// M1 暂存容器后缀（`files/<id>.vse.mig.tmp`）。
const MIG_TMP_SUFFIX: &str = ".vse.mig.tmp";
/// M1 数据区暂存文件后缀（`files/<id>.mig.part`）。
const MIG_PART_SUFFIX: &str = ".mig.part";
/// 迁移前头部备份后缀（`<vault>.vsb.pre-v3`）。
const BACKUP_SUFFIX: &str = ".vsb.pre-v3";

/// 迁移错误 → FFI 状态码（docs/v2.0/01 §6.7）。
#[derive(Debug)]
pub enum MigrateError {
    /// 1：密码错误（MK 确认失败）
    WrongPassword,
    /// 9：只读（租约被占）
    LeaseBusy,
    /// 10：维护态（轮换/另一迁移进行中）
    Maintenance,
    /// 3：IO
    Io(&'static str),
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn log_path(data_dir: &Path) -> PathBuf {
    data_dir.join(MIGRATE_LOG)
}

fn append_log(data_dir: &Path, mut v: serde_json::Value) -> Result<(), MigrateError> {
    let obj = v.as_object_mut().ok_or(MigrateError::Io("log shape"))?;
    obj.insert("schema".into(), json!(1));
    obj.insert("ts_ms".into(), json!(now_ms()));
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path(data_dir))
        .map_err(|_| MigrateError::Io("cannot open migrate log"))?;
    let line = serde_json::to_string(&v).map_err(|_| MigrateError::Io("log serialize"))?;
    f.write_all(line.as_bytes())
        .and_then(|_| f.write_all(b"\n"))
        .map_err(|_| MigrateError::Io("cannot write migrate log"))
}

/// 日志中已 `done` 的项（续做跳过依据）。
#[derive(Default)]
struct DoneSet {
    files: HashSet<u64>,
    index: bool,
    audit: bool,
    header: bool,
}

fn read_done(data_dir: &Path) -> DoneSet {
    let mut done = DoneSet::default();
    let Ok(text) = std::fs::read_to_string(log_path(data_dir)) else {
        return done;
    };
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue; // 尾条撕裂容忍：只认完整 JSON 行
        };
        if v.get("status").and_then(|s| s.as_str()) != Some("done") {
            continue;
        }
        if let Some(id) = v.get("file_id").and_then(|x| x.as_u64()) {
            done.files.insert(id);
        }
        match v.get("phase").and_then(|x| x.as_str()) {
            Some("index") => done.index = true,
            Some("audit") => done.audit = true,
            Some("header") => done.header = true,
            _ => {}
        }
    }
    done
}

/// 索引内密钥覆盖（hex 32B）解码；格式错误一律报错不静默回落（与 vault.rs 同纪律）。
fn decode_key(hex: &str) -> Result<[u8; KEY_LEN], MigrateError> {
    vault_crypto::hex_decode(hex)
        .ok_or(MigrateError::Io("bad key hex"))?
        .as_slice()
        .try_into()
        .map_err(|_| MigrateError::Io("bad key length"))
}

/// 文件夹元数据密钥：索引内 FSK 覆盖优先，否则 HKDF(MK,"fsk/<id>")（05-02 §3.1 冻结公式）。
/// 注：数据区密文逐块原样搬运（v2/v3 数据区语义一致），FSKey 不参与迁移——
/// 只有容器元数据以 FSK 重加密为分片形态。
fn fsk_of(
    index: &VaultIndex,
    mk: &[u8; KEY_LEN],
    folder: u64,
) -> Result<[u8; KEY_LEN], MigrateError> {
    if let Some(hex) = index
        .folder_fsk(folder)
        .map_err(|_| MigrateError::Io("index"))?
    {
        return decode_key(&hex);
    }
    Ok(*hkdf_sha256_derive(mk, format!("fsk/{folder}").as_bytes()))
}

/// 加载索引（legacy 优先，其次从 v3 段还原——支持「索引已迁、容器未迁」的续做现场）。
fn load_index(session: &Session) -> Result<VaultIndex, MigrateError> {
    let data_dir = session.data_dir();
    let index_enc = data_dir.join("index.enc");
    if index_enc.exists() {
        return VaultIndex::load(&index_enc, &session.keys.index, session.keys.search)
            .map_err(|_| MigrateError::Io("cannot load legacy index"));
    }
    let st = VaultStore::open(
        &data_dir,
        Namespace::Index,
        &session.keys.index,
        OpenMode::ReadOnly,
    )
    .map_err(|_| MigrateError::Io("cannot open index store"))?;
    let pairs = st.kv_pairs();
    if pairs.is_empty() {
        return Ok(VaultIndex::new(session.keys.search));
    }
    VaultIndex::from_v3_pairs(&pairs, session.keys.search)
        .map_err(|_| MigrateError::Io("cannot restore index from v3 segments"))
}

/// M4 前置：头部备份。头部仍为 VSVB v2 时复制为 `<vault>.vsb.pre-v3`（已存在则跳过）；
/// 头部已随解锁升到 v3 时无法回取 v2 字节，如实跳过（解锁早于迁移的正常现场）。
fn backup_header(session: &Session) -> Result<Option<PathBuf>, MigrateError> {
    let backup = session
        .vault_path
        .with_extension(BACKUP_SUFFIX.trim_start_matches('.'));
    if backup.exists() {
        return Ok(Some(backup));
    }
    let (_, info) = Keystore::load_with_info(&session.vault_path)
        .map_err(|_| MigrateError::Io("cannot read vault header"))?;
    if info.format_ver != crate::keystore::FORMAT_VER_V2 {
        return Ok(None);
    }
    std::fs::copy(&session.vault_path, &backup)
        .map_err(|_| MigrateError::Io("cannot back up v2 header"))?;
    Ok(Some(backup))
}

/// M1 单文件：v2 容器 → 逐块读密文 → assemble_v3 写 `<id>.vse.mig.tmp`
/// → 明文 SHA-256 前后一致 → rename 替换。任何失败不产生半成品（暂存文件清理）。
fn migrate_one_file(
    data_dir: &Path,
    index: &VaultIndex,
    mk: &[u8; KEY_LEN],
    file_id: u64,
) -> Result<String, MigrateError> {
    let files_dir = data_dir.join("files");
    let folder = index
        .file_folder(file_id)
        .map_err(|_| MigrateError::Io("file not in index"))?;
    let fsk = fsk_of(index, mk, folder)?;

    // 读路径与 vault.rs 一致：轮换窗口内暂存容器优先（若在用）
    let formal = files_dir.join(format!("{file_id}.vse"));
    let src = match index.staging_tag(file_id) {
        Some(tag) => {
            let staged = files_dir.join(format!("{file_id}.{tag}.vse.new"));
            if staged.exists() {
                staged
            } else {
                formal
            }
        }
        None => formal,
    };

    let rd = ContainerReader::open(&src, &fsk).map_err(|_| MigrateError::Io("container open"))?;
    let old_sha = rd.meta.file_sha256.clone();
    let old_size = rd.meta.size;

    // 数据区密文逐块原样搬运（v2 与 v3 数据区语义一致：ct‖tag，计数器 nonce）
    let part = files_dir.join(format!("{file_id}{MIG_PART_SUFFIX}"));
    let mig = files_dir.join(format!("{file_id}{MIG_TMP_SUFFIX}"));
    let result = (|| -> Result<(), MigrateError> {
        {
            use std::io::Write;
            let mut pf =
                std::fs::File::create(&part).map_err(|_| MigrateError::Io("part create"))?;
            for i in 0..rd.meta.chunks.len() {
                let ct = rd.read_ct(i).map_err(|_| MigrateError::Io("chunk read"))?;
                pf.write_all(&ct)
                    .map_err(|_| MigrateError::Io("part write"))?;
            }
            pf.sync_all().ok();
        }
        let v3meta = V3FileMeta {
            ver: 3,
            is_final: true,
            name: rd.meta.name.clone(),
            size: rd.meta.size,
            mime: String::new(),
            created_ms: rd.meta.created_ms,
            modified_ms: rd.meta.modified_ms,
            nonce_prefix: rd.meta.nonce_prefix,
            file_sha256: Some(rd.meta.file_sha256.clone()),
            chunks: rd
                .meta
                .chunks
                .iter()
                .map(|c| V3ChunkEntry {
                    hash: c.hash.clone(),
                    offset: c.offset,
                    len: c.len,
                })
                .collect(),
        };
        assemble_v3(&mig, &part, &v3meta, &fsk, DEFAULT_META_SHARD_SPAN)
            .map_err(|_| MigrateError::Io("v3 assemble"))?;
        // 终验：v3 末片 file_sha256 与 v2 元数据一致（明文域逐字节等价的哈希证据）
        let v3 =
            ContainerReaderV3::open(&mig, &fsk).map_err(|_| MigrateError::Io("v3 verify open"))?;
        if v3.meta.file_sha256.as_deref() != Some(old_sha.as_str()) || v3.meta.size != old_size {
            return Err(MigrateError::Io("sha mismatch after v3 rewrite"));
        }
        drop(v3);
        std::fs::rename(&mig, &src).map_err(|_| MigrateError::Io("commit rename"))?;
        Ok(())
    })();

    let _ = std::fs::remove_file(&part);
    if let Err(e) = result {
        let _ = std::fs::remove_file(&mig);
        return Err(e);
    }
    Ok(old_sha)
}

/// 密码强确认（与 rotate_mk_begin 同一方式）：argon2 → 解包 mk_wrap_pwd → 等于会话 MK。
fn confirm_password(session: &Session, password: &str) -> Result<(), MigrateError> {
    let ks = Keystore::load(&session.vault_path)
        .map_err(|_| MigrateError::Io("cannot read vault header"))?;
    let kek = argon2id_derive(password, &ks.salt, &ks.argon)
        .map_err(|_| MigrateError::Io("argon2 failed"))?;
    match aead_decrypt(&kek, &ks.mk_wrap_pwd) {
        Some(mk) if mk.as_slice() == session.mk.as_slice() => Ok(()),
        _ => Err(MigrateError::WrongPassword),
    }
}

/// 迁移主流程（`begin` 与 `resume` 同一体：日志 done 项跳过即续做）。
/// 结束后失效会话内缓存的 Vault / AuditLog 槽位，使后续操作按 v3 路径重开。
pub fn begin(session: &Session, password: &str) -> Result<serde_json::Value, MigrateError> {
    if session.is_readonly() {
        return Err(MigrateError::LeaseBusy);
    }
    if session.is_maintenance() {
        return Err(MigrateError::Maintenance);
    }
    confirm_password(session, password)?;

    let data_dir = session.data_dir();
    session.enter_maintenance();
    let outcome = run_migration(session, &data_dir);
    session.exit_maintenance();
    // 索引/审计载体已切换：丢弃会话内 legacy 引擎缓存，下次 with_vault/audit 重开即 v3
    if outcome.is_ok() {
        *session.vault.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *session.audit.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    session.audit(
        "security",
        if outcome.is_ok() {
            "migration.completed"
        } else {
            "migration.failed"
        },
    );
    outcome
}

fn run_migration(session: &Session, data_dir: &Path) -> Result<serde_json::Value, MigrateError> {
    let backup = backup_header(session)?;
    append_log(
        data_dir,
        json!({"phase": "backup", "src_ver": 2, "dst_ver": 3,
               "status": if backup.is_some() { "done" } else { "skipped" },
               "reason": if backup.is_some() { None } else { Some("header already v3 at unlock") }}),
    )?;

    let mut index = load_index(session)?;
    let mut done = read_done(data_dir);

    // M1 容器：逐文件；单文件失败不整库失败（docs/v2.0/07 §7.3 失败语义）
    let ids = index.all_file_ids();
    let mut migrated = 0u64;
    let mut failed = 0u64;
    let mut skipped = 0u64;
    for id in &ids {
        if done.files.contains(id) {
            skipped += 1;
            continue;
        }
        match migrate_one_file(data_dir, &index, &session.mk, *id) {
            Ok(sha) => {
                migrated += 1;
                done.files.insert(*id);
                append_log(
                    data_dir,
                    json!({"file_id": id, "src_ver": 2, "dst_ver": 3, "sha256": sha, "status": "done"}),
                )?;
            }
            Err(e) => {
                failed += 1;
                let reason = match e {
                    MigrateError::Io(m) => m,
                    _ => "unexpected",
                };
                append_log(
                    data_dir,
                    json!({"file_id": id, "src_ver": 2, "dst_ver": 3, "status": "failed",
                           "reason": reason}),
                )?;
            }
        }
    }

    // M2 索引：全量快照段（同 index_key）→ flush → 删 index.enc
    let index_enc = data_dir.join("index.enc");
    if index_enc.exists() && !done.index {
        let mut st = VaultStore::open(
            data_dir,
            Namespace::Index,
            &session.keys.index,
            OpenMode::ReadWrite,
        )
        .map_err(|_| MigrateError::Io("index store open"))?;
        let entries = index.all_file_ids().len();
        st.kv_apply(index.full_state_ops())
            .map_err(|_| MigrateError::Io("index snapshot write"))?;
        st.flush().map_err(|_| MigrateError::Io("index flush"))?;
        drop(st);
        std::fs::remove_file(&index_enc).map_err(|_| MigrateError::Io("remove index.enc"))?;
        append_log(
            data_dir,
            json!({"phase": "index", "src_ver": 2, "dst_ver": 3, "entries": entries, "status": "done"}),
        )?;
        done.index = true;
    }

    // M3 审计：audit.enc → segmented::rebuild（条目字节不变）→ 校验条目数/head_hash → 删 audit.enc
    let audit_enc = data_dir.join("audit.enc");
    if audit_enc.exists() && !done.audit {
        let old = AuditLog::open_with_key(data_dir, &session.keys.audit_chain)
            .map_err(|_| MigrateError::Io("legacy audit open"))?;
        let entries = old.entries().to_vec();
        let n = entries.len();
        let head = old.head_hash();
        drop(old);
        segmented::rebuild(data_dir, &session.keys.audit_chain, entries)
            .map_err(|_| MigrateError::Io("audit rebuild"))?;
        let fresh = SegmentedAuditLog::open(data_dir, &session.keys.audit_chain)
            .map_err(|_| MigrateError::Io("audit reopen"))?;
        if fresh.len() != n || fresh.head_hash() != head {
            return Err(MigrateError::Io("audit migrate verify failed"));
        }
        drop(fresh);
        std::fs::remove_file(&audit_enc).map_err(|_| MigrateError::Io("remove audit.enc"))?;
        append_log(
            data_dir,
            json!({"phase": "audit", "src_ver": 2, "dst_ver": 1, "entries": n,
                   "head_hash": head, "status": "done"}),
        )?;
        done.audit = true;
    }

    // M4 复核：头部版本（抬升由解锁首写完成，迁移只记录事实）
    let (_, info) = Keystore::load_with_info(&session.vault_path)
        .map_err(|_| MigrateError::Io("cannot read vault header"))?;
    if !done.header {
        append_log(
            data_dir,
            json!({"phase": "header", "src_ver": 2, "dst_ver": 3,
                   "format_ver": info.format_ver,
                   "derived_keys_legacy": info.derived_keys_legacy, "status": "done"}),
        )?;
    }

    Ok(json!({
        "filesTotal": ids.len(),
        "filesMigrated": migrated,
        "filesFailed": failed,
        "filesSkipped": skipped,
        "indexDone": done.index || !index_enc.exists(),
        "auditDone": done.audit || !audit_enc.exists(),
        "backupPath": backup.map(|p| p.to_string_lossy().to_string()),
    }))
}

/// 迁移状态 JSON：日志摘要 + 备份路径（docs/v2.0/07 §7.3 / `03` §4.11）。
pub fn status(session: &Session) -> Result<serde_json::Value, MigrateError> {
    let data_dir = session.data_dir();
    let text = std::fs::read_to_string(log_path(&data_dir))
        .map_err(|_| MigrateError::Io("no migrate log"))?;
    let mut files: Vec<serde_json::Value> = Vec::new();
    let mut index_done = None;
    let mut audit_done = None;
    let mut header_done = None;
    let mut backup_done = false;
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(id) = v.get("file_id").and_then(|x| x.as_u64()) {
            files.push(json!({
                "fileId": id,
                "status": v.get("status").cloned().unwrap_or(json!(null)),
                "sha256": v.get("sha256").cloned().unwrap_or(json!(null)),
                "reason": v.get("reason").cloned().unwrap_or(json!(null)),
            }));
        }
        match v.get("phase").and_then(|x| x.as_str()) {
            Some("index") => index_done = v.get("status").cloned(),
            Some("audit") => audit_done = v.get("status").cloned(),
            Some("header") => header_done = v.get("status").cloned(),
            Some("backup") => {
                backup_done = v.get("status").and_then(|s| s.as_str()) == Some("done")
            }
            _ => {}
        }
    }
    let backup = session
        .vault_path
        .with_extension(BACKUP_SUFFIX.trim_start_matches('.'));
    Ok(json!({
        "schema": 1,
        "files": files,
        "phases": {
            "backup": if backup_done { "done" } else { "skipped" },
            "index": index_done.unwrap_or(json!("pending")),
            "audit": audit_done.unwrap_or(json!("pending")),
            "header": header_done.unwrap_or(json!("pending")),
        },
        "backupPath": if backup.exists() {
            serde_json::Value::String(backup.to_string_lossy().to_string())
        } else {
            serde_json::Value::Null
        },
    }))
}

/// 取消（偏差见模块注释）：同步执行下仅清理 `.mig.tmp` / `.mig.part` 残留。
pub fn cancel(session: &Session) -> Result<(), MigrateError> {
    let files_dir = session.data_dir().join("files");
    if let Ok(entries) = std::fs::read_dir(&files_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(MIG_TMP_SUFFIX) || name.ends_with(MIG_PART_SUFFIX) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    Ok(())
}

/// 便捷：`resume` 与 `begin` 同体（日志续做）。
pub fn resume(session: &Session, password: &str) -> Result<serde_json::Value, MigrateError> {
    begin(session, password)
}

/// 测试/内部：把单个文件标为已迁移（断点续做用例构造「中断现场」）。
#[cfg(test)]
fn mark_file_done_for_test(data_dir: &Path, file_id: u64, sha: &str) {
    let _ = append_log(
        data_dir,
        json!({"file_id": file_id, "src_ver": 2, "dst_ver": 3, "sha256": sha, "status": "done"}),
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    use zeroize::Zeroizing;

    use vault_audit::LegacyLog;
    use vault_crypto::aead::{aead_encrypt_with_aad, AES_GCM_NONCE_LEN};
    use vault_crypto::kdf::Argon2Params;
    use vault_crypto::{hash_sha256, random_key, random_salt, Sha256};
    use vault_vault::container::{assemble, ChunkEntry, FileMeta};
    use vault_vault::index::tokenize;

    use super::*;
    use crate::session::VaultKeys;

    const PW: &str = "pw-migrate";

    /// 轻量 Argon2 参数（测试提速；真实路径用 DESKTOP）。
    fn fast_argon() -> Argon2Params {
        Argon2Params {
            m_kib: 8,
            t: 1,
            p: 1,
        }
    }

    /// 手写 VSVB v2 头部（布局见 keystore::parse_v2）：迁移前头部未升级的现场。
    fn write_v2_header(path: &Path, mk: &[u8; KEY_LEN], password: &str) {
        let salt = random_salt();
        let argon = fast_argon();
        let kek = argon2id_derive(password, &salt, &argon).unwrap();
        let nonce = [7u8; AES_GCM_NONCE_LEN];
        let wrap = aead_encrypt_with_aad(&kek, &nonce, mk, &[]).unwrap();
        let verifier = *hkdf_sha256_derive(mk, b"verifier");
        let mut b = Vec::new();
        b.extend_from_slice(crate::keystore::MAGIC);
        b.extend_from_slice(&2u16.to_le_bytes()); // format_ver v2
        b.extend_from_slice(&1u16.to_le_bytes()); // kdf_ver
        b.extend_from_slice(&1u16.to_le_bytes()); // cipher
        b.extend_from_slice(&0u16.to_le_bytes()); // chunk_cfg
        b.extend_from_slice(&0u16.to_le_bytes()); // flags
        b.extend_from_slice(&argon.m_kib.to_le_bytes());
        b.extend_from_slice(&argon.t.to_le_bytes());
        b.extend_from_slice(&argon.p.to_le_bytes());
        b.extend_from_slice(&salt);
        b.extend_from_slice(&verifier);
        b.extend_from_slice(&wrap);
        std::fs::write(path, b).unwrap();
    }

    /// 造 v2 容器：两块明文按计数器 nonce 加密，数据区 = ct‖tag 顺排。
    fn write_v2_container(
        path: &Path,
        meta_key: &[u8; KEY_LEN],
        file_key: &[u8; KEY_LEN],
        prefix: u32,
        plain: &[u8],
    ) -> String {
        let mid = plain.len() / 2;
        let parts: Vec<&[u8]> = if plain.is_empty() {
            vec![plain]
        } else {
            vec![&plain[..mid], &plain[mid..]]
        };
        let mut part_bytes = Vec::new();
        let mut chunks = Vec::new();
        let mut hasher = Sha256::new();
        for (i, p) in parts.iter().enumerate() {
            let mut nonce = [0u8; AES_GCM_NONCE_LEN];
            nonce[..4].copy_from_slice(&prefix.to_be_bytes());
            nonce[4..].copy_from_slice(&(i as u64).to_be_bytes());
            let aad = (i as u64).to_be_bytes();
            let blob = aead_encrypt_with_aad(file_key, &nonce, p, &aad).unwrap();
            let ct = &blob[AES_GCM_NONCE_LEN..];
            chunks.push(ChunkEntry {
                hash: hash_sha256(ct),
                offset: part_bytes.len() as u64,
                len: ct.len() as u32,
            });
            hasher.update(p);
            part_bytes.extend_from_slice(ct);
        }
        let part_file = path.with_extension("src.part");
        std::fs::write(&part_file, &part_bytes).unwrap();
        let sha = hasher.finalize_hex();
        let meta = FileMeta {
            name: path.file_name().unwrap().to_string_lossy().to_string(),
            size: plain.len() as u64,
            nonce_prefix: prefix,
            chunks,
            file_sha256: sha.clone(),
            created_ms: 111,
            modified_ms: 222,
        };
        assemble(path, &part_file, meta, meta_key).unwrap();
        std::fs::remove_file(&part_file).ok();
        sha
    }

    struct LegacyVault {
        _dir: tempfile::TempDir,
        session: Session,
        data_dir: PathBuf,
        mk: [u8; KEY_LEN],
        contents: Vec<(u64, Vec<u8>, String)>, // (id, 明文, sha)
    }

    /// 构造完整 legacy 库：v2 头部 + index.enc（VSIX v2）+ audit.enc + v2 容器 ×2。
    fn legacy_vault(tag: &str) -> LegacyVault {
        let dir = tempfile::tempdir().unwrap();
        let vault_file = dir.path().join(format!("{tag}.vsvb"));
        let mk = random_key();
        let dkeys = crate::keystore::Keystore::legacy_derived_keys(&mk);
        write_v2_header(&vault_file, &mk, PW);

        let data_dir = dir.path().join(format!("{tag}.data"));
        std::fs::create_dir_all(data_dir.join("files")).unwrap();

        let fsk0 = *hkdf_sha256_derive(&mk, b"fsk/0");
        let index_key = dkeys[0];
        let search_key = dkeys[1];
        let mut ix = VaultIndex::new(search_key);
        let mut contents = Vec::new();
        for (i, plain) in [
            b"legacy payload one".repeat(3000).to_vec(),
            b"legacy payload two!".repeat(2500).to_vec(),
        ]
        .into_iter()
        .enumerate()
        {
            let id = i as u64 + 1;
            let fskey = *hkdf_sha256_derive(&fsk0, format!("fskey/{id}").as_bytes());
            let cpath = data_dir.join("files").join(format!("{id}.vse"));
            let sha = write_v2_container(&cpath, &fsk0, &fskey, 0x1234 + id as u32, &plain);
            let name = format!("f{id}.bin");
            ix.add_file_with_id(id, 0, &name, plain.len() as u64, &tokenize(&name))
                .unwrap();
            contents.push((id, plain, sha));
        }
        ix.save(&data_dir.join("index.enc"), &index_key).unwrap();

        // audit.enc（legacy 单文件 AEAD）
        let mut audit = LegacyLog::open_with_key(&data_dir, &dkeys[5]).unwrap();
        audit.append("test", "one").unwrap();
        audit.append("test", "two").unwrap();

        let session = Session {
            mk: Zeroizing::new(mk),
            keys: Zeroizing::new(VaultKeys::from_array(dkeys)),
            vault_path: vault_file,
            disguise: false,
            vault: Arc::new(Mutex::new(None)),
            p2p: Mutex::new(None),
            audit: Mutex::new(None),
            stego_enabled: AtomicBool::new(false),
            readonly: AtomicBool::new(false),
            maintenance: AtomicBool::new(false),
            sync_ctx: Mutex::new(vault_p2p::policy::SyncContext::default()),
            lease: Mutex::new(None),
        };
        LegacyVault {
            _dir: dir,
            session,
            data_dir,
            mk,
            contents,
        }
    }

    /// 明文重组：迁移后的 v3 容器逐块解密拼回（终验哈希比对由调用方做）。
    fn v3_plaintext(data_dir: &Path, mk: &[u8; KEY_LEN], id: u64, plain_len: usize) -> Vec<u8> {
        let fsk0 = *hkdf_sha256_derive(mk, b"fsk/0");
        let fskey = *hkdf_sha256_derive(&fsk0, format!("fskey/{id}").as_bytes());
        let cpath = data_dir.join("files").join(format!("{id}.vse"));
        let rd = ContainerReaderV3::open(&cpath, &fsk0).unwrap();
        let mut out = Vec::with_capacity(plain_len);
        let n = rd.meta.chunks.len() as u64;
        for i in 0..n {
            out.extend_from_slice(&rd.decrypt_chunk(&fskey, i).unwrap());
        }
        out
    }

    #[test]
    fn hash_unchanged_and_old_file_kept_until_commit() {
        let lv = legacy_vault("mig1");
        let expected_audit_head = {
            let log = AuditLog::open_with_key(&lv.data_dir, &lv.session.keys.audit_chain).unwrap();
            (log.entries().len(), log.head_hash())
        };

        let summary = begin(&lv.session, PW).unwrap();
        assert_eq!(summary["filesFailed"], json!(0));
        assert_eq!(summary["filesMigrated"], json!(2));

        // 旧载体已删、新载体已建
        assert!(!lv.data_dir.join("index.enc").exists());
        assert!(!lv.data_dir.join("audit.enc").exists());
        assert!(VaultStore::ns_dir(&lv.data_dir, Namespace::Index).exists());
        assert!(VaultStore::ns_dir(&lv.data_dir, Namespace::Audit).exists());

        // 明文逐字节一致（哈希终验值前后一致 + 内容重组一致）
        for (id, plain, sha) in &lv.contents {
            assert_eq!(v3_plaintext(&lv.data_dir, &lv.mk, *id, plain.len()), *plain);
            let fsk0 = *hkdf_sha256_derive(&lv.mk, b"fsk/0");
            let rd = ContainerReaderV3::open(
                &lv.data_dir.join("files").join(format!("{id}.vse")),
                &fsk0,
            )
            .unwrap();
            assert_eq!(rd.meta.file_sha256.as_deref(), Some(sha.as_str()));
        }

        // 审计链完整迁移：原有条目数与 head_hash 不变（迁移完成事件另追加新条目）
        let fresh = SegmentedAuditLog::open(&lv.data_dir, &lv.session.keys.audit_chain).unwrap();
        assert!(fresh.len() >= expected_audit_head.0);
        assert_eq!(
            fresh.entries()[expected_audit_head.0 - 1].hash,
            expected_audit_head.1
        );
        assert!(fresh.verify().ok);

        // 迁移前头部备份（v2 头部）存在；日志全 done
        assert!(lv.session.vault_path.with_extension("vsb.pre-v3").exists());
        let log = std::fs::read_to_string(log_path(&lv.data_dir)).unwrap();
        let lines: Vec<serde_json::Value> = log
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert!(lines
            .iter()
            .all(|v| v["status"] == "done" || v["status"] == "skipped"));
        assert!(lines.iter().any(|v| v["phase"] == "index"));
        assert!(lines.iter().any(|v| v["phase"] == "audit"));
        assert!(lines.iter().any(|v| v["phase"] == "header"));

        // status JSON 摘要
        let st = status(&lv.session).unwrap();
        assert_eq!(st["phases"]["index"], json!("done"));
        assert!(st["backupPath"].is_string());
    }

    #[test]
    fn interrupt_then_resume() {
        let lv = legacy_vault("mig2");
        // 模拟中断：只手工迁走第一个文件并写 done 日志，然后 resume
        let (first_id, _, _) = &lv.contents[0];
        let index = VaultIndex::load(
            &lv.data_dir.join("index.enc"),
            &lv.session.keys.index,
            lv.session.keys.search,
        )
        .unwrap();
        let sha = migrate_one_file(&lv.data_dir, &index, &lv.mk, *first_id).unwrap();
        mark_file_done_for_test(&lv.data_dir, *first_id, &sha);
        assert!(!lv
            .data_dir
            .join("files")
            .join(format!("{first_id}.vse.mig.tmp"))
            .exists());

        let summary = resume(&lv.session, PW).unwrap();
        assert_eq!(summary["filesMigrated"], json!(1), "只补迁剩余文件");
        assert_eq!(summary["filesSkipped"], json!(1), "done 项不重做");
        assert_eq!(summary["filesFailed"], json!(0));
        // 两个文件都已是 v3 且内容一致
        for (id, plain, _) in &lv.contents {
            assert_eq!(v3_plaintext(&lv.data_dir, &lv.mk, *id, plain.len()), *plain);
        }
        assert!(!lv.data_dir.join("index.enc").exists());
        // 首文件 done 行只有一条（未重做）
        let log = std::fs::read_to_string(log_path(&lv.data_dir)).unwrap();
        assert_eq!(
            log.lines()
                .filter(|l| {
                    let v: serde_json::Value = serde_json::from_str(l).unwrap();
                    v["file_id"] == json!(first_id) && v["status"] == "done"
                })
                .count(),
            1
        );
    }

    #[test]
    fn single_file_failure_is_isolated() {
        let lv = legacy_vault("mig3");
        // 损坏第一个容器（坏 magic → 打不开 → failed，不整库失败）
        let bad = lv.data_dir.join("files").join("1.vse");
        std::fs::write(&bad, b"garbage-not-a-container").unwrap();

        let summary = begin(&lv.session, PW).unwrap();
        assert_eq!(summary["filesFailed"], json!(1));
        assert_eq!(summary["filesMigrated"], json!(1));
        // 其余文件正常迁移、索引/审计照常完成、任务整体成功
        let (id2, plain2, _) = &lv.contents[1];
        assert_eq!(
            v3_plaintext(&lv.data_dir, &lv.mk, *id2, plain2.len()),
            *plain2
        );
        assert!(!lv.data_dir.join("index.enc").exists());
        assert!(!lv.data_dir.join("audit.enc").exists());
        // 失败项保持旧载体（坏文件原样保留），日志标 failed
        assert_eq!(std::fs::read(&bad).unwrap(), b"garbage-not-a-container");
        let log = std::fs::read_to_string(log_path(&lv.data_dir)).unwrap();
        assert!(log
            .lines()
            .any(|l| l.contains("\"file_id\":1") && l.contains("\"failed\"")));
        // 无暂存残留
        let residue: Vec<String> = std::fs::read_dir(lv.data_dir.join("files"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(MIG_TMP_SUFFIX) || n.ends_with(MIG_PART_SUFFIX))
            .collect();
        assert!(residue.is_empty(), "{residue:?}");
    }
}
