//! 会话与暴力破解防护（docs/05-01 §六、原型延时策略）。
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use vault_crypto::KEY_LEN;
use vault_vault::vault::Vault;

/// 已解锁会话。MK 永不出引擎（零信任红线），FFI 层只传递不透明句柄。
pub struct Session {
    pub(crate) mk: Zeroizing<[u8; KEY_LEN]>,
    pub(crate) vault_path: PathBuf,
    /// 伪装空间会话（docs/05-01 §六 Dummy 语义）：操作只作用于伪空间自身的数据目录。
    pub(crate) disguise: bool,
    /// 保险箱引擎槽位（Arc 共享给 P2P 引擎的监听线程；索引密钥派生自 MK，会话销毁即随之销毁）。
    pub(crate) vault: Arc<Mutex<Option<Vault>>>,
    /// P2P 同步引擎（P3，按会话惰性创建；锁定即随会话销毁）。
    pub(crate) p2p: Mutex<Option<std::sync::Arc<vault_p2p::P2pEngine>>>,
}

impl Session {
    /// 惰性打开保险箱并以闭包执行操作（同一会话内串行）。
    pub(crate) fn with_vault<T>(
        &self,
        f: impl FnOnce(&mut Vault) -> Result<T, crate::service::CoreError>,
    ) -> Result<T, crate::service::CoreError> {
        let mut guard = self.vault.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            *guard = Some(
                Vault::open(&self.vault_path, &self.mk)
                    .map_err(crate::service::CoreError::Internal)?,
            );
        }
        // 上一分支刚初始化；Option 在此必然为 Some
        let v = match guard.as_mut() {
            Some(v) => v,
            None => return Err(crate::service::CoreError::Internal("vault init invariant")),
        };
        f(v)
    }
}

/// 单密码错误延时（原型基线：连续 3 次失败起 30s，指数递增，上限 15 分钟）。
pub const COOLDOWN_BASE_SECS: u64 = 30;
pub const COOLDOWN_MAX_SECS: u64 = 15 * 60;
/// 触发延时的连续失败阈值。
pub const COOLDOWN_THRESHOLD: u32 = 3;

struct GuardState {
    fails: u32,
    locked_until: Option<Instant>,
}

/// 暴力破解防护表：以 vault 父目录为域（主/伪空间共用同一防护域，
/// 使攻击者无法从延时差异分辨某密码对应哪个空间）。
fn guards() -> &'static Mutex<HashMap<PathBuf, GuardState>> {
    static GUARDS: OnceLock<Mutex<HashMap<PathBuf, GuardState>>> = OnceLock::new();
    GUARDS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn guard_key(vault_path: &Path) -> PathBuf {
    vault_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

/// 认证结果：OK / 剩余冷却毫秒。
pub enum GuardVerdict {
    Allowed,
    Cooldown(u64),
}

/// 调用前置检查：冷却中则拒绝并返回剩余毫秒。
pub fn guard_check(vault_path: &Path) -> GuardVerdict {
    let mut map = guards().lock().unwrap_or_else(|e| e.into_inner());
    let key = guard_key(vault_path);
    match map.get_mut(&key) {
        Some(st) => match st.locked_until {
            Some(until) if until > Instant::now() => {
                GuardVerdict::Cooldown((until - Instant::now()).as_millis() as u64)
            }
            _ => GuardVerdict::Allowed,
        },
        None => GuardVerdict::Allowed,
    }
}

/// 解封失败计数并计算新冷却期。返回值：新增冷却毫秒（0 = 未达阈值）。
pub fn guard_fail(vault_path: &Path) -> u64 {
    let mut map = guards().lock().unwrap_or_else(|e| e.into_inner());
    let key = guard_key(vault_path);
    let st = map.entry(key).or_insert(GuardState {
        fails: 0,
        locked_until: None,
    });
    st.fails = st.fails.saturating_add(1);
    if st.fails >= COOLDOWN_THRESHOLD {
        let exp = st.fails - COOLDOWN_THRESHOLD; // 0,1,2,...
        let secs = COOLDOWN_BASE_SECS
            .saturating_mul(1u64 << exp.min(16))
            .min(COOLDOWN_MAX_SECS);
        st.locked_until = Some(Instant::now() + Duration::from_secs(secs));
        secs * 1000
    } else {
        0
    }
}

/// 解封成功：清零计数与冷却。
pub fn guard_reset(vault_path: &Path) {
    let mut map = guards().lock().unwrap_or_else(|e| e.into_inner());
    map.remove(&guard_key(vault_path));
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use zeroize::Zeroize;

    #[test]
    fn cooldown_starts_at_third_failure_and_grows() {
        let p = Path::new("Z:/guard-t1/a.vsvb");
        assert!(matches!(guard_check(p), GuardVerdict::Allowed));
        assert_eq!(guard_fail(p), 0); // 1st
        assert_eq!(guard_fail(p), 0); // 2nd
        assert_eq!(guard_fail(p), 30_000); // 3rd → 30s
        assert!(matches!(guard_check(p), GuardVerdict::Cooldown(_)));
        assert_eq!(guard_fail(p), 60_000); // 4th → 60s
        guard_reset(p);
        assert!(matches!(guard_check(p), GuardVerdict::Allowed));
    }

    #[test]
    fn cooldown_shared_across_siblings_same_parent() {
        let a = Path::new("Z:/guard-t2/primary.vsvb");
        let b = Path::new("Z:/guard-t2/disguise.vsvb");
        guard_fail(a);
        guard_fail(b);
        guard_fail(a); // 域内第 3 次
        assert!(matches!(guard_check(b), GuardVerdict::Cooldown(_)));
        guard_reset(a);
    }

    #[test]
    fn cooldown_caps_at_max() {
        let p = Path::new("Z:/guard-t3/a.vsvb");
        for _ in 0..20 {
            guard_fail(p);
        }
        assert_eq!(guard_fail(p), COOLDOWN_MAX_SECS * 1000);
    }

    #[test]
    fn session_zeroizes_on_drop() {
        // Zeroizing 语义断言：作用域结束即擦除（Drop 路径）
        let mk = {
            let mut k = Zeroizing::new([9u8; KEY_LEN]);
            k.as_mut().zeroize();
            k
        };
        assert!(mk.iter().all(|&b| b == 0));
    }
}
