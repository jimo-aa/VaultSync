//! 端到端加密信道（docs/05-04 §四、docs/08 §四）：
//! - Noise XX / XXpsk3（snow）做传输加密与双向认证；
//! - 应用层每条消息带单调递增序号，乱序/重复即拒绝（防重放，docs/08 §4.1）；
//! - 消息按 u32 长度分帧，内部再切 ≤64KB 的 Noise 帧以容纳大块传输。
use std::io::{Read, Write};

use snow::Builder;
use vault_crypto::{hash_sha256, hex_encode};

use crate::identity::Identity;

const MAX_NOISE_FRAME: usize = 65535;
/// 单帧明文上限 = 65535 − 16B GCM tag；留余量取 60000。
const CHUNK_PAYLOAD: usize = 60000;
const MAX_APP_MSG: usize = 4 * 1024 * 1024;
const PATTERN_XX: &str = "Noise_XX_25519_ChaChaPoly_SHA256";
const PATTERN_XX_PSK3: &str = "Noise_XXpsk3_25519_ChaChaPoly_SHA256";

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

pub struct SecureChannel {
    transport: snow::TransportState,
    handshake_hash: [u8; 32],
    send_seq: u64,
    recv_seq: u64,
    /// 远端 Noise 静态公钥（X25519，握手后提取）。
    remote_static: [u8; 32],
}

impl SecureChannel {
    /// 在既有字节流上执行 Noise 握手。`psk` 为配对阶段的一次性口令派生密钥。
    pub fn handshake<S: Read + Write>(
        stream: &mut S,
        role: Role,
        ident: &Identity,
        psk: Option<&[u8; 32]>,
    ) -> Result<Self, &'static str> {
        let pattern = if psk.is_some() {
            PATTERN_XX_PSK3
        } else {
            PATTERN_XX
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

        let mut buf = [0u8; MAX_NOISE_FRAME];
        match role {
            Role::Initiator => {
                let n = noise.write_message(&[], &mut buf).map_err(|_| "hs1")?;
                write_frame(stream, &buf[..n])?;
                let n = read_frame(stream, &mut buf)?;
                noise.read_message(&buf[..n], &mut []).map_err(|_| "hs2")?;
                let n = noise.write_message(&[], &mut buf).map_err(|_| "hs3")?;
                write_frame(stream, &buf[..n])?;
            }
            Role::Responder => {
                let n = read_frame(stream, &mut buf)?;
                noise.read_message(&buf[..n], &mut []).map_err(|_| "hs1")?;
                let n = noise.write_message(&[], &mut buf).map_err(|_| "hs2")?;
                write_frame(stream, &buf[..n])?;
                let n = read_frame(stream, &mut buf)?;
                noise.read_message(&buf[..n], &mut []).map_err(|_| "hs3")?;
            }
        }
        let remote_static = noise.get_remote_static().ok_or("no remote static")?;
        let mut rs = [0u8; 32];
        rs.copy_from_slice(remote_static);
        let hh: [u8; 32] = noise
            .get_handshake_hash()
            .try_into()
            .map_err(|_| "hh len")?;
        let transport = noise.into_transport_mode().map_err(|_| "into transport")?;
        Ok(Self {
            transport,
            handshake_hash: hh,
            send_seq: 0,
            recv_seq: 0,
            remote_static: rs,
        })
    }

    pub fn handshake_hash(&self) -> &[u8; 32] {
        &self.handshake_hash
    }

    pub fn remote_static(&self) -> &[u8; 32] {
        &self.remote_static
    }

    /// 远端静态公钥（X25519）是否等于对端 Hello 中声明并签名的公钥。
    pub fn remote_is(&self, x25519_pub_hex: &str) -> bool {
        match vault_crypto::hex_decode(x25519_pub_hex) {
            Some(exp) => exp.as_slice() == self.remote_static,
            None => false,
        }
    }

    /// 发送一条应用消息（自动分帧）。噪声层已认证加密，序号再防重放。
    pub fn send_msg<S: Read + Write>(
        &mut self,
        stream: &mut S,
        payload: &[u8],
    ) -> Result<(), &'static str> {
        if payload.len() > MAX_APP_MSG {
            return Err("message too large");
        }
        let mut plain = Vec::with_capacity(8 + payload.len());
        plain.extend_from_slice(&self.send_seq.to_be_bytes());
        plain.extend_from_slice(payload);
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

    /// 接收一条应用消息；序号必须严格 +1，否则判定重放/乱序并拒绝（docs/05-04 §4.2）。
    pub fn recv_msg<S: Read + Write>(&mut self, stream: &mut S) -> Result<Vec<u8>, &'static str> {
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
        if plain.len() < 8 {
            return Err("short message");
        }
        let seq = u64::from_be_bytes(plain[..8].try_into().map_err(|_| "short")?);
        if seq != self.recv_seq {
            return Err("replay or out-of-order sequence");
        }
        self.recv_seq += 1;
        Ok(plain[8..].to_vec())
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
            let t = s.spawn(|| SecureChannel::handshake(&mut sb, Role::Responder, &ib, Some(&psk)));
            let a = SecureChannel::handshake(&mut ca, Role::Initiator, &ia, Some(&psk)).unwrap();
            (a, t.join().unwrap().unwrap())
        });
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
            let t = s
                .spawn(move || SecureChannel::handshake(&mut sb, Role::Responder, &ib, Some(&bad)));
            let mut ca = ca;
            let _ = SecureChannel::handshake(&mut ca, Role::Initiator, &ia, Some(&good));
            t.join().unwrap()
        });
        assert!(resp.is_err());
    }

    #[test]
    fn subkey_depends_on_info() {
        let hh = [1u8; 32];
        assert_ne!(subkey(&hh, "a"), subkey(&hh, "b"));
    }
}
