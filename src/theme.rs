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

// Design tokens pentru raza colturilor (stil Mitcad): tab-uri pill, carduri rotunjite.
pub const RADIUS_SM: f32 = 6.0;  // badge, chip
pub const RADIUS_MD: f32 = 9.0;  // butoane, carduri de ribbon
pub const RADIUS_LG: f32 = 12.0; // panouri, ferestre

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

/// Inaltimea de baza a textului (px); 0 = implicitul egui. Setata din `update()`.
static FONT_BASE_PX: AtomicU32 = AtomicU32::new(0);

pub fn set_font_base(px: f32) {
    FONT_BASE_PX.store(px.to_bits(), Ordering::Relaxed);
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
    /// coloana dupa care se sorteaza rezultatele (None = ordinea testului, dupa distanta)
    #[serde(default)]
    pub sort_col: Option<u8>,
    #[serde(default = "yes")]
    pub sort_asc: bool,
    /// camera zboara lin spre clash (Animate transitions din Navisworks)
    #[serde(default = "yes")]
    pub animate: bool,
    /// panoul Item 1 / Item 2 cu proprietatile elementelor de sub tabel
    #[serde(default = "yes")]
    pub items_panel: bool,
    /// culoarea elementului A / B al clash-ului curent
    #[serde(default = "clash_col_a")]
    pub col_a: [u8; 3],
    #[serde(default = "clash_col_b")]
    pub col_b: [u8; 3],
    /// opacitatea celorlalte elemente la "Dim other" (0 = invizibile, 255 = opace)
    #[serde(default = "dim_alpha_default")]
    pub dim_alpha: u8,
}

pub fn clash_col_a() -> [u8; 3] {
    [230, 45, 40]
}

pub fn clash_col_b() -> [u8; 3] {
    [40, 200, 70]
}

pub fn dim_alpha_default() -> u8 {
    28
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
            sort_col: None,
            sort_asc: true,
            animate: true,
            items_panel: true,
            col_a: clash_col_a(),
            col_b: clash_col_b(),
            dim_alpha: dim_alpha_default(),
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
    /// OSNAP: raza de prindere in pixeli (6..24)
    #[serde(default = "default_snap_radius")]
    pub snap_radius: f32,
    /// OSNAP: tipuri active (bit 1=Vertex, 2=Midpoint, 4=Edge, 8=Surface)
    #[serde(default = "default_snap_types")]
    pub snap_types: u8,
    /// sistem de unitati: "m" | "mm" | "in" | "ft" (completeaza units_mm; vechiul proiect ramane valid)
    #[serde(default)]
    pub unit_system: String,
    /// numar de zecimale la afisarea distantilor (0..6)
    #[serde(default = "default_precision")]
    pub unit_precision: u8,
    /// keybindings customizabile: "open" -> "Ctrl+O" etc. (doar override-urile fata de default)
    #[serde(default)]
    pub keybinds: std::collections::BTreeMap<String, String>,
    /// interfata in romana (altfel engleza)
    #[serde(default)]
    pub romanian: bool,
    /// marimea intregii interfate: 1.0 = 100% (Setari > Interface size / DPI)
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    /// fontul interfetei: „Inter” (folosit implicit) sau „Default” (al egui)
    #[serde(default = "default_font_family")]
    pub font_family: String,
    /// inaltimea de baza a textului in px (0 = 14 px, implicitul)
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    /// directia sectiunii exportate in DWG / DXF (Current = planul de taiere activ)
    #[serde(default)]
    pub cad_view: crate::app::CadView,
    /// pozitia planului de taiere intre 0 (latura minima) si 1 (latura maxima) a modelului
    #[serde(default = "default_cad_view_pos")]
    pub cad_view_pos: f32,
    /// versiunea BCF la export: 2 sau 3
    #[serde(default = "default_bcf_version")]
    pub bcf_version: u8,
    /// inaltimea planului de taiere masurata de la fundul cubului de sectiune
    #[serde(default = "default_cut_offset")]
    pub section_cut_offset: f32,
    /// aliniaza planul de taiere la axa de grila / nivelul cel mai apropiat
    #[serde(default = "yes")]
    pub snap_cut_to_grid: bool,
    /// folderul cu scripturi .rhai (gol = %APPDATA%\ManageBIM\Scripts)
    #[serde(default)]
    pub scripts_dir: String,
    /// valorile parametrilor pentru scripturile din catalogul grafic
    #[serde(default)]
    pub script_params: std::collections::BTreeMap<String, String>,
    /// afiseaza in 3D planul de taiere ales pentru export (implicit OPRIT)
    #[serde(default)]
    pub show_cad_plane: bool,
    /// afiseaza sarmul cubului de sectiune si sagetile fetelor (fara a afecta taietura)
    #[serde(default = "yes")]
    pub show_section_box: bool,
    /// muchiile din spatele planului (pana la peretele opus al cubului), punctate
    #[serde(default = "yes")]
    pub show_section_behind: bool,
    /// eticheta folderului virtual „fară folder” din Viewpoints
    #[serde(default = "default_vp_root")]
    pub vp_root_label: String,
    /// unitatea XML-ului pentru Navisworks: "auto" (din modelul Navisworks) sau m / cm / mm / ft / in
    #[serde(default = "auto")]
    pub navis_units: String,
    /// preferinte UI persistente (Clash Detective / 4D)
    #[serde(default)]
    pub clash_ui: ClashUiPrefs,
    /// culoarea de evidentiere a elementelor selectate in 3D
    #[serde(default = "default_sel_rgb")]
    pub sel_rgb: [u8; 3],
    /// cat de puternic acopera culoarea selectiei culoarea elementului (0.2..1.0)
    #[serde(default = "default_sel_mix")]
    pub sel_mix: f32,
    /// ultimele comenzi rulate din paleta (cea mai noua prima)
    #[serde(default)]
    pub recent_cmds: Vec<String>,
    /// iconitele din ribbon colorate pe categorii (altfel monocrome)
    #[serde(default = "yes")]
    pub color_icons: bool,
    /// fundalul vederii 3D: "theme" (al temei), "white", "grey", "black", "sky"
    #[serde(default = "default_vp_bg")]
    pub vp_bg: String,
    /// Render (path trace): curata zgomotul cu Intel Open Image Denoise la final
    #[serde(default = "yes")]
    pub render_denoise: bool,
    /// rezolutia texturilor PBR pe GPU: 0 = 512, 1 = 1024, 2 = 2048
    #[serde(default)]
    pub tex_res: u32,
    /// dezactiveaza LOD-ul adaptiv in viewport (deseneaza toate elementele, si in miscare)
    #[serde(default)]
    pub no_lod: bool,
    /// Model View Definition folosit la exportul IFC
    #[serde(default)]
    pub ifc_mvd: crate::ifc_mvd::Mvd,
    /// mapare categorie sursa -> clasa IFC la export
    #[serde(default = "crate::ifc_map::default_rules")]
    pub ifc_map: Vec<crate::ifc_map::MapRule>,
    /// catalog de clasificare local (bSDD / Uniclass / OmniClass)
    #[serde(default = "crate::classification::builtin")]
    pub classifications: crate::classification::Catalog,
    /// verifica la pornire daca exista o versiune noua pe GitHub (doar citeste pagina publica)
    #[serde(default = "crate::app::yes_default")]
    pub check_updates: bool,
}

fn default_vp_bg() -> String {
    "theme".into()
}

pub fn default_sel_rgb() -> [u8; 3] {
    [60, 140, 255]
}

pub fn default_sel_mix() -> f32 {
    0.7
}

fn auto() -> String {
    "auto".into()
}

fn yes() -> bool {
    true
}

fn default_precision() -> u8 {
    3
}

fn default_snap_radius() -> f32 {
    12.0
}

fn default_snap_types() -> u8 {
    0b1111 // Vertex | Midpoint | Edge | Surface
}

fn default_ui_scale() -> f32 {
    1.0
}

fn default_font_family() -> String {
    "Inter".into()
}

fn default_font_size() -> f32 {
    0.0
}

fn default_vp_root() -> String {
    "No folder".into()
}

fn default_cad_view_pos() -> f32 {
    // 0 = planul EXACT pe fata aleasa a cubului (cubul e format din planuri)
    0.0
}

fn default_cut_offset() -> f32 {
    1.2
}

fn default_bcf_version() -> u8 {
    2
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
        Settings { theme: Theme::Dark, auto_cache: true, use_cache: true, recent: Vec::new(), tools: Default::default(), layout: Default::default(), walk_speed: default_walk(), gravity: false, collision: false, eye_height: 1.7, tree_levels: crate::app::default_tree_levels(), ribbon_min: false, ribbon_mode: RibbonMode::Full, sync_selection: false, sync_follow: true, visual_style: crate::render::VisualStyle::Realistic, nav_bar: true, units_mm: false, navis_units: auto(), romanian: false, ui_scale: default_ui_scale(), font_family: default_font_family(), font_size: default_font_size(), cad_view: crate::app::CadView::Current, cad_view_pos: default_cad_view_pos(), bcf_version: default_bcf_version(), section_cut_offset: default_cut_offset(), snap_cut_to_grid: true, scripts_dir: String::new(), script_params: std::collections::BTreeMap::new(), show_cad_plane: false, show_section_box: true, show_section_behind: true, snap_radius: default_snap_radius(), snap_types: default_snap_types(), unit_system: "m".into(), unit_precision: default_precision(), keybinds: std::collections::BTreeMap::new(), vp_root_label: default_vp_root(), clash_ui: ClashUiPrefs::default(), sel_rgb: default_sel_rgb(), sel_mix: default_sel_mix(), recent_cmds: Vec::new(), color_icons: true, vp_bg: default_vp_bg(), render_denoise: true, tex_res: 0, no_lod: false, ifc_mvd: crate::ifc_mvd::Mvd::Reference, ifc_map: crate::ifc_map::default_rules(), classifications: crate::classification::builtin(), check_updates: true }
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
    /// Exportă setările curente într-un fișier .mbprofile (partajabil cu colegii).
    pub fn export_profile(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Importă setările dintr-un .mbprofile. `recent` și `tools` (căi locale) nu se preiau.
    pub fn import_profile(path: &std::path::Path) -> Result<Settings, String> {
        let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut p: Settings = serde_json::from_str(&s).map_err(|e| e.to_string())?;
        p.recent.clear();
        p.tools = Default::default();
        Ok(p)
    }

    pub fn load() -> Settings {
        // setarile; daca fisierul e stricat (cadere in timpul scrierii), copia .bak de la salvarea anterioara
        let read = |p: std::path::PathBuf| std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str::<Settings>(&s).ok());
        let mut s: Settings = settings_path()
            .and_then(|p| read(p.clone()).or_else(|| read(crate::formats::backup_path(&p))))
            .unwrap_or_default();
        // 27.11: highlightul planului e intodeauna INCHIS la pornire;
        // utilizatorul il deschide doar cand are nevoie de manerul de tragere.
        s.show_cad_plane = false;
        // migrare units_mm -> unit_system (proiectele vechi raman valide)
        if s.unit_system.is_empty() {
            s.unit_system = if s.units_mm { "mm".into() } else { "m".into() };
        }
        crate::app::set_unit(match s.unit_system.as_str() {
            "mm" => crate::app::Unit::Mm,
            "in" => crate::app::Unit::In,
            "ft" => crate::app::Unit::Ft,
            _ => crate::app::Unit::M,
        });
        crate::app::set_precision(s.unit_precision);
        // vechiul implicit (3 m/s) -> viteza de mers normala; o valoare aleasa de utilizator ramane
        if s.walk_speed == 3.0 {
            s.walk_speed = default_walk();
        }
        // marimea interfetei: doar valori rezonabile, altfel revenim la 100%
        if !s.ui_scale.is_finite() || !(0.75..=2.0).contains(&s.ui_scale) {
            s.ui_scale = default_ui_scale();
        }
        if !s.sel_mix.is_finite() || !(0.2..=1.0).contains(&s.sel_mix) {
            s.sel_mix = default_sel_mix();
        }
        s.recent_cmds.truncate(8);
        // vechile implicituri ale pozitiei planului (0.5 / 0.25 = in interiorul
        // cubului) migram pe 0 = planul chiar pe fata aleasa
        if (s.cad_view_pos - 0.5).abs() < 1e-6 || (s.cad_view_pos - 0.25).abs() < 1e-6 {
            s.cad_view_pos = 0.0;
        }
        if !s.cad_view_pos.is_finite() || !(0.0..=1.0).contains(&s.cad_view_pos) {
            s.cad_view_pos = default_cad_view_pos();
        }
        if !s.font_size.is_finite() || !(0.0..=32.0).contains(&s.font_size) {
            s.font_size = default_font_size();
        }
        if s.font_family.trim().is_empty() {
            s.font_family = default_font_family();
        }
        if s.bcf_version != 2 && s.bcf_version != 3 {
            s.bcf_version = default_bcf_version();
        }
        if s.tex_res > 2 {
            s.tex_res = 0;
        }
        crate::materials::set_tex_tier(s.tex_res);
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
        v.window_rounding = Rounding::same(RADIUS_LG - 2.0);
        v.menu_rounding = Rounding::same(RADIUS_MD);
    }
    v.window_rounding = Rounding::same(RADIUS_LG - 2.0);
    v.menu_rounding = Rounding::same(RADIUS_MD);
    v.striped = true;
    style.visuals = v;
    // compact: mai mult loc pentru model, randuri si butoane tot usor de tintit
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.spacing.interact_size.y = 20.0;
    style.spacing.indent = 14.0;
    style.spacing.menu_margin = egui::Margin::same(6.0);
    // ferestrele respira putin mai mult (aspect mai profesional), fara sa piarda densitatea
    style.spacing.window_margin = egui::Margin::symmetric(10.0, 8.0);
    // bare de derulare subtiri, care apar doar la nevoie (ca in aplicatiile moderne)
    style.spacing.scroll = egui::style::ScrollStyle { bar_width: 8.0, floating_width: 3.0, ..egui::style::ScrollStyle::floating() };
    // tooltip-urile: apar mai repede si au latime de citit
    style.spacing.tooltip_width = 340.0;
    style.interaction.tooltip_delay = 0.35;
    style.animation_time = 0.12;
    style.interaction.selectable_labels = false;
    // inaltimea textului aleasa de utilizator (0 = implicitul egui: Body 14 px)
    let base = f32::from_bits(FONT_BASE_PX.load(Ordering::Relaxed));
    if base > 0.0 && base.is_finite() {
        use egui::TextStyle::*;
        style.text_styles.insert(Small, egui::FontId::proportional(base * 0.85));
        style.text_styles.insert(Body, egui::FontId::proportional(base));
        style.text_styles.insert(Button, egui::FontId::proportional(base));
        style.text_styles.insert(Heading, egui::FontId::proportional(base * 1.3));
        style.text_styles.insert(Monospace, egui::FontId::monospace(base * 0.95));
    }
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
/// iconitele din ribbon colorate pe categorii (Settings › Appearance › Coloured icons)
pub static COLOR_ICONS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Culoarea unei iconite dupa ce face comanda (fisiere galben, export albastru, stergere rosu ...).
/// None = culoarea textului (comenzi neutre).
pub fn icon_tint(s: &str, dark: bool) -> Option<Color32> {
    // nuante potrivite pe fundal inchis; pe tema deschisa le inchidem putin
    let c = |r: u8, g: u8, b: u8| {
        if dark {
            Color32::from_rgb(r, g, b)
        } else {
            Color32::from_rgb((r as f32 * 0.78) as u8, (g as f32 * 0.78) as u8, (b as f32 * 0.78) as u8)
        }
    };
    let yellow = c(240, 190, 70);
    let blue = c(90, 160, 255);
    let red = c(240, 95, 90);
    let green = c(80, 200, 120);
    let purple = c(175, 130, 250);
    let orange = c(250, 150, 60);
    let teal = c(70, 200, 200);
    Some(match s.trim() {
        "📁" | "📂" | "📄" | "💾" | "⚡" => yellow,
        "📤" | "📥" | "🌐" | "🔗" | "☁" => blue,
        "🗑" | "⛔" | "🚫" | "✖" | "⚠" | "⏹" => red,
        "📏" | "📐" | "◎" | "🎯" | "✔" | "▶" | "🛡" | "🟩" => green,
        "👁" | "🎨" | "🖼" | "📷" | "🎞" | "🎬" | "💡" | "☀" | "🌙" | "🌃" | "📦" => purple,
        "✂" | "⬚" | "▣" | "🔲" | "▦" | "🏗" | "◆" | "▤" | "▨" => orange,
        "🔍" | "🔎" | "📋" | "📊" | "🧭" | "📅" | "🌲" | "🛤" | "🚶" => teal,
        _ => return None,
    })
}

// ===================================================================== font Inter + iconite Phosphor

/// Fontul Inter (OFL, github.com/rsms/inter) pentru text si iconitele vectoriale Phosphor (MIT)
/// in locul emoji-urilor; fonturile implicite raman ca rezerva pentru caracterele rare.
pub fn install_fonts(ctx: &egui::Context) {
    install_fonts_with(ctx, "Inter");
}

/// Fonturile din sistem (Windows: C:\Windows\Fonts + cele per-utilizator), citite o singura data.
pub fn system_fonts() -> &'static Vec<(String, String)> {
    static SYS: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
    SYS.get_or_init(|| {
        let mut out: Vec<(String, String)> = Vec::new();
        let dirs = [
            std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into())).join("Fonts"),
            std::env::var_os("LOCALAPPDATA").map(|d| std::path::PathBuf::from(d).join("Microsoft\\Windows\\Fonts")).unwrap_or_default(),
        ];
        for d in dirs {
            if let Ok(rd) = std::fs::read_dir(&d) {
                for e in rd.flatten() {
                    let p = e.path();
                    let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
                    if ext == "ttf" || ext == "otf" {
                        let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        if !name.is_empty() {
                            out.push((name, p.to_string_lossy().to_string()));
                        }
                    }
                }
            }
        }
        out.sort();
        out.dedup();
        out
    })
}

/// Fontul proportional al interfetei: „Inter” (implicit), „IBM Plex Sans”,
/// „Default” (fonturile egui) sau orice font din sistem (vezi `system_fonts`).
pub fn install_fonts_with(ctx: &egui::Context, family: &str) {
    let mut f = egui::FontDefinitions::default();
    let name = family.trim();
    // fonturile incluse in aplicatie (embed in executabil, fara fisiere externe)
    let bundled: Option<(&'static str, Vec<u8>)> = match name.to_ascii_lowercase().as_str() {
        "inter" => Some(("Inter", include_bytes!("../assets/fonts/Inter-Regular.ttf").to_vec())),
        "lato" => Some(("Lato", include_bytes!("../assets/fonts/Lato-Regular.ttf").to_vec())),
        _ => None,
    };
    if let Some((key, bytes)) = bundled {
        f.font_data.insert(key.into(), egui::FontData::from_owned(bytes));
        if let Some(p) = f.families.get_mut(&egui::FontFamily::Proportional) {
            p.insert(0, key.into());
        }
    } else if !name.eq_ignore_ascii_case("default") {
        // font din sistem: incarcat din fisierul lui; daca nu exista, revenim la Inter
        let hit = system_fonts().iter().find(|(n, _)| n.eq_ignore_ascii_case(name));
        let bytes = hit.and_then(|(_, p)| std::fs::read(p).ok());
        if let Some(bytes) = bytes {
            f.font_data.insert(name.into(), egui::FontData::from_owned(bytes));
            if let Some(p) = f.families.get_mut(&egui::FontFamily::Proportional) {
                p.insert(0, name.into());
            }
        } else {
            f.font_data.insert("Inter".into(), egui::FontData::from_owned(include_bytes!("../assets/fonts/Inter-Regular.ttf").to_vec()));
            if let Some(p) = f.families.get_mut(&egui::FontFamily::Proportional) {
                p.insert(0, "Inter".into());
            }
        }
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
        "🛤" => P::PATH,
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
        // 0.28: toate iconitele din ribbon au varianta vectoriala (fara patrate goale)
        "#" => P::HASH,
        "↶" => P::ARROW_U_UP_LEFT,
        "⏩" => P::FAST_FORWARD,
        "▤" => P::STACK,
        "▨" => P::CHECKERBOARD,
        "▲" => P::CARET_UP,
        "▼" => P::CARET_DOWN,
        "◫" => P::COLUMNS,
        "⚡" => P::LIGHTNING,
        "⬇" => P::ARROW_DOWN,
        "🏷" => P::TAG,
        "📜" => P::SCROLL,
        "🗺" => P::MAP_TRIFOLD,
        "🟩" => P::CUBE_TRANSPARENT,
        "🛡" => P::SHIELD_CHECK,
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
