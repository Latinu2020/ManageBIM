//! Fereastra Script Library (Rhai): biblioteca de scripturi, constructorul vizual si manualul.
//! Mutata din ui.rs in 0.28 (fisierul depasea 6 000 de linii).
use super::*;

impl App {
    pub(super) fn script_window(&mut self, ctx: &egui::Context) {
        if !self.script_open { return; }
        let mut open = self.script_open;
        egui::Window::new("📜 Script Library")
            .open(&mut open)
            .default_size([620.0, 540.0])
            .min_width(450.0)
            .min_height(320.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("📁 Choose folder…").on_hover_text("Pick the folder with your .rhai scripts").clicked() {
                        self.request_path(Purpose::SelectScriptsDir);
                    }
                    if ui.button("↺ Default").on_hover_text("Use %APPDATA%\\ManageBIM\\Scripts").clicked() {
                        self.settings.scripts_dir.clear();
                        self.settings.save();
                        self.toast("Scripts folder reset to default.");
                    }
                    if ui.button("➕ New script").clicked() {
                        match script::create(&self.settings.scripts_dir) {
                            Ok(p) => self.toast(format!("Script created: {}", crate::file_name(&p.to_string_lossy().to_string()))),
                            Err(e) => self.error(e),
                        }
                    }
                    if ui.button("📥 Install examples").clicked() {
                        match script::install_examples(&self.settings.scripts_dir) {
                            Ok(n) => self.toast(format!("Installed {n} example scripts.")),
                            Err(e) => self.error(e),
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Folder:").small().strong());
                    ui.label(RichText::new(script::dir(&self.settings.scripts_dir).display().to_string()).small().color(crate::theme::accent()));
                    if ui.small_button("📋").on_hover_text("Copy folder path").clicked() {
                        ui.ctx().copy_text(script::dir(&self.settings.scripts_dir).to_string_lossy().to_string());
                        self.toast("Scripts folder path copied to clipboard.");
                    }
                });
                ui.separator();
                self.script_builder_ui(ui);
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.label(RichText::new("BUILT-IN SCRIPTS").size(12.0).strong().color(ui.visuals().weak_text_color()));
                    let mut last_cat = "";
                    for spec in script_catalog::CATALOG {
                        if spec.category != last_cat {
                            ui.add_space(4.0);
                            ui.label(RichText::new(spec.category).strong().size(13.0));
                            last_cat = spec.category;
                        }
                        let accent = crate::theme::accent();
                        egui::Frame::none().fill(ui.visuals().faint_bg_color).rounding(7.0).inner_margin(egui::Margin::symmetric(9.0, 6.0)).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(spec.name).strong());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button(RichText::new("▶ Run").color(accent)).clicked() {
                                        let src = script_catalog::source(spec, &self.settings.script_params);
                                        self.run_source(spec.name, &src);
                                    }
                                });
                            });
                            ui.label(RichText::new(spec.desc).small().weak());
                            if !spec.params.is_empty() {
                                ui.collapsing("Parameters", |ui| {
                                    egui::Grid::new(("script_params", spec.id)).num_columns(2).spacing([8.0, 3.0]).show(ui, |ui| {
                                        for p in spec.params {
                                            let mut changed = false;
                                            {
                                                let val = self.settings.script_params.entry(p.key.to_string()).or_insert_with(|| p.default.to_string());
                                                ui.label(p.label);
                                                if ui.add(egui::TextEdit::singleline(val).desired_width(190.0)).changed() {
                                                    changed = true;
                                                }
                                            }
                                            if changed { self.settings.save(); }
                                            ui.end_row();
                                        }
                                    });
                                });
                            }
                        });
                        ui.add_space(3.0);
                    }
                    ui.separator();
                    ui.label(RichText::new("MY SCRIPTS (.rhai)").size(12.0).strong().color(ui.visuals().weak_text_color()));
                    let scripts = script::list(&self.settings.scripts_dir);
                    if scripts.is_empty() {
                        ui.label(RichText::new("No .rhai files yet — use New script or Install examples.").weak().small());
                    } else {
                        for p in scripts {
                            let name = crate::file_name(&p.to_string_lossy().to_string());
                            ui.horizontal(|ui| {
                                ui.label("📜");
                                ui.label(&name);
                                if ui.button("▶ Run").clicked() {
                                    self.run_script(&p.to_string_lossy().to_string());
                                }
                                if ui.small_button("🗑").on_hover_text("Delete this script").clicked() {
                                    let _ = std::fs::remove_file(&p);
                                    self.toast(format!("Deleted {name}"));
                                }
                            })
                            .response
                            .on_hover_text(p.display().to_string());
                        }
                    }
                    ui.separator();
                    // ghidul urmeaza limba interfetei (0.28): in engleza descrierile `en`, regulile doar in romana
                    let ro = crate::i18n::is_romanian();
                    ui.collapsing(RichText::new(if ro { "📖 Manual scripturi (Rhai)" } else { "📖 Script manual (Rhai)" }).strong(), |ui| {
                        ui.label(RichText::new(if ro { "Nu trebuie să scrii cod: folosește «Visual script builder» de mai sus. Acest ghid e pentru când vrei să scrii singur." } else { "No code needed: use the Visual script builder above. This guide is for when you want to write scripts yourself." }).small().weak());
                        ui.add_space(4.0);
                        if ro {
                        ui.label(RichText::new("REGULI DE BAZĂ").strong().color(crate::theme::accent()));
                        for (rule, desc) in crate::script_guide::RULES_RO {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(*rule).strong().monospace());
                                ui.label(RichText::new(*desc).small());
                            });
                        }
                        ui.add_space(6.0);
                        }
                        ui.label(RichText::new(if ro { "FUNCȚII DISPONIBILE (9)" } else { "AVAILABLE FUNCTIONS (9)" }).strong().color(crate::theme::accent()));
                        for e in crate::script_guide::ENTRIES {
                            egui::Frame::none().fill(ui.visuals().faint_bg_color).rounding(crate::theme::RADIUS_SM).inner_margin(egui::Margin::symmetric(7.0, 4.0)).show(ui, |ui| {
                                ui.label(RichText::new(e.func).strong().monospace().color(crate::theme::accent()));
                                ui.label(RichText::new(if ro { e.ro } else { e.en }).small());
                                ui.label(RichText::new(e.example).small().monospace().color(ui.visuals().weak_text_color()));
                                if ro {
                                    ui.label(RichText::new(e.effect_ro).small().weak());
                                }
                            });
                            ui.add_space(2.0);
                        }
                    });
                });
            });
        self.script_open = open;
    }
}
