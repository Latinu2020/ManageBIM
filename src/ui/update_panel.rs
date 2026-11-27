//! Updates: the check at startup / on request, the badge in the status bar and the window
//! "Update available" with downloading and starting the installer.
use super::*;

impl App {
    /// Starts the check (at startup, if enabled in settings; or manually).
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

    /// Called every frame: the automatic check once, then the results of the threads.
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

    /// The badge in the status bar when there is a new version.
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

impl App {
    /// The bell in the status bar: the recent messages (and the errors) remain readable after they disappear.
    pub(super) fn notes_bell(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as P;
        let unseen = self.notes.len().saturating_sub(self.notes_seen);
        let err = self.notes[self.notes_seen.min(self.notes.len())..].iter().any(|n| n.1);
        let col = if err { Color32::from_rgb(230, 85, 70) } else if unseen > 0 { accent() } else { ui.visuals().weak_text_color() };
        let label = if unseen > 0 { format!("{} {unseen}", P::BELL) } else { P::BELL.to_string() };
        let r = ui.menu_button(RichText::new(label).font(crate::theme::icon_font(14.0)).color(col), |ui| {
            self.notes_seen = self.notes.len();
            ui.set_min_width(420.0);
            ui.horizontal(|ui| {
                ui.strong(crate::i18n::tr("Notifications"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(!self.notes.is_empty(), egui::Button::new(crate::i18n::tr("Clear")).small()).clicked() {
                        self.notes.clear();
                        self.notes_seen = 0;
                    }
                });
            });
            ui.separator();
            if self.notes.is_empty() {
                ui.label(RichText::new(crate::i18n::tr("No messages yet.")).weak());
                return;
            }
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                for (t, e, m) in self.notes.iter().rev() {
                    let s = t.elapsed().as_secs();
                    let ago = if s < 60 { "now".to_string() } else if s < 3600 { format!("{} min", s / 60) } else { format!("{} h", s / 3600) };
                    ui.horizontal_top(|ui| {
                        let (ic, c) = if *e { (P::WARNING_CIRCLE, Color32::from_rgb(230, 85, 70)) } else { (P::CHECK_CIRCLE, Color32::from_rgb(80, 185, 110)) };
                        ui.label(RichText::new(ic).font(crate::theme::icon_font(14.0)).color(c));
                        ui.add_sized([340.0, 0.0], egui::Label::new(RichText::new(m).size(12.5)).wrap());
                        ui.label(RichText::new(ago).small().weak());
                    });
                    ui.add_space(2.0);
                }
            });
            if ui.button(crate::i18n::tr("Copy all")).clicked() {
                let all: Vec<String> = self.notes.iter().map(|(_, e, m)| format!("{}{m}", if *e { "ERROR: " } else { "" })).collect();
                ui.output_mut(|o| o.copied_text = all.join("\n"));
            }
        });
        r.response.on_hover_text(if unseen > 0 { format!("{unseen} new message(s) - click to read") } else { "Messages and errors".into() });
    }
}
