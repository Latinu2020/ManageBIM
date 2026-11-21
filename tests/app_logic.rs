use bimrust::app::*;
use bimrust::clash::ClashStatus;
use bimrust::*;
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

fn rule(f: &str, op: SearchOp, v: &str, or: bool) -> SearchRule {
    SearchRule { field: f.into(), op, value: v.into(), or, group: String::new() }
}

#[test]
fn find_and_or_numeric() {
    let app = app();
    let walls_l1 = app.find(&[rule("Level", SearchOp::Equals, "Level 1", false), rule("IFC Type", SearchOp::Equals, "IfcWall", false)]);
    assert_eq!(walls_l1.len(), 7);
    let roof_or_door = app.find(&[rule("IFC Type", SearchOp::Equals, "IfcRoof", false), rule("IFC Type", SearchOp::Equals, "IfcDoor", true)]);
    assert_eq!(roof_or_door.len(), 2);
    let upper = app.find(&[rule("Level elevation", SearchOp::Greater, "3", false)]);
    let all_upper = app.find(&[rule("Level", SearchOp::NotEquals, "Ground Floor", false), rule("Level elevation", SearchOp::Defined, "", false)]);
    assert!(!upper.is_empty());
    assert_eq!(upper.len(), all_upper.len());
    let no_sys = app.find(&[rule("System", SearchOp::Undefined, "", false)]);
    assert_eq!(no_sys.len(), 120 + 33 - 33 + 0 + (33 - 33)); // only MEP items have a System
    println!("walls L1 {}, roof|door {}, elevation>3 {}", walls_l1.len(), roof_or_door.len(), upper.len());
}

#[test]
fn clash_rules_and_groups() {
    let mut app = app();
    app.clash_tests[0].a = Selector { kind: SelKind::Model, value: "Block_A_Architecture_Structure.ifc".into() };
    app.clash_tests[0].b = Selector { kind: SelKind::Model, value: "Block_A_MEP.ifc".into() };
    app.run_clash_blocking(0);
    let n0 = app.clash_tests[0].results.len();
    assert!(n0 > 0);
    // exclude beams: those results disappear (not "Resolved")
    app.clash_tests[0].rules.exclude_rules.push(rule("IFC Type", SearchOp::Equals, "IfcBeam", false));
    app.run_clash_blocking(0);
    let t = &app.clash_tests[0];
    let beams = t.results.iter().filter(|r| app.elem(r.a).map(|x| x.1.ifc_type == "IfcBeam").unwrap_or(false)).count();
    assert_eq!(beams, 0);
    assert_eq!(t.count(ClashStatus::Resolved), 0);
    let n1 = t.results.len();
    assert!(n1 < n0);
    // ignore same level
    app.clash_tests[0].rules.exclude_rules.clear();
    app.clash_tests[0].rules.same_level = true;
    app.run_clash_blocking(0);
    let n2 = app.clash_tests[0].results.iter().filter(|r| r.status != ClashStatus::Resolved).count();
    assert!(n2 < n0);
    // groups
    app.clash_tests[0].rules.same_level = false;
    app.run_clash_blocking(0);
    app.auto_group(0, GroupBy::Level, 5.0);
    let groups = app.group_names(0);
    assert!(groups.len() >= 2, "{groups:?}");
    let csv = app.clash_report_csv(0);
    assert!(csv.lines().nth(1).unwrap().contains("Level:"));
    println!("all {n0}, w/o beams {n1}, w/o same level {n2}, groups {groups:?}");
}

#[test]
fn clash_status_follows_navisworks_lifecycle() {
    let mut app = app();
    let mep = "Block_A_MEP.ifc";
    let arch = "Block_A_Architecture_Structure.ifc";
    app.clash_tests[0].a = Selector { kind: SelKind::Model, value: arch.into() };
    app.clash_tests[0].b = Selector { kind: SelKind::Model, value: mep.into() };

    // prima rulare: totul e New
    app.run_clash_blocking(0);
    let n = app.clash_tests[0].results.len();
    assert!(n > 0);
    assert_eq!(app.clash_tests[0].count(ClashStatus::New), n);

    // a doua rulare, configuratie identica: aceleasi clash-uri devin Active
    app.run_clash_blocking(0);
    assert_eq!(app.clash_tests[0].results.len(), n);
    assert_eq!(app.clash_tests[0].count(ClashStatus::Active), n);
    assert_eq!(app.clash_tests[0].count(ClashStatus::New), 0);

    // Approved isi pastreaza statusul la re-rulare (ca in Navisworks)
    app.clash_tests[0].results[0].status = ClashStatus::Approved;
    app.run_clash_blocking(0);
    assert_eq!(app.clash_tests[0].results[0].status, ClashStatus::Approved);

    // daca nimic nu mai e gasit, vechiul clash devine Resolved
    app.clash_tests[0].b = Selector { kind: SelKind::Model, value: "no-such-model.ifc".into() };
    app.run_clash_blocking(0);
    assert_eq!(app.clash_tests[0].count(ClashStatus::Resolved), n);

    // daca geometria reapare, Resolved revine la New
    app.clash_tests[0].b = Selector { kind: SelKind::Model, value: mep.into() };
    app.run_clash_blocking(0);
    assert!(app.clash_tests[0].count(ClashStatus::New) > 0);
}

#[test]
fn prop_index_and_multi_type_sets() {
    let mut app = app();
    app.refresh_caches();
    let idx = &app.prop_index;
    assert_eq!(idx.cats[0].0, ITEM_CAT);
    let types = idx.prop(ITEM_CAT, "IFC Type").unwrap();
    println!("categories: {:?}", idx.cats.iter().map(|c| format!("{} ({})", c.0, c.1.len())).collect::<Vec<_>>());
    println!("IFC types: {:?}", types.values);
    // every property of every element is in the index (no limit)
    let total_props: std::collections::HashSet<(String, String)> = app.models.iter().flatten().flat_map(|m| m.elements.iter().flat_map(|e| e.props.iter().map(|p| (p.group.clone(), p.name.clone())))).collect();
    let indexed: usize = idx.cats.iter().filter(|c| c.0 != ITEM_CAT).map(|c| c.1.len()).sum();
    assert_eq!(indexed, total_props.len());
    // Navisworks article example: columns OR windows, from all models; plus ducts from the MEP model
    app.prop_checked.insert((ITEM_CAT.into(), "IFC Type".into(), "IfcColumn".into()));
    app.prop_checked.insert((ITEM_CAT.into(), "IFC Type".into(), "IfcWindow".into()));
    app.prop_checked.insert((ITEM_CAT.into(), "IFC Type".into(), "IfcDuctSegment".into()));
    let rules = app.rules_from_checked();
    let ids = app.find_scoped(&rules, false);
    let n_col = app.find(&[SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "IfcColumn", false)]).len();
    let n_win = app.find(&[SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "IfcWindow", false)]).len();
    let n_duct = app.find(&[SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "IfcDuctSegment", false)]).len();
    assert_eq!(ids.len(), n_col + n_win + n_duct);
    assert!(ids.iter().any(|i| i.m == 0) && ids.iter().any(|i| i.m == 1));
    // category-scoped property: Pset_MEP / System = Heating
    let heat = app.find(&[SearchRule::new("Pset_MEP", "System", SearchOp::Equals, "Heating", false)]);
    assert_eq!(heat.len(), 3);
    // model scope
    app.find_models_off.insert(1);
    assert_eq!(app.find_scoped(&rules, false).len(), n_col + n_win);
    println!("columns {n_col} + windows {n_win} + ducts {n_duct} = {}", ids.len());
}

#[test]
fn transforms_and_reset() {
    let mut app = app();
    app.base_models = app.models.clone();
    app.model_xf = vec![ModelXf::default(); 2];
    let b0 = app.models[0].as_ref().unwrap().aabb;
    app.set_model_xf(0, ModelXf { translate: [10.0, -5.0, 1.0], rot_z: 0.0, scale: 1.0, pivot_origin: false });
    let b1 = app.models[0].as_ref().unwrap().aabb;
    assert!((b1.min.x - b0.min.x - 10.0).abs() < 1e-3 && (b1.min.y - b0.min.y + 5.0).abs() < 1e-3 && (b1.min.z - b0.min.z - 1.0).abs() < 1e-3);
    // rotate 90° about the model centre: X and Y extents swap
    app.set_model_xf(0, ModelXf { translate: [0.0; 3], rot_z: 90.0, scale: 1.0, pivot_origin: false });
    let b2 = app.models[0].as_ref().unwrap().aabb;
    assert!((b2.size().x - b0.size().y).abs() < 1e-2 && (b2.size().y - b0.size().x).abs() < 1e-2, "{b0:?} {b2:?}");
    // move one item, then reset it
    app.set_model_xf(0, ModelXf::default());
    let id = bimrust::model::ElemId { m: 1, e: 0 };
    let e0 = app.elem(id).unwrap().1.aabb;
    app.selected.insert(id);
    app.move_selected(glam::Vec3::new(0.0, 0.0, 0.5));
    let e1 = app.elem(id).unwrap().1.aabb;
    assert!((e1.min.z - e0.min.z - 0.5).abs() < 1e-4);
    app.reset_items(&[id]);
    let e2 = app.elem(id).unwrap().1.aabb;
    assert!((e2.min.z - e0.min.z).abs() < 1e-5);
    // project file round trip keeps transforms
    app.set_model_xf(1, ModelXf { translate: [1.0, 2.0, 3.0], rot_z: 15.0, scale: 1.0, pivot_origin: true });
    app.selected.clear();
    app.selected.insert(bimrust::model::ElemId { m: 0, e: 3 });
    app.move_selected(glam::Vec3::new(2.0, 0.0, 0.0));
    let pf = app.project_file();
    let json = serde_json::to_string(&pf).unwrap();
    let back: ProjectFile = serde_json::from_str(&json).unwrap();
    assert_eq!(back.model_xf[1].rot_z, 15.0);
    assert_eq!(back.elem_offsets.len(), 1);
    app.reset_all_transforms();
    assert_eq!(app.models[0].as_ref().unwrap().aabb.min, b0.min);
}

#[test]
fn animation_orbit_and_avi() {
    use bimrust::anim::Animation;
    use bimrust::camera::Camera;
    let mut app = app();
    app.refresh_caches();
    let bb = app.scene_aabb();
    let cam = Camera { eye: [bb.center().x - 30.0, bb.center().y, 10.0], yaw: 0.0, pitch: -0.3, focus: 30.0, fov_deg: 50.0, ortho: false };
    let a = Animation::orbit("o".into(), &cam, &bb, 10.0);
    assert!((a.duration() - 10.0).abs() < 1e-4);
    let s0 = a.sample(0.0).unwrap();
    let s5 = a.sample(5.0).unwrap();
    assert!((s0.eye[0] - cam.eye[0]).abs() < 1e-3);
    assert!((s5.eye[0] - (2.0 * bb.center().x - cam.eye[0])).abs() < 0.5, "{:?}", s5.eye);
    // AVI writer: 12 frames 320x180
    let path = std::env::temp_dir().join("bimrust_test.avi");
    let mut sink = bimrust::video::open_sink(bimrust::video::VideoFormat::Avi, &path, 320, 180, 24, 90).unwrap();
    for i in 0..12u32 {
        let mut rgba = vec![0u8; 320 * 180 * 4];
        for (k, p) in rgba.chunks_exact_mut(4).enumerate() {
            let x = (k % 320) as u32;
            p.copy_from_slice(&[((x + i * 20) % 256) as u8, 80, 200, 255]);
        }
        sink.add(&bimrust::video::Frame { w: 320, h: 180, rgba }).unwrap();
    }
    sink.finish().unwrap();
    let d = std::fs::read(&path).unwrap();
    assert_eq!(&d[0..4], b"RIFF");
    assert_eq!(&d[8..12], b"AVI ");
    assert_eq!(u32::from_le_bytes([d[4], d[5], d[6], d[7]]) as usize, d.len() - 8);
    println!("avi {} bytes at {}", d.len(), path.display());
}

#[test]
fn materials_rules() {
    use bimrust::materials::MatTarget;
    let mut app = app();
    app.refresh_caches();
    app.auto_materials();
    let walls = app.find(&[SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "IfcWall", false)]);
    let plaster = app.materials.iter().position(|m| m.name == "Plaster white").unwrap() as u16;
    assert_eq!(app.material_map().get(&walls[0]), Some(&plaster));
    // a later rule on a set wins and stays dynamic
    app.sets.push(SelSet { name: "W".into(), kind: SetKind::Search(vec![SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "IfcWall", false)]) });
    app.assign_material(MatTarget::Set("W".into()), "Brick red");
    let brick = app.materials.iter().position(|m| m.name == "Brick red").unwrap() as u16;
    assert_eq!(app.material_map().get(&walls[3]), Some(&brick));
    let pf = serde_json::to_string(&app.project_file()).unwrap();
    assert!(pf.contains("Brick red"));
}

#[test]
fn walk_gravity_and_collision() {
    use bimrust::app::NavMode;
    let mut a = app();
    let bb = a.scene_aabb();
    // start above the middle of the building, in walk mode with gravity
    let c = bb.center();
    a.nav = NavMode::Walk;
    a.settings.gravity = true;
    a.settings.collision = true;
    a.settings.eye_height = 1.7;
    a.cam.eye = [c.x, c.y, bb.max.z + 5.0];
    a.cam.pitch = 0.0;
    for _ in 0..400 {
        a.gravity_tick(1.0 / 60.0);
    }
    let z = a.cam.eye[2];
    // must land on the roof (or top slab), 1.7 m above it
    let (_, t, _, _) = a.pick(a.cam.eye(), glam::Vec3::NEG_Z).expect("ground under the camera");
    assert!((t - 1.7).abs() < 0.08, "eye {z} is {t} m above the ground");
    // walk far towards +X: the parapet / edge must not be passed through when collision hits something
    let start = a.cam.eye();
    a.cam.yaw = 0.0;
    for _ in 0..600 {
        a.walk_move(glam::Vec3::new(0.0, 1.0, 0.0), 0.05, 1.0 / 60.0);
    }
    assert!(a.cam.eye().x > start.x, "walked forward");
}
