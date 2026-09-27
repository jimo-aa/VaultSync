//! P2P 同步引擎编排（docs/05-03、docs/08）。
//!
//! 职责：设备监听 / 配对（邀请码 PSK + 指纹核验）/ 增量同步（块清单比对 + 差量传输 +
//! 断点续传幂等 + 限速）/ 冲突解决（删除优先 + 多版本副本 + 向量时钟）/ 远程销毁（签名 + 延迟语义）。
//!
//! 密钥红线：FSKey 仅经 E2E 信道携带给已配对对端；中继只见不透明 Noise 帧。
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::io::Write as IoWrite;
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use vault_crypto::{aead_decrypt, aead_encrypt, hex_encode, random_bytes, KEY_LEN};
use vault_vault::vault::Vault;
use zeroize::Zeroizing;

use crate::burn::{self, BurnPhase, BurnTicket};
use crate::channel::{psk_from_code, subkey, Role, SecureChannel};
use crate::identity::Identity;
use crate::orders::{OrderRec, OrderStore};
use crate::peers::{PeerRec, PeerStore};
use crate::policy::{SyncContext, SyncFilter};
use crate::proto::{delete_sign_body, destroy_sign_body, hello_sign_body, ManifestItem, Msg};
use crate::relay_client::{self, RelayFailure, RelayStream};

/// 误删保护窗口（docs/05-03 §6.2：窗口期内删除保留加密副本）。
const PROTECT_WINDOW_MS: u64 = 24 * 3600 * 1000;
const INVITE_TTL_MS: u64 = 10 * 60 * 1000;
/// P8-4：映射观测超时（每个观测目标；失败即如实降级，不打洞）。
const OBSERVE_TIMEOUT: Duration = Duration::from_millis(800);
/// P8-4：单次打洞尝试窗口（05-03 §四「打洞成功（≤5 s）」）。
const PUNCH_TIMEOUT: Duration = Duration::from_secs(5);
/// P8-4：候选上限（05-03 §3.2「每端候选上限 8 个」——含观测到的公网映射）。
const MAX_CANDIDATES: usize = 8;
/// P8-4：配对短码有效期（05-03 §3.3：120 s，过期需重新发起）。
pub const MATCH_CODE_TTL_MS: u64 = 120 * 1000;
const MAX_EVENTS: usize = 100;
/// P8-9：阅后即焚的明文分片大小（信道内传输，`Noise` 分帧上限之下）。
const BURN_CHUNK: usize = 256 * 1024;

/// 直连（`TcpStream`）与中继（`relay_client::RelayStream`）的公共流抽象——P8-5：
/// 同步全链路（Noise 握手 / Hello / 清单 / 块传输）在两种承载上透明运行。
/// `SecureChannel` 泛型于 `Read + Write`，`Box<dyn Duplex>` 即满足；本 trait
/// 只补齐两种承载的差异面（读超时与源地址观测）。
pub trait Duplex: std::io::Read + std::io::Write + Send {
    fn set_read_timeout(&self, d: Option<Duration>) -> std::io::Result<()>;
    /// 源端地址观测（中继承载时为**中继**地址，调用方以 relayed 标志区分，
    /// 绝不当成对端回拨地址——与既有纪律一致）。
    fn peer_addr(&self) -> std::io::Result<SocketAddr>;
    /// P8-4：承载方提供的 UDP 映射观测端点（直连无中继 → 空 = 不做打洞尝试）。
    fn observe_addrs(&self) -> Vec<SocketAddr> {
        Vec::new()
    }
}

impl Duplex for TcpStream {
    fn set_read_timeout(&self, d: Option<Duration>) -> std::io::Result<()> {
        TcpStream::set_read_timeout(self, d)
    }
    fn peer_addr(&self) -> std::io::Result<SocketAddr> {
        TcpStream::peer_addr(self)
    }
}

impl Duplex for RelayStream {
    fn set_read_timeout(&self, d: Option<Duration>) -> std::io::Result<()> {
        RelayStream::set_read_timeout(self, d)
    }
    fn peer_addr(&self) -> std::io::Result<SocketAddr> {
        Ok(self.relay_addr())
    }
    fn observe_addrs(&self) -> Vec<SocketAddr> {
        RelayStream::observe_addrs(self)
    }
}

/// P8-4：per-peer 打洞统计（**进程内存态**；只记计数与原因，**不记 IP**——
/// 与 `path_status` 的隐私纪律一致：载荷只给候选数量与类型）。
#[derive(Default, Clone)]
struct PunchStat {
    attempts: u64,
    /// 最近一次是否**双向证实**打通。
    ok: bool,
    /// 失败 / 跳过原因（`hole_punch_timeout` / `symmetric_nat` / `no_observe_endpoint`
    /// / `no_candidates` / `observe_failed`）。
    reason: Option<&'static str>,
    /// 同一 socket 对不同观测目标映射端口不同（对称 NAT 判定）。
    symmetric_nat: bool,
    local_candidates: u32,
    remote_candidates: u32,
    /// 路径状态机降级步数（`PathMachine::steps()`：0 = 一次打通，1 = 已降级到中继）。
    degraded_steps: u8,
    /// 路径状态机判定后的路径（`punched` / `relayed`）。
    kind: Option<&'static str>,
}

/// P8-4：一次打洞尝试的结果（记账与序列化的紧凑载体；**只有计数、原因与端口，无 IP**）。
struct PunchReport {
    ok: bool,
    reason: Option<&'static str>,
    symmetric_nat: bool,
    local_candidates: u32,
    remote_candidates: u32,
    degraded_steps: u8,
    kind: &'static str,
    /// 本端在观测 #1（中继眼中）的映射端口——核对 NAT 类型用（非 IP）。
    mapped_port: Option<u16>,
    /// 本端在观测 #2（**对端**眼中）的映射端口——对称 NAT 的直接证据（非 IP）。
    peer_observed_port: Option<u16>,
    elapsed_ms: u64,
}

impl PunchReport {
    /// 结构化输出（探针直接返回它；`path_status` 的 `punch` 段是它的子集）。
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": 1,
            "ok": self.ok,
            "reason": self.reason,
            "symmetricNat": self.symmetric_nat,
            "localCandidates": self.local_candidates,
            "remoteCandidates": self.remote_candidates,
            "kind": self.kind,
            "degradedSteps": self.degraded_steps,
            "mappedPort": self.mapped_port,
            "peerObservedPort": self.peer_observed_port,
            "elapsedMs": self.elapsed_ms,
        })
    }
}

/// P8-4：探针角色（Noise 握手必须一端发起、一端响应）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeRole {
    Initiator,
    Responder,
}

impl ProbeRole {
    pub fn as_str(self) -> &'static str {
        match self {
            ProbeRole::Initiator => "initiator",
            ProbeRole::Responder => "responder",
        }
    }

    /// 由 FFI 的整数编码还原（`0` = 发起端，其它 = 响应端）。
    pub fn from_code(code: u32) -> Self {
        if code == 0 {
            ProbeRole::Initiator
        } else {
            ProbeRole::Responder
        }
    }
}

/// per-peer 路径统计（P8-4/P8-5；**进程内存态**：只反映本进程观测到的会话，
/// 不落盘、不编造——未观测到的字段如实缺省）。
#[derive(Default, Clone)]
struct PeerPathStat {
    /// 最近一次会话的路径："direct" | "relay"；None = 尚无完成握手的会话。
    path: Option<&'static str>,
    path_since_ms: u64,
    attempts_direct: u64,
    attempts_relay: u64,
    /// 最近一次失败原因（目前仅中继失败带明确 reason：docs/08 §3.4 映射）。
    last_failure: Option<(String, u64)>,
    /// 最近一次**本端发起**的中继会话线上字节（0 = 无观测）。
    bytes_in: u64,
    bytes_out: u64,
    /// 最近一次会话生效的填充档（0 = 未协商/旧端）。
    padding_tier: u8,
    /// P8-4：打洞尝试统计（与 `path` 分离——数据仍走中继时不得谎称已走打洞路径）。
    punch: PunchStat,
}

/// P8-4：Hello 交换结果（含对端声明的候选，供打洞尝试使用）。
struct HelloInfo {
    device_id: String,
    name: String,
    pub_hex: String,
    port: u16,
    pq: bool,
    /// 对端候选（`ip:port`；旧端 → 空）。
    candidates: Vec<String>,
}

/// P8-4：打洞准备结果（绑定探测 socket + 映射观测 + 对称 NAT 前置判定）。
struct PreparedPunch {
    /// 专用 UDP 探测 socket（其本地端口即候选端口）。
    sock: UdpSocket,
    /// 本端候选（观测到的映射在前，其余为网卡地址；上限见 `MAX_CANDIDATES`）。
    candidates: Vec<String>,
    /// 观测 #1：中继眼中的本机映射。
    mapped: Option<SocketAddr>,
    /// 观测 #1 失败 / 无端点的如实原因。
    observe_reason: Option<&'static str>,
    /// 两个观测目标给出不同映射端口 → 对称 NAT（前置判定，不打洞）。
    symmetric: bool,
    /// 本端 LAN 类候选数（PathStatus 记账用，不含映射）。
    lan_count: u32,
}

/// 会话承载路径描述（发起端路径记账用）。
struct SessionPath {
    relayed: bool,
    /// 中继线上字节句柄（本端发起的中继会话才携带；收尾时读取终值）。
    wires: Option<(Arc<AtomicU64>, Arc<AtomicU64>)>,
}

impl SessionPath {
    fn direct() -> Self {
        Self {
            relayed: false,
            wires: None,
        }
    }
    fn relay(wires: (Arc<AtomicU64>, Arc<AtomicU64>)) -> Self {
        Self {
            relayed: true,
            wires: Some(wires),
        }
    }
}

/// 远程销毁的本机擦除回调（由宿主 vault-core 注入：删保险箱文件 + 数据目录）。
pub type WipeFn = Box<dyn Fn() -> Result<(), String> + Send>;
/// P7-10 远程锁定的本机执行回调（清保险箱槽位与密钥材料）。
pub type LockFn = Box<dyn Fn() + Send>;

/// 同步作业控制面（P8-2）：取消 + 暂停门，成员与宿主任务注册表的
/// TaskCtl 同源（由 FFI 层桥接），使 task_pause/task_cancel 在引擎的
/// 文件 / 块检查点上即时生效。
#[derive(Clone, Default)]
pub struct SyncCtl {
    pub cancel: Arc<AtomicBool>,
    pub pause: Arc<AtomicBool>,
    pub gate: Arc<(Mutex<bool>, Condvar)>,
}

impl SyncCtl {
    /// 检查点：取消 → Err("cancelled")；暂停中阻塞等待恢复。
    pub fn checkpoint(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::SeqCst) {
            return Err("cancelled".into());
        }
        let (lock, cv) = &*self.gate;
        let mut go = lock.lock().unwrap_or_else(|e| e.into_inner());
        while !*go && !self.cancel.load(Ordering::SeqCst) {
            go = cv
                .wait_timeout(go, Duration::from_millis(200))
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        if self.cancel.load(Ordering::SeqCst) {
            return Err("cancelled".into());
        }
        Ok(())
    }
}

/// 销毁指令被拒的原因（docs/08 §四.3：破坏性信令必须验签后执行）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderReject {
    /// 签名不合法或载荷被篡改：不执行、记事件、断开且不回执（不给伪造者任何回执）。
    BadSignature,
    /// `target` 不是本机 device_id：不执行、回执 Error（多半是投递到了错误设备）。
    TargetMismatch,
    /// 载荷不是一枚合法的销毁指令帧。
    Malformed,
    /// P7-11：武装状态落盘失败——拒绝武装（不留纯内存倒计时）。
    PersistFailed,
    /// P7-4：两端旧 MK 不同（ct 解封失败）——拒绝且不写任何状态。
    MkMismatch,
}

struct Invite {
    psk: [u8; 32],
    expires_ms: u64,
}

struct ArmedDestroy {
    deadline: Instant,
    target: String,
}

/// 本机所发销毁指令的对端回执。
enum AckOutcome {
    /// 对端已验签、核对目标并接受（延迟武装或立即擦除）。
    Ack,
    /// 对端回执 Error（如 target 不符）：保留在队列，等下次上线重投。
    Rejected(String),
}

/// 「提前到达的业务消息」上限：发起端在 Hello 后立刻发 SyncReq，可能在销毁指令的 ACK 之前
/// 到达，需要暂存而不是当成协议错误。设上限避免对端用消息洪水撑爆内存。
const MAX_EARLY_MSGS: usize = 32;

/// 会话内消息暂存：投递/等待 ACK 期间提前到达的业务消息存放于此，
/// 由调用方紧接着的正常读取取走（消息不丢、顺序不变）。
#[derive(Default)]
struct EarlyMsgs(VecDeque<Msg>);

impl EarlyMsgs {
    fn pop(&mut self) -> Option<Msg> {
        self.0.pop_front()
    }

    fn push(&mut self, m: Msg) -> Result<(), String> {
        if self.0.len() >= MAX_EARLY_MSGS {
            return Err("too many interleaved messages".into());
        }
        self.0.push_back(m);
        Ok(())
    }
}

/// 由 TCP 观测地址 + Hello 声明的监听端口拼出可回拨地址。
/// 端口 0 = 对端（旧端）未声明 → 回退用观测到的源端口（尽力而为）；地址非 IP（不可能）则放弃。
fn dialable_addr(observed: Option<std::net::SocketAddr>, peer_port: u16) -> Option<String> {
    let obs = observed?;
    let port = if peer_port == 0 {
        obs.port()
    } else {
        peer_port
    };
    Some(format!("{}:{port}", obs.ip()))
}

/// 引擎密钥束（P7-2）：从属密钥由调用方（会话）注入，引擎不接触 keystore。
#[derive(Clone, Copy)]
pub struct EngineKeys {
    pub mk: [u8; KEY_LEN],
    /// 从属密钥 4：orders.enc
    pub orders: [u8; KEY_LEN],
    /// 从属密钥 8：设备身份 / 发现指纹
    pub ident: [u8; KEY_LEN],
    /// 从属密钥 1/2：索引与搜索（保险箱槽位打开用）
    pub index: [u8; KEY_LEN],
    pub search: [u8; KEY_LEN],
}

pub struct EngineInner {
    ident: Identity,
    /// 密钥束副本（P7-4 轮换 notice 解封需 MK；与调用方同值）
    keys_rt: EngineKeys,
    device_name: String,
    data_dir: PathBuf,
    /// 与会话共享的保险箱槽位（None = 会话已锁定）。
    vault_slot: Arc<Mutex<Option<Vault>>>,
    peers: Mutex<PeerStore>,
    /// 离线设备的销毁指令待投递队列（AEAD 落盘，键 HKDF(MK,"p2p-orders")）。
    orders: Mutex<OrderStore>,
    invite: Mutex<Option<Invite>>,
    events: Mutex<Vec<String>>,
    armed: Mutex<Option<ArmedDestroy>>,
    wipe: Mutex<Option<WipeFn>>,
    /// P7-10：远程锁定执行回调（可重入：调用方回填同一闭包）。
    lock_cb: Mutex<Option<LockFn>>,
    port: AtomicU64,
    rate_bps: AtomicU64,
    /// P7-8：最近一次成功握手的 cipher_suite（0 = 尚未握手 / 1 / 2），status 暴露。
    last_suite: AtomicU8,
    /// P8-6：最近一次握手的填充协商结果与诊断（status 暴露）。
    pad_tier: AtomicU8,
    pad_legacy: AtomicBool,
    /// P8-4：是否尝试打洞（默认开；有观测端点且两端都声明候选时才真的会跑）。
    punch_enabled: AtomicBool,
    frame_hist: Mutex<[u64; 4]>,
    /// P8-2：当前同步作业的控制面（None = 非队列作业，检查点直通）。
    sync_ctl: Mutex<Option<SyncCtl>>,
    /// P8-4：发现开关（默认开；关闭后不发也不收 mDNS）。
    discovery_enabled: AtomicBool,
    /// P8-4：mDNS 守护进程（广播本机 `vs-<fp4>`；懒启动，失败静默降级）。
    mdns: Mutex<Option<vault_net::mdns_sd::ServiceDaemon>>,
    /// P8-4：最近一次配对的短码与有效期（双端独立计算，人工比对用）。
    match_code: Mutex<Option<(String, u64)>>,
    /// P8-4/P8-5：per-peer 路径统计（path_status per-peer 段来源；进程内存态）。
    path_stats: Mutex<HashMap<String, PeerPathStat>>,
    /// P8-5：最近一次未归属对端的路径失败（拨号期失败无法定位 peer）。
    last_path_failure: Mutex<Option<(String, u64)>>,
    /// P8-6：填充开销记账（载荷 / 线上字节累计，overheadRatio 来源）。
    bytes_payload: AtomicU64,
    bytes_wire: AtomicU64,
    /// P8-7：选择性同步过滤策略（`enabled=false` = 默认全同步；由 vault-core 注入）。
    sync_filter: Mutex<SyncFilter>,
    /// P8-7：外壳注入的会话上下文（时段 / 网络类型）。
    sync_ctx: Mutex<SyncContext>,
    /// P8-7：本次会话被跳过的条目（`(fileId, reason)`；会话开始清空）。
    skipped: Mutex<Vec<(u64, String)>>,
    /// P8-9：本机持有的阅后即焚票据（发送端；秘密仅内存）。
    burns_local: Mutex<HashMap<u64, BurnLocal>>,
    /// P8-9：已接收的阅后即焚载荷（接收端；落盘密钥仅内存）。
    burns_inbox: Mutex<HashMap<u64, BurnInbox>>,
    /// P8-9：接收端票据秘密（仅内存；由邀请码还原，用于派生落盘密钥）。
    burn_secret_in: Mutex<Option<Zeroizing<Vec<u8>>>>,
    /// P8-9：burn_id 分配序号。
    burn_seq: AtomicU64,
    dead: AtomicBool,
}

/// P8-9：发送端票据状态。
struct BurnLocal {
    ticket: BurnTicket,
    phase: BurnPhase,
    /// 最近一次投递目标的诊断串（不含明文）。
    target: Option<String>,
}

/// P8-9：接收端的一次性报价（打包传参，避免超长参数表）。
struct BurnOfferRec {
    burn_id: u64,
    name: String,
    size: u64,
    sha: String,
    total_chunks: usize,
    open_timeout_secs: u64,
}

/// P8-9：接收端载荷状态（`key` 仅内存 → 重启后不可解，自然收敛 `Expired`）。
struct BurnInbox {
    name: String,
    size: u64,
    key: Zeroizing<[u8; KEY_LEN]>,
    stored: PathBuf,
    phase: BurnPhase,
    opened_ms: Option<u64>,
    /// 用户显式「另存」→ 放弃该次自动擦除（`05-04` §5.5）。
    saved: bool,
    /// 打开后的自动擦除窗口（秒；来自报价）。
    open_timeout_secs: u64,
    bytes: u64,
    retries: u32,
}

#[derive(Clone)]
pub struct P2pEngine {
    inner: Arc<EngineInner>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 候选列表为空（未配置或被 `allowPublicRelay = false` 过滤后为空）时的失败：
/// `13 + relay_unavailable`——语义是「无可用中继路径」，不是参数错误。
fn empty_candidates_failure() -> RelayFailure {
    RelayFailure {
        relay_code: 0,
        reason: "relay_unavailable".into(),
        local_code: 13,
        degrade_reason: Some("relay_unavailable"),
        diag: "中继候选列表为空：未配置自建中继（或官方中继已被禁用）".into(),
    }
}

/// 向量时钟支配判定：a 支配 b 当且仅当 b 的每一位 ≤ a 且至少一位严格 >。
pub(crate) fn dominates(a: &BTreeMap<String, u64>, b: &BTreeMap<String, u64>) -> bool {
    if a == b {
        return false;
    }
    let ge = b.iter().all(|(k, v)| a.get(k).copied().unwrap_or(0) >= *v);
    let gt = a.iter().any(|(k, v)| b.get(k).copied().unwrap_or(0) < *v);
    ge && gt
}

impl P2pEngine {
    /// 创建引擎：加载/生成设备身份（AEAD 于 MK 子密钥落盘），打开保险箱槽位并启动监听。
    /// P7-10：注入远程锁定回调（清保险箱槽位与密钥材料；可重入）。
    pub fn set_lock_fn(&self, f: LockFn) {
        *self.inner.lock_cb.lock().unwrap_or_else(|e| e.into_inner()) = Some(f);
    }

    /// 是否存在已配对设备（P7-3 轮换传播空集判定）。
    pub fn has_paired_peers(&self) -> bool {
        !self
            .inner
            .peers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .list()
            .is_empty()
    }

    pub fn new(
        vault_path: &Path,
        keys: &EngineKeys,
        device_name: &str,
        vault_slot: Arc<Mutex<Option<Vault>>>,
        wipe: WipeFn,
    ) -> Result<Self, &'static str> {
        let mk = &keys.mk;
        let stem = vault_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("invalid vault name")?;
        let parent = vault_path.parent().unwrap_or(Path::new("."));
        let data_dir = parent.join(format!("{stem}.data"));
        let p2p_dir = data_dir.join("p2p");
        std::fs::create_dir_all(&p2p_dir).map_err(|_| "cannot create p2p dir")?;

        // P7-2：身份/发现密钥（从属密钥 8）与订单密钥（从属密钥 4）由调用方注入；
        // 对端表（p2p-peers）不在从属密钥层，仍由 MK 派生。
        let ident_key = keys.ident;
        let ident = match std::fs::read(p2p_dir.join("device.id")) {
            Ok(blob) => {
                let pt = aead_decrypt(&ident_key, &blob).ok_or("identity decrypt failed")?;
                let seed: [u8; 32] = pt.as_slice().try_into().map_err(|_| "identity len")?;
                Identity::from_seed(&seed)
            }
            Err(_) => {
                let id = Identity::generate();
                let nonce_v = random_bytes(vault_crypto::AES_GCM_NONCE_LEN);
                let nonce: [u8; vault_crypto::AES_GCM_NONCE_LEN] =
                    nonce_v.as_slice().try_into().map_err(|_| "nonce len")?;
                let blob =
                    aead_encrypt(&ident_key, &nonce, &id.seed()).map_err(|_| "identity wrap")?;
                std::fs::write(p2p_dir.join("device.id"), &blob)
                    .map_err(|_| "cannot write identity")?;
                id
            }
        };

        let peers = PeerStore::load_or_new(&p2p_dir, mk)?;
        let orders = OrderStore::load_or_new(&p2p_dir, &keys.orders)?;
        let engine = Self {
            inner: Arc::new(EngineInner {
                ident,
                keys_rt: *keys,
                device_name: device_name.to_string(),
                data_dir: data_dir.clone(),
                vault_slot,
                peers: Mutex::new(peers),
                orders: Mutex::new(orders),
                invite: Mutex::new(None),
                events: Mutex::new(Vec::new()),
                armed: Mutex::new(None),
                wipe: Mutex::new(Some(wipe)),
                lock_cb: Mutex::new(None),
                port: AtomicU64::new(0),
                rate_bps: AtomicU64::new(0),
                last_suite: AtomicU8::new(0),
                pad_tier: AtomicU8::new(0),
                pad_legacy: AtomicBool::new(false),
                punch_enabled: AtomicBool::new(true),
                frame_hist: Mutex::new([0u64; 4]),
                sync_ctl: Mutex::new(None),
                discovery_enabled: AtomicBool::new(true),
                mdns: Mutex::new(None),
                match_code: Mutex::new(None),
                path_stats: Mutex::new(HashMap::new()),
                last_path_failure: Mutex::new(None),
                bytes_payload: AtomicU64::new(0),
                bytes_wire: AtomicU64::new(0),
                sync_filter: Mutex::new(SyncFilter::default()),
                sync_ctx: Mutex::new(SyncContext::default()),
                skipped: Mutex::new(Vec::new()),
                burns_local: Mutex::new(HashMap::new()),
                burns_inbox: Mutex::new(HashMap::new()),
                burn_secret_in: Mutex::new(None),
                burn_seq: AtomicU64::new(1),
                dead: AtomicBool::new(false),
            }),
        };

        // P7-11：武装状态续走（可能立即擦除）
        engine.resume_armed_on_startup();

        // 打开保险箱进槽位；设备位供向量时钟自增
        let mut part_scan_report = String::new();
        {
            let mut guard = engine
                .inner
                .vault_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if guard.is_none() {
                let mut v = Vault::open(vault_path, mk, &keys.index, &keys.search)?;
                v.set_device_id(&engine.inner.ident.device_id());
                // P8-3：启动恢复扫描（05-03 §6.2 情形 1/3/5/6；情形 2 需配对表，
                // 由对端投递时校验兜底）——配额 = max(5 GiB, 库大小 × 20%)
                let known: HashSet<(u64, [u8; 32])> = {
                    let manifest = v.sync_manifest().unwrap_or_default();
                    serde_json::from_str::<Vec<ManifestItem>>(&manifest)
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|i| {
                            vault_crypto::hex_decode(&i.file_sha)
                                .and_then(|b| <[u8; 32]>::try_from(b).ok())
                                .map(|s| (i.id, s))
                        })
                        .collect()
                };
                let vault_bytes = std::fs::metadata(vault_path).map(|m| m.len()).unwrap_or(0);
                let quota = (vault_bytes * 20 / 100).max(5 * 1024 * 1024 * 1024);
                let actions = vault_net::part::scan_dir(&data_dir, now_ms(), quota, &|id, sha| {
                    known.contains(&(id, *sha))
                });
                let mut kept = 0usize;
                let mut dropped = 0usize;
                for (p, a) in &actions {
                    match a {
                        vault_net::part::ScanAction::Resumable { .. } => kept += 1,
                        _ => dropped += 1,
                    }
                    let _ = p;
                }
                if !actions.is_empty() {
                    part_scan_report =
                        format!("part scan: kept={kept} dropped={dropped} (quota={quota})");
                }
                *guard = Some(v);
            }
        }
        if !part_scan_report.is_empty() {
            engine.log(&part_scan_report);
        }

        let listener = TcpListener::bind(("0.0.0.0", 0)).map_err(|_| "cannot bind listener")?;
        engine.inner.port.store(
            listener.local_addr().map_err(|_| "addr")?.port() as u64,
            Ordering::SeqCst,
        );
        listener.set_nonblocking(true).map_err(|_| "nonblocking")?;
        let bg = engine.clone();
        std::thread::Builder::new()
            .name("vsync-listener".into())
            .spawn(move || bg.listener_loop(listener))
            .map_err(|_| "spawn listener")?;
        Ok(engine)
    }

    pub fn device_id(&self) -> String {
        self.inner.ident.device_id()
    }

    pub fn fingerprint(&self) -> String {
        self.inner.ident.fingerprint()
    }

    pub fn port(&self) -> u16 {
        self.inner.port.load(Ordering::SeqCst) as u16
    }

    pub fn set_rate_bps(&self, bps: u64) {
        self.inner.rate_bps.store(bps, Ordering::SeqCst);
    }

    /// P8-2：绑定/清除当前同步作业的控制面（task_pause / task_cancel 在
    /// 引擎的文件与块检查点上生效）。
    pub fn set_sync_ctl(&self, ctl: Option<SyncCtl>) {
        *self
            .inner
            .sync_ctl
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = ctl;
    }

    /// P8-4：发现开关。关闭后不发也不收 mDNS（只保留邀请码 / 手动载体）；
    /// 关闭是用户选择，不报错。
    pub fn set_discovery_enabled(&self, on: bool) {
        self.inner.discovery_enabled.store(on, Ordering::SeqCst);
        if !on {
            // 停止广播（守护进程直接丢弃；浏览按需创建，不受影响）
            *self.inner.mdns.lock().unwrap_or_else(|e| e.into_inner()) = None;
            self.log("discovery: disabled by user");
        }
    }

    /// P8-4：开始广播本机 `vs-<fp4>`（懒启动；失败静默——发现不可用如实
    /// 降级，邀请码 / 手动载体仍在）。
    fn ensure_mdns(&self) -> Option<vault_net::mdns_sd::ServiceDaemon> {
        if !self.inner.discovery_enabled.load(Ordering::SeqCst) {
            return None;
        }
        let mut guard = self.inner.mdns.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            let daemon = vault_net::mdns_sd::ServiceDaemon::new().ok()?;
            let fp = self.inner.ident.public_hex();
            // 只在广播成功时缓存守护进程；失败则下次重试（发现不可用如实降级）
            if vault_net::discovery::advertise(&daemon, &fp, "0001", self.port()).is_some() {
                *guard = Some(daemon);
            }
        }
        guard.clone()
    }

    /// P8-4：局域网发现快照（docs/05-03 §3.4）。浏览 1.5 s 聚合候选；
    /// 未配对设备 `deviceId` / `name` 为 null；reachable 经 300 ms TCP 轻量
    /// 探测（不打洞、不建长连接）。**发现不构成身份证明**：配对仍须带外要素。
    pub fn discover(&self) -> serde_json::Value {
        let enabled = self.inner.discovery_enabled.load(Ordering::SeqCst);
        let mut candidates: Vec<serde_json::Value> = Vec::new();
        if enabled {
            if let Some(daemon) = self.ensure_mdns() {
                let receiver = daemon.browse(vault_net::discovery::SERVICE_TYPE);
                if let Ok(rx) = receiver {
                    let deadline =
                        std::time::Instant::now() + std::time::Duration::from_millis(1500);
                    let mut seen: Vec<vault_net::discovery::Discovered> = Vec::new();
                    while std::time::Instant::now() < deadline {
                        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                            Ok(vault_net::mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                                let props = info.get_properties();
                                let fp = props
                                    .get_property_val_str("fp")
                                    .unwrap_or_default()
                                    .to_string();
                                let port = info.get_port();
                                let now = now_ms();
                                vault_net::discovery::aggregate(
                                    &mut seen,
                                    vault_net::discovery::Discovered {
                                        instance: info.get_fullname().to_string(),
                                        fp,
                                        addrs: info
                                            .get_addresses()
                                            .iter()
                                            .map(|ip| format!("{}:{}", ip, port))
                                            .collect(),
                                        port,
                                        last_seen_ms: now,
                                    },
                                    now,
                                    vault_net::discovery::RETAIN_MS,
                                );
                            }
                            Ok(_) => {}
                            Err(_) => break,
                        }
                    }
                    for d in &seen {
                        let fp8 = d.fp.clone();
                        let paired = {
                            let peers = self.inner.peers.lock().unwrap_or_else(|e| e.into_inner());
                            peers
                                .list()
                                .iter()
                                .any(|(_, r)| r.pub_hex.starts_with(&fp8))
                        };
                        let (device_id, name) = if paired {
                            let peers = self.inner.peers.lock().unwrap_or_else(|e| e.into_inner());
                            match peers
                                .list()
                                .iter()
                                .find(|(_, r)| r.pub_hex.starts_with(&fp8))
                            {
                                Some((pid, r)) => (Some(pid.clone()), Some(r.name.clone())),
                                None => (None, None),
                            }
                        } else {
                            (None, None) // 未配对：身份信息只在配对后可获得
                        };
                        // 轻量探测：TCP 300 ms（不打洞、不建长连接）
                        let mut reachable = false;
                        let mut rtt: Option<u64> = None;
                        for a in &d.addrs {
                            if let Ok(addr) = a.parse::<std::net::SocketAddr>() {
                                let t0 = std::time::Instant::now();
                                if std::net::TcpStream::connect_timeout(
                                    &addr,
                                    std::time::Duration::from_millis(300),
                                )
                                .is_ok()
                                {
                                    reachable = true;
                                    rtt = Some((t0.elapsed().as_millis() as u64).max(1));
                                    break;
                                }
                            }
                        }
                        candidates.push(serde_json::json!({
                            "deviceId": device_id,
                            "name": name,
                            "fingerprint": d.fp,
                            "addrs": d.addrs.iter().map(|a| serde_json::json!({
                                "addr": a, "transport": "tcp",
                                "reachable": reachable, "rttMs": rtt,
                            })).collect::<Vec<_>>(),
                            "paired": paired,
                        }));
                    }
                }
            }
        }
        let (code, expires) = self
            .inner
            .match_code
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .map(|(c, e)| (Some(c), Some(e)))
            .unwrap_or((None, None));
        serde_json::json!({
            "schema": 1,
            "tsMs": now_ms(),
            "enabled": enabled,
            "self": {
                "deviceId": self.device_id(),
                "fingerprint": self.fingerprint(),
                "port": self.port(),
            },
            "matchCode": code,
            "matchCodeExpiresMs": expires,
            "candidates": candidates,
        })
    }

    /// 同步检查点：取消 → Err("cancelled")；暂停中阻塞等待恢复。
    /// 无控制面（非队列作业）时直通。
    pub fn sync_checkpoint(&self) -> Result<(), String> {
        let ctl = self
            .inner
            .sync_ctl
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        match ctl {
            Some(c) => c.checkpoint(),
            None => Ok(()),
        }
    }

    fn log(&self, msg: &str) {
        let mut ev = self.inner.events.lock().unwrap_or_else(|e| e.into_inner());
        ev.push(format!("{} {msg}", now_ms()));
        if ev.len() > MAX_EVENTS {
            ev.remove(0);
        }
    }

    /// P7-8：握手成功后统一记账——套件号入 status、降级原因入事件日志（强制可见）。
    fn note_channel(&self, ch: &SecureChannel, context: &str) {
        self.inner
            .last_suite
            .store(ch.cipher_suite(), Ordering::SeqCst);
        match (ch.cipher_suite(), ch.pq_note()) {
            (2, _) => self.log(&format!("{context}: cipher_suite=2 (hybrid pq)")),
            (1, Some(note)) if note != "pq not offered" => {
                self.log(&format!("{context}: cipher_suite=1 ({note})"))
            }
            _ => self.log(&format!("{context}: cipher_suite=1")),
        }
    }

    /// P8-6：连接结束（或同步完成）后把该信道的帧统计累加进引擎诊断
    /// （帧直方图 + 载荷/线上字节累计——overheadRatio 的来源）。
    fn absorb_padding(&self, ch: &SecureChannel) {
        {
            let mut h = self
                .inner
                .frame_hist
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let (a, b, c, d) = ch.frame_histogram();
            h[0] += a;
            h[1] += b;
            h[2] += c;
            h[3] += d;
        }
        let (p, w) = ch.byte_totals();
        self.inner.bytes_payload.fetch_add(p, Ordering::SeqCst);
        self.inner.bytes_wire.fetch_add(w, Ordering::SeqCst);
    }

    /// 拨号 + 握手（配对路径用：同一地址可重拨重试）。返回流与信道供后续业务收发。
    fn dial_handshake(
        addr: &str,
        ident: &Identity,
        psk: Option<&[u8; 32]>,
        offer_pq: bool,
    ) -> Result<(TcpStream, SecureChannel), String> {
        let mut stream = TcpStream::connect(addr).map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| e.to_string())?;
        let ch = SecureChannel::handshake(&mut stream, Role::Initiator, ident, psk, offer_pq)
            .map_err(|e| e.to_string())?;
        Ok((stream, ch))
    }

    /// P7-8：直连地址是否允许提议 suite 2——仅当该地址命中某已配对记录
    /// 且其 Hello 曾声明 pq（对端升级后首次连接即自动 learns，见 `hello`）。
    /// 中继房间与地址无关，一律不提议（T1 灰度范围，偏差见 LOG）。
    fn pq_offer_for_addr(&self, addr: &str) -> bool {
        self.inner
            .peers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .list()
            .iter()
            .any(|(_, r)| r.addr.as_deref() == Some(addr) && r.pq_cap)
    }

    /// 生成一次性邀请码（10 分钟有效）。code 与本机指纹返回给 UI，双方核对指纹防 MITM。
    pub fn pair_begin(&self) -> Result<serde_json::Value, &'static str> {
        let code = hex_encode(&random_bytes(16));
        *self.inner.invite.lock().unwrap_or_else(|e| e.into_inner()) = Some(Invite {
            psk: psk_from_code(&code),
            expires_ms: now_ms() + INVITE_TTL_MS,
        });
        Ok(serde_json::json!({
            "code": code,
            "fingerprint": self.fingerprint(),
            "deviceId": self.device_id(),
            "port": self.port(),
        }))
    }

    /// 主动连接新设备完成配对：PSK 握手证明持有邀请码，握手哈希双向签名绑定身份。
    /// 配对成功同时登记对端监听地址（本次拨出的 `addr` 即可回拨地址）。
    ///
    /// P7-8：配对是唯一「对端能力未知」的乐观探测点——先试 suite 2；对端若是旧版
    /// （不认识扩展帧）其握手侧报错断连，此时换裸帧重试一次（降级在事件日志可见）。
    /// 对端能力随 Hello 记入 peers.pq_cap，之后的直连同步直接按记录协商，不再有失败尝试。
    pub fn pair_join(&self, addr: &str, code: &str) -> Result<serde_json::Value, String> {
        let psk = psk_from_code(code);
        let (mut stream, mut ch) =
            match Self::dial_handshake(addr, &self.inner.ident, Some(&psk), true) {
                Ok((s, ch)) => {
                    self.note_channel(&ch, "pairing");
                    (s, ch)
                }
                Err(e) => {
                    let (s, ch) = Self::dial_handshake(addr, &self.inner.ident, Some(&psk), false)
                        .map_err(|e2| format!("{e}; pq fallback also failed: {e2}"))?;
                    self.log("pairing: peer not pq-aware; downgraded to suite 1");
                    self.note_channel(&ch, "pairing");
                    (s, ch)
                }
            };
        let (peer_id, peer_name, peer_pub, _peer_port, peer_pq) =
            self.hello(&mut ch, &mut stream)?;
        send_json(&mut ch, &mut stream, &Msg::PairReq).map_err(|e| e.to_string())?;
        let resp: Msg = recv_json(&mut ch, &mut stream).map_err(|e| e.to_string())?;
        match resp {
            Msg::PairAccept => {}
            Msg::Error { msg } => return Err(msg),
            _ => return Err("pairing rejected".into()),
        }
        self.inner
            .peers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .upsert(
                &peer_id,
                PeerRec {
                    name: peer_name.clone(),
                    pub_hex: peer_pub.clone(),
                    paired_ms: now_ms(),
                    counter: 0,
                    addr: Some(addr.to_string()),
                    pq_cap: peer_pq,
                },
            )
            .map_err(|e| e.to_string())?;
        // P8-4：短码（双端独立计算、人工比对——MITM 防线的可计算判据）
        let short =
            crate::identity::Identity::match_code(&self.inner.ident.public_hex(), &peer_pub);
        let expires = now_ms() + MATCH_CODE_TTL_MS;
        if let Some(c) = &short {
            *self
                .inner
                .match_code
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Some((c.clone(), expires));
            self.log(&format!("pairing: match code {c} (compare with peer UI)"));
        }
        self.absorb_padding(&ch);
        Ok(serde_json::json!({
            "peerId": peer_id, "name": peer_name,
            "shortCode": short, "matchCodeExpiresMs": expires,
        }))
    }

    /// 解除与某已配对设备的配对（从对端登记表移除）；返回是否曾存在该设备（幂等）。
    pub fn unpair(&self, device_id: &str) -> Result<bool, &'static str> {
        let removed = {
            let mut peers = self.inner.peers.lock().unwrap_or_else(|e| e.into_inner());
            peers.remove(device_id)?
        };
        if removed {
            self.log(&format!("unpair: removed {device_id}"));
        }
        Ok(removed)
    }

    /// Hello 交换 + 身份核验：签名覆盖 hh‖x25519；信道静态密钥 = 声明的 X25519 公钥；
    /// device_id 派生自声明的 Ed25519 公钥。返回值末两位为对端声明的监听端口
    /// （0 = 旧端未声明）与混合 KEM 能力（false = 旧端或初始化失败）。
    /// 对端 pq 声明经认证信道到达后即回写 peers.pq_cap（对端升级后的自愈点）。
    fn hello<S: std::io::Read + std::io::Write>(
        &self,
        ch: &mut SecureChannel,
        stream: &mut S,
    ) -> Result<(String, String, String, u16, bool), &'static str> {
        let i = self.hello_ext(ch, stream, &[])?;
        Ok((i.device_id, i.name, i.pub_hex, i.port, i.pq))
    }

    /// P8-4：Hello 扩展形态——额外携带本机候选（`ip:port`），并返回对端候选。
    /// 旧端不声明该字段 → 空列表 → 本次会话不做打洞尝试（如实降级，不猜地址）。
    fn hello_ext<S: std::io::Read + std::io::Write>(
        &self,
        ch: &mut SecureChannel,
        stream: &mut S,
        candidates: &[String],
    ) -> Result<HelloInfo, &'static str> {
        let hh = *ch.handshake_hash();
        let x25519_hex = hex_encode(&self.inner.ident.x25519_pub());
        let sig = self.inner.ident.sign(&hello_sign_body(&hh, &x25519_hex));
        // 本端 pq 能力现算（每次握手 ~0.1ms 级派生，不缓存，fail-open 天然新鲜）
        let pq_local = vault_crypto::pq::mlkem768_sk_from_seed(&self.inner.ident.seed()).is_some();
        // P8-6：协议版本与填充档位声明（05-04 §3.3；字段不在签名体内，同 port 口径）
        const PROTO_VER: u16 = 2;
        const PAD_TIER_MAX: u8 = 4;
        send_json(
            ch,
            stream,
            &Msg::Hello {
                device_id: self.device_id(),
                name: self.inner.device_name.clone(),
                pub_hex: self.inner.ident.public_hex(),
                x25519_hex,
                sig,
                port: self.port(),
                pq: pq_local,
                proto_ver: PROTO_VER,
                pad_tier_min: 0,
                pad_tier_max: PAD_TIER_MAX,
                candidates: candidates.to_vec(),
            },
        )?;
        let resp: Msg = recv_json(ch, stream)?;
        let (
            device_id,
            name,
            pub_hex,
            x25519_hex,
            sig,
            port,
            pq_peer,
            peer_proto,
            peer_pad_min,
            peer_pad_max,
            peer_candidates,
        ) = match resp {
            Msg::Hello {
                device_id,
                name,
                pub_hex,
                x25519_hex,
                sig,
                port,
                pq,
                proto_ver,
                pad_tier_min,
                pad_tier_max,
                candidates,
            } => (
                device_id,
                name,
                pub_hex,
                x25519_hex,
                sig,
                port,
                pq,
                proto_ver,
                pad_tier_min,
                pad_tier_max,
                candidates,
            ),
            Msg::Error { msg } => return Err(leak_str(&msg)),
            _ => return Err("expected hello"),
        };
        if !Identity::verify(&pub_hex, &hello_sign_body(&hh, &x25519_hex), &sig) {
            return Err("hello signature invalid");
        }
        if !ch.remote_is(&x25519_hex) {
            return Err("channel key does not match claimed identity");
        }
        if Identity::device_id_of(&pub_hex).as_deref() != Some(device_id.as_str()) {
            return Err("device id mismatch");
        }
        // P8-6：填充协商（05-04 §3.3）。旧端（proto_ver < 2 或字段未声明）→ 档 0，
        // 恒旧帧格式（不得在未协商时填充）；下限违背取低档不中断同步，降级可见。
        let effective = if peer_proto >= 2 && peer_pad_max > 0 {
            vault_net::frame::negotiate(PAD_TIER_MAX, peer_pad_max)
        } else {
            0
        };
        ch.set_padding(effective);
        self.inner.pad_tier.store(effective, Ordering::SeqCst);
        self.inner
            .pad_legacy
            .store(effective == 0, Ordering::SeqCst);
        let (h1, h2, h3, h4) = ch.frame_histogram();
        *self
            .inner
            .frame_hist
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = [h1, h2, h3, h4];
        if effective == 0 {
            self.log("padding: tier=0 (legacy peer or undeclared); legacy framing");
        } else if vault_net::frame::floor_breached(0, peer_pad_min, effective) {
            self.log(&format!(
                "padding: tier={effective} (floor breached; PATH_DEGRADED padding_downgrade)"
            ));
        } else {
            self.log(&format!("padding: tier={effective} negotiated"));
        }
        // 能力自愈：对端升级（false→true）或能力丢失（true→false）都如实回写。
        {
            let mut peers = self.inner.peers.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((_, rec)) = peers.by_pub(&pub_hex) {
                if rec.pq_cap != pq_peer {
                    let mut updated = rec;
                    updated.pq_cap = pq_peer;
                    let _ = peers.upsert(&device_id, updated);
                }
            }
        }
        Ok(HelloInfo {
            device_id,
            name,
            pub_hex,
            port,
            pq: pq_peer,
            candidates: peer_candidates,
        })
    }

    fn listener_loop(&self, listener: TcpListener) {
        loop {
            if self.inner.dead.load(Ordering::SeqCst) {
                return;
            }
            {
                let armed = self.inner.armed.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(a) = armed.as_ref() {
                    if Instant::now() >= a.deadline {
                        drop(armed);
                        self.execute_wipe();
                        return;
                    }
                }
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    let eng = self.clone();
                    // accept 出的连接继承非阻塞标志，连接处理线程需要阻塞 IO
                    if stream.set_nonblocking(false).is_err() {
                        continue;
                    }
                    let _ = std::thread::Builder::new()
                        .name("vsync-conn".into())
                        .spawn(move || eng.handle_conn(Box::new(stream), false));
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(_) => return,
            }
        }
    }

    fn handle_conn(&self, mut stream: Box<dyn Duplex>, relayed: bool) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(120)));
        // 投递/等待 ACK 期间提前到达的业务消息暂存区（本连接的其余读取从这里取）
        let mut early = EarlyMsgs::default();
        // 对端拨入时的**监听**地址：IP 取自 TCP 源地址，端口取自 Hello 声明（旧端未声明时为 0，
        // 回退用观测到的源端口——那只在"对端同样从该端口监听"时才对，属尽力而为）。
        // 经中继接入时源地址是中继自身，绝不能当成对端地址回拨，故 relayed 一律不记。
        let observed = stream.peer_addr().ok();
        let pending_psk: Option<[u8; 32]> = self
            .inner
            .invite
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .filter(|i| i.expires_ms > now_ms())
            .map(|i| i.psk);
        let is_pairing = pending_psk.is_some();
        // P8-4：拨入侧同样做打洞准备（两端都发候选、两端都尝试，才可能双方都打通）
        let prepared = self.prepare_punch(&stream.observe_addrs());
        let local_cands: &[String] = prepared
            .as_ref()
            .map(|p| p.candidates.as_slice())
            .unwrap_or(&[]);
        let mut ch = match SecureChannel::handshake(
            &mut stream,
            Role::Responder,
            &self.inner.ident,
            pending_psk.as_ref(),
            true,
        ) {
            Ok(c) => c,
            Err(e) => {
                self.log(&format!("handshake failed: {e}"));
                return;
            }
        };
        self.note_channel(&ch, "incoming");
        if is_pairing {
            // 配对成功即消费邀请码（一次性）
            *self.inner.invite.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
        let hi = match self.hello_ext(&mut ch, &mut stream, local_cands) {
            Ok(h) => h,
            Err(e) => {
                self.log(&format!("hello failed: {e}"));
                return;
            }
        };
        let (peer_id, peer_name, peer_pub, peer_port, peer_pq) = (
            hi.device_id.clone(),
            hi.name.clone(),
            hi.pub_hex.clone(),
            hi.port,
            hi.pq,
        );
        // P8-5：路径记账——对端已识别即计入（拨入侧同样成立）
        self.note_path_attempt(&peer_id, relayed);
        // P8-4：后台打洞尝试（两端各自执行；结果只作路径证据，数据仍走本承载）
        if let Some(p) = prepared {
            self.spawn_punch(&peer_id, &hi.candidates, p);
        }
        if is_pairing {
            let mut peers = self.inner.peers.lock().unwrap_or_else(|e| e.into_inner());
            // 本次观测不到可回拨地址时，保留旧记录里的地址（重新配对不该丢掉已知地址）
            let addr = dialable_addr(observed, peer_port)
                .or_else(|| peers.get(&peer_id).and_then(|p| p.addr));
            let _ = peers.upsert(
                &peer_id,
                PeerRec {
                    name: peer_name.clone(),
                    pub_hex: peer_pub.clone(),
                    paired_ms: now_ms(),
                    counter: 0,
                    addr,
                    pq_cap: peer_pq,
                },
            );
            // P8-4：短码（响应端独立计算，双端比对一致 = 公钥一致）
            let short =
                crate::identity::Identity::match_code(&self.inner.ident.public_hex(), &peer_pub);
            if let Some(c) = &short {
                *self
                    .inner
                    .match_code
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) =
                    Some((c.clone(), now_ms() + MATCH_CODE_TTL_MS));
                self.log(&format!("pairing: match code {c} (compare with peer UI)"));
            }
            self.log(&format!("paired with {peer_id} ({peer_name})"));
        } else {
            let registered = self
                .inner
                .peers
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .by_pub(&peer_pub)
                .is_some();
            if !registered {
                let _ = send_json(
                    &mut ch,
                    &mut stream,
                    &Msg::Error {
                        msg: "unregistered device".into(),
                    },
                );
                self.log("rejected unregistered device");
                return;
            }
            // 高优先级信令：已登记对端拨入后、进入业务消息循环之前先投递本机队列中发给它的
            // 销毁指令（docs/08 §四.2）。未登记设备在上方已断开，不投递。
            if let Err(e) =
                self.deliver_pending_orders(&mut ch, &mut stream, &peer_id, &peer_pub, &mut early)
            {
                self.log(&format!("order delivery aborted: {e}"));
                return;
            }
        }
        if !relayed {
            // 记录可回拨地址（离线销毁指令投递的前提，docs/05-06 §5.1）。
            // 位置在登记/核验之后：`PeerStore::set_addr` 对未登记对端一律拒绝，
            // 不会因为"看到过某个源地址"就凭空建条目。
            if let Some(addr) = dialable_addr(observed, peer_port) {
                self.remember_peer_addr(&peer_id, &addr).ok();
            }
        }

        loop {
            let msg: Msg = match early.pop() {
                Some(m) => m,
                None => match recv_json(&mut ch, &mut stream) {
                    Ok(m) => m,
                    Err(_) => return,
                },
            };
            match msg {
                Msg::PairReq => {
                    if send_json(&mut ch, &mut stream, &Msg::PairAccept).is_err() {
                        return;
                    }
                }
                // P8-9 阅后即焚：一次性会话的报价与载荷（握手已凭 burn PSK 认证）
                Msg::BurnOffer {
                    burn_id,
                    name,
                    size,
                    sha,
                    total_chunks,
                    open_timeout_secs,
                } => {
                    if let Err(e) = self.burn_receive_session(
                        &mut ch,
                        &mut stream,
                        BurnOfferRec {
                            burn_id,
                            name,
                            size,
                            sha,
                            total_chunks,
                            open_timeout_secs,
                        },
                    ) {
                        self.log(&format!("burn receive failed: {e}"));
                        let _ = send_json(&mut ch, &mut stream, &Msg::Error { msg: e });
                        return;
                    }
                }
                Msg::SyncReq { .. } => {
                    if let Err(e) = self.respond_sync(&mut ch, &mut stream, &peer_id) {
                        self.log(&format!("sync responder failed: {e}"));
                        let _ = send_json(&mut ch, &mut stream, &Msg::Error { msg: e });
                        return;
                    }
                }
                Msg::SendFileBegin {
                    item,
                    fskey_wrapped,
                    overwrite,
                    base_rev,
                    ..
                } => {
                    if self
                        .receive_push_rest(
                            &mut ch,
                            &mut stream,
                            item,
                            fskey_wrapped,
                            overwrite,
                            base_rev,
                        )
                        .is_err()
                    {
                        return;
                    }
                }
                Msg::PullFile { id } => {
                    let item = match self.manifest_item(id) {
                        Ok(i) => i,
                        Err(e) => {
                            let _ = send_json(&mut ch, &mut stream, &Msg::Error { msg: e });
                            return;
                        }
                    };
                    if self
                        .respond_push(&mut ch, &mut stream, &item, &peer_id)
                        .is_err()
                    {
                        return;
                    }
                }
                Msg::DeleteOrder { id, sig } => {
                    if !Identity::verify(&peer_pub, &delete_sign_body(id), &sig) {
                        self.log("delete order signature invalid");
                        return;
                    }
                    match self.apply_remote_delete(id) {
                        Ok(note) => self.log(&format!("delete #{id} from peer order: {note}")),
                        Err(e) => self.log(&format!("delete #{id} failed: {e}")),
                    }
                }
                Msg::VcMerge { id, vc } => {
                    let mut slot = self
                        .inner
                        .vault_slot
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    if let Some(v) = slot.as_mut() {
                        let _ = v.merge_vc(id, &vc);
                        let _ = v.save_index_now();
                    }
                }
                Msg::DestroyCmd {
                    target,
                    delay_ms,
                    ts_ms,
                    sig,
                } => {
                    // 高优先级指令：对端公钥验签 + target 必须等于本机 device_id（docs/08 §四.3）
                    match self.apply_order(&peer_pub, &target, delay_ms, ts_ms, &sig) {
                        Ok(true) => {
                            if send_json(&mut ch, &mut stream, &Msg::DestroyAck).is_err() {
                                return;
                            }
                        }
                        Ok(false) => {
                            // 已立即擦除：先把 ACK 送出（对端据此从队列移除），再断开
                            let _ = send_json(&mut ch, &mut stream, &Msg::DestroyAck);
                            return;
                        }
                        Err(OrderReject::TargetMismatch) => {
                            let _ = send_json(
                                &mut ch,
                                &mut stream,
                                &Msg::Error {
                                    msg: "destroy target mismatch".into(),
                                },
                            );
                            return;
                        }
                        Err(_) => return, // 验签失败 / 载荷非法：不执行、不回执、断开
                    }
                }
                Msg::Error { msg } => {
                    self.log(&format!("peer error: {msg}"));
                    return;
                }
                Msg::SyncDone
                | Msg::DestroyAck
                | Msg::FileApplied { .. }
                | Msg::SyncResp { .. }
                | Msg::PairAccept
                | Msg::BlocksNeeded { .. }
                | Msg::BlockData { .. }
                | Msg::BlockEnd { .. }
                | Msg::BurnData { .. }
                | Msg::BurnDone { .. }
                | Msg::BurnAck { .. }
                | Msg::Hello { .. } => return,
            }
        }
    }

    fn manifest_item(&self, id: u64) -> Result<ManifestItem, String> {
        let slot = self
            .inner
            .vault_slot
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let v = slot.as_ref().ok_or("vault locked")?;
        let json = v.sync_manifest().map_err(|e| e.to_string())?;
        let all: Vec<ManifestItem> = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        all.into_iter()
            .find(|i| i.id == id)
            .ok_or(format!("file {id} not found"))
    }

    /// 应用远端删除：误删保护窗口期内先留加密冲突副本（docs/05-03 §6.2 规则 1）。
    fn apply_remote_delete(&self, id: u64) -> Result<String, String> {
        let mut slot = self
            .inner
            .vault_slot
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let v = slot.as_mut().ok_or("vault locked")?;
        if !v.has_file(id) {
            return Ok("already absent".into());
        }
        let modified = v.file_modified(id).map_err(|e| e.to_string())?;
        if modified + PROTECT_WINDOW_MS > now_ms() {
            v.save_conflict_copy(id, BTreeMap::new())
                .map_err(|e| e.to_string())?;
            v.delete_file(id, false).map_err(|e| e.to_string())?;
            return Ok("kept conflict copy (protection window)".into());
        }
        v.delete_file(id, false).map_err(|e| e.to_string())?;
        Ok("deleted".into())
    }

    fn execute_wipe(&self) {
        self.inner.dead.store(true, Ordering::SeqCst);
        let wipe = self
            .inner
            .wipe
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(w) = wipe {
            match w() {
                Ok(()) => self.log("vault wiped (remote destroy)"),
                Err(e) => self.log(&format!("wipe failed: {e}")),
            }
        }
    }

    // ==== 高优先级信令：销毁指令的校验 / 执行 / 投递（docs/05-06 §5.1、docs/08 §四.2）====
    //
    // 执行判定只认「对端长期身份签名 + target == 本机 device_id」，**不认**任何本机队列标记；
    // 队列只是"待投递的字节"，即使队列文件被替换/注入，伪造者也拿不到对端私钥。

    /// 校验并执行一条销毁指令。
    ///
    /// 返回 `Ok(true)` = 已验证并武装延迟擦除（会话可继续）；`Ok(false)` = 已验证并**已立即擦除**
    /// （调用方必须终止会话）；`Err(_)` = 拒绝，且**未**执行任何擦除动作。
    fn apply_order(
        &self,
        peer_pub: &str,
        target: &str,
        delay_ms: u64,
        ts_ms: u64,
        sig: &str,
    ) -> Result<bool, OrderReject> {
        if peer_pub.is_empty()
            || !Identity::verify(peer_pub, &destroy_sign_body(target, delay_ms, ts_ms), sig)
        {
            // 不打印公钥/签名内容，只记录判定结果
            self.log("destroy order rejected: signature invalid");
            return Err(OrderReject::BadSignature);
        }
        if target != self.device_id() {
            self.log("destroy order rejected: target mismatch");
            return Err(OrderReject::TargetMismatch);
        }
        if delay_ms == 0 {
            self.log("destroy order accepted: immediate wipe");
            self.execute_wipe();
            return Ok(false);
        }
        // P7-11：武装落盘（失败 → 拒绝武装，不留纯内存倒计时）
        if let Err(e) = self.persist_armed(target, delay_ms) {
            self.log(&format!("destroy armed rejected: persist failed ({e})"));
            return Err(OrderReject::PersistFailed);
        }
        *self.inner.armed.lock().unwrap_or_else(|e| e.into_inner()) = Some(ArmedDestroy {
            deadline: Instant::now() + Duration::from_millis(delay_ms),
            target: target.to_string(),
        });
        self.log(&format!("destroy order accepted: armed for {delay_ms}ms"));
        Ok(true)
    }

    /// 投递帧路径：载荷为 `Msg::DestroyCmd` 的 JSON 原文（队列条目 `payload` 即此格式）。
    /// 与线上路径共用 `apply_order`，保证"收到的字节"与"验签/执行所用字段"同源。
    pub fn apply_order_payload(&self, peer_pub: &str, payload: &str) -> Result<bool, OrderReject> {
        // P7-4：轮换传播（{"RotationNotice":{...}}）优先分派
        if payload.contains("\"RotationNotice\"") {
            let v: serde_json::Value =
                serde_json::from_str(payload).map_err(|_| OrderReject::Malformed)?;
            let n = v.get("RotationNotice").ok_or(OrderReject::Malformed)?;
            let rotation_id = n["rotationId"].as_str().ok_or(OrderReject::Malformed)?;
            let blob_hex = n["ct"].as_str().ok_or(OrderReject::Malformed)?;
            let issued = n["issuedMs"].as_u64().ok_or(OrderReject::Malformed)?;
            let expires = n["expiresMs"].as_u64().ok_or(OrderReject::Malformed)?;
            let sig = n["sig"].as_str().ok_or(OrderReject::Malformed)?;
            self.apply_rotation_notice(rotation_id, blob_hex, issued, expires, sig, peer_pub)?;
            return Ok(false);
        }
        // P7-10：锁定指令（{"LockCmd":{...}}）优先分派
        if payload.contains("\"LockCmd\"") {
            let v: serde_json::Value =
                serde_json::from_str(payload).map_err(|_| OrderReject::Malformed)?;
            let cmd = v.get("LockCmd").ok_or(OrderReject::Malformed)?;
            let target = cmd["target"].as_str().ok_or(OrderReject::Malformed)?;
            let ts_ms = cmd["ts_ms"].as_u64().ok_or(OrderReject::Malformed)?;
            let sig = cmd["sig"].as_str().ok_or(OrderReject::Malformed)?;
            self.apply_lock_order(target, ts_ms, sig, peer_pub)?;
            return Ok(false); // 锁定即时生效，无延迟武装
        }
        match serde_json::from_str::<Msg>(payload) {
            Ok(Msg::DestroyCmd {
                target,
                delay_ms,
                ts_ms,
                sig,
            }) => self.apply_order(peer_pub, &target, delay_ms, ts_ms, &sig),
            _ => {
                self.log("destroy order rejected: malformed payload");
                Err(OrderReject::Malformed)
            }
        }
    }

    /// 读取下一条消息（先取暂存区）；期间收到的销毁指令（对端上线后投递的高优先级信令）
    /// 就地处理并回 ACK。`Err` = 会话必须中止（本机已擦除 / 指令非法 / 读失败）。
    fn recv_with_orders(
        &self,
        ch: &mut SecureChannel,
        stream: &mut Box<dyn Duplex>,
        peer_pub: &str,
        early: &mut EarlyMsgs,
    ) -> Result<Msg, String> {
        loop {
            let msg: Msg = match early.pop() {
                Some(m) => m,
                None => recv_json(ch, stream).map_err(|e| e.to_string())?,
            };
            match msg {
                Msg::DestroyCmd {
                    target,
                    delay_ms,
                    ts_ms,
                    sig,
                } => match self.apply_order(peer_pub, &target, delay_ms, ts_ms, &sig) {
                    Ok(true) => {
                        send_json(ch, stream, &Msg::DestroyAck).map_err(|e| e.to_string())?;
                    }
                    Ok(false) => {
                        let _ = send_json(ch, stream, &Msg::DestroyAck);
                        return Err("local wipe executed by remote order".into());
                    }
                    Err(OrderReject::TargetMismatch) => {
                        let _ = send_json(
                            ch,
                            stream,
                            &Msg::Error {
                                msg: "destroy target mismatch".into(),
                            },
                        );
                        return Err("incoming destroy order target mismatch".into());
                    }
                    Err(e) => return Err(format!("incoming destroy order rejected: {e:?}")),
                },
                other => return Ok(other),
            }
        }
    }

    /// 等待本机所发销毁指令的 ACK；期间若收到对端投递的指令（双向同时上线）按同一路径处理
    /// 并回 ACK，收到业务消息则暂存到 `early`（发起端 SyncReq 可能早于 ACK 到达）。
    /// 只从信道读、不从 `early` 取：投递发生在本连接的第一次读取之前，此时 `early` 必为空。
    /// `Err` = 会话必须中止。
    fn await_order_ack(
        &self,
        ch: &mut SecureChannel,
        stream: &mut Box<dyn Duplex>,
        peer_pub: &str,
        early: &mut EarlyMsgs,
    ) -> Result<AckOutcome, String> {
        loop {
            let msg: Msg = recv_json(ch, stream).map_err(|e| e.to_string())?;
            match msg {
                Msg::DestroyAck => return Ok(AckOutcome::Ack),
                Msg::Error { msg } => return Ok(AckOutcome::Rejected(msg)),
                Msg::DestroyCmd {
                    target,
                    delay_ms,
                    ts_ms,
                    sig,
                } => match self.apply_order(peer_pub, &target, delay_ms, ts_ms, &sig) {
                    Ok(true) => {
                        send_json(ch, stream, &Msg::DestroyAck).map_err(|e| e.to_string())?;
                    }
                    Ok(false) => {
                        let _ = send_json(ch, stream, &Msg::DestroyAck);
                        return Err("local wipe executed by remote order".into());
                    }
                    Err(OrderReject::TargetMismatch) => {
                        let _ = send_json(
                            ch,
                            stream,
                            &Msg::Error {
                                msg: "destroy target mismatch".into(),
                            },
                        );
                        return Err("incoming destroy order target mismatch".into());
                    }
                    Err(e) => return Err(format!("incoming destroy order rejected: {e:?}")),
                },
                other => early.push(other)?,
            }
        }
    }

    /// 投递本机队列中发给该对端的全部销毁指令。
    ///
    /// 时机固定：**信道建立（Hello 核验）之后、清单交换之前**（docs/08 §四.2「先于一切业务消息」）。
    /// 无队列条目时**不做任何 I/O**，正常同步路径零额外往返。
    /// 收到 ACK 才从队列移除；对端拒收或链路中断则保留，等下次上线重投（最终一致语义）。
    /// `Err` = 会话必须中止（本机已擦除 / 对端投递来非法指令 / 链路已断）。
    fn deliver_pending_orders(
        &self,
        ch: &mut SecureChannel,
        stream: &mut Box<dyn Duplex>,
        peer_id: &str,
        peer_pub: &str,
        early: &mut EarlyMsgs,
    ) -> Result<usize, String> {
        let orders = self
            .inner
            .orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .list_for(peer_id);
        let mut delivered = 0usize;
        for o in orders {
            // 投递队列里保存的帧原文（不重新序列化：投递字节 = 签名覆盖的字节）
            if ch.send_msg(stream, o.payload.as_bytes()).is_err() {
                self.log(&format!(
                    "destroy order send failed for {peer_id}: kept in queue"
                ));
                break;
            }
            match self.await_order_ack(ch, stream, peer_pub, early)? {
                AckOutcome::Ack => {
                    let _ = self
                        .inner
                        .orders
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&o.peer_id, &o.target, o.issued_ms);
                    delivered += 1;
                    self.log(&format!(
                        "destroy order delivered to {peer_id} (delay {}s)",
                        o.delay_secs
                    ));
                }
                AckOutcome::Rejected(msg) => {
                    self.log(&format!(
                        "destroy order rejected by {peer_id}: {msg} (kept in queue)"
                    ));
                    break;
                }
            }
        }
        Ok(delivered)
    }

    /// 记录对端已知监听地址（UI 兜底手填；未登记对端返回 Err）。日志不写地址本身（元数据最小化）。
    pub fn remember_peer_addr(&self, device_id: &str, addr: &str) -> Result<(), &'static str> {
        self.inner
            .peers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .set_addr(device_id, addr)?;
        self.log(&format!("peer addr remembered: {device_id}"));
        Ok(())
    }

    /// 全设备联动销毁的第一步：为**每一个已配对设备**各签发一条销毁指令并入队（离线设备下次
    /// 上线后投递执行）。签名用本机长期身份，私钥不出引擎；立即返回，不尝试连接对端。
    pub fn destroy_queue_all(&self, delay_secs: u64) -> Result<serde_json::Value, String> {
        if self.inner.dead.load(Ordering::SeqCst) {
            return Err("engine destroyed".into());
        }
        let peers = self
            .inner
            .peers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .list();
        let delay_ms = delay_secs.saturating_mul(1000);
        let mut queued: Vec<String> = Vec::with_capacity(peers.len());
        {
            let mut store = self.inner.orders.lock().unwrap_or_else(|e| e.into_inner());
            for (peer_id, _rec) in peers.iter() {
                let ts = now_ms();
                let sig = self
                    .inner
                    .ident
                    .sign(&destroy_sign_body(peer_id, delay_ms, ts));
                let payload = serde_json::to_string(&Msg::DestroyCmd {
                    target: peer_id.clone(),
                    delay_ms,
                    ts_ms: ts,
                    sig: sig.clone(),
                })
                .map_err(|e| e.to_string())?;
                store
                    .push(OrderRec {
                        kind: 1,
                        peer_id: peer_id.clone(),
                        target: peer_id.clone(),
                        delay_secs,
                        issued_ms: ts,
                        sig_hex: sig,
                        payload,
                    })
                    .map_err(|e| e.to_string())?;
                queued.push(peer_id.clone());
            }
        }
        self.log(&format!(
            "destroy orders queued: {} peer(s), delay {delay_secs}s",
            queued.len()
        ));
        Ok(serde_json::json!({"queued": queued.len(), "peers": queued}))
    }

    /// 队列快照供 UI 展示（不含签名/载荷，避免把可用凭据暴露给展示层）。
    pub fn pending_orders(&self) -> serde_json::Value {
        let store = self.inner.orders.lock().unwrap_or_else(|e| e.into_inner());
        let items: Vec<serde_json::Value> = store
            .all()
            .iter()
            .map(|o| {
                serde_json::json!({
                    "peer": o.peer_id,
                    "issuedMs": o.issued_ms,
                    "delaySecs": o.delay_secs,
                })
            })
            .collect();
        serde_json::json!({"count": items.len(), "items": items})
    }

    /// 取消投递（清空本机队列）；返回被清掉的条数。已投递执行的指令不受影响（不可撤回）。
    pub fn cancel_pending_orders(&self) -> usize {
        let n = self
            .inner
            .orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear()
            .unwrap_or(0);
        if n > 0 {
            self.log(&format!("destroy orders cancelled: {n}"));
        }
        n
    }

    /// 发起一次增量同步（直连地址）。P7-8：按地址命中的对端记录提议 suite 2。
    pub fn sync_with(&self, addr: &str) -> Result<serde_json::Value, String> {
        let offer = self.pq_offer_for_addr(addr);
        let stream: Box<dyn Duplex> = TcpStream::connect(addr)
            .map_err(|e| e.to_string())
            .map(Box::new)?;
        self.run_sync(stream, offer, SessionPath::direct())
    }

    /// 经中继同步（P8-5 VSR2）：token 挑战-应答接入同房间两端后照常 E2E 握手
    /// （docs/08 §三）。P7-8 偏差（T1 灰度）：房间映射不到对端记录，恒以
    /// suite 1 握手（见 LOG）。
    /// 失败语义（docs/08 §3.4）：接入期失败返回结构化 `RelayFailure`——R 码已
    /// 映射为本机码 + 可读诊断 + 降级原因；接入成功后的会话失败同样收敛到
    /// `RelayFailure`（reason=session，本机码 3）。
    pub fn sync_via_relay(
        &self,
        relay_addr: &str,
        room: &str,
        token: &str,
    ) -> Result<serde_json::Value, RelayFailure> {
        let stream = match relay_client::connect(relay_addr, room, token) {
            Ok(s) => s,
            Err(f) => {
                // 拨号期失败无法归属对端：记入引擎事件日志 + 未归属路径失败
                self.log(&f.diag);
                *self
                    .inner
                    .last_path_failure
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = Some((f.reason.clone(), now_ms()));
                return Err(f);
            }
        };
        let wires = stream.wire_arcs();
        self.run_sync(Box::new(stream), false, SessionPath::relay(wires))
            .map_err(|e| RelayFailure {
                relay_code: 0,
                reason: "session".into(),
                local_code: 3,
                degrade_reason: None,
                diag: e,
            })
    }

    /// 响应端经中继接入：VSR2 鉴权后进入既有连接处理流程（握手/核验/同步/销毁）。
    pub fn serve_relay_connection(
        &self,
        relay_addr: &str,
        room: &str,
        token: &str,
    ) -> Result<(), RelayFailure> {
        let stream = relay_client::connect(relay_addr, room, token)?;
        // relayed = true：源地址是中继的，不当作对端可回拨地址登记
        self.handle_conn(Box::new(stream), true);
        Ok(())
    }

    /// 多中继候选顺序尝试（P8-5 收尾，docs/08 §3.6 / §五）：按用户**显式配置的
    /// 顺序**逐个尝试；某中继失败（含鉴权失败）→ 只尝试下一个，绝不静默回退
    /// 到未配置的路径。列表为空 → `13 + relay_unavailable`，**不发起任何连接**
    /// （`allowPublicRelay = false` 过滤后为空即此形态——连握手都不发生）。
    pub fn sync_via_relay_candidates(
        &self,
        candidates: &[(String, String)],
        room: &str,
    ) -> Result<serde_json::Value, RelayFailure> {
        let mut last = empty_candidates_failure();
        for (addr, token) in candidates {
            match self.sync_via_relay(addr, room, token) {
                Ok(v) => return Ok(v),
                Err(f) => last = f,
            }
        }
        // 全部失败（含空候选）：记入未归属路径失败（与单中继拨号失败同口径，
        // 供 path_status.lastFailure 呈现「为什么慢 / 为什么不可用」）
        *self
            .inner
            .last_path_failure
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some((last.reason.clone(), now_ms()));
        Err(last)
    }

    /// 响应端的多中继候选顺序尝试（语义同 [`Self::sync_via_relay_candidates`]）。
    pub fn serve_relay_candidates(
        &self,
        candidates: &[(String, String)],
        room: &str,
    ) -> Result<(), RelayFailure> {
        let mut last = empty_candidates_failure();
        for (addr, token) in candidates {
            match self.serve_relay_connection(addr, room, token) {
                Ok(()) => return Ok(()),
                Err(f) => last = f,
            }
        }
        *self
            .inner
            .last_path_failure
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some((last.reason.clone(), now_ms()));
        Err(last)
    }

    fn run_sync(
        &self,
        mut stream: Box<dyn Duplex>,
        offer_pq: bool,
        path: SessionPath,
    ) -> Result<serde_json::Value, String> {
        if self.inner.dead.load(Ordering::SeqCst) {
            return Err("engine destroyed".into());
        }
        stream
            .set_read_timeout(Some(Duration::from_secs(120)))
            .map_err(|e| e.to_string())?;
        // P8-4：打洞准备（仅当承载方给了观测端点时才付出代价——直连路径恒 None）
        let prepared = self.prepare_punch(&stream.observe_addrs());
        let local_cands: &[String] = prepared
            .as_ref()
            .map(|p| p.candidates.as_slice())
            .unwrap_or(&[]);
        let mut ch = SecureChannel::handshake(
            &mut stream,
            Role::Initiator,
            &self.inner.ident,
            None,
            offer_pq,
        )
        .map_err(|e| e.to_string())?;
        self.note_channel(&ch, "sync");
        let hi = self
            .hello_ext(&mut ch, &mut stream, local_cands)
            .map_err(|e| e.to_string())?;
        let (peer_id, peer_pub) = (hi.device_id.clone(), hi.pub_hex.clone());
        let registered = self
            .inner
            .peers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .by_pub(&peer_pub)
            .is_some();
        if !registered {
            return Err("peer not paired with this vault".into());
        }

        // P8-5：对端已识别即计入路径尝试（拨号期失败无法归属，不入 per-peer 统计）
        self.note_path_attempt(&peer_id, path.relayed);
        // P8-4：后台打洞尝试（不阻塞本次同步；结果只作路径证据，数据仍走本承载）
        if let Some(p) = prepared {
            self.spawn_punch(&peer_id, &hi.candidates, p);
        }
        self.reset_skipped();

        let mut session = || -> Result<serde_json::Value, String> {
            // 高优先级信令：握手完成、清单交换之前先把本机队列中发给该对端的销毁指令投递掉
            //（docs/08 §四.2）。对端同样会在其 Hello 后投递，故用能顺带处理来件的读取路径。
            let mut early = EarlyMsgs::default();
            self.deliver_pending_orders(&mut ch, &mut stream, &peer_id, &peer_pub, &mut early)?;

            let local_items: Vec<ManifestItem> = {
                let slot = self
                    .inner
                    .vault_slot
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let v = slot.as_ref().ok_or("vault locked")?;
                let json = v.sync_manifest().map_err(|e| e.to_string())?;
                serde_json::from_str(&json).map_err(|e| e.to_string())?
            };
            send_json(
                &mut ch,
                &mut stream,
                &Msg::SyncReq {
                    manifest: local_items.clone(),
                },
            )
            .map_err(|e| e.to_string())?;
            let resp: Msg = self.recv_with_orders(&mut ch, &mut stream, &peer_pub, &mut early)?;
            let peer_items = match resp {
                Msg::SyncResp { manifest } => manifest,
                Msg::Error { msg } => return Err(msg),
                _ => return Err("expected syncresp".into()),
            };

            let summary =
                self.execute_plan(&mut ch, &mut stream, &local_items, &peer_items, &peer_id)?;
            let _ = send_json(&mut ch, &mut stream, &Msg::SyncDone);
            Ok(summary)
        };
        match session() {
            Ok(summary) => {
                self.absorb_padding(&ch);
                self.note_path_finish(Some(&peer_id), &path, None);
                let cnt = |k: &str| summary[k].as_array().map(Vec::len).unwrap_or(0);
                self.log(&format!(
                    "sync with {peer_id}: pushed {} pulled {} deleted {} conflicts {} merged {}",
                    cnt("pushed"),
                    cnt("pulled"),
                    cnt("deleted"),
                    cnt("conflicts"),
                    cnt("merged"),
                ));
                Ok(summary)
            }
            Err(e) => {
                self.note_path_finish(Some(&peer_id), &path, Some(&e));
                Err(e)
            }
        }
    }

    // ==== P8-4：打洞（候选经 Hello 交换 + 中继观测端点 + 双向验证）====

    /// P8-4：是否尝试打洞（FFI / 测试可关；默认开）。
    pub fn set_punch_enabled(&self, on: bool) {
        self.inner.punch_enabled.store(on, Ordering::SeqCst);
    }

    fn punch_enabled(&self) -> bool {
        self.inner.punch_enabled.load(Ordering::SeqCst)
    }

    /// P8-4：打洞准备——绑定专用探测 socket、做映射观测、判定对称 NAT。
    /// **只在承载方提供了观测端点且打洞开启时付出代价**（否则直接 None，零开销）。
    /// 候选构成：观测到的公网映射在前（跨网段唯一有用的那个），其后是网卡地址；
    /// 总数受 [`MAX_CANDIDATES`] 约束（05-03 §3.2 每端上限 8）。
    fn prepare_punch(&self, observe: &[SocketAddr]) -> Option<PreparedPunch> {
        if !self.punch_enabled() || observe.is_empty() {
            return None;
        }
        let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
        let port = sock.local_addr().ok()?.port();
        let ips = vault_net::discovery::local_candidates_now();
        let lan_count = ips
            .iter()
            .filter(|ip| {
                vault_net::discovery::classify_ip(**ip) == vault_net::discovery::AddrClass::Lan
            })
            .count() as u32;
        let mut candidates: Vec<SocketAddr> = Vec::new();
        let mut mapped = None;
        let mut observe_reason = None;
        // 观测 #1：中继看到的本机映射（打洞最关键的候选）
        match vault_net::punch::observe(&sock, observe[0], OBSERVE_TIMEOUT) {
            Ok(m) => {
                mapped = Some(m);
                candidates.push(m);
            }
            Err(_) => observe_reason = Some("observe_failed"),
        }
        // 观测 #2（可选）：同一 socket 对另一目标 —— 端口不同即对称 NAT（前置判定）
        let mut symmetric = false;
        if let (Some(o2), Some(m1)) = (observe.get(1), mapped) {
            if let Ok(m2) = vault_net::punch::observe(&sock, *o2, OBSERVE_TIMEOUT) {
                symmetric = m1.port() != m2.port();
            }
        }
        for ip in ips {
            if candidates.len() >= MAX_CANDIDATES {
                break;
            }
            candidates.push(SocketAddr::new(ip, port));
        }
        candidates.truncate(MAX_CANDIDATES);
        Some(PreparedPunch {
            sock,
            candidates: candidates.iter().map(|a| a.to_string()).collect(),
            mapped,
            observe_reason,
            symmetric,
            lan_count,
        })
    }

    /// P8-4：打洞**同步核心**——对称前置判定 → 无候选判定 → 双向验证打洞。
    ///
    /// 由两条路共用，保证判定与字段口径**完全一致**（否则「后台自动尝试」与「显式探针」
    /// 会各说各话）：
    /// - [`Self::spawn_punch`]：每次经中继同步后台执行（不阻塞会话）；
    /// - [`Self::punch_probe`]：矩阵用的显式探针（同步等结果）。
    fn run_punch(
        &self,
        prepared: PreparedPunch,
        peer_candidates: &[String],
        timeout: Duration,
    ) -> PunchReport {
        let PreparedPunch {
            sock,
            candidates,
            mapped,
            observe_reason,
            symmetric,
            lan_count,
        } = prepared;
        let remote: Vec<SocketAddr> = peer_candidates
            .iter()
            .filter_map(|s| s.parse::<SocketAddr>().ok())
            .take(MAX_CANDIDATES)
            .collect();
        let local_n = candidates.len() as u32;
        let remote_n = remote.len() as u32;
        let t0 = Instant::now();
        // 路径状态机：目标是 Punched；任一步不成就降级到 Relayed（只降不升）
        let mut machine = vault_net::path::PathMachine::new(vault_net::path::PathKind::Punched);
        let (ok, reason, by_peer) = if symmetric {
            // 05-03 §4.3 步骤 5：对称 NAT 不做无望长尝试（前置判定，毫秒级回落）
            machine.degrade(vault_net::path::DegradeReason::SymmetricNat);
            (false, Some("symmetric_nat"), None)
        } else if remote.is_empty() {
            machine.degrade(vault_net::path::DegradeReason::HolePunchTimeout);
            (false, Some("no_candidates"), None)
        } else {
            match vault_net::punch::punch_verified(&sock, &remote, timeout) {
                Some(o) => (true, None, o.self_mapped_by_peer),
                None => {
                    machine.degrade(vault_net::path::DegradeReason::HolePunchTimeout);
                    (false, Some("hole_punch_timeout"), None)
                }
            }
        };
        // 事后观测 #2 比对（无第二观测端点时的对称 NAT 判据）
        let symmetric_nat =
            symmetric || matches!((mapped, by_peer), (Some(a), Some(b)) if a.port() != b.port());
        let final_reason = if ok { None } else { reason.or(observe_reason) };
        // PathStatus：只给候选**计数**与当前路径（隐私纪律：不给 IP）
        let status = vault_net::path::PathStatus {
            direct_candidates: lan_count,
            punched_candidates: local_n,
            // 中继是配置项而非发现结果 → 候选计数如实为 0
            relay_candidates: 0,
            current: Some(machine.current),
        };
        let _ = status;
        PunchReport {
            ok,
            reason: final_reason,
            symmetric_nat,
            local_candidates: local_n,
            remote_candidates: remote_n,
            kind: machine.current.as_str(),
            degraded_steps: machine.steps(),
            mapped_port: mapped.map(|a| a.port()),
            peer_observed_port: by_peer.map(|a| a.port()),
            elapsed_ms: t0.elapsed().as_millis() as u64,
        }
    }

    /// P8-4：后台执行打洞尝试（**不阻塞会话**：数据仍走本会话承载，打洞结果只作
    /// 路径证据）。成功 = 双向证实；失败 / 跳过一律带 reason 记账，不谎称打通。
    fn spawn_punch(&self, peer_id: &str, peer_candidates: &[String], prepared: PreparedPunch) {
        let me = self.clone();
        let peer_id = peer_id.to_string();
        let cands = peer_candidates.to_vec();
        let _ = std::thread::Builder::new()
            .name("vsync-punch".into())
            .spawn(move || {
                let rep = me.run_punch(prepared, &cands, PUNCH_TIMEOUT);
                me.note_punch(&peer_id, &rep);
            });
    }

    /// P8-4：**单次打洞探针**（矩阵 / 诊断驱动用）。
    ///
    /// 走**真实路径**：VSR2 接入（顺带从 `OK` 行取观测端点）→ Noise 握手 → `hello_ext`
    /// 真实候选交换 → 映射观测 → 打洞（**同步**等结果——探针本就是阻塞语义）。
    ///
    /// **不要求已配对**：不做清单交换、不校验配对、不读写保险箱；只记 `punch` 统计
    /// 与一条事件日志。两端各调一次（一端 `Initiator`、一端 `Responder`），
    /// 两端都拿到结构化结果（便于矩阵逐格抄录）。
    /// `code` = 共享邀请码（两端一致即可，用于 `Noise XXpsk3`）；`None` = 仅靠 XX 静态密钥。
    /// 返回 JSON **只含计数、原因与端口**（无 IP）。
    pub fn punch_probe(
        &self,
        relay_addr: &str,
        room: &str,
        token: &str,
        code: Option<&str>,
        role: ProbeRole,
        timeout_ms: u64,
    ) -> Result<serde_json::Value, RelayFailure> {
        if self.inner.dead.load(Ordering::SeqCst) {
            return Err(RelayFailure::local("engine-destroyed", 3, "引擎已销毁"));
        }
        let stream = relay_client::connect(relay_addr, room, token)?;
        let observe = stream.observe_addrs();
        // 观测 + 候选准备（无观测端点时 prepared = None，下面如实报告不可打洞）
        let prepared = self.prepare_punch(&observe);
        let mut stream: Box<dyn Duplex> = Box::new(stream);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
        let psk = code.map(psk_from_code);
        let mut ch = SecureChannel::handshake(
            &mut stream,
            match role {
                ProbeRole::Initiator => Role::Initiator,
                ProbeRole::Responder => Role::Responder,
            },
            &self.inner.ident,
            psk.as_ref(),
            false,
        )
        .map_err(|e| RelayFailure::local("probe-handshake", 6, format!("探针握手失败：{e}")))?;
        self.note_channel(&ch, "punch-probe");
        let local_cands: Vec<String> = prepared
            .as_ref()
            .map(|p| p.candidates.clone())
            .unwrap_or_default();
        let hi = self
            .hello_ext(&mut ch, &mut stream, &local_cands)
            .map_err(|e| RelayFailure::local("probe-hello", 6, format!("探针 Hello 失败：{e}")))?;
        let Some(prep) = prepared else {
            // 无观测端点（或打洞被关闭）→ 如实返回不可打洞，**不谎报** ok
            let v = serde_json::json!({
                "schema": 1,
                "role": role.as_str(),
                "peerDeviceId": hi.device_id,
                "ok": false,
                "reason": if observe.is_empty() { "no_observe_endpoint" } else { "punch_disabled" },
                "symmetricNat": false,
                "localCandidates": 0,
                "remoteCandidates": hi.candidates.len(),
                "kind": "relayed",
                "degradedSteps": 1,
                "mappedPort": serde_json::Value::Null,
                "peerObservedPort": serde_json::Value::Null,
                "elapsedMs": 0,
            });
            return Ok(v);
        };
        let rep = self.run_punch(prep, &hi.candidates, Duration::from_millis(timeout_ms));
        self.note_punch(&hi.device_id, &rep);
        let mut v = rep.to_json();
        if let Some(obj) = v.as_object_mut() {
            obj.insert("role".into(), serde_json::json!(role.as_str()));
            obj.insert("peerDeviceId".into(), serde_json::json!(hi.device_id));
        }
        Ok(v)
    }

    /// P8-4：打洞结果记账 + 诊断（**不含 IP**：与 `path_status` 隐私纪律一致）。
    fn note_punch(&self, peer: &str, rec: &PunchReport) {
        {
            let mut ps = self
                .inner
                .path_stats
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let st = ps.entry(peer.to_string()).or_default();
            st.punch.attempts += 1;
            st.punch.ok = rec.ok;
            st.punch.reason = rec.reason;
            st.punch.symmetric_nat = rec.symmetric_nat;
            st.punch.local_candidates = rec.local_candidates;
            st.punch.remote_candidates = rec.remote_candidates;
            st.punch.degraded_steps = rec.degraded_steps;
            st.punch.kind = Some(rec.kind);
        }
        self.log(&format!(
            "punch peer={peer} ok={} kind={} reason={} symmetricNat={} localCandidates={} remoteCandidates={}",
            rec.ok,
            rec.kind,
            rec.reason.unwrap_or("none"),
            rec.symmetric_nat,
            rec.local_candidates,
            rec.remote_candidates
        ));
    }

    /// 路径记账（P8-5）：会话建立（对端已识别）即计入尝试与新路径。
    fn note_path_attempt(&self, peer: &str, relayed: bool) {
        let mut ps = self
            .inner
            .path_stats
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let st = ps.entry(peer.to_string()).or_default();
        if relayed {
            st.attempts_relay += 1;
            st.path = Some("relay");
        } else {
            st.attempts_direct += 1;
            st.path = Some("direct");
        }
        st.path_since_ms = now_ms();
    }

    /// 路径记账（P8-5）：会话收尾——中继会话补线上字节与填充档；
    /// 中继会话失败记 last_failure（reason=session_error）。直连失败不入档
    /// （设备离线属常态，不构成路径降级证据）。
    fn note_path_finish(&self, peer: Option<&str>, path: &SessionPath, err: Option<&str>) {
        if !path.relayed {
            return;
        }
        let Some(peer) = peer else {
            return;
        };
        let mut ps = self
            .inner
            .path_stats
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let st = ps.entry(peer.to_string()).or_default();
        if let Some((i, o)) = &path.wires {
            st.bytes_in = i.load(Ordering::SeqCst);
            st.bytes_out = o.load(Ordering::SeqCst);
        }
        st.padding_tier = self.inner.pad_tier.load(Ordering::SeqCst);
        if let Some(_e) = err {
            st.last_failure = Some(("session_error".to_string(), now_ms()));
        }
    }

    /// P8-7：注入选择性同步过滤策略（由 vault-core 从 `sync-policy.enc` 解析后调用）。
    pub fn set_sync_filter(&self, f: SyncFilter) {
        *self
            .inner
            .sync_filter
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = f;
    }

    /// P8-7：注入会话上下文（时段 / 当前网络类型；由外壳判定与上报）。
    pub fn set_sync_context(&self, ctx: SyncContext) {
        *self
            .inner
            .sync_ctx
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = ctx;
    }

    /// P8-7：最近一次会话被跳过的条目快照 `[{id, reason}]`。
    pub fn skipped(&self) -> serde_json::Value {
        let sk = self.inner.skipped.lock().unwrap_or_else(|e| e.into_inner());
        serde_json::json!(sk
            .iter()
            .map(|(id, r)| serde_json::json!({"id": id, "reason": r}))
            .collect::<Vec<_>>())
    }

    // ==== P8-9 阅后即焚（docs/v2.0/05-04 §五）====

    fn burn_dir(&self) -> PathBuf {
        self.inner.data_dir.join("burn")
    }

    /// 展示用票据短码（审计只写它，**不写秘密 / 文件名**）。
    fn short_id(id: u64) -> String {
        format!("{:08x}", id.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32)
    }

    /// 创建一次性票据（发送端）。`ttl_secs = 0` = 不过期；`open_timeout_secs` 默认 60。
    /// 返回 `{burnId, code, room, expiresMs, openTimeoutSecs}`——`code` **仅此一次返回**。
    pub fn burn_create(
        &self,
        file_id: u64,
        ttl_secs: u64,
        open_timeout_secs: u64,
    ) -> Result<serde_json::Value, String> {
        let name = {
            let slot = self
                .inner
                .vault_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let v = slot.as_ref().ok_or("vault locked")?;
            v.file_name(file_id).map_err(|e| e.to_string())?
        };
        let id = self.inner.burn_seq.fetch_add(1, Ordering::SeqCst);
        let t = burn::new_ticket(id, file_id, &name, ttl_secs, open_timeout_secs, now_ms());
        let code = burn::burn_code(&t.secret);
        let room = burn::burn_room(&t.secret);
        let expires = t.expires_ms();
        self.inner
            .burns_local
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                id,
                BurnLocal {
                    ticket: t,
                    phase: BurnPhase::Offering,
                    target: None,
                },
            );
        self.log(&format!(
            "burn create id={} ttl={ttl_secs}s open_timeout={open_timeout_secs}s",
            Self::short_id(id)
        ));
        Ok(serde_json::json!({
            "burnId": id, "code": code, "room": room,
            "expiresMs": expires, "openTimeoutSecs": open_timeout_secs,
        }))
    }

    /// 接收端：登记一次性票据（由邀请码还原秘密）并准备接收。
    /// `relay` = `Some((中继地址, token))` 时另起线程加入派生房间（经中继接收）。
    pub fn burn_receive(
        &self,
        code: &str,
        relay: Option<(&str, &str)>,
    ) -> Result<serde_json::Value, String> {
        let secret = burn::burn_secret_from_code(code).ok_or("invalid burn code")?;
        let psk = burn::burn_psk(&secret);
        let room = burn::burn_room(&secret);
        let expires = now_ms() + INVITE_TTL_MS;
        *self.inner.invite.lock().unwrap_or_else(|e| e.into_inner()) = Some(Invite {
            psk,
            expires_ms: expires,
        });
        *self
            .inner
            .burn_secret_in
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(Zeroizing::new(secret));
        if let Some((relay_addr, token)) = relay {
            let me = self.clone();
            let room2 = room.clone();
            let relay2 = relay_addr.to_string();
            let token2 = token.to_string();
            let _ = std::thread::Builder::new()
                .name("vsync-burn-recv".into())
                .spawn(
                    move || match relay_client::connect(&relay2, &room2, &token2) {
                        Ok(s) => me.handle_conn(Box::new(s), true),
                        Err(f) => me.log(&format!("burn relay receive failed: {}", f.diag)),
                    },
                );
        }
        self.log(&format!("burn joined room={room}（等待报价，TTL 15 min）"));
        Ok(serde_json::json!({"room": room, "expiresMs": expires, "ready": true}))
    }

    /// 发送端：直连投递（`05-04` §5.2）。
    pub fn burn_send_direct(&self, burn_id: u64, addr: &str) -> Result<serde_json::Value, String> {
        let stream: Box<dyn Duplex> = TcpStream::connect(addr)
            .map_err(|e| e.to_string())
            .map(Box::new)?;
        self.burn_send_over(stream, burn_id, addr)
    }

    /// 发送端：经中继投递（房间由票据派生，中继只看到房间 token 与字节数）。
    pub fn burn_send_relay(
        &self,
        burn_id: u64,
        relay_addr: &str,
        token: &str,
    ) -> Result<serde_json::Value, String> {
        let room = {
            let local = self
                .inner
                .burns_local
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let t = local.get(&burn_id).ok_or("unknown burn id")?;
            burn::burn_room(&t.ticket.secret)
        };
        let stream = relay_client::connect(relay_addr, &room, token)
            .map_err(|f| format!("relay: {}", f.diag))?;
        self.burn_send_over(Box::new(stream), burn_id, relay_addr)
    }

    fn burn_send_over(
        &self,
        mut stream: Box<dyn Duplex>,
        burn_id: u64,
        target: &str,
    ) -> Result<serde_json::Value, String> {
        let (psk, name, file_id, open_timeout) = {
            let local = self
                .inner
                .burns_local
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let t = local.get(&burn_id).ok_or("unknown burn id")?;
            if t.ticket.consumed {
                return Err("burn ticket already used (single use)".into());
            }
            (
                burn::burn_psk(&t.ticket.secret),
                t.ticket.name.clone(),
                t.ticket.file_id,
                t.ticket.open_timeout_secs,
            )
        };
        // 导出明文（仅本函数栈内持有；随后经 Noise 信道传输）
        let tmp = self.inner.data_dir.join(format!("burn-send-{burn_id}.tmp"));
        {
            let slot = self
                .inner
                .vault_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let v = slot.as_ref().ok_or("vault locked")?;
            v.export_file(file_id, &tmp).map_err(|e| e.to_string())?;
        }
        let plain = std::fs::read(&tmp).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&tmp);

        stream
            .set_read_timeout(Some(Duration::from_secs(120)))
            .map_err(|e| e.to_string())?;
        let mut ch = SecureChannel::handshake(
            &mut stream,
            Role::Initiator,
            &self.inner.ident,
            Some(&psk),
            false,
        )
        .map_err(|e| e.to_string())?;
        self.note_channel(&ch, "burn");
        let (peer_id, _n, _p, _port, _pq) = self
            .hello(&mut ch, &mut stream)
            .map_err(|e| e.to_string())?;
        let total = plain.len().div_ceil(BURN_CHUNK).max(1);
        send_json(
            &mut ch,
            &mut stream,
            &Msg::BurnOffer {
                burn_id,
                name: name.clone(),
                size: plain.len() as u64,
                sha: vault_crypto::hash_sha256(&plain),
                total_chunks: total,
                open_timeout_secs: open_timeout,
            },
        )
        .map_err(|e| e.to_string())?;
        for (i, chunk) in plain.chunks(BURN_CHUNK).enumerate() {
            self.sync_checkpoint()?;
            send_json(
                &mut ch,
                &mut stream,
                &Msg::BurnData {
                    burn_id,
                    idx: i,
                    data_b64: base64_encode(chunk),
                },
            )
            .map_err(|e| e.to_string())?;
        }
        send_json(&mut ch, &mut stream, &Msg::BurnDone { burn_id }).map_err(|e| e.to_string())?;
        let ack: Msg = recv_json(&mut ch, &mut stream).map_err(|e| e.to_string())?;
        let delivered = match ack {
            Msg::BurnAck { phase, .. } if phase == "delivered" => true,
            Msg::BurnAck { phase, .. } => return Err(format!("burn rejected: {phase}")),
            Msg::Error { msg } => return Err(msg),
            _ => return Err("expected burnack".into()),
        };
        // 交付即失效：票据不可复用（`05-04` §5.2）
        {
            let mut local = self
                .inner
                .burns_local
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(t) = local.get_mut(&burn_id) {
                t.ticket.consumed = true;
                t.phase = if delivered {
                    BurnPhase::Delivered
                } else {
                    BurnPhase::Rejected
                };
                t.target = Some(target.to_string());
            }
        }
        self.log(&format!(
            "burn delivered id={} bytes={} peer={peer_id}",
            Self::short_id(burn_id),
            plain.len()
        ));
        Ok(serde_json::json!({"delivered": delivered, "bytes": plain.len(), "peer": peer_id}))
    }

    /// 接收端会话（在 `handle_conn` 的消息循环内被 `Msg::BurnOffer` 唤起）。
    fn burn_receive_session(
        &self,
        ch: &mut SecureChannel,
        stream: &mut Box<dyn Duplex>,
        offer: BurnOfferRec,
    ) -> Result<(), String> {
        let BurnOfferRec {
            burn_id,
            name,
            size,
            sha,
            total_chunks,
            open_timeout_secs,
        } = offer;
        let secret = self
            .inner
            .burn_secret_in
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or("no burn invite registered")?;
        let key = burn::burn_file_key(&secret);
        let mut data: Vec<u8> = Vec::with_capacity(size as usize);
        let mut got = 0usize;
        loop {
            let msg: Msg = recv_json(ch, stream).map_err(|e| e.to_string())?;
            match msg {
                Msg::BurnData {
                    burn_id: rid,
                    idx,
                    data_b64,
                } if rid == burn_id => {
                    if idx != got {
                        return Err("burn chunk out of order".into());
                    }
                    let chunk = base64_decode(&data_b64).ok_or("bad burn b64")?;
                    got += 1;
                    data.extend_from_slice(&chunk);
                }
                Msg::BurnDone { burn_id: rid } if rid == burn_id => break,
                Msg::Error { msg } => return Err(msg),
                _ => return Err("unexpected message in burn session".into()),
            }
        }
        if got != total_chunks || vault_crypto::hash_sha256(&data) != sha {
            return Err("burn payload mismatch".into());
        }
        // 落盘：AEAD(专用 burn 密钥, 明文) → `<data_dir>/burn/<id>.vse`
        let dir = self.burn_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let stored = dir.join(format!("{burn_id}.vse"));
        let nonce_v = random_bytes(vault_crypto::AES_GCM_NONCE_LEN);
        let nonce: [u8; vault_crypto::AES_GCM_NONCE_LEN] = nonce_v
            .as_slice()
            .try_into()
            .map_err(|_| "nonce len".to_string())?;
        let blob = aead_encrypt(&key, &nonce, &data).map_err(|e| e.to_string())?;
        std::fs::write(&stored, blob).map_err(|e| e.to_string())?;
        self.inner
            .burns_inbox
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                burn_id,
                BurnInbox {
                    name: name.clone(),
                    size: data.len() as u64,
                    key: Zeroizing::new(key),
                    stored,
                    phase: BurnPhase::Delivered,
                    opened_ms: None,
                    saved: false,
                    open_timeout_secs: open_timeout_secs.max(1),
                    bytes: data.len() as u64,
                    retries: 0,
                },
            );
        send_json(
            ch,
            stream,
            &Msg::BurnAck {
                burn_id,
                phase: "delivered".into(),
            },
        )
        .map_err(|e| e.to_string())?;
        self.log(&format!(
            "burn joined + delivered id={} bytes={}",
            Self::short_id(burn_id),
            data.len()
        ));
        Ok(())
    }

    /// 「打开」：解密为**受控临时明文**（Unix 下 0600），并起定时擦除。
    /// 返回 `{path, name, size, openTimeoutSecs}`——明文路径交外壳打开系统默认程序。
    pub fn burn_open(&self, burn_id: u64) -> Result<serde_json::Value, String> {
        let (name, size, timeout, tmp) = {
            let inbox = self
                .inner
                .burns_inbox
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let it = inbox.get(&burn_id).ok_or("unknown burn id")?;
            match it.phase {
                BurnPhase::Delivered | BurnPhase::Opened => {}
                _ => return Err("burn not openable in current phase".into()),
            }
            (
                it.name.clone(),
                it.size,
                it.open_timeout_secs,
                self.burn_dir().join("tmp").join(format!("{burn_id}.part")),
            )
        };
        let plain = self.burn_read_payload(burn_id)?;
        let _ = std::fs::create_dir_all(self.burn_dir().join("tmp"));
        std::fs::write(&tmp, &plain).map_err(|e| e.to_string())?;
        restrict_to_owner(&tmp);
        {
            let mut inbox = self
                .inner
                .burns_inbox
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(it) = inbox.get_mut(&burn_id) {
                it.phase = BurnPhase::Opened;
                it.opened_ms = Some(now_ms());
            }
        }
        // 打开后 `openTimeoutSecs` 自动擦除（临时明文 + 容器 + 密钥）
        let me = self.clone();
        let _ = std::thread::Builder::new()
            .name("vsync-burn-timer".into())
            .spawn(move || {
                std::thread::sleep(Duration::from_secs(timeout.max(1)));
                let _ = me.burn_erase(burn_id);
            });
        self.log(&format!("burn opened id={}", Self::short_id(burn_id)));
        Ok(serde_json::json!({
            "path": tmp.to_string_lossy(),
            "name": name, "size": size, "openTimeoutSecs": timeout,
        }))
    }

    /// 解密 burn 载荷（内存中；调用方负责写盘 / 立即擦除）。
    fn burn_read_payload(&self, burn_id: u64) -> Result<Vec<u8>, String> {
        let (key, stored) = {
            let inbox = self
                .inner
                .burns_inbox
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let it = inbox.get(&burn_id).ok_or("unknown burn id")?;
            (*it.key, it.stored.clone())
        };
        let blob = std::fs::read(&stored).map_err(|e| e.to_string())?;
        aead_decrypt(&key, &blob).ok_or_else(|| "burn payload unreadable".to_string())
    }

    /// 显式「另存」：写用户路径并**放弃该次自动擦除**（`05-04` §5.5）。
    pub fn burn_save_as(&self, burn_id: u64, dest: &str) -> Result<(), String> {
        let plain = self.burn_read_payload(burn_id)?;
        std::fs::write(dest, &plain).map_err(|e| e.to_string())?;
        let mut inbox = self
            .inner
            .burns_inbox
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(it) = inbox.get_mut(&burn_id) {
            it.saved = true;
        }
        self.log(&format!("burn saved_as id={}", Self::short_id(burn_id)));
        Ok(())
    }

    /// 擦除（临时明文 + 容器 + 内存密钥）；重试 1/5/30 s ×3，失败进 `EraseFailed`。
    pub fn burn_erase(&self, burn_id: u64) -> Result<serde_json::Value, String> {
        let (stored, tmp) = {
            let inbox = self
                .inner
                .burns_inbox
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            match inbox.get(&burn_id) {
                Some(it) => (
                    it.stored.clone(),
                    self.burn_dir().join("tmp").join(format!("{burn_id}.part")),
                ),
                None => {
                    // 发送端票据：擦除本地临时残留即可
                    (
                        self.inner.data_dir.join(format!("burn-send-{burn_id}.tmp")),
                        self.inner.data_dir.join(format!("burn-send-{burn_id}.tmp")),
                    )
                }
            }
        };
        let (ok, attempts) = burn::erase_with_retries(&[tmp, stored], &burn::RETRY_DELAYS, |p| {
            std::fs::remove_file(p)
        });
        {
            let mut inbox = self
                .inner
                .burns_inbox
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(it) = inbox.get_mut(&burn_id) {
                it.phase = if ok {
                    BurnPhase::Erased
                } else {
                    BurnPhase::EraseFailed
                };
                it.retries = attempts.saturating_sub(1);
                // 擦除成功即丢弃内存密钥（Drop 时零化）
                if ok {
                    it.key = Zeroizing::new([0u8; KEY_LEN]);
                }
            }
        }
        self.log(&format!(
            "burn {} id={} attempts={attempts}",
            if ok { "erased" } else { "erase_failed" },
            Self::short_id(burn_id)
        ));
        Ok(serde_json::json!({"erased": ok, "attempts": attempts}))
    }

    /// 票据 / 载荷状态（UI 与冒烟共用）。
    pub fn burn_status(&self, burn_id: u64) -> serde_json::Value {
        let local = self
            .inner
            .burns_local
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(t) = local.get(&burn_id) {
            return serde_json::json!({
                "burnId": burn_id, "side": "sender",
                "phase": t.phase.as_str(), "opened": t.phase == BurnPhase::Opened,
                "erased": t.phase == BurnPhase::Erased,
                "consumed": t.ticket.consumed, "target": t.target,
            });
        }
        drop(local);
        let inbox = self
            .inner
            .burns_inbox
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        match inbox.get(&burn_id) {
            Some(it) => serde_json::json!({
                "burnId": burn_id, "side": "receiver",
                "phase": it.phase.as_str(), "name": it.name,
                "opened": it.opened_ms.is_some(), "erased": it.phase == BurnPhase::Erased,
                "saved": it.saved, "bytes": it.bytes, "retries": it.retries,
            }),
            None => serde_json::json!({
                "burnId": burn_id, "side": "unknown",
                "phase": BurnPhase::Expired.as_str(), "erased": false,
            }),
        }
    }

    /// 启动扫描：`burn/` 下的一切残留都是 `Expired` 垃圾（秘密不落盘 → 重启后不可解），
    /// 一律擦除（`05-04` §5.5 的「下次启动重试」）。返回删除文件数。
    pub fn burn_sweep_startup(&self) -> usize {
        let dir = self.burn_dir();
        let mut n = 0usize;
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    if let Ok(inner) = std::fs::read_dir(&p) {
                        for f in inner.flatten() {
                            if std::fs::remove_file(f.path()).is_ok() {
                                n += 1;
                            }
                        }
                    }
                } else if std::fs::remove_file(&p).is_ok() {
                    n += 1;
                }
            }
        }
        if n > 0 {
            self.log(&format!("burn startup sweep: erased {n} leftover file(s)"));
        }
        n
    }

    /// P8-9：本机持有票据的只读快照（供 `vault_core` 汇报）。
    pub fn burn_pending(&self) -> serde_json::Value {
        let local = self
            .inner
            .burns_local
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        serde_json::json!(local
            .iter()
            .map(|(id, t)| serde_json::json!({
                "burnId": id, "phase": t.phase.as_str(), "consumed": t.ticket.consumed,
            }))
            .collect::<Vec<_>>())
    }

    /// P8-9：一次性载荷的落盘目录（UI / 冒烟自检用）。
    pub fn burn_dir_for_ui(&self) -> String {
        self.burn_dir().to_string_lossy().to_string()
    }

    /// 清理超期冲突副本（P8-8；策略注入时调用）。返回删除条数。
    pub fn sweep_expired_conflicts(&self, keep_days: u32) -> usize {
        let template = {
            let f = self
                .inner
                .sync_filter
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            f.copy_name_template.clone()
        };
        let marker = vault_vault::vault::Vault::conflict_marker(&template);
        let mut slot = self
            .inner
            .vault_slot
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        match slot.as_mut() {
            Some(v) => {
                let n = v.sweep_conflict_copies(&marker, keep_days, now_ms());
                if n > 0 {
                    self.log(&format!("conflict copies swept: {n}"));
                }
                n
            }
            None => 0,
        }
    }

    /// P8-8：冲突副本策略（模板 / 每文件上限）——来自注入的同步策略
    /// （`sync_policy.conflict`，不新增第三个策略文件）。
    fn conflict_policy(&self) -> (String, u32) {
        let f = self
            .inner
            .sync_filter
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        (f.copy_name_template.clone(), f.max_copies_per_file)
    }

    fn reset_skipped(&self) {
        self.inner
            .skipped
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    fn record_skip(&self, id: u64, reason: &str) {
        let mut sk = self.inner.skipped.lock().unwrap_or_else(|e| e.into_inner());
        if !sk.iter().any(|(i, _)| *i == id) {
            sk.push((id, reason.to_string()));
        }
    }

    /// P8-7：出队过滤决策（入队前四维 + 出站前网络复核）。
    /// 文件夹维度需要保险箱的文件夹树 → 在此处判定（含子树递归）。
    fn outbound_skip(&self, item: &ManifestItem, peer_id: &str) -> Option<&'static str> {
        let (f, ctx) = {
            let f = self
                .inner
                .sync_filter
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let ctx = self
                .inner
                .sync_ctx
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            (f.clone(), ctx.clone())
        };
        if !f.enabled {
            return None;
        }
        let folder_hit = {
            let slot = self
                .inner
                .vault_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            slot.as_ref()
                .map(|v| v.file_in_folders(item.id, &f.folder_exclude))
                .unwrap_or(false)
        };
        f.check_enqueue(item.size, folder_hit, peer_id, &ctx)
            .or_else(|| f.check_egress(&ctx))
    }

    /// 计划与执行：docs/05-03 §6.2 固定优先级（删除优先 > 多版本保留；向量时钟仅辅助判定）。
    /// P8-7：`peer_id` 供选择性同步的目标设备维度使用。
    fn execute_plan(
        &self,
        ch: &mut SecureChannel,
        stream: &mut Box<dyn Duplex>,
        local: &[ManifestItem],
        peer: &[ManifestItem],
        peer_id: &str,
    ) -> Result<serde_json::Value, String> {
        let lm: BTreeMap<u64, &ManifestItem> = local.iter().map(|i| (i.id, i)).collect();
        let pm: BTreeMap<u64, &ManifestItem> = peer.iter().map(|i| (i.id, i)).collect();
        let mut summary = serde_json::json!({
            "pushed": [], "pulled": [], "deleted": [], "conflicts": [], "merged": [], "skipped": []
        });
        let mut ids: Vec<u64> = lm.keys().copied().chain(pm.keys().copied()).collect();
        ids.sort_unstable();
        ids.dedup();
        let push = |arr: &str, v: serde_json::Value, s: &mut serde_json::Value| {
            if let Some(x) = s[arr].as_array_mut() {
                x.push(v);
            }
        };

        for id in ids {
            // P8-2：文件边界检查点（取消立即收敛；暂停阻塞等待恢复）
            self.sync_checkpoint()?;
            let a = lm.get(&id).copied();
            let b = pm.get(&id).copied();
            match (a, b) {
                // 双方均为活文件
                (Some(fa), Some(fb)) if fa.deleted_ms.is_none() && fb.deleted_ms.is_none() => {
                    if !fa.file_sha.is_empty() && fa.file_sha == fb.file_sha {
                        if fa.vc != fb.vc {
                            {
                                let mut slot = self
                                    .inner
                                    .vault_slot
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner());
                                if let Some(v) = slot.as_mut() {
                                    let _ = v.merge_vc(id, &fb.vc);
                                    let _ = v.save_index_now();
                                }
                            }
                            send_json(
                                ch,
                                stream,
                                &Msg::VcMerge {
                                    id,
                                    vc: fa.vc.clone(),
                                },
                            )
                            .map_err(|e| e.to_string())?;
                            push("merged", serde_json::json!(id), &mut summary);
                        }
                        continue;
                    }
                    if dominates(&fa.vc, &fb.vc) {
                        let r = self.push_file(ch, stream, fa, false, peer_id)?;
                        self.note_outcome(r, "pushed", id, &mut summary);
                    } else if dominates(&fb.vc, &fa.vc) {
                        let r = self.pull_file(ch, stream, id, false)?;
                        self.note_outcome(r, "pulled", id, &mut summary);
                    } else if fa.vc != fb.vc || fa.file_sha != fb.file_sha {
                        // 分叉：较新修改者胜出；落后方先自留加密冲突副本（docs/05-03 规则 2）。
                        // P8-8：`rev` 不同步推进时内容仍可能不同（原地编辑不改 vc）——
                        // sha 不同即视为分叉，交接收端做块级三方合并。
                        let r = if fa.modified_ms >= fb.modified_ms {
                            self.push_file(ch, stream, fa, true, peer_id)?
                        } else {
                            self.pull_file(ch, stream, id, true)?
                        };
                        self.note_outcome(r, "conflicts", id, &mut summary);
                    }
                }
                // 本端活文件、对端墓碑
                (Some(fa), Some(tb)) if fa.deleted_ms.is_none() => {
                    if dominates(&tb.vc, &fa.vc) {
                        self.delete_local(id)?;
                        push("deleted", serde_json::json!(id), &mut summary);
                    } else if dominates(&fa.vc, &tb.vc) {
                        let r = self.push_file(ch, stream, fa, false, peer_id)?; // 复活对端
                        self.note_outcome(r, "pushed", id, &mut summary);
                    } else {
                        // 并发删除 + 修改 → 删除优先；保护窗口内留加密副本
                        self.delete_local_protected(id)?;
                        push("deleted", serde_json::json!(id), &mut summary);
                    }
                }
                // 本端墓碑、对端活文件
                (Some(ta), Some(fb)) if fb.deleted_ms.is_none() => {
                    if dominates(&ta.vc, &fb.vc) || ta.vc != fb.vc {
                        // 删除优先（含并发）：带签名删除指令，对端窗口期内自留副本
                        let sig = self.inner.ident.sign(&delete_sign_body(id));
                        send_json(ch, stream, &Msg::DeleteOrder { id, sig })
                            .map_err(|e| e.to_string())?;
                        push("deleted", serde_json::json!(id), &mut summary);
                    } else {
                        let r = self.pull_file(ch, stream, id, false)?; // 对端复活
                        self.note_outcome(r, "pulled", id, &mut summary);
                    }
                }
                // 单侧存在
                (Some(fa), None) if fa.deleted_ms.is_none() => {
                    let r = self.push_file(ch, stream, fa, false, peer_id)?;
                    self.note_outcome(r, "pushed", id, &mut summary);
                }
                (None, Some(fb)) if fb.deleted_ms.is_none() => {
                    let r = self.pull_file(ch, stream, id, false)?;
                    self.note_outcome(r, "pulled", id, &mut summary);
                }
                _ => {}
            }
        }
        Ok(summary)
    }

    /// P8-7：把一次出站结果写进摘要——`Skipped` = 被策略跳过（记 skipped 与引擎状态，
    /// 不计入 pushed/pulled）；`AutoMerged` = 接收端块级自动合并（记 `merged` 而非
    /// `conflicts`，避免把「已自动合并」误报成冲突）；`Done` = 按 `ok` 归档。
    fn note_outcome(&self, r: TxOutcome, ok: &str, id: u64, summary: &mut serde_json::Value) {
        match r {
            TxOutcome::Skipped(reason) => {
                self.record_skip(id, reason);
                if let Some(x) = summary["skipped"].as_array_mut() {
                    x.push(serde_json::json!({"id": id, "reason": reason}));
                }
            }
            TxOutcome::AutoMerged => {
                if let Some(x) = summary["merged"].as_array_mut() {
                    x.push(serde_json::json!(id));
                }
            }
            TxOutcome::Done => {
                if let Some(x) = summary[ok].as_array_mut() {
                    x.push(serde_json::json!(id));
                }
            }
        }
    }

    fn delete_local(&self, id: u64) -> Result<(), String> {
        let mut slot = self
            .inner
            .vault_slot
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let v = slot.as_mut().ok_or("vault locked")?;
        if v.has_file(id) {
            v.delete_file(id, false).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn delete_local_protected(&self, id: u64) -> Result<(), String> {
        let mut slot = self
            .inner
            .vault_slot
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let v = slot.as_mut().ok_or("vault locked")?;
        if v.has_file(id) {
            let modified = v.file_modified(id).map_err(|e| e.to_string())?;
            if modified + PROTECT_WINDOW_MS > now_ms() {
                v.save_conflict_copy(id, BTreeMap::new())
                    .map_err(|e| e.to_string())?;
            }
            v.delete_file(id, false).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// 推送：SendFileBegin → BlocksNeeded → BlockData… → BlockEnd → FileApplied。
    /// P8-7：先过选择性同步策略；`Skipped` = 被跳过**且未发送任何块**。
    /// P8-8：接收端若做块级自动合并，回 `FileApplied{merge:"auto"}` → `AutoMerged`。
    fn push_file(
        &self,
        ch: &mut SecureChannel,
        stream: &mut Box<dyn Duplex>,
        item: &ManifestItem,
        overwrite: bool,
        peer_id: &str,
    ) -> Result<TxOutcome, String> {
        let id = item.id;
        if let Some(reason) = self.outbound_skip(item, peer_id) {
            self.log(&format!("sync skip #{id}: {reason}"));
            return Ok(TxOutcome::Skipped(reason));
        }
        let fskey = {
            let slot = self
                .inner
                .vault_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let v = slot.as_ref().ok_or("vault locked")?;
            v.fskey_bytes_for(id).map_err(|e| e.to_string())?
        };
        let wrapped = wrap_fskey(ch.handshake_hash(), &fskey)?;
        // P8-8：`base_rev` = 本版本所基于的基线（`rev − 1`）；`rev = 0`（未曾原地编辑）→ 0
        let base_rev = item.rev.saturating_sub(1);
        send_json(
            ch,
            stream,
            &Msg::SendFileBegin {
                item: item.clone(),
                fskey_wrapped: hex_encode(&wrapped),
                total_chunks: item.chunks.len(),
                overwrite,
                base_rev,
            },
        )
        .map_err(|e| e.to_string())?;
        let resp: Msg = recv_json(ch, stream).map_err(|e| e.to_string())?;
        let idxs = match resp {
            Msg::BlocksNeeded { id: rid, idxs } if rid == id => idxs,
            Msg::Error { msg } => return Err(msg),
            _ => return Err("expected blocksneeded".into()),
        };
        for idx in idxs {
            let ct = {
                let slot = self
                    .inner
                    .vault_slot
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let v = slot.as_ref().ok_or("vault locked")?;
                v.read_chunk_ct(id, idx).map_err(|e| e.to_string())?
            };
            self.sync_checkpoint()?;
            self.throttle(ct.len());
            send_json(
                ch,
                stream,
                &Msg::BlockData {
                    id,
                    idx,
                    ct_b64: base64_encode(&ct),
                },
            )
            .map_err(|e| e.to_string())?;
        }
        send_json(ch, stream, &Msg::BlockEnd { id }).map_err(|e| e.to_string())?;
        let done: Msg = recv_json(ch, stream).map_err(|e| e.to_string())?;
        match done {
            Msg::FileApplied { merge, .. } => {
                if merge.as_deref() == Some("auto") {
                    self.log(&format!("push #{id}: peer auto-merged (block-level)"));
                    Ok(TxOutcome::AutoMerged)
                } else {
                    Ok(TxOutcome::Done)
                }
            }
            Msg::Error { msg } => Err(msg),
            _ => Err("expected fileapplied".into()),
        }
    }

    /// 拉取：PullFile → 对端走推送流程。overwrite 时对端 SendFileBegin 标记覆盖。
    /// P8-7：对端因策略拒绝时回 `Error{msg:"filtered:<reason>"}` → 本端记为 skipped。
    fn pull_file(
        &self,
        ch: &mut SecureChannel,
        stream: &mut Box<dyn Duplex>,
        id: u64,
        overwrite: bool,
    ) -> Result<TxOutcome, String> {
        send_json(ch, stream, &Msg::PullFile { id }).map_err(|e| e.to_string())?;
        let begin: Msg = recv_json(ch, stream).map_err(|e| e.to_string())?;
        match begin {
            Msg::SendFileBegin {
                item,
                fskey_wrapped,
                overwrite: peer_marked,
                base_rev,
                ..
            } => {
                // overwrite 语义由接收端（本端）执行冲突副本；以请求时的标记为准
                let merged = self.receive_push_rest(
                    ch,
                    stream,
                    item,
                    fskey_wrapped,
                    overwrite || peer_marked,
                    base_rev,
                )?;
                Ok(if merged {
                    TxOutcome::AutoMerged
                } else {
                    TxOutcome::Done
                })
            }
            Msg::Error { msg } => {
                if let Some(reason) = msg.strip_prefix("filtered:") {
                    Ok(TxOutcome::Skipped(forced_reason(reason)))
                } else {
                    Err(msg)
                }
            }
            _ => Err("expected sendfilebegin".into()),
        }
    }

    /// 通用接收推送流程：解封 FSKey → 块清单比对（增量/续传幂等）→ 收块 → 装配 → 整文件校验。
    /// 返回 `true` = 已做块级自动合并（调用方据此回 `FileApplied{merge:"auto"}`）。
    fn receive_push_rest(
        &self,
        ch: &mut SecureChannel,
        stream: &mut Box<dyn Duplex>,
        item: ManifestItem,
        fskey_wrapped: String,
        overwrite: bool,
        base_rev: u64,
    ) -> Result<bool, String> {
        let id = item.id;
        let sub = subkey(ch.handshake_hash(), "fskey");
        let wrapped_bytes = vault_crypto::hex_decode(&fskey_wrapped).ok_or("bad fskey wrap")?;
        let fskey: [u8; KEY_LEN] = aead_decrypt(&sub, &wrapped_bytes)
            .ok_or("fskey unwrap failed")?
            .as_slice()
            .try_into()
            .map_err(|_| "fskey len")?;
        let fskey_hex = hex_encode(&fskey);

        {
            let slot = self
                .inner
                .vault_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let v = slot.as_ref().ok_or("vault locked")?;
            if v.has_file(id) && !overwrite {
                return Err(format!("file {id} already exists locally"));
            }
        }

        // 05-03 §6.1：.part 头部持久化（VSPT，vault-net::part）——跨会话续传，
        // 取代 V1.0「每次开始传输即截断清零」。同版本判据 = 整文件哈希 + 大小 + 块数；
        // 对端文件已变 → 作废重传（安全优先于带宽）。
        let part = self.part_path(id);
        let part_raw = part.with_extension("raw");
        let sha32: [u8; 32] = vault_crypto::hex_decode(&item.file_sha)
            .and_then(|v| <[u8; 32]>::try_from(v).ok())
            .unwrap_or([0u8; 32]);
        let expected = vault_net::part::PartMeta {
            file_id: id,
            total_bytes: item.size,
            file_sha256: sha32,
            chunk_count: item.chunks.len() as u32,
        };
        let mut pf = match vault_net::part::open(&part) {
            Ok(pf)
                if pf.file_sha256() == &expected.file_sha256
                    && pf.total_bytes() == expected.total_bytes
                    && pf.chunk_count() == expected.chunk_count =>
            {
                pf // 跨会话续传：已收块直接沿用
            }
            _ => {
                let _ = std::fs::remove_file(&part);
                vault_net::part::create(&part, &expected).map_err(|e| e.to_string())?;
                vault_net::part::open(&part).map_err(|e| e.to_string())?
            }
        };

        // 与本地既有容器块清单比对：哈希相同的块不再传输（增量 + 断点续传的幂等基础）；
        // 上次会话已收进 .part 的块同样不再请求
        let have: HashSet<String> = {
            let slot = self
                .inner
                .vault_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let v = slot.as_ref().ok_or("vault locked")?;
            if v.has_file(id) {
                let json = v.sync_manifest().map_err(|e| e.to_string())?;
                let all: Vec<ManifestItem> =
                    serde_json::from_str(&json).map_err(|e| e.to_string())?;
                all.into_iter()
                    .find(|i| i.id == id)
                    .map(|i| i.chunks.iter().map(|c| c.hash.clone()).collect())
                    .unwrap_or_default()
            } else {
                HashSet::new()
            }
        };
        let idxs: Vec<usize> = item
            .chunks
            .iter()
            .enumerate()
            .filter(|(i, c)| !have.contains(&c.hash) && !pf.has_chunk(*i))
            .map(|(i, _)| i)
            .collect();
        send_json(
            ch,
            stream,
            &Msg::BlocksNeeded {
                id,
                idxs: idxs.clone(),
            },
        )
        .map_err(|e| e.to_string())?;

        {
            let slot = self
                .inner
                .vault_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let v = slot.as_ref().ok_or("vault locked")?;
            for (i, c) in item.chunks.iter().enumerate() {
                if pf.has_chunk(i) {
                    continue; // 跨会话已收：沿用（不做重复拷贝）
                }
                if !idxs.contains(&i) {
                    // 本地既有块：校验哈希后拷入 VSPT（密文一致）
                    let ct = v.read_chunk_ct(id, i).map_err(|e| e.to_string())?;
                    if vault_crypto::hash_sha256(&ct) != c.hash {
                        return Err("local chunk hash mismatch".into());
                    }
                    pf.write_chunk(i, &ct).map_err(|e| e.to_string())?;
                }
            }
        }

        let expect: HashSet<usize> = idxs.iter().copied().collect();
        let mut got: HashSet<usize> = HashSet::new();
        loop {
            let msg: Msg = recv_json(ch, stream).map_err(|e| e.to_string())?;
            match msg {
                Msg::BlockData {
                    id: rid,
                    idx,
                    ct_b64,
                } if rid == id => {
                    if !expect.contains(&idx) || !got.insert(idx) {
                        return Err("unexpected or duplicate block".into());
                    }
                    let ct = base64_decode(&ct_b64).ok_or("bad block b64")?;
                    if vault_crypto::hash_sha256(&ct) != item.chunks[idx].hash {
                        return Err("block hash mismatch".into());
                    }
                    self.sync_checkpoint()?;
                    self.throttle(ct.len());
                    pf.write_chunk(idx, &ct).map_err(|e| e.to_string())?;
                }
                Msg::BlockEnd { id: rid } if rid == id => break,
                Msg::Error { msg } => return Err(msg),
                _ => return Err("expected blockdata".into()),
            }
        }
        if pf.received_count() as usize != item.chunks.len() {
            return Err("missing blocks".into());
        }
        // 物化原始数据区（按块序拼接）供 ingest_remote 装配容器
        {
            let mut raw = std::fs::File::create(&part_raw).map_err(|e| e.to_string())?;
            for i in 0..item.chunks.len() {
                let ct = pf
                    .read_chunk(i)
                    .ok_or_else(|| "missing chunk at materialize".to_string())?;
                raw.write_all(&ct).map_err(|e| e.to_string())?;
            }
            raw.flush().map_err(|e| e.to_string())?;
        }

        // P8-8：双方从同一基线各编辑一次 → 先做**块级三方合并**（不相交即自动合并，
        // 不产生冲突副本；相交则落回既有「自留副本 + 应用对端」路径）。
        //
        // **顺序硬约束**：本判定必须早于下面的「覆盖冲突」块——覆盖路径会先
        // `save_conflict_copy` + `delete_file` 摘除本端条目，之后再判定就只能看到
        // 墓碑（`has_file=false`），自动合并永不触发（2026-09-24 已由 e2e 抓出并修正）。
        let meta = vault_vault::container::FileMeta {
            name: item.name.clone(),
            size: item.size,
            nonce_prefix: item.nonce_prefix,
            chunks: item
                .chunks
                .iter()
                .map(|c| vault_vault::container::ChunkEntry {
                    hash: c.hash.clone(),
                    offset: c.offset,
                    len: c.len,
                })
                .collect(),
            file_sha256: item.file_sha.clone(),
            created_ms: item.modified_ms,
            modified_ms: item.modified_ms,
        };
        let merge_class = {
            let mut slot = self
                .inner
                .vault_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            match slot.as_mut() {
                Some(v) if v.has_file(id) => {
                    match v.try_merge_incoming(id, base_rev, &meta, &part_raw, &fskey_hex) {
                        Ok(c) => c,
                        Err(e) => {
                            // 合并评估失败不阻断同步：退回覆盖路径，但如实记诊断
                            self.log(&format!("merge attempt failed #{id}: {e}"));
                            vault_vault::merge::MergeClass::Replace
                        }
                    }
                }
                _ => vault_vault::merge::MergeClass::Replace,
            }
        };
        if merge_class == vault_vault::merge::MergeClass::Auto {
            std::fs::remove_file(&part).ok();
            std::fs::remove_file(&part_raw).ok();
            self.log(&format!(
                "recv #{id}: block-level auto-merge (baseRev={base_rev})"
            ));
            send_json(
                ch,
                stream,
                &Msg::FileApplied {
                    id,
                    name: item.name.clone(),
                    merge: Some("auto".to_string()),
                },
            )
            .map_err(|e| e.to_string())?;
            return Ok(true);
        }
        if merge_class == vault_vault::merge::MergeClass::Conflict {
            self.log(&format!(
                "recv #{id}: block-level merge conflict → conflict copy (baseRev={base_rev})"
            ));
        }

        // 覆盖冲突：先把本地既有版本另存为加密冲突副本并摘除原条目，
        // 再让新内容占据原 id（副本容器已独立拷贝，删原容器无碍）
        if overwrite {
            let mut slot = self
                .inner
                .vault_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let v = slot.as_mut().ok_or("vault locked")?;
            if v.has_file(id) {
                let (tpl, max_copies) = self.conflict_policy();
                v.save_conflict_copy_with(id, item.vc.clone(), &tpl, max_copies)
                    .map_err(|e| e.to_string())?;
                v.delete_file(id, false).map_err(|e| e.to_string())?;
            }
        }

        {
            let mut slot = self
                .inner
                .vault_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let v = slot.as_mut().ok_or("vault locked")?;
            v.ingest_remote(
                id,
                item.folder,
                &item.name,
                item.size,
                item.modified_ms,
                meta,
                &part_raw,
                &fskey_hex,
                item.vc.clone(),
            )
            .map_err(|e| e.to_string())?;
            // P8-8：保留对端的 `rev` 血缘（供后续三方合并判定基线）
            if item.rev > 0 {
                let _ = v.set_rev(id, item.rev);
            }
            v.verify_file(id).map_err(|e| e.to_string())?;
        }
        std::fs::remove_file(&part).ok();
        std::fs::remove_file(&part_raw).ok();
        send_json(
            ch,
            stream,
            &Msg::FileApplied {
                id,
                name: item.name.clone(),
                merge: None,
            },
        )
        .map_err(|e| e.to_string())?;
        Ok(false)
    }

    /// 响应端同步：回清单 → 处理指令（收推送 / 拉取 / 合并 / 删除）直到 SyncDone。
    fn respond_sync(
        &self,
        ch: &mut SecureChannel,
        stream: &mut Box<dyn Duplex>,
        peer_id: &str,
    ) -> Result<(), String> {
        self.reset_skipped();
        let local_items: Vec<ManifestItem> = {
            let slot = self
                .inner
                .vault_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let v = slot.as_ref().ok_or("vault locked")?;
            let json = v.sync_manifest().map_err(|e| e.to_string())?;
            serde_json::from_str(&json).map_err(|e| e.to_string())?
        };
        send_json(
            ch,
            stream,
            &Msg::SyncResp {
                manifest: local_items,
            },
        )
        .map_err(|e| e.to_string())?;
        loop {
            let msg: Msg = recv_json(ch, stream).map_err(|e| e.to_string())?;
            match msg {
                Msg::SendFileBegin {
                    item,
                    fskey_wrapped,
                    overwrite,
                    base_rev,
                    ..
                } => {
                    self.receive_push_rest(ch, stream, item, fskey_wrapped, overwrite, base_rev)?;
                }
                Msg::PullFile { id } => {
                    let item = self.manifest_item(id)?;
                    self.respond_push(ch, stream, &item, peer_id)?;
                }
                Msg::VcMerge { id, vc } => {
                    let mut slot = self
                        .inner
                        .vault_slot
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    if let Some(v) = slot.as_mut() {
                        let _ = v.merge_vc(id, &vc);
                        let _ = v.save_index_now();
                    }
                }
                Msg::DeleteOrder { id, sig } => {
                    // 删除指令签名校验：公钥取自已登记对端表
                    let pub_hex = self
                        .inner
                        .peers
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .list()
                        .into_iter()
                        .map(|(_, p)| p.pub_hex)
                        .find(|pk| Identity::verify(pk, &delete_sign_body(id), &sig))
                        .ok_or("delete order signature invalid")?;
                    let _ = pub_hex;
                    match self.apply_remote_delete(id) {
                        Ok(note) => self.log(&format!("responder delete #{id}: {note}")),
                        Err(e) => {
                            self.log(&format!("responder delete #{id} failed: {e}"));
                            return Err(e);
                        }
                    }
                }
                Msg::SyncDone => return Ok(()),
                Msg::Error { msg } => return Err(msg),
                _ => return Err("unexpected message in sync phase".into()),
            }
        }
    }

    /// 响应端推送（与发起端 push_file 对称）。
    /// P8-7：响应端出站同样受策略约束（否则「排除文件夹」会经对端 pull 泄漏）；
    /// 被跳过时回 `Error{msg:"filtered:<reason>"}`，由请求端记为 skipped。
    fn respond_push(
        &self,
        ch: &mut SecureChannel,
        stream: &mut Box<dyn Duplex>,
        item: &ManifestItem,
        peer_id: &str,
    ) -> Result<(), String> {
        let id = item.id;
        if let Some(reason) = self.outbound_skip(item, peer_id) {
            self.log(&format!("sync skip (respond) #{id}: {reason}"));
            self.record_skip(id, reason);
            return send_json(
                ch,
                stream,
                &Msg::Error {
                    msg: format!("filtered:{reason}"),
                },
            )
            .map_err(|e| e.to_string());
        }
        let fskey = {
            let slot = self
                .inner
                .vault_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let v = slot.as_ref().ok_or("vault locked")?;
            v.fskey_bytes_for(id).map_err(|e| e.to_string())?
        };
        let wrapped = wrap_fskey(ch.handshake_hash(), &fskey)?;
        send_json(
            ch,
            stream,
            &Msg::SendFileBegin {
                item: item.clone(),
                fskey_wrapped: hex_encode(&wrapped),
                total_chunks: item.chunks.len(),
                overwrite: false,
                base_rev: 0,
            },
        )
        .map_err(|e| e.to_string())?;
        let resp: Msg = recv_json(ch, stream).map_err(|e| e.to_string())?;
        let idxs = match resp {
            Msg::BlocksNeeded { id: rid, idxs } if rid == id => idxs,
            Msg::Error { msg } => return Err(msg),
            _ => return Err("expected blocksneeded".into()),
        };
        for idx in idxs {
            let ct = {
                let slot = self
                    .inner
                    .vault_slot
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let v = slot.as_ref().ok_or("vault locked")?;
                v.read_chunk_ct(id, idx).map_err(|e| e.to_string())?
            };
            self.sync_checkpoint()?;
            self.throttle(ct.len());
            send_json(
                ch,
                stream,
                &Msg::BlockData {
                    id,
                    idx,
                    ct_b64: base64_encode(&ct),
                },
            )
            .map_err(|e| e.to_string())?;
        }
        send_json(ch, stream, &Msg::BlockEnd { id }).map_err(|e| e.to_string())?;
        let done: Msg = recv_json(ch, stream).map_err(|e| e.to_string())?;
        match done {
            Msg::FileApplied { .. } => Ok(()),
            Msg::Error { msg } => Err(msg),
            _ => Err("expected fileapplied".into()),
        }
    }

    /// 向已配对设备发送远程销毁指令（签名 + 延迟语义，docs/08 §4.3）。
    pub fn destroy_arm_remote(
        &self,
        addr: &str,
        target: &str,
        delay_secs: u64,
    ) -> Result<serde_json::Value, String> {
        let mut stream: Box<dyn Duplex> = TcpStream::connect(addr)
            .map_err(|e| e.to_string())
            .map(Box::new)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| e.to_string())?;
        let mut ch = SecureChannel::handshake(
            &mut stream,
            Role::Initiator,
            &self.inner.ident,
            None,
            self.pq_offer_for_addr(addr),
        )
        .map_err(|e| e.to_string())?;
        self.note_channel(&ch, "destroy");
        let (_pid, _pn, ppub, _port, _peer_pq) = self
            .hello(&mut ch, &mut stream)
            .map_err(|e| e.to_string())?;
        let registered = self
            .inner
            .peers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .by_pub(&ppub)
            .is_some();
        if !registered {
            return Err("peer not paired".into());
        }
        let ts = now_ms();
        // saturating：秒 → 毫秒，异常大的入参不得在调试构建里 panic（跨 FFI 边界 panic 会 abort）
        let delay_ms = delay_secs.saturating_mul(1000);
        let sig = self
            .inner
            .ident
            .sign(&destroy_sign_body(target, delay_ms, ts));
        send_json(
            &mut ch,
            &mut stream,
            &Msg::DestroyCmd {
                target: target.to_string(),
                delay_ms,
                ts_ms: ts,
                sig,
            },
        )
        .map_err(|e| e.to_string())?;
        // 对端也可能在 Hello 后立刻投递它队列中发给本机的指令，故走可顺带处理的读取路径
        let mut early = EarlyMsgs::default();
        let ack: Msg = self.recv_with_orders(&mut ch, &mut stream, &ppub, &mut early)?;
        match ack {
            Msg::DestroyAck => {
                self.log(&format!("destroy command sent to {target}"));
                Ok(serde_json::json!({"sent": true, "delayMs": delay_ms}))
            }
            Msg::Error { msg } => Err(msg),
            _ => Err("expected destroyack".into()),
        }
    }

    /// P7-10 远程锁定：向对端投递 kind=2 锁定指令（`vsync-lock:{target}:{delay_ms}:{ts}`）。
    /// `peer_id = None` 表示全部已配对设备；离线对端照常入队（返回 pending=true）。
    /// 锁定回调（LockFn）由调用方注入；无回调时接收端仅记审计（如实降级）。
    pub fn push_lock(&self, peer_id: Option<&str>) -> Result<serde_json::Value, String> {
        let peers = self.inner.peers.lock().unwrap_or_else(|e| e.into_inner());
        let targets: Vec<String> = match peer_id {
            Some(id) => {
                if peers.get(id).is_none() {
                    return Err("peer not paired".into());
                }
                vec![id.to_string()]
            }
            None => peers.list().into_iter().map(|(id, _)| id).collect(),
        };
        drop(peers);
        let ts = now_ms();
        let mut queued = Vec::new();
        for target in &targets {
            // 配对存在性校验（签名域不含对端公钥，锁定指令天然是"发给谁"语义）
            if self
                .inner
                .peers
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(target)
                .is_none()
            {
                return Err("peer not paired".into());
            }
            let domain = format!("vsync-lock:{}:0:{ts}", target);
            let sig = self.inner.ident.sign(domain.as_bytes());
            let payload = serde_json::to_string(&serde_json::json!({
                "LockCmd": {"target": target, "delay_ms": 0, "ts_ms": ts, "sig": sig}
            }))
            .map_err(|e| e.to_string())?;
            self.inner
                .orders
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(OrderRec {
                    kind: 2,
                    peer_id: target.clone(),
                    target: target.clone(),
                    delay_secs: 0,
                    issued_ms: ts,
                    sig_hex: sig.clone(),
                    payload,
                })
                .map_err(|e| e.to_string())?;
            queued.push(target.clone());
        }
        self.log(&format!("lock orders queued: {} peer(s)", queued.len()));
        Ok(serde_json::json!({"queued": queued.len(), "peers": queued}))
    }

    /// P7-4 接收端：校验（签名域 vsync-rotate / 未过期 / issued 偏差 ≤300s /
    /// 用**自己的旧 MK** 解封 ct——成功即证明两端旧 MK 相同）→ 保存 inbound
    /// 待本端解锁后续做（数据面重加密需要密码，由 vault-core 解锁路径接管）。
    fn apply_rotation_notice(
        &self,
        rotation_id_hex: &str,
        blob_hex: &str,
        issued_ms: u64,
        expires_ms: u64,
        sig_hex: &str,
        peer_pub: &str,
    ) -> Result<(), OrderReject> {
        let from = crate::identity::Identity::device_id_of(peer_pub).unwrap_or_default();
        let domain = format!("vsync-rotate:{from}:{rotation_id_hex}:{issued_ms}:{expires_ms}");
        if !crate::identity::Identity::verify(peer_pub, domain.as_bytes(), sig_hex) {
            self.log("rotation notice rejected: signature invalid");
            return Err(OrderReject::BadSignature);
        }
        let now = now_ms();
        if now.saturating_sub(issued_ms) > 300_000 || issued_ms.saturating_sub(now) > 300_000 {
            self.log("rotation notice rejected: clock skew > 300s");
            return Err(OrderReject::Malformed);
        }
        if expires_ms != 0 && now > expires_ms {
            self.log("rotation notice rejected: expired");
            return Err(OrderReject::Malformed);
        }
        let blob = vault_crypto::hex_decode(blob_hex).ok_or(OrderReject::Malformed)?;
        // 用自己的旧 MK 解封：失败 = 两端旧 MK 不同 → 拒绝且不写任何状态
        let wrap_key =
            vault_crypto::kdf::hkdf_sha256_derive(&self.inner.keys_rt.mk, b"rotation-recover");
        let mk_new = aead_decrypt(&wrap_key, &blob).ok_or(OrderReject::MkMismatch)?;
        drop(mk_new);
        // 保存 inbound（rotation_id + 包装原样），本端下次解锁后自动续做
        let inbound = serde_json::json!({
            "schema": 1, "rotationId": rotation_id_hex, "ct": blob_hex,
            "issuedMs": issued_ms, "expiresMs": expires_ms,
        });
        let p = self.inner.data_dir.join("rotation.inbound");
        let body = serde_json::to_vec(&inbound).map_err(|_| OrderReject::Malformed)?;
        std::fs::write(&p, body).map_err(|_| OrderReject::PersistFailed)?;
        self.log(&format!(
            "rotation notice accepted: id={rotation_id_hex} (applied on next unlock)"
        ));
        Ok(())
    }

    /// P7-4：向对端入队轮换传播（kind=3）。blob = 新 MK' 被旧 MK 包装
    /// （与 VSRR 同构造，nonce‖ct 60B hex）。
    pub fn push_rotation(
        &self,
        peer_id: Option<&str>,
        rotation_id_hex: &str,
        blob_hex: &str,
    ) -> Result<serde_json::Value, String> {
        let peers = self.inner.peers.lock().unwrap_or_else(|e| e.into_inner());
        let targets: Vec<String> = match peer_id {
            Some(id) => {
                if peers.get(id).is_none() {
                    return Err("peer not paired".into());
                }
                vec![id.to_string()]
            }
            None => peers.list().into_iter().map(|(id, _)| id).collect(),
        };
        drop(peers);
        let issued = now_ms();
        let expires = issued + 7 * 24 * 3600 * 1000; // 默认 7 天
        let from = self.device_id();
        let mut queued = Vec::new();
        for target in &targets {
            let domain = format!("vsync-rotate:{from}:{rotation_id_hex}:{issued}:{expires}");
            let sig = self.inner.ident.sign(domain.as_bytes());
            let payload = serde_json::to_string(&serde_json::json!({
                "RotationNotice": {
                    "rotationId": rotation_id_hex, "ct": blob_hex,
                    "issuedMs": issued, "expiresMs": expires,
                    "sig": sig,
                }
            }))
            .map_err(|e| e.to_string())?;
            self.inner
                .orders
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(OrderRec {
                    kind: 3,
                    peer_id: target.clone(),
                    target: target.clone(),
                    delay_secs: 0,
                    issued_ms: issued,
                    sig_hex: sig,
                    payload,
                })
                .map_err(|e| e.to_string())?;
            queued.push(target.clone());
        }
        self.log(&format!(
            "rotation notices queued: {} peer(s) id={rotation_id_hex}",
            queued.len()
        ));
        Ok(serde_json::json!({"queued": queued.len(), "peers": queued}))
    }

    /// P7-4：是否仍有未投递的轮换传播（该对端的同步出站应被阻塞）。
    pub fn has_pending_rotation(&self, peer_id: &str) -> bool {
        self.inner
            .orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .list_for(peer_id)
            .iter()
            .any(|o| o.kind == 3)
    }

    /// P7-10 接收端：校验（签名 / target / 时钟偏差 ≤300s）→ 调 LockFn。
    /// 锁定不做二次确认（与销毁同级通道）；校验失败 → 丢弃 + 诊断。
    fn apply_lock_order(
        &self,
        target: &str,
        ts_ms: u64,
        sig_hex: &str,
        peer_pub: &str,
    ) -> Result<(), OrderReject> {
        let domain = format!("vsync-lock:{}:0:{ts_ms}", target);
        if !crate::identity::Identity::verify(peer_pub, domain.as_bytes(), sig_hex) {
            self.log("lock order rejected: signature invalid");
            return Err(OrderReject::BadSignature);
        }
        if target != self.device_id() {
            self.log("lock order rejected: target mismatch");
            return Err(OrderReject::TargetMismatch);
        }
        let now = now_ms();
        if now.saturating_sub(ts_ms) > 300_000 || ts_ms.saturating_sub(now) > 300_000 {
            self.log("lock order rejected: clock skew > 300s");
            return Err(OrderReject::Malformed);
        }
        self.log("lock order accepted: locking now");
        let lock = self
            .inner
            .lock_cb
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        match lock {
            Some(f) => {
                f();
                // 回填以便再次锁定（回调是一次性的）
                *self.inner.lock_cb.lock().unwrap_or_else(|e| e.into_inner()) = Some(f);
            }
            None => self.log("lock order: no lock callback wired (degraded)"),
        }
        Ok(())
    }

    /// 取消已武装的本机延迟销毁；返回是否有被取消的任务。
    /// P7-11：取消幂等落盘（armed 置空并持久化）。
    pub fn destroy_cancel(&self) -> bool {
        let cancelled = self
            .inner
            .armed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .is_some();
        let _ = self
            .inner
            .orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .set_armed(None);
        cancelled
    }

    /// P7-11：把武装状态写入 orders.enc（绝对 deadlineMs）。
    fn persist_armed(&self, target: &str, delay_ms: u64) -> Result<(), &'static str> {
        let rec = crate::orders::ArmedDestroyRec {
            schema: 1,
            target: target.to_string(),
            delay_ms,
            armed_ms: now_ms(),
            deadline_ms: now_ms() + delay_ms,
            order_issued_ms: now_ms(),
            nonce: vault_crypto::hex_encode(&vault_crypto::random_bytes(16)),
        };
        self.inner
            .orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .set_armed(Some(rec))
    }

    /// P7-11：启动续走——orders.enc 有武装记录时恢复倒计时或到期立即执行。
    fn resume_armed_on_startup(&self) {
        let rec = match self
            .inner
            .orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .armed()
        {
            Some(r) => r,
            None => return,
        };
        let now = now_ms();
        if now >= rec.deadline_ms {
            self.log("destroy armed_expired_executed: deadline passed while offline");
            let _ = self
                .inner
                .wipe
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
                .map(|f| f());
            let _ = self
                .inner
                .orders
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .set_armed(None);
            return;
        }
        let remain = rec.deadline_ms - now;
        *self.inner.armed.lock().unwrap_or_else(|e| e.into_inner()) = Some(ArmedDestroy {
            deadline: Instant::now() + Duration::from_millis(remain),
            target: rec.target.clone(),
        });
        self.log(&format!("destroy armed_resumed: {remain}ms remaining"));
    }

    fn throttle(&self, bytes: usize) {
        let bps = self.inner.rate_bps.load(Ordering::SeqCst);
        if bps == 0 {
            return;
        }
        let ms = (bytes as u64 * 1000) / bps.max(1);
        if ms > 0 {
            std::thread::sleep(Duration::from_millis(ms));
        }
    }

    fn part_path(&self, id: u64) -> PathBuf {
        self.inner.data_dir.join(format!("p2p-{id}.part"))
    }

    pub fn status(&self) -> serde_json::Value {
        let peers = self.inner.peers.lock().unwrap_or_else(|e| e.into_inner());
        let events = self.inner.events.lock().unwrap_or_else(|e| e.into_inner());
        let armed = self.inner.armed.lock().unwrap_or_else(|e| e.into_inner());
        // P8-6：填充协商诊断（05-04 F-07；帧数直方图按档 1–4）+
        // overheadRatio =（线上 − 载荷）/ 线上（无会话时 0.0）
        let padding = {
            let h = self
                .inner
                .frame_hist
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let payload = self.inner.bytes_payload.load(Ordering::SeqCst);
            let wire = self.inner.bytes_wire.load(Ordering::SeqCst);
            let overhead = if wire > 0 {
                ((wire - payload.min(wire)) as f64 / wire as f64 * 1000.0).round() / 1000.0
            } else {
                0.0
            };
            serde_json::json!({
                "tier": self.inner.pad_tier.load(Ordering::SeqCst),
                "legacyPeer": self.inner.pad_legacy.load(Ordering::SeqCst),
                "frameLenHistogram": [h[0], h[1], h[2], h[3]],
                "overheadRatio": overhead,
            })
        };
        let last_failure = {
            let lf = self
                .inner
                .last_path_failure
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            lf.as_ref()
                .map(|(r, at)| serde_json::json!({"reason": r, "atMs": at}))
        };
        let skipped_snapshot = {
            let sk = self.inner.skipped.lock().unwrap_or_else(|e| e.into_inner());
            sk.iter()
                .map(|(id, r)| serde_json::json!({"id": id, "reason": r}))
                .collect::<Vec<_>>()
        };
        serde_json::json!({
            "deviceId": self.device_id(),
            "fingerprint": self.fingerprint(),
            "port": self.port(),
            "name": self.inner.device_name,
            // P7-8：最近一次握手协商结果（0 尚未握手 / 1 经典 / 2 混合 KEM），强制可见
            "pqSuite": self.inner.last_suite.load(Ordering::SeqCst),
            "padding": padding,
            // P8-5：最近一次未归属对端的路径失败（拨号期失败；per-peer 见 path_stats）
            "lastPathFailure": last_failure,
            "peers": peers.list().iter().map(|(id, p)| serde_json::json!({
                "deviceId": id, "name": p.name,
                "fingerprint": crate::identity::Identity::fingerprint_of(&p.pub_hex),
                // 可空：从未观测到/未手填过地址时为 null（UI 据此提示手填，见 remember_peer_addr）
                "addr": p.addr,
                "pqCap": p.pq_cap,
            })).collect::<Vec<_>>(),
            "armedDestroy": armed.as_ref().map(|a| serde_json::json!({
                "target": a.target,
                "remainingMs": a.deadline.saturating_duration_since(Instant::now()).as_millis() as u64,
            })),
            "events": events.iter().rev().take(20).cloned().collect::<Vec<_>>(),
            // P8-7：最近一次会话被跳过的条目（选择性同步可诊断）
            "skipped": skipped_snapshot,
            "alive": !self.inner.dead.load(Ordering::SeqCst),
        })
    }

    /// P8-4/P8-5：per-peer 路径状态（`vault_core_p2p_path_status` 的 per-peer
    /// 段来源；05-03 §4.4 的本进程可观测子集——rtt / 对端候选数等未观测字段
    /// 不输出，不编造）。
    pub fn path_stats(&self) -> serde_json::Value {
        let ps = self
            .inner
            .path_stats
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let peers: Vec<serde_json::Value> = ps
            .iter()
            .map(|(id, s)| {
                serde_json::json!({
                    "peerId": id,
                    "path": s.path,
                    "pathSinceMs": s.path_since_ms,
                    "attempts": {"direct": s.attempts_direct, "relay": s.attempts_relay},
                    "lastFailure": s.last_failure.as_ref().map(|(r, at)| serde_json::json!({"reason": r, "atMs": at})),
                    "bytesIn": s.bytes_in,
                    "bytesOut": s.bytes_out,
                    "paddingTier": s.padding_tier,
                    // P8-4：打洞尝试（**只有计数与原因，绝不含 IP**——隐私纪律）
                    "punch": {
                        "attempts": s.punch.attempts,
                        "ok": s.punch.ok,
                        "reason": s.punch.reason,
                        "symmetricNat": s.punch.symmetric_nat,
                        "localCandidates": s.punch.local_candidates,
                        "remoteCandidates": s.punch.remote_candidates,
                        "degradedSteps": s.punch.degraded_steps,
                        "kind": s.punch.kind,
                    },
                })
            })
            .collect();
        serde_json::json!({ "schema": 1, "peers": peers })
    }
}

/// P8-7/P8-8：一次出站传输的终态（摘要与事件据此归档）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TxOutcome {
    /// 正常完成。
    Done,
    /// 被选择性同步策略跳过（未发送任何块）。
    Skipped(&'static str),
    /// 接收端做了块级自动合并（本端原样内容未落盘，两侧改动已合并）。
    AutoMerged,
}

/// P8-9：受控临时明文的权限收紧——Unix 下 0600；Windows 未做 ACL 收紧（如实声明，
/// 见 LOG：NTFS 上仅靠属性无法阻断同账户其他进程，需 ACL 或全盘加密作为前提）。
fn restrict_to_owner(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// P8-7：把线上「跳过原因」串映射回静态枚举（未知值收敛为 `unspecified`，
/// 不把对端自造的字符串带进本端事件文案）。
fn forced_reason(r: &str) -> &'static str {
    use crate::policy::{
        SKIP_DEVICE_NOT_TARGET, SKIP_FOLDER_EXCLUDED, SKIP_NET_TYPE_BLOCKED, SKIP_PEER_MODE,
        SKIP_SIZE_EXCEEDED, SKIP_WINDOW_CLOSED,
    };
    match r {
        SKIP_FOLDER_EXCLUDED => SKIP_FOLDER_EXCLUDED,
        SKIP_SIZE_EXCEEDED => SKIP_SIZE_EXCEEDED,
        SKIP_NET_TYPE_BLOCKED => SKIP_NET_TYPE_BLOCKED,
        SKIP_DEVICE_NOT_TARGET => SKIP_DEVICE_NOT_TARGET,
        SKIP_PEER_MODE => SKIP_PEER_MODE,
        SKIP_WINDOW_CLOSED => SKIP_WINDOW_CLOSED,
        _ => "unspecified",
    }
}

fn wrap_fskey(hh: &[u8; 32], fskey: &[u8; KEY_LEN]) -> Result<Vec<u8>, String> {
    let sub = subkey(hh, "fskey");
    let nonce_v = random_bytes(vault_crypto::AES_GCM_NONCE_LEN);
    let nonce: [u8; vault_crypto::AES_GCM_NONCE_LEN] = nonce_v
        .as_slice()
        .try_into()
        .map_err(|_| "nonce len".to_string())?;
    aead_encrypt(&sub, &nonce, fskey).map_err(|e| e.to_string())
}

fn send_json<S: std::io::Read + std::io::Write>(
    ch: &mut SecureChannel,
    stream: &mut S,
    msg: &Msg,
) -> Result<(), &'static str> {
    let bytes = serde_json::to_vec(msg).map_err(|_| "serialize")?;
    ch.send_msg(stream, &bytes)
}

fn recv_json<S: std::io::Read + std::io::Write>(
    ch: &mut SecureChannel,
    stream: &mut S,
) -> Result<Msg, &'static str> {
    let bytes = ch.recv_msg(stream)?;
    serde_json::from_slice(&bytes).map_err(|_| "deserialize")
}

fn leak_str(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

// ==== base64（标准字母表，自实现避免引依赖）====

pub(crate) fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub(crate) fn base64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = s.bytes().filter(|&c| c != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 {
            return None;
        }
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c)? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use vault_crypto::kdf::hkdf_sha256_derive;

    #[test]
    fn base64_roundtrip() {
        for len in [0usize, 1, 2, 3, 4, 255, 70000] {
            let data: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
            let enc = base64_encode(&data);
            assert_eq!(base64_decode(&enc).unwrap(), data);
        }
    }

    #[test]
    fn vector_clock_dominance() {
        let mut a = BTreeMap::new();
        let mut b = BTreeMap::new();
        a.insert("A".into(), 1);
        b.insert("B".into(), 1);
        assert!(!dominates(&a, &b)); // 并发
        let mut a2 = a.clone();
        a2.insert("A".into(), 2);
        a2.insert("B".into(), 1);
        assert!(dominates(&a2, &b));
        assert!(!dominates(&b, &a2));
        assert!(!dominates(&a, &a));
    }

    use std::path::PathBuf;

    #[test]
    fn manifest_fskey_manual_decrypt() {
        let dir = tempfile::tempdir().unwrap();
        let vp = dir.path().join("m.vsvb");
        std::fs::write(&vp, b"h").unwrap();
        let mk = vault_crypto::random_key();
        let keys = test_keys(&mk);
        let mut v = Vault::open(&vp, &mk, &keys.index, &keys.search).unwrap();
        v.set_device_id("vd-x");
        let src = dir.path().join("f.txt");
        std::fs::write(&src, b"manual decrypt probe ".repeat(5000)).unwrap();
        let id = v.import_file(&src, 0).unwrap();
        let json = v.sync_manifest().unwrap();
        let items: Vec<ManifestItem> = serde_json::from_str(&json).unwrap();
        let it = items.iter().find(|i| i.id == id).unwrap().clone();
        let fskey = v.fskey_bytes_for(id).unwrap();
        // 手动按 nonce 方案解密块 0
        let ct = v.read_chunk_ct(id, 0).unwrap();
        let mut nonce = [0u8; 12];
        nonce[..4].copy_from_slice(&it.nonce_prefix.to_be_bytes());
        nonce[4..].copy_from_slice(&0u64.to_be_bytes());
        let mut blob = nonce.to_vec();
        blob.extend_from_slice(&ct);
        let pt = vault_crypto::aead::aead_decrypt_with_aad(&fskey, &blob, &0u64.to_be_bytes());
        assert!(pt.is_some(), "清单派生 fskey 必须能解密块 0");
    }

    struct TestDev {
        dir: tempfile::TempDir,
        vault_path: PathBuf,
        slot: Arc<Mutex<Option<Vault>>>,
        engine: P2pEngine,
        mk: [u8; KEY_LEN],
        /// 擦除回调被调用次数（P5-5 投递/验签用例的断言点）。
        wipes: Arc<AtomicUsize>,
    }

    /// 擦除回调：计数 + 真删数据目录与保险箱文件（与 vault-core 注入的语义一致）。
    fn wipe_counter(wipes: Arc<AtomicUsize>, vp: PathBuf) -> WipeFn {
        Box::new(move || {
            wipes.fetch_add(1, Ordering::SeqCst);
            let stem = vp.file_stem().unwrap().to_str().unwrap().to_string();
            let data = vp.parent().unwrap().join(format!("{stem}.data"));
            let _ = std::fs::remove_dir_all(&data);
            let _ = std::fs::remove_file(&vp);
            Ok(())
        })
    }

    /// 测试密钥束：从属密钥取 V1.0 冻结公式（与正式 v2 兼容路径同值）。
    fn test_keys(mk: &[u8; KEY_LEN]) -> EngineKeys {
        EngineKeys {
            mk: *mk,
            orders: *hkdf_sha256_derive(mk, b"p2p-orders"),
            ident: *hkdf_sha256_derive(mk, b"p2p-ident"),
            index: *hkdf_sha256_derive(mk, b"vault-index"),
            search: *hkdf_sha256_derive(mk, b"search"),
        }
    }

    fn mk_dev(tag: &str) -> TestDev {
        let dir = tempfile::tempdir().unwrap();
        let vault_path = dir.path().join(format!("{tag}.vsvb"));
        std::fs::write(&vault_path, b"fake-header").unwrap();
        let mk = vault_crypto::random_key();
        let slot: Arc<Mutex<Option<Vault>>> = Arc::new(Mutex::new(None));
        let wipes = Arc::new(AtomicUsize::new(0));
        let wipe = wipe_counter(wipes.clone(), vault_path.clone());
        let keys = test_keys(&mk);
        let engine = P2pEngine::new(&vault_path, &keys, tag, slot.clone(), wipe).unwrap();
        // P7-10：远程锁定的执行体（与 vault-core p2p_service 同语义：清槽位）
        {
            let slot2 = slot.clone();
            engine.set_lock_fn(Box::new(move || {
                if let Ok(mut g) = slot2.lock() {
                    *g = None;
                }
            }));
        }
        TestDev {
            dir,
            vault_path,
            slot,
            engine,
            mk,
            wipes,
        }
    }

    /// 在同一临时目录上重开引擎：读回同一份 peers.enc / orders.enc（落盘往返断言用）。
    fn reopen(dev: &TestDev) -> P2pEngine {
        let keys = test_keys(&dev.mk);
        P2pEngine::new(
            &dev.vault_path,
            &keys,
            "reopened",
            dev.slot.clone(),
            wipe_counter(dev.wipes.clone(), dev.vault_path.clone()),
        )
        .unwrap()
    }

    /// 直接写入对端表模拟「已配对」（配对流程由 e2e 用例覆盖）。
    fn fake_peer(dev: &TestDev, id: &str, tag: char) {
        dev.engine
            .inner
            .peers
            .lock()
            .unwrap()
            .upsert(
                id,
                PeerRec {
                    name: id.to_string(),
                    pub_hex: tag.to_string().repeat(64),
                    paired_ms: 1,
                    counter: 0,
                    addr: None,
                    pq_cap: false,
                },
            )
            .unwrap();
    }

    fn import(dev: &TestDev, name: &str, data: &[u8]) -> u64 {
        import_into(dev, name, data, 0)
    }

    fn import_into(dev: &TestDev, name: &str, data: &[u8], folder: u64) -> u64 {
        let src = dev.dir.path().join(name);
        std::fs::write(&src, data).unwrap();
        let mut slot = dev.slot.lock().unwrap();
        slot.as_mut().unwrap().import_file(&src, folder).unwrap()
    }

    fn mkdir(dev: &TestDev, parent: u64, name: &str) -> u64 {
        let mut slot = dev.slot.lock().unwrap();
        slot.as_mut().unwrap().mkdir(parent, name).unwrap()
    }

    fn export_ok(dev: &TestDev, id: u64, expect: &[u8]) -> bool {
        let dest = dev.dir.path().join(format!("out-{id}.bin"));
        let ok = dev
            .slot
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .export_file(id, &dest)
            .is_ok();
        ok && std::fs::read(&dest).unwrap() == expect
    }

    fn addr_of(dev: &TestDev) -> String {
        format!("127.0.0.1:{}", dev.engine.port())
    }

    /// 轮询等待（响应端投递/收 ACK 在监听线程里异步完成，断言前需等它落地）。
    fn wait_until<F: Fn() -> bool>(cond: F, ms: u64) -> bool {
        let deadline = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < deadline {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        cond()
    }

    /// P7-8 端到端证据：配对与同步全程协商 suite 2（hybrid PQ），能力经 Hello 自愈登记，
    /// 删除同步最终在对端落地（对端应用与发起端返回是并发的，断言须有界等待——
    /// 这也是历史上「全量跑偶发失败」抖动的根因：发起端返回不等于对端已应用）。
    #[test]
    fn pq_pairing_suite2_and_delete_sync_e2e() {
        let a = mk_dev("pqA");
        let b = mk_dev("pqB");
        let invite = b.engine.pair_begin().unwrap();
        let code = invite["code"].as_str().unwrap().to_string();
        assert!(
            a.engine.pair_join(&addr_of(&b), &code).is_ok(),
            "双新端配对必须成功"
        );
        // 配对记录：对端能力已登记（Hello pq → pqCap），最近握手套件 = 2
        let st = b.engine.status();
        assert_eq!(st["pqSuite"], serde_json::json!(2), "pqSuite 必须可见");
        assert_eq!(
            st["peers"][0]["pqCap"],
            serde_json::json!(true),
            "对端 PQ 能力必须已登记"
        );

        // A 推送 → B 拉取（全程 suite 2 信道）
        let d1 = b"pq delete flow payload".repeat(2000);
        let f1 = import(&a, "pqfile.txt", &d1);
        let s1 = a.engine.sync_with(&addr_of(&b)).unwrap();
        assert_eq!(s1["pushed"].as_array().unwrap().len(), 1);

        // A 删除 → 同步 → B 最终必须应用（保护窗口内留加密副本，文件本体消失）
        {
            let mut slot = a.slot.lock().unwrap();
            slot.as_mut().unwrap().delete_file(f1, false).unwrap();
        }
        let s2 = a.engine.sync_with(&addr_of(&b)).unwrap();
        assert_eq!(s2["deleted"].as_array().unwrap().len(), 1);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut gone = false;
        while std::time::Instant::now() < deadline {
            let holds = {
                let slot = b.slot.lock().unwrap();
                !slot.as_ref().unwrap().has_file(f1)
            };
            if holds {
                gone = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(gone, "5s 内 B 必须应用对端删除");
        let evs = b.engine.status()["events"].clone();
        assert!(
            evs.as_array()
                .unwrap()
                .iter()
                .any(|e| e.as_str().unwrap_or("").contains("responder delete")),
            "B 侧必须留下删除应用事件: {evs}"
        );
    }

    #[test]
    fn pair_sync_incremental_conflict_destroy_e2e() {
        let a = mk_dev("devA");
        let b = mk_dev("devB");
        assert_ne!(a.engine.device_id(), b.engine.device_id());
        assert_eq!(a.engine.fingerprint().len(), 19);

        // 配对：B 出邀请码，A 加入；错误码必须被拒
        let _invite = b.engine.pair_begin().unwrap();
        let bad = a.engine.pair_join(&addr_of(&b), "wrong-code");
        assert!(bad.is_err(), "错误邀请码不得完成配对");
        let _ = b.engine.pair_begin().unwrap(); // 重新生成（前次可能已被错误尝试消耗）
        let invite = b.engine.pair_begin().unwrap();
        let code = invite["code"].as_str().unwrap().to_string();
        let joined = a.engine.pair_join(&addr_of(&b), &code);
        if joined.is_err() {
            println!("B events: {:?}", b.engine.status()["events"]);
            println!("A events: {:?}", a.engine.status()["events"]);
        }
        let joined = joined.unwrap();
        assert!(joined["peerId"].as_str().unwrap().starts_with("vd-"));

        // 初始同步：B 有 2 个文件 → A 拉取
        let d1 = b"Hello sync world! ".repeat(4000); // ~72KB，多块
        let d2 = b"binary\x00\x01\x02".repeat(100);
        let f1 = import(&b, "hello.txt", &d1);
        let f2 = import(&b, "blob.bin", &d2);
        let s1 = b.engine.sync_with(&addr_of(&a)).unwrap();
        assert_eq!(s1["pushed"].as_array().unwrap().len(), 2);
        assert!(export_ok(&a, f1, &d1));
        assert!(export_ok(&a, f2, &d2));
        // 双向各自的检索等不受影响；FSKey 覆盖存在
        {
            let slot = a.slot.lock().unwrap();
            assert!(slot.as_ref().unwrap().fskey_hex_for(f1).unwrap().len() == 64);
        }

        // 增量：A 导入新文件推给 B
        let d1b = b"Changed content for incremental sync!".repeat(3000);
        let f3 = import(&a, "newdoc.txt", &d1b);
        let s2 = a.engine.sync_with(&addr_of(&b)).unwrap();
        assert_eq!(s2["pushed"].as_array().unwrap().len(), 1);
        assert!(export_ok(&b, f3, &d1b));
        let d4 = b"created-on-b.txt".to_vec();
        let f4 = import(&b, "b-file.txt", &d4);
        let s3 = b.engine.sync_with(&addr_of(&a)).unwrap();
        assert_eq!(s3["pushed"].as_array().unwrap().len(), 1);
        assert!(export_ok(&a, f4, &d4));

        // 删除同步：A 删除 f3 → B 收到删除指令
        {
            let mut slot = a.slot.lock().unwrap();
            slot.as_mut().unwrap().delete_file(f3, false).unwrap();
        }
        let s4 = a.engine.sync_with(&addr_of(&b)).unwrap();
        assert_eq!(s4["deleted"].as_array().unwrap().len(), 1);
        // 对端应用与发起端返回并发：断言必须有界等待（发起端返回 ≠ 对端已应用）
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut gone = false;
        while std::time::Instant::now() < deadline {
            let holds = {
                let slot = b.slot.lock().unwrap();
                !slot.as_ref().unwrap().has_file(f3)
            };
            if holds {
                gone = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(gone, "5s 内 B 必须应用对端删除");

        // 冲突端到端说明：引擎当前无"原地编辑"操作（仅导入/删除/重命名），同 id 双端
        // 并发分叉在真实操作序列中暂不可达；分叉判定（dominates）与冲突副本/覆盖接收
        // 路径已由单元测试覆盖，端到端冲突用例待 P4 文件编辑功能落地后补充（LOG 记录）。

        // 远程销毁：A 对 B 下发延迟 1s 销毁，到期后 B 侧保险箱文件必须被擦除
        let target = b.engine.device_id();
        let sent = a
            .engine
            .destroy_arm_remote(&addr_of(&b), &target, 1)
            .unwrap();
        assert_eq!(sent["sent"], serde_json::json!(true));
        std::thread::sleep(std::time::Duration::from_millis(1800));
        assert!(!b.vault_path.exists(), "延迟销毁到期后保险箱文件必须被擦除");
        assert!(!b.dir.path().join("devB.data").exists());
    }

    #[test]
    fn unpair_removes_peer_and_logs_event() {
        let a = mk_dev("unpair-dev");
        // 直接写入对端表模拟「已配对」状态（配对流程由 e2e 用例覆盖）
        fake_peer(&a, "vd-ghost", 'c');
        assert!(a.engine.unpair("vd-ghost").unwrap(), "已配对设备解绑成功");
        assert!(
            !a.engine.unpair("vd-ghost").unwrap(),
            "重复解绑幂等 → false"
        );
        assert!(!a.engine.unpair("vd-nope").unwrap(), "不存在的设备 → false");
        let st = a.engine.status();
        assert_eq!(st["peers"].as_array().unwrap().len(), 0);
        let events = st["events"].as_array().unwrap();
        assert!(events.iter().any(|e| e
            .as_str()
            .unwrap_or("")
            .contains("unpair: removed vd-ghost")));
    }

    #[test]
    fn sync_via_relay_and_wrong_peer_rejected() {
        const TOKEN: &str = "vsr2-engine-e2e-token-0123456789abcdef";
        let a = mk_dev("rA");
        let b = mk_dev("rB");
        // 未配对设备直接同步 → 拒绝
        assert!(a.engine.sync_with(&addr_of(&b)).is_err());

        // VSR2 中继 mock：控制阶段按协议走（CHALLENGE→AUTH→OK，不校验 proof——
        // 服务端侧已有全量鉴权测试），配对同房间两端后双向转发（数据阶段透明）
        let relay = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let raddr = relay.local_addr().unwrap().to_string();
        std::thread::spawn(move || {
            use std::collections::HashMap;
            use std::io::BufRead;
            let mut rooms: HashMap<String, TcpStream> = HashMap::new();
            for stream in relay.incoming().flatten() {
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
                let _ =
                    s.write_all(b"VSR2 CHALLENGE aabbccdd00112233aabbccdd00112233 585021 5000\n");
                let mut auth = String::new();
                if r.read_line(&mut auth).is_err() {
                    continue;
                }
                assert!(auth.starts_with("VSR2 AUTH "));
                assert!(!auth.contains(TOKEN), "token 原值不得上线");
                let _ = s.write_all(b"VSR2 OK sess0000000000000000000000000000 0007\n");
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
                                let _ = std::io::copy(&mut first, &mut second);
                            });
                            let _ = std::io::copy(&mut s2, &mut f1);
                            let _ = up.join();
                        });
                    }
                }
            }
        });

        // 配对后：B 作为响应端经中继接入，A 作为发起端经中继同步
        let invite = b.engine.pair_begin().unwrap();
        let code = invite["code"].as_str().unwrap().to_string();
        a.engine.pair_join(&addr_of(&b), &code).unwrap();
        let data = b"relay relay relay".repeat(5000);
        let f = import(&b, "relay.txt", &data);
        let bengine = b.engine.clone();
        let raddr2 = raddr.clone();
        let server = std::thread::spawn(move || {
            bengine
                .serve_relay_connection(&raddr2, "room-1", TOKEN)
                .unwrap()
        });
        let s = a.engine.sync_via_relay(&raddr, "room-1", TOKEN).unwrap();
        assert_eq!(s["pulled"].as_array().unwrap().len(), 1);
        assert!(export_ok(&a, f, &data));
        let _ = server.join();

        // P8-5 路径记账：per-peer 段应登记 relay 尝试与成功路径 + 线上字节
        let ps = a.engine.path_stats();
        assert_eq!(ps["schema"], serde_json::json!(1));
        let peers = ps["peers"].as_array().unwrap();
        let rec = peers
            .iter()
            .find(|p| p["path"] == serde_json::json!("relay"))
            .expect("应有 relay 路径记录");
        assert!(rec["attempts"]["relay"].as_u64().unwrap() >= 1);
        assert!(rec["bytesIn"].as_u64().unwrap() > 0);
        assert!(rec["bytesOut"].as_u64().unwrap() > 0);
    }

    /// 计数 mock 中继：控制阶段按协议走（`err_line` 非空 = 在 CHALLENGE 位置
    /// 改发 ERR 行）；配对同房间两端后双向转发；返回地址 + HELLO 计数。
    fn counting_relay(
        err_line: Option<&'static str>,
    ) -> (std::net::SocketAddr, Arc<std::sync::atomic::AtomicUsize>) {
        let relay = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = relay.local_addr().unwrap();
        let conns = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c2 = Arc::clone(&conns);
        std::thread::spawn(move || {
            use std::collections::HashMap;
            use std::io::BufRead;
            let mut rooms: HashMap<String, TcpStream> = HashMap::new();
            for stream in relay.incoming().flatten() {
                let mut s = stream;
                let mut r = std::io::BufReader::new(s.try_clone().unwrap());
                let mut hello = String::new();
                if r.read_line(&mut hello).is_err() {
                    continue;
                }
                c2.fetch_add(1, Ordering::SeqCst);
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
                let _ = s.write_all(b"VSR2 OK sess0000000000000000000000000000 0007\n");
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
                                let _ = std::io::copy(&mut first, &mut second);
                            });
                            let _ = std::io::copy(&mut s2, &mut f1);
                            let _ = up.join();
                        });
                    }
                }
            }
        });
        (addr, conns)
    }

    /// P8-5 收尾（候选顺序 + 空候选不拨号）：① 空候选 → `13 + relay_unavailable`
    /// 且**任何中继都未被拨号**（`allowPublicRelay = false` 过滤后为空即此形态）；
    /// ② 按配置顺序尝试——#1 鉴权失败（R5，不自动重试）→ #2 成功完成同步。
    #[test]
    fn relay_candidates_try_in_order_and_empty_list_never_dials() {
        const TOKEN: &str = "vsr2-candidates-token-0123456789ab";
        let a = mk_dev("candA");
        let b = mk_dev("candB");
        let (bad_addr, bad_conns) = counting_relay(Some("VSR2 ERR 5 auth-failed"));
        let (good_addr, good_conns) = counting_relay(None);

        let invite = b.engine.pair_begin().unwrap();
        let code = invite["code"].as_str().unwrap().to_string();
        a.engine.pair_join(&addr_of(&b), &code).unwrap();

        // 空候选：不拨号、13 + relay_unavailable
        let e = a
            .engine
            .sync_via_relay_candidates(&[], "room-c0")
            .unwrap_err();
        assert_eq!(e.local_code, 13);
        assert_eq!(e.degrade_reason, Some("relay_unavailable"));
        assert_eq!(
            bad_conns.load(Ordering::SeqCst),
            0,
            "空候选不得发起任何连接"
        );
        assert_eq!(good_conns.load(Ordering::SeqCst), 0);

        // 顺序尝试：#1 鉴权失败 → #2 成功
        let data = b"candidates candidates".repeat(3000);
        let f = import(&b, "cand.txt", &data);
        let bengine = b.engine.clone();
        let gaddr = good_addr.to_string();
        let server = std::thread::spawn(move || {
            bengine
                .serve_relay_candidates(&[(gaddr, TOKEN.to_string())], "room-c1")
                .unwrap()
        });
        let cands = vec![
            (bad_addr.to_string(), TOKEN.to_string()),
            (good_addr.to_string(), TOKEN.to_string()),
        ];
        let s = a
            .engine
            .sync_via_relay_candidates(&cands, "room-c1")
            .unwrap();
        assert_eq!(s["pulled"].as_array().unwrap().len(), 1);
        assert!(export_ok(&a, f, &data));
        let _ = server.join();
        assert!(
            bad_conns.load(Ordering::SeqCst) >= 1,
            "坏中继必须被尝试过（顺序尝试而非跳过）"
        );
        assert!(good_conns.load(Ordering::SeqCst) >= 2, "好中继必须承接两端");
    }

    /// P8-5 负向：错 token → 中继拒（R5）→ 本机码 3 + 可读诊断 + 未归属失败入档。
    #[test]
    fn sync_via_relay_wrong_token_fails_with_mapped_error() {
        let a = mk_dev("rW");
        // 无对端：连一个按协议拒 proof 的 mock（服务端侧已有全量鉴权测试）
        let relay = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let raddr = relay.local_addr().unwrap().to_string();
        std::thread::spawn(move || {
            use std::io::BufRead;
            for stream in relay.incoming().flatten() {
                let mut s = stream;
                let mut r = std::io::BufReader::new(s.try_clone().unwrap());
                let mut hello = String::new();
                if r.read_line(&mut hello).is_err() {
                    continue;
                }
                let _ =
                    s.write_all(b"VSR2 CHALLENGE aabbccdd00112233aabbccdd00112233 585021 5000\n");
                let mut auth = String::new();
                let _ = r.read_line(&mut auth);
                let _ = s.write_all(b"VSR2 ERR 5 auth-failed\n");
            }
        });
        let err = a
            .engine
            .sync_via_relay(&raddr, "room-w", "vsr2-engine-e2e-token-0123456789abcdef")
            .unwrap_err();
        assert_eq!(err.relay_code, 5);
        assert_eq!(err.local_code, 3);
        assert_eq!(err.degrade_reason, Some("relay_unavailable"));
        assert!(err.diag.contains("auth-failed"));
        // 未归属路径失败已入引擎状态（事件日志含诊断）
        let st = a.engine.status();
        assert!(st["lastPathFailure"]["reason"] == serde_json::json!("auth-failed"));
        let events = st["events"].as_array().unwrap();
        assert!(
            events
                .iter()
                .any(|e| e.as_str().unwrap_or("").contains("auth-failed")),
            "可读诊断必须进引擎事件日志"
        );
    }

    /// P8-5 参数前置拦截：坏房间名 / 短 token → 本机码 7（不发起连接）。
    #[test]
    fn sync_via_relay_invalid_args_rejected_locally() {
        let a = mk_dev("rI");
        let err = a
            .engine
            .sync_via_relay(
                "127.0.0.1:1",
                "bad room!",
                "vsr2-engine-e2e-token-0123456789",
            )
            .unwrap_err();
        assert_eq!(err.local_code, 7);
        let err = a
            .engine
            .sync_via_relay("127.0.0.1:1", "room-i", "short")
            .unwrap_err();
        assert_eq!(err.local_code, 7);
    }

    // ==== P8-7 选择性同步过滤 ====

    /// 证据（面板 P8-7 `sync_policy::filter_applies_at_enqueue`，**负向**）：
    /// 被排除文件夹（含子树）的文件的块**一次也不进入传输**——对端拿不到该文件，
    /// 且本端以 `skipped[reason=folder_excluded]` 上报（否证「只在 UI 层过滤」）。
    #[test]
    fn filter_applies_at_enqueue_and_reports_skipped() {
        use crate::policy::{SyncFilter, SKIP_FOLDER_EXCLUDED};
        let a = mk_dev("fA");
        let b = mk_dev("fB");
        let invite = b.engine.pair_begin().unwrap();
        let code = invite["code"].as_str().unwrap().to_string();
        a.engine.pair_join(&addr_of(&b), &code).unwrap();

        let folder = mkdir(&a, 0, "excluded");
        let sub = mkdir(&a, folder, "sub"); // 子树递归
        let f_excl = import_into(&a, "hidden.bin", b"secret-in-excluded-folder!", folder);
        let f_sub = import_into(&a, "deep.bin", b"also excluded by subtree", sub);
        let f_root = import(&a, "public.bin", b"this one should sync");

        a.engine.set_sync_filter(SyncFilter {
            enabled: true,
            folder_exclude: vec![folder],
            ..Default::default()
        });
        let s = a.engine.sync_with(&addr_of(&b)).unwrap();
        let skipped = s["skipped"].as_array().unwrap();
        let ids: Vec<u64> = skipped.iter().filter_map(|v| v["id"].as_u64()).collect();
        assert!(ids.contains(&f_excl), "排除文件夹内的文件必须被跳过");
        assert!(ids.contains(&f_sub), "子树文件必须被跳过（递归）");
        for v in skipped {
            assert_eq!(v["reason"], serde_json::json!(SKIP_FOLDER_EXCLUDED));
        }
        assert!(s["pushed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_u64() == Some(f_root)));
        // 对端确实没收到被排除的文件（块从未出站）
        {
            let slot = b.slot.lock().unwrap();
            let v = slot.as_ref().unwrap();
            assert!(!v.has_file(f_excl), "被排除文件的块不得到达对端");
            assert!(!v.has_file(f_sub), "子树排除同样不得到达对端");
            assert!(v.has_file(f_root));
        }
        // 引擎快照可重建（`p2p_status().skipped`）
        let sk = a.engine.skipped();
        assert!(sk.as_array().unwrap().len() >= 2);
        assert!(export_ok(&b, f_root, b"this one should sync"));
    }

    /// 证据（`sync_policy::net_type_recheck_before_egress`）：入队后网络类型变化
    /// → 出站前拦下（网络类型在传输开始时才确定）。
    #[test]
    fn net_type_recheck_before_egress() {
        use crate::policy::{SyncContext, SyncFilter, SKIP_NET_TYPE_BLOCKED};
        let a = mk_dev("nA");
        let b = mk_dev("nB");
        let invite = b.engine.pair_begin().unwrap();
        let code = invite["code"].as_str().unwrap().to_string();
        a.engine.pair_join(&addr_of(&b), &code).unwrap();
        let f = import(&a, "net.bin", b"net-type-gated payload");
        a.engine.set_sync_filter(SyncFilter {
            enabled: true,
            net_types: vec!["wifi".into()],
            ..Default::default()
        });
        a.engine.set_sync_context(SyncContext {
            in_window: true,
            net_type: "cellular".into(),
        });
        let s = a.engine.sync_with(&addr_of(&b)).unwrap();
        assert!(s["pushed"].as_array().unwrap().is_empty(), "蜂窝下不得出站");
        assert_eq!(
            s["skipped"][0]["reason"],
            serde_json::json!(SKIP_NET_TYPE_BLOCKED)
        );
        // 切回 WiFi → 正常出站
        a.engine.set_sync_context(SyncContext {
            in_window: true,
            net_type: "wifi".into(),
        });
        let s = a.engine.sync_with(&addr_of(&b)).unwrap();
        assert!(s["pushed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_u64() == Some(f)));
        assert!(export_ok(&b, f, b"net-type-gated payload"));
    }

    /// 时段外（外壳判定）→ `window_closed`；策略未启用则一律全同步。
    #[test]
    fn window_closed_and_disabled_policy() {
        use crate::policy::{SyncContext, SyncFilter, SKIP_WINDOW_CLOSED};
        let a = mk_dev("wA");
        let b = mk_dev("wB");
        let invite = b.engine.pair_begin().unwrap();
        let code = invite["code"].as_str().unwrap().to_string();
        a.engine.pair_join(&addr_of(&b), &code).unwrap();
        let f = import(&a, "win.bin", b"window gated payload");
        a.engine.set_sync_filter(SyncFilter {
            enabled: true,
            ..Default::default()
        });
        a.engine.set_sync_context(SyncContext {
            in_window: false,
            net_type: "wifi".into(),
        });
        let s = a.engine.sync_with(&addr_of(&b)).unwrap();
        assert!(s["skipped"][0]["reason"] == serde_json::json!(SKIP_WINDOW_CLOSED));
        assert!(s["pushed"].as_array().unwrap().is_empty());
        // 策略未启用（默认值）→ 全同步（策略读取失败时的口径）
        a.engine.set_sync_filter(SyncFilter::default());
        let s = a.engine.sync_with(&addr_of(&b)).unwrap();
        assert!(s["pushed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_u64() == Some(f)));
    }

    // ==== P8-8 原地编辑与块级三方合并 ====

    /// 原地编辑（测试辅助）：以同一 file_id 提交新版本，返回新 rev。
    fn edit(dev: &TestDev, id: u64, name: &str, data: &[u8]) -> u64 {
        let src = dev.dir.path().join(name);
        std::fs::write(&src, data).unwrap();
        let mut slot = dev.slot.lock().unwrap();
        slot.as_mut().unwrap().apply_edit(id, &src, 1).unwrap()
    }

    /// 面板证据（P8-8）：`conflict::in_place_edit_creates_conflict`——
    /// 两台设备从同一基线各原地编辑同一文件（改动**不相交**）→ 同步后自动块级合并
    /// （rev 递增、**无冲突副本**、合并结果含两侧改动）；相交改动 → 产生冲突副本。
    ///
    /// **`#[ignore]` 已移除（2026-09-24 修复）**：缺陷根因是合并判定块被放在
    /// 「覆盖冲突」块之后（覆盖路径先摘除本端条目 → 合并看不到本地版本），
    /// 现已把判定前移并加顺序约束注释。
    #[test]
    fn in_place_edit_auto_merges_and_conflicts() {
        let a = mk_dev("cA");
        let b = mk_dev("cB");
        let invite = b.engine.pair_begin().unwrap();
        let code = invite["code"].as_str().unwrap().to_string();
        a.engine.pair_join(&addr_of(&b), &code).unwrap();
        // 基线文件 300 KB（CDC 会分出多段，便于验证「按块」而不是「整文件」）
        let base: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let fid = import(&a, "doc.bin", &base);
        a.engine.sync_with(&addr_of(&b)).unwrap();
        assert!(export_ok(&b, fid, &base), "基线必须先在两端一致");

        // ---- 场景一：不相交改动 → 自动合并 ----
        let mut la = base.clone();
        for x in la[..2000].iter_mut() {
            *x ^= 0x5A; // A 改头部
        }
        let mut lb = base.clone();
        let n = lb.len();
        for x in lb[n - 2000..].iter_mut() {
            *x ^= 0xA5; // B 改尾部
        }
        assert_eq!(edit(&a, fid, "a1.bin", &la), 1, "A 编辑后 rev = 1");
        assert_eq!(edit(&b, fid, "b1.bin", &lb), 1, "B 编辑后 rev = 1");
        // A 发起同步（分叉：sha 不同即分叉）；谁推谁拉取决于 modified_ms，
        // 故断言对「接收端合并」与「随后收敛」都成立
        let s = a.engine.sync_with(&addr_of(&b)).unwrap();
        assert!(
            s["conflicts"].as_array().unwrap().is_empty(),
            "不相交改动不得记成冲突：{s}"
        );
        assert!(
            !s["merged"].as_array().unwrap().is_empty(),
            "应发生块级自动合并：{s}"
        );
        // 合并落在**接收端**（rev+1）；两侧内容此轮可能尚未一致，但合并侧必须含双方改动
        let (rev_a, cont_a) = export_with_rev(&a, fid);
        let (rev_b, cont_b) = export_with_rev(&b, fid);
        assert!(
            rev_a.max(rev_b) >= 2,
            "自动合并应提交为新 rev（A={rev_a} B={rev_b}）"
        );
        let merged_side = if rev_a >= rev_b { &cont_a } else { &cont_b };
        assert_eq!(
            &merged_side[..2000],
            &la[..2000],
            "A 的头部改动必须在合并结果中"
        );
        assert_eq!(
            &merged_side[n - 2000..],
            &lb[n - 2000..],
            "B 的尾部改动必须在合并结果中"
        );
        assert_eq!(merged_side.len(), n);
        // 再同步一轮：两端收敛到同一内容，且全程无冲突副本
        b.engine.sync_with(&addr_of(&a)).unwrap();
        let (_, cont_a2) = export_with_rev(&a, fid);
        let (_, cont_b2) = export_with_rev(&b, fid);
        assert_eq!(cont_a2, cont_b2, "两端内容必须收敛一致");
        assert_eq!(cont_a2, *merged_side, "收敛结果即合并结果");
        assert!(
            !has_conflict_copy(&a) && !has_conflict_copy(&b),
            "不相交改动全程不得产生冲突副本（自动块级合并）"
        );

        // ---- 场景二：相交改动 → 冲突副本 ----
        let fid2 = import(&a, "doc2.bin", &base);
        a.engine.sync_with(&addr_of(&b)).unwrap();
        let mut xa = base.clone();
        for x in xa[..3000].iter_mut() {
            *x ^= 0x11; // 同一区域
        }
        let mut xb = base.clone();
        for x in xb[..3000].iter_mut() {
            *x ^= 0x22; // 同一区域（改法不同）
        }
        edit(&a, fid2, "a2.bin", &xa);
        edit(&b, fid2, "b2.bin", &xb);
        let s2 = a.engine.sync_with(&addr_of(&b)).unwrap();
        assert!(
            !s2["conflicts"].as_array().unwrap().is_empty(),
            "相交改动必须记为冲突：{s2}"
        );
        // 冲突副本落在**接收端**（谁推谁拉取决于 modified_ms）——两端任一留下副本即可
        assert!(
            has_conflict_copy(&a) || has_conflict_copy(&b),
            "相交改动必须留下冲突副本（两侧都无 = 静默丢失）"
        );
        assert!(
            s2["merged"].as_array().unwrap().is_empty(),
            "相交改动不得报告为已合并：{s2}"
        );
    }

    /// 导出文件并读取其 `rev`（测试辅助）。
    fn export_with_rev(dev: &TestDev, id: u64) -> (u64, Vec<u8>) {
        let dest = dev.dir.path().join(format!("rev-out-{id}.bin"));
        let slot = dev.slot.lock().unwrap();
        let v = slot.as_ref().unwrap();
        v.export_file(id, &dest).unwrap();
        let rev = v.file_rev(id).unwrap();
        (rev, std::fs::read(&dest).unwrap())
    }

    /// 是否存在冲突副本（`*.conflict-*`）。
    fn has_conflict_copy(dev: &TestDev) -> bool {
        let json = {
            let slot = dev.slot.lock().unwrap();
            slot.as_ref().unwrap().sync_manifest().unwrap()
        };
        json.contains(".conflict-")
    }

    // ==== P5-5 全设备联动销毁：离线设备高优先级信令队列 ====

    /// 单元 1：队列落盘往返（重开同一目录仍在）、`cancel_pending_orders` 生效、
    /// `pending_orders` 结构正确、队列文件为密文。
    #[test]
    fn destroy_orders_queue_roundtrip_and_cancel() {
        let a = mk_dev("qA");
        fake_peer(&a, "vd-peer-1", 'a');
        fake_peer(&a, "vd-peer-2", 'b');

        // 入队：不依赖任何对端可达性（这正是离线设备场景）
        let q = a.engine.destroy_queue_all(0).unwrap();
        assert_eq!(q["queued"], serde_json::json!(2));
        let peers = q["peers"].as_array().unwrap();
        assert_eq!(peers.len(), 2);
        assert!(peers.iter().any(|p| p == "vd-peer-1"));

        // pending_orders 结构：{count, items:[{peer,issuedMs,delaySecs}]}，且不得含签名/载荷
        let p = a.engine.pending_orders();
        assert_eq!(p["count"], serde_json::json!(2));
        let items = p["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        for it in items {
            assert!(it["peer"].as_str().unwrap().starts_with("vd-peer-"));
            assert!(it["issuedMs"].as_u64().unwrap() > 0);
            assert_eq!(it["delaySecs"], serde_json::json!(0));
            assert!(it.get("sig_hex").is_none() && it.get("payload").is_none());
        }

        // 重开同一目录：队列必须在（AEAD 落盘）
        let reopened = reopen(&a);
        assert_eq!(reopened.pending_orders()["count"], serde_json::json!(2));
        // 队列文件不得明文出现对端 id（键 HKDF(MK,"p2p-orders") 封装）
        let blob = std::fs::read(a.dir.path().join("qA.data/p2p/orders.enc")).unwrap();
        let needle = b"vd-peer-1";
        assert!(
            !blob.windows(needle.len()).any(|w| w == needle.as_slice()),
            "队列条目不得明文落盘"
        );

        // 取消投递（UI「取消投递」）：清空并持久化，重复调用幂等
        assert_eq!(reopened.cancel_pending_orders(), 2);
        assert_eq!(reopened.pending_orders()["count"], serde_json::json!(0));
        assert_eq!(reopened.cancel_pending_orders(), 0);
        assert_eq!(reopen(&a).pending_orders()["count"], serde_json::json!(0));

        // 延迟语义：delay_secs 记入条目，且重复入队同目标只保留最新一条（不叠加）
        let d1 = a.engine.destroy_queue_all(60).unwrap();
        assert_eq!(d1["queued"], serde_json::json!(2));
        assert_eq!(
            a.engine.pending_orders()["items"][0]["delaySecs"],
            serde_json::json!(60)
        );
        assert_eq!(a.engine.pending_orders()["count"], serde_json::json!(2));
        assert_eq!(a.engine.cancel_pending_orders(), 2);

        // 无已配对设备 → 空队列（不报错，UI 可提示"无设备可联动"）
        let b = mk_dev("qB");
        assert_eq!(
            b.engine.destroy_queue_all(0).unwrap()["queued"],
            serde_json::json!(0)
        );
        assert_eq!(b.engine.pending_orders()["count"], serde_json::json!(0));
        assert_eq!(b.engine.cancel_pending_orders(), 0);
    }

    /// 单元 1b：`remember_peer_addr`（UI 兜底手填）与 `status().peers[].addr`。
    #[test]
    fn peer_addr_remember_and_status_exposes_it() {
        let a = mk_dev("addrA");
        fake_peer(&a, "vd-p1", 'a');
        assert_eq!(
            a.engine.status()["peers"][0]["addr"],
            serde_json::json!(null)
        );
        a.engine
            .remember_peer_addr("vd-p1", "192.168.1.7:4100")
            .unwrap();
        assert_eq!(
            a.engine.status()["peers"][0]["addr"],
            serde_json::json!("192.168.1.7:4100")
        );
        // 未登记对端不得被凭空创建
        assert!(a
            .engine
            .remember_peer_addr("vd-nope", "10.0.0.1:1")
            .is_err());
        assert_eq!(a.engine.status()["peers"].as_array().unwrap().len(), 1);
        // 重开后仍在
        assert_eq!(
            reopen(&a).status()["peers"][0]["addr"],
            serde_json::json!("192.168.1.7:4100")
        );
    }

    /// 单元 2：投递帧的验签路径——错误签名 / 错误 target / 非法载荷一律拒绝且**不擦除**；
    /// 合法指令（延迟）只武装、合法指令（0 延迟）才擦除（阳性对照）。
    #[test]
    fn destroy_order_frame_rejected_when_signature_or_target_wrong() {
        let a = mk_dev("ordA");
        let b = mk_dev("ordB");
        let c = mk_dev("ordC");
        let a_pub = a.engine.inner.ident.public_hex();
        let b_id = b.engine.device_id();
        let a_id = a.engine.device_id();
        let frame = |target: &str, delay_ms: u64, ts: u64, sig: &str| {
            serde_json::to_string(&Msg::DestroyCmd {
                target: target.to_string(),
                delay_ms,
                ts_ms: ts,
                sig: sig.to_string(),
            })
            .unwrap()
        };
        let sign = |target: &str, delay_ms: u64, ts: u64| {
            a.engine
                .inner
                .ident
                .sign(&destroy_sign_body(target, delay_ms, ts))
        };

        // (1) 签名与签名体不符（同一把私钥签了别的内容）
        let bad = a.engine.inner.ident.sign(b"vsync-destroy:vd-other:0:1");
        assert_eq!(
            b.engine
                .apply_order_payload(&a_pub, &frame(&b_id, 0, 1, &bad)),
            Err(OrderReject::BadSignature)
        );
        // (2) 第三者冒充（用另一把私钥签正确的签名体）
        let rogue = Identity::generate();
        let forged = rogue.sign(&destroy_sign_body(&b_id, 0, 1));
        assert_eq!(
            b.engine
                .apply_order_payload(&a_pub, &frame(&b_id, 0, 1, &forged)),
            Err(OrderReject::BadSignature)
        );
        // (3) 合法签名但目标不是本机
        let sig_other = sign(&a_id, 0, 1);
        assert_eq!(
            b.engine
                .apply_order_payload(&a_pub, &frame(&a_id, 0, 1, &sig_other)),
            Err(OrderReject::TargetMismatch)
        );
        // (4) 载荷不是销毁指令 / 不是 JSON
        assert_eq!(
            b.engine.apply_order_payload(&a_pub, "{\"SyncDone\":null}"),
            Err(OrderReject::Malformed)
        );
        assert_eq!(
            b.engine.apply_order_payload(&a_pub, "not-json"),
            Err(OrderReject::Malformed)
        );

        // 以上全部拒绝路径都不得触发擦除，也不得武装倒计时
        assert_eq!(b.wipes.load(Ordering::SeqCst), 0, "验签/目标不符不得擦除");
        assert!(b.vault_path.exists());
        assert_eq!(b.engine.status()["armedDestroy"], serde_json::json!(null));
        let ev = b.engine.status()["events"].clone().to_string();
        assert!(ev.contains("signature invalid"), "{ev}");
        assert!(ev.contains("target mismatch"), "{ev}");

        // (5) 阳性对照：合法指令 delay>0 → 只武装，不擦除
        let ts = now_ms();
        let sig_delay = sign(&b_id, 60_000, ts);
        assert_eq!(
            b.engine
                .apply_order_payload(&a_pub, &frame(&b_id, 60_000, ts, &sig_delay)),
            Ok(true)
        );
        assert_eq!(b.wipes.load(Ordering::SeqCst), 0, "延迟销毁只武装");
        assert_eq!(
            b.engine.status()["armedDestroy"]["target"],
            serde_json::json!(b_id)
        );
        assert!(b.engine.destroy_cancel(), "武装可取消");

        // (6) 阳性对照：合法指令 delay=0 → 立即擦除（证明上面不是"永远拒绝"）
        let ts = now_ms();
        let sig_now = sign(&b_id, 0, ts);
        assert_eq!(
            b.engine
                .apply_order_payload(&a_pub, &frame(&b_id, 0, ts, &sig_now)),
            Ok(false)
        );
        assert_eq!(b.wipes.load(Ordering::SeqCst), 1);
        assert!(!b.vault_path.exists());
        assert!(!b.dir.path().join("ordB.data").exists());
        // 未受影响的第三方设备不得被擦除
        assert_eq!(c.wipes.load(Ordering::SeqCst), 0);
        assert!(c.vault_path.exists());
    }

    /// 集成 A（方向：**B 拨 A**，A 为响应端）：覆盖"对端拨入时投递"分支。
    ///
    /// A 在 B 离线（不可达，A 侧不去连接）期间 `destroy_queue_all(0)` 入队 → B 上线主动向 A
    /// 发起同步 → A 在 Hello 之后、清单交换之前投递 → B 验签后立即擦除 → A 收到 ACK 后队列清空。
    #[test]
    fn offline_destroy_order_delivered_when_peer_dials_in() {
        let a = mk_dev("qdirA");
        let b = mk_dev("qdirB");

        // 配对：B 出邀请码，A 拨入 B（pair_join 方向记下 B 的地址）
        let code = b.engine.pair_begin().unwrap()["code"]
            .as_str()
            .unwrap()
            .to_string();
        a.engine.pair_join(&addr_of(&b), &code).unwrap();
        assert_eq!(
            a.engine.status()["peers"][0]["addr"],
            serde_json::json!(addr_of(&b)),
            "pair_join 应记下拨出的对端地址（可回拨）"
        );
        assert_eq!(
            b.engine.status()["peers"][0]["addr"],
            serde_json::json!(addr_of(&a)),
            "响应端应记下对端 Hello 声明的监听端口 + 源 IP"
        );

        // A 在 B 不可达时入队（不发起任何连接）
        assert_eq!(
            a.engine.destroy_queue_all(0).unwrap()["queued"],
            serde_json::json!(1)
        );
        assert_eq!(a.engine.pending_orders()["count"], serde_json::json!(1));
        assert_eq!(b.wipes.load(Ordering::SeqCst), 0);

        // B 上线：主动向 A 发起一次同步（此方向 B 为发起端、A 为响应端）
        let sync = b.engine.sync_with(&addr_of(&a));
        println!("B sync result after remote wipe: {sync:?}");

        // B 的擦除回调必须被调用，保险箱与数据目录必须消失
        assert_eq!(
            b.wipes.load(Ordering::SeqCst),
            1,
            "投递的销毁指令必须执行擦除"
        );
        assert!(!b.vault_path.exists());
        assert!(!b.dir.path().join("qdirB.data").exists());
        // A 侧队列清空（收到 ACK 才移除），且记住的地址未被清掉
        assert!(
            wait_until(
                || a.engine.pending_orders()["count"] == serde_json::json!(0),
                5000
            ),
            "A 队列应在收到 ACK 后清空，实际 {}",
            a.engine.pending_orders()
        );
        assert_eq!(
            a.engine.status()["peers"][0]["addr"],
            serde_json::json!(addr_of(&b))
        );
        // 事件时间线：A 记投递、B 记执行
        let ea = a.engine.status()["events"].clone().to_string();
        let eb = b.engine.status()["events"].clone().to_string();
        assert!(
            ea.contains("destroy order delivered to"),
            "A 事件缺投递记录: {ea}"
        );
        assert!(
            eb.contains("destroy order accepted: immediate wipe"),
            "B 事件缺执行记录: {eb}"
        );
    }

    /// 集成 B（方向：**A 拨 B**，A 为发起端）：覆盖"我方主动 sync_with 时投递"分支。
    /// 用 delay>0 避免擦除，从而断言"武装"而非"擦除"（投递语义与方向无关）。
    #[test]
    fn offline_destroy_order_delivered_when_we_dial_out() {
        let a = mk_dev("qoutA");
        let b = mk_dev("qoutB");
        let code = b.engine.pair_begin().unwrap()["code"]
            .as_str()
            .unwrap()
            .to_string();
        a.engine.pair_join(&addr_of(&b), &code).unwrap();

        // A 入队一条延迟 1 小时的销毁指令（不擦除，只武装）
        assert_eq!(
            a.engine.destroy_queue_all(3600).unwrap()["queued"],
            serde_json::json!(1)
        );

        // A 主动拨 B：A 作为发起端在 Hello 后、清单交换前投递；B 回 ACK 后同步照常完成
        let s = a.engine.sync_with(&addr_of(&b)).unwrap();
        assert_eq!(s["pushed"].as_array().unwrap().len(), 0);

        assert_eq!(b.wipes.load(Ordering::SeqCst), 0, "延迟销毁只武装不擦除");
        assert_eq!(
            b.engine.status()["armedDestroy"]["target"],
            serde_json::json!(b.engine.device_id())
        );
        assert_eq!(a.engine.pending_orders()["count"], serde_json::json!(0));
        let ea = a.engine.status()["events"].clone().to_string();
        assert!(
            ea.contains("destroy order delivered to"),
            "A 事件缺投递记录: {ea}"
        );
        let eb = b.engine.status()["events"].clone().to_string();
        assert!(
            eb.contains("destroy order accepted: armed for"),
            "B 事件缺武装记录: {eb}"
        );
        assert!(b.engine.destroy_cancel());
    }

    /// P7-11 证据：destroy_order::armed_survives_restart
    /// 武装状态落盘：引擎重开（=应用重启）后倒计时续走、到期即执行。
    #[test]
    fn armed_survives_restart() {
        // 直接构造 orders.enc 的武装记录（接收端语义；签名验签由 e2e 覆盖）
        let b = mk_dev("armR");
        let rec = crate::orders::ArmedDestroyRec {
            schema: 1,
            target: b.engine.device_id(),
            delay_ms: 60_000,
            armed_ms: 0,
            deadline_ms: now_ms() + 60_000,
            order_issued_ms: 0,
            nonce: "n".into(),
        };
        b.engine
            .inner
            .orders
            .lock()
            .unwrap()
            .set_armed(Some(rec))
            .unwrap();

        // 重开（等价重启）：倒计时续走
        let b2 = reopen(&b);
        assert_eq!(b.wipes.load(Ordering::SeqCst), 0, "未到期不得执行");
        let armed = b2.status()["armedDestroy"].clone();
        assert!(armed.is_object(), "重启后武装必须恢复：{armed}");
        let remain = armed["remainingMs"].as_u64().unwrap_or(0);
        assert!(remain > 50_000 && remain <= 60_000, "remaining={remain}");
        assert!(b2.status()["events"].to_string().contains("armed_resumed"));

        // 到期即执行：写一条已过期的武装记录 → 重开 → 立即擦除
        let expired = crate::orders::ArmedDestroyRec {
            schema: 1,
            target: b.engine.device_id(),
            delay_ms: 1,
            armed_ms: 0,
            deadline_ms: now_ms().saturating_sub(1_000),
            order_issued_ms: 0,
            nonce: "n2".into(),
        };
        b.engine
            .inner
            .orders
            .lock()
            .unwrap()
            .set_armed(Some(expired))
            .unwrap();
        let b3 = reopen(&b);
        assert_eq!(
            b.wipes.load(Ordering::SeqCst),
            1,
            "已过期的武装在启动时立即执行"
        );
        assert!(b3.status()["events"]
            .to_string()
            .contains("armed_expired_executed"));

        // 取消幂等落盘：清空后重开不再恢复
        let _ = b3.destroy_cancel();
        let b4 = reopen(&b);
        assert_eq!(b4.status()["armedDestroy"], serde_json::json!(null));
    }

    /// P7-10 证据：lock::remote_lock_signature_and_target_checked
    /// 锁定指令四条校验（签名/target/时钟偏差），执行回调被触发。
    #[test]
    fn remote_lock_signature_and_target_checked() {
        let a = mk_dev("lockA");
        let b = mk_dev("lockB");
        let code = b.engine.pair_begin().unwrap()["code"]
            .as_str()
            .unwrap()
            .to_string();
        a.engine.pair_join(&addr_of(&b), &code).unwrap();

        // push_lock 全部对端 → 入队 kind=2
        let q = a.engine.push_lock(None).unwrap();
        assert_eq!(q["queued"], serde_json::json!(1));
        let orders = a.engine.pending_orders();
        assert_eq!(orders["count"], serde_json::json!(1));

        // 投递：载荷是 {"LockCmd":{...}}，接收端验签 + target + 时钟
        let rec_payload = {
            let store = a.engine.inner.orders.lock().unwrap();
            store.list_for(&b.engine.device_id())[0].payload.clone()
        };
        let a_pub = a.engine.inner.ident.public_hex();
        let before_wipes = b.wipes.load(Ordering::SeqCst);

        // (1) 合法指令 → 锁定回调被触发（擦除计数不增，但 vault 槽位被清）
        b.engine
            .apply_order_payload(&a_pub, &rec_payload)
            .expect("valid lock order");
        assert_eq!(b.wipes.load(Ordering::SeqCst), before_wipes, "锁定不是擦除");
        assert!(
            b.slot.lock().unwrap().is_none(),
            "远程锁定必须清空保险箱槽位（密钥材料离手）"
        );

        // (2) 伪造签名 → 拒绝
        let rogue = Identity::generate();
        let ts = now_ms();
        let forged = format!(
            "{{\"LockCmd\":{{\"target\":\"{}\",\"delay_ms\":0,\"ts_ms\":{},\"sig\":\"{}\"}}}}",
            b.engine.device_id(),
            ts,
            rogue.sign(format!("vsync-lock:{}:0:{ts}", b.engine.device_id()).as_bytes())
        );
        assert_eq!(
            b.engine.apply_order_payload(&a_pub, &forged),
            Err(OrderReject::BadSignature)
        );

        // (3) 时钟偏差 > 300s → 拒绝
        let stale_ts = now_ms() - 400_000;
        let stale = format!(
            "{{\"LockCmd\":{{\"target\":\"{}\",\"delay_ms\":0,\"ts_ms\":{},\"sig\":\"{}\"}}}}",
            b.engine.device_id(),
            stale_ts,
            a.engine
                .inner
                .ident
                .sign(format!("vsync-lock:{}:0:{stale_ts}", b.engine.device_id()).as_bytes())
        );
        assert_eq!(
            b.engine.apply_order_payload(&a_pub, &stale),
            Err(OrderReject::Malformed)
        );
    }

    /// 面板证据（P8-4）：候选经 Hello 交换 + 中继观测端点 → 两端后台打洞 →
    /// **双向证实**才记 `ok`，并可经 `path_status` 读出（且**不含 IP**）。
    #[test]
    fn punch_records_bidirectional_success_and_hides_addresses() {
        let a = mk_dev("pA");
        let b = mk_dev("pB");
        // 观测端点（模拟中继 UDP 观测）：诚实回反射
        let obs = udp_observer(0);
        let obs_addr = obs.local_addr().unwrap();

        let pa = a.engine.prepare_punch(&[obs_addr]).expect("准备打洞");
        let pb = b.engine.prepare_punch(&[obs_addr]).expect("准备打洞");
        let (pa_cands, pb_cands) = (pa.candidates.clone(), pb.candidates.clone());
        assert!(
            !pa.candidates.is_empty(),
            "本端必须给出候选（含观测到的映射）"
        );
        assert!(pa.mapped.is_some(), "观测 #1 必须成功");
        assert!(!pa.symmetric, "单观测目标不判对称");
        assert!(pa.candidates.len() <= 8, "候选上限 8（05-03 §3.2）");

        a.engine.spawn_punch("vd-bbbbbb", &pb_cands, pa);
        b.engine.spawn_punch("vd-aaaaaa", &pa_cands, pb);

        let ok = wait_until(
            || {
                let sa = a.engine.path_stats();
                sa["peers"][0]["punch"]["attempts"].as_u64().unwrap_or(0) >= 1
                    && sa["peers"][0]["punch"]["ok"].as_bool().unwrap_or(false)
                    && b.engine.path_stats()["peers"][0]["punch"]["ok"]
                        .as_bool()
                        .unwrap_or(false)
            },
            8000,
        );
        let sa = a.engine.path_stats();
        assert!(ok, "两端都必须双向证实打通：{sa}");
        let punch = &sa["peers"][0]["punch"];
        assert_eq!(punch["kind"], "punched", "路径状态机应停在 punched");
        assert_eq!(punch["degradedSteps"], 0, "成功即无降级步数");
        assert_eq!(punch["symmetricNat"], false);
        assert!(punch["localCandidates"].as_u64().unwrap_or(0) >= 1);
        assert!(punch["remoteCandidates"].as_u64().unwrap_or(0) >= 1);
        // 隐私纪律：打洞段只有计数与原因，**不得出现 IP**
        let text = serde_json::to_string(&sa).unwrap();
        assert!(!text.contains("127.0.0.1"), "不得泄露观测地址：{text}");
        assert!(!text.contains(obs_addr.to_string().as_str()));
    }

    /// 面板证据（P8-4，**负向**）：两个观测目标给出不同映射端口 → 判对称 NAT，
    /// **不做无望长尝试**（毫秒级返回），如实记 `symmetric_nat` 并降级到 relayed。
    #[test]
    fn punch_symmetric_nat_falls_back_fast() {
        let a = mk_dev("pS");
        let o1 = udp_observer(0);
        let o2 = udp_observer(7777); // 谎报端口 = 对称 NAT 特征
        let (a1, a2) = (o1.local_addr().unwrap(), o2.local_addr().unwrap());

        let prep = a
            .engine
            .prepare_punch(&[a1, a2])
            .expect("准备打洞（双观测目标）");
        assert!(prep.symmetric, "两目标映射端口不同必须判对称 NAT");

        let t0 = std::time::Instant::now();
        a.engine
            .spawn_punch("vd-cccccc", &["127.0.0.1:9".to_string()], prep);
        let done = wait_until(
            || {
                a.engine.path_stats()["peers"][0]["punch"]["attempts"]
                    .as_u64()
                    .unwrap_or(0)
                    >= 1
            },
            6000,
        );
        assert!(done, "打洞尝试必须记账");
        assert!(
            t0.elapsed() < Duration::from_secs(2),
            "对称 NAT 必须快速回落（实测 {:?}），不做无望长尝试",
            t0.elapsed()
        );
        let punch = a.engine.path_stats()["peers"][0]["punch"].clone();
        assert_eq!(punch["ok"], false, "对称 NAT 不得报打通：{punch}");
        assert_eq!(punch["reason"], "symmetric_nat");
        assert_eq!(punch["symmetricNat"], true);
        assert_eq!(punch["kind"], "relayed", "状态机必须降到 relayed");
        assert_eq!(punch["degradedSteps"], 1);
    }

    /// 面板证据（P8-4，**探针**）：显式打洞探针走**真实路径**（VSR2 接入 → 从 `OK` 行取观测
    /// 端点 → Noise 握手 → `hello_ext` 候选交换 → 观测 → 打洞），**不要求已配对**，
    /// 两端各调一次即各拿到结构化结果（矩阵逐格抄录就靠它）。
    #[test]
    fn punch_probe_runs_without_pairing_and_reports_both_sides() {
        const TOKEN: &str = "vsr2-probe-test-token-0123456789abcdef";
        let a = mk_dev("prA");
        let b = mk_dev("prB");
        let obs = udp_observer(0);
        let raddr = spawn_probe_relay(&obs.local_addr().unwrap().to_string());

        let a2 = a.engine.clone();
        let b2 = b.engine.clone();
        let (r1, r2) = (raddr.clone(), raddr.clone());
        let t1 = std::thread::spawn(move || {
            a2.punch_probe(
                &r1,
                "probe-room",
                TOKEN,
                Some("probe-shared-code"),
                ProbeRole::Initiator,
                5000,
            )
        });
        // 响应端稍后接入（房间需要两端都在；先后顺序与真实使用一致）
        std::thread::sleep(Duration::from_millis(150));
        let t2 = std::thread::spawn(move || {
            b2.punch_probe(
                &r2,
                "probe-room",
                TOKEN,
                Some("probe-shared-code"),
                ProbeRole::Responder,
                5000,
            )
        });
        let va = t1.join().unwrap().expect("发起端探针必须返回");
        let vb = t2.join().unwrap().expect("响应端探针必须返回");

        for (tag, v) in [("发起端", &va), ("响应端", &vb)] {
            assert_eq!(v["ok"], true, "{tag} 探针应打通：{v}");
            assert_eq!(v["kind"], "punched", "{tag}：{v}");
            assert_eq!(v["degradedSteps"], 0, "{tag}：{v}");
            assert_eq!(v["symmetricNat"], false, "{tag}：{v}");
            assert!(
                v["localCandidates"].as_u64().unwrap_or(0) >= 1,
                "{tag}：{v}"
            );
            assert!(
                v["remoteCandidates"].as_u64().unwrap_or(0) >= 1,
                "{tag}：{v}"
            );
            // 两次观测的端口都要有（核对 NAT 类型 / 对称判定的依据）
            assert!(v["mappedPort"].as_u64().is_some(), "{tag}：{v}");
            assert!(v["peerObservedPort"].as_u64().is_some(), "{tag}：{v}");
            assert!(v["elapsedMs"].as_u64().unwrap_or(9999) < 6000, "{tag}：{v}");
            assert!(v["peerDeviceId"].as_str().unwrap().starts_with("vd-"));
            // 隐私纪律：探针 JSON 只含计数/原因/端口，**不得含 IP**
            let text = serde_json::to_string(v).unwrap();
            assert!(!text.contains("127.0.0.1"), "不得泄露地址：{text}");
        }
        assert_eq!(va["role"], "initiator");
        assert_eq!(vb["role"], "responder");

        // 探针**不改配对状态**（未配对仍是未配对）
        let st = a.engine.status();
        assert_eq!(
            st["peers"].as_array().map(Vec::len).unwrap_or(0),
            0,
            "探针不得写入对端登记表：{st}"
        );
        // 结果与后台路径共用同一套记账 → 同时出现在 path_status 的 punch 段
        let ps = a.engine.path_stats();
        assert_eq!(ps["peers"][0]["punch"]["ok"], true, "{ps}");
        assert_eq!(ps["peers"][0]["punch"]["attempts"], 1, "{ps}");
    }

    /// 探针用的 mock 中继：VSR2 控制阶段 + `OK` 行捎带观测端点 + 同房间双向转发。
    fn spawn_probe_relay(observe_hint: &str) -> String {
        let relay = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let raddr = relay.local_addr().unwrap().to_string();
        let hint = observe_hint.to_string();
        std::thread::spawn(move || {
            use std::collections::HashMap;
            use std::io::BufRead;
            let mut rooms: HashMap<String, TcpStream> = HashMap::new();
            for stream in relay.incoming().flatten() {
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
                let _ =
                    s.write_all(b"VSR2 CHALLENGE aabbccdd00112233aabbccdd00112233 585021 5000\n");
                let mut auth = String::new();
                if r.read_line(&mut auth).is_err() {
                    continue;
                }
                // P8-4：OK 行第 4 字段 = 观测端点（探针据此做映射观测）
                let _ = s.write_all(
                    format!("VSR2 OK sess0000000000000000000000000000 0007 {hint}\n").as_bytes(),
                );
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
                                let _ = std::io::copy(&mut first, &mut second);
                            });
                            let _ = std::io::copy(&mut s2, &mut f1);
                            let _ = up.join();
                        });
                    }
                }
            }
        });
        raddr
    }

    /// 观测端点（测试夹具）：`delta != 0` 时谎报端口（模拟对称 NAT 的不同映射）。
    fn udp_observer(delta: u16) -> std::net::UdpSocket {
        let s = std::net::UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        let sock = s.try_clone().unwrap();
        std::thread::spawn(move || {
            let mut buf = [0u8; 64];
            loop {
                let Ok((n, from)) = sock.recv_from(&mut buf) else {
                    return;
                };
                let _ = n;
                let Some(mut reply) = vault_net::punch::encode_reflexion(from) else {
                    continue;
                };
                if delta != 0 {
                    let p = from.port().wrapping_add(delta);
                    reply[8..10].copy_from_slice(&p.to_be_bytes());
                }
                let _ = sock.send_to(&reply, from);
            }
        });
        s
    }

    /// 面板证据（P8-9 阅后即焚）：一次性会话 → 直连投递 → **单次性**（复用被拒）
    /// → 打开（受控临时明文）→ 超时**自动擦除**（临时明文与容器都不复存在）。
    #[test]
    fn burn_end_to_end_delivers_opens_and_erases() {
        let a = mk_dev("bA");
        let b = mk_dev("bB");
        let payload = b"burn-after-reading payload".to_vec();
        let fid = import(&a, "secret.txt", &payload);

        // 发送端建票（TTL 900 s、打开后 1 s 擦除——冒烟用短窗口）
        let created = a.engine.burn_create(fid, 900, 1).expect("burn_create");
        let burn_id = created["burnId"].as_u64().unwrap();
        let code = created["code"].as_str().unwrap().to_string();
        assert_eq!(code.len(), 16, "邀请码 16 字符");
        assert!(
            !created["room"].as_str().unwrap().is_empty(),
            "必须给出派生房间（中继路径用）"
        );

        // 接收端凭码登记（秘密仅内存）
        b.engine.burn_receive(&code, None).expect("burn_receive");
        assert!(
            b.engine.burn_receive("BADCODE", None).is_err(),
            "非法邀请码必须拒绝"
        );

        let sent = a
            .engine
            .burn_send_direct(burn_id, &addr_of(&b))
            .expect("burn_send");
        assert_eq!(sent["delivered"], true, "投递应成功：{sent}");
        assert_eq!(sent["bytes"].as_u64().unwrap(), payload.len() as u64);

        // 单次性：同一票据再次投递必须被拒（`05-04` §5.2）
        let again = a.engine.burn_send_direct(burn_id, &addr_of(&b));
        assert!(again.is_err(), "已交付票据不得复用：{again:?}");

        // 接收端：delivered + 一次性容器落盘
        let st = b.engine.burn_status(burn_id);
        assert_eq!(st["phase"], "delivered", "{st}");
        let dir = PathBuf::from(b.engine.burn_dir_for_ui());
        let container = dir.join(format!("{burn_id}.vse"));
        assert!(container.exists(), "应落盘一次性容器");

        // 打开：临时明文内容正确 + 擦除窗口来自报价
        let opened = b.engine.burn_open(burn_id).expect("burn_open");
        let tmp = PathBuf::from(opened["path"].as_str().unwrap());
        assert_eq!(std::fs::read(&tmp).unwrap(), payload, "打开内容必须正确");
        assert_eq!(opened["openTimeoutSecs"].as_u64().unwrap(), 1);
        assert_eq!(b.engine.burn_status(burn_id)["phase"], "opened");

        // 超时 → 自动擦除（临时明文 + 容器）
        let gone = wait_until(|| !tmp.exists() && !container.exists(), 8000);
        assert!(gone, "打开后超时应自动擦除临时明文与容器");
        let st2 = b.engine.burn_status(burn_id);
        assert_eq!(st2["phase"], "erased", "{st2}");
        assert_eq!(st2["erased"], true);

        // 启动扫描：残留一律清掉（秘密不落盘 → 重启后不可解）
        std::fs::create_dir_all(dir.join("tmp")).unwrap();
        std::fs::write(dir.join("tmp").join("leftover.part"), b"x").unwrap();
        std::fs::write(dir.join("9.vse"), b"y").unwrap();
        let swept = b.engine.burn_sweep_startup();
        assert_eq!(swept, 2, "启动扫描必须清掉全部残留");
        assert!(!dir.join("9.vse").exists());
    }

    /// 面板证据（P8-9）：秘密不落盘——重启后票据秘密消失，接收端无法再解密残留。
    #[test]
    fn burn_secret_never_persisted_across_restart() {
        let a = mk_dev("bR");
        let b = mk_dev("bR2");
        let fid = import(&a, "once.txt", b"one-shot");
        let created = a.engine.burn_create(fid, 900, 60).unwrap();
        let burn_id = created["burnId"].as_u64().unwrap();
        let code = created["code"].as_str().unwrap().to_string();
        b.engine.burn_receive(&code, None).unwrap();
        a.engine.burn_send_direct(burn_id, &addr_of(&b)).unwrap();
        assert!(b.engine.burn_dir_for_ui().contains("burn"));
        // 模拟重启：同一数据目录重开引擎 → 内存中的秘密与载荷密钥都没了
        let reopened = reopen(&b);
        let st = reopened.burn_status(burn_id);
        assert_eq!(st["phase"], "expired", "重启后票据必须收敛 expired：{st}");
        assert!(reopened.burn_open(burn_id).is_err(), "重启后不得可解");
        // 启动扫描把不可解残留清掉
        assert!(reopened.burn_sweep_startup() >= 1);
    }

    /// P7-4 证据：rotation_push::propagates_and_acks / mk_mismatch_rejected
    /// 传播入队 kind=3 → 接收端用自己的旧 MK 解封 ct（同源证明）→ inbound
    /// 落盘；两端旧 MK 不同 → 拒绝且不写任何状态。
    #[test]
    fn rotation_push_and_apply() {
        let a = mk_dev("rotA");
        let b = mk_dev("rotB");
        let code = b.engine.pair_begin().unwrap()["code"]
            .as_str()
            .unwrap()
            .to_string();
        a.engine.pair_join(&addr_of(&b), &code).unwrap();

        let rotation_id = "ab".repeat(16);
        let mk_new = vault_crypto::random_key();
        // 发起端：新 MK' 被旧 MK(a) 包装（VSRR 同构造）
        let wrap_key =
            vault_crypto::kdf::hkdf_sha256_derive(&a.engine.inner.keys_rt.mk, b"rotation-recover");
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&vault_crypto::random_bytes(12));
        let blob = vault_crypto::aead_encrypt(&wrap_key, &nonce, &mk_new).unwrap();
        let issued = now_ms();
        let expires = issued + 7 * 24 * 3600 * 1000;
        let from = a.engine.device_id();
        let domain = format!("vsync-rotate:{from}:{rotation_id}:{issued}:{expires}");
        let sig = a.engine.inner.ident.sign(domain.as_bytes());
        let payload = serde_json::json!({
            "RotationNotice": {
                "rotationId": rotation_id, "ct": vault_crypto::hex_encode(&blob),
                "issuedMs": issued, "expiresMs": expires, "sig": sig,
            }
        })
        .to_string();

        // 入队 + 投递（接收端用自己的旧 MK(b) 解封——两端旧 MK 不同 → 拒绝）
        a.engine
            .push_rotation(None, &rotation_id, &vault_crypto::hex_encode(&blob))
            .unwrap();
        assert!(a.engine.has_pending_rotation(&b.engine.device_id()));
        let a_pub = a.engine.inner.ident.public_hex(); // 发送方公钥（验签用）
        assert_eq!(
            b.engine.apply_order_payload(&a_pub, &payload),
            Err(OrderReject::MkMismatch),
            "不同源旧 MK 必须拒绝"
        );
        assert!(
            !b.dir
                .path()
                .join("rotB.data")
                .join("rotation.inbound")
                .exists(),
            "拒绝路径不得写任何状态"
        );

        // 同源（用 b 的旧 MK 包装）→ 解封成功 → inbound 落盘
        let wrap_key_b =
            vault_crypto::kdf::hkdf_sha256_derive(&b.engine.inner.keys_rt.mk, b"rotation-recover");
        let blob_b = vault_crypto::aead_encrypt(&wrap_key_b, &nonce, &mk_new).unwrap();
        let payload_b = serde_json::json!({
            "RotationNotice": {
                "rotationId": rotation_id, "ct": vault_crypto::hex_encode(&blob_b),
                "issuedMs": issued, "expiresMs": expires, "sig": sig,
            }
        })
        .to_string();
        b.engine
            .apply_order_payload(&a_pub, &payload_b)
            .expect("same old mk must accept");
        assert!(
            b.dir
                .path()
                .join("rotB.data")
                .join("rotation.inbound")
                .exists(),
            "接受的 notice 必须落盘（下次解锁续做）"
        );
    }
}
