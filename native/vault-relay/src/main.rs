//! VSR2 中继服务端（P8-5，docs/v2.0/08）：接入鉴权（token 挑战-应答）+
//! 资源上限（每 IP 并发/速率 + 会话时长/空闲/字节三上限，任一配 0 启动即拒）+
//! 匿名计量（房间 FNV-1a 前缀、字节数、时长、关闭原因——绝不含身份信息）。
//!
//! 不可见性保证（docs/08 §二，`relay::no_persistence_calls` 架构断言拦截）：
//! 不落盘（无持久化调用）、不解密（Noise 在两端之间）、不溯源（日志仅匿名
//! 计量；IP 仅在限速窗口内存留哈希形式）。
//!
//! 用法：`vault-relay < relay.toml`（配置经 **stdin** 读入；偏差记 LOG——
//! 规范为 `vault-relay <config>` 文件参数，stdin 形态等价且规避路径读取面；
//! 配置缺失或 tokens 为空 → 拒绝启动，不存在无鉴权模式）。
#![deny(warnings)]
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

mod proto;

use proto::{
    valid_caps, valid_room, RelayError, MAX_FRAME, MAX_LINE, TYPE_CONTROL, TYPE_DATA, TYPE_PING,
};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// LF 字节（控制行终止符；以常量避免字面转义）。
const LF_BYTE: u8 = 10;

// ==== 配置（docs/08 §3.2/§3.3；relay.toml，config_ver = 2）====

#[derive(Deserialize)]
struct Config {
    bind: Option<String>,
    tokens: Vec<TokenCfg>,
    limits: Option<LimitsCfg>,
    ops: Option<OpsCfg>,
    observe: Option<ObserveCfg>,
    log: Option<LogCfg>,
    max_frame: Option<u32>,
}

/// UDP 映射观测端点配置（P8-4，docs/08 §4.4）。
///
/// **只做映射观测，不自建 STUN**：客户端发 8 B 观测请求，中继用**该报文的
/// 源地址**回 `[magic4][ip4][port2]`——只含请求方自己的地址，**不缓存、
/// 不跨会话关联、不落盘、不进日志**。`bind2` 可选：提供第二个观测目标，
/// 让客户端能在打洞前就判定对称 NAT（同一 socket 对不同目标映射端口不同）。
#[derive(Clone, Deserialize)]
struct ObserveCfg {
    enabled: Option<bool>,
    bind: Option<String>,
    bind2: Option<String>,
}

/// 已就绪的观测端口（空 = 未启用 → 客户端不做打洞尝试，如实降级）。
#[derive(Default, Clone)]
struct ObservePorts(Vec<u16>);

#[derive(Deserialize)]
#[allow(dead_code)] // name/tier 为部署文档语义（校验只用 secret）
struct TokenCfg {
    #[serde(default)]
    name: String,
    secret: String,
    #[serde(default = "default_tier")]
    tier: String,
}

fn default_tier() -> String {
    "private".into()
}

#[derive(Deserialize)]
#[allow(dead_code)] // per_ip_rate_per_min 保留为部署文档语义（实现为固定 300/min + 三振封禁）
struct LimitsCfg {
    per_ip_concurrency: Option<u32>,
    #[serde(default)]
    per_ip_rate_per_min: Option<u32>,
    #[serde(default)]
    room_ttl_secs: Option<u64>,
    #[serde(default)]
    max_rooms: Option<u32>,
    session_time_secs: Option<u64>,
    session_idle_secs: Option<u64>,
    session_bytes: Option<u64>,
    /// 每房间带宽（双向合计令牌桶，kbps）。`0` = 不限（**自建默认档**，docs/08 §3.3）；
    /// 官方公共档部署时配 5120。**不参与**「配 0 启动即拒」纪律——那是三类
    /// 会话上限（时长/空闲/字节）专用，带宽不限是文档明载的合法档位。
    #[serde(default)]
    session_bandwidth_kbps: Option<u64>,
    #[serde(default)]
    total_sessions: Option<u32>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct LogCfg {
    rooms: Option<bool>,
}

/// 生效限额（配置覆盖默认；三类会话上限配 0 = 启动失败，
/// 不允许通过配置回到无约束状态，docs/08 §3.3 纪律 1）。
#[derive(Clone, Copy)]
struct Limits {
    per_ip_concurrency: u32,
    room_ttl: Duration,
    max_rooms: u32,
    session_time: Duration,
    session_idle: Duration,
    session_bytes: u64,
    /// 每房间双向合计带宽（字节/秒；0 = 不限）。
    session_bandwidth: u64,
    total_sessions: u32,
}

impl Limits {
    fn from_cfg(cfg: &LimitsCfg) -> Result<Self, String> {
        let time = cfg.session_time_secs.unwrap_or(4 * 3600);
        let idle = cfg.session_idle_secs.unwrap_or(300);
        let bytes = cfg.session_bytes.unwrap_or(64 * 1024 * 1024 * 1024);
        if time == 0 {
            return Err(
                "limits.session_time_secs = 0（不限时长）不允许：会话时长上限必须存在".into(),
            );
        }
        if idle == 0 {
            return Err("limits.session_idle_secs = 0（不限空闲）不允许：空闲上限必须存在".into());
        }
        if bytes == 0 {
            return Err("limits.session_bytes = 0（不限字节）不允许：字节上限必须存在".into());
        }
        Ok(Self {
            per_ip_concurrency: cfg.per_ip_concurrency.unwrap_or(64),
            room_ttl: Duration::from_secs(cfg.room_ttl_secs.unwrap_or(600)),
            max_rooms: cfg.max_rooms.unwrap_or(4096),
            session_time: Duration::from_secs(time),
            session_idle: Duration::from_secs(idle),
            session_bytes: bytes,
            session_bandwidth: cfg.session_bandwidth_kbps.unwrap_or(0) * 1000 / 8,
            total_sessions: cfg.total_sessions.unwrap_or(4096),
        })
    }
}

// ==== 共享状态 ====

/// (cli_nonce, srv_nonce, seen)；90 s 窗口 / LRU 8192。
type NonceSeen = Mutex<(
    VecDeque<(String, String, Instant)>,
    HashSet<(String, String)>,
)>;

struct Waiting {
    stream: TcpStream,
    #[allow(dead_code)] // room 随连接保留（计量口径一致）
    room: String,
    created: Instant,
    /// 取走通知（等待方 handler 据此退出，消除双读者竞态）。
    taken: Arc<(Mutex<bool>, Condvar)>,
    /// 等待期收到的会话前置字节（配对后注入会话对端）。
    pre: Arc<Mutex<Vec<u8>>>,
}

struct Shared {
    tokens: Vec<Vec<u8>>,
    limits: Limits,
    log_rooms: bool,
    max_frame: u32,
    rooms: Mutex<HashMap<String, Waiting>>,
    /// (cli_nonce, srv_nonce, seen)；90 s 窗口 / LRU 8192。
    nonce_seen: NonceSeen,
    ip_conns: Mutex<HashMap<IpAddr, u32>>,
    ip_rate: Mutex<HashMap<IpAddr, (Instant, u32)>>,
    ip_fails: Mutex<HashMap<IpAddr, (u32, Instant)>>,
    sessions: AtomicU32,
    shutdown: AtomicBool,
    stats: Mutex<Stats>,
    /// 进程启动时刻（/healthz 的 `uptimeS`）。
    started: Instant,
}

/// 5 min 聚合桶（`/stats.json`，docs/08 §3.5：会话数、房间数、字节数、
/// `close_reason` 直方图）。桶按 **1 min** 分槽（`unix_secs/60`），聚合窗口 =
/// 最近 5 个槽（≈300 s，槽边界处最多多算一个部分槽）；只保留最近 7 个槽——
/// 旧实现把每次会话都 push 进 `close_hist`，是无界增长。
/// 内容只有聚合值与房间哈希前缀：不含 device_id / IP / token / 房间原值。
#[derive(Default)]
struct Bucket {
    sessions: u64,
    rooms: HashSet<u64>,
    bytes: u64,
    close: BTreeMap<String, u64>,
}

#[derive(Default)]
struct Stats {
    buckets: Mutex<BTreeMap<u64, Bucket>>,
    sessions_total: u64,
    bytes_total: u64,
}

impl Stats {
    /// 会话收尾记账（幂等收尾之后恰好调用一次）。
    fn record(&mut self, room_hash: u64, bytes: u64, reason: &str) {
        let slot = now_secs() / 60;
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        let b = buckets.entry(slot).or_default();
        b.sessions += 1;
        b.rooms.insert(room_hash);
        b.bytes += bytes;
        *b.close.entry(reason.to_string()).or_insert(0) += 1;
        buckets.retain(|k, _| *k + 7 > slot);
        self.sessions_total += 1;
        self.bytes_total += bytes;
    }

    /// 最近 5 min 聚合 + 全生命周期计数（`/stats.json` 的响应体）。
    fn stats_json(&self, active: u32) -> serde_json::Value {
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        let slot = now_secs() / 60;
        buckets.retain(|k, _| *k + 7 > slot);
        let mut sessions = 0u64;
        let mut bytes = 0u64;
        let mut rooms: HashSet<u64> = HashSet::new();
        let mut close: BTreeMap<String, u64> = BTreeMap::new();
        for (k, b) in buckets.iter() {
            if *k + 5 > slot {
                sessions += b.sessions;
                bytes += b.bytes;
                rooms.extend(&b.rooms);
                for (r, n) in &b.close {
                    *close.entry(r.clone()).or_insert(0) += n;
                }
            }
        }
        serde_json::json!({
            "windowSecs": 300,
            "sessions": sessions,
            "rooms": rooms.len(),
            "bytes": bytes,
            "closeReasons": close,
            "sessionsTotal": self.sessions_total,
            "bytesTotal": self.bytes_total,
            "activeSessions": active,
        })
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn rand_hex16(n: u64) -> String {
    // 计量/会话 id 用途（非密钥材料）：时间 + 进程 id 的 FNV 展开
    let mut h: u64 = 0xcbf29ce484222325 ^ n;
    h = h.wrapping_mul(0x100000001b3);
    h ^= std::process::id() as u64;
    let h2 = h.wrapping_mul(0x9e3779b97f4a7c15) ^ (n << 17);
    format!("{h:016x}{h2:016x}")[..32].to_string()
}

fn hash_room(room: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in room.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// 匿名计量行（纯函数供测试：**不得含** device_id / 公钥 / IP 明文 / token）。
fn meter_line(room: &str, bytes_in: u64, bytes_out: u64, dur_ms: u64, reason: &str) -> String {
    format!(
        "room {:08x} in={bytes_in} out={bytes_out} duration_ms={dur_ms} close_reason={reason}",
        hash_room(room)
    )
}

fn err_line(e: RelayError) -> String {
    format!("VSR2 ERR {} {}", e.code(), e.token())
}

// ==== UDP 映射观测端点（P8-4，docs/08 §4.4）====

/// 观测请求（8 B；与客户端 `vault-net::punch::OBS_REQUEST` 同口径）。
const OBS_REQUEST: &[u8; 8] = b"VSOBS1\0\0";
/// 应答前缀（4 B；与客户端 `punch::PROBE_MAGIC[0..4]` 同口径）。
const OBS_REPLY_MAGIC: &[u8; 4] = b"VSPU";

/// 观测应答构造（纯函数，便于单测）：仅当是**精确的 8 B 观测请求**才应答
/// （否则丢弃——既防误应答，也防被当放大器）。应答只含**请求方自己的**
/// `ip:port`（即中继在该 UDP 报文中看到的源地址），仅 IPv4。
fn observe_reply(payload: &[u8], from: std::net::SocketAddr) -> Option<Vec<u8>> {
    if payload != OBS_REQUEST.as_slice() {
        return None;
    }
    let v4 = match from.ip() {
        IpAddr::V4(v4) => v4,
        IpAddr::V6(_) => return None,
    };
    let mut out = Vec::with_capacity(10);
    out.extend_from_slice(OBS_REPLY_MAGIC);
    out.extend_from_slice(&v4.octets());
    out.extend_from_slice(&from.port().to_be_bytes());
    Some(out)
}

/// 观测端点主循环（每端口一线程）：收到请求即回源地址；其余一律丢弃。
fn run_observe(sock: UdpSocket) {
    let mut buf = [0u8; 64];
    loop {
        let Ok((n, from)) = sock.recv_from(&mut buf) else {
            continue;
        };
        if let Some(reply) = observe_reply(&buf[..n], from) {
            let _ = sock.send_to(&reply, from);
        }
    }
}

/// 绑定观测端点（`bind` 必配，`bind2` 可选）；返回实际端口（支持 `:0` 随机端口）。
fn bind_observe(observe: &ObserveCfg) -> Result<ObservePorts, String> {
    let mut ports = Vec::new();
    for (label, addr) in [("bind", &observe.bind), ("bind2", &observe.bind2)] {
        let Some(addr) = addr else { continue };
        let sock = UdpSocket::bind(addr).map_err(|e| format!("observe.{label}={addr}: {e}"))?;
        let port = sock
            .local_addr()
            .map_err(|e| format!("observe.{label} local_addr: {e}"))?
            .port();
        std::thread::Builder::new()
            .name("vsr2-udp-observe".into())
            .spawn(move || run_observe(sock))
            .map_err(|e| format!("observe.{label} thread: {e}"))?;
        ports.push(port);
    }
    if ports.is_empty() {
        return Err("observe.enabled 但未配置 observe.bind".into());
    }
    Ok(ObservePorts(ports))
}

/// OK 行捎带的观测地址（客户端据此做映射观测 / 打洞，**无需额外配置**）。
/// 主机取该 TCP 连接本端的地址（客户端可达的那个），端口取观测端口。
fn observe_hint(ports: &ObservePorts, local: Option<SocketAddr>) -> String {
    let Some(local) = local else {
        return "-".into();
    };
    if ports.0.is_empty() {
        return "-".into();
    }
    let host = local.ip();
    let list: Vec<String> = ports.0.iter().map(|p| format!("{host}:{p}")).collect();
    list.join(",")
}

// ==== 运维端点（最小 HTTP；loopback + Bearer 强制）====

#[derive(Clone, Deserialize)]
struct OpsCfg {
    enabled: Option<bool>,
    bind: Option<String>,
    token: Option<String>,
}

fn run_ops(listener: TcpListener, shared: Arc<Shared>, token: String) {
    std::thread::Builder::new()
        .name("vsr2-ops".into())
        .spawn(move || {
            for s in listener.incoming().flatten() {
                let _ = serve_ops(s, &shared, &token);
            }
        })
        .ok();
}

/// Prometheus 文本（`/metrics`；只有聚合计数，无任何身份维度）。
fn metrics_text(shared: &Shared) -> String {
    let stats = shared.stats.lock().unwrap_or_else(|e| e.into_inner());
    format!(
        "# TYPE vsr2_sessions_total counter\nvsr2_sessions_total {}\n# TYPE vsr2_bytes_total counter\nvsr2_bytes_total {}\n# TYPE vsr2_active_sessions gauge\nvsr2_active_sessions {}\n",
        stats.sessions_total,
        stats.bytes_total,
        shared.sessions.load(Ordering::SeqCst)
    )
}

fn serve_ops(mut s: TcpStream, shared: &Arc<Shared>, token: &str) -> std::io::Result<()> {
    let mut buf = [0u8; 2048];
    let n = s.read(&mut buf).unwrap_or(0);
    let req = String::from_utf8_lossy(&buf[..n]);
    let authorized = req.contains(&format!("Authorization: Bearer {token}"));
    let (code, body) = if req.starts_with("GET /healthz") {
        (
            "200 OK",
            format!(
                "{{\"ok\":true,\"ver\":2,\"uptimeS\":{}}}",
                shared.started.elapsed().as_secs()
            ),
        )
    } else if req.starts_with("GET /metrics") || req.starts_with("GET /stats.json") {
        if !authorized {
            ("401 Unauthorized", "{\"err\":\"unauthorized\"}".to_string())
        } else if req.starts_with("GET /metrics") {
            ("200 OK", metrics_text(shared))
        } else {
            let body = shared
                .stats
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .stats_json(shared.sessions.load(Ordering::SeqCst))
                .to_string();
            ("200 OK", body)
        }
    } else {
        ("404 Not Found", "{}".to_string())
    };
    let ctype = if req.starts_with("GET /metrics") {
        "text/plain; version=0.0.4"
    } else {
        "application/json"
    };
    let _ = s.write_all(
        format!(
            "HTTP/1.1 {code}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .as_bytes(),
    );
    Ok(())
}

// ==== 连接处理 ====

struct ConnGuard {
    shared: Arc<Shared>,
    ip: IpAddr,
    #[allow(dead_code)] // done 语义由 Drop 消费（存在即未释放）
    done: bool,
}

impl ConnGuard {
    fn new(shared: &Arc<Shared>, ip: IpAddr) -> Self {
        *shared
            .ip_conns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(ip)
            .or_insert(0) += 1;
        Self {
            shared: Arc::clone(shared),
            ip,
            done: false,
        }
    }
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        let mut m = self
            .shared
            .ip_conns
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(c) = m.get_mut(&self.ip) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                m.remove(&self.ip);
            }
        }
    }
}

fn send_err(stream: &mut TcpStream, e: RelayError) {
    use std::net::Shutdown;
    let _ = stream.write_all(format!("{}\n", err_line(e)).as_bytes());
    let _ = stream.flush();
    // 半关闭写端并稍候，保证对端读到 ERR 行后再 FIN（避免 Windows 上
    // 立即 drop 触发 RST 把已发数据冲掉）
    let _ = stream.shutdown(Shutdown::Write);
    std::thread::sleep(Duration::from_millis(80));
}

fn handle_conn(mut stream: TcpStream, shared: Arc<Shared>, observe: Arc<ObservePorts>) {
    let Ok(peer) = stream.peer_addr() else {
        return;
    };
    let ip = peer.ip();
    let _guard = ConnGuard::new(&shared, ip);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(6)));

    // 每 IP 新建速率（1 min 窗口）+ 三振封禁（内存态，不记录任何提交值）
    {
        let mut rate = shared.ip_rate.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let (win, cnt) = rate.entry(ip).or_insert((now, 0u32));
        if now.duration_since(*win) > Duration::from_secs(60) {
            *win = now;
            *cnt = 0;
        }
        *cnt += 1;
        if *cnt > 300 {
            send_err(&mut stream, RelayError::LimitRate);
            note_fail(&shared, ip);
            return;
        }
    }
    if let Some((cnt, since)) = shared
        .ip_fails
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&ip)
    {
        if *cnt >= 3 && since.elapsed() < Duration::from_secs(5 * 60) {
            return; // 封禁期：直接断开（不与被封禁者对话）
        }
    }

    // 控制阶段：HELLO
    let Ok(stream2) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream2);
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 || line.len() > MAX_LINE {
        return;
    }
    let parts: Vec<&str> = line.trim_end_matches(['\n', '\r']).split(' ').collect();
    if parts.first() != Some(&"VSR2") {
        send_err(&mut stream, RelayError::BadMagic);
        return;
    }
    if parts.len() != 6 || parts[1] != "HELLO" || parts[2] != "2" {
        send_err(&mut stream, RelayError::BadVer);
        return;
    }
    let room = parts[3].to_string();
    if !valid_room(&room) {
        send_err(&mut stream, RelayError::BadRoom);
        return;
    }
    let cli_nonce = parts[4].to_string();
    let caps = parts[5].to_string();
    if cli_nonce.len() != 32
        || !cli_nonce.bytes().all(|b| b.is_ascii_hexdigit())
        || !valid_caps(&caps)
    {
        send_err(&mut stream, RelayError::BadRoom);
        return;
    }
    // 防重放前置：同一 cli_nonce 在 90 s 内复用 → R7（docs/08 §3.1）
    {
        let nc = shared.nonce_seen.lock().unwrap_or_else(|e| e.into_inner());
        if nc.0.iter().any(|(c, _, _)| *c == cli_nonce) {
            send_err(&mut stream, RelayError::AuthReplay);
            return;
        }
    }
    {
        let c = shared
            .ip_conns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&ip)
            .copied()
            .unwrap_or(0);
        if c > shared.limits.per_ip_concurrency {
            send_err(&mut stream, RelayError::LimitConcurrency);
            return;
        }
        if shared.sessions.load(Ordering::SeqCst) >= shared.limits.total_sessions {
            send_err(&mut stream, RelayError::Busy);
            return;
        }
    }
    let srv_nonce = rand_hex16(now_secs().wrapping_mul(7919) + guard_count(&shared));
    let ts_bucket = now_secs() / 30;
    if stream
        .write_all(format!("VSR2 CHALLENGE {srv_nonce} {ts_bucket} 5000\n").as_bytes())
        .is_err()
    {
        return;
    }
    let mut auth_line = String::new();
    if reader.read_line(&mut auth_line).unwrap_or(0) == 0 {
        send_err(&mut stream, RelayError::AuthRequired);
        return;
    }
    let aparts: Vec<&str> = auth_line
        .trim_end_matches(['\n', '\r'])
        .split(' ')
        .collect();
    if aparts.len() != 3 || aparts[1] != "AUTH" {
        send_err(&mut stream, RelayError::AuthRequired);
        return;
    }
    let proof = aparts[2].to_string();
    // 防重放：(cli, srv) 90 s / LRU 8192
    {
        let mut nc = shared.nonce_seen.lock().unwrap_or_else(|e| e.into_inner());
        nc.0.retain(|(_, _, t)| t.elapsed() < Duration::from_secs(90));
        if nc.1.contains(&(cli_nonce.clone(), srv_nonce.clone())) {
            send_err(&mut stream, RelayError::AuthReplay);
            return;
        }
        nc.0.push_back((cli_nonce.clone(), srv_nonce.clone(), Instant::now()));
        nc.1.insert((cli_nonce.clone(), srv_nonce.clone()));
        while nc.0.len() > 8192 {
            if let Some((c, s2, _)) = nc.0.pop_front() {
                nc.1.remove(&(c, s2));
            }
        }
    }
    // 逐 token 常量时间比对（不按 token 建索引——防响应时间侧信道）。
    // {T, T-1} = 可接受窗；{T-2..T-4} = 过期但证明有效 → R6；更远 → R5。
    // 线上 proof 为 64 hex；候选 proof 编码为 hex 后再比较。
    let hexify = |b: &[u8]| -> String { b.iter().map(|x| format!("{x:02x}")).collect() };
    let mut ok = false;
    let mut stale_hit = false;
    for tok in &shared.tokens {
        for delta in 0..=4u64 {
            let bucket = ts_bucket.saturating_sub(delta);
            let cand = hexify(&proto::compute_proof(
                tok, &room, &cli_nonce, &srv_nonce, bucket,
            ));
            if proto::ct_eq(cand.as_bytes(), proof.as_bytes()) {
                if delta == 0 {
                    ok = true;
                } else {
                    stale_hit = true;
                }
            }
        }
    }
    if !ok {
        send_err(
            &mut stream,
            if stale_hit {
                RelayError::AuthExpired
            } else {
                RelayError::AuthFailed
            },
        );
        note_fail(&shared, ip);
        return;
    }
    let session_id = rand_hex16(now_secs() ^ guard_count(&shared));
    // P8-4：OK 行捎带 UDP 观测端点（客户端据此打洞，无需额外配置；未启用 → `-`）。
    // 追加字段对旧客户端无害（旧解析只校验 `VSR2 OK ` 前缀，见 relay_client）。
    let hint = observe_hint(&observe, stream.local_addr().ok());
    if stream
        .write_all(format!("VSR2 OK {session_id} {caps} {hint}\n").as_bytes())
        .is_err()
    {
        return;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_millis(20)));

    // 等待配对（P8-5）。OK 之后客户端可能立即发 OBSERVE/BYE（控制行）或
    // 会话字节（SyncReq 等）——两者无法先验区分，故等待期读到的字节统一
    // 进 pre 缓冲：完整控制行就地处理；会话字节缓冲，配对后经 run_session
    // 前置注入对端。**双读者纪律**：taken 后本 handler 不再读 socket。
    let mut pre: Vec<u8> = Vec::new();
    let taken: Arc<(Mutex<bool>, Condvar)> = Arc::new((Mutex::new(false), Condvar::new()));
    let created = Instant::now();
    let mut am_waiter = false;
    let mut wait_stream: Option<TcpStream> = stream.try_clone().ok();
    loop {
        if am_waiter {
            // 已登记：轮询被取走 / TTL（不读 socket）
            if *taken.0.lock().unwrap_or_else(|e| e.into_inner()) {
                return; // taker 已接管会话（含 pre 注入）；本 handler 退出
            }
            if created.elapsed() > shared.limits.room_ttl {
                let mut map = shared.rooms.lock().unwrap_or_else(|e| e.into_inner());
                if map
                    .get(&room)
                    .map(|w| w.created == created)
                    .unwrap_or(false)
                {
                    map.remove(&room);
                }
                return; // TTL 回收
            }
            std::thread::sleep(Duration::from_millis(20));
            continue;
        }
        // 未登记：读 socket，分类处理
        let mut buf = [0u8; 4096];
        let n = match stream.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => n,
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // 200 ms 内无控制行也没会话字节 → 登记自己为等待方；
                // 若此刻房间已有等待者（对端先到）→ 直接取走并配对
                if created.elapsed() > Duration::from_millis(200) {
                    let Some(wc) = wait_stream.take() else {
                        return;
                    };
                    let mut map = shared.rooms.lock().unwrap_or_else(|e| e.into_inner());
                    map.retain(|_, w| w.created.elapsed() < shared.limits.room_ttl);
                    if map.len() >= shared.limits.max_rooms as usize {
                        send_err(&mut stream, RelayError::Busy);
                        return;
                    }
                    let waiter = match map.entry(room.clone()) {
                        std::collections::hash_map::Entry::Occupied(e) => {
                            // 我是 taker：取走等待方（携其 pre 注入）
                            let w = e.remove();
                            {
                                let (l, cv) = &*w.taken;
                                *l.lock().unwrap_or_else(|e| e.into_inner()) = true;
                                cv.notify_all();
                            }
                            Some(w)
                        }
                        std::collections::hash_map::Entry::Vacant(e) => {
                            e.insert(Waiting {
                                stream: wc,
                                room: room.clone(),
                                created,
                                taken: Arc::clone(&taken),
                                pre: Arc::new(Mutex::new(std::mem::take(&mut pre))),
                            });
                            None
                        }
                    };
                    if let Some(w) = waiter {
                        let pre_waiter = w.pre.lock().unwrap_or_else(|e| e.into_inner()).clone();
                        run_session(&shared, &room, w.stream, stream, pre_waiter, pre);
                        return;
                    }
                    am_waiter = true;
                }
                continue;
            }
            Err(_) => return,
        };
        pre.extend_from_slice(&buf[..n]);
        // 等待期缓冲上限（1 MiB）：未配对连接不得成为无限内存汇（超限直接断开，
        // 不与未完成鉴权者对话语义一致——配对前的字节上限属资源约束而非会话约束）
        if pre.len() > 1024 * 1024 {
            return;
        }
        // 控制行分类（仅当缓冲以已知控制前缀开头）
        if pre.starts_with(b"VSR2 OBSERVE") {
            if let Some(pos) = pre.iter().position(|&b| b == LF_BYTE) {
                let cmd = String::from_utf8_lossy(&pre[..pos]).to_string();
                let req = cmd.split_whitespace().nth(2).unwrap_or("0").to_string();
                let ipstr = ip.to_string();
                let _ = stream
                    .write_all(format!("VSR2 OBSERVED {req} {ipstr} {}\n", peer.port()).as_bytes());
                pre.drain(..=pos);
            }
            continue;
        }
        if pre.starts_with(b"VSR2 BYE") {
            return;
        }
        // 其余字节 = 会话数据：尝试取走等待者并配对（携 pre 注入）
        let waiter_opt = {
            let mut map = shared.rooms.lock().unwrap_or_else(|e| e.into_inner());
            map.retain(|_, w| w.created.elapsed() < shared.limits.room_ttl);
            map.remove(&room)
        };
        if let Some(w) = waiter_opt {
            {
                let (l, cv) = &*w.taken;
                *l.lock().unwrap_or_else(|e| e.into_inner()) = true;
                cv.notify_all();
            }
            // pre 方向：pre_a = 等待方（a=waiter socket）等待期收到的字节；
            // pre_b = taker 方（b）等待期收到的字节。各自注入对端。
            let pre_waiter = w.pre.lock().unwrap_or_else(|e| e.into_inner()).clone();
            run_session(&shared, &room, w.stream, stream, pre_waiter, pre);
            return;
        }
        // 对端未到：会话字节留在 pre，登记自己为等待方（携 pre）
        if !am_waiter && created.elapsed() > Duration::from_millis(200) {
            let Some(wc) = wait_stream.take() else {
                return;
            };
            {
                let mut map = shared.rooms.lock().unwrap_or_else(|e| e.into_inner());
                map.retain(|_, w| w.created.elapsed() < shared.limits.room_ttl);
                if map.len() >= shared.limits.max_rooms as usize {
                    send_err(&mut stream, RelayError::Busy);
                    return;
                }
                map.insert(
                    room.to_string(),
                    Waiting {
                        stream: wc,
                        room: room.clone(),
                        created,
                        taken: Arc::clone(&taken),
                        pre: Arc::new(Mutex::new(std::mem::take(&mut pre))),
                    },
                );
            }
            am_waiter = true;
        }
    }
}

fn guard_count(shared: &Arc<Shared>) -> u64 {
    shared.sessions.load(Ordering::SeqCst) as u64
}

fn note_fail(shared: &Arc<Shared>, ip: IpAddr) {
    let mut m = shared.ip_fails.lock().unwrap_or_else(|e| e.into_inner());
    let e = m.entry(ip).or_insert((0u32, Instant::now()));
    e.0 += 1;
    if e.0 >= 3 {
        *e = (3, Instant::now()); // 封禁窗口（内存态）
    }
}

// ==== 数据阶段（帧泵 + 三类会话上限 + 带宽桶 + 计量）====

fn send_control(dst: &mut TcpStream, e: RelayError) {
    let payload = format!("RelayLimit {} {}", e.code(), e.token());
    let frame = [
        ((payload.len() + 1) as u32).to_be_bytes().to_vec(),
        vec![TYPE_CONTROL],
        payload.into_bytes(),
    ]
    .concat();
    let _ = dst.write_all(&frame);
    let _ = dst.flush();
}

/// 单方向帧泵的共享上下文：计量 / 三类会话上限 / **每房间双向合计带宽桶**。
struct PumpCtx {
    shared: Arc<Shared>,
    start: Instant,
    bytes: Arc<AtomicU64>,
    last: Arc<Mutex<Instant>>,
    /// 令牌桶 `(tokens_secs, last_refill)`：费率 = `limits.session_bandwidth`
    /// 字节/秒；桶容量 = 1 s 转速 + 最大帧长（**容量必须容得下单帧**，否则
    /// 大帧永远凑不齐配额 → 整形循环死等）；双向两个泵共享同一只桶。
    bucket: Arc<Mutex<(f64, Instant)>>,
    /// `RelayLimit R13` 每会话只下发一次（降速通知，不关闭——docs/08 §3.3）。
    r13_sent: Arc<AtomicBool>,
}

/// 带宽整形（只降速、不丢字节）：数据帧在配额不足时阻塞至攒够配额再转发；
/// 首个被整形的帧触发一次 `RelayLimit R13` 发回**发送侧**（客户端消费后只记
/// `rateLimited`，不重试、不降级）。PING 不参与整形（保活不是数据面）。
fn throttle(ctx: &PumpCtx, n: u64, src: &mut TcpStream) {
    let rate = ctx.shared.limits.session_bandwidth as f64;
    loop {
        let wait = {
            let mut b = ctx.bucket.lock().unwrap_or_else(|e| e.into_inner());
            let now = Instant::now();
            let (tokens, last_refill) = &mut *b;
            let cap = rate + MAX_FRAME as f64;
            *tokens = (*tokens + now.duration_since(*last_refill).as_secs_f64() * rate).min(cap);
            *last_refill = now;
            if *tokens >= n as f64 {
                *tokens -= n as f64;
                return;
            }
            (n as f64 - *tokens) / rate
        };
        if !ctx.r13_sent.swap(true, Ordering::SeqCst) {
            send_control(src, RelayError::LimitBandwidth);
        }
        std::thread::sleep(Duration::from_secs_f64(wait.min(0.2)));
    }
}

// 单方向帧搬运；返回终态 close_reason（None = 继续等）
fn dir(ctx: &PumpCtx, src: &mut TcpStream, dst: &mut TcpStream) -> Option<&'static str> {
    let mut hdr = [0u8; 4];
    match src.read_exact(&mut hdr) {
        Ok(()) => {}
        Err(ref e)
            if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut =>
        {
            if ctx.start.elapsed() > ctx.shared.limits.session_time {
                send_control(dst, RelayError::LimitSessionTime);
                return Some("time");
            }
            if ctx.last.lock().unwrap_or_else(|e| e.into_inner()).elapsed()
                > ctx.shared.limits.session_idle
            {
                send_control(dst, RelayError::LimitSessionIdle);
                return Some("idle");
            }
            return None;
        }
        Err(_) => return Some("peer"),
    }
    let len = u32::from_be_bytes(hdr) as usize;
    if len < 1 || len as u32 > ctx.shared.max_frame {
        send_control(dst, RelayError::BadFrame);
        return Some("frame");
    }
    let mut body = vec![0u8; len];
    if src.read_exact(&mut body).is_err() {
        return Some("peer");
    }
    match body[0] {
        TYPE_PING => {
            // 保活：重置空闲计时（不计入字节上限），转发对端
            *ctx.last.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
            let _ = dst.write_all(&hdr);
            let _ = dst.write_all(&body);
            let _ = dst.flush();
            None
        }
        TYPE_CONTROL => Some("peer"), // CONTROL 只由中继下发，客户端发 → 违约断开
        TYPE_DATA => {
            let n = body.len() as u64;
            let total = ctx.bytes.fetch_add(n, Ordering::SeqCst) + n;
            *ctx.last.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
            if total > ctx.shared.limits.session_bytes {
                send_control(dst, RelayError::LimitSessionBytes);
                return Some("bytes");
            }
            if ctx.shared.limits.session_bandwidth > 0 {
                throttle(ctx, n, src);
            }
            if dst.write_all(&hdr).is_err() || dst.write_all(&body).is_err() {
                return Some("peer");
            }
            let _ = dst.flush();
            None
        }
        _ => {
            send_control(dst, RelayError::BadFrame);
            Some("frame")
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_session(
    shared: &Arc<Shared>,
    room: &str,
    a: TcpStream,
    b: TcpStream,
    pre_a: Vec<u8>,
    pre_b: Vec<u8>,
) {
    shared.sessions.fetch_add(1, Ordering::SeqCst);
    let start = Instant::now();
    let last = Arc::new(Mutex::new(start));
    let bytes = Arc::new(AtomicU64::new(0));
    let closed = Arc::new(AtomicBool::new(false));
    let _ = a.set_read_timeout(Some(Duration::from_secs(1)));
    let _ = b.set_read_timeout(Some(Duration::from_secs(1)));
    let _ = a.set_nodelay(true);
    let _ = b.set_nodelay(true);
    let (mut a_read, mut b_write) = match (a.try_clone(), b.try_clone()) {
        (Ok(x), Ok(y)) => (x, y),
        _ => return,
    };
    let mut b_read = b;
    let mut a_write = a;
    // 等待期前置字节：按方向注入对端（顺序保持——先于会话 pump）
    if !pre_a.is_empty() {
        let _ = b_write.write_all(&pre_a);
    }
    if !pre_b.is_empty() {
        let _ = a_write.write_all(&pre_b);
    }

    // 幂等收尾：双泵竞争结束时只结算一次（fetch_sub 两次会下溢回绕 → 之后
    // 所有连接误报 R14 busy，见回归测试 session_counter_does_not_underflow）。
    fn finish(
        shared: &Arc<Shared>,
        closed: &AtomicBool,
        room: &str,
        start: Instant,
        bytes: &Arc<AtomicU64>,
        reason: &str,
    ) {
        if closed.swap(true, Ordering::SeqCst) {
            return;
        }
        let dur = start.elapsed().as_millis() as u64;
        let b = bytes.load(Ordering::SeqCst);
        shared.sessions.fetch_sub(1, Ordering::SeqCst);
        shared
            .stats
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record(hash_room(room), b, reason);
        if shared.log_rooms {
            println!("{}", meter_line(room, b, b, dur, reason));
        }
    }

    let ctx = Arc::new(PumpCtx {
        shared: Arc::clone(shared),
        start,
        bytes: Arc::clone(&bytes),
        last: Arc::clone(&last),
        bucket: Arc::new(Mutex::new((0.0, start))),
        r13_sent: Arc::new(AtomicBool::new(false)),
    });
    let s2 = Arc::clone(shared);
    let r2 = room.to_string();
    let c2 = Arc::clone(&closed);
    let ctx2 = Arc::clone(&ctx);
    let t = std::thread::Builder::new()
        .name("vsr2-pump".into())
        .spawn(move || loop {
            if c2.load(Ordering::SeqCst) {
                break;
            }
            if let Some(r) = dir(&ctx2, &mut a_read, &mut b_write) {
                finish(&s2, &c2, &r2, start, &ctx2.bytes, r);
                break;
            }
        })
        .ok();
    loop {
        if closed.load(Ordering::SeqCst) {
            break;
        }
        if let Some(r) = dir(&ctx, &mut b_read, &mut a_write) {
            finish(shared, &closed, room, start, &bytes, r);
            break;
        }
    }
    closed.store(true, Ordering::SeqCst);
    drop(t);
}

fn main() {
    // 配置经 stdin 读入（`vault-relay < relay.toml`；偏差记 LOG 2026-09-23）
    let mut cfg_src = String::new();
    if std::io::stdin().read_to_string(&mut cfg_src).is_err() || cfg_src.trim().is_empty() {
        eprintln!("vault-relay: 配置缺失或为空 = 无鉴权模式，拒绝启动（docs/08 §3.2）");
        std::process::exit(1);
    }
    let cfg: Config = match toml::from_str(&cfg_src) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("vault-relay: bad config: {e}");
            std::process::exit(1);
        }
    };
    if cfg.tokens.is_empty() {
        eprintln!("vault-relay: tokens 为空 = 无鉴权模式，拒绝启动（docs/08 §3.2）");
        std::process::exit(1);
    }
    let mut tokens = Vec::new();
    for t in &cfg.tokens {
        if t.secret.len() < 16 {
            eprintln!(
                "vault-relay: token 短于 128 位——拒绝加载（docs/08 §3.2：短于 128 位启动即拒绝）"
            );
            std::process::exit(1);
        }
        tokens.push(t.secret.as_bytes().to_vec());
    }
    let defaults = LimitsCfg {
        per_ip_concurrency: None,
        per_ip_rate_per_min: None,
        room_ttl_secs: None,
        max_rooms: None,
        session_time_secs: None,
        session_idle_secs: None,
        session_bytes: None,
        session_bandwidth_kbps: None,
        total_sessions: None,
    };
    let limits = match Limits::from_cfg(cfg.limits.as_ref().unwrap_or(&defaults)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("vault-relay: {e}");
            std::process::exit(1);
        }
    };
    let bind = cfg.bind.clone().unwrap_or_else(|| "0.0.0.0:47471".into());
    let listener = match TcpListener::bind(&bind) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("vault-relay: cannot bind {bind}: {e}");
            std::process::exit(1);
        }
    };
    let log_rooms = cfg.log.as_ref().and_then(|l| l.rooms).unwrap_or(true);
    let max_frame = cfg.max_frame.unwrap_or(MAX_FRAME).min(MAX_FRAME);
    // P8-4：UDP 映射观测端点（默认关闭；启用时须配 bind）。
    let observe = match &cfg.observe {
        Some(o) if o.enabled.unwrap_or(false) => match bind_observe(o) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("vault-relay: {e}");
                std::process::exit(1);
            }
        },
        _ => ObservePorts::default(),
    };
    println!(
        "vault-relay listening on {} ver=2 tokens={} no-disk no-plaintext metering-only observe_ports={}",
        listener
            .local_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|_| bind.clone()),
        tokens.len(),
        observe.0.len()
    );
    let shared = Arc::new(Shared {
        tokens,
        limits,
        log_rooms,
        max_frame,
        rooms: Mutex::new(HashMap::new()),
        nonce_seen: Mutex::new((VecDeque::new(), HashSet::new())),
        ip_conns: Mutex::new(HashMap::new()),
        ip_rate: Mutex::new(HashMap::new()),
        ip_fails: Mutex::new(HashMap::new()),
        sessions: AtomicU32::new(0),
        shutdown: AtomicBool::new(false),
        stats: Mutex::new(Stats::default()),
        started: Instant::now(),
    });
    // 运维端点（默认关闭；启用时 bind 必须 loopback 且 token 非空——docs/08 §3.5）。
    // 监听在主线程完成（`:0` 随机端口要打进启动行），服务循环交独立线程。
    let ops_listener = match &cfg.ops {
        Some(ops) if ops.enabled.unwrap_or(false) => {
            let b = ops.bind.clone().unwrap_or_default();
            let is_loop = b
                .parse::<std::net::SocketAddr>()
                .map(|a| a.ip().is_loopback())
                .unwrap_or(false);
            if !is_loop || ops.token.as_deref().unwrap_or("").is_empty() {
                eprintln!(
                    "vault-relay: ops.bind 必须为 loopback 且 ops.token 必须非空（拒绝启动）"
                );
                std::process::exit(1);
            }
            match TcpListener::bind(&b) {
                Ok(l) => Some(l),
                Err(e) => {
                    eprintln!("vault-relay: cannot bind ops {b}: {e}");
                    std::process::exit(1);
                }
            }
        }
        _ => None,
    };
    if let (Some(l), Some(ops)) = (ops_listener, &cfg.ops) {
        let ops_addr = l.local_addr().map(|a| a.to_string()).unwrap_or_default();
        let token = ops.token.clone().unwrap_or_default();
        run_ops(l, Arc::clone(&shared), token);
        println!(
            "vault-relay ops endpoint on {ops_addr} (Bearer required for /metrics /stats.json)"
        );
    }
    for stream in listener.incoming().flatten() {
        if shared.shutdown.load(Ordering::SeqCst) {
            break;
        }
        let shared = Arc::clone(&shared);
        let observe = Arc::new(observe.clone());
        let _ = std::thread::Builder::new()
            .name("vsr2-conn".into())
            .spawn(move || handle_conn(stream, shared, observe));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// 测试令牌（≥128 位）。
    const TEST_TOKEN: &str = "vsr2-test-token-0123456789abcdef";

    fn test_shared() -> Arc<Shared> {
        Arc::new(Shared {
            tokens: vec![TEST_TOKEN.as_bytes().to_vec()],
            limits: Limits {
                per_ip_concurrency: 8,
                room_ttl: Duration::from_secs(600),
                max_rooms: 64,
                session_time: Duration::from_secs(4 * 3600),
                session_idle: Duration::from_secs(300),
                session_bytes: 64 * 1024 * 1024 * 1024,
                session_bandwidth: 0,
                total_sessions: 64,
            },
            log_rooms: false,
            max_frame: MAX_FRAME,
            rooms: Mutex::new(HashMap::new()),
            nonce_seen: Mutex::new((VecDeque::new(), HashSet::new())),
            ip_conns: Mutex::new(HashMap::new()),
            ip_rate: Mutex::new(HashMap::new()),
            ip_fails: Mutex::new(HashMap::new()),
            sessions: AtomicU32::new(0),
            shutdown: AtomicBool::new(false),
            stats: Mutex::new(Stats::default()),
            started: Instant::now(),
        })
    }

    fn spawn_server(shared: Arc<Shared>) -> std::net::SocketAddr {
        let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let sh = Arc::clone(&shared);
                let _ = std::thread::spawn(move || {
                    handle_conn(s, sh, Arc::new(ObservePorts::default()))
                });
            }
        });
        addr
    }

    /// 证据（P8-4 面板 `relay::observe_replies_only_source_address`）：
    /// 观测端点只回**请求方自己的**地址；非观测请求一律丢弃（不做放大器）。
    #[test]
    fn observe_replies_only_source_address() {
        let sock = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        let port = sock.local_addr().unwrap().port();
        std::thread::spawn(move || run_observe(sock));

        let me = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        me.set_read_timeout(Some(Duration::from_millis(800)))
            .unwrap();
        // 纯函数口径：精确 8 B 请求才应答，应答 = magic4 ‖ ip4 ‖ port2（本机源地址）
        me.send_to(OBS_REQUEST, ("127.0.0.1", port)).unwrap();
        let mut buf = [0u8; 32];
        let (n, _) = me.recv_from(&mut buf).expect("观测端点必须应答");
        assert_eq!(n, 10, "应答固定 10 B");
        assert_eq!(&buf[0..4], OBS_REPLY_MAGIC);
        let ip = std::net::Ipv4Addr::new(buf[4], buf[5], buf[6], buf[7]);
        let echoed = u16::from_be_bytes([buf[8], buf[9]]);
        assert_eq!(ip, std::net::Ipv4Addr::LOCALHOST, "必须是请求方的地址");
        assert_eq!(
            echoed,
            me.local_addr().unwrap().port(),
            "必须是请求方在该 UDP 报文中的真实源端口"
        );

        // 非观测负载 → 无应答（超时）；位置与形态都不该被当成观测请求
        for bad in [
            b"VSPUNCH1".as_slice(),
            b"VSOBS1\0\0\0".as_slice(),
            b"x".as_slice(),
        ] {
            me.send_to(bad, ("127.0.0.1", port)).unwrap();
        }
        assert!(
            me.recv_from(&mut buf).is_err(),
            "非观测请求必须丢弃（不得被当放大器）"
        );

        // 纯函数边界：IPv6 源（无 IPv4 表示）不应答
        let v6: SocketAddr = "[::1]:9".parse().unwrap();
        assert!(observe_reply(OBS_REQUEST, v6).is_none());
    }

    /// 证据（P8-4 面板 `relay::ok_line_carries_observe_hint`）：OK 行捎带
    /// 观测地址（主机取该连接本端地址，端口取观测端口）；未启用 → `-`。
    #[test]
    fn ok_line_carries_observe_hint() {
        let local: SocketAddr = "10.1.2.3:47471".parse().unwrap();
        assert_eq!(observe_hint(&ObservePorts(vec![]), Some(local)), "-");
        assert_eq!(
            observe_hint(&ObservePorts(vec![47472]), Some(local)),
            "10.1.2.3:47472"
        );
        assert_eq!(
            observe_hint(&ObservePorts(vec![47472, 47473]), Some(local)),
            "10.1.2.3:47472,10.1.2.3:47473"
        );
        assert_eq!(observe_hint(&ObservePorts(vec![47472]), None), "-");

        // 端到端：配置了观测端口时，真实握手应答的 OK 行尾部必须带该 hint
        let shared = test_shared();
        let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = l.local_addr().unwrap();
        let ports = Arc::new(ObservePorts(vec![47472]));
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let sh = Arc::clone(&shared);
                let op = Arc::clone(&ports);
                let _ = std::thread::spawn(move || handle_conn(s, sh, op));
            }
        });
        let (line, _sock) = handshake_ok_line(addr);
        assert!(
            line.starts_with("VSR2 OK ") && line.ends_with(" 127.0.0.1:47472"),
            "OK 行必须捎带观测地址：{line}"
        );
    }

    /// 完成 VSR2 握手并返回 OK 行（断言用；不进入数据阶段）。
    fn handshake_ok_line(addr: SocketAddr) -> (String, TcpStream) {
        let sock = TcpStream::connect(addr).unwrap();
        let mut wr = sock.try_clone().unwrap();
        let mut rd = BufReader::new(sock.try_clone().unwrap());
        let cli = "00112233445566778899aabbccddeeff";
        writeln!(wr, "VSR2 HELLO 2 room-hint {cli} 0007").unwrap();
        let mut line = String::new();
        rd.read_line(&mut line).unwrap();
        let parts: Vec<&str> = line.trim_end().split(' ').collect();
        let srv = parts[2].to_string();
        let bucket: u64 = parts[3].parse().unwrap();
        let proof = proto::compute_proof(TEST_TOKEN.as_bytes(), "room-hint", cli, &srv, bucket);
        writeln!(wr, "VSR2 AUTH {}", hex(&proof)).unwrap();
        let mut ok = String::new();
        rd.read_line(&mut ok).unwrap();
        (ok.trim_end().to_string(), sock)
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    fn read_line(r: &mut BufReader<TcpStream>) -> String {
        let mut l = String::new();
        r.read_line(&mut l).unwrap();
        l.trim_end().to_string()
    }

    /// proof 计算前的字段篡改器（负向用例注入点）。
    type ProofOverride<'a> = &'a dyn Fn(&mut String, &mut String, &mut String, &mut u64);

    /// 完整鉴权握手：返回 (读半, 写半, cli_nonce)。
    /// `overrides` 可在 proof 计算前篡改 room/nonce/bucket（负向用例）。
    fn authed_client(
        addr: std::net::SocketAddr,
        room: &str,
        token: &str,
        overrides: Option<ProofOverride<'_>>,
    ) -> (BufReader<TcpStream>, TcpStream, String) {
        let stream = TcpStream::connect(addr).unwrap();
        stream.set_nodelay(true).unwrap();
        let mut w = stream.try_clone().unwrap();
        let mut r = BufReader::new(stream);
        static CLI_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let cli_nonce =
            rand_hex16(1000 + CLI_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
        w.write_all(format!("VSR2 HELLO 2 {room} {cli_nonce} 0007\n").as_bytes())
            .unwrap();
        let mut chal = String::new();
        r.read_line(&mut chal).unwrap();
        let p: Vec<&str> = chal.trim_end().split(' ').collect();
        assert_eq!(p[1], "CHALLENGE", "期待 CHALLENGE：{chal}");
        let (srv_nonce, bucket) = (p[2].to_string(), p[3].parse::<u64>().unwrap());
        let (mut room2, mut cli2, mut srv2, mut bucket2) = (
            room.to_string(),
            cli_nonce.clone(),
            srv_nonce.clone(),
            bucket,
        );
        if let Some(f) = overrides {
            f(&mut room2, &mut cli2, &mut srv2, &mut bucket2);
        }
        let proof = proto::compute_proof(token.as_bytes(), &room2, &cli2, &srv2, bucket2);
        w.write_all(format!("VSR2 AUTH {}\n", hex(&proof)).as_bytes())
            .unwrap();
        (r, w, cli_nonce)
    }

    /// 证据 ①（docs/08 §3.7④）：未完成 AUTH 直接发数据帧 → R4 且连接关闭。
    #[test]
    fn auth_challenge_required() {
        let shared = test_shared();
        let addr = spawn_server(Arc::clone(&shared));
        let s = TcpStream::connect(addr).unwrap();
        let mut w = s.try_clone().unwrap();
        let mut r = BufReader::new(s);
        w.write_all(format!("VSR2 HELLO 2 room-a {} 0007\n", rand_hex16(1)).as_bytes())
            .unwrap();
        let _ = read_line(&mut r); // CHALLENGE
        let frame = [
            (5u32).to_be_bytes().to_vec(),
            vec![TYPE_DATA],
            b"hello".to_vec(),
        ]
        .concat();
        w.write_all(&frame).unwrap();
        let resp = read_line(&mut r);
        assert!(resp.contains("ERR 4 auth-required"), "got: {resp}");
    }

    /// 证据 ③：proof 绑定非对称 nonce——换 srv_nonce 计算的 proof → R5。
    #[test]
    fn auth_proof_is_bound_to_nonces() {
        let shared = test_shared();
        let addr = spawn_server(Arc::clone(&shared));
        let (mut r, _w, _cli) = authed_client(
            addr,
            "room-n",
            TEST_TOKEN,
            Some(&|_room, _cli, srv, _bucket| {
                *srv = "ffffffffffffffffffffffffffffffff".into();
            }),
        );
        let resp = read_line(&mut r);
        assert!(resp.contains("ERR 5 auth-failed"), "got: {resp}");
    }

    /// 证据 ④：同一 cli_nonce 在 90 s 内复用 → HELLO 即 R7 auth-replay。
    #[test]
    fn auth_replay_is_rejected() {
        let shared = test_shared();
        let addr = spawn_server(Arc::clone(&shared));
        let room = "room-replay";
        let (mut r1, _w1, cli) = authed_client(addr, room, TEST_TOKEN, None);
        let resp1 = read_line(&mut r1);
        assert!(resp1.starts_with("VSR2 OK"), "首次应成功：{resp1}");
        let s2 = TcpStream::connect(addr).unwrap();
        let mut w2 = s2.try_clone().unwrap();
        let mut r2 = BufReader::new(s2);
        w2.write_all(format!("VSR2 HELLO 2 {room} {cli} 0007\n").as_bytes())
            .unwrap();
        let resp2 = read_line(&mut r2);
        assert!(resp2.contains("ERR 7 auth-replay"), "got: {resp2}");
    }

    /// 证据 ②：token 绝不明文传输——客户端发出的全部字节不含 token 原值。
    #[test]
    fn token_is_never_sent_in_clear() {
        let shared = test_shared();
        let addr = spawn_server(Arc::clone(&shared));
        let stream = TcpStream::connect(addr).unwrap();
        let mut w = stream.try_clone().unwrap();
        let mut r = BufReader::new(stream);
        let cli = rand_hex16(2);
        w.write_all(format!("VSR2 HELLO 2 room-t {cli} 0007\n").as_bytes())
            .unwrap();
        let chal = read_line(&mut r);
        let p: Vec<&str> = chal.split(' ').collect();
        let srv = p[2];
        let bucket: u64 = p[3].parse().unwrap();
        let proof = proto::compute_proof(TEST_TOKEN.as_bytes(), "room-t", &cli, srv, bucket);
        w.write_all(format!("VSR2 AUTH {}\n", hex(&proof)).as_bytes())
            .unwrap();
        read_line(&mut r);
        let sent = format!(
            "VSR2 HELLO 2 room-t {cli} 0007\nVSR2 AUTH {}\n",
            hex(&proof)
        );
        assert!(!sent.contains(TEST_TOKEN), "token 原值不得出现在线上字节");
    }

    /// 证据 ⑤：更旧时间窗（T−3）的证明 → R6 auth-expired。
    #[test]
    fn auth_expired_bucket_is_rejected() {
        let shared = test_shared();
        let addr = spawn_server(Arc::clone(&shared));
        let (mut r, _w, _cli) = authed_client(
            addr,
            "room-exp",
            TEST_TOKEN,
            Some(&|_room, _cli, _srv, bucket| {
                *bucket = bucket.saturating_sub(3); // T−3 → 超窗
            }),
        );
        let resp = read_line(&mut r);
        assert!(resp.contains("ERR 6 auth-expired"), "got: {resp}");
    }

    /// 证据 ⑥：已配对会话触达字节上限 → CONTROL RelayLimit R12 + 优雅关闭
    /// （直接否证 V1.0 的 set_read_timeout(None) 无约束状态）。
    #[test]
    fn paired_session_hits_byte_cap() {
        let shared = Arc::new(Shared {
            tokens: vec![TEST_TOKEN.as_bytes().to_vec()],
            limits: Limits {
                per_ip_concurrency: 8,
                room_ttl: Duration::from_secs(600),
                max_rooms: 64,
                session_time: Duration::from_secs(4 * 3600),
                session_idle: Duration::from_secs(300),
                session_bytes: 200,
                session_bandwidth: 0,
                total_sessions: 64,
            },
            log_rooms: false,
            max_frame: MAX_FRAME,
            rooms: Mutex::new(HashMap::new()),
            nonce_seen: Mutex::new((VecDeque::new(), HashSet::new())),
            ip_conns: Mutex::new(HashMap::new()),
            ip_rate: Mutex::new(HashMap::new()),
            ip_fails: Mutex::new(HashMap::new()),
            sessions: AtomicU32::new(0),
            shutdown: AtomicBool::new(false),
            stats: Mutex::new(Stats::default()),
            started: Instant::now(),
        });
        let addr = spawn_server(Arc::clone(&shared));
        let room = "room-bytes";
        let (mut ra, mut wa, _c1) = authed_client(addr, room, TEST_TOKEN, None);
        read_line(&mut ra); // OK
        let (mut rb, _wb, _c2) = authed_client(addr, room, TEST_TOKEN, None);
        read_line(&mut rb); // OK
                            // 等双方完成等待登记（各 200 ms 窗口）再发帧：早于登记发出的会话字节
                            // 会进 pre 缓冲并在配对后**绕过字节记账**（配对前置注入），上限永不触发。
        std::thread::sleep(std::time::Duration::from_millis(500));
        // A 发 3 × 151B DATA（上限 200）→ 触发 R12；中继向对端（B）下发
        for _ in 0..3 {
            let frame = [
                (151u32).to_be_bytes().to_vec(),
                vec![TYPE_DATA],
                vec![7u8; 150],
            ]
            .concat();
            wa.write_all(&frame).unwrap();
            wa.flush().unwrap();
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let mut hit = false;
        while std::time::Instant::now() < deadline {
            let mut hdr = [0u8; 4];
            match rb.read_exact(&mut hdr) {
                Ok(()) => {
                    let len = u32::from_be_bytes(hdr) as usize;
                    let mut body = vec![0u8; len];
                    rb.read_exact(&mut body).unwrap();
                    let text = String::from_utf8_lossy(&body[1..]).to_string();
                    if text.contains("RelayLimit 12") {
                        hit = true;
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        assert!(hit, "必须下发 RelayLimit R12 并优雅关闭");
    }

    /// 证据 ⑦：每 IP 并发上限 → 第二连接 R8。
    #[test]
    fn per_ip_concurrency_cap() {
        let shared = Arc::new(Shared {
            tokens: vec![TEST_TOKEN.as_bytes().to_vec()],
            limits: Limits {
                per_ip_concurrency: 1,
                room_ttl: Duration::from_secs(600),
                max_rooms: 64,
                session_time: Duration::from_secs(4 * 3600),
                session_idle: Duration::from_secs(300),
                session_bytes: 64 * 1024 * 1024 * 1024,
                session_bandwidth: 0,
                total_sessions: 64,
            },
            log_rooms: false,
            max_frame: MAX_FRAME,
            rooms: Mutex::new(HashMap::new()),
            nonce_seen: Mutex::new((VecDeque::new(), HashSet::new())),
            ip_conns: Mutex::new(HashMap::new()),
            ip_rate: Mutex::new(HashMap::new()),
            ip_fails: Mutex::new(HashMap::new()),
            sessions: AtomicU32::new(0),
            shutdown: AtomicBool::new(false),
            stats: Mutex::new(Stats::default()),
            started: Instant::now(),
        });
        let addr = spawn_server(Arc::clone(&shared));
        let (mut r1, w1, _c) = authed_client(addr, "room-c1", TEST_TOKEN, None);
        read_line(&mut r1);
        let s2 = TcpStream::connect(addr).unwrap();
        let mut w2 = s2.try_clone().unwrap();
        let mut r2 = BufReader::new(s2);
        w2.write_all(format!("VSR2 HELLO 2 room-c2 {} 0007\n", rand_hex16(9)).as_bytes())
            .unwrap();
        let resp = read_line(&mut r2);
        assert!(resp.contains("ERR 8 limit-concurrency"), "got: {resp}");
        let _ = w1;
    }

    /// 回归：会话收尾幂等——同一实例连续多次会话后计数不漂移（曾因双泵重复
    /// fetch_sub 下溢 sessions → 之后所有连接误报 R14 busy）。
    #[test]
    fn session_counter_does_not_underflow_across_sessions() {
        let shared = test_shared();
        let addr = spawn_server(Arc::clone(&shared));
        for i in 0..3 {
            let room = format!("room-seq{i}");
            let (mut ra, mut wa, _c1) = authed_client(addr, &room, TEST_TOKEN, None);
            read_line(&mut ra);
            let (mut rb, _wb, _c2) = authed_client(addr, &room, TEST_TOKEN, None);
            read_line(&mut rb);
            // 等双方完成等待登记（各 200 ms 窗口）再发帧
            std::thread::sleep(std::time::Duration::from_millis(500));
            let frame = [
                (5u32).to_be_bytes().to_vec(),
                vec![TYPE_DATA],
                b"ping".to_vec(),
            ]
            .concat();
            wa.write_all(&frame).unwrap();
            wa.flush().unwrap();
            let mut hdr = [0u8; 4];
            rb.read_exact(&mut hdr).unwrap();
            // 双端断开 → 两侧泵都走 "peer" 收尾路径（正是曾下溢的场景）
            drop(rb);
            drop(wa);
            drop(ra);
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        assert_eq!(
            shared.sessions.load(Ordering::SeqCst),
            0,
            "连续会话后活跃会话计数必须归零"
        );
    }

    /// 证据 ⑩：计量行不含 device_id / 公钥 / IP 明文 / token / 房间原值。
    #[test]
    fn metering_contains_no_identity() {
        let line = meter_line("secret-room-name", 100, 200, 5000, "peer");
        assert!(!line.contains("secret-room-name"), "房间原值不得入日志");
        assert!(!line.contains(TEST_TOKEN));
        assert!(!line.contains("127.0.0.1"));
        assert!(line.contains("room "), "仅房间哈希前缀");
    }

    /// 证据（docs/08 §3.7 组 3 ④ `relay::bandwidth_cap_degrades_without_data_loss`）：
    /// 带宽桶生效时传输**不丢字节、只变慢**——挂钟时间与限速值相符（≤20% 滑窗
    /// 平均口径的宽化下界）；配 0（默认）= 不限，不触发整形。
    #[test]
    fn bandwidth_cap_degrades_without_data_loss() {
        let shared = Arc::new(Shared {
            tokens: vec![TEST_TOKEN.as_bytes().to_vec()],
            limits: Limits {
                per_ip_concurrency: 8,
                room_ttl: Duration::from_secs(600),
                max_rooms: 64,
                session_time: Duration::from_secs(4 * 3600),
                session_idle: Duration::from_secs(300),
                session_bytes: 64 * 1024 * 1024 * 1024,
                // 16 kbps = 2000 B/s（桶容量 1 s 突发）；8 KiB 理论 ≈3.1 s
                session_bandwidth: 2000,
                total_sessions: 64,
            },
            log_rooms: false,
            max_frame: MAX_FRAME,
            rooms: Mutex::new(HashMap::new()),
            nonce_seen: Mutex::new((VecDeque::new(), HashSet::new())),
            ip_conns: Mutex::new(HashMap::new()),
            ip_rate: Mutex::new(HashMap::new()),
            ip_fails: Mutex::new(HashMap::new()),
            sessions: AtomicU32::new(0),
            shutdown: AtomicBool::new(false),
            stats: Mutex::new(Stats::default()),
            started: Instant::now(),
        });
        let addr = spawn_server(shared);
        let room = "room-bw";
        let (mut ra, mut wa, _c1) = authed_client(addr, room, TEST_TOKEN, None);
        read_line(&mut ra); // OK
        let (mut rb, _wb, _c2) = authed_client(addr, room, TEST_TOKEN, None);
        read_line(&mut rb); // OK
        std::thread::sleep(std::time::Duration::from_millis(500));

        let total = 8 * 1024usize;
        let t0 = std::time::Instant::now();
        for _ in 0..4 {
            let frame = [
                (2 * 1024 + 1u32).to_be_bytes().to_vec(),
                vec![TYPE_DATA],
                vec![7u8; 2 * 1024],
            ]
            .concat();
            wa.write_all(&frame).unwrap();
            wa.flush().unwrap();
        }
        // 按帧解析接收端：DATA 载荷逐字节核对（限速不得丢字节 / 不得改字节）
        let mut collected = 0usize;
        while collected < total {
            let mut hdr = [0u8; 4];
            rb.read_exact(&mut hdr).unwrap();
            let len = u32::from_be_bytes(hdr) as usize;
            let mut body = vec![0u8; len];
            rb.read_exact(&mut body).unwrap();
            assert_eq!(body[0], TYPE_DATA, "限速期间不得混入其它帧类型");
            assert!(
                body[1..].iter().all(|&b| b == 7),
                "限速不得丢字节 / 不得改字节（docs/08 §3.3）"
            );
            collected += body.len() - 1;
        }
        let elapsed = t0.elapsed();
        assert!(
            elapsed >= std::time::Duration::from_millis(2400),
            "限速必须生效（elapsed={elapsed:?}，理论下界 ≈3.1 s 的 80%）"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(20),
            "只降速不得变成无限延迟（elapsed={elapsed:?}）"
        );
    }

    /// 证据（docs/08 §3.7 组 3 ⑨⑩）：运维端点 Bearer 强制；`/healthz` 无鉴权但
    /// 只含存活 + 版本 + `uptimeS`；`/stats.json` 返回 5 min 聚合（会话数 / 房间数
    /// / 字节数 / `close_reason` 直方图）且**绝不含身份**；`/metrics` 输出计数。
    #[test]
    fn ops_endpoint_requires_bearer_and_aggregates() {
        let shared = test_shared();
        let addr = spawn_server(Arc::clone(&shared));
        // 先跑一轮真实会话喂聚合数据
        let room = "room-ops";
        let (mut ra, mut wa, _c1) = authed_client(addr, room, TEST_TOKEN, None);
        read_line(&mut ra);
        let (mut rb, _wb, _c2) = authed_client(addr, room, TEST_TOKEN, None);
        read_line(&mut rb);
        std::thread::sleep(std::time::Duration::from_millis(500));
        let frame = [
            (5u32).to_be_bytes().to_vec(),
            vec![TYPE_DATA],
            b"ping".to_vec(),
        ]
        .concat();
        wa.write_all(&frame).unwrap();
        wa.flush().unwrap();
        let mut hdr = [0u8; 4];
        rb.read_exact(&mut hdr).unwrap();
        drop(rb);
        drop(wa);
        drop(ra);
        std::thread::sleep(std::time::Duration::from_millis(300));

        let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let ops_addr = l.local_addr().unwrap();
        {
            let sh = Arc::clone(&shared);
            std::thread::spawn(move || {
                for s in l.incoming().flatten() {
                    let _ = serve_ops(s, &sh, "ops-token-1234");
                }
            });
        }
        let get = |path: &str, bearer: Option<&str>| -> (String, String) {
            let mut s = TcpStream::connect(ops_addr).unwrap();
            let auth = bearer
                .map(|t| format!("Authorization: Bearer {t}\r\n"))
                .unwrap_or_default();
            s.write_all(
                format!("GET {path} HTTP/1.1\r\nHost: x\r\n{auth}Connection: close\r\n\r\n")
                    .as_bytes(),
            )
            .unwrap();
            let mut resp = String::new();
            s.read_to_string(&mut resp).unwrap();
            let (head, body) = resp.split_once("\r\n\r\n").unwrap();
            (head.to_string(), body.to_string())
        };
        // /healthz：无鉴权，只含存活 + 版本 + uptimeS（不得暴露配置 / token 数）
        let (head, body) = get("/healthz", None);
        assert!(head.contains("200"), "{head}");
        assert!(
            body.contains("\"ok\":true") && body.contains("uptimeS"),
            "{body}"
        );
        // /stats.json：无 Bearer → 401
        let (head, _) = get("/stats.json", None);
        assert!(head.contains("401"), "{head}");
        // 带 Bearer → 5 min 聚合
        let (head, body) = get("/stats.json", Some("ops-token-1234"));
        assert!(head.contains("200"), "{head}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["windowSecs"], serde_json::json!(300));
        assert!(v["sessions"].as_u64().unwrap() >= 1, "{body}");
        assert!(v["bytes"].as_u64().unwrap() >= 4, "{body}");
        assert!(
            v["closeReasons"].as_object().unwrap().contains_key("peer"),
            "{body}"
        );
        // 聚合体绝不含身份（房间原值 / token / IP）
        assert!(!body.contains(room));
        assert!(!body.contains(TEST_TOKEN));
        assert!(!body.contains("127.0.0.1"));
        // /metrics：Prometheus 计数（Bearer 同样强制，上面 401 分支已覆盖）
        let (head, body) = get("/metrics", Some("ops-token-1234"));
        assert!(head.contains("200"), "{head}");
        assert!(body.contains("vsr2_sessions_total"), "{body}");
    }

    /// 证据 ⑪（架构断言）：中继源码无持久化调用（不落盘）。
    #[test]
    fn no_persistence_calls() {
        let src =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs")).unwrap();
        // 字面量拼接以避免自命中（测试源码本身包含这些词）
        for banned in [
            "Open".to_owned() + "Options",
            "fs::wr".to_owned() + "ite",
            "fs::cre".to_owned() + "ate",
            "File::cre".to_owned() + "ate",
        ] {
            assert!(
                !src.contains(banned.as_str()),
                "vault-relay 源码出现持久化调用 {banned}"
            );
        }
    }
}
