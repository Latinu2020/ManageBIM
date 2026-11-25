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
                // deschide fereastra cu tipul pregatit; obiectul se creeaza doar cand apesi Create
                let on = self.coord_open && self.coord_pending == Some(k);
                if rbtn(ui, k.icon(), short, &format!("{} - {} Opens the Coordination objects window; press Create there to add one around the selection, or at the centre of the view.", k.label(), k.tip()), on, has).clicked() {
                    if on {
                        self.coord_pending = None;
                    } else {
                        self.coord_pending = Some(k);
                        self.coord_open = true;
                    }
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
            // in coltul din stanga-jos, nu peste centrul vederii unde e obiectul si gizmo-ul lui
            .default_size([560.0, 420.0])
            .default_pos(ctx.screen_rect().left_bottom() + EV::new(330.0, -470.0))
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
                    if ui.button("➡ Send to Revit / Navisworks").on_hover_text("Show the visible clearance zones, voids and placeholders in Revit or Navisworks (display only, nothing is changed): press ManageBIM › Coordination there").clicked() {
                        self.send_coord_to_revit();
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
                // tip ales din ribbon: bara de confirmare (nimic nu se creeaza pana nu apesi Create)
                if let Some(k) = self.coord_pending {
                    let nsel = self.selected.iter().filter(|id| Some(id.m as usize) != self.coord_slot).count();
                    let where_ = if nsel > 0 {
                        format!("around the {nsel} selected item(s){}", if k == CoordKind::Clearance { format!(" + {:.2} m margin", self.coord_margin) } else { String::new() })
                    } else {
                        "at the centre of the view (select elements first to wrap them)".to_string()
                    };
                    let c = k.color();
                    egui::Frame::none()
                        .fill(Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 34))
                        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(c[0], c[1], c[2])))
                        .rounding(4.0)
                        .inner_margin(6.0)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(format!("{} New {}", k.icon(), k.label())).strong());
                                ui.label(RichText::new(where_).small().weak());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button("Cancel").clicked() {
                                        self.coord_pending = None;
                                    }
                                    if ui.button(RichText::new("➕ Create").strong()).clicked() {
                                        self.coord_pending = None;
                                        self.add_coord_object(k);
                                    }
                                });
                            });
                        });
                    ui.add_space(4.0);
                }
                if self.coord_objects.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("No coordination objects yet.").strong());
                    ui.label(RichText::new("• Clearance zone: select equipment (AHU, boiler, panel) and press Clearance - a zone with the margin above appears around it.\n• Provision for void: Auto voids finds every pipe / duct / tray crossing a wall or slab; or select one and press Void.\n• Placeholder: reserve space for equipment that is not modelled yet, or a corridor for a route.\nAll of them take part in Clash Detective and are exported to IFC.").small().weak());
                    return;
                }
                let counts: Vec<usize> = CoordKind::ALL.iter().map(|k| self.coord_objects.iter().filter(|o| o.kind == *k).count()).collect();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{} clearance · {} voids · {} placeholders", counts[0], counts[1], counts[2])).small().weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // gizmo-ul din 3D pentru obiectul ales (ca la lumini)
                        for (m, label, tip) in [(2u8, "⟳ Rotate", "Drag the blue circle to turn the object around the vertical axis (Shift: 15° steps)"), (1, "✥ Move", "Drag the X / Y / Z arrows to move the object"), (0, "⇲ Resize", "Drag the arrows on the faces to change the size")] {
                            if ui.selectable_label(self.coord_mode == m, label).on_hover_text(tip).clicked() {
                                self.coord_mode = m;
                            }
                        }
                        ui.label(RichText::new("Gizmo").small().weak());
                    });
                });
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
                    ui.label("Rotation");
                    ui.horizontal(|ui| {
                        changed |= ui.add(egui::DragValue::new(&mut o.rot_deg).range(-360.0..=360.0).speed(0.5).max_decimals(1).suffix("°")).on_hover_text("Turn around the vertical axis through the centre").changed();
                        if ui.small_button("0°").clicked() { o.rot_deg = 0.0; changed = true; }
                        if ui.small_button("+90°").clicked() { o.rot_deg = (o.rot_deg + 90.0) % 360.0; changed = true; }
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
        if !open {
            self.coord_pending = None;
        }
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
