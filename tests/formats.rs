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
/// Atributele blocului (ATTRIB) si XDATA devin proprietati; TAG da numele elementului.
#[test]
fn dxf_block_attributes_and_xdata() {
    let m = load("attribs.dxf");
    let b = m.elements.iter().find(|e| e.ifc_type == "Block").expect("block");
    assert_eq!(b.name, "PUMP P-101");
    let has = |g: &str, n: &str, v: &str| b.props.iter().any(|p| p.group == g && p.name == n && p.value == v);
    assert!(has("Attributes", "TAG", "P-101"), "{:?}", b.props);
    assert!(has("Attributes", "MANUFACTURER", "Grundfos"));
    assert!(has("XData PUMPDATA", "Flow", "12 m3/h"));
    assert!(has("XData PUMPDATA", "Value 1", "7.5"));
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
/// PLY segmentat: un element pe `segment`, proprietatile fetelor si varfurilor, comentariile antetului.
#[test]
fn ply_segments_and_properties() {
    let m = load("segmented.ply");
    assert_eq!(m.elements.len(), 2);
    let a = &m.elements[0];
    assert!(a.name.ends_with("segment 7"), "{}", a.name);
    let has = |e: &model::RawElement, g: &str, n: &str, v: &str| e.props.iter().any(|p| p.group == g && p.name == n && p.value == v);
    assert!(has(a, "PLY", "segment", "7"), "{:?}", a.props);
    assert!(has(a, "PLY face", "confidence", "0.9500"));
    assert!(has(a, "PLY header", "author", "ManageBIM test"));
    assert!(a.props.iter().any(|p| p.group == "PLY vertex" && p.name == "quality"));
    assert_eq!(a.colors[0], [255, 0, 0, 255], "face colour wins");
    assert_eq!(m.elements[1].colors[0], [0, 0, 255, 255]);
}
/// glTF cu EXT_mesh_features + EXT_structural_metadata: o plasa cu 2 obiecte -> 2 elemente cu proprietati.
#[test]
fn gltf_mesh_features_split_objects() {
    use base64::Engine;
    let mut buf: Vec<u8> = Vec::new();
    let pos: [[f32; 3]; 6] = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [5., 0., 0.], [6., 0., 0.], [5., 1., 0.]];
    for p in pos {
        for v in p {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }
    buf.extend_from_slice(&[0, 0, 0, 1, 1, 1, 0, 0]); // 72..80: feature id pe varf (+ padding)
    buf.extend_from_slice(b"WallDoor"); // 80..88
    for o in [0u32, 4, 8] {
        buf.extend_from_slice(&o.to_le_bytes()); // 88..100
    }
    let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
    let json = serde_json::json!({
        "asset": {"version": "2.0"},
        "extensionsUsed": ["EXT_mesh_features", "EXT_structural_metadata"],
        "buffers": [{"byteLength": buf.len(), "uri": format!("data:application/octet-stream;base64,{b64}")}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 72},
            {"buffer": 0, "byteOffset": 72, "byteLength": 6},
            {"buffer": 0, "byteOffset": 80, "byteLength": 8},
            {"buffer": 0, "byteOffset": 88, "byteLength": 12}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 6, "type": "VEC3", "min": [0, 0, 0], "max": [6, 1, 0]},
            {"bufferView": 1, "componentType": 5121, "count": 6, "type": "SCALAR"}
        ],
        "meshes": [{"name": "Facade", "primitives": [{
            "attributes": {"POSITION": 0, "_FEATURE_ID_0": 1},
            "extensions": {"EXT_mesh_features": {"featureIds": [{"featureCount": 2, "attribute": 0, "propertyTable": 0}]}}
        }]}],
        "nodes": [{"mesh": 0, "name": "Facade"}],
        "scenes": [{"nodes": [0]}],
        "scene": 0,
        "extensions": {"EXT_structural_metadata": {
            "schema": {"classes": {"part": {"name": "Part", "properties": {"name": {"type": "STRING"}}}}},
            "propertyTables": [{"class": "part", "count": 2, "properties": {"name": {"values": 2, "stringOffsets": 3}}}]
        }}
    });
    let dir = std::env::temp_dir().join(format!("mb_gltf_meta_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("features.gltf");
    std::fs::write(&p, serde_json::to_vec(&json).unwrap()).unwrap();
    let m = load_any(&p.to_string_lossy(), &|_, _| {}).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let names: Vec<&str> = m.elements.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.contains(&"Facade · Wall") && names.contains(&"Facade · Door"), "{names:?}");
    let door = m.elements.iter().find(|e| e.name == "Facade · Door").unwrap();
    assert!(door.props.iter().any(|p| p.group == "Part" && p.name == "name" && p.value == "Door"));
}
/// 3MF: unitati, culori din basematerials, componente transformate, metadate pe obiect si pe model.
#[test]
fn threemf_objects_materials_components() {
    let model = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
 <metadata name="Title">Bracket set</metadata>
 <resources>
  <basematerials id="1"><base name="PLA red" displaycolor="#FF0000"/><base name="Steel" displaycolor="#808080FF"/></basematerials>
  <object id="2" type="model" name="Tri" pid="1" pindex="0">
   <metadatagroup><metadata name="Supplier">ACME</metadata></metadatagroup>
   <mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1000" y="0" z="0"/><vertex x="0" y="1000" z="0"/></vertices>
   <triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh>
  </object>
  <object id="3" type="model" name="Assembly" partnumber="AS-1">
   <components><component objectid="2" transform="1 0 0 0 1 0 0 0 1 5000 0 0"/></components>
  </object>
 </resources>
 <build><item objectid="2"/><item objectid="3"/></build>
</model>"##;
    let mut z = markup::Zip::default();
    z.add("_rels/.rels", br#"<?xml version="1.0"?><Relationships><Relationship Target="/3D/3dmodel.model" Id="r0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>"#);
    z.add("3D/3dmodel.model", model.as_bytes());
    let dir = std::env::temp_dir().join(format!("mb_3mf_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("set.3mf");
    std::fs::write(&p, z.finish()).unwrap();
    let m = load_any(&p.to_string_lossy(), &|_, _| {}).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(m.elements.len(), 2);
    let tri = &m.elements[0];
    assert_eq!(tri.name, "Tri");
    assert!((size(tri).x - 1.0).abs() < 1e-9, "mm -> m");
    assert_eq!(tri.colors[0], [255, 0, 0, 255]);
    assert!(tri.props.iter().any(|p| p.name == "Supplier" && p.value == "ACME"));
    assert!(tri.props.iter().any(|p| p.name == "Title" && p.value == "Bracket set"));
    let asm = &m.elements[1];
    assert!(asm.props.iter().any(|p| p.name == "Part number" && p.value == "AS-1"));
    assert!((asm.tris[0][0].x - 5.0).abs() < 1e-9, "component transform");
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

/// Timpii pe etape la incarcarea unui DWG mare (rulat manual): MB_PROFILE_FILE=cale cargo test --release --test formats profile_load -- --ignored --nocapture
#[test]
#[ignore]
fn profile_load() {
    let Ok(p) = std::env::var("MB_PROFILE_FILE") else { return };
    let t0 = std::time::Instant::now();
    let last = std::cell::RefCell::new((String::new(), 0.0f64));
    let prog = |_f: f32, s: &str| {
        let now = t0.elapsed().as_secs_f64();
        let mut l = last.borrow_mut();
        if l.0 != s {
            if !l.0.is_empty() {
                eprintln!("{:>7.3} s  {}", now - l.1, l.0);
            }
            *l = (s.to_string(), now);
        }
    };
    let raw = bimrust::load_any(&p, &prog).unwrap();
    let l = last.borrow().clone();
    eprintln!("{:>7.3} s  {}", t0.elapsed().as_secs_f64() - l.1, l.0);
    let t1 = std::time::Instant::now();
    let m = bimrust::build_model(raw, &p, raw_center());
    eprintln!("{:>7.3} s  build model ({} elements)", t1.elapsed().as_secs_f64(), m.elements.len());
    eprintln!("total {:.3} s", t0.elapsed().as_secs_f64());
}

fn raw_center() -> glam::DVec3 {
    glam::DVec3::ZERO
}

/// Arhiva ambientCG (ex. Asphalt033_1K-JPG.zip, comprimata deflate) -> material PBR cu toate hartile.
#[test]
fn ambientcg_zip_import() {
    let src = std::path::Path::new("third_party/pbr-library/Pro Library/Asphalt033");
    if !src.exists() {
        eprintln!("bundled ambientCG textures not present, skipped");
        return;
    }
    let mut z = markup::Zip::default();
    for e in std::fs::read_dir(src).unwrap().flatten() {
        z.add(&e.file_name().to_string_lossy(), &std::fs::read(e.path()).unwrap());
    }
    z.add("Asphalt033.mtlx", b"<materialx/>");
    let d = std::env::temp_dir().join(format!("mb_acg_{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let zp = d.join("Asphalt033_1K-JPG.zip");
    std::fs::write(&zp, z.finish()).unwrap();
    let m = materials::import_pbr(&zp.to_string_lossy(), &d.join("lib")).unwrap();
    assert_eq!(m.name, "Asphalt033");
    assert!(m.texture.ends_with("_Color.jpg"), "{}", m.texture);
    assert!(m.normal_map.ends_with("_NormalGL.jpg") && m.roughness_map.ends_with("_Roughness.jpg") && m.ao_map.ends_with("_AmbientOcclusion.jpg") && m.height_map.ends_with("_Displacement.jpg"), "{m:?}");
    assert!(std::path::Path::new(&m.texture).exists());
    let _ = std::fs::remove_dir_all(&d);
}
