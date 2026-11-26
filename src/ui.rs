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
mod levels;
mod ribbon;
mod viewport;
mod start_page;
mod docking;
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
        self.dashboard_step(ctx);
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
                let active_right = pointer.is_some_and(|p| p.x >= rect.min.x + half);
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
                let active_b = ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| p.x >= full.min.x + full.width() * self.split_ratio.clamp(0.05, 0.95));
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

    // ================================================================== Compare

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
        self.levels_manager(ctx);
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
        if i > 0 && (s.len() - i).is_multiple_of(3) {
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
                if best.as_ref().is_none_or(|x| worst < x.0) {
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
