//! Camera animations (Navisworks Animator-style keyframes) and render settings.
use crate::camera::Camera;
use crate::model::Aabb;
use crate::video::VideoFormat;
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Keyframe {
    pub t: f32,
    pub cam: Camera,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Animation {
    pub name: String,
    pub keys: Vec<Keyframe>,
    /// play the TimeLiner 4D simulation from start to end during the animation
    #[serde(default)]
    pub sim_4d: bool,
    /// ease in / out at the start and the end
    #[serde(default = "yes")]
    pub ease: bool,
}

fn yes() -> bool {
    true
}

impl Animation {
    pub fn new(name: String) -> Animation {
        Animation { name, keys: Vec::new(), sim_4d: false, ease: true }
    }
    pub fn duration(&self) -> f32 {
        self.keys.last().map(|k| k.t).unwrap_or(0.0)
    }
    pub fn sort(&mut self) {
        self.keys.sort_by(|a, b| a.t.total_cmp(&b.t));
    }

    /// Camera at time `t` (Catmull-Rom through the keyframes).
    pub fn sample(&self, t: f32) -> Option<Camera> {
        let k = &self.keys;
        if k.is_empty() {
            return None;
        }
        if k.len() == 1 {
            return Some(k[0].cam);
        }
        let dur = self.duration().max(1e-4);
        let mut t = t.clamp(0.0, dur);
        if self.ease {
            // ease in/out on global time
            let u = t / dur;
            let e = u * u * (3.0 - 2.0 * u);
            t = (0.65 * u + 0.35 * e) * dur;
        }
        let i = k.iter().rposition(|x| x.t <= t).unwrap_or(0).min(k.len() - 2);
        let (t0, t1) = (k[i].t, k[i + 1].t);
        let u = if t1 > t0 { ((t - t0) / (t1 - t0)).clamp(0.0, 1.0) } else { 1.0 };
        let get = |j: isize| k[j.clamp(0, k.len() as isize - 1) as usize].cam;
        let (c0, c1, c2, c3) = (get(i as isize - 1), get(i as isize), get(i as isize + 1), get(i as isize + 2));
        // yaw continuu
        let mut y = [c0.yaw, c1.yaw, c2.yaw, c3.yaw];
        for j in 1..4 {
            let mut d = y[j] - y[j - 1];
            while d > PI {
                d -= 2.0 * PI;
            }
            while d < -PI {
                d += 2.0 * PI;
            }
            y[j] = y[j - 1] + d;
        }
        let cr = |a: f32, b: f32, c: f32, d: f32| 0.5 * ((2.0 * b) + (-a + c) * u + (2.0 * a - 5.0 * b + 4.0 * c - d) * u * u + (-a + 3.0 * b - 3.0 * c + d) * u * u * u);
        let crv = |a: Vec3, b: Vec3, c: Vec3, d: Vec3| Vec3::new(cr(a.x, b.x, c.x, d.x), cr(a.y, b.y, c.y, d.y), cr(a.z, b.z, c.z, d.z));
        let eye = crv(c0.eye(), c1.eye(), c2.eye(), c3.eye());
        Some(Camera {
            eye: eye.into(),
            yaw: cr(y[0], y[1], y[2], y[3]),
            pitch: cr(c0.pitch, c1.pitch, c2.pitch, c3.pitch).clamp(-1.55, 1.55),
            focus: (c1.focus + (c2.focus - c1.focus) * u).max(0.05),
            fov_deg: c1.fov_deg + (c2.fov_deg - c1.fov_deg) * u,
            ortho: c1.ortho,
        })
    }

    /// Turntable: full circle around the scene at the current height and distance.
    pub fn orbit(name: String, cur: &Camera, scene: &Aabb, seconds: f32) -> Animation {
        let c = scene.center();
        let e = cur.eye();
        let mut r = Vec3::new(e.x - c.x, e.y - c.y, 0.0).length();
        if r < 0.5 {
            r = scene.size().length() * 0.9;
        }
        let z = e.z;
        let start = (e.y - c.y).atan2(e.x - c.x);
        let n = 36;
        let mut keys = Vec::with_capacity(n + 1);
        for i in 0..=n {
            let a = start + 2.0 * PI * i as f32 / n as f32;
            let eye = Vec3::new(c.x + r * a.cos(), c.y + r * a.sin(), z);
            let d = c - eye;
            let yaw = d.y.atan2(d.x);
            let pitch = d.z.atan2(Vec3::new(d.x, d.y, 0.0).length());
            keys.push(Keyframe { t: seconds * i as f32 / n as f32, cam: Camera { eye: eye.into(), yaw, pitch, focus: d.length(), fov_deg: cur.fov_deg , ortho: cur.ortho } });
        }
        Animation { name, keys, sim_4d: false, ease: false }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Background {
    Theme,
    White,
    Dark,
}

impl Background {
    pub fn label(&self) -> &'static str {
        match self {
            Background::Theme => "Gradient (current theme)",
            Background::White => "White",
            Background::Dark => "Dark",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RenderSettings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub quality: u8,
    pub supersample: bool,
    pub background: Background,
    pub format: VideoFormat,
    /// sun direction (degrees): azimuth from +X counter-clockwise, elevation above horizon
    #[serde(default = "sun_az")]
    pub sun_az: f32,
    #[serde(default = "sun_el")]
    pub sun_el: f32,
    /// luminozitate generala, lumina ambientala, intensitatea soarelui (1 = implicit)
    #[serde(default = "one")]
    pub exposure: f32,
    #[serde(default = "one")]
    pub ambient: f32,
    #[serde(default = "one")]
    pub sun_power: f32,
    /// umbrire de contact (SSAO), 0 = oprita
    #[serde(default = "ao_default")]
    pub ao: f32,
    /// margini fine pe elemente (detectate din adancime), 0 = oprite
    #[serde(default = "edges_default")]
    pub edges: f32,
    /// chenarul de incadrare al randarii in vizualizare (implicit oprit)
    #[serde(default)]
    pub show_frame: bool,
    /// pe video / imagini 4D: data simularii si legenda (Construct / Demolish / Temporary)
    #[serde(default = "yes")]
    pub overlay_4d: bool,
    /// pe imagini: legenda Appearance Profiler (colorare dupa parametru)
    #[serde(default = "yes")]
    pub legend_on_images: bool,
    /// umbre aruncate de soare (shadow map)
    #[serde(default = "yes")]
    pub shadows: bool,
    /// soarele dupa data, ora si locatie (altfel azimut / elevatie manuale)
    #[serde(default)]
    pub sun_from_time: bool,
    #[serde(default = "lat_default")]
    pub lat: f32,
    #[serde(default = "lon_default")]
    pub lon: f32,
    /// fusul orar (ore fata de UTC, ex. 3 = Romania vara)
    #[serde(default = "tz_default")]
    pub tz: f32,
    #[serde(default = "date_default")]
    pub sun_date: String,
    #[serde(default = "hour_default")]
    pub sun_hour: f32,
    /// unghiul nordului proiectului fata de +Y (grade, in sens trigonometric)
    #[serde(default)]
    pub north_deg: f32,
}

fn lat_default() -> f32 {
    44.43
}
fn lon_default() -> f32 {
    26.10
}
fn tz_default() -> f32 {
    3.0
}
fn date_default() -> String {
    "2026-06-21".into()
}
fn hour_default() -> f32 {
    14.0
}

fn ao_default() -> f32 {
    0.8
}

fn edges_default() -> f32 {
    0.55
}

fn one() -> f32 {
    1.0
}

fn sun_az() -> f32 {
    -60.0
}
fn sun_el() -> f32 {
    50.0
}

impl RenderSettings {
    /// (expunere, ambient, soare) pentru shader, in limite sigure
    pub fn light(&self) -> Vec3 {
        Vec3::new(self.exposure.clamp(0.3, 2.5), self.ambient.clamp(0.0, 3.0), self.sun_power.clamp(0.0, 3.0))
    }
    pub fn reset_light(&mut self) {
        (self.sun_az, self.sun_el, self.exposure, self.ambient, self.sun_power, self.ao) = (sun_az(), sun_el(), 1.0, 1.0, 1.0, ao_default());
        self.edges = edges_default();
    }
    pub fn sun_dir(&self) -> Vec3 {
        let (a, e) = (self.sun_az.to_radians(), self.sun_el.to_radians());
        Vec3::new(e.cos() * a.cos(), e.cos() * a.sin(), e.sin()).normalize()
    }
}

impl Default for RenderSettings {
    fn default() -> Self {
        RenderSettings { width: 1920, height: 1080, fps: 30, quality: 92, supersample: true, background: Background::Theme, format: VideoFormat::Mp4, sun_az: sun_az(), sun_el: sun_el(), exposure: 1.0, ambient: 1.0, sun_power: 1.0, ao: ao_default(), edges: edges_default(), show_frame: false, overlay_4d: true, legend_on_images: true, shadows: true, sun_from_time: false, lat: lat_default(), lon: lon_default(), tz: tz_default(), sun_date: date_default(), sun_hour: hour_default(), north_deg: 0.0 }
    }
}

pub const RESOLUTIONS: [(u32, u32, &str); 5] = [(1280, 720, "HD 720p"), (1920, 1080, "Full HD 1080p"), (2560, 1440, "QHD 1440p"), (3840, 2160, "4K UHD"), (1080, 1080, "Square 1080")];

/// Pozitia soarelui (algoritmul NOAA simplificat, precizie ~0.5 grade) pentru o data (zile de la 1970-01-01),
/// ora locala, fusul orar si coordonatele geografice. Intoarce (azimut de la Nord in sensul acelor de ceasornic,
/// elevatie deasupra orizontului), in grade.
pub fn sun_position(day: i32, hour_local: f32, tz: f32, lat: f32, lon: f32) -> (f32, f32) {
    let (lat, lon) = (lat as f64, lon as f64);
    // zile juliene de la J2000.0
    let jd = day as f64 + 2440587.5 + (hour_local as f64 - tz as f64) / 24.0;
    let n = jd - 2451545.0;
    let g = (357.529 + 0.98560028 * n).to_radians();
    let q = 280.459 + 0.98564736 * n;
    let l = (q + 1.915 * g.sin() + 0.020 * (2.0 * g).sin()).to_radians();
    let e = (23.439 - 0.00000036 * n).to_radians();
    let dec = (e.sin() * l.sin()).asin();
    let ra = (e.cos() * l.sin()).atan2(l.cos());
    // timpul sideral la Greenwich -> unghiul orar local
    let gmst = (18.697374558 + 24.06570982441908 * n).rem_euclid(24.0);
    let ha = ((gmst * 15.0 + lon).to_radians() - ra).rem_euclid(std::f64::consts::TAU);
    let latr = lat.to_radians();
    let el = (latr.sin() * dec.sin() + latr.cos() * dec.cos() * ha.cos()).asin();
    let az = (-ha.sin()).atan2(dec.tan() * latr.cos() - latr.sin() * ha.cos());
    (az.to_degrees().rem_euclid(360.0) as f32, el.to_degrees() as f32)
}

impl RenderSettings {
    /// Daca soarele e dupa data / ora / locatie: actualizeaza azimutul si elevatia folosite de randare
    /// (azimutul din scena = de la +X, in sens trigonometric; nordul proiectului = +Y rotit cu `north_deg`).
    pub fn update_sun(&mut self) {
        if !self.sun_from_time {
            return;
        }
        let Some(day) = crate::timeline::parse_date(&self.sun_date) else { return };
        let (az_n, el) = sun_position(day, self.sun_hour, self.tz, self.lat, self.lon);
        // de la Nord in sens orar -> de la +X in sens trigonometric (Nord = +Y = 90 grade)
        let a = (90.0 - az_n + self.north_deg).rem_euclid(360.0);
        self.sun_az = if a > 180.0 { a - 360.0 } else { a };
        // sub orizont: lumina ramane razanta (fara umbre aberante de jos in sus)
        self.sun_el = el.max(2.0);
    }
}
