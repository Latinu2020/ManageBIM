//! Section of `impl App` (see the parent file for the rest).
use super::*;

impl App {
    /// The title block editor (PDF sheets and DWG on sheet): fields, format, scale.
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

    /// The list of viewpoints in folders: drag a viewpoint onto a folder; the folder's ⋯ menu exports only
    /// its content (BCF, PDF, Excel, HTML, Navisworks XML with folders).
    pub(super) fn vp_tree_ui(&mut self, ui: &mut egui::Ui, restore: &mut Option<usize>) {
        let folders = self.vp_all_folders();
        let mut move_to: Option<(usize, String)> = None;
        let mut act: Option<(String, &'static str)> = None;
        let rename_id = egui::Id::new("vp_folder_rename");
        let mut renaming: Option<(String, String)> = ui.data(|d| d.get_temp(rename_id));
        let mut rename_vp: Option<usize> = None;
        if let Some(p) = self.vp.rename_pending.take() {
            renaming = Some(p);
        }
        let status_col = |s: &str| match s {
            "Open" => Color32::from_rgb(239, 68, 68),
            "In Progress" => Color32::from_rgb(245, 158, 11),
            "Resolved" => Color32::from_rgb(34, 197, 94),
            _ => Color32::GRAY,
        };
        egui::ScrollArea::vertical().id_salt("vp_list").auto_shrink([false, false]).show(ui, |ui| {
            // ---- a viewpoint row (can be dragged)
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
                let mut resp_out: Option<egui::Response> = None;
                ui.horizontal(|ui| {
                    ui.add_space(depth as f32 * 14.0);
                    // the DRAG HANDLE is separate from the label: otherwise dnd eats the click.
                    let _drag = ui.dnd_drag_source(egui::Id::new(("vp_drag", i)), i, |ui| {
                        let (dr, _) = ui.allocate_exact_size(EV::splat(12.0), egui::Sense::hover());
                        ui.painter().circle_filled(dr.center(), 4.5, col);
                        dr
                    });
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
                        ui.selectable_label(sel, t.clone()).on_hover_text("Click: go to the viewpoint · double-click: rename · drag the dot onto a folder")
                    };
                    if resp.clicked() && !editing {
                        this.vp_sel = i;
                        *restore = Some(i);
                    }
                    if resp.double_clicked() {
                        *rename_vp = Some(i);
                    }
                    resp_out = Some(resp);
                });
                if let Some(resp) = resp_out {
                    resp.context_menu(|ui| {
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
                }
            };
            // ---- the header of a folder: drop target + menu
            let header = |ui: &mut egui::Ui, f: &str, label: &str, depth: usize, count: usize, this: &mut App, act: &mut Option<(String, &'static str)>, renaming: &mut Option<(String, String)>, move_to: &mut Option<(usize, String)>| -> bool {
                let cur = this.vp.folder_cur == f;
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
                            // confirm also on a click outside (not only Enter), otherwise
                            // the rename is lost if the user clicks somewhere else
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
                                this.vp.folder_cur = f.to_string();
                            }
                            r.on_hover_text(if f.is_empty() { "All viewpoints without a folder - drop here to take one out of its folder" } else { "Click: new viewpoints go into this folder · drop viewpoints here" });
                        }
                        ui.menu_button("⋯", |ui| {
                            if f.is_empty() {
                                // virtual root: it has no name to delete, but we can
                                // create folders and change the displayed label
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
            // root: the viewpoints without a folder
            let root_n = self.viewpoints.iter().filter(|v| v.folder.is_empty()).count();
            let root_label = self.settings.vp_root_label.clone();
            header(ui, "", &root_label, 0, root_n, self, &mut act, &mut renaming, &mut move_to);
            for i in 0..self.viewpoints.len() {
                if self.viewpoints[i].folder.is_empty() {
                    item(ui, i, 1, self, restore, &mut move_to, &mut rename_vp);
                }
            }
            // the folders (sorted: parents before children); the ones in collapsed folders are not shown
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
                this.vp.filter = Some(f.clone());
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
                    self.vp.folders.push(name.clone());
                    renaming = Some((name.clone(), name.rsplit('/').next().unwrap_or("").to_string()));
                }
                "rename" => renaming = Some((f.clone(), f.rsplit('/').next().unwrap_or(&f).to_string())),
                "rename_cancel" => renaming = None,
                "rename_done" => {
                    if let Some((old, new)) = f.split_once('\u{1}') {
                        let new = new.trim().replace('/', "-");
                        if !new.is_empty() && old.is_empty() {
                            // virtual root: only the label changes
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
                            for x in &mut self.vp.folders {
                                if let Some(n) = re(x) {
                                    *x = n;
                                }
                            }
                            if let Some(n) = re(&self.vp.folder_cur) {
                                self.vp.folder_cur = n;
                            }
                        }
                    }
                    renaming = None;
                }
                "delete" => {
                    // the viewpoints move up to the parent folder
                    let parent = f.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default();
                    for v in &mut self.viewpoints {
                        if v.folder == f || v.folder.starts_with(&format!("{f}/")) {
                            v.folder = parent.clone();
                        }
                    }
                    self.vp.folders.retain(|x| x != &f && !x.starts_with(&format!("{f}/")));
                    if self.vp.folder_cur == f {
                        self.vp.folder_cur = parent;
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

    /// new folder at the root (or in the current folder)
    pub(super) fn vp_new_folder(&mut self) {
        let base = if self.vp.folder_cur.is_empty() { String::new() } else { format!("{}/", self.vp.folder_cur) };
        let mut k = 1;
        let mut name = format!("{base}New folder");
        while self.vp_all_folders().contains(&name) {
            k += 1;
            name = format!("{base}New folder {k}");
        }
        self.vp.folders.push(name.clone());
        self.vp.folder_cur = name.clone();
        let short = name.rsplit('/').next().unwrap_or("").to_string();
        // you rename it immediately
        self.vp.rename_pending = Some((name, short));
    }

    /// The overlaid 2D drawings: lines projected over the model (above it, like a plan on the table).
    /// The legend of the coloring by parameter (Appearance Profiler) in the bottom-left corner of the view.
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
                    x = -x; // Back / Right: the mirroring from the export is undone
                }
                match ov.vert {
                    Some(0) => Vec3::new(x, ov.z, y), // YZ plane, cut at Y
                    Some(1) => Vec3::new(ov.z, x, y), // XZ plane, cut at X
                    _ => Vec3::new(x, y, ov.z),       // XY plane at elevation Z
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

    /// The DWG / DXF overlays window: visibility, elevation (level), offset, rotation, color.
    /// The window for managing the view templates: apply, rename, delete.
    // the template index drives the selection, the buttons and the rename of each row
    #[allow(clippy::needless_range_loop)]
    pub(super) fn view_templates_window(&mut self, ctx: &egui::Context) {
        if !self.view_templates_open { return; }
        let mut open = true;
        let mut apply: Option<usize> = None;
        let mut update: Option<usize> = None;
        let mut del: Option<usize> = None;
        let mut dup: Option<usize> = None;
        let mut renamed: Option<(String, String)> = None;
        let mut set_default: Option<usize> = None;
        let mut swap: Option<(usize, usize)> = None;
        let mut save = false;
        let n = self.view_templates.len();
        egui::Window::new(crate::theme::icon_job("◫", "View Templates", 14.0, ctx.style().visuals.text_color()))
            .id(egui::Id::new("view_templates_win"))
            .open(&mut open)
            .default_width(560.0)
            .default_height(480.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("➕ Save current view").strong().color(Color32::WHITE)).fill(accent())).clicked() {
                        save = true;
                    }
                    ui.label(RichText::new("Ctrl+1…9 applies a template · ◀ ▶ next to the ViewCube cycles").small().weak());
                });
                ui.separator();
                if n == 0 {
                    ui.add_space(8.0);
                    ui.label(RichText::new("No templates yet").strong());
                    ui.label(RichText::new("Set up a view (camera, hidden elements, section, style, colours) and press Save current view. Templates are saved in the project.").small().weak());
                    return;
                }
                let cur = self.vt_current;
                egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
                    for i in 0..n {
                        let active = cur == i;
                        let frame_col = if active { accent() } else { crate::theme::border() };
                        egui::Frame::none()
                            .fill(if active { accent().gamma_multiply(0.10) } else { ui.visuals().faint_bg_color })
                            .rounding(crate::theme::RADIUS_MD)
                            .stroke(egui::Stroke::new(1.0_f32, frame_col))
                            .inner_margin(egui::Margin::symmetric(10.0, 7.0))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                let t = &mut self.view_templates[i];
                                ui.horizontal(|ui| {
                                    // the number of the Ctrl+1..9 shortcut
                                    let key = if i < 9 { format!("{}", i + 1) } else { "·".into() };
                                    ui.label(RichText::new(key).monospace().strong().color(accent())).on_hover_text("Ctrl + this number applies the template");
                                    if t.is_default {
                                        ui.label(RichText::new("★").color(accent())).on_hover_text("Default: applied when the project opens");
                                    }
                                    let old_name = t.name.clone();
                                    if ui.add(egui::TextEdit::singleline(&mut t.name).desired_width(170.0).font(egui::TextStyle::Body)).changed() {
                                        renamed = Some((old_name, t.name.clone()));
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.small_button("🗑").on_hover_text("Delete").clicked() { del = Some(i); }
                                        if ui.small_button("⧉").on_hover_text("Duplicate").clicked() { dup = Some(i); }
                                        if ui.small_button("★").on_hover_text("Set as default (applied when the project opens)").clicked() { set_default = Some(i); }
                                        if ui.add_enabled(i + 1 < n, egui::Button::new("↓").small()).on_hover_text("Move down").clicked() { swap = Some((i, i + 1)); }
                                        if ui.add_enabled(i > 0, egui::Button::new("↑").small()).on_hover_text("Move up").clicked() { swap = Some((i, i - 1)); }
                                        if ui.small_button("⟳ Update").on_hover_text("Overwrite this template with the current view (keeps name, note and what it includes)").clicked() { update = Some(i); }
                                        if ui.add(egui::Button::new(RichText::new("▶ Apply").strong())).clicked() { apply = Some(i); }
                                    });
                                });
                                // summary: what the template contains
                                let mut chips: Vec<String> = vec![t.visual_style.label().to_string()];
                                if t.section.enabled {
                                    chips.push(if t.section.mode == crate::app::SectionMode::Box { "Section box".into() } else { "Section planes".into() });
                                }
                                if !t.hidden.is_empty() { chips.push(format!("{} hidden", t.hidden.len())); }
                                if !t.overrides.is_empty() { chips.push(format!("{} coloured", t.overrides.len())); }
                                if !t.selected.is_empty() { chips.push(format!("{} selected", t.selected.len())); }
                                if t.show_grids { chips.push("Grids".into()); }
                                if t.show_levels { chips.push("Levels".into()); }
                                ui.horizontal_wrapped(|ui| {
                                    for c in &chips {
                                        egui::Frame::none().fill(ui.visuals().extreme_bg_color).rounding(8.0).inner_margin(egui::Margin::symmetric(6.0, 1.0)).show(ui, |ui| {
                                            ui.label(RichText::new(c).small());
                                        });
                                    }
                                });
                                // what it applies (like „Include" in Revit View Templates)
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new("Applies").small().weak());
                                    for (k, label) in crate::app::VT_INCLUDE.iter().enumerate() {
                                        ui.checkbox(&mut t.include[k], RichText::new(*label).small());
                                    }
                                });
                                ui.add(egui::TextEdit::singleline(&mut t.note).hint_text("Note: what this view is for").desired_width(f32::INFINITY).font(egui::TextStyle::Small));
                            });
                        ui.add_space(4.0);
                    }
                });
            });
        if save { self.save_view_template(); }
        if let Some(i) = apply { self.apply_view_template(i); }
        if let Some(i) = update { self.update_view_template(i); }
        if let Some(i) = set_default {
            for (k, t) in self.view_templates.iter_mut().enumerate() {
                t.is_default = k == i && !t.is_default;
            }
        }
        if let Some((a, b)) = swap {
            self.view_templates.swap(a, b);
            if self.vt_current == a { self.vt_current = b; } else if self.vt_current == b { self.vt_current = a; }
        }
        if let Some(i) = dup {
            if let Some(t) = self.view_templates.get(i).cloned() {
                let mut c = t;
                c.name = format!("{} copy", c.name);
                c.is_default = false;
                self.view_templates.insert(i + 1, c);
            }
        }
        // rename: the viewpoints linked to the template follow it
        if let Some((old, new)) = renamed {
            for v in self.viewpoints.iter_mut().filter(|v| v.template == old) {
                v.template = new.clone();
            }
        }
        if let Some(i) = del {
            if i < self.view_templates.len() {
                let gone = self.view_templates.remove(i).name;
                // the links to the deleted template disappear (the viewpoint keeps its own look)
                for v in self.viewpoints.iter_mut().filter(|v| v.template == gone) {
                    v.template.clear();
                }
                if self.vt_current >= i && self.vt_current > 0 {
                    self.vt_current -= 1;
                }
            }
        }
        self.view_templates_open = open;
    }

    /// The 2D preview of the drawing from the section box, in Inventor style: cream sheet with border and
    /// title block, thick cut outline, visible edges, dashed hidden lines, fine hatch.
    /// Zoom with the wheel, pan with the mouse, toggles and direct export to PDF / DWG.
    /// The drawing is EXACTLY the exported one (the same `section_drawing` function, the same real coordinates).
    pub(super) fn section_preview_window(&mut self, ctx: &egui::Context) {
        if !self.sec.preview_open {
            return;
        }
        let mut open = true;
        let screen = ctx.screen_rect();
        let size = egui::vec2(860.0f32.min(screen.width() * 0.6), 600.0f32.min(screen.height() * 0.62));
        let mut export: Option<Purpose> = None;
        egui::Window::new(crate::theme::icon_job("🗺", "2D Drawing Preview", 13.0, ctx.style().visuals.text_color()))
            .id(egui::Id::new("sec_preview_win2"))
            .open(&mut open)
            .default_size(size)
            .min_width(420.0)
            .min_height(300.0)
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                // ---- the toolbar
                let mut changed = false;
                ui.horizontal_wrapped(|ui| {
                    let view = self.settings.cad_view;
                    ui.label(RichText::new(match view { crate::app::CadView::Current => "Plan / section".to_string(), v => format!("{} view", v.label()) }).strong());
                    ui.separator();
                    changed |= ui.toggle_value(&mut self.cad_visible_edges, "▭ Visible edges").on_hover_text("Edges of the shapes seen beyond the cut (like an Inventor drawing view)").changed();
                    changed |= ui.toggle_value(&mut self.cad_hidden_edges, "┄ Hidden lines").on_hover_text("Edges behind other faces, dashed").changed();
                    changed |= ui.toggle_value(&mut self.cad_hatch, "▨ Hatch").changed();
                    changed |= ui.toggle_value(&mut self.cad_dims, "↔ Dimensions").changed();
                    ui.separator();
                    if ui.button("⛶ Fit").on_hover_text("Fit the drawing in the window (double-click the sheet does the same)").clicked() {
                        self.prev_zoom = 1.0;
                        self.prev_pan = [0.0, 0.0];
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("📐 DWG…").on_hover_text("Export this drawing to DWG / DXF in real coordinates (attach it back as an overlay and it lands exactly in place)").clicked() {
                            self.cad_sheet = false;
                            export = Some(Purpose::ExportSectionCad);
                        }
                        if ui.button(RichText::new("📄 PDF…").strong()).on_hover_text("Vector PDF sheet with title block").clicked() {
                            export = Some(Purpose::ExportSectionPdf);
                        }
                    });
                });
                if changed {
                    self.settings.save();
                }
                // ---- the sheet
                let (area, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                let painter = ui.painter_at(area);
                let paper = Color32::from_rgb(246, 243, 232); // cream, like the Inventor sheet
                let ink = Color32::from_rgb(20, 22, 26);
                painter.rect_filled(area, 4.0, Color32::from_rgb(120, 124, 132));
                let sheet = area.shrink(10.0);
                painter.rect_filled(sheet, 0.0, paper);
                let frame = sheet.shrink(12.0);
                painter.rect_stroke(frame, 0.0, Stroke::new(1.4_f32, ink));
                if !self.section.enabled && self.settings.cad_view == crate::app::CadView::Current {
                    painter.text(frame.center(), Align2::CENTER_CENTER, "Turn on the section box, then pick a face (Annotation › Pick face)", FontId::proportional(13.0), Color32::from_rgb(90, 90, 90));
                    return;
                }
                let (segs, texts) = self.section_preview_drawing();
                if segs.is_empty() {
                    painter.text(frame.center(), Align2::CENTER_CENTER, "Nothing to draw here - move the cube or the cut over the model", FontId::proportional(13.0), Color32::from_rgb(90, 90, 90));
                    return;
                }
                // the title block (bottom-right) - the drawing area is the rest of the frame
                let tb = egui::Rect::from_min_size(egui::pos2(frame.right() - 250.0, frame.bottom() - 58.0), egui::vec2(250.0, 58.0));
                let draw_area = egui::Rect::from_min_max(frame.min + egui::vec2(14.0, 14.0), egui::pos2(frame.right() - 14.0, tb.top() - 8.0));
                // zoom / pan
                if resp.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll.abs() > 0.1 {
                        let old = self.prev_zoom;
                        let z = (old * (1.0 + scroll * 0.0025)).clamp(0.2, 60.0);
                        // zoom toward the cursor
                        if let Some(h) = resp.hover_pos() {
                            let c = draw_area.center() + egui::vec2(self.prev_pan[0], self.prev_pan[1]);
                            let d = h - c;
                            let k = z / old;
                            self.prev_pan[0] -= d.x * (k - 1.0);
                            self.prev_pan[1] -= d.y * (k - 1.0);
                        }
                        self.prev_zoom = z;
                    }
                }
                if resp.dragged() {
                    let d = resp.drag_delta();
                    self.prev_pan[0] += d.x;
                    self.prev_pan[1] += d.y;
                }
                if resp.double_clicked() {
                    self.prev_zoom = 1.0;
                    self.prev_pan = [0.0, 0.0];
                }
                let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                for (_, s) in &segs {
                    x0 = x0.min(s[0]).min(s[2]);
                    y0 = y0.min(s[1]).min(s[3]);
                    x1 = x1.max(s[0]).max(s[2]);
                    y1 = y1.max(s[1]).max(s[3]);
                }
                let (w, h) = ((x1 - x0).max(1e-3), (y1 - y0).max(1e-3));
                let fit = ((draw_area.width() as f64 / w).min(draw_area.height() as f64 / h)) as f32;
                let sc = fit * self.prev_zoom;
                let c = draw_area.center() + egui::vec2(self.prev_pan[0], self.prev_pan[1]);
                let (cx, cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
                let pt = |x: f64, y: f64| egui::pos2(c.x + ((x - cx) as f32) * sc, c.y - ((y - cy) as f32) * sc);
                let clip = painter.with_clip_rect(draw_area.expand(4.0));
                // the order: fine (hatch, grids) -> hidden -> visible -> the thick cut outline
                let rank = |l: &str| match l.to_ascii_uppercase().as_str() {
                    "HATCH" | "GRIDS" | "LEVELS" | "DIMS" => 0,
                    "HIDDEN" | "BEHIND" => 1,
                    "VISIBLE" => 2,
                    _ => 3,
                };
                let mut order: Vec<usize> = (0..segs.len()).collect();
                order.sort_by_key(|&i| rank(&segs[i].0));
                let thick = (1.7 * self.prev_zoom.sqrt()).clamp(1.2, 3.0);
                for i in order {
                    let (layer, s) = &segs[i];
                    let (a, b) = (pt(s[0], s[1]), pt(s[2], s[3]));
                    if !draw_area.expand(40.0).intersects(egui::Rect::from_two_pos(a, b)) {
                        continue;
                    }
                    match rank(layer) {
                        0 => {
                            let col = if layer.eq_ignore_ascii_case("HATCH") { Color32::from_rgb(150, 150, 145) } else { Color32::from_rgb(70, 95, 140) };
                            clip.line_segment([a, b], Stroke::new(0.6_f32, col));
                        }
                        1 => {
                            clip.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(0.8_f32, Color32::from_rgb(95, 98, 105)), 5.0, 3.0));
                        }
                        2 => {
                            clip.line_segment([a, b], Stroke::new(1.0_f32, ink));
                        }
                        _ => {
                            clip.line_segment([a, b], Stroke::new(thick, Color32::BLACK));
                        }
                    }
                }
                for t in &texts {
                    let size = (t.mm as f32 * 2.2 * self.prev_zoom.sqrt()).clamp(7.0, 18.0);
                    clip.text(pt(t.x, t.y), Align2::LEFT_BOTTOM, &t.s, FontId::proportional(size), Color32::from_rgb(25, 30, 40));
                }
                // ---- the title block, like in Inventor (project / title / scale / date / number)
                painter.rect_filled(tb, 0.0, paper);
                painter.rect_stroke(tb, 0.0, Stroke::new(1.2_f32, ink));
                let mid = tb.left() + 160.0;
                painter.line_segment([egui::pos2(tb.left(), tb.top() + 26.0), egui::pos2(tb.right(), tb.top() + 26.0)], Stroke::new(0.8_f32, ink));
                painter.line_segment([egui::pos2(mid, tb.top() + 26.0), egui::pos2(mid, tb.bottom())], Stroke::new(0.8_f32, ink));
                let project = self.project_path.as_ref().map(|p| crate::file_name(p)).unwrap_or_else(|| "ManageBIM".into());
                let title = match self.settings.cad_view { crate::app::CadView::Current => "Section".to_string(), v => format!("{} view", v.label()) };
                painter.text(tb.left_top() + egui::vec2(6.0, 5.0), Align2::LEFT_TOP, self.title_block.project_or(&project), FontId::proportional(13.0), ink);
                painter.text(egui::pos2(tb.left() + 6.0, tb.top() + 31.0), Align2::LEFT_TOP, "TITLE", FontId::proportional(8.0), Color32::from_rgb(90, 90, 90));
                painter.text(egui::pos2(tb.left() + 6.0, tb.top() + 41.0), Align2::LEFT_TOP, &title, FontId::proportional(12.0), ink);
                let scale = if self.title_block.scale > 0 { format!("1:{}", self.title_block.scale) } else { "Fit".into() };
                painter.text(egui::pos2(mid + 6.0, tb.top() + 31.0), Align2::LEFT_TOP, format!("SCALE {scale}"), FontId::proportional(9.0), ink);
                painter.text(egui::pos2(mid + 6.0, tb.top() + 44.0), Align2::LEFT_TOP, self.title_block.paper.clone(), FontId::proportional(9.0), ink);
                // the zoom indicator, subtle
                painter.text(egui::pos2(frame.left() + 6.0, frame.bottom() - 6.0), Align2::LEFT_BOTTOM, format!("{:.0}%  ·  wheel: zoom  ·  drag: pan  ·  double-click: fit", self.prev_zoom * 100.0), FontId::proportional(10.0), Color32::from_rgb(110, 110, 110));
            });
        self.sec.preview_open = open;
        if let Some(p) = export {
            self.request_path(p);
        }
    }

    /// The Multi view window: you choose the section box views and the distance between them, you see the sheet
    /// (exactly the exported one) and you export PDF / DWG. Zoom with the wheel, pan with the mouse.
    pub(super) fn multiview_window(&mut self, ctx: &egui::Context) {
        if !self.mv_open {
            return;
        }
        let mut open = true;
        let screen = ctx.screen_rect();
        let size = egui::vec2(980.0f32.min(screen.width() * 0.66), 640.0f32.min(screen.height() * 0.66));
        let mut export = false;
        egui::Window::new(crate::theme::icon_job("◫", "Multi-view drawing", 13.0, ctx.style().visuals.text_color()))
            .id(egui::Id::new("mv_win"))
            .open(&mut open)
            .default_size(size)
            .min_width(520.0)
            .min_height(340.0)
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                // ---- the views and the distance
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Views").strong());
                    for (k, name) in ["Front", "Right", "Back", "Left", "Top", "Bottom"].iter().enumerate() {
                        ui.toggle_value(&mut self.mv_views[k], *name);
                    }
                    ui.separator();
                    ui.label(RichText::new("Spacing").strong()).on_hover_text("Distance between the views on the sheet (real metres, 1:1 in DWG)");
                    let mut auto = self.mv_gap <= 0.0;
                    if ui.checkbox(&mut auto, "Auto").on_hover_text("15% of the largest view, at least 2 m").changed() {
                        self.mv_gap = if auto { 0.0 } else { self.mv_cache.as_ref().map(|c| c.1.gap as f32).unwrap_or(3.0).max(0.5) };
                    }
                    if !auto {
                        ui.add(egui::DragValue::new(&mut self.mv_gap).range(0.1..=500.0).speed(0.1).max_decimals(2).suffix(" m"));
                    }
                    ui.separator();
                    ui.checkbox(&mut self.mv_titles, "Titles").on_hover_text("FRONT VIEW, TOP VIEW... under each view");
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Lines").strong());
                    let mut changed = false;
                    changed |= ui.toggle_value(&mut self.cad_visible_edges, "▭ Visible edges").changed();
                    changed |= ui.toggle_value(&mut self.cad_hidden_edges, "┄ Hidden lines").changed();
                    changed |= ui.toggle_value(&mut self.cad_hatch, "▨ Hatch").changed();
                    changed |= ui.toggle_value(&mut self.cad_dims, "↔ Dimensions").changed();
                    if changed {
                        self.settings.save();
                    }
                    ui.separator();
                    if ui.button("⛶ Fit").clicked() {
                        ui.data_mut(|d| d.insert_temp(egui::Id::new("mv_view"), (1.0f32, egui::Vec2::ZERO)));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("⬇ Export PDF / DWG…").strong()).on_hover_text("DWG / DXF: 1:1 in model space, Front in real coordinates (attach it back as an overlay and it lands in place).\nPDF: the same sheet with title block.").clicked() {
                            export = true;
                        }
                    });
                });
                let sheet = self.multiview_layout();
                let (area, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                let painter = ui.painter_at(area);
                let paper = Color32::from_rgb(246, 243, 232);
                let ink = Color32::from_rgb(20, 22, 26);
                painter.rect_filled(area, 4.0, Color32::from_rgb(120, 124, 132));
                let sheet_r = area.shrink(10.0);
                painter.rect_filled(sheet_r, 0.0, paper);
                let frame = sheet_r.shrink(12.0);
                painter.rect_stroke(frame, 0.0, Stroke::new(1.4_f32, ink));
                let sheet = match sheet {
                    Ok(s) => s,
                    Err(e) => {
                        painter.text(frame.center(), Align2::CENTER_CENTER, e, FontId::proportional(13.0), Color32::from_rgb(90, 90, 90));
                        return;
                    }
                };
                let draw_area = frame.shrink(18.0);
                let vid = egui::Id::new("mv_view");
                let (mut zoom, mut pan): (f32, egui::Vec2) = ui.data(|d| d.get_temp(vid)).unwrap_or((1.0, egui::Vec2::ZERO));
                if resp.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll.abs() > 0.1 {
                        let z = (zoom * (1.0 + scroll * 0.0025)).clamp(0.2, 60.0);
                        if let Some(h) = resp.hover_pos() {
                            let d = h - (draw_area.center() + pan);
                            pan -= d * (z / zoom - 1.0);
                        }
                        zoom = z;
                    }
                }
                if resp.dragged() {
                    pan += resp.drag_delta();
                }
                if resp.double_clicked() {
                    zoom = 1.0;
                    pan = egui::Vec2::ZERO;
                }
                ui.data_mut(|d| d.insert_temp(vid, (zoom, pan)));
                // the limits: segments + titles
                let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                for (_, f) in &sheet.frames {
                    x0 = x0.min(f[0]);
                    y0 = y0.min(f[1]);
                    x1 = x1.max(f[2]);
                    y1 = y1.max(f[3]);
                }
                if self.mv_titles {
                    y0 -= sheet.gap * 0.5;
                }
                let (w, h) = ((x1 - x0).max(1e-3), (y1 - y0).max(1e-3));
                let fit = ((draw_area.width() as f64 / w).min(draw_area.height() as f64 / h)) as f32;
                let sc = fit * zoom;
                let c = draw_area.center() + pan;
                let (cx, cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
                let pt = |x: f64, y: f64| egui::pos2(c.x + ((x - cx) as f32) * sc, c.y - ((y - cy) as f32) * sc);
                let clip = painter.with_clip_rect(frame.shrink(2.0));
                // the frame of each view (only in the preview) + the distance between the views
                let blue = Color32::from_rgb(70, 120, 200);
                for (name, f) in &sheet.frames {
                    let r = egui::Rect::from_two_pos(pt(f[0], f[1]), pt(f[2], f[3])).expand(4.0);
                    for e in [[r.left_top(), r.right_top()], [r.right_top(), r.right_bottom()], [r.right_bottom(), r.left_bottom()], [r.left_bottom(), r.left_top()]] {
                        clip.extend(egui::Shape::dashed_line(&e, Stroke::new(0.8_f32, blue.gamma_multiply(0.6)), 4.0, 3.0));
                    }
                    if !self.mv_titles {
                        clip.text(r.left_top() + egui::vec2(2.0, -2.0), Align2::LEFT_BOTTOM, name, FontId::proportional(10.0), blue);
                    }
                }
                // the dimension of the distance between the first two adjacent views on the same row
                if sheet.frames.len() > 1 {
                    let a = sheet.frames[0].1;
                    if let Some((_, b)) = sheet.frames.iter().skip(1).find(|(_, b)| (b[1] - a[1]).abs() < 1e-6 && b[0] > a[2]) {
                        let y = (a[1] + a[3]) * 0.5;
                        let (p, q) = (pt(a[2], y), pt(b[0], y));
                        clip.arrow(p, (q - p) * 0.5, Stroke::new(1.0_f32, blue));
                        clip.arrow(q, (p - q) * 0.5, Stroke::new(1.0_f32, blue));
                        clip.text(egui::pos2((p.x + q.x) * 0.5, p.y - 3.0), Align2::CENTER_BOTTOM, format!("{:.2} m", b[0] - a[2]), FontId::proportional(11.0), blue);
                    }
                }
                let rank = |l: &str| match l.to_ascii_uppercase().as_str() {
                    "HATCH" | "GRIDS" | "LEVELS" | "DIMS" => 0,
                    "HIDDEN" | "BEHIND" => 1,
                    "VISIBLE" => 2,
                    _ => 3,
                };
                let mut order: Vec<usize> = (0..sheet.segs.len()).collect();
                order.sort_by_key(|&i| rank(&sheet.segs[i].0));
                let thick = (1.5 * zoom.sqrt()).clamp(1.1, 3.0);
                for i in order {
                    let (layer, s) = &sheet.segs[i];
                    let (a, b) = (pt(s[0], s[1]), pt(s[2], s[3]));
                    if !frame.expand(40.0).intersects(egui::Rect::from_two_pos(a, b)) {
                        continue;
                    }
                    match rank(layer) {
                        0 => {
                            let col = if layer.eq_ignore_ascii_case("HATCH") { Color32::from_rgb(150, 150, 145) } else { Color32::from_rgb(70, 95, 140) };
                            clip.line_segment([a, b], Stroke::new(0.5_f32, col));
                        }
                        1 => {
                            clip.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(0.7_f32, Color32::from_rgb(95, 98, 105)), 4.0, 3.0));
                        }
                        2 => {
                            clip.line_segment([a, b], Stroke::new(0.9_f32, ink));
                        }
                        _ => {
                            clip.line_segment([a, b], Stroke::new(thick, Color32::BLACK));
                        }
                    }
                }
                for t in &sheet.texts {
                    let big = t.layer == "VIEW_TITLES";
                    let size = if big { (11.0 * zoom.sqrt()).clamp(9.0, 20.0) } else { (t.mm as f32 * 2.0 * zoom.sqrt()).clamp(6.5, 16.0) };
                    clip.text(pt(t.x, t.y), Align2::LEFT_BOTTOM, &t.s, FontId::proportional(size), if big { ink } else { Color32::from_rgb(25, 30, 40) });
                }
                painter.text(
                    egui::pos2(frame.left() + 6.0, frame.bottom() - 6.0),
                    Align2::LEFT_BOTTOM,
                    format!("{} views · spacing {:.2} m · DWG 1:1 in model space · {:.0}%  ·  wheel: zoom  ·  drag: pan  ·  double-click: fit", sheet.frames.len(), sheet.gap, zoom * 100.0),
                    FontId::proportional(10.0),
                    Color32::from_rgb(110, 110, 110),
                );
            });
        self.mv_open = open;
        if export {
            self.request_path(Purpose::ExportMultiView);
        }
    }

    /// The segments + texts of the 2D section drawing: cut, hatch, chain dimensions, levels, grids.
    /// The same functions used by the DWG / PDF export (`hatch_lines`, `section_annotations`).
    fn section_preview_drawing(&self) -> (crate::model::CadSegs, Vec<crate::app::CadText>) {
        // EXACTLY the same source as the DWG / PDF export - there is no discrepancy anymore.
        match self.section_drawing() {
            Some((segs, texts, _, _)) => (segs, texts),
            None => (Vec::new(), Vec::new()),
        }
    }

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
