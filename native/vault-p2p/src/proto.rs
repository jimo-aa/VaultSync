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
    Hello {
        device_id: String,
        name: String,
        pub_hex: String,
        x25519_hex: String,
        sig: String,
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
    SendFileBegin {
        item: ManifestItem,
        fskey_wrapped: String,
        total_chunks: usize,
        overwrite: bool,
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
    Error {
        msg: String,
    },
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
