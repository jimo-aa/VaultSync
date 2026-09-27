//! 跨图分片与乱序重组（P8-10，docs/v2.0/05-05 §3.2–3.5）。
//!
//! 只做**字节搬运与结构校验**：分片帧的编解码、CRC32、乱序落位、缺片/重复判定、
//! 整文件 SHA-256 终验。加密仍在 `vault-core` 侧先行（隐写载荷本身即密文）。
//!
//! `payload_ver = 2` 布局（docs/05-05 §3.2）：
//!
//! ```text
//! [ver u8=0x02][flags u8][shard_index u16 BE][shard_total u16 BE]
//! [file_len u32 BE][shard_crc32 u32 BE][file_hdr 仅首片][shard_bytes]
//! file_hdr = [name_len u16 BE][name UTF-8][SHA-256(整文件明文) 32B]
//! ```
//!
//! `flags`：bit0 首片 / bit1 单图模式 / bit2 位分散启用 / bit3 位序置乱启用（默认关闭）。
//! **位分散的规则由（载荷长度，图尺寸）确定性导出**，双端无需协商——`flags` 中的
//! 该位只作诊断（`docs/05-05` §3.6 的 `pos(i)=i×s+s/2`）。
#![deny(unsafe_code)]

use vault_crypto::{hash_sha256_bytes, Sha256};

/// V2 载荷版本字节（V1 布局首字节为 0x00——`[u32 name_len]` 的高字节）。
pub const PAYLOAD_VER_V2: u8 = 0x02;
pub const FLAG_FIRST: u8 = 0b0000_0001;
pub const FLAG_SINGLE: u8 = 0b0000_0010;
pub const FLAG_DISPERSED: u8 = 0b0000_0100;
pub const FLAG_SHUFFLED: u8 = 0b0000_1000;
/// 分片头固定长度（ver + flags + idx + total + file_len + crc）。
pub const SHARD_HEADER_LEN: usize = 1 + 1 + 2 + 2 + 4 + 4;
/// 分片总数上限（防恶意图集；docs/05-05 §3.5）。
pub const MAX_SHARDS: u16 = 4096;
/// 单文件上限默认值（4 GiB；`opts_json.maxBytes` 可覆盖）。
pub const DEFAULT_MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;

const ERR_FORMAT: &str = "unsupported png format";
const ERR_UNKNOWN_VER: &str = "unknown payload version";
const ERR_CRC: &str = "shard crc mismatch";
const ERR_INCONSISTENT: &str = "shard metadata inconsistent";
const ERR_HASH: &str = "reassembled file hash mismatch";
const ERR_TOO_LARGE: &str = "payload length out of range";

/// 首片携带的文件头。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileHeader {
    pub name: String,
    /// 整文件明文 SHA-256（终验用）。
    pub sha256: [u8; 32],
}

/// 解析后的分片（乱序重组输入）。
#[derive(Clone, Debug)]
pub struct Shard {
    pub flags: u8,
    pub index: u16,
    pub total: u16,
    pub file_len: u32,
    pub file_hdr: Option<FileHeader>,
    pub bytes: Vec<u8>,
}

/// CRC32（IEEE 802.3，反射多项式 0xEDB88320）——分片级完整性。
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// 编码一个分片（`file_hdr` 仅首片传入）。
pub fn encode_shard(
    index: u16,
    total: u16,
    file_len: u32,
    file_hdr: Option<&FileHeader>,
    bytes: &[u8],
    dispersed: bool,
) -> Vec<u8> {
    let mut flags = 0u8;
    if index == 0 {
        flags |= FLAG_FIRST;
    }
    if total == 1 {
        flags |= FLAG_SINGLE;
    }
    if dispersed {
        flags |= FLAG_DISPERSED;
    }
    let mut out = Vec::with_capacity(SHARD_HEADER_LEN + bytes.len());
    out.push(PAYLOAD_VER_V2);
    out.push(flags);
    out.extend_from_slice(&index.to_be_bytes());
    out.extend_from_slice(&total.to_be_bytes());
    out.extend_from_slice(&file_len.to_be_bytes());
    out.extend_from_slice(&crc32(bytes).to_be_bytes());
    if let Some(h) = file_hdr {
        let name = h.name.as_bytes();
        let n = u16::try_from(name.len()).unwrap_or(u16::MAX);
        out.extend_from_slice(&n.to_be_bytes());
        out.extend_from_slice(&name[..n as usize]);
        out.extend_from_slice(&h.sha256);
    }
    out.extend_from_slice(bytes);
    out
}

/// 解析分片帧。**先校验长度字段再分配**（防恶意图集）；
/// `max_bytes` = 单文件上限（`file_len` 超过即拒）。
pub fn parse_shard(raw: &[u8], max_bytes: u64) -> Result<Shard, &'static str> {
    if raw.len() < SHARD_HEADER_LEN {
        return Err(ERR_FORMAT);
    }
    if raw[0] != PAYLOAD_VER_V2 {
        return Err(ERR_UNKNOWN_VER);
    }
    let flags = raw[1];
    let index = u16::from_be_bytes([raw[2], raw[3]]);
    let total = u16::from_be_bytes([raw[4], raw[5]]);
    let file_len = u32::from_be_bytes([raw[6], raw[7], raw[8], raw[9]]);
    let crc = u32::from_be_bytes([raw[10], raw[11], raw[12], raw[13]]);
    if total == 0 || total > MAX_SHARDS || index >= total {
        return Err(ERR_INCONSISTENT);
    }
    if u64::from(file_len) > max_bytes {
        return Err(ERR_TOO_LARGE);
    }
    if (index == 0) != (flags & FLAG_FIRST != 0) {
        return Err(ERR_INCONSISTENT);
    }
    let mut offset = SHARD_HEADER_LEN;
    let mut file_hdr = None;
    if index == 0 {
        if raw.len() < offset + 2 {
            return Err(ERR_FORMAT);
        }
        let name_len = u16::from_be_bytes([raw[offset], raw[offset + 1]]) as usize;
        offset += 2;
        if raw.len() < offset + name_len + 32 {
            return Err(ERR_FORMAT);
        }
        let name =
            String::from_utf8(raw[offset..offset + name_len].to_vec()).map_err(|_| ERR_FORMAT)?;
        offset += name_len;
        let mut sha256 = [0u8; 32];
        sha256.copy_from_slice(&raw[offset..offset + 32]);
        offset += 32;
        file_hdr = Some(FileHeader { name, sha256 });
    }
    let bytes = raw[offset..].to_vec();
    if crc32(&bytes) != crc {
        return Err(ERR_CRC);
    }
    Ok(Shard {
        flags,
        index,
        total,
        file_len,
        file_hdr,
        bytes,
    })
}

/// 乱序重组：按 `shard_index` 落位；重复索引**拒绝**（不做「后到者胜」）；
/// 缺片 → `Err(Missing(...))`；终验不一致 → `Err(Hash)`（调用方须删除已写出的目标文件）。
pub fn reassemble(shards: &[Shard]) -> Result<(FileHeader, Vec<u8>), ReassembleError> {
    let Some(first) = shards.first() else {
        return Err(ReassembleError::Missing(vec![]));
    };
    let total = first.total;
    let file_len = first.file_len;
    let mut slots: Vec<Option<&Shard>> = vec![None; total as usize];
    for s in shards {
        if s.total != total || s.file_len != file_len {
            return Err(ReassembleError::Inconsistent);
        }
        if slots[s.index as usize].is_some() {
            return Err(ReassembleError::Duplicate(s.index));
        }
        slots[s.index as usize] = Some(s);
    }
    let missing: Vec<u16> = slots
        .iter()
        .enumerate()
        .filter(|(_, s)| s.is_none())
        .map(|(i, _)| i as u16)
        .collect();
    if !missing.is_empty() {
        return Err(ReassembleError::Missing(missing));
    }
    let hdr = slots[0]
        .and_then(|s| s.file_hdr.clone())
        .ok_or(ReassembleError::Inconsistent)?;
    let mut out = Vec::with_capacity(file_len as usize);
    for s in slots.into_iter().flatten() {
        out.extend_from_slice(&s.bytes);
    }
    if out.len() != file_len as usize {
        return Err(ReassembleError::Inconsistent);
    }
    // 终验：整文件 SHA-256（不一致 → 调用方删除已写出的目标文件）
    let mut h = Sha256::new();
    h.update(&out);
    if h.finalize_bytes() != hdr.sha256 {
        return Err(ReassembleError::HashMismatch);
    }
    Ok((hdr, out))
}

/// 重组失败原因（FFI 层映射：`6` + 可读诊断；缺片附 `missing[]`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReassembleError {
    /// 同一 `shard_index` 出现两次。
    Duplicate(u16),
    /// 缺片（索引列表）。
    Missing(Vec<u16>),
    /// 各片 `shard_total` / `file_len` 不一致，或缺首片。
    Inconsistent,
    /// 终验哈希不符。
    HashMismatch,
}

impl ReassembleError {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReassembleError::Duplicate(_) => "duplicate shard index",
            ReassembleError::Missing(_) => "missing shards",
            ReassembleError::Inconsistent => ERR_INCONSISTENT,
            ReassembleError::HashMismatch => ERR_HASH,
        }
    }
}

/// 便利：按块大小切分并逐片编码（`shard_total = 1` 时即单图模式）。
pub fn split_and_encode(
    name: &str,
    plaintext: &[u8],
    capacities: &[usize],
    dispersed: bool,
) -> Result<Vec<Vec<u8>>, String> {
    if capacities.is_empty() {
        return Err("no images given".into());
    }
    let total_hdr_overhead = SHARD_HEADER_LEN + 2 + name.len() + 32;
    let total = capacities.len();
    let mut frames = Vec::with_capacity(total);
    let file_hdr = FileHeader {
        name: name.to_string(),
        sha256: hash_sha256_bytes(plaintext),
    };
    let mut offset = 0usize;
    for (i, &cap) in capacities.iter().enumerate() {
        let overhead = if i == 0 {
            total_hdr_overhead
        } else {
            SHARD_HEADER_LEN
        };
        if cap <= overhead {
            return Err(format!("image #{i} capacity too small"));
        }
        let take = (cap - overhead).min(plaintext.len().saturating_sub(offset));
        let is_last = i + 1 == total;
        if is_last && offset + take < plaintext.len() {
            return Err(format!(
                "需要 {} 张图（净容量不足 {} 字节）",
                total,
                plaintext.len() - (offset + take)
            ));
        }
        let chunk = &plaintext[offset..offset + take];
        offset += take;
        frames.push(encode_shard(
            i as u16,
            total as u16,
            plaintext.len() as u32,
            if i == 0 { Some(&file_hdr) } else { None },
            chunk,
            dispersed,
        ));
    }
    if offset < plaintext.len() {
        return Err("capacity insufficient".into());
    }
    Ok(frames)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn hdr(name: &str, data: &[u8]) -> FileHeader {
        FileHeader {
            name: name.to_string(),
            sha256: hash_sha256_bytes(data),
        }
    }

    /// 证据（面板 `stego::multi_image_reassemble_out_of_order`）：乱序到达按索引落位，
    /// 重组结果与原文逐字节一致，且终验通过。
    #[test]
    fn multi_image_reassemble_out_of_order() {
        let data: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        let h = hdr("report.pdf", &data);
        let parts: Vec<&[u8]> = vec![&data[0..4000], &data[4000..7000], &data[7000..]];
        let shards: Vec<Shard> = parts
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let raw = encode_shard(
                    i as u16,
                    3,
                    data.len() as u32,
                    if i == 0 { Some(&h) } else { None },
                    p,
                    true,
                );
                parse_shard(&raw, DEFAULT_MAX_BYTES).unwrap()
            })
            .collect();
        // 乱序：2, 0, 1
        let shuffled = vec![shards[2].clone(), shards[0].clone(), shards[1].clone()];
        let (h2, out) = reassemble(&shuffled).unwrap();
        assert_eq!(out, data);
        assert_eq!(h2, h);
        // 首片 flags 与单图标志
        assert_ne!(shards[0].flags & FLAG_FIRST, 0);
        assert_ne!(shards[0].flags & FLAG_DISPERSED, 0);
        assert_eq!(shards[0].flags & FLAG_SHUFFLED, 0, "位序置乱默认关闭");
    }
    /// 证据（`duplicate_shard_index_rejected` 与 `missing_shard_is_reported`）：
    /// 重复索引拒绝（不做「后到者胜」）；缺片报 `missing[]`。
    #[test]
    fn duplicate_and_missing_are_reported() {
        let data = vec![7u8; 300];
        let h = hdr("a.bin", &data);
        let s0 = parse_shard(
            &encode_shard(0, 2, data.len() as u32, Some(&h), &data[..100], false),
            DEFAULT_MAX_BYTES,
        )
        .unwrap();
        let s1 = parse_shard(
            &encode_shard(1, 2, data.len() as u32, None, &data[100..200], false),
            DEFAULT_MAX_BYTES,
        )
        .unwrap();
        let dup = vec![s0.clone(), s0.clone(), s1.clone()];
        assert_eq!(
            reassemble(&dup),
            Err(ReassembleError::Duplicate(0)),
            "重复索引必须拒绝"
        );
        let missing = vec![s0.clone()];
        assert_eq!(reassemble(&missing), Err(ReassembleError::Missing(vec![1])));
    }

    /// 分片 CRC 篡改 → 解析即拒；终验哈希不符 → `HashMismatch`（调用方删目标文件）。
    #[test]
    fn crc_and_final_hash_are_enforced() {
        let data = vec![9u8; 200];
        let h = hdr("t.bin", &data);
        let mut raw = encode_shard(0, 1, data.len() as u32, Some(&h), &data, false);
        let last = raw.len() - 1;
        raw[last] ^= 0xff;
        assert_eq!(parse_shard(&raw, DEFAULT_MAX_BYTES).unwrap_err(), ERR_CRC);

        // 元数据一致但内容与首片声明的 sha 不符（手工构造）
        let wrong = FileHeader {
            name: "t.bin".into(),
            sha256: [0u8; 32],
        };
        let raw2 = encode_shard(0, 1, data.len() as u32, Some(&wrong), &data, false);
        let s = parse_shard(&raw2, DEFAULT_MAX_BYTES).unwrap();
        assert_eq!(reassemble(&[s]), Err(ReassembleError::HashMismatch));
    }

    /// 防御：未知版本字节 / 超上限 file_len / 元数据不一致 各自被拒（先校验再分配）。
    #[test]
    fn malformed_shards_are_rejected() {
        let data = vec![1u8; 64];
        // 未知版本
        assert_eq!(
            parse_shard(&[0x00, 0, 0, 0, 0, 1, 0, 0, 0, 64, 0, 0, 0, 0], 1024).unwrap_err(),
            ERR_UNKNOWN_VER
        );
        // file_len 超上限
        let h = hdr("x", &data);
        let raw = encode_shard(0, 1, 1_000_000, Some(&h), &data, false);
        assert_eq!(parse_shard(&raw, 1024).unwrap_err(), ERR_TOO_LARGE);
        // total = 0
        let raw0 = encode_shard(0, 0, 10, Some(&h), &data, false);
        assert_eq!(
            parse_shard(&raw0, DEFAULT_MAX_BYTES).unwrap_err(),
            ERR_INCONSISTENT
        );
        // 首片标志与 index 不符
        let raw1 = encode_shard(1, 2, 10, None, &data, false);
        let mut bad = raw1.clone();
        bad[1] |= FLAG_FIRST;
        assert_eq!(
            parse_shard(&bad, DEFAULT_MAX_BYTES).unwrap_err(),
            ERR_INCONSISTENT
        );
    }

    /// 容量簿记：切分结果图数与净容量吻合；容量不足时如实报需要几张图。
    #[test]
    fn split_matches_capacity_budget() {
        let data = vec![3u8; 500];
        // 每图净容量 200（含首片头开销）→ 首片 200-46=154，其余 200-14=186
        let caps = vec![200usize, 200, 200, 200];
        let frames = split_and_encode("f.bin", &data, &caps, true).unwrap();
        assert_eq!(frames.len(), 4);
        // 3 张不够：154 + 186 + 186 = 526 ≥ 500 → 实际只需 3 张
        let frames3 = split_and_encode("f.bin", &data, &caps[..3], true).unwrap();
        assert_eq!(frames3.len(), 3);
        let mut total = 0usize;
        for f in &frames3 {
            total += f.len();
        }
        assert!(total > 500, "分片总长含头开销");
        // 两张不够 → 报错
        assert!(split_and_encode("f.bin", &data, &caps[..2], true).is_err());
    }
}
