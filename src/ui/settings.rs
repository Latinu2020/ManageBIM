//! Fereastra de setări a aplicației: aspect (inclusiv font / DPI), navigare,
//! export DWG / DXF, fișiere și sincronizare. Toate modificările se salvează
//! imediat în `settings.json`, ca să nu se piardă la închidere.

use super::*;

/// Titlul unei sectiuni din Settings: majuscule mici in culoarea de accent + linie fina (stil Revit / VS Code).
fn section_title(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(crate::i18n::tr(title).to_uppercase()).size(11.5).strong().color(accent()));
    let r = ui.available_rect_before_wrap();
    let y = ui.cursor().top() + 1.0;
    ui.painter().hline(r.left()..=r.right(), y, egui::Stroke::new(1.0_f32, crate::theme::border()));
    ui.add_space(6.0);
}

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
                    section_title(ui, "Appearance");
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
                        ui.label("Quick size");
                        for p in [90, 100, 125, 150] {
                            let on = ((self.settings.ui_scale * 100.0).round() as i32) == p;
                            if ui.selectable_label(on, format!("{p}%")).clicked() && !on {
                                self.settings.ui_scale = p as f32 / 100.0;
                                self.settings.save();
                            }
                        }
                    });
                    if ui.checkbox(&mut self.settings.color_icons, "Coloured ribbon icons").on_hover_text("Colour the ribbon icons by what they do: files yellow, export blue, delete / clash red, measure green, view purple, section orange, search teal.").changed() {
                        self.settings.save();
                    }
                    ui.horizontal(|ui| {
                        ui.label("3D background");
                        for (code, label, tip) in [("theme", "Theme", "The gradient of the current theme"), ("white", "White", "Clean white - for reports and prints"), ("grey", "Grey", "Neutral grey - good contrast for all colours"), ("black", "Black", "Black - for presentations"), ("sky", "Sky", "Blue sky gradient")] {
                            if ui.selectable_label(self.settings.vp_bg == code, label).on_hover_text(tip).clicked() {
                                self.settings.vp_bg = code.into();
                                self.settings.save();
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("PBR texture size");
                        let tier = self.settings.tex_res;
                        let mut changed = false;
                        for (t, label) in [(0u32, "512 px"), (1, "1024 px"), (2, "2048 px")] {
                            let tip = match t {
                                2 => "Sharp textures from freepbr.com 2K packs. Uses more GPU memory, so fewer textures can be loaded at once.",
                                1 => "Good detail without using too much GPU memory.",
                                _ => "Fastest, lowest memory. Textures are resampled down to 512 px.",
                            };
                            changed |= ui.selectable_label(tier == t, label).on_hover_text(tip).clicked();
                        }
                        if changed {
                            self.settings.tex_res = tier;
                            crate::materials::set_tex_tier(tier);
                            self.tex_layers.clear();
                            self.pbr_layers.clear();
                            self.tex_failed.clear();
                            self.tex_loaded_res = crate::materials::tex_size();
                            self.materials_changed();
                            self.settings.save();
                        }
                        ui.label(RichText::new(format!("(max {} layers)", crate::materials::max_textures())).small().weak());
                    });
                    if ui.checkbox(&mut self.settings.no_lod, "Full detail in 3D view (disable adaptive LOD)")
                        .on_hover_text("Normally small elements are skipped while the camera moves to keep the view smooth; the PNG / path trace render always uses the full model. Turn this on to see every element even while moving.")
                        .changed()
                    {
                        self.settings.save();
                    }
                    ui.add_space(6.0);
                    section_title(ui, "Selection");
                    let mut sel_changed = false;
                    ui.horizontal(|ui| {
                        ui.label("Highlight colour");
                        let mut c = Color32::from_rgb(self.settings.sel_rgb[0], self.settings.sel_rgb[1], self.settings.sel_rgb[2]);
                        if ui.color_edit_button_srgba(&mut c).changed() {
                            self.settings.sel_rgb = [c.r(), c.g(), c.b()];
                            sel_changed = true;
                        }
                        for (name, rgb) in [("Blue", [60, 140, 255]), ("Orange", [255, 140, 0]), ("Green", [40, 200, 90]), ("Magenta", [230, 50, 200]), ("Yellow", [255, 220, 0])] {
                            let col = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                            let (r, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), Sense::click());
                            ui.painter().rect_filled(r, crate::theme::RADIUS_SM, col);
                            if self.settings.sel_rgb == rgb {
                                ui.painter().rect_stroke(r, crate::theme::RADIUS_SM, egui::Stroke::new(2.0_f32, ui.visuals().strong_text_color()));
                            }
                            if resp.on_hover_text(name).clicked() {
                                self.settings.sel_rgb = rgb;
                                sel_changed = true;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Highlight strength");
                        let mut pct = (self.settings.sel_mix * 100.0).round() as i32;
                        let r = ui.add(egui::Slider::new(&mut pct, 20..=100).step_by(5.0).suffix("%"));
                        if r.changed() {
                            self.settings.sel_mix = pct as f32 / 100.0;
                            sel_changed = true;
                        }
                        if ui.small_button("↺ Default").clicked() {
                            self.settings.sel_rgb = crate::theme::default_sel_rgb();
                            self.settings.sel_mix = crate::theme::default_sel_mix();
                            sel_changed = true;
                        }
                    })
                    .response
                    .on_hover_text("How strongly the selected elements are painted with the highlight colour (100% = solid colour).");
                    if sel_changed {
                        self.settings.save();
                        self.looks.clear();
                        self.colors_dirty = true;
                    }
                    // fontul interfetei: cele incluse + toate fonturile din sistem
                    let mut pick: Option<String> = None;
                    ui.horizontal(|ui| {
                        ui.label("Font");
                        let r = ui.add(egui::TextEdit::singleline(&mut self.settings.font_family).desired_width(120.0));
                        if r.changed() {
                            self.settings.save();
                        }
                        let btn = ui.small_button("⏷");
                        let id = ui.make_persistent_id("mb_font_list");
                        if btn.clicked() {
                            ui.memory_mut(|m| m.toggle_popup(id));
                        }
                        egui::popup_below_widget(ui, id, &btn, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                            ui.set_min_width(230.0);
                            egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                                for name in ["Inter", "Lato", "Default"] {
                                    let label = match name {
                                        "Default" => "Default (egui)".to_string(),
                                        n => format!("{n}  (included)"),
                                    };
                                    if ui.selectable_label(self.settings.font_family == name, label).clicked() {
                                        pick = Some(name.to_string());
                                    }
                                }
                                ui.separator();
                                let q = self.settings.font_family.trim().to_lowercase();
                                let q = if q.is_empty() || q == "inter" || q == "lato" || q == "default" { String::new() } else { q };
                                let mut shown = 0;
                                for (n, _) in theme::system_fonts() {
                                    if !q.is_empty() && !n.to_lowercase().contains(&q) {
                                        continue;
                                    }
                                    if ui.selectable_label(false, n).clicked() {
                                        pick = Some(n.clone());
                                    }
                                    shown += 1;
                                    if shown >= 400 {
                                        break;
                                    }
                                }
                                if shown == 0 {
                                    ui.label(RichText::new("No system font matches.").weak().small());
                                }
                            });
                        });
                    })
                    .response
                    .on_hover_text("Type a name or pick from the list: the two fonts included with ManageBIM, or any font installed in Windows.");
                    if let Some(f) = pick {
                        self.settings.font_family = f;
                        self.settings.save();
                        // forteaza reinstalarea fontului la urmatorul cadru
                        self.font_family_cur.clear();
                    }
                    ui.horizontal(|ui| {
                        ui.label("Font size (px)");
                        let mut px = if self.settings.font_size > 0.0 { self.settings.font_size } else { 14.0 };
                        let r = ui.add(egui::Slider::new(&mut px, 10.0..=26.0));
                        if r.changed() && (!r.dragged() || r.drag_stopped()) {
                            self.settings.font_size = px;
                            self.settings.save();
                            self.theme_dirty = true;
                        }
                    })
                    .response
                    .on_hover_text("The height of the text itself. Interface size scales the whole window; this sets the base text height (14 = default).");
                    ui.horizontal(|ui| {
                        ui.label("Language");
                        if ui.button(if self.settings.romanian { "Română" } else { "English" }).clicked() {
                            self.toggle_language();
                        }
                    });
                    ui.checkbox(&mut self.settings.nav_bar, "Floating navigation cube / bar in the 3D view");

                    // ------------------------------------------------ export
                    ui.separator();
                    section_title(ui, "DWG / DXF section export");
                    ui.horizontal(|ui| {
                        ui.label("View");
                        let cur = self.settings.cad_view;
                        egui::ComboBox::from_id_salt("mb_cad_view").selected_text(cur.label()).show_ui(ui, |ui| {
                            for v in crate::app::CadView::ALL {
                                let label = v.label();
                                let mut tmp = v;
                                if ui.selectable_value(&mut tmp, cur, label).clicked() {
                                    self.settings.cad_view = v;
                                    if v != crate::app::CadView::Current {
                                        self.settings.cad_view_pos = 0.0;
                                    }
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
                    if ui.checkbox(&mut self.settings.snap_cut_to_grid, "Snap the cut plane to grids / levels").changed() {
                        self.settings.save();
                    }
                    ui.checkbox(&mut self.cad_annotate, "Annotations: hatch, element tags, grids, levels")
                        .on_hover_text("Turn off for a clean outline (only the cut edges, one DWG layer per element type).");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.cad_hatch, "Hatch cut materials")
                            .on_hover_text("Hatch the cut materials (filled contours) - off = clean outline.");
                        ui.checkbox(&mut self.cad_dims, "Dimensions (chains)")
                            .on_hover_text("Include dimension chains in the exported DWG / PDF cut.");
                        ui.checkbox(&mut self.cad_levels, "Levels")
                            .on_hover_text("Include level lines and labels in the exported DWG / PDF cut.");
                    });
                    ui.checkbox(&mut self.settings.show_cad_plane, "Highlight cut plane in 3D")
                        .on_hover_text("Off by default: the cut plane quad and drag handle appear only when you turn this on.");
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
                    section_title(ui, "Navigation & units");
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
                    ui.horizontal(|ui| {
                        ui.label("Units");
                        for (code, label) in [("m", "m"), ("mm", "mm"), ("in", "in"), ("ft", "ft")] {
                            if ui.selectable_label(self.settings.unit_system == code, label).clicked() {
                                self.settings.unit_system = code.into();
                                self.settings.units_mm = code == "mm";
                                crate::app::set_unit(match code { "mm" => crate::app::Unit::Mm, "in" => crate::app::Unit::In, "ft" => crate::app::Unit::Ft, _ => crate::app::Unit::M });
                                self.settings.save();
                            }
                        }
                        ui.label("decimals");
                        if ui.add(egui::DragValue::new(&mut self.settings.unit_precision).range(0..=6)).changed() {
                            crate::app::set_precision(self.settings.unit_precision);
                            self.settings.save();
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("OSNAP (F3)");
                        ui.checkbox(&mut self.osnap_on, "On")
                            .on_hover_text("Object Snap: snap to geometry while measuring, relocating or picking grid points. F3 toggles it.");
                        ui.label("radius");
                        if ui.add(egui::DragValue::new(&mut self.settings.snap_radius).range(6.0..=24.0)).changed() {
                            self.settings.save();
                        }
                        ui.label("px");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Snap to");
                        for (bit, name) in [(1u8, "Vertex"), (2, "Midpoint"), (4, "Edge"), (8, "Surface")] {
                            let on = self.settings.snap_types & bit != 0;
                            let mut t = on;
                            if ui.checkbox(&mut t, name).changed() {
                                if t { self.settings.snap_types |= bit; } else { self.settings.snap_types &= !bit; }
                                self.settings.save();
                            }
                        }
                    });
                    ui.separator();
                    section_title(ui, "Keyboard shortcuts");
                    ui.label(RichText::new("Click a binding, press the new keys, then Enter. Esc cancels.").small().weak());
                    let mut reset = false;
                    egui::Grid::new("kb_grid").num_columns(3).spacing([10.0, 4.0]).show(ui, |ui| {
                        for (cmd, _default) in crate::keybind::COMMANDS {
                            ui.label(cmd.replace('_', " "));
                            let cur = crate::keybind::binding(&self.settings.keybinds, cmd);
                            let id = egui::Id::new(("kb_edit", cmd));
                            let editing: Option<String> = ui.data(|d| d.get_temp(id));
                            let mut buf = editing.clone().unwrap_or_else(|| cur.clone());
                            let te = ui.add(egui::TextEdit::singleline(&mut buf).desired_width(120.0));
                            if editing.is_none() && te.clicked() {
                                ui.data_mut(|d| d.insert_temp(id, cur.clone()));
                            }
                            if editing.is_some() {
                                if ui.input(|k| k.key_pressed(Key::Enter)) {
                                    if let Some(conflict) = crate::keybind::conflicts(&self.settings.keybinds, cmd, &buf) {
                                        self.toast(format!("Conflict with '{conflict}'"));
                                    } else {
                                        if buf == crate::keybind::binding(&self.settings.keybinds, cmd) { self.settings.keybinds.remove(cmd); }
                                        else { self.settings.keybinds.insert(cmd.to_string(), buf.clone()); }
                                        self.settings.save();
                                    }
                                    ui.data_mut(|d| d.remove::<String>(id));
                                } else if ui.input(|k| k.key_pressed(Key::Escape)) {
                                    ui.data_mut(|d| d.remove::<String>(id));
                                }
                            }
                            if ui.small_button("↺").on_hover_text("Reset to default").clicked() {
                                self.settings.keybinds.remove(cmd);
                                self.settings.save();
                            }
                            ui.end_row();
                        }
                    });
                    if ui.small_button("Reset all shortcuts").clicked() { reset = true; }
                    if reset { self.settings.keybinds.clear(); self.settings.save(); }

                    // ------------------------------------------------ profil partajabil
                    ui.separator();
                    section_title(ui, "Settings profile");
                    ui.horizontal(|ui| {
                        if ui.button("📤 Export profile (.mbprofile)").on_hover_text("Save theme, panels, shortcuts, units and filters to a file you can share with colleagues").clicked() {
                            self.request_path(Purpose::ExportSettingsProfile);
                        }
                        if ui.button("📥 Import profile").on_hover_text("Load a .mbprofile. Local paths (recent files, tools) are not imported.").clicked() {
                            self.request_path(Purpose::ImportSettingsProfile);
                        }
                    });
                    // ------------------------------------------------ fisiere / sync
                    ui.separator();
                    section_title(ui, "Scripts");
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
                    section_title(ui, "Files & live link");
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
