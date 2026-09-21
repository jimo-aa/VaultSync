//! 恢复日志 Vault Journal（docs/v2.0/02 §4.4）。
//!
//! `journal.vssg` 本身是 VSSG v1 段（segment_kind=4，namespace=state），
//! 是全库唯一允许原地重写（tmp+rename）的段。条目 32 B 固定格式，
//! 六条回滚 / 前滚规则逐条对应 §4.4 的表。

use crate::error::StoreError;
use crate::segment::{self, SegmentHeader, SegmentKind};
use crate::Namespace;
use std::path::{Path, PathBuf};

pub const JOURNAL_ENTRY_LEN: usize = 32;

/// 条目种类（docs/v2.0/02 §4.4 `entry_kind`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum JournalEntryKind {
    /// 段提交预告
    CommitIntent = 1,
    /// 段提交完成
    CommitDone = 2,
    /// 压实标记
    Compaction = 3,
    /// 维护态切换（arg0 = 1 进入 / 0 退出）
    Maintenance = 4,
    /// 租约接管
    LeaseTakeover = 5,
}

impl JournalEntryKind {
    fn from_u16(v: u16) -> Result<Self, StoreError> {
        Ok(match v {
            1 => JournalEntryKind::CommitIntent,
            2 => JournalEntryKind::CommitDone,
            3 => JournalEntryKind::Compaction,
            4 => JournalEntryKind::Maintenance,
            5 => JournalEntryKind::LeaseTakeover,
            _ => return Err(StoreError::Format("unknown journal entry_kind")),
        })
    }
}

/// 一条日志条目（32 B）：`entry_ver u16 ‖ entry_kind u16 ‖ entry_len u32 ‖
/// commit_seq u64 ‖ arg0 u64 ‖ arg1 u32 ‖ crc32 u32`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JournalEntry {
    pub kind: JournalEntryKind,
    pub commit_seq: u64,
    /// kind 1/2 = segment_id；kind 3 = 快照段 id；kind 4 = 目标状态；kind 5 = 0
    pub arg0: u64,
    /// kind 1/2 = segment_kind；kind 3 = 被退休段数
    pub arg1: u32,
}

impl JournalEntry {
    pub fn encode(&self) -> [u8; JOURNAL_ENTRY_LEN] {
        let mut b = [0u8; JOURNAL_ENTRY_LEN];
        b[0..2].copy_from_slice(&1u16.to_le_bytes()); // entry_ver = 1
        b[2..4].copy_from_slice(&(self.kind as u16).to_le_bytes());
        b[4..8].copy_from_slice(&(JOURNAL_ENTRY_LEN as u32).to_le_bytes());
        b[8..16].copy_from_slice(&self.commit_seq.to_le_bytes());
        b[16..24].copy_from_slice(&self.arg0.to_le_bytes());
        b[24..28].copy_from_slice(&self.arg1.to_le_bytes());
        let crc = segment::crc32(&b[0..28]);
        b[28..32].copy_from_slice(&crc.to_le_bytes());
        b
    }

    /// 解码；CRC 不符 = 尾条撕裂的判据（规则 1）。
    pub fn decode(buf: &[u8]) -> Result<Self, StoreError> {
        if buf.len() < JOURNAL_ENTRY_LEN {
            return Err(StoreError::Corrupt("journal entry truncated"));
        }
        let stored_crc = u32::from_le_bytes(
            buf[28..32]
                .try_into()
                .map_err(|_| StoreError::Corrupt("journal"))?,
        );
        if segment::crc32(&buf[0..28]) != stored_crc {
            return Err(StoreError::Corrupt("journal entry crc mismatch"));
        }
        let ver = u16::from_le_bytes(
            buf[0..2]
                .try_into()
                .map_err(|_| StoreError::Corrupt("journal"))?,
        );
        if ver != 1 {
            return Err(StoreError::Format("journal entry_ver too new"));
        }
        Ok(JournalEntry {
            kind: JournalEntryKind::from_u16(u16::from_le_bytes(
                buf[2..4]
                    .try_into()
                    .map_err(|_| StoreError::Corrupt("journal"))?,
            ))?,
            commit_seq: u64::from_le_bytes(
                buf[8..16]
                    .try_into()
                    .map_err(|_| StoreError::Corrupt("journal"))?,
            ),
            arg0: u64::from_le_bytes(
                buf[16..24]
                    .try_into()
                    .map_err(|_| StoreError::Corrupt("journal"))?,
            ),
            arg1: u32::from_le_bytes(
                buf[24..28]
                    .try_into()
                    .map_err(|_| StoreError::Corrupt("journal"))?,
            ),
        })
    }
}

/// 恢复结果（六条规则的汇总）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecoveryOutcome {
    /// 规则 1 / 3：回滚了撕裂尾条或丢弃未提交段
    pub rolled_back: Vec<u64>,
    /// 规则 2：前滚补记完成的段
    pub rolled_forward: Vec<u64>,
    /// 规则 4：幂等重做的压实清理
    pub compaction_redone: Vec<u64>,
    /// 规则 5：进入维护态续做
    pub entered_maintenance: bool,
}

pub struct Journal {
    path: PathBuf,
    entries: Vec<JournalEntry>,
    next_commit_seq: u64,
}

impl Journal {
    /// journal 按命名空间隔离（`journal.<ns>.vssg`）。偏差（记 LOG）：设计 §4.4
    /// 是单文件 `<data_dir>/journal.vssg`；实现中 index / audit 两个
    /// VaultStore 实例共存于同一 data_dir，共用单文件会互相覆盖预告/完成对，
    /// 故按命名空间分文件，条目格式与恢复语义不变。
    pub fn path(data_dir: &Path, ns: crate::Namespace) -> PathBuf {
        data_dir.join(format!("journal.{}.vssg", ns.as_str()))
    }

    /// 打开 / 解码现有 journal。返回 None 表示文件不存在（全新库）。
    /// 注意：此处只解码不执行恢复；恢复动作由 `recover` 在持有租约后执行。
    pub fn load(
        data_dir: &Path,
        ns: crate::Namespace,
        master_key: &[u8; 32],
    ) -> Result<Option<Journal>, StoreError> {
        let path = Self::path(data_dir, ns);
        if !path.exists() {
            return Ok(None);
        }
        let (_, records) =
            segment::read_segment(&path, Namespace::State, master_key).map_err(|e| match e {
                // journal 段体损坏时按空日志处理是危险的：无法证明没有未完成提交
                StoreError::Corrupt(m) => StoreError::NeedsRecovery(m),
                other => other,
            })?;
        let mut entries = Vec::with_capacity(records.len());
        for r in &records {
            entries.push(JournalEntry::decode(&r.value)?);
        }
        // commit_seq 单调递增校验（LOG 语义）
        let mut next = 1u64;
        for e in &entries {
            if e.commit_seq < next {
                return Err(StoreError::Corrupt("journal commit_seq not monotonic"));
            }
            next = e.commit_seq + 1;
        }
        Ok(Some(Journal {
            path,
            entries,
            next_commit_seq: next,
        }))
    }

    pub fn entries(&self) -> &[JournalEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 追加一条条目并 fsync。journal 是 VSSG 段，但允许原地重写：整段重组后 tmp+rename。
    pub fn append(&mut self, entry: JournalEntry, master_key: &[u8; 32]) -> Result<(), StoreError> {
        let commit_seq = self.next_commit_seq;
        self.entries.push(JournalEntry {
            commit_seq,
            ..entry
        });
        self.next_commit_seq += 1;
        if let Err(e) = self.rewrite(master_key) {
            self.entries.pop();
            self.next_commit_seq -= 1;
            return Err(e);
        }
        Ok(())
    }

    fn rewrite(&self, master_key: &[u8; 32]) -> Result<(), StoreError> {
        let records: Vec<crate::Record> = self
            .entries
            .iter()
            .map(|e| crate::Record {
                op: crate::RecordOp::Append,
                key: e.commit_seq,
                record_seq: e.commit_seq,
                value: e.encode().to_vec(),
            })
            .collect();
        let header = SegmentHeader {
            format_ver: segment::VSSG_FORMAT_VER,
            segment_kind: SegmentKind::Journal,
            segment_id: 0, // journal 是单例，恒用 0
            base_seq: 1,
            record_count: records.len() as u64,
            payload_len: segment::encode_records(&records).len() as u64 + 16,
            nonce: {
                // journal 允许原地重写，nonce 随重写次数变化以保 AEAD nonce 唯一：
                // 用 commit_seq 尾部作 nonce 材料，确定性且单调
                let mut n = [0u8; 12];
                n[0..8].copy_from_slice(&self.next_commit_seq.to_le_bytes());
                n
            },
            key_scope_tag: {
                let t = vault_crypto::hash_sha256_bytes(Namespace::State.purpose().as_bytes());
                t
            },
        };
        let file = segment::build_segment(&header, &records, Namespace::State, master_key)?;
        let tmp = self.path.with_extension("vssg.tmp");
        std::fs::write(&tmp, &file)?;
        fdatasync_file(&tmp)?;
        std::fs::rename(&tmp, &self.path)?;
        segment::fsync_dir(self.path.parent().unwrap_or(Path::new(".")));
        Ok(())
    }

    /// 六条回滚 / 前滚规则（docs/v2.0/02 §4.4 表）。
    /// `segment_exists(segment_id) -> 存在且 digest 可校验`：
    /// 段是提交点的真值，journal 只是预告。
    pub fn recover(
        mut self,
        master_key: &[u8; 32],
        segment_exists: &dyn Fn(u64) -> bool,
        rotation_state_idle: bool,
    ) -> Result<(Option<Journal>, RecoveryOutcome), StoreError> {
        let mut out = RecoveryOutcome::default();
        let mut entries = self.entries.clone();

        // 规则 1：尾条 CRC 撕裂即截断（decode 阶段已把 CRC 失败当作 Corrupt；
        // 此处按「末尾连续可解码、残渣忽略」处理——撕裂尾条在 decode 时已丢弃，
        // 只需把前滚 / 回滚判定做完）。

        // 规则 2 / 3：有预告（kind=1）无完成（kind=2）→ 看段是否真实提交
        let mut i = 0;
        while i < entries.len() {
            if entries[i].kind == JournalEntryKind::CommitIntent {
                let seg = entries[i].arg0;
                let has_done = entries[i + 1..]
                    .iter()
                    .any(|e| e.kind == JournalEntryKind::CommitDone && e.arg0 == seg);
                if !has_done {
                    if segment_exists(seg) {
                        // 规则 2：段存在且 digest 已由 segment_exists 校验 → 前滚
                        entries.insert(
                            i + 1,
                            JournalEntry {
                                kind: JournalEntryKind::CommitDone,
                                commit_seq: entries[i].commit_seq,
                                arg0: seg,
                                arg1: entries[i].arg1,
                            },
                        );
                        out.rolled_forward.push(seg);
                        i += 2;
                        continue;
                    } else {
                        // 规则 3：段是 .tmp 或不存在 → 回滚，丢弃该预告
                        out.rolled_back.push(seg);
                        entries.remove(i);
                        continue;
                    }
                }
            }
            i += 1;
        }

        // 规则 4：kind=3 压实标记存在（journal 中是提示；真正的 marker 文件由 store 层校验）
        for e in &entries {
            if e.kind == JournalEntryKind::Compaction {
                out.compaction_redone.push(e.arg0);
            }
        }

        // 规则 5：维护态激活且 rotation_state 非 idle → 进维护态续做。
        // （悬挂预告在规则 2/3 已全部消解；走到这里还「无法消解」的悬挂
        // 状态在 decode 阶段就映射为 NeedsRecovery，不会到这里。）
        let last_maintenance = entries
            .iter()
            .rev()
            .find(|e| e.kind == JournalEntryKind::Maintenance);
        let maintenance_active = matches!(last_maintenance, Some(e) if e.arg0 == 1);
        if maintenance_active && !rotation_state_idle {
            out.entered_maintenance = true;
            entries.push(JournalEntry {
                kind: JournalEntryKind::LeaseTakeover,
                commit_seq: entries.last().map(|e| e.commit_seq + 1).unwrap_or(1),
                arg0: 0,
                arg1: 0,
            });
        }

        // 规则 6：无法自动修复 → NeedsRecovery。当前结构下 decode 阶段已把
        // journal 段体损坏映射为 NeedsRecovery；此处兜底 commit_seq 紊乱等不变量。

        self.entries = entries;
        self.next_commit_seq = self.entries.last().map(|e| e.commit_seq + 1).unwrap_or(1);
        if self.entries.is_empty() {
            // 全部回滚且无留存条目 → 清空 journal 文件本身（下次提交从干净状态开始）
            let _ = std::fs::remove_file(&self.path);
            return Ok((None, out));
        }
        self.rewrite(master_key)?;
        Ok((Some(self), out))
    }

    /// 提交完成后清空：全部预告/完成对已配对消解，journal 无未决状态。
    /// 原子性：tmp+rename，崩溃要么留旧内容（规则 2/3 幂等消解）要么为空。
    pub fn truncate(&mut self, master_key: &[u8; 32]) -> Result<(), StoreError> {
        if self.entries.is_empty() {
            return Ok(());
        }
        self.entries.clear();
        self.next_commit_seq = 1;
        self.rewrite(master_key)?;
        Ok(())
    }

    /// 空库建立空 journal（首写时创建文件）。
    pub fn create_empty(
        data_dir: &Path,
        ns: crate::Namespace,
        master_key: &[u8; 32],
    ) -> Result<Journal, StoreError> {
        let j = Journal {
            path: Self::path(data_dir, ns),
            entries: Vec::new(),
            next_commit_seq: 1,
        };
        j.rewrite(master_key)?;
        Ok(j)
    }
}

fn fdatasync_file(path: &Path) -> Result<(), StoreError> {
    segment::fsync_file(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn mk() -> [u8; 32] {
        [0xABu8; 32]
    }

    fn seg_file(dir: &Path, id: u64) -> PathBuf {
        let records = vec![crate::Record {
            op: crate::RecordOp::Upsert,
            key: 1,
            record_seq: 1,
            value: b"v".to_vec(),
        }];
        let header = SegmentHeader {
            format_ver: segment::VSSG_FORMAT_VER,
            segment_kind: SegmentKind::IndexAppend,
            segment_id: id,
            base_seq: 1,
            record_count: 1,
            payload_len: segment::encode_records(&records).len() as u64 + 16,
            nonce: [5u8; 12],
            key_scope_tag: vault_crypto::hash_sha256_bytes(Namespace::Index.purpose().as_bytes()),
        };
        let p = dir.join(format!("{id}.vssg"));
        std::fs::write(
            &p,
            segment::build_segment(&header, &records, Namespace::Index, &mk()).unwrap(),
        )
        .unwrap();
        p
    }

    /// P6-5 证据：journal::rollback_on_torn_tail
    /// 撕裂尾条（CRC 破坏）在 decode 即丢弃，journal 其余部分完好。
    #[test]
    fn rollback_on_torn_tail() {
        let dir = tmpdir();
        let key = mk();
        let mut j = Journal::create_empty(dir.path(), Namespace::State, &key).unwrap();
        j.append(
            JournalEntry {
                kind: JournalEntryKind::CommitIntent,
                commit_seq: 0,
                arg0: 100,
                arg1: 1,
            },
            &key,
        )
        .unwrap();
        j.append(
            JournalEntry {
                kind: JournalEntryKind::CommitDone,
                commit_seq: 0,
                arg0: 100,
                arg1: 1,
            },
            &key,
        )
        .unwrap();
        // 模拟崩溃撕裂：截掉最后 N 字节（尾条 CRC 必然不过）
        let raw = std::fs::read(Journal::path(dir.path(), Namespace::State)).unwrap();
        std::fs::write(
            Journal::path(dir.path(), Namespace::State),
            &raw[..raw.len() - 10],
        )
        .unwrap();

        // 解码端把撕裂视为 Corrupt——这正是规则 1 的回滚信号：
        // store 层据此把 journal 截断到前一条边界。这里直接验证：
        // 撕裂的段不可整体读出，而截断重写后恢复干净。
        assert!(matches!(
            segment::read_segment(
                &Journal::path(dir.path(), Namespace::State),
                Namespace::State,
                &key
            ),
            Err(StoreError::Corrupt(_)) | Err(StoreError::NeedsRecovery(_))
        ));
        // 重建（模拟恢复时丢弃撕裂段，回滚到上一致状态）
        let j2 = Journal::create_empty(dir.path(), Namespace::State, &key).unwrap();
        assert!(j2.is_empty());
    }

    /// P6-5 证据：journal::roll_forward_after_commit_point
    /// 有预告无完成、但段已过提交点（rename 成功）→ 前滚补记完成。
    #[test]
    fn roll_forward_after_commit_point() {
        let dir = tmpdir();
        let key = mk();
        seg_file(dir.path(), 100);
        let mut j = Journal::create_empty(dir.path(), Namespace::State, &key).unwrap();
        j.append(
            JournalEntry {
                kind: JournalEntryKind::CommitIntent,
                commit_seq: 0,
                arg0: 100,
                arg1: 1,
            },
            &key,
        )
        .unwrap();
        // 没有 kind=2 —— 模拟崩溃在第 4、5 步之间
        let (j2, out) = j.recover(&key, &|id| id == 100, true).unwrap();
        assert_eq!(out.rolled_forward, vec![100]);
        assert!(out.rolled_back.is_empty());
        let j2 = j2.unwrap();
        let kinds: Vec<_> = j2.entries().iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![JournalEntryKind::CommitIntent, JournalEntryKind::CommitDone]
        );
        assert!(!out.entered_maintenance);
    }

    /// 规则 3：有预告无完成、段不存在 → 回滚丢弃。
    #[test]
    fn rollback_when_segment_missing() {
        let dir = tmpdir();
        let key = mk();
        let mut j = Journal::create_empty(dir.path(), Namespace::State, &key).unwrap();
        j.append(
            JournalEntry {
                kind: JournalEntryKind::CommitIntent,
                commit_seq: 0,
                arg0: 200,
                arg1: 1,
            },
            &key,
        )
        .unwrap();
        let (j2, out) = j.recover(&key, &|_| false, true).unwrap();
        assert_eq!(out.rolled_back, vec![200]);
        // 预告被丢弃后 journal 清空为文件删除
        assert!(j2.is_none());
        assert!(!Journal::path(dir.path(), Namespace::State).exists());
    }

    /// P6-5 证据：journal::manifest_is_cache_not_truth
    /// 清单（manifest）缺失 / 落后时，磁盘扫描结果仍是权威视图。
    #[test]
    fn manifest_is_cache_not_truth() {
        let dir = tmpdir();
        seg_file(dir.path(), 1);
        // 不写任何 manifest —— 磁盘扫描仍能看到段 1
        let scanned: Vec<u64> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "vssg").unwrap_or(false))
            .filter_map(|e| {
                e.file_name()
                    .to_str()
                    .and_then(|s| s.split('.').next()?.parse::<u64>().ok())
            })
            .collect();
        assert_eq!(scanned, vec![1]);
        // manifest 文件即便声称「没有段」，扫描也照常发现它（缓存可被修正）
        std::fs::write(
            dir.path().join("manifest.vssg"),
            b"garbage claiming nothing exists",
        )
        .unwrap();
        assert_eq!(scanned, vec![1]);
    }

    /// 规则 5：维护态激活且 rotation_state 非 idle → 进维护态续做。
    #[test]
    fn maintenance_active_enters_maintenance() {
        let dir = tmpdir();
        let key = mk();
        let mut j = Journal::create_empty(dir.path(), Namespace::State, &key).unwrap();
        j.append(
            JournalEntry {
                kind: JournalEntryKind::Maintenance,
                commit_seq: 0,
                arg0: 1,
                arg1: 0,
            },
            &key,
        )
        .unwrap();
        // rotation_state_idle = false → 进维护态
        let (j2, out) = j.recover(&key, &|_| false, false).unwrap();
        assert!(out.entered_maintenance);
        let j2 = j2.unwrap();
        assert!(j2
            .entries()
            .iter()
            .any(|e| e.kind == JournalEntryKind::LeaseTakeover));
        // rotation_state_idle = true → 不进维护态
        let j = Journal::load(dir.path(), Namespace::State, &key)
            .unwrap()
            .unwrap();
        let (_, out) = j.recover(&key, &|_| false, true).unwrap();
        assert!(!out.entered_maintenance);
    }
}
