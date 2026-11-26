//! IFC Mapping window (BIM › Map classes): which IFC class and type the non-IFC categories
//! (STEP, OBJ, glTF, DWG, Revit categories...) get on IFC export, and the MVD. Moved out of Settings,
//! which now holds only application preferences.
use super::*;

impl App {
    pub(super) fn ifc_mapping_window(&mut self, ctx: &egui::Context) {
        if !self.ifc_map_open {
            return;
        }
        let mut open = true;
        let mut io: Option<Purpose> = None;
        egui::Window::new(format!("{} IFC Mapping", egui_phosphor::regular::TREE_STRUCTURE))
            .open(&mut open)
            .default_size([620.0, 460.0])
            .min_width(480.0)
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(RichText::new("On IFC export, elements that are not IFC (STEP, OBJ, glTF, DWG...) get the IFC class and type of the first rule whose source category matches, like the IFC export mapping in Revit.").small().weak());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("MVD").on_hover_text("Model View Definition: which properties and fields are written in the IFC file");
                    egui::ComboBox::from_id_salt("map_ifc_mvd").width(200.0).selected_text(self.settings.ifc_mvd.code()).show_ui(ui, |ui| {
                        for m in crate::ifc_mvd::Mvd::ALL {
                            if ui.selectable_value(&mut self.settings.ifc_mvd, m, m.label()).on_hover_text(m.tip()).clicked() {
                                self.settings.save();
                            }
                        }
                    });
                    ui.separator();
                    let fid = egui::Id::new("ifc_map_filter");
                    let mut filter: String = ui.data(|d| d.get_temp(fid)).unwrap_or_default();
                    ui.add(egui::TextEdit::singleline(&mut filter).hint_text("Filter rules…").desired_width(160.0));
                    ui.data_mut(|d| d.insert_temp(fid, filter));
                });
                ui.separator();
                let filter = ui.data(|d| d.get_temp::<String>(egui::Id::new("ifc_map_filter"))).unwrap_or_default().to_lowercase();
                // separator + button row below the table (with spacing); too little here makes the window creep taller
                let bottom = 56.0;
                let mut changed = false;
                let mut del: Option<usize> = None;
                egui::ScrollArea::vertical().id_salt("ifc_map_rules").max_height((ui.available_height() - bottom).clamp(120.0, 900.0)).auto_shrink([false, true]).show(ui, |ui| {
                    egui::Grid::new("ifc_map_grid_win").num_columns(4).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                        ui.strong("Source category");
                        ui.strong("IFC class");
                        ui.strong("IFC type (PredefinedType)");
                        ui.strong("");
                        ui.end_row();
                        for (i, r) in self.settings.ifc_map.iter_mut().enumerate() {
                            if !filter.is_empty() && !r.source.to_lowercase().contains(&filter) && !r.class.to_lowercase().contains(&filter) && !r.ifc_type.to_lowercase().contains(&filter) {
                                continue;
                            }
                            changed |= ui.add(egui::TextEdit::singleline(&mut r.source).desired_width(160.0)).lost_focus();
                            egui::ComboBox::from_id_salt(("icw", i)).width(200.0).selected_text(r.class.clone()).show_ui(ui, |ui| {
                                for (code, label) in crate::ifc_map::CLASSES {
                                    changed |= ui.selectable_value(&mut r.class, code.to_string(), format!("{code} — {label}")).clicked();
                                }
                            });
                            changed |= ui.add(egui::TextEdit::singleline(&mut r.ifc_type).desired_width(120.0)).lost_focus();
                            if ui.small_button("✖").on_hover_text("Delete this rule").clicked() {
                                del = Some(i);
                            }
                            ui.end_row();
                        }
                    });
                });
                if let Some(i) = del {
                    self.settings.ifc_map.remove(i);
                    changed = true;
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("➕ Add rule").clicked() {
                        self.settings.ifc_map.push(crate::ifc_map::MapRule::default());
                        changed = true;
                    }
                    if ui.button("📥 Import CSV…").on_hover_text("Load a mapping table from CSV (Source Category; IFC Class Name; IFC Type)").clicked() {
                        io = Some(Purpose::ImportIfcMap);
                    }
                    if ui.button("📤 Export CSV…").on_hover_text("Save the mapping table to CSV").clicked() {
                        io = Some(Purpose::ExportIfcMap);
                    }
                    if ui.button("↺ Default").on_hover_text("Restore the default mapping for non-IFC geometry").clicked() {
                        self.settings.ifc_map = crate::ifc_map::default_rules();
                        changed = true;
                    }
                    ui.label(RichText::new(format!("{} rules", self.settings.ifc_map.len())).weak().small());
                });
                if changed {
                    self.settings.save();
                }
            });
        if let Some(p) = io {
            self.request_path(p);
        }
        self.ifc_map_open = open;
    }
}
