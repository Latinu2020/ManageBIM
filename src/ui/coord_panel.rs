//! Fereastra "Coordination objects": lista zonelor de mentenanta, golurilor propuse si volumelor
//! rezervate, cu dimensiuni, pozitie, stare, gazda si legatura cu Clash Detective.
use super::*;
use crate::coord_objects::{CoordKind, STATUSES};
use crate::model::Aabb;

impl App {
    /// Grupul din tab-ul Coordination (ribbon).
    pub(super) fn coord_ribbon(&mut self, ui: &mut egui::Ui) {
        let has = self.loaded_models().next().is_some();
        rgroup(ui, "Coordination objects", |ui| {
            for k in CoordKind::ALL {
                let short = match k {
                    CoordKind::Clearance => "Clearance",
                    CoordKind::ProvisionForVoid => "Void",
                    CoordKind::Placeholder => "Placeholder",
                };
                if rbtn(ui, k.icon(), short, &format!("{} - {} Made around the selection, or at the centre of the view.", k.label(), k.tip()), false, has).clicked() {
                    self.add_coord_object(k);
                }
            }
            if rbtn(ui, "⚡", "Auto voids", "Propose a provision for void wherever a pipe, duct or cable tray crosses a wall or slab (only the selected services, when something is selected)", false, has).clicked() {
                self.auto_voids();
            }
            if rbtn(ui, "📋", "Objects", "Coordination objects window: sizes, position, status, host element, visibility", self.coord_open, true).clicked() {
                self.coord_open = !self.coord_open;
            }
        });
    }

    pub(super) fn coord_objects_window(&mut self, ctx: &egui::Context) {
        if !self.coord_open {
            return;
        }
        let mut open = true;
        let mut rebuild = false;
        let mut remove: Option<usize> = None;
        let mut select: Option<usize> = None;
        let mut zoom: Option<usize> = None;
        egui::Window::new(crate::theme::icon_job("🛡", "Coordination objects", 14.0, ctx.style().visuals.text_color()))
            .id(egui::Id::new("coord_win"))
            .open(&mut open)
            .default_size([640.0, 460.0])
            .min_width(480.0)
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.menu_button(RichText::new("➕ Add ⏷").strong(), |ui| {
                        for k in CoordKind::ALL {
                            if ui.button(format!("{}  {}", k.icon(), k.label())).on_hover_text(k.tip()).clicked() {
                                self.add_coord_object(k);
                                ui.close_menu();
                            }
                        }
                    });
                    if ui.button("⚡ Auto voids").on_hover_text("A void wherever a pipe / duct / cable tray crosses a wall or slab").clicked() {
                        self.auto_voids();
                    }
                    if ui.button("➡ Send voids to Revit").on_hover_text("Write the APPROVED voids for the Revit add-in; in Revit press ManageBIM › Voids to cut them into the host walls and slabs").clicked() {
                        self.send_voids_to_revit();
                    }
                    if ui.button("⚠ Clash test").on_hover_text("New Clash Detective test: coordination objects vs all models (what enters a clearance zone, what a void hits...)").clicked() {
                        self.coord_clash_test();
                    }
                    ui.separator();
                    ui.label(RichText::new("Margins").small().weak());
                    ui.add(egui::DragValue::new(&mut self.coord_margin).range(0.0..=5.0).speed(0.01).max_decimals(2).suffix(" m")).on_hover_text("Clearance zone: distance around the selected equipment");
                    ui.add(egui::DragValue::new(&mut self.void_margin).range(0.0..=1.0).speed(0.005).max_decimals(3).suffix(" m")).on_hover_text("Auto voids: gap around the service on each side");
                });
                ui.separator();
                if self.coord_objects.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("No coordination objects yet.").strong());
                    ui.label(RichText::new("• Clearance zone: select equipment (AHU, boiler, panel) and press Clearance - a zone with the margin above appears around it.\n• Provision for void: Auto voids finds every pipe / duct / tray crossing a wall or slab; or select one and press Void.\n• Placeholder: reserve space for equipment that is not modelled yet, or a corridor for a route.\nAll of them take part in Clash Detective and are exported to IFC.").small().weak());
                    return;
                }
                let counts: Vec<usize> = CoordKind::ALL.iter().map(|k| self.coord_objects.iter().filter(|o| o.kind == *k).count()).collect();
                ui.label(RichText::new(format!("{} clearance · {} voids · {} placeholders", counts[0], counts[1], counts[2])).small().weak());
                let list_h = (ui.available_height() * 0.5).clamp(120.0, 300.0);
                egui::ScrollArea::vertical().id_salt("coord_list").max_height(list_h).auto_shrink([false, true]).show(ui, |ui| {
                    for i in 0..self.coord_objects.len() {
                        let sel = self.coord_sel == Some(i);
                        let c = self.coord_objects[i].kind.color();
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(EV::new(10.0, 16.0), Sense::hover());
                            ui.painter().rect_filled(r, 2.0, Color32::from_rgb(c[0], c[1], c[2]));
                            let mut vis = self.coord_objects[i].visible;
                            if ui.checkbox(&mut vis, "").on_hover_text("Show / hide in 3D").changed() {
                                self.coord_objects[i].visible = vis;
                                rebuild = true;
                            }
                            let o = &self.coord_objects[i];
                            let s = o.size();
                            if ui.selectable_label(sel, format!("{} {}", o.kind.icon(), o.name)).clicked() {
                                select = Some(i);
                            }
                            ui.label(RichText::new(format!("{:.0}×{:.0}×{:.0} mm", s.x * 1000.0, s.y * 1000.0, s.z * 1000.0)).small().monospace().weak());
                            ui.label(RichText::new(&o.status).small());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("🗑").on_hover_text("Delete").clicked() {
                                    remove = Some(i);
                                }
                                if ui.small_button("🔍").on_hover_text("Select and zoom to it").clicked() {
                                    zoom = Some(i);
                                }
                            });
                        });
                    }
                });
                ui.separator();
                // ---- editorul obiectului ales
                let Some(i) = self.coord_sel.filter(|&i| i < self.coord_objects.len()) else {
                    ui.label(RichText::new("Click an object to edit it.").small().weak());
                    return;
                };
                let mut changed = false;
                egui::Grid::new("coord_edit").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                    let o = &mut self.coord_objects[i];
                    ui.label("Name");
                    changed |= ui.add(egui::TextEdit::singleline(&mut o.name).desired_width(300.0)).lost_focus();
                    ui.end_row();
                    ui.label("Kind");
                    egui::ComboBox::from_id_salt("coord_kind").selected_text(o.kind.label()).show_ui(ui, |ui| {
                        for k in CoordKind::ALL {
                            changed |= ui.selectable_value(&mut o.kind, k, k.label()).on_hover_text(k.tip()).changed();
                        }
                    });
                    ui.end_row();
                    ui.label("Status");
                    egui::ComboBox::from_id_salt("coord_status").selected_text(o.status.clone()).show_ui(ui, |ui| {
                        for s in STATUSES {
                            if ui.selectable_label(o.status == s, s).clicked() {
                                o.status = s.into();
                                changed = true;
                            }
                        }
                    });
                    ui.end_row();
                    ui.label("Size W × D × H");
                    ui.horizontal(|ui| {
                        let mut s = o.size() * 1000.0;
                        let mut any = false;
                        for (k, sfx) in [(0usize, " mm W"), (1, " mm D"), (2, " mm H")] {
                            any |= ui.add(egui::DragValue::new(&mut s[k]).range(10.0..=100_000.0).speed(5.0).max_decimals(0).suffix(sfx)).changed();
                        }
                        if any {
                            o.set_size(s / 1000.0);
                            changed = true;
                        }
                    });
                    ui.end_row();
                    ui.label("Centre X / Y / Z");
                    ui.horizontal(|ui| {
                        let mut c = o.center();
                        let mut any = false;
                        for k in 0..3 {
                            any |= ui.add(egui::DragValue::new(&mut c[k]).speed(0.01).max_decimals(3).suffix(" m")).changed();
                        }
                        if any {
                            o.set_center(c);
                            changed = true;
                        }
                    });
                    ui.end_row();
                    ui.label("Discipline");
                    changed |= ui.add(egui::TextEdit::singleline(&mut o.discipline).hint_text("MEP - HVAC, Structure…").desired_width(300.0)).lost_focus();
                    ui.end_row();
                    if o.kind == CoordKind::ProvisionForVoid {
                        ui.label("Host");
                        changed |= ui.add(egui::TextEdit::singleline(&mut o.host).hint_text("wall / slab name or GUID").desired_width(300.0)).lost_focus();
                        ui.end_row();
                    }
                    ui.label("Note");
                    changed |= ui.add(egui::TextEdit::multiline(&mut o.note).desired_rows(2).desired_width(300.0)).lost_focus();
                    ui.end_row();
                });
                if ui.button("🎯 Fit to selection").on_hover_text("Resize this object to the box of the selected elements (plus the clearance margin for clearance zones)").clicked() {
                    let m = if self.coord_objects[i].kind == CoordKind::Clearance { self.coord_margin } else { 0.0 };
                    let mut bb = Aabb::EMPTY;
                    for id in &self.selected {
                        if Some(id.m as usize) == self.coord_slot {
                            continue;
                        }
                        if let Some((_, e)) = self.elem(*id) {
                            bb.union(&e.aabb);
                        }
                    }
                    if bb.is_valid() {
                        self.coord_objects[i].min = (bb.min - Vec3::splat(m)).to_array();
                        self.coord_objects[i].max = (bb.max + Vec3::splat(m)).to_array();
                        changed = true;
                    }
                }
                if changed {
                    rebuild = true;
                }
            });
        self.coord_open = open;
        if let Some(i) = remove {
            self.remove_coord_object(i);
        } else if rebuild {
            self.rebuild_coord_model();
        }
        if let Some(i) = select.or(zoom) {
            self.coord_sel = Some(i);
            if let Some(s) = self.coord_slot {
                let id = ElemId { m: s as u32, e: i as u32 };
                self.apply(Action::Select(vec![id], false));
                if zoom.is_some() {
                    self.apply(Action::Focus(vec![id]));
                }
            }
        }
    }
}
