//! Export / import: Appearance Profiler (.bimprofile), Compare (.bimcmp + CSV), arbore de selectie (.bimtree).
use bimrust::app::*;
use bimrust::compare::Diff;
use bimrust::model::ElemId;
use bimrust::profiler::ProfileMode;
use bimrust::*;
use std::sync::{Arc, Mutex};

const ARCH: &str = "samples/Block_A_Architecture_Structure.ifc";
const MEP: &str = "samples/Block_A_MEP.ifc";
// APPDATA is process-global: serialize tests that redirect it.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn tmp(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("managebim_xio_{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    // setarile (arborele) se scriu aici, nu in %APPDATA% al utilizatorului
    std::env::set_var("APPDATA", &d);
    d.join(name).to_string_lossy().to_string()
}

fn app_with(paths: &[&str]) -> App {
    let p = |_: f32, _: &str| {};
    let mut app = App::new(vec![]);
    let mut o = None;
    for path in paths {
        let raw = load_any(path, &p).unwrap();
        let oo = *o.get_or_insert(raw.center());
        let m = build_model(raw, path, oo);
        app.push_slot();
        let i = app.models.len() - 1;
        app.trees[i] = Some(ModelTree::build(&m, &app.settings.tree_levels));
        app.models[i] = Some(Arc::new(m));
    }
    app.refresh_caches();
    app
}

#[test]
fn profile_export_import_keeps_custom_colours_on_another_model() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let file = tmp("types.bimprofile");
    let mut a = app_with(&[ARCH]);
    a.profile.cat = ITEM_CAT.into();
    a.profile.prop = "IFC Type".into();
    a.profile.mode = ProfileMode::Values;
    a.profile_generate();
    // culorile mele
    let wall = a.profile.rules.iter().position(|r| r.label.eq_ignore_ascii_case("IfcWall")).expect("walls in the sample");
    a.profile.rules[wall].color = [12, 34, 56, 255];
    a.profile.rules[0].visible = false;
    let n_rules = a.profile.rules.len();
    a.handle_path(Purpose::ExportProfile, file.clone());

    // alt model (MEP): aceleasi reguli si culori, numaratori refacute pe modelul nou
    let mut b = app_with(&[MEP]);
    b.handle_path(Purpose::ImportProfile, file.clone());
    assert!(b.profile.active);
    assert_eq!(b.profile.rules.len(), n_rules);
    assert_eq!(b.profile.rules[wall].color, [12, 34, 56, 255]);
    assert!(!b.profile.rules[0].visible);
    let m = b.models[0].clone().unwrap();
    let matched: u32 = b.profile.rules.iter().map(|r| r.count).sum();
    let mep_types_in_profile = m.elements.iter().filter(|e| a.profile.rules.iter().any(|r| r.label.eq_ignore_ascii_case(&e.ifc_type))).count();
    assert_eq!(matched as usize, mep_types_in_profile);

    // tipurile MEP care lipsesc din profil primesc culori noi, cele vechi raman neatinse
    let missing = b.profile_uncovered();
    assert!(missing > 0, "MEP has types the architecture profile does not know");
    b.profile_add_missing();
    assert_eq!(b.profile.rules.len(), n_rules + missing);
    assert_eq!(b.profile.rules[wall].color, [12, 34, 56, 255]);
    assert_eq!(b.profile.rules.iter().map(|r| r.count as usize).sum::<usize>(), m.elements.len());
    assert_eq!(b.profile_uncovered(), 0);
}

#[test]
fn profile_ranges_can_be_edited_by_hand() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut a = app_with(&[ARCH]);
    a.profile.cat = "Element".into();
    a.profile.prop = "Level elevation".into();
    a.profile.mode = ProfileMode::Ranges;
    a.profile.bins = 2;
    a.profile_generate();
    let total: u32 = a.profile.rules.iter().map(|r| r.count).sum();
    // intervalele mele: tot ce e sub 1 m intr-o regula, restul in alta
    a.profile.rules[0].lo = Some(-100.0);
    a.profile.rules[0].hi = Some(1.0);
    a.profile.rules[1].lo = Some(1.0);
    a.profile.rules[1].hi = Some(1000.0);
    a.profile.relabel_range(0);
    a.profile.relabel_range(1);
    a.profile_recount();
    assert_eq!(a.profile.rules.iter().map(|r| r.count).sum::<u32>(), total);
    assert!(a.profile.rules[0].label.starts_with("-100 – 1"), "{}", a.profile.rules[0].label);
    a.profile.add_rule();
    assert_eq!(a.profile.rules[2].lo, Some(1000.0));
}

#[test]
fn compare_save_open_and_csv() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let p = |_: f32, _: &str| {};
    let old = load_any(ARCH, &p).unwrap();
    let mut new = load_any(ARCH, &p).unwrap();
    new.elements[1].name = "Renamed".into();
    new.elements[2].guid = "NEWGUID0000000000000000".into();
    for t in new.elements[5].tris.iter_mut() {
        for v in t.iter_mut() {
            v.z += 0.1;
        }
    }
    let o = old.center();
    let build = |o: glam::DVec3, old, new| {
        let mut app = App::new(vec![]);
        app.models.push(Some(Arc::new(build_model(old, "v1.ifc", o))));
        app.models.push(Some(Arc::new(build_model(new, "v2.ifc", o))));
        app.compare_pick = (Some(0), Some(1));
        app
    };
    let mut a = build(o, old, new);
    a.run_compare();
    let file = tmp("cmp.bimcmp");
    let csv = tmp("cmp.csv");
    a.handle_path(Purpose::ExportCompare, file.clone());
    a.handle_path(Purpose::ExportCompareCsv, csv.clone());
    let r1 = a.compare.as_ref().unwrap();

    // aceleasi modele, alta sesiune: deschidem comparatia salvata
    let old = load_any(ARCH, &p).unwrap();
    let mut new = load_any(ARCH, &p).unwrap();
    new.elements[1].name = "Renamed".into();
    new.elements[2].guid = "NEWGUID0000000000000000".into();
    for t in new.elements[5].tris.iter_mut() {
        for v in t.iter_mut() {
            v.z += 0.1;
        }
    }
    let mut b = build(o, old, new);
    b.compare_pick = (None, None); // modelele se gasesc dupa numele din fisier
    b.handle_path(Purpose::ImportCompare, file.clone());
    let r2 = b.compare.as_ref().expect("comparison imported");
    assert!(b.compare_on);
    assert_eq!(r1.counts, r2.counts);
    assert_eq!(r1.map, r2.map);
    assert_eq!(r1.matched_a, r2.matched_a);
    assert_eq!(r1.changes, r2.changes);
    assert_eq!(r2.counts[Diff::ALL.iter().position(|d| *d == Diff::Geometry).unwrap()], 1);

    let text = std::fs::read_to_string(&csv).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].contains("Status;GlobalId;Name"));
    let changed = r1.counts[0] + r1.counts[1] + r1.counts[2] + r1.counts[3];
    assert_eq!(lines.len() - 1, changed);
    assert!(text.contains("Renamed") && text.contains("NEWGUID0000000000000000"));
}

#[test]
fn tree_grouping_by_any_property_and_export_import() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let file = tmp("tree.bimtree");
    let mut app = app_with(&[ARCH]);
    let m = app.models[0].clone().unwrap();
    // implicit: Level > IFC Type
    let t = app.trees[0].as_ref().unwrap();
    let levels: std::collections::BTreeSet<&str> = m.elements.iter().map(|e| e.storey.as_str()).collect();
    assert_eq!(t.roots.len(), levels.len());
    assert!(t.roots.iter().all(|n| !n.children.is_empty()));

    // IFC Type > Level > Type Name
    let custom = vec![TreeLevel::new(ITEM_CAT, "IFC Type"), TreeLevel::new(ITEM_CAT, "Level"), TreeLevel::new("Type", "Type Name")];
    app.set_tree_levels(custom.clone());
    let t = app.trees[0].as_ref().unwrap();
    let types: std::collections::BTreeSet<&str> = m.elements.iter().map(|e| e.ifc_type.as_str()).collect();
    assert_eq!(t.roots.len(), types.len());
    let total: usize = t.roots.iter().map(|n| n.ids.len()).sum();
    assert_eq!(total, m.elements.len());
    // al treilea nivel exista si e frunza
    let leaf = &t.roots[0].children[0].children[0];
    assert!(leaf.children.is_empty() && !leaf.ids.is_empty());
    // ordine naturala: "(no ...)" la final
    if let Some(last) = t.roots.last() {
        assert!(!t.roots[..t.roots.len() - 1].iter().any(|n| n.label.starts_with("(no ")), "{}", last.label);
    }

    app.handle_path(Purpose::ExportTree, file.clone());
    app.set_tree_levels(Vec::new()); // lista plata
    assert!(app.trees[0].as_ref().unwrap().roots.is_empty());
    app.handle_path(Purpose::ImportTree, file.clone());
    assert_eq!(app.settings.tree_levels, custom);
    assert_eq!(app.trees[0].as_ref().unwrap().roots.len(), types.len());

    // un element ales ramane accesibil prin arbore
    let id = ElemId { m: 0, e: 3 };
    assert!(app.trees[0].as_ref().unwrap().roots.iter().any(|n| n.ids.contains(&id.e)));
}
