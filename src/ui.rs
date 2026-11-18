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
        crate::app::set_units_mm(self.settings.units_mm);
        self.poll_jobs();
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
        self.update_sim(dt);

        // fisiere trase in fereastra
        let dropped: Vec<String> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.as_ref().map(|p| p.to_string_lossy().to_string())).collect());
        for p in dropped {
            self.open_path(&p, true);
        }
        self.shortcuts(ctx);

        self.settings.layout.fixup();
        self.ribbon(ctx);
        // pagina de start: doar ribbon-ul (inactiv) si ecranul de pornire pe toata fereastra
        if self.start_screen() {
            egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| self.viewport(ui));
            self.tools_window(ctx);
            self.windows(ctx);
            self.toasts_ui(ctx);
            if !self.toasts.is_empty() {
                ctx.request_repaint();
            }
            return;
        }
        self.status_bar(ctx);
        let maxh = (ctx.screen_rect().height() * 0.65).max(200.0);
        let side_max = (ctx.screen_rect().width() * 0.38).max(260.0);
        if !self.settings.layout.in_area(Dock::Bottom).is_empty() {
            egui::TopBottomPanel::bottom("bottom").resizable(true).default_height(220.0).min_height(100.0).max_height(maxh).show(ctx, |ui| self.dock_area(ui, Dock::Bottom));
        }
        if !self.settings.layout.in_area(Dock::Left).is_empty() {
            egui::SidePanel::left("left").resizable(true).default_width(250.0).min_width(180.0).max_width(side_max).show(ctx, |ui| self.dock_area(ui, Dock::Left));
        }
        if !self.settings.layout.in_area(Dock::Right).is_empty() {
            egui::SidePanel::right("right").resizable(true).default_width(270.0).min_width(190.0).max_width(side_max).show(ctx, |ui| self.dock_area(ui, Dock::Right));
        }
        egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| self.viewport(ui));
        // mers: tastele (shortcuts) si mouse-ul (viewport) au setat viteza tinta - aplicam lin
        self.walk_tick(dt);
        self.section_follow_tick();
        self.float_windows(ctx);
        self.windows(ctx);
        self.tools_window(ctx);
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
        let (ctrl, o, s, f, h, i, esc, a, del) = ctx.input(|i| {
            (i.modifiers.command, i.key_pressed(Key::O), i.key_pressed(Key::S), i.key_pressed(Key::F), i.key_pressed(Key::H), i.key_pressed(Key::I), i.key_pressed(Key::Escape), i.key_pressed(Key::A), i.key_pressed(Key::Delete))
        });
        if ctrl && o {
            self.request_path(Purpose::OpenModel { append: false });
        }
        if ctrl && f {
            self.open_panel(Panel::Find);
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
        if typing {
            return;
        }
        if ctrl && a {
            let all = self.all_ids();
            self.apply(Action::Select(all, false));
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
        match self.project_path.clone() {
            Some(p) => self.save_project(&p),
            None => self.request_path(Purpose::SaveProject),
        }
    }

    fn file_menu(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(250.0);
        if ui.button("📂 Open Model…  (Ctrl+O)").clicked() {
            ui.close_menu();
            self.request_path(Purpose::OpenModel { append: false });
        }
        if ui.button("➕ Append Model…").clicked() {
            ui.close_menu();
            self.request_path(Purpose::OpenModel { append: true });
        }
        if ui.button("⟳ Refresh Changed Models  (F5)").on_hover_text("Reload the models whose files were modified on disk").clicked() {
            ui.close_menu();
            self.refresh_models(false);
        }
        if ui.button("⟳ Reload All Models  (Ctrl+F5)").clicked() {
            ui.close_menu();
            self.refresh_models(true);
        }
        ui.separator();
        if ui.button("📁 Open Project (.bimr)…").clicked() {
            ui.close_menu();
            self.request_path(Purpose::OpenProject);
        }
        if ui.button("💾 Save Project  (Ctrl+S)").clicked() {
            ui.close_menu();
            self.save_now();
        }
        if ui.button("Save Project As…").clicked() {
            ui.close_menu();
            self.request_path(Purpose::SaveProject);
        }
        ui.separator();
        ui.menu_button("Recent", |ui| {
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
            if ui.button("Clear list").clicked() {
                self.settings.recent.clear();
                self.settings.save();
                ui.close_menu();
            }
        });
        ui.separator();
        if ui.button("📦 Export Model Cache (.bimc)").on_hover_text("Native compact format with all geometry and properties (like NWC), written next to the sources.").clicked() {
            ui.close_menu();
            self.export_all_caches();
        }
        ui.separator();
        if ui.button("New Project (close all)").clicked() {
            ui.close_menu();
            self.reset();
        }
        if ui.button("Exit").clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn ribbon(&mut self, ctx: &egui::Context) {
        const TABS: [&str; 7] = ["Home", "Viewpoint", "Review", "Coordination", "Render", "Output", "View"];
        egui::TopBottomPanel::top("ribbon").frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(6.0, 3.0))).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.style_mut().spacing.item_spacing.x = 2.0;
                // accentul Nord e deschis (cyan): text inchis pentru contrast
                let ftxt = match self.settings.theme { Theme::Nord => Color32::from_rgb(46, 52, 64), Theme::TokyoNight => Color32::from_rgb(26, 27, 38), _ => Color32::WHITE };
                let fb = egui::Button::new(RichText::new("  ☰ File  ").strong().color(ftxt)).fill(accent());
                let r = ui.add(fb);
                let popup = ui.make_persistent_id("file_menu");
                if r.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(popup));
                }
                egui::popup_below_widget(ui, popup, &r, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| self.file_menu(ui));
                ui.add_space(6.0);
                for (i, t) in TABS.iter().enumerate() {
                    let on = self.ribbon_tab == i && !self.settings.ribbon_min;
                    let txt = if on { RichText::new(*t).strong() } else { RichText::new(*t) };
                    let r = ui.selectable_label(on, txt);
                    if r.double_clicked() {
                        // ca in Office: dublu-click pe tab restrange / extinde ribbon-ul
                        self.settings.ribbon_min = !self.settings.ribbon_min;
                        self.settings.save();
                    } else if r.clicked() {
                        if self.settings.ribbon_min {
                            self.settings.ribbon_min = false;
                            self.settings.save();
                        }
                        self.ribbon_tab = i;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (ic, tip) = if self.settings.ribbon_min { ("⏷", "Show the ribbon (or click a tab)") } else { ("⏶", "Hide the ribbon - more room for the model (double-click a tab does the same)") };
                    if ui.button(RichText::new(ic).size(13.0)).on_hover_text(tip).clicked() {
                        self.settings.ribbon_min = !self.settings.ribbon_min;
                        self.settings.save();
                    }
                    let next = self.settings.theme.next();
                    let icon = match next {
                        Theme::Nord => "❄",
                        Theme::TokyoNight => "🌃",
                        Theme::Light => "☀",
                        Theme::Dark => "🌙",
                    };
                    if ui.button(RichText::new(icon).size(14.0)).on_hover_text(format!("Theme: {} - click for {}", self.settings.theme.label(), next.label())).clicked() {
                        self.set_theme(next);
                    }
                    ui.label(RichText::new("ManageBIM").strong().color(accent()));
                    if let Some(p) = &self.project_path {
                        ui.label(RichText::new(crate::file_name(p)).weak());
                    }
                });
            });
            if self.settings.ribbon_min {
                return;
            }
            ui.add_space(2.0);
            // inaltime FIXA pe toate tab-urile (nu mai "sare" cand schimbi tab-ul); controalele stivuite
            // (checkbox-uri, slidere) folosesc randuri compacte ca sa incapa in 3 randuri
            const RIBBON_H: f32 = 62.0;
            egui::ScrollArea::horizontal().id_salt("ribbon_scroll").max_height(RIBBON_H).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(RIBBON_H);
                    ui.set_max_height(RIBBON_H);
                    ui.spacing_mut().interact_size.y = 16.0;
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.spacing_mut().button_padding = egui::vec2(6.0, 1.0);
                    // pagina de start: ribbon-ul se vede, dar e inactiv pana se deschide un model / proiect
                    if self.start_screen() {
                        ui.disable();
                    }
                    match self.ribbon_tab {
                        0 => self.ribbon_home(ui),
                        1 => self.ribbon_viewpoint(ui),
                        2 => self.ribbon_review(ui),
                        3 => self.ribbon_coord(ui),
                        4 => self.ribbon_render(ui),
                        5 => self.ribbon_output(ui),
                        _ => self.ribbon_view(ui),
                    }
                });
            });
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
            if rbtn(ui, "💾", "Save", "Save the .bimr project  Ctrl+S", false, true).clicked() {
                self.save_now();
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
        });
        rgroup(ui, "Select & Search", |ui| {
            if rbtn(ui, "👆", "Select", "Selection tool (Esc)", self.tool == Tool::Select, true).clicked() {
                self.cur_measure.clear();
                self.tool = Tool::Select;
            }
            if rbtn(ui, "⬚", "Select All", "Select every visible element  Ctrl+A", false, true).clicked() {
                let all = self.all_ids();
                self.apply(Action::Select(all, false));
            }
            let has = !self.selected.is_empty();
            if rbtn(ui, "🔲", "Clear", "Clear the selection", false, has).clicked() {
                self.clear_selection();
            }
            self.panel_btn(ui, Panel::Find, "🔍", "Find Items", "Search by any property, save as search set  Ctrl+F");
            self.panel_btn(ui, Panel::Sets, "📋", "Sets", "Selection & search sets");
        });
        self.visibility_group(ui);
        rgroup(ui, "Display", |ui| {
            self.panel_btn(ui, Panel::Tree, "🌲", "Tree", "Selection tree");
            self.panel_btn(ui, Panel::PropTree, "🔎", "Prop Tree", "Browse all properties of all models");
            self.panel_btn(ui, Panel::Properties, "ℹ", "Properties", "Properties of the selected element");
            self.panel_btn(ui, Panel::Profiler, "🎨", "Profiler", "Appearance Profiler: colour the model by any property");
        });
    }

    // ================================================================== Appearance Profiler
    fn profiler_ui(&mut self, ui: &mut egui::Ui) {
        use crate::profiler::{ProfileMode, Unmatched};
        if self.loaded_models().next().is_none() {
            ui.label(RichText::new("No models loaded.").weak());
            return;
        }
        let cats: Vec<String> = self.prop_index.cats.iter().map(|c| c.0.clone()).collect();
        let props: Vec<String> = self.prop_index.props(&self.profile.cat).map(|v| v.iter().map(|p| p.name.clone()).collect()).unwrap_or_default();
        let mut regen = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("Category");
            egui::ComboBox::from_id_salt("prof_cat").width(150.0).selected_text(&self.profile.cat).show_ui(ui, |ui| {
                for c in &cats {
                    if ui.selectable_label(*c == self.profile.cat, c).clicked() && *c != self.profile.cat {
                        self.profile.cat = c.clone();
                        // prima proprietate din categoria noua
                        self.profile.prop = self.prop_index.props(c).and_then(|v| v.first()).map(|p| p.name.clone()).unwrap_or_default();
                    }
                }
            });
            ui.label("Property");
            egui::ComboBox::from_id_salt("prof_prop").width(180.0).selected_text(&self.profile.prop).show_ui(ui, |ui| {
                for p in &props {
                    ui.selectable_value(&mut self.profile.prop, p.clone(), p);
                }
            });
            ui.separator();
            ui.radio_value(&mut self.profile.mode, ProfileMode::Values, "By value");
            ui.radio_value(&mut self.profile.mode, ProfileMode::Ranges, "By range");
            if self.profile.mode == ProfileMode::Ranges {
                ui.add(egui::DragValue::new(&mut self.profile.bins).range(2..=20).suffix(" ranges"));
            }
            if ui.button(RichText::new("▶ Generate").strong()).on_hover_text("Build the colour rules from the values found in all loaded models").clicked() {
                regen = true;
            }
            ui.separator();
            if ui.button("📂 Import…").on_hover_text("Load a saved profile (.bimprofile) with its colours and apply it to these models").clicked() {
                self.request_path(Purpose::ImportProfile);
            }
            if ui.add_enabled(!self.profile.rules.is_empty(), egui::Button::new("💾 Export…")).on_hover_text("Save this profile (property, rules and your colours) to reuse it on other models").clicked() {
                self.request_path(Purpose::ExportProfile);
            }
        });
        if regen {
            self.profile_generate();
        }
        if self.profile.rules.is_empty() {
            ui.label(RichText::new("Choose a property and press Generate. Examples: Item / Level, Item / IFC Type, a Pset's FireRating or System, a quantity like NetVolume (By range).").weak());
            return;
        }
        let mut dirty = false;
        ui.horizontal_wrapped(|ui| {
            dirty |= ui.checkbox(&mut self.profile.active, RichText::new("Show colours").strong()).changed();
            ui.label("Elements without a rule:");
            egui::ComboBox::from_id_salt("prof_unm").selected_text(self.profile.unmatched.label()).show_ui(ui, |ui| {
                for u in Unmatched::ALL {
                    dirty |= ui.selectable_value(&mut self.profile.unmatched, u, u.label()).changed();
                }
            });
            if ui.button("All visible").clicked() {
                self.profile.rules.iter_mut().for_each(|r| r.visible = true);
                dirty = true;
            }
            if self.profile.mode == ProfileMode::Values && ui.button("➕ Add missing values").on_hover_text("Keep your colours and add new colours only for values that have no rule yet (e.g. after importing a profile)").clicked() {
                self.profile_add_missing();
            }
            if ui.button("➕ Rule").on_hover_text(if self.profile.mode == ProfileMode::Values { "Add a value by hand (type it exactly as in the model)" } else { "Add a range after the last one" }).clicked() {
                self.profile.add_rule();
                self.profile_recount();
            }
            if ui.button("Clear").on_hover_text("Remove the profile and go back to the normal colours").clicked() {
                self.profile_clear();
            }
        });
        let mut select: Option<(usize, bool)> = None;
        let mut remap = false;
        let mut delete: Option<usize> = None;
        let ranges = self.profile.mode == ProfileMode::Ranges;
        egui::ScrollArea::vertical().id_salt("prof_rules").auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("prof_grid").striped(true).num_columns(6).show(ui, |ui| {
                ui.label(RichText::new("Colour").small());
                ui.label(RichText::new("Show").small());
                ui.label(RichText::new(format!("{} / {}", self.profile.cat, self.profile.prop)).small());
                ui.label(RichText::new(if ranges { "From – to" } else { "Edit value" }).small());
                ui.label(RichText::new("Items").small());
                ui.label("");
                ui.end_row();
                let n = self.profile.rules.len();
                for i in 0..n {
                    let r = &mut self.profile.rules[i];
                    let mut c = Color32::from_rgba_unmultiplied(r.color[0], r.color[1], r.color[2], 255);
                    if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
                        r.color = [c.r(), c.g(), c.b(), 255];
                        dirty = true;
                    }
                    dirty |= ui.checkbox(&mut r.visible, "").changed();
                    if ui.selectable_label(false, &r.label).on_hover_text("Click: select these items · Ctrl+click: add to the selection").clicked() {
                        select = Some((i, ui.input(|i| i.modifiers.command)));
                    }
                    if ranges {
                        let mut relabel = false;
                        ui.horizontal(|ui| {
                            if let (Some(lo), Some(hi)) = (r.lo.as_mut(), r.hi.as_mut()) {
                                relabel |= ui.add(egui::DragValue::new(lo).speed(0.1).max_decimals(3)).changed();
                                ui.label("–");
                                relabel |= ui.add(egui::DragValue::new(hi).speed(0.1).max_decimals(3)).changed();
                            }
                        });
                        if relabel {
                            self.profile.relabel_range(i);
                            remap = true;
                        }
                    } else {
                        let resp = ui.add(egui::TextEdit::singleline(&mut self.profile.rules[i].label).desired_width(140.0));
                        if resp.lost_focus() || (resp.changed() && !resp.has_focus()) {
                            remap = true;
                        }
                    }
                    ui.label(self.profile.rules[i].count.to_string());
                    if ui.small_button("🗑").on_hover_text("Remove this rule").clicked() {
                        delete = Some(i);
                    }
                    ui.end_row();
                }
            });
        });
        if let Some(i) = delete {
            self.profile.rules.remove(i);
            remap = true;
        }
        if remap {
            self.profile_recount();
        }
        if let Some((i, add)) = select {
            self.profile_select(i, add);
        }
        if dirty {
            self.colors_dirty = true;
        }
    }

    // ================================================================== Quantification
    fn qto_ui(&mut self, ui: &mut egui::Ui) {
        use crate::qto::{fmt, QtoScope, QtyColumn};
        if self.loaded_models().next().is_none() {
            ui.label(RichText::new("No models loaded.").weak());
            return;
        }
        let cats: Vec<String> = self.prop_index.cats.iter().map(|c| c.0.clone()).collect();
        let props_of = |s: &App, c: &str| -> Vec<String> { s.prop_index.props(c).map(|v| v.iter().map(|p| p.name.clone()).collect()).unwrap_or_default() };
        // --- rand 1: nume, domeniu, sabloane, export
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.qto.name).desired_width(150.0).hint_text("Name"));
            egui::ComboBox::from_id_salt("qto_scope").selected_text(self.qto.scope.label()).show_ui(ui, |ui| {
                for s in QtoScope::ALL {
                    ui.selectable_value(&mut self.qto.scope, s, s.label());
                }
            });
            ui.separator();
            if ui.button("📊 Excel…").on_hover_text("Summary sheet (with totals) + itemized sheet - .xlsx").clicked() {
                self.request_path(Purpose::ExportQtoXlsx);
            }
            if ui.button("CSV…").on_hover_text("Summary as CSV (decimal comma, ';' separator)").clicked() {
                self.request_path(Purpose::ExportQtoCsv);
            }
            ui.separator();
            if ui.button("📂 Template…").on_hover_text("Load a saved quantification setup (.bimqto)").clicked() {
                self.request_path(Purpose::ImportQtoTemplate);
            }
            if ui.button("💾 Template…").on_hover_text("Save this setup (grouping + columns) to reuse it on other projects").clicked() {
                self.request_path(Purpose::ExportQtoTemplate);
            }
        });
        // --- rand 2: grupare
        let mut del_g: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Group by").strong());
            for (i, lv) in self.qto.group_by.iter_mut().enumerate() {
                if i > 0 {
                    ui.label("›");
                }
                egui::ComboBox::from_id_salt(("qg_cat", i)).width(90.0).selected_text(&lv.cat).show_ui(ui, |ui| {
                    for c in &cats {
                        if ui.selectable_label(*c == lv.cat, c).clicked() && *c != lv.cat {
                            lv.cat = c.clone();
                            lv.prop = self.prop_index.props(c).and_then(|v| v.first()).map(|p| p.name.clone()).unwrap_or_default();
                        }
                    }
                });
                let props: Vec<String> = self.prop_index.props(&lv.cat).map(|v| v.iter().map(|p| p.name.clone()).collect()).unwrap_or_default();
                egui::ComboBox::from_id_salt(("qg_prop", i)).width(130.0).selected_text(&lv.prop).show_ui(ui, |ui| {
                    for p in &props {
                        ui.selectable_value(&mut lv.prop, p.clone(), p);
                    }
                });
                if ui.small_button("✕").on_hover_text("Remove this grouping level").clicked() {
                    del_g = Some(i);
                }
            }
            if self.qto.group_by.len() < 4 && ui.small_button("➕").on_hover_text("Add a grouping level").clicked() {
                self.qto.group_by.push(TreeLevel::new(ITEM_CAT, "Level"));
            }
        });
        if let Some(i) = del_g {
            self.qto.group_by.remove(i);
        }
        // --- rand 3: coloane
        let mut del_c: Option<usize> = None;
        let mut add_c: Option<QtyColumn> = None;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Columns").strong());
            for c in QtyColumn::GEOMETRIC {
                let mut on = self.qto.columns.contains(&c);
                if ui.checkbox(&mut on, c.title()).on_hover_text(c.hint()).changed() {
                    if on {
                        add_c = Some(c.clone());
                    } else if let Some(i) = self.qto.columns.iter().position(|x| *x == c) {
                        del_c = Some(i);
                    }
                }
            }
            for (i, c) in self.qto.columns.iter().enumerate() {
                if let QtyColumn::Prop { cat, prop } = c {
                    if ui.small_button(format!("{prop} ✕")).on_hover_text(format!("{cat} / {prop} - click to remove")).clicked() {
                        del_c = Some(i);
                    }
                }
            }
            // proprietate numerica noua (ex. Qto_WallBaseQuantities / NetVolume)
            let id = ui.make_persistent_id("qto_newcol");
            let (mut pc, mut pp): (String, String) = ui.data_mut(|d| d.get_temp(id)).unwrap_or_default();
            if pc.is_empty() {
                pc = cats.iter().find(|c| c.starts_with("Qto_") || c.starts_with("BaseQuantities")).or(cats.first()).cloned().unwrap_or_default();
            }
            egui::ComboBox::from_id_salt("qto_nc_cat").width(110.0).selected_text(if pc.is_empty() { "Category" } else { &pc }).show_ui(ui, |ui| {
                for c in &cats {
                    if ui.selectable_label(*c == pc, c).clicked() {
                        pc = c.clone();
                        pp.clear();
                    }
                }
            });
            let props = props_of(self, &pc);
            egui::ComboBox::from_id_salt("qto_nc_prop").width(130.0).selected_text(if pp.is_empty() { "Property" } else { &pp }).show_ui(ui, |ui| {
                for p in &props {
                    ui.selectable_value(&mut pp, p.clone(), p);
                }
            });
            if ui.add_enabled(!pp.is_empty(), egui::Button::new("➕ Sum column")).on_hover_text("Add a column that sums this numeric property").clicked() {
                add_c = Some(QtyColumn::Prop { cat: pc.clone(), prop: pp.clone() });
            }
            ui.data_mut(|d| d.insert_temp(id, (pc, pp)));
        });
        if let Some(i) = del_c {
            self.qto.columns.remove(i);
        }
        if let Some(c) = add_c {
            if !self.qto.columns.contains(&c) {
                self.qto.columns.push(c);
                // coloanele geometrice in ordinea lor fixa, proprietatile la final (sortare stabila)
                self.qto.columns.sort_by_key(|x| QtyColumn::GEOMETRIC.iter().position(|g| g == x).unwrap_or(100));
            }
        }
        ui.separator();

        // --- tabelul (recalculat automat la orice schimbare)
        self.qto_update();
        let Some(r) = &self.qto_result else { return };
        if r.count == 0 {
            ui.label(RichText::new(if self.qto.scope == QtoScope::Selection { "Nothing selected." } else { "No items." }).weak());
            return;
        }
        let nk = r.setup.group_by.len();
        let ncol = r.setup.columns.len();
        let ctrl = ui.input(|i| i.modifiers.command);
        let mut select: Option<(Vec<ElemId>, bool)> = None;
        egui::ScrollArea::both().id_salt("qto_table").auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("qto_grid").striped(true).num_columns(nk.max(1) + ncol).min_col_width(60.0).show(ui, |ui| {
                if nk == 0 {
                    ui.label(RichText::new("Group").strong());
                }
                for g in &r.setup.group_by {
                    ui.label(RichText::new(&g.prop).strong());
                }
                for i in 0..ncol {
                    ui.label(RichText::new(r.column_title(i)).strong()).on_hover_text(r.setup.columns[i].hint());
                }
                ui.end_row();
                for row in &r.rows {
                    if nk == 0 {
                        if ui.selectable_label(false, "All").clicked() {
                            select = Some((row.ids.clone(), ctrl));
                        }
                    }
                    for key in &row.keys {
                        let resp = ui.selectable_label(false, key).on_hover_text("Click: select these items · Ctrl+click: add to the selection");
                        if resp.clicked() {
                            select = Some((row.ids.clone(), ctrl));
                        }
                    }
                    for (ci, v) in row.values.iter().enumerate() {
                        let t = fmt(*v, &r.setup.columns[ci]);
                        let resp = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(t)).inner;
                        if row.missing[ci] > 0 && v.is_some() {
                            resp.on_hover_text(format!("{} of {} items have no value here", row.missing[ci], row.ids.len()));
                        }
                    }
                    ui.end_row();
                }
                ui.label(RichText::new("TOTAL").strong());
                for _ in 1..nk.max(1) {
                    ui.label("");
                }
                for (ci, v) in r.totals.iter().enumerate() {
                    let t = fmt(*v, &r.setup.columns[ci]);
                    let resp = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(t).strong())).inner;
                    if r.missing[ci] > 0 {
                        resp.on_hover_text(format!("{} items have no value for this column", r.missing[ci]));
                    }
                }
                ui.end_row();
            });
        });
        if let Some((ids, add)) = select {
            self.apply(Action::Select(ids, add));
        }
    }

    // ================================================================== Compare
    fn compare_ui(&mut self, ui: &mut egui::Ui) {
        use crate::compare::Diff;
        let models: Vec<(usize, String)> = self.loaded_models().map(|(i, m)| (i, m.name.clone())).collect();
        if models.len() < 2 {
            ui.label(RichText::new("Load both versions of the model (Open the old one, then ➕ Append the new one) to compare them.").weak());
            return;
        }
        // implicit: A = primul model, B = ultimul
        let ok = |s: Option<usize>| s.filter(|i| models.iter().any(|m| m.0 == *i));
        let a = ok(self.compare_pick.0).unwrap_or(models[0].0);
        let b = ok(self.compare_pick.1).unwrap_or(models[models.len() - 1].0);
        self.compare_pick = (Some(a), Some(b));
        let name = |i: usize| models.iter().find(|m| m.0 == i).map(|m| m.1.clone()).unwrap_or_default();
        let mut run = false;
        ui.horizontal_wrapped(|ui| {
            for (lbl, id, slot) in [("A (old)", "cmp_a", 0), ("B (new)", "cmp_b", 1)] {
                ui.label(lbl);
                let cur = if slot == 0 { a } else { b };
                egui::ComboBox::from_id_salt(id).width(220.0).selected_text(name(cur)).show_ui(ui, |ui| {
                    for (i, n) in &models {
                        if ui.selectable_label(*i == cur, n).clicked() {
                            if slot == 0 {
                                self.compare_pick.0 = Some(*i);
                            } else {
                                self.compare_pick.1 = Some(*i);
                            }
                        }
                    }
                });
            }
            if ui.button("⇅").on_hover_text("Swap A and B").clicked() {
                self.compare_pick = (Some(b), Some(a));
            }
            ui.label("Tolerance");
            ui.add(egui::DragValue::new(&mut self.compare_tol).range(0.0..=1000.0).speed(0.5).suffix(" mm"));
            run = ui.add_enabled(a != b, egui::Button::new(RichText::new("▶ Compare").strong())).on_hover_text("Match elements by GUID and compare geometry and properties").clicked();
            ui.separator();
            if ui.button("📂 Open…").on_hover_text("Open a saved comparison (.bimcmp) on these two versions").clicked() {
                self.request_path(Purpose::ImportCompare);
            }
            let has = self.compare.is_some();
            if ui.add_enabled(has, egui::Button::new("💾 Save…")).on_hover_text("Save the comparison (.bimcmp) to open it again later or send it to the team").clicked() {
                self.request_path(Purpose::ExportCompare);
            }
            if ui.add_enabled(has, egui::Button::new("📊 CSV report…")).on_hover_text("Every changed element with its status and changes - opens in Excel").clicked() {
                self.request_path(Purpose::ExportCompareCsv);
            }
        });
        if run {
            self.run_compare();
        }
        let Some(r) = &self.compare else {
            ui.label(RichText::new("Elements are matched by IFC GlobalId. Geometry counts as changed when its bounding box moves more than the tolerance or its triangle count changes.").weak());
            return;
        };
        let counts = r.counts;
        let lists: Vec<(Diff, Vec<ElemId>)> = Diff::ALL.iter().map(|d| (*d, r.ids(*d))).collect();
        let changes: Vec<(ElemId, String, Vec<String>)> = r.changes.iter().take(2000).map(|(id, v)| (*id, self.elem(*id).map(|x| x.1.name.clone()).unwrap_or_default(), v.clone())).collect();
        let n_changes = r.changes.len();
        let mut acts: Vec<Action> = Vec::new();
        let mut new_set: Option<(String, Vec<ElemId>)> = None;
        let mut dirty = false;
        ui.horizontal(|ui| {
            dirty |= ui.checkbox(&mut self.compare_on, RichText::new("Show results in 3D").strong()).changed();
            if ui.button("Clear").clicked() {
                acts.clear();
                self.compare_clear();
            }
        });
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            egui::Grid::new("cmp_sum").striped(true).num_columns(4).show(ui, |ui| {
                for (k, (d, ids)) in lists.iter().enumerate() {
                    let c = d.color();
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 3.0, Color32::from_rgb(c[0], c[1], c[2]));
                    ui.label(d.label());
                    ui.label(RichText::new(counts[k].to_string()).strong());
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!ids.is_empty(), egui::Button::new("Select").small()).clicked() {
                            acts.push(Action::Select(ids.clone(), false));
                        }
                        if ui.add_enabled(!ids.is_empty() && *d != Diff::Same, egui::Button::new("Save as set").small()).clicked() {
                            new_set = Some((format!("Compare: {}", d.label()), ids.clone()));
                        }
                    });
                    ui.end_row();
                }
            });
            let ui = &mut cols[1];
            ui.label(RichText::new(format!("Changes ({n_changes})")).strong());
            egui::ScrollArea::vertical().id_salt("cmp_changes").auto_shrink([false, false]).show(ui, |ui| {
                for (id, nm, why) in &changes {
                    let first = why.first().cloned().unwrap_or_default();
                    let r = ui.selectable_label(self.selected.contains(id), format!("{nm}  —  {first}")).on_hover_text(why.join("\n"));
                    if r.clicked() {
                        acts.push(Action::Select(vec![*id], false));
                        acts.push(Action::Focus(vec![*id]));
                    }
                }
                if n_changes > changes.len() {
                    ui.label(RichText::new(format!("… {} more (use Select / Save as set)", n_changes - changes.len())).weak());
                }
            });
        });
        for a in acts {
            self.apply(a);
        }
        if let Some((n, ids)) = new_set {
            self.sets.push(SelSet { name: n.clone(), kind: SetKind::Explicit(ids) });
            self.toast(format!("Set created: {n}"));
        }
        if dirty {
            self.colors_dirty = true;
        }
    }

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
            if rbtn(ui, "✋", "Pan", "Pan: left-drag moves the model sideways (also: middle-drag or Shift+drag in any mode)", self.nav == NavMode::Pan, true).clicked() {
                self.nav = NavMode::Pan;
            }
            if rbtn(ui, "🚶", "Walk", "Walk: hold left button and drag up / down to walk, left / right to turn (Ctrl = faster). W A S D / arrows also walk, Q / E up / down.", self.nav == NavMode::Walk, true).clicked() {
                self.nav = NavMode::Walk;
            }
            ui.vertical(|ui| {
                ui.add_space(6.0);
                ui.label("Walk speed");
                let mut v = self.settings.walk_speed;
                if ui.add(egui::Slider::new(&mut v, 0.2..=20.0).logarithmic(true).suffix(" m/s").max_decimals(1)).changed() {
                    self.settings.walk_speed = v;
                }
                if ui.input(|i| i.pointer.any_released()) && (self.settings.walk_speed - self.walk_saved).abs() > 1e-4 {
                    self.walk_saved = self.settings.walk_speed;
                    self.settings.save();
                }
                ui.horizontal(|ui| {
                    for (n, v) in [("Slow", 0.7), ("Normal", 1.4), ("Fast", 4.0)] {
                        if ui.small_button(n).on_hover_text(format!("{v} m/s")).clicked() {
                            self.settings.walk_speed = v;
                            self.settings.save();
                        }
                    }
                });
            });
            ui.vertical(|ui| {
                ui.add_space(4.0);
                let mut c = (self.settings.gravity, self.settings.collision);
                ui.checkbox(&mut c.0, "Gravity").on_hover_text("Walk on floors and stairs at eye height");
                ui.checkbox(&mut c.1, "Collision").on_hover_text("Stop at walls and objects");
                ui.horizontal(|ui| {
                    ui.label("Eye");
                    let mut h = self.settings.eye_height;
                    if ui.add(egui::DragValue::new(&mut h).range(0.5..=3.0).speed(0.01).suffix(" m")).changed() {
                        self.settings.eye_height = h;
                    }
                });
                if c != (self.settings.gravity, self.settings.collision) {
                    self.settings.gravity = c.0;
                    self.settings.collision = c.1;
                    if c.0 || c.1 {
                        self.nav = NavMode::Walk;
                    }
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
            ui.vertical(|ui| {
                ui.add_space(8.0);
                ui.label("Field of view");
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
            if rbtn(ui, "🔗", "Follow", "Section box follows the selection: every time you select elements the box fits around them (the camera stays)", self.section_follow, true).clicked() {
                self.section_follow = !self.section_follow;
                self.section_sel_sig = 0;
            }
            self.panel_btn(ui, Panel::Section, "⚙", "Planes", "Section planes & box");
        });
        self.visibility_group(ui);
    }

    fn ribbon_coord(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Coordination", |ui| {
            self.panel_btn(ui, Panel::Clash, "⚠", "Clash", "Clash Detective — clashes are shown only while this panel is visible");
            self.panel_btn(ui, Panel::TimeLiner, "📅", "TimeLiner", "4D schedule and simulation");
            self.panel_btn(ui, Panel::Find, "🔍", "Find Items", "Search by properties");
            self.panel_btn(ui, Panel::Sets, "📋", "Sets", "Selection & search sets");
        });
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
    }

    fn ribbon_render(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Presentation", |ui| {
            let on = self.panel_shown(Panel::Render) && self.render_subtab == 0;
            if rbtn(ui, "🎬", "Animation", "Camera animations & video export", on, true).clicked() {
                self.render_subtab = 0;
                if on {
                    self.settings.layout.close(Panel::Render);
                } else {
                    self.open_panel(Panel::Render);
                }
            }
            let on = self.panel_shown(Panel::Render) && self.render_subtab == 1;
            if rbtn(ui, "🎨", "Materials", "Material library, image textures & assignments", on, true).clicked() {
                self.render_subtab = 1;
                if on {
                    self.settings.layout.close(Panel::Render);
                } else {
                    self.open_panel(Panel::Render);
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
            if rbtn(ui, "📥", "Import Lib", "Import a material library (.bimmat)", false, true).clicked() {
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
            let can = !self.animations.is_empty();
            if rbtn(ui, "🎞", "Video", "Export the selected animation (AVI / MP4 / GIF / PNG)", false, can).clicked() {
                self.render_subtab = 0;
                self.open_panel(Panel::Render);
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
        });
        rgroup(ui, "Navisworks (XML)", |ui| {
            if rbtn(ui, "📷", "Viewpoints", "Export viewpoints with markups, comments and section box\n(Navisworks: Saved Viewpoints > right-click > Import Viewpoints)", false, !self.viewpoints.is_empty()).clicked() {
                self.request_path(Purpose::ExportNavisXml);
            }
            if rbtn(ui, "📥", "Import BCF", "Import BCF 2.1 / 3.0 issues (Revit, ACC, Solibri, BIMcollab...)", false, true).clicked() {
                self.request_path(Purpose::ImportBcf);
            }
            if rbtn(ui, "📥", "Import VP", "Import viewpoints exported from Navisworks (Saved Viewpoints > right-click > Export Viewpoints)", false, true).clicked() {
                self.request_path(Purpose::ImportNavisXml);
            }
            if rbtn(ui, "📋", "Sets", "Export selection and search sets\n(Navisworks: Sets > Import Search Sets)", false, !self.sets.is_empty()).clicked() {
                self.request_path(Purpose::ExportNavisSets);
            }
            if rbtn(ui, "⚠", "Clash Tests", "Export the clash tests (type, tolerance, selections A / B) and the sets they use\n(Navisworks: Clash Detective > Import / Export > Import Clash Tests)", false, !self.clash_tests.is_empty()).clicked() {
                self.request_path(Purpose::ExportNavisClash);
            }
        });
        rgroup(ui, "IFC", |ui| {
            let tip = if self.selected.is_empty() { "Export all visible items to IFC4: geometry, colours, levels, GUIDs and all properties" } else { "Export the selected items to IFC4: geometry, colours, levels, GUIDs and all properties" };
            if rbtn(ui, "🏗", "Export IFC", tip, false, self.loaded_models().next().is_some()).clicked() {
                self.request_path(Purpose::ExportIfc);
            }
        });
        rgroup(ui, "Data", |ui| {
            if rbtn(ui, "📤", "Sets", "Export selection / search sets", false, !self.sets.is_empty()).clicked() {
                self.request_path(Purpose::ExportSets);
            }
            if rbtn(ui, "📥", "Sets", "Import sets", false, true).clicked() {
                self.request_path(Purpose::ImportSets);
            }
            if rbtn(ui, "📤", "Schedule", "Export the TimeLiner schedule (CSV)", false, true).clicked() {
                self.request_path(Purpose::ExportTasksCsv);
            }
            if rbtn(ui, "📥", "Schedule", "Import a TimeLiner schedule (CSV)", false, true).clicked() {
                self.request_path(Purpose::ImportTasksCsv);
            }
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

    fn ribbon_view(&mut self, ui: &mut egui::Ui) {
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
                    if ui.checkbox(&mut open, p.title()).changed() {
                        if open {
                            self.open_panel(*p);
                        } else {
                            self.settings.layout.close(*p);
                        }
                    }
                }
            });
            if rbtn(ui, "⟲", "Reset", "Restore the default window arrangement", false, true).clicked() {
                self.settings.layout = Default::default();
            }
        });
        rgroup(ui, "Scene", |ui| {
            if rbtn(ui, "💡", "Lighting", "Brightness, ambient light and sun (also used for renders and videos)", self.show_light, true).clicked() {
                self.show_light = !self.show_light;
            }
            let has_cloud = self.loaded_models().any(|(_, m)| m.cloud.is_some());
            if rbtn(ui, "☁", "Point Cloud", "Point density and size (E57 point clouds)", self.show_cloud, has_cloud).clicked() {
                self.show_cloud = !self.show_cloud;
            }
        });
        rgroup(ui, "Grids & Levels", |ui| {
            if rbtn(ui, "#", "Grids", "Show / hide the grid axes in the 3D view", self.show_grids, true).clicked() {
                self.show_grids = !self.show_grids;
            }
            if rbtn(ui, "≡", "Levels", "Show / hide the levels in the 3D view", self.show_levels, true).clicked() {
                self.show_levels = !self.show_levels;
            }
            if rbtn(ui, "✏", "Edit Levels", "Your own levels: ±0.00 of the project, names and elevations (editable)", self.show_levels_editor, true).clicked() {
                self.show_levels_editor = !self.show_levels_editor;
            }
        });
        rgroup(ui, "Theme", |ui| {
            if rbtn(ui, "☀", "Light", "Light theme", self.settings.theme == Theme::Light, true).clicked() {
                self.set_theme(Theme::Light);
            }
            if rbtn(ui, "🌙", "Dark", "Dark theme", self.settings.theme == Theme::Dark, true).clicked() {
                self.set_theme(Theme::Dark);
            }
            if rbtn(ui, "❄", "Nord", "Nord theme: calm arctic blue-grey (nordtheme.com)", self.settings.theme == Theme::Nord, true).clicked() {
                self.set_theme(Theme::Nord);
            }
            if rbtn(ui, "🌃", "Tokyo Night", "Tokyo Night theme: deep night blue with blue / purple accents", self.settings.theme == Theme::TokyoNight, true).clicked() {
                self.set_theme(Theme::TokyoNight);
            }
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
        rgroup(ui, "Help", |ui| {
            if rbtn(ui, "🖱", "Controls", "Mouse & keyboard navigation", false, true).clicked() {
                self.show_help = true;
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
                let i = self.report_job.as_ref().unwrap().idx;
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
                if let Some(img) = shot {
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
                    let i = self.report_job.as_ref().unwrap().idx;
                    let item = self.report_item(i, png, w2, h2);
                    let n = self.viewpoints.len();
                    let j = self.report_job.as_mut().unwrap();
                    j.items.push(item);
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
        rgroup(ui, "Redline", |ui| {
            for k in crate::markup::RedKind::ALL {
                let on = self.tool == Tool::Redline(k);
                if rbtn(ui, k.icon(), k.label(), &format!("Redline {} on the current view (saved in a viewpoint)", k.label()), on, true).clicked() {
                    self.cur_measure.clear();
                    self.tool = if on { Tool::Select } else { Tool::Redline(k) };
                }
            }
            if rbtn(ui, "✖", "Erase", "Click a markup to delete it", self.tool == Tool::RedErase, true).clicked() {
                self.tool = if self.tool == Tool::RedErase { Tool::Select } else { Tool::RedErase };
            }
            ui.vertical(|ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    for c in [[230u8, 40, 40], [245, 190, 20], [40, 170, 70], [40, 110, 230], [20, 20, 20], [250, 250, 250]] {
                        let (r, resp) = ui.allocate_exact_size(EV::splat(16.0), Sense::click());
                        ui.painter().rect_filled(r, 3.0, Color32::from_rgb(c[0], c[1], c[2]));
                        if self.red_color == c {
                            ui.painter().rect_stroke(r.expand(1.5), 3.0, Stroke::new(2.0_f32, accent()));
                        }
                        if resp.clicked() {
                            self.red_color = c;
                        }
                    }
                    ui.color_edit_button_srgb(&mut self.red_color);
                });
                ui.add(egui::Slider::new(&mut self.red_width, 1.0..=10.0).text("Thickness"));
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
            if rbtn(ui, "📤", "BCF", "BCF 2.1 issues (Revit, Navisworks, Solibri, BIMcollab, ACC...)", false, can).clicked() {
                self.request_path(Purpose::ExportBcf);
            }
            if rbtn(ui, "🧭", "Navisworks", "Viewpoints with markups, comments and section box as Navisworks XML\n(in Navisworks: Saved Viewpoints > right-click > Import Viewpoints)", false, can).clicked() {
                self.request_path(Purpose::ExportNavisXml);
            }
        });
    }

    /// Nimic deschis si nimic in curs de incarcare: se arata pagina de start.
    fn start_screen(&self) -> bool {
        self.loaded_models().next().is_none() && self.loads.is_empty() && self.project_path.is_none()
    }

    // ================================================================== pagina de start
    /// Ecranul de pornire, cand nu e deschis niciun model: Open / Open Project / New si fisierele recente
    /// ca niste carduri (tip, nume, data modificarii); click deschide, click dreapta scoate din lista.
    fn start_page(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let pal = self.pal();
        let side_w = 220.0f32.min(rect.width() * 0.35);
        let side = Rect::from_min_size(rect.min, EV::new(side_w, rect.height()));
        let dark = self.settings.theme.is_dark();
        let side_fill = if dark { Color32::from_black_alpha(70) } else { Color32::from_white_alpha(150) };
        ui.painter().rect_filled(side, 0.0, side_fill);
        let mut open_path: Option<String> = None;
        let mut forget: Option<String> = None;
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(side.shrink2(EV::new(18.0, 22.0))), |ui| {
            ui.label(RichText::new("ManageBIM").size(24.0).strong().color(accent()));
            ui.label(RichText::new(format!("version {}", env!("CARGO_PKG_VERSION"))).small().color(pal.overlay_weak));
            ui.add_space(22.0);
            let big = |t: &str| egui::Button::new(RichText::new(t).size(14.0)).min_size(EV::new(side_w - 36.0, 32.0));
            if ui.add(big("📂  Open…")).on_hover_text("Open a model: IFC, Revit / Navisworks, DWG, STEP, FBX, E57…  (Ctrl+O)").clicked() {
                self.request_path(Purpose::OpenModel { append: false });
            }
            ui.add_space(4.0);
            if ui.add(big("🗂  Open Project…")).on_hover_text("Open a ManageBIM project (.bimr)").clicked() {
                self.request_path(Purpose::OpenProject);
            }
            ui.add_space(4.0);
            if ui.add(big("✚  New")).on_hover_text("New project: choose the first model").clicked() {
                self.reset();
                self.request_path(Purpose::OpenModel { append: true });
            }
            ui.add_space(18.0);
            ui.label(RichText::new("or drag files into the window").small().color(pal.overlay_weak));
            // recuperare dupa o inchidere neasteptata
            if let Some((_, orig, when)) = self.autosave.recover.clone() {
                ui.add_space(22.0);
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
        // carduri
        let area = Rect::from_min_max(Pos2::new(side.right() + 28.0, rect.top() + 20.0), Pos2::new(rect.right() - 20.0, rect.bottom() - 10.0));
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(area), |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Recent").size(20.0).strong().color(pal.overlay_text));
                if !self.settings.recent.is_empty() {
                    ui.add_space(12.0);
                    if ui.small_button("Clear list").clicked() {
                        self.settings.recent.clear();
                        self.settings.save();
                    }
                }
            });
            ui.add_space(8.0);
            if self.settings.recent.is_empty() {
                ui.label(RichText::new("No recent files yet - open a model to start.").color(pal.overlay_weak));
                return;
            }
            let recent = self.settings.recent.clone();
            egui::ScrollArea::vertical().id_salt("start_recent").auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = EV::new(12.0, 12.0);
                    for p in &recent {
                        let (r, resp) = ui.allocate_exact_size(EV::new(150.0, 150.0), Sense::click());
                        let exists = std::path::Path::new(p).exists();
                        let hov = resp.hovered() && exists;
                        let card = if dark { Color32::from_white_alpha(if hov { 26 } else { 14 }) } else { Color32::from_white_alpha(if hov { 255 } else { 215 }) };
                        let painter = ui.painter();
                        painter.rect(r, 6.0, card, Stroke::new(1.0_f32, if hov { accent() } else { Color32::from_white_alpha(if dark { 22 } else { 0 }) }));
                        let (tag, col) = file_badge(p);
                        let icon = Rect::from_center_size(Pos2::new(r.center().x, r.top() + 50.0), EV::new(64.0, 64.0));
                        let col = if exists { col } else { col.gamma_multiply(0.35) };
                        painter.rect_filled(icon.translate(EV::new(5.0, 5.0)), 6.0, col.gamma_multiply(0.55));
                        painter.rect_filled(icon, 6.0, col);
                        painter.text(icon.center(), Align2::CENTER_CENTER, tag, FontId::proportional(if tag.len() > 3 { 15.0 } else { 19.0 }), Color32::WHITE);
                        let name = crate::file_name(p);
                        let short: String = if name.chars().count() > 20 { format!("{}…", name.chars().take(19).collect::<String>()) } else { name.clone() };
                        let tc = if exists { pal.overlay_text } else { pal.overlay_weak };
                        painter.text(Pos2::new(r.left() + 10.0, r.bottom() - 40.0), Align2::LEFT_CENTER, short, FontId::proportional(12.5), tc);
                        let date = if exists { std::fs::metadata(p).and_then(|m| m.modified()).map(fmt_date).unwrap_or_default() } else { "file not found".into() };
                        painter.text(Pos2::new(r.left() + 10.0, r.bottom() - 20.0), Align2::LEFT_CENTER, date, FontId::proportional(10.5), pal.overlay_weak);
                        let resp = resp.on_hover_text(p.as_str());
                        if resp.clicked() && exists {
                            open_path = Some(p.clone());
                        }
                        resp.context_menu(|ui| {
                            if exists && ui.button("Open").clicked() {
                                open_path = Some(p.clone());
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
                    }
                });
            });
        });
        if let Some(p) = forget {
            self.settings.recent.retain(|x| x != &p);
            self.settings.save();
        }
        if let Some(p) = open_path {
            self.open_path(&p, false);
        }
    }

    // ================================================================== ViewCube
    fn view_cube(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let pal = self.pal();
        // cub compact: mai mult loc pentru model
        let size = 22.0f32;
        let c = Pos2::new(rect.right() - 52.0, rect.top() + 60.0);
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
        let edge = 0.62f32;
        let bands = [(-1.0f32, -edge), (-edge, edge), (edge, 1.0)];
        let hover = resp.hover_pos();
        let mut hit: Option<Vec3> = None;
        let mut cells: Vec<(Vec<Pos2>, Vec3, f32)> = Vec::new();
        for &(dot, fi) in &vis {
            let (n, fu, fv, _) = faces[fi];
            for (iu, bu) in bands.iter().enumerate() {
                for (iv, bv) in bands.iter().enumerate() {
                    let poly: Vec<Pos2> = [(bu.0, bv.0), (bu.1, bv.0), (bu.1, bv.1), (bu.0, bv.1)].iter().map(|(a, b)| proj(n + fu * *a + fv * *b)).collect();
                    let dir = n + fu * (iu as f32 - 1.0) + fv * (iv as f32 - 1.0);
                    cells.push((poly, dir, -dot));
                }
            }
        }
        if let Some(hp) = hover {
            // cea mai apropiata fata e ultima
            for (poly, dir, _) in cells.iter().rev() {
                if inside(poly, hp) {
                    hit = Some(*dir);
                    break;
                }
            }
        }
        let p = ui.painter();
        let dark = self.settings.theme.is_dark();
        let base = if dark { Color32::from_rgb(150, 160, 178) } else { Color32::from_rgb(236, 240, 246) };
        let edge_col = if dark { Color32::from_rgb(140, 150, 165) } else { Color32::from_rgb(120, 130, 145) };
        for (poly, dir, light) in &cells {
            let k = 0.55 + 0.45 * light;
            let mut col = Color32::from_rgb((base.r() as f32 * k) as u8, (base.g() as f32 * k) as u8, (base.b() as f32 * k) as u8);
            if hit.map(|h| (h - *dir).length() < 1e-4).unwrap_or(false) {
                col = accent().gamma_multiply(0.85);
            }
            p.add(egui::Shape::convex_polygon(poly.clone(), col, Stroke::NONE));
        }
        for &(_, fi) in &vis {
            let (n, fu, fv, label) = faces[fi];
            let pts: Vec<Pos2> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().map(|(a, b): &(f32, f32)| proj(n + fu * *a + fv * *b)).collect();
            p.add(egui::Shape::closed_line(pts.clone(), Stroke::new(1.0_f32, edge_col)));
            // eticheta doar daca fata e destul de vizibila
            if n.dot(f) < -0.35 {
                let fs = 8.0 * (-n.dot(f)).sqrt().max(0.7);
                p.text(proj(n), Align2::CENTER_CENTER, label, FontId::proportional(fs), Color32::from_gray(30));
            }
        }
        // busola sub cub
        let ring_c = c + EV::new(0.0, size * 1.25);
        let ring: Vec<Pos2> = (0..48).map(|i| {
            let a = i as f32 / 48.0 * std::f32::consts::TAU;
            proj(Vec3::new(a.cos() * 1.6, a.sin() * 1.6, -1.05))
        }).collect();
        let _ = ring_c;
        p.add(egui::Shape::closed_line(ring, Stroke::new(1.0_f32, edge_col.gamma_multiply(0.7))));
        p.text(proj(Vec3::new(0.0, 1.85, -1.05)), Align2::CENTER_CENTER, "N", FontId::proportional(8.5), Color32::from_rgb(230, 80, 70));
        // buton Home si meniu
        let hr = Rect::from_center_size(c + EV::new(-size * 1.7, -size * 1.6), EV::splat(16.0));
        let hres = ui.interact(hr, ui.id().with("vc_home"), Sense::click()).on_hover_text("Home (right-click the cube for more)");
        p.text(hr.center(), Align2::CENTER_CENTER, "🏠", FontId::proportional(12.0), if hres.hovered() { accent() } else { pal.overlay_text });
        if hres.clicked() {
            self.go_home();
        }
        let pr = Rect::from_center_size(c + EV::new(size * 1.7, -size * 1.6), EV::new(28.0, 14.0));
        let pres = ui.interact(pr, ui.id().with("vc_proj"), Sense::click()).on_hover_text("Switch perspective / orthographic");
        p.text(pr.center(), Align2::CENTER_CENTER, if self.cam.ortho { "ORTHO" } else { "PERSP" }, FontId::proportional(8.0), if pres.hovered() { accent() } else { pal.overlay_text });
        if pres.clicked() {
            self.cam.ortho = !self.cam.ortho;
        }
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
        ui.separator();
        if ui.button("✖ Close").clicked() {
            self.settings.layout.close(p);
            ui.close_menu();
        }
    }

    fn panel_body(&mut self, ui: &mut egui::Ui, p: Panel) {
        match p {
            Panel::Tree => {
                self.tree_config_ui(ui);
                let acts = egui::ScrollArea::both().id_salt("tree_scroll").auto_shrink([false, false]).show(ui, |ui| self.tree_ui(ui)).inner;
                for a in acts {
                    self.apply(a);
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
            Panel::Project => self.models_ui(ui),
            Panel::Profiler => self.profiler_ui(ui),
            Panel::Compare => self.compare_ui(ui),
            Panel::Quantities => self.qto_ui(ui),
        }
    }

    fn dock_area(&mut self, ui: &mut egui::Ui, d: Dock) {
        let tabs = self.settings.layout.in_area(d);
        let active = self.settings.layout.active[d.idx()];
        ui.horizontal_wrapped(|ui| {
            ui.style_mut().spacing.item_spacing.x = 3.0;
            for p in &tabs {
                let on = Some(*p) == active;
                let r = ui.selectable_label(on, if on { RichText::new(p.title()).strong() } else { RichText::new(p.title()) }).on_hover_text("Right-click: dock left / right / bottom, float or close");
                if r.clicked() {
                    self.settings.layout.active[d.idx()] = Some(*p);
                }
                r.context_menu(|ui| self.panel_menu(ui, *p));
            }
            if let Some(a) = active {
                ui.add_space(4.0);
                ui.menu_button("⏷", |ui| self.panel_menu(ui, a)).response.on_hover_text("Move this window");
                if ui.small_button("✖").on_hover_text("Close this window").clicked() {
                    self.settings.layout.close(a);
                }
            }
        });
        ui.separator();
        if let Some(a) = self.settings.layout.active[d.idx()] {
            if d == Dock::Bottom {
                self.panel_body(ui, a);
            } else {
                // panourile laterale nu au voie sa impinga viewport-ul: continutul lat se deruleaza
                egui::ScrollArea::horizontal().id_salt(("side_scroll", d.idx())).auto_shrink([false, false]).show(ui, |ui| self.panel_body(ui, a));
            }
        }
    }

    fn float_windows(&mut self, ctx: &egui::Context) {
        for p in self.settings.layout.in_area(Dock::Float) {
            let mut open = true;
            egui::Window::new(p.title())
                .id(egui::Id::new(("float_panel", p)))
                .open(&mut open)
                .default_size([560.0, 400.0])
                .min_width(260.0)
                .min_height(160.0)
                .resizable(true)
                .collapsible(true)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.menu_button("⏷ Dock…", |ui| self.panel_menu(ui, p));
                    });
                    self.panel_body(ui, p);
                });
            if !open {
                self.settings.layout.close(p);
            }
        }
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let nm = self.loaded_models().count();
                let ne: usize = self.loaded_models().map(|(_, m)| m.elements.len()).sum();
                let nt: usize = self.loaded_models().map(|(_, m)| m.tri_total()).sum();
                if nm == 0 {
                    ui.label(RichText::new("No model open").weak());
                } else {
                    ui.label(format!("{} {} · {} elements", nm, if nm == 1 { "model" } else { "models" }, thousands(ne))).on_hover_text(format!("{} triangles", thousands(nt)));
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
                if self.sim_view() {
                    ui.separator();
                    ui.colored_label(Color32::from_rgb(90, 200, 120), format!("Simulation: {}", timeline::fmt_date(self.sim_day.floor() as i32)));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let nb = if self.settings.nav_bar { "Hide the navigation bar" } else { "Show the navigation bar" };
                    if ui.small_button(if self.settings.nav_bar { "🧭" } else { "🧭 ✖" }).on_hover_text(nb).clicked() {
                        self.settings.nav_bar = !self.settings.nav_bar;
                        self.settings.save();
                    }
                    ui.label(RichText::new(format!("{:.0} FPS", self.fps)).weak()).on_hover_text("Frames per second");
                    let u = if self.settings.units_mm { "mm" } else { "m" };
                    if ui.small_button(u).on_hover_text("Length unit for measurements, distances and coordinates - click to switch m / mm (quantities stay in m, m², m³)").clicked() {
                        self.settings.units_mm = !self.settings.units_mm;
                        self.settings.save();
                    }
                });
            });
        });
    }

    // ================================================================== viewport
    fn viewport(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        // fundal in degrade
        let mut mesh = egui::Mesh::default();
        let pal = self.pal();
        let top = pal.vp_top;
        let bot = pal.vp_bottom;
        mesh.colored_vertex(rect.left_top(), top);
        mesh.colored_vertex(rect.right_top(), top);
        mesh.colored_vertex(rect.right_bottom(), bot);
        mesh.colored_vertex(rect.left_bottom(), bot);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(mesh));

        let aspect = rect.width() / rect.height().max(1.0);
        let params = self.frame_params(aspect);
        if let Some((w, h)) = self.pending_capture.take() {
            let (bg_top, bg_bottom) = self.render_bg();
            let cp = self.frame_params(w as f32 / h.max(1) as f32);
            self.renderer.lock().unwrap().capture = Some(crate::render::CaptureReq { params: cp, w, h, bg_top, bg_bottom });
        }
        // ce se deseneaza: campul vizual, fara ascunse; simplificat cat timp camera se misca
        let lists = self.draw_lists(&params, rect.height());
        self.renderer.lock().unwrap().lists = Some(lists);
        if self.lod_moving() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(350));
        }
        let r = self.renderer.clone();
        painter.add(egui::PaintCallback {
            rect,
            callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |_info, p| {
                let mut r = r.lock().unwrap();
                unsafe { r.paint(p.gl(), &params) };
            })),
        });
        if let Some(e) = self.renderer.lock().unwrap().error.clone() {
            painter.text(rect.center(), Align2::CENTER_CENTER, e, FontId::proportional(14.0), Color32::RED);
        }

        // --------------------------------------------------------- interactiune
        let uv = |p: Pos2| Vec2::new((p.x - rect.left()) / rect.width(), (p.y - rect.top()) / rect.height());
        let mods = ui.input(|i| i.modifiers);
        let hover = resp.hover_pos();
        let mut snap: Option<Vec3> = None;
        // tipul punctului prins: 0 suprafata, 1 colt, 2 mijloc de muchie, 3 pe muchie
        let mut snap_kind = 0u8;
        if let (Tool::Measure(_), Some(hp)) = (self.tool, hover) {
            let (o, d) = self.cam.ray(uv(hp), aspect);
            if let Some((_, _, p, tri)) = self.pick(o, d) {
                let vpm = params.view_proj;
                let scr = |v: Vec3| to_screen(&vpm, rect, v).map(|s| s.distance(hp));
                let mut best = (p, 0u8, f32::INFINITY);
                // prioritate: colt (12 px), mijloc de muchie (10 px), punct pe muchie (7 px)
                for v in tri {
                    if let Some(dd) = scr(v) {
                        if dd < 12.0 && dd < best.2 {
                            best = (v, 1, dd);
                        }
                    }
                }
                if best.1 == 0 {
                    for k in 0..3 {
                        let (a, b) = (tri[k], tri[(k + 1) % 3]);
                        let m = (a + b) * 0.5;
                        if let Some(dd) = scr(m) {
                            if dd < 10.0 && dd < best.2 {
                                best = (m, 2, dd);
                            }
                        }
                    }
                }
                if best.1 == 0 {
                    for k in 0..3 {
                        let (a, b) = (tri[k], tri[(k + 1) % 3]);
                        let ab = b - a;
                        let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
                        let q = a + ab * t;
                        if let Some(dd) = scr(q) {
                            if dd < 7.0 && dd < best.2 {
                                best = (q, 3, dd);
                            }
                        }
                    }
                }
                snap = Some(best.0);
                snap_kind = best.1;
            }
        }
        self.vp_rect = [rect.left(), rect.top(), rect.width(), rect.height()];
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
        let walk = self.nav == NavMode::Walk && red_tool.is_none();
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
        if !walk && ((red_tool.is_none() && resp.drag_started_by(egui::PointerButton::Primary)) || resp.drag_started_by(egui::PointerButton::Middle)) {
            self.pivot = None;
            if let Some(p) = resp.interact_pointer_pos() {
                let (o, d) = self.cam.ray(uv(p), aspect);
                if let Some((_, t, hit, _)) = self.pick(o, d) {
                    self.pivot = Some(hit);
                    self.cam.focus = t * d.dot(self.cam.fwd()).max(0.05);
                }
            }
        }
        let delta = resp.drag_delta();
        if (walk || red_tool.is_some()) && resp.dragged_by(egui::PointerButton::Primary) && !mods.shift {
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
                self.cam.zoom(scroll / 50.0, d, ms);
            }
        }
        if resp.clicked() {
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
                        // punct pe podea: unde loveste raza (podea, scara, rampa)
                        let (o, d) = self.cam.ray(uv(p), aspect);
                        if let Some((_, _, hit, _)) = self.pick(o, d) {
                            self.path_run = None;
                            self.walk_path.push(hit);
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
                    Tool::Select => {
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
        if self.section.enabled && self.section.mode == SectionMode::Box {
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
            for (tag, a, b) in g.grids.iter().filter(|_| self.show_grids) {
                let (a, b) = (Vec3::new(a.x, a.y, gz), Vec3::new(b.x, b.y, gz));
                seg(a, b, gcol, 1.0);
                for end in [a, b] {
                    if let Some(s) = to_screen(&vp, rect, end) {
                        if rect.contains(s) {
                            painter.circle(s, 10.0, pal.overlay_btn, Stroke::new(1.2_f32, gcol));
                            painter.text(s, Align2::CENTER_CENTER, tag, FontId::proportional(10.5), pal.overlay_text);
                        }
                    }
                }
            }
            if self.show_levels && bb.is_valid() && g.levels.len() < 60 {
                let lcol = Color32::from_rgba_unmultiplied(235, 200, 120, 60);
                let oz = self.origin_f64().z;
                for (name, z) in &g.levels {
                    let c = |x: f32, y: f32| Vec3::new(x, y, *z);
                    let (x0, y0, x1, y1) = (bb.min.x, bb.min.y, bb.max.x, bb.max.y);
                    for (p, q) in [(c(x0, y0), c(x1, y0)), (c(x1, y0), c(x1, y1)), (c(x1, y1), c(x0, y1)), (c(x0, y1), c(x0, y0))] {
                        seg(p, q, lcol, 1.0);
                    }
                    if let Some(s) = to_screen(&vp, rect, c(x0, y0)) {
                        if rect.contains(s) {
                            let el = *z as f64 + oz - if self.use_user_levels { self.level_zero } else { 0.0 };
                            label_bg(&painter, s, &format!("{name}  {el:+.2}"), Color32::from_rgb(235, 200, 120));
                        }
                    }
                }
            }
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
            for w in dense.windows(2) {
                line(w[0] + lift, w[1] + lift, pc, 3.0);
            }
            if self.tool == Tool::WalkPath {
                if let (Some(last), Some(hp)) = (self.walk_path.last(), hover) {
                    if let Some(a) = to_screen(&vp, rect, *last + lift) {
                        painter.line_segment([a, hp], Stroke::new(1.5_f32, pc.gamma_multiply(0.5)));
                    }
                }
            }
            for (i, p) in self.walk_path.iter().enumerate() {
                if let Some(sp) = to_screen(&vp, rect, *p + lift) {
                    painter.circle_filled(sp, 7.0, pc);
                    painter.text(sp, Align2::CENTER_CENTER, (i + 1).to_string(), FontId::proportional(10.0), Color32::BLACK);
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
            painter.text(
                rect.center_top() + EV::new(0.0, 14.0),
                Align2::CENTER_TOP,
                format!("Walk path: click on the floor to add points ({n} points, {len:.0} m) · Backspace: undo · double-click / Enter: done"),
                FontId::proportional(14.0),
                pal.overlay_text,
            );
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
            if ta <= aspect + 1e-3 {
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
        if self.report_job.is_none() {
            if self.loaded_models().next().is_some() {
                self.view_cube(ui, rect);
            }
            if self.settings.nav_bar && self.loaded_models().next().is_some() && self.render_job.is_none() {
                self.nav_bar(ui, rect);
            }
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
        if self.loaded_models().next().is_none() && self.loads.is_empty() {
            // pagina de start (ca Navisworks): Open / New + fisierele recente
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| self.start_page(ui));
        }
        let mut yy = rect.center().y - 30.0 * self.loads.len() as f32 / 2.0;
        for j in &self.loads {
            let r = Rect::from_center_size(Pos2::new(rect.center().x, yy), EV::new(360.0, 24.0));
            painter.rect_filled(r, 4.0, Color32::from_black_alpha(170));
            let mut f = r.shrink(3.0);
            f.set_width(f.width() * j.progress.clamp(0.02, 1.0));
            painter.rect_filled(f, 3.0, accent());
            painter.text(r.center(), Align2::CENTER_CENTER, format!("{} — {} {:.0}%", crate::file_name(&j.path), j.stage, j.progress * 100.0), FontId::proportional(13.0), Color32::WHITE);
            yy += 30.0;
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
                pal.overlay_weak,
            );
        }
    }

    /// Bara de navigare plutitoare (jos, pe centru): ce se foloseste des, fara drum prin ribbon.
    fn nav_bar(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let pal = self.pal();
        let pos = rect.center_bottom() - EV::new(0.0, 12.0);
        egui::Area::new(egui::Id::new("nav_bar")).fixed_pos(pos).pivot(Align2::CENTER_BOTTOM).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
            egui::Frame::none().fill(pal.overlay_btn).rounding(egui::Rounding::same(18.0)).inner_margin(egui::Margin::symmetric(10.0, 5.0)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    ui.spacing_mut().button_padding = EV::new(7.0, 3.0);
                    let b = |ui: &mut egui::Ui, icon: &str, tip: &str, on: bool, enabled: bool| -> bool {
                        let txt = RichText::new(icon).size(16.0);
                        ui.add_enabled(enabled, egui::Button::new(txt).selected(on).frame(on)).on_hover_text(tip).clicked()
                    };
                    let orbit = self.nav == NavMode::Orbit && self.tool == Tool::Select;
                    if b(ui, "🔄", "Orbit (drag to rotate around the point under the cursor)", orbit, true) {
                        self.nav = NavMode::Orbit;
                        self.tool = Tool::Select;
                        self.path_run = None;
                    }
                    let pan = self.nav == NavMode::Pan && self.tool == Tool::Select;
                    if b(ui, "✋", "Pan (drag to move the model sideways)", pan, true) {
                        self.nav = NavMode::Pan;
                        self.tool = Tool::Select;
                        self.path_run = None;
                    }
                    let walk = self.nav == NavMode::Walk && self.tool == Tool::Select;
                    if b(ui, "🚶", "Walk (drag up = forward, sideways = turn; W A S D)", walk, true) {
                        self.nav = NavMode::Walk;
                        self.tool = Tool::Select;
                    }
                    if b(ui, "✏", "Draw a walk path: click points on the floor, double-click to finish", self.tool == Tool::WalkPath, true) {
                        self.path_run = None;
                        self.tool = if self.tool == Tool::WalkPath { Tool::Select } else { Tool::WalkPath };
                        self.nav = NavMode::Walk;
                    }
                    if self.walk_path.len() >= 2 {
                        let playing = self.path_run.as_ref().map(|r| !r.paused).unwrap_or(false);
                        if b(ui, if playing { "⏸" } else { "▶" }, "Walk along the path (Space) · Esc stops", playing, true) {
                            self.walk_path_play();
                        }
                    }
                    if self.nav == NavMode::Walk {
                        let mut v = self.settings.walk_speed;
                        let r = ui.add(egui::DragValue::new(&mut v).range(0.2..=20.0).speed(0.05).max_decimals(1).suffix(" m/s")).on_hover_text("Walk speed (drag or type)");
                        if r.changed() {
                            self.settings.walk_speed = v;
                        }
                        if r.drag_stopped() || r.lost_focus() {
                            self.settings.save();
                        }
                    }
                    ui.separator();
                    let bb = self.scene_aabb();
                    if b(ui, "⛶", "Zoom to the whole model", false, true) {
                        let mut t = self.cam;
                        t.fit(&bb);
                        self.fly_to(t);
                    }
                    if b(ui, "🏠", "Home view (Home key)", false, true) {
                        self.go_home();
                    }
                    let has = !self.selected.is_empty();
                    if b(ui, "🎯", "Focus on the selection (F) · double-click an element does the same", false, has) {
                        let s: Vec<ElemId> = self.selected.iter().copied().collect();
                        self.apply(Action::Focus(s));
                    }
                    if b(ui, "⬚", "Fit Section: section box around the selection", false, has) {
                        self.section_to_selection();
                    }
                    if self.section.enabled && b(ui, "✂", "Turn sectioning off", true, true) {
                        self.section.enabled = false;
                    }
                    if !self.hidden.is_empty() && b(ui, "👁", &format!("Unhide all ({} hidden)", self.hidden.len()), false, true) {
                        self.apply(Action::ShowAll);
                        self.clash_focus = None;
                    }
                    ui.separator();
                    let (ic, tip) = if self.cam.ortho { ("⬚", "Orthographic - click for perspective") } else { ("🔭", "Perspective - click for orthographic") };
                    if b(ui, ic, tip, false, true) {
                        self.cam.ortho = !self.cam.ortho;
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

    fn tree_ui(&self, ui: &mut egui::Ui) -> Vec<Action> {
        let mut acts = Vec::new();
        let ctrl = ui.input(|i| i.modifiers.command);
        if self.loaded_models().next().is_none() {
            ui.label(RichText::new("No model loaded.").weak());
        }
        let eye_btn = |ui: &mut egui::Ui, ids: &[ElemId], acts: &mut Vec<Action>| {
            let all_hidden = !ids.is_empty() && ids.iter().all(|i| self.hidden.contains(i));
            let t = if all_hidden { RichText::new("○").weak() } else { RichText::new("👁") };
            if ui.add(egui::Button::new(t).small().frame(false)).on_hover_text(if all_hidden { "Unhide" } else { "Hide" }).clicked() {
                if all_hidden {
                    acts.push(Action::Show(ids.to_vec()));
                } else {
                    acts.push(Action::Hide(ids.to_vec()));
                }
            }
        };
        let group_menu = |r: &egui::Response, ids: &Vec<ElemId>, acts: &mut Vec<Action>| {
            r.context_menu(|ui| {
                if ui.button("Select").clicked() {
                    acts.push(Action::Select(ids.clone(), false));
                    ui.close_menu();
                }
                if ui.button("Hide").clicked() {
                    acts.push(Action::Hide(ids.clone()));
                    ui.close_menu();
                }
                if ui.button("Hide Unselected").clicked() {
                    acts.push(Action::Isolate(ids.clone()));
                    ui.close_menu();
                }
                if ui.button("Unhide").clicked() {
                    acts.push(Action::Show(ids.clone()));
                    ui.close_menu();
                }
                if ui.button("Focus").clicked() {
                    acts.push(Action::Focus(ids.clone()));
                    ui.close_menu();
                }
            });
        };
        for (mi, m) in self.loaded_models() {
            let Some(tree) = self.trees.get(mi).and_then(|t| t.as_ref()) else { continue };
            let mids: Vec<ElemId> = (0..m.elements.len() as u32).map(|e| ElemId { m: mi as u32, e }).collect();
            let id = ui.make_persistent_id(("model", mi));
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                .show_header(ui, |ui| {
                    eye_btn(ui, &mids, &mut acts);
                    let r = ui.selectable_label(false, RichText::new(format!("📂 {}", m.name)).strong());
                    if r.clicked() {
                        acts.push(Action::Select(mids.clone(), ctrl));
                    }
                    r.context_menu(|ui| {
                        if ui.button("Select").clicked() {
                            acts.push(Action::Select(mids.clone(), false));
                            ui.close_menu();
                        }
                        if ui.button("Hide").clicked() {
                            acts.push(Action::Hide(mids.clone()));
                            ui.close_menu();
                        }
                        if ui.button("Hide Unselected").clicked() {
                            acts.push(Action::Isolate(mids.clone()));
                            ui.close_menu();
                        }
                        if ui.button("Unhide").clicked() {
                            acts.push(Action::Show(mids.clone()));
                            ui.close_menu();
                        }
                        if ui.button("Focus").clicked() {
                            acts.push(Action::Focus(mids.clone()));
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("🗑 Remove model from project").on_hover_text("Closes this model in ManageBIM. The file on disk is not touched.").clicked() {
                            acts.push(Action::RemoveModel(mi));
                            ui.close_menu();
                        }
                    });
                })
                .body(|ui| {
                    let single = tree.roots.len() == 1;
                    if tree.roots.is_empty() {
                        // fara niveluri de grupare: elementele direct sub model
                        Self::tree_elems(self, ui, mi, m, &mids, ctrl, &eye_btn, &group_menu, &mut acts);
                    }
                    for (k, node) in tree.roots.iter().enumerate() {
                        let key = ui.make_persistent_id(("tree", mi, k, &node.label));
                        Self::tree_node(self, ui, mi, m, node, key, single, ctrl, &eye_btn, &group_menu, &mut acts);
                    }
                });
        }
        acts
    }

    /// Un nod al arborelui (recursiv: grupurile de sub el, sau elementele la ultimul nivel).
    #[allow(clippy::too_many_arguments)]
    fn tree_node(
        &self,
        ui: &mut egui::Ui,
        mi: usize,
        m: &crate::model::Model,
        node: &crate::app::TreeNode,
        key: egui::Id,
        open: bool,
        ctrl: bool,
        eye_btn: &dyn Fn(&mut egui::Ui, &[ElemId], &mut Vec<Action>),
        group_menu: &dyn Fn(&egui::Response, &Vec<ElemId>, &mut Vec<Action>),
        acts: &mut Vec<Action>,
    ) {
        let ids: Vec<ElemId> = node.ids.iter().map(|e| ElemId { m: mi as u32, e: *e }).collect();
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), key, open)
            .show_header(ui, |ui| {
                eye_btn(ui, &ids, acts);
                let all_sel = ids.iter().all(|i| self.selected.contains(i));
                let r = ui.selectable_label(all_sel, format!("{}  ({})", node.label, ids.len()));
                if r.clicked() {
                    acts.push(Action::Select(ids.clone(), ctrl));
                }
                group_menu(&r, &ids, acts);
            })
            .body(|ui| {
                if node.children.is_empty() {
                    Self::tree_elems(self, ui, mi, m, &ids, ctrl, eye_btn, group_menu, acts);
                } else {
                    let single = node.children.len() == 1;
                    for (k, ch) in node.children.iter().enumerate() {
                        let ck = key.with((k, &ch.label));
                        Self::tree_node(self, ui, mi, m, ch, ck, single, ctrl, eye_btn, group_menu, acts);
                    }
                }
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn tree_elems(
        &self,
        ui: &mut egui::Ui,
        _mi: usize,
        m: &crate::model::Model,
        ids: &[ElemId],
        ctrl: bool,
        eye_btn: &dyn Fn(&mut egui::Ui, &[ElemId], &mut Vec<Action>),
        group_menu: &dyn Fn(&egui::Response, &Vec<ElemId>, &mut Vec<Action>),
        acts: &mut Vec<Action>,
    ) {
        for (k, id) in ids.iter().enumerate() {
            if k >= 1500 {
                ui.label(RichText::new(format!("… {} more elements (use Find)", ids.len() - k)).weak());
                break;
            }
            let e = &m.elements[id.e as usize];
            ui.horizontal(|ui| {
                eye_btn(ui, &[*id], acts);
                let mut t = RichText::new(&e.name);
                if self.hidden.contains(id) {
                    t = t.weak();
                }
                let r = ui.selectable_label(self.selected.contains(id), t);
                if r.clicked() {
                    acts.push(Action::Select(vec![*id], ctrl));
                }
                if r.double_clicked() {
                    acts.push(Action::Focus(vec![*id]));
                }
                group_menu(&r, &vec![*id], acts);
            });
        }
    }

    /// Bara de sus a arborelui: gruparea curenta + editorul de niveluri (orice proprietate).
    fn tree_config_ui(&mut self, ui: &mut egui::Ui) {
        let levels = self.settings.tree_levels.clone();
        let desc = if levels.is_empty() { "(flat list)".to_string() } else { levels.iter().map(|l| l.prop.clone()).collect::<Vec<_>>().join(" › ") };
        ui.horizontal(|ui| {
            ui.label(RichText::new("Group by").small().weak());
            if ui.selectable_label(self.tree_edit, RichText::new(format!("⚙ {desc}")).small()).on_hover_text("Choose how the Selection Tree is grouped (any property, any order)").clicked() {
                self.tree_edit = !self.tree_edit;
            }
        });
        if !self.tree_edit {
            return;
        }
        let cats: Vec<String> = self.prop_index.cats.iter().map(|c| c.0.clone()).collect();
        let mut new = levels.clone();
        let mut action: Option<(usize, i32)> = None; // (index, -1 sus / +1 jos / 0 sterge)
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::Grid::new("tree_levels").num_columns(4).show(ui, |ui| {
                for (i, lv) in new.iter_mut().enumerate() {
                    ui.label(RichText::new(format!("{}.", i + 1)).weak());
                    egui::ComboBox::from_id_salt(("tl_cat", i)).width(110.0).selected_text(&lv.cat).show_ui(ui, |ui| {
                        for c in &cats {
                            if ui.selectable_label(*c == lv.cat, c).clicked() && *c != lv.cat {
                                lv.cat = c.clone();
                                lv.prop = self.prop_index.props(c).and_then(|v| v.first()).map(|p| p.name.clone()).unwrap_or_default();
                            }
                        }
                    });
                    let props: Vec<String> = self.prop_index.props(&lv.cat).map(|v| v.iter().map(|p| p.name.clone()).collect()).unwrap_or_default();
                    egui::ComboBox::from_id_salt(("tl_prop", i)).width(150.0).selected_text(&lv.prop).show_ui(ui, |ui| {
                        for p in &props {
                            ui.selectable_value(&mut lv.prop, p.clone(), p);
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.small_button("⬆").clicked() {
                            action = Some((i, -1));
                        }
                        if ui.small_button("⬇").clicked() {
                            action = Some((i, 1));
                        }
                        if ui.small_button("🗑").clicked() {
                            action = Some((i, 0));
                        }
                    });
                    ui.end_row();
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("➕ Level").clicked() {
                    new.push(TreeLevel::new(ITEM_CAT, "Name"));
                }
                ui.menu_button(RichText::new("Presets").small(), |ui| {
                    let presets: [(&str, Vec<TreeLevel>); 5] = [
                        ("Level › IFC Type (default)", crate::app::default_tree_levels()),
                        ("IFC Type › Level", vec![TreeLevel::new(ITEM_CAT, "IFC Type"), TreeLevel::new(ITEM_CAT, "Level")]),
                        ("Level › IFC Type › Type Name", vec![TreeLevel::new(ITEM_CAT, "Level"), TreeLevel::new(ITEM_CAT, "IFC Type"), TreeLevel::new("Type", "Type Name")]),
                        ("Level › Material", vec![TreeLevel::new(ITEM_CAT, "Level"), TreeLevel::new("Material", "Material")]),
                        ("Flat list (no grouping)", Vec::new()),
                    ];
                    for (name, lv) in presets {
                        if ui.button(name).clicked() {
                            new = lv;
                            ui.close_menu();
                        }
                    }
                });
                if ui.small_button("💾 Export…").on_hover_text("Save this grouping (.bimtree) to reuse it").clicked() {
                    self.request_path(Purpose::ExportTree);
                }
                if ui.small_button("📂 Import…").clicked() {
                    self.request_path(Purpose::ImportTree);
                }
                if ui.small_button("Done").clicked() {
                    self.tree_edit = false;
                }
            });
        });
        if let Some((i, d)) = action {
            match d {
                0 => {
                    new.remove(i);
                }
                -1 if i > 0 => new.swap(i, i - 1),
                1 if i + 1 < new.len() => new.swap(i, i + 1),
                _ => {}
            }
        }
        if new != self.settings.tree_levels {
            self.set_tree_levels(new);
        }
    }

    fn describe_rules(rules: &[SearchRule]) -> String {
        let mut s = String::new();
        for (i, r) in rules.iter().enumerate() {
            if i > 0 {
                s.push_str(if r.or { "\nOR  " } else { "\nAND " });
            }
            let cat = if r.group.is_empty() { String::new() } else { format!("{} / ", r.group) };
            let v = if r.op.needs_value() { format!(" {}", r.value) } else { String::new() };
            s.push_str(&format!("{cat}{} {}{v}", r.field, r.op.label()));
        }
        s
    }

    fn sets_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("➕ Selection Set")).on_hover_text("Fixed list of the selected items (from any model)").clicked() {
                let n = self.sets.len() + 1;
                self.sets.push(SelSet { name: format!("Selection {n}"), kind: SetKind::Explicit(self.selected.iter().copied().collect()) });
            }
            if ui.button("➕ Search Set").on_hover_text("Define conditions in Find Items, then Save Search Set").clicked() {
                self.open_panel(Panel::Find);
            }
            if ui.add_enabled(!self.sets.is_empty(), egui::Button::new("Export…")).on_hover_text("Share sets with other projects").clicked() {
                self.request_path(Purpose::ExportSets);
            }
            if ui.button("Import…").clicked() {
                self.request_path(Purpose::ImportSets);
            }
            if ui.add_enabled(!self.sets.is_empty(), egui::Button::new("🧭 Navisworks…")).on_hover_text("Export as Navisworks search sets XML (Sets > Import Search Sets)").clicked() {
                self.request_path(Purpose::ExportNavisSets);
            }
        });
        ui.separator();
        if self.sets.is_empty() {
            ui.label(RichText::new("No sets yet.\n\n• Properties tab: tick values (e.g. several IFC types) → Save as Search Set\n• Find Items: conditions → Save Search Set\n• Select items → ➕ Selection Set\n\nSets work across all loaded models and can be used in Clash Detective and TimeLiner.").weak());
        }
        let mut acts = Vec::new();
        let mut del = None;
        let mut edit = None;
        let ctrl = ui.input(|i| i.modifiers.command);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for i in 0..self.sets.len() {
                let ids = self.resolve_set(&self.sets[i]);
                let all_sel = !ids.is_empty() && ids.iter().all(|x| self.selected.contains(x));
                ui.horizontal(|ui| {
                    let (icon, tip) = match &self.sets[i].kind {
                        SetKind::Explicit(_) => ("📋", "Selection set (fixed items)".to_string()),
                        SetKind::Search(r) => ("🔍", format!("Search set (updates automatically)\n\n{}", Self::describe_rules(r))),
                    };
                    if ui.selectable_label(all_sel, icon).on_hover_text(format!("{tip}\n\nClick: select {} items", ids.len())).clicked() {
                        acts.push(Action::Select(ids.clone(), ctrl));
                    }
                    ui.add(egui::TextEdit::singleline(&mut self.sets[i].name).desired_width(118.0));
                    if ui.small_button(format!("{}", ids.len())).on_hover_text("Select these items").clicked() {
                        acts.push(Action::Select(ids.clone(), ctrl));
                    }
                    if ui.small_button("👁").on_hover_text("Hide / unhide").clicked() {
                        if ids.iter().all(|x| self.hidden.contains(x)) {
                            acts.push(Action::Show(ids.clone()));
                        } else {
                            acts.push(Action::Hide(ids.clone()));
                        }
                    }
                    match self.sets[i].kind {
                        SetKind::Search(_) => {
                            if ui.small_button("✏").on_hover_text("Edit the conditions in Find Items").clicked() {
                                edit = Some(i);
                            }
                        }
                        SetKind::Explicit(_) => {
                            if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("⟳").small()).on_hover_text("Update: replace the items with the current selection").clicked() {
                                self.sets[i].kind = SetKind::Explicit(self.selected.iter().copied().collect());
                            }
                        }
                    }
                    if ui.small_button("🗑").on_hover_text("Delete set").clicked() {
                        del = Some(i);
                    }
                });
            }
        });
        if let Some(i) = edit {
            if let SetKind::Search(r) = &self.sets[i].kind {
                self.find_rules = r.clone();
                self.editing_set = Some(i);
                self.open_panel(Panel::Find);
            }
        }
        if let Some(i) = del {
            self.sets.remove(i);
            self.editing_set = None;
        }
        for a in acts {
            self.apply(a);
        }
    }

    /// Navisworks-style condition rows: Category | Property | Condition | Value.
    /// Returns true when Enter was pressed in a value field.
    fn rule_rows(ui: &mut egui::Ui, salt: &str, rules: &mut Vec<SearchRule>, idx: &PropIndex, logic: bool) -> bool {
        let mut del = None;
        let mut enter = false;
        let mut add_or: Vec<SearchRule> = Vec::new();
        for (i, r) in rules.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if logic {
                    if i == 0 {
                        ui.add_sized([40.0, 20.0], egui::Label::new(RichText::new("").small()));
                    } else {
                        let t = if r.or { "OR" } else { "AND" };
                        if ui.add_sized([40.0, 20.0], egui::Button::new(RichText::new(t).small().strong())).on_hover_text("Switch AND / OR (OR starts an alternative condition, like Navisworks \"Or Condition\")").clicked() {
                            r.or = !r.or;
                        }
                    }
                }
                // Category
                let cat_txt = if r.group.is_empty() { if r.field == "Any" { "(any category)".to_string() } else { "(any category)".to_string() } } else { r.group.clone() };
                egui::ComboBox::from_id_salt((salt, "c", i)).width(150.0).selected_text(cat_txt).show_ui(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                        if ui.selectable_label(r.group.is_empty() && r.field == "Any", "(any category)").clicked() {
                            r.group.clear();
                            r.field = "Any".into();
                        }
                        for (c, ps) in &idx.cats {
                            if ui.selectable_label(&r.group == c, format!("{c}  ({})", ps.len())).clicked() && &r.group != c {
                                r.group = c.clone();
                                r.field = ps.first().map(|p| p.name.clone()).unwrap_or_else(|| "(any)".into());
                            }
                        }
                    });
                });
                // Property
                if r.group.is_empty() {
                    let t = if r.field == "Any" { "(any property)".to_string() } else { r.field.clone() };
                    ui.add_enabled_ui(false, |ui| {
                        egui::ComboBox::from_id_salt((salt, "p", i)).width(170.0).selected_text(t).show_ui(ui, |_| {});
                    });
                } else {
                    egui::ComboBox::from_id_salt((salt, "p", i)).width(170.0).selected_text(r.field.clone()).show_ui(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                            ui.selectable_value(&mut r.field, "(any)".to_string(), "(any)");
                            if let Some(ps) = idx.props(&r.group) {
                                for p in ps {
                                    ui.selectable_value(&mut r.field, p.name.clone(), &p.name);
                                }
                            }
                        });
                    });
                }
                // Condition
                egui::ComboBox::from_id_salt((salt, "o", i)).width(110.0).selected_text(r.op.label()).show_ui(ui, |ui| {
                    for o in SearchOp::ALL {
                        ui.selectable_value(&mut r.op, o, o.label());
                    }
                });
                // Value + list of existing values
                if r.op.needs_value() {
                    let resp = ui.add(egui::TextEdit::singleline(&mut r.value).desired_width(150.0).hint_text("value"));
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        enter = true;
                    }
                    if let Some(pv) = idx.prop(&r.group, &r.field) {
                        ui.menu_button("⏷", |ui| {
                            let fid = egui::Id::new((salt, "vf", i));
                            let mut f: String = ui.data_mut(|d| d.get_temp(fid)).unwrap_or_default();
                            ui.set_min_width(280.0);
                            if pv.values.is_empty() {
                                ui.label(RichText::new(format!("{} distinct values - type the value or use 'contains'.", pv.distinct)).weak());
                                return;
                            }
                            ui.add(egui::TextEdit::singleline(&mut f).hint_text("filter values").desired_width(260.0));
                            ui.data_mut(|d| d.insert_temp(fid, f.clone()));
                            ui.label(RichText::new("Click = use value · OR = add as an alternative row").weak().small());
                            let fl = f.to_lowercase();
                            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                                for (v, n) in pv.values.iter().filter(|(v, _)| fl.is_empty() || v.to_lowercase().contains(&fl)).take(1000) {
                                    ui.horizontal(|ui| {
                                        if ui.selectable_label(&r.value == v, format!("{v}   ({n})")).clicked() {
                                            r.value = v.clone();
                                            if !matches!(r.op, SearchOp::Equals | SearchOp::NotEquals | SearchOp::Contains | SearchOp::NotContains) {
                                                r.op = SearchOp::Equals;
                                            }
                                            ui.close_menu();
                                        }
                                        if logic && ui.small_button("OR").on_hover_text("Add an OR row with this value").clicked() {
                                            add_or.push(SearchRule::new(&r.group, &r.field, SearchOp::Equals, v, true));
                                        }
                                    });
                                }
                            });
                        });
                    }
                }
                if ui.small_button("✖").on_hover_text("Remove condition").clicked() {
                    del = Some(i);
                }
            });
        }
        if let Some(i) = del {
            rules.remove(i);
        }
        // un rand gol inlocuit direct, altfel se adauga ca OR
        for r in add_or {
            if let Some(last) = rules.last_mut() {
                if last.group == r.group && last.field == r.field && last.value.trim().is_empty() {
                    last.value = r.value;
                    last.op = SearchOp::Equals;
                    continue;
                }
            }
            rules.push(r);
        }
        enter
    }

    fn find_items_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_top(|ui| {
            // ---- search in (models)
            ui.vertical(|ui| {
                ui.set_width(220.0);
                ui.strong("Search in");
                let ids: Vec<(usize, String)> = self.loaded_models().map(|(i, m)| (i, m.name.clone())).collect();
                ui.horizontal(|ui| {
                    if ui.small_button("All").clicked() {
                        self.find_models_off.clear();
                    }
                    if ui.small_button("None").clicked() {
                        self.find_models_off = ids.iter().map(|x| x.0).collect();
                    }
                });
                egui::ScrollArea::vertical().id_salt("fmodels").max_height(ui.available_height() - 40.0).show(ui, |ui| {
                    for (i, n) in &ids {
                        let mut on = !self.find_models_off.contains(i);
                        if ui.checkbox(&mut on, n.as_str()).on_hover_text(n.as_str()).changed() {
                            if on {
                                self.find_models_off.remove(i);
                            } else {
                                self.find_models_off.insert(*i);
                            }
                        }
                    }
                    if ids.is_empty() {
                        ui.label(RichText::new("No models loaded.").weak());
                    }
                });
                ui.checkbox(&mut self.find_in_selection, "Current selection only");
            });
            ui.separator();
            // ---- conditions
            ui.vertical(|ui| {
                if let Some(si) = self.editing_set {
                    let name = self.sets.get(si).map(|s| s.name.clone()).unwrap_or_default();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Editing search set “{name}”")).strong().color(accent()));
                        if ui.button("💾 Update set").clicked() {
                            if let Some(s) = self.sets.get_mut(si) {
                                s.kind = SetKind::Search(self.find_rules.clone());
                            }
                            self.toast(format!("Search set “{name}” updated."));
                            self.editing_set = None;
                        }
                        if ui.button("Stop editing").clicked() {
                            self.editing_set = None;
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.add_space(46.0);
                    for (w, t) in [(156.0, "Category"), (176.0, "Property"), (116.0, "Condition"), (180.0, "Value")] {
                        ui.add_sized([w, 16.0], egui::Label::new(RichText::new(t).strong()));
                    }
                });
                if self.find_rules.is_empty() {
                    self.find_rules.push(SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "", false));
                }
                let enter = egui::ScrollArea::vertical()
                    .id_salt("findrules")
                    .max_height((ui.available_height() - 70.0).max(60.0))
                    .show(ui, |ui| Self::rule_rows(ui, "find", &mut self.find_rules, &self.prop_index, true))
                    .inner;
                ui.horizontal_wrapped(|ui| {
                    if ui.button("➕ Condition").clicked() {
                        let last = self.find_rules.last().cloned();
                        let r = match last {
                            Some(l) if !l.group.is_empty() => SearchRule::new(&l.group, &l.field, SearchOp::Equals, "", false),
                            _ => SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "", false),
                        };
                        self.find_rules.push(r);
                    }
                    if ui.button("➕ OR Condition").clicked() {
                        let last = self.find_rules.last().cloned();
                        let r = match last {
                            Some(l) if !l.group.is_empty() => SearchRule::new(&l.group, &l.field, SearchOp::Equals, "", true),
                            _ => SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "", true),
                        };
                        self.find_rules.push(r);
                    }
                    ui.separator();
                    let go = ui.add(egui::Button::new(RichText::new("🔍 Find All").strong().color(Color32::WHITE)).fill(accent())).clicked();
                    if go || enter {
                        let r = self.find_scoped(&self.find_rules.clone(), self.find_in_selection);
                        self.find_count = Some(r.len());
                        self.apply(Action::Select(r, false));
                    }
                    if ui.button("Clear").clicked() {
                        self.find_rules = vec![SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "", false)];
                        self.find_count = None;
                    }
                    ui.separator();
                    if ui.button("💾 Save Search Set").on_hover_text("Saves the conditions: the set re-evaluates itself on every model, also after models are updated").clicked() {
                        let n = self.sets.len() + 1;
                        self.sets.push(SelSet { name: Self::rules_name(&self.find_rules).unwrap_or(format!("Search {n}")), kind: SetKind::Search(self.find_rules.clone()) });
                        self.open_panel(Panel::Sets);
                        self.toast("Search set saved (Sets panel).");
                    }
                    if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("💾 Save Selection Set")).on_hover_text("Saves the currently selected items as a fixed list").clicked() {
                        let n = self.sets.len() + 1;
                        self.sets.push(SelSet { name: format!("Selection {n}"), kind: SetKind::Explicit(self.selected.iter().copied().collect()) });
                        self.open_panel(Panel::Sets);
                    }
                    if let Some(n) = self.find_count {
                        ui.label(RichText::new(format!("{n} items found")).strong());
                    }
                });
            });
        });
    }

    /// A readable name for a set made from rules ("IfcColumn + IfcWindow").
    fn rules_name(rules: &[SearchRule]) -> Option<String> {
        let vals: Vec<&str> = rules.iter().filter(|r| r.op == SearchOp::Equals && !r.value.is_empty()).map(|r| r.value.as_str()).collect();
        if vals.is_empty() || vals.len() != rules.len() {
            return None;
        }
        let mut s = vals.iter().take(4).cloned().collect::<Vec<_>>().join(" + ");
        if vals.len() > 4 {
            s.push_str(&format!(" +{}", vals.len() - 4));
        }
        Some(s)
    }

    /// Selection Tree in "Properties" mode: tick values (several categories / types / levels…)
    /// from all models, then save them as a search set.
    fn props_tree_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.prop_filter).hint_text("🔍 filter categories, properties, values").desired_width(f32::INFINITY));
        });
        let nchk = self.prop_checked.len();
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(nchk > 0, egui::Button::new("💾 Save as Search Set")).on_hover_text("Ticked values are combined with OR, from all models").clicked() {
                let rules = self.rules_from_checked();
                let n = self.sets.len() + 1;
                self.sets.push(SelSet { name: Self::rules_name(&rules).unwrap_or(format!("Search {n}")), kind: SetKind::Search(rules) });
                self.toast("Search set saved (Sets).");
            }
            if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("Save Selection Set")).clicked() {
                let n = self.sets.len() + 1;
                self.sets.push(SelSet { name: format!("Selection {n}"), kind: SetKind::Explicit(self.selected.iter().copied().collect()) });
            }
            if ui.add_enabled(nchk > 0, egui::Button::new("🔍 Edit in Find Items")).on_hover_text("Open these conditions in Find Items to refine them").clicked() {
                self.find_rules = self.rules_from_checked();
                self.open_panel(Panel::Find);
            }
            if ui.add_enabled(nchk > 0, egui::Button::new("Clear")).clicked() {
                self.prop_checked.clear();
                self.clear_selection();
            }
        });
        if nchk > 0 {
            ui.label(RichText::new(format!("{nchk} values ticked · {} items selected", self.selected.len())).color(accent()));
        } else {
            ui.label(RichText::new("Tick values to select items from all models (e.g. several IFC types).").weak().small());
        }
        ui.separator();
        let fl = self.prop_filter.trim().to_lowercase();
        let mut changed = false;
        let idx = std::mem::take(&mut self.prop_index);
        egui::ScrollArea::vertical().id_salt("proptree").auto_shrink([false, false]).show(ui, |ui| {
            if idx.cats.is_empty() {
                ui.label(RichText::new("No model loaded.").weak());
            }
            for (ci, (cat, props)) in idx.cats.iter().enumerate() {
                let cat_hit = fl.is_empty() || cat.to_lowercase().contains(&fl);
                let shown: Vec<&PropValues> = props
                    .iter()
                    .filter(|p| cat_hit || p.name.to_lowercase().contains(&fl) || p.values.iter().any(|(v, _)| v.to_lowercase().contains(&fl)))
                    .collect();
                if shown.is_empty() {
                    continue;
                }
                let checked_in_cat = self.prop_checked.iter().filter(|k| &k.0 == cat).count();
                let mut head = RichText::new(format!("{cat}  ({})", props.len())).strong();
                if checked_in_cat > 0 {
                    head = head.color(accent());
                }
                egui::CollapsingHeader::new(head).id_salt(("pc", ci)).default_open(ci == 0 || !fl.is_empty()).show(ui, |ui| {
                    for (pi, p) in shown.iter().enumerate() {
                        let prop_hit = cat_hit || p.name.to_lowercase().contains(&fl);
                        let mut ph = RichText::new(format!("{}  ({} values)", p.name, p.distinct));
                        if self.prop_checked.iter().any(|k| &k.0 == cat && k.1 == p.name) {
                            ph = ph.color(accent());
                        }
                        egui::CollapsingHeader::new(ph).id_salt(("pp", ci, pi, &p.name)).default_open(!fl.is_empty() && !prop_hit).show(ui, |ui| {
                            if p.values.is_empty() {
                                ui.label(RichText::new(format!("{} distinct values - use Find Items with 'contains'.", p.distinct)).weak().small());
                                return;
                            }
                            let vals: Vec<&(String, u32)> = p.values.iter().filter(|(v, _)| prop_hit || v.to_lowercase().contains(&fl)).collect();
                            for (k, (v, n)) in vals.iter().enumerate() {
                                if k >= 500 {
                                    ui.label(RichText::new(format!("… {} more - use the filter", vals.len() - k)).weak().small());
                                    break;
                                }
                                let key = (cat.clone(), p.name.clone(), v.clone());
                                let mut on = self.prop_checked.contains(&key);
                                if ui.checkbox(&mut on, format!("{v}   ({n})")).changed() {
                                    if on {
                                        self.prop_checked.insert(key);
                                    } else {
                                        self.prop_checked.remove(&key);
                                    }
                                    changed = true;
                                }
                            }
                        });
                    }
                });
            }
        });
        self.prop_index = idx;
        if changed {
            if self.prop_checked.is_empty() {
                self.clear_selection();
            } else {
                let rules = self.rules_from_checked();
                let ids = self.find_scoped(&rules, false);
                self.apply(Action::Select(ids, false));
            }
        }
    }

    fn properties_panel(&mut self, ui: &mut egui::Ui) {
        let Some(pid) = self.primary.or_else(|| self.selected.iter().next().copied()) else {
            ui.label(RichText::new("Select an element in the 3D view or in the tree.").weak());
            return;
        };
        let sel: Vec<ElemId> = self.selected.iter().copied().collect();
        if sel.len() > 1 {
            ui.label(format!("{} elements selected — showing the last one.", sel.len()));
        }
        let mut acts = Vec::new();
        ui.horizontal_wrapped(|ui| {
            if ui.small_button("Hide").clicked() {
                acts.push(Action::Hide(sel.clone()));
            }
            if ui.small_button("Hide Unselected").clicked() {
                acts.push(Action::Isolate(sel.clone()));
            }
            if ui.small_button("Focus").clicked() {
                acts.push(Action::Focus(sel.clone()));
            }
            if sel.len() == 2 && ui.small_button("Min. distance").clicked() {
                self.min_distance_selected();
            }
        });
        // aspect
        egui::CollapsingHeader::new("Appearance override").default_open(false).show(ui, |ui| {
            let mut c = Color32::from_rgba_unmultiplied(self.override_color[0], self.override_color[1], self.override_color[2], 255);
            ui.horizontal(|ui| {
                ui.label("Colour");
                if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
                    self.override_color = [c.r(), c.g(), c.b(), self.override_color[3]];
                }
            });
            let mut tr = 100.0 - self.override_color[3] as f32 / 255.0 * 100.0;
            if ui.add(egui::Slider::new(&mut tr, 0.0..=90.0).text("Transparency %")).changed() {
                self.override_color[3] = ((100.0 - tr) / 100.0 * 255.0) as u8;
            }
            ui.horizontal(|ui| {
                if ui.button("Apply to selection").clicked() {
                    for id in &sel {
                        self.overrides.insert(*id, self.override_color);
                    }
                    self.colors_dirty = true;
                }
                if ui.button("Reset").clicked() {
                    for id in &sel {
                        self.overrides.remove(id);
                    }
                    self.colors_dirty = true;
                }
                if ui.button("Reset all").clicked() {
                    self.overrides.clear();
                    self.colors_dirty = true;
                }
            });
        });
        ui.separator();
        let Some((m, e)) = self.elem(pid) else { return };
        let mut groups: Vec<(String, Vec<(String, String)>)> = Vec::new();
        groups.push(("File".into(), vec![("Model".into(), m.name.clone()), ("Path".into(), m.path.clone())]));
        for p in &e.props {
            match groups.iter_mut().find(|g| g.0 == p.group) {
                Some(g) => g.1.push((p.name.clone(), p.value.clone())),
                None => groups.push((p.group.clone(), vec![(p.name.clone(), p.value.clone())])),
            }
        }
        let bb = e.aabb;
        let oz = self.origin.lock().unwrap().map(|o| o.z as f32).unwrap_or(0.0);
        groups.push((
            "Geometry".into(),
            vec![
                ("Triangles".into(), e.tri_count.to_string()),
                ("Size X×Y×Z (m)".into(), format!("{:.3} × {:.3} × {:.3}", bb.size().x, bb.size().y, bb.size().z)),
                ("Elevation min / max Z (m)".into(), format!("{:.3} / {:.3}", bb.min.z + oz, bb.max.z + oz)),
            ],
        ));
        let mut copy = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new(&e.name).strong().size(15.0));
            ui.label(RichText::new(&e.ifc_type).color(accent()));
            for (gi, (g, rows)) in groups.iter().enumerate() {
                egui::CollapsingHeader::new(RichText::new(g).strong()).id_salt(("pg", gi, g)).default_open(gi < 6).show(ui, |ui| {
                    egui::Grid::new(("pgrid", gi, g)).num_columns(2).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                        for (n, v) in rows {
                            ui.label(RichText::new(n).weak());
                            let r = ui.add(egui::Label::new(v).wrap().sense(Sense::click()));
                            r.context_menu(|ui| {
                                if ui.button("Copy value").clicked() {
                                    copy = Some(v.clone());
                                    ui.close_menu();
                                }
                            });
                            ui.end_row();
                        }
                    });
                });
            }
        });
        if let Some(c) = copy {
            ui.ctx().copy_text(c);
        }
        for a in acts {
            self.apply(a);
        }
    }

    fn selector_combo(&self, ui: &mut egui::Ui, id: &str, s: &mut Selector) {
        egui::ComboBox::from_id_salt(id).width(190.0).selected_text(s.label()).show_ui(ui, |ui| {
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                ui.selectable_value(s, Selector { kind: SelKind::All, value: String::new() }, "All elements");
                ui.separator();
                for (_, m) in self.loaded_models() {
                    let v = Selector { kind: SelKind::Model, value: m.name.clone() };
                    let l = v.label();
                    ui.selectable_value(s, v, l);
                }
                if !self.sets.is_empty() {
                    ui.separator();
                }
                for set in &self.sets {
                    let v = Selector { kind: SelKind::Set, value: set.name.clone() };
                    let l = v.label();
                    ui.selectable_value(s, v, l);
                }
                ui.separator();
                for t in &self.cache_types {
                    let v = Selector { kind: SelKind::Type, value: t.clone() };
                    let l = v.label();
                    ui.selectable_value(s, v, l);
                }
            });
        });
    }

    fn clash_ui(&mut self, ui: &mut egui::Ui) {
        if self.cur_test >= self.clash_tests.len() {
            self.cur_test = 0;
        }
        ui.horizontal_top(|ui| {
            // ---- tests
            ui.vertical(|ui| {
                ui.set_width(210.0);
                ui.horizontal(|ui| {
                    ui.strong("Tests");
                    if ui.small_button("➕").on_hover_text("New test").clicked() {
                        let n = self.clash_tests.len() + 1;
                        self.clash_tests.push(ClashTest::new(format!("Test {n}")));
                        self.cur_test = self.clash_tests.len() - 1;
                        self.cur_result = None;
                        self.sel_results.clear();
                    }
                    if ui.add_enabled(self.clash_tests.len() > 1, egui::Button::new("🗑").small()).on_hover_text("Delete test").clicked() {
                        self.clash_tests.remove(self.cur_test);
                        self.cur_test = 0;
                        self.cur_result = None;
                        self.sel_results.clear();
                        self.clash_focus = None;
                        self.colors_dirty = true;
                    }
                    if ui.add_enabled(self.clash_job.is_none(), egui::Button::new("▶ Run all").small()).on_hover_text("Run every test one after another").clicked() {
                        self.clash_queue = (1..self.clash_tests.len()).collect();
                        self.cur_test = 0;
                        self.run_clash(0);
                    }
                });
                egui::ScrollArea::vertical().id_salt("tests").max_height(ui.available_height() - 4.0).show(ui, |ui| {
                    for i in 0..self.clash_tests.len() {
                        let t = &self.clash_tests[i];
                        let open = t.count(ClashStatus::New) + t.count(ClashStatus::Active);
                        let txt = format!("{}  —  {} open / {} total", t.name, open, t.results.len());
                        if ui.selectable_label(i == self.cur_test, txt).clicked() && i != self.cur_test {
                            self.cur_test = i;
                            self.cur_result = None;
                            self.sel_results.clear();
                            self.clash_focus = None;
                            self.colors_dirty = true;
                        }
                    }
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                let ti = self.cur_test;
                // ---- setup (Select tab in Navisworks)
                let mut test = self.clash_tests[ti].clone();
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut test.name).desired_width(110.0));
                    ui.label("A:");
                    self.selector_combo(ui, "selA", &mut test.a);
                    ui.label("B:");
                    self.selector_combo(ui, "selB", &mut test.b);
                    egui::ComboBox::from_id_salt("ctype").selected_text(test.ty.label()).show_ui(ui, |ui| {
                        for t in [ClashType::Hard, ClashType::Clearance, ClashType::Duplicates] {
                            ui.selectable_value(&mut test.ty, t, t.label());
                        }
                    });
                    ui.label("Tolerance");
                    ui.add(egui::DragValue::new(&mut test.tol_mm).range(0.0..=5000.0).speed(1.0).suffix(" mm"));
                    if let Some(job) = &self.clash_job {
                        ui.add(egui::ProgressBar::new(job.prog.frac()).desired_width(160.0).show_percentage());
                        if ui.button("Cancel").clicked() {
                            job.prog.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    } else if ui.add_enabled(self.loaded_models().next().is_some(), egui::Button::new(RichText::new("▶ Run Test").strong().color(Color32::WHITE)).fill(accent())).clicked() {
                        self.clash_tests[ti].name = test.name.clone();
                        self.clash_tests[ti].a = test.a.clone();
                        self.clash_tests[ti].b = test.b.clone();
                        self.clash_tests[ti].ty = test.ty;
                        self.clash_tests[ti].tol_mm = test.tol_mm;
                        self.run_clash(ti);
                    }
                });
                {
                    let t = &mut self.clash_tests[ti];
                    t.name = test.name;
                    t.a = test.a;
                    t.b = test.b;
                    t.ty = test.ty;
                    t.tol_mm = test.tol_mm;
                }
                // ---- sub-tabs
                ui.horizontal(|ui| {
                    let nres = self.clash_tests[ti].results.len();
                    let nrules = self.clash_tests[ti].rules.count(self.clash_tests[ti].ignore_same_file);
                    ui.selectable_value(&mut self.clash_subtab, 0, format!("Results ({nres})"));
                    ui.selectable_value(&mut self.clash_subtab, 1, format!("Rules ({nrules})"));
                });
                ui.separator();
                if self.clash_subtab == 0 {
                    self.clash_results(ui, ti);
                } else {
                    self.clash_rules_ui(ui, ti);
                }
            });
        });
    }

    fn chips(ui: &mut egui::Ui, salt: &str, list: &mut Vec<String>, options: &[String], add_text: &str) {
        let mut del = None;
        ui.horizontal_wrapped(|ui| {
            for (i, v) in list.iter().enumerate() {
                if ui.add(egui::Button::new(format!("{v}  ✖")).small()).on_hover_text("Remove").clicked() {
                    del = Some(i);
                }
            }
            let mut pick: Option<String> = None;
            egui::ComboBox::from_id_salt((salt, "add")).width(150.0).selected_text(RichText::new(add_text).weak()).show_ui(ui, |ui| {
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    if options.is_empty() {
                        ui.label(RichText::new("(none available)").weak());
                    }
                    for o in options {
                        if !list.contains(o) && ui.selectable_label(false, o).clicked() {
                            pick = Some(o.clone());
                        }
                    }
                });
            });
            if let Some(p) = pick {
                list.push(p);
            }
        });
        if let Some(i) = del {
            list.remove(i);
        }
    }

    fn clash_rules_ui(&mut self, ui: &mut egui::Ui, ti: usize) {
        let mut rules = self.clash_tests[ti].rules.clone();
        let mut same_file = self.clash_tests[ti].ignore_same_file;
        let set_names: Vec<String> = self.sets.iter().map(|s| s.name.clone()).collect();
        let props = self.cache_props.clone();
        let idx = std::mem::take(&mut self.prop_index);
        egui::ScrollArea::vertical().id_salt("crules").auto_shrink([false, false]).show(ui, |ui| {
            ui.columns(2, |c| {
                let ui = &mut c[0];
                ui.label(RichText::new("Ignore clashes between items…").strong());
                ui.checkbox(&mut same_file, "in the same file (model)");
                ui.checkbox(&mut rules.same_level, "on the same level");
                ui.checkbox(&mut rules.same_type, "of the same IFC type / category");
                ui.add_space(4.0);
                ui.label("with the same value of property (e.g. System):");
                Self::chips(ui, "sameprop", &mut rules.same_props, &props, "➕ add property");
                ui.add_space(4.0);
                ui.label("that are both in the same selection set:");
                Self::chips(ui, "sameset", &mut rules.same_sets, &set_names, "➕ add set");

                let ui = &mut c[1];
                ui.label(RichText::new("Exclude items from this test (they never clash)").strong());
                ui.label("Items in these selection / search sets:");
                Self::chips(ui, "exset", &mut rules.exclude_sets, &set_names, "➕ add set");
                ui.add_space(4.0);
                ui.label("Items matching any of these rules:");
                Self::rule_rows(ui, "exrule", &mut rules.exclude_rules, &idx, false);
                ui.horizontal(|ui| {
                    if ui.button("➕ Exclude rule").clicked() {
                        rules.exclude_rules.push(SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "", false));
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Examples: IFC Type = IfcSpace · Name contains Insulation · Phase = Demolition").weak().small());
            });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Rules apply when the test runs. Existing results that a rule now excludes are removed.").weak());
                if self.clash_job.is_none() && ui.button("▶ Re-run test with these rules").clicked() {
                    self.clash_tests[ti].rules = rules.clone();
                    self.clash_tests[ti].ignore_same_file = same_file;
                    self.run_clash(ti);
                    self.clash_subtab = 0;
                }
            });
        });
        self.prop_index = idx;
        self.clash_tests[ti].rules = rules;
        self.clash_tests[ti].ignore_same_file = same_file;
    }

    fn clash_results(&mut self, ui: &mut egui::Ui, ti: usize) {
        // ---- display options
        ui.horizontal_wrapped(|ui| {
            ui.label("Display:");
            let before = (self.clash_others, self.clash_highlight_all);
            egui::ComboBox::from_id_salt("others").width(190.0).selected_text(self.clash_others.label()).show_ui(ui, |ui| {
                for o in [Others::Dim, Others::Hide, Others::Normal] {
                    ui.selectable_value(&mut self.clash_others, o, o.label());
                }
            });
            ui.checkbox(&mut self.clash_highlight_all, "Highlight all clashes").on_hover_text("Colour the items of every open clash (A light red, B light green)");
            ui.checkbox(&mut self.clash_auto_zoom, "Auto-zoom");
            if before != (self.clash_others, self.clash_highlight_all) {
                self.colors_dirty = true;
            }
            if ui.button("Show all clashes").on_hover_text("Select every open clash in the list and show only them").clicked() {
                let idx: Vec<usize> = self.clash_tests[ti].results.iter().enumerate().filter(|(_, r)| r.status != ClashStatus::Resolved && self.clash_filter.map_or(true, |f| r.status == f)).map(|(i, _)| i).collect();
                self.sel_results = idx.iter().copied().collect();
                self.clash_others = Others::Hide;
                self.apply(Action::FocusClashes(ti, idx));
            }
        });
        // ---- status / grouping / export
        ui.horizontal_wrapped(|ui| {
            for s in ClashStatus::ALL {
                let n = self.clash_tests[ti].count(s);
                ui.label(RichText::new(format!("{} {}", s.label(), n)).color(c32(s.color())));
            }
            ui.separator();
            let fl = self.clash_filter.map(|s| s.label()).unwrap_or("All statuses");
            let before = self.clash_filter;
            egui::ComboBox::from_id_salt("cfilter").selected_text(fl).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.clash_filter, None, "All statuses");
                for s in ClashStatus::ALL {
                    ui.selectable_value(&mut self.clash_filter, Some(s), s.label());
                }
            });
            if before != self.clash_filter {
                self.colors_dirty = true;
            }
            ui.separator();
            let sel: Vec<usize> = self.sel_results.iter().copied().collect();
            if ui.add_enabled(!sel.is_empty(), egui::Button::new("📁 New Group")).on_hover_text("Group the selected clashes (Ctrl / Shift + click to select several)").clicked() {
                self.new_group(ti, &sel);
            }
            if ui.add_enabled(!sel.is_empty(), egui::Button::new("Ungroup")).clicked() {
                self.ungroup(ti, &sel);
            }
            ui.menu_button("Auto-group ⏷", |ui| {
                for g in GroupBy::ALL {
                    if g == GroupBy::Proximity {
                        ui.horizontal(|ui| {
                            if ui.button(g.label()).clicked() {
                                self.auto_group(ti, g, self.group_cell);
                                ui.close_menu();
                            }
                            ui.add(egui::DragValue::new(&mut self.group_cell).range(0.5..=100.0).speed(0.5).suffix(" m"));
                        });
                    } else if ui.button(format!("by {}", g.label())).clicked() {
                        self.auto_group(ti, g, self.group_cell);
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button("Remove all groups").clicked() {
                    let all: Vec<usize> = (0..self.clash_tests[ti].results.len()).collect();
                    self.ungroup(ti, &all);
                    ui.close_menu();
                }
            });
            if ui.small_button("Expand all").clicked() {
                self.open_groups = self.group_names(ti).into_iter().collect();
            }
            if ui.small_button("Collapse all").clicked() {
                self.open_groups.clear();
            }
            ui.separator();
            if ui.button("HTML report").clicked() {
                self.request_path(Purpose::ExportClashHtml);
            }
            if ui.button("CSV").clicked() {
                self.request_path(Purpose::ExportClashCsv);
            }
            if ui.button("📤 BCF").on_hover_text("Open clashes as BCF topics (camera + the two elements) - Revit, ACC, Solibri, BIMcollab, Navisworks").clicked() {
                self.request_path(Purpose::ExportClashBcf);
            }
            if ui.button("🧭 Navisworks").on_hover_text("Open clashes as Navisworks viewpoints with a section box around each clash (Saved Viewpoints > Import Viewpoints)").clicked() {
                self.request_path(Purpose::ExportClashNavis);
            }
            let lr = self.clash_tests[ti].last_run.clone();
            if !lr.is_empty() {
                ui.label(RichText::new(format!("Last run: {lr}")).weak().small());
            }
        });
        ui.separator();
        self.clash_table(ui, ti);
    }

    fn clash_table(&mut self, ui: &mut egui::Ui, ti: usize) {
        #[derive(Clone)]
        enum Row {
            Group(String, Vec<usize>),
            Item(usize, bool),
        }
        let filter = self.clash_filter;
        let res_len = self.clash_tests[ti].results.len();
        let pass = |r: &ClashResult| filter.map_or(true, |f| r.status == f);
        // ---- randuri afisate: grupuri (la pozitia primului membru) + rezultate negrupate
        let mut rows: Vec<Row> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        {
            let results = &self.clash_tests[ti].results;
            for (i, r) in results.iter().enumerate() {
                if r.group.is_empty() {
                    if pass(r) {
                        rows.push(Row::Item(i, false));
                    }
                } else if seen.insert(r.group.clone()) {
                    let members: Vec<usize> = results.iter().enumerate().filter(|(_, x)| x.group == r.group && pass(x)).map(|(k, _)| k).collect();
                    if !members.is_empty() {
                        rows.push(Row::Group(r.group.clone(), members.clone()));
                        if self.open_groups.contains(&r.group) {
                            rows.extend(members.into_iter().map(|k| Row::Item(k, true)));
                        }
                    }
                }
            }
        }
        if rows.is_empty() {
            ui.label(RichText::new("No results. Set up selections A / B and press “Run Test”. Click a result to view it in 3D; Ctrl / Shift + click selects several.").weak());
            return;
        }
        let items: Vec<usize> = rows.iter().filter_map(|r| if let Row::Item(i, _) = r { Some(*i) } else { None }).collect();
        // ---- Page Up / Down
        let (pu, pd) = ui.input(|i| (i.key_pressed(Key::PageUp), i.key_pressed(Key::PageDown)));
        if (pu || pd) && !items.is_empty() {
            let pos = self.cur_result.and_then(|c| items.iter().position(|r| *r == c));
            let next = match (pos, pd) {
                (None, _) => 0,
                (Some(p), true) => (p + 1).min(items.len() - 1),
                (Some(p), false) => p.saturating_sub(1),
            };
            self.sel_results = [items[next]].into_iter().collect();
            self.apply(Action::FocusClash(ti, items[next]));
        }
        // locatia: axe + nivel (C-4 / Level 2) cand exista axe, altfel nivelul elementului A
        let grids = self.scene_grids();
        let use_grid = !grids.grids.is_empty();
        let labels: Vec<(String, String, String)> = (0..res_len)
            .map(|i| {
                let r = &self.clash_tests[ti].results[i];
                let la = self.elem(r.a).map(|(m, e)| format!("{} [{}]", e.name, m.name)).unwrap_or_default();
                let lb = self.elem(r.b).map(|(m, e)| format!("{} [{}]", e.name, m.name)).unwrap_or_default();
                let lv = if use_grid { crate::app::grid_location_in(&grids, glam::Vec3::from(r.point)) } else { self.elem(r.a).map(|(_, e)| e.storey.clone()).unwrap_or_default() };
                (la, lb, lv)
            })
            .collect();
        let group_status: HashMap<String, ClashStatus> = rows
            .iter()
            .filter_map(|r| if let Row::Group(n, m) = r { Some((n.clone(), self.group_status(ti, m))) } else { None })
            .collect();
        let pal = self.pal();
        let sel = self.sel_results.clone();
        let open_groups = self.open_groups.clone();
        let mods = ui.input(|i| i.modifiers);
        let mut clicked: Option<Row> = None;
        let mut toggle: Option<String> = None;
        let mut rename: Option<(String, String)> = None;
        let test = &mut self.clash_tests[ti];
        let mh = (ui.available_height() - 26.0).max(60.0);
        TableBuilder::new(ui)
            .max_scroll_height(mh)
            .auto_shrink([false, true])
            .striped(true)
            .resizable(true)
            .sense(Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::initial(230.0).clip(true))
            .column(Column::exact(92.0))
            .column(Column::exact(66.0))
            .column(Column::initial(200.0).clip(true))
            .column(Column::initial(200.0).clip(true))
            .column(Column::initial(90.0).clip(true))
            .column(Column::initial(118.0).clip(true))
            .column(Column::initial(90.0))
            .column(Column::remainder().at_least(120.0))
            .header(20.0, |mut h| {
                for n in ["Name", "Status", "Distance", "Item A", "Item B", if use_grid { "Location" } else { "Level" }, "Found", "Assigned to", "Comment"] {
                    h.col(|ui| {
                        ui.strong(n);
                    });
                }
            })
            .body(|body| {
                body.rows(22.0, rows.len(), |mut row| {
                    let k = row.index();
                    match rows[k].clone() {
                        Row::Group(name, members) => {
                            row.set_selected(members.iter().all(|m| sel.contains(m)));
                            let open = open_groups.contains(&name);
                            row.col(|ui| {
                                if ui.add(egui::Button::new(if open { "⏷" } else { "⏵" }).small().frame(false)).clicked() {
                                    toggle = Some(name.clone());
                                }
                                ui.label("📁");
                                let id = egui::Id::new(("gname", ti, name.clone()));
                                let mut buf = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| name.clone());
                                let resp = ui.add(egui::TextEdit::singleline(&mut buf).desired_width(f32::INFINITY).font(egui::TextStyle::Body));
                                if resp.has_focus() {
                                    ui.data_mut(|d| d.insert_temp(id, buf.clone()));
                                }
                                if resp.lost_focus() {
                                    if !buf.trim().is_empty() && buf.trim() != name {
                                        rename = Some((name.clone(), buf.trim().to_string()));
                                    }
                                    ui.data_mut(|d| d.remove::<String>(id));
                                }
                            });
                            row.col(|ui| {
                                let gs = group_status[&name];
                                let mut st = gs;
                                egui::ComboBox::from_id_salt(("gst", ti, name.clone())).width(80.0).selected_text(RichText::new(gs.label()).color(c32(gs.color()))).show_ui(ui, |ui| {
                                    for s in ClashStatus::ALL {
                                        ui.selectable_value(&mut st, s, RichText::new(s.label()).color(c32(s.color())));
                                    }
                                });
                                if st != gs {
                                    for m in &members {
                                        test.results[*m].status = st;
                                    }
                                }
                            });
                            row.col(|ui| {
                                let d = members.iter().map(|m| test.results[*m].dist).fold(f32::INFINITY, f32::min);
                                ui.label(format!("{d:.3}"));
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(format!("{} clashes", members.len())).strong());
                            });
                            row.col(|_| {});
                            row.col(|ui| {
                                let l0 = &labels[members[0]].2;
                                let same = members.iter().all(|m| &labels[*m].2 == l0);
                                ui.label(if same { l0.clone() } else { "Mixed".into() });
                            });
                            row.col(|_| {});
                            row.col(|ui| {
                                let mut a = test.results[members[0]].assigned.clone();
                                if ui.add(egui::TextEdit::singleline(&mut a).desired_width(f32::INFINITY)).changed() {
                                    for m in &members {
                                        test.results[*m].assigned = a.clone();
                                    }
                                }
                            });
                            row.col(|ui| {
                                let mut c = test.results[members[0]].comment.clone();
                                if ui.add(egui::TextEdit::singleline(&mut c).desired_width(f32::INFINITY).hint_text("comment for the whole group")).changed() {
                                    for m in &members {
                                        test.results[*m].comment = c.clone();
                                    }
                                }
                            });
                            if row.response().clicked() {
                                clicked = Some(Row::Group(name, members));
                            }
                        }
                        Row::Item(i, indent) => {
                            row.set_selected(sel.contains(&i));
                            let r = &mut test.results[i];
                            row.col(|ui| {
                                if indent {
                                    ui.add_space(22.0);
                                }
                                ui.label(&r.name);
                            });
                            row.col(|ui| {
                                egui::ComboBox::from_id_salt(("st", i)).width(80.0).selected_text(RichText::new(r.status.label()).color(c32(r.status.color()))).show_ui(ui, |ui| {
                                    for s in ClashStatus::ALL {
                                        ui.selectable_value(&mut r.status, s, RichText::new(s.label()).color(c32(s.color())));
                                    }
                                });
                            });
                            row.col(|ui| {
                                ui.label(format!("{:.3}", r.dist));
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(&labels[i].0).color(pal.elem_a));
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(&labels[i].1).color(pal.elem_b));
                            });
                            row.col(|ui| {
                                ui.label(&labels[i].2);
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(&r.found).small());
                            });
                            row.col(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut r.assigned).desired_width(f32::INFINITY));
                            });
                            row.col(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut r.comment).desired_width(f32::INFINITY));
                            });
                            if row.response().clicked() {
                                clicked = Some(Row::Item(i, indent));
                            }
                        }
                    }
                });
            });
        if let Some(g) = toggle {
            if !self.open_groups.remove(&g) {
                self.open_groups.insert(g);
            }
        }
        if let Some((old, new)) = rename {
            for r in self.clash_tests[ti].results.iter_mut() {
                if r.group == old {
                    r.group = new.clone();
                }
            }
            if self.open_groups.remove(&old) {
                self.open_groups.insert(new);
            }
        }
        match clicked {
            Some(Row::Item(i, _)) => {
                if mods.shift {
                    if let (Some(a), Some(b)) = (self.cur_result.and_then(|c| items.iter().position(|x| *x == c)), items.iter().position(|x| *x == i)) {
                        let (lo, hi) = (a.min(b), a.max(b));
                        if !mods.command {
                            self.sel_results.clear();
                        }
                        self.sel_results.extend(items[lo..=hi].iter().copied());
                    } else {
                        self.sel_results = [i].into_iter().collect();
                    }
                } else if mods.command {
                    if !self.sel_results.remove(&i) {
                        self.sel_results.insert(i);
                    }
                } else {
                    self.sel_results = [i].into_iter().collect();
                }
                let v: Vec<usize> = self.sel_results.iter().copied().collect();
                if v.is_empty() {
                    self.clash_focus = None;
                    self.colors_dirty = true;
                } else {
                    let keep = self.cur_result;
                    self.apply(Action::FocusClashes(ti, v));
                    if mods.shift {
                        self.cur_result = keep;
                    } else {
                        self.cur_result = Some(i);
                    }
                }
            }
            Some(Row::Group(_, members)) => {
                if !mods.command {
                    self.sel_results.clear();
                }
                self.sel_results.extend(members.iter().copied());
                let v: Vec<usize> = self.sel_results.iter().copied().collect();
                self.apply(Action::FocusClashes(ti, v));
            }
            None => {}
        }
    }

    // ------------------------------------------------------------------ TimeLiner
    fn attach_selection(&mut self) {
        let Some(t) = self.cur_task else {
            self.error("Select a task in TimeLiner first.");
            return;
        };
        let sel: Vec<ElemId> = self.selected.iter().copied().collect();
        if let Some(task) = self.tasks.get_mut(t) {
            let mut s: HashSet<ElemId> = task.elems.iter().copied().collect();
            s.extend(sel);
            task.elems = s.into_iter().collect();
            task.elems.sort();
            let n = task.elems.len();
            self.invalidate_sim();
            self.toast(format!("The task now has {n} attached elements."));
        }
    }

    fn timeliner_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("➕ Task").clicked() {
                let start = self.sim_range().map(|r| r.1 + 1).unwrap_or_else(timeline::today);
                let n = self.tasks.len() + 1;
                self.tasks.push(timeline::Task { name: format!("Task {n}"), start, end: start + 13, kind: TaskKind::Construct, rule: String::new(), elems: Vec::new() });
                self.cur_task = Some(self.tasks.len() - 1);
                self.invalidate_sim();
            }
            if ui.add_enabled(self.cur_task.is_some() && !self.selected.is_empty(), egui::Button::new("Attach selection")).clicked() {
                self.attach_selection();
            }
            if ui.add_enabled(self.cur_task.is_some(), egui::Button::new("Detach all")).clicked() {
                if let Some(t) = self.cur_task.and_then(|t| self.tasks.get_mut(t)) {
                    t.elems.clear();
                }
                self.invalidate_sim();
            }
            if ui.button("Auto-attach using rules").on_hover_text("Rule: “set:SetName”, “Level=Ground Floor; IFC Type=IfcWall” or free text").clicked() {
                let mut tot = 0;
                for i in 0..self.tasks.len() {
                    let e = self.rule_elems(&self.tasks[i].rule.clone());
                    if !e.is_empty() {
                        tot += e.len();
                        self.tasks[i].elems = e;
                    }
                }
                self.invalidate_sim();
                self.toast(format!("Auto-attach: {tot} attachments."));
            }
            if ui.add_enabled(self.cur_task.is_some(), egui::Button::new("🗑 Delete")).clicked() {
                if let Some(t) = self.cur_task.take() {
                    if t < self.tasks.len() {
                        self.tasks.remove(t);
                    }
                }
                self.invalidate_sim();
            }
            ui.separator();
            if ui.button("Import CSV").on_hover_text("Columns: Name;Start;End;Type;Rule — dates YYYY-MM-DD or DD.MM.YYYY").clicked() {
                self.request_path(Purpose::ImportTasksCsv);
            }
            if ui.button("Export CSV").clicked() {
                self.request_path(Purpose::ExportTasksCsv);
            }
        });
        ui.separator();
        let avail = ui.available_size();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(EV::new(avail.x * 0.52, avail.y), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_max_width(avail.x * 0.52);
                let mut clicked = None;
                let mut changed = false;
                let cur = self.cur_task;
                let tasks = &mut self.tasks;
                let mh = (ui.available_height() - 30.0).max(60.0);
                TableBuilder::new(ui)
                    .id_salt("tasks")
                    .max_scroll_height(mh)
                    .auto_shrink([false, true])
                    .striped(true)
                    .sense(Sense::click())
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::initial(140.0).clip(true))
                    .column(Column::exact(84.0))
                    .column(Column::exact(84.0))
                    .column(Column::exact(100.0))
                    .column(Column::initial(140.0).clip(true))
                    .column(Column::remainder().at_least(50.0))
                    .header(20.0, |mut h| {
                        for n in ["Name", "Start", "End", "Type", "Rule", "Items"] {
                            h.col(|ui| {
                                ui.strong(n);
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(24.0, tasks.len(), |mut row| {
                            let i = row.index();
                            row.set_selected(cur == Some(i));
                            let t = &mut tasks[i];
                            row.col(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut t.name).desired_width(f32::INFINITY));
                            });
                            row.col(|ui| {
                                changed |= date_edit(ui, egui::Id::new(("ts", i)), &mut t.start);
                            });
                            row.col(|ui| {
                                changed |= date_edit(ui, egui::Id::new(("te", i)), &mut t.end);
                            });
                            row.col(|ui| {
                                egui::ComboBox::from_id_salt(("tk", i)).width(90.0).selected_text(t.kind.label()).show_ui(ui, |ui| {
                                    for k in [TaskKind::Construct, TaskKind::Demolish, TaskKind::Temporary] {
                                        changed |= ui.selectable_value(&mut t.kind, k, k.label()).changed();
                                    }
                                });
                            });
                            row.col(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut t.rule).desired_width(f32::INFINITY).hint_text("e.g. Level=Ground Floor"));
                            });
                            row.col(|ui| {
                                ui.label(t.elems.len().to_string());
                            });
                            if row.response().clicked() {
                                clicked = Some(i);
                            }
                        });
                    });
                if changed {
                    for t in self.tasks.iter_mut() {
                        t.end = t.end.max(t.start);
                    }
                    self.invalidate_sim();
                }
                if let Some(i) = clicked {
                    self.cur_task = Some(i);
                    let e = self.tasks[i].elems.clone();
                    if !e.is_empty() {
                        self.apply(Action::Select(e, false));
                    }
                }
                if self.tasks.is_empty() {
                    ui.label(RichText::new("Add tasks or import a CSV schedule, attach elements (selection or rules) and start the simulation.").weak());
                }
            });
            ui.separator();
            ui.vertical(|ui| self.simulation_ui(ui));
        });
    }

    fn simulation_ui(&mut self, ui: &mut egui::Ui) {
        let range = self.sim_range();
        ui.horizontal_wrapped(|ui| {
            let was = self.sim_active;
            ui.checkbox(&mut self.sim_active, RichText::new("4D Simulation").strong());
            if self.sim_active && !was {
                if let Some((s, _)) = range {
                    if self.sim_day < s as f32 - 1.0 || self.sim_day > range.unwrap().1 as f32 + 1.0 {
                        self.sim_day = s as f32;
                    }
                }
                self.invalidate_sim();
            }
            ui.add_enabled_ui(self.sim_active && range.is_some(), |ui| {
                if ui.button("⏮").clicked() {
                    self.sim_day = range.unwrap().0 as f32 - 0.5;
                    self.sim_play = false;
                }
                let pl = if self.sim_play { "⏸ Pause" } else { "▶ Play" };
                if ui.button(pl).clicked() {
                    if !self.sim_play && self.sim_day >= range.unwrap().1 as f32 + 0.99 {
                        self.sim_day = range.unwrap().0 as f32 - 0.5;
                    }
                    self.sim_play = !self.sim_play;
                }
                if ui.button("⏭").clicked() {
                    self.sim_day = range.unwrap().1 as f32 + 1.0;
                    self.sim_play = false;
                }
                ui.add(egui::DragValue::new(&mut self.sim_speed).range(0.5..=365.0).speed(0.5).suffix(" days/s"));
            });
        });
        let Some((s, e)) = range else {
            ui.label(RichText::new("No tasks.").weak());
            return;
        };
        ui.add_enabled_ui(self.sim_active, |ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 120.0).max(100.0);
            ui.add(egui::Slider::new(&mut self.sim_day, (s as f32 - 1.0)..=(e as f32 + 1.0)).show_value(false));
        });
        ui.label(RichText::new(format!("Date: {}   (range {} – {})", timeline::fmt_date(self.sim_day.floor() as i32), timeline::fmt_date(s), timeline::fmt_date(e))).strong());
        // ---- Gantt
        let h = (self.tasks.len() as f32 * 18.0 + 24.0).max(60.0);
        egui::ScrollArea::vertical().id_salt("gantt").auto_shrink([false, false]).show(ui, |ui| {
            let (rect, resp) = ui.allocate_exact_size(EV::new(ui.available_width(), h), Sense::click());
            let p = ui.painter_at(rect);
            let pal = self.pal();
            p.rect_filled(rect, 4.0, pal.gantt_bg);
            let span = (e - s + 1).max(1) as f32;
            let x = |d: f32| rect.left() + 6.0 + (d - s as f32) / span * (rect.width() - 12.0);
            // grila lunara
            let (mut y0, mut m0, _) = timeline::civil_from_days(s);
            loop {
                let d = timeline::days_from_civil(y0, m0, 1);
                if d > e {
                    break;
                }
                if d >= s {
                    let xx = x(d as f32);
                    p.line_segment([Pos2::new(xx, rect.top()), Pos2::new(xx, rect.bottom())], Stroke::new(1.0_f32, pal.gantt_grid));
                    p.text(Pos2::new(xx + 2.0, rect.top() + 2.0), Align2::LEFT_TOP, format!("{y0}-{m0:02}"), FontId::proportional(10.0), pal.gantt_text.gamma_multiply(0.7));
                }
                m0 += 1;
                if m0 > 12 {
                    m0 = 1;
                    y0 += 1;
                }
            }
            for (i, t) in self.tasks.iter().enumerate() {
                let yy = rect.top() + 18.0 + i as f32 * 18.0;
                let col = match t.kind {
                    TaskKind::Construct => timeline::COL_CONSTRUCT,
                    TaskKind::Demolish => timeline::COL_DEMOLISH,
                    TaskKind::Temporary => timeline::COL_TEMP,
                };
                let r = Rect::from_min_max(Pos2::new(x(t.start as f32), yy + 2.0), Pos2::new(x(t.end as f32 + 1.0).max(x(t.start as f32) + 3.0), yy + 15.0));
                let mut c = Color32::from_rgb(col[0], col[1], col[2]);
                if self.cur_task == Some(i) {
                    p.rect_stroke(r.expand(1.5), 3.0, Stroke::new(1.5_f32, pal.gantt_text));
                } else {
                    c = c.gamma_multiply(0.8);
                }
                p.rect_filled(r, 2.0, c);
                let g = p.layout_no_wrap(t.name.clone(), FontId::proportional(11.0), Color32::WHITE);
                let tp = if g.size().x + 6.0 < r.width() { Pos2::new(r.left() + 3.0, r.center().y - g.size().y / 2.0) } else { Pos2::new(r.right() + 4.0, r.center().y - g.size().y / 2.0) };
                let fg = if g.size().x + 6.0 < r.width() { Color32::from_rgb(15, 20, 25) } else { pal.gantt_text };
                p.galley_with_override_text_color(tp, g, fg);
            }
            let cx = x(self.sim_day);
            p.line_segment([Pos2::new(cx, rect.top()), Pos2::new(cx, rect.bottom())], Stroke::new(2.0_f32, Color32::from_rgb(255, 70, 70)));
            if resp.clicked() {
                if let Some(pp) = resp.interact_pointer_pos() {
                    let i = ((pp.y - rect.top() - 18.0) / 18.0).floor();
                    if i >= 0.0 && (i as usize) < self.tasks.len() {
                        self.cur_task = Some(i as usize);
                    }
                    if self.sim_view() {
                        self.sim_day = s as f32 + (pp.x - rect.left() - 6.0) / (rect.width() - 12.0) * span;
                    }
                }
            }
        });
    }

    // ------------------------------------------------------------------ viewpoints
    fn viewpoints_ui(&mut self, ui: &mut egui::Ui) {
        use crate::markup::{PRIORITIES, STATUSES};
        ui.horizontal_wrapped(|ui| {
            if ui.button("📷 Save Current View").clicked() {
                self.save_viewpoint();
            }
            ui.separator();
            ui.label("Export:");
            let can = !self.viewpoints.is_empty();
            if ui.add_enabled(can, egui::Button::new("📄 HTML report")).clicked() {
                self.request_path(Purpose::ExportVpHtml);
            }
            if ui.add_enabled(can, egui::Button::new("📊 Excel")).clicked() {
                self.request_path(Purpose::ExportVpXlsx);
            }
            if ui.add_enabled(can, egui::Button::new("📤 BCF 2.1")).clicked() {
                self.request_path(Purpose::ExportBcf);
            }
            if ui.add_enabled(can, egui::Button::new("🧭 Navisworks XML")).on_hover_text("Saved Viewpoints > right-click > Import Viewpoints in Navisworks").clicked() {
                self.request_path(Purpose::ExportNavisXml);
            }
            if ui.button("📥 Import BCF…").on_hover_text("BCF 2.1 / 3.0 issues from Revit, ACC, Solibri, BIMcollab...: camera, status, comments, selected elements").clicked() {
                self.request_path(Purpose::ImportBcf);
            }
            if ui.button("📥 Import Navisworks XML…").on_hover_text("Viewpoints exported from Navisworks: camera, section box, comments, redlines").clicked() {
                self.request_path(Purpose::ImportNavisXml);
            }
            ui.label(RichText::new("Redline tools: ribbon Review. Markups belong to the viewpoint and show while you stay at it.").weak().small());
        });
        ui.separator();
        if self.viewpoints.is_empty() {
            ui.label(RichText::new("No saved viewpoints. Save one, or just draw a redline (Review tab) - a viewpoint is created automatically.").weak());
            return;
        }
        if self.vp_sel >= self.viewpoints.len() {
            self.vp_sel = self.viewpoints.len() - 1;
        }
        let mut restore = None;
        let mut del = None;
        let mut update = false;
        let w = ui.available_width();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(EV::new((w * 0.36).max(220.0), ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
                egui::ScrollArea::vertical().id_salt("vp_list").auto_shrink([false, false]).show(ui, |ui| {
                    for (i, v) in self.viewpoints.iter().enumerate() {
                        let col = match v.issue.status.as_str() {
                            "Open" => Color32::from_rgb(239, 68, 68),
                            "In Progress" => Color32::from_rgb(245, 158, 11),
                            "Resolved" => Color32::from_rgb(34, 197, 94),
                            _ => Color32::GRAY,
                        };
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(EV::splat(10.0), Sense::hover());
                            ui.painter().circle_filled(r.center(), 4.5, col);
                            let mut t = format!("{}. {}", i + 1, v.name);
                            if !v.redlines.is_empty() {
                                t.push_str(&format!("  ✏{}", v.redlines.len()));
                            }
                            if !v.comments.is_empty() {
                                t.push_str(&format!("  💬{}", v.comments.len()));
                            }
                            let r = ui.selectable_label(self.vp_sel == i, t);
                            if r.clicked() {
                                self.vp_sel = i;
                                restore = Some(i);
                            }
                            if r.double_clicked() {
                                restore = Some(i);
                            }
                        });
                    }
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                let i = self.vp_sel;
                let who = crate::markup::user_name();
                let v = &mut self.viewpoints[i];
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut v.name).desired_width(240.0).font(egui::TextStyle::Heading));
                    if ui.button("▶ Go to").clicked() {
                        restore = Some(i);
                    }
                    if ui.button("⟳ Update view").on_hover_text("Store the current camera, section, hidden items and selection").clicked() {
                        update = true;
                    }
                    if ui.add_enabled(!v.redlines.is_empty(), egui::Button::new("Clear redlines")).clicked() {
                        v.redlines.clear();
                    }
                    if ui.button("🗑").on_hover_text("Delete viewpoint").clicked() {
                        del = Some(i);
                    }
                });
                egui::Grid::new("vp_issue").num_columns(4).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Status");
                    egui::ComboBox::from_id_salt("vp_status").selected_text(v.issue.status.clone()).show_ui(ui, |ui| {
                        for st in STATUSES {
                            ui.selectable_value(&mut v.issue.status, st.to_string(), st);
                        }
                    });
                    ui.label("Priority");
                    egui::ComboBox::from_id_salt("vp_prio").selected_text(v.issue.priority.clone()).show_ui(ui, |ui| {
                        for p in PRIORITIES {
                            ui.selectable_value(&mut v.issue.priority, p.to_string(), p);
                        }
                    });
                    ui.end_row();
                    ui.label("Assigned to");
                    ui.add(egui::TextEdit::singleline(&mut v.issue.assigned).desired_width(160.0));
                    ui.label("Author");
                    ui.label(format!("{} · {}", v.issue.author, crate::markup::pretty_date(&v.issue.created)));
                    ui.end_row();
                });
                ui.label("Description");
                ui.add(egui::TextEdit::multiline(&mut v.comment).desired_rows(2).desired_width(f32::INFINITY));
                ui.label(RichText::new(format!("Comments ({})", v.comments.len())).strong());
                egui::ScrollArea::vertical().id_salt("vp_comments").max_height(110.0).show(ui, |ui| {
                    let mut rm = None;
                    for (k, c) in v.comments.iter().enumerate() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&c.author).strong());
                            ui.label(RichText::new(crate::markup::pretty_date(&c.date)).weak().small());
                            ui.label(&c.text);
                            if ui.small_button("✖").clicked() {
                                rm = Some(k);
                            }
                        });
                    }
                    if let Some(k) = rm {
                        v.comments.remove(k);
                    }
                });
                let id = ui.id().with("new_comment");
                let mut txt: String = ui.data_mut(|d| d.get_temp(id).unwrap_or_default());
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut txt).hint_text("add a comment…").desired_width(320.0));
                    let send = ui.button("Add").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
                    if send && !txt.trim().is_empty() {
                        v.comments.push(crate::markup::VpComment { author: who.clone(), date: crate::markup::now_iso(), text: txt.trim().to_string() });
                        txt.clear();
                    }
                });
                ui.data_mut(|d| d.insert_temp(id, txt));
            });
        });
        if let Some(i) = restore {
            self.restore_viewpoint(i);
        }
        if update {
            let i = self.vp_sel;
            let mut hidden: Vec<ElemId> = self.hidden.iter().copied().collect();
            hidden.sort();
            let sel: Vec<ElemId> = self.selected.iter().copied().collect();
            let v = &mut self.viewpoints[i];
            v.cam = self.cam;
            v.section = self.section.clone();
            v.hidden = hidden;
            v.selected = sel;
            self.active_vp = Some(i);
        }
        if let Some(i) = del {
            self.viewpoints.remove(i);
            self.active_vp = None;
            self.vp_sel = i.saturating_sub(1);
        }
    }

    // ------------------------------------------------------------------ sectiune
    fn init_section_defaults(&mut self) {
        let bb = self.scene_aabb();
        if !bb.is_valid() {
            return;
        }
        if self.section.bmin == [-1.0; 3] && self.section.bmax == [1.0; 3] {
            self.section.bmin = bb.min.to_array();
            self.section.bmax = bb.max.to_array();
            for p in &mut self.section.planes {
                p.offset = if p.dir < 2 { bb.center().z } else if p.dir < 4 { bb.center().x } else { bb.center().y };
            }
        }
    }

    fn section_to_selection(&mut self) {
        if let Some(bb) = self.section_box_on_selection() {
            let mut t = self.cam;
            t.fit(&bb);
            self.fly_to(t);
        }
    }

    /// Section box in jurul selectiei (cu o margine mica); intoarce cutia elementelor.
    fn section_box_on_selection(&mut self) -> Option<crate::model::Aabb> {
        let s: Vec<ElemId> = self.selected.iter().copied().collect();
        let bb = self.ids_aabb(&s);
        if !bb.is_valid() {
            return None;
        }
        // margine: 2% din marimea selectiei, minim 5 cm
        let pad = Vec3::splat((bb.size().length() * 0.02).max(0.05));
        self.section.enabled = true;
        self.section.mode = SectionMode::Box;
        self.section.bmin = (bb.min - pad).to_array();
        self.section.bmax = (bb.max + pad).to_array();
        Some(bb)
    }

    /// "Follow": cutia urmareste selectia (doar cand selectia se schimba; camera ramane pe loc).
    fn section_follow_tick(&mut self) {
        if !self.section_follow || self.selected.is_empty() {
            return;
        }
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for id in &self.selected {
            id.hash(&mut h);
        }
        let sig = h.finish() | 1;
        if sig != self.section_sel_sig {
            self.section_sel_sig = sig;
            self.section_box_on_selection();
        }
    }

    fn section_ui(&mut self, ui: &mut egui::Ui) {
        let bb = self.scene_aabb();
        let (lo, hi) = if bb.is_valid() { (bb.min - Vec3::splat(0.5), bb.max + Vec3::splat(0.5)) } else { (Vec3::splat(-10.0), Vec3::splat(10.0)) };
        ui.horizontal(|ui| {
            if ui.checkbox(&mut self.section.enabled, RichText::new("Sectioning enabled").strong()).changed() && self.section.enabled {
                self.init_section_defaults();
            }
            ui.radio_value(&mut self.section.mode, SectionMode::Planes, "Planes");
            ui.radio_value(&mut self.section.mode, SectionMode::Box, "Box");
            ui.separator();
            if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("Box around selection")).clicked() {
                self.section_to_selection();
            }
            if ui.button("Reset").clicked() {
                self.section = Section::default();
                self.section.enabled = true;
                self.init_section_defaults();
            }
            ui.separator();
            ui.checkbox(&mut self.section_caps, "Filled cuts").on_hover_text("Cut faces drawn solid and hatched (closed solids), instead of seeing inside the elements");
        });
        ui.separator();
        ui.spacing_mut().slider_width = (ui.available_width() - 330.0).clamp(150.0, 700.0);
        match self.section.mode {
            SectionMode::Planes => {
                let mut del = None;
                for (i, p) in self.section.planes.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut p.enabled, format!("Plane {}", i + 1));
                        egui::ComboBox::from_id_salt(("pd", i)).width(150.0).selected_text(PLANE_DIRS[p.dir as usize]).show_ui(ui, |ui| {
                            for (k, n) in PLANE_DIRS.iter().enumerate() {
                                ui.selectable_value(&mut p.dir, k as u8, *n);
                            }
                        });
                        let ax = if p.dir < 2 { 2 } else if p.dir < 4 { 0 } else { 1 };
                        ui.add(egui::Slider::new(&mut p.offset, lo[ax]..=hi[ax]).suffix(" m").fixed_decimals(2));
                        if ui.small_button("🗑").clicked() {
                            del = Some(i);
                        }
                    });
                }
                if let Some(i) = del {
                    self.section.planes.remove(i);
                }
                if self.section.planes.len() < 6 && ui.button("➕ Plane").clicked() {
                    self.section.planes.push(PlaneDef { enabled: true, dir: 2, offset: bb.center().x });
                }
            }
            SectionMode::Box => {
                for (ax, n) in ["X", "Y", "Z"].iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{n} min"));
                        ui.add(egui::Slider::new(&mut self.section.bmin[ax], lo[ax]..=hi[ax]).fixed_decimals(2));
                    });
                    ui.horizontal(|ui| {
                        ui.label(format!("{n} max"));
                        ui.add(egui::Slider::new(&mut self.section.bmax[ax], lo[ax]..=hi[ax]).fixed_decimals(2));
                    });
                    if self.section.bmax[ax] < self.section.bmin[ax] {
                        self.section.bmax[ax] = self.section.bmin[ax];
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------ masuratori
    fn measure_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            let mut t = self.tool;
            ui.selectable_value(&mut t, Tool::Select, "Select");
            for k in [MeasureKind::PointToPoint, MeasureKind::Polyline, MeasureKind::Angle, MeasureKind::Area] {
                ui.selectable_value(&mut t, Tool::Measure(k), k.label());
            }
            if t != self.tool {
                self.cur_measure.clear();
                self.tool = t;
            }
            ui.separator();
            if ui.add_enabled(!self.cur_measure.is_empty(), egui::Button::new("Finish")).clicked() {
                self.finish_measure();
            }
            if ui.button("Clear all").clicked() {
                self.measures.clear();
                self.cur_measure.clear();
                self.min_dist_result = None;
            }
            if ui.add_enabled(self.selected.len() == 2, egui::Button::new("Shortest distance between 2 selected elements")).clicked() {
                self.min_distance_selected();
            }
        });
        ui.label(RichText::new("Points snap to nearby vertices. Units: metres.").weak());
        ui.separator();
        let mut del = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (i, m) in self.measures.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{}.", i + 1)).weak());
                    ui.label(m.kind.label());
                    ui.strong(m.value());
                    if ui.small_button("Copy").clicked() {
                        ui.ctx().copy_text(m.value());
                    }
                    if ui.small_button("🗑").clicked() {
                        del = Some(i);
                    }
                });
            }
        });
        if let Some(i) = del {
            self.measures.remove(i);
        }
    }

    // ------------------------------------------------------------------ modele
    // ------------------------------------------------------------------ project (NWF-style)
    fn fmt_time(t: std::time::SystemTime) -> String {
        let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
        let (y, m, d) = timeline::civil_from_days((secs / 86400) as i32);
        let s = secs % 86400;
        format!("{y}-{m:02}-{d:02} {:02}:{:02} UTC", s / 3600, (s % 3600) / 60)
    }

    fn fmt_xyz(v: glam::DVec3) -> String {
        format!("{:.3}, {:.3}, {:.3}", v.x, v.y, v.z)
    }

    fn open_folder(path: &str) {
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("explorer.exe").arg(format!("/select,{}", path)).spawn();
        }
        #[cfg(not(windows))]
        {
            let _ = path;
        }
    }

    fn models_ui(&mut self, ui: &mut egui::Ui) {
        let mut remove = None;
        let mut reload = None;
        let mut export = None;
        let mut coords = None;
        ui.columns(3, |cols| {
            // ---- project + models
            let ui = &mut cols[0];
            let pname = self.project_path.as_ref().map(|p| crate::file_name(p)).unwrap_or_else(|| "(unsaved project)".into());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Project").strong());
                ui.label(RichText::new(&pname).color(accent()));
            });
            if let Some(p) = &self.project_path {
                ui.label(RichText::new(p).weak().small());
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button("➕ Append").on_hover_text("Add a model to the project (like Navisworks Append)").clicked() {
                    self.request_path(Purpose::OpenModel { append: true });
                }
                if ui.button("💾 Save").clicked() {
                    match self.project_path.clone() {
                        Some(p) => self.save_project(&p),
                        None => self.request_path(Purpose::SaveProject),
                    }
                }
                if ui.button("Save As…").clicked() {
                    self.request_path(Purpose::SaveProject);
                }
                if ui.button("Open…").clicked() {
                    self.request_path(Purpose::OpenProject);
                }
            });
            ui.label(RichText::new("A .bimr project references the model files (like a Navisworks NWF) and stores clash tests, viewpoints, sets, TimeLiner, transforms and animations.").weak().small());
            ui.separator();
            let rows: Vec<(usize, String, usize, bool)> = self
                .loaded_models()
                .map(|(i, m)| {
                    let moved = !self.model_xf.get(i).map_or(true, |x| x.is_identity()) || self.elem_offsets.keys().any(|k| k.m as usize == i);
                    (i, m.name.clone(), m.elements.len(), moved)
                })
                .collect();
            egui::ScrollArea::vertical().id_salt("projmods").auto_shrink([false, false]).show(ui, |ui| {
                for (i, name, n, moved) in &rows {
                    let sel = self.proj_sel == Some(*i);
                    let mut t = format!("📂 {name}   ({n})");
                    if *moved {
                        t.push_str("   📍 moved");
                    }
                    if ui.selectable_label(sel, t).clicked() {
                        self.proj_sel = Some(*i);
                    }
                }
                for j in &self.loads {
                    ui.label(RichText::new(format!("⏳ {} - {:.0}%", crate::file_name(&j.path), j.progress * 100.0)).weak());
                }
                if rows.is_empty() && self.loads.is_empty() {
                    ui.label(RichText::new("No models. Use ➕ Append.").weak());
                }
            });

            // ---- model properties
            let ui = &mut cols[1];
            ui.label(RichText::new("Model Properties").strong());
            let sel = self.proj_sel.filter(|i| self.models.get(*i).map_or(false, |m| m.is_some())).or(rows.first().map(|r| r.0));
            if let Some(i) = sel {
                let m = self.models[i].clone().unwrap();
                let meta = std::fs::metadata(&m.path).ok();
                let cache = crate::bimc::cache_path(&m.path);
                let cache_state = if m.path.to_lowercase().ends_with(".bimc") {
                    "this is a cache file".to_string()
                } else if crate::bimc::fresh_cache(&m.path).is_some() {
                    format!("up to date ({})", crate::file_name(&cache))
                } else if std::path::Path::new(&cache).exists() {
                    "outdated (re-created on next open)".into()
                } else {
                    "none".into()
                };
                let o = self.origin_f64();
                let bmin = self.world(m.aabb.min);
                let bmax = self.world(m.aabb.max);
                let levels: HashSet<&str> = m.elements.iter().map(|e| e.storey.as_str()).collect();
                let types: HashSet<&str> = m.elements.iter().map(|e| e.ifc_type.as_str()).collect();
                let xf = self.model_xf.get(i).copied().unwrap_or_default();
                let moved_items = self.elem_offsets.keys().filter(|k| k.m as usize == i).count();
                let mut rows: Vec<(&str, Vec<(String, String)>)> = vec![
                    (
                        "File",
                        vec![
                            ("Name".into(), m.name.clone()),
                            ("Location".into(), std::path::Path::new(&m.path).parent().map(|p| p.display().to_string()).unwrap_or_default()),
                            ("Full path".into(), m.path.clone()),
                            ("Size".into(), meta.as_ref().map(|x| format!("{:.2} MB", x.len() as f64 / 1e6)).unwrap_or_else(|| "file not found".into())),
                            ("Modified".into(), meta.as_ref().and_then(|x| x.modified().ok()).map(Self::fmt_time).unwrap_or_default()),
                            ("BIMC cache".into(), cache_state),
                        ],
                    ),
                    ("Model", {
                        // cheile "~..." sunt date interne (axe, niveluri); arata doar numarul lor
                        let mut v: Vec<(String, String)> = m.info.iter().filter(|(k, _)| !k.starts_with('~')).cloned().collect();
                        let ga = self.grid_data(i);
                        if !ga.grids.is_empty() {
                            v.push(("Grid axes".into(), ga.grids.iter().map(|g| g.0.clone()).collect::<Vec<_>>().join(", ")));
                        }
                        if !ga.levels.is_empty() {
                            v.push(("Levels".into(), ga.levels.iter().map(|l| format!("{} ({:+.3} m)", l.0, l.1)).collect::<Vec<_>>().join(", ")));
                        }
                        v
                    }),
                    (
                        "Contents",
                        vec![
                            ("Elements".into(), m.elements.len().to_string()),
                            ("Triangles".into(), m.tri_total().to_string()),
                            ("Levels".into(), levels.len().to_string()),
                            ("Types".into(), types.len().to_string()),
                        ],
                    ),
                    (
                        "Location (world coordinates, m)",
                        vec![
                            ("Min X, Y, Z".into(), Self::fmt_xyz(bmin)),
                            ("Max X, Y, Z".into(), Self::fmt_xyz(bmax)),
                            ("Centre".into(), Self::fmt_xyz((bmin + bmax) * 0.5)),
                            ("Size X × Y × Z".into(), format!("{:.3} × {:.3} × {:.3}", bmax.x - bmin.x, bmax.y - bmin.y, bmax.z - bmin.z)),
                            ("Scene origin offset".into(), Self::fmt_xyz(o)),
                        ],
                    ),
                    (
                        "Transform",
                        vec![
                            ("Translation".into(), format!("{:.3}, {:.3}, {:.3}", xf.translate[0], xf.translate[1], xf.translate[2])),
                            ("Rotation Z".into(), format!("{:.3}°", xf.rot_z)),
                            ("Scale".into(), format!("{}", xf.scale)),
                            ("Pivot".into(), if xf.pivot_origin { "world origin".into() } else { "model centre".into() }),
                            ("Moved items".into(), moved_items.to_string()),
                        ],
                    ),
                ];
                if !m.warnings.is_empty() {
                    rows.push(("Warnings", m.warnings.iter().map(|w| ("⚠".to_string(), w.clone())).collect()));
                }
                egui::ScrollArea::vertical().id_salt("mprops").auto_shrink([false, false]).max_height(ui.available_height() - 34.0).show(ui, |ui| {
                    for (gi, (g, r)) in rows.iter().enumerate() {
                        egui::CollapsingHeader::new(RichText::new(*g).strong()).id_salt(("mp", gi)).default_open(true).show(ui, |ui| {
                            egui::Grid::new(("mpg", gi)).num_columns(2).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                                for (k, v) in r {
                                    ui.label(RichText::new(k).weak());
                                    ui.add(egui::Label::new(v).wrap());
                                    ui.end_row();
                                }
                            });
                        });
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if cfg!(windows) && ui.small_button("📁 Show in folder").clicked() {
                        Self::open_folder(&m.path);
                    }
                    if ui.small_button("📐 Edit coordinates").clicked() {
                        coords = Some(i);
                    }
                    if ui.small_button("📦 Export .bimc…").clicked() {
                        export = Some(i);
                    }
                    if ui.small_button("Reload").clicked() {
                        reload = Some(i);
                    }
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            } else {
                ui.label(RichText::new("Select a model.").weak());
            }

            // ---- log
            let ui = &mut cols[2];
            ui.strong("Log");
            egui::ScrollArea::vertical().id_salt("log").auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
                for l in &self.log {
                    ui.label(RichText::new(l).small().monospace());
                }
            });
        });
        if let Some(i) = coords {
            self.proj_sel = Some(i);
            self.open_panel(Panel::Coords);
        }
        if let Some(i) = export {
            self.request_path(Purpose::ExportCache(i));
        }
        if let Some(i) = remove {
            self.remove_model(i);
            self.refresh_caches();
        }
        if let Some(i) = reload {
            if let Some(m) = self.models[i].clone() {
                self.reload_model(i, m.path.clone());
            }
        }
    }

    // ------------------------------------------------------------------ coordinates
    fn coords_ui(&mut self, ui: &mut egui::Ui) {
        let models: Vec<(usize, String)> = self.loaded_models().map(|(i, m)| (i, m.name.clone())).collect();
        if models.is_empty() {
            ui.label(RichText::new("No models loaded.").weak());
            return;
        }
        let slot = self.proj_sel.filter(|i| models.iter().any(|m| m.0 == *i)).unwrap_or(models[0].0);
        self.proj_sel = Some(slot);
        ui.columns(2, |cols| {
            // ---- model transform
            let ui = &mut cols[0];
            ui.horizontal(|ui| {
                ui.label(RichText::new("Model transform").strong());
                let cur = models.iter().find(|m| m.0 == slot).map(|m| m.1.clone()).unwrap_or_default();
                egui::ComboBox::from_id_salt("xfmodel").width(260.0).selected_text(cur).show_ui(ui, |ui| {
                    for (i, n) in &models {
                        if ui.selectable_label(*i == slot, n).clicked() {
                            self.proj_sel = Some(*i);
                        }
                    }
                });
            });
            if let Some(m) = self.models.get(slot).cloned().flatten() {
                let (a, b) = (self.world(m.aabb.min), self.world(m.aabb.max));
                ui.label(RichText::new(format!("Current extents: min {}   max {}", Self::fmt_xyz(a), Self::fmt_xyz(b))).weak().small());
            }
            let applied = self.model_xf.get(slot).copied().unwrap_or_default();
            let mut d = self.xf_draft.get(&slot).copied().unwrap_or(applied);
            egui::Grid::new("xfgrid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("Translation X / Y / Z");
                ui.horizontal(|ui| {
                    for k in 0..3 {
                        ui.add(egui::DragValue::new(&mut d.translate[k]).speed(0.01).fixed_decimals(3).suffix(" m"));
                    }
                });
                ui.end_row();
                ui.label("Rotation about Z");
                ui.add(egui::DragValue::new(&mut d.rot_z).speed(0.1).fixed_decimals(3).suffix(" °"));
                ui.end_row();
                ui.label("Scale");
                ui.add(egui::DragValue::new(&mut d.scale).speed(0.001).range(0.001..=1000.0).fixed_decimals(4));
                ui.end_row();
                ui.label("Rotate / scale about");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut d.pivot_origin, false, "model centre");
                    ui.radio_value(&mut d.pivot_origin, true, "world origin");
                });
                ui.end_row();
            });
            if d != applied {
                self.xf_draft.insert(slot, d);
            } else {
                self.xf_draft.remove(&slot);
            }
            ui.horizontal_wrapped(|ui| {
                let changed = d != applied;
                if ui.add_enabled(changed, egui::Button::new(RichText::new("✔ Apply").strong())).clicked() {
                    self.xf_draft.remove(&slot);
                    self.set_model_xf(slot, d);
                    self.toast("Model transform applied. Re-run clash tests to update the results.");
                }
                if ui.add_enabled(changed, egui::Button::new("Discard")).clicked() {
                    self.xf_draft.remove(&slot);
                }
                if ui.add_enabled(!applied.is_identity(), egui::Button::new("↺ Reset model")).on_hover_text("Back to the original coordinates from the file").clicked() {
                    self.xf_draft.remove(&slot);
                    self.set_model_xf(slot, ModelXf::default());
                }
                if ui.button("↺ Reset everything").on_hover_text("All models and all moved items back to their original coordinates").clicked() {
                    self.reset_all_transforms();
                    self.toast("All coordinates reset to the original files.");
                }
            });
            ui.label(RichText::new("The model files are never modified - transforms are stored in the .bimr project.").weak().small());

            // ---- selected items
            let ui = &mut cols[1];
            ui.label(RichText::new("Selected items").strong());
            let sel: Vec<ElemId> = self.selected.iter().copied().collect();
            let moved_total = self.elem_offsets.len();
            if sel.is_empty() {
                ui.label(RichText::new("Select items in the 3D view, tree or with Find Items to move them.").weak());
            } else {
                let bb = self.ids_aabb(&sel);
                let c = self.world(bb.center());
                ui.label(format!("{} items · centre {} · bottom Z {:.3}", sel.len(), Self::fmt_xyz(c), self.world(bb.min).z));
                let moved_sel: Vec<ElemId> = sel.iter().copied().filter(|i| self.elem_offsets.contains_key(i)).collect();
                if let Some(o) = moved_sel.first().and_then(|i| self.elem_offsets.get(i)) {
                    ui.label(RichText::new(format!("Moved by {:.3}, {:.3}, {:.3} m from the original position", o[0], o[1], o[2])).color(accent()));
                }
                egui::Grid::new("movegrid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Move by ΔX / ΔY / ΔZ");
                    ui.horizontal(|ui| {
                        for k in 0..3 {
                            ui.add(egui::DragValue::new(&mut self.move_delta[k]).speed(0.01).fixed_decimals(3).suffix(" m"));
                        }
                        if ui.button("Move").clicked() {
                            let d = Vec3::new(self.move_delta[0] as f32, self.move_delta[1] as f32, self.move_delta[2] as f32);
                            self.move_selected(d);
                        }
                    });
                    ui.end_row();
                    ui.label("Move centre to X / Y / Z");
                    ui.horizontal(|ui| {
                        let mut t = self.move_target.unwrap_or([c.x, c.y, c.z]);
                        for k in 0..3 {
                            ui.add(egui::DragValue::new(&mut t[k]).speed(0.01).fixed_decimals(3));
                        }
                        self.move_target = Some(t);
                        if ui.button("Move").clicked() {
                            let d = Vec3::new((t[0] - c.x) as f32, (t[1] - c.y) as f32, (t[2] - c.z) as f32);
                            self.move_selected(d);
                            self.move_target = None;
                        }
                        if ui.small_button("⟳").on_hover_text("Take the current centre").clicked() {
                            self.move_target = None;
                        }
                    });
                    ui.end_row();
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(!moved_sel.is_empty(), egui::Button::new(format!("↺ Reset selected ({})", moved_sel.len()))).clicked() {
                        self.reset_items(&moved_sel);
                    }
                });
            }
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Moved items in the project: {moved_total}"));
                if ui.add_enabled(moved_total > 0, egui::Button::new("Select moved")).clicked() {
                    let v: Vec<ElemId> = self.elem_offsets.keys().copied().collect();
                    self.apply(Action::Select(v, false));
                }
                if ui.add_enabled(moved_total > 0, egui::Button::new("↺ Reset all items")).clicked() {
                    let v: Vec<ElemId> = self.elem_offsets.keys().copied().collect();
                    self.reset_items(&v);
                }
            });
        });
    }

    // ------------------------------------------------------------------ materials
    fn swatch(ui: &mut egui::Ui, m: &crate::materials::Material) {
        let (r, _) = ui.allocate_exact_size(EV::new(22.0, 16.0), Sense::hover());
        let c = Color32::from_rgba_unmultiplied(m.color[0], m.color[1], m.color[2], m.rgba()[3].max(80));
        ui.painter().rect_filled(r, 3.0, c);
        if !m.texture.is_empty() {
            ui.painter().text(r.center(), Align2::CENTER_CENTER, "🖼", FontId::proportional(11.0), Color32::BLACK);
        } else if m.pattern != crate::materials::Pattern::None {
            let s = Stroke::new(1.0_f32, Color32::from_black_alpha(70));
            for k in 1..3 {
                let y = r.top() + r.height() * k as f32 / 3.0;
                ui.painter().line_segment([Pos2::new(r.left(), y), Pos2::new(r.right(), y)], s);
            }
        }
        if m.metal > 40 || m.gloss > 60 {
            ui.painter().circle_filled(r.left_top() + EV::new(6.0, 5.0), 2.5, Color32::from_white_alpha(200));
        }
        ui.painter().rect_stroke(r, 3.0, Stroke::new(1.0_f32, Color32::from_gray(120)));
    }

    fn materials_ui(&mut self, ui: &mut egui::Ui) {
        if self.mat_sel >= self.materials.len() {
            self.mat_sel = 0;
        }
        let mut changed = false;
        let mut pick_tex = false;
        let mut lib_io: Option<Purpose> = None;
        ui.columns(2, |cols| {
            // ---- library + editor
            let ui = &mut cols[0];
            ui.horizontal(|ui| {
                ui.label(RichText::new("Material library").strong());
                if ui.small_button("➕ New").clicked() {
                    let mut m = self.materials.get(self.mat_sel).cloned().unwrap_or_else(|| crate::materials::library()[0].clone());
                    m.name = format!("Material {}", self.materials.len() + 1);
                    self.materials.push(m);
                    self.mat_sel = self.materials.len() - 1;
                }
                if ui.small_button("Duplicate").clicked() {
                    if let Some(m) = self.materials.get(self.mat_sel).cloned() {
                        let mut m = m;
                        m.name.push_str(" copy");
                        self.materials.push(m);
                        self.mat_sel = self.materials.len() - 1;
                    }
                }
                if ui.add_enabled(self.materials.len() > 1, egui::Button::new("🗑").small()).clicked() {
                    let n = self.materials.remove(self.mat_sel).name;
                    self.mat_assign.retain(|a| a.material != n);
                    self.mat_sel = 0;
                    changed = true;
                }
                if ui.small_button("📥 Images…").on_hover_text("Import PNG / JPG images as new texture materials").clicked() {
                    lib_io = Some(Purpose::ImportTextures);
                }
                ui.menu_button("Library ⏷", |ui| {
                    if ui.button("📤 Export library (.bimmat)…").clicked() {
                        lib_io = Some(Purpose::ExportMaterials);
                        ui.close_menu();
                    }
                    if ui.button("📥 Import library (.bimmat)…").clicked() {
                        lib_io = Some(Purpose::ImportMaterials);
                        ui.close_menu();
                    }
                });
                if ui.small_button("Reset library").on_hover_text("Restore the built-in materials (keeps your own)").clicked() {
                    for m in crate::materials::library() {
                        if let Some(x) = self.materials.iter_mut().find(|x| x.name == m.name) {
                            *x = m;
                        } else {
                            self.materials.push(m);
                        }
                    }
                    changed = true;
                }
            });
            ui.horizontal_top(|ui| {
                egui::ScrollArea::vertical().id_salt("matlib").max_height(ui.available_height()).max_width(200.0).show(ui, |ui| {
                    ui.vertical(|ui| {
                    ui.set_width(190.0);
                    for i in 0..self.materials.len() {
                        ui.horizontal(|ui| {
                            Self::swatch(ui, &self.materials[i]);
                            let used = self.mat_assign.iter().any(|a| a.material == self.materials[i].name);
                            let mut t = RichText::new(&self.materials[i].name);
                            if used {
                                t = t.strong();
                            }
                            if ui.selectable_label(i == self.mat_sel, t).clicked() {
                                self.mat_sel = i;
                            }
                        });
                    }
                    });
                });
                ui.separator();
                ui.vertical(|ui| {
                    let i = self.mat_sel;
                    if i < self.materials.len() {
                        let old_name = self.materials[i].name.clone();
                        let m = &mut self.materials[i];
                        egui::Grid::new("mated").num_columns(2).spacing([8.0, 5.0]).show(ui, |ui| {
                            ui.label("Name");
                            ui.add(egui::TextEdit::singleline(&mut m.name).desired_width(150.0));
                            ui.end_row();
                            ui.label("Colour");
                            changed |= egui::color_picker::color_edit_button_srgb(ui, &mut m.color).changed();
                            ui.end_row();
                            ui.label("Transparency");
                            changed |= ui.add(egui::Slider::new(&mut m.transparency, 0..=90).suffix(" %")).changed();
                            ui.end_row();
                            ui.label("Gloss");
                            changed |= ui.add(egui::Slider::new(&mut m.gloss, 0..=100)).changed();
                            ui.end_row();
                            ui.label("Metallic");
                            changed |= ui.add(egui::Slider::new(&mut m.metal, 0..=100)).changed();
                            ui.end_row();
                            ui.label("Texture");
                            egui::ComboBox::from_id_salt("pat").selected_text(m.pattern.label()).show_ui(ui, |ui| {
                                for p in crate::materials::Pattern::ALL {
                                    changed |= ui.selectable_value(&mut m.pattern, p, p.label()).changed();
                                }
                            });
                            ui.end_row();
                            ui.label("Texture size");
                            changed |= ui.add(egui::DragValue::new(&mut m.scale).range(0.05..=4.0).speed(0.01).suffix(" m")).changed();
                            ui.end_row();
                            ui.label("Image");
                            ui.horizontal(|ui| {
                                if m.texture.is_empty() {
                                    ui.label(RichText::new("(procedural)").weak());
                                } else {
                                    ui.label(crate::file_name(&m.texture)).on_hover_text(&m.texture);
                                    if ui.small_button("✖").on_hover_text("Remove the image").clicked() {
                                        m.texture.clear();
                                        changed = true;
                                    }
                                }
                                if ui.small_button("🖼 Choose…").on_hover_text("PNG / JPG texture, repeated every \"Texture size\" metres").clicked() {
                                    pick_tex = true;
                                }
                            });
                            ui.end_row();
                        });
                        if m.name != old_name {
                            let new = m.name.clone();
                            for a in self.mat_assign.iter_mut().filter(|a| a.material == old_name) {
                                a.material = new.clone();
                            }
                            changed = true;
                        }
                    }
                });
            });

            // ---- assignments
            let ui = &mut cols[1];
            let mname = self.materials.get(self.mat_sel).map(|m| m.name.clone()).unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Assignments").strong());
                changed |= ui.checkbox(&mut self.materials_on, "Show materials in the 3D view").changed();
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("Assign “{mname}” to:")).color(accent()));
                if ui.add_enabled(!self.selected.is_empty(), egui::Button::new(format!("Selection ({})", self.selected.len()))).clicked() {
                    let v: Vec<ElemId> = self.selected.iter().copied().collect();
                    self.assign_material(MatTarget::Items(v), &mname);
                }
                ui.menu_button("Set ⏷", |ui| {
                    if self.sets.is_empty() {
                        ui.label(RichText::new("No sets - create them in Find Items / Properties").weak());
                    }
                    for s in self.sets.clone() {
                        if ui.button(&s.name).clicked() {
                            self.assign_material(MatTarget::Set(s.name.clone()), &mname);
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button("IFC type ⏷", |ui| {
                    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for t in self.cache_types.clone() {
                            if ui.button(&t).clicked() {
                                self.assign_material(MatTarget::Type(t.clone()), &mname);
                                ui.close_menu();
                            }
                        }
                    });
                });
            });
            ui.horizontal_wrapped(|ui| {
                if ui.button("⚡ Auto by IFC type").on_hover_text("Walls → plaster, slabs → concrete, windows → glass, ducts → galvanized steel, …").clicked() {
                    self.auto_materials();
                }
                if ui.add_enabled(!self.mat_assign.is_empty(), egui::Button::new("Clear all")).clicked() {
                    self.mat_assign.clear();
                    changed = true;
                }
            });
            ui.label(RichText::new("Rules lower in the list win. Set rules stay dynamic: search sets pick up new or changed items.").weak().small());
            let mut del = None;
            let mut up = None;
            let names: Vec<String> = self.materials.iter().map(|m| m.name.clone()).collect();
            egui::ScrollArea::vertical().id_salt("matassign").max_height((ui.available_height() - 60.0).max(60.0)).show(ui, |ui| {
                for (k, a) in self.mat_assign.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        changed |= ui.checkbox(&mut a.enabled, "").changed();
                        ui.add_sized([170.0, 18.0], egui::Label::new(a.target.label()).truncate());
                        egui::ComboBox::from_id_salt(("am", k)).width(140.0).selected_text(a.material.clone()).show_ui(ui, |ui| {
                            for n in &names {
                                changed |= ui.selectable_value(&mut a.material, n.clone(), n).changed();
                            }
                        });
                        if ui.small_button("▲").clicked() && k > 0 {
                            up = Some(k);
                        }
                        if ui.small_button("✖").clicked() {
                            del = Some(k);
                        }
                    });
                }
                if self.mat_assign.is_empty() {
                    ui.label(RichText::new("No materials assigned - the model shows its file colours.").weak());
                }
            });
            if let Some(k) = up {
                self.mat_assign.swap(k, k - 1);
                changed = true;
            }
            if let Some(k) = del {
                self.mat_assign.remove(k);
                changed = true;
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("☀ Sun");
                changed |= ui.add(egui::Slider::new(&mut self.render.sun_az, -180.0..=180.0).text("azimuth °")).changed();
                changed |= ui.add(egui::Slider::new(&mut self.render.sun_el, 5.0..=90.0).text("height °")).changed();
            });
        });
        if pick_tex {
            self.request_path(Purpose::MaterialTexture);
        }
        if let Some(p) = lib_io {
            self.request_path(p);
        }
        if changed {
            self.materials_changed();
        }
    }

    fn tools_window(&mut self, ctx: &egui::Context) {
        if !self.show_tools {
            return;
        }
        let mut open = true;
        let mut t = self.settings.tools.clone();
        let mut save = false;
        egui::Window::new("External converters").open(&mut open).collapsible(false).default_width(640.0).show(ctx, |ui| {
            ui.label("Optional - ManageBIM uses these programs only if they are installed:");
            ui.add_space(4.0);
            egui::Grid::new("tools").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
                for (name, formats, url, path) in [
                    ("ODA File Converter", ".dwg (alternative reader)", "https://www.opendesign.com/guestfiles/oda_file_converter", &mut t.oda),
                    ("Navisworks (nwd2bimc.exe)", ".nwd .nwc .nwf .rvt - needs Navisworks Manage / Simulate", "https://www.autodesk.com/products/navisworks", &mut t.navisworks),
                ] {
                    let ok = !path.is_empty() && std::path::Path::new(path.as_str()).exists();
                    ui.label(RichText::new(if ok { "✔" } else { "✖" }).color(if ok { Color32::from_rgb(60, 170, 80) } else { Color32::from_rgb(210, 70, 60) }));
                    ui.vertical(|ui| {
                        ui.strong(name);
                        ui.label(RichText::new(formats).weak().small());
                    });
                    ui.add(egui::TextEdit::singleline(path).desired_width(330.0).hint_text("path to the program (.exe)"));
                    ui.horizontal(|ui| {
                        #[cfg(windows)]
                        if ui.small_button("Browse…").clicked() {
                            if let Some(p) = rfd::FileDialog::new().add_filter("Program", &["exe"]).pick_file() {
                                *path = p.to_string_lossy().to_string();
                            }
                        }
                        ui.hyperlink_to("Download", url);
                    });
                    ui.end_row();
                }
            });
            ui.add_space(4.0);
            ui.label(RichText::new("Built in (no extra program): IFC, BIMC, glTF / GLB, OBJ + MTL, STL, PLY, DXF, DWG (incl. ACIS solids), STEP, IGES, BREP, FreeCAD .FCStd. Blender, SketchUp, Rhino: export to glTF / OBJ / STEP. DGN: export to IFC or DWG.").weak().small());
            match crate::occt::status() {
                Ok(v) => ui.label(RichText::new(format!("✔ OpenCascade (STEP / IGES / BREP / FCStd / DWG solids): {v}")).small()),
                Err(e) => ui.label(RichText::new(format!("✖ OpenCascade runtime unavailable ({e}) - reinstall ManageBIM (the occt folder is missing).")).small().color(Color32::from_rgb(210, 140, 60))),
            };
            ui.horizontal(|ui| {
                if ui.button("Detect automatically").clicked() {
                    let d = crate::formats::detect_tools();
                    for (cur, new) in [(&mut t.oda, d.oda), (&mut t.navisworks, d.navisworks)] {
                        if !new.is_empty() {
                            *cur = new;
                        }
                    }
                }
                if ui.button(RichText::new("Save").strong()).clicked() {
                    save = true;
                }
            });
        });
        if t != self.settings.tools || save {
            self.settings.tools = t;
            crate::formats::set_tools(self.settings.tools.clone());
            self.settings.save();
        }
        if !open || save {
            self.show_tools = false;
        }
    }

    // ------------------------------------------------------------------ render / animation
    fn render_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.render_subtab, 0, "🎬 Animation & Output");
            ui.selectable_value(&mut self.render_subtab, 1, format!("🎨 Materials ({})", self.mat_assign.len()));
        });
        ui.separator();
        if self.render_subtab == 1 {
            self.materials_ui(ui);
            return;
        }
        if self.ffmpeg.is_none() {
            let ok = crate::video::ffmpeg_available();
            self.ffmpeg = Some(ok);
            if !ok && self.render.format == VideoFormat::Mp4 {
                self.render.format = VideoFormat::Avi;
            }
        }
        if self.cur_anim >= self.animations.len() {
            self.cur_anim = self.animations.len().saturating_sub(1);
        }
        let busy = self.render_job.is_some();
        ui.columns(3, |cols| {
            // ---- animations
            let ui = &mut cols[0];
            ui.label(RichText::new("Animations").strong());
            ui.horizontal_wrapped(|ui| {
                if ui.button("➕ New").on_hover_text("Empty animation - add keyframes from the current view").clicked() {
                    let n = self.animations.len() + 1;
                    self.animations.push(Animation::new(format!("Animation {n}")));
                    self.cur_anim = self.animations.len() - 1;
                }
                if ui.add_enabled(!self.viewpoints.is_empty(), egui::Button::new("📷 From viewpoints")).on_hover_text("Fly through all saved viewpoints").clicked() {
                    let mut a = Animation::new(format!("Walkthrough {}", self.animations.len() + 1));
                    for (i, v) in self.viewpoints.iter().enumerate() {
                        a.keys.push(crate::anim::Keyframe { t: i as f32 * self.key_step, cam: v.cam });
                    }
                    self.animations.push(a);
                    self.cur_anim = self.animations.len() - 1;
                }
                if ui.button("🔄 Orbit 360°").on_hover_text("Turntable around the model at the current height and distance").clicked() {
                    let bb = self.scene_aabb();
                    let a = Animation::orbit(format!("Orbit {}", self.animations.len() + 1), &self.cam, &bb, self.orbit_secs);
                    self.animations.push(a);
                    self.cur_anim = self.animations.len() - 1;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Orbit");
                ui.add(egui::DragValue::new(&mut self.orbit_secs).range(2.0..=600.0).suffix(" s"));
                ui.label("Key step");
                ui.add(egui::DragValue::new(&mut self.key_step).range(0.5..=60.0).speed(0.1).suffix(" s"));
            });
            ui.separator();
            let mut del = None;
            egui::ScrollArea::vertical().id_salt("anims").auto_shrink([false, false]).show(ui, |ui| {
                for i in 0..self.animations.len() {
                    ui.horizontal(|ui| {
                        let d = self.animations[i].duration();
                        if ui.selectable_label(i == self.cur_anim, format!("🎬 {:.1} s", d)).clicked() {
                            self.cur_anim = i;
                            self.stop_anim();
                        }
                        ui.add(egui::TextEdit::singleline(&mut self.animations[i].name).desired_width(140.0));
                        if ui.small_button("🗑").clicked() {
                            del = Some(i);
                        }
                    });
                }
                if self.animations.is_empty() {
                    ui.label(RichText::new("No animations yet.\nCreate an orbit, a walkthrough from viewpoints, or a new animation and capture keyframes from the 3D view.").weak());
                }
            });
            if let Some(i) = del {
                self.animations.remove(i);
                self.stop_anim();
            }

            // ---- keyframes + preview
            let ui = &mut cols[1];
            let ai = self.cur_anim;
            if ai >= self.animations.len() {
                ui.label(RichText::new("Select or create an animation.").weak());
            } else {
                ui.label(RichText::new(format!("Keyframes - {}", self.animations[ai].name)).strong());
                ui.horizontal_wrapped(|ui| {
                    if ui.button("📷 Capture keyframe").on_hover_text("Adds the current 3D view").clicked() {
                        let a = &mut self.animations[ai];
                        let t = if a.keys.is_empty() { 0.0 } else { a.duration() + self.key_step };
                        a.keys.push(crate::anim::Keyframe { t, cam: self.cam });
                    }
                    ui.menu_button("➕ Viewpoint ⏷", |ui| {
                        for v in self.viewpoints.clone() {
                            if ui.button(&v.name).clicked() {
                                let a = &mut self.animations[ai];
                                let t = if a.keys.is_empty() { 0.0 } else { a.duration() + self.key_step };
                                a.keys.push(crate::anim::Keyframe { t, cam: v.cam });
                                ui.close_menu();
                            }
                        }
                        if self.viewpoints.is_empty() {
                            ui.label(RichText::new("No viewpoints").weak());
                        }
                    });
                });
                let mut goto = None;
                let mut kdel = None;
                let mut kupd = None;
                let mut resort = false;
                egui::ScrollArea::vertical().id_salt("keys").max_height((ui.available_height() - 110.0).max(60.0)).show(ui, |ui| {
                    egui::Grid::new("keygrid").num_columns(4).striped(true).spacing([8.0, 3.0]).show(ui, |ui| {
                        for (k, key) in self.animations[ai].keys.iter_mut().enumerate() {
                            ui.label(format!("#{}", k + 1));
                            if ui.add(egui::DragValue::new(&mut key.t).range(0.0..=3600.0).speed(0.05).fixed_decimals(2).suffix(" s")).changed() {
                                resort = true;
                            }
                            ui.horizontal(|ui| {
                                if ui.small_button("Go to").clicked() {
                                    goto = Some(key.t);
                                }
                                if ui.small_button("Update").on_hover_text("Replace with the current view").clicked() {
                                    kupd = Some(k);
                                }
                            });
                            if ui.small_button("✖").clicked() {
                                kdel = Some(k);
                            }
                            ui.end_row();
                        }
                    });
                });
                if resort && !ui.ctx().is_using_pointer() {
                    self.animations[ai].sort();
                }
                if let Some(k) = kupd {
                    self.animations[ai].keys[k].cam = self.cam;
                }
                if let Some(k) = kdel {
                    self.animations[ai].keys.remove(k);
                }
                if let Some(t) = goto {
                    self.stop_anim();
                    self.goto_anim_time(t);
                }
                let has_tasks = !self.tasks.is_empty();
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut self.animations[ai].ease, "Ease in / out");
                    ui.add_enabled(has_tasks, egui::Checkbox::new(&mut self.animations[ai].sim_4d, "Play TimeLiner 4D")).on_hover_text("The construction sequence runs from start to end during the animation");
                    ui.checkbox(&mut self.anim_loop, "Loop preview");
                });
                let dur = self.animations[ai].duration();
                ui.horizontal(|ui| {
                    let playing = self.anim_play.is_some();
                    if ui.add_enabled(!busy && self.animations[ai].keys.len() >= 2, egui::Button::new(if playing { "⏸ Pause" } else { "▶ Play" })).clicked() {
                        if playing {
                            self.anim_play = None;
                        } else {
                            let start = if self.anim_time >= dur - 1e-3 { 0.0 } else { self.anim_time };
                            self.anim_play = Some(start);
                        }
                    }
                    if ui.add_enabled(!busy, egui::Button::new("⏹")).clicked() {
                        self.stop_anim();
                        self.goto_anim_time(0.0);
                    }
                    let mut t = self.anim_time;
                    ui.spacing_mut().slider_width = (ui.available_width() - 80.0).max(60.0);
                    if ui.add_enabled(!busy, egui::Slider::new(&mut t, 0.0..=dur.max(0.01)).fixed_decimals(1).suffix(" s")).changed() {
                        self.anim_play = None;
                        self.goto_anim_time(t);
                    }
                });
            }

            // ---- output
            let ui = &mut cols[2];
            ui.label(RichText::new("Output").strong());
            let ff = self.ffmpeg.unwrap_or(false);
            ui.add_enabled_ui(!busy, |ui| {
                egui::Grid::new("outgrid").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                    ui.label("Resolution");
                    ui.horizontal(|ui| {
                        let cur = crate::anim::RESOLUTIONS.iter().find(|r| r.0 == self.render.width && r.1 == self.render.height).map(|r| r.2).unwrap_or("Custom");
                        egui::ComboBox::from_id_salt("res").width(130.0).selected_text(cur).show_ui(ui, |ui| {
                            for (w, h, n) in crate::anim::RESOLUTIONS {
                                if ui.selectable_label(cur == n, format!("{n}  ({w}×{h})")).clicked() {
                                    self.render.width = w;
                                    self.render.height = h;
                                }
                            }
                        });
                        ui.add(egui::DragValue::new(&mut self.render.width).range(64..=7680));
                        ui.label("×");
                        ui.add(egui::DragValue::new(&mut self.render.height).range(64..=4320));
                    });
                    ui.end_row();
                    ui.label("Frame rate");
                    ui.horizontal(|ui| {
                        for f in [24, 25, 30, 60] {
                            ui.selectable_value(&mut self.render.fps, f, format!("{f}"));
                        }
                        ui.label("fps");
                    });
                    ui.end_row();
                    ui.label("Format");
                    egui::ComboBox::from_id_salt("fmt").width(260.0).selected_text(self.render.format.label()).show_ui(ui, |ui| {
                        for f in VideoFormat::ALL {
                            let en = f != VideoFormat::Mp4 || ff;
                            ui.add_enabled_ui(en, |ui| {
                                ui.selectable_value(&mut self.render.format, f, f.label());
                            });
                        }
                    });
                    ui.end_row();
                    ui.label("Quality");
                    ui.add(egui::Slider::new(&mut self.render.quality, 50..=100));
                    ui.end_row();
                    ui.label("Background");
                    egui::ComboBox::from_id_salt("bg").selected_text(self.render.background.label()).show_ui(ui, |ui| {
                        for b in [Background::Theme, Background::White, Background::Dark] {
                            ui.selectable_value(&mut self.render.background, b, b.label());
                        }
                    });
                    ui.end_row();
                    ui.label("Anti-aliasing");
                    ui.checkbox(&mut self.render.supersample, "2× supersampling");
                    ui.end_row();
                });
                if !ff {
                    ui.label(RichText::new("MP4 needs ffmpeg on the PATH (ffmpeg.org). AVI plays in Windows, PowerPoint and VLC.").weak().small());
                }
            });
            if let Some(j) = &self.render_job {
                let f = j.frame as f32 / j.frames.max(1) as f32;
                let txt = if j.frame >= j.frames { "Finishing…".to_string() } else { format!("Frame {} / {}", j.frame, j.frames) };
                ui.add(egui::ProgressBar::new(f).text(txt).desired_width(ui.available_width()));
                if ui.button("Cancel").clicked() {
                    self.cancel_render();
                }
            } else {
                let can = self.animations.get(self.cur_anim).map_or(false, |a| a.keys.len() >= 2);
                let frames = self.animations.get(self.cur_anim).map(|a| (a.duration() * self.render.fps as f32).ceil() as u32 + 1).unwrap_or(0);
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(can, egui::Button::new(RichText::new("🎞 Export Animation…").strong().color(Color32::WHITE)).fill(accent())).clicked() {
                        self.stop_anim();
                        self.request_path(Purpose::ExportVideo);
                    }
                    if ui.button("🖼 Render Image…").on_hover_text("Current view at the chosen resolution, PNG").clicked() {
                        self.request_path(Purpose::RenderImage);
                    }
                });
                if can {
                    ui.label(RichText::new(format!("{frames} frames at {}×{}", self.render.width, self.render.height)).weak().small());
                }
            }
        });
    }

    // ================================================================== ferestre
    fn windows(&mut self, ctx: &egui::Context) {
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
                Purpose::ExportClashBcf => "Export clashes as BCF",
                Purpose::ExportIfc => "Export to IFC4",
                Purpose::SharedFolder => "Shared issues folder",
                Purpose::ExportClashNavis => "Export clashes as Navisworks viewpoints",
                Purpose::ImportBcf => "Import BCF issues",
                Purpose::ImportTasksCsv => "Import CSV Schedule",
                Purpose::ExportTasksCsv => "Export CSV Schedule",
                Purpose::ExportCache(_) => "Export ManageBIM Cache (.bimc)",
                Purpose::ExportSets => "Export Sets",
                Purpose::ImportTextures => "Import image as material",
                Purpose::ExportVpHtml => "Viewpoints report (HTML)",
                Purpose::ExportVpXlsx => "Viewpoints report (Excel .xlsx)",
                Purpose::ExportBcf => "Export BCF 2.1 issues",
                Purpose::ExportNavisXml => "Export viewpoints for Navisworks (XML)",
                Purpose::ImportNavisXml => "Import Navisworks viewpoints (XML)",
                Purpose::ExportNavisSets => "Export sets for Navisworks (XML)",
                Purpose::ExportNavisClash => "Export clash tests for Navisworks (XML)",
                Purpose::MaterialTexture => "Texture image",
                Purpose::ExportMaterials => "Export material library",
                Purpose::ImportMaterials => "Import material library",
                Purpose::ExportVideo => "Export Animation",
                Purpose::RenderImage => "Render Image",
                Purpose::ImportSets => "Import Sets",
                Purpose::ExportProfile => "Export Appearance Profile",
                Purpose::ImportProfile => "Import Appearance Profile",
                Purpose::ExportCompare => "Save Comparison",
                Purpose::ImportCompare => "Open Comparison",
                Purpose::ExportCompareCsv => "Comparison report (CSV)",
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
                    ui.add(egui::Slider::new(&mut self.point_size, 1.0..=8.0).suffix(" px").fixed_decimals(0).step_by(1.0));
                    ui.end_row();
                });
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
                egui::Grid::new("light").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Brightness");
                    changed |= ui.add(egui::Slider::new(&mut self.render.exposure, 0.3..=2.5).fixed_decimals(2)).changed();
                    ui.end_row();
                    ui.label("Ambient light");
                    changed |= ui.add(egui::Slider::new(&mut self.render.ambient, 0.0..=3.0).fixed_decimals(2)).changed();
                    ui.end_row();
                    ui.label("Sun intensity");
                    changed |= ui.add(egui::Slider::new(&mut self.render.sun_power, 0.0..=3.0).fixed_decimals(2)).changed();
                    ui.end_row();
                    ui.label("Sun azimuth");
                    changed |= ui.add(egui::Slider::new(&mut self.render.sun_az, -180.0..=180.0).suffix("°")).changed();
                    ui.end_row();
                    ui.label("Sun height");
                    changed |= ui.add(egui::Slider::new(&mut self.render.sun_el, 5.0..=90.0).suffix("°")).changed();
                    ui.end_row();
                    ui.label("Contact shadows").on_hover_text("Ambient occlusion (SSAO): soft shadows in corners and where elements meet");
                    changed |= ui.add(egui::Slider::new(&mut self.render.ao, 0.0..=1.0).fixed_decimals(2)).changed();
                    ui.end_row();
                });
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
        egui::Area::new(egui::Id::new("toasts")).anchor(Align2::RIGHT_BOTTOM, [-16.0, -40.0]).interactable(false).show(ctx, |ui| {
            for (m, t, err) in self.toasts.iter().rev().take(5) {
                let a = (1.0 - (t.elapsed().as_secs_f32() - 5.0).max(0.0)).clamp(0.0, 1.0);
                egui::Frame::popup(ui.style()).fill(if *err { Color32::from_rgb(110, 30, 30) } else { Color32::from_rgb(30, 45, 65) }).multiply_with_opacity(a).show(ui, |ui| {
                    ui.set_max_width(420.0);
                    ui.label(RichText::new(m).color(Color32::WHITE));
                });
                ui.add_space(4.0);
            }
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

/// big ribbon button: icon on top, label below
fn rbtn(ui: &mut egui::Ui, icon: &str, label: &str, tip: &str, on: bool, enabled: bool) -> egui::Response {
    let enabled = enabled && ui.is_enabled();
    let font = FontId::proportional(10.5);
    let tw = ui.fonts(|f| f.layout_no_wrap(label.to_string(), font.clone(), Color32::WHITE).size().x);
    let w = (tw + 10.0).max(40.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 42.0), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let vis = ui.style().interact_selectable(&resp, on);
    let p = ui.painter();
    if on || (enabled && resp.hovered()) {
        let fill = if on { accent().gamma_multiply(0.28) } else { vis.weak_bg_fill };
        p.rect(rect.shrink(1.0), 4.0, fill, if on { egui::Stroke::new(1.0_f32, accent()) } else { egui::Stroke::NONE });
    }
    let col = if !enabled { ui.visuals().weak_text_color().gamma_multiply(0.6) } else if on { accent() } else { ui.visuals().text_color() };
    p.text(rect.center_top() + egui::vec2(0.0, 14.0), Align2::CENTER_CENTER, icon, FontId::proportional(17.0), col);
    p.text(rect.center_bottom() - egui::vec2(0.0, 8.0), Align2::CENTER_CENTER, label, font, col);
    if enabled {
        resp.on_hover_text(tip)
    } else {
        resp.on_hover_text(format!("{tip}  (not available now)"))
    }
}

/// labelled ribbon group with a separator after it
fn rgroup(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        let r = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 1.0;
                ui.set_min_height(46.0);
                ui.set_max_height(50.0);
                add(ui);
            })
            .response;
        let w = r.rect.width();
        ui.allocate_ui_with_layout(egui::vec2(w, 11.0), egui::Layout::top_down(egui::Align::Center), |ui| {
            ui.set_width(w);
            ui.label(RichText::new(title).size(9.5).weak());
        });
    });
    ui.separator();
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
