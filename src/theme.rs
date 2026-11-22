//! Light / Dark / Nord themes and persistent user settings.
use eframe::egui::{self, Color32, Rounding, Shadow, Stroke};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Theme {
    Dark,
    Light,
    /// paleta Nord (nordtheme.com): albastru-gri arctic, contrast bland
    Nord,
    /// Tokyo Night (github.com/folke/tokyonight.nvim): albastru-noapte, accente albastru / mov
    #[serde(alias = "Tokyo Night")]
    TokyoNight,
    /// Studio: aproape negru, accent albastru electric, panouri ca niste carduri (aspect de aplicatie BIM moderna)
    Studio,
}

impl Theme {
    pub fn is_dark(self) -> bool {
        self != Theme::Light
    }
    pub fn label(self) -> &'static str {
        match self {
            Theme::Dark => "Dark",
            Theme::Light => "Light",
            Theme::Nord => "Nord",
            Theme::TokyoNight => "Tokyo Night",
            Theme::Studio => "Studio",
        }
    }
    /// urmatoarea tema la click pe butonul din bara de tab-uri
    pub fn next(self) -> Theme {
        match self {
            Theme::Dark => Theme::Nord,
            Theme::Nord => Theme::TokyoNight,
            Theme::TokyoNight => Theme::Studio,
            Theme::Studio => Theme::Light,
            Theme::Light => Theme::Dark,
        }
    }
}

/// Culoarea de accent a temei curente (butoane active, selectie, progres) - setata de `apply`.
static ACCENT_RGB: AtomicU32 = AtomicU32::new(0x3b82f6);

pub fn accent() -> Color32 {
    let v = ACCENT_RGB.load(Ordering::Relaxed);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Conturul panourilor si ferestrelor temei curente - setata de `apply`.
/// Panourile si ferestrele il folosesc ca sa iasa putin in evidenta fata de fundal.
static BORDER_RGB: AtomicU32 = AtomicU32::new(0x404754);

pub fn border() -> Color32 {
    let v = BORDER_RGB.load(Ordering::Relaxed);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Preferintele UI care trebuie sa supravietuiasca unei restartari (Clash + 4D).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClashUiPrefs {
    #[serde(default)]
    pub subtab: u8,
    #[serde(default)]
    pub filter: Option<crate::clash::ClashStatus>,
    #[serde(default)]
    pub filter_sel: u8,
    #[serde(default = "others_default")]
    pub others: crate::app::Others,
    #[serde(default = "yes")]
    pub auto_zoom: bool,
    #[serde(default)]
    pub highlight_all: bool,
    #[serde(default = "group_cell_default")]
    pub group_cell: f32,
    #[serde(default = "yes")]
    pub only_linked: bool,
}

fn group_cell_default() -> f32 {
    5.0
}

fn others_default() -> crate::app::Others {
    crate::app::Others::Dim
}

impl Default for ClashUiPrefs {
    fn default() -> Self {
        ClashUiPrefs {
            subtab: 0,
            filter: None,
            filter_sel: 0,
            others: crate::app::Others::Dim,
            auto_zoom: true,
            highlight_all: false,
            group_cell: 5.0,
            only_linked: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub theme: Theme,
    /// write a .bimc cache next to every IFC that is opened
    pub auto_cache: bool,
    /// open the .bimc cache instead of the IFC when the cache is up to date
    pub use_cache: bool,
    /// recently opened projects / models
    #[serde(default)]
    pub recent: Vec<String>,
    /// optional external converters (ODA File Converter, Navisworks)
    #[serde(default)]
    pub tools: crate::formats::Tools,
    /// panel arrangement (docked / floating windows)
    #[serde(default)]
    pub layout: crate::layout::UiLayout,
    /// walk speed in metres per second
    #[serde(default = "default_walk")]
    pub walk_speed: f32,
    #[serde(default)]
    pub gravity: bool,
    #[serde(default)]
    pub collision: bool,
    #[serde(default = "default_eye")]
    pub eye_height: f32,
    /// gruparea arborelui de selectie (ex. Level > IFC Type, sau System > Level > Type Name)
    #[serde(default = "crate::app::default_tree_levels")]
    pub tree_levels: Vec<crate::app::TreeLevel>,
    /// ribbon restrans: doar randul cu tab-uri (mai mult loc pentru model)
    #[serde(default)]
    pub ribbon_min: bool,
    /// inaltimea ribbon-ului: Full (iconite + etichete), Compact (doar iconite), Mini (doar tab-urile)
    #[serde(default)]
    pub ribbon_mode: RibbonMode,
    /// selectie sincronizata cu Revit / Navisworks (Live Link)
    #[serde(default)]
    pub sync_selection: bool,
    /// zoom pe selectia venita din Revit / Navisworks
    #[serde(default = "yes")]
    pub sync_follow: bool,
    /// stilul de afisare al modelului
    #[serde(default)]
    pub visual_style: crate::render::VisualStyle,
    /// bara de navigare plutitoare din vizualizarea 3D
    #[serde(default = "yes")]
    pub nav_bar: bool,
    /// lungimile (masuratori, distante, coordonate) afisate in milimetri in loc de metri
    #[serde(default)]
    pub units_mm: bool,
    /// interfata in romana (altfel engleza)
    #[serde(default)]
    pub romanian: bool,
    /// marimea intregii interfate: 1.0 = 100% (Setari > Interface size / DPI)
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    /// directia sectiunii exportate in DWG / DXF (Current = planul de taiere activ)
    #[serde(default)]
    pub cad_view: crate::app::CadView,
    /// pozitia planului de taiere intre 0 (latura minima) si 1 (latura maxima) a modelului
    #[serde(default = "default_cad_view_pos")]
    pub cad_view_pos: f32,
    /// folderul cu scripturi .rhai (gol = %APPDATA%\ManageBIM\Scripts)
    #[serde(default)]
    pub scripts_dir: String,
    /// afiseaza in 3D planul de taiere ales pentru export
    #[serde(default = "yes")]
    pub show_cad_plane: bool,
    /// eticheta folderului virtual „fară folder” din Viewpoints
    #[serde(default = "default_vp_root")]
    pub vp_root_label: String,
    /// unitatea XML-ului pentru Navisworks: "auto" (din modelul Navisworks) sau m / cm / mm / ft / in
    #[serde(default = "auto")]
    pub navis_units: String,
    /// preferinte UI persistente (Clash Detective / 4D)
    #[serde(default)]
    pub clash_ui: ClashUiPrefs,
}

fn auto() -> String {
    "auto".into()
}

fn yes() -> bool {
    true
}

fn default_ui_scale() -> f32 {
    1.0
}

fn default_vp_root() -> String {
    "No folder".into()
}

fn default_cad_view_pos() -> f32 {
    0.25
}

fn default_eye() -> f32 {
    1.7
}

/// viteza de mers a unui om (m/s); vechiul implicit 3.0 era prea rapid in interior
fn default_walk() -> f32 {
    1.4
}

impl Default for Settings {
    fn default() -> Self {
        Settings { theme: Theme::Dark, auto_cache: true, use_cache: true, recent: Vec::new(), tools: Default::default(), layout: Default::default(), walk_speed: default_walk(), gravity: false, collision: false, eye_height: 1.7, tree_levels: crate::app::default_tree_levels(), ribbon_min: false, ribbon_mode: RibbonMode::Full, sync_selection: false, sync_follow: true, visual_style: crate::render::VisualStyle::Realistic, nav_bar: true, units_mm: false, navis_units: auto(), romanian: false, ui_scale: default_ui_scale(), cad_view: crate::app::CadView::Current, cad_view_pos: default_cad_view_pos(), scripts_dir: String::new(), show_cad_plane: true, vp_root_label: default_vp_root(), clash_ui: ClashUiPrefs::default() }
    }
}

fn settings_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))?;
    let new = base.join("ManageBIM").join("settings.json");
    // migrare din versiunile vechi (BimRust)
    let old = base.join("BimRust").join("settings.json");
    if !new.exists() && old.exists() {
        let _ = std::fs::create_dir_all(base.join("ManageBIM"));
        let _ = std::fs::copy(&old, &new);
    }
    Some(new)
}

impl Settings {
    pub fn load() -> Settings {
        // setarile; daca fisierul e stricat (cadere in timpul scrierii), copia .bak de la salvarea anterioara
        let read = |p: std::path::PathBuf| std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str::<Settings>(&s).ok());
        let mut s: Settings = settings_path()
            .and_then(|p| read(p.clone()).or_else(|| read(crate::formats::backup_path(&p))))
            .unwrap_or_default();
        // vechiul implicit (3 m/s) -> viteza de mers normala; o valoare aleasa de utilizator ramane
        if s.walk_speed == 3.0 {
            s.walk_speed = default_walk();
        }
        // marimea interfetei: doar valori rezonabile, altfel revenim la 100%
        if !s.ui_scale.is_finite() || !(0.75..=2.0).contains(&s.ui_scale) {
            s.ui_scale = default_ui_scale();
        }
        if !s.cad_view_pos.is_finite() || !(0.0..=1.0).contains(&s.cad_view_pos) {
            s.cad_view_pos = default_cad_view_pos();
        }
        s
    }
    pub fn add_recent(&mut self, path: &str) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(12);
        self.save();
    }

    pub fn save(&self) {
        if let Some(p) = settings_path() {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            if let Ok(s) = serde_json::to_string_pretty(self) {
                let _ = crate::formats::write_atomic(&p, s.as_bytes(), true);
            }
        }
    }
}

/// Colours used by custom-painted parts of the UI (viewport, Gantt, overlays).
#[derive(Clone, Copy)]
pub struct Palette {
    pub accent: Color32,
    pub vp_top: Color32,
    pub vp_bottom: Color32,
    pub overlay_text: Color32,
    pub overlay_weak: Color32,
    pub overlay_btn: Color32,
    pub elem_a: Color32,
    pub elem_b: Color32,
    pub gantt_bg: Color32,
    pub gantt_grid: Color32,
    pub gantt_text: Color32,
    pub label_bg: Color32,
    /// conturul vizibil al panourilor si ferestrelor (le face sa iasa usor in evidenta)
    pub border: Color32,
}

pub fn palette(t: Theme) -> Palette {
    match t {
        Theme::Dark => Palette {
            accent: Color32::from_rgb(82, 146, 255),
            vp_top: Color32::from_rgb(34, 40, 52),
            vp_bottom: Color32::from_rgb(96, 108, 126),
            overlay_text: Color32::from_rgb(235, 238, 245),
            overlay_weak: Color32::from_white_alpha(140),
            overlay_btn: Color32::from_rgba_unmultiplied(20, 24, 31, 190),
            elem_a: Color32::from_rgb(245, 112, 100),
            elem_b: Color32::from_rgb(110, 214, 125),
            gantt_bg: Color32::from_rgb(24, 27, 33),
            gantt_grid: Color32::from_rgb(50, 55, 64),
            gantt_text: Color32::from_rgb(220, 224, 232),
            label_bg: Color32::from_rgba_unmultiplied(15, 18, 24, 215),
            border: Color32::from_rgb(64, 71, 84),
        },
        Theme::Light => Palette {
            accent: Color32::from_rgb(37, 99, 235),
            vp_top: Color32::from_rgb(170, 189, 212),
            vp_bottom: Color32::from_rgb(240, 243, 247),
            overlay_text: Color32::from_rgb(28, 34, 46),
            overlay_weak: Color32::from_rgba_unmultiplied(28, 34, 46, 150),
            overlay_btn: Color32::from_rgba_unmultiplied(255, 255, 255, 215),
            elem_a: Color32::from_rgb(196, 45, 35),
            elem_b: Color32::from_rgb(25, 135, 55),
            gantt_bg: Color32::from_rgb(250, 251, 253),
            gantt_grid: Color32::from_rgb(221, 226, 234),
            gantt_text: Color32::from_rgb(40, 46, 58),
            label_bg: Color32::from_rgba_unmultiplied(20, 26, 36, 220),
            border: Color32::from_rgb(201, 209, 221),
        },
        // Nord: varianta usor mai inchisa (Polar Night adancit), Snow Storm (text), Frost (accent)
        Theme::Nord => Palette {
            accent: Color32::from_rgb(136, 192, 208),
            vp_top: Color32::from_rgb(36, 41, 52),
            vp_bottom: Color32::from_rgb(80, 90, 108),
            overlay_text: Color32::from_rgb(236, 239, 244),
            overlay_weak: Color32::from_rgba_unmultiplied(216, 222, 233, 150),
            overlay_btn: Color32::from_rgba_unmultiplied(36, 41, 52, 215),
            elem_a: Color32::from_rgb(191, 97, 106),
            elem_b: Color32::from_rgb(163, 190, 140),
            gantt_bg: Color32::from_rgb(36, 41, 52),
            gantt_grid: Color32::from_rgb(57, 65, 82),
            gantt_text: Color32::from_rgb(229, 233, 240),
            label_bg: Color32::from_rgba_unmultiplied(36, 41, 52, 225),
            border: Color32::from_rgb(68, 77, 96),
        },
        // Tokyo Night: bg #1a1b26, fg #c0caf5, blue #7aa2f7, red #f7768e, green #9ece6a
        Theme::TokyoNight => Palette {
            accent: Color32::from_rgb(122, 162, 247),
            vp_top: Color32::from_rgb(26, 27, 38),
            vp_bottom: Color32::from_rgb(65, 72, 104),
            overlay_text: Color32::from_rgb(192, 202, 245),
            overlay_weak: Color32::from_rgba_unmultiplied(169, 177, 214, 150),
            overlay_btn: Color32::from_rgba_unmultiplied(22, 22, 30, 215),
            elem_a: Color32::from_rgb(247, 118, 142),
            elem_b: Color32::from_rgb(158, 206, 106),
            gantt_bg: Color32::from_rgb(26, 27, 38),
            gantt_grid: Color32::from_rgb(41, 46, 66),
            gantt_text: Color32::from_rgb(192, 202, 245),
            label_bg: Color32::from_rgba_unmultiplied(22, 22, 30, 225),
            border: Color32::from_rgb(54, 60, 86),
        },
        // Studio: aproape negru, albastru electric
        Theme::Studio => Palette {
            accent: Color32::from_rgb(59, 130, 246),
            vp_top: Color32::from_rgb(28, 34, 46),
            vp_bottom: Color32::from_rgb(15, 19, 26),
            overlay_text: Color32::from_rgb(226, 230, 239),
            overlay_weak: Color32::from_rgba_unmultiplied(138, 147, 165, 150),
            overlay_btn: Color32::from_rgba_unmultiplied(34, 40, 49, 235),
            elem_a: Color32::from_rgb(255, 99, 99),
            elem_b: Color32::from_rgb(80, 210, 140),
            gantt_bg: Color32::from_rgb(26, 30, 36),
            gantt_grid: Color32::from_rgb(52, 58, 69),
            gantt_text: Color32::from_rgb(226, 230, 239),
            label_bg: Color32::from_rgba_unmultiplied(17, 19, 23, 230),
            border: Color32::from_rgb(48, 54, 65),
        },
    }
}

fn widget(v: &mut egui::style::WidgetVisuals, bg: Color32, stroke: Color32, fg: Color32, r: f32) {
    // rotunjimea vine din parametru (inainte era fixata la 3.5)
    v.bg_fill = bg;
    v.weak_bg_fill = bg;
    v.bg_stroke = Stroke::new(1.0_f32, stroke);
    v.fg_stroke = Stroke::new(1.0_f32, fg);
    v.rounding = Rounding::same(r);
}

pub fn apply(ctx: &egui::Context, t: Theme) {
    let p = palette(t);
    ACCENT_RGB.store(((p.accent.r() as u32) << 16) | ((p.accent.g() as u32) << 8) | p.accent.b() as u32, Ordering::Relaxed);
    BORDER_RGB.store(((p.border.r() as u32) << 16) | ((p.border.g() as u32) << 8) | p.border.b() as u32, Ordering::Relaxed);
    let mut style = (*ctx.style()).clone();
    let mut v = if t.is_dark() { egui::Visuals::dark() } else { egui::Visuals::light() };
    match t {
        Theme::Nord => {
            v.panel_fill = Color32::from_rgb(37, 42, 53);
            v.window_fill = Color32::from_rgb(49, 55, 70);
            v.extreme_bg_color = Color32::from_rgb(31, 35, 44);
            v.faint_bg_color = Color32::from_rgb(43, 49, 62);
            v.code_bg_color = Color32::from_rgb(31, 35, 44);
            v.window_stroke = Stroke::new(1.0_f32, Color32::from_rgb(63, 72, 90));
            let text = Color32::from_rgb(216, 222, 233);
            widget(&mut v.widgets.noninteractive, Color32::from_rgb(49, 55, 70), Color32::from_rgb(58, 66, 82), Color32::from_rgb(198, 205, 218), 6.0);
            widget(&mut v.widgets.inactive, Color32::from_rgb(58, 66, 82), Color32::from_rgb(58, 66, 82), text, 6.0);
            widget(&mut v.widgets.hovered, Color32::from_rgb(66, 75, 94), Color32::from_rgb(129, 161, 193), Color32::from_rgb(236, 239, 244), 6.0);
            widget(&mut v.widgets.active, Color32::from_rgb(84, 116, 155), p.accent, Color32::from_rgb(236, 239, 244), 6.0);
            widget(&mut v.widgets.open, Color32::from_rgb(58, 66, 82), Color32::from_rgb(63, 72, 90), text, 6.0);
            v.selection.bg_fill = Color32::from_rgb(84, 116, 155);
            v.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(236, 239, 244));
            v.hyperlink_color = p.accent;
            v.warn_fg_color = Color32::from_rgb(235, 203, 139);
            v.error_fg_color = Color32::from_rgb(191, 97, 106);
            v.window_shadow = Shadow { offset: egui::vec2(0.0, 6.0), blur: 18.0, spread: 0.0, color: Color32::from_black_alpha(90) };
            v.popup_shadow = Shadow { offset: egui::vec2(0.0, 4.0), blur: 12.0, spread: 0.0, color: Color32::from_black_alpha(80) };
        }
        Theme::Studio => {
            v.panel_fill = Color32::from_rgb(17, 19, 24);
            v.window_fill = Color32::from_rgb(22, 25, 31);
            v.extreme_bg_color = Color32::from_rgb(11, 12, 15);
            v.faint_bg_color = Color32::from_rgb(21, 24, 30);
            v.code_bg_color = Color32::from_rgb(11, 12, 15);
            v.window_stroke = Stroke::new(1.0_f32, Color32::from_rgb(38, 42, 51));
            let text = Color32::from_rgb(214, 219, 228);
            widget(&mut v.widgets.noninteractive, Color32::from_rgb(22, 25, 31), Color32::from_rgb(34, 38, 46), Color32::from_rgb(170, 177, 190), 6.0);
            widget(&mut v.widgets.inactive, Color32::from_rgb(30, 34, 42), Color32::from_rgb(30, 34, 42), text, 6.0);
            widget(&mut v.widgets.hovered, Color32::from_rgb(40, 45, 56), Color32::from_rgb(70, 110, 200), Color32::WHITE, 6.0);
            widget(&mut v.widgets.active, Color32::from_rgb(40, 90, 220), p.accent, Color32::WHITE, 6.0);
            widget(&mut v.widgets.open, Color32::from_rgb(30, 34, 42), Color32::from_rgb(50, 56, 68), text, 6.0);
            v.selection.bg_fill = Color32::from_rgb(40, 96, 230);
            v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
            v.hyperlink_color = Color32::from_rgb(110, 160, 255);
            v.warn_fg_color = Color32::from_rgb(240, 180, 80);
            v.error_fg_color = Color32::from_rgb(255, 99, 99);
            v.window_shadow = Shadow { offset: egui::vec2(0.0, 8.0), blur: 24.0, spread: 0.0, color: Color32::from_black_alpha(150) };
            v.popup_shadow = Shadow { offset: egui::vec2(0.0, 4.0), blur: 14.0, spread: 0.0, color: Color32::from_black_alpha(130) };
        }
        Theme::TokyoNight => {
            v.panel_fill = Color32::from_rgb(26, 27, 38);
            v.window_fill = Color32::from_rgb(31, 35, 53);
            v.extreme_bg_color = Color32::from_rgb(22, 22, 30);
            v.faint_bg_color = Color32::from_rgb(30, 32, 46);
            v.code_bg_color = Color32::from_rgb(22, 22, 30);
            v.window_stroke = Stroke::new(1.0_f32, Color32::from_rgb(41, 46, 66));
            let text = Color32::from_rgb(192, 202, 245);
            widget(&mut v.widgets.noninteractive, Color32::from_rgb(31, 35, 53), Color32::from_rgb(41, 46, 66), Color32::from_rgb(169, 177, 214), 6.0);
            widget(&mut v.widgets.inactive, Color32::from_rgb(41, 46, 66), Color32::from_rgb(41, 46, 66), text, 6.0);
            widget(&mut v.widgets.hovered, Color32::from_rgb(52, 59, 88), Color32::from_rgb(122, 162, 247), Color32::from_rgb(220, 226, 255), 6.0);
            widget(&mut v.widgets.active, Color32::from_rgb(61, 89, 161), p.accent, Color32::WHITE, 6.0);
            widget(&mut v.widgets.open, Color32::from_rgb(41, 46, 66), Color32::from_rgb(65, 72, 104), text, 6.0);
            v.selection.bg_fill = Color32::from_rgb(61, 89, 161);
            v.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(192, 202, 245));
            v.hyperlink_color = Color32::from_rgb(125, 207, 255);
            v.warn_fg_color = Color32::from_rgb(224, 175, 104);
            v.error_fg_color = Color32::from_rgb(247, 118, 142);
            v.window_shadow = Shadow { offset: egui::vec2(0.0, 6.0), blur: 18.0, spread: 0.0, color: Color32::from_black_alpha(120) };
            v.popup_shadow = Shadow { offset: egui::vec2(0.0, 4.0), blur: 12.0, spread: 0.0, color: Color32::from_black_alpha(110) };
        }
        Theme::Dark => {
            v.panel_fill = Color32::from_rgb(30, 33, 40);
            v.window_fill = Color32::from_rgb(37, 41, 49);
            v.extreme_bg_color = Color32::from_rgb(21, 23, 28);
            v.faint_bg_color = Color32::from_rgb(35, 39, 47);
            v.code_bg_color = Color32::from_rgb(24, 27, 33);
            v.window_stroke = Stroke::new(1.0_f32, Color32::from_rgb(55, 61, 72));
            let text = Color32::from_rgb(214, 219, 228);
            widget(&mut v.widgets.noninteractive, Color32::from_rgb(37, 41, 49), Color32::from_rgb(50, 55, 65), Color32::from_rgb(190, 196, 206), 6.0);
            widget(&mut v.widgets.inactive, Color32::from_rgb(47, 52, 62), Color32::from_rgb(47, 52, 62), text, 6.0);
            widget(&mut v.widgets.hovered, Color32::from_rgb(60, 67, 80), Color32::from_rgb(90, 110, 145), Color32::WHITE, 6.0);
            widget(&mut v.widgets.active, Color32::from_rgb(70, 80, 98), p.accent, Color32::WHITE, 6.0);
            widget(&mut v.widgets.open, Color32::from_rgb(52, 58, 70), Color32::from_rgb(70, 78, 92), text, 6.0);
            v.selection.bg_fill = Color32::from_rgb(43, 96, 196);
            v.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(225, 235, 255));
            v.hyperlink_color = p.accent;
            v.window_shadow = Shadow { offset: egui::vec2(0.0, 6.0), blur: 18.0, spread: 0.0, color: Color32::from_black_alpha(110) };
            v.popup_shadow = Shadow { offset: egui::vec2(0.0, 4.0), blur: 12.0, spread: 0.0, color: Color32::from_black_alpha(100) };
        }
        Theme::Light => {
            v.panel_fill = Color32::from_rgb(246, 248, 251);
            v.window_fill = Color32::from_rgb(255, 255, 255);
            v.extreme_bg_color = Color32::from_rgb(255, 255, 255);
            v.faint_bg_color = Color32::from_rgb(239, 242, 247);
            v.code_bg_color = Color32::from_rgb(238, 241, 246);
            v.window_stroke = Stroke::new(1.0_f32, Color32::from_rgb(214, 220, 229));
            let text = Color32::from_rgb(36, 42, 54);
            widget(&mut v.widgets.noninteractive, Color32::from_rgb(246, 248, 251), Color32::from_rgb(222, 227, 235), Color32::from_rgb(70, 78, 92), 6.0);
            widget(&mut v.widgets.inactive, Color32::from_rgb(232, 236, 242), Color32::from_rgb(214, 220, 229), text, 6.0);
            widget(&mut v.widgets.hovered, Color32::from_rgb(220, 229, 243), Color32::from_rgb(140, 170, 225), Color32::from_rgb(15, 20, 30), 6.0);
            widget(&mut v.widgets.active, Color32::from_rgb(204, 218, 241), p.accent, Color32::from_rgb(10, 15, 25), 6.0);
            widget(&mut v.widgets.open, Color32::from_rgb(226, 232, 241), Color32::from_rgb(200, 208, 220), text, 6.0);
            v.selection.bg_fill = Color32::from_rgb(198, 220, 255);
            v.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(20, 70, 170));
            v.hyperlink_color = p.accent;
            v.window_shadow = Shadow { offset: egui::vec2(0.0, 6.0), blur: 18.0, spread: 0.0, color: Color32::from_black_alpha(35) };
            v.popup_shadow = Shadow { offset: egui::vec2(0.0, 4.0), blur: 12.0, spread: 0.0, color: Color32::from_black_alpha(30) };
        }
    }
    // ---- aspect "fara chenare" (redesign): adancimea vine din suprafete, nu din contururi.
    // Toate temele mostenesc aceleasi reguli; fiecare isi pastreaza culorile.
    {
        let dark = v.dark_mode;
        // tonul cu cateva unitati mai deschis (inchis in tema Light) decat panoul
        let tone = |c: Color32, d: i16| {
            let f = |x: u8| (x as i16 + if dark { d } else { -d }).clamp(0, 255) as u8;
            Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
        };
        let pf = v.panel_fill;
        // hover fara contur: doar fundalul se lumineaza
        v.widgets.hovered.bg_stroke = Stroke::NONE;
        v.widgets.hovered.weak_bg_fill = tone(pf, 16);
        v.widgets.hovered.bg_fill = tone(pf, 18);
        for w in [&mut v.widgets.inactive, &mut v.widgets.active, &mut v.widgets.open] {
            w.bg_stroke = Stroke::NONE;
        }
        // separatoarele raman doar ca o nuanta abia vizibila
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, tone(pf, 7));
        // selectia: nuanta de accent, fara contur alb; textul selectat in accent deschis
        let a = p.accent;
        v.selection.bg_fill = a.gamma_multiply(0.22);
        let light = |c: u8| ((c as u16 + 255) / 2) as u8;
        v.selection.stroke = Stroke::new(1.0_f32, if dark { Color32::from_rgb(light(a.r()), light(a.g()), light(a.b())) } else { a });
        // dunga tabelelor: se simte, nu se vede
        v.faint_bg_color = tone(pf, 4);
        // ferestrele si panourile: contur vizibil, ca sa iasa usor in evidenta fata de fundal
        v.window_stroke = Stroke::new(1.0_f32, p.border);
        // design system comun tuturor temelor: colturi rotunjite, densitate compacta, umbre
        for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.rounding = Rounding::same(8.0);
        }
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
        style.spacing.interact_size.y = 20.0;
        v.window_rounding = Rounding::same(10.0);
        v.menu_rounding = Rounding::same(8.0);
    }
    v.window_rounding = Rounding::same(10.0);
    v.menu_rounding = Rounding::same(8.0);
    v.striped = true;
    style.visuals = v;
    // compact: mai mult loc pentru model, randuri si butoane tot usor de tintit
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.spacing.interact_size.y = 20.0;
    style.spacing.indent = 14.0;
    style.spacing.menu_margin = egui::Margin::same(5.0);
    style.spacing.window_margin = egui::Margin::same(6.0);
    style.interaction.selectable_labels = false;
    ctx.set_style(style);
}

/// Treptele ribbon-ului (ca in redesign): se pliaza, nu dispare niciodata complet.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub enum RibbonMode {
    /// iconite cu eticheta dedesubt si titlul cardului
    #[default]
    Full,
    /// doar iconite (numele in tooltip) - mai mult loc pentru model
    Compact,
    /// doar randul cu tab-uri; clic pe un tab il deschide
    Mini,
}

impl RibbonMode {
    pub fn next(self) -> RibbonMode {
        match self {
            RibbonMode::Full => RibbonMode::Compact,
            RibbonMode::Compact => RibbonMode::Mini,
            RibbonMode::Mini => RibbonMode::Full,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            RibbonMode::Full => "Full",
            RibbonMode::Compact => "Compact",
            RibbonMode::Mini => "Mini",
        }
    }
    /// inaltimea zonei cu carduri (fara randul de tab-uri)
    pub fn height(self) -> f32 {
        match self {
            RibbonMode::Full => 70.0,
            RibbonMode::Compact => 54.0,
            RibbonMode::Mini => 0.0,
        }
    }
}

/// modul curent al ribbon-ului, citit de butoanele ribbon-ului (functii libere)
pub static RIBBON_COMPACT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// ===================================================================== font Inter + iconite Phosphor

/// Fontul Inter (OFL, github.com/rsms/inter) pentru text si iconitele vectoriale Phosphor (MIT)
/// in locul emoji-urilor; fonturile implicite raman ca rezerva pentru caracterele rare.
pub fn install_fonts(ctx: &egui::Context) {
    let mut f = egui::FontDefinitions::default();
    f.font_data.insert("Inter".into(), egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-Regular.ttf")));
    if let Some(p) = f.families.get_mut(&egui::FontFamily::Proportional) {
        p.insert(0, "Inter".into());
    }
    // iconitele au familia lor ("icons"): Phosphor contine si litere goale (pentru ligaturi), iar Inter
    // are glife proprii in zona privata (U+E000...) - amestecate in aceeasi familie s-ar "fura" reciproc.
    // Emoji-urile ramase raman vizibile prin fonturile de rezerva.
    f.font_data.insert("phosphor".into(), egui_phosphor::Variant::Regular.font_data());
    let mut icons = vec!["phosphor".to_string()];
    if let Some(p) = f.families.get(&egui::FontFamily::Proportional) {
        icons.extend(p.iter().filter(|n| n.as_str() != "Inter").cloned());
    }
    f.families.insert(egui::FontFamily::Name("icons".into()), icons);
    ctx.set_fonts(f);
}

/// fontul iconitelor vectoriale
pub fn icon_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("icons".into()))
}

/// text cu iconita in fata ("🌲 Selection Tree" sau iconita Phosphor + text), ca LayoutJob
/// (iconita in fontul iconitelor, textul in Inter)
pub fn icon_job(icon: &str, text: &str, size: f32, color: Color32) -> egui::text::LayoutJob {
    let mut j = egui::text::LayoutJob::default();
    let fmt = |font: egui::FontId| egui::TextFormat { font_id: font, color, valign: egui::Align::Center, ..Default::default() };
    if !icon.is_empty() {
        j.append(ph(icon), 0.0, fmt(icon_font(size)));
        j.append(" ", 0.0, fmt(egui::FontId::proportional(size)));
    }
    j.append(text, 0.0, fmt(egui::FontId::proportional(size)));
    j
}

/// titlul unui panou ("🌲 Selection Tree") cu iconita vectoriala
pub fn title_job(title: &str, size: f32, color: Color32) -> egui::text::LayoutJob {
    match title.split_once(' ') {
        Some((ic, rest)) if !ic.chars().next().map(|c| c.is_alphanumeric()).unwrap_or(true) => icon_job(ic, rest, size, color),
        _ => icon_job("", title, size, color),
    }
}

/// emoji folosit in interfata -> iconita Phosphor echivalenta (necunoscut = ramane emoji-ul)
pub fn ph(s: &str) -> &str {
    use egui_phosphor::regular as P;
    match s.trim() {
        "📥" => P::DOWNLOAD_SIMPLE,
        "📤" => P::UPLOAD_SIMPLE,
        "⬚" => P::SELECTION,
        "✏" => P::PENCIL_SIMPLE,
        "📋" => P::CLIPBOARD_TEXT,
        "📊" => P::CHART_BAR,
        "🖼" => P::IMAGE,
        "📷" => P::CAMERA,
        "📦" => P::PACKAGE,
        "🏠" => P::HOUSE,
        "🎯" => P::CROSSHAIR,
        "🎨" => P::PALETTE,
        "⛶" => P::CORNERS_OUT,
        "🚶" => P::PERSON_SIMPLE_WALK,
        "🚫" => P::EYE_SLASH,
        "🗑" => P::TRASH,
        "🔗" => P::LINK,
        "🔍" | "🔎" => P::MAGNIFYING_GLASS,
        "🔄" => P::ARROWS_CLOCKWISE,
        "📌" => P::PUSH_PIN,
        "📄" => P::FILE,
        "📁" => P::FOLDER,
        "📂" => P::FOLDER_OPEN,
        "💡" => P::LIGHTBULB,
        "👆" | "🖱" => P::CURSOR,
        "👁" => P::EYE,
        "🎞" | "🎬" => P::FILM_STRIP,
        "🌐" => P::GLOBE,
        "⧉" => P::COPY,
        "⟳" => P::ARROW_CLOCKWISE,
        "⟲" => P::ARROW_COUNTER_CLOCKWISE,
        "✋" => P::HAND,
        "⚠" => P::WARNING,
        "⚙" => P::GEAR,
        "◎" => P::TARGET,
        "▣" => P::CUBE_FOCUS,
        "🧭" => P::COMPASS,
        "🗔" => P::APP_WINDOW,
        "🔲" => P::SQUARE,
        "🔭" => P::PERSPECTIVE,
        "📐" => P::ANGLE,
        "📏" => P::RULER,
        "📅" => P::CALENDAR,
        "💾" => P::FLOPPY_DISK,
        "🏗" => P::BUILDINGS,
        "🌲" => P::TREE_STRUCTURE,
        "🌙" => P::MOON,
        "🌃" => P::MOON_STARS,
        "➕" => P::PLUS,
        "❓" => P::QUESTION,
        "❄" => P::SNOWFLAKE,
        "✖" => P::X,
        "✔" => P::CHECK,
        "✂" => P::SCISSORS,
        "⛔" => P::PROHIBIT,
        "⚖" => P::SCALES,
        "☁" => P::CLOUD,
        "☀" => P::SUN,
        "◆" => P::DIAMOND,
        "▦" => P::GRID_FOUR,
        "⏹" => P::STOP,
        "≡" => P::LIST,
        "ℹ" => P::INFO,
        "▶" => P::PLAY,
        "⏸" => P::PAUSE,
        "↔" => P::ARROWS_OUT_CARDINAL,
        _ => s,
    }
}

/// "🌲 Selection Tree" -> iconita Phosphor + text
pub fn ph_label(s: &str) -> String {
    match s.split_once(' ') {
        Some((ic, rest)) if !ic.chars().next().map(|c| c.is_alphanumeric()).unwrap_or(true) => format!("{} {rest}", ph(ic)),
        _ => s.to_string(),
    }
}
