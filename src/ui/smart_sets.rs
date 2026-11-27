//! The "Smart set" window: search sets built from the property values (with the element count),
//! suggestions from the selection, search written in words and using the set directly in a clash test.
use super::*;
use crate::smart_sets::{common_props, parse_query, rules_for, suggest_name, Index, Key};
use egui_phosphor::regular as P;

fn icon(s: &str) -> RichText {
    RichText::new(s).font(crate::theme::icon_font(14.0))
}

impl App {
    /// the signature of the loaded models (the index is rebuilt when they change)
    fn smart_models_sig(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for (i, m) in self.loaded_models() {
            (i, m.elements.len(), m.name.as_str()).hash(&mut h);
        }
        h.finish()
    }

    pub(super) fn smart_sets_window(&mut self, ctx: &egui::Context) {
        if !self.smart.open {
            return;
        }
        // the property index: once per model set
        let sig = self.smart_models_sig();
        if self.smart.index.as_ref().map(|(s, _)| *s) != Some(sig) {
            let ix = Index::build(self.loaded_models().map(|(_, m)| m));
            self.smart.index = Some((sig, ix));
            self.smart.count = None;
        }
        // the element count: only when the choices change
        let csig = self.smart.sig() ^ sig;
        if self.smart.count.map(|(s, _)| s) != Some(csig) {
            let rules = rules_for(&self.smart.choices);
            let n = if rules.is_empty() { 0 } else { self.find(&rules).len() };
            self.smart.count = Some((csig, n));
        }
        if !self.smart.name_edited {
            self.smart.name = suggest_name(&self.smart.choices);
        }
        let mut open = true;
        let mut action: Option<u8> = None; // 1 select, 2 save, 3 save+exclude, 4 save as A, 5 save as B
        let mut exclude_test: Option<usize> = None;
        egui::Window::new(crate::theme::icon_job(P::SPARKLE, crate::i18n::tr("Smart set"), 15.0, ctx.style().visuals.strong_text_color()))
            .open(&mut open)
            .default_size([760.0, 600.0])
            .min_width(560.0)
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                let Some((_, ix)) = self.smart.index.as_ref() else { return };
                let ix = ix.clone();
                // ---- search in words
                ui.horizontal(|ui| {
                    ui.label(icon(P::MAGNIFYING_GLASS));
                    let r = ui.add(egui::TextEdit::singleline(&mut self.smart.query).hint_text(crate::i18n::tr("Describe it: ducts level 2 · walls, EI60 · valves cs150")).desired_width(ui.available_width() - 90.0));
                    let go = ui.button(crate::i18n::tr("Find")).clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    if go && !self.smart.query.trim().is_empty() {
                        let (c, missed) = parse_query(&self.smart.query, &ix);
                        self.smart.choices = c;
                        self.smart.missed = missed;
                        self.smart.cur = self.smart.choices.first().map(|c| c.0.clone());
                    }
                });
                // opened from Clash Detective: the exclusion from that test, with one click
                if let Some(ti) = self.smart.target_test.filter(|t| *t < self.clash.tests.len()) {
                    let tname = self.clash.tests[ti].name.clone();
                    let n = self.smart.count.map(|c| c.1).unwrap_or(0);
                    egui::Frame::none().fill(Color32::from_rgb(230, 120, 40).gamma_multiply(0.15)).rounding(6.0).inner_margin(egui::Margin::same(8.0)).show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(format!("{} \"{tname}\": {}", crate::i18n::tr("Ignore in"), crate::i18n::tr("tick what the items share below (type, system, level…), then"))).strong());
                            if ui.add_enabled(n > 0, egui::Button::new(RichText::new(format!("{} ({n})", crate::i18n::tr("Save & exclude"))).strong())).clicked() {
                                exclude_test = Some(ti);
                                action = Some(3);
                            }
                            if ui.small_button("✖").on_hover_text(crate::i18n::tr("Not for a clash test")).clicked() {
                                self.smart.target_test = None;
                            }
                        });
                    });
                }
                if !self.smart.missed.is_empty() {
                    ui.label(RichText::new(format!("{} {}", crate::i18n::tr("Not found in the model:"), self.smart.missed.join(", "))).small().color(Color32::from_rgb(230, 160, 60)));
                }
                // ---- suggestions from the selection
                if !self.selected.is_empty() {
                    let ids: Vec<ElemId> = self.selected.iter().copied().take(2000).collect();
                    let common = common_props(&self.models, &ids, &ix);
                    if !common.is_empty() {
                        ui.add_space(4.0);
                        ui.label(RichText::new(format!("{} ({} selected)", crate::i18n::tr("The selected items share"), self.selected.len())).small().weak());
                        ui.horizontal_wrapped(|ui| {
                            for (k, v, n) in common.iter().take(10) {
                                let on = self.smart.has(k, v);
                                let label = format!("{} = {}  ({n})", k.1, v);
                                if ui.selectable_label(on, label).on_hover_text(format!("{} › {}\n{n} items in the model have this value", k.0, k.1)).clicked() {
                                    self.smart.toggle(k, v);
                                    self.smart.cur = Some(k.clone());
                                }
                            }
                        });
                    }
                }
                ui.separator();
                // ---- properties | values
                let h = (ui.available_height() - 120.0).max(180.0);
                ui.columns(2, |c| {
                    let ui = &mut c[0];
                    ui.horizontal(|ui| {
                        ui.strong(crate::i18n::tr("Property"));
                        ui.add(egui::TextEdit::singleline(&mut self.smart.prop_filter).hint_text(crate::i18n::tr("Filter…")).desired_width(150.0));
                    });
                    let f = self.smart.prop_filter.to_lowercase();
                    egui::ScrollArea::vertical().id_salt("smart_props").max_height(h).auto_shrink([false, false]).show(ui, |ui| {
                        let mut keys: Vec<&Key> = ix.props.keys().filter(|k| ix.useful(k)).filter(|k| f.is_empty() || k.1.to_lowercase().contains(&f) || k.0.to_lowercase().contains(&f)).collect();
                        // the standard fields first, then by how many elements have the property
                        keys.sort_by(|a, b| (b.0 == ITEM_CAT).cmp(&(a.0 == ITEM_CAT)).then_with(|| ix.count.get(*b).cmp(&ix.count.get(*a))).then_with(|| a.cmp(b)));
                        for k in keys.into_iter().take(400) {
                            let n = ix.count.get(k).copied().unwrap_or(0);
                            let chosen = self.smart.choices.iter().any(|c| &c.0 == k);
                            let mut t = RichText::new(format!("{}  ", k.1));
                            if chosen {
                                t = t.strong().color(accent());
                            }
                            let r = ui.selectable_label(self.smart.cur.as_ref() == Some(k), t).on_hover_text(format!("{} › {}\n{n} items, {} values", k.0, k.1, ix.props.get(k).map(|m| m.len()).unwrap_or(0)));
                            if r.clicked() {
                                self.smart.cur = Some(k.clone());
                                self.smart.val_filter.clear();
                            }
                        }
                    });
                    let ui = &mut c[1];
                    match self.smart.cur.clone() {
                        None => {
                            ui.label(RichText::new(crate::i18n::tr("Pick a property on the left, then tick its values. Values of one property = OR; different properties = AND.")).weak());
                        }
                        Some(k) => {
                            ui.horizontal(|ui| {
                                ui.strong(format!("{} › {}", k.0, k.1));
                                ui.add(egui::TextEdit::singleline(&mut self.smart.val_filter).hint_text(crate::i18n::tr("Filter…")).desired_width(130.0));
                            });
                            let vf = self.smart.val_filter.to_lowercase();
                            egui::ScrollArea::vertical().id_salt("smart_vals").max_height(h).auto_shrink([false, false]).show(ui, |ui| {
                                for (v, n) in ix.values(&k).into_iter().filter(|(v, _)| vf.is_empty() || v.to_lowercase().contains(&vf)).take(1000) {
                                    let mut on = self.smart.has(&k, &v);
                                    if ui.checkbox(&mut on, format!("{v}   ({n})")).changed() {
                                        self.smart.toggle(&k, &v);
                                    }
                                }
                            });
                        }
                    }
                });
                ui.separator();
                // ---- the chosen conditions
                ui.horizontal_wrapped(|ui| {
                    if self.smart.choices.is_empty() {
                        ui.label(RichText::new(crate::i18n::tr("No conditions yet.")).weak());
                    }
                    let mut drop = None;
                    for (i, (k, v)) in self.smart.choices.iter().enumerate() {
                        if i > 0 {
                            ui.label(RichText::new("AND").small().weak());
                        }
                        egui::Frame::none().fill(accent().gamma_multiply(0.15)).rounding(10.0).inner_margin(egui::Margin::symmetric(8.0, 2.0)).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("{}: {}", k.1, v.join(" OR "))).color(accent()));
                                if ui.small_button("✖").clicked() {
                                    drop = Some(i);
                                }
                            });
                        });
                    }
                    if let Some(i) = drop {
                        self.smart.choices.remove(i);
                    }
                });
                // ---- result and saving
                let n = self.smart.count.map(|c| c.1).unwrap_or(0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{n} {}", crate::i18n::tr("items"))).strong().size(15.0));
                    ui.label(crate::i18n::tr("Name"));
                    if ui.add(egui::TextEdit::singleline(&mut self.smart.name).desired_width(220.0)).changed() {
                        self.smart.name_edited = true;
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    let can = n > 0;
                    if ui.add_enabled(can, egui::Button::new(crate::i18n::tr("Select"))).clicked() {
                        action = Some(1);
                    }
                    if ui.add_enabled(can, egui::Button::new(RichText::new(crate::i18n::tr("Save search set")).strong())).on_hover_text(crate::i18n::tr("Saved with the project; updates itself when the models change")).clicked() {
                        action = Some(2);
                    }
                    let tests: Vec<String> = self.clash.tests.iter().map(|t| t.name.clone()).collect();
                    ui.add_enabled_ui(can && !tests.is_empty(), |ui| {
                        ui.menu_button(crate::i18n::tr("Save & exclude from clash test ⏷"), |ui| {
                            for (i, t) in tests.iter().enumerate() {
                                if ui.button(t).clicked() {
                                    exclude_test = Some(i);
                                    action = Some(3);
                                    ui.close_menu();
                                }
                            }
                        });
                        if ui.button(crate::i18n::tr("Save as Selection A")).on_hover_text(crate::i18n::tr("Of the current clash test")).clicked() {
                            action = Some(4);
                        }
                        if ui.button(crate::i18n::tr("Save as Selection B")).on_hover_text(crate::i18n::tr("Of the current clash test")).clicked() {
                            action = Some(5);
                        }
                    });
                    if ui.button(crate::i18n::tr("Clear")).clicked() {
                        self.smart.choices.clear();
                        self.smart.query.clear();
                        self.smart.missed.clear();
                        self.smart.name_edited = false;
                    }
                });
            });
        self.smart.open = open;
        if !open {
            self.smart.target_test = None;
        }
        let Some(a) = action else { return };
        let rules = rules_for(&self.smart.choices);
        if a == 1 {
            let ids = self.find(&rules);
            self.apply(Action::Select(ids, false));
            return;
        }
        // the unique name
        let mut name = self.smart.name.trim().to_string();
        if name.is_empty() {
            name = "New set".into();
        }
        let base = name.clone();
        let mut k = 2;
        while self.sets.iter().any(|s| s.name == name) {
            name = format!("{base} ({k})");
            k += 1;
        }
        self.sets.push(SelSet { name: name.clone(), kind: SetKind::Search(rules) });
        let msg = match a {
            3 => {
                let ti = exclude_test.unwrap_or(self.cur_test);
                if let Some(t) = self.clash.tests.get_mut(ti) {
                    if !t.rules.exclude_sets.contains(&name) {
                        t.rules.exclude_sets.push(name.clone());
                    }
                    format!("Set \"{name}\" saved and excluded from clash test \"{}\" - run the test again to update it.", t.name)
                } else {
                    format!("Set \"{name}\" saved.")
                }
            }
            4 | 5 => {
                let ti = self.cur_test.min(self.clash.tests.len().saturating_sub(1));
                if let Some(t) = self.clash.tests.get_mut(ti) {
                    let sel = Selector { kind: SelKind::Set, value: name.clone() };
                    if a == 4 {
                        t.a = sel;
                    } else {
                        t.b = sel;
                    }
                    format!("Set \"{name}\" is now Selection {} of \"{}\".", if a == 4 { "A" } else { "B" }, t.name)
                } else {
                    format!("Set \"{name}\" saved.")
                }
            }
            _ => format!("Search set \"{name}\" saved ({} items).", self.smart.count.map(|c| c.1).unwrap_or(0)),
        };
        self.toast(msg);
        self.smart.target_test = None;
        self.smart.name_edited = false;
        self.open_panel(Panel::Sets);
    }
}
