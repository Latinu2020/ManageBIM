//! Test de anduranta (manual, release): o scena federata mare generata din proiectul demo
//! (copii ale cladirii pe o grila), apoi incarcare, clash, verificari rapide, cautare si cache .bimc.
//! cargo test --release --test large_model -- --ignored --nocapture   (MB_COPIES = copii pe latura, implicit 24)
use bimrust::model::{ElemId, Model, RawModel};
use glam::DVec3;
use std::sync::Arc;
use std::time::Instant;

fn grid(raw: &RawModel, n: usize, dx: f64, dy: f64) -> RawModel {
    let mut out = RawModel { elements: Vec::with_capacity(raw.elements.len() * n * n), info: raw.info.clone(), warnings: Vec::new() };
    for i in 0..n {
        for j in 0..n {
            let off = DVec3::new(i as f64 * dx, j as f64 * dy, 0.0);
            for e in &raw.elements {
                let mut c = bimrust::model::RawElement {
                    name: e.name.clone(),
                    ifc_type: e.ifc_type.clone(),
                    guid: format!("{}_{i}_{j}", e.guid),
                    storey: e.storey.clone(),
                    props: e.props.clone(),
                    tris: e.tris.iter().map(|t| [t[0] + off, t[1] + off, t[2] + off]).collect(),
                    colors: e.colors.clone(),
                    default_hidden: e.default_hidden,
                };
                c.props.push(bimrust::model::Prop { group: "Grid".into(), name: "Block".into(), value: format!("{i}-{j}") });
                out.elements.push(c);
            }
        }
    }
    out
}

#[test]
#[ignore]
fn large_federated_scene() {
    let n: usize = std::env::var("MB_COPIES").ok().and_then(|s| s.parse().ok()).unwrap_or(24);
    let t = Instant::now();
    let arch = bimrust::load_any("samples/Block_A_Architecture_Structure.ifc", &|_, _| {}).expect("arch");
    let mep = bimrust::load_any("samples/Block_A_MEP.ifc", &|_, _| {}).expect("mep");
    println!("demo IFC parsed: {:.2} s", t.elapsed().as_secs_f64());
    // marimea cladirii -> pasul grilei
    let mut lo = DVec3::splat(f64::MAX);
    let mut hi = DVec3::splat(f64::MIN);
    for e in &arch.elements {
        for tr in &e.tris {
            for p in tr {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
        }
    }
    let (dx, dy) = (hi.x - lo.x + 8.0, hi.y - lo.y + 8.0);
    let t = Instant::now();
    let (ga, gm) = (grid(&arch, n, dx, dy), grid(&mep, n, dx, dy));
    println!("generated {}x{} copies in {:.2} s", n, n, t.elapsed().as_secs_f64());
    let t = Instant::now();
    let ma = Arc::new(Model::from_raw(ga, "Arch.ifc".into(), "Arch.ifc".into(), DVec3::ZERO));
    let mm = Arc::new(Model::from_raw(gm, "MEP.ifc".into(), "MEP.ifc".into(), DVec3::ZERO));
    let load = t.elapsed().as_secs_f64();
    let elems = ma.elements.len() + mm.elements.len();
    let tris = ma.tri_total() + mm.tri_total();
    let mb = ((ma.pos.len() + mm.pos.len()) * 12 + (ma.tri_color.len() + mm.tri_color.len()) * 4) as f64 / 1e6;
    println!("models built (BVH, normals): {load:.2} s - {elems} elements, {tris} triangles, geometry ~{mb:.0} MB");

    // clash: arhitectura + structura vs instalatii, toata scena
    let models = vec![Some(ma.clone()), Some(mm.clone())];
    let a: Vec<ElemId> = (0..ma.elements.len() as u32).map(|e| ElemId { m: 0, e }).collect();
    let b: Vec<ElemId> = (0..mm.elements.len() as u32).map(|e| ElemId { m: 1, e }).collect();
    let t = Instant::now();
    let found = bimrust::clash::run(&models, &a, &b, bimrust::clash::ClashType::Hard, 0.01, false, &bimrust::clash::Progress::new());
    let clash_s = t.elapsed().as_secs_f64();
    println!("clash Arch vs MEP (hard): {} clashes in {clash_s:.2} s", found.len());

    let t = Instant::now();
    let q = bimrust::check::quick_checks(&[(0, &ma), (1, &mm)]);
    let quick_s = t.elapsed().as_secs_f64();
    println!("quick checks: {quick_s:.2} s ({})", q.iter().map(|c| format!("{} {}", c.name, c.result.failed.len())).collect::<Vec<_>>().join(", "));
    // duplicate: niciunul (fiecare copie e in alt loc), GUID-uri unice
    assert_eq!(q.iter().find(|c| c.name == "Duplicate GUID").unwrap().result.failed.len(), 0);

    // cache .bimc: scriere + citire
    let p = std::env::temp_dir().join(format!("mb_large_{}.bimc", std::process::id()));
    let t = Instant::now();
    let ps = p.to_string_lossy().to_string();
    bimrust::bimc::write(&bimrust::bimc::from_model(&ma, DVec3::ZERO), &ps).expect("write bimc");
    let w = t.elapsed().as_secs_f64();
    let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0) as f64 / 1e6;
    let t = Instant::now();
    let back = bimrust::bimc::read(&ps).expect("read bimc");
    let r = t.elapsed().as_secs_f64();
    let _ = std::fs::remove_file(&p);
    println!(".bimc Arch: write {w:.2} s, read {r:.2} s, {size:.0} MB on disk");
    assert_eq!(back.elements.len(), ma.elements.len());

    // MB_KEEP = folder: pastreaza scena ca .bimc (pentru deschidere in aplicatie)
    if let Ok(dir) = std::env::var("MB_KEEP") {
        for (m, n) in [(&ma, "Large_Arch.bimc"), (&mm, "Large_MEP.bimc")] {
            let p = std::path::Path::new(&dir).join(n).to_string_lossy().to_string();
            bimrust::bimc::write(&bimrust::bimc::from_model(m, DVec3::ZERO), &p).expect("keep bimc");
        }
    }

    // pragurile: o scena de ~100 000 elemente trebuie sa ramana utilizabila
    assert!(load < 60.0, "model build {load:.1} s");
    assert!(clash_s < 60.0, "clash {clash_s:.1} s");
    assert!(quick_s < 10.0, "quick checks {quick_s:.1} s");
}
