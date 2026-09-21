//! LSB 隐写引擎（P5-3 实现，设计见 docs/05-05 §三 / §六）。
#![deny(warnings)]
#![forbid(unsafe_code)]
//!
//! 本 crate 只做「位搬运」：把调用方给的**已加密字节**（通常是 vault-vault 的 VSEF 容器密文）
//! 按位写进 PNG 像素 RGB 通道的最低有效位，并能原样取出；此处**不做任何加密 / 解密**
//! （加密先行，见 docs/05-05 §3.3）。
//!
//! 帧格式与顺序：
//!
//! - 载荷帧 = `[u32 大端 payload 长度] ‖ payload`；
//! - 从第 1 个像素的 R 通道 LSB 开始，按位（LSB-first：bit0 先写）顺序写入 R→G→B，跳过 A；
//! - 容量 = `(width * height * 3) / 8 - 4`（已扣除长度前缀）；超出容量直接报错，**绝不截断**。
//!
//! 格式约束（docs/05-05 §六：PNG 无损才保得住 LSB）：
//!
//! - 只接受 8 位 RGB / RGBA PNG；灰度 / 调色板 / 16 位一律返回 `unsupported png format`
//!   （调色板图写 LSB 会改变索引语义，无法安全隐写）；
//! - 输出统一为 **RGBA 8 位**无损 PNG，避免后继工具误判；A 通道原样保留（输入无 A 则填 255）。
//!
//! 错误信息（`&'static str`，FFI 层按此上抛）：
//!
//! | 信息 | 触发条件 |
//! | --- | --- |
//! | `unsupported png format` | 非 8 位 RGB / RGBA（灰度 / 调色板 / 16 位） |
//! | `invalid png` | PNG 解码或编码失败 |
//! | `empty payload` | 提取时长度字段为 0；嵌入空载荷 |
//! | `payload length out of range` | 提取时长度字段超出图片容量（防恶意图片诱导巨额分配） |
//! | `payload exceeds image capacity` | 嵌入时载荷超过 `capacity_bytes` |
//! | `image too large` | 尺寸头声称的解码缓冲超过防御上限 |
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub use vault_crypto;

/// 载荷帧长度前缀字节数：u32 大端。
const LEN_PREFIX: usize = 4;

/// 参与隐写的通道数（R、G、B；A 通道跳过，避免改动透明通道的低透明抖动）。
const CHANNELS_PER_PIXEL: usize = 3;

/// 解码缓冲防御上限：尺寸头是可被构造的，先卡住上限再分配，避免巨额分配（256 MiB ≈ 8192×8192 RGBA）。
const MAX_DECODED_BYTES: usize = 256 * 1024 * 1024;

const ERR_FORMAT: &str = "unsupported png format";
const ERR_DECODE: &str = "invalid png";
const ERR_EMPTY: &str = "empty payload";
const ERR_OUT_OF_RANGE: &str = "payload length out of range";
const ERR_OVER_CAPACITY: &str = "payload exceeds image capacity";
const ERR_TOO_LARGE: &str = "image too large";

/// 引擎自检：确认依赖边可用。
pub fn self_check() -> bool {
    vault_crypto::self_check()
}

/// 图片容量（可嵌入的载荷字节数，已扣除 4 字节长度前缀）。
///
/// `capacity_bytes(1920, 1080) == 777596`（docs/05-05 §3.2：1920 × 1080 × 3 ÷ 8 - 4）。
pub fn capacity_bytes(width: u32, height: u32) -> usize {
    let channel_bits = u64::from(width) * u64::from(height) * CHANNELS_PER_PIXEL as u64;
    // 尺寸极小时容量为 0 而非下溢。
    let capacity = (channel_bits / 8).saturating_sub(LEN_PREFIX as u64);
    usize::try_from(capacity).unwrap_or(usize::MAX)
}

/// 从 PNG 字节读出图片尺寸（用于 UI 预估容量与前置校验）。
///
/// 只解析文件头，不展开像素数据；尺寸可读即返回，格式支持性由 `embed` / `extract` 把关。
pub fn png_size(png: &[u8]) -> Result<(u32, u32), &'static str> {
    let decoder = png::Decoder::new(png);
    let reader = decoder.read_info().map_err(|_| ERR_DECODE)?;
    let info = reader.info();
    Ok((info.width, info.height))
}

/// 把 payload 嵌入 png，返回新的 PNG 字节（RGBA 8 位无损）。
pub fn embed(png: &[u8], payload: &[u8]) -> Result<Vec<u8>, &'static str> {
    if payload.is_empty() {
        // 长度字段为 0 的帧是 `extract` 明确拒绝的，直接不生产这种图。
        return Err(ERR_EMPTY);
    }
    let (width, height, mut rgba) = decode_rgba(png)?;
    if payload.len() > capacity_bytes(width, height) {
        return Err(ERR_OVER_CAPACITY);
    }
    let payload_len = u32::try_from(payload.len()).map_err(|_| ERR_OVER_CAPACITY)?;

    let mut frame = Vec::with_capacity(LEN_PREFIX + payload.len());
    frame.extend_from_slice(&payload_len.to_be_bytes());
    frame.extend_from_slice(payload);

    write_lsb(&mut rgba, &frame);
    encode_rgba(&rgba, width, height)
}

/// 从 PNG 提取载荷。
pub fn extract(png: &[u8]) -> Result<Vec<u8>, &'static str> {
    let (width, height, rgba) = decode_rgba(png)?;

    let prefix: [u8; LEN_PREFIX] = match read_lsb(&rgba, LEN_PREFIX).try_into() {
        Ok(bytes) => bytes,
        Err(_) => return Err(ERR_DECODE),
    };
    let payload_len = u32::from_be_bytes(prefix) as usize;
    if payload_len == 0 {
        return Err(ERR_EMPTY);
    }
    // 先校验长度字段再按长度读取：恶意图片无法诱导巨额分配。
    if payload_len > capacity_bytes(width, height) {
        return Err(ERR_OUT_OF_RANGE);
    }

    let frame = read_lsb(&rgba, LEN_PREFIX + payload_len);
    Ok(frame[LEN_PREFIX..].to_vec())
}

/// 解码 PNG 并统一展开为 RGBA 8 位像素缓冲（长度 = width × height × 4）。
fn decode_rgba(png: &[u8]) -> Result<(u32, u32, Vec<u8>), &'static str> {
    let decoder = png::Decoder::new(png);
    let mut reader = decoder.read_info().map_err(|_| ERR_DECODE)?;

    let info = reader.info();
    let width = info.width;
    let height = info.height;
    let has_alpha = match (info.color_type, info.bit_depth) {
        (png::ColorType::Rgb, png::BitDepth::Eight) => false,
        (png::ColorType::Rgba, png::BitDepth::Eight) => true,
        _ => return Err(ERR_FORMAT),
    };

    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or(ERR_TOO_LARGE)?;
    let channels = if has_alpha { 4usize } else { 3usize };
    if pixels.checked_mul(channels).ok_or(ERR_TOO_LARGE)? > MAX_DECODED_BYTES {
        return Err(ERR_TOO_LARGE);
    }

    let mut raw = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut raw).map_err(|_| ERR_DECODE)?;
    let frame = &raw[..frame.buffer_size()];
    if frame.len() != pixels * channels {
        return Err(ERR_DECODE);
    }

    // 统一展开为 RGBA：无 A 通道时补不透明 255。
    let mut rgba = Vec::with_capacity(pixels * 4);
    if has_alpha {
        rgba.extend_from_slice(frame);
    } else {
        for pixel in frame.chunks_exact(3) {
            rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 0xFF]);
        }
    }
    Ok((width, height, rgba))
}

/// 编码为 RGBA 8 位无损 PNG。
fn encode_rgba(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, &'static str> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|_| ERR_DECODE)?;
        writer.write_image_data(rgba).map_err(|_| ERR_DECODE)?;
        writer.finish().map_err(|_| ERR_DECODE)?;
    }
    Ok(out)
}

/// 把 `frame` 的比特按 LSB-first 顺序写入像素 R→G→B 通道的最低有效位（跳过 A）。
///
/// 调用方须保证 `frame.len() * 8 <= (rgba.len() / 4) * CHANNELS_PER_PIXEL`；写满即停。
fn write_lsb(rgba: &mut [u8], frame: &[u8]) {
    let mut bits = frame.iter().flat_map(|byte| {
        let byte = *byte;
        (0..8).map(move |i| (byte >> i) & 1)
    });
    for pixel in rgba.chunks_exact_mut(4) {
        for channel in pixel.iter_mut().take(CHANNELS_PER_PIXEL) {
            match bits.next() {
                Some(bit) => *channel = (*channel & 0xFE) | bit,
                None => return,
            }
        }
    }
}

/// 从 RGBA 像素缓冲读取 `byte_len` 字节（LSB-first，R→G→B，跳过 A）。
fn read_lsb(rgba: &[u8], byte_len: usize) -> Vec<u8> {
    let mut out = vec![0u8; byte_len];
    let total_bits = byte_len * 8;
    let mut bit_pos = 0usize;
    'pixels: for pixel in rgba.chunks_exact(4) {
        for &channel in pixel.iter().take(CHANNELS_PER_PIXEL) {
            if bit_pos >= total_bits {
                break 'pixels;
            }
            out[bit_pos / 8] |= (channel & 1) << (bit_pos % 8);
            bit_pos += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试辅助：把原始像素编码成 PNG。
    fn encode_raw(
        width: u32,
        height: u32,
        color: png::ColorType,
        depth: png::BitDepth,
        data: &[u8],
    ) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(color);
            encoder.set_depth(depth);
            if color == png::ColorType::Indexed {
                // 4 色调色板（每色 3 字节）
                encoder.set_palette(vec![0u8, 0, 0, 255, 255, 255, 0, 255, 0, 0, 0, 255]);
            }
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(data).unwrap();
            writer.finish().unwrap();
        }
        out
    }

    /// 测试辅助：确定性 64×64 RGB 测试图。
    ///
    /// 像素值一律取偶数，因此未嵌入数据的图片前 4 字节 LSB 恒为 0（长度字段读作 0）。
    fn rgb_test_png(width: u32, height: u32) -> Vec<u8> {
        let mut data = Vec::with_capacity((width as usize) * (height as usize) * 3);
        for i in 0..width * height {
            let base = ((i % 251) as u8) & 0xFE;
            data.push(base);
            data.push(base.wrapping_add(38) & 0xFE);
            data.push(base.wrapping_add(102) & 0xFE);
        }
        encode_raw(
            width,
            height,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &data,
        )
    }

    /// 读取任意 PNG 的像素格式元信息。
    fn image_format(png: &[u8]) -> (png::ColorType, png::BitDepth) {
        let decoder = png::Decoder::new(png);
        let reader = decoder.read_info().unwrap();
        (reader.info().color_type, reader.info().bit_depth)
    }

    #[test]
    fn dependency_edges_work() {
        assert!(self_check());
    }

    #[test]
    fn roundtrip_preserves_payload_exactly() {
        let png = rgb_test_png(64, 64);
        let mut payload = "VSEF v2 隐写载荷：中文 + 二进制混排".as_bytes().to_vec();
        payload.extend_from_slice(&[0x00, 0x01, 0x7F, 0x80, 0xFE, 0xFF, 0x00, 0xA5]);
        assert!(payload.len() <= capacity_bytes(64, 64));

        let embedded = embed(&png, &payload).unwrap();
        assert_eq!(extract(&embedded).unwrap(), payload);
        assert_eq!(png_size(&embedded).unwrap(), (64, 64));
        // 输出统一为 RGBA 8 位 PNG
        assert_eq!(
            image_format(&embedded),
            (png::ColorType::Rgba, png::BitDepth::Eight)
        );
        assert_ne!(embedded, png);
        // 非 PNG 输入
        assert!(png_size(&[0u8; 4]).is_err());
        assert_eq!(extract(&[0u8; 4]), Err(ERR_DECODE));
        assert_eq!(embed(&[0u8; 4], b"x"), Err(ERR_DECODE));
    }

    #[test]
    fn capacity_matches_documented_formula() {
        assert_eq!(capacity_bytes(1920, 1080), 1920 * 1080 * 3 / 8 - 4);
        assert_eq!(capacity_bytes(1920, 1080), 777596);
        assert_eq!(capacity_bytes(64, 64), 64 * 64 * 3 / 8 - 4);
        assert_eq!(capacity_bytes(1, 1), 0);
        assert_eq!(capacity_bytes(0, 0), 0);
    }

    #[test]
    fn payload_at_capacity_ok_and_one_byte_over_fails() {
        let png = rgb_test_png(64, 64);
        let capacity = capacity_bytes(64, 64);

        let full = vec![0xA5u8; capacity];
        let embedded = embed(&png, &full).unwrap();
        assert_eq!(extract(&embedded).unwrap(), full);

        let over = vec![0x5Au8; capacity + 1];
        assert_eq!(embed(&png, &over), Err(ERR_OVER_CAPACITY));
        // 超容量报错后不产出图片，且空载荷同样被拒
        assert_eq!(embed(&png, &[]), Err(ERR_EMPTY));
    }

    #[test]
    fn only_lsb_bits_change() {
        let png = rgb_test_png(64, 64);
        let payload = "VaultSync LSB 不变量".as_bytes().to_vec();
        let embedded = embed(&png, &payload).unwrap();

        let (w0, h0, before) = decode_rgba(&png).unwrap();
        let (w1, h1, after) = decode_rgba(&embedded).unwrap();
        assert_eq!((w0, h0), (w1, h1));
        assert_eq!(before.len(), after.len());
        for (i, (orig, new)) in before.iter().zip(after.iter()).enumerate() {
            if i % 4 == 3 {
                assert_eq!(orig, new, "A 通道被改动 @{i}");
            } else {
                assert_eq!(orig & 0xFE, new & 0xFE, "非 LSB 位被改动 @{i}");
            }
        }
    }

    #[test]
    fn unsupported_formats_are_rejected() {
        let gray = encode_raw(
            64,
            64,
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            &vec![128u8; 64 * 64],
        );
        assert_eq!(embed(&gray, b"payload"), Err(ERR_FORMAT));
        assert_eq!(extract(&gray), Err(ERR_FORMAT));

        let gray16 = encode_raw(
            64,
            64,
            png::ColorType::Grayscale,
            png::BitDepth::Sixteen,
            &vec![0x80u8; 64 * 64 * 2],
        );
        assert_eq!(extract(&gray16), Err(ERR_FORMAT));

        let indexed = encode_raw(
            64,
            64,
            png::ColorType::Indexed,
            png::BitDepth::Eight,
            &vec![1u8; 64 * 64],
        );
        assert_eq!(extract(&indexed), Err(ERR_FORMAT));
    }

    #[test]
    fn plain_png_and_oversized_length_field_are_rejected() {
        // 未嵌入任何数据的普通 PNG：长度字段为 0 → Err
        let png = rgb_test_png(64, 64);
        assert_eq!(extract(&png), Err(ERR_EMPTY));

        // 手工把长度字段写成 capacity + 1 → 必须先于分配被抓出
        let (width, height) = (64u32, 64u32);
        let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
        for (i, byte) in rgba.iter_mut().enumerate() {
            *byte = ((i % 251) as u8) & 0xFE;
        }
        let bogus_len = (capacity_bytes(width, height) as u32 + 1).to_be_bytes();
        write_lsb(&mut rgba, &bogus_len);
        let crafted = encode_raw(
            width,
            height,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &rgba,
        );
        assert_eq!(extract(&crafted), Err(ERR_OUT_OF_RANGE));
    }

    #[test]
    fn rgba_alpha_channel_is_preserved() {
        let (width, height) = (48u32, 32u32);
        let mut rgba = Vec::new();
        for i in 0..width * height {
            rgba.push(((i % 200) as u8) & 0xFE);
            rgba.push(((i * 3 % 200) as u8) & 0xFE);
            rgba.push(((i * 7 % 200) as u8) & 0xFE);
            rgba.push((i % 256) as u8); // A 含奇数值，验证隐写不触碰 A 通道
        }
        let png = encode_raw(
            width,
            height,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &rgba,
        );

        let payload = "alpha 保留验证".as_bytes().to_vec();
        let embedded = embed(&png, &payload).unwrap();
        let (_, _, after) = decode_rgba(&embedded).unwrap();
        for (i, (orig, new)) in rgba.iter().zip(after.iter()).enumerate() {
            if i % 4 == 3 {
                assert_eq!(orig, new, "A 通道被改动 @{i}");
            }
        }
        assert_eq!(extract(&embedded).unwrap(), payload);
        assert_eq!(
            image_format(&embedded),
            (png::ColorType::Rgba, png::BitDepth::Eight)
        );
    }
}
