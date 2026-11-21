//! Sectiune din `impl App` (vezi fisierul parinte pentru restul).
use super::*;

impl App {
    pub(super) fn tree_ui(&self, ui: &mut egui::Ui) -> Vec<Action> {
        let mut acts = Vec::new();
        let ctrl = ui.input(|i| i.modifiers.command);
        if self.loaded_models().next().is_none() {
            ui.label(RichText::new("No model loaded.").weak());
        }
        let eye_btn = |ui: &mut egui::Ui, ids: &[ElemId], acts: &mut Vec<Action>| {
            let all_hidden = !ids.is_empty() && ids.iter().all(|i| self.hidden.contains(i));
            let t = if all_hidden { RichText::new(egui_phosphor::regular::EYE_SLASH).font(crate::theme::icon_font(13.0)).weak() } else { RichText::new(egui_phosphor::regular::EYE).font(crate::theme::icon_font(13.0)).color(ui.visuals().weak_text_color()) };
            if ui.add(egui::Button::new(t).small().frame(false)).on_hover_text(if all_hidden { "Unhide" } else { "Hide" }).clicked() {
                if all_hidden {
                    acts.push(Action::Show(ids.to_vec()));
                } else {
                    acts.push(Action::Hide(ids.to_vec()));
                }
            }
        };
        let group_menu = |r: &egui::Response, ids: &Vec<ElemId>, acts: &mut Vec<Action>| {
            r.context_menu(|ui| {
                if ui.button("Select").clicked() {
                    acts.push(Action::Select(ids.clone(), false));
                    ui.close_menu();
                }
                if ui.button("Hide").clicked() {
                    acts.push(Action::Hide(ids.clone()));
                    ui.close_menu();
                }
                if ui.button("Hide Unselected").clicked() {
                    acts.push(Action::Isolate(ids.clone()));
                    ui.close_menu();
                }
                if ui.button("Unhide").clicked() {
                    acts.push(Action::Show(ids.clone()));
                    ui.close_menu();
                }
                if ui.button("Focus").clicked() {
                    acts.push(Action::Focus(ids.clone()));
                    ui.close_menu();
                }
            });
        };
        for (mi, m) in self.loaded_models() {
            let Some(tree) = self.trees.get(mi).and_then(|t| t.as_ref()) else { continue };
            let mids: Vec<ElemId> = (0..m.elements.len() as u32).map(|e| ElemId { m: mi as u32, e }).collect();
            let id = ui.make_persistent_id(("model", mi));
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                .show_header(ui, |ui| {
                    eye_btn(ui, &mids, &mut acts);
                    // punct colorat pe model (ca disciplinele: arhitectura, structura, instalatii...)
                    let mc = crate::profiler::palette(mi);
                    let (dr, _) = ui.allocate_exact_size(EV::splat(10.0), Sense::hover());
                    ui.painter().circle_filled(dr.center(), 4.5, Color32::from_rgb(mc[0], mc[1], mc[2]));
                    let r = ui.selectable_label(false, RichText::new(&m.name).strong());
                    if r.clicked() {
                        acts.push(Action::Select(mids.clone(), ctrl));
                    }
                    r.context_menu(|ui| {
                        if ui.button("Select").clicked() {
                            acts.push(Action::Select(mids.clone(), false));
                            ui.close_menu();
                        }
                        if ui.button("Hide").clicked() {
                            acts.push(Action::Hide(mids.clone()));
                            ui.close_menu();
                        }
                        if ui.button("Hide Unselected").clicked() {
                            acts.push(Action::Isolate(mids.clone()));
                            ui.close_menu();
                        }
                        if ui.button("Unhide").clicked() {
                            acts.push(Action::Show(mids.clone()));
                            ui.close_menu();
                        }
                        if ui.button("Focus").clicked() {
                            acts.push(Action::Focus(mids.clone()));
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("🗑 Remove model from project").on_hover_text("Closes this model in ManageBIM. The file on disk is not touched.").clicked() {
                            acts.push(Action::RemoveModel(mi));
                            ui.close_menu();
                        }
                    });
                })
                .body(|ui| {
                    let single = tree.roots.len() == 1;
                    if tree.roots.is_empty() {
                        // fara niveluri de grupare: elementele direct sub model
                        Self::tree_elems(self, ui, mi, m, &mids, ctrl, &eye_btn, &group_menu, &mut acts);
                    }
                    for (k, node) in tree.roots.iter().enumerate() {
                        let key = ui.make_persistent_id(("tree", mi, k, &node.label));
                        Self::tree_node(self, ui, mi, m, node, key, single, ctrl, &eye_btn, &group_menu, &mut acts);
                    }
                });
        }
        acts
    }

    /// Un nod al arborelui (recursiv: grupurile de sub el, sau elementele la ultimul nivel).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn tree_node(
        &self,
        ui: &mut egui::Ui,
        mi: usize,
        m: &crate::model::Model,
        node: &crate::app::TreeNode,
        key: egui::Id,
        open: bool,
        ctrl: bool,
        eye_btn: &dyn Fn(&mut egui::Ui, &[ElemId], &mut Vec<Action>),
        group_menu: &dyn Fn(&egui::Response, &Vec<ElemId>, &mut Vec<Action>),
        acts: &mut Vec<Action>,
    ) {
        let ids: Vec<ElemId> = node.ids.iter().map(|e| ElemId { m: mi as u32, e: *e }).collect();
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), key, open)
            .show_header(ui, |ui| {
                eye_btn(ui, &ids, acts);
                let all_sel = ids.iter().all(|i| self.selected.contains(i));
                // grupele de ultim nivel (categorii / tipuri) au punct colorat; cele cu subgrupe (niveluri) sunt ingrosate
                let leaf = node.children.is_empty();
                let r = tree_row(ui, &node.label, Some(ids.len()), if leaf { Some(cat_color(&node.label)) } else { None }, all_sel, !leaf, false);
                if r.clicked() {
                    acts.push(Action::Select(ids.clone(), ctrl));
                }
                group_menu(&r, &ids, acts);
            })
            .body(|ui| {
                if node.children.is_empty() {
                    Self::tree_elems(self, ui, mi, m, &ids, ctrl, eye_btn, group_menu, acts);
                } else {
                    let single = node.children.len() == 1;
                    for (k, ch) in node.children.iter().enumerate() {
                        let ck = key.with((k, &ch.label));
                        Self::tree_node(self, ui, mi, m, ch, ck, single, ctrl, eye_btn, group_menu, acts);
                    }
                }
            });
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn tree_elems(
        &self,
        ui: &mut egui::Ui,
        _mi: usize,
        m: &crate::model::Model,
        ids: &[ElemId],
        ctrl: bool,
        eye_btn: &dyn Fn(&mut egui::Ui, &[ElemId], &mut Vec<Action>),
        group_menu: &dyn Fn(&egui::Response, &Vec<ElemId>, &mut Vec<Action>),
        acts: &mut Vec<Action>,
    ) {
        for (k, id) in ids.iter().enumerate() {
            if k >= 1500 {
                ui.label(RichText::new(format!("… {} more elements (use Find)", ids.len() - k)).weak());
                break;
            }
            let e = &m.elements[id.e as usize];
            ui.horizontal(|ui| {
                eye_btn(ui, &[*id], acts);
                // punctul are culoarea elementului din model
                let c = m.tri_color.get(e.tri_start as usize).copied().unwrap_or([160, 160, 160, 255]);
                let r = tree_row(ui, &e.name, None, Some(Color32::from_rgb(c[0], c[1], c[2])), self.selected.contains(id), false, self.hidden.contains(id));
                if r.clicked() {
                    acts.push(Action::Select(vec![*id], ctrl));
                }
                if r.double_clicked() {
                    acts.push(Action::Focus(vec![*id]));
                }
                group_menu(&r, &vec![*id], acts);
            });
        }
    }

    /// Bara de sus a arborelui: gruparea curenta + editorul de niveluri (orice proprietate).
    pub(super) fn tree_config_ui(&mut self, ui: &mut egui::Ui) {
        let levels = self.settings.tree_levels.clone();
        let desc = if levels.is_empty() { "(flat list)".to_string() } else { levels.iter().map(|l| l.prop.clone()).collect::<Vec<_>>().join(" › ") };
        ui.horizontal(|ui| {
            ui.label(RichText::new("Group by").small().weak());
            if ui.selectable_label(self.tree_edit, RichText::new(format!("⚙ {desc}")).small()).on_hover_text("Choose how the Selection Tree is grouped (any property, any order)").clicked() {
                self.tree_edit = !self.tree_edit;
            }
        });
        if !self.tree_edit {
            return;
        }
        let cats: Vec<String> = self.prop_index.cats.iter().map(|c| c.0.clone()).collect();
        let mut new = levels.clone();
        let mut action: Option<(usize, i32)> = None; // (index, -1 sus / +1 jos / 0 sterge)
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::Grid::new("tree_levels").num_columns(4).show(ui, |ui| {
                for (i, lv) in new.iter_mut().enumerate() {
                    ui.label(RichText::new(format!("{}.", i + 1)).weak());
                    egui::ComboBox::from_id_salt(("tl_cat", i)).width(110.0).selected_text(&lv.cat).show_ui(ui, |ui| {
                        for c in &cats {
                            if ui.selectable_label(*c == lv.cat, c).clicked() && *c != lv.cat {
                                lv.cat = c.clone();
                                lv.prop = self.prop_index.props(c).and_then(|v| v.first()).map(|p| p.name.clone()).unwrap_or_default();
                            }
                        }
                    });
                    let props: Vec<String> = self.prop_index.props(&lv.cat).map(|v| v.iter().map(|p| p.name.clone()).collect()).unwrap_or_default();
                    egui::ComboBox::from_id_salt(("tl_prop", i)).width(150.0).selected_text(&lv.prop).show_ui(ui, |ui| {
                        for p in &props {
                            ui.selectable_value(&mut lv.prop, p.clone(), p);
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.small_button("⬆").clicked() {
                            action = Some((i, -1));
                        }
                        if ui.small_button("⬇").clicked() {
                            action = Some((i, 1));
                        }
                        if ui.small_button("🗑").clicked() {
                            action = Some((i, 0));
                        }
                    });
                    ui.end_row();
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("➕ Level").clicked() {
                    new.push(TreeLevel::new(ITEM_CAT, "Name"));
                }
                ui.menu_button(RichText::new("Presets").small(), |ui| {
                    let presets: [(&str, Vec<TreeLevel>); 5] = [
                        ("Level › IFC Type (default)", crate::app::default_tree_levels()),
                        ("IFC Type › Level", vec![TreeLevel::new(ITEM_CAT, "IFC Type"), TreeLevel::new(ITEM_CAT, "Level")]),
                        ("Level › IFC Type › Type Name", vec![TreeLevel::new(ITEM_CAT, "Level"), TreeLevel::new(ITEM_CAT, "IFC Type"), TreeLevel::new("Type", "Type Name")]),
                        ("Level › Material", vec![TreeLevel::new(ITEM_CAT, "Level"), TreeLevel::new("Material", "Material")]),
                        ("Flat list (no grouping)", Vec::new()),
                    ];
                    for (name, lv) in presets {
                        if ui.button(name).clicked() {
                            new = lv;
                            ui.close_menu();
                        }
                    }
                });
                if ui.small_button("💾 Export…").on_hover_text("Save this grouping (.bimtree) to reuse it").clicked() {
                    self.request_path(Purpose::ExportTree);
                }
                if ui.small_button("📂 Import…").clicked() {
                    self.request_path(Purpose::ImportTree);
                }
                if ui.small_button("Done").clicked() {
                    self.tree_edit = false;
                }
            });
        });
        if let Some((i, d)) = action {
            match d {
                0 => {
                    new.remove(i);
                }
                -1 if i > 0 => new.swap(i, i - 1),
                1 if i + 1 < new.len() => new.swap(i, i + 1),
                _ => {}
            }
        }
        if new != self.settings.tree_levels {
            self.set_tree_levels(new);
        }
    }

    pub(super) fn sets_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("➕ Selection Set")).on_hover_text("Fixed list of the selected items (from any model)").clicked() {
                let n = self.sets.len() + 1;
                self.sets.push(SelSet { name: format!("Selection {n}"), kind: SetKind::Explicit(self.selected.iter().copied().collect()) });
            }
            if ui.button("➕ Search Set").on_hover_text("Define conditions in Find Items, then Save Search Set").clicked() {
                self.open_panel(Panel::Find);
            }
            if ui.add_enabled(!self.sets.is_empty(), egui::Button::new("Export…")).on_hover_text("Share sets with other projects").clicked() {
                self.request_path(Purpose::ExportSets);
            }
            if ui.button("Import…").clicked() {
                self.request_path(Purpose::ImportSets);
            }
            if ui.add_enabled(!self.sets.is_empty(), egui::Button::new("🧭 Navisworks…")).on_hover_text("Export as Navisworks search sets XML (Sets > Import Search Sets)").clicked() {
                self.request_path(Purpose::ExportNavisSets);
            }
        });
        ui.separator();
        if self.sets.is_empty() {
            ui.label(RichText::new("No sets yet.\n\n• Properties tab: tick values (e.g. several IFC types) → Save as Search Set\n• Find Items: conditions → Save Search Set\n• Select items → ➕ Selection Set\n\nSets work across all loaded models and can be used in Clash Detective and TimeLiner.").weak());
        }
        let mut acts = Vec::new();
        let mut del = None;
        let mut edit = None;
        let ctrl = ui.input(|i| i.modifiers.command);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for i in 0..self.sets.len() {
                let ids = self.resolve_set(&self.sets[i]);
                let all_sel = !ids.is_empty() && ids.iter().all(|x| self.selected.contains(x));
                ui.horizontal(|ui| {
                    let (icon, tip) = match &self.sets[i].kind {
                        SetKind::Explicit(_) => ("📋", "Selection set (fixed items)".to_string()),
                        SetKind::Search(r) => ("🔍", format!("Search set (updates automatically)\n\n{}", Self::describe_rules(r))),
                    };
                    if ui.selectable_label(all_sel, icon).on_hover_text(format!("{tip}\n\nClick: select {} items", ids.len())).clicked() {
                        acts.push(Action::Select(ids.clone(), ctrl));
                    }
                    ui.add(egui::TextEdit::singleline(&mut self.sets[i].name).desired_width(118.0));
                    if ui.small_button(format!("{}", ids.len())).on_hover_text("Select these items").clicked() {
                        acts.push(Action::Select(ids.clone(), ctrl));
                    }
                    if ui.small_button("👁").on_hover_text("Hide / unhide").clicked() {
                        if ids.iter().all(|x| self.hidden.contains(x)) {
                            acts.push(Action::Show(ids.clone()));
                        } else {
                            acts.push(Action::Hide(ids.clone()));
                        }
                    }
                    match self.sets[i].kind {
                        SetKind::Search(_) => {
                            if ui.small_button("✏").on_hover_text("Edit the conditions in Find Items").clicked() {
                                edit = Some(i);
                            }
                        }
                        SetKind::Explicit(_) => {
                            if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("⟳").small()).on_hover_text("Update: replace the items with the current selection").clicked() {
                                self.sets[i].kind = SetKind::Explicit(self.selected.iter().copied().collect());
                            }
                        }
                    }
                    if ui.small_button("🗑").on_hover_text("Delete set").clicked() {
                        del = Some(i);
                    }
                });
            }
        });
        if let Some(i) = edit {
            if let SetKind::Search(r) = &self.sets[i].kind {
                self.find_rules = r.clone();
                self.editing_set = Some(i);
                self.open_panel(Panel::Find);
            }
        }
        if let Some(i) = del {
            self.sets.remove(i);
            self.editing_set = None;
        }
        for a in acts {
            self.apply(a);
        }
    }

    pub(super) fn find_items_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_top(|ui| {
            // ---- search in (models)
            ui.vertical(|ui| {
                ui.set_width(220.0);
                ui.strong("Search in");
                let ids: Vec<(usize, String)> = self.loaded_models().map(|(i, m)| (i, m.name.clone())).collect();
                ui.horizontal(|ui| {
                    if ui.small_button("All").clicked() {
                        self.find_models_off.clear();
                    }
                    if ui.small_button("None").clicked() {
                        self.find_models_off = ids.iter().map(|x| x.0).collect();
                    }
                });
                egui::ScrollArea::vertical().id_salt("fmodels").max_height(ui.available_height() - 40.0).show(ui, |ui| {
                    for (i, n) in &ids {
                        let mut on = !self.find_models_off.contains(i);
                        if ui.checkbox(&mut on, n.as_str()).on_hover_text(n.as_str()).changed() {
                            if on {
                                self.find_models_off.remove(i);
                            } else {
                                self.find_models_off.insert(*i);
                            }
                        }
                    }
                    if ids.is_empty() {
                        ui.label(RichText::new("No models loaded.").weak());
                    }
                });
                ui.checkbox(&mut self.find_in_selection, "Current selection only");
            });
            ui.separator();
            // ---- conditions
            ui.vertical(|ui| {
                if let Some(si) = self.editing_set {
                    let name = self.sets.get(si).map(|s| s.name.clone()).unwrap_or_default();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Editing search set “{name}”")).strong().color(accent()));
                        if ui.button("💾 Update set").clicked() {
                            if let Some(s) = self.sets.get_mut(si) {
                                s.kind = SetKind::Search(self.find_rules.clone());
                            }
                            self.toast(format!("Search set “{name}” updated."));
                            self.editing_set = None;
                        }
                        if ui.button("Stop editing").clicked() {
                            self.editing_set = None;
                        }
                    });
                }
                if self.find_rules.is_empty() {
                    self.find_rules.push(SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "", false));
                }
                // ---- conditiile ca "chip-uri": Level = 2 ×  Type = Wall ×  + Condition   [Find All]
                let mut remove: Option<usize> = None;
                let mut chip_go = false;
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                    let chip_bg = card_fill(ui.visuals());
                    for (i, r) in self.find_rules.iter().enumerate() {
                        if r.or && i > 0 {
                            ui.label(RichText::new("OR").small().strong().color(accent()));
                        }
                        let val = if r.op.needs_value() { if r.value.is_empty() { "…".to_string() } else { r.value.clone() } } else { String::new() };
                        egui::Frame::none().fill(chip_bg).rounding(7.0).inner_margin(egui::Margin { left: 9.0, right: 4.0, top: 3.0, bottom: 3.0 }).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                ui.label(RichText::new(&r.field).size(12.5).color(ui.visuals().weak_text_color()));
                                ui.label(RichText::new(r.op.label()).size(12.5).color(ui.visuals().weak_text_color()));
                                if !val.is_empty() {
                                    ui.label(RichText::new(val).size(12.5).monospace().strong());
                                }
                                if ui.add(egui::Button::new(RichText::new(egui_phosphor::regular::X).font(crate::theme::icon_font(11.0)).color(ui.visuals().weak_text_color())).frame(false)).on_hover_text("Remove this condition").clicked() {
                                    remove = Some(i);
                                }
                            });
                        });
                    }
                    if ui.add(egui::Button::new(RichText::new("+ Condition").size(12.5)).fill(chip_bg).stroke(Stroke::NONE).rounding(7.0)).on_hover_text("Add a condition (AND) - set it in the rows below").clicked() {
                        let last = self.find_rules.last().cloned();
                        let r = match last {
                            Some(l) if !l.group.is_empty() => SearchRule::new(&l.group, &l.field, SearchOp::Equals, "", false),
                            _ => SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "", false),
                        };
                        self.find_rules.push(r);
                    }
                    ui.add_space(8.0);
                    chip_go = ui.add(egui::Button::new(RichText::new("Find All").strong().color(Color32::WHITE)).fill(accent()).rounding(7.0).min_size(EV::new(96.0, 24.0))).clicked();
                });
                if let Some(i) = remove {
                    self.find_rules.remove(i);
                    if self.find_rules.is_empty() {
                        self.find_rules.push(SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "", false));
                    }
                }
                let nm = self.loaded_models().filter(|(i, _)| !self.find_models_off.contains(i)).count();
                let scope: usize = self.loaded_models().filter(|(i, _)| !self.find_models_off.contains(i)).map(|(_, m)| m.elements.len()).sum();
                ui.label(RichText::new(format!("{} conditions · {} elements in scope ({} models){}", self.find_rules.len(), thousands(scope), nm, self.find_count.map(|n| format!(" · {n} found")).unwrap_or_default())).small().weak());
                ui.horizontal(|ui| {
                    ui.add_space(46.0);
                    for (w, t) in [(156.0, "Category"), (176.0, "Property"), (116.0, "Condition"), (180.0, "Value")] {
                        ui.add_sized([w, 16.0], egui::Label::new(RichText::new(t).strong()));
                    }
                });
                ui.add_space(4.0);
                let enter = egui::ScrollArea::vertical()
                    .id_salt("findrules")
                    .max_height((ui.available_height() - 70.0).max(60.0))
                    .show(ui, |ui| Self::rule_rows(ui, "find", &mut self.find_rules, &self.prop_index, true))
                    .inner;
                // "+ Condition" si "Find All" sunt sus, la chip-uri; aici raman OR, Clear si seturile
                ui.horizontal_wrapped(|ui| {
                    if ui.button("➕ OR Condition").on_hover_text("Start an OR block: items matching either block are found").clicked() {
                        let last = self.find_rules.last().cloned();
                        let r = match last {
                            Some(l) if !l.group.is_empty() => SearchRule::new(&l.group, &l.field, SearchOp::Equals, "", true),
                            _ => SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "", true),
                        };
                        self.find_rules.push(r);
                    }
                    if enter || chip_go {
                        let r = self.find_scoped(&self.find_rules.clone(), self.find_in_selection);
                        self.find_count = Some(r.len());
                        self.apply(Action::Select(r, false));
                    }
                    if ui.button("Clear").clicked() {
                        self.find_rules = vec![SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "", false)];
                        self.find_count = None;
                    }
                    ui.separator();
                    if ui.button("💾 Save Search Set").on_hover_text("Saves the conditions: the set re-evaluates itself on every model, also after models are updated").clicked() {
                        let n = self.sets.len() + 1;
                        self.sets.push(SelSet { name: Self::rules_name(&self.find_rules).unwrap_or(format!("Search {n}")), kind: SetKind::Search(self.find_rules.clone()) });
                        self.open_panel(Panel::Sets);
                        self.toast("Search set saved (Sets panel).");
                    }
                    if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("💾 Save Selection Set")).on_hover_text("Saves the currently selected items as a fixed list").clicked() {
                        let n = self.sets.len() + 1;
                        self.sets.push(SelSet { name: format!("Selection {n}"), kind: SetKind::Explicit(self.selected.iter().copied().collect()) });
                        self.open_panel(Panel::Sets);
                    }
                    if let Some(n) = self.find_count {
                        ui.label(RichText::new(format!("{n} items found")).strong());
                    }
                });
            });
        });
    }

    /// A readable name for a set made from rules ("IfcColumn + IfcWindow").
    pub(super) fn rules_name(rules: &[SearchRule]) -> Option<String> {
        let vals: Vec<&str> = rules.iter().filter(|r| r.op == SearchOp::Equals && !r.value.is_empty()).map(|r| r.value.as_str()).collect();
        if vals.is_empty() || vals.len() != rules.len() {
            return None;
        }
        let mut s = vals.iter().take(4).cloned().collect::<Vec<_>>().join(" + ");
        if vals.len() > 4 {
            s.push_str(&format!(" +{}", vals.len() - 4));
        }
        Some(s)
    }

    /// Selection Tree in "Properties" mode: tick values (several categories / types / levels…)
    /// from all models, then save them as a search set.
    pub(super) fn props_tree_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.prop_filter).hint_text("🔍 filter categories, properties, values").desired_width(f32::INFINITY));
        });
        let nchk = self.prop_checked.len();
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(nchk > 0, egui::Button::new("💾 Save as Search Set")).on_hover_text("Ticked values are combined with OR, from all models").clicked() {
                let rules = self.rules_from_checked();
                let n = self.sets.len() + 1;
                self.sets.push(SelSet { name: Self::rules_name(&rules).unwrap_or(format!("Search {n}")), kind: SetKind::Search(rules) });
                self.toast("Search set saved (Sets).");
            }
            if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("Save Selection Set")).clicked() {
                let n = self.sets.len() + 1;
                self.sets.push(SelSet { name: format!("Selection {n}"), kind: SetKind::Explicit(self.selected.iter().copied().collect()) });
            }
            if ui.add_enabled(nchk > 0, egui::Button::new("🔍 Edit in Find Items")).on_hover_text("Open these conditions in Find Items to refine them").clicked() {
                self.find_rules = self.rules_from_checked();
                self.open_panel(Panel::Find);
            }
            if ui.add_enabled(nchk > 0, egui::Button::new("Clear")).clicked() {
                self.prop_checked.clear();
                self.clear_selection();
            }
        });
        if nchk > 0 {
            ui.label(RichText::new(format!("{nchk} values ticked · {} items selected", self.selected.len())).color(accent()));
        } else {
            ui.label(RichText::new("Tick values to select items from all models (e.g. several IFC types).").weak().small());
        }
        ui.separator();
        let fl = self.prop_filter.trim().to_lowercase();
        let mut changed = false;
        let idx = std::mem::take(&mut self.prop_index);
        egui::ScrollArea::vertical().id_salt("proptree").auto_shrink([false, false]).show(ui, |ui| {
            if idx.cats.is_empty() {
                ui.label(RichText::new("No model loaded.").weak());
            }
            for (ci, (cat, props)) in idx.cats.iter().enumerate() {
                let cat_hit = fl.is_empty() || cat.to_lowercase().contains(&fl);
                let shown: Vec<&PropValues> = props
                    .iter()
                    .filter(|p| cat_hit || p.name.to_lowercase().contains(&fl) || p.values.iter().any(|(v, _)| v.to_lowercase().contains(&fl)))
                    .collect();
                if shown.is_empty() {
                    continue;
                }
                let checked_in_cat = self.prop_checked.iter().filter(|k| &k.0 == cat).count();
                let mut head = RichText::new(format!("{cat}  ({})", props.len())).strong();
                if checked_in_cat > 0 {
                    head = head.color(accent());
                }
                egui::CollapsingHeader::new(head).id_salt(("pc", ci)).default_open(ci == 0 || !fl.is_empty()).show(ui, |ui| {
                    for (pi, p) in shown.iter().enumerate() {
                        let prop_hit = cat_hit || p.name.to_lowercase().contains(&fl);
                        let mut ph = RichText::new(format!("{}  ({} values)", p.name, p.distinct));
                        if self.prop_checked.iter().any(|k| &k.0 == cat && k.1 == p.name) {
                            ph = ph.color(accent());
                        }
                        egui::CollapsingHeader::new(ph).id_salt(("pp", ci, pi, &p.name)).default_open(!fl.is_empty() && !prop_hit).show(ui, |ui| {
                            if p.values.is_empty() {
                                ui.label(RichText::new(format!("{} distinct values - use Find Items with 'contains'.", p.distinct)).weak().small());
                                return;
                            }
                            let vals: Vec<&(String, u32)> = p.values.iter().filter(|(v, _)| prop_hit || v.to_lowercase().contains(&fl)).collect();
                            for (k, (v, n)) in vals.iter().enumerate() {
                                if k >= 500 {
                                    ui.label(RichText::new(format!("… {} more - use the filter", vals.len() - k)).weak().small());
                                    break;
                                }
                                let key = (cat.clone(), p.name.clone(), v.clone());
                                let mut on = self.prop_checked.contains(&key);
                                if ui.checkbox(&mut on, format!("{v}   ({n})")).changed() {
                                    if on {
                                        self.prop_checked.insert(key);
                                    } else {
                                        self.prop_checked.remove(&key);
                                    }
                                    changed = true;
                                }
                            }
                        });
                    }
                });
            }
        });
        self.prop_index = idx;
        if changed {
            if self.prop_checked.is_empty() {
                self.clear_selection();
            } else {
                let rules = self.rules_from_checked();
                let ids = self.find_scoped(&rules, false);
                self.apply(Action::Select(ids, false));
            }
        }
    }

    pub(super) fn properties_panel(&mut self, ui: &mut egui::Ui) {
        let Some(pid) = self.primary.or_else(|| self.selected.iter().next().copied()) else {
            ui.label(RichText::new("Select an element in the 3D view or in the tree.").weak());
            return;
        };
        let sel: Vec<ElemId> = self.selected.iter().copied().collect();
        if sel.len() > 1 {
            ui.label(format!("{} elements selected — showing the last one.", sel.len()));
        }
        let mut acts = Vec::new();
        // filtrul e chiar sub titlul panoului, inainte de orice card
        let filter_id = ui.make_persistent_id("prop_filter");
        let mut filter: String = ui.data_mut(|d| d.get_temp(filter_id)).unwrap_or_default();
        ui.add(egui::TextEdit::singleline(&mut filter).hint_text("🔍 Filter properties").desired_width(f32::INFINITY));
        ui.add_space(3.0);
        // actiunea principala sus, nu jos
        let b = egui::Button::new(RichText::new("🎯  Show in Model").strong().size(13.0).color(Color32::WHITE)).fill(accent()).rounding(7.0);
        if ui.add_sized([ui.available_width(), 28.0], b).on_hover_text("Zoom to the selected item(s)").clicked() {
            acts.push(Action::Focus(sel.clone()));
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui.small_button("Hide").clicked() {
                acts.push(Action::Hide(sel.clone()));
            }
            if ui.small_button("Hide Unselected").clicked() {
                acts.push(Action::Isolate(sel.clone()));
            }
            if ui.small_button("Focus").clicked() {
                acts.push(Action::Focus(sel.clone()));
            }
            if sel.len() == 2 && ui.small_button("Min. distance").clicked() {
                self.min_distance_selected();
            }
        });
        // ---- date utilizator ca in Navisworks: Custom > Add / Delete / Rename User Data Tab, Insert New Property
        self.user_data_ui(ui, pid, &sel);
        // aspect
        egui::CollapsingHeader::new("Appearance override").default_open(false).show(ui, |ui| {
            let mut c = Color32::from_rgba_unmultiplied(self.override_color[0], self.override_color[1], self.override_color[2], 255);
            ui.horizontal(|ui| {
                ui.label("Colour");
                if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
                    self.override_color = [c.r(), c.g(), c.b(), self.override_color[3]];
                }
            });
            let mut tr = 100.0 - self.override_color[3] as f32 / 255.0 * 100.0;
            if ui.add(egui::Slider::new(&mut tr, 0.0..=90.0).text("Transparency %")).changed() {
                self.override_color[3] = ((100.0 - tr) / 100.0 * 255.0) as u8;
            }
            ui.horizontal(|ui| {
                if ui.button("Apply to selection").clicked() {
                    for id in &sel {
                        self.overrides.insert(*id, self.override_color);
                    }
                    self.colors_dirty = true;
                }
                if ui.button("Reset").clicked() {
                    for id in &sel {
                        self.overrides.remove(id);
                    }
                    self.colors_dirty = true;
                }
                if ui.button("Reset all").clicked() {
                    self.overrides.clear();
                    self.colors_dirty = true;
                }
            });
        });
        ui.separator();
        let Some((m, e)) = self.elem(pid) else { return };
        let mut groups: Vec<(String, Vec<(String, String)>)> = Vec::new();
        groups.push(("File".into(), vec![("Model".into(), m.name.clone()), ("Path".into(), m.path.clone())]));
        let user_g: HashSet<(String, String)> = self.user_params.iter().map(|p| (p.group.clone(), p.name.to_lowercase())).collect();
        for p in &e.props {
            if p.group == USER_GROUP || user_g.contains(&(p.group.clone(), p.name.to_lowercase())) {
                continue;
            }
            match groups.iter_mut().find(|g| g.0 == p.group) {
                Some(g) => g.1.push((p.name.clone(), p.value.clone())),
                None => groups.push((p.group.clone(), vec![(p.name.clone(), p.value.clone())])),
            }
        }
        let bb = e.aabb;
        let oz = self.origin.lock().unwrap_or_else(|e| e.into_inner()).map(|o| o.z as f32).unwrap_or(0.0);
        groups.push((
            "Geometry".into(),
            vec![
                ("Triangles".into(), e.tri_count.to_string()),
                ("Size X×Y×Z (m)".into(), format!("{:.3} × {:.3} × {:.3}", bb.size().x, bb.size().y, bb.size().z)),
                ("Elevation min / max Z (m)".into(), format!("{:.3} / {:.3}", bb.min.z + oz, bb.max.z + oz)),
            ],
        ));
        let mut copy = None;
        // carduri compacte, cu un chenar abia vizibil
        let card_fill = ui.visuals().faint_bg_color;
        let card_stroke = Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color);
        let weak = ui.visuals().weak_text_color();
        {
                egui::Frame::none().fill(card_fill).stroke(card_stroke).rounding(6.0).inner_margin(5.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let c = m.tri_color.get(e.tri_start as usize).copied().unwrap_or([200, 200, 200, 255]);
                        let (r, _) = ui.allocate_exact_size(EV::splat(9.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.5, Color32::from_rgb(c[0], c[1], c[2]));
                        let name = ui.label(RichText::new(&e.name).strong().size(13.5));
                        name.on_hover_text(format!("{}\n{}\n{}", e.ifc_type, e.guid, m.name));
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&e.ifc_type).small().color(accent()));
                        if !e.storey.is_empty() {
                            ui.label(RichText::new(format!("· {}", e.storey)).small().color(weak));
                        }
                    });
                });
                let q = filter.trim().to_lowercase();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for (gi, (g, rows)) in groups.iter().enumerate() {
                        let rows: Vec<&(String, String)> = rows.iter().filter(|(n, v)| q.is_empty() || n.to_lowercase().contains(&q) || v.to_lowercase().contains(&q) || g.to_lowercase().contains(&q)).collect();
                        if rows.is_empty() {
                            continue;
                        }
                        egui::Frame::none().fill(card_fill).stroke(card_stroke).rounding(6.0).inner_margin(egui::Margin { left: 6.0, right: 6.0, top: 3.0, bottom: 3.0 }).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let id = ui.make_persistent_id(("pcard", g));
                            let mut st = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, gi < 6 || !q.is_empty());
                            let open = st.is_open();
                            let head = ui.horizontal(|ui| {
                                ui.label(RichText::new(g.to_uppercase()).small().strong());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(if open { "▾" } else { "▸" }).color(weak));
                                    ui.label(RichText::new(rows.len().to_string()).small().color(weak));
                                });
                            });
                            if ui.interact(head.response.rect, id.with("h"), Sense::click()).clicked() {
                                st.toggle(ui);
                            }
                            st.store(ui.ctx());
                            if open {
                                ui.add_space(2.0);
                                egui::Grid::new(("pgrid", gi, g)).num_columns(2).spacing([6.0, 0.0]).show(ui, |ui| {
                                    for (n, v) in rows {
                                        ui.label(RichText::new(n).small().color(weak));
                                        let r = ui.add(egui::Label::new(v.as_str()).truncate().sense(Sense::click()));
                                        let r = r.on_hover_text(v.as_str());
                                        r.context_menu(|ui| {
                                            if ui.button("Copy value").clicked() {
                                                copy = Some(v.clone());
                                                ui.close_menu();
                                            }
                                        });
                                        ui.end_row();
                                    }
                                });
                            }
                        });
                        ui.add_space(2.0);
                    }
                });
        }
        ui.data_mut(|d| d.insert_temp(filter_id, filter));
        if let Some(c) = copy {
            ui.ctx().copy_text(c);
        }
        for a in acts {
            self.apply(a);
        }
    }

    pub(super) fn selector_combo(&self, ui: &mut egui::Ui, id: &str, s: &mut Selector) {
        egui::ComboBox::from_id_salt(id).width(190.0).selected_text(s.label()).show_ui(ui, |ui| {
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                ui.selectable_value(s, Selector { kind: SelKind::All, value: String::new() }, "All elements");
                ui.separator();
                for dsc in ["Structure", "Architecture", "MEP - HVAC", "MEP - Plumbing", "MEP - Electrical"] {
                    let v = Selector { kind: SelKind::Discipline, value: dsc.to_string() };
                    let l = v.label();
                    ui.selectable_value(s, v, l);
                }
                ui.separator();
                for (_, m) in self.loaded_models() {
                    let v = Selector { kind: SelKind::Model, value: m.name.clone() };
                    let l = v.label();
                    ui.selectable_value(s, v, l);
                }
                if !self.sets.is_empty() {
                    ui.separator();
                }
                for set in &self.sets {
                    let v = Selector { kind: SelKind::Set, value: set.name.clone() };
                    let l = v.label();
                    ui.selectable_value(s, v, l);
                }
                ui.separator();
                for t in &self.cache_types {
                    let v = Selector { kind: SelKind::Type, value: t.clone() };
                    let l = v.label();
                    ui.selectable_value(s, v, l);
                }
            });
        });
    }

    /// Tab-urile de date utilizator ale elementului (Navisworks: butonul Custom din Properties):
    /// proprietati cu valori scrise pe toate elementele selectate, salvate in proiect.
    pub(super) fn user_data_ui(&mut self, ui: &mut egui::Ui, pid: ElemId, sel: &[ElemId]) {
        let mut tabs: Vec<(String, Vec<(String, String)>)> = self.user_tabs_of(pid);
        for t in &self.user_tabs_new {
            if !tabs.iter().any(|x| &x.0 == t) {
                tabs.push((t.clone(), Vec::new()));
            }
        }
        if !tabs.iter().any(|t| t.0 == self.user_tab) {
            self.user_tab = tabs.first().map(|t| t.0.clone()).unwrap_or_default();
        }
        let cur = self.user_tab.clone();
        let card_fill = ui.visuals().faint_bg_color;
        let card_stroke = Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color);
        let weak = ui.visuals().weak_text_color();
        enum Op {
            AddTab,
            DelTab,
            Rename(String),
            Insert,
            Set(String, String),
            RenameProp(String, String),
            Del(String),
        }
        let mut ops: Vec<Op> = Vec::new();
        egui::Frame::none().fill(card_fill).stroke(card_stroke).rounding(8.0).inner_margin(egui::Margin { left: 10.0, right: 10.0, top: 8.0, bottom: 8.0 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("USER DATA").small().strong());
                ui.label(RichText::new(if sel.len() > 1 { format!("· {} selected items", sel.len()) } else { String::new() }).small().color(weak));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.menu_button("Custom ⏷", |ui| {
                        if ui.button("Add User Data Tab").clicked() {
                            ops.push(Op::AddTab);
                            ui.close_menu();
                        }
                        if ui.add_enabled(!cur.is_empty(), egui::Button::new("Delete User Data Tab")).clicked() {
                            ops.push(Op::DelTab);
                            ui.close_menu();
                        }
                        if ui.add_enabled(!cur.is_empty(), egui::Button::new("Rename User Data Tab")).clicked() {
                            self.user_tab_rename = Some(cur.clone());
                            ui.close_menu();
                        }
                        if ui.button("Insert New Property").clicked() {
                            ops.push(Op::Insert);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("📥 Import properties (Excel / CSV)…").on_hover_text("Write properties onto elements from a table: one row per element, matched by GlobalId, Name or any property; every other column becomes a property").clicked() {
                            self.request_path(Purpose::ImportUserProps);
                            ui.close_menu();
                        }
                        if ui.button("📤 Export properties (Excel)…").on_hover_text("One row per element (selection, or all visible) with its key - fill in values or add columns, then import").clicked() {
                            self.request_path(Purpose::ExportUserProps);
                            ui.close_menu();
                        }
                    });
                });
            });
            if tabs.is_empty() {
                ui.label(RichText::new("No user data. Custom › Add User Data Tab, then Insert New Property.").small().color(weak));
                return;
            }
            // tab-urile, ca in Navisworks
            ui.horizontal_wrapped(|ui| {
                for (t, _) in &tabs {
                    if ui.selectable_label(*t == cur, t).clicked() {
                        self.user_tab = t.clone();
                    }
                }
            });
            if let Some(mut nm) = self.user_tab_rename.clone() {
                ui.horizontal(|ui| {
                    ui.label("Rename tab");
                    let r = ui.add(egui::TextEdit::singleline(&mut nm).desired_width(150.0));
                    r.request_focus();
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    if ui.button("OK").clicked() || enter {
                        ops.push(Op::Rename(nm.clone()));
                    } else if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        self.user_tab_rename = None;
                    } else {
                        self.user_tab_rename = Some(nm);
                    }
                });
            }
            let rows = tabs.iter().find(|t| t.0 == cur).map(|t| t.1.clone()).unwrap_or_default();
            egui::Grid::new(("udgrid", &cur)).num_columns(3).spacing([8.0, 3.0]).striped(true).show(ui, |ui| {
                ui.label(RichText::new("Property").small().color(weak));
                ui.label(RichText::new("Value").small().color(weak));
                ui.label("");
                ui.end_row();
                for (k, (n, v)) in rows.iter().enumerate() {
                    // textul in lucru se pastreaza intre cadre; se scrie la Enter / iesirea din camp
                    let nid = egui::Id::new(("udn", &cur, k, pid));
                    let vid = egui::Id::new(("udv", &cur, k, pid));
                    let mut nn: String = ui.data(|d| d.get_temp(nid)).unwrap_or_else(|| n.clone());
                    let mut vv: String = ui.data(|d| d.get_temp(vid)).unwrap_or_else(|| v.clone());
                    // latimea campurilor dupa latimea panoului (Properties e de obicei ingust)
                    let w = (ui.available_width() - 40.0).max(120.0);
                    let rn = ui.add(egui::TextEdit::singleline(&mut nn).desired_width((w * 0.42).clamp(70.0, 180.0)).hint_text("property"));
                    let rv = ui.add(egui::TextEdit::singleline(&mut vv).desired_width((w * 0.58).clamp(80.0, 260.0)).hint_text("value"));
                    if rn.has_focus() || rn.changed() {
                        ui.data_mut(|d| d.insert_temp(nid, nn.clone()));
                    }
                    if rv.has_focus() || rv.changed() {
                        ui.data_mut(|d| d.insert_temp(vid, vv.clone()));
                    }
                    if rn.lost_focus() {
                        ui.data_mut(|d| d.remove::<String>(nid));
                        if nn.trim() != n {
                            ops.push(Op::RenameProp(n.clone(), nn.trim().to_string()));
                        }
                    }
                    if rv.lost_focus() {
                        ui.data_mut(|d| d.remove::<String>(vid));
                        if vv != *v {
                            ops.push(Op::Set(n.clone(), vv.clone()));
                        }
                    }
                    if ui.small_button("✖").on_hover_text("Delete property").clicked() {
                        ops.push(Op::Del(n.clone()));
                    }
                    ui.end_row();
                }
            });
            if ui.small_button("➕ Property").on_hover_text("Insert New Property").clicked() {
                ops.push(Op::Insert);
            }
        });
        ui.add_space(4.0);
        for op in ops {
            match op {
                Op::AddTab => {
                    let mut n = 1;
                    let name = loop {
                        let c = if n == 1 { "User Data".to_string() } else { format!("User Data {n}") };
                        if !tabs.iter().any(|t| t.0 == c) && !self.user_tabs_new.contains(&c) {
                            break c;
                        }
                        n += 1;
                    };
                    self.user_tabs_new.push(name.clone());
                    self.user_tab = name;
                }
                Op::DelTab => {
                    self.remove_user_param(sel, &cur, "");
                    self.user_tabs_new.retain(|t| *t != cur);
                    self.user_tab.clear();
                }
                Op::Rename(nm) => {
                    let nm = nm.trim().to_string();
                    if !nm.is_empty() {
                        self.rename_user_tab(sel, &cur, &nm);
                        for t in self.user_tabs_new.iter_mut() {
                            if *t == cur {
                                *t = nm.clone();
                            }
                        }
                        self.user_tab = nm;
                    }
                    self.user_tab_rename = None;
                }
                Op::Insert => {
                    let tab = if cur.is_empty() {
                        self.user_tabs_new.push("User Data".into());
                        "User Data".to_string()
                    } else {
                        cur.clone()
                    };
                    let mut k = 1;
                    let name = loop {
                        let c = if k == 1 { "New Property".to_string() } else { format!("New Property {k}") };
                        if !rows_has(&tabs, &tab, &c) {
                            break c;
                        }
                        k += 1;
                    };
                    self.set_user_prop(sel, &tab, &name, "");
                    self.user_tab = tab;
                }
                Op::Set(n, v) => {
                    self.set_user_prop(sel, &cur, &n, &v);
                }
                Op::RenameProp(o, n) => {
                    if !n.is_empty() {
                        self.rename_user_prop(sel, &cur, &o, &n);
                    }
                }
                Op::Del(n) => {
                    self.remove_user_param(sel, &cur, &n);
                }
            }
            self.refresh_caches();
        }
    }

    /// Fereastra de import a proprietatilor: coloana cheie, cum se potriveste, tab-ul tinta.
    pub(super) fn prop_import_window(&mut self, ctx: &egui::Context) {
        let Some(mut imp) = self.prop_import.clone() else { return };
        let mut open = true;
        let mut run = false;
        let mut cancel = false;
        let (ok_rows, n_el) = self.prop_import_preview(&imp);
        egui::Window::new("📥 Import properties").open(&mut open).collapsible(false).default_width(560.0).anchor(Align2::CENTER_CENTER, EV::ZERO).show(ctx, |ui| {
            ui.label(RichText::new(crate::file_name(&imp.path)).strong());
            let head: Vec<String> = imp.rows[imp.header].iter().map(|c| c.trim().trim_start_matches('\u{feff}').to_string()).collect();
            egui::Grid::new("pimp").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("Element key column");
                egui::ComboBox::from_id_salt("pimp_key").selected_text(head.get(imp.key_col).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                    for (i, h) in head.iter().enumerate() {
                        ui.selectable_value(&mut imp.key_col, i, h);
                    }
                });
                ui.end_row();
                ui.label("matches the element's");
                let mut opts: Vec<String> = vec!["Key".into(), "GlobalId".into(), "Name".into()];
                for p in &self.cache_props {
                    if !opts.contains(p) {
                        opts.push(p.clone());
                    }
                }
                egui::ComboBox::from_id_salt("pimp_by").width(220.0).selected_text(&imp.match_by).show_ui(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for o in &opts {
                            let tip = match o.as_str() {
                                "Key" => "The Key column of an exported properties template (GUID, or #index for items without one)",
                                "GlobalId" => "IFC GlobalId / GUID / Revit UniqueId",
                                "Name" => "Element name",
                                _ => "Value of this property",
                            };
                            ui.selectable_value(&mut imp.match_by, o.clone(), o).on_hover_text(tip);
                        }
                    });
                });
                ui.end_row();
                ui.label("Write into tab");
                ui.add(egui::TextEdit::singleline(&mut imp.tab).desired_width(200.0));
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut imp.only_selected, format!("Only the selected items ({})", self.selected.len()));
                ui.end_row();
            });
            ui.add_space(4.0);
            let props: Vec<&String> = head.iter().enumerate().filter(|(i, h)| *i != imp.key_col && !h.is_empty()).map(|(_, h)| h).collect();
            ui.label(RichText::new(format!("Properties (one per column): {}", props.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "))).small());
            let total = imp.rows.len() - imp.header - 1;
            let col = if ok_rows == 0 { Color32::from_rgb(230, 90, 70) } else { Color32::from_rgb(70, 170, 90) };
            ui.label(RichText::new(format!("{ok_rows} of {total} rows match {n_el} elements")).strong().color(col));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(ok_rows > 0, egui::Button::new(RichText::new("Import").strong())).clicked() {
                    run = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if !open || cancel {
            self.prop_import = None;
        } else {
            self.prop_import = Some(imp);
            if run {
                self.run_prop_import();
            }
        }
    }
}
