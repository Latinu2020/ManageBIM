//! Radiance RGBE (.hdr) reader, used as the environment for image-based lighting.
//! The format is simple and open (https://www.hdri-hub.com/hdriphy/hotgeneve/papers/HDRI.pdf).
//! We do not download anything: the user picks a .hdr file from the computer.

#[derive(Clone, Debug)]
pub struct Hdri {
    pub width: usize,
    pub height: usize,
    /// linear RGB, width*height*3
    pub pixels: Vec<f32>,
}

fn rgbe(r: u8, g: u8, b: u8, e: u8) -> [f32; 3] {
    if e == 0 {
        return [0.0; 3];
    }
    let k = 2.0f32.powi(e as i32 - 136);
    [(r as f32 + 0.5) * k, (g as f32 + 0.5) * k, (b as f32 + 0.5) * k]
}

/// A hostile 20-byte header used to allocate gigabytes before a single pixel was read.
const MAX_HDRI_PIXELS: usize = 32 * 1024 * 1024;

/// Parses the bytes of a Radiance HDR file (public so the fuzz target can feed it arbitrary data).
pub fn parse(d: &[u8], path: &str) -> Result<Hdri, String> {
    // dir: the line "-Y <h> +X <w>" (or the variants with +Y / -X)
    let header_end = d.windows(4).position(|w| w == b"\n\n").map(|i| i + 2).unwrap_or(0);
    // the line with dimensions may come after the two newlines: we search there too
    let scan_end = (header_end + 256).min(d.len());
    let prefix = String::from_utf8_lossy(&d[..scan_end]);
    let mut dims = None;
    let mut data_start = header_end.min(d.len());
    for line in prefix.lines() {
        let t = line.trim();
        if !t.starts_with(['+', '-']) {
            continue;
        }
        let p: Vec<&str> = t.split_whitespace().collect();
        if p.len() < 4 {
            continue;
        }
        if let (Some(h), Some(w)) = (p[1].parse::<usize>().ok(), p[3].parse::<usize>().ok()) {
            if w > 0 && h > 0 && w < 32768 && h < 32768 {
                dims = Some((w, h));
                if let Some(pos) = prefix.find(t) {
                    data_start = pos + t.len() + 1;
                }
                break;
            }
        }
    }
    let (w, h) = dims.ok_or_else(|| format!("{path}: not a Radiance .hdr file (no -Y/+X header)."))?;
    let n_floats = w.checked_mul(h).and_then(|n| n.checked_mul(3)).filter(|n| *n <= MAX_HDRI_PIXELS * 3)
        .ok_or_else(|| format!("{path}: HDR image is too large ({w} x {h})."))?;
    let rle = (8..32768).contains(&w);
    // Check the payload before allocating: RLE needs at least 4 bytes of header + 2 bytes per channel
    // run for each row; plain RGBE needs 4 bytes per pixel.
    let remaining = d.len().saturating_sub(data_start);
    let min_bytes = if rle { h.saturating_mul(12) } else { w.checked_mul(h).and_then(|n| n.checked_mul(4)).unwrap_or(usize::MAX) };
    if remaining < min_bytes {
        return Err(format!("{path}: truncated HDR data."));
    }
    let mut i = data_start;
    let mut pixels = vec![0.0f32; n_floats];
    for y in 0..h {
        if rle && i + 4 <= d.len() && d[i] == 2 && d[i + 1] == 2 && ((d[i + 2] as usize) << 8 | d[i + 3] as usize) == w {
            i += 4;
            // each channel is encoded separately with run-length; the row buffer follows the header
            // width (a fixed 8192 buffer used to panic for w = 8193..32767)
            let mut chans = vec![vec![0u8; w]; 4];
            for ch in chans.iter_mut() {
                let mut x = 0usize;
                while x < w {
                    if i >= d.len() {
                        return Err(format!("{path}: truncated HDR scanline."));
                    }
                    let n = d[i] as usize;
                    i += 1;
                    if n > 128 {
                        // run: a single repeated byte follows
                        if i >= d.len() {
                            return Err(format!("{path}: truncated HDR run."));
                        }
                        let v = d[i];
                        i += 1;
                        let count = n - 128;
                        if count == 0 || count > w - x {
                            return Err(format!("{path}: invalid HDR run length."));
                        }
                        ch[x..x + count].fill(v);
                        x += count;
                    } else {
                        if n == 0 || i + n > d.len() || n > w - x {
                            return Err(format!("{path}: invalid HDR literals."));
                        }
                        ch[x..x + n].copy_from_slice(&d[i..i + n]);
                        i += n;
                        x += n;
                    }
                }
            }
            for x in 0..w {
                let p = rgbe(chans[0][x], chans[1][x], chans[2][x], chans[3][x]);
                pixels[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&p);
            }
        } else {
            for x in 0..w {
                if i + 4 > d.len() {
                    return Err(format!("{path}: truncated HDR data."));
                }
                let p = rgbe(d[i], d[i + 1], d[i + 2], d[i + 3]);
                i += 4;
                pixels[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&p);
            }
        }
    }
    Ok(Hdri { width: w, height: h, pixels })
}

pub fn read(path: &str) -> Result<Hdri, String> {
    let d = std::fs::read(path).map_err(|e| format!("Cannot read {path}: {e}"))?;
    parse(&d, path)
}

/// Loads the environment from a .hdr (Radiance) or from a PNG / JPG photo.
/// For photos we use the same resampling as for textures and convert sRGB -> linear.
pub fn load(path: &str) -> Result<Hdri, String> {
    if path.to_ascii_lowercase().ends_with(".hdr") {
        return read(path);
    }
    let px = crate::materials::load_texture(path)?;
    let n = crate::materials::tex_size() as usize;
    let mut pixels = vec![0.0f32; n * n * 3];
    let to_lin = |b: u8| {
        let x = b as f32 / 255.0;
        if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
    };
    for i in 0..n * n {
        let q = i * 4;
        pixels[i * 3] = to_lin(px[q]);
        pixels[i * 3 + 1] = to_lin(px[q + 1]);
        pixels[i * 3 + 2] = to_lin(px[q + 2]);
    }
    Ok(Hdri { width: n, height: n, pixels })
}

/// equirectangular sampling: direction (x,y,z) -> (longitude, latitude)
pub fn sample(tex: &Hdri, dir: glam::Vec3) -> [f32; 3] {
    let d = dir.normalize_or_zero();
    let u = (d.x.atan2(d.y) / std::f32::consts::TAU + 0.5).rem_euclid(1.0);
    let v = (d.z.clamp(-1.0, 1.0).asin() / std::f32::consts::PI + 0.5).clamp(0.0, 1.0);
    let (x, y) = ((u * tex.width as f32) as usize % tex.width, ((1.0 - v) * tex.height as f32) as usize % tex.height);
    let i = (y * tex.width + x) * 3;
    [tex.pixels[i], tex.pixels[i + 1], tex.pixels[i + 2]]
}

#[cfg(test)]
mod tests {
    #[test]
    fn rgbe_decodes_exponent() {
        let c = super::rgbe(128, 128, 128, 128);
        assert!(c[0] > 0.4 && c[0] < 0.6);
        assert_eq!(c[0], c[1]);
        assert_eq!(super::rgbe(0, 0, 0, 0), [0.0; 3]);
    }

    /// a small HDR file, written in simple RGBE format (no RLE), must be read and sampled
    #[test]
    fn reads_plain_rgbe() {
        let dir = std::env::temp_dir().join("mb_hdr_test");
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("t.hdr");
        let mut b = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 2 +X 2\n".to_vec();
        for v in [[120u8, 128, 136, 128], [200, 100, 50, 128], [0, 0, 0, 0], [128, 128, 128, 128]] {
            b.extend_from_slice(&v);
        }
        std::fs::write(&p, b).unwrap();
        let h = super::read(p.to_str().unwrap()).expect("hdr");
        assert_eq!((h.width, h.height), (2, 2));
        assert!(h.pixels[0] > 0.0);
        let s = super::sample(&h, glam::Vec3::new(0.0, 0.0, 1.0));
        assert!(s.iter().all(|v| v.is_finite()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A 20-byte header claiming a huge image must be rejected before allocating gigabytes.
    #[test]
    fn huge_header_is_rejected() {
        let b = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 32767 +X 32767\n".to_vec();
        let e = super::parse(&b, "evil.hdr").expect_err("huge header");
        assert!(e.contains("too large") || e.contains("truncated"), "{e}");
    }

    fn rle_row(w: usize) -> Vec<u8> {
        let mut b = vec![2, 2, (w >> 8) as u8, w as u8];
        for _ in 0..4 {
            let mut x = 0;
            while x < w {
                let n = (w - x).min(127);
                b.push(n as u8);
                b.extend(std::iter::repeat_n(128, n));
                x += n;
            }
        }
        b
    }

    /// Radiance RLE is legal above the old fixed 8192-pixel row buffer: it must not panic.
    #[test]
    fn rle_wider_than_old_fixed_buffer() {
        let w = 9000usize;
        let mut b = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 1 +X 9000\n".to_vec();
        b.extend_from_slice(&rle_row(w));
        let h = super::parse(&b, "wide.hdr").expect("wide RLE HDR");
        assert_eq!((h.width, h.height), (9000, 1));
        assert_eq!(h.pixels.len(), w * 3);
        assert!(h.pixels.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn truncated_rle_returns_error_not_panic() {
        let mut b = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 1 +X 9000\n".to_vec();
        b.extend_from_slice(&[2, 2, 0x23, 0x28, 200, 7]);
        assert!(super::parse(&b, "short.hdr").is_err());
    }
}
