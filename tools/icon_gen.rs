// Iconita ManageBIM (trei cuburi: albastru / portocaliu / verde) desenata din cod, la orice marime,
// si scrisa ca .ico (BMP 32 bit cu alfa) + resursa Windows (.res) compilata cu rc.exe din Windows SDK.
// Inclus cu `include!` din build.rs (aplicatia) si installers/setup/build.rs (installer-ul).

/// RGBA n x n - acelasi desen ca iconita ferestrei (src/main.rs), cu margini netezite.
#[allow(dead_code)]
pub fn icon_rgba(n: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (n * n * 4) as usize];
    let boxes: [(f32, f32, f32, [u8; 3]); 3] = [(0.38, 0.08, 0.5, [70, 140, 230]), (0.1, 0.32, 0.5, [235, 135, 45]), (0.42, 0.5, 0.44, [70, 190, 100])];
    let ss = 4u32; // supraesantionare 4x4
    for y in 0..n {
        for x in 0..n {
            let (mut r, mut g, mut b, mut a) = (0f32, 0f32, 0f32, 0f32);
            for sy in 0..ss {
                for sx in 0..ss {
                    let fx = (x as f32 + (sx as f32 + 0.5) / ss as f32) / n as f32;
                    let fy = (y as f32 + (sy as f32 + 0.5) / ss as f32) / n as f32;
                    if let Some(c) = boxes.iter().find(|(x0, y0, s, _)| fx >= *x0 && fx < x0 + s && fy >= *y0 && fy < y0 + s).map(|b| b.3) {
                        r += c[0] as f32;
                        g += c[1] as f32;
                        b += c[2] as f32;
                        a += 1.0;
                    }
                }
            }
            let i = ((y * n + x) * 4) as usize;
            if a > 0.0 {
                rgba[i] = (r / a) as u8;
                rgba[i + 1] = (g / a) as u8;
                rgba[i + 2] = (b / a) as u8;
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
        let row_mask = ((n + 31) / 32 * 4) as usize;
        d.extend_from_slice(&40u32.to_le_bytes());
        d.extend_from_slice(&(n as i32).to_le_bytes());
        d.extend_from_slice(&((n * 2) as i32).to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&32u16.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&((n * n * 4) as u32 + (row_mask * n as usize) as u32).to_le_bytes());
        d.extend_from_slice(&[0u8; 16]);
        for y in (0..n).rev() {
            for x in 0..n {
                let i = ((y * n + x) * 4) as usize;
                d.extend_from_slice(&[px[i + 2], px[i + 1], px[i], px[i + 3]]);
            }
        }
        d.extend(std::iter::repeat(0u8).take(row_mask * n as usize));
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
    let rcexe = find_rc()?;
    let res = out.join("managebim.res");
    let ok = std::process::Command::new(rcexe).arg("/nologo").arg("/fo").arg(&res).arg(&rc).status().ok()?.success();
    ok.then_some(res)
}

#[allow(dead_code)]
fn find_rc() -> Option<std::path::PathBuf> {
    let base = std::path::PathBuf::from(std::env::var_os("ProgramFiles(x86)").unwrap_or_else(|| "C:\\Program Files (x86)".into())).join("Windows Kits").join("10").join("bin");
    let mut vers: Vec<std::path::PathBuf> = std::fs::read_dir(&base).ok()?.flatten().map(|e| e.path()).filter(|p| p.join("x64").join("rc.exe").is_file()).collect();
    vers.sort();
    vers.pop().map(|p| p.join("x64").join("rc.exe"))
}
