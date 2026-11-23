//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    pub(super) fn clash_ui(&mut self, ui: &mut egui::Ui) {
        use crate::clash_io::{status_counts, test_state, TestState};
        if self.clash_tests.is_empty() {
            self.clash_tests.push(ClashTest::new("Test 1".into()));
        }
        if self.cur_test >= self.clash_tests.len() {
            self.cur_test = 0;
        }
        let ti = self.cur_test;
        // ---- antet: testul curent, ultima rulare, totalul (ca in Navisworks)
        {
            let t = &self.clash_tests[ti];
            let k = status_counts(t);
            let open = k[0] + k[1];
            ui.horizontal(|ui| {
                ui.label(RichText::new(&t.name).size(15.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("Clashes - Total: {}  (Open: {}  Closed: {})", t.results.len(), open, t.results.len() - open));
                    ui.add_space(12.0);
                    ui.label(format!("Last Run: {}", if t.last_run.is_empty() { "<None>" } else { &t.last_run }));
                });
            });
        }
        // ---- lista testelor: Name | Status | Clashes | New | Active | Reviewed | Approved | Resolved
        let mut pick: Option<usize> = None;
        let mut ctx_action: Option<(usize, u8)> = None;
        // redenumire cu clic dreapta > Rename: randul care se redenumeste, tinut intre cadre
        let rename_key = egui::Id::new("mb_clash_rename");
        let mut renaming: Option<usize> = ui.data(|d| d.get_temp(rename_key));
        let mut rename_to: Option<(usize, String)> = None;
        egui::Frame::group(ui.style()).inner_margin(2.0).show(ui, |ui| {
            let h = (24.0 + 20.0 * self.clash_tests.len() as f32).clamp(70.0, 190.0);
            super::table::table(ui)
                .id_salt("clash_tests_tbl")
                .max_scroll_height(h)
                .min_scrolled_height(h.min(70.0))
                .column(Column::remainder().at_least(150.0).clip(true))
                .column(super::table::num(52.0))
                .column(super::table::num(58.0))
                .columns(super::table::num(76.0), 5)
                .header(20.0, |mut hd| {
                    hd.col(|ui| {
                        ui.strong("Name");
                    });
                    hd.col(|ui| {
                        ui.strong("Status");
                    });
                    hd.col(|ui| {
                        ui.strong("Clashes");
                    });
                    for st in ClashStatus::ALL {
                        hd.col(|ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                let (r, _) = ui.allocate_exact_size(EV::new(4.0, 14.0), egui::Sense::hover());
                                ui.painter().rect_filled(r, 0.0, c32(st.color()));
                                ui.add(egui::Label::new(RichText::new(st.label()).strong()).extend());
                            });
                        });
                    }
                })
                .body(|body| {
                    let n = self.clash_tests.len();
                    body.rows(20.0, n, |mut row| {
                        let i = row.index();
                        let t = &self.clash_tests[i];
                        let k = status_counts(t);
                        let state = test_state(t);
                        let running = self.clash_job.as_ref().map_or(false, |j| j.test == i);
                        row.set_selected(i == ti);
                        row.col(|ui| {
                            if let Some(new) = super::table::name_cell(ui, egui::Id::new("mb_clash_tests"), i, &mut renaming, &t.name) {
                                rename_to = Some((i, new));
                            }
                        });
                        row.col(|ui| {
                            let (txt, col) = if running {
                                ("Running", accent())
                            } else {
                                match state {
                                    TestState::New => ("New", ui.visuals().text_color()),
                                    TestState::Done => ("Done", Color32::from_rgb(70, 170, 90)),
                                    TestState::Old => ("Old", Color32::from_rgb(230, 150, 40)),
                                }
                            };
                            ui.label(RichText::new(txt).color(col)).on_hover_text(match state {
                                TestState::New => "Not run yet",
                                TestState::Done => "Up to date",
                                TestState::Old => "The test changed after the last run (selections, type, tolerance or rules) - Update",
                            });
                        });
                        row.col(|ui| {
                            ui.strong(t.results.len().to_string());
                        });
                        for v in k {
                            row.col(|ui| {
                                ui.label(v.to_string());
                            });
                        }
                        let r = row.response();
                        if r.clicked() {
                            pick = Some(i);
                        }
                        r.context_menu(|ui| {
                            for (a, l) in [(0u8, "▶ Run"), (1, "Reset (remove results)"), (2, "Compact (remove resolved)"), (3, "Duplicate"), (4, "🗑 Delete")] {
                                if ui.button(l).clicked() {
                                    ctx_action = Some((i, a));
                                    ui.close_menu();
                                }
                            }
                            ui.separator();
                            super::table::rename_item(ui, &mut renaming, i);
                        });
                    });
                });
        });
        if let Some(i) = pick {
            if i != self.cur_test {
                self.cur_test = i;
                self.cur_result = None;
                self.sel_results.clear();
                self.clash_focus = None;
                self.colors_dirty = true;
            }
        }
        // numele se aplica dupa randul de tabel (in interior s-a citit doar `&self`)
        if let Some((i, name)) = rename_to {
            if let Some(t) = self.clash_tests.get_mut(i) {
                t.name = name;
            }
        }
        // starea redenumirii trebuie sa treaca intre cadre: meniul contextual se
        // deschide intr-un strat ulterior, deci celula o citeste abia urmatorul cadru
        ui.data_mut(|d| match renaming {
            Some(r) => {
                d.insert_temp(rename_key, r);
            }
            None => {
                d.remove::<usize>(rename_key);
            }
        });
        if let Some((i, a)) = ctx_action {
            match a {
                0 => {
                    self.cur_test = i;
                    self.run_clash(i);
                }
                1 => {
                    let t = &mut self.clash_tests[i];
                    t.results.clear();
                    t.last_run.clear();
                    t.run_key = 0;
                    self.colors_dirty = true;
                }
                2 => self.clash_tests[i].results.retain(|r| r.status != ClashStatus::Resolved),
                3 => {
                    let mut t = self.clash_tests[i].clone();
                    t.name = format!("{} (copy)", t.name);
                    t.results.clear();
                    t.last_run.clear();
                    t.run_key = 0;
                    self.clash_tests.insert(i + 1, t);
                    self.cur_test = i + 1;
                }
                _ => {
                    self.clash_tests.remove(i);
                    if self.clash_tests.is_empty() {
                        self.clash_tests.push(ClashTest::new("Test 1".into()));
                    }
                    self.cur_test = self.cur_test.min(self.clash_tests.len() - 1);
                    self.cur_result = None;
                    self.sel_results.clear();
                    self.clash_focus = None;
                    self.colors_dirty = true;
                }
            }
        }
        // ---- butoanele de sub lista (Navisworks: Add Test, Reset All, Compact All, Delete All, Update All, Import / Export)
        let idle = self.clash_job.is_none();
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(idle, egui::Button::new("➕ Add Test")).clicked() {
                let mut n = self.clash_tests.len() + 1;
                while self.clash_tests.iter().any(|t| t.name == format!("Test {n}")) {
                    n += 1;
                }
                self.clash_tests.push(ClashTest::new(format!("Test {n}")));
                self.cur_test = self.clash_tests.len() - 1;
                self.cur_result = None;
                self.sel_results.clear();
                self.clash_subtab = 2;
            }
            ui.menu_button("📋 Templates ⏷", |ui| {
                ui.label(RichText::new("Add a ready-made test").strong());
                for (name, about, t) in ClashTest::templates() {
                    if ui.button(name).on_hover_text(about).clicked() {
                        self.clash_tests.push(t);
                        self.cur_test = self.clash_tests.len() - 1;
                        self.cur_result = None;
                        self.sel_results.clear();
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button("All of the above").clicked() {
                    self.clash_tests.extend(ClashTest::templates().into_iter().map(|t| t.2));
                    self.cur_test = self.clash_tests.len() - 1;
                    self.cur_result = None;
                    self.sel_results.clear();
                    ui.close_menu();
                }
            })
            .response
            .on_hover_text("Test templates: Structure vs MEP, clearances, duplicates - disciplines are recognised automatically");
            ui.separator();
            let has_res = self.clash_tests.iter().any(|t| !t.results.is_empty());
            if ui.add_enabled(idle && has_res, egui::Button::new("Reset All")).on_hover_text("Remove the results of every test (the tests stay, status New)").clicked() {
                self.clash_reset_all();
            }
            if ui.add_enabled(idle && self.clash_tests.iter().any(|t| t.results.iter().any(|r| r.status == ClashStatus::Resolved)), egui::Button::new("Compact All")).on_hover_text("Remove the Resolved clashes from every test").clicked() {
                self.clash_compact_all();
            }
            let del = ui.add_enabled(idle, egui::Button::new("Delete All")).on_hover_text("Delete every clash test");
            let pid = ui.make_persistent_id("del_all_confirm");
            if del.clicked() {
                ui.memory_mut(|m| m.toggle_popup(pid));
            }
            egui::popup_below_widget(ui, pid, &del, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(200.0);
                ui.label(format!("Delete all {} clash tests?", self.clash_tests.len()));
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Delete All").color(Color32::from_rgb(230, 80, 70))).clicked() {
                        self.clash_delete_all();
                        ui.memory_mut(|m| m.close_popup());
                    }
                    if ui.button("Cancel").clicked() {
                        ui.memory_mut(|m| m.close_popup());
                    }
                });
            });
            ui.separator();
            if ui
                .add_enabled(idle && self.loaded_models().next().is_some(), egui::Button::new(RichText::new("⟳ Update All").strong().color(Color32::WHITE)).fill(accent()))
                .on_hover_text("Run every test again. Clashes found again stay Active (or keep Reviewed / Approved), new ones are New, the ones gone are Resolved.\nA summary shows what changed.")
                .clicked()
            {
                self.clash_update_all();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("⇅ Import / Export ⏷", |ui| {
                    if ui.button("📥 Import Clash Tests…").on_hover_text("Navisworks XML (Clash Detective > Import / Export > Export Clash Tests), CSV or Excel").clicked() {
                        self.request_path(Purpose::ImportClashTests);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("📤 Export Clash Tests (Navisworks XML)…").on_hover_text("In Navisworks: Clash Detective > Import / Export > Import Clash Tests. The selection sets used are exported too.").clicked() {
                        self.request_path(Purpose::ExportNavisClash);
                        ui.close_menu();
                    }
                    if ui.button("📤 Export Clash Tests (CSV)…").on_hover_text("One row per test - edit in Excel and import back").clicked() {
                        self.request_path(Purpose::ExportClashTestsCsv);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("▦ Clash Matrix…").clicked() {
                        self.show_clash_matrix = true;
                        ui.close_menu();
                    }
                });
                if ui.selectable_label(self.show_clash_matrix, "▦ Matrix").on_hover_text("Clash matrix: Selection A × Selection B, the results of each test").clicked() {
                    self.show_clash_matrix = !self.show_clash_matrix;
                }
            });
        });
        if let Some(job) = &self.clash_job {
            let left = self.clash_queue.len();
            ui.horizontal(|ui| {
                let name = self.clash_tests.get(job.test).map(|t| t.name.clone()).unwrap_or_default();
                ui.label(format!("Running {name}{}", if left > 0 { format!("  ({left} more)") } else { String::new() }));
                ui.add(egui::ProgressBar::new(job.prog.frac()).desired_width(220.0).show_percentage());
                if ui.button("Cancel").clicked() {
                    job.prog.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            });
        }
        // ---- tab-urile: Rules | Select | Results | Report
        let ti = self.cur_test.min(self.clash_tests.len() - 1);
        ui.horizontal(|ui| {
            let nres = self.clash_tests[ti].results.len();
            let nrules = self.clash_tests[ti].rules.count(self.clash_tests[ti].ignore_same_file);
            ui.selectable_value(&mut self.clash_subtab, 1, format!("Rules ({nrules})"));
            ui.selectable_value(&mut self.clash_subtab, 2, "Select");
            ui.selectable_value(&mut self.clash_subtab, 0, format!("Results ({nres})"));
            ui.selectable_value(&mut self.clash_subtab, 3, "Report");
        });
        ui.separator();
        match self.clash_subtab {
            1 => self.clash_rules_ui(ui, ti),
            2 => self.clash_select_ui(ui, ti),
            3 => self.clash_report_ui(ui, ti),
            _ => self.clash_results(ui, ti),
        }
    }

    /// Matricea de clash pe teste: Selection A pe randuri, Selection B pe coloane; in fiecare celula testele
    /// perechii cu starile lor. Dedesubt: matricea de seturi din care se creeaza testele.
    pub(super) fn clash_matrix_window(&mut self, ctx: &egui::Context) {
        use crate::clash_io::{status_counts, test_matrix};
        let mut open = true;
        let m = test_matrix(&self.clash_tests);
        let vid = egui::Id::new("cm_view");
        let mut view: usize = ctx.data(|d| d.get_temp(vid)).unwrap_or(0);
        let mut goto: Option<usize> = None;
        let mut export = false;
        let mut create: Option<(Vec<(Selector, Selector)>, ClashType, f32)> = None;
        egui::Window::new("▦ Clash Matrix").open(&mut open).default_width(720.0).default_height(520.0).show(ctx, |ui| {
            const VIEWS: [&str; 8] = ["Open (New + Active)", "Total", "New", "Active", "Reviewed", "Approved", "Resolved", "Tests"];
            ui.horizontal(|ui| {
                ui.label("Show");
                egui::ComboBox::from_id_salt("cm_v").selected_text(VIEWS[view]).show_ui(ui, |ui| {
                    for (i, v) in VIEWS.iter().enumerate() {
                        ui.selectable_value(&mut view, i, *v);
                    }
                });
                ui.label(RichText::new("Rows = Selection A, columns = Selection B. Click a cell to open its test.").weak().small());
            });
            ui.add_space(4.0);
            if m.cells.is_empty() {
                ui.label(RichText::new("No clash tests yet.").weak());
            } else {
                let maxv = m.cells.keys().filter_map(|(r, c)| m.counts(&self.clash_tests, *r, *c)).map(|k| k[0] + k[1]).max().unwrap_or(1).max(1) as f32;
                egui::ScrollArea::both().id_salt("cm_scroll").max_height(300.0).show(ui, |ui| {
                    egui::Grid::new("cm_grid").spacing([4.0, 4.0]).show(ui, |ui| {
                        ui.label(RichText::new("A \\ B").weak().small());
                        for c in &m.cols {
                            ui.label(RichText::new(c).strong().small());
                        }
                        ui.label(RichText::new("Total").strong().small());
                        ui.end_row();
                        for r in 0..m.rows.len() {
                            ui.label(RichText::new(&m.rows[r]).strong().small());
                            let mut row_tot = 0;
                            for c in 0..m.cols.len() {
                                let Some(ix) = m.cells.get(&(r, c)) else {
                                    egui::Frame::none().fill(Color32::from_white_alpha(6)).rounding(4.0).inner_margin(egui::Margin::symmetric(8.0, 4.0)).show(ui, |ui| {
                                        ui.set_min_width(90.0);
                                        ui.label(RichText::new("—").weak());
                                    });
                                    continue;
                                };
                                let k = m.counts(&self.clash_tests, r, c).unwrap_or([0; 5]);
                                let open_n = k[0] + k[1];
                                let total: usize = k.iter().sum();
                                let v = match view {
                                    0 => open_n,
                                    1 => total,
                                    2..=6 => k[view - 2],
                                    _ => open_n,
                                };
                                row_tot += v;
                                let run = ix.iter().all(|&i| !self.clash_tests[i].last_run.is_empty());
                                let t = open_n as f32 / maxv;
                                let fill = if !run {
                                    Color32::from_rgba_unmultiplied(120, 120, 140, 50)
                                } else if open_n == 0 {
                                    Color32::from_rgba_unmultiplied(70, 180, 100, 90)
                                } else {
                                    Color32::from_rgba_unmultiplied(230, (170.0 - 120.0 * t) as u8, 60, (90.0 + 110.0 * t) as u8)
                                };
                                let resp = egui::Frame::none()
                                    .fill(fill)
                                    .rounding(4.0)
                                    .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                                    .show(ui, |ui| {
                                        ui.set_min_width(90.0);
                                        ui.vertical(|ui| {
                                            let big = if !run { "not run".to_string() } else if view == 7 { format!("{open_n} / {total}") } else { v.to_string() };
                                            ui.label(RichText::new(big).strong());
                                            for &i in ix {
                                                ui.label(RichText::new(&self.clash_tests[i].name).small());
                                            }
                                        });
                                    })
                                    .response
                                    .interact(Sense::click());
                                let tip: Vec<String> = ix
                                    .iter()
                                    .map(|&i| {
                                        let t = &self.clash_tests[i];
                                        let q = status_counts(t);
                                        format!("{} ({}, {} mm)\nNew {}  Active {}  Reviewed {}  Approved {}  Resolved {}  -  total {}\nLast run: {}", t.name, t.ty.label(), t.tol_mm, q[0], q[1], q[2], q[3], q[4], t.results.len(), if t.last_run.is_empty() { "never" } else { &t.last_run })
                                    })
                                    .collect();
                                let resp = resp.on_hover_text(format!("{} × {}\n\n{}", m.rows[r], m.cols[c], tip.join("\n\n")));
                                if resp.clicked() {
                                    goto = ix.first().copied();
                                }
                            }
                            ui.label(RichText::new(row_tot.to_string()).strong());
                            ui.end_row();
                        }
                    });
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("📊 Export Excel").on_hover_text("The matrix with the tests of each cell, one sheet per status, and the list of tests").clicked() {
                        export = true;
                    }
                    if ui.add_enabled(self.clash_job.is_none() && self.loaded_models().next().is_some(), egui::Button::new("⟳ Update All")).clicked() {
                        self.clash_update_all();
                    }
                });
            }
            ui.separator();
            // ---- matricea de seturi: bifezi perechile, se creeaza testele
            egui::CollapsingHeader::new(RichText::new("Create tests from a sets matrix").strong()).default_open(m.cells.is_empty() || self.clash_tests.iter().all(|t| t.results.is_empty() && t.last_run.is_empty())).show(ui, |ui| {
                let mut cands: Vec<Selector> = self.sets.iter().map(|s| Selector { kind: SelKind::Set, value: s.name.clone() }).collect();
                for d in ["Structure", "Architecture", "MEP - HVAC", "MEP - Plumbing", "MEP - Electrical"] {
                    cands.push(Selector { kind: SelKind::Discipline, value: d.into() });
                }
                for (_, md) in self.loaded_models() {
                    cands.push(Selector { kind: SelKind::Model, value: md.name.clone() });
                }
                let pid = egui::Id::new("cm_pick");
                let mut picked: HashSet<(String, String)> = ui.data(|d| d.get_temp(pid)).unwrap_or_default();
                let tyid = egui::Id::new("cm_ty");
                let (mut ty, mut tol): (ClashType, f32) = ui.data(|d| d.get_temp(tyid)).unwrap_or((ClashType::Hard, 10.0));
                ui.label(RichText::new("Tick the pairs that must be tested (row = Selection A, column = Selection B). Pairs that already have a test are marked ✔.").weak().small());
                egui::ScrollArea::both().id_salt("cm_sets").max_height(240.0).show(ui, |ui| {
                    egui::Grid::new("cm_sets_grid").spacing([6.0, 2.0]).striped(true).show(ui, |ui| {
                        ui.label("");
                        for b in &cands {
                            ui.label(RichText::new(b.label()).small()).on_hover_text(b.label());
                        }
                        ui.end_row();
                        for a in &cands {
                            ui.label(RichText::new(a.label()).small());
                            for b in &cands {
                                let exists = self.clash_tests.iter().any(|t| (&t.a, &t.b) == (a, b));
                                if exists {
                                    ui.label(RichText::new("✔").color(Color32::from_rgb(70, 170, 90)));
                                    continue;
                                }
                                let key = (a.label(), b.label());
                                let mut on = picked.contains(&key);
                                if ui.checkbox(&mut on, "").changed() {
                                    if on {
                                        picked.insert(key);
                                    } else {
                                        picked.remove(&key);
                                    }
                                }
                            }
                            ui.end_row();
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Type");
                    egui::ComboBox::from_id_salt("cm_type").selected_text(ty.label()).show_ui(ui, |ui| {
                        for t in [ClashType::Hard, ClashType::Clearance, ClashType::Duplicates] {
                            ui.selectable_value(&mut ty, t, t.label());
                        }
                    });
                    ui.label("Tolerance");
                    ui.add(egui::DragValue::new(&mut tol).range(0.0..=5000.0).speed(1.0).suffix(" mm"));
                    if ui.add_enabled(!picked.is_empty(), egui::Button::new(format!("➕ Create {} tests", picked.len()))).clicked() {
                        let pairs = picked.iter().map(|(a, b)| (crate::clash_io::selector_from_label(a), crate::clash_io::selector_from_label(b))).collect();
                        create = Some((pairs, ty, tol));
                        picked.clear();
                    }
                });
                ui.data_mut(|d| {
                    d.insert_temp(pid, picked);
                    d.insert_temp(tyid, (ty, tol));
                });
            });
        });
        ctx.data_mut(|d| d.insert_temp(vid, view));
        self.show_clash_matrix = open;
        if let Some(i) = goto {
            self.cur_test = i;
            self.cur_result = None;
            self.sel_results.clear();
            self.clash_subtab = 0;
            self.open_panel(crate::layout::Panel::Clash);
        }
        if let Some((pairs, ty, tol)) = create {
            let n = self.create_tests_from_matrix(&pairs, ty, tol);
            self.toast(format!("{n} clash tests created - press Update All to run them."));
        }
        if export {
            self.request_path(Purpose::ExportClashMatrix);
        }
    }

    /// Sumarul dupa Update All: pe fiecare test, starile acum si diferenta fata de rularea anterioara.
    pub(super) fn update_summary_window(&mut self, ctx: &egui::Context) {
        let Some(sum) = self.update_summary.clone() else { return };
        let mut open = true;
        let mut close = false;
        let mut goto = None;
        egui::Window::new("⟳ Update All - what changed").open(&mut open).collapsible(false).default_width(640.0).anchor(Align2::CENTER_CENTER, EV::ZERO).show(ctx, |ui| {
            let (mut new_n, mut act, mut res) = (0usize, 0usize, 0usize);
            for (_, _, a) in &sum {
                new_n += a[0];
                act += a[1];
                res += a[4];
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{new_n} new")).strong().color(c32(ClashStatus::New.color())));
                ui.label("·");
                ui.label(RichText::new(format!("{act} still active")).strong().color(c32(ClashStatus::Active.color())));
                ui.label("·");
                ui.label(RichText::new(format!("{res} resolved")).strong().color(c32(ClashStatus::Resolved.color())));
                ui.label(RichText::new(format!("in {} tests", sum.len())).weak());
            });
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                egui::Grid::new("upd_sum").striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                    ui.strong("Test");
                    for s in ClashStatus::ALL {
                        ui.label(RichText::new(s.label()).strong().color(c32(s.color())));
                    }
                    ui.strong("Total");
                    ui.end_row();
                    for (i, (name, b, a)) in sum.iter().enumerate() {
                        if ui.link(name).on_hover_text("Open this test").clicked() {
                            goto = Some(i);
                        }
                        for j in 0..5 {
                            let d = a[j] as i64 - b[j] as i64;
                            let txt = if d == 0 { a[j].to_string() } else { format!("{} ({:+})", a[j], d) };
                            ui.label(if d != 0 { RichText::new(txt).strong() } else { RichText::new(txt) });
                        }
                        ui.label(a.iter().sum::<usize>().to_string());
                        ui.end_row();
                    }
                });
            });
            ui.add_space(4.0);
            ui.label(RichText::new("New = found for the first time · Active = found again · Resolved = no longer found. Reviewed / Approved keep their status.").weak().small());
            if ui.button("Close").clicked() {
                close = true;
            }
        });
        if let Some(i) = goto {
            if i < self.clash_tests.len() {
                self.cur_test = i;
                self.cur_result = None;
                self.sel_results.clear();
                self.clash_subtab = 0;
            }
        }
        if !open || close {
            self.update_summary = None;
        }
    }

    /// Tab-ul Select: Selection A / Selection B, tipul testului, toleranta, Run Test.
    pub(super) fn clash_select_ui(&mut self, ui: &mut egui::Ui, ti: usize) {
        let mut test = self.clash_tests[ti].clone();
        let mut run = false;
        egui::ScrollArea::vertical().id_salt("csel").auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut test.name).desired_width(220.0));
            });
            ui.add_space(4.0);
            ui.columns(2, |c| {
                for (k, (lbl, sel, id)) in [("Selection A", &mut test.a, "selA"), ("Selection B", &mut test.b, "selB")].into_iter().enumerate() {
                    let ui = &mut c[k];
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.strong(lbl);
                        self.selector_combo(ui, id, sel);
                        // numarul de elemente: calculat o data pe selectie (Discipline / All parcurg tot modelul)
                        // pentru „Current selection" includem dimensiunea selecției și o amprentă
                        // a ID-urilor, ca numărul afișat să se actualizeze când selectezi altceva.
                        let sel_sig: Vec<ElemId> = self.selected.iter().copied().take(8).collect();
                        let key = egui::Id::new(("sel_count", sel.label(), self.loaded_models().map(|(_, m)| m.elements.len()).sum::<usize>(), self.sets.len(), self.selected.len(), sel_sig));
                        let n = match ui.data(|d| d.get_temp::<usize>(key)) {
                            Some(n) => n,
                            None => {
                                let n = self.resolve_selector(sel).len();
                                ui.data_mut(|d| d.insert_temp(key, n));
                                n
                            }
                        };
                        ui.label(RichText::new(format!("{n} items")).weak().small());
                    });
                }
            });
            ui.add_space(6.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.strong("Settings");
                egui::Grid::new("csettings").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Type");
                    egui::ComboBox::from_id_salt("ctype").selected_text(test.ty.label()).show_ui(ui, |ui| {
                        for t in [ClashType::Hard, ClashType::Clearance, ClashType::Duplicates] {
                            ui.selectable_value(&mut test.ty, t, t.label());
                        }
                    });
                    ui.end_row();
                    ui.label("Tolerance");
                    ui.add(egui::DragValue::new(&mut test.tol_mm).range(0.0..=5000.0).speed(1.0).suffix(" mm"));
                    ui.end_row();
                    if test.ty == ClashType::Hard {
                        ui.label("Severity").on_hover_text("By penetration depth: under the first value = Minor, between = Major, over the second = Critical");
                        ui.horizontal(|ui| {
                            ui.label("Major from");
                            ui.add(egui::DragValue::new(&mut test.sev_mm[0]).range(0.0..=1000.0).speed(1.0).suffix(" mm"));
                            ui.label("Critical from");
                            ui.add(egui::DragValue::new(&mut test.sev_mm[1]).range(0.0..=5000.0).speed(1.0).suffix(" mm"));
                        });
                        test.sev_mm[1] = test.sev_mm[1].max(test.sev_mm[0]);
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
                if ui.add_enabled(self.clash_job.is_none() && self.loaded_models().next().is_some(), egui::Button::new(RichText::new("▶ Run Test").strong().color(Color32::WHITE)).fill(accent())).clicked() {
                    run = true;
                }
            });
        });
        let t = &mut self.clash_tests[ti];
        t.name = test.name;
        t.a = test.a;
        t.b = test.b;
        t.ty = test.ty;
        t.tol_mm = test.tol_mm;
        t.sev_mm = test.sev_mm;
        if run {
            self.run_clash(ti);
            self.clash_subtab = 0;
        }
    }

    /// Tab-ul Report: rapoartele si exporturile testului curent / tuturor testelor.
    pub(super) fn clash_report_ui(&mut self, ui: &mut egui::Ui, ti: usize) {
        let has = !self.clash_tests[ti].results.is_empty();
        egui::Grid::new("creport").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.strong("Current test");
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(has, egui::Button::new("HTML report")).clicked() {
                    self.request_path(Purpose::ExportClashHtml);
                }
                if ui.add_enabled(has, egui::Button::new("CSV report")).clicked() {
                    self.request_path(Purpose::ExportClashCsv);
                }
                if ui.add_enabled(has, egui::Button::new("📤 BCF")).on_hover_text("Open clashes as BCF topics (camera + the two elements) - Revit, ACC, Solibri, BIMcollab, Navisworks").clicked() {
                    self.request_path(Purpose::ExportClashBcf);
                }
                if ui.add_enabled(has, egui::Button::new("🧭 Navisworks viewpoints")).on_hover_text("Open clashes as Navisworks viewpoints with a section box around each clash (Saved Viewpoints > Import Viewpoints)").clicked() {
                    self.request_path(Purpose::ExportClashNavis);
                }
            });
            ui.end_row();
            ui.strong("All tests");
            ui.horizontal_wrapped(|ui| {
                if ui.button("▦ Clash matrix (Excel)").on_hover_text("Selection A × Selection B with the results of each test, one sheet per status").clicked() {
                    self.request_path(Purpose::ExportClashMatrix);
                }
                if ui.button("Tests (Navisworks XML)").clicked() {
                    self.request_path(Purpose::ExportNavisClash);
                }
                if ui.button("Tests (CSV)").clicked() {
                    self.request_path(Purpose::ExportClashTestsCsv);
                }
            });
            ui.end_row();
        });
    }

    pub(super) fn chips(ui: &mut egui::Ui, salt: &str, list: &mut Vec<String>, options: &[String], add_text: &str) {
        let mut del = None;
        ui.horizontal_wrapped(|ui| {
            for (i, v) in list.iter().enumerate() {
                if ui.add(egui::Button::new(format!("{v}  ✖")).small()).on_hover_text("Remove").clicked() {
                    del = Some(i);
                }
            }
            let mut pick: Option<String> = None;
            egui::ComboBox::from_id_salt((salt, "add")).width(150.0).selected_text(RichText::new(add_text).weak()).show_ui(ui, |ui| {
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    if options.is_empty() {
                        ui.label(RichText::new("(none available)").weak());
                    }
                    for o in options {
                        if !list.contains(o) && ui.selectable_label(false, o).clicked() {
                            pick = Some(o.clone());
                        }
                    }
                });
            });
            if let Some(p) = pick {
                list.push(p);
            }
        });
        if let Some(i) = del {
            list.remove(i);
        }
    }

    pub(super) fn clash_rules_ui(&mut self, ui: &mut egui::Ui, ti: usize) {
        let mut rules = self.clash_tests[ti].rules.clone();
        let mut same_file = self.clash_tests[ti].ignore_same_file;
        let set_names: Vec<String> = self.sets.iter().map(|s| s.name.clone()).collect();
        let props = self.cache_props.clone();
        let idx = std::mem::take(&mut self.prop_index);
        egui::ScrollArea::vertical().id_salt("crules").auto_shrink([false, false]).show(ui, |ui| {
            ui.columns(2, |c| {
                let ui = &mut c[0];
                ui.label(RichText::new("Ignore clashes between items…").strong());
                ui.checkbox(&mut same_file, "in the same file (model)");
                ui.checkbox(&mut rules.same_level, "on the same level");
                ui.checkbox(&mut rules.same_type, "of the same IFC type / category");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut rules.normal_joints, "that are normal connections").on_hover_text("Wall-slab, wall-wall, beam-column, beam-slab, column-footing, door-wall, pipe / duct - fitting, pipe / duct - support or hanger, insulation - pipe / duct, terminal - duct... are joined by design: they count only when they go deeper than the limit");
                    ui.add_enabled(rules.normal_joints, egui::DragValue::new(&mut rules.joint_mm).range(0.0..=500.0).speed(1.0).prefix("up to ").suffix(" mm")).on_hover_text("Maximum penetration of a normal connection");
                });
                ui.checkbox(&mut rules.auto_assign, "Assign each new clash to the discipline that moves").on_hover_text("Structure > Architecture > HVAC > Plumbing > Electrical: the lower one is responsible (the Assigned field is filled only when empty)");
                ui.add_space(4.0);
                ui.label("with the same value of property (e.g. System):");
                Self::chips(ui, "sameprop", &mut rules.same_props, &props, "➕ add property");
                ui.add_space(4.0);
                ui.label("that are both in the same selection set:");
                Self::chips(ui, "sameset", &mut rules.same_sets, &set_names, "➕ add set");

                let ui = &mut c[1];
                ui.label(RichText::new("Exclude items from this test (they never clash)").strong());
                ui.label("Items in these selection / search sets:");
                Self::chips(ui, "exset", &mut rules.exclude_sets, &set_names, "➕ add set");
                ui.add_space(4.0);
                ui.label("Items matching any of these rules:");
                Self::rule_rows(ui, "exrule", &mut rules.exclude_rules, &idx, false);
                ui.horizontal(|ui| {
                    if ui.button("➕ Exclude rule").clicked() {
                        rules.exclude_rules.push(SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "", false));
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Examples: IFC Type = IfcSpace · Name contains Insulation · Phase = Demolition").weak().small());
            });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Rules apply when the test runs. Existing results that a rule now excludes are removed.").weak());
                if self.clash_job.is_none() && ui.button("▶ Re-run test with these rules").clicked() {
                    self.clash_tests[ti].rules = rules.clone();
                    self.clash_tests[ti].ignore_same_file = same_file;
                    self.run_clash(ti);
                    self.clash_subtab = 0;
                }
            });
        });
        self.prop_index = idx;
        self.clash_tests[ti].rules = rules;
        self.clash_tests[ti].ignore_same_file = same_file;
    }

    pub(super) fn clash_results(&mut self, ui: &mut egui::Ui, ti: usize) {
        // ---- display options
        ui.horizontal_wrapped(|ui| {
            ui.label("Display:");
            let before = (self.clash_others, self.clash_highlight_all);
            egui::ComboBox::from_id_salt("others").width(190.0).selected_text(self.clash_others.label()).show_ui(ui, |ui| {
                for o in [Others::Dim, Others::Hide, Others::Normal] {
                    ui.selectable_value(&mut self.clash_others, o, o.label());
                }
            });
            ui.checkbox(&mut self.clash_highlight_all, "Highlight all clashes").on_hover_text("Colour the items of every open clash (A light red, B light green)");
            ui.checkbox(&mut self.clash_auto_zoom, "Auto-zoom");
            let mut prefs = self.settings.clash_ui.clone();
            ui.checkbox(&mut prefs.animate, "Animate").on_hover_text("The camera flies smoothly to each clash (Animate transitions)");
            ui.checkbox(&mut prefs.items_panel, "Items").on_hover_text("Show Item 1 / Item 2 with their properties under the results");
            ui.menu_button("🎨 Colours ⏷", |ui| {
                ui.label(RichText::new("Highlight colours of the current clash").small().weak());
                for (lbl, col) in [("Item 1 (A)", &mut prefs.col_a), ("Item 2 (B)", &mut prefs.col_b)] {
                    ui.horizontal(|ui| {
                        let mut c = Color32::from_rgb(col[0], col[1], col[2]);
                        if ui.color_edit_button_srgba(&mut c).changed() {
                            *col = [c.r(), c.g(), c.b()];
                        }
                        ui.label(lbl);
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Dim other: opacity");
                    let mut pct = (prefs.dim_alpha as f32 / 255.0 * 100.0).round() as i32;
                    if ui.add(egui::Slider::new(&mut pct, 3..=80).suffix("%")).on_hover_text("How visible the other elements stay when Display = Dim other (lower = more transparent)").changed() {
                        prefs.dim_alpha = (pct as f32 / 100.0 * 255.0).round() as u8;
                    }
                });
                if ui.button("↺ Default colours").clicked() {
                    prefs.col_a = crate::theme::clash_col_a();
                    prefs.col_b = crate::theme::clash_col_b();
                    prefs.dim_alpha = crate::theme::dim_alpha_default();
                }
            });
            if prefs != self.settings.clash_ui {
                let recolor = (prefs.col_a, prefs.col_b, prefs.dim_alpha) != (self.settings.clash_ui.col_a, self.settings.clash_ui.col_b, self.settings.clash_ui.dim_alpha);
                self.settings.clash_ui = prefs;
                self.settings.save();
                if recolor {
                    self.colors_dirty = true;
                }
            }
            if before != (self.clash_others, self.clash_highlight_all) {
                self.colors_dirty = true;
            }
            if ui.button("Show all clashes").on_hover_text("Select every open clash in the list and show only them").clicked() {
                let idx: Vec<usize> = self.clash_tests[ti].results.iter().enumerate().filter(|(_, r)| r.status != ClashStatus::Resolved && self.clash_filter.map_or(true, |f| r.status == f)).map(|(i, _)| i).collect();
                self.sel_results = idx.iter().copied().collect();
                self.clash_others = Others::Hide;
                self.apply(Action::FocusClashes(ti, idx));
            }
            if ui.button("⇄ SwitchBack").on_hover_text("Send this clash back to Revit / Navisworks: camera + the two elements, as a ManageBIM issue viewpoint").clicked() {
                self.switchback();
            }
        });
        // ---- status / grouping / export
        ui.horizontal_wrapped(|ui| {
            for s in ClashStatus::ALL {
                let n = self.clash_tests[ti].count(s);
                ui.label(RichText::new(format!("{} {}", s.label(), n)).color(c32(s.color())));
            }
            ui.separator();
            let fl = self.clash_filter.map(|s| s.label()).unwrap_or("All statuses");
            let before = self.clash_filter;
            egui::ComboBox::from_id_salt("cfilter").selected_text(fl).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.clash_filter, None, "All statuses");
                for s in ClashStatus::ALL {
                    ui.selectable_value(&mut self.clash_filter, Some(s), s.label());
                }
            });
            if before != self.clash_filter {
                self.colors_dirty = true;
            }
            ui.separator();
            ui.label("Filter by selection:").on_hover_text("Show only clashes involving the elements selected in the model / tree");
            for (m, lbl, tip) in [
                (0, "None", "Show all clashes"),
                (1, "Inclusive", "Clashes involving at least one selected item"),
                (2, "Exclusive", "Clashes involving ALL the selected items"),
            ] {
                if ui.selectable_value(&mut self.clash_filter_sel, m, lbl).on_hover_text(tip).clicked() {
                    self.colors_dirty = true;
                }
            }
            ui.separator();
            let sel: Vec<usize> = self.sel_results.iter().copied().collect();
            if ui.add_enabled(!sel.is_empty(), egui::Button::new("📁 New Group")).on_hover_text("Group the selected clashes (Ctrl / Shift + click to select several)").clicked() {
                self.new_group(ti, &sel);
            }
            if ui.add_enabled(!sel.is_empty(), egui::Button::new("Ungroup")).clicked() {
                self.ungroup(ti, &sel);
            }
            ui.menu_button("Auto-group ⏷", |ui| {
                for g in GroupBy::ALL {
                    if g == GroupBy::Proximity {
                        ui.horizontal(|ui| {
                            if ui.button(g.label()).clicked() {
                                self.auto_group(ti, g, self.group_cell);
                                ui.close_menu();
                            }
                            ui.add(egui::DragValue::new(&mut self.group_cell).range(0.5..=100.0).speed(0.5).suffix(" m"));
                        });
                    } else if ui.button(format!("by {}", g.label())).clicked() {
                        self.auto_group(ti, g, self.group_cell);
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button("Remove all groups").clicked() {
                    let all: Vec<usize> = (0..self.clash_tests[ti].results.len()).collect();
                    self.ungroup(ti, &all);
                    ui.close_menu();
                }
            });
            if ui.small_button("Expand all").clicked() {
                self.open_groups = self.group_names(ti).into_iter().collect();
            }
            if ui.small_button("Collapse all").clicked() {
                self.open_groups.clear();
            }
            let lr = self.clash_tests[ti].last_run.clone();
            if !lr.is_empty() {
                ui.label(RichText::new(format!("Last run: {lr}")).weak().small());
            }
        });
        ui.separator();
        self.clash_table(ui, ti);
    }

    pub(super) fn clash_table(&mut self, ui: &mut egui::Ui, ti: usize) {
        let dark_clash = ui.visuals().dark_mode;
        #[derive(Clone)]
        enum Row {
            Group(String, Vec<usize>),
            Item(usize, bool),
        }
        let filter = self.clash_filter;
        let sel_mode = self.clash_filter_sel;
        let picked: std::collections::BTreeSet<ElemId> = self.selected.iter().copied().collect();
        let res_len = self.clash_tests[ti].results.len();
        let pass = |r: &ClashResult| {
            if filter.map_or(false, |f| r.status != f) {
                return false;
            }
            match sel_mode {
                1 => !picked.is_empty() && (picked.contains(&r.a) || picked.contains(&r.b)),
                2 => !picked.is_empty() && picked.contains(&r.a) && picked.contains(&r.b),
                _ => true,
            }
        };
        // locatia: axe + nivel (C-4 / Level 2) cand exista axe, altfel nivelul elementului A
        let grids = self.scene_grids();
        let use_grid = !grids.grids.is_empty();
        let labels: Vec<(String, String, String)> = (0..res_len)
            .map(|i| {
                let r = &self.clash_tests[ti].results[i];
                let la = self.elem(r.a).map(|(m, e)| format!("{} [{}]", e.name, m.name)).unwrap_or_default();
                let lb = if r.b_is_scan { "Point cloud (scan)".to_string() } else { self.elem(r.b).map(|(m, e)| format!("{} [{}]", e.name, m.name)).unwrap_or_default() };
                let lv = if use_grid { crate::app::grid_location_in(&grids, glam::Vec3::from(r.point)) } else { self.elem(r.a).map(|(_, e)| e.storey.clone()).unwrap_or_default() };
                (la, lb, lv)
            })
            .collect();
        // ---- ordinea: dupa coloana aleasa in antet (clic pe antet), altfel ordinea testului
        let (sort_col, sort_asc) = (self.settings.clash_ui.sort_col, self.settings.clash_ui.sort_asc);
        let mut order: Vec<usize> = (0..res_len).collect();
        if let Some(c) = sort_col {
            let results = &self.clash_tests[ti].results;
            // "Clash12" -> ("Clash", 12): Clash2 inaintea lui Clash10
            let nat = |s: &str| {
                let p = s.trim_end_matches(|ch: char| ch.is_ascii_digit());
                (p.to_lowercase(), s[p.len()..].parse::<u64>().unwrap_or(0))
            };
            let st_ix = |s: ClashStatus| ClashStatus::ALL.iter().position(|x| *x == s).unwrap_or(0);
            order.sort_by(|&x, &y| {
                let (a, b) = (&results[x], &results[y]);
                let o = match c {
                    0 => nat(&a.name).cmp(&nat(&b.name)),
                    1 => st_ix(a.status).cmp(&st_ix(b.status)),
                    2 => a.dist.partial_cmp(&b.dist).unwrap_or(std::cmp::Ordering::Equal),
                    3 => labels[x].0.to_lowercase().cmp(&labels[y].0.to_lowercase()),
                    4 => labels[x].1.to_lowercase().cmp(&labels[y].1.to_lowercase()),
                    5 => labels[x].2.cmp(&labels[y].2),
                    6 => a.found.cmp(&b.found),
                    7 => a.assigned.to_lowercase().cmp(&b.assigned.to_lowercase()),
                    _ => a.comment.to_lowercase().cmp(&b.comment.to_lowercase()),
                };
                let o = if sort_asc { o } else { o.reverse() };
                o.then(x.cmp(&y))
            });
        }
        // ---- randuri afisate: grupuri (la pozitia primului membru) + rezultate negrupate
        let mut rows: Vec<Row> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        {
            let results = &self.clash_tests[ti].results;
            for &i in &order {
                let r = &results[i];
                if r.group.is_empty() {
                    if pass(r) {
                        rows.push(Row::Item(i, false));
                    }
                } else if seen.insert(r.group.clone()) {
                    let members: Vec<usize> = order.iter().copied().filter(|&k| results[k].group == r.group && pass(&results[k])).collect();
                    if !members.is_empty() {
                        rows.push(Row::Group(r.group.clone(), members.clone()));
                        if self.open_groups.contains(&r.group) {
                            rows.extend(members.into_iter().map(|k| Row::Item(k, true)));
                        }
                    }
                }
            }
        }
        if rows.is_empty() {
            ui.label(RichText::new("No results. Choose Selection A / B in the Select tab and press “Run Test” (or Update All). Click a result to view it in 3D; Ctrl / Shift + click selects several.").weak());
            return;
        }
        let items: Vec<usize> = rows.iter().filter_map(|r| if let Row::Item(i, _) = r { Some(*i) } else { None }).collect();
        // ---- Page Up / Down
        let (pu, pd) = ui.input(|i| (i.key_pressed(Key::PageUp), i.key_pressed(Key::PageDown)));
        if (pu || pd) && !items.is_empty() {
            let pos = self.cur_result.and_then(|c| items.iter().position(|r| *r == c));
            let next = match (pos, pd) {
                (None, _) => 0,
                (Some(p), true) => (p + 1).min(items.len() - 1),
                (Some(p), false) => p.saturating_sub(1),
            };
            self.sel_results = [items[next]].into_iter().collect();
            self.apply(Action::FocusClash(ti, items[next]));
        }
        let group_status: HashMap<String, ClashStatus> = rows
            .iter()
            .filter_map(|r| if let Row::Group(n, m) = r { Some((n.clone(), self.group_status(ti, m))) } else { None })
            .collect();
        let pal = self.pal();
        let sel = self.sel_results.clone();
        let open_groups = self.open_groups.clone();
        let mods = ui.input(|i| i.modifiers);
        let mut clicked: Option<Row> = None;
        let mut toggle: Option<String> = None;
        let mut rename: Option<(String, String)> = None;
        let (sty, stol, ssev) = (self.clash_tests[ti].ty, self.clash_tests[ti].tol_mm, self.clash_tests[ti].sev_mm);
        let sev_of = |d: f32| crate::app::severity_of(sty, stol, ssev, d);
        let mut sort_click: Option<u8> = None;
        // panoul Item 1 / Item 2 ia partea de jos cand e un rezultat curent
        let show_items = self.settings.clash_ui.items_panel && self.cur_result.map_or(false, |c| c < res_len);
        let avail = ui.available_height();
        let items_h = if show_items { (avail * 0.38).clamp(120.0, 260.0) } else { 0.0 };
        let test = &mut self.clash_tests[ti];
        let mh = (avail - 26.0 - items_h).max(60.0);
        TableBuilder::new(ui)
            .max_scroll_height(mh)
            .auto_shrink([false, true])
            .striped(true)
            .resizable(true)
            .sense(Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::initial(230.0).clip(true))
            .column(Column::exact(92.0))
            .column(Column::exact(86.0))
            .column(Column::initial(200.0).clip(true))
            .column(Column::initial(200.0).clip(true))
            .column(Column::initial(90.0).clip(true))
            .column(Column::initial(118.0).clip(true))
            .column(Column::initial(90.0))
            .column(Column::remainder().at_least(120.0))
            .header(20.0, |mut h| {
                for (ci, n) in ["Name", "Status", "Distance", "Item A", "Item B", if use_grid { "Location" } else { "Level" }, "Found", "Assigned to", "Comment"].into_iter().enumerate() {
                    h.col(|ui| {
                        let arrow = match sort_col {
                            Some(c) if c as usize == ci => if sort_asc { " ▲" } else { " ▼" },
                            _ => "",
                        };
                        let r = ui.add(egui::Label::new(RichText::new(format!("{n}{arrow}")).strong()).sense(Sense::click()));
                        if r.on_hover_text("Click to sort (again = reverse, third time = test order)").clicked() {
                            sort_click = Some(ci as u8);
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(22.0, rows.len(), |mut row| {
                    let k = row.index();
                    match rows[k].clone() {
                        Row::Group(name, members) => {
                            row.set_selected(members.iter().all(|m| sel.contains(m)));
                            let open = open_groups.contains(&name);
                            row.col(|ui| {
                                if ui.add(egui::Button::new(if open { "⏷" } else { "⏵" }).small().frame(false)).clicked() {
                                    toggle = Some(name.clone());
                                }
                                ui.label("📁");
                                let id = egui::Id::new(("gname", ti, name.clone()));
                                let mut buf = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| name.clone());
                                let resp = ui.add(egui::TextEdit::singleline(&mut buf).desired_width(f32::INFINITY).font(egui::TextStyle::Body));
                                if resp.has_focus() {
                                    ui.data_mut(|d| d.insert_temp(id, buf.clone()));
                                }
                                if resp.lost_focus() {
                                    if !buf.trim().is_empty() && buf.trim() != name {
                                        rename = Some((name.clone(), buf.trim().to_string()));
                                    }
                                    ui.data_mut(|d| d.remove::<String>(id));
                                }
                            });
                            row.col(|ui| {
                                let gs = group_status[&name];
                                let mut st = gs;
                                egui::ComboBox::from_id_salt(("gst", ti, name.clone())).width(80.0).selected_text(RichText::new(gs.label()).color(c32(gs.color()))).show_ui(ui, |ui| {
                                    for s in ClashStatus::ALL {
                                        ui.selectable_value(&mut st, s, RichText::new(s.label()).color(c32(s.color())));
                                    }
                                });
                                if st != gs {
                                    for m in &members {
                                        crate::app::clash_logic::apply_status(&mut test.results[*m], st);
                                    }
                                }
                            });
                            row.col(|ui| {
                                let d = members.iter().map(|m| test.results[*m].dist).fold(f32::INFINITY, f32::min);
                                ui.label(format!("{d:.3}"));
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(format!("{} clashes", members.len())).strong());
                            });
                            row.col(|_| {});
                            row.col(|ui| {
                                let l0 = &labels[members[0]].2;
                                let same = members.iter().all(|m| &labels[*m].2 == l0);
                                ui.label(if same { l0.clone() } else { "Mixed".into() });
                            });
                            row.col(|_| {});
                            row.col(|ui| {
                                let mut a = test.results[members[0]].assigned.clone();
                                if ui.add(egui::TextEdit::singleline(&mut a).desired_width(f32::INFINITY)).changed() {
                                    for m in &members {
                                        test.results[*m].assigned = a.clone();
                                    }
                                }
                            });
                            row.col(|ui| {
                                let mut c = test.results[members[0]].comment.clone();
                                if ui.add(egui::TextEdit::singleline(&mut c).desired_width(f32::INFINITY).hint_text("comment for the whole group")).changed() {
                                    for m in &members {
                                        test.results[*m].comment = c.clone();
                                    }
                                }
                            });
                            if row.response().clicked() {
                                clicked = Some(Row::Group(name, members));
                            }
                        }
                        Row::Item(i, indent) => {
                            row.set_selected(sel.contains(&i));
                            let r = &mut test.results[i];
                            row.col(|ui| {
                                if indent {
                                    ui.add_space(22.0);
                                }
                                ui.label(&r.name);
                            });
                            row.col(|ui| {
                                let mut st = r.status;
                                let badge = c32(r.status.color()).gamma_multiply(if dark_clash { 0.20 } else { 0.18 });
                                egui::Frame::none().fill(badge).rounding(4.0).inner_margin(egui::Margin::symmetric(4.0, 1.0)).show(ui, |ui| {
                                let cb = egui::ComboBox::from_id_salt(("st", i)).width(72.0).selected_text(RichText::new(r.status.label()).color(c32(r.status.color())));
                                let cb = cb.show_ui(ui, |ui| {
                                    for s in ClashStatus::ALL {
                                        ui.selectable_value(&mut st, s, RichText::new(s.label()).color(c32(s.color())));
                                    }
                                });
                                cb.response.on_hover_text(format!("Approved: {} {}\nResolved: {} {}", r.approved_by, r.approved_at, r.resolved_by, r.resolved_at));
                                });
                                if st != r.status {
                                    crate::app::clash_logic::apply_status(r, st);
                                }
                            });
                            row.col(|ui| {
                                let sv = sev_of(r.dist);
                                let c = sv.color();
                                ui.label(RichText::new("●").color(Color32::from_rgb(c[0], c[1], c[2]))).on_hover_text(sv.label());
                                ui.label(format!("{:.0} mm", r.dist * 1000.0)).on_hover_text(format!("{} - {}", sv.label(), if r.dist < 0.0 { format!("penetration {:.0} mm", -r.dist * 1000.0) } else { format!("gap {:.0} mm", r.dist * 1000.0) }));
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(&labels[i].0).color(pal.elem_a));
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(&labels[i].1).color(pal.elem_b));
                            });
                            row.col(|ui| {
                                ui.label(&labels[i].2);
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(&r.found).small());
                            });
                            row.col(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut r.assigned).desired_width(f32::INFINITY));
                            });
                            row.col(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut r.comment).desired_width(f32::INFINITY));
                            });
                            if row.response().clicked() {
                                clicked = Some(Row::Item(i, indent));
                            }
                        }
                    }
                });
            });
        if let Some(g) = toggle {
            if !self.open_groups.remove(&g) {
                self.open_groups.insert(g);
            }
        }
        if let Some((old, new)) = rename {
            for r in self.clash_tests[ti].results.iter_mut() {
                if r.group == old {
                    r.group = new.clone();
                }
            }
            if self.open_groups.remove(&old) {
                self.open_groups.insert(new);
            }
        }
        match clicked {
            Some(Row::Item(i, _)) => {
                if mods.shift {
                    if let (Some(a), Some(b)) = (self.cur_result.and_then(|c| items.iter().position(|x| *x == c)), items.iter().position(|x| *x == i)) {
                        let (lo, hi) = (a.min(b), a.max(b));
                        if !mods.command {
                            self.sel_results.clear();
                        }
                        self.sel_results.extend(items[lo..=hi].iter().copied());
                    } else {
                        self.sel_results = [i].into_iter().collect();
                    }
                } else if mods.command {
                    if !self.sel_results.remove(&i) {
                        self.sel_results.insert(i);
                    }
                } else {
                    self.sel_results = [i].into_iter().collect();
                }
                let v: Vec<usize> = self.sel_results.iter().copied().collect();
                if v.is_empty() {
                    self.clash_focus = None;
                    self.colors_dirty = true;
                } else {
                    let keep = self.cur_result;
                    self.apply(Action::FocusClashes(ti, v));
                    if mods.shift {
                        self.cur_result = keep;
                    } else {
                        self.cur_result = Some(i);
                    }
                }
            }
            Some(Row::Group(_, members)) => {
                if !mods.command {
                    self.sel_results.clear();
                }
                self.sel_results.extend(members.iter().copied());
                let v: Vec<usize> = self.sel_results.iter().copied().collect();
                self.apply(Action::FocusClashes(ti, v));
            }
            None => {}
        }
        // clic pe antet: crescator -> descrescator -> ordinea testului
        if let Some(c) = sort_click {
            let p = &mut self.settings.clash_ui;
            match p.sort_col {
                Some(x) if x == c && p.sort_asc => p.sort_asc = false,
                Some(x) if x == c => p.sort_col = None,
                _ => {
                    p.sort_col = Some(c);
                    p.sort_asc = true;
                }
            }
            self.settings.save();
        }
        if show_items {
            if let Some(i) = self.cur_result {
                ui.separator();
                self.clash_items_panel(ui, ti, i);
            }
        }
    }

    /// Item 1 / Item 2 (ca in Navisworks): datele si proprietatile celor doua elemente ale
    /// rezultatului curent, cu Select / Zoom / Hide si viewpoint-ul salvat al rezultatului.
    fn clash_items_panel(&mut self, ui: &mut egui::Ui, ti: usize, i: usize) {
        let Some(r) = self.clash_tests.get(ti).and_then(|t| t.results.get(i)).cloned() else { return };
        let mut act: Option<Action> = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new(&r.name).strong());
            ui.label(RichText::new(r.status.label()).color(c32(r.status.color())));
            ui.label(RichText::new(if r.dist < 0.0 { format!("penetration {}", fmt_len(-r.dist as f64)) } else { format!("gap {}", fmt_len(r.dist as f64)) }).weak());
            ui.separator();
            let cam = self.cam_anim.map(|(_, to, _)| to).unwrap_or(self.cam);
            if ui.button(if r.view.is_some() { "📷 Update viewpoint" } else { "📷 Save viewpoint" }).on_hover_text("Save the current camera on this clash: selecting it again (and the BCF / Navisworks export) uses this view").clicked() {
                if let Some(x) = self.clash_tests.get_mut(ti).and_then(|t| t.results.get_mut(i)) {
                    x.view = Some(cam);
                }
                self.toast(format!("Viewpoint saved on {}.", r.name));
            }
            if r.view.is_some() {
                ui.label(RichText::new("● saved view").small().color(Color32::from_rgb(80, 200, 120)));
                if ui.small_button("✖").on_hover_text("Forget the saved viewpoint (use auto-zoom again)").clicked() {
                    if let Some(x) = self.clash_tests.get_mut(ti).and_then(|t| t.results.get_mut(i)) {
                        x.view = None;
                    }
                }
            }
            if !r.approved_by.is_empty() {
                ui.label(RichText::new(format!("Approved: {} {}", r.approved_by, r.approved_at)).small().weak());
            }
            if !r.resolved_by.is_empty() {
                ui.label(RichText::new(format!("Resolved: {} {}", r.resolved_by, r.resolved_at)).small().weak());
            }
        });
        let (ca, cb) = (self.settings.clash_ui.col_a, self.settings.clash_ui.col_b);
        let items = [("Item 1", r.a, ca, false), ("Item 2", r.b, cb, r.b_is_scan)];
        ui.columns(2, |cols| {
            for (k, (title, id, col, scan)) in items.into_iter().enumerate() {
                let ui = &mut cols[k];
                ui.horizontal(|ui| {
                    ui.label(RichText::new("■").color(Color32::from_rgb(col[0], col[1], col[2])));
                    ui.label(RichText::new(title).strong());
                    if scan {
                        return;
                    }
                    if ui.small_button("Select").on_hover_text("Select only this item").clicked() {
                        act = Some(Action::Select(vec![id], false));
                    }
                    if ui.small_button("Zoom").on_hover_text("Zoom to this item").clicked() {
                        act = Some(Action::Focus(vec![id]));
                    }
                    if ui.small_button("Hide").on_hover_text("Hide this item (to see what is behind it)").clicked() {
                        act = Some(Action::Hide(vec![id]));
                    }
                });
                if scan {
                    ui.label(RichText::new("Point cloud (scan)").weak());
                    continue;
                }
                let Some((m, e)) = self.elem(id) else {
                    ui.label(RichText::new("Element not loaded").weak());
                    continue;
                };
                egui::ScrollArea::vertical().id_salt(("clash_item", k)).max_height(ui.available_height().max(80.0)).auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new(("clash_item_grid", k)).num_columns(2).striped(true).spacing([10.0, 2.0]).show(ui, |ui| {
                        for (n, v) in [("Name", e.name.as_str()), ("Model", m.name.as_str()), ("Type", e.ifc_type.as_str()), ("Level", e.storey.as_str()), ("GUID", e.guid.as_str())] {
                            ui.label(RichText::new(n).small().weak());
                            ui.label(RichText::new(v).small());
                            ui.end_row();
                        }
                        let mut last_group = "";
                        for p in &e.props {
                            if p.value.trim().is_empty() {
                                continue;
                            }
                            if p.group != last_group {
                                last_group = &p.group;
                                ui.label(RichText::new(&p.group).small().strong().color(accent()));
                                ui.label("");
                                ui.end_row();
                            }
                            ui.label(RichText::new(&p.name).small().weak());
                            ui.label(RichText::new(&p.value).small());
                            ui.end_row();
                        }
                    });
                });
            }
        });
        if let Some(a) = act {
            self.apply(a);
        }
    }

    // ------------------------------------------------------------------ TimeLiner
    pub(super) fn attach_selection(&mut self) {
        let Some(t) = self.cur_task else {
            self.error("Select a task in TimeLiner first.");
            return;
        };
        let sel: Vec<ElemId> = self.selected.iter().copied().collect();
        if let Some(task) = self.tasks.get_mut(t) {
            let mut s: HashSet<ElemId> = task.elems.iter().copied().collect();
            s.extend(sel);
            task.elems = s.into_iter().collect();
            task.elems.sort();
            let n = task.elems.len();
            self.invalidate_sim();
            self.toast(format!("The task now has {n} attached elements."));
        }
    }
}
