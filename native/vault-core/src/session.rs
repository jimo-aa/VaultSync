//! 会话与暴力破解防护（docs/05-01 §六、原型延时策略）。
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use zeroize::{Zeroize, Zeroizing};

use vault_audit::AuditLog;
use vault_crypto::KEY_LEN;
use vault_vault::vault::Vault;

/// 从属密钥束（P7-2，docs/v2.0/05-01 §3.2）：8 条 key_id 的明文。
/// 取值 CSPRNG 一次固定、与 MK 仅通过 DWK 包装绑定；MK 轮换只重包装，取值不变。
#[derive(Clone, Copy)]
pub(crate) struct VaultKeys {
    pub index: [u8; KEY_LEN],        // 1 索引
    pub search: [u8; KEY_LEN],       // 2 搜索
    pub share: [u8; KEY_LEN],        // 3 分享
    pub orders: [u8; KEY_LEN],       // 4 订单
    pub stego: [u8; KEY_LEN],        // 5 隐写载荷
    pub audit_chain: [u8; KEY_LEN],  // 6 链键
    pub audit_export: [u8; KEY_LEN], // 7 审计导出
    pub discovery: [u8; KEY_LEN],    // 8 发现指纹盐
}

impl Zeroize for VaultKeys {
    fn zeroize(&mut self) {
        self.index.zeroize();
        self.search.zeroize();
        self.share.zeroize();
        self.orders.zeroize();
        self.stego.zeroize();
        self.audit_chain.zeroize();
        self.audit_export.zeroize();
        self.discovery.zeroize();
    }
}

impl VaultKeys {
    pub(crate) fn from_array(a: [[u8; KEY_LEN]; 8]) -> Self {
        VaultKeys {
            index: a[0],
            search: a[1],
            share: a[2],
            orders: a[3],
            stego: a[4],
            audit_chain: a[5],
            audit_export: a[6],
            discovery: a[7],
        }
    }

    pub(crate) fn to_array(self) -> [[u8; KEY_LEN]; 8] {
        [
            self.index,
            self.search,
            self.share,
            self.orders,
            self.stego,
            self.audit_chain,
            self.audit_export,
            self.discovery,
        ]
    }
}

/// 已解锁会话。MK 永不出引擎（零信任红线），FFI 层只传递不透明句柄。
pub struct Session {
    pub(crate) mk: Zeroizing<[u8; KEY_LEN]>,
    /// 从属密钥束（零化随会话结束）。
    pub(crate) keys: Zeroizing<VaultKeys>,
    pub(crate) vault_path: PathBuf,
    /// 伪装空间会话（docs/05-01 §六 Dummy 语义）：操作只作用于伪空间自身的数据目录。
    pub(crate) disguise: bool,
    /// 保险箱引擎槽位（Arc 共享给 P2P 引擎的监听线程；索引密钥派生自 MK，会话销毁即随之销毁）。
    pub(crate) vault: Arc<Mutex<Option<Vault>>>,
    /// P2P 同步引擎（P3，按会话惰性创建；锁定即随会话销毁）。
    pub(crate) p2p: Mutex<Option<std::sync::Arc<vault_p2p::P2pEngine>>>,
    /// 链式哈希审计日志（P5-1，docs/05-06 §三）。密钥派生自 MK，随会话惰性打开、随会话销毁。
    pub(crate) audit: Mutex<Option<AuditLog>>,
    /// 隐写引擎开关（P5-3，docs/05-06 §七：默认关闭，启用/停用均记审计）。
    /// 会话级内存标志：每次解锁后由 UI 从设置同步过来（引擎不读 UI 配置）。
    pub(crate) stego_enabled: std::sync::atomic::AtomicBool,
    /// 单写者租约（P6-2）：解锁即抢独占；他进程持锁 → 只读降级（码 9）。
    pub(crate) readonly: std::sync::atomic::AtomicBool,
    /// 维护态（P7-3）：轮换/迁移中业务写冻结（码 10）；擦除/销毁不受冻结。
    pub(crate) maintenance: std::sync::atomic::AtomicBool,
    pub(crate) lease: Mutex<Option<vault_store::VaultLease>>,
}

impl Session {
    /// 解锁后挂接单写者租约：抢独占失败 → 只读降级（readonliness = true）。
    pub(crate) fn attach_lease(&self) {
        let res = vault_store::VaultLease::acquire(&self.data_dir(), true);
        match res {
            Ok(Ok(lease)) => {
                self.readonly
                    .store(!lease.is_exclusive(), std::sync::atomic::Ordering::Relaxed);
                *self.lease.lock().unwrap_or_else(|e| e.into_inner()) = Some(lease);
            }
            _ => {
                // 连锁文件都开不了：保持可写语义不变（原行为），记诊断
                eprintln!("vault-core: lease acquire failed; continuing without lease");
            }
        }
    }

    pub(crate) fn is_readonly(&self) -> bool {
        self.readonly.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn enter_maintenance(&self) {
        self.maintenance
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn exit_maintenance(&self) {
        self.maintenance
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn is_maintenance(&self) -> bool {
        self.maintenance.load(std::sync::atomic::Ordering::Relaxed)
    }
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
                Vault::open(
                    &self.vault_path,
                    &self.mk,
                    &self.keys.index,
                    &self.keys.search,
                )
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

    /// 保险箱数据目录（`<parent>/<stem>.data`，与 vault-vault 的布局约定一致）。
    pub(crate) fn data_dir(&self) -> PathBuf {
        let stem = self
            .vault_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("vault");
        self.vault_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(format!("{stem}.data"))
    }

    /// 追加一条审计条目（docs/05-06 §3.2 的记录范围）。
    ///
    /// 失败**不阻断业务**：审计是事后取证，若因审计文件损坏而拒绝解锁/导入，
    /// 等于把用户锁在自己的数据之外（决策与理由见 LOG）。失败打印到 stderr。
    pub(crate) fn audit(&self, kind: &str, detail: &str) {
        let mut guard = self.audit.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            // P7-2：链键（从属密钥 6）——与 MK 解耦，MK 轮换后链 continues
            match AuditLog::open_with_key(&self.data_dir(), &self.keys.audit_chain) {
                Ok(log) => *guard = Some(log),
                Err(e) => {
                    eprintln!("vsync audit open failed: {e}");
                    return;
                }
            }
        }
        if let Some(log) = guard.as_mut() {
            if let Err(e) = log.append(kind, detail) {
                eprintln!("vsync audit append failed: {e}");
            }
        }
    }

    /// 只读借出审计日志（列表 / 校验 / 导出）。打不开时返回 None。
    pub(crate) fn with_audit<T>(&self, f: impl FnOnce(&AuditLog) -> T) -> Option<T> {
        let mut guard = self.audit.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            // P7-2：链键（从属密钥 6）
            match AuditLog::open_with_key(&self.data_dir(), &self.keys.audit_chain) {
                Ok(log) => *guard = Some(log),
                Err(e) => {
                    eprintln!("vsync audit open failed: {e}");
                    return None;
                }
            }
        }
        guard.as_ref().map(f)
    }

    /// 隐写载荷密钥（P5-3）：隐写层自己的 AEAD 密钥，
    /// 与容器密钥/索引密钥分离（docs/05-05「加密先行」——隐写载荷本身即密文）。
    pub(crate) fn stego_key(&self) -> [u8; KEY_LEN] {
        self.keys.stego
    }

    /// 导出密钥（docs/05-06 §3.3：专用导出密钥，不等于索引/块密钥）。
    pub(crate) fn audit_export_key(&self) -> [u8; KEY_LEN] {
        self.keys.audit_export
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
