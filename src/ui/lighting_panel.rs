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
                        // in centrul modelului, la 2 m deasupra - nu la camera (altfel e la sute de metri)
                        let bb = self.scene_aabb();
                        let c = bb.center();
                        let center = if bb.is_valid() { c } else { Vec3::ZERO };
                        l.pos = [center.x, center.y, center.z + 2.0];
                        l.target = [center.x, center.y, center.z];
                        self.light_sel = Some(self.lights.len());
                        self.lights.push(l);
                        // muta camera astfel incat lumina sa fie vizibila imediat
                        let mut t = self.cam;
                        t.focus = t.focus.min(30.0);
                        self.fly_to(t);
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button("🎯 Place by clicking in 3D").on_hover_text("Switches to the Place Light tool: click a surface in the 3D view to drop a point light there").clicked() {
                    self.tool = Tool::PlaceLight;
                    ui.close_menu();
                }
            });
            ui.separator();
            ui.label(RichText::new("Gizmo").small().strong().color(accent())).on_hover_text("Move: drag the X/Y/Z arrows. Rotate: drag the circles to aim the light.");
            if ui.selectable_label(!self.light_rotate_mode, "✥ Move").on_hover_text("Drag the red X / green Y / blue Z arrows to move the light").clicked() {
                self.light_rotate_mode = false;
            }
            if ui.selectable_label(self.light_rotate_mode, "⟳ Rotate").on_hover_text("Drag a circle to rotate the light direction around that plane's axis (XY → Z, XZ → Y, YZ → X)").clicked() {
                self.light_rotate_mode = true;
            }
            if ui.checkbox(&mut self.render.night, "🌙 Night").on_hover_text("Dark sky and low ambient light - the lamps in the model light the scene").clicked() {
                // nimic altceva: starea se citeste la pornirea randarii
            }
            let placing = self.tool == Tool::PlaceLight;
            if ui.selectable_label(placing, "🎯 Place in 3D").on_hover_text("Click a surface in the 3D view to place a point light; press Esc to go back to Select").clicked() {
                self.tool = if placing { Tool::Select } else { Tool::PlaceLight };
            }
            if ui.add_enabled(self.light_sel.is_some(), egui::Button::new("🎯 Focus").small())
                .on_hover_text("Fly the camera to the selected light (useful if it was created far away in an older project)").clicked()
            {
                if let Some(i) = self.light_sel {
                    if i < self.lights.len() {
                        let p = Vec3::from(self.lights[i].pos);
                        let mut t = self.cam;
                        t.eye = (p + Vec3::new(-3.0, -3.0, 2.0)).to_array();
                        t.focus = 4.0;
                        t.yaw = std::f32::consts::FRAC_PI_4;
                        t.pitch = -0.35;
                        self.fly_to(t);
                    }
                }
            }
            if ui.add_enabled(!self.lights.is_empty(), egui::Button::new("🗑 Remove").small()).clicked() {
                // sterge lumina selectata, sau ultima daca nimic nu e selectat
                let i = self.light_sel.take().unwrap_or_else(|| self.lights.len().saturating_sub(1));
                if i < self.lights.len() {
                    self.lights.remove(i);
                }
                self.light_sel = if self.lights.is_empty() { None } else { Some((i).min(self.lights.len() - 1)) };
            }
        });
        if self.lights.is_empty() {
            ui.label(RichText::new("No lights yet. Use Create Light, or click a surface in the 3D view with the Place in 3D tool.").small().weak());
            return;
        }
        ui.separator();
        let mut remove_pending: Option<usize> = None;
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
                    if ui.small_button("🗑").on_hover_text("Remove this light").clicked() {
                        remove_pending = Some(i);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Lux (power)").small());
                    ui.add(egui::Slider::new(&mut lamp.power, 0.0..=20.0).step_by(0.1));
                    ui.label(RichText::new("Range").small());
                    ui.add(egui::DragValue::new(&mut lamp.radius).range(0.0..=5000.0).speed(0.5).suffix(" m"))
                        .on_hover_text("0 m = unlimited range (light does not fall off with distance)");
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
        if let Some(i) = remove_pending {
            if i < self.lights.len() {
                self.lights.remove(i);
            }
            self.light_sel = if self.lights.is_empty() { None } else { Some(i.min(self.lights.len() - 1)) };
        }
    }
}
