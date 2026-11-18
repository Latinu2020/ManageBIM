//! Refresh (F5): modelul se reincarca din fisierul modificat, iar selectia / ascunderile /
//! culorile se pastreaza pe GUID.
use bimrust::app::*;
use bimrust::model::ElemId;
use bimrust::*;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

/// copie a unui IFC demo intr-un folder temporar propriu testului
fn temp_ifc(tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!("managebim_refresh_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("Block_A_MEP.ifc");
    std::fs::copy("samples/Block_A_MEP.ifc", &p).unwrap();
    p.to_string_lossy().to_string()
}

/// incarca fisierul direct in slotul 0 (fara open_path, ca sa nu atingem lista Recent a utilizatorului)
fn app_with(path: &str) -> App {
    let p = |_: f32, _: &str| {};
    let raw = load_any(path, &p).unwrap();
    let o = raw.center();
    let mut app = App::new(vec![]);
    app.settings.use_cache = false;
    app.settings.auto_cache = false;
    *app.origin.lock().unwrap() = Some(o);
    let m = Arc::new(build_model(raw, path, o));
    app.push_slot();
    app.base_models[0] = Some(m.clone());
    app.models[0] = Some(m);
    app.file_stamps.insert(0, std::fs::metadata(path).unwrap().modified().unwrap());
    app
}

fn wait_loads(app: &mut App) {
    let t = Instant::now();
    while !app.loads.is_empty() {
        app.poll_jobs();
        assert!(t.elapsed() < Duration::from_secs(60), "reload timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn bump_mtime(path: &str, content: &str) {
    std::fs::write(path, content).unwrap();
    // garantam o data diferita chiar pe sisteme de fisiere cu rezolutie mica
    let f = std::fs::File::options().write(true).open(path).unwrap();
    f.set_modified(SystemTime::now() + Duration::from_secs(5)).unwrap();
}

#[test]
fn refresh_detects_change_and_keeps_state() {
    let path = temp_ifc("keep");
    let mut app = app_with(&path);
    let n = app.models[0].as_ref().unwrap().elements.len();
    assert!(n > 5);

    // nimic modificat -> nimic de reincarcat
    app.refresh_models(false);
    assert!(app.loads.is_empty());
    assert!(app.changed_files.is_empty());

    // stare utilizator
    let sel = ElemId { m: 0, e: 1 };
    let hid = ElemId { m: 0, e: 2 };
    let col = ElemId { m: 0, e: 3 };
    let guid_sel = app.models[0].as_ref().unwrap().elements[1].guid.clone();
    let guid_hid = app.models[0].as_ref().unwrap().elements[2].guid.clone();
    assert!(!guid_sel.is_empty());
    app.selected.insert(sel);
    app.primary = Some(sel);
    app.hidden.insert(hid);
    app.overrides.insert(col, [255, 0, 0, 255]);

    // fisierul e rescris (acelasi continut) -> detectat ca modificat
    let txt = std::fs::read_to_string(&path).unwrap();
    bump_mtime(&path, &txt);
    app.last_file_check = Instant::now() - Duration::from_secs(5);
    app.check_file_changes();
    assert_eq!(app.changed_files.iter().copied().collect::<Vec<_>>(), vec![0]);

    app.refresh_models(false);
    assert_eq!(app.loads.len(), 1);
    // in timpul reincarcarii modelul vechi ramane pe ecran
    assert!(app.models[0].is_some());
    wait_loads(&mut app);

    let m = app.models[0].as_ref().unwrap().clone();
    assert_eq!(m.elements.len(), n);
    assert!(app.changed_files.is_empty());
    assert!(app.reloading.is_empty());
    let sel_now: Vec<_> = app.selected.iter().copied().collect();
    assert_eq!(sel_now.len(), 1);
    assert_eq!(m.elements[sel_now[0].e as usize].guid, guid_sel);
    assert_eq!(app.primary, Some(sel_now[0]));
    assert!(app.hidden.iter().any(|i| m.elements[i.e as usize].guid == guid_hid));
    assert_eq!(app.overrides.len(), 1);
}

#[test]
fn refresh_drops_elements_whose_guid_disappeared() {
    let path = temp_ifc("gone");
    let mut app = app_with(&path);
    let n = app.models[0].as_ref().unwrap().elements.len();
    let (keep, gone) = (ElemId { m: 0, e: 1 }, ElemId { m: 0, e: 4 });
    let guid_keep = app.models[0].as_ref().unwrap().elements[1].guid.clone();
    let guid_gone = app.models[0].as_ref().unwrap().elements[4].guid.clone();
    app.selected.insert(keep);
    app.selected.insert(gone);
    app.hidden.insert(gone);

    // elementul 4 primeste alt GUID in fisier = "sters" + "element nou"
    let txt = std::fs::read_to_string(&path).unwrap();
    assert_eq!(txt.matches(&guid_gone).count(), 1, "GUID must appear exactly once in the sample");
    let new_guid: String = guid_gone.chars().rev().collect();
    bump_mtime(&path, &txt.replace(&guid_gone, &new_guid));

    app.refresh_models(false);
    wait_loads(&mut app);

    let m = app.models[0].as_ref().unwrap().clone();
    assert_eq!(m.elements.len(), n);
    let sel_guids: Vec<String> = app.selected.iter().map(|i| m.elements[i.e as usize].guid.clone()).collect();
    assert_eq!(sel_guids, vec![guid_keep]);
    assert!(!app.hidden.iter().any(|i| i.m == 0 && m.elements[i.e as usize].guid == new_guid));
    assert!(app.log.iter().any(|l| l.contains("1 new, 1 removed")), "log: {:?}", app.log);
}

#[test]
fn refresh_force_reloads_unchanged_and_reports_missing_file() {
    let path = temp_ifc("force");
    let mut app = app_with(&path);
    app.refresh_models(true);
    assert_eq!(app.loads.len(), 1);
    wait_loads(&mut app);
    assert!(app.models[0].is_some());

    std::fs::remove_file(&path).unwrap();
    app.refresh_models(true);
    assert!(app.loads.is_empty());
    assert!(app.models[0].is_some(), "the model stays on screen when its file is gone");
    assert!(app.log.iter().any(|l| l.contains("file not found")));
}
