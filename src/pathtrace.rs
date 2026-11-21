//! Randare progresiva (CPU) pentru o imagine statica, cu anulare si export PNG.
//! Implementare de baza: Lambert + lumina directa (soare) + ambianta, pe BVH global.

use crate::model::Bvh;
use glam::Vec3;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct Scene {
    pub tris: Vec<Vec3>,   // 3 vertexuri per triunghi
    pub colors: Vec<[u8; 4]>,
    pub bvh: Bvh,
}

impl Scene {
    pub fn build(app: &crate::app::App) -> Self {
        let origin = app.origin_f64();
        let mut tris = Vec::new();
        let mut colors = Vec::new();
        for (_, m) in app.loaded_models() {
            let n = m.tri_total();
            for t in 0..n {
                let i = t * 3;
                let (Some(a), Some(b), Some(c)) = (m.pos.get(i).copied(), m.pos.get(i + 1).copied(), m.pos.get(i + 2).copied()) else { continue };
                tris.push(a + origin.as_vec3());
                tris.push(b + origin.as_vec3());
                tris.push(c + origin.as_vec3());
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
}

// We'll use Arc<PathTraceJob> everywhere instead.
pub fn start_arc(params: (u32, u32, u32, f32), scene: Arc<Scene>, cam_eye: Vec3, cam_fwd: Vec3, cam_up: Vec3, cam_right: Vec3, fov_deg: f32, sun: Vec3, light: f32) -> Arc<PathTraceJob> {
    let (w, h, samples, tlimit) = params;
    let job = Arc::new(PathTraceJob {
        width: w, height: h, samples_target: samples, time_limit_secs: tlimit,
        cancel: AtomicBool::new(false),
        buf: Mutex::new(vec![0.0; (w * h * 3) as usize]),
        count: AtomicU32::new(0),
        done: AtomicBool::new(false),
        started: Instant::now(),
        scene, cam_eye, cam_fwd, cam_up, cam_right, fov_deg, sun, light,
    });
    let j = job.clone();
    std::thread::spawn(move || run(j));
    job
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
        let mut buf = job.buf.lock().unwrap();
        for py in 0..job.height {
            for px in 0..job.width {
                // stratified jitter
                let jx = (s as f32 * 0.7548777).fract();
                let jy = (s as f32 * 0.5698403).fract();
                let u = (px as f32 + jx) / w;
                let v = (py as f32 + jy) / h;
                let ndc_x = u * 2.0 - 1.0;
                let ndc_y = 1.0 - v * 2.0;
                let dir = (job.cam_fwd + job.cam_right * (ndc_x * half_w) + job.cam_up * (ndc_y * half_h)).normalize();
                let col = trace(&job, job.cam_eye, dir);
                let idx = ((py * job.width + px) * 3) as usize;
                buf[idx] += col[0];
                buf[idx + 1] += col[1];
                buf[idx + 2] += col[2];
            }
        }
        job.count.fetch_add(1, Ordering::Relaxed);
        s += 1;
    }
    job.done.store(true, Ordering::Relaxed);
}

fn trace(job: &PathTraceJob, o: Vec3, d: Vec3) -> [f32; 3] {
    if let Some((t, tri)) = job.scene.bvh.raycast(&job.scene.tris, o, d, 1e6, &|_| true) {
        let i = tri as usize * 3;
        let a = job.scene.tris[i];
        let b = job.scene.tris[i + 1];
        let c = job.scene.tris[i + 2];
        let n = (b - a).cross(c - a).normalize();
        let base = job.scene.colors[tri as usize];
        let base = [base[0] as f32 / 255.0, base[1] as f32 / 255.0, base[2] as f32 / 255.0];
        let p = o + d * t;
        // shadow ray to sun
        let sun_dir = job.sun.normalize();
        let shadow = if sun_dir.length() > 0.0 {
            if job.scene.bvh.raycast(&job.scene.tris, p + n * 1e-3, sun_dir, 1e6, &|_| true).is_none() { 1.0 } else { 0.2 }
        } else { 1.0 };
        let ndotl = n.dot(sun_dir).max(0.0);
        let ambient = 0.18;
        let diffuse = ndotl * job.light * shadow;
        [base[0] * (ambient + diffuse), base[1] * (ambient + diffuse), base[2] * (ambient + diffuse)]
    } else {
        // sky
        let t = (d.y * 0.5 + 0.5).clamp(0.0, 1.0);
        [0.55 + 0.35 * t, 0.65 + 0.25 * t, 0.85 + 0.15 * t]
    }
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
