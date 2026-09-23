//! 路径状态机（P8-1 交付、P8-4 完善发现/打洞，docs/v2.0/05-03 §四）。
//!
//! `Direct → Punched → Relayed` 单向自动降级；每次迁移由宿主发
//! `PEER_STATE` / `PATH_DEGRADED`（带原因）。`p2p_path_status` 只给
//! 候选计数，**不给 IP 明文**（05-03 §四）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PathKind {
    /// 局域网 / 手填地址直连。
    Direct,
    /// UDP 打洞成功（P8-4）。
    Punched,
    /// 中继转发（末档）。
    Relayed,
}

impl PathKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PathKind::Direct => "direct",
            PathKind::Punched => "punched",
            PathKind::Relayed => "relayed",
        }
    }
}

/// 降级原因（`PATH_DEGRADED(13)` 的 `reason` 枚举：05-03 §4.2 六种
/// + `padding_downgrade`（05-04 §3.3）+ `proto_downgrade`（02 事件表））。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DegradeReason {
    /// 直连超时（≤3 s 未成功）。
    DirectTimeout,
    /// 打洞超时（≤5 s 未成功）。
    HolePunchTimeout,
    /// 对称 NAT（快速判定，不做无望长尝试）。
    SymmetricNat,
    /// 中继限额。
    RelayLimit,
    /// 中继不可达。
    RelayUnavailable,
    /// 填充档位降级。
    PaddingDowngrade,
    /// 对端协议能力不足。
    ProtoDowngrade,
}

impl DegradeReason {
    pub fn as_str(self) -> &'static str {
        match self {
            DegradeReason::DirectTimeout => "direct_timeout",
            DegradeReason::HolePunchTimeout => "hole_punch_timeout",
            DegradeReason::SymmetricNat => "symmetric_nat",
            DegradeReason::RelayLimit => "relay_limit",
            DegradeReason::RelayUnavailable => "relay_unavailable",
            DegradeReason::PaddingDowngrade => "padding_downgrade",
            DegradeReason::ProtoDowngrade => "proto_downgrade",
        }
    }
}

/// 路径状态机：只允许**降级方向**迁移（升级 = 断开重协商，不在线内升档）。
#[derive(Clone, Copy, Debug)]
pub struct PathMachine {
    pub current: PathKind,
    degraded_steps: u8,
}

impl PathMachine {
    pub fn new(start: PathKind) -> Self {
        Self {
            current: start,
            degraded_steps: 0,
        }
    }

    /// 自动降级：Direct → Punched → Relayed；已在末档则停留并报告 `RelayLimit`。
    /// 返回 `(新档位, 原因)` 供宿主发事件。
    pub fn degrade(&mut self, reason: DegradeReason) -> (PathKind, DegradeReason) {
        self.degraded_steps += 1;
        self.current = match self.current {
            PathKind::Direct => PathKind::Punched,
            _ => PathKind::Relayed,
        };
        (self.current, reason)
    }

    /// 本次连接累计降级步数（诊断口径）。
    pub fn steps(&self) -> u8 {
        self.degraded_steps
    }
}

/// 路径诊断快照（只计数、不含地址明文）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathStatus {
    pub direct_candidates: u32,
    pub punched_candidates: u32,
    pub relay_candidates: u32,
    pub current: Option<PathKind>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// 证据（面板 P8-1 `path::auto_downgrade_order`）：降级顺序固定
    /// Direct → Punched → Relayed，末档停留。
    #[test]
    fn auto_downgrade_order() {
        let mut m = PathMachine::new(PathKind::Direct);
        assert_eq!(m.current, PathKind::Direct);
        let (k, _) = m.degrade(DegradeReason::HolePunchTimeout);
        assert_eq!(k, PathKind::Punched);
        let (k, r) = m.degrade(DegradeReason::ProtoDowngrade);
        assert_eq!(k, PathKind::Relayed);
        assert_eq!(r.as_str(), "proto_downgrade");
        // 末档停留：不再有更低档
        let (k, r) = m.degrade(DegradeReason::RelayLimit);
        assert_eq!(k, PathKind::Relayed);
        assert_eq!(r, DegradeReason::RelayLimit);
        assert_eq!(m.steps(), 3);
    }

    /// 诊断快照只含计数（负向：不含地址字段）。
    #[test]
    fn path_status_counts_only() {
        let s = PathStatus {
            direct_candidates: 2,
            punched_candidates: 1,
            relay_candidates: 1,
            current: Some(PathKind::Direct),
        };
        let json = serde_json::to_string(
            &({
                let mut m = std::collections::BTreeMap::new();
                m.insert("direct", s.direct_candidates);
                m.insert("punched", s.punched_candidates);
                m.insert("relay", s.relay_candidates);
                m
            }),
        )
        .unwrap();
        assert!(!json.contains("127.0.0.1"), "路径诊断不得携带 IP 明文");
    }
}
