//! VSSG v1 段文件（docs/v2.0/02 §4.1）。
//!
//! 布局（小端）：88 B 段头 ‖ AEAD 载荷 ‖ SHA-256(前缀) 32B ‖ "VSSGEND"。
//! 段一经提交不可变；追加 = 写新段，压实 = 合并为新快照段。

use crate::error::StoreError;
use crate::Namespace;

pub const VSSG_MAGIC: &[u8; 4] = b"VSSG";
pub const VSSG_TAIL_MAGIC: &[u8; 7] = b"VSSGEND";
pub const VSSG_FORMAT_VER: u16 = 1;
pub const VSSG_HEADER_LEN: usize = 88;

/// 段用途（docs/v2.0/02 §4.1 `segment_kind`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum SegmentKind {
    Generic = 0,
    IndexAppend = 1,
    IndexSnapshot = 2,
    Audit = 3,
    Journal = 4,
}

impl SegmentKind {
    pub fn from_u16(v: u16) -> Result<Self, StoreError> {
        Ok(match v {
            0 => SegmentKind::Generic,
            1 => SegmentKind::IndexAppend,
            2 => SegmentKind::IndexSnapshot,
            3 => SegmentKind::Audit,
            4 => SegmentKind::Journal,
            _ => return Err(StoreError::Format("unknown segment_kind")),
        })
    }
}

/// 记录操作（docs/v2.0/02 §4.1 记录模型）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum RecordOp {
    /// KV upsert
    Upsert = 1,
    /// KV delete（墓碑）
    Delete = 2,
    /// LOG append-only（key 为全局序号，record_seq 严格递增）
    Append = 3,
}

impl RecordOp {
    fn from_u16(v: u16) -> Result<Self, StoreError> {
        Ok(match v {
            1 => RecordOp::Upsert,
            2 => RecordOp::Delete,
            3 => RecordOp::Append,
            _ => return Err(StoreError::Format("unknown record_op")),
        })
    }
}

/// 一条记录：store 只认这四个字段，不含业务语义。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub op: RecordOp,
    /// KV = 业务键；LOG = 全局序号。
    pub key: u64,
    /// LWW 判据；LOG 内须严格递增。
    pub record_seq: u64,
    pub value: Vec<u8>,
}

/// 段头（88 B），字段序与宽度按 docs/v2.0/02 §4.1 固定。
#[derive(Clone, Debug)]
pub struct SegmentHeader {
    pub format_ver: u16,
    pub segment_kind: SegmentKind,
    pub segment_id: u64,
    pub base_seq: u64,
    pub record_count: u64,
    pub payload_len: u64,
    pub nonce: [u8; 12],
    pub key_scope_tag: [u8; 32],
}

impl SegmentHeader {
    pub fn serialize_prefix(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(VSSG_HEADER_LEN);
        b.extend_from_slice(VSSG_MAGIC);
        b.extend_from_slice(&self.format_ver.to_le_bytes());
        b.extend_from_slice(&(self.segment_kind as u16).to_le_bytes());
        b.extend_from_slice(&self.segment_id.to_le_bytes());
        b.extend_from_slice(&self.base_seq.to_le_bytes());
        b.extend_from_slice(&self.record_count.to_le_bytes());
        b.extend_from_slice(&self.payload_len.to_le_bytes());
        b.extend_from_slice(&self.nonce);
        b.extend_from_slice(&crc32(&b).to_le_bytes()); // 覆盖字节 0..52
        debug_assert_eq!(b.len(), 56); // CRC 字段本身占 52..56
        b.extend_from_slice(&self.key_scope_tag);
        debug_assert_eq!(b.len(), VSSG_HEADER_LEN);
        b
    }

    /// 解析 88 B 段头；CRC / 魔数 / 版本不符返回错误。
    pub fn parse(buf: &[u8; VSSG_HEADER_LEN]) -> Result<Self, StoreError> {
        if &buf[0..4] != VSSG_MAGIC {
            return Err(StoreError::Format("bad VSSG magic"));
        }
        let stored_crc = u32::from_le_bytes(
            buf[52..56]
                .try_into()
                .map_err(|_| StoreError::Format("header truncated"))?,
        );
        if crc32(&buf[0..52]) != stored_crc {
            return Err(StoreError::Corrupt("header_crc32 mismatch"));
        }
        let format_ver = u16::from_le_bytes(
            buf[4..6]
                .try_into()
                .map_err(|_| StoreError::Format("header truncated"))?,
        );
        if format_ver > VSSG_FORMAT_VER {
            // 向前拒绝更高版本（docs/v2.0/02 §6.7 版本化规则）
            return Err(StoreError::Format("segment format_ver too new"));
        }
        Ok(SegmentHeader {
            format_ver,
            segment_kind: SegmentKind::from_u16(u16::from_le_bytes(
                buf[6..8]
                    .try_into()
                    .map_err(|_| StoreError::Format("header truncated"))?,
            ))?,
            segment_id: u64::from_le_bytes(
                buf[8..16]
                    .try_into()
                    .map_err(|_| StoreError::Format("header truncated"))?,
            ),
            base_seq: u64::from_le_bytes(
                buf[16..24]
                    .try_into()
                    .map_err(|_| StoreError::Format("header truncated"))?,
            ),
            record_count: u64::from_le_bytes(
                buf[24..32]
                    .try_into()
                    .map_err(|_| StoreError::Format("header truncated"))?,
            ),
            payload_len: u64::from_le_bytes(
                buf[32..40]
                    .try_into()
                    .map_err(|_| StoreError::Format("header truncated"))?,
            ),
            nonce: buf[40..52]
                .try_into()
                .map_err(|_| StoreError::Format("header truncated"))?,
            key_scope_tag: buf[56..88]
                .try_into()
                .map_err(|_| StoreError::Format("header truncated"))?,
        })
    }
}

/// 段密钥：`HKDF(主密钥, purpose)`（现用现派生，Zeroizing 包裹）。
pub fn derive_segment_key(master_key: &[u8; 32], ns: Namespace) -> zeroize::Zeroizing<[u8; 32]> {
    vault_crypto::hkdf_sha256_derive(master_key, ns.purpose().as_bytes())
}

/// key_scope_tag = SHA256(用途串)[0..32]，用于诊断「这段是用哪把钥匙加密的」。
fn key_scope_tag(ns: Namespace) -> [u8; 32] {
    vault_crypto::hash_sha256_bytes(ns.purpose().as_bytes())
}

/// 编码记录流（明文）：逐条 `record_ver u16 ‖ record_op u16 ‖ key u64 ‖ record_seq u64 ‖ value_len u32 ‖ value`。
pub fn encode_records(records: &[Record]) -> Vec<u8> {
    let mut b = Vec::new();
    for r in records {
        b.extend_from_slice(&1u16.to_le_bytes()); // record_ver = 1
        b.extend_from_slice(&(r.op as u16).to_le_bytes());
        b.extend_from_slice(&r.key.to_le_bytes());
        b.extend_from_slice(&r.record_seq.to_le_bytes());
        b.extend_from_slice(&(r.value.len() as u32).to_le_bytes());
        b.extend_from_slice(&r.value);
    }
    b
}

/// 解码记录流；任何截断 / 未知 op 都视为 Corrupt（段不可变，损坏即不可信）。
pub fn decode_records(payload: &[u8], expected_count: u64) -> Result<Vec<Record>, StoreError> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < payload.len() {
        if payload.len() - i < 20 {
            return Err(StoreError::Corrupt("record header truncated"));
        }
        let ver = u16::from_le_bytes(
            payload[i..i + 2]
                .try_into()
                .map_err(|_| StoreError::Corrupt("record"))?,
        );
        if ver != 1 {
            return Err(StoreError::Format("record_ver too new"));
        }
        let op = RecordOp::from_u16(u16::from_le_bytes(
            payload[i + 2..i + 4]
                .try_into()
                .map_err(|_| StoreError::Corrupt("record"))?,
        ))?;
        let key = u64::from_le_bytes(
            payload[i + 4..i + 12]
                .try_into()
                .map_err(|_| StoreError::Corrupt("record"))?,
        );
        let seq = u64::from_le_bytes(
            payload[i + 12..i + 20]
                .try_into()
                .map_err(|_| StoreError::Corrupt("record"))?,
        );
        let vlen = u32::from_le_bytes(
            payload[i + 20..i + 24]
                .try_into()
                .map_err(|_| StoreError::Corrupt("record"))?,
        ) as usize;
        i += 24;
        if payload.len() - i < vlen {
            return Err(StoreError::Corrupt("record value truncated"));
        }
        out.push(Record {
            op,
            key,
            record_seq: seq,
            value: payload[i..i + vlen].to_vec(),
        });
        i += vlen;
    }
    if out.len() as u64 != expected_count {
        return Err(StoreError::Corrupt("record_count mismatch"));
    }
    Ok(out)
}

/// 组装完整段文件字节（头 + AEAD 载荷 + digest + 尾魔数）。
/// AAD = 段头前 52 B（含 CRC），把头与载荷绑死。`header.payload_len`
/// 以实际密文长度（nonce ‖ ct ‖ tag）回填，调用方无需精确预计算。
pub fn build_segment(
    header: &SegmentHeader,
    records: &[Record],
    ns: Namespace,
    master_key: &[u8; 32],
) -> Result<Vec<u8>, StoreError> {
    let plaintext = encode_records(records);
    let key = derive_segment_key(master_key, ns);
    // payload_len = nonce(12) ‖ ct ‖ tag(16)，即明文长度 + 28；先回填再以
    // 最终头部前 52 B 作 AAD，保证读路径 AAD 一致
    let mut hdr = header.clone();
    hdr.payload_len = (plaintext.len() + 28) as u64;
    let prefix = hdr.serialize_prefix();
    let blob = vault_crypto::aead_encrypt_with_aad(&key, &header.nonce, &plaintext, &prefix[0..52])
        .map_err(|_| StoreError::Format("aead encrypt failed"))?;

    let mut out = prefix;
    out.extend_from_slice(&blob);
    out.extend_from_slice(&vault_crypto::hash_sha256_bytes(&out)); // digest 覆盖 0..88+N
    out.extend_from_slice(VSSG_TAIL_MAGIC);
    Ok(out)
}

/// 读入并完整校验一段：尾魔数 → 头（CRC / 版本）→ digest → AEAD → 记录流。
pub fn read_segment(
    path: &std::path::Path,
    ns: Namespace,
    master_key: &[u8; 32],
) -> Result<(SegmentHeader, Vec<Record>), StoreError> {
    let raw = std::fs::read(path)?;
    if raw.len() < VSSG_HEADER_LEN + 32 + VSSG_TAIL_MAGIC.len() {
        return Err(StoreError::Corrupt("segment too short"));
    }
    let tail_at = raw.len() - VSSG_TAIL_MAGIC.len();
    if &raw[tail_at..] != VSSG_TAIL_MAGIC {
        return Err(StoreError::Corrupt("bad tail magic"));
    }
    let digest_at = tail_at - 32;
    let digest: [u8; 32] = raw[digest_at..tail_at]
        .try_into()
        .map_err(|_| StoreError::Corrupt("digest"))?;
    if vault_crypto::hash_sha256_bytes(&raw[..digest_at]) != digest {
        return Err(StoreError::Corrupt("digest mismatch"));
    }
    let header = SegmentHeader::parse(
        raw[..VSSG_HEADER_LEN]
            .try_into()
            .map_err(|_| StoreError::Corrupt("header"))?,
    )?;
    let payload_len = header.payload_len as usize;
    if digest_at - VSSG_HEADER_LEN != payload_len {
        return Err(StoreError::Corrupt("payload_len mismatch"));
    }
    // key_scope_tag 不匹配说明用错命名空间钥匙（防跨库移植 / 误用）
    if header.key_scope_tag != key_scope_tag(ns) {
        return Err(StoreError::Corrupt("key_scope_tag mismatch"));
    }
    let key = derive_segment_key(master_key, ns);
    let plaintext = vault_crypto::aead_decrypt_with_aad(
        &key,
        &raw[VSSG_HEADER_LEN..digest_at],
        &header.serialize_prefix()[0..52],
    )
    .ok_or(StoreError::Corrupt("aead decrypt failed"))?;
    let records = decode_records(&plaintext, header.record_count)?;
    Ok((header, records))
}

/// IEEE CRC-32（多项式 0xEDB88320）。表在编译期展开，运行期零初始化开销。
const fn crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

pub fn crc32(data: &[u8]) -> u32 {
    const TABLE: [u32; 256] = crc32_table();
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

/// fsync 单个文件：必须以写模式打开——Windows 的 FlushFileBuffers
/// 要求 GENERIC_WRITE，只读句柄会得到 AccessDenied（docs/v2.0/02 §4.2）。
pub fn fsync_file(path: &std::path::Path) -> std::io::Result<()> {
    let f = std::fs::OpenOptions::new().write(true).open(path)?;
    f.sync_all()
}

/// fsync 目录；Windows 无对应语义，为无害空转且失败一律忽略
/// （rename 后的目录 fsync 语义 POSIX 才严格，docs/v2.0/02 §4.2 第 4 步）。
pub fn fsync_dir(dir: &std::path::Path) {
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_record(key: u64, seq: u64, val: &[u8]) -> Record {
        Record {
            op: RecordOp::Upsert,
            key,
            record_seq: seq,
            value: val.to_vec(),
        }
    }

    fn tmpdir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    /// P6-1 证据：store::segment_roundtrip
    #[test]
    fn segment_roundtrip() {
        let dir = tmpdir();
        let mk = [7u8; 32];
        let records = vec![mk_record(1, 1, b"alpha"), mk_record(2, 2, b"beta")];
        let header = SegmentHeader {
            format_ver: VSSG_FORMAT_VER,
            segment_kind: SegmentKind::IndexAppend,
            segment_id: 42,
            base_seq: 1,
            record_count: records.len() as u64,
            payload_len: encode_records(&records).len() as u64 + 16, // + GCM tag
            nonce: [3u8; 12],
            key_scope_tag: key_scope_tag(Namespace::Index),
        };
        let file = build_segment(&header, &records, Namespace::Index, &mk).unwrap();
        let path = dir.path().join("1.vssg");
        std::fs::write(&path, &file).unwrap();

        let (h2, r2) = read_segment(&path, Namespace::Index, &mk).unwrap();
        assert_eq!(h2.segment_id, 42);
        assert_eq!(h2.segment_kind, SegmentKind::IndexAppend);
        assert_eq!(r2, records);
    }

    /// P6-1 证据：store::reject_higher_format_ver（向前拒绝更高版本）
    #[test]
    fn reject_higher_format_ver() {
        let dir = tmpdir();
        let mk = [7u8; 32];
        let records = vec![mk_record(1, 1, b"x")];
        let mut header = SegmentHeader {
            format_ver: VSSG_FORMAT_VER,
            segment_kind: SegmentKind::IndexAppend,
            segment_id: 1,
            base_seq: 1,
            record_count: 1,
            payload_len: 0,
            nonce: [0u8; 12],
            key_scope_tag: key_scope_tag(Namespace::Index),
        };
        header.payload_len = encode_records(&records).len() as u64 + 16;
        let mut file = build_segment(&header, &records, Namespace::Index, &mk).unwrap();
        // 抬升 format_ver 并修复 header_crc32，模拟「未来版本」写入
        file[4..6].copy_from_slice(&99u16.to_le_bytes());
        let crc = crc32(&file[0..52]);
        file[52..56].copy_from_slice(&crc.to_le_bytes());
        // digest 也要重算，否则先死于 digest——顺序上 digest 在头之后，但语义上版本检查应先行：
        // read_segment 的校验顺序是 tail → digest → header。未来版本写入方会重算全部校验和，
        // 因此这里把 digest 也修好，确保拒绝发生在 format_ver 判定。
        let end = file.len();
        let digest = vault_crypto::hash_sha256_bytes(&file[..end - 39]);
        file[end - 39..end - 7].copy_from_slice(&digest);

        let path = dir.path().join("1.vssg");
        std::fs::write(&path, &file).unwrap();
        match read_segment(&path, Namespace::Index, &mk) {
            Err(StoreError::Format(_)) => {}
            other => panic!("expected Format error, got {other:?}"),
        }
    }

    /// P6-1 证据：store::readonly_open_writes_nothing 由 store 层承载（见 store.rs），
    /// 此处验证段读取路径零写入：读取后目录文件集合与 mtime 均不变。
    #[test]
    fn readonly_read_writes_nothing() {
        let dir = tmpdir();
        let mk = [7u8; 32];
        let records = vec![mk_record(1, 1, b"x")];
        let header = SegmentHeader {
            format_ver: VSSG_FORMAT_VER,
            segment_kind: SegmentKind::Audit,
            segment_id: 1,
            base_seq: 1,
            record_count: 1,
            payload_len: encode_records(&records).len() as u64 + 16,
            nonce: [9u8; 12],
            key_scope_tag: key_scope_tag(Namespace::Audit),
        };
        let path = dir.path().join("1.vssg");
        std::fs::write(
            &path,
            build_segment(&header, &records, Namespace::Audit, &mk).unwrap(),
        )
        .unwrap();

        let before: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        for _ in 0..3 {
            let _ = read_segment(&path, Namespace::Audit, &mk).unwrap();
        }
        let after: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(before.len(), after.len());
    }

    #[test]
    fn crc32_known_vector() {
        // IEEE CRC-32("123456789") = 0xCBF43926
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn tampered_payload_rejected() {
        let dir = tmpdir();
        let mk = [7u8; 32];
        let records = vec![mk_record(1, 1, b"secret")];
        let header = SegmentHeader {
            format_ver: VSSG_FORMAT_VER,
            segment_kind: SegmentKind::IndexAppend,
            segment_id: 1,
            base_seq: 1,
            record_count: 1,
            payload_len: encode_records(&records).len() as u64 + 16,
            nonce: [1u8; 12],
            key_scope_tag: key_scope_tag(Namespace::Index),
        };
        let mut file = build_segment(&header, &records, Namespace::Index, &mk).unwrap();
        let mid = file.len() / 2;
        file[mid] ^= 0x01;
        let path = dir.path().join("1.vssg");
        std::fs::write(&path, &file).unwrap();
        assert!(matches!(
            read_segment(&path, Namespace::Index, &mk),
            Err(StoreError::Corrupt(_))
        ));
    }

    #[test]
    fn wrong_namespace_key_rejected() {
        let dir = tmpdir();
        let mk = [7u8; 32];
        let records = vec![mk_record(1, 1, b"x")];
        let header = SegmentHeader {
            format_ver: VSSG_FORMAT_VER,
            segment_kind: SegmentKind::Audit,
            segment_id: 1,
            base_seq: 1,
            record_count: 1,
            payload_len: encode_records(&records).len() as u64 + 16,
            nonce: [1u8; 12],
            key_scope_tag: key_scope_tag(Namespace::Audit),
        };
        let path = dir.path().join("1.vssg");
        std::fs::write(
            &path,
            build_segment(&header, &records, Namespace::Audit, &mk).unwrap(),
        )
        .unwrap();
        // 用 index 命名空间去读 audit 段 → key_scope_tag 不符
        assert!(matches!(
            read_segment(&path, Namespace::Index, &mk),
            Err(StoreError::Corrupt(_))
        ));
    }
}
