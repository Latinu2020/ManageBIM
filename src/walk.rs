//! Mers prin model: miscare lina (accelerare / franare) si mers automat pe un traseu desenat.
//!
//! Traseul = puncte pe podea (click in model). Se netezeste (Catmull-Rom), apoi camera merge pe el la
//! inaltimea ochilor, cu viteza de mers, privind inainte (rotire lina in curbe). Se poate salva ca
//! animatie (Animator) - deci si ca video.
use crate::anim::{Animation, Keyframe};
use crate::camera::Camera;
use glam::Vec3;
use std::f32::consts::PI;

/// Constanta de timp pentru accelerare / franare (secunde): ~95% din viteza tinta in 3 * TAU.
pub const TAU: f32 = 0.16;

/// Apropie `cur` de `target` exponential, independent de FPS.
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

/// Raspunsul joystick-ului (mers cu mouse-ul): zona moarta + curba patratica, plafonat.
/// `px` = deplasarea mouse-ului fata de punctul de start, `full` = pixeli pentru viteza 1.
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

/// Traseu netezit, parcurs la viteza constanta.
#[derive(Clone, Debug)]
pub struct PathRun {
    pts: Vec<Vec3>,
    cum: Vec<f32>,
    pub dist: f32,
    pub paused: bool,
    yaw: Option<f32>,
}

impl PathRun {
    /// `None` daca traseul are mai putin de 2 puncte distincte.
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

    /// Punctul (pe podea) la distanta `d` de la start.
    pub fn pos_at(&self, d: f32) -> Vec3 {
        let d = d.clamp(0.0, self.length());
        let i = self.cum.partition_point(|&c| c <= d).clamp(1, self.pts.len() - 1);
        let (c0, c1) = (self.cum[i - 1], self.cum[i]);
        let u = if c1 > c0 { (d - c0) / (c1 - c0) } else { 0.0 };
        self.pts[i - 1].lerp(self.pts[i], u)
    }

    /// Directia de privire (yaw) la distanta `d`: spre un punct putin mai in fata.
    pub fn yaw_at(&self, d: f32) -> f32 {
        let a = self.pos_at(d);
        let mut b = self.pos_at(d + 1.5);
        if (b - a).truncate().length() < 1e-3 {
            b = a + (a - self.pos_at(d - 1.5));
        }
        let v = b - a;
        v.y.atan2(v.x)
    }

    /// Avanseaza camera pe traseu. Intoarce true la capat.
    pub fn step(&mut self, cam: &mut Camera, speed: f32, eye_h: f32, dt: f32) -> bool {
        if !self.paused {
            self.dist = (self.dist + speed.max(0.05) * dt).min(self.length());
        }
        let p = self.pos_at(self.dist);
        cam.eye = (p + Vec3::Z * eye_h).into();
        let target = self.yaw_at(self.dist);
        // rotire lina in curbe (max ~90 grade / s), fara salt la pornire
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

    /// Animatie (Animator) echivalenta: cadre-cheie la ~1 m, durata = lungime / viteza.
    pub fn to_animation(&self, name: String, speed: f32, eye_h: f32, fov_deg: f32) -> Animation {
        let speed = speed.max(0.05);
        let len = self.length();
        let n = ((len / 1.0).ceil() as usize).clamp(2, 2000);
        let mut keys = Vec::with_capacity(n + 1);
        let mut yaw_prev: Option<f32> = None;
        for i in 0..=n {
            let d = len * i as f32 / n as f32;
            let p = self.pos_at(d);
            // yaw continuu (fara salturi de 2*PI intre cadre)
            let mut y = self.yaw_at(d);
            if let Some(py) = yaw_prev {
                y = py + wrap(y - py);
            }
            yaw_prev = Some(y);
            keys.push(Keyframe { t: d / speed, cam: Camera { eye: (p + Vec3::Z * eye_h).into(), yaw: y, pitch: -0.05, focus: 3.0, fov_deg, ortho: false } });
        }
        Animation { name, keys, sim_4d: false, ease: false }
    }
}

/// Catmull-Rom prin punctele de control, esantionat la ~`step` metri.
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
            let q = 0.5 * ((2.0 * p1) + (-p0 + p2) * u + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * u2 + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * u3);
            out.push(q);
        }
    }
    out
}
