//! 块级三方合并（P8-8，docs/v2.0/05-03 §6.4）。
//!
//! **如实声明：这不是文本 / Office 语义合并**——按「共同祖先的 CDC 块边界」切段，
//! 逐段判「谁动了这一块」，不相交即拼接为新版本；相交则交冲突副本。
//! 基线缺失 / 段外长度不可比 → `Replace`（按新版本整文件提交）。
#![deny(unsafe_code)]

/// 合并三态（docs/05-03 §6.4）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeClass {
    /// 两侧改动块不相交 → 自动合并为 `rev+1`。
    Auto,
    /// 任一块被两侧同时改动 → 生成冲突副本。
    Conflict,
    /// 无法比较（缺基线 / 长度不可比）→ 按新版本整体提交。
    Replace,
}

impl MergeClass {
    pub fn as_str(self) -> &'static str {
        match self {
            MergeClass::Auto => "auto",
            MergeClass::Conflict => "conflict",
            MergeClass::Replace => "replace",
        }
    }
}

#[derive(Clone, Debug)]
pub struct MergeOutcome {
    pub class: MergeClass,
    /// `Auto` 时的合并结果（其余为 None）。
    pub merged: Option<Vec<u8>>,
    /// `Conflict` 时被两侧同时改动的段序号。
    pub conflict_segments: Vec<usize>,
}

/// 三方合并：`bounds` = 共同祖先的段边界（升序，含 0 与 base.len()）。
/// `local` / `peer` 任一短于 `base` → `Replace`（截断型改动无法按段比较）。
pub fn three_way(base: &[u8], local: &[u8], peer: &[u8], bounds: &[usize]) -> MergeOutcome {
    if bounds.len() < 2 || local.len() < base.len() || peer.len() < base.len() {
        return MergeOutcome {
            class: MergeClass::Replace,
            merged: None,
            conflict_segments: Vec::new(),
        };
    }
    let mut out: Vec<u8> = Vec::with_capacity(local.len().max(peer.len()));
    let mut conflicts: Vec<usize> = Vec::new();

    for (seg, w) in bounds.windows(2).enumerate() {
        let (s, e) = (w[0], w[1]);
        if e > base.len() || e > local.len() || e > peer.len() {
            return MergeOutcome {
                class: MergeClass::Replace,
                merged: None,
                conflict_segments: Vec::new(),
            };
        }
        let b = &base[s..e];
        let l = &local[s..e];
        let p = &peer[s..e];
        let local_changed = l != b;
        let peer_changed = p != b;
        match (local_changed, peer_changed) {
            (false, false) => out.extend_from_slice(b),
            (true, false) => out.extend_from_slice(l),
            (false, true) => out.extend_from_slice(p),
            (true, true) => {
                if l == p {
                    out.extend_from_slice(l); // 两侧改法相同 → 不算冲突
                } else {
                    conflicts.push(seg);
                    out.extend_from_slice(l); // 占位保留本端，实际由冲突副本承载
                }
            }
        }
    }
    // 段外尾部（任一侧更长）：按与段内相同的「谁改了这一块」规则裁决
    let tail_start = *bounds.last().unwrap_or(&0);
    let tb = &base[tail_start.min(base.len())..];
    let tl = &local[tail_start.min(local.len())..];
    let tp = &peer[tail_start.min(peer.len())..];
    match (tl != tb, tp != tb) {
        (false, false) => {}
        (true, false) => out.extend_from_slice(tl),
        (false, true) => out.extend_from_slice(tp),
        (true, true) => {
            if tl == tp {
                out.extend_from_slice(tl);
            } else {
                conflicts.push(bounds.len() - 1);
                out.extend_from_slice(tl);
            }
        }
    }

    if !conflicts.is_empty() {
        return MergeOutcome {
            class: MergeClass::Conflict,
            merged: None,
            conflict_segments: conflicts,
        };
    }
    MergeOutcome {
        class: MergeClass::Auto,
        merged: Some(out),
        conflict_segments: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 共同祖先：「hello world」按固定边界切 3 段。
    fn base_and_bounds() -> (Vec<u8>, Vec<usize>) {
        let base = b"AAAABBBBCCCC".to_vec();
        let bounds = vec![0, 4, 8, 12];
        (base, bounds)
    }

    /// 证据（面板 `conflict::three_way_block_merge_states`，三态之 auto）：
    /// 不相交改动 → 自动合并，两侧改动都在。
    #[test]
    fn disjoint_edits_auto_merge() {
        let (base, bounds) = base_and_bounds();
        let local = b"AAAAxxxxCCCC".to_vec(); // 改第 2 段
        let peer = b"AAAAmmmmCCCC".to_vec(); // 也改第 2 段 → 冲突
        let other = b"yyyyBBBBCCCC".to_vec(); // 改第 1 段
                                              // 独占第 2 段 ↔ 改第 1 段 → auto
        let m = three_way(&base, &local, &other, &bounds);
        assert_eq!(m.class, MergeClass::Auto);
        assert_eq!(m.merged.unwrap(), b"yyyyxxxxCCCC".to_vec());
        // 同一段被两侧改动 → conflict
        let c = three_way(&base, &local, &peer, &bounds);
        assert_eq!(c.class, MergeClass::Conflict);
        assert_eq!(c.conflict_segments, vec![1]);
        // 两侧改法相同 → 不算冲突
        let s = three_way(&base, &local, &local, &bounds);
        assert_eq!(s.class, MergeClass::Auto);
        assert_eq!(s.merged.unwrap(), local);
        // 无改动 → auto 且等于 base
        let n = three_way(&base, &base, &base, &bounds);
        assert_eq!(n.class, MergeClass::Auto);
        assert_eq!(n.merged.unwrap(), base);
    }

    /// 证据（三态之 replace）：缺基线（无边界）/ 长度不可比 → replace。
    #[test]
    fn missing_baseline_or_truncation_is_replace() {
        let (base, _) = base_and_bounds();
        assert_eq!(
            three_way(&base, &base, &base, &[0]).class,
            MergeClass::Replace
        );
        assert_eq!(
            three_way(&base, &base, &base, &[]).class,
            MergeClass::Replace
        );
        // 任一侧截断（短于 base）→ replace（无法按段比较）
        let short = b"AAAA".to_vec();
        assert_eq!(
            three_way(&base, &short, &base, &[0, 4, 8, 12]).class,
            MergeClass::Replace
        );
    }

    /// 尾部追加：仅一侧追加 → auto；两侧追加不同 → conflict。
    #[test]
    fn tail_growth_handling() {
        let (base, bounds) = base_and_bounds();
        let mut local = base.clone();
        local.extend_from_slice(b"L");
        let mut peer = base.clone();
        peer.extend_from_slice(b"P");
        assert_eq!(
            three_way(&base, &base, &local, &bounds).class,
            MergeClass::Auto
        );
        assert_eq!(
            three_way(&base, &local, &peer, &bounds).class,
            MergeClass::Conflict
        );
        assert_eq!(
            three_way(&base, &local, &local, &bounds).class,
            MergeClass::Auto
        );
    }
}
