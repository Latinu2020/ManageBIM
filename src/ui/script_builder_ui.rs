//! The interface of the visual script editor (Script Library › Visual script builder).
use super::*;
use crate::script_builder::{FieldKind, PropIndex, Scenario, Step, StepKind};

/// What the user asked for while drawing the list (applied afterwards, so we do not hold borrows).
enum Pending {
    Browse(usize, usize, &'static str),
    Delete(usize),
    Up(usize),
    Down(usize),
}

impl App {
    /// The property index of the loaded models (rebuilt only when the models change).
    fn scenario_props(&mut self) -> std::sync::Arc<PropIndex> {
        let sig: u64 = self.loaded_models().map(|(mi, m)| (mi as u64 + 1) * 1_000_003 + m.elements.len() as u64).sum();
        if let Some((s, ix)) = &self.scenario_index {
            if *s == sig {
                return ix.clone();
            }
        }
        let mut rows: Vec<Vec<(&str, &str)>> = Vec::new();
        for (_, m) in self.loaded_models() {
            for e in &m.elements {
                let mut r: Vec<(&str, &str)> = vec![("Category", e.ifc_type.as_str()), ("Name", e.name.as_str()), ("Level", e.storey.as_str()), ("GlobalId", e.guid.as_str())];
                r.extend(e.props.iter().map(|p| (p.name.as_str(), p.value.as_str())));
                rows.push(r);
            }
        }
        let ix = std::sync::Arc::new(PropIndex::build(rows.into_iter()));
        self.scenario_index = Some((sig, ix.clone()));
        ix
    }

    pub(super) fn script_builder_ui(&mut self, ui: &mut egui::Ui) {
        ui.collapsing(RichText::new("🧩 Visual script builder — create a script without code").strong(), |ui| {
            let ix = self.scenario_props();
            ui.label(RichText::new(crate::i18n::tr("1. Pick a ready-made scenario or add steps   2. Choose the values from the lists   3. ▶ Run")).small().weak());
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut self.scenario.name).desired_width(180.0));
                // ready-made scenarios
                let tid = ui.make_persistent_id("vb_templates");
                let tr = ui.button("📋 Templates").on_hover_text("Start from a ready-made scenario");
                if tr.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(tid));
                }
                egui::popup_below_widget(ui, tid, &tr, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                    ui.set_min_width(320.0);
                    for (name, desc, sc) in Scenario::templates() {
                        if ui.button(RichText::new(name).strong()).on_hover_text(desc).clicked() {
                            self.scenario = sc;
                            ui.memory_mut(|m| m.close_popup());
                        }
                        ui.label(RichText::new(desc).small().weak());
                        ui.add_space(3.0);
                    }
                });
                if ui.button("📂 Open…").on_hover_text("Open a scenario saved from this editor (.rhai)").clicked() {
                    let dir = super::script::dir(&self.settings.scripts_dir);
                    if let Some(p) = rfd::FileDialog::new().add_filter("ManageBIM script", &["rhai"]).set_directory(&dir).pick_file() {
                        match std::fs::read_to_string(&p).ok().as_deref().and_then(Scenario::from_file) {
                            Some(sc) => {
                                self.scenario = sc;
                                self.toast("Scenario opened - you can edit the steps.");
                            }
                            None => self.error("This script was not made with the visual builder (it has no steps to show). Open it in the script list instead."),
                        }
                    }
                }
                if ui.small_button("↻").on_hover_text("Refresh the property lists from the loaded models").clicked() {
                    self.scenario_index = None;
                }
            });
            if ix.total == 0 {
                ui.label(RichText::new(crate::i18n::tr("ℹ Load a model to see the property lists and the preview.")).small().color(Color32::from_rgb(230, 170, 60)));
            }
            // add-step menu
            let add_id = ui.make_persistent_id("vb_add_menu");
            let add_r = ui.button("➕ Add step");
            if add_r.clicked() {
                ui.memory_mut(|m| m.toggle_popup(add_id));
            }
            egui::popup_below_widget(ui, add_id, &add_r, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(300.0);
                for k in StepKind::ALL {
                    if ui.button(RichText::new(format!("{}  — {}", k.label(), k.ro()))).on_hover_text(k.explain()).clicked() {
                        self.scenario.steps.push(Step::new(k));
                        ui.memory_mut(|m| m.close_popup());
                    }
                }
            });
            ui.separator();
            let mut pending: Option<Pending> = None;
            let red = Color32::from_rgb(235, 85, 80);
            for (i, st) in self.scenario.steps.iter_mut().enumerate() {
                egui::Frame::none().fill(ui.visuals().faint_bg_color).rounding(crate::theme::RADIUS_SM).inner_margin(egui::Margin::symmetric(7.0, 4.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}.", i + 1)).small().weak());
                        ui.label(RichText::new(st.kind.label()).strong()).on_hover_text(st.kind.explain());
                        if ui.small_button("↑").on_hover_text("Move up").clicked() {
                            pending = Some(Pending::Up(i));
                        }
                        if ui.small_button("↓").on_hover_text("Move down").clicked() {
                            pending = Some(Pending::Down(i));
                        }
                        if ui.small_button("🗑").on_hover_text("Remove step").clicked() {
                            pending = Some(Pending::Delete(i));
                        }
                    });
                    ui.label(RichText::new(st.kind.explain()).small().weak());
                    let fields = st.kind.fields();
                    if !fields.is_empty() {
                        egui::Grid::new(("vb_fields", i)).num_columns(2).spacing([8.0, 3.0]).show(ui, |ui| {
                            for (fi, fl) in fields.iter().enumerate() {
                                ui.label(RichText::new(fl.label).small());
                                ui.vertical(|ui| {
                                    let prev = if fi > 0 { st.values.get(fi - 1).cloned().unwrap_or_default() } else { String::new() };
                                    let err = st.field_error(fi);
                                    ui.horizontal(|ui| {
                                        let Some(val) = st.values.get_mut(fi) else { return };
                                        let r = ui.add(egui::TextEdit::singleline(val).desired_width(190.0).hint_text(fl.hint));
                                        if err.is_some() {
                                            ui.painter().rect_stroke(r.rect.expand(1.0), crate::theme::RADIUS_SM, egui::Stroke::new(1.5_f32, red));
                                        }
                                        match fl.kind {
                                            FieldKind::Prop | FieldKind::Value if ix.total > 0 => {
                                                let pid = ui.make_persistent_id(("vb_pick", i, fi));
                                                let b = ui.small_button("⏷").on_hover_text("Pick from the model");
                                                if b.clicked() {
                                                    ui.memory_mut(|m| m.toggle_popup(pid));
                                                }
                                                egui::popup_below_widget(ui, pid, &b, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                                                    ui.set_min_width(260.0);
                                                    let filter = val.trim().to_lowercase();
                                                    let items: Vec<(String, String)> = if fl.kind == FieldKind::Prop {
                                                        ix.keys.iter().filter(|(_, k)| k.filled > 0).map(|(k, info)| (k.clone(), format!("{k}   ({} el.)", info.filled))).collect()
                                                    } else {
                                                        ix.values_of(&prev).into_iter().map(|(v, n)| (v.clone(), format!("{v}   ({n})"))).collect()
                                                    };
                                                    // if the typed text filters out everything, we show the full list
                                                    let any = items.iter().any(|(k, _)| k.to_lowercase().contains(&filter));
                                                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                                                        if items.is_empty() {
                                                            ui.label(RichText::new("No values for this property.").weak());
                                                        }
                                                        for (k, shown) in items.iter().filter(|(k, _)| !any || filter.is_empty() || k.to_lowercase().contains(&filter)).take(400) {
                                                            if ui.selectable_label(false, shown).clicked() {
                                                                *val = k.clone();
                                                                ui.memory_mut(|m| m.close_popup());
                                                            }
                                                        }
                                                    });
                                                });
                                            }
                                            FieldKind::Path(ext) => {
                                                if ui.small_button("📁").on_hover_text("Choose where to save").clicked() {
                                                    pending = Some(Pending::Browse(i, fi, ext));
                                                }
                                            }
                                            _ => {}
                                        }
                                    });
                                    if let Some(e) = err {
                                        ui.label(RichText::new(e).small().color(red));
                                    }
                                });
                                ui.end_row();
                            }
                        });
                    }
                    // preview: what the step will do on the current model
                    if ix.total > 0 && st.is_valid() {
                        let v = |k: usize| st.values.get(k).cloned().unwrap_or_default();
                        let msg = match st.kind {
                            StepKind::SelectWhere => Some(match ix.count_where(&v(0), &v(1)) {
                                Some(0) => crate::i18n::tr("⚠ Nothing will be selected: no match in the model").to_string(),
                                Some(n) => if crate::i18n::is_romanian() { format!("👁 Va selecta {n} elemente") } else { format!("👁 Will select {n} elements") },
                                None => crate::i18n::tr("👁 Unknown count (property with very many values)").to_string(),
                            }),
                            StepKind::FindEmpty => Some(match ix.count_empty(&v(0)) {
                                0 => crate::i18n::tr("✔ Every element has this property filled in").to_string(),
                                n => if crate::i18n::is_romanian() { format!("👁 Va găsi {n} din {} elemente fără „{}”", ix.total, v(0)) } else { format!("👁 Will find {n} of {} elements without \"{}\"", ix.total, v(0)) },
                            }),
                            StepKind::SetProp => Some(crate::i18n::tr("👁 Written on the elements selected by the steps above").to_string()),
                            _ => None,
                        };
                        if let Some(m) = msg {
                            let col = if m.starts_with('⚠') { Color32::from_rgb(230, 170, 60) } else { crate::theme::accent() };
                            ui.label(RichText::new(m).small().color(col));
                        }
                    }
                });
                ui.add_space(2.0);
            }
            match pending {
                Some(Pending::Delete(i)) if i < self.scenario.steps.len() => {
                    self.scenario.steps.remove(i);
                }
                Some(Pending::Up(i)) if i > 0 => self.scenario.steps.swap(i, i - 1),
                Some(Pending::Down(i)) if i + 1 < self.scenario.steps.len() => self.scenario.steps.swap(i, i + 1),
                Some(Pending::Browse(i, fi, ext)) => {
                    let name = format!("{}.{ext}", if self.scenario.name.trim().is_empty() { "report" } else { self.scenario.name.trim() });
                    if let Some(p) = rfd::FileDialog::new().add_filter(ext.to_uppercase(), &[ext]).set_file_name(name).save_file() {
                        if let Some(v) = self.scenario.steps.get_mut(i).and_then(|s| s.values.get_mut(fi)) {
                            *v = p.to_string_lossy().replace('\\', "/");
                        }
                    }
                }
                _ => {}
            }
            if self.scenario.steps.is_empty() {
                ui.label(RichText::new(crate::i18n::tr("No steps yet. Press ➕ Add step or 📋 Templates.")).small().weak());
            }
            ui.separator();
            ui.horizontal(|ui| {
                let ok = self.scenario.is_valid();
                let run = ui.add_enabled(ok, egui::Button::new(RichText::new("▶ Run scenario").color(if ok { crate::theme::accent() } else { ui.visuals().weak_text_color() })));
                let run = if ok { run } else { run.on_disabled_hover_text("Add at least one step and fix the fields marked in red.") };
                if run.clicked() {
                    let (name, src) = (self.scenario.name.clone(), self.scenario.to_rhai());
                    self.run_source(&name, &src);
                    // set_prop may add new properties: the lists are rebuilt on the next opening
                    self.scenario_index = None;
                }
                if ui.add_enabled(!self.scenario.steps.is_empty(), egui::Button::new("💾 Save")).on_hover_text("Save to the scripts folder; you can reopen it here with 📂 Open").clicked() {
                    let src = self.scenario.to_file();
                    match super::script::save_named(&self.settings.scripts_dir, &self.scenario.name, &src) {
                        Ok(p) => self.toast(format!("Script saved: {}", crate::file_name(&p.to_string_lossy()))),
                        Err(e) => self.error(e),
                    }
                }
                if ui.button("🗑 Clear").clicked() {
                    self.scenario.steps.clear();
                }
            });
            ui.collapsing("Generated Rhai code", |ui| {
                let code = self.scenario.to_rhai();
                egui::TextEdit::multiline(&mut code.as_str()).desired_width(1.0e6).code_editor().show(ui);
            });
        });
    }
}
