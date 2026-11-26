// Iconita ManageBIM: placa albastra rotunjita (gradient) cu un cub izometric alb - acelasi semn ca logo-ul
// de pe pagina de start. Desenata din cod la orice marime (supraesantionata, margini netede) si scrisa ca
// .ico (BMP 32 bit cu alfa) + resursa Windows (.res) compilata cu rc.exe din Windows SDK.
// Inclus cu `include!` din build.rs (aplicatia), installers/setup/build.rs (installer-ul) si src/main.rs (fereastra).

/// distanta cu semn la un dreptunghi rotunjit centrat in (0.5, 0.5), jumatate de latura `h`, raza `r`
#[allow(dead_code)]
fn icon_sd_round_rect(x: f32, y: f32, h: f32, r: f32) -> f32 {
    let qx = (x - 0.5).abs() - (h - r);
    let qy = (y - 0.5).abs() - (h - r);
    let ox = qx.max(0.0);
    let oy = qy.max(0.0);
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r
}

/// punct in poligon convex (varfuri in ordine)
#[allow(dead_code)]
fn icon_in_poly(x: f32, y: f32, p: &[(f32, f32)]) -> bool {
    let mut sign = 0.0f32;
    for i in 0..p.len() {
        let (ax, ay) = p[i];
        let (bx, by) = p[(i + 1) % p.len()];
        let c = (bx - ax) * (y - ay) - (by - ay) * (x - ax);
        if c.abs() < 1e-9 {
            continue;
        }
        if sign == 0.0 {
            sign = c.signum();
        } else if c.signum() != sign {
            return false;
        }
    }
    true
}

/// o fata a cubului: poligonul (coordonate 0..1) si culoarea
type IconFace<'a> = (&'a [(f32, f32)], [f32; 3]);

/// Culoarea (RGBA, premultiplicat cu acoperirea) intr-un punct al iconitei, coordonate 0..1.
#[allow(dead_code)]
fn icon_sample(x: f32, y: f32, n: u32) -> Option<[f32; 4]> {
    // placa: la marimi mici aproape tot patratul (se vede mai bine in bara de activitati)
    let h = if n <= 24 { 0.49 } else { 0.47 };
    if icon_sd_round_rect(x, y, h, 0.22) > 0.0 {
        return None;
    }
    // gradient albastru (sus mai deschis), aceeasi nuanta ca accentul aplicatiei
    let t = y.clamp(0.0, 1.0);
    let top = [86.0, 152.0, 250.0];
    let bot = [37.0, 99.0, 222.0];
    let mut c = [top[0] + (bot[0] - top[0]) * t, top[1] + (bot[1] - top[1]) * t, top[2] + (bot[2] - top[2]) * t];
    // stralucire fina in coltul de sus (doar la marimi mari)
    if n >= 48 {
        let d = ((x - 0.3).powi(2) + (y - 0.15).powi(2)).sqrt();
        let g = (1.0 - d / 0.55).clamp(0.0, 1.0) * 0.18;
        for v in c.iter_mut() {
            *v += (255.0 - *v) * g;
        }
    }
    // cubul izometric (fete: sus alb, stanga deschis, dreapta mai inchis)
    let (cx, cy) = (0.5, 0.53);
    let s = if n <= 24 { 0.33 } else { 0.29 };
    let k = 0.866 * s;
    let top_v = (cx, cy - s);
    let ul = (cx - k, cy - s / 2.0);
    let ur = (cx + k, cy - s / 2.0);
    let ce = (cx, cy);
    let ll = (cx - k, cy + s / 2.0);
    let lr = (cx + k, cy + s / 2.0);
    let bo = (cx, cy + s);
    let faces: [IconFace; 3] = [
        (&[top_v, ur, ce, ul], [255.0, 255.0, 255.0]),
        (&[ul, ce, bo, ll], [212.0, 228.0, 255.0]),
        (&[ce, ur, lr, bo], [160.0, 193.0, 250.0]),
    ];
    for (poly, fc) in faces {
        if icon_in_poly(x, y, poly) {
            c = fc;
            break;
        }
    }
    Some([c[0], c[1], c[2], 1.0])
}

/// RGBA n x n, cu margini netezite (supraesantionare).
#[allow(dead_code)]
pub fn icon_rgba(n: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (n * n * 4) as usize];
    let ss = if n <= 64 { 6u32 } else { 3 };
    for y in 0..n {
        for x in 0..n {
            let (mut r, mut g, mut b, mut a) = (0f32, 0f32, 0f32, 0f32);
            for sy in 0..ss {
                for sx in 0..ss {
                    let fx = (x as f32 + (sx as f32 + 0.5) / ss as f32) / n as f32;
                    let fy = (y as f32 + (sy as f32 + 0.5) / ss as f32) / n as f32;
                    if let Some(c) = icon_sample(fx, fy, n) {
                        r += c[0];
                        g += c[1];
                        b += c[2];
                        a += 1.0;
                    }
                }
            }
            let i = ((y * n + x) * 4) as usize;
            if a > 0.0 {
                rgba[i] = (r / a).round() as u8;
                rgba[i + 1] = (g / a).round() as u8;
                rgba[i + 2] = (b / a).round() as u8;
                rgba[i + 3] = (a / (ss * ss) as f32 * 255.0).round() as u8;
            }
        }
    }
    rgba
}

/// Fisier .ico cu mai multe marimi (DIB 32 bit, de jos in sus, + masca AND).
#[allow(dead_code)]
pub fn ico_bytes(sizes: &[u32]) -> Vec<u8> {
    let mut images = Vec::new();
    for &n in sizes {
        let px = icon_rgba(n);
        let mut d = Vec::new();
        let row_mask = (n.div_ceil(32) * 4) as usize;
        d.extend_from_slice(&40u32.to_le_bytes());
        d.extend_from_slice(&(n as i32).to_le_bytes());
        d.extend_from_slice(&((n * 2) as i32).to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&32u16.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&((n * n * 4) + (row_mask * n as usize) as u32).to_le_bytes());
        d.extend_from_slice(&[0u8; 16]);
        for y in (0..n).rev() {
            for x in 0..n {
                let i = ((y * n + x) * 4) as usize;
                d.extend_from_slice(&[px[i + 2], px[i + 1], px[i], px[i + 3]]);
            }
        }
        d.extend(std::iter::repeat_n(0u8, row_mask * n as usize));
        images.push((n, d));
    }
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut off = 6 + 16 * images.len() as u32;
    for (n, d) in &images {
        let b = if *n >= 256 { 0u8 } else { *n as u8 };
        out.extend_from_slice(&[b, b, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(d.len() as u32).to_le_bytes());
        out.extend_from_slice(&off.to_le_bytes());
        off += d.len() as u32;
    }
    for (_, d) in &images {
        out.extend_from_slice(d);
    }
    out
}

/// Scrie managebim.ico + .rc in `out`, compileaza .res cu rc.exe; intoarce calea .res (None fara Windows SDK).
#[allow(dead_code)]
pub fn compile_icon_res(out: &std::path::Path, extra_rc: &str) -> Option<std::path::PathBuf> {
    let ico = out.join("managebim.ico");
    std::fs::write(&ico, ico_bytes(&[16, 20, 24, 32, 40, 48, 64, 128, 256])).ok()?;
    let rc = out.join("managebim.rc");
    std::fs::write(&rc, format!("1 ICON \"{}\"\n{}", ico.display().to_string().replace('\\', "\\\\"), extra_rc)).ok()?;
    let res = out.join("managebim.res");
    if let Some(rcexe) = find_rc() {
        let ok = std::process::Command::new(rcexe).arg("/nologo").arg("/fo").arg(&res).arg(&rc).status().ok()?.success();
        return ok.then_some(res);
    }
    // compilare incrucisata (MinGW): windres produce un obiect COFF pe care il primeste linker-ul
    let obj = out.join("managebim_res.o");
    let ok = std::process::Command::new("x86_64-w64-mingw32-windres").arg("-O").arg("coff").arg("-i").arg(&rc).arg("-o").arg(&obj).status().ok()?.success();
    ok.then_some(obj)
}

#[allow(dead_code)]
fn find_rc() -> Option<std::path::PathBuf> {
    let base = std::path::PathBuf::from(std::env::var_os("ProgramFiles(x86)").unwrap_or_else(|| "C:\\Program Files (x86)".into())).join("Windows Kits").join("10").join("bin");
    let mut vers: Vec<std::path::PathBuf> = std::fs::read_dir(&base).ok()?.flatten().map(|e| e.path()).filter(|p| p.join("x64").join("rc.exe").is_file()).collect();
    vers.sort();
    vers.pop().map(|p| p.join("x64").join("rc.exe"))
}
