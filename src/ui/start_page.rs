//! Start page shown when no project is open (recent files, samples, news).
use super::*;

impl App {
    /// Nothing open and nothing loading: the start page is shown.
    pub(super) fn start_screen(&self) -> bool {
        self.loaded_models().next().is_none() && self.loads.is_empty() && self.project_path.is_none() && !self.blank_project && self.bimd_path.is_none() || self.show_start
    }

    // ================================================================== start page
    /// The start screen, when no model is open: Open / Open Project / New and the recent files
    /// as cards (type, name, modification date); click opens, right click removes from the list.
    pub(super) fn start_page(&mut self, ui: &mut egui::Ui) {
        use crate::app::StartAction;
        let rect = ui.max_rect();
        let side_w = 252.0f32.min(rect.width() * 0.35);
        let side = Rect::from_min_size(rect.min, EV::new(side_w, rect.height()));
        // the colours of the theme (not of the 3D background: with a white / sky background the texts vanished)
        let vis = ui.visuals().clone();
        let strong = vis.strong_text_color();
        let weak = vis.weak_text_color();
        let card_bg = vis.faint_bg_color;
        let card_hover = vis.widgets.hovered.weak_bg_fill;
        let border = crate::theme::border();
        ui.painter().rect_filled(rect, 0.0, vis.panel_fill);
        ui.painter().rect_filled(side, 0.0, vis.window_fill);
        ui.painter().line_segment([side.right_top(), side.right_bottom()], Stroke::new(1.0_f32, border));
        // ---- brand: isometric logo + title + tagline (subtle card, not bare text)
        {
            let p = ui.painter();
            let hero = Rect::from_min_size(side.min + EV::new(18.0, 16.0), EV::new(side_w - 36.0, 82.0));
            p.rect_filled(hero, crate::theme::RADIUS_LG, card_bg);
            p.rect_stroke(hero, crate::theme::RADIUS_LG, egui::Stroke::new(1.0_f32, crate::theme::border()));
            let tile = Rect::from_center_size(Pos2::new(hero.min.x + 37.0, hero.center().y), EV::new(42.0, 42.0));
            p.rect_filled(tile, crate::theme::RADIUS_MD, accent());
            let c = tile.center();
            let s = 11.0;
            let top = [Pos2::new(c.x, c.y - s), Pos2::new(c.x + s, c.y - s * 0.5), Pos2::new(c.x, c.y), Pos2::new(c.x - s, c.y - s * 0.5)];
            let left = [top[3], top[2], Pos2::new(c.x, c.y + s), Pos2::new(c.x - s, c.y + s * 0.5)];
            let right = [top[2], top[1], Pos2::new(c.x + s, c.y + s * 0.5), Pos2::new(c.x, c.y + s)];
            let white = Color32::from_white_alpha(235);
            p.add(egui::Shape::convex_polygon(top.to_vec(), Color32::from_white_alpha(245), Stroke::NONE));
            p.add(egui::Shape::convex_polygon(left.to_vec(), white.gamma_multiply(0.65), Stroke::NONE));
            p.add(egui::Shape::convex_polygon(right.to_vec(), white.gamma_multiply(0.85), Stroke::NONE));
            p.text(Pos2::new(hero.min.x + 70.0, hero.min.y + 31.0), Align2::LEFT_CENTER, "ManageBIM", FontId::proportional(19.0), accent());
            p.text(Pos2::new(hero.min.x + 70.0, hero.min.y + 52.0), Align2::LEFT_CENTER, "Coordinate · Check · Deliver", FontId::proportional(10.5), weak);
        }
        let mut act: Option<StartAction> = None;
        let mut forget: Option<String> = None;
        let open_name = self.bimd_path.as_ref().or(self.project_path.as_ref()).map(|p| crate::file_name(p)).or_else(|| self.loaded_models().next().map(|(_, m)| m.name.clone())).or_else(|| self.blank_project.then(|| "New project".to_string()));
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(side.shrink2(EV::new(18.0, 18.0))), |ui| {
            // like Navisworks: back arrow to the open project
            if self.show_start {
                if let Some(n) = &open_name {
                    if ui.add(egui::Button::new(RichText::new(format!("⬅  {n}")).size(13.0)).frame(false)).on_hover_text("Back to the open project").clicked() {
                        self.show_start = false;
                    }
                    ui.add_space(8.0);
                }
            }
            ui.add_space(98.0);
            ui.label(RichText::new(format!("v{} · BIM coordination workspace", env!("CARGO_PKG_VERSION"))).small().color(weak));
            ui.add_space(12.0);
            let bw = side_w - 36.0;
            // vector icon + text (emojis have no glyphs in the UI font)
            let tcol = ui.visuals().text_color();
            let on_accent = match self.settings.theme { Theme::Nord | Theme::TokyoNight => Color32::from_rgb(30, 36, 48), _ => Color32::WHITE };
            let big = |t: &str| egui::Button::new(crate::theme::title_job(t, 14.0, tcol)).min_size(EV::new(bw, 34.0)).fill(card_bg).stroke(Stroke::new(1.0_f32, border)).rounding(8.0);
            // Open… with a menu (model / project), like the Open button in Navisworks
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let primary = egui::Button::new(crate::theme::title_job("📂 Open…", 14.0, on_accent)).fill(accent()).rounding(8.0).min_size(EV::new(bw - 34.0, 34.0));
                if ui.add(primary).on_hover_text("Open a model or a project (IFC, Revit / Navisworks, DWG, STEP, FBX, E57, .bimr, .bimd…)  Ctrl+O").clicked() {
                    act = Some(StartAction::OpenModelDialog);
                }
                ui.menu_button(RichText::new("⏷").size(14.0), |ui| {
                    if ui.button("📂 Open model…").clicked() {
                        act = Some(StartAction::OpenModelDialog);
                        ui.close_menu();
                    }
                    if ui.button("🗂 Open project (.bimr / .bimd)…").clicked() {
                        act = Some(StartAction::OpenProjectDialog);
                        ui.close_menu();
                    }
                });
            });
            ui.add_space(4.0);
            if ui.add(big("➕ New")).on_hover_text("New empty project: add models, grids, levels; save it with Save (Ctrl+S)").clicked() {
                act = Some(StartAction::New);
            }
            if open_name.is_some() && self.show_start {
                ui.add_space(4.0);
                if ui.add(big("✖ Close project")).clicked() {
                    act = Some(StartAction::Close);
                }
            }
            ui.add_space(4.0);
            if ui.add(big("🏗 Demo project")).on_hover_text("Block A: architecture + structure, MEP and a 4D schedule - try every feature").clicked() {
                self.open_demo();
            }
            if ui.add(big("❓ Manual")).on_hover_text("The ManageBIM manual, with search").clicked() {
                self.manual_open = true;
            }
            ui.add_space(16.0);
            ui.label(RichText::new("or drag files into the window  ·  Ctrl+K: commands").small().color(weak));
            // recovery after an unexpected shutdown
            if let Some((_, orig, when)) = self.autosave.recover.clone() {
                ui.add_space(20.0);
                egui::Frame::none().fill(Color32::from_rgba_unmultiplied(235, 170, 60, 34)).stroke(Stroke::new(1.0_f32, Color32::from_rgb(235, 170, 60))).rounding(6.0).inner_margin(8.0).show(ui, |ui| {
                    ui.label(RichText::new("⚠ ManageBIM was closed unexpectedly").strong());
                    ui.label(RichText::new(format!("Autosaved {when}{}", orig.as_deref().map(|p| format!(" - {}", crate::file_name(p))).unwrap_or_default())).small());
                    ui.horizontal(|ui| {
                        if ui.button("Recover").clicked() {
                            self.autosave_recover();
                        }
                        if ui.button("Discard").clicked() {
                            self.autosave_discard();
                        }
                    });
                });
            }
        });
        // ---- Recent: list / grid, sorting, search
        let area = Rect::from_min_max(Pos2::new(side.right() + 28.0, rect.top() + 18.0), Pos2::new(rect.right() - 20.0, rect.bottom() - 10.0));
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(area), |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Recent").size(20.0).strong().color(strong));
                ui.add_space(14.0);
                if ui.selectable_label(self.start_list, RichText::new("☰").size(15.0)).on_hover_text("List").clicked() {
                    self.start_list = true;
                }
                if ui.selectable_label(!self.start_list, RichText::new("▦").size(15.0)).on_hover_text("Tiles").clicked() {
                    self.start_list = false;
                }
                ui.add_space(10.0);
                ui.label(RichText::new("Sort by").color(weak));
                const SORTS: [&str; 4] = ["Last opened", "Name", "Date modified", "Type"];
                egui::ComboBox::from_id_salt("start_sort").width(120.0).selected_text(SORTS[self.start_sort.min(3) as usize]).show_ui(ui, |ui| {
                    for (i, s) in SORTS.iter().enumerate() {
                        ui.selectable_value(&mut self.start_sort, i as u8, *s);
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.settings.recent.is_empty() && ui.small_button("Clear list").clicked() {
                        self.settings.recent.clear();
                        self.settings.save();
                    }
                    ui.add(egui::TextEdit::singleline(&mut self.start_search).hint_text("🔍 Search").desired_width(180.0));
                });
            });
            ui.add_space(8.0);
            let q = self.start_search.trim().to_lowercase();
            let mut recent: Vec<(usize, String, Option<std::time::SystemTime>)> = self
                .settings
                .recent
                .iter()
                .enumerate()
                .filter(|(_, p)| q.is_empty() || p.to_lowercase().contains(&q))
                .map(|(i, p)| (i, p.clone(), std::fs::metadata(p).and_then(|m| m.modified()).ok()))
                .collect();
            match self.start_sort {
                1 => recent.sort_by(|a, b| crate::app::natural_cmp(&crate::file_name(&a.1).to_lowercase(), &crate::file_name(&b.1).to_lowercase())),
                2 => recent.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0))),
                3 => recent.sort_by(|a, b| file_badge(&a.1).0.cmp(file_badge(&b.1).0).then(a.0.cmp(&b.0))),
                _ => {}
            }
            if recent.is_empty() {
                if !self.settings.recent.is_empty() {
                    ui.label(RichText::new("Nothing matches the search.").color(weak));
                    return;
                }
                // start page for a new user: clear, friendly, with real actions
                ui.add_space((area.height() - 330.0).max(24.0));
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Welcome to ManageBIM").size(30.0).strong().color(strong));
                    ui.add_space(8.0);
                    ui.label(RichText::new("Open a federated BIM model, or explore the full workflow with the demo project.").size(14.0).color(weak));
                    ui.add_space(28.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = EV::new(12.0, 12.0);
                        let card = |ui: &mut egui::Ui, glyph: &str, title: &str, sub: &str| {
                            let mut job = egui::text::LayoutJob { halign: egui::Align::Center, ..Default::default() };
                            job.append(crate::theme::ph(glyph), 0.0, egui::TextFormat { font_id: crate::theme::icon_font(22.0), color: accent(), ..Default::default() });
                            job.append(&format!("\n{title}\n"), 0.0, egui::TextFormat { font_id: FontId::proportional(14.0), color: strong, ..Default::default() });
                            job.append(sub, 0.0, egui::TextFormat { font_id: FontId::proportional(12.0), color: weak, ..Default::default() });
                            ui.add(egui::Button::new(job).min_size(EV::new(190.0, 96.0)).fill(card_bg).stroke(Stroke::new(1.0_f32, border)).rounding(10.0)).clicked()
                        };
                        if card(ui, "📂", "Open a model", "IFC · DWG · STEP · E57…") {
                            act = Some(StartAction::OpenModelDialog);
                        }
                        if card(ui, "🏗", "Try the demo", "Clash · 4D · export") {
                            self.open_demo();
                        }
                        if card(ui, "⌨", "Commands", "Search or press Ctrl+K") {
                            self.palette_open = true;
                        }
                    });
                    ui.add_space(20.0);
                    ui.label(RichText::new("Tip: drag & drop files anywhere in the window.").small().color(weak));
                });
                return;
            }
            egui::ScrollArea::vertical().id_salt("start_recent").auto_shrink([false, false]).show(ui, |ui| {
                let mut card = |ui: &mut egui::Ui, p: &String, modified: Option<std::time::SystemTime>, size: EV| {
                    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
                    let exists = std::path::Path::new(p).exists();
                    let hov = resp.hovered() && exists;
                    let bg = if hov { card_hover } else { card_bg };
                    let painter = ui.painter();
                    painter.rect(r, 6.0, bg, Stroke::new(1.0_f32, if hov { accent() } else { border }));
                    let (tag, col) = file_badge(p);
                    let col = if exists { col } else { col.gamma_multiply(0.35) };
                    let name = crate::file_name(p);
                    let tc = if exists { strong } else { weak };
                    let date = if exists { modified.map(fmt_date).unwrap_or_default() } else { "file not found".into() };
                    if self.start_list {
                        // row: small icon, name, folder, date
                        let icon = Rect::from_min_size(r.min + EV::new(8.0, 5.0), EV::new(26.0, 26.0));
                        painter.rect_filled(icon, 4.0, col);
                        painter.text(icon.center(), Align2::CENTER_CENTER, tag, FontId::proportional(if tag.len() > 3 { 8.0 } else { 10.0 }), Color32::WHITE);
                        painter.text(Pos2::new(r.left() + 44.0, r.center().y), Align2::LEFT_CENTER, &name, FontId::proportional(13.0), tc);
                        let folder = std::path::Path::new(p).parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
                        painter.text(Pos2::new(r.left() + 360.0, r.center().y), Align2::LEFT_CENTER, folder, FontId::proportional(11.0), weak);
                        painter.text(Pos2::new(r.right() - 10.0, r.center().y), Align2::RIGHT_CENTER, date, FontId::proportional(11.0), weak);
                    } else {
                        let icon = Rect::from_center_size(Pos2::new(r.center().x, r.top() + 50.0), EV::new(64.0, 64.0));
                        painter.rect_filled(icon.translate(EV::new(5.0, 5.0)), 6.0, col.gamma_multiply(0.55));
                        painter.rect_filled(icon, 6.0, col);
                        painter.text(icon.center(), Align2::CENTER_CENTER, tag, FontId::proportional(if tag.len() > 3 { 15.0 } else { 19.0 }), Color32::WHITE);
                        // small type icon next to the name, as in Navisworks
                        let small = Rect::from_min_size(Pos2::new(r.left() + 10.0, r.bottom() - 48.0), EV::new(14.0, 14.0));
                        painter.rect_filled(small, 2.0, col);
                        let short: String = if name.chars().count() > 18 { format!("{}…", name.chars().take(17).collect::<String>()) } else { name.clone() };
                        painter.text(Pos2::new(r.left() + 30.0, r.bottom() - 41.0), Align2::LEFT_CENTER, short, FontId::proportional(12.5), tc);
                        painter.text(Pos2::new(r.left() + 10.0, r.bottom() - 20.0), Align2::LEFT_CENTER, date, FontId::proportional(10.5), weak);
                    }
                    let resp = resp.on_hover_text(p.as_str());
                    if resp.clicked() && exists {
                        act = Some(StartAction::Open(p.clone()));
                    }
                    resp.context_menu(|ui| {
                        if exists && ui.button("Open").clicked() {
                            act = Some(StartAction::Open(p.clone()));
                            ui.close_menu();
                        }
                        if cfg!(windows) && exists && ui.button("Show in folder").clicked() {
                            Self::open_folder(p);
                            ui.close_menu();
                        }
                        if ui.button("Remove from list").clicked() {
                            forget = Some(p.clone());
                            ui.close_menu();
                        }
                    });
                };
                if self.start_list {
                    let w = ui.available_width();
                    for (_, p, m) in &recent {
                        card(ui, p, *m, EV::new(w, 36.0));
                        ui.add_space(2.0);
                    }
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = EV::new(12.0, 12.0);
                        for (_, p, m) in &recent {
                            card(ui, p, *m, EV::new(150.0, 150.0));
                        }
                    });
                }
            });
        });
        if let Some(p) = forget {
            self.settings.recent.retain(|x| x != &p);
            self.settings.save();
        }
        if let Some(a) = act {
            self.start_action(a);
        }
    }
}
