//! Randare progresiva (CPU) pentru o imagine statica, cu anulare si export PNG.
//! Lambert + soare cu umbre moi (discul soarelui esantionat) + cer cu ambient occlusion, pe BVH global.
//! Fiecare esantion e aleator pe pixel (zgomot Monte Carlo); la final Intel Open Image Denoise
//! curata zgomotul folosind albedo-ul si normala primei intersectii.

use rayon::prelude::*;
use crate::model::Bvh;
use glam::Vec3;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Starea denoise-ului unui job.
pub const DN_OFF: u8 = 0;
pub const DN_RUNNING: u8 = 1;
pub const DN_DONE: u8 = 2;
pub const DN_FAILED: u8 = 3;

/// raza unghiulara a discului soarelui (rad): umbre moi, dar inca precise
const SUN_RADIUS: f32 = 0.035;
/// distanta pana la care un obstacol intuneca lumina cerului (ambient occlusion), in metri
const AO_DIST: f32 = 2.5;
/// raza becului (m) pentru lampile Point / Spot: umbrele lampilor au margini moi, ca in realitate
const LAMP_RADIUS: f32 = 0.06;
/// raza unghiulara (rad) a luminilor Distant: penumbra ca la soare
const DISTANT_RADIUS: f32 = 0.02;

pub struct Scene {
    pub tris: Vec<Vec3>,   // 3 vertexuri per triunghi
    /// culoarea finala (material / override / profil, ca in vederea 3D), alfa = opacitatea
    pub colors: Vec<[u8; 4]>,
    /// parametrii materialului per triunghi, ca in shader: [model sau 16 + textura, luciu, metal, scara]
    pub mats: Vec<[u8; 4]>,
    /// OpenPBR per triunghi: [coat, coat roughness, sheen, IOR]
    pub extra: Vec<[f32; 4]>,
    /// texturile imagine (512 x 512 RGBA), in ordinea straturilor de pe GPU
    pub textures: Vec<Vec<u8>>,
    /// planurile sectiunii active (n, w): se pastreaza punctele cu n·p <= w
    pub planes: Vec<glam::Vec4>,
    /// lampi punctuale in model
    pub lights: Vec<crate::lights::SceneLight>,
    pub bvh: Bvh,
}

impl Scene {
    /// The visible triangles with their final colours and materials (as in the 3D view), without the BVH.
    pub fn gather(app: &crate::app::App) -> Self {
        // Camera si geometria trebuie sa fie in ACELASI spatiu de coordonate.
        // Renderer-ul OpenGL foloseste vertecsi locali (m.pos) si cam.eye() local,
        // fara origin (origin e doar pentru export / coordonate reale). Daca adaugam
        // origin aici, razele nu mai lovesc modelul si imaginea ramane doar "cer".
        let mut tris = Vec::new();
        let mut colors = Vec::new();
        let mut mats = Vec::new();
        let mut extra = Vec::new();
        let mat_of = |id: crate::model::ElemId| -> [f32; 4] {
            if let Some(k) = app.mat_cache.as_ref().and_then(|c| c.get(&id).copied()) {
                if let Some(m) = app.materials.get(k as usize) {
                    return [m.coat as f32 / 100.0, m.coat_roughness as f32 / 100.0, m.sheen as f32 / 100.0, m.ior];
                }
            }
            [0.0, 0.05, 0.0, crate::materials::default_ior()]
        };
        for (mi, m) in app.loaded_models() {
            // aspectul calculat pentru vederea 3D: materiale, override-uri, profil, 4D, ascunse
            let looks = app.looks.get(&mi).filter(|l| l.len() == m.elements.len());
            for (ei, e) in m.elements.iter().enumerate() {
                let look = looks.map(|l| l[ei]);
                let id = crate::model::ElemId { m: mi as u32, e: ei as u32 };
                // elementele ascunse nu se randeaza (inainte apareau in path trace)
                if e.default_hidden || look.map_or(!app.is_visible(id), |l| l.hidden) {
                    continue;
                }
                let (t0, t1) = (e.tri_start as usize, (e.tri_start + e.tri_count) as usize);
                for t in t0..t1 {
                    let i = t * 3;
                    let (Some(a), Some(b), Some(c)) = (m.pos.get(i).copied(), m.pos.get(i + 1).copied(), m.pos.get(i + 2).copied()) else { continue };
                    let file = m.tri_color.get(t).copied().unwrap_or([200, 200, 200, 255]);
                    let col = look.and_then(|l| l.col).unwrap_or(file);
                    if col[3] < 3 {
                        continue;
                    }
                    tris.extend_from_slice(&[a, b, c]);
                    colors.push(col);
                    mats.push(look.map_or([0; 4], |l| l.gpu));
                    extra.push(mat_of(id));
                }
            }
        }
        let textures = app.tex_layers.iter().map(|t| t.1.clone()).collect();
        let planes = app.section.gpu_planes();
        let lights: Vec<_> = app.lights.iter().filter(|l| l.enabled).cloned().collect();
        Scene { tris, colors, mats, extra, textures, planes, lights, bvh: Bvh::default() }
    }

    pub fn build(app: &crate::app::App) -> Self {
        let mut s = Self::gather(app);
        s.bvh = Bvh::build(&s.tris);
        s
    }

    /// punctul ramane dupa sectiune (ca in shader)
    fn kept(&self, p: Vec3) -> bool {
        self.planes.iter().all(|pl| pl.truncate().dot(p) <= pl.w + 1e-4)
    }

    /// prima intersectie vizibila (sectiunea respectata)
    fn hit(&self, o: Vec3, d: Vec3, tmax: f32) -> Option<(f32, u32)> {
        if self.planes.is_empty() {
            self.bvh.raycast(&self.tris, o, d, tmax, &|_| true)
        } else {
            self.bvh.raycast(&self.tris, o, d, tmax, &|p| self.kept(p))
        }
    }

    /// lumina care trece spre soare / cer: sticla lasa sa treaca o parte, opacul opreste tot
    fn transmit(&self, o: Vec3, d: Vec3, tmax: f32) -> f32 {
        let (mut o, mut left, mut k) = (o, tmax, 1.0f32);
        for _ in 0..4 {
            let Some((t, tri)) = self.hit(o, d, left) else { return k };
            let a = self.colors[tri as usize][3] as f32 / 255.0;
            if a > 0.95 {
                return 0.0;
            }
            k *= 1.0 - a;
            o += d * (t + 1e-3);
            left -= t + 1e-3;
            if left <= 0.0 || k < 0.02 {
                return k;
            }
        }
        k
    }

    /// (albedo, rugozitate, metalicitate, normala tangenta) in punctul p, din hartile PBR sau din textura simpla
    fn surface(&self, tri: usize, p: Vec3, n: Vec3) -> ([f32; 3], f32, f32, Option<Vec3>) {
        let c = self.colors[tri];
        let base = [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0];
        let m = self.mats[tri];
        let code = m[0];
        if code == 0 {
            return (base, 1.0 - m[1] as f32 / 255.0, m[2] as f32 / 255.0, None);
        }
        let sc = 0.05 + m[3] as f32 / 255.0 * 4.0;
        let an = n.abs();
        let uv = (if an.z >= an.x && an.z >= an.y { glam::Vec2::new(p.x, p.y) } else if an.x >= an.y { glam::Vec2::new(p.y, p.z) } else { glam::Vec2::new(p.x, p.z) }) / sc;
        let s = crate::materials::tex_size() as usize;
        let sample = |layer: usize| -> Option<[u8; 4]> {
            let tex = self.textures.get(layer)?;
            let x = (uv.x.rem_euclid(1.0) * s as f32) as usize % s;
            let y = (uv.y.rem_euclid(1.0) * s as f32) as usize % s;
            let i = (y * s + x) * 4;
            Some([tex.get(i)?.to_owned(), tex.get(i + 1)?.to_owned(), tex.get(i + 2)?.to_owned(), tex.get(i + 3)?.to_owned()])
        };
        if code == crate::materials::PBR_CODE {
            let l = m[1] as usize;
            if let (Some(ca), Some(cs)) = (sample(l), sample(l + 1)) {
                let f = |b: u8| b as f32 / 255.0;
                let albedo = [f(ca[0]), f(ca[1]), f(ca[2])];
                // normala din spatiul tangent: construim baza TBN din normala geometrica
                let (t, b) = basis(n);
                let tn = Vec3::new(f(cs[0]) * 2.0 - 1.0, f(cs[1]) * 2.0 - 1.0, (f(cs[2]) * 2.0 - 1.0).max(0.05)).normalize_or_zero();
                let nm = (t * tn.x + b * tn.y + n * tn.z).normalize_or_zero();
                return (albedo, f(ca[3]), f(cs[3]), Some(nm));
            }
        } else if code >= 16 {
            if let Some(px) = sample((code - 16) as usize) {
                let f = |b: u8| b as f32 / 255.0;
                return ([f(px[0]), f(px[1]), f(px[2])], 1.0 - m[1] as f32 / 255.0, m[2] as f32 / 255.0, None);
            }
        }
        (pattern(code, base, uv), 1.0 - m[1] as f32 / 255.0, m[2] as f32 / 255.0, None)
    }
}

fn hash2(x: f32, y: f32) -> f32 {
    ((x * 127.1 + y * 311.7).sin() * 43758.545).fract().abs()
}

fn noise2(x: f32, y: f32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let (fx, fy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    lerp(lerp(hash2(ix, iy), hash2(ix + 1.0, iy), fx), lerp(hash2(ix, iy + 1.0), hash2(ix + 1.0, iy + 1.0), fx), fy)
}

/// modelele procedurale ale materialelor (caramida, gresie, lemn, piatra, iarba, panouri) - ca in shader
fn pattern(code: u8, base: [f32; 3], uv: glam::Vec2) -> [f32; 3] {
    let mul = |k: f32| [base[0] * k, base[1] * k, base[2] * k];
    let mix = |a: [f32; 3], b: [f32; 3], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
    let step = |edge: f32, x: f32| -> f32 { if x < edge { 1.0 } else { 0.0 } };
    match code {
        1 => {
            let (mut bx, by) = (uv.x, uv.y * 3.0);
            if by.floor().rem_euclid(2.0) > 0.5 {
                bx += 0.5;
            }
            let (fx, fy) = (bx - bx.floor(), by - by.floor());
            let mortar = (step(0.035, fx) + step(0.09, fy)).min(1.0);
            let v = 0.82 + 0.3 * hash2(bx.floor(), by.floor()) + 0.08 * noise2(uv.x * 30.0, uv.y * 30.0);
            mix(mul(v), [0.80, 0.78, 0.74], mortar)
        }
        2 => {
            let (fx, fy) = (uv.x - uv.x.floor(), uv.y - uv.y.floor());
            let g = (step(0.03, fx) + step(0.03, fy)).min(1.0);
            mix(mul(0.92 + 0.12 * hash2(uv.x.floor(), uv.y.floor())), mul(0.62), g)
        }
        3 => {
            let r = uv.x * 7.0 + noise2(uv.x * 1.5, uv.y * 10.0) * 2.5;
            mul(0.74 + 0.36 * (0.5 + 0.5 * (r * std::f32::consts::PI).sin()))
        }
        4 => mul(0.80 + 0.32 * (noise2(uv.x * 6.0, uv.y * 6.0) * 0.55 + noise2(uv.x * 25.0, uv.y * 25.0) * 0.30 + noise2(uv.x * 90.0, uv.y * 90.0) * 0.15)),
        5 => mul(0.65 + 0.55 * (noise2(uv.x * 14.0, uv.y * 14.0) * 0.6 + noise2(uv.x * 60.0, uv.y * 60.0) * 0.4)),
        6 => {
            let (fx, fy) = (uv.x - uv.x.floor(), uv.y * 0.5 - (uv.y * 0.5).floor());
            let j = (step(0.012, fx) + step(0.006, fy)).min(1.0);
            mix(base, mul(0.55), j)
        }
        _ => base,
    }
}

pub struct PathTraceJob {
    pub width: u32,
    pub height: u32,
    pub samples_target: u32,
    pub time_limit_secs: f32,
    pub cancel: AtomicBool,
    pub buf: Mutex<Vec<f32>>,   // RGB linear, width*height*3
    pub count: AtomicU32,
    pub done: AtomicBool,
    pub started: Instant,
    pub scene: Arc<Scene>,
    pub cam_eye: Vec3,
    pub cam_fwd: Vec3,
    pub cam_up: Vec3,
    pub cam_right: Vec3,
    pub fov_deg: f32,
    pub sun: Vec3,
    pub light: f32,
    /// mediu: (mod, intensitate, rotatie grade)
    pub env: (u8, f32, f32),
    /// scena de noapte: cer intunecat
    pub night: bool,
    /// HDRI / fotografie folosita ca mediu (None = cer procedural)
    pub hdri: Option<std::sync::Arc<crate::hdri::Hdri>>,
    /// albedo (RGB) + normala (XYZ) ale primei intersectii, acumulate ca `buf` (6 valori / pixel)
    pub aux: Mutex<Vec<f32>>,
    /// curata zgomotul cu Intel Open Image Denoise la final
    pub denoise: bool,
    /// DN_OFF / DN_RUNNING / DN_DONE / DN_FAILED
    pub denoise_state: AtomicU8,
    /// imaginea curatata (RGB liniar, deja impartita la numarul de esantioane)
    pub denoised: Mutex<Option<Vec<f32>>>,
    pub denoise_msg: Mutex<String>,
}

// We'll use Arc<PathTraceJob> everywhere instead.
#[allow(clippy::too_many_arguments)]
pub fn start_arc(params: (u32, u32, u32, f32), scene: Arc<Scene>, cam_eye: Vec3, cam_fwd: Vec3, cam_up: Vec3, cam_right: Vec3, fov_deg: f32, sun: Vec3, light: f32, env: (u8, f32, f32), night: bool, hdri: Option<std::sync::Arc<crate::hdri::Hdri>>, denoise: bool) -> Arc<PathTraceJob> {
    let job = new_job(params, scene, cam_eye, cam_fwd, cam_up, cam_right, fov_deg, sun, light, env, night, hdri, denoise);
    let j = job.clone();
    std::thread::spawn(move || {
        // a panic while rendering must not leave the job running for ever (no new render could start)
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(j.clone()))).is_err() {
            *j.denoise_msg.lock().unwrap_or_else(|e| e.into_inner()) = "internal error while rendering".into();
            if j.denoise {
                j.denoise_state.store(DN_FAILED, Ordering::Relaxed);
            }
            j.done.store(true, Ordering::Relaxed);
        }
    });
    job
}

/// A render job without the CPU worker: the GPU path tracer fills `buf` / `aux` / `count`, then calls
/// `finish_job`.
#[allow(clippy::too_many_arguments)]
pub fn new_job(params: (u32, u32, u32, f32), scene: Arc<Scene>, cam_eye: Vec3, cam_fwd: Vec3, cam_up: Vec3, cam_right: Vec3, fov_deg: f32, sun: Vec3, light: f32, env: (u8, f32, f32), night: bool, hdri: Option<std::sync::Arc<crate::hdri::Hdri>>, denoise: bool) -> Arc<PathTraceJob> {
    let (w, h, samples, tlimit) = params;
    Arc::new(PathTraceJob {
        width: w, height: h, samples_target: samples, time_limit_secs: tlimit,
        cancel: AtomicBool::new(false),
        buf: Mutex::new(vec![0.0; (w * h * 3) as usize]),
        count: AtomicU32::new(0),
        done: AtomicBool::new(false),
        started: Instant::now(),
        scene, cam_eye, cam_fwd, cam_up, cam_right, fov_deg, sun, light, env, night, hdri,
        aux: Mutex::new(vec![0.0; (w * h * 6) as usize]),
        denoise,
        denoise_state: AtomicU8::new(DN_OFF),
        denoised: Mutex::new(None),
        denoise_msg: Mutex::new(String::new()),
    })
}

/// The end of a GPU render: denoise in the background (if asked), then mark the job done.
pub fn finish_job(job: Arc<PathTraceJob>) {
    std::thread::spawn(move || {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| finish(&job))).is_err() {
            job.done.store(true, Ordering::Relaxed);
        }
    });
}

fn finish(job: &PathTraceJob) {
    if job.denoise && job.count.load(Ordering::Relaxed) > 0 {
        job.denoise_state.store(DN_RUNNING, Ordering::Relaxed);
        match denoise_job(job) {
            Ok(img) => {
                *job.denoised.lock().unwrap_or_else(|e| e.into_inner()) = Some(img);
                job.denoise_state.store(DN_DONE, Ordering::Relaxed);
            }
            Err(e) => {
                *job.denoise_msg.lock().unwrap_or_else(|e| e.into_inner()) = e;
                job.denoise_state.store(DN_FAILED, Ordering::Relaxed);
            }
        }
    }
    job.done.store(true, Ordering::Relaxed);
}

/// Generator pseudo-aleator mic (PCG) - unul per pixel si per esantion, fara stare comuna intre fire.
struct Rng(u64);

impl Rng {
    fn new(px: u32, py: u32, s: u32) -> Self {
        let mut r = Rng(((px as u64) << 40) ^ ((py as u64) << 20) ^ (s as u64) ^ 0x9E37_79B9_7F4A_7C15);
        r.next();
        r.next();
        r
    }
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        let x = (((self.0 >> 18) ^ self.0) >> 27) as u32;
        let rot = (self.0 >> 59) as u32;
        (x.rotate_right(rot) >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// punct uniform in sfera unitate (respingere): esantionarea suprafetei luminii -> umbre moi
/// Lumina directa (difuza) a lampilor intr-un punct cu normala `n`, cu umbre: folosita pentru ricoseul
/// luminii lampilor (punctul vazut de o raza de emisfera). Aceeasi atenuare si con ca lumina directa.
fn lamp_irradiance(sc: &Scene, p: Vec3, n: Vec3, rng: &mut Rng) -> [f32; 3] {
    use crate::lights::LightKind;
    let mut e = [0.0f32; 3];
    for lamp in sc.lights.iter() {
        let (l, d, mut atten) = match lamp.kind {
            LightKind::Point | LightKind::Spot => {
                let lv = Vec3::from(lamp.pos) + in_unit_ball(rng) * LAMP_RADIUS - p;
                let d = lv.length();
                if d < 1e-3 {
                    continue;
                }
                let a = if lamp.radius <= 0.001 { lamp.power.max(0.0) } else { lamp.power.max(0.0) / (1.0 + (d / lamp.radius).powi(2)) };
                (lv / d, d, a)
            }
            LightKind::Distant => {
                let dir = (Vec3::from(lamp.target) - Vec3::from(lamp.pos)).normalize_or_zero();
                if dir == Vec3::ZERO {
                    continue;
                }
                (-dir, 1e5, lamp.power.max(0.0))
            }
        };
        if lamp.kind == LightKind::Spot {
            let half = (lamp.angle.clamp(5.0, 120.0) * 0.5).to_radians();
            let axis = (Vec3::from(lamp.target) - Vec3::from(lamp.pos)).normalize_or_zero();
            let soft = half.cos().max(1e-4);
            atten *= (((-l).dot(axis).max(0.0) - soft) / (1.0 - soft)).clamp(0.0, 1.0).powf(1.5);
        }
        let nl = n.dot(l).max(0.0);
        if atten <= 0.0 || nl <= 0.0 || sc.hit(p, l, d - 1e-2).is_some() {
            continue;
        }
        let lc = lamp.linear();
        for (k, v) in e.iter_mut().enumerate() {
            *v += DIFFUSE * nl * atten * lc[k];
        }
    }
    e
}

fn in_unit_ball(rng: &mut Rng) -> Vec3 {
    for _ in 0..16 {
        let v = Vec3::new(rng.next() * 2.0 - 1.0, rng.next() * 2.0 - 1.0, rng.next() * 2.0 - 1.0);
        if v.length_squared() <= 1.0 {
            return v;
        }
    }
    Vec3::ZERO
}

/// baza ortonormala in jurul lui n
fn basis(n: Vec3) -> (Vec3, Vec3) {
    let a = if n.x.abs() > 0.9 { Vec3::Y } else { Vec3::X };
    let t = n.cross(a).normalize();
    (t, n.cross(t))
}

fn run(job: Arc<PathTraceJob>) {
    let w = job.width as f32;
    let h = job.height as f32;
    let aspect = w / h;
    let fov = job.fov_deg.to_radians();
    let half_h = (fov * 0.5).tan();
    let half_w = half_h * aspect;
    let mut s = 0u32;
    while s < job.samples_target && !job.cancel.load(Ordering::Relaxed) {
        if job.time_limit_secs > 0.0 && job.started.elapsed().as_secs_f32() >= job.time_limit_secs {
            break;
        }
        let mut buf = job.buf.lock().unwrap_or_else(|e| e.into_inner());
        let mut aux = job.aux.lock().unwrap_or_else(|e| e.into_inner());
        let width = job.width as usize;
        // Un sample = toate pixelii, impartiti pe toate nucleele (rayon).
        buf.par_chunks_mut(3).zip(aux.par_chunks_mut(6)).enumerate().for_each(|(i, (p, x))| {
            let px = (i % width) as u32;
            let py = (i / width) as u32;
            let mut rng = Rng::new(px, py, s);
            // anti-aliasing: alt punct din pixel la fiecare esantion
            let u = (px as f32 + rng.next()) / w;
            let v = (py as f32 + rng.next()) / h;
            let ndc_x = u * 2.0 - 1.0;
            let ndc_y = 1.0 - v * 2.0;
            let dir = (job.cam_fwd + job.cam_right * (ndc_x * half_w) + job.cam_up * (ndc_y * half_h)).normalize();
            let (col, alb, nrm) = trace(&job, job.cam_eye, dir, &mut rng);
            for k in 0..3 {
                p[k] += col[k];
                x[k] += alb[k];
                x[3 + k] += nrm[k];
            }
        });
        drop(aux);
        drop(buf);
        job.count.fetch_add(1, Ordering::Relaxed);
        s += 1;
    }
    finish(&job);
}

/// Imaginea medie (buf / count) curatata de OIDN cu albedo + normala.
fn denoise_job(job: &PathTraceJob) -> Result<Vec<f32>, String> {
    let n = job.count.load(Ordering::Relaxed).max(1) as f32;
    let color: Vec<f32> = job.buf.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|x| x / n).collect();
    let aux = job.aux.lock().unwrap_or_else(|e| e.into_inner());
    let mut albedo = Vec::with_capacity(color.len());
    let mut normal = Vec::with_capacity(color.len());
    for x in aux.chunks_exact(6) {
        albedo.extend_from_slice(&[(x[0] / n).clamp(0.0, 1.0), (x[1] / n).clamp(0.0, 1.0), (x[2] / n).clamp(0.0, 1.0)]);
        normal.extend_from_slice(&[x[3] / n, x[4] / n, x[5] / n]);
    }
    drop(aux);
    crate::oidn::denoise(&crate::oidn::Input { width: job.width as usize, height: job.height as usize, color: &color, albedo: Some(&albedo), normal: Some(&normal) })
}

/// Mediul (IBL) pe directia d: cer, orizont, soare si nori slabi.
/// E procedural, deci functioneaza offline; inlocuieste gradientul albastru din randarea veche.
fn env(job: &PathTraceJob, d: Vec3) -> [f32; 3] {
    if job.night {
        // cer de noapte: foarte putina lumina, lasam lampile sa domine
        let up = (d.z * 0.5 + 0.5).clamp(0.0, 1.0);
        let s = job.sun.normalize_or_zero();
        let moon = 0.35 * d.dot(s).max(0.0).powf(180.0);
        return [0.012 + 0.045 * up + moon, 0.018 + 0.055 * up + moon, 0.045 + 0.10 * up + moon * 1.1];
    }
    env_cols(job.env, job.sun, d, job.hdri.as_deref())
}

fn env_cols(env: (u8, f32, f32), sun: Vec3, d: Vec3, hdri: Option<&crate::hdri::Hdri>) -> [f32; 3] {
    // HDRI-ul (fisier .hdr sau fotografie) inlocuieste cerul procedural
    if let Some(h) = hdri {
        let a = env.2.to_radians();
        let (ca, sa) = (a.cos(), a.sin());
        let rd = Vec3::new(d.x * ca - d.y * sa, d.x * sa + d.y * ca, d.z);
        let c = crate::hdri::sample(h, rd);
        let k = env.1.max(0.0);
        return [c[0] * k, c[1] * k, c[2] * k];
    }
    // rotirea mediului in jurul axei Z
    let a = env.2.to_radians();
    let (ca, sa) = (a.cos(), a.sin());
    let d = Vec3::new(d.x * ca - d.y * sa, d.x * sa + d.y * ca, d.z);
    let up = (d.z * 0.5 + 0.5).clamp(0.0, 1.0);
    let horizon = (1.0 - d.z.abs()).powi(4);
    // palete: 0 = exterior, 1 = studio (neutru), 2 = apus (cald)
    let (zenith, sky, ground) = match env.0 {
        1 => ([0.62, 0.64, 0.68], [0.78, 0.79, 0.81], [0.26, 0.27, 0.29]),
        2 => ([0.34, 0.26, 0.36], [0.96, 0.52, 0.26], [0.20, 0.16, 0.19]),
        _ => ([0.26, 0.38, 0.58], [0.72, 0.78, 0.88], [0.33, 0.35, 0.38]),
    };
    let mix = |k: usize| ground[k] + (sky[k] - ground[k]) * up + (zenith[k] - sky[k]) * up.powi(3);
    let mut c = [mix(0), mix(1), mix(2)];
    let glow = if env.0 == 1 { 0.0 } else {
        let s = sun.normalize_or_zero();
        let cos = d.dot(s).max(0.0);
        cos.powf(220.0) * 9.0 + cos.powf(7.0) * 0.45
    };
    c[0] += glow + horizon * 0.06;
    c[1] += glow * if env.0 == 2 { 0.55 } else { 0.93 } + horizon * 0.05;
    c[2] += glow * if env.0 == 2 { 0.30 } else { 0.80 } + horizon * 0.04;
    // variatie lenta, determinista (norii) - doar exterior
    if env.0 != 1 {
        let w = ((d.x * 4.3).sin() * (d.y * 5.7).cos() * 0.5 + 0.5) * up.powi(2) * 0.14;
        c[0] += w;
        c[1] += w;
        c[2] += w * 1.05;
    }
    let k = env.1.max(0.0);
    [c[0] * k, c[1] * k, c[2] * k]
}

/// F0 (reflexia la incidenta normala) din IOR, amestecat cu culoarea pentru metale.
fn f0_of(ior: f32, base: [f32; 3], metal: f32) -> [f32; 3] {
    let f = ((ior - 1.0) / (ior + 1.0)).powi(2);
    [f + (base[0] - f) * metal, f + (base[1] - f) * metal, f + (base[2] - f) * metal]
}

/// BRDF microfacet GGX: D * G * F / (4 * NdotV * NdotL), conserva energia.
fn ggx(n: Vec3, v: Vec3, l: Vec3, rough: f32, f0: [f32; 3]) -> [f32; 3] {
    let (nh, nv, nl) = (n.dot((v + l).normalize_or_zero()).max(0.0), n.dot(v).max(1e-4), n.dot(l).max(0.0));
    if nl <= 0.0 || nh <= 0.0 {
        return [0.0; 3];
    }
    let a = (rough * rough).max(0.002);
    let a2 = a * a;
    let dd = nh * nh * (a2 - 1.0) + 1.0;
    let d = a2 / (std::f32::consts::PI * dd * dd);
    let k = ((rough + 1.0) * (rough + 1.0)) / 8.0;
    let g = (nv / (nv * (1.0 - k) + k)) * (nl / (nl * (1.0 - k) + k));
    let vh = v.dot((v + l).normalize_or_zero()).max(0.0);
    let f = |i: usize| f0[i] + (1.0 - f0[i]) * (1.0 - vh).powi(5);
    let m = d * g / (4.0 * nv * nl);
    [m * f(0), m * f(1), m * f(2)]
}

/// Directia unei reflexii GGX esantionate (importance sampling), pentru bounce-ul 2.
fn ggx_reflect(d: Vec3, n: Vec3, t: Vec3, b: Vec3, rough: f32, rng: &mut Rng) -> Vec3 {
    let a = (rough * rough).max(0.002);
    let r1 = rng.next().max(1e-6);
    let r2 = rng.next();
    let ct = ((1.0 - r1) / (1.0 + (a * a - 1.0) * r1)).sqrt();
    let st = (1.0 - ct * ct).sqrt();
    let phi = std::f32::consts::TAU * r2;
    let h = (t * (st * phi.cos()) + b * (st * phi.sin()) + n * ct).normalize_or_zero();
    (d - h * (2.0 * d.dot(h))).normalize_or_zero()
}

/// cate reflexii / treceri prin sticla urmarim (adancimea razei)
const MAX_DEPTH: u32 = 4;
const DIFFUSE: f32 = 0.22;

/// (culoare, albedo, normala) pentru raza o + t*d
fn trace(job: &PathTraceJob, o: Vec3, d: Vec3, rng: &mut Rng) -> ([f32; 3], [f32; 3], [f32; 3]) {
    trace_d(job, o, d, rng, 0)
}

fn trace_d(job: &PathTraceJob, o: Vec3, d: Vec3, rng: &mut Rng, depth: u32) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let sc = &job.scene;
    let tris = &sc.tris;
    let Some((t, tri)) = sc.hit(o, d, 1e6) else {
        let s = env(job, d);
        return (s, s, [0.0; 3]);
    };
    let ti = tri as usize;
    let i = ti * 3;
    let (a, b, c) = (tris[i], tris[i + 1], tris[i + 2]);
    let mut n = (b - a).cross(c - a).normalize_or_zero();
    // fata vazuta din spate: normala spre camera (altfel ar iesi neagra)
    if n.dot(d) > 0.0 {
        n = -n;
    }
    let hit = o + d * t;
    let alpha = sc.colors[ti][3] as f32 / 255.0;
    let (base, rough, metal, mapped_n) = sc.surface(ti, hit, n);
    let m = sc.mats[ti];
    let ex = sc.extra.get(ti).copied().unwrap_or([0.0, 0.05, 0.0, crate::materials::default_ior()]);
    let (coat, coat_rough, sheen, ior) = (ex[0], ex[1], ex[2], ex[3]);
    // normala din harta inlocuieste normala geometrica pentru umbrire / reflexii
    if let Some(nm) = mapped_n {
        n = nm;
    }
    // rugozitatea din harta (0 = oglinda) e inversul luciului global
    let gloss = if m[0] == crate::materials::PBR_CODE { (1.0 - rough).clamp(0.03, 1.0) } else { m[1] as f32 / 255.0 };
    // sticla / materiale transparente: raza trece mai departe cu probabilitatea (1 - opacitate)
    if alpha < 0.99 && depth < MAX_DEPTH && rng.next() > alpha {
        let (behind, _, _) = trace_d(job, hit + d * 1e-3, d, rng, depth + 1);
        // sticla nuanteaza putin lumina care trece
        let tint = |k: usize| behind[k] * (0.85 + 0.15 * base[k]);
        return ([tint(0), tint(1), tint(2)], base, n.to_array());
    }
    let p = hit + n * 1e-3;
    // soarele: o directie aleatoare pe disc -> umbre cu margini moi
    let sun_dir = job.sun.normalize_or_zero();
    let mut lit_sun = 0.0;
    if sun_dir != Vec3::ZERO {
        let (st, sb) = basis(sun_dir);
        let (r, phi) = (SUN_RADIUS * rng.next().sqrt(), std::f32::consts::TAU * rng.next());
        let l = (sun_dir + st * (r * phi.cos()) + sb * (r * phi.sin())).normalize();
        let ndotl = n.dot(l).max(0.0);
        if ndotl > 0.0 {
            // sticla lasa soarele sa treaca; 0.35 = penumbra (nu negru total)
            let tr = sc.transmit(p, l, 1e6);
            lit_sun = tr;
        }
    }
    // IBL: o raza pe emisfera, culoarea din mediu (cer + soare), cu ambient occlusion
    let (nt, nb) = basis(n);
    let (r1, r2) = (rng.next(), rng.next());
    let (r, phi) = (r1.sqrt(), std::f32::consts::TAU * r2);
    let hd = (nt * (r * phi.cos()) + nb * (r * phi.sin()) + n * (1.0 - r1).max(0.0).sqrt()).normalize();
    let open = 0.18 + 0.82 * sc.transmit(p, hd, AO_DIST);
    let irradiance = env(job, hd);
    let up = n.z.max(0.0);
    let amb = 0.30 + 0.28 * up;
    let k = amb * open;
    let mut col = [base[0] * irradiance[0] * k, base[1] * irradiance[1] * k, base[2] * irradiance[2] * k];
    // 0.28: lumina reflectata de la lampi (un ricoseu difuz). Raza de emisfera de mai sus loveste o
    // suprafata apropiata; lumina directa a lampilor in acel punct, colorata de suprafata, se intoarce
    // aici. Un perete luminat de o lampa lumineaza astfel podeaua si tavanul - interioare realiste.
    if !sc.lights.is_empty() && depth == 0 {
        if let Some((t2, tri2)) = sc.hit(p + n * 1e-3, hd, 40.0) {
            let q = p + n * 1e-3 + hd * t2;
            let j = tri2 as usize * 3;
            let mut n2 = (tris[j + 1] - tris[j]).cross(tris[j + 2] - tris[j]).normalize_or_zero();
            if n2.dot(hd) > 0.0 {
                n2 = -n2;
            }
            let (b2, ..) = sc.surface(tri2 as usize, q, n2);
            let e = lamp_irradiance(sc, q + n2 * 1e-3, n2, rng);
            // cos-weighted: pdf si cos se simplifica; 0.55 = albedo mediu al ricoseului (conservare)
            for (k2, v) in col.iter_mut().enumerate() {
                *v += base[k2] * b2[k2] * e[k2] * 0.55 * (1.0 - metal);
            }
        }
    }

    let vd = -d;
    let head = n.dot(vd).abs();
    let f0 = f0_of(ior, base, metal);
    let sp = (1.0 - metal) * (1.0 - gloss) * 0.04 + metal * 0.25; // energie difuza redusa de specular
    for v in col.iter_mut() {
        *v *= 1.0 - sp;
    }
    // lumina directa prin BRDF GGX (microfacet) in loc de Blinn-Phong
    if sun_dir != Vec3::ZERO && lit_sun > 0.0 {
        let l = sun_dir;
        let g = ggx(n, vd, l, gloss.max(0.05), f0);
        let nl = n.dot(l).max(0.0);
        for (k, v) in col.iter_mut().enumerate() {
            *v += base[k] * nl * (1.0 - metal) * DIFFUSE + g[k] * lit_sun * job.light * nl;
        }
    }

    // lampi punctuale plasate in model: lumina difuza + reflexie GGX, cu umbra
    for lamp in sc.lights.iter() {
        use crate::lights::LightKind;
        let (l, d, atten_base) = match lamp.kind {
            LightKind::Point | LightKind::Spot => {
                // un punct aleator pe bec (nu centrul): umbrele moi apar prin acumularea esantioanelor
                let lv = Vec3::from(lamp.pos) + in_unit_ball(rng) * LAMP_RADIUS - p;
                let d = lv.length();
                if d < 1e-3 {
                    continue;
                }
                let dir = lv / d;
                let atten = if lamp.radius <= 0.001 {
                    lamp.power.max(0.0)
                } else {
                    lamp.power.max(0.0) / (1.0 + (d / lamp.radius).powi(2))
                };
                (dir, d, atten)
            }
            LightKind::Distant => {
                // directia de propagare e pozitie -> tinta; spre lumina (l) e opusul ei.
                // (inainte se folosea directia de propagare: luminau fetele din spate, umbrele erau inversate)
                let dir = (Vec3::from(lamp.target) - Vec3::from(lamp.pos)).normalize_or_zero();
                if dir == Vec3::ZERO {
                    continue;
                }
                let l = (-dir + in_unit_ball(rng) * DISTANT_RADIUS).normalize_or_zero();
                if l == Vec3::ZERO {
                    continue;
                }
                (l, 1e5, lamp.power.max(0.0))
            }
        };
        let mut atten = atten_base;
        if lamp.kind == LightKind::Spot {
            let half = (lamp.angle.clamp(5.0, 120.0) * 0.5).to_radians();
            let axis = (Vec3::from(lamp.target) - Vec3::from(lamp.pos)).normalize_or_zero();
            // l arata SPRE lampa; conul se masoara dinspre lampa spre punct (-l). Inainte era l: spoturile
            // nu luminau nimic in randarea finala
            let a = (-l).dot(axis).max(0.0);
            let soft = half.cos().max(1e-4);
            atten *= (((a - soft) / (1.0 - soft)).clamp(0.0, 1.0)).powf(1.5);
        }
        if atten <= 0.0 {
            continue;
        }
        let nl = n.dot(l).max(0.0);
        if nl <= 0.0 {
            continue;
        }
        let shadow = if sc.hit(p, l, d - 1e-2).is_some() { 0.12 } else { 1.0 };
        let lc = lamp.linear();
        let g = ggx(n, vd, l, gloss.max(0.05), f0);
        for (k, v) in col.iter_mut().enumerate() {
            *v += (base[k] * (1.0 - metal) * DIFFUSE + g[k]) * nl * atten * shadow * lc[k];
        }
    }

    // OpenPBR: stratul de luciu (clearcoat) - F0 = 4% pentru orice IOR
    if coat > 0.01 {
        let cg = ggx(n, vd, sun_dir, coat_rough.max(0.03), [0.04; 3]);
        let nl = n.dot(sun_dir).max(0.0) * lit_sun;
        for (k, v) in col.iter_mut().enumerate() {
            *v += coat * cg[k] * nl * job.light;
        }
    }
    // OpenPBR: sheen - reflexie moale pe margini, specifica tesaturilor
    if sheen > 0.01 {
        let edge = (1.0 - head).powi(4);
        for (k, v) in col.iter_mut().enumerate() {
            *v += sheen * edge * base[k] * irradiance[k] * 0.55;
        }
    }

    // reflexia scenei: importance sampling GGX + Fresnel (2-3 bounces)
    if (gloss > 0.02 || metal > 0.02 || coat > 0.01) && depth < MAX_DEPTH {
        let rough = if coat > 0.01 && rng.next() < coat { coat_rough } else { gloss };
        let rdir = ggx_reflect(d, n, nt, nb, rough.max(0.03), rng);
        let (rc, _, _) = trace_d(job, p, rdir, rng, depth + 1);
        let fres = |k: usize| f0[k] + (1.0 - f0[k]) * (1.0 - head).powi(5);
        let w = ((gloss * 0.55).max(metal * 0.75) + coat * 0.25).clamp(0.0, 0.95);
        for (k, v) in col.iter_mut().enumerate() {
            let f = (fres(k) * w).clamp(0.0, 1.0);
            *v = *v * (1.0 - f) + rc[k] * f;
        }
    }
    (col, base, n.to_array())
}

/// ACES filmic tonemap (curba folosita de blender / filme): pastri detaliile in lumini.
pub fn aces(x: f32) -> f32 {
    let x = x.max(0.0) * 0.6;
    ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
}

/// RGB liniar -> sRGB 8 bit cu tonemap ACES.
pub fn to_srgb(rgb: [f32; 3]) -> [u8; 3] {
    let mut out = [0u8; 3];
    for (i, v) in rgb.iter().enumerate() {
        out[i] = (aces(*v).powf(1.0 / 2.2) * 255.0 + 0.5) as u8;
    }
    out
}

pub fn save_png(path: &str, buf: &[f32], count: u32, w: u32, h: u32) -> Result<(), String> {
    let mut img = Vec::with_capacity((w * h * 3) as usize);
    let n = count.max(1) as f32;
    for p in buf.chunks_exact(3) {
        img.extend_from_slice(&to_srgb([p[0] / n, p[1] / n, p[2] / n]));
    }
    let f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(f, w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut wtr = enc.write_header().map_err(|e| e.to_string())?;
    wtr.write_image_data(&img).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// podea 20x20 m la z=0 + un cub de 2 m in mijloc (Z = sus, ca modelele)
    fn scene() -> Arc<Scene> {
        let mut tris: Vec<Vec3> = Vec::new();
        let q = |t: &mut Vec<Vec3>, a: Vec3, b: Vec3, c: Vec3, d: Vec3| t.extend_from_slice(&[a, b, c, a, c, d]);
        q(&mut tris, Vec3::new(-10., -10., 0.), Vec3::new(10., -10., 0.), Vec3::new(10., 10., 0.), Vec3::new(-10., 10., 0.));
        let (l, h) = (-1.0f32, 1.0f32);
        let v = |x: f32, y: f32, z: f32| Vec3::new(x, y, z + 1.0);
        q(&mut tris, v(l, l, h), v(h, l, h), v(h, h, h), v(l, h, h));
        q(&mut tris, v(l, l, l), v(h, l, l), v(h, l, h), v(l, l, h));
        q(&mut tris, v(h, l, l), v(h, h, l), v(h, h, h), v(h, l, h));
        q(&mut tris, v(h, h, l), v(l, h, l), v(l, h, h), v(h, h, h));
        q(&mut tris, v(l, h, l), v(l, l, l), v(l, l, h), v(l, h, h));
        let colors = vec![[200, 200, 200, 255]; tris.len() / 3];
        let mats = vec![[0u8; 4]; tris.len() / 3];
        let bvh = Bvh::build(&tris);
        let extra = vec![[0.0f32, 0.05, 0.0, crate::materials::default_ior()]; mats.len()];
        Arc::new(Scene { tris, colors, mats, extra, textures: Vec::new(), planes: Vec::new(), lights: Vec::new(), bvh })
    }

    /// aceeasi scena cu materiale: podeaua din caramida + cubul de sticla + o sectiune care taie cubul
    fn scene_with(cube_alpha: u8, floor_mat: [u8; 4], planes: Vec<glam::Vec4>) -> Arc<Scene> {
        let s = scene();
        let mut colors = s.colors.clone();
        let mut mats = s.mats.clone();
        mats[0] = floor_mat;
        mats[1] = floor_mat;
        for c in colors.iter_mut().skip(2) {
            c[3] = cube_alpha;
        }
        let extra = vec![[0.0f32, 0.05, 0.0, crate::materials::default_ior()]; mats.len()];
        Arc::new(Scene { tris: s.tris.clone(), colors, mats, extra, textures: Vec::new(), planes, lights: Vec::new(), bvh: Bvh::build(&s.tris) })
    }

    fn run_scene(sc: Arc<Scene>) -> Arc<PathTraceJob> {
        let eye = Vec3::new(0.0, -12.0, 9.0);
        let fwd = (Vec3::new(0.0, 0.0, 0.5) - eye).normalize();
        let right = fwd.cross(Vec3::Z).normalize();
        let up = right.cross(fwd);
        let job = start_arc((48, 48, 16, 0.0), sc, eye, fwd, up, right, 50.0, Vec3::new(0.6, 0.0, 0.8), 1.0, (0, 1.0, 0.0), false, None, false);
        while !job.done.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        job
    }

    fn lum(job: &PathTraceJob, x: usize, y: usize) -> f32 {
        let n = job.count.load(Ordering::Relaxed) as f32;
        let b = job.buf.lock().unwrap();
        let i = (y * job.width as usize + x) * 3;
        (b[i] + b[i + 1] + b[i + 2]) / (3.0 * n)
    }

    /// cubul de sticla lasa soarele sa treaca: umbra lui e mult mai slaba decat a cubului opac
    #[test]
    fn glass_lets_light_through() {
        let opaque = run_scene(scene_with(255, [0; 4], Vec::new()));
        let glass = run_scene(scene_with(40, [0; 4], Vec::new()));
        let shadow = |j: &PathTraceJob| (12..21).flat_map(|x| (22..32).map(move |y| (x, y))).map(|(x, y)| lum(j, x, y)).fold(f32::INFINITY, f32::min);
        assert!(shadow(&glass) > shadow(&opaque) * 1.3, "glass shadow {} vs opaque {}", shadow(&glass), shadow(&opaque));
        // si prin sticla se vede podeaua din spatele cubului (centrul imaginii e mai luminos)
        assert!(lum(&glass, 24, 22) > lum(&opaque, 24, 22));
    }

    /// modelul procedural al materialului (caramida) apare pe podea: pixelii variaza, nu sunt uniformi
    #[test]
    fn material_pattern_is_rendered() {
        // modelul procedural e verificat direct: zgomotul Monte Carlo din imagine ar ascunde diferentele mici
        let base = [0.8f32, 0.8, 0.8];
        let plain = super::pattern(0, base, glam::Vec2::new(0.5, 0.5));
        let brick: Vec<f32> = (0..30).map(|i| super::pattern(1, base, glam::Vec2::new(i as f32 * 0.11, i as f32 * 0.07))[0]).collect();
        let mn = brick.iter().copied().fold(f32::INFINITY, f32::min);
        let mx = brick.iter().copied().fold(f32::MIN, f32::max);
        assert!(mx - mn > 0.05, "patternul de caramida nu variaza: {mn}..{mx}");
        assert!((plain[0] - base[0]).abs() < 1e-6);
        // integrare: scena cu materialul de caramida se randeaza fara erori
        let brick_job = run_scene(scene_with(255, [1, 0, 0, 10], Vec::new()));
        assert!(brick_job.buf.lock().unwrap().iter().all(|v| v.is_finite()));
    }

    /// sectiunea (z <= 1) taie cubul: partea de sus dispare, ca in vederea 3D
    #[test]
    fn section_cuts_the_cube() {
        let full = run_scene(scene_with(255, [0; 4], Vec::new()));
        let cut = run_scene(scene_with(255, [0; 4], vec![glam::Vec4::new(0.0, 0.0, 1.0, 1.0)]));
        // partea de sus a cubului (z 1..2) e taiata: o parte vizibila din imagine se schimba
        let changed = (0..48).flat_map(|x| (0..48).map(move |y| (x, y))).filter(|&(x, y)| (lum(&cut, x, y) - lum(&full, x, y)).abs() > 0.05).count();
        assert!(changed > 20, "only {changed} pixels changed");
        // iar sub plan (podeaua departe de cub) imaginea ramane aceeasi
        assert!((lum(&cut, 44, 44) - lum(&full, 44, 44)).abs() < 0.03);
    }

    fn render(denoise: bool) -> Arc<PathTraceJob> {
        render_sized(denoise, 48, 8)
    }

    fn render_sized(denoise: bool, size: u32, samples: u32) -> Arc<PathTraceJob> {
        // camera deasupra, privind in jos spre cub; soarele din +X, sus
        let eye = Vec3::new(0.0, -12.0, 9.0);
        let fwd = (Vec3::new(0.0, 0.0, 0.5) - eye).normalize();
        let right = fwd.cross(Vec3::Z).normalize();
        let up = right.cross(fwd);
        let job = start_arc((size, size, samples, 0.0), scene(), eye, fwd, up, right, 50.0, Vec3::new(0.6, 0.0, 0.8), 1.0, (0, 1.0, 0.0), false, None, denoise);
        let t = Instant::now();
        while !job.done.load(Ordering::Relaxed) {
            assert!(t.elapsed().as_secs() < 60, "render too slow");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        job
    }

    #[test]
    fn lit_floor_shadow_and_sky_are_plausible() {
        let job = render(false);
        let n = job.count.load(Ordering::Relaxed) as f32;
        assert_eq!(n, 8.0);
        let buf = job.buf.lock().unwrap();
        let px = |x: usize, y: usize| {
            let i = (y * 48 + x) * 3;
            (buf[i] + buf[i + 1] + buf[i + 2]) / (3.0 * n)
        };
        assert!(buf.iter().all(|x| x.is_finite()));
        // sus = cer (albastru), jos = podea luminata; umbra cubului (spre -X) e mai intunecata decat podeaua libera din +X
        let i = (2 * 48 + 24) * 3;
        assert!(buf[i + 2] > buf[i], "top of the image should be sky-blue");
        let lit = px(40, 40);
        // umbra: la stanga cubului (soarele vine din +X), pe randurile bazei lui
        let shade = (12..21).flat_map(|x| (22..32).map(move |y| (x, y))).map(|(x, y)| px(x, y)).fold(f32::INFINITY, f32::min);
        assert!(lit > 0.3, "floor in the sun should be bright: {lit}");
        assert!(shade < lit * 0.85, "shadow of the cube should be darker: {shade} vs {lit}");
    }

    /// parametrii OpenPBR ai materialului folosit trebuie sa ajunga in scena de randare
    #[test]
    fn openpbr_params_reach_scene() {
        let p = |_: f32, _: &str| {};
        let raw = crate::load_any("samples/Block_A_Architecture_Structure.ifc", &p).expect("ifc");
        let o = raw.center();
        let mut app = crate::app::App::new(vec![]);
        app.models.push(Some(std::sync::Arc::new(crate::build_model(raw, "samples/model.ifc", o))));
        app.refresh_caches();
        let mut m = crate::materials::library()[0].clone();
        m.name = "OpenPBR test".into();
        m.coat = 70;
        m.sheen = 40;
        m.ior = 1.7;
        app.materials.push(m.clone());
        app.materials_on = true;
        app.assign_material(crate::materials::MatTarget::Items(app.all_ids()), "OpenPBR test");
        app.materials_changed();
        app.rebuild_colors();
        let sc = Scene::build(&app);
        assert!(!sc.extra.is_empty());
        assert!(sc.extra.iter().all(|e| (e[0] - 0.7).abs() < 1e-3 && (e[2] - 0.4).abs() < 1e-3 && (e[3] - 1.7).abs() < 1e-3), "{:?}", sc.extra[0]);
    }

    /// modurile de mediu (outdoor / studio / sunset) si intensitatea trebuie sa schimbe lumina IBL
    #[test]
    fn environment_modes_change_the_ibl() {
        let dir = Vec3::new(0.3, 0.4, 0.8).normalize();
        let sun = Vec3::new(0.6, 0.0, 0.8).normalize();
        let outdoor = super::env_cols((0, 1.0, 0.0), sun, dir, None);
        let studio = super::env_cols((1, 1.0, 0.0), sun, dir, None);
        let sunset = super::env_cols((2, 1.0, 0.0), sun, dir, None);
        let dark = super::env_cols((0, 0.0, 0.0), sun, dir, None);
        let dist = |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        assert!(outdoor.iter().all(|v| v.is_finite()));
        assert!(dist(outdoor, studio) > 0.05, "outdoor vs studio: {outdoor:?} {studio:?}");
        assert!(dist(studio, sunset) > 0.05, "studio vs sunset: {studio:?} {sunset:?}");
        assert!(dark.iter().all(|v| *v == 0.0));
        let a = super::env_cols((0, 1.0, 0.0), sun, dir, None);
        let b = super::env_cols((0, 1.0, 90.0), sun, dir, None);
        assert!((a[0] - b[0]).abs() > 0.01, "rotatia mediului nu are efect");
    }

    /// o lampa punctuala trebuie sa lumineze scena (si sa fie respectata raza / enable-ul)
    #[test]
    fn point_lights_affect_the_render() {
        let sc = scene_with(255, [0; 4], Vec::new());
        // camera deasupra, privind spre cub (la origine)
        let eye = Vec3::new(0.0, -12.0, 9.0);
        let fwd = (Vec3::new(0.0, 0.0, 1.5) - eye).normalize();
        let right = fwd.cross(Vec3::Z).normalize();
        let up = right.cross(fwd);
        let j0 = start_arc((48, 48, 12, 0.0), sc.clone(), eye, fwd, up, right, 50.0, Vec3::ZERO, 0.0, (1, 0.05, 0.0), false, None, false);
        while !j0.done.load(Ordering::Relaxed) { std::thread::sleep(std::time::Duration::from_millis(5)); }
        let dark = lum(&j0, 24, 24);
        let lamp = crate::lights::SceneLight { name: "L".into(), pos: [0.0, 0.0, 5.0], color: [255, 255, 255], power: 8.0, radius: 20.0, enabled: true, ..Default::default() };
        let sc2 = std::sync::Arc::new(Scene {
            tris: sc.tris.clone(), colors: sc.colors.clone(), mats: sc.mats.clone(), extra: sc.extra.clone(),
            textures: sc.textures.clone(), planes: sc.planes.clone(), lights: vec![lamp],
            bvh: crate::model::Bvh::build(&sc.tris),
        });
        let j1 = start_arc((48, 48, 12, 0.0), sc2, eye, fwd, up, right, 50.0, Vec3::ZERO, 0.0, (1, 0.05, 0.0), false, None, false);
        while !j1.done.load(Ordering::Relaxed) { std::thread::sleep(std::time::Duration::from_millis(5)); }
        assert!(lum(&j1, 24, 24) > dark + 0.05, "lampa nu lumineaza: {} vs {}", lum(&j1, 24, 24), dark);
    }

    #[test]
    fn spot_light_lights_inside_its_cone() {
        // spot deasupra cubului, indreptat in jos: lumineaza; indreptat in sus: nu (inainte nu lumina deloc)
        let sc = scene_with(255, [0; 4], Vec::new());
        let eye = Vec3::new(0.0, -12.0, 9.0);
        let fwd = (Vec3::new(0.0, 0.0, 1.5) - eye).normalize();
        let right = fwd.cross(Vec3::Z).normalize();
        let up = right.cross(fwd);
        let shot = |target_z: f32| {
            let lamp = crate::lights::SceneLight { name: "S".into(), kind: crate::lights::LightKind::Spot, pos: [0.0, 0.0, 6.0], target: [0.0, 0.0, target_z], angle: 70.0, color: [255, 255, 255], power: 8.0, radius: 0.0, enabled: true };
            let s = std::sync::Arc::new(Scene {
                tris: sc.tris.clone(), colors: sc.colors.clone(), mats: sc.mats.clone(), extra: sc.extra.clone(),
                textures: sc.textures.clone(), planes: sc.planes.clone(), lights: vec![lamp],
                bvh: crate::model::Bvh::build(&sc.tris),
            });
            let j = start_arc((48, 48, 12, 0.0), s, eye, fwd, up, right, 50.0, Vec3::ZERO, 0.0, (1, 0.05, 0.0), false, None, false);
            while !j.done.load(Ordering::Relaxed) { std::thread::sleep(std::time::Duration::from_millis(5)); }
            lum(&j, 24, 24)
        };
        let down = shot(0.0);
        let upward = shot(20.0);
        assert!(down > upward + 0.05, "spot in jos {down} vs in sus {upward}");
    }

    #[test]
    fn distant_light_shines_along_its_direction() {
        // lumina Distant de sus in jos lumineaza fata de sus a cubului; indreptata in sus, nu
        let sc = scene_with(255, [0; 4], Vec::new());
        let eye = Vec3::new(0.0, -12.0, 9.0);
        let fwd = (Vec3::new(0.0, 0.0, 1.5) - eye).normalize();
        let right = fwd.cross(Vec3::Z).normalize();
        let up = right.cross(fwd);
        let shot = |target_z: f32| {
            let lamp = crate::lights::SceneLight { name: "D".into(), kind: crate::lights::LightKind::Distant, pos: [0.0, 0.0, 10.0], target: [0.0, 0.0, target_z], color: [255, 255, 255], power: 3.0, radius: 0.0, enabled: true, ..Default::default() };
            let s = std::sync::Arc::new(Scene {
                tris: sc.tris.clone(), colors: sc.colors.clone(), mats: sc.mats.clone(), extra: sc.extra.clone(),
                textures: sc.textures.clone(), planes: sc.planes.clone(), lights: vec![lamp],
                bvh: crate::model::Bvh::build(&sc.tris),
            });
            let j = start_arc((48, 48, 12, 0.0), s, eye, fwd, up, right, 50.0, Vec3::ZERO, 0.0, (1, 0.05, 0.0), false, None, false);
            while !j.done.load(Ordering::Relaxed) { std::thread::sleep(std::time::Duration::from_millis(5)); }
            lum(&j, 24, 24)
        };
        let down = shot(0.0);
        let upward = shot(20.0);
        assert!(down > upward + 0.05, "Distant in jos {down} vs in sus {upward}");
    }

    #[test]
    fn aces_tonemap_is_bounded() {
        assert_eq!(super::aces(-1.0), 0.0);
        assert!(super::aces(0.0) < 0.02);
        let one = super::aces(1.0);
        assert!(one > 0.6 && one < 1.0, "aces(1) = {one}");
        let bright = super::aces(100.0);
        assert!(bright.is_finite() && (bright as f64) <= 1.0, "aces(100) = {bright}");
        let c = super::to_srgb([0.2, 0.5, 1.5]);
        assert_eq!(c.len(), 3);
        assert!(c[0] > 0 && c[2] > c[0]);
    }

    #[test]
    fn denoise_runs_when_oidn_is_installed() {
        if crate::oidn::available().is_err() {
            eprintln!("OIDN not installed - test skipped");
            return;
        }
        let job = render(true);
        assert_eq!(job.denoise_state.load(Ordering::Relaxed), DN_DONE, "{}", job.denoise_msg.lock().unwrap());
        let img = job.denoised.lock().unwrap().clone().expect("denoised image");
        assert_eq!(img.len(), 48 * 48 * 3);
        assert!(img.iter().all(|x| x.is_finite() && *x >= 0.0));
        // MB_PT_DUMP=folder: salveaza imaginea cu zgomot si cea curatata, pentru verificare vizuala
        if let Some(dir) = std::env::var_os("MB_PT_DUMP") {
            let dir = std::path::PathBuf::from(dir);
            let big = render_sized(true, 320, 4);
            let n = big.count.load(Ordering::Relaxed);
            save_png(&dir.join("pt_noisy.png").to_string_lossy(), &big.buf.lock().unwrap(), n, 320, 320).unwrap();
            save_png(&dir.join("pt_denoised.png").to_string_lossy(), big.denoised.lock().unwrap().as_ref().unwrap(), 1, 320, 320).unwrap();
        }
    }
}
