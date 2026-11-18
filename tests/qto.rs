//! Quantification: cantitati geometrice exacte, cantitati IFC convertite in SI, grupare, Excel, sabloane.
use bimrust::app::*;
use bimrust::model::{ElemId, RawElement, RawModel};
use bimrust::qto::{geo_qty, QtoScope, QtoSetup, QtyColumn};
use bimrust::*;
use glam::DVec3;
use std::sync::Arc;

const ARCH: &str = "samples/Block_A_Architecture_Structure.ifc";

/// cutie (x0..x0+dx, 0..dy, 0..dz) cu fete orientate spre exterior
fn boxx(x0: f64, dx: f64, dy: f64, dz: f64) -> Vec<[DVec3; 3]> {
    let p = |x: f64, y: f64, z: f64| DVec3::new(x0 + x * dx, y * dy, z * dz);
    let quads = [
        [p(0., 0., 0.), p(0., 1., 0.), p(1., 1., 0.), p(1., 0., 0.)], // jos (normala -z)
        [p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)], // sus (+z)
        [p(0., 0., 0.), p(1., 0., 0.), p(1., 0., 1.), p(0., 0., 1.)], // -y
        [p(0., 1., 0.), p(0., 1., 1.), p(1., 1., 1.), p(1., 1., 0.)], // +y
        [p(0., 0., 0.), p(0., 0., 1.), p(0., 1., 1.), p(0., 1., 0.)], // -x
        [p(1., 0., 0.), p(1., 1., 0.), p(1., 1., 1.), p(1., 0., 1.)], // +x
    ];
    quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect()
}

fn el(name: &str, ty: &str, level: &str, tris: Vec<[DVec3; 3]>) -> RawElement {
    let n = tris.len();
    RawElement { name: name.into(), ifc_type: ty.into(), guid: format!("G-{name}"), storey: level.into(), props: Vec::new(), tris, colors: vec![[200, 200, 200, 255]; n], default_hidden: false }
}

#[test]
fn geometric_quantities_of_a_box_are_exact() {
    let raw = RawModel { elements: vec![el("B", "IfcSlab", "L1", boxx(0.0, 3.0, 2.0, 4.0))], info: Vec::new(), warnings: Vec::new() };
    let o = raw.center();
    let m = build_model(raw, "box.obj", o);
    let q = geo_qty(&m, &m.elements[0]);
    let close = |a: f64, b: f64| (a - b).abs() < 1e-3;
    assert!(close(q.volume, 24.0), "volume {}", q.volume);
    assert!(close(q.surface, 52.0), "surface {}", q.surface);
    assert!(close(q.plan, 6.0), "plan {}", q.plan);
    assert!(close(q.length, 3.0) && close(q.height, 4.0), "{q:?}");
}

#[test]
fn grouping_totals_scope_and_excel() {
    // 3 cutii: doua pe L1 (IfcWall, IfcSlab), una pe L2 (IfcWall)
    let raw = RawModel {
        elements: vec![
            el("W1", "IfcWall", "L1", boxx(0.0, 1.0, 1.0, 1.0)),
            el("S1", "IfcSlab", "L1", boxx(5.0, 2.0, 1.0, 1.0)),
            el("W2", "IfcWall", "L2", boxx(10.0, 3.0, 1.0, 1.0)),
        ],
        info: Vec::new(),
        warnings: Vec::new(),
    };
    let o = raw.center();
    let mut app = App::new(vec![]);
    app.models.push(Some(Arc::new(build_model(raw, "boxes.obj", o))));
    app.refresh_caches();
    app.qto = QtoSetup { name: "Test".into(), group_by: vec![TreeLevel::new(ITEM_CAT, "IFC Type"), TreeLevel::new(ITEM_CAT, "Level")], columns: vec![QtyColumn::Count, QtyColumn::Volume], scope: QtoScope::All };
    app.qto_update();
    let r = app.qto_result.as_ref().unwrap();
    let keys: Vec<Vec<String>> = r.rows.iter().map(|x| x.keys.clone()).collect();
    assert_eq!(keys, vec![vec!["IfcSlab", "L1"], vec!["IfcWall", "L1"], vec!["IfcWall", "L2"]]);
    assert_eq!(r.totals[0], Some(3.0));
    assert!((r.totals[1].unwrap() - 6.0).abs() < 1e-3, "{:?}", r.totals);

    // doar selectia
    app.qto.scope = QtoScope::Selection;
    app.selected.insert(ElemId { m: 0, e: 2 });
    app.qto_update();
    let r = app.qto_result.as_ref().unwrap();
    assert_eq!(r.count, 1);
    assert!((r.totals[1].unwrap() - 3.0).abs() < 1e-3);

    // Excel: zip valid (PK), CSV cu virgula zecimala
    let dir = std::env::temp_dir().join(format!("managebim_qto_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let x = dir.join("q.xlsx").to_string_lossy().to_string();
    let c = dir.join("q.csv").to_string_lossy().to_string();
    app.qto.scope = QtoScope::All;
    app.handle_path(Purpose::ExportQtoXlsx, x.clone());
    app.handle_path(Purpose::ExportQtoCsv, c.clone());
    let bytes = std::fs::read(&x).unwrap();
    assert_eq!(&bytes[..2], b"PK");
    let csv = std::fs::read_to_string(&c).unwrap();
    assert!(csv.contains("\"IfcWall\";\"L2\";1;3,000"), "{csv}");
    assert!(csv.contains("\"TOTAL\""));

    // sablon: salvat si incarcat in alta sesiune
    let t = dir.join("q.bimqto").to_string_lossy().to_string();
    app.handle_path(Purpose::ExportQtoTemplate, t.clone());
    let mut b = App::new(vec![]);
    b.handle_path(Purpose::ImportQtoTemplate, t);
    assert_eq!(b.qto, app.qto);
    // si in proiect
    let pf: ProjectFile = serde_json::from_str(&serde_json::to_string(&app.project_file()).unwrap()).unwrap();
    assert_eq!(pf.qto.unwrap(), app.qto);
}

#[test]
fn ifc_quantities_are_converted_to_si_and_summed() {
    // adaugam un IfcElementQuantity (fisierul e in milimetri) pe primul perete din exemplu
    let txt = std::fs::read_to_string(ARCH).unwrap();
    let wall = regex_first_wall(&txt);
    let end = txt.rfind("ENDSEC;").unwrap();
    let extra = format!(
        "#900001=IFCQUANTITYLENGTH('Length',$,$,6000.,$);\n#900002=IFCQUANTITYAREA('NetSideArea',$,$,15000000.,$);\n#900003=IFCQUANTITYVOLUME('NetVolume',$,$,3.6E9,$);\n#900004=IFCELEMENTQUANTITY('0QTOQTOQTOQTOQTOQTOQT0',$,'Qto_WallBaseQuantities',$,'',(#900001,#900002,#900003));\n#900005=IFCRELDEFINESBYPROPERTIES('0QTOQTOQTOQTOQTOQTOQT1',$,$,$,(#{wall}),#900004);\n"
    );
    let ifc = format!("{}{}{}", &txt[..end], extra, &txt[end..]);
    let raw = bimrust::ifc::load_ifc_bytes(ifc.into_bytes(), &|_, _| {}).unwrap();
    let w = raw.elements.iter().find(|e| e.props.iter().any(|p| p.group == "Qto_WallBaseQuantities")).expect("quantities attached");
    let get = |n: &str| w.props.iter().find(|p| p.name == n).unwrap().value.clone();
    assert_eq!(get("Length"), "6 m");
    assert_eq!(get("NetSideArea"), "15 m²");
    assert_eq!(get("NetVolume"), "3.6 m³");

    let o = raw.center();
    let mut app = App::new(vec![]);
    app.models.push(Some(Arc::new(build_model(raw, ARCH, o))));
    app.refresh_caches();
    app.qto = QtoSetup {
        name: "Walls".into(),
        group_by: vec![TreeLevel::new(ITEM_CAT, "IFC Type")],
        columns: vec![QtyColumn::Count, QtyColumn::Prop { cat: "Qto_WallBaseQuantities".into(), prop: "NetVolume".into() }],
        scope: QtoScope::All,
    };
    app.qto_update();
    let r = app.qto_result.as_ref().unwrap();
    assert_eq!(r.units[1], "m³");
    assert_eq!(r.column_title(1), "NetVolume (m³)");
    assert!((r.totals[1].unwrap() - 3.6).abs() < 1e-9);
    // celelalte elemente nu au proprietatea -> numarate ca lipsa, nu ca zero
    assert_eq!(r.missing[1] as usize, r.count - 1);
}

fn regex_first_wall(txt: &str) -> String {
    for line in txt.lines() {
        if let Some(rest) = line.strip_prefix('#') {
            if let Some((id, body)) = rest.split_once('=') {
                if body.starts_with("IFCWALL") {
                    return id.to_string();
                }
            }
        }
    }
    panic!("no wall in sample");
}
