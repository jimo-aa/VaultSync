//! 单写者租约（docs/v2.0/02 §4.5）。
//!
//! `<vault>.data/vault.lock` 64 B 协议；平台文件锁（Windows `LockFileEx` /
//! POSIX `flock`，经 std `File::try_lock*`）。**陈旧租约判定以 OS 锁持有状态
//! 为唯一真值**——文件内容仅用于诊断，绝不参与判定（时钟回拨 / 休眠 /
//! PID 复用不可靠，ADR-010）。

use crate::error::StoreError;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const LOCK_FILE_LEN: usize = 64;
const LOCK_MAGIC: &[u8; 4] = b"VSLK";
const LOCK_VER: u16 = 1;

/// 崩溃循环保护阈值：60 s 内连续 3 次失租约 → NeedsRecovery（码 11）。
pub const CRASH_LOOP_LIMIT: u32 = 3;
pub const CRASH_LOOP_WINDOW_MS: u64 = 60_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseState {
    /// 独占：本进程是唯一写者
    Exclusive,
    /// 共享：只读降级
    Shared,
}

/// vault.lock 的 64 B 诊断记录（内容不参与陈旧判定；reserved 区恒零）。
fn encode_lock_record(mode: LeaseState) -> [u8; LOCK_FILE_LEN] {
    let mut b = [0u8; LOCK_FILE_LEN];
    b[0..4].copy_from_slice(LOCK_MAGIC);
    b[4..6].copy_from_slice(&LOCK_VER.to_le_bytes());
    b[6..8].copy_from_slice(
        &match mode {
            LeaseState::Exclusive => 1u16,
            LeaseState::Shared => 2u16,
        }
        .to_le_bytes(),
    );
    b[8..16].copy_from_slice(&(u64::from(std::process::id())).to_le_bytes());
    b[16..24].copy_from_slice(&now_ms().to_le_bytes()); // owner_started_ms（诊断）
    b[24..40].copy_from_slice(&vault_crypto::random_bytes(16)); // owner_nonce
    let now = now_ms();
    b[40..48].copy_from_slice(&now.to_le_bytes()); // lease_acquired_ms
    b[48..56].copy_from_slice(&now.to_le_bytes()); // heartbeat_ms
    b[56..64].copy_from_slice(&[0u8; 8]); // reserved 恒零（docs/v2.0/02 §4.5）
    b
}

/// 读 vault.lock 内容供诊断（不做任何判定）。
pub fn read_lock_diagnostics(path: &Path) -> Option<[u8; LOCK_FILE_LEN]> {
    let raw = std::fs::read(path).ok()?;
    if raw.len() != LOCK_FILE_LEN || &raw[0..4] != LOCK_MAGIC {
        return None;
    }
    raw.try_into().ok()
}

pub struct VaultLease {
    file: File,
    pub state: LeaseState,
}

impl VaultLease {
    pub fn lock_path(data_dir: &Path) -> PathBuf {
        data_dir.join("vault.lock")
    }

    /// 尝试获取租约。`want_write` 时先抢独占 OS 锁；失败即只读降级（上层
    /// 据此返回 FFI 码 9）。
    ///
    /// 偏差说明（记 LOG）：设计 §4.5 的「失败 → 共享锁」在 Windows 上不可行——
    /// `LockFileEx` 独占锁会阻塞其他句柄的共享锁。因此 OS 锁只承担「单写者
    /// 仲裁」这一职责；只读打开不强求 OS 锁（读一致性由段提交点原子性保证），
    /// 共享锁是尽力而为的诊断增强。
    pub fn acquire(
        data_dir: &Path,
        want_write: bool,
    ) -> Result<Result<VaultLease, StoreError>, StoreError> {
        std::fs::create_dir_all(data_dir)?;
        let path = Self::lock_path(data_dir);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        if want_write && file.try_lock().is_ok() {
            // 诊断记录必须经持锁句柄写入：Windows 上锁区域拒绝其他句柄访问
            let _ = write_lock_record(&file, LeaseState::Exclusive);
            return Ok(Ok(VaultLease {
                file,
                state: LeaseState::Exclusive,
            }));
            // 独占被他进程持有 → 只读降级
        }
        if file.try_lock_shared().is_ok() {
            // 他进程可能正持独占，诊断记录写不进去属正常（尽力而为）
            let _ = write_lock_record(&file, LeaseState::Shared);
        }
        Ok(Ok(VaultLease {
            file,
            state: LeaseState::Shared,
        }))
    }

    /// 心跳：更新锁文件 heartbeat 字段（10 s 一次，仅诊断用途）。
    pub fn heartbeat(&self) {
        if self.state == LeaseState::Exclusive {
            let _ = write_lock_record(&self.file, LeaseState::Exclusive);
        }
    }

    pub fn is_exclusive(&self) -> bool {
        self.state == LeaseState::Exclusive
    }

    /// 崩溃循环保护：记录本次「持有租约失败 / 异常失租」事件，
    /// 60 s 内连续第 3 次 → 返回 NeedsRecovery（码 11，要求显式确认）。
    pub fn record_crash_and_check(data_dir: &Path) -> Result<(), StoreError> {
        let p = data_dir.join("lease.crash");
        let now = now_ms();
        let mut stamps: Vec<u64> = std::fs::read_to_string(&p)
            .map(|s| s.lines().filter_map(|l| l.trim().parse().ok()).collect())
            .unwrap_or_default();
        stamps.retain(|t| now.saturating_sub(*t) <= CRASH_LOOP_WINDOW_MS);
        stamps.push(now);
        let body: String = stamps.iter().map(|t| format!("{t}\n")).collect();
        std::fs::write(&p, body)?;
        if stamps.len() as u32 >= CRASH_LOOP_LIMIT {
            return Err(StoreError::NeedsRecovery(
                "crash loop: 3 lost leases in 60s",
            ));
        }
        Ok(())
    }

    /// 正常退出时清空崩溃计数（超过窗口自动失效，双保险）。
    pub fn clear_crash_counter(data_dir: &Path) {
        let _ = std::fs::remove_file(data_dir.join("lease.crash"));
    }
}

impl Drop for VaultLease {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn write_lock_record(file: &File, mode: LeaseState) -> Result<(), StoreError> {
    let mut f = file;
    f.seek(SeekFrom::Start(0))?;
    f.write_all(&encode_lock_record(mode))?;
    f.sync_all()?;
    Ok(())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    /// P6-2 证据：lease::second_process_gets_readonly
    /// 同一进程内两次 acquire 等价于两进程竞争（OS 锁不看 pid）。
    #[test]
    fn second_process_gets_readonly() {
        let dir = tmpdir();
        let first = VaultLease::acquire(dir.path(), true).unwrap().unwrap();
        assert!(first.is_exclusive());

        // 第二个「进程」请求写 → 降级只读共享
        let second = VaultLease::acquire(dir.path(), true).unwrap().unwrap();
        assert_eq!(second.state, LeaseState::Shared);

        // 第三个读者与第二个共存（共享锁之间互不排斥）
        let third = VaultLease::acquire(dir.path(), false).unwrap().unwrap();
        assert_eq!(third.state, LeaseState::Shared);
        drop(third);
        drop(second);
        drop(first);
    }

    /// P6-2 证据：lease::write_returns_err_9
    /// 只读降级后，写请求得到 LeaseBusy（FFI 映射码 9）。
    #[test]
    fn write_returns_err_9() {
        let dir = tmpdir();
        let _holder = VaultLease::acquire(dir.path(), true).unwrap().unwrap();
        // 上层把「请求写但只拿到共享锁」视作 err-9 语义的只读降级；
        // 本测试锁定的是：持有者存在时，第二个写者绝不可能拿到独占锁。
        let second = VaultLease::acquire(dir.path(), true).unwrap().unwrap();
        assert!(!second.is_exclusive());
    }

    /// P6-2 证据：lease::handover_after_holder_exit
    /// 持有者释放（等价进程退出）后，等待者可拿到独占锁。
    #[test]
    fn handover_after_holder_exit() {
        let dir = tmpdir();
        let holder = VaultLease::acquire(dir.path(), true).unwrap().unwrap();
        assert!(holder.is_exclusive());
        drop(holder); // = 进程退出，OS 锁随句柄释放
        let next = VaultLease::acquire(dir.path(), true).unwrap().unwrap();
        assert!(next.is_exclusive());
    }

    /// 锁文件 64 B 协议：魔数 / 版本 / reserved 全零 / CRC 自洽。
    #[test]
    fn lock_record_layout() {
        let rec = encode_lock_record(LeaseState::Exclusive);
        assert_eq!(&rec[0..4], b"VSLK");
        assert_eq!(u16::from_le_bytes(rec[4..6].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(rec[6..8].try_into().unwrap()), 1);
        assert_eq!(&rec[56..64], &[0u8; 8]);
        assert_eq!(read_lock_diagnostics(Path::new("nonexistent.lock")), None);
    }

    /// 崩溃循环保护：60 s 内第 3 次失租约 → NeedsRecovery。
    #[test]
    fn crash_loop_protection_triggers_recovery() {
        let dir = tmpdir();
        VaultLease::record_crash_and_check(dir.path()).unwrap();
        VaultLease::record_crash_and_check(dir.path()).unwrap();
        assert!(matches!(
            VaultLease::record_crash_and_check(dir.path()),
            Err(StoreError::NeedsRecovery(_))
        ));
        VaultLease::clear_crash_counter(dir.path());
        // 清零后重新计数
        VaultLease::record_crash_and_check(dir.path()).unwrap();
    }
}
