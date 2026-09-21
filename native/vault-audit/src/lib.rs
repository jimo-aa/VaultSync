//! 链式哈希审计日志（docs/05-06 §三，P5-1）。
//!
//! 递推规则：hash_i = SHA256( prev_hash ‖ seq(be) ‖ ts_ms(be) ‖ len(kind)(be) ‖ kind ‖
//! len(detail)(be) ‖ detail )，逐条串联；创世条目的 prev_hash 为 32 个 0 字节的 hex。
//! 哈希只覆盖上述确定性字节拼接，不用 JSON（键序不稳会破坏可复现性）。
//!
//! 落盘：整体 JSON 经 AEAD 加密为单文件 `<data_dir>/audit.enc`，密钥 = HKDF(MK,"audit-log")；
//! 写盘为「先写 .tmp 再 rename」的原子替换。明文缓冲只在内存短暂存在，
//! [`AuditLog::to_plaintext_json`] 仅供链校验与测试，禁止直接落盘。
//!
//! 导出（docs/05-06 §3.3）：明文为信封 `{"version":1,"exportedMs":…,"entries":[…]}`，
//! 用调用方给的专用导出密钥（vault-core 传 HKDF(MK,"audit-export")）加密。
//! 多设备（§3.1）：每设备维护独立链，[`AuditLog::head_hash`] 供跨设备交叉核对。
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![deny(warnings)]

pub mod segmented;
pub use segmented::SegmentedAuditLog;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vault_crypto::kdf::hkdf_sha256_derive;
use vault_crypto::{
    aead_decrypt, aead_encrypt, random_bytes, Sha256, AES_GCM_NONCE_LEN, AES_GCM_TAG_LEN, KEY_LEN,
};

/// 落盘与导出件信封的格式版本。
pub const AUDIT_FORMAT_VER: u16 = 1;

/// 审计日志文件名（位于保险箱数据目录下）。
const AUDIT_FILE: &str = "audit.enc";

/// 日志密钥的 HKDF info。
const AUDIT_KEY_INFO: &[u8] = b"audit-log";

/// 一条审计记录。kind 为短类别串（`vault` / `device` / `session` / `security`，docs/05-06 §3.2）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// 序号，从 1 起连续。
    pub seq: u64,
    /// 记录时刻（Unix 毫秒）。
    pub ts_ms: u64,
    pub kind: String,
    pub detail: String,
    /// 前一条的 hash（hex，64 字符）；创世条目为 32 个 0 字节的 hex。
    pub prev_hash: String,
    /// 本条哈希（hex，64 字符）。
    pub hash: String,
}

/// 链校验结论。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditVerify {
    pub ok: bool,
    /// 已通过校验的条目数；失败时为出错条目之前（不含）的条目数。
    pub checked: usize,
    /// 首个出错条目的 seq。
    pub broken_at: Option<u64>,
    /// 可读原因。
    pub reason: Option<String>,
}

/// 落盘形态（整体 AEAD 加密前的明文 JSON）。
#[derive(Serialize, Deserialize)]
struct LogData {
    #[serde(default = "format_ver")]
    version: u16,
    #[serde(default)]
    entries: Vec<AuditEntry>,
}

/// 导出件信封（docs/05-06 §3.3）。
#[derive(Serialize, Deserialize)]
struct ExportEnvelope {
    #[serde(default = "format_ver")]
    version: u16,
    #[serde(rename = "exportedMs", default)]
    exported_ms: u64,
    #[serde(default)]
    entries: Vec<AuditEntry>,
}

fn format_ver() -> u16 {
    AUDIT_FORMAT_VER
}

pub struct LegacyLog {
    d: LogData,
    path: PathBuf,
    key: [u8; KEY_LEN],
}

/// 审计日志（P6-4 起双路径）：
/// - `Legacy`：v0.5.0 单文件 `audit.enc`（保持原读写语义，P7-9 迁移）；
/// - `Segmented`：VSAU v1 分段存储（新建库），append O(1)。
///
/// 变体装箱：两路径内部状态尺寸差异大，枚举保持小。
pub enum AuditLog {
    Legacy(Box<LegacyLog>),
    Segmented(Box<SegmentedAuditLog>),
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 创世条目的 prev_hash：32 个 0 字节的 hex（64 字符）。
pub(crate) fn genesis_prev_hash() -> String {
    vault_crypto::hex_encode(&[0u8; 32])
}

/// 链校验（对任意条目序列）；LegacyLog::verify 与分段路径共用。
pub(crate) fn verify_entries(entries: &[AuditEntry]) -> AuditVerify {
    let mut expected_prev = genesis_prev_hash();
    for (i, e) in entries.iter().enumerate() {
        let expected_seq = i as u64 + 1;
        if e.seq != expected_seq {
            return broken(
                i,
                e.seq,
                format!("seq 不连续：期望 {expected_seq}，实际 {}", e.seq),
            );
        }
        if e.prev_hash != expected_prev {
            return broken(
                i,
                e.seq,
                format!("链断：第 {} 条 prev_hash 与前一条 hash 不符", e.seq),
            );
        }
        let recomputed = compute_hash(&e.prev_hash, e.seq, e.ts_ms, &e.kind, &e.detail);
        if recomputed != e.hash {
            return broken(i, e.seq, format!("哈希不匹配：第 {} 条内容已被改动", e.seq));
        }
        expected_prev = e.hash.clone();
    }
    AuditVerify {
        ok: true,
        checked: entries.len(),
        broken_at: None,
        reason: None,
    }
}

/// 链式哈希：确定性字节拼接（长度前缀防拼接歧义）。
pub(crate) fn compute_hash(
    prev_hash: &str,
    seq: u64,
    ts_ms: u64,
    kind: &str,
    detail: &str,
) -> String {
    let mut h = Sha256::new();
    h.update(prev_hash.as_bytes());
    h.update(&seq.to_be_bytes());
    h.update(&ts_ms.to_be_bytes());
    h.update(&(kind.len() as u32).to_be_bytes());
    h.update(kind.as_bytes());
    h.update(&(detail.len() as u32).to_be_bytes());
    h.update(detail.as_bytes());
    h.finalize_hex()
}

/// AEAD 加密：随机 nonce，返回 nonce ‖ ct ‖ tag（整体落盘）。
fn encrypt_blob(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<Vec<u8>, &'static str> {
    let mut nonce = [0u8; AES_GCM_NONCE_LEN];
    nonce.copy_from_slice(&random_bytes(AES_GCM_NONCE_LEN));
    aead_encrypt(key, &nonce, plaintext)
}

fn decrypt_blob(
    key: &[u8; KEY_LEN],
    blob: &[u8],
    truncated: &'static str,
) -> Result<Vec<u8>, &'static str> {
    if blob.len() < AES_GCM_NONCE_LEN + AES_GCM_TAG_LEN {
        return Err(truncated);
    }
    aead_decrypt(key, blob).ok_or("audit decrypt failed (wrong key or tampered)")
}

/// 原子写：临时名 + rename。
fn write_atomic(path: &Path, data: &[u8]) -> Result<(), &'static str> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data).map_err(|_| "cannot write audit log")?;
    std::fs::rename(&tmp, path).map_err(|_| "cannot finalize audit log")
}

fn broken(checked: usize, broken_at: u64, reason: String) -> AuditVerify {
    AuditVerify {
        ok: false,
        checked,
        broken_at: Some(broken_at),
        reason: Some(reason),
    }
}

impl AuditLog {
    /// 打开（不存在则新建空日志）：密钥 = HKDF(MK,"audit-log")。
    /// 双路径：存在 `audit.enc` → legacy；否则 → VSAU v1 分段。
    pub fn open(data_dir: &Path, mk: &[u8; KEY_LEN]) -> Result<Self, &'static str> {
        Self::open_with_key(data_dir, &hkdf_sha256_derive(mk, AUDIT_KEY_INFO))
    }

    /// 用调用方提供的密钥打开（测试 / 密钥轮换重加密场景）。
    pub fn open_with_key(data_dir: &Path, key: &[u8; KEY_LEN]) -> Result<Self, &'static str> {
        if data_dir.join(AUDIT_FILE).exists() {
            Ok(AuditLog::Legacy(Box::new(LegacyLog::open_with_key(
                data_dir, key,
            )?)))
        } else {
            Ok(AuditLog::Segmented(Box::new(SegmentedAuditLog::open(
                data_dir, key,
            )?)))
        }
    }

    /// 追加一条并立即落盘（审计不得滞后于操作）。
    pub fn append(&mut self, kind: &str, detail: &str) -> Result<AuditEntry, &'static str> {
        match self {
            AuditLog::Legacy(l) => l.append(kind, detail),
            AuditLog::Segmented(l) => l.append(kind, detail),
        }
    }

    pub fn entries(&self) -> &[AuditEntry] {
        match self {
            AuditLog::Legacy(l) => l.entries(),
            AuditLog::Segmented(l) => l.entries(),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            AuditLog::Legacy(l) => l.len(),
            AuditLog::Segmented(l) => l.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            AuditLog::Legacy(l) => l.is_empty(),
            AuditLog::Segmented(l) => l.is_empty(),
        }
    }

    pub fn verify(&self) -> AuditVerify {
        match self {
            AuditLog::Legacy(l) => l.verify(),
            AuditLog::Segmented(l) => l.verify(),
        }
    }

    /// 链头哈希（空日志返回创世零哈希），供跨设备交叉核对（docs/05-06 §3.1）。
    pub fn head_hash(&self) -> String {
        match self {
            AuditLog::Legacy(l) => l.head_hash(),
            AuditLog::Segmented(l) => l.head_hash(),
        }
    }

    /// 导出为加密文件：信封明文用调用方给的专用导出密钥加密（docs/05-06 §3.3）。
    pub fn export_encrypted(
        &self,
        dest: &Path,
        export_key: &[u8; KEY_LEN],
    ) -> Result<(), &'static str> {
        let env = ExportEnvelope {
            version: AUDIT_FORMAT_VER,
            exported_ms: now_ms(),
            entries: self.entries().to_vec(),
        };
        let json = serde_json::to_vec(&env).map_err(|_| "audit export serialize failed")?;
        let blob = encrypt_blob(export_key, &json)?;
        write_atomic(dest, &blob)
    }

    /// 明文读写：仅供链校验与测试使用（仅 legacy 路径支持）。
    pub fn to_plaintext_json(&self) -> Result<Vec<u8>, &'static str> {
        match self {
            AuditLog::Legacy(l) => l.to_plaintext_json(),
            AuditLog::Segmented(_) => Err("plaintext json only for legacy audit log"),
        }
    }

    /// 读取导出件（校验/导入用）：解密后返回条目，并顺带做一次链校验。
    pub fn read_export(
        blob_path: &Path,
        export_key: &[u8; KEY_LEN],
    ) -> Result<(Vec<AuditEntry>, AuditVerify), &'static str> {
        LegacyLog::read_export(blob_path, export_key)
    }

    /// 从明文 JSON 还原（legacy 形态；无落盘路径）。
    pub fn from_plaintext_json(bytes: &[u8]) -> Result<Self, &'static str> {
        Ok(AuditLog::Legacy(Box::new(LegacyLog::from_plaintext_json(
            bytes,
        )?)))
    }

    /// 密钥轮换时的日志重加密。v3 分段路径：值 JSON 逐字节原样重写入新密钥段，
    /// 链与条目字节不变。
    pub fn rekey(
        data_dir: &Path,
        old_mk: &[u8; KEY_LEN],
        new_mk: &[u8; KEY_LEN],
    ) -> Result<usize, &'static str> {
        if data_dir.join(AUDIT_FILE).exists() {
            return LegacyLog::rekey(data_dir, old_mk, new_mk);
        }
        let old = SegmentedAuditLog::open(data_dir, &hkdf_sha256_derive(old_mk, AUDIT_KEY_INFO))?;
        let entries = old.entries().to_vec();
        segmented::rebuild(
            data_dir,
            &hkdf_sha256_derive(new_mk, AUDIT_KEY_INFO),
            entries,
        )
    }
}

impl LegacyLog {
    /// 用调用方提供的密钥打开（测试 / 密钥轮换重加密场景）。
    pub fn open_with_key(data_dir: &Path, key: &[u8; KEY_LEN]) -> Result<Self, &'static str> {
        std::fs::create_dir_all(data_dir).map_err(|_| "cannot create audit dir")?;
        let path = data_dir.join(AUDIT_FILE);
        let d = match std::fs::read(&path) {
            Ok(blob) => {
                let pt = decrypt_blob(key, &blob, "audit log truncated")?;
                serde_json::from_slice(&pt).map_err(|_| "audit log parse failed")?
            }
            Err(_) => LogData {
                version: AUDIT_FORMAT_VER,
                entries: Vec::new(),
            },
        };
        Ok(Self { d, path, key: *key })
    }

    /// 追加一条并立即落盘（审计不得滞后于操作）。
    pub fn append(&mut self, kind: &str, detail: &str) -> Result<AuditEntry, &'static str> {
        let entry = self.push(kind, detail)?;
        self.save()?;
        Ok(entry)
    }

    /// 仅在内存中追加（`self_check` 用，不落盘）。
    fn push(&mut self, kind: &str, detail: &str) -> Result<AuditEntry, &'static str> {
        if kind.is_empty() {
            return Err("empty kind");
        }
        let seq = self.d.entries.last().map(|e| e.seq + 1).unwrap_or(1);
        let ts_ms = now_ms();
        let prev_hash = self.head_hash();
        let hash = compute_hash(&prev_hash, seq, ts_ms, kind, detail);
        let entry = AuditEntry {
            seq,
            ts_ms,
            kind: kind.to_string(),
            detail: detail.to_string(),
            prev_hash,
            hash,
        };
        self.d.entries.push(entry.clone());
        Ok(entry)
    }

    pub fn entries(&self) -> &[AuditEntry] {
        &self.d.entries
    }

    pub fn len(&self) -> usize {
        self.d.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.d.entries.is_empty()
    }

    /// 链校验：报出哈希不匹配 / 链断（prev_hash ≠ 上一条 hash）/ seq 不连续三种问题。
    pub fn verify(&self) -> AuditVerify {
        verify_entries(&self.d.entries)
    }

    /// 链头哈希（空日志返回创世零哈希），供跨设备交叉核对（docs/05-06 §3.1）。
    pub fn head_hash(&self) -> String {
        self.d
            .entries
            .last()
            .map(|e| e.hash.clone())
            .unwrap_or_else(genesis_prev_hash)
    }

    /// 导出为加密文件：信封明文用调用方给的专用导出密钥加密（docs/05-06 §3.3）。
    pub fn export_encrypted(
        &self,
        dest: &Path,
        export_key: &[u8; KEY_LEN],
    ) -> Result<(), &'static str> {
        let env = ExportEnvelope {
            version: AUDIT_FORMAT_VER,
            exported_ms: now_ms(),
            entries: self.d.entries.clone(),
        };
        let json = serde_json::to_vec(&env).map_err(|_| "audit export serialize failed")?;
        let blob = encrypt_blob(export_key, &json)?;
        write_atomic(dest, &blob)
    }

    /// 读取导出件（校验/导入用）：解密后返回条目，并顺带做一次链校验。
    pub fn read_export(
        blob_path: &Path,
        export_key: &[u8; KEY_LEN],
    ) -> Result<(Vec<AuditEntry>, AuditVerify), &'static str> {
        let blob = std::fs::read(blob_path).map_err(|_| "cannot read audit export")?;
        let pt = decrypt_blob(export_key, &blob, "audit export truncated")?;
        let env: ExportEnvelope =
            serde_json::from_slice(&pt).map_err(|_| "audit export parse failed")?;
        let log = Self {
            d: LogData {
                version: env.version,
                entries: env.entries,
            },
            path: blob_path.to_path_buf(),
            key: *export_key,
        };
        let verdict = log.verify();
        Ok((log.d.entries, verdict))
    }

    /// 明文读写：仅供链校验与测试使用，**不经 AEAD，禁止直接落盘**。
    pub fn to_plaintext_json(&self) -> Result<Vec<u8>, &'static str> {
        serde_json::to_vec(&self.d).map_err(|_| "audit serialize failed")
    }

    /// 从明文 JSON 还原（无落盘路径：`append` 会因无法落盘而失败）。
    pub fn from_plaintext_json(bytes: &[u8]) -> Result<Self, &'static str> {
        let d: LogData = serde_json::from_slice(bytes).map_err(|_| "audit log parse failed")?;
        Ok(Self {
            d,
            path: PathBuf::new(),
            key: [0u8; KEY_LEN],
        })
    }

    /// 密钥轮换时的日志重加密：用 `old_mk` 读出条目、以 `new_mk` 重新加密落盘。
    ///
    /// 链哈希不依赖密钥（只覆盖条目内容），因此重加密后**链校验依然成立**，
    /// 历史不会被切断——这正是 MK 全库轮换（docs/07 §三）里审计日志该有的行为：
    /// 换密钥而不是丢历史。返回迁移的条目数。
    pub fn rekey(
        data_dir: &Path,
        old_mk: &[u8; KEY_LEN],
        new_mk: &[u8; KEY_LEN],
    ) -> Result<usize, &'static str> {
        // 旧文件不存在（如从未写过审计）→ 无需迁移
        let path = data_dir.join(AUDIT_FILE);
        if !path.exists() {
            return Ok(0);
        }
        let old = Self::open_with_key(data_dir, &hkdf_sha256_derive(old_mk, AUDIT_KEY_INFO))?;
        // 直接以新密钥构造落盘对象：**不要**用新密钥去 open（文件还在旧密钥下，必然解密失败）
        let fresh = Self {
            d: old.d,
            path,
            key: *hkdf_sha256_derive(new_mk, AUDIT_KEY_INFO),
        };
        let n = fresh.d.entries.len();
        fresh.save()?;
        Ok(n)
    }

    fn save(&self) -> Result<(), &'static str> {
        let json = self.to_plaintext_json()?;
        let blob = encrypt_blob(&self.key, &json)?;
        write_atomic(&self.path, &blob)
    }
}

/// 引擎自检：链式哈希 + 校验 + 篡改检出可用（vault-core / vault-vault / vault-p2p 依赖本项）。
pub fn self_check() -> bool {
    if !vault_crypto::self_check() {
        return false;
    }
    let mut log = LegacyLog {
        d: LogData {
            version: AUDIT_FORMAT_VER,
            entries: Vec::new(),
        },
        path: PathBuf::new(),
        key: [0u8; KEY_LEN],
    };
    let mut ok = log.push("session", "self-check").is_ok();
    ok = ok && log.push("vault", "self-check").is_ok();
    ok = ok && log.verify().ok && log.head_hash() == log.d.entries[1].hash;
    // 篡改详情后必须能被检出
    if let Some(e) = log.d.entries.get_mut(1) {
        e.detail = "tampered".to_string();
    }
    ok && !log.verify().ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// 临时目录（env::temp_dir + 进程内唯一名），Drop 时清理；不引入 dev-dependency。
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let p = std::env::temp_dir().join(format!(
                "vault-audit-test-{tag}-{}-{nanos}-{n}",
                std::process::id()
            ));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// 三条记录的日志（真实落盘于临时目录；链语义测试走 legacy 形态）。
    fn three_entries() -> LegacyLog {
        let dir = TempDir::new("fixture");
        let mut log = LegacyLog::open_with_key(dir.path(), &vault_crypto::random_key()).unwrap();
        log.append("session", "unlock").unwrap();
        log.append("vault", "import:1").unwrap();
        log.append("security", "alert:brute-force").unwrap();
        log
    }

    /// 明文往返：改字段后重新解析（保持 JSON 合法）。
    fn reparse(log: &LegacyLog, mutate: impl FnOnce(&mut LogData)) -> LegacyLog {
        let mut d: LogData = serde_json::from_slice(&log.to_plaintext_json().unwrap()).unwrap();
        mutate(&mut d);
        LegacyLog::from_plaintext_json(&serde_json::to_vec(&d).unwrap()).unwrap()
    }

    #[test]
    fn rekey_preserves_chain_and_switches_key() {
        let dir = std::env::temp_dir().join(format!("vsync_audit_rekey_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let old_mk = [3u8; KEY_LEN];
        let new_mk = [4u8; KEY_LEN];

        let mut log = AuditLog::open(&dir, &old_mk).expect("open");
        log.append("session", "unlock").expect("a1");
        log.append("vault", "import").expect("a2");
        let head_before = log.head_hash();
        drop(log);

        let n = AuditLog::rekey(&dir, &old_mk, &new_mk).expect("rekey");
        assert_eq!(n, 2);

        // 新密钥可读、链仍连续、链头不变
        let after = AuditLog::open(&dir, &new_mk).expect("open new");
        assert_eq!(after.len(), 2);
        assert!(after.verify().ok);
        assert_eq!(after.head_hash(), head_before);
        // 旧密钥不再可读
        assert!(AuditLog::open(&dir, &old_mk).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_three_then_verify_ok() {
        let log = three_entries();
        assert_eq!(log.len(), 3);
        assert!(!log.is_empty());
        assert_eq!(log.entries()[0].seq, 1);
        assert_eq!(log.entries()[2].kind, "security");
        let last = log.entries()[2].clone();
        assert_eq!(log.head_hash(), last.hash);
        assert_ne!(log.entries()[0].hash, log.entries()[1].hash);

        let v = log.verify();
        assert!(v.ok);
        assert_eq!(v.checked, 3);
        assert_eq!(v.broken_at, None);
        assert_eq!(v.reason, None);
    }

    #[test]
    fn tampered_detail_is_detected() {
        let log = three_entries();
        let tampered = reparse(&log, |d| d.entries[1].detail = "import:999".to_string());
        let v = tampered.verify();
        assert!(!v.ok);
        assert_eq!(v.broken_at, Some(2));
        assert_eq!(v.checked, 1);
        assert!(v.reason.unwrap().contains("哈希不匹配"));
        assert_eq!(tampered.len(), 3);
    }

    #[test]
    fn broken_prev_hash_is_detected() {
        let log = three_entries();
        let tampered = reparse(&log, |d| d.entries[1].prev_hash = "0".repeat(64));
        let v = tampered.verify();
        assert!(!v.ok);
        assert_eq!(v.broken_at, Some(2));
        assert!(v.reason.unwrap().contains("链断"));
    }

    #[test]
    fn seq_gap_is_detected() {
        let log = three_entries();
        let tampered = reparse(&log, |d| d.entries[2].seq = 9);
        let v = tampered.verify();
        assert!(!v.ok);
        assert_eq!(v.broken_at, Some(9));
        assert!(v.reason.unwrap().contains("seq 不连续"));
    }

    #[test]
    fn reopen_persists_entries_and_head() {
        let dir = TempDir::new("persist");
        let mk = vault_crypto::random_key();
        let (head, len) = {
            let mut log =
                LegacyLog::open_with_key(dir.path(), &hkdf_sha256_derive(&mk, AUDIT_KEY_INFO))
                    .unwrap();
            log.append("session", "unlock").unwrap();
            log.append("device", "pair:vd-a").unwrap();
            (log.head_hash(), log.len())
        };
        assert!(dir.path().join("audit.enc").exists());

        let log2 =
            LegacyLog::open_with_key(dir.path(), &hkdf_sha256_derive(&mk, AUDIT_KEY_INFO)).unwrap();
        assert_eq!(log2.len(), len);
        assert_eq!(log2.head_hash(), head);
        assert_eq!(log2.entries()[0].detail, "unlock");
        assert_eq!(log2.entries()[1].kind, "device");
        assert!(log2.verify().ok);
    }

    #[test]
    fn wrong_key_cannot_open() {
        let dir = TempDir::new("wrongkey");
        let mk = vault_crypto::random_key();
        {
            let mut log = AuditLog::open(dir.path(), &mk).unwrap();
            log.append("vault", "import:1").unwrap();
        }
        assert!(AuditLog::open(dir.path(), &vault_crypto::random_key()).is_err());
        assert!(AuditLog::open_with_key(dir.path(), &vault_crypto::random_key()).is_err());
        // 正确密钥仍可打开
        let log =
            AuditLog::open_with_key(dir.path(), &hkdf_sha256_derive(&mk, AUDIT_KEY_INFO)).unwrap();
        assert_eq!(log.len(), 1);
        assert!(log.verify().ok);
    }

    #[test]
    fn export_roundtrip_and_wrong_key() {
        let dir = TempDir::new("export");
        let mk = vault_crypto::random_key();
        let export_key = *hkdf_sha256_derive(&mk, b"audit-export");
        let dest = dir.path().join("audit-export.enc");

        let mut log = AuditLog::open(dir.path(), &mk).unwrap();
        log.append("vault", "export").unwrap();
        log.append("session", "lock").unwrap();
        log.export_encrypted(&dest, &export_key).unwrap();

        let (entries, v) = AuditLog::read_export(&dest, &export_key).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(v.ok);
        assert_eq!(v.checked, 2);
        assert_eq!(entries[1].detail, "lock");

        // 信封字段齐备，且条目内容确实只在密文内
        let raw = std::fs::read(&dest).unwrap();
        let pt = aead_decrypt(&export_key, &raw).unwrap();
        let text = String::from_utf8(pt).unwrap();
        assert!(text.contains("\"version\":1"));
        assert!(text.contains("\"exportedMs\""));
        assert!(text.contains("\"entries\""));
        assert!(!raw.windows(4).any(|w| w == b"lock"));

        // 错误密钥 / 截断文件均拒绝
        assert!(AuditLog::read_export(&dest, &vault_crypto::random_key()).is_err());
        let trunc = dir.path().join("trunc.enc");
        std::fs::write(&trunc, &raw[..8]).unwrap();
        assert!(AuditLog::read_export(&trunc, &export_key).is_err());
    }

    #[test]
    fn empty_log_verifies_with_genesis_head() {
        let dir = TempDir::new("empty");
        let mk = vault_crypto::random_key();
        let log = AuditLog::open(dir.path(), &mk).unwrap();
        assert_eq!(log.len(), 0);
        assert!(log.is_empty());
        assert!(log.entries().is_empty());
        assert_eq!(log.head_hash(), "0".repeat(64));
        let v = log.verify();
        assert!(v.ok);
        assert_eq!(v.checked, 0);
        assert_eq!(v.broken_at, None);
        // 空日志尚无落盘文件；首条追加即创建
        assert!(!dir.path().join("audit.enc").exists());
        assert!(log.export_encrypted(&dir.path().join("e.enc"), &mk).is_ok());
    }

    #[test]
    fn empty_kind_is_rejected() {
        let dir = TempDir::new("kind");
        let mut log = AuditLog::open(dir.path(), &vault_crypto::random_key()).unwrap();
        assert_eq!(log.append("", "x").unwrap_err(), "empty kind");
        assert_eq!(log.len(), 0);
        // 空 detail 允许（如仅记录类别）
        assert!(log.append("vault", "").is_ok());
        assert_eq!(log.entries()[0].detail, "");
        assert!(log.verify().ok);
    }

    /// 固定输入 → 固定摘要：逐字节锁定拼接顺序（跨设备交叉核对依赖格式可复现）。
    #[test]
    fn hash_byte_layout_is_frozen() {
        let prev = "0".repeat(64);
        let (seq, ts_ms, kind, detail) = (1u64, 1_700_000_000_000u64, "vault", "import:1");
        let h = compute_hash(&prev, seq, ts_ms, kind, detail);
        assert_eq!(
            h,
            "941be86ff15214a80a5ddb2de314e0b08ac909cf2b37c16c4a4aa27bf6293b79"
        );

        // 独立复算：按 docs/05-06 §3.1 的字节序手工拼接，不经过 compute_hash
        let mut buf = Vec::new();
        buf.extend_from_slice(prev.as_bytes());
        buf.extend_from_slice(&seq.to_be_bytes());
        buf.extend_from_slice(&ts_ms.to_be_bytes());
        buf.extend_from_slice(&(kind.len() as u32).to_be_bytes());
        buf.extend_from_slice(kind.as_bytes());
        buf.extend_from_slice(&(detail.len() as u32).to_be_bytes());
        buf.extend_from_slice(detail.as_bytes());
        assert_eq!(vault_crypto::hash_sha256(&buf), h);
    }

    #[test]
    fn plaintext_json_is_faithful() {
        let log = three_entries();
        let clone = LegacyLog::from_plaintext_json(&log.to_plaintext_json().unwrap()).unwrap();
        assert_eq!(clone.entries(), log.entries());
        assert_eq!(clone.head_hash(), log.head_hash());
        assert!(clone.verify().ok);
        assert!(LegacyLog::from_plaintext_json(b"not json").is_err());
    }

    #[test]
    fn self_check_detects_tampering() {
        assert!(self_check());
    }

    // ==== P6-4 证据测试（VSAU v1 分段化；docs/v2.0/09 §三 P6-4）====

    /// P6-4 证据：audit_segments::chain_links_across_segments
    /// 多段落盘（每条 append 一个段 + 段描述）后重开：全链连续、段描述
    /// 覆盖各段、首尾哈希正确。
    #[test]
    fn audit_segments_chain_links_across_segments() {
        let dir = TempDir::new("v3seg");
        let mk = vault_crypto::random_key();
        let mut log = AuditLog::open(dir.path(), &mk).unwrap();
        assert!(matches!(log, AuditLog::Segmented(_)));
        let mut heads = Vec::new();
        for i in 0..12u64 {
            log.append("vault", &format!("import:{i}")).unwrap();
            heads.push(log.head_hash());
        }
        drop(log);

        let log2 = AuditLog::open(dir.path(), &mk).unwrap();
        assert_eq!(log2.len(), 12);
        let v = log2.verify();
        assert!(v.ok, "chain must link across segments: {:?}", v.reason);
        assert_eq!(v.checked, 12);
        // 段间链头指针：最后一条的哈希 = 全链头
        assert_eq!(log2.head_hash(), heads[11]);
        if let AuditLog::Segmented(seg) = &log2 {
            let descs = seg.segment_descs();
            assert!(!descs.is_empty());
            // 描述里的 seq 区间恰好衔接成 1..=12
            let mut expected = 1u64;
            for (_sid, first, last) in &descs {
                assert_eq!(*first, expected);
                expected = last + 1;
            }
            assert_eq!(expected, 13);
        }
    }

    /// P6-4 证据：audit_segments::compaction_preserves_head_hash
    /// 压实四条硬不变量：头哈希、条目哈希逐条、seq 连续、链连续全不变。
    #[test]
    fn audit_segments_compaction_preserves_head_hash() {
        let dir = TempDir::new("v3compact");
        let mk = vault_crypto::random_key();
        let mut log = AuditLog::open(dir.path(), &mk).unwrap();
        for i in 0..20u64 {
            log.append("session", &format!("op:{i}")).unwrap();
        }
        let head_before = log.head_hash();
        let entries_before = log.entries().to_vec();
        drop(log);

        let mut log = AuditLog::open(dir.path(), &mk).unwrap();
        if let AuditLog::Segmented(seg) = &mut log {
            seg.compact().expect("compact");
            // 压实后立即校验
            assert!(seg.verify().ok);
        } else {
            panic!("expected segmented path");
        }
        assert_eq!(log.head_hash(), head_before, "head hash invariant");
        assert_eq!(log.entries(), &entries_before[..], "entry hashes invariant");
        assert!(log.verify().ok);
        drop(log);

        // 重开后依然一致
        let mut log2 = AuditLog::open(dir.path(), &mk).unwrap();
        assert_eq!(log2.len(), 20);
        assert_eq!(log2.head_hash(), head_before);
        assert!(log2.verify().ok);
        // 压实后继续追加：链从原头继续
        log2.append("vault", "after-compact").unwrap();
        assert!(log2.verify().ok);
        assert_eq!(log2.len(), 21);
    }

    /// P6-4 证据：audit_segments::append_is_o1
    /// 单条 append 代价与日志长度无关：在 400 条基础上再追加 200 条，
    /// 单条耗时相对前 50 条的增长 < 4×（O(n) 会放大 ~8×，此处远低于）。
    #[test]
    fn audit_segments_append_is_o1() {
        let dir = TempDir::new("v3o1");
        let mk = vault_crypto::random_key();
        let mut log = AuditLog::open(dir.path(), &mk).unwrap();
        for i in 0..50u64 {
            log.append("vault", &format!("warm:{i}")).unwrap();
        }
        let t0 = std::time::Instant::now();
        for i in 0..50u64 {
            log.append("vault", &format!("early:{i}")).unwrap();
        }
        let early_avg = t0.elapsed() / 50;
        for i in 0..350u64 {
            log.append("vault", &format!("bulk:{i}")).unwrap();
        }
        let t1 = std::time::Instant::now();
        for i in 0..50u64 {
            log.append("vault", &format!("late:{i}")).unwrap();
        }
        let late_avg = t1.elapsed() / 50;
        assert!(
            late_avg.as_nanos() < early_avg.as_nanos() * 4,
            "append cost grew with log size: early={early_avg:?} late={late_avg:?}"
        );
        assert!(log.verify().ok);
    }
}
