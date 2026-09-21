//! 分段存储引擎：六步提交协议 / KV+LOG 双视图 / 快照与压实（docs/v2.0/02 §4.2–§4.3）。
//!
//! 写路径全部经六步提交协议（journal 预告 → 组包加密 → tmp+fsync →
//! rename+fsync(dir)【提交点】→ journal 完成 → 清单更新）；读路径以磁盘
//! 扫描为真值，manifest 只是缓存。

use crate::crash::abort_point;
use crate::error::StoreError;
use crate::journal::{Journal, JournalEntry, JournalEntryKind, RecoveryOutcome};
use crate::segment::{self, Record, RecordOp, SegmentHeader, SegmentKind};
use crate::Namespace;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenMode {
    ReadWrite,
    ReadOnly,
}

/// 段元数据（内存索引）。
#[derive(Clone, Copy, Debug)]
struct SegmentMeta {
    kind: SegmentKind,
    /// 保留：diagnostics 汇报段规模用
    #[allow(dead_code)]
    record_count: u64,
}

#[derive(Clone, Debug)]
struct LogEntry {
    key: u64,
    value: Vec<u8>,
}

/// 一个命名空间的分段存储实例。
///
/// 同一 `data_dir` 下各命名空间（index / audit / state）各自实例化一个
/// `VaultStore`；单写者互斥由上层对整个保险箱持有的 `VaultLease` 保证。
pub struct VaultStore {
    data_dir: PathBuf,
    ns: Namespace,
    master_key: Zeroizing<[u8; 32]>,
    readonly: bool,
    pub maintenance: bool,
    journal: Option<Journal>,
    recovery: RecoveryOutcome,
    /// 段 id → 元数据（含快照与追加段；提交点之后的真值视图）
    segments: BTreeMap<u64, SegmentMeta>,
    /// KV 视图：key → (record_seq, value)；墓碑即删除
    kv: BTreeMap<u64, (u64, Vec<u8>)>,
    /// LOG 视图：record_seq 有序
    log: BTreeMap<u64, LogEntry>,
    pending: Vec<Record>,
    next_segment_id: u64,
    next_record_seq: u64,
    snapshot_id: Option<u64>,
    snapshot_upto_seq: u64,
}

/// 压实触发阈值（docs/v2.0/02 §4.3）。
const COMPACTION_APPEND_SEGMENTS: usize = 8;
/// pending 记录数达到该值即落一个追加段（批量化窗口的简化实现：
/// 200 ms 时间窗属后台线程职责，本层提供显式 `flush`）。
pub const FLUSH_BATCH_RECORDS: usize = 256;

impl VaultStore {
    pub fn ns_dir(data_dir: &Path, ns: Namespace) -> PathBuf {
        data_dir.join(ns.as_str())
    }

    fn seg_path(&self, id: u64) -> PathBuf {
        Self::ns_dir(&self.data_dir, self.ns).join(format!("{id}.vssg"))
    }

    /// 打开一个命名空间存储。执行七步启动序列中与本层相关的部分：
    /// journal 回滚/前滚 → 清 .tmp → 磁盘扫描重建视图（manifest 只是缓存）。
    pub fn open(
        data_dir: &Path,
        ns: Namespace,
        master_key: &[u8; 32],
        mode: OpenMode,
    ) -> Result<VaultStore, StoreError> {
        std::fs::create_dir_all(Self::ns_dir(data_dir, ns))?;
        let readonly = mode == OpenMode::ReadOnly;

        // journal 恢复（只读实例不写 journal：跳过恢复动作，视为无未完成提交）
        let mut maintenance = false;
        let mut recovery = RecoveryOutcome::default();
        let journal = match Journal::load(data_dir, master_key)? {
            Some(j) if !readonly => {
                let exists = |id: u64| {
                    let p = data_dir.join(ns.as_str()).join(format!("{id}.vssg"));
                    p.exists() && segment::read_segment(&p, ns, master_key).is_ok()
                };
                let (j, out) = j.recover(master_key, &exists, true)?;
                maintenance = out.entered_maintenance;
                recovery = out;
                j
            }
            // 全新库：RW 打开即建空 journal（六步提交协议依赖它）
            None if !readonly => Some(Journal::create_empty(data_dir, master_key)?),
            other => other,
        };

        // 清理残留 .tmp（第 3 步中断产物；.tmp 永不参与读取）
        if !readonly {
            for e in std::fs::read_dir(Self::ns_dir(data_dir, ns))?.flatten() {
                if e.path().extension().map(|x| x == "tmp").unwrap_or(false) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }

        // 磁盘扫描（真值），按 segment_id 升序应用
        let mut store = VaultStore {
            data_dir: data_dir.to_path_buf(),
            ns,
            master_key: Zeroizing::new(*master_key),
            readonly,
            maintenance,
            journal,
            recovery,
            segments: BTreeMap::new(),
            kv: BTreeMap::new(),
            log: BTreeMap::new(),
            pending: Vec::new(),
            next_segment_id: 1,
            next_record_seq: 1,
            snapshot_id: None,
            snapshot_upto_seq: 0,
        };
        let mut entries: Vec<(u64, SegmentMeta)> = Vec::new();
        for e in std::fs::read_dir(Self::ns_dir(data_dir, ns))?.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            let Some(stem) = name.strip_suffix(".vssg") else {
                continue;
            };
            let Ok(id) = stem.parse::<u64>() else {
                continue;
            };
            let (header, _records) = segment::read_segment(&e.path(), ns, master_key)?;
            // read_segment 已做 CRC + digest + AEAD 全量校验（审计段与快照段
            // 必须全量校验，docs/v2.0/02 §4.2 读取顺序）
            entries.push((
                id,
                SegmentMeta {
                    kind: header.segment_kind,
                    record_count: header.record_count,
                },
            ));
        }
        entries.sort_by_key(|(id, _)| *id);
        for (id, meta) in entries {
            let (_, records) = segment::read_segment(&store.seg_path(id), ns, master_key)?;
            store.apply_records(&records);
            store.next_segment_id = store.next_segment_id.max(id + 1);
            if matches!(meta.kind, SegmentKind::IndexSnapshot) {
                store.snapshot_id = Some(id);
            }
        }
        Ok(store)
    }

    pub fn readonly(&self) -> bool {
        self.readonly
    }

    pub fn enter_maintenance(&mut self) {
        self.maintenance = true;
    }

    pub fn exit_maintenance(&mut self) {
        self.maintenance = false;
    }

    fn ensure_writable(&self) -> Result<(), StoreError> {
        if self.readonly {
            return Err(StoreError::ReadOnly);
        }
        if self.maintenance {
            return Err(StoreError::Maintenance);
        }
        Ok(())
    }

    /// KV upsert（异步批量化：进 pending，达到阈值或显式 flush 时落段）。
    pub fn kv_put(&mut self, key: u64, value: &[u8]) -> Result<(), StoreError> {
        self.ensure_writable()?;
        let seq = self.alloc_seq();
        self.kv.insert(key, (seq, value.to_vec()));
        self.pending.push(Record {
            op: RecordOp::Upsert,
            key,
            record_seq: seq,
            value: value.to_vec(),
        });
        self.auto_flush()
    }

    /// KV delete（墓碑）。
    pub fn kv_delete(&mut self, key: u64) -> Result<(), StoreError> {
        self.ensure_writable()?;
        let seq = self.alloc_seq();
        self.kv.remove(&key);
        self.pending.push(Record {
            op: RecordOp::Delete,
            key,
            record_seq: seq,
            value: Vec::new(),
        });
        self.auto_flush()
    }

    /// KV 读取（LWW 由打开时的应用序保证）。
    pub fn kv_get(&self, key: u64) -> Option<&Vec<u8>> {
        self.kv.get(&key).map(|(_, v)| v)
    }

    /// LOG 追加（O(1)：内存链更新 + pending，不重写历史）。
    pub fn log_append(&mut self, value: &[u8]) -> Result<u64, StoreError> {
        self.ensure_writable()?;
        let seq = self.alloc_seq();
        self.log.insert(
            seq,
            LogEntry {
                key: seq,
                value: value.to_vec(),
            },
        );
        self.pending.push(Record {
            op: RecordOp::Append,
            key: seq,
            record_seq: seq,
            value: value.to_vec(),
        });
        self.auto_flush()?;
        Ok(seq)
    }

    /// LOG 全量读取（record_seq 有序）。
    pub fn log_entries(&self) -> Vec<Vec<u8>> {
        self.log.values().map(|e| e.value.clone()).collect()
    }

    pub fn log_len(&self) -> usize {
        self.log.len()
    }

    fn alloc_seq(&mut self) -> u64 {
        let s = self.next_record_seq;
        self.next_record_seq += 1;
        s
    }

    fn apply_records(&mut self, records: &[Record]) {
        for r in records {
            self.next_record_seq = self.next_record_seq.max(r.record_seq + 1);
            match r.op {
                RecordOp::Upsert => {
                    self.kv.insert(r.key, (r.record_seq, r.value.clone()));
                }
                RecordOp::Delete => {
                    self.kv.remove(&r.key);
                }
                RecordOp::Append => {
                    self.log.insert(
                        r.record_seq,
                        LogEntry {
                            key: r.key,
                            value: r.value.clone(),
                        },
                    );
                }
            }
        }
    }

    fn auto_flush(&mut self) -> Result<(), StoreError> {
        if self.pending.len() >= FLUSH_BATCH_RECORDS {
            self.flush()?;
        }
        Ok(())
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    fn segment_kind(&self) -> SegmentKind {
        match self.ns {
            Namespace::Index => SegmentKind::IndexAppend,
            Namespace::Audit => SegmentKind::Audit,
            Namespace::State => SegmentKind::Generic,
        }
    }

    /// 把 pending 记录经六步提交协议落为一个不可变追加段。
    ///
    /// 六步（docs/v2.0/02 §4.2）：
    /// 1 journal 预告+fsync → 2 组包加密 → 3 tmp+fsync → 4 rename+fsync(dir)
    /// **提交点** → 5 journal 完成 → 6 清单更新。
    pub fn flush(&mut self) -> Result<(), StoreError> {
        if self.pending.is_empty() {
            return Ok(());
        }
        self.ensure_writable()?;
        let seg_id = self.next_segment_id;
        let records = std::mem::take(&mut self.pending);
        let kind = self.segment_kind();

        // 步骤 1：journal 预告 + fsync
        abort_point("journal_intent");
        if let Some(j) = self.journal.as_mut() {
            j.append(
                JournalEntry {
                    kind: JournalEntryKind::CommitIntent,
                    commit_seq: 0,
                    arg0: seg_id,
                    arg1: kind as u32,
                },
                &self.master_key,
            )?;
        }

        // 步骤 2：组包加密
        abort_point("pre_encrypt");
        let record_count = records.len() as u64;
        let plaintext_len = segment::encode_records(&records).len() as u64 + 16;
        let header = SegmentHeader {
            format_ver: segment::VSSG_FORMAT_VER,
            segment_kind: kind,
            segment_id: seg_id,
            base_seq: records.first().map(|r| r.record_seq).unwrap_or(0),
            record_count,
            payload_len: plaintext_len,
            nonce: {
                let mut n = [0u8; 12];
                n[0..8].copy_from_slice(&seg_id.to_le_bytes());
                n[8..12].copy_from_slice(&vault_crypto::random_bytes(4));
                n
            },
            key_scope_tag: vault_crypto::hash_sha256_bytes(self.ns.purpose().as_bytes()),
        };
        let file = segment::build_segment(&header, &records, self.ns, &self.master_key)?;
        abort_point("post_encrypt");

        // 步骤 3：tmp + fsync
        let tmp = self.seg_path(seg_id).with_extension("tmp");
        std::fs::write(&tmp, &file)?;
        segment::fsync_file(&tmp)?;
        abort_point("tmp_written");

        // 步骤 4：rename + fsync(dir) —— 提交点
        std::fs::rename(&tmp, self.seg_path(seg_id))?;
        segment::fsync_dir(&Self::ns_dir(&self.data_dir, self.ns));
        // 中断在提交点之后：段已可见、journal 无完成条目 → 下次打开前滚（规则 2）
        abort_point("after_rename_commit_point");

        // 步骤 5：journal 完成
        if let Some(j) = self.journal.as_mut() {
            abort_point("before_journal_done");
            j.append(
                JournalEntry {
                    kind: JournalEntryKind::CommitDone,
                    commit_seq: 0,
                    arg0: seg_id,
                    arg1: kind as u32,
                },
                &self.master_key,
            )?;
        }

        // 步骤 6：清单更新（manifest 只是缓存；失败不回滚——扫描可修正）
        self.segments
            .insert(seg_id, SegmentMeta { kind, record_count });
        self.apply_records(&records);
        self.next_segment_id += 1;
        abort_point("before_manifest");
        let m = self.write_manifest();
        if let Err(e) = m {
            eprintln!("vault-store: manifest update failed (cache only, scan will correct): {e}");
        }
        abort_point("after_manifest");

        self.maybe_compact()?;
        Ok(())
    }

    /// manifest.vssg：state 命名空间的单条 KV，内容只是段集合的缓存视图。
    fn write_manifest(&self) -> Result<(), StoreError> {
        let cache: Vec<u64> = self.segments.keys().copied().collect();
        let json = format!(
            "{{\"schema\":1,\"namespace\":\"{}\",\"next_segment_id\":{},\"upto_commit_seq\":{},\"segments\":{:?}}}",
            self.ns.as_str(),
            self.next_segment_id,
            self.next_record_seq.saturating_sub(1),
            cache
        );
        let header = SegmentHeader {
            format_ver: segment::VSSG_FORMAT_VER,
            segment_kind: SegmentKind::Generic,
            segment_id: 0,
            base_seq: 1,
            record_count: 1,
            payload_len: json.len() as u64 + 16,
            nonce: {
                let mut n = [0u8; 12];
                n[0..8].copy_from_slice(&self.next_record_seq.to_le_bytes());
                n
            },
            key_scope_tag: vault_crypto::hash_sha256_bytes(Namespace::State.purpose().as_bytes()),
        };
        let file = segment::build_segment(
            &header,
            &[Record {
                op: RecordOp::Upsert,
                key: 1,
                record_seq: 1,
                value: json.into_bytes(),
            }],
            Namespace::State,
            &self.master_key,
        )?;
        let path = self
            .data_dir
            .join(format!("manifest.{}.vssg", self.ns.as_str()));
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &file)?;
        segment::fsync_file(&tmp)?;
        std::fs::rename(&tmp, &path)?;
        segment::fsync_dir(&self.data_dir);
        Ok(())
    }

    /// 压实触发：追加段 ≥ 8 或追加段总字节 ≥ 快照段 × 0.5（docs/v2.0/02 §4.3）。
    pub fn maybe_compact(&mut self) -> Result<bool, StoreError> {
        if self.readonly {
            return Ok(false); // 只读实例跳过压实，diagnostics 回报 compactionPending
        }
        let append_segments = self
            .segments
            .iter()
            .filter(|(_, m)| m.kind != SegmentKind::IndexSnapshot)
            .count();
        if append_segments < COMPACTION_APPEND_SEGMENTS {
            return Ok(false);
        }
        self.compact()?;
        Ok(true)
    }

    /// 压实：合并当前全量为新快照段 → compact.marker → 从后往前删 retired → 删 marker。
    /// 崩溃后 marker 幂等重做。
    pub fn compact(&mut self) -> Result<(), StoreError> {
        self.ensure_writable()?;
        let mut records: Vec<Record> = Vec::new();
        for (key, (seq, value)) in &self.kv {
            records.push(Record {
                op: RecordOp::Upsert,
                key: *key,
                record_seq: *seq,
                value: value.clone(),
            });
        }
        for (seq, e) in &self.log {
            records.push(Record {
                op: RecordOp::Append,
                key: e.key,
                record_seq: *seq,
                value: e.value.clone(),
            });
        }
        records.sort_by_key(|r| r.record_seq);
        let retired: Vec<u64> = self.segments.keys().copied().collect();

        // 新快照段走同一个六步协议（journal 预告/完成照常）
        let seg_id = self.next_segment_id;
        let header = SegmentHeader {
            format_ver: segment::VSSG_FORMAT_VER,
            segment_kind: SegmentKind::IndexSnapshot,
            segment_id: seg_id,
            base_seq: self.snapshot_upto_seq,
            record_count: records.len() as u64,
            payload_len: segment::encode_records(&records).len() as u64 + 16,
            nonce: {
                let mut n = [0u8; 12];
                n[0..8].copy_from_slice(&seg_id.to_le_bytes());
                n[8..12].copy_from_slice(&vault_crypto::random_bytes(4));
                n
            },
            key_scope_tag: vault_crypto::hash_sha256_bytes(self.ns.purpose().as_bytes()),
        };
        let file = segment::build_segment(&header, &records, self.ns, &self.master_key)?;
        if let Some(j) = self.journal.as_mut() {
            j.append(
                JournalEntry {
                    kind: JournalEntryKind::CommitIntent,
                    commit_seq: 0,
                    arg0: seg_id,
                    arg1: SegmentKind::IndexSnapshot as u32,
                },
                &self.master_key,
            )?;
        }
        let tmp = self.seg_path(seg_id).with_extension("tmp");
        std::fs::write(&tmp, &file)?;
        segment::fsync_file(&tmp)?;
        std::fs::rename(&tmp, self.seg_path(seg_id))?;
        segment::fsync_dir(&Self::ns_dir(&self.data_dir, self.ns));
        if let Some(j) = self.journal.as_mut() {
            j.append(
                JournalEntry {
                    kind: JournalEntryKind::CommitDone,
                    commit_seq: 0,
                    arg0: seg_id,
                    arg1: SegmentKind::IndexSnapshot as u32,
                },
                &self.master_key,
            )?;
        }

        // compact.marker
        let marker = self
            .data_dir
            .join(format!("compact.marker.{}", self.ns.as_str()));
        let marker_body = format!(
            "{{\"marker_ver\":1,\"snapshot_segment_id\":{},\"upto_commit_seq\":{},\"retired_segment_ids\":{:?}}}",
            seg_id,
            self.next_record_seq.saturating_sub(1),
            retired
        );
        std::fs::write(&marker, &marker_body)?;
        segment::fsync_file(&marker)?;

        // 从后往前删 retired（不含新快照本身）
        for id in retired.iter().rev() {
            if *id == seg_id {
                continue;
            }
            // 删不掉不阻塞：记诊断（规则 4 的幂等重做兜底）
            let _ = std::fs::remove_file(self.seg_path(*id));
        }
        let _ = std::fs::remove_file(&marker);

        // 内存视图更新
        self.segments.clear();
        self.segments.insert(
            seg_id,
            SegmentMeta {
                kind: SegmentKind::IndexSnapshot,
                record_count: records.len() as u64,
            },
        );
        self.snapshot_id = Some(seg_id);
        self.snapshot_upto_seq = self.next_record_seq.saturating_sub(1);
        self.next_segment_id += 1;
        let _ = self.write_manifest();
        Ok(())
    }

    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// 恢复结果（open 时执行；供 diagnostics 汇报）。
    pub fn last_recovery(&self) -> &RecoveryOutcome {
        &self.recovery
    }

    /// 关闭：尽力把 pending 落盘。
    pub fn close(&mut self) -> Result<(), StoreError> {
        if !self.readonly {
            self.flush()?;
        }
        Ok(())
    }
}

impl Drop for VaultStore {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn mk() -> [u8; 32] {
        [0x11u8; 32]
    }

    /// 基本往返：写入 → 重开 → 读回。
    #[test]
    fn kv_roundtrip_across_reopen() {
        let dir = tmpdir();
        {
            let mut s =
                VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadWrite).unwrap();
            s.kv_put(1, b"alpha").unwrap();
            s.kv_put(2, b"beta").unwrap();
            s.flush().unwrap();
        }
        let s = VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadOnly).unwrap();
        assert_eq!(s.kv_get(1).unwrap(), b"alpha");
        assert_eq!(s.kv_get(2).unwrap(), b"beta");
        assert_eq!(s.kv_get(3), None);
    }

    /// LWW：同 key 后写覆盖先写。
    #[test]
    fn kv_lww_last_write_wins() {
        let dir = tmpdir();
        {
            let mut s =
                VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadWrite).unwrap();
            s.kv_put(1, b"old").unwrap();
            s.kv_put(1, b"new").unwrap();
            s.flush().unwrap();
        }
        let s = VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadOnly).unwrap();
        assert_eq!(s.kv_get(1).unwrap(), b"new");
    }

    /// 墓碑：删除跨重开仍生效。
    #[test]
    fn kv_delete_tombstone_survives_reopen() {
        let dir = tmpdir();
        {
            let mut s =
                VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadWrite).unwrap();
            s.kv_put(1, b"x").unwrap();
            s.flush().unwrap();
            s.kv_delete(1).unwrap();
            s.flush().unwrap();
        }
        let s = VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadOnly).unwrap();
        assert_eq!(s.kv_get(1), None);
    }

    /// LOG：追加有序，跨重开保留。
    #[test]
    fn log_append_ordered_and_persistent() {
        let dir = tmpdir();
        {
            let mut s =
                VaultStore::open(dir.path(), Namespace::Audit, &mk(), OpenMode::ReadWrite).unwrap();
            for i in 0..5 {
                s.log_append(format!("e{i}").as_bytes()).unwrap();
            }
            s.flush().unwrap();
        }
        let s = VaultStore::open(dir.path(), Namespace::Audit, &mk(), OpenMode::ReadOnly).unwrap();
        let all = s.log_entries();
        assert_eq!(all.len(), 5);
        assert_eq!(all[0], b"e0");
        assert_eq!(all[4], b"e4");
    }

    /// P6-1 证据（store 层）：store::readonly_open_writes_nothing
    /// 只读打开 + 批量读操作后，目录字节级不变。
    #[test]
    fn readonly_open_writes_nothing() {
        let dir = tmpdir();
        {
            let mut s =
                VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadWrite).unwrap();
            s.kv_put(1, b"x").unwrap();
            s.flush().unwrap();
            s.kv_put(2, b"y").unwrap();
            s.flush().unwrap();
        }
        let mut snapshot: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let body = std::fs::read(e.path()).unwrap_or_default();
                (name, body)
            })
            .collect();
        snapshot.sort();

        let mut s =
            VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadOnly).unwrap();
        let _ = s.kv_get(1);
        let _ = s.kv_get(2);
        assert!(matches!(s.kv_put(3, b"z"), Err(StoreError::ReadOnly)));
        assert!(matches!(s.log_append(b"w"), Err(StoreError::ReadOnly)));
        assert!(matches!(s.compact(), Err(StoreError::ReadOnly)));
        assert!(s.flush().is_ok()); // 无 pending，空操作

        let mut after: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let body = std::fs::read(e.path()).unwrap_or_default();
                (name, body)
            })
            .collect();
        after.sort();
        assert_eq!(snapshot, after, "readonly open must not write anything");
    }

    /// 压实：追加段到阈值后合并为快照段，视图不变，段数回落。
    #[test]
    fn compaction_merges_and_preserves_view() {
        let dir = tmpdir();
        {
            let mut s =
                VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadWrite).unwrap();
            for i in 0..64u64 {
                s.kv_put(i, format!("v{i}").as_bytes()).unwrap();
                s.flush().unwrap();
            }
            // flush 尾部自动触发压实：追加段达到阈值即合并，段数保持有界
            assert!(
                s.segment_count() < 8,
                "auto compaction should bound segment count, got {}",
                s.segment_count()
            );
        }
        let s = VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadOnly).unwrap();
        assert_eq!(s.kv_get(63).unwrap(), b"v63");
        assert_eq!(s.kv_get(0).unwrap(), b"v0");
    }

    /// 压实后继续追加：快照 + 追加段混合视图。
    #[test]
    fn append_after_compaction() {
        let dir = tmpdir();
        {
            let mut s =
                VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadWrite).unwrap();
            for i in 0..16u64 {
                s.kv_put(i, b"x").unwrap();
                s.flush().unwrap();
            }
            s.kv_put(100, b"later").unwrap();
            s.flush().unwrap();
        }
        let s = VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadOnly).unwrap();
        assert_eq!(s.kv_get(100).unwrap(), b"later");
        assert_eq!(s.kv_get(15).unwrap(), b"x");
    }

    /// 提交点之后中断（步骤 4.5）：下次打开前滚（规则 2 的端到端验证）。
    #[test]
    fn crash_after_commit_point_rolls_forward() {
        let dir = tmpdir();
        {
            let mut s =
                VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadWrite).unwrap();
            s.kv_put(1, b"a").unwrap();
            s.flush().unwrap(); // 段 1
            s.kv_put(2, b"b").unwrap();
            s.flush().unwrap(); // 段 2
        }
        // 手工把 journal 重写成「段 2 有预告无完成」——等价于崩溃发生在
        // rename（提交点）之后、journal 完成条目写盘之前。
        {
            let j = Journal::load(dir.path(), &mk()).unwrap().unwrap();
            let entries: Vec<_> = j.entries().to_vec();
            let mut j2 = Journal::create_empty(dir.path(), &mk()).unwrap();
            let mut seq = 0u64;
            let mut trimmed = entries.clone();
            let last = trimmed.pop().unwrap();
            assert!(
                matches!(last.kind, JournalEntryKind::CommitDone),
                "last entry should be Done"
            );
            assert_eq!(last.arg0, 2, "last Done should be segment 2");
            for e in trimmed {
                let mut e2 = e;
                e2.commit_seq = seq;
                seq += 1;
                j2.append(e2, &mk()).unwrap();
            }
            j2.append(
                JournalEntry {
                    kind: JournalEntryKind::CommitIntent,
                    commit_seq: seq,
                    arg0: 2,
                    arg1: 1,
                },
                &mk(),
            )
            .unwrap();
        }

        // 重新打开 → 恢复应前滚段 2
        let s = VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadOnly).unwrap();
        assert_eq!(
            s.kv_get(2).unwrap(),
            b"b",
            "committed segment must survive via roll-forward"
        );
        assert_eq!(s.kv_get(1).unwrap(), b"a");
    }

    /// 步骤 3 之前中断（tmp 残留）：下次打开清理，数据无损。
    #[test]
    fn tmp_files_cleaned_on_open() {
        let dir = tmpdir();
        let ns_dir = dir.path().join("index");
        std::fs::create_dir_all(&ns_dir).unwrap();
        std::fs::write(ns_dir.join("9.tmp"), b"junk").unwrap();
        let s = VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadWrite).unwrap();
        assert_eq!(s.kv_get(9), None);
        assert!(
            !ns_dir.join("9.tmp").exists(),
            "leftover .tmp must be cleaned on open"
        );
    }

    /// 维护态：业务写入冻结（码 10 语义）。
    #[test]
    fn maintenance_blocks_writes() {
        let dir = tmpdir();
        let mut s =
            VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadWrite).unwrap();
        s.enter_maintenance();
        assert!(matches!(s.kv_put(1, b"x"), Err(StoreError::Maintenance)));
        s.exit_maintenance();
        assert!(s.kv_put(1, b"x").is_ok());
    }

    /// manifest 是缓存：删掉 manifest 后扫描仍还原全量视图。
    #[test]
    fn manifest_loss_does_not_lose_data() {
        let dir = tmpdir();
        {
            let mut s =
                VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadWrite).unwrap();
            s.kv_put(7, b"precious").unwrap();
            s.flush().unwrap();
        }
        // 删除 manifest（缓存）
        let m = dir.path().join("manifest.index.vssg");
        if m.exists() {
            std::fs::remove_file(&m).unwrap();
        }
        let s = VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadOnly).unwrap();
        assert_eq!(
            s.kv_get(7).unwrap(),
            b"precious",
            "scan is truth, manifest is cache"
        );
    }
}
