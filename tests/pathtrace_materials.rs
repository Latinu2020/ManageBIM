//! End-to-end check: the materials assigned in the project reach the path trace scene
//! (not just the raw colours from the file), and the section / hidden elements are respected.
use bimrust::app::App;
use bimrust::*;
use std::sync::Arc;

fn app() -> App {
    let p = |_: f32, _: &str| {};
    let raw = load_any("samples/Block_A_Architecture_Structure.ifc", &p).unwrap();
    let o = raw.center();
    let mut app = App::new(vec![]);
    app.models.push(Some(Arc::new(build_model(raw, "samples/Block_A_Architecture_Structure.ifc", o))));
    app
}

#[test]
fn assigned_materials_reach_the_pathtrace_scene() {
    let mut app = app();
    // the raw scene (without materials)
    app.rebuild_colors();
    let plain = bimrust::pathtrace::Scene::build(&app);
    assert!(!plain.colors.is_empty(), "modelul de test nu are geometrie");

    // the raw colours of the file, per element
    let first_elem = &app.models[0].as_ref().unwrap().elements[0];
    let t0 = first_elem.tri_start as usize;
    let raw_col = app.models[0].as_ref().unwrap().tri_color[t0];

    // we assign materials by IFC type (Exactly as in the UI: Render with materials)
    // cache_types is filled on the normal loading of the model
    app.refresh_caches();
    app.auto_materials();
    app.materials_on = true;
    app.materials_changed();
    app.rebuild_colors();

    let with = bimrust::pathtrace::Scene::build(&app);
    assert_eq!(with.colors.len(), plain.colors.len());

    // at least some triangles have the material colour / parameters, different from the file
    let changed_col = plain.colors.iter().zip(&with.colors).filter(|(a, b)| a != b).count();
    let changed_mat = with.mats.iter().filter(|m| **m != [0, 0, 0, 0]).count();
    assert!(changed_col > 0 || changed_mat > 0, "materialele nu au ajuns in scena (raw_col {raw_col:?})");
    eprintln!("materiale in scena: {changed_col} triunghiuri recolorate, {changed_mat} cu parametri PBR");

    // a hidden element must not appear in the scene
    let n_before = with.colors.len();
    app.hidden.insert(bimrust::model::ElemId { m: 0, e: 0 });
    app.colors_dirty = true;
    app.rebuild_colors();
    let after = bimrust::pathtrace::Scene::build(&app);
    assert!(after.colors.len() < n_before, "elementul ascuns a ramas in randare");
}
