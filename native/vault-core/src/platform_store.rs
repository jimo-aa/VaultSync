//! 平台安全存储抽象：KEK_bio 的托管载体（docs/05-01 §3.1）。
//!
//! Windows → Credential Manager（keyring windows-native）
//! macOS   → Keychain（keyring apple-native）
//! Linux   → 无编译进后端 → [`SecureStore::open`] 返回 None → 生物识别路径禁用，
//!            主密码路径不受影响（docs/05-01 §六降级矩阵）。
use vault_crypto::{hex_decode, hex_encode, KEY_LEN};

/// 平台安全存储 trait。生产实现见 [`OsSecureStore`]；测试用内存实现。
pub trait SecureStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, &'static str>;
    fn set(&self, account: &str, value: &str) -> Result<(), &'static str>;
    fn delete(&self, account: &str) -> Result<(), &'static str>;
}

const SERVICE: &str = "VaultSync.KEK_bio";

pub struct OsSecureStore;

impl SecureStore for OsSecureStore {
    fn get(&self, account: &str) -> Result<Option<String>, &'static str> {
        match keyring::Entry::new(SERVICE, account)
            .map_err(|_| "keyring entry unavailable")?
            .get_password()
        {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err("secure store read failed"),
        }
    }

    fn set(&self, account: &str, value: &str) -> Result<(), &'static str> {
        keyring::Entry::new(SERVICE, account)
            .map_err(|_| "keyring entry unavailable")?
            .set_password(value)
            .map_err(|_| "secure store write failed")
    }

    fn delete(&self, account: &str) -> Result<(), &'static str> {
        match keyring::Entry::new(SERVICE, account)
            .map_err(|_| "keyring entry unavailable")?
            .delete_credential()
        {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("secure store delete failed"),
        }
    }
}

/// 平台存储是否可用（不可用 = 生物识别路径整体禁用）。
pub fn open_os_store() -> Option<OsSecureStore> {
    // 首次 Entry 构造即探测平台后端可用性。
    keyring::Entry::new(SERVICE, "probe")
        .ok()
        .map(|_| OsSecureStore)
}

/// KEK_bio 存取：以 vault 路径的哈希为 account，二进制以 hex 承载。
pub mod kek_bio {
    use super::*;

    fn account_for(vault_path: &std::path::Path) -> String {
        let p = vault_path.to_string_lossy();
        let bytes = p.as_bytes();
        // 路径可能含非 ASCII/超长，压缩为稳定短标识（FNV-1a 64 + 长度）
        let mut h: u64 = 0xcbf29ce484222325;
        for b in bytes {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x100000001b3);
        }
        format!("vault-{h:016x}-{}", bytes.len())
    }

    pub fn load(
        store: &dyn SecureStore,
        vault_path: &std::path::Path,
    ) -> Result<Option<[u8; KEY_LEN]>, &'static str> {
        match store.get(&account_for(vault_path))? {
            None => Ok(None),
            Some(hex) => {
                let raw = hex_decode(&hex).ok_or("corrupted KEK_bio in secure store")?;
                let key: [u8; KEY_LEN] = raw
                    .try_into()
                    .map_err(|_| "corrupted KEK_bio in secure store")?;
                Ok(Some(key))
            }
        }
    }

    pub fn store(
        store: &dyn SecureStore,
        vault_path: &std::path::Path,
        kek: &[u8; KEY_LEN],
    ) -> Result<(), &'static str> {
        store.set(&account_for(vault_path), &hex_encode(kek))
    }

    pub fn remove(
        store: &dyn SecureStore,
        vault_path: &std::path::Path,
    ) -> Result<(), &'static str> {
        store.delete(&account_for(vault_path))
    }
}

#[cfg(test)]
pub(crate) mod mock {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// 内存安全存储（单测用，模拟平台安全区）。
    #[derive(Default)]
    pub struct MockStore(pub Mutex<HashMap<String, String>>);

    impl SecureStore for MockStore {
        fn get(&self, account: &str) -> Result<Option<String>, &'static str> {
            Ok(self
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(account)
                .cloned())
        }
        fn set(&self, account: &str, value: &str) -> Result<(), &'static str> {
            self.0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(account.into(), value.into());
            Ok(())
        }
        fn delete(&self, account: &str) -> Result<(), &'static str> {
            self.0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(account);
            Ok(())
        }
    }

    /// 拒绝一切写入的存储（模拟安全区不可用平台）。
    pub struct UnavailableStore;
    impl SecureStore for UnavailableStore {
        fn get(&self, _: &str) -> Result<Option<String>, &'static str> {
            Err("secure store unavailable")
        }
        fn set(&self, _: &str, _: &str) -> Result<(), &'static str> {
            Err("secure store unavailable")
        }
        fn delete(&self, _: &str) -> Result<(), &'static str> {
            Err("secure store unavailable")
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn hex_key_roundtrip_via_mock() {
        let store = mock::MockStore::default();
        let path = std::path::Path::new("C:/v/x.vsvb");
        assert!(matches!(kek_bio::load(&store, path), Ok(None)));
        let kek: [u8; KEY_LEN] = core::array::from_fn(|i| i as u8);
        kek_bio::store(&store, path, &kek).expect("store ok");
        assert_eq!(
            kek_bio::load(&store, path).ok().flatten().as_ref(),
            Some(&kek)
        );
        kek_bio::remove(&store, path).expect("remove ok");
        assert!(matches!(kek_bio::load(&store, path), Ok(None)));
    }
}
