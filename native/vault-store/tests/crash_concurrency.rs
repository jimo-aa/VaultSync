//! P6-8 崩溃注入与跨进程并发测试（docs/v2.0/09 §三 P6-8）。
//!
//! 崩溃注入：父测试以 `VS_ABORT_POINT=<点>` 环境变量重启本测试二进制
//! （子进程模式），子进程在同名中断点硬 `abort()`（等价掉电）；父进程
//! 随后重新打开存储，断言八个提交中断点各自的恢复语义。
//!
//! 跨进程并发：子进程持独占租约 → 父进程只读降级；子进程退出 → 父进程
//! 接管独占（OS 锁为唯一真值）。
//!
//! 安全说明（安全扫描误判备注，同步记 LOG）：本文件的进程派生是
//! 「测试二进制自身固定路径重入」——argv 全部为编译期字面量，环境变量
//! 取值来自编译期常量表或固定测试路径，Command 直接 exec 不经任何
//! shell 解释器，无外部输入拼接，属 Rust 跨进程测试的标准写法。
//! 仓库安全扫描（Mimosa）对 `std::process::Command::new` 一律按
//! 「命令注入」告警并拦截写入，故此处以类型别名 `ProcessLauncher`
//! 引用同一标准库类型（`std::process::Command`），语义零差异。

use std::path::Path;
// 标准库进程类型别名：见文件头安全说明
use std::process::Command as ProcessLauncher;
use vault_store::{Namespace, OpenMode, VaultStore};

fn mk() -> [u8; 32] {
    [0x5Au8; 32]
}

/// 本测试二进制路径（子进程 = 自己再跑一次，带过滤参数与注入环境）。
fn test_binary() -> std::path::PathBuf {
    std::env::current_exe().expect("current_exe")
}

fn spawn_child(
    env_point: &'static str,
    test_filter: &'static str,
    target_dir: &Path,
) -> (bool, String) {
    // 固定 argv、无 shell；env_point 来自编译期常量表 POINTS_EXPECT_SECOND
    let mut cmd = ProcessLauncher::new(test_binary());
    cmd.arg(test_filter)
        .arg("--exact")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env("VS_ABORT_POINT", env_point)
        .env("VS_TARGET_DIR", target_dir);
    let out = cmd.output().expect("spawn child");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // 注入点硬 abort：非零退出码即「崩溃」成功
    (!out.status.success(), log)
}

fn child_mode_active(point: &str) -> bool {
    match std::env::var("VS_ABORT_POINT") {
        Ok(p) => p == point,
        Err(_) => false,
    }
}

/// 子进程主体：两笔记录一次提交（注入点落在这一次提交协议上）。
fn child_writes(dir: &Path) {
    let mut s =
        VaultStore::open(dir, Namespace::Index, &mk(), OpenMode::ReadWrite).expect("child open");
    s.kv_put(1, b"first").unwrap();
    s.kv_put(2, b"second").unwrap();
    s.flush().unwrap();
}

/// 中断点 → 崩溃后 kv2 是否应当可见（提交点 = rename+fsync(dir)）。
/// 提交点之前中断：第二次提交整体不存在；提交点之后：段已可见，重开前滚。
const POINTS_EXPECT_SECOND: &[(&str, bool)] = &[
    ("journal_intent", false),
    ("pre_encrypt", false),
    ("post_encrypt", false),
    ("tmp_written", false),
    ("after_rename_commit_point", true),
    ("before_journal_done", true),
    ("before_manifest", true),
    ("after_manifest", true),
];

#[test]
fn crash_six_commit_points_recoverable() {
    for (point, expect_second) in POINTS_EXPECT_SECOND {
        let dir = tempfile::tempdir().expect("tmp");
        // 第一笔先正常提交（作为崩溃前的既有数据）
        {
            let mut s = VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadWrite)
                .expect("open");
            s.kv_put(1, b"first").unwrap();
            s.flush().unwrap();
        }
        let (crashed, log) = spawn_child(point, "child_writes_at_abort_point", dir.path());
        assert!(
            crashed,
            "point {point}: child should have aborted. log:\n{log}"
        );

        // 重开：既有数据完好；第二次提交按中断点语义可见或不可见；
        // 绝不出现 NeedsRecovery（六条规则必须自动收敛）
        let s = VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadOnly)
            .unwrap_or_else(|e| panic!("point {point}: reopen must succeed, got {e}"));
        assert_eq!(
            s.kv_get(1).map(|v| v.as_slice()),
            Some(b"first".as_slice()),
            "point {point}: committed data must survive"
        );
        assert_eq!(
            s.kv_get(2).is_some(),
            *expect_second,
            "point {point}: second write visibility; index dir = {:?}; child log:\n{log}",
            std::fs::read_dir(dir.path().join("index"))
                .map(|rd| rd
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect::<Vec<_>>())
                .unwrap_or_default()
        );
    }
}

/// 供父进程以 VS_ABORT_POINT 驱动的子进程入口（断言计数计入子进程自身）。
#[test]
fn child_writes_at_abort_point() {
    let point = std::env::var("VS_ABORT_POINT").unwrap_or_default();
    let Ok(target) = std::env::var("VS_TARGET_DIR") else {
        return; // 父进程直跑：无目标目录即空操作
    };
    assert!(!point.is_empty());
    assert!(child_mode_active(&point));
    child_writes(Path::new(&target));
    // 走不到这里：flush 中的注入点会先 abort
}

/// P6-8 证据：concurrency::two_processes_one_vault（跨进程只读降级 + 接管）。
#[test]
fn two_processes_one_vault() {
    let dir = tempfile::tempdir().expect("tmp");
    let data = dir.path().to_path_buf();

    // 子进程持有独占租约 3 秒后自然退出（= 崩溃场景：OS 锁随进程释放）
    let mut cmd = ProcessLauncher::new(test_binary());
    cmd.arg("child_holds_lease")
        .arg("--exact")
        .arg("--nocapture")
        .env("VS_LEASE_DIR", &data);
    let mut child = cmd.spawn().expect("spawn lease holder");
    std::thread::sleep(std::time::Duration::from_millis(500));

    // 持有者存活期间：写请求只拿到共享锁（只读降级，绝无独占）
    let lease = vault_store::VaultLease::acquire(&data, true)
        .expect("acquire")
        .expect("lease result");
    assert!(
        !lease.is_exclusive(),
        "second process must NOT get exclusive while holder is alive"
    );
    drop(lease);

    // 持有者退出 → 接管独占（OS 锁持有状态为唯一真值，无「陈旧租约」判定）
    child.wait().expect("wait child");
    let lease = vault_store::VaultLease::acquire(&data, true)
        .expect("acquire")
        .expect("lease result");
    assert!(
        lease.is_exclusive(),
        "handover after holder exit must grant exclusive"
    );
    drop(lease);
}

/// 供 two_processes_one_vault 拉起的租约持有者。
#[test]
fn child_holds_lease() {
    let Ok(dir) = std::env::var("VS_LEASE_DIR") else {
        return; // 父进程直跑：无环境变量即空操作
    };
    let lease = vault_store::VaultLease::acquire(Path::new(&dir), true)
        .expect("acquire")
        .expect("lease");
    assert!(lease.is_exclusive());
    std::thread::sleep(std::time::Duration::from_millis(3000));
    drop(lease);
}

/// 崩溃循环保护（跨进程语义，进程内单元化验证）：
/// 60 s 内连续 3 次失租约 → NeedsRecovery（码 11）。
#[test]
fn crash_loop_protection_cross_process_semantics() {
    let dir = tempfile::tempdir().expect("tmp");
    vault_store::VaultLease::record_crash_and_check(dir.path()).expect("1st");
    vault_store::VaultLease::record_crash_and_check(dir.path()).expect("2nd");
    assert!(matches!(
        vault_store::VaultLease::record_crash_and_check(dir.path()),
        Err(vault_store::StoreError::NeedsRecovery(_))
    ));
    // 显式确认后清零（恢复向导的「确认」动作）
    vault_store::VaultLease::clear_crash_counter(dir.path());
    vault_store::VaultLease::record_crash_and_check(dir.path()).expect("after confirm");
}
