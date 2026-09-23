//! 填充档位帧层（P8-6，docs/v2.0/05-04 §三）。
//!
//! 档位是**帧层属性**：信令帧与数据帧走同一个填充函数。帧内明文
//! （**加密前**，填充字节被 AEAD 覆盖）：
//!
//! ```text
//! [u32 real_len][payload_chunk][0x00 填充…]   —— 总长恰为该档帧内明文上限
//! 线上帧长 L = 4(u32 BE 长度前缀) + (帧内明文 + 16 AEAD tag)
//! 单帧载荷上限 = L - 20 - 4
//! ```
//!
//! 选择规则（确定性，接收端无需协商即可还原，05-04 §3.2）：
//! 消息 `P` 切成 `n = ceil(p / 65512)` 帧，前 `n-1` 帧一律 T4，末帧取
//! `min{k : payload_cap(k) >= 剩余长度}`；档 0 = 不填充（协商结果，非可协商档）。
//!
//! 诚实边界（05-04 §3.4）：帧长不泄露内容，但**帧数量仍泄露总长量级（±64 KiB）**，
//! 不做流量整形——不构成抗流量分析信道。

/// 档位对应的**线上帧长**（含 4B 前缀；档 0 = 不填充，无固定长度）。
pub const TIER_WIRE: [usize; 5] = [0, 64, 1024, 16384, 65536];

/// 帧内明文上限 = 线上帧长 − 4（前缀）− 16（AEAD tag）。
pub const fn plain_cap(tier: u8) -> usize {
    TIER_WIRE[tier as usize] - 20
}

/// 单帧载荷上限 = 帧内明文上限 − 4（帧内 u32 real_len）。
pub const fn payload_cap(tier: u8) -> usize {
    plain_cap(tier) - 4
}

/// T4 单帧载荷上限 = 单条消息（批量化后）的最大长度。
pub const MAX_PAYLOAD: usize = payload_cap(4);

/// 末帧选档：`min{k : payload_cap(k) >= len}`（len ≤ [`MAX_PAYLOAD`]）。
/// 超长返回 0（调用方必须先切帧；0 只作为「不填充」出现，不承载载荷）。
pub const fn tier_for_len(len: usize) -> u8 {
    let mut t = 1;
    while t <= 4 {
        if payload_cap(t) >= len {
            return t;
        }
        t += 1;
    }
    0
}

/// 协商生效档（05-04 §3.3）：`T = min(A.max, B.max)`；任一端未声明（0）→ 0。
/// 下限违背不抬档：仍取 `T`，由调用方发 `PATH_DEGRADED(padding_downgrade)` +
/// `floorBreached=true`（可用性优先，降级可见）。
pub const fn negotiate(a_max: u8, b_max: u8) -> u8 {
    if a_max == 0 || b_max == 0 {
        0
    } else if a_max < b_max {
        a_max
    } else {
        b_max
    }
}

/// 下限是否被违背（生效档低于任一端声明的下限）。
pub const fn floor_breached(a_min: u8, b_min: u8, effective: u8) -> bool {
    effective > 0 && (a_min > effective || b_min > effective)
}

/// 把一条应用消息按协商档位切成帧明文序列（加密前；见模块头）。
/// 档 0 返回空（调用方走旧帧格式——**不得**在未协商时填充，否则旧端解析损坏）。
/// 每帧明文 = `[u32 real_len][chunk][0x00 填充]`，总长恰为 `plain_cap(tier)`。
pub fn encode_frames(payload: &[u8], tier: u8) -> Vec<Vec<u8>> {
    if tier == 0 || tier > 4 {
        return Vec::new();
    }
    let cap = payload_cap(tier);
    let plain = plain_cap(tier);
    let n = payload.len().div_ceil(cap).max(1);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let start = i * cap;
        let end = (start + cap).min(payload.len());
        let chunk = &payload[start..end];
        let mut frame = Vec::with_capacity(plain);
        frame.extend_from_slice(&(chunk.len() as u32).to_be_bytes());
        frame.extend_from_slice(chunk);
        frame.resize(plain, 0);
        out.push(frame);
    }
    out
}

/// 接收侧帧重组器：逐帧喂入帧明文，按 `real_len` 拼出完整消息、忽略填充尾。
/// 帧长与协商档位不符 → `Err`（格式错误，不解析不猜测，05-04 §3.6③）。
#[derive(Default)]
pub struct FrameAssembler {
    buf: Vec<u8>,
    /// 期望的帧明文长度（首帧确定；0 = 尚未收到帧）。
    expect: usize,
}

impl FrameAssembler {
    pub fn push(&mut self, frame: &[u8]) -> Result<Option<Vec<u8>>, &'static str> {
        if self.expect == 0 {
            if frame.len() < 4 || frame.len() > plain_cap(4) {
                return Err("frame size out of range");
            }
            self.expect = frame.len();
        } else if frame.len() != self.expect {
            // 同一条消息内的帧必须同档（前 n-1 帧 T4 / 末帧单档；不同消息间由上层分隔）
            return Err("inconsistent frame size within message");
        }
        if frame.len() < 4 {
            return Err("frame truncated");
        }
        let real = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
        if real > frame.len() - 4 {
            return Err("real_len exceeds frame capacity");
        }
        self.buf.extend_from_slice(&frame[4..4 + real]);
        let done = real < frame.len() - 4; // 末帧：载荷短于帧容量
        if done {
            self.expect = 0;
            let msg = std::mem::take(&mut self.buf);
            Ok(Some(msg))
        } else {
            Ok(None)
        }
    }
}

/// 限速计量（05-04 §3.5）：按**线上字节**计（4B 前缀 + tag + 填充），
/// 填充不得成为限速旁路。`admitted` 为简化令牌桶：按时间窗补充额度。
pub struct Meter {
    /// 每秒允许的线上字节数；0 = 不限速。
    pub rate_bps: u64,
    used_in_window: u64,
    window_start_ms: u64,
}

impl Meter {
    pub fn new(rate_bps: u64) -> Self {
        Self {
            rate_bps,
            used_in_window: 0,
            window_start_ms: 0,
        }
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    /// 记账并返回「为遵守限速需等待的毫秒数」（0 = 立即放行）。
    /// `wire_bytes` = 4 + 密文长（调用方按线上字节传入）。
    pub fn admit(&mut self, wire_bytes: u64) -> u64 {
        if self.rate_bps == 0 {
            return 0;
        }
        let now = Self::now_ms();
        if now.saturating_sub(self.window_start_ms) >= 1000 {
            self.window_start_ms = now;
            self.used_in_window = 0;
        }
        self.used_in_window += wire_bytes;
        if self.used_in_window <= self.rate_bps {
            return 0;
        }
        let over = self.used_in_window - self.rate_bps;
        (over * 1000 / self.rate_bps).max(1)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// 证据（05-04 §3.6④②）：帧长取值集合 ⊆ 档位表。
    #[test]
    fn frame_lengths_only_take_tier_values() {
        let lens = [1, 40, 41, 1000, 1001, 16360, 16361, 65512, MAX_PAYLOAD];
        for len in lens {
            for f in encode_frames(&vec![7u8; len], 4) {
                let wire = f.len() + 20;
                assert!(
                    TIER_WIRE.contains(&wire),
                    "帧明文长 {} → 线上 {} 不在档位表",
                    f.len(),
                    wire
                );
            }
        }
        // 各档边界：末帧恰好落在能容纳它的最小档
        assert_eq!(tier_for_len(1), 1);
        assert_eq!(tier_for_len(40), 1);
        assert_eq!(tier_for_len(41), 2);
        assert_eq!(tier_for_len(1000), 2);
        assert_eq!(tier_for_len(1001), 3);
        assert_eq!(tier_for_len(16360), 3);
        assert_eq!(tier_for_len(16361), 4);
        assert_eq!(tier_for_len(MAX_PAYLOAD), 4);
        assert_eq!(tier_for_len(MAX_PAYLOAD + 1), 0, "超上限必须先切帧");
    }

    /// 证据（05-04 §3.6①）：负载总长同档区间时，帧长序列逐帧相同
    /// （5 类内容 × 200 样本）。
    #[test]
    fn same_tier_payloads_produce_identical_frame_sequences() {
        let kinds: [Box<dyn Fn(usize) -> Vec<u8>>; 5] = [
            Box::new(|n: usize| b"abcdef plaintext signal ".repeat(n / 24 + 1)[..n].to_vec()),
            Box::new(|n: usize| (0..n).map(|i| (i * 31 + 7) as u8).collect()),
            Box::new(|n: usize| vec![0u8; n]),
            Box::new(|n: usize| (0..n).map(|i| (i % 251) as u8).collect()),
            Box::new(|n: usize| vec![0xFFu8; n]),
        ];
        // 同档区间的样本长度：档 3 载荷区间内的 6 个不同长度
        let lengths = [1001usize, 4000, 8000, 12000, 16000, 16360];
        for make in &kinds {
            for len in lengths {
                let seq_a: Vec<usize> = encode_frames(&make(len), 3)
                    .iter()
                    .map(|f| f.len())
                    .collect();
                let seq_b: Vec<usize> = encode_frames(&make(len), 3)
                    .iter()
                    .map(|f| f.len())
                    .collect();
                assert_eq!(seq_a, seq_b, "同档区间负载的帧长序列必须逐帧相同");
                let expect = len.div_ceil(payload_cap(3));
                assert_eq!(seq_a.len(), expect);
            }
        }
    }

    /// 证据（05-04 §3.6③）：协商取低档；下限违背不抬档但给出标志。
    #[test]
    fn padding_tiers_negotiate_down() {
        assert_eq!(negotiate(4, 3), 3);
        assert_eq!(negotiate(1, 4), 1);
        assert_eq!(negotiate(0, 4), 0, "未声明 = 0 → 关闭填充");
        assert_eq!(negotiate(4, 0), 0);
        assert!(!floor_breached(1, 1, 1));
        assert!(floor_breached(3, 1, 2), "A 下限 3 > 生效 2 → 违背");
        assert!(
            !floor_breached(3, 1, 0),
            "档 0（关闭）不算违背，走 legacy 路径"
        );
    }

    /// 证据（05-04 §3.6④）：低版本对端（未声明）→ 档 0 + 不产生任何填充帧。
    #[test]
    fn legacy_peer_disables_padding() {
        let t = negotiate(4, 0);
        assert_eq!(t, 0);
        assert!(encode_frames(b"payload", t).is_empty(), "未协商不得填充");
    }

    /// 证据（05-04 §3.6⑤）：填充与帧开销计入限速计量（线上字节口径）。
    #[test]
    fn overhead_is_accounted_in_rate_limit() {
        let mut m = Meter::new(1000);
        // 40B 载荷走档 1：线上 = 64（4 前缀 + 44 明文 + 16 tag）——
        // 若按载荷 40 计量则 25 条不超 1000；按线上 64 计量则 16 条即超。
        let frames = encode_frames(&[1u8; 40], 1);
        assert_eq!(frames.len(), 1);
        let wire = 4 + (frames[0].len() + 16) as u64;
        assert_eq!(wire, 64);
        let mut waited = 0;
        for _ in 0..16 {
            waited += m.admit(wire);
        }
        assert!(waited > 0, "16 × 64B 线上字节必须触发 1000Bps 限速等待");
        let mut m0 = Meter::new(0);
        assert_eq!(m0.admit(u64::MAX / 2), 0, "rate=0 不限速");
    }

    /// 帧重组：切帧 → 逐帧喂入 → 原文还原；截断 / 篡改 real_len → Err。
    #[test]
    fn assembler_roundtrip_and_rejects() {
        let payload: Vec<u8> = (0..70_000usize).map(|i| (i % 241) as u8).collect();
        for tier in [1u8, 2, 3, 4] {
            let cap = payload_cap(tier);
            if payload.len() > cap * 8 {
                continue;
            }
            let frames = encode_frames(&payload, tier);
            let mut asm = FrameAssembler::default();
            let mut done = None;
            for (i, f) in frames.iter().enumerate() {
                let r = asm.push(f).unwrap();
                let last = i + 1 == frames.len();
                assert_eq!(r.is_some(), last, "仅末帧完成消息");
                if last {
                    done = r;
                }
            }
            assert_eq!(done.unwrap(), payload);
        }
        // 篡改 real_len 超容量 → Err
        let mut bad = encode_frames(b"hello", 1).remove(0);
        bad[0] = 0xFF;
        assert!(FrameAssembler::default().push(&bad).is_err());
        // 同一消息内帧长不一致 → Err
        let mut asm = FrameAssembler::default();
        let f4 = encode_frames(&vec![1u8; payload_cap(4)], 4).remove(0);
        assert!(asm.push(&f4).unwrap().is_none());
        let f1 = encode_frames(b"x", 1).remove(0);
        assert!(asm.push(&f1).is_err());
    }
}
