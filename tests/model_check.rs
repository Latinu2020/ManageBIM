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
    no_name.requirements.push(Facet { kind: Kind::Property, set: Value::Any, name: Value::one("FireRating"), value: Value::Any, card: Card::Prohibited });
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

/// exporta modelul de proba ca viewer web (rulat manual: --ignored), pentru verificare in browser
#[test]
#[ignore]
fn export_sample_web_viewer() {
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
    // element din IFC exportat din Revit: GlobalId comprimat + UniqueId in proprietati
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
    // trei pereti (patrate verticale 2 x 2 m in planul XZ): la y = 0, y = 3 si y = 50
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
    // scanarea acopera primul perete (y = 0) si zona pana la y = 4 (al doilea perete lipseste in realitate)
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
    let pdf = bimrust::pdf::sheets("Block A", std::slice::from_ref(&it), "2026-11-20");
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
