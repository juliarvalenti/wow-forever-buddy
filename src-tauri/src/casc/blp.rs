//! BLP2 textures (what icons are stored as) to RGBA, and RGBA to PNG.
//! Only the full-size image is decoded. <https://wowdev.wiki/BLP>

use super::bytes::Bytes;
use super::{CascError, CascResult};

/// Icons are 64x64; the biggest textures in the game are 4096 wide.
const MAX_SIDE: u32 = 4096;
const HEADER: usize = 20 + 16 * 4 * 2 + 256 * 4;

#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// Row-major RGBA, 4 bytes a pixel.
    pub rgba: Vec<u8>,
}

pub fn decode(data: &[u8]) -> CascResult<Image> {
    let mut b = Bytes::new(data);
    if b.take(4, "BLP magic")? != b"BLP2" {
        return Err(CascError::Bad("BLP magic"));
    }
    if b.u32_le("BLP type")? != 1 {
        return Err(CascError::Bad("BLP type")); // 0 is JPEG, never used for icons
    }
    let encoding = b.u8("BLP header")?;
    let alpha_depth = b.u8("BLP header")?;
    let alpha_encoding = b.u8("BLP header")?;
    b.skip(1, "BLP header")?; // has mips
    let width = b.u32_le("BLP size")?;
    let height = b.u32_le("BLP size")?;
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(CascError::Bad("BLP size"));
    }
    let offset = b.u32_le("BLP mips")? as usize;
    b.skip(15 * 4, "BLP mips")?;
    let size = b.u32_le("BLP mips")? as usize;
    b.skip(15 * 4, "BLP mips")?;
    let palette = b.take(256 * 4, "BLP palette")?;
    debug_assert_eq!(b.pos(), HEADER);
    let mip = data
        .get(offset..offset.saturating_add(size))
        .ok_or(CascError::Bad("BLP mip"))?;

    let (w, h) = (width as usize, height as usize);
    let rgba = match encoding {
        1 => palettized(mip, palette, alpha_depth, w, h)?,
        2 => dxt(mip, alpha_depth, alpha_encoding, w, h)?,
        3 => {
            let bgra = mip.get(..w * h * 4).ok_or(CascError::Bad("BLP mip"))?;
            bgra.as_chunks::<4>()
                .0
                .iter()
                .flat_map(|&[b, g, r, a]| [r, g, b, a])
                .collect()
        }
        _ => return Err(CascError::Bad("BLP encoding")),
    };
    Ok(Image {
        width,
        height,
        rgba,
    })
}

fn palettized(mip: &[u8], palette: &[u8], depth: u8, w: usize, h: usize) -> CascResult<Vec<u8>> {
    let n = w * h;
    let alpha_len = match depth {
        0 => 0,
        1 => n.div_ceil(8),
        4 => n.div_ceil(2),
        8 => n,
        _ => return Err(CascError::Bad("BLP alpha depth")),
    };
    let indices = mip.get(..n).ok_or(CascError::Bad("BLP mip"))?;
    let alpha = mip.get(n..n + alpha_len).ok_or(CascError::Bad("BLP mip"))?;
    let mut out = Vec::with_capacity(n * 4);
    for (i, &index) in indices.iter().enumerate() {
        let p = &palette[usize::from(index) * 4..][..4];
        let a = match depth {
            0 => 255,
            1 => ((alpha[i / 8] >> (i % 8)) & 1) * 255,
            4 => ((alpha[i / 2] >> ((i % 2) * 4)) & 0xF) * 17,
            _ => alpha[i],
        };
        out.extend([p[2], p[1], p[0], a]);
    }
    Ok(out)
}

fn dxt(mip: &[u8], depth: u8, alpha_encoding: u8, w: usize, h: usize) -> CascResult<Vec<u8>> {
    let block_len = match alpha_encoding {
        0 => 8,      // DXT1
        1 | 7 => 16, // DXT3, DXT5
        _ => return Err(CascError::Bad("BLP alpha encoding")),
    };
    let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
    let blocks = mip
        .get(..bw * bh * block_len)
        .ok_or(CascError::Bad("BLP mip"))?;
    let mut out = vec![0; w * h * 4];
    for (i, block) in blocks.chunks_exact(block_len).enumerate() {
        let mut px = [[0u8; 4]; 16];
        match alpha_encoding {
            0 => color_block(block, depth > 0, &mut px),
            1 => {
                color_block(&block[8..], false, &mut px);
                for (j, p) in px.iter_mut().enumerate() {
                    p[3] = ((block[j / 2] >> ((j % 2) * 4)) & 0xF) * 17;
                }
            }
            _ => {
                color_block(&block[8..], false, &mut px);
                dxt5_alpha(&block[..8], &mut px);
            }
        }
        let (x0, y0) = ((i % bw) * 4, (i / bw) * 4);
        for (j, p) in px.iter().enumerate() {
            let (x, y) = (x0 + j % 4, y0 + j / 4);
            if x < w && y < h {
                out[(y * w + x) * 4..][..4].copy_from_slice(p);
            }
        }
    }
    Ok(out)
}

fn rgb565(c: u16) -> [u8; 3] {
    let r = (c >> 11) & 0x1F;
    let g = (c >> 5) & 0x3F;
    let b = c & 0x1F;
    [
        ((r << 3) | (r >> 2)) as u8,
        ((g << 2) | (g >> 4)) as u8,
        ((b << 3) | (b >> 2)) as u8,
    ]
}

/// The 8-byte color half of a DXT block. `punch` lets DXT1's 3-color mode
/// make its fourth color transparent.
fn color_block(block: &[u8], punch: bool, px: &mut [[u8; 4]; 16]) {
    let c0 = u16::from_le_bytes([block[0], block[1]]);
    let c1 = u16::from_le_bytes([block[2], block[3]]);
    let (a, b) = (rgb565(c0), rgb565(c1));
    let mix = |wa: u16, wb: u16, d: u16| -> [u8; 4] {
        let ch = |i: usize| ((u16::from(a[i]) * wa + u16::from(b[i]) * wb) / d) as u8;
        [ch(0), ch(1), ch(2), 255]
    };
    let colors = if c0 > c1 {
        [mix(1, 0, 1), mix(0, 1, 1), mix(2, 1, 3), mix(1, 2, 3)]
    } else {
        let last = if punch { [0, 0, 0, 0] } else { [0, 0, 0, 255] };
        [mix(1, 0, 1), mix(0, 1, 1), mix(1, 1, 2), last]
    };
    let bits = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
    for (j, p) in px.iter_mut().enumerate() {
        *p = colors[((bits >> (j * 2)) & 3) as usize];
    }
}

fn dxt5_alpha(block: &[u8], px: &mut [[u8; 4]; 16]) {
    let (a0, a1) = (u16::from(block[0]), u16::from(block[1]));
    let mut levels = [0u8; 8];
    levels[0] = a0 as u8;
    levels[1] = a1 as u8;
    if a0 > a1 {
        for k in 1..7u16 {
            levels[k as usize + 1] = (((7 - k) * a0 + k * a1) / 7) as u8;
        }
    } else {
        for k in 1..5u16 {
            levels[k as usize + 1] = (((5 - k) * a0 + k * a1) / 5) as u8;
        }
        levels[6] = 0;
        levels[7] = 255;
    }
    let bits = block[2..8]
        .iter()
        .rev()
        .fold(0u64, |acc, &b| (acc << 8) | u64::from(b));
    for (j, p) in px.iter_mut().enumerate() {
        p[3] = levels[((bits >> (j * 3)) & 7) as usize];
    }
}

pub fn to_png(image: &Image) -> CascResult<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|_| CascError::Bad("PNG encode"))?;
    writer
        .write_image_data(&image.rgba)
        .map_err(|_| CascError::Bad("PNG encode"))?;
    writer.finish().map_err(|_| CascError::Bad("PNG encode"))?;
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A BLP2 with one mip.
    pub fn build(
        encoding: u8,
        alpha_depth: u8,
        alpha_encoding: u8,
        w: u32,
        h: u32,
        palette: &[[u8; 4]],
        mip: &[u8],
    ) -> Vec<u8> {
        let mut out = b"BLP2".to_vec();
        out.extend(1u32.to_le_bytes());
        out.extend([encoding, alpha_depth, alpha_encoding, 0]);
        out.extend(w.to_le_bytes());
        out.extend(h.to_le_bytes());
        out.extend((HEADER as u32).to_le_bytes());
        out.extend([0; 15 * 4]);
        out.extend((mip.len() as u32).to_le_bytes());
        out.extend([0; 15 * 4]);
        for i in 0..256 {
            out.extend(palette.get(i).copied().unwrap_or_default());
        }
        out.extend(mip);
        out
    }

    #[test]
    fn palettized_with_one_bit_alpha() {
        // BGRA palette: 0 = blue, 1 = red.
        let palette = [[255, 0, 0, 0], [0, 0, 255, 0]];
        let file = build(1, 1, 0, 2, 1, &palette, &[0, 1, 0b10]);
        let img = decode(&file).unwrap();
        assert_eq!(img.rgba, [0, 0, 255, 0, 255, 0, 0, 255]);
    }

    #[test]
    fn argb_is_reordered_to_rgba() {
        let file = build(3, 8, 0, 1, 1, &[], &[1, 2, 3, 4]);
        assert_eq!(decode(&file).unwrap().rgba, [3, 2, 1, 4]);
    }

    #[test]
    fn dxt1_solid_block() {
        // c0 = pure red (0xF800) > c1 = 0, all indices 0.
        let block = [0x00, 0xF8, 0, 0, 0, 0, 0, 0];
        let img = decode(&build(2, 0, 0, 4, 4, &[], &block)).unwrap();
        assert!(img.rgba.chunks(4).all(|p| p == [255, 0, 0, 255]));
    }

    #[test]
    fn dxt1_punch_through_is_transparent() {
        // c0 <= c1 with index 3 everywhere: transparent when there's alpha.
        let block = [0, 0, 0x00, 0xF8, 0xFF, 0xFF, 0xFF, 0xFF];
        let img = decode(&build(2, 1, 0, 4, 4, &[], &block)).unwrap();
        assert!(img.rgba.chunks(4).all(|p| p[3] == 0));
    }

    #[test]
    fn dxt5_alpha_levels_and_odd_sizes() {
        // Alpha endpoints 255/0, every index 1 (= a1 = 0); white color.
        let mut block = vec![255, 0];
        block.extend([
            0b0100_1001,
            0b1001_0010,
            0b0010_0100,
            0b0100_1001,
            0b1001_0010,
            0b0010_0100,
        ]);
        block.extend([0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0]);
        // 3x2: one block, clipped.
        let img = decode(&build(2, 8, 7, 3, 2, &[], &block)).unwrap();
        assert_eq!(img.rgba.len(), 3 * 2 * 4);
        assert!(img.rgba.chunks(4).all(|p| p == [255, 255, 255, 0]));
    }

    #[test]
    fn a_mip_past_the_end_or_a_huge_size_is_refused() {
        let file = build(3, 8, 0, 2, 2, &[], &[0; 4]);
        assert!(decode(&file).is_err());
        let file = build(3, 8, 0, 100_000, 1, &[], &[0; 4]);
        assert!(matches!(decode(&file), Err(CascError::Bad("BLP size"))));
    }

    #[test]
    fn png_round_trips_its_size() {
        let img = Image {
            width: 2,
            height: 1,
            rgba: vec![1, 2, 3, 4, 5, 6, 7, 8],
        };
        let png = to_png(&img).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
    }

    proptest::proptest! {
        #[test]
        fn never_panics(enc in 0u8..5, depth in proptest::sample::select(vec![0u8, 1, 4, 8, 3]),
                        alpha in proptest::sample::select(vec![0u8, 1, 7, 2]),
                        w in 1u32..20, h in 1u32..20,
                        mip in proptest::collection::vec(proptest::num::u8::ANY, 0..2048)) {
            let _ = decode(&build(enc, depth, alpha, w, h, &[], &mip));
        }

        #[test]
        fn never_panics_on_garbage(data in proptest::collection::vec(proptest::num::u8::ANY, 0..1400)) {
            let _ = decode(&data);
        }
    }
}
