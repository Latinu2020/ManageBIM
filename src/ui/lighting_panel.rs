//! Panoul Lighting (stil Navisworks / Autodesk Ray Tracer):
//! Create Light (Point / Spot / Distant), lista luminilor, putere (lux), raza, culoare,
//! pozitie si tinta, plus unealta de plasare prin click in 3D.
use super::*;

impl App {
    pub(super) fn lighting_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("LIGHTING").small().strong().color(accent()))
                .on_hover_text("Lights are saved in the project and used by the path trace render. Click with the Place Light tool to add one in the 3D view.");
            let id = ui.make_persistent_id("create_light_menu");
            let r = ui.button("💡 Create Light ⏷");
            if r.clicked() {
                ui.memory_mut(|m| m.toggle_popup(id));
            }
            egui::popup_below_widget(ui, id, &r, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(260.0);
                for k in crate::lights::LightKind::ALL {
                    let tip = match k {
                        crate::lights::LightKind::Point => "Radiates in all directions from one point (like a bulb)",
                        crate::lights::LightKind::Spot => "Cone aimed at a target, with adjustable angle",
                        crate::lights::LightKind::Distant => "Parallel rays, like the sun - no distance falloff",
                    };
                    if ui.button(RichText::new(k.label()).strong()).on_hover_text(tip).clicked() {
                        let mut l = crate::lights::SceneLight::default();
                        l.kind = k;
                        l.name = format!("{} {}", k.label(), self.lights.len() + 1);
                        l.pos = self.cam.eye().to_array();
                        l.target = (self.cam.eye() + self.cam.fwd() * 5.0).to_array();
                        self.light_sel = Some(self.lights.len());
                        self.lights.push(l);
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button("🎯 Place by clicking in 3D").on_hover_text("Switches to the Place Light tool: click a surface in the 3D view to drop a point light there").clicked() {
                    self.tool = Tool::PlaceLight;
                    ui.close_menu();
                }
            });
            let placing = self.tool == Tool::PlaceLight;
            if ui.selectable_label(placing, "🎯 Place in 3D").on_hover_text("Click a surface in the 3D view to place a point light; press Esc to go back to Select").clicked() {
                self.tool = if placing { Tool::Select } else { Tool::PlaceLight };
            }
            if ui.add_enabled(!self.lights.is_empty(), egui::Button::new("🗑 Remove").small()).clicked() {
                if let Some(i) = self.light_sel.take() {
                    if i < self.lights.len() {
                        self.lights.remove(i);
                    }
                }
            }
        });
        if self.lights.is_empty() {
            ui.label(RichText::new("No lights yet. Use Create Light, or click a surface in the 3D view with the Place in 3D tool.").small().weak());
            return;
        }
        ui.separator();
        for (i, lamp) in self.lights.iter_mut().enumerate() {
            let sel = self.light_sel == Some(i);
            let frame = egui::Frame::none()
                .fill(if sel { accent().gamma_multiply(0.14) } else { ui.visuals().faint_bg_color })
                .rounding(crate::theme::RADIUS_SM)
                .inner_margin(egui::Margin::symmetric(7.0, 5.0));
            frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.selectable_label(sel, "").clicked() {
                        self.light_sel = Some(i);
                    }
                    ui.checkbox(&mut lamp.enabled, "");
                    ui.add(egui::TextEdit::singleline(&mut lamp.name).desired_width(120.0));
                    egui::ComboBox::from_id_salt(("lkind", i)).selected_text(lamp.kind.label()).show_ui(ui, |ui| {
                        for k in crate::lights::LightKind::ALL {
                            ui.selectable_value(&mut lamp.kind, k, k.label());
                        }
                    });
                    let mut c = Color32::from_rgb(lamp.color[0], lamp.color[1], lamp.color[2]);
                    if ui.color_edit_button_srgba(&mut c).changed() {
                        lamp.color = [c.r(), c.g(), c.b()];
                    }
                    if ui.small_button("🗑").clicked() {
                        self.light_sel = None;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Lux (power)").small());
                    ui.add(egui::Slider::new(&mut lamp.power, 0.0..=20.0).step_by(0.1));
                    ui.label(RichText::new("Range").small());
                    ui.add(egui::DragValue::new(&mut lamp.radius).range(0.0..=500.0).speed(0.5).suffix(" m"));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Position").small());
                    for axis in 0..3 {
                        ui.label(RichText::new(["x", "y", "z"][axis]).small());
                        ui.add(egui::DragValue::new(&mut lamp.pos[axis]).speed(0.05).suffix(" m"));
                    }
                });
                if lamp.kind != crate::lights::LightKind::Point {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Target").small());
                        for axis in 0..3 {
                            ui.add(egui::DragValue::new(&mut lamp.target[axis]).speed(0.05).suffix(" m"));
                        }
                    });
                }
                if lamp.kind == crate::lights::LightKind::Spot {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Cone").small());
                        ui.add(egui::Slider::new(&mut lamp.angle, 5.0..=120.0).suffix("°"));
                    });
                }
            });
            ui.add_space(3.0);
        }
    }
}
