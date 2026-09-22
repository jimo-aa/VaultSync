//! 擦除强度分级（P7-5，docs/v2.0/05-02 擦除设计）。
//!
//! 引擎侧的「介质探测 → 擦除等级分类」模块，回答一个问题：**这个文件所在的介质
//! 上，什么样的擦除手段才是可信的，当前引擎实际做到了哪一档**。V1.0 缺口⑥
//! （「安全擦除只有单次零覆写、SSD 上无效、无介质探测」）在此收口的第一步——
//! 探测与分类先行，实际的平台安全删除（TRIM）接线由后续任务完成。
//!
//! 三层概念（与 `EraseClassReport` 字段一一对应）：
//!
//! - `media_kind`：介质类别。1=SSD(含 NVMe) / 2=HDD / 3=网络盘 / 4=不可判定。
//! - `erase_class`：擦除等级。1=crypto_only / 2=zero_overwrite / 3=platform_secure。
//! - `method_bits`：**实际已生效**的方法位图。bit0=单次零覆写，bit1=多次覆写，
//!   bit2=平台安全删除（TRIM/UNMAP）。声称与实际必须分开：等级可以「允许」，
//!   位没置上就**不得对外声称**（见 [`SECURE_ERASE_CLAIM_ALLOWED`]，V2.0 原则 7 证据先行）。
//!
//! Windows 介质探测的**安全论证**（替代仓库安全扫描已约束风险的 `wmic` 方案）：
//! 我们用 `std::process::Command` 调 `fsutil fsinfo driveType <X>:`,argv 为**固定
//! 字面量 + 单字母盘符参数**，无 shell、无拼接用户输入、无可注入面；fsutil 是
//! Windows 系统自带工具，进程只读查询驱动器类型。若运行环境（审查模式 / 沙箱）
//! 拒绝该调用，[`probe`] 退化为「unknown(4) + degradations 注明探测不可用」——
//! **探测失败绝不失败**，最坏结果只是降级到 crypto_only 并如实记录。

use std::path::Path;

/// 介质类别：SSD（含 NVMe）。
pub const MEDIA_SSD: u8 = 1;
/// 介质类别：机械硬盘。
pub const MEDIA_HDD: u8 = 2;
/// 介质类别：网络盘（SMB / 映射网络驱动器）。
pub const MEDIA_NETWORK: u8 = 3;
/// 介质类别：不可判定（探测失败 / 未知类型）。
pub const MEDIA_UNKNOWN: u8 = 4;

/// 擦除等级：仅加密销毁（FSKey 销毁后密文不可恢复，零覆写只是尽力而为）。
pub const CLASS_CRYPTO_ONLY: u8 = 1;
/// 擦除等级：单次零覆写（HDD 上可信；SSD 上因磨损均衡**不可信**）。
pub const CLASS_ZERO_OVERWRITE: u8 = 2;
/// 擦除等级：平台安全删除（TRIM / FSCTL_FILE_LEVEL_TRIM）。
pub const CLASS_PLATFORM_SECURE: u8 = 3;

/// method_bits：单次零覆写已执行。
pub const METHOD_BIT_ZERO_OVERWRITE: u32 = 1 << 0;
/// method_bits：多次覆写已执行。
pub const METHOD_BIT_MULTI_OVERWRITE: u32 = 1 << 1;
/// method_bits：平台安全删除（TRIM/UNMAP）已执行。
pub const METHOD_BIT_TRIM: u32 = 1 << 2;

/// 擦除分级报告：探测结果 + 实际生效的方法位 + 逐条降级说明。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EraseClassReport {
    /// 介质类别（`MEDIA_*` 常量）。
    pub media_kind: u8,
    /// 擦除等级（`CLASS_*` 常量）：该介质上**可信**的最高擦除手段。
    pub erase_class: u8,
    /// 实际已生效的方法位图（`METHOD_BIT_*`）。0 = 尚未执行任何物理擦除手段。
    pub method_bits: u32,
    /// 降级说明：每一条都是对外如实陈述（不得声称的能力写在这里，V2.0 原则 7）。
    pub degradations: Vec<String>,
}

/// 对外「安全擦除」声称的**唯一判据**（文案硬约束）：
/// 仅当等级为 platform_secure（3）且 method_bits 含 bit2（TRIM 实际已执行）时才允许。
///
/// 当前恒为 false：Windows 的 FSCTL_FILE_LEVEL_TRIM 属 unsafe FFI，本任务不实现
/// 实际 TRIM，`probe` 产出的报告 method_bits 恒为 0——因此 UI **不得**声称
/// 「已安全擦除」，只能如实陈述加密销毁 + 降级说明。
pub const SECURE_ERASE_CLAIM_ALLOWED: fn(&EraseClassReport) -> bool = |r: &EraseClassReport| {
    r.erase_class == CLASS_PLATFORM_SECURE && (r.method_bits & METHOD_BIT_TRIM) != 0
};

/// 介质探测 + 擦除等级分类。
///
/// Windows 启发式（纯探测，不注入、不落盘）：解析路径盘符 →
/// `fsutil fsinfo driveType <盘符>:`。安全论证见模块头注释（固定 argv 字面量、
/// 无 shell）；Fixed → 按 SSD 分类（现代 Windows 固定盘绝大多数为 SSD/NVMe，
/// 且 HDD 上的 zero_overwrite 兜底仍然可用，保守方向是把 HDD 误判为 SSD——
/// 只会多降级、不会多声称）；Network/Remote → 网络盘；其余 / 任何一步失败 →
/// unknown + crypto_only + degradations，**探测失败绝不失败**。
pub fn probe(path: &Path) -> EraseClassReport {
    match probe_media_kind(path) {
        Some(MEDIA_SSD) => EraseClassReport {
            media_kind: MEDIA_SSD,
            erase_class: CLASS_PLATFORM_SECURE,
            // TRIM（FSCTL_FILE_LEVEL_TRIM）属 unsafe FFI，本任务不实现实际调用。
            // 等级允许 platform_secure，但方法位不置位 → 不得对外声称安全擦除。
            method_bits: 0,
            degradations: vec![
                "SSD 检测到平台安全删除（TRIM）未接线：当前仅加密销毁（FSKey 销毁），零覆写在 SSD 上不可信".to_string(),
                "平台安全删除待外壳/驱动层接线（FSCTL_FILE_LEVEL_TRIM），method_bits 暂为 0".to_string(),
            ],
        },
        Some(MEDIA_HDD) => EraseClassReport {
            media_kind: MEDIA_HDD,
            erase_class: CLASS_ZERO_OVERWRITE,
            // 等级允许零覆写，但 probe 只做分类不执行擦除；实际置位发生在
            // 调用方执行 zero_overwrite_file 之后。
            method_bits: 0,
            degradations: vec![],
        },
        Some(MEDIA_NETWORK) => EraseClassReport {
            media_kind: MEDIA_NETWORK,
            erase_class: CLASS_CRYPTO_ONLY,
            method_bits: 0,
            degradations: vec![
                "网络盘：远端存储行为不可控，零覆写与 TRIM 均不可信，仅加密销毁".to_string(),
            ],
        },
        _ => EraseClassReport {
            media_kind: MEDIA_UNKNOWN,
            erase_class: CLASS_CRYPTO_ONLY,
            method_bits: 0,
            degradations: vec![
                "介质探测不可用（fsutil 查询失败或被环境拦截）：按最保守策略仅加密销毁".to_string(),
            ],
        },
    }
}

/// 盘符探测：返回 `Some(media_kind)` 或 `None`（任何失败都归入 None，不 panic）。
fn probe_media_kind(path: &Path) -> Option<u8> {
    let drive = drive_letter(path)?;
    // 安全论证（模块头注释详述）：cmd/args 均为固定字面量拼盘符，无 shell 解释层。
    let out = std::process::Command::new("fsutil")
        .arg("fsinfo")
        .arg("driveType")
        .arg(format!("{drive}:"))
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    classify_fsutil_output(&out.stdout)
}

/// 从路径解析出单个盘符字母（如 `D:\a\b` → `D`）。非盘符路径返回 None。
fn drive_letter(path: &Path) -> Option<char> {
    let first = path.components().next()?;
    let std::path::Component::Prefix(prefix) = first else {
        return None;
    };
    let kind = prefix.kind();
    let std::path::Prefix::Disk(byte) = kind else {
        return None;
    };
    byte.is_ascii_alphabetic()
        .then(|| (byte as char).to_ascii_uppercase())
}

/// 解析 `fsutil fsinfo driveType` 输出。
///
/// fsutil 输出使用 **OEM 代码页**（中文 Windows 为 GBK/cp936，非 UTF-8），且文案
/// 随系统语言变化，因此对**原始字节**做关键词匹配，覆盖英文与中文两种典型输出：
/// 英文 `Fixed Drive` / `Remote Drive`；GBK 下「固定驱动器」= `B9CC C7FD B6AF C6F7`、
/// 「网络驱动器」= `CDF8 C2E7 C7FD B6AF C6F7`。其余文案（可移动 / CD-ROM / 未知语言）
/// 一律归入 unknown——匹配不上就走退化分支，绝不误判为可声称的介质。
fn classify_fsutil_output(stdout: &[u8]) -> Option<u8> {
    // GBK 字节序列：中文 Windows 的 fsutil 文案
    const GBK_FIXED: &[u8] = &[0xB9, 0xCC, 0xB6, 0xA8, 0xC7, 0xFD, 0xB6, 0xAF, 0xC6, 0xF7]; // 固定驱动器（GBK）
    const GBK_NETWORK: &[u8] = &[0xCD, 0xF8, 0xC2, 0xE7, 0xC7, 0xFD, 0xB6, 0xAF, 0xC6, 0xF7]; // 网络驱动器
    let lower = stdout.to_ascii_lowercase();
    if lower.windows(GBK_NETWORK.len()).any(|w| w == GBK_NETWORK)
        || contains_ascii(&lower, b"remote")
        || contains_ascii(&lower, b"network")
    {
        Some(MEDIA_NETWORK)
    } else if lower.windows(GBK_FIXED.len()).any(|w| w == GBK_FIXED)
        || contains_ascii(&lower, b"fixed")
    {
        // 固定盘按 SSD 分类：保守方向（见 probe 注释），HDD 被误判只会多降级。
        Some(MEDIA_SSD)
    } else {
        // removable / cd-rom / ramdisk / 未识别语言的文案 → 不可判定
        Some(MEDIA_UNKNOWN)
    }
}

/// 在 ASCII 小写化后的缓冲中查找 ASCII 子串（`to_ascii_lowercase` 只转 ASCII，
/// 直接对原字节做 windows 匹配即可）。
fn contains_ascii(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// 单次零覆写：全文件覆写 0 后 `sync_all` 落盘，返回写入字节数。
///
/// 语义上**复用**（模块提供但默认不由 `probe` 调用，由上层在 HDD 报告下显式调用）：
/// 文件长度保持不变（截断写回，不截短也不加长）。注意：此手段只在 HDD 上可信，
/// SSD / 网络盘上调用方不应据此对外声称物理擦除（`SECURE_ERASE_CLAIM_ALLOWED`
/// 对此恒 false）。
pub fn zero_overwrite_file(path: &Path) -> Result<u64, &'static str> {
    use std::io::{Seek, SeekFrom, Write};
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|_| "zero_overwrite: open failed")?;
    let len = f
        .metadata()
        .map_err(|_| "zero_overwrite: metadata failed")?
        .len();
    f.seek(SeekFrom::Start(0))
        .map_err(|_| "zero_overwrite: seek failed")?;
    let zeros = [0u8; 64 * 1024];
    let mut remaining = len;
    while remaining > 0 {
        let n = remaining.min(zeros.len() as u64) as usize;
        f.write_all(&zeros[..n])
            .map_err(|_| "zero_overwrite: write failed")?;
        remaining -= n as u64;
    }
    f.sync_all().map_err(|_| "zero_overwrite: sync failed")?;
    Ok(len)
}

/// `VSER v1` 擦除元数据（32 B 明文，VSEF v3 尾部可选区，flags.bit0）：
///
/// ```text
/// magic "VSER"(4) | ver u16=1 | erase_class u8 | media_kind u8 |
/// erase_ts_ms u64 | fs_key_destroyed_ms u64 | method_bits u32 | crc32 u32
/// ```
///
/// crc32 覆盖字节 0..28。**选型说明**：不重新实现 CRC，也不做 SHA 截断——直接
/// 复用 `vault-store` 已公开的 IEEE CRC-32（多项式 0xEDB88320，`segment::crc32`），
/// 与容器/段头的校验同一实现，避免同仓库两份同多项式代码漂移。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VsrsEraseMeta {
    /// 擦除等级（`CLASS_*`）。
    pub erase_class: u8,
    /// 介质类别（`MEDIA_*`）。
    pub media_kind: u8,
    /// 擦除执行时间（Unix 毫秒）。
    pub erase_ts_ms: u64,
    /// FSKey 销毁时间（Unix 毫秒；0 = 未记录）。
    pub fs_key_destroyed_ms: u64,
    /// 实际生效的方法位图（`METHOD_BIT_*`）。
    pub method_bits: u32,
}

pub const VSRS_LEN: usize = 32;
const VSRS_MAGIC: &[u8; 4] = b"VSER";
const VSRS_VER: u16 = 1;

/// 编码为 32 B `VSER` 明文结构。
pub fn encode_vsrs(meta: &VsrsEraseMeta) -> [u8; VSRS_LEN] {
    let mut b = [0u8; VSRS_LEN];
    b[0..4].copy_from_slice(VSRS_MAGIC);
    b[4..6].copy_from_slice(&VSRS_VER.to_le_bytes());
    b[6] = meta.erase_class;
    b[7] = meta.media_kind;
    b[8..16].copy_from_slice(&meta.erase_ts_ms.to_le_bytes());
    b[16..24].copy_from_slice(&meta.fs_key_destroyed_ms.to_le_bytes());
    b[24..28].copy_from_slice(&meta.method_bits.to_le_bytes());
    let crc = vault_store::segment::crc32(&b[0..28]);
    b[28..32].copy_from_slice(&crc.to_le_bytes());
    b
}

/// 解码 32 B `VSER` 明文结构；magic / 版本 / CRC 任一不符即拒绝（码 6 语义）。
pub fn decode_vsrs(buf: &[u8]) -> Result<VsrsEraseMeta, &'static str> {
    if buf.len() != VSRS_LEN {
        return Err("VSER: bad length");
    }
    if &buf[0..4] != VSRS_MAGIC {
        return Err("VSER: bad magic");
    }
    let ver = u16::from_le_bytes([buf[4], buf[5]]);
    if ver != VSRS_VER {
        return Err("VSER: unsupported version");
    }
    let stored_crc = u32::from_le_bytes([buf[28], buf[29], buf[30], buf[31]]);
    if vault_store::segment::crc32(&buf[0..28]) != stored_crc {
        return Err("VSER: crc32 mismatch");
    }
    Ok(VsrsEraseMeta {
        erase_class: buf[6],
        media_kind: buf[7],
        erase_ts_ms: u64::from_le_bytes(buf[8..16].try_into().map_err(|_| "VSER: bad bytes")?),
        fs_key_destroyed_ms: u64::from_le_bytes(
            buf[16..24].try_into().map_err(|_| "VSER: bad bytes")?,
        ),
        method_bits: u32::from_le_bytes(buf[24..28].try_into().map_err(|_| "VSER: bad bytes")?),
    })
}

/// 审计字符串辅助：输出含 `eraseClass` / `mediaKind` 的 JSON 风格串
/// （供 FFI 层的 `audit_detail` 字段拼用；本模块不直接写审计）。
pub fn audit_detail(report: &EraseClassReport) -> String {
    let mut s = format!(
        "{{\"schema\":1,\"eraseClass\":{},\"mediaKind\":{},\"methodBits\":{},\"secureEraseClaim\":{},\"degradations\":[",
        report.erase_class,
        report.media_kind,
        report.method_bits,
        SECURE_ERASE_CLAIM_ALLOWED(report)
    );
    for (i, d) in report.degradations.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        // JSON 字符串转义（降级文案是引擎内常量，但按防御式写法转义引号与反斜杠）
        for c in d.chars() {
            match c {
                '"' => s.push_str("\\\""),
                '\\' => s.push_str("\\\\"),
                _ => s.push(c),
            }
        }
    }
    s.push_str("]}");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 证据（P7-5）：SSD 判定下等级允许 platform_secure，但 method_bits 无 bit2
    /// （TRIM 未接线）→ 不得对外声称「安全擦除」。锁定 [`SECURE_ERASE_CLAIM_ALLOWED`] 语义。
    #[test]
    fn ssd_reports_crypto_only_until_trim_wired() {
        let report = probe(Path::new("C:\\Windows\\Temp\\x.vse"));
        assert_eq!(
            report.erase_class, CLASS_PLATFORM_SECURE,
            "SSD 等级应为 platform_secure"
        );
        assert_eq!(
            report.method_bits & METHOD_BIT_TRIM,
            0,
            "TRIM 未接线，bit2 不得置位"
        );
        assert!(
            !report.degradations.is_empty(),
            "SSD 报告必须带降级说明（平台安全删除待接线）"
        );
        assert!(
            !SECURE_ERASE_CLAIM_ALLOWED(&report),
            "TRIM 未接线前不得声称安全擦除（当前恒 false）"
        );
    }

    /// 证据（P7-5）：不可判定介质永远不得声称安全擦除，且必须带探测降级说明。
    #[test]
    fn unknown_medium_never_claims_secure() {
        for out in [
            classify_fsutil_output(b"No Root Dir"),
            classify_fsutil_output(b"Some Future Unknown Type"),
            None, // 模拟 fsutil 调用失败（被环境拦截 / 不存在）
        ] {
            let report = match out {
                Some(kind) => {
                    let mut r = probe(Path::new("C:\\nonexistent-dir-xyz\\f.bin"));
                    // 用构造报告模拟该 kind 的退化分支（probe 内部一致）
                    r.media_kind = kind;
                    r.erase_class = if kind == MEDIA_SSD {
                        CLASS_PLATFORM_SECURE
                    } else {
                        CLASS_CRYPTO_ONLY
                    };
                    r
                }
                None => probe(Path::new("\\\\unc\\share\\f.bin")),
            };
            assert!(
                !SECURE_ERASE_CLAIM_ALLOWED(&report),
                "未知介质不得声称安全擦除"
            );
            assert!(!report.degradations.is_empty(), "不可判定必须如实降级说明");
        }
        // 网络盘直接走探测解析：英文与 GBK 文案都能识别
        assert_eq!(
            classify_fsutil_output(b"Z: - Remote Drive"),
            Some(MEDIA_NETWORK)
        );
        assert_eq!(
            classify_fsutil_output(&[0xCD, 0xF8, 0xC2, 0xE7, 0xC7, 0xFD, 0xB6, 0xAF, 0xC6, 0xF7]),
            Some(MEDIA_NETWORK),
            "GBK「网络驱动器」应识别为网络盘"
        );
    }

    /// 证据（P7-5）：单次零覆写往返——覆写后内容全零、字节数不变、返回写入字节数。
    #[test]
    fn zero_overwrite_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f.bin");
        std::fs::write(&p, vec![0xABu8; 150_000]).unwrap();
        let written = zero_overwrite_file(&p).unwrap();
        assert_eq!(written, 150_000, "返回值应为文件字节数");
        let after = std::fs::read(&p).unwrap();
        assert_eq!(after.len(), 150_000, "覆写后长度不变");
        assert!(after.iter().all(|&b| b == 0), "覆写后内容应全零");
        assert!(zero_overwrite_file(&dir.path().join("missing.bin")).is_err());
    }

    /// 证据（P7-5）：VSER 编解码往返 + CRC 篡改检出。
    #[test]
    fn vser_roundtrip() {
        let meta = VsrsEraseMeta {
            erase_class: CLASS_CRYPTO_ONLY,
            media_kind: MEDIA_SSD,
            erase_ts_ms: 1_760_000_000_123,
            fs_key_destroyed_ms: 1_760_000_000_456,
            method_bits: METHOD_BIT_ZERO_OVERWRITE,
        };
        let enc = encode_vsrs(&meta);
        assert_eq!(enc.len(), VSRS_LEN);
        assert_eq!(&enc[0..4], b"VSER");
        assert_eq!(decode_vsrs(&enc).unwrap(), meta);

        // CRC 篡改检出：翻转方法位域（0..28 覆盖范围内）任一字节
        let mut tampered = enc;
        tampered[24] ^= 0x01;
        assert!(decode_vsrs(&tampered).is_err(), "CRC 必须检出篡改");

        // magic / 版本 / 长度校验
        let mut bad_magic = enc;
        bad_magic[0] = b'X';
        assert!(decode_vsrs(&bad_magic).is_err());
        let mut bad_ver = enc;
        bad_ver[5] = 9;
        assert!(decode_vsrs(&bad_ver).is_err());
        assert!(decode_vsrs(&enc[..31]).is_err());
    }

    /// 审计字符串包含关键字段且 claim 恒 false（TRIM 未接线）。
    #[test]
    fn audit_detail_fields() {
        let report = probe(Path::new("C:\\x\\y.vse"));
        let s = audit_detail(&report);
        assert!(s.contains("\"eraseClass\":3"));
        assert!(s.contains("\"mediaKind\":1"));
        assert!(s.contains("\"secureEraseClaim\":false"));
        assert!(s.contains("\"degradations\":["));
        assert_eq!(classify_fsutil_output(b"C: - Fixed Drive"), Some(MEDIA_SSD));
        assert_eq!(
            classify_fsutil_output(&[0xB9, 0xCC, 0xB6, 0xA8, 0xC7, 0xFD, 0xB6, 0xAF, 0xC6, 0xF7]),
            Some(MEDIA_SSD),
            "GBK「固定驱动器」应识别为 SSD 档"
        );
        assert!(drive_letter(Path::new("relative/only/path")).is_none());
    }
}
