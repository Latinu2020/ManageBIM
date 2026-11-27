//! Headroom (clear height) and egress (evacuation distance) on a synthetic model.
use bimrust::egress::{egress, headroom, Params};
use bimrust::model::{ElemId, RawElement, RawModel};
use glam::DVec3;
use std::sync::Arc;

fn boxel(min: [f64; 3], max: [f64; 3], ty: &str, name: &str) -> RawElement {
    let (a, b) = (DVec3::from(min), DVec3::from(max));
    let c = |x: f64, y: f64, z: f64| DVec3::new(if x > 0.5 { b.x } else { a.x }, if y > 0.5 { b.y } else { a.y }, if z > 0.5 { b.z } else { a.z });
    let v = [c(0., 0., 0.), c(1., 0., 0.), c(1., 1., 0.), c(0., 1., 0.), c(0., 0., 1.), c(1., 0., 1.), c(1., 1., 1.), c(0., 1., 1.)];
    let f = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
    let tris: Vec<[DVec3; 3]> = f.iter().map(|t| [v[t[0]], v[t[1]], v[t[2]]]).collect();
    RawElement { name: name.into(), ifc_type: ty.into(), guid: name.into(), storey: "L0".into(), props: Vec::new(), colors: vec![[200, 200, 200, 255]; tris.len()], tris, default_hidden: false }
}

fn run(elements: Vec<RawElement>) -> (Vec<Option<Arc<bimrust::model::Model>>>, Vec<ElemId>, Vec<String>) {
    let names: Vec<String> = elements.iter().map(|e| e.name.clone()).collect();
    let n = elements.len();
    let m = bimrust::build_model(RawModel { elements, info: Vec::new(), warnings: Vec::new() }, "test.ifc", DVec3::ZERO);
    let ids = (0..n as u32).map(|e| ElemId { m: 0, e }).collect();
    (vec![Some(Arc::new(m))], ids, names)
}

#[test]
fn headroom_finds_low_duct_not_walls_or_furniture() {
    let (models, ids, names) = run(vec![
        boxel([0., 0., -0.2], [10., 6., 0.0], "IfcSlab", "Floor"),
        boxel([0., 0., 3.0], [10., 6., 3.2], "IfcSlab", "Slab above"),
        // wall on the floor with the door opening (lintel at 2.0 m) - not an overhead obstacle
        boxel([4.9, 0., 0.0], [5.1, 2.0, 3.0], "IfcWall", "Wall"),
        boxel([4.9, 3.0, 0.0], [5.1, 6.0, 3.0], "IfcWall", "Wall 2"),
        boxel([4.9, 2.0, 2.0], [5.1, 3.0, 3.0], "IfcWall", "Lintel wall"),
        // table (furniture) - ignored
        boxel([1., 1., 0.0], [2., 2., 0.75], "IfcFurnishingElement", "Table"),
        // duct at 1.95 m (15 cm missing) and beam at 2.3 m (ok)
        boxel([0., 4.0, 1.95], [10., 4.4, 2.25], "IfcDuctSegment", "Duct"),
        boxel([7.0, 0., 2.3], [7.3, 6., 3.0], "IfcBeam", "Beam"),
    ]);
    let r = headroom(&models, &ids, &Params::default());
    let hits: Vec<&str> = r.iter().map(|f| names[f.a.e as usize].as_str()).collect();
    assert_eq!(hits, vec!["Duct"], "{hits:?}");
    assert!((r[0].dist + 0.15).abs() < 0.02, "missing ~15 cm: {}", r[0].dist);
    assert_eq!(names[r[0].b.e as usize], "Floor");
}

#[test]
fn egress_measures_walking_distance_around_walls() {
    // corridor 60 x 4 m, exterior door only at the left end (x = 0)
    let (models, ids, names) = run(vec![
        boxel([0., 0., -0.2], [60., 4., 0.0], "IfcSlab", "Floor"),
        boxel([-0.2, 0., 0.0], [0.0, 1.5, 3.0], "IfcWall", "End wall a"),
        boxel([-0.2, 2.5, 0.0], [0.0, 4.0, 3.0], "IfcWall", "End wall b"),
        boxel([-0.2, 1.5, 0.0], [0.0, 2.5, 2.1], "IfcDoor", "Exit door"),
        boxel([60.0, 0., 0.0], [60.2, 4.0, 3.0], "IfcWall", "Far wall"),
        boxel([0., -0.2, 0.0], [60., 0.0, 3.0], "IfcWall", "Side wall a"),
        boxel([0., 4.0, 0.0], [60., 4.2, 3.0], "IfcWall", "Side wall b"),
    ]);
    let p = Params { max_travel: 45.0, ..Params::default() };
    let r = egress(&models, &ids, &p);
    assert_eq!(r.len(), 1, "one zone too far");
    assert_eq!(names[r[0].b.e as usize], "Exit door");
    // the opposite end is at ~60 m: ~15 m over the limit
    assert!(r[0].dist < -13.0 && r[0].dist > -17.0, "{}", r[0].dist);
    assert!(r[0].point.x > 55.0);
    // with a 70 m limit there is nothing anymore
    let p = Params { max_travel: 70.0, ..Params::default() };
    assert!(egress(&models, &ids, &p).is_empty());
}

#[test]
fn egress_reports_level_without_exit() {
    let (models, ids, _) = run(vec![
        boxel([0., 0., -0.2], [10., 6., 0.0], "IfcSlab", "Floor"),
        boxel([-0.2, -0.2, 0.0], [10.2, 0.0, 3.0], "IfcWall", "W1"),
        boxel([-0.2, 6.0, 0.0], [10.2, 6.2, 3.0], "IfcWall", "W2"),
        boxel([-0.2, 0.0, 0.0], [0.0, 6.0, 3.0], "IfcWall", "W3"),
        boxel([10.0, 0.0, 0.0], [10.2, 6.0, 3.0], "IfcWall", "W4"),
    ]);
    let r = egress(&models, &ids, &Params::default());
    assert_eq!(r.len(), 1, "closed room without door -> one result");
    assert!(r[0].dist <= -44.0);
}

/// The demo project (Block A): headroom and egress on the real model (manual: cargo test --test egress -- --ignored --nocapture)
#[test]
#[ignore]
fn demo_block_a() {
    let root = env!("CARGO_MANIFEST_DIR");
    let mut models = Vec::new();
    let mut ids = Vec::new();
    for (k, f) in ["Block_A_Architecture_Structure.ifc", "Block_A_MEP.ifc"].iter().enumerate() {
        let p = format!("{root}/samples/{f}");
        let raw = bimrust::load_any(&p, &|_, _| {}).expect("load");
        let m = bimrust::build_model(raw, &p, DVec3::ZERO);
        for e in 0..m.elements.len() as u32 {
            ids.push(ElemId { m: k as u32, e });
        }
        models.push(Some(Arc::new(m)));
    }
    let name = |id: ElemId| models[id.m as usize].as_ref().map(|m| format!("{} ({})", m.elements[id.e as usize].name, m.elements[id.e as usize].ifc_type)).unwrap_or_default();
    for f in headroom(&models, &ids, &Params::default()) {
        println!("headroom: {} over {} - {:.0} mm too low at {:?}", name(f.a), name(f.b), -f.dist * 1000.0, f.point);
    }
    for f in egress(&models, &ids, &Params::default()) {
        println!("egress: {} -> {} : {:.1} m over at {:?}", name(f.a), name(f.b), -f.dist, f.point);
    }
}
