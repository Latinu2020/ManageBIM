//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    /// Axele / nivelurile modelului din slotul dat: din IFC (IfcGrid, IfcBuildingStorey) sau, fara niveluri in
    /// fisier, deduse din elemente (cota minima a elementelor fiecarui nivel). Calculate o data per model.
    pub fn grid_data(&self, slot: usize) -> Arc<GridData> {
        let Some(m) = self.models.get(slot).cloned().flatten() else { return Arc::new(GridData::default()) };
        let key = Arc::as_ptr(&m) as usize;
        if let Some((k, g)) = self.grid_cache.borrow().get(&slot) {
            if *k == key {
                return g.clone();
            }
        }
        let o = self.origin_f64();
        let xf = self.base_models.get(slot).cloned().flatten().map(|b| {
            let x = self.model_xf.get(slot).copied().unwrap_or_default();
            if x.is_identity() { Mat4::IDENTITY } else { self.xf_matrix(&b, &x) }
        });
        let xf = xf.unwrap_or(Mat4::IDENTITY);
        let loc = |p: [f64; 3]| xf.transform_point3((DVec3::from_array(p) - o).as_vec3());
        let mut g = GridData::default();
        if let Some((_, v)) = m.info.iter().find(|(k, _)| k == "~grids") {
            if let Ok(list) = serde_json::from_str::<Vec<(String, [f64; 3], [f64; 3])>>(v) {
                g.grids = list.into_iter().map(|(t, a, b)| (t, loc(a), loc(b))).collect();
            }
        }
        if let Some((_, v)) = m.info.iter().find(|(k, _)| k == "~levels") {
            if let Ok(list) = serde_json::from_str::<Vec<(String, f64)>>(v) {
                g.levels = list.into_iter().map(|(n, z)| (n, loc([o.x, o.y, z]).z)).collect();
            }
        }
        if g.levels.is_empty() {
            let mut by: HashMap<&str, f32> = HashMap::new();
            for e in &m.elements {
                if !e.storey.is_empty() && e.aabb.is_valid() {
                    let z = by.entry(e.storey.as_str()).or_insert(f32::INFINITY);
                    *z = z.min(e.aabb.min.z);
                }
            }
            g.levels = by.into_iter().filter(|(_, z)| z.is_finite()).map(|(n, z)| (n.to_string(), z)).collect();
        }
        g.levels.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        let g = Arc::new(g);
        self.grid_cache.borrow_mut().insert(slot, (key, g.clone()));
        g
    }

    /// Axele si nivelurile tuturor modelelor (nivelurile cu acelasi nume o singura data, axele identice o data).
    pub fn scene_grids(&self) -> GridData {
        let mut all = GridData::default();
        let custom = self.use_user_levels && !self.user_levels.is_empty();
        let my_grids = self.use_user_grids && !self.user_grids.is_empty();
        if my_grids {
            let o = self.origin_f64();
            all.grids = self.user_grids.iter().map(|g| (g.name.clone(), Vec3::new((g.a[0] - o.x) as f32, (g.a[1] - o.y) as f32, 0.0), Vec3::new((g.b[0] - o.x) as f32, (g.b[1] - o.y) as f32, 0.0))).collect();
        }
        if custom {
            let oz = self.origin_f64().z;
            all.levels = self.user_levels.iter().map(|l| (l.name.clone(), (self.level_zero + l.elev - oz) as f32)).collect();
        }
        for (slot, _) in self.loaded_models() {
            let g = self.grid_data(slot);
            for x in g.grids.iter().filter(|_| !my_grids) {
                if !all.grids.iter().any(|y| y.0 == x.0 && (y.1 - x.1).length() < 0.05 && (y.2 - x.2).length() < 0.05) {
                    all.grids.push(x.clone());
                }
            }
            for l in g.levels.iter().filter(|_| !custom) {
                if !all.levels.iter().any(|y| y.0 == l.0) {
                    all.levels.push(l.clone());
                }
            }
        }
        all.levels.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        all
    }

    /// Locatia unui punct (coordonate locale): intersectia de axe cea mai apropiata si nivelul, ex. "C-4 / Level 2".
    pub fn grid_location(&self, p: Vec3) -> String {
        let g = self.scene_grids();
        grid_location_in(&g, p)
    }

    /// Axele si nivelurile din modele (fara nivelurile utilizatorului).
    pub fn scene_grids_models(&self) -> GridData {
        let mut all = GridData::default();
        for (slot, _) in self.loaded_models() {
            let g = self.grid_data(slot);
            for l in &g.levels {
                if !all.levels.iter().any(|y| y.0 == l.0) {
                    all.levels.push(l.clone());
                }
            }
            all.grids.extend(g.grids.iter().cloned());
        }
        all.levels.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        all
    }

    /// Axele folosite acum (ale tale sau din modele), in coordonate reale - pentru editor / export.
    pub fn grids_world(&self) -> Vec<UserGrid> {
        if self.use_user_grids && !self.user_grids.is_empty() {
            return self.user_grids.clone();
        }
        let o = self.origin_f64();
        self.scene_grids_models().grids.iter().map(|(n, a, b)| UserGrid { name: n.clone(), a: [a.x as f64 + o.x, a.y as f64 + o.y], b: [b.x as f64 + o.x, b.y as f64 + o.y] }).collect()
    }

    /// Planul de taiere pentru exportul DWG / PDF cand directia nu e "Current".
    /// Coordonate LOCALE, ca in camera si in viewport (fara origin).
    pub fn cad_cut_plane(&self) -> Option<(usize, f32)> {
        use crate::app::CadView;
        let view = self.settings.cad_view;
        // In modul "Current", daca sectiunea e Box, planul taietii folosit la
        // export e planul orizontal la nivel + 1.2 m (ca in planurile Revit).
        if view == crate::app::CadView::Current {
            if self.section.enabled && self.section.mode == crate::app::SectionMode::Box {
                let (a, b) = (self.section.bmin, self.section.bmax);
                // Planul e extras DIN CUB: se muta odata cu el cand tragi de fete.
                let z = (a[2] + self.settings.section_cut_offset.max(0.01)).min(b[2] - 0.01).max(a[2] + 0.01);
                return Some((2, z));
            }
            return None;
        }
        let a = view.axis()?;
        let bb = self.scene_aabb();
        if !bb.is_valid() {
            return None;
        }
        let t = self.settings.cad_view_pos.clamp(0.0, 1.0);
        let (lo, hi) = (bb.min, bb.max);
        // Directiile opuse trebuie sa dea planuri DIFERITE, altfel nu se vede
        // nicio schimbare cand alegi Front/Back sau Left/Right.
        let value = match view {
            CadView::Front => lo.y + (hi.y - lo.y) * t,
            CadView::Back => hi.y - (hi.y - lo.y) * t,
            CadView::Left => lo.x + (hi.x - lo.x) * t,
            CadView::Right => hi.x - (hi.x - lo.x) * t,
            CadView::Top => hi.z - (hi.z - lo.z) * t,
            CadView::Bottom => lo.z + (hi.z - lo.z) * t,
            CadView::Current => unreachable!(),
        };
        Some((a, value))
    }

    /// Taietura sectiunii curente (plan / nivel) ca linii 2D in DWG (scriitor integrat) sau DXF:
    /// un layer pe tip de element (Walls, Doors, IfcColumn...), coordonate reale in metri.
    pub fn export_section_cad(&mut self, path: &str) {
        let view = self.settings.cad_view;
        if view == crate::app::CadView::Current && !self.section.enabled {
            return self.error("Turn on a section first, or choose Front / Back / Left / Right in Settings › Export.");
        }
        // planul de taiere: axa (0 = X, 1 = Y, 2 = Z), valoarea (coordonate locale) si zona (doar la box)
        let (axis, value, clip): (usize, f32, Option<([f32; 3], [f32; 3])>) = if view.axis().is_some() {
            // vedere din directie: taie intregul model, la pozitia exacta din settings
            let Some((a, v)) = self.cad_cut_plane() else {
                return self.error("No model geometry to cut.");
            };
            (a, v, None)
        } else {
            match self.section.mode {
            SectionMode::Box => {
                let (a, b) = (self.section.bmin, self.section.bmax);
                // acelasi plan ca cel evidentiat in 3D (din cub, nu din nivel)
                let z = self.cad_cut_plane().map(|(_, v)| v).unwrap_or(a[2] + 1.2);
                (2, z, Some((a, b)))
            }
            SectionMode::Planes => {
                let Some(p) = self.section.planes.iter().find(|p| p.enabled) else { return self.error("No active section plane.") };
                let axis = match p.dir {
                    0 | 1 => 2,
                    2 | 3 => 0,
                    _ => 1,
                };
                (axis, p.offset, None)
            }
            }
        };
        let o = self.origin_f64();
        let models: Vec<(usize, Arc<Model>)> = self.loaded_models().map(|(i, m)| (i, m.clone())).collect();
        // (layer, x1, y1, x2, y2) in coordonatele plansei
        let mut segs: Vec<(String, [f64; 4])> = Vec::new();
        // elementele taiate: (primul segment, dupa ultimul, se hasureaza, eticheta)
        let mut cut_elems: Vec<(usize, usize, bool, Option<String>)> = Vec::new();
        // din spate / dreapta axa orizontala se inverseaza (ca in vederea din acea directie)
        let mirror = view.mirrored();
        let to2d = |p: Vec3| -> [f64; 2] {
            let w = [p.x as f64 + o.x, p.y as f64 + o.y, p.z as f64 + o.z];
            let mut r = match axis {
                2 => [w[0], w[1]],
                0 => [w[1], w[2]],
                _ => [w[0], w[2]],
            };
            if mirror {
                r[0] = -r[0];
            }
            r
        };
        let inside = |p: Vec3| match clip {
            Some((a, b)) => p.x >= a[0] - 1e-3 && p.x <= b[0] + 1e-3 && p.y >= a[1] - 1e-3 && p.y <= b[1] + 1e-3,
            None => true,
        };
        for (mi, m) in &models {
            if m.cloud.is_some() {
                continue;
            }
            for (ei, e) in m.elements.iter().enumerate() {
                let id = ElemId { m: *mi as u32, e: ei as u32 };
                if e.tri_count == 0 || e.default_hidden || self.hidden.contains(&id) {
                    continue;
                }
                if e.aabb.min[axis] > value || e.aabb.max[axis] < value {
                    continue;
                }
                let layer = cad_layer(if e.ifc_type.is_empty() { "Model" } else { &e.ifc_type });
                let first = segs.len();
                for t in e.tri_start..e.tri_start + e.tri_count {
                    let i = t as usize * 3;
                    let (Some(a), Some(b), Some(c)) = (m.pos.get(i).copied(), m.pos.get(i + 1).copied(), m.pos.get(i + 2).copied()) else { continue };
                    let d = [a[axis] - value, b[axis] - value, c[axis] - value];
                    let mut pts: Vec<Vec3> = Vec::with_capacity(2);
                    for (p, q, dp, dq) in [(a, b, d[0], d[1]), (b, c, d[1], d[2]), (c, a, d[2], d[0])] {
                        if (dp < 0.0) != (dq < 0.0) {
                            let t = dp / (dp - dq);
                            pts.push(p + (q - p) * t);
                        }
                    }
                    if pts.len() == 2 && inside(pts[0]) && inside(pts[1]) && (pts[0] - pts[1]).length_squared() > 1e-8 {
                        let (p, q) = (to2d(pts[0]), to2d(pts[1]));
                        segs.push((layer.clone(), [p[0], p[1], q[0], q[1]]));
                    }
                }
                if segs.len() > first {
                    let tag = if axis == 2 { cut_tag(e) } else { None };
                    cut_elems.push((first, segs.len(), cut_is_hatched(&e.ifc_type), tag));
                }
            }
        }
        if segs.is_empty() {
            return self.error("The section cuts no visible element.");
        }
        // planul complet: hasura pe elementele taiate, etichete (usi, ferestre, pereti), axe cu cote, niveluri pe sectiuni.
        // Marimile textelor / bulelor sunt in mm de hartie: u = metri pe mm (scara plansei sau 1:100 in Model space).
        let mut texts: Vec<CadText> = Vec::new();
        if self.cad_annotate {
            let u = if self.cad_sheet { sheet_scale(&segs, &self.title_block) as f64 / 1000.0 } else { 0.1 };
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for (_, p) in &segs {
                x0 = x0.min(p[0]).min(p[2]);
                y0 = y0.min(p[1]).min(p[3]);
                x1 = x1.max(p[0]).max(p[2]);
                y1 = y1.max(p[1]).max(p[3]);
            }
            let mut extra: Vec<(String, [f64; 4])> = Vec::new();
            let mut tag_boxes: Vec<[f64; 4]> = Vec::new();
            // usile / ferestrele primele (numerele lor conteaza), apoi peretii
            let mut cut_elems = cut_elems;
            cut_elems.sort_by_key(|c| c.3.as_ref().map(|_| 0).unwrap_or(1) + if c.2 { 1 } else { 0 });
            for (a, b, hatch, tag) in &cut_elems {
                let own: Vec<[f64; 4]> = segs[*a..*b].iter().map(|s| s.1).collect();
                if *hatch {
                    extra.extend(hatch_lines(&own, 1.5 * u).into_iter().map(|h| ("HATCH".to_string(), h)));
                }
                if let Some(tag) = tag {
                    let (mut ex0, mut ey0, mut ex1, mut ey1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                    for p in &own {
                        ex0 = ex0.min(p[0]).min(p[2]);
                        ey0 = ey0.min(p[1]).min(p[3]);
                        ex1 = ex1.max(p[0]).max(p[2]);
                        ey1 = ey1.max(p[1]).max(p[3]);
                    }
                    // de-a lungul elementului (perete vertical pe plansa -> text rotit 90 grade), deasupra lui
                    let vertical = ey1 - ey0 > (ex1 - ex0) * 1.5;
                    let (cx, cy) = ((ex0 + ex1) / 2.0, (ey0 + ey1) / 2.0);
                    let w = tag.chars().count() as f64 * 2.0 * u * 0.8;
                    let th = 2.6 * u;
                    // locul liber cel mai apropiat: doar DEASUPRA elementului (cu gap), apoi tot mai departe lateral / in sus
                    // (nu plasam niciodata sub element - se vede ce e dedesupt)
                    let mut placed = None;
                    for k in 0..6 {
                        let gap = 1.0 * u + k as f64 * th;
                        let b = if vertical {
                            let x = ex0 - gap - th;
                            [x, cy - w / 2.0, x + th, cy + w / 2.0]
                        } else {
                            let y = ey1 + gap;
                            [cx - w / 2.0, y, cx + w / 2.0, y + th]
                        };
                        if !tag_boxes.iter().any(|o: &[f64; 4]| b[0] < o[2] && o[0] < b[2] && b[1] < o[3] && o[1] < b[3]) {
                            placed = Some(b);
                            break;
                        }
                    }
                    let b = placed.unwrap_or_else(|| if vertical { [ex0 - 1.0 * u - th, cy - w / 2.0, ex0 - 1.0 * u, cy + w / 2.0] } else { [cx - w / 2.0, ey1 + 1.0 * u, cx + w / 2.0, ey1 + 1.0 * u + th] });
                    tag_boxes.push(b);
                    texts.push(if vertical { CadText::new("TAGS", b[2], b[1], 2.0, 90.0, tag.clone()) } else { CadText::new("TAGS", b[0], b[1], 2.0, 0.0, tag.clone()) });
                }
            }
            let grids = self.grids_world();
            if axis == 2 {
                plan_grid_annotations(&grids, u, &mut extra, &mut texts);
            } else {
                let oz = self.origin_f64().z;
                let levels: Vec<(String, f64)> = self.scene_grids().levels.iter().map(|(n, z)| (n.clone(), *z as f64 + oz)).collect();
                let o = self.origin_f64();
                let world = value as f64 + if axis == 0 { o.x } else { o.y };
                section_annotations(&levels, &grids, axis, world, [x0, y0, x1, y1], u, &mut extra, &mut texts);
            }
            segs.extend(extra);
        }
        // PDF: aceeasi taietura, desen vectorial pe plansa cu cartus si aceleasi anotatii
        if path.to_lowercase().ends_with(".pdf") {
            let n = segs.len();
            let level = self.section_level.and_then(|i| self.scene_grids().levels.get(i).map(|l| l.0.clone()));
            let title = if view != crate::app::CadView::Current {
                format!("{} section", view.label())
            } else {
                match (axis, level) {
                    (2, Some(l)) => format!("Plan - {l}"),
                    (2, None) => "Plan".to_string(),
                    _ => "Section".to_string(),
                }
            };
            let project = self.project_path.as_ref().map(|p| model_label(&crate::file_name(p))).unwrap_or_else(|| "ManageBIM".into());
            let bytes = crate::pdf::section_sheet(&segs, &texts, &self.title_block, &project, &title, &now_string());
            return match std::fs::write(path, bytes) {
                Ok(()) => self.toast(format!("Section exported as PDF ({n} lines): {}", crate::file_name(path))),
                Err(e) => self.error(format!("Cannot write {path}: {e}")),
            };
        }
        let dwg = path.to_lowercase().ends_with(".dwg");
        let dxf_path = if dwg { std::path::Path::new(path).with_extension("dxf").to_string_lossy().to_string() } else { with_ext(path, "dxf") };
        let n = segs.len();
        // implicit: 2D in Model space, coordonate reale (m); optional: pe plansa A3 cu cartus, la scara
        let mut content = if self.cad_sheet {
            let level = self.section_level.and_then(|i| self.scene_grids().levels.get(i).map(|l| l.0.clone()));
            let title = if view != crate::app::CadView::Current {
                format!("{} section", view.label())
            } else {
                match (axis, level) {
                    (2, Some(l)) => format!("Plan - {l}"),
                    (2, None) => "Plan".to_string(),
                    _ => "Section".to_string(),
                }
            };
            let project = self.project_path.as_ref().map(|p| model_label(&crate::file_name(p))).unwrap_or_else(|| "ManageBIM".into());
            cut_sheet_dxf_annot(&segs, &texts, &self.title_block, &project, &title, &now_string()).0
        } else {
            cut_dxf_annot(&segs, &texts, 0.1)
        };
        // metadatele taieturii, ca la import sa putem pune desenul EXACT in planul
        // din care a fost extras (comment 999 - ignorat de alte CAD-uri, citit de noi)
        if view.axis().is_some() && !self.cad_sheet {
            let o = self.origin_f64();
            let meta = format!("999\nMB_CUT_PLANE axis={axis} value={value:.4} mirror={} origin={:.4},{:.4},{:.4}\n", if mirror { 1 } else { 0 }, o.x, o.y, o.z);
            content = content.replacen("2\nHEADER\n", &format!("2\nHEADER\n{meta}"), 1);
            // sidecar pentru DWG (conversia poate elimina comentariile DXF)
            let _ = std::fs::write(format!("{path}.mbplane"), meta.lines().nth(1).unwrap_or("").to_string());
        }
        if let Err(e) = std::fs::write(&dxf_path, content) {
            return self.error(format!("Cannot write {dxf_path}: {e}"));
        }
        let what = if view != crate::app::CadView::Current {
            format!("{} section", view.label())
        } else {
            match axis {
                2 => "plan".to_string(),
                _ => "section".to_string(),
            }
        };
        if !dwg {
            return self.toast(format!("Section cut exported ({what}, {n} lines): {}", crate::file_name(&dxf_path)));
        }
        match crate::formats::dxf_to_dwg(&dxf_path) {
            Ok(out) => {
                // DXF-ul a fost doar intermediar
                let _ = std::fs::remove_file(&dxf_path);
                self.toast(format!("Section cut exported ({what}, {n} lines): {}", crate::file_name(&out)))
            }
            Err(e) => self.toast(format!("Saved as DXF ({}) - {e}", crate::file_name(&dxf_path))),
        }
    }

    /// Export DXF; daca extensia e .dwg, DXF-ul se converteste in DWG (scriitorul integrat, ODA doar ca rezerva).
    pub fn export_grids(&mut self, path: &str) {
        let grids = self.grids_world();
        if grids.is_empty() {
            return self.error("No grid axes: create them in View > Edit Grids, or open a model with grids.");
        }
        let dwg = path.to_lowercase().ends_with(".dwg");
        let dxf_path = if dwg { std::path::Path::new(path).with_extension("dxf").to_string_lossy().to_string() } else { with_ext(path, "dxf") };
        if let Err(e) = std::fs::write(&dxf_path, grid_dxf(&grids, 0.5)) {
            return self.error(format!("Cannot write {dxf_path}: {e}"));
        }
        if !dwg {
            return self.toast(format!("{} grid axes exported: {}", grids.len(), crate::file_name(&dxf_path)));
        }
        match crate::formats::dxf_to_dwg(&dxf_path) {
            Ok(out) => {
                let _ = std::fs::remove_file(&dxf_path);
                self.toast(format!("{} grid axes exported: {}", grids.len(), crate::file_name(&out)))
            }
            Err(e) => self.toast(format!("Saved as DXF ({}) - {e}", crate::file_name(&dxf_path))),
        }
    }

    /// Sectiune pe nivel: box de la nivel pana la nivelul urmator (tot planul XY al scenei).
    pub fn section_level_box(&mut self, li: usize) {
        let g = self.scene_grids();
        let bb = self.scene_aabb();
        let Some(l) = g.levels.get(li).cloned() else { return };
        if !bb.is_valid() {
            return;
        }
        let top = g.levels.get(li + 1).map(|n| n.1).unwrap_or(bb.max.z + 0.01);
        self.section.enabled = true;
        self.section.mode = SectionMode::Box;
        // marginile: sub nivel (ex. placa de sub) si sub nivelul urmator (ex. fara placa de deasupra)
        self.section.bmin = [bb.min.x - 0.5, bb.min.y - 0.5, l.1 - self.level_below.max(0.0) - 0.01];
        self.section.bmax = [bb.max.x + 0.5, bb.max.y + 0.5, top + self.level_above - 0.02];
        self.section_level = Some(li);
        self.colors_dirty = true;
        self.toast(format!("Section: {} (to {})", l.0, g.levels.get(li + 1).map(|n| n.0.clone()).unwrap_or_else(|| "the top".into())));
    }

    /// Nivelul de deasupra / dedesubt (PgUp / PgDn sau ◀ ▶), pastrand tipul sectiunii (box de nivel sau plan).
    pub fn section_level_step(&mut self, delta: i32) {
        let n = self.scene_grids().levels.len();
        if n == 0 {
            return;
        }
        let cur = self.section_level.unwrap_or(0) as i32;
        let li = (cur + delta).clamp(0, n as i32 - 1) as usize;
        if self.section.enabled && self.section.mode == SectionMode::Planes {
            self.section_plan_cut(li, self.plan_cut_h);
        } else {
            self.section_level_box(li);
        }
    }

    /// Doar elementele nivelului (dupa numele nivelului din model; altfel dupa inaltime), restul ascunse.
    pub fn isolate_level(&mut self, li: usize) {
        let g = self.scene_grids();
        let Some(l) = g.levels.get(li).cloned() else { return };
        let top = g.levels.get(li + 1).map(|n| n.1).unwrap_or(f32::INFINITY);
        let by_name = self.loaded_models().any(|(_, m)| m.elements.iter().any(|e| e.storey == l.0));
        let mut keep = Vec::new();
        for (mi, m) in self.loaded_models() {
            for (ei, e) in m.elements.iter().enumerate() {
                let ok = if by_name { e.storey == l.0 } else { e.aabb.is_valid() && { let z = (e.aabb.min.z + e.aabb.max.z) * 0.5; z >= l.1 - 0.01 && z < top } };
                if ok {
                    keep.push(ElemId { m: mi as u32, e: ei as u32 });
                }
            }
        }
        let n = keep.len();
        self.apply(Action::Isolate(keep));
        self.section_level = Some(li);
        self.toast(format!("{}: {n} items shown, the rest hidden (Unhide All to undo)", l.0));
    }

    /// Sectiune tip plan: totul de deasupra lui (nivel + h) e taiat.
    pub fn section_plan_cut(&mut self, li: usize, h: f32) {
        let g = self.scene_grids();
        let Some(l) = g.levels.get(li) else { return };
        self.section.enabled = true;
        self.section.mode = SectionMode::Planes;
        self.section.planes = vec![PlaneDef { enabled: true, dir: 0, offset: l.1 + h }];
        self.section_level = Some(li);
        self.plan_cut_h = h;
        self.colors_dirty = true;
        self.toast(format!("Plan cut: {} + {h:.2} m", l.0));
    }

    /// Genereaza reteaua din `grid_gen` (cu rotatie) ca axele utilizatorului.
    pub fn generate_user_grid(&mut self) {
        let g = self.grid_gen.clone();
        let mut out = generate_grid(g.origin, g.nx, g.dx, &g.fx, g.ny, g.dy, &g.fy, self.grid_ext as f64);
        transform_grids(&mut out, g.origin, g.angle_deg, [0.0, 0.0]);
        self.user_grids = out;
        self.use_user_grids = true;
        self.show_grids = true;
    }

    /// Clic in 3D in timpul alegerii: 1 = originea (apoi directia), 2 = directia -> reteaua se regenereaza.
    pub fn grid_pick_point(&mut self, local: Vec3) {
        let o = self.origin_f64();
        let w = [local.x as f64 + o.x, local.y as f64 + o.y];
        match self.grid_pick {
            1 => {
                self.grid_gen.origin = [(w[0] * 1000.0).round() / 1000.0, (w[1] * 1000.0).round() / 1000.0];
                self.grid_pick = 2;
                self.toast("Grid origin set. Now click a point along the first axis direction (Esc: keep the current angle).");
            }
            2 => {
                let (dx, dy) = (w[0] - self.grid_gen.origin[0], w[1] - self.grid_gen.origin[1]);
                if dx.hypot(dy) > 0.01 {
                    self.grid_gen.angle_deg = (dy.atan2(dx).to_degrees() * 100.0).round() / 100.0;
                }
                self.grid_pick = 0;
                self.generate_user_grid();
                self.toast(format!("Grid aligned: origin {:.3}, {:.3} · angle {:.2}°", self.grid_gen.origin[0], self.grid_gen.origin[1], self.grid_gen.angle_deg));
            }
            _ => {}
        }
    }

    /// Axe din DWG / DXF: devin axele mele (se pot muta / roti / edita apoi). `layer` = alt strat decat cel ghicit.
    pub fn import_grids_cad(&mut self, path: &str, layer: Option<&str>) {
        match crate::formats::grids_from_cad(path, layer) {
            Ok(g) if g.grids.is_empty() => {
                // stratul axelor nu s-a putut ghici: se alege din lista in fereastra Grids
                self.grid_import = Some((path.to_string(), g.layers, String::new()));
                self.show_grids_editor = true;
                self.toast("No layer named GRID / AXE / AX in the drawing - choose the layer with the grid lines in the Grids window.");
            }
            Ok(g) => {
                let n = g.grids.len();
                self.user_grids = g.grids.into_iter().map(|(name, a, b)| UserGrid { name, a, b }).collect();
                self.use_user_grids = true;
                self.show_grids = true;
                self.grid_import = Some((path.to_string(), g.layers, g.layer.clone()));
                self.toast(format!("{n} grid axes imported from {} (layer {}, units x{})", crate::file_name(path), g.layer, g.scale));
            }
            Err(e) => self.error(e),
        }
    }

    /// Adauga un desen 2D (plan de arhitectura, de instalatii...) la cota nivelului curent (sau 0).
    pub fn import_overlay(&mut self, path: &str) {
        let z = self.section_level.and_then(|i| self.scene_grids().levels.get(i).map(|l| l.1)).unwrap_or(0.0);
        let mut ov = CadOverlay { name: crate::file_name(path), path: path.to_string(), z, dx: 0.0, dy: 0.0, rot_deg: 0.0, visible: true, color: [255, 120, 60], opacity: 0.85, segs: Default::default() };
        match self.load_overlay_segs(&mut ov) {
            Ok(n) => {
                self.toast(format!("{} added as 2D overlay: {n} lines at {:+.2} m (Sections › DWG Overlay to move it)", ov.name, z));
                self.overlays.push(ov);
                self.overlay_open = true;
            }
            Err(e) => self.error(format!("{}: {e}", crate::file_name(path))),
        }
    }

    /// (re)citeste liniile desenului, in coordonatele scenei (relative la origine)
    pub fn load_overlay_segs(&self, ov: &mut CadOverlay) -> Result<usize, String> {
        let (lines, _) = crate::formats::cad_lines(&ov.path)?;
        if lines.is_empty() {
            return Err("no lines in the drawing".into());
        }
        let o = self.origin_f64();
        let segs: Vec<[f32; 4]> = lines.iter().map(|(_, p)| [(p[0] - o.x) as f32, (p[1] - o.y) as f32, (p[2] - o.x) as f32, (p[3] - o.y) as f32]).collect();
        let n = segs.len();
        ov.segs = Arc::new(segs);
        Ok(n)
    }
}
