//! 保险箱引擎编排：导入 / 导出 / 文件夹树 / 擦除 / 阅后即焚（docs/05-02 §四）。
//!
//! 布局：`<vault 文件同目录>/<stem>.data/`
//! ```text
//! index.enc            加密索引（VSIX v2，整体 AEAD）
//! files/<id>.vse       每文件独立密文容器（VSEF v2）
//! ```
//! 密钥：FSK(folder)=HKDF(MK,"fsk/<id>")；FSKey=HKDF(FSK,"fskey/<id>")（docs/05-02 §3.1）。
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use zeroize::Zeroizing;

use vault_crypto::kdf::hkdf_sha256_derive;
use vault_crypto::{random_bytes, random_key, KEY_LEN};

use crate::container::{assemble, chunk_cfg_for, ContainerReader, FileMeta};
use crate::index::{tokenize, VaultIndex};

pub struct Vault {
    data_dir: PathBuf,
    index_path: PathBuf,
    index: VaultIndex,
    mk: Zeroizing<[u8; KEY_LEN]>,
    /// 本机向量时钟设备位（P3 同步；由上层在解锁后设置）。
    device_id: String,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Vault {
    /// 从 vault 头文件定位数据目录并加载（或初始化）索引。
    pub fn open(vault_path: &Path, mk: &[u8; KEY_LEN]) -> Result<Self, &'static str> {
        let stem = vault_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("invalid vault file name")?;
        let parent = vault_path.parent().unwrap_or(Path::new("."));
        let data_dir = parent.join(format!("{stem}.data"));
        std::fs::create_dir_all(data_dir.join("files")).map_err(|_| "cannot create data dir")?;
        let index_path = data_dir.join("index.enc");
        let index_key = Self::index_key(mk);
        let search_key = Self::derive_key(mk, "search");
        let index = match VaultIndex::load(&index_path, &index_key, search_key) {
            Ok(ix) => ix,
            Err("cannot read index") => VaultIndex::new(search_key),
            Err(e) => return Err(e),
        };
        Ok(Self {
            data_dir,
            index_path,
            index,
            mk: Zeroizing::new(*mk),
            device_id: String::new(),
        })
    }

    /// 设置向量时钟本机设备位（同步引擎初始化后调用）。
    pub fn set_device_id(&mut self, device_id: &str) {
        self.device_id = device_id.to_string();
    }

    fn index_key(mk: &[u8; KEY_LEN]) -> [u8; KEY_LEN] {
        Self::derive_key(mk, "vault-index")
    }

    fn derive_key(mk: &[u8; KEY_LEN], info: &str) -> [u8; KEY_LEN] {
        *hkdf_sha256_derive(mk, info.as_bytes())
    }

    fn fsk(&self, folder_id: u64) -> [u8; KEY_LEN] {
        Self::derive_key(&self.mk, &format!("fsk/{folder_id}"))
    }

    /// 文件密钥：同步文件用索引内 FSKey 覆盖，本机文件按 MK 派生（docs/05-02 §3.1）。
    fn fskey(&self, folder_id: u64, file_id: u64) -> Result<[u8; KEY_LEN], &'static str> {
        if let Some(hex) = self.index.file_fskey(file_id)? {
            let k = vault_crypto::hex_decode(&hex).ok_or("bad fskey hex")?;
            let arr: [u8; KEY_LEN] = k.as_slice().try_into().map_err(|_| "bad fskey len")?;
            return Ok(arr);
        }
        let fsk = self.fsk(folder_id);
        Ok(*hkdf_sha256_derive(
            &fsk,
            format!("fskey/{file_id}").as_bytes(),
        ))
    }

    fn save_index(&self) -> Result<(), &'static str> {
        self.index
            .save(&self.index_path, &Self::index_key(&self.mk))
    }

    fn container_path(&self, file_id: u64) -> PathBuf {
        self.data_dir.join("files").join(format!("{file_id}.vse"))
    }

    /// 流式加密导入（F-01，恒定内存）：CDC 分块 → FSKey + 计数器 nonce 逐块 GCM。
    pub fn import_file(&mut self, src: &Path, folder_id: u64) -> Result<u64, &'static str> {
        let mut f = std::fs::File::open(src).map_err(|_| "cannot open source file")?;
        let size = f.metadata().map_err(|_| "cannot stat source")?.len();
        let name = src
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("invalid source file name")?
            .to_string();
        let modified_ms = f
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        // 先占 id：FSKey 派生依赖 file_id
        let file_id = self.index.alloc_id();
        let fskey = {
            let fsk = self.fsk(folder_id);
            Zeroizing::new(*hkdf_sha256_derive(
                &fsk,
                format!("fskey/{file_id}").as_bytes(),
            ))
        };

        let nonce_prefix = u32::from_le_bytes(
            random_bytes(4)
                .as_slice()
                .try_into()
                .map_err(|_| "nonce len")?,
        );
        let cfg = chunk_cfg_for(size);

        let part = self.data_dir.join(format!("import-{file_id}.part"));
        let mut partf = std::fs::File::create(&part).map_err(|_| "cannot create part file")?;
        let mut hasher = vault_crypto::Sha256::new();
        let mut chunks: Vec<(String, u64, u32)> = Vec::new();
        let mut offset: u64 = 0;
        let mut counter: u64 = 0;

        if size > 0 {
            let chunker = fastcdc::v2020::StreamCDC::new(&mut f, cfg.min, cfg.avg, cfg.max);
            for chunk in chunker {
                let c = match chunk {
                    Ok(c) => c,
                    Err(_) => return Err("cdc chunk read failed"),
                };
                let plain = &c.data;
                hasher.update(plain);
                let mut nonce = [0u8; vault_crypto::AES_GCM_NONCE_LEN];
                nonce[..4].copy_from_slice(&nonce_prefix.to_be_bytes());
                nonce[4..].copy_from_slice(&counter.to_be_bytes());
                let mut aad = [0u8; 8];
                aad.copy_from_slice(&counter.to_be_bytes());
                // aead_encrypt_with_aad 返回 nonce‖ct‖tag；nonce 已确定性派生，
                // 数据区只落 ct‖tag（docs/05-02 §3.3 数据区语义）
                let blob = vault_crypto::aead::aead_encrypt_with_aad(&fskey, &nonce, plain, &aad)?;
                let ct = &blob[vault_crypto::AES_GCM_NONCE_LEN..];
                partf.write_all(ct).map_err(|_| "write chunk failed")?;
                chunks.push((vault_crypto::hash_sha256(ct), offset, ct.len() as u32));
                offset += ct.len() as u64;
                counter += 1;
            }
        }
        drop(partf);

        // 内容采样令牌：文本型文件取前 1MB（含 NUL 视为二进制跳过）
        let mut tokens = tokenize(&name);
        if size > 0 {
            let mut sample = Vec::new();
            if f.seek(std::io::SeekFrom::Start(0)).is_ok() {
                let _ = Read::by_ref(&mut f)
                    .take(1024 * 1024)
                    .read_to_end(&mut sample);
            }
            let probe = &sample[..sample.len().min(8192)];
            if !probe.contains(&0u8) {
                let text = String::from_utf8_lossy(&sample);
                tokens.extend(crate::index::content_tokens(&text));
            }
        }

        let meta = FileMeta {
            name: name.clone(),
            size,
            nonce_prefix,
            chunks: chunks
                .into_iter()
                .map(|(hash, off, len)| crate::container::ChunkEntry {
                    hash,
                    offset: off,
                    len,
                })
                .collect(),
            file_sha256: hasher.finalize_hex(),
            created_ms: modified_ms,
            modified_ms,
        };

        let cpath = self.container_path(file_id);
        assemble(&cpath, &part, meta, &self.fsk(folder_id))?;
        std::fs::remove_file(&part).ok();

        self.index
            .add_file_with_id(file_id, folder_id, &name, size, &tokens)?;
        self.index.bump_vc(file_id, &self.device_id).ok();
        self.save_index()?;
        Ok(file_id)
    }

    /// 流式解密导出（F-02）：逐块 GCM 校验 + 整文件 SHA-256 终验。
    pub fn export_file(&self, file_id: u64, dest: &Path) -> Result<(), &'static str> {
        let folder = self.index.file_folder(file_id)?;
        let fskey = self.fskey(folder, file_id)?;
        let rd = ContainerReader::open(&self.container_path(file_id), &self.fsk(folder))?;
        rd.export_to(dest, &fskey)
    }

    pub fn mkdir(&mut self, parent: u64, name: &str) -> Result<u64, &'static str> {
        let id = self.index.mkdir(parent, name)?;
        self.save_index()?;
        Ok(id)
    }

    pub fn rename_file(&mut self, file_id: u64, name: &str) -> Result<(), &'static str> {
        self.index.rename_file(file_id, name)?;
        self.save_index()
    }

    pub fn rename_folder(&mut self, folder_id: u64, name: &str) -> Result<(), &'static str> {
        self.index.rename_folder(folder_id, name)?;
        self.save_index()
    }

    pub fn list_children(&self, folder_id: u64) -> Result<String, &'static str> {
        self.index.list_children(folder_id)
    }

    pub fn search(&self, query: &str) -> String {
        self.index.search(query)
    }

    pub fn set_tags(&mut self, file_id: u64, tags: Vec<String>) -> Result<(), &'static str> {
        self.index.set_tags(file_id, tags)?;
        self.save_index()
    }

    /// 安全擦除（F-03，docs/05-02 §4.3）：容器整体删除（密文+密钥材料同灭），
    /// secure=true 时删除前单次覆写（介质兜底；SSD 上为单次覆写语义）。
    pub fn delete_file(&mut self, file_id: u64, secure: bool) -> Result<(), &'static str> {
        // 删除前快取向量时钟并自增本机位作墓碑（使删除在时钟上支配原版本，docs/05-03 §6.2）
        let mut vc = self.index.file_vc(file_id).unwrap_or_default();
        if !self.device_id.is_empty() {
            let next = vc.get(&self.device_id).copied().unwrap_or(0) + 1;
            vc.insert(self.device_id.clone(), next);
        }
        self.index.remove_file(file_id)?;
        self.index.record_tombstone(file_id, vc);
        let cpath = self.container_path(file_id);
        self.save_index()?;
        if secure {
            if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(&cpath) {
                let len = f.metadata().map(|m| m.len()).unwrap_or(0);
                f.seek(std::io::SeekFrom::Start(0)).ok();
                let zeros = vec![0u8; 64 * 1024];
                let mut left = len;
                while left > 0 {
                    let n = zeros.len().min(left as usize);
                    if f.write_all(&zeros[..n]).is_err() {
                        break;
                    }
                    left -= n as u64;
                }
                f.sync_all().ok();
            }
        }
        std::fs::remove_file(&cpath).map_err(|_| "cannot delete container")
    }

    pub fn delete_folder(&mut self, folder_id: u64, secure: bool) -> Result<usize, &'static str> {
        // 递归收集受影响文件，逐个留墓碑（删除语义同步到对端）
        for id in self.index.files_under(folder_id) {
            if let Ok(vc) = self.index.file_vc(id) {
                self.index.record_tombstone(id, vc);
            }
        }
        let ids = self.index.remove_folder(folder_id)?;
        self.save_index()?;
        for id in &ids {
            let cpath = self.container_path(*id);
            if secure {
                if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(&cpath) {
                    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
                    f.seek(std::io::SeekFrom::Start(0)).ok();
                    let zeros = vec![0u8; 64 * 1024];
                    let mut left = len;
                    while left > 0 {
                        let n = zeros.len().min(left as usize);
                        if f.write_all(&zeros[..n]).is_err() {
                            break;
                        }
                        left -= n as u64;
                    }
                    f.sync_all().ok();
                }
            }
            std::fs::remove_file(&cpath).ok();
        }
        Ok(ids.len())
    }

    /// 阅后即焚分享：创建一次性令牌（明文仅返回一次），打开即消耗机会。
    pub fn create_share(
        &mut self,
        file_id: u64,
        ttl_secs: u64,
        max_opens: u32,
    ) -> Result<(u64, String), &'static str> {
        let token = vault_crypto::hex_encode(&random_key());
        let token_hash = self.index.token_hash(&token);
        let id = self
            .index
            .create_share(file_id, token_hash, ttl_secs, max_opens)?;
        self.save_index()?;
        Ok((id, token))
    }

    /// 通过分享导出：校验令牌/有效期/次数 → 解密导出。
    pub fn open_share(
        &mut self,
        share_id: u64,
        token: &str,
        dest: &Path,
    ) -> Result<(), &'static str> {
        let file_id = self.index.consume_share(share_id, token)?;
        self.save_index()?;
        self.export_file(file_id, dest)
    }

    // ==== P3 同步支撑（docs/05-03 §五）====

    /// 摄取同步来的文件：密文块已按序写入 `part`（对端原样密文），装配容器并登记。
    /// FSKey 覆盖与向量时钟随条目入索引；密文块在本机不解密、不重加密。
    #[allow(clippy::too_many_arguments)]
    pub fn ingest_remote(
        &mut self,
        file_id: u64,
        folder_id: u64,
        name: &str,
        size: u64,
        modified_ms: u64,
        meta: crate::container::FileMeta,
        part: &Path,
        fskey_hex: &str,
        vc: std::collections::BTreeMap<String, u64>,
    ) -> Result<(), &'static str> {
        if self.index.file_folder(file_id).is_ok() {
            return Err("duplicate file id");
        }
        let cpath = self.container_path(file_id);
        assemble(&cpath, part, meta, &self.fsk(folder_id))?;
        let tokens = tokenize(name);
        self.index.add_remote_file(
            file_id,
            folder_id,
            name,
            size,
            modified_ms,
            fskey_hex,
            vc,
            &tokens,
        )?;
        self.save_index()
    }

    /// 同步清单：条目元数据 + 每文件密文块哈希列表（从容器元数据直接取，docs/05-03 §5.1）。
    /// 墓碑以 deletedMs 标记的清单条目形态包含在内（删除语义同步）。
    pub fn sync_manifest(&self) -> Result<String, &'static str> {
        let mut files = self.index.manifest_files();
        for f in files.iter_mut() {
            let id = f["id"].as_u64().unwrap_or(0);
            let folder = self.index.file_folder(id)?;
            let rd = ContainerReader::open(&self.container_path(id), &self.fsk(folder))?;
            f["chunks"] = serde_json::json!(rd
                .meta
                .chunks
                .iter()
                .map(|c| serde_json::json!({"hash": c.hash, "offset": c.offset, "len": c.len}))
                .collect::<Vec<_>>());
            f["noncePrefix"] = serde_json::json!(rd.meta.nonce_prefix);
            f["fileSha"] = serde_json::json!(rd.meta.file_sha256);
            f["createdMs"] = serde_json::json!(rd.meta.created_ms);
        }
        // 墓碑条目（无块清单）
        let mut tombs = self.index.manifest_tombstones();
        for t in tombs.iter_mut() {
            t["chunks"] = serde_json::json!([]);
        }
        files.extend(tombs);
        serde_json::to_string(&files).map_err(|_| "json serialize failed")
    }

    /// 读取密文块（对端请求的块序号），返回密文原样字节。
    pub fn read_chunk_ct(&self, file_id: u64, chunk_index: usize) -> Result<Vec<u8>, &'static str> {
        let folder = self.index.file_folder(file_id)?;
        let rd = ContainerReader::open(&self.container_path(file_id), &self.fsk(folder))?;
        rd.read_ct(chunk_index)
    }

    /// 待同步文件的 FSKey 原始字节（本机派生或索引内覆盖），E2E 信道携带给对端。
    pub fn fskey_bytes_for(&self, file_id: u64) -> Result<[u8; KEY_LEN], &'static str> {
        let folder = self.index.file_folder(file_id)?;
        self.fskey(folder, file_id)
    }

    /// 文件是否存在（同步计划判定用）。
    pub fn has_file(&self, file_id: u64) -> bool {
        self.index.file_folder(file_id).is_ok()
    }

    /// 同步容器与索引内 FSKey 覆盖的 hex 形式（冲突副本登记用）。
    pub fn fskey_hex_for(&self, file_id: u64) -> Result<String, &'static str> {
        Ok(vault_crypto::hex_encode(&self.fskey_bytes_for(file_id)?))
    }

    /// 保险箱是否没有任何文件（加入同步组前的一致性检查）。
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// 向量时钟并集（同步引擎用；内部保存由调用方 save_index_now 触发）。
    pub fn merge_vc(
        &mut self,
        file_id: u64,
        remote: &std::collections::BTreeMap<String, u64>,
    ) -> Result<(), &'static str> {
        self.index.merge_vc(file_id, remote)
    }

    /// 立即落盘加密索引（同步引擎在共享槽位上修改后调用）。
    pub fn save_index_now(&self) -> Result<(), &'static str> {
        self.save_index()
    }

    /// 文件最近修改时间（同步删除保护窗口判定）。
    pub fn file_modified(&self, file_id: u64) -> Result<u64, &'static str> {
        self.index.file_modified(file_id)
    }

    /// 整文件密文域校验：逐块 GCM 解密 + 密文哈希 + 明文 SHA-256 终验（同步接收后调用）。
    pub fn verify_file(&self, file_id: u64) -> Result<(), &'static str> {
        let folder = self.index.file_folder(file_id)?;
        let fskey = self.fskey(folder, file_id)?;
        let rd = ContainerReader::open(&self.container_path(file_id), &self.fsk(folder))?;
        rd.verify(&fskey)
    }

    /// 将既有文件另存为冲突副本（复制容器 + 新登记；时钟 = 对端时钟 ∪ 本机时钟，本机位 +1）。
    pub fn save_conflict_copy(
        &mut self,
        file_id: u64,
        remote_vc: std::collections::BTreeMap<String, u64>,
    ) -> Result<u64, &'static str> {
        let folder = self.index.file_folder(file_id)?;
        let name = self.index.file_name(file_id)?;
        let size = self.index.file_size(file_id)?;
        let modified = self.index.file_modified(file_id)?;
        let fskey_hex = self.fskey_hex_for(file_id)?;
        let local_vc = self.index.file_vc(file_id).unwrap_or_default();
        let src = self.container_path(file_id);
        let new_id = self.index.peek_next_id();
        let dest = self.container_path(new_id);
        std::fs::copy(&src, &dest).map_err(|_| "cannot copy container")?;
        let own_next = local_vc.get(&self.device_id).copied().unwrap_or(0) + 1;
        let mut merged = remote_vc;
        for (dev, cnt) in local_vc {
            let slot = merged.entry(dev).or_insert(0);
            if cnt > *slot {
                *slot = cnt;
            }
        }
        merged.insert(self.device_id.clone(), own_next);
        // 冲突副本命名：<原名>.conflict-<时间戳>（docs/05-03 §6.2 多版本保留）
        let stamped = format!("{name}.conflict-{}", now_ms());
        let tokens = crate::index::tokenize(&stamped);
        self.index.add_remote_file(
            new_id, folder, &stamped, size, modified, &fskey_hex, merged, &tokens,
        )?;
        self.save_index()?;
        Ok(new_id)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn fresh_vault(tag: &str) -> (tempfile::TempDir, Vault) {
        let dir = tempfile::tempdir().expect("tmp");
        let vault_file = dir.path().join(format!("{tag}.vsvb"));
        std::fs::write(&vault_file, b"fake-header").expect("write header stub");
        let mk = random_key();
        let v = Vault::open(&vault_file, &mk).expect("open");
        (dir, v)
    }

    fn write_src(dir: &Path, name: &str, data: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, data).expect("write src");
        p
    }

    #[test]
    fn import_export_roundtrip_text() {
        let (dir, mut v) = fresh_vault("rt");
        let content = b"Hello VaultSync! ".repeat(5000);
        let src = write_src(dir.path(), "hello.txt", &content);
        let id = v.import_file(&src, 0).expect("import");
        let dest = dir.path().join("out.txt");
        v.export_file(id, &dest).expect("export");
        assert_eq!(std::fs::read(&dest).expect("read"), content);
        assert!(v.search("hello").contains("hello.txt"));
        assert!(v.search("vaultsync").contains("\"id\""));
    }

    #[test]
    fn empty_file_roundtrip() {
        let (dir, mut v) = fresh_vault("empty");
        let src = write_src(dir.path(), "empty.dat", b"");
        let id = v.import_file(&src, 0).expect("import");
        let dest = dir.path().join("out.bin");
        v.export_file(id, &dest).expect("export");
        assert_eq!(std::fs::read(&dest).expect("read").len(), 0);
    }

    #[test]
    fn large_file_multi_chunk_adaptive() {
        let (dir, mut v) = fresh_vault("large");
        let content: Vec<u8> = (0..8 * 1024 * 1024u64).map(|i| (i % 251) as u8).collect();
        let src = write_src(dir.path(), "big.bin", &content);
        let id = v.import_file(&src, 0).expect("import");
        let dest = dir.path().join("big.out");
        v.export_file(id, &dest).expect("export");
        assert_eq!(std::fs::read(&dest).expect("read"), content);
        assert!(v.data_dir.join("files").join(format!("{id}.vse")).exists());
    }

    #[test]
    fn binary_file_skips_content_tokens() {
        let (dir, mut v) = fresh_vault("bin");
        let mut data = vec![0xABu8; 4096];
        data[0] = 0;
        let src = write_src(dir.path(), "blob.bin", &data);
        let id = v.import_file(&src, 0).expect("import");
        assert!(v.search("blob").contains("blob.bin"));
        let _ = id;
    }

    #[test]
    fn rename_mkdir_tree_and_delete() {
        let (dir, mut v) = fresh_vault("tree");
        let src = write_src(dir.path(), "a.txt", b"data");
        let id = v.import_file(&src, 0).expect("import");
        let docs = v.mkdir(0, "docs").expect("mkdir");
        v.rename_file(id, "b.txt").expect("rename");
        let list = v.list_children(docs).expect("list docs");
        assert!(list.contains("\"folders\":"));
        v.delete_file(id, true).expect("delete");
        assert!(!v.container_path(id).exists());
        assert!(v.export_file(id, &dir.path().join("x")).is_err());
    }

    #[test]
    fn share_burn_after_read() {
        let (dir, mut v) = fresh_vault("share");
        let src = write_src(dir.path(), "secret.txt", b"top secret");
        let id = v.import_file(&src, 0).expect("import");
        let (share_id, token) = v.create_share(id, 3600, 1).expect("share");

        let d1 = dir.path().join("open1.txt");
        v.open_share(share_id, &token, &d1).expect("open1");
        assert_eq!(std::fs::read(&d1).expect("read"), b"top secret");

        let d2 = dir.path().join("open2.txt");
        // 耗尽检查先于令牌校验：防止次数用尽后无限爆破令牌
        assert_eq!(
            v.open_share(share_id, "wrong", &d2).unwrap_err(),
            "share exhausted"
        );
        assert_eq!(
            v.open_share(share_id, &token, &d2).unwrap_err(),
            "share exhausted"
        );
    }

    #[test]
    fn folder_delete_recursive_secure() {
        let (dir, mut v) = fresh_vault("rec");
        let docs = v.mkdir(0, "docs").expect("mkdir");
        let sub = v.mkdir(docs, "sub").expect("mkdir");
        let f1 = write_src(dir.path(), "1.txt", b"111");
        let f2 = write_src(dir.path(), "2.txt", b"222");
        let id1 = v.import_file(&f1, docs).expect("i1");
        let id2 = v.import_file(&f2, sub).expect("i2");
        let n = v.delete_folder(docs, true).expect("rm");
        assert_eq!(n, 2);
        assert!(!v.container_path(id1).exists());
        assert!(!v.container_path(id2).exists());
    }

    #[test]
    fn debug_chunk_decrypt() {
        let (dir, mut v) = fresh_vault("dbg");
        let content = b"ABCDEFGH".repeat(1000); // 8KB
        let src = write_src(dir.path(), "d.txt", &content);
        let id = v.import_file(&src, 0).expect("import");
        let cpath = v.container_path(id);
        let fsk = v.fsk(0);
        let fskey = v.fskey(0, id).expect("fskey");
        println!("id={id} fsk={:02x?} fskey={:02x?}", &fsk[..4], &fskey[..4]);
        let rd = crate::container::ContainerReader::open(&cpath, &fsk).expect("open");
        println!("chunks={} size={}", rd.meta.chunks.len(), rd.meta.size);
        println!("prefix={:08x}", rd.meta.nonce_prefix);
        let mut srcf = std::fs::File::open(&cpath).expect("open c");
        use std::io::{Read, Seek};
        srcf.seek(std::io::SeekFrom::Start(0)).ok();
        let mut whole = Vec::new();
        srcf.read_to_end(&mut whole).expect("read");
        let (c0hash, c0off, c0len): (String, u64, u32) = {
            let c = &rd.meta.chunks[0];
            (c.hash.clone(), c.offset, c.len)
        };
        println!("c0 off={c0off} len={c0len} hash={c0hash}");
        let base =
            (whole.len() - rd.meta.chunks.iter().map(|c| c.len as usize).sum::<usize>()) as u64;
        println!("computed base={base}");
        let start = (base + c0off) as usize;
        let ct = &whole[start..start + c0len as usize];
        println!("ct hash match: {}", vault_crypto::hash_sha256(ct) == c0hash);
        let mut nonce = [0u8; 12];
        nonce[..4].copy_from_slice(&rd.meta.nonce_prefix.to_be_bytes());
        nonce[4..].copy_from_slice(&0u64.to_be_bytes());
        let mut blob = nonce.to_vec();
        blob.extend_from_slice(ct);
        let pt = vault_crypto::aead::aead_decrypt_with_aad(&fskey, &blob, &0u64.to_be_bytes());
        println!("decrypt chunk0 ok: {}", pt.is_some());
        // 候选密钥逐一尝试
        let mk = &*v.mk;
        let cands: Vec<(&str, [u8; 32])> = vec![
            ("fskey(0,id)", fskey),
            ("fsk(0)", v.fsk(0)),
            ("mk", *mk),
            (
                "hkdf(mk,fskey/1)",
                *vault_crypto::kdf::hkdf_sha256_derive(mk, b"fskey/1"),
            ),
        ];
        for (name, k) in &cands {
            let ok =
                vault_crypto::aead::aead_decrypt_with_aad(k, &blob, &0u64.to_be_bytes()).is_some();
            println!("CAND {} -> {}", name, ok);
        }
    }
}
