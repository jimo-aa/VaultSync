//! 架构边界测试（docs/v2.0/02 §三）：`vault-net` **不得知道同步语义**——
//! 按 crate 公开 API 名称黑名单断言（面板 P8-1 证据 `arch::net_has_no_sync_semantics`）。
#![forbid(unsafe_code)]

/// 同步语义符号黑名单：`vault-net` 公开 API 中出现任一子串即为架构违规。
const FORBIDDEN: &[&str] = &[
    "manifest",
    "chunk_hash",
    "block_hash",
    "vector_clock",
    "vc_",
    "_vc",
    "file_id_set",
    "sync_manifest",
    "conflict",
    "folder",
];

/// 反射 `vault-net` 的公开 API（导出函数 / 结构 / 方法签名文本化后核对）。
/// 无外部反射设施，采用「源码公开项清单」口径：对 `pub` 项做 grep 级断言的
/// 等价物 —— 由构建期 `rustdoc` 不可用，这里直接对 crates 源码的 `pub` 行扫描。
fn public_api_lines() -> Vec<String> {
    let mut out = Vec::new();
    for f in ["lib.rs", "queue.rs", "frame.rs", "part.rs", "path.rs"] {
        let path = format!("{}/src/{f}", env!("CARGO_MANIFEST_DIR"));
        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };
        for line in src.lines() {
            let t = line.trim_start();
            if t.starts_with("pub ") || t.starts_with("pub(crate) ") {
                out.push(t.to_string());
            }
        }
    }
    out
}

#[test]
fn net_has_no_sync_semantics() {
    let api = public_api_lines();
    assert!(!api.is_empty(), "公开 API 清单不得为空（扫描路径失效）");
    for line in &api {
        for bad in FORBIDDEN {
            assert!(
                !line.contains(bad),
                "vault-net 公开 API 出现同步语义符号 {bad:?}: {line}"
            );
        }
    }
}

/// 依赖矩阵（02 §3.1）：vault-net 只依赖 vault-crypto（strict 下三角）。
#[test]
fn net_depends_only_on_vault_crypto() {
    let toml = std::fs::read_to_string(format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR")))
        .expect("cargo toml");
    for line in toml.lines() {
        let t = line.trim_start();
        if t.starts_with("vault-") && t.contains("path =") {
            assert!(
                t.starts_with("vault-crypto"),
                "vault-net 只允许依赖 vault-crypto，发现: {t}"
            );
        }
    }
}
