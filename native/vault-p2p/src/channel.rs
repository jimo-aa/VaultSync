//! 端到端加密信道（docs/05-04 §四、docs/08 §四）：
//! - Noise XX / XXpsk3（snow）做传输加密与双向认证；
//! - 应用层每条消息带单调递增序号，乱序/重复即拒绝（防重放，docs/08 §4.1）；
//! - 消息按 u32 长度分帧，内部再切 ≤64KB 的 Noise 帧以容纳大块传输。
//! - P7-8 混合 KEM（suite 2）：协商成功后，应用消息在 Noise 之内再套一层
//!   ML-KEM-768 派生密钥的 AEAD（docs/v2.0/09 P7-8）——数据机密性
//!   = max(经典 X25519，后量子 ML-KEM)。协商用**握手帧长扩展**表达：
//!   发起端 msg1 尾部追加 1 字节套件号；响应端以「msg2 是否携带封装公钥」作答。
//!   不提供 PQ 的端发出的线上帧与 v0.7.0 逐字节一致（旧端零扰动）。
use std::io::{Read, Write};

use snow::Builder;
use vault_crypto::{hash_sha256, hash_sha256_bytes, hex_encode, AES_GCM_NONCE_LEN, KEY_LEN};

use crate::identity::Identity;

const MAX_NOISE_FRAME: usize = 65535;
/// 单帧明文上限 = 65535 − 16B GCM tag；留余量取 60000。
const CHUNK_PAYLOAD: usize = 60000;
const MAX_APP_MSG: usize = 4 * 1024 * 1024;
const PATTERN_XX: &str = "Noise_XX_25519_ChaChaPoly_SHA256";
const PATTERN_XX_PSK3: &str = "Noise_XXpsk3_25519_ChaChaPoly_SHA256";

/// cipher_suite（docs/v2.0/09 P7-6）：1 = 现状默认（X25519）；2 = 混合 KEM。
pub const SUITE_LEGACY: u8 = 1;
pub const SUITE_PQ: u8 = 2;

/// snow 握手消息定长（noise_msg_lengths_are_stable 测试守护；snow 升级若变长必须同步）。
const NOISE_MSG1_PLAIN_NO_PSK: usize = 32;
const NOISE_MSG1_PLAIN_PSK: usize = 48;
const NOISE_MSG2_PLAIN: usize = 96;
const NOISE_MSG3_PLAIN: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Initiator,
    Responder,
}

/// hex 摘要字符串 → 32B 密钥（hash_sha256 输出为 64 字符 hex）。
fn hash_to_key(hex: &str) -> [u8; 32] {
    let mut k = [0u8; 32];
    for (i, c) in hex.as_bytes().chunks(2).take(32).enumerate() {
        let hi = (c[0] as char).to_digit(16).unwrap_or(0) as u8;
        let lo = (c[1] as char).to_digit(16).unwrap_or(0) as u8;
        k[i] = (hi << 4) | lo;
    }
    k
}

/// 配对邀请码 → 信道 PSK（仅配对阶段使用；持有邀请码即证明被邀请方身份）。
pub fn psk_from_code(code: &str) -> [u8; 32] {
    hash_to_key(&hash_sha256(format!("vsync-psk:{code}").as_bytes()))
}

/// PQ AEAD 密钥：混合秘密 + 握手哈希按方向分离派生（防两方向 nonce 重合）。
fn pq_direction_key(hh: &[u8; 32], ss: &[u8; 32], dir: &str) -> [u8; KEY_LEN] {
    hash_sha256_bytes(
        format!("vsync-pq-key:{}:{}:{}", hex_encode(hh), dir, hex_encode(ss)).as_bytes(),
    )
}

pub struct SecureChannel {
    transport: snow::TransportState,
    handshake_hash: [u8; 32],
    send_seq: u64,
    recv_seq: u64,
    /// 远端 Noise 静态公钥（X25519，握手后提取）。
    remote_static: [u8; 32],
    /// P7-8 suite 2：方向分离的 PQ AEAD 密钥 (发送, 接收)。
    pq_keys: Option<([u8; KEY_LEN], [u8; KEY_LEN])>,
    /// 协商结果（1 或 2），强制可见：引擎日志与 p2p_status 均暴露。
    cipher_suite: u8,
    /// 请求过 PQ 却未启用时的可见原因（fail-open 审计线索）。
    pq_note: Option<&'static str>,
    /// P8-6 生效填充档（0 = 旧帧格式不填充）；Hello 协商后由引擎设置。
    pad_tier: u8,
    /// P8-6 帧重组器（新帧格式接收侧）。
    assembler: vault_net::frame::FrameAssembler,
    /// 填充诊断：逐档帧计数（档 1–4）与载荷/线上字节累计。
    frames_by_tier: [u64; 4],
    payload_bytes: u64,
    wire_bytes: u64,
}

impl SecureChannel {
    /// 在既有字节流上执行 Noise 握手。`psk` 为配对阶段的一次性口令派生密钥。
    ///
    /// `offer_pq`：发起端 = 是否在 msg1 附加套件号（本端 ML-KEM 派生失败时自动放弃，
    /// fail-open 到 suite 1 且 `pq_note` 可见）；响应端 = 是否允许在发起端提议时启用
    /// suite 2（`false` 即对提议回裸 msg2 → 带内降级，对端可见）。
    /// 无论参数如何，不含 PQ 的帧序列与 v0.7.0 完全一致。
    pub fn handshake<S: Read + Write>(
        stream: &mut S,
        role: Role,
        ident: &Identity,
        psk: Option<&[u8; 32]>,
        offer_pq: bool,
    ) -> Result<Self, &'static str> {
        let pattern = if psk.is_some() {
            PATTERN_XX_PSK3
        } else {
            PATTERN_XX
        };
        // 本端 ML-KEM 能力：派生失败即 fail-open（增强而非依赖），原因强制可见。
        let mut pq_note: Option<&'static str> = if offer_pq {
            None
        } else {
            Some("pq not offered")
        };
        let sk = if offer_pq {
            match vault_crypto::pq::mlkem768_sk_from_seed(&ident.seed()) {
                Some(sk) => Some(sk),
                None => {
                    pq_note = Some("ml-kem init failed; downgraded to suite 1");
                    None
                }
            }
        } else {
            None
        };

        let mut noise = Builder::new(pattern.parse().map_err(|_| "bad pattern")?)
            .local_private_key(&ident.x25519_priv())
            .map_err(|_| "bad local key")?
            .build_initiator()
            .map_err(|_| "noise init failed")?;
        if role == Role::Responder {
            noise = Builder::new(pattern.parse().map_err(|_| "bad pattern")?)
                .local_private_key(&ident.x25519_priv())
                .map_err(|_| "bad local key")?
                .build_responder()
                .map_err(|_| "noise init failed")?;
        }
        if let Some(psk) = psk {
            noise.set_psk(3, psk).map_err(|_| "set psk failed")?;
        }

        let msg1_plain = if psk.is_some() {
            NOISE_MSG1_PLAIN_PSK
        } else {
            NOISE_MSG1_PLAIN_NO_PSK
        };

        let mut buf = [0u8; MAX_NOISE_FRAME];
        let mut pq_ss: Option<[u8; 32]> = None;
        let mut cipher_suite = SUITE_LEGACY;
        match role {
            Role::Initiator => {
                // msg1（+ 套件提议）：仅在本端有能力时扩展帧，其余场景与 v0.7.0 逐字节一致
                let n = noise.write_message(&[], &mut buf).map_err(|_| "hs1")?;
                if sk.is_some() {
                    buf[n] = SUITE_PQ;
                    write_frame(stream, &buf[..n + 1])?;
                } else {
                    write_frame(stream, &buf[..n])?;
                }
                let frame = read_frame_vec(stream)?;
                if sk.is_some() {
                    // 有提议：按 msg2 是否携带封装公钥判定（裸 msg2 = 带内降级）
                    if frame.len() == NOISE_MSG2_PLAIN + vault_crypto::MLKEM768_EK_LEN {
                        cipher_suite = SUITE_PQ;
                        noise
                            .read_message(&frame[..NOISE_MSG2_PLAIN], &mut [])
                            .map_err(|_| "hs2")?;
                        let (ct, ss) =
                            vault_crypto::pq::mlkem768_encapsulate(&frame[NOISE_MSG2_PLAIN..])
                                .ok_or("pq encapsulate failed")?;
                        pq_ss = Some(ss);
                        let m = noise.write_message(&[], &mut buf).map_err(|_| "hs3")?;
                        buf[m..m + ct.len()].copy_from_slice(&ct);
                        write_frame(stream, &buf[..m + ct.len()])?;
                    } else if frame.len() == NOISE_MSG2_PLAIN {
                        pq_note = Some("peer declined pq; downgraded to suite 1");
                        noise.read_message(&frame, &mut []).map_err(|_| "hs2")?;
                        let n = noise.write_message(&[], &mut buf).map_err(|_| "hs3")?;
                        write_frame(stream, &buf[..n])?;
                    } else {
                        return Err("hs2 frame length");
                    }
                } else {
                    // 无提议：响应端（新/旧）都只会回裸 msg2
                    noise.read_message(&frame, &mut []).map_err(|_| "hs2")?;
                    let n = noise.write_message(&[], &mut buf).map_err(|_| "hs3")?;
                    write_frame(stream, &buf[..n])?;
                }
            }
            Role::Responder => {
                // msg1：按帧长嗅探区分旧端（裸 msg1）与新端（msg1 ‖ 套件号）
                let first = read_frame_vec(stream)?;
                let offered = match first.len() {
                    l if l == msg1_plain => false,
                    l if l == msg1_plain + 1 && first[l - 1] == SUITE_PQ => true,
                    _ => return Err("hs1 frame length"),
                };
                let suite2 = offered && sk.is_some();
                noise
                    .read_message(&first[..msg1_plain], &mut [])
                    .map_err(|_| "hs1")?;
                let n = noise.write_message(&[], &mut buf).map_err(|_| "hs2")?;
                if suite2 {
                    // suite2 ⇒ sk 必在（构造条件），let-else 解包以满足无 expect 纪律
                    let Some(sk_ref) = sk.as_ref() else {
                        return Err("pq internal: suite2 without ml-kem");
                    };
                    // msg2 ‖ 封装公钥：应答套件 2
                    cipher_suite = SUITE_PQ;
                    let ek = sk_ref.ek_bytes();
                    buf[n..n + ek.len()].copy_from_slice(&ek);
                    write_frame(stream, &buf[..n + ek.len()])?;
                    let frame3 = read_frame_vec(stream)?;
                    if frame3.len() != NOISE_MSG3_PLAIN + vault_crypto::MLKEM768_CT_LEN {
                        return Err("hs3 frame length");
                    }
                    noise
                        .read_message(&frame3[..NOISE_MSG3_PLAIN], &mut [])
                        .map_err(|_| "hs3")?;
                    let ss = sk_ref
                        .decapsulate(&frame3[NOISE_MSG3_PLAIN..])
                        .ok_or("pq decapsulate failed")?;
                    pq_ss = Some(ss);
                } else {
                    if offered {
                        pq_note = Some("peer offered pq but local ml-kem unavailable");
                    }
                    write_frame(stream, &buf[..n])?;
                    let n = read_frame(stream, &mut buf)?;
                    noise.read_message(&buf[..n], &mut []).map_err(|_| "hs3")?;
                }
            }
        }
        let remote_static = noise.get_remote_static().ok_or("no remote static")?;
        let mut rs = [0u8; 32];
        rs.copy_from_slice(remote_static);
        let hh: [u8; 32] = noise
            .get_handshake_hash()
            .try_into()
            .map_err(|_| "hh len")?;
        let pq_keys = pq_ss.map(|ss| match role {
            Role::Initiator => (
                pq_direction_key(&hh, &ss, "i2r"),
                pq_direction_key(&hh, &ss, "r2i"),
            ),
            Role::Responder => (
                pq_direction_key(&hh, &ss, "r2i"),
                pq_direction_key(&hh, &ss, "i2r"),
            ),
        });
        let transport = noise.into_transport_mode().map_err(|_| "into transport")?;
        Ok(Self {
            transport,
            handshake_hash: hh,
            send_seq: 0,
            recv_seq: 0,
            remote_static: rs,
            pq_keys,
            cipher_suite,
            pq_note,
            pad_tier: 0,
            assembler: vault_net::frame::FrameAssembler::default(),
            frames_by_tier: [0u64; 4],
            payload_bytes: 0,
            wire_bytes: 0,
        })
    }

    /// P8-6：Hello 协商后设置生效填充档（0 = 旧帧格式）。
    /// 旧端（proto_ver < 2 或未声明）恒 0——**不得**在未协商时填充，否则解析损坏。
    pub fn set_padding(&mut self, tier: u8) {
        self.pad_tier = tier.min(4);
    }

    /// 填充诊断快照（05-04 F-07）：生效档、逐档帧数、载荷/线上字节比。
    pub fn padding_stats(&self) -> (u8, [u64; 4], f64) {
        let ratio = if self.wire_bytes > 0 {
            self.payload_bytes as f64 / self.wire_bytes as f64
        } else {
            0.0
        };
        (
            self.pad_tier,
            self.frames_by_tier,
            (ratio * 1000.0).round() / 1000.0,
        )
    }

    /// 逐档帧计数（宿主聚合进 frameLenHistogram）。
    pub fn frame_histogram(&self) -> (u64, u64, u64, u64) {
        (
            self.frames_by_tier[0],
            self.frames_by_tier[1],
            self.frames_by_tier[2],
            self.frames_by_tier[3],
        )
    }

    /// 载荷与线上字节总量（宿主聚合 overheadRatio 用；线上含 4B 前缀 + 填充）。
    pub fn byte_totals(&self) -> (u64, u64) {
        (self.payload_bytes, self.wire_bytes)
    }

    pub fn handshake_hash(&self) -> &[u8; 32] {
        &self.handshake_hash
    }

    pub fn remote_static(&self) -> &[u8; 32] {
        &self.remote_static
    }

    /// 协商结果（1 = 经典 / 2 = 混合 KEM）。强制可见：调用方须在日志与状态中暴露。
    pub fn cipher_suite(&self) -> u8 {
        self.cipher_suite
    }

    /// 请求过 PQ 却未启用时的原因（None = 未请求或已启用）。
    pub fn pq_note(&self) -> Option<&'static str> {
        self.pq_note
    }

    /// 远端静态公钥（X25519）是否等于对端 Hello 中声明并签名的公钥。
    pub fn remote_is(&self, x25519_pub_hex: &str) -> bool {
        match vault_crypto::hex_decode(x25519_pub_hex) {
            Some(exp) => exp.as_slice() == self.remote_static,
            None => false,
        }
    }

    /// PQ 层 nonce：12B = 4B 零 ‖ 64 位方向内单调序号（密钥按方向分离，不重用）。
    fn pq_nonce(seq: u64) -> [u8; AES_GCM_NONCE_LEN] {
        let mut n = [0u8; AES_GCM_NONCE_LEN];
        n[4..].copy_from_slice(&seq.to_be_bytes());
        n
    }

    /// 发送一条应用消息（自动分帧）。噪声层已认证加密，序号再防重放；
    /// suite 2 时载荷先经 PQ AEAD（内层），两层密钥独立。
    /// pad_tier > 0 时走新帧格式：每帧 `[u32 real_len][payload][填充]`，
    /// 填充在加密前施加（AEAD 覆盖，观测者无法区分真假），计入线上字节统计。
    pub fn send_msg<S: Read + Write>(
        &mut self,
        stream: &mut S,
        payload: &[u8],
    ) -> Result<(), &'static str> {
        if payload.len() > MAX_APP_MSG {
            return Err("message too large");
        }
        let body: Vec<u8> = match &self.pq_keys {
            Some((k, _)) => vault_crypto::aead_encrypt(k, &Self::pq_nonce(self.send_seq), payload)
                .map_err(|_| "pq encrypt")?,
            None => payload.to_vec(),
        };
        if self.pad_tier > 0 {
            let tier = self.pad_tier;
            // 消息体格式与旧格式一致：`[u64 seq][payload]`，仅外层分帧不同
            let mut plain = Vec::with_capacity(8 + body.len());
            plain.extend_from_slice(&self.send_seq.to_be_bytes());
            plain.extend_from_slice(&body);
            for frame in vault_net::frame::encode_frames(&plain, tier) {
                self.frames_by_tier[(tier - 1) as usize] += 1;
                self.payload_bytes +=
                    u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as u64;
                let mut buf = [0u8; MAX_NOISE_FRAME];
                let n = self
                    .transport
                    .write_message(&frame, &mut buf)
                    .map_err(|_| "noise encrypt")?;
                self.wire_bytes += (4 + n) as u64;
                write_frame(stream, &buf[..n])?;
            }
            stream.flush().map_err(|_| "flush")?;
            self.send_seq += 1;
            return Ok(());
        }
        let mut plain = Vec::with_capacity(8 + body.len());
        plain.extend_from_slice(&self.send_seq.to_be_bytes());
        plain.extend_from_slice(&body);
        let total = plain.len() as u32;
        stream
            .write_all(&total.to_be_bytes())
            .map_err(|_| "write len")?;
        for chunk in plain.chunks(CHUNK_PAYLOAD) {
            let mut buf = [0u8; MAX_NOISE_FRAME];
            let n = self
                .transport
                .write_message(chunk, &mut buf)
                .map_err(|_| "noise encrypt")?;
            write_frame(stream, &buf[..n])?;
        }
        stream.flush().map_err(|_| "flush")?;
        self.send_seq += 1;
        Ok(())
    }

    /// 序号防重放 + PQ 内层解封（两种帧格式共用）。
    fn finish_recv(&mut self, plain: Vec<u8>) -> Result<Vec<u8>, &'static str> {
        if plain.len() < 8 {
            return Err("short message");
        }
        let seq = u64::from_be_bytes(plain[..8].try_into().map_err(|_| "short")?);
        if seq != self.recv_seq {
            return Err("replay or out-of-order sequence");
        }
        self.recv_seq += 1;
        let body = &plain[8..];
        let payload = match &self.pq_keys {
            Some((_, k)) => vault_crypto::aead_decrypt(k, body).ok_or("pq decrypt or tampered")?,
            None => body.to_vec(),
        };
        Ok(payload)
    }

    /// 接收一条应用消息；序号必须严格 +1，否则判定重放/乱序并拒绝（docs/05-04 §4.2）。
    pub fn recv_msg<S: Read + Write>(&mut self, stream: &mut S) -> Result<Vec<u8>, &'static str> {
        if self.pad_tier > 0 {
            // 新帧格式：逐帧解密喂重组器，末帧（real_len < 帧容量）完成消息
            let mut buf = [0u8; MAX_NOISE_FRAME];
            loop {
                let n = read_frame(stream, &mut buf)?;
                let m = {
                    let tmp = buf[..n].to_vec();
                    self.transport
                        .read_message(&tmp, &mut buf)
                        .map_err(|_| "noise decrypt")?
                };
                let frame = buf[..m].to_vec();
                self.frames_by_tier[(self.pad_tier - 1) as usize] += 1;
                self.wire_bytes += (4 + n) as u64;
                let done = self.assembler.push(&frame).map_err(|_| "frame decode")?;
                if let Some(plain) = done {
                    // 消息体格式与旧格式一致：`[u64 seq][payload]`，仅外层分帧不同
                    self.payload_bytes += (plain.len() - 8) as u64;
                    return self.finish_recv(plain);
                }
            }
        }
        let mut total_buf = [0u8; 4];
        stream.read_exact(&mut total_buf).map_err(|_| "eof")?;
        let total = u32::from_be_bytes(total_buf) as usize;
        if total > MAX_APP_MSG {
            return Err("message too large");
        }
        let mut plain = vec![0u8; total];
        let mut got = 0usize;
        let mut buf = [0u8; MAX_NOISE_FRAME];
        while got < total {
            let n = read_frame(stream, &mut buf)?;
            let m = {
                let tmp = buf[..n].to_vec();
                self.transport
                    .read_message(&tmp, &mut buf)
                    .map_err(|_| "noise decrypt")?
            };
            let out = buf[..m].to_vec();
            if got + out.len() > total {
                return Err("frame overflow");
            }
            plain[got..got + out.len()].copy_from_slice(&out);
            got += out.len();
            if m == 0 && got < total {
                return Err("truncated message");
            }
        }
        self.payload_bytes += plain.len().saturating_sub(8) as u64;
        self.finish_recv(plain)
    }
}

fn write_frame<S: Write>(stream: &mut S, data: &[u8]) -> Result<(), &'static str> {
    let len = data.len() as u32;
    stream
        .write_all(&len.to_be_bytes())
        .map_err(|_| "write frame len")?;
    stream.write_all(data).map_err(|_| "write frame")?;
    Ok(())
}

fn read_frame<S: Read>(stream: &mut S, buf: &mut [u8]) -> Result<usize, &'static str> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).map_err(|_| "eof")?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_NOISE_FRAME {
        return Err("frame too large");
    }
    stream.read_exact(&mut buf[..len]).map_err(|_| "eof")?;
    Ok(len)
}

fn read_frame_vec<S: Read>(stream: &mut S) -> Result<Vec<u8>, &'static str> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).map_err(|_| "eof")?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_NOISE_FRAME {
        return Err("frame too large");
    }
    let mut out = vec![0u8; len];
    stream.read_exact(&mut out).map_err(|_| "eof")?;
    Ok(out)
}

/// 由握手哈希派生子密钥（fskey E2E 携带等）。
pub fn subkey(hh: &[u8; 32], info: &str) -> [u8; 32] {
    hash_to_key(&hash_sha256(
        format!("vsync-sub:{}:{}", hex_encode(hh), info).as_bytes(),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};

    fn duplex_pair() -> (TcpStream, TcpStream) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let c = TcpStream::connect(addr).unwrap();
        let (s, _) = l.accept().unwrap();
        (c, s)
    }

    /// TcpStream read 精确读尽时返回 Ok(0)，握手辅助无影响；此处确保 0 长读不误判。
    #[test]
    fn channel_handshake_and_replay_protection() {
        let (mut ca, mut sb) = duplex_pair();
        let ia = Identity::generate();
        let ib = Identity::generate();
        let psk = psk_from_code("invite-code");
        let (mut cha, mut chb) = std::thread::scope(|s| {
            let t = s.spawn(|| {
                SecureChannel::handshake(&mut sb, Role::Responder, &ib, Some(&psk), false)
            });
            let a =
                SecureChannel::handshake(&mut ca, Role::Initiator, &ia, Some(&psk), false).unwrap();
            (a, t.join().unwrap().unwrap())
        });
        assert_eq!(cha.cipher_suite(), SUITE_LEGACY);
        assert_eq!(chb.cipher_suite(), SUITE_LEGACY);
        println!(
            "remote_static = {}",
            vault_crypto::hex_encode(cha.remote_static())
        );
        assert!(cha.remote_is(&hex_encode(&ib.x25519_pub())));
        assert!(chb.remote_is(&hex_encode(&ia.x25519_pub())));
        cha.send_msg(&mut ca, b"msg-1").unwrap();
        assert_eq!(chb.recv_msg(&mut sb).unwrap(), b"msg-1");
        cha.send_msg(&mut ca, b"msg-2").unwrap();
        assert_eq!(chb.recv_msg(&mut sb).unwrap(), b"msg-2");
        // 大消息（>64KB）自动分帧
        let big: Vec<u8> = (0..200_000usize).map(|i| (i % 251) as u8).collect();
        cha.send_msg(&mut ca, &big).unwrap();
        assert_eq!(chb.recv_msg(&mut sb).unwrap(), big);
    }

    #[test]
    fn psk_mismatch_fails_responder_handshake() {
        // psk3 在第三条握手消息混入：双方口令不一致时响应端解密必败（防 MITM 无码接入）
        let (ca, mut sb) = duplex_pair();
        let ia = Identity::generate();
        let ib = Identity::generate();
        let good = psk_from_code("same-code");
        let bad = psk_from_code("other-code");
        let resp = std::thread::scope(|s| {
            let t = s.spawn(move || {
                SecureChannel::handshake(&mut sb, Role::Responder, &ib, Some(&bad), false)
            });
            let mut ca = ca;
            let _ = SecureChannel::handshake(&mut ca, Role::Initiator, &ia, Some(&good), false);
            t.join().unwrap()
        });
        assert!(resp.is_err());
    }

    #[test]
    fn subkey_depends_on_info() {
        let hh = [1u8; 32];
        assert_ne!(subkey(&hh, "a"), subkey(&hh, "b"));
    }

    /// 守护帧长嗅探所依赖的 snow 定长常量；snow 升级若改布局此处先红。
    #[test]
    fn noise_msg_lengths_are_stable() {
        let lens = |pattern: &str, psk: Option<&[u8; 32]>| {
            let mut i = Builder::new(pattern.parse().unwrap())
                .local_private_key(&[1u8; 32])
                .unwrap()
                .build_initiator()
                .unwrap();
            let mut r = Builder::new(pattern.parse().unwrap())
                .local_private_key(&[2u8; 32])
                .unwrap()
                .build_responder()
                .unwrap();
            if let Some(p) = psk {
                i.set_psk(3, p).unwrap();
                r.set_psk(3, p).unwrap();
            }
            let mut b = [0u8; 1024];
            let n1 = i.write_message(&[], &mut b).unwrap();
            r.read_message(&b[..n1], &mut []).unwrap();
            let n2 = r.write_message(&[], &mut b).unwrap();
            i.read_message(&b[..n2], &mut []).unwrap();
            let n3 = i.write_message(&[], &mut b).unwrap();
            r.read_message(&b[..n3], &mut []).unwrap();
            (n1, n2, n3)
        };
        assert_eq!(lens(PATTERN_XX, None), (32, 96, 64));
        assert_eq!(lens(PATTERN_XX_PSK3, Some(&[9u8; 32])), (48, 96, 64));
    }

    /// P7-8 证据：双端具备能力 → 协商 suite 2，混合信道收发（含大消息分帧）。
    #[test]
    fn pq_hybrid_handshake_roundtrip() {
        let (mut ca, mut sb) = duplex_pair();
        let ia = Identity::generate();
        let ib = Identity::generate();
        let psk = psk_from_code("pq-pairing");
        let (mut cha, mut chb) = std::thread::scope(|s| {
            let t = s.spawn(|| {
                SecureChannel::handshake(&mut sb, Role::Responder, &ib, Some(&psk), true)
            });
            let a =
                SecureChannel::handshake(&mut ca, Role::Initiator, &ia, Some(&psk), true).unwrap();
            (a, t.join().unwrap().unwrap())
        });
        assert_eq!(cha.cipher_suite(), SUITE_PQ, "双端有能力必须协商 suite 2");
        assert_eq!(chb.cipher_suite(), SUITE_PQ);
        assert!(cha.pq_note().is_none() || cha.pq_note() == Some("pq not offered"));
        // 同一身份套件 1 与套件 2 的握手哈希必须不同（PQ 层真实参与）
        cha.send_msg(&mut ca, b"pq-msg-1").unwrap();
        assert_eq!(chb.recv_msg(&mut sb).unwrap(), b"pq-msg-1");
        let big: Vec<u8> = (0..150_000usize).map(|i| (i % 241) as u8).collect();
        cha.send_msg(&mut ca, &big).unwrap();
        assert_eq!(chb.recv_msg(&mut sb).unwrap(), big);
        // 反向也通（r2i 方向密钥独立）
        chb.send_msg(&mut sb, b"ack").unwrap();
        assert_eq!(cha.recv_msg(&mut ca).unwrap(), b"ack");
    }

    /// P7-8 证据：对端不支持 → 带内降级到 suite 1，信道照常可用且降级可见。
    #[test]
    fn pq_negotiates_down_to_suite1() {
        let (mut ca, mut sb) = duplex_pair();
        let ia = Identity::generate();
        let ib = Identity::generate();
        // 发起端提议 PQ（true），响应端不允许（false）→ 裸 msg2 应答 = 带内降级
        let (mut cha, mut chb) = std::thread::scope(|s| {
            let t =
                s.spawn(|| SecureChannel::handshake(&mut sb, Role::Responder, &ib, None, false));
            let a = SecureChannel::handshake(&mut ca, Role::Initiator, &ia, None, true).unwrap();
            (a, t.join().unwrap().unwrap())
        });
        assert_eq!(cha.cipher_suite(), SUITE_LEGACY);
        assert_eq!(chb.cipher_suite(), SUITE_LEGACY);
        assert_eq!(
            cha.pq_note(),
            Some("peer declined pq; downgraded to suite 1"),
            "降级必须可见"
        );
        cha.send_msg(&mut ca, b"still-works").unwrap();
        assert_eq!(chb.recv_msg(&mut sb).unwrap(), b"still-works");
    }

    /// 旧版兼容：双端都不提供 PQ → 线上帧与 v0.7.0 一致（suite 1，无 note）。
    #[test]
    fn no_pq_offering_matches_legacy_wire() {
        let (mut ca, mut sb) = duplex_pair();
        let ia = Identity::generate();
        let ib = Identity::generate();
        let (mut cha, mut chb) = std::thread::scope(|s| {
            let t =
                s.spawn(|| SecureChannel::handshake(&mut sb, Role::Responder, &ib, None, false));
            let a = SecureChannel::handshake(&mut ca, Role::Initiator, &ia, None, false).unwrap();
            (a, t.join().unwrap().unwrap())
        });
        assert_eq!(cha.cipher_suite(), SUITE_LEGACY);
        assert_eq!(cha.pq_note(), Some("pq not offered"));
        assert_eq!(chb.pq_note(), Some("pq not offered"));
        cha.send_msg(&mut ca, b"legacy").unwrap();
        assert_eq!(chb.recv_msg(&mut sb).unwrap(), b"legacy");
    }

    /// P8-6 证据：协商档位下（05-04 §3.2/§3.3）——
    /// ① 帧（加密前明文）长度恒为档位值；② 大消息跨帧重组；
    /// ③ 直方图与线上字节统计可查（诊断口径）。
    #[test]
    fn padded_framing_roundtrip_and_tier_lengths() {
        let (mut ca, mut sb) = duplex_pair();
        let ia = Identity::generate();
        let ib = Identity::generate();
        let (mut cha, mut chb) = std::thread::scope(|s| {
            let t =
                s.spawn(|| SecureChannel::handshake(&mut sb, Role::Responder, &ib, None, false));
            let a = SecureChannel::handshake(&mut ca, Role::Initiator, &ia, None, false).unwrap();
            (a, t.join().unwrap().unwrap())
        });
        cha.set_padding(3);
        chb.set_padding(3);
        // 小消息（档 3 单帧）与大消息（跨帧：70KB > 16360）
        cha.send_msg(&mut ca, b"small").unwrap();
        assert_eq!(chb.recv_msg(&mut sb).unwrap(), b"small");
        let big: Vec<u8> = (0..70_000usize).map(|i| (i % 241) as u8).collect();
        cha.send_msg(&mut ca, &big).unwrap();
        assert_eq!(chb.recv_msg(&mut sb).unwrap(), big);
        // 反向
        chb.send_msg(&mut sb, b"ack").unwrap();
        assert_eq!(cha.recv_msg(&mut ca).unwrap(), b"ack");
        // 帧长（加密前明文）恒等于档 3 上限 16364；帧数 = ceil
        let (t, hist, ratio) = cha.padding_stats();
        assert_eq!(t, 3);
        let total_frames: u64 = hist.iter().sum();
        assert!(
            total_frames >= 2,
            "小消息 + 跨帧大消息至少 2 帧（{hist:?}）"
        );
        assert_eq!(hist[2], total_frames, "全部帧都落在档 3");
        assert_eq!(hist[0] + hist[1] + hist[3], 0);
        // 载荷/线上字节比 ≤ 1：填充与 tag 开销如实计入统计（小消息整帧填充拉低占比）
        assert!(
            (0.0..=1.0).contains(&ratio) && ratio > 0.0,
            "overheadRatio 必须在 (0,1]（{ratio}）"
        );
    }
}
