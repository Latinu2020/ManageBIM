//! Floating navigation bar at the bottom of the 3D view (undo, selection, visibility, navigation, tools, light, ray trace, walk path) and its light popup.
use super::*;

impl App {
    /// Floating navigation bar (bottom, centred): what is used often, without a trip through the ribbon.
    /// The bottom toolbar ("studio" look): big buttons with icon and label, in groups
    /// (selection, visibility, navigation, tools). The colours come from the current theme.
    pub(super) fn nav_bar(&mut self, ctx: &egui::Context, rect: Rect) {
        let pal = self.pal();
        let pos = rect.center_bottom() - EV::new(0.0, 14.0);
        egui::Area::new(egui::Id::new("nav_bar")).fixed_pos(pos).pivot(Align2::CENTER_BOTTOM).order(egui::Order::Middle).show(ctx, |ui| {
            egui::Frame::none()
                .fill(pal.overlay_btn)
                .stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(18)))
                .rounding(egui::Rounding::same(14.0))
                .shadow(egui::epaint::Shadow { offset: EV::new(0.0, 6.0), blur: 20.0, spread: 0.0, color: Color32::from_black_alpha(110) })
                .inner_margin(egui::Margin::symmetric(8.0, 3.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let text_col = pal.overlay_text;
                        // big button: icon + label; active = background in the accent colour
                        let big = |ui: &mut egui::Ui, icon: &str, label: &str, tip: &str, on: bool, enabled: bool| -> bool {
                            // compact: ~24 px, icon only (the name appears in the tooltip)
                            let _ = label;
                            let (r, resp) = ui.allocate_exact_size(EV::new(30.0, 28.0), if enabled { Sense::click() } else { Sense::hover() });
                            let p = ui.painter();
                            if on {
                                p.rect_filled(r.shrink(1.0), 7.0, accent());
                            } else if enabled && resp.hovered() {
                                p.rect_filled(r.shrink(1.0), 7.0, Color32::from_white_alpha(16));
                            }
                            let col = if !enabled { text_col.gamma_multiply(0.35) } else if on { Color32::WHITE } else { text_col };
                            p.text(r.center(), Align2::CENTER_CENTER, crate::theme::ph(icon), crate::theme::icon_font(16.0), col);
                            let resp = resp.on_hover_text(format!("{label}: {tip}"));
                            enabled && resp.clicked()
                        };
                        let sep = |ui: &mut egui::Ui| {
                            let (r, _) = ui.allocate_exact_size(EV::new(8.0, 28.0), Sense::hover());
                            ui.painter().line_segment([r.center_top() + EV::new(0.0, 5.0), r.center_bottom() - EV::new(0.0, 5.0)], Stroke::new(1.0_f32, Color32::from_white_alpha(28)));
                        };
                        // ---- back / forward (selection, visibility, section, camera)
                        if big(ui, "⟲", "Undo", "Undo: selection, hidden items, section, camera, properties, colours and materials (Ctrl+Z)", false, self.can_undo()) {
                            self.undo_view();
                        }
                        if big(ui, "⟳", "Redo", "Redo view (Ctrl+Y)", false, self.can_redo()) {
                            self.redo_view();
                        }
                        sep(ui);
                        // ---- selection
                        if big(ui, "👆", "Select", "Select (click an item · Ctrl+click adds)", self.tool == Tool::Select && self.nav == NavMode::Orbit, true) {
                            self.cur_measure.clear();
                            self.tool = Tool::Select;
                            self.nav = NavMode::Orbit;
                        }
                        if big(ui, "⬚", "Window", "Window selection: drag left -> right = inside, right -> left = touching (Ctrl adds)", self.tool == Tool::Window, true) {
                            self.cur_measure.clear();
                            self.tool = if self.tool == Tool::Window { Tool::Select } else { Tool::Window };
                        }
                        sep(ui);
                        // ---- visibility
                        let sel: Vec<ElemId> = self.selected.iter().copied().collect();
                        if big(ui, "🚫", "Hide", "Hide the selected items (H)", false, !sel.is_empty()) {
                            self.apply(Action::Hide(sel.clone()));
                        }
                        if big(ui, "◎", "Isolate", "Show only the selection - hide the rest (I)", false, !sel.is_empty()) {
                            self.apply(Action::Isolate(sel.clone()));
                        }
                        if big(ui, "👁", "Unhide", &format!("Unhide all ({} hidden)", self.hidden.len()), false, !self.hidden.is_empty()) {
                            self.apply(Action::ShowAll);
                            self.clash.focus = None;
                        }
                        if big(ui, "🎯", "Focus", "Focus on the selection (F) · double-click an item does the same", false, !sel.is_empty()) {
                            self.apply(Action::Focus(sel.clone()));
                        }
                        sep(ui);
                        // ---- navigation
                        let orbit = self.nav == NavMode::Orbit && matches!(self.tool, Tool::Select);
                        if big(ui, "🔄", "Orbit", "Orbit: drag to rotate around the point under the cursor", orbit, true) {
                            self.nav = NavMode::Orbit;
                            self.tool = Tool::Select;
                            self.path_run = None;
                        }
                        if big(ui, "✋", "Pan", "Pan: drag to move the model sideways (middle button always pans)", self.nav == NavMode::Pan, true) {
                            self.nav = NavMode::Pan;
                            self.tool = Tool::Select;
                            self.path_run = None;
                        }
                        if big(ui, "🚶", "Walk", "Walk: drag up = forward, sideways = turn; W A S D", self.nav == NavMode::Walk && self.tool != Tool::WalkPath, true) {
                            self.nav = NavMode::Walk;
                            self.tool = Tool::Select;
                        }
                        let bb = self.scene_aabb();
                        if big(ui, "⛶", "Fit", "Zoom to the whole model", false, true) {
                            let mut t = self.cam;
                            t.fit(&bb);
                            self.fly_to(t);
                        }
                        if big(ui, "🏠", "Home", "Home view (Home key)", false, true) {
                            self.go_home();
                        }
                        sep(ui);
                        // ---- tools
                        let meas = matches!(self.tool, Tool::Measure(_));
                        if big(ui, "📏", "Measure", "Measure point to point (snaps to corners, midpoints, edges) - more kinds in Review › Measure", meas, true) {
                            self.cur_measure.clear();
                            self.tool = if meas { Tool::Select } else { Tool::Measure(MeasureKind::PointToPoint) };
                        }
                        let sec = self.section.enabled;
                        if big(ui, "▣", "Section", if sec { "Turn the section off" } else { "Section box around the selection (or the whole model) - stretch it with the arrows on its faces" }, sec, true) {
                            if sec {
                                self.section.enabled = false;
                            } else if !sel.is_empty() {
                                self.section_to_selection();
                            } else if bb.is_valid() {
                                self.section.enabled = true;
                                self.section.mode = SectionMode::Box;
                                self.section.bmin = (bb.min - Vec3::splat(0.3)).to_array();
                                self.section.bmax = (bb.max + Vec3::splat(0.3)).to_array();
                            }
                        }
                        let red = matches!(self.tool, Tool::Redline(_) | Tool::RedErase);
                        if big(ui, "✏", "Markup", "Markups on the current view (cloud; more in Review › Redline) - saved with a viewpoint", red, true) {
                            self.cur_measure.clear();
                            self.tool = if red { Tool::Select } else { Tool::Redline(crate::markup::RedKind::Cloud) };
                        }
                        let (ic, lb, tip) = if self.cam.ortho { ("⬚", "Ortho", "Orthographic - click for perspective") } else { ("🔭", "Persp", "Perspective - click for orthographic") };
                        if big(ui, ic, lb, tip, false, true) {
                            self.cam.ortho = !self.cam.ortho;
                        }
                        // light and ray trace, one click away
                        let lid = egui::Id::new("nav_light_open");
                        let light_open: bool = ui.data(|d| d.get_temp(lid)).unwrap_or(false);
                        if big(ui, "💡", "Light", "Brightness, ambient light, sun and shadows", light_open, true) {
                            ui.data_mut(|d| d.insert_temp(lid, !light_open));
                        }
                        if big(ui, "✨", "Ray Trace", "Path-traced view on the graphics card: refines while the camera stands still (like Navisworks)", self.rt.on, self.loaded_models().next().is_some()) {
                            self.toggle_raytrace();
                        }
                        // walking: path, playback, speed - always on the bar (fixed width: the buttons do not move under the mouse
                        // when you change the navigation mode)
                        sep(ui);
                        if big(ui, "🛤", "Path", "Draw a walk path: click points on the floor, double-click / Enter to finish (switches to Walk)", self.tool == Tool::WalkPath, true) {
                            self.path_run = None;
                            if self.tool == Tool::WalkPath {
                                self.tool = Tool::Select;
                            } else {
                                self.nav = NavMode::Walk;
                                self.tool = Tool::WalkPath;
                            }
                        }
                        let playing = self.path_run.as_ref().map(|r| !r.paused).unwrap_or(false);
                        if big(ui, if playing { "⏸" } else { "▶" }, if playing { "Pause" } else { "Play" }, "Walk along the drawn path (Space) · Esc stops", playing, self.walk_path.len() >= 2) {
                            self.nav = NavMode::Walk;
                            self.walk_path_play();
                        }
                        let mut v = self.settings.walk_speed;
                        let r = ui.add_sized([62.0, 22.0], egui::DragValue::new(&mut v).range(0.2..=20.0).speed(0.05).max_decimals(1).suffix(" m/s")).on_hover_text("Walk speed (drag or type)");
                        if r.changed() {
                            self.settings.walk_speed = v;
                        }
                        if r.drag_stopped() || r.lost_focus() {
                            self.settings.save();
                        }
                    });
                });
        });
        // the light popup, just above the navigation bar
        let lid = egui::Id::new("nav_light_open");
        if ctx.data(|d| d.get_temp::<bool>(lid)).unwrap_or(false) {
            let area = egui::Area::new(egui::Id::new("nav_light_popup")).fixed_pos(pos - EV::new(0.0, 46.0)).pivot(Align2::CENTER_BOTTOM).order(egui::Order::Foreground).show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| self.quick_light_ui(ui)).inner
            });
            // closes with "More lighting…", Esc, or a click outside it (not on the bar button itself)
            let outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && p.y < pos.y - 40.0));
            if area.inner || outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                ctx.data_mut(|d| d.insert_temp(lid, false));
            }
        }
    }
}
