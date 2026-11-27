//! Builds examples/Block_A_project.bimr (a Navisworks-NWF-like project) from the demo files.
use bimrust::app::*;
use std::sync::Arc;
fn main() {
    let dir = std::env::args().nth(1).expect("examples dir");
    let d = std::path::Path::new(&dir);
    let files = ["Block_A_Architecture_Structure.ifc", "Block_A_MEP.ifc"];
    let mut app = App::new(vec![]);
    let mut origin = None;
    for (i, f) in files.iter().enumerate() {
        let p = d.join(f).to_string_lossy().to_string();
        let raw = bimrust::load_any(&p, &|_, _| {}).unwrap();
        let o = *origin.get_or_insert(raw.center());
        let m = Arc::new(bimrust::build_model(raw, &p, o));
        app.push_slot();
        app.base_models[i] = Some(m.clone());
        app.models[i] = Some(m);
    }
    *app.origin.lock().unwrap() = origin;
    app.refresh_caches();
    app.project_path = Some(d.join("Block_A_project.bimr").to_string_lossy().to_string());
    // sets
    app.sets.push(SelSet {
        name: "Structure (columns + beams + slabs)".into(),
        kind: SetKind::Search(vec![
            SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "IfcColumn", false),
            SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "IfcBeam", true),
            SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "IfcSlab", true),
        ]),
    });
    app.sets.push(SelSet { name: "HVAC".into(), kind: SetKind::Search(vec![SearchRule::new("Pset_MEP", "Phase", SearchOp::Equals, "HVAC", false)]) });
    // clash test: structure vs HVAC, grouped by level
    let mut t = ClashTest::new("Structure vs HVAC".into());
    t.a = Selector { kind: SelKind::Set, value: "Structure (columns + beams + slabs)".into() };
    t.b = Selector { kind: SelKind::Set, value: "HVAC".into() };
    app.clash.tests = vec![t];
    app.run_clash_blocking(0);
    app.auto_group(0, GroupBy::Level, 5.0);
    // schedule + camera + viewpoint + animation
    app.import_tasks_csv(&d.join("Block_A_schedule_4D.csv").to_string_lossy());
    let bb = app.scene_aabb();
    app.cam.yaw = std::f32::consts::FRAC_PI_4;
    app.cam.pitch = -0.5;
    app.cam.fit(&bb);
    app.save_viewpoint();
    app.viewpoints[0].name = "Overview".into();
    let mut a = bimrust::anim::Animation::orbit("Construction sequence 360°".into(), &app.cam, &bb, 20.0);
    a.sim_4d = true;
    app.animations.push(a);
    app.auto_materials();
    app.assign_material(bimrust::materials::MatTarget::Type("IfcWall".into()), "Brick red");
    let path = app.project_path.clone().unwrap();
    app.save_project(&path);
    println!("{} - {} clashes, {} tasks, log: {:?}", path, app.clash.tests[0].results.len(), app.tasks.len(), app.log);
}
