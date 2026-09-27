//! 同步信令消息（docs/05-04：信令全程在 Noise 信道内传输，应用层再带序号防重放）。
//! 全部 JSON 序列化；块密文以 base64 携带。
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 清单条目（文件或墓碑共用 shape，`deletedMs` 存在即为墓碑）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestItem {
    pub id: u64,
    #[serde(default)]
    pub folder: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub modified_ms: u64,
    #[serde(default)]
    pub vc: BTreeMap<String, u64>,
    #[serde(default)]
    pub deleted_ms: Option<u64>,
    #[serde(default)]
    pub chunks: Vec<ChunkRef>,
    #[serde(default)]
    pub nonce_prefix: u32,
    #[serde(default)]
    pub file_sha: String,
    /// P8-8：条目版本号（原地编辑血缘；接收端保留并作为三方合并基线判据）。
    #[serde(default)]
    pub rev: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkRef {
    pub hash: String,
    #[serde(default)]
    pub offset: u64,
    #[serde(default)]
    pub len: u32,
}

#[derive(Serialize, Deserialize)]
pub enum Msg {
    /// 握手后第一条：声明身份，对 hh‖x25519 签名（把 Noise 静态密钥绑定到 Ed25519 身份）。
    ///
    /// `port`（P5-5 新增）= 发送方**监听**端口，用于对端登记可回拨地址（离线销毁指令投递）。
    /// 对端拨入时 TCP 源端口是临时端口，不是对端监听端口，只靠 `peer_addr()` 会得到
    /// 无法回拨的地址，故必须由 Hello 显式声明。该字段**不在签名体内**：
    /// 信道本身已认证加密（Noise XX，`remote_is` 已核对静态密钥），伪造者无法进入信道；
    /// 且该值只影响"往哪拨"，不影响任何执行判定（销毁指令另有签名与 target 核对）。
    /// `#[serde(default)]`：旧端不发该字段 → 0（未知），接收端回退用观测到的源端口。
    /// `pq`（P7-8 新增，不在签名体内，同 `port` 信任口径）= 本端混合 KEM 能力
    /// （ML-KEM-768 初始化成功）。对端记入 peers（pqCap）作后续直连同步的协商依据；
    /// 旧端无此字段 → false，对端即不再向其提议 suite 2。
    Hello {
        device_id: String,
        name: String,
        pub_hex: String,
        x25519_hex: String,
        sig: String,
        #[serde(default)]
        port: u16,
        #[serde(default)]
        pq: bool,
        /// P8-6（不在签名体内，同 `port` 信任口径）：协议版本。≥ 2 = 支持
        /// 每帧 `real_len` 新帧格式与填充；0/1 = 旧端（恒旧帧格式，不填充）。
        #[serde(default)]
        proto_ver: u16,
        /// 填充档位下/上限（0 = 未声明）；生效档 = min(两端 max)，下限违背
        /// 取低档并发 PATH_DEGRADED（可用性优先，降级可见，05-04 §3.3）。
        #[serde(default)]
        pad_tier_min: u8,
        #[serde(default)]
        pad_tier_max: u8,
        /// P8-4：本机候选（`ip:port` 字面量，含 UDP 映射观测结果）。旧端缺省空
        /// 列表 → 不打洞（如实降级，不猜地址）。候选随 Noise 信道内传输，
        /// 对端已由信道密钥 + Hello 签名认证，故候选来源可信（数量另受上限约束）。
        #[serde(default)]
        candidates: Vec<String>,
    },
    /// 配对：发起端（已配对设备）请求登记；响应端凭 PSK 信任。
    PairReq,
    PairAccept,
    /// 同步主流程：发起端送出全量清单。
    SyncReq {
        manifest: Vec<ManifestItem>,
    },
    SyncResp {
        manifest: Vec<ManifestItem>,
    },
    /// 推送文件：fskey_wrapped = AEAD(子密钥(hh,"fskey"), FSKey)，仅本信道可解。
    /// overwrite = 向量时钟分叉时以本版本为准（对端先留存冲突副本）。
    /// `base_rev`（P8-8）= 本版本所基于的基线版本号（`rev - 1`；0 = 未曾原地编辑）；
    /// 接收端凭它判定「双方是否从同一基线各编辑了一次」→ 三方块级合并。
    SendFileBegin {
        item: ManifestItem,
        fskey_wrapped: String,
        total_chunks: usize,
        overwrite: bool,
        #[serde(default)]
        base_rev: u64,
    },
    BlockData {
        id: u64,
        idx: usize,
        ct_b64: String,
    },
    BlockEnd {
        id: u64,
    },
    FileApplied {
        id: u64,
        name: String,
        /// P8-8：接收端的合并结果（`Some("auto")` = 已块级自动合并，本次未落盘对端原样内容）。
        #[serde(default)]
        merge: Option<String>,
    },
    /// 请求对端推送该文件（拉取阶段）。
    PullFile {
        id: u64,
    },
    /// 块幂等：接收端比对已有块清单，仅请求缺失块（断点续传语义）。
    BlocksNeeded {
        id: u64,
        idxs: Vec<usize>,
    },
    VcMerge {
        id: u64,
        vc: BTreeMap<String, u64>,
    },
    /// 删除指令：携带发起方长期身份签名，验签后执行（docs/08 §4.3）。
    DeleteOrder {
        id: u64,
        sig: String,
    },
    /// 远程销毁：签名 + 延迟毫秒（0 = 到达即执行）。
    DestroyCmd {
        target: String,
        delay_ms: u64,
        ts_ms: u64,
        sig: String,
    },
    DestroyAck,
    /// 单向同步流程结束（发起端发起）。
    SyncDone,
    /// P8-9 阅后即焚：报价（接收端凭 burn PSK 完成握手即证明持有票据秘密）。
    BurnOffer {
        burn_id: u64,
        name: String,
        size: u64,
        sha: String,
        total_chunks: usize,
        /// 打开后的自动擦除窗口（秒；接收端据此起定时器，缺省 60）。
        #[serde(default = "d_open_timeout")]
        open_timeout_secs: u64,
    },
    /// P8-9：分片载荷（**明文**在 Noise 内传输；接收端用专用 burn 密钥再落盘）。
    BurnData {
        burn_id: u64,
        idx: usize,
        data_b64: String,
    },
    /// P8-9：传输完成（发送端宣告；接收端据此落盘完成并回 ACK）。
    BurnDone {
        burn_id: u64,
    },
    /// P8-9：阶段回执（`phase ∈ delivered`）——只回阶段枚举，不含明文 / 文件名。
    BurnAck {
        burn_id: u64,
        phase: String,
    },
    Error {
        msg: String,
    },
}

/// P8-9：`BurnOffer.open_timeout_secs` 的缺省（秒）。
fn d_open_timeout() -> u64 {
    60
}

/// 删除指令签名体（签名即对此串）。
pub fn delete_sign_body(id: u64) -> Vec<u8> {
    format!("vsync-del:{id}").into_bytes()
}

/// Hello 签名体：握手哈希 ‖ X25519 公钥（把信道静态密钥绑定到长期身份）。
pub fn hello_sign_body(hh: &[u8; 32], x25519_hex: &str) -> Vec<u8> {
    let mut b = hh.to_vec();
    b.extend_from_slice(x25519_hex.as_bytes());
    b
}

/// 销毁指令签名体。
pub fn destroy_sign_body(target: &str, delay_ms: u64, ts_ms: u64) -> Vec<u8> {
    format!("vsync-destroy:{target}:{delay_ms}:{ts_ms}").into_bytes()
}
