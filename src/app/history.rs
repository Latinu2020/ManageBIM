//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    /// ViewCube Home: saved home view, otherwise the whole model in isometric
    pub fn go_home(&mut self) {
        match self.home {
            Some(h) => {
                let mut t = h;
                t.ortho = self.cam.ortho;
                self.fly_to(t);
            }
            None => {
                let bb = self.scene_aabb();
                let mut t = self.cam;
                t.set_view(std::f32::consts::FRAC_PI_4, -0.6, &bb);
                self.fly_to(t);
            }
        }
    }

    pub fn open_panel(&mut self, p: Panel) {
        self.settings.layout.open(p);
    }

    pub fn panel_shown(&self, p: Panel) -> bool {
        self.settings.layout.shown(p)
    }

    // ------------------------------------------------------------------ viewpoints
    pub fn save_viewpoint(&mut self) {
        let n = self.viewpoints.len() + 1;
        let mut hidden: Vec<ElemId> = self.hidden.iter().copied().collect();
        hidden.sort();
        let selected: Vec<ElemId> = self.selected.iter().copied().collect();
        self.viewpoints.push(Viewpoint {
            name: format!("Viewpoint {n}"),
            cam: self.cam,
            section: self.section.clone(),
            hidden,
            comment: String::new(),
            redlines: Vec::new(),
            issue: Default::default(),
            comments: Vec::new(),
            selected,
            measures: self.measures.clone(),
            // viewpoint-urile noi intra in folderul ales in panou
            folder: self.vp_folder_cur.clone(),
        });
        self.active_vp = Some(self.viewpoints.len() - 1);
        self.vp_sel = self.viewpoints.len() - 1;
    }

    /// redlines are shown only while the camera is still at the viewpoint they belong to
    pub fn vp_matches(&self, i: usize) -> bool {
        match self.viewpoints.get(i) {
            Some(v) => {
                let a = &v.cam;
                let b = &self.cam;
                (a.eye() - b.eye()).length() < 1e-3 * (1.0 + a.focus) && (a.yaw - b.yaw).abs() < 1e-4 && (a.pitch - b.pitch).abs() < 1e-4 && a.ortho == b.ortho && (a.focus - b.focus).abs() < 1e-3 * (1.0 + a.focus)
            }
            None => false,
        }
    }

    pub fn guid_of(&self, id: ElemId) -> Option<String> {
        self.elem(id).map(|(_, e)| e.guid.clone()).filter(|g| g.len() == 22)
    }

    pub fn report_item(&self, i: usize, image: Vec<u8>, w: u32, h: u32) -> crate::markup::ReportItem {
        let v = &self.viewpoints[i];
        let c = v.cam;
        let eye = self.world(c.eye());
        let d = c.fwd();
        let u = c.up();
        crate::markup::ReportItem {
            index: i + 1,
            name: v.name.clone(),
            description: v.comment.clone(),
            issue: v.issue.clone(),
            comments: v.comments.clone(),
            redlines: v.redlines.len(),
            eye: eye.to_array(),
            dir: [d.x as f64, d.y as f64, d.z as f64],
            up: [u.x as f64, u.y as f64, u.z as f64],
            fov_deg: c.fov_deg,
            ortho_height: if c.ortho { Some(2.0 * c.ortho_half() as f64) } else { None },
            selected_guids: v.selected.iter().filter_map(|id| self.guid_of(*id)).collect(),
            hidden_guids: v.hidden.iter().filter_map(|id| self.guid_of(*id)).take(5000).collect(),
            image,
            image_w: w,
            image_h: h,
        }
    }

    /// viewpoint-ul intra in exportul curent? (toate, sau doar folderul ales si subfolderele lui)
    pub fn vp_in_scope(&self, i: usize) -> bool {
        let Some(v) = self.viewpoints.get(i) else { return false };
        match &self.vp_filter {
            None => true,
            Some(f) => v.folder == *f || v.folder.starts_with(&format!("{f}/")),
        }
    }

    /// toate folderele (cele cu viewpoints si cele create goale), sortate
    pub fn vp_all_folders(&self) -> Vec<String> {
        let mut f: Vec<String> = self.vp_folders.clone();
        for v in &self.viewpoints {
            // si folderele parinte ("A/B" -> "A")
            let mut p = String::new();
            for seg in v.folder.split('/').filter(|s| !s.is_empty()) {
                if !p.is_empty() {
                    p.push('/');
                }
                p.push_str(seg);
                if !f.contains(&p) {
                    f.push(p.clone());
                }
            }
        }
        f.sort();
        f.dedup();
        f
    }

    pub fn start_report(&mut self, kind: u8, path: &str) {
        if self.viewpoints.is_empty() {
            self.error("No viewpoints to export - save a viewpoint (with markups) first.");
            return;
        }
        let list: Vec<usize> = (0..self.viewpoints.len()).filter(|i| self.vp_in_scope(*i)).collect();
        if list.is_empty() {
            self.vp_filter = None;
            self.error("The folder has no viewpoints.");
            return;
        }
        let hidden = self.hidden.clone();
        self.report_job = Some(ReportJob { kind, path: path.to_string(), idx: 0, phase: 0, wait: 0, items: Vec::new(), saved_cam: self.cam, saved_section: self.section.clone(), saved_hidden: hidden, saved_vp: self.active_vp, list });
        self.vp_filter = None;
        self.toast("Capturing viewpoints for the report…");
    }

    pub fn finish_report(&mut self) {
        let Some(job) = self.report_job.take() else { return };
        self.cam = job.saved_cam;
        self.section = job.saved_section.clone();
        self.hidden = job.saved_hidden.clone();
        self.active_vp = job.saved_vp;
        self.colors_dirty = true;
        let project = self.project_path.as_ref().map(|p| crate::file_name(p)).unwrap_or_else(|| "ManageBIM project".into());
        let mut path = job.path.clone();
        let ext = match job.kind {
            0 => ".html",
            1 => ".xlsx",
            3 => ".pdf",
            _ => ".bcf",
        };
        if !path.to_lowercase().ends_with(ext) && !(job.kind == 2 && path.to_lowercase().ends_with(".bcfzip")) {
            path.push_str(ext);
        }
        let data: Vec<u8> = match job.kind {
            0 => crate::markup::html_report(&project, &job.items).into_bytes(),
            1 => {
                let thumbs: Vec<(Vec<u8>, u32, u32)> = job
                    .items
                    .iter()
                    .map(|it| match png_rgb(&it.image) {
                        Some((rgb, w, h)) => {
                            let (t, tw, th) = crate::markup::downscale_rgb(&rgb, w, h, 240);
                            (crate::markup::encode_png(&t, tw, th), tw, th)
                        }
                        None => (Vec::new(), 0, 0),
                    })
                    .collect();
                crate::markup::xlsx_report(&project, &job.items, &thumbs)
            }
            3 => crate::pdf::sheets(&project, &job.items, &now_string(), &self.title_block),
            _ => crate::markup::bcf_zip(&project, &job.items),
        };
        match std::fs::write(&path, data) {
            Ok(()) => self.toast(format!("{} viewpoints exported to {}", job.items.len(), crate::file_name(&path))),
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }

    pub fn restore_viewpoint(&mut self, i: usize) {
        self.cam_anim = None;
        self.active_vp = Some(i);
        self.vp_sel = i;
        if let Some(v) = self.viewpoints.get(i).cloned() {
            self.cam = v.cam;
            self.section = v.section;
            self.hidden = v.hidden.into_iter().collect();
            // cotele salvate cu viewpoint-ul (viewpoint-urile vechi, fara cote, pastreaza masuratorile curente)
            if !v.measures.is_empty() {
                self.measures = v.measures;
            }
            self.clash_focus = None;
            self.colors_dirty = true;
        }
    }

    /// semnatura ieftina a starii vizualizarii (selectie, ascunse, sectiune, camera oprita)
    pub(super) fn view_sig(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mix = |acc: u64, id: &ElemId| acc ^ ((id.m as u64) << 32 | id.e as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let sel = self.selected.iter().fold(0u64, mix);
        let hid = self.hidden.iter().fold(0u64, mix);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (self.selected.len(), sel, self.hidden.len(), hid).hash(&mut h);
        (self.section.enabled, self.section.mode == SectionMode::Box).hash(&mut h);
        for v in self.section.bmin.iter().chain(self.section.bmax.iter()) {
            ((v * 100.0).round() as i64).hash(&mut h);
        }
        for p in &self.section.planes {
            (p.enabled, p.dir, (p.offset * 100.0).round() as i64).hash(&mut h);
        }
        let e = self.cam.eye();
        for v in [e.x, e.y, e.z, self.cam.yaw * 100.0, self.cam.pitch * 100.0, self.cam.focus] {
            ((v * 20.0).round() as i64).hash(&mut h);
        }
        self.cam.ortho.hash(&mut h);
        h.finish()
    }

    /// Apelat la fiecare cadru: cand starea s-a schimbat si camera s-a oprit, o pune in istoric.
    pub fn history_tick(&mut self, pointer_down: bool) {
        if pointer_down || self.cam_anim.is_some() || self.lod_moved.elapsed().as_secs_f32() < 0.35 {
            return;
        }
        let sig = self.view_sig();
        if sig == self.hist_sig {
            return;
        }
        self.hist_sig = sig;
        let snap = ViewSnap { selected: self.selected.clone(), hidden: Arc::new(self.hidden.clone()), section: self.section.clone(), cam: self.cam };
        self.hist.truncate(self.hist_pos + 1);
        self.hist.push(snap);
        if self.hist.len() > 60 {
            self.hist.remove(0);
        }
        self.hist_pos = self.hist.len() - 1;
    }

    pub fn can_undo(&self) -> bool {
        self.hist_pos > 0 && !self.hist.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        self.hist_pos + 1 < self.hist.len()
    }

    /// Ctrl+Z: starea anterioara (selectie, vizibilitate, sectiune, camera)
    pub fn undo_view(&mut self) {
        if self.can_undo() {
            self.hist_pos -= 1;
            self.restore_snap();
        }
    }

    /// Ctrl+Y: inainte
    pub fn redo_view(&mut self) {
        if self.can_redo() {
            self.hist_pos += 1;
            self.restore_snap();
        }
    }

    pub(super) fn restore_snap(&mut self) {
        let Some(s) = self.hist.get(self.hist_pos).cloned() else { return };
        self.selected = s.selected;
        self.primary = self.selected.iter().next().copied();
        self.hidden = (*s.hidden).clone();
        self.section = s.section;
        self.fly_to(s.cam);
        self.colors_dirty = true;
        // starea refacuta nu se mai inregistreaza o data
        let mut tmp = self.cam;
        std::mem::swap(&mut tmp, &mut self.cam);
        self.cam = s.cam;
        self.hist_sig = self.view_sig();
        self.cam = tmp;
    }

    /// Shaded / Realistic / Concept / Wireframe (pastrat in setari).
    pub fn set_visual_style(&mut self, s: crate::render::VisualStyle) {
        self.settings.visual_style = s;
        self.settings.save();
        if s == crate::render::VisualStyle::Realistic {
            // realist = cu materiale / texturi; fara atribuiri inca: automat dupa tipul IFC (pereti, sticla, beton...)
            if self.mat_assign.is_empty() && self.loaded_models().next().is_some() {
                self.auto_materials();
            } else if !self.materials_on {
                self.materials_on = true;
                self.materials_changed();
            }
        }
        self.toast(format!("Visual style: {}", s.label()));
    }
}
