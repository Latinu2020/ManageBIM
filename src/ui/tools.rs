//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    // ------------------------------------------------------------------ sectiune
    pub(super) fn init_section_defaults(&mut self) {
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

    pub(super) fn section_to_selection(&mut self) {
        if let Some(bb) = self.section_box_on_selection() {
            let mut t = self.cam;
            t.fit(&bb);
            self.fly_to(t);
        }
    }

    /// Section box in jurul selectiei (cu o margine mica); intoarce cutia elementelor.
    pub(super) fn section_box_on_selection(&mut self) -> Option<crate::model::Aabb> {
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
    pub(super) fn section_follow_tick(&mut self) {
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

    pub(super) fn section_ui(&mut self, ui: &mut egui::Ui) {
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
            ui.checkbox(&mut self.show_section_plane, "Show plane").on_hover_text("Draw the section plane as a transparent green surface");
        });
        // sectiune dupa nivel: un nivel intreg (cu margini sub / deasupra), taietura de plan sau doar elementele nivelului
        let levels = self.scene_grids().levels;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("By level").strong());
            if levels.is_empty() {
                ui.label(RichText::new("no levels in the models - add them in View › Edit Levels").weak());
                return;
            }
            let mut li = self.section_level.unwrap_or(0).min(levels.len() - 1);
            egui::ComboBox::from_id_salt("seclev").width(150.0).selected_text(format!("{}  {:+.2}", levels[li].0, levels[li].1)).show_ui(ui, |ui| {
                for (i, l) in levels.iter().enumerate().rev() {
                    ui.selectable_value(&mut li, i, format!("{}  {:+.2}", l.0, l.1));
                }
            });
            if Some(li) != self.section_level {
                self.section_level = Some(li);
            }
            if ui.small_button("▼").on_hover_text("Level below (PgDn)").clicked() {
                self.section_level_step(-1);
            }
            if ui.small_button("▲").on_hover_text("Level above (PgUp)").clicked() {
                self.section_level_step(1);
            }
            if ui.button("▣ Level box").on_hover_text("Section box from this level to the next one").clicked() {
                self.section_level_box(li);
            }
            if ui.button("✂ Plan cut").on_hover_text("Cut everything above the height below (like a floor plan)").clicked() {
                self.section_plan_cut(li, self.plan_cut_h);
            }
            if ui.button("👁 Only this level").on_hover_text("Show only the items of this level (by the level name in the models, otherwise by height); Unhide All to undo").clicked() {
                self.isolate_level(li);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Plan cut height").weak());
            ui.add(egui::DragValue::new(&mut self.plan_cut_h).speed(0.05).range(-5.0..=20.0).fixed_decimals(2).suffix(" m"));
            ui.label(RichText::new("Level box: below the level").weak());
            ui.add(egui::DragValue::new(&mut self.level_below).speed(0.02).range(0.0..=5.0).fixed_decimals(2).suffix(" m")).on_hover_text("e.g. 0.30 to include the slab under the floor");
            ui.label(RichText::new("past the next level").weak());
            ui.add(egui::DragValue::new(&mut self.level_above).speed(0.02).range(-5.0..=5.0).fixed_decimals(2).suffix(" m")).on_hover_text("negative = stop under the next slab, positive = include it");
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
                let h = (self.section.bmax[2] - self.section.bmin[2]).max(0.02);
                ui.horizontal(|ui| {
                    ui.label("Cut height in box");
                    if ui.add(egui::Slider::new(&mut self.settings.section_cut_offset, 0.05..=h).suffix(" m").fixed_decimals(2)).changed() {
                        self.settings.save();
                    }
                })
                .response
                .on_hover_text("The orange highlighted plane AND the DWG/PDF cut are taken from here. Stretch the Z faces of the box and the section follows.");
            }
        }
    }

    // ------------------------------------------------------------------ masuratori
    pub(super) fn measure_ui(&mut self, ui: &mut egui::Ui) {
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
    pub(super) fn fmt_time(t: std::time::SystemTime) -> String {
        let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
        let (y, m, d) = timeline::civil_from_days((secs / 86400) as i32);
        let s = secs % 86400;
        format!("{y}-{m:02}-{d:02} {:02}:{:02} UTC", s / 3600, (s % 3600) / 60)
    }

    pub(super) fn fmt_xyz(v: glam::DVec3) -> String {
        format!("{:.3}, {:.3}, {:.3}", v.x, v.y, v.z)
    }

    pub(super) fn open_folder(path: &str) {
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("explorer.exe").arg(format!("/select,{}", path)).spawn();
        }
        #[cfg(not(windows))]
        {
            if let Some(dir) = std::path::Path::new(path).parent() {
                crate::formats::open_with_system(dir);
            }
        }
    }

    pub(super) fn models_ui(&mut self, ui: &mut egui::Ui) {
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
    pub(super) fn coords_ui(&mut self, ui: &mut egui::Ui) {
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
    pub(super) fn swatch(ui: &mut egui::Ui, m: &crate::materials::Material) {
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

    pub(super) fn materials_ui(&mut self, ui: &mut egui::Ui) {
        if self.mat_sel >= self.materials.len() {
            self.mat_sel = 0;
        }
        let mut changed = false;
        let mut pick_tex: Option<Purpose> = None;
        let mut lib_io: Option<Purpose> = None;
        // fereastra de asignare dupa importul unui pachet PBR
        if !self.pending_mat_assign.is_empty() {
            let names = self.pending_mat_assign.clone();
            let mut choice: Option<u8> = None;
            egui::Window::new("🎨 PBR material imported")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ui.ctx(), |ui| {
                    ui.label(RichText::new(format!("Where should {} be applied?", names.join(", "))).strong());
                    ui.label(RichText::new("You can change this later in Assignments. The material is already in your library.").small().weak());
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(!self.selected.is_empty(), egui::Button::new(format!("🎯 Selection ({})", self.selected.len()))).clicked() {
                            choice = Some(0);
                        }
                        if ui.button("🌐 All visible items").clicked() {
                            choice = Some(1);
                        }
                        if ui.button("🏗 By IFC type…").on_hover_text("Opens the IFC type menu in Assignments").clicked() {
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
                if ui.small_button("📦 PBR pack…").on_hover_text("Import a PBR pack (.zip from freepbr.com, ambientCG, Poly Haven...) or an already unzipped folder. All maps are kept: albedo, normal, roughness, metallic, AO and height. You choose where to apply it after import.").clicked() {
                    lib_io = Some(Purpose::ImportPbr);
                }
                if ui.small_button("📁 PBR folder…").on_hover_text("Import an already unzipped PBR folder").clicked() {
                    lib_io = Some(Purpose::ImportPbrFolder);
                }
                ui.menu_button("Library ⏷", |ui| {
                    if ui.button("📦 Import PBR pack (.zip)…").clicked() {
                        lib_io = Some(Purpose::ImportPbr);
                        ui.close_menu();
                    }
                    if ui.button("📁 Import PBR folder (already unzipped)…").clicked() {
                        lib_io = Some(Purpose::ImportPbrFolder);
                        ui.close_menu();
                    }
                    if ui.button("⭐ Save selected material to my library").on_hover_text("Keep this material for all your projects").clicked() {
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
                    for m in crate::materials::full_library() {
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
                                    pick_tex = Some(Purpose::MaterialTexture);
                                }
                            });
                            ui.end_row();
                            // ---- hartile PBR (freepbr.com, ambientCG, Poly Haven)
                            let is_pbr = m.is_pbr();
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
                                    if ui.small_button("📁").on_hover_text(format!("Choose the {label} image (PNG / JPG)")).clicked() {
                                        pick_tex = Some(purpose);
                                    }
                                });
                                ui.end_row();
                            };
                            map_row(ui, "Normal", &mut m.normal_map, Purpose::MaterialNormal);
                            map_row(ui, "Roughness", &mut m.roughness_map, Purpose::MaterialRoughness);
                            map_row(ui, "Metallic", &mut m.metallic_map, Purpose::MaterialMetallic);
                            map_row(ui, "AO", &mut m.ao_map, Purpose::MaterialAo);
                            map_row(ui, "Height", &mut m.height_map, Purpose::MaterialHeight);
                            if is_pbr {
                                ui.label(RichText::new("Normal strength").small());
                                changed |= ui.add(egui::Slider::new(&mut m.normal_strength, 0.0..=2.0).step_by(0.05).suffix("×")).on_hover_text("How strong the surface relief looks (0 = flat, 1 = normal, 2 = exaggerated)").changed();
                                ui.end_row();
                                ui.label(RichText::new("Roughness ×").small());
                                changed |= ui.add(egui::Slider::new(&mut m.rough_mult, 0.0..=2.0).step_by(0.05).suffix("×")).on_hover_text("Multiply the roughness map: below 1 = shinier, above 1 = more matte").changed();
                                ui.end_row();
                                ui.label(RichText::new("Metallic ×").small());
                                changed |= ui.add(egui::Slider::new(&mut m.metal_mult, 0.0..=2.0).step_by(0.05).suffix("×")).on_hover_text("Multiply the metallic map: below 1 = less metal, above 1 = more metal").changed();
                                ui.end_row();
                                if ui.button("↺ Reset PBR strength").clicked() {
                                    m.normal_strength = 1.0;
                                    m.rough_mult = 1.0;
                                    m.metal_mult = 1.0;
                                    changed = true;
                                }
                                ui.end_row();
                            }
                        });
                        if m.name != old_name {
                            let new = m.name.clone();
                            for a in self.mat_assign.iter_mut().filter(|a| a.material == old_name) {
                                a.material = new.clone();
                            }
                            changed = true;
                        }
                        // previzualizarea hartii de culoare ( imaginea reala, nu doar un swatch )
                        let mat = &self.materials[self.mat_sel];
                        let tex = if (!mat.texture.is_empty() || mat.is_pbr()) && self.tex_loaded_res == crate::materials::tex_size() {
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
                                ui.add_space(4.0);
                                ui.label(RichText::new("Preview (albedo)").small().weak());
                                let s = 96.0_f32;
                                ui.image((t.id(), egui::vec2(s, s)));
                            }
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
        if let Some(p) = pick_tex {
            self.request_path(p);
        }
        if let Some(p) = lib_io {
            self.request_path(p);
        }
        if changed {
            self.materials_changed();
        }
    }

    pub(super) fn tools_window(&mut self, ctx: &egui::Context) {
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
            // versiunea Navisworks folosita la NWD / NWC / RVT (cand sunt instalate mai multe)
            let years = crate::formats::navis_years();
            if !years.is_empty() {
                ui.horizontal(|ui| {
                    ui.label("Navisworks version");
                    let cur = if t.navis_year == 0 { format!("Automatic ({})", years.first().map(|y| y.to_string()).unwrap_or_default()) } else { t.navis_year.to_string() };
                    egui::ComboBox::from_id_salt("navis_year").selected_text(cur).show_ui(ui, |ui| {
                        ui.selectable_value(&mut t.navis_year, 0, "Automatic (newest supported, then the others)");
                        let mut sorted = years.clone();
                        sorted.sort_unstable();
                        for y in sorted {
                            let label = if y > crate::formats::LAST_SUPPORTED_NAVIS { format!("{y} (not supported yet)") } else { y.to_string() };
                            ui.selectable_value(&mut t.navis_year, y, label);
                        }
                    });
                })
                .response
                .on_hover_text("Used to open NWD / NWC / NWF / RVT. If it fails, the other installed versions are tried automatically.");
            }
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

    // ================================================================== ferestre
    /// Fereastra "As-built check": modelul fata de norul de puncte.
    pub(super) fn asbuilt_window(&mut self, ctx: &egui::Context) {
        use crate::asbuilt::Status;
        if !self.asbuilt_open {
            return;
        }
        let mut open = true;
        let mut acts: Vec<Action> = Vec::new();
        let mut run = false;
        let mut colors: Option<bool> = None;
        egui::Window::new(crate::theme::icon_job("☁", "As-built check  (model vs point cloud)", 14.0, ctx.style().visuals.text_color()))
            .id(egui::Id::new("asbuilt_win"))
            .open(&mut open)
            .default_size([520.0, 440.0])
            .resizable(true)
            .show(ctx, |ui| {
                let clouds = self.loaded_models().filter(|(_, m)| m.cloud.is_some()).count();
                let designs = self.loaded_models().filter(|(_, m)| m.cloud.is_none()).count();
                ui.label(RichText::new(format!("{clouds} point cloud(s) · {designs} model(s). Every visible element is sampled on its surface; it counts as built when scan points cover enough of it.")).small().weak());
                ui.horizontal(|ui| {
                    ui.label("Tolerance");
                    let mut mm = self.asbuilt_params.tol * 1000.0;
                    if ui.add(egui::DragValue::new(&mut mm).range(5.0..=500.0).speed(1.0).suffix(" mm")).on_hover_text("Maximum distance between the scan points and the model surface").changed() {
                        self.asbuilt_params.tol = mm / 1000.0;
                    }
                    ui.label("Built ≥");
                    let mut b = self.asbuilt_params.built * 100.0;
                    if ui.add(egui::DragValue::new(&mut b).range(5.0..=100.0).speed(1.0).suffix(" %")).on_hover_text("Coverage for 'Built' (hidden faces are never scanned, so this is low)").changed() {
                        self.asbuilt_params.built = b / 100.0;
                    }
                    ui.label("Partial ≥");
                    let mut pp = self.asbuilt_params.partial * 100.0;
                    if ui.add(egui::DragValue::new(&mut pp).range(1.0..=100.0).speed(1.0).suffix(" %")).changed() {
                        self.asbuilt_params.partial = pp / 100.0;
                    }
                });
                ui.horizontal(|ui| {
                    run = ui.add_enabled(clouds > 0 && designs > 0, egui::Button::new(RichText::new("▶ Run check").strong().color(Color32::WHITE)).fill(accent())).clicked();
                    let has = self.asbuilt.is_some();
                    let mut show = self.asbuilt_show;
                    if ui.add_enabled(has, egui::Checkbox::new(&mut show, "Colour the model")).changed() {
                        colors = Some(show);
                    }
                    if ui.add_enabled(has, egui::Button::new("📊 Excel…")).clicked() {
                        self.request_path(Purpose::ExportAsBuilt);
                    }
                    if ui.add_enabled(has, egui::Button::new("⚠ Add scan clashes")).on_hover_text("Send Missing / Partial items to Clash Detective as a 'Scan vs Model' test").clicked() {
                        self.add_scan_clashes();
                    }
                });
                let Some(res) = &self.asbuilt else {
                    if clouds == 0 {
                        ui.label(RichText::new("Open the scan (E57 / LAS) and Append the design model (or the other way round), then Run check.").weak());
                    }
                    return;
                };
                ui.separator();
                // rezumat pe stari: clic = filtru, butoane select / isolate
                ui.horizontal_wrapped(|ui| {
                    for s in Status::ALL {
                        let n = res.iter().filter(|r| r.status == s).count();
                        let c = s.color();
                        let on = self.asbuilt_filter == Some(s);
                        let (dr, _) = ui.allocate_exact_size(EV::splat(10.0), Sense::hover());
                        ui.painter().circle_filled(dr.center(), 4.5, Color32::from_rgb(c[0], c[1], c[2]));
                        if ui.selectable_label(on, format!("{} {}", s.label(), n)).clicked() {
                            self.asbuilt_filter = if on { None } else { Some(s) };
                        }
                    }
                });
                let shown: Vec<&crate::asbuilt::ElemResult> = res.iter().filter(|r| self.asbuilt_filter.map(|f| f == r.status).unwrap_or(true)).collect();
                let ids: Vec<ElemId> = shown.iter().map(|r| r.id).collect();
                ui.horizontal(|ui| {
                    if ui.small_button("Select").clicked() {
                        acts.push(Action::Select(ids.clone(), false));
                    }
                    if ui.small_button("Isolate").clicked() {
                        acts.push(Action::Isolate(ids.clone()));
                    }
                    ui.label(RichText::new(format!("{} elements", shown.len())).small().weak());
                });
                egui::ScrollArea::vertical().id_salt("asb_list").auto_shrink([false, false]).show_rows(ui, 21.0, shown.len().min(5000), |ui, range| {
                    for r in &shown[range] {
                        let (name, ty) = self.elem(r.id).map(|(_, e)| (e.name.clone(), e.ifc_type.clone())).unwrap_or_default();
                        let c = r.status.color();
                        let label = format!("{name}  ·  {ty}  ·  {:.0}%", r.coverage * 100.0);
                        let resp = tree_row(ui, &label, None, Some(Color32::from_rgb(c[0], c[1], c[2])), self.selected.contains(&r.id), false, false);
                        if resp.clicked() {
                            acts.push(Action::Select(vec![r.id], ui.input(|i| i.modifiers.ctrl)));
                        }
                        if resp.double_clicked() {
                            acts.push(Action::Focus(vec![r.id]));
                        }
                    }
                });
            });
        if !open {
            self.asbuilt_open = false;
        }
        if let Some(on) = colors {
            self.asbuilt_colors(on);
        }
        if run {
            self.run_asbuilt();
        }
        for a in acts {
            self.apply(a);
        }
    }
}
