//! Ribbon: the tabs, their groups and buttons (and the view templates they use).
use super::*;

impl App {
    pub(super) fn ribbon(&mut self, ctx: &egui::Context) {
        use crate::theme::RibbonMode;
        const TABS: [&str; 14] = ["Home", "Select", "Viewpoint", "Review", "Sections", "Annotation", "BIM", "Coordination", "Analyze", "Render", "Export", "Share", "Script", "View"];
        // setarile vechi: "ribbon restrans" = treapta Mini
        if self.settings.ribbon_min {
            self.settings.ribbon_min = false;
            self.settings.ribbon_mode = RibbonMode::Mini;
        }
        if self.ribbon_tab >= TABS.len() {
            self.ribbon_tab = 0;
        }
        let mode = self.settings.ribbon_mode;
        crate::theme::RIBBON_COMPACT.store(mode == RibbonMode::Compact, std::sync::atomic::Ordering::Relaxed);
        crate::theme::COLOR_ICONS.store(self.settings.color_icons, std::sync::atomic::Ordering::Relaxed);
        let fill = ctx.style().visuals.panel_fill;
        // bara de sus: cautare / focus / tema / setari / proiect - separata de ribbon
        self.app_bar(ctx);
        egui::TopBottomPanel::top("ribbon").show_separator_line(false).frame(egui::Frame::none().fill(fill).inner_margin(egui::Margin { left: 8.0, right: 8.0, top: 5.0, bottom: 5.0 })).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.style_mut().spacing.item_spacing.x = 3.0;
                // accentul Nord e deschis (cyan): text inchis pentru contrast
                let ftxt = match self.settings.theme { Theme::Nord => Color32::from_rgb(46, 52, 64), Theme::TokyoNight => Color32::from_rgb(26, 27, 38), _ => Color32::WHITE };
                let fb = egui::Button::new(crate::theme::icon_job("≡", crate::i18n::tr("File "), 14.0, ftxt)).fill(accent()).rounding(crate::theme::RADIUS_MD).min_size(EV::new(78.0, 26.0));
                let r = ui.add(fb);
                let popup = ui.make_persistent_id("file_menu");
                if r.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(popup));
                }
                egui::popup_below_widget(ui, popup, &r, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| self.file_menu(ui));
                ui.add_space(8.0);
                // Tab-urile au acum toata latimea: bara de sus e separata.
                // Scroll-ul ramane doar pentru ferestre foarte inguste.
                // fereastra ingusta: tab-urile se strang (latime minima mai mica, text putin mai mic) ca sa
                // se vada toate 14, fara derulare
                let tabs_w = ui.available_width();
                let (tab_min, tab_font, tab_pad) = if tabs_w >= 14.0 * 81.0 {
                    (78.0, None, 6.0)
                } else if tabs_w >= 14.0 * 66.0 {
                    (0.0, None, 5.0)
                } else {
                    (0.0, Some(12.0), 3.0)
                };
                egui::ScrollArea::horizontal().id_salt("ribbon_tabs").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.style_mut().spacing.item_spacing.x = if tab_min > 0.0 { 3.0 } else { 1.0 };
                        ui.style_mut().spacing.button_padding.x = tab_pad;
                        for (i, t) in TABS.iter().enumerate() {
                    let on = self.ribbon_tab == i && mode != RibbonMode::Mini;
                    let t = crate::i18n::tr(t);
                    let mut txt = if on { RichText::new(t).color(accent()).strong() } else { RichText::new(t) };
                    if let Some(sz) = tab_font {
                        txt = txt.size(sz);
                    }
                    let b = egui::Button::new(txt).fill(if on { accent().gamma_multiply(0.16) } else { Color32::TRANSPARENT }).stroke(Stroke::NONE).rounding(crate::theme::RADIUS_LG).min_size(EV::new(tab_min, 24.0));
                    let r = ui.add(b);
                    if on {
                        // subliniere subtila a tab-ului activ (ca in Office / Navisworks 2024)
                        let rr = r.rect;
                        ui.painter().line_segment(
                            [Pos2::new(rr.min.x + 7.0, rr.max.y + 2.0), Pos2::new(rr.max.x - 7.0, rr.max.y + 2.0)],
                            Stroke::new(2.0_f32, accent()),
                        );
                    }
                    if r.double_clicked() {
                        // ca in Office: dublu-click pe tab = Mini / inapoi
                        self.settings.ribbon_mode = if mode == RibbonMode::Mini { RibbonMode::Full } else { RibbonMode::Mini };
                        self.settings.save();
                    } else if r.clicked() {
                        if mode == RibbonMode::Mini {
                            self.settings.ribbon_mode = RibbonMode::Full;
                            self.settings.save();
                        }
                        self.ribbon_tab = i;
                    }
                            }
                        });
                    });
                });
            if mode == RibbonMode::Mini {
                return;
            }
            ui.add_space(4.0);
            // inaltime FIXA pe toate tab-urile (nu mai "sare" cand schimbi tab-ul)
            let h = mode.height();
            // ribbon-ul se potriveste in fereastra: latimea butoanelor (62 px) scade automat pe tab-ul
            // curent cat sa incapa tot (minim 38 px), si creste la loc cand e spatiu - fara bara de scroll
            let avail_w = ui.available_width();
            let wid = egui::Id::new(("ribbon_btn_w", self.ribbon_tab));
            let w_now: f32 = ui.data(|d| d.get_temp(wid)).unwrap_or(62.0);
            crate::theme::set_ribbon_btn_w(w_now);
            // daca nici cu butoanele cele mai inguste nu incape: tab-ul trece pe iconite (numele in tooltip)
            // pana cand fereastra e iar destul de lata (latimea necesara e tinuta minte)
            let cid = egui::Id::new(("ribbon_auto_compact", self.ribbon_tab));
            let auto_compact: Option<f32> = ui.data(|d| d.get_temp(cid));
            if let Some(need) = auto_compact {
                if avail_w > need + 10.0 {
                    ui.data_mut(|d| d.remove::<f32>(cid));
                    ui.ctx().request_repaint();
                } else if mode != RibbonMode::Compact {
                    crate::theme::RIBBON_COMPACT.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            let forced_compact = auto_compact.is_some() && mode != RibbonMode::Compact;
            let out = egui::ScrollArea::horizontal().id_salt("ribbon_scroll").max_height(h).scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(h);
                    ui.set_max_height(h);
                    // golul dintre carduri separa grupele - nu linii
                    ui.spacing_mut().item_spacing = egui::vec2(5.0, 1.0);
                    ui.spacing_mut().interact_size.y = 16.0;
                    ui.spacing_mut().button_padding = egui::vec2(6.0, 1.0);
                    // tranzitie la schimbarea tab-ului: continutul apare cu fade
                    let now = ui.input(|i| i.time);
                    let fid = egui::Id::new("ribbon_fade");
                    let (ft, fs): (usize, f64) = ui.data(|d| d.get_temp(fid)).unwrap_or((usize::MAX, 0.0));
                    let start = if ft != self.ribbon_tab { ui.data_mut(|d| d.insert_temp(fid, (self.ribbon_tab, now))); now } else { fs };
                    let alpha = ((now - start) / 0.16).clamp(0.0, 1.0) as f32;
                    if alpha < 1.0 {
                        ui.multiply_opacity(0.25 + 0.75 * alpha);
                        ui.ctx().request_repaint();
                    }
                    // pagina de start: ribbon-ul se vede, dar e inactiv pana se deschide un model / proiect
                    if self.start_screen() {
                        ui.disable();
                    }
                    match self.ribbon_tab {
                        0 => {
                            self.ribbon_home(ui);
                            self.ribbon_select(ui);
                            self.visibility_group(ui);
                            self.ribbon_display(ui);
                        }
                        1 => {
                            self.ribbon_select(ui);
                            let has = !self.selected.is_empty();
                            rgroup(ui, "Select Similar", |ui| {
                                if rbtn(ui, "⧉", "Same Type", "Every visible item of the same type as the selection (Revit Family and Type / IFC type)  Ctrl+Shift+A", false, has).clicked() {
                                    self.select_similar(true);
                                }
                                if rbtn(ui, "▦", "Same Category", "Every visible item of the same category / IFC class (all walls, all doors...)", false, has).clicked() {
                                    self.select_similar(false);
                                }
                            });
                            rgroup(ui, "Live Link", |ui| {
                                let on = self.settings.sync_selection;
                                if rbtn(ui, "🔗", "Sync Selection", "Selection follows Revit and Navisworks, both ways:\nselect here and the elements are selected in Revit / Navisworks, and the other way round.\nIn Revit: ManageBIM › Sync Selection. In Navisworks: Tool Add-ins › Sync Selection (once per session).\nThe models must be the same (exported from Revit / IFC from Revit / NWC).", on, true).clicked() {
                                    self.set_sync(!on);
                                }
                                let follow = self.settings.sync_follow;
                                if rbtn(ui, "🎯", "Zoom to It", "Zoom to the elements selected in Revit / Navisworks", follow, on).clicked() {
                                    self.settings.sync_follow = !follow;
                                    self.settings.save();
                                }
                            });
                        }
                        2 => self.ribbon_viewpoint(ui),
                        3 => self.ribbon_review(ui),
                        4 => {
                            self.ribbon_sections(ui);
                            self.ribbon_grids(ui);
                        }
                        5 => self.ribbon_annotation(ui),
                        6 => self.ribbon_bim(ui),
                        7 => self.ribbon_coord(ui),
                        8 => {
                            self.ribbon_models(ui);
                            self.ribbon_relocate(ui);
                        }
                        9 => self.ribbon_render(ui),
                        10 => {
                            self.ribbon_output(ui);
                            self.ribbon_output2(ui);
                        }
                        11 => {
                            self.ribbon_share(ui);
                        }
                        12 => self.ribbon_script(ui),
                        _ => {
                            self.ribbon_view(ui);
                            self.ribbon_view2(ui);
                            self.ribbon_help(ui);
                        }
                    }
                });
            });
            // potrivirea latimii pentru cadrul urmator (doar in modul cu etichete)
            if mode != RibbonMode::Compact && !forced_compact && w_now <= 38.5 && out.content_size.x > avail_w + 1.0 {
                ui.data_mut(|d| d.insert_temp(cid, out.content_size.x));
                ui.ctx().request_repaint();
            }
            crate::theme::RIBBON_COMPACT.store(mode == RibbonMode::Compact, std::sync::atomic::Ordering::Relaxed);
            if mode != RibbonMode::Compact && !forced_compact {
                let content = out.content_size.x.max(1.0);
                // latimea ferestrei la care tab-ul a trebuit ingustat: butoanele cresc la loc doar cand
                // fereastra se mareste. Altfel grupuri care isi schimba forma dupa latime (Interface: slider
                // lat / "UI 80%") faceau bucla ingusteaza - creste - ingusteaza si tab-ul View tremura.
                let sid = egui::Id::new(("ribbon_shrunk_at", self.ribbon_tab));
                let shrunk_at: Option<f32> = ui.data(|d| d.get_temp(sid));
                let may_grow = shrunk_at.is_none_or(|a| avail_w > a + 8.0);
                let target = if content > avail_w + 1.0 {
                    ui.data_mut(|d| d.insert_temp(sid, avail_w));
                    (w_now * (avail_w - 4.0) / content).floor().max(38.0)
                } else if content < avail_w - 40.0 && w_now < 62.0 && may_grow {
                    if shrunk_at.is_some() {
                        ui.data_mut(|d| d.remove::<f32>(sid));
                    }
                    (w_now + 2.0).min(62.0)
                } else {
                    w_now
                };
                if (target - w_now).abs() >= 1.0 {
                    ui.data_mut(|d| d.insert_temp(wid, target));
                    ui.ctx().request_repaint();
                }
            }
            crate::theme::set_ribbon_btn_w(62.0);
        });
    }

    pub(super) fn panel_btn(&mut self, ui: &mut egui::Ui, p: Panel, icon: &str, label: &str, tip: &str) {
        if rbtn(ui, icon, label, tip, self.panel_shown(p), true).clicked() {
            self.settings.layout.toggle(p);
        }
    }

    pub(super) fn ribbon_home(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Project", |ui| {
            if rbtn(ui, "📂", "Open", "Open a model (replaces the scene)  Ctrl+O", false, true).clicked() {
                self.request_path(Purpose::OpenModel { append: false });
            }
            if rbtn(ui, "➕", "Append", "Append a model to the federated scene", false, true).clicked() {
                self.request_path(Purpose::OpenModel { append: true });
            }
            if rbtn(ui, "💾", "Save", "Save the project  Ctrl+S (.bimr, or the .bimd it was opened from)", false, true).clicked() {
                self.save_now();
            }
            if rbtn(ui, "📦", "Publish", "Publish .bimd: the project and all models in one file (like Navisworks NWD)", false, self.loaded_models().next().is_some()).clicked() {
                self.request_path(Purpose::PublishBimd);
            }
            if rbtn(ui, "🏠", "Start", "Start page: recent files, Open, New (the project stays open - come back with the arrow)", false, true).clicked() {
                self.show_start = true;
            }
            // Refresh: evidentiat cand unul din fisierele incarcate s-a modificat pe disc
            let has_models = self.loaded_models().next().is_some();
            let n_changed = self.changed_files.len();
            let label = if n_changed > 0 { format!("Refresh ({n_changed})") } else { "Refresh".to_string() };
            let tip = if n_changed > 0 {
                let names: Vec<String> = self.changed_files.iter().filter_map(|i| self.models.get(*i).cloned().flatten()).map(|m| m.name.clone()).collect();
                format!("Modified on disk: {}\nClick (F5) to reload them. Shift+click (Ctrl+F5) reloads all models.", names.join(", "))
            } else {
                "Reload the models whose files changed on disk  F5\nShift+click reloads all models  Ctrl+F5".to_string()
            };
            let r = rbtn(ui, "⟳", &label, &tip, n_changed > 0, has_models && self.loads.is_empty());
            if r.clicked() {
                let force = ui.input(|i| i.modifiers.shift);
                self.refresh_models(force);
            }
            self.panel_btn(ui, Panel::Project, "📁", "Models", "Project & Model Properties (location, files, history)");
            if rbtn(ui, "📊", "Dashboard", "Project summary: models, clashes, issues, checks, levels - export as a PDF or Word report", self.dash_open, true).clicked() {
                self.dash_open = !self.dash_open;
                self.dash = None;
            }
        });
    }

    pub(super) fn ribbon_select(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Select & Search", |ui| {
            if rbtn(ui, "👆", "Select", "Selection tool (Esc)", self.tool == Tool::Select, true).clicked() {
                self.cur_measure.clear();
                self.tool = Tool::Select;
            }
            if rbtn(ui, "⬚", "Window", "Window selection: drag left -> right = items fully inside, right -> left = items touching (Ctrl adds). Middle / Shift-drag still pans", self.tool == Tool::Window, true).clicked() {
                self.cur_measure.clear();
                self.tool = if self.tool == Tool::Window { Tool::Select } else { Tool::Window };
            }
            if rbtn(ui, "⬚", "Select All", "Select every visible element  Ctrl+A", false, true).clicked() {
                let all = self.all_ids();
                self.apply(Action::Select(all, false));
            }
            let has = !self.selected.is_empty();
            if rbtn(ui, "🔲", "Clear", "Clear the selection", false, has).clicked() {
                self.clear_selection();
            }
            self.panel_btn(ui, Panel::Find, "🔍", "Find Items", "Search by any property, save as search set  Ctrl+F");
            self.panel_btn(ui, Panel::Sets, "📋", "Sets", "Selection & search sets");
        });
    }

    pub(super) fn ribbon_display(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Display", |ui| {
            self.panel_btn(ui, Panel::Tree, "🌲", "Tree", "Selection tree");
            self.panel_btn(ui, Panel::PropTree, "🔎", "Prop Tree", "Browse all properties of all models");
            self.panel_btn(ui, Panel::Properties, "ℹ", "Properties", "Properties of the selected element");
            self.panel_btn(ui, Panel::Profiler, "🎨", "Profiler", "Appearance Profiler: colour the model by any property");
        });
    }

    pub(super) fn visibility_group(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Visibility", |ui| {
            let sel: Vec<ElemId> = self.selected.iter().copied().collect();
            if rbtn(ui, "🚫", "Hide", "Hide selection (H)", false, !sel.is_empty()).clicked() {
                self.apply(Action::Hide(sel.clone()));
            }
            if rbtn(ui, "◎", "Hide Unsel.", "Hide everything not selected (I)", false, !sel.is_empty()).clicked() {
                self.apply(Action::Isolate(sel.clone()));
            }
            if rbtn(ui, "👁", "Unhide All", "Show all hidden elements", false, true).clicked() {
                self.apply(Action::ShowAll);
                self.clash.focus = None;
            }
        });
    }

    pub(super) fn ribbon_viewpoint(&mut self, ui: &mut egui::Ui) {
        let bb = self.scene_aabb();
        rgroup(ui, "Navigation Mode", |ui| {
            if rbtn(ui, "🔄", "Orbit", "Orbit: left-drag rotates around the point under the cursor", self.nav == NavMode::Orbit, true).clicked() {
                self.nav = NavMode::Orbit;
            }
            if rbtn(ui, "✋", "Pan", "Pan: left-drag moves the model sideways", self.nav == NavMode::Pan, true).clicked() {
                self.nav = NavMode::Pan;
            }
            if rbtn(ui, "🚶", "Walk", "Walk: drag up / down, W A S D / arrows", self.nav == NavMode::Walk, true).clicked() {
                self.nav = NavMode::Walk;
            }
            // Toate controalele sunt butoane de dimensiune fixa; setarile se schimba in popup.
            let spd = rbtn(ui, "⏩", &format!("{:.1}", self.settings.walk_speed), "Walk speed (m/s)", false, true);
            let sid = ui.make_persistent_id("walk_speed_pop");
            if spd.clicked() {
                ui.memory_mut(|m| m.toggle_popup(sid));
            }
            egui::popup_below_widget(ui, sid, &spd, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(220.0);
                let mut v = self.settings.walk_speed;
                ui.label("Walk speed");
                if ui.add(egui::Slider::new(&mut v, 0.2..=20.0).logarithmic(true).suffix(" m/s").max_decimals(1)).changed() {
                    self.settings.walk_speed = v;
                }
                ui.horizontal(|ui| {
                    for (n, x) in [("Slow", 0.7), ("Normal", 1.4), ("Fast", 4.0)] {
                        if ui.button(n).clicked() {
                            self.settings.walk_speed = x;
                        }
                    }
                });
                if (self.settings.walk_speed - self.walk_saved).abs() > 1e-4 {
                    self.walk_saved = self.settings.walk_speed;
                    self.settings.save();
                }
            });
            if rbtn(ui, "⬇", "Gravity", "Walk on floors and stairs at eye height", self.settings.gravity, true).clicked() {
                self.settings.gravity = !self.settings.gravity;
                if self.settings.gravity { self.nav = NavMode::Walk; }
                self.settings.save();
            }
            if rbtn(ui, "▣", "Collision", "Stop at walls and objects", self.settings.collision, true).clicked() {
                self.settings.collision = !self.settings.collision;
                if self.settings.collision { self.nav = NavMode::Walk; }
                self.settings.save();
            }
            let eye = rbtn(ui, "👁", &format!("{:.2}", self.settings.eye_height), "Eye height (m)", false, true);
            let eid = ui.make_persistent_id("eye_pop");
            if eye.clicked() {
                ui.memory_mut(|m| m.toggle_popup(eid));
            }
            egui::popup_below_widget(ui, eid, &eye, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(180.0);
                let mut h = self.settings.eye_height;
                if ui.add(egui::Slider::new(&mut h, 0.5..=3.0).suffix(" m")).changed() {
                    self.settings.eye_height = h;
                    self.settings.save();
                }
            });
        });
        rgroup(ui, "Walk Path", |ui| {
            let drawing = self.tool == Tool::WalkPath;
            if rbtn(ui, "✏", "Draw", "Draw a walk path: click points on the floor, double-click / Enter to finish, Backspace to undo a point", drawing, self.loaded_models().next().is_some()).clicked() {
                self.path_run = None;
                self.tool = if drawing { Tool::Select } else { Tool::WalkPath };
                if !drawing {
                    self.nav = NavMode::Walk;
                }
            }
            let ok = self.walk_path.len() >= 2;
            let playing = self.path_run.as_ref().map(|r| !r.paused).unwrap_or(false);
            if rbtn(ui, if playing { "⏸" } else { "▶" }, if playing { "Pause" } else { "Play" }, "Walk along the path at the walk speed, looking ahead (Space)", playing, ok).clicked() {
                self.walk_path_play();
            }
            if rbtn(ui, "⏹", "Stop", "Stop walking the path (Esc)", false, self.path_run.is_some()).clicked() {
                self.walk_path_stop();
            }
            if rbtn(ui, "🎞", "To Animation", "Save the path as an animation (Animator) - edit it, play it or export a video", false, ok).clicked() {
                self.walk_path_to_animation();
            }
            if rbtn(ui, "🧭", "To Navisworks", "Send the path to Navisworks: ManageBIM › Walk path there turns it into a viewpoint animation (Saved Viewpoints) at the same walk speed and eye height", false, ok).clicked() {
                self.send_walk_path_to_navisworks();
            }
            if rbtn(ui, "🗑", "Clear", "Delete the walk path", false, !self.walk_path.is_empty()).clicked() {
                self.walk_path.clear();
                self.path_run = None;
            }
        });
        rgroup(ui, "Projection", |ui| {
            if rbtn(ui, "🔭", "Perspective", "Perspective camera", !self.cam.ortho, true).clicked() {
                self.cam.ortho = false;
            }
            if rbtn(ui, "⬚", "Orthographic", "Orthographic (parallel) camera - no perspective distortion", self.cam.ortho, true).clicked() {
                self.cam.ortho = true;
            }
        });
        rgroup(ui, "Home", |ui| {
            if rbtn(ui, "🏠", "Home", "Go to the Home view (Home key)", false, true).clicked() {
                self.go_home();
            }
            if rbtn(ui, "📌", "Set Home", "Save the current view as Home (stored in the .bimr project)", self.home.is_some(), true).clicked() {
                self.home = Some(self.cam);
                self.toast("Current view set as Home.");
            }
        });
        rgroup(ui, "Navigate", |ui| {
            if rbtn(ui, "⛶", "Zoom All", "Zoom to the whole model", false, true).clicked() {
                let mut t = self.cam;
                t.fit(&bb);
                self.fly_to(t);
            }
            if rbtn(ui, "🎯", "Focus", "Focus on the selection (F)", false, !self.selected.is_empty()).clicked() {
                let s: Vec<ElemId> = self.selected.iter().copied().collect();
                self.apply(Action::Focus(s));
            }
        });
        rgroup(ui, "Standard Views", |ui| {
            for (n, ic, yaw, pitch) in [("Top", "⏶", FRAC_PI_2, -1.55), ("Front", "⏹", FRAC_PI_2, 0.0), ("Right", "⏵", PI, 0.0), ("Back", "⏺", -FRAC_PI_2, 0.0), ("Left", "⏴", 0.0, 0.0), ("Iso", "🔭", FRAC_PI_4, -0.6)] {
                if rbtn(ui, ic, n, &format!("{n} view"), false, true).clicked() {
                    let mut t = self.cam;
                    t.set_view(yaw, pitch, &bb);
                    self.fly_to(t);
                }
            }
        });
        rgroup(ui, "Saved Viewpoints", |ui| {
            if rbtn(ui, "📷", "Save View", "Save the current view as a viewpoint", false, true).clicked() {
                self.save_viewpoint();
                self.open_panel(Panel::Viewpoints);
            }
            self.panel_btn(ui, Panel::Viewpoints, "🖼", "Viewpoints", "Saved viewpoints");
        });
        rgroup(ui, "Camera", |ui| {
            let fov = rbtn(ui, "◎", &format!("{:.0}°", self.cam.fov_deg), "Field of view", false, true);
            let id = ui.make_persistent_id("fov_pop");
            if fov.clicked() {
                ui.memory_mut(|m| m.toggle_popup(id));
            }
            egui::popup_below_widget(ui, id, &fov, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(200.0);
                ui.add(egui::Slider::new(&mut self.cam.fov_deg, 25.0..=90.0).suffix("°"));
            });
        });
    }

    pub(super) fn ribbon_review(&mut self, ui: &mut egui::Ui) {
        self.ribbon_redline(ui);
        self.ribbon_issues(ui);
        rgroup(ui, "Measure", |ui| {
            for (k, ic, n) in [(MeasureKind::PointToPoint, "📏", "Point-Point"), (MeasureKind::Polyline, "〰", "Polyline"), (MeasureKind::Angle, "∠", "Angle"), (MeasureKind::Area, "⬛", "Area")] {
                let on = self.tool == Tool::Measure(k);
                if rbtn(ui, ic, n, k.label(), on, true).clicked() {
                    self.cur_measure.clear();
                    self.tool = if on { Tool::Select } else { Tool::Measure(k) };
                }
            }
            if rbtn(ui, "🗑", "Clear", "Delete all measurements", false, !self.measures.is_empty()).clicked() {
                self.measures.clear();
                self.cur_measure.clear();
            }
            self.panel_btn(ui, Panel::Measure, "📊", "List", "Measurements panel");
        });
        // ca in Navisworks (Measure > Transform): relocare cu puncte alese in model
    }

    pub(super) fn ribbon_relocate(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Relocate", |ui| {
            let cur = self.xform.as_ref().map(|x| x.mode);
            let has = self.loaded_models().next().is_some();
            for (m, ic, n, tip) in [
                (crate::app::XformMode::Move, "✥", "Move", "Move a model point to point (snaps to corners)"),
                (crate::app::XformMode::Rotate, "⟳", "Rotate", "Rotate a model about a picked point (current direction -> new direction)"),
                (crate::app::XformMode::Align, "⇥", "Align", "Align a face of a model against a target face (rotation in plan + placing it on the face)"),
            ] {
                if rbtn(ui, ic, n, tip, cur == Some(m), has).clicked() {
                    if cur == Some(m) {
                        self.xform = None;
                    } else {
                        let sel = self.relocate_selection;
                        self.xform_start(m, sel);
                    }
                }
            }
            let sel = rbtn(ui, "◎", if self.relocate_selection { "Selection" } else { "Model" }, "Relocate: whole model or only the selection", false, true);
            let sid = ui.make_persistent_id("reloc_sel_pop");
            if sel.clicked() {
                ui.memory_mut(|m| m.toggle_popup(sid));
            }
            egui::popup_below_widget(ui, sid, &sel, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(180.0);
                if ui.selectable_label(!self.relocate_selection, "Model").clicked() {
                    self.relocate_selection = false;
                    ui.close_menu();
                }
                if ui.selectable_label(self.relocate_selection, "Selection").clicked() {
                    self.relocate_selection = true;
                    ui.close_menu();
                }
            });
        });
    }

    pub(super) fn ribbon_sections(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Sectioning", |ui| {
            let sec = self.section.enabled;
            if rbtn(ui, "✂", "Enable", "Turn sectioning on / off", sec, true).clicked() {
                self.section.enabled = !sec;
                if !sec {
                    self.init_section_defaults();
                    self.open_panel(Panel::Section);
                }
            }
            if rbtn(ui, "⬚", "Fit Section", "Section box around the selected elements, and zoom to them (also: right-click > Section box around selection)", false, !self.selected.is_empty()).clicked() {
                self.section_to_selection();
            }
            if rbtn(ui, "▣", "Model box", "Section box around the whole model - stretch it with the arrows on its faces (like Revit)", false, self.loaded_models().next().is_some()).clicked() {
                let bb = self.scene_aabb();
                if bb.is_valid() {
                    self.section.enabled = true;
                    self.section.mode = SectionMode::Box;
                    self.section.bmin = (bb.min - Vec3::splat(0.3)).to_array();
                    self.section.bmax = (bb.max + Vec3::splat(0.3)).to_array();
                    self.section_level = None;
                }
            }
            if rbtn(ui, "🔗", "Follow", "Section box follows the selection: every time you select elements the box fits around them (the camera stays)", self.section_follow, true).clicked() {
                self.section_follow = !self.section_follow;
                self.section_sel_sig = 0;
            }
            self.panel_btn(ui, Panel::Section, "⚙", "Planes", "Section planes & box");
            // contur cub on/off (inainte era o bifa „Hide section box" care facea invers)
            if rbtn(ui, "▣", "Box outline", "Show the outline of the section box and its face arrows in 3D. The cut and all exports stay active when it is off.", self.settings.show_section_box, true).clicked() {
                self.settings.show_section_box = !self.settings.show_section_box;
                self.settings.save();
            }
            // exporturile (DWG / PDF / Title Block / Overlay / Preview) sunt DOAR în tab-ul Annotation
        });
        // acces rapid: nivelul, sus / jos, box de nivel sau taietura de plan
        let levels = self.scene_grids().levels;
        if !levels.is_empty() {
            rgroup(ui, "Level", |ui| {
                let li = self.section_level.unwrap_or(0).min(levels.len() - 1);
                let cur = &levels[li].0;
                let lev = rbtn(ui, "▤", cur, "Current level (click to choose)", false, true);
                let lid = ui.make_persistent_id("rib_lev_pop");
                if lev.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(lid));
                }
                egui::popup_below_widget(ui, lid, &lev, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                    ui.set_min_width(180.0);
                    for (i, l) in levels.iter().enumerate().rev() {
                        if ui.selectable_label(self.section_level == Some(i), &l.0).clicked() {
                            self.section_level_box(i);
                            ui.close_menu();
                        }
                    }
                });
                if rbtn(ui, "▼", "Below", "Level below (PgDn)", false, true).clicked() {
                    self.section_level_step(-1);
                }
                if rbtn(ui, "▲", "Above", "Level above (PgUp)", false, true).clicked() {
                    self.section_level_step(1);
                }
                if rbtn(ui, "▣", "Box", "Section box on this level (floor to next)", false, true).clicked() {
                    self.section_level_box(li);
                }
                if rbtn(ui, "✂", "Cut", "Plan cut at the current height", false, true).clicked() {
                    self.section_plan_cut(li, self.plan_cut_h);
                }
                if rbtn(ui, "👁", "Isolate", "Only this level's items", false, true).clicked() {
                    self.isolate_level(li);
                }
            });
        }
        self.visibility_group(ui);
    }

    /// Tab-ul Annotation: tot ce tine de desenul 2D al sectiunii - export DWG / PDF,
    /// anotatii (cote, niveluri, grile, hasura) si previewul mic, usor de gasit.
    pub(super) fn ribbon_annotation(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Cut & Export", |ui| {
            if rbtn(ui, "📐", "Cut to DWG", "Export the section cut as 2D lines (DWG / DXF), one layer per element type - with hatching, dimension chains, levels and tags", false, self.section.enabled).clicked() {
                self.cad_sheet = false;
                self.request_path(Purpose::ExportSectionCad);
            }
            if rbtn(ui, "📄", "Cut on Sheet", "The same cut on a drawing sheet (A3 - A0) at a standard scale, with frame and title block (DWG / DXF)", false, self.section.enabled).clicked() {
                self.cad_sheet = true;
                self.request_path(Purpose::ExportSectionCad);
            }
            if rbtn(ui, "📄", "Cut to PDF", "Export the cut as a vector PDF sheet (A3 - A0) with title block, at the exact cut position", false, self.section.enabled || self.settings.cad_view != crate::app::CadView::Current).clicked() {
                self.request_path(Purpose::ExportSectionPdf);
            }
            if rbtn(ui, "🗺", "2D Preview", "Small closable map-style preview of the cut, before exporting", self.section_preview_open, self.section.enabled).clicked() {
                self.section_preview_open = !self.section_preview_open;
            }
            let box_on = self.section.enabled && self.section.mode == SectionMode::Box;
            if rbtn(ui, "◫", "Multi view", "Several views of the section box on one drawing, like an Inventor sheet: choose the views (Front, Right, Back, Left, Top, Bottom) and the spacing, see the preview, then export DWG / DXF at 1:1 in model space (Front stays in real coordinates) or a PDF sheet.", self.mv_open, box_on).clicked() {
                self.mv_open = !self.mv_open;
            }
        });
        rgroup(ui, "Cut View", |ui| {
            let box_on = self.section.enabled && self.section.mode == SectionMode::Box;
            if rbtn(ui, "🟩", "Pick face", "Click a face of the section box in the 3D view (it turns light green): the 2D drawing is made from that face - Top, Bottom, Front, Back, Left or Right. Esc cancels.", self.face_pick, box_on).clicked() {
                self.face_pick = !self.face_pick;
            }
            let cur = self.settings.cad_view;
            egui::ComboBox::from_id_salt("annot_cut_view").selected_text(crate::i18n::tr(cur.label())).show_ui(ui, |ui| {
                for v in crate::app::CadView::ALL {
                    let label = crate::i18n::tr(v.label());
                    let mut tmp = v;
                    if ui.selectable_value(&mut tmp, cur, label).clicked() {
                        self.settings.cad_view = v;
                        // highlightul nu se aprinde automat - doar utilizatorul il deschide
                        if v != crate::app::CadView::Current {
                            self.settings.cad_view_pos = 0.0;
                        }
                        self.settings.save();
                    }
                }
            });
        });
        rgroup(ui, "Annotations", |ui| {
            if rbtn(ui, "✏", "Title Block", "Edit the title block, sheet format (A3 - A0) and scale", self.title_block_open, true).clicked() {
                self.title_block_open = !self.title_block_open;
            }
            // Grids / Levels se comută în grupul „Grids & Levels" (Home / Sections), nu aici.
            if rbtn(ui, "▤", "Filled cuts", "Cut faces drawn solid and hatched (closed solids), instead of seeing inside the elements", self.section_caps, true).clicked() {
                self.section_caps = !self.section_caps;
            }
            if rbtn(ui, "✎", "Annotate", "Annotations in the exported DWG / PDF cut: hatch, element tags, grids, levels - off = only the cut outline (one layer per element type)", self.cad_annotate, true).clicked() {
                self.cad_annotate = !self.cad_annotate;
            }
            if rbtn(ui, "▨", "Hatch", "Hatch the cut materials (filled contours) in the exported DWG / PDF cut - off = clean outline", self.cad_hatch, true).clicked() {
                self.cad_hatch = !self.cad_hatch;
            }
            if rbtn(ui, "📐", "Dimensions", "Include dimension chains (mm / m) in the exported DWG / PDF cut", self.cad_dims, true).clicked() {
                self.cad_dims = !self.cad_dims;
            }
            if rbtn(ui, "≡", "Levels in Cut", "Include level lines and elevation labels in the exported DWG / PDF cut", self.cad_levels, true).clicked() {
                self.cad_levels = !self.cad_levels;
            }
            if rbtn(ui, "🗺", "DWG Overlay", "Lay a 2D drawing (DWG / DXF) over the model at a level: compare the plan with the model", self.overlay_open, true).clicked() {
                self.overlay_open = !self.overlay_open;
            }
        });
    }

    /// Toate comenzile BIM (IFC, IDS, clasificare, date, schimb) intr-un singur tab.
    pub(super) fn ribbon_bim(&mut self, ui: &mut egui::Ui) {
        let has = self.loaded_models().next().is_some();
        rgroup(ui, "IFC export", |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("MVD").small().weak()).on_hover_text("Model View Definition: which properties are written to the IFC file");
                egui::ComboBox::from_id_salt("bim_mvd").width(180.0).selected_text(self.settings.ifc_mvd.code()).show_ui(ui, |ui| {
                    for m in crate::ifc_mvd::Mvd::ALL {
                        if ui.selectable_value(&mut self.settings.ifc_mvd, m, m.label()).on_hover_text(m.tip()).clicked() {
                            self.settings.save();
                        }
                    }
                });
            });
            if rbtn(ui, "fmt:IFC", "Export IFC", "Export all visible items to IFC4 (geometry, colours, levels, GUIDs, properties). The MVD above selects which properties are written.", false, has).clicked() {
                self.ifc_selection = false;
                self.request_path(Purpose::ExportIfc);
            }
            if rbtn(ui, "fmt:IFC", "Selection IFC", "Export only the selected items to IFC4", false, !self.selected.is_empty()).clicked() {
                self.ifc_selection = true;
                self.request_path(Purpose::ExportIfc);
            }
            if rbtn(ui, "🗺", "Map classes", "IFC Mapping: map non-IFC categories (STEP, OBJ, glTF, DWG) to an IFC class and type before export, like in Revit. Import / export the table as CSV.", self.ifc_map_open, true).clicked() {
                self.ifc_map_open = !self.ifc_map_open;
            }
        });
        rgroup(ui, "Classification", |ui| {
            if rbtn(ui, "🏷", "bSDD", "Classification: search the buildingSMART Data Dictionary online or the local Uniclass / OmniClass catalogue and assign codes to the selected elements (exported to IFC)", self.class_open, true).clicked() {
                self.class_open = !self.class_open;
            }
            if rbtn(ui, "📋", "IDS check", "Check the model against buildingSMART IDS requirements (Model Checker)", false, has).clicked() {
                self.open_panel(crate::layout::Panel::Check);
            }
            if rbtn(ui, "📥", "Import IDS", "Load an IDS (.ids) requirements file", false, true).clicked() {
                self.request_path(Purpose::ImportCheckRules);
            }
        });
        rgroup(ui, "Data / COBie", |ui| {
            if rbtn(ui, "🏷", "Properties", "Property editor: pick elements and add your own properties, or fill the standard IFC property sets (Pset_WallCommon, Pset_DoorCommon, manufacturer, warranty...) - all exported to IFC with the correct data types", self.pe.open, has).clicked() {
                if self.pe.open {
                    self.pe.open = false;
                } else {
                    self.open_prop_editor();
                }
            }
            if rbtn(ui, "📥", "Import Props", "Write properties onto elements from Excel / CSV, matched by GlobalId / Name", false, has).clicked() {
                self.request_path(Purpose::ImportUserProps);
            }
            if rbtn(ui, "📤", "Export Props", "Excel template: one row per element with its properties", false, has).clicked() {
                self.request_path(Purpose::ExportUserProps);
            }
            if rbtn(ui, "🏗", "COBie", "COBie 2.4 handover workbook (xlsx)", false, has).clicked() {
                self.request_path(Purpose::ExportCobie);
            }
            if rbtn(ui, "📊", "Quantities", "Count, volume, areas, lengths by any property - export to Excel", false, has).clicked() {
                self.open_panel(crate::layout::Panel::Quantities);
            }
        });
        rgroup(ui, "Open standards", |ui| {
            if rbtn(ui, "fmt:GLB", "Export GLB", "Export to glTF binary for Blender / Rhino / Twinmotion: colours, materials, hierarchy and BIM properties", false, has).clicked() {
                self.ifc_selection = !self.selected.is_empty();
                self.request_path(Purpose::ExportGlb);
            }
            if rbtn(ui, "fmt:BCF", "Import BCF", "Import BCF 2.1 / 3.0 issues (Revit, ACC, Solibri, BIMcollab)", false, true).clicked() {
                self.request_path(Purpose::ImportBcf);
            }
            if rbtn(ui, "📷", "Import VP", "Import viewpoints exported from Navisworks", false, true).clicked() {
                self.request_path(Purpose::ImportNavisXml);
            }
            if rbtn(ui, "📋", "Sets", "Import / export selection and search sets (Navisworks XML)", false, true).clicked() {
                self.request_path(Purpose::ExportNavisSets);
            }
        });
    }

    pub(super) fn ribbon_coord(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Coordination", |ui| {
            self.panel_btn(ui, Panel::Clash, "⚠", "Clash", "Clash Detective — clashes are shown only while this panel is visible");
            self.panel_btn(ui, Panel::Check, "✔", "Checker", "Model Checker: check properties and data against rules or a buildingSMART IDS file - results to Excel / BCF");
            self.panel_btn(ui, Panel::TimeLiner, "📅", "TimeLiner", "4D schedule and simulation");
        });
        self.coord_ribbon(ui);
    }

    pub(super) fn ribbon_models(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Models", |ui| {
            self.panel_btn(ui, Panel::Quantities, "📊", "Quantities", "Quantification: count, volume, areas, lengths grouped by any property - export to Excel");
            self.panel_btn(ui, Panel::Compare, "⚖", "Compare", "Compare two versions of a model: removed, added, moved / reshaped and changed-property elements");
            self.panel_btn(ui, Panel::Coords, "📐", "Coordinates", "Move / rotate models and elements (with Reset)");
            self.panel_btn(ui, Panel::Project, "📁", "Project", "Project & Model Properties");
            if rbtn(ui, "📦", "Export .bimc", "Write a .bimc cache next to every loaded model", false, true).clicked() {
                self.export_all_caches();
            }
        });
        rgroup(ui, "Scan", |ui| {
            let has_cloud = self.loaded_models().any(|(_, m)| m.cloud.is_some());
            if rbtn(ui, "☁", "Scan vs Model", "As-built check: compare the model with the point cloud (E57 / LAS) - built, partial, missing, not scanned; colours and Excel report", self.asbuilt_open, has_cloud).clicked() {
                self.asbuilt_open = !self.asbuilt_open;
            }
        });
    }

    pub(super) fn ribbon_render(&mut self, ui: &mut egui::Ui) {
        self.style_group(ui);
        rgroup(ui, "Ray Trace", |ui| {
            if rbtn(ui, "✨", "Ray Trace", "Interactive Ray Trace: the 3D view is path traced on the graphics card - soft shadows, reflections, sky light, materials. It refines while the camera stands still (half resolution while moving) and is denoised when it is done, like Navisworks.", self.rt.on, self.loaded_models().next().is_some()).clicked() {
                self.toggle_raytrace();
            }
        });
        rgroup(ui, "Presentation", |ui| {
            let on = self.panel_shown(Panel::Animation);
            if rbtn(ui, "🎞", "Animations", "Animations: Record, Orbit 360°, viewpoints, 4D - play them and export video", on, true).clicked() {
                if on {
                    self.settings.layout.close(Panel::Animation);
                } else {
                    self.open_panel(Panel::Animation);
                }
            }
            // fereastra proprie, pe toata inaltimea: lista de materiale lunga si lata
            if rbtn(ui, "🎨", "Materials", "Material library, image textures & assignments", self.mats.window, true).clicked() {
                self.mats.window = !self.mats.window;
                if self.mats.window && self.panel_shown(Panel::Render) && self.render_subtab == 1 {
                    // acelasi editor nu sta deschis in doua locuri
                    self.render_subtab = 0;
                }
            }
            if rbtn(ui, "💡", "Show Mat.", "Show render materials in the viewport", self.materials_on, true).clicked() {
                self.materials_on = !self.materials_on;
                self.materials_changed();
            }
        });
        rgroup(ui, "Textures", |ui| {
            if rbtn(ui, "📥", "Images", "Import PNG / JPG images as new texture materials", false, true).clicked() {
                self.render_subtab = 1;
                self.open_panel(Panel::Render);
                self.request_path(Purpose::ImportTextures);
            }
            if rbtn(ui, "📥", "Import Lib", "Import materials into your library: a PBR pack .zip (ambientCG, freepbr, Poly Haven...), a .bimmat library or an Excel / CSV list - saved for every project", false, true).clicked() {
                self.request_path(Purpose::ImportMaterials);
            }
            if rbtn(ui, "📤", "Export Lib", "Export the material library (.bimmat)", false, true).clicked() {
                self.request_path(Purpose::ExportMaterials);
            }
        });
        rgroup(ui, "Output", |ui| {
            if rbtn(ui, "🖼", "Image", "Render the current view to PNG", false, true).clicked() {
                self.request_path(Purpose::RenderImage);
            }
            if rbtn(ui, "◎", "Path trace", "Progressive path traced image: preview, samples, time limit, Cancel, export PNG", false, true).clicked() {
                self.pt.open = true;
            }
            if rbtn(ui, "💡", "Lighting", "Create point / spot / distant lights, set their power (lux) and range, place and move them in the 3D view", false, true).clicked() {
                self.render_subtab = 2;
                self.open_panel(Panel::Render);
            }
            if rbtn(ui, "🎞", "Video", "Animations and video export", false, true).clicked() {
                self.open_panel(Panel::Animation);
            }
        });
    }

    pub(super) fn ribbon_output(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Clash Reports", |ui| {
            let can = !self.clash.tests.is_empty();
            if rbtn(ui, "📄", "HTML", "Export the clash report of the current test (HTML)", false, can).clicked() {
                self.request_path(Purpose::ExportClashHtml);
            }
            if rbtn(ui, "📊", "CSV", "Export the clash report of the current test (CSV)", false, can).clicked() {
                self.request_path(Purpose::ExportClashCsv);
            }
            if rbtn(ui, "📄", "PDF", "Export the clash report of the current test as a professional PDF sheet with title block", false, can).clicked() {
                self.request_path(Purpose::ExportClashPdf);
            }
        });
        rgroup(ui, "Navisworks", |ui| {
            if rbtn(ui, "📷", "Viewpoints", "Export viewpoints with markups, comments and section box to Navisworks", false, !self.viewpoints.is_empty()).clicked() {
                self.request_path(Purpose::ExportNavisXml);
            }
            if rbtn(ui, "⚠", "Clash Tests", "Export clash tests and sets to Navisworks", false, !self.clash.tests.is_empty()).clicked() {
                self.request_path(Purpose::ExportNavisClash);
            }
        });
        // formatele deschise si aici (nu doar in tab-ul BIM), cu iconite de format de fisier
        let has = self.loaded_models().next().is_some();
        rgroup(ui, "Open standards", |ui| {
            if rbtn(ui, "fmt:IFC", "IFC", "Export all visible items to IFC4 (MVD from the BIM tab). With a selection: only the selected items.", false, has).clicked() {
                self.ifc_selection = !self.selected.is_empty();
                self.request_path(Purpose::ExportIfc);
            }
            if rbtn(ui, "fmt:GLB", "GLB", "glTF binary for Blender / Rhino / Twinmotion / Unreal: colours, materials, hierarchy and BIM properties", false, has).clicked() {
                self.ifc_selection = !self.selected.is_empty();
                self.request_path(Purpose::ExportGlb);
            }
        });
    }

    pub(super) fn ribbon_share(&mut self, ui: &mut egui::Ui) {
        let has = self.loaded_models().next().is_some();
        let what = if self.selected.is_empty() { "all visible items" } else { "the selected items" };
        rgroup(ui, "Web", |ui| {
            if rbtn(ui, "🌐", "Web viewer", &format!("One .html file with {what}: 3D view, tree, search, properties, hide / isolate, X-ray.\nOpens in Chrome / Edge / Firefox / Safari, offline, nothing to install."), false, has).clicked() {
                self.web_selection = !self.selected.is_empty();
                self.request_path(Purpose::ExportWeb);
            }
        });
        rgroup(ui, "Reference", |ui| {
            if rbtn(ui, "🔗", "Revit / CAD", &format!("Reference of {what} for Revit and AutoCAD / Plant 3D (MBREFERENCE). Revit can also link the IFC export from the BIM tab."), false, has).clicked() {
                self.request_path(Purpose::ExportReference);
            }
        });
        rgroup(ui, "AI", |ui| {
            let on = self.mcp.server.is_some();
            let tip = if on {
                format!("MCP server running on {}\nAI assistants (Claude, OpenCode, Ollama clients) can read and drive the model", crate::mcp::url(self.settings.mcp.port))
            } else {
                "Connect AI assistants (Claude Code / Desktop, OpenCode, VS Code, local Ollama via an MCP client) to ManageBIM".to_string()
            };
            if rbtn(ui, "🤖", "AI connection", &tip, on || self.mcp.open, true).clicked() {
                self.mcp.open = !self.mcp.open;
            }
        });
    }

    pub(super) fn ribbon_output2(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Data", |ui| {
            let has_sel = !self.selected.is_empty();
            let _ = has_sel;
            if rbtn(ui, "📥", "Import Properties", "Write properties onto elements from Excel / CSV: one row per element (matched by GlobalId, Name or any property), every other column = a property", false, self.loaded_models().next().is_some()).clicked() {
                self.request_path(Purpose::ImportUserProps);
            }
            if rbtn(ui, "📤", "Export Properties", "Excel template: one row per element (selection, or all visible) with its key and its user properties", false, self.loaded_models().next().is_some()).clicked() {
                self.request_path(Purpose::ExportUserProps);
            }
            if rbtn(ui, "🏗", "COBie…", "COBie 2.4 handover workbook (xlsx): Contact, Facility, Floor, Type, Component, System — for facility management", false, self.loaded_models().next().is_some()).clicked() {
                self.request_path(Purpose::ExportCobie);
            }
            let r = rbtn(ui, "📋", "Sets ⏷", "Import / export selection and search sets", false, true);
            let pid = ui.make_persistent_id("data_sets_menu");
            if r.clicked() {
                ui.memory_mut(|m| m.toggle_popup(pid));
            }
            egui::popup_below_widget(ui, pid, &r, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                ui.set_min_width(190.0);
                if ui.add_enabled(!self.sets.is_empty(), egui::Button::new("📤 Export sets…")).clicked() {
                    self.request_path(Purpose::ExportSets);
                }
                if ui.button("📥 Import sets…").clicked() {
                    self.request_path(Purpose::ImportSets);
                }
            });
            let r = rbtn(ui, "📅", "Schedule ⏷", "Import / export the TimeLiner schedule", false, true);
            let pid = ui.make_persistent_id("data_sched_menu");
            if r.clicked() {
                ui.memory_mut(|m| m.toggle_popup(pid));
            }
            egui::popup_below_widget(ui, pid, &r, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                ui.set_min_width(230.0);
                if ui.button("📤 Export schedule (CSV)…").clicked() {
                    self.request_path(Purpose::ExportTasksCsv);
                }
                if ui.button("📥 Import schedule…").on_hover_text("CSV, MS Project XML or Primavera P6 XML.\nLink tasks to elements with [Level=Level 1;Type=IfcSlab] at the end of the task name (or Text1 in MS Project).").clicked() {
                    self.request_path(Purpose::ImportTasksCsv);
                }
            });
            if rbtn(ui, "📦", "BIMC", "Export .bimc caches next to the models", false, true).clicked() {
                self.export_all_caches();
            }
        });
    }

    /// Stilul de afisare: Realistic / Shaded / Concept / Wireframe
    pub(super) fn style_group(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Visual Style", |ui| {
            use crate::render::VisualStyle as V;
            let cur = self.settings.visual_style;
            for (s, icon, tip) in [
                (V::Realistic, "☀", "Materials, textures, reflections, contact shadows"),
                (V::Shaded, "◆", "Model colours with light, no textures"),
                (V::Concept, "✏", "White study model with strong edges"),
                (V::Hidden, "◫", "Hidden line: neutral surfaces, hidden edges removed"),
                (V::Wireframe, "▦", "Triangle edges only - see through the model"),
            ] {
                if rbtn(ui, icon, s.label(), tip, cur == s, true).clicked() && cur != s {
                    self.set_visual_style(s);
                }
            }
        });
    }

    /// Starea curenta de vizualizare ca sablon (numele / nota / flagurile se completeaza de apelant).
    pub(super) fn view_template_snapshot(&self, name: String) -> crate::app::ViewTemplate {
        crate::app::ViewTemplate {
            name,
            hidden: self.hidden.iter().copied().collect(),
            section: self.section.clone(),
            cam: self.cam,
            visual_style: self.settings.visual_style,
            show_grids: self.show_grids,
            show_levels: self.level.show_lines,
            section_caps: self.section_caps,
            overrides: self.overrides.iter().map(|(k, v)| (*k, *v)).collect(),
            selected: self.selected.iter().copied().collect(),
            measures: self.measures.clone(),
            vp_filter: self.vp_filter.clone(),
            is_default: false,
            note: String::new(),
            include: [true; 5],
        }
    }

    /// Salvează starea curentă de vizualizare ca şablon în proiect.
    pub(super) fn save_view_template(&mut self) {
        let t = self.view_template_snapshot(format!("View {}", self.view_templates.len() + 1));
        self.view_templates.push(t);
        self.vt_current = self.view_templates.len() - 1;
        self.vt_applied = true;
        self.toast("View template saved (in project)");
    }

    /// Suprascrie sablonul `i` cu vederea curenta, pastrand numele, nota, steaua si ce include.
    pub(super) fn update_view_template(&mut self, i: usize) {
        let Some(old) = self.view_templates.get(i).cloned() else { return };
        let mut t = self.view_template_snapshot(old.name.clone());
        t.note = old.note;
        t.is_default = old.is_default;
        t.include = old.include;
        self.view_templates[i] = t;
        self.toast(format!("'{}' updated with the current view", old.name));
    }

    /// Aplică un şablon de vizualizare - doar partile bifate la „Include".
    pub(super) fn apply_view_template(&mut self, i: usize) {
        let Some(t) = self.view_templates.get(i).cloned() else { return };
        let [cam, vis, sec, style, col] = t.include;
        if vis {
            self.hidden = t.hidden.into_iter().collect();
            self.selected = t.selected.into_iter().collect();
            self.measures = t.measures;
            self.vp_filter = t.vp_filter;
        }
        if sec {
            self.section = t.section;
        }
        if cam {
            self.fly_to(t.cam);
        }
        if style {
            self.apply_style_only(t.visual_style);
            self.show_grids = t.show_grids;
            self.level.show_lines = t.show_levels;
            self.section_caps = t.section_caps;
        }
        if col {
            self.overrides = t.overrides.into_iter().collect();
        }
        self.vt_current = i;
        self.vt_applied = true;
        // vederea nu mai este cea a viewpoint-ului activ: nu-l mai aratam ca activ (si nici redline-urile lui)
        if cam || vis || sec {
            self.active_vp = None;
        }
        self.colors_dirty = true;
        self.toast(format!("Applied '{}'", t.name));
    }

    pub(super) fn ribbon_view(&mut self, ui: &mut egui::Ui) {
        self.style_group(ui);
        rgroup(ui, "Compare", |ui| {
            if rbtn(ui, "◫", "Split view", "Two viewports side by side in this window (drag the divider to stretch)", self.split_view, true).clicked() {
                self.split_view = !self.split_view;
                if self.split_view { self.cam_b = self.cam; self.section_b = self.section.clone(); }
            }
            if rbtn(ui, "🔗", "Link cameras", "Copy camera A to B when turning link on; unlink to move B independently", self.split_link, self.split_view).clicked() {
                self.split_link = !self.split_link;
                if self.split_link { self.cam_b = self.cam; self.section_b = self.section.clone(); }
            }
        });
        rgroup(ui, "View Templates", |ui| {
            if rbtn(ui, "➕", "New Template", "Save the current visibility, section and visual style as a template in this project", false, true).clicked() {
                self.save_view_template();
            }
            let n = self.view_templates.len();
            if rbtn(ui, "◫", &format!("Apply ({n})"), "Apply a saved view template", false, n > 0).clicked() {
                self.view_templates_open = !self.view_templates_open;
            }
        });
        // ferestrele si optiunile in meniuri derulante: ribbon-ul pastreaza aceeasi inaltime pe toate tab-urile
        rgroup(ui, "Windows", |ui| {
            let open_n = crate::layout::ALL_PANELS.iter().filter(|p| self.settings.layout.is_open(**p)).count();
            let r = rbtn(ui, "🗔", &format!("Panels ({open_n}) ⏷"), "Show / hide any panel", false, true);
            let id = ui.make_persistent_id("win_menu");
            if r.clicked() {
                ui.memory_mut(|m| m.toggle_popup(id));
            }
            egui::popup_below_widget(ui, id, &r, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(200.0);
                for p in crate::layout::ALL_PANELS.iter() {
                    let mut open = self.settings.layout.is_open(*p);
                    if ui.checkbox(&mut open, crate::theme::title_job(p.title(), 14.0, ui.visuals().text_color())).changed() {
                        if open {
                            self.open_panel(*p);
                        } else {
                            self.settings.layout.close(*p);
                        }
                    }
                }
            });
            let all_auto = self.settings.layout.autohide.iter().all(|a| *a);
            if rbtn(ui, "📌", "Auto-hide", "All panel areas auto-hide: thin tab strips, the panels slide in over the model when you point at them", all_auto, true).clicked() {
                self.settings.layout.autohide = [!all_auto; 3];
                self.flyout = None;
            }
            if rbtn(ui, "⛶", "Hide panels", "Hide all panels - the whole window for the model (F11)", self.settings.layout.hide_all, true).clicked() {
                self.settings.layout.hide_all = !self.settings.layout.hide_all;
                self.flyout = None;
            }
            if rbtn(ui, "⟲", "Reset", "Restore the default window arrangement", false, true).clicked() {
                self.settings.layout = Default::default();
            }
        });
        rgroup(ui, "Scene", |ui| {
            // un singur loc pentru lumina: soare, cer, lampi (Render › Lighting)
            let has_cloud = self.loaded_models().any(|(_, m)| m.cloud.is_some());
            if rbtn(ui, "☁", "Point Cloud", "Point density and size (E57 point clouds)", self.show_cloud, has_cloud).clicked() {
                self.show_cloud = !self.show_cloud;
            }
        });
    }

    pub(super) fn ribbon_grids(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Grids & Levels", |ui| {
            if rbtn(ui, "#", "Grids", "Show / hide the grid axes in the 3D view", self.show_grids, true).clicked() {
                self.show_grids = !self.show_grids;
            }
            if rbtn(ui, "≡", "Levels", "Show / hide the levels in the 3D view", self.level.show_lines, true).clicked() {
                self.level.show_lines = !self.level.show_lines;
            }
            if rbtn(ui, "✏", "Edit Grids", "Create grid axes in ManageBIM (generator or one by one), size, export DXF / DWG", self.show_grids_editor, true).clicked() {
                self.show_grids_editor = !self.show_grids_editor;
            }
            if rbtn(ui, "✏", "Edit Levels", "Your own levels: ±0.00 of the project, names and elevations (editable)", self.level.show_editor, true).clicked() {
                self.level.show_editor = !self.level.show_editor;
            }
            if rbtn(ui, "📤", "Send Levels", "Send these levels to AutoCAD / Plant 3D (command MBLEVELS: level lines, a UCS per level, the level on every object) and Navisworks (ManageBIM › Levels: a selection set and a plan view per level)", false, self.loaded_models().next().is_some()).clicked() {
                self.send_levels_outbox();
            }
        });
    }

    pub(super) fn ribbon_view2(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Language", |ui| {
            if rbtn(ui, "🌐", if self.settings.romanian { "English" } else { "Română" }, "Interface language: English / Română", false, true).clicked() {
                self.toggle_language();
            }
        });
        rgroup(ui, "Theme", |ui| {
            let cur = self.settings.theme;
            for (t, icon, tip) in [
                (Theme::Studio, "◆", "Studio theme"),
                (Theme::Dark, "🌙", "Dark theme"),
                (Theme::Nord, "❄", "Nord theme"),
                (Theme::TokyoNight, "🌃", "Tokyo Night theme"),
                (Theme::Light, "☀", "Light theme"),
            ] {
                if rbtn(ui, icon, t.label(), tip, cur == t, true).clicked() && cur != t {
                    self.set_theme(t);
                }
            }
        });
        rgroup(ui, "Interface", |ui| {
            // fereastra ingusta: doar valoarea (glisare), fara eticheta si slider lat
            let narrow = crate::theme::ribbon_btn_w() < 55.0 || crate::theme::RIBBON_COMPACT.load(std::sync::atomic::Ordering::Relaxed);
            ui.horizontal(|ui| {
                let mut pct = (self.settings.ui_scale * 100.0).round() as i32;
                let r = if narrow {
                    ui.add(egui::DragValue::new(&mut pct).range(75..=200).speed(1.0).prefix("UI ").suffix("%"))
                } else {
                    ui.label("Font / UI size");
                    ui.add_sized([110.0, ui.spacing().interact_size.y], egui::Slider::new(&mut pct, 75..=200).step_by(5.0).suffix("%"))
                };
                if r.changed() && (!r.dragged() || r.drag_stopped()) {
                    self.settings.ui_scale = pct as f32 / 100.0;
                    self.settings.save();
                }
            })
            .response
            .on_hover_text("Scales the whole interface: fonts, panels, tables and dialogs. 100% = default. Saved between sessions.");
        });
        rgroup(ui, "Options", |ui| {
            let r = rbtn(ui, "⚙", "Options ⏷", "Cache, converters, file associations", false, true);
            let id = ui.make_persistent_id("opt_menu");
            if r.clicked() {
                ui.memory_mut(|m| m.toggle_popup(id));
            }
            egui::popup_below_widget(ui, id, &r, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(260.0);
                let mut c = self.settings.clone();
                ui.checkbox(&mut c.auto_cache, "Create .bimc cache when opening a model");
                ui.checkbox(&mut c.use_cache, "Open the up-to-date .bimc cache instead");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Length unit");
                    for (code, label) in crate::app::Unit::ALL {
                        if ui.selectable_label(self.settings.unit_system == code, code).on_hover_text(label).clicked() {
                            self.settings.unit_system = code.into();
                            self.settings.units_mm = code == "mm";
                            crate::app::set_unit(crate::app::Unit::from_code(code));
                            self.settings.save();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Interface size");
                    let mut pct = (self.settings.ui_scale * 100.0).round() as i32;
                    let r = ui.add_sized([140.0, ui.spacing().interact_size.y], egui::Slider::new(&mut pct, 75..=200).step_by(5.0).suffix("%"));
                    if r.changed() && (!r.dragged() || r.drag_stopped()) {
                        self.settings.ui_scale = pct as f32 / 100.0;
                        self.settings.save();
                    }
                })
                .response
                .on_hover_text("Scales the whole interface: text, panels, tables and dialogs (100% = default). Same as the DPI setting of Windows, but only for ManageBIM.");
                ui.horizontal(|ui| {
                    ui.label("Navisworks XML units");
                    let cur = self.settings.navis_units.clone();
                    let shown = if cur == "auto" { format!("Auto ({})", self.navis_units()) } else { cur.clone() };
                    egui::ComboBox::from_id_salt("nw_units").selected_text(shown).show_ui(ui, |ui| {
                        for u in ["auto", "m", "cm", "mm", "ft", "in"] {
                            if ui.selectable_label(cur == u, if u == "auto" { "Auto (Navisworks model units)" } else { u }).clicked() {
                                self.settings.navis_units = u.to_string();
                                self.settings.save();
                            }
                        }
                    });
                })
                .response
                .on_hover_text("Viewpoints are written in the units of the Navisworks document (Options > Interface > Display Units) so they land in the right place. Auto uses the units of a model opened from Navisworks, otherwise metres.");
                if c.auto_cache != self.settings.auto_cache || c.use_cache != self.settings.use_cache {
                    self.settings.auto_cache = c.auto_cache;
                    self.settings.use_cache = c.use_cache;
                    self.settings.save();
                }
                ui.separator();
                if ui.button("Converters (DWG, Navisworks)…").clicked() {
                    self.show_tools = true;
                    ui.close_menu();
                }
                #[cfg(windows)]
                if ui.button("Associate .bimc / .bimr with ManageBIM").clicked() {
                    self.register_file_types();
                    ui.close_menu();
                }
            });
        });
    }

    pub(super) fn ribbon_help(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Help", |ui| {
            if rbtn(ui, "🖱", "Controls", "Mouse & keyboard navigation", false, true).clicked() {
                self.show_help = true;
            }
            if rbtn(ui, "⌨", "Shortcuts", "All keyboard shortcuts (F1)", self.keys_open, true).clicked() {
                self.keys_open = !self.keys_open;
            }
            if rbtn(ui, "📄", "Manual", "The ManageBIM manual: every feature, step by step, with search", self.manual_open, true).clicked() {
                self.manual_open = !self.manual_open;
            }
            if rbtn(ui, "🏗", "Demo", "Open the demo project (Block A: architecture + structure, MEP, 4D schedule)", false, true).clicked() {
                self.open_demo();
            }
            if rbtn(ui, "≡", "Commands", "Command palette (Ctrl+K): type what you want to do", self.palette_open, true).clicked() {
                self.palette_open = true;
            }
            if rbtn(ui, "❓", "About", "About ManageBIM", false, true).clicked() {
                self.show_about = true;
            }
        });
    }

    pub(super) fn ribbon_redline(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Markup", |ui| {
            for k in crate::markup::RedKind::ALL {
                let on = self.tool == Tool::Redline(k);
                if rbtn(ui, k.icon(), k.label(), &format!("{} — saved with the current viewpoint", k.label()), on, true).clicked() {
                    self.cur_measure.clear();
                    self.tool = if on { Tool::Select } else { Tool::Redline(k) };
                }
            }
        });
        rgroup(ui, "Edit", |ui| {
            if rbtn(ui, "✖", "Erase", "Click a markup to delete it", self.tool == Tool::RedErase, true).clicked() {
                self.tool = if self.tool == Tool::RedErase { Tool::Select } else { Tool::RedErase };
            }
            let vp = self.viewpoints.get(self.vp_sel).map(|v| v.redlines.len()).unwrap_or(0);
            if rbtn(ui, "↶", "Undo", "Remove the last markup of this viewpoint (Ctrl+Z)", false, vp > 0).clicked() {
                if let Some(v) = self.viewpoints.get_mut(self.vp_sel) { v.redlines.pop(); }
            }
            if rbtn(ui, "🗑", "Clear", "Remove all markups of this viewpoint", false, vp > 0).clicked() {
                if let Some(v) = self.viewpoints.get_mut(self.vp_sel) { v.redlines.clear(); }
            }
        });
        rgroup(ui, "Style", |ui| {
            let col = rbtn(ui, "🎨", "Colour", "Markup colour and line weight", false, true);
            let cid = ui.make_persistent_id("red_col_pop");
            if col.clicked() { ui.memory_mut(|m| m.toggle_popup(cid)); }
            egui::popup_below_widget(ui, cid, &col, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(230.0);
                ui.label(RichText::new("Colour").small().strong());
                ui.horizontal_wrapped(|ui| {
                    // paletă standard de desen tehnic (roșu / portocaliu / galben / verde / albastru / negru / alb)
                    for c in [[230u8, 40, 40], [251, 146, 60], [250, 204, 21], [74, 222, 128], [59, 130, 246], [30, 35, 45], [250, 250, 250]] {
                        let (r, resp) = ui.allocate_exact_size(EV::splat(18.0), Sense::click());
                        ui.painter().rect_filled(r, crate::theme::RADIUS_SM, Color32::from_rgb(c[0], c[1], c[2]));
                        if self.red_color == c {
                            ui.painter().rect_stroke(r.expand(2.0), crate::theme::RADIUS_SM, Stroke::new(2.0_f32, accent()));
                        }
                        if resp.clicked() { self.red_color = c; }
                    }
                    ui.color_edit_button_srgb(&mut self.red_color);
                });
                ui.separator();
                ui.label(RichText::new("Line weight").small().strong());
                ui.horizontal(|ui| {
                    for (w, label) in [(1.5_f32, "Fine"), (3.0, "Medium"), (6.0, "Heavy")] {
                        let on = (self.red_width - w).abs() < 0.1;
                        if ui.selectable_label(on, label).clicked() { self.red_width = w; }
                    }
                });
                ui.add(egui::Slider::new(&mut self.red_width, 1.0..=10.0).text("Width"));
            });
        });
    }

    pub(super) fn ribbon_issues(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Shared Issues", |ui| {
            let on = self.shared_dir.is_some();
            let tip = match &self.shared_dir {
                Some(d) => format!("Shared in: {d}\n{}\nClick to choose another folder", self.shared.status),
                None => "Share the viewpoints / issues with colleagues through a common folder\n(network drive, SharePoint or OneDrive synced folder)".to_string(),
            };
            if rbtn(ui, "🌐", if on { "Shared" } else { "Share…" }, &tip, on, true).clicked() {
                self.request_path(Purpose::SharedFolder);
            }
            if rbtn(ui, "⟳", "Sync", "Sync now with the shared folder (also automatic every few seconds)", false, on).clicked() {
                self.shared_sync(true);
            }
            if rbtn(ui, "⛔", "Stop Sharing", "Stop sharing (the files in the folder stay)", false, on).clicked() {
                self.shared_dir = None;
                self.shared = Default::default();
            }
        });
        rgroup(ui, "Viewpoints & Issues", |ui| {
            self.panel_btn(ui, Panel::Viewpoints, "📋", "Issues", "Viewpoints, markups, status, comments");
            let can = !self.viewpoints.is_empty();
            if rbtn(ui, "📄", "HTML", "Report of all viewpoints with snapshots and markups (HTML)", false, can).clicked() {
                self.request_path(Purpose::ExportVpHtml);
            }
            if rbtn(ui, "📊", "Excel", "Excel workbook with one row and a snapshot per viewpoint", false, can).clicked() {
                self.request_path(Purpose::ExportVpXlsx);
            }
            if rbtn(ui, "📄", "PDF Sheets", "One A3 sheet per viewpoint: large image, description, comments and a title block (project, viewpoint, status, date, sheet number)", false, can).clicked() {
                self.request_path(Purpose::ExportVpPdf);
            }
            if rbtn(ui, "fmt:BCF", "BCF", "BCF 2.1 issues (Revit, Navisworks, Solibri, BIMcollab, ACC...)", false, can).clicked() {
                self.request_path(Purpose::ExportBcf);
            }
            if rbtn(ui, "🧭", "Navisworks", "Viewpoints with markups, comments and section box as Navisworks XML\n(in Navisworks: Saved Viewpoints > right-click > Import Viewpoints)", false, can).clicked() {
                self.request_path(Purpose::ExportNavisXml);
            }
        });
    }

    pub(super) fn ribbon_script(&mut self, ui: &mut egui::Ui) {
        rgroup(ui, "Scripts", |ui| {
            if rbtn(ui, "📜", "Console", "Open the script console: list, create and manage scripts", self.script_open, true).clicked() {
                self.script_open = !self.script_open;
            }
            if rbtn(ui, "➕", "New Script", "Create a new .rhai script in the ManageBIM Scripts folder", false, true).clicked() {
                match script::create(&self.settings.scripts_dir) {
                    Ok(p) => self.toast(format!("Script created: {}", crate::file_name(p.to_string_lossy().as_ref()))),
                    Err(e) => self.error(e),
                }
            }
            if rbtn(ui, "📂", "Folder", "Copy the Scripts folder path to the clipboard", false, true).clicked() {
                ui.ctx().copy_text(script::dir(&self.settings.scripts_dir).to_string_lossy().to_string());
                self.toast("Scripts folder path copied to clipboard.");
            }
            if rbtn(ui, "❓", "API Help", "Show the scripting API reference and examples", self.script_help_open, true).clicked() {
                self.script_help_open = !self.script_help_open;
                self.script_open = true;
            }
        });
        rgroup(ui, "Run", |ui| {
            let scripts = script::list(&self.settings.scripts_dir);
            let n = scripts.len();
            ui.label(format!("{n} script(s) in folder"));
            if ui.button("▶ Run all").clicked() {
                for p in scripts {
                    self.run_script(p.to_string_lossy().as_ref());
                }
            }
        });
    }
}
