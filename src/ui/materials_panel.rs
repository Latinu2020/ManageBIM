//! The Materials window, reorganized: library, editor, PBR maps, OpenPBR parameters,
//! assignments and import / export (.bimmat, .mtlx MaterialX, PBR zip, images).
use super::*;

fn section(ui: &mut egui::Ui, title: &str, tip: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(crate::i18n::tr(title).to_uppercase()).size(11.0).strong().color(accent())).on_hover_text(tip);
    ui.add_space(2.0);
}

impl App {
    pub(super) fn materials_ui(&mut self, ui: &mut egui::Ui) {
        if self.mats.sel >= self.materials.len() {
            self.mats.sel = 0;
        }
        let mut changed = false;
        let mut pick_tex: Option<Purpose> = None;
        let mut io: Option<Purpose> = None;

        // the assignment dialog after importing a PBR package
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

        // ---- the professional library (ambientCG, CC0): progress and completion
        self.pro_library_ui(ui);
        // ---- toolbar
        ui.horizontal_wrapped(|ui| {
            if ui.small_button("➕ New").clicked() {
                let mut m = self.materials.get(self.mats.sel).cloned().unwrap_or_else(|| crate::materials::library()[0].clone());
                m.name = format!("Material {}", self.materials.len() + 1);
                self.materials.push(m);
                self.mats.sel = self.materials.len() - 1;
            }
            if ui.small_button("Duplicate").clicked() {
                if let Some(m) = self.materials.get(self.mats.sel).cloned() {
                    let mut m = m;
                    m.name.push_str(" copy");
                    self.materials.push(m);
                    self.mats.sel = self.materials.len() - 1;
                }
            }
            if ui.add_enabled(self.materials.len() > 1, egui::Button::new("🗑").small()).clicked() {
                let n = self.materials.remove(self.mats.sel).name;
                self.mats.assign.retain(|a| a.material != n);
                self.mats.sel = 0;
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
                if ui.button("★ Save selected to my library").clicked() {
                    let i = self.mats.sel;
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
                if ui.button("📥 Import MaterialX (.mtlx)…").on_hover_text("Standard MaterialX files (ambientCG, GPUOpen, Substance, Blender): OpenPBR, standard_surface, UsdPreviewSurface, node graphs and image maps. Imported materials and their maps go to your library.").clicked() {
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

        // three columns over the full panel height: the list (wide), the editor (with its own scrolling), the assignments
        self.mat_thumbs_poll(ui.ctx());
        let avail = ui.available_rect_before_wrap();
        let h = avail.height().max(220.0);
        let w = avail.width();
        let gap = 12.0;
        let narrow = w < 820.0;
        let list_w = if narrow { (w * 0.42).max(200.0) } else { (w * 0.34).clamp(260.0, 520.0) };
        let asg_w = if narrow { 0.0 } else { (w * 0.27).clamp(230.0, 440.0) };
        let ed_w = (w - list_w - asg_w - if narrow { gap } else { 2.0 * gap }).max(180.0);
        let r0 = egui::Rect::from_min_size(avail.min, egui::vec2(list_w, h));
        // narrow panel: the assignments below the editor (60 / 40 of the height)
        let (r1, r2) = if narrow {
            let x = r0.max.x + gap;
            let eh = (h * 0.6).floor();
            (egui::Rect::from_min_size(egui::pos2(x, avail.min.y), egui::vec2(ed_w, eh)), egui::Rect::from_min_size(egui::pos2(x, avail.min.y + eh + 8.0), egui::vec2(ed_w, h - eh - 8.0)))
        } else {
            let r1 = egui::Rect::from_min_size(egui::pos2(r0.max.x + gap, avail.min.y), egui::vec2(ed_w, h));
            (r1, egui::Rect::from_min_size(egui::pos2(r1.max.x + gap, avail.min.y), egui::vec2(asg_w, h)))
        };
        let sep = ui.visuals().widgets.noninteractive.bg_stroke;
        ui.painter().vline(r0.max.x + gap / 2.0, avail.min.y..=avail.min.y + h, sep);
        if !narrow {
            ui.painter().vline(r1.max.x + gap / 2.0, avail.min.y..=avail.min.y + h, sep);
        }
        let mk = |ui: &mut egui::Ui, r: egui::Rect| ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::top_down(egui::Align::Min)));
        let mut c0 = mk(ui, r0);
        let mut c1 = mk(ui, r1);
        let mut c2 = mk(ui, r2);
        ui.allocate_rect(egui::Rect::from_min_size(avail.min, egui::vec2(w, h)), egui::Sense::hover());
        {
            // ---- list
            let ui = &mut c0;
            ui.horizontal(|ui| {
                section(ui, "Library", "Click a material to edit it");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.mats.filter).hint_text(crate::i18n::tr("Filter…")).desired_width(130.0));
                });
            });
            let mine = self.user_library_names();
            let filter = self.mats.filter.to_lowercase();
            egui::ScrollArea::vertical().id_salt("matlib").auto_shrink([false, false]).max_height(ui.available_height()).show(ui, |ui| {
                for i in 0..self.materials.len() {
                    if !filter.is_empty() && !self.materials[i].name.to_lowercase().contains(&filter) {
                        continue;
                    }
                    let in_lib = mine.contains(&self.materials[i].name);
                    if self.mat_row(ui, i, in_lib) {
                        self.mats.sel = i;
                    }
                }
            });
        }

        // ---- editor
        egui::ScrollArea::vertical().id_salt("mated_scroll").auto_shrink([false, false]).max_height(r1.height()).show(&mut c1, |ui| {
            let i = self.mats.sel;
            if i >= self.materials.len() {
                return;
            }
            let old_name = self.materials[i].name.clone();
            // save: to your library (all projects) or as a file (.mtlx with the textures next to it)
            let in_lib = self.user_library_names().contains(&old_name);
            ui.horizontal_wrapped(|ui| {
                let (txt, tip) = if in_lib {
                    ("✔ In my library", "Saved in your material library; edits are saved there automatically. Click to save again now.")
                } else {
                    ("★ Save to my library", "Keep this material (with its texture maps) in your library, available in every project")
                };
                if ui.button(RichText::new(crate::i18n::tr(txt)).strong()).on_hover_text(crate::i18n::tr(tip)).clicked() {
                    self.save_material_to_library(i);
                }
                if ui.button(crate::i18n::tr("💾 Save as file…")).on_hover_text(crate::i18n::tr("Save this material as a MaterialX file (.mtlx) with its texture maps copied next to it - for another computer, a colleague or Blender / Maya / Houdini / Omniverse")).clicked() {
                    io = Some(Purpose::ExportMaterialOne);
                }
            });
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

            // the changed name is applied to the rules after we finish editing the material
            let new_name = m.name.clone();
            // preview
            let mat = &self.materials[self.mats.sel];
            let tex = if !mat.texture.is_empty() && self.tex_loaded_res == crate::materials::tex_size() {
                self.tex_layers.iter().find(|(k, _)| k == &mat.texture).map(|(_, px)| px.clone())
            } else {
                None
            };
            let need = self.mats.preview.as_ref().map(|(p, _)| p != &mat.texture).unwrap_or(true);
            if !mat.texture.is_empty() && need {
                if let Some(px) = tex {
                    let n = crate::materials::tex_size() as usize;
                    let img = eframe::egui::ColorImage::from_rgba_unmultiplied([n, n], &px);
                    let t = ui.ctx().load_texture(format!("mat_preview_{}", mat.texture), img, egui::TextureOptions::LINEAR);
                    self.mats.preview = Some((mat.texture.clone(), t));
                }
            }
            if let Some((p, t)) = &self.mats.preview {
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
                for a in self.mats.assign.iter_mut().filter(|a| a.material == old_name) {
                    a.material = new_name.clone();
                }
                changed = true;
            }

        });

        {
            // ---- assignments (the right column; below the editor when the panel is narrow)
            let ui = &mut c2;
            section(ui, "Assignments", "Rules lower in the list win");
            let mname = self.materials.get(self.mats.sel).map(|m| m.name.clone()).unwrap_or_default();
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
                if ui.add_enabled(!self.mats.assign.is_empty(), egui::Button::new("Clear all")).clicked() {
                    self.mats.assign.clear();
                    changed = true;
                }
            });
            let mut del = None;
            let mut up = None;
            egui::ScrollArea::vertical().id_salt("matassign").max_height(ui.available_height().max(80.0)).show(ui, |ui| {
                for (k, a) in self.mats.assign.iter_mut().enumerate() {
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
                self.mats.assign.remove(k);
                changed = true;
            }
            if let Some(k) = up {
                self.mats.assign.swap(k, k - 1);
                changed = true;
            }
        }

        if let Some(p) = pick_tex {
            self.request_path(p);
        }
        if let Some(p) = io {
            self.request_path(p);
        }
        if changed {
            self.materials_changed();
        }
        // changes to a material in your library are saved there too (~1 s after the last change,
        // so we do not write the file at every step of a slider)
        let key = egui::Id::new("mat_lib_autosave");
        let now = ui.input(|i| i.time);
        if changed {
            if let Some(m) = self.materials.get(self.mats.sel) {
                let name = m.name.clone();
                if self.user_library_names().contains(&name) {
                    ui.data_mut(|d| d.insert_temp(key, (name, now)));
                }
            }
        }
        if let Some((name, t)) = ui.data(|d| d.get_temp::<(String, f64)>(key)) {
            if now - t > 1.0 {
                ui.data_mut(|d| d.remove::<(String, f64)>(key));
                if let Some(m) = self.materials.iter().find(|m| m.name == name).cloned() {
                    let _ = crate::materials::save_to_user_library(std::slice::from_ref(&m));
                    self.user_lib_cache = None;
                }
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
            }
        }
    }
}

impl App {
    /// The professional material library (ambientCG scanned textures, CC0): download with progress,
    /// then the materials replace the bundled variants with the same name and are applied by IFC type.
    fn pro_library_ui(&mut self, ui: &mut egui::Ui) {
        // completion
        let finished = self.pro_lib_job.as_ref().map(|j| j.lock().unwrap_or_else(|e| e.into_inner()).finished).unwrap_or(false);
        if finished {
            let job = self.pro_lib_job.take().unwrap();
            let g = job.lock().unwrap_or_else(|e| e.into_inner());
            let mats = g.installed.clone();
            let errors = g.errors.clone();
            drop(g);
            if !mats.is_empty() {
                if let Err(e) = crate::materials::save_to_user_library(&mats) {
                    self.error(format!("Materials downloaded but the library could not be saved: {e}"));
                }
                for m in &mats {
                    match self.materials.iter_mut().find(|x| x.name == m.name) {
                        Some(x) => *x = m.clone(),
                        None => self.materials.push(m.clone()),
                    }
                }
                if self.mats.assign.is_empty() {
                    self.auto_materials();
                }
                self.materials_on = true;
                self.materials_changed();
                self.toast(format!("{} professional PBR materials installed - they replace the built-in ones and are used by \"Auto by IFC type\".", mats.len()));
            }
            if !errors.is_empty() {
                self.error(format!("{} material(s) could not be downloaded:\n{}", errors.len(), errors.join("\n")));
            }
        }
        let frame = egui::Frame::none().fill(accent().gamma_multiply(0.10)).rounding(crate::theme::RADIUS_SM).inner_margin(egui::Margin::symmetric(8.0, 6.0));
        frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            if let Some(job) = self.pro_lib_job.clone() {
                let (done, total, cur) = {
                    let g = job.lock().unwrap_or_else(|e| e.into_inner());
                    (g.done, g.total.max(1), g.current.clone())
                };
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(format!("Downloading professional materials… {done} / {total}  ·  {cur}")).small());
                    if ui.small_button("Cancel").clicked() {
                        job.lock().unwrap_or_else(|e| e.into_inner()).cancel = true;
                    }
                });
                ui.add(egui::ProgressBar::new(done as f32 / total as f32).desired_height(6.0));
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
            } else {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("✨ Professional PBR library").strong().color(accent()));
                    let included = crate::materials::bundled_library_dir().is_some();
                    ui.label(RichText::new(if included { format!("✓ {} scanned materials included (concrete, brick, plaster, metals, wood, tiles, roof, stone, grass…) - ambientCG, CC0. Download 2K for sharper close-ups.", crate::pro_materials::PRO_LIBRARY.len()) } else { format!("{} scanned materials (concrete, brick, plaster, metals, wood, tiles, roof, stone, grass…) from ambientCG - CC0, free for commercial use.", crate::pro_materials::PRO_LIBRARY.len()) }).small().weak());
                    for (res, size, tip) in [("1K", "~85 MB", "1024 px - fast, good for the 3D view"), ("2K", "~500 MB", "2048 px - sharper close-ups and print renders (set Settings › Appearance › Texture size to 2048)")] {
                        if included && res == "1K" {
                            continue;
                        }
                        if ui.button(RichText::new(format!("⬇ Install {res}  ({size})")).strong()).on_hover_text(format!("{tip}. Downloads the maps (colour, normal, roughness, metallic, AO, height) into your material library; materials with the same name replace the built-in ones.")).clicked() {
                            match crate::pro_materials::spawn_install(if res == "1K" { "1K" } else { "2K" }) {
                                Ok(j) => self.pro_lib_job = Some(j),
                                Err(e) => self.error(e),
                            }
                        }
                    }
                });
            }
        });
        ui.add_space(3.0);
    }
}

// ---------------------------------------------------------------- list with thumbnails

impl App {
    /// The thumbnails finished by the worker thread become egui textures.
    pub(super) fn mat_thumbs_poll(&mut self, ctx: &egui::Context) {
        let mut done = Vec::new();
        if let Some((_, rx)) = &self.mats.thumb_job {
            while let Ok(r) = rx.try_recv() {
                done.push(r);
            }
        }
        for (path, t) in done {
            let tex = t.map(|t| {
                let img = egui::ColorImage::from_rgba_unmultiplied([t.size, t.size], &t.rgba);
                ctx.load_texture(format!("mat_thumb_{path}"), img, egui::TextureOptions::LINEAR)
            });
            self.mats.thumbs.insert(path, tex);
        }
    }

    /// The texture thumbnail (requested from the worker thread on the first call; None until then).
    fn mat_thumb(&mut self, tex: &str) -> Option<egui::TextureHandle> {
        if tex.is_empty() {
            return None;
        }
        let path = self.texture_path(tex);
        if let Some(t) = self.mats.thumbs.get(&path) {
            return t.clone();
        }
        if self.mats.thumb_job.is_none() {
            let (tx, rx) = std::sync::mpsc::channel::<String>();
            let (otx, orx) = std::sync::mpsc::channel();
            std::thread::Builder::new()
                .name("mat-thumbs".into())
                .spawn(move || {
                    while let Ok(p) = rx.recv() {
                        let t = std::panic::catch_unwind(|| crate::materials::thumbnail(&p).ok()).ok().flatten();
                        if otx.send((p, t)).is_err() {
                            break;
                        }
                    }
                })
                .ok();
            self.mats.thumb_job = Some((tx, orx));
        }
        // marked "in progress" (None) so it is not requested again every frame
        self.mats.thumbs.insert(path.clone(), None);
        if let Some((tx, _)) = &self.mats.thumb_job {
            let _ = tx.send(path);
        }
        None
    }

    /// A list row: thumbnail (texture or color), name, what it contains; returns true on click.
    fn mat_row(&mut self, ui: &mut egui::Ui, i: usize, in_lib: bool) -> bool {
        let (tex, name, sub, color, used, accent_name) = {
            let m = &self.materials[i];
            let maps = [&m.texture, &m.normal_map, &m.roughness_map, &m.metallic_map, &m.ao_map, &m.height_map].iter().filter(|p| !p.is_empty()).count();
            let mut sub = if maps > 0 { format!("PBR · {maps} map{}", if maps == 1 { "" } else { "s" }) } else { "Colour".to_string() };
            if m.metal > 40 {
                sub.push_str(" · metal");
            }
            if m.coat > 0 {
                sub.push_str(" · coat");
            }
            if m.transparency > 0 {
                sub.push_str(&format!(" · {}% transp.", m.transparency));
            }
            let used = self.mats.assign.iter().filter(|a| a.material == m.name).count();
            (m.texture.clone(), m.name.clone(), sub, m.color, used, m.is_pbr() || m.coat > 0 || m.sheen > 0)
        };
        let thumb = self.mat_thumb(&tex);
        let row_h = 46.0;
        let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), egui::Sense::click());
        let sel = i == self.mats.sel;
        let vis = ui.visuals();
        if sel {
            ui.painter().rect_filled(r, 4.0, vis.selection.bg_fill);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 4.0, vis.widgets.hovered.weak_bg_fill);
        }
        let t = egui::Rect::from_min_size(r.min + egui::vec2(4.0, 3.0), egui::vec2(40.0, 40.0));
        match &thumb {
            Some(h) => {
                egui::Image::new((h.id(), t.size())).rounding(4.0).paint_at(ui, t);
            }
            None => {
                // the color, with a simple sphere shading
                let c = Color32::from_rgb(color[0], color[1], color[2]);
                ui.painter().rect_filled(t, 4.0, c);
                ui.painter().circle_filled(t.center() + egui::vec2(-7.0, -7.0), 7.0, Color32::from_white_alpha(45));
                if !tex.is_empty() {
                    ui.painter().text(t.center(), Align2::CENTER_CENTER, egui_phosphor::regular::IMAGE, FontId::proportional(14.0), Color32::from_black_alpha(140));
                }
            }
        }
        ui.painter().rect_stroke(t, 4.0, Stroke::new(1.0_f32, Color32::from_gray(110)));
        let tx = t.max.x + 8.0;
        let name_col = if sel { vis.selection.stroke.color } else if accent_name { accent() } else { vis.text_color() };
        let mut x_end = r.max.x - 6.0;
        if in_lib {
            ui.painter().text(egui::pos2(x_end, r.center().y), Align2::RIGHT_CENTER, "★", FontId::proportional(14.0), accent());
            x_end -= 18.0;
        }
        let font = FontId::proportional(14.0);
        let galley = ui.painter().layout(name.clone(), font, name_col, (x_end - tx).max(20.0));
        let name_h = galley.size().y.min(18.0);
        ui.painter().with_clip_rect(egui::Rect::from_min_max(egui::pos2(tx, r.min.y), egui::pos2(x_end, r.max.y))).galley(egui::pos2(tx, r.min.y + 6.0), galley, name_col);
        let sub = if used > 0 { format!("{sub} · used ×{used}") } else { sub };
        ui.painter().with_clip_rect(egui::Rect::from_min_max(egui::pos2(tx, r.min.y), egui::pos2(x_end, r.max.y))).text(egui::pos2(tx, r.min.y + 8.0 + name_h), Align2::LEFT_TOP, sub, FontId::proportional(11.5), vis.weak_text_color());
        if sel && self.mats.scroll {
            resp.scroll_to_me(Some(egui::Align::Center));
            self.mats.scroll = false;
        }
        let resp = if in_lib { resp.on_hover_text("★ In your material library - available in every project; changes are saved there too") } else { resp };
        resp.clicked()
    }
}

impl App {
    /// The Materials window: takes the height of the app window (below the ribbon) and ~80% of the width;
    /// it can be moved and resized, and the list uses the full height.
    pub(super) fn materials_window(&mut self, ctx: &egui::Context) {
        if !self.mats.window {
            return;
        }
        let screen = ctx.screen_rect();
        let top = 150.0f32.min(screen.height() * 0.2);
        let h = (screen.height() - top - 40.0).max(420.0);
        let w = (screen.width() * 0.8).clamp(720.0, 1600.0);
        let mut open = true;
        egui::Window::new(crate::theme::icon_job(egui_phosphor::regular::PALETTE, crate::i18n::tr("Materials"), 15.0, ctx.style().visuals.strong_text_color()))
            .id(egui::Id::new(("materials_window", screen.height() as i32 / 50)))
            .open(&mut open)
            .default_pos(egui::pos2((screen.width() - w) / 2.0, top))
            .default_size([w, h])
            .min_width(560.0)
            .min_height(320.0)
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                // the content takes the whole window size (resized by the user)
                let r = ui.available_rect_before_wrap();
                ui.set_min_size(r.size());
                self.materials_ui(ui);
            });
        self.mats.window = open;
    }
}
