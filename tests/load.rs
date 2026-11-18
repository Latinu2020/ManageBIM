use bimrust::*;
use std::sync::Arc;
#[test]
fn load_and_clash() {
    let p = |_: f32, _: &str| {};
    let a = load_any("tests/fixtures/demo_arhitectura.ifc", &p).unwrap();
    let origin = a.center();
    for e in &a.elements { println!("A {:30} {:22} {:10} tris={} col={:?}", e.name, e.ifc_type, e.storey, e.tris.len(), e.colors[0]); }
    println!("warn {:?}", a.warnings);
    let b = load_any("tests/fixtures/demo_instalatii.ifc", &p).unwrap();
    for e in &b.elements { println!("B {:30} {:22} tris={}", e.name, e.ifc_type, e.tris.len()); }
    let ma = Arc::new(build_model(a, "tests/fixtures/demo_arhitectura.ifc", origin));
    let mb = Arc::new(build_model(b, "tests/fixtures/demo_instalatii.ifc", origin));
    println!("bbox A {:?} B {:?}", ma.aabb, mb.aabb);
    let models = vec![Some(ma.clone()), Some(mb.clone())];
    let ia: Vec<_> = (0..ma.elements.len() as u32).map(|e| model::ElemId{m:0,e}).collect();
    let ib: Vec<_> = (0..mb.elements.len() as u32).map(|e| model::ElemId{m:1,e}).collect();
    let pr = clash::Progress::new();
    let hard = clash::run(&models, &ia, &ib, clash::ClashType::Hard, 0.001, false, &pr);
    for f in &hard { println!("HARD {} x {} d={:.3}", ma.elements[f.a.e as usize].name, mb.elements[f.b.e as usize].name, f.dist); }
    let cl = clash::run(&models, &ia, &ib, clash::ClashType::Clearance, 0.15, false, &pr);
    for f in &cl { println!("CLR {} x {} d={:.3}", ma.elements[f.a.e as usize].name, mb.elements[f.b.e as usize].name, f.dist); }
    let self_hard = clash::run(&models, &ia, &ia, clash::ClashType::Hard, 0.001, false, &pr);
    for f in &self_hard { println!("SELF {} x {} d={:.3}", ma.elements[f.a.e as usize].name, ma.elements[f.b.e as usize].name, f.dist); }
    assert!(hard.iter().any(|f| mb.elements[f.b.e as usize].name.contains("VE-01")));
    assert!(cl.iter().any(|f| mb.elements[f.b.e as usize].name.contains("DN80")));
}

#[test]
fn demo_block() {
    let p = |_: f32, _: &str| {};
    let a = load_any("samples/Block_A_Architecture_Structure.ifc", &p).unwrap();
    let b = load_any("samples/Block_A_MEP.ifc", &p).unwrap();
    println!("A {} elem, warn {:?}; B {} elem, warn {:?}", a.elements.len(), a.warnings, b.elements.len(), b.warnings);
    let o = a.center();
    let ma = std::sync::Arc::new(build_model(a, "a", o));
    let mb = std::sync::Arc::new(build_model(b, "b", o));
    println!("A bb {:?}", ma.aabb);
    let models = vec![Some(ma.clone()), Some(mb.clone())];
    let ia: Vec<_> = (0..ma.elements.len() as u32).map(|e| model::ElemId{m:0,e}).collect();
    let ib: Vec<_> = (0..mb.elements.len() as u32).map(|e| model::ElemId{m:1,e}).collect();
    let pr = clash::Progress::new();
    let hard = clash::run(&models, &ia, &ib, clash::ClashType::Hard, 0.01, false, &pr);
    println!("hard {}", hard.len());
    let cl = clash::run(&models, &ia, &ib, clash::ClashType::Clearance, 0.1, false, &pr);
    println!("clear {}", cl.len());
    let csv = std::fs::read_to_string("samples/Block_A_schedule_4D.csv").unwrap();
    let (t, e) = timeline::parse_csv(&csv);
    println!("tasks {} errs {:?}", t.len(), e);
    assert_eq!(t.len(), 16);
    assert_eq!(t[0].rule, "Level=Ground Floor;IFC Type=IfcSlab");
    assert!(hard.len() > 10);
}

#[test]
fn bimc_roundtrip() {
    let p = |_: f32, _: &str| {};
    let a = load_any("samples/Block_A_Architecture_Structure.ifc", &p).unwrap();
    let bytes = bimc::encode(&a);
    let b = bimc::decode(&bytes).unwrap();
    assert_eq!(a.elements.len(), b.elements.len());
    let mut maxerr = 0.0f64;
    for (x, y) in a.elements.iter().zip(&b.elements) {
        assert_eq!(x.name, y.name);
        assert_eq!(x.guid, y.guid);
        assert_eq!(x.props.len(), y.props.len());
        assert_eq!(x.tris.len(), y.tris.len());
        assert_eq!(x.colors, y.colors);
        for (t, u) in x.tris.iter().zip(&y.tris) {
            for k in 0..3 { maxerr = maxerr.max((t[k] - u[k]).abs().max_element()); }
        }
    }
    println!("ifc {} bytes -> bimc {} bytes, max err {:.6} m", std::fs::metadata("samples/Block_A_Architecture_Structure.ifc").unwrap().len(), bytes.len(), maxerr);
    assert!(maxerr <= 0.00005 + 1e-9);
}

#[test]
fn bimc_deflate_variant() {
    let p = |_: f32, _: &str| {};
    let a = load_any("samples/Block_A_MEP.ifc", &p).unwrap();
    let d = bimc::encode_deflate(&a);
    let b = bimc::decode(&d).unwrap();
    assert_eq!(a.elements.len(), b.elements.len());
    println!("deflate variant {} bytes, lz4 {} bytes", d.len(), bimc::encode(&a).len());
}
