//! .bimc v3 (shared geometry): the result read must be IDENTICAL to the encoding without sharing (v2 / v1),
//! and the properties, names and colors stay those of each element.
use bimrust::bimc;
use bimrust::model::{Prop, RawElement, RawModel};
use glam::DVec3;

fn cube(at: DVec3, name: &str, color: [u8; 4], props: Vec<Prop>) -> RawElement {
    let c = |x: f64, y: f64, z: f64| at + DVec3::new(x, y, z) * 0.3;
    let v = [c(0., 0., 0.), c(1., 0., 0.), c(1., 1., 0.), c(0., 1., 0.), c(0., 0., 1.), c(1., 0., 1.), c(1., 1., 1.), c(0., 1., 1.)];
    let f = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
    let tris: Vec<[DVec3; 3]> = f.iter().map(|t| [v[t[0]], v[t[1]], v[t[2]]]).collect();
    RawElement { name: name.into(), ifc_type: "Flange".into(), guid: format!("g-{name}"), storey: "L1".into(), props, colors: vec![color; tris.len()], tris, default_hidden: false }
}

fn same(a: &RawModel, b: &RawModel) {
    assert_eq!(a.elements.len(), b.elements.len());
    for (x, y) in a.elements.iter().zip(&b.elements) {
        assert_eq!(x.name, y.name);
        assert_eq!(x.guid, y.guid);
        assert_eq!(x.colors, y.colors, "{}", x.name);
        assert_eq!(x.props.len(), y.props.len());
        for (p, q) in x.props.iter().zip(&y.props) {
            assert_eq!((&p.group, &p.name, &p.value), (&q.group, &q.name, &q.value));
        }
        assert_eq!(x.tris.len(), y.tris.len());
        for (s, t) in x.tris.iter().zip(&y.tris) {
            for k in 0..3 {
                assert!((s[k] - t[k]).length() < 1e-9, "{}: {:?} vs {:?}", x.name, s[k], t[k]);
            }
        }
    }
}

#[test]
fn shared_geometry_keeps_props_colors_and_exact_geometry() {
    let p = |v: &str| vec![Prop { group: "Plant 3D".into(), name: "Tag".into(), value: v.into() }];
    let mut els = Vec::new();
    for i in 0..40 {
        // 40 identical flanges, moved, each with a different property and some with a different color
        let col = if i % 3 == 0 { [255, 0, 0, 255] } else { [0, 120, 200, 255] };
        els.push(cube(DVec3::new(1000.0 + i as f64 * 1.7, 2000.0 - i as f64 * 0.31, 15.0 + i as f64 * 0.013), &format!("F{i}"), col, p(&format!("FL-{i}"))));
    }
    // different geometry (another size) with the same type name: must NOT be confused
    let mut big = cube(DVec3::new(5.0, 5.0, 5.0), "Big", [9, 9, 9, 255], p("X"));
    for t in big.tris.iter_mut() {
        for v in t.iter_mut() {
            *v = DVec3::new(5.0, 5.0, 5.0) + (*v - DVec3::new(5.0, 5.0, 5.0)) * 2.0;
        }
    }
    els.push(big);
    let raw = RawModel { elements: els, info: vec![("k".into(), "v".into())], warnings: vec![] };
    let v3 = bimc::encode(&raw);
    assert_eq!(u16::from_le_bytes([v3[4], v3[5]]), 3);
    let a = bimc::decode(&v3).unwrap();
    let b = bimc::decode(&bimc::encode_v2(&raw, 7)).unwrap();
    same(&a, &b);
    // ... and relative to the original (0.1 mm quantization)
    for (x, y) in a.elements.iter().zip(&raw.elements) {
        for (s, t) in x.tris.iter().zip(&y.tris) {
            for k in 0..3 {
                assert!((s[k] - t[k]).length() < 1e-4);
            }
        }
    }
    assert_eq!(a.elements[40].name, "Big");
    let size_big = (a.elements[40].tris.iter().flatten().map(|p| p.x).fold(f64::MIN, f64::max)) - 5.0;
    assert!((size_big - 0.6).abs() < 1e-6, "different geometry not shared");
    // DEFLATE (the .NET format) reads the same
    same(&bimc::decode(&bimc::encode_deflate(&raw)).unwrap(), &b);
}

#[test]
fn real_plant3d_model_roundtrip_and_smaller() {
    let dwg = std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()).join("Documents\\SampleProject\\Plant_3D_Models\\Area_2\\2-P-001.dwg");
    if !dwg.is_file() {
        eprintln!("sample project not found - skipped");
        return;
    }
    let raw = bimrust::load_any(&dwg.to_string_lossy(), &|_, _| {}).unwrap();
    let v3 = bimc::encode(&raw);
    let v2 = bimc::encode_v2(&raw, 4096);
    let a = bimc::decode(&v3).unwrap();
    let b = bimc::decode(&v2).unwrap();
    same(&a, &b);
    println!("2-P-001: v3 {} KB vs v2 {} KB", v3.len() / 1024, v2.len() / 1024);
}
