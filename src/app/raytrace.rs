//! Interactive Ray Trace (Render › Ray Trace): the 3D view is path traced on the graphics card and
//! refines while the camera stands still - half resolution while it moves, denoised once converged,
//! like Interactive Ray Trace in Navisworks. The same GPU path tracer renders the final images.
use super::*;
use crate::gpu_pt::{PtFrame, PtScene};
use eframe::egui;
use std::hash::{Hash, Hasher};

/// State of the interactive Ray Trace view.
#[derive(Default)]
pub struct RtState {
    /// the view is path traced
    pub on: bool,
    /// the scene on the GPU (rebuilt when the look of the model changes)
    pub scene: Option<Arc<PtScene>>,
    /// key of `scene` (or of the scene being built)
    pub key: u64,
    /// a scene being built in the background (BVH)
    pub pending: Option<Arc<Mutex<Option<Arc<PtScene>>>>>,
    /// bumped by rebuild_colors: materials, overrides, profile, 4D colours, hidden items
    pub looks_gen: u64,
    /// the HDRI in use, loaded once
    pub hdri: Option<(String, Arc<crate::hdri::Hdri>)>,
    /// the last error was already shown
    pub error_shown: bool,
    /// an AI capture turned the view on just for itself: turn it off again afterwards
    pub off_after_capture: bool,
}

impl App {
    /// Turns the interactive Ray Trace view on / off.
    pub fn toggle_raytrace(&mut self) {
        self.rt.on = !self.rt.on;
        self.rt.error_shown = false;
        if self.rt.on {
            if self.loaded_models().next().is_none() {
                self.rt.on = false;
                self.error("Open a model first - ray tracing needs geometry.");
                return;
            }
            self.toast("Ray Trace on: the view refines while the camera stands still (lower resolution while moving). Click Ray Trace again to go back.");
        } else {
            // free the GPU scene; it is rebuilt the next time
            self.rt.scene = None;
            self.rt.pending = None;
            self.rt.key = 0;
        }
    }

    /// What the scene depends on: the models, the look of every element, hidden items, section, lamps, textures.
    fn rt_scene_key(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for (slot, m) in self.loaded_models() {
            (slot, Arc::as_ptr(m) as usize).hash(&mut h);
        }
        self.rt.looks_gen.hash(&mut h);
        self.hidden.len().hash(&mut h);
        for p in self.section.gpu_planes() {
            p.to_array().map(f32::to_bits).hash(&mut h);
        }
        for l in self.lights.iter().filter(|l| l.enabled) {
            l.pos.map(f32::to_bits).hash(&mut h);
            l.target.map(f32::to_bits).hash(&mut h);
            (l.power.to_bits(), l.radius.to_bits(), l.angle.to_bits(), l.color).hash(&mut h);
        }
        self.tex_layers.len().hash(&mut h);
        h.finish().max(1)
    }

    fn rt_hdri(&mut self) -> Option<Arc<crate::hdri::Hdri>> {
        let path = self.render.hdri_path.clone();
        if path.is_empty() {
            return None;
        }
        if let Some((p, h)) = &self.rt.hdri {
            if *p == path {
                return Some(h.clone());
            }
        }
        match crate::hdri::load(&path) {
            Ok(h) => {
                let h = Arc::new(h);
                self.rt.hdri = Some((path, h.clone()));
                Some(h)
            }
            Err(e) => {
                self.render.hdri_path.clear();
                self.error(e);
                None
            }
        }
    }

    /// The camera, sun and sky of the current view as a path-tracing frame.
    fn pt_frame(&mut self, scene: Arc<PtScene>, max_samples: u32, moving: bool) -> PtFrame {
        let cam = self.cam;
        let env = (self.render.env_mode, self.render.env_intensity.clamp(0.0, 3.0), self.render.env_rotation);
        let light = self.render.sun_power.clamp(0.1, 3.0) * self.render.exposure.clamp(0.1, crate::anim::MAX_BRIGHTNESS);
        let hdri = self.rt_hdri();
        let sun = self.render.sun_dir();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for v in [cam.eye(), cam.fwd(), cam.up(), sun] {
            v.to_array().map(f32::to_bits).hash(&mut h);
        }
        (cam.fov_deg.to_bits(), light.to_bits(), env.0, env.1.to_bits(), env.2.to_bits(), self.render.night, hdri.as_ref().map(|x| Arc::as_ptr(x) as usize)).hash(&mut h);
        PtFrame {
            scene,
            eye: cam.eye(),
            fwd: cam.fwd(),
            up: cam.up(),
            right: cam.right(),
            fov_deg: cam.fov_deg,
            sun,
            light,
            env,
            night: self.render.night,
            hdri,
            max_samples,
            moving,
            denoise: self.settings.render_denoise,
            key: h.finish(),
        }
    }

    /// A final render (Render › Path Trace) is running on the GPU: the live view pauses meanwhile.
    pub fn gpu_render_running(&self) -> bool {
        self.renderer.lock().map(|r| r.pt_job.is_some()).unwrap_or(false)
    }

    /// The frame for the interactive view this paint (None while the first scene is still being built).
    pub fn rt_frame(&mut self, _aspect: f32) -> Option<PtFrame> {
        // a scene finished in the background
        if let Some(slot) = &self.rt.pending {
            let done = slot.lock().ok().and_then(|mut s| s.take());
            if let Some(sc) = done {
                self.rt.scene = Some(sc);
                self.rt.pending = None;
            }
        }
        let key = self.rt_scene_key();
        if key != self.rt.key && self.rt.pending.is_none() {
            // the look of the model changed: gather the visible triangles now (fast), the BVH on a thread
            self.rt.key = key;
            self.rebuild_colors();
            let sc = crate::pathtrace::Scene::gather(self);
            let slot: Arc<Mutex<Option<Arc<PtScene>>>> = Arc::new(Mutex::new(None));
            let out = slot.clone();
            std::thread::spawn(move || {
                let mut sc = sc;
                sc.bvh = crate::model::Bvh::build(&sc.tris);
                let packed = Arc::new(PtScene::pack(&sc, key));
                *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(packed);
            });
            self.rt.pending = Some(slot);
        }
        let scene = self.rt.scene.clone()?;
        let moving = self.lod_moving() || self.cam_anim.is_some();
        Some(self.pt_frame(scene, self.settings.rt_samples.max(1), moving))
    }

    /// The frame for a path-traced capture (still camera).
    pub fn rt_capture_frame(&mut self, scene: Arc<PtScene>) -> PtFrame {
        let mut f = self.pt_frame(scene, 64, false);
        f.key ^= 0xca97;
        f
    }

    /// Builds the ray-trace scene now (AI captures with ray_trace: true), without waiting for the background build.
    pub fn rt_scene_now(&mut self) {
        let key = self.rt_scene_key();
        if self.rt.scene.as_ref().is_some_and(|s| s.id == key) {
            return;
        }
        self.rebuild_colors();
        let sc = crate::pathtrace::Scene::build(self);
        self.rt.scene = Some(Arc::new(PtScene::pack(&sc, key)));
        self.rt.key = key;
        self.rt.pending = None;
    }

    /// Progress over the view ("Ray Trace · 64 / 256"), and repaints until the image is final.
    pub fn rt_overlay(&mut self, ui: &egui::Ui, painter: &egui::Painter, rect: egui::Rect) {
        let (status, err) = {
            let r = self.renderer.lock().unwrap_or_else(|e| e.into_inner());
            (r.gpu_live.status, r.gpu_live.error.clone())
        };
        if let Some(e) = err {
            if !self.rt.error_shown {
                self.rt.error_shown = true;
                self.rt.on = false;
                self.error(format!("Ray Trace is not available on this graphics card: {e}"));
            }
            return;
        }
        let building = self.rt.pending.is_some();
        let text = if self.rt.scene.is_none() {
            "Ray Trace · preparing the scene…".to_string()
        } else if status.denoised {
            format!("Ray Trace · {} samples · denoised", status.samples)
        } else if status.denoising {
            format!("Ray Trace · {} samples · denoising…", status.samples)
        } else {
            format!("Ray Trace · {} / {} samples{}{}", status.samples.min(status.max), status.max, if status.w > 0 && (status.w as f32) < rect.width() * 0.75 { " · moving (¼ resolution)" } else { "" }, if building { " · updating the scene…" } else { "" })
        };
        let pos = rect.left_top() + egui::vec2(10.0, 10.0);
        let font = egui::FontId::proportional(12.0);
        let galley = painter.layout_no_wrap(text, font, egui::Color32::WHITE);
        let bg = egui::Rect::from_min_size(pos, galley.size()).expand2(egui::vec2(8.0, 4.0));
        painter.rect_filled(bg, 6.0, egui::Color32::from_black_alpha(150));
        painter.galley(pos, galley, egui::Color32::WHITE);
        if self.rt.scene.is_none() || building || status.samples < status.max || status.denoising {
            ui.ctx().request_repaint();
        }
    }

    /// Final render on the GPU (Render › Path Trace) when it is available and enabled; false = use the CPU.
    pub(super) fn start_gpu_render(&mut self, scene: Arc<crate::pathtrace::Scene>, denoise: bool) -> bool {
        if !self.settings.pt_gpu {
            return false;
        }
        if self.renderer.lock().map(|r| r.gpu_live.error.is_some()).unwrap_or(true) {
            return false;
        }
        let (w, h, samples, tlimit) = self.pt.params;
        let packed = Arc::new(PtScene::pack(&scene, self.rt_scene_key() ^ 0x5eed));
        let frame = self.pt_frame(packed, samples, false);
        let job = crate::pathtrace::new_job((w, h, samples, tlimit), scene, frame.eye, frame.fwd, frame.up, frame.right, frame.fov_deg, frame.sun, frame.light, frame.env, frame.night, frame.hdri.clone(), denoise);
        if let Ok(mut r) = self.renderer.lock() {
            r.pt_job = Some(crate::render::GpuJob { frame, job: job.clone(), last_read: std::time::Instant::now(), step: 1 });
        }
        self.pt.job = Some(job);
        true
    }
}
