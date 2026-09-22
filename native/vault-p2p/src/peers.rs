//! 已配对设备登记表：AEAD 加密落盘于保险箱数据目录（密钥 = HKDF(MK,"p2p-peers")）。
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vault_crypto::kdf::hkdf_sha256_derive;
use vault_crypto::{aead_decrypt, aead_encrypt, random_bytes, AES_GCM_NONCE_LEN, KEY_LEN};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerRec {
    pub name: String,
    /// Ed25519 公钥 hex（指纹 / 信令验签 / 信道静态密钥换算源）。
    pub pub_hex: String,
    pub paired_ms: u64,
    /// 本机向该对端发送信令的累计计数（保留字段，时钟位即设备 ID）。
    pub counter: u64,
    /// 对端最近一次已知监听地址（`ip:port`），供离线销毁指令自动投递（docs/05-06 §5.1）。
    /// `#[serde(default)]`：P5-5 之前的 peers.enc 没有该字段，旧盘必须仍可读。
    /// 该值只是"尽力"可达地址（局域网直连）；经中继接入时的源地址不写入（见引擎注释）。
    #[serde(default)]
    pub addr: Option<String>,
    /// P7-8：对端混合 KEM 能力（由其 Hello `pq` 声明，经信道内认证后记录）。
    /// `#[serde(default)]`：P7-8 之前的记录视为 false——对旧端永不提议 suite 2。
    #[serde(default)]
    pub pq_cap: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct PeerData {
    peers: BTreeMap<String, PeerRec>,
}

pub struct PeerStore {
    d: PeerData,
    path: PathBuf,
    key: [u8; KEY_LEN],
}

impl PeerStore {
    pub fn load_or_new(dir: &Path, mk: &[u8; KEY_LEN]) -> Result<Self, &'static str> {
        std::fs::create_dir_all(dir).map_err(|_| "cannot create p2p dir")?;
        let key = *hkdf_sha256_derive(mk, b"p2p-peers");
        let path = dir.join("peers.enc");
        let d = match std::fs::read(&path) {
            Ok(blob) => {
                if blob.len() < AES_GCM_NONCE_LEN + 16 {
                    return Err("peer store truncated");
                }
                let pt = aead_decrypt(&key, &blob).ok_or("peer store decrypt failed")?;
                serde_json::from_slice(&pt).map_err(|_| "peer store parse failed")?
            }
            Err(_) => PeerData::default(),
        };
        Ok(Self { d, path, key })
    }

    pub fn get(&self, device_id: &str) -> Option<PeerRec> {
        self.d.peers.get(device_id).cloned()
    }

    pub fn by_pub(&self, pub_hex: &str) -> Option<(String, PeerRec)> {
        self.d
            .peers
            .iter()
            .find(|(_, p)| p.pub_hex == pub_hex)
            .map(|(k, v)| (k.clone(), v.clone()))
    }

    pub fn upsert(&mut self, device_id: &str, rec: PeerRec) -> Result<(), &'static str> {
        self.d.peers.insert(device_id.to_string(), rec);
        self.save()
    }

    /// 记录/更新对端已知监听地址（UI 兜底手填或配对时观测所得）；未登记的对端不新建条目。
    pub fn set_addr(&mut self, device_id: &str, addr: &str) -> Result<(), &'static str> {
        let rec = self.d.peers.get_mut(device_id).ok_or("unknown peer")?;
        if rec.addr.as_deref() == Some(addr) {
            return Ok(()); // 幂等：地址未变不落盘
        }
        rec.addr = Some(addr.to_string());
        self.save()
    }

    /// 解除配对：存在则删除并落盘；返回是否删除过（不存在时不改动落盘）。
    pub fn remove(&mut self, device_id: &str) -> Result<bool, &'static str> {
        if self.d.peers.remove(device_id).is_none() {
            return Ok(false);
        }
        self.save()?;
        Ok(true)
    }

    pub fn list(&self) -> Vec<(String, PeerRec)> {
        self.d
            .peers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    fn save(&self) -> Result<(), &'static str> {
        let json = serde_json::to_vec(&self.d).map_err(|_| "peer serialize failed")?;
        let nonce_v = random_bytes(AES_GCM_NONCE_LEN);
        let mut nonce = [0u8; AES_GCM_NONCE_LEN];
        nonce.copy_from_slice(&nonce_v);
        let full = aead_encrypt(&self.key, &nonce, &json).map_err(|_| "peer encrypt failed")?;
        // 落盘 blob = nonce ‖ ct‖tag（aead_encrypt 返回 nonce‖ct‖tag，直接整体落盘）
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, &full).map_err(|_| "cannot write peers")?;
        std::fs::rename(&tmp, &self.path).map_err(|_| "cannot finalize peers")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_wrong_key() {
        let dir = tempfile::tempdir().unwrap();
        let mk = vault_crypto::random_key();
        let mut st = PeerStore::load_or_new(dir.path(), &mk).unwrap();
        st.upsert(
            "vd-abc",
            PeerRec {
                name: "Laptop".into(),
                pub_hex: "aa".repeat(32),
                paired_ms: 1,
                counter: 0,
                addr: None,
                pq_cap: false,
            },
        )
        .unwrap();
        drop(st);
        let st2 = PeerStore::load_or_new(dir.path(), &mk).unwrap();
        assert_eq!(st2.get("vd-abc").unwrap().name, "Laptop");
        assert!(st2.by_pub(&"aa".repeat(32)).is_some());
        // 错误密钥解不开
        assert!(PeerStore::load_or_new(dir.path(), &vault_crypto::random_key()).is_err());
    }

    #[test]
    fn remove_unpair_and_persist() {
        let dir = tempfile::tempdir().unwrap();
        let mk = vault_crypto::random_key();
        let rec = |name: &str, tag: char| PeerRec {
            name: name.into(),
            pub_hex: tag.to_string().repeat(64),
            paired_ms: 1,
            counter: 0,
            addr: None,
            pq_cap: false,
        };
        let mut st = PeerStore::load_or_new(dir.path(), &mk).unwrap();
        st.upsert("vd-a", rec("A", 'a')).unwrap();
        st.upsert("vd-b", rec("B", 'b')).unwrap();
        assert_eq!(st.list().len(), 2);

        assert!(st.remove("vd-a").unwrap(), "存在则删除返回 true");
        let rest = st.list();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].0, "vd-b");
        assert!(st.by_pub(&"a".repeat(64)).is_none());

        // 不存在的 id：Ok(false) 且不落盘
        assert!(!st.remove("vd-absent").unwrap());
        assert!(!st.remove("vd-a").unwrap());
        assert_eq!(st.list().len(), 1);

        // 重载后删除结果持久化
        drop(st);
        let st2 = PeerStore::load_or_new(dir.path(), &mk).unwrap();
        assert_eq!(st2.list().len(), 1);
        assert!(st2.get("vd-a").is_none());
        assert!(st2.get("vd-b").is_some());
    }

    /// P5-5：旧 peers.enc（无 addr 字段）必须仍可解析；新字段缺省为 None。
    #[test]
    fn legacy_peer_json_without_addr_still_reads() {
        let rec: PeerRec =
            serde_json::from_str(r#"{"name":"Old","pub_hex":"ab","paired_ms":7,"counter":3}"#)
                .unwrap();
        assert!(rec.addr.is_none());
        assert_eq!(rec.paired_ms, 7);
        // P7-8：无 pqCap 字段 → false（对旧端永不提议 suite 2）
        assert!(!rec.pq_cap);
        // 反向：带 addr 的也有序化/反序列化一致
        let mut rec2 = rec.clone();
        rec2.addr = Some("10.0.0.5:41000".into());
        rec2.pq_cap = true;
        let s = serde_json::to_string(&rec2).unwrap();
        assert!(s.contains("\"addr\":\"10.0.0.5:41000\""));
        assert!(s.contains("\"pq_cap\":true"));
        assert_eq!(serde_json::from_str::<PeerRec>(&s).unwrap().addr, rec2.addr);
        assert!(serde_json::from_str::<PeerRec>(&s).unwrap().pq_cap);
    }

    #[test]
    fn set_addr_persists_and_rejects_unknown_peer() {
        let dir = tempfile::tempdir().unwrap();
        let mk = vault_crypto::random_key();
        let mut st = PeerStore::load_or_new(dir.path(), &mk).unwrap();
        // 未登记对端不得被凭空创建
        assert!(st.set_addr("vd-ghost", "10.0.0.9:1234").is_err());
        assert!(st.get("vd-ghost").is_none());

        st.upsert(
            "vd-b",
            PeerRec {
                name: "B".into(),
                pub_hex: "b".repeat(64),
                paired_ms: 1,
                counter: 0,
                addr: None,
                pq_cap: false,
            },
        )
        .unwrap();
        st.set_addr("vd-b", "10.0.0.9:1234").unwrap();
        st.set_addr("vd-b", "10.0.0.9:1234").unwrap(); // 幂等
        st.set_addr("vd-b", "10.0.0.9:2233").unwrap(); // 覆盖
        drop(st);
        let st2 = PeerStore::load_or_new(dir.path(), &mk).unwrap();
        assert_eq!(
            st2.get("vd-b").unwrap().addr.as_deref(),
            Some("10.0.0.9:2233")
        );
    }
}
