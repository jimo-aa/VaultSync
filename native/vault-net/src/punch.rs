//! UDP 打洞（P8-4，docs/v2.0/05-03 §4.3）。
//!
//! **不自建 STUN 服务**：映射观测「只借中继」（VSR2 的观测端点，P8-5）；
//! 打洞步骤：双方经中继信令交换候选 → 各自做映射观测 → 同时向对方全部候选
//! （含公网映射）发 binding 探测 → 任一组合同步成功即 `Punched`。
//!
//! **对称 NAT 快速判定**：同一 socket 对不同观测目标看到的映射端口不同
//! → 判 `symmetric_nat`，**不做无望的长尝试**，立即回落 `Relayed`。
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

/// binding 探测魔数（8B；观测应答 = `[magic 4B][ip 4B][port u16 BE]`，仅 IPv4）。
pub const PROBE_MAGIC: &[u8; 8] = b"VSPUNCH1";
/// 观测请求（8B）。
const OBS_REQUEST: &[u8; 8] = b"VSOBS1\0\0";
/// 观测 / 打洞**反射应答**共用的魔数前缀（4B）。
const REFLEX_MAGIC: &[u8; 4] = b"VSPU";
/// 反射应答长度（magic4 ‖ ip4 ‖ port2）。
const REFLEX_LEN: usize = 10;

/// 映射观测结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping {
    /// 观测目标（observer 的地址，用于区分「同一 socket 对不同目标」）。
    pub observer: SocketAddr,
    /// 观测到的本机公网映射 `ip:port`。
    pub mapped: SocketAddr,
}

/// 打洞结果（P8-4）：**双向可达**验证 + 本端在对端眼中的映射。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PunchOutcome {
    /// 对端地址（收到对端探测/应答的来源地址，即锁定的对端映射）。
    pub peer: SocketAddr,
    /// 本端在**对端**眼中的映射（= 对端回给我们的反射应答；观测 #2，
    /// 与 [`observe`] 的观测 #1 比对即得对称 NAT 判定）。
    pub self_mapped_by_peer: Option<SocketAddr>,
    /// 是否收到对端探测（证明本端候选对端可达）。
    pub peer_probe_seen: bool,
}

/// 对称 NAT 判定：同一 socket 对两个不同目标的观测映射端口不同 → 对称
/// （05-03 §4.3 步骤 5：不做无望的长尝试，立即回落）。
pub fn is_symmetric(a: &Mapping, b: &Mapping) -> bool {
    a.mapped.port() != b.mapped.port()
}

/// 反射应答编码：把「我看到的你的来源地址」编成 10 B（仅 IPv4）。
pub fn encode_reflexion(seen: SocketAddr) -> Option<[u8; REFLEX_LEN]> {
    let v4 = match seen.ip() {
        std::net::IpAddr::V4(v4) => v4,
        std::net::IpAddr::V6(_) => return None,
    };
    let mut out = [0u8; REFLEX_LEN];
    out[0..4].copy_from_slice(REFLEX_MAGIC);
    out[4..8].copy_from_slice(&v4.octets());
    out[8..10].copy_from_slice(&seen.port().to_be_bytes());
    Some(out)
}

/// 反射应答解码（长度与魔数不符 → None）。
pub fn decode_reflexion(buf: &[u8]) -> Option<SocketAddr> {
    if buf.len() < REFLEX_LEN || &buf[0..4] != REFLEX_MAGIC {
        return None;
    }
    let ip = std::net::Ipv4Addr::new(buf[4], buf[5], buf[6], buf[7]);
    let port = u16::from_be_bytes([buf[8], buf[9]]);
    Some(SocketAddr::new(ip.into(), port))
}

/// 向观测目标发请求并读取「对方看到的本机地址」应答。
/// 应答格式见 [`encode_reflexion`]；超时或格式错 → Err。
pub fn observe(
    sock: &UdpSocket,
    observer: SocketAddr,
    timeout: Duration,
) -> std::io::Result<SocketAddr> {
    sock.send_to(OBS_REQUEST, observer)?;
    sock.set_read_timeout(Some(timeout))?;
    let mut buf = [0u8; 32];
    let (n, _from) = sock.recv_from(&mut buf)?;
    decode_reflexion(&buf[..n])
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "bad observer reply"))
}

/// 打洞（**双向验证**，05-03 §4.3 步骤 3–5）：向对端全部候选周期发 binding 探测；
/// 收到对端探测即回**反射应答**（告诉对端「你在本次对话中对我而言是哪个地址」）；
/// 收到对端反射应答即得本端在对端眼中的映射。
///
/// **返回条件**：两个方向都已证实——既收到过对端探测，也收到过对端反射应答
/// （后者证明我们的探测确实到达了对端、对端的应答也回到了我们）。仅收到一边
/// 不返回（避免单侧自嗨式的「假成功」）；超时/候选为空 → None（调用方降级）。
pub fn punch_verified(
    sock: &UdpSocket,
    peer_candidates: &[SocketAddr],
    timeout: Duration,
) -> Option<PunchOutcome> {
    if peer_candidates.is_empty() {
        return None;
    }
    sock.set_read_timeout(Some(Duration::from_millis(100)))
        .ok()?;
    let deadline = std::time::Instant::now() + timeout;
    let mut peer_probe: Option<SocketAddr> = None;
    let mut self_mapped: Option<SocketAddr> = None;
    let mut last_sent: Option<std::time::Instant> = None;
    while std::time::Instant::now() < deadline {
        // 周期性重发探测（NAT 映射保活；300 ms 间隔）
        let need_send = last_sent
            .map(|t: std::time::Instant| t.elapsed() > Duration::from_millis(300))
            .unwrap_or(true);
        if need_send {
            for cand in peer_candidates {
                let _ = sock.send_to(PROBE_MAGIC, *cand);
            }
            last_sent = Some(std::time::Instant::now());
        }
        let mut buf = [0u8; 32];
        if let Ok((n, from)) = sock.recv_from(&mut buf) {
            if n == PROBE_MAGIC.len() && buf[..PROBE_MAGIC.len()] == *PROBE_MAGIC {
                // 对端探测：回反射应答（仅此一处应答，且只回来源地址）
                peer_probe = Some(from);
                if let Some(reply) = encode_reflexion(from) {
                    let _ = sock.send_to(&reply, from);
                }
            } else if let Some(mine) = decode_reflexion(&buf[..n]) {
                // 对端的反射应答 = 本端在对端眼中的映射（观测 #2）
                self_mapped = Some(mine);
                if peer_probe.is_none() {
                    peer_probe = Some(from);
                }
            }
        }
        if peer_probe.is_some() && self_mapped.is_some() {
            return Some(PunchOutcome {
                peer: peer_probe.unwrap_or_else(|| peer_candidates[0]),
                self_mapped_by_peer: self_mapped,
                peer_probe_seen: true,
            });
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn udp(port: u16) -> UdpSocket {
        let s = UdpSocket::bind(("127.0.0.1", port)).unwrap();
        s.set_nonblocking(false).unwrap();
        s
    }

    /// 反射应答编解码往返 + 边界（IPv6 不可编码、长度不符拒绝）。
    #[test]
    fn reflexion_codec_roundtrips() {
        let a: SocketAddr = "203.0.113.7:41234".parse().unwrap();
        let enc = encode_reflexion(a).expect("IPv4 可编码");
        assert_eq!(enc.len(), REFLEX_LEN);
        assert_eq!(decode_reflexion(&enc), Some(a));
        assert_eq!(decode_reflexion(&enc[..9]), None, "长度不足必须拒绝");
        assert_eq!(decode_reflexion(b"XXXXXXXXXX"), None, "魔数不符必须拒绝");
        let v6: SocketAddr = "[2001:db8::1]:9".parse().unwrap();
        assert!(encode_reflexion(v6).is_none(), "仅 IPv4");
    }

    /// 证据（面板 P8-4 `punch::symmetric_nat_falls_back_fast`）：
    /// 同一 socket 对两个观测目标的映射端口不同 → 立即判对称 NAT 回落，
    /// 全程无长等待（毫秒级完成）。
    #[test]
    fn symmetric_nat_falls_back_fast() {
        // 第二个观测目标谎报端口（模拟对称 NAT：同一 socket 对不同目标映射端口不同）
        let (o1_addr, o2_addr) = spawn_observers([0, 7777]);
        let sock = udp(0);
        let t0 = std::time::Instant::now();
        let m1 = Mapping {
            observer: o1_addr,
            mapped: observe(&sock, o1_addr, Duration::from_secs(2)).unwrap(),
        };
        let m2 = Mapping {
            observer: o2_addr,
            mapped: observe(&sock, o2_addr, Duration::from_secs(2)).unwrap(),
        };
        assert!(is_symmetric(&m1, &m2), "谎报端口必须被判为对称 NAT");
        assert!(
            t0.elapsed() < Duration::from_secs(2),
            "对称 NAT 判定必须快速（{t0:?}）——不做无望的长尝试"
        );
    }

    /// 非对称 NAT：两个观测目标端口一致 → 不判对称，进入打洞。
    #[test]
    fn consistent_mapping_not_symmetric() {
        let (o1_addr, o2_addr) = spawn_observers([0, 0]);
        let sock = udp(0);
        let m1 = Mapping {
            observer: o1_addr,
            mapped: observe(&sock, o1_addr, Duration::from_secs(2)).unwrap(),
        };
        let m2 = Mapping {
            observer: o2_addr,
            mapped: observe(&sock, o2_addr, Duration::from_secs(2)).unwrap(),
        };
        assert!(!is_symmetric(&m1, &m2), "同端口映射 = 锥形 NAT，继续打洞");
    }

    /// 双观测端点：`deltas[0]` / `deltas[1]` 分别为各自对端口报告的偏移量
    /// （0 = 诚实应答；非 0 = 谎报端口，用于模拟对称 NAT 对不同目标的不同映射）。
    fn spawn_observers(deltas: [u16; 2]) -> (SocketAddr, SocketAddr) {
        let mut addrs = Vec::new();
        for delta in deltas {
            let o = udp(0);
            addrs.push(o.local_addr().unwrap());
            std::thread::spawn(move || {
                let mut buf = [0u8; 32];
                let (n, from) = o.recv_from(&mut buf).unwrap();
                let _ = n;
                let Some(mut reply) = encode_reflexion(from) else {
                    return;
                };
                if delta != 0 {
                    let p = from.port().wrapping_add(delta);
                    reply[8..10].copy_from_slice(&p.to_be_bytes());
                }
                let _ = o.send_to(&reply, from);
            });
        }
        (addrs[0], addrs[1])
    }

    /// 打洞成功：两端各自探测 + 反射应答 → **双向证实**才返回，
    /// 且本端能读到「对端眼中的自己」（观测 #2）。
    #[test]
    fn punch_succeeds_on_reachable_candidate() {
        let a = udp(0);
        let b = udp(0);
        let b_addr = b.local_addr().unwrap();
        // B 端模拟对端打洞器：收到探测即回反射应答；也主动向 A 发探测
        let a_addr = a.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut buf = [0u8; 32];
            let _ = b.send_to(PROBE_MAGIC, a_addr);
            for _ in 0..40 {
                if let Ok((n, from)) = b.recv_from(&mut buf) {
                    if n == 8 && buf[..8] == *PROBE_MAGIC {
                        if let Some(r) = encode_reflexion(from) {
                            let _ = b.send_to(&r, from);
                        }
                    }
                }
            }
        });
        let got =
            punch_verified(&a, &[b_addr], Duration::from_secs(3)).expect("可达候选必须打洞成功");
        assert_eq!(got.peer, b_addr, "锁定的对端地址");
        assert!(got.peer_probe_seen, "必须收到过对端探测");
        assert_eq!(
            got.self_mapped_by_peer,
            Some(a_addr),
            "必须读到本端在对端眼中的映射（观测 #2）"
        );
    }

    /// 打洞超时 → None（调用方降级 Relayed）；空候选直接 None，不做无望等待。
    #[test]
    fn punch_times_out_returns_none() {
        let a = udp(0);
        assert!(punch_verified(&a, &[], Duration::from_millis(300)).is_none());
        let dead: SocketAddr = "127.0.0.1:9".parse().unwrap();
        assert!(punch_verified(&a, &[dead], Duration::from_millis(400)).is_none());
    }

    /// **负向**：只收到对端探测、收不到对端反射应答（对方不守反射协议）→ 不返回，
    /// 不得报「打洞成功」（单侧可达不算打通）。
    #[test]
    fn one_sided_contact_is_not_success() {
        let a = udp(0);
        let a_addr = a.local_addr().unwrap();
        let b = udp(0);
        let b_addr = b.local_addr().unwrap();
        std::thread::spawn(move || {
            // 只发裸探测、不回反射应答
            let _ = b.send_to(PROBE_MAGIC, a_addr);
            std::thread::sleep(Duration::from_millis(500));
        });
        assert!(
            punch_verified(&a, &[b_addr], Duration::from_millis(400)).is_none(),
            "单侧接触不得判定为打通"
        );
    }
}
