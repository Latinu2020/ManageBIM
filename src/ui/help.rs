//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    /// Paleta de comenzi: Ctrl+K (sau clic pe "Search or Ctrl+K"), scrii, Enter.
    pub(super) fn command_palette(&mut self, ctx: &egui::Context) {
        if !self.palette_open {
            return;
        }
        let cmds = commands();
        let q = self.palette_q.trim().to_lowercase();
        let mut hits: Vec<(i32, usize)> = cmds
            .iter()
            .enumerate()
            .filter_map(|(i, c)| fuzzy(&q, &format!("{} {} {}", c.name, c.cat, c.keys)).map(|s| (s + if fuzzy(&q, c.name).is_some() { 50 } else { 0 }, i)))
            .collect();
        hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        hits.truncate(12);
        // elemente din model cu numele cautat (selectie + zoom)
        // cautarea in elemente se face o singura data pe text (nu la fiecare cadru): rezultatul e tinut in memorie
        let cache_id = egui::Id::new("palette_elems");
        let cached: Option<(String, Vec<(ElemId, String, String)>)> = ctx.data(|d| d.get_temp(cache_id));
        let mut elems: Vec<(ElemId, String, String)> = Vec::new();
        if let Some((cq, v)) = cached.filter(|(cq, _)| *cq == q) {
            let _ = cq;
            elems = v;
        } else if q.len() >= 2 {
            'outer: for (mi, m) in self.loaded_models() {
                for (ei, e) in m.elements.iter().enumerate() {
                    if e.name.to_lowercase().contains(&q) || e.guid.to_lowercase() == q {
                        elems.push((ElemId { m: mi as u32, e: ei as u32 }, e.name.clone(), e.ifc_type.clone()));
                        if elems.len() >= 6 {
                            break 'outer;
                        }
                    }
                }
            }
        }
        ctx.data_mut(|d| d.insert_temp(cache_id, (q.clone(), elems.clone())));
        let total = hits.len() + elems.len();
        if self.palette_sel >= total.max(1) {
            self.palette_sel = total.saturating_sub(1);
        }
        let (up, down, enter, esc) = ctx.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        if up {
            self.palette_sel = self.palette_sel.saturating_sub(1);
        }
        if down && self.palette_sel + 1 < total {
            self.palette_sel += 1;
        }
        let mut chosen: Option<usize> = if enter && total > 0 { Some(self.palette_sel) } else { None };
        let screen = ctx.screen_rect();
        let w = 560.0_f32.min(screen.width() - 40.0);
        // fundal estompat; clic in afara inchide
        let bg = egui::Area::new(egui::Id::new("palette_bg")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(90));
            resp.clicked()
        });
        let mut close = esc || bg.inner;
        egui::Area::new(egui::Id::new("palette")).order(egui::Order::Tooltip).fixed_pos(Pos2::new(screen.center().x - w / 2.0, screen.top() + 90.0)).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).rounding(12.0).inner_margin(10.0).show(ui, |ui| {
                ui.set_width(w);
                let te = ui.add(egui::TextEdit::singleline(&mut self.palette_q).hint_text("Type a command or an element name…  (↑ ↓ Enter, Esc)").desired_width(f32::INFINITY).font(egui::FontId::proportional(16.0)).margin(egui::vec2(8.0, 6.0)));
                te.request_focus();
                if te.changed() {
                    self.palette_sel = 0;
                }
                ui.add_space(6.0);
                let mut k = 0usize;
                let mut last_cat = "";
                for (_, i) in &hits {
                    let c = &cmds[*i];
                    if c.cat != last_cat {
                        last_cat = c.cat;
                        ui.label(RichText::new(c.cat.to_uppercase()).small().weak());
                    }
                    let sel = k == self.palette_sel;
                    let col = if sel { ui.visuals().strong_text_color() } else { ui.visuals().text_color() };
                    let (r, resp) = ui.allocate_exact_size(EV::new(w, 26.0), Sense::click());
                    if sel {
                        ui.painter().rect_filled(r, 6.0, accent().gamma_multiply(0.2));
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, 6.0, ui.visuals().widgets.hovered.weak_bg_fill);
                    }
                    ui.painter().text(r.left_center() + EV::new(16.0, 0.0), Align2::CENTER_CENTER, crate::theme::ph(c.icon), crate::theme::icon_font(15.0), if sel { accent() } else { ui.visuals().weak_text_color() });
                    ui.painter().text(r.left_center() + EV::new(34.0, 0.0), Align2::LEFT_CENTER, c.name, FontId::proportional(14.0), col);
                    if resp.clicked() {
                        chosen = Some(k);
                    }
                    if resp.hovered() && ui.input(|i| i.pointer.delta().length() > 0.0) {
                        self.palette_sel = k;
                    }
                    k += 1;
                }
                if !elems.is_empty() {
                    ui.label(RichText::new("ELEMENTS").small().weak());
                    for (_, name, ty) in &elems {
                        let sel = k == self.palette_sel;
                        let (r, resp) = ui.allocate_exact_size(EV::new(w, 24.0), Sense::click());
                        if sel {
                            ui.painter().rect_filled(r, 6.0, accent().gamma_multiply(0.2));
                        }
                        ui.painter().text(r.left_center() + EV::new(16.0, 0.0), Align2::CENTER_CENTER, egui_phosphor::regular::CUBE, crate::theme::icon_font(14.0), ui.visuals().weak_text_color());
                        ui.painter().text(r.left_center() + EV::new(34.0, 0.0), Align2::LEFT_CENTER, format!("{name}   ·  {ty}"), FontId::proportional(13.5), ui.visuals().text_color());
                        if resp.clicked() {
                            chosen = Some(k);
                        }
                        k += 1;
                    }
                }
                if total == 0 {
                    ui.label(RichText::new("Nothing found.").weak());
                }
            });
        });
        if let Some(k) = chosen {
            close = true;
            if k < hits.len() {
                (cmds[hits[k].1].run)(self);
            } else if let Some((id, _, _)) = elems.get(k - hits.len()) {
                self.apply(Action::Select(vec![*id], false));
                self.apply(Action::Focus(vec![*id]));
            }
        }
        if close {
            self.palette_open = false;
            self.palette_q.clear();
            self.palette_sel = 0;
        }
    }

    /// Fereastra manualului: cuprins + cautare in stanga, capitolul in dreapta.
    pub(super) fn manual_window(&mut self, ctx: &egui::Context) {
        if !self.manual_open {
            return;
        }
        let mut open = true;
        let mut demo = false;
        egui::Window::new(crate::theme::icon_job("❓", "ManageBIM manual", 14.0, ctx.style().visuals.text_color()))
            .id(egui::Id::new("manual_win"))
            .open(&mut open)
            .default_size([760.0, 520.0])
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    // ---- cuprins
                    ui.vertical(|ui| {
                        ui.set_width(210.0);
                        ui.add(egui::TextEdit::singleline(&mut self.manual_q).hint_text("🔍 Search the manual").desired_width(200.0));
                        ui.add_space(4.0);
                        let q = self.manual_q.trim().to_lowercase();
                        egui::ScrollArea::vertical().id_salt("manual_toc").max_height(ui.available_height() - 40.0).show(ui, |ui| {
                            for (i, (icon, title, body)) in MANUAL.iter().enumerate() {
                                if !q.is_empty() && !title.to_lowercase().contains(&q) && !body.to_lowercase().contains(&q) {
                                    continue;
                                }
                                let sel = self.manual_page == i;
                                let b = egui::Button::new(crate::theme::icon_job(icon, title, 13.5, if sel { accent() } else { ui.visuals().text_color() }))
                                    .fill(if sel { accent().gamma_multiply(0.16) } else { Color32::TRANSPARENT })
                                    .stroke(Stroke::NONE)
                                    .rounding(6.0)
                                    .min_size(EV::new(200.0, 26.0));
                                if ui.add(b).clicked() {
                                    self.manual_page = i;
                                }
                            }
                        });
                        ui.add_space(6.0);
                        if ui.add(egui::Button::new(RichText::new("Open demo project").strong().color(Color32::WHITE)).fill(accent()).rounding(7.0).min_size(EV::new(200.0, 28.0))).clicked() {
                            demo = true;
                        }
                    });
                    // ---- capitolul
                    ui.vertical(|ui| {
                        let (icon, title, body) = MANUAL[self.manual_page.min(MANUAL.len() - 1)];
                        ui.label(crate::theme::icon_job(icon, title, 20.0, ui.visuals().strong_text_color()));
                        ui.add_space(6.0);
                        let q = self.manual_q.trim().to_lowercase();
                        egui::ScrollArea::vertical().id_salt("manual_body").auto_shrink([false, false]).show(ui, |ui| {
                            for line in body.lines() {
                                let line = line.trim_end();
                                let hit = !q.is_empty() && line.to_lowercase().contains(&q);
                                if let Some(h) = line.strip_prefix("# ") {
                                    ui.add_space(6.0);
                                    ui.label(RichText::new(h.to_uppercase()).small().strong().color(ui.visuals().weak_text_color()));
                                } else if let Some(t) = line.strip_prefix("> ") {
                                    egui::Frame::none().fill(accent().gamma_multiply(0.12)).rounding(7.0).inner_margin(8.0).show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.label(crate::theme::icon_job("💡", t, 13.5, ui.visuals().text_color()));
                                    });
                                } else if let Some(t) = line.strip_prefix("- ") {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(RichText::new("•").color(accent()));
                                        let mut rt = RichText::new(t).size(13.5);
                                        if hit {
                                            rt = rt.background_color(accent().gamma_multiply(0.25));
                                        }
                                        ui.label(rt);
                                    });
                                } else if !line.is_empty() {
                                    let mut rt = RichText::new(line).size(13.5);
                                    if hit {
                                        rt = rt.background_color(accent().gamma_multiply(0.25));
                                    }
                                    ui.label(rt);
                                }
                                ui.add_space(2.0);
                            }
                        });
                    });
                });
            });
        if !open {
            self.manual_open = false;
        }
        if demo {
            self.open_demo();
        }
    }
}
