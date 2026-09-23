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
const OBS_REQUEST: &[u8; 8] = b"VSOBS1\0\0";

/// 映射观测结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping {
    /// 观测目标（observer 的地址，用于区分「同一 socket 对不同目标」）。
    pub observer: SocketAddr,
    /// 观测到的本机公网映射 `ip:port`。
    pub mapped: SocketAddr,
}

/// 对称 NAT 判定：同一 socket 对两个不同目标的观测映射端口不同 → 对称
/// （05-03 §4.3 步骤 5：不做无望的长尝试，立即回落）。
pub fn is_symmetric(a: &Mapping, b: &Mapping) -> bool {
    a.mapped.port() != b.mapped.port()
}

/// 向观测目标发请求并读取「对方看到的本机地址」应答。
/// 应答格式见 [`PROBE_MAGIC`]；超时或格式错 → Err。
pub fn observe(
    sock: &UdpSocket,
    observer: SocketAddr,
    timeout: Duration,
) -> std::io::Result<SocketAddr> {
    sock.send_to(OBS_REQUEST, observer)?;
    sock.set_read_timeout(Some(timeout))?;
    let mut buf = [0u8; 32];
    let (n, from) = sock.recv_from(&mut buf)?;
    if n < 10 || buf[0..4] != PROBE_MAGIC[0..4] {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "bad observer reply",
        ));
    }
    let _ = from;
    let ip = std::net::Ipv4Addr::new(buf[4], buf[5], buf[6], buf[7]);
    let port = u16::from_be_bytes([buf[8], buf[9]]);
    Ok(SocketAddr::new(ip.into(), port))
}

/// 打洞：同时向对端全部候选发 binding 探测；收到任一候选的探测/响应
/// 即锁定该组合并返回对端地址。超时返回 None（调用方降级 Relayed）。
pub fn punch(
    sock: &UdpSocket,
    peer_candidates: &[SocketAddr],
    timeout: Duration,
) -> Option<SocketAddr> {
    sock.set_read_timeout(Some(Duration::from_millis(100)))
        .ok()?;
    let deadline = std::time::Instant::now() + timeout;
    let mut last_sent = None;
    while std::time::Instant::now() < deadline {
        // 周期性重发探测（NAT 映射保活）
        let need_send = last_sent
            .map(|t: std::time::Instant| t.elapsed() > Duration::from_millis(300))
            .unwrap_or(true);
        if need_send {
            for cand in peer_candidates {
                let _ = sock.send_to(PROBE_MAGIC, *cand);
            }
            last_sent = Some(std::time::Instant::now());
        }
        let mut buf = [0u8; 16];
        if let Ok((n, from)) = sock.recv_from(&mut buf) {
            if n >= PROBE_MAGIC.len() && &buf[..PROBE_MAGIC.len()] == PROBE_MAGIC {
                // 收到对端探测 → 回一个（让对端也锁定），并锁定该组合
                let _ = sock.send_to(PROBE_MAGIC, from);
                return Some(from);
            }
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

    /// 证据（面板 P8-4 `punch::symmetric_nat_falls_back_fast`）：
    /// 同一 socket 对两个观测目标的映射端口不同 → 立即判对称 NAT 回落，
    /// 全程无长等待（毫秒级完成）。
    #[test]
    fn symmetric_nat_falls_back_fast() {
        // 本地双观测端点（模拟两个不同目标）：一个诚实应答，一个谎报端口
        let o1 = udp(0);
        let o1_addr = o1.local_addr().unwrap();
        let o2 = udp(0);
        let o2_addr = o2.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8];
            let (n, from) = o1.recv_from(&mut buf).unwrap();
            let _ = n;
            // 诚实观测：应答对端真实来源
            let ip = match from.ip() {
                std::net::IpAddr::V4(v4) => v4,
                _ => return,
            };
            let mut reply = [0u8; 10];
            reply[0..4].copy_from_slice(&PROBE_MAGIC[0..4]);
            reply[4..8].copy_from_slice(&ip.octets());
            reply[8..10].copy_from_slice(&from.port().to_be_bytes());
            let _ = o1.send_to(&reply, from);
        });
        std::thread::spawn(move || {
            let mut buf = [0u8; 8];
            let (n, from) = o2.recv_from(&mut buf).unwrap();
            let _ = n;
            let ip = match from.ip() {
                std::net::IpAddr::V4(v4) => v4,
                _ => return,
            };
            let mut reply = [0u8; 10];
            reply[0..4].copy_from_slice(&PROBE_MAGIC[0..4]);
            reply[4..8].copy_from_slice(&ip.octets());
            // 谎报端口（模拟对称 NAT 对不同目标分配不同映射）
            reply[8..10].copy_from_slice(&from.port().wrapping_add(7777).to_be_bytes());
            let _ = o2.send_to(&reply, from);
        });
        let sock = udp(0);
        let t0 = std::time::Instant::now();
        let mapped1 = observe(&sock, o1_addr, Duration::from_secs(2)).unwrap();
        let mapped2 = observe(&sock, o2_addr, Duration::from_secs(2)).unwrap();
        let m1 = Mapping {
            observer: o1_addr,
            mapped: mapped1,
        };
        let m2 = Mapping {
            observer: o2_addr,
            mapped: mapped2,
        };
        assert!(is_symmetric(&m1, &m2), "谎报端口必须被判为对称 NAT");
        assert!(
            t0.elapsed() < Duration::from_secs(2),
            "对称 NAT 判定必须快速（{t0:?}）——不做无望的长尝试"
        );
        // 回落语义：对称 → 立即建议 Relayed（path 状态机由宿主执行）
    }

    /// 非对称 NAT：两个观测目标端口一致 → 不判对称，进入打洞。
    #[test]
    fn consistent_mapping_not_symmetric() {
        let o1 = udp(0);
        let o1_addr = o1.local_addr().unwrap();
        let o2 = udp(0);
        let o2_addr = o2.local_addr().unwrap();
        for o in [o1, o2] {
            std::thread::spawn(move || {
                let mut buf = [0u8; 8];
                let (n, from) = o.recv_from(&mut buf).unwrap();
                let _ = n;
                let ip = match from.ip() {
                    std::net::IpAddr::V4(v4) => v4,
                    _ => return,
                };
                let mut reply = [0u8; 10];
                reply[0..4].copy_from_slice(&PROBE_MAGIC[0..4]);
                reply[4..8].copy_from_slice(&ip.octets());
                reply[8..10].copy_from_slice(&from.port().to_be_bytes());
                let _ = o.send_to(&reply, from);
            });
        }
        let sock = udp(0);
        let mapped1 = observe(&sock, o1_addr, Duration::from_secs(2)).unwrap();
        let mapped2 = observe(&sock, o2_addr, Duration::from_secs(2)).unwrap();
        let m1 = Mapping {
            observer: o1_addr,
            mapped: mapped1,
        };
        let m2 = Mapping {
            observer: o2_addr,
            mapped: mapped2,
        };
        assert!(!is_symmetric(&m1, &m2), "同端口映射 = 锥形 NAT，继续打洞");
    }

    /// 打洞成功路径：本地双 socket 互发 binding 探测 → 双方锁定。
    #[test]
    fn punch_succeeds_on_reachable_candidate() {
        let a = udp(0);
        let b = udp(0);
        let b_addr = b.local_addr().unwrap();
        // B 端：收到探测后向来源回发 binding 应答
        std::thread::spawn(move || {
            let mut buf = [0u8; 16];
            if let Ok((n, from)) = b.recv_from(&mut buf) {
                if n >= PROBE_MAGIC.len() {
                    let _ = b.send_to(PROBE_MAGIC, from);
                }
            }
        });
        let got = punch(&a, &[b_addr], Duration::from_secs(3));
        assert!(got.is_some(), "可达候选必须打洞成功");
    }

    /// 打洞超时 → None（调用方降级 Relayed）。
    #[test]
    fn punch_times_out_returns_none() {
        let a = udp(0);
        // 无对端候选
        assert!(punch(&a, &[], Duration::from_millis(300)).is_none());
    }
}
