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

    /// Window selection: `crossing` = everything touching the rectangle, otherwise only what is fully inside.
    /// Visible elements only; `add` (Ctrl) keeps the existing selection.
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

    // ================================================================== viewport
    pub(super) fn viewport(&mut self, ui: &mut egui::Ui, overlays: bool) {
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        // background: gradient in three bands, with a slightly lit "horizon line" (more elegant than two colours)
        let mut mesh = egui::Mesh::default();
        let pal = self.pal();
        // Hidden line (Rhino): paper-white background, so the black lines show.
        let rhino = self.settings.visual_style == crate::render::VisualStyle::Hidden;
        // the background chosen in Settings › Appearance (default = the theme's)
        let (top, bot) = match self.settings.vp_bg.as_str() {
            _ if rhino => (Color32::from_rgb(252, 252, 253), Color32::from_rgb(240, 242, 245)),
            // night scene (Render › Lighting): dark sky, the lamps stand out
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
        // start page: only it, no model / cube / overlays (the project stays open underneath)
        if self.start_screen() {
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| self.start_page(ui));
            if self.show_start && ui.input(|i| i.key_pressed(Key::Escape)) && self.has_open_project() {
                self.show_start = false;
            }
            return;
        }

        let aspect = rect.width() / rect.height().max(1.0);
        let params = self.frame_params(aspect);
        // the image waits a few frames after the style / materials changed (see still_wait)
        let capture = if self.still_wait == 0 { self.pending_capture.take() } else { None };
        if let Some((w, h)) = capture {
            let (bg_top, bg_bottom) = self.render_bg();
            let cp = self.frame_params(w as f32 / h.max(1) as f32);
            // Dashboard / AI: the image shows what is seen in the viewport (hidden items and clash dimming included)
            let lists = (self.dash_job.is_some() || self.mcp.capture.is_some()).then(|| std::sync::Arc::new(self.draw_lists(&cp, h as f32)));
            // Ray Trace view on: the image is path traced too (64 samples, or the view's own count if lower)
            let pt = if self.rt.on { self.rt.scene.clone().map(|sc| (self.rt_capture_frame(sc), self.settings.rt_samples.clamp(16, 64))) } else { None };
            self.renderer.lock().unwrap_or_else(|e| e.into_inner()).capture = Some(crate::render::CaptureReq { params: cp, w, h, bg_top, bg_bottom, lists, pt });
        }
        // what gets drawn: the field of view, without hidden items; simplified while the camera moves.
        // The list is captured by the callback (NOT in the renderer) - otherwise, in split view, the second
        // viewport overwrites the first and elements disappear when I move the camera.
        let lists = self.draw_lists(&params, rect.height());
        if self.lod_moving() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(350));
        }
        let r = self.renderer.clone();
        // interactive Ray Trace: only in the main 3D view
        // paused while a final Path Trace render uses the graphics card (the render shows in its own window)
        let pt = if self.rt.on && overlays && !self.gpu_render_running() { self.rt_frame(rect.width() / rect.height().max(1.0)) } else { None };
        // start page over the project: the model is no longer drawn underneath
        if !self.start_screen() {
            let lists = std::sync::Arc::new(lists);
            painter.add(egui::PaintCallback {
                rect,
                callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |_info, p| {
                    let mut r = r.lock().unwrap_or_else(|e| e.into_inner());
                    r.pt_live = pt.clone();
                    unsafe { r.paint(p.gl(), &params, Some(&lists)) };
                })),
            });
        }
        if self.rt.on && overlays && !self.gpu_render_running() {
            self.rt_overlay(ui, &painter, rect);
        }
        let gl_error = self.renderer.lock().unwrap_or_else(|e| e.into_inner()).error.clone();
        if let Some(e) = gl_error {
            painter.text(rect.center(), Align2::CENTER_CENTER, e, FontId::proportional(14.0), Color32::RED);
            // no OpenGL 3.3 on this graphics card: offer the software renderer (Mesa) right here
            if !crate::softgl::active() && crate::softgl::available() {
                let b = Rect::from_center_size(rect.center() + EV::new(0.0, 46.0), EV::new(280.0, 30.0));
                if ui.put(b, egui::Button::new("Restart with software rendering")).on_hover_text("Draws with the processor (Mesa llvmpipe): slower, but everything works. The project is saved first.").clicked() {
                    self.settings.software_gl = true;
                    self.settings.save();
                    self.restart_for_graphics(ui.ctx(), true);
                }
            }
        }

        // --------------------------------------------------------- interaction
        let uv = |p: Pos2| Vec2::new((p.x - rect.left()) / rect.width(), (p.y - rect.top()) / rect.height());
        let mods = ui.input(|i| i.modifiers);
        let hover = resp.hover_pos();
        let mut snap: Option<Vec3> = None;
        // type of the snapped point: 0 surface, 1 corner, 2 edge midpoint, 3 on edge
        let mut snap_kind = 0u8;
        // the element and the face normal under the cursor (relocation: align to face)
        let mut snap_hit: Option<(ElemId, Vec3)> = None;
        if self.xform.is_some() && ui.input(|i| i.key_pressed(Key::Escape)) {
            self.xform = None;
        }
        if self.grid_pick > 0 && ui.input(|i| i.key_pressed(Key::Escape)) {
            self.grid_pick = 0;
        }
        // F3 = OSNAP on/off (as in AutoCAD / Navisworks)
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
                // Vertex (Endpoint) - full radius
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
                // Edge (Nearest on edge)
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
        // overlaid 2D drawings (DWG / DXF)
        if !self.overlays.is_empty() && self.report_job.is_none() {
            self.draw_overlays(&painter, rect, &params.view_proj);
            self.draw_profile_legend(&painter, rect);
        }
        // section box as in Revit: one arrow on each face; drag it and the face moves along its axis
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
                // bigger arrow, grabbed anywhere along its length (a 16 px zone around the shaft and the tip)
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
                    // halo: you see clearly what you grab
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
                // how many pixels 1 m on the axis takes on screen: mouse motion projected onto the axis direction
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
        // ---- the selected coordination object (Coordination objects window open): gizmo with three modes,
        // as for lights - Resize (arrows on faces, in the rotated object's axes), Move (X / Y / Z arrows from the
        // centre), Rotate (circle around the vertical). The virtual model is rebuilt when the mouse is released.
        let mut coord_hover: Option<usize> = None;
        let coord_edit = if self.coord_open { self.coord_sel.filter(|&i| i < self.coord_objects.len()) } else { None };
        if let Some(ci) = coord_edit {
            let vpm = params.view_proj;
            let (a, b) = (Vec3::from(self.coord_objects[ci].min), Vec3::from(self.coord_objects[ci].max));
            let c = (a + b) * 0.5;
            let h = (b - a) * 0.5;
            let rot = self.coord_objects[ci].rot_deg.to_radians();
            // the object's local axes (rotated around the vertical)
            let ex = Vec3::new(rot.cos(), rot.sin(), 0.0);
            let ey = Vec3::new(-rot.sin(), rot.cos(), 0.0);
            let ez = Vec3::Z;
            let local = [ex, ey, ez];
            let kc = self.coord_objects[ci].kind.color();
            let base = Color32::from_rgb(kc[0], kc[1], kc[2]);
            let mode = self.coord_mode;
            // outline of the rotated box (visible through walls too)
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
            // the rotation circle in the horizontal plane of the centre
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
                // the object's direction (local X axis), so the angle shows
                if let (Some(p0), Some(p1)) = (to_screen(&vpm, rect, c), to_screen(&vpm, rect, c + ex * ring_r)) {
                    painter.line_segment([p0, p1], Stroke::new(2.0_f32, col));
                    painter.text(p1 + EV::new(8.0, -8.0), Align2::LEFT_BOTTOM, format!("{:.1}°", self.coord_objects[ci].rot_deg), FontId::proportional(13.0), col);
                }
            } else {
                for k in 0..6usize {
                    let (ax, s) = (k / 2, if k % 2 == 0 { -1.0f32 } else { 1.0 });
                    // Resize: from the centre of each face, in local axes; Move: from the centre, in world axes (+ only)
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
            // the cursor angle in the horizontal plane of the centre (rotation)
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
                                // Shift: 15 degree steps (as in Revit / AutoCAD)
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
                                        // the face moves by dm, the opposite face stays in place
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
        // ---- lights: 3D gizmo. Move = the X/Y/Z arrows, Rotate = the XY / XZ / YZ circles.
        // The hit test and the drag start happen BEFORE navigation: otherwise the orbit moves
        // the camera in the same frame as the light, and the circles run away from the cursor (rotation did not work).
        let cam_c = self.cam;
        // (index, position, arrow length, rotation circle radius) - constant sizes on screen
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
        // the cursor angle around the light, in the plane perpendicular to axis `ai`.
        // Normal: the mouse ray intersected with the plane (exact, correct sign). Plane seen edge-on:
        // the angle on screen, with the sign given by the axis orientation relative to the camera.
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
                0 => lp + Vec3::new(u, v, 0.0),  // XY -> rotation around Z
                1 => lp + Vec3::new(u, 0.0, v),  // XZ -> around Y
                _ => lp + Vec3::new(0.0, u, v), // YZ -> around X
            }
        };
        let drag_start = resp.drag_started_by(egui::PointerButton::Primary);
        // at the start of the drag we test the point where the button was pressed, not where the mouse got to
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
                    // a point light has no direction: we turn it into a Spot, so that rotation has an effect
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
                    // plane seen almost edge-on (ray parallel to it): we measure the angle on screen
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
                    // difference brought into [-pi, pi]: no jump when the angle passes through +-180°
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
        // ---- walk path (Draw tool): points are dragged on the floor, right click on a point = delete
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
        // the light gizmo has priority: we do not let orbit / pan move the camera at the same time
        let light_handle = self.wp_drag.is_some() ||self.light_axis.is_some() || self.light_rot.is_some() || self.tool == Tool::PlaceLight;
        let handle_drag = self.box_drag.is_some() || light_handle || self.coord_drag.is_some() || (handle_hover.is_some() && resp.drag_started_by(egui::PointerButton::Primary));
        // window selection (Window tool): dragging with the left button draws the rectangle
        let box_tool = self.tool == Tool::Window && !handle_drag;
        if box_tool {
            if resp.drag_started_by(egui::PointerButton::Primary) {
                self.box_start = resp.interact_pointer_pos().map(|p| [p.x, p.y]);
            }
            if let (Some(s), Some(p)) = (self.box_start, resp.interact_pointer_pos().or(hover)) {
                let a = Pos2::new(s[0], s[1]);
                let r = Rect::from_two_pos(a, p);
                // left -> right: window (blue, solid); right -> left: crossing (green, dotted)
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
            // walking as in Navisworks: drag up = forward, sideways = turn (joystick).
            // Progressive response (dead zone + quadratic curve, capped at 2x walking speed),
            // smooth acceleration / braking is done by walk_tick.
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
            // primary drag draws; only middle / right navigate
        }
        if !walk && ((red_tool.is_none() && !box_tool && !handle_drag && resp.drag_started_by(egui::PointerButton::Primary)) || resp.drag_started_by(egui::PointerButton::Middle)) {
            self.pivot = None;
            if let Some(p) = resp.interact_pointer_pos() {
                let (o, d) = self.cam.ray(uv(p), aspect);
                if let Some((_, t, hit, _)) = self.pick(o, d) {
                    self.pivot = Some(hit);
                    if self.cam.ortho {
                        // ortho: `focus` gives the view scale - we do not change it (the ray starts far behind the
                        // camera, and its distance would suddenly enlarge the view). We only move the eye along the
                        // view direction, so that the picked point is at distance `focus` (the image does not change).
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
            // Pan mode: the model follows the mouse
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
                // ~20% per wheel step (before ~32%: on dense point clouds the view seemed to "jump")
                self.cam.zoom(scroll / 85.0, d, ms);
            }
        }
        // picking the axes origin / direction has priority over the current tool
        // relocation: the picked points (with snap) and the face normal
        let xform_click = resp.clicked() && self.xform.is_some() && snap.is_some();
        if xform_click {
            if let (Some(s), Some((id, n))) = (snap, snap_hit) {
                // normal towards the camera (visible face)
                let n = if n.dot(self.cam.eye() - s) < 0.0 { -n } else { n };
                self.xform_point(s, n, id);
            }
        }
        // ---- lights: drawing. Each lamp = a bulb with halo and rays (clearly visible on any background and
        // through walls), a label with name and power. Spot / Distant have a direction arrow (Spot also the cone).
        // The selected light has its gizmo on top: X/Y/Z arrows (Move) or rotation circles (Rotate).
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
            // halo + rays + bulb
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
            // direction: an arrow of constant length on screen; Spot: also the light cone
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
                            // circle colour = colour of the axis it rotates around (XY -> Z blue ...)
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
            // label on a dark background: readable on a white or black model
            let g = painter.layout_no_wrap(format!("{} · {:.1} lux", lamp.name, lamp.power), FontId::proportional(13.0), Color32::WHITE);
            let pos = sp + EV::new(sz + 12.0, -sz - 20.0);
            painter.rect_filled(Rect::from_min_size(pos - EV::splat(4.0), g.size() + EV::splat(8.0)), 4.0, Color32::from_black_alpha(175));
            painter.galley(pos, g, Color32::WHITE);
        }
        // in Rotate mode: the current direction (azimuth / tilt) under the light
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
        // ---- picking a section box face for the 2D drawing (Annotation › Pick face):
        // the face under the cursor turns light green; click = the view from that face (Top / Front ...)
        let box_on = self.section.enabled && self.section.mode == SectionMode::Box;
        if self.face_pick && (!box_on || ui.input(|i| i.key_pressed(Key::Escape))) {
            self.face_pick = false;
        }
        let mut face_hover: Option<(usize, bool)> = None;
        if self.face_pick && box_on {
            if let Some(h) = hover.filter(|h| rect.contains(*h)) {
                let (o, d) = self.cam.ray(uv(h), aspect);
                let (a, b) = (Vec3::from(self.section.bmin), Vec3::from(self.section.bmax));
                // slab method: the face the ray enters through (or exits, if the eye is inside the box)
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
        // the chosen face (current view) and the face under the cursor: light green, transparent
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
            // the chosen face is drawn by the cut plane (green, it also follows its dragging); here only the hover
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
                // exactly on the box face, without the separate orange plane
                self.settings.cad_view_pos = 0.0;
                self.settings.show_cad_plane = false;
                self.face_pick = false;
                self.section_preview_open = true;
                self.settings.save();
                self.toast(format!("{} face chosen - the 2D drawing is in the preview; export it as PDF / DWG", self.settings.cad_view.label()));
            }
        }
        // click on a light's bulb (Select / Window tool): selects the light and shows its gizmo
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
                        // point on the floor (floor, stair, ramp; a click on a wall drops to the floor below it)
                        let vpm = params.view_proj;
                        let lift = Vec3::Z * 0.05;
                        let on_point = self.walk_path.iter().any(|q| to_screen(&vpm, rect, *q + lift).is_some_and(|s| s.distance(p) < 10.0));
                        if !on_point {
                            if let Some(mut hit) = self.walk_floor_hit(uv(p), aspect) {
                                // click on an existing segment: the point is inserted there
                                let mut insert: Option<usize> = None;
                                for k in 1..self.walk_path.len() {
                                    if let (Some(a), Some(b)) = (to_screen(&vpm, rect, self.walk_path[k - 1] + lift), to_screen(&vpm, rect, self.walk_path[k] + lift)) {
                                        if seg_dist_px(a, b, p) < 6.0 {
                                            insert = Some(k);
                                            break;
                                        }
                                    }
                                }
                                // Shift: the direction from the last point in 45° steps (in plan)
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
                        // as in Navisworks: click on a surface = light placed there
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
                // double-click = the last point (already added by the click) and done
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
                self.clash.focus = None;
                ui.close_menu();
            }
            if ui.button("Clear selection (Esc)").clicked() {
                self.clear_selection();
                ui.close_menu();
            }
        });

        // --------------------------------------------------------- overlays
        let vp = params.view_proj;
        let line = |a: Vec3, b: Vec3, col: Color32, w: f32| {
            if let (Some(sa), Some(sb)) = (to_screen(&vp, rect, a), to_screen(&vp, rect, b)) {
                painter.line_segment([sa, sb], Stroke::new(w, col));
            }
        };
        // section planes: transparent green surfaces, as large as the scene
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
                    // subdivided, so it stays correct when part of it is behind the camera
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
        // The cut plane IS PART OF THE BOX: it is drawn inside it, it
        // moves with it and is DRAGGED DIRECTLY with the mouse (the arrow handle in the centre).
        // The highlight is OFF by default: it only appears when the user has picked a
        // FACE of the box in Cut View (Top / Bottom / Front / Back / Left / Right).
        // In Current mode (default) no plane is shown.
        // the plane of the chosen view (light green): always visible with the section box - the
        // „Highlight plane" checkbox is no longer needed; without a box the option from Settings remains
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
                    // same as for export: the BOX faces when it is active, otherwise the scene
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
            // In box mode the plane sits EXACTLY where the box face is (the box is made of
            // planes; no jumps on the grid). Without a box, the named views
            // align to the grid / level; while dragging it follows the mouse.
            let value = if self.cut_drag || view == crate::app::CadView::Current || (self.section.enabled && self.section.mode == SectionMode::Box) { raw } else { self.snap_cut(axis, raw) };
            // extent of the plane: INSIDE THE BOX if it is active, otherwise the scene's
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
            // the cut plane: light green, like the chosen box face
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
            // ---- the drag handle: a bar along the plane axis, with arrows at both ends
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
                // grabbing and dragging the plane
                if near && !self.cut_drag && self.box_drag.is_none() && resp.drag_started_by(egui::PointerButton::Primary) {
                    self.cut_drag = true;
                }
                if self.cut_drag && ppm > 1e-3 {
                    if resp.dragged_by(egui::PointerButton::Primary) {
                        let dm = resp.drag_delta().dot(d) / ppm;
                        if view == crate::app::CadView::Current {
                            // the height of the plane inside the box
                            let (za, zb) = (self.section.bmin[2], self.section.bmax[2]);
                            self.settings.section_cut_offset = (self.settings.section_cut_offset + dm).clamp(0.05, (zb - za).max(0.06));
                        } else {
                            // the position along the axis (the box span when it is active),
                            // with the direction of the chosen view
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
        // 27.11: we NO LONGER draw the lines flattened on the plane („the 2D preview in 3D").
        // The section is now real 3D GEOMETRY: the shader clips at the box and draws
        // the hatched cap on the plane; everything in the box beyond the plane is seen directly.
        if self.section.enabled && self.section.mode == SectionMode::Box && self.settings.show_section_box {
            let (a, b) = (Vec3::from(self.section.bmin), Vec3::from(self.section.bmax));
            let c = |i: u32| Vec3::new(if i & 1 == 0 { a.x } else { b.x }, if i & 2 == 0 { a.y } else { b.y }, if i & 4 == 0 { a.z } else { b.z });
            for (i, j) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
                line(c(i), c(j), Color32::from_rgb(255, 190, 60), 1.5);
            }
        }
        // grids and levels: grids on the level below the camera (like Navisworks), with label bubbles at the ends;
        // levels as a discreet outline around the scene, with name and elevation
        if (self.show_grids || self.level.show_lines) && self.loaded_models().next().is_some() && self.report_job.is_none() {
            let g = self.scene_grids();
            let bb = self.scene_aabb();
            let eye = self.cam.eye();
            let gz = g.levels.iter().rev().find(|l| l.1 <= eye.z).or(g.levels.first()).map(|l| l.1).unwrap_or(if bb.is_valid() { bb.min.z } else { 0.0 });
            let gcol = Color32::from_rgba_unmultiplied(120, 175, 255, 170);
            let seg = |a: Vec3, b: Vec3, col: Color32, w: f32| {
                // small pieces: the parts behind the camera disappear without breaking the line
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
            // grids as on the drawing sheet: dash-dot line (axis), white bubble with outline and label, placed BEYOND the line end;
            // the bubbles have a fixed size on screen (visible from afar too), the lines can be extended (Edit Grids › Extend lines)
            let br = self.grid_bubble.clamp(6.0, 30.0);
            let ink = Color32::from_rgb(40, 60, 95);
            for (tag, a, b) in g.grids.iter().filter(|_| self.show_grids) {
                let (mut a, mut b) = (Vec3::new(a.x, a.y, gz), Vec3::new(b.x, b.y, gz));
                let dir = (b - a).normalize_or_zero();
                a -= dir * self.grid_ext;
                b += dir * self.grid_ext;
                // the polyline on screen (in pieces, so it stays correct when part of it is behind the camera)
                let n = 32;
                let pts: Vec<Option<Pos2>> = (0..=n).map(|k| to_screen(&vp, rect, a.lerp(b, k as f32 / n as f32))).collect();
                let mut run: Vec<Pos2> = Vec::new();
                let flush = |run: &mut Vec<Pos2>| {
                    if run.len() >= 2 {
                        // dash-dot: long dash, gap, dot, gap
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
                // the bubbles: in the extension of the line, tangent at the end
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
                // 0.28: the labels do not go under the bottom navigation bar and do not overlap each other
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
                // bottom to top: each label sits at its level; one that would overlap another is no longer
                // shown (the level line remains) - before, they stacked in a column far from the level (~20 levels)
                labels.sort_by(|a, b| b.0.y.total_cmp(&a.0.y));
                let mut skipped = 0;
                for (s, text) in labels {
                    // up to two rows higher (the label stays near its level); otherwise it is hidden
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
        // relocation in progress: the picked points, the line to the cursor and the hint for the next step
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
            // capture zone around the cursor (like the AutoCAD aperture): the OSNAP radius, faint
            if let (true, Some(hp)) = (self.osnap_on, hover) {
                if rect.contains(hp) {
                    painter.circle_stroke(hp, self.settings.snap_radius, Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(120, 230, 255, 70)));
                }
            }
            if let Some(sp) = snap.and_then(|s| to_screen(&vp, rect, s)) {
                // snap marker, sized with the OSNAP radius: square = vertex, triangle = midpoint,
                // X = on an edge, circle = surface
                let z = (self.settings.snap_radius / 12.0).clamp(0.8, 3.0);
                let st = Stroke::new(2.0 * z.sqrt(), Color32::from_rgb(120, 230, 255));
                match snap_kind {
                    1 => {
                        painter.rect_stroke(Rect::from_center_size(sp, EV::splat(11.0 * z)), 0.0, st);
                    }
                    2 => {
                        let pts = vec![sp + EV::new(0.0, -7.0) * z, sp + EV::new(6.5, 5.0) * z, sp + EV::new(-6.5, 5.0) * z];
                        painter.add(egui::Shape::closed_line(pts, st));
                    }
                    3 => {
                        painter.line_segment([sp + EV::new(-5.0, -5.0) * z, sp + EV::new(5.0, 5.0) * z], st);
                        painter.line_segment([sp + EV::new(-5.0, 5.0) * z, sp + EV::new(5.0, -5.0) * z], st);
                    }
                    _ => {
                        painter.circle_stroke(sp, 6.0 * z, st);
                    }
                }
                let lbl = ["", "Vertex", "Midpoint", "Edge"][snap_kind as usize];
                if !lbl.is_empty() {
                    painter.text(sp + EV::splat(7.0 * z + 4.0), Align2::LEFT_TOP, lbl, FontId::proportional(10.5 + z), Color32::from_rgb(120, 230, 255));
                }
            }
            let hint = match k {
                MeasureKind::PointToPoint => "Click two points",
                MeasureKind::Angle => "Click: point, vertex, point",
                _ => "Click to add points · double-click / Esc to finish",
            };
            painter.text(rect.center_top() + EV::new(0.0, 14.0), Align2::CENTER_TOP, format!("Measure: {} — {hint}", k.label()), FontId::proportional(14.0), pal.overlay_text);
        }
        // walk path: smoothed line + numbered points
        if !self.walk_path.is_empty() && (self.tool == Tool::WalkPath || self.nav == NavMode::Walk || self.path_run.is_some()) && self.report_job.is_none() {
            let pc = Color32::from_rgb(60, 200, 255);
            let lift = Vec3::Z * 0.05;
            let dense = crate::app::walk::densify(&self.walk_path, 0.25);
            // discreet shadow under the line (readable on any background), then the line
            for w in dense.windows(2) {
                line(w[0] + lift, w[1] + lift, Color32::from_black_alpha(90), 6.0);
            }
            for w in dense.windows(2) {
                line(w[0] + lift, w[1] + lift, pc, 3.0);
            }
            // direction arrows every ~1.5 m
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
            // the length of each segment
            if self.tool == Tool::WalkPath {
                for k in 1..self.walk_path.len() {
                    let (a, b) = (self.walk_path[k - 1], self.walk_path[k]);
                    if let Some(s) = to_screen(&vp, rect, (a + b) * 0.5 + lift) {
                        label_bg_center(&painter, s + EV::new(0.0, -6.0), &crate::app::fmt_len((b - a).length() as f64), pc);
                    }
                }
                // 3D preview of the next segment, on the floor, with its length
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
                                label_bg_center(&painter, b + EV::new(0.0, -12.0), &crate::app::fmt_len((q - last).length() as f64), pc);
                            }
                        }
                    }
                }
            }
            let n_pts = self.walk_path.len();
            for (i, p) in self.walk_path.iter().enumerate() {
                if let Some(sp) = to_screen(&vp, rect, *p + lift) {
                    // start green, end red, intermediate ones blue (with outline, visible on any background)
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
        // the render frame (Render tab)
        if self.panel_shown(Panel::Render) {
            let ta = self.render.width as f32 / self.render.height.max(1) as f32;
            // the frame only appears when you turn it on (Render › Show frame)
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
        // axes gizmo
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
        // The ViewCube is a global overlay: in split view we draw it only once (on the active half).
        // The navigation bar is global - it is drawn only once in update(), not here.
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
        // states
        if self.start_screen() {
            // start page (like Navisworks): Open / New + recent files
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| self.start_page(ui));
        } else if self.loaded_models().next().is_none() && self.loads.is_empty() {
            // new / empty project: empty scene, with a discreet hint
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
            // Cancel: a large file opened by mistake no longer keeps the application busy for whole minutes
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
                // below the Ray Trace label when that is shown
                rect.left_top() + EV::new(10.0, if self.rt.on { 34.0 } else { 8.0 }),
                Align2::LEFT_TOP,
                if self.nav == NavMode::Walk {
                    "Walk · drag up / down: forward / back · sideways: turn · right-drag: look · W A S D".to_string()
                } else if self.nav == NavMode::Pan {
                    "Pan · drag: move the model · wheel: zoom · right-drag: look · click: select".to_string()
                } else {
                    "Click: select · drag: orbit · middle-drag: pan · wheel: zoom · double-click: focus".to_string()
                },
                FontId::proportional(12.0),
                // on a light background (Hidden line, white background / sky) the text is dark, otherwise it is not visible
                if self.settings.visual_style == crate::render::VisualStyle::Hidden || matches!(self.settings.vp_bg.as_str(), "white" | "sky") { Color32::from_rgb(90, 96, 106) } else { pal.overlay_weak },
            );
        }
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
