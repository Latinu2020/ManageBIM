//! View cube in the corner of the 3D view: faces, edges and corners turn the camera; home and perspective / ortho.
use super::*;

impl App {
    // ================================================================== ViewCube
    pub(super) fn view_cube(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let pal = self.pal();
        // compact cube: more room for the model
        let size = 24.0f32;
        let c = Pos2::new(rect.right() - 68.0, rect.top() + 66.0);
        let area = Rect::from_center_size(c, EV::splat(size * 3.4));
        let resp = ui.interact(area, ui.id().with("viewcube"), Sense::click());
        let (r, u, f) = (self.cam.right(), self.cam.up(), self.cam.fwd());
        let proj = |p: Vec3| c + EV::new(p.dot(r), -p.dot(u)) * size;
        // faces: normal, U / V axes, label
        let faces: [(Vec3, Vec3, Vec3, &str); 6] = [
            (Vec3::Z, Vec3::X, Vec3::Y, "TOP"),
            (-Vec3::Z, Vec3::X, -Vec3::Y, "BOTTOM"),
            (-Vec3::Y, Vec3::X, Vec3::Z, "FRONT"),
            (Vec3::Y, -Vec3::X, Vec3::Z, "BACK"),
            (Vec3::X, Vec3::Y, Vec3::Z, "RIGHT"),
            (-Vec3::X, -Vec3::Y, Vec3::Z, "LEFT"),
        ];
        let mut vis: Vec<(f32, usize)> = faces.iter().enumerate().filter(|(_, fc)| fc.0.dot(f) < -0.02).map(|(i, fc)| (fc.0.dot(f), i)).collect();
        vis.sort_by(|a, b| b.0.total_cmp(&a.0)); // far -> near (total_cmp: no panic on NaN)
        // edge / corner zones (chamfers, like the Autodesk ViewCube): 26 precise directions
        let edge = 0.64f32;
        let bands = [(-1.0f32, -edge), (-edge, edge), (edge, 1.0)];
        let hover = resp.hover_pos();
        let mut hit: Option<Vec3> = None;
        // (polygon, direction, face, is the face centre)
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
        // looking straight at a face: arrows towards the 4 neighbouring faces (as in Autodesk)
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
            // the nearest face is last
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
        // ---- compass under the cube (drawn first, the cube covers it): ring with ticks and N / E / S / W
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
        // ---- cube faces: shaded by a fixed light (top-left), chamfers slightly darker
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
            // the thin lines of the chamfers
            for e in [-edge, edge] {
                p.line_segment([proj(n + fu * e - fv), proj(n + fu * e + fv)], Stroke::new(0.5_f32, edge_col.gamma_multiply(0.35)));
                p.line_segment([proj(n - fu + fv * e), proj(n + fu + fv * e)], Stroke::new(0.5_f32, edge_col.gamma_multiply(0.35)));
            }
            let pts: Vec<Pos2> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().map(|(a, b): &(f32, f32)| proj(n + fu * *a + fv * *b)).collect();
            p.add(egui::Shape::closed_line(pts, Stroke::new(1.2_f32, edge_col)));
            // label rotated together with the face (by the projected U axis), never upside down
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
            // the letters fade out when the ring is seen almost edge-on
            let fade = (f.z.abs() * 1.6).clamp(0.35, 1.0);
            p.text(q, Align2::CENTER_CENTER, txt, FontId::proportional(if txt == "N" { 10.0 } else { 8.5 }), col.gamma_multiply(fade));
        }
        // Home button and menu
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
        // View Templates under the cube (0.28): a pill with ◀ ▶ (previous / next template, like Navisworks Saved
        // Views) and the current template name in the middle - a click opens the View Templates window
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
        // the cube is near the right edge: the pill does not leave the 3D view
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
}
