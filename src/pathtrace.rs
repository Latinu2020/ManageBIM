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

pub struct Scene {
    pub tris: Vec<Vec3>,   // 3 vertexuri per triunghi
    /// culoarea finala (material / override / profil, ca in vederea 3D), alfa = opacitatea
    pub colors: Vec<[u8; 4]>,
    /// parametrii materialului per triunghi, ca in shader: [model sau 16 + textura, luciu, metal, scara]
    pub mats: Vec<[u8; 4]>,
    /// texturile imagine (512 x 512 RGBA), in ordinea straturilor de pe GPU
    pub textures: Vec<Vec<u8>>,
    /// planurile sectiunii active (n, w): se pastreaza punctele cu n·p <= w
    pub planes: Vec<glam::Vec4>,
    pub bvh: Bvh,
}

impl Scene {
    pub fn build(app: &crate::app::App) -> Self {
        // Camera si geometria trebuie sa fie in ACELASI spatiu de coordonate.
        // Renderer-ul OpenGL foloseste vertecsi locali (m.pos) si cam.eye() local,
        // fara origin (origin e doar pentru export / coordonate reale). Daca adaugam
        // origin aici, razele nu mai lovesc modelul si imaginea ramane doar "cer".
        let mut tris = Vec::new();
        let mut colors = Vec::new();
        let mut mats = Vec::new();
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
                }
            }
        }
        let textures = app.tex_layers.iter().map(|t| t.1.clone()).collect();
        let planes = app.section.gpu_planes();
        let bvh = Bvh::build(&tris);
        Scene { tris, colors, mats, textures, planes, bvh }
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

    /// culoarea de baza in punctul p: textura imagine sau model procedural (aceleasi formule ca shader-ul)
    fn base_color(&self, tri: usize, p: Vec3, n: Vec3) -> [f32; 3] {
        let c = self.colors[tri];
        let base = [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0];
        let m = self.mats[tri];
        let code = m[0];
        if code == 0 {
            return base;
        }
        let sc = 0.05 + m[3] as f32 / 255.0 * 4.0;
        let an = n.abs();
        let uv = if an.z >= an.x && an.z >= an.y { glam::Vec2::new(p.x, p.y) } else if an.x >= an.y { glam::Vec2::new(p.y, p.z) } else { glam::Vec2::new(p.x, p.z) } / sc;
        if code >= 16 {
            let Some(tex) = self.textures.get((code - 16) as usize) else { return base };
            let s = crate::materials::TEX_SIZE as usize;
            let (x, y) = ((uv.x.rem_euclid(1.0) * s as f32) as usize % s, (uv.y.rem_euclid(1.0) * s as f32) as usize % s);
            let i = (y * s + x) * 4;
            return match tex.get(i..i + 3) {
                Some(px) => [px[0] as f32 / 255.0, px[1] as f32 / 255.0, px[2] as f32 / 255.0],
                None => base,
            };
        }
        pattern(code, base, uv)
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
pub fn start_arc(params: (u32, u32, u32, f32), scene: Arc<Scene>, cam_eye: Vec3, cam_fwd: Vec3, cam_up: Vec3, cam_right: Vec3, fov_deg: f32, sun: Vec3, light: f32, denoise: bool) -> Arc<PathTraceJob> {
    let (w, h, samples, tlimit) = params;
    let job = Arc::new(PathTraceJob {
        width: w, height: h, samples_target: samples, time_limit_secs: tlimit,
        cancel: AtomicBool::new(false),
        buf: Mutex::new(vec![0.0; (w * h * 3) as usize]),
        count: AtomicU32::new(0),
        done: AtomicBool::new(false),
        started: Instant::now(),
        scene, cam_eye, cam_fwd, cam_up, cam_right, fov_deg, sun, light,
        aux: Mutex::new(vec![0.0; (w * h * 6) as usize]),
        denoise,
        denoise_state: AtomicU8::new(DN_OFF),
        denoised: Mutex::new(None),
        denoise_msg: Mutex::new(String::new()),
    });
    let j = job.clone();
    std::thread::spawn(move || {
        // o panica in randare nu lasa job-ul „in curs” pentru totdeauna (altfel nu mai poti porni alta randare)
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
    if job.denoise && job.count.load(Ordering::Relaxed) > 0 {
        job.denoise_state.store(DN_RUNNING, Ordering::Relaxed);
        let r = denoise_job(&job);
        match r {
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

/// culoarea cerului pe directia d (Z = sus, ca modelele)
fn sky(d: Vec3) -> [f32; 3] {
    let t = (d.z * 0.5 + 0.5).clamp(0.0, 1.0);
    [0.55 + 0.35 * t, 0.65 + 0.25 * t, 0.85 + 0.15 * t]
}

/// cate reflexii / treceri prin sticla urmarim (adancimea razei)
const MAX_DEPTH: u32 = 3;

/// (culoare, albedo, normala) pentru raza o + t*d
fn trace(job: &PathTraceJob, o: Vec3, d: Vec3, rng: &mut Rng) -> ([f32; 3], [f32; 3], [f32; 3]) {
    trace_d(job, o, d, rng, 0)
}

fn trace_d(job: &PathTraceJob, o: Vec3, d: Vec3, rng: &mut Rng, depth: u32) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let sc = &job.scene;
    let tris = &sc.tris;
    let Some((t, tri)) = sc.hit(o, d, 1e6) else {
        let s = sky(d);
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
    let base = sc.base_color(ti, hit, n);
    let alpha = sc.colors[ti][3] as f32 / 255.0;
    let m = sc.mats[ti];
    let (gloss, metal) = (m[1] as f32 / 255.0, m[2] as f32 / 255.0);
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
    let mut direct = 0.0;
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
            direct = ndotl * job.light * (0.35 + 0.65 * tr);
        }
    }
    // lumina cerului: o raza pe emisfera (cosinus); un obstacol apropiat = ambient occlusion
    let (nt, nb) = basis(n);
    let (r1, r2) = (rng.next(), rng.next());
    let (r, phi) = (r1.sqrt(), std::f32::consts::TAU * r2);
    let hd = (nt * (r * phi.cos()) + nb * (r * phi.sin()) + n * (1.0 - r1).max(0.0).sqrt()).normalize();
    let open = 0.25 + 0.75 * sc.transmit(p, hd, AO_DIST);
    // fetele spre cer primesc mai mult (cer), cele spre sol doar reflexia solului
    let up = n.z.max(0.0);
    let ambient = (0.10 + 0.20 * (0.6 + 0.4 * up)) * open;
    let k = ambient + direct;
    let mut col = [base[0] * k * (1.0 - 0.35 * metal), base[1] * k * (1.0 - 0.35 * metal), base[2] * k * (1.0 - 0.35 * metal)];
    // luciu: stralucirea soarelui (Blinn-Phong, ca in vederea 3D) + reflexia reala a scenei (Fresnel)
    let vd = -d;
    let head = n.dot(vd).abs();
    if gloss > 0.02 || metal > 0.02 {
        let g = gloss.max(0.07);
        let h = (sun_dir + vd).normalize_or_zero();
        let spec = n.dot(h).max(0.0).powf(12.0 + (160.0 - 12.0) * g) * g * lit_sun * job.light;
        let tint = |k: usize| 1.0 + (base[k] - 1.0) * metal;
        for (k, v) in col.iter_mut().enumerate() {
            *v += tint(k) * spec * 0.85;
        }
        if depth < MAX_DEPTH {
            let f0 = |k: usize| 0.04 + (base[k] - 0.04) * metal;
            let fres = |k: usize| f0(k) + (1.0 - f0(k)) * (1.0 - head).powi(5);
            let w = (gloss * 0.6).max(metal * 0.8);
            // reflexie lucioasa, putin imprastiata pentru materialele mai mate
            let rdir = d - n * 2.0 * d.dot(n);
            let fuzz = (1.0 - gloss) * 0.25;
            let rdir = (rdir + (nt * (rng.next() - 0.5) + nb * (rng.next() - 0.5)) * fuzz).normalize();
            let (rc, _, _) = trace_d(job, p, rdir, rng, depth + 1);
            for (k, v) in col.iter_mut().enumerate() {
                let f = (fres(k) * w).clamp(0.0, 1.0);
                *v = *v * (1.0 - f) + rc[k] * f;
            }
        }
    }
    (col, base, n.to_array())
}

pub fn save_png(path: &str, buf: &[f32], count: u32, w: u32, h: u32) -> Result<(), String> {
    let mut img = Vec::with_capacity((w * h * 3) as usize);
    let n = count.max(1) as f32;
    for p in buf.chunks_exact(3) {
        let r = (p[0] / n).clamp(0.0, 1.0);
        let g = (p[1] / n).clamp(0.0, 1.0);
        let b = (p[2] / n).clamp(0.0, 1.0);
        // gamma 2.2
        let to8 = |x: f32| (x.powf(1.0 / 2.2) * 255.0 + 0.5) as u8;
        img.push(to8(r));
        img.push(to8(g));
        img.push(to8(b));
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
        Arc::new(Scene { tris, colors, mats, textures: Vec::new(), planes: Vec::new(), bvh })
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
        Arc::new(Scene { tris: s.tris.clone(), colors, mats, textures: Vec::new(), planes, bvh: Bvh::build(&s.tris) })
    }

    fn run_scene(sc: Arc<Scene>) -> Arc<PathTraceJob> {
        let eye = Vec3::new(0.0, -12.0, 9.0);
        let fwd = (Vec3::new(0.0, 0.0, 0.5) - eye).normalize();
        let right = fwd.cross(Vec3::Z).normalize();
        let up = right.cross(fwd);
        let job = start_arc((48, 48, 16, 0.0), sc, eye, fwd, up, right, 50.0, Vec3::new(0.6, 0.0, 0.8), 1.0, false);
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
        let plain = run_scene(scene_with(255, [0; 4], Vec::new()));
        let brick = run_scene(scene_with(255, [1, 0, 0, 10], Vec::new()));
        let spread = |j: &PathTraceJob| {
            let v: Vec<f32> = (30..46).flat_map(|x| (36..46).map(move |y| (x, y))).map(|(x, y)| lum(j, x, y)).collect();
            let m = v.iter().sum::<f32>() / v.len() as f32;
            v.iter().map(|x| (x - m).abs()).sum::<f32>() / v.len() as f32
        };
        assert!(spread(&brick) > spread(&plain) * 2.0, "brick {} vs plain {}", spread(&brick), spread(&plain));
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
        let job = start_arc((size, size, samples, 0.0), scene(), eye, fwd, up, right, 50.0, Vec3::new(0.6, 0.0, 0.8), 1.0, denoise);
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
