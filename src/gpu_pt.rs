//! GPU path tracer: the same light model as the CPU path tracer (`pathtrace.rs`) - soft sun shadows,
//! sky / HDRI light with ambient occlusion, lamps and sun with up to four diffuse bounces, GGX reflections, clearcoat,
//! sheen, glass, PBR textures and section planes - running in an OpenGL 3.3 fragment shader.
//!
//! The scene (triangles in BVH order + BVH nodes) lives in two RGBA32F 2D textures read with
//! `texelFetch`, so no compute shaders or texture buffers are needed. Every frame adds one sample per
//! pixel to float accumulation targets (colour, albedo, normal) with additive blending; the display pass
//! divides by the sample count and applies the ACES tone map, exactly like the CPU render.
//! Used for the interactive Ray Trace view (progressive, like Navisworks) and for final renders.
use glam::{Vec3, Vec4};
use eframe::glow::{self, HasContext};
use std::sync::{Arc, Mutex};

/// width of the data textures (texels)
const TW: usize = 4096;
/// reflections / glass passes followed (as MAX_DEPTH in pathtrace.rs)
const MAX_DEPTH: u32 = 4;

/// The scene packed for the GPU.
pub struct PtScene {
    /// 4 texels per triangle, in BVH order: (v0, colour bits), (v1, material bits), (v2, 0), (coat, coat roughness, sheen, IOR)
    pub tris: Vec<f32>,
    /// 2 texels per BVH node: (min, a), (max, b | leaf << 31)
    pub nodes: Vec<f32>,
    pub n_tris: usize,
    pub planes: Vec<Vec4>,
    pub lights: Vec<crate::lights::SceneLight>,
    /// changes whenever the scene is rebuilt (restarts the accumulation)
    pub id: u64,
}

impl PtScene {
    /// Packs a CPU scene (with its BVH already built).
    pub fn pack(sc: &crate::pathtrace::Scene, id: u64) -> PtScene {
        let order = &sc.bvh.order;
        let mut tris = Vec::with_capacity(order.len() * 16);
        let bits = |c: [u8; 4]| f32::from_bits(c[0] as u32 | (c[1] as u32) << 8 | (c[2] as u32) << 16 | (c[3] as u32) << 24);
        for &t in order {
            let t = t as usize;
            let (a, b, c) = (sc.tris[t * 3], sc.tris[t * 3 + 1], sc.tris[t * 3 + 2]);
            let ex = sc.extra.get(t).copied().unwrap_or([0.0, 0.05, 0.0, crate::materials::default_ior()]);
            tris.extend_from_slice(&[a.x, a.y, a.z, bits(sc.colors[t]), b.x, b.y, b.z, bits(sc.mats[t]), c.x, c.y, c.z, 0.0]);
            tris.extend_from_slice(&ex);
        }
        let mut nodes = Vec::with_capacity(sc.bvh.nodes.len() * 8);
        for n in &sc.bvh.nodes {
            let b = if n.leaf { n.b | 0x8000_0000 } else { n.b };
            nodes.extend_from_slice(&[n.bb.min.x, n.bb.min.y, n.bb.min.z, f32::from_bits(n.a), n.bb.max.x, n.bb.max.y, n.bb.max.z, f32::from_bits(b)]);
        }
        PtScene { n_tris: order.len(), tris, nodes, planes: sc.planes.clone(), lights: sc.lights.clone(), id }
    }
}

/// Most samples the interactive view adds in one frame. The 25 ms time budget is the real limit: a slow
/// graphics card draws less than one sample per frame, a fast one fills the budget with up to this many
/// samples (0.29.0 stopped at 8, which left fast cards idle most of each frame).
const LIVE_MAX_PASSES: u32 = 64;

/// One frame of path tracing: camera, sun, sky and the scene.
#[derive(Clone)]
pub struct PtFrame {
    pub scene: Arc<PtScene>,
    pub eye: Vec3,
    pub fwd: Vec3,
    pub up: Vec3,
    pub right: Vec3,
    pub fov_deg: f32,
    pub sun: Vec3,
    /// sun power x brightness
    pub light: f32,
    /// sky: (mode, intensity, rotation in degrees)
    pub env: (u8, f32, f32),
    pub night: bool,
    pub hdri: Option<Arc<crate::hdri::Hdri>>,
    /// samples per pixel to reach (the view stops refining there)
    pub max_samples: u32,
    /// the camera is moving: half resolution for a fluid preview
    pub moving: bool,
    /// clean the converged image with Intel Open Image Denoise
    pub denoise: bool,
    /// how many times the sun and lamp light bounce between surfaces (1 - 4)
    pub bounces: u32,
    /// camera and lighting: a different key restarts the accumulation
    pub key: u64,
}

/// What the interactive view shows now (read by the app for the overlay and repaints).
#[derive(Clone, Copy, Default, Debug)]
pub struct PtStatus {
    pub samples: u32,
    pub max: u32,
    pub w: i32,
    pub h: i32,
    pub denoising: bool,
    pub denoised: bool,
}

/// A sample is drawn in horizontal bands within a time budget. Until the whole sample is finished, the
/// rows already drawn hold one *extra* contribution: dividing them by `samples` (the finished count) makes
/// them too bright - a moving bright stripe that looks like a blotch and never settles on a slow card.
/// `show()` uses one scale for those rows and another for the rest.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ShowPasses {
    /// accumulation rows `[0, partial_rows)` contain `samples + 1` contributions (counted from the bottom)
    partial_rows: i32,
    total_rows: i32,
    /// scale for rows with only the finished samples
    complete: f32,
    /// scale for rows that also contain the band in progress
    partial: f32,
}

fn show_passes(samples: u32, band_y: i32, h: i32) -> ShowPasses {
    let total_rows = h.max(1);
    ShowPasses {
        partial_rows: band_y.clamp(0, total_rows),
        total_rows,
        complete: 1.0 / samples.max(1) as f32,
        partial: 1.0 / (samples + 1) as f32,
    }
}

/// Quick edge filter: full strength on the first sample, gone at 48 samples.
fn filter_strength(samples: u32) -> f32 {
    let n = samples.max(1) as f32;
    (1.0 - (n - 1.0) / 47.0).clamp(0.0, 1.0)
}

/// The raster view is kept only until the first complete sample: before that the accumulation
/// targets are still empty and would show as a black band. Afterwards the image is shown on every
/// frame, including while the next sample is being drawn in bands (show() uses the right divisor
/// for those rows). Requiring `band_y == 0` made the view flicker back to raster between samples.
fn ready_to_display(samples: u32, _band_y: i32) -> bool {
    samples >= 1
}

type Denoised = Arc<Mutex<Option<(u64, Vec<f32>)>>>;

#[derive(Default)]
pub struct GpuPt {
    prog: Option<glow::Program>,
    disp: Option<glow::Program>,
    vao: Option<glow::VertexArray>,
    pub error: Option<String>,
    scene_id: u64,
    tri_tex: Option<glow::Texture>,
    node_tex: Option<glow::Texture>,
    n_tris: usize,
    hdri_key: usize,
    hdri_tex: Option<glow::Texture>,
    /// accumulation: framebuffer + colour / albedo / normal, size
    fbo: Option<(glow::Framebuffer, [glow::Texture; 3], i32, i32)>,
    key: u64,
    pub samples: u32,
    /// a sample is drawn in horizontal bands (a few per frame within a time budget): the next band's first row
    band_y: i32,
    /// rows per band, adapted so one band takes a few milliseconds on this graphics card
    band_h: i32,
    den_tex: Option<(glow::Texture, u64)>,
    den_started: u64,
    den_result: Denoised,
    pub status: PtStatus,
}

fn as_bytes(v: &[f32]) -> &[u8] {
    // SAFETY: f32 has no padding; the byte view covers exactly the same memory
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

fn as_bytes_mut(v: &mut [f32]) -> &mut [u8] {
    // SAFETY: as above; every byte pattern is a valid f32
    unsafe { std::slice::from_raw_parts_mut(v.as_mut_ptr() as *mut u8, std::mem::size_of_val(v)) }
}

unsafe fn data_texture(gl: &glow::Context, data: &[f32]) -> Result<glow::Texture, String> {
    let texels = (data.len() / 4).max(1);
    let rows = texels.div_ceil(TW);
    let mut buf = vec![0f32; rows * TW * 4];
    buf[..data.len()].copy_from_slice(data);
    let t = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(t));
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
    gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA32F as i32, TW as i32, rows as i32, 0, glow::RGBA, glow::FLOAT, Some(as_bytes(&buf)));
    gl.bind_texture(glow::TEXTURE_2D, None);
    Ok(t)
}

impl GpuPt {
    unsafe fn init(&mut self, gl: &glow::Context) -> bool {
        if self.prog.is_some() {
            return true;
        }
        if self.error.is_some() {
            return false;
        }
        let v = gl.version();
        if v.is_embedded || (v.major, v.minor) < (3, 3) {
            self.error = Some("GPU path tracing needs OpenGL 3.3".into());
            return false;
        }
        let header = "#version 330 core\n";
        let fs = FS_PT.replace("$PBR_CODE", &crate::materials::PBR_CODE.to_string()).replace("$MAX_DEPTH", &MAX_DEPTH.to_string()).replace("$TW", &TW.to_string());
        match (crate::render::compile_program(gl, header, VS_FULL, &fs), crate::render::compile_program(gl, header, VS_FULL, FS_SHOW)) {
            (Ok(p), Ok(d)) => {
                self.prog = Some(p);
                self.disp = Some(d);
                self.vao = gl.create_vertex_array().ok();
                true
            }
            (Err(e), _) | (_, Err(e)) => {
                self.error = Some(format!("GPU path tracing shader: {e}"));
                false
            }
        }
    }

    unsafe fn upload(&mut self, gl: &glow::Context, f: &PtFrame) -> Result<(), String> {
        if self.scene_id != f.scene.id || self.tri_tex.is_none() {
            for t in [self.tri_tex.take(), self.node_tex.take()].into_iter().flatten() {
                gl.delete_texture(t);
            }
            self.tri_tex = Some(data_texture(gl, &f.scene.tris)?);
            self.node_tex = Some(data_texture(gl, &f.scene.nodes)?);
            self.n_tris = f.scene.n_tris;
            self.scene_id = f.scene.id;
        }
        let hk = f.hdri.as_ref().map_or(0, |h| Arc::as_ptr(h) as usize);
        if hk != self.hdri_key {
            if let Some(t) = self.hdri_tex.take() {
                gl.delete_texture(t);
            }
            if let Some(h) = &f.hdri {
                let t = gl.create_texture()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(t));
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGB32F as i32, h.width as i32, h.height as i32, 0, glow::RGB, glow::FLOAT, Some(as_bytes(&h.pixels)));
                gl.bind_texture(glow::TEXTURE_2D, None);
                self.hdri_tex = Some(t);
            }
            self.hdri_key = hk;
        }
        Ok(())
    }

    /// (Re)creates the accumulation targets at w x h; true if they were recreated (accumulation restarts).
    unsafe fn targets(&mut self, gl: &glow::Context, w: i32, h: i32) -> Result<bool, String> {
        if let Some((_, _, fw, fh)) = self.fbo {
            if fw == w && fh == h {
                return Ok(false);
            }
        }
        if let Some((fb, ts, _, _)) = self.fbo.take() {
            gl.delete_framebuffer(fb);
            for t in ts {
                gl.delete_texture(t);
            }
        }
        let fb = gl.create_framebuffer()?;
        let mut ts = Vec::new();
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
        for i in 0..3u32 {
            let t = match gl.create_texture() {
                Ok(t) => t,
                Err(e) => {
                    for t in &ts { gl.delete_texture(*t); }
                    gl.delete_framebuffer(fb);
                    return Err(e);
                }
            };
            gl.bind_texture(glow::TEXTURE_2D, Some(t));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA32F as i32, w, h, 0, glow::RGBA, glow::FLOAT, None);
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0 + i, glow::TEXTURE_2D, Some(t), 0);
            ts.push(t);
        }
        gl.bind_texture(glow::TEXTURE_2D, None);
        let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
        if status != glow::FRAMEBUFFER_COMPLETE {
            // never cache an incomplete target: the next frame would keep drawing into it
            for t in &ts { gl.delete_texture(*t); }
            gl.delete_framebuffer(fb);
            return Err(format!("GPU path tracing: float render targets not available (0x{status:x})"));
        }
        self.fbo = Some((fb, [ts[0], ts[1], ts[2]], w, h));
        Ok(true)
    }

    unsafe fn clear(&mut self, gl: &glow::Context) {
        if let Some((fb, ..)) = self.fbo {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
            // glClearBuffer respects the scissor rectangle egui leaves on: without this only part of the
            // targets is zeroed and the rest keeps old memory (blocks of wrong colour in the image)
            gl.disable(glow::SCISSOR_TEST);
            gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1, glow::COLOR_ATTACHMENT2]);
            for i in 0..3 {
                gl.clear_buffer_f32_slice(glow::COLOR, i, &[0.0; 4]);
            }
        }
        self.samples = 0;
        self.band_y = 0;
    }

    /// Adds up to `n` samples per pixel to the accumulation (w x h). The caller restores its framebuffer.
    /// Each sample is drawn in horizontal bands; with `budget_ms`, drawing stops as soon as that time is used
    /// (the rest of the sample continues on the next call), so a big model never blocks the interface or hits
    /// the Windows GPU timeout. Returns false when the GPU path tracer is not available (then `error` says why).
    ///
    /// # Safety
    /// Call with the OpenGL context current (inside the egui_glow paint callback).
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn accumulate(&mut self, gl: &glow::Context, f: &PtFrame, w: i32, h: i32, n: u32, tex_array: Option<glow::Texture>, budget_ms: Option<f32>) -> bool {
        if !self.init(gl) {
            return false;
        }
        if let Err(e) = self.upload(gl, f) {
            self.error = Some(e);
            return false;
        }
        let key = f.key ^ f.scene.id.rotate_left(17) ^ ((w as u64) << 40 | (h as u64) << 20);
        match self.targets(gl, w, h) {
            Err(e) => {
                self.error = Some(e);
                return false;
            }
            Ok(new) => {
                if new || key != self.key {
                    self.key = key;
                    self.clear(gl);
                }
            }
        }
        let (Some(prog), Some((fb, ..))) = (self.prog, self.fbo) else { return false };
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
        gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1, glow::COLOR_ATTACHMENT2]);
        gl.viewport(0, 0, w, h);
        gl.disable(glow::SCISSOR_TEST);
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::CULL_FACE);
        gl.enable(glow::BLEND);
        gl.blend_equation(glow::FUNC_ADD);
        gl.blend_func(glow::ONE, glow::ONE);
        gl.use_program(Some(prog));
        let u = |name: &str| gl.get_uniform_location(prog, name);
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, self.tri_tex);
        gl.uniform_1_i32(u("u_tris").as_ref(), 0);
        gl.active_texture(glow::TEXTURE0 + 1);
        gl.bind_texture(glow::TEXTURE_2D, self.node_tex);
        gl.uniform_1_i32(u("u_nodes").as_ref(), 1);
        gl.active_texture(glow::TEXTURE0 + 2);
        gl.bind_texture(glow::TEXTURE_2D_ARRAY, tex_array);
        gl.uniform_1_i32(u("u_tex").as_ref(), 2);
        gl.uniform_1_i32(u("u_has_tex").as_ref(), tex_array.is_some() as i32);
        gl.active_texture(glow::TEXTURE0 + 3);
        gl.bind_texture(glow::TEXTURE_2D, self.hdri_tex);
        gl.uniform_1_i32(u("u_hdri").as_ref(), 3);
        gl.uniform_1_i32(u("u_has_hdri").as_ref(), self.hdri_tex.is_some() as i32);
        gl.uniform_1_i32(u("u_ntris").as_ref(), self.n_tris as i32);
        let aspect = w as f32 / h.max(1) as f32;
        let half_h = (f.fov_deg.to_radians() * 0.5).tan();
        gl.uniform_3_f32(u("u_eye").as_ref(), f.eye.x, f.eye.y, f.eye.z);
        gl.uniform_3_f32(u("u_fwd").as_ref(), f.fwd.x, f.fwd.y, f.fwd.z);
        gl.uniform_3_f32(u("u_up").as_ref(), f.up.x, f.up.y, f.up.z);
        gl.uniform_3_f32(u("u_right").as_ref(), f.right.x, f.right.y, f.right.z);
        gl.uniform_2_f32(u("u_half").as_ref(), half_h * aspect, half_h);
        gl.uniform_2_f32(u("u_res").as_ref(), w as f32, h as f32);
        let sun = f.sun.normalize_or_zero();
        gl.uniform_3_f32(u("u_sun").as_ref(), sun.x, sun.y, sun.z);
        gl.uniform_1_f32(u("u_light").as_ref(), f.light);
        gl.uniform_3_f32(u("u_env").as_ref(), f.env.0 as f32, f.env.1, f.env.2);
        gl.uniform_1_i32(u("u_night").as_ref(), f.night as i32);
        let mut pl = [0f32; 24];
        for (i, p) in f.scene.planes.iter().take(6).enumerate() {
            pl[i * 4..i * 4 + 4].copy_from_slice(&p.to_array());
        }
        gl.uniform_4_f32_slice(u("u_planes").as_ref(), &pl);
        gl.uniform_1_i32(u("u_nplanes").as_ref(), f.scene.planes.len().min(6) as i32);
        let mut lamps = [0f32; 128];
        let mut nl = 0;
        for l in f.scene.lights.iter().take(8) {
            let c = l.linear();
            let kind = match l.kind {
                crate::lights::LightKind::Point => 0.0,
                crate::lights::LightKind::Spot => 1.0,
                crate::lights::LightKind::Distant => 2.0,
            };
            lamps[nl * 16..nl * 16 + 16].copy_from_slice(&[l.pos[0], l.pos[1], l.pos[2], kind, l.target[0], l.target[1], l.target[2], l.power, c[0], c[1], c[2], l.radius, l.angle, 0.0, 0.0, 0.0]);
            nl += 1;
        }
        gl.uniform_4_f32_slice(u("u_lamp").as_ref(), &lamps);
        gl.uniform_1_i32(u("u_nlamps").as_ref(), nl as i32);
        gl.uniform_1_i32(u("u_bounces").as_ref(), f.bounces.clamp(1, crate::pathtrace::MAX_BOUNCES) as i32);
        gl.bind_vertex_array(self.vao);
        let lf = u("u_frame");
        if self.band_h <= 0 {
            self.band_h = 64;
        }
        let t0 = std::time::Instant::now();
        let mut passes = 0;
        gl.enable(glow::SCISSOR_TEST);
        while passes < n {
            // the first sample overwrites every pixel (no blending), the next ones add up: the targets never
            // need a clear that could miss pixels (stale memory showed as coloured blocks)
            if self.samples == 0 {
                gl.disable(glow::BLEND);
            } else {
                gl.enable(glow::BLEND);
            }
            let y1 = (self.band_y + self.band_h).min(h);
            gl.scissor(0, self.band_y, w, y1 - self.band_y);
            gl.uniform_1_u32(lf.as_ref(), self.samples.wrapping_mul(2_654_435_761).wrapping_add(self.samples));
            let tb = std::time::Instant::now();
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            if budget_ms.is_some() {
                // wait for the band: its time sizes the next bands (about 6 ms each)
                gl.finish();
                let ms = tb.elapsed().as_secs_f32() * 1000.0;
                let rows = (y1 - self.band_y).max(1) as f32;
                self.band_h = ((rows * 6.0 / ms.max(0.05)) as i32).clamp(8, h.max(8));
            }
            self.band_y = y1;
            if self.band_y >= h {
                self.band_y = 0;
                self.samples += 1;
                passes += 1;
            }
            if budget_ms.is_some_and(|b| t0.elapsed().as_secs_f32() * 1000.0 >= b) {
                break;
            }
        }
        gl.disable(glow::SCISSOR_TEST);
        gl.bind_vertex_array(None);
        gl.disable(glow::BLEND);
        for i in 0..4 {
            gl.active_texture(glow::TEXTURE0 + i);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
        gl.active_texture(glow::TEXTURE0 + 2);
        gl.bind_texture(glow::TEXTURE_2D_ARRAY, None);
        gl.active_texture(glow::TEXTURE0);
        true
    }

    /// Draws the accumulated image (or its denoised version) into the current framebuffer / viewport.
    ///
    /// # Safety
    /// Call with the OpenGL context current (inside the egui_glow paint callback).
    pub unsafe fn show(&self, gl: &glow::Context, vp: [i32; 4], acc_h: i32) {
        let (Some(disp), Some((_, ts, ..))) = (self.disp, self.fbo) else { return };
        gl.use_program(Some(disp));
        gl.disable(glow::BLEND);
        gl.disable(glow::DEPTH_TEST);
        gl.active_texture(glow::TEXTURE0);
        let den = self.den_tex.filter(|d| d.1 == self.key);
        gl.bind_texture(glow::TEXTURE_2D, Some(den.map_or(ts[0], |d| d.0)));
        gl.uniform_1_i32(gl.get_uniform_location(disp, "u_acc").as_ref(), 0);
        // albedo and normal guide the quick filter of the first samples (edges and textures stay sharp)
        gl.active_texture(glow::TEXTURE0 + 1);
        gl.bind_texture(glow::TEXTURE_2D, Some(ts[1]));
        gl.uniform_1_i32(gl.get_uniform_location(disp, "u_alb").as_ref(), 1);
        gl.active_texture(glow::TEXTURE0 + 2);
        gl.bind_texture(glow::TEXTURE_2D, Some(ts[2]));
        gl.uniform_1_i32(gl.get_uniform_location(disp, "u_nrm").as_ref(), 2);
        let u_scale = gl.get_uniform_location(disp, "u_scale");
        let u_gscale = gl.get_uniform_location(disp, "u_gscale");
        let u_filter = gl.get_uniform_location(disp, "u_filter");
        let passes = show_passes(self.samples, self.band_y, acc_h);
        let draw = |gl: &glow::Context, samples: u32, scale: f32| {
            gl.uniform_1_f32(u_scale.as_ref(), if den.is_some() { 1.0 } else { scale });
            gl.uniform_1_f32(u_gscale.as_ref(), scale);
            // full filter at 1 sample, none from 48 samples on (and never on the denoised image)
            gl.uniform_1_f32(u_filter.as_ref(), if den.is_some() { 0.0 } else { filter_strength(samples) });
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        };
        gl.bind_vertex_array(self.vao);
        if passes.partial_rows > 0 && passes.partial_rows < passes.total_rows {
            // The sample in progress covers only the bottom rows: they need their own divisor. Otherwise the
            // stripe is too bright (and the edge filter divides by the wrong count) until the sample finishes.
            let mut sc = [0i32; 4];
            gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut sc);
            let was_on = gl.is_enabled(glow::SCISSOR_TEST);
            let y = vp[1] + (passes.partial_rows as f32 * vp[3] as f32 / passes.total_rows as f32).round() as i32;
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(vp[0], vp[1], vp[2], (y - vp[1]).max(0));
            draw(gl, self.samples + 1, passes.partial);
            gl.scissor(vp[0], y, vp[2], (vp[1] + vp[3] - y).max(0));
            draw(gl, self.samples, passes.complete);
            if was_on {
                gl.scissor(sc[0], sc[1], sc[2], sc[3]);
            } else {
                gl.disable(glow::SCISSOR_TEST);
            }
        } else {
            draw(gl, self.samples, passes.complete);
        }
        gl.bind_vertex_array(None);
        for i in [2, 1, 0] {
            gl.active_texture(glow::TEXTURE0 + i);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    /// The sums in the accumulation targets: (colour RGB, albedo RGB + normal XYZ), like PathTraceJob.buf / aux.
    ///
    /// # Safety
    /// Call with the OpenGL context current (inside the egui_glow paint callback).
    pub unsafe fn read_back(&self, gl: &glow::Context) -> Option<(Vec<f32>, Vec<f32>)> {
        let (fb, _, w, h) = self.fbo?;
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(fb));
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, 4);
        let mut planes = Vec::new();
        for i in 0..3u32 {
            let mut px = vec![0f32; (w * h * 4) as usize];
            gl.read_buffer(glow::COLOR_ATTACHMENT0 + i);
            gl.read_pixels(0, 0, w, h, glow::RGBA, glow::FLOAT, glow::PixelPackData::Slice(as_bytes_mut(&mut px)));
            planes.push(px);
        }
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
        // OpenGL rows start at the bottom: flip, keep RGB
        let (w, h) = (w as usize, h as usize);
        let mut col = vec![0f32; w * h * 3];
        let mut aux = vec![0f32; w * h * 6];
        for y in 0..h {
            let src = (h - 1 - y) * w;
            for x in 0..w {
                let (s, d) = ((src + x) * 4, y * w + x);
                col[d * 3..d * 3 + 3].copy_from_slice(&planes[0][s..s + 3]);
                aux[d * 6..d * 6 + 3].copy_from_slice(&planes[1][s..s + 3]);
                aux[d * 6 + 3..d * 6 + 6].copy_from_slice(&planes[2][s..s + 3]);
            }
        }
        Some((col, aux))
    }

    /// Interactive view: refine the image of `f` in the current viewport (vp = x, y, w, h in pixels).
    /// One resolution throughout; once `max_samples` is reached the image is denoised
    /// (Intel Open Image Denoise, in the background) and shown clean.
    /// Returns true only when the image was drawn into the viewport: the caller keeps the normal
    /// rasterised view until at least one whole sample exists, so the band in progress never appears
    /// as a black rectangle rising over the 3D view.
    ///
    /// # Safety
    /// Call with the OpenGL context current (inside the egui_glow paint callback).
    pub unsafe fn live(&mut self, gl: &glow::Context, f: &PtFrame, vp: [i32; 4], tex_array: Option<glow::Texture>, display: bool) -> bool {
        let prev_fb = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
        let scissor = gl.is_enabled(glow::SCISSOR_TEST);
        // accumulate() sets its own scissor box for the bands: restore the box too, not just the flag,
        // otherwise the rest of the frame (and the next viewport) is clipped to the last band
        let mut scissor_box = [0i32; 4];
        gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut scissor_box);
        // Keep one resolution for the whole session: switching to half resolution while `moving`
        // recreated the targets and cleared the accumulation. When the camera actually changes,
        // its key in PtFrame already restarts the accumulation, so the half-resolution step only
        // caused a visible black / flickering switch between 1/2 and full size.
        let scale = 1.0;
        let (w, h) = (((vp[2] as f32 * scale) as i32).max(8), ((vp[3] as f32 * scale) as i32).max(8));
        let ok = self.samples >= f.max_samples.max(1) && self.fbo.is_some_and(|x| x.2 == w && x.3 == h) && {
            // already converged for this view: no new samples, only check the key
            let key = f.key ^ f.scene.id.rotate_left(17) ^ ((w as u64) << 40 | (h as u64) << 20);
            key == self.key && self.scene_id == f.scene.id
        } || self.accumulate(gl, f, w, h, f.max_samples.saturating_sub(self.samples).clamp(1, LIVE_MAX_PASSES), tex_array, Some(25.0));
        gl.bind_framebuffer(glow::FRAMEBUFFER, prev_fb);
        gl.viewport(vp[0], vp[1], vp[2], vp[3]);
        if scissor {
            gl.scissor(scissor_box[0], scissor_box[1], scissor_box[2], scissor_box[3]);
            gl.enable(glow::SCISSOR_TEST);
        } else {
            gl.disable(glow::SCISSOR_TEST);
        }
        if !ok {
            return false;
        }
        // A sample is drawn in bands: until one whole sample exists, the accumulation targets still
        // contain empty rows. Showing them would black out part of the viewport (and leave the rest
        // showing the previous raster frame), so keep the raster view until this is true.
        let ready = ready_to_display(self.samples, self.band_y);
        // denoise the converged image once per view
        let converged = self.samples >= f.max_samples.max(1) && !f.moving;
        if converged && f.denoise && self.den_started != self.key && crate::oidn::available().is_ok() {
            self.den_started = self.key;
            if let Some((col, aux)) = self.read_back(gl) {
                gl.bind_framebuffer(glow::FRAMEBUFFER, prev_fb);
                let (key, n, res) = (self.key, self.samples.max(1) as f32, self.den_result.clone());
                let (dw, dh) = (w as usize, h as usize);
                std::thread::spawn(move || {
                    let color: Vec<f32> = col.iter().map(|v| v / n).collect();
                    let mut albedo = Vec::with_capacity(color.len());
                    let mut normal = Vec::with_capacity(color.len());
                    for x in aux.chunks_exact(6) {
                        albedo.extend_from_slice(&[(x[0] / n).clamp(0.0, 1.0), (x[1] / n).clamp(0.0, 1.0), (x[2] / n).clamp(0.0, 1.0)]);
                        normal.extend_from_slice(&[x[3] / n, x[4] / n, x[5] / n]);
                    }
                    if let Ok(img) = crate::oidn::denoise(&crate::oidn::Input { width: dw, height: dh, color: &color, albedo: Some(&albedo), normal: Some(&normal) }) {
                        *res.lock().unwrap_or_else(|e| e.into_inner()) = Some((key, img));
                    }
                });
            }
        }
        // a finished denoise for this view: upload it (rows flipped back for OpenGL)
        let done = self.den_result.lock().ok().and_then(|mut d| d.take());
        if let Some((key, img)) = done {
            if key == self.key {
                let (fw, fh) = (w as usize, h as usize);
                let mut rgba = vec![0f32; fw * fh * 4];
                for y in 0..fh {
                    for x in 0..fw {
                        let (s, d) = ((y * fw + x) * 3, ((fh - 1 - y) * fw + x) * 4);
                        if s + 2 < img.len() {
                            rgba[d..d + 3].copy_from_slice(&img[s..s + 3]);
                            rgba[d + 3] = 1.0;
                        }
                    }
                }
                let t = self.den_tex.take().map(|d| d.0).or_else(|| gl.create_texture().ok());
                if let Some(t) = t {
                    gl.bind_texture(glow::TEXTURE_2D, Some(t));
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                    gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA32F as i32, w, h, 0, glow::RGBA, glow::FLOAT, Some(as_bytes(&rgba)));
                    gl.bind_texture(glow::TEXTURE_2D, None);
                    self.den_tex = Some((t, key));
                }
            }
        }
        if display && ready {
            self.show(gl, vp, h);
        }
        let denoised = self.den_tex.is_some_and(|d| d.1 == self.key);
        self.status = PtStatus { samples: self.samples, max: f.max_samples, w, h, denoising: converged && f.denoise && self.den_started == self.key && !denoised && crate::oidn::available().is_ok(), denoised };
        display && ready
    }
}

const VS_FULL: &str = r#"
out vec2 v_uv;
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    v_uv = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

/// accumulated sums / count -> ACES tone map -> gamma 2.2 (as pathtrace::to_srgb)
/// A quick edge-aware filter for the first samples (while the camera moves): the light (colour / albedo)
/// is averaged over 7 x 7 neighbours with weights from distance, normal and albedo similarity, then the
/// albedo is put back - noise goes, textures and edges stay. Fades out as samples add up.
const FS_SHOW: &str = r#"
in vec2 v_uv;
out vec4 o_col;
uniform sampler2D u_acc;
uniform sampler2D u_alb;
uniform sampler2D u_nrm;
uniform float u_scale;
uniform float u_gscale;
uniform float u_filter;
float aces(float x) {
    x = max(x, 0.0) * 0.6;
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}
void main() {
    vec3 c = texture(u_acc, v_uv).rgb * u_scale;
    if (u_filter > 0.01) {
        vec3 a0 = texture(u_alb, v_uv).rgb * u_gscale;
        vec3 n0 = texture(u_nrm, v_uv).rgb * u_gscale;
        // the sky (no surface) is not filtered
        if (dot(n0, n0) > 0.04) {
            n0 = normalize(n0);
            vec2 px = 1.0 / vec2(textureSize(u_acc, 0));
            vec3 sum = vec3(0.0);
            float wsum = 0.0;
            for (int y = -3; y <= 3; y++) {
                for (int x = -3; x <= 3; x++) {
                    vec2 uv = v_uv + vec2(x, y) * px * 1.5;
                    vec3 a = texture(u_alb, uv).rgb * u_gscale;
                    vec3 n = texture(u_nrm, uv).rgb * u_gscale;
                    if (dot(n, n) < 0.04) continue;
                    float wn = pow(max(dot(normalize(n), n0), 0.0), 32.0);
                    vec3 da = a - a0;
                    float wa = exp(-dot(da, da) * 40.0);
                    float ws = exp(-float(x * x + y * y) / 8.0);
                    float w = wn * wa * ws;
                    vec3 irr = texture(u_acc, uv).rgb * u_scale / max(a, vec3(0.02));
                    sum += irr * w;
                    wsum += w;
                }
            }
            if (wsum > 1e-4) c = mix(c, sum / wsum * max(a0, vec3(0.02)), u_filter);
        }
    }
    o_col = vec4(pow(vec3(aces(c.r), aces(c.g), aces(c.b)), vec3(1.0 / 2.2)), 1.0);
}
"#;

/// The path tracer: a port of `pathtrace::trace_d`, with the recursion turned into a loop
/// (throughput for glass and reflections).
const FS_PT: &str = r#"
in vec2 v_uv;
layout(location = 0) out vec4 o_col;
layout(location = 1) out vec4 o_alb;
layout(location = 2) out vec4 o_nrm;
uniform sampler2D u_tris;
uniform sampler2D u_nodes;
uniform sampler2DArray u_tex;
uniform sampler2D u_hdri;
uniform int u_has_tex;
uniform int u_bounces;
uniform int u_has_hdri;
uniform int u_ntris;
uniform vec3 u_eye;
uniform vec3 u_fwd;
uniform vec3 u_up;
uniform vec3 u_right;
uniform vec2 u_half;
uniform vec2 u_res;
uniform uint u_frame;
uniform vec3 u_sun;
uniform float u_light;
uniform vec3 u_env;
uniform int u_night;
uniform vec4 u_planes[6];
uniform int u_nplanes;
uniform vec4 u_lamp[32];
uniform int u_nlamps;

const int TW = $TW;
const uint PBR = $PBR_CODEu;
const int MAX_DEPTH = $MAX_DEPTH;
const float PI = 3.14159265;
const float TAU = 6.2831853;
const float SUN_RADIUS = 0.012;
const float AO_DIST = 2.5;
const float LAMP_RADIUS = 0.06;
const float DISTANT_RADIUS = 0.02;
const float DIFFUSE = 0.22;

vec4 tri_t(int i) { return texelFetch(u_tris, ivec2(i % TW, i / TW), 0); }
vec4 node_t(int i) { return texelFetch(u_nodes, ivec2(i % TW, i / TW), 0); }
uvec4 unpack8(float f) { uint u = floatBitsToUint(f); return uvec4(u & 255u, (u >> 8) & 255u, (u >> 16) & 255u, (u >> 24) & 255u); }

uint rng_state;
uint pcg(uint v) {
    uint s = v * 747796405u + 2891336453u;
    uint w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}
float rnd() { rng_state = pcg(rng_state); return float(rng_state >> 8) / 16777216.0; }

bool kept(vec3 p) {
    for (int i = 0; i < u_nplanes; i++) {
        if (dot(u_planes[i].xyz, p) > u_planes[i].w + 1e-4) return false;
    }
    return true;
}

float ray_tri(vec3 o, vec3 d, vec3 a, vec3 b, vec3 c) {
    vec3 e1 = b - a, e2 = c - a;
    vec3 p = cross(d, e2);
    float det = dot(e1, p);
    if (abs(det) < 1e-12) return -1.0;
    float inv = 1.0 / det;
    vec3 s = o - a;
    float u = dot(s, p) * inv;
    if (u < 0.0 || u > 1.0) return -1.0;
    vec3 q = cross(s, e1);
    float v = dot(d, q) * inv;
    if (v < 0.0 || u + v > 1.0) return -1.0;
    float t = dot(e2, q) * inv;
    return t > 1e-6 ? t : -1.0;
}

bool box(vec3 o, vec3 inv, vec3 mn, vec3 mx, float tmax) {
    vec3 t0 = (mn - o) * inv, t1 = (mx - o) * inv;
    vec3 tn = min(t0, t1), tf = max(t0, t1);
    float a = max(max(tn.x, tn.y), max(tn.z, 0.0));
    float b = min(min(tf.x, tf.y), min(tf.z, tmax));
    return a <= b;
}

// first visible hit (section planes respected)
bool hit(vec3 o, vec3 d, float tmax, out float th, out int tri) {
    th = tmax;
    tri = -1;
    if (u_ntris == 0) return false;
    vec3 sd = vec3(abs(d.x) < 1e-9 ? 1e-9 : d.x, abs(d.y) < 1e-9 ? 1e-9 : d.y, abs(d.z) < 1e-9 ? 1e-9 : d.z);
    vec3 inv = 1.0 / sd;
    int stack[64];
    int sp = 0;
    stack[sp++] = 0;
    while (sp > 0) {
        int ni = stack[--sp];
        vec4 n0 = node_t(ni * 2), n1 = node_t(ni * 2 + 1);
        if (!box(o, inv, n0.xyz, n1.xyz, th)) continue;
        uint a = floatBitsToUint(n0.w), b = floatBitsToUint(n1.w);
        if ((b & 0x80000000u) != 0u) {
            int cnt = int(b & 0x7fffffffu);
            for (int k = 0; k < cnt; k++) {
                int t = int(a) + k;
                vec3 A = tri_t(t * 4).xyz, B = tri_t(t * 4 + 1).xyz, C = tri_t(t * 4 + 2).xyz;
                float tt = ray_tri(o, d, A, B, C);
                if (tt > 0.0 && tt < th && kept(o + d * tt)) { th = tt; tri = t; }
            }
        } else if (sp < 62) {
            stack[sp++] = int(a);
            stack[sp++] = int(b);
        }
    }
    return tri >= 0;
}

float alpha_of(int tri) { return float(unpack8(tri_t(tri * 4).w).a) / 255.0; }

// light passing towards the sun / sky: glass lets part of it through, opaque stops it
float transmit(vec3 o, vec3 d, float tmax) {
    float left = tmax, k = 1.0;
    for (int i = 0; i < 4; i++) {
        float t; int tri;
        if (!hit(o, d, left, t, tri)) return k;
        float a = alpha_of(tri);
        if (a > 0.95) return 0.0;
        k *= 1.0 - a;
        o += d * (t + 1e-3);
        left -= t + 1e-3;
        if (left <= 0.0 || k < 0.02) return k;
    }
    return k;
}

// sky visibility for the ambient occlusion: fades smoothly with the distance of the obstacle (as pathtrace.rs)
float ao(vec3 p, vec3 d) {
    float t; int tri;
    if (!hit(p, d, AO_DIST, t, tri)) return 1.0;
    float near = smoothstep(0.0, 1.0, clamp(t / AO_DIST, 0.0, 1.0));
    return 1.0 - alpha_of(tri) * (1.0 - near);
}

void basis(vec3 n, out vec3 t, out vec3 b) {
    vec3 a = abs(n.x) > 0.9 ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0);
    t = normalize(cross(n, a));
    b = cross(n, t);
}

float hash2(float x, float y) { return abs(fract(sin(x * 127.1 + y * 311.7) * 43758.545)); }
float noise2(float x, float y) {
    float ix = floor(x), iy = floor(y);
    float fx = x - ix, fy = y - iy;
    fx = fx * fx * (3.0 - 2.0 * fx);
    fy = fy * fy * (3.0 - 2.0 * fy);
    return mix(mix(hash2(ix, iy), hash2(ix + 1.0, iy), fx), mix(hash2(ix, iy + 1.0), hash2(ix + 1.0, iy + 1.0), fx), fy);
}
float stepl(float edge, float x) { return x < edge ? 1.0 : 0.0; }
vec3 pattern(uint code, vec3 base, vec2 uv) {
    if (code == 1u) {
        float bx = uv.x, by = uv.y * 3.0;
        if (mod(floor(by), 2.0) > 0.5) bx += 0.5;
        float fx = bx - floor(bx), fy = by - floor(by);
        float mortar = min(stepl(0.035, fx) + stepl(0.09, fy), 1.0);
        float v = 0.82 + 0.3 * hash2(floor(bx), floor(by)) + 0.08 * noise2(uv.x * 30.0, uv.y * 30.0);
        return mix(base * v, vec3(0.80, 0.78, 0.74), mortar);
    }
    if (code == 2u) {
        float fx = uv.x - floor(uv.x), fy = uv.y - floor(uv.y);
        float g = min(stepl(0.03, fx) + stepl(0.03, fy), 1.0);
        return mix(base * (0.92 + 0.12 * hash2(floor(uv.x), floor(uv.y))), base * 0.62, g);
    }
    if (code == 3u) {
        float r = uv.x * 7.0 + noise2(uv.x * 1.5, uv.y * 10.0) * 2.5;
        return base * (0.74 + 0.36 * (0.5 + 0.5 * sin(r * PI)));
    }
    if (code == 4u) return base * (0.80 + 0.32 * (noise2(uv.x * 6.0, uv.y * 6.0) * 0.55 + noise2(uv.x * 25.0, uv.y * 25.0) * 0.30 + noise2(uv.x * 90.0, uv.y * 90.0) * 0.15));
    if (code == 5u) return base * (0.65 + 0.55 * (noise2(uv.x * 14.0, uv.y * 14.0) * 0.6 + noise2(uv.x * 60.0, uv.y * 60.0) * 0.4));
    if (code == 6u) {
        float fx = uv.x - floor(uv.x), fy = uv.y * 0.5 - floor(uv.y * 0.5);
        float j = min(stepl(0.012, fx) + stepl(0.006, fy), 1.0);
        return mix(base, base * 0.55, j);
    }
    return base;
}

// (albedo, roughness, metalness) at p; n is replaced by the normal map when there is one
void surface(int tri, vec3 p, inout vec3 n, out vec3 base, out float rough, out float metal) {
    uvec4 c = unpack8(tri_t(tri * 4).w);
    uvec4 m = unpack8(tri_t(tri * 4 + 1).w);
    base = vec3(c.rgb) / 255.0;
    rough = 1.0 - float(m.y) / 255.0;
    metal = float(m.z) / 255.0;
    uint code = m.x;
    if (code == 0u) return;
    float sc = 0.05 + float(m.w) / 255.0 * 4.0;
    vec3 an = abs(n);
    vec2 uv = (an.z >= an.x && an.z >= an.y ? p.xy : (an.x >= an.y ? p.yz : p.xz)) / sc;
    if (code == PBR) {
        if (u_has_tex == 1) {
            vec4 ca = texture(u_tex, vec3(fract(uv), float(m.y)));
            vec4 cs = texture(u_tex, vec3(fract(uv), float(m.y + 1u)));
            vec3 t, b;
            basis(n, t, b);
            vec3 tn = normalize(vec3(cs.r * 2.0 - 1.0, cs.g * 2.0 - 1.0, max(cs.b * 2.0 - 1.0, 0.05)));
            n = normalize(t * tn.x + b * tn.y + n * tn.z);
            base = ca.rgb;
            rough = ca.a;
            metal = cs.a;
        }
        return;
    }
    if (code >= 16u) {
        if (u_has_tex == 1) base = texture(u_tex, vec3(fract(uv), float(code - 16u))).rgb;
        return;
    }
    base = pattern(code, base, uv);
}

vec3 env_cols(vec3 d) {
    float a = radians(u_env.z);
    float ca = cos(a), sa = sin(a);
    vec3 rd = vec3(d.x * ca - d.y * sa, d.x * sa + d.y * ca, d.z);
    if (u_has_hdri == 1) {
        float u = fract(atan(rd.x, rd.y) / TAU + 0.5);
        float v = clamp(asin(clamp(rd.z, -1.0, 1.0)) / PI + 0.5, 0.0, 1.0);
        return texture(u_hdri, vec2(u, 1.0 - v)).rgb * max(u_env.y, 0.0);
    }
    d = rd;
    float up = clamp(d.z * 0.5 + 0.5, 0.0, 1.0);
    float horizon = pow(1.0 - abs(d.z), 4.0);
    int mode = int(u_env.x + 0.5);
    vec3 zenith = vec3(0.26, 0.38, 0.58), sky = vec3(0.72, 0.78, 0.88), ground = vec3(0.33, 0.35, 0.38);
    if (mode == 1) { zenith = vec3(0.62, 0.64, 0.68); sky = vec3(0.78, 0.79, 0.81); ground = vec3(0.26, 0.27, 0.29); }
    if (mode == 2) { zenith = vec3(0.34, 0.26, 0.36); sky = vec3(0.96, 0.52, 0.26); ground = vec3(0.20, 0.16, 0.19); }
    vec3 c = ground + (sky - ground) * up + (zenith - sky) * up * up * up;
    float glow = 0.0;
    if (mode != 1) {
        float cs = max(dot(d, u_sun), 0.0);
        glow = pow(cs, 220.0) * 9.0 + pow(cs, 7.0) * 0.45;
    }
    c.r += glow + horizon * 0.06;
    c.g += glow * (mode == 2 ? 0.55 : 0.93) + horizon * 0.05;
    c.b += glow * (mode == 2 ? 0.30 : 0.80) + horizon * 0.04;
    if (mode != 1) {
        float w = ((sin(d.x * 4.3) * cos(d.y * 5.7)) * 0.5 + 0.5) * up * up * 0.14;
        c += vec3(w, w, w * 1.05);
    }
    return c * max(u_env.y, 0.0);
}

vec3 env(vec3 d) {
    if (u_night == 1) {
        float up = clamp(d.z * 0.5 + 0.5, 0.0, 1.0);
        float moon = 0.35 * pow(max(dot(d, u_sun), 0.0), 180.0);
        return vec3(0.012 + 0.045 * up + moon, 0.018 + 0.055 * up + moon, 0.045 + 0.10 * up + moon * 1.1);
    }
    return env_cols(d);
}

vec3 f0_of(float ior, vec3 base, float metal) {
    float f = pow((ior - 1.0) / (ior + 1.0), 2.0);
    return vec3(f) + (base - vec3(f)) * metal;
}

vec3 ggx(vec3 n, vec3 v, vec3 l, float rough, vec3 f0) {
    vec3 h = normalize(v + l);
    float nh = max(dot(n, h), 0.0), nv = max(dot(n, v), 1e-4), nl = max(dot(n, l), 0.0);
    if (nl <= 0.0 || nh <= 0.0) return vec3(0.0);
    float a = max(rough * rough, 0.002);
    float a2 = a * a;
    float dd = nh * nh * (a2 - 1.0) + 1.0;
    float D = a2 / (PI * dd * dd);
    float k = ((rough + 1.0) * (rough + 1.0)) / 8.0;
    float G = (nv / (nv * (1.0 - k) + k)) * (nl / (nl * (1.0 - k) + k));
    float vh = max(dot(v, h), 0.0);
    vec3 F = f0 + (1.0 - f0) * pow(1.0 - vh, 5.0);
    return F * (D * G / (4.0 * nv * nl));
}

vec3 ggx_reflect(vec3 d, vec3 n, vec3 t, vec3 b, float rough) {
    float a = max(rough * rough, 0.002);
    float r1 = max(rnd(), 1e-6), r2 = rnd();
    float ct = sqrt((1.0 - r1) / (1.0 + (a * a - 1.0) * r1));
    float st = sqrt(max(1.0 - ct * ct, 0.0));
    float phi = TAU * r2;
    vec3 h = normalize(t * (st * cos(phi)) + b * (st * sin(phi)) + n * ct);
    return normalize(d - h * (2.0 * dot(d, h)));
}

vec3 in_unit_ball() {
    for (int i = 0; i < 16; i++) {
        vec3 v = vec3(rnd(), rnd(), rnd()) * 2.0 - 1.0;
        if (dot(v, v) <= 1.0) return v;
    }
    return vec3(0.0);
}

// lamp i as (direction to the light, distance, attenuation); false when it does not light p
bool lamp_at(int i, vec3 p, out vec3 l, out float dist, out float atten) {
    vec4 a = u_lamp[i * 4], b = u_lamp[i * 4 + 1], c = u_lamp[i * 4 + 2], e = u_lamp[i * 4 + 3];
    int kind = int(a.w + 0.5);
    float power = max(b.w, 0.0);
    if (kind == 2) {
        vec3 dir = b.xyz - a.xyz;
        if (dot(dir, dir) < 1e-12) return false;
        l = normalize(-normalize(dir) + in_unit_ball() * DISTANT_RADIUS);
        dist = 1e5;
        atten = power;
    } else {
        vec3 lv = a.xyz + in_unit_ball() * LAMP_RADIUS - p;
        dist = length(lv);
        if (dist < 1e-3) return false;
        l = lv / dist;
        atten = c.w <= 0.001 ? power : power / (1.0 + pow(dist / c.w, 2.0));
        if (kind == 1) {
            float half_a = radians(clamp(e.x, 5.0, 120.0) * 0.5);
            vec3 axis = normalize(b.xyz - a.xyz);
            float soft = max(cos(half_a), 1e-4);
            atten *= pow(clamp((max(dot(-l, axis), 0.0) - soft) / (1.0 - soft), 0.0, 1.0), 1.5);
        }
    }
    return atten > 0.0;
}

// diffuse light of the lamps at q (with shadows): the lamp light bounced once
vec3 lamp_irradiance(vec3 q, vec3 n) {
    vec3 e = vec3(0.0);
    for (int i = 0; i < u_nlamps; i++) {
        vec3 l; float d, atten;
        if (!lamp_at(i, q, l, d, atten)) continue;
        float nl = max(dot(n, l), 0.0);
        float t; int tri;
        if (nl <= 0.0 || hit(q, l, d - 1e-2, t, tri)) continue;
        e += DIFFUSE * nl * atten * u_lamp[i * 4 + 2].rgb;
    }
    return e;
}

// indirect light arriving at p along the cosine-weighted direction hd: the sun and lamp light reflected by
// the surfaces met along a path of up to u_bounces diffuse bounces, each surface tinting it
vec3 indirect_light(vec3 p, vec3 n, vec3 hd) {
    bool has_sun = dot(u_sun, u_sun) > 0.0;
    vec3 acc = vec3(0.0);
    if (u_nlamps == 0 && !has_sun) return acc;
    vec3 thr = vec3(1.0);
    vec3 o = p + n * 1e-3, dir = hd;
    for (int b = 0; b < 4; b++) {
        if (b >= u_bounces) break;
        float t2; int tri2;
        if (!hit(o, dir, 40.0, t2, tri2)) break;
        vec3 q = o + dir * t2;
        vec3 A2 = tri_t(tri2 * 4).xyz, B2 = tri_t(tri2 * 4 + 1).xyz, C2 = tri_t(tri2 * 4 + 2).xyz;
        vec3 n2 = normalize(cross(B2 - A2, C2 - A2));
        if (dot(n2, dir) > 0.0) n2 = -n2;
        vec3 b2; float r_2, m_2;
        surface(tri2, q, n2, b2, r_2, m_2);
        vec3 q2 = q + n2 * 1e-3;
        vec3 e = u_nlamps > 0 ? lamp_irradiance(q2, n2) : vec3(0.0);
        float nl2 = has_sun ? max(dot(n2, u_sun), 0.0) : 0.0;
        if (nl2 > 0.0) e += vec3(DIFFUSE * nl2 * transmit(q2, u_sun, 1e6));
        thr *= b2 * 0.55;
        acc += thr * e;
        vec3 t, bt;
        basis(n2, t, bt);
        float r1 = rnd(), r2 = rnd();
        float rr = sqrt(r1), phi = TAU * r2;
        dir = normalize(t * (rr * cos(phi)) + bt * (rr * sin(phi)) + n2 * sqrt(max(1.0 - r1, 0.0)));
        o = q2;
    }
    return acc;
}

void main() {
    ivec2 px = ivec2(gl_FragCoord.xy);
    rng_state = pcg(uint(px.x) * 1973u + uint(px.y) * 9277u + u_frame * 26699u);
    rng_state = pcg(rng_state ^ u_frame);
    vec2 uv = (vec2(px) + vec2(rnd(), rnd())) / u_res;
    vec3 d = normalize(u_fwd + u_right * ((uv.x * 2.0 - 1.0) * u_half.x) + u_up * ((uv.y * 2.0 - 1.0) * u_half.y));
    vec3 o = u_eye;
    vec3 acc = vec3(0.0), thr = vec3(1.0);
    vec3 alb0 = vec3(0.0), nrm0 = vec3(0.0);
    bool first = true;
    for (int depth = 0; depth <= MAX_DEPTH; depth++) {
        float t; int tri;
        if (!hit(o, d, 1e6, t, tri)) {
            vec3 s = env(d);
            acc += thr * s;
            if (first) alb0 = s;
            break;
        }
        vec3 A = tri_t(tri * 4).xyz, B = tri_t(tri * 4 + 1).xyz, C = tri_t(tri * 4 + 2).xyz;
        vec3 n = normalize(cross(B - A, C - A));
        if (dot(n, d) > 0.0) n = -n;
        vec3 hp = o + d * t;
        float alpha = alpha_of(tri);
        vec3 base; float rough, metal;
        surface(tri, hp, n, base, rough, metal);
        uvec4 m = unpack8(tri_t(tri * 4 + 1).w);
        vec4 ex = tri_t(tri * 4 + 3);
        float coat = ex.x, coat_rough = ex.y, sheen = ex.z, ior = ex.w;
        float gloss = m.x == PBR ? clamp(1.0 - rough, 0.03, 1.0) : float(m.y) / 255.0;
        if (first) { alb0 = base; nrm0 = n; first = false; }
        // glass: the ray goes on with probability (1 - opacity), slightly tinted
        if (alpha < 0.99 && depth < MAX_DEPTH && rnd() > alpha) {
            thr *= 0.85 + 0.15 * base;
            o = hp + d * 1e-3;
            continue;
        }
        vec3 p = hp + n * 1e-3;
        // the sun: a random direction on its disc -> soft shadows
        float lit_sun = 0.0;
        if (dot(u_sun, u_sun) > 0.0) {
            vec3 st, sb;
            basis(u_sun, st, sb);
            float r = SUN_RADIUS * sqrt(rnd()), phi = TAU * rnd();
            vec3 l = normalize(u_sun + st * (r * cos(phi)) + sb * (r * sin(phi)));
            if (dot(n, l) > 0.0) lit_sun = transmit(p, l, 1e6);
        }
        // sky light: one ray on the hemisphere, with ambient occlusion
        vec3 nt, nb;
        basis(n, nt, nb);
        float r1 = rnd(), r2 = rnd();
        float rr = sqrt(r1), phi = TAU * r2;
        vec3 hd = normalize(nt * (rr * cos(phi)) + nb * (rr * sin(phi)) + n * sqrt(max(1.0 - r1, 0.0)));
        float open = 0.18 + 0.82 * ao(p, hd);
        vec3 irr = env(hd);
        float amb = 0.30 + 0.28 * max(n.z, 0.0);
        vec3 col = base * irr * (amb * open);
        // indirect light: sun and lamp light bounced by the surfaces around, up to u_bounces times
        // (a sunny floor lights the ceiling above it, walls tint each other, corners get light)
        if (depth == 0) col += base * indirect_light(p, n, hd) * (1.0 - metal);
        vec3 vd = -d;
        float head = abs(dot(n, vd));
        vec3 f0 = f0_of(ior, base, metal);
        float sp = (1.0 - metal) * (1.0 - gloss) * 0.04 + metal * 0.25;
        col *= 1.0 - sp;
        if (dot(u_sun, u_sun) > 0.0 && lit_sun > 0.0) {
            vec3 g = ggx(n, vd, u_sun, max(gloss, 0.05), f0);
            float nl = max(dot(n, u_sun), 0.0);
            col += base * nl * (1.0 - metal) * DIFFUSE + g * lit_sun * u_light * nl;
        }
        // lamps in the model: diffuse + GGX, with shadow
        for (int i = 0; i < u_nlamps; i++) {
            vec3 l; float dl, atten;
            if (!lamp_at(i, p, l, dl, atten)) continue;
            float nl = max(dot(n, l), 0.0);
            if (nl <= 0.0) continue;
            float tt; int tri3;
            float shadow = hit(p, l, dl - 1e-2, tt, tri3) ? 0.12 : 1.0;
            vec3 g = ggx(n, vd, l, max(gloss, 0.05), f0);
            col += (base * (1.0 - metal) * DIFFUSE + g) * nl * atten * shadow * u_lamp[i * 4 + 2].rgb;
        }
        // OpenPBR clearcoat (F0 = 4 %) and sheen
        if (coat > 0.01) {
            vec3 cg = ggx(n, vd, u_sun, max(coat_rough, 0.03), vec3(0.04));
            col += coat * cg * max(dot(n, u_sun), 0.0) * lit_sun * u_light;
        }
        if (sheen > 0.01) col += sheen * pow(1.0 - head, 4.0) * base * irr * 0.55;
        // reflection of the scene: GGX importance sampling + Fresnel
        if ((gloss > 0.02 || metal > 0.02 || coat > 0.01) && depth < MAX_DEPTH) {
            float rg = (coat > 0.01 && rnd() < coat) ? coat_rough : gloss;
            vec3 rdir = ggx_reflect(d, n, nt, nb, max(rg, 0.03));
            vec3 fres = f0 + (1.0 - f0) * pow(1.0 - head, 5.0);
            float w = min(max(max(gloss * 0.55, metal * 0.75) + coat * 0.25, 0.0), 0.95);
            vec3 f = clamp(fres * w, 0.0, 1.0);
            acc += thr * col * (1.0 - f);
            thr *= f;
            o = p;
            d = rdir;
            continue;
        }
        acc += thr * col;
        break;
    }
    o_col = vec4(acc, 1.0);
    o_alb = vec4(alb0, 1.0);
    o_nrm = vec4(nrm0, 1.0);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_band_gets_its_own_divisor() {
        let p = show_passes(4, 10, 100);
        assert_eq!(p.partial_rows, 10);
        assert_eq!(p.total_rows, 100);
        assert!((p.complete - 0.25).abs() < 1e-6);
        // the drawn rows have 5 contributions, not 4: a single divisor makes them 25% too bright
        assert!((p.partial - 0.2).abs() < 1e-6);
    }

    #[test]
    fn first_partial_band_is_not_darkened() {
        let p = show_passes(0, 5, 20);
        assert_eq!(p.partial_rows, 5);
        assert!((p.complete - 1.0).abs() < 1e-6);
        assert!((p.partial - 1.0).abs() < 1e-6);
    }

    #[test]
    fn finished_sample_uses_one_divisor_for_the_whole_image() {
        let p = show_passes(8, 0, 600);
        assert_eq!(p.partial_rows, 0);
        assert!((p.complete - 0.125).abs() < 1e-6);
        assert!((p.partial - 1.0 / 9.0).abs() < 1e-6);
    }

    #[test]
    fn raster_view_is_kept_until_a_whole_sample_exists() {
        assert!(!ready_to_display(0, 0));
        assert!(!ready_to_display(0, 64));
        // after the first sample the image stays on screen even while another band is drawn
        assert!(ready_to_display(1, 32));
        assert!(ready_to_display(2, 32));
        assert!(ready_to_display(256, 0));
    }

    #[test]
    fn quick_filter_fades_out_by_48_samples() {
        assert!((filter_strength(1) - 1.0).abs() < 1e-6);
        assert!(filter_strength(24) > 0.0 && filter_strength(24) < 1.0);
        assert_eq!(filter_strength(48), 0.0);
        assert_eq!(filter_strength(256), 0.0);
    }
}
