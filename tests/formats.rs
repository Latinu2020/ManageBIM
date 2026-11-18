use bimrust::*;
fn load(f: &str) -> model::RawModel {
    load_any(&format!("tests/fixtures/fmt/{f}"), &|_, _| {}).unwrap_or_else(|e| panic!("{f}: {e}"))
}
fn size(e: &model::RawElement) -> glam::DVec3 {
    let mut mn = glam::DVec3::splat(f64::INFINITY);
    let mut mx = -mn;
    for t in &e.tris { for p in t { mn = mn.min(*p); mx = mx.max(*p); } }
    mx - mn
}
#[test]
fn dxf_blocks_layers_units() {
    let m = load("test.dxf");
    let cols: Vec<_> = m.elements.iter().filter(|e| e.ifc_type == "Block").collect();
    assert_eq!(cols.len(), 3);
    assert!((size(cols[0]).z - 3.0).abs() < 1e-6, "mm -> m");
    // block on layer 0 inside an insert with colour 2 (yellow) inherits the insert colour
    assert!(cols.iter().any(|e| e.colors[0] == [235, 220, 60, 255]));
    let wall = m.elements.iter().find(|e| e.storey == "A-WALL").unwrap();
    assert!((size(wall).z - 3.0).abs() < 1e-6, "extruded polyline thickness");
    assert!(m.elements.iter().any(|e| e.storey == "M-DUCT" && e.tris.len() > 30));
}
#[test]
fn obj_mtl_yup() {
    let m = load("test.obj");
    assert_eq!(m.elements.len(), 2);
    let b = &m.elements[0];
    assert_eq!(b.name, "Box");
    assert!((size(b).z - 2.0).abs() < 1e-9, "Y-up converted to Z-up");
    assert_eq!(b.colors[0][..3], [229, 25, 25]);
    assert!(m.elements[1].colors[0][3] < 255, "MTL transparency");
}
#[test]
fn stl_ply() {
    assert_eq!(load("test_ascii.stl").elements.len(), 2);
    assert_eq!(load("test_bin.stl").elements[0].tris.len(), 2);
    let p = load("test.ply");
    assert_eq!(p.elements[0].tris.len(), 2);
    assert_eq!(p.elements[0].colors[0], [200, 40, 40, 255]);
}
#[test]
fn bimrust_obj_metadata() {
    let m = load("from_blender.obj");
    let w = &m.elements[0];
    assert_eq!(w.storey, "Level 1");
    assert!(w.props.iter().any(|p| p.name == "FireRating" && p.value == "REI 90"));
}
#[test]
fn fbx_nodes_hierarchy_materials_units() {
    let m = load("test.fbx");
    let names: Vec<&str> = m.elements.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    let red = m.elements.iter().find(|e| e.name == "Red box").expect("Red box");
    let green = m.elements.iter().find(|e| e.name == "Green box").expect("Green box");
    assert_eq!(red.tris.len(), 12, "6 quads -> 12 triangles");
    let s = size(red);
    // 2 x 1 x 1 m (in orice ordine a axelor dupa conversia Z-sus)
    let mut d = [s.x, s.y, s.z];
    d.sort_by(|a, b| a.total_cmp(b));
    assert!((d[0] - 1.0).abs() < 1e-6 && (d[1] - 1.0).abs() < 1e-6 && (d[2] - 2.0).abs() < 1e-6, "metres: {s:?}");
    assert_eq!(red.colors[0][..3], [204, 26, 26]);
    assert_eq!(green.colors[0][..3], [26, 179, 51]);
    assert_eq!(green.storey, "Frame", "parent node = group in the tree");
    let gc = green.tris.iter().flatten().fold(glam::DVec3::ZERO, |a, p| a + *p) / 36.0;
    let rc = red.tris.iter().flatten().fold(glam::DVec3::ZERO, |a, p| a + *p) / 36.0;
    assert!((gc - rc).length() > 2.0, "child transform applied");
}

#[test]
fn ifczip_opens() {
    // arhivam un IFC de test intr-un .ifczip (stored) si il deschidem
    let ifc = std::fs::read("tests/fixtures/demo_instalatii.ifc").unwrap();
    let name = b"model/demo.ifc";
    let crc = {
        let mut c = 0xFFFF_FFFFu32;
        for b in &ifc {
            c ^= *b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
            }
        }
        !c
    };
    let mut z = Vec::new();
    let le16 = |v: &mut Vec<u8>, x: u16| v.extend_from_slice(&x.to_le_bytes());
    let le32 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
    le32(&mut z, 0x0403_4b50);
    for x in [20u16, 0, 0, 0, 0] {
        le16(&mut z, x);
    }
    le32(&mut z, crc);
    le32(&mut z, ifc.len() as u32);
    le32(&mut z, ifc.len() as u32);
    le16(&mut z, name.len() as u16);
    le16(&mut z, 0);
    z.extend_from_slice(name);
    z.extend_from_slice(&ifc);
    let cd = z.len() as u32;
    le32(&mut z, 0x0201_4b50);
    for x in [20u16, 20, 0, 0, 0, 0] {
        le16(&mut z, x);
    }
    le32(&mut z, crc);
    le32(&mut z, ifc.len() as u32);
    le32(&mut z, ifc.len() as u32);
    le16(&mut z, name.len() as u16);
    for x in [0u16, 0, 0, 0] {
        le16(&mut z, x);
    }
    le32(&mut z, 0);
    le32(&mut z, 0);
    z.extend_from_slice(name);
    let cd_len = z.len() as u32 - cd;
    le32(&mut z, 0x0605_4b50);
    for x in [0u16, 0, 1, 1] {
        le16(&mut z, x);
    }
    le32(&mut z, cd_len);
    le32(&mut z, cd);
    le16(&mut z, 0);
    let p = std::env::temp_dir().join(format!("mb_test_{}.ifczip", std::process::id()));
    std::fs::write(&p, z).unwrap();
    let a = load_any(&p.to_string_lossy(), &|_, _| {}).unwrap();
    let b = load_any("tests/fixtures/demo_instalatii.ifc", &|_, _| {}).unwrap();
    assert_eq!(a.elements.len(), b.elements.len());
    let _ = std::fs::remove_file(&p);
}

#[test]
fn closed_formats_explain() {
    for f in ["x.dgn", "x.rvt", "x.blend", "x.skp"] {
        let e = match load_any(f, &|_, _| {}) { Err(e) => e, Ok(_) => panic!("{f} should fail") };
        assert!(e.contains("Export") || e.contains("export"), "{f}: {e}");
    }
}

#[test]
fn e57_point_cloud_reads_scans_points_and_colours() {
    let p = format!("{}\\Downloads\\Trimble_StSulpice-Cloud-50mm.e57", std::env::var("USERPROFILE").unwrap_or_default());
    if !std::path::Path::new(&p).exists() {
        eprintln!("sample E57 not found, skipped");
        return;
    }
    let t = std::time::Instant::now();
    let (raw, c) = bimrust::e57::read(&p, &|_, _| {}).unwrap();
    eprintln!("E57: {} scans, {} points in {:.2}s", raw.elements.len(), c.pos.len(), t.elapsed().as_secs_f32());
    assert_eq!(raw.elements.len(), 5);
    assert_eq!(c.pos.len(), 8_484_455, "every point of the 5 scans");
    assert_eq!(c.pos.len(), c.col.len());
    assert_eq!(c.scans.iter().map(|s| s.2 as usize).sum::<usize>(), c.pos.len());
    assert!(c.pos.len() > 1_000_000);
    // coordonate rezonabile (o biserica: zeci de metri) si culori reale, nu doar gri
    let far = c.pos.iter().filter(|p| p.iter().any(|v| v.abs() > 500.0)).count();
    assert!(far * 1000 < c.pos.len(), "{far} points far away");
    let coloured = c.col.iter().filter(|k| !(k[0] == k[1] && k[1] == k[2])).count();
    assert!(coloured * 2 > c.col.len(), "colours read");
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &c.pos { for i in 0..3 { lo[i] = lo[i].min(p[i]); hi[i] = hi[i].max(p[i]); } }
    eprintln!("bbox {lo:?} .. {hi:?}, base {:?}", c.base);
}
