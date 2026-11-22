//! ManageBIM-Setup.exe - un singur fisier: aplicatia, OpenCascade, exemplele si add-in-urile (Revit, AutoCAD /
//! Plant 3D, Navisworks). Instaleaza pentru utilizatorul curent, FARA drepturi de administrator:
//! despacheteaza pachetul intr-un folder temporar si ruleaza install.ps1 (aceeasi logica ca Install-ManageBIM.cmd).
//!   ManageBIM-Setup.exe                asistent grafic: Bun venit > Componente > Instalare > Finish
//!   ManageBIM-Setup.exe /S             fara intrebari (instalare silentioasa)
//!   ManageBIM-Setup.exe /NoAddins      doar aplicatia
//!   ManageBIM-Setup.exe /TestRoot=dir  totul intr-un folder de test (fara registru) - pentru verificare
#![windows_subsystem = "windows"]

use eframe::egui::{self, Color32, RichText};
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

static PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.bin"));
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(windows)]
mod win {
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(hwnd: *mut core::ffi::c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    pub const ICON_ERROR: u32 = 0x10;
    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
    pub fn msg(text: &str, kind: u32) -> i32 {
        unsafe { MessageBoxW(std::ptr::null_mut(), w(text).as_ptr(), w("ManageBIM Setup").as_ptr(), kind | 0x10000) }
    }
}

fn unpack(dest: &Path) -> Result<usize, String> {
    let b = PAYLOAD;
    if b.len() < 9 || &b[..5] != b"MBPK1" {
        return Err("The installer is empty or damaged (built without a package).".into());
    }
    let n = u32::from_le_bytes(b[5..9].try_into().unwrap()) as usize;
    let mut p = 9;
    let rd = |p: &mut usize, k: usize| -> Result<&[u8], String> {
        let s = b.get(*p..*p + k).ok_or("damaged installer")?;
        *p += k;
        Ok(s)
    };
    for _ in 0..n {
        let nl = u16::from_le_bytes(rd(&mut p, 2)?.try_into().unwrap()) as usize;
        let name = String::from_utf8_lossy(rd(&mut p, nl)?).to_string();
        let orig = u64::from_le_bytes(rd(&mut p, 8)?.try_into().unwrap()) as usize;
        let cl = u64::from_le_bytes(rd(&mut p, 8)?.try_into().unwrap()) as usize;
        let comp = rd(&mut p, cl)?;
        if name.contains("..") || name.starts_with('/') {
            return Err(format!("invalid path in package: {name}"));
        }
        let data = miniz_oxide::inflate::decompress_to_vec_with_limit(comp, orig).map_err(|e| format!("{name}: {e:?}"))?;
        let path = dest.join(name.replace('/', "\\"));
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, data).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(n)
}

/// Ce instalam.
#[derive(Clone)]
struct Options {
    /// None = toate add-in-urile gasite; Some(lista) = doar acestea (revit / acad / navis); lista goala = niciunul
    addins: Option<Vec<&'static str>>,
    desktop: bool,
    test_root: Option<PathBuf>,
}

struct Paths {
    local: PathBuf,
    app_dir: PathBuf,
}

/// Despacheteaza si ruleaza install.ps1; fiecare linie de iesire ajunge la `on_line` (progres in fereastra).
fn install(paths: &Paths, opt: &Options, on_line: &dyn Fn(String)) -> Result<String, String> {
    let tmp = std::env::temp_dir().join(format!("ManageBIM-setup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let result = (|| -> Result<String, String> {
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        on_line("Unpacking the files…".into());
        let n = unpack(&tmp)?;
        on_line(format!("{n} files unpacked"));
        let script = tmp.join("install.ps1");
        let mut c = std::process::Command::new("powershell.exe");
        c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(&script);
        match &opt.addins {
            Some(v) if v.is_empty() => {
                c.arg("-NoAddins");
            }
            Some(v) => {
                c.arg("-Addins").arg(v.join(","));
            }
            None => {}
        }
        if !opt.desktop {
            c.arg("-NoDesktop");
        }
        if let Some(t) = &opt.test_root {
            c.arg("-TestRoot").arg(t);
        }
        c.current_dir(&tmp).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000); // fara fereastra de consola
        }
        let mut child = c.spawn().map_err(|e| format!("PowerShell could not be started: {e}"))?;
        // stderr pe un fir separat (altfel un buffer plin poate bloca procesul)
        let ep = child.stderr.take();
        let et = std::thread::spawn(move || {
            let mut s = String::new();
            if let Some(mut e) = ep {
                let _ = e.read_to_string(&mut s);
            }
            s
        });
        let mut log = String::new();
        if let Some(out) = child.stdout.take() {
            for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
                let l = line.trim().to_string();
                if !l.is_empty() {
                    on_line(l.clone());
                }
                log.push_str(&line);
                log.push('\n');
            }
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        let err = et.join().unwrap_or_default();
        let _ = std::fs::create_dir_all(&paths.local);
        let _ = std::fs::write(paths.local.join("ManageBIM-setup.log"), format!("{log}\n{err}"));
        if !status.success() || !paths.app_dir.join("ManageBIM.exe").is_file() {
            let tail: Vec<&str> = err.lines().chain(log.lines()).filter(|l| !l.trim().is_empty()).take(12).collect();
            return Err(tail.join("\n"));
        }
        // avertismentele PowerShell (stderr) apar si ele in rezumat
        for l in err.lines().filter(|l| l.contains("WARNING")) {
            log.push_str(l);
            log.push('\n');
        }
        Ok(log)
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

// ------------------------------------------------------------------ ce e instalat pe PC

struct Found {
    revit: Vec<String>,
    acad: Vec<String>,
    navis: Vec<String>,
}

fn find_programs() -> Found {
    let pf = PathBuf::from(std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".into())).join("Autodesk");
    let dirs: Vec<String> = std::fs::read_dir(&pf).map(|rd| rd.flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().to_string()).collect()).unwrap_or_default();
    let pick = |prefix: &str, strip: &str| {
        // doar folderele cu un an in nume (ex. "AutoCAD 2025", "Navisworks Manage 2025"), nu "AutoCAD Activity Insights"
        let mut v: Vec<String> = dirs.iter().filter(|d| d.starts_with(prefix) && d.split_whitespace().any(|w| w.len() == 4 && w.starts_with("20") && w.chars().all(|c| c.is_ascii_digit()))).map(|d| d.trim_start_matches(strip).trim().to_string()).collect();
        v.sort();
        v
    };
    let revit: Vec<String> = (2023..=2027).map(|y| y.to_string()).filter(|y| dirs.iter().any(|d| d == &format!("Revit {y}"))).collect();
    Found { revit, acad: pick("AutoCAD", "AutoCAD"), navis: pick("Navisworks", "Navisworks") }
}

/// Versiunea deja instalata (din uninstall.ps1: $version = 'x.y.z').
fn installed_version(app_dir: &Path) -> Option<String> {
    if !app_dir.join("ManageBIM.exe").is_file() {
        return None;
    }
    let s = std::fs::read_to_string(app_dir.join("uninstall.ps1")).unwrap_or_default();
    let v = s.lines().find_map(|l| l.trim().strip_prefix("$version = '").and_then(|r| r.split('\'').next()).map(|v| v.to_string()));
    Some(v.unwrap_or_else(|| "an earlier version".into()))
}

// ------------------------------------------------------------------ asistentul grafic

#[derive(PartialEq, Clone, Copy)]
enum Page {
    Welcome,
    Components,
    Installing,
    Finish,
}

#[derive(Default)]
struct Job {
    lines: Vec<String>,
    done: Option<Result<String, String>>,
}

struct Wizard {
    page: Page,
    paths: Arc<Paths>,
    test_root: Option<PathBuf>,
    old: Option<String>,
    found: Found,
    revit: bool,
    acad: bool,
    navis: bool,
    desktop: bool,
    launch: bool,
    running_programs: Vec<String>,
    job: Arc<Mutex<Job>>,
    started: bool,
    pub launch_now: Arc<Mutex<bool>>,
}

const NORD_BG: Color32 = Color32::from_rgb(46, 52, 64);
const NORD_PANEL: Color32 = Color32::from_rgb(59, 66, 82);
const NORD_ACCENT: Color32 = Color32::from_rgb(136, 192, 208);
const NORD_GREEN: Color32 = Color32::from_rgb(163, 190, 140);
const NORD_YELLOW: Color32 = Color32::from_rgb(235, 203, 139);
const NORD_RED: Color32 = Color32::from_rgb(191, 97, 106);
const NORD_TEXT: Color32 = Color32::from_rgb(229, 233, 240);

fn running(names: &[&str]) -> Vec<String> {
    // tasklist (fara consola): programele care trebuie inchise inainte de actualizare
    let mut c = std::process::Command::new("tasklist.exe");
    c.args(["/FO", "CSV", "/NH"]).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let out = c.output().map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase()).unwrap_or_default();
    names.iter().filter(|n| out.contains(&format!("\"{}\"", n.to_lowercase()))).map(|n| n.trim_end_matches(".exe").to_string()).collect()
}

impl Wizard {
    fn new(paths: Paths, test_root: Option<PathBuf>, no_addins: bool) -> Wizard {
        let old = installed_version(&paths.app_dir);
        let found = find_programs();
        let (revit, acad, navis) = (!no_addins && !found.revit.is_empty(), !no_addins && !found.acad.is_empty(), !no_addins && !found.navis.is_empty());
        Wizard {
            page: Page::Welcome,
            paths: Arc::new(paths),
            test_root,
            old,
            found,
            revit,
            acad,
            navis,
            desktop: true,
            launch: true,
            running_programs: Vec::new(),
            job: Arc::new(Mutex::new(Job::default())),
            started: false,
            launch_now: Arc::new(Mutex::new(false)),
        }
    }

    fn start(&mut self, ctx: &egui::Context) {
        if self.started {
            return;
        }
        self.started = true;
        let mut list = Vec::new();
        if self.revit {
            list.push("revit");
        }
        if self.acad {
            list.push("acad");
        }
        if self.navis {
            list.push("navis");
        }
        let opt = Options { addins: Some(list), desktop: self.desktop, test_root: self.test_root.clone() };
        let (job, paths, ctx) = (self.job.clone(), self.paths.clone(), ctx.clone());
        std::thread::spawn(move || {
            let j2 = job.clone();
            let c2 = ctx.clone();
            let r = install(&paths, &opt, &move |l| {
                j2.lock().unwrap().lines.push(l);
                c2.request_repaint();
            });
            job.lock().unwrap().done = Some(r);
            ctx.request_repaint();
        });
    }

    fn steps(&self, ui: &mut egui::Ui) {
        ui.add_space(18.0);
        ui.label(RichText::new("ManageBIM").size(22.0).strong().color(NORD_ACCENT));
        ui.label(RichText::new(format!("version {VERSION}")).color(NORD_TEXT.gamma_multiply(0.7)));
        ui.add_space(26.0);
        for (p, name) in [(Page::Welcome, "Welcome"), (Page::Components, "Components"), (Page::Installing, "Install"), (Page::Finish, "Finish")] {
            let cur = p == self.page;
            let done = (p as u8) < (self.page as u8);
            let mark = if done { "✔" } else if cur { "▶" } else { "•" };
            let col = if cur { NORD_ACCENT } else if done { NORD_GREEN } else { NORD_TEXT.gamma_multiply(0.55) };
            ui.label(RichText::new(format!("{mark}  {name}")).size(15.0).color(col).strong());
            ui.add_space(6.0);
        }
    }

    fn page_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let title = |ui: &mut egui::Ui, t: &str| {
            ui.add_space(14.0);
            ui.label(RichText::new(t).size(21.0).strong().color(NORD_TEXT));
            ui.add_space(10.0);
        };
        match self.page {
            Page::Welcome => {
                match &self.old {
                    Some(v) if v == VERSION => {
                        title(ui, "Reinstall ManageBIM");
                        ui.label(RichText::new(format!("ManageBIM {v} is already installed on this PC.\nIt will be reinstalled (repair).")).size(14.0));
                        ui.add_space(6.0);
                        ui.label("Your settings, projects and models are kept. The add-ins are reinstalled too.");
                    }
                    Some(v) => {
                        title(ui, "Upgrade ManageBIM");
                        ui.label(RichText::new(format!("ManageBIM {v} is installed on this PC.\nIt will be upgraded to {VERSION}.")).size(14.0));
                        ui.add_space(6.0);
                        ui.label("Your settings, projects and models are kept. The add-ins are updated too.");
                    }
                    None => {
                        title(ui, "Welcome to ManageBIM Setup");
                        ui.label(RichText::new(format!("This wizard installs ManageBIM {VERSION} for {}.", std::env::var("USERNAME").unwrap_or_default())).size(14.0));
                    }
                }
                ui.add_space(10.0);
                let card = |ui: &mut egui::Ui, icon: &str, title: &str, desc: &str, col: Color32| {
                    egui::Frame::none()
                        .fill(NORD_PANEL)
                        .rounding(8.0)
                        .stroke(egui::Stroke::new(1.0_f32, col.gamma_multiply(0.55)))
                        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(icon).size(17.0).color(col));
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(title).strong().size(13.0));
                                    ui.label(RichText::new(desc).small().color(NORD_TEXT.gamma_multiply(0.72)));
                                });
                            });
                        });
                    ui.add_space(5.0);
                };
                ui.label(RichText::new("What you can do with ManageBIM").size(15.0).strong().color(NORD_ACCENT));
                ui.add_space(6.0);
                card(ui, "⚔", "Clash Detective", "Hard & clearance tests, Navisworks-style workflow, BCF reports", NORD_RED);
                card(ui, "🧭", "Federated review", "IFC, DWG, Revit / Navisworks, STEP, point clouds in one view", NORD_ACCENT);
                card(ui, "📜", "Scripts (Rhai)", "Automate selection, properties and clash tests from the Script tab", NORD_GREEN);
                card(ui, "🎞", "Path trace & 4D", "Progressive CPU renders, animations, TimeLiner and PDF sheets", NORD_YELLOW);
                ui.add_space(8.0);
                ui.label(RichText::new("No administrator rights - ManageBIM is installed only for your Windows user.").small().color(NORD_GREEN));
                ui.label(RichText::new(format!("Folder: {}", self.paths.app_dir.display())).small().color(NORD_TEXT.gamma_multiply(0.7)));
            }
            Page::Components => {
                title(ui, "Choose components");
                ui.add_enabled(false, egui::Checkbox::new(&mut true, "ManageBIM application (required)"));
                ui.checkbox(&mut self.desktop, "Desktop shortcut");
                ui.add_space(10.0);
                ui.label(RichText::new("Add-ins (plugins) for the programs found on this PC").strong());
                ui.add_space(4.0);
                let row = |ui: &mut egui::Ui, on: &mut bool, name: &str, found: &[String], what: &str| {
                    let ok = !found.is_empty();
                    if !ok {
                        *on = false;
                    }
                    ui.add_enabled(ok, egui::Checkbox::new(on, name));
                    let txt = if ok { format!("      {what} - found: {}", found.join(", ")) } else { "      not installed on this PC".to_string() };
                    ui.label(RichText::new(txt).small().color(if ok { NORD_TEXT.gamma_multiply(0.75) } else { NORD_TEXT.gamma_multiply(0.45) }));
                };
                row(ui, &mut self.revit, "Revit", &self.found.revit, "Export, Send to ManageBIM, ManageBIM Issues");
                row(ui, &mut self.acad, "AutoCAD / Plant 3D", &self.found.acad, "Send to ManageBIM (with Plant 3D properties)");
                row(ui, &mut self.navis, "Navisworks", &self.found.navis, "Export, Send to ManageBIM, ManageBIM Issues");
                ui.add_space(10.0);
                if !self.running_programs.is_empty() {
                    ui.label(RichText::new(format!("⚠ Close these programs before installing: {}", self.running_programs.join(", "))).color(NORD_YELLOW));
                }
            }
            Page::Installing => {
                title(ui, if self.old.is_some() { "Upgrading…" } else { "Installing…" });
                self.start(ctx);
                let j = self.job.lock().unwrap();
                ui.add(egui::ProgressBar::new(((j.lines.len() as f32) / 14.0).min(0.95)).animate(true).fill(NORD_ACCENT));
                ui.add_space(8.0);
                egui::ScrollArea::vertical().max_height(230.0).stick_to_bottom(true).show(ui, |ui| {
                    for l in &j.lines {
                        let (t, c) = if let Some(r) = l.strip_prefix("[OK]") { (format!("✔ {}", r.trim()), NORD_GREEN) } else if l.contains("WARNING") { (l.clone(), NORD_YELLOW) } else { (l.clone(), NORD_TEXT.gamma_multiply(0.8)) };
                        ui.label(RichText::new(t).color(c).small());
                    }
                });
                let finished = j.done.is_some();
                drop(j);
                if finished {
                    self.page = Page::Finish;
                }
            }
            Page::Finish => {
                let j = self.job.lock().unwrap();
                match &j.done {
                    Some(Ok(log)) => {
                        title(ui, if self.old.is_some() { "ManageBIM has been upgraded" } else { "ManageBIM is installed" });
                        for l in log.lines().filter(|l| l.contains("[OK]")) {
                            ui.label(RichText::new(format!("✔ {}", l.trim().trim_start_matches("[OK]").trim())).color(NORD_GREEN));
                        }
                        let warns: Vec<&str> = log.lines().filter(|l| l.contains("WARNING")).collect();
                        if !warns.is_empty() {
                            ui.add_space(6.0);
                            for w in warns {
                                ui.label(RichText::new(w.trim()).color(NORD_YELLOW).small());
                            }
                        }
                        ui.add_space(12.0);
                        ui.label(RichText::new("Getting started").strong().color(NORD_ACCENT));
                        ui.label(RichText::new("• Help › Demo — open a sample federated project").small());
                        ui.label(RichText::new("• Ctrl+K — search any command or element").small());
                        ui.label(RichText::new("• ⚙ Settings — fonts / DPI, DWG export, scripts folder").small());
                        drop(j);
                        ui.checkbox(&mut self.launch, "Start ManageBIM now");
                    }
                    Some(Err(e)) => {
                        title(ui, "The installation failed");
                        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                            ui.label(RichText::new(e).color(NORD_RED));
                        });
                        ui.add_space(8.0);
                        ui.label(RichText::new(format!("Details: {}", self.paths.local.join("ManageBIM-setup.log").display())).small());
                    }
                    None => {}
                }
            }
        }
    }
}

impl eframe::App for Wizard {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // verificare automata (MB_SETUP_AUTO=welcome|components|install): sare la pagina ceruta, fara clicuri
        if let Ok(a) = std::env::var("MB_SETUP_AUTO") {
            if self.page == Page::Welcome && (a == "components" || a == "install") {
                self.page = Page::Components;
            }
            if self.page == Page::Components && a == "install" {
                self.page = Page::Installing;
            }
        }
        egui::TopBottomPanel::bottom("buttons").frame(egui::Frame::none().fill(NORD_PANEL).inner_margin(egui::Margin::symmetric(16.0, 10.0))).show(ctx, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let btn = |t: &str| egui::Button::new(RichText::new(t).size(14.0)).min_size(egui::vec2(96.0, 30.0));
                match self.page {
                    Page::Welcome => {
                        if ui.add(btn("Cancel")).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.add(btn("Next  ›").fill(NORD_ACCENT.gamma_multiply(0.45))).clicked() {
                            self.running_programs = running(&["ManageBIM.exe", "Revit.exe", "acad.exe", "Roamer.exe"]);
                            self.page = Page::Components;
                        }
                    }
                    Page::Components => {
                        if ui.add(btn("Cancel")).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        let label = if self.old.is_some() { "Upgrade" } else { "Install" };
                        let blocked = self.running_programs.iter().any(|p| p.eq_ignore_ascii_case("ManageBIM"));
                        if ui.add_enabled(!blocked, btn(label).fill(NORD_ACCENT.gamma_multiply(0.45))).clicked() {
                            self.page = Page::Installing;
                        }
                        if ui.add(btn("‹  Back")).clicked() {
                            self.page = Page::Welcome;
                        }
                        if ui.small_button("⟳ Check again").on_hover_text("Check again which programs are still open").clicked() {
                            self.running_programs = running(&["ManageBIM.exe", "Revit.exe", "acad.exe", "Roamer.exe"]);
                        }
                    }
                    Page::Installing => {
                        ui.add_enabled(false, btn("Please wait…"));
                    }
                    Page::Finish => {
                        if ui.add(btn("Finish").fill(NORD_ACCENT.gamma_multiply(0.45))).clicked() {
                            let ok = matches!(self.job.lock().unwrap().done, Some(Ok(_)));
                            *self.launch_now.lock().unwrap() = ok && self.launch;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                }
            });
        });
        egui::SidePanel::left("steps").exact_width(170.0).resizable(false).frame(egui::Frame::none().fill(NORD_PANEL).inner_margin(egui::Margin::symmetric(18.0, 8.0))).show(ctx, |ui| self.steps(ui));
        egui::CentralPanel::default().frame(egui::Frame::none().fill(NORD_BG).inner_margin(egui::Margin::symmetric(24.0, 8.0))).show(ctx, |ui| {
            let c = ui.ctx().clone();
            egui::ScrollArea::vertical().show(ui, |ui| self.page_ui(ui, &c));
        });
        // un clic pe X in timpul instalarii nu opreste instalarea la jumatate
        if ctx.input(|i| i.viewport().close_requested()) && self.page == Page::Installing {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
    }
}

fn icon() -> egui::IconData {
    let n = 64u32;
    let mut rgba = vec![0u8; (n * n * 4) as usize];
    for y in 0..n {
        for x in 0..n {
            let i = ((y * n + x) * 4) as usize;
            let (fx, fy) = (x as f32 / n as f32, y as f32 / n as f32);
            let in_box = |x0: f32, y0: f32, s: f32| fx >= x0 && fx < x0 + s && fy >= y0 && fy < y0 + s;
            let c: Option<[u8; 3]> = if in_box(0.38, 0.08, 0.5) {
                Some([70, 140, 230])
            } else if in_box(0.1, 0.32, 0.5) {
                Some([235, 135, 45])
            } else if in_box(0.42, 0.5, 0.44) {
                Some([70, 190, 100])
            } else {
                None
            };
            if let Some(c) = c {
                rgba[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
    }
    egui::IconData { rgba, width: n, height: n }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).map(|a| a.to_lowercase()).collect();
    let silent = args.iter().any(|a| a == "/s" || a == "--silent" || a == "-s");
    let no_addins = args.iter().any(|a| a.trim_start_matches(['/', '-']) == "noaddins");
    let test_root: Option<PathBuf> = std::env::args().skip(1).find_map(|a| {
        let l = a.to_lowercase();
        l.starts_with("/testroot=").then(|| PathBuf::from(&a["/testroot=".len()..]))
    });
    let local = match &test_root {
        Some(t) => t.join("LocalAppData"),
        None => std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default(),
    };
    let paths = Paths { app_dir: local.join("Programs").join("ManageBIM"), local };

    if silent {
        let opt = Options { addins: if no_addins { Some(Vec::new()) } else { None }, desktop: true, test_root };
        if install(&paths, &opt, &|_| {}).is_err() {
            std::process::exit(1);
        }
        return;
    }

    let app_dir = paths.app_dir.clone();
    let wiz = Wizard::new(paths, test_root, no_addins);
    let launch = wiz.launch_now.clone();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("ManageBIM {VERSION} Setup"))
            .with_inner_size([720.0, 560.0])
            .with_resizable(false)
            .with_maximize_button(false)
            .with_icon(icon()),
        centered: true,
        ..Default::default()
    };
    let r = eframe::run_native(
        "ManageBIM Setup",
        options,
        Box::new(|cc| {
            let mut v = egui::Visuals::dark();
            v.panel_fill = NORD_BG;
            v.window_fill = NORD_PANEL;
            v.override_text_color = Some(NORD_TEXT);
            v.selection.bg_fill = NORD_ACCENT.gamma_multiply(0.5);
            v.widgets.inactive.weak_bg_fill = Color32::from_rgb(67, 76, 94);
            v.widgets.hovered.weak_bg_fill = Color32::from_rgb(76, 86, 106);
            cc.egui_ctx.set_visuals(v);
            Ok(Box::new(wiz))
        }),
    );
    if let Err(e) = r {
        // fara OpenGL (ex. sesiune remote veche): instalare directa, cu mesaj la final
        #[cfg(windows)]
        win::msg(&format!("The setup window could not be opened ({e}).\nManageBIM will be installed with the default options."), 0x40);
        let opt = Options { addins: if no_addins { Some(Vec::new()) } else { None }, desktop: true, test_root: None };
        let p = Paths { app_dir: app_dir.clone(), local: app_dir.parent().and_then(|p| p.parent()).map(|p| p.to_path_buf()).unwrap_or_default() };
        if let Err(e) = install(&p, &opt, &|_| {}) {
            #[cfg(windows)]
            win::msg(&format!("The installation failed:\n\n{e}"), win::ICON_ERROR);
            std::process::exit(1);
        }
        return;
    }
    if *launch.lock().unwrap() {
        let _ = std::process::Command::new(app_dir.join("ManageBIM.exe")).current_dir(&app_dir).spawn();
    }
}
