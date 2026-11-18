//! Actualizarea incrementala a culorilor trebuie sa dea exact acelasi buffer ca o reconstructie completa.
use bimrust::app::*;
use bimrust::model::ElemId;
use bimrust::render::Upload;
use bimrust::*;
use std::collections::HashMap;
use std::sync::Arc;

fn app() -> App {
    let p = |_: f32, _: &str| {};
    let a = load_any("samples/Block_A_Architecture_Structure.ifc", &p).unwrap();
    let b = load_any("samples/Block_A_MEP.ifc", &p).unwrap();
    let o = a.center();
    let mut app = App::new(vec![]);
    app.models.push(Some(Arc::new(build_model(a, "samples/Block_A_Architecture_Structure.ifc", o))));
    app.models.push(Some(Arc::new(build_model(b, "samples/Block_A_MEP.ifc", o))));
    app
}

/// "GPU" simulat: aplica Colors / ColorRange pe bufferele fiecarui slot; intoarce cati octeti s-au trimis
fn apply(app: &App, gpu: &mut HashMap<u32, (Vec<u8>, Vec<u8>)>) -> usize {
    let mut sent = 0;
    for u in std::mem::take(&mut app.renderer.lock().unwrap().pending) {
        match u {
            Upload::Colors { slot, col, mat } => {
                sent += col.len() + mat.len();
                gpu.insert(slot, (col, mat));
            }
            Upload::ColorRange { slot, first, col, mat } => {
                sent += col.len() + mat.len();
                let g = gpu.get_mut(&slot).expect("range before full upload");
                let o = first as usize * 4;
                g.0[o..o + col.len()].copy_from_slice(&col);
                g.1[o..o + mat.len()].copy_from_slice(&mat);
            }
            _ => {}
        }
    }
    sent
}

/// reconstructie completa de referinta pentru starea curenta
fn reference(app: &mut App) -> HashMap<u32, (Vec<u8>, Vec<u8>)> {
    let saved = std::mem::take(&mut app.looks);
    app.colors_dirty = true;
    app.rebuild_colors();
    let mut r = HashMap::new();
    apply(app, &mut r);
    app.looks = saved;
    r
}

fn step(app: &mut App, gpu: &mut HashMap<u32, (Vec<u8>, Vec<u8>)>, what: &str) -> usize {
    app.colors_dirty = true;
    app.rebuild_colors();
    let sent = apply(app, gpu);
    let want = reference(app);
    for (slot, (c, m)) in &want {
        let (gc, gm) = &gpu[slot];
        assert!(gc == c && gm == m, "{what}: slot {slot} differs from a full rebuild");
    }
    sent
}

#[test]
fn incremental_colors_match_full_rebuild() {
    let mut app = app();
    let mut gpu = HashMap::new();
    let full = step(&mut app, &mut gpu, "initial");
    assert!(full > 0);

    // un click de selectie: se trimite doar elementul selectat
    app.selected.insert(ElemId { m: 0, e: 5 });
    let s = step(&mut app, &mut gpu, "select one");
    assert!(s > 0 && s * 20 < full, "select should send a small range, sent {s} of {full}");

    // nimic schimbat -> nimic trimis
    assert_eq!(step(&mut app, &mut gpu, "no change"), 0);

    // selectie in alt model + deselectare + ascundere + culoare + elemente vecine (interval comun)
    app.selected.clear();
    app.selected.insert(ElemId { m: 1, e: 2 });
    app.hidden.insert(ElemId { m: 0, e: 7 });
    app.hidden.insert(ElemId { m: 0, e: 8 });
    app.overrides.insert(ElemId { m: 1, e: 0 }, [255, 0, 0, 255]);
    step(&mut app, &mut gpu, "mixed changes");

    // selectie mare (peste 1/3 din triunghiuri) -> reconstructie completa, tot corecta
    app.selected = app.all_ids().into_iter().filter(|i| i.m == 0).collect();
    step(&mut app, &mut gpu, "select everything");

    // inapoi la normal
    app.selected.clear();
    app.hidden.clear();
    app.overrides.clear();
    step(&mut app, &mut gpu, "reset");
}
