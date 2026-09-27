//! 选择性同步过滤（P8-7，docs/v2.0/05-03 §5.5）。
//!
//! 职责边界：引擎只负责「按给定策略在**出队路径**上过滤」，策略的来源（`sync-policy.enc`
//! 的解密、校验、UI 编辑）在 `vault-core`；时段与网络类型**由外壳表达**——引擎不感知
//! 时区与网络栈，宿主经 [`SyncContext`] 注入（docs/05-03 §5.5「时长与电量维度由外壳表达」）。
//!
//! 判据（05-03 §5.5）：**过滤必须在引擎的出队路径上执行才算实现**——被排除文件夹的块
//! 一次也不得进入传输队列（`sync_policy::filter_applies_at_enqueue` 是负向证据）。
#![deny(unsafe_code)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 跳过原因（docs/05-03 §5.5 枚举；`skipped[reason]` 的唯一文案出口）。
pub const SKIP_FOLDER_EXCLUDED: &str = "folder_excluded";
pub const SKIP_SIZE_EXCEEDED: &str = "size_exceeded";
pub const SKIP_NET_TYPE_BLOCKED: &str = "net_type_blocked";
pub const SKIP_DEVICE_NOT_TARGET: &str = "device_not_target";
pub const SKIP_PEER_MODE: &str = "peer_mode";
pub const SKIP_WINDOW_CLOSED: &str = "window_closed";

/// 选择性同步过滤策略（由 `vault-core` 从 `sync-policy.enc` 解析后注入）。
/// `enabled = false`（或策略读取失败时的默认值）= **默认全同步**——绝不解为「全不同步」
/// （docs/05-03 §5.5 失败语义：宁可多同步，不可静默停同步）。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncFilter {
    #[serde(default)]
    pub enabled: bool,
    /// 排除的文件夹（含**子树递归**）。
    #[serde(default)]
    pub folder_exclude: Vec<u64>,
    /// 超过该大小不自动同步（0 = 不限）；超限项进「手动待同步」。
    #[serde(default)]
    pub size_max_auto_bytes: u64,
    /// 允许的网络类型（空 = 不限）：`wifi` / `ethernet` / `cellular` / `metered`。
    #[serde(default)]
    pub net_types: Vec<String>,
    /// 目标设备白名单（空 = 全部）。
    #[serde(default)]
    pub target_device_ids: Vec<String>,
    /// 逐设备模式：`full` | `excludeFolders` | `manual`。
    #[serde(default)]
    pub per_peer_modes: BTreeMap<String, String>,
    /// 冲突副本命名模板（P8-8；占位符 `{name}` 与 `{ts}`）。
    #[serde(default = "d_copy_template")]
    pub copy_name_template: String,
    /// 每文件冲突副本上限（P8-8；超出淘汰最旧）。
    #[serde(default = "d_max_copies")]
    pub max_copies_per_file: u32,
    /// 冲突副本保留期（天；启动扫描清理超期副本）。
    #[serde(default = "d_keep_days")]
    pub keep_window_days: u32,
}

fn d_copy_template() -> String {
    "{name}.conflict-{ts}".to_string()
}
fn d_max_copies() -> u32 {
    5
}
fn d_keep_days() -> u32 {
    7
}

/// 外壳注入的会话上下文（时段 / 当前网络类型）。
#[derive(Clone, Debug)]
pub struct SyncContext {
    /// 是否在「自动同步时段」内（时区判定属外壳）。
    pub in_window: bool,
    /// 当前网络类型：`wifi` / `ethernet` / `cellular` / `metered`（空 = 未知，视为不限）。
    pub net_type: String,
}

impl Default for SyncContext {
    fn default() -> Self {
        Self {
            in_window: true,
            net_type: String::new(),
        }
    }
}

impl SyncFilter {
    /// 入队前筛选（四维 + 逐设备 + 时段）。返回 `Some(reason)` = 跳过。
    /// `folder_excluded` 由调用方用保险箱的文件夹树判定（含子树）。
    pub fn check_enqueue(
        &self,
        size: u64,
        folder_excluded: bool,
        peer_id: &str,
        ctx: &SyncContext,
    ) -> Option<&'static str> {
        if !self.enabled {
            return None;
        }
        if let Some(m) = self.per_peer_modes.get(peer_id) {
            if m == "manual" {
                return Some(SKIP_PEER_MODE);
            }
        }
        if !ctx.in_window {
            return Some(SKIP_WINDOW_CLOSED);
        }
        if !self.target_device_ids.is_empty()
            && !self.target_device_ids.iter().any(|d| d == peer_id)
        {
            return Some(SKIP_DEVICE_NOT_TARGET);
        }
        if folder_excluded {
            return Some(SKIP_FOLDER_EXCLUDED);
        }
        if self.size_max_auto_bytes > 0 && size > self.size_max_auto_bytes {
            return Some(SKIP_SIZE_EXCEEDED);
        }
        None
    }

    /// 出站前复核（网络类型在传输开始时才确定，docs/05-03 §5.5 第二处生效点）。
    /// **网络类型未知（空串）时放行**：外壳未上报 ≠ 受限网络，宁可同步也不静默停同步
    /// （与「策略读取失败取默认全同步」同一纪律）。
    pub fn check_egress(&self, ctx: &SyncContext) -> Option<&'static str> {
        if !self.enabled {
            return None;
        }
        if !ctx.in_window {
            return Some(SKIP_WINDOW_CLOSED);
        }
        if !self.net_types.is_empty()
            && !ctx.net_type.is_empty()
            && !self.net_types.iter().any(|t| t == &ctx.net_type)
        {
            return Some(SKIP_NET_TYPE_BLOCKED);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled() -> SyncFilter {
        SyncFilter {
            enabled: true,
            ..Default::default()
        }
    }

    /// 证据（面板 P8-7 `policy::four_dimension_filters_apply`）：四维各自生效。
    #[test]
    fn four_dimension_filters_apply() {
        let ctx = SyncContext::default();
        // ① 文件夹（含子树——子树判定在调用方，这里验证布尔口径）
        let f = SyncFilter {
            folder_exclude: vec![7],
            ..enabled()
        };
        assert_eq!(
            f.check_enqueue(10, true, "vd-a", &ctx),
            Some(SKIP_FOLDER_EXCLUDED)
        );
        assert_eq!(f.check_enqueue(10, false, "vd-a", &ctx), None);
        // ② 大小
        let f = SyncFilter {
            size_max_auto_bytes: 100,
            ..enabled()
        };
        assert_eq!(
            f.check_enqueue(101, false, "vd-a", &ctx),
            Some(SKIP_SIZE_EXCEEDED)
        );
        assert_eq!(
            f.check_enqueue(100, false, "vd-a", &ctx),
            None,
            "等于阈值放行"
        );
        // ③ 目标设备
        let f = SyncFilter {
            target_device_ids: vec!["vd-work".into()],
            ..enabled()
        };
        assert_eq!(
            f.check_enqueue(10, false, "vd-personal", &ctx),
            Some(SKIP_DEVICE_NOT_TARGET)
        );
        assert_eq!(f.check_enqueue(10, false, "vd-work", &ctx), None);
        // ④ 逐设备 manual
        let f = SyncFilter {
            per_peer_modes: [("vd-x".to_string(), "manual".to_string())].into(),
            ..enabled()
        };
        assert_eq!(
            f.check_enqueue(10, false, "vd-x", &ctx),
            Some(SKIP_PEER_MODE)
        );
        assert_eq!(f.check_enqueue(10, false, "vd-y", &ctx), None);
    }

    /// 时段外 → `window_closed`；网络类型按白名单出站前拦下。
    #[test]
    fn window_and_net_type() {
        let f = enabled();
        let ctx = SyncContext {
            in_window: false,
            net_type: "wifi".into(),
        };
        assert_eq!(
            f.check_enqueue(10, false, "vd-a", &ctx),
            Some(SKIP_WINDOW_CLOSED)
        );
        assert_eq!(f.check_egress(&ctx), Some(SKIP_WINDOW_CLOSED));

        let f = SyncFilter {
            net_types: vec!["wifi".into(), "ethernet".into()],
            ..enabled()
        };
        let ctx = SyncContext {
            in_window: true,
            net_type: "cellular".into(),
        };
        assert_eq!(f.check_egress(&ctx), Some(SKIP_NET_TYPE_BLOCKED));
        let ctx = SyncContext {
            in_window: true,
            net_type: "wifi".into(),
        };
        assert_eq!(f.check_egress(&ctx), None, "白名单内放行");
        // 网络类型未知（空串）时不因白名单拦（避免误杀 → 静默停同步）
        let ctx = SyncContext {
            in_window: true,
            net_type: String::new(),
        };
        assert_eq!(f.check_egress(&ctx), None);
    }

    /// 未启用 = 默认全同步（策略读取失败时的口径；绝不解为全不同步）。
    #[test]
    fn disabled_means_sync_all() {
        let f = SyncFilter::default();
        let ctx = SyncContext {
            in_window: false,
            net_type: "cellular".into(),
        };
        assert_eq!(f.check_enqueue(999_999_999, true, "vd-a", &ctx), None);
        assert_eq!(f.check_egress(&ctx), None);
    }
}
