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
                        // deasupra modelului, indreptata in jos spre acoperis: lumina si pata ei se vad
                        // imediat din exterior (inainte era in centrul modelului = in interiorul cladirii)
                        let bb = self.scene_aabb();
                        let (center, top, size) = if bb.is_valid() { (bb.center(), bb.max.z, bb.size().length()) } else { (Vec3::ZERO, 0.0, 10.0) };
                        let h = (size * 0.15).clamp(3.0, 40.0);
                        l.pos = [center.x, center.y, top + h];
                        l.target = [center.x, center.y, top];
                        l.radius = l.radius.max(h * 3.0);
                        if k == crate::lights::LightKind::Spot {
                            l.angle = 60.0;
                        }
                        self.light_sel = Some(self.lights.len());
                        self.lights.push(l);
                        // muta camera astfel incat lumina sa fie vizibila imediat
                        let mut t = self.cam;
                        t.focus = t.focus.min(30.0);
                        self.fly_to(t);
                        ui.memory_mut(|m| m.close_popup());
                    }
                }
                ui.separator();
                if ui.button("🎯 Place by clicking in 3D").on_hover_text("Switches to the Place Light tool: click a surface in the 3D view to drop a point light there").clicked() {
                    self.tool = Tool::PlaceLight;
                    ui.memory_mut(|m| m.close_popup());
                }
            });
            ui.separator();
            ui.label(RichText::new("Gizmo").small().strong().color(accent())).on_hover_text("Move: drag the X/Y/Z arrows. Rotate: drag the circles to aim the light.");
            if ui.selectable_label(!self.light_rotate_mode, "✥ Move").on_hover_text("Drag the red X / green Y / blue Z arrows to move the light").clicked() {
                self.light_rotate_mode = false;
            }
            if ui.selectable_label(self.light_rotate_mode, "⟳ Rotate").on_hover_text("Drag a circle to aim a Spot / Distant light: blue circle turns around Z (left / right), green around Y, red around X (up / down)").clicked() {
                self.light_rotate_mode = true;
                // o lumina punctuala nu are directie: spunem cum se orienteaza
                if let Some(i) = self.light_sel.filter(|&i| i < self.lights.len()) {
                    if self.lights[i].kind == crate::lights::LightKind::Point {
                        self.toast("Drag a circle in the 3D view: the point light becomes a Spot light you can aim");
                    }
                }
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
        ui.separator();
        // pe coloane cand e loc (panoul de jos e lat si scund): Soare & cer | Lampi
        let w = ui.available_width();
        if w > 980.0 {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(470.0);
                    self.sun_sky_ui(ui);
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width((w - 500.0).max(300.0));
                    self.lamps_ui(ui);
                });
            });
        } else {
            self.sun_sky_ui(ui);
            ui.add_space(2.0);
            self.lamps_ui(ui);
        }
    }

    /// Soarele si cerul (inainte erau intr-o fereastra separata): totul in panoul Lighting.
    fn sun_sky_ui(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("☀ Sun & sky").strong())
            .id_salt("sun_sky_hdr")
            .default_open(true)
            .show(ui, |ui| {
                let mut changed = false;
                ui.horizontal(|ui| {
                    let mut on = self.render.sun_power > 0.0;
                    if ui.checkbox(&mut on, "Sun on").on_hover_text("Turn the sun off for interiors or night scenes lit only by lamps").changed() {
                        self.render.sun_power = if on { 1.0 } else { 0.0 };
                        changed = true;
                    }
                    if ui.button("⟲ Default").on_hover_text("Default brightness, ambient light and sun").clicked() {
                        self.render.reset_light();
                        changed = true;
                    }
                    ui.label(RichText::new("Applies to the 3D view, images, videos and the path trace.").weak().small());
                });
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_max_width(520.0);
                        changed |= self.sun_settings_ui(ui, "sun_panel_grid");
                    });
                });
                if changed {
                    self.render.update_sun();
                    ui.ctx().request_repaint();
                }
            });
    }

    /// Lampile din model: fise una langa alta.
    fn lamps_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("LAMPS").small().strong().color(accent()));
        if self.lights.is_empty() {
            ui.label(RichText::new("No lamps yet. Use Create Light, or click a surface in the 3D view with the Place in 3D tool.").small().weak());
            return;
        }
        let mut remove_pending: Option<usize> = None;
        let card_w = 470.0f32.min(ui.available_width());
        ui.horizontal_wrapped(|ui| {
        for (i, lamp) in self.lights.iter_mut().enumerate() {
            let sel = self.light_sel == Some(i);
            let frame = egui::Frame::none()
                .fill(if sel { accent().gamma_multiply(0.14) } else { ui.visuals().faint_bg_color })
                .rounding(crate::theme::RADIUS_SM)
                .inner_margin(egui::Margin::symmetric(7.0, 5.0));
            // fise una langa alta: panoul de jos e lat si scund, nu mai trebuie marit peste vederea 3D
            ui.vertical(|ui| {
            ui.set_width(card_w);
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
                    // fara plafon: slider logaritmic 0..10 000, iar in casuta se poate tasta orice valoare mai mare
                    ui.add(egui::Slider::new(&mut lamp.power, 0.0..=10_000.0).logarithmic(true).smallest_positive(0.1).clamping(egui::SliderClamping::Never).max_decimals(1))
                        .on_hover_text("Light power. Drag for 0 - 10 000, or double-click the number and type any larger value.");
                    lamp.power = lamp.power.max(0.0);
                    ui.label(RichText::new("Range").small());
                    ui.add(egui::DragValue::new(&mut lamp.radius).range(0.0..=100_000.0).speed(0.5).max_decimals(1).suffix(" m"))
                        .on_hover_text("0 m = unlimited range (light does not fall off with distance)");
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Position").small());
                    for axis in 0..3 {
                        ui.label(RichText::new(["x", "y", "z"][axis]).small());
                        ui.add(egui::DragValue::new(&mut lamp.pos[axis]).speed(0.05).max_decimals(2).suffix(" m"));
                    }
                });
                if lamp.kind != crate::lights::LightKind::Point {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Target").small());
                        for axis in 0..3 {
                            ui.add(egui::DragValue::new(&mut lamp.target[axis]).speed(0.05).max_decimals(2).suffix(" m"));
                        }
                    });
                    // orientare numerica (ca la Rotate din gizmo): azimut in plan + inclinare fata de orizontala
                    ui.horizontal(|ui| {
                        let (mut az, mut tilt) = lamp.aim_angles();
                        ui.label(RichText::new("Aim").small()).on_hover_text("Direction of the light: azimuth (0° = +X, 90° = +Y) and tilt (-90° = straight down)");
                        let a = ui.add(egui::DragValue::new(&mut az).range(-180.0..=180.0).speed(1.0).max_decimals(1).suffix("° az"));
                        let t = ui.add(egui::DragValue::new(&mut tilt).range(-90.0..=90.0).speed(1.0).max_decimals(1).suffix("° tilt"));
                        if a.changed() || t.changed() {
                            lamp.set_aim_angles(az, tilt);
                        }
                        if ui.small_button("⬇").on_hover_text("Aim straight down").clicked() {
                            lamp.set_aim_angles(az, -90.0);
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
            });
        }
        });
        if let Some(i) = remove_pending {
            if i < self.lights.len() {
                self.lights.remove(i);
            }
            self.light_sel = if self.lights.is_empty() { None } else { Some(i.min(self.lights.len() - 1)) };
        }
    }
}

impl App {
    /// Soarele: intensitate, umbre, pozitie manuala sau dupa data / ora / locatie (studii de insorire).
    /// Folosit in panoul Lighting si in fereastra Lighting; intoarce true daca s-a schimbat ceva.
    pub(super) fn sun_settings_ui(&mut self, ui: &mut egui::Ui, id: &str) -> bool {
        let mut changed = false;
        egui::Grid::new(id).num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Brightness");
                    changed |= ui.add(egui::Slider::new(&mut self.render.exposure, 0.3..=2.5).fixed_decimals(2)).changed();
                    ui.end_row();
                    ui.label("Ambient light");
                    changed |= ui.add(egui::Slider::new(&mut self.render.ambient, 0.0..=3.0).fixed_decimals(2)).changed();
                    ui.end_row();
                    ui.label("Sun intensity");
                    changed |= ui.add(egui::Slider::new(&mut self.render.sun_power, 0.0..=10.0).logarithmic(true).smallest_positive(0.05).fixed_decimals(2)).changed();
                    ui.end_row();
                    ui.label("Shadows").on_hover_text("Shadows cast by the sun (3D view, images and videos)");
                    changed |= ui.checkbox(&mut self.render.shadows, "Cast shadows").changed();
                    ui.end_row();
                    ui.label("Sun position");
                    changed |= ui.checkbox(&mut self.render.sun_from_time, "From date, time and location").on_hover_text("Real sun position (shadow studies): date, local time, latitude / longitude and the project north").changed();
                    ui.end_row();
                    if self.render.sun_from_time {
                        ui.label("Date");
                        ui.horizontal(|ui| {
                            changed |= ui.add(egui::TextEdit::singleline(&mut self.render.sun_date).desired_width(90.0).hint_text("YYYY-MM-DD")).changed();
                            for (lbl, d) in [("21 Mar", "-03-21"), ("21 Jun", "-06-21"), ("21 Dec", "-12-21")] {
                                if ui.small_button(lbl).clicked() {
                                    let y = self.render.sun_date.get(..4).unwrap_or("2026").to_string();
                                    self.render.sun_date = format!("{y}{d}");
                                    changed = true;
                                }
                            }
                        });
                        ui.end_row();
                        ui.label("Local time");
                        let h = self.render.sun_hour;
                        changed |= ui.add(egui::Slider::new(&mut self.render.sun_hour, 4.0..=21.0).step_by(0.25).custom_formatter(|v, _| format!("{:02}:{:02}", v as u32, ((v.fract()) * 60.0).round() as u32))).changed();
                        let _ = h;
                        ui.end_row();
                        ui.label("Latitude / longitude");
                        ui.horizontal(|ui| {
                            changed |= ui.add(egui::DragValue::new(&mut self.render.lat).range(-89.0..=89.0).speed(0.05).suffix("°")).changed();
                            changed |= ui.add(egui::DragValue::new(&mut self.render.lon).range(-180.0..=180.0).speed(0.05).suffix("°")).changed();
                        });
                        ui.end_row();
                        ui.label("Time zone / North");
                        ui.horizontal(|ui| {
                            changed |= ui.add(egui::DragValue::new(&mut self.render.tz).range(-12.0..=14.0).speed(0.25).prefix("UTC+")).on_hover_text("Romania: +2 in winter, +3 in summer").changed();
                            changed |= ui.add(egui::DragValue::new(&mut self.render.north_deg).range(-180.0..=180.0).speed(0.5).suffix("°")).on_hover_text("Project north: angle of true north from the model +Y axis (counter-clockwise)").changed();
                        });
                        ui.end_row();
                        if changed {
                            self.render.update_sun();
                        }
                        let (az, el) = crate::timeline::parse_date(&self.render.sun_date).map(|d| crate::anim::sun_position(d, self.render.sun_hour, self.render.tz, self.render.lat, self.render.lon)).unwrap_or((0.0, 0.0));
                        ui.label("");
                        ui.label(RichText::new(if el < 0.0 { format!("Sun below the horizon ({el:.0}°)") } else { format!("Sun: azimuth {az:.0}° from north, {el:.0}° high") }).small().weak());
                        ui.end_row();
                    } else {
                        ui.label("Sun azimuth");
                        changed |= ui.add(egui::Slider::new(&mut self.render.sun_az, -180.0..=180.0).suffix("°")).changed();
                        ui.end_row();
                        ui.label("Sun height");
                        changed |= ui.add(egui::Slider::new(&mut self.render.sun_el, 5.0..=90.0).suffix("°")).changed();
                        ui.end_row();
                    }
                    ui.label("Contact shadows").on_hover_text("Ambient occlusion (SSAO): soft shadows in corners and where elements meet");
                    changed |= ui.add(egui::Slider::new(&mut self.render.ao, 0.0..=1.0).fixed_decimals(2)).changed();
                    ui.end_row();
                    ui.label("Edges").on_hover_text("Fine outlines on element edges and silhouettes (0 = off)");
                    changed |= ui.add(egui::Slider::new(&mut self.render.edges, 0.0..=1.0).fixed_decimals(2)).changed();
                    ui.end_row();
        });
        changed
    }
}
