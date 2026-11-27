//! General check: the demo project goes through the main functions, like a user.
//! Any error reported by the application (error toast) in a step that must work fails the test.

use bimrust::app::App;
use std::path::PathBuf;

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn wait(app: &mut App) {
    let t0 = std::time::Instant::now();
    while (!app.loads.is_empty() || app.clash.job.is_some()) && t0.elapsed().as_secs() < 120 {
        app.poll_jobs();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    app.poll_jobs();
}

/// the errors raised since the last call
fn errors(app: &mut App) -> Vec<String> {
    let e: Vec<String> = app.toasts.iter().filter(|t| t.2).map(|t| t.0.clone()).collect();
    app.toasts.clear();
    e
}

fn file_ok(p: &PathBuf, min: u64) {
    let n = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    assert!(n >= min, "{} missing or too small ({n} bytes)", p.display());
}

#[test]
fn demo_project_end_to_end() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // isolated settings and data (they do not touch the user folders)
    let root = std::env::temp_dir().join(format!("mb_smoke_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("appdata")).unwrap();
    std::fs::create_dir_all(root.join("local")).unwrap();
    std::fs::create_dir_all(root.join("out")).unwrap();
    std::env::set_var("APPDATA", root.join("appdata"));
    std::env::set_var("LOCALAPPDATA", root.join("local"));
    let out = root.join("out");
    let o = |n: &str| out.join(n);
    let s = |p: &PathBuf| p.to_string_lossy().to_string();

    // ---- 1. the demo project: two models + the 4D schedule
    let mut app = App::new(vec![]);
    app.open_demo();
    wait(&mut app);
    assert!(errors(&mut app).is_empty());
    let models: Vec<usize> = app.loaded_models().map(|(i, _)| i).collect();
    assert_eq!(models.len(), 2, "both demo models load");
    let n_elems: usize = app.loaded_models().map(|(_, m)| m.elements.len()).sum();
    assert!(n_elems > 100, "{n_elems} elements");
    assert!(!app.tasks.is_empty(), "4D schedule imported");

    // ---- 2. selection, Select Same Type, hiding, Undo / Redo
    app.lod_moved = std::time::Instant::now() - std::time::Duration::from_secs(5);
    app.history_tick(false);
    let first = app.all_ids()[0];
    app.apply(bimrust::app::Action::Select(vec![first], false));
    app.select_similar(false);
    assert!(app.selected.len() > 1, "same category selects more items");
    app.history_tick(false);
    let sel: Vec<_> = app.selected.iter().copied().collect();
    app.apply(bimrust::app::Action::Hide(sel));
    app.history_tick(false);
    app.undo_view();
    assert!(app.hidden.is_empty(), "undo brings the items back");
    app.redo_view();
    assert!(!app.hidden.is_empty());
    app.apply(bimrust::app::Action::ShowAll);
    app.clear_selection();
    assert!(errors(&mut app).is_empty());

    // ---- 3. clash: architecture+structure against MEP, with the smart rules
    let mut t = bimrust::app::ClashTest::new("Arch vs MEP".into());
    t.a = bimrust::app::Selector { kind: bimrust::app::SelKind::Model, value: app.models[models[0]].as_ref().unwrap().name.clone() };
    t.b = bimrust::app::Selector { kind: bimrust::app::SelKind::Model, value: app.models[models[1]].as_ref().unwrap().name.clone() };
    app.clash.tests.push(t);
    let ti = app.clash.tests.len() - 1;
    app.cur_test = ti;
    app.run_clash_blocking(ti);
    wait(&mut app);
    assert!(errors(&mut app).is_empty());
    let n_clash = app.clash.tests[ti].results.len();
    let n_tests = app.clash.tests.len();
    if n_clash > 0 {
        app.export_clash_bcf(&s(&o("clashes.bcf")));
        file_ok(&o("clashes.bcf"), 200);
        app.export_clash_matrix(&s(&o("matrix.xlsx")));
        file_ok(&o("matrix.xlsx"), 1000);
    }

    // ---- 4. Model Checker: starter rules, run, reports, IDS round trip
    app.check_rules.extend(bimrust::check::starter_rules());
    app.run_check();
    assert!(app.check_results.is_some());
    app.export_check_xlsx(&s(&o("check.xlsx")));
    file_ok(&o("check.xlsx"), 1000);
    app.export_ids(&s(&o("rules.ids")));
    file_ok(&o("rules.ids"), 300);
    let n_rules = app.check_rules.len();
    app.check_rules.clear();
    app.import_ids(&s(&o("rules.ids")));
    assert_eq!(app.check_rules.len(), n_rules, "IDS import gives the same rules back");
    assert!(errors(&mut app).is_empty());

    // ---- 5. viewpoints in folders + Navisworks export
    app.vp_folder_cur = "Level 1/MEP".into();
    app.save_viewpoint();
    app.vp_folder_cur.clear();
    app.save_viewpoint();
    assert_eq!(app.viewpoints[0].folder, "Level 1/MEP");
    app.export_navis_xml(&s(&o("viewpoints.xml")));
    let xml = std::fs::read_to_string(o("viewpoints.xml")).unwrap();
    assert!(xml.contains("<viewfolder name=\"Level 1\""));

    // ---- 6. exports: IFC, web viewer, DWG (the level section: 2D and on sheet)
    app.ifc_selection = false;
    app.export_ifc(&s(&o("export.ifc")));
    file_ok(&o("export.ifc"), 10_000);
    app.web_selection = false;
    app.export_web(&s(&o("model.html")));
    file_ok(&o("model.html"), 10_000);
    if !app.scene_grids().levels.is_empty() {
        app.section_level_box(0);
        app.cad_sheet = false;
        app.export_section_cad(&s(&o("plan.dwg")));
        file_ok(&o("plan.dwg"), 1000);
        app.cad_sheet = true;
        app.export_section_cad(&s(&o("sheet.dwg")));
        file_ok(&o("sheet.dwg"), 1000);
        app.section.enabled = false;
    }
    assert!(errors(&mut app).is_empty());
    // the exported IFC is reopened
    let raw = bimrust::load_any(&s(&o("export.ifc")), &|_, _| {}).expect("exported IFC reopens");
    assert!(raw.elements.len() * 10 >= n_elems * 9, "exported IFC keeps the elements ({} of {n_elems})", raw.elements.len());

    // ---- 7. save + reopen project (with .bak on the second save)
    let proj = o("demo.bimr");
    app.save_project(&s(&proj));
    app.save_project(&s(&proj));
    file_ok(&proj, 500);
    file_ok(&bimrust::formats::backup_path(&proj), 500);
    let mut b = App::new(vec![]);
    b.load_project(&s(&proj));
    wait(&mut b);
    assert!(errors(&mut b).is_empty());
    assert_eq!(b.loaded_models().count(), 2);
    assert_eq!(b.viewpoints.len(), 2);
    assert_eq!(b.viewpoints[0].folder, "Level 1/MEP");
    assert_eq!(b.check_rules.len(), n_rules);
    assert_eq!(b.clash.tests.len(), n_tests);
    assert_eq!(b.clash.tests[ti].results.len(), n_clash, "clash results kept");

    let _ = std::fs::remove_dir_all(&root);
}

/// Preview (manual): the full plan of the demo project as DXF + SVG in target/ (cargo test -- --ignored plan_preview).
#[test]
#[ignore]
fn plan_preview() {
    let root = std::env::temp_dir().join(format!("mb_prev_{}", std::process::id()));
    std::fs::create_dir_all(root.join("a")).unwrap();
    std::fs::create_dir_all(root.join("l")).unwrap();
    std::env::set_var("APPDATA", root.join("a"));
    std::env::set_var("LOCALAPPDATA", root.join("l"));
    let mut app = App::new(vec![]);
    app.open_demo();
    wait(&mut app);
    let li = std::env::var("MB_LEVEL").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    // the demo has no grids: grids on the columns (5 x 3) for the preview
    let bb = app.scene_aabb();
    let o = app.origin_f64();
    let (x0, y0, x1, y1) = (bb.min.x as f64 + o.x, bb.min.y as f64 + o.y, bb.max.x as f64 + o.x, bb.max.y as f64 + o.y);
    app.user_grids = (0..5).map(|i| { let x = x0 + 0.25 + (x1 - x0 - 0.5) * i as f64 / 4.0; bimrust::app::UserGrid { name: (i + 1).to_string(), a: [x, y0 - 1.5], b: [x, y1 + 1.5] } })
        .chain((0..3).map(|j| { let y = y0 + 0.25 + (y1 - y0 - 0.5) * j as f64 / 2.0; bimrust::app::UserGrid { name: ((b'A' + j as u8) as char).to_string(), a: [x0 - 1.5, y], b: [x1 + 1.5, y] } }))
        .collect();
    app.use_user_grids = std::env::var("MB_GRIDS").is_ok();
    app.section_level_box(li);
    for (name, sheet) in [("plan_preview", false), ("sheet_preview", true)] {
        app.cad_sheet = sheet;
        let p = std::path::Path::new("target").join(format!("{name}.dxf"));
        app.export_section_cad(&p.to_string_lossy());
        let dxf = std::fs::read_to_string(&p).unwrap();
        // DXF -> SVG (LINE and TEXT)
        let g: Vec<&str> = dxf.lines().collect();
        let (mut lines, mut texts) = (Vec::new(), Vec::new());
        let mut i = 0;
        while i + 1 < g.len() {
            if g[i].trim() == "0" && (g[i + 1] == "LINE" || g[i + 1] == "TEXT") {
                let kind = g[i + 1];
                let mut j = i + 2;
                let mut v = std::collections::HashMap::new();
                while j + 1 < g.len() && g[j].trim() != "0" {
                    v.insert(g[j].trim().to_string(), g[j + 1].to_string());
                    j += 2;
                }
                let f = |k: &str| v.get(k).and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(0.0);
                if kind == "LINE" {
                    lines.push((v.get("8").cloned().unwrap_or_default(), f("10"), f("20"), f("11"), f("21")));
                } else {
                    texts.push((f("10"), f("20"), f("40"), f("50"), v.get("1").cloned().unwrap_or_default()));
                }
                i = j;
            } else {
                i += 2;
            }
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for l in &lines {
            x0 = x0.min(l.1).min(l.3);
            x1 = x1.max(l.1).max(l.3);
            y0 = y0.min(l.2).min(l.4);
            y1 = y1.max(l.2).max(l.4);
        }
        let col = |l: &str| match l {
            "HATCH" => "#999",
            "GRIDS" => "#c00",
            "DIMS" => "#06c",
            "LEVELS" => "#080",
            "SHEET" => "#000",
            _ => "#111",
        };
        let sw = (x1 - x0).max(y1 - y0) / 1600.0;
        let mut svg = format!("<svg xmlns='http://www.w3.org/2000/svg' width='1600' height='{:.0}' viewBox='{} {} {} {}' style='background:#fff'>", 1600.0 * (y1 - y0) / (x1 - x0), x0, -y1, x1 - x0, y1 - y0);
        for l in &lines {
            svg += &format!("<line x1='{}' y1='{}' x2='{}' y2='{}' stroke='{}' stroke-width='{sw}'/>", l.1, -l.2, l.3, -l.4, col(&l.0));
        }
        for t in &texts {
            svg += &format!("<text x='{}' y='{}' font-size='{}' font-family='Arial' transform='rotate({} {} {})'>{}</text>", t.0, -t.1, t.2, -t.3, t.0, -t.1, t.4.replace('&', "&amp;").replace('<', "&lt;"));
        }
        svg += "</svg>";
        std::fs::write(std::path::Path::new("target").join(format!("{name}.svg")), svg).unwrap();
    }
}

/// Preview (manual): demo project with a level section and a low sun, for checking the shadows
/// (cargo test -- --ignored shadow_preview_project; MB_OUT = the .bimr path).
#[test]
#[ignore]
fn shadow_preview_project() {
    let out = std::env::var("MB_OUT").expect("MB_OUT");
    let root = std::env::temp_dir().join(format!("mb_sh_{}", std::process::id()));
    std::fs::create_dir_all(root.join("a")).unwrap();
    std::fs::create_dir_all(root.join("l")).unwrap();
    std::env::set_var("APPDATA", root.join("a"));
    std::env::set_var("LOCALAPPDATA", root.join("l"));
    let mut app = App::new(vec![]);
    app.open_demo();
    wait(&mut app);
    app.section_level_box(0);
    app.render.sun_az = 210.0 - 360.0;
    app.render.sun_el = 28.0;
    app.render.shadows = std::env::var("MB_NOSHADOW").is_err();
    app.save_project(&out);
    assert!(std::path::Path::new(&out).is_file());
}
