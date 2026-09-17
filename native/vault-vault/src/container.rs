//! 加密文件容器（.vse）：Header + 加密元数据 + 数据区（docs/05-02 §3.3）。
//!
//! ```text
//! Header(明文, docs/07 版本化): magic "VSEF" | format_ver u16 | kdf_ver u16
//!                              | cipher_suite u16 | chunk_cfg u16 | flags u16
//! 加密元数据(AEAD under FSK): 随机 12B nonce 前缀字段 + 名称/大小/时间/nonce_prefix
//!                             /块清单(sha256+偏移+密文长)/整文件 SHA-256
//! 数据区: CDC 块密文顺序排列；块 nonce = prefix(4B BE) ∥ counter(8B BE)（docs/05-02 §3.2）
//! ```
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vault_crypto::aead::{aead_decrypt, aead_encrypt, AES_GCM_NONCE_LEN};
use vault_crypto::KEY_LEN;

pub const MAGIC: &[u8; 4] = b"VSEF";
pub const FORMAT_VER: u16 = 2;
pub const CIPHER_SUITE: u16 = 1;

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
