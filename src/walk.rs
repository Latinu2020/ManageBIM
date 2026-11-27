//! Walking through the model: smooth motion (acceleration / braking) and automatic walking along a drawn path.
//!
//! The path = points on the floor (click in the model). It is smoothed (Catmull-Rom), then the camera walks along it at
//! eye height, at walking speed, looking ahead (smooth turning in curves). It can be saved as
//! an animation (Animator) - and therefore as a video too.
use crate::anim::{Animation, Keyframe};
use crate::camera::Camera;
use glam::Vec3;
use std::f32::consts::PI;

/// Time constant for acceleration / braking (seconds): ~95% of the target speed in 3 * TAU.
pub const TAU: f32 = 0.16;

/// Brings `cur` towards `target` exponentially, independent of FPS.
pub fn smooth(cur: Vec3, target: Vec3, dt: f32) -> Vec3 {
    let k = 1.0 - (-dt.max(0.0) / TAU).exp();
    let v = cur + (target - cur) * k;
    if v.length_squared() < 1e-6 && target == Vec3::ZERO {
        Vec3::ZERO
    } else {
        v
    }
}

pub fn smooth1(cur: f32, target: f32, dt: f32) -> f32 {
    smooth(Vec3::new(cur, 0.0, 0.0), Vec3::new(target, 0.0, 0.0), dt).x
}

/// The joystick response (walking with the mouse): dead zone + quadratic curve, capped.
/// `px` = the mouse offset from the start point, `full` = pixels for speed 1.
pub fn joystick(px: f32, dead: f32, full: f32, max: f32) -> f32 {
    let a = px.abs();
    if a <= dead {
        return 0.0;
    }
    let u = ((a - dead) / full).min(max.sqrt());
    (u * u).min(max) * px.signum()
}

fn wrap(mut a: f32) -> f32 {
    while a > PI {
        a -= 2.0 * PI;
    }
    while a < -PI {
        a += 2.0 * PI;
    }
    a
}

/// Smoothed path, travelled at constant speed.
#[derive(Clone, Debug)]
pub struct PathRun {
    pts: Vec<Vec3>,
    cum: Vec<f32>,
    pub dist: f32,
    pub paused: bool,
    yaw: Option<f32>,
}

impl PathRun {
    /// `None` if the path has fewer than 2 distinct points.
    pub fn new(ctrl: &[Vec3]) -> Option<PathRun> {
        let pts = densify(ctrl, 0.25);
        if pts.len() < 2 {
            return None;
        }
        let mut cum = Vec::with_capacity(pts.len());
        let mut s = 0.0;
        cum.push(0.0);
        for w in pts.windows(2) {
            s += (w[1] - w[0]).length();
            cum.push(s);
        }
        if s < 1e-3 {
            return None;
        }
        Some(PathRun { pts, cum, dist: 0.0, paused: false, yaw: None })
    }

    pub fn length(&self) -> f32 {
        *self.cum.last().unwrap_or(&0.0)
    }

    /// The point (on the floor) at distance `d` from the start.
    pub fn pos_at(&self, d: f32) -> Vec3 {
        let d = d.clamp(0.0, self.length());
        let i = self.cum.partition_point(|&c| c <= d).clamp(1, self.pts.len() - 1);
        let (c0, c1) = (self.cum[i - 1], self.cum[i]);
        let u = if c1 > c0 { (d - c0) / (c1 - c0) } else { 0.0 };
        self.pts[i - 1].lerp(self.pts[i], u)
    }

    /// The view direction (yaw) at distance `d`: towards a point slightly ahead.
    pub fn yaw_at(&self, d: f32) -> f32 {
        let a = self.pos_at(d);
        let mut b = self.pos_at(d + 1.5);
        if (b - a).truncate().length() < 1e-3 {
            b = a + (a - self.pos_at(d - 1.5));
        }
        let v = b - a;
        v.y.atan2(v.x)
    }

    /// Advances the camera along the path. Returns true at the end.
    pub fn step(&mut self, cam: &mut Camera, speed: f32, eye_h: f32, dt: f32) -> bool {
        if !self.paused {
            self.dist = (self.dist + speed.max(0.05) * dt).min(self.length());
        }
        let p = self.pos_at(self.dist);
        cam.eye = (p + Vec3::Z * eye_h).into();
        let target = self.yaw_at(self.dist);
        // smooth turning in curves (max ~90 degrees / s), no jump at the start
        let cur = *self.yaw.get_or_insert(target);
        let d = wrap(target - cur);
        let max = 1.6 * dt;
        let ny = if self.dist <= 0.0 { target } else { cur + d.clamp(-max, max) };
        self.yaw = Some(ny);
        cam.yaw = ny;
        cam.pitch += (-0.05 - cam.pitch) * (1.0 - (-dt / 0.4).exp());
        cam.focus = 3.0;
        cam.ortho = false;
        self.dist >= self.length()
    }

    /// The equivalent animation (Animator): keyframes every ~1 m, duration = length / speed.
    pub fn to_animation(&self, name: String, speed: f32, eye_h: f32, fov_deg: f32) -> Animation {
        let speed = speed.max(0.05);
        let len = self.length();
        let n = ((len / 1.0).ceil() as usize).clamp(2, 2000);
        let mut keys = Vec::with_capacity(n + 1);
        let mut yaw_prev: Option<f32> = None;
        for i in 0..=n {
            let d = len * i as f32 / n as f32;
            let p = self.pos_at(d);
            // continuous yaw (no 2*PI jumps between frames)
            let mut y = self.yaw_at(d);
            if let Some(py) = yaw_prev {
                y = py + wrap(y - py);
            }
            yaw_prev = Some(y);
            keys.push(Keyframe { t: d / speed, cam: Camera { eye: (p + Vec3::Z * eye_h).into(), yaw: y, pitch: -0.05, focus: 3.0, fov_deg, ortho: false } });
        }
        Animation { name, keys, sim_4d: false, ease: false, sets: Vec::new(), kind: crate::anim::AnimKind::WalkPath }
    }
}

/// Catmull-Rom through the control points, sampled every ~`step` metres.
pub fn densify(ctrl: &[Vec3], step: f32) -> Vec<Vec3> {
    let mut c: Vec<Vec3> = Vec::with_capacity(ctrl.len());
    for p in ctrl {
        if c.last().map(|l: &Vec3| (*l - *p).length() > 1e-3).unwrap_or(true) {
            c.push(*p);
        }
    }
    if c.len() < 3 {
        return c;
    }
    let mut out = vec![c[0]];
    for i in 0..c.len() - 1 {
        let p0 = c[i.saturating_sub(1)];
        let (p1, p2) = (c[i], c[i + 1]);
        let p3 = c[(i + 2).min(c.len() - 1)];
        let n = (((p2 - p1).length() / step.max(0.01)).ceil() as usize).clamp(1, 400);
        for k in 1..=n {
            let u = k as f32 / n as f32;
            let (u2, u3) = (u * u, u * u * u);
            let mut q = 0.5 * ((2.0 * p1) + (-p0 + p2) * u + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * u2 + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * u3);
            // the height: linear between points (curve only in plan) - otherwise the path "jumps" metres above the floor
            // when the points are at different heights (stairs, ramps, another level)
            q.z = p1.z + (p2.z - p1.z) * u;
            out.push(q);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn path_stays_on_the_floor() {
        use glam::Vec3;
        let pts = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 3.0), Vec3::new(10.0, 2.0, 3.0), Vec3::new(12.0, 8.0, 0.0)];
        let d = super::densify(&pts, 0.25);
        assert!(d.iter().all(|p| p.z >= -1e-4 && p.z <= 3.0 + 1e-4));
    }
}
