//! P2P 同步引擎编排（docs/05-03、docs/08）。
//!
//! 职责：设备监听 / 配对（邀请码 PSK + 指纹核验）/ 增量同步（块清单比对 + 差量传输 +
//! 断点续传幂等 + 限速）/ 冲突解决（删除优先 + 多版本副本 + 向量时钟）/ 远程销毁（签名 + 延迟语义）。
//!
//! 密钥红线：FSKey 仅经 E2E 信道携带给已配对对端；中继只见不透明 Noise 帧。
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::io::{BufRead, Seek, Write as IoWrite};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use vault_crypto::kdf::hkdf_sha256_derive;
use vault_crypto::{aead_decrypt, aead_encrypt, hex_encode, random_bytes, KEY_LEN};
use vault_vault::vault::Vault;

use crate::channel::{psk_from_code, subkey, Role, SecureChannel};
use crate::identity::Identity;
use crate::orders::{OrderRec, OrderStore};
use crate::peers::{PeerRec, PeerStore};
use crate::proto::{delete_sign_body, destroy_sign_body, hello_sign_body, ManifestItem, Msg};

/// 误删保护窗口（docs/05-03 §6.2：窗口期内删除保留加密副本）。
const PROTECT_WINDOW_MS: u64 = 24 * 3600 * 1000;
const INVITE_TTL_MS: u64 = 10 * 60 * 1000;
const MAX_EVENTS: usize = 100;
const RELAY_MAGIC: &str = "VSR1";

/// 远程销毁的本机擦除回调（由宿主 vault-core 注入：删保险箱文件 + 数据目录）。
pub type WipeFn = Box<dyn Fn() -> Result<(), String> + Send>;

/// 销毁指令被拒的原因（docs/08 §四.3：破坏性信令必须验签后执行）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderReject {
    /// 签名不合法或载荷被篡改：不执行、记事件、断开且不回执（不给伪造者任何回执）。
    BadSignature,
    /// `target` 不是本机 device_id：不执行、回执 Error（多半是投递到了错误设备）。
    TargetMismatch,
    /// 载荷不是一枚合法的销毁指令帧。
    Malformed,
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

pub struct EngineInner {
    ident: Identity,
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
    port: AtomicU64,
    rate_bps: AtomicU64,
    dead: AtomicBool,
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
    pub fn new(
        vault_path: &Path,
        mk: &[u8; KEY_LEN],
        device_name: &str,
        vault_slot: Arc<Mutex<Option<Vault>>>,
        wipe: WipeFn,
    ) -> Result<Self, &'static str> {
        let stem = vault_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("invalid vault name")?;
        let parent = vault_path.parent().unwrap_or(Path::new("."));
        let data_dir = parent.join(format!("{stem}.data"));
        let p2p_dir = data_dir.join("p2p");
        std::fs::create_dir_all(&p2p_dir).map_err(|_| "cannot create p2p dir")?;

        let ident_key = *hkdf_sha256_derive(mk, b"p2p-ident");
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
        let orders = OrderStore::load_or_new(&p2p_dir, mk)?;
        let engine = Self {
            inner: Arc::new(EngineInner {
                ident,
                device_name: device_name.to_string(),
                data_dir: data_dir.clone(),
                vault_slot,
                peers: Mutex::new(peers),
                orders: Mutex::new(orders),
                invite: Mutex::new(None),
                events: Mutex::new(Vec::new()),
                armed: Mutex::new(None),
                wipe: Mutex::new(Some(wipe)),
                port: AtomicU64::new(0),
                rate_bps: AtomicU64::new(0),
                dead: AtomicBool::new(false),
            }),
        };

        // 打开保险箱进槽位；设备位供向量时钟自增
        {
            let mut guard = engine
                .inner
                .vault_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if guard.is_none() {
                let mut v = Vault::open(vault_path, mk)?;
                v.set_device_id(&engine.inner.ident.device_id());
                *guard = Some(v);
            }
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

    fn log(&self, msg: &str) {
        let mut ev = self.inner.events.lock().unwrap_or_else(|e| e.into_inner());
        ev.push(format!("{} {msg}", now_ms()));
        if ev.len() > MAX_EVENTS {
            ev.remove(0);
        }
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
    pub fn pair_join(&self, addr: &str, code: &str) -> Result<serde_json::Value, String> {
        let psk = psk_from_code(code);
        let mut stream = TcpStream::connect(addr).map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| e.to_string())?;
        let mut ch =
            SecureChannel::handshake(&mut stream, Role::Initiator, &self.inner.ident, Some(&psk))
                .map_err(|e| e.to_string())?;
        let (peer_id, peer_name, peer_pub, _peer_port) = self.hello(&mut ch, &mut stream)?;
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
                    pub_hex: peer_pub,
                    paired_ms: now_ms(),
                    counter: 0,
                    addr: Some(addr.to_string()),
                },
            )
            .map_err(|e| e.to_string())?;
        Ok(serde_json::json!({"peerId": peer_id, "name": peer_name}))
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
    /// device_id 派生自声明的 Ed25519 公钥。返回值末位为对端声明的监听端口（0 = 旧端未声明）。
    fn hello<S: std::io::Read + std::io::Write>(
        &self,
        ch: &mut SecureChannel,
        stream: &mut S,
    ) -> Result<(String, String, String, u16), &'static str> {
        let hh = *ch.handshake_hash();
        let x25519_hex = hex_encode(&self.inner.ident.x25519_pub());
        let sig = self.inner.ident.sign(&hello_sign_body(&hh, &x25519_hex));
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
            },
        )?;
        let resp: Msg = recv_json(ch, stream)?;
        let (device_id, name, pub_hex, x25519_hex, sig, port) = match resp {
            Msg::Hello {
                device_id,
                name,
                pub_hex,
                x25519_hex,
                sig,
                port,
            } => (device_id, name, pub_hex, x25519_hex, sig, port),
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
        Ok((device_id, name, pub_hex, port))
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
                        .spawn(move || eng.handle_conn(stream, false));
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(_) => return,
            }
        }
    }

    fn handle_conn(&self, mut stream: TcpStream, relayed: bool) {
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
        let mut ch = match SecureChannel::handshake(
            &mut stream,
            Role::Responder,
            &self.inner.ident,
            pending_psk.as_ref(),
        ) {
            Ok(c) => c,
            Err(e) => {
                self.log(&format!("handshake failed: {e}"));
                return;
            }
        };
        if is_pairing {
            // 配对成功即消费邀请码（一次性）
            *self.inner.invite.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
        let (peer_id, peer_name, peer_pub, peer_port) = match self.hello(&mut ch, &mut stream) {
            Ok(h) => h,
            Err(e) => {
                self.log(&format!("hello failed: {e}"));
                return;
            }
        };
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
                },
            );
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
                Msg::SyncReq { .. } => {
                    if let Err(e) = self.respond_sync(&mut ch, &mut stream) {
                        self.log(&format!("sync responder failed: {e}"));
                        let _ = send_json(&mut ch, &mut stream, &Msg::Error { msg: e });
                        return;
                    }
                }
                Msg::SendFileBegin {
                    item,
                    fskey_wrapped,
                    overwrite,
                    ..
                } => {
                    if self
                        .receive_push_rest(&mut ch, &mut stream, item, fskey_wrapped, overwrite)
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
                    if self.respond_push(&mut ch, &mut stream, &item).is_err() {
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
        stream: &mut TcpStream,
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
        stream: &mut TcpStream,
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
        stream: &mut TcpStream,
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

    /// 发起一次增量同步（直连地址）。
    pub fn sync_with(&self, addr: &str) -> Result<serde_json::Value, String> {
        self.run_sync(move || TcpStream::connect(addr).map_err(|e| e.to_string()))
    }

    /// 经中继同步：先注册房间令牌，中继对接同房间两端后照常 E2E 握手（docs/08）。
    pub fn sync_via_relay(
        &self,
        relay_addr: &str,
        room: &str,
    ) -> Result<serde_json::Value, String> {
        self.run_sync(move || Self::dial_relay(relay_addr, room))
    }

    /// 响应端经中继接入：注册房间后进入既有连接处理流程（握手/核验/同步/销毁）。
    pub fn serve_relay_connection(&self, relay_addr: &str, room: &str) -> Result<(), String> {
        let stream = Self::dial_relay(relay_addr, room)?;
        // relayed = true：源地址是中继的，不当作对端可回拨地址登记
        self.handle_conn(stream, true);
        Ok(())
    }

    /// 向中继注册房间令牌并拿到对接后的字节流（docs/08：中继仅见不透明帧）。
    fn dial_relay(relay_addr: &str, room: &str) -> Result<TcpStream, String> {
        let mut s = TcpStream::connect(relay_addr).map_err(|e| e.to_string())?;
        s.set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| e.to_string())?;
        s.write_all(format!("{RELAY_MAGIC} {room}\n").as_bytes())
            .map_err(|e| e.to_string())?;
        let mut line = String::new();
        std::io::BufReader::new(&s)
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        if !line.starts_with("OK") {
            return Err(format!("relay rejected: {}", line.trim()));
        }
        s.set_read_timeout(None).map_err(|e| e.to_string())?;
        Ok(s)
    }

    fn run_sync<F>(&self, connect: F) -> Result<serde_json::Value, String>
    where
        F: FnOnce() -> Result<TcpStream, String>,
    {
        if self.inner.dead.load(Ordering::SeqCst) {
            return Err("engine destroyed".into());
        }
        let mut stream = connect()?;
        stream
            .set_read_timeout(Some(Duration::from_secs(120)))
            .map_err(|e| e.to_string())?;
        let mut ch =
            SecureChannel::handshake(&mut stream, Role::Initiator, &self.inner.ident, None)
                .map_err(|e| e.to_string())?;
        let (peer_id, _peer_name, peer_pub, _peer_port) = self
            .hello(&mut ch, &mut stream)
            .map_err(|e| e.to_string())?;
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

        let summary = self.execute_plan(&mut ch, &mut stream, &local_items, &peer_items)?;
        let _ = send_json(&mut ch, &mut stream, &Msg::SyncDone);
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

    /// 计划与执行：docs/05-03 §6.2 固定优先级（删除优先 > 多版本保留；向量时钟仅辅助判定）。
    fn execute_plan(
        &self,
        ch: &mut SecureChannel,
        stream: &mut TcpStream,
        local: &[ManifestItem],
        peer: &[ManifestItem],
    ) -> Result<serde_json::Value, String> {
        let lm: BTreeMap<u64, &ManifestItem> = local.iter().map(|i| (i.id, i)).collect();
        let pm: BTreeMap<u64, &ManifestItem> = peer.iter().map(|i| (i.id, i)).collect();
        let mut summary = serde_json::json!({
            "pushed": [], "pulled": [], "deleted": [], "conflicts": [], "merged": []
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
                        self.push_file(ch, stream, fa, false)?;
                        push("pushed", serde_json::json!(id), &mut summary);
                    } else if dominates(&fb.vc, &fa.vc) {
                        self.pull_file(ch, stream, id, false)?;
                        push("pulled", serde_json::json!(id), &mut summary);
                    } else if fa.vc != fb.vc {
                        // 分叉：较新修改者胜出；落后方先自留加密冲突副本（docs/05-03 规则 2）
                        if fa.modified_ms >= fb.modified_ms {
                            self.push_file(ch, stream, fa, true)?;
                        } else {
                            self.pull_file(ch, stream, id, true)?;
                        }
                        push("conflicts", serde_json::json!(id), &mut summary);
                    }
                }
                // 本端活文件、对端墓碑
                (Some(fa), Some(tb)) if fa.deleted_ms.is_none() => {
                    if dominates(&tb.vc, &fa.vc) {
                        self.delete_local(id)?;
                        push("deleted", serde_json::json!(id), &mut summary);
                    } else if dominates(&fa.vc, &tb.vc) {
                        self.push_file(ch, stream, fa, false)?; // 复活对端
                        push("pushed", serde_json::json!(id), &mut summary);
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
                        self.pull_file(ch, stream, id, false)?; // 对端复活
                        push("pulled", serde_json::json!(id), &mut summary);
                    }
                }
                // 单侧存在
                (Some(fa), None) if fa.deleted_ms.is_none() => {
                    self.push_file(ch, stream, fa, false)?;
                    push("pushed", serde_json::json!(id), &mut summary);
                }
                (None, Some(fb)) if fb.deleted_ms.is_none() => {
                    self.pull_file(ch, stream, id, false)?;
                    push("pulled", serde_json::json!(id), &mut summary);
                }
                _ => {}
            }
        }
        Ok(summary)
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
    fn push_file(
        &self,
        ch: &mut SecureChannel,
        stream: &mut TcpStream,
        item: &ManifestItem,
        overwrite: bool,
    ) -> Result<(), String> {
        let id = item.id;
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
                overwrite,
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

    /// 拉取：PullFile → 对端走推送流程。overwrite 时对端 SendFileBegin 标记覆盖。
    fn pull_file(
        &self,
        ch: &mut SecureChannel,
        stream: &mut TcpStream,
        id: u64,
        overwrite: bool,
    ) -> Result<(), String> {
        send_json(ch, stream, &Msg::PullFile { id }).map_err(|e| e.to_string())?;
        let begin: Msg = recv_json(ch, stream).map_err(|e| e.to_string())?;
        match begin {
            Msg::SendFileBegin {
                item,
                fskey_wrapped,
                overwrite: peer_marked,
                ..
            } => {
                // overwrite 语义由接收端（本端）执行冲突副本；以请求时的标记为准
                self.receive_push_rest(ch, stream, item, fskey_wrapped, overwrite || peer_marked)
            }
            Msg::Error { msg } => Err(msg),
            _ => Err("expected sendfilebegin".into()),
        }
    }

    /// 通用接收推送流程：解封 FSKey → 块清单比对（增量/续传幂等）→ 收块 → 装配 → 整文件校验。
    fn receive_push_rest(
        &self,
        ch: &mut SecureChannel,
        stream: &mut TcpStream,
        item: ManifestItem,
        fskey_wrapped: String,
        overwrite: bool,
    ) -> Result<(), String> {
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

        // 与本地既有容器块清单比对：哈希相同的块不再传输（增量 + 断点续传的幂等基础）
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
            .filter(|(_, c)| !have.contains(&c.hash))
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

        // 已有块从本地容器原样拷入 part（密文一致），缺块等 BlockData
        let part = self.part_path(id);
        std::fs::write(&part, b"").map_err(|e| e.to_string())?;
        let mut copied = 0usize;
        {
            let slot = self
                .inner
                .vault_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let v = slot.as_ref().ok_or("vault locked")?;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .open(&part)
                .map_err(|e| e.to_string())?;
            for (i, c) in item.chunks.iter().enumerate() {
                if !idxs.contains(&i) {
                    let ct = v.read_chunk_ct(id, i).map_err(|e| e.to_string())?;
                    if vault_crypto::hash_sha256(&ct) != c.hash {
                        return Err("local chunk hash mismatch".into());
                    }
                    f.seek(std::io::SeekFrom::Start(c.offset))
                        .map_err(|e| e.to_string())?;
                    f.write_all(&ct).map_err(|e| e.to_string())?;
                    copied += 1;
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
                    self.throttle(ct.len());
                    let mut f = std::fs::OpenOptions::new()
                        .write(true)
                        .open(&part)
                        .map_err(|e| e.to_string())?;
                    f.seek(std::io::SeekFrom::Start(item.chunks[idx].offset))
                        .map_err(|e| e.to_string())?;
                    f.write_all(&ct).map_err(|e| e.to_string())?;
                }
                Msg::BlockEnd { id: rid } if rid == id => break,
                Msg::Error { msg } => return Err(msg),
                _ => return Err("expected blockdata".into()),
            }
        }
        if got.len() + copied != item.chunks.len() {
            return Err("missing blocks".into());
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
                v.save_conflict_copy(id, item.vc.clone())
                    .map_err(|e| e.to_string())?;
                v.delete_file(id, false).map_err(|e| e.to_string())?;
            }
        }

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
                &part,
                &fskey_hex,
                item.vc.clone(),
            )
            .map_err(|e| e.to_string())?;
            v.verify_file(id).map_err(|e| e.to_string())?;
        }
        std::fs::remove_file(&part).ok();
        send_json(
            ch,
            stream,
            &Msg::FileApplied {
                id,
                name: item.name.clone(),
            },
        )
        .map_err(|e| e.to_string())
    }

    /// 响应端同步：回清单 → 处理指令（收推送 / 拉取 / 合并 / 删除）直到 SyncDone。
    fn respond_sync(&self, ch: &mut SecureChannel, stream: &mut TcpStream) -> Result<(), String> {
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
                    ..
                } => {
                    self.receive_push_rest(ch, stream, item, fskey_wrapped, overwrite)?;
                }
                Msg::PullFile { id } => {
                    let item = self.manifest_item(id)?;
                    self.respond_push(ch, stream, &item)?;
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
    fn respond_push(
        &self,
        ch: &mut SecureChannel,
        stream: &mut TcpStream,
        item: &ManifestItem,
    ) -> Result<(), String> {
        let id = item.id;
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
        let mut stream = TcpStream::connect(addr).map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| e.to_string())?;
        let mut ch =
            SecureChannel::handshake(&mut stream, Role::Initiator, &self.inner.ident, None)
                .map_err(|e| e.to_string())?;
        let (_pid, _pn, ppub, _port) = self
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

    /// 取消已武装的本机延迟销毁；返回是否有被取消的任务。
    pub fn destroy_cancel(&self) -> bool {
        self.inner
            .armed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .is_some()
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
        serde_json::json!({
            "deviceId": self.device_id(),
            "fingerprint": self.fingerprint(),
            "port": self.port(),
            "name": self.inner.device_name,
            "peers": peers.list().iter().map(|(id, p)| serde_json::json!({
                "deviceId": id, "name": p.name,
                "fingerprint": crate::identity::Identity::fingerprint_of(&p.pub_hex),
                // 可空：从未观测到/未手填过地址时为 null（UI 据此提示手填，见 remember_peer_addr）
                "addr": p.addr,
            })).collect::<Vec<_>>(),
            "armedDestroy": armed.as_ref().map(|a| serde_json::json!({
                "target": a.target,
                "remainingMs": a.deadline.saturating_duration_since(Instant::now()).as_millis() as u64,
            })),
            "events": events.iter().rev().take(20).cloned().collect::<Vec<_>>(),
            "alive": !self.inner.dead.load(Ordering::SeqCst),
        })
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
        let mut v = Vault::open(&vp, &mk).unwrap();
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

    fn mk_dev(tag: &str) -> TestDev {
        let dir = tempfile::tempdir().unwrap();
        let vault_path = dir.path().join(format!("{tag}.vsvb"));
        std::fs::write(&vault_path, b"fake-header").unwrap();
        let mk = vault_crypto::random_key();
        let slot: Arc<Mutex<Option<Vault>>> = Arc::new(Mutex::new(None));
        let wipes = Arc::new(AtomicUsize::new(0));
        let wipe = wipe_counter(wipes.clone(), vault_path.clone());
        let engine = P2pEngine::new(&vault_path, &mk, tag, slot.clone(), wipe).unwrap();
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
        P2pEngine::new(
            &dev.vault_path,
            &dev.mk,
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
                },
            )
            .unwrap();
    }

    fn import(dev: &TestDev, name: &str, data: &[u8]) -> u64 {
        let src = dev.dir.path().join(name);
        std::fs::write(&src, data).unwrap();
        let mut slot = dev.slot.lock().unwrap();
        slot.as_mut().unwrap().import_file(&src, 0).unwrap()
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
        println!("B events: {:?}", b.engine.status()["events"]);
        println!("A events: {:?}", a.engine.status()["events"]);
        assert!(!{
            let slot = b.slot.lock().unwrap();
            slot.as_ref().unwrap().has_file(f3)
        });

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
        let a = mk_dev("rA");
        let b = mk_dev("rB");
        // 未配对设备直接同步 → 拒绝
        assert!(a.engine.sync_with(&addr_of(&b)).is_err());

        // 极简中继 mock：同房间两端到达后双向转发字节
        let relay = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let raddr = relay.local_addr().unwrap().to_string();
        std::thread::spawn(move || {
            use std::collections::HashMap;
            use std::io::BufRead;
            let mut rooms: HashMap<String, TcpStream> = HashMap::new();
            for stream in relay.incoming().flatten() {
                let mut s = stream;
                let mut line = String::new();
                if std::io::BufReader::new(&s).read_line(&mut line).is_err() {
                    continue;
                }
                let room = line
                    .trim()
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("")
                    .to_string();
                let _ = s.write_all(b"OK\n");
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
        let server = std::thread::spawn(move || bengine.serve_relay_connection(&raddr2, "room-1"));
        let s = a.engine.sync_via_relay(&raddr, "room-1").unwrap();
        assert_eq!(s["pulled"].as_array().unwrap().len(), 1);
        assert!(export_ok(&a, f, &data));
        let _ = server.join();
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
}
