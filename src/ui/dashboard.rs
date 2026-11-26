//! Fereastra Dashboard: sumarul proiectului (modele, clash-uri, issues, verificari, niveluri) si exportul
//! ca raport PDF / Word (crate::dashboard).
use super::*;
use crate::dashboard::{ClashRow, Jpeg, ModelRow, Summary, SEVERITY, SEVERITY_RGB, STATUS, STATUS_RGB};
use egui_phosphor::regular as P;

fn count_list<'a>(it: impl Iterator<Item = &'a str>) -> Vec<(String, usize)> {
    let mut m: std::collections::BTreeMap<String, usize> = Default::default();
    for s in it {
        *m.entry(s.trim().to_string()).or_default() += 1;
    }
    let mut v: Vec<(String, usize)> = m.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

/// bara orizontala impartita in segmente colorate
fn stacked_bar(ui: &mut egui::Ui, parts: &[(String, usize, [u8; 3])], w: f32) {
    let total: usize = parts.iter().map(|p| p.1).sum();
    let (r, resp) = ui.allocate_exact_size(egui::vec2(w.max(1.0), 12.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, 4.0, ui.visuals().extreme_bg_color);
    if total > 0 {
        let mut x = r.min.x;
        for (_, n, c) in parts {
            let sw = r.width() * *n as f32 / total as f32;
            if sw > 0.5 {
                p.rect_filled(egui::Rect::from_min_size(egui::pos2(x, r.min.y), egui::vec2(sw, r.height())), 2.0, Color32::from_rgb(c[0], c[1], c[2]));
            }
            x += sw;
        }
    }
    let tip: Vec<String> = parts.iter().filter(|p| p.1 > 0).map(|(n, c, _)| format!("{n}: {c}")).collect();
    if !tip.is_empty() {
        resp.on_hover_text(tip.join("\n"));
    }
}

impl App {
    /// Sumarul proiectului, din starea curenta.
    pub(crate) fn dashboard_summary(&self) -> Summary {
        let fallback = self.project_path.as_deref().map(crate::file_name).unwrap_or_else(|| self.loaded_models().next().map(|(_, m)| m.name.clone()).unwrap_or_else(|| "Project".into()));
        let mut s = Summary {
            project: self.title_block.project_or(&fallback).to_string(),
            author: crate::markup::user_name(),
            date: crate::markup::now_iso().chars().take(10).collect(),
            ..Default::default()
        };
        let mut cats: std::collections::HashMap<&str, usize> = Default::default();
        for (_, m) in self.loaded_models() {
            let fmt = std::path::Path::new(&m.path).extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
            s.models.push(ModelRow { name: m.name.clone(), format: fmt, elements: m.elements.len(), triangles: m.tri_total(), points: m.cloud.as_ref().map(|c| c.pos.len()).unwrap_or(0) });
            for e in &m.elements {
                *cats.entry(e.ifc_type.as_str()).or_default() += 1;
            }
        }
        let mut c: Vec<(String, usize)> = cats.into_iter().filter(|(k, _)| !k.is_empty()).map(|(k, v)| (k.to_string(), v)).collect();
        c.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        c.truncate(12);
        s.categories = c;
        let oz = self.origin_f64().z;
        let zero = if self.use_user_levels { self.level_zero } else { 0.0 };
        s.levels = self.scene_grids().levels.iter().map(|(n, z)| (n.clone(), *z as f64 + oz - zero)).collect();
        for t in &self.clash_tests {
            let mut by_status = [0usize; 5];
            let mut by_sev = [0usize; 3];
            for r in &t.results {
                let k = ClashStatus::ALL.iter().position(|x| *x == r.status).unwrap_or(0);
                by_status[k] += 1;
                if k <= 2 {
                    let sv = t.severity(r.dist);
                    by_sev[crate::app::Severity::ALL.iter().position(|x| *x == sv).unwrap_or(2)] += 1;
                }
            }
            let kind = match t.special {
                Some((crate::egress::Check::Headroom, p)) => format!("Headroom {:.2} m", p.min_headroom),
                Some((crate::egress::Check::Egress, p)) => format!("Egress {:.0} m", p.max_travel),
                None => t.ty.label().to_string(),
            };
            // fiecare rezultat, cu elementele, locatia (axe / nivel), responsabilul si comentariul
            let item = |id: ElemId| self.elem(id).map(|(m, e)| format!("{} [{}]", if e.name.is_empty() { &e.ifc_type } else { &e.name }, m.name)).unwrap_or_default();
            let level = |id: ElemId| self.elem(id).map(|(_, e)| e.storey.clone()).unwrap_or_default();
            let results: Vec<crate::dashboard::ResultRow> = t
                .results
                .iter()
                .map(|r| {
                    let loc = self.grid_location(Vec3::from(r.point));
                    crate::dashboard::ResultRow {
                        name: r.name.clone(),
                        group: r.group.clone(),
                        status: r.status.label().to_string(),
                        severity: t.severity(r.dist).label().to_string(),
                        distance_mm: r.dist * 1000.0,
                        item_a: item(r.a),
                        item_b: if r.b_is_scan { "Point cloud".into() } else { item(r.b) },
                        location: if loc.is_empty() { level(r.a) } else { loc },
                        assigned: r.assigned.clone(),
                        comment: r.comment.clone(),
                        found: r.found.clone(),
                        image: None,
                    }
                })
                .collect();
            // grupurile: numar, deschise, statusul cel mai frecvent, responsabilii
            let mut gmap: std::collections::BTreeMap<String, Vec<&ClashResult>> = Default::default();
            for r in t.results.iter().filter(|r| !r.group.is_empty()) {
                gmap.entry(r.group.clone()).or_default().push(r);
            }
            let groups = gmap
                .into_iter()
                .map(|(name, rs)| {
                    let open = rs.iter().filter(|r| matches!(r.status, ClashStatus::New | ClashStatus::Active | ClashStatus::Reviewed)).count();
                    let status = count_list(rs.iter().map(|r| r.status.label())).first().map(|x| x.0.clone()).unwrap_or_default();
                    let mut who: Vec<&str> = rs.iter().map(|r| r.assigned.as_str()).filter(|a| !a.is_empty()).collect();
                    who.sort();
                    who.dedup();
                    crate::dashboard::GroupRow { name, count: rs.len(), open, status, assigned: who.join(", ") }
                })
                .collect();
            let tolerance = match t.special {
                Some((crate::egress::Check::Headroom, p)) => format!("{:.2} m clear height", p.min_headroom),
                Some((crate::egress::Check::Egress, p)) => format!("{:.0} m travel", p.max_travel),
                None => format!("{} mm", t.tol_mm),
            };
            s.clash.push(ClashRow { name: t.name.clone(), kind, total: t.results.len(), by_status, by_severity: by_sev, last_run: t.last_run.clone(), tolerance, groups, results });
        }
        s.viewpoints = self.viewpoints.len();
        // fiecare viewpoint (issue) cu markup-urile lui; imaginile se ataseaza la export
        s.views = self
            .viewpoints
            .iter()
            .map(|v| crate::dashboard::VpRow {
                name: v.name.clone(),
                folder: v.folder.clone(),
                status: v.issue.status.clone(),
                priority: v.issue.priority.clone(),
                assigned: v.issue.assigned.clone(),
                author: v.issue.author.clone(),
                created: crate::markup::pretty_date(&v.issue.created),
                description: v.comment.clone(),
                comments: v.comments.iter().map(|c| format!("{} ({}): {}", c.author, crate::markup::pretty_date(&c.date), c.text)).collect(),
                markups: v.redlines.len(),
                image: None,
            })
            .collect();
        s.issue_status = count_list(self.viewpoints.iter().map(|v| v.issue.status.as_str()));
        s.issue_priority = count_list(self.viewpoints.iter().map(|v| v.issue.priority.as_str()));
        s.issue_assignee = count_list(self.viewpoints.iter().map(|v| v.issue.assigned.as_str()));
        if let Some(res) = &self.check_results {
            let checked: usize = res.iter().map(|r| r.checked).sum();
            let failed: usize = res.iter().map(|r| r.failed.len()).sum();
            s.check = Some((self.check_rules.len().max(res.len()), checked, failed));
        }
        s.sets = self.sets.len();
        s.coord_objects = self.coord_objects.len();
        s.materials = self.mat_assign.len();
        s.tasks = self.tasks.len();
        s.hidden = self.hidden.len();
        s
    }

    pub(super) fn dashboard_window(&mut self, ctx: &egui::Context) {
        if !self.dash_open {
            return;
        }
        if self.dash.is_none() {
            self.dash = Some(self.dashboard_summary());
        }
        let Some(s) = self.dash.clone() else { return };
        let mut open = true;
        let mut export: Option<Purpose> = None;
        let mut refresh = false;
        let mut goto: Option<Panel> = None;
        let screen = ctx.screen_rect();
        egui::Window::new(crate::theme::icon_job(P::CHART_BAR, crate::i18n::tr("Dashboard"), 15.0, ctx.style().visuals.strong_text_color()))
            .open(&mut open)
            .default_size([(screen.width() * 0.7).clamp(720.0, 1300.0), (screen.height() * 0.8).max(500.0)])
            .min_width(640.0)
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&s.project).size(18.0).strong());
                        ui.label(RichText::new(format!("{} · {}", s.date, s.author)).weak());
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(crate::theme::icon_job(P::FILE_DOC, crate::i18n::tr("Word"), 14.0, ui.visuals().text_color())).on_hover_text(crate::i18n::tr("Export the dashboard as a Word report (.docx)")).clicked() {
                            export = Some(Purpose::ExportDashboardDocx);
                        }
                        if ui.button(crate::theme::icon_job(P::FILE_PDF, crate::i18n::tr("PDF"), 14.0, ui.visuals().text_color())).on_hover_text(crate::i18n::tr("Export the dashboard as a PDF report (A4)")).clicked() {
                            export = Some(Purpose::ExportDashboardPdf);
                        }
                        if ui.button(crate::theme::icon_job(P::ARROWS_CLOCKWISE, crate::i18n::tr("Refresh"), 14.0, ui.visuals().text_color())).clicked() {
                            refresh = true;
                        }
                    });
                });
                ui.add_space(8.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    // ---- indicatori
                    let kpis = s.kpis();
                    let cols = if ui.available_width() > 900.0 { 6 } else { 3 };
                    egui::Grid::new("dash_kpi").num_columns(cols).spacing([10.0, 10.0]).show(ui, |ui| {
                        let w = (ui.available_width() - 10.0 * (cols as f32 - 1.0)) / cols as f32;
                        for (i, (label, value, detail)) in kpis.iter().enumerate() {
                            egui::Frame::none().fill(ui.visuals().faint_bg_color).stroke(ui.visuals().widgets.noninteractive.bg_stroke).rounding(8.0).inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
                                ui.set_width((w - 26.0).max(60.0));
                                ui.label(RichText::new(crate::i18n::tr(label)).small().weak());
                                ui.label(RichText::new(value).size(26.0).strong().color(accent()));
                                ui.label(RichText::new(detail).small().weak());
                            });
                            if (i + 1) % cols == 0 {
                                ui.end_row();
                            }
                        }
                    });
                    ui.add_space(12.0);
                    let card = |ui: &mut egui::Ui, title: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
                        egui::Frame::none().fill(ui.visuals().faint_bg_color).stroke(ui.visuals().widgets.noninteractive.bg_stroke).rounding(8.0).inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
                            ui.set_width(ui.available_width().max(0.0));
                            ui.label(RichText::new(crate::i18n::tr(title).to_uppercase()).size(11.0).strong().color(ui.visuals().weak_text_color()));
                            ui.add_space(6.0);
                            add(ui);
                        });
                        ui.add_space(10.0);
                    };
                    // ---- clash-uri
                    card(ui, "Clash Detective", &mut |ui| {
                        if s.clash.is_empty() {
                            ui.label(RichText::new(crate::i18n::tr("No clash tests yet.")).weak());
                        }
                        egui::Grid::new("dash_clash").num_columns(5).striped(true).spacing([14.0, 6.0]).show(ui, |ui| {
                            for h in ["Test", "Type", "Results", "By status", "Open: Critical / Major / Minor"] {
                                ui.label(RichText::new(crate::i18n::tr(h)).strong());
                            }
                            ui.end_row();
                            for c in &s.clash {
                                ui.label(&c.name);
                                ui.label(RichText::new(&c.kind).weak());
                                ui.label(c.total.to_string());
                                let parts: Vec<(String, usize, [u8; 3])> = (0..5).map(|k| (STATUS[k].to_string(), c.by_status[k], STATUS_RGB[k])).collect();
                                stacked_bar(ui, &parts, 220.0);
                                ui.horizontal(|ui| {
                                    for k in 0..3 {
                                        let col = SEVERITY_RGB[k];
                                        ui.label(RichText::new(format!("{}", c.by_severity[k])).strong().color(Color32::from_rgb(col[0], col[1], col[2]))).on_hover_text(SEVERITY[k]);
                                    }
                                });
                                ui.end_row();
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            for k in 0..5 {
                                let c = STATUS_RGB[k];
                                ui.label(RichText::new("■").color(Color32::from_rgb(c[0], c[1], c[2])));
                                ui.label(RichText::new(crate::i18n::tr(STATUS[k])).small());
                            }
                            if ui.link(crate::i18n::tr("Open Clash Detective")).clicked() {
                                goto = Some(Panel::Clash);
                            }
                        });
                    });
                    // ---- issues
                    card(ui, "Viewpoints & issues", &mut |ui| {
                        let pal = [[230, 70, 60], [240, 150, 40], [70, 130, 230], [60, 175, 90], [150, 150, 150], [140, 90, 200]];
                        let parts: Vec<(String, usize, [u8; 3])> = s.issue_status.iter().enumerate().map(|(i, (n, c))| (n.clone(), *c, pal[i % pal.len()])).collect();
                        ui.label(format!("{} {} · {} {}", s.viewpoints, crate::i18n::tr("viewpoints"), s.issues_open(), crate::i18n::tr("open")));
                        stacked_bar(ui, &parts, ui.available_width().min(600.0));
                        ui.columns(3, |c| {
                            for (i, (title, list)) in [("Status", &s.issue_status), ("Priority", &s.issue_priority), ("Assigned to", &s.issue_assignee)].into_iter().enumerate() {
                                c[i].label(RichText::new(crate::i18n::tr(title)).strong());
                                for (n, k) in list.iter().take(8) {
                                    c[i].label(format!("{}  {k}", if n.is_empty() { "(unassigned)" } else { n }));
                                }
                            }
                        });
                        if ui.link(crate::i18n::tr("Open Viewpoints")).clicked() {
                            goto = Some(Panel::Viewpoints);
                        }
                    });
                    // ---- verificari + modele
                    card(ui, "Model Checker", &mut |ui| match s.check {
                        Some((r, c, f)) => {
                            let ok = c.saturating_sub(f);
                            ui.label(format!("{r} rules · {c} checks · {f} failed"));
                            stacked_bar(ui, &[("Passed".into(), ok, [60, 175, 90]), ("Failed".into(), f, [230, 70, 60])], ui.available_width().min(600.0));
                        }
                        None => {
                            ui.label(RichText::new(crate::i18n::tr("Not run in this session - Analyze › Checker.")).weak());
                        }
                    });
                    card(ui, "Models", &mut |ui| {
                        egui::Grid::new("dash_models").num_columns(5).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                            for h in ["Model", "Format", "Elements", "Triangles", "Points"] {
                                ui.label(RichText::new(crate::i18n::tr(h)).strong());
                            }
                            ui.end_row();
                            for m in &s.models {
                                ui.label(&m.name);
                                ui.label(RichText::new(&m.format).weak());
                                ui.label(crate::dashboard::thousands(m.elements));
                                ui.label(crate::dashboard::thousands(m.triangles));
                                ui.label(if m.points > 0 { crate::dashboard::thousands(m.points) } else { "-".into() });
                                ui.end_row();
                            }
                        });
                    });
                    ui.columns(2, |c| {
                        card(&mut c[0], "Categories", &mut |ui| {
                            let max = s.categories.first().map(|x| x.1).unwrap_or(1).max(1);
                            for (name, n) in &s.categories {
                                ui.horizontal(|ui| {
                                    ui.add_sized([160.0, 16.0], egui::Label::new(name.as_str()).truncate());
                                    stacked_bar(ui, &[(name.clone(), *n, [37, 99, 222]), (String::new(), max - n, [0, 0, 0])].into_iter().filter(|p| !p.0.is_empty() || p.1 == 0).collect::<Vec<_>>(), 140.0 * *n as f32 / max as f32 + 1.0);
                                    ui.label(RichText::new(n.to_string()).weak());
                                });
                            }
                        });
                        card(&mut c[1], "Levels", &mut |ui| {
                            if s.levels.is_empty() {
                                ui.label(RichText::new(crate::i18n::tr("No levels.")).weak());
                            }
                            for (n, z) in &s.levels {
                                ui.label(format!("{n}   {z:+.2} m"));
                            }
                        });
                    });
                    ui.label(RichText::new(format!("{} sets · {} coordination objects · {} material rules · {} 4D tasks · {} hidden items", s.sets, s.coord_objects, s.materials, s.tasks, s.hidden)).small().weak());
                });
            });
        self.dash_open = open;
        if refresh {
            self.dash = Some(self.dashboard_summary());
        }
        if let Some(p) = goto {
            self.open_panel(p);
        }
        if let Some(p) = export {
            self.dash = Some(self.dashboard_summary());
            self.request_path(p);
        }
    }
}

impl App {
    /// Exportul Dashboard: intai capturile (fisele de clash si viewpoint-urile cu markup-uri), apoi fisierul.
    pub(crate) fn dashboard_export(&mut self, pdf: bool, path: String) {
        let tests: Vec<(Vec<String>, Vec<bool>)> = self
            .clash_tests
            .iter()
            .map(|t| (t.results.iter().map(|r| r.group.clone()).collect(), t.results.iter().map(|r| matches!(r.status, ClashStatus::New | ClashStatus::Active | ClashStatus::Reviewed)).collect()))
            .collect();
        let queue = crate::dashboard::pick_images(&tests, 25, 60);
        let views: Vec<usize> = (0..self.viewpoints.len().min(60)).collect();
        if (queue.is_empty() && views.is_empty()) || self.loaded_models().next().is_none() {
            return self.dashboard_write(pdf, &path, Vec::new(), Vec::new());
        }
        let saved_bottom = self.settings.layout.bottom_open;
        if !queue.is_empty() {
            // modul Clash Detective vizibil pe durata capturilor de clash (elementele colorate, restul estompat)
            self.open_panel(Panel::Clash);
        }
        self.dash_job = Some(crate::dashboard::CaptureJob {
            pdf,
            path,
            queue,
            idx: 0,
            wait: 0,
            waiting: false,
            images: Vec::new(),
            saved_cam: self.cam,
            saved_bottom,
            saved_test: self.cur_test,
            views,
            view_images: Vec::new(),
            saved_section: self.section.clone(),
            saved_hidden: self.hidden.clone(),
            saved_selected: self.selected.clone(),
            saved_vp: self.active_vp,
        });
        self.toast("Dashboard: capturing the clash and viewpoint images…");
    }

    /// Un pas pe cadru: arata clash-ul / viewpoint-ul urmator, asteapta 3 cadre (culorile), cere captura.
    pub(super) fn dashboard_step(&mut self, ctx: &egui::Context) {
        let Some(j) = self.dash_job.as_mut() else { return };
        ctx.request_repaint();
        if j.waiting {
            return;
        }
        let total = j.queue.len() + j.views.len();
        if j.idx >= total {
            let j = self.dash_job.take().unwrap();
            self.cam = j.saved_cam;
            self.cam_anim = None;
            self.section = j.saved_section;
            self.hidden = j.saved_hidden;
            self.selected = j.saved_selected;
            self.active_vp = j.saved_vp;
            self.settings.layout.bottom_open = j.saved_bottom;
            self.cur_test = j.saved_test;
            self.clash_focus = None;
            self.colors_dirty = true;
            self.dashboard_write(j.pdf, &j.path, j.images, j.view_images);
            return;
        }
        if j.wait == 0 {
            j.wait = 3;
            if j.idx < j.queue.len() {
                let (ti, ri) = j.queue[j.idx];
                self.cur_test = ti;
                self.apply(Action::FocusClashes(ti, vec![ri]));
            } else {
                let vi = j.views[j.idx - j.queue.len()];
                // viewpoint-urile fara modul clash (zona de jos coborata), cu camera / sectiunea / ascunsele lor
                self.settings.layout.bottom_open = false;
                self.clash_focus = None;
                self.restore_viewpoint(vi);
            }
            if let Some((_, to, _)) = self.cam_anim.take() {
                self.cam = to;
            }
            return;
        }
        j.wait -= 1;
        if j.wait == 0 {
            j.waiting = true;
            self.pending_capture = Some((1280, 720));
        }
    }

    /// Captura ceruta de export (apelat din render_step); intoarce true daca a fost a lui.
    pub(crate) fn dashboard_capture_done(&mut self, r: &Result<crate::video::Frame, String>) -> bool {
        let Some(j) = self.dash_job.as_ref() else { return false };
        if !j.waiting {
            return false;
        }
        let view = j.idx.checked_sub(j.queue.len()).and_then(|k| j.views.get(k).copied());
        let reds = view.and_then(|vi| self.viewpoints.get(vi)).map(|v| v.redlines.clone()).unwrap_or_default();
        // punctul clash-ului, proiectat in imagine (cerc rosu + cruce), ca sa se vada exact unde e
        let mark = match (view, r) {
            (None, Ok(f)) => {
                let (ti, ri) = j.queue[j.idx];
                self.clash_tests.get(ti).and_then(|t| t.results.get(ri)).and_then(|cr| {
                    let vp = self.frame_params(f.w as f32 / f.h.max(1) as f32).view_proj;
                    let c = vp * Vec3::from(cr.point).extend(1.0);
                    (c.w > 1e-6).then(|| [(c.x / c.w * 0.5 + 0.5) * f.w as f32, (0.5 - c.y / c.w * 0.5) * f.h as f32])
                })
            }
            _ => None,
        };
        let Some(j) = self.dash_job.as_mut() else { return false };
        if let Ok(f) = r {
            let mut f = crate::video::Frame { w: f.w, h: f.h, rgba: f.rgba.clone() };
            if !reds.is_empty() {
                f.draw_redlines(&reds);
            }
            if let Some([x, y]) = mark {
                let rad = f.h as f32 * 0.045;
                let red = [230, 40, 40, 255];
                let n = 40;
                for k in 0..n {
                    let (a0, a1) = (k as f32 / n as f32 * std::f32::consts::TAU, (k + 1) as f32 / n as f32 * std::f32::consts::TAU);
                    f.draw_line([x + rad * a0.cos(), y + rad * a0.sin()], [x + rad * a1.cos(), y + rad * a1.sin()], 4.0, red);
                }
                for (dx, dy) in [(1.0, 0.0), (0.0, 1.0)] {
                    f.draw_line([x - dx * rad * 0.45, y - dy * rad * 0.45], [x + dx * rad * 0.45, y + dy * rad * 0.45], 2.5, red);
                }
            }
            let rgb = f.rgb();
            let mut jpg = Vec::new();
            if jpeg_encoder::Encoder::new(&mut jpg, 82).encode(&rgb, f.w as u16, f.h as u16, jpeg_encoder::ColorType::Rgb).is_ok() {
                match view {
                    Some(vi) => j.view_images.push((vi, (jpg, f.w, f.h))),
                    None => j.images.push((j.queue[j.idx], (jpg, f.w, f.h))),
                }
            }
        }
        j.idx += 1;
        j.waiting = false;
        j.wait = 0;
        true
    }

    fn dashboard_write(&mut self, pdf: bool, path: &str, images: Vec<((usize, usize), Jpeg)>, view_images: Vec<(usize, Jpeg)>) {
        let ext = if pdf { ".pdf" } else { ".docx" };
        let mut out = path.to_string();
        if !out.to_ascii_lowercase().ends_with(ext) {
            out.push_str(ext);
        }
        let mut s = self.dashboard_summary();
        let (n, nv) = (images.len(), view_images.len());
        for ((ti, ri), img) in images {
            if let Some(r) = s.clash.get_mut(ti).and_then(|c| c.results.get_mut(ri)) {
                r.image = Some(img);
            }
        }
        for (vi, img) in view_images {
            if let Some(v) = s.views.get_mut(vi) {
                v.image = Some(img);
            }
        }
        self.dash = Some(s.clone());
        let bytes = if pdf { crate::dashboard::pdf(&s) } else { crate::dashboard::docx(&s) };
        match std::fs::write(&out, bytes) {
            Ok(()) => {
                self.toast(format!("Dashboard exported: {} ({n} clash sheets, {nv} viewpoints with markups)", crate::file_name(&out)));
                crate::formats::open_with_system(std::path::Path::new(&out));
            }
            Err(e) => self.error(format!("Cannot write {out}: {e}")),
        }
    }
}
