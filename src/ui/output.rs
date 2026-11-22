//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    /// Editorul cartusului (planse PDF si DWG pe plansa): campuri, format, scara.
    pub(super) fn title_block_window(&mut self, ctx: &egui::Context) {
        if !self.title_block_open {
            return;
        }
        let mut open = true;
        let mut export: Option<Purpose> = None;
        egui::Window::new(crate::theme::icon_job("📄", "Title block & sheet", 14.0, ctx.style().visuals.text_color()))
            .id(egui::Id::new("tb_win"))
            .open(&mut open)
            .default_width(420.0)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(RichText::new("Used by PDF Sheets and Cut to DWG (on sheet). Saved with the project.").small().weak());
                ui.add_space(4.0);
                let fallback = self.project_path.as_ref().map(|p| model_label(&crate::file_name(p))).unwrap_or_else(|| "ManageBIM".into());
                let tb = &mut self.title_block;
                egui::Grid::new("tb_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    let field = |ui: &mut egui::Ui, label: &str, v: &mut String, hint: &str| {
                        ui.label(label);
                        ui.add(egui::TextEdit::singleline(v).hint_text(hint).desired_width(260.0));
                        ui.end_row();
                    };
                    field(ui, "Company", &mut tb.company, "your company");
                    field(ui, "Project", &mut tb.project, &fallback);
                    field(ui, "Client", &mut tb.client, "");
                    field(ui, "Drawn by", &mut tb.drawn, "issue author");
                    field(ui, "Checked by", &mut tb.checked, "");
                    field(ui, "Sheet number prefix", &mut tb.number_prefix, "S-  ->  S-01, S-02...");
                    field(ui, "Revision", &mut tb.revision, "A");
                    field(ui, "Notes", &mut tb.notes, "one line in the title block");
                    ui.label("Sheet format");
                    ui.horizontal(|ui| {
                        for p in crate::pdf::PAPERS {
                            let (w, h) = crate::pdf::paper_mm(p);
                            if ui.selectable_label(tb.paper == p, p).on_hover_text(format!("{w:.0} x {h:.0} mm, landscape")).clicked() {
                                tb.paper = p.to_string();
                            }
                        }
                    });
                    ui.end_row();
                    ui.label("Scale (DWG)");
                    let cur = if tb.scale == 0 { "Auto (largest that fits)".to_string() } else { format!("1:{}", tb.scale) };
                    egui::ComboBox::from_id_salt("tb_scale").width(200.0).selected_text(cur).show_ui(ui, |ui| {
                        ui.selectable_value(&mut tb.scale, 0, "Auto (largest that fits)");
                        for n in crate::pdf::SCALES {
                            ui.selectable_value(&mut tb.scale, n, format!("1:{n}"));
                        }
                    });
                    ui.end_row();
                });
                ui.add_space(4.0);
                ui.checkbox(&mut self.cad_annotate, "Complete 2D drawing (Cut to DWG)").on_hover_text(
                    "Hatch on cut walls / columns / slabs, tags (door & window numbers, wall types),\ngrid axes with bubbles and dimension chains on plans, level lines and grids on sections.\nOff = only the cut lines.",
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let has_vp = !self.viewpoints.is_empty();
                    if ui.add_enabled(has_vp, egui::Button::new("📄 PDF sheets…")).on_hover_text("One sheet per saved viewpoint").clicked() {
                        export = Some(Purpose::ExportVpPdf);
                    }
                    if ui.add_enabled(self.section.enabled, egui::Button::new("📐 Cut to DWG on sheet…")).on_hover_text("The cut of the current section, scaled on this sheet").clicked() {
                        self.cad_sheet = true;
                        export = Some(Purpose::ExportSectionCad);
                    }
                    if ui.button("Reset").clicked() {
                        self.title_block = Default::default();
                    }
                });
            });
        if !open {
            self.title_block_open = false;
        }
        if let Some(p) = export {
            self.request_path(p);
        }
    }

    /// Lista viewpoint-urilor pe foldere: tragi un viewpoint pe un folder; meniul ⋯ al folderului exporta doar
    /// continutul lui (BCF, PDF, Excel, HTML, Navisworks XML cu foldere).
    pub(super) fn vp_tree_ui(&mut self, ui: &mut egui::Ui, restore: &mut Option<usize>) {
        let folders = self.vp_all_folders();
        let mut move_to: Option<(usize, String)> = None;
        let mut act: Option<(String, &'static str)> = None;
        let rename_id = egui::Id::new("vp_folder_rename");
        let mut renaming: Option<(String, String)> = ui.data(|d| d.get_temp(rename_id));
        let mut rename_vp: Option<usize> = None;
        if let Some(p) = self.vp_rename_pending.take() {
            renaming = Some(p);
        }
        let status_col = |s: &str| match s {
            "Open" => Color32::from_rgb(239, 68, 68),
            "In Progress" => Color32::from_rgb(245, 158, 11),
            "Resolved" => Color32::from_rgb(34, 197, 94),
            _ => Color32::GRAY,
        };
        egui::ScrollArea::vertical().id_salt("vp_list").auto_shrink([false, false]).show(ui, |ui| {
            // ---- un rand de viewpoint (se poate trage)
            let item = |ui: &mut egui::Ui, i: usize, depth: usize, this: &mut App, restore: &mut Option<usize>, move_to: &mut Option<(usize, String)>, rename_vp: &mut Option<usize>| {
                let status = this.viewpoints[i].issue.status.clone();
                let name = this.viewpoints[i].name.clone();
                let nred = this.viewpoints[i].redlines.len();
                let ncom = this.viewpoints[i].comments.len();
                let col = status_col(&status);
                let mut t = format!("{}. {}", i + 1, name);
                if nred > 0 {
                    t.push_str(&format!("  ✏{nred}"));
                }
                if ncom > 0 {
                    t.push_str(&format!("  💬{ncom}"));
                }
                let sel = this.vp_sel == i;
                let editing = *rename_vp == Some(i);
                let mut label_clicked = false;
                let mut label_double = false;
                let r = ui
                    .dnd_drag_source(egui::Id::new(("vp_drag", i)), i, |ui| {
                        ui.horizontal(|ui| {
                            ui.add_space(depth as f32 * 14.0);
                            let (dot, _) = ui.allocate_exact_size(EV::splat(10.0), Sense::hover());
                            ui.painter().circle_filled(dot.center(), 4.5, col);
                            let resp = if editing {
                                let id = egui::Id::new(("vp_rename_buf", i));
                                let mut buf: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| this.viewpoints[i].name.clone());
                                let te = ui.add(egui::TextEdit::singleline(&mut buf).desired_width(120.0));
                                te.request_focus();
                                ui.data_mut(|d| d.insert_temp(id, buf.clone()));
                                if let Some(new) = te.lost_focus().then(|| buf.trim().to_string()).filter(|s| !s.is_empty()) {
                                    this.viewpoints[i].name = new;
                                    *rename_vp = None;
                                    ui.data_mut(|d| d.remove::<String>(id));
                                } else if ui.input(|k| k.key_pressed(Key::Enter)) {
                                    let new = buf.trim().to_string();
                                    if !new.is_empty() {
                                        this.viewpoints[i].name = new;
                                    }
                                    *rename_vp = None;
                                    ui.data_mut(|d| d.remove::<String>(id));
                                } else if ui.input(|k| k.key_pressed(Key::Escape)) {
                                    *rename_vp = None;
                                    ui.data_mut(|d| d.remove::<String>(id));
                                }
                                te
                            } else {
                                ui.selectable_label(sel, t).on_hover_text("Click: go to the viewpoint · double-click: rename · drag onto a folder")
                            };
                            label_clicked = resp.clicked();
                            label_double = resp.double_clicked();
                            resp
                        })
                        .inner
                    })
                    .inner;
                if label_clicked && !editing {
                    this.vp_sel = i;
                    *restore = Some(i);
                }
                if label_double {
                    *rename_vp = Some(i);
                }
                r.context_menu(|ui| {
                    if ui.button("✏ Rename").clicked() {
                        *rename_vp = Some(i);
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.label(RichText::new("Move to folder").small().weak());
                    if ui.button("(no folder)").clicked() {
                        *move_to = Some((i, String::new()));
                        ui.close_menu();
                    }
                    for f in &folders {
                        if ui.button(format!("📁 {f}")).clicked() {
                            *move_to = Some((i, f.clone()));
                            ui.close_menu();
                        }
                    }
                });
            };
            // ---- antetul unui folder: tinta de plasare + meniu
            let header = |ui: &mut egui::Ui, f: &str, label: &str, depth: usize, count: usize, this: &mut App, act: &mut Option<(String, &'static str)>, renaming: &mut Option<(String, String)>, move_to: &mut Option<(usize, String)>| -> bool {
                let cur = this.vp_folder_cur == f;
                let open_id = egui::Id::new(("vp_folder_open", f));
                let mut open: bool = ui.data(|d| d.get_temp(open_id)).unwrap_or(true);
                let (_, dropped) = ui.dnd_drop_zone::<usize, ()>(egui::Frame::none(), |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(depth as f32 * 14.0);
                        if !f.is_empty() && ui.add(egui::Button::new(if open { "▾" } else { "▸" }).frame(false)).clicked() {
                            open = !open;
                        }
                        if let Some((old, buf)) = renaming.as_mut().filter(|(o, _)| o == f) {
                            let te = ui.add(egui::TextEdit::singleline(buf).desired_width(150.0));
                            te.request_focus();
                            if ui.input(|i| i.key_pressed(Key::Enter)) {
                                *act = Some((format!("{old}\u{1}{buf}"), "rename_done"));
                            }
                            if ui.input(|i| i.key_pressed(Key::Escape)) {
                                *act = Some((String::new(), "rename_cancel"));
                            }
                            // confirmarea si la click in afara (nu doar Enter), altfel
                            // redenumirea se pierde daca utilizatorul apasa in alta parte
                            if te.lost_focus() && act.is_none() {
                                if buf.trim().is_empty() {
                                    *act = Some((String::new(), "rename_cancel"));
                                } else {
                                    *act = Some((format!("{old}\u{1}{buf}"), "rename_done"));
                                }
                            }
                        } else {
                            let col = if cur { accent() } else { ui.visuals().text_color() };
                            let r = ui.add(egui::Button::new(crate::theme::icon_job(if f.is_empty() { "📋" } else { "📁" }, &format!("{label}  ({count})"), 13.0, col)).frame(false));
                            if r.clicked() {
                                this.vp_folder_cur = f.to_string();
                            }
                            r.on_hover_text(if f.is_empty() { "All viewpoints without a folder - drop here to take one out of its folder" } else { "Click: new viewpoints go into this folder · drop viewpoints here" });
                        }
                        ui.menu_button("⋯", |ui| {
                            if f.is_empty() {
                                // radacina virtuala: nu are nume de sters, dar putem
                                // crea foldere si schimba eticheta afisata
                                for (lbl, key) in [("➕ New folder", "sub"), ("✏ Rename label", "rename")] {
                                    if ui.button(lbl).clicked() {
                                        *act = Some((f.to_string(), key));
                                        ui.close_menu();
                                    }
                                }
                            } else {
                                for (lbl, key) in [
                                    ("➕ New subfolder", "sub"),
                                    ("✏ Rename", "rename"),
                                    ("🗑 Delete folder (keeps the viewpoints)", "delete"),
                                    ("📤 Export folder: BCF…", "bcf"),
                                    ("📄 Export folder: PDF sheets…", "pdf"),
                                    ("📊 Export folder: Excel…", "xlsx"),
                                    ("📄 Export folder: HTML report…", "html"),
                                    ("🧭 Export folder: Navisworks XML…", "navis"),
                                ] {
                                    if ui.button(lbl).clicked() {
                                        *act = Some((f.to_string(), key));
                                        ui.close_menu();
                                    }
                                }
                            }
                        });
                    });
                });
                if let Some(i) = dropped {
                    *move_to = Some((*i, f.to_string()));
                }
                ui.data_mut(|d| d.insert_temp(open_id, open));
                open
            };
            // radacina: viewpoint-urile fara folder
            let root_n = self.viewpoints.iter().filter(|v| v.folder.is_empty()).count();
            let root_label = self.settings.vp_root_label.clone();
            header(ui, "", &root_label, 0, root_n, self, &mut act, &mut renaming, &mut move_to);
            for i in 0..self.viewpoints.len() {
                if self.viewpoints[i].folder.is_empty() {
                    item(ui, i, 1, self, restore, &mut move_to, &mut rename_vp);
                }
            }
            // folderele (sortate: parintii inaintea copiilor); cele din folderele inchise nu se arata
            let mut closed: Vec<String> = Vec::new();
            for f in &folders {
                if closed.iter().any(|c| f.starts_with(&format!("{c}/"))) {
                    continue;
                }
                let depth = f.matches('/').count();
                let label = f.rsplit('/').next().unwrap_or(f).to_string();
                let n = self.viewpoints.iter().filter(|v| v.folder == *f || v.folder.starts_with(&format!("{f}/"))).count();
                let open = header(ui, f, &label, depth, n, self, &mut act, &mut renaming, &mut move_to);
                if !open {
                    closed.push(f.clone());
                    continue;
                }
                for i in 0..self.viewpoints.len() {
                    if self.viewpoints[i].folder == *f {
                        item(ui, i, depth + 1, self, restore, &mut move_to, &mut rename_vp);
                    }
                }
            }
        });
        if let Some((i, f)) = move_to {
            if let Some(v) = self.viewpoints.get_mut(i) {
                v.folder = f;
            }
        }
        if let Some((f, key)) = act {
            let export = |this: &mut App, p: Purpose| {
                this.vp_filter = Some(f.clone());
                this.request_path(p);
            };
            match key {
                "sub" => {
                    let mut k = 1;
                    let mut name = if f.is_empty() { "New folder".to_string() } else { format!("{f}/New folder") };
                    while self.vp_all_folders().contains(&name) {
                        k += 1;
                        name = if f.is_empty() { format!("New folder {k}") } else { format!("{f}/New folder {k}") };
                    }
                    self.vp_folders.push(name.clone());
                    renaming = Some((name.clone(), name.rsplit('/').next().unwrap_or("").to_string()));
                }
                "rename" => renaming = Some((f.clone(), f.rsplit('/').next().unwrap_or(&f).to_string())),
                "rename_cancel" => renaming = None,
                "rename_done" => {
                    if let Some((old, new)) = f.split_once('\u{1}') {
                        let new = new.trim().replace('/', "-");
                        if !new.is_empty() && old.is_empty() {
                            // radacina virtuala: doar eticheta se schimba
                            self.settings.vp_root_label = new;
                            self.settings.save();
                        } else if !new.is_empty() {
                            let parent = old.rsplit_once('/').map(|(p, _)| format!("{p}/")).unwrap_or_default();
                            let target = format!("{parent}{new}");
                            let re = |s: &str| -> Option<String> {
                                if s == old {
                                    Some(target.clone())
                                } else {
                                    s.strip_prefix(&format!("{old}/")).map(|rest| format!("{target}/{rest}"))
                                }
                            };
                            for v in &mut self.viewpoints {
                                if let Some(n) = re(&v.folder) {
                                    v.folder = n;
                                }
                            }
                            for x in &mut self.vp_folders {
                                if let Some(n) = re(x) {
                                    *x = n;
                                }
                            }
                            if let Some(n) = re(&self.vp_folder_cur) {
                                self.vp_folder_cur = n;
                            }
                        }
                    }
                    renaming = None;
                }
                "delete" => {
                    // viewpoint-urile urca in folderul parinte
                    let parent = f.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default();
                    for v in &mut self.viewpoints {
                        if v.folder == f || v.folder.starts_with(&format!("{f}/")) {
                            v.folder = parent.clone();
                        }
                    }
                    self.vp_folders.retain(|x| x != &f && !x.starts_with(&format!("{f}/")));
                    if self.vp_folder_cur == f {
                        self.vp_folder_cur = parent;
                    }
                }
                "bcf" => export(self, Purpose::ExportBcf),
                "pdf" => export(self, Purpose::ExportVpPdf),
                "xlsx" => export(self, Purpose::ExportVpXlsx),
                "html" => export(self, Purpose::ExportVpHtml),
                "navis" => export(self, Purpose::ExportNavisXml),
                _ => {}
            }
        }
        ui.data_mut(|d| match renaming {
            Some(r) => d.insert_temp(rename_id, r),
            None => d.remove::<(String, String)>(rename_id),
        });
    }

    /// folder nou la radacina (sau in folderul curent)
    pub(super) fn vp_new_folder(&mut self) {
        let base = if self.vp_folder_cur.is_empty() { String::new() } else { format!("{}/", self.vp_folder_cur) };
        let mut k = 1;
        let mut name = format!("{base}New folder");
        while self.vp_all_folders().contains(&name) {
            k += 1;
            name = format!("{base}New folder {k}");
        }
        self.vp_folders.push(name.clone());
        self.vp_folder_cur = name.clone();
        let short = name.rsplit('/').next().unwrap_or("").to_string();
        // il redenumesti imediat
        self.vp_rename_pending = Some((name, short));
    }

    /// Desenele 2D suprapuse: linii proiectate peste model (deasupra lui, ca un plan pe masa).
    /// Legenda colorarii dupa parametru (Appearance Profiler) in coltul stanga-jos al vederii.
    pub(super) fn draw_profile_legend(&self, painter: &egui::Painter, rect: Rect) {
        if !self.profile.active || self.profile.rules.is_empty() || !self.profile_legend {
            return;
        }
        let vis: Vec<&crate::profiler::ProfileRule> = self.profile.rules.iter().filter(|r| r.visible).take(14).collect();
        let more = self.profile.rules.iter().filter(|r| r.visible).count().saturating_sub(vis.len());
        let font = FontId::proportional(12.0);
        let line = 18.0;
        let w = vis.iter().map(|r| painter.layout_no_wrap(format!("{}  ({})", r.label, r.count), font.clone(), Color32::WHITE).size().x).fold(painter.layout_no_wrap(self.profile.prop.clone(), FontId::proportional(13.0), Color32::WHITE).size().x, f32::max) + 40.0;
        let h = 12.0 + line * (vis.len() as f32 + 1.2 + if more > 0 { 1.0 } else { 0.0 });
        let r = Rect::from_min_size(egui::pos2(rect.left() + 12.0, rect.bottom() - h - 70.0), egui::vec2(w.min(rect.width() * 0.45), h));
        painter.rect_filled(r, 8.0, Color32::from_rgba_unmultiplied(18, 20, 26, 205));
        let mut y = r.top() + 8.0;
        painter.text(egui::pos2(r.left() + 10.0, y), Align2::LEFT_TOP, &self.profile.prop, FontId::proportional(13.0), Color32::WHITE);
        y += line * 1.2;
        for ru in &vis {
            painter.rect_filled(Rect::from_min_size(egui::pos2(r.left() + 10.0, y + 2.0), egui::vec2(12.0, 12.0)), 3.0, Color32::from_rgb(ru.color[0], ru.color[1], ru.color[2]));
            painter.text(egui::pos2(r.left() + 28.0, y), Align2::LEFT_TOP, format!("{}  ({})", ru.label, ru.count), font.clone(), Color32::from_gray(230));
            y += line;
        }
        if more > 0 {
            painter.text(egui::pos2(r.left() + 28.0, y), Align2::LEFT_TOP, format!("+ {more} more"), font, Color32::from_gray(160));
        }
    }

    pub(super) fn draw_overlays(&self, painter: &egui::Painter, rect: Rect, vp: &glam::Mat4) {
        let mut budget = 300_000usize;
        for ov in self.overlays.iter().filter(|o| o.visible) {
            let col = Color32::from_rgba_unmultiplied(ov.color[0], ov.color[1], ov.color[2], (ov.opacity.clamp(0.05, 1.0) * 255.0) as u8);
            let (s, c) = ov.rot_deg.to_radians().sin_cos();
            let tr = |x: f32, y: f32| {
                let (mut x, y) = (x * c - y * s + ov.dx, x * s + y * c + ov.dy);
                if ov.mirror {
                    x = -x; // Back / Right: se intoarce oglindirea de la export
                }
                match ov.vert {
                    Some(0) => Vec3::new(x, ov.z, y), // plan YZ, taietura la Y
                    Some(1) => Vec3::new(ov.z, x, y), // plan XZ, taietura la X
                    _ => Vec3::new(x, y, ov.z),       // plan XY la cota Z
                }
            };
            for g in ov.segs.iter() {
                if budget == 0 {
                    return;
                }
                budget -= 1;
                if let (Some(a), Some(b)) = (to_screen(vp, rect, tr(g[0], g[1])), to_screen(vp, rect, tr(g[2], g[3]))) {
                    painter.line_segment([a, b], Stroke::new(1.2_f32, col));
                }
            }
        }
    }

    /// Fereastra suprapunerilor DWG / DXF: vizibilitate, cota (nivel), deplasare, rotatie, culoare.
    pub(super) fn overlay_window(&mut self, ctx: &egui::Context) {
        if !self.overlay_open {
            return;
        }
        let mut open = true;
        let mut remove = None;
        let mut reload = None;
        let mut add = false;
        let mut zoom = None;
        egui::Window::new(crate::theme::icon_job("📐", "DWG overlay", 14.0, ctx.style().visuals.text_color()))
            .id(egui::Id::new("overlay_win"))
            .open(&mut open)
            .default_width(440.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(RichText::new("2D drawings (DWG / DXF) laid over the model at a level - to compare the plan with the model.").small().weak());
                add = ui.button("➕ Add DWG / DXF…").clicked();
                ui.separator();
                if self.overlays.is_empty() {
                    ui.label(RichText::new("No drawings yet.").weak());
                }
                for (i, ov) in self.overlays.iter_mut().enumerate() {
                    egui::Frame::none().fill(card_fill(ui.visuals())).rounding(7.0).inner_margin(8.0).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut ov.visible, RichText::new(&ov.name).strong());
                            ui.label(RichText::new(format!("{} lines", ov.segs.len())).small().weak());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("🎯").on_hover_text("Zoom to this overlay (if you cannot see it)").clicked() {
                                    zoom = Some(i);
                                }
                                if ui.small_button("🗑").on_hover_text("Remove").clicked() {
                                    remove = Some(i);
                                }
                                if ui.small_button("⟳").on_hover_text("Read the drawing again (after it was changed)").clicked() {
                                    reload = Some(i);
                                }
                            });
                        });
                        egui::Grid::new(("ov_grid", i)).num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
                            ui.label("Plane");
                            let plane = match ov.vert { Some(0) => "Front / Back (YZ)", Some(1) => "Left / Right (XZ)", _ => "Horizontal (XY)" };
                            egui::ComboBox::from_id_salt(("ov_plane", i)).width(150.0).selected_text(plane).show_ui(ui, |ui| {
                                if ui.selectable_label(ov.vert.is_none(), "Horizontal (XY)").clicked() { ov.vert = None; }
                                if ui.selectable_label(ov.vert == Some(0), "Front / Back (YZ)").clicked() { ov.vert = Some(0); }
                                if ui.selectable_label(ov.vert == Some(1), "Left / Right (XZ)").clicked() { ov.vert = Some(1); }
                            });
                            ui.label(if ov.vert.is_some() { "Cut at" } else { "Z" });
                            ui.add(egui::DragValue::new(&mut ov.z).speed(0.01).suffix(" m"));
                            ui.end_row();
                            ui.label("Move X");
                            ui.add(egui::DragValue::new(&mut ov.dx).speed(0.01).suffix(" m"));
                            ui.label("Move Y");
                            ui.add(egui::DragValue::new(&mut ov.dy).speed(0.01).suffix(" m"));
                            ui.end_row();
                            ui.label("Rotate");
                            ui.add(egui::DragValue::new(&mut ov.rot_deg).speed(0.1).suffix("°"));
                            ui.label("Mirror X");
                            ui.checkbox(&mut ov.mirror, "");
                            ui.end_row();
                            ui.label("Colour");
                            ui.horizontal(|ui| {
                                ui.color_edit_button_srgb(&mut ov.color);
                                ui.add(egui::Slider::new(&mut ov.opacity, 0.1..=1.0).show_value(false));
                            });
                            ui.end_row();
                        });
                    });
                    ui.add_space(4.0);
                }
            });
        if !open {
            self.overlay_open = false;
        }
        if let Some(i) = zoom {
            let ov = self.overlays[i].clone();
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for s in ov.segs.iter() {
                x0 = x0.min(s[0] as f64).min(s[2] as f64);
                y0 = y0.min(s[1] as f64).min(s[3] as f64);
                x1 = x1.max(s[0] as f64).max(s[2] as f64);
                y1 = y1.max(s[1] as f64).max(s[3] as f64);
            }
            if x0 <= x1 && y0 <= y1 {
                let m = 2.0_f64;
                let bb = crate::model::Aabb {
                    min: glam::Vec3::new((x0 - m) as f32, (y0 - m) as f32, ov.z - 1.0),
                    max: glam::Vec3::new((x1 + m) as f32, (y1 + m) as f32, ov.z + 1.0),
                };
                self.cam.fit(&bb);
                self.toast(format!("Zoomed to overlay {} ({} lines).", ov.name, ov.segs.len()));
            }
        }
        if let Some(i) = remove {
            self.overlays.remove(i);
        }
        if let Some(i) = reload {
            let mut ov = self.overlays[i].clone();
            match self.load_overlay_segs(&mut ov) {
                Ok(n) => {
                    self.overlays[i] = ov;
                    self.toast(format!("Overlay reloaded: {n} lines"));
                }
                Err(e) => self.error(e),
            }
        }
        if add {
            self.request_path(Purpose::ImportOverlay);
        }
    }
}
