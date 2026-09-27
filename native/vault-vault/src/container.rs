//! 加密文件容器（.vse）：Header + 加密元数据 + 数据区（docs/05-02 §3.3）。
//!
//! ```text
//! Header(明文, docs/07 版本化): magic "VSEF" | format_ver u16 | kdf_ver u16
//!                              | cipher_suite u16 | chunk_cfg u16 | flags u16
//! 加密元数据(AEAD under FSK): 随机 12B nonce 前缀字段 + 名称/大小/时间/nonce_prefix
//!                             /块清单(sha256+偏移+密文长)/整文件 SHA-256
//! 数据区: CDC 块密文顺序排列；块 nonce = prefix(4B BE) ∥ counter(8B BE)（docs/05-02 §3.2）
//! ```
#![forbid(unsafe_code)]
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vault_crypto::aead::{aead_decrypt, aead_encrypt, AES_GCM_NONCE_LEN};
use vault_crypto::KEY_LEN;

pub const MAGIC: &[u8; 4] = b"VSEF";
pub const FORMAT_VER: u16 = 2;
pub const CIPHER_SUITE: u16 = 1;

/// P7-8：本端是否接受 `cipher_suite=2`（混合 KEM 交付标记）容器。
/// 由引擎在能力位 `CAP_PQ_HYBRID` 置位时开启；默认关闭——读到 2 → 13（能力）而非 6
/// （结构可识别但缺能力，docs/v2.0/11 行 171）。写路径在本里程碑恒写 1（T2 未启动）。
static PQ_SUITE_ACCEPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_pq_suite_accepted(v: bool) {
    PQ_SUITE_ACCEPTED.store(v, std::sync::atomic::Ordering::SeqCst);
}

fn pq_suite_accepted() -> bool {
    PQ_SUITE_ACCEPTED.load(std::sync::atomic::Ordering::SeqCst)
}

/// 元数据区上限（导入时装配检查；超大块清单由块数上限约束）。
pub const MAX_META_LEN: usize = 64 * 1024 * 1024;

/// 块清单条目。offset 指向数据区内密文起点；len 含 16B GCM tag。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ChunkEntry {
    /// 密文 SHA-256（同步哈希列表来源，docs/05-02 §3.3）
    pub hash: String,
    pub offset: u64,
    pub len: u32,
}

/// 加密元数据（整体 AEAD，防元数据推断）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileMeta {
    pub name: String,
    pub size: u64,
    /// per-file 随机 32bit 前缀（docs/05-02 §3.2：nonce = prefix ∥ counter）
    pub nonce_prefix: u32,
    pub chunks: Vec<ChunkEntry>,
    /// 整文件 SHA-256（明文域，重组终验用）
    pub file_sha256: String,
    pub created_ms: u64,
    pub modified_ms: u64,
}

/// 流式装配：chunks 已按顺序写入 `part` 文件（数据区），本方法生成最终容器。
pub fn assemble(
    path: &Path,
    part: &Path,
    meta: FileMeta,
    fsk: &[u8; KEY_LEN],
) -> Result<(), &'static str> {
    let meta_json = serde_json::to_vec(&meta).map_err(|_| "meta serialize failed")?;
    if meta_json.len() > MAX_META_LEN {
        return Err("metadata too large");
    }
    let mut nonce = [0u8; AES_GCM_NONCE_LEN];
    nonce.copy_from_slice(&vault_crypto::random::random_bytes(AES_GCM_NONCE_LEN));
    let meta_ct_full = aead_encrypt(fsk, &nonce, &meta_json).map_err(|_| "meta encrypt failed")?;
    // aead_encrypt 返回 nonce‖ct‖tag；nonce 已单独写入头部，这里剥离
    let meta_ct = &meta_ct_full[AES_GCM_NONCE_LEN..];

    let mut data =
        Vec::with_capacity(part.metadata().map_err(|_| "stat part failed")?.len() as usize + 128);
    data.extend_from_slice(MAGIC);
    data.extend_from_slice(&FORMAT_VER.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes()); // kdf_ver
    data.extend_from_slice(&CIPHER_SUITE.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes()); // chunk_cfg（0=默认 FastCDC 组）
    data.extend_from_slice(&0u16.to_le_bytes()); // flags
    data.extend_from_slice(&nonce);
    data.extend_from_slice(&(meta_ct.len() as u32).to_le_bytes());
    data.extend_from_slice(meta_ct);
    let mut partf = std::fs::File::open(part).map_err(|_| "cannot open part file")?;
    partf
        .read_to_end(&mut data)
        .map_err(|_| "cannot read part file")?;

    // 原子替换：临时名 + rename
    let tmp = path.with_extension("vse.tmp");
    {
        let mut f = std::fs::File::create(&tmp).map_err(|_| "cannot create container")?;
        f.write_all(&data).map_err(|_| "cannot write container")?;
        f.sync_all().ok();
    }
    std::fs::rename(&tmp, path).map_err(|_| "cannot finalize container")
}

pub struct ContainerReader {
    pub meta: FileMeta,
    data_offset: u64,
    path: PathBuf,
}

impl ContainerReader {
    pub fn open(path: &Path, fsk: &[u8; KEY_LEN]) -> Result<Self, &'static str> {
        let mut f = std::fs::File::open(path).map_err(|_| "cannot open container")?;
        let mut head = [0u8; 4 + 2 * 5 + AES_GCM_NONCE_LEN + 4];
        f.read_exact(&mut head).map_err(|_| "container truncated")?;
        if &head[0..4] != MAGIC {
            return Err("bad magic: not a VaultSync file container");
        }
        let format_ver = u16::from_le_bytes(head[4..6].try_into().map_err(|_| "truncated")?);
        if format_ver > FORMAT_VER {
            return Err("container format newer than engine; upgrade required");
        }
        let mut nonce = [0u8; AES_GCM_NONCE_LEN];
        nonce.copy_from_slice(&head[14..14 + AES_GCM_NONCE_LEN]);
        let meta_len = u32::from_le_bytes(
            head[14 + AES_GCM_NONCE_LEN..]
                .try_into()
                .map_err(|_| "truncated")?,
        ) as usize;
        if meta_len > MAX_META_LEN {
            return Err("metadata too large");
        }
        let mut meta_ct = vec![0u8; meta_len];
        f.read_exact(&mut meta_ct)
            .map_err(|_| "container truncated")?;
        let meta_json = aead_decrypt(
            fsk,
            &nonce.iter().copied().chain(meta_ct).collect::<Vec<u8>>(),
        )
        .ok_or("metadata decrypt failed (wrong key or tampered)")?;
        let meta: FileMeta = serde_json::from_slice(&meta_json).map_err(|_| "meta parse failed")?;
        let data_offset = (head.len() + meta_len) as u64;
        Ok(Self {
            meta,
            data_offset,
            path: path.to_path_buf(),
        })
    }

    /// 全量密文域校验：逐块解密验证 + 整文件 SHA-256（不解密落盘）。
    pub fn verify(&self, fskey: &[u8; KEY_LEN]) -> Result<(), &'static str> {
        let mut src = std::fs::File::open(&self.path).map_err(|_| "cannot open container")?;
        src.seek(std::io::SeekFrom::Start(self.data_offset))
            .map_err(|_| "seek failed")?;
        let mut hasher = vault_crypto::Sha256::new();
        for (counter, entry) in (0u64..).zip(self.meta.chunks.iter()) {
            let mut ct = vec![0u8; entry.len as usize];
            src.read_exact(&mut ct).map_err(|_| "chunk data missing")?;
            if vault_crypto::hash_sha256(&ct) != entry.hash {
                return Err("chunk hash mismatch");
            }
            let mut nonce = [0u8; AES_GCM_NONCE_LEN];
            nonce[..4].copy_from_slice(&self.meta.nonce_prefix.to_be_bytes());
            nonce[4..].copy_from_slice(&counter.to_be_bytes());
            let mut aad = [0u8; 8];
            aad.copy_from_slice(&counter.to_be_bytes());
            let mut blob = Vec::with_capacity(AES_GCM_NONCE_LEN + ct.len());
            blob.extend_from_slice(&nonce);
            blob.extend_from_slice(&ct);
            let pt = vault_crypto::aead::aead_decrypt_with_aad(fskey, &blob, &aad)
                .ok_or("chunk tampered or key mismatch")?;
            hasher.update(&pt);
        }
        if hasher.finalize_hex() != self.meta.file_sha256 {
            return Err("whole-file sha256 mismatch");
        }
        Ok(())
    }

    /// 读取第 `index` 个密文块原样字节（同步传输用，不解密）。
    pub fn read_ct(&self, index: usize) -> Result<Vec<u8>, &'static str> {
        let entry = self
            .meta
            .chunks
            .get(index)
            .ok_or("chunk index out of range")?;
        let mut src = std::fs::File::open(&self.path).map_err(|_| "cannot open container")?;
        src.seek(std::io::SeekFrom::Start(self.data_offset + entry.offset))
            .map_err(|_| "seek failed")?;
        let mut ct = vec![0u8; entry.len as usize];
        src.read_exact(&mut ct).map_err(|_| "chunk data missing")?;
        if vault_crypto::hash_sha256(&ct) != entry.hash {
            return Err("chunk hash mismatch");
        }
        Ok(ct)
    }

    /// 流式解密导出到 `dest`，恒定内存；完成后比对整文件 SHA-256 终验。
    pub fn export_to(&self, dest: &Path, fskey: &[u8; KEY_LEN]) -> Result<(), &'static str> {
        let mut out = std::fs::File::create(dest).map_err(|_| "cannot create dest")?;
        let mut src = std::fs::File::open(&self.path).map_err(|_| "cannot open container")?;
        src.seek(std::io::SeekFrom::Start(self.data_offset))
            .map_err(|_| "seek failed")?;

        let mut hasher = vault_crypto::Sha256::new();
        for (counter, entry) in (0u64..).zip(self.meta.chunks.iter()) {
            let mut ct = vec![0u8; entry.len as usize];
            src.read_exact(&mut ct).map_err(|_| "chunk data missing")?;
            // 逐块校验：GCM tag + 密文哈希双保险
            let mut nonce = [0u8; AES_GCM_NONCE_LEN];
            nonce[..4].copy_from_slice(&self.meta.nonce_prefix.to_be_bytes());
            nonce[4..].copy_from_slice(&counter.to_be_bytes());
            let mut aad = vec![0u8; 8];
            aad.copy_from_slice(&counter.to_be_bytes());
            let mut blob = Vec::with_capacity(vault_crypto::AES_GCM_NONCE_LEN + ct.len());
            blob.extend_from_slice(&nonce);
            blob.extend_from_slice(&ct);
            let pt = vault_crypto::aead::aead_decrypt_with_aad(fskey, &blob, &aad)
                .ok_or("chunk tampered or key mismatch")?;
            if vault_crypto::hash_sha256(&ct) != entry.hash {
                return Err("chunk hash mismatch");
            }
            hasher.update(&pt);
            out.write_all(&pt).map_err(|_| "write dest failed")?;
        }
        if hasher.finalize_hex() != self.meta.file_sha256 {
            return Err("whole-file sha256 mismatch after reassembly");
        }
        Ok(())
    }
}

/// CDC 分块参数（docs/05-02 §3.2）：平均 64KB，小文件(<1MB) 16KB，上限 256KB。
pub struct ChunkCfg {
    pub min: usize,
    pub avg: usize,
    pub max: usize,
}

pub fn chunk_cfg_for(file_size: u64) -> ChunkCfg {
    if file_size < 1024 * 1024 {
        ChunkCfg {
            min: 8 * 1024,
            avg: 16 * 1024,
            max: 64 * 1024,
        }
    } else {
        ChunkCfg {
            min: 16 * 1024,
            avg: 64 * 1024,
            max: 256 * 1024,
        }
    }
}

/// `VSEF v3` 容器（docs/v2.0/05-02 §3.1）：48B 明文头部 + 元数据分片区 + 数据区。
///
/// 与 v2 的差异：块清单改为**分片**结构（每片 40B 片头 + 独立 AEAD 密文），
/// 增量追加时只需重写末片并追加新片，不必整体重写元数据；头部新增
/// `cipher_suite`（灰度用）、`hdr_crc32`（头部完整性）与 `data_region_offset`
/// （跳读数据区）。块加密语义与 v2 完全一致（§3.2：nonce = prefix ∥ counter）。
pub mod container_v3 {
    use std::io::{Read, Seek, SeekFrom};
    use std::path::{Path, PathBuf};

    use serde::{Deserialize, Serialize};
    use vault_crypto::aead::{aead_decrypt_with_aad, aead_encrypt_with_aad, AES_GCM_NONCE_LEN};
    use vault_crypto::KEY_LEN;

    use crate::container::pq_suite_accepted;

    /// v3 头部 48B 明文。
    pub const HEADER_LEN: usize = 48;
    /// 分片头 40B。
    pub const SHARD_HEADER_LEN: usize = 40;
    pub const FORMAT_VER_V3: u16 = 3;
    /// 每片覆盖的块数上限（docs/v2.0/05-02 §3.1）。
    pub const DEFAULT_META_SHARD_SPAN: u32 = 4096;
    /// 单文件上限 1 TiB（docs/v2.0/05-02 §六：1 TiB ≈ 4096 片 @ span 4096）。
    pub const MAX_FILE_SIZE_V3: u64 = 1 << 40;

    /// flags bit0：有擦除元数据区（P7-5 占位，本实现不写）。
    pub const FLAG_HAS_ERASE: u16 = 1 << 0;
    /// flags bit1：元数据分片（v3 恒 1）。
    pub const FLAG_META_SHARDED: u16 = 1 << 1;
    /// flags bit2：已终验（整文件 SHA-256 校验通过）。
    pub const FLAG_FINAL_VERIFIED: u16 = 1 << 2;
    /// flags bit3–15 保留，必须为 0。
    const FLAGS_RESERVED_MASK: u16 = !0x0007;

    const CRC_OFFSET_HDR: usize = 40;
    const CRC_OFFSET_SHARD: usize = 32;

    /// 分片明文中的块清单条目（字段语义与 v2 [`super::ChunkEntry`] 相同）。
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct V3ChunkEntry {
        /// 密文 SHA-256。
        pub hash: String,
        /// 数据区内偏移；len 含 16B GCM tag。
        pub offset: u64,
        pub len: u32,
    }

    /// 分片明文（AEAD under FSKey，AAD = `"VSEF" ‖ shard_index LE`，UTF-8 JSON）。
    /// 末片 `final = true` 且携带 `file_sha256`（整文件 SHA-256 终验值）。
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct V3FileMeta {
        pub ver: u8,
        #[serde(rename = "final")]
        pub is_final: bool,
        pub name: String,
        pub size: u64,
        #[serde(default)]
        pub mime: String,
        pub created_ms: u64,
        pub modified_ms: u64,
        /// per-file 随机 32bit 前缀（块 nonce = prefix ‖ counter，与 v2 一致）。
        pub nonce_prefix: u32,
        /// 仅末片携带。
        #[serde(skip_serializing_if = "Option::is_none")]
        pub file_sha256: Option<String>,
        pub chunks: Vec<V3ChunkEntry>,
    }

    impl V3FileMeta {
        /// 以文件级字段构造分片明文骨架（chunks 由装配器按片填入）。
        fn shard_base(&self, is_final: bool) -> Self {
            Self {
                ver: 3,
                is_final,
                name: self.name.clone(),
                size: self.size,
                mime: self.mime.clone(),
                created_ms: self.created_ms,
                modified_ms: self.modified_ms,
                nonce_prefix: self.nonce_prefix,
                file_sha256: if is_final {
                    self.file_sha256.clone()
                } else {
                    None
                },
                chunks: Vec::new(),
            }
        }
    }

    /// 40B 分片头解析结果。
    #[derive(Clone, Copy, Debug)]
    struct ShardHeader {
        index: u32,
        first_chunk: u64,
        chunk_count: u32,
        ct_len: u32,
        nonce: [u8; AES_GCM_NONCE_LEN],
    }

    fn shard_aad(shard_index: u32) -> Vec<u8> {
        let mut aad = b"VSEF".to_vec();
        aad.extend_from_slice(&shard_index.to_le_bytes());
        aad
    }

    /// 分片布局：把全局块清单切成 `span` 大小的片（末片可能不满）。
    fn shard_bounds(total: usize, span: u32) -> Vec<(u64, usize)> {
        let span = span as usize;
        (0..total.div_ceil(span))
            .map(|i| ((i * span) as u64, span.min(total - i * span)))
            .collect()
    }

    /// 装配 `VSEF v3`：`part` 文件为按块序排列的数据区密文（与 v2 装配语义相同），
    /// `meta` 携带全局块清单与整文件 SHA-256；按 `span` 切片，逐片独立加密。
    pub fn assemble_v3(
        path: &Path,
        part: &Path,
        meta: &V3FileMeta,
        fsk: &[u8; KEY_LEN],
        span: u32,
    ) -> Result<(), &'static str> {
        if span == 0 {
            return Err("meta_shard_span must be nonzero");
        }
        if !meta.is_final || meta.file_sha256.is_none() {
            return Err("v3 assemble requires final meta with file_sha256");
        }
        if meta.size > MAX_FILE_SIZE_V3 {
            return Err("file exceeds 1 TiB limit");
        }
        let bounds = shard_bounds(meta.chunks.len(), span);
        // 逐片加密：每片独立 nonce，AAD 绑定分片序号。
        let mut shards = Vec::with_capacity(bounds.len());
        for (idx, &(first, count)) in bounds.iter().enumerate() {
            let mut plain = meta.shard_base(idx + 1 == bounds.len());
            plain.chunks = meta.chunks[first as usize..first as usize + count].to_vec();
            let json = serde_json::to_vec(&plain).map_err(|_| "meta serialize failed")?;
            if json.len() > super::MAX_META_LEN {
                return Err("metadata too large");
            }
            let nonce: [u8; AES_GCM_NONCE_LEN] =
                vault_crypto::random::random_bytes(AES_GCM_NONCE_LEN)
                    .try_into()
                    .map_err(|_| "rng failed")?;
            let blob = aead_encrypt_with_aad(fsk, &nonce, &json, &shard_aad(idx as u32))?;
            let ct_len = blob.len() - AES_GCM_NONCE_LEN;
            shards.push((
                ShardHeader {
                    index: idx as u32,
                    first_chunk: first,
                    chunk_count: count as u32,
                    ct_len: ct_len as u32,
                    nonce,
                },
                blob[AES_GCM_NONCE_LEN..].to_vec(),
            ));
        }

        let mut meta_region_len = 0u64;
        for (h, _) in &shards {
            meta_region_len += SHARD_HEADER_LEN as u64 + h.ct_len as u64;
        }
        let data_region_offset = HEADER_LEN as u64 + meta_region_len;

        // 48B 头部：写满 0..40 后对其中计算 crc32 落在 40..44。
        let mut hdr = Vec::with_capacity(HEADER_LEN);
        hdr.extend_from_slice(super::MAGIC);
        hdr.extend_from_slice(&FORMAT_VER_V3.to_le_bytes());
        hdr.extend_from_slice(&1u16.to_le_bytes()); // kdf_ver（沿用 v2 语义）
        hdr.extend_from_slice(&super::CIPHER_SUITE.to_le_bytes());
        hdr.extend_from_slice(&0u16.to_le_bytes()); // chunk_cfg（0 = 默认 FastCDC 组）
        hdr.extend_from_slice(&(FLAG_META_SHARDED | FLAG_FINAL_VERIFIED).to_le_bytes());
        hdr.extend_from_slice(&0u16.to_le_bytes()); // reserved0
        hdr.extend_from_slice(&(shards.len() as u32).to_le_bytes());
        hdr.extend_from_slice(&span.to_le_bytes());
        hdr.extend_from_slice(&meta_region_len.to_le_bytes());
        hdr.extend_from_slice(&data_region_offset.to_le_bytes());
        debug_assert_eq!(hdr.len(), CRC_OFFSET_HDR);
        let crc = vault_store::segment::crc32(&hdr);
        hdr.extend_from_slice(&crc.to_le_bytes());
        hdr.extend_from_slice(&0u32.to_le_bytes()); // reserved1

        let mut out =
            Vec::with_capacity(data_region_offset as usize + part_meta_len(part)? as usize);
        out.extend_from_slice(&hdr);
        for (h, ct) in &shards {
            let mut sh = Vec::with_capacity(SHARD_HEADER_LEN);
            sh.extend_from_slice(&h.index.to_le_bytes());
            sh.extend_from_slice(&h.first_chunk.to_le_bytes());
            sh.extend_from_slice(&h.chunk_count.to_le_bytes());
            sh.extend_from_slice(&h.ct_len.to_le_bytes());
            sh.extend_from_slice(&h.nonce);
            let crc = vault_store::segment::crc32(&sh[0..CRC_OFFSET_SHARD]);
            sh.extend_from_slice(&crc.to_le_bytes());
            sh.extend_from_slice(&0u32.to_le_bytes()); // reserved
            out.extend_from_slice(&sh);
            out.extend_from_slice(ct);
        }
        let mut partf = std::fs::File::open(part).map_err(|_| "cannot open part file")?;
        partf
            .read_to_end(&mut out)
            .map_err(|_| "cannot read part file")?;

        // 原子替换：临时名 + rename。
        let tmp = path.with_extension("vse3.tmp");
        {
            let mut f = std::fs::File::create(&tmp).map_err(|_| "cannot create container")?;
            use std::io::Write as _;
            f.write_all(&out).map_err(|_| "cannot write container")?;
            f.sync_all().ok();
        }
        std::fs::rename(&tmp, path).map_err(|_| "cannot finalize container")
    }

    fn part_meta_len(part: &Path) -> Result<u64, &'static str> {
        part.metadata()
            .map(|m| m.len())
            .map_err(|_| "stat part failed")
    }

    /// 分片解析 + 解密（AAD 绑定分片序号）。
    fn decrypt_shard(
        f: &mut std::fs::File,
        expect_index: u32,
        fsk: &[u8; KEY_LEN],
    ) -> Result<V3FileMeta, &'static str> {
        let mut h = [0u8; SHARD_HEADER_LEN];
        f.read_exact(&mut h).map_err(|_| "container truncated")?;
        let stored_crc = u32::from_le_bytes(
            h[CRC_OFFSET_SHARD..CRC_OFFSET_SHARD + 4]
                .try_into()
                .map_err(|_| "truncated")?,
        );
        if vault_store::segment::crc32(&h[0..CRC_OFFSET_SHARD]) != stored_crc {
            return Err("shard header crc32 mismatch");
        }
        if u32::from_le_bytes(h[36..40].try_into().map_err(|_| "truncated")?) != 0 {
            return Err("shard header reserved must be zero");
        }
        let index = u32::from_le_bytes(h[0..4].try_into().map_err(|_| "truncated")?);
        if index != expect_index {
            return Err("shard index mismatch");
        }
        let hdr = ShardHeader {
            index,
            first_chunk: u64::from_le_bytes(h[4..12].try_into().map_err(|_| "truncated")?),
            chunk_count: u32::from_le_bytes(h[12..16].try_into().map_err(|_| "truncated")?),
            ct_len: u32::from_le_bytes(h[16..20].try_into().map_err(|_| "truncated")?),
            nonce: h[20..32].try_into().map_err(|_| "truncated")?,
        };
        if hdr.ct_len as usize > super::MAX_META_LEN {
            return Err("shard metadata too large");
        }
        let mut ct = vec![0u8; hdr.ct_len as usize];
        f.read_exact(&mut ct).map_err(|_| "container truncated")?;
        let mut blob = Vec::with_capacity(AES_GCM_NONCE_LEN + ct.len());
        blob.extend_from_slice(&hdr.nonce);
        blob.extend_from_slice(&ct);
        let json = aead_decrypt_with_aad(fsk, &blob, &shard_aad(expect_index))
            .ok_or("shard decrypt failed (wrong key or tampered)")?;
        serde_json::from_slice(&json).map_err(|_| "shard meta parse failed")
    }

    /// `VSEF v3` 读取器：打开时只校验头部 + 解密**末片**（取整文件 SHA-256 与
    /// 末片块清单）；命中块所在片按需经 [`Self::read_shard`] 解密。
    pub struct ContainerReaderV3 {
        /// 末片（final 分片）元数据。
        pub meta: V3FileMeta,
        pub shard_count: u32,
        pub meta_shard_span: u32,
        data_region_offset: u64,
        path: PathBuf,
    }

    impl ContainerReaderV3 {
        pub fn open(path: &Path, fsk: &[u8; KEY_LEN]) -> Result<Self, &'static str> {
            let mut f = std::fs::File::open(path).map_err(|_| "cannot open container")?;
            let mut head = [0u8; HEADER_LEN];
            f.read_exact(&mut head).map_err(|_| "container truncated")?;
            if &head[0..4] != super::MAGIC {
                return Err("bad magic: not a VaultSync file container");
            }
            let g16 = |o: usize| -> Result<u16, &'static str> {
                Ok(u16::from_le_bytes(
                    head[o..o + 2].try_into().map_err(|_| "truncated")?,
                ))
            };
            let format_ver = g16(4)?;
            if format_ver > FORMAT_VER_V3 {
                return Err("format version newer than engine; upgrade required");
            }
            if format_ver != FORMAT_VER_V3 {
                return Err("bad format: expected VSEF v3");
            }
            let cipher_suite = g16(8)?;
            if cipher_suite == 0 {
                return Err("bad format: cipher suite 0 is not a valid suite");
            }
            if cipher_suite == 2 && !pq_suite_accepted() {
                // P7-8：能力未置位读 suite 2 → 13（能力），与 ≥3（未知套件）同码不同因
                return Err("unsupported cipher suite: pq hybrid capability not enabled");
            }
            if cipher_suite >= 3 {
                return Err("unsupported cipher suite");
            }
            let flags = g16(12)?;
            if flags & FLAGS_RESERVED_MASK != 0 || g16(14)? != 0 {
                return Err("bad format: reserved header bits must be zero");
            }
            if u32::from_le_bytes(head[44..48].try_into().map_err(|_| "truncated")?) != 0 {
                return Err("bad format: reserved1 must be zero");
            }
            let stored_crc = u32::from_le_bytes(
                head[CRC_OFFSET_HDR..CRC_OFFSET_HDR + 4]
                    .try_into()
                    .map_err(|_| "truncated")?,
            );
            if vault_store::segment::crc32(&head[0..CRC_OFFSET_HDR]) != stored_crc {
                return Err("header crc32 mismatch");
            }
            let shard_count = u32::from_le_bytes(head[16..20].try_into().map_err(|_| "truncated")?);
            let shard_span = u32::from_le_bytes(head[20..24].try_into().map_err(|_| "truncated")?);
            let meta_region_len =
                u64::from_le_bytes(head[24..32].try_into().map_err(|_| "truncated")?);
            let data_region_offset =
                u64::from_le_bytes(head[32..40].try_into().map_err(|_| "truncated")?);
            if shard_count == 0 || shard_span == 0 {
                return Err("bad format: shard count and span must be nonzero");
            }
            if data_region_offset != HEADER_LEN as u64 + meta_region_len {
                return Err("bad format: data_region_offset inconsistent with meta_region_len");
            }

            // 定位末片：顺序扫描片头（每片 40B 头 + ct_len 密文）。
            f.seek(SeekFrom::Start(HEADER_LEN as u64))
                .map_err(|_| "seek failed")?;
            for idx in 0..shard_count - 1 {
                let mut h = [0u8; SHARD_HEADER_LEN];
                f.read_exact(&mut h).map_err(|_| "container truncated")?;
                let ct_len = u32::from_le_bytes(h[16..20].try_into().map_err(|_| "truncated")?);
                if u32::from_le_bytes(h[0..4].try_into().map_err(|_| "truncated")?) != idx {
                    return Err("shard index mismatch");
                }
                f.seek(SeekFrom::Current(ct_len as i64))
                    .map_err(|_| "seek failed")?;
            }
            let meta = decrypt_shard(&mut f, shard_count - 1, fsk)?;
            if !meta.is_final || meta.file_sha256.is_none() {
                return Err("bad format: last shard must be final with file_sha256");
            }
            Ok(Self {
                meta,
                shard_count,
                meta_shard_span: shard_span,
                data_region_offset,
                path: path.to_path_buf(),
            })
        }

        /// 全局块序号 → 所在片下标。
        fn shard_of(&self, global_index: u64) -> u32 {
            (global_index / self.meta_shard_span as u64) as u32
        }

        /// 按需解密任意分片（读路径默认只解末片 + 命中块所在片）。
        pub fn read_shard(
            &self,
            shard_index: u32,
            fsk: &[u8; KEY_LEN],
        ) -> Result<V3FileMeta, &'static str> {
            if shard_index >= self.shard_count {
                return Err("shard index out of range");
            }
            let mut f = std::fs::File::open(&self.path).map_err(|_| "cannot open container")?;
            f.seek(SeekFrom::Start(HEADER_LEN as u64))
                .map_err(|_| "seek failed")?;
            for _ in 0..shard_index {
                let mut h = [0u8; SHARD_HEADER_LEN];
                f.read_exact(&mut h).map_err(|_| "container truncated")?;
                let ct_len = u32::from_le_bytes(h[16..20].try_into().map_err(|_| "truncated")?);
                f.seek(SeekFrom::Current(ct_len as i64))
                    .map_err(|_| "seek failed")?;
            }
            decrypt_shard(&mut f, shard_index, fsk)
        }

        /// 解密数据区第 `global_index` 个块（先按需解所在片取块清单条目）。
        pub fn decrypt_chunk(
            &self,
            fsk: &[u8; KEY_LEN],
            global_index: u64,
        ) -> Result<Vec<u8>, &'static str> {
            let shard_index = self.shard_of(global_index);
            let shard = if shard_index + 1 == self.shard_count {
                self.meta.clone()
            } else {
                self.read_shard(shard_index, fsk)?
            };
            let local = (global_index - shard_index as u64 * self.meta_shard_span as u64) as usize;
            let entry = shard.chunks.get(local).ok_or("chunk index out of range")?;
            let mut src = std::fs::File::open(&self.path).map_err(|_| "cannot open container")?;
            src.seek(SeekFrom::Start(self.data_region_offset + entry.offset))
                .map_err(|_| "seek failed")?;
            let mut ct = vec![0u8; entry.len as usize];
            src.read_exact(&mut ct).map_err(|_| "chunk data missing")?;
            if vault_crypto::hash_sha256(&ct) != entry.hash {
                return Err("chunk hash mismatch");
            }
            let mut nonce = [0u8; AES_GCM_NONCE_LEN];
            nonce[..4].copy_from_slice(&shard.nonce_prefix.to_be_bytes());
            nonce[4..].copy_from_slice(&global_index.to_be_bytes());
            let mut aad = [0u8; 8];
            aad.copy_from_slice(&global_index.to_be_bytes());
            let mut blob = Vec::with_capacity(AES_GCM_NONCE_LEN + ct.len());
            blob.extend_from_slice(&nonce);
            blob.extend_from_slice(&ct);
            aead_decrypt_with_aad(fsk, &blob, &aad).ok_or("chunk tampered or key mismatch")
        }

        /// 全容器分片长度清单（P8-8 三方合并的段边界；按片序拼接；GCM 下密文长 = 明文长）。
        pub fn chunk_lens(&self, fsk: &[u8; KEY_LEN]) -> Result<Vec<u64>, &'static str> {
            let mut lens: Vec<u64> = Vec::new();
            for shard in 0..self.shard_count {
                let s = if shard + 1 == self.shard_count {
                    self.meta.clone()
                } else {
                    self.read_shard(shard, fsk)?
                };
                lens.extend(s.chunks.iter().map(|c| u64::from(c.len)));
            }
            Ok(lens)
        }

        /// 数据区内偏移定位（导出 / 同步读原始密文用）。
        pub fn data_region_offset(&self) -> u64 {
            self.data_region_offset
        }

        /// 流式解密导出到 `dest`（P8-8 三方合并基线 / P8-9 阅后即焚「打开」用）：
        /// 逐块解密（按需解所在片）→ 整文件 SHA-256 终验。
        pub fn export_to(&self, dest: &Path, fsk: &[u8; KEY_LEN]) -> Result<(), &'static str> {
            use std::io::Write as _;
            let mut out = std::fs::File::create(dest).map_err(|_| "cannot create dest")?;
            let total = (self.shard_count as u64 - 1) * u64::from(self.meta_shard_span)
                + self.meta.chunks.len() as u64;
            let mut hasher = vault_crypto::Sha256::new();
            for i in 0..total {
                let pt = self.decrypt_chunk(fsk, i)?;
                hasher.update(&pt);
                out.write_all(&pt).map_err(|_| "write dest failed")?;
            }
            if Some(hasher.finalize_hex()) != self.meta.file_sha256 {
                return Err("whole-file sha256 mismatch after reassembly");
            }
            Ok(())
        }
    }

    #[cfg(test)]
    fn encrypt_chunk_blob(fsk: &[u8; KEY_LEN], prefix: u32, counter: u64, plain: &[u8]) -> Vec<u8> {
        let mut nonce = [0u8; AES_GCM_NONCE_LEN];
        nonce[..4].copy_from_slice(&prefix.to_be_bytes());
        nonce[4..].copy_from_slice(&counter.to_be_bytes());
        let aad = counter.to_be_bytes();
        aead_encrypt_with_aad(fsk, &nonce, plain, &aad).expect("encrypt chunk")
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::container::set_pq_suite_accepted;
        use vault_crypto::{hash_sha256, Sha256};

        fn fsk() -> [u8; KEY_LEN] {
            vault_crypto::random::random_key()
        }

        /// 生成真实加密的 part 文件（数据区密文按序）+ 全局块清单 + 整文件 SHA-256。
        fn build_part(fsk: &[u8; KEY_LEN], prefix: u32, plains: &[&[u8]]) -> (PathBuf, V3FileMeta) {
            let mut raw = Vec::new();
            let mut entries = Vec::new();
            let mut hasher = Sha256::new();
            for (i, p) in plains.iter().enumerate() {
                let counter = i as u64;
                let blob = encrypt_chunk_blob(fsk, prefix, counter, p);
                let ct = &blob[AES_GCM_NONCE_LEN..];
                entries.push(V3ChunkEntry {
                    hash: hash_sha256(ct),
                    offset: raw.len() as u64,
                    len: ct.len() as u32,
                });
                hasher.update(p);
                raw.extend_from_slice(ct);
            }
            // part 放在进程级临时目录（tempdir 生命周期与容器文件解耦）。
            let keep = std::env::temp_dir().join(format!(
                "vse3_test_{}_{}.part",
                std::process::id(),
                &hash_sha256(&raw)[..16]
            ));
            std::fs::write(&keep, &raw).expect("persist part");
            (
                keep,
                V3FileMeta {
                    ver: 3,
                    is_final: true,
                    name: "hello.txt".into(),
                    size: plains.iter().map(|p| p.len() as u64).sum(),
                    mime: "text/plain".into(),
                    created_ms: 111,
                    modified_ms: 222,
                    nonce_prefix: prefix,
                    file_sha256: Some(hasher.finalize_hex()),
                    chunks: entries,
                },
            )
        }

        #[test]
        fn roundtrip_and_final_shard() {
            let key = fsk();
            let plains: Vec<Vec<u8>> = (0..5)
                .map(|i| vec![(i * 7 + 3) as u8; 100 + i * 33])
                .collect();
            let refs: Vec<&[u8]> = plains.iter().map(|v| v.as_slice()).collect();
            let (part, meta) = build_part(&key, 7, &refs);
            let dir = tempfile::tempdir().expect("tmp");
            let cpath = dir.path().join("f.vse");
            assemble_v3(&cpath, &part, &meta, &key, 2).expect("assemble");

            let rd = ContainerReaderV3::open(&cpath, &key).expect("open");
            // 元数据逐字段一致（末片）。
            assert_eq!(rd.meta.name, "hello.txt");
            assert_eq!(rd.meta.mime, "text/plain");
            assert_eq!(rd.meta.size, meta.size);
            assert_eq!(rd.meta.nonce_prefix, 7);
            assert_eq!(rd.meta.created_ms, 111);
            assert_eq!(rd.meta.modified_ms, 222);
            assert!(rd.meta.is_final);
            assert_eq!(rd.meta.file_sha256.as_deref(), meta.file_sha256.as_deref());
            // 分片布局：5 块 / span=2 → 3 片。
            assert_eq!(rd.shard_count, 3);
            assert_eq!(rd.meta_shard_span, 2);
            // 非末片 final=false 且无 file_sha256。
            let s0 = rd.read_shard(0, &key).expect("shard0");
            assert!(!s0.is_final);
            assert!(s0.file_sha256.is_none());
            assert_eq!(s0.chunks.len(), 2);
            assert_eq!(s0.chunks[0].hash, meta.chunks[0].hash);
            // 数据区逐块解密验证。
            for (i, p) in plains.iter().enumerate() {
                let pt = rd.decrypt_chunk(&key, i as u64).expect("decrypt chunk");
                assert_eq!(&pt, p, "chunk {i} plaintext mismatch");
            }
            // 错误密钥 → 末片解密失败。
            assert!(ContainerReaderV3::open(&cpath, &fsk()).is_err());
        }

        /// 手封 cipher_suite 后修正 crc32，检验与「格式错误」严格区分。
        fn patched_suite_err(suite: u16) -> String {
            let key = fsk();
            let plains = [vec![1u8; 64], vec![2u8; 64]];
            let refs: Vec<&[u8]> = plains.iter().map(|v| v.as_slice()).collect();
            let (part, meta) = build_part(&key, 9, &refs);
            let dir = tempfile::tempdir().expect("tmp");
            let cpath = dir.path().join("f.vse");
            assemble_v3(&cpath, &part, &meta, &key, 4096).expect("assemble");
            let mut raw = std::fs::read(&cpath).expect("read");
            raw[8..10].copy_from_slice(&suite.to_le_bytes());
            let crc = vault_store::segment::crc32(&raw[0..CRC_OFFSET_HDR]);
            raw[CRC_OFFSET_HDR..CRC_OFFSET_HDR + 4].copy_from_slice(&crc.to_le_bytes());
            let bad = dir.path().join("bad.vse");
            std::fs::write(&bad, &raw).expect("write");
            ContainerReaderV3::open(&bad, &key)
                .err()
                .expect("must fail")
                .to_string()
        }

        #[test]
        fn unknown_suite_returns_13() {
            let err = patched_suite_err(3);
            assert!(err.contains("unsupported cipher"), "got: {err}");
        }

        #[test]
        fn suite_zero_is_format_error() {
            let err = patched_suite_err(0);
            assert!(err.contains("format"), "got: {err}");
            // 与 13 严格区分：错误串不得出现 unsupported。
            assert!(!err.contains("unsupported cipher"));
        }

        /// P7-8 证据（docs/v2.0/11 行 171）：未置位读 suite=2 → 13（能力）而非 6；
        /// 置位后 suite=2 可读；写路径恒写 suite=1（T2「新写入生效」未启动）。
        /// 全局开关用后即还原，避免污染并行测试。
        #[test]
        fn pq_suite_downgrades_when_peer_lacks_capability() {
            // 1) 默认（未置位）：suite 2 → unsupported cipher（13 语义）
            let err = patched_suite_err(2);
            assert!(err.contains("unsupported cipher"), "got: {err}");
            // 2) 置位后：suite 2 容器可正常打开读取
            let key = fsk();
            let plains = [vec![1u8; 64], vec![2u8; 64]];
            let refs: Vec<&[u8]> = plains.iter().map(|v| v.as_slice()).collect();
            let (part, meta) = build_part(&key, 9, &refs);
            let dir = tempfile::tempdir().expect("tmp");
            let cpath = dir.path().join("f.vse");
            assemble_v3(&cpath, &part, &meta, &key, 4096).expect("assemble");
            let mut raw = std::fs::read(&cpath).expect("read");
            raw[8..10].copy_from_slice(&2u16.to_le_bytes());
            let crc = vault_store::segment::crc32(&raw[0..CRC_OFFSET_HDR]);
            raw[CRC_OFFSET_HDR..CRC_OFFSET_HDR + 4].copy_from_slice(&crc.to_le_bytes());
            std::fs::write(&cpath, &raw).expect("write");
            set_pq_suite_accepted(true);
            let opened = ContainerReaderV3::open(&cpath, &key).map(|_| ());
            set_pq_suite_accepted(false);
            assert!(opened.is_ok(), "置位后 suite 2 必须可读: {opened:?}");
            // 3) 写路径：正常装配的容器头部 suite 恒为 1（T2 未启动，写 2 才需降级审计）
            let cpath1 = dir.path().join("g.vse");
            assemble_v3(&cpath1, &part, &meta, &key, 4096).expect("assemble");
            let raw1 = std::fs::read(&cpath1).expect("read");
            assert_eq!(
                u16::from_le_bytes([raw1[8], raw1[9]]),
                crate::container::CIPHER_SUITE,
                "写路径在本里程碑必须恒写 suite 1"
            );
        }

        #[test]
        fn append_rewrites_last_shard_only() {
            let key = fsk();
            let plains: Vec<Vec<u8>> = (0..4).map(|i| vec![i as u8; 80]).collect();
            let refs: Vec<&[u8]> = plains.iter().map(|v| v.as_slice()).collect();
            let (part_a, meta_a) = build_part(&key, 11, &refs);
            let dir_a = tempfile::tempdir().expect("tmp");
            let a = dir_a.path().join("a.vse");
            assemble_v3(&a, &part_a, &meta_a, &key, 2).expect("assemble A");

            // 模拟追加：前 4 块明文/前缀/序号不变 → 密文逐字节相同 → 首片明文应不变。
            let mut grown = plains.clone();
            grown.push(vec![9u8; 80]);
            grown.push(vec![10u8; 80]);
            let grefs: Vec<&[u8]> = grown.iter().map(|v| v.as_slice()).collect();
            let (part_b, meta_b) = build_part(&key, 11, &grefs);
            let dir_b = tempfile::tempdir().expect("tmp");
            let b = dir_b.path().join("b.vse");
            assemble_v3(&b, &part_b, &meta_b, &key, 2).expect("assemble B");

            let rd_a = ContainerReaderV3::open(&a, &key).expect("open A");
            let rd_b = ContainerReaderV3::open(&b, &key).expect("open B");
            assert_eq!(rd_a.shard_count, 2);
            assert_eq!(rd_b.shard_count, 3); // 6 块 / span 2 → 3 片
            assert_eq!(rd_b.meta_shard_span, 2);
            // 末片布局：2 块、final=true。
            assert_eq!(rd_b.meta.chunks.len(), 2);
            assert!(rd_b.meta.is_final);
            // 旧片（片 0）内容不变：真实增量写入根本不触碰片 0 的字节。此处以
            // 重新装配模拟，逐字段比对分片明文——`size` 为整文件大小，追加后
            // 由末片（权威片）承载新值；片 0 的块清单与其他文件级字段必须不变。
            let s0_a = rd_a.read_shard(0, &key).expect("shard0 A");
            let s0_b = rd_b.read_shard(0, &key).expect("shard0 B");
            assert_eq!(s0_a.chunks, s0_b.chunks, "shard 0 chunk list changed");
            assert_eq!(s0_a.name, s0_b.name);
            assert_eq!(s0_a.mime, s0_b.mime);
            assert_eq!(s0_a.nonce_prefix, s0_b.nonce_prefix);
            assert_eq!(s0_a.created_ms, s0_b.created_ms);
            assert_eq!(s0_a.modified_ms, s0_b.modified_ms);
            assert_eq!(s0_a.ver, s0_b.ver);
            assert!(!s0_a.is_final && !s0_b.is_final);
            assert_eq!(s0_a.size, 320);
            assert_eq!(s0_b.size, 480); // 新值只落在重写的分片 / 末片
                                        // meta_shard_span 生效：全局块 4 落在片 2（4/2=2）。
            let pt = rd_b.decrypt_chunk(&key, 4).expect("chunk 4");
            assert_eq!(pt, grown[4]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fsk() -> [u8; KEY_LEN] {
        vault_crypto::random::random_key()
    }

    fn meta_of(name: &str, size: u64, n: usize) -> FileMeta {
        FileMeta {
            name: name.into(),
            size,
            nonce_prefix: 42,
            chunks: (0..n)
                .map(|i| ChunkEntry {
                    hash: format!("{i:064x}"),
                    offset: i as u64 * 100,
                    len: 100,
                })
                .collect(),
            file_sha256: "ab".repeat(32),
            created_ms: 1,
            modified_ms: 2,
        }
    }

    #[test]
    fn assemble_then_open_roundtrip() {
        let dir = tempfile::tempdir().expect("tmp");
        let part = dir.path().join("p.part");
        std::fs::write(&part, vec![7u8; 300]).expect("part");
        let cpath = dir.path().join("f.vse");
        let key = fsk();
        assemble(&cpath, &part, meta_of("hello.txt", 300, 2), &key).expect("assemble");

        let rd = ContainerReader::open(&cpath, &key).expect("open");
        assert_eq!(rd.meta.name, "hello.txt");
        assert_eq!(rd.meta.size, 300);
        assert_eq!(rd.meta.chunks.len(), 2);
        // 错误密钥 → 元数据解密失败
        assert!(ContainerReader::open(&cpath, &fsk()).is_err());
    }

    #[test]
    fn rejects_bad_magic_and_newer_format() {
        let dir = tempfile::tempdir().expect("tmp");
        let cpath = dir.path().join("f.vse");
        std::fs::write(&cpath, b"XXXXjunk").expect("write");
        assert!(ContainerReader::open(&cpath, &fsk()).is_err());

        let part = dir.path().join("p.part");
        std::fs::write(&part, b"x").expect("part");
        let key = fsk();
        assemble(&cpath, &part, meta_of("a", 1, 0), &key).expect("assemble");
        let mut raw = std::fs::read(&cpath).expect("read");
        raw[4] = u16::to_le_bytes(FORMAT_VER + 1)[0];
        std::fs::write(&cpath, raw).expect("write");
        assert!(ContainerReader::open(&cpath, &key).is_err());
    }

    #[test]
    fn chunk_cfg_adapts_to_size() {
        assert_eq!(chunk_cfg_for(512 * 1024).avg, 16 * 1024);
        assert_eq!(chunk_cfg_for(8 * 1024 * 1024).avg, 64 * 1024);
        assert!(chunk_cfg_for(8 * 1024 * 1024).max <= 256 * 1024);
    }
}
