//! VSR2 中继客户端（P8-5，docs/v2.0/08 §一.2/§三）。
//!
//! 职责：挑战-应答接入（token 绝不明文传输，只发 HMAC proof）+ 数据阶段帧适配——
//! 对上层（`SecureChannel` / 同步编排）呈现为普通 `Read + Write` 字节流，
//! `u32 BE len ‖ type ‖ payload` 帧封装在本层内部完成。PING 保活由独立线程承担
//! （官方公共档空闲上限 60 s，25 s 间隔留 2 倍余量；PING 不计入中继字节上限）。
//!
//! 错误纪律（docs/08 §3.4）：中继错误码 R1–R17 与 `vault_core_err_e` **不是同一
//! 空间**——本模块把 R 码映射为本机码 {3, 6, 7, 13} + 可读诊断 + PATH_DEGRADED
//! reason（如适用），绝不把 R 码原样上抛给 UI。`RelayLimit 13`（带宽降速）只
//! 记录不关闭——它是降速通知而非断连指令。
//!
//! 中继不可见性对客户端的镜像：线上只出现 HELLO/AUTH 控制行与不透明 DATA 帧，
//! token 只以 HMAC 输出形态出现（`token_is_never_sent_in_clear` 的客户端侧对偶）。
#![deny(unsafe_code)]

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use vault_crypto::{hex_encode, hmac_sha256_hex, random_bytes};

/// 数据帧类型（与 `vault-relay` proto 一致：1 DATA / 2 CONTROL / 3 PING）。
const TYPE_DATA: u8 = 1;
const TYPE_CONTROL: u8 = 2;
const TYPE_PING: u8 = 3;
/// 线上帧 `len` 上限（= 1 + DATA 载荷上限，docs/08 §1.2）。
const MAX_FRAME: usize = 65_541;
/// DATA 载荷上限。
const DATA_MAX: usize = MAX_FRAME - 1;
/// 控制行上限（与中继一致）。
const MAX_LINE: usize = 512;
/// 客户端能力摘要：bit0 填充 / bit1 `.part` 续传 / bit2 OBSERVE（docs/08 §1.2）。
pub const CLIENT_CAPS: &str = "0007";
/// 保活间隔：官方公共档空闲上限 60 s 的 1/2 强。
const PING_INTERVAL: Duration = Duration::from_secs(25);

/// 中继接入 / 会话失败的结构化描述（docs/08 §3.4 映射纪律的载体）。
#[derive(Debug, Clone)]
pub struct RelayFailure {
    /// 中继错误码 R1–R17；本地产失败为 0。
    pub relay_code: u8,
    /// 原因 token（如 `auth-failed`）或本地原因（`unreachable` / `bad-room`…）。
    pub reason: String,
    /// 映射后的本机错误码：3 = 网络/IO，6 = 协议格式，7 = 参数，13 = 路径受限不可用。
    pub local_code: i32,
    /// PATH_DEGRADED(13) 事件的 reason（docs/08 §3.4 映射列）；None = 不发布该事件。
    pub degrade_reason: Option<&'static str>,
    /// 可读诊断（进引擎事件日志，供 UI 展开说明）。
    pub diag: String,
}

impl RelayFailure {
    /// 本模块 / 引擎侧自造失败（中继未参与：握手、Hello、引擎状态等）。
    pub(crate) fn local(reason: &str, code: i32, diag: impl Into<String>) -> Self {
        Self {
            relay_code: 0,
            reason: reason.to_string(),
            local_code: code,
            degrade_reason: None,
            diag: diag.into(),
        }
    }

    /// 网络层失败（connect / IO）。本机码 3。
    pub fn unreachable(diag: impl Into<String>) -> Self {
        Self::local("unreachable", 3, diag)
    }

    /// 参数层失败（房间名 / token 不合规）。本机码 7——调用方传参错误，
    /// 在拨号前拦截，省一次注定失败的往返。
    pub fn invalid_arg(reason: &str, diag: impl Into<String>) -> Self {
        Self::local(reason, 7, diag)
    }

    /// 由中继 `ERR <R#> <token>` 行构造（docs/08 §3.4 映射表）。
    fn from_relay_line(code: u8, token: &str) -> Self {
        let (local_code, degrade_reason, hint): (i32, Option<&'static str>, &str) = match code {
            1..=2 => (6, None, "中继协议版本不支持：请升级客户端"),
            3 => (7, None, "房间名非法（仅允许字母数字与 - _）"),
            4 => (3, None, "未在时限内完成鉴权（可重试一次）"),
            5 => (
                3,
                Some("relay_unavailable"),
                "中继鉴权失败：请核对中继 token",
            ),
            6..=7 => (3, None, "中继时间窗 / 重放校验未通过（可重试一次）"),
            8..=9 => (3, None, "中继连接数 / 新建频率超限（请稍后重试）"),
            10..=12 => (
                13,
                Some("relay_limit"),
                "中继会话达到上限（时长 / 空闲 / 字节）",
            ),
            14..=15 => (
                13,
                Some("relay_unavailable"),
                "中继不可用（满载或房间过期）",
            ),
            16 => (6, None, "中继帧协议违约"),
            17 => (3, None, "中继内部错误"),
            _ => (3, None, "中继返回未知错误"),
        };
        Self {
            relay_code: code,
            reason: token.to_string(),
            local_code,
            degrade_reason,
            diag: format!("relay R{code} {token}: {hint}"),
        }
    }
}

/// 房间名校验（与中继 `valid_room` 同口径：`[A-Za-z0-9_-]`，≤128）。
fn valid_room(room: &str) -> bool {
    !room.is_empty()
        && room.len() <= 128
        && room
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn build_frame(ty: u8, payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(4 + 1 + payload.len());
    f.extend_from_slice(&((payload.len() + 1) as u32).to_be_bytes());
    f.push(ty);
    f.extend_from_slice(payload);
    f
}

/// 读取一行控制行（LF 结尾；**不用 BufReader**——其缓冲会吞掉后续数据帧）。
fn read_ctl_line(mut sock: &TcpStream) -> io::Result<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let n = sock.read(&mut byte)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "relay closed during handshake",
            ));
        }
        if byte[0] == b'\n' {
            break;
        }
        line.push(byte[0]);
        if line.len() > MAX_LINE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "relay control line too long",
            ));
        }
    }
    Ok(String::from_utf8_lossy(&line)
        .trim_end_matches('\r')
        .to_string())
}

fn parse_err_line(line: &str) -> Option<RelayFailure> {
    let rest = line.strip_prefix("VSR2 ERR ")?;
    let (code_s, token) = rest.split_once(' ')?;
    let code = code_s.parse::<u8>().ok()?;
    Some(RelayFailure::from_relay_line(code, token))
}

/// P8-4：解析 OK 行捎带的 UDP 观测端点。
/// 字段序：`VSR2`(0) `OK`(1) `<session_id>`(2) `<caps>`(3) `<hint>`(4…)；
/// 对后续追加字段宽容（取第一个非 `-` 且含 `:` 的字段），未启用 → 空。
fn parse_observe_hint(line: &str) -> Vec<SocketAddr> {
    line.split(' ')
        .skip(4)
        .find(|f| *f != "-" && f.contains(':'))
        .map(|h| {
            h.split(',')
                .filter_map(|s| s.parse::<SocketAddr>().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// VSR2 接入：HELLO → CHALLENGE → AUTH → OK（docs/08 §3.1 六步流转）。
/// 返回帧适配流；失败返回结构化 `RelayFailure`。
pub fn connect(relay_addr: &str, room: &str, token: &str) -> Result<RelayStream, RelayFailure> {
    if !valid_room(room) {
        return Err(RelayFailure::invalid_arg(
            "bad-room",
            "房间名非法：仅允许字母数字与 - _，长度 ≤128",
        ));
    }
    // 中继启动即拒绝 <128 位 token（docs/08 §3.2），客户端同口径提前拦截。
    if token.len() < 16 {
        return Err(RelayFailure::invalid_arg(
            "short-token",
            "中继 token 短于 128 位（16 字节）：合规中继必拒，请核对配置",
        ));
    }
    let sock = TcpStream::connect(relay_addr)
        .map_err(|e| RelayFailure::unreachable(format!("中继不可达 {relay_addr}: {e}")))?;
    let _ = sock.set_nodelay(true);
    sock.set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| RelayFailure::unreachable(e.to_string()))?;
    let peer = sock
        .peer_addr()
        .map_err(|e| RelayFailure::unreachable(e.to_string()))?;
    let mut wr = sock
        .try_clone()
        .map_err(|e| RelayFailure::unreachable(e.to_string()))?;

    // cli_nonce：CSPRNG 16B → 32 hex；被 proof 绑定，防跨连接重放（docs/08 §3.1）
    let cli_nonce = hex_encode(&random_bytes(16));
    writeln!(wr, "VSR2 HELLO 2 {room} {cli_nonce} {CLIENT_CAPS}")
        .map_err(|e| RelayFailure::unreachable(e.to_string()))?;

    let line = read_ctl_line(&sock).map_err(|e| RelayFailure::unreachable(e.to_string()))?;
    if let Some(f) = parse_err_line(&line) {
        return Err(f);
    }
    let parts: Vec<&str> = line.split(' ').collect();
    if parts.len() != 5 || parts[0] != "VSR2" || parts[1] != "CHALLENGE" {
        return Err(RelayFailure::local(
            "bad-challenge",
            6,
            "中继应答不是 CHALLENGE（协议不匹配？）",
        ));
    }
    let srv_nonce = parts[2].to_string();
    let ts_bucket: u64 = parts[3]
        .parse()
        .map_err(|_| RelayFailure::local("bad-challenge", 6, "CHALLENGE 时间窗不可解析"))?;
    // proof = HMAC-SHA256(token, "VSR2-AUTH" ‖ room ‖ cli ‖ srv ‖ ts_bucket)，
    // 与服务端逐字段一致（ts_bucket 以十进制文本拼入）。
    let proof = hmac_sha256_hex(
        token.as_bytes(),
        format!("VSR2-AUTH{room}{cli_nonce}{srv_nonce}{ts_bucket}").as_bytes(),
    );
    writeln!(wr, "VSR2 AUTH {proof}").map_err(|e| RelayFailure::unreachable(e.to_string()))?;

    let line = read_ctl_line(&sock).map_err(|e| RelayFailure::unreachable(e.to_string()))?;
    if let Some(f) = parse_err_line(&line) {
        return Err(f);
    }
    if !line.starts_with("VSR2 OK ") {
        return Err(RelayFailure::local(
            "bad-ok",
            6,
            "中继应答不是 OK（协议不匹配？）",
        ));
    }
    // P8-4：OK 行第 4 字段（可选）捎带 UDP 观测端点（形如 `ip:port[,ip:port]`；`-` = 未启用）
    let observe = parse_observe_hint(&line);
    // 握手期 10 s 超时完成使命；会话超时由上层按需设置（try_clone 共享 socket 选项）
    let _ = sock.set_read_timeout(None);
    RelayStream::new(sock, wr, peer, observe)
}

/// VSR2 数据阶段的帧适配流：上层把它当普通字节流，帧封装 / 解复用 / PING
/// 保活 / RelayLimit 处理都在本类型内部。
///
/// **读语义**：DATA 帧载荷按序吐出；PING 帧消费掉（保活语义由中继承担）；
/// `RelayLimit R13`（带宽降速）记录后**继续**；其余 CONTROL → `ConnectionAborted`
/// 错误（可读诊断进错误消息——这是「超限优雅关闭 + 可读原因」的客户端对偶）。
#[derive(Debug)]
pub struct RelayStream {
    rd: TcpStream,
    wr: Arc<Mutex<TcpStream>>,
    peer: SocketAddr,
    /// P8-4：中继捎带的 UDP 观测端点（空 = 未启用 → 上层不做打洞尝试）。
    observe: Vec<SocketAddr>,
    buf: Vec<u8>,
    pos: usize,
    wire_in: Arc<AtomicU64>,
    wire_out: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
}

impl RelayStream {
    fn new(
        rd: TcpStream,
        wr: TcpStream,
        peer: SocketAddr,
        observe: Vec<SocketAddr>,
    ) -> Result<Self, RelayFailure> {
        let wr = Arc::new(Mutex::new(wr));
        let wire_in = Arc::new(AtomicU64::new(0));
        let wire_out = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let ping_wr = Arc::clone(&wr);
        let ping_stop = Arc::clone(&stop);
        std::thread::Builder::new()
            .name("vsr2-ping".into())
            .spawn(move || loop {
                std::thread::sleep(PING_INTERVAL);
                if ping_stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut w) = ping_wr.lock() else {
                    break;
                };
                let f = build_frame(TYPE_PING, &[]);
                if w.write_all(&f).is_err() || w.flush().is_err() {
                    break;
                }
            })
            .map_err(|e| RelayFailure::unreachable(e.to_string()))?;
        Ok(Self {
            rd,
            wr,
            peer,
            observe,
            buf: Vec::new(),
            pos: 0,
            wire_in,
            wire_out,
            stop,
        })
    }

    /// 中继地址（观测 / 诊断用；**不是**对端设备地址）。
    pub fn relay_addr(&self) -> SocketAddr {
        self.peer
    }

    /// P8-4：中继的 UDP 映射观测端点（空 = 中继未启用观测 → 不做打洞尝试）。
    pub fn observe_addrs(&self) -> Vec<SocketAddr> {
        self.observe.clone()
    }

    /// 设置读超时（共享底层 socket 选项；宿主与直连路径对齐时调用）。
    pub fn set_read_timeout(&self, d: Option<Duration>) -> io::Result<()> {
        self.rd.set_read_timeout(d)
    }

    /// 线上字节计数的共享句柄（宿主在流被移入会话后仍可读取终值）。
    pub fn wire_arcs(&self) -> (Arc<AtomicU64>, Arc<AtomicU64>) {
        (Arc::clone(&self.wire_in), Arc::clone(&self.wire_out))
    }

    /// 线上字节计数（帧级：含 4B 前缀与 type 字节，不含等待期前置注入）。
    pub fn wire_counters(&self) -> (u64, u64) {
        (
            self.wire_in.load(Ordering::SeqCst),
            self.wire_out.load(Ordering::SeqCst),
        )
    }

    fn read_frame(&mut self) -> io::Result<(u8, Vec<u8>)> {
        let mut hdr = [0u8; 4];
        self.rd.read_exact(&mut hdr)?;
        let len = u32::from_be_bytes(hdr) as usize;
        if !(1..=MAX_FRAME).contains(&len) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "relay frame length out of range",
            ));
        }
        let mut body = vec![0u8; len];
        self.rd.read_exact(&mut body)?;
        self.wire_in.fetch_add((4 + len) as u64, Ordering::SeqCst);
        let ty = body[0];
        Ok((ty, body.split_off(1)))
    }
}

impl Read for RelayStream {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.pos < self.buf.len() {
            let n = (self.buf.len() - self.pos).min(out.len());
            out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
            self.pos += n;
            return Ok(n);
        }
        loop {
            let (ty, payload) = self.read_frame()?;
            match ty {
                TYPE_DATA => {
                    self.buf = payload;
                    self.pos = 0;
                    let n = self.buf.len().min(out.len());
                    out[..n].copy_from_slice(&self.buf[..n]);
                    self.pos = n;
                    return Ok(n);
                }
                TYPE_PING => continue, // 保活帧：不向上层呈现
                TYPE_CONTROL => {
                    let text = String::from_utf8_lossy(&payload).to_string();
                    if text.starts_with("RelayLimit 13") {
                        // 带宽降速：通知不关闭（docs/08 §3.3 纪律 2），诊断留计数
                        continue;
                    }
                    let code = text
                        .strip_prefix("RelayLimit ")
                        .and_then(|r| r.split(' ').next())
                        .and_then(|c| c.parse::<u8>().ok())
                        .unwrap_or(16);
                    let f = RelayFailure::from_relay_line(code, "relay-limit");
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        f.diag.clone(),
                    ));
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "relay unknown frame type",
                    ))
                }
            }
        }
    }
}

impl Write for RelayStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut wr = self
            .wr
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "relay writer poisoned"))?;
        for chunk in buf.chunks(DATA_MAX) {
            let f = build_frame(TYPE_DATA, chunk);
            wr.write_all(&f)?;
            self.wire_out.fetch_add(f.len() as u64, Ordering::SeqCst);
        }
        wr.flush()?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.wr
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "relay writer poisoned"))?
            .flush()
    }
}

impl Drop for RelayStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // 关闭读半：解除上层可能阻塞的 read，也让保活线程的写失败退出
        let _ = self.rd.shutdown(Shutdown::Both);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::io::BufRead;

    const TOKEN: &str = "vsr2-client-test-token-0123456789";

    /// 极简 VSR2 中继 mock：控制阶段按协议走一遍（不校验 proof——服务端侧
    /// 已有全量鉴权测试），配对同房间两端后双向 `io::copy`（数据阶段对中继
    /// 透明，帧只是不透明字节）。`err_at` 指定在 CHALLENGE 位置改发 ERR 行。
    fn mock_relay(
        err_line: Option<&'static str>,
    ) -> (
        std::net::SocketAddr,
        std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let conns = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c2 = std::sync::Arc::clone(&conns);
        std::thread::spawn(move || {
            let mut rooms: std::collections::HashMap<String, TcpStream> =
                std::collections::HashMap::new();
            for stream in l.incoming().flatten() {
                let mut s = stream;
                let mut r = std::io::BufReader::new(s.try_clone().unwrap());
                let mut hello = String::new();
                if r.read_line(&mut hello).is_err() {
                    continue;
                }
                let p: Vec<&str> = hello.trim_end().split(' ').collect();
                if p.len() != 6 || p[0] != "VSR2" || p[1] != "HELLO" || p[2] != "2" {
                    continue;
                }
                let room = p[3].to_string();
                if let Some(err) = err_line {
                    let _ = s.write_all(format!("{err}\n").as_bytes());
                    continue;
                }
                let _ =
                    s.write_all(b"VSR2 CHALLENGE aabbccdd00112233aabbccdd00112233 585021 5000\n");
                let mut auth = String::new();
                if r.read_line(&mut auth).is_err() {
                    continue;
                }
                // 客户端侧自证：AUTH 行必须存在且不含 token 原值
                assert!(auth.starts_with("VSR2 AUTH "));
                assert!(!auth.contains(TOKEN));
                let _ =
                    s.write_all(b"VSR2 OK sess0000000000000000000000000000 0007 127.0.0.1:47472\n");
                c2.fetch_add(1, Ordering::SeqCst);
                match rooms.remove(&room) {
                    None => {
                        rooms.insert(room, s);
                    }
                    Some(first) => {
                        let mut first = first;
                        std::thread::spawn(move || {
                            let mut second = s;
                            let mut f1 = first.try_clone().unwrap();
                            let mut s2 = second.try_clone().unwrap();
                            let up = std::thread::spawn(move || {
                                let _ = std::io::copy(&mut first, &mut s2);
                            });
                            let _ = std::io::copy(&mut second, &mut f1);
                            let _ = up.join();
                        });
                    }
                }
            }
        });
        (addr, conns)
    }

    /// 接入握手 + 帧化透明往返：大载荷自动分片，对端原样回显。
    #[test]
    fn handshake_and_framed_roundtrip() {
        let (addr, _c) = mock_relay(None);
        let mut a = connect(&addr.to_string(), "room-x", TOKEN).unwrap();
        let mut b = connect(&addr.to_string(), "room-x", TOKEN).unwrap();
        let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let expect = data.clone();
        let echo = std::thread::spawn(move || {
            let mut got = vec![0u8; expect.len()];
            b.read_exact(&mut got).unwrap();
            b.write_all(&got).unwrap();
        });
        a.write_all(&data).unwrap();
        a.flush().unwrap();
        let mut back = vec![0u8; data.len()];
        a.read_exact(&mut back).unwrap();
        echo.join().unwrap();
        assert_eq!(back, data, "帧适配必须透明承载任意大 payload");
        let (i, o) = a.wire_counters();
        assert!(o >= data.len() as u64, "线上字节必须按帧计（含帧头）");
        assert!(i >= data.len() as u64);
        // P8-4：接入后观测端点必须可从流上取得（上层据此决定是否打洞）
        let obs = a.observe_addrs();
        assert_eq!(obs.len(), 1, "观测端点必须被解析：{obs:?}");
        assert_eq!(obs[0].port(), 47472);
    }

    /// 中继 ERR 行 → 结构化失败：R 码映射为本机码 + 降级原因 + 可读诊断。
    #[test]
    fn relay_err_maps_to_local_code() {
        let (addr, _c) = mock_relay(Some("VSR2 ERR 5 auth-failed"));
        let f = connect(&addr.to_string(), "room-e", TOKEN).unwrap_err();
        assert_eq!(f.relay_code, 5);
        assert_eq!(f.local_code, 3);
        assert_eq!(f.degrade_reason, Some("relay_unavailable"));
        assert!(f.diag.contains("auth-failed"));
        let f = connect(&addr.to_string(), "room-e", TOKEN).unwrap_err();
        assert_eq!(f.reason, "auth-failed");
    }

    /// RelayLimit 控制帧 → 读侧 ConnectionAborted + 可读诊断（R13 除外）。
    #[test]
    fn relay_limit_closes_with_readable_error() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            let mut s = s;
            let mut r = std::io::BufReader::new(s.try_clone().unwrap());
            let mut hello = String::new();
            r.read_line(&mut hello).unwrap();
            let _ = s.write_all(b"VSR2 CHALLENGE 00112233445566770011223344556677 585021 5000\n");
            let mut auth = String::new();
            r.read_line(&mut auth).unwrap();
            let _ = s.write_all(b"VSR2 OK sess0000000000000000000000000000 0007\n");
            // 中继下发 RelayLimit R12（字节上限）
            let _ = s.write_all(&build_frame(
                TYPE_CONTROL,
                b"RelayLimit 12 limit-session-bytes",
            ));
            let _ = s.flush();
        });
        let mut st = connect(&addr.to_string(), "room-l", TOKEN).unwrap();
        let mut buf = [0u8; 8];
        let err = st.read(&mut buf).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::ConnectionAborted);
        assert!(
            err.to_string().contains("relay R12"),
            "诊断须含 R 码语义: {err}"
        );
    }

    /// 服务端 PING 帧被消费，不打断数据流。
    #[test]
    fn server_ping_is_consumed() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            let mut s = s;
            let mut r = std::io::BufReader::new(s.try_clone().unwrap());
            let mut hello = String::new();
            r.read_line(&mut hello).unwrap();
            let _ = s.write_all(b"VSR2 CHALLENGE 00112233445566770011223344556677 585021 5000\n");
            let mut auth = String::new();
            r.read_line(&mut auth).unwrap();
            let _ = s.write_all(b"VSR2 OK sess0000000000000000000000000000 0007\n");
            let _ = s.write_all(&build_frame(TYPE_PING, &[]));
            let _ = s.write_all(&build_frame(TYPE_DATA, b"hello"));
            let _ = s.flush();
        });
        let mut st = connect(&addr.to_string(), "room-p", TOKEN).unwrap();
        let mut buf = [0u8; 5];
        st.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"hello");
    }

    /// 本地参数拦截：坏房间名 / 短 token → 7，不发起连接。
    #[test]
    fn local_arg_validation() {
        let (addr, _c) = mock_relay(None);
        let f = connect(&addr.to_string(), "bad room!", TOKEN).unwrap_err();
        assert_eq!(f.local_code, 7);
        let f = connect(&addr.to_string(), "room-ok", "short").unwrap_err();
        assert_eq!(f.local_code, 7);
        assert!(f.diag.contains("128 位"));
    }

    /// P8-4：OK 行捎带的观测端点必须被解析出来（含 `-` 与缺省两种“未启用”形态）。
    /// 这条测试是本字段解析的唯一防线——曾因取错字段序（拿 `caps` 当 hint）
    /// 导致打洞在真机上从未被尝试（冒烟抓出）。
    #[test]
    fn ok_line_observe_hint_is_parsed() {
        // 真实形态：VSR2 OK <sid> <caps> <hint>
        let line = "VSR2 OK sess0000000000000000000000000000 0007 10.0.0.5:47472,10.0.0.5:47473";
        let observe = parse_observe_hint(line);
        assert_eq!(observe.len(), 2, "两个观测目标都要解析出来：{observe:?}");
        assert_eq!(observe[0].port(), 47472);
        assert_eq!(observe[1].port(), 47473);
        // 未启用：`-` 或缺省 → 空（上层据此不做打洞尝试）
        assert!(parse_observe_hint("VSR2 OK sess0000000000000000000000000000 0007 -").is_empty());
        assert!(parse_observe_hint("VSR2 OK sess0000000000000000000000000000 0007").is_empty());
        // 旧端形态（无 caps 也无 hint）→ 空，且不得把 caps 误当 hint
        assert!(parse_observe_hint("VSR2 OK sess0000000000000000000000000000").is_empty());
    }

    /// PING 帧形状（保活不计字节上限的前提是中继能识别它）。
    #[test]
    fn ping_frame_shape() {
        assert_eq!(build_frame(TYPE_PING, &[]), vec![0, 0, 0, 1, TYPE_PING]);
    }
}
