use bimrust::layout::{Dock, Panel, UiLayout};
use bimrust::materials::{library, load_texture, tex_size};

#[test]
fn textures_png_and_jpg_load_and_resample() {
    for f in ["tests/fixtures/tex/tiles_test.png", "tests/fixtures/tex/tiles_test.jpg"] {
        let px = load_texture(f).expect(f);
        assert_eq!(px.len(), (tex_size() * tex_size() * 4) as usize);
        assert!(px.chunks_exact(4).all(|c| c[3] == 255));
    }
    assert!(load_texture("Cargo.toml").is_err());
    assert!(load_texture("missing.png").is_err());
}

#[test]
fn textured_material_gpu_code() {
    let mut m = library()[0].clone();
    m.texture = "x.png".into();
    assert_eq!(m.gpu(Some(3))[0], 19);
    assert_eq!(m.gpu(None)[0], m.pattern.code());
    // serde: older projects without "texture" still load
    let j = serde_json::to_string(&library()[0]).unwrap().replace(",\"texture\":\"\"", "");
    let back: bimrust::materials::Material = serde_json::from_str(&j).unwrap();
    assert!(back.texture.is_empty());
}

#[test]
fn panel_layout_dock_float_close() {
    let mut l = UiLayout::default();
    assert!(l.shown(Panel::Tree) && l.shown(Panel::Find) && !l.shown(Panel::Clash));
    l.open(Panel::Clash);
    assert!(l.shown(Panel::Clash) && !l.shown(Panel::Find));
    l.move_to(Panel::Clash, Dock::Float);
    assert!(l.shown(Panel::Clash));
    assert_eq!(l.active[2], Some(Panel::Find));
    l.move_to(Panel::Clash, Dock::Right);
    assert!(l.shown(Panel::Clash) && !l.shown(Panel::Properties));
    l.close(Panel::Clash);
    assert!(!l.shown(Panel::Clash) && l.shown(Panel::Properties));
    for p in l.in_area(Dock::Left) {
        l.close(p);
    }
    assert!(l.in_area(Dock::Left).is_empty() && l.active[0].is_none());
    let j = serde_json::to_string(&l).unwrap();
    let mut back: UiLayout = serde_json::from_str(&j).unwrap();
    back.fixup();
    assert_eq!(back, l);
}

#[test]
fn camera_ray_matches_projection_persp_and_ortho() {
    use bimrust::camera::Camera;
    use bimrust::model::Aabb;
    let bb = Aabb { min: glam::Vec3::new(-10.0, -10.0, 0.0), max: glam::Vec3::new(10.0, 10.0, 8.0) };
    for ortho in [false, true] {
        let mut c = Camera::default();
        c.ortho = ortho;
        c.fit(&bb);
        let aspect = 1.6;
        let vp = c.proj(aspect, &bb) * c.view();
        for uv in [glam::Vec2::new(0.5, 0.5), glam::Vec2::new(0.2, 0.7), glam::Vec2::new(0.9, 0.1)] {
            let (o, d) = c.ray(uv, aspect);
            for t in [1.0f32, 30.0, 200.0, 600.0] {
                let p = vp.project_point3(o + d * t);
                let want = glam::Vec2::new(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
                if p.z.abs() <= 1.0 {
                    assert!((p.truncate() - want).length() < 1e-3, "ortho={ortho} uv={uv} t={t} got {p}");
                }
            }
        }
        // ViewCube: looking from the front face
        c.set_dir(-glam::Vec3::Y);
        assert!((c.fwd() - glam::Vec3::Y).length() < 1e-4);
        c.set_dir(glam::Vec3::Z);
        assert!(c.fwd().z < -0.99);
    }
}
