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
//!
//! P5-4 密钥轮换扩展（同样 serde default，旧索引可读）：
//! - `Folder.fskey`：文件夹 FSK 覆盖（hex）。None = 按 MK 派生 `HKDF(MK,"fsk/<id>")`；
//!   Some = 轮换写入的**随机** FSK'（语义与 `FileEntry.fskey` 对称，见 docs/07 §二 FSK 行）。
//! - `IndexData.staging`：轮换中「暂存容器」标记（file_id → 随机 tag），是轮换的崩溃恢复锚点：
//!   密钥与标记在同一次索引提交中生效，读路径据此把「标记指向的暂存容器」与「正式容器」判开
//!   （详见 `vault.rs::Vault::rotate_containers` 的阶段说明）。
#![forbid(unsafe_code)]
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use serde::{Deserialize, Serialize};
use vault_crypto::KEY_LEN;

pub const ROOT_FOLDER: u64 = 0;
pub const INDEX_MAGIC: &[u8; 4] = b"VSIX";
pub const INDEX_FORMAT_VER: u16 = 2;
/// VSIX v3（分段存储，docs/v2.0/05-02 §3.2）：条目值布局
/// `entry_ver u16=3 | reserved u16 | payload_len u32 | JSON payload`。
pub const INDEX_FORMAT_VER_V3: u16 = 3;

/// V3 键空间命名空间（key 高位段）。
/// 设计定义 bit62=分享票据、bit63=索引元数据；bit61=文件夹（P6 实现扩展：
/// 本实现的文件夹与文件共用 next_id 计数器，必须再加一段区分，偏差记 LOG）。
pub const NS_FOLDER: u64 = 1 << 61;
pub const NS_SHARE: u64 = 1 << 62;
pub const NS_META: u64 = 1 << 63;

const MAX_TOKENS_PER_FILE: usize = 10000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Folder {
    pub name: String,
    pub parent: u64,
    /// 文件夹标签（P4 右键菜单「标签」，与文件同一 UI 概念）。
    /// 与文件夹名一样不进倒排索引（搜索只返回文件），serde default 保证旧索引可读。
    #[serde(default)]
    pub tags: Vec<String>,
    /// 文件夹 FSK 覆盖（hex，32B；P5-4 密钥轮换）。None = 按 MK 派生 `HKDF(MK,"fsk/<id>")`；
    /// Some = 轮换后写入的随机 FSK'。**随机而非派生**：MK 泄露场景下派生值可被重算出来，
    /// 随机值才真正切断旧密钥路径（docs/07 §二）。serde default 保证旧索引可读。
    #[serde(default)]
    pub fskey: Option<String>,
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
    // ==== VSIX v3 新增字段（docs/v2.0/05-02 §3.2，serde default 保证 v2 可读）====
    /// 条目版本号（原地编辑 / 冲突判定，P8-8 启用；P6 先占位）。
    #[serde(default)]
    pub rev: u64,
    /// 该条目密文最后一次轮换时的轮换代标识（P7-3 启用；P6 先占位）。
    #[serde(default)]
    pub rotated_with: Option<String>,
    /// 全文提取器版本（P7-9 启用；0 = 未提取）。
    #[serde(default)]
    pub extract_ver: u16,
    /// 提取是否被 64 MiB 上限截断（P7-9 启用）。
    #[serde(default)]
    pub truncated: bool,
    /// 内容采样令牌的 HMAC 列表（v3 起随条目持久化：倒排索引可从条目重建，
    /// 不再整体落盘。名称 / 标签令牌仍实时派生，只有内容采样需要存）。
    #[serde(default)]
    pub tokens_extra: Vec<String>,
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
    /// 轮换中的暂存容器标记：file_id → 暂存文件名里的随机 tag（P5-4）。
    /// 标记与「新 FSKey / 新 FSK 覆盖」在同一次索引落盘中提交，故「标记指向的暂存文件存在」
    /// 恰好等价于「该文件当前的密文是索引中当前密钥的产物」。serde default 保证旧索引可读。
    #[serde(default)]
    staging: BTreeMap<u64, String>,
    next_id: u64,
}

/// VSIX v3 变更日志条目：条目级 upsert/delete（key 含命名空间位）。
enum PendingOp {
    Upsert(u64, Vec<u8>),
    Delete(u64),
}

pub struct VaultIndex {
    d: IndexData,
    search_key: [u8; KEY_LEN],
    /// v3 变更日志：每次 mutation 追加条目级 upsert/delete，由保存方
    /// `take_pending_ops` 取走经 vault-store 六步提交协议落段（P6-3）。
    pending: Vec<PendingOp>,
    /// 元数据条目（next_id / staging / tombstones）是否变脏。
    meta_dirty: bool,
    /// 整表重写标记（rebase_search_key 后倒排不可增量迁移）。
    full_rewrite: bool,
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
            staging: BTreeMap::new(),
            next_id: 1,
        };
        d.folders.insert(
            ROOT_FOLDER,
            Folder {
                name: String::new(),
                parent: 0,
                tags: Vec::new(),
                fskey: None,
            },
        );
        Self {
            d,
            search_key,
            pending: Vec::new(),
            meta_dirty: false,
            full_rewrite: false,
        }
    }

    fn hmac_token(&self, token: &str) -> String {
        vault_crypto::hmac_sha256_hex(&self.search_key, token.as_bytes())
    }

    /// 供编排层使用（分享令牌哈希）。
    pub fn token_hash(&self, token: &str) -> String {
        self.hmac_token(token)
    }

    fn push_file(&mut self, id: u64) {
        if let Some(f) = self.d.files.get(&id) {
            if let Ok(json) = serde_json::to_vec(f) {
                self.pending.push(PendingOp::Upsert(id, json));
            }
        }
    }

    fn push_folder(&mut self, id: u64) {
        if let Some(f) = self.d.folders.get(&id) {
            if let Ok(json) = serde_json::to_vec(f) {
                self.pending.push(PendingOp::Upsert(NS_FOLDER | id, json));
            }
        }
    }

    fn push_share(&mut self, id: u64) {
        if let Some(sh) = self.d.shares.get(&id) {
            if let Ok(json) = serde_json::to_vec(sh) {
                self.pending.push(PendingOp::Upsert(NS_SHARE | id, json));
            }
        }
    }

    fn push_meta(&mut self) {
        if !self.meta_dirty {
            return;
        }
        self.meta_dirty = false;
        let meta = MetaV3 {
            schema: 3,
            next_id: self.d.next_id,
            staging: self.d.staging.clone(),
            tombstones: self.d.tombstones.clone(),
            tokenizer_ver: 2,
        };
        if let Ok(json) = serde_json::to_vec(&meta) {
            self.pending.push(PendingOp::Upsert(NS_META, json));
        }
    }

    /// 预分配 id（FSKey 派生需要 file_id，先于加密分配）。
    pub fn alloc_id(&mut self) -> u64 {
        let id = self.d.next_id;
        self.d.next_id += 1;
        self.meta_dirty = true; // next_id 变更随元数据条目落盘
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
                rev: 0,
                rotated_with: None,
                extract_ver: 0,
                truncated: false,
                tokens_extra: Vec::new(),
            },
        );
        self.index_tokens(id, tokens);
        self.push_file(id);
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
                tags: Vec::new(),
                fskey: None,
            },
        );
        self.push_folder(id);
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
        self.push_folder(id);
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
        self.push_file(id);
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
                rev: 0,
                rotated_with: None,
                extract_ver: 0,
                truncated: false,
                tokens_extra: Vec::new(),
            },
        );
        self.index_tokens(id, tokens);
        self.push_file(id);
        Ok(id)
    }

    pub fn remove_file(&mut self, id: u64) -> Result<FileEntry, &'static str> {
        let e = self.d.files.remove(&id).ok_or("file not found")?;
        // 轮换标记随条目一并清除：否则 id 被同步复用时读路径会指向陈旧暂存容器
        self.d.staging.remove(&id);
        self.deindex_name(id, &e.name);
        for t in &e.tags {
            self.deindex_name(id, t);
        }
        self.pending.push(PendingOp::Delete(id));
        self.meta_dirty = true; // staging 变更
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
            self.pending.push(PendingOp::Delete(NS_FOLDER | cur));
        }
        Ok(removed)
    }

    /// 设置标签：文件走倒排重索引；文件夹无倒排（与文件夹名一致），仅存元数据。
    pub fn set_tags(&mut self, id: u64, tags: Vec<String>) -> Result<(), &'static str> {
        if !self.d.files.contains_key(&id) {
            let f = self.d.folders.get_mut(&id).ok_or("entry not found")?;
            f.tags = tags;
            self.push_folder(id);
            return Ok(());
        }
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
        self.push_file(id);
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
            // 根目录（id=0, parent=0）不是自己的子文件夹，必须排除
            .filter(|(id, f)| f.parent == folder && **id != ROOT_FOLDER)
            .map(|(id, f)| serde_json::json!({"id": id, "name": f.name, "tags": f.tags}))
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
        self.push_share(id);
        Ok(id)
    }

    /// 校验并消耗一次打开机会；成功返回 file_id。
    pub fn consume_share(&mut self, share_id: u64, token: &str) -> Result<u64, &'static str> {
        let token_hash = self.hmac_token(token);
        let file_id = {
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
            sh.file_id
        };
        self.push_share(share_id);
        Ok(file_id)
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
        // 对端 id 可能与本机已删条目的 id 重合：清掉陈旧轮换标记（新容器由 ingest 直接落正式路径）
        self.d.staging.remove(&id);
        if id >= self.d.next_id {
            self.d.next_id = id + 1;
            self.meta_dirty = true;
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
                rev: 0,
                rotated_with: None,
                extract_ver: 0,
                truncated: false,
                tokens_extra: Vec::new(),
            },
        );
        self.index_tokens(id, tokens);
        let _ = now;
        self.push_file(id);
        Ok(())
    }

    /// 本机内容变更后自增本机向量时钟位。
    pub fn bump_vc(&mut self, id: u64, device: &str) -> Result<(), &'static str> {
        let c = self.d.files.get_mut(&id).ok_or("file not found")?;
        let next = c.vc.get(device).copied().unwrap_or(0) + 1;
        c.vc.insert(device.to_string(), next);
        self.push_file(id);
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
        self.push_file(id);
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

    // ==== P5-4 密钥轮换支撑（docs/07 §二/§三）====

    /// 文件夹 FSK 覆盖（hex）；None = 按 MK 派生。不存在的 id 返回 Err（调用方据此校验）。
    pub fn folder_fsk(&self, folder_id: u64) -> Result<Option<String>, &'static str> {
        self.d
            .folders
            .get(&folder_id)
            .map(|f| f.fskey.clone())
            .ok_or("folder not found")
    }

    pub fn set_folder_fskey(
        &mut self,
        folder_id: u64,
        fskey_hex: &str,
    ) -> Result<(), &'static str> {
        let f = self
            .d
            .folders
            .get_mut(&folder_id)
            .ok_or("folder not found")?;
        f.fskey = Some(fskey_hex.to_string());
        self.push_folder(folder_id);
        Ok(())
    }

    /// 写入文件 FSKey 覆盖（轮换后的随机 FSKey；语义同同步携带的覆盖）。
    pub fn set_file_fskey(&mut self, file_id: u64, fskey_hex: &str) -> Result<(), &'static str> {
        let f = self.d.files.get_mut(&file_id).ok_or("file not found")?;
        f.fskey = Some(fskey_hex.to_string());
        self.push_file(file_id);
        Ok(())
    }

    /// 全部文件夹 id（含根）。
    pub fn folder_ids(&self) -> Vec<u64> {
        self.d.folders.keys().copied().collect()
    }

    /// 全部文件 id（BTreeMap 序，天然稳定）。
    pub fn all_file_ids(&self) -> Vec<u64> {
        self.d.files.keys().copied().collect()
    }

    /// 文件夹**直属**文件 id（不含子文件夹）。FSK 只保护直属文件：子文件夹的 FSK 由 MK 独立
    /// 派生，父文件夹 FSK 泄露不影响它们（docs/07 §二 FSK 行的影响域即「该文件夹全部文件」）。
    pub fn files_direct(&self, folder: u64) -> Vec<u64> {
        self.d
            .files
            .iter()
            .filter(|(_, f)| f.folder == folder)
            .map(|(k, _)| *k)
            .collect()
    }

    /// 标记「该文件的暂存容器（tag）是当前索引密钥的产物」。
    pub fn mark_staging(&mut self, file_id: u64, tag: &str) {
        self.d.staging.insert(file_id, tag.to_string());
        self.meta_dirty = true;
    }

    pub fn staging_tag(&self, file_id: u64) -> Option<String> {
        self.d.staging.get(&file_id).cloned()
    }

    /// 清除指定文件的轮换标记（只清本轮涉及的文件：其它待生效文件的标记仍指向其暂存容器）。
    pub fn clear_staging(&mut self, file_ids: &[u64]) {
        for id in file_ids {
            self.d.staging.remove(id);
        }
        self.meta_dirty = true;
    }

    /// 重挂搜索密钥并**重建**倒排索引（MK 全库轮换用）。
    ///
    /// 倒排索引的键是 `HMAC(搜索密钥, 词)`，密钥更换后旧令牌既无法解密也无法转换，只能重建：
    /// 文件名与标签令牌在此重算，内容采样令牌由调用方在轮换时从解密出的明文里取得
    ///（`content`：file_id → 令牌；与 `import_file` 同一采样规则）。
    pub fn rebase_search_key(
        &mut self,
        search_key: [u8; KEY_LEN],
        content: &BTreeMap<u64, Vec<String>>,
    ) {
        self.search_key = search_key;
        self.d.search.clear();
        self.full_rewrite = true; // 倒排整体重建：下一次保存走整表快照
        let entries: Vec<(u64, String, Vec<String>)> = self
            .d
            .files
            .iter()
            .map(|(id, f)| (*id, f.name.clone(), f.tags.clone()))
            .collect();
        for (id, name, tags) in entries {
            let mut tokens = tokenize(&name);
            for t in &tags {
                tokens.extend(tokenize(t));
            }
            if let Some(extra) = content.get(&id) {
                tokens.extend(extra.iter().cloned());
            }
            self.index_tokens(id, &tokens);
        }
    }

    /// 作废全部分享令牌（MK 轮换时调用）：分享只存令牌的 HMAC（明文不落盘），
    /// 搜索密钥更换后无法迁移，保留只会留下永远校验失败的死条目。
    pub fn clear_shares(&mut self) {
        for id in self.d.shares.keys().copied().collect::<Vec<_>>() {
            self.pending.push(PendingOp::Delete(NS_SHARE | id));
        }
        self.d.shares.clear();
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
        self.meta_dirty = true;
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
            tags: Vec::new(),
            fskey: None,
        });
        Ok(Self {
            d,
            search_key,
            pending: Vec::new(),
            meta_dirty: false,
            full_rewrite: false,
        })
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

// ==== P7-9 检索 V2（docs/v2.0/05-02 §4.5）====

/// 文本提取上限：单文件 64 MiB，超出即 `truncated=true`（诚实标记，不静默截断）。
pub const EXTRACT_LIMIT_BYTES: u64 = 64 * 1024 * 1024;
/// 命中窗口前后各 32 字符。
pub const SNIPPET_CONTEXT_CHARS: usize = 32;
/// 每条命中最多 3 段。
pub const MAX_SNIPPETS: usize = 3;
/// 检索 V2 载荷 schema 版本。
pub const SEARCH_V2_SCHEMA: u32 = 1;
/// 默认返回 top-N（docs/05-02 §4.5：位置只对 top-N ≤ 32 重算）。
pub const SEARCH_V2_LIMIT: usize = 32;

/// 高熵判定（脱敏阈值）：长度 ≥ 32 且 不同字符数/长度 > 0.6。
/// 设计口径「字符集熵高于阈值」的实现近似，见 docs/v2.0/05-02 §4.5。
pub fn is_high_entropy(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 32 {
        return false;
    }
    let mut distinct: Vec<char> = chars.clone();
    distinct.sort_unstable();
    distinct.dedup();
    distinct.len() as f64 / chars.len() as f64 > 0.6
}

/// 提取截断标记：`truncate_marker(len) -> (extracted_len, truncated)`。
/// len ≤ 64 MiB 原样返回；超出返回 (64 MiB, true)。
pub fn truncate_marker(total_len: u64) -> (u64, bool) {
    if total_len <= EXTRACT_LIMIT_BYTES {
        (total_len, false)
    } else {
        (EXTRACT_LIMIT_BYTES, true)
    }
}

/// 脱敏片段（docs/v2.0/05-02 §4.5）：`display` 中命中词以 «» 标注；
/// 高熵命中整段替换为 "•••" 且 `scan=true`（「此处有命中但不展示」）；
/// `offset_hint` = "第 {行} 段 · 字符 {偏移}"（字符按 Unicode 标量计）。
#[derive(Clone, Debug, serde::Serialize)]
pub struct Span {
    pub display: String,
    pub offset_hint: String,
    pub scan: bool,
}

/// 内容片段重算：`content_snippets(file_plain, query_tokens) -> Vec<Span>` 纯函数。
///
/// 诚实边界：倒排索引只存令牌 HMAC、不存位置（docs/v2.0/05-02 §4.5），内容片段
/// 需要**明文**。本函数供上层（FFI/编排层）解密容器取出明文后调用；引擎层的
/// `search_v2` 本身不触碰容器（本任务不做容器解密，content 类命中的 spans 由
/// 上层用本函数补齐）。
/// 合并后的片段：窗口起止（字符索引）+ 窗口内命中列表。
type MergedSpan = (usize, usize, Vec<(usize, usize, String)>);

pub fn content_snippets(file_plain: &str, query_tokens: &[String]) -> Vec<Span> {
    if file_plain.is_empty() || query_tokens.is_empty() {
        return Vec::new();
    }
    let lower = file_plain.to_lowercase();
    // 命中区间（字符索引）：(start, end, matched_text)
    let mut hits: Vec<(usize, usize, String)> = Vec::new();
    for tok in query_tokens {
        let t = tok.trim().to_lowercase();
        if t.chars().count() < 2 {
            continue; // 与 tokenize 的 ≥2 字符口径一致
        }
        let mut from = 0usize;
        while let Some(rel) = lower[from..].find(&t) {
            let b0 = from + rel;
            let b1 = b0 + t.len();
            let start = lower[..b0].chars().count();
            let end = lower[..b1].chars().count();
            hits.push((
                start,
                end,
                file_plain.chars().skip(start).take(end - start).collect(),
            ));
            from = b1;
        }
    }
    if hits.is_empty() {
        return Vec::new();
    }
    hits.sort_by_key(|h| h.0);
    // 合并窗口重叠的命中为同一段（窗口 ±32 字符）
    let total_chars = file_plain.chars().count();
    let mut merged: Vec<MergedSpan> = Vec::new();
    for (s, e, m) in hits {
        let (ws, we) = (
            s.saturating_sub(SNIPPET_CONTEXT_CHARS),
            (e + SNIPPET_CONTEXT_CHARS).min(total_chars),
        );
        match merged.last_mut() {
            Some(last) if ws <= last.1 => {
                last.1 = we.max(last.1);
                last.2.push((s, e, m));
            }
            _ => merged.push((ws, we, vec![(s, e, m)])),
        }
        if merged.len() >= MAX_SNIPPETS {
            break;
        }
    }
    merged
        .into_iter()
        .take(MAX_SNIPPETS)
        .map(|(ws, we, inner)| {
            let any_high_entropy = inner.iter().any(|(_, _, m)| is_high_entropy(m));
            if any_high_entropy {
                // 高熵命中：整段脱敏，不展示原文
                return Span {
                    display: "•••".to_string(),
                    offset_hint: offset_hint(file_plain, inner[0].0),
                    scan: true,
                };
            }
            // 窗口内标注全部命中词 «»
            let chars: Vec<char> = file_plain.chars().collect();
            let mut display = String::new();
            let mut cur = ws;
            for (s, e, _) in &inner {
                if *s < cur {
                    continue; // 已被上一段覆盖
                }
                display.extend(chars[cur..*s].iter());
                display.push('«');
                display.extend(chars[*s..*e].iter());
                display.push('»');
                cur = *e;
            }
            display.extend(chars[cur..we].iter());
            Span {
                display,
                offset_hint: offset_hint(file_plain, inner[0].0),
                scan: false,
            }
        })
        .collect()
}

/// 段号（换行计）+ 字符偏移提示，与 docs/v2.0/05-02 §4.5 示例同形。
fn offset_hint(text: &str, char_off: usize) -> String {
    let line = text.chars().take(char_off).filter(|c| *c == '\n').count() + 1;
    format!("第 {line} 段 · 字符 {char_off}")
}

/// 检索 V2 选项（docs/v2.0/05-02 §4.5：排名公式与权重版本化）。
#[derive(Clone, Debug)]
pub struct SearchV2Opts {
    /// top-N，默认 32。
    pub limit: usize,
    /// 排名版本号（rankVer）。
    pub rank_ver: u32,
}

impl Default for SearchV2Opts {
    fn default() -> Self {
        Self {
            limit: SEARCH_V2_LIMIT,
            rank_ver: 1,
        }
    }
}

/// 时间衰减：近 30 天 1.0，之后线性降至 0.9（约 270 天到底后钳制）。
fn time_decay(now_ms: u64, modified_ms: u64) -> f64 {
    let day_ms = 86_400_000u64;
    if now_ms <= modified_ms {
        return 1.0;
    }
    let age_days = (now_ms - modified_ms) / day_ms;
    if age_days <= 30 {
        return 1.0;
    }
    (1.0 - 0.1 * ((age_days - 30) as f64 / 270.0)).max(0.9)
}

impl VaultIndex {
    /// 检索 V2（docs/v2.0/05-02 §4.5）：排名打分 + class 标注 + 脱敏片段。
    ///
    /// 载荷 `{"schema":1,"total":N,"rankVer":1,"hits":[{hitId,fileId,name,class,
    /// "score","modifiedMs","spans":[{display,offsetHint,scan}]}]}`。
    ///
    /// 诚实边界：
    /// - **class 权重**：name=1.0 / tag=0.8 / content=1.0×idf；`idf = ln(1 + N/df)`。
    ///   `score = (0.6×w_name/tag + 0.4×w_content) × time_decay`。
    /// - **content 类命中的 spans 为空数组**：倒排只存令牌 HMAC，片段重算需要明文，
    ///   上层解密后调 `content_snippets` 补齐（见该函数注释）。name/tag 类命中在本层
    ///   直接产出脱敏片段（元数据本就在索引内）。
    pub fn search_v2(&self, query: &str, opts: &SearchV2Opts) -> serde_json::Value {
        let now = now_ms();
        let query_tokens = tokenize(query);
        let n_files = self.d.files.len().max(1) as f64;
        let ln_base = (1.0 + n_files).ln().max(1.0);

        // query_token → 命中文件集合（倒排 + 条目持久化内容采样）与 df
        let mut per_file: HashMap<u64, (f64, f64, u8, Vec<String>)> = HashMap::new(); // id → (w_nt, w_c, class_prio, toks)
        for t in &query_tokens {
            let hex = self.hmac_token(t);
            // 命中候选：倒排表 ∪ 条目 tokens_extra（内容采样只存 HMAC，随条目持久化；
            // 内存中新登记的 extras 未经倒排重建，也必须可检——两路取并）。
            let mut ids: BTreeSet<u64> = self
                .d
                .search
                .get(&hex)
                .map(|s| s.iter().copied().collect())
                .unwrap_or_default();
            for (&id, f) in &self.d.files {
                if f.tokens_extra.iter().any(|h| h == &hex) {
                    ids.insert(id);
                }
            }
            let df = ids.len().max(1) as f64;
            let idf = (1.0 + n_files / df).ln() / ln_base;
            for &id in ids.iter() {
                let Some(f) = self.d.files.get(&id) else {
                    continue;
                };
                let name_toks = tokenize(&f.name);
                let tag_toks: Vec<String> = f.tags.iter().flat_map(|tg| tokenize(tg)).collect();
                let is_name = name_toks.iter().any(|x| x == t);
                let is_tag = tag_toks.iter().any(|x| x == t);
                let entry = per_file.entry(id).or_insert((0.0, 0.0, 0, Vec::new()));
                if is_name || is_tag {
                    entry.0 += if is_name { 1.0 } else { 0.8 };
                    if is_name {
                        entry.2 = 2;
                    } else if entry.2 < 2 {
                        entry.2 = 1;
                    }
                } else {
                    entry.1 += idf;
                }
                entry.3.push(t.clone());
            }
        }

        let mut hits: Vec<(u64, f64, u8, Vec<String>)> = per_file
            .into_iter()
            .map(|(id, (mut w_nt, w_c, class, toks))| {
                w_nt = w_nt.min(1.0);
                let w_c = (w_c / ln_base).min(1.0);
                let decay = {
                    let m = self.d.files.get(&id).map(|f| f.modified_ms).unwrap_or(0);
                    time_decay(now, m)
                };
                let score = (0.6 * w_nt + 0.4 * w_c) * decay;
                (id, score, class, toks)
            })
            .filter(|(_, s, _, _)| *s > 0.0)
            .collect();
        hits.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let total = hits.len();
        let hits_json: Vec<serde_json::Value> = hits
            .into_iter()
            .take(opts.limit)
            .enumerate()
            .map(|(hit_id, (id, score, class, toks))| {
                let f = &self.d.files[&id];
                // name/tag 命中在索引层即可产出片段；content 命中留给上层补
                let spans: Vec<serde_json::Value> = match class {
                    2 => content_snippets(&f.name, &toks)
                        .iter()
                        .map(span_json)
                        .collect(),
                    1 => f
                        .tags
                        .iter()
                        .filter_map(|tg| content_snippets(tg, &toks).into_iter().next())
                        .map(|s| span_json(&s))
                        .collect(),
                    _ => Vec::new(),
                };
                serde_json::json!({
                    "hitId": hit_id,
                    "fileId": id,
                    "name": f.name,
                    "class": match class { 2 => "name", 1 => "tag", _ => "content" },
                    "score": (score * 1000.0).round() / 1000.0,
                    "modifiedMs": f.modified_ms,
                    "spans": spans,
                })
            })
            .collect();
        serde_json::json!({
            "schema": SEARCH_V2_SCHEMA,
            "total": total,
            "rankVer": opts.rank_ver,
            "hits": hits_json,
        })
    }
}

fn span_json(s: &Span) -> serde_json::Value {
    serde_json::json!({"display": s.display, "offsetHint": s.offset_hint, "scan": s.scan})
}

// ==== VSIX v3 桥接（docs/v2.0/05-02 §3.2；P6-3）====

/// v3 元数据条目（key = NS_META）。
#[derive(Serialize, Deserialize)]
struct MetaV3 {
    schema: u16,
    next_id: u64,
    staging: BTreeMap<u64, String>,
    tombstones: BTreeMap<u64, Tombstone>,
    tokenizer_ver: u16,
}

/// v3 条目值布局：`entry_ver u16 | reserved u16 | payload_len u32 | JSON payload`。
pub fn encode_v3_value(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// 解码 v3 条目值；entry_ver 高于 3 → Err（向前拒绝）。
pub fn decode_v3_value(value: &[u8]) -> Result<Vec<u8>, &'static str> {
    if value.len() < 8 {
        return Err("v3 entry truncated");
    }
    let ver = u16::from_le_bytes(value[0..2].try_into().map_err(|_| "truncated")?);
    if ver > 3 {
        return Err("index entry_ver too new");
    }
    let len = u32::from_le_bytes(value[4..8].try_into().map_err(|_| "truncated")?) as usize;
    if value.len() < 8 + len {
        return Err("v3 entry payload truncated");
    }
    Ok(value[8..8 + len].to_vec())
}

impl VaultIndex {
    /// 是否有未提交变更（v3 保存方判断是否需要落段）。
    pub fn has_pending(&self) -> bool {
        self.full_rewrite || self.meta_dirty || !self.pending.is_empty()
    }

    /// 是否要求整表重写（rebase_search_key 置位，消费后清除）。
    pub fn take_full_rewrite(&mut self) -> bool {
        std::mem::replace(&mut self.full_rewrite, false)
    }

    /// 取走变更日志，转换为 vault-store 的 (is_upsert, key, value) 操作序列。
    pub fn take_pending_ops(&mut self) -> Vec<(bool, u64, Vec<u8>)> {
        self.push_meta();
        let ops: Vec<(bool, u64, Vec<u8>)> = self
            .pending
            .drain(..)
            .map(|op| match op {
                PendingOp::Upsert(k, v) => (true, k, encode_v3_value(&v)),
                PendingOp::Delete(k) => (false, k, Vec::new()),
            })
            .collect();
        ops
    }

    /// 当前全量状态 → 整表操作序列（v3 首建 / 整表重写用）。
    /// 快照覆盖全部状态，任何未提交的增量日志一并作废。
    pub fn full_state_ops(&mut self) -> Vec<(bool, u64, Vec<u8>)> {
        self.pending.clear();
        let mut ops: Vec<(bool, u64, Vec<u8>)> = Vec::new();
        for (id, f) in &self.d.folders {
            if let Ok(json) = serde_json::to_vec(f) {
                ops.push((true, NS_FOLDER | id, encode_v3_value(&json)));
            }
        }
        for (id, f) in &self.d.files {
            if let Ok(json) = serde_json::to_vec(f) {
                ops.push((true, *id, encode_v3_value(&json)));
            }
        }
        for (id, sh) in &self.d.shares {
            if let Ok(json) = serde_json::to_vec(sh) {
                ops.push((true, NS_SHARE | id, encode_v3_value(&json)));
            }
        }
        self.meta_dirty = true;
        self.push_meta();
        while let Some(op) = self.pending.pop() {
            if let PendingOp::Upsert(k, v) = op {
                // push_meta 存的是裸 JSON，这里统一套 v3 值封装
                ops.push((true, k, if k == NS_META { encode_v3_value(&v) } else { v }));
            }
        }
        ops
    }

    /// 内容采样令牌 HMAC 登记（导入 / 同步摄取 / 整表重建时调用方传入）。
    pub fn set_tokens_extra(
        &mut self,
        file_id: u64,
        tokens_extra: Vec<String>,
    ) -> Result<(), &'static str> {
        let f = self.d.files.get_mut(&file_id).ok_or("file not found")?;
        f.tokens_extra = tokens_extra;
        self.push_file(file_id);
        Ok(())
    }

    /// 从 v3 键值视图重建内存索引（打开时由 vault-store 的 kv_pairs 驱动）。
    pub fn from_v3_pairs(
        pairs: &[(u64, Vec<u8>)],
        search_key: [u8; KEY_LEN],
    ) -> Result<Self, &'static str> {
        let mut d = IndexData {
            folders: BTreeMap::new(),
            files: BTreeMap::new(),
            shares: BTreeMap::new(),
            search: HashMap::new(),
            tombstones: BTreeMap::new(),
            staging: BTreeMap::new(),
            next_id: 1,
        };
        for &(key, ref value) in pairs {
            let payload = decode_v3_value(value)?;
            if key == NS_META {
                let meta: MetaV3 =
                    serde_json::from_slice(&payload).map_err(|_| "meta parse failed")?;
                d.next_id = meta.next_id;
                d.staging = meta.staging;
                d.tombstones = meta.tombstones;
            } else if key & NS_SHARE != 0 {
                let sh: Share =
                    serde_json::from_slice(&payload).map_err(|_| "share parse failed")?;
                d.shares.insert(key & !NS_SHARE, sh);
            } else if key & NS_FOLDER != 0 {
                let f: Folder =
                    serde_json::from_slice(&payload).map_err(|_| "folder parse failed")?;
                d.folders.insert(key & !NS_FOLDER, f);
            } else {
                let f: FileEntry =
                    serde_json::from_slice(&payload).map_err(|_| "file parse failed")?;
                d.files.insert(key, f);
            }
        }
        let mut ix = Self {
            d,
            search_key,
            pending: Vec::new(),
            meta_dirty: false,
            full_rewrite: false,
        };
        ix.d.folders.entry(ROOT_FOLDER).or_insert(Folder {
            name: String::new(),
            parent: 0,
            tags: Vec::new(),
            fskey: None,
        });
        // 倒排重建：名称 / 标签令牌实时派生 + 条目持久化的内容采样 HMAC
        let entries: Vec<(u64, Vec<String>)> =
            ix.d.files
                .iter()
                .map(|(id, f)| {
                    let mut toks = tokenize(&f.name);
                    for t in &f.tags {
                        toks.extend(tokenize(t));
                    }
                    let mut hexes: Vec<String> = toks.iter().map(|t| ix.hmac_token(t)).collect();
                    hexes.extend(f.tokens_extra.iter().cloned());
                    (*id, hexes)
                })
                .collect();
        for (id, hexes) in entries {
            for hex in hexes {
                ix.d.search.entry(hex).or_default().insert(id);
            }
        }
        Ok(ix)
    }
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
        // 根目录不能把自己列为子文件夹（parent=0 自匹配 bug 回归测试）
        let root_children = ix.list_children(ROOT_FOLDER).expect("list");
        assert!(!root_children.contains("\"id\":0"));
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
    fn folder_tags_and_rename() {
        let mut ix = VaultIndex::new(key());
        let folder = ix.mkdir(ROOT_FOLDER, "相册").expect("mkdir");
        let fid = ix.add_file(folder, "a.png", 1, &[]).expect("add");

        // 文件夹标签：存元数据、随 list_children 返回；不进倒排（搜索仍只返回文件）
        ix.set_tags(folder, vec!["证件".into(), "重要".into()])
            .expect("folder tags");
        let listed = ix.list_children(ROOT_FOLDER).expect("list");
        assert!(listed.contains("\"tags\":[\"证件\",\"重要\"]"));
        assert!(!ix.search("证件").contains("\"id\""));

        // 改名后父子关系与子文件不受影响
        ix.rename_folder(folder, "相册2026").expect("rename folder");
        let listed = ix.list_children(ROOT_FOLDER).expect("list");
        assert!(listed.contains("相册2026"));
        assert_eq!(ix.file_folder(fid).expect("folder"), folder);
        assert!(ix.rename_folder(ROOT_FOLDER, "x").is_err());

        // 不存在的 id 报错（不再误报 file not found）
        assert_eq!(ix.set_tags(9999, vec![]).unwrap_err(), "entry not found");

        // 递归删除文件夹连带子文件
        let ids = ix.remove_folder(folder).expect("rm");
        assert_eq!(ids, vec![fid]);
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

    /// 手工封一个 VSIX v2 索引块（用给定 JSON 原文），用于构造「旧版本引擎写出的索引」。
    fn seal_index_json(json: &str, key: &[u8; KEY_LEN]) -> Vec<u8> {
        let mut nonce = [0u8; vault_crypto::AES_GCM_NONCE_LEN];
        nonce.copy_from_slice(&vault_crypto::random_bytes(vault_crypto::AES_GCM_NONCE_LEN));
        let ct =
            vault_crypto::aead::aead_encrypt_with_aad(key, &nonce, json.as_bytes(), INDEX_MAGIC)
                .expect("aead");
        let mut out = Vec::new();
        out.extend_from_slice(INDEX_MAGIC);
        out.extend_from_slice(&INDEX_FORMAT_VER.to_le_bytes());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct[vault_crypto::AES_GCM_NONCE_LEN..]);
        out
    }

    /// P5-4 向后兼容：P4 时代的索引 JSON（Folder 无 fskey、IndexData 无 staging）必须可读。
    #[test]
    fn legacy_index_without_fskey_and_staging_loads() {
        let k = key();
        let legacy = serde_json::json!({
            "folders": {
                "0": {"name": "", "parent": 0, "tags": []},
                "1": {"name": "归档", "parent": 0, "tags": ["旧"]}
            },
            "files": {
                "1": {"folder": 1, "name": "a.txt", "size": 3, "tags": ["旧"],
                      "created_ms": 1, "modified_ms": 2}
            },
            "shares": {},
            "search": {},
            "next_id": 2
        })
        .to_string();
        let blob = seal_index_json(&legacy, &k);
        let ix = VaultIndex::from_encrypted(&blob, &k, k).expect("legacy index must load");
        assert!(ix
            .list_children(ROOT_FOLDER)
            .expect("list")
            .contains("归档"));
        assert_eq!(ix.file_folder(1).expect("folder"), 1);
        // 新字段缺省：无 FSK 覆盖、无轮换标记
        assert_eq!(ix.folder_fsk(1).expect("row"), None);
        assert_eq!(ix.staging_tag(1), None);
        assert_eq!(ix.file_fskey(1).expect("fskey"), None);
    }

    /// P5-4 索引字段：FSK 覆盖 / FSKey 覆盖 / 轮换标记的落盘往返，以及删除条目清标记。
    #[test]
    fn fsk_override_staging_and_roundtrip() {
        let k = key();
        let mut ix = VaultIndex::new(k);
        let f = ix.mkdir(ROOT_FOLDER, "A").expect("mkdir");
        assert_eq!(ix.folder_fsk(ROOT_FOLDER).expect("root"), None);
        assert!(ix.set_folder_fskey(9999, "ab").is_err());
        let fsk_hex = vault_crypto::hex_encode(&vault_crypto::random_key());
        ix.set_folder_fskey(f, &fsk_hex).expect("set fsk");

        let id = ix.add_file(f, "a.txt", 1, &[]).expect("add");
        let fkey_hex = vault_crypto::hex_encode(&vault_crypto::random_key());
        ix.set_file_fskey(id, &fkey_hex).expect("set fskey");
        assert!(ix.set_file_fskey(id + 999, &fkey_hex).is_err());
        ix.mark_staging(id, "tag1");
        assert_eq!(ix.staging_tag(id), Some("tag1".into()));
        assert_eq!(ix.files_direct(f), vec![id]);
        assert_eq!(ix.all_file_ids(), vec![id]);

        let blob = ix.to_encrypted(&k).expect("enc");
        let mut ix2 = VaultIndex::from_encrypted(&blob, &k, k).expect("dec");
        assert_eq!(ix2.folder_fsk(f).expect("row"), Some(fsk_hex));
        assert_eq!(ix2.file_fskey(id).expect("row"), Some(fkey_hex));
        assert_eq!(ix2.staging_tag(id), Some("tag1".into()));
        assert!(ix2.folder_ids().contains(&ROOT_FOLDER) && ix2.folder_ids().contains(&f));

        // 删除条目必须同时清掉轮换标记（否则 id 复用时读路径会指向陈旧暂存文件）
        ix2.remove_file(id).expect("rm");
        assert_eq!(ix2.staging_tag(id), None);
        assert!(ix2.files_direct(f).is_empty());
    }

    /// P5-4 搜索密钥重挂：倒排索引以新密钥重建，旧密钥算出的令牌不再命中。
    #[test]
    fn rebase_search_key_rebuilds_inverted_index() {
        let k_old = key();
        let mut ix = VaultIndex::new(k_old);
        let id = ix
            .add_file(0, "文档.txt", 1, &tokenize("文档.txt"))
            .expect("add");
        ix.set_tags(id, vec!["报告".into()]).expect("tags");
        assert!(ix.search("文档").contains("文档"));

        let k_new = key();
        let mut content: BTreeMap<u64, Vec<String>> = BTreeMap::new();
        content.insert(id, tokenize("季度财务"));
        ix.rebase_search_key(k_new, &content);

        // 新搜索密钥下：文件名 / 标签 / 内容采样令牌三类都可检索
        assert!(ix.search("文档").contains("文档"));
        assert!(ix.search("报告").contains("文档"));
        assert!(ix.search("财务").contains("文档"));
        // 旧搜索密钥算出的令牌不在倒排表里（旧令牌无法迁移）
        let stale = vault_crypto::hmac_sha256_hex(&k_old, "文档".as_bytes());
        assert!(!ix.d.search.contains_key(&stale));
        let fresh = vault_crypto::hmac_sha256_hex(&k_new, "文档".as_bytes());
        assert!(ix.d.search.contains_key(&fresh));
    }

    /// P5-4 分享令牌作废：清空后 consume 一律找不到条目。
    #[test]
    fn clear_shares_invalidates_tokens() {
        let mut ix = VaultIndex::new(key());
        let fid = ix.add_file(0, "s.txt", 1, &[]).expect("add");
        let sh = ix
            .create_share(fid, ix.hmac_token("tok"), 3600, 1)
            .expect("share");
        assert!(ix.consume_share(sh, "tok").is_ok());
        ix.clear_shares();
        assert_eq!(ix.consume_share(sh, "tok").unwrap_err(), "share not found");
    }
}

/// P7-9 检索 V2 证据测试（docs/v2.0/05-02 §4.5 §五 证据）。
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod search_v2 {
    use super::*;

    fn key() -> [u8; KEY_LEN] {
        vault_crypto::random_key()
    }

    #[test]
    fn snippet_is_redacted() {
        // 长度 ≥ 32 且不同字符数/长度 > 0.6 的高熵命中 → 整段 ••• + scan:true
        let secret = "aZ3kQ9wE5rT7yU1iO4pL6jH8gF2dS0xCv".to_string(); // 33 个互异字符
        assert_eq!(secret.chars().count(), 33);
        assert!(is_high_entropy(&secret));
        let plain = format!("前文 {secret} 后文说明");
        let spans = content_snippets(&plain, std::slice::from_ref(&secret));
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].display, "•••");
        assert!(spans[0].scan);
        assert!(!spans[0].display.contains('a'));
    }

    #[test]
    fn snippet_marks_hit_with_guillemets() {
        let spans = content_snippets("季度营收明细与季度成本说明", &["季度".to_string()]);
        assert_eq!(spans.len(), 1);
        assert!(spans[0].display.contains("«季度»"), "{}", spans[0].display);
        assert!(!spans[0].scan);
        assert!(spans[0].offset_hint.contains("字符"));
    }

    #[test]
    fn max_three_spans() {
        let plain = "甲乙甲乙甲乙甲乙甲乙甲乙甲乙甲乙甲乙甲乙甲乙";
        let spans = content_snippets(plain, &["甲".to_string()]);
        assert!(spans.len() <= 3, "got {}", spans.len());
    }

    #[test]
    fn truncation_marker() {
        assert_eq!(truncate_marker(0), (0, false));
        assert_eq!(
            truncate_marker(EXTRACT_LIMIT_BYTES),
            (EXTRACT_LIMIT_BYTES, false)
        );
        let (len, trunc) = truncate_marker(EXTRACT_LIMIT_BYTES + 1);
        assert_eq!(len, EXTRACT_LIMIT_BYTES);
        assert!(trunc);
    }

    #[test]
    fn search_v2_ranking_and_payload() {
        let mut ix = VaultIndex::new(key());
        // 两个文件都含「季度」：content 类，idf 使独占命中者排名更高
        let a = ix
            .add_file(0, "q1.txt", 1, &tokenize("q1.txt 季度报告"))
            .expect("a");
        let b = ix
            .add_file(0, "q2.txt", 1, &tokenize("q2.txt 季度 财务 季度"))
            .expect("b");
        ix.set_tokens_extra(a, vec![ix.hmac_token("季度")]).ok();
        ix.set_tokens_extra(b, vec![ix.hmac_token("季度")]).ok();
        let v = ix.search_v2("季度", &SearchV2Opts::default());
        assert_eq!(v["schema"], 1);
        assert_eq!(v["total"], 2);
        let hits = v["hits"].as_array().expect("hits");
        assert_eq!(hits.len(), 2);
        // 每条命中载荷字段齐全
        for h in hits {
            assert!(h["fileId"].is_u64());
            assert!(h["score"].is_f64());
            assert!(h["modifiedMs"].is_u64());
            assert!(matches!(
                h["class"].as_str(),
                Some("name") | Some("tag") | Some("content")
            ));
        }
        // 名称命中（独占）→ class=name，带 «» 片段
        let v2 = ix.search_v2("q2", &SearchV2Opts::default());
        let hits2 = v2["hits"].as_array().expect("hits");
        assert_eq!(hits2.len(), 1);
        assert_eq!(hits2[0]["class"], "name");
        assert!(hits2[0]["spans"].as_array().is_some_and(|s| !s.is_empty()));
        // 仅内容采样命中（「财务」只存在于导入令牌集）→ class=content，spans 留空
        let v3 = ix.search_v2("财务", &SearchV2Opts::default());
        let hits3 = v3["hits"].as_array().expect("hits");
        assert_eq!(hits3.len(), 1);
        assert_eq!(hits3[0]["fileId"], b);
        assert_eq!(hits3[0]["class"], "content");
        assert_eq!(hits3[0]["spans"].as_array().map(Vec::len), Some(0));
        let _ = (a,);
    }

    #[test]
    fn content_hit_spans_left_to_upper_layer() {
        // content 类命中：spans 为空（需上层解密后经 content_snippets 补齐——诚实边界）
        let mut ix = VaultIndex::new(key());
        let id = ix
            .add_file(0, "note.txt", 1, &tokenize("note.txt"))
            .expect("f");
        // 内容采样令牌与导入同一分词口径（CJK bigram）
        let toks = tokenize("北极熊出没手册");
        ix.set_tokens_extra(id, toks.iter().map(|t| ix.hmac_token(t)).collect())
            .ok();
        let v = ix.search_v2("北极熊", &SearchV2Opts::default());
        let hits = v["hits"].as_array().expect("hits");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["class"], "content");
        assert_eq!(hits[0]["spans"].as_array().map(Vec::len), Some(0));
    }
}
