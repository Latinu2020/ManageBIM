//! Fereastra "AI connection": pornirea serverului MCP si configuratiile gata de copiat pentru
//! Claude Code, Claude Desktop, OpenCode, VS Code si clientii pentru modele locale (Ollama).
use super::*;
use egui_phosphor::regular as P;
use std::sync::atomic::Ordering;

const GREEN: Color32 = Color32::from_rgb(70, 185, 105);
const RED: Color32 = Color32::from_rgb(225, 80, 70);

/// buton cu iconita (fontul de iconite) + text
fn ij(icon: &str, text: &str, ui: &egui::Ui) -> egui::WidgetText {
    crate::theme::icon_job(icon, text, 14.0, ui.visuals().text_color()).into()
}

/// doar iconita
fn ib(icon: &str) -> RichText {
    RichText::new(icon).font(crate::theme::icon_font(15.0))
}

/// Comutator in stil iOS / Windows 11.
fn switch(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let size = egui::vec2(38.0, 20.0);
    let (r, mut resp) = ui.allocate_exact_size(size, egui::Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool(resp.id, *on);
    let bg = if *on { accent() } else { ui.visuals().widgets.inactive.bg_fill };
    ui.painter().rect_filled(r, 10.0, bg);
    let x = egui::lerp(r.left() + 10.0..=r.right() - 10.0, t);
    ui.painter().circle_filled(egui::pos2(x, r.center().y), 7.5, Color32::WHITE);
    resp
}

/// Cadru "card" cu titlu mic.
fn card<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::none()
        .fill(ui.visuals().faint_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .rounding(8.0)
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(crate::i18n::tr(title).to_uppercase()).size(11.0).strong().color(ui.visuals().weak_text_color()));
            ui.add_space(6.0);
            add(ui)
        })
        .inner
}

/// Bloc de cod cu butonul Copy in colt.
fn code_block(ui: &mut egui::Ui, text: &str) -> bool {
    let mut copied = false;
    egui::Frame::none()
        .fill(ui.visuals().extreme_bg_color)
        .rounding(6.0)
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let w = (ui.available_width() - 100.0).max(120.0);
                ui.add_sized([w, 0.0], egui::Label::new(RichText::new(text).monospace().size(12.0)).wrap());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if ui.button(ij(P::COPY, crate::i18n::tr("Copy"), ui)).clicked() {
                        ui.output_mut(|o| o.copied_text = text.to_string());
                        copied = true;
                    }
                });
            });
        });
    copied
}

impl App {
    pub(super) fn mcp_window(&mut self, ctx: &egui::Context) {
        if !self.mcp_open {
            return;
        }
        let mut open = true;
        let mut restart = false;
        let tab_id = egui::Id::new("mcp_client_tab");
        let show_tok_id = egui::Id::new("mcp_show_token");
        egui::Window::new(crate::theme::icon_job(P::ROBOT, crate::i18n::tr("AI connection"), 15.0, ctx.style().visuals.strong_text_color()))
            .open(&mut open)
            .resizable(true)
            .default_width(620.0)
            .min_width(480.0)
            .default_height(640.0)
            .collapsible(false)
            .show(ctx, |ui| {
                // ---- antet: descriere + starea serverului
                let (state, col) = match (&self.mcp, self.mcp_status.is_empty()) {
                    (Some(_), _) => (crate::i18n::tr("Running"), GREEN),
                    (None, false) => (crate::i18n::tr("Error"), RED),
                    (None, true) => (crate::i18n::tr("Off"), ui.visuals().weak_text_color()),
                };
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(crate::i18n::tr("Model Context Protocol server")).size(16.0).strong());
                        ui.label(RichText::new(crate::i18n::tr("Let AI assistants read and drive the open model.")).weak());
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::Frame::none().fill(col.gamma_multiply(0.18)).rounding(12.0).inner_margin(egui::Margin::symmetric(10.0, 4.0)).show(ui, |ui| {
                            ui.label(RichText::new(format!("â— {state}")).color(col).strong());
                        });
                    });
                });
                ui.add_space(8.0);

                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    // ---- serverul
                    card(ui, "Server", |ui| {
                        egui::Grid::new("mcp_srv").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
                            ui.label(crate::i18n::tr("Enable server"));
                            ui.horizontal(|ui| {
                                if switch(ui, &mut self.settings.mcp.enabled).changed() {
                                    self.mcp_status.clear();
                                    self.settings.save();
                                }
                                ui.label(RichText::new(crate::i18n::tr("Only this computer (127.0.0.1) can connect")).small().weak());
                            });
                            ui.end_row();

                            ui.label(crate::i18n::tr("Allow changes"));
                            ui.horizontal(|ui| {
                                if switch(ui, &mut self.settings.mcp.allow_edit).changed() {
                                    self.settings.save();
                                }
                                let t = if self.settings.mcp.allow_edit {
                                    "Assistants may select, hide, set properties, run clash tests and scripts"
                                } else {
                                    "Read-only: model info, search and properties"
                                };
                                ui.label(RichText::new(crate::i18n::tr(t)).small().weak());
                            });
                            ui.end_row();

                            ui.label(crate::i18n::tr("Port"));
                            ui.horizontal(|ui| {
                                let r = ui.add(egui::DragValue::new(&mut self.settings.mcp.port).range(1024..=65535));
                                if r.lost_focus() || r.drag_stopped() {
                                    self.settings.save();
                                    restart = true;
                                }
                                if ui.button(ij(P::ARROWS_CLOCKWISE, crate::i18n::tr("Restart"), ui)).clicked() {
                                    restart = true;
                                }
                            });
                            ui.end_row();

                            ui.label(crate::i18n::tr("Endpoint"));
                            ui.horizontal(|ui| {
                                let url = crate::mcp::url(self.settings.mcp.port);
                                ui.label(RichText::new(&url).monospace());
                                if ui.add(egui::Button::new(ib(P::COPY)).small()).on_hover_text(crate::i18n::tr("Copy")).clicked() {
                                    ui.output_mut(|o| o.copied_text = url.clone());
                                    self.toast("Endpoint copied.");
                                }
                            });
                            ui.end_row();

                            ui.label(crate::i18n::tr("Token"));
                            ui.horizontal(|ui| {
                                let mut show = ui.data(|d| d.get_temp::<bool>(show_tok_id).unwrap_or(false));
                                let shown = if show { self.settings.mcp.token.clone() } else { "â€¢".repeat(24) };
                                ui.label(RichText::new(shown).monospace());
                                if ui.add(egui::Button::new(ib(if show { P::EYE_SLASH } else { P::EYE })).small()).on_hover_text(crate::i18n::tr("Show / hide")).clicked() {
                                    show = !show;
                                    ui.data_mut(|d| d.insert_temp(show_tok_id, show));
                                }
                                if ui.add(egui::Button::new(ib(P::COPY)).small()).on_hover_text(crate::i18n::tr("Copy")).clicked() {
                                    ui.output_mut(|o| o.copied_text = self.settings.mcp.token.clone());
                                    self.toast("Token copied.");
                                }
                                if ui.add(egui::Button::new(ib(P::KEY)).small()).on_hover_text(crate::i18n::tr("New token - clients using the old token must be updated")).clicked() {
                                    self.settings.mcp.token = crate::mcp::new_token();
                                    self.settings.save();
                                    restart = true;
                                }
                            });
                            ui.end_row();

                            ui.label(crate::i18n::tr("Activity"));
                            match &self.mcp {
                                Some(s) => {
                                    let n = s.requests.load(Ordering::Relaxed);
                                    let c = s.client.lock().map(|c| c.clone()).unwrap_or_default();
                                    let who = if c.is_empty() { crate::i18n::tr("no client connected yet").to_string() } else { format!("{}: {c}", crate::i18n::tr("last client")) };
                                    ui.label(RichText::new(format!("{n} {} Â· {who}", crate::i18n::tr("requests"))).weak());
                                }
                                None if !self.mcp_status.is_empty() => {
                                    ui.label(RichText::new(&self.mcp_status).color(RED));
                                }
                                None => {
                                    ui.label(RichText::new(crate::i18n::tr("Server is off")).weak());
                                }
                            }
                            ui.end_row();
                        });
                    });
                    ui.add_space(10.0);

                    // ---- clientii: cate un tab, pasi numerotati, configuratia de copiat
                    let port = self.settings.mcp.port;
                    let token = self.settings.mcp.token.clone();
                    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "ManageBIM.exe".into());
                    let clients: [(&str, Vec<&str>, String); 5] = [
                        ("Claude Code", vec!["Open a terminal in any folder.", "Run the command below once.", "Ask Claude: \"what is in the open ManageBIM model?\""], crate::mcp::cfg_claude_code(port, &token)),
                        ("Claude Desktop", vec!["Claude Desktop â€º Settings â€º Developer â€º Edit Config.", "Paste the block into claude_desktop_config.json (merge with existing servers).", "Restart Claude Desktop. ManageBIM must be running with the server on."], crate::mcp::cfg_claude_desktop(&exe)),
                        ("OpenCode", vec!["Put this in opencode.json (project folder or ~/.config/opencode/).", "Works with any model, also local Ollama models that support tools (qwen3, llama3.1, mistral-small).", "Run opencode and ask about the model."], crate::mcp::cfg_opencode(port, &token)),
                        ("VS Code", vec!["Create .vscode/mcp.json in your workspace with this content.", "Open Copilot Chat in Agent mode and start the managebim server."], crate::mcp::cfg_vscode(port, &token)),
                        ("Other", vec!["Any client with Streamable HTTP MCP support (Cursor, Continue, LM Studio, Open WebUIâ€¦).", "Use this endpoint and header."], format!("{}\nAuthorization: Bearer {token}", crate::mcp::url(port))),
                    ];
                    let mut tab = ui.data(|d| d.get_temp::<usize>(tab_id).unwrap_or(0)).min(clients.len() - 1);
                    card(ui, "Connect a client", |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for (k, (name, _, _)) in clients.iter().enumerate() {
                                if ui.selectable_label(tab == k, RichText::new(*name).strong()).clicked() {
                                    tab = k;
                                }
                            }
                        });
                        ui.separator();
                        let (name, steps, cfg) = &clients[tab];
                        for (k, s) in steps.iter().enumerate() {
                            ui.horizontal_wrapped(|ui| {
                                egui::Frame::none().fill(accent().gamma_multiply(0.2)).rounding(9.0).inner_margin(egui::Margin::symmetric(6.0, 1.0)).show(ui, |ui| {
                                    ui.label(RichText::new(format!("{}", k + 1)).small().strong().color(accent()));
                                });
                                ui.label(crate::i18n::tr(s));
                            });
                        }
                        ui.add_space(6.0);
                        if code_block(ui, cfg) {
                            self.toast(format!("{name} configuration copied."));
                        }
                        if self.mcp.is_none() {
                            ui.add_space(4.0);
                            ui.label(crate::theme::icon_job(P::WARNING, crate::i18n::tr("Turn on the server above before connecting."), 14.0, Color32::from_rgb(230, 170, 60)));
                        }
                    });
                    ui.data_mut(|d| d.insert_temp(tab_id, tab));
                    ui.add_space(10.0);

                    // ---- uneltele disponibile
                    card(ui, "Tools available to the assistant", |ui| {
                        egui::Grid::new("mcp_tools").num_columns(2).spacing([12.0, 4.0]).striped(true).show(ui, |ui| {
                            for t in crate::mcp::tools() {
                                let name = t["name"].as_str().unwrap_or("");
                                let edit = crate::mcp::is_edit(name) || matches!(name, "select_elements" | "set_visibility" | "zoom_to");
                                let ic = if edit { P::PENCIL_SIMPLE } else { P::EYE };
                                let blocked = crate::mcp::is_edit(name) && !self.settings.mcp.allow_edit;
                                let title = t["title"].as_str().unwrap_or(name);
                                let col = if blocked { ui.visuals().weak_text_color() } else if edit { accent() } else { ui.visuals().strong_text_color() };
                                ui.label(crate::theme::icon_job(ic, title, 13.5, col)).on_hover_text(if blocked { crate::i18n::tr("Blocked: Allow changes is off") } else { name });
                                ui.add(egui::Label::new(RichText::new(t["description"].as_str().unwrap_or("")).small().weak()).wrap());
                                ui.end_row();
                            }
                        });
                    });
                });
            });
        if restart {
            self.mcp_restart(ctx);
        }
        self.mcp_open = open;
    }
}
