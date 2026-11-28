//! A section of `impl App` (see the parent file for the rest).
use super::*;

impl App {
    // ================================================================== Appearance Profiler
    pub(super) fn profiler_ui(&mut self, ui: &mut egui::Ui) {
        use crate::profiler::{ProfileMode, Unmatched};
        if self.loaded_models().next().is_none() {
            ui.label(RichText::new("No models loaded.").weak());
            return;
        }
        let cats: Vec<String> = self.prop_index.cats.iter().map(|c| c.0.clone()).collect();
        let props: Vec<String> = self.prop_index.props(&self.profile.cat).map(|v| v.iter().map(|p| p.name.clone()).collect()).unwrap_or_default();
        let mut regen = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("Category");
            egui::ComboBox::from_id_salt("prof_cat").width(150.0).selected_text(&self.profile.cat).show_ui(ui, |ui| {
                for c in &cats {
                    if ui.selectable_label(*c == self.profile.cat, c).clicked() && *c != self.profile.cat {
                        self.profile.cat = c.clone();
                        // the first property in the new category
                        self.profile.prop = self.prop_index.props(c).and_then(|v| v.first()).map(|p| p.name.clone()).unwrap_or_default();
                    }
                }
            });
            ui.label("Property");
            egui::ComboBox::from_id_salt("prof_prop").width(180.0).selected_text(&self.profile.prop).show_ui(ui, |ui| {
                for p in &props {
                    ui.selectable_value(&mut self.profile.prop, p.clone(), p);
                }
            });
            ui.separator();
            ui.radio_value(&mut self.profile.mode, ProfileMode::Values, "By value");
            ui.radio_value(&mut self.profile.mode, ProfileMode::Ranges, "By range");
            if self.profile.mode == ProfileMode::Ranges {
                ui.add(egui::DragValue::new(&mut self.profile.bins).range(2..=20).suffix(" ranges"));
            }
            if ui.button(RichText::new("▶ Generate").strong()).on_hover_text("Build the colour rules from the values found in all loaded models").clicked() {
                regen = true;
            }
            ui.separator();
            if ui.button("📂 Import…").on_hover_text("Load a saved profile (.bimprofile) with its colours and apply it to these models").clicked() {
                self.request_path(Purpose::ImportProfile);
            }
            if ui.add_enabled(!self.profile.rules.is_empty(), egui::Button::new("💾 Export…")).on_hover_text("Save this profile (property, rules and your colours) to reuse it on other models").clicked() {
                self.request_path(Purpose::ExportProfile);
            }
        });
        if regen {
            self.profile_generate();
        }
        if self.profile.rules.is_empty() {
            ui.label(RichText::new("Choose a property and press Generate. Examples: Item / Level, Item / IFC Type, a Pset's FireRating or System, a quantity like NetVolume (By range).").weak());
            return;
        }
        let mut dirty = false;
        ui.horizontal_wrapped(|ui| {
            dirty |= ui.checkbox(&mut self.profile.active, RichText::new("Show colours").strong()).changed();
            ui.checkbox(&mut self.profile_legend, "Legend").on_hover_text("Show the colour legend in the 3D view (also drawn on images and videos - Render options)");
            ui.label("Elements without a rule:");
            egui::ComboBox::from_id_salt("prof_unm").selected_text(self.profile.unmatched.label()).show_ui(ui, |ui| {
                for u in Unmatched::ALL {
                    dirty |= ui.selectable_value(&mut self.profile.unmatched, u, u.label()).changed();
                }
            });
            if ui.button("All visible").clicked() {
                self.profile.rules.iter_mut().for_each(|r| r.visible = true);
                dirty = true;
            }
            if self.profile.mode == ProfileMode::Values && ui.button("➕ Add missing values").on_hover_text("Keep your colours and add new colours only for values that have no rule yet (e.g. after importing a profile)").clicked() {
                self.profile_add_missing();
            }
            if ui.button("➕ Rule").on_hover_text(if self.profile.mode == ProfileMode::Values { "Add a value by hand (type it exactly as in the model)" } else { "Add a range after the last one" }).clicked() {
                self.profile.add_rule();
                self.profile_recount();
            }
            if ui.button("Clear").on_hover_text("Remove the profile and go back to the normal colours").clicked() {
                self.profile_clear();
            }
        });
        let mut select: Option<(usize, bool)> = None;
        let mut remap = false;
        let mut delete: Option<usize> = None;
        let ranges = self.profile.mode == ProfileMode::Ranges;
        egui::ScrollArea::vertical().id_salt("prof_rules").auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("prof_grid").striped(true).num_columns(6).show(ui, |ui| {
                ui.label(RichText::new("Colour").small());
                ui.label(RichText::new("Show").small());
                ui.label(RichText::new(format!("{} / {}", self.profile.cat, self.profile.prop)).small());
                ui.label(RichText::new(if ranges { "From – to" } else { "Edit value" }).small());
                ui.label(RichText::new("Items").small());
                ui.label("");
                ui.end_row();
                let n = self.profile.rules.len();
                for i in 0..n {
                    let r = &mut self.profile.rules[i];
                    let mut c = Color32::from_rgba_unmultiplied(r.color[0], r.color[1], r.color[2], 255);
                    if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
                        r.color = [c.r(), c.g(), c.b(), 255];
                        dirty = true;
                    }
                    dirty |= ui.checkbox(&mut r.visible, "").changed();
                    if ui.selectable_label(false, &r.label).on_hover_text("Click: select these items · Ctrl+click: add to the selection").clicked() {
                        select = Some((i, ui.input(|i| i.modifiers.command)));
                    }
                    if ranges {
                        let mut relabel = false;
                        ui.horizontal(|ui| {
                            if let (Some(lo), Some(hi)) = (r.lo.as_mut(), r.hi.as_mut()) {
                                relabel |= ui.add(egui::DragValue::new(lo).speed(0.1).max_decimals(3)).changed();
                                ui.label("–");
                                relabel |= ui.add(egui::DragValue::new(hi).speed(0.1).max_decimals(3)).changed();
                            }
                        });
                        if relabel {
                            self.profile.relabel_range(i);
                            remap = true;
                        }
                    } else {
                        let resp = ui.add(egui::TextEdit::singleline(&mut self.profile.rules[i].label).desired_width(140.0));
                        if resp.lost_focus() || (resp.changed() && !resp.has_focus()) {
                            remap = true;
                        }
                    }
                    ui.label(self.profile.rules[i].count.to_string());
                    if ui.small_button("🗑").on_hover_text("Remove this rule").clicked() {
                        delete = Some(i);
                    }
                    ui.end_row();
                }
            });
        });
        if let Some(i) = delete {
            self.profile.rules.remove(i);
            remap = true;
        }
        if remap {
            self.profile_recount();
        }
        if let Some((i, add)) = select {
            self.profile_select(i, add);
        }
        if dirty {
            self.colors_dirty = true;
        }
    }

    // ================================================================== Quantification
    pub(super) fn qto_ui(&mut self, ui: &mut egui::Ui) {
        use crate::qto::{fmt, QtoScope, QtyColumn};
        if self.loaded_models().next().is_none() {
            ui.label(RichText::new("No models loaded.").weak());
            return;
        }
        let cats: Vec<String> = self.prop_index.cats.iter().map(|c| c.0.clone()).collect();
        let props_of = |s: &App, c: &str| -> Vec<String> { s.prop_index.props(c).map(|v| v.iter().map(|p| p.name.clone()).collect()).unwrap_or_default() };
        // --- row 1: name, scope, templates, export
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.qto.name).desired_width(150.0).hint_text("Name"));
            egui::ComboBox::from_id_salt("qto_scope").selected_text(self.qto.scope.label()).show_ui(ui, |ui| {
                for s in QtoScope::ALL {
                    ui.selectable_value(&mut self.qto.scope, s, s.label());
                }
            });
            ui.separator();
            if ui.button("📊 Excel…").on_hover_text("Summary sheet (with totals) + itemized sheet - .xlsx").clicked() {
                self.request_path(Purpose::ExportQtoXlsx);
            }
            if ui.button("CSV…").on_hover_text("Summary as CSV (decimal comma, ';' separator)").clicked() {
                self.request_path(Purpose::ExportQtoCsv);
            }
            ui.separator();
            if ui.button("📂 Template…").on_hover_text("Load a saved quantification setup (.bimqto)").clicked() {
                self.request_path(Purpose::ImportQtoTemplate);
            }
            if ui.button("💾 Template…").on_hover_text("Save this setup (grouping + columns) to reuse it on other projects").clicked() {
                self.request_path(Purpose::ExportQtoTemplate);
            }
        });
        // --- row 2: grouping
        let mut del_g: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Group by").strong());
            for (i, lv) in self.qto.group_by.iter_mut().enumerate() {
                if i > 0 {
                    ui.label("›");
                }
                egui::ComboBox::from_id_salt(("qg_cat", i)).width(90.0).selected_text(&lv.cat).show_ui(ui, |ui| {
                    for c in &cats {
                        if ui.selectable_label(*c == lv.cat, c).clicked() && *c != lv.cat {
                            lv.cat = c.clone();
                            lv.prop = self.prop_index.props(c).and_then(|v| v.first()).map(|p| p.name.clone()).unwrap_or_default();
                        }
                    }
                });
                let props: Vec<String> = self.prop_index.props(&lv.cat).map(|v| v.iter().map(|p| p.name.clone()).collect()).unwrap_or_default();
                egui::ComboBox::from_id_salt(("qg_prop", i)).width(130.0).selected_text(&lv.prop).show_ui(ui, |ui| {
                    for p in &props {
                        ui.selectable_value(&mut lv.prop, p.clone(), p);
                    }
                });
                if ui.small_button("✕").on_hover_text("Remove this grouping level").clicked() {
                    del_g = Some(i);
                }
            }
            if self.qto.group_by.len() < 4 && ui.small_button("➕").on_hover_text("Add a grouping level").clicked() {
                self.qto.group_by.push(TreeLevel::new(ITEM_CAT, "Level"));
            }
        });
        if let Some(i) = del_g {
            self.qto.group_by.remove(i);
        }
        // --- row 3: columns
        let mut del_c: Option<usize> = None;
        let mut add_c: Option<QtyColumn> = None;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Columns").strong());
            for c in QtyColumn::GEOMETRIC {
                let mut on = self.qto.columns.contains(&c);
                if ui.checkbox(&mut on, c.title()).on_hover_text(c.hint()).changed() {
                    if on {
                        add_c = Some(c.clone());
                    } else if let Some(i) = self.qto.columns.iter().position(|x| *x == c) {
                        del_c = Some(i);
                    }
                }
            }
            for (i, c) in self.qto.columns.iter().enumerate() {
                if let QtyColumn::Prop { cat, prop } = c {
                    if ui.small_button(format!("{prop} ✕")).on_hover_text(format!("{cat} / {prop} - click to remove")).clicked() {
                        del_c = Some(i);
                    }
                }
            }
            // new numeric property (e.g. Qto_WallBaseQuantities / NetVolume)
            let id = ui.make_persistent_id("qto_newcol");
            let (mut pc, mut pp): (String, String) = ui.data_mut(|d| d.get_temp(id)).unwrap_or_default();
            if pc.is_empty() {
                pc = cats.iter().find(|c| c.starts_with("Qto_") || c.starts_with("BaseQuantities")).or(cats.first()).cloned().unwrap_or_default();
            }
            egui::ComboBox::from_id_salt("qto_nc_cat").width(110.0).selected_text(if pc.is_empty() { "Category" } else { &pc }).show_ui(ui, |ui| {
                for c in &cats {
                    if ui.selectable_label(*c == pc, c).clicked() {
                        pc = c.clone();
                        pp.clear();
                    }
                }
            });
            let props = props_of(self, &pc);
            egui::ComboBox::from_id_salt("qto_nc_prop").width(130.0).selected_text(if pp.is_empty() { "Property" } else { &pp }).show_ui(ui, |ui| {
                for p in &props {
                    ui.selectable_value(&mut pp, p.clone(), p);
                }
            });
            if ui.add_enabled(!pp.is_empty(), egui::Button::new("➕ Sum column")).on_hover_text("Add a column that sums this numeric property").clicked() {
                add_c = Some(QtyColumn::Prop { cat: pc.clone(), prop: pp.clone() });
            }
            ui.data_mut(|d| d.insert_temp(id, (pc, pp)));
        });
        if let Some(i) = del_c {
            self.qto.columns.remove(i);
        }
        if let Some(c) = add_c {
            if !self.qto.columns.contains(&c) {
                self.qto.columns.push(c);
                // the geometric columns in their fixed order, the properties at the end (stable sort)
                self.qto.columns.sort_by_key(|x| QtyColumn::GEOMETRIC.iter().position(|g| g == x).unwrap_or(100));
            }
        }
        ui.separator();

        // --- 5D cost: price catalog + priced take-off + budget vs. actual
        let mut act: Option<Purpose> = None;
        egui::CollapsingHeader::new("💰 5D cost — prices, budget and variance")
            .default_open(self.cost.catalog.is_some())
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("📂 Price catalog…").on_hover_text("Excel / CSV with Code, Description, Unit, Unit price and (optional) Budget columns").clicked() {
                        act = Some(Purpose::ImportPriceCatalog);
                    }
                    if let Some(c) = &self.cost.catalog {
                        ui.label(RichText::new(format!("{} · {} items", c.name, c.items.len())).weak());
                        if ui.button("📊 Excel…").on_hover_text("Priced take-off: quantity × unit price, with budget and variance").clicked() {
                            act = Some(Purpose::ExportCostXlsx);
                        }
                        if ui.button("CSV…").clicked() {
                            act = Some(Purpose::ExportCostCsv);
                        }
                    }
                });
                if self.cost.catalog.is_none() {
                    ui.label(RichText::new("Load a price catalog to price the quantities and compare them with the budget.").weak());
                    return;
                }
                let titles: Vec<String> = self.qto.columns.iter().map(|c| c.title()).collect();
                let cur = self.cost.column.min(titles.len().saturating_sub(1));
                ui.horizontal_wrapped(|ui| {
                    ui.label("Quantity:");
                    let mut pick = cur;
                    egui::ComboBox::from_id_salt("cost_col").selected_text(titles.get(cur).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                        for (i, t) in titles.iter().enumerate() {
                            ui.selectable_value(&mut pick, i, t);
                        }
                    });
                    if pick != cur {
                        self.cost.column = pick;
                    }
                });
                if let Some(p) = self.cost_update() {
                    let cur = p.currency.clone();
                    let money = |v: f64| if cur.is_empty() { format!("{v:.2}") } else { format!("{v:.2} {cur}") };
                    ui.label(RichText::new(format!(
                        "Total {} · budget {} · variance {} · {} priced, {} without a price",
                        money(p.amount_total),
                        money(p.budget_total),
                        money(p.variance_total),
                        p.matched,
                        p.unmatched
                    )).strong());
                    egui::ScrollArea::both().id_salt("cost_table").max_height(220.0).auto_shrink([false, true]).show(ui, |ui| {
                        egui::Grid::new("cost_grid").striped(true).min_col_width(60.0).show(ui, |ui| {
                            for h in ["Group", "Code", "Description", "Quantity", "Unit price", "Amount", "Budget", "Variance"] {
                                ui.label(RichText::new(h).strong());
                            }
                            ui.end_row();
                            for row in &p.rows {
                                ui.label(row.keys.join(" / "));
                                ui.label(&row.code);
                                ui.label(&row.description);
                                ui.label(match row.quantity {
                                    Some(q) => format!("{q:.3} {}", row.unit),
                                    None => "–".into(),
                                });
                                ui.label(row.price.map(|v| format!("{v:.2}")).unwrap_or_else(|| "–".into()));
                                ui.label(row.amount.map(|v| format!("{v:.2}")).unwrap_or_else(|| "–".into()));
                                ui.label(row.budget.map(|v| format!("{v:.2}")).unwrap_or_else(|| "–".into()));
                                ui.label(row.variance.map(|v| format!("{v:+.2}")).unwrap_or_else(|| "–".into()));
                                ui.end_row();
                            }
                        });
                    });
                }
            });
        if let Some(p) = act {
            self.request_path(p);
        }

        // --- the table (recalculated automatically on any change)
        self.qto_update();
        let Some(r) = &self.qto_result else { return };
        if r.count == 0 {
            ui.label(RichText::new(if self.qto.scope == QtoScope::Selection { "Nothing selected." } else { "No items." }).weak());
            return;
        }
        let nk = r.setup.group_by.len();
        let ncol = r.setup.columns.len();
        let ctrl = ui.input(|i| i.modifiers.command);
        let mut select: Option<(Vec<ElemId>, bool)> = None;
        egui::ScrollArea::both().id_salt("qto_table").auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("qto_grid").striped(true).num_columns(nk.max(1) + ncol).min_col_width(60.0).show(ui, |ui| {
                if nk == 0 {
                    ui.label(RichText::new("Group").strong());
                }
                for g in &r.setup.group_by {
                    ui.label(RichText::new(&g.prop).strong());
                }
                for i in 0..ncol {
                    ui.label(RichText::new(r.column_title(i)).strong()).on_hover_text(r.setup.columns[i].hint());
                }
                ui.end_row();
                for row in &r.rows {
                    if nk == 0
                        && ui.selectable_label(false, "All").clicked() {
                            select = Some((row.ids.clone(), ctrl));
                        }
                    for key in &row.keys {
                        let resp = ui.selectable_label(false, key).on_hover_text("Click: select these items · Ctrl+click: add to the selection");
                        if resp.clicked() {
                            select = Some((row.ids.clone(), ctrl));
                        }
                    }
                    for (ci, v) in row.values.iter().enumerate() {
                        let t = fmt(*v, &r.setup.columns[ci]);
                        let resp = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(t)).inner;
                        if row.missing[ci] > 0 && v.is_some() {
                            resp.on_hover_text(format!("{} of {} items have no value here", row.missing[ci], row.ids.len()));
                        }
                    }
                    ui.end_row();
                }
                ui.label(RichText::new("TOTAL").strong());
                for _ in 1..nk.max(1) {
                    ui.label("");
                }
                for (ci, v) in r.totals.iter().enumerate() {
                    let t = fmt(*v, &r.setup.columns[ci]);
                    let resp = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(t).strong())).inner;
                    if r.missing[ci] > 0 {
                        resp.on_hover_text(format!("{} items have no value for this column", r.missing[ci]));
                    }
                }
                ui.end_row();
            });
        });
        if let Some((ids, add)) = select {
            self.apply(Action::Select(ids, add));
        }
    }

    /// Model Checker: rules (own or from IDS) on the left, the details of the chosen rule + the failing elements on the right.
    pub(super) fn check_ui(&mut self, ui: &mut egui::Ui) {
        use crate::check::{Card, Facet, Kind, Rule, Value};
        let mut acts: Vec<Action> = Vec::new();
        let mut run = false;
        ui.horizontal_wrapped(|ui| {
            run = ui.add(egui::Button::new(RichText::new("▶ Run check").strong().color(Color32::WHITE)).fill(accent())).on_hover_text("Check every loaded model against the active rules").clicked();
            if ui.button("➕ New rule").on_hover_text("A rule: which elements it applies to + what they must have").clicked() {
                let mut r = Rule::new(&format!("Rule {}", self.check_rules.len() + 1));
                r.requirements.push(Facet::property("", "", Value::Any));
                self.check_rules.push(r);
                self.check_sel = Some(self.check_rules.len() - 1);
                self.check_results = None;
            }
            if ui.button("⭐ Starter rules").on_hover_text("Add a few common rules (names, fire rating of walls, IsExternal of doors) to start from").clicked() {
                self.check_rules.extend(crate::check::starter_rules());
                self.check_results = None;
            }
            if ui.button("📥 Import IDS…").on_hover_text("buildingSMART IDS 1.0 (.ids) - or a saved ManageBIM rules file (.mbrules)").clicked() {
                self.request_path(Purpose::ImportCheckRules);
            }
            if ui.add_enabled(!self.check_rules.is_empty(), egui::Button::new("💾 Save rules…")).on_hover_text("Save the rules (.mbrules) to reuse them on other projects. The rules are also saved with the project.").clicked() {
                self.request_path(Purpose::ExportCheckRules);
            }
            if ui.add_enabled(!self.check_rules.is_empty(), egui::Button::new("📤 Export IDS…")).on_hover_text("Save the rules as a buildingSMART IDS 1.0 file - for Solibri, BIMcollab, usBIM, ifctester or the design team").clicked() {
                self.request_path(Purpose::ExportIds);
            }
            ui.separator();
            let has = self.check_results.is_some();
            if ui.add_enabled(has, egui::Button::new("📊 Excel…")).on_hover_text("Report: summary per rule + every failing element with the reason").clicked() {
                self.request_path(Purpose::ExportCheckXlsx);
            }
            if ui.add_enabled(has, egui::Button::new("📋 BCF…")).on_hover_text("One BCF issue per failing rule, with the failing elements selected - for Revit, ACC, Solibri, BIMcollab").clicked() {
                self.request_path(Purpose::ExportCheckBcf);
            }
            if ui.add_enabled(!self.check_rules.is_empty(), egui::Button::new("🗑 Clear")).on_hover_text("Remove all rules and results").clicked() {
                self.check_rules.clear();
                self.check_results = None;
                self.check_sel = None;
                self.check_warn.clear();
            }
        });
        if run {
            self.run_check();
        }
        // ---- quick checks, without rules
        ui.horizontal(|ui| {
            if ui.button(RichText::new("⚡ Quick checks").strong()).on_hover_text("No IDS needed: elements without level, name, classification, material or geometry, duplicates, duplicate GUIDs").clicked() {
                self.run_quick_checks();
            }
            if ui.add_enabled(!self.quick_checks.is_empty(), egui::Button::new("📊 CSV…")).on_hover_text("Every element found by the quick checks, with the reason").clicked() {
                self.request_path(Purpose::ExportQuickChecksCsv);
            }
            if !self.quick_checks.is_empty() && ui.button("✖").on_hover_text("Clear the quick check results").clicked() {
                self.quick_checks.clear();
            }
        });
        if !self.quick_checks.is_empty() {
            let mut sel: Option<Vec<ElemId>> = None;
            egui::Grid::new("quick_checks").striped(true).num_columns(3).show(ui, |ui| {
                for q in &self.quick_checks {
                    let n = q.result.failed.len();
                    let (txt, col) = if n == 0 { ("✔", Color32::from_rgb(70, 190, 110)) } else { ("✖", Color32::from_rgb(235, 85, 85)) };
                    ui.label(RichText::new(txt).color(col));
                    ui.label(q.name).on_hover_text(q.about);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{n} / {}", q.result.checked)).strong());
                        if n > 0 && ui.small_button("Select").clicked() {
                            let mut ids: Vec<ElemId> = q.result.failed.iter().map(|f| f.0).collect();
                            ids.dedup();
                            sel = Some(ids);
                        }
                    });
                    ui.end_row();
                }
            });
            if let Some(ids) = sel {
                self.apply(Action::Select(ids, false));
            }
            ui.separator();
        }
        if self.check_rules.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new("No rules yet. Import a buildingSMART IDS file, add the ⭐ starter rules or create a ➕ New rule (e.g. \"every IfcDoor has Pset_DoorCommon.FireRating\"), then press ▶ Run check.").weak());
            return;
        }
        // summary
        if let Some(res) = &self.check_results {
            let active: Vec<usize> = (0..self.check_rules.len()).filter(|i| self.check_rules[*i].enabled).collect();
            let fail = active.iter().filter(|i| res.get(**i).map(|r| !r.failed.is_empty()).unwrap_or(false)).count();
            let na = active.iter().filter(|i| res.get(**i).map(|r| r.checked == 0).unwrap_or(true)).count();
            let bad = self.check_failed_ids(None).len();
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("✔ {} pass", active.len() - fail - na)).color(Color32::from_rgb(70, 190, 110)).strong());
                ui.label(RichText::new(format!("✖ {fail} fail")).color(Color32::from_rgb(235, 85, 85)).strong());
                if na > 0 {
                    ui.label(RichText::new(format!("○ {na} not applicable")).weak());
                }
                ui.label(RichText::new(format!("· {bad} elements with problems")).weak());
                if bad > 0 {
                    if ui.small_button("Select all").clicked() {
                        acts.push(Action::Select(self.check_failed_ids(None), false));
                    }
                    if ui.small_button("Isolate all").clicked() {
                        acts.push(Action::Isolate(self.check_failed_ids(None)));
                    }
                }
            });
        }
        if !self.check_warn.is_empty() {
            egui::CollapsingHeader::new(RichText::new(format!("⚠ {} IDS conditions not supported", self.check_warn.len())).small()).id_salt("check_warn").show(ui, |ui| {
                for w in &self.check_warn {
                    ui.label(RichText::new(w).small().weak());
                }
            });
        }
        ui.separator();
        let results = self.check_results.clone();
        let mut remove: Option<usize> = None;
        let sel_now = self.check_sel;
        let mut new_sel: Option<usize> = None;
        let mut dirty = false;
        ui.columns(2, |cols| {
            // ---------------- the list of rules
            egui::ScrollArea::vertical().id_salt("check_rules").auto_shrink([false, false]).show(&mut cols[0], |ui| {
                for i in 0..self.check_rules.len() {
                    let r = &mut self.check_rules[i];
                    let res = results.as_ref().and_then(|v| v.get(i));
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut r.enabled, "").on_hover_text("Active").changed() {
                            dirty = true;
                        }
                        let (icon, col) = match res {
                            _ if !r.enabled => ("○", ui.visuals().weak_text_color()),
                            None => ("•", ui.visuals().weak_text_color()),
                            Some(x) if x.checked == 0 => ("○", ui.visuals().weak_text_color()),
                            Some(x) if x.failed.is_empty() => ("✔", Color32::from_rgb(70, 190, 110)),
                            Some(_) => ("✖", Color32::from_rgb(235, 85, 85)),
                        };
                        ui.label(RichText::new(icon).color(col).strong());
                        let txt = match res {
                            Some(x) if r.enabled => format!("{}   {}/{}", r.name, x.failed.len(), x.checked),
                            _ => r.name.clone(),
                        };
                        let resp = ui.selectable_label(sel_now == Some(i), txt).on_hover_text(format!("Applies to: {}\nRequires: {}{}", r.applies_text(), r.requirements.iter().map(|f| f.describe()).collect::<Vec<_>>().join("; "), if r.source.is_empty() { String::new() } else { format!("\nFrom: {}", r.source) }));
                        if resp.clicked() {
                            new_sel = Some(i);
                        }
                        if resp.double_clicked() {
                            let ids: Vec<ElemId> = res.map(|x| x.failed.iter().map(|f| f.0).collect()).unwrap_or_default();
                            if !ids.is_empty() {
                                acts.push(Action::Select(ids.clone(), false));
                                acts.push(Action::Focus(ids));
                            }
                        }
                    });
                }
            });
            // ---------------- the chosen rule
            let ui = &mut cols[1];
            let Some(si) = self.check_sel.filter(|i| *i < self.check_rules.len()) else {
                ui.label(RichText::new("Click a rule to see its failing elements and edit it.").weak());
                return;
            };
            egui::ScrollArea::vertical().id_salt("check_detail").auto_shrink([false, false]).show(ui, |ui| {
                let r = &mut self.check_rules[si];
                ui.horizontal(|ui| {
                    dirty |= ui.add(egui::TextEdit::singleline(&mut r.name).desired_width(ui.available_width() - 70.0).font(egui::TextStyle::Heading)).changed();
                    if ui.small_button("🗑").on_hover_text("Delete this rule").clicked() {
                        remove = Some(si);
                    }
                    if ui.small_button("⧉").on_hover_text("Duplicate this rule").clicked() {
                        let mut c = r.clone();
                        c.name.push_str(" (copy)");
                        c.source.clear();
                        // added after the loop
                        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("check_dup"), serde_json::to_string(&c).unwrap_or_default()));
                    }
                });
                if !r.description.is_empty() {
                    ui.label(RichText::new(&r.description).small().weak());
                }
                // failures first: this is what people look at
                if let Some(x) = results.as_ref().and_then(|v| v.get(si)) {
                    if r.enabled {
                        ui.add_space(4.0);
                        if x.checked == 0 {
                            ui.label(RichText::new("No element matches \"Applies to\" - nothing was checked.").weak());
                        } else if x.failed.is_empty() {
                            ui.label(RichText::new(format!("✔ All {} elements pass.", x.checked)).color(Color32::from_rgb(70, 190, 110)));
                        } else {
                            let ids: Vec<ElemId> = x.failed.iter().map(|f| f.0).collect();
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("✖ {} of {} elements fail", x.failed.len(), x.checked)).color(Color32::from_rgb(235, 85, 85)).strong());
                                if ui.small_button("Select").clicked() {
                                    acts.push(Action::Select(ids.clone(), false));
                                }
                                if ui.small_button("Isolate").clicked() {
                                    acts.push(Action::Isolate(ids.clone()));
                                }
                                if ui.small_button("Focus").clicked() {
                                    acts.push(Action::Focus(ids.clone()));
                                }
                            });
                            let rows = x.failed.len().min(5000);
                            egui::ScrollArea::vertical().id_salt(("check_fail", si)).max_height(220.0).auto_shrink([false, true]).show_rows(ui, 18.0, rows, |ui, range| {
                                for (id, why) in &x.failed[range] {
                                    let (name, ty) = self.elem(*id).map(|(_, e)| (e.name.clone(), e.ifc_type.clone())).unwrap_or_default();
                                    ui.horizontal(|ui| {
                                        let resp = ui.selectable_label(self.selected.contains(id), RichText::new(&name).strong()).on_hover_text("Click: select · double-click: zoom to it");
                                        if resp.clicked() {
                                            acts.push(Action::Select(vec![*id], ui.input(|i| i.modifiers.ctrl)));
                                        }
                                        if resp.double_clicked() {
                                            acts.push(Action::Focus(vec![*id]));
                                        }
                                        ui.label(RichText::new(ty).small().color(accent()));
                                        ui.label(RichText::new(why).small().weak());
                                    });
                                }
                            });
                        }
                    }
                }
                ui.add_space(6.0);
                let r = &mut self.check_rules[si];
                ui.label(RichText::new("APPLIES TO").small().strong().weak()).on_hover_text("All conditions must be true for an element to be checked. Empty = every element.");
                let mut del = None;
                for (k, f) in r.applicability.iter_mut().enumerate() {
                    let (ch, rm) = facet_editor(ui, f, ("ap", si, k), false);
                    dirty |= ch;
                    if rm {
                        del = Some(k);
                    }
                }
                if let Some(k) = del {
                    r.applicability.remove(k);
                    dirty = true;
                }
                if ui.small_button("➕ condition").clicked() {
                    r.applicability.push(Facet { kind: Kind::Entity, ..Default::default() });
                    dirty = true;
                }
                ui.add_space(6.0);
                ui.label(RichText::new("REQUIREMENTS").small().strong().weak()).on_hover_text("Every requirement must be met, otherwise the element fails.");
                let mut del = None;
                for (k, f) in r.requirements.iter_mut().enumerate() {
                    let (ch, rm) = facet_editor(ui, f, ("rq", si, k), true);
                    dirty |= ch;
                    if rm {
                        del = Some(k);
                    }
                }
                if let Some(k) = del {
                    r.requirements.remove(k);
                    dirty = true;
                }
                if ui.small_button("➕ requirement").clicked() {
                    r.requirements.push(Facet { kind: Kind::Property, card: Card::Required, ..Default::default() });
                    dirty = true;
                }
            });
        });
        if new_sel.is_some() {
            self.check_sel = new_sel;
        }
        if let Some(s) = ui.ctx().data_mut(|d| d.remove_temp::<String>(egui::Id::new("check_dup"))) {
            if let Ok(r) = serde_json::from_str::<Rule>(&s) {
                self.check_rules.push(r);
                self.check_sel = Some(self.check_rules.len() - 1);
                dirty = true;
            }
        }
        if let Some(i) = remove {
            self.check_rules.remove(i);
            self.check_sel = None;
            dirty = true;
        }
        if dirty {
            // the old results no longer match the rules
            self.check_results = None;
        }
        for a in acts {
            self.apply(a);
        }
    }

    pub(super) fn compare_ui(&mut self, ui: &mut egui::Ui) {
        use crate::compare::Diff;
        let models: Vec<(usize, String)> = self.loaded_models().map(|(i, m)| (i, m.name.clone())).collect();
        if models.len() < 2 {
            ui.label(RichText::new("Load both versions of the model (Open the old one, then ➕ Append the new one) to compare them.").weak());
            return;
        }
        // default: A = the first model, B = the last
        let ok = |s: Option<usize>| s.filter(|i| models.iter().any(|m| m.0 == *i));
        let a = ok(self.compare_pick.0).unwrap_or(models[0].0);
        let b = ok(self.compare_pick.1).unwrap_or(models[models.len() - 1].0);
        self.compare_pick = (Some(a), Some(b));
        let name = |i: usize| models.iter().find(|m| m.0 == i).map(|m| m.1.clone()).unwrap_or_default();
        let mut run = false;
        ui.horizontal_wrapped(|ui| {
            for (lbl, id, slot) in [("A (old)", "cmp_a", 0), ("B (new)", "cmp_b", 1)] {
                ui.label(lbl);
                let cur = if slot == 0 { a } else { b };
                egui::ComboBox::from_id_salt(id).width(220.0).selected_text(name(cur)).show_ui(ui, |ui| {
                    for (i, n) in &models {
                        if ui.selectable_label(*i == cur, n).clicked() {
                            if slot == 0 {
                                self.compare_pick.0 = Some(*i);
                            } else {
                                self.compare_pick.1 = Some(*i);
                            }
                        }
                    }
                });
            }
            if ui.button("⇅").on_hover_text("Swap A and B").clicked() {
                self.compare_pick = (Some(b), Some(a));
            }
            ui.label("Tolerance");
            ui.add(egui::DragValue::new(&mut self.compare_tol).range(0.0..=1000.0).speed(0.5).suffix(" mm"));
            run = ui.add_enabled(a != b, egui::Button::new(RichText::new("▶ Compare").strong())).on_hover_text("Match elements by GUID and compare geometry and properties").clicked();
            ui.separator();
            if ui.button("📂 Open…").on_hover_text("Open a saved comparison (.bimcmp) on these two versions").clicked() {
                self.request_path(Purpose::ImportCompare);
            }
            let has = self.compare.is_some();
            if ui.add_enabled(has, egui::Button::new("💾 Save…")).on_hover_text("Save the comparison (.bimcmp) to open it again later or send it to the team").clicked() {
                self.request_path(Purpose::ExportCompare);
            }
            if ui.add_enabled(has, egui::Button::new("📊 CSV report…")).on_hover_text("Every changed element with its status and changes - opens in Excel").clicked() {
                self.request_path(Purpose::ExportCompareCsv);
            }
        });
        if run {
            self.run_compare();
        }
        let Some(r) = &self.compare else {
            ui.label(RichText::new("Elements are matched by IFC GlobalId. Geometry counts as changed when its bounding box moves more than the tolerance or its triangle count changes.").weak());
            return;
        };
        let counts = r.counts;
        let lists: Vec<(Diff, Vec<ElemId>)> = Diff::ALL.iter().map(|d| (*d, r.ids(*d))).collect();
        let changes: Vec<(ElemId, String, Vec<String>)> = r.changes.iter().take(2000).map(|(id, v)| (*id, self.elem(*id).map(|x| x.1.name.clone()).unwrap_or_default(), v.clone())).collect();
        let n_changes = r.changes.len();
        let mut acts: Vec<Action> = Vec::new();
        let mut new_set: Option<(String, Vec<ElemId>)> = None;
        let mut acts2: Vec<Action> = Vec::new();
        let mut dirty = false;
        let ndev = r.dev.len();
        let auto_max = {
            // automatic scale: the largest deviation (95%) among the changed elements, minimum 10 mm
            let mut v: Vec<f32> = r.dev.values().map(|d| d.max).collect();
            v.sort_by(|a, b| a.total_cmp(b));
            v.get(((v.len() as f32 * 0.95) as usize).min(v.len().saturating_sub(1))).copied().unwrap_or(0.1).max(0.01)
        };
        let mut heat_changed = false;
        ui.horizontal_wrapped(|ui| {
            dirty |= ui.checkbox(&mut self.compare_on, RichText::new("Show results in 3D").strong()).changed();
            heat_changed |= ui
                .add_enabled(ndev > 0, egui::Checkbox::new(&mut self.compare_heat, RichText::new("Deviation heatmap").strong()))
                .on_hover_text("Changed elements coloured by how far each surface moved between A and B: green = in place, yellow = half the scale, red = at or beyond the scale")
                .changed();
            if self.compare_heat && ndev > 0 {
                let mut auto = self.compare_heat_max_mm <= 0.0;
                if ui.checkbox(&mut auto, "Auto scale").changed() {
                    self.compare_heat_max_mm = if auto { 0.0 } else { (auto_max * 1000.0).round() };
                    heat_changed = true;
                }
                if !auto {
                    heat_changed |= ui.add(egui::DragValue::new(&mut self.compare_heat_max_mm).range(1.0..=100_000.0).speed(5.0).suffix(" mm")).changed();
                }
                // legend
                let scale = if auto { auto_max } else { self.compare_heat_max_mm / 1000.0 };
                let (rect, _) = ui.allocate_exact_size(egui::vec2(140.0, 14.0), egui::Sense::hover());
                for k in 0..28 {
                    let t = k as f32 / 27.0;
                    let c = crate::compare::heat_color(t);
                    let x0 = rect.left() + rect.width() * k as f32 / 28.0;
                    ui.painter().rect_filled(egui::Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x0 + rect.width() / 28.0 + 0.5, rect.bottom())), 0.0, Color32::from_rgb(c[0], c[1], c[2]));
                }
                ui.label(RichText::new(format!("0 … {:.0} mm", scale * 1000.0)).small());
            }
            if ui.button("Clear").clicked() {
                acts.clear();
                self.compare_clear();
            }
        });
        if heat_changed {
            // re-sends all the colours (the map is applied / removed)
            self.looks.clear();
            dirty = true;
        }
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            egui::Grid::new("cmp_sum").striped(true).num_columns(4).show(ui, |ui| {
                for (k, (d, ids)) in lists.iter().enumerate() {
                    let c = d.color();
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 3.0, Color32::from_rgb(c[0], c[1], c[2]));
                    ui.label(d.label());
                    ui.label(RichText::new(counts[k].to_string()).strong());
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!ids.is_empty(), egui::Button::new("Select").small()).clicked() {
                            acts.push(Action::Select(ids.clone(), false));
                        }
                        if ui.add_enabled(!ids.is_empty() && *d != Diff::Same, egui::Button::new("Save as set").small()).clicked() {
                            new_set = Some((format!("Compare: {}", d.label()), ids.clone()));
                        }
                    });
                    ui.end_row();
                }
            });
            let ui = &mut cols[1];
            ui.label(RichText::new(format!("Changes ({n_changes})")).strong());
            egui::ScrollArea::vertical().id_salt("cmp_changes").auto_shrink([false, false]).show(ui, |ui| {
                for (id, nm, why) in &changes {
                    let first = why.first().cloned().unwrap_or_default();
                    let r = ui.selectable_label(self.selected.contains(id), format!("{nm}  —  {first}")).on_hover_text(why.join("\n"));
                    if r.clicked() {
                        acts.push(Action::Select(vec![*id], false));
                        acts.push(Action::Focus(vec![*id]));
                    }
                }
                if n_changes > changes.len() {
                    ui.label(RichText::new(format!("… {} more (use Select / Save as set)", n_changes - changes.len())).weak());
                }
            });
        });
        // ---- compare on parameters: which property changed, on how many elements, old -> new
        if !self.compare_params.is_empty() {
            let summary = crate::compare::param_summary(&self.compare_params);
            ui.add_space(6.0);
            egui::CollapsingHeader::new(RichText::new(format!("Changed parameters ({})", summary.len())).strong()).id_salt("cmp_params").default_open(true).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Click a parameter: its elements are selected and the old → new values listed.").weak().small());
                    if ui.button("📊 Parameter CSV…").on_hover_text("One row per element and changed parameter: GUID, element, type, parameter, old value, new value").clicked() {
                        self.request_path(Purpose::ExportCompareParamsCsv);
                    }
                });
                ui.columns(2, |cols| {
                    egui::ScrollArea::vertical().id_salt("cmp_plist").max_height(220.0).auto_shrink([false, true]).show(&mut cols[0], |ui| {
                        for (p, n) in summary.iter().take(500) {
                            let on = self.compare_param_pick.as_deref() == Some(p.as_str());
                            if ui.selectable_label(on, format!("{p}   ({n})")).clicked() {
                                self.compare_param_pick = if on { None } else { Some(p.clone()) };
                                if !on {
                                    let mut ids: Vec<ElemId> = self.compare_params.iter().filter(|c| &c.param == p).map(|c| c.id).collect();
                                    ids.dedup();
                                    acts2.push(Action::Select(ids, false));
                                }
                            }
                        }
                    });
                    let ui = &mut cols[1];
                    if let Some(p) = self.compare_param_pick.clone() {
                        let rows: Vec<(ElemId, String, String)> = self.compare_params.iter().filter(|c| c.param == p).take(1000).map(|c| (c.id, c.old.clone(), c.new.clone())).collect();
                        egui::ScrollArea::vertical().id_salt("cmp_pvals").max_height(220.0).auto_shrink([false, true]).show(ui, |ui| {
                            for (id, o, n) in rows {
                                let nm = self.elem(id).map(|x| x.1.name.clone()).unwrap_or_default();
                                let o = if o.is_empty() { "(none)".to_string() } else { o };
                                let n = if n.is_empty() { "(removed)".to_string() } else { n };
                                if ui.selectable_label(self.selected.contains(&id), format!("{nm}:  {o}  →  {n}")).clicked() {
                                    acts2.push(Action::Select(vec![id], false));
                                    acts2.push(Action::Focus(vec![id]));
                                }
                            }
                        });
                    }
                });
            });
        }
        for a in acts2 {
            self.apply(a);
        }
        for a in acts {
            self.apply(a);
        }
        if let Some((n, ids)) = new_set {
            self.sets.push(SelSet { name: n.clone(), kind: SetKind::Explicit(ids) });
            self.toast(format!("Set created: {n}"));
        }
        if dirty {
            self.colors_dirty = true;
        }
    }

    pub(super) fn describe_rules(rules: &[SearchRule]) -> String {
        let mut s = String::new();
        for (i, r) in rules.iter().enumerate() {
            if i > 0 {
                s.push_str(if r.or { "\nOR  " } else { "\nAND " });
            }
            let cat = if r.group.is_empty() { String::new() } else { format!("{} / ", r.group) };
            let v = if r.op.needs_value() { format!(" {}", r.value) } else { String::new() };
            s.push_str(&format!("{cat}{} {}{v}", r.field, r.op.label()));
        }
        s
    }

    /// Navisworks-style condition rows: Category | Property | Condition | Value.
    /// Returns true when Enter was pressed in a value field.
    pub(super) fn rule_rows(ui: &mut egui::Ui, salt: &str, rules: &mut Vec<SearchRule>, idx: &PropIndex, logic: bool) -> bool {
        let mut del = None;
        let mut enter = false;
        let mut add_or: Vec<SearchRule> = Vec::new();
        for (i, r) in rules.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if logic {
                    if i == 0 {
                        ui.add_sized([40.0, 20.0], egui::Label::new(RichText::new("").small()));
                    } else {
                        let t = if r.or { "OR" } else { "AND" };
                        if ui.add_sized([40.0, 20.0], egui::Button::new(RichText::new(t).small().strong())).on_hover_text("Switch AND / OR (OR starts an alternative condition, like Navisworks \"Or Condition\")").clicked() {
                            r.or = !r.or;
                        }
                    }
                }
                // Category
                let cat_txt = if r.group.is_empty() { "(any category)".to_string() } else { r.group.clone() };
                egui::ComboBox::from_id_salt((salt, "c", i)).width(150.0).selected_text(cat_txt).show_ui(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                        if ui.selectable_label(r.group.is_empty() && r.field == "Any", "(any category)").clicked() {
                            r.group.clear();
                            r.field = "Any".into();
                        }
                        for (c, ps) in &idx.cats {
                            if ui.selectable_label(&r.group == c, format!("{c}  ({})", ps.len())).clicked() && &r.group != c {
                                r.group = c.clone();
                                r.field = ps.first().map(|p| p.name.clone()).unwrap_or_else(|| "(any)".into());
                            }
                        }
                    });
                });
                // Property
                if r.group.is_empty() {
                    let t = if r.field == "Any" { "(any property)".to_string() } else { r.field.clone() };
                    ui.add_enabled_ui(false, |ui| {
                        egui::ComboBox::from_id_salt((salt, "p", i)).width(170.0).selected_text(t).show_ui(ui, |_| {});
                    });
                } else {
                    egui::ComboBox::from_id_salt((salt, "p", i)).width(170.0).selected_text(r.field.clone()).show_ui(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                            ui.selectable_value(&mut r.field, "(any)".to_string(), "(any)");
                            if let Some(ps) = idx.props(&r.group) {
                                for p in ps {
                                    ui.selectable_value(&mut r.field, p.name.clone(), &p.name);
                                }
                            }
                        });
                    });
                }
                // Condition
                egui::ComboBox::from_id_salt((salt, "o", i)).width(110.0).selected_text(r.op.label()).show_ui(ui, |ui| {
                    for o in SearchOp::ALL {
                        ui.selectable_value(&mut r.op, o, o.label());
                    }
                });
                // Value + list of existing values
                if r.op.needs_value() {
                    let resp = ui.add(egui::TextEdit::singleline(&mut r.value).desired_width(150.0).hint_text("value"));
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        enter = true;
                    }
                    if let Some(pv) = idx.prop(&r.group, &r.field) {
                        ui.menu_button("⏷", |ui| {
                            let fid = egui::Id::new((salt, "vf", i));
                            let mut f: String = ui.data_mut(|d| d.get_temp(fid)).unwrap_or_default();
                            ui.set_min_width(280.0);
                            if pv.values.is_empty() {
                                ui.label(RichText::new(format!("{} distinct values - type the value or use 'contains'.", pv.distinct)).weak());
                                return;
                            }
                            ui.add(egui::TextEdit::singleline(&mut f).hint_text("filter values").desired_width(260.0));
                            ui.data_mut(|d| d.insert_temp(fid, f.clone()));
                            ui.label(RichText::new("Click = use value · OR = add as an alternative row").weak().small());
                            let fl = f.to_lowercase();
                            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                                for (v, n) in pv.values.iter().filter(|(v, _)| fl.is_empty() || v.to_lowercase().contains(&fl)).take(1000) {
                                    ui.horizontal(|ui| {
                                        if ui.selectable_label(&r.value == v, format!("{v}   ({n})")).clicked() {
                                            r.value = v.clone();
                                            if !matches!(r.op, SearchOp::Equals | SearchOp::NotEquals | SearchOp::Contains | SearchOp::NotContains) {
                                                r.op = SearchOp::Equals;
                                            }
                                            ui.close_menu();
                                        }
                                        if logic && ui.small_button("OR").on_hover_text("Add an OR row with this value").clicked() {
                                            add_or.push(SearchRule::new(&r.group, &r.field, SearchOp::Equals, v, true));
                                        }
                                    });
                                }
                            });
                        });
                    }
                }
                if ui.small_button("✖").on_hover_text("Remove condition").clicked() {
                    del = Some(i);
                }
            });
        }
        if let Some(i) = del {
            rules.remove(i);
        }
        // an empty row is replaced directly, otherwise it is added as OR
        for r in add_or {
            if let Some(last) = rules.last_mut() {
                if last.group == r.group && last.field == r.field && last.value.trim().is_empty() {
                    last.value = r.value;
                    last.op = SearchOp::Equals;
                    continue;
                }
            }
            rules.push(r);
        }
        enter
    }
}
