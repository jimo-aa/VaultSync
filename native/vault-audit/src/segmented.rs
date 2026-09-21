//! VSAU v1 分段审计日志（P6-4，docs/v2.0/02 §四 / 09 §三 P6-4）。
//!
//! 载体：`vault-store` 的 Audit 段（`data_dir/audit/<id>.vssg`），条目值为
//! 与 V1.0 **逐字节同构**的 `AuditEntry` JSON（改载体不改递推，黄金向量
//! 测试 `hash_byte_layout_is_frozen` 保持通过）。段描述记录以 KV key=0
//! 承载段间链头指针（segmentId / firstSeq / lastSeq / headHash）。
//! 单条 append = 一次段提交，代价与全日志长度无关（O(1)）。
//!
//! 压实四条硬不变量：段首 prevHeadHash、段尾 headHash、baseSeq、所有条目
//! 哈希逐条不变——条目哈希只由内容决定（密钥无关），压实重写不改任何条目
//! 字节，四条不变量天然成立（测试锁定）。
use crate::{compute_hash, genesis_prev_hash, now_ms, verify_entries, AuditEntry, AuditVerify};
use vault_crypto::KEY_LEN;
use vault_store::{Journal, Namespace, OpenMode, VaultStore};

/// 段描述记录的 KV key（设计约定：key = 0 唯一标识）。
const DESC_KEY: u64 = 0;
/// 段描述条数上限：超过即按现存段重建（压实退休的旧段描述清除）。
const DESC_MAX: usize = 1024;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct SegmentDesc {
    segment_id: u64,
    first_seq: u64,
    last_seq: u64,
    head_hash: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct DescRecord {
    schema: u16,
    descs: Vec<SegmentDesc>,
}

pub struct SegmentedAuditLog {
    store: VaultStore,
    entries: Vec<AuditEntry>,
    descs: Vec<SegmentDesc>,
}

fn store_err(e: vault_store::StoreError) -> &'static str {
    match e {
        vault_store::StoreError::ReadOnly => "vault opened read-only",
        vault_store::StoreError::NeedsRecovery(_) => "recovery required",
        _ => "audit store io failed",
    }
}

impl SegmentedAuditLog {
    pub fn open(data_dir: &std::path::Path, key: &[u8; KEY_LEN]) -> Result<Self, &'static str> {
        let store = VaultStore::open(data_dir, Namespace::Audit, key, OpenMode::ReadWrite)
            .map_err(store_err)?;
        let mut entries = Vec::new();
        for v in store.log_entries() {
            let e: AuditEntry =
                serde_json::from_slice(&v).map_err(|_| "audit entry parse failed")?;
            entries.push(e);
        }
        let descs = store
            .kv_get(DESC_KEY)
            .and_then(|v| serde_json::from_slice::<DescRecord>(v).ok())
            .map(|r| r.descs)
            .unwrap_or_default();
        Ok(Self {
            store,
            entries,
            descs,
        })
    }

    /// 段描述重建：仅保留现存段的描述（压实退休后调用）。
    fn refresh_descs(&mut self) {
        let live: std::collections::HashSet<u64> = self.store.segment_ids().into_iter().collect();
        self.descs.retain(|d| live.contains(&d.segment_id));
        self.write_desc();
    }

    fn write_desc(&mut self) {
        if let Ok(json) = serde_json::to_vec(&DescRecord {
            schema: 1,
            descs: self.descs.clone(),
        }) {
            let _ = self.store.kv_apply(vec![(true, DESC_KEY, json)]);
        }
    }

    pub fn append(&mut self, kind: &str, detail: &str) -> Result<AuditEntry, &'static str> {
        if kind.is_empty() {
            return Err("empty kind");
        }
        let seq = self.entries.last().map(|e| e.seq + 1).unwrap_or(1);
        let ts_ms = now_ms();
        let prev_hash = self.head_hash();
        let hash = compute_hash(&prev_hash, seq, ts_ms, kind, detail);
        let entry = AuditEntry {
            seq,
            ts_ms,
            kind: kind.to_string(),
            detail: detail.to_string(),
            prev_hash,
            hash: hash.clone(),
        };
        let json = serde_json::to_vec(&entry).map_err(|_| "audit serialize failed")?;

        // 段间链头指针：pending 段预登记（next_segment_id 即本次 flush 将用的 id）
        let starts_new_segment = self.store.pending_is_empty();
        if starts_new_segment {
            self.descs.push(SegmentDesc {
                segment_id: self.store.next_segment_id(),
                first_seq: seq,
                last_seq: seq,
                head_hash: hash,
            });
        } else if let Some(d) = self.descs.last_mut() {
            d.last_seq = seq;
            d.head_hash = hash;
        }
        self.write_desc();

        self.store.log_append(&json).map_err(store_err)?;
        self.entries.push(entry.clone());
        self.store.flush().map_err(store_err)?;
        if self.descs.len() > DESC_MAX {
            self.refresh_descs();
            self.store.flush().map_err(store_err).map(|_| ())?;
        }
        Ok(entry)
    }

    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn head_hash(&self) -> String {
        self.entries
            .last()
            .map(|e| e.hash.clone())
            .unwrap_or_else(genesis_prev_hash)
    }

    pub fn verify(&self) -> AuditVerify {
        verify_entries(&self.entries)
    }

    /// 压实（四条硬不变量：条目哈希只由内容决定，重写不改任何条目字节）。
    pub fn compact(&mut self) -> Result<(), &'static str> {
        self.store.compact().map_err(store_err)?;
        self.refresh_descs();
        self.store.flush().map_err(store_err).map(|_| ())
    }

    /// 段间链描述（诊断 / 跨段校验用）。
    pub fn segment_descs(&self) -> Vec<(u64, u64, u64)> {
        self.descs
            .iter()
            .map(|d| (d.segment_id, d.first_seq, d.last_seq))
            .collect()
    }
}

/// 从条目序列重建分段日志（MK 轮换重加密用）：值 JSON 逐字节原样写入
/// （链哈希只由内容决定，不重算、不改任何条目字节）。
pub fn rebuild(
    data_dir: &std::path::Path,
    key: &[u8; KEY_LEN],
    entries: Vec<AuditEntry>,
) -> Result<usize, &'static str> {
    let ns_dir = VaultStore::ns_dir(data_dir, Namespace::Audit);
    let _ = std::fs::remove_dir_all(&ns_dir);
    let _ = std::fs::remove_file(Journal::path(data_dir, Namespace::Audit));
    let mut log = SegmentedAuditLog::open(data_dir, key)?;
    let n = entries.len();
    for e in &entries {
        let json = serde_json::to_vec(e).map_err(|_| "audit serialize failed")?;
        log.store.log_append(&json).map_err(store_err)?;
    }
    log.store.flush().map_err(store_err).map(|_| ())?;
    Ok(n)
}
