//! Fereastra de setări a aplicației: aspect (inclusiv font / DPI), navigare,
//! export DWG / DXF, fișiere și sincronizare. Toate modificările se salvează
//! imediat în `settings.json`, ca să nu se piardă la închidere.

use super::*;

impl App {
    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open { return; }
        let mut open = self.settings_open;
        egui::Window::new("⚙ Settings")
            .open(&mut open)
            .default_size([640.0, 660.0])
            .min_width(460.0)
            .min_height(320.0)
            .resizable(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    // ------------------------------------------------ aspect
                    ui.heading("Appearance");
                    ui.horizontal(|ui| {
                        ui.label("Theme");
                        for t in [Theme::Studio, Theme::Dark, Theme::Nord, Theme::TokyoNight, Theme::Light] {
                            if ui.selectable_label(self.settings.theme == t, t.label()).clicked() {
                                self.set_theme(t);
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Interface size (fonts / DPI)");
                        let mut pct = (self.settings.ui_scale * 100.0).round() as i32;
                        let r = ui.add(egui::Slider::new(&mut pct, 75..=200).step_by(5.0).suffix("%"));
                        if r.changed() && (!r.dragged() || r.drag_stopped()) {
                            self.settings.ui_scale = pct as f32 / 100.0;
                            self.settings.save();
                        }
                    })
                    .response
                    .on_hover_text("Scales the whole interface: text, tables, panels and dialogs. 100% = default.");
                    ui.horizontal(|ui| {
                        ui.label("Language");
                        if ui.button(if self.settings.romanian { "Română" } else { "English" }).clicked() {
                            self.toggle_language();
                        }
                    });
                    ui.checkbox(&mut self.settings.nav_bar, "Floating navigation cube / bar in the 3D view");

                    // ------------------------------------------------ export
                    ui.separator();
                    ui.heading("DWG / DXF section export");
                    ui.horizontal(|ui| {
                        ui.label("View");
                        let cur = self.settings.cad_view;
                        egui::ComboBox::from_id_salt("mb_cad_view").selected_text(cur.label()).show_ui(ui, |ui| {
                            for v in crate::app::CadView::ALL {
                                let label = v.label();
                                let mut tmp = v;
                                if ui.selectable_value(&mut tmp, cur, label).clicked() {
                                    self.settings.cad_view = v;
                                    self.settings.save();
                                }
                            }
                        });
                    })
                    .response
                    .on_hover_text("Current = the active section plane. Front / Back / Left / Right cut the whole model from that direction (no section needed).");
                    if self.settings.cad_view != crate::app::CadView::Current {
                        ui.horizontal(|ui| {
                            ui.label("Cut position");
                            let mut pct = (self.settings.cad_view_pos * 100.0).round() as i32;
                            if ui.add(egui::Slider::new(&mut pct, 0..=100).suffix("%")).changed() {
                                self.settings.cad_view_pos = pct as f32 / 100.0;
                                self.settings.save();
                            }
                        })
                        .response
                        .on_hover_text("Distance from the chosen face: 0% = exactly on that face (front / left / top), 100% = the opposite face.");
                    }
                    ui.checkbox(&mut self.cad_annotate, "Annotations: hatch, element tags, grids, levels")
                        .on_hover_text("Turn off for a clean outline (only the cut edges, one DWG layer per element type).");
                    ui.horizontal(|ui| {
                        ui.label("BCF export version");
                        if ui.selectable_label(self.settings.bcf_version == 2, "2.1").clicked() {
                            self.settings.bcf_version = 2;
                            self.settings.save();
                        }
                        if ui.selectable_label(self.settings.bcf_version == 3, "3.0").clicked() {
                            self.settings.bcf_version = 3;
                            self.settings.save();
                        }
                    })
                    .response
                    .on_hover_text("BCF 3.0 places comments inside the Topic (newer BIMcollab / Solibri).");
                    if ui.button("Title block & sheet format (A3 – A0)…").clicked() {
                        self.title_block_open = true;
                    }

                    // ------------------------------------------------ navigare
                    ui.separator();
                    ui.heading("Navigation & units");
                    ui.horizontal(|ui| {
                        ui.label("Walk speed (m/s)");
                        if ui.add(egui::DragValue::new(&mut self.settings.walk_speed).range(0.2..=10.0).speed(0.1)).changed() {
                            self.settings.save();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Eye height (m)");
                        if ui.add(egui::DragValue::new(&mut self.settings.eye_height).range(0.8..=2.5).speed(0.05)).changed() {
                            self.settings.save();
                        }
                    });
                    if ui.checkbox(&mut self.settings.gravity, "Gravity when walking").changed() { self.settings.save(); }
                    if ui.checkbox(&mut self.settings.collision, "Collision with geometry").changed() { self.settings.save(); }
                    if ui.checkbox(&mut self.settings.units_mm, "Lengths in millimetres").changed() {
                        self.settings.save();
                        crate::app::set_units_mm(self.settings.units_mm);
                    }

                    // ------------------------------------------------ fisiere / sync
                    ui.separator();
                    ui.heading("Scripts");
                    ui.horizontal(|ui| {
                        ui.label("Scripts folder");
                        if ui.add(egui::TextEdit::singleline(&mut self.settings.scripts_dir).desired_width(280.0).hint_text("default: %APPDATA%\\ManageBIM\\Scripts")).changed() {
                            self.settings.save();
                        }
                    })
                    .response
                    .on_hover_text("Leave empty for the default folder. Change it any time - the Script console picks it up immediately.");
                    ui.label(RichText::new(super::script::dir(&self.settings.scripts_dir).display().to_string()).weak().small());

                    ui.separator();
                    ui.heading("Files & live link");
                    if ui.checkbox(&mut self.settings.auto_cache, "Create a .bimc cache when opening a model").changed() { self.settings.save(); }
                    if ui.checkbox(&mut self.settings.use_cache, "Open the up-to-date .bimc cache instead of the source").changed() { self.settings.save(); }
                    if ui.checkbox(&mut self.settings.sync_selection, "Sync selection with Revit / Navisworks").changed() { self.settings.save(); }
                    if ui.checkbox(&mut self.settings.sync_follow, "Zoom to the selection coming from Revit / Navisworks").changed() { self.settings.save(); }

                    ui.separator();
                    ui.label(RichText::new("Settings are saved automatically in settings.json").weak().small());
                });
            });
        self.settings_open = open;
    }
}
