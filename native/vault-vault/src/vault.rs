//! 保险箱引擎编排：导入 / 导出 / 文件夹树 / 擦除 / 阅后即焚 / 密钥轮换（docs/05-02 §四、docs/07 §二/§三）。
//!
//! 布局：`<vault 文件同目录>/<stem>.data/`
//! ```text
//! index.enc            加密索引（VSIX v2，整体 AEAD）
//! files/<id>.vse       每文件独立密文容器（VSEF v2）
//! files/<id>.<tag>.vse.new  密钥轮换的暂存容器（仅轮换窗口内存在，见 rotate_containers）
//! ```
//! 密钥：FSK(folder)=HKDF(MK,"fsk/<id>") 或索引内 FSK 覆盖（轮换写入的随机值）；
//! FSKey=索引内覆盖（同步来的 / 轮换后的）或 HKDF(FSK,"fskey/<id>")（docs/05-02 §3.1）。
#![forbid(unsafe_code)]
use std::collections::BTreeMap;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use zeroize::Zeroizing;

use vault_crypto::kdf::hkdf_sha256_derive;
use vault_crypto::{random_bytes, random_key, KEY_LEN};

use crate::container::{assemble, chunk_cfg_for, ContainerReader, FileMeta};
use crate::index::{tokenize, SearchV2Opts, VaultIndex};
use vault_store::{Namespace, OpenMode, VaultStore};

pub struct Vault {
    data_dir: PathBuf,
    index_path: PathBuf,
    index: VaultIndex,
    /// MK：FSK/FSKey 派生根（文件夹密钥**不属于**从属密钥层，05-01 §3.1 密钥树）。
    mk: Zeroizing<[u8; KEY_LEN]>,
    /// 从属密钥 1（索引段密钥根，P7-2）：与 MK 解耦，MK 轮换不变。
    index_key: Zeroizing<[u8; KEY_LEN]>,
    /// 从属密钥 2（搜索令牌 HMAC 密钥，P7-2）：MK 轮换后令牌依然有效。
    search_key: Zeroizing<[u8; KEY_LEN]>,
    /// 本机向量时钟设备位（P3 同步；由上层在解锁后设置）。
    device_id: String,
    /// Some = VSIX v3 分段存储路径（P6-3）；None = legacy VSIX v2 单文件路径
    /// （v0.5.0 旧库保持原读写语义，待 P7-9 迁移工具统一）。
    store: Option<VaultStore>,
}

/// 全库轮换重建倒排索引时可保留的内容采样令牌总量上限。
/// 倒排索引必须整表重建（HMAC 令牌不可迁移），这里给内存设一个上界：超出后只重建
/// 文件名/标签令牌（检索仍可用，仅少了内容采样命中），避免超大库轮换时内存膨胀。
const MAX_REBASE_TOKENS: usize = 2_000_000;

/// 单文件轮换的阶段一产物：暂存容器的随机标签与新 FSKey。
/// 标签让「索引标记 ↔ 暂存文件 ↔ 密钥」三者一一对应：新一轮轮换换用新标签，
/// 读路径绝不可能取到上一轮遗留的暂存文件（详见 `rotate_containers`）。
struct Staged {
    file_id: u64,
    tag: String,
    fskey: Zeroizing<[u8; KEY_LEN]>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Vault {
    /// 从 vault 头文件定位数据目录并加载（或初始化）索引。
    /// `index_key` / `search_key` = 从属密钥 1/2（P7-2；v2 兼容期值 = 冻结公式）。
    pub fn open(
        vault_path: &Path,
        mk: &[u8; KEY_LEN],
        index_key: &[u8; KEY_LEN],
        search_key: &[u8; KEY_LEN],
    ) -> Result<Self, &'static str> {
        let stem = vault_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("invalid vault file name")?;
        let parent = vault_path.parent().unwrap_or(Path::new("."));
        let data_dir = parent.join(format!("{stem}.data"));
        std::fs::create_dir_all(data_dir.join("files")).map_err(|_| "cannot create data dir")?;
        let index_path = data_dir.join("index.enc");
        let index_key = Zeroizing::new(*index_key);
        let search_key = Zeroizing::new(*search_key);
        // 双路径（P6-3）：旧库（存在 index.enc）继续走 VSIX v2 单文件；
        // 新建库走 VSIX v3 分段存储。P7-9 迁移工具负责把旧库统一到 v3。
        let (index, store) = if index_path.exists() {
            let ix = VaultIndex::load(&index_path, &index_key, *search_key)?;
            (ix, None)
        } else {
            // 段密钥根 = 从属密钥 1（P7-2）：与 MK 解耦，MK 轮换不动任何段
            let st = VaultStore::open(&data_dir, Namespace::Index, &index_key, OpenMode::ReadWrite)
                .map_err(Self::store_err)?;
            let pairs = st.kv_pairs();
            let ix = if pairs.is_empty() {
                VaultIndex::new(*search_key)
            } else {
                VaultIndex::from_v3_pairs(&pairs, *search_key)?
            };
            (ix, Some(st))
        };
        let v = Self {
            data_dir,
            index_path,
            index,
            mk: Zeroizing::new(*mk),
            index_key,
            search_key,
            device_id: String::new(),
            store,
        };
        // 清扫上次轮换/装配崩溃遗留的临时文件（含明文临时文件，见函数注释）
        v.sweep_temp_files();
        Ok(v)
    }

    /// 设置向量时钟本机设备位（同步引擎初始化后调用）。
    pub fn set_device_id(&mut self, device_id: &str) {
        self.device_id = device_id.to_string();
    }

    fn derive_key(mk: &[u8; KEY_LEN], info: &str) -> Zeroizing<[u8; KEY_LEN]> {
        hkdf_sha256_derive(mk, info.as_bytes())
    }

    /// 解析索引内的密钥覆盖（hex，32B）。覆盖只允许来自本机轮换或同步携带，
    /// 因此格式错误一律报错而不静默回落派生——静默回落会用一个错键去开容器，掩盖损坏。
    fn decode_key(hex: &str) -> Result<[u8; KEY_LEN], &'static str> {
        let raw = vault_crypto::hex_decode(hex).ok_or("bad key hex")?;
        raw.as_slice().try_into().map_err(|_| "bad key length")
    }

    /// 文件夹密钥：索引内 FSK 覆盖优先（轮换写入），否则 HKDF(MK,"fsk/<id>")（docs/05-02 §3.1）。
    /// 覆盖是**随机**值而非派生值：MK 泄露场景下派生值会被重算出来，随机值才切断旧密钥路径
    ///（docs/07 §二的轮换动机，同样适用于 FSKey）。
    fn fsk(&self, folder_id: u64) -> Result<Zeroizing<[u8; KEY_LEN]>, &'static str> {
        if let Some(hex) = self.index.folder_fsk(folder_id)? {
            return Ok(Zeroizing::new(Self::decode_key(&hex)?));
        }
        Ok(Self::derive_key(&self.mk, &format!("fsk/{folder_id}")))
    }

    /// 文件密钥：同步文件与轮换后的文件用索引内 FSKey 覆盖，其余按 FSK 派生（docs/05-02 §3.1）。
    fn fskey(
        &self,
        folder_id: u64,
        file_id: u64,
    ) -> Result<Zeroizing<[u8; KEY_LEN]>, &'static str> {
        if let Some(hex) = self.index.file_fskey(file_id)? {
            return Ok(Zeroizing::new(Self::decode_key(&hex)?));
        }
        let fsk = self.fsk(folder_id)?;
        Ok(hkdf_sha256_derive(
            &fsk[..],
            format!("fskey/{file_id}").as_bytes(),
        ))
    }

    /// vault-store 错误转 &str（保持本 crate 错误风格）。
    fn store_err(e: vault_store::StoreError) -> &'static str {
        match e {
            vault_store::StoreError::ReadOnly => "vault opened read-only",
            vault_store::StoreError::LeaseBusy => "lease held by another process",
            vault_store::StoreError::NeedsRecovery(_) => "recovery required",
            vault_store::StoreError::Maintenance => "vault in maintenance",
            _ => "index store io failed",
        }
    }

    fn save_index(&mut self) -> Result<(), &'static str> {
        match self.store.as_mut() {
            None => {
                // legacy VSIX v2（保持 v0.5.0 原语义；非原子写是已知缺口，
                // 由 P6-3 v3 路径消灭，旧库待 P7-9 迁移）
                self.index.save(&self.index_path, &self.index_key)
            }
            Some(st) => {
                let ops = if self.index.take_full_rewrite() {
                    self.index.full_state_ops()
                } else {
                    self.index.take_pending_ops()
                };
                if ops.is_empty() {
                    return Ok(());
                }
                st.kv_apply(ops).map_err(Self::store_err)?;
                st.flush().map_err(Self::store_err).map(|_| ())
            }
        }
    }

    fn container_path(&self, file_id: u64) -> PathBuf {
        self.data_dir.join("files").join(format!("{file_id}.vse"))
    }

    /// 轮换暂存容器路径：`files/<id>.<tag>.vse.new`（tag 为每轮轮换的随机标记）。
    fn staged_container_path(&self, file_id: u64, tag: &str) -> PathBuf {
        self.data_dir
            .join("files")
            .join(format!("{file_id}.{tag}.vse.new"))
    }

    /// 容器读取路径：轮换窗口内以暂存容器为准，其余取正式路径。
    /// 不变量见 `rotate_containers`：索引中的密钥与「标记指向的暂存文件（若存在）或正式容器」配对，
    /// 故此处只做「标记 + 文件存在」的判定，不需要探测密钥。
    fn read_container_path(&self, file_id: u64) -> PathBuf {
        if let Some(tag) = self.index.staging_tag(file_id) {
            let staged = self.staged_container_path(file_id, &tag);
            if staged.exists() {
                return staged;
            }
        }
        self.container_path(file_id)
    }

    /// 某文件的全部密文副本（正式容器 + 轮换暂存容器），删除/覆写用。
    fn container_paths(&self, file_id: u64) -> Vec<PathBuf> {
        let mut paths = vec![self.container_path(file_id)];
        if let Some(tag) = self.index.staging_tag(file_id) {
            paths.push(self.staged_container_path(file_id, &tag));
        }
        paths
    }

    /// 单次零覆写（SSD 兜底语义，与既有擦除实现一致）；失败不报错（介质不支持时为 best-effort）。
    fn overwrite_in_place(path: &Path) {
        if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(path) {
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

    /// open 时一次性的临时文件清扫（轮换/装配崩溃的残骸）：
    /// - `files/*.vse.new`：仅当索引标记不再指向它时删除——无标记即「阶段二提交之前」的垃圾
    ///   （时序保证见 `rotate_containers`），有标记则仍是在用暂存容器，必须保留；
    /// - `files/*.vse.tmp`（assemble 的中间产物）与 `rotate-*.plain.tmp`（轮换的明文临时文件）
    ///   一律删除：docs/07 §二/§三 要求明文只允许短暂存在于内存或临时文件。
    fn sweep_temp_files(&self) {
        let files_dir = self.data_dir.join("files");
        if let Ok(entries) = std::fs::read_dir(&files_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let stale = if name.ends_with(".vse.tmp") {
                    true
                } else if let Some(stem) = name.strip_suffix(".vse.new") {
                    match stem.split_once('.') {
                        Some((id_part, tag)) => id_part
                            .parse::<u64>()
                            .map(|id| self.index.staging_tag(id).as_deref() != Some(tag))
                            .unwrap_or(false),
                        None => false,
                    }
                } else {
                    false
                };
                if stale {
                    std::fs::remove_file(entry.path()).ok();
                }
            }
        }
        if let Ok(entries) = std::fs::read_dir(&self.data_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("rotate-") && name.ends_with(".plain.tmp") {
                    std::fs::remove_file(entry.path()).ok();
                }
            }
        }
    }

    /// 内容采样令牌（文件名之外）：文本型文件取前 1MB，含 NUL 视为二进制跳过。
    /// 导入与轮换共用同一规则，保证 MK 轮换后重建的倒排索引与导入时一致。
    fn sample_content_tokens(f: &mut std::fs::File, size: u64) -> Vec<String> {
        if size == 0 {
            return Vec::new();
        }
        let mut sample = Vec::new();
        if f.seek(std::io::SeekFrom::Start(0)).is_ok() {
            let _ = Read::by_ref(f).take(1024 * 1024).read_to_end(&mut sample);
        }
        let probe = &sample[..sample.len().min(8192)];
        if probe.contains(&0u8) {
            return Vec::new();
        }
        crate::index::content_tokens(&String::from_utf8_lossy(&sample))
    }

    /// 以 (元数据键, 文件键) 把明文文件流式加密为容器（恒定内存）：CDC 分块 → 计数器 nonce 逐块 GCM
    /// → `assemble`（内部临时文件 + rename 原子落位）。返回内容采样令牌。
    /// 导入与密钥轮换共用此路径，保证两条写入路径的分块、nonce、元数据语义完全一致。
    #[allow(clippy::too_many_arguments)]
    fn encrypt_plain_to_container(
        &self,
        plain: &Path,
        dest: &Path,
        name: &str,
        created_ms: u64,
        modified_ms: u64,
        meta_key: &[u8; KEY_LEN],
        file_key: &[u8; KEY_LEN],
        sample_tokens: bool,
    ) -> Result<Vec<String>, &'static str> {
        let mut f = std::fs::File::open(plain).map_err(|_| "cannot open plaintext file")?;
        let size = f.metadata().map_err(|_| "cannot stat plaintext")?.len();
        let nonce_prefix = u32::from_le_bytes(
            random_bytes(4)
                .as_slice()
                .try_into()
                .map_err(|_| "nonce len")?,
        );
        let cfg = chunk_cfg_for(size);

        let part = dest.with_extension("vse.part");
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
                let blob =
                    vault_crypto::aead::aead_encrypt_with_aad(file_key, &nonce, plain, &aad)?;
                let ct = &blob[vault_crypto::AES_GCM_NONCE_LEN..];
                partf.write_all(ct).map_err(|_| "write chunk failed")?;
                chunks.push((vault_crypto::hash_sha256(ct), offset, ct.len() as u32));
                offset += ct.len() as u64;
                counter += 1;
            }
        }
        drop(partf);

        let tokens = if sample_tokens {
            Self::sample_content_tokens(&mut f, size)
        } else {
            Vec::new()
        };
        let meta = FileMeta {
            name: name.to_string(),
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
            created_ms,
            modified_ms,
        };
        assemble(dest, &part, meta, meta_key)?;
        std::fs::remove_file(&part).ok();
        Ok(tokens)
    }

    /// 流式加密导入（F-01，恒定内存）：CDC 分块 → FSKey + 计数器 nonce 逐块 GCM。
    pub fn import_file(&mut self, src: &Path, folder_id: u64) -> Result<u64, &'static str> {
        let md = std::fs::metadata(src).map_err(|_| "cannot stat source")?;
        let size = md.len();
        let name = src
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("invalid source file name")?
            .to_string();
        let modified_ms = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        // 先占 id：FSKey 派生依赖 file_id；同时校验文件夹存在（不留孤儿容器）
        let file_id = self.index.alloc_id();
        let meta_key = self.fsk(folder_id)?;
        let file_key = hkdf_sha256_derive(&meta_key[..], format!("fskey/{file_id}").as_bytes());

        let cpath = self.container_path(file_id);
        let content_tokens = self.encrypt_plain_to_container(
            src,
            &cpath,
            &name,
            modified_ms,
            modified_ms,
            &meta_key,
            &file_key,
            true, // 导入需要内容采样令牌建倒排
        )?;

        let mut tokens = tokenize(&name);
        tokens.extend(content_tokens.iter().cloned());
        self.index
            .add_file_with_id(file_id, folder_id, &name, size, &tokens)?;
        // 内容采样令牌的 HMAC 随条目持久化（v3：倒排索引可从条目重建）
        self.index.set_tokens_extra(
            file_id,
            content_tokens
                .iter()
                .map(|t| self.index.token_hash(t))
                .collect(),
        )?;
        self.index.bump_vc(file_id, &self.device_id).ok();
        self.save_index()?;
        Ok(file_id)
    }

    /// 流式解密导出（F-02）：逐块 GCM 校验 + 整文件 SHA-256 终验。
    pub fn export_file(&self, file_id: u64, dest: &Path) -> Result<(), &'static str> {
        let folder = self.index.file_folder(file_id)?;
        let fskey = self.fskey(folder, file_id)?;
        let fsk = self.fsk(folder)?;
        let rd = ContainerReader::open(&self.read_container_path(file_id), &fsk)?;
        rd.export_to(dest, &fskey)
    }

    pub fn mkdir(&mut self, parent: u64, name: &str) -> Result<u64, &'static str> {
        let id = self.index.mkdir(parent, name)?;
        self.save_index()?;
        Ok(id)
    }

    /// 文件名（隐写载荷需要随密文携带原文件名，供提取端还原）。
    pub fn file_name(&self, file_id: u64) -> Result<String, &'static str> {
        self.index.file_name(file_id)
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

    /// 检索 V2（docs/v2.0/05-02 §4.5，P7-9）：排名打分 + class 标注 + 脱敏片段。
    /// content 类命中的 spans 为空——片段重算需明文，上层解密后经
    /// `index::content_snippets` 补齐（诚实边界，引擎侧不解密全文索引）。
    pub fn search_v2(&self, query: &str, opts: &SearchV2Opts) -> serde_json::Value {
        self.index.search_v2(query, opts)
    }

    pub fn set_tags(&mut self, file_id: u64, tags: Vec<String>) -> Result<(), &'static str> {
        self.index.set_tags(file_id, tags)?;
        self.save_index()
    }

    /// 安全擦除（F-03，docs/05-02 §4.3）：容器整体删除（密文+密钥材料同灭），
    /// secure=true 时删除前单次覆写（介质兜底；SSD 上为单次覆写语义）。
    /// 轮换窗口内该文件可能同时存在暂存容器，一并覆写删除（孤儿暂存文件由 open 清扫兜底）。
    pub fn delete_file(&mut self, file_id: u64, secure: bool) -> Result<(), &'static str> {
        // 删除前快取向量时钟并自增本机位作墓碑（使删除在时钟上支配原版本，docs/05-03 §6.2）
        let mut vc = self.index.file_vc(file_id).unwrap_or_default();
        if !self.device_id.is_empty() {
            let next = vc.get(&self.device_id).copied().unwrap_or(0) + 1;
            vc.insert(self.device_id.clone(), next);
        }
        // 标记随条目一同消失，故先取路径集合
        let paths = self.container_paths(file_id);
        self.index.remove_file(file_id)?;
        self.index.record_tombstone(file_id, vc);
        self.save_index()?;
        if secure {
            for p in &paths {
                Self::overwrite_in_place(p);
            }
        }
        for p in paths.iter().skip(1) {
            std::fs::remove_file(p).ok();
        }
        std::fs::remove_file(&paths[0]).map_err(|_| "cannot delete container")
    }

    pub fn delete_folder(&mut self, folder_id: u64, secure: bool) -> Result<usize, &'static str> {
        // 递归收集受影响文件，逐个留墓碑（删除语义同步到对端）
        let affected = self.index.files_under(folder_id);
        for id in &affected {
            if let Ok(vc) = self.index.file_vc(*id) {
                self.index.record_tombstone(*id, vc);
            }
        }
        // 标记随条目消失，故先取每个文件的密文路径集合（正式容器 + 轮换暂存容器）
        let path_sets: Vec<Vec<PathBuf>> = affected
            .iter()
            .map(|id| self.container_paths(*id))
            .collect();
        let ids = self.index.remove_folder(folder_id)?;
        self.save_index()?;
        for paths in &path_sets {
            for p in paths {
                if secure {
                    Self::overwrite_in_place(p);
                }
                std::fs::remove_file(p).ok();
            }
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
        assemble(&cpath, part, meta, &*self.fsk(folder_id)?)?;
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
            let rd = ContainerReader::open(&self.read_container_path(id), &*self.fsk(folder)?)?;
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
        let rd = ContainerReader::open(&self.read_container_path(file_id), &*self.fsk(folder)?)?;
        rd.read_ct(chunk_index)
    }

    /// 待同步文件的 FSKey 原始字节（本机派生或索引内覆盖），E2E 信道携带给对端。
    /// 与 `read_container_path` 配对：索引内覆盖（含轮换后的新 FSKey）正是当前密文所用密钥。
    pub fn fskey_bytes_for(&self, file_id: u64) -> Result<[u8; KEY_LEN], &'static str> {
        let folder = self.index.file_folder(file_id)?;
        Ok(*self.fskey(folder, file_id)?)
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

    /// 从属密钥 2（P7-9 检索 V2 接线预留；避免结构性 dead_code）。
    #[allow(dead_code)]
    pub(crate) fn search_key(&self) -> &[u8; KEY_LEN] {
        &self.search_key
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
    pub fn save_index_now(&mut self) -> Result<(), &'static str> {
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
        let rd = ContainerReader::open(&self.read_container_path(file_id), &*self.fsk(folder)?)?;
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
        // 复制的是当前生效的容器（轮换窗口内即暂存容器），其密钥正是索引里的 FSKey 覆盖
        let src = self.read_container_path(file_id);
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

    // ==== P5-4 密钥轮换（docs/07 §二 FSKey/FSK 行、§三 的数据面）====
    //
    // 密钥来源（硬性要求，见 docs/07 §二 的泄露影响域分析）：
    //   FSKey' 与 FSK' 一律用 `vault_crypto::random` 生成 32B 随机值，**不用 HKDF 派生**。
    //   轮换的动机是「旧密钥可能已泄露」，而 HKDF 派生值在 MK 泄露场景下可被攻击者重算出来，
    //   等于没换；只有随机值与旧密钥材料在计算上无关，才真正切断旧密钥路径。
    //   代价：随机值必须存进索引（`fskey`/`Folder.fskey` 覆盖），索引本身仍受 MK/KEK 体系保护。

    /// 单文件密钥轮换（docs/07 §二 FSKey 行）：以全新随机 FSKey 重写该文件容器，
    /// 并把新 FSKey 写进索引的 `fskey` 覆盖字段。返回新 FSKey 的 hex。
    ///
    /// 影响域仅该文件：其余文件的容器字节与索引条目一字不动（回归测试见证）。
    /// 不推进 vc、不改 modified_ms：轮换是本机密钥操作，不改变内容的因果版本——
    /// vc 只描述内容的因果历史，密钥更换不属于内容变更；对端若需重新拉取，
    /// 由上层以显式推送/覆盖语义表达（本方法不做同步层决策）。
    pub fn rotate_file_key(&mut self, file_id: u64) -> Result<String, &'static str> {
        self.index.file_folder(file_id)?; // 不存在 → "file not found"
        if !self.read_container_path(file_id).exists() {
            return Err("container not found");
        }
        self.rotate_containers(&[file_id], &BTreeMap::new(), false)?;
        self.index
            .file_fskey(file_id)?
            .ok_or("fskey missing after rotation")
    }

    /// 文件夹密钥轮换（docs/07 §二 FSK 行：「文件夹内逐文件重加密」）：
    /// 为该文件夹生成**新的随机 FSK 覆盖**，并以 (FSK', 每文件新随机 FSKey) 重写其直属文件容器。
    /// 返回重写的文件数；根文件夹（id=0）是文件夹，允许轮换。
    ///
    /// 范围：只重写 `folder == folder_id` 的**直属**文件（子文件夹的 FSK 由 MK 独立派生，
    /// 不受该文件夹 FSK 泄露影响，故不越界重写；需要连子文件夹一并轮换时，调用方逐个传入子文件夹 id）。
    /// 空文件夹仍写入新的 FSK 覆盖并返回 0（幂等；未来导入即用新键）。
    pub fn rotate_folder_keys(&mut self, folder_id: u64) -> Result<usize, &'static str> {
        self.index.folder_fsk(folder_id)?; // 不存在 → "folder not found"
        let ids = self.index.files_direct(folder_id);
        let mut overrides: BTreeMap<u64, [u8; KEY_LEN]> = BTreeMap::new();
        overrides.insert(folder_id, random_key());
        let (n, _) = self.rotate_containers(&ids, &overrides, false)?;
        Ok(n)
    }

    /// 全库重加密（docs/07 §三 的数据面）：以 new_mk 重写索引键、全部文件夹 FSK 覆盖与全部容器
    /// （每个文件新随机 FSKey）。**成功后 Vault 内部 MK 切换为 new_mk**，调用方负责把 MK' 重新
    /// 包装进 KEK 副本并重写保险箱头部。返回重写的文件数。
    ///
    /// 顺序与崩溃可恢复性：容器重写与索引提交（`rotate_containers` 阶段一~四）全部在**旧索引键**
    /// 下完成，因此中途崩溃时用旧 MK 仍能打开并读全库、可重试；只有容器全部重写完毕才切换 MK 并以
    /// 新索引键落盘（落盘失败即回滚 MK，磁盘保持「旧 MK 可开的索引 + 新容器密钥」的一致状态）。
    /// MK 切换后，旧 MK 派生出的任何 FSK/FSKey 与本库密文再无关系（覆盖与 FSKey 皆为随机值）。
    ///
    /// P7-2 后的附带说明：搜索令牌与分享票据哈希已迁移到从属密钥层（与 MK 解耦），
    /// 轮换后检索与分享**保持有效**，无需重建/作废（`rebase_search_key` 保留给
    /// 从属密钥自身的独立轮换使用，MK 轮换不再调用）。
    pub fn rekey_all(&mut self, new_mk: &[u8; KEY_LEN]) -> Result<usize, &'static str> {
        // 明文比较即可：new_mk 由调用方（本进程内）提供，不构成可观测的旁路。
        if *new_mk == *self.mk {
            return Err("new mk equals current mk");
        }
        let mut overrides: BTreeMap<u64, [u8; KEY_LEN]> = BTreeMap::new();
        for folder in self.index.folder_ids() {
            overrides.insert(folder, random_key());
        }
        let n = {
            let ids = self.index.all_file_ids();
            let (n, _content_tokens) = self.rotate_containers(&ids, &overrides, true)?;
            let _ = _content_tokens;
            n
        };

        // P7-2：搜索令牌（从属密钥 2）与分享票据（从属密钥 3）与 MK 解耦，
        // MK 轮换后**依然有效**——不再重建倒排、不再作废分享（V1.0 两个破坏性代价消除）。
        // 索引段的段密钥根 = 从属密钥 1 → 索引 store 无需任何重建（P6 的 rebuild 窗口随之消失）。

        let old_mk = std::mem::replace(&mut self.mk, Zeroizing::new(*new_mk));
        let saved = self.save_index().map_err(|_| "cannot save rekeyed index");
        if let Err(e) = saved {
            self.mk = old_mk; // 回滚：磁盘仍是旧 MK 可开的索引，容器密钥已在其中，可重试
            return Err(e);
        }
        Ok(n)
    }

    /// 轮换核心：把 `ids` 的容器逐个重写为「新元数据键 + 新随机 FSKey」并原子提交索引。
    ///
    /// 崩溃恢复设计（容器格式 V2 不含密钥代际信息，故用「暂存容器 + 索引标记」两阶段切换）：
    /// ```text
    /// 阶段一  逐个写暂存容器 files/<id>.<tag>.vse.new（assemble 内部 tmp + fsync + rename，
    ///         任何时刻读到的暂存文件都是完整容器）；明文只在临时文件里短暂存在，写完立即删除；
    /// 阶段二  一次 save_index 提交「新 FSK 覆盖 + 新 FSKey 覆盖 + 标记 <id>→<tag>」——三者
    ///         在同一次 AEAD 落盘中生效，不存在「密钥已换而标记未落」的中间态；
    /// 阶段三  逐个 rename 暂存容器 → 正式路径（原子替换）；
    /// 阶段四  一次 save_index 清除本轮标记。
    /// ```
    /// 任意点崩溃都是可读状态：读路径按「标记 + 标记指向的暂存文件存在」判定——
    ///   · 阶段一崩溃：索引未变（旧密钥、无标记），正式容器仍是旧密钥的产物 → 一致；
    ///   · 阶段二之后崩溃：密钥已提交，标记指向的暂存容器存在 → 读暂存容器（一致）；
    ///     若 rename 已完成则暂存文件不存在 → 读正式容器（同为已提交密钥的产物）；
    ///   · 无标记的暂存文件是对应文件在「阶段二提交前」的残骸，绝不被读路径引用，
    ///     由 `open` 的清扫删除，也会被下一次轮换覆盖（旧 tag 不再被引用）。
    /// 因此**不会出现索引指向不可解或半写容器**；代价是轮换期间磁盘上短暂存在双份密文
    ///（暂存容器在 rename 后即消失）与一份明文临时文件（写完立即删除，open 时兜底清扫）。
    /// 索引与目录项的落盘沿既有约定（文件内容 fsync + rename，目录项不额外 fsync）。
    fn rotate_containers(
        &mut self,
        ids: &[u64],
        folder_overrides: &BTreeMap<u64, [u8; KEY_LEN]>,
        collect_content_tokens: bool,
    ) -> Result<(usize, BTreeMap<u64, Vec<String>>), &'static str> {
        let mut staged: Vec<Staged> = Vec::with_capacity(ids.len());
        let mut content_tokens: BTreeMap<u64, Vec<String>> = BTreeMap::new();
        // 只有全库轮换需要重建倒排（内容令牌取自这里的明文采样）；单文件/文件夹轮换不采样
        let mut token_budget = if collect_content_tokens {
            MAX_REBASE_TOKENS
        } else {
            0
        };

        // 阶段一：逐个解密（复用导出路径：逐块 GCM + 密文哈希 + 整文件 SHA-256 终验）后以新密钥重写
        for &file_id in ids {
            let folder = self.index.file_folder(file_id)?;
            let old_meta_key = self.fsk(folder)?; // 仍是旧 FSK（新覆盖在阶段二才提交）
            let old_file_key = self.fskey(folder, file_id)?;
            let new_meta_key: [u8; KEY_LEN] = folder_overrides
                .get(&folder)
                .copied()
                .unwrap_or(*old_meta_key);
            let new_file_key = random_key(); // 随机而非派生：见本节开头
            let tag = vault_crypto::hex_encode(&random_bytes(8));

            let src = self.read_container_path(file_id);
            let dest = self.staged_container_path(file_id, &tag);
            let plain = self.data_dir.join(format!("rotate-{file_id}.plain.tmp"));
            let tokens = {
                let rd = ContainerReader::open(&src, &old_meta_key)?;
                let old = (
                    rd.meta.name.clone(),
                    rd.meta.created_ms,
                    rd.meta.modified_ms,
                );
                let res = rd.export_to(&plain, &old_file_key).and_then(|()| {
                    self.encrypt_plain_to_container(
                        &plain,
                        &dest,
                        &old.0,
                        old.1,
                        old.2,
                        &new_meta_key,
                        &new_file_key,
                        token_budget > 0,
                    )
                });
                // 明文临时文件用后立即删除（成功失败都删；崩溃残留由 open 清扫兜底）
                std::fs::remove_file(&plain).ok();
                res?
            };
            if token_budget > 0 {
                token_budget = token_budget.saturating_sub(tokens.len());
                content_tokens.insert(file_id, tokens);
            }
            staged.push(Staged {
                file_id,
                tag,
                fskey: Zeroizing::new(new_file_key),
            });
        }

        // 阶段二：单次索引提交（新密钥与标记同一次落盘，原子生效）
        for (folder, key) in folder_overrides {
            self.index
                .set_folder_fskey(*folder, &vault_crypto::hex_encode(key))?;
        }
        for s in &staged {
            self.index
                .set_file_fskey(s.file_id, &vault_crypto::hex_encode(&*s.fskey))?;
            self.index.mark_staging(s.file_id, &s.tag);
        }
        self.save_index()?;

        // 阶段三：逐个落位。中途失败时已落位者已生效、未落位者仍走暂存路径，状态依旧一致可读，
        // 如实报错交由调用方（或下一次轮换）重试。
        for s in &staged {
            let from = self.staged_container_path(s.file_id, &s.tag);
            let to = self.container_path(s.file_id);
            std::fs::rename(&from, &to).map_err(|_| "cannot promote staged container")?;
        }

        // 阶段四：清除本轮标记（只清本轮涉及的文件，其它待生效文件的标记仍指向其暂存容器）
        let rotated: Vec<u64> = staged.iter().map(|s| s.file_id).collect();
        self.index.clear_staging(&rotated);
        self.save_index()?;
        Ok((staged.len(), content_tokens))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[allow(unused_mut)]
mod tests {
    /// 测试辅助：从属密钥 1/2（兼容期冻结公式；正式路径来自 keystore 包装区）。
    fn sub_keys_for_test(mk: &[u8; KEY_LEN]) -> ([u8; KEY_LEN], [u8; KEY_LEN]) {
        (
            *hkdf_sha256_derive(mk, b"vault-index"),
            *hkdf_sha256_derive(mk, b"search"),
        )
    }

    use super::*;

    fn fresh_vault(tag: &str) -> (tempfile::TempDir, Vault) {
        let dir = tempfile::tempdir().expect("tmp");
        let vault_file = dir.path().join(format!("{tag}.vsvb"));
        std::fs::write(&vault_file, b"fake-header").expect("write header stub");
        let mk = random_key();
        let (v_ik, v_sk) = sub_keys_for_test(&mk);
        let v = Vault::open(&vault_file, &mk, &v_ik, &v_sk).expect("open");
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
        let fsk = v.fsk(0).expect("fsk");
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
            ("fskey(0,id)", *fskey),
            ("fsk(0)", *fsk),
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

    // ==== P5-4 密钥轮换测试（docs/07 §六：轮换后其他文件不受影响 / 全库轮换）====

    /// 正式容器路径的密文字节（用于断言「其它文件完全不受影响」）。
    fn container_bytes(v: &Vault, id: u64) -> Vec<u8> {
        std::fs::read(v.container_path(id)).expect("read container")
    }

    /// 容器加密元数据里的块清单（密文哈希 + 偏移 + 长度）。
    fn chunk_entries(v: &Vault, id: u64) -> Vec<crate::container::ChunkEntry> {
        let folder = v.index.file_folder(id).expect("folder");
        let fsk = v.fsk(folder).expect("fsk");
        crate::container::ContainerReader::open(&v.container_path(id), &fsk)
            .expect("open")
            .meta
            .chunks
            .clone()
    }

    /// files/ 下残留的暂存容器文件名。
    fn staged_names(v: &Vault) -> Vec<String> {
        std::fs::read_dir(v.data_dir.join("files"))
            .expect("read dir")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".vse.new"))
            .collect()
    }

    /// 造一个「阶段一/二已完成、阶段三（rename 落位）之前」的崩溃状态：
    /// 正式容器仍是旧密钥产物，暂存容器是当前索引密钥产物，索引里带轮换标记。
    fn make_pending_rotation(v: &mut Vault, id: u64, tag: &str) -> PathBuf {
        let folder = v.index.file_folder(id).expect("folder");
        let meta_key = v.fsk(folder).expect("fsk");
        let (name, created, modified) = {
            let rd = ContainerReader::open(&v.container_path(id), &meta_key).expect("open");
            (
                rd.meta.name.clone(),
                rd.meta.created_ms,
                rd.meta.modified_ms,
            )
        };
        let plain = v.data_dir.join(format!("stage-plain-{id}.bin"));
        v.export_file(id, &plain).expect("export plaintext");
        let new_fskey = random_key();
        let staged = v.staged_container_path(id, tag);
        v.encrypt_plain_to_container(
            &plain, &staged, &name, created, modified, &meta_key, &new_fskey, false,
        )
        .expect("stage");
        std::fs::remove_file(&plain).ok();
        v.index
            .set_file_fskey(id, &vault_crypto::hex_encode(&new_fskey))
            .expect("set fskey");
        v.index.mark_staging(id, tag);
        v.save_index().expect("save index");
        staged
    }

    #[test]
    fn rotate_file_key_isolates_file_and_kills_old_key() {
        let (dir, mut v) = fresh_vault("rot1");
        let id1 = v
            .import_file(&write_src(dir.path(), "one.txt", &b"alpha".repeat(4000)), 0)
            .expect("i1");
        let id2 = v
            .import_file(&write_src(dir.path(), "two.txt", &b"beta".repeat(3000)), 0)
            .expect("i2");

        let pre1 = dir.path().join("pre1.bin");
        let pre2 = dir.path().join("pre2.bin");
        v.export_file(id1, &pre1).expect("e1");
        v.export_file(id2, &pre2).expect("e2");
        let bytes2 = container_bytes(&v, id2);
        let chunks2 = chunk_entries(&v, id2);
        let old_hex = v.fskey_hex_for(id1).expect("old fskey hex");
        let old_key = vault_crypto::kdf::hkdf_sha256_derive(
            &v.fsk(0).expect("fsk")[..],
            format!("fskey/{id1}").as_bytes(),
        );
        assert_eq!(v.index.file_fskey(id1).expect("row"), None); // 本机文件原本无覆盖

        let new_hex = v.rotate_file_key(id1).expect("rotate");
        assert_ne!(new_hex, old_hex);
        assert_eq!(new_hex.len(), 64);
        assert_eq!(v.index.file_fskey(id1).expect("row"), Some(new_hex.clone()));

        // 内容逐字节一致
        let post1 = dir.path().join("post1.bin");
        v.export_file(id1, &post1).expect("export after");
        assert_eq!(
            std::fs::read(&post1).expect("read"),
            std::fs::read(&pre1).expect("read")
        );
        assert_eq!(std::fs::read(&pre1).expect("read"), b"alpha".repeat(4000));

        // 其它文件完全不受影响：容器字节与块清单逐字段相同
        assert_eq!(container_bytes(&v, id2), bytes2);
        assert_eq!(chunk_entries(&v, id2), chunks2);
        assert_eq!(v.index.file_fskey(id2).expect("row"), None);
        let post2 = dir.path().join("post2.bin");
        v.export_file(id2, &post2).expect("export after");
        assert_eq!(
            std::fs::read(&post2).expect("read"),
            std::fs::read(&pre2).expect("read")
        );

        // 旧 FSKey 无法解出块：元数据仍可开（FSK 未变），但块 GCM 解不开/校验不过
        let cpath = v.container_path(id1);
        let rd = ContainerReader::open(&cpath, &v.fsk(0).expect("fsk")).expect("meta opens");
        assert!(rd.export_to(&dir.path().join("old.bin"), &old_key).is_err());
        assert!(rd.verify(&old_key).is_err());
        assert!(v.verify_file(id1).is_ok()); // 新 FSKey 正常

        // 无残留：标记已清除、暂存文件不剩
        assert_eq!(v.index.staging_tag(id1), None);
        assert!(staged_names(&v).is_empty());

        // 不存在的 id / 容器缺失 → 明确 Err
        assert_eq!(v.rotate_file_key(9999).unwrap_err(), "file not found");
        std::fs::remove_file(&cpath).ok();
        assert_eq!(v.rotate_file_key(id1).unwrap_err(), "container not found");

        // 已是「同步来的」形态（索引内已有 FSKey 覆盖）的文件同样可轮换
        let id2_derived = v.fskey_hex_for(id2).expect("derived hex");
        v.index
            .set_file_fskey(id2, &id2_derived)
            .expect("set override");
        v.save_index().expect("save");
        assert_eq!(
            container_bytes(&v, id2),
            bytes2,
            "覆盖值等于派生值：容器无需重写"
        );
        let id2_new = v.rotate_file_key(id2).expect("rotate override file");
        assert_ne!(id2_new, id2_derived);
        assert_eq!(v.index.file_fskey(id2).expect("row"), Some(id2_new));
        let post2b = dir.path().join("post2b.bin");
        v.export_file(id2, &post2b).expect("export");
        assert_eq!(
            std::fs::read(&post2b).expect("read"),
            std::fs::read(&pre2).expect("read")
        );
    }

    #[test]
    fn rotate_folder_keys_rewrites_folder_only() {
        let (dir, mut v) = fresh_vault("rot2");
        let a = v.mkdir(0, "A").expect("mkdir A");
        let b = v.mkdir(0, "B").expect("mkdir B");
        let sub = v.mkdir(a, "sub").expect("mkdir sub");
        let ida1 = v
            .import_file(&write_src(dir.path(), "a1.txt", &b"a1".repeat(5000)), a)
            .expect("ia1");
        let ida2 = v
            .import_file(&write_src(dir.path(), "a2.bin", &[7u8; 90000]), a)
            .expect("ia2");
        let idsub = v
            .import_file(&write_src(dir.path(), "s.txt", b"sub"), sub)
            .expect("isub");
        let idb = v
            .import_file(&write_src(dir.path(), "b.txt", b"bbb"), b)
            .expect("ib");

        let old_fsk_a = v.fsk(a).expect("fsk a");
        let pre: Vec<PathBuf> = [ida1, ida2]
            .iter()
            .map(|id| {
                let p = dir.path().join(format!("pre-{id}.bin"));
                v.export_file(*id, &p).expect("pre export");
                p
            })
            .collect();
        let bytes_sub = container_bytes(&v, idsub);
        let bytes_b = container_bytes(&v, idb);

        let n = v.rotate_folder_keys(a).expect("rotate folder");
        assert_eq!(n, 2); // 只重写直属文件，不含子文件夹

        // 文件夹内所有文件仍可正确导出（逐字节一致）
        for (id, pre_path) in [ida1, ida2].iter().zip(&pre) {
            let out = dir.path().join(format!("post-{id}.bin"));
            v.export_file(*id, &out).expect("post export");
            assert_eq!(
                std::fs::read(&out).expect("read"),
                std::fs::read(pre_path).expect("read")
            );
        }
        // 每个文件都有新随机 FSKey 覆盖，且与旧派生键不同
        for id in [ida1, ida2] {
            let row = v.index.file_fskey(id).expect("row").expect("override");
            assert_eq!(row.len(), 64);
            assert_ne!(
                row,
                vault_crypto::hex_encode(&*vault_crypto::kdf::hkdf_sha256_derive(
                    &old_fsk_a[..],
                    format!("fskey/{id}").as_bytes(),
                ))
            );
        }
        // 文件夹外（含子文件夹）文件容器字节不变
        assert_eq!(container_bytes(&v, idb), bytes_b);
        assert_eq!(container_bytes(&v, idsub), bytes_sub);
        // 旧 FSK 无法再开该文件夹内任一容器的元数据
        assert!(ContainerReader::open(&v.container_path(ida1), &old_fsk_a).is_err());
        assert!(ContainerReader::open(&v.container_path(ida2), &old_fsk_a).is_err());
        // Folder.fskey 覆盖已写入，且新 FSK 与旧派生值不同
        let fsk_hex = v.index.folder_fsk(a).expect("row").expect("override");
        assert_eq!(fsk_hex.len(), 64);
        assert_ne!(*v.fsk(a).expect("fsk"), *old_fsk_a);
        assert_eq!(v.index.folder_fsk(sub).expect("row"), None);
        assert!(staged_names(&v).is_empty());

        // 空文件夹：幂等成功、返回 0，仍写入新的 FSK 覆盖
        let empty = v.mkdir(0, "empty").expect("mkdir empty");
        assert_eq!(v.rotate_folder_keys(empty).expect("rotate empty"), 0);
        assert!(v.index.folder_fsk(empty).expect("row").is_some());
        // 不存在的文件夹 → 明确 Err；根文件夹（id=0）是文件夹，有直属文件时同样允许并生效
        assert_eq!(v.rotate_folder_keys(9999).unwrap_err(), "folder not found");
        let idroot = v
            .import_file(&write_src(dir.path(), "root.txt", b"root-level"), 0)
            .expect("iroot");
        assert_eq!(v.rotate_folder_keys(0).expect("rotate root"), 1);
        assert!(v.index.folder_fsk(0).expect("row").is_some());
        let out = dir.path().join("root.out");
        v.export_file(idroot, &out).expect("export root file");
        assert_eq!(std::fs::read(&out).expect("read"), b"root-level");
        // 根轮换不影响其它文件夹（子文件夹用各自独立派生的 FSK）
        assert_eq!(container_bytes(&v, idb), bytes_b);
        assert_eq!(container_bytes(&v, idsub), bytes_sub);
    }

    #[test]
    fn rekey_all_switches_mk_and_preserves_content() {
        let dir = tempfile::tempdir().expect("tmp");
        let vault_file = dir.path().join("rekey.vsvb");
        std::fs::write(&vault_file, b"stub-header").expect("stub");
        let old_mk = random_key();
        let (v_ik, v_sk) = sub_keys_for_test(&old_mk);
        let mut v = Vault::open(&vault_file, &old_mk, &v_ik, &v_sk).expect("open");

        let sub = v.mkdir(0, "sub").expect("mkdir");
        let ids: Vec<u64> = [
            // 内容含分隔符，使内容采样能产出 "root" 令牌（名称令牌只有 txt）
            (
                "r.txt",
                b"root report
"
                .repeat(3000)
                .to_vec(),
                0u64,
            ),
            ("s.bin", vec![9u8; 200_000], sub),
            ("empty.bin", Vec::new(), 0),
        ]
        .iter()
        .map(|(name, data, folder)| {
            v.import_file(&write_src(dir.path(), name, data), *folder)
                .expect("import")
        })
        .collect();
        let pre: Vec<PathBuf> = ids
            .iter()
            .map(|id| {
                let p = dir.path().join(format!("pre-{id}.bin"));
                v.export_file(*id, &p).expect("pre export");
                p
            })
            .collect();
        let (share_id, token) = v.create_share(ids[0], 3600, 1).expect("share");
        assert!(v.search("root").contains("r.txt"));

        let new_mk = random_key();
        let n = v.rekey_all(&new_mk).expect("rekey");
        assert_eq!(n, ids.len());

        // P7-2 语义：从属密钥不随 MK 轮换 → 重开用与创建时相同的一组（来自旧 MK 冻结公式）；
        // 新 MK 只改变 FSK 派生与 MK 包装，索引/搜索/分享全部照旧
        let (v2_ik, v2_sk) = sub_keys_for_test(&old_mk);
        let v2 = Vault::open(&vault_file, &new_mk, &v2_ik, &v2_sk).expect("open with new mk");
        for (id, pre_path) in ids.iter().zip(&pre) {
            let out = dir.path().join(format!("post-{id}.bin"));
            v2.export_file(*id, &out).expect("post export");
            assert_eq!(
                std::fs::read(&out).expect("read"),
                std::fs::read(pre_path).expect("read")
            );
        }
        assert!(v2.search("root").contains("r.txt"));
        // 错误 MK（配错误从属密钥）打开必须失败：容器元数据 FSK 解不开
        let (ek, es) = sub_keys_for_test(&[0u8; KEY_LEN]);
        let err = Vault::open(&vault_file, &[0u8; KEY_LEN], &ek, &es)
            .err()
            .expect("must fail");
        assert!(!err.is_empty(), "wrong credentials must not open");
        // P7-2 直接收益断言：既有分享令牌在 MK 轮换后**仍然有效**
        let (v3_ik, v3_sk) = sub_keys_for_test(&old_mk);
        let mut v3 = Vault::open(&vault_file, &new_mk, &v3_ik, &v3_sk).expect("open");
        // P7-2：分享票据跨 MK 轮换有效 → 打开成功（V1.0 在此处必须作废重发）
        v3.open_share(share_id, &token, &dir.path().join("burn"))
            .expect("share must survive mk rotation");
        // 传入与当前相同的 MK → 明确 Err（不做幂等空转）
        assert_eq!(
            v3.rekey_all(&new_mk).unwrap_err(),
            "new mk equals current mk"
        );
        // 轮换后仍可正常导入导出
        let extra = v3
            .import_file(&write_src(dir.path(), "extra.txt", b"after rekey"), 0)
            .expect("import after rekey");
        let out = dir.path().join("extra.out");
        v3.export_file(extra, &out).expect("export after rekey");
        assert_eq!(std::fs::read(&out).expect("read"), b"after rekey");
        assert!(staged_names(&v3).is_empty());
    }

    /// 崩溃窗口：阶段二已提交（索引标记 + 新密钥）、阶段三（rename 落位）未完成时，
    /// 读路径必须取暂存容器；重启后该状态仍可读，下一次轮换把它收敛到正式路径。
    #[test]
    fn staged_container_wins_and_survives_restart() {
        let dir = tempfile::tempdir().expect("tmp");
        let vault_file = dir.path().join("pending.vsvb");
        std::fs::write(&vault_file, b"stub-header").expect("stub");
        let mk = random_key();
        let (v_ik, v_sk) = sub_keys_for_test(&mk);
        let mut v = Vault::open(&vault_file, &mk, &v_ik, &v_sk).expect("open");
        let content = b"pending-rotation-payload".repeat(600);
        let id = v
            .import_file(&write_src(dir.path(), "p.txt", &content), 0)
            .expect("import");
        let canonical_before = container_bytes(&v, id);

        let staged = make_pending_rotation(&mut v, id, "deadbeef");
        assert!(staged.exists() && v.index.staging_tag(id).as_deref() == Some("deadbeef"));
        // 正式容器未被替换（仍是旧密钥产物），但读路径已改用暂存容器
        assert_eq!(container_bytes(&v, id), canonical_before);
        let out = dir.path().join("pending.out");
        v.export_file(id, &out)
            .expect("export via staged container");
        assert_eq!(std::fs::read(&out).expect("read"), content);

        // 重启（Vault::open 的清扫不得删掉被标记引用的暂存容器）
        drop(v);
        let (v_ik, v_sk) = sub_keys_for_test(&mk);
        let mut v = Vault::open(&vault_file, &mk, &v_ik, &v_sk).expect("reopen");
        assert!(staged.exists());
        let out2 = dir.path().join("pending2.out");
        v.export_file(id, &out2).expect("export after restart");
        assert_eq!(std::fs::read(&out2).expect("read"), content);
        assert!(v.verify_file(id).is_ok());
        assert!(v.sync_manifest().expect("manifest").contains("p.txt"));

        // 下一次轮换收敛：落位到正式路径、清标记、旧暂存文件成为无标记残骸（下次 open 清扫）
        v.rotate_file_key(id).expect("rotate again");
        assert_eq!(v.index.staging_tag(id), None);
        assert_ne!(container_bytes(&v, id), canonical_before);
        let out3 = dir.path().join("pending3.out");
        v.export_file(id, &out3).expect("export after convergence");
        assert_eq!(std::fs::read(&out3).expect("read"), content);
        drop(v);
        let (v_ik, v_sk) = sub_keys_for_test(&mk);
        let v = Vault::open(&vault_file, &mk, &v_ik, &v_sk).expect("reopen after convergence");
        assert!(staged_names(&v).is_empty());
        let out4 = dir.path().join("pending4.out");
        v.export_file(id, &out4).expect("export after sweep");
        assert_eq!(std::fs::read(&out4).expect("read"), content);
    }

    /// 崩溃残骸清扫：无标记引用的暂存容器、assemble 中间产物与**明文临时文件**必须删除。
    #[test]
    fn open_sweeps_unreferenced_temp_files() {
        let dir = tempfile::tempdir().expect("tmp");
        let vault_file = dir.path().join("sweep.vsvb");
        std::fs::write(&vault_file, b"stub-header").expect("stub");
        let mk = random_key();
        let (v_ik, v_sk) = sub_keys_for_test(&mk);
        let mut v = Vault::open(&vault_file, &mk, &v_ik, &v_sk).expect("open");
        let id = v
            .import_file(&write_src(dir.path(), "keep.txt", b"payload"), 0)
            .expect("import");
        let orphan = v.staged_container_path(id, "deadbeef");
        std::fs::write(&orphan, b"junk").expect("orphan");
        let half = v.data_dir.join("files").join(format!("{id}.vse.tmp"));
        std::fs::write(&half, b"junk").expect("tmp");
        let plain = v.data_dir.join("rotate-1.plain.tmp");
        std::fs::write(&plain, b"plaintext").expect("plain");
        drop(v);

        let (v_ik, v_sk) = sub_keys_for_test(&mk);
        let v = Vault::open(&vault_file, &mk, &v_ik, &v_sk).expect("reopen");
        assert!(!orphan.exists());
        assert!(!half.exists());
        assert!(!plain.exists());
        let out = dir.path().join("keep.out");
        v.export_file(id, &out).expect("export");
        assert_eq!(std::fs::read(&out).expect("read"), b"payload");
    }

    // ==== P6-3 证据测试（VSIX v3 分段化；docs/v2.0/09 §三 P6-3）====

    /// P6-3 证据：index_v3::append_is_atomic
    /// 未走提交协议的内存变更（含未 flush 的 store 操作）绝不部分可见：
    /// 要么整批随提交点可见，要么整批不可见。
    #[test]
    fn index_v3_append_is_atomic() {
        let dir = tempfile::tempdir().expect("tmp");
        let vault_file = dir.path().join("v3atomic.vsvb");
        std::fs::write(&vault_file, b"stub").expect("stub");
        let mk = random_key();
        let (v_ik, v_sk) = sub_keys_for_test(&mk);
        let mut v = Vault::open(&vault_file, &mk, &v_ik, &v_sk).expect("open");
        assert!(v.store.is_some(), "new vault must use v3 segmented path");
        let id1 = v
            .import_file(&write_src(dir.path(), "a.txt", b"committed"), 0)
            .expect("import committed");

        // 模拟「提交点之前崩溃」：直接改内存索引 + 注入未 flush 的 store 操作
        let _ = v.index.mkdir(0, "uncommitted-folder");
        v.store
            .as_mut()
            .expect("v3 store")
            .kv_apply(vec![(true, 9_999_999u64, b"uncommitted".to_vec())])
            .expect("apply");
        let path = vault_file.clone();
        let data_dir = v.data_dir.clone();
        std::mem::forget(v); // 跳过 Drop 的自动 flush = 等价掉电

        let (v2_ik, v2_sk) = sub_keys_for_test(&mk);
        let v2 = Vault::open(&path, &mk, &v2_ik, &v2_sk).expect("reopen");
        assert!(v2.has_file(id1), "committed entry must survive");
        assert!(
            !v2.list_children(0)
                .expect("list")
                .contains("uncommitted-folder"),
            "uncommitted index change must be invisible"
        );
        assert!(
            v2.store.as_ref().expect("v3").kv_get(9_999_999).is_none(),
            "uncommitted store op must be invisible"
        );
        let _ = data_dir;
    }

    /// P6-3 证据：index_v3::crash_between_rename_and_manifest
    /// manifest 只是缓存：删除清单后磁盘扫描仍完整还原索引。
    #[test]
    fn index_v3_crash_between_rename_and_manifest() {
        let dir = tempfile::tempdir().expect("tmp");
        let vault_file = dir.path().join("v3mf.vsvb");
        std::fs::write(&vault_file, b"stub").expect("stub");
        let mk = random_key();
        let (v_ik, v_sk) = sub_keys_for_test(&mk);
        let mut v = Vault::open(&vault_file, &mk, &v_ik, &v_sk).expect("open");
        let id = v
            .import_file(&write_src(dir.path(), "m.txt", b"payload"), 0)
            .expect("import");
        drop(v);

        // 等价于「清单更新（六步协议第 6 步）永远没发生」
        let manifest = dir.path().join("manifest.index.vssg");
        if manifest.exists() {
            std::fs::remove_file(&manifest).expect("remove manifest");
        }
        let (v2_ik, v2_sk) = sub_keys_for_test(&mk);
        let v2 = Vault::open(&vault_file, &mk, &v2_ik, &v2_sk).expect("reopen without manifest");
        assert!(
            v2.has_file(id),
            "scan is truth: entry must survive manifest loss"
        );
        assert_eq!(v2.file_name(id).expect("name"), "m.txt");
    }

    /// P6-3 证据：index_v3::legacy_v2_readonly_then_migrate
    /// 双路径共存：v0.5.0 旧库（index.enc）保持 legacy 读写语义不动，
    /// 新库走 v3；迁移本体是 P7-9。
    #[test]
    fn index_v3_legacy_v2_readonly_then_migrate() {
        let dir = tempfile::tempdir().expect("tmp");
        let vault_file = dir.path().join("legacy.vsvb");
        std::fs::write(&vault_file, b"stub").expect("stub");
        let mk = random_key();
        // 手工构造 legacy 库：写一个 VSIX v2 index.enc
        let search_key = *vault_crypto::kdf::hkdf_sha256_derive(&mk, b"search");
        let ix = VaultIndex::new(search_key);
        let index_key = *vault_crypto::kdf::hkdf_sha256_derive(&mk, b"vault-index");
        let blob = ix.to_encrypted(&index_key).expect("enc");
        std::fs::create_dir_all(dir.path().join("legacy.data").join("files")).expect("mkdir");
        std::fs::write(dir.path().join("legacy.data").join("index.enc"), blob)
            .expect("write v2 index");

        let (v_ik, v_sk) = sub_keys_for_test(&mk);
        let mut v = Vault::open(&vault_file, &mk, &v_ik, &v_sk).expect("open legacy");
        assert!(v.store.is_none(), "legacy vault must use v2 path");
        let id = v
            .import_file(&write_src(dir.path(), "old.txt", b"legacy data"), 0)
            .expect("import on legacy");
        assert!(dir.path().join("legacy.data").join("index.enc").exists());
        assert!(
            !dir.path().join("legacy.data").join("index").exists(),
            "legacy vault must not create v3 segment dir"
        );
        let out = dir.path().join("old.out");
        v.export_file(id, &out).expect("export legacy");
        assert_eq!(std::fs::read(&out).expect("read"), b"legacy data");
        drop(v);

        // 新库则走 v3：有分段目录与 .vssg 段文件，无 index.enc
        let vault_file2 = dir.path().join("fresh.vsvb");
        std::fs::write(&vault_file2, b"stub").expect("stub");
        let (v3_ik, v3_sk) = sub_keys_for_test(&mk);
        let mut v3 = Vault::open(&vault_file2, &mk, &v3_ik, &v3_sk).expect("open fresh");
        assert!(v3.store.is_some());
        let _ = v3.mkdir(0, "d").expect("mkdir");
        assert!(dir.path().join("fresh.data").join("index").exists());
        let has_vssg = std::fs::read_dir(dir.path().join("fresh.data").join("index"))
            .expect("dir")
            .flatten()
            .any(|e| e.file_name().to_string_lossy().ends_with(".vssg"));
        assert!(has_vssg, "v3 vault must persist via VSSG segments");
    }
}
