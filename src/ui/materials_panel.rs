//! Fereastra Materials, reorganizata: biblioteca, editor, harti PBR, parametri OpenPBR,
//! asignari si import / export (.bimmat, .mtlx MaterialX, zip PBR, imagini).
use super::*;

fn swatch(ui: &mut egui::Ui, m: &crate::materials::Material) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(22.0, 16.0), egui::Sense::hover());
    let c = Color32::from_rgba_unmultiplied(m.color[0], m.color[1], m.color[2], m.rgba()[3].max(80));
    ui.painter().rect_filled(r, 3.0, c);
    if !m.texture.is_empty() {
        ui.painter().text(r.center(), Align2::CENTER_CENTER, "🖼", FontId::proportional(11.0), Color32::BLACK);
    } else if m.coat > 0 || m.sheen > 0 {
        ui.painter().text(r.center(), Align2::CENTER_CENTER, "✦", FontId::proportional(11.0), Color32::WHITE);
    }
    if m.metal > 40 || m.gloss > 60 {
        ui.painter().circle_filled(r.left_top() + egui::vec2(6.0, 5.0), 2.5, Color32::from_white_alpha(200));
    }
    ui.painter().rect_stroke(r, 3.0, Stroke::new(1.0_f32, Color32::from_gray(120)));
}

fn section(ui: &mut egui::Ui, title: &str, tip: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(crate::i18n::tr(title).to_uppercase()).size(11.0).strong().color(accent())).on_hover_text(tip);
    ui.add_space(2.0);
}

impl App {
    pub(super) fn materials_ui(&mut self, ui: &mut egui::Ui) {
        if self.mat_sel >= self.materials.len() {
            self.mat_sel = 0;
        }
        let mut changed = false;
        let mut pick_tex: Option<Purpose> = None;
        let mut io: Option<Purpose> = None;

        // dialogul de asignare dupa importul unui pachet PBR
        if !self.pending_mat_assign.is_empty() {
            let names = self.pending_mat_assign.clone();
            let mut choice: Option<u8> = None;
            egui::Window::new("🎨 PBR material imported")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ui.ctx(), |ui| {
                    ui.label(RichText::new(format!("Where should {} be applied?", names.join(", "))).strong());
                    ui.label(RichText::new("The material is already in your library. You can change this later.").small().weak());
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(!self.selected.is_empty(), egui::Button::new(format!("🎯 Selection ({})", self.selected.len()))).clicked() {
                            choice = Some(0);
                        }
                        if ui.button("🌐 All visible items").clicked() {
                            choice = Some(1);
                        }
                        if ui.button("🏗 By IFC type…").clicked() {
                            choice = Some(2);
                        }
                        if ui.button("Later").clicked() {
                            choice = Some(3);
                        }
                    });
                });
            if let Some(c) = choice {
                let all = self.pending_mat_assign.clone();
                self.pending_mat_assign.clear();
                match c {
                    0 => {
                        let v: Vec<_> = self.selected.iter().copied().collect();
                        for n in &all {
                            self.assign_material(MatTarget::Items(v.clone()), n);
                        }
                    }
                    1 => {
                        let v: Vec<_> = self.all_ids().into_iter().filter(|id| self.is_visible(*id)).collect();
                        for n in &all {
                            self.assign_material(MatTarget::Items(v.clone()), n);
                        }
                        self.toast(format!("{} applied to all visible items.", all.join(", ")));
                    }
                    _ => {}
                }
            }
        }

        // ---- toolbar
        ui.horizontal_wrapped(|ui| {
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
            ui.separator();
            if ui.small_button("📦 PBR .zip / folder…").on_hover_text("freepbr.com, Poliigon, ambientCG, Poly Haven: all maps are kept").clicked() {
                io = Some(Purpose::ImportPbr);
            }
            if ui.small_button("📥 Images…").clicked() {
                io = Some(Purpose::ImportTextures);
            }
            ui.menu_button("Library ⏷", |ui| {
                if ui.button("📁 Import PBR folder…").clicked() {
                    io = Some(Purpose::ImportPbrFolder);
                    ui.close_menu();
                }
                if ui.button("⭐ Save selected to my library").clicked() {
                    let i = self.mat_sel;
                    self.save_material_to_library(i);
                    ui.close_menu();
                }
                if ui.button("📂 Open my library folder").clicked() {
                    if let Some(d) = crate::materials::user_library_dir() {
                        let _ = std::fs::create_dir_all(&d);
                        crate::formats::open_with_system(&d);
                    }
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("📤 Export .bimmat…").clicked() {
                    io = Some(Purpose::ExportMaterials);
                    ui.close_menu();
                }
                if ui.button("📥 Import .bimmat…").clicked() {
                    io = Some(Purpose::ImportMaterials);
                    ui.close_menu();
                }
                if ui.button("📤 Export MaterialX (.mtlx)…").on_hover_text("OpenPBR Surface + images, for Maya / Houdini / Blender / Omniverse").clicked() {
                    io = Some(Purpose::ExportMaterialX);
                    ui.close_menu();
                }
                if ui.button("📥 Import MaterialX (.mtlx)…").on_hover_text("Reads OpenPBR Surface / UsdPreviewSurface parameters and image paths (simple materials, no graph evaluation)").clicked() {
                    io = Some(Purpose::ImportMaterialX);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Reset library").clicked() {
                    for m in crate::materials::full_library() {
                        if let Some(x) = self.materials.iter_mut().find(|x| x.name == m.name) {
                            *x = m;
                        } else {
                            self.materials.push(m);
                        }
                    }
                    changed = true;
                    ui.close_menu();
                }
            });
            ui.checkbox(&mut self.materials_on, "Show materials in 3D");
        });

        ui.columns(3, |cols| {
            // ---- lista
            let ui = &mut cols[0];
            section(ui, "Library", "Click a material to edit it");
            egui::ScrollArea::vertical().id_salt("matlib").max_height(ui.available_height() * 0.55).show(ui, |ui| {
                for i in 0..self.materials.len() {
                    ui.horizontal(|ui| {
                        swatch(ui, &self.materials[i]);
                        let m = &self.materials[i];
                        let used = self.mat_assign.iter().any(|a| a.material == m.name);
                        let mut t = RichText::new(&m.name);
                        if used {
                            t = t.strong();
                        }
                        if m.is_pbr() || m.coat > 0 || m.sheen > 0 {
                            t = t.color(accent());
                        }
                        if ui.selectable_label(i == self.mat_sel, t).clicked() {
                            self.mat_sel = i;
                        }
                    });
                }
            });

            // ---- editor
            let ui = &mut cols[1];
            let i = self.mat_sel;
            if i >= self.materials.len() {
                return;
            }
            let old_name = self.materials[i].name.clone();
            let m = &mut self.materials[i];
            section(ui, "Surface", "Base colour, gloss and transparency");
            egui::Grid::new("mated_base").num_columns(2).spacing([8.0, 3.0]).show(ui, |ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut m.name).desired_width(160.0));
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
                ui.label("Pattern");
                egui::ComboBox::from_id_salt("pat").selected_text(m.pattern.label()).show_ui(ui, |ui| {
                    for p in crate::materials::Pattern::ALL {
                        changed |= ui.selectable_value(&mut m.pattern, p, p.label()).changed();
                    }
                });
                ui.end_row();
                ui.label("Texture size");
                changed |= ui.add(egui::DragValue::new(&mut m.scale).range(0.05..=4.0).speed(0.01).suffix(" m")).changed();
                ui.end_row();
            });

            section(ui, "OpenPBR Surface", "Clean coat, fabric sheen and index of refraction");
            egui::Grid::new("mated_openpbr").num_columns(2).spacing([8.0, 3.0]).show(ui, |ui| {
                ui.label("Coat (clearcoat)");
                changed |= ui.add(egui::Slider::new(&mut m.coat, 0..=100).suffix(" %")).on_hover_text("A thin clear layer over the surface, like lacquer or a varnished car").changed();
                ui.end_row();
                ui.label("Coat roughness");
                changed |= ui.add(egui::Slider::new(&mut m.coat_roughness, 0..=100)).changed();
                ui.end_row();
                ui.label("Sheen");
                changed |= ui.add(egui::Slider::new(&mut m.sheen, 0..=100).suffix(" %")).on_hover_text("Soft grazing-angle reflections of fabric, velvet and upholstery").changed();
                ui.end_row();
                ui.label("Refraction (IOR)");
                changed |= ui.add(egui::DragValue::new(&mut m.ior).range(1.0..=2.5).speed(0.01)).on_hover_text("1.0 = air, 1.5 = glass, 1.6 = acrylic, 2.4 = diamond").changed();
                ui.end_row();
                if ui.button("↺ Reset OpenPBR").clicked() {
                    m.coat = 0;
                    m.coat_roughness = 5;
                    m.sheen = 0;
                    m.ior = crate::materials::default_ior();
                    changed = true;
                }
                ui.end_row();
            });

            section(ui, "Texture maps", "Albedo / normal / roughness / metallic / AO / height");
            egui::Grid::new("mated_maps").num_columns(2).spacing([8.0, 3.0]).show(ui, |ui| {
                let mut map_row = |ui: &mut egui::Ui, label: &str, path: &mut String, purpose: Purpose| {
                    ui.label(label);
                    ui.horizontal(|ui| {
                        if path.is_empty() {
                            ui.label(RichText::new("—").weak());
                        } else {
                            ui.label(RichText::new(crate::file_name(path)).small()).on_hover_text(path.as_str());
                            if ui.small_button("✖").clicked() {
                                path.clear();
                                changed = true;
                            }
                        }
                        if ui.small_button("📁").clicked() {
                            pick_tex = Some(purpose);
                        }
                    });
                    ui.end_row();
                };
                map_row(ui, "Albedo", &mut m.texture, Purpose::MaterialTexture);
                map_row(ui, "Normal", &mut m.normal_map, Purpose::MaterialNormal);
                map_row(ui, "Roughness", &mut m.roughness_map, Purpose::MaterialRoughness);
                map_row(ui, "Metallic", &mut m.metallic_map, Purpose::MaterialMetallic);
                map_row(ui, "AO", &mut m.ao_map, Purpose::MaterialAo);
                map_row(ui, "Height", &mut m.height_map, Purpose::MaterialHeight);
                ui.label("Normal strength");
                changed |= ui.add(egui::Slider::new(&mut m.normal_strength, 0.0..=2.0).step_by(0.05).suffix("×")).changed();
                ui.end_row();
                ui.label("Roughness ×");
                changed |= ui.add(egui::Slider::new(&mut m.rough_mult, 0.0..=2.0).step_by(0.05).suffix("×")).changed();
                ui.end_row();
                ui.label("Metallic ×");
                changed |= ui.add(egui::Slider::new(&mut m.metal_mult, 0.0..=2.0).step_by(0.05).suffix("×")).changed();
                ui.end_row();
            });

            // numele schimbat se aplica regulilor dupa ce terminam de editat materialul
            let new_name = m.name.clone();
            // previzualizare
            let mat = &self.materials[self.mat_sel];
            let tex = if !mat.texture.is_empty() && self.tex_loaded_res == crate::materials::tex_size() {
                self.tex_layers.iter().find(|(k, _)| k == &mat.texture).map(|(_, px)| px.clone())
            } else {
                None
            };
            let need = self.mat_preview.as_ref().map(|(p, _)| p != &mat.texture).unwrap_or(true);
            if !mat.texture.is_empty() && need {
                if let Some(px) = tex {
                    let n = crate::materials::tex_size() as usize;
                    let img = eframe::egui::ColorImage::from_rgba_unmultiplied([n, n], &px);
                    let t = ui.ctx().load_texture(format!("mat_preview_{}", mat.texture), img, egui::TextureOptions::LINEAR);
                    self.mat_preview = Some((mat.texture.clone(), t));
                }
            }
            if let Some((p, t)) = &self.mat_preview {
                if p == &mat.texture {
                    ui.horizontal(|ui| {
                        ui.image((t.id(), egui::vec2(96.0, 96.0)));
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Albedo preview").small().weak());
                            ui.label(RichText::new(&mat.name).strong());
                            ui.label(RichText::new(format!("IOR {:.2} · coat {}% · sheen {}%", mat.ior, mat.coat, mat.sheen)).small().weak());
                        });
                    });
                }
            }
            if new_name != old_name {
                for a in self.mat_assign.iter_mut().filter(|a| a.material == old_name) {
                    a.material = new_name.clone();
                }
                changed = true;
            }

            // ---- asignari (coloana din dreapta)
            let ui = &mut cols[2];
            section(ui, "Assignments", "Rules lower in the list win");
            let mname = self.materials.get(self.mat_sel).map(|m| m.name.clone()).unwrap_or_default();
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!self.selected.is_empty(), egui::Button::new(format!("🎯 Selection ({})", self.selected.len()))).clicked() {
                    let v: Vec<_> = self.selected.iter().copied().collect();
                    self.assign_material(MatTarget::Items(v), &mname);
                }
                ui.menu_button("Set ⏷", |ui| {
                    for s in self.sets.clone() {
                        if ui.button(&s.name).clicked() {
                            self.assign_material(MatTarget::Set(s.name.clone()), &mname);
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button("IFC type ⏷", |ui| {
                    egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                        for t in self.cache_types.clone() {
                            if ui.button(&t).clicked() {
                                self.assign_material(MatTarget::Type(t.clone()), &mname);
                                ui.close_menu();
                            }
                        }
                    });
                });
                if ui.button("⚡ Auto by IFC type").clicked() {
                    self.auto_materials();
                }
                if ui.add_enabled(!self.mat_assign.is_empty(), egui::Button::new("Clear all")).clicked() {
                    self.mat_assign.clear();
                    changed = true;
                }
            });
            let mut del = None;
            let mut up = None;
            egui::ScrollArea::vertical().id_salt("matassign").max_height(ui.available_height().max(80.0)).show(ui, |ui| {
                for (k, a) in self.mat_assign.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        changed |= ui.checkbox(&mut a.enabled, "").changed();
                        ui.add_sized([130.0, 18.0], egui::Label::new(a.target.label()).truncate());
                        let names: Vec<String> = self.materials.iter().map(|m| m.name.clone()).collect();
                        egui::ComboBox::from_id_salt(("am", k)).width(120.0).selected_text(a.material.clone()).show_ui(ui, |ui| {
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
            });
            if let Some(k) = del {
                self.mat_assign.remove(k);
                changed = true;
            }
            if let Some(k) = up {
                self.mat_assign.swap(k, k - 1);
                changed = true;
            }
        });

        if let Some(p) = pick_tex {
            self.request_path(p);
        }
        if let Some(p) = io {
            self.request_path(p);
        }
        if changed {
            self.materials_changed();
        }
    }
}
