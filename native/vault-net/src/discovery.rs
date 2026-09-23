//! 局域网设备发现（P8-4，docs/v2.0/05-03 §三）。
//!
//! **安全定位（必须随实现与 UI 文案声明）**：发现**只解决「地址从哪来」**，
//! 不构成任何身份证明——同网段不代表可信。任何配对都必须经带外要素
//! （邀请码 / 扫码 / 短码比对）才能完成，**免手填 ≠ 免认证**。
//!
//! 隐私纪律（只减不增）：服务类型 `_vaultsync._tcp.local.`；实例名
//! `vs-<fp4>`（**不含主机名**——主机名常含用户名）；TXT 只有
//! `v=1` / `fp=<指纹前 8 hex>` / `caps=<能力摘要 4 hex>` / `port=<监听端口>`
//! 四项，**不广播** device_id、用户名、库名、文件数、条目数。
use std::collections::BTreeMap;
use std::net::IpAddr;

/// mDNS 服务类型（`01` §4.2）。
pub const SERVICE_TYPE: &str = "_vaultsync._tcp.local.";

/// 广播周期（失联判定 = 连续 3 个周期无更新 → lost；再 15 s 后移除）。
pub const ADVERTISE_PERIOD_MS: u64 = 15_000;
/// 发现列表保留时长（30 s 后移除，05-03 §三）。
pub const RETAIN_MS: u64 = 30_000;

/// 实例名 `vs-<fp4>`（公钥指纹前 4 hex；fp_hex 为全量 hex，取前 4 字符）。
pub fn instance_name(fp_hex: &str) -> String {
    format!("vs-{}", fp_hex.chars().take(4).collect::<String>())
}

/// 从实例名还原指纹前缀（校验用；不符合 `vs-` 前缀 → None）。
pub fn instance_fp_prefix(instance: &str) -> Option<&str> {
    instance.strip_prefix("vs-")
}

/// TXT 记录（**只减不增**：四项之外不得添加任何字段）。
pub fn txt_map(fp8_hex: &str, caps4_hex: &str, port: u16) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("v".to_string(), "1".to_string()),
        ("fp".to_string(), fp8_hex.chars().take(8).collect()),
        ("caps".to_string(), caps4_hex.chars().take(4).collect()),
        ("port".to_string(), port.to_string()),
    ])
}

/// 接口地址分类（05-03 §3.2）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AddrClass {
    /// RFC1918 私网。
    Lan,
    /// 169.254 / fe80::。
    LinkLocal,
    /// 全局。
    Public,
    /// 无法分类（loopback 等，直接排除）。
    Excluded,
}

pub fn classify_ip(ip: IpAddr) -> AddrClass {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            if o[0] == 10
                || (o[0] == 172 && (16..=31).contains(&o[1]))
                || (o[0] == 192 && o[1] == 168)
            {
                AddrClass::Lan
            } else if o[0] == 169 && o[1] == 254 {
                AddrClass::LinkLocal
            } else if o[0] == 127 || v4.is_unspecified() {
                AddrClass::Excluded
            } else {
                AddrClass::Public
            }
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() {
                AddrClass::Excluded
            } else if v6.segments()[0] & 0xffc0 == 0xfe80 {
                AddrClass::LinkLocal
            } else {
                AddrClass::Public
            }
        }
    }
}

/// 虚拟接口黑名单（VPN / 容器网桥 / Hyper-V 等**默认排除**——否则会出现
/// 「打洞成功但业务不通」的难查故障；用户可在设置显式勾选，判据：名称黑名单）。
pub fn is_virtual_interface(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [
        "vethernet",
        "hyper-v",
        "virtualbox",
        "vmware",
        "docker",
        "wsl",
        "loopback",
        "tap-",
        "tun-",
        "virbr",
        "vEthernet".to_ascii_lowercase().as_str(),
    ]
    .iter()
    .any(|kw| n.contains(kw))
}

/// 本机候选地址（05-03 §3.2）：非 loopback、排除虚拟接口、分类排序
/// （同网段判断需要对端网段信息，此处按 Lan > Public > LinkLocal），
/// **每端候选上限 8 个**。
pub fn local_candidates(interfaces: &[(String, IpAddr)]) -> Vec<IpAddr> {
    let mut lan = Vec::new();
    let mut public = Vec::new();
    let mut link = Vec::new();
    for (name, ip) in interfaces {
        if is_virtual_interface(name) {
            continue;
        }
        match classify_ip(*ip) {
            AddrClass::Lan => lan.push(*ip),
            AddrClass::Public => public.push(*ip),
            AddrClass::LinkLocal => link.push(*ip),
            AddrClass::Excluded => {}
        }
    }
    lan.extend(public);
    lan.extend(link);
    lan.truncate(8);
    lan
}

/// 发现候选（浏览结果条目）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Discovered {
    /// 实例名（`vs-<fp4>`）。
    pub instance: String,
    /// TXT 中的指纹前缀（8 hex）。
    pub fp: String,
    /// mDNS 解析出的地址（`A`/`SRV` 天然提供；TXT 不重复公告地址）。
    pub addrs: Vec<String>,
    /// TXT 端口。
    pub port: u16,
    /// 最近一次见到该实例的时刻（宿主时钟注入；失联/移除判据）。
    pub last_seen_ms: u64,
}

/// 浏览结果聚合（简单策略：同实例合并地址；调用方注入 now_ms 与
/// `retain_ms`（30 s）——超龄条目丢弃）。
pub fn aggregate(seen: &mut Vec<Discovered>, fresh: Discovered, now_ms: u64, retain_ms: u64) {
    seen.retain(|d| now_ms.saturating_sub(d.last_seen_ms) <= retain_ms);
    if let Some(e) = seen.iter_mut().find(|d| d.instance == fresh.instance) {
        e.last_seen_ms = fresh.last_seen_ms;
        for a in fresh.addrs {
            if !e.addrs.contains(&a) {
                e.addrs.push(a);
            }
        }
        e.fp = fresh.fp;
        e.port = fresh.port;
    } else {
        seen.push(fresh);
    }
}

/// 用 mdns-sd 守护进程广播本机服务（失败不致命——发现是增强载体，
/// 邀请码 / 扫码 / 手动三种载体仍在；返回 None = 广播不可用）。
pub fn advertise(
    daemon: &mdns_sd::ServiceDaemon,
    fp_hex: &str,
    caps4_hex: &str,
    port: u16,
) -> Option<mdns_sd::ServiceInfo> {
    let props = txt_map(fp_hex, caps4_hex, port)
        .into_iter()
        .collect::<Vec<_>>();
    let info = mdns_sd::ServiceInfo::new(
        SERVICE_TYPE,
        &instance_name(fp_hex),
        &format!("{}.{}", instance_name(fp_hex), SERVICE_TYPE),
        "0.0.0.0",
        port,
        &props[..],
    )
    .ok()?
    .enable_addr_auto();
    // 注册失败不致命——发现不可用如实降级（邀请码 / 手动载体仍在）
    daemon.register(info.clone()).ok()?;
    Some(info)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// 证据（面板 P8-4 `net::discover_txt_has_no_device_id`）：
    /// TXT 只含 v/fp/caps/port 四项，**不含** device_id / 用户名 / 库名。
    #[test]
    fn discover_txt_has_no_device_id() {
        let txt = txt_map("a3f12b0c9d8e7766", "0041", 51234);
        assert_eq!(txt.len(), 4, "TXT 只减不增：{txt:?}");
        assert_eq!(txt["v"], "1");
        assert_eq!(txt["fp"], "a3f12b0c");
        assert_eq!(txt["caps"], "0041");
        assert_eq!(txt["port"], "51234");
        for banned in ["device", "id", "name", "user", "vault", "files", "count"] {
            assert!(
                !txt.keys().any(|k| k.to_ascii_lowercase().contains(banned)),
                "TXT 不得包含 {banned} 类字段"
            );
        }
    }

    /// 实例名不含主机名：只取指纹前 4 hex。
    #[test]
    fn instance_name_hides_hostname() {
        assert_eq!(instance_name("a3f12b0c9d"), "vs-a3f1");
        assert_eq!(instance_fp_prefix("vs-a3f1"), Some("a3f1"));
        assert_eq!(instance_fp_prefix("my-laptop"), None, "非 vs- 前缀拒绝");
        // 反例：实例名构造不接收主机名参数
        let n = instance_name("ffffffff");
        assert!(!n.contains("desktop"), "实例名不得含主机名/用户名");
    }

    /// 虚拟接口默认排除 + 候选上限 8 + 分类排序。
    #[test]
    fn local_candidates_exclude_virtual_and_cap_at_8() {
        use std::net::Ipv4Addr;
        let ip = |a: [u8; 4]| IpAddr::V4(Ipv4Addr::from(a));
        let ifs = vec![
            ("Ethernet".to_string(), ip([192, 168, 1, 10])),
            ("vEthernet (WSL)".to_string(), ip([172, 20, 0, 1])),
            ("Hyper-V Virtual".to_string(), ip([10, 0, 0, 5])),
            ("Docker".to_string(), ip([172, 17, 0, 1])),
            ("Loopback Pseudo-Interface".to_string(), ip([127, 0, 0, 1])),
            ("WLAN".to_string(), ip([192, 168, 1, 11])),
            ("Tailscale".to_string(), ip([100, 64, 0, 1])), // public 类
            ("en0".to_string(), ip([169, 254, 3, 4])),      // link-local
        ];
        let got = local_candidates(&ifs);
        assert!(got.iter().all(|ip| !ip.is_loopback()), "loopback 必须排除");
        assert!(
            !got.contains(&ip([172, 20, 0, 1])),
            "WSL 虚拟接口必须默认排除"
        );
        assert!(got.contains(&ip([192, 168, 1, 10])));
        // 超 8 个候选截断
        let many: Vec<(String, IpAddr)> = (0..12)
            .map(|i| (format!("eth{i}"), ip([192, 168, i as u8 + 1, 2])))
            .collect();
        assert_eq!(local_candidates(&many).len(), 8, "候选上限 8");
    }

    /// 失联/移除判定：30 s 未更新 → 从聚合列表移除；同实例合并地址。
    #[test]
    fn aggregate_merges_and_expires() {
        let mut seen = Vec::new();
        let d = |t: u64, addr: &str| Discovered {
            instance: "vs-a3f1".into(),
            fp: "a3f12b0c".into(),
            addrs: vec![addr.to_string()],
            port: 51234,
            last_seen_ms: t,
        };
        aggregate(&mut seen, d(1000, "192.168.1.24:51234"), 1000, RETAIN_MS);
        aggregate(&mut seen, d(5000, "192.168.1.25:51234"), 5000, RETAIN_MS);
        assert_eq!(seen[0].addrs.len(), 2, "同实例地址合并");
        // 36 s 后（最后一次见到 5000，已超 30 s 保留期）→ 移除
        aggregate(&mut seen, d(36_000, "9.9.9.9:1"), 36_000, RETAIN_MS);
        assert_eq!(seen.len(), 1, "超龄条目必须移除，仅剩本次发现");
        assert_eq!(
            seen[0].addrs,
            vec!["9.9.9.9:1".to_string()],
            "旧条目的过期地址必须随条目一并消失"
        );
    }
}
