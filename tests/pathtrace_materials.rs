//! Verificare cap-la-cap: materialele atribuite în proiect ajung în scena path trace-ului
//! (nu doar culorile brute din fișier), iar secțiunea / elementele ascunse sunt respectate.
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
    // scena brute (fara materiale)
    app.rebuild_colors();
    let plain = bimrust::pathtrace::Scene::build(&app);
    assert!(!plain.colors.is_empty(), "modelul de test nu are geometrie");

    // culorile brute ale fisierului, pe elemente
    let first_elem = &app.models[0].as_ref().unwrap().elements[0];
    let t0 = first_elem.tri_start as usize;
    let raw_col = app.models[0].as_ref().unwrap().tri_color[t0];

    // atribuim materiale dupa tipul IFC (Exact ca in UI: Render with materials)
    // cache_types se umple la incarcarea normala a modelului
    app.refresh_caches();
    app.auto_materials();
    app.materials_on = true;
    app.materials_changed();
    app.rebuild_colors();

    let with = bimrust::pathtrace::Scene::build(&app);
    assert_eq!(with.colors.len(), plain.colors.len());

    // cel putin unele triunghiuri au culoarea / parametrii materialului, diferiti de fisier
    let changed_col = plain.colors.iter().zip(&with.colors).filter(|(a, b)| a != b).count();
    let changed_mat = with.mats.iter().filter(|m| **m != [0, 0, 0, 0]).count();
    assert!(changed_col > 0 || changed_mat > 0, "materialele nu au ajuns in scena (raw_col {raw_col:?})");
    eprintln!("materiale in scena: {changed_col} triunghiuri recolorate, {changed_mat} cu parametri PBR");

    // un element ascuns nu trebuie sa apara in scena
    let n_before = with.colors.len();
    app.hidden.insert(bimrust::model::ElemId { m: 0, e: 0 });
    app.colors_dirty = true;
    app.rebuild_colors();
    let after = bimrust::pathtrace::Scene::build(&app);
    assert!(after.colors.len() < n_before, "elementul ascuns a ramas in randare");
}
