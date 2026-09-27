//! 设置与同步策略的加密落盘（P8-7 / P8-10；docs/v2.0/05-03 §5.5、05-05 §5.3）。
//!
//! 两个**单文件 AEAD 载体**（tmp + rename 原子写，与 `audit.enc` 同款做法）：
//! - `settings.enc`：通用设置（隐写开关等），密钥 `HKDF(MK,"settings")`；
//! - `sync-policy.enc`：选择性同步策略，密钥 `HKDF(MK,"sync-policy")`。
//!
//! **直派生口径说明（如实登记）**：这两把键**不在** `VSVB v3` 的 8 条从属密钥之列
//! （该表在 P7-1 冻结，改动即变更头部格式）。文档（05-05 §5.3 / 05-03 §5.5）
//! 明确要求 `HKDF(MK,"settings")` / `HKDF(MK,"sync-policy")` **直派生**，故此处
//! 照文档实现；它们只保护「本地开关与策略」，不参与任何数据解密链条。
//!
//! 失败语义（05-03 §5.5）：**读取失败 → 默认策略（全同步）+ 如实提示**，
//! 绝不静默按「全不同步」执行。
#![deny(unsafe_code)]

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use vault_crypto::{
    aead_decrypt, aead_encrypt, hkdf_sha256_derive, random_bytes, AES_GCM_NONCE_LEN, KEY_LEN,
};

pub(crate) const SETTINGS_FILE: &str = "settings.enc";
pub(crate) const POLICY_FILE: &str = "sync-policy.enc";

fn key(mk: &[u8; KEY_LEN], info: &str) -> [u8; KEY_LEN] {
    *hkdf_sha256_derive(mk, info.as_bytes())
}

/// 读取单文件 AEAD 载体；文件缺失或解密失败一律 `None`（不区分——
/// 避免把「文件存在但密钥不对」变成可探测信息），调用方取默认值。
pub(crate) fn load(data_dir: &Path, file: &str, mk: &[u8; KEY_LEN], info: &str) -> Option<Vec<u8>> {
    let blob = std::fs::read(data_dir.join(file)).ok()?;
    aead_decrypt(&key(mk, info), &blob)
}

/// 原子写（同目录 tmp + rename）；空目录自动创建。
pub(crate) fn save(
    data_dir: &Path,
    file: &str,
    mk: &[u8; KEY_LEN],
    info: &str,
    plaintext: &[u8],
) -> Result<(), String> {
    std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let nonce_v = random_bytes(AES_GCM_NONCE_LEN);
    let nonce: [u8; AES_GCM_NONCE_LEN] = nonce_v
        .as_slice()
        .try_into()
        .map_err(|_| "nonce len".to_string())?;
    let blob = aead_encrypt(&key(mk, info), &nonce, plaintext).map_err(|e| e.to_string())?;
    let tmp = data_dir.join(format!("{file}.tmp"));
    std::fs::write(&tmp, &blob).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, data_dir.join(file)).map_err(|e| e.to_string())
}

// ==== settings.enc（通用设置）====

/// 自建中继端点（P8-5 收尾；docs/v2.0/08 §3.6 多中继）。`public = true` 标记
/// **官方公共中继**条目——`allowPublicRelay = false` 时在拨号前被过滤
/// （连握手都不发生）；`false` = 用户自建中继，不受该开关影响。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayEndpoint {
    pub addr: String,
    pub token: String,
    #[serde(default)]
    pub public: bool,
}

/// 通用设置（P8-10 隐写开关等；`serde(default)` 保证向前兼容）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub schema: u32,
    /// 隐写术启用开关（P8-10：解锁后引擎自读，不再依赖 UI 每次同步）。
    #[serde(default)]
    pub stego_enabled: bool,
    /// 是否允许连接官方公共中继（P8-5 收尾；默认允许）。`false` 时客户端
    /// **不向官方中继发起任何连接**（docs/08 §3.6：失败不静默回退）。
    #[serde(default = "d_true")]
    pub allow_public_relay: bool,
    /// 自建 / 官方中继候选（**按用户配置顺序**依次尝试，docs/08 §3.6 多中继）。
    #[serde(default)]
    pub relays: Vec<RelayEndpoint>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            schema: 0,
            stego_enabled: false,
            allow_public_relay: true,
            relays: Vec::new(),
        }
    }
}

impl Settings {
    pub const INFO: &'static str = "settings";
    const MAX_RELAYS: usize = 8;

    /// 严格校验（与 `SyncPolicy::validate` 同纪律：任一不合法即整体拒绝）。
    /// token 下限与客户端前置拦截同口径（<16 字节 = <128 位，合规中继必拒）。
    pub fn validate(&self) -> Result<(), String> {
        if self.relays.len() > Self::MAX_RELAYS {
            return Err(format!("中继候选不得超过 {} 条", Self::MAX_RELAYS));
        }
        for r in &self.relays {
            if r.addr.trim().is_empty() {
                return Err("中继地址不得为空".into());
            }
            if r.token.len() < 16 {
                return Err(format!(
                    "中继 {} 的 token 短于 128 位（16 字节）：合规中继必拒",
                    r.addr
                ));
            }
        }
        Ok(())
    }

    pub fn load(data_dir: &Path, mk: &[u8; KEY_LEN]) -> Self {
        load(data_dir, SETTINGS_FILE, mk, Self::INFO)
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path, mk: &[u8; KEY_LEN]) -> Result<(), String> {
        self.validate()?;
        let body = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        save(data_dir, SETTINGS_FILE, mk, Self::INFO, &body)
    }

    /// 生效的中继候选（保序）：`allowPublicRelay = false` 时**在拨号前**剔除
    /// `public = true` 条目——不向官方中继发起任何连接（docs/08 §3.6）。
    pub fn relay_candidates(&self) -> Vec<(String, String)> {
        self.relays
            .iter()
            .filter(|r| !r.public || self.allow_public_relay)
            .map(|r| (r.addr.clone(), r.token.clone()))
            .collect()
    }
}

// ==== sync-policy.enc（选择性同步策略，P8-7；conflict 段供 P8-8 复用）====

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub tz: String,
}

/// 带宽上限（kbps；`0` = 不限——**默认即不限**，是产品语义而非缺省巧合）。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bandwidth {
    #[serde(default)]
    pub up_kbps: u64,
    #[serde(default)]
    pub down_kbps: u64,
}

/// 四维过滤（空集合 / 0 = 不限，即默认全同步）。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filters {
    #[serde(default)]
    pub folder_exclude: Vec<u64>,
    #[serde(default)]
    pub size_max_auto_bytes: u64,
    #[serde(default)]
    pub net_types: Vec<String>,
    #[serde(default)]
    pub target_device_ids: Vec<String>,
}

/// 冲突副本保留策略（P8-8；与同步策略同一文件，不新增第三个策略文件）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictPolicy {
    #[serde(default = "d_keep_days")]
    pub keep_window_days: u32,
    #[serde(default = "d_copy_template")]
    pub copy_name_template: String,
    #[serde(default = "d_max_copies")]
    pub max_copies_per_file: u32,
}

fn d_keep_days() -> u32 {
    7
}
fn d_copy_template() -> String {
    "{name}.conflict-{ts}".to_string()
}
fn d_max_copies() -> u32 {
    5
}

impl Default for ConflictPolicy {
    fn default() -> Self {
        ConflictPolicy {
            keep_window_days: d_keep_days(),
            copy_name_template: d_copy_template(),
            max_copies_per_file: d_max_copies(),
        }
    }
}

fn d_true() -> bool {
    true
}
fn d_concurrency() -> u32 {
    3
}
fn d_tier() -> String {
    "auto".to_string()
}

/// `sync_policy` 完整载荷（docs/05-03 §5.5）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPolicy {
    #[serde(default = "d_schema")]
    pub schema: u32,
    #[serde(default = "d_true")]
    pub enabled: bool,
    #[serde(default)]
    pub window: Option<Window>,
    #[serde(default)]
    pub bandwidth: Bandwidth,
    #[serde(default)]
    pub wifi_only: bool,
    #[serde(default = "d_concurrency")]
    pub concurrency: u32,
    #[serde(default = "d_tier")]
    pub padding_tier: String,
    #[serde(default)]
    pub filters: Filters,
    #[serde(default)]
    pub per_peer_modes: BTreeMap<String, String>,
    #[serde(default)]
    pub conflict: ConflictPolicy,
}

fn d_schema() -> u32 {
    1
}

impl Default for SyncPolicy {
    fn default() -> Self {
        SyncPolicy {
            schema: 1,
            enabled: true,
            window: None,
            bandwidth: Bandwidth::default(),
            wifi_only: false,
            concurrency: 3,
            padding_tier: "auto".to_string(),
            filters: Filters::default(),
            per_peer_modes: BTreeMap::new(),
            conflict: ConflictPolicy::default(),
        }
    }
}

impl SyncPolicy {
    pub const INFO: &'static str = "sync-policy";
    const NET_TYPES: [&'static str; 4] = ["wifi", "ethernet", "cellular", "metered"];
    const PEER_MODES: [&'static str; 3] = ["full", "excludeFolders", "manual"];

    /// 严格校验：任一不合法即整体拒绝（**原子拒绝**，旧策略完好）。
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1 {
            return Err(format!("schema 必须为 1（得到 {}）", self.schema));
        }
        if !(1..=16).contains(&self.concurrency) {
            return Err("concurrency 必须在 1..=16".into());
        }
        let tier_ok = self.padding_tier == "auto"
            || self
                .padding_tier
                .parse::<u8>()
                .map(|t| t <= 4)
                .unwrap_or(false);
        if !tier_ok {
            return Err("paddingTier 必须为 auto 或 0..=4".into());
        }
        if let Some(w) = &self.window {
            if !is_hhmm(&w.start) || !is_hhmm(&w.end) {
                return Err("window.start / window.end 必须为 HH:MM".into());
            }
        }
        if self.bandwidth.up_kbps > 10_000_000 || self.bandwidth.down_kbps > 10_000_000 {
            return Err("bandwidth 上限不得超过 10000000 kbps".into());
        }
        for t in &self.filters.net_types {
            if !Self::NET_TYPES.contains(&t.as_str()) {
                return Err(format!("netTypes 含未知取值 {t}"));
            }
        }
        for (peer, m) in &self.per_peer_modes {
            if !Self::PEER_MODES.contains(&m.as_str()) {
                return Err(format!(
                    "perPeerModes[{peer}] 必须为 full|excludeFolders|manual"
                ));
            }
        }
        if !(1..=365).contains(&self.conflict.keep_window_days) {
            return Err("conflict.keepWindowDays 必须在 1..=365".into());
        }
        if !(1..=100).contains(&self.conflict.max_copies_per_file) {
            return Err("conflict.maxCopiesPerFile 必须在 1..=100".into());
        }
        if !self.conflict.copy_name_template.contains("{name}") {
            return Err("conflict.copyNameTemplate 必须含 {name}".into());
        }
        Ok(())
    }

    /// 读取（文件缺失 / 解密失败 / 解析失败 → 默认策略 + `fromDefault=true`）。
    pub fn load(data_dir: &Path, mk: &[u8; KEY_LEN]) -> (Self, bool) {
        match load(data_dir, POLICY_FILE, mk, Self::INFO) {
            Some(b) => match serde_json::from_slice::<SyncPolicy>(&b) {
                Ok(p) if p.validate().is_ok() => (p, false),
                _ => (SyncPolicy::default(), true),
            },
            None => (SyncPolicy::default(), true),
        }
    }

    pub fn save(&self, data_dir: &Path, mk: &[u8; KEY_LEN]) -> Result<(), String> {
        self.validate()?;
        let body = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        save(data_dir, POLICY_FILE, mk, Self::INFO, &body)
    }

    /// 转成引擎侧的过滤策略（`vault-p2p` 不依赖 vault-core，故用同一字段名的结构传递）。
    pub fn to_engine_filter(&self) -> vault_p2p::policy::SyncFilter {
        vault_p2p::policy::SyncFilter {
            enabled: self.enabled,
            folder_exclude: self.filters.folder_exclude.clone(),
            size_max_auto_bytes: self.filters.size_max_auto_bytes,
            net_types: self.filters.net_types.clone(),
            target_device_ids: self.filters.target_device_ids.clone(),
            per_peer_modes: self.per_peer_modes.clone(),
            copy_name_template: self.conflict.copy_name_template.clone(),
            max_copies_per_file: self.conflict.max_copies_per_file,
            keep_window_days: self.conflict.keep_window_days,
        }
    }
}

fn is_hhmm(s: &str) -> bool {
    let Some((h, m)) = s.split_once(':') else {
        return false;
    };
    if h.len() != 2 || m.len() != 2 {
        return false;
    }
    matches!(h.parse::<u8>(), Ok(0..=23)) && matches!(m.parse::<u8>(), Ok(0..=59))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// 证据：非法策略**原子拒绝**（逐条校验；旧策略不受影响）。
    #[test]
    fn invalid_policy_rejected_atomically() {
        let mut p = SyncPolicy::default();
        assert!(p.validate().is_ok(), "默认策略必须合法");

        p.concurrency = 0;
        assert!(p.validate().is_err());
        p.concurrency = 3;

        p.padding_tier = "9".into();
        assert!(p.validate().is_err());
        p.padding_tier = "auto".into();

        p.window = Some(Window {
            start: "9:00".into(),
            end: "23:00".into(),
            tz: "local".into(),
        });
        assert!(p.validate().is_err(), "HH:MM 必须两位");
        p.window = None;

        p.filters.net_types = vec!["5g".into()];
        assert!(p.validate().is_err());
        p.filters.net_types.clear();

        p.per_peer_modes.insert("vd-a".into(), "sometimes".into());
        assert!(p.validate().is_err());
        p.per_peer_modes.clear();

        p.conflict.keep_window_days = 0;
        assert!(p.validate().is_err());
        p.conflict.keep_window_days = 7;

        p.conflict.max_copies_per_file = 101;
        assert!(p.validate().is_err());
        p.conflict.max_copies_per_file = 5;

        p.conflict.copy_name_template = "copy".into();
        assert!(p.validate().is_err());
        p.conflict.copy_name_template = "{name}.conflict-{ts}".into();

        assert!(p.validate().is_ok(), "逐项复原后应重新合法");
    }

    /// 往返：落盘可读且字段保真；文件缺失/被篡改 → 默认策略 + fromDefault。
    #[test]
    fn policy_roundtrip_and_default_on_missing() {
        let dir = tempfile::tempdir().unwrap();
        let mk = vault_crypto::random_key();
        let (d, from_default) = SyncPolicy::load(dir.path(), &mk);
        assert!(from_default && d.enabled, "缺文件 → 默认全同步");

        let mut p = SyncPolicy::default();
        p.filters.folder_exclude = vec![7, 9];
        p.filters.size_max_auto_bytes = 1 << 30;
        p.conflict.keep_window_days = 30;
        p.save(dir.path(), &mk).unwrap();
        let (back, from_default) = SyncPolicy::load(dir.path(), &mk);
        assert!(!from_default);
        assert_eq!(back.filters.folder_exclude, vec![7, 9]);
        assert_eq!(back.conflict.keep_window_days, 30);

        // 篡改文件 → 解密失败 → 默认策略（不 panic、不误判为全不同步）
        let path = dir.path().join(POLICY_FILE);
        let mut blob = std::fs::read(&path).unwrap();
        let last = blob.len() - 1;
        blob[last] ^= 0xff;
        std::fs::write(&path, &blob).unwrap();
        let (p2, from_default) = SyncPolicy::load(dir.path(), &mk);
        assert!(from_default && p2.enabled);
        // 引擎过滤策略映射保真
        let f = p.to_engine_filter();
        assert_eq!(f.folder_exclude, vec![7, 9]);
        assert!(f.enabled);
    }

    /// 设置载体（settings.enc）往返：隐写开关 / 中继候选 / allowPublicRelay
    /// 均可跨重启保持；非法设置**原子拒绝**；public 条目在禁用官方中继时
    /// 被过滤（保序、自建条目保留）。
    #[test]
    fn settings_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mk = vault_crypto::random_key();
        let d = Settings::load(dir.path(), &mk);
        assert!(!d.stego_enabled);
        assert!(d.allow_public_relay, "默认允许官方中继");
        assert!(d.relays.is_empty());

        let s = Settings {
            schema: 1,
            stego_enabled: true,
            allow_public_relay: false,
            relays: vec![
                RelayEndpoint {
                    addr: "relay.example.com:47471".into(),
                    token: "selfhosted-token-0123456789ab".into(),
                    public: false,
                },
                RelayEndpoint {
                    addr: "public.relay:47471".into(),
                    token: "public-token-0123456789abcdef".into(),
                    public: true,
                },
            ],
        };
        s.save(dir.path(), &mk).unwrap();
        let back = Settings::load(dir.path(), &mk);
        assert!(back.stego_enabled);
        assert!(!back.allow_public_relay);
        assert_eq!(back.relays, s.relays);
        // 禁用官方中继 → public 条目在拨号前被过滤，自建条目保序保留
        let cands = back.relay_candidates();
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].0, "relay.example.com:47471");
        // 放开开关 → 全部保留（保序）
        let mut s2 = back.clone();
        s2.allow_public_relay = true;
        assert_eq!(s2.relay_candidates().len(), 2);

        // 非法：短 token（<128 位）→ 原子拒绝
        let mut s3 = s2.clone();
        s3.relays[0].token = "short".into();
        assert!(s3.validate().is_err());
        // 非法：超上限（>8 条）
        let mut s4 = s2.clone();
        s4.relays = (0..9)
            .map(|i| RelayEndpoint {
                addr: format!("r{i}:1"),
                token: "long-enough-token-0123456789".into(),
                public: false,
            })
            .collect();
        assert!(s4.validate().is_err());
    }
}
