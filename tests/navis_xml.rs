//! Export viewpoints + markups in formatul XML Navisworks (nw-exchange-12.0.xsd).
use bimrust::markup::{navis_viewpoints_xml, nw_quaternion, NwView, RedKind, Redline};

// APPDATA is process-global: tests that redirect it must not run in parallel.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn rot(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    // v' = q v q*  (q = x, y, z, w)
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let m = [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
        [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
        [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
    ];
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

fn close(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)
}

#[test]
fn camera_quaternion_maps_navisworks_axes() {
    // camera Navisworks: priveste spre -Z local, sus = +Y local
    for (fwd, up) in [([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]), ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]), ([0.6, 0.0, -0.8], [0.8, 0.0, 0.6]), ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0])] {
        let q = nw_quaternion(fwd, up);
        let n = (q.iter().map(|v| v * v).sum::<f64>()).sqrt();
        assert!((n - 1.0).abs() < 1e-9, "unit quaternion");
        assert!(close(rot(q, [0.0, 0.0, -1.0]), fwd), "view direction for {fwd:?}");
        assert!(close(rot(q, [0.0, 1.0, 0.0]), up), "up for {fwd:?}");
    }
}

#[test]
fn xml_contains_camera_section_comments_and_markups() {
    let red = |kind, pts: Vec<[f32; 2]>, text: &str| Redline { kind, pts, color: [255, 0, 0], width: 3.0, text: text.into() };
    let v = NwView {
        name: "Clash <1> & pipe".into(),
        guid: "5d0f2c9e-0000-4000-8000-000000000001".into(),
        eye: [10.0, -5.0, 3.0],
        fwd: [1.0, 0.0, 0.0],
        up: [0.0, 0.0, 1.0],
        fov_deg: 50.0,
        ortho_height: None,
        aspect: 1.6,
        focal: 8.0,
        near: 0.1,
        far: 500.0,
        scene_min: [0.0; 3],
        scene_max: [50.0, 30.0, 12.0],
        section_box: Some(([1.0, 2.0, 0.0], [5.0, 6.0, 3.0])),
        comments: vec![("ana".into(), "Move the pipe".into(), "2026-11-18T10:20:30Z".into(), "active")],
        redlines: vec![
            red(RedKind::Line, vec![[-0.2, -0.1], [0.2, 0.1]], ""),
            red(RedKind::Arrow, vec![[0.0, 0.0], [0.1, 0.1]], ""),
            red(RedKind::Rect, vec![[-0.1, -0.1], [0.1, 0.1]], ""),
            red(RedKind::Ellipse, vec![[-0.1, -0.1], [0.1, 0.1]], ""),
            red(RedKind::Cloud, vec![[-0.1, -0.1], [0.1, 0.1]], ""),
            red(RedKind::Freehand, (0..200).map(|i| [i as f32 * 0.001, 0.0]).collect(), ""),
            red(RedKind::Text, vec![[0.3, -0.2]], "check <here>"),
        ],
        folder: String::new(),
    };
    let x = navis_viewpoints_xml(&[v], "m");
    assert!(x.starts_with("<?xml") && x.contains("<exchange") && x.trim_end().ends_with("</exchange>"));
    assert!(x.contains("name=\"Clash &lt;1&gt; &amp; pipe\""), "escaped names");
    assert!(x.contains("projection=\"persp\"") && x.contains("<pos3f x=\"10.000000\" y=\"-5.000000\" z=\"3.000000\"/>"));
    assert!(x.contains("mode=\"box\"") && x.contains("<box><box3f><min><pos3f x=\"1.000000\""));
    assert!(x.contains("<comment id=\"1\" status=\"active\"><user>ana</user><body>Move the pipe</body><createddate><date year=\"2026\" month=\"11\" day=\"18\" hour=\"10\" minute=\"20\" second=\"30\"/>"));
    for tag in ["<rlline", "<rlarrow", "<rlellipse", "<rlcloud", "<rltext"] {
        assert!(x.contains(tag), "{tag}");
    }
    assert!(x.contains("<text>check &lt;here&gt;</text>"));
    // y in sus in Navisworks: punctul de sus al ecranului (y negativ la noi) are y pozitiv
    assert!(x.contains("<pos2f x=\"0.37500\" y=\"0.40000\"/>"), "text position in Navisworks screen coordinates");
    // fisierul de proba, validat separat cu schema oficiala (tools / verificare manuala)
    let _ = std::fs::write(std::env::temp_dir().join("managebim_navis_test.xml"), &x);
}

// ---- pe aplicatia reala: export -> import (viewpoints), seturi si teste de clash
use bimrust::app::*;
use std::sync::Arc;

fn app() -> App {
    let p = |_: f32, _: &str| {};
    let a = bimrust::load_any("samples/Block_A_Architecture_Structure.ifc", &p).unwrap();
    let b = bimrust::load_any("samples/Block_A_MEP.ifc", &p).unwrap();
    let o = a.center();
    let mut app = App::new(vec![]);
    app.models.push(Some(Arc::new(bimrust::build_model(a, "samples/Block_A_Architecture_Structure.ifc", o))));
    app.models.push(Some(Arc::new(bimrust::build_model(b, "samples/Block_A_MEP.ifc", o))));
    *app.origin.lock().unwrap() = Some(o);
    app
}

#[test]
fn viewpoints_roundtrip_and_sets_clash_export() {
    let dir = std::env::temp_dir().join(format!("mb_nw_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut app = app();
    app.vp_rect = [0.0, 0.0, 1600.0, 900.0];
    app.cam.eye = [3.0, -12.0, 4.0];
    app.cam.yaw = 1.2;
    app.cam.pitch = -0.3;
    app.cam.focus = 9.0;
    app.section.enabled = true;
    app.section.mode = SectionMode::Box;
    app.section.bmin = [-2.0, -3.0, 0.0];
    app.section.bmax = [4.0, 5.0, 3.0];
    app.save_viewpoint();
    {
        let v = &mut app.viewpoints[0];
        v.name = "Ușă / perete".into();
        v.comment = "Mută ușa 200 mm".into();
        v.issue.status = "In Progress".into();
        v.issue.priority = "High".into();
        v.redlines.push(bimrust::markup::Redline { kind: RedKind::Arrow, pts: vec![[-0.2, -0.1], [0.3, 0.2]], color: [255, 0, 0], width: 3.0, text: String::new() });
        v.redlines.push(bimrust::markup::Redline { kind: RedKind::Text, pts: vec![[0.1, 0.1]], color: [0, 0, 255], width: 2.0, text: "aici".into() });
    }
    let x = dir.join("vp.xml");
    app.export_navis_xml(&x.to_string_lossy());
    app.import_navis_xml(&x.to_string_lossy());
    assert_eq!(app.viewpoints.len(), 2, "imported next to the original");
    let (a, b) = (&app.viewpoints[0], &app.viewpoints[1]);
    assert_eq!(b.name, a.name);
    assert!((b.cam.eye() - a.cam.eye()).length() < 1e-3, "{:?} vs {:?}", b.cam.eye, a.cam.eye);
    assert!((b.cam.fwd() - a.cam.fwd()).length() < 1e-4);
    assert!((b.cam.fov_deg - a.cam.fov_deg).abs() < 1e-3);
    assert!(b.section.enabled && b.section.mode == SectionMode::Box);
    assert!((glam::Vec3::from(b.section.bmin) - glam::Vec3::from(a.section.bmin)).length() < 1e-3);
    assert_eq!(b.comment, a.comment);
    assert_eq!((b.issue.status.as_str(), b.issue.priority.as_str()), ("In Progress", "High"));
    assert_eq!(b.issue.guid, a.issue.guid);
    assert_eq!(b.redlines.len(), 2);
    for (r, s) in a.redlines.iter().zip(&b.redlines) {
        assert_eq!(r.kind, s.kind);
        for (p, q) in r.pts.iter().zip(&s.pts) {
            assert!((p[0] - q[0]).abs() < 1e-3 && (p[1] - q[1]).abs() < 1e-3, "markup position {p:?} {q:?}");
        }
    }
    assert_eq!(b.redlines[1].text, "aici");

    // seturi + teste de clash
    let doors = app.find(&[SearchRule::new("", "IFC Type", SearchOp::Equals, "IfcDoor", false)]);
    app.sets.push(SelSet { name: "Uși".into(), kind: SetKind::Search(vec![SearchRule::new("", "IFC Type", SearchOp::Equals, "IfcDoor", false), SearchRule::new("", "Level", SearchOp::Equals, "Level 1", true)]) });
    app.sets.push(SelSet { name: "Explicit".into(), kind: SetKind::Explicit(doors.clone()) });
    let (xml, warn) = app.navis_selectionsets_xml();
    assert!(warn.is_empty(), "{warn:?}");
    assert!(xml.contains("name=\"Uși\"") && xml.contains("LcOaSceneBaseClassUserName") && xml.contains("flags=\"74\""));
    assert_eq!(xml.matches("LcOaNodeGuid").count(), doors.len());
    app.clash_tests[0].a = Selector { kind: SelKind::Set, value: "Uși".into() };
    app.clash_tests[0].b = Selector { kind: SelKind::Model, value: "Block_A_MEP.ifc".into() };
    app.clash_tests[0].tol_mm = 10.0;
    app.export_navis_sets(&dir.join("sets.xml").to_string_lossy());
    app.export_navis_clash(&dir.join("clash.xml").to_string_lossy());
    let c = std::fs::read_to_string(dir.join("clash.xml")).unwrap();
    assert!(c.contains("<batchtest") && c.contains("test_type=\"hard\"") && c.contains("tolerance=\"0.010000\"") && c.contains("lcop_selection_set_tree/Uși"));
    // copii pentru validarea cu schema Navisworks (vezi raportul de verificare)
    for f in ["vp.xml", "sets.xml", "clash.xml"] {
        let _ = std::fs::copy(dir.join(f), std::env::temp_dir().join(format!("managebim_nw_{f}")));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn clashes_to_bcf_and_navisworks_and_bcf_import() {
    let dir = std::env::temp_dir().join(format!("mb_bcf_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut app = app();
    app.clash_tests[0].a = Selector { kind: SelKind::Model, value: "Block_A_Architecture_Structure.ifc".into() };
    app.clash_tests[0].b = Selector { kind: SelKind::Model, value: "Block_A_MEP.ifc".into() };
    app.run_clash_blocking(0);
    app.cur_test = 0;
    let n = app.clash_tests[0].results.len();
    assert!(n > 0);
    let bcf = dir.join("clashes.bcf");
    app.export_clash_bcf(&bcf.to_string_lossy());
    app.export_clash_navis(&dir.join("clashes.xml").to_string_lossy());
    let topics = bimrust::markup::parse_bcf(std::fs::read(&bcf).unwrap()).unwrap();
    assert_eq!(topics.len(), n, "one topic per open clash");
    assert!(topics.iter().all(|t| t.camera.is_some() && t.selected.len() == 2 && t.title.starts_with("Clash")));
    let x = std::fs::read_to_string(dir.join("clashes.xml")).unwrap();
    assert_eq!(x.matches("<view ").count(), n);
    assert_eq!(x.matches("mode=\"box\"").count(), n, "section box around every clash");
    // import inapoi: elementele clash-ului se gasesc dupa GUID
    let before = app.viewpoints.len();
    app.import_bcf(&bcf.to_string_lossy());
    assert_eq!(app.viewpoints.len(), before + n);
    // ordinea topic-urilor in BCF urmeaza GUID-urile (aleatoare): caut viewpoint-ul clash-ului 0
    assert!(app.viewpoints[before..].iter().all(|v| v.selected.len() == 2), "both clashing elements selected");
    let r = &app.clash_tests[0].results[0];
    let v = app.viewpoints[before..].iter().find(|v| v.selected.contains(&r.a) && v.selected.contains(&r.b)).expect("viewpoint for clash 0");
    let p = glam::Vec3::from(r.point);
    assert!(v.cam.fwd().dot((p - v.cam.eye()).normalize()) > 0.99, "camera looks at the clash");
    let _ = std::fs::copy(dir.join("clashes.xml"), std::env::temp_dir().join("managebim_nw_clashes.xml"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn draw_lists_cull_safely() {
    let mut app = app();
    let bb = app.scene_aabb();
    app.cam.fit(&bb);
    let p = app.frame_params(1.6);
    let lists = app.draw_lists(&p, 900.0);
    for (slot, m) in app.loaded_models() {
        let l = &lists[&(slot as u32)];
        let total = m.tri_total() as i32 * 3;
        assert!(l.all.iter().all(|&(f, n)| f >= 0 && n > 0 && f + n <= total), "ranges inside the buffer");
        // vedere completa, nimic ascuns: fiecare element e acoperit
        for e in &m.elements {
            if e.tri_count == 0 {
                continue;
            }
            let f = e.tri_start as i32 * 3;
            assert!(l.all.iter().any(|&(a, n)| f >= a && f < a + n), "element {} drawn", e.name);
        }
    }
    // camera intoarsa cu spatele: aproape nimic de desenat
    let mut away = app.cam;
    away.yaw += std::f32::consts::PI;
    away.eye = (bb.center() + (bb.center() - app.cam.eye())).into();
    app.cam = away;
    let p2 = app.frame_params(1.6);
    let l2 = app.draw_lists(&p2, 900.0);
    let drawn: i32 = l2.values().flat_map(|l| l.all.iter().map(|r| r.1)).sum();
    let full: i32 = lists.values().flat_map(|l| l.all.iter().map(|r| r.1)).sum();
    assert!(drawn < full, "frustum culling: {drawn} < {full}");
}

#[test]
fn ifc_export_roundtrip_keeps_items_guids_and_properties() {
    let mut app = app();
    let path = std::env::temp_dir().join(format!("mb_export_{}.ifc", std::process::id()));
    let p = path.to_string_lossy().to_string();
    app.export_ifc(&p);
    let raw = bimrust::load_any(&p, &|_, _| {}).expect("our own IFC reads back");
    let src: Vec<&bimrust::model::Element> = app.loaded_models().flat_map(|(_, m)| m.elements.iter().filter(|e| e.tri_count > 0 && !e.default_hidden)).collect();
    assert_eq!(raw.elements.len(), src.len(), "every item exported");
    let back: std::collections::HashMap<&str, &bimrust::model::RawElement> = raw.elements.iter().map(|e| (e.guid.as_str(), e)).collect();
    let mut checked = 0;
    for e in src.iter().filter(|e| e.guid.len() == 22) {
        let b = back.get(e.guid.as_str()).unwrap_or_else(|| panic!("GUID {} kept", e.guid));
        assert_eq!(b.name, e.name);
        assert_eq!(b.storey, e.storey, "level of {}", e.name);
        assert!(!b.tris.is_empty(), "geometry of {}", e.name);
        // proprietatile din seturile IFC (nu cele calculate) se regasesc
        for pr in e.props.iter().filter(|p| p.group.starts_with("Pset_")) {
            assert!(b.props.iter().any(|q| q.group == pr.group && q.name == pr.name && q.value == pr.value), "{} / {} = {} on {}", pr.group, pr.name, pr.value, e.name);
        }
        checked += 1;
    }
    assert!(checked > 50, "checked {checked}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn markups_become_polylines_for_revit_and_navisworks() {
    use bimrust::markup::{redline_polylines, RedKind, Redline};
    let r = |kind, pts: Vec<[f32; 2]>| Redline { kind, pts, color: [255, 0, 0], width: 3.0, text: "Fix".into() };
    let a = [-0.2f32, -0.1];
    let b = [0.3f32, 0.2];
    assert_eq!(redline_polylines(&r(RedKind::Line, vec![a, b])), vec![vec![a, b]]);
    assert_eq!(redline_polylines(&r(RedKind::Arrow, vec![a, b])).len(), 2, "shaft + head");
    let rect = &redline_polylines(&r(RedKind::Rect, vec![a, b]))[0];
    assert_eq!(rect.len(), 5);
    assert_eq!(rect[0], rect[4], "closed rectangle");
    let cloud = &redline_polylines(&r(RedKind::Cloud, vec![a, b]))[0];
    assert!(cloud.len() > 40, "scalloped cloud");
    // festoanele ies in afara dreptunghiului, dar putin
    assert!(cloud.iter().all(|p| p[0] > a[0] - 0.05 && p[0] < b[0] + 0.05 && p[1] > a[1] - 0.05 && p[1] < b[1] + 0.05));
    assert!(redline_polylines(&r(RedKind::Text, vec![a])).is_empty());
}

#[test]
fn ifc_grids_and_levels_give_clash_locations() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/samples/Block_A_Architecture_Structure.ifc")).unwrap();
    // doua axe: A (de-a lungul lui Y, la x=0) si 1 (de-a lungul lui X, la y=0), in unitatile fisierului
    let unit = if src.contains(".MILLI.") { 1000.0 } else { 1.0 };
    let extra = format!(
        "#900001=IFCCARTESIANPOINT((0.,{n}));\n#900002=IFCCARTESIANPOINT((0.,{p}));\n#900003=IFCCARTESIANPOINT(({n},0.));\n#900004=IFCCARTESIANPOINT(({p},0.));\n\
         #900010=IFCCARTESIANPOINT((0.,0.,0.));\n#900011=IFCAXIS2PLACEMENT3D(#900010,$,$);\n#900012=IFCLOCALPLACEMENT($,#900011);\n\
         #900020=IFCPOLYLINE((#900001,#900002));\n#900021=IFCGRIDAXIS('A',#900020,.T.);\n#900022=IFCPOLYLINE((#900003,#900004));\n#900023=IFCGRIDAXIS('1',#900022,.T.);\n\
         #900030=IFCGRID('3$aaaaaaaaaaaaaaaaaaaa',$,'Grid',$,$,#900012,$,(#900021),(#900023),$);\n",
        n = -50.0 * unit,
        p = 50.0 * unit
    );
    let i = src.rfind("ENDSEC;").unwrap();
    let data = format!("{}{}{}", &src[..i], extra, &src[i..]);
    let raw = bimrust::ifc::load_ifc_bytes(data.into_bytes(), &|_, _| {}).unwrap();
    let grids = &raw.info.iter().find(|(k, _)| k == "~grids").expect("grids read").1;
    assert!(grids.contains("\"A\"") && grids.contains("\"1\""), "{grids}");
    let levels = &raw.info.iter().find(|(k, _)| k == "~levels").expect("levels read").1;
    assert!(levels.contains("Level 1"), "{levels}");
    // locatia unui punct langa intersectie, deasupra primului nivel
    let g = bimrust::app::GridData {
        grids: vec![("A".into(), glam::Vec3::new(0.0, -50.0, 0.0), glam::Vec3::new(0.0, 50.0, 0.0)), ("1".into(), glam::Vec3::new(-50.0, 0.0, 0.0), glam::Vec3::new(50.0, 0.0, 0.0)), ("2".into(), glam::Vec3::new(-50.0, 6.0, 0.0), glam::Vec3::new(50.0, 6.0, 0.0))],
        levels: vec![("Ground".into(), 0.0), ("Level 1".into(), 3.0)],
    };
    assert_eq!(bimrust::app::grid_location_in(&g, glam::Vec3::new(0.4, 1.0, 3.5)), "A-1 / Level 1");
    assert_eq!(bimrust::app::grid_location_in(&g, glam::Vec3::new(-0.4, 5.0, 1.0)), "A-2 / Ground");
}

#[test]
fn shared_issues_sync_between_two_colleagues() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("mb_shared_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let d = dir.to_string_lossy().to_string();
    let mut a = app();
    let mut b = app();
    a.shared_dir = Some(d.clone());
    b.shared_dir = Some(d.clone());
    // A: un issue cu doua elemente selectate si o cota
    let ids: Vec<_> = (0..2).map(|e| bimrust::model::ElemId { m: 0, e }).collect();
    a.selected = ids.iter().copied().collect();
    a.save_viewpoint();
    a.viewpoints[0].name = "Pipe through beam".into();
    a.viewpoints[0].comment = "move the pipe".into();
    a.shared_sync(true);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "one file per issue");
    // B preia issue-ul, cu aceleasi elemente (dupa GUID) si aceeasi camera
    b.shared_sync(true);
    assert_eq!(b.viewpoints.len(), 1);
    assert_eq!(b.viewpoints[0].name, "Pipe through beam");
    let mut sel = b.viewpoints[0].selected.clone();
    sel.sort();
    assert_eq!(sel, ids);
    assert!((glam::Vec3::from(b.viewpoints[0].cam.eye) - glam::Vec3::from(a.viewpoints[0].cam.eye)).length() < 1e-3);
    // B schimba starea -> A o vede
    std::thread::sleep(std::time::Duration::from_millis(20));
    b.viewpoints[0].issue.status = "Resolved".into();
    b.shared_sync(true);
    a.shared_sync(true);
    assert_eq!(a.viewpoints[0].issue.status, "Resolved");
    // A sterge -> dispare si la B
    std::thread::sleep(std::time::Duration::from_millis(20));
    a.viewpoints.clear();
    a.shared_sync(true);
    b.shared_sync(true);
    assert!(b.viewpoints.is_empty(), "deletion reaches the colleague");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tools_grids_levels_sections_and_project_roundtrip() {
    use bimrust::app::{generate_grid, grid_dxf, grid_label, SectionMode};
    assert_eq!((grid_label("A", 0), grid_label("A", 25), grid_label("A", 26), grid_label("1", 4)), ("A".into(), "Z".into(), "AA".into(), "5".into()));
    let g = generate_grid([100.0, 200.0], 4, 6.0, "1", 3, 5.0, "A", 2.0);
    assert_eq!(g.len(), 7);
    assert_eq!(g[3].name, "4");
    assert_eq!(g[3].a, [118.0, 198.0]);
    assert_eq!(g[6].name, "C");
    let dxf = grid_dxf(&g, 0.5);
    assert_eq!(dxf.matches("\nLINE\n").count(), 7);
    assert_eq!(dxf.matches("\nCIRCLE\n").count(), 14);
    assert!(dxf.ends_with("EOF\n"));

    let mut app = app();
    // axe proprii in jurul modelului -> locatia clash-urilor le foloseste
    let bb = app.scene_aabb();
    let o = app.origin_f64();
    app.user_grids = generate_grid([bb.min.x as f64 + o.x, bb.min.y as f64 + o.y], 3, 5.0, "1", 3, 5.0, "A", 1.0);
    app.use_user_grids = true;
    let loc = app.grid_location(glam::Vec3::new(bb.min.x + 0.2, bb.min.y + 0.3, bb.min.z + 0.5));
    assert!(loc.starts_with("A-1"), "{loc}");
    // sectiune pe nivel si taietura de plan
    let levels = app.scene_grids().levels;
    assert!(levels.len() >= 2, "levels from the models");
    app.section_level_box(0);
    assert!(app.section.enabled && app.section.mode == SectionMode::Box);
    assert!((app.section.bmin[2] - (levels[0].1 - 0.01)).abs() < 1e-4 && app.section.bmax[2] < levels[1].1);
    // marginile reglabile: placa de sub nivel inclusa
    app.level_below = 0.3;
    app.section_level_box(0);
    assert!((app.section.bmin[2] - (levels[0].1 - 0.31)).abs() < 1e-4);
    app.section_level_step(1);
    assert_eq!(app.section_level, Some(1));
    app.section_plan_cut(1, 1.2);
    assert_eq!(app.section.mode, SectionMode::Planes);
    assert!((app.section.planes[0].offset - (levels[1].1 + 1.2)).abs() < 1e-4);
    // niveluri proprii + proiect salvat / redeschis
    app.level.user = vec![bimrust::app::UserLevel { name: "P0".into(), elev: 0.0 }, bimrust::app::UserLevel { name: "P1".into(), elev: 3.5 }];
    app.level.custom = true;
    app.level.zero = 12.0;
    let path = std::env::temp_dir().join(format!("mb_tools_{}.bimr", std::process::id()));
    let p = path.to_string_lossy().to_string();
    // setarile (lista Recent) intr-un folder de test, nu in cele ale utilizatorului
    std::env::set_var("APPDATA", std::env::temp_dir().join("mb_test_appdata"));
    app.save_project(&p);
    let mut b = App::new(vec![]);
    b.load_project(&p);
    // starea proiectului se aplica dupa incarcarea modelelor
    let t0 = std::time::Instant::now();
    while !b.loads.is_empty() && t0.elapsed().as_secs() < 60 {
        b.poll_jobs();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    b.poll_jobs();
    assert_eq!(b.user_grids.len(), 6);
    assert!(b.use_user_grids && b.level.custom);
    assert_eq!(b.level.user[1].elev, 3.5);
    assert_eq!(b.level.zero, 12.0);
    let _ = std::fs::remove_file(&path);
    // proiect nou: gol, fara pagina de start
    b.new_project();
    assert!(b.blank_project && b.project_path.is_none() && b.loaded_models().next().is_none());
}

#[test]
fn grid_alignment_by_picking_and_rotation() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use bimrust::app::{transform_grids, UserGrid};
    let mut g = vec![UserGrid { name: "1".into(), a: [10.0, 0.0], b: [10.0, 20.0] }];
    transform_grids(&mut g, [10.0, 0.0], 90.0, [1.0, 2.0]);
    assert!((g[0].a[0] - 11.0).abs() < 1e-9 && (g[0].a[1] - 2.0).abs() < 1e-9);
    assert!((g[0].b[0] - (-9.0)).abs() < 1e-9 && (g[0].b[1] - 2.0).abs() < 1e-9);
    // alegere in 3D: originea, apoi directia la 30 de grade -> reteaua rotita
    let mut app = app();
    let o = app.origin_f64();
    app.grid_gen.nx = 2;
    app.grid_gen.ny = 2;
    app.grid_pick = 1;
    app.grid_pick_point(glam::Vec3::new(1.0, 1.0, 0.0));
    assert_eq!(app.grid_pick, 2);
    let a = 30f32.to_radians();
    app.grid_pick_point(glam::Vec3::new(1.0 + 10.0 * a.cos(), 1.0 + 10.0 * a.sin(), 0.0));
    assert_eq!(app.grid_pick, 0);
    assert!((app.grid_gen.angle_deg - 30.0).abs() < 0.02, "{}", app.grid_gen.angle_deg);
    assert_eq!(app.user_grids.len(), 4);
    // axa "A" (prima cu litera) trece prin origine, la 30 de grade
    let ga = app.user_grids.iter().find(|g| g.name == "A").unwrap();
    let d = (ga.b[1] - ga.a[1]).atan2(ga.b[0] - ga.a[0]).to_degrees();
    assert!((d - 30.0).abs() < 0.05, "{d}");
    let (ox, oy) = (1.0 + o.x, 1.0 + o.y);
    let cross = (ga.b[0] - ga.a[0]) * (oy - ga.a[1]) - (ga.b[1] - ga.a[1]) * (ox - ga.a[0]);
    assert!(cross.abs() < 1e-3, "axis A through the picked origin");
}

/// Proiect demo pentru verificarea vizuala (rulat manual): cargo test --release --test navis_xml demo_grid_project -- --ignored
#[test]
#[ignore]
fn demo_grid_project() {
    let mut app = app();
    let bb = app.scene_aabb();
    let o = app.origin_f64();
    app.grid_gen.origin = [bb.min.x as f64 + o.x, bb.min.y as f64 + o.y];
    app.grid_gen.nx = 5;
    app.grid_gen.dx = (bb.size().x as f64 / 4.0).max(1.0);
    app.grid_gen.ny = 3;
    app.grid_gen.dy = (bb.size().y as f64 / 2.0).max(1.0);
    app.generate_user_grid();
    app.grid_ext = 3.0;
    app.cam.fit(&bb);
    let out = std::env::var("MB_DEMO_PROJECT").unwrap_or_else(|_| std::env::temp_dir().join("mb_demo_grid.bimr").to_string_lossy().to_string());
    std::env::set_var("APPDATA", std::env::temp_dir().join("mb_test_appdata"));
    let abs = |p: &str| std::fs::canonicalize(p).unwrap().to_string_lossy().trim_start_matches(r"\\?\").to_string();
    let mut pf = app.project_file();
    pf.models = vec![abs("samples/Block_A_Architecture_Structure.ifc"), abs("samples/Block_A_MEP.ifc")];
    std::fs::write(&out, serde_json::to_string_pretty(&pf).unwrap()).unwrap();
}

#[test]
fn bimd_holds_project_and_models_in_one_file() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("APPDATA", std::env::temp_dir().join("mb_test_appdata"));
    let mut a = app();
    a.save_viewpoint();
    a.viewpoints[0].name = "Check duct".into();
    a.user_grids = bimrust::app::generate_grid([0.0, 0.0], 2, 5.0, "1", 2, 5.0, "A", 1.0);
    a.use_user_grids = true;
    let path = std::env::temp_dir().join(format!("mb_pub_{}.bimd", std::process::id()));
    let p = path.to_string_lossy().to_string();
    let msg = a.publish_bimd(&p).unwrap();
    assert!(msg.contains("2 models"), "{msg}");
    // deschis pe "alt PC": fara fisierele sursa, doar .bimd
    let mut b = App::new(vec![]);
    b.open_bimd(&p);
    let t0 = std::time::Instant::now();
    while !b.loads.is_empty() && t0.elapsed().as_secs() < 60 {
        b.poll_jobs();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    b.poll_jobs();
    let count = |x: &App| x.loaded_models().map(|(_, m)| m.elements.len()).sum::<usize>();
    assert_eq!(count(&b), count(&a), "every element back");
    assert!(b.loaded_models().all(|(_, m)| m.path.to_lowercase().ends_with(".bimc")), "models from inside the .bimd");
    assert_eq!(b.viewpoints.len(), 1);
    assert_eq!(b.viewpoints[0].name, "Check duct");
    assert_eq!(b.user_grids.len(), 4);
    assert_eq!(b.bimd_path.as_deref(), Some(p.as_str()), "Save goes back into the .bimd");
    assert!(b.project_path.is_none());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn start_page_actions_ask_before_closing_an_open_project() {
    use bimrust::app::StartAction;
    let mut a = app();
    a.show_start = true;
    a.start_action(StartAction::New);
    assert!(a.confirm_start, "an open project asks to save first");
    assert!(a.loaded_models().next().is_some(), "nothing closed yet");
    let pending = a.start_pending.clone().unwrap();
    a.run_start_action(pending);
    assert!(a.blank_project && !a.show_start && a.loaded_models().next().is_none());
    // fara proiect deschis: actiunea se face direct
    let mut b = App::new(vec![]);
    b.start_action(StartAction::New);
    assert!(!b.confirm_start && b.blank_project);
}

#[test]
fn grids_import_back_from_dxf() {
    use bimrust::app::{generate_grid, grid_dxf};
    let g = generate_grid([1000.0, 2000.0], 3, 7.5, "1", 2, 6.0, "A", 1.5);
    let path = std::env::temp_dir().join(format!("mb_grid_imp_{}.dxf", std::process::id()));
    std::fs::write(&path, grid_dxf(&g, 0.5)).unwrap();
    let r = bimrust::formats::grids_from_cad(&path.to_string_lossy(), None).unwrap();
    assert_eq!(r.layer, "GRID");
    assert_eq!(r.grids.len(), 5, "{:?}", r.grids.iter().map(|x| &x.0).collect::<Vec<_>>());
    for a in &g {
        let b = r.grids.iter().find(|x| x.0 == a.name).unwrap_or_else(|| panic!("axis {} with its tag", a.name));
        let close = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).hypot(p[1] - q[1]) < 1e-3;
        assert!((close(a.a, b.1) && close(a.b, b.2)) || (close(a.a, b.2) && close(a.b, b.1)), "{} position", a.name);
    }
    // alt strat ales manual
    assert!(bimrust::formats::grids_from_cad(&path.to_string_lossy(), Some("GRID_TAGS")).is_err());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn relocate_model_point_to_point_and_rotate() {
    use bimrust::app::XformMode;
    let mut a = app();
    // base_models = modelele incarcate (ca dupa incarcare), ca transformarile sa se poata reface
    a.base_models = a.models.clone();
    while a.model_xf.len() < a.models.len() {
        a.model_xf.push(Default::default());
    }
    let id = bimrust::model::ElemId { m: 0, e: 0 };
    let before = a.models[0].as_ref().unwrap().aabb;
    a.xform_start(XformMode::Move, false);
    a.xform_point(glam::Vec3::new(0.0, 0.0, 0.0), glam::Vec3::Z, id);
    a.xform_point(glam::Vec3::new(10.0, -5.0, 2.0), glam::Vec3::Z, id);
    assert!(a.xform.is_none());
    let after = a.models[0].as_ref().unwrap().aabb;
    assert!((after.min - before.min - glam::Vec3::new(10.0, -5.0, 2.0)).length() < 1e-3, "moved by the picked vector");
    // rotatie 90 de grade in jurul unui punct: punctul ramane pe loc
    let p = before.center() + glam::Vec3::new(10.0, -5.0, 2.0);
    a.xform_start(XformMode::Rotate, false);
    a.xform_point(p, glam::Vec3::Z, id);
    a.xform_point(p + glam::Vec3::X, glam::Vec3::Z, id);
    a.xform_point(p + glam::Vec3::Y, glam::Vec3::Z, id);
    assert!((a.model_xf[0].rot_z - 90.0).abs() < 1e-3, "{}", a.model_xf[0].rot_z);
    let c = a.models[0].as_ref().unwrap().aabb.center();
    assert!((c.truncate() - p.truncate()).length() < 1e-2, "rotated about the picked pivot");
}

#[test]
fn clash_matrix_counts_pairs_of_models() {
    let mut app = app();
    app.clash_tests[0].a = Selector { kind: SelKind::Model, value: "Block_A_Architecture_Structure.ifc".into() };
    app.clash_tests[0].b = Selector { kind: SelKind::Model, value: "Block_A_MEP.ifc".into() };
    app.run_clash_blocking(0);
    let n = app.clash_tests[0].results.len();
    assert!(n > 0);
    app.clash_tests[0].results[0].status = bimrust::clash::ClashStatus::Resolved;
    let m = app.clash_matrix();
    assert_eq!(m.models, vec!["Block_A_Architecture_Structure".to_string(), "Block_A_MEP".to_string()]);
    let c = m.get(1, 0);
    assert_eq!(c[0] + c[1] + c[2], n);
    assert_eq!(c[2], 1, "the resolved one");
    let p = std::env::temp_dir().join(format!("mb_matrix_{}.xlsx", std::process::id()));
    app.export_clash_matrix(&p.to_string_lossy());
    let rows = bimrust::tables::xlsx_rows(std::fs::read(&p).unwrap()).unwrap();
    assert!(rows.iter().any(|r| r.first().map(|s| s == "Block_A_Architecture_Structure").unwrap_or(false)));
    let _ = std::fs::remove_file(&p);
}
