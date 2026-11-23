//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    // ------------------------------------------------------------------ timeliner
    pub fn sim_range(&self) -> Option<(i32, i32)> {
        timeline::range(&self.tasks)
    }

    pub fn update_sim(&mut self, dt: f32) {
        if !self.sim_view() {
            self.sim_play = false;
            if !self.sim_cache.is_empty() {
                self.sim_cache.clear();
                self.sim_cache_day = i32::MIN;
                self.colors_dirty = true;
            }
            return;
        }
        let Some((s, e)) = self.sim_range() else { return };
        if self.sim_play {
            self.sim_day += dt * self.sim_speed;
            if self.sim_day >= e as f32 + 1.0 {
                self.sim_day = e as f32 + 1.0;
                self.sim_play = false;
            }
        }
        self.sim_day = self.sim_day.clamp(s as f32 - 1.0, e as f32 + 1.0);
        let d = self.sim_day.floor() as i32;
        if d != self.sim_cache_day {
            self.sim_cache = timeline::state_at(&self.tasks, d);
            self.sim_cache_day = d;
            self.colors_dirty = true;
        }
    }

    pub fn invalidate_sim(&mut self) {
        self.sim_cache_day = i32::MIN;
    }

    // ------------------------------------------------------------------ animation / render
    pub fn sim_sync(&mut self, anim: usize, t: f32) {
        let Some(a) = self.animations.get(anim) else { return };
        if a.sim_4d {
            if let Some((s, e)) = self.sim_range() {
                let dur = a.duration().max(1e-3);
                self.anim_sim_active = true;
                self.sim_day = s as f32 + (e - s + 1) as f32 * (t / dur).clamp(0.0, 1.0);
                return;
            }
        }
        if self.anim_sim_active {
            self.anim_sim_active = false;
            self.colors_dirty = true;
        }
    }

    pub fn goto_anim_time(&mut self, t: f32) {
        self.anim_time = t;
        if let Some(c) = self.animations.get(self.cur_anim).and_then(|a| a.sample(t)) {
            self.cam = c;
        }
        self.sim_sync(self.cur_anim, t);
        self.anim_objects(self.cur_anim, t);
    }

    /// Elementele unui set animat.
    pub fn anim_set_items(&self, s: &crate::anim::AnimSet) -> Vec<ElemId> {
        if !s.items.is_empty() {
            return s.items.clone();
        }
        self.resolve_selector(&s.target)
    }

    /// Aplica starea seturilor de obiecte animate la timpul t (culoare, transparenta, vizibilitate).
    pub fn anim_objects(&mut self, anim: usize, t: f32) {
        let mut map: HashMap<ElemId, ([u8; 4], bool)> = HashMap::new();
        if let Some(a) = self.animations.get(anim) {
            for s in a.sets.iter().filter(|s| s.enabled) {
                let Some(k) = s.sample(t) else { continue };
                for id in self.anim_set_items(s) {
                    let base = self.elem(id).map(|(m, e)| m.tri_color.get(e.tri_start as usize).copied().unwrap_or([200, 200, 200, 255])).unwrap_or([200, 200, 200, 255]);
                    let c = k.color.unwrap_or([base[0], base[1], base[2]]);
                    let alpha = ((1.0 - k.transparency.clamp(0.0, 1.0)) * base[3] as f32) as u8;
                    // complet transparent = ascuns
                    map.insert(id, ([c[0], c[1], c[2], alpha], k.visible && alpha > 2));
                }
            }
        }
        if map != self.anim_obj {
            self.anim_obj = map;
            self.colors_dirty = true;
        }
    }

    /// Real-time preview in the viewport (si Record).
    pub fn anim_tick(&mut self, dt: f32) {
        // Record: un keyframe la fiecare 0,2 s cand camera s-a miscat
        if let Some((start, keys)) = &mut self.anim_rec {
            let t = start.elapsed().as_secs_f32();
            let moved = keys.last().map_or(true, |k| {
                let (a, b) = (k.cam, self.cam);
                (a.eye() - b.eye()).length() > 1e-3 || (a.yaw - b.yaw).abs() > 1e-4 || (a.pitch - b.pitch).abs() > 1e-4 || (a.fov_deg - b.fov_deg).abs() > 1e-3
            });
            if keys.last().map_or(true, |k| t - k.t >= 0.2) && moved {
                keys.push(crate::anim::Keyframe { t, cam: self.cam });
            }
        }
        let Some(t) = self.anim_play else { return };
        let dur = self.animations.get(self.cur_anim).map(|a| a.duration()).unwrap_or(0.0);
        let mut t = t + dt * self.anim_dir;
        if t > dur || t < 0.0 {
            if self.anim_pp && dur > 0.0 {
                // ping-pong: schimba directia
                self.anim_dir = -self.anim_dir;
                t = t.clamp(0.0, dur);
            } else if self.anim_loop && dur > 0.0 {
                t = if self.anim_dir > 0.0 { 0.0 } else { dur };
            } else {
                self.anim_play = None;
                self.goto_anim_time(t.clamp(0.0, dur));
                return;
            }
        }
        self.anim_play = Some(t);
        self.goto_anim_time(t);
    }

    /// Redare inainte (dir 1) sau inapoi (dir -1) de la timpul curent.
    pub fn play_anim(&mut self, dir: f32) {
        let dur = self.animations.get(self.cur_anim).map(|a| a.duration()).unwrap_or(0.0);
        if dur <= 0.0 {
            return;
        }
        self.anim_dir = dir;
        let mut t = self.anim_time;
        if dir > 0.0 && t >= dur - 1e-3 {
            t = 0.0;
        } else if dir < 0.0 && t <= 1e-3 {
            t = dur;
        }
        self.anim_play = Some(t);
    }

    /// Pas la keyframe-ul urmator (dir 1) sau anterior (dir -1).
    pub fn step_anim(&mut self, dir: i32) {
        let Some(a) = self.animations.get(self.cur_anim) else { return };
        let ts = a.key_times();
        let now = self.anim_time;
        let t = if dir > 0 { ts.iter().copied().find(|x| *x > now + 1e-3) } else { ts.iter().rev().copied().find(|x| *x < now - 1e-3) };
        if let Some(t) = t {
            self.anim_play = None;
            self.goto_anim_time(t);
        }
    }

    /// Record (Navisworks Animation > Record): porneste / opreste inregistrarea navigarii; la oprire
    /// devine o animatie noua.
    pub fn toggle_record(&mut self) {
        match self.anim_rec.take() {
            None => {
                self.stop_anim();
                self.anim_rec = Some((Instant::now(), vec![crate::anim::Keyframe { t: 0.0, cam: self.cam }]));
                self.toast("Recording: navigate in the 3D view (orbit, walk, zoom). Press Record again to stop.");
            }
            Some((start, mut keys)) => {
                let t = start.elapsed().as_secs_f32();
                if keys.last().map_or(true, |k| t - k.t > 0.05) {
                    keys.push(crate::anim::Keyframe { t, cam: self.cam });
                }
                if keys.len() < 2 || t < 0.3 {
                    return self.toast("Recording stopped - nothing recorded (move the camera while recording).");
                }
                let n = self.animations.iter().filter(|a| a.name.starts_with("Recording")).count() + 1;
                let mut a = crate::anim::Animation::new(format!("Recording {n}"));
                a.ease = false;
                a.kind = crate::anim::AnimKind::Recording;
                a.keys = keys;
                self.animations.push(a);
                self.cur_anim = self.animations.len() - 1;
                self.anim_time = 0.0;
                self.toast(format!("Recording saved: {:.1} s, {} keyframes.", t, self.animations[self.cur_anim].keys.len()));
            }
        }
    }

    /// Animatie prin toate viewpoint-urile salvate, in ordine.
    pub fn anim_from_viewpoints(&mut self) {
        if self.viewpoints.is_empty() {
            return self.error("Save some viewpoints first (Viewpoint › Save View).");
        }
        let mut a = crate::anim::Animation::new(format!("Walkthrough {}", self.animations.len() + 1));
        a.kind = crate::anim::AnimKind::Viewpoints;
        for (i, v) in self.viewpoints.iter().enumerate() {
            a.keys.push(crate::anim::Keyframe { t: i as f32 * self.key_step, cam: v.cam });
        }
        if a.keys.len() == 1 {
            // un singur viewpoint: camera sta pe loc cat un pas
            let k = a.keys[0];
            a.keys.push(crate::anim::Keyframe { t: self.key_step, cam: k.cam });
        }
        self.stop_anim();
        self.animations.push(a);
        self.cur_anim = self.animations.len() - 1;
        self.anim_time = 0.0;
    }

    /// Import 4D: secventa de constructie din TimeLiner ca animatie (camera = vederea curenta). Fara
    /// planificare, intai se importa una (CSV, MS Project, Primavera P6).
    pub fn import_4d_animation(&mut self) {
        let Some((s, e)) = self.sim_range() else {
            self.anim_4d_pending = true;
            self.toast("Choose the construction schedule (CSV, MS Project XML or Primavera P6 XML) - the 4D animation is created from it.");
            return self.request_path(Purpose::ImportTasksCsv);
        };
        // ~1 s pe saptamana de santier, intre 10 s si 60 s
        let weeks = ((e - s + 1).max(1) as f32) / 7.0;
        let dur = weeks.clamp(10.0, 60.0);
        let mut a = crate::anim::Animation::new(format!("4D sequence {}", self.animations.iter().filter(|a| a.kind == crate::anim::AnimKind::FourD).count() + 1));
        a.kind = crate::anim::AnimKind::FourD;
        a.sim_4d = true;
        a.ease = false;
        a.keys.push(crate::anim::Keyframe { t: 0.0, cam: self.cam });
        a.keys.push(crate::anim::Keyframe { t: dur, cam: self.cam });
        self.stop_anim();
        self.animations.push(a);
        self.cur_anim = self.animations.len() - 1;
        self.anim_time = 0.0;
        self.open_panel(crate::layout::Panel::Animation);
        self.toast(format!("4D animation created: {} tasks over {:.0} s, seen from the current view. Press ▶ to play it.", self.tasks.len(), dur));
    }

    pub fn stop_anim(&mut self) {
        self.anim_play = None;
        self.anim_dir = 1.0;
        if self.anim_sim_active {
            self.anim_sim_active = false;
            self.colors_dirty = true;
        }
        if !self.anim_obj.is_empty() {
            self.anim_obj.clear();
            self.colors_dirty = true;
        }
    }

    pub fn render_bg(&self) -> ([u8; 3], [u8; 3]) {
        let pal = crate::theme::palette(self.settings.theme);
        let c = |c: eframe::egui::Color32| [c.r(), c.g(), c.b()];
        match self.render.background {
            Background::Theme => (c(pal.vp_top), c(pal.vp_bottom)),
            Background::White => ([255, 255, 255], [236, 239, 243]),
            Background::Dark => ([28, 32, 40], [72, 80, 94]),
        }
    }

    pub fn start_render_job(&mut self, path: &str) {
        if self.render_job.is_some() {
            return;
        }
        let Some(anim) = self.animations.get(self.cur_anim) else { return self.error("Create an animation first.") };
        if anim.keys.is_empty() || anim.duration() <= 0.0 {
            return self.error("This animation is empty - record, orbit or capture at least two keyframes.");
        }
        let fmt = self.render.format;
        if fmt == VideoFormat::Mp4 && !self.ffmpeg.unwrap_or(false) {
            return self.error("MP4 needs ffmpeg (https://ffmpeg.org) on the PATH - choose AVI or install ffmpeg.");
        }
        let mut path = std::path::PathBuf::from(path);
        if path.extension().map(|e| e.to_string_lossy().to_lowercase()) != Some(fmt.ext().to_string()) {
            path.set_extension(fmt.ext());
        }
        // marimea vine si din proiect (nevalidata): limitele din UI
        let (w, h, fps, q) = (self.render.width.clamp(64, 7680) & !1, self.render.height.clamp(64, 4320) & !1, self.render.fps.clamp(1, 120), self.render.quality);
        let ss = if self.render.supersample && fmt != VideoFormat::Gif { 2 } else { 1 };
        let frames = (anim.duration() * fps as f32).ceil() as u32 + 1;
        let sink = match crate::video::open_sink(fmt, &path, w, h, fps, q) {
            Ok(s) => s,
            Err(e) => return self.error(e),
        };
        // coada limitata: un cadru 4K supersampled are ~500 MB; captura asteapta encoderul in loc sa umple memoria
        let (tx, rx) = std::sync::mpsc::sync_channel::<Frame>(2);
        let (dtx, drx) = channel();
        let p2 = path.clone();
        std::thread::spawn(move || {
            // o panica in encoder devine eroare (altfel exportul ar ramane „in curs” pentru totdeauna)
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                let mut sink = sink;
                let mut res = Ok(());
                while let Ok(f) = rx.recv() {
                    let f = if ss > 1 { f.downscale(ss) } else { f };
                    if let Err(e) = sink.add(&f) {
                        res = Err(e);
                        break;
                    }
                }
                if res.is_ok() {
                    res = sink.finish();
                }
                if res.is_ok() && fmt == VideoFormat::Mp4 {
                    res = crate::video::convert_mp4(&p2);
                }
                res
            }))
            .unwrap_or_else(|_| Err("Internal error in the video encoder - the export was stopped.".to_string()));
            let _ = dtx.send(res);
        });
        self.stop_anim();
        let saved_sel = std::mem::take(&mut self.selected);
        self.colors_dirty = true;
        // Render with materials: video in stilul Realistic (materiale, texturi, umbre); vederea revine la final
        let saved_style = if self.render.with_materials {
            let prev = (self.settings.visual_style, self.materials_on);
            self.settings.visual_style = crate::render::VisualStyle::Realistic;
            if self.mat_assign.is_empty() && self.loaded_models().next().is_some() {
                self.auto_materials();
            }
            self.materials_on = true;
            self.materials_changed();
            Some(prev)
        } else {
            None
        };
        self.render_job = Some(RenderJob { anim: self.cur_anim, frame: 0, frames, fps, ss, w: w * ss, h: h * ss, tx: Some(tx), done: drx, path, waiting: false, started: Instant::now(), saved_sel, saved_style });
    }

    /// Ce se scrie pe imaginile / cadrele exportate: (titlu, randuri culoare + text, jos).
    /// 4D: data simularii si sarcinile in lucru (sus); Appearance Profiler: legenda culorilor (jos).
    pub fn image_overlays(&self) -> Vec<(String, Vec<([u8; 4], String)>, bool)> {
        let mut out = Vec::new();
        if self.render.overlay_4d && self.sim_view() && !self.tasks.is_empty() {
            let day = self.sim_day.floor() as i32;
            let col = |k: timeline::TaskKind| match k {
                timeline::TaskKind::Construct => timeline::COL_CONSTRUCT,
                timeline::TaskKind::Demolish => timeline::COL_DEMOLISH,
                timeline::TaskKind::Temporary => timeline::COL_TEMP,
            };
            let active: Vec<&timeline::Task> = self.tasks.iter().filter(|t| t.start <= day && day <= t.end).collect();
            let mut rows: Vec<([u8; 4], String)> = active.iter().take(6).map(|t| (col(t.kind), format!("{}  ({} - {})", t.name, timeline::fmt_date(t.start), timeline::fmt_date(t.end)))).collect();
            if active.len() > 6 {
                rows.push(([120, 120, 120, 255], format!("+ {} more tasks", active.len() - 6)));
            }
            if rows.is_empty() {
                rows = [timeline::TaskKind::Construct, timeline::TaskKind::Demolish, timeline::TaskKind::Temporary].into_iter().map(|k| (col(k), k.label().to_string())).collect();
            }
            let (s, e) = self.sim_range().unwrap_or((day, day));
            let pct = if e > s { ((day - s) as f32 / (e - s) as f32 * 100.0).clamp(0.0, 100.0) } else { 100.0 };
            out.push((format!("4D  {}   ({pct:.0}%)", timeline::fmt_date(day)), rows, false));
        }
        if self.render.legend_on_images && self.profile.active && !self.profile.rules.is_empty() {
            let vis: Vec<&crate::profiler::ProfileRule> = self.profile.rules.iter().filter(|r| r.visible).collect();
            let mut rows: Vec<([u8; 4], String)> = vis.iter().take(14).map(|r| (r.color, if r.count > 0 { format!("{}  ({})", r.label, r.count) } else { r.label.clone() })).collect();
            if vis.len() > 14 {
                rows.push(([120, 120, 120, 255], format!("+ {} more", vis.len() - 14)));
            }
            out.push((self.profile.prop.clone(), rows, true));
        }
        out
    }

    pub fn cancel_render(&mut self) {
        if let Some(j) = self.render_job.as_mut() {
            j.frames = j.frame;
            j.tx = None;
        }
    }

    /// Drives the export: one captured frame per UI frame. Called before the viewport is drawn.
    pub fn start_pathtrace(&mut self) {
        // Un job terminat ramane in `self.pathtrace` (ca sa putem exporta PNG-ul),
        // dar NU trebuie sa blocheze o randare noua.
        if let Some(j) = &self.pathtrace {
            if !j.done.load(std::sync::atomic::Ordering::Relaxed) {
                self.toast("Path trace already running.");
                return;
            }
        }
        if self.loaded_models().next().is_none() {
            self.error("Open a model first - path tracing needs geometry.");
            return;
        }
        // „Render with materials”: materialele (auto dupa tipul IFC daca nu exista) intra in scena,
        // apoi vederea 3D revine cum era
        let restore = if self.render.with_materials && !self.materials_on {
            if self.mat_assign.is_empty() {
                self.auto_materials();
            }
            self.materials_on = true;
            Some(false)
        } else {
            if self.render.with_materials && self.mat_assign.is_empty() {
                self.auto_materials();
            }
            None
        };
        // fara nuanta de selectie si cu aspectul final calculat chiar acum
        self.materials_changed();
        self.rebuild_colors();
        let scene = std::sync::Arc::new(crate::pathtrace::Scene::build(self));
        if let Some(on) = restore {
            self.materials_on = on;
            self.materials_changed();
        }
        let cam = self.cam;
        let sun = self.render.sun_dir();
        let (w, h, samples, tlimit) = self.pathtrace_params;
        self.pathtrace_tex = None;
        self.pathtrace_notified = false;
        self.pathtrace_done_at = None;
        let denoise = self.settings.render_denoise && crate::oidn::available().is_ok();
        self.pathtrace = Some(crate::pathtrace::start_arc((w, h, samples, tlimit), scene, cam.eye(), cam.fwd(), cam.up(), cam.right(), cam.fov_deg, sun, self.render.sun_power.clamp(0.1, 3.0), denoise));
        self.toast(format!("Path tracing {}×{} · {} samples · {} s max{}", w, h, samples, tlimit, if denoise { " · denoise" } else { "" }));
    }

    pub fn cancel_pathtrace(&mut self) {
        if let Some(j) = &self.pathtrace {
            j.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    pub fn pathtrace_step(&mut self, ctx: &eframe::egui::Context) {
        let Some(job) = self.pathtrace.clone() else { return; };
        let count = job.count.load(std::sync::atomic::Ordering::Relaxed);
        if count == 0 {
            ctx.request_repaint();
            return;
        }
        // Worker-ul tine mutex-ul cat timp randreaza un sample intreg. Cu try_lock
        // nu blocam firul UI (la 512/1024 px altfel aplicatia pare inghetata).
        let done = job.done.load(std::sync::atomic::Ordering::Relaxed);
        // dupa terminare imaginea nu se mai schimba: nu refacem textura la fiecare cadru
        if done && self.pathtrace_notified && self.pathtrace_tex.is_some() {
            return;
        }
        let w = job.width as usize;
        let h = job.height as usize;
        let mut pixels = Vec::with_capacity(w * h * 4);
        let to8 = |x: f32| (x.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0 + 0.5) as u8;
        // imaginea curatata de OIDN, daca exista; altfel media esantioanelor
        let den = if done { job.denoised.lock().ok().and_then(|d| d.clone()) } else { None };
        if let Some(img) = den {
            for p in img.chunks_exact(3) {
                pixels.extend_from_slice(&[to8(p[0]), to8(p[1]), to8(p[2]), 255]);
            }
        } else {
            let Ok(buf) = job.buf.try_lock() else {
                ctx.request_repaint();
                return;
            };
            let n = count as f32;
            for p in buf.chunks_exact(3) {
                pixels.extend_from_slice(&[to8(p[0] / n), to8(p[1] / n), to8(p[2] / n), 255]);
            }
        }
        let img = eframe::egui::ColorImage::from_rgba_unmultiplied([w, h], &pixels);
        if let Some(tex) = &mut self.pathtrace_tex {
            tex.set(img, eframe::egui::TextureOptions::default());
        } else {
            self.pathtrace_tex = Some(ctx.load_texture("pathtrace", img, eframe::egui::TextureOptions::default()));
        }
        if job.done.load(std::sync::atomic::Ordering::Relaxed) {
            if !self.pathtrace_notified {
                self.pathtrace_notified = true;
                self.pathtrace_done_at = Some(job.started.elapsed().as_secs_f32());
                self.toast(format!("✅ {} · {} {} · {:.1}s — {}", crate::i18n::tr("Path trace FINISHED"), job.count.load(std::sync::atomic::Ordering::Relaxed), crate::i18n::tr("samples"), self.pathtrace_done_at.unwrap_or(0.0), crate::i18n::tr("Export PNG is ready")));
            }
        }
        // Repaint doar cat timp randarea e activa. Daca o cerem si dupa
        // terminare, egui ramane intr-o bucla permanenta si aplicatia "merge" continuu.
        if !job.done.load(std::sync::atomic::Ordering::Relaxed) {
            ctx.request_repaint();
        }
    }

    pub fn export_pathtrace(&mut self, path: &str) {
        let job = self.pathtrace.clone();
        let Some(job) = job else {
            self.error("No path trace running.");
            return;
        };
        // imaginea curatata (deja medie) sau suma esantioanelor / numar
        let den = job.denoised.lock().ok().and_then(|d| d.clone());
        let r = match den {
            Some(img) => crate::pathtrace::save_png(path, &img, 1, job.width, job.height),
            None => {
                let buf = job.buf.lock().unwrap_or_else(|e| e.into_inner());
                let count = job.count.load(std::sync::atomic::Ordering::Relaxed).max(1);
                crate::pathtrace::save_png(path, &buf, count, job.width, job.height)
            }
        };
        match r {
            Ok(()) => self.toast(format!("Path trace saved: {}", crate::file_name(path))),
            Err(e) => self.error(e),
        }
    }
    pub fn render_step(&mut self) {
        let captured = self.renderer.lock().unwrap_or_else(|e| e.into_inner()).captured.take();
        if let Some(r) = captured {
            let overlay = self.image_overlays();
            let r = r.map(|mut f| {
                for (title, rows, bottom) in &overlay {
                    f.draw_legend(title, rows, *bottom);
                }
                f
            });
            if let Some(p) = self.still_path.take() {
                // vederea revine cum era (stil, materiale, selectie)
                if let Some((style, mat, sel)) = self.still_restore.take() {
                    let changed = self.settings.visual_style != style || self.materials_on != mat;
                    self.settings.visual_style = style;
                    self.materials_on = mat;
                    self.selected = sel;
                    if changed {
                        self.materials_changed();
                    }
                    self.colors_dirty = true;
                }
                match r {
                    Ok(f) => {
                        let f = if self.render.supersample { f.downscale(2) } else { f };
                        match crate::video::save_png(std::path::Path::new(&p), &f) {
                            Ok(()) => self.toast(format!("Image saved: {} ({}×{})", crate::file_name(&p), f.w, f.h)),
                            Err(e) => self.error(e),
                        }
                    }
                    Err(e) => self.error(e),
                }
            } else if let Some(j) = self.render_job.as_mut() {
                j.waiting = false;
                match r {
                    Ok(f) => {
                        if let Some(tx) = &j.tx {
                            let _ = tx.send(f);
                        }
                        j.frame += 1;
                    }
                    Err(e) => {
                        j.frames = j.frame;
                        j.tx = None;
                        self.error(e);
                    }
                }
            }
        }
        let Some(j) = self.render_job.as_ref() else { return };
        if j.frame >= j.frames {
            // toate cadrele trimise: inchide encoderul si asteapta rezultatul
            if let Some(j) = self.render_job.as_mut() {
                j.tx = None;
            }
            let res = match self.render_job.as_ref().map(|j| j.done.try_recv()) {
                Some(Ok(r)) => Some(r),
                // firul encoderului a disparut fara rezultat: exportul se inchide cu eroare, vederea revine
                Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => Some(Err("The video encoder stopped unexpectedly.".to_string())),
                _ => None,
            };
            if let Some(r) = res {
                let j = self.render_job.take().unwrap();
                self.selected = j.saved_sel;
                if let Some((style, mat)) = j.saved_style {
                    self.settings.visual_style = style;
                    self.materials_on = mat;
                    self.materials_changed();
                }
                self.anim_sim_active = false;
                self.colors_dirty = true;
                match r {
                    Ok(()) => {
                        let secs = j.started.elapsed().as_secs_f32();
                        let target = if self.render.format == VideoFormat::PngSequence { crate::video::png_dir(&j.path) } else { j.path.clone() };
                        self.toast(format!("Animation exported: {} ({} frames in {:.0} s)", target.display(), j.frame, secs));
                    }
                    Err(e) => self.error(e),
                }
            }
            return;
        }
        if !j.waiting {
            let (anim, t, w, h) = (j.anim, j.frame as f32 / j.fps as f32, j.w, j.h);
            if let Some(c) = self.animations.get(anim).and_then(|a| a.sample(t)) {
                self.cam = c;
            }
            self.anim_time = t;
            self.sim_sync(anim, t);
            self.anim_objects(anim, t);
            self.pending_capture = Some((w, h));
            if let Some(j) = self.render_job.as_mut() {
                j.waiting = true;
            }
        }
    }
}
