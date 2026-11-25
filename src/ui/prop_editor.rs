//! Fereastra "Property editor": alegi elemente (in 3D sau in arbore) si le adaugi proprietati -
//! fie libere (set + nume + valoare), fie din seturile standard IFC (Pset_WallCommon, Pset_DoorCommon,
//! Pset_ManufacturerTypeInformation...) propuse dupa clasa IFC a elementelor. Totul ajunge in exportul IFC
//! (seturile standard cu tipul corect). Valorile originale din model nu se pierd: o suprascriere stearsa
//! readuce valoarea initiala.
use super::*;
use crate::ifc_psets::{psets_for, PKind, PSETS};
use std::collections::{BTreeMap, BTreeSet};

/// Starea ferestrei (in App ca un singur camp).
#[derive(Default)]
pub struct PropEditor {
    pub open: bool,
    /// 0 = IFC standard, 1 = Custom
    pub tab: u8,
    pub group: String,
    pub rows: Vec<(String, String)>,
    pub pset: String,
    pub vals: BTreeMap<String, String>,
    /// valorile gasite pe selectie la incarcare (se aplica doar ce difera de ele)
    pub loaded: BTreeMap<String, String>,
    /// semnatura selectiei pentru care s-au incarcat valorile
    pub sig: u64,
    /// toate seturile, nu doar cele potrivite clasei
    pub all: bool,
}

impl App {
    /// Deschide fereastra (din ribbon / Properties).
    pub(super) fn open_prop_editor(&mut self) {
        self.pe.open = true;
        if self.pe.group.trim().is_empty() {
            self.pe.group = "User Data".into();
        }
        if self.pe.rows.is_empty() {
            self.pe.rows.push((String::new(), String::new()));
        }
    }

    /// Elementele tinta: selectia, fara obiectele de coordonare (care au proprietatile lor).
    fn pe_targets(&self) -> Vec<ElemId> {
        self.selected.iter().copied().filter(|id| Some(id.m as usize) != self.coord_slot && self.elem(*id).is_some()).collect()
    }

    /// Valoarea comuna a proprietatii pe tinte: Some(v) daca toate au aceeasi, None daca difera / lipseste.
    fn pe_common(&self, ids: &[ElemId], group: &str, name: &str) -> (Option<String>, bool) {
        let mut val: Option<String> = None;
        let mut varies = false;
        for id in ids {
            let v = self.elem(*id).and_then(|(_, e)| e.props.iter().find(|p| p.group == group && p.name.eq_ignore_ascii_case(name)).map(|p| p.value.clone())).unwrap_or_default();
            match &val {
                None => val = Some(v),
                Some(x) if *x != v => varies = true,
                _ => {}
            }
        }
        if varies { (None, true) } else { (val.filter(|v| !v.is_empty()), false) }
    }

    pub(super) fn prop_editor_window(&mut self, ctx: &egui::Context) {
        if !self.pe.open {
            return;
        }
        let ids = self.pe_targets();
        // clasele IFC ale selectiei (pentru seturile standard)
        let mut classes: BTreeMap<String, usize> = BTreeMap::new();
        for id in &ids {
            if let Some((_, e)) = self.elem(*id) {
                let c = if e.ifc_type.trim().is_empty() { "(no class)".to_string() } else { e.ifc_type.clone() };
                *classes.entry(c).or_default() += 1;
            }
        }
        let sig = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            ids.hash(&mut h);
            self.pe.pset.hash(&mut h);
            h.finish()
        };
        // seturile propuse: ale claselor selectate, apoi cele generale
        let mut offered: Vec<&'static crate::ifc_psets::PsetDef> = Vec::new();
        if self.pe.all || classes.is_empty() {
            offered.extend(PSETS.iter());
        } else {
            for c in classes.keys() {
                for p in psets_for(c) {
                    if !offered.iter().any(|o| o.name == p.name) {
                        offered.push(p);
                    }
                }
            }
        }
        if !offered.iter().any(|p| p.name == self.pe.pset) {
            self.pe.pset = offered.first().map(|p| p.name.to_string()).unwrap_or_default();
        }
        // valorile curente se reincarca cand se schimba selectia sau setul
        if sig != self.pe.sig {
            self.pe.sig = sig;
            self.pe.vals.clear();
            self.pe.loaded.clear();
            if let Some(def) = PSETS.iter().find(|p| p.name == self.pe.pset) {
                for d in def.props {
                    if let (Some(v), _) = self.pe_common(&ids, def.name, d.name) {
                        self.pe.vals.insert(d.name.to_string(), v.clone());
                        self.pe.loaded.insert(d.name.to_string(), v);
                    }
                }
            }
        }
        let mut open = true;
        let mut apply_custom = false;
        let mut apply_std = false;
        let mut remove: Option<(String, String)> = None;
        egui::Window::new(crate::theme::icon_job("🏷", "Property editor", 14.0, ctx.style().visuals.text_color()))
            .id(egui::Id::new("prop_editor_win"))
            .open(&mut open)
            .default_size([520.0, 520.0])
            .min_width(440.0)
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                // ---- tinta
                egui::Frame::none().fill(card_fill(ui.visuals())).rounding(6.0).inner_margin(8.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if ids.is_empty() {
                        ui.label(RichText::new("No elements selected").strong());
                        ui.label(RichText::new("Pick elements in the 3D view or in the tree (Ctrl+click adds more). This window stays open while you pick.").small().weak());
                    } else {
                        ui.label(RichText::new(format!("{} element(s) selected", ids.len())).strong());
                        let summary = classes.iter().map(|(c, n)| format!("{c} ×{n}")).collect::<Vec<_>>().join(" · ");
                        ui.label(RichText::new(summary).small().weak());
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.pe.tab, 0, RichText::new("IFC standard").strong()).on_hover_text("buildingSMART property sets for the selected IFC classes - written to IFC with the correct data type");
                    ui.selectable_value(&mut self.pe.tab, 1, RichText::new("Custom").strong()).on_hover_text("Your own property set, names and values");
                });
                ui.separator();
                if self.pe.tab == 0 {
                    ui.horizontal(|ui| {
                        ui.label("Property set");
                        egui::ComboBox::from_id_salt("pe_pset").width(270.0).selected_text(self.pe.pset.clone()).show_ui(ui, |ui| {
                            for p in &offered {
                                ui.selectable_value(&mut self.pe.pset, p.name.to_string(), p.name);
                            }
                        });
                        ui.checkbox(&mut self.pe.all, "All sets").on_hover_text("Show every standard set, not only the ones for the selected classes");
                    });
                    let Some(def) = PSETS.iter().find(|p| p.name == self.pe.pset) else { return };
                    ui.label(RichText::new("Empty fields are left as they are. ‹varies› = the selected elements have different values.").small().weak());
                    egui::ScrollArea::vertical().id_salt("pe_std").max_height(ui.available_height() - 40.0).auto_shrink([false, true]).show(ui, |ui| {
                        egui::Grid::new("pe_std_grid").num_columns(2).spacing([12.0, 5.0]).striped(true).show(ui, |ui| {
                            for d in def.props {
                                ui.label(d.name).on_hover_text(format!("{} · {}", def.name, d.kind.hint()));
                                let (_, varies) = self.pe_common(&ids, def.name, d.name);
                                let v = self.pe.vals.entry(d.name.to_string()).or_default();
                                match d.kind {
                                    PKind::Bool | PKind::Enum(_) => {
                                        let opts: Vec<&str> = match d.kind {
                                            PKind::Enum(o) => o.to_vec(),
                                            _ => vec!["TRUE", "FALSE"],
                                        };
                                        let shown = if v.is_empty() { if varies { "‹varies›".to_string() } else { "—".to_string() } } else { v.clone() };
                                        egui::ComboBox::from_id_salt(("pe_v", d.name)).width(200.0).selected_text(shown).show_ui(ui, |ui| {
                                            ui.selectable_value(v, String::new(), "— (leave as is)");
                                            for o in opts {
                                                ui.selectable_value(v, o.to_string(), o);
                                            }
                                        });
                                    }
                                    _ => {
                                        let hint = if varies { "‹varies›".to_string() } else { d.kind.hint().to_string() };
                                        ui.add_sized([220.0, 22.0], egui::TextEdit::singleline(v).hint_text(hint));
                                    }
                                }
                                ui.end_row();
                            }
                        });
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let n = self.pe.vals.iter().filter(|(k, v)| !v.trim().is_empty() && self.pe.loaded.get(*k) != Some(*v)).count();
                        if ui.add_enabled(!ids.is_empty() && n > 0, egui::Button::new(RichText::new(format!("✔ Apply {n} value(s) to {} element(s)", ids.len())).strong())).clicked() {
                            apply_std = true;
                        }
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Property set");
                        ui.add(egui::TextEdit::singleline(&mut self.pe.group).desired_width(220.0).hint_text("User Data"));
                        // tab-urile existente pe selectie
                        let mut existing: BTreeSet<String> = BTreeSet::new();
                        for id in &ids {
                            for (t, _) in self.user_tabs_of(*id) {
                                existing.insert(t);
                            }
                        }
                        if !existing.is_empty() {
                            ui.menu_button("⏷", |ui| {
                                for t in existing {
                                    if ui.button(&t).clicked() {
                                        self.pe.group = t;
                                        ui.close_menu();
                                    }
                                }
                            });
                        }
                    });
                    ui.add_space(2.0);
                    let mut del_row: Option<usize> = None;
                    egui::Grid::new("pe_rows").num_columns(3).spacing([8.0, 4.0]).show(ui, |ui| {
                        ui.label(RichText::new("Name").small().weak());
                        ui.label(RichText::new("Value").small().weak());
                        ui.label("");
                        ui.end_row();
                        for (i, (n, v)) in self.pe.rows.iter_mut().enumerate() {
                            ui.add_sized([170.0, 22.0], egui::TextEdit::singleline(n).hint_text("e.g. Zone"));
                            ui.add_sized([220.0, 22.0], egui::TextEdit::singleline(v).hint_text("value"));
                            if ui.small_button("✖").on_hover_text("Remove this row").clicked() {
                                del_row = Some(i);
                            }
                            ui.end_row();
                        }
                    });
                    if let Some(i) = del_row {
                        self.pe.rows.remove(i);
                    }
                    ui.horizontal(|ui| {
                        if ui.button("➕ Row").clicked() {
                            self.pe.rows.push((String::new(), String::new()));
                        }
                        let n = self.pe.rows.iter().filter(|(n, _)| !n.trim().is_empty()).count();
                        if ui.add_enabled(!ids.is_empty() && n > 0 && !self.pe.group.trim().is_empty(), egui::Button::new(RichText::new(format!("✔ Apply to {} element(s)", ids.len())).strong())).clicked() {
                            apply_custom = true;
                        }
                    });
                    // ce a adaugat deja utilizatorul pe selectie
                    ui.separator();
                    ui.label(RichText::new("Added on the selection").small().strong());
                    let mut rows: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
                    for id in &ids {
                        for (t, props) in self.user_tabs_of(*id) {
                            for (n, v) in props {
                                rows.entry((t.clone(), n)).or_default().insert(v);
                            }
                        }
                    }
                    if rows.is_empty() {
                        ui.label(RichText::new("Nothing yet.").small().weak());
                    }
                    egui::ScrollArea::vertical().id_salt("pe_added").max_height(160.0).auto_shrink([false, true]).show(ui, |ui| {
                        for ((t, n), vals) in &rows {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(t).small().weak());
                                ui.label(RichText::new(n).strong());
                                let v = if vals.len() == 1 { vals.iter().next().cloned().unwrap_or_default() } else { "‹varies›".into() };
                                ui.label(v);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button("🗑").on_hover_text("Remove from the selected elements (an original value comes back)").clicked() {
                                        remove = Some((t.clone(), n.clone()));
                                    }
                                });
                            });
                        }
                    });
                }
            });
        self.pe.open = open;
        if apply_custom {
            let g = self.pe.group.trim().to_string();
            let rows: Vec<(String, String)> = self.pe.rows.iter().filter(|(n, _)| !n.trim().is_empty()).cloned().collect();
            for (n, v) in &rows {
                self.set_user_prop(&ids, &g, n, v);
            }
            self.toast(format!("{} propert{} written to {} element(s) in '{g}' (exported to IFC as a property set)", rows.len(), if rows.len() == 1 { "y" } else { "ies" }, ids.len()));
            self.pe.rows = vec![(String::new(), String::new())];
            self.pe.sig = 0;
        }
        if apply_std {
            let g = self.pe.pset.clone();
            let vals: Vec<(String, String)> = self.pe.vals.iter().filter(|(k, v)| !v.trim().is_empty() && self.pe.loaded.get(*k) != Some(*v)).map(|(k, v)| (k.clone(), v.trim().to_string())).collect();
            for (n, v) in &vals {
                self.set_user_prop(&ids, &g, n, v);
            }
            self.toast(format!("{g}: {} value(s) written to {} element(s)", vals.len(), ids.len()));
            self.pe.sig = 0;
        }
        if let Some((t, n)) = remove {
            self.remove_user_param(&ids, &t, &n);
            self.pe.sig = 0;
        }
    }
}
