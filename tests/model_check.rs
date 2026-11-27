use bimrust::check::{parse_ids, run, Card, Facet, Kind, Pattern, Rule, Value};
use bimrust::model::{Element, Model, Prop};

fn el(name: &str, ty: &str, props: &[(&str, &str, &str)]) -> Element {
    Element {
        name: name.into(),
        ifc_type: ty.into(),
        guid: format!("guid-{name}"),
        storey: "Level 1".into(),
        props: props.iter().map(|(g, n, v)| Prop { group: g.to_string(), name: n.to_string(), value: v.to_string() }).collect(),
        tri_start: 0,
        tri_count: 1,
        aabb: bimrust::model::Aabb::EMPTY,
        default_hidden: false,
        bvh: Default::default(),
    }
}

fn model(elements: Vec<Element>) -> Model {
    Model { name: "m.ifc".into(), path: String::new(), elements, pos: Vec::new(), tri_color: Vec::new(), aabb: bimrust::model::Aabb::EMPTY, info: Vec::new(), warnings: Vec::new(), cloud: None }
}

#[test]
fn xsd_patterns_are_anchored_and_support_classes_and_counts() {
    let p = Pattern::new(r"EI\d{2,3}").unwrap();
    assert!(p.is_match("EI60"));
    assert!(p.is_match("EI120"));
    assert!(!p.is_match("EI6"));
    assert!(!p.is_match("xEI60"));
    let q = Pattern::new("(REI|EI)[0-9]+( min)?").unwrap();
    assert!(q.is_match("REI90 min"));
    assert!(!q.is_match("R90"));
    assert!(Pattern::new("[A-Z]{2}-[^ ]+").unwrap().is_match("WL-01a"));
    assert!(Pattern::new("a*").unwrap().is_match(""));
}

#[test]
fn values_compare_leniently() {
    assert!(Value::OneOf(vec!["TRUE".into()]).matches(".T."));
    assert!(Value::OneOf(vec!["ei60".into()]).matches("EI60"));
    assert!(Value::OneOf(vec!["12".into()]).matches("12.0"));
    assert!(Value::Range { min: Some(0.1), max: Some(0.5) }.matches("0,25 m"));
    assert!(!Value::Range { min: Some(0.1), max: Some(0.5) }.matches("abc"));
}

#[test]
fn rules_find_missing_and_wrong_values() {
    let m = model(vec![
        el("W1", "IfcWall", &[("Pset_WallCommon", "FireRating", "EI60")]),
        el("W2", "IfcWall", &[("Pset_WallCommon", "FireRating", "none")]),
        el("W3", "IfcWall", &[]),
        el("D1", "IfcDoor", &[]),
    ]);
    let mut r = Rule::new("walls fire rating");
    r.applicability.push(Facet::entity(&["IFCWALL"]));
    r.requirements.push(Facet::property("Pset_WallCommon", "FireRating", Value::Pattern(r"EI\d+".into())));
    let mut no_name = Rule::new("prohibited");
    no_name.requirements.push(Facet { kind: Kind::Property, set: Value::Any, name: Value::one("FireRating"), value: Value::Any, card: Card::Prohibited, unique: false });
    let res = run(&[r, no_name], &[(0, &m)]);
    assert_eq!(res[0].checked, 3, "only walls are checked");
    assert_eq!(res[0].failed.len(), 2);
    assert!(res[0].failed.iter().any(|(id, why)| id.e == 1 && why.contains("'none'")));
    assert!(res[0].failed.iter().any(|(id, why)| id.e == 2 && why.contains("missing")));
    assert_eq!(res[1].checked, 4);
    assert_eq!(res[1].failed.len(), 2, "W1 and W2 have the prohibited property");
}

#[test]
fn ids_file_is_imported_as_rules() {
    let ids = r#"<?xml version="1.0" encoding="UTF-8"?>
<ids:ids xmlns:ids="http://standards.buildingsmart.org/IDS" xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <ids:info><ids:title>Test</ids:title></ids:info>
  <ids:specifications>
    <ids:specification name="Doors" ifcVersion="IFC4">
      <ids:applicability minOccurs="0" maxOccurs="unbounded">
        <ids:entity><ids:name><ids:simpleValue>IFCDOOR</ids:simpleValue></ids:name></ids:entity>
      </ids:applicability>
      <ids:requirements>
        <ids:property dataType="IFCLABEL" cardinality="required">
          <ids:propertySet><ids:simpleValue>Pset_DoorCommon</ids:simpleValue></ids:propertySet>
          <ids:baseName><ids:simpleValue>FireRating</ids:simpleValue></ids:baseName>
          <ids:value><xs:restriction base="xs:string"><xs:enumeration value="EI30"/><xs:enumeration value="EI60"/></xs:restriction></ids:value>
        </ids:property>
        <ids:attribute cardinality="required"><ids:name><ids:simpleValue>Name</ids:simpleValue></ids:name></ids:attribute>
        <ids:partOf relation="IFCRELAGGREGATES"><ids:entity><ids:name><ids:simpleValue>IFCBUILDINGSTOREY</ids:simpleValue></ids:name></ids:entity></ids:partOf>
      </ids:requirements>
    </ids:specification>
    <ids:specification name="Widths">
      <ids:applicability><ids:entity><ids:name><ids:simpleValue>IFCDOOR</ids:simpleValue></ids:name></ids:entity></ids:applicability>
      <ids:requirements>
        <ids:property cardinality="optional"><ids:propertySet><ids:simpleValue>Qto_DoorBaseQuantities</ids:simpleValue></ids:propertySet><ids:baseName><ids:simpleValue>Width</ids:simpleValue></ids:baseName>
          <ids:value><xs:restriction base="xs:double"><xs:minInclusive value="0.8"/><xs:maxInclusive value="2.0"/></xs:restriction></ids:value></ids:property>
      </ids:requirements>
    </ids:specification>
  </ids:specifications>
</ids:ids>"#;
    let (rules, warn) = parse_ids(ids, "test.ids").unwrap();
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].name, "Doors");
    assert_eq!(rules[0].requirements.len(), 2);
    assert_eq!(rules[0].requirements[0].value, Value::OneOf(vec!["EI30".into(), "EI60".into()]));
    assert_eq!(warn.len(), 1, "partOf is reported as not supported");
    assert_eq!(rules[1].requirements[0].card, Card::Optional);
    let m = model(vec![
        el("Door A", "IfcDoor", &[("Pset_DoorCommon", "FireRating", "EI60"), ("Qto_DoorBaseQuantities", "Width", "0.9 m")]),
        el("Door B", "IfcDoor", &[("Pset_DoorCommon", "FireRating", "EI90"), ("Qto_DoorBaseQuantities", "Width", "0.6 m")]),
        el("Door C", "IfcDoor", &[("Pset_DoorCommon", "FireRating", "EI30")]),
        el("Wall", "IfcWall", &[]),
    ]);
    let res = run(&rules, &[(0, &m)]);
    assert_eq!((res[0].checked, res[0].failed.len()), (3, 1));
    assert_eq!((res[1].checked, res[1].failed.len()), (3, 1), "only Door B has a wrong width; Door C has none (optional)");
    assert!(parse_ids("<foo/>", "x").is_err());
}

#[test]
fn web_viewer_embeds_geometry_and_properties() {
    let mut a = el("Wall 1", "IfcWall", &[("Pset_WallCommon", "FireRating", "EI60")]);
    a.tri_count = 1;
    let mut m = model(vec![a]);
    m.pos = vec![glam::Vec3::ZERO, glam::Vec3::X, glam::Vec3::Y];
    m.tri_color = vec![[200, 10, 10, 255]];
    let items: Vec<(&Model, &Element)> = m.elements.iter().map(|e| (&m, e)).collect();
    let (html, st) = bimrust::web::html(&items, "Test <model>", "2026-11-19");
    assert_eq!((st.elements, st.triangles), (1, 1));
    assert!(html.contains("Test &lt;model&gt;"), "title is escaped");
    assert!(!html.contains("__DATA__"));
    let (vc, ec, meta) = bimrust::web::read_back(&html).unwrap();
    assert_eq!((vc, ec), (3, 1));
    assert_eq!(meta["elems"][0][0], "Wall 1");
    assert_eq!(meta["elems"][0][5][0][2], "EI60");
}

#[test]
fn pbr_pack_names_and_roles() {
    assert_eq!(bimrust::materials::pbr_name(r"C:\dl\fabric-pattern-07-bl.zip"), "Fabric pattern 07");
    assert_eq!(bimrust::materials::pbr_name("Wood_Floor_2K.zip"), "Wood Floor");
}

/// exports the sample model as a web viewer (run manually: --ignored), for checking in a browser
#[test]
#[ignore]
fn export_sample_web_viewer() {
    // the .bimc cache (with the window openings already cut), if it exists; otherwise the IFC
    let p = "samples/Block_A_Architecture_Structure.ifc";
    let raw = bimrust::load_any(p, &|_, _| {}).unwrap();
    let m = bimrust::build_model(raw, p, glam::DVec3::ZERO);
    let items: Vec<(&Model, &Element)> = m.elements.iter().filter(|e| e.tri_count > 0).map(|e| (&m, e)).collect();
    let (html, _) = bimrust::web::html(&items, "Block A", "2026-11-19");
    let out = std::env::var("MB_WEB_OUT").unwrap_or_else(|_| "target/sample_web.html".into());
    std::fs::write(out, html).unwrap();
}

#[test]
fn live_link_ids_match_revit_and_navisworks_forms() {
    // element from an IFC exported from Revit: compressed GlobalId + UniqueId in the properties
    let g = bimrust::sync::guid_to_ifc("0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0").unwrap();
    let mut e = el("Wall", "IfcWall", &[("Element", "UniqueId", "abc-123-uid")]);
    e.guid = g.clone();
    let ids = bimrust::sync::ids_of(&e);
    assert!(ids.contains(&g));
    assert!(ids.contains(&"0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0".to_string()), "Navisworks / Revit export id form");
    assert!(ids.contains(&"abc-123-uid".to_string()), "Revit UniqueId");
    let sel = bimrust::sync::parse_selection("MBSEL1\nabc-123-uid|x\n").unwrap();
    assert!(sel.iter().any(|s| ids.contains(s)));
}

#[test]
fn as_built_finds_built_missing_and_unscanned() {
    use bimrust::asbuilt::{check, Occupancy, Params, Status};
    use glam::Vec3;
    // three walls (vertical 2 x 2 m squares in the XZ plane): at y = 0, y = 3 and y = 50
    let quad = |y: f32| vec![Vec3::new(0.0, y, 0.0), Vec3::new(2.0, y, 0.0), Vec3::new(2.0, y, 2.0), Vec3::new(0.0, y, 0.0), Vec3::new(2.0, y, 2.0), Vec3::new(0.0, y, 2.0)];
    let mut pos = Vec::new();
    let mut elems = Vec::new();
    for (k, y) in [0.0f32, 3.0, 50.0].iter().enumerate() {
        pos.extend(quad(*y));
        let mut e = el(&format!("W{k}"), "IfcWall", &[]);
        e.tri_start = (k * 2) as u32;
        e.tri_count = 2;
        let mut bb = bimrust::model::Aabb::EMPTY;
        bb.grow(Vec3::new(0.0, *y, 0.0));
        bb.grow(Vec3::new(2.0, *y, 2.0));
        e.aabb = bb;
        elems.push(e);
    }
    let mut m = model(elems);
    m.tri_color = vec![[200, 200, 200, 255]; 6];
    m.pos = pos;
    // the scan covers the first wall (y = 0) and the area up to y = 4 (the second wall is missing in reality)
    let mut pts = Vec::new();
    for i in 0..=40 {
        for j in 0..=40 {
            pts.push(Vec3::new(i as f32 * 0.05, 0.01, j as f32 * 0.05));
        }
    }
    pts.push(Vec3::new(0.0, 4.0, 0.0));
    pts.push(Vec3::new(2.0, 4.0, 2.0));
    let cloud = bimrust::model::PointCloud { pos: pts, col: Vec::new(), scans: Vec::new() };
    let occ = Occupancy::build(&[&cloud], 0.025);
    let items: Vec<(usize, &Model, u32)> = (0..3).map(|e| (0usize, &m, e as u32)).collect();
    let res = check(&items, &occ, Params::default());
    assert_eq!(res[0].status, Status::Built, "{:?}", res[0]);
    assert_eq!(res[1].status, Status::Missing, "{:?}", res[1]);
    assert_eq!(res[2].status, Status::NotScanned);
}

#[test]
fn pdf_sheets_are_written() {
    use bimrust::markup::{Issue, ReportItem, VpComment};
    let (w, h) = (320u32, 200u32);
    let rgb: Vec<u8> = (0..w * h).flat_map(|i| [(i % w * 255 / w) as u8, 120, (i / w * 255 / h) as u8]).collect();
    let png = bimrust::markup::encode_png(&rgb, w, h);
    let it = ReportItem {
        index: 1,
        name: "Clash C-4 / Level 2 (duct vs beam)".into(),
        description: "The supply duct crosses the beam at grid C-4. Lower the duct by 150 mm or reroute it.".into(),
        issue: Issue { guid: "g".into(), status: "Open".into(), priority: "High".into(), assigned: "MEP team".into(), author: "reviewer".into(), created: "2026-11-20T10:00:00Z".into() },
        comments: vec![VpComment { author: "reviewer".into(), date: "2026-11-20".into(), text: "Checked on site (paranteze) and ș diacritics.".into() }],
        redlines: 0,
        eye: [0.0; 3],
        dir: [1.0, 0.0, 0.0],
        up: [0.0, 0.0, 1.0],
        fov_deg: 50.0,
        ortho_height: None,
        selected_guids: Vec::new(),
        hidden_guids: Vec::new(),
        image: png,
        image_w: w,
        image_h: h,
    };
    let pdf = bimrust::pdf::sheets("Block A", std::slice::from_ref(&it), "2026-11-20", &bimrust::pdf::TitleBlock::default());
    assert!(pdf.starts_with(b"%PDF-1.4"));
    assert!(pdf.ends_with(b"%%EOF\n"));
    let s = String::from_utf8_lossy(&pdf);
    assert_eq!(s.matches("/Type /Page ").count(), 1);
    assert!(s.contains("/DCTDecode"));
    let out = std::env::var("MB_PDF_OUT").unwrap_or_else(|_| "target/test_sheets.pdf".into());
    std::fs::write(out, &pdf).unwrap();
}

#[test]
fn section_cut_dxf_has_layers_and_lines() {
    let segs = vec![("Walls".to_string(), [0.0, 0.0, 5.0, 0.0]), ("IfcDoor".to_string(), [1.0, 0.0, 2.0, 0.0]), ("Walls".to_string(), [5.0, 0.0, 5.0, 4.0])];
    let d = bimrust::app::cut_dxf(&segs);
    assert_eq!(d.matches("\nLINE\n").count(), 3);
    assert!(d.contains("\nLAYER\n2\nWalls\n") && d.contains("\nLAYER\n2\nIfcDoor\n"));
    assert!(d.trim_end().ends_with("EOF"));
    assert_eq!(bimrust::app::cad_layer("Ifc/Wall:*"), "Ifc_Wall__");
}

#[test]
fn sheet_dwg_uses_paper_scale_and_title_block() {
    let segs = vec![("Walls".to_string(), [0.0, 0.0, 30.0, 0.0]), ("Walls".to_string(), [30.0, 0.0, 30.0, 18.0])];
    let mut tb = bimrust::pdf::TitleBlock {
        company: "Acme Engineering".into(),
        project: "Block A".into(),
        ..Default::default()
    };
    // A3: 30 x 18 m fits at 1:100 (300 x 180 mm)
    let (d, s) = bimrust::app::cut_sheet_dxf(&segs, &tb, "x", "Plan - Level 1", "2026-11-20 10:00");
    assert_eq!(s, 100);
    assert!(d.contains("ACME ENGINEERING") && d.contains("Block A") && d.contains("1:100"));
    assert!(d.contains("$INSUNITS\n70\n4\n"), "sheet in millimetres");
    // A1 fits at a larger scale (1:50); a fixed scale is respected
    tb.paper = "A1".into();
    assert_eq!(bimrust::app::cut_sheet_dxf(&segs, &tb, "x", "t", "d").1, 50);
    tb.scale = 200;
    assert_eq!(bimrust::app::cut_sheet_dxf(&segs, &tb, "x", "t", "d").1, 200);
}

#[test]
fn unique_requirement_flags_duplicates() {
    let m = model(vec![
        el("D1", "IfcDoor", &[("Identity", "Mark", "101")]),
        el("D2", "IfcDoor", &[("Identity", "Mark", "102")]),
        el("D3", "IfcDoor", &[("Identity", "Mark", "101")]),
    ]);
    let mut r = Rule::new("unique marks");
    r.applicability.push(Facet::entity(&["IfcDoor"]));
    let mut f = Facet::property("", "Mark", Value::Any);
    f.unique = true;
    r.requirements.push(f);
    let res = run(&[r], &[(0, &m)]);
    assert_eq!(res[0].checked, 3);
    assert_eq!(res[0].failed.len(), 2, "D1 and D3 share Mark 101");
    assert!(res[0].failed.iter().all(|(_, w)| w.contains("not unique")));
}

#[test]
fn dwg_is_written_without_oda() {
    let segs = vec![("Walls".to_string(), [0.0, 0.0, 30.0, 0.0]), ("IfcDoor".to_string(), [30.0, 0.0, 30.0, 18.0])];
    let dir = std::path::Path::new("target");
    for (name, dxf) in [("dwg_test_model.dxf", bimrust::app::cut_dxf(&segs)), ("dwg_test_sheet.dxf", bimrust::app::cut_sheet_dxf(&segs, &bimrust::pdf::TitleBlock::default(), "Block A", "Plan - Level 1", "2026-11-20").0)] {
        let p = dir.join(name);
        std::fs::write(&p, dxf).unwrap();
        let out = bimrust::formats::dxf_to_dwg_native(&p.to_string_lossy()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.starts_with(b"AC1032"), "AutoCAD 2018 DWG header");
    }
}

/// The directly written DWG: re-read, it must have the same layers, lines, texts, units and coordinates.
#[test]
fn dwg_export_round_trip_is_exact() {
    use acadrust::entities::EntityType;
    let segs = vec![
        ("Walls".to_string(), [1000.5, 2000.25, 1030.5, 2000.25]),
        ("IfcDoor".to_string(), [1030.5, 2000.25, 1030.5, 2018.0]),
        ("Ifc_Slab".to_string(), [1000.5, 2018.0, 1030.5, 2018.0]),
    ];
    let tb = bimrust::pdf::TitleBlock {
        company: "Acme".into(),
        ..Default::default()
    };
    let cases = [
        ("rt_model.dxf", bimrust::app::cut_dxf(&segs), 6i16, 3usize, 0usize),
        ("rt_sheet.dxf", bimrust::app::cut_sheet_dxf(&segs, &tb, "Block A", "Plan - Level 1", "2026-11-20").0, 4, 3 + 4 + 4 + 4 + 5, 10),
    ];
    for (name, dxf, units, lines, texts) in cases {
        let p = std::path::Path::new("target").join(name);
        std::fs::write(&p, &dxf).unwrap();
        let out = bimrust::formats::dxf_to_dwg_native(&p.to_string_lossy()).unwrap();
        let doc = acadrust::DwgReader::from_mmap(&out).and_then(|mut r| r.read()).unwrap();
        assert_eq!(doc.header.insertion_units, units, "{name}: units");
        let mut n_lines = 0;
        let mut n_texts = 0;
        let mut first: Option<(f64, f64, f64, f64, String)> = None;
        let mut text_values = Vec::new();
        for e in doc.entities() {
            match e {
                EntityType::Line(l) => {
                    n_lines += 1;
                    if first.is_none() {
                        first = Some((l.start.x, l.start.y, l.end.x, l.end.y, l.common.layer.clone()));
                    }
                }
                EntityType::Text(t) => {
                    n_texts += 1;
                    assert!(t.height > 0.0);
                    text_values.push(t.value.clone());
                }
                _ => {}
            }
        }
        assert_eq!(n_lines, lines, "{name}: lines");
        assert_eq!(n_texts, texts, "{name}: texts {text_values:?}");
        for l in ["Walls", "IfcDoor", "Ifc_Slab"] {
            assert!(doc.layers.get(l).is_some(), "{name}: layer {l} missing");
        }
        if name == "rt_model.dxf" {
            let (x1, y1, x2, y2, layer) = first.unwrap();
            assert_eq!(layer, "Walls");
            assert!((x1 - 1000.5).abs() < 1e-6 && (y1 - 2000.25).abs() < 1e-6 && (x2 - 1030.5).abs() < 1e-6 && (y2 - 2000.25).abs() < 1e-6, "coordinates kept");
        } else {
            assert!(doc.layers.get("SHEET").is_some() && doc.layers.get("SHEET_TEXT").is_some());
            assert!(text_values.iter().any(|t| t == "ACME") && text_values.iter().any(|t| t == "Plan - Level 1") && text_values.iter().any(|t| t.starts_with("1:")));
        }
    }
}

#[test]
fn dwg_export_texts_on_layer_and_grids() {
    use acadrust::entities::EntityType;
    // the title block texts on their layer
    let segs = vec![("Walls".to_string(), [0.0, 0.0, 10.0, 0.0])];
    let p = std::path::Path::new("target").join("rt_layers.dxf");
    std::fs::write(&p, bimrust::app::cut_sheet_dxf(&segs, &bimrust::pdf::TitleBlock::default(), "P", "T", "2026-11-20").0).unwrap();
    let out = bimrust::formats::dxf_to_dwg_native(&p.to_string_lossy()).unwrap();
    let doc = acadrust::DwgReader::from_mmap(&out).and_then(|mut r| r.read()).unwrap();
    for e in doc.entities() {
        if let EntityType::Text(t) = e {
            assert_eq!(t.common.layer, "SHEET_TEXT");
        }
    }
    // the grids: lines, bubbles (circles) and centred labels
    let grids = vec![
        bimrust::app::UserGrid { name: "A".into(), a: [0.0, 0.0], b: [0.0, 20.0] },
        bimrust::app::UserGrid { name: "1".into(), a: [-2.0, 2.0], b: [30.0, 2.0] },
    ];
    let g = std::path::Path::new("target").join("rt_grids.dxf");
    std::fs::write(&g, bimrust::app::grid_dxf(&grids, 0.5)).unwrap();
    let out = bimrust::formats::dxf_to_dwg_native(&g.to_string_lossy()).unwrap();
    let doc = acadrust::DwgReader::from_mmap(&out).and_then(|mut r| r.read()).unwrap();
    let (mut l, mut c, mut t) = (0, 0, 0);
    for e in doc.entities() {
        match e {
            EntityType::Line(_) => l += 1,
            EntityType::Circle(_) => c += 1,
            EntityType::Text(x) => {
                t += 1;
                assert!(x.value == "A" || x.value == "1");
            }
            _ => {}
        }
    }
    assert_eq!((l, c, t), (2, 4, 4));
    assert!(doc.layers.get("GRID").is_some() && doc.layers.get("GRID_TAGS").is_some());
}

/// presentation PDF sheet (run manually: MB_SHEET_IMG=imagine.png ... --ignored)
#[test]
#[ignore]
fn export_demo_sheet() {
    use bimrust::markup::{Issue, ReportItem, VpComment};
    let Ok(img) = std::env::var("MB_SHEET_IMG") else { return };
    let png = std::fs::read(img).unwrap();
    let it = ReportItem {
        index: 4,
        name: "Level 2 - duct crossing beam at C-4".into(),
        description: "The supply duct DN400 crosses the concrete beam at grid C-4 / Level 2. Lower the duct by 150 mm or reroute it through the corridor. Structure to confirm whether a sleeve is possible.".into(),
        issue: Issue { guid: "g".into(), status: "Open".into(), priority: "High".into(), assigned: "MEP team".into(), author: "BIM coordinator".into(), created: "2026-11-20T10:00:00Z".into() },
        comments: vec![VpComment { author: "Structure".into(), date: "2026-11-20".into(), text: "No sleeve possible in this beam - please reroute.".into() }],
        redlines: 0,
        eye: [0.0; 3],
        dir: [1.0, 0.0, 0.0],
        up: [0.0, 0.0, 1.0],
        fov_deg: 50.0,
        ortho_height: None,
        selected_guids: Vec::new(),
        hidden_guids: Vec::new(),
        image: png,
        image_w: 0,
        image_h: 0,
    };
    let tb = bimrust::pdf::TitleBlock {
        company: "Your Company".into(),
        project: "Block A - Coordination".into(),
        client: "Client name".into(),
        drawn: "BIM coordinator".into(),
        checked: "Project lead".into(),
        number_prefix: "CO-".into(),
        ..Default::default()
    };
    let pdf = bimrust::pdf::sheets("Block A", std::slice::from_ref(&it), "2026-11-20", &tb);
    std::fs::write(std::env::var("MB_SHEET_OUT").unwrap_or_else(|_| "target/demo_sheet.pdf".into()), pdf).unwrap();
}

#[test]
fn navisworks_version_order_prefers_supported() {
    use bimrust::formats::order_years;
    // 2027 installed but unsupported: the last one is tried
    assert_eq!(order_years(&[2027, 2025, 2024], 0, None), vec![2025, 2024, 2027]);
    // the preference from settings and the version that worked last time have priority
    assert_eq!(order_years(&[2027, 2025, 2024], 2024, None), vec![2024, 2025, 2027]);
    assert_eq!(order_years(&[2027, 2025, 2024], 0, Some(2027)), vec![2027, 2025, 2024]);
    // a preference that is not installed is ignored
    assert_eq!(order_years(&[2025], 2023, None), vec![2025]);
}

#[test]
fn atomic_write_keeps_backup_and_never_half_files() {
    let dir = std::path::Path::new("target").join("atomic_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("project.bimr");
    bimrust::formats::write_atomic(&p, b"version 1", true).unwrap();
    bimrust::formats::write_atomic(&p, b"version 2", true).unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), b"version 2");
    assert_eq!(std::fs::read(bimrust::formats::backup_path(&p)).unwrap(), b"version 1");
    // no temporary file left behind
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
}

#[test]
fn undo_redo_restores_selection_and_visibility() {
    use bimrust::model::ElemId;
    let mut app = bimrust::app::App::new(Vec::new());
    let a = ElemId { m: 0, e: 1 };
    let b = ElemId { m: 0, e: 2 };
    // camera stopped long ago: the history records immediately
    app.lod_moved = std::time::Instant::now() - std::time::Duration::from_secs(5);
    app.history_tick(false);
    app.selected.insert(a);
    app.history_tick(false);
    app.hidden.insert(b);
    app.history_tick(false);
    assert!(app.can_undo());
    app.undo_view();
    assert!(app.hidden.is_empty() && app.selected.contains(&a));
    app.undo_view();
    assert!(app.selected.is_empty());
    assert!(!app.can_undo());
    app.redo_view();
    app.redo_view();
    assert!(app.hidden.contains(&b));
    assert!(!app.can_redo());
}

#[test]
fn smart_clash_kinds_joints_and_responsible() {
    use bimrust::clash_smart::*;
    let k = |t: &str, n: &str| kind_of(&el(n, t, &[]));
    assert_eq!(k("IfcBeam", "Steel Beam HEA200"), Kind::Beam, "'steel' is not a fitting");
    assert_eq!(k("IfcWallStandardCase", "Basic Wall"), Kind::Wall);
    assert_eq!(k("IfcFlowSegment", "Pipe DN100"), Kind::Pipe);
    assert_eq!(k("IfcDiscreteAccessory", "Pipe Hanger"), Kind::Support);
    assert_eq!(k("IfcCovering", "Pipe Insulation 30mm"), Kind::Insulation);
    assert!(normal_joint(Kind::Wall, Kind::Slab) && normal_joint(Kind::Support, Kind::Pipe));
    assert!(!normal_joint(Kind::Duct, Kind::Beam), "a duct through a beam is a real clash");
    let d = |t: &str, n: &str| discipline_of(&el(n, t, &[]), "model.ifc");
    assert_eq!(responsible(d("IfcBeam", "Beam"), d("IfcFlowSegment", "Duct 400x200")), Some(Discipline::Ducts));
    assert_eq!(responsible(d("IfcWall", "Wall"), d("IfcColumn", "Column")), Some(Discipline::Architecture));
    assert_eq!(system_of(&el("P", "IfcPipe", &[("Mechanical", "System Name", "Supply Water 1")])).as_deref(), Some("Supply Water 1"));
}

#[test]
fn navisworks_xml_keeps_viewpoint_folders() {
    use bimrust::markup::{navis_viewpoints_xml, parse_navis_viewpoints, NwView};
    let view = |name: &str, folder: &str| NwView {
        name: name.into(),
        guid: bimrust::markup::new_guid(),
        eye: [10.0, 5.0, 3.0],
        fwd: [1.0, 0.0, 0.0],
        up: [0.0, 0.0, 1.0],
        fov_deg: 50.0,
        ortho_height: None,
        aspect: 1.6,
        focal: 10.0,
        near: 0.1,
        far: 1000.0,
        scene_min: [0.0; 3],
        scene_max: [30.0; 3],
        section_box: None,
        comments: Vec::new(),
        redlines: Vec::new(),
        folder: folder.into(),
    };
    let views = vec![view("root view", ""), view("L2 duct", "Level 2/MEP"), view("L2 wall", "Level 2"), view("L1", "Level 1")];
    let xml = navis_viewpoints_xml(&views, "m");
    assert_eq!(xml.matches("<viewfolder ").count(), 3, "Level 2, Level 2/MEP, Level 1");
    let back = parse_navis_viewpoints(&xml, 1.6).unwrap();
    let f = |n: &str| back.iter().find(|v| v.name == n).map(|v| v.folder.clone()).unwrap();
    assert_eq!(f("root view"), "");
    assert_eq!(f("L2 duct"), "Level 2/MEP");
    assert_eq!(f("L2 wall"), "Level 2");
    assert_eq!(f("L1"), "Level 1");
}

#[test]
fn ifc_openings_are_cut_from_walls() {
    let raw = bimrust::load_any("tests/data/wall_opening.ifc", &|_, _| {}).unwrap();
    let m = bimrust::build_model(raw, "tests/data/wall_opening.ifc", glam::DVec3::ZERO);
    let wall = m.elements.iter().find(|e| e.name == "Wall 1").expect("wall");
    let o = m.aabb.min; // the model is recentred: coordinates relative to the origin
    let _ = o;
    // no wall triangle passes through the inside of the opening (x 1.9..3.1, z 0.9..2.3, in relative local coordinates)
    let tris = wall.tri_start as usize..(wall.tri_start + wall.tri_count) as usize;
    assert!(wall.tri_count > 12, "the wall is cut (a plain box has 12 triangles), got {}", wall.tri_count);
    // the area of the front face (normal -Y) drops by the opening area: 5 x 3 - 1.2 x 1.4
    let mut front = 0.0f32;
    for t in tris {
        let (a, b, c) = (m.pos[t * 3], m.pos[t * 3 + 1], m.pos[t * 3 + 2]);
        let n = (b - a).cross(c - a);
        if n.y.abs() > 0.99 * n.length() && n.y < 0.0 {
            front += n.length() * 0.5;
        }
    }
    assert!((front - (15.0 - 1.68)).abs() < 0.01, "front face area {front}");
}

#[test]
fn ifc_systems_classifications_and_georeference() {
    let raw = bimrust::load_any("tests/data/wall_opening.ifc", &|_, _| {}).unwrap();
    let m = bimrust::build_model(raw, "tests/data/wall_opening.ifc", glam::DVec3::ZERO);
    let wall = m.elements.iter().find(|e| e.name == "Wall 1").unwrap();
    let prop = |g: &str, n: &str| wall.props.iter().find(|p| p.group == g && p.name == n).map(|p| p.value.clone());
    assert_eq!(prop("Systems", "System").as_deref(), Some("Supply Air 1"));
    assert_eq!(prop("Classification", "Uniclass 2015").as_deref(), Some("EF_25_10"));
    assert!(m.info.iter().any(|(k, v)| k == "Georeference" && v.contains("E 500000.000") && v.contains("EPSG:3844")));
    // IDS: the classification and the system are checked directly
    use bimrust::check::{run, Facet, Kind, Rule, Value};
    let mut r = Rule::new("walls classified");
    r.applicability.push(Facet::entity(&["IfcWall"]));
    r.requirements.push(Facet { kind: Kind::Classification, set: Value::one("Uniclass 2015"), value: Value::Pattern("EF_.*".into()), ..Default::default() });
    let res = run(&[r], &[(0, &m)]);
    assert_eq!((res[0].checked, res[0].failed.len()), (1, 0));
}

#[test]
fn ids_export_round_trip() {
    use bimrust::check::{parse_ids, write_ids, Card, Facet, Kind, Rule, Value};
    let mut a = Rule::new("Doors fire rating & <width>");
    a.description = "Doors need a fire rating".into();
    a.applicability.push(Facet::entity(&["IfcDoor"]));
    a.requirements.push(Facet::property("Pset_DoorCommon", "FireRating", Value::OneOf(vec!["EI30".into(), "EI60".into()])));
    a.requirements.push(Facet { kind: Kind::Property, set: Value::one("Qto_DoorBaseQuantities"), name: Value::one("Width"), value: Value::Range { min: Some(0.8), max: Some(2.0) }, card: Card::Optional, unique: false });
    a.requirements.push(Facet::attribute("Name", Value::Pattern("D-[0-9]+".into())));
    let mut b = Rule::new("Everything classified");
    b.requirements.push(Facet { kind: Kind::Classification, set: Value::one("Uniclass 2015"), value: Value::Any, card: Card::Required, ..Default::default() });
    b.requirements.push(Facet { kind: Kind::Property, set: Value::Any, name: Value::one("Prohibited"), value: Value::Any, card: Card::Prohibited, unique: false });
    let xml = write_ids(&[a.clone(), b.clone()], "Test", "2026-11-20 10:00");
    assert!(xml.contains("<ids:simpleValue>IFCDOOR</ids:simpleValue>"), "entity names upper case");
    assert!(xml.contains("&amp; &lt;width&gt;"), "escaped");
    let (back, warn) = parse_ids(&xml, "x.ids").unwrap();
    assert!(warn.is_empty(), "{warn:?}");
    assert_eq!(back.len(), 2);
    assert_eq!(back[0].name, a.name);
    assert_eq!(back[0].description, a.description);
    assert_eq!(back[0].applicability[0].name, Value::OneOf(vec!["IFCDOOR".into()]));
    assert_eq!(back[0].requirements[1..], a.requirements[1..]);
    assert_eq!(back[0].requirements[0].value, a.requirements[0].value);
    assert!(back[1].applicability.is_empty(), "no conditions = every element");
    assert_eq!(back[1].requirements, b.requirements);
}

#[test]
fn dwg_overlay_reads_lines_and_circles() {
    let grids = vec![bimrust::app::UserGrid { name: "A".into(), a: [0.0, 0.0], b: [0.0, 20.0] }];
    let p = std::path::Path::new("target").join("overlay_test.dxf");
    std::fs::write(&p, bimrust::app::grid_dxf(&grids, 0.5)).unwrap();
    let (lines, scale) = bimrust::formats::cad_lines(&p.to_string_lossy()).unwrap();
    assert_eq!(scale, 1.0, "drawing in metres");
    // 1 grid line + 2 tessellated bubbles (circles)
    assert!(lines.len() > 1 + 2 * 40, "{} segments", lines.len());
    assert!(lines.iter().any(|(l, s)| l == "GRID" && (s[3] - 20.0).abs() < 1e-9));
    // a DWG written by us reads the same
    let dwg = bimrust::formats::dxf_to_dwg_native(&p.to_string_lossy()).unwrap();
    let (l2, _) = bimrust::formats::cad_lines(&dwg).unwrap();
    assert_eq!(l2.len(), lines.len());
}

/// Full plan: hatch only inside the outline, at 45 degrees.
#[test]
fn hatch_fills_only_the_cut_contour() {
    // 1 x 1 m square with a 0.4 x 0.4 m hole in the middle (wall with opening) - even / odd rule
    let sq = |a: f64, b: f64| vec![[a, a, b, a], [b, a, b, b], [b, b, a, b], [a, b, a, a]];
    let mut c = sq(0.0, 1.0);
    c.extend(sq(0.3, 0.7));
    let h = bimrust::app::hatch_lines(&c, 0.1);
    assert!(h.len() > 10, "{} hatch lines", h.len());
    for l in &h {
        let (dx, dy) = (l[2] - l[0], l[3] - l[1]);
        assert!((dx - dy).abs() < 1e-9, "45 degrees");
        for t in [0.0, 0.5, 1.0] {
            let (x, y) = (l[0] + dx * t, l[1] + dy * t);
            assert!((-1e-6..=1.0 + 1e-6).contains(&x) && (-1e-6..=1.0 + 1e-6).contains(&y), "outside the contour");
            if t == 0.5 {
                assert!(!(x > 0.3 + 1e-6 && x < 0.7 - 1e-6 && y > 0.3 + 1e-6 && y < 0.7 - 1e-6), "inside the hole");
            }
        }
    }
    assert!(bimrust::app::hatch_lines(&c, 0.0).is_empty());
    assert!(bimrust::app::cut_is_hatched("IfcWallStandardCase") && bimrust::app::cut_is_hatched("Columns") && !bimrust::app::cut_is_hatched("IfcDoor") && !bimrust::app::cut_is_hatched("IfcOpeningElement"));
}

/// Grids in plan: bubbles, a dimension chain with distances in mm and the overall dimension; levels and grids in section.
#[test]
fn plan_grids_get_bubbles_and_dimensions() {
    use bimrust::app::UserGrid;
    let g = |n: &str, a: [f64; 2], b: [f64; 2]| UserGrid { name: n.into(), a, b };
    // 3 vertical grids (1, 2, 3 at 0 / 6 / 13.5 m; the second drawn reversed) + 2 horizontal (A, B at 0 / 5 m)
    let grids = vec![g("1", [0.0, 0.0], [0.0, 20.0]), g("2", [6.0, 20.0], [6.0, 0.0]), g("3", [13.5, 0.0], [13.5, 20.0]), g("A", [0.0, 0.0], [20.0, 0.0]), g("B", [0.0, 5.0], [20.0, 5.0])];
    let (mut segs, mut texts) = (Vec::new(), Vec::new());
    bimrust::app::plan_grid_annotations(&grids, 0.1, &mut segs, &mut texts);
    let dims: Vec<&str> = texts.iter().filter(|t| t.layer == "DIMS_TEXT").map(|t| t.s.as_str()).collect();
    for d in ["6000", "7500", "13500", "5000"] {
        assert!(dims.contains(&d), "dimension {d} missing: {dims:?}");
    }
    assert_eq!(dims.len(), 4, "{dims:?}");
    let labels: Vec<&str> = texts.iter().filter(|t| t.layer == "GRID_TEXT").map(|t| t.s.as_str()).collect();
    assert_eq!(labels.len(), 5);
    assert_eq!(segs.iter().filter(|s| s.0 == "GRIDS").count(), 5 * 21, "axis line + 20-segment bubble each");
    // the dimension texts read from below or from the right
    assert!(texts.iter().all(|t| t.rot > -90.0 - 1e-6 && t.rot <= 90.0 + 1e-6));

    // section at Y = 10: grids 1, 2, 3 appear as verticals at X = 0 / 6 / 13.5; the levels as horizontal lines
    let (mut segs, mut texts) = (Vec::new(), Vec::new());
    let levels = vec![("Level 0".to_string(), 0.0), ("Level 1".to_string(), 3.5)];
    bimrust::app::section_annotations(&levels, &grids, 1, 10.0, [0.0, -0.5, 13.5, 7.0], 0.1, true, true, &mut segs, &mut texts);
    let mut xs: Vec<f64> = segs.iter().filter(|s| s.0 == "GRIDS" && (s.1[0] - s.1[2]).abs() < 1e-9 && (s.1[3] - s.1[1]) > 5.0).map(|s| s.1[0]).collect();
    xs.sort_by(|a, b| a.total_cmp(b));
    assert_eq!(xs, vec![0.0, 6.0, 13.5]);
    let lv: Vec<&str> = texts.iter().filter(|t| t.layer == "LEVELS_TEXT").map(|t| t.s.as_str()).collect();
    assert_eq!(lv, vec!["Level 0  +0.000", "Level 1  +3.500"]);
}

/// DWG with a full plan (hatch, labels, rotated dimensions): written without ODA and re-read with the texts.
#[test]
fn annotated_cut_round_trips_to_dwg() {
    use acadrust::entities::EntityType;
    use bimrust::app::{CadText, UserGrid};
    let mut segs = vec![("Walls".to_string(), [0.0, 0.0, 6.0, 0.0]), ("Walls".to_string(), [6.0, 0.0, 6.0, 0.2]), ("Walls".to_string(), [6.0, 0.2, 0.0, 0.2]), ("Walls".to_string(), [0.0, 0.2, 0.0, 0.0])];
    let own: Vec<[f64; 4]> = segs.iter().map(|s| s.1).collect();
    segs.extend(bimrust::app::hatch_lines(&own, 0.15).into_iter().map(|h| ("HATCH".to_string(), h)));
    let mut texts = vec![CadText::new("TAGS", 2.0, 0.3, 2.0, 0.0, "Basic Wall 200")];
    let grids = vec![UserGrid { name: "A".into(), a: [-1.0, 0.0], b: [7.0, 0.0] }, UserGrid { name: "B".into(), a: [-1.0, 6.0], b: [7.0, 6.0] }];
    bimrust::app::plan_grid_annotations(&grids, 0.1, &mut segs, &mut texts);
    let n_texts = texts.len();
    for (name, dxf) in [("annot_model.dxf", bimrust::app::cut_dxf_annot(&segs, &texts, 0.1)), ("annot_sheet.dxf", bimrust::app::cut_sheet_dxf_annot(&segs, &texts, &bimrust::pdf::TitleBlock::default(), "P", "Plan", "2026-11-20").0)] {
        let p = std::path::Path::new("target").join(name);
        std::fs::write(&p, &dxf).unwrap();
        let out = bimrust::formats::dxf_to_dwg_native(&p.to_string_lossy()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let doc = acadrust::DwgReader::from_mmap(&out).and_then(|mut r| r.read()).unwrap();
        let mut vals = Vec::new();
        let mut rotated = false;
        for e in doc.entities() {
            if let EntityType::Text(t) = e {
                vals.push(t.value.clone());
                rotated |= t.rotation.abs() > 0.1;
            }
        }
        assert!(vals.len() >= n_texts, "{name}: {vals:?}");
        assert!(vals.iter().any(|v| v == "Basic Wall 200") && vals.iter().any(|v| v == "6000"), "{name}: {vals:?}");
        assert!(rotated, "{name}: the dimension between vertical grids is a rotated text");
        for l in ["HATCH", "TAGS", "GRIDS", "DIMS", "DIMS_TEXT", "GRID_TEXT"] {
            assert!(doc.layers.get(l).is_some(), "{name}: layer {l}");
        }
    }
}

/// RO / EN interface: switching, untranslated texts stay in English, no duplicate keys.
#[test]
fn romanian_interface_toggle() {
    use bimrust::i18n::{set_romanian, tr, PAIRS};
    let mut keys: Vec<&str> = PAIRS.iter().map(|p| p.0).collect();
    keys.sort();
    let n = keys.len();
    keys.dedup();
    assert_eq!(keys.len(), n, "duplicate translation keys");
    set_romanian(true);
    assert_eq!(tr("Sections"), "Secțiuni");
    assert_eq!(tr("✔ Model Checker"), "✔ Verificare model");
    assert_eq!(tr("no translation here"), "no translation here");
    assert_eq!(bimrust::i18n::tr_lines("Light theme\nDark theme"), "Temă luminoasă\nTemă întunecată");
    set_romanian(false);
    assert_eq!(tr("Sections"), "Sections");
}

/// User-added parameters: written in the "User" group, replaced, deleted; the key = GUID or #index.
#[test]
fn user_params_apply_replace_and_remove() {
    use bimrust::app::{apply_user_params, user_key, UserParam};
    let raw = bimrust::load_any("tests/data/wall_opening.ifc", &|_, _| {}).expect("ifc");
    let mut m = bimrust::model::Model::from_raw(raw, "wall.ifc".into(), "tests/data/wall_opening.ifc".into(), glam::DVec3::ZERO);
    let k0 = user_key(&m.elements[0], 0);
    let p = |name: &str, value: &str| UserParam { model: "wall.ifc".into(), key: k0.clone(), name: name.into(), value: value.into(), group: "User".into() };
    apply_user_params(&mut m, &[p("Status", "Approved"), p("Zone", "Z1"), UserParam { model: "other.ifc".into(), ..p("Status", "x") }]);
    let user: Vec<(String, String)> = m.elements[0].props.iter().filter(|q| q.group == "User").map(|q| (q.name.clone(), q.value.clone())).collect();
    assert_eq!(user, vec![("Status".to_string(), "Approved".to_string()), ("Zone".to_string(), "Z1".to_string())]);
    assert!(m.elements.iter().skip(1).all(|e| e.props.iter().all(|q| q.group != "User")));
    // the second application rebuilds the group (does not duplicate it), an empty list deletes it
    apply_user_params(&mut m, &[p("Status", "Rejected")]);
    assert_eq!(m.elements[0].props.iter().filter(|q| q.group == "User").count(), 1);
    apply_user_params(&mut m, &[]);
    assert!(m.elements[0].props.iter().all(|q| q.group != "User"));
}

/// Overriding an existing IFC property (e.g. Pset_WallCommon.FireRating) touches only
/// the chosen element, and when the override is deleted the original value comes back - nothing is lost.
#[test]
fn user_override_keeps_and_restores_originals() {
    use bimrust::app::{apply_user_params_shadow, user_key, UserParam, UserShadow};
    use bimrust::model::Prop;
    let raw = bimrust::load_any("tests/data/wall_opening.ifc", &|_, _| {}).expect("ifc");
    let mut m = bimrust::model::Model::from_raw(raw, "wall.ifc".into(), "tests/data/wall_opening.ifc".into(), glam::DVec3::ZERO);
    // a second wall (a copy, another GUID) to see that it is not touched
    let mut e1 = m.elements[0].clone();
    e1.guid = "second-wall".into();
    m.elements.truncate(1);
    m.elements.push(e1);
    for (i, e) in m.elements.iter_mut().enumerate().take(2) {
        e.props.push(Prop { group: "Pset_WallCommon".into(), name: "FireRating".into(), value: format!("REI {}", 60 + 30 * i) });
    }
    let k0 = user_key(&m.elements[0], 0);
    let fr = |m: &bimrust::model::Model, i: usize| m.elements[i].props.iter().filter(|p| p.group == "Pset_WallCommon" && p.name == "FireRating").map(|p| p.value.clone()).collect::<Vec<_>>();
    let mut sh = UserShadow::default();
    let prev_of = |sh: &UserShadow| sh.applied.iter().map(|k| (k.1.clone(), k.2.clone(), k.3.clone())).collect::<std::collections::HashSet<_>>();
    let over = vec![UserParam { model: "wall.ifc".into(), key: k0.clone(), name: "FireRating".into(), value: "REI 120".into(), group: "Pset_WallCommon".into() }];
    let prev = prev_of(&sh);
    sh.applied.clear();
    apply_user_params_shadow(&mut m, &over, &prev, &mut sh);
    assert_eq!(fr(&m, 0), vec!["REI 120".to_string()]);
    assert_eq!(fr(&m, 1), vec!["REI 90".to_string()], "other elements keep their own value");
    // the second application (same state) does not duplicate and does not change the other elements
    let prev = prev_of(&sh);
    sh.applied.clear();
    apply_user_params_shadow(&mut m, &over, &prev, &mut sh);
    assert_eq!(fr(&m, 0), vec!["REI 120".to_string()]);
    assert_eq!(fr(&m, 1), vec!["REI 90".to_string()]);
    // override deleted: the original comes back
    let prev = prev_of(&sh);
    sh.applied.clear();
    apply_user_params_shadow(&mut m, &[], &prev, &mut sh);
    assert_eq!(fr(&m, 0), vec!["REI 60".to_string()]);
    assert_eq!(fr(&m, 1), vec!["REI 90".to_string()]);
}

/// Clash severity and test templates.
#[test]
fn clash_severity_and_templates() {
    use bimrust::app::{severity_of, ClashTest, SelKind, Severity};
    use bimrust::clash::ClashType;
    let s = |d: f32| severity_of(ClashType::Hard, 10.0, [10.0, 50.0], d);
    assert_eq!(s(-0.005), Severity::Minor);
    assert_eq!(s(-0.020), Severity::Major);
    assert_eq!(s(-0.080), Severity::Critical);
    let c = |d: f32| severity_of(ClashType::Clearance, 50.0, [10.0, 50.0], d);
    assert_eq!(c(0.0), Severity::Critical);
    assert_eq!(c(0.010), Severity::Major);
    assert_eq!(c(0.040), Severity::Minor);
    let t = ClashTest::templates();
    assert!(t.len() >= 6);
    assert!(t.iter().any(|(_, _, x)| x.ty == ClashType::Clearance && x.a.kind == SelKind::Discipline));
}

/// Quick checks: the duplicate and the doubled GUID are found, normal elements pass.
#[test]
fn quick_checks_find_duplicates() {
    let raw = bimrust::load_any("tests/data/wall_opening.ifc", &|_, _| {}).expect("ifc");
    let mut m = bimrust::model::Model::from_raw(raw, "w.ifc".into(), "w".into(), glam::DVec3::ZERO);
    let n = m.elements.len();
    // the same element copied (same GUID, same geometry)
    let e = m.elements.iter().find(|e| e.tri_count > 0).unwrap().clone();
    m.elements.push(e);
    let res = bimrust::check::quick_checks(&[(0, &m)]);
    let get = |name: &str| res.iter().find(|q| q.name == name).unwrap();
    assert_eq!(get("Duplicates").result.failed.len(), 2);
    assert_eq!(get("Duplicate GUID").result.failed.len(), 2);
    assert_eq!(get("Duplicate GUID").result.checked, n + 1);
}

/// MS Project / Primavera P6 (XML) schedules and the sun position.
#[test]
fn schedule_xml_and_sun_position() {
    let msp = r#"<?xml version="1.0"?><Project xmlns="http://schemas.microsoft.com/project"><Tasks>
      <Task><UID>0</UID><Name>Project</Name><Summary>1</Summary><Start>2026-03-02T08:00:00</Start><Finish>2026-04-30T17:00:00</Finish></Task>
      <Task><UID>1</UID><Name>Slab L1 [Level=Level 1;Type=IfcSlab]</Name><Summary>0</Summary><Start>2026-03-02T08:00:00</Start><Finish>2026-03-13T17:00:00</Finish></Task>
      <Task><UID>2</UID><Name>Demolish shed</Name><Start>2026-03-16T08:00:00</Start><Finish>2026-03-18T17:00:00</Finish>
        <ExtendedAttribute><FieldID>188743731</FieldID><Value>Name=Shed</Value></ExtendedAttribute></Task>
    </Tasks></Project>"#;
    let (t, w, f) = bimrust::timeline::parse_schedule_xml(msp).unwrap();
    assert_eq!(f, "MS Project");
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(t.len(), 2, "summary task skipped");
    assert_eq!(t[0].name, "Slab L1");
    assert_eq!(t[0].rule, "Level=Level 1;Type=IfcSlab");
    assert_eq!(bimrust::timeline::fmt_date(t[0].start), "2026-03-02");
    assert_eq!(bimrust::timeline::fmt_date(t[0].end), "2026-03-13");
    assert_eq!(t[1].rule, "Name=Shed");
    assert_eq!(t[1].kind, bimrust::timeline::TaskKind::Demolish);
    let p6 = r#"<APIBusinessObjects><Project><Name>P</Name>
      <Activity><Id>A1000</Id><Name>Columns L0</Name><PlannedStartDate>2026-05-04T08:00:00</PlannedStartDate><PlannedFinishDate>2026-05-08T17:00:00</PlannedFinishDate></Activity>
      <Activity><Id>A1010</Id><Name>No dates</Name></Activity>
    </Project></APIBusinessObjects>"#;
    let (t, w, f) = bimrust::timeline::parse_schedule_xml(p6).unwrap();
    assert_eq!(f, "Primavera P6");
    assert_eq!(t.len(), 1);
    assert_eq!(w.len(), 1);
    assert!(bimrust::timeline::parse_schedule_xml("<Other/>").is_err());
    // Bucharest, 21 June, 13:20 summer time (~ solar noon): the sun to the south, ~69 degrees
    let day = bimrust::timeline::parse_date("2026-06-21").unwrap();
    let (az, el) = bimrust::anim::sun_position(day, 13.3, 3.0, 44.43, 26.10);
    assert!((az - 180.0).abs() < 4.0, "azimuth {az}");
    assert!((el - 69.0).abs() < 1.5, "elevation {el}");
    // 21 December at noon: low (~22 degrees)
    let (_, el) = bimrust::anim::sun_position(bimrust::timeline::parse_date("2026-12-21").unwrap(), 12.3, 2.0, 44.43, 26.10);
    assert!((el - 22.0).abs() < 1.5, "winter elevation {el}");
}

/// Compare on parameters: the changed properties, with old and new values, grouped by parameter.
#[test]
fn compare_lists_changed_parameters() {
    let raw = bimrust::load_any("tests/data/wall_opening.ifc", &|_, _| {}).expect("ifc");
    let a = bimrust::model::Model::from_raw(raw, "a.ifc".into(), "a".into(), glam::DVec3::ZERO);
    let raw = bimrust::load_any("tests/data/wall_opening.ifc", &|_, _| {}).expect("ifc");
    let mut b = bimrust::model::Model::from_raw(raw, "b.ifc".into(), "b".into(), glam::DVec3::ZERO);
    let i = b.elements.iter().position(|e| !e.props.is_empty() && !e.guid.is_empty()).unwrap();
    let old = b.elements[i].props[0].value.clone();
    b.elements[i].props[0].value = "CHANGED".into();
    b.elements[i].props.push(bimrust::model::Prop { group: "Pset_X".into(), name: "New".into(), value: "1".into() });
    let r = bimrust::compare::compare(&a, 0, &b, 1, 1.0);
    let ch = bimrust::compare::param_changes(&a, &b, &r);
    assert_eq!(ch.len(), 2, "{ch:?}");
    assert!(ch.iter().any(|c| c.old == old && c.new == "CHANGED"));
    assert!(ch.iter().any(|c| c.param == "Pset_X / New" && c.old.is_empty() && c.new == "1"));
    let sum = bimrust::compare::param_summary(&ch);
    assert_eq!(sum.len(), 2);
    let csv = bimrust::compare::param_csv(&ch, &b);
    assert!(csv.contains("CHANGED") && csv.lines().count() == 3);
}

/// Geometric diff: wall moved 300 mm on X, slab lowered 100 mm -> the exact deviations, per triangle.
#[test]
fn compare_geometric_deviation_heatmap() {
    use bimrust::model::{Model, RawElement, RawModel};
    use glam::DVec3;
    let boxed = |o: DVec3, s: DVec3| -> Vec<[DVec3; 3]> {
        let v = |x: f64, y: f64, z: f64| o + DVec3::new(x * s.x, y * s.y, z * s.z);
        let p = [v(0., 0., 0.), v(1., 0., 0.), v(1., 1., 0.), v(0., 1., 0.), v(0., 0., 1.), v(1., 0., 1.), v(1., 1., 1.), v(0., 1., 1.)];
        [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]].iter().map(|t| [p[t[0]], p[t[1]], p[t[2]]]).collect()
    };
    let el = |g: &str, tris: Vec<[DVec3; 3]>| {
        let n = tris.len();
        RawElement { name: g.into(), ifc_type: "IfcWall".into(), guid: g.into(), storey: "L1".into(), props: Vec::new(), tris, colors: vec![[200, 200, 200, 255]; n], default_hidden: false }
    };
    let a = RawModel { elements: vec![el("wall", boxed(DVec3::ZERO, DVec3::new(4.0, 0.2, 3.0))), el("slab", boxed(DVec3::new(0.0, 2.0, 3.0), DVec3::new(5.0, 5.0, 0.25)))], info: Vec::new(), warnings: Vec::new() };
    let b = RawModel { elements: vec![el("wall", boxed(DVec3::new(0.3, 0.0, 0.0), DVec3::new(4.0, 0.2, 3.0))), el("slab", boxed(DVec3::new(0.0, 2.0, 2.9), DVec3::new(5.0, 5.0, 0.25)))], info: Vec::new(), warnings: Vec::new() };
    let ma = Model::from_raw(a, "A".into(), String::new(), DVec3::ZERO);
    let mb = Model::from_raw(b, "B".into(), String::new(), DVec3::ZERO);
    let r = bimrust::compare::compare(&ma, 0, &mb, 1, 5.0);
    assert_eq!(r.dev.len(), 2);
    let dev = |g: &str| {
        let i = mb.elements.iter().position(|e| e.guid == g).unwrap();
        r.dev[&bimrust::model::ElemId { m: 1, e: i as u32 }].clone()
    };
    let w = dev("wall");
    assert!((w.max - 0.3).abs() < 1e-4, "wall moved 300 mm: {}", w.max);
    assert_eq!(w.tri.len(), 12);
    // the side faces of the wall sliding in their own plane stay in place (0), the moved end = 300 mm
    assert!(w.tri.iter().any(|d| *d < 1e-4) && w.tri.iter().any(|d| (*d - 0.3).abs() < 1e-4));
    let s = dev("slab");
    assert!((s.max - 0.1).abs() < 1e-4, "slab lowered 100 mm: {}", s.max);
    assert!((s.moved[2] + 0.1).abs() < 1e-4);
    assert!(r.changes.values().any(|c| c.iter().any(|x| x.starts_with("Max deviation 300 mm"))));
    assert_eq!(bimrust::compare::heat_color(0.0)[1], 175);
    assert_eq!(bimrust::compare::heat_color(1.0)[0], 225);
}
