//! Generează o librărie PBR completă, local, fără internet și fără licențe:
//! texturi procedurale (albedo, normal, roughness, metallic, AO) pentru materialele BIM uzuale.
//!
//! Rulează: cargo run --release --bin gen_pbr_library
//! Instalează în %APPDATA%\ManageBIM\Materials\PBR Library\ și actualizează library.bimmat.
//! Scrie și o copie portabilă în Downloads (ManageBIM-PBR-Library.bimmat + .zip).

use std::io::Cursor;

const N: usize = 512;

#[derive(Clone, Copy, PartialEq)]
enum Kind { Concrete, Brick, Wood, Tile, Metal, Roof, Grass, Plaster, Glass, Paint, Stone, Fabric }

struct Def {
    name: &'static str,
    color: [u8; 3],
    gloss: u8,
    metal: u8,
    scale: f32,
    kind: Kind,
    transparency: u8,
}

fn defs() -> Vec<Def> {
    use Kind::*;
    vec![
        Def { name: "Concrete", color: [186, 186, 180], gloss: 8, metal: 0, scale: 1.6, kind: Concrete, transparency: 0 },
        Def { name: "Exposed concrete", color: [156, 156, 152], gloss: 7, metal: 0, scale: 0.8, kind: Concrete, transparency: 0 },
        Def { name: "Smooth concrete panel surface boracity", color: [200, 200, 196], gloss: 14, metal: 0, scale: 2.4, kind: Concrete, transparency: 0 },
        Def { name: "Brick red", color: [162, 78, 56], gloss: 6, metal: 0, scale: 0.5, kind: Brick, transparency: 0 },
        Def { name: "Brick yellow", color: [208, 174, 116], gloss: 6, metal: 0, scale: 0.5, kind: Brick, transparency: 0 },
        Def { name: "Plaster white", color: [242, 240, 234], gloss: 6, metal: 0, scale: 1.0, kind: Plaster, transparency: 0 },
        Def { name: "Paint grey", color: [132, 136, 142], gloss: 32, metal: 0, scale: 1.0, kind: Paint, transparency: 0 },
        Def { name: "Paint red", color: [200, 52, 42], gloss: 45, metal: 0, scale: 1.0, kind: Paint, transparency: 0 },
        Def { name: "Paint yellow", color: [230, 190, 45], gloss: 45, metal: 0, scale: 1.0, kind: Paint, transparency: 0 },
        Def { name: "Paint blue", color: [52, 100, 200], gloss: 45, metal: 0, scale: 1.0, kind: Paint, transparency: 0 },
        Def { name: "Paint green", color: [62, 160, 82], gloss: 45, metal: 0, scale: 1.0, kind: Paint, transparency: 0 },
        Def { name: "Glass clear", color: [196, 224, 240], gloss: 96, metal: 0, scale: 1.0, kind: Glass, transparency: 78 },
        Def { name: "Glass tinted", color: [92, 132, 152], gloss: 96, metal: 0, scale: 1.0, kind: Glass, transparency: 62 },
        Def { name: "Steel", color: [158, 163, 170], gloss: 88, metal: 92, scale: 1.0, kind: Metal, transparency: 0 },
        Def { name: "Aluminium", color: [208, 212, 218], gloss: 78, metal: 82, scale: 1.0, kind: Metal, transparency: 0 },
        Def { name: "Galvanized steel", color: [186, 190, 194], gloss: 58, metal: 68, scale: 0.3, kind: Metal, transparency: 0 },
        Def { name: "Copper", color: [192, 122, 76], gloss: 80, metal: 90, scale: 1.0, kind: Metal, transparency: 0 },
        Def { name: "Stainless steel", color: [200, 204, 208], gloss: 92, metal: 100, scale: 1.0, kind: Metal, transparency: 0 },
        Def { name: "Wood oak", color: [178, 130, 84], gloss: 28, metal: 0, scale: 1.0, kind: Wood, transparency: 0 },
        Def { name: "Wood dark", color: [102, 68, 44], gloss: 32, metal: 0, scale: 1.0, kind: Wood, transparency: 0 },
        Def { name: "Ceramic tiles", color: [232, 232, 226], gloss: 62, metal: 0, scale: 0.3, kind: Tile, transparency: 0 },
        Def { name: "Roof tiles", color: [158, 68, 50], gloss: 12, metal: 0, scale: 0.35, kind: Roof, transparency: 0 },
        Def { name: "Facade panels", color: [214, 217, 220], gloss: 48, metal: 24, scale: 1.2, kind: Tile, transparency: 0 },
        Def { name: "Asphalt", color: [66, 68, 72], gloss: 7, metal: 0, scale: 0.5, kind: Stone, transparency: 0 },
        Def { name: "Grass", color: [92, 146, 68], gloss: 4, metal: 0, scale: 1.0, kind: Grass, transparency: 0 },
        Def { name: "Water", color: [68, 120, 160], gloss: 98, metal: 0, scale: 2.0, kind: Glass, transparency: 38 },
        Def { name: "Fabric upholstery", color: [110, 100, 110], gloss: 4, metal: 0, scale: 0.6, kind: Fabric, transparency: 0 },
        Def { name: "Marble white", color: [226, 224, 218], gloss: 70, metal: 0, scale: 1.4, kind: Stone, transparency: 0 },
        Def { name: "Granite grey", color: [128, 128, 130], gloss: 42, metal: 0, scale: 0.9, kind: Stone, transparency: 0 },
        Def { name: "Sandstone", color: [196, 172, 132], gloss: 10, metal: 0, scale: 1.1, kind: Stone, transparency: 0 },
        Def { name: "Terracotta", color: [176, 96, 66], gloss: 12, metal: 0, scale: 0.7, kind: Brick, transparency: 0 },
        Def { name: "Gypsum board", color: [238, 236, 230], gloss: 5, metal: 0, scale: 1.6, kind: Plaster, transparency: 0 },
        Def { name: "Insulation wool", color: [222, 208, 150], gloss: 2, metal: 0, scale: 0.4, kind: Fabric, transparency: 0 },
        Def { name: "Plastic white", color: [236, 238, 240], gloss: 58, metal: 0, scale: 1.0, kind: Paint, transparency: 0 },
        Def { name: "Rubber black", color: [44, 46, 48], gloss: 10, metal: 0, scale: 0.8, kind: Concrete, transparency: 0 },
        Def { name: "Oak light", color: [204, 168, 120], gloss: 30, metal: 0, scale: 1.1, kind: Wood, transparency: 0 },
        Def { name: "Wood painted white", color: [230, 230, 226], gloss: 48, metal: 0, scale: 1.0, kind: Wood, transparency: 0 },
        Def { name: "Metal brushed dark", color: [86, 90, 96], gloss: 74, metal: 95, scale: 1.0, kind: Metal, transparency: 0 },
        Def { name: "Solar panel", color: [32, 48, 82], gloss: 92, metal: 30, scale: 0.4, kind: Tile, transparency: 0 },
        Def { name: "Concrete block", color: [172, 172, 166], gloss: 7, metal: 0, scale: 0.6, kind: Concrete, transparency: 0 },
        Def { name: "Carpet grey", color: [120, 122, 124], gloss: 3, metal: 0, scale: 0.5, kind: Fabric, transparency: 0 },
    ]
}

fn hash(x: u32, y: u32, s: u32) -> f32 {
    let mut v = x.wrapping_mul(374761393).wrapping_add(y.wrapping_mul(668265263)).wrapping_add(s.wrapping_mul(2246822519));
    v = (v ^ (v >> 13)).wrapping_mul(1274126177);
    ((v ^ (v >> 16)) & 0xffff) as f32 / 65535.0
}

fn noise(x: f32, y: f32, s: u32) -> f32 {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - ix as f32, y - iy as f32);
    let (fx, fy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let h = |a: i32, b: i32| hash((a as u32) & 0xffff, (b as u32) & 0xffff, s);
    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;
    l(l(h(ix, iy), h(ix + 1, iy), fx), l(h(ix, iy + 1), h(ix + 1, iy + 1), fx), fy)
}

fn fbm(x: f32, y: f32, s: u32) -> f32 {
    noise(x, y, s) * 0.55 + noise(x * 2.7, y * 2.7, s + 1) * 0.28 + noise(x * 6.3, y * 6.3, s + 2) * 0.17
}

/// (factor de culoare 0..1, inaltime 0..1, rugozitate 0..1)
fn surface(kind: Kind, u: f32, v: f32, s: u32, base_rough: f32) -> (f32, f32, f32) {
    let n = fbm(u * 8.0, v * 8.0, s);
    let fine = fbm(u * 40.0, v * 40.0, s + 3);
    match kind {
        Kind::Concrete | Kind::Plaster | Kind::Paint => (0.86 + 0.20 * n + 0.06 * fine, n, (base_rough + (fine - 0.5) * 0.18).clamp(0.03, 1.0)),
        Kind::Stone => {
            let veins = (fbm(u * 3.0 + fine * 0.6, v * 3.0, s + 7) * 3.0).fract();
            (0.82 + 0.24 * veins + 0.08 * n, n * 0.6 + veins * 0.3, (base_rough + (fine - 0.5) * 0.2).clamp(0.03, 1.0))
        }
        Kind::Brick => {
            let row = (v * 10.0).floor();
            let ox = if row as i32 % 2 == 0 { 0.0 } else { 0.5 };
            let bx = (u * 5.0 + ox).fract();
            let by = (v * 10.0).fract();
            let mortar = (bx < 0.035 || by < 0.09) as u8 as f32;
            let tint = 0.78 + 0.34 * hash((u * 5.0).floor() as u32, row as u32, s);
            (1.0 - mortar * 0.42 + (tint - 1.0) * (1.0 - mortar), (1.0 - mortar) * (0.7 + 0.3 * fine), (base_rough + mortar * 0.2).clamp(0.03, 1.0))
        }
        Kind::Wood => {
            let grain = (fbm(u * 2.0, v * 26.0 + fbm(u * 3.0, v * 2.0, s) * 3.0, s + 5) * 2.0).sin() * 0.5 + 0.5;
            (0.72 + 0.36 * grain + 0.06 * fine, grain, (base_rough + (grain - 0.5) * 0.25).clamp(0.03, 1.0))
        }
        Kind::Tile | Kind::Roof => {
            let tx = (u * 8.0).fract();
            let ty = (v * 8.0).fract();
            let g = (tx < 0.04 || ty < 0.04) as u8 as f32;
            let tint = 0.86 + 0.2 * hash((u * 8.0).floor() as u32, (v * 8.0).floor() as u32, s);
            (tint * (1.0 - g * 0.45), (1.0 - g) * (0.75 + 0.25 * fine), (base_rough + g * 0.15).clamp(0.03, 1.0))
        }
        Kind::Metal => {
            let brush = fbm(u * 4.0, v * 120.0, s + 9);
            (0.88 + 0.16 * brush, 0.5 + (brush - 0.5) * 0.3, (base_rough + (brush - 0.5) * 0.28).clamp(0.03, 1.0))
        }
        Kind::Grass => (0.6 + 0.55 * n + 0.2 * fine, n, (base_rough + (fine - 0.5) * 0.2).clamp(0.03, 1.0)),
        Kind::Fabric => {
            let weave = (((u * 90.0).sin() * (v * 90.0).sin()) * 0.5 + 0.5) * 0.25;
            (0.85 + 0.2 * n + weave, 0.6 + weave, (base_rough + (n - 0.5) * 0.12).clamp(0.03, 1.0))
        }
        Kind::Glass => (0.97 + 0.03 * fine, 0.5, (base_rough + (fine - 0.5) * 0.04).clamp(0.02, 1.0)),
    }
}

fn png(w: u32, h: u32, px: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(Cursor::new(&mut out), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(px).unwrap();
    out
}

fn main() {
    let lib = std::env::var("APPDATA").map(|p| std::path::PathBuf::from(p).join("ManageBIM").join("Materials"))
        .expect("APPDATA nu e setat");
    let root = lib.join("PBR Library");
    std::fs::create_dir_all(&root).expect("nu pot crea folderul librariei");
    // pornim de la zero: vechile materiale importate se elimina, raman doar cele generate aici
    let mut mats: Vec<bimrust::materials::Material> = Vec::new();
    let all = defs();

    for (di, d) in all.iter().enumerate() {
        let dir = root.join(d.name.replace([' ', '/'], "_"));
        std::fs::create_dir_all(&dir).unwrap();
        let (mut albedo, mut normal, mut rough, mut metal, mut ao) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut height = vec![0.0f32; N * N];
        let base_rough = 1.0 - d.gloss as f32 / 100.0;
        for y in 0..N {
            for x in 0..N {
                let (u, v) = (x as f32 / N as f32, y as f32 / N as f32);
                let (c, hh, r) = surface(d.kind, u, v, 17 + di as u32 * 13, base_rough);
                height[y * N + x] = hh;
                let col = |base: u8, f: f32| -> u8 { (base as f32 * f).clamp(0.0, 255.0).round() as u8 };
                albedo.extend_from_slice(&[col(d.color[0], c), col(d.color[1], c), col(d.color[2], c), 255]);
                rough.extend_from_slice(&[(r * 255.0) as u8; 4]);
                let mv = (d.metal as f32 / 100.0 * 255.0) as u8;
                metal.extend_from_slice(&[mv; 4]);
            }
        }
        // normala din gradientul inaltimii (spatiu tangent OpenGL: Z pozitiv)
        for y in 0..N {
            for x in 0..N {
                let h = |xx: usize, yy: usize| height[yy * N + xx];
                let dx = (h((x + 1) % N, y) - h((x + N - 1) % N, y)) * 2.5;
                let dy = (h(x, (y + 1) % N) - h(x, (y + N - 1) % N)) * 2.5;
                let n = glam::Vec3::new(-dx, -dy, 1.0).normalize();
                let b = |v: f32| -> u8 { ((v + 1.0) * 127.5).clamp(0.0, 255.0).round() as u8 };
                normal.extend_from_slice(&[b(n.x), b(n.y), b(n.z), 255]);
                let av = 235 + (fbm(x as f32 / 24.0, y as f32 / 24.0, 77) * 20.0) as u8;
                ao.extend_from_slice(&[av; 4]);
            }
        }
        let save = |name: &str, data: &[u8]| -> String {
            let p = dir.join(name);
            std::fs::write(&p, png(N as u32, N as u32, data)).unwrap();
            p.to_string_lossy().to_string()
        };
        let m = bimrust::materials::Material {
            name: d.name.into(), color: d.color, transparency: d.transparency, gloss: d.gloss,
            metal: d.metal, pattern: bimrust::materials::Pattern::None, scale: d.scale,
            texture: save("albedo.png", &albedo),
            normal_map: save("normal.png", &normal),
            roughness_map: save("roughness.png", &rough),
            metallic_map: save("metallic.png", &metal),
            ao_map: save("ao.png", &ao),
            height_map: String::new(),
            normal_strength: 0.65, rough_mult: 1.0, metal_mult: 1.0,
            coat: if d.kind == Kind::Glass { 0 } else { 0 }, coat_roughness: 5,
            sheen: if matches!(d.kind, Kind::Fabric) { 60 } else { 0 },
            ior: match d.kind { Kind::Glass => 1.45, _ => 1.5 },
        };
        match mats.iter_mut().find(|x| x.name == m.name) {
            Some(x) => *x = m,
            None => mats.push(m),
        }
        println!("{}", d.name);
    }

    let f = lib.join("library.bimmat");
    // backup de siguranta: vechiul fisier ramane ca .old, dar nu mai e folosit de aplicatie
    if f.exists() {
        let _ = std::fs::copy(&f, lib.join("library.bimmat.old"));
    }
    let tmp = lib.join("library.bimmat.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&mats).unwrap()).unwrap();
    std::fs::rename(&tmp, &f).unwrap();
    println!("\nInstalat: {} materiale in {}", mats.len(), f.display());

    // copie portabila in Downloads (doar fisierul .bimmat; texturile sunt in folderul de mai sus)
    if let Ok(dl) = std::env::var("USERPROFILE") {
        let p = std::path::PathBuf::from(dl).join("Downloads").join("ManageBIM-PBR-Library.bimmat");
        let _ = std::fs::write(&p, serde_json::to_string_pretty(&all.iter().filter_map(|d| mats.iter().find(|m| m.name == d.name).cloned()).collect::<Vec<_>>()).unwrap());
        println!("Copie: {}", p.display());
    }
}
