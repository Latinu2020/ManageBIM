//! Verificare generala: proiectul demo trece prin functiile principale, ca un utilizator.
//! Orice eroare raportata de aplicatie (toast de eroare) intr-un pas care trebuie sa mearga pica testul.

use bimrust::app::App;
use std::path::PathBuf;

fn wait(app: &mut App) {
    let t0 = std::time::Instant::now();
    while (!app.loads.is_empty() || app.clash_job.is_some()) && t0.elapsed().as_secs() < 120 {
        app.poll_jobs();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    app.poll_jobs();
}

/// erorile aparute de la ultimul apel
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
    // setari si date izolate (nu ating folderele utilizatorului)
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

    // ---- 1. proiectul demo: doua modele + planificarea 4D
    let mut app = App::new(vec![]);
    app.open_demo();
    wait(&mut app);
    assert!(errors(&mut app).is_empty());
    let models: Vec<usize> = app.loaded_models().map(|(i, _)| i).collect();
    assert_eq!(models.len(), 2, "both demo models load");
    let n_elems: usize = app.loaded_models().map(|(_, m)| m.elements.len()).sum();
    assert!(n_elems > 100, "{n_elems} elements");
    assert!(!app.tasks.is_empty(), "4D schedule imported");

    // ---- 2. selectie, Select Same Type, ascundere, Undo / Redo
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

    // ---- 3. clash: arhitectura+structura fata de instalatii, cu regulile inteligente
    let mut t = bimrust::app::ClashTest::new("Arch vs MEP".into());
    t.a = bimrust::app::Selector { kind: bimrust::app::SelKind::Model, value: app.models[models[0]].as_ref().unwrap().name.clone() };
    t.b = bimrust::app::Selector { kind: bimrust::app::SelKind::Model, value: app.models[models[1]].as_ref().unwrap().name.clone() };
    app.clash_tests.push(t);
    let ti = app.clash_tests.len() - 1;
    app.cur_test = ti;
    app.run_clash_blocking(ti);
    wait(&mut app);
    assert!(errors(&mut app).is_empty());
    let n_clash = app.clash_tests[ti].results.len();
    let n_tests = app.clash_tests.len();
    if n_clash > 0 {
        app.export_clash_bcf(&s(&o("clashes.bcf")));
        file_ok(&o("clashes.bcf"), 200);
        app.export_clash_matrix(&s(&o("matrix.xlsx")));
        file_ok(&o("matrix.xlsx"), 1000);
    }

    // ---- 4. Model Checker: reguli de pornire, rulare, rapoarte, IDS dus-intors
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

    // ---- 5. viewpoints in foldere + export Navisworks
    app.vp_folder_cur = "Level 1/MEP".into();
    app.save_viewpoint();
    app.vp_folder_cur.clear();
    app.save_viewpoint();
    assert_eq!(app.viewpoints[0].folder, "Level 1/MEP");
    app.export_navis_xml(&s(&o("viewpoints.xml")));
    let xml = std::fs::read_to_string(o("viewpoints.xml")).unwrap();
    assert!(xml.contains("<viewfolder name=\"Level 1\""));

    // ---- 6. exporturi: IFC, viewer web, DWG (sectiunea pe nivel: 2D si pe plansa)
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
    // IFC-ul exportat se redeschide
    let raw = bimrust::load_any(&s(&o("export.ifc")), &|_, _| {}).expect("exported IFC reopens");
    assert!(raw.elements.len() * 10 >= n_elems * 9, "exported IFC keeps the elements ({} of {n_elems})", raw.elements.len());

    // ---- 7. salvare + redeschidere proiect (cu .bak la a doua salvare)
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
    assert_eq!(b.clash_tests.len(), n_tests);
    assert_eq!(b.clash_tests[ti].results.len(), n_clash, "clash results kept");

    let _ = std::fs::remove_dir_all(&root);
}
