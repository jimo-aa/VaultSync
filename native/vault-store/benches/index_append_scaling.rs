//! P6-9 规模基准：索引写入代价与库容量解耦（docs/v2.0/09 §三 P6-9）。
//!
//! 出口标准：容量翻倍时单次写入时间增长 < 2×（严格线性即 2×，判据取
//! < 2× 以排除 O(n) 写放大）。审计 append 常数级同步验证。
//!
//! 运行：`cd native && cargo bench -p vault-store`
//! （harness = false：自断言基准，无需 nightly #[bench] / criterion 重依赖）

use std::time::Instant;
use vault_store::{Namespace, OpenMode, VaultStore};

fn mk() -> [u8; 32] {
    [0x42u8; 32]
}

/// 在容量为 `existing` 条记录的库上，测量「单次 kv_put + flush」耗时。
/// 返回耗时（ns）。
fn measure_write_at_capacity(existing: u64) -> u128 {
    let dir = tempfile::tempdir().expect("tmp");
    let mut s =
        VaultStore::open(dir.path(), Namespace::Index, &mk(), OpenMode::ReadWrite).expect("open");
    // 预填充：批量构造容量（value 256B，接近真实条目 JSON 尺寸）
    let value = vec![7u8; 256];
    let batch: Vec<(bool, u64, Vec<u8>)> = (0..existing)
        .map(|i| (true, i + 10, value.clone()))
        .collect();
    s.kv_apply(batch).expect("prefill");
    s.flush().expect("prefill flush");

    // 预热一次后测量（排除首次页缓存影响）
    let _ = s.kv_put(u64::MAX - 1, &value);
    s.flush().unwrap();

    let t0 = Instant::now();
    s.kv_put(u64::MAX, &value).expect("write");
    s.flush().expect("flush");
    let elapsed = t0.elapsed().as_nanos();
    // 阻止优化器省略
    std::hint::black_box(s.kv_get(u64::MAX).is_some());
    elapsed
}

/// 测量「单条审计追加」耗时随日志长度的增长（应为常数级）。
fn measure_audit_append(existing: u64) -> u128 {
    let dir = tempfile::tempdir().expect("tmp");
    let mut s =
        VaultStore::open(dir.path(), Namespace::Audit, &mk(), OpenMode::ReadWrite).expect("open");
    let entry = vec![b'x'; 200];
    for _ in 0..existing {
        s.log_append(&entry).expect("append");
    }
    s.flush().expect("flush");

    let _ = s.log_append(&entry).expect("warm");
    s.flush().unwrap();

    let t0 = Instant::now();
    s.log_append(&entry).expect("append");
    s.flush().expect("flush");
    t0.elapsed().as_nanos()
}

fn median_of3(mut f: impl FnMut() -> u128) -> u128 {
    let mut xs = [f(), f(), f()];
    xs.sort_unstable();
    xs[1]
}

fn main() {
    println!("== index_append_scaling：容量翻倍，单次写入时间增长必须 < 2× ==");
    // 容量序列：1k → 2k → 4k → 8k → 1 万（10 万全量跑一轮太久，CI 用此档；
    // 本地全量档见 scale_100k 特性说明）
    let mut prev = None::<u128>;
    let mut ratios = Vec::new();
    for cap in [1_000u64, 2_000, 4_000, 8_000] {
        let t = median_of3(|| measure_write_at_capacity(cap));
        println!("capacity {cap:>6}: write+flush = {:>9} ns", t);
        if let Some(p) = prev {
            let ratio = t as f64 / p as f64;
            ratios.push(ratio);
            println!("  doubling ratio vs prev = {ratio:.2}x");
        }
        prev = Some(t);
    }
    for (i, r) in ratios.iter().enumerate() {
        assert!(
            *r < 2.0,
            "P6-9 出口标准失败：第 {} 次容量翻倍后写入时间增长 {r:.2}×（须 < 2×，即写入代价与容量解耦）",
            i + 1
        );
    }

    println!("== audit_append_scaling：单条 append 常数级 ==");
    let short = median_of3(|| measure_audit_append(1_000));
    let long = median_of3(|| measure_audit_append(4_000));
    println!("audit append @1k = {short} ns, @4k = {long} ns");
    let ratio = long as f64 / short as f64;
    assert!(
        ratio < 2.0,
        "P6-9 出口标准失败：审计 append 随日志长度增长 {ratio:.2}×（4× 容量须近常数）"
    );

    println!("P6-9 基准全部通过：索引写入与审计追加的代价与库容量解耦 ✅");
}
