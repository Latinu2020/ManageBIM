//! The BIM classification window: local catalog (Uniclass / OmniClass / imported from CSV-JSON) and
//! online search in the buildingSMART Data Dictionary (bSDD). The chosen code is written on the selected
//! elements as the "Classification" property (the group = the system / dictionary) plus
//! "Classification URI" for the bSDD classes; on IFC export they become IfcClassificationReference.
use super::*;

/// the names of the properties written on the elements
const P_CLASS: &str = "Classification";
const P_URI: &str = "Classification URI";

impl App {
    /// The Classification window (opened from BIM › bSDD).
    pub(super) fn classification_window(&mut self, ctx: &egui::Context) {
        self.poll_bsdd();
        if !self.class_open {
            return;
        }
        let mut open = true;
        egui::Window::new(crate::theme::icon_job("🏷", "Classification · bSDD", 14.0, ctx.style().visuals.text_color()))
            .id(egui::Id::new("class_win"))
            .open(&mut open)
            .default_size([760.0, 560.0])
            .max_width(1100.0)
            .min_width(520.0)
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| self.classification_ui(ui));
        if !open {
            self.class_open = false;
        }
    }

    /// Reads the results of the bSDD searches running on a separate thread.
    fn poll_bsdd(&mut self) {
        if let Some(rx) = &self.bsdd_rx {
            match rx.try_recv() {
                Ok(Ok((classes, total))) => {
                    self.bsdd_status = if classes.is_empty() { "No classes found in bSDD.".into() } else { format!("{} of {} classes", classes.len(), total) };
                    self.bsdd_results = classes;
                    self.bsdd_total = total;
                    self.bsdd_rx = None;
                }
                Ok(Err(e)) => {
                    self.bsdd_status = e;
                    self.bsdd_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.bsdd_status = "bSDD search stopped unexpectedly.".into();
                    self.bsdd_rx = None;
                }
            }
        }
        if let Some(rx) = &self.bsdd_dicts_rx {
            match rx.try_recv() {
                Ok(Ok(d)) => {
                    self.bsdd_dicts = d;
                    self.bsdd_dicts_rx = None;
                }
                Ok(Err(e)) => {
                    self.bsdd_status = e;
                    self.bsdd_dicts_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.bsdd_dicts_rx = None,
            }
        }
    }

    fn bsdd_search(&mut self) {
        let q = self.bsdd_query.trim().to_string();
        if q.len() < 2 {
            self.bsdd_status = "Type at least 2 characters.".into();
            return;
        }
        let dict = if self.bsdd_dict.is_empty() { None } else { Some(self.bsdd_dict.clone()) };
        self.bsdd_rx = Some(crate::bsdd::spawn_search(q, dict, 200));
        self.bsdd_status = "Searching bSDD…".into();
    }

    /// Writes the classification on the selected elements.
    fn assign_class(&mut self, system: &str, code: &str, name: &str, uri: &str) {
        let v: Vec<_> = self.selected.iter().copied().collect();
        if v.is_empty() {
            return;
        }
        let value = if name.trim().is_empty() { code.to_string() } else { format!("{code} — {name}") };
        let n = self.set_user_prop(&v, system, P_CLASS, &value);
        if !uri.trim().is_empty() {
            self.set_user_prop(&v, system, P_URI, uri);
        }
        self.toast(format!("{code} ({system}) assigned to {n} element(s)"));
    }

    /// Deletes all the manually assigned classifications on the selected elements.
    fn clear_classes(&mut self) {
        let v: Vec<_> = self.selected.iter().copied().collect();
        let groups: std::collections::BTreeSet<String> = self.user_params.iter().filter(|p| p.name.eq_ignore_ascii_case(P_CLASS)).map(|p| p.group.clone()).collect();
        let mut n = 0;
        for g in groups {
            n += self.remove_user_param(&v, &g, P_CLASS);
            self.remove_user_param(&v, &g, P_URI);
        }
        self.toast(format!("{n} classification(s) removed"));
    }

    /// The existing classifications on the main selected element (system, value).
    fn current_classes(&self) -> Vec<(String, String)> {
        let Some(id) = self.primary.or_else(|| self.selected.iter().next().copied()) else { return Vec::new() };
        let Some((_, e)) = self.elem(id) else { return Vec::new() };
        e.props
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case(P_CLASS) || p.group.eq_ignore_ascii_case(P_CLASS))
            .filter(|p| !p.value.trim().is_empty() && !p.name.ends_with(" name"))
            .map(|p| (if p.name.eq_ignore_ascii_case(P_CLASS) { p.group.clone() } else { p.name.clone() }, p.value.clone()))
            .collect()
    }

    pub(super) fn classification_ui(&mut self, ui: &mut egui::Ui) {
        let sel_count = self.selected.len();
        // ---- header: the selection and its classifications
        egui::Frame::none().fill(ui.visuals().faint_bg_color).rounding(crate::theme::RADIUS_SM).inner_margin(egui::Margin::symmetric(8.0, 6.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (txt, col) = if sel_count > 0 { (format!("{sel_count} element(s) selected"), accent()) } else { ("Select elements in the 3D view or the tree to assign a code".to_string(), ui.visuals().weak_text_color()) };
                ui.label(RichText::new(txt).small().strong().color(col));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(sel_count > 0, egui::Button::new(RichText::new("✖ Clear").small())).on_hover_text("Remove the classifications assigned in ManageBIM from the selected elements").clicked() {
                        self.clear_classes();
                    }
                });
            });
            let cur = self.current_classes();
            if !cur.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Current:").small().weak());
                    for (sys, val) in cur.iter().take(6) {
                        ui.label(RichText::new(format!("{sys}: {val}")).small().monospace()).on_hover_text(sys);
                    }
                });
            }
        });
        ui.add_space(4.0);
        // ---- tabs
        ui.horizontal(|ui| {
            for (i, (icon, label, tip)) in [("📚", "Local catalogue", "Offline list: built-in Uniclass / OmniClass, or your own CSV / JSON"), ("🌐", "bSDD online", "Search the buildingSMART Data Dictionary (IFC, Uniclass, ETIM, NL-SfB, CCI, …)")].into_iter().enumerate() {
                if ui.selectable_label(self.class_tab == i, RichText::new(format!("{icon} {label}")).strong()).on_hover_text(tip).clicked() {
                    self.class_tab = i;
                }
            }
        });
        ui.separator();
        if self.class_tab == 0 {
            self.class_local_ui(ui, sel_count);
        } else {
            self.class_bsdd_ui(ui, sel_count);
        }
    }

    fn class_local_ui(&mut self, ui: &mut egui::Ui, sel_count: usize) {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.class_search).hint_text("🔍 Search code, name or system…").desired_width(ui.available_width() - 210.0));
            if ui.small_button("📥 Import").on_hover_text("Import a classification catalogue (CSV with columns System;Code;Name;Description, or JSON)").clicked() {
                self.request_path(Purpose::ImportClassifications);
            }
            if ui.small_button("📤 Export").on_hover_text("Export the catalogue to CSV (opens in Excel)").clicked() {
                self.request_path(Purpose::ExportClassifications);
            }
            if ui.small_button("↺").on_hover_text("Restore the built-in Uniclass 2015 / OmniClass list").clicked() {
                self.settings.classifications = crate::classification::builtin();
                self.settings.save();
            }
        });
        let q = self.class_search.trim().to_lowercase();
        let shown: Vec<usize> = (0..self.settings.classifications.len())
            .filter(|&i| {
                let c = &self.settings.classifications[i];
                q.is_empty() || c.code.to_lowercase().contains(&q) || c.name.to_lowercase().contains(&q) || c.system.to_lowercase().contains(&q)
            })
            .take(500)
            .collect();
        ui.label(RichText::new(format!("{} of {} entries", shown.len(), self.settings.classifications.len())).small().weak());
        let mut assign: Option<(String, String, String, String)> = None;
        let mut remove: Option<usize> = None;
        // fixed table height: the window no longer grows / shrinks with the number of rows
        let h = (ui.available_height() - 6.0).clamp(160.0, 430.0);
        egui_extras::TableBuilder::new(ui)
            .id_salt("class_local_tbl")
            .striped(true)
            .resizable(false)
            .auto_shrink([false, false])
            .min_scrolled_height(h)
            .max_scroll_height(h)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(egui_extras::Column::exact(30.0))
            .column(egui_extras::Column::initial(120.0).at_least(60.0))
            .column(egui_extras::Column::remainder().at_least(120.0))
            .column(egui_extras::Column::initial(110.0).at_least(60.0))
            .column(egui_extras::Column::exact(24.0))
            .header(20.0, |mut h| {
                h.col(|_| {});
                h.col(|ui| { ui.label(RichText::new("Code").small().strong()); });
                h.col(|ui| { ui.label(RichText::new("Name").small().strong()); });
                h.col(|ui| { ui.label(RichText::new("System").small().strong()); });
                h.col(|_| {});
            })
            .body(|body| {
                body.rows(22.0, shown.len(), |mut row| {
                    let i = shown[row.index()];
                    let c = self.settings.classifications[i].clone();
                    row.col(|ui| {
                        if ui.add_enabled(sel_count > 0, egui::Button::new("➕").small()).on_hover_text(format!("Assign {} to the {sel_count} selected element(s)", c.code)).clicked() {
                            let uri = if c.description.starts_with("http") { c.description.clone() } else { String::new() };
                            assign = Some((c.system.clone(), c.code.clone(), c.name.clone(), uri));
                        }
                    });
                    row.col(|ui| { ui.label(RichText::new(&c.code).monospace().small()); });
                    row.col(|ui| {
                        let r = ui.label(RichText::new(&c.name).small());
                        if !c.description.is_empty() {
                            r.on_hover_text(&c.description);
                        }
                    });
                    row.col(|ui| { ui.label(RichText::new(&c.system).small().weak()); });
                    row.col(|ui| {
                        if ui.small_button("🗑").on_hover_text("Remove from the catalogue").clicked() {
                            remove = Some(i);
                        }
                    });
                });
            });
        if let Some((s, c, n, u)) = assign {
            self.assign_class(&s, &c, &n, &u);
        }
        if let Some(i) = remove {
            self.settings.classifications.remove(i);
            self.settings.save();
        }
        if self.settings.classifications.is_empty() {
            ui.label(RichText::new("The catalogue is empty. Import a CSV / JSON, add classes from bSDD online, or restore the default list.").weak().small());
        }
    }

    fn class_bsdd_ui(&mut self, ui: &mut egui::Ui, sel_count: usize) {
        let busy = self.bsdd_rx.is_some();
        if self.bsdd_dicts.is_empty() && self.bsdd_dicts_rx.is_none() {
            self.bsdd_dicts_rx = Some(crate::bsdd::spawn_dictionaries());
        }
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut self.bsdd_query).hint_text("🔍 e.g. wall, door, fire damper, EF_25_10…").desired_width(ui.available_width() - 250.0));
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            let sel_label = if self.bsdd_dict.is_empty() {
                "All dictionaries".to_string()
            } else {
                self.bsdd_dicts.iter().find(|d| d.uri == self.bsdd_dict).map(|d| d.label()).unwrap_or_else(|| self.bsdd_dict.rsplit('/').next().unwrap_or("").to_string())
            };
            egui::ComboBox::from_id_salt("bsdd_dict").width(150.0).selected_text(RichText::new(sel_label).small()).show_ui(ui, |ui| {
                ui.set_min_width(260.0);
                ui.selectable_value(&mut self.bsdd_dict, String::new(), "All dictionaries");
                if self.bsdd_dicts.is_empty() {
                    ui.label(RichText::new(if self.bsdd_dicts_rx.is_some() { "Loading dictionaries…" } else { "Dictionaries unavailable" }).small().weak());
                }
                for d in &self.bsdd_dicts {
                    ui.selectable_value(&mut self.bsdd_dict, d.uri.clone(), d.label()).on_hover_text(&d.uri);
                }
            });
            if ui.add_enabled(!busy, egui::Button::new(RichText::new("Search").strong())).clicked() || (enter && !busy) {
                self.bsdd_search();
            }
        });
        ui.horizontal(|ui| {
            if busy {
                ui.spinner();
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }
            ui.label(RichText::new(&self.bsdd_status).small().weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.hyperlink_to(RichText::new("search.bsdd.buildingsmart.org ↗").small(), "https://search.bsdd.buildingsmart.org");
            });
        });
        let mut assign: Option<crate::bsdd::BsddClass> = None;
        let mut keep: Option<crate::bsdd::BsddClass> = None;
        let rows = self.bsdd_results.clone();
        // fixed table height: the window no longer grows / shrinks with the number of rows
        let h = (ui.available_height() - 6.0).clamp(160.0, 430.0);
        egui_extras::TableBuilder::new(ui)
            .id_salt("class_bsdd_tbl")
            .striped(true)
            .resizable(false)
            .auto_shrink([false, false])
            .min_scrolled_height(h)
            .max_scroll_height(h)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(egui_extras::Column::exact(30.0))
            .column(egui_extras::Column::exact(26.0))
            .column(egui_extras::Column::initial(130.0).at_least(60.0))
            .column(egui_extras::Column::remainder().at_least(120.0))
            .column(egui_extras::Column::initial(110.0).at_least(60.0))
            .header(20.0, |mut h| {
                h.col(|_| {});
                h.col(|_| {});
                h.col(|ui| { ui.label(RichText::new("Code").small().strong()); });
                h.col(|ui| { ui.label(RichText::new("Name").small().strong()); });
                h.col(|ui| { ui.label(RichText::new("Dictionary").small().strong()); });
            })
            .body(|body| {
                body.rows(22.0, rows.len(), |mut row| {
                    let c = &rows[row.index()];
                    let code = c.code();
                    row.col(|ui| {
                        if ui.add_enabled(sel_count > 0, egui::Button::new("➕").small()).on_hover_text(format!("Assign {code} to the {sel_count} selected element(s)")).clicked() {
                            assign = Some(c.clone());
                        }
                    });
                    row.col(|ui| {
                        if ui.small_button("⭐").on_hover_text("Keep in the local catalogue (works offline later)").clicked() {
                            keep = Some(c.clone());
                        }
                    });
                    row.col(|ui| { ui.label(RichText::new(&code).monospace().small()); });
                    row.col(|ui| {
                        let mut tip = String::new();
                        if !c.parent_class_name.is_empty() {
                            tip.push_str(&format!("Parent: {}\n", c.parent_class_name));
                        }
                        if !c.description.is_empty() {
                            tip.push_str(&c.description);
                            tip.push('\n');
                        }
                        tip.push_str(&c.uri);
                        ui.label(RichText::new(&c.name).small()).on_hover_text(tip);
                    });
                    row.col(|ui| { ui.label(RichText::new(&c.dictionary_name).small().weak()); });
                });
            });
        if rows.is_empty() && !busy {
            ui.add_space(8.0);
            ui.label(RichText::new("Search the buildingSMART Data Dictionary and assign a class to the selected elements. The class URI is saved too, and the code is exported to IFC as IfcClassificationReference.").small().weak());
        }
        if let Some(c) = assign {
            self.assign_class(&c.dictionary_name, &c.code(), &c.name, &c.uri);
        }
        if let Some(c) = keep {
            let it = c.to_item();
            if !self.settings.classifications.iter().any(|x| x.system == it.system && x.code == it.code) {
                self.settings.classifications.push(it);
                self.settings.save();
                self.toast(format!("{} added to the local catalogue", c.code()));
            }
        }
    }
}
