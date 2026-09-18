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
}
