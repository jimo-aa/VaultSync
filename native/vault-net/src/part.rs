//! 跨会话断点续传 `.part` 载体（P8-3，docs/v2.0/05-03 §6.1–§6.2）。
//!
//! 直接否证 V1.0 的「每次开始传输即截断清零、断线全量重传」：头部 + 块索引表
//! 持久化，重启后头部字节逐位不变，续传只补缺失块。
//!
//! ```text
//! 0   magic "VSPT" ｜ 4  part_ver u16=1 ｜ 6  flags u16（bit0=已终验）
//! 8   file_id u64 ｜ 16 total_bytes u64 ｜ 24 file_sha256[32]（版本判据）
//! 56  chunk_count u32 ｜ 60 index_off u32=72 ｜ 64 index_len u32=chunk_count×12
//! 68  header_crc32（覆盖 0..68）
//! 72  块索引表 chunk_count × (offset u64 ｜ len u32)；未收到填 0/0
//! 72+index_len  数据区（已收块按索引偏移存放，允许空洞）
//! ```
//!
//! 版本一致性 = `file_sha256`（对端清单整文件哈希）：对端文件已变 → 一律作废
//! 重传，绝不把两个版本的块拼成一个文件（安全优先于带宽，05-03 §6.1）。
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// 龄期上限：30 天未续 → 清理（05-03 §6.2）。
pub const MAX_AGE_MS: u64 = 30 * 24 * 3600 * 1000;

const HEADER_LEN: usize = 72;
const MAGIC: &[u8; 4] = b"VSPT";
const PART_VER: u16 = 1;
const FLAG_VERIFIED: u16 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartMeta {
    pub file_id: u64,
    pub total_bytes: u64,
    pub file_sha256: [u8; 32],
    pub chunk_count: u32,
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    crc ^ 0xFFFF_FFFF
}

/// 新建 `.part`（头部 + 全 0 索引表；已存在则返回 Err——续传走 [`open`]）。
pub fn create(path: &Path, meta: &PartMeta) -> Result<(), &'static str> {
    if path.exists() {
        return Err("part already exists");
    }
    let index_len = meta.chunk_count as usize * 12;
    let mut hdr = vec![0u8; HEADER_LEN];
    hdr[0..4].copy_from_slice(MAGIC);
    hdr[4..6].copy_from_slice(&PART_VER.to_le_bytes());
    hdr[8..16].copy_from_slice(&meta.file_id.to_le_bytes());
    hdr[16..24].copy_from_slice(&meta.total_bytes.to_le_bytes());
    hdr[24..56].copy_from_slice(&meta.file_sha256);
    hdr[56..60].copy_from_slice(&meta.chunk_count.to_le_bytes());
    hdr[60..64].copy_from_slice(&(HEADER_LEN as u32).to_le_bytes());
    hdr[64..68].copy_from_slice(&(index_len as u32).to_le_bytes());
    let crc = crc32(&hdr[0..68]);
    hdr[68..72].copy_from_slice(&crc.to_le_bytes());
    let mut f = std::fs::File::create(path).map_err(|_| "cannot create part")?;
    f.write_all(&hdr).map_err(|_| "part header write failed")?;
    f.write_all(&vec![0u8; index_len])
        .map_err(|_| "part index write failed")?;
    f.flush().map_err(|_| "part flush failed")
}

/// 打开既有 `.part`：校验 magic / 版本 / crc / 索引一致性（不猜测不修补）。
pub fn open(path: &Path) -> Result<PartFile, &'static str> {
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|_| "cannot open part")?;
    let mut hdr = [0u8; HEADER_LEN];
    f.read_exact(&mut hdr).map_err(|_| "part truncated")?;
    if &hdr[0..4] != MAGIC {
        return Err("part bad magic");
    }
    if u16::from_le_bytes([hdr[4], hdr[5]]) != PART_VER {
        return Err("part version unsupported");
    }
    let stored_crc = u32::from_le_bytes([hdr[68], hdr[69], hdr[70], hdr[71]]);
    if crc32(&hdr[0..68]) != stored_crc {
        return Err("part header crc mismatch");
    }
    let chunk_count = u32::from_le_bytes([hdr[56], hdr[57], hdr[58], hdr[59]]) as usize;
    let index_off = u32::from_le_bytes([hdr[60], hdr[61], hdr[62], hdr[63]]) as usize;
    let index_len = u32::from_le_bytes([hdr[64], hdr[65], hdr[66], hdr[67]]) as usize;
    if index_off != HEADER_LEN || index_len != chunk_count * 12 {
        return Err("part index inconsistent");
    }
    let mut index_raw = vec![0u8; index_len];
    f.seek(SeekFrom::Start(index_off as u64))
        .map_err(|_| "part seek failed")?;
    f.read_exact(&mut index_raw)
        .map_err(|_| "part index read failed")?;
    let mut index = Vec::with_capacity(chunk_count);
    for c in index_raw.chunks_exact(12) {
        let off = u64::from_le_bytes(c[0..8].try_into().expect("12B entry"));
        let len = u32::from_le_bytes(c[8..12].try_into().expect("12B entry"));
        index.push((off, len));
    }
    Ok(PartFile {
        path: path.to_path_buf(),
        file_id: u64::from_le_bytes(hdr[8..16].try_into().expect("header")),
        total_bytes: u64::from_le_bytes(hdr[16..24].try_into().expect("header")),
        file_sha256: hdr[24..56].try_into().expect("header"),
        flags: u16::from_le_bytes([hdr[6], hdr[7]]),
        index,
        data_base: (index_off + index_len) as u64,
    })
}

/// 已打开的 `.part`。
pub struct PartFile {
    path: PathBuf,
    file_id: u64,
    total_bytes: u64,
    file_sha256: [u8; 32],
    flags: u16,
    index: Vec<(u64, u32)>,
    data_base: u64,
}

impl PartFile {
    pub fn file_id(&self) -> u64 {
        self.file_id
    }
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
    pub fn file_sha256(&self) -> &[u8; 32] {
        &self.file_sha256
    }
    pub fn chunk_count(&self) -> u32 {
        self.index.len() as u32
    }
    pub fn verified(&self) -> bool {
        self.flags & FLAG_VERIFIED != 0
    }

    pub fn received_count(&self) -> u32 {
        self.index.iter().filter(|(_, l)| *l > 0).count() as u32
    }

    /// 已收块字节合计（配额与续传百分比口径）。
    pub fn received_bytes(&self) -> u64 {
        self.index.iter().map(|(_, l)| *l as u64).sum()
    }

    /// 写入第 `idx` 块（密文），登记索引。幂等（重收同块覆盖同槽）。
    pub fn write_chunk(&mut self, idx: usize, data: &[u8]) -> Result<(), &'static str> {
        if idx >= self.index.len() {
            return Err("chunk index out of range");
        }
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&self.path)
            .map_err(|_| "part not writable")?;
        // 布局：新块追加到数据区尾部（当前最大 offset+len）；重收已有块则覆盖原槽
        let (off, old_len) = self.index[idx];
        let target = if old_len > 0 {
            off
        } else {
            self.index
                .iter()
                .filter(|(_, l)| *l > 0)
                .map(|(o, l)| o + *l as u64)
                .max()
                .unwrap_or(0)
        };
        f.seek(SeekFrom::Start(self.data_base + target))
            .map_err(|_| "part seek failed")?;
        f.write_all(data).map_err(|_| "part chunk write failed")?;
        f.flush().map_err(|_| "part chunk flush failed")?;
        // 更新索引条目（72 + idx*12 起的 12B：offset u64 ｜ len u32）
        f.seek(SeekFrom::Start((HEADER_LEN + idx * 12) as u64))
            .map_err(|_| "part seek failed")?;
        f.write_all(&target.to_le_bytes())
            .map_err(|_| "part index write failed")?;
        f.write_all(&(data.len() as u32).to_le_bytes())
            .map_err(|_| "part index write failed")?;
        self.index[idx] = (target, data.len() as u32);
        Ok(())
    }

    /// 读取已收块；未收到返回 None。
    pub fn read_chunk(&self, idx: usize) -> Option<Vec<u8>> {
        let (off, len) = self.index[idx];
        if len == 0 {
            return None;
        }
        let mut f = std::fs::File::open(&self.path).ok()?;
        f.seek(SeekFrom::Start(self.data_base + off)).ok()?;
        let mut buf = vec![0u8; len as usize];
        f.read_exact(&mut buf).ok()?;
        Some(buf)
    }

    /// 缺失块序号清单（续传只请求这些块——断线重连后重传 ≤ 缺失数）。
    pub fn missing_chunks(&self) -> Vec<usize> {
        self.index
            .iter()
            .enumerate()
            .filter(|(_, (o, l))| *l == 0 && *o == 0)
            .map(|(i, _)| i)
            .collect()
    }

    /// 整文件终验通过 → flags.bit0 置位（提交由宿主的串行写者执行，本层不提交）。
    pub fn mark_verified(&mut self) -> Result<(), &'static str> {
        self.flags |= FLAG_VERIFIED;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&self.path)
            .map_err(|_| "part not writable")?;
        f.seek(SeekFrom::Start(6)).map_err(|_| "part seek failed")?;
        f.write_all(&self.flags.to_le_bytes())
            .map_err(|_| "part flags write failed")
    }
}

/// 启动扫描的单文件动作（05-03 §6.2 六情形；情形 2/3 的判据由调用方注入，
/// 本 crate 不得知道索引与配对语义）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanAction {
    /// 头部合法且版本一致 → 可续传（已收块数）。
    Resumable { file_id: u64, received: u32 },
    /// 头部损坏（magic / crc / 索引不一致）→ 已删除。
    DeletedCorrupt,
    /// 调用方判据不通过（file_id 不存在 / 哈希版本不符）→ 已删除。
    DeletedStale { reason: &'static str },
    /// 龄期超 30 天 → 已删除。
    DeletedAged,
    /// 配额超限（最久未续先淘汰）→ 已删除。
    DeletedOverQuota,
}

/// 启动恢复扫描：`keep` = 调用方版本判据（file_id, file_sha256）→ 有效？
/// `quota_bytes` = `max(5 GiB, 库 × 20%)`；`now_ms` 供测试注入时钟。
pub fn scan_dir(
    dir: &Path,
    now_ms: u64,
    quota_bytes: u64,
    keep: &dyn Fn(u64, &[u8; 32]) -> bool,
) -> Vec<(PathBuf, ScanAction)> {
    let mut parts: Vec<(PathBuf, u64, u64)> = Vec::new(); // (path, mtime_ms, size)
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("part") {
            continue;
        }
        // 情形 1：头部损坏
        let Ok(pf) = open(&p) else {
            let _ = std::fs::remove_file(&p);
            out.push((p, ScanAction::DeletedCorrupt));
            continue;
        };
        // 情形 3：版本判据（file_id 存在性 / 哈希一致性）由调用方裁定
        if !keep(pf.file_id(), pf.file_sha256()) {
            let _ = std::fs::remove_file(&p);
            out.push((
                p,
                ScanAction::DeletedStale {
                    reason: "version_changed_or_unknown",
                },
            ));
            continue;
        }
        // 情形 6：龄期
        let mtime_ms = e
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64);
        let age_ok = mtime_ms
            .map(|t| now_ms.saturating_sub(t) <= MAX_AGE_MS)
            .unwrap_or(false);
        if !age_ok {
            let _ = std::fs::remove_file(&p);
            out.push((p, ScanAction::DeletedAged));
            continue;
        }
        let size = e.metadata().map(|m| m.len()).unwrap_or(0);
        parts.push((p, mtime_ms.unwrap_or(0), size));
    }
    // 情形 5：配额（最久未续先淘汰；mtime 升序近似「最久未续」）
    let total: u64 = parts.iter().map(|(_, _, s)| *s).sum();
    if total > quota_bytes {
        let mut by_oldest = parts.clone();
        by_oldest.sort_by_key(|(_, t, _)| *t);
        let mut over = total - quota_bytes;
        for (p, _, size) in by_oldest {
            if over == 0 {
                break;
            }
            let _ = std::fs::remove_file(&p);
            over = over.saturating_sub(size);
            let name = p.file_name().map(|n| n.to_os_string());
            parts.retain(|(pp, _, _)| pp.file_name().map(|n| n.to_os_string()) != name);
            out.push((p, ScanAction::DeletedOverQuota));
        }
    }
    // 情形 4：合法且版本一致 → 续传
    for (p, _, _) in parts {
        let (file_id, received) = match open(&p) {
            Ok(pf) => (pf.file_id(), pf.received_count()),
            Err(_) => continue,
        };
        out.push((p, ScanAction::Resumable { file_id, received }));
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn vault_net_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    fn meta(id: u64, chunks: u32) -> PartMeta {
        PartMeta {
            file_id: id,
            total_bytes: 3000,
            // 判据哈希按「期望内容」计算：三个块各为重复字节 1/2/3（与测试块一致）
            file_sha256: vault_crypto::hash_sha256_bytes(&[
                vec![1u8; 1000],
                vec![2u8; 1000],
                vec![3u8; 1000],
            ]
            .concat()),
            chunk_count: chunks,
        }
    }

    /// 证据（05-03 §6.2）：跨会话续传——重启后头部字节不变、只补缺失块、
    /// 组装结果哈希与版本判据一致。
    #[test]
    fn resume_across_sessions_and_header_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("p2p-7.part");
        create(&p, &meta(7, 3)).unwrap();
        let mut pf = open(&p).unwrap();
        assert_eq!(pf.received_count(), 0);
        let c0 = vec![1u8; 1000];
        let c1 = vec![2u8; 1000];
        let c2 = vec![3u8; 1000];
        pf.write_chunk(0, &c0).unwrap();
        pf.write_chunk(1, &c1).unwrap();
        assert_eq!(pf.received_count(), 2);
        let header_before = std::fs::read(&p).unwrap()[..72].to_vec();
        drop(pf);

        // 「重启」：重新 open，头部 72B 逐位不变（否证 V1.0 的截断清零）
        let mut pf2 = open(&p).unwrap();
        let header_after = std::fs::read(&p).unwrap()[..72].to_vec();
        assert_eq!(header_before, header_after, "重启后 .part 头部必须逐位不变");
        assert_eq!(pf2.received_count(), 2, "已收块跨会话保留");
        // 只补缺失块（重传 ≤ 缺失数）
        assert_eq!(pf2.missing_chunks(), vec![2]);
        pf2.write_chunk(2, &c2).unwrap();
        assert_eq!(pf2.received_count(), 3);

        // 组装 + 版本终验
        let mut assembled = Vec::new();
        for i in 0..3 {
            assembled.extend_from_slice(&pf2.read_chunk(i).unwrap());
        }
        assert_eq!(
            vault_crypto::hash_sha256_bytes(&assembled),
            *pf2.file_sha256(),
            "组装结果必须与版本判据哈希一致"
        );
    }

    /// 证据（05-03 §6.2）：哈希版本不符 → 作废（安全优先于带宽）。
    #[test]
    fn sha_mismatch_discards_part() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("p2p-9.part");
        create(&p, &meta(9, 2)).unwrap();
        // 对端文件已变：调用方判据返回 false
        let actions = scan_dir(dir.path(), vault_net_now(), u64::MAX, &|id, _sha| id != 9);
        assert!(
            actions
                .iter()
                .any(|(p, a)| p.ends_with("p2p-9.part")
                    && matches!(a, ScanAction::DeletedStale { .. })),
            "版本不符必须删除：{actions:?}"
        );
        assert!(!p.exists());
    }

    /// 证据（05-03 §6.2）：配额上限（最久未续先淘汰）。
    #[test]
    fn part_quota_enforced() {
        let dir = tempfile::tempdir().unwrap();
        for id in [1u64, 2, 3] {
            let p = dir.path().join(format!("p2p-{id}.part"));
            create(&p, &meta(id, 1)).unwrap();
            let mut pf = open(&p).unwrap();
            pf.write_chunk(0, &vec![id as u8; 1000]).unwrap();
            std::thread::sleep(Duration::from_millis(15)); // mtime 区分新旧
        }
        // 配额 2500B < 3000B → 最旧的 id=1 被淘汰
        let actions = scan_dir(dir.path(), vault_net_now(), 2500, &|_, _| true);
        assert!(actions
            .iter()
            .any(|(p, a)| p.ends_with("p2p-1.part") && *a == ScanAction::DeletedOverQuota));
        assert!(!dir.path().join("p2p-1.part").exists());
        assert!(dir.path().join("p2p-3.part").exists(), "最新的保留");
    }

    /// 头部损坏 → 删除（不猜测不修补）。
    #[test]
    fn corrupt_header_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("p2p-5.part");
        create(&p, &meta(5, 2)).unwrap();
        let mut raw = std::fs::read(&p).unwrap();
        raw[30] ^= 0xFF; // 破坏 crc 覆盖区
        std::fs::write(&p, &raw).unwrap();
        let actions = scan_dir(dir.path(), vault_net_now(), u64::MAX, &|_, _| true);
        assert!(actions
            .iter()
            .any(|(_, a)| *a == ScanAction::DeletedCorrupt));
        assert!(!p.exists());
    }
}
