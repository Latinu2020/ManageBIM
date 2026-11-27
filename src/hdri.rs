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

pub fn read(path: &str) -> Result<Hdri, String> {
    let d = std::fs::read(path).map_err(|e| format!("Cannot read {path}: {e}"))?;
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
    let mut i = data_start;
    let mut pixels = vec![0.0f32; w * h * 3];
    let rle = w >= 8 && w < 32768;
    for y in 0..h {
        if rle && i + 4 <= d.len() && d[i] == 2 && d[i + 1] == 2 && ((d[i + 2] as usize) << 8 | d[i + 3] as usize) == w {
            i += 4;
            // each channel is encoded separately with run-length
            let mut chans = [[0u8; 8192]; 4];
            for c in 0..4 {
                let mut x = 0usize;
                while x < w {
                    if i + 2 > d.len() {
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
                        for k in 0..count {
                            if x + k < w {
                                chans[c][x + k] = v;
                            }
                        }
                    } else {
                        if i + n > d.len() {
                            return Err(format!("{path}: truncated HDR literals."));
                        }
                        chans[c][x..x + n].copy_from_slice(&d[i..i + n]);
                        i += n;
                    }
                    x += n;
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
}
