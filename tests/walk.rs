//! Mers lin si mers pe traseu (src/walk.rs).
use bimrust::app::walk::{joystick, smooth, PathRun};
use bimrust::camera::Camera;
use glam::Vec3;

#[test]
fn smoothing_accelerates_and_stops_without_jumps() {
    let target = Vec3::new(0.0, 1.4, 0.0);
    let mut v = Vec3::ZERO;
    // primul cadru (chiar lung) nu sare direct la viteza maxima
    v = smooth(v, target, 0.05);
    assert!(v.y > 0.0 && v.y < 0.6 * target.y, "{v:?}");
    for _ in 0..60 {
        v = smooth(v, target, 1.0 / 60.0);
    }
    assert!((v.y - 1.4).abs() < 0.01, "reaches walking speed in ~1 s: {v:?}");
    // independent de FPS: 30 FPS ajunge in acelasi loc ca 120 FPS
    let (mut a, mut b) = (Vec3::ZERO, Vec3::ZERO);
    for _ in 0..30 {
        a = smooth(a, target, 1.0 / 30.0);
    }
    for _ in 0..120 {
        b = smooth(b, target, 1.0 / 120.0);
    }
    assert!((a - b).length() < 1e-3);
    // franare pana la oprire completa
    for _ in 0..120 {
        v = smooth(v, Vec3::ZERO, 1.0 / 60.0);
    }
    assert_eq!(v, Vec3::ZERO);
}

#[test]
fn joystick_has_dead_zone_and_cap() {
    assert_eq!(joystick(5.0, 10.0, 120.0, 2.0), 0.0);
    let small = joystick(40.0, 10.0, 120.0, 2.0);
    assert!(small > 0.0 && small < 0.1, "small drag = slow: {small}");
    assert!((joystick(10_000.0, 10.0, 120.0, 2.0) - 2.0).abs() < 1e-6, "capped");
    assert!(joystick(-60.0, 10.0, 120.0, 2.0) < 0.0);
}

#[test]
fn path_walks_at_speed_and_looks_ahead() {
    // L: 10 m spre +X, apoi 10 m spre +Y
    let pts = [Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 0.0)];
    let mut run = PathRun::new(&pts).unwrap();
    let len = run.length();
    assert!(len > 19.0 && len < 20.5, "smoothed length ~20 m: {len}");
    let mut cam = Camera::default();
    run.step(&mut cam, 1.4, 1.7, 0.0);
    assert!((cam.eye[2] - 1.7).abs() < 1e-4, "eye height above the floor");
    assert!(cam.yaw.abs() < 0.2, "looks along +X at start: {}", cam.yaw);
    let mut t = 0.0;
    while !run.step(&mut cam, 1.4, 1.7, 1.0 / 30.0) {
        t += 1.0 / 30.0;
        assert!(t < 30.0, "never finished");
    }
    assert!((t - len / 1.4).abs() < 0.2, "duration = length / speed: {t}");
    assert!((cam.yaw - std::f32::consts::FRAC_PI_2).abs() < 0.2, "looks along +Y at the end: {}", cam.yaw);
    let e = Vec3::from(cam.eye);
    assert!((e - Vec3::new(10.0, 10.0, 1.7)).length() < 1e-3);
}

#[test]
fn path_to_animation() {
    let pts = [Vec3::ZERO, Vec3::new(14.0, 0.0, 0.0)];
    let run = PathRun::new(&pts).unwrap();
    let a = run.to_animation("p".into(), 1.4, 1.7, 50.0);
    assert!((a.duration() - 10.0).abs() < 1e-3);
    let mid = a.sample(5.0).unwrap();
    assert!((mid.eye[0] - 7.0).abs() < 0.05 && (mid.eye[2] - 1.7).abs() < 1e-4);
    assert!(PathRun::new(&[Vec3::ONE]).is_none());
    assert!(PathRun::new(&[Vec3::ONE, Vec3::ONE]).is_none());
}
