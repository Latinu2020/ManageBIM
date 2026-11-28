//! Section of `impl App` (see the parent file for the rest).
use super::*;
use crate::compare::heat_color as heat;

impl App {
    // ------------------------------------------------------------------ colours
    // ------------------------------------------------------------------ materials
    /// element -> material index, resolved from the assignment rules (later rules win)
    pub fn material_map(&mut self) -> &HashMap<ElemId, u16> {
        if self.mats.cache.is_none() {
            let mut map: HashMap<ElemId, u16> = HashMap::new();
            for a in self.mats.assign.iter().filter(|a| a.enabled) {
                let Some(mi) = self.materials.iter().position(|m| m.name == a.material) else { continue };
                let ids: Vec<ElemId> = match &a.target {
                    MatTarget::Set(n) => self.sets.iter().find(|s| &s.name == n).map(|s| self.resolve_set(s)).unwrap_or_default(),
                    MatTarget::Items(v) => v.iter().copied().filter(|i| self.elem(*i).is_some()).collect(),
                    MatTarget::Type(t) => {
                        let t = t.to_lowercase();
                        self.all_ids().into_iter().filter(|i| self.elem(*i).is_some_and(|(_, e)| e.ifc_type.to_lowercase().contains(&t))).collect()
                    }
                };
                for id in ids {
                    map.insert(id, mi as u16);
                }
            }
            self.mats.cache = Some(map);
        }
        self.mats.cache.get_or_insert_with(HashMap::new)
    }

    /// The real path of a texture (if the file was moved, it looks for it next to the project).
    pub(crate) fn texture_path(&self, path: &str) -> String {
        if std::path::Path::new(path).exists() {
            return path.to_string();
        }
        if let Some(d) = self.project_path.as_ref().and_then(|p| std::path::Path::new(p).parent().map(|d| d.to_path_buf())) {
            let alt = d.join(crate::file_name(path));
            if alt.exists() {
                return alt.to_string_lossy().to_string();
            }
        }
        path.to_string()
    }

    /// GPU layer of an image texture (loaded on first use).
    pub fn texture_layer(&mut self, path: &str) -> Option<u8> {
        if path.is_empty() || self.tex_failed.contains(path) || self.tex_loaded_res != crate::materials::tex_size() {
            if self.tex_loaded_res != crate::materials::tex_size() {
                self.tex_layers.clear();
                self.pbr_layers.clear();
                self.tex_failed.clear();
                self.tex_loaded_res = crate::materials::tex_size();
            }
            if path.is_empty() || self.tex_failed.contains(path) {
                return None;
            }
        }
        if let Some(i) = self.tex_layers.iter().position(|t| t.0 == path) {
            return Some(i as u8);
        }
        if self.tex_layers.len() >= crate::materials::max_textures() {
            self.error(format!("Too many image textures for {} px (max {}). Lower the texture resolution in Settings.", crate::materials::tex_size(), crate::materials::max_textures()));
            self.tex_failed.insert(path.to_string());
            return None;
        }
        match crate::materials::load_texture(&self.texture_path(path)) {
            Ok(px) => {
                self.tex_layers.push((path.to_string(), px));
                self.push_textures();
                Some((self.tex_layers.len() - 1) as u8)
            }
            Err(e) => {
                self.error(e);
                self.tex_failed.insert(path.to_string());
                None
            }
        }
    }

    fn push_textures(&mut self) {
        let all: Vec<Vec<u8>> = self.tex_layers.iter().map(|t| t.1.clone()).collect();
        self.renderer.lock().unwrap_or_else(|e| e.into_inner()).pending.push(Upload::Textures(all));
    }

    /// PBR material: two packed layers (albedo+A0+roughness in A / normal+metallic in A).
    /// returns the first layer; the shader knows the second one as layer+1 (code 128+layer).
    pub fn pbr_layer(&mut self, m: &crate::materials::Material) -> Option<u8> {
        if !m.is_pbr() || m.texture.is_empty() {
            return self.texture_layer(&m.texture);
        }
        let sig = format!("{}|{}|{}|{}|{}|{}|{}|{}|{}", m.texture, m.normal_map, m.roughness_map, m.metallic_map, m.ao_map, m.height_map, m.normal_strength, m.rough_mult, m.metal_mult);
        if self.tex_loaded_res != crate::materials::tex_size() {
            self.tex_layers.clear();
            self.pbr_layers.clear();
            self.tex_failed.clear();
            self.tex_loaded_res = crate::materials::tex_size();
        }
        if let Some(l) = self.pbr_layers.get(&sig) {
            return Some(*l);
        }
        if self.tex_layers.len() + 2 > crate::materials::max_textures() {
            self.error(format!(
                "Too many textured materials for {} px (max {} layers / {} textured materials). Only materials used in Assignments load textures - remove unused rules or increase Texture size in Settings › Appearance (higher resolution means fewer layers, lower means more).",
                crate::materials::tex_size(), crate::materials::max_textures(), crate::materials::max_textures() / 2
            ));
            return None;
        }
        let load = |p: &str| {
            if p.is_empty() { None } else { crate::materials::load_texture(&self.texture_path(p)).ok() }
        };
        let albedo = match load(&m.texture) {
            Some(a) => a,
            None => return self.texture_layer(&m.texture),
        };
        let normal = load(&m.normal_map);
        let rough = load(&m.roughness_map);
        let metal = load(&m.metallic_map);
        let ao = load(&m.ao_map);
        let (a, b) = crate::materials::pack_pbr(&albedo, normal.as_deref(), rough.as_deref(), metal.as_deref(), ao.as_deref(), m.normal_strength, m.rough_mult, m.metal_mult);
        let first = self.tex_layers.len() as u8;
        self.tex_layers.push((format!("{sig}#base"), a));
        self.tex_layers.push((format!("{sig}#surf"), b));
        self.push_textures();
        self.pbr_layers.insert(sig, first);
        Some(first)
    }

    /// One material per image file (named after the file). The image is copied into your material
    /// library folder and the material is saved there too - available in every project.
    pub fn import_texture_materials(&mut self, paths: &[String]) {
        let lib = crate::materials::user_library_dir().map(|d| d.join("textures"));
        let mut added = Vec::new();
        for p in paths {
            if crate::materials::load_texture(p).is_err() {
                self.error(format!("Not a PNG / JPG image: {}", crate::file_name(p)));
                continue;
            }
            let name = std::path::Path::new(p).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Texture".into());
            // copy of the image into the library (the original may be moved / deleted later)
            let tex = lib
                .as_ref()
                .and_then(|d| {
                    std::fs::create_dir_all(d).ok()?;
                    let dst = d.join(crate::file_name(p));
                    std::fs::copy(p, &dst).ok()?;
                    Some(dst.to_string_lossy().to_string())
                })
                .unwrap_or_else(|| p.clone());
            let mut m = crate::materials::library()[4].clone();
            m.name = name;
            m.color = [255, 255, 255];
            m.texture = tex;
            m.scale = 1.0;
            match self.materials.iter_mut().find(|x| x.name == m.name) {
                Some(x) => *x = m.clone(),
                None => self.materials.push(m.clone()),
            }
            added.push(m);
        }
        if !added.is_empty() {
            let last = added.last().map(|m| m.name.clone()).unwrap_or_default();
            self.mats.sel = self.materials.iter().position(|m| m.name == last).unwrap_or(0);
            self.mats.scroll = true;
            self.keep_in_library(&mut added.clone());
            self.toast(format!("{} texture materials added and saved to your library - set the real size of the image in \"Texture size\".", added.len()));
        }
    }

    /// Imported materials stay in your library (available in any project, after a restart).
    pub fn keep_in_library(&mut self, mats: &mut [crate::materials::Material]) {
        if mats.is_empty() {
            return;
        }
        // the maps go into the library (the material no longer depends on the downloaded folder)
        for m in mats.iter_mut() {
            crate::materials::adopt_maps(m);
        }
        if let Err(e) = crate::materials::save_to_user_library(mats) {
            self.error(format!("The materials are in the project but could not be saved to your library: {e}"));
        }
        self.user_lib_cache = None;
    }

    /// The names of the materials in your library (marked with ★ in the list).
    pub fn user_library_names(&mut self) -> std::collections::HashSet<String> {
        if self.user_lib_cache.is_none() {
            self.user_lib_cache = Some(crate::materials::user_library().into_iter().map(|m| m.name).collect());
        }
        self.user_lib_cache.clone().unwrap_or_default()
    }

    /// PBR packs (.zip downloaded from freepbr.com, ambientCG, Poly Haven... or unzipped folders) ->
    /// new materials, saved permanently in your library (%APPDATA%\ManageBIM\Materials).
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
        self.user_lib_cache = None;
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
        self.mats.scroll = true;
        self.mats.sel = self.materials.iter().position(|m| m.name == last).unwrap_or(0);
        self.pending_mat_assign = added.iter().map(|m| m.name.clone()).collect();
        self.materials_changed();
        self.toast(format!("{} PBR material(s) added: {} - choose where to apply them in the Materials panel.", added.len(), added.iter().map(|m| m.name.clone()).collect::<Vec<_>>().join(", ")));
    }

    /// Saves the chosen material to your library (available in any project).
    pub fn save_material_to_library(&mut self, i: usize) {
        let Some(m) = self.materials.get(i).cloned() else { return };
        self.user_lib_cache = None;
        match crate::materials::save_to_user_library(std::slice::from_ref(&m)) {
            Ok(_) => self.toast(format!("\"{}\" saved to your material library.", m.name)),
            Err(e) => self.error(format!("Cannot save the material library: {e}")),
        }
    }

    pub fn materials_changed(&mut self) {
        self.mats.cache = None;
        self.colors_dirty = true;
    }

    pub fn assign_material(&mut self, target: MatTarget, material: &str) {
        // the same target: replaces the material
        if let Some(a) = self.mats.assign.iter_mut().find(|a| a.target == target) {
            a.material = material.to_string();
        } else {
            self.mats.assign.push(MatAssign { target, material: material.to_string(), enabled: true });
        }
        self.materials_on = true;
        self.materials_changed();
    }

    pub fn auto_materials(&mut self) {
        let mut n = 0;
        for (kw, m) in crate::materials::AUTO_RULES {
            let present = self.cache_types.iter().any(|t| t.to_lowercase().contains(&kw.to_lowercase()));
            if present && self.materials.iter().any(|x| x.name == m) && !self.mats.assign.iter().any(|a| a.target == MatTarget::Type(kw.to_string())) {
                self.mats.assign.insert(n, MatAssign { target: MatTarget::Type(kw.to_string()), material: m.to_string(), enabled: true });
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
        self.rt.looks_gen = self.rt.looks_gen.wrapping_add(1);
        if self.materials_on {
            self.material_map(); // fills mat_cache if needed; below we only borrow it
        }
        let profile_on = self.profile.active && !self.profile.rules.is_empty();
        if profile_on {
            self.profile_map();
        }
        // ONLY the materials used in rules take GPU layers: otherwise a large library exhausts
        // the texture array, although most materials are not assigned to anything.
        let used: std::collections::HashSet<String> = self.mats.assign.iter().filter(|a| a.enabled).map(|a| a.material.clone()).collect();
        let mat_gpu: Vec<[u8; 4]> = (0..self.materials.len())
            .map(|i| {
                let m = self.materials[i].clone();
                let layer = if !m.texture.is_empty() && used.contains(&m.name) {
                    self.pbr_layer(&m)
                } else {
                    None
                };
                m.gpu(layer)
            })
            .collect();
        let roles = self.clash_roles();
        let ghost = !roles.is_empty();
        let others = self.clash.others;
        // the clash colours (Settings in the Results tab): A, B and the „pale” ones of the other open clashes
        let (ca, cb, dim_a) = (self.settings.clash_ui.col_a, self.settings.clash_ui.col_b, self.settings.clash_ui.dim_alpha);
        let pale = |c: [u8; 3]| [((c[0] as u16 + 255) / 2) as u8, ((c[1] as u16 + 255) / 2) as u8, ((c[2] as u16 + 255) / 2) as u8, 235];
        let (role_a, role_b, role_pa, role_pb) = ([ca[0], ca[1], ca[2], 255], [cb[0], cb[1], cb[2], 255], pale(ca), pale(cb));
        let sim = self.sim_view();
        let empty = HashMap::new();
        let empty_p: HashMap<ElemId, u16> = HashMap::new();
        let matmap = if self.materials_on { self.mats.cache.as_ref().unwrap_or(&empty) } else { &empty };
        let (hidden_set, overrides, sim_cache, selected, materials) = (&self.hidden, &self.overrides, &self.sim.cache, &self.selected, &self.materials);
        let sim_only_linked = self.sim.only_linked && !sim_cache.is_empty();
        let anim_obj = &self.anim_obj;
        let (pmap, prules, punmatched) = (self.profile_map.as_ref().unwrap_or(&empty_p), &self.profile.rules, self.profile.unmatched);
        let cmp = self.compare.as_ref().filter(|_| self.compare_on);
        // the look of an element: manual override > profile > material; the 4D simulation and the clash take priority
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
            // Compare: the colour by change type; the old pair (A) of a matched element is hidden
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
            // Animator: colour / transparency / visibility of the animated sets
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
            // the colours on the GPU match the last "looks" sent; None = everything must be sent
            let prev = self.looks.get(&mi).filter(|p| p.len() == looks.len());
            let changed: Vec<usize> = match prev {
                Some(p) => (0..looks.len()).filter(|&i| p[i] != looks[i]).collect(),
                None => (0..looks.len()).collect(),
            };
            let total_tris = m.tri_color.len();
            let changed_tris: usize = changed.iter().map(|&i| m.elements[i].tri_count as usize).sum();
            if prev.is_none() || changed_tris * 3 > total_tris {
                // many changes (or the first time): the whole buffer, filled on all cores
                ups.push(full_colors(mi as u32, m, &looks, tint));
            } else {
                // few changes (e.g. a selection click): only the affected elements, grouped in contiguous ranges
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
        // Compare: per-triangle deviation map over the colours above (the changed elements from B)
        if let Some(c) = self.compare.as_ref().filter(|_| self.compare_on && self.compare_heat) {
            let mut maxes: Vec<f32> = c.dev.values().map(|d| d.max).collect();
            maxes.sort_by(|a, b| a.total_cmp(b));
            let auto = maxes.get(((maxes.len() as f32 * 0.95) as usize).min(maxes.len().saturating_sub(1))).copied().unwrap_or(0.1).max(0.01);
            let scale = if self.compare_heat_max_mm > 0.0 { self.compare_heat_max_mm / 1000.0 } else { auto };
            for (id, d) in &c.dev {
                let Some(m) = self.models.get(id.m as usize).and_then(|m| m.as_ref()) else { continue };
                let Some(e) = m.elements.get(id.e as usize) else { continue };
                let hidden = new_looks.get(&(id.m as usize)).and_then(|l| l.get(id.e as usize)).map(|l| l.hidden).unwrap_or(false);
                if hidden || d.tri.len() != e.tri_count as usize {
                    continue;
                }
                let mut col = vec![0u8; d.tri.len() * 12];
                for (t, v) in d.tri.iter().enumerate() {
                    let k = heat(*v / scale);
                    for vtx in 0..3 {
                        col[(t * 3 + vtx) * 4..(t * 3 + vtx) * 4 + 4].copy_from_slice(&k);
                    }
                }
                let mat = vec![0u8; col.len()];
                ups.push(Upload::ColorRange { slot: id.m, first: e.tri_start * 3, col, mat });
            }
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
        let roles = if self.clash.others != Others::Normal { Some(self.clash_roles()).filter(|r| !r.is_empty()) } else { None };
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
        // point clouds: the nearest point (in front) within a narrow cone around the ray -
        // measurements snap exactly to the scanned points (the returned triangle is degenerate: [p, p, p])
        if let Some(hit) = self.pick_cloud(o, d, best.as_ref().map_or(f32::INFINITY, |b| b.1), &accept) {
            best = Some(hit);
        }
        best
    }

    /// (colour mode, min elevation, max elevation, round) for the point cloud shader
    pub fn cloud_uniform(&self) -> Vec4 {
        let (mut z0, mut z1) = (f32::MAX, f32::MIN);
        for (_, m) in self.loaded_models() {
            if m.cloud.is_some() && m.aabb.is_valid() {
                z0 = z0.min(m.aabb.min.z);
                z1 = z1.max(m.aabb.max.z);
            }
        }
        if z0 > z1 {
            (z0, z1) = (0.0, 1.0);
        }
        Vec4::new(self.cloud.mode as f32, z0, z1, if self.cloud.round { 1.0 } else { 0.0 })
    }

    /// The cloud point hit by the ray (only the displayed points - the current density - and the visible scans).
    fn pick_cloud(&self, o: Vec3, d: Vec3, tmax: f32, accept: &(dyn Fn(Vec3) -> bool + Sync)) -> Option<(ElemId, f32, Vec3, [Vec3; 3])> {
        use rayon::prelude::*;
        let len = d.length();
        if len < 1e-9 {
            return None;
        }
        let dn = d / len;
        let tmax_w = if tmax.is_finite() { tmax * len } else { f32::INFINITY };
        // tolerance: ~5 px on screen (angular in perspective, constant in orthographic)
        let vp_h = self.vp_rect[3].max(100.0);
        let px = (self.cam.fov() * 0.5).tan() * 2.0 / vp_h;
        let tol = 5.0 * px;
        let inv = Vec3::new(1.0 / dn.x, 1.0 / dn.y, 1.0 / dn.z);
        let mut best: Option<(ElemId, f32, Vec3)> = None;
        for (mi, m) in self.loaded_models() {
            let Some(c) = m.cloud.as_ref() else { continue };
            for &(ei, first, n) in &c.scans {
                let id = ElemId { m: mi as u32, e: ei };
                let Some(e) = m.elements.get(ei as usize) else { continue };
                let lim = best.as_ref().map_or(tmax_w, |b| b.1);
                let bb = Aabb { min: e.aabb.min - Vec3::splat(0.05), max: e.aabb.max + Vec3::splat(0.05) };
                if !self.is_visible(id) || !bb.ray(o, inv, lim) {
                    continue;
                }
                let shown = ((n as f64) * self.cloud.density.clamp(0.01, 1.0) as f64).ceil() as usize;
                let (a, b) = (first as usize, (first as usize + shown).min(c.pos.len()));
                if a >= b {
                    continue;
                }
                let ortho = self.cam.ortho;
                let focus = self.cam.focus;
                let hit = c.pos[a..b]
                    .par_iter()
                    .filter_map(|p| {
                        let v = *p - o;
                        let t = v.dot(dn);
                        if t <= 0.0 || t >= lim {
                            return None;
                        }
                        let r = tol * if ortho { focus } else { t };
                        let perp2 = v.length_squared() - t * t;
                        (perp2 <= r * r && accept(*p)).then_some((t, *p))
                    })
                    .min_by(|x, y| x.0.total_cmp(&y.0));
                if let Some((t, p)) = hit {
                    if best.as_ref().is_none_or(|b| t < b.1) {
                        best = Some((id, t, p));
                    }
                }
            }
        }
        best.map(|(id, t, p)| (id, t / len, p, [p, p, p]))
    }

    /// What is drawn in the current frame, per model: the elements in the field of view, not hidden, inside the section box;
    /// while the camera moves, elements smaller than ~2 pixels are skipped (when the camera is still, everything is drawn).
    /// The transparent pass gets only the elements with something transparent. Nearby ranges are merged
    /// (the extra vertices are cut / hidden by the shader anyway), so the visual result does not change.
    pub fn draw_lists(&mut self, p: &FrameParams, vp_h: f32) -> HashMap<u32, crate::render::DrawList> {
        // camera movement (for the temporary simplification)
        let c = self.cam;
        let moved = c.eye != self.lod_cam.eye || c.yaw != self.lod_cam.yaw || c.pitch != self.lod_cam.pitch || c.focus != self.lod_cam.focus || c.fov_deg != self.lod_cam.fov_deg;
        if moved {
            self.lod_cam = c;
            self.lod_moved = Instant::now();
        }
        let moving = self.lod_moved.elapsed().as_secs_f32() < 0.3;
        // the frustum planes (Gribb-Hartmann) from the view-projection matrix
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
        // adaptive LOD: while moving, the threshold (pixels) grows when FPS drops and comes back when FPS is good;
        // camera stopped = the whole model, except objects under one pixel (invisible anyway)
        if moving {
            if self.fps > 0.0 && self.fps < 28.0 {
                self.lod_px = (self.lod_px * 1.25).min(24.0);
            } else if self.fps > 50.0 {
                self.lod_px = (self.lod_px / 1.15).max(2.0);
            }
        }
        // rendering / export use the whole model anyway; here we can also force the viewport to full detail
        let min_px = if self.settings.no_lod { 0.0 } else if moving { self.lod_px } else { 0.6 };
        let eye = c.eye();
        let mut out = HashMap::new();
        let mut st = crate::render::DrawStats::default();
        let slots: Vec<usize> = self.loaded_models().map(|(i, _)| i).collect();
        for slot in slots {
            let Some(model) = self.models[slot].clone() else { continue };
            // the elements with transparent triangles in the file colours (computed once per model)
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
                st.elements += 1;
                st.triangles += e.tri_count as u64;
                if e.tri_count == 0 {
                    continue;
                }
                let look = looks.and_then(|l| l.get(ei));
                if look.map(|l| l.hidden).unwrap_or(false) {
                    st.hidden += 1;
                    continue;
                }
                let bb = &e.aabb;
                if !bb.is_valid() || !inside(bb) {
                    st.culled_frustum += 1;
                    continue;
                }
                if let Some(sb) = &section_box {
                    if !bb.overlaps(sb, 0.0) {
                        st.culled_section += 1;
                        continue;
                    }
                }
                if min_px > 0.0 {
                    let r = bb.size().length() * 0.5;
                    let px = if c.ortho { r / c.ortho_half().max(1e-3) * vp_h * 0.5 } else { r / ((bb.center() - eye).length().max(1e-3) * tanh) * vp_h * 0.5 };
                    if px < min_px {
                        st.culled_lod += 1;
                        continue;
                    }
                }
                st.elements_drawn += 1;
                st.triangles_drawn += e.tri_count as u64;
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
            // too many ranges -> we merge them more loosely (draw calls cost more than the extra vertices)
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
            st.ranges += (all.len() + transp.len()) as u64;
            // point cloud: the visible scans, the first N% of the points (shuffled on reading); fewer while moving
            let mut points = Vec::new();
            if let Some(cl) = &model.cloud {
                // while moving it is reduced only when the cloud is very large (over ~8 million points displayed) and
                // proportionally - previously it always dropped to 35%, and on every zoom the points disappeared and
                // came back (the screen "flickered")
                let base = self.cloud.density.clamp(0.01, 1.0);
                let shown = cl.pos.len() as f32 * base;
                let budget = 8_000_000.0f32;
                // ...and only when the graphics card really cannot keep up (under ~30 FPS); otherwise the cloud stays the same while moving
                let slow = self.fps > 0.0 && self.fps < 30.0;
                let dens = if moving && slow && shown > budget { base * (budget / shown).max(0.35) } else { base };
                for &(ei, start, n) in &cl.scans {
                    if looks.and_then(|l| l.get(ei as usize)).map(|l| l.hidden).unwrap_or(false) {
                        continue;
                    }
                    if let Some(e) = model.elements.get(ei as usize) {
                        if e.aabb.is_valid() && (!inside(&e.aabb) || section_box.as_ref().is_some_and(|sb| !e.aabb.overlaps(sb, 0.0))) {
                            continue;
                        }
                    }
                    st.points += n as u64;
                    let k = ((n as f32 * dens).ceil() as i32).clamp(0, n as i32);
                    if k > 0 {
                        st.points_drawn += k as u64;
                        points.push((start as i32, k));
                    }
                }
            }
            out.insert(slot as u32, crate::render::DrawList { all, transp, points });
        }
        self.perf = st;
        out
    }

    /// the camera is moving now (simplified drawing is active) - for the full redraw after it stops
    pub fn lod_moving(&self) -> bool {
        self.lod_moved.elapsed().as_secs_f32() < 0.35
    }

    /// the triangles of all loaded models (very large scenes skip the shadows while the camera moves)
    pub fn scene_tris(&self) -> usize {
        self.loaded_models().map(|(_, m)| m.pos.len() / 3).sum()
    }

    /// The sphere enclosing the visible models (without point clouds), for the shadow map.
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
        // the lamps from the model reach the GPU shader: the light is visible in real time in 3D
        let mut lamps = [Vec4::ZERO; 8];
        let mut lamp_data = [Vec4::ZERO; 8];
        let mut lamp_dir = [Vec4::ZERO; 8];
        for (k, l) in self.lights.iter().filter(|l| l.enabled).take(8).enumerate() {
            let col = l.linear();
            lamps[k] = Vec4::new(l.pos[0], l.pos[1], l.pos[2], l.power.max(0.0) / 20.0);
            // radius 0 = unlimited: sent as 0 (previously it became 0.1 m and the light disappeared completely)
            lamp_data[k] = Vec4::new(col[0], col[1], col[2], l.radius.max(0.0));
            let d = l.direction();
            let w = match l.kind {
                crate::lights::LightKind::Point => -2.0,
                crate::lights::LightKind::Distant => 2.0,
                crate::lights::LightKind::Spot => (l.angle.clamp(5.0, 120.0) * 0.5).to_radians().cos(),
            };
            lamp_dir[k] = Vec4::new(d.x, d.y, d.z, w);
        }
        FrameParams { view_proj: vp, eye: self.cam.eye(), planes, nplanes: pl.len().min(6) as i32, sun: self.render.sun_dir(), light: self.render.light(),
            lamps, lamp_data, lamp_dir, night: self.render.night, nlamps: self.lights.iter().filter(|l| l.enabled).count().min(8) as i32, point_size: self.cloud.point_size, cloud: self.cloud_uniform(), cloud_world: [self.cloud.world_mm / 1000.0, proj.y_axis.y * self.vp_rect[3].max(100.0) * self.ppp * 0.5], caps: self.sec.caps, ao: if self.settings.visual_style == crate::render::VisualStyle::Wireframe || self.settings.visual_style == crate::render::VisualStyle::Hidden { 0.0 } else { self.render.ao }, edges: if self.settings.visual_style == crate::render::VisualStyle::Concept || self.settings.visual_style == crate::render::VisualStyle::Hidden { self.render.edges.max(0.9) } else { self.render.edges }, style: self.settings.visual_style, depth_k: [proj.z_axis.z, proj.w_axis.z, if self.cam.ortho { 1.0 } else { 0.0 }, proj.y_axis.y], shadows: self.render.shadows && self.settings.visual_style != crate::render::VisualStyle::Wireframe && self.settings.visual_style != crate::render::VisualStyle::Hidden && !(self.lod_moving() && self.scene_tris() > 2_000_000), scene: self.shadow_sphere() }
    }
}
