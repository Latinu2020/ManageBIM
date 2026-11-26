//! Level & Storey Manager: the level schedule (name, elevation from ±0.00, storey height, real Z)
//! and an orthographic view of the model (front / side) with the level lines over it.
use super::*;
use crate::model::{Aabb, ElemId, Model};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Direction of the orthographic view.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum OrthoDir {
    /// looking towards +Y: horizontal = X
    #[default]
    Front,
    /// looking towards -X: horizontal = Y
    Side,
}

/// The orthographic image: RGBA pixels and the world rectangle it covers (horizontal u0..u1, vertical z0..z1).
pub(crate) struct Ortho {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
    pub u0: f32,
    pub u1: f32,
    pub z0: f32,
    pub z1: f32,
}

/// CPU rasterisation (z-buffer) of the visible triangles, projected orthographically on a vertical plane
/// and shaded by their normal. The longest side of the image has `size` pixels.
pub(crate) fn ortho_elevation(models: &[Arc<Model>], hidden: &HashSet<ElemId>, slots: &[usize], dir: OrthoDir, bb: &Aabb, size: usize) -> Ortho {
    let (u0, u1) = match dir {
        OrthoDir::Front => (bb.min.x, bb.max.x),
        OrthoDir::Side => (bb.min.y, bb.max.y),
    };
    let (z0, z1) = (bb.min.z, bb.max.z);
    let (du, dz) = ((u1 - u0).max(1e-3), (z1 - z0).max(1e-3));
    let s = size as f32 / du.max(dz);
    let (w, h) = (((du * s).ceil() as usize).clamp(1, size), ((dz * s).ceil() as usize).clamp(1, size));
    let mut rgba = vec![0u8; w * h * 4];
    let mut depth = vec![f32::INFINITY; w * h];
    let light = Vec3::new(-0.45, -0.75, 0.5).normalize();
    // projection: (column, row, depth); smaller depth = closer to the viewer
    let proj = |p: Vec3| -> (f32, f32, f32) {
        let (u, d) = match dir {
            OrthoDir::Front => (p.x, p.y),
            OrthoDir::Side => (p.y, -p.x),
        };
        ((u - u0) * s, (z1 - p.z) * s, d)
    };
    for (m, &slot) in models.iter().zip(slots) {
        for (ei, e) in m.elements.iter().enumerate() {
            if e.default_hidden || hidden.contains(&ElemId { m: slot as u32, e: ei as u32 }) {
                continue;
            }
            for t in e.tri_start as usize..(e.tri_start + e.tri_count) as usize {
                let Some(v) = m.pos.get(t * 3..t * 3 + 3) else { break };
                let c = m.tri_color.get(t).copied().unwrap_or([200, 200, 200, 255]);
                if c[3] < 8 {
                    continue;
                }
                let n = (v[1] - v[0]).cross(v[2] - v[0]);
                let nl = n.length();
                if nl < 1e-12 {
                    continue;
                }
                let shade = 0.38 + 0.62 * (n / nl).dot(light).abs();
                let col = [(c[0] as f32 * shade) as u8, (c[1] as f32 * shade) as u8, (c[2] as f32 * shade) as u8];
                let (a, b, cc) = (proj(v[0]), proj(v[1]), proj(v[2]));
                let area = (b.0 - a.0) * (cc.1 - a.1) - (b.1 - a.1) * (cc.0 - a.0);
                let x0 = a.0.min(b.0).min(cc.0).floor().max(0.0) as usize;
                let x1 = (a.0.max(b.0).max(cc.0).ceil() as usize).min(w);
                let y0 = a.1.min(b.1).min(cc.1).floor().max(0.0) as usize;
                let y1 = (a.1.max(b.1).max(cc.1).ceil() as usize).min(h);
                if x0 >= x1 || y0 >= y1 {
                    continue;
                }
                if area.abs() < 1e-6 {
                    // triangle seen edge-on (wall parallel to the view): at least a one-pixel outline
                    for p in [a, b, cc] {
                        let (x, y) = (p.0 as usize, p.1 as usize);
                        if x < w && y < h && p.2 < depth[y * w + x] {
                            depth[y * w + x] = p.2;
                            rgba[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&[col[0], col[1], col[2], 255]);
                        }
                    }
                    continue;
                }
                for y in y0..y1 {
                    let py = y as f32 + 0.5;
                    for x in x0..x1 {
                        let px = x as f32 + 0.5;
                        let w0 = ((b.0 - px) * (cc.1 - py) - (b.1 - py) * (cc.0 - px)) / area;
                        let w1 = ((cc.0 - px) * (a.1 - py) - (cc.1 - py) * (a.0 - px)) / area;
                        let w2 = 1.0 - w0 - w1;
                        if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                            continue;
                        }
                        let d = w0 * a.2 + w1 * b.2 + w2 * cc.2;
                        let k = y * w + x;
                        if d < depth[k] {
                            depth[k] = d;
                            rgba[k * 4..k * 4 + 4].copy_from_slice(&[col[0], col[1], col[2], 255]);
                        }
                    }
                }
            }
        }
    }
    Ortho { w, h, rgba, u0, u1, z0, z1 }
}

/// Key of the cached image: the loaded models, the hidden items, the direction.
fn ortho_key(app: &App, dir: OrthoDir) -> u64 {
    let mut hs = std::collections::hash_map::DefaultHasher::new();
    for (slot, m) in app.loaded_models() {
        (slot, Arc::as_ptr(m) as usize).hash(&mut hs);
    }
    app.hidden.len().hash(&mut hs);
    dir.hash(&mut hs);
    hs.finish()
}

fn fmt_elev(v: f64) -> String {
    if v.abs() < 0.0005 {
        "±0.000".into()
    } else {
        format!("{v:+.3}")
    }
}

impl App {
    /// The Level & Storey Manager window (replaces the old level editor).
    pub(super) fn levels_manager(&mut self, ctx: &egui::Context) {
        if !self.level.show_editor {
            return;
        }
        let mut open = true;
        let oz = self.origin_f64().z;
        egui::Window::new(format!("{} Level & Storey Manager", egui_phosphor::regular::STACK))
            .open(&mut open)
            .collapsible(false)
            .default_size([980.0, 560.0])
            .min_width(640.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.level.custom, "Use my levels").on_hover_text("Off: the levels found in the models are used (read-only here)");
                    ui.separator();
                    ui.label("±0.00 at real Z =");
                    ui.add(egui::DragValue::new(&mut self.level.zero).speed(0.01).fixed_decimals(3).suffix(" m")).on_hover_text("The project zero: every elevation below is measured from here");
                    if ui.small_button("Lowest model level").clicked() {
                        if let Some(l) = self.scene_grids_models().levels.first() {
                            self.level.zero = l.1 as f64 + oz;
                        }
                    }
                    if ui.small_button("Model bottom").on_hover_text("Put ±0.00 at the lowest point of the geometry").clicked() {
                        let bb = self.scene_aabb();
                        if bb.is_valid() {
                            self.level.zero = bb.min.z as f64 + oz;
                        }
                    }
                });
                ui.separator();
                let avail = ui.available_size();
                let left_w = (avail.x * 0.48).clamp(380.0, 520.0);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(egui::vec2(left_w, avail.y.max(120.0)), egui::Layout::top_down(egui::Align::Min), |ui| self.levels_schedule(ui, oz));
                    ui.separator();
                    ui.allocate_ui_with_layout(egui::vec2(ui.available_width().max(240.0), avail.y.max(220.0)), egui::Layout::top_down(egui::Align::Min), |ui| self.levels_ortho(ui, oz));
                });
            });
        self.level.show_editor = open;
    }

    /// The level schedule, top to bottom (as in Revit): name, elevation, storey height, real Z, actions.
    fn levels_schedule(&mut self, ui: &mut egui::Ui, oz: f64) {
        let custom = self.level.custom;
        // the levels shown: mine (editable) or the models' (read-only), elevations from ±0.00
        let rows: Vec<(String, f64)> = if custom {
            self.level.user.iter().map(|l| (l.name.clone(), l.elev)).collect()
        } else {
            self.scene_grids_models().levels.iter().map(|(n, z)| (n.clone(), *z as f64 + oz - self.level.zero)).collect()
        };
        let mut order: Vec<usize> = (0..rows.len()).collect();
        order.sort_by(|a, b| rows[*b].1.total_cmp(&rows[*a].1));
        enum Op {
            Del(usize),
            Above(usize),
            Below(usize),
            Sort,
        }
        let mut op = None;
        egui::ScrollArea::vertical().max_height((ui.available_height() - 70.0).max(120.0)).auto_shrink([false, true]).show(ui, |ui| {
            egui::Grid::new("lvl_schedule").num_columns(6).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                for h in ["Level", "Elevation", "Height", "Real Z", "", ""] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for (k, &i) in order.iter().enumerate() {
                    let height = if k > 0 { Some(rows[order[k - 1]].1 - rows[i].1) } else { None };
                    let zero = rows[i].1.abs() < 0.0005;
                    if custom {
                        let l = &mut self.level.user[i];
                        ui.add(egui::TextEdit::singleline(&mut l.name).desired_width(140.0));
                        let r = ui.add(egui::DragValue::new(&mut l.elev).speed(0.01).fixed_decimals(3).suffix(" m"));
                        if r.drag_stopped() || r.lost_focus() {
                            op = Some(Op::Sort);
                        }
                    } else {
                        ui.label(&rows[i].0);
                        ui.label(format!("{} m", fmt_elev(rows[i].1)));
                    }
                    ui.label(height.map(|h| format!("{h:.3} m")).unwrap_or_else(|| "—".into())).on_hover_text("Storey height: distance to the level above");
                    ui.label(RichText::new(format!("{:.3}", rows[i].1 + self.level.zero)).weak()).on_hover_text("Absolute Z in the model coordinates (m)");
                    if zero {
                        ui.label(RichText::new("±0").color(Color32::from_rgb(220, 70, 60)).strong()).on_hover_text("This level is the project zero");
                    } else {
                        ui.label("");
                    }
                    if custom {
                        ui.horizontal(|ui| {
                            if ui.small_button("⬆").on_hover_text("Add a level above").clicked() {
                                op = Some(Op::Above(i));
                            }
                            if ui.small_button("⬇").on_hover_text("Add a level below").clicked() {
                                op = Some(Op::Below(i));
                            }
                            if ui.small_button("✖").on_hover_text("Delete").clicked() {
                                op = Some(Op::Del(i));
                            }
                        });
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
            if rows.is_empty() {
                ui.label(RichText::new("No levels yet.").weak());
            }
        });
        // insert: above = with the height of the storey below (or 3 m); below = half way / 3 m lower
        let typical = |i: usize, lv: &[crate::app::UserLevel]| -> f64 {
            let mut z: Vec<f64> = lv.iter().map(|l| l.elev).collect();
            z.sort_by(f64::total_cmp);
            let p = z.iter().position(|v| *v >= lv[i].elev).unwrap_or(0);
            if p > 0 { (z[p] - z[p - 1]).max(2.5) } else { 3.0 }
        };
        match op {
            Some(Op::Del(i)) => {
                self.level.user.remove(i);
            }
            Some(Op::Above(i)) => {
                let lv = &self.level.user;
                let above = lv.iter().map(|l| l.elev).filter(|e| *e > lv[i].elev + 1e-6).fold(f64::INFINITY, f64::min);
                let elev = if above.is_finite() { (lv[i].elev + above) / 2.0 } else { lv[i].elev + typical(i, lv) };
                let n = self.level.user.len();
                self.level.user.push(crate::app::UserLevel { name: format!("Level {n}"), elev: (elev * 1000.0).round() / 1000.0 });
                self.level.user.sort_by(|a, b| a.elev.total_cmp(&b.elev));
            }
            Some(Op::Below(i)) => {
                let lv = &self.level.user;
                let below = lv.iter().map(|l| l.elev).filter(|e| *e < lv[i].elev - 1e-6).fold(f64::NEG_INFINITY, f64::max);
                let elev = if below.is_finite() { (lv[i].elev + below) / 2.0 } else { lv[i].elev - 3.0 };
                let n = self.level.user.len();
                self.level.user.push(crate::app::UserLevel { name: format!("Level {n}"), elev: (elev * 1000.0).round() / 1000.0 });
                self.level.user.sort_by(|a, b| a.elev.total_cmp(&b.elev));
            }
            Some(Op::Sort) => self.level.user.sort_by(|a, b| a.elev.total_cmp(&b.elev)),
            None => {}
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("➕ Add level").on_hover_text("On top of the highest level").clicked() {
                let top = self.level.user.iter().map(|l| l.elev).fold(f64::NEG_INFINITY, f64::max);
                let elev = if top.is_finite() { top + 3.0 } else { 0.0 };
                let n = self.level.user.len();
                self.level.user.push(crate::app::UserLevel { name: if n == 0 { "Ground Floor".into() } else { format!("Level {n}") }, elev });
                self.level.custom = true;
            }
            if ui.button("⤓ Copy from models").on_hover_text("Start from the levels found in the loaded models (then edit them)").clicked() {
                let g = self.scene_grids_models();
                self.level.user = g.levels.iter().map(|(n, z)| crate::app::UserLevel { name: n.clone(), elev: ((*z as f64 + oz - self.level.zero) * 1000.0).round() / 1000.0 }).collect();
                self.level.custom = true;
            }
        });
        ui.collapsing("What do the levels change?", |ui| {
            ui.label(
                RichText::new(
                    "Only labels and references, never the geometry: the level lines and tags in the 3D view, \
                     the location of clashes and issues (e.g. C-4 / Level 2), section by level, the Dashboard and \
                     the levels sent to Plant 3D / Navisworks. Saved with the project (.bimr).",
                )
                .small(),
            );
        });
    }

    /// Orthographic view of the model with the level lines: zoom, pan, and drag a line to move a level.
    fn levels_ortho(&mut self, ui: &mut egui::Ui, oz: f64) {
        let did = egui::Id::new("lvl_ortho_dir");
        let mut dir: OrthoDir = ui.data(|d| d.get_temp(did)).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Ortho elevation").strong());
            ui.selectable_value(&mut dir, OrthoDir::Front, "Front (X)");
            ui.selectable_value(&mut dir, OrthoDir::Side, "Side (Y)");
        });
        ui.data_mut(|d| d.insert_temp(did, dir));
        let bb = self.scene_aabb();
        let levels: Vec<(String, f64)> = self.scene_grids().levels.iter().map(|(n, z)| (n.clone(), *z as f64 + oz - self.level.zero)).collect();
        // the image: redrawn only when the models, the hidden items or the direction change
        let key = ortho_key(self, dir);
        let tid = egui::Id::new("lvl_ortho_tex");
        let cached: Option<(u64, egui::TextureHandle, [f32; 4])> = ui.data(|d| d.get_temp(tid));
        let img = match cached {
            Some((k, t, r)) if k == key => Some((t, r)),
            _ if bb.is_valid() => {
                let (slots, models): (Vec<usize>, Vec<Arc<Model>>) = self.loaded_models().map(|(s, m)| (s, m.clone())).unzip();
                let o = ortho_elevation(&models, &self.hidden, &slots, dir, &bb, 1400);
                let ci = egui::ColorImage::from_rgba_unmultiplied([o.w, o.h], &o.rgba);
                let t = ui.ctx().load_texture("lvl_ortho", ci, egui::TextureOptions::LINEAR);
                let r = [o.u0, o.u1, o.z0, o.z1];
                ui.data_mut(|d| d.insert_temp(tid, (key, t.clone(), r)));
                Some((t, r))
            }
            _ => None,
        };
        let size = ui.available_size().max(egui::vec2(240.0, 220.0));
        let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
        // vertical range: the geometry plus every level, with a margin
        let (mut zlo, mut zhi) = img.as_ref().map(|(_, r)| (r[2] as f64 + oz, r[3] as f64 + oz)).unwrap_or((f64::INFINITY, f64::NEG_INFINITY));
        for (_, e) in &levels {
            zlo = zlo.min(e + self.level.zero);
            zhi = zhi.max(e + self.level.zero);
        }
        zlo = zlo.min(self.level.zero);
        zhi = zhi.max(self.level.zero);
        if !zlo.is_finite() {
            ui.label("Load a model to see the elevation.");
            return;
        }
        let pad = ((zhi - zlo) * 0.08).max(0.5);
        let (zlo, zhi) = (zlo - pad, zhi + pad);
        let (ml, mr) = (70.0, 150.0); // left: storey heights; right: labels
        let plot = egui::Rect::from_min_max(egui::pos2(rect.min.x + ml, rect.min.y + 8.0), egui::pos2(rect.max.x - mr, rect.max.y - 8.0));
        let span_u = img.as_ref().map(|(_, r)| (r[1] - r[0]) as f64).unwrap_or(1.0).max(1e-3);

        // zoom (wheel / pinch, around the cursor) and pan (drag the empty area, or middle / right button);
        // double-click = fit everything again
        let vid = egui::Id::new("lvl_ortho_view");
        let (mut zoom, mut pan): (f32, egui::Vec2) = ui.data(|d| d.get_temp(vid)).unwrap_or((1.0, egui::Vec2::ZERO));
        let c = plot.center();
        if let Some(h) = resp.hover_pos() {
            let (wheel, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let f = (wheel * 0.0025).exp() * pinch;
            if (f - 1.0).abs() > 1e-4 {
                let nz = (zoom * f).clamp(1.0, 60.0);
                pan = (h - c) - ((h - c) - pan) * (nz / zoom);
                zoom = nz;
                if zoom <= 1.0 + 1e-3 {
                    pan = egui::Vec2::ZERO;
                }
            }
        }
        if resp.double_clicked() {
            zoom = 1.0;
            pan = egui::Vec2::ZERO;
        }
        // same scale both ways (orthographic, not stretched), then the zoom / pan
        let sc = (plot.height() as f64 / (zhi - zlo)).min(plot.width() as f64 / (span_u * 1.1)) * zoom as f64;
        let zc = (zlo + zhi) / 2.0;
        let cy = c.y + pan.y;
        let to_y = |z: f64| cy - ((z - zc) * sc) as f32;
        let from_y = |y: f32| zc + (cy - y) as f64 / sc;
        if let Some((t, r)) = &img {
            let wpx = (span_u * sc) as f32;
            let x0 = c.x + pan.x - wpx / 2.0;
            let ir = egui::Rect::from_min_max(egui::pos2(x0, to_y(r[3] as f64 + oz)), egui::pos2(x0 + wpx, to_y(r[2] as f64 + oz)));
            p.image(t.id(), ir, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        let text = ui.visuals().text_color();
        let weak = ui.visuals().weak_text_color();
        let font = egui::FontId::proportional(11.5);
        // ±0.00: thick red line
        let y0 = to_y(self.level.zero);
        let red = Color32::from_rgb(220, 70, 60);
        p.line_segment([egui::pos2(rect.min.x + 4.0, y0), egui::pos2(rect.max.x - 4.0, y0)], egui::Stroke::new(2.0_f32, red));
        p.text(egui::pos2(rect.min.x + 6.0, y0 - 2.0), egui::Align2::LEFT_BOTTOM, format!("±0.00 = Z {:.3}", self.level.zero), font.clone(), red);
        // the levels: dashed line, label with name and elevation, storey height between two levels
        let blue = Color32::from_rgb(70, 140, 230);
        let mut sorted: Vec<&(String, f64)> = levels.iter().collect();
        sorted.sort_by(|a, b| a.1.total_cmp(&b.1));
        let hover_y = resp.hover_pos().map(|h| h.y);
        let drag_id = egui::Id::new("lvl_drag");
        // the level being dragged, by name (indices change when the list is re-sorted)
        let mut dragging: Option<String> = ui.data(|d| d.get_temp(drag_id));
        let near = |h: f32| sorted.iter().map(|(n, e)| (n.clone(), (to_y(e + self.level.zero) - h).abs())).filter(|x| x.1 < 6.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|x| x.0);
        let hot_name = dragging.clone().or_else(|| hover_y.and_then(near));
        for (k, (name, e)) in sorted.iter().enumerate() {
            let y = to_y(e + self.level.zero);
            let hot = hot_name.as_deref() == Some(name.as_str());
            let st = egui::Stroke::new(if hot { 2.2_f32 } else { 1.2 }, blue);
            p.extend(egui::Shape::dashed_line(&[egui::pos2(rect.min.x + ml - 10.0, y), egui::pos2(plot.max.x + 10.0, y)], st, 8.0, 4.0));
            // elevation symbol (triangle) and label
            let tx = plot.max.x + 14.0;
            p.add(egui::Shape::convex_polygon(vec![egui::pos2(tx, y), egui::pos2(tx - 6.0, y - 8.0), egui::pos2(tx + 6.0, y - 8.0)], blue, egui::Stroke::NONE));
            p.text(egui::pos2(tx + 10.0, y - 1.0), egui::Align2::LEFT_BOTTOM, name, font.clone(), text);
            p.text(egui::pos2(tx + 10.0, y + 1.0), egui::Align2::LEFT_TOP, format!("{} m", fmt_elev(*e)), font.clone(), weak);
            if let Some((_, below)) = k.checked_sub(1).map(|j| sorted[j]) {
                let yb = to_y(below + self.level.zero);
                let x = rect.min.x + 30.0;
                p.line_segment([egui::pos2(x, y), egui::pos2(x, yb)], egui::Stroke::new(1.0_f32, weak));
                for yy in [y, yb] {
                    p.line_segment([egui::pos2(x - 4.0, yy + 4.0), egui::pos2(x + 4.0, yy - 4.0)], egui::Stroke::new(1.2_f32, weak));
                }
                if (yb - y) > 14.0 {
                    p.text(egui::pos2(x + 4.0, (y + yb) / 2.0), egui::Align2::LEFT_CENTER, format!("{:.2}", e - below), font.clone(), weak);
                }
            }
        }
        // dragging: on a level line = move that level (rounded to 1 cm); elsewhere = pan the view
        let panning_id = egui::Id::new("lvl_panning");
        let mut panning: bool = ui.data(|d| d.get_temp(panning_id)).unwrap_or(false);
        if resp.drag_started() {
            let other_button = ui.input(|i| i.pointer.button_down(egui::PointerButton::Middle) || i.pointer.button_down(egui::PointerButton::Secondary));
            let press_y = ui.input(|i| i.pointer.press_origin()).map(|p| p.y).or(hover_y);
            dragging = if other_button { None } else { press_y.and_then(near) };
            panning = dragging.is_none();
            if dragging.is_some() && !self.level.custom {
                // the model's levels become my levels (same names and elevations), so they can be moved
                let g = self.scene_grids_models();
                self.level.user = g.levels.iter().map(|(n, z)| crate::app::UserLevel { name: n.clone(), elev: ((*z as f64 + oz - self.level.zero) * 1000.0).round() / 1000.0 }).collect();
                self.level.custom = true;
                self.toast("The model levels were copied to your levels so they can be moved (Use my levels is now on).");
            }
        }
        if resp.dragged() {
            if let (Some(name), Some(pos)) = (dragging.as_ref(), resp.interact_pointer_pos()) {
                if let Some(l) = self.level.user.iter_mut().find(|l| &l.name == name) {
                    l.elev = ((from_y(pos.y) - self.level.zero) * 100.0).round() / 100.0;
                }
            } else if panning {
                pan += resp.drag_delta();
            }
        }
        if resp.drag_stopped() {
            if dragging.is_some() {
                self.level.user.sort_by(|a, b| a.elev.total_cmp(&b.elev));
            }
            dragging = None;
            panning = false;
        }
        ui.data_mut(|d| {
            d.insert_temp(vid, (zoom, pan));
            d.insert_temp(panning_id, panning);
            match &dragging {
                Some(n) => d.insert_temp(drag_id, n.clone()),
                None => d.remove::<String>(drag_id),
            }
        });
        if dragging.is_some() || (!panning && hot_name.is_some()) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        } else if panning {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        // elevation under the cursor, and how to navigate
        if let Some(h) = resp.hover_pos() {
            let z = from_y(h.y);
            p.line_segment([egui::pos2(plot.min.x, h.y), egui::pos2(plot.max.x, h.y)], egui::Stroke::new(0.5_f32, weak));
            p.text(egui::pos2(plot.min.x + 4.0, h.y - 2.0), egui::Align2::LEFT_BOTTOM, format!("{} m  (Z {:.3})", fmt_elev(z - self.level.zero), z), font.clone(), text);
        }
        let hint = if zoom > 1.0 { format!("zoom {zoom:.1}x · double-click to fit") } else { "wheel: zoom · drag: pan · drag a level line: move it".into() };
        p.text(rect.right_bottom() + egui::vec2(-6.0, -4.0), egui::Align2::RIGHT_BOTTOM, hint, egui::FontId::proportional(10.5), weak);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bvh, Element};

    /// a single vertical 2 x 1 m square in the XZ plane (front), red
    #[test]
    fn front_view_covers_the_wall() {
        let (a, b, c, d) = (Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 1.0), Vec3::new(0.0, 0.0, 1.0));
        let pos = vec![a, b, c, a, c, d];
        let bb = Aabb { min: Vec3::ZERO, max: Vec3::new(2.0, 0.0, 1.0) };
        let e = Element { name: String::new(), ifc_type: "IfcWall".into(), guid: String::new(), storey: String::new(), props: Vec::new(), tri_start: 0, tri_count: 2, aabb: bb, default_hidden: false, bvh: Bvh::default() };
        let m = Arc::new(Model { name: "m".into(), path: String::new(), elements: vec![e], pos, tri_color: vec![[255, 0, 0, 255]; 2], aabb: bb, info: Vec::new(), warnings: Vec::new(), cloud: None });
        let o = ortho_elevation(std::slice::from_ref(&m), &HashSet::new(), &[0], OrthoDir::Front, &bb, 100);
        assert_eq!((o.w, o.h), (100, 50));
        let filled = o.rgba.chunks(4).filter(|p| p[3] == 255).count();
        assert!(filled > 100 * 50 * 95 / 100, "{filled}");
        // hidden -> nothing drawn
        let hid: HashSet<ElemId> = [ElemId { m: 0, e: 0 }].into_iter().collect();
        let o = ortho_elevation(&[m], &hid, &[0], OrthoDir::Front, &bb, 100);
        assert!(o.rgba.chunks(4).all(|p| p[3] == 0));
    }
}
