//! 高优先级信令队列：离线设备的销毁指令待投递队列（docs/05-06 §5.1、docs/08 §四.2）。
//!
//! 语义（V0.2）：销毁指令对离线设备是**最终一致**——指令落本机队列，目标设备下次上线
//! 建立信道后、清单交换之前先被投递并校验执行；"永不上线"的设备不受保护（设计取舍，
//! 见 docs/06 §四.5）。
//!
//! 安全约束：
//! - 队列落盘与 `peers.enc` 同款 AEAD 封装（密钥 = HKDF(MK,"p2p-orders")），文件名 `orders.enc`
//!   与对端表同目录；本机队列只是"待投递的字节"，**不作为执行凭据**——对端收到后一律
//!   用发起方长期身份公钥验签、并核对 `target == 本机 device_id` 才执行（见 `engine::apply_order`）。
//! - 条目原样保存签名指令帧（`payload`），因此投递不需要私钥，也不需要重新签名。
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vault_crypto::kdf::hkdf_sha256_derive;
use vault_crypto::{aead_decrypt, aead_encrypt, random_bytes, AES_GCM_NONCE_LEN, KEY_LEN};

/// 一条待投递的销毁指令。
///
/// `peer_id` 与 `target` 当前恒等（接收端要求 `target == 自己的 device_id`，即
/// "发给 X 的指令其 target 就是 X"）；分开保存是为将来"代投第三方"留出语义空间，
/// 且与协议字段一一对应，便于排障时直接对照。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderRec {
    /// 队列归属：该指令要投递给的对端 device_id。
    pub peer_id: String,
    /// 指令目标（签名体覆盖字段），必须等于接收端自己的 device_id。
    pub target: String,
    /// 延迟秒数（0 = 收到即执行）。
    pub delay_secs: u64,
    /// 签发时刻（ms，签名体覆盖字段）。
    pub issued_ms: u64,
    /// 发起方长期身份对 `vsync-destroy:{target}:{delay_ms}:{ts}` 的 Ed25519 签名（hex）。
    pub sig_hex: String,
    /// 待投递帧原文（`Msg::DestroyCmd` 的 JSON 序列化）。
    pub payload: String,
}

#[derive(Default, Serialize, Deserialize)]
struct OrderData {
    #[serde(default)]
    orders: Vec<OrderRec>,
}

/// 加密落盘的销毁指令队列（append 顺序 = 签发顺序）。
pub struct OrderStore {
    d: OrderData,
    path: PathBuf,
    key: [u8; KEY_LEN],
}

impl OrderStore {
    pub fn load_or_new(dir: &Path, mk: &[u8; KEY_LEN]) -> Result<Self, &'static str> {
        std::fs::create_dir_all(dir).map_err(|_| "cannot create p2p dir")?;
        let key = *hkdf_sha256_derive(mk, b"p2p-orders");
        let path = dir.join("orders.enc");
        let d = match std::fs::read(&path) {
            Ok(blob) => {
                if blob.len() < AES_GCM_NONCE_LEN + 16 {
                    return Err("order store truncated");
                }
                let pt = aead_decrypt(&key, &blob).ok_or("order store decrypt failed")?;
                serde_json::from_slice(&pt).map_err(|_| "order store parse failed")?
            }
            Err(_) => OrderData::default(),
        };
        Ok(Self { d, path, key })
    }

    /// 入队。同一 (peer_id,target) 只保留最新一条：重复触发联动销毁不叠加投递次数，
    /// 且旧签名的 `ts` 已过时，保留多条只会让 UI 展示混乱。
    pub fn push(&mut self, rec: OrderRec) -> Result<(), &'static str> {
        self.d
            .orders
            .retain(|o| !(o.peer_id == rec.peer_id && o.target == rec.target));
        self.d.orders.push(rec);
        self.save()
    }

    /// 该对端的待投递指令快照（克隆，调用方在读/写队列时不得持有本存储的锁）。
    pub fn list_for(&self, peer_id: &str) -> Vec<OrderRec> {
        self.d
            .orders
            .iter()
            .filter(|o| o.peer_id == peer_id)
            .cloned()
            .collect()
    }

    /// 投递成功（对端 ACK）后移除单条；返回是否确有该条目。
    pub fn remove(
        &mut self,
        peer_id: &str,
        target: &str,
        issued_ms: u64,
    ) -> Result<bool, &'static str> {
        let before = self.d.orders.len();
        self.d
            .orders
            .retain(|o| !(o.peer_id == peer_id && o.target == target && o.issued_ms == issued_ms));
        if self.d.orders.len() == before {
            return Ok(false);
        }
        self.save()?;
        Ok(true)
    }

    /// 清空队列（UI「取消投递」）；返回被清掉的条数，空队列不落盘。
    pub fn clear(&mut self) -> Result<usize, &'static str> {
        if self.d.orders.is_empty() {
            return Ok(0);
        }
        let n = self.d.orders.len();
        self.d.orders.clear();
        self.save()?;
        Ok(n)
    }

    pub fn all(&self) -> &[OrderRec] {
        &self.d.orders
    }

    pub fn count(&self) -> usize {
        self.d.orders.len()
    }

    fn save(&self) -> Result<(), &'static str> {
        let json = serde_json::to_vec(&self.d).map_err(|_| "order serialize failed")?;
        let nonce_v = random_bytes(AES_GCM_NONCE_LEN);
        let nonce: [u8; AES_GCM_NONCE_LEN] =
            nonce_v.as_slice().try_into().map_err(|_| "nonce len")?;
        let full = aead_encrypt(&self.key, &nonce, &json).map_err(|_| "order encrypt failed")?;
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, &full).map_err(|_| "cannot write orders")?;
        std::fs::rename(&tmp, &self.path).map_err(|_| "cannot finalize orders")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn rec(peer: &str, ts: u64, delay_secs: u64) -> OrderRec {
        OrderRec {
            peer_id: peer.into(),
            target: peer.into(),
            delay_secs,
            issued_ms: ts,
            sig_hex: format!("sig-{ts}"),
            payload: format!("{{\"DestroyCmd\":{{\"target\":\"{peer}\"}}}}"),
        }
    }

    /// 队列落盘往返：重开同一目录仍在；重放同一目标只保留最新一条。
    #[test]
    fn orders_roundtrip_and_dedup() {
        let dir = tempfile::tempdir().unwrap();
        let mk = vault_crypto::random_key();
        let mut st = OrderStore::load_or_new(dir.path(), &mk).unwrap();
        assert_eq!(st.count(), 0);
        st.push(rec("vd-a", 1, 0)).unwrap();
        st.push(rec("vd-b", 2, 30)).unwrap();
        st.push(rec("vd-a", 3, 0)).unwrap(); // 同目标重复签发 → 替换
        assert_eq!(st.count(), 2);
        assert_eq!(st.list_for("vd-a").len(), 1);
        assert_eq!(st.list_for("vd-a")[0].issued_ms, 3);
        assert_eq!(st.list_for("vd-b")[0].delay_secs, 30);

        drop(st);
        let st2 = OrderStore::load_or_new(dir.path(), &mk).unwrap();
        assert_eq!(st2.count(), 2, "重开同一目录队列必须仍在");
        assert_eq!(st2.list_for("vd-b")[0].sig_hex, "sig-2");

        // 错误密钥解不开（AEAD 封装与 peers.enc 同款）
        assert!(OrderStore::load_or_new(dir.path(), &vault_crypto::random_key()).is_err());

        // remove：命中才落盘，未命中返回 false
        let mut st3 = OrderStore::load_or_new(dir.path(), &mk).unwrap();
        assert!(!st3.remove("vd-b", "vd-b", 99).unwrap());
        assert!(st3.remove("vd-b", "vd-b", 2).unwrap());
        assert_eq!(st3.count(), 1);
        drop(st3);
        assert_eq!(OrderStore::load_or_new(dir.path(), &mk).unwrap().count(), 1);
    }

    #[test]
    fn clear_and_empty_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let mk = vault_crypto::random_key();
        let mut st = OrderStore::load_or_new(dir.path(), &mk).unwrap();
        assert_eq!(st.clear().unwrap(), 0, "空队列取消投递 → 0");
        st.push(rec("vd-a", 1, 0)).unwrap();
        st.push(rec("vd-c", 2, 0)).unwrap();
        assert_eq!(st.clear().unwrap(), 2);
        assert_eq!(st.count(), 0);
        drop(st);
        let mut st2 = OrderStore::load_or_new(dir.path(), &mk).unwrap();
        assert_eq!(st2.count(), 0);
        assert_eq!(st2.clear().unwrap(), 0);
        // 截断/损坏的 blob 必须报错而不是静默当空队列（避免"看起来已取消"）
        std::fs::write(dir.path().join("orders.enc"), b"short").unwrap();
        assert!(OrderStore::load_or_new(dir.path(), &mk).is_err());
    }

    /// 队列文件是密文：条目里的 device_id / 签名 / 载荷都不得以明文出现（零信任红线）。
    #[test]
    fn orders_file_is_encrypted() {
        let dir = tempfile::tempdir().unwrap();
        let mk = vault_crypto::random_key();
        let mut st = OrderStore::load_or_new(dir.path(), &mk).unwrap();
        st.push(rec("vd-visible-marker", 1, 0)).unwrap();
        drop(st);
        let blob = std::fs::read(dir.path().join("orders.enc")).unwrap();
        let needle = b"vd-visible-marker";
        assert!(
            !blob.windows(needle.len()).any(|w| w == &needle[..]),
            "队列条目不得明文落盘"
        );
    }
}
