//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    // ------------------------------------------------------------------ culori
    // ------------------------------------------------------------------ materials
    /// element -> material index, resolved from the assignment rules (later rules win)
    pub fn material_map(&mut self) -> &HashMap<ElemId, u16> {
        if self.mat_cache.is_none() {
            let mut map: HashMap<ElemId, u16> = HashMap::new();
            for a in self.mat_assign.iter().filter(|a| a.enabled) {
                let Some(mi) = self.materials.iter().position(|m| m.name == a.material) else { continue };
                let ids: Vec<ElemId> = match &a.target {
                    MatTarget::Set(n) => self.sets.iter().find(|s| &s.name == n).map(|s| self.resolve_set(s)).unwrap_or_default(),
                    MatTarget::Items(v) => v.iter().copied().filter(|i| self.elem(*i).is_some()).collect(),
                    MatTarget::Type(t) => {
                        let t = t.to_lowercase();
                        self.all_ids().into_iter().filter(|i| self.elem(*i).map_or(false, |(_, e)| e.ifc_type.to_lowercase().contains(&t))).collect()
                    }
                };
                for id in ids {
                    map.insert(id, mi as u16);
                }
            }
            self.mat_cache = Some(map);
        }
        self.mat_cache.as_ref().unwrap()
    }

    /// GPU layer of an image texture (loaded on first use).
    pub fn texture_layer(&mut self, path: &str) -> Option<u8> {
        if path.is_empty() || self.tex_failed.contains(path) {
            return None;
        }
        if let Some(i) = self.tex_layers.iter().position(|t| t.0 == path) {
            return Some(i as u8);
        }
        if self.tex_layers.len() >= crate::materials::MAX_TEXTURES {
            self.error("Too many image textures (max 64).");
            self.tex_failed.insert(path.to_string());
            return None;
        }
        // cauta si langa proiect daca fisierul a fost mutat
        let mut real = path.to_string();
        if !std::path::Path::new(&real).exists() {
            if let Some(d) = self.project_path.as_ref().and_then(|p| std::path::Path::new(p).parent().map(|d| d.to_path_buf())) {
                let alt = d.join(crate::file_name(path));
                if alt.exists() {
                    real = alt.to_string_lossy().to_string();
                }
            }
        }
        match crate::materials::load_texture(&real) {
            Ok(px) => {
                self.tex_layers.push((path.to_string(), px));
                let all: Vec<Vec<u8>> = self.tex_layers.iter().map(|t| t.1.clone()).collect();
                self.renderer.lock().unwrap_or_else(|e| e.into_inner()).pending.push(Upload::Textures(all));
                Some((self.tex_layers.len() - 1) as u8)
            }
            Err(e) => {
                self.error(e);
                self.tex_failed.insert(path.to_string());
                None
            }
        }
    }

    /// One material per image file (named after the file).
    pub fn import_texture_materials(&mut self, paths: &[String]) {
        let mut n = 0;
        for p in paths {
            if crate::materials::load_texture(p).is_err() {
                self.error(format!("Not a PNG / JPG image: {}", crate::file_name(p)));
                continue;
            }
            let name = std::path::Path::new(p).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Texture".into());
            let mut m = crate::materials::library()[4].clone();
            m.name = name;
            m.color = [255, 255, 255];
            m.texture = p.clone();
            m.scale = 1.0;
            self.materials.push(m);
            n += 1;
        }
        if n > 0 {
            self.mat_sel = self.materials.len() - 1;
            self.toast(format!("{n} texture materials added - set the real size of the image in \"Texture size\"."));
        }
    }

    /// Pachete PBR (.zip descarcate de pe freepbr.com, ambientCG, Poly Haven... sau foldere dezarhivate) ->
    /// materiale noi, salvate permanent in biblioteca ta (%APPDATA%\ManageBIM\Materials).
    pub fn import_pbr_packs(&mut self, paths: &[String]) {
        let Some(root) = crate::materials::user_library_dir() else { return self.error("APPDATA is not set - cannot create the material library.") };
        let mut added: Vec<crate::materials::Material> = Vec::new();
        for p in paths {
            match crate::materials::import_pbr(p, &root) {
                Ok(m) => added.push(m),
                Err(e) => self.error(e),
            }
        }
        if added.is_empty() {
            return;
        }
        if let Err(e) = crate::materials::save_to_user_library(&added) {
            self.error(format!("The materials were added to the project but not saved to the library: {e}"));
        }
        for m in &added {
            match self.materials.iter_mut().find(|x| x.name == m.name) {
                Some(x) => *x = m.clone(),
                None => self.materials.push(m.clone()),
            }
        }
        let last = added.last().map(|m| m.name.clone()).unwrap_or_default();
        self.mat_sel = self.materials.iter().position(|m| m.name == last).unwrap_or(0);
        self.materials_changed();
        self.toast(format!("{} PBR materials added to your library ({}) - available in every project. Set the real size in \"Texture size\".", added.len(), added.iter().map(|m| m.name.clone()).collect::<Vec<_>>().join(", ")));
    }

    /// Salveaza materialul ales in biblioteca ta (disponibil in orice proiect).
    pub fn save_material_to_library(&mut self, i: usize) {
        let Some(m) = self.materials.get(i).cloned() else { return };
        match crate::materials::save_to_user_library(std::slice::from_ref(&m)) {
            Ok(_) => self.toast(format!("\"{}\" saved to your material library.", m.name)),
            Err(e) => self.error(format!("Cannot save the material library: {e}")),
        }
    }

    pub fn materials_changed(&mut self) {
        self.mat_cache = None;
        self.colors_dirty = true;
    }

    pub fn assign_material(&mut self, target: MatTarget, material: &str) {
        // aceeasi tinta: inlocuieste materialul
        if let Some(a) = self.mat_assign.iter_mut().find(|a| a.target == target) {
            a.material = material.to_string();
        } else {
            self.mat_assign.push(MatAssign { target, material: material.to_string(), enabled: true });
        }
        self.materials_on = true;
        self.materials_changed();
    }

    pub fn auto_materials(&mut self) {
        let mut n = 0;
        for (kw, m) in crate::materials::AUTO_RULES {
            let present = self.cache_types.iter().any(|t| t.to_lowercase().contains(&kw.to_lowercase()));
            if present && self.materials.iter().any(|x| x.name == m) && !self.mat_assign.iter().any(|a| a.target == MatTarget::Type(kw.to_string())) {
                self.mat_assign.insert(n, MatAssign { target: MatTarget::Type(kw.to_string()), material: m.to_string(), enabled: true });
                n += 1;
            }
        }
        self.materials_on = true;
        self.materials_changed();
        self.toast(format!("{n} material rules added by IFC type (selection / set rules below them still win)."));
    }

    pub fn rebuild_colors(&mut self) {
        if !self.colors_dirty {
            return;
        }
        self.colors_dirty = false;
        if self.materials_on {
            self.material_map(); // umple mat_cache daca e nevoie; mai jos doar il imprumutam
        }
        let profile_on = self.profile.active && !self.profile.rules.is_empty();
        if profile_on {
            self.profile_map();
        }
        let mat_gpu: Vec<[u8; 4]> = (0..self.materials.len())
            .map(|i| {
                let t = self.materials[i].texture.clone();
                let layer = if t.is_empty() { None } else { self.texture_layer(&t) };
                self.materials[i].gpu(layer)
            })
            .collect();
        let roles = self.clash_roles();
        let ghost = !roles.is_empty();
        let others = self.clash_others;
        // culorile clash-ului (Settings in fila Results): A, B si cele „palide” ale celorlalte clash-uri deschise
        let (ca, cb, dim_a) = (self.settings.clash_ui.col_a, self.settings.clash_ui.col_b, self.settings.clash_ui.dim_alpha);
        let pale = |c: [u8; 3]| [((c[0] as u16 + 255) / 2) as u8, ((c[1] as u16 + 255) / 2) as u8, ((c[2] as u16 + 255) / 2) as u8, 235];
        let (role_a, role_b, role_pa, role_pb) = ([ca[0], ca[1], ca[2], 255], [cb[0], cb[1], cb[2], 255], pale(ca), pale(cb));
        let sim = self.sim_view();
        let empty = HashMap::new();
        let empty_p: HashMap<ElemId, u16> = HashMap::new();
        let matmap = if self.materials_on { self.mat_cache.as_ref().unwrap_or(&empty) } else { &empty };
        let (hidden_set, overrides, sim_cache, selected, materials) = (&self.hidden, &self.overrides, &self.sim_cache, &self.selected, &self.materials);
        let sim_only_linked = self.sim_only_linked && !sim_cache.is_empty();
        let anim_obj = &self.anim_obj;
        let (pmap, prules, punmatched) = (self.profile_map.as_ref().unwrap_or(&empty_p), &self.profile.rules, self.profile.unmatched);
        let cmp = self.compare.as_ref().filter(|_| self.compare_on);
        // aspectul unui element: override manual > profil > material; simularea 4D si clash-ul au prioritate
        let look_of = |id: ElemId| -> Look {
            let mut hidden = hidden_set.contains(&id);
            let mut ovr: Option<[u8; 4]> = overrides.get(&id).copied();
            if profile_on && ovr.is_none() {
                match pmap.get(&id).and_then(|r| prules.get(*r as usize)) {
                    Some(r) if r.visible => ovr = Some(r.color),
                    Some(_) => hidden = true,
                    None => match punmatched {
                        crate::profiler::Unmatched::Keep => {}
                        crate::profiler::Unmatched::Ghost => ovr = Some(crate::profiler::GHOST),
                        crate::profiler::Unmatched::Hide => hidden = true,
                    },
                }
            }
            // Compare: culoarea dupa tipul schimbarii; perechea veche (A) a unui element potrivit se ascunde
            if let Some(c) = cmp {
                if c.matched_a.contains_key(&id) {
                    hidden = true;
                } else {
                    ovr = Some(c.map.get(&id).map(|d| d.color()).unwrap_or(crate::profiler::GHOST));
                }
            }
            if sim {
                match sim_cache.get(&id) {
                    Some(SimState::Hidden) => hidden = true,
                    Some(SimState::Highlight(c)) => ovr = Some(*c),
                    None if sim_only_linked => hidden = true,
                    _ => {}
                }
            }
            // Animator: culoarea / transparenta / vizibilitatea seturilor animate
            if let Some((c, vis)) = anim_obj.get(&id) {
                if !*vis {
                    hidden = true;
                } else {
                    ovr = Some(*c);
                }
            }
            if ghost {
                match roles.get(&id) {
                    Some(1) => ovr = Some(role_a),
                    Some(2) => ovr = Some(role_b),
                    Some(3) => ovr = Some(role_pa),
                    Some(4) => ovr = Some(role_pb),
                    _ => match others {
                        Others::Dim => {
                            if !hidden {
                                ovr = Some([150, 150, 155, dim_a]);
                            }
                        }
                        Others::Hide => hidden = true,
                        Others::Normal => {}
                    },
                }
            }
            let sel = selected.contains(&id) && !ghost;
            let (col, gpu) = match (ovr, matmap.get(&id)) {
                (None, Some(k)) => (Some(materials[*k as usize].rgba()), mat_gpu[*k as usize]),
                _ => (ovr, [0u8; 4]),
            };
            Look { col, gpu, sel, hidden }
        };

        let tint = (self.settings.sel_rgb, self.settings.sel_mix.clamp(0.2, 1.0));
        let mut ups = Vec::new();
        let mut new_looks: HashMap<usize, Vec<Look>> = HashMap::new();
        for (mi, m) in self.loaded_models() {
            let looks: Vec<Look> = (0..m.elements.len() as u32).map(|e| look_of(ElemId { m: mi as u32, e })).collect();
            // culorile de pe GPU corespund ultimelor "looks" trimise; None = trebuie trimis tot
            let prev = self.looks.get(&mi).filter(|p| p.len() == looks.len());
            let changed: Vec<usize> = match prev {
                Some(p) => (0..looks.len()).filter(|&i| p[i] != looks[i]).collect(),
                None => (0..looks.len()).collect(),
            };
            let total_tris = m.tri_color.len();
            let changed_tris: usize = changed.iter().map(|&i| m.elements[i].tri_count as usize).sum();
            if prev.is_none() || changed_tris * 3 > total_tris {
                // multe schimbari (sau prima data): tot bufferul, umplut pe toate nucleele
                ups.push(full_colors(mi as u32, m, &looks, tint));
            } else {
                // putine schimbari (ex. un click de selectie): doar elementele afectate, grupate in intervale continue
                let mut k = 0;
                while k < changed.len() {
                    let mut j = k;
                    while j + 1 < changed.len() && changed[j + 1] == changed[j] + 1 {
                        j += 1;
                    }
                    let (a, b) = (&m.elements[changed[k]], &m.elements[changed[j]]);
                    let (t0, t1) = (a.tri_start as usize, (b.tri_start + b.tri_count) as usize);
                    let mut col = vec![0u8; (t1 - t0) * 12];
                    let mut mat = vec![0u8; (t1 - t0) * 12];
                    for &ei in &changed[k..=j] {
                        let e = &m.elements[ei];
                        let (s, n) = (e.tri_start as usize - t0, e.tri_count as usize);
                        fill_look(&looks[ei], &m.tri_color[e.tri_start as usize..e.tri_start as usize + n], &mut col[s * 12..(s + n) * 12], &mut mat[s * 12..(s + n) * 12], tint);
                    }
                    ups.push(Upload::ColorRange { slot: mi as u32, first: (t0 * 3) as u32, col, mat });
                    k = j + 1;
                }
            }
            new_looks.insert(mi, looks);
        }
        self.looks = new_looks;
        if !ups.is_empty() {
            self.renderer.lock().unwrap_or_else(|e| e.into_inner()).pending.extend(ups);
        }
    }

    // ------------------------------------------------------------------ picking
    pub fn pick(&self, o: Vec3, d: Vec3) -> Option<(ElemId, f32, Vec3, [Vec3; 3])> {
        let planes = self.section.gpu_planes();
        let accept = |p: Vec3| planes.iter().all(|pl| pl.truncate().dot(p) <= pl.w + 1e-4);
        let inv = Vec3::new(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
        let mut best: Option<(ElemId, f32, Vec3, [Vec3; 3])> = None;
        // in Clash Detective (dim / hide other) only the clashing items can be picked
        let roles = if self.clash_others != Others::Normal { Some(self.clash_roles()).filter(|r| !r.is_empty()) } else { None };
        for (mi, m) in self.loaded_models() {
            if !m.aabb.ray(o, inv, f32::INFINITY) {
                continue;
            }
            for (ei, e) in m.elements.iter().enumerate() {
                let tmax = best.as_ref().map_or(f32::INFINITY, |b| b.1);
                if !e.aabb.ray(o, inv, tmax) {
                    continue;
                }
                let id = ElemId { m: mi as u32, e: ei as u32 };
                if !self.is_visible(id) {
                    continue;
                }
                if let Some(r) = roles.as_ref() {
                    if !r.contains_key(&id) {
                        continue;
                    }
                }
                let tris = m.tris(e);
                if let Some((t, ti)) = e.bvh.raycast(tris, o, d, tmax, &accept) {
                    let k = ti as usize * 3;
                    best = Some((id, t, o + d * t, [tris[k], tris[k + 1], tris[k + 2]]));
                }
            }
        }
        best
    }

    /// Ce se deseneaza in cadrul curent, pe model: elementele din campul vizual, neascunse, in section box;
    /// in timpul miscarii camerei se sar elementele mai mici de ~2 pixeli (cand camera sta, se deseneaza tot).
    /// Pasul transparent primeste doar elementele cu ceva transparent. Intervalele apropiate se unesc
    /// (varfurile in plus sunt oricum taiate / ascunse de shader), deci rezultatul vizual nu se schimba.
    pub fn draw_lists(&mut self, p: &FrameParams, vp_h: f32) -> HashMap<u32, crate::render::DrawList> {
        // miscarea camerei (pentru simplificarea temporara)
        let c = self.cam;
        let moved = c.eye != self.lod_cam.eye || c.yaw != self.lod_cam.yaw || c.pitch != self.lod_cam.pitch || c.focus != self.lod_cam.focus || c.fov_deg != self.lod_cam.fov_deg;
        if moved {
            self.lod_cam = c;
            self.lod_moved = Instant::now();
        }
        let moving = self.lod_moved.elapsed().as_secs_f32() < 0.3;
        // planurile frustumului (Gribb-Hartmann) din matricea vedere-proiectie
        let m = p.view_proj;
        let (r0, r1, r2, r3) = (m.row(0), m.row(1), m.row(2), m.row(3));
        let planes = [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r3 + r2, r3 - r2];
        let inside = |bb: &Aabb| -> bool {
            planes.iter().all(|pl| {
                let n = pl.truncate();
                let v = Vec3::new(if n.x >= 0.0 { bb.max.x } else { bb.min.x }, if n.y >= 0.0 { bb.max.y } else { bb.min.y }, if n.z >= 0.0 { bb.max.z } else { bb.min.z });
                n.dot(v) + pl.w >= 0.0
            })
        };
        let section_box = (self.section.enabled && self.section.mode == SectionMode::Box).then(|| Aabb { min: Vec3::from(self.section.bmin), max: Vec3::from(self.section.bmax) });
        let tanh = (c.fov() * 0.5).tan().max(1e-3);
        // LOD adaptiv: in miscare pragul (pixeli) creste cand FPS scade si revine cand FPS e bun;
        // camera oprita = tot modelul, mai putin obiectele sub un pixel (oricum invizibile)
        if moving {
            if self.fps > 0.0 && self.fps < 28.0 {
                self.lod_px = (self.lod_px * 1.25).min(24.0);
            } else if self.fps > 50.0 {
                self.lod_px = (self.lod_px / 1.15).max(2.0);
            }
        }
        let min_px = if moving { self.lod_px } else { 0.6 };
        let eye = c.eye();
        let mut out = HashMap::new();
        let slots: Vec<usize> = self.loaded_models().map(|(i, _)| i).collect();
        for slot in slots {
            let Some(model) = self.models[slot].clone() else { continue };
            // elementele cu triunghiuri transparente in culorile din fisier (calculat o data per model)
            let key = Arc::as_ptr(&model) as usize;
            let need = self.alpha_cache.get(&slot).map(|(k, _)| *k != key).unwrap_or(true);
            if need {
                let flags: Vec<bool> = model.elements.iter().map(|e| model.tri_color[e.tri_start as usize..(e.tri_start + e.tri_count) as usize].iter().any(|c| c[3] < 255)).collect();
                self.alpha_cache.insert(slot, (key, flags));
            }
            let base_alpha = &self.alpha_cache[&slot].1;
            let looks = self.looks.get(&slot);
            let mut all: Vec<(i32, i32)> = Vec::new();
            let mut transp: Vec<(i32, i32)> = Vec::new();
            let push = |v: &mut Vec<(i32, i32)>, first: i32, count: i32| match v.last_mut() {
                Some(l) if first - (l.0 + l.1) <= 600 => l.1 = first + count - l.0,
                _ => v.push((first, count)),
            };
            for (ei, e) in model.elements.iter().enumerate() {
                if e.tri_count == 0 {
                    continue;
                }
                let look = looks.and_then(|l| l.get(ei));
                if look.map(|l| l.hidden).unwrap_or(false) {
                    continue;
                }
                let bb = &e.aabb;
                if !bb.is_valid() || !inside(bb) {
                    continue;
                }
                if let Some(sb) = &section_box {
                    if !bb.overlaps(sb, 0.0) {
                        continue;
                    }
                }
                if min_px > 0.0 {
                    let r = bb.size().length() * 0.5;
                    let px = if c.ortho { r / c.ortho_half().max(1e-3) * vp_h * 0.5 } else { r / ((bb.center() - eye).length().max(1e-3) * tanh) * vp_h * 0.5 };
                    if px < min_px {
                        continue;
                    }
                }
                let (first, count) = (e.tri_start as i32 * 3, e.tri_count as i32 * 3);
                push(&mut all, first, count);
                let t = match look.and_then(|l| l.col) {
                    Some(col) => col[3] < 255,
                    None => base_alpha.get(ei).copied().unwrap_or(false),
                };
                if t {
                    push(&mut transp, first, count);
                }
            }
            // prea multe intervale -> le unim mai larg (apelurile de desenare costa mai mult decat varfurile in plus)
            for v in [&mut all, &mut transp] {
                let mut gap = 4000;
                while v.len() > 3000 {
                    let mut merged: Vec<(i32, i32)> = Vec::with_capacity(v.len() / 2);
                    for &(f, n) in v.iter() {
                        match merged.last_mut() {
                            Some(l) if f - (l.0 + l.1) <= gap => l.1 = f + n - l.0,
                            _ => merged.push((f, n)),
                        }
                    }
                    *v = merged;
                    gap *= 4;
                }
            }
            // nor de puncte: scanarile vizibile, primele N% puncte (amestecate la citire); in miscare mai putine
            let mut points = Vec::new();
            if let Some(cl) = &model.cloud {
                let dens = self.cloud_density.clamp(0.01, 1.0) * if moving { 0.35 } else { 1.0 };
                for &(ei, start, n) in &cl.scans {
                    if looks.and_then(|l| l.get(ei as usize)).map(|l| l.hidden).unwrap_or(false) {
                        continue;
                    }
                    if let Some(e) = model.elements.get(ei as usize) {
                        if e.aabb.is_valid() && (!inside(&e.aabb) || section_box.as_ref().map_or(false, |sb| !e.aabb.overlaps(sb, 0.0))) {
                            continue;
                        }
                    }
                    let k = ((n as f32 * dens).ceil() as i32).clamp(0, n as i32);
                    if k > 0 {
                        points.push((start as i32, k));
                    }
                }
            }
            out.insert(slot as u32, crate::render::DrawList { all, transp, points });
        }
        out
    }

    /// camera se misca acum (desenarea simplificata e activa) - pentru redesenarea completa dupa oprire
    pub fn lod_moving(&self) -> bool {
        self.lod_moved.elapsed().as_secs_f32() < 0.35
    }

    /// triunghiurile tuturor modelelor incarcate (scenele foarte mari sar umbrele cat timp camera se misca)
    pub fn scene_tris(&self) -> usize {
        self.loaded_models().map(|(_, m)| m.pos.len() / 3).sum()
    }

    /// Sfera care cuprinde modelele vizibile (fara nori de puncte), pentru harta de umbre.
    pub fn shadow_sphere(&self) -> glam::Vec4 {
        let mut bb = crate::model::Aabb::EMPTY;
        for (_, m) in self.loaded_models() {
            if m.cloud.is_none() && m.aabb.is_valid() {
                bb.union(&m.aabb);
            }
        }
        if !bb.is_valid() {
            return glam::Vec4::ZERO;
        }
        let c = bb.center();
        glam::Vec4::new(c.x, c.y, c.z, (bb.size().length() * 0.5).max(1.0))
    }

    pub fn frame_params(&self, aspect: f32) -> FrameParams {
        let bb = self.scene_aabb();
        let proj = self.cam.proj(aspect, &bb);
        let vp = proj * self.cam.view();
        let pl = self.section.gpu_planes();
        let mut planes = [Vec4::ZERO; 6];
        for (i, p) in pl.iter().take(6).enumerate() {
            planes[i] = *p;
        }
        FrameParams { view_proj: vp, eye: self.cam.eye(), planes, nplanes: pl.len().min(6) as i32, sun: self.render.sun_dir(), light: self.render.light(), point_size: self.point_size, caps: self.section_caps, ao: if self.settings.visual_style == crate::render::VisualStyle::Wireframe || self.settings.visual_style == crate::render::VisualStyle::Hidden { 0.0 } else { self.render.ao }, edges: if self.settings.visual_style == crate::render::VisualStyle::Concept || self.settings.visual_style == crate::render::VisualStyle::Hidden { self.render.edges.max(0.9) } else { self.render.edges }, style: self.settings.visual_style, depth_k: [proj.z_axis.z, proj.w_axis.z, if self.cam.ortho { 1.0 } else { 0.0 }, proj.y_axis.y], shadows: self.render.shadows && self.settings.visual_style != crate::render::VisualStyle::Wireframe && self.settings.visual_style != crate::render::VisualStyle::Hidden && !(self.lod_moving() && self.scene_tris() > 2_000_000), scene: self.shadow_sphere() }
    }
}
