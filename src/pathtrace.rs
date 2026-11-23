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
    pub colors: Vec<[u8; 4]>,
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
        for (_, m) in app.loaded_models() {
            let n = m.tri_total();
            for t in 0..n {
                let i = t * 3;
                let (Some(a), Some(b), Some(c)) = (m.pos.get(i).copied(), m.pos.get(i + 1).copied(), m.pos.get(i + 2).copied()) else { continue };
                tris.push(a);
                tris.push(b);
                tris.push(c);
                let col = m.tri_color.get(t).copied().unwrap_or([200, 200, 200, 255]);
                colors.push(col);
            }
        }
        let bvh = Bvh::build(&tris);
        Scene { tris, colors, bvh }
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
    std::thread::spawn(move || run(j));
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

/// (culoare, albedo, normala) pentru raza o + t*d
fn trace(job: &PathTraceJob, o: Vec3, d: Vec3, rng: &mut Rng) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let bvh = &job.scene.bvh;
    let tris = &job.scene.tris;
    let Some((t, tri)) = bvh.raycast(tris, o, d, 1e6, &|_| true) else {
        let s = sky(d);
        return (s, s, [0.0; 3]);
    };
    let i = tri as usize * 3;
    let (a, b, c) = (tris[i], tris[i + 1], tris[i + 2]);
    let mut n = (b - a).cross(c - a).normalize_or_zero();
    // fata vazuta din spate: normala spre camera (altfel ar iesi neagra)
    if n.dot(d) > 0.0 {
        n = -n;
    }
    let base = job.scene.colors[tri as usize];
    let base = [base[0] as f32 / 255.0, base[1] as f32 / 255.0, base[2] as f32 / 255.0];
    let p = o + d * t + n * 1e-3;
    // soarele: o directie aleatoare pe disc -> umbre cu margini moi
    let sun_dir = job.sun.normalize_or_zero();
    let mut direct = 0.0;
    if sun_dir != Vec3::ZERO {
        let (st, sb) = basis(sun_dir);
        let (r, phi) = (SUN_RADIUS * rng.next().sqrt(), std::f32::consts::TAU * rng.next());
        let l = (sun_dir + st * (r * phi.cos()) + sb * (r * phi.sin())).normalize();
        let ndotl = n.dot(l).max(0.0);
        if ndotl > 0.0 {
            // 0.35 = penumbra (nu negru total), ca la umbre difuze reale
            let lit = if bvh.raycast(tris, p, l, 1e6, &|_| true).is_none() { 1.0 } else { 0.35 };
            direct = ndotl * job.light * lit;
        }
    }
    // lumina cerului: o raza pe emisfera (cosinus); un obstacol apropiat = ambient occlusion
    let (nt, nb) = basis(n);
    let (r1, r2) = (rng.next(), rng.next());
    let (r, phi) = (r1.sqrt(), std::f32::consts::TAU * r2);
    let hd = (nt * (r * phi.cos()) + nb * (r * phi.sin()) + n * (1.0 - r1).max(0.0).sqrt()).normalize();
    let open = if bvh.raycast(tris, p, hd, AO_DIST, &|_| true).is_none() { 1.0 } else { 0.25 };
    // fetele spre cer primesc mai mult (cer), cele spre sol doar reflexia solului
    let up = n.z.max(0.0);
    let ambient = (0.10 + 0.20 * (0.6 + 0.4 * up)) * open;
    let k = ambient + direct;
    ([base[0] * k, base[1] * k, base[2] * k], base, n.to_array())
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
        let bvh = Bvh::build(&tris);
        Arc::new(Scene { tris, colors, bvh })
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
