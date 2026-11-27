//! Docked and floating panels: dock areas, strips, flyouts, panel menus and bodies.
use super::*;

impl App {
    // ================================================================== arrangeable windows
    pub(super) fn panel_menu(&mut self, ui: &mut egui::Ui, p: Panel) {
        let cur = self.settings.layout.get(p).map(|s| s.dock);
        for d in [Dock::Left, Dock::Right, Dock::Bottom, Dock::Float] {
            if Some(d) != cur && ui.button(d.label()).clicked() {
                self.settings.layout.move_to(p, d);
                ui.close_menu();
            }
        }
        if p == Panel::Clash && ui.button("⛶ Focus mode").clicked() {
            self.settings.layout.open(p);
            self.clash.focus_mode = true;
            ui.close_menu();
        }
        ui.separator();
        if ui.button("✖ Close").clicked() {
            self.settings.layout.close(p);
            ui.close_menu();
        }
    }

    pub(super) fn panel_body(&mut self, ui: &mut egui::Ui, p: Panel) {
        match p {
            Panel::Tree => {
                // search in the model (name, type, level, GUID): the results replace the tree while searching
                let sid = ui.make_persistent_id("tree_search");
                let mut q: String = ui.data_mut(|d| d.get_temp(sid)).unwrap_or_default();
                ui.add(egui::TextEdit::singleline(&mut q).hint_text("🔍 Search model").desired_width(f32::INFINITY));
                ui.data_mut(|d| d.insert_temp(sid, q.clone()));
                let ql = q.trim().to_lowercase();
                if ql.is_empty() {
                    self.tree_config_ui(ui);
                    let acts = egui::ScrollArea::both().id_salt("tree_scroll").auto_shrink([false, false]).show(ui, |ui| self.tree_ui(ui)).inner;
                    for a in acts {
                        self.apply(a);
                    }
                } else {
                    let mut hits: Vec<(ElemId, String, String)> = Vec::new();
                    let mut total = 0usize;
                    for (mi, m) in self.loaded_models() {
                        for (ei, e) in m.elements.iter().enumerate() {
                            if e.name.to_lowercase().contains(&ql) || e.ifc_type.to_lowercase().contains(&ql) || e.storey.to_lowercase().contains(&ql) || e.guid.to_lowercase() == ql {
                                total += 1;
                                if hits.len() < 500 {
                                    hits.push((ElemId { m: mi as u32, e: ei as u32 }, e.name.clone(), format!("{} · {}", e.ifc_type, e.storey)));
                                }
                            }
                        }
                    }
                    let mut act: Option<Action> = None;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{total} found")).weak());
                        if total > 0 && ui.small_button("Select all").clicked() {
                            let all: Vec<ElemId> = self.loaded_models().flat_map(|(mi, m)| m.elements.iter().enumerate().filter(|(_, e)| e.name.to_lowercase().contains(&ql) || e.ifc_type.to_lowercase().contains(&ql) || e.storey.to_lowercase().contains(&ql) || e.guid.to_lowercase() == ql).map(move |(ei, _)| ElemId { m: mi as u32, e: ei as u32 }).collect::<Vec<_>>()).collect();
                            act = Some(Action::Select(all, false));
                        }
                    });
                    egui::ScrollArea::vertical().id_salt("tree_hits").auto_shrink([false, false]).show(ui, |ui| {
                        for (id, n, sub) in &hits {
                            let on = self.selected.contains(id);
                            let r = ui.selectable_label(on, RichText::new(n)).on_hover_text(sub.as_str());
                            if r.clicked() {
                                act = Some(Action::Select(vec![*id], ui.input(|i| i.modifiers.command)));
                            }
                            if r.double_clicked() {
                                act = Some(Action::Focus(vec![*id]));
                            }
                        }
                    });
                    if let Some(a) = act {
                        self.apply(a);
                    }
                }
            }
            Panel::PropTree => self.props_tree_ui(ui),
            Panel::Sets => self.sets_ui(ui),
            Panel::Properties => self.properties_panel(ui),
            Panel::Find => self.find_items_ui(ui),
            Panel::Clash => self.clash_ui(ui),
            Panel::TimeLiner => self.timeliner_ui(ui),
            Panel::Viewpoints => self.viewpoints_ui(ui),
            Panel::Section => self.section_ui(ui),
            Panel::Measure => self.measure_ui(ui),
            Panel::Coords => self.coords_ui(ui),
            Panel::Render => self.render_ui(ui),
            Panel::Animation => self.animations_ui(ui),
            Panel::Project => self.models_ui(ui),
            Panel::Profiler => self.profiler_ui(ui),
            Panel::Compare => self.compare_ui(ui),
            Panel::Check => self.check_ui(ui),
            Panel::Quantities => self.qto_ui(ui),
        }
    }

    /// The strip of an auto-hide zone: one button per panel (icon); hover / click opens the panel over the model.
    pub(super) fn dock_strip(&mut self, ui: &mut egui::Ui, d: Dock) {
        let tabs = self.settings.layout.in_area(d);
        let show = |ui: &mut egui::Ui, p: Panel, this: &mut Self| {
            let icon = crate::theme::ph(p.title().split_whitespace().next().unwrap_or("•")).to_string();
            let on = this.flyout.map(|f| f.1 == p).unwrap_or(false);
            let r = ui.add(egui::Button::new(RichText::new(icon).font(crate::theme::icon_font(15.0))).selected(on).min_size(EV::new(24.0, 24.0))).on_hover_text(p.title().trim_start_matches(|c: char| !c.is_alphanumeric()).to_string());
            if r.hovered() || r.clicked() {
                this.flyout = Some((d, p));
                this.flyout_seen = Instant::now();
            }
        };
        if d == Dock::Bottom {
            ui.horizontal(|ui| {
                for p in tabs {
                    show(ui, p, self);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("📌").on_hover_text("Pin the panels (always visible)").clicked() {
                        self.settings.layout.autohide[d.idx()] = false;
                        self.flyout = None;
                    }
                });
            });
        } else {
            ui.vertical_centered(|ui| {
                if ui.small_button("📌").on_hover_text("Pin the panels (always visible)").clicked() {
                    self.settings.layout.autohide[d.idx()] = false;
                    self.flyout = None;
                }
                ui.add_space(6.0);
                for p in tabs {
                    show(ui, p, self);
                }
            });
        }
    }

    /// The panel opened over the model from an auto-hide zone; it closes when the mouse leaves (or Esc / click in the model).
    pub(super) fn flyout_ui(&mut self, ctx: &egui::Context, strips: [Option<Rect>; 3]) {
        let Some((d, p)) = self.flyout else { return };
        let Some(strip) = strips[d.idx()] else {
            self.flyout = None;
            return;
        };
        let screen = ctx.screen_rect();
        let (pos, size) = match d {
            Dock::Left => (strip.right_top(), EV::new(320.0, strip.height())),
            Dock::Right => (Pos2::new(strip.left() - 340.0, strip.top()), EV::new(340.0, strip.height())),
            _ => {
                let h = (screen.height() * 0.38).clamp(200.0, 420.0);
                (Pos2::new(strip.left(), strip.top() - h), EV::new(strip.width(), h))
            }
        };
        let fr = egui::Area::new(egui::Id::new("flyout")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            egui::Frame::window(&ctx.style()).rounding(6.0).inner_margin(8.0).show(ui, |ui| {
                ui.set_min_size(size - EV::splat(16.0));
                ui.set_max_size(size - EV::splat(16.0));
                ui.horizontal(|ui| {
                    ui.label(crate::theme::title_job(p.title(), 14.0, ui.visuals().strong_text_color()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✖").on_hover_text("Hide").clicked() {
                            self.flyout = None;
                        }
                        if ui.small_button("📌").on_hover_text("Pin this area (panels always visible)").clicked() {
                            self.settings.layout.autohide[d.idx()] = false;
                            self.settings.layout.active[d.idx()] = Some(p);
                            self.flyout = None;
                        }
                    });
                });
                ui.separator();
                egui::ScrollArea::both().id_salt(("flyout_scroll", p)).auto_shrink([false, false]).show(ui, |ui| self.panel_body(ui, p));
            });
        });
        let area = fr.response.rect;
        // stays open while the mouse is on the panel / on the strip, while typing or dragging something
        let hover = ctx.pointer_hover_pos().is_some_and(|h| area.expand(6.0).contains(h) || strip.contains(h));
        let busy = ctx.wants_keyboard_input() || ctx.is_using_pointer() || ctx.memory(|m| m.any_popup_open());
        if hover || busy {
            self.flyout_seen = Instant::now();
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) || self.flyout_seen.elapsed().as_secs_f32() > 0.6 {
            self.flyout = None;
        }
        if self.flyout.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    pub(super) fn dock_area(&mut self, ui: &mut egui::Ui, d: Dock) {
        let tabs = self.settings.layout.in_area(d);
        let active = self.settings.layout.active[d.idx()];
        if d == Dock::Bottom {
            // the bottom zone: the tabs are in the status bar; here only the title and the window controls
            ui.horizontal(|ui| {
                if let Some(a) = active {
                    ui.label(crate::theme::title_job(a.title(), 14.0, ui.visuals().strong_text_color()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let tall = self.settings.layout.bottom_tall;
                        let glyph = if tall { "⤡" } else { "⤢" };
                        if ui.add(egui::Button::new(RichText::new(glyph).size(16.0)).frame(false)).on_hover_text(if tall { "Restore the panel height" } else { "Maximize this panel (more room for clashes and tests)" }).clicked() {
                            self.settings.layout.bottom_tall = !tall;
                            self.settings.save();
                        }
                        if a == Panel::Clash && ui.button("⛶ Focus").on_hover_text("Open Clash Detective in a large focus window (Esc to exit)").clicked() {
                            self.settings.layout.open(Panel::Clash);
                            self.clash.focus_mode = true;
                        }
                        if ui.button("⧉ Float").on_hover_text("Open this panel in a large resizable floating window").clicked() {
                            self.settings.layout.move_to(a, Dock::Float);
                            ui.close_menu();
                        }
                        if ui.add(egui::Button::new(RichText::new(egui_phosphor::regular::CARET_DOWN).font(crate::theme::icon_font(15.0))).frame(false)).on_hover_text("Lower the panel (click its tab in the status bar to raise it again)").clicked() {
                            self.settings.layout.bottom_open = false;
                        }
                        if ui.add(egui::Button::new(RichText::new(egui_phosphor::regular::X).font(crate::theme::icon_font(14.0))).frame(false)).on_hover_text("Close this window").clicked() {
                            self.settings.layout.close(a);
                        }
                        ui.menu_button(RichText::new(egui_phosphor::regular::DOTS_THREE).font(crate::theme::icon_font(15.0)), |ui| self.panel_menu(ui, a)).response.on_hover_text("Move this window (dock left / right, float)");
                    });
                }
            });
            ui.add_space(2.0);
        } else if d == Dock::Right {
            // The right panel is only Properties: the title on top and the content over the full height.
            if let Some(a) = active {
                ui.horizontal(|ui| {
                    ui.label(crate::theme::title_job(a.title(), 14.0, ui.visuals().strong_text_color()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.menu_button(RichText::new(egui_phosphor::regular::DOTS_THREE).font(crate::theme::icon_font(15.0)), |ui| self.panel_menu(ui, a));
                    });
                });
                ui.add_space(3.0);
                egui::ScrollArea::vertical().id_salt(("side_scroll", d.idx())).auto_shrink([false, false]).show(ui, |ui| self.panel_body(ui, a));
            }
            return;
        } else {
        ui.horizontal_wrapped(|ui| {
            ui.style_mut().spacing.item_spacing.x = 3.0;
            // auto-hide: the zone becomes a thin strip, the panels appear over the model when you touch them
            if ui.small_button("📌").on_hover_text("Auto-hide this area: panels slide in over the model when you point at their tab").clicked() {
                self.settings.layout.autohide[d.idx()] = true;
            }
            // tabs as a segmented control (a card with "pills"): no outline, active = a lighter surface
            let seg_on = ui.visuals().widgets.hovered.weak_bg_fill;
            egui::Frame::none().fill(card_fill(ui.visuals())).rounding(8.0).inner_margin(3.0).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for p in &tabs {
                        let on = Some(*p) == active;
                        // in the bottom zone the icon stays too; on the sides only the name (more compact)
                        let t = p.short().to_string();
                        let txt = if on { RichText::new(t).size(13.0).strong() } else { RichText::new(t).size(13.0).color(ui.visuals().weak_text_color()) };
                        ui.spacing_mut().button_padding = egui::vec2(7.0, 2.0);
                        let b = egui::Button::new(txt).fill(if on { seg_on } else { Color32::TRANSPARENT }).stroke(Stroke::NONE).rounding(6.0);
                        let r = ui.add(b).on_hover_text(format!("{}\nRight-click: dock left / right / bottom, float or close", p.title()));
                        if r.clicked() {
                            self.settings.layout.active[d.idx()] = Some(*p);
                        }
                        r.context_menu(|ui| self.panel_menu(ui, *p));
                    }
                });
            });
            if d == Dock::Right {
                ui.menu_button("＋ Add tab", |ui| {
                    ui.label(RichText::new("Bring a panel into the Properties bar as a tab").small().weak());
                    ui.separator();
                    for p in crate::layout::ALL_PANELS {
                        if !tabs.contains(&p) && ui.button(p.title()).clicked() {
                            self.settings.layout.move_to(p, Dock::Right);
                            ui.close_menu();
                        }
                    }
                }).response.on_hover_text("Add any panel here (Blender-style properties tabs)");
            }
            if let Some(a) = active {
                ui.add_space(4.0);
                ui.menu_button("⏷", |ui| self.panel_menu(ui, a)).response.on_hover_text("Move or close this window");
                // sides: closing is in the ⏷ menu (the tabs stay on a single row)
            }
        });
        ui.add_space(4.0);
        }
        if let Some(a) = self.settings.layout.active[d.idx()] {
            if d == Dock::Bottom {
                // the content may not keep the panel tall: limited to the chosen height (it can be dragged freely)
                let h = ui.available_height().max(40.0);
                let size = EV::new(ui.available_width(), h);
                ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_max_height(h);
                    let clip = ui.max_rect().intersect(ui.clip_rect());
                    ui.set_clip_rect(clip);
                    // panels without their own scrolling: tall content scrolls inside the panel,
                    // it no longer pushes the bottom panel over the 3D view
                    if matches!(a, Panel::Project | Panel::Section | Panel::Measure | Panel::Coords | Panel::Profiler | Panel::Compare) {
                        egui::ScrollArea::vertical().id_salt(("bottom_scroll", a)).max_height(h).auto_shrink([false, false]).show(ui, |ui| self.panel_body(ui, a));
                    } else {
                        self.panel_body(ui, a);
                    }
                });
            } else {
                // the side panels may not push the viewport: wide content scrolls
                egui::ScrollArea::horizontal().id_salt(("side_scroll", d.idx())).auto_shrink([false, false]).show(ui, |ui| self.panel_body(ui, a));
            }
        }
    }

    /// Model focus: only a small exit button over the model; Esc restores the layout.
    pub(super) fn immersive_overlay(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.toggle_immersive();
            return;
        }
        egui::Area::new(egui::Id::new("immersive_exit"))
            .anchor(Align2::LEFT_TOP, EV::new(10.0, 10.0))
            .show(ctx, |ui| {
                let b = egui::Button::new(RichText::new("⛶ Exit model focus · Esc").size(12.0))
                    .fill(accent().gamma_multiply(0.35))
                    .rounding(8.0);
                if ui.add(b).clicked() {
                    self.toggle_immersive();
                }
            });
    }

    pub(super) fn float_windows(&mut self, ctx: &egui::Context) {
        for p in self.settings.layout.in_area(Dock::Float) {
            let mut open = true;
            let max = self.float_max.contains(&p);
            let screen = ctx.screen_rect();
            let size = if max {
                [(screen.width() - 40.0).max(640.0), (screen.height() - 90.0).max(480.0)]
            } else if p == Panel::Clash {
                // no larger than ~60% of the screen: the 3D view stays visible
                [1100.0f32.min(screen.width() * 0.62), 760.0f32.min(screen.height() * 0.6)]
            } else {
                [560.0f32.min(screen.width() * 0.5), 400.0f32.min(screen.height() * 0.5)]
            };
            // the id includes the state: on toggle egui recreates the window with the new size
            egui::Window::new(crate::theme::title_job(p.title(), 14.0, ctx.style().visuals.text_color()))
                .id(egui::Id::new(("float_panel", p, max)))
                .open(&mut open)
                .default_size(size)
                .min_width(260.0)
                .min_height(160.0)
                .resizable(true)
                .collapsible(true)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let lbl = if max { "⤡ Restore size" } else { "⤢ Stretch to screen" };
                        if ui.button(lbl).on_hover_text("Maximize the window; drag any edge or corner to stretch it").clicked() {
                            if max {
                                self.float_max.remove(&p);
                            } else {
                                self.float_max.insert(p);
                            }
                        }
                        ui.menu_button("⏷ Dock…", |ui| self.panel_menu(ui, p));
                    });
                    // the content may not enlarge the window beyond the screen (long tables, lists):
                    // it stays at the window size, which can be dragged by its edges
                    let h = ui.available_height().clamp(80.0, (screen.height() - 120.0).max(200.0));
                    let size = EV::new(ui.available_width(), h);
                    ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_max_height(h);
                        let clip = ui.max_rect().intersect(ui.clip_rect());
                        ui.set_clip_rect(clip);
                        self.panel_body(ui, p);
                    });
                });
            if !open {
                self.settings.layout.close(p);
            }
        }
    }
}
