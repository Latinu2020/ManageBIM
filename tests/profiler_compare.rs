use bimrust::app::*;
use bimrust::compare::Diff;
use bimrust::model::ElemId;
use bimrust::profiler::{ProfileMode, Unmatched, GHOST};
use bimrust::*;
use glam::DVec3;
use std::sync::Arc;

const ARCH: &str = "samples/Block_A_Architecture_Structure.ifc";

fn app_with(paths: &[&str]) -> App {
    let p = |_: f32, _: &str| {};
    let mut app = App::new(vec![]);
    let mut o = None;
    for path in paths {
        let raw = load_any(path, &p).unwrap();
        let oo = *o.get_or_insert(raw.center());
        app.models.push(Some(Arc::new(build_model(raw, path, oo))));
    }
    app.refresh_caches();
    app
}

#[test]
fn profiler_by_level_colours_every_storey() {
    let mut app = app_with(&[ARCH]);
    app.profile.cat = ITEM_CAT.into();
    app.profile.prop = "Level".into();
    app.profile.mode = ProfileMode::Values;
    app.profile_generate();
    assert!(app.profile.active);
    let m = app.models[0].clone().unwrap();
    let levels: std::collections::BTreeSet<&str> = m.elements.iter().map(|e| e.storey.as_str()).filter(|s| !s.is_empty()).collect();
    assert_eq!(app.profile.rules.len(), levels.len());
    assert_eq!(app.profile.rules.iter().map(|r| r.count as usize).sum::<usize>(), m.elements.iter().filter(|e| !e.storey.is_empty()).count());

    app.rebuild_colors();
    let looks = &app.looks[&0];
    for (i, e) in m.elements.iter().enumerate() {
        let r = app.profile.rules.iter().find(|r| r.label == e.storey).unwrap();
        assert_eq!(looks[i].col, Some(r.color), "element {i} on {}", e.storey);
    }

    // a hidden rule -> its elements hidden; a manual override wins over the profile
    app.profile.rules[0].visible = false;
    let manual = ElemId { m: 0, e: 0 };
    app.overrides.insert(manual, [1, 2, 3, 255]);
    app.colors_dirty = true;
    app.rebuild_colors();
    let looks = &app.looks[&0];
    let hidden_label = app.profile.rules[0].label.clone();
    for (i, e) in m.elements.iter().enumerate() {
        if i == 0 {
            assert_eq!(looks[i].col, Some([1, 2, 3, 255]));
        } else if e.storey == hidden_label {
            assert!(looks[i].hidden);
        }
    }
}

#[test]
fn profiler_by_range_and_select() {
    let mut app = app_with(&[ARCH]);
    app.profile.cat = ITEM_CAT.into();
    app.profile.prop = "Level".into();
    app.profile.mode = ProfileMode::Ranges;
    app.profile_generate(); // "Level" is not numeric
    assert!(app.profile.rules.is_empty() && !app.profile.active);

    // "Level elevation" e numeric ("3.000 m")
    app.profile.cat = "Element".into();
    app.profile.prop = "Level elevation".into();
    app.profile.bins = 3;
    app.profile.unmatched = Unmatched::Ghost;
    app.profile_generate();
    assert!(!app.profile.rules.is_empty());
    assert!(app.profile.rules[0].label.ends_with(" m"), "{}", app.profile.rules[0].label);
    app.profile_select(0, false);
    assert_eq!(app.selected.len() as u32, app.profile.rules[0].count);

    // the project keeps the profile
    let json = serde_json::to_string(&app.project_file()).unwrap();
    let pf: ProjectFile = serde_json::from_str(&json).unwrap();
    assert_eq!(pf.profile.unwrap(), app.profile);

    app.rebuild_colors();
    let unmatched = app.models[0].as_ref().unwrap().elements.iter().position(|e| !e.props.iter().any(|p| p.name == "Level elevation"));
    if let Some(i) = unmatched {
        assert_eq!(app.looks[&0][i].col, Some(GHOST));
    }
}

#[test]
fn compare_finds_every_kind_of_change() {
    let p = |_: f32, _: &str| {};
    let old = load_any(ARCH, &p).unwrap();
    let mut new = load_any(ARCH, &p).unwrap();
    // 0: moved 250 mm along X; 1: renamed; 2: new GUID (= deleted + added); 3: deleted
    for t in new.elements[0].tris.iter_mut() {
        for v in t.iter_mut() {
            *v += DVec3::new(0.25, 0.0, 0.0);
        }
    }
    new.elements[1].name = "Renamed wall".into();
    let moved_guid = new.elements[2].guid.clone();
    new.elements[2].guid = "NEWGUID0000000000000000".into();
    let deleted_guid = new.elements.remove(3).guid;
    let n_new = new.elements.len();

    let o = old.center();
    let mut app = App::new(vec![]);
    app.models.push(Some(Arc::new(build_model(old, "old.ifc", o))));
    app.models.push(Some(Arc::new(build_model(new, "new.ifc", o))));
    app.compare_pick = (Some(0), Some(1));
    app.compare_tol = 1.0;
    app.run_compare();
    let r = app.compare.as_ref().unwrap();
    let c = |d: Diff| r.counts[Diff::ALL.iter().position(|x| *x == d).unwrap()];
    assert_eq!(c(Diff::Removed), 2, "changed-GUID element + deleted element");
    assert_eq!(c(Diff::Added), 1);
    assert_eq!(c(Diff::Geometry), 1);
    assert_eq!(c(Diff::Properties), 1);
    assert_eq!(c(Diff::Same), n_new - 3);

    let a = app.models[0].clone().unwrap();
    let removed: Vec<String> = r.ids(Diff::Removed).iter().map(|id| a.elements[id.e as usize].guid.clone()).collect();
    assert!(removed.contains(&moved_guid) && removed.contains(&deleted_guid));
    let geo = r.ids(Diff::Geometry)[0];
    assert!(r.changes[&geo][0].starts_with("Moved 250 mm"), "{:?}", r.changes[&geo]);
    let prop = r.ids(Diff::Properties)[0];
    assert!(r.changes[&prop].iter().any(|s| s.contains("Renamed wall")), "{:?}", r.changes[&prop]);

    // in 3D: the old matched element is hidden, the new one colored; the deleted ones stay red
    app.rebuild_colors();
    assert!(app.looks[&0][0].hidden);
    assert_eq!(app.looks[&1][0].col, Some(Diff::Geometry.color()));
    assert_eq!(app.looks[&0][3].col, Some(Diff::Removed.color()));

    // any model change invalidates the result
    app.refresh_caches();
    assert!(app.compare.is_none() && !app.compare_on);
}
