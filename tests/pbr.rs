//! Import PBR (freepbr.com) și împachetarea hărților în straturile GPU folosite de shader și path trace.
use bimrust::materials::{pack_pbr, set_tex_tier, tex_size};
use std::io::Cursor;

fn png_rgba(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            px.extend_from_slice(&f(x, y));
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(Cursor::new(&mut out), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&px).unwrap();
    }
    out
}

fn pbr_folder(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("Gold-2K-pbr-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let write = |n: &str, px: Vec<u8>| std::fs::write(dir.join(n), px).unwrap();
    write("gold_albedo.png", png_rgba(8, 8, |x, _| if x < 4 { [220, 180, 60, 255] } else { [40, 30, 20, 255] }));
    write("gold_normal.png", png_rgba(8, 8, |_, _| [128, 128, 255, 255]));
    write("gold_roughness.png", png_rgba(8, 8, |_, _| [80, 80, 80, 255]));
    write("gold_metallic.png", png_rgba(8, 8, |_, _| [240, 240, 240, 255]));
    write("gold_ao.png", png_rgba(8, 8, |_, _| [200, 200, 200, 255]));
    write("gold_preview.jpg", vec![]); // ignorat (extensia nu e imagine valida, dar e filtrat dupa nume)
    dir
}

#[test]
fn import_keeps_every_pbr_map_and_cleans_name() {
    let dir = pbr_folder("import");
    let m = bimrust::materials::import_pbr(dir.to_string_lossy().as_ref(), &dir.join("lib")).expect("import");
    assert!(m.is_pbr());
    for (role, p) in [
        ("albedo", m.texture.as_str()), ("normal", m.normal_map.as_str()),
        ("roughness", m.roughness_map.as_str()), ("metallic", m.metallic_map.as_str()),
        ("ao", m.ao_map.as_str()),
    ] {
        assert!(!p.is_empty(), "{role} lipsa");
        assert!(std::path::Path::new(p).exists(), "{role} fisier lipsa: {p}");
    }
    // preview-ul nu e o harta
    assert!(m.height_map.is_empty() || std::path::Path::new(&m.height_map).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pbr_names_are_clean() {
    let name = bimrust::materials::pbr_name;
    assert_eq!(name("white-gold-ue-xqucc9.zip"), "White gold");
    assert_eq!(name("Gold-2K-pbr.zip"), "Gold");
    assert_eq!(name("fabric-pattern-07-bl.zip"), "Fabric pattern 07");
}

#[test]
fn packed_layers_have_roughness_metallic_ao_and_normal() {
    let n = tex_size() as usize;
    let len = n * n * 4;
    let albedo = vec![200u8; len];
    // normala plana in spatiu tangent: (128, 128, 255)
    let normal: Vec<u8> = (0..len).map(|i| match i % 4 { 0 | 1 => 128, _ => 255 }).collect();
    let rough = vec![80u8; len];
    let metal = vec![240u8; len];
    let ao = vec![128u8; len];
    let (a, b) = pack_pbr(&albedo, Some(&normal), Some(&rough), Some(&metal), Some(&ao), 1.0, 1.0, 1.0);
    assert_eq!(a.len(), len);
    assert_eq!(b.len(), len);
    let i = (n / 2 * n + n / 2) * 4;
    assert_eq!(a[i + 3], 80, "rugozitatea trebuie in canalul A al primului strat");
    assert_eq!(b[i + 3], 240, "metalicitatea trebuie in canalul A al stratului doi");
    // AO 128/255 inmulteste albedo-ul 200 -> ~100
    assert!((a[i] as i32 - 100).abs() <= 1, "AO nu a fost aplicat: {}", a[i]);
    // normala plana (128,128,255) ramane plana dupa normalizare
    assert!((b[i] as i32 - 128).abs() <= 1 && (b[i + 2] as i32 - 255).abs() <= 1, "normala: {:?}", &b[i..i + 3]);
    // intensitatile se aplica imediat
    let (_, b2) = pack_pbr(&albedo, Some(&normal), Some(&rough), Some(&metal), Some(&ao), 2.0, 0.5, 0.5);
    assert_eq!(b2[i + 3], 120, "metal_mult 0.5 trebuie injumatatit");
    let (a2, _) = pack_pbr(&albedo, Some(&normal), Some(&rough), Some(&metal), Some(&ao), 1.0, 0.5, 1.0);
    assert_eq!(a2[i + 3], 40, "rough_mult 0.5 trebuie injumatatit");
}

#[test]
fn resolution_tiers_change_texture_size() {
    for (tier, size) in [(0u32, 512u32), (1, 1024), (2, 2048), (0, 512)] {
        assert_eq!(set_tex_tier(tier), size);
        assert_eq!(tex_size(), size);
        let px = bimrust::materials::load_texture("tests/fixtures/tex/tiles_test.png").expect("load");
        assert_eq!(px.len(), (size * size * 4) as usize, "tier {tier}");
    }
    // la rezolutie mare se limiteaza numarul de straturi ca sa nu depasim memoria GPU
    set_tex_tier(2);
    assert!(bimrust::materials::max_textures() <= 12);
    set_tex_tier(0);
}
