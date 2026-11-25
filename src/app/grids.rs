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
    /// Cea mai apropiata axa de grila / nivel pentru planul de taiere.
    /// Top / Bottom se aliniaza la niveluri; Front / Back / Left / Right la liniile
    /// de grila (axiale). Planul astfel „urmareste" grila: muti grila, se muta si planul.
    pub fn snap_cut(&self, axis: usize, v: f32) -> f32 {
        if !self.settings.snap_cut_to_grid {
            return v;
        }
        let g = self.scene_grids();
        let mut cands: Vec<f32> = Vec::new();
        if axis == 2 {
            cands.extend(g.levels.iter().map(|(_, z)| *z));
        } else {
            for (_, a, b) in &g.grids {
                if axis == 0 && (a.x - b.x).abs() < 0.01 {
                    cands.push(a.x); // linie verticala in plan (X constant)
                }
                if axis == 1 && (a.y - b.y).abs() < 0.01 {
                    cands.push(a.y); // linie orizontala (Y constant)
                }
            }
        }
        cands.into_iter().min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs())).unwrap_or(v)
    }

    pub fn cad_cut_plane(&self) -> Option<(usize, f32)> {
        use crate::app::CadView;
        let view = self.settings.cad_view;
        // In modul "Current", daca sectiunea e Box, planul taietii folosit la
        // export e planul orizontal la nivel + 1.2 m (ca in planurile Revit).
        if view == crate::app::CadView::Current {
            if self.section.enabled && self.section.mode == crate::app::SectionMode::Box {
                let (a, b) = (self.section.bmin, self.section.bmax);
                // FARA aliniere la grila in modul cub: planul sta EXACT unde l-a pus
                // utilizatorul (chiar si pe fata cubului), iar exportul taie exact acolo.
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
        // CAND CUBUL DE SECTIUNE E ACTIV, vederile denumite se pozitioneaza pe
        // FETELE CUBULUI (nu pe scena): Top = fata de sus a cubului, Front = fata
        // din fata etc. Tragi fata cubului cu sageata -> planul o urmareste.
        let (lo, hi) = if self.section.enabled && self.section.mode == crate::app::SectionMode::Box {
            (self.section.bmin[a], self.section.bmax[a])
        } else {
            (bb.min[a], bb.max[a])
        };
        // Directiile opuse trebuie sa dea planuri DIFERITE; apoi planul se
        // aliniaza la axa de grila / nivelul cel mai apropiat.
        let raw = match view {
            CadView::Front => lo + (hi - lo) * t,
            CadView::Back => hi - (hi - lo) * t,
            CadView::Left => lo + (hi - lo) * t,
            CadView::Right => hi - (hi - lo) * t,
            CadView::Top => hi - (hi - lo) * t,
            CadView::Bottom => lo + (hi - lo) * t,
            CadView::Current => unreachable!(),
        };
        // Cand cubul e activ, planul e chiar o FATA a cubului: exact unde a pus
        // utilizatorul cubul, fara aliniere la grila. Fara cub, se aliniaza.
        let value = if self.section.enabled && self.section.mode == crate::app::SectionMode::Box { raw } else { self.snap_cut(a, raw) };
        Some((a, value))
    }

    /// Calculeaza SECTIONEA propriu-zisa: intersectia triunghiurilor cu planul,
    /// limitata strict la cub. Intoarce segmentele 3D pe plan (aceleasi pe care
    /// le scrie exportul DWG), ca sa le putem desena in viewport.
    pub fn section_cut_lines(&self, axis: usize, value: f32) -> Vec<(Vec3, Vec3)> {
        if !self.section.enabled || self.section.mode != crate::app::SectionMode::Box {
            return Vec::new();
        }
        let (bmin, bmax) = (self.section.bmin, self.section.bmax);
        let mut out = Vec::new();
        for (_, m) in self.loaded_models() {
            for e in &m.elements {
                if e.tri_count == 0 {
                    continue;
                }
                if (0..3).any(|d| e.aabb.min[d] > bmax[d] || e.aabb.max[d] < bmin[d]) {
                    continue;
                }
                for t in e.tri_start..e.tri_start + e.tri_count {
                    let i = t as usize * 3;
                    let (Some(a), Some(b), Some(c)) = (m.pos.get(i).copied(), m.pos.get(i + 1).copied(), m.pos.get(i + 2).copied()) else { continue };
                    let d = [a[axis] - value, b[axis] - value, c[axis] - value];
                    let mut pts: Vec<Vec3> = Vec::with_capacity(2);
                    for (p, q, dp, dq) in [(a, b, d[0], d[1]), (b, c, d[1], d[2]), (c, a, d[2], d[0])] {
                        if (dp < 0.0) != (dq < 0.0) {
                            let k = dp / (dp - dq);
                            pts.push(p + (q - p) * k);
                        }
                    }
                    if pts.len() == 2 && (pts[0] - pts[1]).length_squared() > 1e-8 {
                        if (0..3).all(|ax| pts.iter().all(|p| p[ax] >= bmin[ax] - 1e-3 && p[ax] <= bmax[ax] + 1e-3)) {
                            out.push((pts[0], pts[1]));
                        }
                    }
                    if out.len() >= 60000 {
                        return out;
                    }
                }
            }
        }
        out
    }

    /// Muchiile geometriei aflate IN SPATELE planului de taiere, intre plan si
    /// peretele opus al cubului, proiectate PE plan. Se deseneaza punctat in 3D
    /// si se scriu pe stratul BEHIND (linetip HIDDEN) in DWG.
    /// `sign`: +1 daca „in spate" inseamna coordonata mai mare pe axa taieturii.
    pub fn section_behind_edges(&self, axis: usize, value: f32, sign: f32) -> Vec<(Vec3, Vec3)> {
        if !self.section.enabled || self.section.mode != crate::app::SectionMode::Box {
            return Vec::new();
        }
        let (bmin, bmax) = (self.section.bmin, self.section.bmax);
        let mut out: Vec<(Vec3, Vec3)> = Vec::new();
        let mut seen: std::collections::HashSet<[i32; 6]> = std::collections::HashSet::new();
        for (_, m) in self.loaded_models() {
            for e in &m.elements {
                if e.tri_count == 0 {
                    continue;
                }
                if (0..3).any(|d| e.aabb.min[d] > bmax[d] || e.aabb.max[d] < bmin[d]) {
                    continue;
                }
                for t in e.tri_start..e.tri_start + e.tri_count {
                    let i = t as usize * 3;
                    let (Some(a), Some(b), Some(c)) = (m.pos.get(i).copied(), m.pos.get(i + 1).copied(), m.pos.get(i + 2).copied()) else { continue };
                    for (p, q) in [(a, b), (b, c), (c, a)] {
                        let dp = sign * (p[axis] - value);
                        let dq = sign * (q[axis] - value);
                        if dp <= 1e-4 && dq <= 1e-4 {
                            continue;
                        }
                        // partea din spate a muchiei (daca o traverseaza, o taiem la plan)
                        let (pp0, qq0) = if dp > 1e-4 && dq > 1e-4 {
                            (p, q)
                        } else if dp > 1e-4 {
                            let t = dp / (dp - dq);
                            (p, p + (q - p) * t)
                        } else {
                            let t = dp / (dp - dq);
                            (q + (p - q) * t, q)
                        };
                        let (mut pp, mut qq) = (pp0, qq0);
                        pp[axis] = value;
                        qq[axis] = value;
                        // STRICT: ambele capete trebuie sa fie in cub (toate cele 3 axe)
                        if (0..3).any(|d| pp[d] < bmin[d] - 1e-3 || pp[d] > bmax[d] + 1e-3 || qq[d] < bmin[d] - 1e-3 || qq[d] > bmax[d] + 1e-3) {
                            continue;
                        }
                        let key = [
                            (pp.x * 1000.0).round() as i32, (pp.y * 1000.0).round() as i32, (pp.z * 1000.0).round() as i32,
                            (qq.x * 1000.0).round() as i32, (qq.y * 1000.0).round() as i32, (qq.z * 1000.0).round() as i32,
                        ];
                        if seen.insert(key) {
                            out.push((pp, qq));
                        }
                        if out.len() >= 40000 {
                            return out;
                        }
                    }
                }
            }
        }
        out
    }

    /// Taietura sectiunii curente (plan / nivel) ca linii 2D in DWG (scriitor integrat) sau DXF:
    /// un layer pe tip de element (Walls, Doors, IfcColumn...), coordonate reale in metri.
    /// Construiește EXACT desenul 2D al tăieturii: segmente (layer, x1,y1,x2,y2) + texte.
    /// Folosit atât de exportul DWG / PDF, cât și de fereastra de preview, ca să fie identice.
    pub fn section_drawing(&self) -> Option<(Vec<(String, [f64; 4])>, Vec<CadText>, usize, f32)> {
        let view = self.settings.cad_view;
        if view == crate::app::CadView::Current && !self.section.enabled {
            return None;
        }
        // planul de taiere: axa (0 = X, 1 = Y, 2 = Z), valoarea (coordonate locale) si zona (doar la box)
        let (axis, value, clip): (usize, f32, Option<([f32; 3], [f32; 3])>) = if view.axis().is_some() {
            let (a, v) = self.cad_cut_plane()?;
            let box_clip = if self.section.enabled && self.section.mode == crate::app::SectionMode::Box {
                Some((self.section.bmin, self.section.bmax))
            } else {
                None
            };
            (a, v, box_clip)
        } else {
            match self.section.mode {
            SectionMode::Box => {
                let (a, b) = (self.section.bmin, self.section.bmax);
                let z = self.cad_cut_plane().map(|(_, v)| v).unwrap_or(a[2] + 1.2);
                (2, z, Some((a, b)))
            }
            SectionMode::Planes => {
                let p = self.section.planes.iter().find(|p| p.enabled)?;
                let axis = match p.dir { 0 | 1 => 2, 2 | 3 => 0, _ => 1 };
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
        // un element intra in sectiune numai daca AABB-ul lui prinde cubul SI planul
        let in_box = |e: &crate::model::Element| match clip {
            Some((a, b)) => (0..3).all(|d| e.aabb.min[d] <= b[d] + 1e-3 && e.aabb.max[d] >= a[d] - 1e-3),
            None => true,
        };
        // taierea exacta a fiecarui segment la frontiera cubului (slab method):
        // nu respingem segmentul daca e in afara, il scurtam pana la perete
        let clip_seg = |p: Vec3, q: Vec3| -> Option<(Vec3, Vec3)> {
            let (a, b) = clip?;
            let d = q - p;
            let (mut t0, mut t1) = (0.0_f32, 1.0_f32);
            for ax in 0..3 {
                if d[ax].abs() < 1e-9 {
                    if p[ax] < a[ax] - 1e-3 || p[ax] > b[ax] + 1e-3 {
                        return None;
                    }
                } else {
                    let mut lo = (a[ax] - 1e-3 - p[ax]) / d[ax];
                    let mut hi = (b[ax] + 1e-3 - p[ax]) / d[ax];
                    if lo > hi {
                        std::mem::swap(&mut lo, &mut hi);
                    }
                    t0 = t0.max(lo);
                    t1 = t1.min(hi);
                    if t0 > t1 {
                        return None;
                    }
                }
            }
            Some((p + d * t0, p + d * t1))
        };
        for (mi, m) in &models {
            if m.cloud.is_some() {
                continue;
            }
            for (ei, e) in m.elements.iter().enumerate() {
                let id = ElemId { m: *mi as u32, e: ei as u32 };
                if e.tri_count == 0 || e.default_hidden || self.hidden.contains(&id) || !in_box(e) {
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
                    if pts.len() == 2 && (pts[0] - pts[1]).length_squared() > 1e-8 {
                        let (c0, c1) = match clip_seg(pts[0], pts[1]) {
                            Some(v) => v,
                            None => continue,
                        };
                        if (c0 - c1).length_squared() <= 1e-8 {
                            continue;
                        }
                        let (p, q) = (to2d(c0), to2d(c1));
                        segs.push((layer.clone(), [p[0], p[1], q[0], q[1]]));
                    }
                }
                if segs.len() > first {
                    let tag = if axis == 2 { cut_tag(e) } else { None };
                    cut_elems.push((first, segs.len(), cut_is_hatched(&e.ifc_type), tag));
                }
            }
        }
        // Proiectia de dupa planul de taiere, ca o vedere de desen din Inventor: muchiile reale ale
        // formei (fara diagonalele plasei), vizibile cu linie plina, ascunse cu linie intrerupta.
        if self.cad_visible_edges || self.cad_hidden_edges {
            segs.extend(self.section_projection(view, axis, value, clip, &models, &to2d));
        }
        if segs.is_empty() {
            return None;
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
                if *hatch && self.cad_hatch {
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
                section_annotations(&levels, &grids, axis, world, [x0, y0, x1, y1], u, self.cad_dims, self.cad_levels, &mut extra, &mut texts);
            }
            segs.extend(extra);
        }
        Some((segs, texts, axis, value))
    }

    /// Liniile vizibile / ascunse de dupa planul de taiere (layere VISIBLE / HIDDEN), cu cache:
    /// calculul (buffer de adancime + muchii) se reface doar cand se schimba vederea sau modelele.
    fn section_projection(
        &self,
        view: crate::app::CadView,
        axis: usize,
        value: f32,
        clip: Option<([f32; 3], [f32; 3])>,
        models: &[(usize, Arc<Model>)],
        to2d: &dyn Fn(Vec3) -> [f64; 2],
    ) -> Vec<(String, [f64; 4])> {
        use crate::app::CadView;
        // directia privirii: ce e "dupa" plan (fata aleasa a cubului priveste spre interiorul lui)
        let sign = match view {
            CadView::Top => -1.0,
            CadView::Bottom => 1.0,
            CadView::Front | CadView::Left => 1.0,
            CadView::Back | CadView::Right => -1.0,
            CadView::Current => if axis == 2 { -1.0 } else { 1.0 },
        };
        let (bmin, bmax) = match clip {
            Some((a, b)) => (Vec3::from(a), Vec3::from(b)),
            None => {
                let bb = self.scene_aabb();
                if !bb.is_valid() {
                    return Vec::new();
                }
                (bb.min, bb.max)
            }
        };
        let key = format!(
            "{axis}|{value:.4}|{sign}|{bmin:?}|{bmax:?}|{}|{}|{}|{}",
            self.cad_visible_edges,
            self.cad_hidden_edges,
            self.hidden.len(),
            models.iter().map(|(i, m)| format!("{i}:{:p}", Arc::as_ptr(m))).collect::<Vec<_>>().join(",")
        );
        if let Some((k, v)) = self.hlr_cache.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            if *k == key {
                return v.clone();
            }
        }
        let hv = crate::hlr::HlrView { axis, plane: value, sign, bmin, bmax, res: 1600 };
        // triunghiurile elementelor din zona de dupa plan (o lista per element)
        let (lo, hi) = if sign > 0.0 { (value, bmax[axis]) } else { (bmin[axis], value) };
        let mut elements: Vec<Vec<[Vec3; 3]>> = Vec::new();
        let mut total = 0usize;
        for (mi, m) in models {
            if m.cloud.is_some() {
                continue;
            }
            for (ei, e) in m.elements.iter().enumerate() {
                let id = ElemId { m: *mi as u32, e: ei as u32 };
                if e.tri_count == 0 || e.default_hidden || self.hidden.contains(&id) {
                    continue;
                }
                if (0..3).any(|d| e.aabb.min[d] > bmax[d] || e.aabb.max[d] < bmin[d]) || e.aabb.max[axis] < lo || e.aabb.min[axis] > hi {
                    continue;
                }
                let mut tris = Vec::with_capacity(e.tri_count as usize);
                for t in e.tri_start..e.tri_start + e.tri_count {
                    let i = t as usize * 3;
                    if let (Some(a), Some(b), Some(c)) = (m.pos.get(i), m.pos.get(i + 1), m.pos.get(i + 2)) {
                        tris.push([Vec3::from(*a), Vec3::from(*b), Vec3::from(*c)]);
                    }
                }
                total += tris.len();
                elements.push(tris);
                // modele uriase: limita, ca previewul sa ramana fluid (cubul mai mic = mai mult detaliu)
                if total > 3_000_000 {
                    break;
                }
            }
        }
        let r = crate::hlr::compute(&hv, &elements, self.cad_hidden_edges, 250_000);
        let mut out = Vec::with_capacity(r.visible.len() + r.hidden.len());
        let mut push = |layer: &str, a: Vec3, b: Vec3| {
            let (p, q) = (to2d(a), to2d(b));
            if (p[0] - q[0]).abs() + (p[1] - q[1]).abs() > 1e-5 {
                out.push((layer.to_string(), [p[0], p[1], q[0], q[1]]));
            }
        };
        if self.cad_visible_edges {
            for (a, b) in &r.visible {
                push("VISIBLE", *a, *b);
            }
        }
        for (a, b) in &r.hidden {
            push("HIDDEN", *a, *b);
        }
        *self.hlr_cache.lock().unwrap_or_else(|e| e.into_inner()) = Some((key, out.clone()));
        out
    }

    /// Plansa cu mai multe vederi din cubul de sectiune, ca in Inventor: fatadele (Front, Right, Back,
    /// Left) pe un rand, Top deasupra lui Front, Bottom sub el. Doar vederile bifate (`mv_views`), cu
    /// distanta `mv_gap` intre ele (0 = automat). Front ramane in coordonatele lui REALE (1:1, metri),
    /// celelalte sunt asezate langa el la dimensiunea lor reala. Rezultatul e in cache (preview fluid).
    pub fn multiview_layout(&mut self) -> Result<Arc<crate::app::MultiViewSheet>, String> {
        use crate::app::CadView;
        if !(self.section.enabled && self.section.mode == crate::app::SectionMode::Box) {
            return Err("Turn on the section box first (Sections › Model box): the views are made from its faces.".into());
        }
        let key = format!(
            "{:?}|{:.3}|{:?}|{:?}|{}|{}|{}|{}|{}|{}|{}",
            self.mv_views,
            self.mv_gap,
            self.section.bmin,
            self.section.bmax,
            self.hidden.len(),
            self.cad_visible_edges,
            self.cad_hidden_edges,
            self.cad_hatch,
            self.cad_dims,
            self.loaded_models().count(),
            self.mv_titles
        );
        if let Some((k, v)) = &self.mv_cache {
            if *k == key {
                return Ok(v.clone());
            }
        }
        let order = [CadView::Front, CadView::Right, CadView::Back, CadView::Left, CadView::Top, CadView::Bottom];
        let (old_view, old_pos) = (self.settings.cad_view, self.settings.cad_view_pos);
        let mut views: Vec<(CadView, Vec<(String, [f64; 4])>, Vec<CadText>)> = Vec::new();
        for (k, v) in order.iter().enumerate() {
            if !self.mv_views[k] {
                continue;
            }
            self.settings.cad_view = *v;
            self.settings.cad_view_pos = 0.0;
            if let Some((segs, texts, _, _)) = self.section_drawing() {
                if !segs.is_empty() {
                    views.push((*v, segs, texts));
                }
            }
        }
        self.settings.cad_view = old_view;
        self.settings.cad_view_pos = old_pos;
        if views.is_empty() {
            return Err(if self.mv_views.iter().any(|b| *b) { "Nothing to draw: move the section box over the model.".into() } else { "Tick at least one view.".into() });
        }
        // limitele unei vederi: liniile SI textele ei (etichetele de nivel ies in afara desenului;
        // distanta dintre vederi se masoara de la ele, ca sa nu intre in vederea vecina)
        let u = 0.1f64;
        let bbox_full = |segs: &[(String, [f64; 4])], texts: &[CadText]| {
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for (_, s) in segs {
                x0 = x0.min(s[0]).min(s[2]);
                y0 = y0.min(s[1]).min(s[3]);
                x1 = x1.max(s[0]).max(s[2]);
                y1 = y1.max(s[1]).max(s[3]);
            }
            for t in texts {
                let w = text_w(&t.s, t.mm, u);
                let h = t.mm * u;
                let (ex, ey) = if (t.rot - 90.0).abs() < 1.0 { (h, w) } else { (w, h) };
                x0 = x0.min(t.x - if (t.rot - 90.0).abs() < 1.0 { h } else { 0.0 });
                y0 = y0.min(t.y);
                x1 = x1.max(t.x + ex);
                y1 = y1.max(t.y + ey);
            }
            [x0, y0, x1, y1]
        };
        let views: Vec<(CadView, Vec<(String, [f64; 4])>, Vec<CadText>, [f64; 4])> = views.into_iter().map(|(v, s, t)| { let b = bbox_full(&s, &t); (v, s, t, b) }).collect();
        // spatiul dintre vederi: ales de utilizator, sau 15% din cea mai mare latime (minim 2 m)
        let maxw = views.iter().map(|v| { let b = v.3; (b[2] - b[0]).max(b[3] - b[1]) }).fold(0.0, f64::max);
        let gap = if self.mv_gap > 0.0 { self.mv_gap as f64 } else { (maxw * 0.15).max(2.0) };
        // vederea de referinta (ramane pe loc): Front, altfel prima fatada, altfel prima vedere
        let is_plan = |v: &CadView| matches!(v, CadView::Top | CadView::Bottom);
        let anchor = views.iter().position(|v| v.0 == CadView::Front).or_else(|| views.iter().position(|v| !is_plan(&v.0))).unwrap_or(0);
        let front = views[anchor].3;
        let elev_h = views.iter().filter(|v| !is_plan(&v.0)).map(|v| v.3[3] - v.3[1]).fold(0.0, f64::max);
        let has_elev = views.iter().any(|v| !is_plan(&v.0));
        let title_h = if self.mv_titles { (gap * 0.35).max(0.5) } else { 0.0 };
        let mut sheet = crate::app::MultiViewSheet::default();
        // fatadele: incepem cu vederea de referinta, apoi celelalte la dreapta ei
        let mut x_cursor = front[2] + gap;
        for (i, (v, segs, tx, b)) in views.iter().enumerate() {
            let b = *b;
            let (dx, dy) = if i == anchor {
                (0.0, 0.0)
            } else if has_elev && *v == CadView::Top {
                // Top deasupra randului de fatade, aliniat cu Front
                (front[0] - b[0], front[1] + elev_h + gap - b[1])
            } else if has_elev && *v == CadView::Bottom {
                // Bottom sub Front (sub titlul lui)
                (front[0] - b[0], front[1] - gap - title_h - b[3])
            } else {
                let d = (x_cursor - b[0], front[1] - b[1]);
                x_cursor = b[2] + d.0 + gap;
                d
            };
            for (l, s) in segs {
                sheet.segs.push((l.clone(), [s[0] + dx, s[1] + dy, s[2] + dx, s[3] + dy]));
            }
            for t in tx {
                let mut t = t.clone();
                t.x += dx;
                t.y += dy;
                sheet.texts.push(t);
            }
            let fr = [b[0] + dx, b[1] + dy, b[2] + dx, b[3] + dy];
            // titlul vederii sub ea, ca in Inventor
            if self.mv_titles {
                sheet.texts.push(CadText::new("VIEW_TITLES", fr[0], fr[1] - title_h, 5.0, 0.0, format!("{} VIEW", v.label().to_uppercase())));
            }
            sheet.frames.push((v.label().to_string(), fr));
        }
        sheet.gap = gap;
        let sheet = Arc::new(sheet);
        self.mv_cache = Some((key, sheet.clone()));
        Ok(sheet)
    }

    /// Exportul plansei cu mai multe vederi: DWG / DXF 1:1 in model space sau PDF cu cartus.
    pub fn export_multiview(&mut self, path: &str) {
        let sheet = match self.multiview_layout() {
            Ok(s) => s,
            Err(e) => return self.error(e),
        };
        let n = sheet.frames.len();
        if path.to_lowercase().ends_with(".pdf") {
            let project = self.project_path.as_ref().map(|p| model_label(&crate::file_name(p))).unwrap_or_else(|| "ManageBIM".into());
            let bytes = crate::pdf::section_sheet(&sheet.segs, &sheet.texts, &self.title_block, &project, "Views from the section box", &now_string());
            return match std::fs::write(path, bytes) {
                Ok(()) => self.toast(format!("{n} views exported as PDF: {}", crate::file_name(path))),
                Err(e) => self.error(format!("Cannot write {path}: {e}")),
            };
        }
        let dwg = path.to_lowercase().ends_with(".dwg");
        let dxf_path = if dwg { std::path::Path::new(path).with_extension("dxf").to_string_lossy().to_string() } else { with_ext(path, "dxf") };
        let content = cut_dxf_annot(&sheet.segs, &sheet.texts, 0.1);
        if let Err(e) = std::fs::write(&dxf_path, content) {
            return self.error(format!("Cannot write {dxf_path}: {e}"));
        }
        if !dwg {
            return self.toast(format!("{n} views exported at 1:1 in model space (Front in real coordinates): {}", crate::file_name(&dxf_path)));
        }
        match crate::formats::dxf_to_dwg(&dxf_path) {
            Ok(out) => {
                let _ = std::fs::remove_file(&dxf_path);
                self.toast(format!("{n} views exported at 1:1 in model space (Front in real coordinates): {}", crate::file_name(&out)))
            }
            Err(e) => self.toast(format!("Saved as DXF ({}) - {e}", crate::file_name(&dxf_path))),
        }
    }

    /// Exportul DWG / DXF / PDF al tăieturii. Folosește EXACT același desen ca previewul.
    pub fn export_section_cad(&mut self, path: &str) {
        let view = self.settings.cad_view;
        let Some((segs, texts, axis, value)) = self.section_drawing() else {
            return self.error("Turn on a section first, or choose Front / Back / Left / Right in Settings › Export.");
        };
        let _ = value;
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
        // metadatele taieturii pentru TOATE exporturile in model space (si din cub),
        // ca la import sa punem desenul EXACT in locul din care a fost extras
        if !self.cad_sheet {
            let o = self.origin_f64();
            let mirror = view.mirrored();
            let meta = format!("999\nMB_CUT_PLANE axis={axis} value={value:.4} mirror={} origin={:.4},{:.4},{:.4}\n", if mirror { 1 } else { 0 }, o.x, o.y, o.z);
            content = content.replacen("2\nHEADER\n", &format!("2\nHEADER\n{meta}"), 1);
            // ...si ca text minuscul IN desen (strat MB_CUT_PLANE): supravietuieste conversiei in DWG,
            // redenumirii fisierului si salvarii din AutoCAD - comentariul DXF si fisierul alaturat nu
            let line = meta.lines().nth(1).unwrap_or("").to_string();
            let (mut x0, mut y0) = (f64::MAX, f64::MAX);
            for (_, s) in &segs {
                x0 = x0.min(s[0]).min(s[2]);
                y0 = y0.min(s[1]).min(s[3]);
            }
            let text = format!("0\nTEXT\n8\nMB_CUT_PLANE\n62\n8\n10\n{:.4}\n20\n{:.4}\n30\n0.0\n40\n0.002\n1\n{line}\n", if x0.is_finite() { x0 } else { 0.0 }, if y0.is_finite() { y0 } else { 0.0 });
            if let Some(p) = content.rfind("0\nENDSEC\n0\nEOF") {
                content.insert_str(p, &text);
            }
            // sidecar pentru DWG (rezerva pentru desenele exportate de versiuni mai vechi)
            let _ = std::fs::write(format!("{path}.mbplane"), line);
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

    /// Adauga un desen 2D peste model. Daca e o taietura exportata din ManageBIM,
    /// revine EXACT in planul si la coordonatele din care a fost extras.
    pub fn import_overlay(&mut self, path: &str) {
        let mut ov = CadOverlay {
            name: crate::file_name(path), path: path.to_string(), z: 0.0, dx: 0.0, dy: 0.0,
            rot_deg: 0.0, vert: None, mirror: false, visible: true,
            color: [255, 120, 60], opacity: 0.85, segs: Default::default(),
        };
        let mut note = String::from("2D overlay");
        if let Some((ax, val, mir, old)) = crate::formats::read_cut_plane_meta(path) {
            let cur = self.origin_f64();
            // planul salvat e in coordonatele locale de la export; liniile 2D sunt in coordonate REALE
            // (lume), deci se aduc singure in coordonatele de acum - corectam doar planul, o singura data
            // (inainte se mai adauga si o deplasare, iar desenul pleca din loc cand originea era alta)
            let shift = (old[ax.min(2)] - cur[ax.min(2)]) as f32;
            ov.z = val + if old == glam::DVec3::ZERO { 0.0 } else { shift };
            ov.mirror = mir;
            ov.vert = match ax { 0 => Some(1), 1 => Some(0), _ => None };
            ov.dx = 0.0;
            ov.dy = 0.0;
            note = format!("section cut restored at {:+.3} m", ov.z);
        }
        match self.load_overlay_segs(&mut ov) {
            Ok(n) => {
                let z = ov.z;
                self.toast(format!("{} added: {n} lines, {note} (Sections › DWG Overlay)", ov.name));
                let _ = z;
                self.overlays.push(ov);
                self.overlay_open = true;
            }
            Err(e) => self.error(format!("{}: {e}", crate::file_name(path))),
        }
    }

    /// (re)citeste liniile desenului, in coordonatele scenei (relative la origine)
    pub fn load_overlay_segs(&self, ov: &mut CadOverlay) -> Result<usize, String> {
        // cititorul DWG / DXF poate crapa pe un desen deteriorat: devine eroare, nu inchide aplicatia
        let path = ov.path.clone();
        let (lines, _) = std::panic::catch_unwind(|| crate::formats::cad_lines(&path)).unwrap_or_else(|_| Err(format!("{} could not be read (damaged drawing).", crate::file_name(&path))))?;
        if lines.is_empty() {
            return Err("no lines in the drawing".into());
        }
        let o = self.origin_f64();
        let (sx, sy) = match ov.vert {
            Some(0) => (o.x, o.z), // YZ: 2D = (x, z)
            Some(1) => (o.y, o.z), // XZ: 2D = (y, z)
            _ => (o.x, o.y),       // XY: plan la Z
        };
        // vederile Back / Right sunt exportate oglindite (x = -lume): desenarea inverseaza x inapoi, deci
        // aici originea se aduna in loc sa se scada (altfel desenul ajungea in partea opusa a originii)
        let ox = if ov.mirror { -sx } else { sx };
        let segs: Vec<[f32; 4]> = lines
            .iter()
            .filter(|(l, _)| !l.eq_ignore_ascii_case("MB_CUT_PLANE"))
            .map(|(_, p)| [(p[0] - ox) as f32, (p[1] - sy) as f32, (p[2] - ox) as f32, (p[3] - sy) as f32])
            .collect();
        let n = segs.len();
        ov.segs = Arc::new(segs);
        Ok(n)
    }
}
