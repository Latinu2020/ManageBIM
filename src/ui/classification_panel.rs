//! Panoul de clasificare BIM: catalog local (bSDD / Uniclass / OmniClass), cautare
//! si atribuire pe elementele selectate. Clasificarea se scrie ca proprietate
//! "Classification" in grupul sistemului si se exporta in IFC / IDS.
use super::*;

impl App {
    pub(super) fn classification_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Classification").small().strong().color(accent()))
                    .on_hover_text("Local bSDD / Uniclass / OmniClass catalogue. Assign a code to the selected elements; it is saved as a property and exported to IFC.");
                if ui.small_button("📥 CSV…").on_hover_text("Import a classification catalogue from CSV").clicked() {
                    self.request_path(Purpose::ImportClassifications);
                }
                if ui.small_button("📤 CSV…").on_hover_text("Export the classification catalogue to CSV").clicked() {
                    self.request_path(Purpose::ExportClassifications);
                }
                if ui.small_button("↺ Default").on_hover_text("Restore the built-in Uniclass / OmniClass list").clicked() {
                    self.settings.classifications = crate::classification::builtin();
                    self.settings.save();
                }
            });
        });
        let sel_count = self.selected.len();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Selected: {sel_count}")).small().color(if sel_count > 0 { accent() } else { ui.visuals().weak_text_color() }));
        });
        ui.add(egui::TextEdit::singleline(&mut self.class_search).hint_text("Search by code or name…").desired_width(f32::INFINITY));
        let q = self.class_search.trim().to_ascii_lowercase();
        let shown: Vec<usize> = (0..self.settings.classifications.len())
            .filter(|&i| {
                let c = &self.settings.classifications[i];
                q.is_empty() || c.code.to_ascii_lowercase().contains(&q) || c.name.to_ascii_lowercase().contains(&q) || c.system.to_ascii_lowercase().contains(&q)
            })
            .take(200)
            .collect();
        egui::ScrollArea::vertical().id_salt("classlist").max_height(ui.available_height().max(100.0)).show(ui, |ui| {
            for i in shown {
                let (code, name, system) = {
                    let c = &self.settings.classifications[i];
                    (c.code.clone(), c.name.clone(), c.system.clone())
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&code).monospace().small());
                    ui.label(RichText::new(&name).small());
                    ui.label(RichText::new(&system).small().weak());
                    let enabled = ui.add_enabled(sel_count > 0, egui::Button::new("➕").small())
                        .on_hover_text(format!("Assign {code} to the {sel_count} selected elements"));
                    if enabled.clicked() {
                        let v: Vec<_> = self.selected.iter().copied().collect();
                        self.set_user_prop(&v, &system, "Classification", &format!("{code} — {name}"));
                        self.toast(format!("{} assigned to {} element(s)", code, v.len()));
                    }
                });
            }
            if self.settings.classifications.is_empty() {
                ui.label(RichText::new("No classifications. Import a CSV or restore the default list.").weak().small());
            }
        });
    }
}
