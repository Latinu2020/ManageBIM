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
    /// folder ales in asistent (None = cel implicit)
    install_dir: Option<PathBuf>,
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
        if let Some(d) = &opt.install_dir {
            c.arg("-InstallDir").arg(d);
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
    /// folderul de instalare (editabil, cu Browse)
    dir: String,
    running_programs: Vec<String>,
    job: Arc<Mutex<Job>>,
    started: bool,
    /// folderul aplicatiei de pornit dupa Finish (None = nu porni)
    pub launch_now: Arc<Mutex<Option<PathBuf>>>,
}

// Paleta aplicatiei (tema inchisa): consecventa cu ManageBIM.
const NORD_BG: Color32 = Color32::from_rgb(15, 20, 32);
const NORD_PANEL: Color32 = Color32::from_rgb(22, 29, 44);
const NORD_PANEL2: Color32 = Color32::from_rgb(29, 38, 56);
const NORD_LINE: Color32 = Color32::from_rgb(42, 53, 73);
const NORD_ACCENT: Color32 = Color32::from_rgb(59, 130, 246);
const NORD_CYAN: Color32 = Color32::from_rgb(34, 211, 238);
const NORD_GREEN: Color32 = Color32::from_rgb(74, 222, 128);
const NORD_YELLOW: Color32 = Color32::from_rgb(250, 204, 21);
const NORD_RED: Color32 = Color32::from_rgb(248, 113, 113);
const NORD_VIOLET: Color32 = Color32::from_rgb(167, 139, 250);
const NORD_TEXT: Color32 = Color32::from_rgb(230, 236, 245);
const NORD_MUTED: Color32 = Color32::from_rgb(141, 155, 179);

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

/// Folderul unei instalari anterioare (Settings > Apps: InstallLocation), daca a fost ales altul.
fn previous_location() -> Option<PathBuf> {
    let mut c = std::process::Command::new("reg.exe");
    c.args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\ManageBIM", "/v", "InstallLocation"]).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let out = String::from_utf8_lossy(&c.output().ok()?.stdout).to_string();
    let line = out.lines().find(|l| l.contains("InstallLocation"))?;
    let v = line.split("REG_SZ").nth(1)?.trim();
    let p = PathBuf::from(v);
    p.join("ManageBIM.exe").is_file().then_some(p)
}

/// Folder in care un utilizator obisnuit (fara administrator) nu poate scrie.
fn needs_admin(p: &Path) -> bool {
    let s = p.to_string_lossy().to_lowercase();
    ["program files", "\\windows\\", "programdata"].iter().any(|k| s.contains(k))
}

/// Buton principal (albastru plin, text alb) / secundar (contur).
fn primary(t: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(t.to_string()).size(14.0).strong().color(Color32::WHITE)).fill(NORD_ACCENT).rounding(7.0).min_size(egui::vec2(110.0, 34.0))
}
fn secondary(t: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(t.to_string()).size(14.0).color(NORD_TEXT)).fill(NORD_PANEL2).stroke(egui::Stroke::new(1.0_f32, NORD_LINE)).rounding(7.0).min_size(egui::vec2(96.0, 34.0))
}

impl Wizard {
    fn new(paths: Paths, test_root: Option<PathBuf>, no_addins: bool) -> Wizard {
        // actualizare: acelasi folder ca instalarea anterioara (si cand a fost ales altul decat cel implicit)
        let prev = if test_root.is_none() { previous_location() } else { None };
        let app_dir = prev.unwrap_or_else(|| paths.app_dir.clone());
        let old = installed_version(&app_dir);
        let found = find_programs();
        let (revit, acad, navis) = (!no_addins && !found.revit.is_empty(), !no_addins && !found.acad.is_empty(), !no_addins && !found.navis.is_empty());
        Wizard {
            page: Page::Welcome,
            dir: app_dir.display().to_string(),
            paths: Arc::new(Paths { app_dir, local: paths.local }),
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
            launch_now: Arc::new(Mutex::new(None)),
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
        // folderul ales in asistent
        let dir = PathBuf::from(self.dir.trim());
        let custom = dir != self.paths.app_dir || self.paths.app_dir != self.paths.local.join("Programs").join("ManageBIM");
        self.paths = Arc::new(Paths { app_dir: dir.clone(), local: self.paths.local.clone() });
        let opt = Options { addins: Some(list), desktop: self.desktop, test_root: self.test_root.clone(), install_dir: custom.then_some(dir) };
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

    /// Bara laterala: sigla, versiunea si pasii numerotati (cerc + linie de legatura).
    fn steps(&self, ui: &mut egui::Ui) {
        ui.add_space(24.0);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
            let p = ui.painter();
            let sq = |x: f32, y: f32, s: f32, c: Color32| p.rect_filled(egui::Rect::from_min_size(r.min + egui::vec2(x, y), egui::vec2(s, s)), 3.0, c);
            sq(12.0, 1.0, 16.0, Color32::from_rgb(70, 140, 230));
            sq(2.0, 9.0, 16.0, Color32::from_rgb(235, 135, 45));
            sq(13.0, 15.0, 14.0, Color32::from_rgb(70, 190, 100));
            ui.label(RichText::new("ManageBIM").size(19.0).strong().color(NORD_TEXT));
        });
        ui.add_space(4.0);
        egui::Frame::none().fill(NORD_ACCENT.gamma_multiply(0.18)).rounding(10.0).inner_margin(egui::Margin::symmetric(8.0, 2.0)).show(ui, |ui| {
            ui.label(RichText::new(format!("v{VERSION}")).size(11.0).color(NORD_CYAN));
        });
        ui.add_space(30.0);
        let steps = [(Page::Welcome, "Welcome", "Overview"), (Page::Components, "Options", "Folder & add-ins"), (Page::Installing, "Install", "Copying files"), (Page::Finish, "Finish", "Ready to use")];
        for (i, (p, name, sub)) in steps.iter().enumerate() {
            let cur = *p == self.page;
            let done = (*p as u8) < (self.page as u8) || (self.page == Page::Finish && *p == Page::Finish);
            let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 46.0), egui::Sense::hover());
            let painter = ui.painter();
            let c = egui::pos2(r.left() + 14.0, r.top() + 14.0);
            if i + 1 < steps.len() {
                let col = if done { NORD_GREEN.gamma_multiply(0.7) } else { NORD_LINE };
                painter.line_segment([c + egui::vec2(0.0, 15.0), egui::pos2(c.x, r.bottom() + 2.0)], egui::Stroke::new(2.0_f32, col));
            }
            let (fill, ring, txt) = if done {
                (NORD_GREEN, NORD_GREEN, "✔".to_string())
            } else if cur {
                (NORD_ACCENT, NORD_CYAN, format!("{}", i + 1))
            } else {
                (NORD_PANEL2, NORD_LINE, format!("{}", i + 1))
            };
            painter.circle(c, 13.0, fill, egui::Stroke::new(1.5_f32, ring));
            painter.text(c, egui::Align2::CENTER_CENTER, txt, egui::FontId::proportional(12.0), if done { NORD_BG } else { Color32::WHITE });
            let tc = if cur { NORD_TEXT } else if done { NORD_TEXT.gamma_multiply(0.8) } else { NORD_MUTED };
            painter.text(egui::pos2(c.x + 24.0, c.y - 8.0), egui::Align2::LEFT_TOP, *name, egui::FontId::proportional(if cur { 14.5 } else { 13.5 }), tc);
            painter.text(egui::pos2(c.x + 24.0, c.y + 8.0), egui::Align2::LEFT_TOP, *sub, egui::FontId::proportional(10.5), NORD_MUTED.gamma_multiply(0.85));
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(14.0);
            ui.label(RichText::new("No administrator rights needed").size(10.5).color(NORD_GREEN.gamma_multiply(0.85)));
            ui.label(RichText::new("BIM coordination & presentation").size(10.5).color(NORD_MUTED));
        });
    }

    fn page_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let title = |ui: &mut egui::Ui, t: &str, sub: &str| {
            ui.add_space(18.0);
            ui.label(RichText::new(t).size(23.0).strong().color(NORD_TEXT));
            if !sub.is_empty() {
                ui.add_space(2.0);
                ui.label(RichText::new(sub).size(13.0).color(NORD_MUTED));
            }
            ui.add_space(14.0);
        };
        let card_frame = |col: Color32| egui::Frame::none().fill(NORD_PANEL2).rounding(10.0).stroke(egui::Stroke::new(1.0_f32, col)).inner_margin(egui::Margin::symmetric(12.0, 9.0));
        match self.page {
            Page::Welcome => {
                let (t, sub) = match &self.old {
                    Some(v) if v == VERSION => ("Repair ManageBIM".to_string(), format!("ManageBIM {v} is already installed - it will be reinstalled. Settings, projects and models are kept.")),
                    Some(v) => ("Upgrade ManageBIM".to_string(), format!("ManageBIM {v} is installed - it will be upgraded to {VERSION}. Settings, projects and models are kept.")),
                    None => ("Welcome to ManageBIM".to_string(), format!("This wizard installs ManageBIM {VERSION} for {}.", std::env::var("USERNAME").unwrap_or_else(|_| "you".into()))),
                };
                title(ui, &t, &sub);
                // banner cu degrade
                let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 74.0), egui::Sense::hover());
                let mut mesh = egui::Mesh::default();
                for (p, c) in [(r.left_top(), Color32::from_rgb(30, 64, 175)), (r.right_top(), Color32::from_rgb(14, 116, 144)), (r.right_bottom(), Color32::from_rgb(13, 82, 104)), (r.left_bottom(), Color32::from_rgb(23, 45, 120))] {
                    mesh.colored_vertex(p, c);
                }
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(0, 2, 3);
                ui.painter().add(egui::Shape::mesh(mesh));
                ui.painter().text(r.left_top() + egui::vec2(16.0, 14.0), egui::Align2::LEFT_TOP, "Review, coordinate and present BIM models", egui::FontId::proportional(16.0), Color32::WHITE);
                ui.painter().text(r.left_top() + egui::vec2(16.0, 40.0), egui::Align2::LEFT_TOP, "IFC · Revit · Navisworks · DWG · STEP · point clouds - in one fast 3D view", egui::FontId::proportional(12.0), Color32::from_rgb(200, 225, 255));
                ui.add_space(14.0);
                let feats = [
                    ("⚠", "Clash Detective", "Hard & clearance tests, voids, BCF issues", NORD_RED),
                    ("🧭", "Federated review", "Selection tree, properties, sets, viewpoints", NORD_ACCENT),
                    ("☀", "Photoreal render", "Scanned PBR materials, lights, sun, path tracing", NORD_YELLOW),
                    ("📐", "2D drawings", "Inventor-style views, PDF / DWG export", NORD_CYAN),
                    ("🏷", "Open BIM", "IFC4 + MVD, bSDD classification, IDS, COBie", NORD_GREEN),
                    ("🎞", "4D & automation", "TimeLiner, animations, video, Rhai scripts", NORD_VIOLET),
                ];
                let w = (ui.available_width() - 10.0) / 2.0;
                for row in feats.chunks(2) {
                    ui.horizontal(|ui| {
                        for (icon, name, desc, col) in row {
                            ui.allocate_ui(egui::vec2(w, 58.0), |ui| {
                                card_frame(col.gamma_multiply(0.45)).show(ui, |ui| {
                                    ui.set_width(w - 26.0);
                                    ui.set_height(38.0);
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(*icon).size(18.0).color(*col));
                                        ui.vertical(|ui| {
                                            ui.label(RichText::new(*name).strong().size(13.0).color(NORD_TEXT));
                                            ui.label(RichText::new(*desc).size(11.0).color(NORD_MUTED));
                                        });
                                    });
                                });
                            });
                        }
                    });
                    ui.add_space(4.0);
                }
            }
            Page::Components => {
                title(ui, "Installation options", "Choose where ManageBIM goes and which add-ins to install.");
                // ---- folderul
                ui.label(RichText::new("Install location").strong().color(NORD_TEXT));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.dir).desired_width(ui.available_width() - 110.0).font(egui::TextStyle::Monospace));
                    if ui.add(secondary("📁 Browse…")).clicked() {
                        let start = PathBuf::from(self.dir.trim());
                        let mut d = rfd::FileDialog::new().set_title("Choose the ManageBIM installation folder");
                        if let Some(parent) = start.parent().filter(|p| p.is_dir()) {
                            d = d.set_directory(parent);
                        }
                        if let Some(p) = d.pick_folder() {
                            // un subfolder ManageBIM, ca sa nu se amestece cu alte fisiere
                            let p = if p.file_name().map_or(false, |n| n.to_string_lossy().eq_ignore_ascii_case("ManageBIM")) { p } else { p.join("ManageBIM") };
                            self.dir = p.display().to_string();
                        }
                    }
                });
                let dir = PathBuf::from(self.dir.trim());
                if needs_admin(&dir) {
                    ui.label(RichText::new("⚠ This folder needs administrator rights - choose a folder in your user profile or on a data drive.").size(11.5).color(NORD_YELLOW));
                } else if self.test_root.is_none() && dir != self.paths.local.join("Programs").join("ManageBIM") {
                    ui.label(RichText::new("Custom folder - upgrades will install here too.").size(11.5).color(NORD_MUTED));
                } else {
                    ui.label(RichText::new("Default: your user folder, no administrator rights.").size(11.5).color(NORD_MUTED));
                }
                if ui.small_button("↺ Default folder").clicked() {
                    self.dir = self.paths.local.join("Programs").join("ManageBIM").display().to_string();
                }
                ui.add_space(12.0);
                // ---- componente
                ui.label(RichText::new("Components").strong().color(NORD_TEXT));
                ui.add_space(4.0);
                let comp = |ui: &mut egui::Ui, on: &mut bool, enabled: bool, name: &str, desc: &str, col: Color32| {
                    card_frame(if *on { col.gamma_multiply(0.55) } else { NORD_LINE }).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.add_enabled(enabled, egui::Checkbox::without_text(on));
                            ui.vertical(|ui| {
                                ui.label(RichText::new(name).strong().size(13.0).color(if enabled { NORD_TEXT } else { NORD_MUTED }));
                                ui.label(RichText::new(desc).size(11.0).color(NORD_MUTED));
                            });
                        });
                    });
                    ui.add_space(4.0);
                };
                comp(ui, &mut true, false, "ManageBIM application", "Viewer, clash detection, 2D drawings, render, 28 scanned PBR materials, OpenCascade, denoiser", NORD_ACCENT);
                comp(ui, &mut self.desktop, true, "Desktop shortcut", "Start ManageBIM from the desktop", NORD_ACCENT);
                let addin = |found: &[String], what: &str| if found.is_empty() { "Not installed on this PC".to_string() } else { format!("{what} - found: {}", found.join(", ")) };
                let (rf, af, nf) = (self.found.revit.clone(), self.found.acad.clone(), self.found.navis.clone());
                if rf.is_empty() {
                    self.revit = false;
                }
                if af.is_empty() {
                    self.acad = false;
                }
                if nf.is_empty() {
                    self.navis = false;
                }
                comp(ui, &mut self.revit, !rf.is_empty(), "Revit add-in", &addin(&rf, "Export, Send to ManageBIM, Issues"), NORD_ACCENT);
                comp(ui, &mut self.acad, !af.is_empty(), "AutoCAD / Plant 3D add-in", &addin(&af, "Send to ManageBIM with Plant 3D properties"), NORD_ACCENT);
                comp(ui, &mut self.navis, !nf.is_empty(), "Navisworks add-in", &addin(&nf, "Export, Send to ManageBIM, Issues"), NORD_ACCENT);
                if !self.running_programs.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("⚠ Close these programs before installing: {}", self.running_programs.join(", "))).color(NORD_YELLOW));
                }
            }
            Page::Installing => {
                title(ui, if self.old.is_some() { "Upgrading ManageBIM…" } else { "Installing ManageBIM…" }, &format!("Into {}", self.dir.trim()));
                self.start(ctx);
                let j = self.job.lock().unwrap();
                // ~14 linii de progres la o instalare completa
                let frac = ((j.lines.len() as f32) / 14.0).min(0.97);
                ui.add(egui::ProgressBar::new(frac).desired_height(14.0).fill(NORD_ACCENT).text(RichText::new(format!("{:.0}%", frac * 100.0)).color(Color32::WHITE)).rounding(7.0));
                ui.add_space(6.0);
                if let Some(last) = j.lines.last() {
                    ui.label(RichText::new(last.trim_start_matches("[OK]").trim()).size(12.5).color(NORD_TEXT));
                }
                ui.add_space(8.0);
                card_frame(NORD_LINE).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::ScrollArea::vertical().max_height(220.0).stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                        for l in &j.lines {
                            let (t, c) = if let Some(r) = l.strip_prefix("[OK]") { (format!("✔ {}", r.trim()), NORD_GREEN) } else if l.contains("WARNING") { (l.clone(), NORD_YELLOW) } else { (l.clone(), NORD_TEXT.gamma_multiply(0.75)) };
                            ui.label(RichText::new(t).color(c).size(11.5).monospace());
                        }
                    });
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
                        ui.add_space(18.0);
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::hover());
                            ui.painter().circle_filled(r.center(), 21.0, NORD_GREEN);
                            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "✔", egui::FontId::proportional(22.0), NORD_BG);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(if self.old.is_some() { "ManageBIM has been upgraded" } else { "ManageBIM is ready" }).size(22.0).strong().color(NORD_TEXT));
                                ui.label(RichText::new(format!("Version {VERSION} · {}", self.dir.trim())).size(12.0).color(NORD_MUTED));
                            });
                        });
                        ui.add_space(12.0);
                        card_frame(NORD_GREEN.gamma_multiply(0.35)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            for l in log.lines().filter(|l| l.contains("[OK]")) {
                                ui.label(RichText::new(format!("✔ {}", l.trim().trim_start_matches("[OK]").trim())).size(12.0).color(NORD_GREEN));
                            }
                            for w in log.lines().filter(|l| l.contains("WARNING")) {
                                ui.label(RichText::new(w.trim()).color(NORD_YELLOW).size(11.5));
                            }
                        });
                        ui.add_space(10.0);
                        card_frame(NORD_LINE).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new("Getting started").strong().color(NORD_ACCENT));
                            ui.label(RichText::new("• Help › Demo - open a sample federated project").size(12.0));
                            ui.label(RichText::new("• Ctrl+K - search any command or element").size(12.0));
                            ui.label(RichText::new("• Render › Materials › Auto by IFC type - photoreal materials in one click").size(12.0));
                        });
                        drop(j);
                        ui.add_space(8.0);
                        ui.checkbox(&mut self.launch, RichText::new("Start ManageBIM now").size(13.5));
                    }
                    Some(Err(e)) => {
                        title(ui, "The installation failed", "Nothing was changed in your existing installation.");
                        card_frame(NORD_RED.gamma_multiply(0.5)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                                ui.label(RichText::new(e).color(NORD_RED).monospace().size(11.5));
                            });
                        });
                        ui.add_space(8.0);
                        ui.label(RichText::new(format!("Details: {}", self.paths.local.join("ManageBIM-setup.log").display())).size(11.5).color(NORD_MUTED));
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
        egui::TopBottomPanel::bottom("buttons")
            .frame(egui::Frame::none().fill(NORD_PANEL).inner_margin(egui::Margin::symmetric(18.0, 12.0)).stroke(egui::Stroke::new(1.0_f32, NORD_LINE)))
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    match self.page {
                        Page::Welcome => {
                            if ui.add(primary("Next  ›")).clicked() {
                                self.running_programs = running(&["ManageBIM.exe", "Revit.exe", "acad.exe", "Roamer.exe"]);
                                self.page = Page::Components;
                            }
                            if ui.add(secondary("Cancel")).clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        Page::Components => {
                            let label = if self.old.is_some() { "Upgrade" } else { "Install" };
                            let dir = PathBuf::from(self.dir.trim());
                            let blocked = self.running_programs.iter().any(|p| p.eq_ignore_ascii_case("ManageBIM")) || self.dir.trim().is_empty() || needs_admin(&dir);
                            if ui.add_enabled(!blocked, primary(label)).clicked() {
                                self.page = Page::Installing;
                            }
                            if ui.add(secondary("‹  Back")).clicked() {
                                self.page = Page::Welcome;
                            }
                            if ui.add(secondary("Cancel")).clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                if ui.small_button("⟳ Check again").on_hover_text("Check again which programs are still open").clicked() {
                                    self.running_programs = running(&["ManageBIM.exe", "Revit.exe", "acad.exe", "Roamer.exe"]);
                                }
                            });
                        }
                        Page::Installing => {
                            ui.add_enabled(false, secondary("Please wait…"));
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.spinner();
                            });
                        }
                        Page::Finish => {
                            if ui.add(primary("Finish")).clicked() {
                                let ok = matches!(self.job.lock().unwrap().done, Some(Ok(_)));
                                *self.launch_now.lock().unwrap() = (ok && self.launch).then(|| PathBuf::from(self.dir.trim()));
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                    }
                });
            });
        egui::SidePanel::left("steps").exact_width(190.0).resizable(false).frame(egui::Frame::none().fill(NORD_PANEL).inner_margin(egui::Margin::symmetric(18.0, 8.0)).stroke(egui::Stroke::new(1.0_f32, NORD_LINE))).show(ctx, |ui| self.steps(ui));
        egui::CentralPanel::default().frame(egui::Frame::none().fill(NORD_BG).inner_margin(egui::Margin::symmetric(26.0, 8.0))).show(ctx, |ui| {
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
        let opt = Options { addins: if no_addins { Some(Vec::new()) } else { None }, desktop: true, test_root, install_dir: None };
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
            .with_inner_size([820.0, 600.0])
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
        let opt = Options { addins: if no_addins { Some(Vec::new()) } else { None }, desktop: true, test_root: None, install_dir: None };
        let p = Paths { app_dir: app_dir.clone(), local: app_dir.parent().and_then(|p| p.parent()).map(|p| p.to_path_buf()).unwrap_or_default() };
        if let Err(e) = install(&p, &opt, &|_| {}) {
            #[cfg(windows)]
            win::msg(&format!("The installation failed:\n\n{e}"), win::ICON_ERROR);
            std::process::exit(1);
        }
        return;
    }
    // folderul ales in asistent (poate fi altul decat cel implicit)
    let dir = launch.lock().unwrap().clone();
    if let Some(dir) = dir {
        let _ = std::process::Command::new(dir.join("ManageBIM.exe")).current_dir(&dir).spawn();
    }
}
