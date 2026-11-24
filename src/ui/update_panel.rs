//! Actualizari: verificarea la pornire / la cerere, insigna din bara de stare si fereastra
//! "Update available" cu descarcarea si pornirea installerului.
use super::*;

impl App {
    /// Porneste verificarea (la pornire, daca e activata in setari; sau manual).
    pub(super) fn check_updates(&mut self, manual: bool) {
        if self.update_rx.is_some() {
            return;
        }
        self.update_manual = manual;
        self.update_rx = Some(crate::update::spawn_check());
        if manual {
            self.toast("Checking for updates…");
        }
    }

    /// Apelat in fiecare cadru: verificarea automata o data, apoi rezultatele firelor.
    pub(super) fn poll_updates(&mut self, ctx: &egui::Context) {
        if !self.update_checked {
            self.update_checked = true;
            if self.settings.check_updates {
                self.check_updates(false);
            }
        }
        if let Some(rx) = &self.update_rx {
            if let Ok(r) = rx.try_recv() {
                self.update_rx = None;
                match r {
                    Ok(Some(u)) => {
                        self.toast(format!("ManageBIM {} is available - click Update in the status bar", u.version));
                        self.update_info = Some(u);
                        if self.update_manual {
                            self.update_open = true;
                        }
                    }
                    Ok(None) => {
                        if self.update_manual {
                            self.toast(format!("ManageBIM {} is up to date", env!("CARGO_PKG_VERSION")));
                        }
                    }
                    Err(e) => {
                        if self.update_manual {
                            self.error(format!("Could not check for updates: {e}"));
                        }
                    }
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
        }
        if let Some(rx) = &self.update_dl_rx {
            if let Ok(r) = rx.try_recv() {
                self.update_dl_rx = None;
                match r {
                    Ok(p) => self.toast(format!("Installer started ({}) - save your work and close ManageBIM to finish the upgrade.", crate::file_name(&p.to_string_lossy()))),
                    Err(e) => self.error(e),
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
        }
    }

    /// Insigna din bara de stare cand exista o versiune noua.
    pub(super) fn update_badge(&mut self, ui: &mut egui::Ui) {
        let Some(u) = &self.update_info else { return };
        let txt = format!("⬆ Update {}", u.version);
        if ui.add(egui::Button::new(RichText::new(txt).small().strong().color(Color32::WHITE)).fill(accent()).rounding(8.0)).on_hover_text("A new version of ManageBIM is available").clicked() {
            self.update_open = true;
        }
    }

    pub(super) fn update_window(&mut self, ctx: &egui::Context) {
        if !self.update_open {
            return;
        }
        let Some(u) = self.update_info.clone() else {
            self.update_open = false;
            return;
        };
        let mut open = true;
        let mut install = false;
        let busy = self.update_dl_rx.is_some();
        egui::Window::new(crate::theme::icon_job("⬇", "Update available", 14.0, ctx.style().visuals.text_color()))
            .id(egui::Id::new("update_win"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .anchor(Align2::CENTER_CENTER, EV::ZERO)
            .show(ctx, |ui| {
                ui.label(RichText::new(format!("ManageBIM {} is available", u.version)).size(17.0).strong());
                ui.label(RichText::new(format!("You have {}. The installer upgrades in place: your settings, projects, materials and install folder are kept.", env!("CARGO_PKG_VERSION"))).small().weak());
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if busy {
                        ui.spinner();
                        ui.label("Downloading the installer…");
                        ctx.request_repaint_after(std::time::Duration::from_millis(300));
                    } else if ui.add_enabled(!u.setup_url.is_empty(), egui::Button::new(RichText::new("⬇ Download and install").strong().color(Color32::WHITE)).fill(accent())).clicked() {
                        install = true;
                    }
                    ui.hyperlink_to("What's new ↗", &u.page);
                });
                ui.add_space(4.0);
                ui.checkbox(&mut self.settings.check_updates, "Check for updates when ManageBIM starts");
            });
        if install {
            self.update_dl_rx = Some(crate::update::spawn_download_and_run(u));
        }
        if !open {
            self.update_open = false;
            self.settings.save();
        }
    }
}
