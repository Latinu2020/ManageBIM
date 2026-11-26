//! Interfata grafica (egui): meniuri, bara de unelte, panouri, viewport 3D.
use crate::app::*;
use crate::clash::{ClashStatus, ClashType};
use crate::model::ElemId;
use crate::theme::{self, Palette, Theme};
use crate::timeline::{self, TaskKind};
use crate::anim::{Animation, Background};
use crate::video::VideoFormat;
use crate::materials::MatTarget;
use eframe::egui::{self, Align2, Color32, FontId, Key, Pos2, Rect, RichText, Sense, Stroke, Vec2 as EV};
use egui_extras::{Column, TableBuilder};
use glam::{Mat4, Vec2, Vec3};
use std::collections::{HashMap, HashSet};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
use std::sync::Arc;
use std::time::Instant;

mod analysis;
mod tree_props;
mod coordination;
mod time;
mod tools;
mod help;
mod output;
mod table;
mod script;
mod settings;
mod script_catalog;
mod script_builder_ui;
mod materials_panel;
mod lighting_panel;
mod classification_panel;
mod coord_panel;
mod update_panel;
mod script_window;
mod pathtrace_window;
mod prop_editor;
mod mcp_window;
mod smart_sets;
mod dashboard;
pub use prop_editor::PropEditor;

/// culoarea de accent a temei curente (Dark / Light / Nord)
fn accent() -> Color32 {
    crate::theme::accent()
}

fn c32(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn to_screen(vp: &Mat4, rect: Rect, p: Vec3) -> Option<Pos2> {
    let c = *vp * p.extend(1.0);
    if c.w <= 1e-4 {
        return None;
    }
    let n = c.truncate() / c.w;
    Some(Pos2::new(rect.left() + (n.x * 0.5 + 0.5) * rect.width(), rect.top() + (1.0 - (n.y * 0.5 + 0.5)) * rect.height()))
}

fn label_bg(p: &egui::Painter, pos: Pos2, text: &str, col: Color32) {
    let g = p.layout_no_wrap(text.to_string(), FontId::proportional(13.0), Color32::WHITE);
    let r = Rect::from_min_size(pos + EV::new(8.0, -8.0 - g.size().y), g.size()).expand(4.0);
    p.rect_filled(r, 4.0, Color32::from_black_alpha(190));
    p.rect_stroke(r, 4.0, Stroke::new(1.0_f32, col));
    p.galley(r.min + EV::splat(4.0), g, Color32::WHITE);
}

/// ca `label_bg`, centrat orizontal pe `pos` (textul deasupra punctului)
fn label_bg_center(p: &egui::Painter, pos: Pos2, text: &str, col: Color32) {
    let w = p.layout_no_wrap(text.to_string(), FontId::proportional(13.0), Color32::WHITE).size().x;
    label_bg(p, pos - EV::new(w * 0.5 + 8.0, 0.0), text, col);
}

fn date_edit(ui: &mut egui::Ui, id: egui::Id, d: &mut i32) -> bool {
    let mut s = ui.data_mut(|m| m.get_temp::<String>(id)).unwrap_or_else(|| timeline::fmt_date(*d));
    let r = ui.add(egui::TextEdit::singleline(&mut s).id(id).desired_width(78.0));
    let mut changed = false;
    if r.has_focus() {
        if let Some(v) = timeline::parse_date(&s) {
            if v != *d {
                *d = v;
                changed = true;
            }
        }
        ui.data_mut(|m| m.insert_temp(id, s));
    } else {
        ui.data_mut(|m| m.remove::<String>(id));
    }
    changed
}

impl eframe::App for App {
    /// inchidere normala: fara oferta de recuperare la urmatoarea pornire
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.autosave.finish();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        if dt > 0.0 {
            self.fps = self.fps * 0.9 + (1.0 / dt) * 0.1;
        }
        if self.theme_dirty {
            theme::apply(ctx, self.settings.theme);
            self.theme_dirty = false;
        }
        // marimea intregii interfate (fonturi + tabele + panouri); egui o aplica la finalul cadrului
        ctx.set_zoom_factor(self.settings.ui_scale);
        // inaltimea textului (0 = implicit); temele o citesc la aplicare
        theme::set_font_base(self.settings.font_size);
        // schimbarea fontului se aplica imediat (refontuirea e scumpa, doar la schimbare)
        if self.font_family_cur != self.settings.font_family {
            self.font_family_cur = self.settings.font_family.clone();
            theme::install_fonts_with(ctx, &self.settings.font_family);
            self.theme_dirty = true;
        }
        crate::app::set_units_mm(self.settings.units_mm);
        self.poll_jobs();
        self.mcp_step(ctx);
        self.check_file_changes();
        if self.loaded_models().next().is_some() {
            // aplicatia sta in repaus fara input: o trezim periodic ca sa observe fisierele modificate
            ctx.request_repaint_after(std::time::Duration::from_secs(2));
        }
        self.update_mode();
        self.anim_tick(dt);
        self.cam_anim_tick(dt);
        self.gravity_tick(dt);
        self.report_step(ctx);
        self.render_step();
        self.pathtrace_step(ctx);
        self.update_sim(dt);

        // fisiere trase in fereastra
        let dropped: Vec<String> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.as_ref().map(|p| p.to_string_lossy().to_string())).collect());
        for p in dropped {
            self.open_path(&p, true);
        }
        self.shortcuts(ctx);

        self.settings.layout.fixup();
        if self.immersive_mode {
            self.immersive_overlay(ctx);
        } else {
            self.ribbon(ctx);
        }
        // pagina de start: doar ribbon-ul (inactiv) si ecranul de pornire pe toata fereastra
        if self.start_screen() {
            egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| self.viewport(ui, true));
            self.tools_window(ctx);
            self.windows(ctx);
            self.toasts_ui(ctx);
            if !self.toasts.is_empty() {
                ctx.request_repaint();
            }
            return;
        }
        self.status_bar(ctx);
        // panoul de jos: cel mult jumatate din ecran (65% doar cand e maximizat), vederea 3D ramane vizibila
        let maxh = (ctx.screen_rect().height() * if self.settings.layout.bottom_tall { 0.65 } else { 0.5 }).max(200.0);
        let side_max = (ctx.screen_rect().width() * 0.38).max(260.0);
        // zonele de panouri: fixate (pin) sau auto-hide (banda subtire cu file, panoul apare peste model)
        let mut strips: [Option<Rect>; 3] = [None; 3];
        if !self.settings.layout.hide_all {
            let ah = self.settings.layout.autohide;
            let pf = egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(6.0, 4.0)).rounding(crate::theme::RADIUS_MD).stroke(egui::Stroke::new(1.0_f32, theme::border()));
            if !self.settings.layout.in_area(Dock::Bottom).is_empty() {
                if ah[2] {
                    strips[2] = Some(egui::TopBottomPanel::bottom("bottom_strip").exact_height(26.0).resizable(false).show(ctx, |ui| self.dock_strip(ui, Dock::Bottom)).response.rect);
                } else {
                    // se ridica / coboara cu animatie din filele din bara de stare
                    let open = self.settings.layout.bottom_open;
                    let h0 = if self.settings.layout.bottom_tall { maxh } else { 220.0 };
                    let r = egui::TopBottomPanel::bottom("bottom").frame(pf).resizable(true).default_height(h0).min_height(60.0).max_height(maxh).show_animated(ctx, open, |ui| self.dock_area(ui, Dock::Bottom));
                    // manerul de redimensionare, vizibil pe marginea de sus (trage in sus / in jos)
                    if let Some(r) = r.filter(|_| open) {
                        let top = r.response.rect.center_top();
                        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("bottom_grip")));
                        p.rect_filled(Rect::from_center_size(top + EV::new(0.0, 2.0), EV::new(46.0, 4.0)), 2.0, accent().gamma_multiply(0.55));
                    }
                }
            }
            if !self.settings.layout.in_area(Dock::Left).is_empty() {
                if ah[0] {
                    strips[0] = Some(egui::SidePanel::left("left_strip").exact_width(30.0).resizable(false).show(ctx, |ui| self.dock_strip(ui, Dock::Left)).response.rect);
                } else {
                    egui::SidePanel::left("left").frame(pf).resizable(true).default_width(250.0).min_width(180.0).max_width(side_max).show(ctx, |ui| self.dock_area(ui, Dock::Left));
                }
            }
            if !self.settings.layout.in_area(Dock::Right).is_empty() {
                if ah[1] {
                    strips[1] = Some(egui::SidePanel::right("right_strip").exact_width(30.0).resizable(false).show(ctx, |ui| self.dock_strip(ui, Dock::Right)).response.rect);
                } else {
                    egui::SidePanel::right("right").frame(pf).resizable(true).default_width(270.0).min_width(190.0).max_width(side_max).show(ctx, |ui| self.dock_area(ui, Dock::Right));
                }
            }
        }
        if self.split_view {
            // Split-view: două jumătăți în aceeași fereastră, cu separator trasabil.
            egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();
                let half = rect.width() * self.split_ratio.clamp(0.05, 0.95);
                let left = Rect::from_min_max(rect.min, egui::pos2(rect.min.x + half, rect.max.y));
                let right = Rect::from_min_max(egui::pos2(rect.min.x + half, rect.min.y), rect.max);
                // jumătatea activă = cea sub cursor (acolo se desenează ViewCube / nav_bar o singură dată)
                let pointer = ctx.input(|i| i.pointer.hover_pos());
                let active_right = pointer.map_or(false, |p| p.x >= rect.min.x + half);
                // --- jumatatea stanga (camera A) ---
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(left), |ui| self.viewport(ui, !active_right));
                // --- separatorul trasabil (stretch) ---
                let sep = ui.painter();
                sep.rect_filled(Rect::from_center_size(egui::pos2(rect.min.x + half, rect.center().y), egui::vec2(3.0, rect.height())), 0.0, accent());
                let drag = ui.interact(Rect::from_center_size(egui::pos2(rect.min.x + half, rect.center().y), egui::vec2(10.0, rect.height())), egui::Id::new("split_sep"), egui::Sense::drag());
                if drag.dragged() {
                    self.split_ratio = ((drag.interact_pointer_pos().map(|p| p.x).unwrap_or(rect.min.x + half) - rect.min.x) / rect.width()).clamp(0.05, 0.95);
                }
                // --- jumatatea dreapta (camera B), prin swap temporar ---
                std::mem::swap(&mut self.cam, &mut self.cam_b);
                std::mem::swap(&mut self.section, &mut self.section_b);
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(right), |ui| self.viewport(ui, active_right));
                std::mem::swap(&mut self.cam, &mut self.cam_b);
                std::mem::swap(&mut self.section, &mut self.section_b);
                // evidențiază jumătatea activă cu un contur discret
                let ar = if active_right { right } else { left };
                ui.painter().rect_stroke(ar.shrink(1.0), crate::theme::RADIUS_MD, egui::Stroke::new(1.0_f32, accent().gamma_multiply(0.5)));
                // Link-ul sincronizează camerele DOAR la activare (în ribbon), nu în fiecare frame —
                // altfel camera B n-ar putea fi mișcată independent pentru comparație.
                // etichete + buton de inchidere pe jumatatea dreapta
                ui.painter().text(left.center_top() + egui::vec2(0.0, 12.0), Align2::CENTER_TOP, "A", FontId::proportional(11.0), accent());
                ui.painter().text(right.center_top() + egui::vec2(0.0, 12.0), Align2::CENTER_TOP, "B", FontId::proportional(11.0), accent());
            });
        } else {
            egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| self.viewport(ui, true));
        }
        // Bara de navigare e un Area global: o desenăm O SINGURĂ DATĂ, centrată pe toată zona
        // de model, și o facem să opereze pe jumătatea activă (A sau B) în split-view.
        if self.settings.nav_bar && self.loaded_models().next().is_some() && self.render_job.is_none() {
            let full = ctx.available_rect();
            if self.split_view {
                let active_b = ctx.input(|i| i.pointer.hover_pos()).map_or(false, |p| p.x >= full.min.x + full.width() * self.split_ratio.clamp(0.05, 0.95));
                if active_b {
                    std::mem::swap(&mut self.cam, &mut self.cam_b);
                    std::mem::swap(&mut self.section, &mut self.section_b);
                    self.nav_bar(ctx, full);
                    std::mem::swap(&mut self.cam, &mut self.cam_b);
                    std::mem::swap(&mut self.section, &mut self.section_b);
                } else {
                    self.nav_bar(ctx, full);
                }
            } else {
                self.nav_bar(ctx, full);
            }
        }
        self.flyout_ui(ctx, strips);
        // mers: tastele (shortcuts) si mouse-ul (viewport) au setat viteza tinta - aplicam lin
        self.walk_tick(dt);
        // Live Link: selectia sincronizata cu Revit / Navisworks (sondare usoara, de 4 ori pe secunda)
        if self.settings.sync_selection {
            self.sync_tick();
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
        self.section_follow_tick();
        // istoricul vizualizarii (Undo / Redo)
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        self.history_tick(pointer_down);
        self.float_windows(ctx);
        self.windows(ctx);
        self.pathtrace_window(ctx);
        self.script_window(ctx);
        self.settings_window(ctx);
        self.clash_focus_window(ctx);
        self.tools_window(ctx);
        self.sync_ui_prefs();
        if self.settings.layout != self.layout_saved {
            self.layout_saved = self.settings.layout.clone();
            self.settings.save();
        }
        // seturile s-au schimbat -> reevalueaza materialele atribuite pe seturi
        let sig = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_string(&self.sets).unwrap_or_default().hash(&mut h);
            h.finish()
        };
        if sig != self.sets_sig {
            self.sets_sig = sig;
            if self.mat_assign.iter().any(|a| matches!(a.target, crate::materials::MatTarget::Set(_))) {
                self.materials_changed();
            }
        }
        self.toasts_ui(ctx);
        self.update_mode();
        self.rebuild_colors();
        if self.still_wait > 0 {
            self.still_wait -= 1;
            ctx.request_repaint();
        }
        self.autosave_tick();
        self.shared_sync(false);

        let walking = !ctx.wants_keyboard_input() && ctx.input(|i| [Key::W, Key::A, Key::S, Key::D, Key::Q, Key::E, Key::ArrowUp, Key::ArrowDown, Key::ArrowLeft, Key::ArrowRight].iter().any(|k| i.key_down(*k)));
        if !self.loads.is_empty() || self.clash_job.is_some() || (self.sim_play && self.sim_view()) || walking || self.anim_play.is_some() || self.render_job.is_some() || self.pending_capture.is_some() || !self.toasts.is_empty() || self.cam_anim.is_some() || self.report_job.is_some() || (self.settings.gravity && self.nav == NavMode::Walk) || self.walk_active() {
            ctx.request_repaint();
        }
    }
}

impl App {
    pub fn pal(&self) -> Palette {
        theme::palette(self.settings.theme)
    }

    fn set_theme(&mut self, t: Theme) {
        if self.settings.theme != t {
            self.settings.theme = t;
            self.settings.save();
            self.theme_dirty = true;
        }
    }

    // ================================================================== shortcuts
    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.wants_keyboard_input();
        // F11: toate panourile ascunse / inapoi (tot ecranul pentru model)
        if ctx.input(|i| i.key_pressed(Key::F11)) {
            self.settings.layout.hide_all = !self.settings.layout.hide_all;
            self.flyout = None;
        }
        // PgUp / PgDn: nivelul de deasupra / dedesubt, cand o sectiune pe nivel e activa
        if !typing && self.section.enabled && self.section_level.is_some() {
            if ctx.input(|i| i.key_pressed(Key::PageUp)) {
                self.section_level_step(1);
            }
            if ctx.input(|i| i.key_pressed(Key::PageDown)) {
                self.section_level_step(-1);
            }
        }
        let kb = &self.settings.keybinds;
        let (ctrl, a, f, h, i, esc, del) = ctx.input(|k| {
            (k.modifiers.command, k.key_pressed(Key::A), k.key_pressed(Key::F), k.key_pressed(Key::H), k.key_pressed(Key::I), k.key_pressed(Key::Escape), k.key_pressed(Key::Delete))
        });
        let s = ctx.input(|k| k.key_pressed(Key::S));
        let (bind_open, bind_find, bind_palette) = (
            crate::keybind::binding(kb, "open"),
            crate::keybind::binding(kb, "find"),
            crate::keybind::binding(kb, "palette"),
        );
        if crate::keybind::pressed(ctx, &bind_open) {
            self.request_path(Purpose::OpenModel { append: false });
        }
        if crate::keybind::pressed(ctx, &bind_find) {
            self.open_panel(Panel::Find);
        }
        // paleta de comenzi (merge si cand scrii intr-un camp)
        if crate::keybind::pressed(ctx, &bind_palette) {
            self.palette_open = !self.palette_open;
            self.palette_q.clear();
            self.palette_sel = 0;
        }
        // F1 = scurtaturile de tastatura
        if ctx.input(|i| i.key_pressed(Key::F1)) {
            self.keys_open = !self.keys_open;
        }
        // F5 = Refresh modelele modificate, Ctrl+F5 = reincarca tot
        if ctx.input(|i| i.key_pressed(Key::F5)) {
            self.refresh_models(ctrl);
        }
        if ctrl && s {
            match self.project_path.clone() {
                Some(p) => self.save_project(&p),
                None => self.request_path(Purpose::SaveProject),
            }
        }
        // Ctrl+1..9: salt direct la View Template-ul cu acel număr (ca Navisworks Saved Views)
        if ctrl && !typing {
            for i in 0..self.view_templates.len().min(9) {
                let key = match i { 0 => Key::Num1, 1 => Key::Num2, 2 => Key::Num3, 3 => Key::Num4, 4 => Key::Num5, 5 => Key::Num6, 6 => Key::Num7, 7 => Key::Num8, _ => Key::Num9 };
                if ctx.input(|k| k.key_pressed(key)) {
                    self.apply_view_template(i);
                    break;
                }
            }
        }
        if typing {
            return;
        }
        // Undo / Redo al vizualizarii: selectie, ascunse, sectiune, camera
        let (kz, ky) = ctx.input(|i| (i.key_pressed(Key::Z), i.key_pressed(Key::Y)));
        if ctrl && kz {
            if ctx.input(|i| i.modifiers.shift) {
                self.redo_view();
            } else {
                self.undo_view();
            }
        }
        if ctrl && ky {
            self.redo_view();
        }
        if ctrl && a {
            if ctx.input(|i| i.modifiers.shift) {
                self.select_similar(true);
            } else {
                let all = self.all_ids();
                self.apply(Action::Select(all, false));
            }
        }
        if !ctrl && f {
            let sel: Vec<ElemId> = self.selected.iter().copied().collect();
            self.apply(Action::Focus(sel));
        }
        if !ctrl && h && !self.selected.is_empty() {
            let sel: Vec<ElemId> = self.selected.iter().copied().collect();
            self.apply(Action::Hide(sel));
        }
        if !ctrl && i && !self.selected.is_empty() {
            let sel: Vec<ElemId> = self.selected.iter().copied().collect();
            self.apply(Action::Isolate(sel));
        }
        if del {
            self.cur_measure.clear();
        }
        if self.tool == Tool::WalkPath {
            // Backspace / Delete: sterge ultimul punct; Enter: gata
            if ctx.input(|i| i.key_pressed(Key::Backspace) || i.key_pressed(Key::Delete)) {
                self.walk_path.pop();
            }
            if ctx.input(|i| i.key_pressed(Key::Enter)) {
                self.tool = Tool::Select;
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Space)) && (self.path_run.is_some() || (self.nav == NavMode::Walk && self.walk_path.len() >= 2)) {
            self.walk_path_play();
        }
        if ctx.input(|i| i.key_pressed(Key::Home)) {
            self.go_home();
        }
        if esc {
            if self.path_run.is_some() {
                self.walk_path_stop();
            } else if !self.cur_measure.is_empty() {
                self.finish_measure();
            } else if self.tool != Tool::Select {
                self.tool = Tool::Select;
            } else {
                self.clear_selection();
            }
        }
        // mers / zbor
        let (w, s2, a2, d, q, e, up, dn, lf, rt, shift) = ctx.input(|i| {
            (
                i.key_down(Key::W),
                i.key_down(Key::S),
                i.key_down(Key::A),
                i.key_down(Key::D),
                i.key_down(Key::Q),
                i.key_down(Key::E),
                i.key_down(Key::ArrowUp),
                i.key_down(Key::ArrowDown),
                i.key_down(Key::ArrowLeft),
                i.key_down(Key::ArrowRight),
                i.modifiers.shift,
            )
        });
        let mut v = Vec3::ZERO;
        if w || up {
            v.y += 1.0;
        }
        if s2 || dn {
            v.y -= 1.0;
        }
        if d || rt {
            v.x += 1.0;
        }
        if a2 || lf {
            v.x -= 1.0;
        }
        if e {
            v.z += 1.0;
        }
        if q {
            v.z -= 1.0;
        }
        if self.nav == NavMode::Walk && !ctrl {
            // in modul Walk sagetile stanga / dreapta rotesc (ca in Navisworks)
            if lf && !a2 {
                v.x += 1.0;
                self.walk_turn_target += 1.0;
            }
            if rt && !d {
                v.x -= 1.0;
                self.walk_turn_target -= 1.0;
            }
        }
        if v != Vec3::ZERO && !ctrl {
            // viteza tinta; walk_tick accelereaza / franeaza lin (Shift = de 2.5 ori mai repede)
            if self.path_run.is_some() {
                self.path_run = None;
            }
            let v = Vec3::new(v.x, v.y, v.z * 0.6);
            self.walk_target += v.normalize_or_zero() * if shift { 2.5 } else { 1.0 };
        }
    }

    fn finish_measure(&mut self) {
        if let Tool::Measure(k) = self.tool {
            let min = match k {
                MeasureKind::Area | MeasureKind::Angle => 3,
                _ => 2,
            };
            if self.cur_measure.len() >= min {
                self.measures.push(Measure { kind: k, pts: self.cur_measure.iter().map(|p| p.to_array()).collect() });
            }
        }
        self.cur_measure.clear();
    }

    // ================================================================== meniu
    // ================================================================== ribbon
    fn save_now(&mut self) {
        // proiect deschis dintr-un .bimd: se publica din nou acolo (ca NWD)
        if let Some(p) = self.bimd_path.clone() {
            match self.publish_bimd(&p) {
                Ok(m) => self.toast(m),
                Err(e) => self.error(e),
            }
            return;
        }
        match self.project_path.clone() {
            Some(p) => self.save_project(&p),
            None => self.request_path(Purpose::SaveProject),
        }
    }

    fn file_menu(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(250.0);
        if ui.button(crate::i18n::tr("📂 Open Model…  (Ctrl+O)")).clicked() {
            ui.close_menu();
            self.request_path(Purpose::OpenModel { append: false });
        }
        if ui.button(crate::i18n::tr("➕ Append Model…")).clicked() {
            ui.close_menu();
            self.request_path(Purpose::OpenModel { append: true });
        }
        if ui.button(crate::i18n::tr("⟳ Refresh Changed Models  (F5)")).on_hover_text("Reload the models whose files were modified on disk").clicked() {
            ui.close_menu();
            self.refresh_models(false);
        }
        if ui.button(crate::i18n::tr("⟳ Reload All Models  (Ctrl+F5)")).clicked() {
            ui.close_menu();
            self.refresh_models(true);
        }
        if ui.button(crate::i18n::tr("⚙ Settings…")).on_hover_text("Appearance, fonts / DPI, DWG export, navigation, files").clicked() {
            ui.close_menu();
            self.settings_open = true;
        }
        ui.separator();
        if ui.button(crate::i18n::tr("📁 Open Project (.bimr)…")).clicked() {
            ui.close_menu();
            self.request_path(Purpose::OpenProject);
        }
        if ui.button(crate::i18n::tr("💾 Save Project  (Ctrl+S)")).clicked() {
            ui.close_menu();
            self.save_now();
        }
        if ui.button(crate::i18n::tr("Save Project As…")).clicked() {
            ui.close_menu();
            self.request_path(Purpose::SaveProject);
        }
        ui.separator();
        ui.menu_button(crate::i18n::tr("Recent"), |ui| {
            let recent = self.settings.recent.clone();
            if recent.is_empty() {
                ui.label(RichText::new("(empty)").weak());
            }
            for p in recent {
                let exists = std::path::Path::new(&p).exists();
                if ui.add_enabled(exists, egui::Button::new(crate::file_name(&p))).on_hover_text(&p).clicked() {
                    ui.close_menu();
                    self.open_path(&p, !p.to_lowercase().ends_with(".bimr") && self.loaded_models().next().is_some());
                }
            }
            ui.separator();
            if ui.button(crate::i18n::tr("Clear list")).clicked() {
                self.settings.recent.clear();
                self.settings.save();
                ui.close_menu();
            }
        });
        ui.separator();
        if ui.button(crate::i18n::tr("📦 Export Model Cache (.bimc)")).on_hover_text("Native compact format with all geometry and properties (like NWC), written next to the sources.").clicked() {
            ui.close_menu();
            self.export_all_caches();
        }
        ui.separator();
        if ui.button(crate::i18n::tr("New Project (close all)")).clicked() {
            ui.close_menu();
            self.new_project();
        }
        if ui.button(crate::i18n::tr("📦 Publish .bimd…  (project + models in one file)")).on_hover_text("Like Navisworks NWD: one file with the project and all models - opens on any PC without the source files").clicked() {
            ui.close_menu();
            self.request_path(Purpose::PublishBimd);
        }
        if ui.button(crate::i18n::tr("🌐 Română / English")).on_hover_text(crate::i18n::tr("Interface language: English / Română")).clicked() {
            self.toggle_language();
            ui.close_menu();
        }
        if ui.button(crate::i18n::tr("🏠 Start Page")).on_hover_text("Recent files, Open, New - the project stays open until you open something else").clicked() {
            ui.close_menu();
            self.show_start = true;
        }
        if ui.button(crate::i18n::tr("Exit")).clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Bara de sus, deasupra ribbonului: cautare, Model focus, tema, Setari si
    /// numele proiectului. E mutata aici ca sa nu impinga / suprapuna taburile.
    fn app_bar(&mut self, ctx: &egui::Context) {
        use crate::theme::RibbonMode;
        let mode = self.settings.ribbon_mode;
        egui::TopBottomPanel::top("appbar")
            .show_separator_line(false)
            .frame(egui::Frame::none().fill(ctx.style().visuals.panel_fill).inner_margin(egui::Margin::symmetric(8.0, 3.0)))
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let next = mode.next();
                    let ic = match mode {
                        RibbonMode::Full => "⏶",
                        RibbonMode::Compact => "⏶",
                        RibbonMode::Mini => "⏷",
                    };
                    if ui.add(egui::Button::new(RichText::new(ic).size(13.0)).frame(false)).on_hover_text(format!("Ribbon: {} - click for {} (Full = icons + labels, Compact = icons only, Mini = tabs only)", mode.label(), next.label())).clicked() {
                        self.settings.ribbon_mode = next;
                        self.settings.save();
                    }
                    if ui.add(egui::Button::new(RichText::new("⛶ Model focus").size(11.5).color(ui.visuals().text_color())).fill(ui.visuals().window_fill).stroke(Stroke::NONE).rounding(7.0).min_size(EV::new(108.0, 22.0))).on_hover_text("Hide the ribbon and panels, keep Properties floating - maximum space for the model (Esc to exit)").clicked() {
                        self.toggle_immersive();
                    }
                    let nt = self.settings.theme.next();
                    let chip = egui::Button::new(RichText::new(self.settings.theme.label()).size(11.5).color(ui.visuals().text_color())).fill(ui.visuals().window_fill).stroke(Stroke::NONE).rounding(7.0).min_size(EV::new(78.0, 22.0));
                    if ui.add(chip).on_hover_text(format!("Theme: {} - click for {}", self.settings.theme.label(), nt.label())).clicked() {
                        self.set_theme(nt);
                    }
                    if ui.add(egui::Button::new("⚙ Settings").frame(false)).on_hover_text("Application settings: fonts / DPI, export, navigation, files").clicked() {
                        self.settings_open = true;
                    }
                    let sb = egui::Button::new(crate::theme::icon_job("🔍", "Search or Ctrl+K", 12.0, ui.visuals().weak_text_color())).fill(ui.visuals().extreme_bg_color).stroke(Stroke::NONE).rounding(7.0).min_size(EV::new(190.0, 22.0));
                    if ui.add(sb).on_hover_text("Command palette: type a command or an element name").clicked() {
                        self.palette_open = true;
                    }
                    ui.label(RichText::new("ManageBIM").strong().color(accent()));
                    if let Some(p) = &self.project_path {
                        ui.add_space(4.0);
                        ui.label(RichText::new(crate::file_name(p)).weak());
                    }
                });
            });
    }

    fn ribbon(&mut self, ctx: &egui::Context) {
        use crate::theme::RibbonMode;
        const TABS: [&str; 14] = ["Home", "Select", "Viewpoint", "Review", "Sections", "Annotation", "BIM", "Coordination", "Analyze", "Render", "Export", "Share", "Script", "View"];
        // setarile vechi: "ribbon restrans" = treapta Mini
        if self.settings.ribbon_min {
            self.settings.ribbon_min = false;
            self.settings.ribbon_mode = RibbonMode::Mini;
        }
        if self.ribbon_tab >= TABS.len() {
            self.ribbon_tab = 0;
        }
        let mode = self.settings.ribbon_mode;
        crate::theme::RIBBON_COMPACT.store(mode == RibbonMode::Compact, std::sync::atomic::Ordering::Relaxed);
        crate::theme::COLOR_ICONS.store(self.settings.color_icons, std::sync::atomic::Ordering::Relaxed);
        let fill = ctx.style().visuals.panel_fill;
        // bara de sus: cautare / focus / tema / setari / proiect - separata de ribbon
        self.app_bar(ctx);
        egui::TopBottomPanel::top("ribbon").show_separator_line(false).frame(egui::Frame::none().fill(fill).inner_margin(egui::Margin { left: 8.0, right: 8.0, top: 5.0, bottom: 5.0 })).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.style_mut().spacing.item_spacing.x = 3.0;
                // accentul Nord e deschis (cyan): text inchis pentru contrast
                let ftxt = match self.settings.theme { Theme::Nord => Color32::from_rgb(46, 52, 64), Theme::TokyoNight => Color32::from_rgb(26, 27, 38), _ => Color32::WHITE };
                let fb = egui::Button::new(crate::theme::icon_job("≡", crate::i18n::tr("File "), 14.0, ftxt)).fill(accent()).rounding(crate::theme::RADIUS_MD).min_size(EV::new(78.0, 26.0));
                let r = ui.add(fb);
                let popup = ui.make_persistent_id("file_menu");
                if r.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(popup));
                }
                egui::popup_below_widget(ui, popup, &r, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| self.file_menu(ui));
                ui.add_space(8.0);
                // Tab-urile au acum toata latimea: bara de sus e separata.
                // Scroll-ul ramane doar pentru ferestre foarte inguste.
                // fereastra ingusta: tab-urile se strang (latime minima mai mica, text putin mai mic) ca sa
                // se vada toate 14, fara derulare
                let tabs_w = ui.available_width();
                let (tab_min, tab_font, tab_pad) = if tabs_w >= 14.0 * 81.0 {
                    (78.0, None, 6.0)
                } else if tabs_w >= 14.0 * 66.0 {
                    (0.0, None, 5.0)
                } else {
                    (0.0, Some(12.0), 3.0)
                };
                egui::ScrollArea::horizontal().id_salt("ribbon_tabs").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.style_mut().spacing.item_spacing.x = if tab_min > 0.0 { 3.0 } else { 1.0 };
                        ui.style_mut().spacing.button_padding.x = tab_pad;
                        for (i, t) in TABS.iter().enumerate() {
                    let on = self.ribbon_tab == i && mode != RibbonMode::Mini;
                    let t = crate::i18n::tr(t);
                    let mut txt = if on { RichText::new(t).color(accent()).strong() } else { RichText::new(t) };
                    if let Some(sz) = tab_font {
                        txt = txt.size(sz);
                    }
                    let b = egui::Button::new(txt).fill(if on { accent().gamma_multiply(0.16) } else { Color32::TRANSPARENT }).stroke(Stroke::NONE).rounding(crate::theme::RADIUS_LG).min_size(EV::new(tab_min, 24.0));
                    let r = ui.add(b);
                    if on {
                        // subliniere subtila a tab-ului activ (ca in Office / Navisworks 2024)
                        let rr = r.rect;
                        ui.painter().line_segment(
                            [Pos2::new(rr.min.x + 7.0, rr.max.y + 2.0), Pos2::new(rr.max.x - 7.0, rr.max.y + 2.0)],
                            Stroke::new(2.0_f32, accent()),
                        );
                    }
                    if r.double_clicked() {
                        // ca in Office: dublu-click pe tab = Mini / inapoi
                        self.settings.ribbon_mode = if mode == RibbonMode::Mini { RibbonMode::Full } else { RibbonMode::Mini };
                        self.settings.save();
                    } else if r.clicked() {
                        if mode == RibbonMode::Mini {
                            self.settings.ribbon_mode = RibbonMode::Full;
                            self.settings.save();
                        }
                        self.ribbon_tab = i;
                    }
                            }
                        });
                    });
                });
            if mode == RibbonMode::Mini {
                return;
            }
            ui.add_space(4.0);
            // inaltime FIXA pe toate tab-urile (nu mai "sare" cand schimbi tab-ul)
            let h = mode.height();
            // ribbon-ul se potriveste in fereastra: latimea butoanelor (62 px) scade automat pe tab-ul
            // curent cat sa incapa tot (minim 38 px), si creste la loc cand e spatiu - fara bara de scroll
            let avail_w = ui.available_width();
            let wid = egui::Id::new(("ribbon_btn_w", self.ribbon_tab));
            let w_now: f32 = ui.data(|d| d.get_temp(wid)).unwrap_or(62.0);
            crate::theme::set_ribbon_btn_w(w_now);
            // daca nici cu butoanele cele mai inguste nu incape: tab-ul trece pe iconite (numele in tooltip)
            // pana cand fereastra e iar destul de lata (latimea necesara e tinuta minte)
            let cid = egui::Id::new(("ribbon_auto_compact", self.ribbon_tab));
            let auto_compact: Option<f32> = ui.data(|d| d.get_temp(cid));
            if let Some(need) = auto_compact {
                if avail_w > need + 10.0 {
                    ui.data_mut(|d| d.remove::<f32>(cid));
                    ui.ctx().request_repaint();
                } else if mode != RibbonMode::Compact {
                    crate::theme::RIBBON_COMPACT.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            let forced_compact = auto_compact.is_some() && mode != RibbonMode::Compact;
            let out = egui::ScrollArea::horizontal().id_salt("ribbon_scroll").max_height(h).scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(h);
                    ui.set_max_height(h);
                    // golul dintre carduri separa grupele - nu linii
                    ui.spacing_mut().item_spacing = egui::vec2(5.0, 1.0);
                    ui.spacing_mut().interact_size.y = 16.0;
                    ui.spacing_mut().button_padding = egui::vec2(6.0, 1.0);
                    // tranzitie la schimbarea tab-ului: continutul apare cu fade
                    let now = ui.input(|i| i.time);
                    let fid = egui::Id::new("ribbon_fade");
                    let (ft, fs): (usize, f64) = ui.data(|d| d.get_temp(fid)).unwrap_or((usize::MAX, 0.0));
                    let start = if ft != self.ribbon_tab { ui.data_mut(|d| d.insert_temp(fid, (self.ribbon_tab, now))); now } else { fs };
                    let alpha = ((now - start) / 0.16).clamp(0.0, 1.0) as f32;
                    if alpha < 1.0 {
                        ui.multiply_opacity(0.25 + 0.75 * alpha);
                        ui.ctx().request_repaint();
                    }
                    // pagina de start: ribbon-ul se vede, dar e inactiv pana se deschide un model / proiect
                    if self.start_screen() {
                        ui.disable();
                    }
                    match self.ribbon_tab {
                        0 => {
                            self.ribbon_home(ui);
                            self.ribbon_select(ui);
                            self.ribbon_display(ui);
                        }
                        1 => {
                            self.ribbon_select(ui);
                            let has = !self.selected.is_empty();
                            rgroup(ui, "Select Similar", |ui| {
                                if rbtn(ui, "⧉", "Same Type", "Every visible item of the same type as the selection (Revit Family and Type / IFC type)  Ctrl+Shift+A", false, has).clicked() {
                                    self.select_similar(true);
                                }
                                if rbtn(ui, "▦", "Same Category", "Every visible item of the same category / IFC class (all walls, all doors...)", false, has).clicked() {
                                    self.select_similar(false);
                                }
                            });
                            rgroup(ui, "Live Link", |ui| {
                                let on = self.settings.sync_selection;
                                if rbtn(ui, "🔗", "Sync Selection", "Selection follows Revit and Navisworks, both ways:\nselect here and the elements are selected in Revit / Navisworks, and the other way round.\nIn Revit: ManageBIM › Sync Selection. In Navisworks: Tool Add-ins › Sync Selection (once per session).\nThe models must be the same (exported from Revit / IFC from Revit / NWC).", on, true).clicked() {
                                    self.set_sync(!on);
                                }
                                let follow = self.settings.sync_follow;
                                if rbtn(ui, "🎯", "Zoom to It", "Zoom to the elements selected in Revit / Navisworks", follow, on).clicked() {
                                    self.settings.sync_follow = !follow;
                                    self.settings.save();
                                }
                            });
                            self.ribbon_display(ui);
                        }
                        2 => self.ribbon_viewpoint(ui),
                        3 => self.ribbon_review(ui),
                        4 => {
                            self.ribbon_sections(ui);
                            self.ribbon_grids(ui);
                        }
                        5 => self.ribbon_annotation(ui),
                        6 => self.ribbon_bim(ui),
                        7 => self.ribbon_coord(ui),
                        8 => {
                            self.ribbon_models(ui);
                            self.ribbon_relocate(ui);
                        }
                        9 => self.ribbon_render(ui),
                        10 => {
                            self.ribbon_output(ui);
                            self.ribbon_output2(ui);
                        }
                        11 => {
                            self.ribbon_share(ui);
                        }
                        12 => self.ribbon_script(ui),
                        _ => {
                            self.ribbon_view(ui);
                            self.ribbon_view2(ui);
                            self.ribbon_help(ui);
                        }
                    }
                });
            });
            // potrivirea latimii pentru cadrul urmator (doar in modul cu etichete)
            if mode != RibbonMode::Compact && !forced_compact && w_now <= 38.5 && out.content_size.x > avail_w + 1.0 {
                ui.data_mut(|d| d.insert_temp(cid, out.content_size.x));
                ui.ctx().request_repaint();
            }
            crate::theme::RIBBON_COMPACT.store(mode == RibbonMode::Compact, std::sync::atomic::Ordering::Relaxed);
            if mode != RibbonMode::Compact && !forced_compact {
                let content = out.content_size.x.max(1.0);
                let target = if content > avail_w + 1.0 {
                    (w_now * (avail_w - 4.0) / content).floor().max(38.0)
                } else if content < avail_w - 40.0 && w_now < 62.0 {
                    (w_now + 2.0).min(62.0)
                } else {
                    w_now
                };
                if (target - w_now).abs() >= 1.0 {
                    ui.data_mut(|d| d.insert_temp(wid, target));
                    ui.ctx().request_repaint();
                }
            }
            crate::theme::set_ribbon_btn_w(62.0);
        });
    }

    fn panel_btn(&mut self, ui: &mut egui::Ui, p: Panel, icon: &str, label: &str, tip: &str) {
        if rbtn(ui, icon, label, tip, self.panel_shown(p), true).clicked() {
            self.settings.layout.toggle(p);
        }
    }

    fn ribbon_home(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Project", |ui| {
            if rbtn(ui, "📂", "Open", "Open a model (replaces the scene)  Ctrl+O", false, true).clicked() {
                self.request_path(Purpose::OpenModel { append: false });
            }
            if rbtn(ui, "➕", "Append", "Append a model to the federated scene", false, true).clicked() {
                self.request_path(Purpose::OpenModel { append: true });
            }
            if rbtn(ui, "💾", "Save", "Save the project  Ctrl+S (.bimr, or the .bimd it was opened from)", false, true).clicked() {
                self.save_now();
            }
            if rbtn(ui, "📦", "Publish", "Publish .bimd: the project and all models in one file (like Navisworks NWD)", false, self.loaded_models().next().is_some()).clicked() {
                self.request_path(Purpose::PublishBimd);
            }
            if rbtn(ui, "🏠", "Start", "Start page: recent files, Open, New (the project stays open - come back with the arrow)", false, true).clicked() {
                self.show_start = true;
            }
            // Refresh: evidentiat cand unul din fisierele incarcate s-a modificat pe disc
            let has_models = self.loaded_models().next().is_some();
            let n_changed = self.changed_files.len();
            let label = if n_changed > 0 { format!("Refresh ({n_changed})") } else { "Refresh".to_string() };
            let tip = if n_changed > 0 {
                let names: Vec<String> = self.changed_files.iter().filter_map(|i| self.models.get(*i).cloned().flatten()).map(|m| m.name.clone()).collect();
                format!("Modified on disk: {}\nClick (F5) to reload them. Shift+click (Ctrl+F5) reloads all models.", names.join(", "))
            } else {
                "Reload the models whose files changed on disk  F5\nShift+click reloads all models  Ctrl+F5".to_string()
            };
            let r = rbtn(ui, "⟳", &label, &tip, n_changed > 0, has_models && self.loads.is_empty());
            if r.clicked() {
                let force = ui.input(|i| i.modifiers.shift);
                self.refresh_models(force);
            }
            self.panel_btn(ui, Panel::Project, "📁", "Models", "Project & Model Properties (location, files, history)");
            if rbtn(ui, "📊", "Dashboard", "Project summary: models, clashes, issues, checks, levels - export as a PDF or Word report", self.dash_open, true).clicked() {
                self.dash_open = !self.dash_open;
                self.dash = None;
            }
        });
    }

    fn ribbon_select(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Select & Search", |ui| {
            if rbtn(ui, "👆", "Select", "Selection tool (Esc)", self.tool == Tool::Select, true).clicked() {
                self.cur_measure.clear();
                self.tool = Tool::Select;
            }
            if rbtn(ui, "⬚", "Window", "Window selection: drag left -> right = items fully inside, right -> left = items touching (Ctrl adds). Middle / Shift-drag still pans", self.tool == Tool::Window, true).clicked() {
                self.cur_measure.clear();
                self.tool = if self.tool == Tool::Window { Tool::Select } else { Tool::Window };
            }
            if rbtn(ui, "⬚", "Select All", "Select every visible element  Ctrl+A", false, true).clicked() {
                let all = self.all_ids();
                self.apply(Action::Select(all, false));
            }
            let has = !self.selected.is_empty();
            if rbtn(ui, "🔲", "Clear", "Clear the selection", false, has).clicked() {
                self.clear_selection();
            }
            if rbtn(ui, "⧉", "Same Type", "Select Similar: every visible item of the same type as the selection (e.g. Basic Wall: 200mm), in all models  Ctrl+Shift+A\nRight-click › Select Same Category for the whole category", false, has).clicked() {
                self.select_similar(true);
            }
            self.panel_btn(ui, Panel::Find, "🔍", "Find Items", "Search by any property, save as search set  Ctrl+F");
            self.panel_btn(ui, Panel::Sets, "📋", "Sets", "Selection & search sets");
        });
        self.visibility_group(ui);
    }

    fn ribbon_display(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Display", |ui| {
            self.panel_btn(ui, Panel::Tree, "🌲", "Tree", "Selection tree");
            self.panel_btn(ui, Panel::PropTree, "🔎", "Prop Tree", "Browse all properties of all models");
            self.panel_btn(ui, Panel::Properties, "ℹ", "Properties", "Properties of the selected element");
            self.panel_btn(ui, Panel::Profiler, "🎨", "Profiler", "Appearance Profiler: colour the model by any property");
        });
    }

    // ================================================================== Compare

    fn visibility_group(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Visibility", |ui| {
            let sel: Vec<ElemId> = self.selected.iter().copied().collect();
            if rbtn(ui, "🚫", "Hide", "Hide selection (H)", false, !sel.is_empty()).clicked() {
                self.apply(Action::Hide(sel.clone()));
            }
            if rbtn(ui, "◎", "Hide Unsel.", "Hide everything not selected (I)", false, !sel.is_empty()).clicked() {
                self.apply(Action::Isolate(sel.clone()));
            }
            if rbtn(ui, "👁", "Unhide All", "Show all hidden elements", false, true).clicked() {
                self.apply(Action::ShowAll);
                self.clash_focus = None;
            }
        });
    }

    fn ribbon_viewpoint(&mut self, ui: &mut egui::Ui) {
        let bb = self.scene_aabb();
        rgroup(ui, "Navigation Mode", |ui| {
            if rbtn(ui, "🔄", "Orbit", "Orbit: left-drag rotates around the point under the cursor", self.nav == NavMode::Orbit, true).clicked() {
                self.nav = NavMode::Orbit;
            }
            if rbtn(ui, "✋", "Pan", "Pan: left-drag moves the model sideways", self.nav == NavMode::Pan, true).clicked() {
                self.nav = NavMode::Pan;
            }
            if rbtn(ui, "🚶", "Walk", "Walk: drag up / down, W A S D / arrows", self.nav == NavMode::Walk, true).clicked() {
                self.nav = NavMode::Walk;
            }
            // Toate controalele sunt butoane de dimensiune fixa; setarile se schimba in popup.
            let spd = rbtn(ui, "⏩", &format!("{:.1}", self.settings.walk_speed), "Walk speed (m/s)", false, true);
            let sid = ui.make_persistent_id("walk_speed_pop");
            if spd.clicked() {
                ui.memory_mut(|m| m.toggle_popup(sid));
            }
            egui::popup_below_widget(ui, sid, &spd, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(220.0);
                let mut v = self.settings.walk_speed;
                ui.label("Walk speed");
                if ui.add(egui::Slider::new(&mut v, 0.2..=20.0).logarithmic(true).suffix(" m/s").max_decimals(1)).changed() {
                    self.settings.walk_speed = v;
                }
                ui.horizontal(|ui| {
                    for (n, x) in [("Slow", 0.7), ("Normal", 1.4), ("Fast", 4.0)] {
                        if ui.button(n).clicked() {
                            self.settings.walk_speed = x;
                        }
                    }
                });
                if (self.settings.walk_speed - self.walk_saved).abs() > 1e-4 {
                    self.walk_saved = self.settings.walk_speed;
                    self.settings.save();
                }
            });
            if rbtn(ui, "⬇", "Gravity", "Walk on floors and stairs at eye height", self.settings.gravity, true).clicked() {
                self.settings.gravity = !self.settings.gravity;
                if self.settings.gravity { self.nav = NavMode::Walk; }
                self.settings.save();
            }
            if rbtn(ui, "▣", "Collision", "Stop at walls and objects", self.settings.collision, true).clicked() {
                self.settings.collision = !self.settings.collision;
                if self.settings.collision { self.nav = NavMode::Walk; }
                self.settings.save();
            }
            let eye = rbtn(ui, "👁", &format!("{:.2}", self.settings.eye_height), "Eye height (m)", false, true);
            let eid = ui.make_persistent_id("eye_pop");
            if eye.clicked() {
                ui.memory_mut(|m| m.toggle_popup(eid));
            }
            egui::popup_below_widget(ui, eid, &eye, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(180.0);
                let mut h = self.settings.eye_height;
                if ui.add(egui::Slider::new(&mut h, 0.5..=3.0).suffix(" m")).changed() {
                    self.settings.eye_height = h;
                    self.settings.save();
                }
            });
        });
        rgroup(ui, "Walk Path", |ui| {
            let drawing = self.tool == Tool::WalkPath;
            if rbtn(ui, "✏", "Draw", "Draw a walk path: click points on the floor, double-click / Enter to finish, Backspace to undo a point", drawing, self.loaded_models().next().is_some()).clicked() {
                self.path_run = None;
                self.tool = if drawing { Tool::Select } else { Tool::WalkPath };
                if !drawing {
                    self.nav = NavMode::Walk;
                }
            }
            let ok = self.walk_path.len() >= 2;
            let playing = self.path_run.as_ref().map(|r| !r.paused).unwrap_or(false);
            if rbtn(ui, if playing { "⏸" } else { "▶" }, if playing { "Pause" } else { "Play" }, "Walk along the path at the walk speed, looking ahead (Space)", playing, ok).clicked() {
                self.walk_path_play();
            }
            if rbtn(ui, "⏹", "Stop", "Stop walking the path (Esc)", false, self.path_run.is_some()).clicked() {
                self.walk_path_stop();
            }
            if rbtn(ui, "🎞", "To Animation", "Save the path as an animation (Animator) - edit it, play it or export a video", false, ok).clicked() {
                self.walk_path_to_animation();
            }
            if rbtn(ui, "🧭", "To Navisworks", "Send the path to Navisworks: ManageBIM › Walk path there turns it into a viewpoint animation (Saved Viewpoints) at the same walk speed and eye height", false, ok).clicked() {
                self.send_walk_path_to_navisworks();
            }
            if rbtn(ui, "🗑", "Clear", "Delete the walk path", false, !self.walk_path.is_empty()).clicked() {
                self.walk_path.clear();
                self.path_run = None;
            }
        });
        rgroup(ui, "Projection", |ui| {
            if rbtn(ui, "🔭", "Perspective", "Perspective camera", !self.cam.ortho, true).clicked() {
                self.cam.ortho = false;
            }
            if rbtn(ui, "⬚", "Orthographic", "Orthographic (parallel) camera - no perspective distortion", self.cam.ortho, true).clicked() {
                self.cam.ortho = true;
            }
        });
        rgroup(ui, "Home", |ui| {
            if rbtn(ui, "🏠", "Home", "Go to the Home view (Home key)", false, true).clicked() {
                self.go_home();
            }
            if rbtn(ui, "📌", "Set Home", "Save the current view as Home (stored in the .bimr project)", self.home.is_some(), true).clicked() {
                self.home = Some(self.cam);
                self.toast("Current view set as Home.");
            }
        });
        rgroup(ui, "Navigate", |ui| {
            if rbtn(ui, "⛶", "Zoom All", "Zoom to the whole model", false, true).clicked() {
                let mut t = self.cam;
                t.fit(&bb);
                self.fly_to(t);
            }
            if rbtn(ui, "🎯", "Focus", "Focus on the selection (F)", false, !self.selected.is_empty()).clicked() {
                let s: Vec<ElemId> = self.selected.iter().copied().collect();
                self.apply(Action::Focus(s));
            }
        });
        rgroup(ui, "Standard Views", |ui| {
            for (n, ic, yaw, pitch) in [("Top", "⏶", FRAC_PI_2, -1.55), ("Front", "⏹", FRAC_PI_2, 0.0), ("Right", "⏵", PI, 0.0), ("Back", "⏺", -FRAC_PI_2, 0.0), ("Left", "⏴", 0.0, 0.0), ("Iso", "🔭", FRAC_PI_4, -0.6)] {
                if rbtn(ui, ic, n, &format!("{n} view"), false, true).clicked() {
                    let mut t = self.cam;
                    t.set_view(yaw, pitch, &bb);
                    self.fly_to(t);
                }
            }
        });
        rgroup(ui, "Saved Viewpoints", |ui| {
            if rbtn(ui, "📷", "Save View", "Save the current view as a viewpoint", false, true).clicked() {
                self.save_viewpoint();
                self.open_panel(Panel::Viewpoints);
            }
            self.panel_btn(ui, Panel::Viewpoints, "🖼", "Viewpoints", "Saved viewpoints");
        });
        rgroup(ui, "Camera", |ui| {
            let fov = rbtn(ui, "◎", &format!("{:.0}°", self.cam.fov_deg), "Field of view", false, true);
            let id = ui.make_persistent_id("fov_pop");
            if fov.clicked() {
                ui.memory_mut(|m| m.toggle_popup(id));
            }
            egui::popup_below_widget(ui, id, &fov, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(200.0);
                ui.add(egui::Slider::new(&mut self.cam.fov_deg, 25.0..=90.0).suffix("°"));
            });
        });
    }

    fn ribbon_review(&mut self, ui: &mut egui::Ui) {
        self.ribbon_redline(ui);
        self.ribbon_issues(ui);
        rgroup(ui, "Measure", |ui| {
            for (k, ic, n) in [(MeasureKind::PointToPoint, "📏", "Point-Point"), (MeasureKind::Polyline, "〰", "Polyline"), (MeasureKind::Angle, "∠", "Angle"), (MeasureKind::Area, "⬛", "Area")] {
                let on = self.tool == Tool::Measure(k);
                if rbtn(ui, ic, n, k.label(), on, true).clicked() {
                    self.cur_measure.clear();
                    self.tool = if on { Tool::Select } else { Tool::Measure(k) };
                }
            }
            if rbtn(ui, "🗑", "Clear", "Delete all measurements", false, !self.measures.is_empty()).clicked() {
                self.measures.clear();
                self.cur_measure.clear();
            }
            self.panel_btn(ui, Panel::Measure, "📊", "List", "Measurements panel");
        });
        // ca in Navisworks (Measure > Transform): relocare cu puncte alese in model
    }

    fn ribbon_relocate(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Relocate", |ui| {
            let cur = self.xform.as_ref().map(|x| x.mode);
            let has = self.loaded_models().next().is_some();
            for (m, ic, n, tip) in [
                (crate::app::XformMode::Move, "✥", "Move", "Move a model point to point (snaps to corners)"),
                (crate::app::XformMode::Rotate, "⟳", "Rotate", "Rotate a model about a picked point (current direction -> new direction)"),
                (crate::app::XformMode::Align, "⇥", "Align", "Align a face of a model against a target face (rotation in plan + placing it on the face)"),
            ] {
                if rbtn(ui, ic, n, tip, cur == Some(m), has).clicked() {
                    if cur == Some(m) {
                        self.xform = None;
                    } else {
                        let sel = self.relocate_selection;
                        self.xform_start(m, sel);
                    }
                }
            }
            let sel = rbtn(ui, "◎", if self.relocate_selection { "Selection" } else { "Model" }, "Relocate: whole model or only the selection", false, true);
            let sid = ui.make_persistent_id("reloc_sel_pop");
            if sel.clicked() {
                ui.memory_mut(|m| m.toggle_popup(sid));
            }
            egui::popup_below_widget(ui, sid, &sel, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(180.0);
                if ui.selectable_label(!self.relocate_selection, "Model").clicked() {
                    self.relocate_selection = false;
                    ui.close_menu();
                }
                if ui.selectable_label(self.relocate_selection, "Selection").clicked() {
                    self.relocate_selection = true;
                    ui.close_menu();
                }
            });
        });
    }

    fn ribbon_sections(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Sectioning", |ui| {
            let sec = self.section.enabled;
            if rbtn(ui, "✂", "Enable", "Turn sectioning on / off", sec, true).clicked() {
                self.section.enabled = !sec;
                if !sec {
                    self.init_section_defaults();
                    self.open_panel(Panel::Section);
                }
            }
            if rbtn(ui, "⬚", "Fit Section", "Section box around the selected elements, and zoom to them (also: right-click > Section box around selection)", false, !self.selected.is_empty()).clicked() {
                self.section_to_selection();
            }
            if rbtn(ui, "▣", "Model box", "Section box around the whole model - stretch it with the arrows on its faces (like Revit)", false, self.loaded_models().next().is_some()).clicked() {
                let bb = self.scene_aabb();
                if bb.is_valid() {
                    self.section.enabled = true;
                    self.section.mode = SectionMode::Box;
                    self.section.bmin = (bb.min - Vec3::splat(0.3)).to_array();
                    self.section.bmax = (bb.max + Vec3::splat(0.3)).to_array();
                    self.section_level = None;
                }
            }
            if rbtn(ui, "🔗", "Follow", "Section box follows the selection: every time you select elements the box fits around them (the camera stays)", self.section_follow, true).clicked() {
                self.section_follow = !self.section_follow;
                self.section_sel_sig = 0;
            }
            self.panel_btn(ui, Panel::Section, "⚙", "Planes", "Section planes & box");
            // contur cub on/off (inainte era o bifa „Hide section box" care facea invers)
            if rbtn(ui, "▣", "Box outline", "Show the outline of the section box and its face arrows in 3D. The cut and all exports stay active when it is off.", self.settings.show_section_box, true).clicked() {
                self.settings.show_section_box = !self.settings.show_section_box;
                self.settings.save();
            }
            // exporturile (DWG / PDF / Title Block / Overlay / Preview) sunt DOAR în tab-ul Annotation
        });
        // acces rapid: nivelul, sus / jos, box de nivel sau taietura de plan
        let levels = self.scene_grids().levels;
        if !levels.is_empty() {
            rgroup(ui, "Level", |ui| {
                let li = self.section_level.unwrap_or(0).min(levels.len() - 1);
                let cur = &levels[li].0;
                let lev = rbtn(ui, "▤", cur, "Current level (click to choose)", false, true);
                let lid = ui.make_persistent_id("rib_lev_pop");
                if lev.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(lid));
                }
                egui::popup_below_widget(ui, lid, &lev, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                    ui.set_min_width(180.0);
                    for (i, l) in levels.iter().enumerate().rev() {
                        if ui.selectable_label(self.section_level == Some(i), &l.0).clicked() {
                            self.section_level_box(i);
                            ui.close_menu();
                        }
                    }
                });
                if rbtn(ui, "▼", "Below", "Level below (PgDn)", false, true).clicked() {
                    self.section_level_step(-1);
                }
                if rbtn(ui, "▲", "Above", "Level above (PgUp)", false, true).clicked() {
                    self.section_level_step(1);
                }
                if rbtn(ui, "▣", "Box", "Section box on this level (floor to next)", false, true).clicked() {
                    self.section_level_box(li);
                }
                if rbtn(ui, "✂", "Cut", "Plan cut at the current height", false, true).clicked() {
                    self.section_plan_cut(li, self.plan_cut_h);
                }
                if rbtn(ui, "👁", "Isolate", "Only this level's items", false, true).clicked() {
                    self.isolate_level(li);
                }
            });
        }
        self.visibility_group(ui);
    }

    /// Tab-ul Annotation: tot ce tine de desenul 2D al sectiunii - export DWG / PDF,
    /// anotatii (cote, niveluri, grile, hasura) si previewul mic, usor de gasit.
    fn ribbon_annotation(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Cut & Export", |ui| {
            if rbtn(ui, "📐", "Cut to DWG", "Export the section cut as 2D lines (DWG / DXF), one layer per element type - with hatching, dimension chains, levels and tags", false, self.section.enabled).clicked() {
                self.cad_sheet = false;
                self.request_path(Purpose::ExportSectionCad);
            }
            if rbtn(ui, "📄", "Cut on Sheet", "The same cut on a drawing sheet (A3 - A0) at a standard scale, with frame and title block (DWG / DXF)", false, self.section.enabled).clicked() {
                self.cad_sheet = true;
                self.request_path(Purpose::ExportSectionCad);
            }
            if rbtn(ui, "📄", "Cut to PDF", "Export the cut as a vector PDF sheet (A3 - A0) with title block, at the exact cut position", false, self.section.enabled || self.settings.cad_view != crate::app::CadView::Current).clicked() {
                self.request_path(Purpose::ExportSectionPdf);
            }
            if rbtn(ui, "🗺", "2D Preview", "Small closable map-style preview of the cut, before exporting", self.section_preview_open, self.section.enabled).clicked() {
                self.section_preview_open = !self.section_preview_open;
            }
            let box_on = self.section.enabled && self.section.mode == SectionMode::Box;
            if rbtn(ui, "◫", "Multi view", "Several views of the section box on one drawing, like an Inventor sheet: choose the views (Front, Right, Back, Left, Top, Bottom) and the spacing, see the preview, then export DWG / DXF at 1:1 in model space (Front stays in real coordinates) or a PDF sheet.", self.mv_open, box_on).clicked() {
                self.mv_open = !self.mv_open;
            }
        });
        rgroup(ui, "Cut View", |ui| {
            let box_on = self.section.enabled && self.section.mode == SectionMode::Box;
            if rbtn(ui, "🟩", "Pick face", "Click a face of the section box in the 3D view (it turns light green): the 2D drawing is made from that face - Top, Bottom, Front, Back, Left or Right. Esc cancels.", self.face_pick, box_on).clicked() {
                self.face_pick = !self.face_pick;
            }
            let cur = self.settings.cad_view;
            egui::ComboBox::from_id_salt("annot_cut_view").selected_text(crate::i18n::tr(cur.label())).show_ui(ui, |ui| {
                for v in crate::app::CadView::ALL {
                    let label = crate::i18n::tr(v.label());
                    let mut tmp = v;
                    if ui.selectable_value(&mut tmp, cur, label).clicked() {
                        self.settings.cad_view = v;
                        // highlightul nu se aprinde automat - doar utilizatorul il deschide
                        if v != crate::app::CadView::Current {
                            self.settings.cad_view_pos = 0.0;
                        }
                        self.settings.save();
                    }
                }
            });
        });
        rgroup(ui, "Annotations", |ui| {
            if rbtn(ui, "✏", "Title Block", "Edit the title block, sheet format (A3 - A0) and scale", self.title_block_open, true).clicked() {
                self.title_block_open = !self.title_block_open;
            }
            // Grids / Levels se comută în grupul „Grids & Levels" (Home / Sections), nu aici.
            if rbtn(ui, "▤", "Filled cuts", "Cut faces drawn solid and hatched (closed solids), instead of seeing inside the elements", self.section_caps, true).clicked() {
                self.section_caps = !self.section_caps;
            }
            if rbtn(ui, "▨", "Hatch", "Hatch the cut materials (filled contours) in the exported DWG / PDF cut - off = clean outline", self.cad_hatch, true).clicked() {
                self.cad_hatch = !self.cad_hatch;
            }
            if rbtn(ui, "📐", "Dimensions", "Include dimension chains (mm / m) in the exported DWG / PDF cut", self.cad_dims, true).clicked() {
                self.cad_dims = !self.cad_dims;
            }
            if rbtn(ui, "≡", "Levels", "Include level lines and elevation labels in the exported DWG / PDF cut", self.cad_levels, true).clicked() {
                self.cad_levels = !self.cad_levels;
            }
            if rbtn(ui, "🗺", "DWG Overlay", "Lay a 2D drawing (DWG / DXF) over the model at a level: compare the plan with the model", self.overlay_open, true).clicked() {
                self.overlay_open = !self.overlay_open;
            }
        });
    }

    /// Toate comenzile BIM (IFC, IDS, clasificare, date, schimb) intr-un singur tab.
    fn ribbon_bim(&mut self, ui: &mut egui::Ui) {
        let has = self.loaded_models().next().is_some();
        rgroup(ui, "IFC export", |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("MVD").small().weak()).on_hover_text("Model View Definition: which properties are written to the IFC file");
                egui::ComboBox::from_id_salt("bim_mvd").width(180.0).selected_text(self.settings.ifc_mvd.code()).show_ui(ui, |ui| {
                    for m in crate::ifc_mvd::Mvd::ALL {
                        if ui.selectable_value(&mut self.settings.ifc_mvd, m, m.label()).on_hover_text(m.tip()).clicked() {
                            self.settings.save();
                        }
                    }
                });
            });
            if rbtn(ui, "fmt:IFC", "Export IFC", "Export all visible items to IFC4 (geometry, colours, levels, GUIDs, properties). The MVD above selects which properties are written.", false, has).clicked() {
                self.ifc_selection = false;
                self.request_path(Purpose::ExportIfc);
            }
            if rbtn(ui, "fmt:IFC", "Selection IFC", "Export only the selected items to IFC4", false, !self.selected.is_empty()).clicked() {
                self.ifc_selection = true;
                self.request_path(Purpose::ExportIfc);
            }
            if rbtn(ui, "🗺", "Map classes", "Map non-IFC categories (STEP, OBJ, glTF, DWG) to an IFC class before export, like in Revit. Opens Settings › IFC export — category mapping. Import / export the table as CSV.", false, true).clicked() {
                self.settings_open = true;
            }
        });
        rgroup(ui, "Classification", |ui| {
            if rbtn(ui, "🏷", "bSDD", "Classification: search the buildingSMART Data Dictionary online or the local Uniclass / OmniClass catalogue and assign codes to the selected elements (exported to IFC)", self.class_open, true).clicked() {
                self.class_open = !self.class_open;
            }
            if rbtn(ui, "📋", "IDS check", "Check the model against buildingSMART IDS requirements (Model Checker)", false, has).clicked() {
                self.open_panel(crate::layout::Panel::Check);
            }
            if rbtn(ui, "📥", "Import IDS", "Load an IDS (.ids) requirements file", false, true).clicked() {
                self.request_path(Purpose::ImportCheckRules);
            }
        });
        rgroup(ui, "Data / COBie", |ui| {
            if rbtn(ui, "🏷", "Properties", "Property editor: pick elements and add your own properties, or fill the standard IFC property sets (Pset_WallCommon, Pset_DoorCommon, manufacturer, warranty...) - all exported to IFC with the correct data types", self.pe.open, has).clicked() {
                if self.pe.open {
                    self.pe.open = false;
                } else {
                    self.open_prop_editor();
                }
            }
            if rbtn(ui, "📥", "Import Props", "Write properties onto elements from Excel / CSV, matched by GlobalId / Name", false, has).clicked() {
                self.request_path(Purpose::ImportUserProps);
            }
            if rbtn(ui, "📤", "Export Props", "Excel template: one row per element with its properties", false, has).clicked() {
                self.request_path(Purpose::ExportUserProps);
            }
            if rbtn(ui, "🏗", "COBie", "COBie 2.4 handover workbook (xlsx)", false, has).clicked() {
                self.request_path(Purpose::ExportCobie);
            }
            if rbtn(ui, "📊", "Quantities", "Count, volume, areas, lengths by any property - export to Excel", false, has).clicked() {
                self.open_panel(crate::layout::Panel::Quantities);
            }
        });
        rgroup(ui, "Open standards", |ui| {
            if rbtn(ui, "fmt:GLB", "Export GLB", "Export to glTF binary for Blender / Rhino / Twinmotion: colours, materials, hierarchy and BIM properties", false, has).clicked() {
                self.ifc_selection = !self.selected.is_empty();
                self.request_path(Purpose::ExportGlb);
            }
            if rbtn(ui, "fmt:BCF", "Import BCF", "Import BCF 2.1 / 3.0 issues (Revit, ACC, Solibri, BIMcollab)", false, true).clicked() {
                self.request_path(Purpose::ImportBcf);
            }
            if rbtn(ui, "📷", "Import VP", "Import viewpoints exported from Navisworks", false, true).clicked() {
                self.request_path(Purpose::ImportNavisXml);
            }
            if rbtn(ui, "📋", "Sets", "Import / export selection and search sets (Navisworks XML)", false, true).clicked() {
                self.request_path(Purpose::ExportNavisSets);
            }
        });
    }

    fn ribbon_coord(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Coordination", |ui| {
            self.panel_btn(ui, Panel::Clash, "⚠", "Clash", "Clash Detective — clashes are shown only while this panel is visible");
            self.panel_btn(ui, Panel::Check, "✔", "Checker", "Model Checker: check properties and data against rules or a buildingSMART IDS file - results to Excel / BCF");
            self.panel_btn(ui, Panel::TimeLiner, "📅", "TimeLiner", "4D schedule and simulation");
            self.panel_btn(ui, Panel::Find, "🔍", "Find Items", "Search by properties");
            self.panel_btn(ui, Panel::Sets, "📋", "Sets", "Selection & search sets");
        });
        self.coord_ribbon(ui);
    }

    fn ribbon_models(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Models", |ui| {
            self.panel_btn(ui, Panel::Quantities, "📊", "Quantities", "Quantification: count, volume, areas, lengths grouped by any property - export to Excel");
            self.panel_btn(ui, Panel::Compare, "⚖", "Compare", "Compare two versions of a model: removed, added, moved / reshaped and changed-property elements");
            self.panel_btn(ui, Panel::Profiler, "🎨", "Profiler", "Appearance Profiler: colour the model by any property (Level, System, Fire Rating, Volume...)");
            self.panel_btn(ui, Panel::Coords, "📐", "Coordinates", "Move / rotate models and elements (with Reset)");
            self.panel_btn(ui, Panel::Project, "📁", "Project", "Project & Model Properties");
            if rbtn(ui, "📦", "Export .bimc", "Write a .bimc cache next to every loaded model", false, true).clicked() {
                self.export_all_caches();
            }
        });
        rgroup(ui, "Scan", |ui| {
            let has_cloud = self.loaded_models().any(|(_, m)| m.cloud.is_some());
            if rbtn(ui, "☁", "Point Cloud", "Point cloud display: density, point size, real size, colour (scan RGB / elevation / grey), round points", self.show_cloud, has_cloud).clicked() {
                self.show_cloud = !self.show_cloud;
            }
            if rbtn(ui, "☁", "Scan vs Model", "As-built check: compare the model with the point cloud (E57 / LAS) - built, partial, missing, not scanned; colours and Excel report", self.asbuilt_open, has_cloud).clicked() {
                self.asbuilt_open = !self.asbuilt_open;
            }
        });
    }

    fn ribbon_render(&mut self, ui: &mut egui::Ui) {
        self.style_group(ui);
        rgroup(ui, "Presentation", |ui| {
            let on = self.panel_shown(Panel::Animation);
            if rbtn(ui, "🎞", "Animations", "Animations: Record, Orbit 360°, viewpoints, 4D - play them and export video", on, true).clicked() {
                if on {
                    self.settings.layout.close(Panel::Animation);
                } else {
                    self.open_panel(Panel::Animation);
                }
            }
            // fereastra proprie, pe toata inaltimea: lista de materiale lunga si lata
            if rbtn(ui, "🎨", "Materials", "Material library, image textures & assignments", self.mat_window, true).clicked() {
                self.mat_window = !self.mat_window;
                if self.mat_window && self.panel_shown(Panel::Render) && self.render_subtab == 1 {
                    // acelasi editor nu sta deschis in doua locuri
                    self.render_subtab = 0;
                }
            }
            if rbtn(ui, "💡", "Show Mat.", "Show render materials in the viewport", self.materials_on, true).clicked() {
                self.materials_on = !self.materials_on;
                self.materials_changed();
            }
        });
        rgroup(ui, "Textures", |ui| {
            if rbtn(ui, "📥", "Images", "Import PNG / JPG images as new texture materials", false, true).clicked() {
                self.render_subtab = 1;
                self.open_panel(Panel::Render);
                self.request_path(Purpose::ImportTextures);
            }
            if rbtn(ui, "📥", "Import Lib", "Import materials into your library: a PBR pack .zip (ambientCG, freepbr, Poly Haven...), a .bimmat library or an Excel / CSV list - saved for every project", false, true).clicked() {
                self.request_path(Purpose::ImportMaterials);
            }
            if rbtn(ui, "📤", "Export Lib", "Export the material library (.bimmat)", false, true).clicked() {
                self.request_path(Purpose::ExportMaterials);
            }
        });
        rgroup(ui, "Output", |ui| {
            if rbtn(ui, "🖼", "Image", "Render the current view to PNG", false, true).clicked() {
                self.request_path(Purpose::RenderImage);
            }
            if rbtn(ui, "◎", "Path trace", "Progressive path traced image: preview, samples, time limit, Cancel, export PNG", false, true).clicked() {
                self.pathtrace_open = true;
            }
            if rbtn(ui, "💡", "Lighting", "Create point / spot / distant lights, set their power (lux) and range, place and move them in the 3D view", false, true).clicked() {
                self.render_subtab = 2;
                self.open_panel(Panel::Render);
            }
            if rbtn(ui, "🎞", "Video", "Animations and video export", false, true).clicked() {
                self.open_panel(Panel::Animation);
            }
        });
    }

    fn ribbon_output(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Clash Reports", |ui| {
            let can = !self.clash_tests.is_empty();
            if rbtn(ui, "📄", "HTML", "Export the clash report of the current test (HTML)", false, can).clicked() {
                self.request_path(Purpose::ExportClashHtml);
            }
            if rbtn(ui, "📊", "CSV", "Export the clash report of the current test (CSV)", false, can).clicked() {
                self.request_path(Purpose::ExportClashCsv);
            }
            if rbtn(ui, "📄", "PDF", "Export the clash report of the current test as a professional PDF sheet with title block", false, can).clicked() {
                self.request_path(Purpose::ExportClashPdf);
            }
        });
        rgroup(ui, "Navisworks", |ui| {
            if rbtn(ui, "📷", "Viewpoints", "Export viewpoints with markups, comments and section box to Navisworks", false, !self.viewpoints.is_empty()).clicked() {
                self.request_path(Purpose::ExportNavisXml);
            }
            if rbtn(ui, "⚠", "Clash Tests", "Export clash tests and sets to Navisworks", false, !self.clash_tests.is_empty()).clicked() {
                self.request_path(Purpose::ExportNavisClash);
            }
        });
        // formatele deschise si aici (nu doar in tab-ul BIM), cu iconite de format de fisier
        let has = self.loaded_models().next().is_some();
        rgroup(ui, "Open standards", |ui| {
            if rbtn(ui, "fmt:IFC", "IFC", "Export all visible items to IFC4 (MVD from the BIM tab). With a selection: only the selected items.", false, has).clicked() {
                self.ifc_selection = !self.selected.is_empty();
                self.request_path(Purpose::ExportIfc);
            }
            if rbtn(ui, "fmt:GLB", "GLB", "glTF binary for Blender / Rhino / Twinmotion / Unreal: colours, materials, hierarchy and BIM properties", false, has).clicked() {
                self.ifc_selection = !self.selected.is_empty();
                self.request_path(Purpose::ExportGlb);
            }
            if rbtn(ui, "fmt:BCF", "BCF", "Viewpoints and issues as BCF 2.1 (Revit, Navisworks, Solibri, BIMcollab, ACC...)", false, !self.viewpoints.is_empty()).clicked() {
                self.request_path(Purpose::ExportBcf);
            }
        });
    }

    fn ribbon_share(&mut self, ui: &mut egui::Ui) {
        let has = self.loaded_models().next().is_some();
        let what = if self.selected.is_empty() { "all visible items" } else { "the selected items" };
        rgroup(ui, "Web", |ui| {
            if rbtn(ui, "🌐", "Web viewer", &format!("One .html file with {what}: 3D view, tree, search, properties, hide / isolate, X-ray.\nOpens in Chrome / Edge / Firefox / Safari, offline, nothing to install."), false, has).clicked() {
                self.web_selection = !self.selected.is_empty();
                self.request_path(Purpose::ExportWeb);
            }
        });
        rgroup(ui, "Reference", |ui| {
            if rbtn(ui, "🔗", "Revit / CAD", &format!("Reference of {what} for Revit and AutoCAD / Plant 3D (MBREFERENCE). Revit can also link the IFC export from the BIM tab."), false, has).clicked() {
                self.request_path(Purpose::ExportReference);
            }
        });
        rgroup(ui, "Report", |ui| {
            if rbtn(ui, "📊", "Dashboard", "Project summary: models, clashes, issues, checks, levels - export as a PDF or Word report", self.dash_open, true).clicked() {
                self.dash_open = !self.dash_open;
                self.dash = None;
            }
        });
        rgroup(ui, "AI", |ui| {
            let on = self.mcp.is_some();
            let tip = if on {
                format!("MCP server running on {}\nAI assistants (Claude, OpenCode, Ollama clients) can read and drive the model", crate::mcp::url(self.settings.mcp.port))
            } else {
                "Connect AI assistants (Claude Code / Desktop, OpenCode, VS Code, local Ollama via an MCP client) to ManageBIM".to_string()
            };
            if rbtn(ui, "🤖", "AI connection", &tip, on || self.mcp_open, true).clicked() {
                self.mcp_open = !self.mcp_open;
            }
        });
    }

    fn ribbon_output2(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Data", |ui| {
            let has_sel = !self.selected.is_empty();
            let _ = has_sel;
            if rbtn(ui, "🏷", "Properties", "Property editor: pick elements and add your own properties, or fill the standard IFC property sets (Pset_WallCommon, Pset_DoorCommon, manufacturer, warranty...) - all exported to IFC", self.pe.open, self.loaded_models().next().is_some()).clicked() {
                if self.pe.open {
                    self.pe.open = false;
                } else {
                    self.open_prop_editor();
                }
            }
            if rbtn(ui, "📥", "Import Properties", "Write properties onto elements from Excel / CSV: one row per element (matched by GlobalId, Name or any property), every other column = a property", false, self.loaded_models().next().is_some()).clicked() {
                self.request_path(Purpose::ImportUserProps);
            }
            if rbtn(ui, "📤", "Export Properties", "Excel template: one row per element (selection, or all visible) with its key and its user properties", false, self.loaded_models().next().is_some()).clicked() {
                self.request_path(Purpose::ExportUserProps);
            }
            if rbtn(ui, "🏗", "COBie…", "COBie 2.4 handover workbook (xlsx): Contact, Facility, Floor, Type, Component, System — for facility management", false, self.loaded_models().next().is_some()).clicked() {
                self.request_path(Purpose::ExportCobie);
            }
            let r = rbtn(ui, "📋", "Sets ⏷", "Import / export selection and search sets", false, true);
            let pid = ui.make_persistent_id("data_sets_menu");
            if r.clicked() {
                ui.memory_mut(|m| m.toggle_popup(pid));
            }
            egui::popup_below_widget(ui, pid, &r, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                ui.set_min_width(190.0);
                if ui.add_enabled(!self.sets.is_empty(), egui::Button::new("📤 Export sets…")).clicked() {
                    self.request_path(Purpose::ExportSets);
                }
                if ui.button("📥 Import sets…").clicked() {
                    self.request_path(Purpose::ImportSets);
                }
            });
            let r = rbtn(ui, "📅", "Schedule ⏷", "Import / export the TimeLiner schedule", false, true);
            let pid = ui.make_persistent_id("data_sched_menu");
            if r.clicked() {
                ui.memory_mut(|m| m.toggle_popup(pid));
            }
            egui::popup_below_widget(ui, pid, &r, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                ui.set_min_width(230.0);
                if ui.button("📤 Export schedule (CSV)…").clicked() {
                    self.request_path(Purpose::ExportTasksCsv);
                }
                if ui.button("📥 Import schedule…").on_hover_text("CSV, MS Project XML or Primavera P6 XML.\nLink tasks to elements with [Level=Level 1;Type=IfcSlab] at the end of the task name (or Text1 in MS Project).").clicked() {
                    self.request_path(Purpose::ImportTasksCsv);
                }
            });
            if rbtn(ui, "📦", "BIMC", "Export .bimc caches next to the models", false, true).clicked() {
                self.export_all_caches();
            }
        });
        rgroup(ui, "Visuals", |ui| {
            if rbtn(ui, "🖼", "Image", "Render the current view to PNG", false, true).clicked() {
                self.request_path(Purpose::RenderImage);
            }
        });
    }

    /// Stilul de afisare: Realistic / Shaded / Concept / Wireframe
    fn style_group(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Visual Style", |ui| {
            use crate::render::VisualStyle as V;
            let cur = self.settings.visual_style;
            for (s, icon, tip) in [
                (V::Realistic, "☀", "Materials, textures, reflections, contact shadows"),
                (V::Shaded, "◆", "Model colours with light, no textures"),
                (V::Concept, "✏", "White study model with strong edges"),
                (V::Hidden, "◫", "Hidden line: neutral surfaces, hidden edges removed"),
                (V::Wireframe, "▦", "Triangle edges only - see through the model"),
            ] {
                if rbtn(ui, icon, s.label(), tip, cur == s, true).clicked() && cur != s {
                    self.set_visual_style(s);
                }
            }
        });
    }

    /// Starea curenta de vizualizare ca sablon (numele / nota / flagurile se completeaza de apelant).
    fn view_template_snapshot(&self, name: String) -> crate::app::ViewTemplate {
        crate::app::ViewTemplate {
            name,
            hidden: self.hidden.iter().copied().collect(),
            section: self.section.clone(),
            cam: self.cam,
            visual_style: self.settings.visual_style,
            show_grids: self.show_grids,
            show_levels: self.show_levels,
            section_caps: self.section_caps,
            overrides: self.overrides.iter().map(|(k, v)| (*k, *v)).collect(),
            selected: self.selected.iter().copied().collect(),
            measures: self.measures.clone(),
            vp_filter: self.vp_filter.clone(),
            is_default: false,
            note: String::new(),
            include: [true; 5],
        }
    }

    /// Salvează starea curentă de vizualizare ca şablon în proiect.
    fn save_view_template(&mut self) {
        let t = self.view_template_snapshot(format!("View {}", self.view_templates.len() + 1));
        self.view_templates.push(t);
        self.vt_current = self.view_templates.len() - 1;
        self.vt_applied = true;
        self.toast("View template saved (in project)");
    }

    /// Suprascrie sablonul `i` cu vederea curenta, pastrand numele, nota, steaua si ce include.
    fn update_view_template(&mut self, i: usize) {
        let Some(old) = self.view_templates.get(i).cloned() else { return };
        let mut t = self.view_template_snapshot(old.name.clone());
        t.note = old.note;
        t.is_default = old.is_default;
        t.include = old.include;
        self.view_templates[i] = t;
        self.toast(format!("'{}' updated with the current view", old.name));
    }

    /// Aplică un şablon de vizualizare - doar partile bifate la „Include".
    fn apply_view_template(&mut self, i: usize) {
        let Some(t) = self.view_templates.get(i).cloned() else { return };
        let [cam, vis, sec, style, col] = t.include;
        if vis {
            self.hidden = t.hidden.into_iter().collect();
            self.selected = t.selected.into_iter().collect();
            self.measures = t.measures;
            self.vp_filter = t.vp_filter;
        }
        if sec {
            self.section = t.section;
        }
        if cam {
            self.fly_to(t.cam);
        }
        if style {
            self.apply_style_only(t.visual_style);
            self.show_grids = t.show_grids;
            self.show_levels = t.show_levels;
            self.section_caps = t.section_caps;
        }
        if col {
            self.overrides = t.overrides.into_iter().collect();
        }
        self.vt_current = i;
        self.vt_applied = true;
        // vederea nu mai este cea a viewpoint-ului activ: nu-l mai aratam ca activ (si nici redline-urile lui)
        if cam || vis || sec {
            self.active_vp = None;
        }
        self.colors_dirty = true;
        self.toast(format!("Applied '{}'", t.name));
    }

    fn ribbon_view(&mut self, ui: &mut egui::Ui) {
        self.style_group(ui);
        rgroup(ui, "Compare", |ui| {
            if rbtn(ui, "◫", "Split view", "Two viewports side by side in this window (drag the divider to stretch)", self.split_view, true).clicked() {
                self.split_view = !self.split_view;
                if self.split_view { self.cam_b = self.cam; self.section_b = self.section.clone(); }
            }
            if rbtn(ui, "🔗", "Link cameras", "Copy camera A to B when turning link on; unlink to move B independently", self.split_link, self.split_view).clicked() {
                self.split_link = !self.split_link;
                if self.split_link { self.cam_b = self.cam; self.section_b = self.section.clone(); }
            }
        });
        rgroup(ui, "View Templates", |ui| {
            if rbtn(ui, "➕", "Save", "Save the current visibility, section and visual style as a template in this project", false, true).clicked() {
                self.save_view_template();
            }
            let n = self.view_templates.len();
            if rbtn(ui, "◫", &format!("Apply ({n})"), "Apply a saved view template", false, n > 0).clicked() {
                self.view_templates_open = !self.view_templates_open;
            }
        });
        // ferestrele si optiunile in meniuri derulante: ribbon-ul pastreaza aceeasi inaltime pe toate tab-urile
        rgroup(ui, "Windows", |ui| {
            let open_n = crate::layout::ALL_PANELS.iter().filter(|p| self.settings.layout.is_open(**p)).count();
            let r = rbtn(ui, "🗔", &format!("Panels ({open_n}) ⏷"), "Show / hide any panel", false, true);
            let id = ui.make_persistent_id("win_menu");
            if r.clicked() {
                ui.memory_mut(|m| m.toggle_popup(id));
            }
            egui::popup_below_widget(ui, id, &r, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(200.0);
                for p in crate::layout::ALL_PANELS.iter() {
                    let mut open = self.settings.layout.is_open(*p);
                    if ui.checkbox(&mut open, crate::theme::title_job(p.title(), 14.0, ui.visuals().text_color())).changed() {
                        if open {
                            self.open_panel(*p);
                        } else {
                            self.settings.layout.close(*p);
                        }
                    }
                }
            });
            let all_auto = self.settings.layout.autohide.iter().all(|a| *a);
            if rbtn(ui, "📌", "Auto-hide", "All panel areas auto-hide: thin tab strips, the panels slide in over the model when you point at them", all_auto, true).clicked() {
                self.settings.layout.autohide = [!all_auto; 3];
                self.flyout = None;
            }
            if rbtn(ui, "⛶", "Hide panels", "Hide all panels - the whole window for the model (F11)", self.settings.layout.hide_all, true).clicked() {
                self.settings.layout.hide_all = !self.settings.layout.hide_all;
                self.flyout = None;
            }
            if rbtn(ui, "⟲", "Reset", "Restore the default window arrangement", false, true).clicked() {
                self.settings.layout = Default::default();
            }
        });
        rgroup(ui, "Scene", |ui| {
            // un singur loc pentru lumina: soare, cer, lampi (Render › Lighting)
            if rbtn(ui, "💡", "Lighting", "Sun, sky, brightness and lamps - all in the Lighting panel (also used for renders and videos)", false, true).clicked() {
                self.render_subtab = 2;
                self.open_panel(Panel::Render);
            }
            let has_cloud = self.loaded_models().any(|(_, m)| m.cloud.is_some());
            if rbtn(ui, "☁", "Point Cloud", "Point density and size (E57 point clouds)", self.show_cloud, has_cloud).clicked() {
                self.show_cloud = !self.show_cloud;
            }
        });
    }

    fn ribbon_grids(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Grids & Levels", |ui| {
            if rbtn(ui, "#", "Grids", "Show / hide the grid axes in the 3D view", self.show_grids, true).clicked() {
                self.show_grids = !self.show_grids;
            }
            if rbtn(ui, "≡", "Levels", "Show / hide the levels in the 3D view", self.show_levels, true).clicked() {
                self.show_levels = !self.show_levels;
            }
            if rbtn(ui, "✏", "Edit Grids", "Create grid axes in ManageBIM (generator or one by one), size, export DXF / DWG", self.show_grids_editor, true).clicked() {
                self.show_grids_editor = !self.show_grids_editor;
            }
            if rbtn(ui, "✏", "Edit Levels", "Your own levels: ±0.00 of the project, names and elevations (editable)", self.show_levels_editor, true).clicked() {
                self.show_levels_editor = !self.show_levels_editor;
            }
        });
    }

    fn ribbon_view2(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Language", |ui| {
            if rbtn(ui, "🌐", if self.settings.romanian { "English" } else { "Română" }, "Interface language: English / Română", false, true).clicked() {
                self.toggle_language();
            }
        });
        rgroup(ui, "Theme", |ui| {
            let cur = self.settings.theme;
            for (t, icon, tip) in [
                (Theme::Studio, "◆", "Studio theme"),
                (Theme::Dark, "🌙", "Dark theme"),
                (Theme::Nord, "❄", "Nord theme"),
                (Theme::TokyoNight, "🌃", "Tokyo Night theme"),
                (Theme::Light, "☀", "Light theme"),
            ] {
                if rbtn(ui, icon, t.label(), tip, cur == t, true).clicked() && cur != t {
                    self.set_theme(t);
                }
            }
        });
        rgroup(ui, "Interface", |ui| {
            // fereastra ingusta: doar valoarea (glisare), fara eticheta si slider lat
            let narrow = crate::theme::ribbon_btn_w() < 55.0 || crate::theme::RIBBON_COMPACT.load(std::sync::atomic::Ordering::Relaxed);
            ui.horizontal(|ui| {
                let mut pct = (self.settings.ui_scale * 100.0).round() as i32;
                let r = if narrow {
                    ui.add(egui::DragValue::new(&mut pct).range(75..=200).speed(1.0).prefix("UI ").suffix("%"))
                } else {
                    ui.label("Font / UI size");
                    ui.add_sized([110.0, ui.spacing().interact_size.y], egui::Slider::new(&mut pct, 75..=200).step_by(5.0).suffix("%"))
                };
                if r.changed() && (!r.dragged() || r.drag_stopped()) {
                    self.settings.ui_scale = pct as f32 / 100.0;
                    self.settings.save();
                }
            })
            .response
            .on_hover_text("Scales the whole interface: fonts, panels, tables and dialogs. 100% = default. Saved between sessions.");
        });
        rgroup(ui, "Options", |ui| {
            let r = rbtn(ui, "⚙", "Options ⏷", "Cache, converters, file associations", false, true);
            let id = ui.make_persistent_id("opt_menu");
            if r.clicked() {
                ui.memory_mut(|m| m.toggle_popup(id));
            }
            egui::popup_below_widget(ui, id, &r, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(260.0);
                let mut c = self.settings.clone();
                ui.checkbox(&mut c.auto_cache, "Create .bimc cache when opening a model");
                ui.checkbox(&mut c.use_cache, "Open the up-to-date .bimc cache instead");
                if ui.checkbox(&mut c.units_mm, "Lengths in millimetres (measurements, distances)").changed() {
                    self.settings.units_mm = c.units_mm;
                    self.settings.save();
                }
                ui.horizontal(|ui| {
                    ui.label("Interface size");
                    let mut pct = (self.settings.ui_scale * 100.0).round() as i32;
                    let r = ui.add_sized([140.0, ui.spacing().interact_size.y], egui::Slider::new(&mut pct, 75..=200).step_by(5.0).suffix("%"));
                    if r.changed() && (!r.dragged() || r.drag_stopped()) {
                        self.settings.ui_scale = pct as f32 / 100.0;
                        self.settings.save();
                    }
                })
                .response
                .on_hover_text("Scales the whole interface: text, panels, tables and dialogs (100% = default). Same as the DPI setting of Windows, but only for ManageBIM.");
                ui.horizontal(|ui| {
                    ui.label("Navisworks XML units");
                    let cur = self.settings.navis_units.clone();
                    let shown = if cur == "auto" { format!("Auto ({})", self.navis_units()) } else { cur.clone() };
                    egui::ComboBox::from_id_salt("nw_units").selected_text(shown).show_ui(ui, |ui| {
                        for u in ["auto", "m", "cm", "mm", "ft", "in"] {
                            if ui.selectable_label(cur == u, if u == "auto" { "Auto (Navisworks model units)" } else { u }).clicked() {
                                self.settings.navis_units = u.to_string();
                                self.settings.save();
                            }
                        }
                    });
                })
                .response
                .on_hover_text("Viewpoints are written in the units of the Navisworks document (Options > Interface > Display Units) so they land in the right place. Auto uses the units of a model opened from Navisworks, otherwise metres.");
                if c.auto_cache != self.settings.auto_cache || c.use_cache != self.settings.use_cache {
                    self.settings.auto_cache = c.auto_cache;
                    self.settings.use_cache = c.use_cache;
                    self.settings.save();
                }
                ui.separator();
                if ui.button("Converters (DWG, Navisworks)…").clicked() {
                    self.show_tools = true;
                    ui.close_menu();
                }
                #[cfg(windows)]
                if ui.button("Associate .bimc / .bimr with ManageBIM").clicked() {
                    self.register_file_types();
                    ui.close_menu();
                }
            });
        });
    }

    fn ribbon_help(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Help", |ui| {
            if rbtn(ui, "🖱", "Controls", "Mouse & keyboard navigation", false, true).clicked() {
                self.show_help = true;
            }
            if rbtn(ui, "⌨", "Shortcuts", "All keyboard shortcuts (F1)", self.keys_open, true).clicked() {
                self.keys_open = !self.keys_open;
            }
            if rbtn(ui, "📄", "Manual", "The ManageBIM manual: every feature, step by step, with search", self.manual_open, true).clicked() {
                self.manual_open = !self.manual_open;
            }
            if rbtn(ui, "🏗", "Demo", "Open the demo project (Block A: architecture + structure, MEP, 4D schedule)", false, true).clicked() {
                self.open_demo();
            }
            if rbtn(ui, "≡", "Commands", "Command palette (Ctrl+K): type what you want to do", self.palette_open, true).clicked() {
                self.palette_open = true;
            }
            if rbtn(ui, "❓", "About", "About ManageBIM", false, true).clicked() {
                self.show_about = true;
            }
        });
    }

    fn to_red(&self, rect: Rect, p: Pos2) -> [f32; 2] {
        [(p.x - rect.center().x) / rect.height(), (p.y - rect.center().y) / rect.height()]
    }

    /// adds a redline to the current viewpoint (a new viewpoint is saved if the view changed)
    fn add_redline(&mut self, r: crate::markup::Redline) {
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

    fn report_step(&mut self, ctx: &egui::Context) {
        let Some(phase) = self.report_job.as_ref().map(|j| j.phase) else { return };
        ctx.request_repaint();
        match phase {
            0 => {
                let i = self.report_job.as_ref().map(|j| j.list.get(j.idx).copied().unwrap_or(0)).unwrap_or(0);
                self.restore_viewpoint(i);
                let j = self.report_job.as_mut().unwrap();
                j.phase = 1;
                j.wait = 4;
            }
            1 => {
                let j = self.report_job.as_mut().unwrap();
                if j.wait > 0 {
                    j.wait -= 1;
                } else {
                    j.phase = 2;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                }
            }
            _ => {
                let shot = ctx.input(|i| {
                    i.raw.events.iter().find_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                });
                // captura de ecran poate sa nu mai vina (fereastra minimizata / driver): nu asteptam la infinit
                if shot.is_none() {
                    let j = self.report_job.as_mut().unwrap();
                    j.shot_wait = j.shot_wait.saturating_add(1);
                    if j.shot_wait == 90 {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                    }
                    if j.shot_wait > 240 {
                        self.error("Viewpoint report stopped: the window did not return a screenshot (is it minimized?). The pages made so far are saved.");
                        self.finish_report();
                    }
                }
                if let Some(img) = shot.filter(|im| im.size[0] > 0 && im.size[1] > 0) {
                    if let Some(j) = self.report_job.as_mut() {
                        j.shot_wait = 0;
                    }
                    let ppp = ctx.pixels_per_point();
                    let [x, y, w, h] = self.vp_rect;
                    let (iw, ih) = (img.size[0] as i64, img.size[1] as i64);
                    let x0 = ((x * ppp).round() as i64).clamp(0, iw - 1);
                    let y0 = ((y * ppp).round() as i64).clamp(0, ih - 1);
                    let x1 = (((x + w) * ppp).round() as i64).clamp(x0 + 1, iw);
                    let y1 = (((y + h) * ppp).round() as i64).clamp(y0 + 1, ih);
                    let (cw, ch) = ((x1 - x0) as u32, (y1 - y0) as u32);
                    let mut rgb = Vec::with_capacity((cw * ch * 3) as usize);
                    for yy in y0..y1 {
                        for xx in x0..x1 {
                            let c = img.pixels[(yy * iw + xx) as usize];
                            rgb.extend_from_slice(&[c.r(), c.g(), c.b()]);
                        }
                    }
                    let (rgb, w2, h2) = crate::markup::downscale_rgb(&rgb, cw, ch, 1600);
                    let png = crate::markup::encode_png(&rgb, w2, h2);
                    let i = self.report_job.as_ref().map(|j| j.list.get(j.idx).copied().unwrap_or(0)).unwrap_or(0);
                    let item = self.report_item(i, png, w2, h2);
                    let n = self.report_job.as_ref().map(|j| j.list.len()).unwrap_or(0);
                    let Some(j) = self.report_job.as_mut() else { return };
                    // viewpoint sters intre timp: pagina lui se sare
                    if let Some(item) = item {
                        j.items.push(item);
                    }
                    j.idx += 1;
                    j.phase = 0;
                    if j.idx >= n {
                        self.finish_report();
                    }
                }
            }
        }
    }

    fn ribbon_redline(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Markup", |ui| {
            for k in crate::markup::RedKind::ALL {
                let on = self.tool == Tool::Redline(k);
                if rbtn(ui, k.icon(), k.label(), &format!("{} — saved with the current viewpoint", k.label()), on, true).clicked() {
                    self.cur_measure.clear();
                    self.tool = if on { Tool::Select } else { Tool::Redline(k) };
                }
            }
        });
        rgroup(ui, "Edit", |ui| {
            if rbtn(ui, "✖", "Erase", "Click a markup to delete it", self.tool == Tool::RedErase, true).clicked() {
                self.tool = if self.tool == Tool::RedErase { Tool::Select } else { Tool::RedErase };
            }
            let vp = self.viewpoints.get(self.vp_sel).map(|v| v.redlines.len()).unwrap_or(0);
            if rbtn(ui, "↶", "Undo", "Remove the last markup of this viewpoint (Ctrl+Z)", false, vp > 0).clicked() {
                if let Some(v) = self.viewpoints.get_mut(self.vp_sel) { v.redlines.pop(); }
            }
            if rbtn(ui, "🗑", "Clear", "Remove all markups of this viewpoint", false, vp > 0).clicked() {
                if let Some(v) = self.viewpoints.get_mut(self.vp_sel) { v.redlines.clear(); }
            }
        });
        rgroup(ui, "Style", |ui| {
            let col = rbtn(ui, "🎨", "Colour", "Markup colour and line weight", false, true);
            let cid = ui.make_persistent_id("red_col_pop");
            if col.clicked() { ui.memory_mut(|m| m.toggle_popup(cid)); }
            egui::popup_below_widget(ui, cid, &col, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(230.0);
                ui.label(RichText::new("Colour").small().strong());
                ui.horizontal_wrapped(|ui| {
                    // paletă standard de desen tehnic (roșu / portocaliu / galben / verde / albastru / negru / alb)
                    for c in [[230u8, 40, 40], [251, 146, 60], [250, 204, 21], [74, 222, 128], [59, 130, 246], [30, 35, 45], [250, 250, 250]] {
                        let (r, resp) = ui.allocate_exact_size(EV::splat(18.0), Sense::click());
                        ui.painter().rect_filled(r, crate::theme::RADIUS_SM, Color32::from_rgb(c[0], c[1], c[2]));
                        if self.red_color == c {
                            ui.painter().rect_stroke(r.expand(2.0), crate::theme::RADIUS_SM, Stroke::new(2.0_f32, accent()));
                        }
                        if resp.clicked() { self.red_color = c; }
                    }
                    ui.color_edit_button_srgb(&mut self.red_color);
                });
                ui.separator();
                ui.label(RichText::new("Line weight").small().strong());
                ui.horizontal(|ui| {
                    for (w, label) in [(1.5_f32, "Fine"), (3.0, "Medium"), (6.0, "Heavy")] {
                        let on = (self.red_width - w).abs() < 0.1;
                        if ui.selectable_label(on, label).clicked() { self.red_width = w; }
                    }
                });
                ui.add(egui::Slider::new(&mut self.red_width, 1.0..=10.0).text("Width"));
            });
        });
    }

    fn ribbon_issues(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Shared Issues", |ui| {
            let on = self.shared_dir.is_some();
            let tip = match &self.shared_dir {
                Some(d) => format!("Shared in: {d}\n{}\nClick to choose another folder", self.shared.status),
                None => "Share the viewpoints / issues with colleagues through a common folder\n(network drive, SharePoint or OneDrive synced folder)".to_string(),
            };
            if rbtn(ui, "🌐", if on { "Shared" } else { "Share…" }, &tip, on, true).clicked() {
                self.request_path(Purpose::SharedFolder);
            }
            if rbtn(ui, "⟳", "Sync", "Sync now with the shared folder (also automatic every few seconds)", false, on).clicked() {
                self.shared_sync(true);
            }
            if rbtn(ui, "⛔", "Stop", "Stop sharing (the files in the folder stay)", false, on).clicked() {
                self.shared_dir = None;
                self.shared = Default::default();
            }
        });
        rgroup(ui, "Viewpoints & Issues", |ui| {
            if rbtn(ui, "📷", "Save View", "Save the current view (with its markups) as a viewpoint", false, true).clicked() {
                self.save_viewpoint();
                self.open_panel(Panel::Viewpoints);
            }
            self.panel_btn(ui, Panel::Viewpoints, "📋", "Issues", "Viewpoints, markups, status, comments");
            let can = !self.viewpoints.is_empty();
            if rbtn(ui, "📄", "HTML", "Report of all viewpoints with snapshots and markups (HTML)", false, can).clicked() {
                self.request_path(Purpose::ExportVpHtml);
            }
            if rbtn(ui, "📊", "Excel", "Excel workbook with one row and a snapshot per viewpoint", false, can).clicked() {
                self.request_path(Purpose::ExportVpXlsx);
            }
            if rbtn(ui, "📄", "PDF Sheets", "One A3 sheet per viewpoint: large image, description, comments and a title block (project, viewpoint, status, date, sheet number)", false, can).clicked() {
                self.request_path(Purpose::ExportVpPdf);
            }
            if rbtn(ui, "✏", "Title Block", "Edit the title block and the sheet format (A3 - A0) used by the PDF sheets", self.title_block_open, true).clicked() {
                self.title_block_open = !self.title_block_open;
            }
            if rbtn(ui, "fmt:BCF", "BCF", "BCF 2.1 issues (Revit, Navisworks, Solibri, BIMcollab, ACC...)", false, can).clicked() {
                self.request_path(Purpose::ExportBcf);
            }
            if rbtn(ui, "🧭", "Navisworks", "Viewpoints with markups, comments and section box as Navisworks XML\n(in Navisworks: Saved Viewpoints > right-click > Import Viewpoints)", false, can).clicked() {
                self.request_path(Purpose::ExportNavisXml);
            }
        });
    }

    /// Selectie cu fereastra: `crossing` = tot ce atinge dreptunghiul, altfel doar ce e complet in interior.
    /// Doar elementele vizibile; `add` (Ctrl) pastreaza selectia existenta.
    fn select_in_window(&mut self, vp: &Mat4, view: Rect, sel: Rect, crossing: bool, add: bool) {
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
                    corners.iter().all(|c| c.map_or(false, |p| sel.contains(p)))
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

    /// Nimic deschis si nimic in curs de incarcare: se arata pagina de start.
    fn start_screen(&self) -> bool {
        self.loaded_models().next().is_none() && self.loads.is_empty() && self.project_path.is_none() && !self.blank_project && self.bimd_path.is_none() || self.show_start
    }

    // ================================================================== pagina de start
    /// Ecranul de pornire, cand nu e deschis niciun model: Open / Open Project / New si fisierele recente
    /// ca niste carduri (tip, nume, data modificarii); click deschide, click dreapta scoate din lista.
    fn start_page(&mut self, ui: &mut egui::Ui) {
        use crate::app::StartAction;
        let rect = ui.max_rect();
        let pal = self.pal();
        let side_w = 230.0f32.min(rect.width() * 0.35);
        let side = Rect::from_min_size(rect.min, EV::new(side_w, rect.height()));
        let dark = self.settings.theme.is_dark();
        let side_fill = if dark { Color32::from_black_alpha(70) } else { Color32::from_white_alpha(150) };
        ui.painter().rect_filled(side, 0.0, side_fill);
        // ---- brand: logo isometric + titlu + tagline (card subtile, nu text gol)
        {
            let p = ui.painter();
            let hero = Rect::from_min_size(side.min + EV::new(18.0, 16.0), EV::new(side_w - 36.0, 82.0));
            p.rect_filled(hero, crate::theme::RADIUS_LG, if dark { Color32::from_white_alpha(14) } else { Color32::from_white_alpha(225) });
            p.rect_stroke(hero, crate::theme::RADIUS_LG, egui::Stroke::new(1.0_f32, crate::theme::border()));
            let tile = Rect::from_center_size(Pos2::new(hero.min.x + 37.0, hero.center().y), EV::new(42.0, 42.0));
            p.rect_filled(tile, crate::theme::RADIUS_MD, accent());
            let c = tile.center();
            let s = 11.0;
            let top = [Pos2::new(c.x, c.y - s), Pos2::new(c.x + s, c.y - s * 0.5), Pos2::new(c.x, c.y), Pos2::new(c.x - s, c.y - s * 0.5)];
            let left = [top[3], top[2], Pos2::new(c.x, c.y + s), Pos2::new(c.x - s, c.y + s * 0.5)];
            let right = [top[2], top[1], Pos2::new(c.x + s, c.y + s * 0.5), Pos2::new(c.x, c.y + s)];
            let white = Color32::from_white_alpha(235);
            p.add(egui::Shape::convex_polygon(top.to_vec(), Color32::from_white_alpha(245), Stroke::NONE));
            p.add(egui::Shape::convex_polygon(left.to_vec(), white.gamma_multiply(0.65), Stroke::NONE));
            p.add(egui::Shape::convex_polygon(right.to_vec(), white.gamma_multiply(0.85), Stroke::NONE));
            p.text(Pos2::new(hero.min.x + 70.0, hero.min.y + 31.0), Align2::LEFT_CENTER, "ManageBIM", FontId::proportional(19.0), accent());
            p.text(Pos2::new(hero.min.x + 70.0, hero.min.y + 52.0), Align2::LEFT_CENTER, "Coordinate · Check · Deliver", FontId::proportional(10.5), pal.overlay_weak);
        }
        let mut act: Option<StartAction> = None;
        let mut forget: Option<String> = None;
        let open_name = self.bimd_path.as_ref().or(self.project_path.as_ref()).map(|p| crate::file_name(p)).or_else(|| self.loaded_models().next().map(|(_, m)| m.name.clone())).or_else(|| self.blank_project.then(|| "New project".to_string()));
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(side.shrink2(EV::new(18.0, 18.0))), |ui| {
            // ca Navisworks: sageata inapoi la proiectul deschis
            if self.show_start {
                if let Some(n) = &open_name {
                    if ui.add(egui::Button::new(RichText::new(format!("⬅  {n}")).size(13.0)).frame(false)).on_hover_text("Back to the open project").clicked() {
                        self.show_start = false;
                    }
                    ui.add_space(8.0);
                }
            }
            ui.add_space(98.0);
            ui.label(RichText::new(format!("v{} · BIM coordination workspace", env!("CARGO_PKG_VERSION"))).small().color(pal.overlay_weak));
            ui.add_space(12.0);
            let bw = side_w - 36.0;
            // iconita vectoriala + text (emoji-urile nu au glife in fontul interfetei)
            let tcol = ui.visuals().text_color();
            let on_accent = match self.settings.theme { Theme::Nord | Theme::TokyoNight => Color32::from_rgb(30, 36, 48), _ => Color32::WHITE };
            let soft = if dark { Color32::from_white_alpha(16) } else { Color32::from_white_alpha(235) };
            let big = |t: &str| egui::Button::new(crate::theme::title_job(t, 14.0, tcol)).min_size(EV::new(bw, 34.0)).fill(soft).rounding(8.0);
            // Open… cu meniu (model / proiect), ca butonul Open din Navisworks
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let primary = egui::Button::new(crate::theme::title_job("📂 Open…", 14.0, on_accent)).fill(accent()).rounding(8.0).min_size(EV::new(bw - 34.0, 34.0));
                if ui.add(primary).on_hover_text("Open a model or a project (IFC, Revit / Navisworks, DWG, STEP, FBX, E57, .bimr, .bimd…)  Ctrl+O").clicked() {
                    act = Some(StartAction::OpenModelDialog);
                }
                ui.menu_button(RichText::new("⏷").size(14.0), |ui| {
                    if ui.button("📂 Open model…").clicked() {
                        act = Some(StartAction::OpenModelDialog);
                        ui.close_menu();
                    }
                    if ui.button("🗂 Open project (.bimr / .bimd)…").clicked() {
                        act = Some(StartAction::OpenProjectDialog);
                        ui.close_menu();
                    }
                });
            });
            ui.add_space(4.0);
            if ui.add(big("➕ New")).on_hover_text("New empty project: add models, grids, levels; save it with Save (Ctrl+S)").clicked() {
                act = Some(StartAction::New);
            }
            if open_name.is_some() && self.show_start {
                ui.add_space(4.0);
                if ui.add(big("✖ Close project")).clicked() {
                    act = Some(StartAction::Close);
                }
            }
            ui.add_space(4.0);
            if ui.add(big("🏗 Demo project")).on_hover_text("Block A: architecture + structure, MEP and a 4D schedule - try every feature").clicked() {
                self.open_demo();
            }
            if ui.add(big("❓ Manual")).on_hover_text("The ManageBIM manual, with search").clicked() {
                self.manual_open = true;
            }
            ui.add_space(16.0);
            ui.label(RichText::new("or drag files into the window  ·  Ctrl+K: commands").small().color(pal.overlay_weak));
            // recuperare dupa o inchidere neasteptata
            if let Some((_, orig, when)) = self.autosave.recover.clone() {
                ui.add_space(20.0);
                egui::Frame::none().fill(Color32::from_rgba_unmultiplied(235, 170, 60, 40)).stroke(Stroke::new(1.0_f32, Color32::from_rgb(235, 170, 60))).rounding(6.0).inner_margin(8.0).show(ui, |ui| {
                    ui.label(RichText::new("⚠ ManageBIM was closed unexpectedly").strong());
                    ui.label(RichText::new(format!("Autosaved {when}{}", orig.as_deref().map(|p| format!(" - {}", crate::file_name(p))).unwrap_or_default())).small());
                    ui.horizontal(|ui| {
                        if ui.button("Recover").clicked() {
                            self.autosave_recover();
                        }
                        if ui.button("Discard").clicked() {
                            self.autosave_discard();
                        }
                    });
                });
            }
        });
        // ---- Recent: lista / grila, sortare, cautare
        let area = Rect::from_min_max(Pos2::new(side.right() + 28.0, rect.top() + 18.0), Pos2::new(rect.right() - 20.0, rect.bottom() - 10.0));
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(area), |ui| {
            let soft = if dark { Color32::from_white_alpha(16) } else { Color32::from_white_alpha(235) };
            ui.horizontal(|ui| {
                ui.label(RichText::new("Recent").size(20.0).strong().color(pal.overlay_text));
                ui.add_space(14.0);
                if ui.selectable_label(self.start_list, RichText::new("☰").size(15.0)).on_hover_text("List").clicked() {
                    self.start_list = true;
                }
                if ui.selectable_label(!self.start_list, RichText::new("▦").size(15.0)).on_hover_text("Tiles").clicked() {
                    self.start_list = false;
                }
                ui.add_space(10.0);
                ui.label("Sort by");
                const SORTS: [&str; 4] = ["Last opened", "Name", "Date modified", "Type"];
                egui::ComboBox::from_id_salt("start_sort").width(120.0).selected_text(SORTS[self.start_sort.min(3) as usize]).show_ui(ui, |ui| {
                    for (i, s) in SORTS.iter().enumerate() {
                        ui.selectable_value(&mut self.start_sort, i as u8, *s);
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.settings.recent.is_empty() && ui.small_button("Clear list").clicked() {
                        self.settings.recent.clear();
                        self.settings.save();
                    }
                    ui.add(egui::TextEdit::singleline(&mut self.start_search).hint_text("🔍 Search").desired_width(180.0));
                });
            });
            ui.add_space(8.0);
            let q = self.start_search.trim().to_lowercase();
            let mut recent: Vec<(usize, String, Option<std::time::SystemTime>)> = self
                .settings
                .recent
                .iter()
                .enumerate()
                .filter(|(_, p)| q.is_empty() || p.to_lowercase().contains(&q))
                .map(|(i, p)| (i, p.clone(), std::fs::metadata(p).and_then(|m| m.modified()).ok()))
                .collect();
            match self.start_sort {
                1 => recent.sort_by(|a, b| crate::app::natural_cmp(&crate::file_name(&a.1).to_lowercase(), &crate::file_name(&b.1).to_lowercase())),
                2 => recent.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0))),
                3 => recent.sort_by(|a, b| file_badge(&a.1).0.cmp(file_badge(&b.1).0).then(a.0.cmp(&b.0))),
                _ => {}
            }
            if recent.is_empty() {
                if !self.settings.recent.is_empty() {
                    ui.label(RichText::new("Nothing matches the search.").color(pal.overlay_weak));
                    return;
                }
                // pagina de pornire pentru un utilizator nou: clar, prietenos, cu actiuni reale
                ui.add_space((area.height() - 330.0).max(24.0));
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Welcome to ManageBIM").size(30.0).strong().color(pal.overlay_text));
                    ui.add_space(8.0);
                    ui.label(RichText::new("Open a federated BIM model, or explore the full workflow with the demo project.").size(14.0).color(pal.overlay_weak));
                    ui.add_space(28.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = EV::new(12.0, 12.0);
                        let card = |ui: &mut egui::Ui, glyph: &str, title: &str, sub: &str| {
                            let txt = format!("{glyph}\n{title}\n{sub}");
                            ui.add(egui::Button::new(RichText::new(txt).size(13.5)).min_size(EV::new(178.0, 90.0)).fill(soft).rounding(10.0)).clicked()
                        };
                        if card(ui, "📂", "Open a model", "IFC · DWG · STEP · E57…") {
                            act = Some(StartAction::OpenModelDialog);
                        }
                        if card(ui, "🏗", "Try the demo", "Clash · 4D · export") {
                            self.open_demo();
                        }
                        if card(ui, "⌨", "Commands", "Search or press Ctrl+K") {
                            self.palette_open = true;
                        }
                    });
                    ui.add_space(20.0);
                    ui.label(RichText::new("Tip: drag & drop files anywhere in the window.").small().color(pal.overlay_weak));
                });
                return;
            }
            egui::ScrollArea::vertical().id_salt("start_recent").auto_shrink([false, false]).show(ui, |ui| {
                let mut card = |ui: &mut egui::Ui, p: &String, modified: Option<std::time::SystemTime>, size: EV| {
                    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
                    let exists = std::path::Path::new(p).exists();
                    let hov = resp.hovered() && exists;
                    let bg = if dark { Color32::from_white_alpha(if hov { 26 } else { 14 }) } else { Color32::from_white_alpha(if hov { 255 } else { 215 }) };
                    let painter = ui.painter();
                    painter.rect(r, 6.0, bg, Stroke::new(1.0_f32, if hov { accent() } else { Color32::from_white_alpha(if dark { 22 } else { 0 }) }));
                    let (tag, col) = file_badge(p);
                    let col = if exists { col } else { col.gamma_multiply(0.35) };
                    let name = crate::file_name(p);
                    let tc = if exists { pal.overlay_text } else { pal.overlay_weak };
                    let date = if exists { modified.map(fmt_date).unwrap_or_default() } else { "file not found".into() };
                    if self.start_list {
                        // rand: iconita mica, nume, folder, data
                        let icon = Rect::from_min_size(r.min + EV::new(8.0, 5.0), EV::new(26.0, 26.0));
                        painter.rect_filled(icon, 4.0, col);
                        painter.text(icon.center(), Align2::CENTER_CENTER, tag, FontId::proportional(if tag.len() > 3 { 8.0 } else { 10.0 }), Color32::WHITE);
                        painter.text(Pos2::new(r.left() + 44.0, r.center().y), Align2::LEFT_CENTER, &name, FontId::proportional(13.0), tc);
                        let folder = std::path::Path::new(p).parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
                        painter.text(Pos2::new(r.left() + 360.0, r.center().y), Align2::LEFT_CENTER, folder, FontId::proportional(11.0), pal.overlay_weak);
                        painter.text(Pos2::new(r.right() - 10.0, r.center().y), Align2::RIGHT_CENTER, date, FontId::proportional(11.0), pal.overlay_weak);
                    } else {
                        let icon = Rect::from_center_size(Pos2::new(r.center().x, r.top() + 50.0), EV::new(64.0, 64.0));
                        painter.rect_filled(icon.translate(EV::new(5.0, 5.0)), 6.0, col.gamma_multiply(0.55));
                        painter.rect_filled(icon, 6.0, col);
                        painter.text(icon.center(), Align2::CENTER_CENTER, tag, FontId::proportional(if tag.len() > 3 { 15.0 } else { 19.0 }), Color32::WHITE);
                        // iconita mica de tip langa nume, ca in Navisworks
                        let small = Rect::from_min_size(Pos2::new(r.left() + 10.0, r.bottom() - 48.0), EV::new(14.0, 14.0));
                        painter.rect_filled(small, 2.0, col);
                        let short: String = if name.chars().count() > 18 { format!("{}…", name.chars().take(17).collect::<String>()) } else { name.clone() };
                        painter.text(Pos2::new(r.left() + 30.0, r.bottom() - 41.0), Align2::LEFT_CENTER, short, FontId::proportional(12.5), tc);
                        painter.text(Pos2::new(r.left() + 10.0, r.bottom() - 20.0), Align2::LEFT_CENTER, date, FontId::proportional(10.5), pal.overlay_weak);
                    }
                    let resp = resp.on_hover_text(p.as_str());
                    if resp.clicked() && exists {
                        act = Some(StartAction::Open(p.clone()));
                    }
                    resp.context_menu(|ui| {
                        if exists && ui.button("Open").clicked() {
                            act = Some(StartAction::Open(p.clone()));
                            ui.close_menu();
                        }
                        if cfg!(windows) && exists && ui.button("Show in folder").clicked() {
                            Self::open_folder(p);
                            ui.close_menu();
                        }
                        if ui.button("Remove from list").clicked() {
                            forget = Some(p.clone());
                            ui.close_menu();
                        }
                    });
                };
                if self.start_list {
                    let w = ui.available_width();
                    for (_, p, m) in &recent {
                        card(ui, p, *m, EV::new(w, 36.0));
                        ui.add_space(2.0);
                    }
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = EV::new(12.0, 12.0);
                        for (_, p, m) in &recent {
                            card(ui, p, *m, EV::new(150.0, 150.0));
                        }
                    });
                }
            });
        });
        if let Some(p) = forget {
            self.settings.recent.retain(|x| x != &p);
            self.settings.save();
        }
        if let Some(a) = act {
            self.start_action(a);
        }
    }
    // ================================================================== ViewCube
    fn view_cube(&mut self, ui: &mut egui::Ui, rect: Rect) {
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

    // ================================================================== ferestre aranjabile
    fn panel_menu(&mut self, ui: &mut egui::Ui, p: Panel) {
        let cur = self.settings.layout.get(p).map(|s| s.dock);
        for d in [Dock::Left, Dock::Right, Dock::Bottom, Dock::Float] {
            if Some(d) != cur && ui.button(d.label()).clicked() {
                self.settings.layout.move_to(p, d);
                ui.close_menu();
            }
        }
        if p == Panel::Clash && ui.button("⛶ Focus mode").clicked() {
            self.settings.layout.open(p);
            self.clash_focus_mode = true;
            ui.close_menu();
        }
        ui.separator();
        if ui.button("✖ Close").clicked() {
            self.settings.layout.close(p);
            ui.close_menu();
        }
    }

    fn panel_body(&mut self, ui: &mut egui::Ui, p: Panel) {
        match p {
            Panel::Tree => {
                // cautare in model (nume, tip, nivel, GUID): rezultatele inlocuiesc arborele cat timp se cauta
                let sid = ui.make_persistent_id("tree_search");
                let mut q: String = ui.data_mut(|d| d.get_temp(sid)).unwrap_or_default();
                ui.add(egui::TextEdit::singleline(&mut q).hint_text("🔍 Search model").desired_width(f32::INFINITY));
                ui.data_mut(|d| d.insert_temp(sid, q.clone()));
                let ql = q.trim().to_lowercase();
                if ql.is_empty() {
                    self.tree_config_ui(ui);
                    let acts = egui::ScrollArea::both().id_salt("tree_scroll").auto_shrink([false, false]).show(ui, |ui| self.tree_ui(ui)).inner;
                    for a in acts {
                        self.apply(a);
                    }
                } else {
                    let mut hits: Vec<(ElemId, String, String)> = Vec::new();
                    let mut total = 0usize;
                    for (mi, m) in self.loaded_models() {
                        for (ei, e) in m.elements.iter().enumerate() {
                            if e.name.to_lowercase().contains(&ql) || e.ifc_type.to_lowercase().contains(&ql) || e.storey.to_lowercase().contains(&ql) || e.guid.to_lowercase() == ql {
                                total += 1;
                                if hits.len() < 500 {
                                    hits.push((ElemId { m: mi as u32, e: ei as u32 }, e.name.clone(), format!("{} · {}", e.ifc_type, e.storey)));
                                }
                            }
                        }
                    }
                    let mut act: Option<Action> = None;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{total} found")).weak());
                        if total > 0 && ui.small_button("Select all").clicked() {
                            let all: Vec<ElemId> = self.loaded_models().flat_map(|(mi, m)| m.elements.iter().enumerate().filter(|(_, e)| e.name.to_lowercase().contains(&ql) || e.ifc_type.to_lowercase().contains(&ql) || e.storey.to_lowercase().contains(&ql) || e.guid.to_lowercase() == ql).map(move |(ei, _)| ElemId { m: mi as u32, e: ei as u32 }).collect::<Vec<_>>()).collect();
                            act = Some(Action::Select(all, false));
                        }
                    });
                    egui::ScrollArea::vertical().id_salt("tree_hits").auto_shrink([false, false]).show(ui, |ui| {
                        for (id, n, sub) in &hits {
                            let on = self.selected.contains(id);
                            let r = ui.selectable_label(on, RichText::new(n)).on_hover_text(sub.as_str());
                            if r.clicked() {
                                act = Some(Action::Select(vec![*id], ui.input(|i| i.modifiers.command)));
                            }
                            if r.double_clicked() {
                                act = Some(Action::Focus(vec![*id]));
                            }
                        }
                    });
                    if let Some(a) = act {
                        self.apply(a);
                    }
                }
            }
            Panel::PropTree => self.props_tree_ui(ui),
            Panel::Sets => self.sets_ui(ui),
            Panel::Properties => self.properties_panel(ui),
            Panel::Find => self.find_items_ui(ui),
            Panel::Clash => self.clash_ui(ui),
            Panel::TimeLiner => self.timeliner_ui(ui),
            Panel::Viewpoints => self.viewpoints_ui(ui),
            Panel::Section => self.section_ui(ui),
            Panel::Measure => self.measure_ui(ui),
            Panel::Coords => self.coords_ui(ui),
            Panel::Render => self.render_ui(ui),
            Panel::Animation => self.animations_ui(ui),
            Panel::Project => self.models_ui(ui),
            Panel::Profiler => self.profiler_ui(ui),
            Panel::Compare => self.compare_ui(ui),
            Panel::Check => self.check_ui(ui),
            Panel::Quantities => self.qto_ui(ui),
        }
    }

    /// Banda unei zone in auto-hide: cate un buton pe panou (iconita); hover / clic deschide panoul peste model.
    fn dock_strip(&mut self, ui: &mut egui::Ui, d: Dock) {
        let tabs = self.settings.layout.in_area(d);
        let show = |ui: &mut egui::Ui, p: Panel, this: &mut Self| {
            let icon = crate::theme::ph(p.title().split_whitespace().next().unwrap_or("•")).to_string();
            let on = this.flyout.map(|f| f.1 == p).unwrap_or(false);
            let r = ui.add(egui::Button::new(RichText::new(icon).font(crate::theme::icon_font(15.0))).selected(on).min_size(EV::new(24.0, 24.0))).on_hover_text(p.title().trim_start_matches(|c: char| !c.is_alphanumeric()).to_string());
            if r.hovered() || r.clicked() {
                this.flyout = Some((d, p));
                this.flyout_seen = Instant::now();
            }
        };
        if d == Dock::Bottom {
            ui.horizontal(|ui| {
                for p in tabs {
                    show(ui, p, self);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("📌").on_hover_text("Pin the panels (always visible)").clicked() {
                        self.settings.layout.autohide[d.idx()] = false;
                        self.flyout = None;
                    }
                });
            });
        } else {
            ui.vertical_centered(|ui| {
                if ui.small_button("📌").on_hover_text("Pin the panels (always visible)").clicked() {
                    self.settings.layout.autohide[d.idx()] = false;
                    self.flyout = None;
                }
                ui.add_space(6.0);
                for p in tabs {
                    show(ui, p, self);
                }
            });
        }
    }

    /// Panoul deschis peste model dintr-o zona auto-hide; se inchide cand mouse-ul pleaca (sau Esc / clic in model).
    fn flyout_ui(&mut self, ctx: &egui::Context, strips: [Option<Rect>; 3]) {
        let Some((d, p)) = self.flyout else { return };
        let Some(strip) = strips[d.idx()] else {
            self.flyout = None;
            return;
        };
        let screen = ctx.screen_rect();
        let (pos, size) = match d {
            Dock::Left => (strip.right_top(), EV::new(320.0, strip.height())),
            Dock::Right => (Pos2::new(strip.left() - 340.0, strip.top()), EV::new(340.0, strip.height())),
            _ => {
                let h = (screen.height() * 0.38).clamp(200.0, 420.0);
                (Pos2::new(strip.left(), strip.top() - h), EV::new(strip.width(), h))
            }
        };
        let fr = egui::Area::new(egui::Id::new("flyout")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            egui::Frame::window(&ctx.style()).rounding(6.0).inner_margin(8.0).show(ui, |ui| {
                ui.set_min_size(size - EV::splat(16.0));
                ui.set_max_size(size - EV::splat(16.0));
                ui.horizontal(|ui| {
                    ui.label(crate::theme::title_job(p.title(), 14.0, ui.visuals().strong_text_color()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✖").on_hover_text("Hide").clicked() {
                            self.flyout = None;
                        }
                        if ui.small_button("📌").on_hover_text("Pin this area (panels always visible)").clicked() {
                            self.settings.layout.autohide[d.idx()] = false;
                            self.settings.layout.active[d.idx()] = Some(p);
                            self.flyout = None;
                        }
                    });
                });
                ui.separator();
                egui::ScrollArea::both().id_salt(("flyout_scroll", p)).auto_shrink([false, false]).show(ui, |ui| self.panel_body(ui, p));
            });
        });
        let area = fr.response.rect;
        // ramane deschis cat mouse-ul e pe panou / pe banda, cat se tasteaza sau se trage ceva
        let hover = ctx.pointer_hover_pos().map_or(false, |h| area.expand(6.0).contains(h) || strip.contains(h));
        let busy = ctx.wants_keyboard_input() || ctx.is_using_pointer() || ctx.memory(|m| m.any_popup_open());
        if hover || busy {
            self.flyout_seen = Instant::now();
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) || self.flyout_seen.elapsed().as_secs_f32() > 0.6 {
            self.flyout = None;
        }
        if self.flyout.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    fn dock_area(&mut self, ui: &mut egui::Ui, d: Dock) {
        let tabs = self.settings.layout.in_area(d);
        let active = self.settings.layout.active[d.idx()];
        if d == Dock::Bottom {
            // zona de jos: filele sunt in bara de stare; aici doar titlul si comenzile ferestrei
            ui.horizontal(|ui| {
                if let Some(a) = active {
                    ui.label(crate::theme::title_job(a.title(), 14.0, ui.visuals().strong_text_color()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let tall = self.settings.layout.bottom_tall;
                        let glyph = if tall { "⤡" } else { "⤢" };
                        if ui.add(egui::Button::new(RichText::new(glyph).size(16.0)).frame(false)).on_hover_text(if tall { "Restore the panel height" } else { "Maximize this panel (more room for clashes and tests)" }).clicked() {
                            self.settings.layout.bottom_tall = !tall;
                            self.settings.save();
                        }
                        if a == Panel::Clash && ui.button("⛶ Focus").on_hover_text("Open Clash Detective in a large focus window (Esc to exit)").clicked() {
                            self.settings.layout.open(Panel::Clash);
                            self.clash_focus_mode = true;
                        }
                        if ui.button("⧉ Float").on_hover_text("Open this panel in a large resizable floating window").clicked() {
                            self.settings.layout.move_to(a, Dock::Float);
                            ui.close_menu();
                        }
                        if ui.add(egui::Button::new(RichText::new(egui_phosphor::regular::CARET_DOWN).font(crate::theme::icon_font(15.0))).frame(false)).on_hover_text("Lower the panel (click its tab in the status bar to raise it again)").clicked() {
                            self.settings.layout.bottom_open = false;
                        }
                        if ui.add(egui::Button::new(RichText::new(egui_phosphor::regular::X).font(crate::theme::icon_font(14.0))).frame(false)).on_hover_text("Close this window").clicked() {
                            self.settings.layout.close(a);
                        }
                        ui.menu_button(RichText::new(egui_phosphor::regular::DOTS_THREE).font(crate::theme::icon_font(15.0)), |ui| self.panel_menu(ui, a)).response.on_hover_text("Move this window (dock left / right, float)");
                    });
                }
            });
            ui.add_space(2.0);
        } else if d == Dock::Right {
            // Panoul din dreapta e doar Properties: titlul sus si continutul pe toata inaltimea.
            if let Some(a) = active {
                ui.horizontal(|ui| {
                    ui.label(crate::theme::title_job(a.title(), 14.0, ui.visuals().strong_text_color()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.menu_button(RichText::new(egui_phosphor::regular::DOTS_THREE).font(crate::theme::icon_font(15.0)), |ui| self.panel_menu(ui, a));
                    });
                });
                ui.add_space(3.0);
                egui::ScrollArea::vertical().id_salt(("side_scroll", d.idx())).auto_shrink([false, false]).show(ui, |ui| self.panel_body(ui, a));
            }
            return;
        } else {
        ui.horizontal_wrapped(|ui| {
            ui.style_mut().spacing.item_spacing.x = 3.0;
            // auto-hide: zona devine o banda subtire, panourile apar peste model cand le atingi
            if ui.small_button("📌").on_hover_text("Auto-hide this area: panels slide in over the model when you point at their tab").clicked() {
                self.settings.layout.autohide[d.idx()] = true;
            }
            // file ca un control segmentat (card cu "pastile"): fara contur, activa = suprafata mai deschisa
            let seg_on = ui.visuals().widgets.hovered.weak_bg_fill;
            egui::Frame::none().fill(card_fill(ui.visuals())).rounding(8.0).inner_margin(3.0).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for p in &tabs {
                        let on = Some(*p) == active;
                        // pe zona de jos ramane si iconita; lateral doar numele (mai compact)
                        let t = p.short().to_string();
                        let txt = if on { RichText::new(t).size(13.0).strong() } else { RichText::new(t).size(13.0).color(ui.visuals().weak_text_color()) };
                        ui.spacing_mut().button_padding = egui::vec2(7.0, 2.0);
                        let b = egui::Button::new(txt).fill(if on { seg_on } else { Color32::TRANSPARENT }).stroke(Stroke::NONE).rounding(6.0);
                        let r = ui.add(b).on_hover_text(format!("{}\nRight-click: dock left / right / bottom, float or close", p.title()));
                        if r.clicked() {
                            self.settings.layout.active[d.idx()] = Some(*p);
                        }
                        r.context_menu(|ui| self.panel_menu(ui, *p));
                    }
                });
            });
            if d == Dock::Right {
                ui.menu_button("＋ Add tab", |ui| {
                    ui.label(RichText::new("Bring a panel into the Properties bar as a tab").small().weak());
                    ui.separator();
                    for p in crate::layout::ALL_PANELS {
                        if !tabs.contains(&p) && ui.button(p.title()).clicked() {
                            self.settings.layout.move_to(p, Dock::Right);
                            ui.close_menu();
                        }
                    }
                }).response.on_hover_text("Add any panel here (Blender-style properties tabs)");
            }
            if let Some(a) = active {
                ui.add_space(4.0);
                ui.menu_button("⏷", |ui| self.panel_menu(ui, a)).response.on_hover_text("Move or close this window");
                // lateral: inchiderea e in meniul ⏷ (filele raman pe un singur rand)
            }
        });
        ui.add_space(4.0);
        }
        if let Some(a) = self.settings.layout.active[d.idx()] {
            if d == Dock::Bottom {
                // continutul nu are voie sa tina panoul inalt: limitat la inaltimea aleasa (se poate trage liber)
                let h = ui.available_height().max(40.0);
                let size = EV::new(ui.available_width(), h);
                ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_max_height(h);
                    let clip = ui.max_rect().intersect(ui.clip_rect());
                    ui.set_clip_rect(clip);
                    // panourile fara derulare proprie: continutul inalt se deruleaza in panou,
                    // nu mai impinge panoul de jos peste vederea 3D
                    if matches!(a, Panel::Project | Panel::Section | Panel::Measure | Panel::Coords | Panel::Profiler | Panel::Compare) {
                        egui::ScrollArea::vertical().id_salt(("bottom_scroll", a)).max_height(h).auto_shrink([false, false]).show(ui, |ui| self.panel_body(ui, a));
                    } else {
                        self.panel_body(ui, a);
                    }
                });
            } else {
                // panourile laterale nu au voie sa impinga viewport-ul: continutul lat se deruleaza
                egui::ScrollArea::horizontal().id_salt(("side_scroll", d.idx())).auto_shrink([false, false]).show(ui, |ui| self.panel_body(ui, a));
            }
        }
    }

    /// Model focus: doar un mic buton de iesire peste model; Esc restaureaza layout-ul.
    fn immersive_overlay(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.toggle_immersive();
            return;
        }
        egui::Area::new(egui::Id::new("immersive_exit"))
            .anchor(Align2::LEFT_TOP, EV::new(10.0, 10.0))
            .show(ctx, |ui| {
                let b = egui::Button::new(RichText::new("⛶ Exit model focus · Esc").size(12.0))
                    .fill(accent().gamma_multiply(0.35))
                    .rounding(8.0);
                if ui.add(b).clicked() {
                    self.toggle_immersive();
                }
            });
    }

    /// Clash Detective in mod Focus: o fereastra aproape cat ecranul, cu spatiu pentru multe teste / clashuri.
    fn clash_focus_window(&mut self, ctx: &egui::Context) {
        if !self.clash_focus_mode {
            return;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.clash_focus_mode = false;
            return;
        }
        let screen = ctx.screen_rect();
        let m = 18.0_f32;
        let size = EV::new((screen.width() - 2.0 * m).max(700.0), (screen.height() - 2.0 * m).max(500.0));
        let mut open = true;
        egui::Window::new(RichText::new("⚠ Clash Detective — Focus mode").strong().color(accent()))
            .id(egui::Id::new("clash_focus_window"))
            .open(&mut open)
            .default_pos(screen.min + EV::new(m, m))
            .fixed_size(size)
            .movable(false)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Focus mode · press Esc to return to the model").weak().small());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✖ Exit focus").clicked() {
                            self.clash_focus_mode = false;
                        }
                    });
                });
                ui.separator();
                self.clash_ui(ui);
            });
        if !open {
            self.clash_focus_mode = false;
        }
    }

    fn ribbon_script(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Scripts", |ui| {
            if rbtn(ui, "📜", "Console", "Open the script console: list, create and manage scripts", self.script_open, true).clicked() {
                self.script_open = !self.script_open;
            }
            if rbtn(ui, "➕", "New Script", "Create a new .rhai script in the ManageBIM Scripts folder", false, true).clicked() {
                match script::create(&self.settings.scripts_dir) {
                    Ok(p) => self.toast(format!("Script created: {}", crate::file_name(&p.to_string_lossy().to_string()))),
                    Err(e) => self.error(e),
                }
            }
            if rbtn(ui, "📂", "Folder", "Copy the Scripts folder path to the clipboard", false, true).clicked() {
                ui.ctx().copy_text(script::dir(&self.settings.scripts_dir).to_string_lossy().to_string());
                self.toast("Scripts folder path copied to clipboard.");
            }
            if rbtn(ui, "❓", "API Help", "Show the scripting API reference and examples", self.script_help_open, true).clicked() {
                self.script_help_open = !self.script_help_open;
                self.script_open = true;
            }
        });
        rgroup(ui, "Run", |ui| {
            let scripts = script::list(&self.settings.scripts_dir);
            let n = scripts.len();
            ui.label(format!("{n} script(s) in folder"));
            if ui.button("▶ Run all").clicked() {
                for p in scripts {
                    self.run_script(&p.to_string_lossy().to_string());
                }
            }
        });
    }


    fn float_windows(&mut self, ctx: &egui::Context) {
        for p in self.settings.layout.in_area(Dock::Float) {
            let mut open = true;
            let max = self.float_max.contains(&p);
            let screen = ctx.screen_rect();
            let size = if max {
                [(screen.width() - 40.0).max(640.0), (screen.height() - 90.0).max(480.0)]
            } else if p == Panel::Clash {
                // nu mai mare decat ~60% din ecran: vederea 3D ramane vizibila
                [1100.0f32.min(screen.width() * 0.62), 760.0f32.min(screen.height() * 0.6)]
            } else {
                [560.0f32.min(screen.width() * 0.5), 400.0f32.min(screen.height() * 0.5)]
            };
            // id-ul include starea: la toggle egui recreaza fereastra cu dimensiunea noua
            egui::Window::new(crate::theme::title_job(p.title(), 14.0, ctx.style().visuals.text_color()))
                .id(egui::Id::new(("float_panel", p, max)))
                .open(&mut open)
                .default_size(size)
                .min_width(260.0)
                .min_height(160.0)
                .resizable(true)
                .collapsible(true)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let lbl = if max { "⤡ Restore size" } else { "⤢ Stretch to screen" };
                        if ui.button(lbl).on_hover_text("Maximize the window; drag any edge or corner to stretch it").clicked() {
                            if max {
                                self.float_max.remove(&p);
                            } else {
                                self.float_max.insert(p);
                            }
                        }
                        ui.menu_button("⏷ Dock…", |ui| self.panel_menu(ui, p));
                    });
                    // continutul nu are voie sa mareasca fereastra peste ecran (tabele lungi, liste):
                    // ramane in marimea ferestrei, care se poate trage de margini
                    let h = ui.available_height().clamp(80.0, (screen.height() - 120.0).max(200.0));
                    let size = EV::new(ui.available_width(), h);
                    ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_max_height(h);
                        let clip = ui.max_rect().intersect(ui.clip_rect());
                        ui.set_clip_rect(clip);
                        self.panel_body(ui, p);
                    });
                });
            if !open {
                self.settings.layout.close(p);
            }
        }
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            let compact_id = egui::Id::new("status_compact");
            let status_compact = ui.data(|d| d.get_temp::<bool>(compact_id)).unwrap_or(false);
            ui.horizontal(|ui| {
                // filele panourilor de jos (Find Items, Clash, TimeLiner...): clic = panoul se ridica,
                // clic pe fila deschisa = coboara (tot ecranul pentru model)
                let bottom = self.settings.layout.in_area(Dock::Bottom);
                // "Add panel" e mereu prezent, chiar si cand bara de jos e goala.
                if !bottom.is_empty() && !self.settings.layout.hide_all && !self.settings.layout.autohide[2] {
                    let active = self.settings.layout.active[2];
                    let open = self.settings.layout.bottom_open;
                    for p in bottom {
                        let on = open && active == Some(p);
                        let col = if on { accent() } else { ui.visuals().weak_text_color() };
                        let icon = p.title().split_whitespace().next().unwrap_or("");
                        let b = egui::Button::new(crate::theme::icon_job(icon, if status_compact { "" } else { p.short() }, 12.5, col)).fill(if on { accent().gamma_multiply(0.16) } else { Color32::TRANSPARENT }).stroke(Stroke::NONE).rounding(6.0);
                        let r = ui.add(b).on_hover_text(if on { format!("{} - click to lower it", p.title()) } else { format!("{} - click to raise it", p.title()) });
                        if r.clicked() {
                            if on {
                                self.settings.layout.bottom_open = false;
                            } else {
                                self.settings.layout.active[2] = Some(p);
                                self.settings.layout.bottom_open = true;
                            }
                        }
                        r.context_menu(|ui| self.panel_menu(ui, p));
                    }
                }
                if !self.settings.layout.hide_all {
                    ui.menu_button(if status_compact { "＋" } else { "＋ Add panel" }, |ui| {
                        ui.label(RichText::new("Add a panel to the bottom toolbar").small().weak());
                        ui.separator();
                        for p in crate::layout::ALL_PANELS {
                            let in_bottom = self.settings.layout.get(p).map(|s| s.dock == Dock::Bottom).unwrap_or(false);
                            if !in_bottom && ui.button(p.title()).clicked() {
                                self.settings.layout.move_to(p, Dock::Bottom);
                                ui.close_menu();
                            }
                        }
                    }).response.on_hover_text("Add a panel as a tab in the bottom horizontal bar");
                    ui.separator();
                }
                let nm = self.loaded_models().count();
                let ne: usize = self.loaded_models().map(|(_, m)| m.elements.len()).sum();
                let nt: usize = self.loaded_models().map(|(_, m)| m.tri_total()).sum();
                if nm == 0 {
                    ui.label(RichText::new("No model open").weak());
                } else {
                    // nivelul curent: alegerea lui face sectiunea pe nivel (ca in Navisworks / Revit)
                    let levels = self.scene_grids().levels;
                    if !levels.is_empty() {
                        let cur = self.section_level.filter(|_| self.section.enabled).and_then(|i| levels.get(i)).map(|l| l.0.clone()).unwrap_or_else(|| "All levels".into());
                        let mut pick: Option<Option<usize>> = None;
                        egui::ComboBox::from_id_salt("status_level").width(120.0).selected_text(format!("Level: {cur}")).show_ui(ui, |ui| {
                            if ui.selectable_label(!self.section.enabled, "All levels").clicked() {
                                pick = Some(None);
                            }
                            for (i, l) in levels.iter().enumerate().rev() {
                                if ui.selectable_label(self.section.enabled && self.section_level == Some(i), format!("{}  {:+.2}", l.0, l.1)).clicked() {
                                    pick = Some(Some(i));
                                }
                            }
                        });
                        match pick {
                            Some(Some(i)) => self.section_level_box(i),
                            Some(None) => self.section.enabled = false,
                            None => {}
                        }
                        ui.separator();
                    }
                    ui.label(format!("{} {} · {} elements", nm, if nm == 1 { "model" } else { "models" }, thousands(ne))).on_hover_text(format!("{} triangles", thousands(nt)));
                }
                // nor de puncte incarcat: densitatea direct in bara de stare (glisare = schimbare, clic = fereastra)
                if self.loaded_models().any(|(_, m)| m.cloud.is_some()) {
                    ui.separator();
                    let mut pct = (self.cloud_density * 100.0).round();
                    let r = ui
                        .add(egui::DragValue::new(&mut pct).range(1.0..=100.0).speed(0.5).prefix("☁ Points ").suffix(" %"))
                        .on_hover_text("Point cloud density - drag left / right to change it; double-click to type a value. Point size, colour and shape: View › Point Cloud (or click the cloud button in Analyze › Scan).");
                    if r.changed() {
                        self.cloud_density = pct / 100.0;
                    }
                    if ui.small_button("⚙").on_hover_text("Point cloud settings (density, point size, real size, colour by elevation, shape)").clicked() {
                        self.show_cloud = !self.show_cloud;
                    }
                }
                if !self.selected.is_empty() {
                    ui.separator();
                    ui.label(RichText::new(format!("{} selected", thousands(self.selected.len()))).color(accent()));
                }
                if !self.hidden.is_empty() {
                    ui.separator();
                    ui.label(format!("{} hidden", thousands(self.hidden.len())));
                }
                if self.section.enabled {
                    ui.separator();
                    ui.colored_label(Color32::from_rgb(240, 180, 60), "Section active");
                }
                if self.settings.sync_selection {
                    ui.separator();
                    // verde = un add-in a raspuns in ultimele 10 minute
                    let live: Vec<&str> = crate::sync::PEERS.iter().enumerate().filter(|(k, _)| self.sync.peer_seen[*k].map(|t| t.elapsed().as_secs() < 600).unwrap_or(false)).map(|(_, p)| p.1).collect();
                    let (txt, col) = if live.is_empty() { ("Live Link: waiting for Revit / Navisworks".to_string(), ui.visuals().weak_text_color()) } else { (format!("Live Link: {}", live.join(" + ")), Color32::from_rgb(70, 190, 110)) };
                    let r = ui.label(crate::theme::icon_job("🔗", &txt, 13.0, col));
                    if !self.sync.last_msg.is_empty() {
                        r.on_hover_text(&self.sync.last_msg);
                    }
                }
                if self.sim_view() {
                    ui.separator();
                    ui.colored_label(Color32::from_rgb(90, 200, 120), format!("Simulation: {}", timeline::fmt_date(self.sim_day.floor() as i32)));
                }
                let left_end = ui.min_rect().right();
                let rr = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let nb = if self.settings.nav_bar { "Hide the navigation bar" } else { "Show the navigation bar" };
                    let nav_icon = RichText::new(egui_phosphor::regular::COMPASS).font(crate::theme::icon_font(14.0)).color(if self.settings.nav_bar { accent() } else { ui.visuals().weak_text_color() });
                    if ui.add(egui::Button::new(nav_icon).frame(false)).on_hover_text(nb).clicked() {
                        self.settings.nav_bar = !self.settings.nav_bar;
                        self.settings.save();
                    }
                    self.notes_bell(ui);
                    self.update_badge(ui);
                    ui.label(RichText::new(format!("{:.0} FPS", self.fps)).weak()).on_hover_text("Frames per second");
                    // unitatea de lungime: acelasi sistem ca in Settings (m / mm / in / ft)
                    let cur_u = if self.settings.unit_system.is_empty() { "m".to_string() } else { self.settings.unit_system.clone() };
                    ui.menu_button(RichText::new(&cur_u).strong(), |ui| {
                        ui.label(RichText::new("Length unit").small().weak());
                        for code in ["m", "mm", "in", "ft"] {
                            if ui.selectable_label(cur_u == code, code).clicked() {
                                self.settings.unit_system = code.into();
                                self.settings.units_mm = code == "mm";
                                crate::app::set_unit(match code { "mm" => crate::app::Unit::Mm, "in" => crate::app::Unit::In, "ft" => crate::app::Unit::Ft, _ => crate::app::Unit::M });
                                self.settings.save();
                                ui.close_menu();
                            }
                        }
                    })
                    .response
                    .on_hover_text("Length unit for measurements, distances and coordinates (quantities stay in m, m², m³)");
                    let os_col = if self.osnap_on { Color32::from_rgb(80, 200, 120) } else { ui.visuals().weak_text_color() };
                    if ui.add(egui::Button::new(RichText::new("OSNAP").small().strong().color(os_col)).frame(false)).on_hover_text(if self.osnap_on { "Object snap ON - click or F3 to turn off" } else { "Object snap OFF - click or F3 to turn on" }).clicked() {
                        self.osnap_on = !self.osnap_on;
                    }
                });
                // bara nu incape (ecran ingust / interfata marita): filele raman doar cu iconite; revin cand e loc
                let right_start = rr.response.rect.left();
                let gap = right_start - left_end;
                let tabs = self.settings.layout.in_area(Dock::Bottom).len() as f32;
                if !status_compact && gap < 4.0 {
                    ui.data_mut(|d| d.insert_temp(compact_id, true));
                    ui.ctx().request_repaint();
                } else if status_compact && gap > tabs * 75.0 + 60.0 {
                    ui.data_mut(|d| d.insert_temp(compact_id, false));
                    ui.ctx().request_repaint();
                }
            });
        });
    }

    // ================================================================== viewport
    fn viewport(&mut self, ui: &mut egui::Ui, overlays: bool) {
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
            self.renderer.lock().unwrap_or_else(|e| e.into_inner()).capture = Some(crate::render::CaptureReq { params: cp, w, h, bg_top, bg_bottom });
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
                let hovered = hover.map_or(false, |h| rect.contains(h) && (seg_dist(h) < 16.0 || h.distance(p0) < 16.0));
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
                    if probe_c.map_or(false, |hp| rect.contains(hp) && seg_near(p0, end, hp) < 13.0) && self.coord_drag.is_none() && coord_hover.is_none() && handle_hover.is_none() {
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
                        let on_point = self.walk_path.iter().any(|q| to_screen(&vpm, rect, *q + lift).map_or(false, |s| s.distance(p) < 10.0));
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
                let near = hover.map_or(false, |h| rect.contains(h) && h.distance(p0) < 22.0);
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
        if (self.show_grids || self.show_levels) && self.loaded_models().next().is_some() && self.report_job.is_none() {
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
            if self.show_levels && bb.is_valid() && g.levels.len() < 60 {
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
                            let el = *z as f64 + oz - if self.use_user_levels { self.level_zero } else { 0.0 };
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
    fn nav_bar(&mut self, ctx: &egui::Context, rect: Rect) {
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

    fn draw_measure(&self, painter: &egui::Painter, vp: &Mat4, rect: Rect, k: MeasureKind, pts: &[Vec3], col: Color32) {
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

    fn windows(&mut self, ctx: &egui::Context) {
        self.manual_window(ctx);
        self.view_templates_window(ctx);
        self.section_preview_window(ctx);
        self.multiview_window(ctx);
        self.overlay_window(ctx);
        self.title_block_window(ctx);
        self.classification_window(ctx);
        self.coord_objects_window(ctx);
        self.prop_editor_window(ctx);
        self.poll_updates(ctx);
        self.update_window(ctx);
        self.mcp_window(ctx);
        self.materials_window(ctx);
        self.keys_window(ctx);
        self.smart_sets_window(ctx);
        self.dashboard_window(ctx);
        self.command_palette(ctx);
        self.asbuilt_window(ctx);
        if let Some(pr) = &mut self.prompt {
            let mut ok = false;
            let mut cancel = false;
            let title = match pr.purpose {
                Purpose::OpenModel { append: false } => "Open Model",
                Purpose::OpenModel { append: true } => "Append Model",
                Purpose::OpenProject => "Open Project",
                Purpose::SaveProject => "Save Project",
                Purpose::ExportClashHtml => "Export HTML Report",
                Purpose::ExportClashCsv => "Export CSV Report",
                Purpose::ExportClashPdf => "Export PDF Report",
                Purpose::ExportClashBcf => "Export clashes as BCF",
                Purpose::ExportIfc => "Export to IFC4",
                Purpose::ExportGlb => "Export to glTF (GLB)",
                Purpose::SharedFolder => "Shared issues folder",
                Purpose::SelectScriptsDir => "Scripts folder",
                Purpose::PublishBimd => "Publish project (.bimd)",
                Purpose::ExportGrids => "Export grid axes",
                Purpose::ImportGridsCad => "Import grid axes (DWG / DXF)",
                Purpose::ExportReference => "Reference for Revit / AutoCAD",
                Purpose::ExportClashMatrix => "Clash matrix (Excel)",
                Purpose::ImportClashTests => "Import Clash Tests (XML / CSV / Excel)",
                Purpose::ImportUserProps => "Import properties onto elements (Excel / CSV)",
                Purpose::ExportUserProps => "Export properties (Excel template)",
                Purpose::ExportClashTestsCsv => "Export Clash Tests (CSV)",
                Purpose::ExportWeb => "Web viewer (HTML)",
                Purpose::ExportAsBuilt => "As-built report (Excel)",
                Purpose::ExportSectionCad => "Section cut (DWG / DXF)",
                Purpose::ExportSectionPdf => "Section cut on sheet (PDF)",
                Purpose::ExportMultiView => "Multi-view drawing",
                Purpose::ExportCobie => "COBie handover (Excel)",
                Purpose::ImportOverlay => "DWG overlay",
                Purpose::ImportCheckRules => "Import IDS or rules",
                Purpose::ExportCheckRules => "Save rules",
                Purpose::ExportIds => "Export IDS",
                Purpose::ExportCheckXlsx => "Model check report (Excel)",
                Purpose::ExportCheckBcf => "Model check issues (BCF)",
                Purpose::ExportClashNavis => "Export clashes as Navisworks viewpoints",
                Purpose::ImportBcf => "Import BCF issues",
                Purpose::ImportTasksCsv => "Import Schedule (CSV, MS Project XML, Primavera P6 XML)",
                Purpose::ExportTasksCsv => "Export CSV Schedule",
                Purpose::ExportCache(_) => "Export ManageBIM Cache (.bimc)",
                Purpose::ExportSets => "Export Sets",
                Purpose::ImportTextures => "Import image as material",
                Purpose::ImportPbr => "Import PBR material packs",
                Purpose::ImportPbrFolder => "Import PBR folder",
                Purpose::ExportVpHtml => "Viewpoints report (HTML)",
                Purpose::ExportVpXlsx => "Viewpoints report (Excel .xlsx)",
                Purpose::ExportVpPdf => "Sheets (PDF)",
                Purpose::ExportBatch => "Batch export folder (PDF, HTML, Excel, BCF)",
                Purpose::ExportSettingsProfile => "Export settings profile (.mbprofile)",
                Purpose::ImportSettingsProfile => "Import settings profile (.mbprofile)",
                Purpose::ExportBcf => "Export BCF 2.1 issues",
                Purpose::ExportNavisXml => "Export viewpoints for Navisworks (XML)",
                Purpose::ImportNavisXml => "Import Navisworks viewpoints (XML)",
                Purpose::ExportNavisSets => "Export sets for Navisworks (XML)",
                Purpose::ExportNavisClash => "Export clash tests for Navisworks (XML)",
                Purpose::MaterialTexture => "Texture image",
                Purpose::MaterialNormal => "Normal map",
                Purpose::MaterialRoughness => "Roughness map",
                Purpose::MaterialMetallic => "Metallic map",
                Purpose::MaterialAo => "Ambient occlusion map",
                Purpose::MaterialHeight => "Height map",
                Purpose::ExportMaterialX => "Export MaterialX",
                Purpose::ExportMaterialOne => "Save material as file",
                Purpose::ExportDashboardPdf => "Export dashboard (PDF)",
                Purpose::ExportDashboardDocx => "Export dashboard (Word)",
                Purpose::ImportMaterialX => "Import MaterialX",
                Purpose::LoadHdri => "Environment map",
                Purpose::ExportIfcMap => "Export IFC mapping",
                Purpose::ImportIfcMap => "Import IFC mapping",
                Purpose::ImportClassifications => "Import classifications",
                Purpose::ExportClassifications => "Export classifications",
                Purpose::ExportMaterials => "Export material library",
                Purpose::ImportMaterials => "Import material library",
                Purpose::ExportVideo => "Export Animation",
                Purpose::RenderImage => "Render Image",
                Purpose::ExportPathTrace => "Export Path Trace",
                Purpose::ImportSets => "Import Sets",
                Purpose::ExportProfile => "Export Appearance Profile",
                Purpose::ImportProfile => "Import Appearance Profile",
                Purpose::ExportCompare => "Save Comparison",
                Purpose::ImportCompare => "Open Comparison",
                Purpose::ExportCompareCsv => "Comparison report (CSV)",
                Purpose::ExportCompareParamsCsv => "Parameter changes (CSV)",
                Purpose::ExportQuickChecksCsv => "Quick checks report (CSV)",
                Purpose::ExportTree => "Export Tree Layout",
                Purpose::ImportTree => "Import Tree Layout",
                Purpose::ExportQtoXlsx => "Export Quantities (Excel)",
                Purpose::ExportQtoCsv => "Export Quantities (CSV)",
                Purpose::ExportQtoTemplate => "Save Quantification Template",
                Purpose::ImportQtoTemplate => "Open Quantification Template",
            };
            egui::Window::new(title).collapsible(false).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label("File path:");
                let r = ui.add(egui::TextEdit::singleline(&mut pr.text).desired_width(420.0));
                r.request_focus();
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    ok = true;
                }
                ui.horizontal(|ui| {
                    ok |= ui.button("OK").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
            if ok {
                let pr = self.prompt.take().unwrap();
                self.handle_path(pr.purpose, pr.text);
            } else if cancel {
                self.prompt = None;
            }
        }
        if self.show_help {
            egui::Window::new("ManageBIM Controls").open(&mut self.show_help).default_width(520.0).show(ctx, |ui| {
                egui::Grid::new("help").striped(true).num_columns(2).show(ui, |ui| {
                    for (k, v) in [
                        ("Left click", "Select element (Ctrl+click: add / remove)"),
                        ("Left drag", "Orbit around the point under the cursor"),
                        ("Shift + drag / middle drag", "Pan"),
                        ("Right drag", "Look around"),
                        ("Mouse wheel", "Zoom towards the cursor"),
                        ("W A S D / arrows", "Walk (Shift = fast), Q / E down / up"),
                        ("Double click", "Focus on element"),
                        ("F", "Focus on selection / all"),
                        ("Ctrl+F", "Find Items"),
                        ("H / I", "Hide selection / hide unselected"),
                        ("Esc", "Clear selection / finish measuring"),
                        ("Ctrl+A", "Select all"),
                        ("Ctrl+O / Ctrl+S", "Open model / save project"),
                        ("F5 / Ctrl+F5", "Refresh changed models / reload all models"),
                        ("Page Up / Down", "Previous / next clash result"),
                        ("Drag files", ".ifc, .bimc, .glb, .gltf, .bimr, .csv into the window"),
                    ] {
                        ui.strong(k);
                        ui.label(v);
                        ui.end_row();
                    }
                });
                ui.separator();
                ui.label("Formats: IFC 2x3 / IFC4 (STEP), ManageBIM cache (.bimc) and glTF 2.0. Navisworks (NWD/NWC) and Revit (RVT) files are closed formats — export to IFC from Revit.");
            });
        }
        if self.confirm_start {
            let pending = self.start_pending.clone().unwrap_or(crate::app::StartAction::Close);
            if !self.has_open_project() {
                self.run_start_action(pending);
            } else {
                let mut action = 0u8;
                egui::Window::new("Save the project?").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                    let name = self.bimd_path.as_ref().or(self.project_path.as_ref()).map(|p| crate::file_name(p)).unwrap_or_else(|| "the current project".into());
                    ui.label(format!("Save the changes to {name}?"));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("💾 Save").clicked() {
                            action = 1;
                        }
                        if ui.button("Don't save").clicked() {
                            action = 2;
                        }
                        if ui.button("Cancel").clicked() {
                            action = 3;
                        }
                    });
                });
                match action {
                    1 => {
                        if self.project_path.is_some() || self.bimd_path.is_some() {
                            self.save_now();
                            self.run_start_action(pending);
                        } else {
                            // fara fisier inca: Save As; actiunea se reia apoi din pagina de start
                            self.confirm_start = false;
                            self.start_pending = None;
                            self.show_start = false;
                            self.request_path(Purpose::SaveProject);
                        }
                    }
                    2 => self.run_start_action(pending),
                    3 => {
                        self.confirm_start = false;
                        self.start_pending = None;
                    }
                    _ => {}
                }
            }
        }
        if self.show_clash_matrix {
            self.clash_matrix_window(ctx);
        }
        if self.update_summary.is_some() {
            self.update_summary_window(ctx);
        }
        if self.prop_import.is_some() {
            self.prop_import_window(ctx);
        }
        if self.show_grids_editor {
            let mut open = true;
            let mut export = false;
            egui::Window::new("# Grids").open(&mut open).collapsible(false).default_width(470.0).show(ctx, |ui| {
                ui.checkbox(&mut self.use_user_grids, "Use my grids (instead of the grids in the models)");
                ui.add_space(4.0);
                // generator: originea si rotatia se pot alege cu clic in model (snap pe colturi)
                egui::CollapsingHeader::new(RichText::new("Generate a grid").strong()).default_open(self.user_grids.is_empty()).show(ui, |ui| {
                    let gg = &mut self.grid_gen;
                    egui::Grid::new("gg").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
                        ui.label("Origin X / Y (real, m)");
                        ui.add(egui::DragValue::new(&mut gg.origin[0]).speed(0.1).fixed_decimals(3));
                        ui.add(egui::DragValue::new(&mut gg.origin[1]).speed(0.1).fixed_decimals(3));
                        ui.label("");
                        ui.end_row();
                        ui.label("Rotation");
                        ui.add(egui::DragValue::new(&mut gg.angle_deg).speed(0.1).range(-360.0..=360.0).fixed_decimals(2).suffix("°"));
                        ui.label("");
                        ui.label("");
                        ui.end_row();
                        ui.label("Numbered axes (count, spacing m, first)");
                        ui.add(egui::DragValue::new(&mut gg.nx).range(0..=60));
                        ui.add(egui::DragValue::new(&mut gg.dx).speed(0.05).range(0.1..=1000.0).fixed_decimals(2));
                        ui.add(egui::TextEdit::singleline(&mut gg.fx).desired_width(40.0));
                        ui.end_row();
                        ui.label("Lettered axes (count, spacing m, first)");
                        ui.add(egui::DragValue::new(&mut gg.ny).range(0..=60));
                        ui.add(egui::DragValue::new(&mut gg.dy).speed(0.05).range(0.1..=1000.0).fixed_decimals(2));
                        ui.add(egui::TextEdit::singleline(&mut gg.fy).desired_width(40.0));
                        ui.end_row();
                    });
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("🎯 Pick origin + direction in 3D").on_hover_text("Click the grid origin in the model (snaps to corners), then a point along the first lettered axis").clicked() {
                            self.grid_pick = 1;
                        }
                        if ui.small_button("Model corner").on_hover_text("Origin at the lower-left corner of the loaded models").clicked() {
                            let bb = self.scene_aabb();
                            if bb.is_valid() {
                                let o = self.origin_f64();
                                self.grid_gen.origin = [(bb.min.x as f64 + o.x).round(), (bb.min.y as f64 + o.y).round()];
                            }
                        }
                        if ui.button("⚙ Generate (replaces my grids)").clicked() {
                            self.generate_user_grid();
                        }
                    });
                    if self.grid_pick > 0 {
                        ui.label(RichText::new(if self.grid_pick == 1 { "Click the origin in the 3D view…" } else { "Click a point along the first axis direction…" }).color(accent()));
                    }
                });
                // mutare / rotire a axelor existente
                egui::CollapsingHeader::new(RichText::new("Move / rotate my axes").strong()).default_open(false).show(ui, |ui| {
                    let id = ui.make_persistent_id("gridmove");
                    let mut mv: (f64, f64, f64) = ui.data_mut(|d| d.get_temp(id)).unwrap_or((0.0, 0.0, 0.0));
                    ui.horizontal(|ui| {
                        ui.label("Move X / Y");
                        ui.add(egui::DragValue::new(&mut mv.0).speed(0.05).fixed_decimals(3).suffix(" m"));
                        ui.add(egui::DragValue::new(&mut mv.1).speed(0.05).fixed_decimals(3).suffix(" m"));
                        ui.label("Rotate");
                        ui.add(egui::DragValue::new(&mut mv.2).speed(0.1).fixed_decimals(2).suffix("°"));
                        if ui.button("Apply").on_hover_text("Rotation around the generator origin, then the move").clicked() {
                            let c = self.grid_gen.origin;
                            crate::app::transform_grids(&mut self.user_grids, c, mv.2, [mv.0, mv.1]);
                            mv = (0.0, 0.0, 0.0);
                        }
                    });
                    ui.data_mut(|d| d.insert_temp(id, mv));
                });
                // lista
                egui::CollapsingHeader::new(RichText::new(format!("My axes ({})", self.user_grids.len())).strong()).default_open(true).show(ui, |ui| {
                    let mut del = None;
                    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        egui::Grid::new("ug").num_columns(6).striped(true).spacing([6.0, 3.0]).show(ui, |ui| {
                            for h in ["Tag", "X1", "Y1", "X2", "Y2", ""] {
                                ui.label(RichText::new(h).small().strong());
                            }
                            ui.end_row();
                            for (i, g) in self.user_grids.iter_mut().enumerate() {
                                ui.add(egui::TextEdit::singleline(&mut g.name).desired_width(40.0));
                                let [a0, a1] = &mut g.a;
                                let [b0, b1] = &mut g.b;
                                for v in [a0, a1, b0, b1] {
                                    ui.add(egui::DragValue::new(v).speed(0.05).fixed_decimals(2));
                                }
                                if ui.small_button("✖").clicked() {
                                    del = Some(i);
                                }
                                ui.end_row();
                            }
                        });
                    });
                    if let Some(i) = del {
                        self.user_grids.remove(i);
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("➕ Add axis").clicked() {
                            let n = self.user_grids.len();
                            let last = self.user_grids.last().cloned();
                            let g = match last {
                                Some(l) => {
                                    // paralela cu ultima, la 6 m
                                    let d = glam::DVec2::from(l.b) - glam::DVec2::from(l.a);
                                    let nrm = glam::DVec2::new(-d.y, d.x).normalize_or_zero() * 6.0;
                                    crate::app::UserGrid { name: crate::app::grid_label(&l.name, 1), a: (glam::DVec2::from(l.a) + nrm).into(), b: (glam::DVec2::from(l.b) + nrm).into() }
                                }
                                None => {
                                    let o = self.origin_f64();
                                    crate::app::UserGrid { name: format!("{}", n + 1), a: [o.x, o.y - 10.0], b: [o.x, o.y + 10.0] }
                                }
                            };
                            self.user_grids.push(g);
                            self.use_user_grids = true;
                        }
                        if ui.button("⤓ Copy from models").clicked() {
                            let saved = self.use_user_grids;
                            self.use_user_grids = false;
                            let g = self.grids_world();
                            self.use_user_grids = saved;
                            if !g.is_empty() {
                                self.user_grids = g;
                                self.use_user_grids = true;
                            }
                        }
                        if ui.button("Clear").clicked() {
                            self.user_grids.clear();
                        }
                    });
                    // import din DWG / DXF (planul de axe al arhitectului / structuristului)
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("📥 Import from DWG / DXF…").on_hover_text("Grid lines from the drawing (layer GRID / AXE / AX…) with the tags from their bubbles; units from the drawing").clicked() {
                            self.request_path(Purpose::ImportGridsCad);
                        }
                        if let Some((path, layers, cur)) = self.grid_import.clone() {
                            ui.label(RichText::new("Layer").weak());
                            let mut pick: Option<String> = None;
                            egui::ComboBox::from_id_salt("grid_layer").width(150.0).selected_text(if cur.is_empty() { "choose…".to_string() } else { cur.clone() }).show_ui(ui, |ui| {
                                for (l, n) in layers.iter().take(200) {
                                    if ui.selectable_label(*l == cur, format!("{l}  ({n} lines)")).clicked() {
                                        pick = Some(l.clone());
                                    }
                                }
                            });
                            if let Some(l) = pick {
                                self.import_grids_cad(&path, Some(&l));
                            }
                        }
                    });
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Bubble size");
                    ui.add(egui::Slider::new(&mut self.grid_bubble, 6.0..=30.0).suffix(" px").fixed_decimals(0));
                });
                ui.horizontal(|ui| {
                    ui.label("Extend lines");
                    ui.add(egui::Slider::new(&mut self.grid_ext, 0.0..=50.0).suffix(" m").fixed_decimals(1)).on_hover_text("Draw the axes longer than they are, so they stay visible from far away");
                });
                ui.horizontal(|ui| {
                    if ui.button("📤 Export DXF / DWG…").on_hover_text("Lines on layer GRID, bubbles and tags on GRID_TAGS, real coordinates in metres").clicked() {
                        export = true;
                    }
                    ui.label(RichText::new("Saved with the project (.bimr).").weak().small());
                });
            });
            self.show_grids_editor = open;
            if export {
                self.request_path(Purpose::ExportGrids);
            }
        }
        if self.show_levels_editor {
            let mut open = true;
            egui::Window::new("☰ Levels").open(&mut open).collapsible(false).default_width(380.0).show(ctx, |ui| {
                ui.checkbox(&mut self.use_user_levels, "Use my levels (instead of the levels in the models)");
                ui.add_space(4.0);
                let oz = self.origin_f64().z;
                ui.horizontal(|ui| {
                    ui.label("±0.00 at real Z =");
                    ui.add(egui::DragValue::new(&mut self.level_zero).speed(0.01).fixed_decimals(3).suffix(" m"));
                    if ui.small_button("Lowest level").on_hover_text("Put ±0.00 at the lowest level of the models").clicked() {
                        if let Some(l) = self.scene_grids_models().levels.first() {
                            self.level_zero = l.1 as f64 + oz;
                        }
                    }
                });
                ui.add_space(4.0);
                let mut del = None;
                egui::Grid::new("ulev").num_columns(3).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                    ui.label(RichText::new("Name").strong());
                    ui.label(RichText::new("Elevation (from ±0.00)").strong());
                    ui.label("");
                    ui.end_row();
                    for (i, l) in self.user_levels.iter_mut().enumerate() {
                        ui.add(egui::TextEdit::singleline(&mut l.name).desired_width(150.0));
                        ui.add(egui::DragValue::new(&mut l.elev).speed(0.01).fixed_decimals(3).suffix(" m"));
                        if ui.small_button("✖").clicked() {
                            del = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if let Some(i) = del {
                    self.user_levels.remove(i);
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button("➕ Add level").clicked() {
                        let top = self.user_levels.iter().map(|l| l.elev).fold(f64::NEG_INFINITY, f64::max);
                        let elev = if top.is_finite() { top + 3.0 } else { 0.0 };
                        self.user_levels.push(crate::app::UserLevel { name: format!("Level {}", self.user_levels.len()), elev });
                        self.use_user_levels = true;
                    }
                    if ui.button("⤓ Copy from models").on_hover_text("Start from the levels found in the loaded models").clicked() {
                        let g = self.scene_grids_models();
                        self.user_levels = g.levels.iter().map(|(n, z)| crate::app::UserLevel { name: n.clone(), elev: ((*z as f64 + oz - self.level_zero) * 1000.0).round() / 1000.0 }).collect();
                        self.use_user_levels = true;
                    }
                    if ui.button("Sort by elevation").clicked() {
                        self.user_levels.sort_by(|a, b| a.elev.total_cmp(&b.elev));
                    }
                });
                ui.label(RichText::new("Saved with the project (.bimr). Used in the 3D view and for the clash locations (e.g. C-4 / Level 2).").weak().small());
            });
            self.show_levels_editor = open;
        }
        if self.show_cloud {
            let mut open = true;
            let total: usize = self.loaded_models().filter_map(|(_, m)| m.cloud.as_ref().map(|c| c.pos.len())).sum();
            egui::Window::new("☁ Point Cloud").open(&mut open).collapsible(false).resizable(false).default_width(320.0).show(ctx, |ui| {
                let mut pct = (self.cloud_density * 100.0).round();
                egui::Grid::new("cloud").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Density");
                    if ui.add(egui::Slider::new(&mut pct, 1.0..=100.0).suffix(" %").integer()).changed() {
                        self.cloud_density = pct / 100.0;
                    }
                    ui.end_row();
                    ui.label("Point size");
                    ui.add(egui::Slider::new(&mut self.point_size, 1.0..=8.0).suffix(" px").fixed_decimals(0).step_by(1.0)).on_hover_text("Smallest size on screen (far away)");
                    ui.end_row();
                    ui.label("Real size");
                    ui.add(egui::Slider::new(&mut self.cloud_world_mm, 0.0..=50.0).suffix(" mm").fixed_decimals(0)).on_hover_text("Size of a point in the model: close up the points grow and close the gaps (continuous surface). 0 = always the pixel size above.");
                    ui.end_row();
                    ui.label("Colour");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.cloud_mode, 0, "Scan RGB");
                        ui.selectable_value(&mut self.cloud_mode, 1, "Elevation").on_hover_text("Blue (low) → red (high): read heights at a glance");
                        ui.selectable_value(&mut self.cloud_mode, 2, "Grey");
                    });
                    ui.end_row();
                    ui.label("Shape");
                    ui.checkbox(&mut self.cloud_round, "Round points with relief").on_hover_text("Each point drawn as a small lit sphere - smoother, more realistic surfaces");
                    ui.end_row();
                });
                ui.label(RichText::new("Measure snaps exactly onto scan points (the tool picks the nearest visible point under the cursor).").weak().small());
                let shown = (total as f64 * self.cloud_density as f64) as usize;
                ui.label(RichText::new(format!("{} of {} points shown. While the camera moves only about a third of them are drawn.", fmt_count(shown), fmt_count(total))).weak().small());
                ui.label(RichText::new("Each scan is an item in the Selection Tree: hide it, isolate it or cut it with the section box.").weak().small());
            });
            self.show_cloud = open;
        }
        if self.show_light {
            let mut open = true;
            let mut changed = false;
            egui::Window::new("💡 Lighting").open(&mut open).collapsible(false).resizable(false).default_width(330.0).show(ctx, |ui| {
                changed |= self.sun_settings_ui(ui, "light");
                ui.horizontal(|ui| {
                    if ui.button("⟲ Default").clicked() {
                        self.render.reset_light();
                        changed = true;
                    }
                    ui.label(RichText::new("Applies to the 3D view, images and videos.").weak().small());
                });
            });
            self.show_light = open;
            if changed {
                ctx.request_repaint();
            }
        }
        if self.show_about {
            egui::Window::new("About ManageBIM").open(&mut self.show_about).collapsible(false).show(ctx, |ui| {
                ui.heading(format!("ManageBIM {}", env!("CARGO_PKG_VERSION")));
                ui.label("Review and coordination of federated BIM models, written in Rust.");
                ui.label(RichText::new(format!("Build {} · OpenCascade: {}", env!("MB_BUILD_DATE"), crate::occt::status().unwrap_or_else(|e| e))).weak().small());
                ui.label(RichText::new("What's new: see docs\\CHANGELOG.md").weak().small());
                ui.label("Modules: 3D viewer, selection tree, properties, find items & sets, Clash Detective, TimeLiner 4D, sectioning, measuring, viewpoints, .bimr projects, .bimc model cache.");
                ui.label(RichText::new("Independent project, not affiliated with Autodesk.").weak());
            });
        }
    }

    fn toasts_ui(&mut self, ctx: &egui::Context) {
        self.toasts.retain(|t| t.1.elapsed().as_secs_f32() < 6.0);
        if self.toasts.is_empty() {
            return;
        }
        // 0.28: O SINGURA notificare vizibila (cea mai noua; erorile au prioritate), cu un contor „+N"
        // pentru celelalte - nu se mai aduna cate trei peste model; istoria completa e in Log (Project)
        let total = self.toasts.len();
        let shown = self.toasts.iter().rposition(|t| t.2).unwrap_or(total - 1);
        let (m, t, err) = self.toasts[shown].clone();
        egui::Area::new(egui::Id::new("toasts")).anchor(Align2::RIGHT_BOTTOM, [-16.0, -40.0]).interactable(false).show(ctx, |ui| {
            let a = (1.0 - (t.elapsed().as_secs_f32() - 5.0).max(0.0)).clamp(0.0, 1.0);
            egui::Frame::popup(ui.style())
                .fill(if err { Color32::from_rgb(110, 30, 30) } else { Color32::from_rgb(30, 45, 65) })
                .rounding(crate::theme::RADIUS_MD)
                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                .multiply_with_opacity(a)
                .show(ui, |ui| {
                    ui.set_max_width(440.0);
                    ui.horizontal(|ui| {
                        let (ic, col) = if err { ("⚠", Color32::from_rgb(255, 170, 160)) } else { ("✔", Color32::from_rgb(120, 220, 150)) };
                        ui.label(RichText::new(crate::theme::ph(ic)).font(crate::theme::icon_font(15.0)).color(col));
                        ui.label(RichText::new(&m).color(Color32::WHITE));
                        if total > 1 {
                            egui::Frame::none().fill(Color32::from_rgba_unmultiplied(59, 130, 246, 70)).rounding(6.0).inner_margin(egui::Margin::symmetric(6.0, 1.0)).show(ui, |ui| {
                                ui.label(RichText::new(format!("+{}", total - 1)).small().color(Color32::from_rgb(190, 215, 255)));
                            });
                        }
                    });
                });
        });
    }
}

/// 12345678 -> "12 345 678" (mai usor de citit in bara de stare)
fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// buton de ribbon: iconita sus si eticheta dedesubt (Full) sau doar iconita (Compact, numele in tooltip).
/// Fara contur: hover = fundalul se lumineaza, activ = nuanta de accent.
fn rows_has(tabs: &[(String, Vec<(String, String)>)], tab: &str, name: &str) -> bool {
    tabs.iter().any(|t| t.0 == tab && t.1.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)))
}

/// Eticheta unui buton din ribbon pe cel mult doua randuri, fara text taiat: un rand daca incape in
/// `w`, altfel impartita la spatiul care da randurile cele mai egale; fontul scade putin doar daca un
/// singur cuvant e mai lat decat butonul.
fn ribbon_label_lines(ui: &egui::Ui, label: &str, w: f32) -> Vec<(String, FontId)> {
    let width = |t: &str, f: &FontId| ui.fonts(|fo| fo.layout_no_wrap(t.to_string(), f.clone(), Color32::WHITE).size().x);
    for size in [10.5_f32, 9.5, 8.8] {
        let f = FontId::proportional(size);
        if width(label, &f) <= w {
            return vec![(label.to_string(), f)];
        }
        let words: Vec<&str> = label.split(' ').filter(|s| !s.is_empty()).collect();
        let mut best: Option<(f32, String, String)> = None;
        for i in 1..words.len() {
            let (a, b) = (words[..i].join(" "), words[i..].join(" "));
            let (wa, wb) = (width(&a, &f), width(&b, &f));
            if wa <= w && wb <= w {
                let worst = wa.max(wb);
                if best.as_ref().map_or(true, |x| worst < x.0) {
                    best = Some((worst, a, b));
                }
            }
        }
        if let Some((_, a, b)) = best {
            return vec![(a, f.clone()), (b, f)];
        }
        // un singur cuvant lung (ex. „Placeholder"): despartit in silabe aproximative cu cratima
        if words.len() == 1 && size < 9.0 {
            // intai un font putin mai mic pe un singur rand (cuvintele scurte nu se mai despart: „Prope-rties")
            for s2 in [8.2_f32, 7.6] {
                let f2 = FontId::proportional(s2);
                if width(label, &f2) <= w {
                    return vec![(label.to_string(), f2)];
                }
            }
            let chars: Vec<char> = label.chars().collect();
            let mid = chars.len() / 2;
            let a: String = chars[..mid].iter().collect::<String>() + "-";
            let b: String = chars[mid..].iter().collect();
            return vec![(a, f.clone()), (b, f)];
        }
    }
    vec![(label.to_string(), FontId::proportional(8.8))]
}

fn rbtn(ui: &mut egui::Ui, icon: &str, label: &str, tip: &str, on: bool, enabled: bool) -> egui::Response {
    let label = crate::i18n::tr(label);
    let tip = crate::i18n::tr_lines(tip);
    let tip = tip.as_str();
    let enabled = enabled && ui.is_enabled();
    let compact = crate::theme::RIBBON_COMPACT.load(std::sync::atomic::Ordering::Relaxed);
    let font = FontId::proportional(10.5);
    // Dimensiune FIXA pentru toate butoanele din ribbon; etichetele se scurteaza ca sa nu se suprapuna.
    let size = if compact {
        egui::vec2(28.0, 30.0)
    } else {
        egui::vec2(crate::theme::ribbon_btn_w(), 48.0)
    };
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let p = ui.painter();
    // hover cu fade (tranzitie scurta), activ = nuanta de accent; colturi rotunjite + contur fin (stil Mitcad)
    let hov = ui.ctx().animate_bool_with_time(resp.id.with("rh"), enabled && resp.hovered(), 0.12);
    let rounding = egui::Rounding::same(crate::theme::RADIUS_MD);
    if on {
        p.rect_filled(rect, rounding, accent().gamma_multiply(0.20));
    } else if hov > 0.0 {
        p.rect_filled(rect, rounding, ui.visuals().widgets.hovered.weak_bg_fill.gamma_multiply(hov));
    }
    // contur discret: doar cand e hover/activ, ca sa nu incarce ribbon-ul
    if on || hov > 0.0 {
        let stroke_col = if on { accent() } else { crate::theme::border().gamma_multiply(0.55 + 0.45 * hov) };
        p.rect_stroke(rect, rounding, egui::Stroke::new(1.0_f32, stroke_col));
    }
    let col = if !enabled { ui.visuals().weak_text_color().gamma_multiply(0.6) } else if on { accent() } else { ui.visuals().text_color() };
    // iconita colorata pe categorii (doar activ si neselectat; dezactivat ramane gri)
    let icol = if enabled && !on && crate::theme::COLOR_ICONS.load(std::sync::atomic::Ordering::Relaxed) {
        crate::theme::icon_tint(icon, ui.visuals().dark_mode).unwrap_or(col)
    } else {
        col
    };
    let icon_c = if compact { rect.center() } else { rect.center_top() + egui::vec2(0.0, 12.0) };
    if let Some(fmt) = icon.strip_prefix("fmt:") {
        // iconita de format de fisier (IFC / GLB / BCF ...): foaie cu colt indoit + eticheta colorata
        file_format_icon(p, icon_c, fmt, enabled && !on, col);
    } else if compact {
        p.text(rect.center(), Align2::CENTER_CENTER, crate::theme::ph(icon), crate::theme::icon_font(17.0), icol);
    } else {
        p.text(icon_c, Align2::CENTER_CENTER, crate::theme::ph(icon), crate::theme::icon_font(17.0), icol);
    }
    if !compact {
        // eticheta intreaga (0.28): pe un rand daca incape, altfel pe doua (impartita la spatiul cel mai
        // echilibrat); un cuvant prea lung primeste un font putin mai mic - nu mai taiem textul cu „…"
        let lines = ribbon_label_lines(ui, &label, rect.width() - 4.0);
        let n = lines.len() as f32;
        for (k, (t, f)) in lines.iter().enumerate() {
            let y = rect.bottom() - 7.0 - (n - 1.0 - k as f32) * 11.0;
            p.text(egui::pos2(rect.center().x, y), Align2::CENTER_CENTER, t, f.clone(), col);
        }
        let _ = font;
    }
    let tip = if compact { format!("{label}\n{tip}") } else { tip.to_string() };
    if enabled {
        resp.on_hover_text(tip)
    } else {
        resp.on_hover_text(format!("{tip}  {}", crate::i18n::tr("(not available now)")))
    }
}

/// Iconita de format de fisier: o foaie cu coltul indoit si o eticheta colorata cu extensia
/// (IFC albastru buildingSMART, GLB verde Khronos, BCF portocaliu...) - ca in Revit / Navisworks.
fn file_format_icon(p: &egui::Painter, c: egui::Pos2, fmt: &str, colored: bool, base: Color32) {
    let tag_col = if !colored {
        base
    } else {
        match fmt {
            "IFC" => Color32::from_rgb(38, 132, 230),
            "GLB" | "GLTF" => Color32::from_rgb(76, 175, 80),
            "BCF" => Color32::from_rgb(240, 140, 40),
            "PDF" => Color32::from_rgb(220, 60, 50),
            "DWG" | "DXF" => Color32::from_rgb(200, 60, 60),
            "XLS" | "CSV" => Color32::from_rgb(30, 140, 80),
            _ => Color32::from_rgb(120, 120, 200),
        }
    };
    let w = 14.0;
    let h = 17.0;
    let fold = 4.5;
    let l = c.x - w * 0.5 + 2.0;
    let t = c.y - h * 0.5;
    let r = l + w;
    let b = t + h;
    let stroke = egui::Stroke::new(1.3_f32, base.gamma_multiply(0.9));
    // conturul foii cu coltul dreapta-sus taiat
    let pts = vec![egui::pos2(l, t), egui::pos2(r - fold, t), egui::pos2(r, t + fold), egui::pos2(r, b), egui::pos2(l, b), egui::pos2(l, t)];
    p.add(egui::Shape::line(pts, stroke));
    p.add(egui::Shape::line(vec![egui::pos2(r - fold, t), egui::pos2(r - fold, t + fold), egui::pos2(r, t + fold)], stroke));
    // eticheta cu extensia, iesita spre stanga peste foaie
    let font = FontId::new(7.5, egui::FontFamily::Proportional);
    let galley = p.layout_no_wrap(fmt.to_string(), font, Color32::WHITE);
    let tw = galley.size().x + 5.0;
    let tag = egui::Rect::from_min_size(egui::pos2(l - 5.0, c.y + 0.5), egui::vec2(tw.max(18.0), 9.0));
    p.rect_filled(tag, 2.0, tag_col);
    p.galley(egui::pos2(tag.center().x - galley.size().x * 0.5, tag.center().y - galley.size().y * 0.5), galley, Color32::WHITE);
}

/// grupa de ribbon = card ridicat (fara chenar, fara separator); titlul sub butoane doar in modul Full.
/// Golul dintre carduri separa grupele.
fn rgroup(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    let title = crate::i18n::tr(title);
    let compact = crate::theme::RIBBON_COMPACT.load(std::sync::atomic::Ordering::Relaxed);
    let fill = card_fill(ui.visuals());
    let avail_h = ui.available_height();
    egui::Frame::none().fill(fill).rounding(crate::theme::RADIUS_LG).stroke(egui::Stroke::new(1.0_f32, crate::theme::border())).inner_margin(egui::Margin { left: if compact { 4.0 } else { 7.0 }, right: if compact { 4.0 } else { 7.0 }, top: 4.0, bottom: 3.0 }).show(ui, |ui| {
        ui.set_min_height(avail_h - 5.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let r = ui
                .horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = if compact { 2.0 } else { 4.0 };
                    if compact {
                        ui.set_min_height(avail_h - 8.0);
                        ui.set_max_height(avail_h - 6.0);
                    } else {
                        ui.set_min_height(48.0);
                        ui.set_max_height(54.0);
                    }
                    add(ui);
                })
                .response;
            if !compact {
                let w = r.rect.width();
                ui.allocate_ui_with_layout(egui::vec2(w, 13.0), egui::Layout::top_down(egui::Align::Center), |ui| {
                    ui.set_width(w);
                    ui.label(RichText::new(title).size(10.5).weak());
                });
            }
        });
    });
}

/// suprafata cardurilor: putin mai deschisa decat panoul (in tema Light: alb pur)
pub fn card_fill(v: &egui::Visuals) -> Color32 {
    let p = v.panel_fill;
    if v.dark_mode {
        let up = |c: u8| c.saturating_add(9);
        Color32::from_rgb(up(p.r()), up(p.g()), up(p.b()))
    } else {
        Color32::WHITE
    }
}

fn inside(poly: &[Pos2], p: Pos2) -> bool {
    let mut sign = 0.0f32;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let cr = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        if cr.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = cr.signum();
        } else if cr.signum() != sign {
            return false;
        }
    }
    true
}

fn red_pt(rect: Rect, p: [f32; 2]) -> Pos2 {
    Pos2::new(rect.center().x + p[0] * rect.height(), rect.center().y + p[1] * rect.height())
}

/// draws markups (viewport units -> pixels)
pub fn draw_redlines(painter: &egui::Painter, rect: Rect, reds: &[crate::markup::Redline]) {
    use crate::markup::RedKind;
    for r in reds {
        let col = Color32::from_rgb(r.color[0], r.color[1], r.color[2]);
        let st = Stroke::new(r.width, col);
        let pts: Vec<Pos2> = r.pts.iter().map(|p| red_pt(rect, *p)).collect();
        if pts.is_empty() {
            continue;
        }
        let a = pts[0];
        let b = *pts.last().unwrap();
        let bx = Rect::from_two_pos(a, b);
        match r.kind {
            RedKind::Freehand => {
                painter.add(egui::Shape::line(pts.clone(), st));
            }
            RedKind::Line => {
                painter.line_segment([a, b], st);
            }
            RedKind::Arrow => {
                painter.line_segment([a, b], st);
                let d = (b - a).normalized();
                let n = EV::new(-d.y, d.x);
                let l = 10.0 + r.width * 3.0;
                painter.add(egui::Shape::convex_polygon(vec![b, b - d * l + n * l * 0.45, b - d * l - n * l * 0.45], col, Stroke::NONE));
            }
            RedKind::Rect => {
                painter.rect_stroke(bx, 0.0, st);
            }
            RedKind::Ellipse => {
                let c = bx.center();
                let (rx, ry) = (bx.width() * 0.5, bx.height() * 0.5);
                let e: Vec<Pos2> = (0..64).map(|i| {
                    let t = i as f32 / 64.0 * std::f32::consts::TAU;
                    Pos2::new(c.x + rx * t.cos(), c.y + ry * t.sin())
                }).collect();
                painter.add(egui::Shape::closed_line(e, st));
            }
            RedKind::Cloud => {
                // revision cloud: arcuri de-a lungul dreptunghiului
                let corners = [bx.left_top(), bx.right_top(), bx.right_bottom(), bx.left_bottom()];
                let bump = (12.0 + r.width * 2.0).max(8.0);
                let mut path = Vec::new();
                for k in 0..4 {
                    let p0 = corners[k];
                    let p1 = corners[(k + 1) % 4];
                    let len = (p1 - p0).length();
                    let n = ((len / bump).round() as usize).max(1);
                    let d = (p1 - p0) / n as f32;
                    let out = EV::new(d.y, -d.x).normalized();
                    for j in 0..n {
                        let s0 = p0 + d * j as f32;
                        let c = s0 + d * 0.5;
                        let rad = d.length() * 0.5;
                        for q in 0..=8 {
                            let t = q as f32 / 8.0 * std::f32::consts::PI;
                            let along = -d.normalized() * rad * t.cos();
                            path.push(c + along + out * rad * t.sin());
                        }
                    }
                }
                painter.add(egui::Shape::line(path, st));
            }
            RedKind::Text => {
                let fs = 13.0 + r.width * 2.0;
                let g = painter.layout_no_wrap(r.text.clone(), FontId::proportional(fs), col);
                let bg = Rect::from_min_size(a, g.size()).expand(3.0);
                painter.rect_filled(bg, 3.0, Color32::from_rgba_unmultiplied(255, 255, 255, 200));
                painter.rect_stroke(bg, 3.0, Stroke::new(1.0_f32, col));
                painter.galley(a, g, col);
            }
        }
    }
}

fn red_hit(r: &crate::markup::Redline, p: [f32; 2], tol: f32) -> bool {
    use crate::markup::RedKind;
    let d = |a: [f32; 2], b: [f32; 2]| -> f32 {
        let (vx, vy) = (b[0] - a[0], b[1] - a[1]);
        let l2 = vx * vx + vy * vy;
        let t = if l2 > 0.0 { (((p[0] - a[0]) * vx + (p[1] - a[1]) * vy) / l2).clamp(0.0, 1.0) } else { 0.0 };
        ((a[0] + vx * t - p[0]).powi(2) + (a[1] + vy * t - p[1]).powi(2)).sqrt()
    };
    match r.kind {
        RedKind::Text => (p[0] - r.pts[0][0]).abs() < tol * 8.0 && (p[1] - r.pts[0][1]).abs() < tol * 2.5 && p[0] >= r.pts[0][0] - tol,
        RedKind::Rect | RedKind::Cloud | RedKind::Ellipse => {
            let a = r.pts[0];
            let b = *r.pts.last().unwrap();
            let c = [[a[0], a[1]], [b[0], a[1]], [b[0], b[1]], [a[0], b[1]]];
            (0..4).any(|k| d(c[k], c[(k + 1) % 4]) < tol * 1.5)
        }
        _ => r.pts.windows(2).any(|w| d(w[0], w[1]) < tol),
    }
}

/// 8484455 -> "8.48 M", 12500 -> "12.5 k"
fn fmt_count(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.2} M", n as f64 / 1e6)
    } else if n >= 10_000 {
        format!("{:.1} k", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}

/// Eticheta si culoarea cardului unui fisier recent, dupa tip.
fn file_badge(path: &str) -> (&'static str, Color32) {
    let ext = std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "bimr" => ("BIMR", Color32::from_rgb(59, 130, 246)),
        "bimd" => ("BIMD", Color32::from_rgb(30, 90, 200)),
        "ifc" | "ifczip" => ("IFC", Color32::from_rgb(20, 150, 140)),
        "nwd" | "nwc" | "nwf" => ("N", Color32::from_rgb(37, 99, 235)),
        "rvt" => ("R", Color32::from_rgb(30, 110, 200)),
        "dwg" | "dxf" => ("DWG", Color32::from_rgb(200, 60, 55)),
        "e57" => ("E57", Color32::from_rgb(150, 100, 210)),
        "bimc" => ("BIMC", Color32::from_rgb(90, 110, 140)),
        "step" | "stp" | "iges" | "igs" | "brep" | "brp" | "fcstd" => ("CAD", Color32::from_rgb(220, 130, 40)),
        "fbx" | "glb" | "gltf" | "obj" | "stl" | "ply" => ("3D", Color32::from_rgb(110, 150, 70)),
        _ => ("FILE", Color32::from_rgb(110, 118, 130)),
    }
}

/// Data locala a unui fisier: "18 Nov 2026 19:50".
fn fmt_date(t: std::time::SystemTime) -> String {
    let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) + local_offset_secs();
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    // zile -> data civila (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{d} {} {y} {:02}:{:02}", M[(m - 1) as usize], tod / 3600, tod % 3600 / 60)
}

/// Decalajul fusului orar local (secunde), din Windows; 0 in alta parte.
pub(crate) fn local_offset_secs() -> i64 {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct Tzi {
            bias: i32,
            _rest: [u8; 168],
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GetTimeZoneInformation(tz: *mut Tzi) -> u32;
        }
        let mut tz = Tzi { bias: 0, _rest: [0; 168] };
        let r = unsafe { GetTimeZoneInformation(&mut tz) };
        // bias in minute: UTC = local + bias; vara (r == 2) se scade si DaylightBias (de obicei -60)
        let dst = if r == 2 { i32::from_le_bytes([tz._rest[164], tz._rest[165], tz._rest[166], tz._rest[167]]) } else { 0 };
        return -((tz.bias + dst) as i64) * 60;
    }
    #[allow(unreachable_code)]
    0
}

/// Textul unei restrictii de nume: "A, B" = una dintre valori, "/regex/" = model, gol = oricare.
fn value_text(v: &crate::check::Value) -> String {
    use crate::check::Value;
    match v {
        Value::Any => String::new(),
        Value::OneOf(l) => l.join(", "),
        Value::Pattern(p) => format!("/{p}/"),
        Value::Range { min, max } => format!("{}..{}", min.map(|x| x.to_string()).unwrap_or_default(), max.map(|x| x.to_string()).unwrap_or_default()),
    }
}

fn parse_value_text(s: &str) -> crate::check::Value {
    use crate::check::Value;
    let t = s.trim();
    if t.is_empty() {
        return Value::Any;
    }
    if t.len() >= 2 && t.starts_with('/') && t.ends_with('/') {
        return Value::Pattern(t[1..t.len() - 1].to_string());
    }
    if let Some((a, b)) = t.split_once("..") {
        let (a, b) = (a.trim(), b.trim());
        let pa = if a.is_empty() { Some(None) } else { a.replace(',', ".").parse::<f64>().ok().map(Some) };
        let pb = if b.is_empty() { Some(None) } else { b.replace(',', ".").parse::<f64>().ok().map(Some) };
        if let (Some(min), Some(max)) = (pa, pb) {
            if min.is_some() || max.is_some() {
                return Value::Range { min, max };
            }
        }
    }
    Value::OneOf(t.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
}

/// camp text legat de o restrictie (textul se pastreaza cat timp scrii, ca sa nu sara virgulele)
fn value_field(ui: &mut egui::Ui, v: &mut crate::check::Value, id: egui::Id, hint: &str, width: f32) -> bool {
    let mut s: String = ui.data_mut(|d| d.get_temp(id)).unwrap_or_else(|| value_text(v));
    if parse_value_text(&s) != *v {
        s = value_text(v);
    }
    let r = ui.add(egui::TextEdit::singleline(&mut s).hint_text(hint).desired_width(width)).on_hover_text("Text: exact value (or several, separated by commas) · /regex/: pattern · 10..20: number range · empty: any");
    let mut ch = false;
    if r.changed() {
        let nv = parse_value_text(&s);
        if nv != *v {
            *v = nv;
            ch = true;
        }
    }
    ui.data_mut(|d| d.insert_temp(id, s));
    ch
}

/// O conditie a unei reguli: tip / proprietate / atribut ... ; intoarce (modificat, de sters)
fn facet_editor(ui: &mut egui::Ui, f: &mut crate::check::Facet, salt: impl std::hash::Hash + Copy, requirement: bool) -> (bool, bool) {
    use crate::check::{Card, Kind};
    let mut ch = false;
    let mut rm = false;
    let id = ui.make_persistent_id(salt);
    egui::Frame::none().fill(ui.visuals().faint_bg_color).rounding(6.0).inner_margin(egui::Margin::symmetric(6.0, 4.0)).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt(id.with("k")).width(96.0).selected_text(f.kind.label()).show_ui(ui, |ui| {
                for k in Kind::ALL {
                    if ui.selectable_value(&mut f.kind, k, k.label()).changed() {
                        ch = true;
                    }
                }
            });
            match f.kind {
                Kind::Entity => {
                    ch |= value_field(ui, &mut f.name, id.with("n"), "IfcWall, IfcSlab or Walls", 170.0);
                    ch |= value_field(ui, &mut f.value, id.with("v"), "predefined type (optional)", 120.0);
                }
                Kind::Property => {
                    ch |= value_field(ui, &mut f.set, id.with("s"), "Pset (any)", 120.0);
                    ui.label(".");
                    ch |= value_field(ui, &mut f.name, id.with("n"), "property name", 120.0);
                }
                Kind::Attribute => ch |= value_field(ui, &mut f.name, id.with("n"), "Name, Description, Tag...", 150.0),
                Kind::Classification => ch |= value_field(ui, &mut f.set, id.with("s"), "system (any)", 120.0),
                Kind::Material => {}
            }
            if f.kind != Kind::Entity {
                ui.label("=");
                ch |= value_field(ui, &mut f.value, id.with("v"), "any value", 120.0);
            }
            if requirement {
                let lbl = |c: Card| match c {
                    Card::Required => "required",
                    Card::Optional => "optional",
                    Card::Prohibited => "prohibited",
                };
                egui::ComboBox::from_id_salt(id.with("c")).width(84.0).selected_text(lbl(f.card)).show_ui(ui, |ui| {
                    for c in [Card::Required, Card::Optional, Card::Prohibited] {
                        if ui.selectable_value(&mut f.card, c, lbl(c)).changed() {
                            ch = true;
                        }
                    }
                });
                if matches!(f.kind, Kind::Property | Kind::Attribute) {
                    ch |= ui.checkbox(&mut f.unique, "unique").on_hover_text("The value must be unique among the checked elements (e.g. Mark, door number, GlobalId) - duplicates fail").changed();
                }
            }
            if ui.small_button("✖").on_hover_text("Remove").clicked() {
                rm = true;
            }
        });
    });
    (ch, rm)
}

/// Rand de arbore / lista in stilul redesign-ului: punct colorat (categorie), text, insigna cu numarul
/// elementelor la dreapta; selectat = nuanta de accent + bara de 2 px pe stanga; hover = fundal mai deschis.
fn tree_row(ui: &mut egui::Ui, label: &str, count: Option<usize>, dot: Option<Color32>, selected: bool, strong: bool, weak: bool) -> egui::Response {
    let h = 21.0;
    // panourile laterale sunt intr-un ScrollArea orizontal: latimea vizibila, nu cea "infinita"
    let w = (ui.clip_rect().right() - ui.cursor().left() - 4.0).min(ui.available_width()).max(60.0);
    let (rect, resp) = ui.allocate_exact_size(EV::new(w, h), Sense::click());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let v = ui.visuals();
    let p = ui.painter();
    // tranzitie lina la hover
    let hov = ui.ctx().animate_bool_with_time(resp.id.with("hov"), resp.hovered(), 0.12);
    if selected {
        p.rect_filled(rect, 5.0, accent().gamma_multiply(0.16));
        p.rect_filled(Rect::from_min_size(rect.left_top() + EV::new(0.0, 3.0), EV::new(2.0, h - 6.0)), 1.0, accent());
    } else if hov > 0.0 {
        p.rect_filled(rect, 5.0, v.widgets.hovered.weak_bg_fill.gamma_multiply(hov));
    }
    let mut x = rect.left() + 8.0;
    if let Some(c) = dot {
        p.circle_filled(Pos2::new(x + 3.0, rect.center().y), 3.5, c);
        x += 12.0;
    }
    let col = if selected { v.strong_text_color() } else if weak { v.weak_text_color() } else if strong { v.strong_text_color() } else { v.text_color() };
    let mut right = rect.right() - 4.0;
    if let Some(n) = count {
        let t = thousands(n);
        let g = ui.fonts(|f| f.layout_no_wrap(t, FontId::proportional(11.0), v.weak_text_color()));
        let bw = g.size().x + 10.0;
        let br = Rect::from_min_size(Pos2::new(right - bw, rect.center().y - 8.0), EV::new(bw, 16.0));
        p.rect_filled(br, 5.0, card_fill(v));
        p.galley(br.center() - g.size() / 2.0, g, v.weak_text_color());
        right = br.left() - 6.0;
    }
    let font = FontId::proportional(13.0);
    let g = ui.fonts(|f| {
        let mut job = egui::text::LayoutJob::simple_singleline(label.to_string(), font.clone(), col);
        job.wrap = egui::text::TextWrapping::truncate_at_width((right - x).max(10.0));
        f.layout_job(job)
    });
    p.galley(Pos2::new(x, rect.center().y - g.size().y / 2.0), g, col);
    let _ = strong;
    resp
}

/// culoare stabila pentru o categorie / tip (acelasi nume = aceeasi culoare peste tot)
fn cat_color(label: &str) -> Color32 {
    let mut h: u32 = 2166136261;
    for b in label.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    let c = crate::profiler::palette(h as usize % 12);
    Color32::from_rgb(c[0], c[1], c[2])
}

// ===================================================================== paleta de comenzi (Ctrl+K)

/// O comanda din paleta: nume, categorie, iconita, cuvinte cheie (si in romana), actiunea.
struct Cmd {
    name: &'static str,
    cat: &'static str,
    icon: &'static str,
    keys: &'static str,
    run: fn(&mut App),
}

fn commands() -> Vec<Cmd> {
    macro_rules! c {
        ($name:expr, $cat:expr, $icon:expr, $keys:expr, $run:expr) => {
            Cmd { name: $name, cat: $cat, icon: $icon, keys: $keys, run: $run }
        };
    }
    vec![
        // ---- fisier / proiect
        c!("Open model…", "File", "📂", "deschide open ifc rvt nwd dwg e57", |a| a.request_path(Purpose::OpenModel { append: false })),
        c!("Append model…", "File", "➕", "adauga append federate", |a| a.request_path(Purpose::OpenModel { append: true })),
        c!("Open project…", "File", "📁", "proiect bimr", |a| a.request_path(Purpose::OpenProject)),
        c!("Save project", "File", "💾", "salveaza save ctrl+s", |a| match a.project_path.clone() {
            Some(p) => a.save_project(&p),
            None => a.request_path(Purpose::SaveProject),
        }),
        c!("Save project as…", "File", "💾", "salveaza ca save as", |a| a.request_path(Purpose::SaveProject)),
        c!("New project", "File", "📄", "nou new", |a| a.new_project()),
        c!("Settings…", "View", "⚙", "setari settings font dpi dimensiune fonturi optiuni", |a| a.settings_open = true),
        c!("Cut section to PDF…", "Export", "📄", "sectiune pdf plansa", |a| a.request_path(Purpose::ExportSectionPdf)),
        c!("Script console", "Script", "📜", "script rhai consola", |a| a.script_open = true),
        c!("Publish project (.bimd)…", "File", "📦", "publica nwd bimd", |a| a.request_path(Purpose::PublishBimd)),
        c!("Refresh changed models", "File", "⟳", "reincarca refresh f5", |a| a.refresh_models(false)),
        c!("Start page", "File", "🏠", "pagina start recente recent", |a| a.show_start = true),
        // ---- selectie
        c!("Select all", "Select", "⬚", "selecteaza tot ctrl+a", |a| {
            let all = a.all_ids();
            a.apply(Action::Select(all, false));
        }),
        c!("Clear selection", "Select", "🔲", "deselecteaza", |a| a.clear_selection()),
        c!("Undo", "Select", "⟲", "inapoi anuleaza ctrl+z selectie ascunse camera proprietati culori materiale", |a| a.undo_view()),
        c!("Redo view", "Select", "⟳", "refa inainte ctrl+y", |a| a.redo_view()),
        c!("Select same type", "Select", "⧉", "similar acelasi tip ctrl+shift+a", |a| a.select_similar(true)),
        c!("Select same category", "Select", "▦", "similar aceeasi categorie", |a| a.select_similar(false)),
        c!("Window selection", "Select", "⬚", "fereastra dreptunghi", |a| a.tool = Tool::Window),
        c!("Hide selected", "Visibility", "🚫", "ascunde hide h", |a| {
            let s: Vec<ElemId> = a.selected.iter().copied().collect();
            a.apply(Action::Hide(s));
        }),
        c!("Isolate selected (hide unselected)", "Visibility", "◎", "izoleaza isolate i", |a| {
            let s: Vec<ElemId> = a.selected.iter().copied().collect();
            a.apply(Action::Isolate(s));
        }),
        c!("Unhide all", "Visibility", "👁", "arata tot show all", |a| a.apply(Action::ShowAll)),
        c!("Focus on selection", "View", "🎯", "zoom selectie f", |a| {
            let s: Vec<ElemId> = a.selected.iter().copied().collect();
            a.apply(Action::Focus(s));
        }),
        // ---- vizualizare
        c!("Fit all", "View", "⛶", "zoom extents tot modelul", |a| {
            let bb = a.scene_aabb();
            let mut t = a.cam;
            t.fit(&bb);
            a.fly_to(t);
        }),
        c!("Home view", "View", "🏠", "acasa home", |a| a.go_home()),
        c!("Perspective / orthographic", "View", "🔭", "ortho perspectiva", |a| a.cam.ortho = !a.cam.ortho),
        c!("Path trace image", "Render", "◎", "path trace renderare progresiva cancel export png", |a| {
            a.pathtrace_open = true;
        }),
        c!("Visual style: Shaded", "View", "◆", "stil umbrit shaded", |a| a.set_visual_style(crate::render::VisualStyle::Shaded)),
        c!("Visual style: Realistic", "View", "☀", "stil realist materiale texturi realistic", |a| a.set_visual_style(crate::render::VisualStyle::Realistic)),
        c!("Visual style: Concept", "View", "✏", "stil conceptual alb margini concept hidden line", |a| a.set_visual_style(crate::render::VisualStyle::Concept)),
        c!("Visual style: Wireframe", "View", "▦", "stil fir de sarma wireframe", |a| a.set_visual_style(crate::render::VisualStyle::Wireframe)),
        c!("Visual style: Hidden line", "View", "◫", "hidden line: suprafete neutre, muchiile ascunse nu se vad", |a| a.set_visual_style(crate::render::VisualStyle::Hidden)),
        c!("Theme: Studio", "View", "◆", "tema", |a| a.set_theme(Theme::Studio)),
        c!("Theme: Dark", "View", "🌙", "tema intunecata", |a| a.set_theme(Theme::Dark)),
        c!("Theme: Light", "View", "☀", "tema luminoasa", |a| a.set_theme(Theme::Light)),
        c!("Theme: Tokyo Night", "View", "🌃", "tema", |a| a.set_theme(Theme::TokyoNight)),
        c!("Theme: Nord", "View", "❄", "tema", |a| a.set_theme(Theme::Nord)),
        c!("Ribbon: Full / Compact / Mini", "View", "≡", "ribbon panglica marime", |a| {
            a.settings.ribbon_mode = a.settings.ribbon_mode.next();
            a.settings.save();
        }),
        c!("Hide / show all panels (F11)", "View", "🗔", "panouri ecran complet", |a| a.settings.layout.hide_all = !a.settings.layout.hide_all),
        // ---- navigare / unelte
        c!("Orbit", "Navigate", "🔄", "roteste", |a| {
            a.nav = NavMode::Orbit;
            a.tool = Tool::Select;
        }),
        c!("Pan", "Navigate", "✋", "deplaseaza", |a| a.nav = NavMode::Pan),
        c!("Walk", "Navigate", "🚶", "mers plimbare", |a| a.nav = NavMode::Walk),
        c!("Measure point to point", "Review", "📏", "masoara distanta", |a| a.tool = Tool::Measure(MeasureKind::PointToPoint)),
        c!("Markup (cloud)", "Review", "✏", "redline nor adnotare", |a| a.tool = Tool::Redline(crate::markup::RedKind::Cloud)),
        // ---- sectiuni
        c!("Section box around selection", "Sections", "⬚", "sectiune selectie box", |a| a.section_to_selection()),
        c!("Section off", "Sections", "✂", "opreste sectiunea", |a| a.section.enabled = false),
        c!("Section cut to DWG / DXF (2D in model)…", "Sections", "📐", "taietura plan autocad dwg dxf export model space", |a| {
            a.cad_sheet = false;
            a.request_path(Purpose::ExportSectionCad);
        }),
        c!("Section cut on sheet (DWG)…", "Sections", "📄", "plansa cartus a3 a2 a1 scara dwg", |a| {
            a.cad_sheet = true;
            a.request_path(Purpose::ExportSectionCad);
        }),
        c!("Language: Română / English", "View", "🌐", "limba romana engleza interfata language", |a| a.toggle_language()),
        c!("Title block & sheet format", "Annotation", "✏", "cartus format a3 a2 a1 a0 scara editare", |a| a.title_block_open = true),
        c!("Classification / bSDD", "BIM", "🏷", "clasificare bsdd uniclass omniclass cod etim buildingsmart", |a| a.class_open = true),
        c!("Check for updates", "Help", "⬇", "update actualizare versiune noua github", |a| a.check_updates(true)),
        c!("DWG overlay (2D drawing over the model)", "Annotation", "🗺", "suprapunere plan dwg dxf 2d", |a| a.overlay_open = true),
        c!("Save view template", "View", "➕", "save view template vizibilitate sectiune stil", |a| a.save_view_template()),
        c!("Apply view template", "View", "◫", "apply view template manage vizibilitate sectiune stil", |a| a.view_templates_open = !a.view_templates_open),
        c!("2D Section Preview (map window)", "Annotation", "🗺", "preview 2d sectiune fereastra mica inainte de export", |a| a.section_preview_open = !a.section_preview_open),
        c!("Section cut to PDF…", "Annotation", "📄", "sectiune export pdf plansa", |a| a.request_path(Purpose::ExportSectionPdf)),
        // ---- panouri
        c!("Selection Tree", "Panels", "🌲", "arbore", |a| a.open_panel(Panel::Tree)),
        c!("Properties", "Panels", "ℹ", "proprietati", |a| a.open_panel(Panel::Properties)),
        c!("Find Items", "Panels", "🔍", "cauta gaseste ctrl+f", |a| a.open_panel(Panel::Find)),
        c!("Sets", "Panels", "📋", "seturi", |a| a.open_panel(Panel::Sets)),
        c!("Clash Detective", "Coordination", "⚠", "clash coliziuni", |a| a.open_panel(Panel::Clash)),
        c!("Model Checker (rules / IDS)", "Coordination", "✔", "verificare reguli ids", |a| a.open_panel(Panel::Check)),
        c!("TimeLiner", "Coordination", "📅", "4d planificare", |a| a.open_panel(Panel::TimeLiner)),
        c!("Viewpoints & issues", "Review", "📷", "vederi issue", |a| a.open_panel(Panel::Viewpoints)),
        c!("Quantification", "Analyze", "📊", "cantitati qto", |a| a.open_panel(Panel::Quantities)),
        c!("Compare versions", "Analyze", "⚖", "compara diferente", |a| a.open_panel(Panel::Compare)),
        c!("Appearance Profiler", "Analyze", "🎨", "culori dupa proprietate", |a| a.open_panel(Panel::Profiler)),
        c!("Scan vs Model (as-built)", "Analyze", "☁", "nor de puncte as built scanare", |a| a.asbuilt_open = true),
        c!("Coordinates / move model", "Analyze", "📐", "muta roteste relocare", |a| a.open_panel(Panel::Coords)),
        c!("Render & materials", "Render", "🎬", "randare materiale lumina", |a| a.open_panel(Panel::Render)),
        c!("Animations (play, export video)", "Render", "🎞", "animatie video orbit record 4d", |a| a.open_panel(Panel::Animation)),
        // ---- export / share
        c!("Export IFC4…", "Export", "🏗", "ifc export", |a| {
            a.ifc_selection = false;
            a.request_path(Purpose::ExportIfc);
        }),
        c!("Export glTF / GLB (Blender, Rhino, Twinmotion)…", "Export", "📦", "gltf glb blender rhino twinmotion unreal enscape export", |a| {
            a.ifc_selection = !a.selected.is_empty();
            a.request_path(Purpose::ExportGlb);
        }),
        c!("Web viewer (HTML)…", "Share", "🌐", "web html browser", |a| {
            a.web_selection = !a.selected.is_empty();
            a.request_path(Purpose::ExportWeb);
        }),
        c!("PDF sheets from viewpoints…", "Export", "📄", "planse pdf", |a| a.request_path(Purpose::ExportVpPdf)),
        c!("Viewpoints report (HTML)…", "Export", "📄", "raport", |a| a.request_path(Purpose::ExportVpHtml)),
        c!("Viewpoints report (Excel)…", "Export", "📊", "raport excel", |a| a.request_path(Purpose::ExportVpXlsx)),
        c!("Issues to BCF…", "Export", "📤", "bcf issue", |a| a.request_path(Purpose::ExportBcf)),
        c!("Import BCF…", "Export", "📥", "bcf import", |a| a.request_path(Purpose::ImportBcf)),
        c!("Reference for Revit / AutoCAD…", "Share", "🔗", "mbref referinta", |a| a.request_path(Purpose::ExportReference)),
        c!("Render image…", "Render", "🖼", "imagine png", |a| a.request_path(Purpose::RenderImage)),
        // ---- coordonare
        c!("Live Link: sync selection on / off", "Coordination", "🔗", "revit navisworks sincronizare selectie", |a| {
            let on = !a.settings.sync_selection;
            a.set_sync(on);
        }),
        c!("Run model check", "Coordination", "✔", "verifica reguli", |a| a.run_check()),
        c!("Import IDS…", "Coordination", "📥", "ids buildingsmart", |a| a.request_path(Purpose::ImportCheckRules)),
        c!("Export IDS…", "Coordination", "📤", "ids buildingsmart export cerinte", |a| a.request_path(Purpose::ExportIds)),
        c!("Clash matrix (Excel)…", "Coordination", "▦", "matrice clash", |a| a.request_path(Purpose::ExportClashMatrix)),
        // ---- ajutor
        c!("User manual", "Help", "❓", "manual ajutor help documentatie", |a| a.manual_open = true),
        c!("Open demo project", "Help", "🏗", "demo exemplu proiect", |a| a.open_demo()),
    ]
}

/// potrivire "fuzzy": toate literele cautarii, in ordine; scor mai mare pentru cuvinte intregi / inceputuri
fn fuzzy(q: &str, text: &str) -> Option<i32> {
    if q.is_empty() {
        return Some(0);
    }
    let t = text.to_lowercase();
    let mut score = 0;
    for w in q.split_whitespace() {
        if let Some(p) = t.find(w) {
            score += 100 - (p as i32).min(60) + if p == 0 || t.as_bytes().get(p - 1) == Some(&b' ') { 40 } else { 0 };
            continue;
        }
        // subsecventa
        let mut it = t.chars();
        let mut gaps = 0;
        for c in w.chars() {
            let mut found = false;
            for x in it.by_ref() {
                if x == c {
                    found = true;
                    break;
                }
                gaps += 1;
            }
            if !found {
                return None;
            }
        }
        score += 30 - gaps.min(29);
    }
    Some(score)
}

impl App {
}

// ===================================================================== manualul integrat

/// Capitolele manualului: (iconita, titlu, text). Randurile care incep cu "- " sunt liste,
/// cele care incep cu "# " sunt subtitluri, "> " = sfat.
const MANUAL: [(&str, &str, &str); 14] = [
    ("🏠", "Getting started", "ManageBIM opens IFC, Revit (through the add-in), Navisworks NWD / NWC, DWG / DXF, STEP, glTF, FBX, OBJ, E57 / LAS point clouds and more.
# First steps
- Start page: New project, Open (a model or a .bimr project) or a recent file.
- Append (Home › Append) adds more models to the same project: architecture, structure, MEP, point clouds.
- Save (Ctrl+S) writes a .bimr project: models, viewpoints, sets, clash tests, rules, materials. Publish (.bimd) packs everything into one file to send.
- Help › Open demo project loads Block A (architecture + structure, MEP and a 4D schedule) to try everything.
> Ctrl+K opens the command palette: type what you want (\"clash\", \"section\", \"pdf\") or the name of an element and press Enter."),
    ("🔄", "Navigation", "# Mouse
- Left drag: orbit around the point under the cursor.
- Middle drag or Shift + left drag: pan. Wheel: zoom towards the cursor.
- Double-click an element: zoom to it. Home key: home view. F: focus on the selection.
# Walk
- Walk mode (bottom bar): drag up = forward, sideways = turn; W A S D also work. Draw a walk path and play it.
# ViewCube and projection
- Click a face / edge / corner of the cube for standard views. Persp / Ortho switches the projection.
> In large models small objects are hidden while you move (adaptive LOD) and come back when the camera stops."),
    ("👆", "Selection & visibility", "- Click selects, Ctrl+click adds / removes. Window: drag left -> right = inside, right -> left = touching.
- Select › Same Type (Ctrl+Shift+A) selects every visible element of the same type; Same Category takes the whole category.
- Hide (H), Hide Unselected (I), Unhide All. The eye icons in the Selection Tree hide groups.
- Find Items: conditions as chips (Level = 2, Type = Wall...), Find All; save them as a search set (Sets).
- Selection Tree: group by any property (Group by), search the model, double-click to zoom."),
    ("◆", "Visual styles & appearance", "- Visual style (View tab, or Ctrl+K): Realistic (materials, textures, reflections), Shaded (file colours), Concept (white study model with strong edges), Wireframe.
- Render › Edges and Contact shadows set the fine outlines and the soft shadows.
- Appearance Profiler colours the model by any property (level, system, fire rating...).
- Materials: library, PBR packs (.zip from freepbr.com, ambientCG...), saved in your own library for every project.
- Themes: Studio, Dark, Light, Tokyo Night, Nord (chip in the top right). The ribbon has three sizes: Full, Compact, Mini."),
    ("✂", "Sections & levels", "- Status bar › Level: pick a level to cut the model at that level (plan view), All levels to turn it off.
- Sections tab: section box around the selection (Fit Section), Model box with stretch arrows, section planes.
- Cut to DWG: the cut of the current section as 2D lines on an A3 sheet with a title block, at a standard scale (1:50 / 1:100 / 1:200), one layer per element type. DWG (AutoCAD 2018) is written directly, no other program needed; DXF is also available.
- Grids and levels: from IFC, from DWG / DXF, or created in the app; clash locations use them (C-4 / Level 2)."),
    ("📏", "Measure & relocate", "- Measure: point to point, chained, angle, area; snaps to corners, midpoints and edges. Units m / mm in the status bar.
- Relocate (Analyze tab): move, rotate and align models or elements - from a point to a point, along an edge, onto a surface.
- Coordinates: shared coordinates / origin, reset moves."),
    ("⚠", "Clash Detective", "- Add a test: selection A against selection B (models, sets, types), hard clash or clearance with a tolerance.
- Run: results grouped by level, grid location, item or type; status New / Active / Reviewed / Approved / Resolved is kept between runs.
- Clash matrix: open clashes per pair of models, Excel export.
- Reports: HTML, CSV, BCF (Revit, ACC, Solibri, BIMcollab), Navisworks viewpoints XML."),
    ("✔", "Model Checker (rules & IDS)", "- A rule = which elements it applies to (type, property, attribute) + what they must have (property present, exact value, list, /pattern/, range 10..20, optional or prohibited).
- Import buildingSMART IDS 1.0 files; start from the starter rules; save rules as .mbrules (also saved in the project).
- Run check: pass / fail per rule, failing elements with the reason, Select / Isolate, Excel and BCF reports."),
    ("🔗", "Live Link with Revit & Navisworks", "- In ManageBIM: Select › Live Link › Sync Selection.
- In Revit: ManageBIM tab › Sync Selection. In Navisworks: Tool Add-ins › Sync Selection.
- Select in one program and the same elements are selected (and zoomed to) in the others. Elements are matched by UniqueId / IFC GUID, so the ManageBIM model must come from the same Revit model (Send / Export BIMC or its IFC).
- Revit / AutoCAD / Plant 3D reference: Share › Reference (Revit / CAD) brings the ManageBIM model into the authoring tool as light reference geometry (Load / Reload / Unload)."),
    ("☁", "Point clouds & as-built", "- Open or Append E57 / LAS scans; point size and density in Render.
- Analyze › Scan vs Model: every visible element is sampled on its surface and compared with the scan: Built, Partial, Missing or Not scanned, with colours on the model, Select / Isolate and an Excel report.
> Hidden faces (backs, insides) are never scanned, so the 'Built' threshold is intentionally low (30 %)."),
    ("📅", "TimeLiner (4D) & quantities", "- TimeLiner: tasks with dates, attached to sets or selections; import / export CSV; simulation with a date slider and video export.
- Quantification: counts, volumes, areas and lengths grouped by any property, with Excel export and templates.
- Compare: two versions of a model - added, removed, moved and changed elements."),
    ("📷", "Viewpoints, issues & markups", "- Save viewpoints with the camera, section, hidden elements and markups (clouds, arrows, text).
- Issues: status, priority, responsible, comments; shared issues folder for the team (network / SharePoint).
- Reports: HTML, Excel, BCF, and PDF Sheets - one A3 sheet per viewpoint with the image, the description, the comments and a title block."),
    ("🌐", "Export & share", "- IFC4 export of the visible elements or of the selection (geometry, colours, levels, GUIDs, all properties).
- Web viewer: one .html file with the model, tree, search, properties, hide / isolate and X-ray - opens offline in any browser.
- .bimd: the whole project in one file. Navisworks XML (viewpoints, sets, clash tests), BCF, Excel / CSV, images and video."),
    ("⌨", "Keyboard shortcuts", "- Ctrl+K: command palette        Ctrl+F: Find Items
- Ctrl+O: open        Ctrl+S: save        F5: refresh changed models
- Ctrl+A: select all        Ctrl+Shift+A: select same type        Esc: clear / cancel
- H: hide        I: hide unselected        F: focus        Home: home view
- W A S D / arrows: walk        PgUp / PgDn: level above / below        F11: hide all panels"),
];

impl App {
}

impl App {
}

impl App {

}

impl App {

}

/// distanta (px) de la punctul `p` la segmentul `a`-`b` de pe ecran
fn seg_dist_px(a: Pos2, b: Pos2, p: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t).distance(p)
}
