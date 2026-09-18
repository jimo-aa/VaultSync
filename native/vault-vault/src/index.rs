//! 保险箱索引：文件夹树 / 文件条目 / 标签 / 加密倒排搜索索引（docs/05-02 F-04~F-06）。
//!
//! 持久化形态：整体 JSON → AEAD 加密（索引密钥 = HKDF(MK,"vault-index")）落盘；
//! 搜索令牌 = HMAC-SHA256(搜索密钥=HKDF(MK,"search"), 归一化词)（SSE-lite，docs/05-02 §4.4）。
//! 明文索引只在内存，落盘前序列化缓冲用后即弃。
//!
//! P3 同步扩展（向后兼容，serde default）：
//! - `FileEntry.fskey`：同步文件的 FSKey 覆盖（hex）。本机导入为 None（按 MK 派生）；对端同步来
//!   的文件携带发送方 FSKey（仅经 E2E 信道），使密文块可原样复用（加密块=同步块=传输块）。
//! - `FileEntry.vc`：per-file 向量时钟（device_id → 计数），因果识别与冲突判定（docs/05-03 §6.2）。
//! - `IndexData.tombstones`：删除墓碑，同步时传播删除语义（删除优先，docs/05-03 §6.2）。
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use serde::{Deserialize, Serialize};
use vault_crypto::KEY_LEN;

pub const ROOT_FOLDER: u64 = 0;
pub const INDEX_MAGIC: &[u8; 4] = b"VSIX";
pub const INDEX_FORMAT_VER: u16 = 2;

const MAX_TOKENS_PER_FILE: usize = 10000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Folder {
    pub name: String,
    pub parent: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileEntry {
    pub folder: u64,
    pub name: String,
    pub size: u64,
    pub tags: Vec<String>,
    pub created_ms: u64,
    pub modified_ms: u64,
    /// FSKey 覆盖（hex，32B）。None = 按 MK 派生（本机导入）；Some = 同步来的文件。
    #[serde(default)]
    pub fskey: Option<String>,
    /// 向量时钟（device_id → 计数）。
    #[serde(default)]
    pub vc: BTreeMap<String, u64>,
}

/// 删除墓碑：同步时向对端传播"该 id 已删除"及其因果历史。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tombstone {
    pub deleted_ms: u64,
    pub vc: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Share {
    pub file_id: u64,
    /// 随机令牌的 HMAC（令牌明文只在创建时返回一次）
    pub token_hash: String,
    pub expires_ms: u64,
    pub max_opens: u32,
    pub opens: u32,
}

#[derive(Default, Serialize, Deserialize)]
struct IndexData {
    folders: BTreeMap<u64, Folder>,
    files: BTreeMap<u64, FileEntry>,
    shares: BTreeMap<u64, Share>,
    /// token_hex -> file_ids
    search: HashMap<String, BTreeSet<u64>>,
    /// 已删除文件墓碑（同步删除语义）
    #[serde(default)]
    tombstones: BTreeMap<u64, Tombstone>,
    next_id: u64,
}

pub struct VaultIndex {
    d: IndexData,
    search_key: [u8; KEY_LEN],
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn is_cjk(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c) || ('\u{3400}'..='\u{4DBF}').contains(&c)
}

/// 分词：拉丁按非字母数字切分（小写，≥2 字符）；CJK 连续段滑窗 bigram。
/// 上限 10000 词/文件，防索引膨胀。
pub fn tokenize(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut out: Vec<String> = Vec::new();
    let mut latin = String::new();
    let mut cjk: Vec<char> = Vec::new();

    macro_rules! flush_latin {
        () => {
            if latin.chars().count() >= 2 {
                out.push(latin.clone());
            }
            latin.clear();
        };
    }
    macro_rules! flush_cjk {
        () => {
            if cjk.len() >= 2 {
                for w in cjk.windows(2) {
                    out.push(w.iter().collect::<String>());
                }
            } else if cjk.len() == 1 {
                out.push(cjk[0].to_string());
            }
            cjk.clear();
        };
    }

    for c in lower.chars() {
        if out.len() >= MAX_TOKENS_PER_FILE {
            break;
        }
        if is_cjk(c) {
            flush_latin!();
            cjk.push(c);
        } else {
            flush_cjk!();
            if c.is_ascii_alphanumeric() {
                latin.push(c);
            } else {
                flush_latin!();
            }
        }
    }
    flush_cjk!();
    flush_latin!();
    out
}

impl VaultIndex {
    pub fn new(search_key: [u8; KEY_LEN]) -> Self {
        let mut d = IndexData {
            folders: BTreeMap::new(),
            files: BTreeMap::new(),
            shares: BTreeMap::new(),
            search: HashMap::new(),
            tombstones: BTreeMap::new(),
            next_id: 1,
        };
        d.folders.insert(
            ROOT_FOLDER,
            Folder {
                name: String::new(),
                parent: 0,
            },
        );
        Self { d, search_key }
    }

    fn hmac_token(&self, token: &str) -> String {
        vault_crypto::hmac_sha256_hex(&self.search_key, token.as_bytes())
    }

    /// 供编排层使用（分享令牌哈希）。
    pub fn token_hash(&self, token: &str) -> String {
        self.hmac_token(token)
    }

    /// 预分配 id（FSKey 派生需要 file_id，先于加密分配）。
    pub fn alloc_id(&mut self) -> u64 {
        let id = self.d.next_id;
        self.d.next_id += 1;
        id
    }

    /// 以预分配 id 登记文件。
    pub fn add_file_with_id(
        &mut self,
        id: u64,
        folder: u64,
        name: &str,
        size: u64,
        tokens: &[String],
    ) -> Result<(), &'static str> {
        if !self.d.folders.contains_key(&folder) {
            return Err("folder not found");
        }
        if name.is_empty() {
            return Err("invalid file name");
        }
        if self.d.files.contains_key(&id) {
            return Err("duplicate file id");
        }
        let now = now_ms();
        self.d.files.insert(
            id,
            FileEntry {
                folder,
                name: name.to_string(),
                size,
                tags: Vec::new(),
                created_ms: now,
                modified_ms: now,
                fskey: None,
                vc: BTreeMap::new(),
            },
        );
        self.index_tokens(id, tokens);
        Ok(())
    }

    fn index_tokens(&mut self, file_id: u64, tokens: &[String]) {
        for t in tokens {
            let hex = self.hmac_token(t);
            self.d.search.entry(hex).or_default().insert(file_id);
        }
    }

    fn deindex_name(&mut self, file_id: u64, name: &str) {
        for t in tokenize(name) {
            let hex = self.hmac_token(&t);
            if let Some(ids) = self.d.search.get_mut(&hex) {
                ids.remove(&file_id);
                if ids.is_empty() {
                    self.d.search.remove(&hex);
                }
            }
        }
    }

    pub fn mkdir(&mut self, parent: u64, name: &str) -> Result<u64, &'static str> {
        if !self.d.folders.contains_key(&parent) {
            return Err("parent folder not found");
        }
        if name.is_empty() || name.contains('/') || name.contains('\\') {
            return Err("invalid folder name");
        }
        let id = self.alloc_id();
        self.d.folders.insert(
            id,
            Folder {
                name: name.to_string(),
                parent,
            },
        );
        Ok(id)
    }

    pub fn rename_folder(&mut self, id: u64, name: &str) -> Result<(), &'static str> {
        if id == ROOT_FOLDER {
            return Err("cannot rename root");
        }
        let f = self.d.folders.get_mut(&id).ok_or("folder not found")?;
        if name.is_empty() {
            return Err("invalid folder name");
        }
        f.name = name.to_string();
        Ok(())
    }

    pub fn rename_file(&mut self, id: u64, name: &str) -> Result<(), &'static str> {
        if name.is_empty() {
            return Err("invalid file name");
        }
        let old_name = {
            let f = self.d.files.get_mut(&id).ok_or("file not found")?;
            std::mem::replace(&mut f.name, name.to_string())
        };
        // 名称令牌重索引
        self.deindex_name(id, &old_name);
        self.index_tokens(id, &tokenize(name));
        Ok(())
    }

    /// 导入登记：tokens 由调用方生成（名称+标签+内容采样）。
    #[allow(clippy::too_many_arguments)]
    pub fn add_file(
        &mut self,
        folder: u64,
        name: &str,
        size: u64,
        tokens: &[String],
    ) -> Result<u64, &'static str> {
        if !self.d.folders.contains_key(&folder) {
            return Err("folder not found");
        }
        if name.is_empty() {
            return Err("invalid file name");
        }
        let id = self.alloc_id();
        let now = now_ms();
        self.d.files.insert(
            id,
            FileEntry {
                folder,
                name: name.to_string(),
                size,
                tags: Vec::new(),
                created_ms: now,
                modified_ms: now,
                fskey: None,
                vc: BTreeMap::new(),
            },
        );
        self.index_tokens(id, tokens);
        Ok(id)
    }

    pub fn remove_file(&mut self, id: u64) -> Result<FileEntry, &'static str> {
        let e = self.d.files.remove(&id).ok_or("file not found")?;
        self.deindex_name(id, &e.name);
        for t in &e.tags {
            self.deindex_name(id, t);
        }
        Ok(e)
    }

    /// 递归删除文件夹；返回被删文件 id（容器文件由调用方删除）。
    pub fn remove_folder(&mut self, id: u64) -> Result<Vec<u64>, &'static str> {
        if !self.d.folders.contains_key(&id) {
            return Err("folder not found");
        }
        if id == ROOT_FOLDER {
            return Err("cannot delete root");
        }
        let mut stack = vec![id];
        let mut removed = Vec::new();
        while let Some(cur) = stack.pop() {
            stack.extend(
                self.d
                    .folders
                    .iter()
                    .filter(|(_, f)| f.parent == cur)
                    .map(|(k, _)| *k)
                    .collect::<Vec<_>>(),
            );
            let ids = self
                .d
                .files
                .iter()
                .filter(|(_, f)| f.folder == cur)
                .map(|(k, _)| *k)
                .collect::<Vec<_>>();
            for fid in ids {
                let _ = self.remove_file(fid);
                removed.push(fid);
            }
            self.d.folders.remove(&cur);
        }
        Ok(removed)
    }

    pub fn set_tags(&mut self, id: u64, tags: Vec<String>) -> Result<(), &'static str> {
        let old = {
            let f = self.d.files.get_mut(&id).ok_or("file not found")?;
            std::mem::replace(&mut f.tags, tags)
        };
        for t in &old {
            self.deindex_name(id, t);
        }
        let new_tokens: Vec<String> = self.d.files[&id]
            .tags
            .iter()
            .flat_map(|t| tokenize(t))
            .collect();
        self.index_tokens(id, &new_tokens);
        Ok(())
    }

    /// 列出子文件夹与文件（按名排序，JSON）。
    pub fn list_children(&self, folder: u64) -> Result<String, &'static str> {
        if !self.d.folders.contains_key(&folder) {
            return Err("folder not found");
        }
        let mut folders: Vec<serde_json::Value> = self
            .d
            .folders
            .iter()
            .filter(|(_, f)| f.parent == folder)
            .map(|(id, f)| serde_json::json!({"id": id, "name": f.name}))
            .collect();
        folders.sort_by_key(|v| v["name"].as_str().map(str::to_string).unwrap_or_default());
        let mut files: Vec<serde_json::Value> = self
            .d
            .files
            .iter()
            .filter(|(_, f)| f.folder == folder)
            .map(|(id, f)| {
                serde_json::json!({
                    "id": id, "name": f.name, "size": f.size,
                    "tags": f.tags, "modifiedMs": f.modified_ms,
                })
            })
            .collect();
        files.sort_by_key(|v| v["name"].as_str().map(str::to_string).unwrap_or_default());
        serde_json::to_string(&serde_json::json!({"folders": folders, "files": files}))
            .map_err(|_| "json serialize failed")
    }

    pub fn file_folder(&self, id: u64) -> Result<u64, &'static str> {
        self.d
            .files
            .get(&id)
            .map(|f| f.folder)
            .ok_or("file not found")
    }

    pub fn file_name(&self, id: u64) -> Result<String, &'static str> {
        self.d
            .files
            .get(&id)
            .map(|f| f.name.clone())
            .ok_or("file not found")
    }

    /// 检索：查询分词 → HMAC → 倒排并集（返回脱敏元数据，不含内容片段）。
    pub fn search(&self, query: &str) -> String {
        let mut hits: BTreeSet<u64> = BTreeSet::new();
        for t in tokenize(query) {
            if let Some(ids) = self.d.search.get(&self.hmac_token(&t)) {
                hits.extend(ids.iter().copied());
            }
        }
        let files: Vec<serde_json::Value> = hits
            .into_iter()
            .filter_map(|id| {
                self.d.files.get(&id).map(|f| {
                    serde_json::json!({
                        "id": id, "name": f.name, "size": f.size,
                        "folder": f.folder, "tags": f.tags,
                    })
                })
            })
            .collect();
        serde_json::to_string(&serde_json::json!({"files": files}))
            .unwrap_or_else(|_| "{\"files\":[]}".to_string())
    }

    pub fn create_share(
        &mut self,
        file_id: u64,
        token_hash: String,
        ttl_secs: u64,
        max_opens: u32,
    ) -> Result<u64, &'static str> {
        if !self.d.files.contains_key(&file_id) {
            return Err("file not found");
        }
        let id = self.alloc_id();
        self.d.shares.insert(
            id,
            Share {
                file_id,
                token_hash,
                expires_ms: now_ms() + ttl_secs * 1000,
                max_opens,
                opens: 0,
            },
        );
        Ok(id)
    }

    /// 校验并消耗一次打开机会；成功返回 file_id。
    pub fn consume_share(&mut self, share_id: u64, token: &str) -> Result<u64, &'static str> {
        let token_hash = self.hmac_token(token);
        let sh = self.d.shares.get_mut(&share_id).ok_or("share not found")?;
        if sh.opens >= sh.max_opens {
            return Err("share exhausted");
        }
        if now_ms() > sh.expires_ms {
            return Err("share expired");
        }
        if token_hash != sh.token_hash {
            return Err("share token mismatch");
        }
        sh.opens += 1;
        Ok(sh.file_id)
    }

    // ==== P3 同步支撑（docs/05-03）====

    /// 登记同步来的文件：固定 id（对端权威）、FSKey 覆盖与向量时钟随条目入索引。
    #[allow(clippy::too_many_arguments)]
    pub fn add_remote_file(
        &mut self,
        id: u64,
        folder: u64,
        name: &str,
        size: u64,
        modified_ms: u64,
        fskey_hex: &str,
        vc: BTreeMap<String, u64>,
        tokens: &[String],
    ) -> Result<(), &'static str> {
        if !self.d.folders.contains_key(&folder) {
            return Err("folder not found");
        }
        if name.is_empty() {
            return Err("invalid file name");
        }
        if self.d.files.contains_key(&id) {
            return Err("duplicate file id");
        }
        let now = now_ms();
        // 摄取完成即把墓碑清掉（该 id 复活）
        self.d.tombstones.remove(&id);
        if id >= self.d.next_id {
            self.d.next_id = id + 1;
        }
        self.d.files.insert(
            id,
            FileEntry {
                folder,
                name: name.to_string(),
                size,
                tags: Vec::new(),
                created_ms: modified_ms,
                modified_ms,
                fskey: Some(fskey_hex.to_string()),
                vc,
            },
        );
        self.index_tokens(id, tokens);
        let _ = now;
        Ok(())
    }

    /// 本机内容变更后自增本机向量时钟位。
    pub fn bump_vc(&mut self, id: u64, device: &str) -> Result<(), &'static str> {
        let c = self.d.files.get_mut(&id).ok_or("file not found")?;
        let next = c.vc.get(device).copied().unwrap_or(0) + 1;
        c.vc.insert(device.to_string(), next);
        Ok(())
    }

    /// 向量时钟并集（逐位取 max）。
    pub fn merge_vc(
        &mut self,
        id: u64,
        remote: &BTreeMap<String, u64>,
    ) -> Result<(), &'static str> {
        let c = self.d.files.get_mut(&id).ok_or("file not found")?;
        for (dev, cnt) in remote {
            let slot = c.vc.entry(dev.clone()).or_insert(0);
            if *cnt > *slot {
                *slot = *cnt;
            }
        }
        Ok(())
    }

    pub fn file_vc(&self, id: u64) -> Result<BTreeMap<String, u64>, &'static str> {
        self.d
            .files
            .get(&id)
            .map(|f| f.vc.clone())
            .ok_or("file not found")
    }

    pub fn file_fskey(&self, id: u64) -> Result<Option<String>, &'static str> {
        self.d
            .files
            .get(&id)
            .map(|f| f.fskey.clone())
            .ok_or("file not found")
    }

    pub fn file_size(&self, id: u64) -> Result<u64, &'static str> {
        self.d
            .files
            .get(&id)
            .map(|f| f.size)
            .ok_or("file not found")
    }

    pub fn file_modified(&self, id: u64) -> Result<u64, &'static str> {
        self.d
            .files
            .get(&id)
            .map(|f| f.modified_ms)
            .ok_or("file not found")
    }

    pub fn peek_next_id(&self) -> u64 {
        self.d.next_id
    }

    pub fn tombstones(&self) -> BTreeMap<u64, Tombstone> {
        self.d.tombstones.clone()
    }

    pub fn record_tombstone(&mut self, id: u64, vc: BTreeMap<String, u64>) {
        self.d.tombstones.insert(
            id,
            Tombstone {
                deleted_ms: now_ms(),
                vc,
            },
        );
    }

    /// 全量清单（同步用）：文件条目 + 墓碑，JSON 数组。
    pub fn sync_manifest(&self) -> String {
        let files: Vec<serde_json::Value> = self
            .d
            .files
            .iter()
            .map(|(id, f)| {
                serde_json::json!({
                    "id": id, "folder": f.folder, "name": f.name, "size": f.size,
                    "modifiedMs": f.modified_ms, "vc": f.vc,
                })
            })
            .collect();
        let tombs: Vec<serde_json::Value> = self
            .d
            .tombstones
            .iter()
            .map(|(id, t)| serde_json::json!({"id": id, "deletedMs": t.deleted_ms, "vc": t.vc}))
            .collect();
        serde_json::to_string(&serde_json::json!({"files": files, "tombstones": tombs}))
            .unwrap_or_else(|_| "{\"files\":[],\"tombstones\":[]}".to_string())
    }

    /// 清单文件条目（serde Value 形式，供编排层补块清单字段）。
    pub fn manifest_files(&self) -> Vec<serde_json::Value> {
        self.d
            .files
            .iter()
            .map(|(id, f)| {
                serde_json::json!({
                    "id": id, "folder": f.folder, "name": f.name, "size": f.size,
                    "modifiedMs": f.modified_ms, "vc": f.vc,
                })
            })
            .collect()
    }

    /// 墓碑清单条目（deletedMs 标记删除，docs/05-03 删除同步语义）。
    pub fn manifest_tombstones(&self) -> Vec<serde_json::Value> {
        self.d
            .tombstones
            .iter()
            .map(|(id, t)| {
                serde_json::json!({
                    "id": id, "folder": 0, "name": "", "size": 0,
                    "modifiedMs": t.deleted_ms, "vc": t.vc, "deletedMs": t.deleted_ms,
                })
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.d.files.is_empty()
    }

    /// 递归列出文件夹下全部文件 id（含子文件夹）。
    pub fn files_under(&self, folder: u64) -> Vec<u64> {
        let mut stack = vec![folder];
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        while let Some(cur) = stack.pop() {
            if !seen.insert(cur) {
                continue;
            }
            stack.extend(
                self.d
                    .folders
                    .iter()
                    .filter(|(_, f)| f.parent == cur)
                    .map(|(k, _)| *k),
            );
            out.extend(
                self.d
                    .files
                    .iter()
                    .filter(|(_, f)| f.folder == cur)
                    .map(|(k, _)| *k),
            );
        }
        out
    }

    /// 序列化 + AEAD 加密（索引密钥由调用方提供）。
    pub fn to_encrypted(&self, key: &[u8; KEY_LEN]) -> Result<Vec<u8>, &'static str> {
        let json = serde_json::to_vec(&self.d).map_err(|_| "index serialize failed")?;
        let mut nonce = [0u8; vault_crypto::AES_GCM_NONCE_LEN];
        nonce.copy_from_slice(&vault_crypto::random_bytes(vault_crypto::AES_GCM_NONCE_LEN));
        let ct = vault_crypto::aead::aead_encrypt_with_aad(key, &nonce, &json, INDEX_MAGIC)?;
        drop(json); // 明文缓冲用后即弃
        let mut out = Vec::with_capacity(4 + 2 + vault_crypto::AES_GCM_NONCE_LEN + ct.len());
        out.extend_from_slice(INDEX_MAGIC);
        out.extend_from_slice(&INDEX_FORMAT_VER.to_le_bytes());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct[vault_crypto::AES_GCM_NONCE_LEN..]); // 去 nonce 前缀
        Ok(out)
    }

    pub fn from_encrypted(
        data: &[u8],
        key: &[u8; KEY_LEN],
        search_key: [u8; KEY_LEN],
    ) -> Result<Self, &'static str> {
        if data.len() < 4 + 2 + vault_crypto::AES_GCM_NONCE_LEN + 16 {
            return Err("index truncated");
        }
        if &data[0..4] != INDEX_MAGIC {
            return Err("bad index magic");
        }
        let ver = u16::from_le_bytes(data[4..6].try_into().map_err(|_| "truncated")?);
        if ver > INDEX_FORMAT_VER {
            return Err("index format newer than engine");
        }
        let mut nonce = [0u8; vault_crypto::AES_GCM_NONCE_LEN];
        nonce.copy_from_slice(&data[6..6 + vault_crypto::AES_GCM_NONCE_LEN]);
        let ct = &data[6 + vault_crypto::AES_GCM_NONCE_LEN..];
        let mut blob = nonce.to_vec();
        blob.extend_from_slice(ct);
        let json = vault_crypto::aead::aead_decrypt_with_aad(key, &blob, INDEX_MAGIC)
            .ok_or("index decrypt failed (wrong key or tampered)")?;
        let mut d: IndexData = serde_json::from_slice(&json).map_err(|_| "index parse failed")?;
        d.folders.entry(ROOT_FOLDER).or_insert(Folder {
            name: String::new(),
            parent: 0,
        });
        Ok(Self { d, search_key })
    }

    pub fn save(&self, path: &Path, key: &[u8; KEY_LEN]) -> Result<(), &'static str> {
        let blob = self.to_encrypted(key)?;
        std::fs::write(path, blob).map_err(|_| "cannot write index")
    }

    pub fn load(
        path: &Path,
        key: &[u8; KEY_LEN],
        search_key: [u8; KEY_LEN],
    ) -> Result<Self, &'static str> {
        let data = std::fs::read(path).map_err(|_| "cannot read index")?;
        Self::from_encrypted(&data, key, search_key)
    }
}

/// 为导入内容生成令牌集（名称 + 标签 + 文本内容采样由调用方拼合后调用）。
pub fn content_tokens(text: &str) -> Vec<String> {
    tokenize(text)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn key() -> [u8; KEY_LEN] {
        vault_crypto::random_key()
    }

    #[test]
    fn folder_tree_and_list() {
        let mut ix = VaultIndex::new(key());
        let docs = ix.mkdir(ROOT_FOLDER, "文档").expect("mkdir");
        let work = ix.mkdir(docs, "2026").expect("mkdir");
        assert!(ix
            .list_children(ROOT_FOLDER)
            .expect("list")
            .contains("文档"));
        let id = ix.add_file(docs, "报告.txt", 10, &[]).expect("add");
        assert_eq!(ix.file_folder(id).expect("folder"), docs);
        let removed = ix.remove_folder(docs).expect("rm");
        assert_eq!(removed.len(), 1);
        assert!(ix.file_folder(id).is_err());
        let _ = work;
    }

    #[test]
    fn encrypted_index_roundtrip_and_wrong_key() {
        let k = key();
        let mut ix = VaultIndex::new(k);
        let _ = ix.mkdir(ROOT_FOLDER, "a").expect("mkdir");
        let blob = ix.to_encrypted(&k).expect("enc");
        let ix2 = VaultIndex::from_encrypted(&blob, &k, k).expect("dec");
        assert!(ix2.list_children(ROOT_FOLDER).expect("l").contains("a"));
        assert!(VaultIndex::from_encrypted(&blob, &key(), k).is_err());
    }

    #[test]
    fn tokenize_latin_and_cjk() {
        let t = tokenize("Hello World 密钥轮换 key-rotation");
        assert!(t.contains(&"hello".to_string()));
        assert!(t.contains(&"world".to_string()));
        assert!(t.contains(&"key".to_string()));
        assert!(t.contains(&"rotation".to_string()));
        assert!(t.contains(&"密钥".to_string()));
        assert!(t.contains(&"钥轮".to_string()));
    }

    #[test]
    fn search_name_rename_and_tags() {
        let mut ix = VaultIndex::new(key());
        let id = ix
            .add_file(0, "设计文档.txt", 1, &tokenize("设计文档.txt"))
            .expect("add1");
        let id2 = ix
            .add_file(0, "photo.dat", 1, &tokenize("photo.dat"))
            .expect("add2");
        assert!(ix.search("设计").contains("设计文档"));
        assert!(ix.search("photo").contains("photo"));
        assert!(!ix.search("不存在的词").contains("id"));

        // 重命名后旧名不再命中、新名命中
        ix.rename_file(id, "会议纪要.txt").expect("rename");
        assert!(!ix.search("设计文档").contains("\"id\""));
        assert!(ix.search("会议纪要").contains("会议纪要"));

        // 标签入索引
        ix.set_tags(id2, vec!["设计".into()]).expect("tags");
        assert!(ix.search("设计").contains("photo"));
    }

    #[test]
    fn share_consume_semantics() {
        let mut ix = VaultIndex::new(key());
        let fid = ix.add_file(0, "secret.txt", 1, &[]).expect("add");
        let sh = ix
            .create_share(fid, ix.hmac_token("tok-abc"), 3600, 2)
            .expect("share");
        assert_eq!(ix.consume_share(sh, "tok-abc").expect("open1"), fid);
        assert_eq!(ix.consume_share(sh, "tok-abc").expect("open2"), fid);
        assert_eq!(
            ix.consume_share(sh, "tok-abc").unwrap_err(),
            "share exhausted"
        );
        assert_eq!(
            ix.consume_share(sh + 999, "tok-abc").unwrap_err(),
            "share not found"
        );

        // 错误令牌
        let fid2 = ix.add_file(0, "x", 1, &[]).expect("add");
        let sh2 = ix
            .create_share(fid2, ix.hmac_token("tok-xyz"), 3600, 1)
            .expect("share");
        assert_eq!(
            ix.consume_share(sh2, "nope").unwrap_err(),
            "share token mismatch"
        );
    }
}
