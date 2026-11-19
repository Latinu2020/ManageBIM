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
    /// bara de navigare plutitoare din vizualizarea 3D
    #[serde(default = "yes")]
    pub nav_bar: bool,
    /// lungimile (masuratori, distante, coordonate) afisate in milimetri in loc de metri
    #[serde(default)]
    pub units_mm: bool,
    /// unitatea XML-ului pentru Navisworks: "auto" (din modelul Navisworks) sau m / cm / mm / ft / in
    #[serde(default = "auto")]
    pub navis_units: String,
}

fn auto() -> String {
    "auto".into()
}

fn yes() -> bool {
    true
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
        Settings { theme: Theme::Dark, auto_cache: true, use_cache: true, recent: Vec::new(), tools: Default::default(), layout: Default::default(), walk_speed: default_walk(), gravity: false, collision: false, eye_height: 1.7, tree_levels: crate::app::default_tree_levels(), ribbon_min: false, nav_bar: true, units_mm: false, navis_units: auto() }
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
        let mut s: Settings = settings_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        // vechiul implicit (3 m/s) -> viteza de mers normala; o valoare aleasa de utilizator ramane
        if s.walk_speed == 3.0 {
            s.walk_speed = default_walk();
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
                let _ = std::fs::write(p, s);
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
        },
        // Nord: Polar Night (fundal), Snow Storm (text), Frost (accent), Aurora (rosu / verde)
        Theme::Nord => Palette {
            accent: Color32::from_rgb(136, 192, 208),
            vp_top: Color32::from_rgb(46, 52, 64),
            vp_bottom: Color32::from_rgb(94, 105, 125),
            overlay_text: Color32::from_rgb(236, 239, 244),
            overlay_weak: Color32::from_rgba_unmultiplied(216, 222, 233, 150),
            overlay_btn: Color32::from_rgba_unmultiplied(46, 52, 64, 215),
            elem_a: Color32::from_rgb(191, 97, 106),
            elem_b: Color32::from_rgb(163, 190, 140),
            gantt_bg: Color32::from_rgb(46, 52, 64),
            gantt_grid: Color32::from_rgb(67, 76, 94),
            gantt_text: Color32::from_rgb(229, 233, 240),
            label_bg: Color32::from_rgba_unmultiplied(46, 52, 64, 225),
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
        },
        // Studio: aproape negru, albastru electric
        Theme::Studio => Palette {
            accent: Color32::from_rgb(52, 120, 255),
            vp_top: Color32::from_rgb(12, 13, 16),
            vp_bottom: Color32::from_rgb(34, 37, 44),
            overlay_text: Color32::from_rgb(226, 230, 238),
            overlay_weak: Color32::from_rgba_unmultiplied(200, 206, 218, 140),
            overlay_btn: Color32::from_rgba_unmultiplied(22, 24, 30, 235),
            elem_a: Color32::from_rgb(255, 99, 99),
            elem_b: Color32::from_rgb(80, 210, 140),
            gantt_bg: Color32::from_rgb(17, 19, 24),
            gantt_grid: Color32::from_rgb(36, 40, 48),
            gantt_text: Color32::from_rgb(220, 224, 232),
            label_bg: Color32::from_rgba_unmultiplied(18, 20, 26, 230),
        },
    }
}

fn widget(v: &mut egui::style::WidgetVisuals, bg: Color32, stroke: Color32, fg: Color32, _r: f32) {
    let r = 3.5;
    v.bg_fill = bg;
    v.weak_bg_fill = bg;
    v.bg_stroke = Stroke::new(1.0_f32, stroke);
    v.fg_stroke = Stroke::new(1.0_f32, fg);
    v.rounding = Rounding::same(r);
}

pub fn apply(ctx: &egui::Context, t: Theme) {
    let p = palette(t);
    ACCENT_RGB.store(((p.accent.r() as u32) << 16) | ((p.accent.g() as u32) << 8) | p.accent.b() as u32, Ordering::Relaxed);
    let mut style = (*ctx.style()).clone();
    let mut v = if t.is_dark() { egui::Visuals::dark() } else { egui::Visuals::light() };
    match t {
        Theme::Nord => {
            v.panel_fill = Color32::from_rgb(46, 52, 64);
            v.window_fill = Color32::from_rgb(59, 66, 82);
            v.extreme_bg_color = Color32::from_rgb(39, 44, 54);
            v.faint_bg_color = Color32::from_rgb(52, 59, 74);
            v.code_bg_color = Color32::from_rgb(39, 44, 54);
            v.window_stroke = Stroke::new(1.0_f32, Color32::from_rgb(76, 86, 106));
            let text = Color32::from_rgb(216, 222, 233);
            widget(&mut v.widgets.noninteractive, Color32::from_rgb(59, 66, 82), Color32::from_rgb(67, 76, 94), Color32::from_rgb(200, 207, 220), 6.0);
            widget(&mut v.widgets.inactive, Color32::from_rgb(67, 76, 94), Color32::from_rgb(67, 76, 94), text, 6.0);
            widget(&mut v.widgets.hovered, Color32::from_rgb(76, 86, 106), Color32::from_rgb(129, 161, 193), Color32::from_rgb(236, 239, 244), 6.0);
            widget(&mut v.widgets.active, Color32::from_rgb(94, 129, 172), p.accent, Color32::from_rgb(236, 239, 244), 6.0);
            widget(&mut v.widgets.open, Color32::from_rgb(67, 76, 94), Color32::from_rgb(76, 86, 106), text, 6.0);
            v.selection.bg_fill = Color32::from_rgb(94, 129, 172);
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
    v.window_rounding = Rounding::same(10.0);
    v.menu_rounding = Rounding::same(8.0);
    v.striped = true;
    style.visuals = v;
    // compact: mai mult loc pentru model, randuri si butoane tot usor de tintit
    style.spacing.item_spacing = egui::vec2(6.0, 3.0);
    style.spacing.button_padding = egui::vec2(6.0, 2.0);
    style.spacing.interact_size.y = 20.0;
    style.spacing.indent = 14.0;
    style.spacing.menu_margin = egui::Margin::same(5.0);
    style.spacing.window_margin = egui::Margin::same(6.0);
    style.interaction.selectable_labels = false;
    ctx.set_style(style);
}
