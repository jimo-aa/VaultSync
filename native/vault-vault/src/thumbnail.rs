//! 缩略图（P7-9，docs/v2.0/05-02 §4.6）——**本任务的诚实边界**：
//!
//! - 设计要求「解密与缩放全部在引擎内完成，Dart 侧只拿已解码 PNG 字节」。真正解码
//!   PNG/JPEG 需要图像编解码依赖（如 `image` crate），按 P6-7 的受审依赖纪律**不在本任务
//!   引入**——解码缩放归 P9-4/P9-9（依赖审计与收口）。本模块因此只落地三件可验证的部分：
//!   ① `detect_image_kind`：png/jpg/webp/gif 魔数判定（FFI 层据此返回 13/6）；
//!   ② `ThumbnailCache`：内存 LRU（128 张 / 32 MB，LOCKED/维护态立即清空，不落盘）；
//!   ③ `generate_png_thumbnail`：**如实返回 Err("thumbnail codec not wired")**，
//!   FFI 层应映射为状态码 13（能力不支持），前端按 `03` §1.2「隐藏」档回落渐变占位。
//! - 引擎**不得**因本占位而对外宣称具备缩略图能力（`docs/v2.0/11` 证据先行原则）：
//!   `CAP_THUMBNAIL`(bit 18) 在真实解码落地前不置位。
#![forbid(unsafe_code)]

use std::collections::{HashMap, VecDeque};
use zeroize::Zeroize;

/// 缓存上限：128 张（docs/v2.0/05-02 §4.6）。
pub const CACHE_MAX_ENTRIES: usize = 128;
/// 缓存字节总量上限：32 MB。
pub const CACHE_MAX_BYTES: usize = 32 * 1024 * 1024;

/// 图像类型魔数判定：Some("png"|"jpg"|"webp"|"gif")；非图像 → None。
pub fn detect_image_kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 8 && bytes[0..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return Some("png");
    }
    if bytes.len() >= 3 && bytes[0..3] == [0xFF, 0xD8, 0xFF] {
        return Some("jpg");
    }
    if bytes.len() >= 12 && bytes[0..4] == *b"RIFF" && bytes[8..12] == *b"WEBP" {
        return Some("webp");
    }
    if bytes.len() >= 6 && (bytes[0..6] == *b"GIF87a" || bytes[0..6] == *b"GIF89a") {
        return Some("gif");
    }
    None
}

/// PNG 缩略图生成占位：真实解码缩放需受审图像依赖，归 P9-4/P9-9（见模块注释）。
/// 调用方（FFI 层）应把此错误如实映射为状态码 13（能力不支持）。
pub fn generate_png_thumbnail(_png_bytes: &[u8], _max_px: u32) -> Result<Vec<u8>, &'static str> {
    Err("thumbnail codec not wired")
}

/// 缩略图内存 LRU 缓存：128 张 / 32 MB 双上限；不落盘、不进剪贴板。
/// `LOCKED`(4) 或转入只读维护态时调用 `clear`（立即清空并零化缓冲）。
pub struct ThumbnailCache {
    max_entries: usize,
    max_bytes: usize,
    bytes: usize,
    /// LRU 序：队尾 = 最近使用。
    order: VecDeque<u64>,
    map: HashMap<u64, Vec<u8>>,
}

impl ThumbnailCache {
    /// 生产口径：`ThumbnailCache::new(CACHE_MAX_ENTRIES, CACHE_MAX_BYTES)`。
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            max_entries,
            max_bytes,
            bytes: 0,
            order: VecDeque::new(),
            map: HashMap::new(),
        }
    }

    /// 命中并把条目提升为最近使用。
    pub fn get(&mut self, file_id: u64) -> Option<Vec<u8>> {
        let hit = self.map.get(&file_id).cloned()?;
        if let Some(pos) = self.order.iter().position(|&id| id == file_id) {
            self.order.remove(pos);
        }
        self.order.push_back(file_id);
        Some(hit)
    }

    /// 写入缓存；超出单条/总量上限返回 false（大图不缓存，不挤出全表）。
    pub fn put(&mut self, file_id: u64, bytes: Vec<u8>) -> bool {
        if bytes.len() > self.max_bytes {
            return false;
        }
        self.remove(file_id);
        loop {
            if self.map.len() < self.max_entries && self.bytes + bytes.len() <= self.max_bytes {
                break;
            }
            let Some(victim) = self.order.pop_front() else {
                break;
            };
            self.drop_entry(victim);
        }
        self.bytes += bytes.len();
        self.order.push_back(file_id);
        self.map.insert(file_id, bytes);
        true
    }

    /// 当前条目数（证据测试用）。
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// 缓存字节数（证据测试用）。
    pub fn byte_len(&self) -> usize {
        self.bytes
    }

    /// LOCKED(4) / 维护态：立即清空并零化全部缓冲（docs/v2.0/05-02 §4.6、03 §七）。
    pub fn clear(&mut self) {
        for (_, mut buf) in self.map.drain() {
            buf.zeroize();
        }
        self.order.clear();
        self.bytes = 0;
    }

    fn remove(&mut self, file_id: u64) {
        if self.map.remove(&file_id).is_some() {
            if let Some(pos) = self.order.iter().position(|&id| id == file_id) {
                self.order.remove(pos);
            }
        }
    }

    fn drop_entry(&mut self, file_id: u64) {
        if let Some(mut buf) = self.map.remove(&file_id) {
            self.bytes -= buf.len();
            buf.zeroize();
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod thumbnail_tests {
    use super::*;

    #[test]
    fn detect_kind() {
        let png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0];
        assert_eq!(detect_image_kind(&png), Some("png"));
        let jpg = vec![0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(detect_image_kind(&jpg), Some("jpg"));
        let mut webp = b"RIFF\x00\x00\x00\x00".to_vec();
        webp.extend_from_slice(b"WEBP");
        assert_eq!(detect_image_kind(&webp), Some("webp"));
        assert_eq!(detect_image_kind(b"GIF89a.."), Some("gif"));
        assert_eq!(detect_image_kind(b"not an image"), None);
        assert_eq!(detect_image_kind(&[]), None);
    }

    #[test]
    fn generate_honestly_unwired() {
        let png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(
            generate_png_thumbnail(&png, 256).unwrap_err(),
            "thumbnail codec not wired"
        );
    }

    #[test]
    fn lru_eviction_and_clear() {
        let mut c = ThumbnailCache::new(2, 1024);
        assert!(c.put(1, vec![0xAA; 10]));
        assert!(c.put(2, vec![0xBB; 10]));
        // 命中 1 → 最近使用顺序变为 2,1
        assert_eq!(c.get(1).expect("hit 1").len(), 10);
        assert!(c.put(3, vec![0xCC; 10]));
        // 容量 2：最久未用的 2 被逐出
        assert_eq!(c.len(), 2);
        assert!(c.get(2).is_none());
        assert!(c.get(1).is_some());
        assert!(c.get(3).is_some());
        // 字节总量上限：单条超限拒绝，不缓存
        assert!(!c.put(4, vec![0xDD; 2048]));
        assert_eq!(c.len(), 2);
        // LOCKED / 维护态：立即清空
        c.clear();
        assert!(c.is_empty());
        assert_eq!(c.byte_len(), 0);
        assert!(c.get(1).is_none());
    }
}
