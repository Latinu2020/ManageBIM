//! Fereastra "AI connection": pornirea serverului MCP si configuratiile gata de copiat pentru
//! Claude Code, Claude Desktop, OpenCode, VS Code si clientii pentru modele locale (Ollama).
use super::*;
use std::sync::atomic::Ordering;

impl App {
    pub(super) fn mcp_window(&mut self, ctx: &egui::Context) {
        if !self.mcp_open {
            return;
        }
        let mut open = true;
        let mut restart = false;
        egui::Window::new(crate::i18n::tr("AI connection (MCP)"))
            .open(&mut open)
            .resizable(true)
            .default_width(560.0)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(crate::i18n::tr(
                    "Lets AI assistants read the open model and act on it: find elements, read properties, select, hide / isolate, set properties, run clash tests and scripts. The server listens only on this computer (127.0.0.1) and needs the token below.",
                ));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut self.settings.mcp.enabled, crate::i18n::tr("Enable MCP server")).changed() {
                        self.mcp_status.clear();
                        self.settings.save();
                    }
                    ui.separator();
                    ui.label(crate::i18n::tr("Port"));
                    let r = ui.add(egui::DragValue::new(&mut self.settings.mcp.port).range(1024..=65535));
                    if r.lost_focus() || r.drag_stopped() {
                        self.settings.save();
                        restart = true;
                    }
                    if ui.checkbox(&mut self.settings.mcp.allow_edit, crate::i18n::tr("Allow changes")).on_hover_text(crate::i18n::tr("Off = read-only: properties, selection queries and model info only")).changed() {
                        self.settings.save();
                    }
                });
                // stare
                let (txt, col) = match (&self.mcp, self.mcp_status.is_empty()) {
                    (Some(s), _) => {
                        let n = s.requests.load(Ordering::Relaxed);
                        let c = s.client.lock().map(|c| c.clone()).unwrap_or_default();
                        let who = if c.is_empty() { String::new() } else { format!(" · last client: {c}") };
                        (format!("● Running on {} · {n} request(s){who}", crate::mcp::url(s.port)), egui::Color32::from_rgb(80, 190, 110))
                    }
                    (None, false) => (format!("● Error: {}", self.mcp_status), egui::Color32::from_rgb(230, 80, 70)),
                    (None, true) => ("● Off".to_string(), ui.visuals().weak_text_color()),
                };
                ui.colored_label(col, txt);
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::tr("Token"));
                    let mut t = self.settings.mcp.token.clone();
                    ui.add(egui::TextEdit::singleline(&mut t).password(true).desired_width(260.0));
                    if ui.button(crate::i18n::tr("Copy")).clicked() {
                        ui.output_mut(|o| o.copied_text = self.settings.mcp.token.clone());
                        self.toast("Token copied.");
                    }
                    if ui.button(crate::i18n::tr("New token")).on_hover_text(crate::i18n::tr("Clients using the old token must be updated")).clicked() {
                        self.settings.mcp.token = crate::mcp::new_token();
                        self.settings.save();
                        restart = true;
                    }
                    if ui.button(crate::i18n::tr("Restart")).clicked() {
                        restart = true;
                    }
                });
                ui.separator();
                let port = self.settings.mcp.port;
                let token = self.settings.mcp.token.clone();
                let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "ManageBIM.exe".into());
                let blocks: [(&str, &str, String); 5] = [
                    ("Claude Code", "Run in a terminal (once):", crate::mcp::cfg_claude_code(port, &token)),
                    ("Claude Desktop", "Settings › Developer › Edit Config (claude_desktop_config.json); restart Claude Desktop:", crate::mcp::cfg_claude_desktop(&exe)),
                    ("OpenCode", "opencode.json (project folder or ~/.config/opencode/). Works with any model, also local Ollama models:", crate::mcp::cfg_opencode(port, &token)),
                    ("VS Code / Copilot", ".vscode/mcp.json:", crate::mcp::cfg_vscode(port, &token)),
                    ("Other clients", "Streamable HTTP endpoint + header (Cursor, Continue, LM Studio, Open WebUI ...):", format!("{}\nAuthorization: Bearer {token}", crate::mcp::url(port))),
                ];
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for (name, how, cfg) in blocks {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.strong(name);
                            if ui.small_button(crate::i18n::tr("Copy")).clicked() {
                                ui.output_mut(|o| o.copied_text = cfg.clone());
                                self.toast(format!("{name} configuration copied."));
                            }
                        });
                        ui.label(egui::RichText::new(crate::i18n::tr(how)).weak());
                        let mut c = cfg.clone();
                        ui.add(egui::TextEdit::multiline(&mut c).code_editor().desired_rows(1).desired_width(f32::INFINITY).interactive(true));
                    }
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(crate::i18n::tr(
                        "Local models (Ollama): Ollama itself has no MCP support - use a client that has it (OpenCode, Continue, Open WebUI, LM Studio) with an Ollama model that supports tools, e.g. qwen3, llama3.1, mistral-small.",
                    ))
                    .weak());
                    ui.label(egui::RichText::new(crate::i18n::tr("Claude Desktop starts ManageBIM.exe --mcp-stdio, a small bridge that talks to this window's server - ManageBIM must be running with the server on.")).weak());
                });
            });
        if restart {
            self.mcp_restart(ctx);
        }
        self.mcp_open = open;
    }
}
