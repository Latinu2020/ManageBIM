//! Obiecte Autodesk Plant 3D din DWG prin cititorul incorporat (`bimrust::plant3d`).
//!
//! Fixture sintetic, construit in memorie cu acadrust exact cum il citeste acadrust dintr-un DWG Plant 3D:
//! entitati custom ("UNKNOWN") cu grafica proxy (PUSH_MODEL_XFORM + contur, MESH pentru teava, SHELL
//! "turtit" + axa pentru grinda) si fluxul de handle-uri DWG care refera blocurile din catalog
//! (`Plant3DCatalogItem_*`, `*U*`) pe care nu le insereaza niciun INSERT. Plus, daca exista pe masina,
//! SampleProject-ul Plant 3D din Documents (2-P-001, 6-B-1000_Equip).
use acadrust::entities::acis::primitives;
use acadrust::entities::solid3d::{AcisData, Solid3D};
use acadrust::entities::{Face3D, Insert, UnknownEntity};
use acadrust::tables::{BlockRecord, Layer};
use acadrust::types::{Color, Handle, Vector3};
use acadrust::{CadDocument, DxfVersion, EntityType};
use bimrust::model::{RawElement, RawModel};
use bimrust::plant3d::{encode_handles, encode_proxy, encode_xform};
use bimrust::*;
use glam::{DMat4, DVec3};

fn v(x: f64, y: f64, z: f64) -> Vector3 {
    Vector3::new(x, y, z)
}

fn block(doc: &mut CadDocument, name: &str) -> Handle {
    let mut r = BlockRecord::new(name);
    r.handle = doc.allocate_handle();
    r.block_entity_handle = doc.allocate_handle();
    r.block_end_handle = doc.allocate_handle();
    let h = r.handle;
    doc.block_records.add(r).unwrap();
    h
}

fn add_in(doc: &mut CadDocument, owner: Handle, mut e: EntityType) {
    e.common_mut().owner_handle = owner;
    doc.add_entity(e).unwrap();
}

/// Cutie din 3DFACE-uri (6 fete), [0,sx]x[0,sy]x[0,sz] - geometrie de bloc care nu depinde de OCCT.
fn face_box(doc: &mut CadDocument, owner: Handle, s: [f64; 3]) {
    let p = |i: usize| v(if i & 1 != 0 { s[0] } else { 0.0 }, if i & 2 != 0 { s[1] } else { 0.0 }, if i & 4 != 0 { s[2] } else { 0.0 });
    for q in [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]] {
        let mut f = Face3D::new(p(q[0]), p(q[1]), p(q[2]), p(q[3]));
        f.common.layer = "0".into();
        f.common.color = Color::ByBlock;
        add_in(doc, owner, EntityType::Face3D(f));
    }
}

/// Entitate custom Plant 3D: grafica proxy + fluxul de handle-uri (reactor, strat, blocuri).
fn plant(doc: &mut CadDocument, class: &str, layer: &str, color: Color, proxy: Vec<(u32, Vec<u8>)>, blocks: &[Handle]) -> String {
    let h = doc.allocate_handle();
    let mut u = UnknownEntity::new(class);
    u.common.handle = h;
    u.common.layer = layer.into();
    u.common.color = color;
    u.common.graphic_data = Some(encode_proxy(&proxy));
    let layer_h = doc.layers.get(layer).map(|l| l.handle.value()).unwrap_or(0x10);
    let mut refs = vec![0x4BB8u64, layer_h];
    refs.extend(blocks.iter().map(|b| b.value()));
    let hs = encode_handles(&refs);
    // inregistrarea DWG: date "obiect" oarecare, apoi fluxul de handle-uri (ultimii dwg_handle_bits biti)
    let mut raw = vec![0x46, 0x40, 0x8b, 0x8e, 0x2a, 0x11, 0x22, 0x33];
    raw.extend(&hs);
    u.dwg_handle_bits = (hs.len() * 8) as i64;
    u.raw_dwg_data = Some(raw);
    u.dwg_type_code = 500;
    doc.add_entity(EntityType::Unknown(u)).unwrap();
    format!("{:X}", h.value())
}

fn push(m: DMat4) -> (u32, Vec<u8>) {
    (29, encode_xform(&m))
}
fn pop() -> (u32, Vec<u8>) {
    (31, Vec::new())
}
fn pts(n_prefix: &[u32], p: &[DVec3]) -> Vec<u8> {
    let mut d: Vec<u8> = n_prefix.iter().flat_map(|x| x.to_le_bytes()).collect();
    for q in p {
        d.extend(q.x.to_le_bytes());
        d.extend(q.y.to_le_bytes());
        d.extend(q.z.to_le_bytes());
    }
    d
}
/// contur (cerc 3 puncte) in coordonatele locale ale piesei - ca Plant 3D
fn circle3p(c: DVec3, r: f64) -> (u32, Vec<u8>) {
    (3, pts(&[], &[c + DVec3::X * r, c + DVec3::Y * r, c - DVec3::X * r]))
}
fn line(a: DVec3, b: DVec3) -> (u32, Vec<u8>) {
    (6, pts(&[2], &[a, b]))
}

fn bbox(tris: &[[DVec3; 3]]) -> (DVec3, DVec3) {
    let mut mn = DVec3::splat(f64::INFINITY);
    let mut mx = -mn;
    for t in tris {
        for p in t {
            mn = mn.min(*p);
            mx = mx.max(*p);
        }
    }
    (mn, mx)
}

/// bbox in mm (modelul e in metri)
fn assert_bbox(e: &RawElement, mn: [f64; 3], mx: [f64; 3]) {
    let (a, b) = bbox(&e.tris);
    for k in 0..3 {
        assert!((a[k] * 1000.0 - mn[k]).abs() < 0.05 && (b[k] * 1000.0 - mx[k]).abs() < 0.05, "{}: bbox {:?} - {:?} mm, want {mn:?} - {mx:?}", e.name, a * 1000.0, b * 1000.0);
    }
}

fn el<'a>(m: &'a RawModel, handle: &str) -> &'a RawElement {
    m.elements.iter().find(|e| e.guid == handle).unwrap_or_else(|| panic!("no element for {handle}: {:?}", m.elements.iter().map(|e| (&e.name, &e.ifc_type, &e.guid)).collect::<Vec<_>>()))
}

fn prop<'a>(e: &'a RawElement, name: &str) -> Option<&'a str> {
    e.props.iter().find(|p| p.name == name).map(|p| p.value.as_str())
}

#[test]
fn plant3d_objects_place_catalog_blocks() {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    doc.header.insertion_units = 4; // mm
    let mut l = Layer::new("P-Valves");
    l.color = Color::from_index(3);
    doc.layers.add(l).unwrap();
    doc.layers.add(Layer::new("Equip")).unwrap();
    doc.layers.add(Layer::new("Pipe")).unwrap();
    doc.layers.add(Layer::new("Steel")).unwrap();

    // --- blocuri din catalog (fara INSERT in model space)
    // armatura: cutie 100 x 40 x 20 din 3DFACE + (cu OCCT) un 3DSOLID cilindru mic in interiorul ei
    let valve = block(&mut doc, "Plant3DCatalogItem_CPVI_test_0001");
    face_box(&mut doc, valve, [100.0, 40.0, 20.0]);
    let mut s = Solid3D::new();
    s.acis_data = AcisData::from_sat(&primitives::build_cylinder([50.0, 20.0, 10.0], 5.0, 10.0).to_sat_string());
    s.common.layer = "0".into();
    s.common.color = Color::ByBlock;
    add_in(&mut doc, valve, EntityType::Solid3D(s));
    // stut: cutie 10 x 10 x 30
    let nozzle = block(&mut doc, "Plant3DCatalogItem_CPP_test_0002");
    face_box(&mut doc, nozzle, [10.0, 10.0, 30.0]);
    // virola: cutie 200 x 200 x 100 (local Z = axa)
    let shell = block(&mut doc, "Plant3DCatalogItem_EQCYL_test_0003#");
    face_box(&mut doc, shell, [200.0, 200.0, 100.0]);
    // corpul echipamentului: bloc anonim cu un INSERT culcat (normala +X, OCS) al virolei
    let body = block(&mut doc, "*U7");
    let mut ins = Insert::new("Plant3DCatalogItem_EQCYL_test_0003#", v(-100.0, -100.0, 50.0));
    ins.normal = v(1.0, 0.0, 0.0);
    ins.common.layer = "0".into();
    add_in(&mut doc, body, EntityType::Insert(ins));

    // --- obiectele Plant 3D
    // armatura: rotita 90 grade in jurul lui Z, la (1000, 2000, 300)
    let m_valve = DMat4::from_translation(DVec3::new(1000.0, 2000.0, 300.0)) * DMat4::from_rotation_z(std::f64::consts::FRAC_PI_2);
    let h_valve = plant(&mut doc, "ACPPPIPEINLINEASSET", "P-Valves", Color::Index(1), vec![(16, vec![0; 4]), push(m_valve), circle3p(DVec3::new(0.0, 20.0, 10.0), 10.0), line(DVec3::ZERO, DVec3::X * 100.0), (32, pts(&[2], &[DVec3::ZERO, DVec3::ZERO])), pop()], &[valve]);
    // echipament: corp *U7 (cu push-uri imbricate, ignorate) + acelasi stut de 2 ori, fiecare cu transformarea lui
    let m_body = DMat4::from_translation(DVec3::new(5000.0, 0.0, 0.0));
    let m_n1 = DMat4::from_translation(DVec3::new(5000.0, 300.0, 50.0));
    let m_n2 = DMat4::from_translation(DVec3::new(5300.0, 0.0, 50.0)) * DMat4::from_rotation_y(std::f64::consts::FRAC_PI_2);
    let h_eq = plant(
        &mut doc,
        "ACPPEQUIPMENT",
        "Equip",
        Color::ByLayer,
        vec![push(m_body), push(DMat4::from_translation(DVec3::new(50.0, 0.0, 0.0))), line(DVec3::ZERO, DVec3::Z), pop(), pop(), push(m_n1), line(DVec3::ZERO, DVec3::Z * 30.0), pop(), push(m_n2), line(DVec3::ZERO, DVec3::Z * 30.0), pop()],
        &[body, nozzle, nozzle],
    );
    // teava: MESH 2 x 9 (cilindru cu 8 laturi, r 25, de la x 0 la x 1000, la y 500)
    let ring = |x: f64| (0..9).map(move |i| DVec3::new(x, 500.0 + 25.0 * (i as f64 * std::f64::consts::TAU / 8.0).cos(), 25.0 * (i as f64 * std::f64::consts::TAU / 8.0).sin()));
    let mesh: Vec<DVec3> = ring(0.0).chain(ring(1000.0)).collect();
    let h_pipe = plant(&mut doc, "ACPPPIPE", "Pipe", Color::Index(5), vec![(8, pts(&[2, 9], &mesh))], &[]);
    // grinda: profil L 60 x 60 (SHELL cu ambele inele la z 500) + axa de la z 0 la z 1000
    let prof = [DVec3::new(0.0, 0.0, 500.0), DVec3::new(60.0, 0.0, 500.0), DVec3::new(60.0, 6.0, 500.0), DVec3::new(6.0, 6.0, 500.0), DVec3::new(6.0, 60.0, 500.0), DVec3::new(0.0, 60.0, 500.0)];
    let mut sp: Vec<DVec3> = prof.iter().map(|p| *p + DVec3::new(3000.0, 0.0, 0.0)).collect();
    sp.extend(sp.clone());
    let mut faces: Vec<i32> = Vec::new();
    for i in 0..6 {
        faces.extend([4, i, (i + 1) % 6, (i + 1) % 6 + 6, i + 6]);
    }
    faces.extend([6, 0, 1, 2, 3, 4, 5, 6, 11, 10, 9, 8, 7, 6]);
    let mut shell_rec = pts(&[12], &sp);
    shell_rec.extend((faces.len() as u32).to_le_bytes());
    shell_rec.extend(faces.iter().flat_map(|f| f.to_le_bytes()));
    let h_beam = plant(&mut doc, "ACPPSTRUCTUREBEAM", "Steel", Color::ByLayer, vec![line(DVec3::new(3000.0, 0.0, 0.0), DVec3::new(3000.0, 0.0, 1000.0)), (9, shell_rec)], &[]);
    // balustrada: doar linii in proxy -> tuburi r 20 mm (segmentul repetat o singura data)
    let rail = [line(DVec3::new(0.0, 8000.0, 0.0), DVec3::new(1000.0, 8000.0, 0.0)), line(DVec3::new(1000.0, 8000.0, 0.0), DVec3::new(0.0, 8000.0, 0.0)), line(DVec3::new(0.0, 8000.0, 0.0), DVec3::new(0.0, 8000.0, 1000.0))];
    let h_rail = plant(&mut doc, "ACPPSTRUCTURERAILING", "Steel", Color::ByLayer, rail.to_vec(), &[]);
    // sudura: doar contur, fara bloc -> niciun element
    let h_weld = plant(&mut doc, "ACPPCONNECTOR", "Pipe", Color::ByLayer, vec![line(DVec3::ZERO, DVec3::X)], &[]);

    // --- ce extrage modulul plant3d direct din document
    let objs = plant3d::collect(&doc);
    assert_eq!(objs.len(), 5, "{:?}", objs.iter().map(|o| &o.class).collect::<Vec<_>>());
    let eq = objs.iter().find(|o| o.handle == h_eq).unwrap();
    assert_eq!(eq.blocks.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), ["*U7", "Plant3DCatalogItem_CPP_test_0002", "Plant3DCatalogItem_CPP_test_0002"]);
    assert!(eq.blocks[2].xf.abs_diff_eq(m_n2, 1e-12), "the k-th block takes the k-th top-level transform");
    assert!(objs.iter().all(|o| o.handle != h_weld));

    // --- modelul complet (DWG -> DXF -> elemente)
    let m = formats::convert_dwg_doc(doc, "plant3d_test.dwg", &|_, _| {}).unwrap_or_else(|e| panic!("{e}"));
    assert!(!m.warnings.iter().any(|w| w.contains("Plant 3D")), "{:?}", m.warnings);
    assert!(m.info.iter().any(|(k, v)| k == "Plant 3D objects" && v.starts_with("5 shown")), "{:?}", m.info);
    assert!(m.elements.iter().all(|e| e.guid != h_weld), "a weld without geometry gets no element");

    // armatura: cutia 100 x 40 x 20 rotita 90 grade -> x [960, 1000], y [2000, 2100], z [300, 320]
    let a = el(&m, &h_valve);
    assert_eq!(a.ifc_type, "Pipe Inline Asset");
    assert_eq!(a.storey, "P-Valves");
    assert_eq!(a.name, format!("Pipe Inline Asset {h_valve}"));
    assert_eq!(prop(a, "Catalog blocks"), Some("Plant3DCatalogItem_CPVI_test_0001"));
    assert_bbox(a, [960.0, 2000.0, 300.0], [1000.0, 2100.0, 320.0]);
    assert!(a.colors.iter().all(|c| c[0] > 200 && c[1] < 80 && c[2] < 80), "entity colour (ACI 1) is inherited by ByBlock block geometry: {:?}", a.colors.first());
    if occt::BUILT_WITH_OCCT {
        // cilindrul ACIS din bloc e si el plasat (mai multe triunghiuri decat cele 12 ale cutiei)
        assert!(a.tris.len() > 12, "ACIS solid inside the catalog block: {} triangles", a.tris.len());
    }

    // echipament: virola culcata (INSERT cu normala +X in *U7): local z [0,100] -> x [50,150] fata de corp
    // -> x [5050, 5150], y [-100, 100], z [-100, 100]; stut 1 la (5000, 300, 50); stut 2 rotit (z -> x) la (5300, 0, 50)
    let e = el(&m, &h_eq);
    assert_eq!(e.ifc_type, "Equipment");
    assert_eq!(prop(e, "Placed parts"), Some("3"));
    assert_bbox(e, [5000.0, -100.0, -100.0], [5330.0, 310.0, 100.0]);
    let body_tris: Vec<[DVec3; 3]> = e.tris.iter().copied().filter(|t| t.iter().all(|p| p.x * 1000.0 < 5200.0 && p.y * 1000.0 < 150.0)).collect();
    assert_eq!(body_tris.len(), 12);
    let (bmn, bmx) = bbox(&body_tris);
    assert!((bmn * 1000.0 - DVec3::new(5050.0, -100.0, -100.0)).abs().max_element() < 0.05 && (bmx * 1000.0 - DVec3::new(5150.0, 100.0, 100.0)).abs().max_element() < 0.05, "INSERT in OCS: {:?} {:?}", bmn * 1000.0, bmx * 1000.0);
    assert!(e.colors.iter().all(|c| c[0] > 200 && c[1] > 200 && c[2] > 200), "ByLayer colour of layer Equip (white)");

    // teava: tub neted (32 laturi) r 25 intre x 0 si 1000, cu normalele spre exterior
    let p = el(&m, &h_pipe);
    assert_eq!(p.ifc_type, "Pipe");
    assert_eq!(p.tris.len(), 64);
    assert_bbox(p, [0.0, 475.0, -25.0], [1000.0, 525.0, 25.0]);
    let axis_y = 0.5;
    assert!(p.tris.iter().all(|t| (t[1] - t[0]).cross(t[2] - t[0]).dot((t[0] + t[1] + t[2]) / 3.0 - DVec3::new((t[0].x + t[1].x + t[2].x) / 3.0, axis_y, 0.0)) > 0.0), "pipe normals point outwards");

    // grinda: profilul L extrudat pe axa z 0 .. 1000
    let b = el(&m, &h_beam);
    assert_eq!(b.ifc_type, "Structure Beam");
    assert_bbox(b, [3000.0, 0.0, 0.0], [3060.0, 60.0, 1000.0]);

    // balustrada: 2 tuburi hexagonale (12 triunghiuri fiecare), cu eticheta de geometrie aproximata
    let r = el(&m, &h_rail);
    assert_eq!(r.ifc_type, "Structure Railing");
    assert_eq!(r.tris.len(), 24);
    assert!(prop(r, "Geometry").is_some_and(|g| g.starts_with("Approximated")));
    let (rmn, rmx) = bbox(&r.tris);
    assert!(rmn.x * 1000.0 > -20.5 && rmx.x * 1000.0 < 1000.5 && rmx.z * 1000.0 > 999.0 && (rmx.y * 1000.0 - 8020.0).abs() < 0.5, "{:?} {:?}", rmn * 1000.0, rmx * 1000.0);
    // ce n-a devenit element e listat in info
    assert!(m.info.iter().any(|(k, v)| k == "Custom objects without 3D geometry" && v == "1 ACPPCONNECTOR"), "{:?}", m.info);
}

#[test]
fn handle_stream_relative_references() {
    // cod 5 absolut, 6 = +1, 8 = -1, A = +n, C = -n (fata de handle-ul obiectului 0x100)
    let raw = [0xFFu8, 0x52, 0x01, 0x23, 0x60, 0x80, 0xA1, 0x05, 0xC1, 0x10];
    let r = plant3d::handle_refs(&raw, 9 * 8, 0x100);
    assert_eq!(r, vec![0x123, 0x101, 0xFF, 0x105, 0xF0]);
    // fara flux de handle-uri / date trunchiate: nimic, fara panica
    assert!(plant3d::handle_refs(&raw, 0, 0x100).is_empty());
    assert!(plant3d::handle_refs(&raw, 1000, 0x100).is_empty());
    assert!(plant3d::parse_proxy(&[1, 2, 3]).placements.is_empty());
    assert!(plant3d::parse_proxy(&encode_proxy(&[(29, vec![0; 10])])).placements.is_empty());
    assert_eq!(plant3d::class_label("ACPPPIPEINLINEASSET"), "Pipe Inline Asset");
    assert_eq!(plant3d::class_label("ACPPWIDGET"), "Widget");
}

/// SampleProject-ul Plant 3D (daca exista pe masina): piping si echipamente plasate, fara avertismente.
#[test]
fn sample_project_plant3d() {
    let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) else { return };
    let dir = std::path::PathBuf::from(home).join("Documents").join("SampleProject").join("Plant_3D_Models");
    for (file, min_objects, classes) in [
        ("Area_2/2-P-001.dwg", 1785, &["Pipe", "Pipe Inline Asset", "Pipe Support", "Pipe Connector"][..]),
        ("Area_6/6-B-1000_Equip.dwg", 79, &["Equipment", "Structure Beam"][..]),
    ] {
        let p = dir.join(file);
        if !p.is_file() || !occt::BUILT_WITH_OCCT {
            eprintln!("skipping {} (sample or OpenCascade missing)", p.display());
            continue;
        }
        let m = formats::convert_dwg_native(&p.to_string_lossy(), &|_, _| {}).unwrap_or_else(|e| panic!("{file}: {e}"));
        let n = m.elements.iter().filter(|e| e.props.iter().any(|q| q.name == "Entity" && q.value.starts_with("ACPP"))).count();
        assert!(n >= min_objects, "{file}: {n} Plant 3D elements");
        for c in classes {
            assert!(m.elements.iter().any(|e| e.ifc_type == *c && !e.tris.is_empty()), "{file}: no {c}");
        }
        assert!(!m.warnings.iter().any(|w| w.contains("Plant 3D") || w.contains("ACIS")), "{file}: {:?}", m.warnings);
    }
    // fisierele mari pe care acadrust nu le citeste strict: recitite in modul de recuperare
    for (file, min_objects) in [("Area_1/1-PE-001.dwg", 1500), ("Area_6/6-B-1000_Piping.dwg", 2700)] {
        let p = dir.join(file);
        if !p.is_file() || !occt::BUILT_WITH_OCCT {
            continue;
        }
        let m = formats::convert_dwg_native(&p.to_string_lossy(), &|_, _| {}).unwrap_or_else(|e| panic!("{file}: {e}"));
        let n = m.elements.iter().filter(|e| e.props.iter().any(|q| q.name == "Entity" && q.value.starts_with("ACPP"))).count();
        assert!(n >= min_objects, "{file}: {n} Plant 3D elements");
        assert!(m.warnings.iter().any(|w| w.contains("recovery mode")), "{file}: {:?}", m.warnings);
    }
}

/// Proprietatile din baza proiectului Plant 3D (Piping.dcf, SQLite) - fixture minim construit aici:
/// legatura handle -> RowId (PnPDataLinks), mostenirea tabelelor (PnPTables), coloane ascunse si nume afisate
/// (PnPColumnAttributes), sub-piese (DwgSubIndex > 0), linia din grupul de linii si propagata prin conexiuni.
#[test]
fn plant3d_project_database_properties() {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    doc.header.insertion_units = 4;
    doc.layers.add(Layer::new("P")).unwrap();
    let valve_blk = block(&mut doc, "Plant3DCatalogItem_CPVI_db_0001");
    face_box(&mut doc, valve_blk, [100.0, 40.0, 20.0]);
    let h_valve = plant(&mut doc, "ACPPPIPEINLINEASSET", "P", Color::ByLayer, vec![push(DMat4::IDENTITY), line(DVec3::ZERO, DVec3::X), pop()], &[valve_blk]);
    let ring = |x: f64| (0..9).map(move |i| DVec3::new(x, 25.0 * (i as f64 * std::f64::consts::TAU / 8.0).cos(), 25.0 * (i as f64 * std::f64::consts::TAU / 8.0).sin()));
    let mesh: Vec<DVec3> = ring(200.0).chain(ring(1000.0)).collect();
    let h_pipe = plant(&mut doc, "ACPPPIPE", "P", Color::ByLayer, vec![(8, pts(&[2, 9], &mesh))], &[]);
    let h_other = plant(&mut doc, "ACPPPIPE", "P", Color::ByLayer, vec![(8, pts(&[2, 9], &mesh.iter().map(|p| *p + DVec3::Y * 500.0).collect::<Vec<_>>()))], &[]);
    let mut m = formats::convert_dwg_doc(doc, "line.dwg", &|_, _| {}).unwrap_or_else(|e| panic!("{e}"));

    // folderul proiectului: Piping.dcf + Plant_3D_Models\Area_9\line.dwg
    let root = std::env::temp_dir().join(format!("mb_plantdb_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let area = root.join("Plant_3D_Models").join("Area_9");
    std::fs::create_dir_all(&area).unwrap();
    let dwg = area.join("line.dwg");
    std::fs::write(&dwg, b"").unwrap();
    // fara baza: modelul ramane neschimbat
    let before = m.elements.iter().map(|e| e.props.len()).sum::<usize>();
    formats::attach_plant_db(&mut m, &dwg, &|_, _| {});
    assert_eq!(m.elements.iter().map(|e| e.props.len()).sum::<usize>(), before);

    let db = root.join("Piping.dcf");
    {
        let c = rusqlite::Connection::open(&db).unwrap();
        let h = |s: &str| i64::from_str_radix(s, 16).unwrap();
        c.execute_batch(
            r#"
            create table PnPTables (TableName text, BaseTable text, Abstract text, PhysicalName text);
            insert into PnPTables values ('PnPBase','', 'TRUE','PnPBase'), ('EngineeringItems','PnPBase','TRUE','EngineeringItems'),
              ('PipeRunComponent','EngineeringItems','TRUE','PipeRunComponent'), ('Valve','PipeRunComponent','FALSE','Valve'),
              ('Pipe','PipeRunComponent','FALSE','Pipe'), ('Fasteners','EngineeringItems','TRUE','Fasteners'), ('Gasket','Fasteners','FALSE','Gasket');
            create table PnPColumnAttributes (TableName text, ColumnName text, AttributeName text, AttributeValue text);
            insert into PnPColumnAttributes values ('EngineeringItems','PartFamilyId','ISHIDDEN','TRUE'), ('EngineeringItems','ItemCode','ISHIDDEN','TRUE'),
              ('Valve','ValveCode','DISPLAYNAME','Valve Code (display)');
            create table PnPBase (PnPID integer primary key, PnPClassName text, PnPGuid blob);
            create table EngineeringItems (PnPID integer primary key, PartFamilyId text, PartFamilyLongDesc text, PartSizeLongDesc text, Spec text, Material text, ItemCode text, NominalDiameter real, Weight real);
            create table PipeRunComponent (PnPID integer primary key, Tag text, LineNumberTag text, Service text, SpecRecordId integer);
            create table Valve (PnPID integer primary key, ValveCode text, OperatorType text);
            create table Pipe (PnPID integer primary key, Length real);
            create table Fasteners (PnPID integer primary key, LineNumberTag text);
            create table Gasket (PnPID integer primary key, Shop_Field text);
            create table PnPDrawings (PnPID integer primary key, [Dwg Name] text, PnPRelativePath text, PnPType text);
            insert into PnPDrawings values (5, 'line.dwg', 'Plant_3D_Models\Area_9\line.dwg', 'DWG'), (6, 'line.dwg', 'Plant_3D_Models\Other\line.dwg', 'DWG');
            create table PnPDataLinks (PnPID integer primary key, RowId integer, DwgId integer, DwgHandleLow integer, DwgHandleHigh integer, DwgSubIndex integer, RowClassName text);
            create table P3dLineGroup (PnPID integer primary key, Tag text, Number text);
            insert into P3dLineGroup values (900, '1002', '1002');
            create table P3dLineGroupPartRelationship (PnPID integer primary key, LineGroup integer, Part integer);
            insert into P3dLineGroupPartRelationship values (1, 900, 101);
            create table P3dPartConnection (PnPID integer primary key, Part1 integer, Part2 integer);
            insert into P3dPartConnection values (1, 101, 100);
            insert into EngineeringItems values (100, 'fam', 'VALVE, GATE, 150 LB', 'VALVE GATE 6" ND', 'CS150', 'CS', 'IC-1', 6.0, 45.5),
              (101, 'fam', 'PIPE, SEAMLESS', 'PIPE 6" ND', 'CS150', 'CS', 'IC-2', 6.0, null),
              (102, 'fam', 'GASKET, SWG', 'GASKET 6" ND', 'CS150', 'CS', 'IC-3', 6.0, null),
              (103, 'fam', 'PIPE, OTHER DRAWING', '', '', '', '', null, null);
            insert into PipeRunComponent values (100, 'V-101', '?', 'FO', 7), (101, '?', '?', 'FO', 7), (103, '?', '?', '', 7);
            insert into Valve values (100, 'GV-6', 'Handwheel');
            insert into Pipe values (101, 812.5), (103, 1.0);
            insert into Fasteners values (102, '?');
            insert into Gasket values (102, 'FIELD');
            "#,
        )
        .unwrap();
        let mut ins = c.prepare("insert into PnPDataLinks (RowId, DwgId, DwgHandleLow, DwgHandleHigh, DwgSubIndex, RowClassName) values (?1, ?2, ?3, 0, ?4, ?5)").unwrap();
        ins.execute(rusqlite::params![100, 5, h(&h_valve), 0, "Valve"]).unwrap();
        ins.execute(rusqlite::params![102, 5, h(&h_valve), 1, "Gasket"]).unwrap();
        ins.execute(rusqlite::params![101, 5, h(&h_pipe), 0, "Pipe"]).unwrap();
        // acelasi handle, alt desen (acelasi nume de fisier, alt folder): nu trebuie folosit
        ins.execute(rusqlite::params![103, 6, h(&h_other), 0, "Pipe"]).unwrap();
    }
    let modified = std::fs::metadata(&db).unwrap().modified().unwrap();
    formats::attach_plant_db(&mut m, &dwg, &|_, _| {});
    assert_eq!(std::fs::metadata(&db).unwrap().modified().unwrap(), modified, "the project database is only read");
    assert!(m.info.iter().any(|(k, v)| k == "Plant 3D project" && v.ends_with("properties for 2 of 3 objects")), "{:?}", m.info);

    let g = |e: &RawElement, group: &str, name: &str| e.props.iter().find(|p| p.group == group && p.name == name).map(|p| p.value.clone());
    let v = el(&m, &h_valve);
    assert_eq!(v.name, "Pipe Inline Asset V-101", "the Tag names the element");
    assert_eq!(g(v, "Plant 3D", "Class").as_deref(), Some("Valve"));
    assert_eq!(g(v, "Plant 3D", "Size").as_deref(), Some("VALVE GATE 6\" ND"));
    assert_eq!(g(v, "Plant 3D", "Long Description (Family)").as_deref(), Some("VALVE, GATE, 150 LB"));
    assert_eq!(g(v, "Plant 3D", "Spec").as_deref(), Some("CS150"));
    assert_eq!(g(v, "Plant 3D", "Nominal Diameter").as_deref(), Some("6"));
    assert_eq!(g(v, "Plant 3D", "Weight").as_deref(), Some("45.5"));
    assert_eq!(g(v, "Plant 3D", "Valve Code (display)").as_deref(), Some("GV-6"), "DISPLAYNAME from PnPColumnAttributes");
    assert_eq!(g(v, "Plant 3D", "Operator Type").as_deref(), Some("Handwheel"));
    assert_eq!(g(v, "Plant 3D", "Line Number").as_deref(), Some("1002"), "line propagated from the connected pipe");
    assert_eq!(v.storey, "Line 1002");
    assert_eq!(g(v, "Plant 3D: Gasket", "Shop / Field").as_deref(), Some("FIELD"), "sub-part (DwgSubIndex 1)");
    for hidden in ["Part Family Id", "Item Code", "Spec Record Id", "PnPGuid"] {
        assert!(!v.props.iter().any(|p| p.group.starts_with("Plant 3D") && p.name.replace(' ', "") == hidden.replace(' ', "")), "{hidden} is internal");
    }
    assert!(!v.props.iter().any(|p| p.group.starts_with("Plant 3D") && (p.value == "?" || p.value.is_empty())), "no empty / unassigned values");

    let p = el(&m, &h_pipe);
    assert_eq!(p.name, format!("Pipe 1002 / {h_pipe}"));
    assert_eq!(p.storey, "Line 1002");
    assert_eq!(g(p, "Plant 3D", "Length").as_deref(), Some("812.5"));
    assert_eq!(g(p, "Plant 3D", "Service").as_deref(), Some("FO"));
    let o = el(&m, &h_other);
    assert!(!o.props.iter().any(|p| p.group.starts_with("Plant 3D") && p.name == "Class"), "rows of another drawing are not used");
    let _ = std::fs::remove_dir_all(&root);
}