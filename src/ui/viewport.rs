//! 3D viewport: drawing, picking, measuring, redlines, the view cube and the navigation bar.
use super::*;

impl App {
    pub(super) fn to_red(&self, rect: Rect, p: Pos2) -> [f32; 2] {
        [(p.x - rect.center().x) / rect.height(), (p.y - rect.center().y) / rect.height()]
    }

    /// adds a redline to the current viewpoint (a new viewpoint is saved if the view changed)
    pub(super) fn add_redline(&mut self, r: crate::markup::Redline) {
        let i = match self.active_vp.filter(|i| self.vp_matches(*i)) {
            Some(i) => i,
            None => {
                self.save_viewpoint();
                let n = self.viewpoints.len();
                self.viewpoints[n - 1].name = format!("Markup {n}");
                self.toast("New viewpoint created for the markup (see Viewpoints).");
                n - 1
            }
        };
        self.active_vp = Some(i);
        self.viewpoints[i].redlines.push(r);
    }

    /// Selectie cu fereastra: `crossing` = tot ce atinge dreptunghiul, altfel doar ce e complet in interior.
    /// Doar elementele vizibile; `add` (Ctrl) pastreaza selectia existenta.
    pub(super) fn select_in_window(&mut self, vp: &Mat4, view: Rect, sel: Rect, crossing: bool, add: bool) {
        let mut ids = Vec::new();
        for (mi, m) in self.loaded_models() {
            for (ei, e) in m.elements.iter().enumerate() {
                let id = ElemId { m: mi as u32, e: ei as u32 };
                if !e.aabb.is_valid() || self.hidden.contains(&id) || e.default_hidden {
                    continue;
                }
                let (a, b) = (e.aabb.min, e.aabb.max);
                let corners: Vec<Option<Pos2>> = (0..8).map(|i| to_screen(vp, view, Vec3::new(if i & 1 == 0 { a.x } else { b.x }, if i & 2 == 0 { a.y } else { b.y }, if i & 4 == 0 { a.z } else { b.z }))).collect();
                let ok = if crossing {
                    let pts: Vec<Pos2> = corners.iter().flatten().copied().collect();
                    if pts.is_empty() {
                        false
                    } else {
                        let r = Rect::from_points(&pts);
                        r.intersects(sel)
                    }
                } else {
                    corners.iter().all(|c| c.is_some_and(|p| sel.contains(p)))
                };
                if ok {
                    ids.push(id);
                }
            }
        }
        let n = ids.len();
        self.apply(Action::Select(ids, add));
        self.toast(format!("{n} items {} the window", if crossing { "touching" } else { "inside" }));
    }

    // ================================================================== ViewCube
    pub(super) fn view_cube(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let pal = self.pal();
        // cub compact: mai mult loc pentru model
        let size = 24.0f32;
        let c = Pos2::new(rect.right() - 68.0, rect.top() + 66.0);
        let area = Rect::from_center_size(c, EV::splat(size * 3.4));
        let resp = ui.interact(area, ui.id().with("viewcube"), Sense::click());
        let (r, u, f) = (self.cam.right(), self.cam.up(), self.cam.fwd());
        let proj = |p: Vec3| c + EV::new(p.dot(r), -p.dot(u)) * size;
        // fete: normala, axe U / V, eticheta
        let faces: [(Vec3, Vec3, Vec3, &str); 6] = [
            (Vec3::Z, Vec3::X, Vec3::Y, "TOP"),
            (-Vec3::Z, Vec3::X, -Vec3::Y, "BOTTOM"),
            (-Vec3::Y, Vec3::X, Vec3::Z, "FRONT"),
            (Vec3::Y, -Vec3::X, Vec3::Z, "BACK"),
            (Vec3::X, Vec3::Y, Vec3::Z, "RIGHT"),
            (-Vec3::X, -Vec3::Y, Vec3::Z, "LEFT"),
        ];
        let mut vis: Vec<(f32, usize)> = faces.iter().enumerate().filter(|(_, fc)| fc.0.dot(f) < -0.02).map(|(i, fc)| (fc.0.dot(f), i)).collect();
        vis.sort_by(|a, b| b.0.total_cmp(&a.0)); // departe -> aproape (total_cmp: fara panic la NaN)
        // zonele de muchie / colt (tesituri, ca ViewCube-ul Autodesk): 26 de directii precise
        let edge = 0.64f32;
        let bands = [(-1.0f32, -edge), (-edge, edge), (edge, 1.0)];
        let hover = resp.hover_pos();
        let mut hit: Option<Vec3> = None;
        // (poligon, directie, fata, este centrul fetei)
        let mut cells: Vec<(Vec<Pos2>, Vec3, usize, bool)> = Vec::new();
        for &(_, fi) in &vis {
            let (n, fu, fv, _) = faces[fi];
            for (iu, bu) in bands.iter().enumerate() {
                for (iv, bv) in bands.iter().enumerate() {
                    let poly: Vec<Pos2> = [(bu.0, bv.0), (bu.1, bv.0), (bu.1, bv.1), (bu.0, bv.1)].iter().map(|(a, b)| proj(n + fu * *a + fv * *b)).collect();
                    let dir = n + fu * (iu as f32 - 1.0) + fv * (iv as f32 - 1.0);
                    cells.push((poly, dir, fi, iu == 1 && iv == 1));
                }
            }
        }
        // privire exact pe o fata: sageti spre cele 4 fete vecine (ca la Autodesk)
        let facing = faces.iter().position(|fc| fc.0.dot(f) < -0.9995);
        let mut arrows: Vec<(Vec<Pos2>, Vec3)> = Vec::new();
        if let Some(fi) = facing {
            let (n, fu, fv, _) = faces[fi];
            for d in [fu, -fu, fv, -fv] {
                let tip = proj(n + d * 1.62);
                let b0 = proj(n + d * 1.22 + (if d.dot(fu).abs() > 0.5 { fv } else { fu }) * 0.34);
                let b1 = proj(n + d * 1.22 - (if d.dot(fu).abs() > 0.5 { fv } else { fu }) * 0.34);
                arrows.push((vec![tip, b0, b1], d));
            }
        }
        if let Some(hp) = hover {
            for (tri, d) in &arrows {
                if inside(tri, hp) || tri[0].distance(hp) < 7.0 {
                    hit = Some(*d);
                }
            }
            // cea mai apropiata fata e ultima
            if hit.is_none() {
                for (poly, dir, _, _) in cells.iter().rev() {
                    if inside(poly, hp) {
                        hit = Some(*dir);
                        break;
                    }
                }
            }
        }
        let p = ui.painter();
        let dark = self.settings.theme.is_dark();
        let base = if dark { Color32::from_rgb(232, 236, 242) } else { Color32::from_rgb(248, 249, 252) };
        let edge_col = if dark { Color32::from_rgb(98, 106, 120) } else { Color32::from_rgb(128, 136, 150) };
        let ring_col = if dark { Color32::from_rgb(120, 130, 146) } else { Color32::from_rgb(150, 158, 172) };
        // ---- busola sub cub (desenata inainte, cubul o acopera): inel cu gradatii si N / E / S / W
        let ring_pt = |a: f32, rad: f32| proj(Vec3::new(a.cos() * rad, a.sin() * rad, -1.08));
        let outer: Vec<Pos2> = (0..64).map(|i| ring_pt(i as f32 / 64.0 * std::f32::consts::TAU, 1.78)).collect();
        let inner: Vec<Pos2> = (0..64).map(|i| ring_pt(i as f32 / 64.0 * std::f32::consts::TAU, 1.52)).collect();
        for i in 0..64 {
            let j = (i + 1) % 64;
            p.add(egui::Shape::convex_polygon(vec![outer[i], outer[j], inner[j], inner[i]], ring_col.gamma_multiply(0.22), Stroke::NONE));
        }
        p.add(egui::Shape::closed_line(outer, Stroke::new(1.0_f32, ring_col.gamma_multiply(0.8))));
        p.add(egui::Shape::closed_line(inner, Stroke::new(0.8_f32, ring_col.gamma_multiply(0.5))));
        for k in 0..16 {
            let a = k as f32 / 16.0 * std::f32::consts::TAU;
            let r0 = if k % 4 == 0 { 1.52 } else { 1.6 };
            p.line_segment([ring_pt(a, r0), ring_pt(a, 1.70)], Stroke::new(0.8_f32, ring_col.gamma_multiply(0.8)));
        }
        // ---- fetele cubului: umbrire dupa o lumina fixa (sus-stanga), tesituri usor mai inchise
        let light = (u * 0.75 - r * 0.35 - f * 0.55).normalize();
        for (poly, dir, fi, center) in &cells {
            let n = faces[*fi].0;
            let k = 0.80 + 0.20 * n.dot(light).max(0.0);
            let k = if *center { k } else { k * 0.95 };
            let mut col = Color32::from_rgb((base.r() as f32 * k) as u8, (base.g() as f32 * k) as u8, (base.b() as f32 * k) as u8);
            if hit.map(|h| (h - *dir).length() < 1e-4).unwrap_or(false) {
                col = accent().gamma_multiply(0.9);
            }
            p.add(egui::Shape::convex_polygon(poly.clone(), col, Stroke::NONE));
        }
        for &(_, fi) in &vis {
            let (n, fu, fv, label) = faces[fi];
            // liniile fine ale tesiturilor
            for e in [-edge, edge] {
                p.line_segment([proj(n + fu * e - fv), proj(n + fu * e + fv)], Stroke::new(0.5_f32, edge_col.gamma_multiply(0.35)));
                p.line_segment([proj(n - fu + fv * e), proj(n + fu + fv * e)], Stroke::new(0.5_f32, edge_col.gamma_multiply(0.35)));
            }
            let pts: Vec<Pos2> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().map(|(a, b): &(f32, f32)| proj(n + fu * *a + fv * *b)).collect();
            p.add(egui::Shape::closed_line(pts, Stroke::new(1.2_f32, edge_col)));
            // eticheta rotita odata cu fata (dupa axa U proiectata), niciodata cu capul in jos
            let vis_k = -n.dot(f);
            if vis_k > 0.30 {
                let su = proj(n + fu) - proj(n);
                let mut ang = su.y.atan2(su.x);
                if ang.cos() < 0.0 {
                    ang += std::f32::consts::PI;
                }
                let fs = (7.2 + 2.6 * vis_k).min(9.6);
                let tcol = if hit.map(|h| (h - n).length() < 1e-4).unwrap_or(false) { Color32::WHITE } else { Color32::from_rgb(52, 58, 70).gamma_multiply(0.45 + 0.55 * vis_k) };
                let galley = p.layout_no_wrap(label.to_string(), FontId::proportional(fs), tcol);
                let half = galley.size() * 0.5;
                let (s, co) = ang.sin_cos();
                let off = EV::new(half.x * co - half.y * s, half.x * s + half.y * co);
                p.add(egui::Shape::Text(egui::epaint::TextShape::new(proj(n) - off, galley, tcol).with_angle(ang)));
            }
        }
        for (tri, d) in &arrows {
            let on = hit.map(|h| (h - *d).length() < 1e-4).unwrap_or(false);
            p.add(egui::Shape::convex_polygon(tri.clone(), if on { accent() } else { edge_col }, Stroke::NONE));
        }
        for (txt, a, col) in [
            ("N", std::f32::consts::FRAC_PI_2, Color32::from_rgb(232, 76, 64)),
            ("E", 0.0, edge_col),
            ("S", -std::f32::consts::FRAC_PI_2, edge_col),
            ("W", std::f32::consts::PI, edge_col),
        ] {
            let q = ring_pt(a, 1.98);
            // literele se estompeaza cand inelul e vazut aproape din profil
            let fade = (f.z.abs() * 1.6).clamp(0.35, 1.0);
            p.text(q, Align2::CENTER_CENTER, txt, FontId::proportional(if txt == "N" { 10.0 } else { 8.5 }), col.gamma_multiply(fade));
        }
        // buton Home si meniu
        let hr = Rect::from_center_size(c + EV::new(-size * 1.7, -size * 1.6), EV::splat(20.0));
        let hres = ui.interact(hr, ui.id().with("vc_home"), Sense::click()).on_hover_text("Home (right-click the cube for more)");
        p.rect_filled(hr, 10.0, pal.overlay_btn);
        p.text(hr.center(), Align2::CENTER_CENTER, egui_phosphor::regular::HOUSE, crate::theme::icon_font(14.0), if hres.hovered() { accent() } else { pal.overlay_text });
        if hres.clicked() {
            self.go_home();
        }
        let pr = Rect::from_center_size(c + EV::new(size * 1.75, -size * 1.6), EV::new(38.0, 16.0));
        let pres = ui.interact(pr, ui.id().with("vc_proj"), Sense::click()).on_hover_text("Switch perspective / orthographic");
        p.rect_filled(pr, 8.0, pal.overlay_btn);
        p.text(pr.center(), Align2::CENTER_CENTER, if self.cam.ortho { "ORTHO" } else { "PERSP" }, FontId::proportional(8.0), if pres.hovered() { accent() } else { pal.overlay_text });
        if pres.clicked() {
            self.cam.ortho = !self.cam.ortho;
        }
        // View Templates sub cub (0.28): pastila cu ◀ ▶ (sablonul anterior / urmator, ca Navisworks Saved
        // Views) si in mijloc numele sablonului curent - click deschide fereastra View Templates
        let n = self.view_templates.len();
        let mut apply: Option<usize> = None;
        let cur_name = if n == 0 {
            "View templates".to_string()
        } else {
            let i = self.vt_current.min(n - 1);
            let t = &self.view_templates[i].name;
            let short: String = t.chars().take(14).collect();
            format!("{} · {}", i + 1, if t.chars().count() > 14 { format!("{short}…") } else { short })
        };
        let font = FontId::proportional(11.0);
        let name_w = p.layout_no_wrap(cur_name.clone(), font.clone(), Color32::WHITE).size().x;
        let (aw, ph) = (18.0, 20.0);
        let total = name_w + 16.0 + aw * 2.0;
        let mut pill = Rect::from_center_size(c + EV::new(0.0, size * 2.95), EV::new(total, ph));
        // cubul e langa marginea din dreapta: pastila nu iese din vederea 3D
        let over = pill.right() - (rect.right() - 6.0);
        if over > 0.0 {
            pill = pill.translate(EV::new(-over, 0.0));
        }
        p.rect_filled(pill, ph * 0.5, pal.overlay_btn);
        p.rect_stroke(pill, ph * 0.5, Stroke::new(1.0_f32, Color32::from_white_alpha(28)));
        let left = Rect::from_min_size(pill.min, EV::new(aw, ph));
        let right = Rect::from_min_size(Pos2::new(pill.max.x - aw, pill.min.y), EV::new(aw, ph));
        let mid = Rect::from_min_max(Pos2::new(left.max.x, pill.min.y), Pos2::new(right.min.x, pill.max.y));
        let lres = ui.interact(left, ui.id().with("vc_vt_prev"), Sense::click()).on_hover_text("Previous view template");
        let rres = ui.interact(right, ui.id().with("vc_vt_next"), Sense::click()).on_hover_text("Next view template");
        let mres = ui.interact(mid, ui.id().with("vc_vt_open"), Sense::click()).on_hover_text("View Templates: save, apply, update, choose what each applies");
        let on = n > 0;
        let arrow_col = |r: &egui::Response| if !on { pal.overlay_weak } else if r.hovered() { accent() } else { pal.overlay_text };
        p.text(left.center(), Align2::CENTER_CENTER, egui_phosphor::regular::CARET_LEFT, crate::theme::icon_font(12.0), arrow_col(&lres));
        p.text(right.center(), Align2::CENTER_CENTER, egui_phosphor::regular::CARET_RIGHT, crate::theme::icon_font(12.0), arrow_col(&rres));
        if mres.hovered() {
            p.rect_filled(mid.shrink2(EV::new(0.0, 2.0)), 6.0, accent().gamma_multiply(0.18));
        }
        p.text(mid.center(), Align2::CENTER_CENTER, &cur_name, font, if mres.hovered() { accent() } else { pal.overlay_text });
        p.line_segment([Pos2::new(left.max.x, pill.min.y + 4.0), Pos2::new(left.max.x, pill.max.y - 4.0)], Stroke::new(1.0_f32, Color32::from_white_alpha(25)));
        p.line_segment([Pos2::new(right.min.x, pill.min.y + 4.0), Pos2::new(right.min.x, pill.max.y - 4.0)], Stroke::new(1.0_f32, Color32::from_white_alpha(25)));
        if on && lres.clicked() {
            apply = Some((self.vt_current + n - 1) % n);
        }
        if on && rres.clicked() {
            apply = Some((self.vt_current + 1) % n);
        }
        if mres.clicked() {
            self.view_templates_open = !self.view_templates_open;
        }
        if let Some(i) = apply { self.vt_current = i; self.apply_view_template(i); }
        if resp.clicked() {
            if let Some(d) = hit {
                let mut t = self.cam;
                t.set_dir(d);
                let bb = self.scene_aabb();
                let (y, pt) = (t.yaw, t.pitch);
                t.set_view(y, pt, &bb);
                self.fly_to(t);
            }
        }
        if resp.hovered() {
            let tip = match hit {
                Some(_) => "Click: look from here · right-click: Home / projection",
                None => "Right-click: Home / projection",
            };
            resp.clone().on_hover_text(tip);
        }
        resp.context_menu(|ui| {
            if ui.button("🏠 Go Home").clicked() {
                self.go_home();
                ui.close_menu();
            }
            if ui.button("📌 Set Current View as Home").clicked() {
                self.home = Some(self.cam);
                self.toast("Current view set as Home (saved with the project).");
                ui.close_menu();
            }
            if ui.add_enabled(self.home.is_some(), egui::Button::new("Reset Home")).clicked() {
                self.home = None;
                ui.close_menu();
            }
            ui.separator();
            let mut o = self.cam.ortho;
            ui.radio_value(&mut o, false, "Perspective");
            ui.radio_value(&mut o, true, "Orthographic");
            self.cam.ortho = o;
            ui.separator();
            if ui.button("⛶ Fit to View").clicked() {
                let bb = self.scene_aabb();
                self.cam.fit(&bb);
                ui.close_menu();
            }
        });
    }

    // ================================================================== viewport
    pub(super) fn viewport(&mut self, ui: &mut egui::Ui, overlays: bool) {
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        // fundal: degrade pe trei benzi, cu o "linie de orizont" usor luminata (mai elegant decat doua culori)
        let mut mesh = egui::Mesh::default();
        let pal = self.pal();
        // Hidden line (Rhino): fundal alb ca hartia, ca sa se vada liniile negre.
        let rhino = self.settings.visual_style == crate::render::VisualStyle::Hidden;
        // fundalul ales in Settings › Appearance (implicit = al temei)
        let (top, bot) = match self.settings.vp_bg.as_str() {
            _ if rhino => (Color32::from_rgb(252, 252, 253), Color32::from_rgb(240, 242, 245)),
            // scena de noapte (Render › Lighting): cer inchis, lampile ies in evidenta
            _ if self.render.night => (Color32::from_rgb(8, 12, 26), Color32::from_rgb(22, 26, 40)),
            "white" => (Color32::from_rgb(255, 255, 255), Color32::from_rgb(246, 247, 249)),
            "grey" => (Color32::from_rgb(150, 155, 162), Color32::from_rgb(112, 116, 122)),
            "black" => (Color32::from_rgb(14, 15, 18), Color32::from_rgb(6, 6, 8)),
            "sky" => (Color32::from_rgb(120, 170, 225), Color32::from_rgb(222, 232, 242)),
            _ => (pal.vp_top, pal.vp_bottom),
        };
        let lift = |c: Color32, d: i16| {
            let f = |x: u8| (x as i16 + d).clamp(0, 255) as u8;
            Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
        };
        let dark = ui.visuals().dark_mode;
        let horizon = lift(bot, if dark { 10 } else { 6 });
        let floor = lift(bot, if dark { -6 } else { -10 });
        let hy = rect.top() + rect.height() * 0.62;
        for (y, c) in [(rect.top(), top), (hy, horizon), (rect.bottom(), floor)] {
            mesh.colored_vertex(Pos2::new(rect.left(), y), c);
            mesh.colored_vertex(Pos2::new(rect.right(), y), c);
        }
        mesh.add_triangle(0, 1, 3);
        mesh.add_triangle(0, 3, 2);
        mesh.add_triangle(2, 3, 5);
        mesh.add_triangle(2, 5, 4);
        painter.add(egui::Shape::mesh(mesh));
        // pagina de start: doar ea, fara model / cub / suprapuneri (proiectul ramane deschis dedesubt)
        if self.start_screen() {
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| self.start_page(ui));
            if self.show_start && ui.input(|i| i.key_pressed(Key::Escape)) && self.has_open_project() {
                self.show_start = false;
            }
            return;
        }

        let aspect = rect.width() / rect.height().max(1.0);
        let params = self.frame_params(aspect);
        // imaginea asteapta cateva cadre dupa ce s-au schimbat stilul / materialele (vezi still_wait)
        let capture = if self.still_wait == 0 { self.pending_capture.take() } else { None };
        if let Some((w, h)) = capture {
            let (bg_top, bg_bottom) = self.render_bg();
            let cp = self.frame_params(w as f32 / h.max(1) as f32);
            // Dashboard / AI: imaginea arata ce se vede in viewport (ascunsele si estomparea clash-ului incluse)
            let lists = (self.dash_job.is_some() || self.mcp_capture.is_some()).then(|| std::sync::Arc::new(self.draw_lists(&cp, h as f32)));
            self.renderer.lock().unwrap_or_else(|e| e.into_inner()).capture = Some(crate::render::CaptureReq { params: cp, w, h, bg_top, bg_bottom, lists });
        }
        // ce se deseneaza: campul vizual, fara ascunse; simplificat cat timp camera se misca.
        // Lista e capturata de callback (NU in renderer) - altfel, in split-view, al doilea
        // viewport o suprascrie pe prima si dispar elemente cand misc camera.
        let lists = self.draw_lists(&params, rect.height());
        if self.lod_moving() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(350));
        }
        let r = self.renderer.clone();
        // pagina de start peste proiect: modelul nu se mai deseneaza dedesubt
        if !self.start_screen() {
            let lists = std::sync::Arc::new(lists);
            painter.add(egui::PaintCallback {
                rect,
                callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |_info, p| {
                    let mut r = r.lock().unwrap_or_else(|e| e.into_inner());
                    unsafe { r.paint(p.gl(), &params, Some(&lists)) };
                })),
            });
        }
        if let Some(e) = self.renderer.lock().unwrap_or_else(|e| e.into_inner()).error.clone() {
            painter.text(rect.center(), Align2::CENTER_CENTER, e, FontId::proportional(14.0), Color32::RED);
        }

        // --------------------------------------------------------- interactiune
        let uv = |p: Pos2| Vec2::new((p.x - rect.left()) / rect.width(), (p.y - rect.top()) / rect.height());
        let mods = ui.input(|i| i.modifiers);
        let hover = resp.hover_pos();
        let mut snap: Option<Vec3> = None;
        // tipul punctului prins: 0 suprafata, 1 colt, 2 mijloc de muchie, 3 pe muchie
        let mut snap_kind = 0u8;
        // elementul si normala fetei sub cursor (relocare: aliniere pe fata)
        let mut snap_hit: Option<(ElemId, Vec3)> = None;
        if self.xform.is_some() && ui.input(|i| i.key_pressed(Key::Escape)) {
            self.xform = None;
        }
        if self.grid_pick > 0 && ui.input(|i| i.key_pressed(Key::Escape)) {
            self.grid_pick = 0;
        }
        // F3 = OSNAP on/off (ca in AutoCAD / Navisworks)
        if ui.input(|i| i.key_pressed(Key::F3)) {
            self.osnap_on = !self.osnap_on;
        }
        let picking = matches!(self.tool, Tool::Measure(_)) || self.grid_pick > 0 || self.xform.is_some();
        if let (true, Some(hp)) = (picking, hover) {
            let (o, d) = self.cam.ray(uv(hp), aspect);
            if let Some((hit_id, _, p, tri)) = self.pick(o, d) {
                snap_hit = Some((hit_id, (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or_zero()));
                let vpm = params.view_proj;
                let scr = |v: Vec3| to_screen(&vpm, rect, v).map(|s| s.distance(hp));
                let rad = self.settings.snap_radius;
                let types = if self.osnap_on { self.settings.snap_types } else { 0 };
                let mut best = (p, 0u8, f32::INFINITY);
                // Vertex (Endpoint) - raza completa
                if types & 1 != 0 {
                    for v in tri {
                        if let Some(dd) = scr(v) {
                            if dd < rad && dd < best.2 { best = (v, 1, dd); }
                        }
                    }
                }
                // Midpoint
                if types & 2 != 0 && best.1 == 0 {
                    for k in 0..3 {
                        let m = (tri[k] + tri[(k + 1) % 3]) * 0.5;
                        if let Some(dd) = scr(m) {
                            if dd < rad * 0.85 && dd < best.2 { best = (m, 2, dd); }
                        }
                    }
                }
                // Edge (Nearest pe muchie)
                if types & 4 != 0 && best.1 == 0 {
                    for k in 0..3 {
                        let (a, b) = (tri[k], tri[(k + 1) % 3]);
                        let ab = b - a;
                        let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
                        let q = a + ab * t;
                        if let Some(dd) = scr(q) {
                            if dd < rad * 0.6 && dd < best.2 { best = (q, 3, dd); }
                        }
                    }
                }
                snap = Some(best.0);
                snap_kind = best.1;
            }
        }
        self.vp_rect = [rect.left(), rect.top(), rect.width(), rect.height()];
        self.ppp = ui.ctx().pixels_per_point();
        if let Some(i) = self.active_vp {
            if !self.vp_matches(i) && self.report_job.is_none() {
                self.active_vp = None;
            }
        }
        let red_tool = match self.tool {
            Tool::Redline(k) => Some(k),
            _ => None,
        };
        if let Some(k) = red_tool {
            if k != crate::markup::RedKind::Text {
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    if let Some(p) = resp.interact_pointer_pos() {
                        let n = self.to_red(rect, p);
                        self.red_draft = Some(crate::markup::Redline { kind: k, pts: vec![n, n], color: self.red_color, width: self.red_width, text: String::new() });
                    }
                }
                if resp.dragged_by(egui::PointerButton::Primary) {
                    if let (Some(p), Some(dr)) = (resp.interact_pointer_pos(), self.red_draft.as_mut()) {
                        let n = [(p.x - rect.center().x) / rect.height(), (p.y - rect.center().y) / rect.height()];
                        if k == crate::markup::RedKind::Freehand {
                            let l = dr.pts[dr.pts.len() - 1];
                            if ((n[0] - l[0]).powi(2) + (n[1] - l[1]).powi(2)).sqrt() * rect.height() > 2.0 {
                                dr.pts.push(n);
                            }
                        } else {
                            dr.pts[1] = n;
                        }
                    }
                }
                if resp.drag_stopped() {
                    if let Some(dr) = self.red_draft.take() {
                        let a = dr.pts[0];
                        let b = dr.pts[dr.pts.len() - 1];
                        let size = ((a[0] - b[0]).abs() + (a[1] - b[1]).abs()) * rect.height();
                        if size > 4.0 || dr.pts.len() > 3 {
                            self.add_redline(dr);
                        }
                    }
                }
            }
        }
        // desenele 2D suprapuse (DWG / DXF)
        if !self.overlays.is_empty() && self.report_job.is_none() {
            self.draw_overlays(&painter, rect, &params.view_proj);
            self.draw_profile_legend(&painter, rect);
        }
        // section box ca in Revit: cate o sageata pe fiecare fata; tragi de ea si fata se muta pe axa ei
        let mut handle_hover: Option<usize> = None;
        if self.section.enabled && self.section.mode == SectionMode::Box && self.settings.show_section_box && self.report_job.is_none() {
            let vpm = params.view_proj;
            let (a, b) = (Vec3::from(self.section.bmin), Vec3::from(self.section.bmax));
            let c = (a + b) * 0.5;
            let size = (b - a).max_element().max(0.1);
            for k in 0..6 {
                let (ax, s) = (k / 2, if k % 2 == 0 { -1.0f32 } else { 1.0 });
                let mut fc = c;
                fc[ax] = if s < 0.0 { a[ax] } else { b[ax] };
                let mut e = Vec3::ZERO;
                e[ax] = s;
                let (Some(p0), Some(p1)) = (to_screen(&vpm, rect, fc), to_screen(&vpm, rect, fc + e * size * 0.06)) else { continue };
                // sageata mai mare, prinsa oriunde pe lungimea ei (zona de 16 px in jurul tijei si al varfului)
                let d = (p1 - p0).normalized();
                let ok = d.x.is_finite() && d.y.is_finite();
                let tip = if ok { p0 + d * 38.0 } else { p0 };
                let end = if ok { tip + d * 16.0 } else { p0 };
                let seg_dist = |h: Pos2| {
                    let ab = end - p0;
                    let t = ((h - p0).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                    h.distance(p0 + ab * t)
                };
                let hovered = hover.is_some_and(|h| rect.contains(h) && (seg_dist(h) < 16.0 || h.distance(p0) < 16.0));
                if hovered && self.box_drag.is_none() && handle_hover.is_none() {
                    handle_hover = Some(k);
                }
                let active = self.box_drag == Some(k) || (hovered && self.box_drag.is_none());
                let col = if active { Color32::from_rgb(255, 160, 40) } else { Color32::from_rgb(60, 140, 255) };
                if active {
                    // halou: se vede clar ce prinzi
                    painter.line_segment([p0, end], Stroke::new(14.0_f32, col.gamma_multiply(0.25)));
                }
                if ok {
                    painter.line_segment([p0, tip], Stroke::new(4.0_f32, col));
                    let n = EV::new(-d.y, d.x);
                    painter.add(egui::Shape::convex_polygon(vec![end, tip + n * 9.0, tip - n * 9.0], col, Stroke::new(1.5_f32, Color32::WHITE)));
                }
                painter.circle(p0, 7.0, col, Stroke::new(1.5_f32, Color32::WHITE));
            }
            if let Some(k) = handle_hover.or(self.box_drag) {
                let _ = k;
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
        }
        if resp.drag_started_by(egui::PointerButton::Primary) && handle_hover.is_some() {
            self.box_drag = handle_hover;
        }
        if let Some(k) = self.box_drag {
            if resp.dragged_by(egui::PointerButton::Primary) {
                let (ax, minf) = (k / 2, k % 2 == 0);
                let (a, b) = (Vec3::from(self.section.bmin), Vec3::from(self.section.bmax));
                let mut fc = (a + b) * 0.5;
                fc[ax] = if minf { a[ax] } else { b[ax] };
                let mut e = Vec3::ZERO;
                e[ax] = 1.0;
                // cati pixeli are 1 m pe axa, pe ecran: miscarea mouse-ului proiectata pe directia axei
                if let (Some(s0), Some(s1)) = (to_screen(&params.view_proj, rect, fc), to_screen(&params.view_proj, rect, fc + e)) {
                    let sv = s1 - s0;
                    let ppm = sv.length();
                    if ppm > 1e-3 {
                        let dm = resp.drag_delta().dot(sv / ppm) / ppm;
                        if minf {
                            self.section.bmin[ax] = (self.section.bmin[ax] + dm).min(self.section.bmax[ax] - 0.01);
                        } else {
                            self.section.bmax[ax] = (self.section.bmax[ax] + dm).max(self.section.bmin[ax] + 0.01);
                        }
                        self.section_level = None;
                    }
                }
            }
            if resp.drag_stopped() || !resp.dragged() {
                self.box_drag = None;
            }
        }
        // ---- obiectul de coordonare ales (fereastra Coordination objects deschisa): gizmo cu trei moduri,
        // ca la lumini - Resize (sageti pe fete, in axele obiectului rotit), Move (sageti X / Y / Z din
        // centru), Rotate (cerc in jurul verticalei). Modelul virtual se reface la eliberarea mouse-ului.
        let mut coord_hover: Option<usize> = None;
        let coord_edit = if self.coord_open { self.coord_sel.filter(|&i| i < self.coord_objects.len()) } else { None };
        if let Some(ci) = coord_edit {
            let vpm = params.view_proj;
            let (a, b) = (Vec3::from(self.coord_objects[ci].min), Vec3::from(self.coord_objects[ci].max));
            let c = (a + b) * 0.5;
            let h = (b - a) * 0.5;
            let rot = self.coord_objects[ci].rot_deg.to_radians();
            // axele locale ale obiectului (rotit in jurul verticalei)
            let ex = Vec3::new(rot.cos(), rot.sin(), 0.0);
            let ey = Vec3::new(-rot.sin(), rot.cos(), 0.0);
            let ez = Vec3::Z;
            let local = [ex, ey, ez];
            let kc = self.coord_objects[ci].kind.color();
            let base = Color32::from_rgb(kc[0], kc[1], kc[2]);
            let mode = self.coord_mode;
            // conturul cutiei rotite (vizibil si prin pereti)
            let corner = |i: u32| c + ex * if i & 1 == 0 { -h.x } else { h.x } + ey * if i & 2 == 0 { -h.y } else { h.y } + ez * if i & 4 == 0 { -h.z } else { h.z };
            for (i, j) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
                if let (Some(p), Some(q)) = (to_screen(&vpm, rect, corner(i)), to_screen(&vpm, rect, corner(j))) {
                    painter.line_segment([p, q], Stroke::new(1.8_f32, base));
                }
            }
            let dist = (c - self.cam.eye()).length().max(0.5);
            let probe_c = if resp.drag_started_by(egui::PointerButton::Primary) { ui.input(|i| i.pointer.press_origin()).or(hover) } else { hover };
            let axis_col = [Color32::from_rgb(240, 80, 70), Color32::from_rgb(80, 220, 100), Color32::from_rgb(80, 140, 255)];
            let seg_near = |p0: Pos2, end: Pos2, hp: Pos2| {
                let ab = end - p0;
                let t = ((hp - p0).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                hp.distance(p0 + ab * t)
            };
            // cercul de rotatie in planul orizontal al centrului
            let ring_r = dist * 0.14;
            let ring_pt = |t: f32| c + Vec3::new(t.cos(), t.sin(), 0.0) * ring_r;
            if mode == 2 {
                let mut pts = Vec::with_capacity(97);
                let mut best = f32::MAX;
                for k in 0..=96 {
                    if let Some(s) = to_screen(&vpm, rect, ring_pt(k as f32 / 96.0 * std::f32::consts::TAU)) {
                        if let Some(hp) = probe_c {
                            best = best.min(s.distance(hp));
                        }
                        pts.push(s);
                    }
                }
                if best < 12.0 && self.coord_drag.is_none() && handle_hover.is_none() {
                    coord_hover = Some(0);
                }
                let active = self.coord_drag.is_some() || coord_hover.is_some();
                let col = if active { Color32::from_rgb(255, 220, 60) } else { axis_col[2] };
                if pts.len() > 1 {
                    painter.add(egui::Shape::line(pts, Stroke::new(if active { 5.0_f32 } else { 3.0 }, col)));
                }
                // directia obiectului (axa locala X), ca sa se vada unghiul
                if let (Some(p0), Some(p1)) = (to_screen(&vpm, rect, c), to_screen(&vpm, rect, c + ex * ring_r)) {
                    painter.line_segment([p0, p1], Stroke::new(2.0_f32, col));
                    painter.text(p1 + EV::new(8.0, -8.0), Align2::LEFT_BOTTOM, format!("{:.1}°", self.coord_objects[ci].rot_deg), FontId::proportional(13.0), col);
                }
            } else {
                for k in 0..6usize {
                    let (ax, s) = (k / 2, if k % 2 == 0 { -1.0f32 } else { 1.0 });
                    // Resize: din centrul fiecarei fete, in axele locale; Move: din centru, in axele lumii (doar +)
                    let (start, dir) = if mode == 0 {
                        (c + local[ax] * h[ax] * s, local[ax] * s)
                    } else {
                        if s < 0.0 {
                            continue;
                        }
                        let mut e = Vec3::ZERO;
                        e[ax] = 1.0;
                        (c, e)
                    };
                    let len = if mode == 0 { dist * 0.06 } else { dist * 0.13 };
                    let (Some(p0), Some(p1)) = (to_screen(&vpm, rect, start), to_screen(&vpm, rect, start + dir * len)) else { continue };
                    let d = (p1 - p0).normalized();
                    if !(d.x.is_finite() && d.y.is_finite()) {
                        continue;
                    }
                    let end = if mode == 0 { p0 + d * 44.0 } else { p1 };
                    let tip = end - d * 14.0;
                    if probe_c.is_some_and(|hp| rect.contains(hp) && seg_near(p0, end, hp) < 13.0) && self.coord_drag.is_none() && coord_hover.is_none() && handle_hover.is_none() {
                        coord_hover = Some(k);
                    }
                    let active = self.coord_drag == Some(k) || (coord_hover == Some(k) && self.coord_drag.is_none());
                    let col = if active { Color32::from_rgb(255, 220, 60) } else if mode == 1 { axis_col[ax] } else { base };
                    painter.line_segment([p0, tip], Stroke::new(if active { 5.0_f32 } else { 3.5 }, col));
                    let n = EV::new(-d.y, d.x);
                    painter.add(egui::Shape::convex_polygon(vec![end, tip + n * 8.0, tip - n * 8.0], col, Stroke::new(1.2_f32, Color32::WHITE)));
                    if mode == 1 {
                        painter.text(end + d * 12.0, Align2::CENTER_CENTER, ["X", "Y", "Z"][ax], FontId::proportional(14.0), col);
                    } else {
                        painter.circle(p0, 5.0, col, Stroke::new(1.2_f32, Color32::WHITE));
                    }
                }
            }
            if coord_hover.is_some() || self.coord_drag.is_some() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            // unghiul cursorului in planul orizontal al centrului (rotire)
            let cam_r = self.cam;
            let ang_at = |hp: Pos2| -> Option<f32> {
                let (ro, rd) = cam_r.ray(uv(hp), aspect);
                if rd.z.abs() < 1e-4 {
                    return None;
                }
                let t = (c.z - ro.z) / rd.z;
                if t <= 0.0 {
                    return None;
                }
                let q = ro + rd * t - c;
                Some(q.y.atan2(q.x))
            };
            if resp.drag_started_by(egui::PointerButton::Primary) && coord_hover.is_some() {
                self.coord_drag = coord_hover;
                self.coord_rot_last = if mode == 2 { probe_c.and_then(ang_at) } else { None };
            }
            if let Some(k) = self.coord_drag {
                if resp.dragged_by(egui::PointerButton::Primary) {
                    let o = &mut self.coord_objects[ci];
                    if mode == 2 {
                        if let (Some(hp), Some(last)) = (resp.interact_pointer_pos(), self.coord_rot_last) {
                            if let Some(a1) = ang_at(hp) {
                                let mut da = a1 - last;
                                while da > std::f32::consts::PI { da -= std::f32::consts::TAU; }
                                while da < -std::f32::consts::PI { da += std::f32::consts::TAU; }
                                o.rot_deg = (o.rot_deg + da.to_degrees()).rem_euclid(360.0);
                                // Shift: pasi de 15 grade (ca in Revit / AutoCAD)
                                self.coord_rot_last = Some(a1);
                            }
                        }
                    } else {
                        let (ax, minf) = (k / 2, k % 2 == 0);
                        let dir = if mode == 0 { local[ax] } else { let mut e = Vec3::ZERO; e[ax] = 1.0; e };
                        let start = if mode == 0 { c + local[ax] * h[ax] * if minf { -1.0 } else { 1.0 } } else { c };
                        if let (Some(s0), Some(s1)) = (to_screen(&vpm, rect, start), to_screen(&vpm, rect, start + dir)) {
                            let sv = s1 - s0;
                            let ppm = sv.length();
                            if ppm > 1e-3 {
                                let dm = resp.drag_delta().dot(sv / ppm) / ppm;
                                if dm.is_finite() {
                                    let (nc, mut nh);
                                    nh = h;
                                    if mode == 0 {
                                        // fata se muta cu dm, fata opusa ramane pe loc
                                        let sgn = if minf { -1.0 } else { 1.0 };
                                        let grow = (sgn * dm).max(0.005 - h[ax] * 2.0);
                                        nh[ax] = (h[ax] + grow * 0.5).max(0.005);
                                        nc = c + local[ax] * sgn * grow * 0.5;
                                    } else {
                                        nc = c + dir * dm;
                                    }
                                    o.min = (nc - nh).to_array();
                                    o.max = (nc + nh).to_array();
                                }
                            }
                        }
                    }
                }
                if !resp.dragged() {
                    self.coord_drag = None;
                    self.coord_rot_last = None;
                    if mode == 2 && ui.input(|i| i.modifiers.shift) {
                        let o = &mut self.coord_objects[ci];
                        o.rot_deg = (o.rot_deg / 15.0).round() * 15.0 % 360.0;
                    }
                    self.rebuild_coord_model();
                }
            }
        }
        // ---- lumini: gizmo in 3D. Move = sagetile X/Y/Z, Rotate = cercurile XY / XZ / YZ.
        // Testul de atingere si inceputul tragerii se fac INAINTE de navigare: altfel orbitul misca
        // camera in acelasi cadru cu lumina, iar cercurile fug de sub cursor (rotirea nu mergea).
        let cam_c = self.cam;
        // (indice, pozitie, lungimea sagetilor, raza cercurilor de rotire) - marimi constante pe ecran
        let light_g = self.light_sel.filter(|&i| i < self.lights.len() && self.lights[i].enabled).map(|i| {
            let lp = Vec3::from(self.lights[i].pos);
            let dist = (lp - cam_c.eye()).length().max(0.5);
            (i, lp, dist * 0.13, dist * 0.16)
        });
        let unit = |a: usize| {
            let mut v = Vec3::ZERO;
            v[a] = 1.0;
            v
        };
        // unghiul cursorului in jurul luminii, in planul perpendicular pe axa `ai`.
        // Normal: raza mouse-ului intersectata cu planul (exact, sens corect). Plan vazut din profil:
        // unghiul pe ecran, cu semnul dat de orientarea axei fata de camera.
        let ring_angle = |lp: Vec3, ai: usize, ptr: Pos2, screen: bool| -> Option<f32> {
            let ax = unit(ai);
            if !screen {
                let (u, v) = match ai { 0 => (Vec3::Y, Vec3::Z), 1 => (Vec3::Z, Vec3::X), _ => (Vec3::X, Vec3::Y) };
                let (o, d) = cam_c.ray(uv(ptr), aspect);
                let den = d.dot(ax);
                if den.abs() < 1e-4 {
                    return None;
                }
                let t = (lp - o).dot(ax) / den;
                if t <= 0.0 {
                    return None;
                }
                let h = o + d * t - lp;
                return Some(h.dot(v).atan2(h.dot(u)));
            }
            let sp = to_screen(&params.view_proj, rect, lp)?;
            let q = ptr - sp;
            if q.length() < 4.0 {
                return None;
            }
            let s = if ax.dot(-cam_c.fwd()) >= 0.0 { -1.0 } else { 1.0 };
            Some(s * q.y.atan2(q.x))
        };
        let ring_point = |lp: Vec3, plane: usize, r: f32, t: f32| -> Vec3 {
            let (u, v) = (t.cos() * r, t.sin() * r);
            match plane {
                0 => lp + Vec3::new(u, v, 0.0),  // XY -> rotire in jurul Z
                1 => lp + Vec3::new(u, 0.0, v),  // XZ -> in jurul Y
                _ => lp + Vec3::new(0.0, u, v), // YZ -> in jurul X
            }
        };
        let drag_start = resp.drag_started_by(egui::PointerButton::Primary);
        // la inceputul tragerii testam punctul unde s-a apasat butonul, nu unde a ajuns mouse-ul
        let probe = if drag_start { ui.input(|i| i.pointer.press_origin()).or(hover) } else { hover };
        let mut light_axis_hover: Option<usize> = None;
        let mut light_ring_hover: Option<usize> = None;
        if let (Some((_, lp, len, rr)), Some(ptr)) = (light_g, probe) {
            if self.light_axis.is_none() && self.light_rot.is_none() && rect.contains(ptr) {
                if !self.light_rotate_mode {
                    let mut best = 16.0f32;
                    for a in 0..3 {
                        if let (Some(sp), Some(tp)) = (to_screen(&params.view_proj, rect, lp), to_screen(&params.view_proj, rect, lp + unit(a) * len)) {
                            let v = tp - sp;
                            if v.length() > 1.0 {
                                let t = ((ptr - sp).dot(v) / v.dot(v)).clamp(0.0, 1.0);
                                let dd = (ptr - (sp + v * t)).length();
                                if dd < best {
                                    best = dd;
                                    light_axis_hover = Some(a);
                                }
                            }
                        }
                    }
                } else {
                    let mut best = 14.0f32;
                    for plane in 0..3 {
                        for k in 0..96 {
                            let t = k as f32 / 96.0 * std::f32::consts::TAU;
                            if let Some(s) = to_screen(&params.view_proj, rect, ring_point(lp, plane, rr, t)) {
                                let dd = (s - ptr).length();
                                if dd < best {
                                    best = dd;
                                    light_ring_hover = Some(plane);
                                }
                            }
                        }
                    }
                }
            }
        }
        if light_axis_hover.is_some() || light_ring_hover.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        if drag_start {
            if let (Some((_, lp, ..)), Some(ptr)) = (light_g, probe) {
                if let Some(a) = light_axis_hover {
                    self.light_axis = Some(a);
                } else if let Some(plane) = light_ring_hover {
                    // o lumina punctuala nu are directie: o transformam in Spot, ca rotirea sa aiba efect
                    if let Some((li, ..)) = light_g {
                        if self.lights[li].kind == crate::lights::LightKind::Point {
                            self.lights[li].kind = crate::lights::LightKind::Spot;
                            self.lights[li].angle = self.lights[li].angle.max(60.0);
                            if self.lights[li].name.starts_with("Point light") {
                                self.lights[li].name = self.lights[li].name.replacen("Point light", "Spot light", 1);
                            }
                            self.toast("Point light changed to a Spot light so it can be aimed");
                        }
                    }
                    let ai = [2usize, 1, 0][plane];
                    // planul aproape din profil (raza paralela cu el): masuram unghiul pe ecran
                    let (_, d) = cam_c.ray(uv(ptr), aspect);
                    let screen = d.dot(unit(ai)).abs() < 0.2;
                    if let Some(a0) = ring_angle(lp, ai, ptr, screen) {
                        self.light_rot = Some((ai, a0, screen));
                    }
                }
            }
        }
        if resp.dragged_by(egui::PointerButton::Primary) {
            if let (Some((li, lp, len, _)), Some(a)) = (light_g, self.light_axis) {
                if let (Some(sp), Some(tp)) = (to_screen(&params.view_proj, rect, lp), to_screen(&params.view_proj, rect, lp + unit(a) * len)) {
                    let v = tp - sp;
                    if v.length() > 1.0 {
                        let meters = resp.drag_delta().dot(v) / v.dot(v) * len;
                        if meters.is_finite() && meters.abs() > 1e-5 {
                            self.lights[li].pos[a] += meters;
                            self.lights[li].target[a] += meters;
                        }
                    }
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            if let (Some((li, lp, ..)), Some((ai, last, screen)), Some(ptr)) = (light_g, self.light_rot, resp.interact_pointer_pos()) {
                if let Some(a) = ring_angle(lp, ai, ptr, screen) {
                    // diferenta adusa in [-pi, pi]: fara salt cand unghiul trece prin +-180°
                    let mut da = a - last;
                    while da > std::f32::consts::PI {
                        da -= std::f32::consts::TAU;
                    }
                    while da < -std::f32::consts::PI {
                        da += std::f32::consts::TAU;
                    }
                    self.lights[li].rotate_direction(unit(ai), da);
                    self.light_rot = Some((ai, a, screen));
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
        }
        if !resp.dragged() {
            self.light_axis = None;
            self.light_rot = None;
        }
        // ---- traseul de mers (unealta Draw): punctele se trag pe podea, click dreapta pe punct = sterge
        if self.tool == Tool::WalkPath {
            let vpm = params.view_proj;
            let mut wp_hover: Option<usize> = None;
            if let Some(hp) = hover {
                let mut bd = 10.0f32;
                for (i, q) in self.walk_path.iter().enumerate() {
                    if let Some(s) = to_screen(&vpm, rect, *q + Vec3::Z * 0.05) {
                        let d = s.distance(hp);
                        if d < bd {
                            bd = d;
                            wp_hover = Some(i);
                        }
                    }
                }
            }
            if wp_hover.is_some() || self.wp_drag.is_some() {
                ui.ctx().set_cursor_icon(if self.wp_drag.is_some() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
            }
            if resp.drag_started_by(egui::PointerButton::Primary) && wp_hover.is_some() {
                self.wp_drag = wp_hover;
            }
            if let Some(i) = self.wp_drag {
                if resp.dragged_by(egui::PointerButton::Primary) {
                    if let Some(hp) = resp.interact_pointer_pos() {
                        if let Some(hit) = self.walk_floor_hit(uv(hp), aspect) {
                            if i < self.walk_path.len() {
                                self.walk_path[i] = hit;
                                self.path_run = None;
                            }
                        }
                    }
                }
                if !resp.dragged() {
                    self.wp_drag = None;
                }
            }
            if resp.secondary_clicked() {
                if let Some(i) = wp_hover {
                    self.walk_path.remove(i);
                    self.path_run = None;
                }
            }
        } else {
            self.wp_drag = None;
        }
        // gizmo-ul de lumina are prioritate: nu lasam orbitul / panul sa miste camera in acelasi timp
        let light_handle = self.wp_drag.is_some() ||self.light_axis.is_some() || self.light_rot.is_some() || self.tool == Tool::PlaceLight;
        let handle_drag = self.box_drag.is_some() || light_handle || self.coord_drag.is_some() || (handle_hover.is_some() && resp.drag_started_by(egui::PointerButton::Primary));
        // selectie cu fereastra (unealta Window): tragerea cu butonul stang deseneaza dreptunghiul
        let box_tool = self.tool == Tool::Window && !handle_drag;
        if box_tool {
            if resp.drag_started_by(egui::PointerButton::Primary) {
                self.box_start = resp.interact_pointer_pos().map(|p| [p.x, p.y]);
            }
            if let (Some(s), Some(p)) = (self.box_start, resp.interact_pointer_pos().or(hover)) {
                let a = Pos2::new(s[0], s[1]);
                let r = Rect::from_two_pos(a, p);
                // stanga -> dreapta: fereastra (albastru, continuu); dreapta -> stanga: crossing (verde, punctat)
                let crossing = p.x < a.x;
                let (fill, col) = if crossing { (Color32::from_rgba_unmultiplied(80, 200, 110, 30), Color32::from_rgb(80, 200, 110)) } else { (Color32::from_rgba_unmultiplied(80, 140, 255, 30), Color32::from_rgb(80, 140, 255)) };
                painter.rect_filled(r, 0.0, fill);
                if crossing {
                    let c = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
                    painter.extend(egui::Shape::dashed_line(&c, Stroke::new(1.2_f32, col), 6.0, 4.0));
                } else {
                    painter.rect_stroke(r, 0.0, Stroke::new(1.2_f32, col));
                }
                if resp.drag_stopped() {
                    self.box_start = None;
                    if r.width() > 3.0 && r.height() > 3.0 {
                        self.select_in_window(&params.view_proj, rect, r, crossing, mods.command || mods.ctrl);
                    }
                }
            }
            if !resp.dragged() && !resp.drag_stopped() {
                self.box_start = None;
            }
        }
        let walk = self.nav == NavMode::Walk && red_tool.is_none() && !box_tool && !handle_drag;
        if walk && resp.drag_started_by(egui::PointerButton::Primary) && !mods.shift {
            self.walk_drag = resp.interact_pointer_pos().map(|p| [p.x, p.y]);
        }
        if !resp.dragged() || !walk {
            self.walk_drag = None;
        }
        if let (Some(o), Some(p)) = (self.walk_drag, resp.interact_pointer_pos().or(hover)) {
            // mers ca in Navisworks: tragi in sus = inainte, lateral = rotire (joystick).
            // Raspuns progresiv (zona moarta + curba patratica, plafonat la 2x viteza de mers),
            // accelerarea / franarea lina o face walk_tick.
            let off = Vec2::new(p.x - o[0], p.y - o[1]);
            let fwd = crate::app::walk::joystick(-off.y, 10.0, 120.0, 2.0);
            let boost = if mods.ctrl { 2.0 } else { 1.0 };
            self.walk_target += Vec3::new(0.0, fwd * boost, 0.0);
            self.walk_turn_target += -crate::app::walk::joystick(off.x, 10.0, 170.0, 1.0);
            self.path_run = None;
            let c = Pos2::new(o[0], o[1]);
            painter.circle_stroke(c, 5.0, Stroke::new(1.5_f32, accent()));
            painter.line_segment([c, p], Stroke::new(1.5_f32, accent().gamma_multiply(0.7)));
            ui.ctx().request_repaint();
        }
        if red_tool.is_some() {
            // primary drag deseneaza; doar middle / right navigheaza
        }
        if !walk && ((red_tool.is_none() && !box_tool && !handle_drag && resp.drag_started_by(egui::PointerButton::Primary)) || resp.drag_started_by(egui::PointerButton::Middle)) {
            self.pivot = None;
            if let Some(p) = resp.interact_pointer_pos() {
                let (o, d) = self.cam.ray(uv(p), aspect);
                if let Some((_, t, hit, _)) = self.pick(o, d) {
                    self.pivot = Some(hit);
                    if self.cam.ortho {
                        // ortho: `focus` da scara vederii - nu o schimbam (raza porneste mult in spatele
                        // camerei, iar distanta ei ar mari brusc vederea). Mutam doar ochiul de-a lungul
                        // directiei de privire, ca punctul ales sa fie la distanta `focus` (imaginea nu se schimba).
                        let f = self.cam.fwd();
                        let along = (hit - self.cam.eye()).dot(f);
                        self.cam.eye = (self.cam.eye() + f * (along - self.cam.focus)).into();
                    } else {
                        self.cam.focus = t * d.dot(self.cam.fwd()).max(0.05);
                    }
                }
            }
        }
        let delta = resp.drag_delta();
        if (walk || red_tool.is_some() || box_tool || handle_drag) && resp.dragged_by(egui::PointerButton::Primary) && !mods.shift {
            // handled above
        } else if resp.dragged_by(egui::PointerButton::Primary) && !mods.shift && self.nav == NavMode::Pan {
            // modul Pan: modelul urmeaza mouse-ul
            self.cam_anim = None;
            self.cam.pan(Vec2::new(delta.x, delta.y), rect.height());
        } else if resp.dragged_by(egui::PointerButton::Primary) && !mods.shift {
            let pv = self.pivot.unwrap_or_else(|| self.cam.target());
            self.cam_anim = None;
            self.cam.orbit(pv, -delta.x * 0.008, -delta.y * 0.008);
            if let Some(sp) = to_screen(&params.view_proj, rect, pv) {
                painter.circle_filled(sp, 4.0, accent());
                painter.circle_stroke(sp, 9.0, Stroke::new(1.5_f32, accent().gamma_multiply(0.6)));
            }
        } else if resp.dragged_by(egui::PointerButton::Middle) || (resp.dragged_by(egui::PointerButton::Primary) && mods.shift) {
            self.cam_anim = None;
            self.cam.pan(Vec2::new(delta.x, delta.y), rect.height());
        } else if resp.dragged_by(egui::PointerButton::Secondary) {
            self.cam.look(-delta.x * 0.005, -delta.y * 0.005);
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.1 {
                let hp = hover.unwrap_or(rect.center());
                let (_, d) = self.cam.ray(uv(hp), aspect);
                let bb = self.scene_aabb();
                let ms = if bb.is_valid() { bb.size().length() * 0.002 } else { 0.01 };
                self.cam_anim = None;
                // ~20% pe treapta de rotita (inainte ~32%: pe norii densi parea ca vederea "sare")
                self.cam.zoom(scroll / 85.0, d, ms);
            }
        }
        // alegerea originii / directiei axelor are prioritate fata de unealta curenta
        // relocare: punctele alese (cu snap) si normala fetei
        let xform_click = resp.clicked() && self.xform.is_some() && snap.is_some();
        if xform_click {
            if let (Some(s), Some((id, n))) = (snap, snap_hit) {
                // normala spre camera (fata vazuta)
                let n = if n.dot(self.cam.eye() - s) < 0.0 { -n } else { n };
                self.xform_point(s, n, id);
            }
        }
        // ---- lumini: desenare. Fiecare lampa = bec cu halou si raze (se vede clar pe orice fundal si
        // prin pereti), eticheta cu numele si puterea. Spot / Distant au sageata de directie (Spot si conul).
        // Lumina selectata are gizmo-ul deasupra: sageti X/Y/Z (Move) sau cercuri de rotire (Rotate).
        let axis_col = [Color32::from_rgb(240, 80, 70), Color32::from_rgb(80, 220, 100), Color32::from_rgb(80, 140, 255)];
        let tau = std::f32::consts::TAU;
        for (li, lamp) in self.lights.iter().enumerate() {
            let lp = Vec3::from(lamp.pos);
            let Some(sp) = to_screen(&params.view_proj, rect, lp) else { continue };
            if !rect.expand(60.0).contains(sp) {
                continue;
            }
            let selected = self.light_sel == Some(li);
            if !lamp.enabled {
                painter.circle_stroke(sp, 6.0, Stroke::new(1.5_f32, Color32::from_gray(140)));
                painter.text(sp + EV::new(10.0, -18.0), Align2::LEFT_TOP, format!("{} (off)", lamp.name), FontId::proportional(12.0), Color32::from_gray(150));
                continue;
            }
            let lc = Color32::from_rgb(lamp.color[0], lamp.color[1], lamp.color[2]);
            let dist = (lp - cam_c.eye()).length().max(0.5);
            // halou + raze + bec
            let sz = (6.0 + lamp.power.clamp(0.0, 20.0) * 0.45).clamp(7.0, 15.0);
            for (k, a) in [(3.4f32, 22u8), (2.5, 40), (1.7, 70)] {
                painter.circle_filled(sp, sz * k, Color32::from_rgba_unmultiplied(lc.r(), lc.g(), lc.b(), a));
            }
            for k in 0..8 {
                let t = k as f32 * tau / 8.0;
                let d = EV::new(t.cos(), t.sin());
                painter.line_segment([sp + d * (sz + 3.0), sp + d * (sz + 9.0)], Stroke::new(2.0_f32, lc));
            }
            painter.circle_filled(sp, sz, lc);
            painter.circle_filled(sp, sz * 0.45, Color32::WHITE);
            painter.circle_stroke(sp, sz, Stroke::new(if selected { 2.5_f32 } else { 1.5 }, if selected { accent() } else { Color32::BLACK }));
            // directia: sageata de lungime constanta pe ecran; Spot: si conul de lumina
            if lamp.kind != crate::lights::LightKind::Point {
                let dir = lamp.direction();
                if let Some(tp) = to_screen(&params.view_proj, rect, lp + dir * dist * 0.22) {
                    let v = tp - sp;
                    if v.length() > 2.0 {
                        let dn = v / v.length();
                        let perp = EV::new(-dn.y, dn.x);
                        painter.line_segment([sp, tp], Stroke::new(3.0_f32, lc));
                        painter.add(egui::Shape::convex_polygon(vec![tp + dn * 4.0, tp - dn * 14.0 + perp * 7.0, tp - dn * 14.0 - perp * 7.0], lc, Stroke::new(1.0_f32, Color32::BLACK)));
                    }
                }
                if lamp.kind == crate::lights::LightKind::Spot {
                    let half = (lamp.angle.clamp(5.0, 120.0) * 0.5).to_radians().min(1.25);
                    let cl = dist * 0.18;
                    let cr = cl * half.tan();
                    let u = dir.any_orthonormal_vector();
                    let w = dir.cross(u);
                    let c = lp + dir * cl;
                    let pts: Vec<Pos2> = (0..=48)
                        .filter_map(|k| {
                            let t = k as f32 / 48.0 * tau;
                            to_screen(&params.view_proj, rect, c + (u * t.cos() + w * t.sin()) * cr)
                        })
                        .collect();
                    if pts.len() > 1 {
                        for k in [0usize, 12, 24, 36] {
                            if let Some(q) = pts.get(k) {
                                painter.line_segment([sp, *q], Stroke::new(1.2_f32, lc.gamma_multiply(0.75)));
                            }
                        }
                        painter.add(egui::Shape::line(pts, Stroke::new(1.5_f32, lc.gamma_multiply(0.85))));
                    }
                }
            }
            if selected {
                if self.light_rotate_mode {
                    let rr = dist * 0.16;
                    let rotating = self.light_rot.map(|r| r.0);
                    for plane in 0..3 {
                        let pts: Vec<Pos2> = (0..=96).filter_map(|k| to_screen(&params.view_proj, rect, ring_point(lp, plane, rr, k as f32 / 96.0 * tau))).collect();
                        if pts.len() > 1 {
                            // culoarea cercului = culoarea axei in jurul careia roteste (XY -> Z albastru ...)
                            let ai = [2usize, 1, 0][plane];
                            let active = light_ring_hover == Some(plane) || rotating == Some(ai);
                            let rc = if active { Color32::from_rgb(255, 220, 60) } else { axis_col[ai] };
                            let rw: f32 = if active { 5.0 } else { 3.0 };
                            painter.add(egui::Shape::line(pts, Stroke::new(rw, rc)));
                        }
                    }
                } else {
                    let len = dist * 0.13;
                    for a in 0..3 {
                        if let Some(tp) = to_screen(&params.view_proj, rect, lp + unit(a) * len) {
                            let v = tp - sp;
                            if v.length() > 1.0 {
                                let active = self.light_axis == Some(a) || light_axis_hover == Some(a);
                                let w: f32 = if active { 6.0 } else { 4.0 };
                                let c = if active { Color32::from_rgb(255, 220, 60) } else { axis_col[a] };
                                painter.line_segment([sp, tp], Stroke::new(w, c));
                                let dn = v / v.length();
                                let perp = EV::new(-dn.y, dn.x);
                                painter.add(egui::Shape::convex_polygon(vec![tp + dn * 6.0, tp - dn * 14.0 + perp * 8.0, tp - dn * 14.0 - perp * 8.0], c, Stroke::NONE));
                                painter.text(tp + dn * 18.0, Align2::CENTER_CENTER, ["X", "Y", "Z"][a], FontId::proportional(15.0), c);
                            }
                        }
                    }
                }
            }
            // eticheta pe fond inchis: lizibila pe model alb sau negru
            let g = painter.layout_no_wrap(format!("{} · {:.1} lux", lamp.name, lamp.power), FontId::proportional(13.0), Color32::WHITE);
            let pos = sp + EV::new(sz + 12.0, -sz - 20.0);
            painter.rect_filled(Rect::from_min_size(pos - EV::splat(4.0), g.size() + EV::splat(8.0)), 4.0, Color32::from_black_alpha(175));
            painter.galley(pos, g, Color32::WHITE);
        }
        // in modul Rotate: directia curenta (azimut / inclinare) sub lumina
        if let (Some((li, lp, ..)), true) = (light_g, self.light_rotate_mode) {
            if let Some(sp) = to_screen(&params.view_proj, rect, lp) {
                let lamp = &self.lights[li];
                let msg = if lamp.kind == crate::lights::LightKind::Point {
                    "Point light: drag a circle to turn it into a Spot light and aim it".to_string()
                } else {
                    let (az, tilt) = lamp.aim_angles();
                    format!("Azimuth {az:.0}° · Tilt {tilt:.0}°  (drag a circle)")
                };
                let g = painter.layout_no_wrap(msg, FontId::proportional(12.5), Color32::WHITE);
                let pos = sp + EV::new(-g.size().x * 0.5, 34.0);
                painter.rect_filled(Rect::from_min_size(pos - EV::splat(4.0), g.size() + EV::splat(8.0)), 4.0, Color32::from_black_alpha(190));
                painter.galley(pos, g, Color32::WHITE);
            }
        }
        let grid_click = resp.clicked() && self.grid_pick > 0 && snap.is_some() && !xform_click;
        if grid_click {
            if let Some(s) = snap {
                self.grid_pick_point(s);
            }
        }
        // ---- alegerea unei fete a cubului de sectiune pentru desenul 2D (Annotation › Pick face):
        // fata de sub cursor se coloreaza verde deschis; click = vederea din acea fata (Top / Front ...)
        let box_on = self.section.enabled && self.section.mode == SectionMode::Box;
        if self.face_pick && (!box_on || ui.input(|i| i.key_pressed(Key::Escape))) {
            self.face_pick = false;
        }
        let mut face_hover: Option<(usize, bool)> = None;
        if self.face_pick && box_on {
            if let Some(h) = hover.filter(|h| rect.contains(*h)) {
                let (o, d) = self.cam.ray(uv(h), aspect);
                let (a, b) = (Vec3::from(self.section.bmin), Vec3::from(self.section.bmax));
                // slab method: fata prin care intra raza (sau iese, daca ochiul e in cub)
                let (mut t0, mut t1) = (f32::MIN, f32::MAX);
                let (mut f0, mut f1) = ((0usize, false), (0usize, false));
                let mut ok = true;
                for ax in 0..3 {
                    if d[ax].abs() < 1e-9 {
                        if o[ax] < a[ax] || o[ax] > b[ax] {
                            ok = false;
                        }
                        continue;
                    }
                    let (ta, tb) = ((a[ax] - o[ax]) / d[ax], (b[ax] - o[ax]) / d[ax]);
                    let (near, far, near_max) = if ta < tb { (ta, tb, false) } else { (tb, ta, true) };
                    if near > t0 {
                        t0 = near;
                        f0 = (ax, near_max);
                    }
                    if far < t1 {
                        t1 = far;
                        f1 = (ax, !near_max);
                    }
                }
                if ok && t0 <= t1 && t1 > 0.0 {
                    face_hover = Some(if t0 > 0.0 { f0 } else { f1 });
                }
            }
            ui.ctx().set_cursor_icon(if face_hover.is_some() { egui::CursorIcon::PointingHand } else { egui::CursorIcon::Crosshair });
        }
        // fata aleasa (vederea curenta) si fata de sub cursor: verde deschis, transparent
        if box_on {
            let (a, b) = (Vec3::from(self.section.bmin), Vec3::from(self.section.bmax));
            let chosen = self.settings.cad_view.axis().map(|ax| (ax, matches!(self.settings.cad_view, crate::app::CadView::Back | crate::app::CadView::Right | crate::app::CadView::Top)));
            let face_quad = |ax: usize, max: bool| -> [Vec3; 4] {
                let v = if max { b[ax] } else { a[ax] };
                match ax {
                    0 => [Vec3::new(v, a.y, a.z), Vec3::new(v, b.y, a.z), Vec3::new(v, b.y, b.z), Vec3::new(v, a.y, b.z)],
                    1 => [Vec3::new(a.x, v, a.z), Vec3::new(b.x, v, a.z), Vec3::new(b.x, v, b.z), Vec3::new(a.x, v, b.z)],
                    _ => [Vec3::new(a.x, a.y, v), Vec3::new(b.x, a.y, v), Vec3::new(b.x, b.y, v), Vec3::new(a.x, b.y, v)],
                }
            };
            // fata aleasa e desenata de planul de taiere (verde, urmeaza si tragerea lui); aici doar hover-ul
            let _ = chosen;
            for (f, alpha, w) in [(face_hover, 80u8, 3.0f32)] {
                let Some((ax, max)) = f else { continue };
                let q = face_quad(ax, max);
                let pts: Option<Vec<Pos2>> = q.iter().map(|p| to_screen(&params.view_proj, rect, *p)).collect();
                if let Some(pts) = pts {
                    painter.add(egui::Shape::convex_polygon(pts.clone(), Color32::from_rgba_unmultiplied(120, 230, 120, alpha), Stroke::NONE));
                    painter.add(egui::Shape::closed_line(pts, Stroke::new(w, Color32::from_rgb(90, 210, 100))));
                }
            }
            if let Some((ax, max)) = face_hover {
                let name = match (ax, max) { (0, false) => "Left", (0, true) => "Right", (1, false) => "Front", (1, true) => "Back", (2, false) => "Bottom", _ => "Top" };
                if let Some(h) = hover {
                    let g = painter.layout_no_wrap(format!("{name} face · click for the 2D drawing"), FontId::proportional(13.0), Color32::WHITE);
                    let pos = h + EV::new(18.0, 18.0);
                    painter.rect_filled(Rect::from_min_size(pos - EV::splat(5.0), g.size() + EV::splat(10.0)), 5.0, Color32::from_rgba_unmultiplied(20, 70, 35, 230));
                    painter.galley(pos, g, Color32::WHITE);
                }
            }
        }
        let face_click = resp.clicked() && self.face_pick && face_hover.is_some();
        if face_click {
            if let Some((ax, max)) = face_hover {
                use crate::app::CadView;
                self.settings.cad_view = match (ax, max) { (0, false) => CadView::Left, (0, true) => CadView::Right, (1, false) => CadView::Front, (1, true) => CadView::Back, (2, false) => CadView::Bottom, _ => CadView::Top };
                // exact pe fata cubului, fara planul portocaliu separat
                self.settings.cad_view_pos = 0.0;
                self.settings.show_cad_plane = false;
                self.face_pick = false;
                self.section_preview_open = true;
                self.settings.save();
                self.toast(format!("{} face chosen - the 2D drawing is in the preview; export it as PDF / DWG", self.settings.cad_view.label()));
            }
        }
        // click pe becul unei lumini (unealta Select / Window): selecteaza lumina si ii arata gizmo-ul
        let light_click = if resp.clicked() && !grid_click && !xform_click && !face_click && !self.face_pick && matches!(self.tool, Tool::Select | Tool::Window) {
            resp.interact_pointer_pos().and_then(|p| {
                self.lights
                    .iter()
                    .enumerate()
                    .filter_map(|(i, l)| to_screen(&params.view_proj, rect, Vec3::from(l.pos)).map(|s| (i, (s - p).length())))
                    .filter(|(_, d)| *d < 18.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(i, _)| i)
            })
        } else {
            None
        };
        if let Some(i) = light_click {
            self.light_sel = Some(i);
        }
        if resp.clicked() && !grid_click && !xform_click && light_click.is_none() && !face_click && !self.face_pick {
            if let Some(p) = resp.interact_pointer_pos() {
                match self.tool {
                    Tool::Measure(k) => {
                        if let Some(s) = snap {
                            self.cur_measure.push(s);
                            if let Some(n) = k.needed() {
                                if self.cur_measure.len() >= n {
                                    self.finish_measure();
                                }
                            }
                        }
                    }
                    Tool::Redline(crate::markup::RedKind::Text) => {
                        let n = self.to_red(rect, p);
                        self.red_text = Some((n, String::new()));
                    }
                    Tool::Redline(_) => {}
                    Tool::WalkPath => {
                        // punct pe podea (podea, scara, rampa; un click pe perete coboara pe podeaua de sub el)
                        let vpm = params.view_proj;
                        let lift = Vec3::Z * 0.05;
                        let on_point = self.walk_path.iter().any(|q| to_screen(&vpm, rect, *q + lift).is_some_and(|s| s.distance(p) < 10.0));
                        if !on_point {
                            if let Some(mut hit) = self.walk_floor_hit(uv(p), aspect) {
                                // click pe un segment existent: punctul se insereaza acolo
                                let mut insert: Option<usize> = None;
                                for k in 1..self.walk_path.len() {
                                    if let (Some(a), Some(b)) = (to_screen(&vpm, rect, self.walk_path[k - 1] + lift), to_screen(&vpm, rect, self.walk_path[k] + lift)) {
                                        if seg_dist_px(a, b, p) < 6.0 {
                                            insert = Some(k);
                                            break;
                                        }
                                    }
                                }
                                // Shift: directia de la ultimul punct in pasi de 45° (in plan)
                                if insert.is_none() && ui.input(|i| i.modifiers.shift) {
                                    if let Some(last) = self.walk_path.last() {
                                        let v = hit - *last;
                                        let l = v.truncate().length();
                                        if l > 1e-3 {
                                            let a = (v.y.atan2(v.x) / std::f32::consts::FRAC_PI_4).round() * std::f32::consts::FRAC_PI_4;
                                            hit = Vec3::new(last.x + a.cos() * l, last.y + a.sin() * l, hit.z);
                                        }
                                    }
                                }
                                self.path_run = None;
                                match insert {
                                    Some(k) => self.walk_path.insert(k, hit),
                                    None => self.walk_path.push(hit),
                                }
                            }
                        }
                    }
                    Tool::PlaceLight => {
                        // ca in Navisworks: click pe o suprafata = lumina plasata acolo
                        let (o, d) = self.cam.ray(uv(p), aspect);
                        if let Some((_, _, hit, n)) = self.pick(o, d) {
                            let mut l = crate::lights::SceneLight::default();
                            l.name = format!("Point light {}", self.lights.len() + 1);
                            let nv = n[0];
                            l.pos = (hit + nv * 0.15).to_array();
                            l.target = (hit - nv).to_array();
                            self.light_sel = Some(self.lights.len());
                            self.lights.push(l);
                            self.tool = Tool::Select;
                        }
                    }
                    Tool::RedErase => {
                        if let Some(i) = self.active_vp {
                            let n = self.to_red(rect, p);
                            let tol = 10.0 / rect.height();
                            if let Some(v) = self.viewpoints.get_mut(i) {
                                if let Some(k) = v.redlines.iter().rposition(|r| red_hit(r, n, tol)) {
                                    v.redlines.remove(k);
                                }
                            }
                        }
                    }
                    Tool::Select | Tool::Window => {
                        let (o, d) = self.cam.ray(uv(p), aspect);
                        match self.pick(o, d) {
                            Some((id, ..)) => self.apply(Action::Select(vec![id], mods.command)),
                            None => {
                                if !mods.command {
                                    self.clear_selection();
                                }
                            }
                        }
                    }
                }
            }
        }
        if resp.double_clicked() {
            if self.tool == Tool::WalkPath {
                // dublu-click = ultimul punct (adaugat deja de click) si gata
                self.tool = Tool::Select;
            } else if let Tool::Measure(_) = self.tool {
                self.finish_measure();
            } else if let Some(p) = resp.interact_pointer_pos() {
                let (o, d) = self.cam.ray(uv(p), aspect);
                if let Some((id, ..)) = self.pick(o, d) {
                    self.apply(Action::Focus(vec![id]));
                }
            }
        }
        resp.context_menu(|ui| {
            let sel: Vec<ElemId> = self.selected.iter().copied().collect();
            ui.add_enabled_ui(!sel.is_empty(), |ui| {
                if ui.button("⧉ Select Same Type  (Ctrl+Shift+A)").on_hover_text("Every visible item of the same type (Revit Family and Type / IFC type), in all models").clicked() {
                    self.select_similar(true);
                    ui.close_menu();
                }
                if ui.button("⧉ Select Same Category").on_hover_text("Every visible item of the same category / IFC class (all walls, all doors...)").clicked() {
                    self.select_similar(false);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Hide (H)").clicked() {
                    self.apply(Action::Hide(sel.clone()));
                    ui.close_menu();
                }
                if ui.button("Hide Unselected (I)").clicked() {
                    self.apply(Action::Isolate(sel.clone()));
                    ui.close_menu();
                }
                if ui.button("Focus (F)").clicked() {
                    self.apply(Action::Focus(sel.clone()));
                    ui.close_menu();
                }
                if ui.button("Section box around selection").clicked() {
                    self.section_to_selection();
                    ui.close_menu();
                }
                if ui.button("Save selection as set").clicked() {
                    let n = self.sets.len() + 1;
                    self.sets.push(SelSet { name: format!("Set {n}"), kind: SetKind::Explicit(sel.clone()) });
                    self.open_panel(Panel::Sets);
                    ui.close_menu();
                }
                if ui.button("🏗 Export selection to IFC…").clicked() {
                    self.ifc_selection = true;
                    self.request_path(Purpose::ExportIfc);
                    ui.close_menu();
                }
                if ui.button("Attach to current TimeLiner task").clicked() {
                    self.attach_selection();
                    ui.close_menu();
                }
            });
            ui.separator();
            if ui.button("Unhide All").clicked() {
                self.apply(Action::ShowAll);
                self.clash_focus = None;
                ui.close_menu();
            }
            if ui.button("Clear selection (Esc)").clicked() {
                self.clear_selection();
                ui.close_menu();
            }
        });

        // --------------------------------------------------------- suprapuneri
        let vp = params.view_proj;
        let line = |a: Vec3, b: Vec3, col: Color32, w: f32| {
            if let (Some(sa), Some(sb)) = (to_screen(&vp, rect, a), to_screen(&vp, rect, b)) {
                painter.line_segment([sa, sb], Stroke::new(w, col));
            }
        };
        // planurile de sectiune: suprafete verzi transparente, cat scena
        if self.section.enabled && self.section.mode == SectionMode::Planes && self.show_section_plane && self.report_job.is_none() {
            let bb = self.scene_aabb();
            if bb.is_valid() {
                let (lo, hi) = (bb.min - Vec3::splat(0.5), bb.max + Vec3::splat(0.5));
                for p in self.section.planes.iter().filter(|p| p.enabled) {
                    let o = p.offset;
                    let quad: [Vec3; 4] = match p.dir {
                        0 | 1 => [Vec3::new(lo.x, lo.y, o), Vec3::new(hi.x, lo.y, o), Vec3::new(hi.x, hi.y, o), Vec3::new(lo.x, hi.y, o)],
                        2 | 3 => [Vec3::new(o, lo.y, lo.z), Vec3::new(o, hi.y, lo.z), Vec3::new(o, hi.y, hi.z), Vec3::new(o, lo.y, hi.z)],
                        _ => [Vec3::new(lo.x, o, lo.z), Vec3::new(hi.x, o, lo.z), Vec3::new(hi.x, o, hi.z), Vec3::new(lo.x, o, hi.z)],
                    };
                    // subdivizat, ca sa ramana corect cand o parte e in spatele camerei
                    let n = 8;
                    let fill = Color32::from_rgba_unmultiplied(60, 200, 90, 38);
                    for i in 0..n {
                        for j in 0..n {
                            let at = |u: f32, v: f32| {
                                let a = quad[0].lerp(quad[1], u);
                                let b = quad[3].lerp(quad[2], u);
                                a.lerp(b, v)
                            };
                            let (u0, u1, v0, v1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32, j as f32 / n as f32, (j + 1) as f32 / n as f32);
                            let pts: Option<Vec<Pos2>> = [at(u0, v0), at(u1, v0), at(u1, v1), at(u0, v1)].iter().map(|q| to_screen(&vp, rect, *q)).collect();
                            if let Some(pts) = pts {
                                painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::NONE));
                            }
                        }
                    }
                    for k in 0..4 {
                        line(quad[k], quad[(k + 1) % 4], Color32::from_rgb(70, 210, 100), 1.5);
                    }
                }
            }
        }
        // Planul de taiere FACE PARTE DIN CUB: se deseneaza in interiorul lui, se
        // misca odata cu el si se TRAGE DIRECT cu mouse-ul (manerul cu sageti din centru).
        // Highlightul e INCHIS implicit: apare doar cand utilizatorul a ales o
        // FATA a cubului in Cut View (Top / Bottom / Front / Back / Left / Right).
        // In modul Current (implicit) nu se vede niciun plan.
        // planul vederii alese (verde deschis): mereu vizibil cu cubul de sectiune - nu mai e nevoie de bifa
        // „Highlight plane"; fara cub ramane optiunea din Settings
        let cut_show = (self.settings.show_cad_plane || (self.section.enabled && self.section.mode == SectionMode::Box))
            && self.report_job.is_none()
            && self.settings.cad_view != crate::app::CadView::Current;
        if cut_show {
            let view = self.settings.cad_view;
            let (axis, raw): (usize, f32) = if view == crate::app::CadView::Current {
                let (a, b) = (self.section.bmin[2], self.section.bmax[2]);
                (2, (a + self.settings.section_cut_offset.max(0.01)).min(b - 0.01).max(a + 0.01))
            } else if let Some(ax) = view.axis() {
                let bb = self.scene_aabb();
                if !bb.is_valid() {
                    (ax, 0.0)
                } else {
                    let t = self.settings.cad_view_pos.clamp(0.0, 1.0);
                    // la fel ca la export: fetele CUBULUI cand e activ, altfel scena
                    let (lo, hi) = if self.section.enabled && self.section.mode == SectionMode::Box {
                        (self.section.bmin[ax], self.section.bmax[ax])
                    } else {
                        (bb.min[ax], bb.max[ax])
                    };
                    let v = match view {
                        crate::app::CadView::Front => lo + (hi - lo) * t,
                        crate::app::CadView::Back => hi - (hi - lo) * t,
                        crate::app::CadView::Left => lo + (hi - lo) * t,
                        crate::app::CadView::Right => hi - (hi - lo) * t,
                        crate::app::CadView::Top => hi - (hi - lo) * t,
                        crate::app::CadView::Bottom => lo + (hi - lo) * t,
                        crate::app::CadView::Current => lo,
                    };
                    (ax, v)
                }
            } else {
                (2, 0.0)
            };
            // In modul cub planul sta EXACT unde e fata cubului (cubul e format din
            // planuri; fara salturi pe grila). Fara cub, vederile denumite se
            // aliniaza la grila / nivel; in timpul tragerii urmareste mouse-ul.
            let value = if self.cut_drag || view == crate::app::CadView::Current || (self.section.enabled && self.section.mode == SectionMode::Box) { raw } else { self.snap_cut(axis, raw) };
            // intinderea planului: IN INTERIORUL CUBULUI daca e activ, altfel al scenei
            let (lo, hi) = if self.section.enabled && self.section.mode == SectionMode::Box {
                (Vec3::from(self.section.bmin) - Vec3::splat(0.02), Vec3::from(self.section.bmax) + Vec3::splat(0.02))
            } else {
                let bb = self.scene_aabb();
                if bb.is_valid() {
                    (bb.min - Vec3::splat(0.5), bb.max + Vec3::splat(0.5))
                } else {
                    (Vec3::splat(-1.0), Vec3::splat(1.0))
                }
            };
            let quad: [Vec3; 4] = match axis {
                0 => [Vec3::new(value, lo.y, lo.z), Vec3::new(value, hi.y, lo.z), Vec3::new(value, hi.y, hi.z), Vec3::new(value, lo.y, hi.z)],
                1 => [Vec3::new(lo.x, value, lo.z), Vec3::new(hi.x, value, lo.z), Vec3::new(hi.x, value, hi.z), Vec3::new(lo.x, value, hi.z)],
                _ => [Vec3::new(lo.x, lo.y, value), Vec3::new(hi.x, lo.y, value), Vec3::new(hi.x, hi.y, value), Vec3::new(lo.x, hi.y, value)],
            };
            let n = 8;
            // planul de taiere: verde deschis, ca fata aleasa a cubului
            let fill = Color32::from_rgba_unmultiplied(120, 230, 120, 34);
            for i in 0..n {
                for j in 0..n {
                    let at = |u: f32, v: f32| {
                        let a = quad[0].lerp(quad[1], u);
                        let b = quad[3].lerp(quad[2], u);
                        a.lerp(b, v)
                    };
                    let (u0, u1, v0, v1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32, j as f32 / n as f32, (j + 1) as f32 / n as f32);
                    let pts: Option<Vec<Pos2>> = [at(u0, v0), at(u1, v0), at(u1, v1), at(u0, v1)].iter().map(|q| to_screen(&vp, rect, *q)).collect();
                    if let Some(pts) = pts {
                        painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::NONE));
                    }
                }
            }
            for k in 0..4 {
                line(quad[k], quad[(k + 1) % 4], Color32::from_rgb(90, 210, 100), 2.0);
            }
            // ---- manerul de tragere: bareta pe axa planului, cu sageti in ambele capete
            let mut c = (lo + hi) * 0.5;
            c[axis] = value;
            let mut e = Vec3::ZERO;
            e[axis] = 1.0;
            if let (Some(p0), Some(p1)) = (to_screen(&vp, rect, c), to_screen(&vp, rect, c + e)) {
                let sv = p1 - p0;
                let ppm = sv.length();
                let d = sv / ppm.max(1e-6);
                let near = hover.is_some_and(|h| rect.contains(h) && h.distance(p0) < 22.0);
                let active = self.cut_drag || near;
                if active {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    let col = Color32::from_rgb(255, 200, 90);
                    let (tip, tip2) = (p0 + d * 30.0, p0 - d * 30.0);
                    painter.line_segment([tip2, tip], Stroke::new(5.0_f32, col));
                    let nn = EV::new(-d.y, d.x) * 8.0;
                    painter.add(egui::Shape::convex_polygon(vec![tip, tip - d * 12.0 + nn, tip - d * 12.0 - nn], col, Stroke::new(1.0_f32, Color32::WHITE)));
                    painter.add(egui::Shape::convex_polygon(vec![tip2, tip2 + d * 12.0 + nn, tip2 + d * 12.0 - nn], col, Stroke::new(1.0_f32, Color32::WHITE)));
                    painter.circle(p0, 7.0, col, Stroke::new(1.5_f32, Color32::WHITE));
                }
                // prinderea si tragerea planului
                if near && !self.cut_drag && self.box_drag.is_none() && resp.drag_started_by(egui::PointerButton::Primary) {
                    self.cut_drag = true;
                }
                if self.cut_drag && ppm > 1e-3 {
                    if resp.dragged_by(egui::PointerButton::Primary) {
                        let dm = resp.drag_delta().dot(d) / ppm;
                        if view == crate::app::CadView::Current {
                            // inaltimea planului in interiorul cubului
                            let (za, zb) = (self.section.bmin[2], self.section.bmax[2]);
                            self.settings.section_cut_offset = (self.settings.section_cut_offset + dm).clamp(0.05, (zb - za).max(0.06));
                        } else {
                            // pozitia de-a lungul axei (span-ul cubului cand e activ),
                            // cu directia vederea alese
                            let span = if self.section.enabled && self.section.mode == SectionMode::Box {
                                (self.section.bmax[axis] - self.section.bmin[axis]).max(1e-3)
                            } else {
                                let sbb = self.scene_aabb();
                                (sbb.max[axis] - sbb.min[axis]).max(1e-3)
                            };
                            let inv = matches!(view, crate::app::CadView::Back | crate::app::CadView::Right | crate::app::CadView::Top);
                            let dt = if inv { -dm } else { dm } / span;
                            self.settings.cad_view_pos = (self.settings.cad_view_pos + dt).clamp(0.0, 1.0);
                        }
                    }
                    if resp.drag_stopped() || !resp.dragged_by(egui::PointerButton::Primary) {
                        self.cut_drag = false;
                        self.settings.save();
                    }
                }
            }
        }
        // 27.11: NU mai desenam liniile aplatizate pe plan („previewul 2D in 3D").
        // Sectiunea e acum GEOMETRIE 3D reala: shaderul taie la cub si deseneaza
        // capacul hasurat pe plan; tot ce e in cub de la plan incolo se vede direct.
        if self.section.enabled && self.section.mode == SectionMode::Box && self.settings.show_section_box {
            let (a, b) = (Vec3::from(self.section.bmin), Vec3::from(self.section.bmax));
            let c = |i: u32| Vec3::new(if i & 1 == 0 { a.x } else { b.x }, if i & 2 == 0 { a.y } else { b.y }, if i & 4 == 0 { a.z } else { b.z });
            for (i, j) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
                line(c(i), c(j), Color32::from_rgb(255, 190, 60), 1.5);
            }
        }
        // axe si niveluri: axele pe nivelul de sub camera (ca Navisworks), cu bulele etichetelor la capete;
        // nivelurile ca un contur discret in jurul scenei, cu numele si cota
        if (self.show_grids || self.level.show_lines) && self.loaded_models().next().is_some() && self.report_job.is_none() {
            let g = self.scene_grids();
            let bb = self.scene_aabb();
            let eye = self.cam.eye();
            let gz = g.levels.iter().rev().find(|l| l.1 <= eye.z).or(g.levels.first()).map(|l| l.1).unwrap_or(if bb.is_valid() { bb.min.z } else { 0.0 });
            let gcol = Color32::from_rgba_unmultiplied(120, 175, 255, 170);
            let seg = |a: Vec3, b: Vec3, col: Color32, w: f32| {
                // bucati mici: partile din spatele camerei dispar fara sa strice linia
                let n = 24;
                let mut prev: Option<Pos2> = None;
                for k in 0..=n {
                    let p = to_screen(&vp, rect, a.lerp(b, k as f32 / n as f32));
                    if let (Some(s), Some(q)) = (prev, p) {
                        painter.line_segment([s, q], Stroke::new(w, col));
                    }
                    prev = p;
                }
            };
            // axe ca pe plansa: linie-punct (ax), bula alba cu contur si eticheta, asezata DINCOLO de capatul liniei;
            // bulele au marime fixa pe ecran (se vad si de departe), liniile se pot prelungi (Edit Grids › Extend lines)
            let br = self.grid_bubble.clamp(6.0, 30.0);
            let ink = Color32::from_rgb(40, 60, 95);
            for (tag, a, b) in g.grids.iter().filter(|_| self.show_grids) {
                let (mut a, mut b) = (Vec3::new(a.x, a.y, gz), Vec3::new(b.x, b.y, gz));
                let dir = (b - a).normalize_or_zero();
                a -= dir * self.grid_ext;
                b += dir * self.grid_ext;
                // polilinia pe ecran (bucati, ca sa ramana corecta cand o parte e in spatele camerei)
                let n = 32;
                let pts: Vec<Option<Pos2>> = (0..=n).map(|k| to_screen(&vp, rect, a.lerp(b, k as f32 / n as f32))).collect();
                let mut run: Vec<Pos2> = Vec::new();
                let flush = |run: &mut Vec<Pos2>| {
                    if run.len() >= 2 {
                        // linie-punct: tronson lung, pauza, punct, pauza
                        let st = Stroke::new(1.4_f32, gcol);
                        painter.extend(egui::Shape::dashed_line(run, st, 16.0, 4.0));
                    }
                    run.clear();
                };
                for p in &pts {
                    match p {
                        Some(s) => run.push(*s),
                        None => flush(&mut run),
                    }
                }
                flush(&mut run);
                // bulele: in prelungirea liniei, tangente la capat
                let ends = [(pts.first().copied().flatten(), pts.get(1).copied().flatten()), (pts.last().copied().flatten(), pts.get(n - 1).copied().flatten())];
                for (end, inner) in ends {
                    let (Some(s), Some(q)) = (end, inner) else { continue };
                    let d = (s - q).normalized();
                    if !d.x.is_finite() {
                        continue;
                    }
                    let c = s + d * br;
                    if rect.expand(br).contains(c) {
                        painter.circle(c, br, Color32::from_rgb(252, 252, 250), Stroke::new(1.6_f32, ink));
                        painter.text(c, Align2::CENTER_CENTER, tag, FontId::proportional(br * 1.05), ink);
                    }
                }
            }
            if self.grid_pick > 0 {
                painter.text(rect.center_top() + EV::new(0.0, 14.0), Align2::CENTER_TOP, if self.grid_pick == 1 { "Grid: click the origin (snaps to corners) · Esc to cancel" } else { "Grid: click a point along the first axis direction · Esc to keep the angle" }, FontId::proportional(14.0), pal.overlay_text);
            }
            if self.level.show_lines && bb.is_valid() && g.levels.len() < 60 {
                let lcol = Color32::from_rgba_unmultiplied(235, 200, 120, 60);
                let oz = self.origin_f64().z;
                // 0.28: etichetele nu intra sub bara de navigare de jos si nu se suprapun intre ele
                let floor_y = if self.settings.nav_bar { rect.bottom() - 74.0 } else { rect.bottom() - 10.0 };
                let mut placed: Vec<f32> = Vec::new();
                let mut labels: Vec<(Pos2, String)> = Vec::new();
                for (name, z) in &g.levels {
                    let c = |x: f32, y: f32| Vec3::new(x, y, *z);
                    let (x0, y0, x1, y1) = (bb.min.x, bb.min.y, bb.max.x, bb.max.y);
                    for (p, q) in [(c(x0, y0), c(x1, y0)), (c(x1, y0), c(x1, y1)), (c(x1, y1), c(x0, y1)), (c(x0, y1), c(x0, y0))] {
                        seg(p, q, lcol, 1.0);
                    }
                    if let Some(s) = to_screen(&vp, rect, c(x0, y0)) {
                        if rect.contains(s) {
                            let el = *z as f64 + oz - if self.level.custom { self.level.zero } else { 0.0 };
                            labels.push((s, format!("{name}  {el:+.2}")));
                        }
                    }
                }
                // de jos in sus: fiecare eticheta sta la nivelul ei; una care s-ar suprapune cu alta nu se mai
                // afiseaza (linia nivelului ramane) - inainte se stivuiau intr-o coloana departe de nivel (~20 de niveluri)
                labels.sort_by(|a, b| b.0.y.total_cmp(&a.0.y));
                let mut skipped = 0;
                for (s, text) in labels {
                    // pana la doua randuri mai sus (eticheta ramane langa nivelul ei); altfel se ascunde
                    let y0 = s.y.min(floor_y);
                    let Some(y) = [y0, y0 - 24.0, y0 - 48.0].into_iter().find(|y| !placed.iter().any(|p| (p - y).abs() < 22.0) && *y >= rect.top() + 12.0) else {
                        skipped += 1;
                        continue;
                    };
                    placed.push(y);
                    label_bg(&painter, Pos2::new(s.x, y), &text, Color32::from_rgb(235, 200, 120));
                }
                if skipped > 0 {
                    let tip = format!("{skipped} level label(s) hidden to avoid overlap - zoom in, or see View › Levels");
                    painter.text(rect.left_bottom() + EV::new(10.0, -10.0 - if self.settings.nav_bar { 64.0 } else { 0.0 }), Align2::LEFT_BOTTOM, tip, FontId::proportional(11.0), Color32::from_rgba_unmultiplied(235, 200, 120, 170));
                }
            }
        }
        // relocare in curs: punctele alese, linia spre cursor si indicatia pasului urmator
        if let Some(x) = &self.xform {
            let col = Color32::from_rgb(255, 140, 60);
            let mut prev: Option<Pos2> = None;
            for (i, (p, _, _)) in x.pts.iter().enumerate() {
                if let Some(s) = to_screen(&vp, rect, *p) {
                    painter.circle_filled(s, 5.0, col);
                    painter.text(s + EV::new(8.0, -8.0), Align2::LEFT_BOTTOM, format!("{}", i + 1), FontId::proportional(12.0), col);
                    if let Some(q) = prev {
                        painter.line_segment([q, s], Stroke::new(2.0_f32, col));
                    }
                    prev = Some(s);
                }
            }
            if let (Some(q), Some(sp)) = (prev, snap.and_then(|s| to_screen(&vp, rect, s))) {
                painter.add(egui::Shape::dashed_line(&[q, sp], Stroke::new(1.5_f32, col), 8.0, 4.0));
            }
            let target = if x.selection { "selected items" } else { "model" };
            painter.text(rect.center_top() + EV::new(0.0, 14.0), Align2::CENTER_TOP, format!("{} ({target}) · Esc to cancel", x.mode.hint(x.pts.len())), FontId::proportional(14.0), pal.overlay_text);
        }
        let mcol = Color32::from_rgb(255, 220, 70);
        for m in &self.measures {
            let pts: Vec<Vec3> = m.pts.iter().map(|p| Vec3::from(*p)).collect();
            self.draw_measure(&painter, &vp, rect, m.kind, &pts, mcol);
            if let Some(lp) = pts.last().and_then(|p| to_screen(&vp, rect, *p)) {
                label_bg(&painter, lp, &m.value(), mcol);
            }
        }
        if let Tool::Measure(k) = self.tool {
            let mut pts = self.cur_measure.clone();
            if let Some(s) = snap {
                pts.push(s);
            }
            self.draw_measure(&painter, &vp, rect, k, &pts, Color32::from_rgb(120, 230, 255));
            if pts.len() >= 2 {
                let m = Measure { kind: k, pts: pts.iter().map(|p| p.to_array()).collect() };
                if let Some(lp) = to_screen(&vp, rect, *pts.last().unwrap()) {
                    label_bg(&painter, lp, &m.value(), Color32::from_rgb(120, 230, 255));
                }
            }
            if let Some(sp) = snap.and_then(|s| to_screen(&vp, rect, s)) {
                // simbolul snap-ului: patrat = colt, triunghi = mijloc, X = pe muchie, cerc = suprafata
                let st = Stroke::new(2.0_f32, Color32::from_rgb(120, 230, 255));
                match snap_kind {
                    1 => {
                        painter.rect_stroke(Rect::from_center_size(sp, EV::splat(11.0)), 0.0, st);
                    }
                    2 => {
                        let pts = vec![sp + EV::new(0.0, -7.0), sp + EV::new(6.5, 5.0), sp + EV::new(-6.5, 5.0)];
                        painter.add(egui::Shape::closed_line(pts, st));
                    }
                    3 => {
                        painter.line_segment([sp + EV::new(-5.0, -5.0), sp + EV::new(5.0, 5.0)], st);
                        painter.line_segment([sp + EV::new(-5.0, 5.0), sp + EV::new(5.0, -5.0)], st);
                    }
                    _ => {
                        painter.circle_stroke(sp, 6.0, st);
                    }
                }
                let lbl = ["", "Vertex", "Midpoint", "Edge"][snap_kind as usize];
                if !lbl.is_empty() {
                    painter.text(sp + EV::new(10.0, 10.0), Align2::LEFT_TOP, lbl, FontId::proportional(10.5), Color32::from_rgb(120, 230, 255));
                }
            }
            let hint = match k {
                MeasureKind::PointToPoint => "Click two points",
                MeasureKind::Angle => "Click: point, vertex, point",
                _ => "Click to add points · double-click / Esc to finish",
            };
            painter.text(rect.center_top() + EV::new(0.0, 14.0), Align2::CENTER_TOP, format!("Measure: {} — {hint}", k.label()), FontId::proportional(14.0), pal.overlay_text);
        }
        // traseul de mers: linie netezita + punctele numerotate
        if !self.walk_path.is_empty() && (self.tool == Tool::WalkPath || self.nav == NavMode::Walk || self.path_run.is_some()) && self.report_job.is_none() {
            let pc = Color32::from_rgb(60, 200, 255);
            let lift = Vec3::Z * 0.05;
            let dense = crate::app::walk::densify(&self.walk_path, 0.25);
            // umbra discreta sub linie (se citeste pe orice fundal), apoi linia
            for w in dense.windows(2) {
                line(w[0] + lift, w[1] + lift, Color32::from_black_alpha(90), 6.0);
            }
            for w in dense.windows(2) {
                line(w[0] + lift, w[1] + lift, pc, 3.0);
            }
            // sageti de directie la ~1.5 m
            let mut acc = 0.0f32;
            for w in dense.windows(2) {
                let seg = (w[1] - w[0]).length();
                acc += seg;
                if acc >= 1.5 && seg > 1e-4 {
                    acc = 0.0;
                    let m = (w[0] + w[1]) * 0.5 + lift;
                    if let (Some(a), Some(b)) = (to_screen(&vp, rect, m), to_screen(&vp, rect, m + (w[1] - w[0]) / seg * 0.3)) {
                        let dir = (b - a).normalized();
                        if dir.length() > 0.5 {
                            let nrm = EV::new(-dir.y, dir.x);
                            let tip = a + dir * 5.0;
                            painter.add(egui::Shape::line(vec![tip - dir * 8.0 + nrm * 5.0, tip, tip - dir * 8.0 - nrm * 5.0], Stroke::new(2.0_f32, Color32::WHITE)));
                        }
                    }
                }
            }
            // lungimea fiecarui segment
            if self.tool == Tool::WalkPath {
                for k in 1..self.walk_path.len() {
                    let (a, b) = (self.walk_path[k - 1], self.walk_path[k]);
                    if let Some(s) = to_screen(&vp, rect, (a + b) * 0.5 + lift) {
                        label_bg_center(&painter, s + EV::new(0.0, -6.0), &format!("{:.2} m", (b - a).length()), pc);
                    }
                }
                // previzualizare 3D a urmatorului segment, pe podea, cu lungimea lui
                if let (Some(last), Some(hp)) = (self.walk_path.last().copied(), hover) {
                    if self.wp_drag.is_none() && rect.contains(hp) {
                        if let Some(mut q) = self.walk_floor_hit(uv(hp), aspect) {
                            if ui.input(|i| i.modifiers.shift) {
                                let v = q - last;
                                let l = v.truncate().length();
                                if l > 1e-3 {
                                    let a = (v.y.atan2(v.x) / std::f32::consts::FRAC_PI_4).round() * std::f32::consts::FRAC_PI_4;
                                    q = Vec3::new(last.x + a.cos() * l, last.y + a.sin() * l, q.z);
                                }
                            }
                            if let (Some(a), Some(b)) = (to_screen(&vp, rect, last + lift), to_screen(&vp, rect, q + lift)) {
                                painter.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(2.0_f32, pc.gamma_multiply(0.8)), 8.0, 5.0));
                                painter.circle_stroke(b, 6.0, Stroke::new(2.0_f32, pc));
                                label_bg_center(&painter, b + EV::new(0.0, -12.0), &format!("{:.2} m", (q - last).length()), pc);
                            }
                        }
                    }
                }
            }
            let n_pts = self.walk_path.len();
            for (i, p) in self.walk_path.iter().enumerate() {
                if let Some(sp) = to_screen(&vp, rect, *p + lift) {
                    // start verde, final rosu, intermediare albastre (cu contur, se vad pe orice fundal)
                    let (col, txt) = if i == 0 {
                        (Color32::from_rgb(70, 200, 110), "S".to_string())
                    } else if i + 1 == n_pts && n_pts > 1 {
                        (Color32::from_rgb(235, 90, 80), "E".to_string())
                    } else {
                        (pc, (i + 1).to_string())
                    };
                    let r = if self.wp_drag == Some(i) { 9.0 } else { 7.5 };
                    painter.circle_filled(sp, r + 1.5, Color32::WHITE);
                    painter.circle_filled(sp, r, col);
                    painter.text(sp, Align2::CENTER_CENTER, txt, FontId::proportional(10.0), Color32::BLACK);
                }
            }
            if let Some(r) = &self.path_run {
                let len = r.length();
                let t = format!("{} Walk path  {:.0} / {:.0} m  ·  {:.1} m/s  ·  Space: {}  ·  Esc: stop", if r.paused { "⏸" } else { "▶" }, r.dist, len, self.settings.walk_speed, if r.paused { "play" } else { "pause" });
                label_bg(&painter, rect.center_top() + EV::new(0.0, 22.0), &t, pc);
            }
        }
        if self.tool == Tool::WalkPath {
            let n = self.walk_path.len();
            let len = crate::app::walk::PathRun::new(&self.walk_path).map(|r| r.length()).unwrap_or(0.0);
            label_bg_center(&painter, rect.center_top() + EV::new(0.0, 30.0), &format!("Walk path · {n} points · {len:.1} m  —  click the floor: add · drag a point: move · click the line: insert · right-click a point: delete · Shift: 45° · Enter: done"), Color32::from_rgb(60, 200, 255));
        }
        if let Some(f) = self.clash_focus_active() {
            let big = f.points.len() == 1;
            for p in f.points.iter().take(500) {
                if let Some(sp) = to_screen(&vp, rect, *p) {
                    if big {
                        painter.circle_stroke(sp, 14.0, Stroke::new(2.5_f32, Color32::from_rgb(255, 80, 60)));
                    } else {
                        painter.circle_stroke(sp, 7.0, Stroke::new(1.5_f32, Color32::from_rgb(255, 80, 60)));
                    }
                    painter.circle_filled(sp, 3.0, Color32::from_rgb(255, 80, 60));
                }
            }
        }
        if let Some((d, p)) = self.min_dist_result {
            if let Some(sp) = to_screen(&vp, rect, p) {
                painter.circle_filled(sp, 4.0, Color32::from_rgb(255, 0, 200));
                label_bg(&painter, sp, &format!("dist. min. {}", crate::app::fmt_len(d as f64)), Color32::from_rgb(255, 0, 200));
            }
        }
        // cadrul de randare (Render tab)
        if self.panel_shown(Panel::Render) {
            let ta = self.render.width as f32 / self.render.height.max(1) as f32;
            // chenarul apare doar cand il activezi (Render › Show frame)
            if self.render.show_frame && ta <= aspect + 1e-3 {
                let fw = rect.height() * ta;
                let fr = Rect::from_center_size(rect.center(), EV::new(fw, rect.height())).shrink(1.0);
                painter.rect_stroke(fr, 0.0, Stroke::new(1.5_f32, Color32::from_rgba_unmultiplied(255, 190, 60, 200)));
                painter.text(fr.left_bottom() + EV::new(6.0, -4.0), Align2::LEFT_BOTTOM, format!("{}×{}", self.render.width, self.render.height), FontId::proportional(11.0), Color32::from_rgb(255, 190, 60));
            }
            if let Some(j) = &self.render_job {
                painter.text(rect.center_bottom() - EV::new(0.0, 16.0), Align2::CENTER_BOTTOM, format!("🎞 Rendering frame {} / {}", j.frame.min(j.frames), j.frames), FontId::proportional(16.0), pal.overlay_text);
            }
        }
        // gizmo axe
        let o = rect.left_bottom() + EV::new(45.0, -45.0);
        let view = self.cam.view();
        for (ax, col, n) in [(Vec3::X, Color32::from_rgb(230, 70, 70), "X"), (Vec3::Y, Color32::from_rgb(80, 210, 90), "Y"), (Vec3::Z, Color32::from_rgb(80, 140, 255), "Z")] {
            if self.report_job.is_some() || self.loaded_models().next().is_none() {
                break;
            }
            let v = view.transform_vector3(ax);
            let e = o + EV::new(v.x, -v.y) * 30.0;
            painter.line_segment([o, e], Stroke::new(2.5_f32, col));
            painter.text(e, Align2::CENTER_CENTER, n, FontId::proportional(12.0), col);
        }
        // ViewCube-ul e overlay global: în split-view îl desenăm o singură dată (pe jumătatea activă).
        // Bara de navigare eArea globală - se desenează o singură dată în update(), nu aici.
        if overlays && self.report_job.is_none() && self.loaded_models().next().is_some() {
            self.view_cube(ui, rect);
        }
        // redlines of the active viewpoint
        let mut reds: Vec<crate::markup::Redline> = self.active_vp.and_then(|i| self.viewpoints.get(i)).map(|v| v.redlines.clone()).unwrap_or_default();
        if let Some(d) = &self.red_draft {
            reds.push(d.clone());
        }
        draw_redlines(&painter, rect, &reds);
        if let Some(i) = self.active_vp.filter(|_| self.report_job.is_none()) {
            if let Some(v) = self.viewpoints.get(i) {
                let t = format!("Viewpoint: {}  ·  {}  ·  {} markups", v.name, v.issue.status, v.redlines.len());
                painter.text(rect.left_bottom() + EV::new(10.0, -10.0), Align2::LEFT_BOTTOM, t, FontId::proportional(13.0), Color32::from_rgb(255, 190, 60));
            }
        }
        if let Some((pos, mut txt)) = self.red_text.take() {
            let sp = Pos2::new(rect.center().x + pos[0] * rect.height(), rect.center().y + pos[1] * rect.height());
            let mut keep = true;
            let mut ok = false;
            egui::Area::new(egui::Id::new("red_text")).fixed_pos(sp).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut txt).hint_text("markup text, Enter").desired_width(220.0));
                    if !r.has_focus() && !r.lost_focus() {
                        r.request_focus();
                    }
                    if ui.input(|i| i.key_pressed(Key::Enter)) {
                        ok = true;
                    }
                    if ui.input(|i| i.key_pressed(Key::Escape)) {
                        keep = false;
                    }
                });
            });
            if ok {
                if !txt.trim().is_empty() {
                    let rl = crate::markup::Redline { kind: crate::markup::RedKind::Text, pts: vec![pos], color: self.red_color, width: self.red_width, text: txt };
                    self.add_redline(rl);
                }
            } else if keep {
                self.red_text = Some((pos, txt));
            }
        }
        // stari
        if self.start_screen() {
            // pagina de start (ca Navisworks): Open / New + fisierele recente
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| self.start_page(ui));
        } else if self.loaded_models().next().is_none() && self.loads.is_empty() {
            // proiect nou / gol: scena goala, cu o indicatie discreta
            painter.text(rect.center(), Align2::CENTER_CENTER, "Empty project - Home › Open / Append to add models (Ctrl+O), Save to keep it (Ctrl+S)", FontId::proportional(14.0), pal.overlay_weak);
        }
        let mut yy = rect.center().y - 34.0 * self.loads.len() as f32 / 2.0;
        let mut cancel = None;
        for (k, j) in self.loads.iter().enumerate() {
            let r = Rect::from_center_size(Pos2::new(rect.center().x - 40.0, yy), EV::new(420.0, 26.0));
            painter.rect_filled(r, 5.0, Color32::from_black_alpha(170));
            let mut f = r.shrink(3.0);
            f.set_width(f.width() * j.progress.clamp(0.02, 1.0));
            painter.rect_filled(f, 4.0, accent());
            painter.text(r.center(), Align2::CENTER_CENTER, format!("{} — {} {:.0}%", crate::file_name(&j.path), j.stage, j.progress * 100.0), FontId::proportional(13.0), Color32::WHITE);
            // Cancel: un fisier mare deschis din greseala nu mai tine aplicatia ocupata minute intregi
            let br = Rect::from_min_size(Pos2::new(r.max.x + 8.0, r.min.y), EV::new(74.0, r.height()));
            if ui.put(br, egui::Button::new(crate::i18n::tr("Cancel"))).on_hover_text(crate::i18n::tr("Stop loading this file")).clicked() {
                cancel = Some(k);
            }
            yy += 34.0;
        }
        if let Some(k) = cancel {
            if let Some(j) = self.loads.get(k) {
                j.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        if let Tool::Redline(k) = self.tool {
            painter.text(rect.center_top() + EV::new(0.0, 14.0), Align2::CENTER_TOP, format!("Redline {}: {} · middle-drag pans, right-drag looks · Esc ends", k.label(), if k == crate::markup::RedKind::Text { "click to place text" } else { "drag to draw" }), FontId::proportional(14.0), pal.overlay_text);
        }
        if self.tool == Tool::RedErase {
            painter.text(rect.center_top() + EV::new(0.0, 14.0), Align2::CENTER_TOP, "Erase redline: click a markup", FontId::proportional(14.0), pal.overlay_text);
        }
        if self.tool == Tool::Select && resp.hovered() && self.loaded_models().next().is_some() && self.report_job.is_none() {
            painter.text(
                rect.left_top() + EV::new(10.0, 8.0),
                Align2::LEFT_TOP,
                if self.nav == NavMode::Walk {
                    "Walk · drag up / down: forward / back · sideways: turn · right-drag: look · W A S D".to_string()
                } else if self.nav == NavMode::Pan {
                    "Pan · drag: move the model · wheel: zoom · right-drag: look · click: select".to_string()
                } else {
                    "Click: select · drag: orbit · middle-drag: pan · wheel: zoom · double-click: focus".to_string()
                },
                FontId::proportional(12.0),
                // pe fundal deschis (Hidden line, fundal alb / cer) textul e inchis, altfel nu se vede
                if self.settings.visual_style == crate::render::VisualStyle::Hidden || matches!(self.settings.vp_bg.as_str(), "white" | "sky") { Color32::from_rgb(90, 96, 106) } else { pal.overlay_weak },
            );
        }
    }

    /// Bara de navigare plutitoare (jos, pe centru): ce se foloseste des, fara drum prin ribbon.
    /// Bara de unelte de jos (aspect "studio"): butoane mari cu iconita si eticheta, pe grupe
    /// (selectie, vizibilitate, navigare, unelte). Culorile vin din tema curenta.
    pub(super) fn nav_bar(&mut self, ctx: &egui::Context, rect: Rect) {
        let pal = self.pal();
        let pos = rect.center_bottom() - EV::new(0.0, 14.0);
        egui::Area::new(egui::Id::new("nav_bar")).fixed_pos(pos).pivot(Align2::CENTER_BOTTOM).order(egui::Order::Middle).show(ctx, |ui| {
            egui::Frame::none()
                .fill(pal.overlay_btn)
                .stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(18)))
                .rounding(egui::Rounding::same(14.0))
                .shadow(egui::epaint::Shadow { offset: EV::new(0.0, 6.0), blur: 20.0, spread: 0.0, color: Color32::from_black_alpha(110) })
                .inner_margin(egui::Margin::symmetric(8.0, 3.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let text_col = pal.overlay_text;
                        // buton mare: iconita + eticheta; activ = fond in culoarea de accent
                        let big = |ui: &mut egui::Ui, icon: &str, label: &str, tip: &str, on: bool, enabled: bool| -> bool {
                            // compact: ~24 px, doar iconita (numele apare in tooltip)
                            let _ = label;
                            let (r, resp) = ui.allocate_exact_size(EV::new(30.0, 28.0), if enabled { Sense::click() } else { Sense::hover() });
                            let p = ui.painter();
                            if on {
                                p.rect_filled(r.shrink(1.0), 7.0, accent());
                            } else if enabled && resp.hovered() {
                                p.rect_filled(r.shrink(1.0), 7.0, Color32::from_white_alpha(16));
                            }
                            let col = if !enabled { text_col.gamma_multiply(0.35) } else if on { Color32::WHITE } else { text_col };
                            p.text(r.center(), Align2::CENTER_CENTER, crate::theme::ph(icon), crate::theme::icon_font(16.0), col);
                            let resp = resp.on_hover_text(format!("{label}: {tip}"));
                            enabled && resp.clicked()
                        };
                        let sep = |ui: &mut egui::Ui| {
                            let (r, _) = ui.allocate_exact_size(EV::new(8.0, 28.0), Sense::hover());
                            ui.painter().line_segment([r.center_top() + EV::new(0.0, 5.0), r.center_bottom() - EV::new(0.0, 5.0)], Stroke::new(1.0_f32, Color32::from_white_alpha(28)));
                        };
                        // ---- inapoi / inainte (selectie, vizibilitate, sectiune, camera)
                        if big(ui, "⟲", "Undo", "Undo: selection, hidden items, section, camera, properties, colours and materials (Ctrl+Z)", false, self.can_undo()) {
                            self.undo_view();
                        }
                        if big(ui, "⟳", "Redo", "Redo view (Ctrl+Y)", false, self.can_redo()) {
                            self.redo_view();
                        }
                        sep(ui);
                        // ---- selectie
                        if big(ui, "👆", "Select", "Select (click an item · Ctrl+click adds)", self.tool == Tool::Select && self.nav == NavMode::Orbit, true) {
                            self.cur_measure.clear();
                            self.tool = Tool::Select;
                            self.nav = NavMode::Orbit;
                        }
                        if big(ui, "⬚", "Window", "Window selection: drag left -> right = inside, right -> left = touching (Ctrl adds)", self.tool == Tool::Window, true) {
                            self.cur_measure.clear();
                            self.tool = if self.tool == Tool::Window { Tool::Select } else { Tool::Window };
                        }
                        sep(ui);
                        // ---- vizibilitate
                        let sel: Vec<ElemId> = self.selected.iter().copied().collect();
                        if big(ui, "🚫", "Hide", "Hide the selected items (H)", false, !sel.is_empty()) {
                            self.apply(Action::Hide(sel.clone()));
                        }
                        if big(ui, "◎", "Isolate", "Show only the selection - hide the rest (I)", false, !sel.is_empty()) {
                            self.apply(Action::Isolate(sel.clone()));
                        }
                        if big(ui, "👁", "Unhide", &format!("Unhide all ({} hidden)", self.hidden.len()), false, !self.hidden.is_empty()) {
                            self.apply(Action::ShowAll);
                            self.clash_focus = None;
                        }
                        if big(ui, "🎯", "Focus", "Focus on the selection (F) · double-click an item does the same", false, !sel.is_empty()) {
                            self.apply(Action::Focus(sel.clone()));
                        }
                        sep(ui);
                        // ---- navigare
                        let orbit = self.nav == NavMode::Orbit && matches!(self.tool, Tool::Select);
                        if big(ui, "🔄", "Orbit", "Orbit: drag to rotate around the point under the cursor", orbit, true) {
                            self.nav = NavMode::Orbit;
                            self.tool = Tool::Select;
                            self.path_run = None;
                        }
                        if big(ui, "✋", "Pan", "Pan: drag to move the model sideways (middle button always pans)", self.nav == NavMode::Pan, true) {
                            self.nav = NavMode::Pan;
                            self.tool = Tool::Select;
                            self.path_run = None;
                        }
                        if big(ui, "🚶", "Walk", "Walk: drag up = forward, sideways = turn; W A S D", self.nav == NavMode::Walk && self.tool != Tool::WalkPath, true) {
                            self.nav = NavMode::Walk;
                            self.tool = Tool::Select;
                        }
                        let bb = self.scene_aabb();
                        if big(ui, "⛶", "Fit", "Zoom to the whole model", false, true) {
                            let mut t = self.cam;
                            t.fit(&bb);
                            self.fly_to(t);
                        }
                        if big(ui, "🏠", "Home", "Home view (Home key)", false, true) {
                            self.go_home();
                        }
                        sep(ui);
                        // ---- unelte
                        let meas = matches!(self.tool, Tool::Measure(_));
                        if big(ui, "📏", "Measure", "Measure point to point (snaps to corners, midpoints, edges) - more kinds in Review › Measure", meas, true) {
                            self.cur_measure.clear();
                            self.tool = if meas { Tool::Select } else { Tool::Measure(MeasureKind::PointToPoint) };
                        }
                        let sec = self.section.enabled;
                        if big(ui, "▣", "Section", if sec { "Turn the section off" } else { "Section box around the selection (or the whole model) - stretch it with the arrows on its faces" }, sec, true) {
                            if sec {
                                self.section.enabled = false;
                            } else if !sel.is_empty() {
                                self.section_to_selection();
                            } else if bb.is_valid() {
                                self.section.enabled = true;
                                self.section.mode = SectionMode::Box;
                                self.section.bmin = (bb.min - Vec3::splat(0.3)).to_array();
                                self.section.bmax = (bb.max + Vec3::splat(0.3)).to_array();
                            }
                        }
                        let red = matches!(self.tool, Tool::Redline(_) | Tool::RedErase);
                        if big(ui, "✏", "Markup", "Markups on the current view (cloud; more in Review › Redline) - saved with a viewpoint", red, true) {
                            self.cur_measure.clear();
                            self.tool = if red { Tool::Select } else { Tool::Redline(crate::markup::RedKind::Cloud) };
                        }
                        let (ic, lb, tip) = if self.cam.ortho { ("⬚", "Ortho", "Orthographic - click for perspective") } else { ("🔭", "Persp", "Perspective - click for orthographic") };
                        if big(ui, ic, lb, tip, false, true) {
                            self.cam.ortho = !self.cam.ortho;
                        }
                        // mers: traseu, redare, viteza - mereu pe bara (latime fixa: butoanele nu se muta sub mouse
                        // cand schimbi modul de navigare)
                        sep(ui);
                        if big(ui, "🛤", "Path", "Draw a walk path: click points on the floor, double-click / Enter to finish (switches to Walk)", self.tool == Tool::WalkPath, true) {
                            self.path_run = None;
                            if self.tool == Tool::WalkPath {
                                self.tool = Tool::Select;
                            } else {
                                self.nav = NavMode::Walk;
                                self.tool = Tool::WalkPath;
                            }
                        }
                        let playing = self.path_run.as_ref().map(|r| !r.paused).unwrap_or(false);
                        if big(ui, if playing { "⏸" } else { "▶" }, if playing { "Pause" } else { "Play" }, "Walk along the drawn path (Space) · Esc stops", playing, self.walk_path.len() >= 2) {
                            self.nav = NavMode::Walk;
                            self.walk_path_play();
                        }
                        let mut v = self.settings.walk_speed;
                        let r = ui.add_sized([62.0, 22.0], egui::DragValue::new(&mut v).range(0.2..=20.0).speed(0.05).max_decimals(1).suffix(" m/s")).on_hover_text("Walk speed (drag or type)");
                        if r.changed() {
                            self.settings.walk_speed = v;
                        }
                        if r.drag_stopped() || r.lost_focus() {
                            self.settings.save();
                        }
                    });
                });
        });
    }

    pub(super) fn draw_measure(&self, painter: &egui::Painter, vp: &Mat4, rect: Rect, k: MeasureKind, pts: &[Vec3], col: Color32) {
        let sp: Vec<Option<Pos2>> = pts.iter().map(|p| to_screen(vp, rect, *p)).collect();
        for w in sp.windows(2) {
            if let (Some(a), Some(b)) = (w[0], w[1]) {
                painter.line_segment([a, b], Stroke::new(2.0_f32, col));
            }
        }
        if k == MeasureKind::Area && sp.len() > 2 {
            if let (Some(a), Some(b)) = (sp[0], sp[sp.len() - 1]) {
                painter.line_segment([a, b], Stroke::new(1.0_f32, col));
            }
        }
        for p in sp.into_iter().flatten() {
            painter.circle_filled(p, 3.5, col);
        }
    }
}
