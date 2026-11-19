//! Starea aplicatiei si logica (fara desenarea panourilor, care e in ui.rs).
use crate::camera::Camera;
use crate::clash::{self, ClashStatus, ClashType};
use crate::model::{Aabb, ElemId, Element, Model};
use crate::render::{self, FrameParams, Renderer, Upload};
use crate::timeline::{self, SimState, Task};
use glam::{DVec3, Mat4, Vec3, Vec4};
use crate::anim::{Animation, Background, RenderSettings};
use crate::video::{Frame, VideoFormat};
use crate::materials::{MatAssign, MatTarget, Material};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[path = "walk.rs"]
pub mod walk;

/// Cum arata un element pe ecran. Pastram ultimul aspect trimis la GPU ca sa actualizam
/// doar elementele care s-au schimbat (un click de selectie nu mai retrimite tot modelul).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Look {
    /// culoare impusa (override / clash / 4D / material); None = culoarea din fisier
    pub col: Option<[u8; 4]>,
    /// parametrii materialului pentru shader
    pub gpu: [u8; 4],
    pub sel: bool,
    pub hidden: bool,
}

/// Culorile + materialele (4 octeti / varf, 3 varfuri / triunghi) pentru triunghiurile unui element.
fn fill_look(l: &Look, tri_color: &[[u8; 4]], col: &mut [u8], mat: &mut [u8]) {
    for (t, tc) in tri_color.iter().enumerate() {
        let mut c = l.col.unwrap_or(*tc);
        if l.sel {
            c = [(c[0] as f32 * 0.3 + 60.0 * 0.7) as u8, (c[1] as f32 * 0.3 + 140.0 * 0.7) as u8, (c[2] as f32 * 0.3 + 255.0 * 0.7) as u8, c[3].max(200)];
        }
        if l.hidden {
            c[3] = 0;
        }
        for v in 0..3 {
            let o = (t * 3 + v) * 4;
            col[o..o + 4].copy_from_slice(&c);
            mat[o..o + 4].copy_from_slice(&l.gpu);
        }
    }
}

/// Tot bufferul de culori al unui model, umplut in paralel (fiecare element are intervalul lui).
fn full_colors(slot: u32, m: &Model, looks: &[Look]) -> Upload {
    use rayon::prelude::*;
    let n = m.tri_color.len() * 12;
    let (mut col, mut mat) = (vec![0u8; n], vec![0u8; n]);
    {
        let mut jobs = Vec::with_capacity(m.elements.len());
        let (mut rc, mut rm) = (&mut col[..], &mut mat[..]);
        for (ei, e) in m.elements.iter().enumerate() {
            let len = e.tri_count as usize * 12;
            let (c, rest_c) = std::mem::take(&mut rc).split_at_mut(len);
            let (k, rest_m) = std::mem::take(&mut rm).split_at_mut(len);
            rc = rest_c;
            rm = rest_m;
            jobs.push((ei, c, k));
        }
        jobs.into_par_iter().with_min_len(64).for_each(|(ei, c, k)| {
            let e = &m.elements[ei];
            fill_look(&looks[ei], &m.tri_color[e.tri_start as usize..(e.tri_start + e.tri_count) as usize], c, k);
        });
    }
    Upload::Colors { slot, col, mat }
}

/// Adauga extensia daca lipseste ("raport" -> "raport.csv").
fn with_ext(path: &str, ext: &str) -> String {
    if path.to_lowercase().ends_with(&format!(".{ext}")) {
        path.to_string()
    } else {
        format!("{path}.{ext}")
    }
}

/// Valorile proprietatii (categorie, nume) ale unui element. Categoria "Item" = campurile standard.
pub fn elem_values<'a>(m: &'a Model, e: &'a Element, cat: &str, prop: &str) -> Vec<&'a str> {
    if cat == ITEM_CAT {
        let v = match prop {
            "Name" => e.name.as_str(),
            "IFC Type" => e.ifc_type.as_str(),
            "Level" => e.storey.as_str(),
            "Model" => m.name.as_str(),
            "GlobalId" => e.guid.as_str(),
            _ => return Vec::new(),
        };
        return vec![v];
    }
    e.props.iter().filter(|p| p.group.eq_ignore_ascii_case(cat) && p.name.eq_ignore_ascii_case(prop)).map(|p| p.value.as_str()).collect()
}

/// Data ultimei modificari a unui fisier (None daca nu exista / nu se poate citi).
fn file_mtime(path: &str) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn file_stem(path: &str) -> String {
    std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// %LOCALAPPDATA%\ManageBIM\Inbox - aici scriu add-in-urile cand trimit modelul direct in ManageBIM.
pub fn inbox_dir() -> Option<std::path::PathBuf> {
    let d = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("ManageBIM").join("Inbox");
    std::fs::create_dir_all(&d).ok()?;
    Some(d)
}

// ====================================================================== tipuri

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Select,
    /// selectie cu fereastra: stanga->dreapta = complet in interior, dreapta->stanga = atinge (ca AutoCAD)
    Window,
    Measure(MeasureKind),
    Redline(crate::markup::RedKind),
    RedErase,
    /// click pe podea: puncte ale traseului de mers
    WalkPath,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum MeasureKind {
    PointToPoint,
    Polyline,
    Angle,
    Area,
}

impl MeasureKind {
    pub fn label(&self) -> &'static str {
        match self {
            MeasureKind::PointToPoint => "Point to Point",
            MeasureKind::Polyline => "Point Line",
            MeasureKind::Angle => "Angle",
            MeasureKind::Area => "Area",
        }
    }
    pub fn needed(&self) -> Option<usize> {
        match self {
            MeasureKind::PointToPoint => Some(2),
            MeasureKind::Angle => Some(3),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Measure {
    pub kind: MeasureKind,
    pub pts: Vec<[f32; 3]>,
}

impl Measure {
    pub fn value(&self) -> String {
        let p: Vec<Vec3> = self.pts.iter().map(|p| Vec3::from(*p)).collect();
        match self.kind {
            MeasureKind::PointToPoint | MeasureKind::Polyline => {
                if p.len() < 2 {
                    return String::new();
                }
                let tot: f32 = p.windows(2).map(|w| (w[1] - w[0]).length()).sum();
                if self.kind == MeasureKind::PointToPoint {
                    let d = p[1] - p[0];
                    format!("{}  (ΔX {}  ΔY {}  ΔZ {})", fmt_len(tot as f64), fmt_num(d.x as f64), fmt_num(d.y as f64), fmt_num(d.z as f64))
                } else {
                    format!("{}  ({} segments)", fmt_len(tot as f64), p.len() - 1)
                }
            }
            MeasureKind::Angle => {
                if p.len() < 3 {
                    return String::new();
                }
                let a = (p[0] - p[1]).normalize_or_zero();
                let b = (p[2] - p[1]).normalize_or_zero();
                format!("{:.2}°", a.dot(b).clamp(-1.0, 1.0).acos().to_degrees())
            }
            MeasureKind::Area => {
                if p.len() < 3 {
                    return String::new();
                }
                let mut n = Vec3::ZERO;
                for i in 0..p.len() {
                    n += p[i].cross(p[(i + 1) % p.len()]);
                }
                format!("{} m²", f3(n.length() * 0.5))
            }
        }
    }
}

pub fn f3(v: f32) -> String {
    format!("{v:.3}")
}

/// Unitatea de afisare a lungimilor (masuratori, distante, coordonate): metri sau milimetri.
static UNITS_MM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_units_mm(mm: bool) {
    UNITS_MM.store(mm, std::sync::atomic::Ordering::Relaxed);
}

pub fn units_mm() -> bool {
    UNITS_MM.load(std::sync::atomic::Ordering::Relaxed)
}

/// Lungime (metri) in unitatea aleasa: "1.250 m" sau "1250 mm".
pub fn fmt_len(m: f64) -> String {
    if units_mm() {
        format!("{:.0} mm", m * 1000.0)
    } else {
        format!("{m:.3} m")
    }
}

/// Numar fara unitate, in unitatea aleasa (ex. componentele ΔX / ΔY / ΔZ).
pub fn fmt_num(m: f64) -> String {
    if units_mm() {
        format!("{:.0}", m * 1000.0)
    } else {
        format!("{m:.3}")
    }
}

pub fn unit_label() -> &'static str {
    if units_mm() {
        "mm"
    } else {
        "m"
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum SearchOp {
    Contains,
    Equals,
    NotEquals,
    Defined,
    NotContains,
    Greater,
    Less,
    Undefined,
}

impl SearchOp {
    pub const ALL: [SearchOp; 8] = [
        SearchOp::Equals,
        SearchOp::NotEquals,
        SearchOp::Contains,
        SearchOp::NotContains,
        SearchOp::Greater,
        SearchOp::Less,
        SearchOp::Defined,
        SearchOp::Undefined,
    ];
    pub fn needs_value(&self) -> bool {
        !matches!(self, SearchOp::Defined | SearchOp::Undefined)
    }
    pub fn label(&self) -> &'static str {
        match self {
            SearchOp::Contains => "contains",
            SearchOp::Equals => "=",
            SearchOp::NotEquals => "≠",
            SearchOp::Defined => "defined",
            SearchOp::NotContains => "does not contain",
            SearchOp::Greater => ">",
            SearchOp::Less => "<",
            SearchOp::Undefined => "undefined",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchRule {
    pub field: String,
    pub op: SearchOp,
    pub value: String,
    /// this row starts a new OR block (Navisworks "OR condition")
    #[serde(default)]
    pub or: bool,
    /// property category (property set / group). Empty = legacy field match by name only.
    #[serde(default)]
    pub group: String,
}

impl SearchRule {
    pub fn new(group: &str, field: &str, op: SearchOp, value: &str, or: bool) -> SearchRule {
        SearchRule { field: field.into(), op, value: value.into(), or, group: group.into() }
    }
}

/// Synthetic category with the basic item data (like Navisworks "Item").
pub const ITEM_CAT: &str = "Item";
pub const ITEM_PROPS: [&str; 5] = ["Name", "IFC Type", "Level", "Model", "GlobalId"];
/// Above this many distinct values a property is not listed value by value.
pub const MAX_LISTED_VALUES: usize = 3000;

#[derive(Clone, Debug, Default)]
pub struct PropValues {
    pub name: String,
    /// value -> element count, sorted by value
    pub values: Vec<(String, u32)>,
    pub distinct: usize,
    pub elements: u32,
}

/// Every category / property / value present in all loaded models.
#[derive(Clone, Debug, Default)]
pub struct PropIndex {
    pub cats: Vec<(String, Vec<PropValues>)>,
}

impl PropIndex {
    pub fn props(&self, cat: &str) -> Option<&Vec<PropValues>> {
        self.cats.iter().find(|c| c.0 == cat).map(|c| &c.1)
    }
    pub fn prop(&self, cat: &str, name: &str) -> Option<&PropValues> {
        self.props(cat)?.iter().find(|p| p.name == name)
    }
    pub fn count(&self) -> usize {
        self.cats.iter().map(|c| c.1.len()).sum()
    }
}

/// Leading number of a property value ("2800 mm" -> 2800, "1,5 m" -> 1.5)
pub fn leading_number(s: &str) -> Option<f64> {
    let t = s.trim().replace(',', ".");
    let end = t.char_indices().take_while(|(i, c)| c.is_ascii_digit() || *c == '.' || ((*c == '-' || *c == '+') && *i == 0) || *c == 'e' || *c == 'E').map(|(i, c)| i + c.len_utf8()).last()?;
    t[..end].parse().ok()
}

pub const STD_FIELDS: [&str; 6] = ["Any", "Name", "IFC Type", "Level", "Model", "GlobalId"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SetKind {
    Explicit(Vec<ElemId>),
    Search(Vec<SearchRule>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelSet {
    pub name: String,
    pub kind: SetKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum SelKind {
    All,
    Model,
    Set,
    Type,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Selector {
    pub kind: SelKind,
    pub value: String,
}

impl Selector {
    pub fn label(&self) -> String {
        match self.kind {
            SelKind::All => "All elements".into(),
            SelKind::Model => format!("Model: {}", self.value),
            SelKind::Set => format!("Set: {}", self.value),
            SelKind::Type => format!("Type: {}", self.value),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClashResult {
    pub name: String,
    pub a: ElemId,
    pub b: ElemId,
    pub ga: String,
    pub gb: String,
    pub point: [f32; 3],
    pub dist: f32,
    pub status: ClashStatus,
    pub assigned: String,
    pub comment: String,
    pub found: String,
    /// clash group name ("" = not grouped)
    #[serde(default)]
    pub group: String,
}

/// Navisworks-style rules: what the test must ignore.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClashRules {
    /// ignore clashes between items on the same level
    #[serde(default)]
    pub same_level: bool,
    /// ignore clashes between items of the same IFC type / category
    #[serde(default)]
    pub same_type: bool,
    /// ignore clashes between items with the same value of these properties (e.g. "System")
    #[serde(default)]
    pub same_props: Vec<String>,
    /// ignore clashes between items that are both in one of these sets
    #[serde(default)]
    pub same_sets: Vec<String>,
    /// items in these sets never clash
    #[serde(default)]
    pub exclude_sets: Vec<String>,
    /// items matching any of these rules never clash
    #[serde(default)]
    pub exclude_rules: Vec<SearchRule>,
}

impl ClashRules {
    pub fn count(&self, same_file: bool) -> usize {
        same_file as usize + self.same_level as usize + self.same_type as usize + self.same_props.len() + self.same_sets.len() + self.exclude_sets.len() + self.exclude_rules.len()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GroupBy {
    Level,
    ItemA,
    ItemB,
    TypePair,
    Proximity,
    /// intersectia de axe cea mai apropiata + nivelul (ex. "C-4 / Level 2")
    Grid,
}

impl GroupBy {
    pub const ALL: [GroupBy; 6] = [GroupBy::Grid, GroupBy::Level, GroupBy::ItemA, GroupBy::ItemB, GroupBy::TypePair, GroupBy::Proximity];
    pub fn label(&self) -> &'static str {
        match self {
            GroupBy::Level => "Level",
            GroupBy::ItemA => "Item A",
            GroupBy::ItemB => "Item B",
            GroupBy::TypePair => "Type A × Type B",
            GroupBy::Proximity => "Proximity (grid cell)",
            GroupBy::Grid => "Grid location (axes / level)",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Others {
    Dim,
    Hide,
    Normal,
}

impl Others {
    pub fn label(&self) -> &'static str {
        match self {
            Others::Dim => "Dim other",
            Others::Hide => "Hide other (clashes only)",
            Others::Normal => "Show other normally",
        }
    }
}

/// Elements and points currently shown as clashing.
#[derive(Clone, Debug, Default)]
pub struct ClashFocus {
    pub a: Vec<ElemId>,
    pub b: Vec<ElemId>,
    pub points: Vec<Vec3>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClashTest {
    pub name: String,
    pub a: Selector,
    pub b: Selector,
    pub ty: ClashType,
    pub tol_mm: f32,
    pub ignore_same_file: bool,
    pub results: Vec<ClashResult>,
    pub last_run: String,
    #[serde(default)]
    pub rules: ClashRules,
}

impl ClashTest {
    pub fn new(name: String) -> ClashTest {
        ClashTest {
            name,
            a: Selector { kind: SelKind::All, value: String::new() },
            b: Selector { kind: SelKind::All, value: String::new() },
            ty: ClashType::Hard,
            tol_mm: 10.0,
            ignore_same_file: false,
            results: Vec::new(),
            last_run: String::new(),
            rules: ClashRules::default(),
        }
    }
    pub fn count(&self, s: ClashStatus) -> usize {
        self.results.iter().filter(|r| r.status == s).count()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum SectionMode {
    Planes,
    Box,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PlaneDef {
    pub enabled: bool,
    /// 0 Sus(+Z) 1 Jos(-Z) 2 +X 3 -X 4 +Y 5 -Y
    pub dir: u8,
    pub offset: f32,
}

pub const PLANE_DIRS: [&str; 6] = ["Top (cut above)", "Bottom (cut below)", "+X", "−X", "+Y", "−Y"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Section {
    pub enabled: bool,
    pub mode: SectionMode,
    pub planes: Vec<PlaneDef>,
    pub bmin: [f32; 3],
    pub bmax: [f32; 3],
}

impl Default for Section {
    fn default() -> Self {
        Section {
            enabled: false,
            mode: SectionMode::Planes,
            planes: vec![PlaneDef { enabled: true, dir: 0, offset: 0.0 }],
            bmin: [-1.0; 3],
            bmax: [1.0; 3],
        }
    }
}

impl Section {
    /// planuri (n, w): pastreaza punctele cu n·p <= w
    pub fn gpu_planes(&self) -> Vec<Vec4> {
        if !self.enabled {
            return Vec::new();
        }
        match self.mode {
            SectionMode::Planes => self
                .planes
                .iter()
                .filter(|p| p.enabled)
                .take(6)
                .map(|p| {
                    let (n, s) = match p.dir {
                        0 => (Vec3::Z, 1.0),
                        1 => (-Vec3::Z, -1.0),
                        2 => (Vec3::X, 1.0),
                        3 => (-Vec3::X, -1.0),
                        4 => (Vec3::Y, 1.0),
                        _ => (-Vec3::Y, -1.0),
                    };
                    n.extend(p.offset * s)
                })
                .collect(),
            SectionMode::Box => {
                let (a, b) = (self.bmin, self.bmax);
                vec![
                    Vec4::new(1.0, 0.0, 0.0, b[0]),
                    Vec4::new(-1.0, 0.0, 0.0, -a[0]),
                    Vec4::new(0.0, 1.0, 0.0, b[1]),
                    Vec4::new(0.0, -1.0, 0.0, -a[1]),
                    Vec4::new(0.0, 0.0, 1.0, b[2]),
                    Vec4::new(0.0, 0.0, -1.0, -a[2]),
                ]
            }
        }
    }
}

/// Per-model placement (like Navisworks "Units and Transform").
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelXf {
    /// translation in metres (world X, Y, Z)
    pub translate: [f64; 3],
    /// rotation about the vertical axis, degrees
    pub rot_z: f64,
    pub scale: f64,
    /// rotate / scale about the world origin (true) or about the model centre (false)
    pub pivot_origin: bool,
}

impl Default for ModelXf {
    fn default() -> Self {
        ModelXf { translate: [0.0; 3], rot_z: 0.0, scale: 1.0, pivot_origin: false }
    }
}

impl ModelXf {
    pub fn is_identity(&self) -> bool {
        self.translate == [0.0; 3] && self.rot_z == 0.0 && self.scale == 1.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Viewpoint {
    pub name: String,
    pub cam: Camera,
    pub section: Section,
    pub hidden: Vec<ElemId>,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub redlines: Vec<crate::markup::Redline>,
    #[serde(default)]
    pub issue: crate::markup::Issue,
    #[serde(default)]
    pub comments: Vec<crate::markup::VpComment>,
    #[serde(default)]
    pub selected: Vec<ElemId>,
    /// masuratorile (cote) vizibile cand s-a salvat viewpoint-ul
    #[serde(default)]
    pub measures: Vec<Measure>,
}

#[derive(Serialize, Deserialize)]
pub struct ProjectFile {
    pub app: String,
    pub version: u32,
    pub models: Vec<String>,
    pub origin: Option<[f64; 3]>,
    pub camera: Camera,
    pub viewpoints: Vec<Viewpoint>,
    pub sets: Vec<SelSet>,
    pub clash_tests: Vec<ClashTest>,
    pub tasks: Vec<Task>,
    pub hidden: Vec<ElemId>,
    pub overrides: Vec<(ElemId, [u8; 4])>,
    pub section: Section,
    pub measures: Vec<Measure>,
    /// model paths relative to the project file
    #[serde(default)]
    pub models_rel: Vec<String>,
    #[serde(default)]
    pub model_xf: Vec<ModelXf>,
    #[serde(default)]
    pub elem_offsets: Vec<(ElemId, [f32; 3])>,
    #[serde(default)]
    pub animations: Vec<Animation>,
    #[serde(default)]
    pub render: RenderSettings,
    /// niveluri definite de utilizator (cota fata de ±0.00) si cota reala a lui ±0.00
    #[serde(default)]
    pub user_levels: Vec<UserLevel>,
    #[serde(default)]
    pub level_zero: f64,
    #[serde(default)]
    pub use_user_levels: bool,
    #[serde(default = "crate::app::yes")]
    pub show_grids: bool,
    #[serde(default = "crate::app::yes")]
    pub show_levels: bool,
    #[serde(default)]
    pub shared_dir: Option<String>,
    #[serde(default)]
    pub user_grids: Vec<UserGrid>,
    #[serde(default)]
    pub use_user_grids: bool,
    #[serde(default = "crate::app::grid_bubble_default")]
    pub grid_bubble: f32,
    #[serde(default = "crate::app::grid_ext_default")]
    pub grid_ext: f32,
    #[serde(default)]
    pub materials: Vec<Material>,
    #[serde(default)]
    pub mat_assign: Vec<MatAssign>,
    #[serde(default)]
    pub materials_on: Option<bool>,
    /// ViewCube "Home" view
    #[serde(default)]
    pub home: Option<Camera>,
    /// traseul de mers (puncte pe podea, metri)
    #[serde(default)]
    pub walk_path: Vec<[f32; 3]>,
    /// Appearance Profiler
    #[serde(default)]
    pub profile: Option<crate::profiler::Profile>,
    /// Quantification
    #[serde(default)]
    pub qto: Option<crate::qto::QtoSetup>,
}

/// Un nivel de grupare al arborelui de selectie: proprietatea (categorie, nume) dupa care se grupeaza.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeLevel {
    pub cat: String,
    pub prop: String,
}

impl TreeLevel {
    pub fn new(cat: &str, prop: &str) -> TreeLevel {
        TreeLevel { cat: cat.into(), prop: prop.into() }
    }
}

/// Arborele implicit (ca pana acum): Level > IFC Type > elemente.
pub fn default_tree_levels() -> Vec<TreeLevel> {
    vec![TreeLevel::new(ITEM_CAT, "Level"), TreeLevel::new(ITEM_CAT, "IFC Type")]
}

/// Fisier .bimtree: configuratia arborelui, refolosibila in alte proiecte.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TreeFile {
    pub app: String,
    pub kind: String,
    pub version: u32,
    pub levels: Vec<TreeLevel>,
}

pub const TREE_FILE_KIND: &str = "selection-tree";

pub struct TreeNode {
    pub label: String,
    /// toate elementele de sub acest nod
    pub ids: Vec<u32>,
    /// gol = ultimul nivel (sub el se listeaza elementele)
    pub children: Vec<TreeNode>,
}

/// arbore de selectie al unui model, grupat dupa nivelurile alese de utilizator
pub struct ModelTree {
    pub roots: Vec<TreeNode>,
}

impl ModelTree {
    pub fn build(m: &Model, levels: &[TreeLevel]) -> ModelTree {
        let all: Vec<u32> = (0..m.elements.len() as u32).collect();
        let roots = if levels.is_empty() { Vec::new() } else { group_tree(m, &all, levels) };
        ModelTree { roots }
    }
}

fn group_tree(m: &Model, ids: &[u32], levels: &[TreeLevel]) -> Vec<TreeNode> {
    let Some((lv, rest)) = levels.split_first() else { return Vec::new() };
    let none = format!("(no {})", lv.prop.to_lowercase());
    let mut groups: HashMap<String, Vec<u32>> = HashMap::new();
    for &i in ids {
        let e = &m.elements[i as usize];
        let v = elem_values(m, e, &lv.cat, &lv.prop).into_iter().map(str::trim).find(|v| !v.is_empty()).map(String::from).unwrap_or_else(|| none.clone());
        groups.entry(v).or_default().push(i);
    }
    let mut keys: Vec<String> = groups.keys().cloned().collect();
    // ordine naturala ("Level 2" < "Level 10"), grupul "(no ...)" la final
    keys.sort_by(|a, b| (*a == none).cmp(&(*b == none)).then_with(|| crate::profiler::natural_cmp(a, b)));
    keys.into_iter()
        .map(|k| {
            let ids = groups.remove(&k).unwrap_or_default();
            let children = group_tree(m, &ids, rest);
            TreeNode { label: k, ids, children }
        })
        .collect()
}

pub use crate::layout::{Dock, Panel};

pub struct ReportJob {
    /// 0 html, 1 xlsx, 2 bcf
    pub kind: u8,
    pub path: String,
    pub idx: usize,
    pub phase: u8,
    pub wait: u32,
    pub items: Vec<crate::markup::ReportItem>,
    pub saved_cam: Camera,
    pub saved_section: Section,
    pub saved_hidden: HashSet<ElemId>,
    pub saved_vp: Option<usize>,
}

pub fn png_rgb(png_data: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(png_data));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0u8; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    let px = &buf[..info.buffer_size()];
    let rgb: Vec<u8> = match info.color_type {
        png::ColorType::Rgb => px.to_vec(),
        png::ColorType::Rgba => px.chunks_exact(4).flat_map(|c| [c[0], c[1], c[2]]).collect(),
        _ => return None,
    };
    Some((rgb, info.width, info.height))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NavMode {
    Orbit,
    Walk,
    /// click stanga + trage = deplasare laterala (ca Pan din Navisworks)
    Pan,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Purpose {
    OpenModel { append: bool },
    OpenProject,
    SaveProject,
    ExportClashHtml,
    ExportClashCsv,
    ExportClashBcf,
    ExportIfc,
    SharedFolder,
    PublishBimd,
    ImportGridsCad,
    ExportReference,
    ExportClashMatrix,
    ExportGrids,
    ExportClashNavis,
    ImportBcf,
    ImportTasksCsv,
    ExportTasksCsv,
    ExportCache(usize),
    ExportSets,
    ImportSets,
    ExportVideo,
    RenderImage,
    ImportTextures,
    ExportVpHtml,
    ExportVpXlsx,
    ExportBcf,
    ExportNavisXml,
    ImportNavisXml,
    ExportNavisSets,
    ExportNavisClash,
    ExportMaterials,
    ImportMaterials,
    MaterialTexture,
    ExportProfile,
    ImportProfile,
    ExportCompare,
    ImportCompare,
    ExportCompareCsv,
    ExportTree,
    ImportTree,
    ExportQtoXlsx,
    ExportQtoCsv,
    ExportQtoTemplate,
    ImportQtoTemplate,
}

pub struct PathPrompt {
    pub purpose: Purpose,
    pub text: String,
}

enum LoadMsg {
    Progress(f32, String),
    Done(Result<(Arc<Model>, Option<String>), String>),
}

pub struct LoadJob {
    pub slot: usize,
    pub path: String,
    pub progress: f32,
    pub stage: String,
    rx: Receiver<LoadMsg>,
}

pub struct ClashJob {
    pub test: usize,
    pub prog: Arc<clash::Progress>,
    rx: Receiver<Vec<clash::Found>>,
    pub started: Instant,
}

pub struct RenderJob {
    pub anim: usize,
    pub frame: u32,
    pub frames: u32,
    pub fps: u32,
    pub ss: u32,
    pub w: u32,
    pub h: u32,
    pub tx: Option<Sender<Frame>>,
    pub done: Receiver<Result<(), String>>,
    pub path: std::path::PathBuf,
    pub waiting: bool,
    pub started: Instant,
    pub saved_sel: BTreeSet<ElemId>,
}

pub enum Action {
    Select(Vec<ElemId>, bool),
    Hide(Vec<ElemId>),
    Show(Vec<ElemId>),
    Isolate(Vec<ElemId>),
    ShowAll,
    Focus(Vec<ElemId>),
    FocusClash(usize, usize),
    FocusClashes(usize, Vec<usize>),
    /// scoate modelul (slotul) din proiect; fisierul de pe disc ramane neatins
    RemoveModel(usize),
}

pub struct App {
    pub models: Vec<Option<Arc<Model>>>,
    pub trees: Vec<Option<ModelTree>>,
    pub origin: Arc<Mutex<Option<DVec3>>>,
    pub renderer: Arc<Mutex<Renderer>>,
    pub cam: Camera,
    pub selected: BTreeSet<ElemId>,
    pub primary: Option<ElemId>,
    pub hidden: HashSet<ElemId>,
    pub overrides: HashMap<ElemId, [u8; 4]>,
    pub colors_dirty: bool,
    pub ribbon_tab: usize,
    pub layout_saved: crate::layout::UiLayout,
    pub tool: Tool,
    pub measures: Vec<Measure>,
    pub cur_measure: Vec<Vec3>,
    pub section: Section,
    pub viewpoints: Vec<Viewpoint>,
    pub sets: Vec<SelSet>,
    pub find_rules: Vec<SearchRule>,
    pub find_count: Option<usize>,
    pub clash_tests: Vec<ClashTest>,
    pub cur_test: usize,
    pub clash_job: Option<ClashJob>,
    pub cur_result: Option<usize>,
    pub clash_focus: Option<ClashFocus>,
    pub sel_results: BTreeSet<usize>,
    pub clash_others: Others,
    pub clash_highlight_all: bool,
    pub clash_auto_zoom: bool,
    pub open_groups: HashSet<String>,
    pub clash_subtab: u8,
    pub find_in_selection: bool,
    pub group_cell: f32,
    pub clash_filter: Option<ClashStatus>,
    pub tasks: Vec<Task>,
    pub cur_task: Option<usize>,
    pub sim_active: bool,
    pub sim_day: f32,
    pub sim_play: bool,
    pub sim_speed: f32,
    pub sim_cache: HashMap<ElemId, SimState>,
    pub sim_cache_day: i32,
    pub loads: Vec<LoadJob>,
    pub toasts: Vec<(String, Instant, bool)>,
    pub log: Vec<String>,
    pub project_path: Option<String>,
    pub prompt: Option<PathPrompt>,
    pub pending_project: Option<ProjectFile>,
    pub pivot: Option<Vec3>,
    pub last_frame: Instant,
    pub fps: f32,
    pub show_help: bool,
    pub show_about: bool,
    /// fereastra Lighting (View › Scene)
    pub show_light: bool,
    /// nori de puncte: densitatea afisata (0..1), marimea punctelor (px), fereastra de control
    pub cloud_density: f32,
    pub point_size: f32,
    pub show_cloud: bool,
    /// axe si niveluri pe model (calculate o data per model)
    pub grid_cache: std::cell::RefCell<HashMap<usize, (usize, Arc<GridData>)>>,
    pub user_levels: Vec<UserLevel>,
    pub level_zero: f64,
    pub use_user_levels: bool,
    pub show_grids: bool,
    pub show_levels: bool,
    pub show_levels_editor: bool,
    /// salvare automata + recuperare dupa o inchidere neasteptata
    pub autosave: Autosave,
    /// sectiune: fetele taiate pline (capace hasurate)
    pub section_caps: bool,
    /// folderul comun pentru issue-uri (retea / SharePoint); None = nepartajat
    pub shared_dir: Option<String>,
    pub shared: crate::shared::SharedState,
    /// proiect nou, gol (New): fara pagina de start, cu Save disponibil
    pub blank_project: bool,
    /// axe create in ManageBIM (coordonate reale, metri)
    pub user_grids: Vec<UserGrid>,
    pub use_user_grids: bool,
    /// marimea bulelor axelor (px) si prelungirea liniilor dincolo de capete (m)
    pub grid_bubble: f32,
    pub grid_ext: f32,
    pub show_grids_editor: bool,
    /// planul de sectiune desenat ca suprafata verde transparenta
    pub show_section_plane: bool,
    /// panoul deschis dintr-o zona auto-hide si ultima data cand mouse-ul era pe el
    pub flyout: Option<(crate::layout::Dock, crate::layout::Panel)>,
    pub flyout_seen: Instant,
    /// sectiunea pe nivel curenta (pentru PgUp / PgDn), marginile box-ului de nivel (m) si inaltimea taieturii de plan
    pub section_level: Option<usize>,
    pub level_below: f32,
    pub level_above: f32,
    pub plan_cut_h: f32,
    /// relocare in curs (mutare / rotire / aliniere pe fata)
    pub xform: Option<XformPick>,
    pub relocate_selection: bool,
    /// fereastra de selectie in curs (punctul de pornire, pe ecran)
    pub box_start: Option<[f32; 2]>,
    /// exportul IFC: doar selectia (true) sau tot ce e vizibil
    pub ifc_selection: bool,
    /// fata section box-ului trasa acum (0..5: -X +X -Y +Y -Z +Z)
    pub box_drag: Option<usize>,
    pub show_clash_matrix: bool,
    /// ultimul import de axe din DWG / DXF: fisierul, straturile cu linii, stratul folosit
    pub grid_import: Option<(String, Vec<(String, usize)>, String)>,
    /// generatorul de axe (originea si directia se pot alege cu clic in model)
    pub grid_gen: GridGen,
    /// alegere in 3D: 0 nimic, 1 originea, 2 directia primei axe
    pub grid_pick: u8,
    /// proiectul deschis dintr-un .bimd: Save il publica din nou acolo
    pub bimd_path: Option<String>,
    /// intrebarea "salvezi?" inainte de intoarcerea la pagina de start
    pub confirm_start: bool,
    /// pagina de start aratata peste proiectul deschis (ca Navisworks: sageata inapoi la proiect)
    pub show_start: bool,
    /// ce se face dupa intrebarea "salvezi?" (deschidere / proiect nou din pagina de start)
    pub start_pending: Option<StartAction>,
    /// pagina de start: sortare (0 ultima deschidere, 1 nume, 2 data modificarii, 3 tip), cautare, lista / grila
    pub start_sort: u8,
    pub start_search: String,
    pub start_list: bool,
    pub override_color: [u8; 4],
    pub min_dist_result: Option<(f32, Vec3)>,
    pub cache_types: Vec<String>,
    pub cache_props: Vec<String>,
    pub prop_index: PropIndex,
    pub find_models_off: HashSet<usize>,
    pub prop_checked: BTreeSet<(String, String, String)>,
    pub prop_filter: String,
    pub editing_set: Option<usize>,
    /// geometry as loaded (for Reset) + applied transforms
    pub base_models: Vec<Option<Arc<Model>>>,
    pub model_xf: Vec<ModelXf>,
    pub elem_offsets: HashMap<ElemId, [f32; 3]>,
    pub xf_draft: HashMap<usize, ModelXf>,
    pub move_delta: [f64; 3],
    pub move_target: Option<[f64; 3]>,
    pub animations: Vec<Animation>,
    pub cur_anim: usize,
    pub render: RenderSettings,
    pub anim_play: Option<f32>,
    pub anim_time: f32,
    pub anim_loop: bool,
    pub anim_sim_active: bool,
    pub render_job: Option<RenderJob>,
    pub still_path: Option<String>,
    pub pending_capture: Option<(u32, u32)>,
    pub ffmpeg: Option<bool>,
    pub orbit_secs: f32,
    pub key_step: f32,
    pub proj_sel: Option<usize>,
    pub materials: Vec<Material>,
    pub mat_assign: Vec<MatAssign>,
    pub materials_on: bool,
    pub mat_sel: usize,
    pub mat_cache: Option<HashMap<ElemId, u16>>,
    pub render_subtab: u8,
    pub show_tools: bool,
    pub sets_sig: u64,
    pub tex_layers: Vec<(String, Vec<u8>)>,
    pub tex_failed: HashSet<String>,
    pub pending_csv: Vec<String>,
    pub settings: crate::theme::Settings,
    pub theme_dirty: bool,
    pub last_mode: (bool, bool),
    pub clash_queue: Vec<usize>,
    /// camera before the first clash was focused - restored when leaving Clash Detective
    pub cam_before_clash: Option<Camera>,
    pub home: Option<Camera>,
    pub active_vp: Option<usize>,
    pub vp_sel: usize,
    pub red_color: [u8; 3],
    pub red_width: f32,
    pub red_draft: Option<crate::markup::Redline>,
    pub red_text: Option<([f32; 2], String)>,
    pub report_job: Option<ReportJob>,
    pub vp_rect: [f32; 4],
    pub cam_anim: Option<(Camera, Camera, f32)>,
    pub fall_speed: f32,
    pub nav: NavMode,
    pub walk_drag: Option<[f32; 2]>,
    pub walk_saved: f32,
    /// viteza tinta in acest cadru (local: x dreapta, y inainte, z sus), in multipli ai vitezei de mers
    pub walk_target: Vec3,
    /// rotirea tinta in acest cadru (rad/s)
    pub walk_turn_target: f32,
    /// viteza / rotirea curente, netezite (accelerare si franare lina)
    pub walk_vel: Vec3,
    pub walk_turn: f32,
    /// traseul de mers desenat (puncte pe podea)
    pub walk_path: Vec<Vec3>,
    /// mersul automat pe traseu, cand ruleaza
    pub path_run: Option<walk::PathRun>,
    /// fisierele din Inbox deja deschise in aceasta sesiune (Send to ManageBIM)
    pub inbox_seen: HashSet<String>,
    /// section box-ul urmareste selectia (butonul Follow)
    pub section_follow: bool,
    pub section_sel_sig: u64,
    /// semnatura ultimei scrieri a Outbox\issues.txt (add-in-urile Revit / Navisworks)
    pub outbox_sig: u64,
    /// desenare simplificata: camera din cadrul trecut, momentul ultimei miscari, elementele cu transparenta
    pub lod_cam: Camera,
    pub lod_moved: Instant,
    pub alpha_cache: HashMap<usize, (usize, Vec<bool>)>,
    /// Refresh: data modificarii fisierului sursa la momentul incarcarii (per slot)
    pub file_stamps: HashMap<usize, std::time::SystemTime>,
    /// Refresh: modelul vechi, pastrat pana termina reincarcarea (pt. remaparea selectiei pe GUID)
    pub reloading: HashMap<usize, Arc<Model>>,
    /// Refresh: sloturile ale caror fisiere s-au modificat pe disc de la incarcare
    pub changed_files: BTreeSet<usize>,
    pub last_file_check: Instant,
    /// aspectul trimis ultima data la GPU, per slot (actualizare incrementala a culorilor)
    pub looks: HashMap<usize, Vec<Look>>,
    /// Appearance Profiler: reguli + harta element -> regula (calculata la nevoie)
    pub profile: crate::profiler::Profile,
    pub profile_map: Option<HashMap<ElemId, u16>>,
    /// Compare: rezultat + afisare in 3D + modelele alese (A = vechi, B = nou) + toleranta (mm)
    pub compare: Option<crate::compare::CompareResult>,
    pub compare_on: bool,
    pub compare_pick: (Option<usize>, Option<usize>),
    pub compare_tol: f32,
    /// editorul de grupare al arborelui de selectie e deschis
    pub tree_edit: bool,
    /// Quantification: configuratia (salvata in proiect / sablon .bimqto) si ultimul rezultat
    pub qto: crate::qto::QtoSetup,
    pub qto_result: Option<crate::qto::QtoResult>,
    /// semnatura starii pentru care s-a calculat rezultatul (setup + selectie / ascunse)
    pub qto_sig: u64,
}

impl App {
    pub fn new(files: Vec<String>) -> App {
        let mut app = App {
            models: Vec::new(),
            trees: Vec::new(),
            origin: Arc::new(Mutex::new(None)),
            renderer: Arc::new(Mutex::new(Renderer::new())),
            cam: Camera::default(),
            selected: BTreeSet::new(),
            primary: None,
            hidden: HashSet::new(),
            overrides: HashMap::new(),
            colors_dirty: true,
            ribbon_tab: 0,
            layout_saved: Default::default(),
            tool: Tool::Select,
            measures: Vec::new(),
            cur_measure: Vec::new(),
            section: Section::default(),
            viewpoints: Vec::new(),
            sets: Vec::new(),
            find_rules: vec![SearchRule::new(ITEM_CAT, "IFC Type", SearchOp::Equals, "", false)],
            find_count: None,
            clash_tests: vec![ClashTest::new("Test 1".into())],
            cur_test: 0,
            clash_job: None,
            cur_result: None,
            clash_focus: None,
            sel_results: BTreeSet::new(),
            clash_others: Others::Dim,
            clash_highlight_all: false,
            clash_auto_zoom: true,
            open_groups: HashSet::new(),
            clash_subtab: 0,
            find_in_selection: false,
            group_cell: 5.0,
            clash_filter: None,
            tasks: Vec::new(),
            cur_task: None,
            sim_active: false,
            sim_day: 0.0,
            sim_play: false,
            sim_speed: 7.0,
            sim_cache: HashMap::new(),
            sim_cache_day: i32::MIN,
            loads: Vec::new(),
            toasts: Vec::new(),
            log: Vec::new(),
            project_path: None,
            prompt: None,
            pending_project: None,
            pivot: None,
            last_frame: Instant::now(),
            fps: 0.0,
            show_help: false,
            show_about: false,
            show_light: false,
            cloud_density: 0.5,
            point_size: 2.0,
            show_cloud: false,
            grid_cache: Default::default(),
            user_levels: Vec::new(),
            level_zero: 0.0,
            use_user_levels: false,
            show_grids: true,
            show_levels: true,
            show_levels_editor: false,
            autosave: Autosave::default(),
            section_caps: true,
            shared_dir: None,
            shared: Default::default(),
            blank_project: false,
            user_grids: Vec::new(),
            use_user_grids: false,
            grid_bubble: 11.0,
            grid_ext: 2.0,
            show_grids_editor: false,
            show_section_plane: true,
            flyout: None,
            flyout_seen: Instant::now(),
            section_level: None,
            level_below: 0.0,
            level_above: 0.0,
            plan_cut_h: 1.2,
            xform: None,
            relocate_selection: false,
            box_start: None,
            ifc_selection: false,
            box_drag: None,
            show_clash_matrix: false,
            grid_import: None,
            grid_gen: GridGen::default(),
            grid_pick: 0,
            bimd_path: None,
            confirm_start: false,
            show_start: false,
            start_pending: None,
            start_sort: 0,
            start_search: String::new(),
            start_list: false,
            override_color: [230, 120, 40, 255],
            min_dist_result: None,
            cache_types: Vec::new(),
            cache_props: Vec::new(),
            prop_index: PropIndex::default(),
            find_models_off: HashSet::new(),
            prop_checked: BTreeSet::new(),
            prop_filter: String::new(),
            editing_set: None,
            base_models: Vec::new(),
            model_xf: Vec::new(),
            elem_offsets: HashMap::new(),
            xf_draft: HashMap::new(),
            move_delta: [0.0; 3],
            move_target: None,
            animations: Vec::new(),
            cur_anim: 0,
            render: RenderSettings::default(),
            anim_play: None,
            anim_time: 0.0,
            anim_loop: false,
            anim_sim_active: false,
            render_job: None,
            still_path: None,
            pending_capture: None,
            ffmpeg: None,
            orbit_secs: 20.0,
            key_step: 3.0,
            proj_sel: None,
            materials: crate::materials::library(),
            mat_assign: Vec::new(),
            materials_on: true,
            mat_sel: 0,
            mat_cache: None,
            render_subtab: 0,
            show_tools: false,
            sets_sig: 0,
            tex_layers: Vec::new(),
            tex_failed: HashSet::new(),
            pending_csv: Vec::new(),
            settings: crate::theme::Settings::load(),
            theme_dirty: true,
            last_mode: (false, false),
            clash_queue: Vec::new(),
            cam_before_clash: None,
            home: None,
            active_vp: None,
            vp_sel: 0,
            red_color: [230, 40, 40],
            red_width: 3.0,
            red_draft: None,
            red_text: None,
            report_job: None,
            vp_rect: [0.0; 4],
            cam_anim: None,
            fall_speed: 0.0,
            nav: NavMode::Orbit,
            walk_drag: None,
            walk_saved: 3.0,
            walk_target: Vec3::ZERO,
            walk_turn_target: 0.0,
            walk_vel: Vec3::ZERO,
            walk_turn: 0.0,
            walk_path: Vec::new(),
            path_run: None,
            inbox_seen: HashSet::new(),
            section_follow: false,
            section_sel_sig: 0,
            outbox_sig: 0,
            lod_cam: Camera::default(),
            lod_moved: Instant::now(),
            alpha_cache: HashMap::new(),
            file_stamps: HashMap::new(),
            reloading: HashMap::new(),
            changed_files: BTreeSet::new(),
            last_file_check: Instant::now(),
            looks: HashMap::new(),
            profile: Default::default(),
            profile_map: None,
            compare: None,
            compare_on: false,
            compare_pick: (None, None),
            compare_tol: 1.0,
            tree_edit: false,
            qto: Default::default(),
            qto_result: None,
            qto_sig: 0,
        };
        // convertoare externe: detectare la prima pornire (sau cand lipsesc)
        let t = &app.settings.tools;
        if t.oda.is_empty() || t.navisworks.is_empty() {
            let d = crate::formats::detect_tools();
            let t = &mut app.settings.tools;
            for (cur, new) in [(&mut t.oda, d.oda), (&mut t.navisworks, d.navisworks)] {
                if cur.is_empty() || !std::path::Path::new(cur.as_str()).exists() {
                    *cur = new;
                }
            }
            app.settings.save();
        }
        crate::formats::set_tools(app.settings.tools.clone());
        for f in files {
            app.open_path(&f, true);
        }
        app
    }

    // ------------------------------------------------------------------ mesaje
    pub fn toast(&mut self, msg: impl Into<String>) {
        let m = msg.into();
        self.log.push(m.clone());
        self.toasts.push((m, Instant::now(), false));
    }
    pub fn error(&mut self, msg: impl Into<String>) {
        let m = msg.into();
        self.log.push(format!("ERROR: {m}"));
        self.toasts.push((m, Instant::now(), true));
    }

    // ------------------------------------------------------------------ acces model
    pub fn elem(&self, id: ElemId) -> Option<(&Arc<Model>, &Element)> {
        let m = self.models.get(id.m as usize)?.as_ref()?;
        Some((m, m.elements.get(id.e as usize)?))
    }
    pub fn all_ids(&self) -> Vec<ElemId> {
        let mut v = Vec::new();
        for (mi, m) in self.models.iter().enumerate() {
            if let Some(m) = m {
                v.extend((0..m.elements.len() as u32).map(|e| ElemId { m: mi as u32, e }));
            }
        }
        v
    }
    pub fn loaded_models(&self) -> impl Iterator<Item = (usize, &Arc<Model>)> {
        self.models.iter().enumerate().filter_map(|(i, m)| m.as_ref().map(|m| (i, m)))
    }
    pub fn scene_aabb(&self) -> Aabb {
        let mut bb = Aabb::EMPTY;
        for (_, m) in self.loaded_models() {
            if m.aabb.is_valid() {
                bb.union(&m.aabb);
            }
        }
        bb
    }
    pub fn ids_aabb(&self, ids: &[ElemId]) -> Aabb {
        let mut bb = Aabb::EMPTY;
        for id in ids {
            if let Some((_, e)) = self.elem(*id) {
                if e.aabb.is_valid() {
                    bb.union(&e.aabb);
                }
            }
        }
        bb
    }
    /// Clash results are only shown while the Clash Detective panel is open.
    pub fn clash_view(&self) -> bool {
        self.settings.layout.shown(Panel::Clash)
    }
    /// The 4D simulation is only shown while the TimeLiner panel is open.
    pub fn sim_view(&self) -> bool {
        (self.sim_active && self.settings.layout.shown(Panel::TimeLiner)) || self.anim_sim_active
    }
    pub fn clash_focus_active(&self) -> Option<&ClashFocus> {
        if self.clash_view() {
            self.clash_focus.as_ref()
        } else {
            None
        }
    }

    /// element -> role (1 = A, 2 = B of focused clashes, 3 / 4 = A / B of the other clashes)
    pub fn clash_roles(&self) -> HashMap<ElemId, u8> {
        let mut roles = HashMap::new();
        if !self.clash_view() {
            return roles;
        }
        if self.clash_highlight_all {
            if let Some(t) = self.clash_tests.get(self.cur_test) {
                for r in t.results.iter().filter(|r| r.status != ClashStatus::Resolved && self.clash_filter.map_or(true, |f| r.status == f)) {
                    roles.entry(r.a).or_insert(3);
                    roles.entry(r.b).or_insert(4);
                }
            }
        }
        if let Some(f) = &self.clash_focus {
            for a in &f.a {
                roles.insert(*a, 1);
            }
            for b in &f.b {
                roles.insert(*b, 2);
            }
        }
        roles
    }

    /// Called every frame: entering / leaving a module restores the full model.
    pub fn update_mode(&mut self) {
        let mode = (self.clash_view(), self.sim_view());
        if mode == self.last_mode {
            return;
        }
        let (was_clash, _) = self.last_mode;
        self.last_mode = mode;
        self.colors_dirty = true;
        if was_clash && !mode.0 {
            if let Some(c) = self.cam_before_clash.take() {
                self.cam = c;
            }
        }
        if let Some(f) = &self.clash_focus {
            let pair: BTreeSet<ElemId> = f.a.iter().chain(f.b.iter()).copied().collect();
            let first = f.a.first().copied();
            if was_clash && !mode.0 && self.selected == pair {
                self.selected.clear();
                self.primary = None;
            } else if !was_clash && mode.0 && self.selected.is_empty() {
                self.selected = pair;
                self.primary = first;
            }
        }
    }

    pub fn is_visible(&self, id: ElemId) -> bool {
        if self.hidden.contains(&id) {
            return false;
        }
        if self.sim_view() {
            if let Some(SimState::Hidden) = self.sim_cache.get(&id) {
                return false;
            }
        }
        true
    }
    pub fn types(&self) -> Vec<String> {
        let mut s = BTreeSet::new();
        for (_, m) in self.loaded_models() {
            for e in &m.elements {
                s.insert(e.ifc_type.clone());
            }
        }
        s.into_iter().collect()
    }
    /// Builds the category -> property -> value index over ALL elements of ALL models.
    pub fn build_prop_index(&self) -> PropIndex {
        let mut map: HashMap<(String, String), HashMap<String, u32>> = HashMap::new();
        let mut order_cats: Vec<String> = Vec::new();
        let mut seen_cat: HashSet<String> = HashSet::new();
        let mut add = |c: &str, n: &str, v: &str, map: &mut HashMap<(String, String), HashMap<String, u32>>| {
            if v.is_empty() {
                return;
            }
            if !seen_cat.contains(c) {
                seen_cat.insert(c.to_string());
                order_cats.push(c.to_string());
            }
            let e = map.entry((c.to_string(), n.to_string())).or_default();
            if let Some(x) = e.get_mut(v) {
                *x += 1;
            } else {
                e.insert(v.to_string(), 1);
            }
        };
        for (_, m) in self.loaded_models() {
            for e in &m.elements {
                add(ITEM_CAT, "Name", &e.name, &mut map);
                add(ITEM_CAT, "IFC Type", &e.ifc_type, &mut map);
                add(ITEM_CAT, "Level", &e.storey, &mut map);
                add(ITEM_CAT, "Model", &m.name, &mut map);
                add(ITEM_CAT, "GlobalId", &e.guid, &mut map);
                for p in &e.props {
                    add(&p.group, &p.name, &p.value, &mut map);
                }
            }
        }
        let mut cats: BTreeMap<String, Vec<PropValues>> = BTreeMap::new();
        for ((c, n), vals) in map {
            let elements = vals.values().sum();
            let distinct = vals.len();
            let mut values: Vec<(String, u32)> = if distinct <= MAX_LISTED_VALUES { vals.into_iter().collect() } else { Vec::new() };
            values.sort_by(|a, b| natural_cmp(&a.0, &b.0));
            cats.entry(c).or_default().push(PropValues { name: n, values, distinct, elements });
        }
        let rank = |c: &str| match c {
            ITEM_CAT => 0,
            "Element" => 1,
            _ => 2,
        };
        let mut out: Vec<(String, Vec<PropValues>)> = cats.into_iter().collect();
        for (c, ps) in out.iter_mut() {
            if c == ITEM_CAT {
                ps.sort_by_key(|p| ITEM_PROPS.iter().position(|x| *x == p.name).unwrap_or(99));
            } else {
                ps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            }
        }
        out.sort_by(|a, b| rank(&a.0).cmp(&rank(&b.0)).then(a.0.to_lowercase().cmp(&b.0.to_lowercase())));
        let _ = order_cats;
        PropIndex { cats: out }
    }

    /// Find with Navisworks-style scope: only the ticked models, optionally inside the selection.
    pub fn find_scoped(&self, rules: &[SearchRule], in_selection: bool) -> Vec<ElemId> {
        let cand: Vec<ElemId> = if in_selection { self.selected.iter().copied().collect() } else { self.all_ids() };
        cand.into_iter().filter(|id| !self.find_models_off.contains(&(id.m as usize)) && self.matches(*id, rules)).collect()
    }

    /// Rules for the values ticked in the Properties tree: values of one property are OR-ed.
    pub fn rules_from_checked(&self) -> Vec<SearchRule> {
        let mut v = Vec::new();
        for (i, (c, n, val)) in self.prop_checked.iter().enumerate() {
            v.push(SearchRule::new(c, n, SearchOp::Equals, val, i > 0));
        }
        v
    }

    pub fn prop_names(&self) -> Vec<String> {
        let mut s = BTreeSet::new();
        for (_, m) in self.loaded_models() {
            for e in m.elements.iter().take(20000) {
                for p in &e.props {
                    if p.group != "Element" {
                        s.insert(p.name.clone());
                    }
                }
            }
        }
        s.into_iter().collect()
    }

    // ------------------------------------------------------------------ cautare
    /// Rules are AND-ed; a rule flagged `or` starts a new block, blocks are OR-ed.
    pub fn matches(&self, id: ElemId, rules: &[SearchRule]) -> bool {
        if rules.is_empty() {
            return false;
        }
        let mut blocks: Vec<Vec<&SearchRule>> = vec![Vec::new()];
        for (i, r) in rules.iter().enumerate() {
            if r.or && i > 0 {
                blocks.push(Vec::new());
            }
            blocks.last_mut().unwrap().push(r);
        }
        blocks.iter().filter(|b| !b.is_empty()).any(|b| b.iter().all(|r| self.rule_match(id, r)))
    }

    pub fn rule_match(&self, id: ElemId, r: &SearchRule) -> bool {
        let Some((m, e)) = self.elem(id) else { return false };
        let val = r.value.trim().to_lowercase();
        let num = leading_number(&r.value);
        // valoarea(ile) campului
        let item_val = |f: &str| -> Option<&str> {
            match f {
                "Name" => Some(e.name.as_str()),
                "IFC Type" => Some(e.ifc_type.as_str()),
                "Level" => Some(e.storey.as_str()),
                "Model" => Some(m.name.as_str()),
                "GlobalId" => Some(e.guid.as_str()),
                _ => None,
            }
        };
        let vals: Vec<&str> = if !r.group.is_empty() {
            if r.group == ITEM_CAT {
                if r.field == "(any)" {
                    ITEM_PROPS.iter().filter_map(|f| item_val(f)).collect()
                } else {
                    item_val(&r.field).into_iter().collect()
                }
            } else {
                e.props
                    .iter()
                    .filter(|p| p.group.eq_ignore_ascii_case(&r.group) && (r.field == "(any)" || p.name.eq_ignore_ascii_case(&r.field)))
                    .map(|p| p.value.as_str())
                    .collect()
            }
        } else {
            match r.field.as_str() {
            "Any" => {
                let mut v: Vec<&str> = vec![e.name.as_str(), e.ifc_type.as_str(), e.storey.as_str(), e.guid.as_str()];
                v.extend(e.props.iter().map(|p| p.value.as_str()));
                v
            }
            "Name" => vec![e.name.as_str()],
            "IFC Type" => vec![e.ifc_type.as_str()],
            "Level" => vec![e.storey.as_str()],
            "Model" => vec![m.name.as_str()],
            "GlobalId" => vec![e.guid.as_str()],
            f => e
                .props
                .iter()
                .filter(|p| p.name.eq_ignore_ascii_case(f) || format!("{} / {}", p.group, p.name).eq_ignore_ascii_case(f))
                .map(|p| p.value.as_str())
                .collect(),
            }
        };
        let present: Vec<&str> = vals.into_iter().filter(|v| !v.is_empty()).collect();
        let lc = |s: &str| s.to_lowercase();
        match r.op {
            SearchOp::Defined => !present.is_empty(),
            SearchOp::Undefined => present.is_empty(),
            SearchOp::Equals => present.iter().any(|v| lc(v) == val),
            SearchOp::NotEquals => !present.iter().any(|v| lc(v) == val),
            SearchOp::Contains => present.iter().any(|v| lc(v).contains(&val)),
            SearchOp::NotContains => !present.iter().any(|v| lc(v).contains(&val)),
            SearchOp::Greater => num.map_or(false, |n| present.iter().any(|v| leading_number(v).map_or(false, |x| x > n))),
            SearchOp::Less => num.map_or(false, |n| present.iter().any(|v| leading_number(v).map_or(false, |x| x < n))),
        }
    }

    pub fn find(&self, rules: &[SearchRule]) -> Vec<ElemId> {
        self.all_ids().into_iter().filter(|id| self.matches(*id, rules)).collect()
    }

    pub fn resolve_set(&self, s: &SelSet) -> Vec<ElemId> {
        match &s.kind {
            SetKind::Explicit(v) => v.iter().copied().filter(|id| self.elem(*id).is_some()).collect(),
            SetKind::Search(r) => self.find(r),
        }
    }

    pub fn resolve_selector(&self, s: &Selector) -> Vec<ElemId> {
        match s.kind {
            SelKind::All => self.all_ids(),
            SelKind::Model => self
                .loaded_models()
                .filter(|(_, m)| m.name == s.value)
                .flat_map(|(i, m)| (0..m.elements.len() as u32).map(move |e| ElemId { m: i as u32, e }))
                .collect(),
            SelKind::Set => self.sets.iter().find(|x| x.name == s.value).map(|x| self.resolve_set(x)).unwrap_or_default(),
            SelKind::Type => self.all_ids().into_iter().filter(|id| self.elem(*id).map_or(false, |(_, e)| e.ifc_type == s.value)).collect(),
        }
    }

    /// Regula TimeLiner: "set:Nume" sau "camp=valoare; camp=valoare" sau text liber
    pub fn rule_elems(&self, rule: &str) -> Vec<ElemId> {
        let rule = rule.trim();
        if rule.is_empty() {
            return Vec::new();
        }
        if let Some(n) = rule.strip_prefix("set:") {
            return self.sets.iter().find(|s| s.name.eq_ignore_ascii_case(n.trim())).map(|s| self.resolve_set(s)).unwrap_or_default();
        }
        let mut rules = Vec::new();
        for part in rule.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some((f, v)) = part.split_once('=') {
                let f = match f.trim().to_lowercase().as_str() {
                    "name" | "nume" => "Name".to_string(),
                    "type" | "tip" | "ifc type" | "tip ifc" => "IFC Type".to_string(),
                    "level" | "nivel" | "storey" => "Level".to_string(),
                    "model" => "Model".to_string(),
                    o => o.to_string(),
                };
                rules.push(SearchRule { field: f, op: SearchOp::Equals, value: v.trim().to_string(), or: false, group: String::new() });
            } else {
                rules.push(SearchRule { field: "Any".into(), op: SearchOp::Contains, value: part.to_string(), or: false, group: String::new() });
            }
        }
        self.find(&rules)
    }

    // ------------------------------------------------------------------ actiuni
    pub fn apply(&mut self, a: Action) {
        match a {
            Action::Select(ids, add) => {
                if !add {
                    self.selected.clear();
                }
                for id in &ids {
                    if add && ids.len() == 1 && self.selected.contains(id) {
                        self.selected.remove(id);
                    } else {
                        self.selected.insert(*id);
                    }
                }
                self.primary = ids.last().copied().filter(|i| self.selected.contains(i));
                self.colors_dirty = true;
            }
            Action::Hide(ids) => {
                for id in ids {
                    self.hidden.insert(id);
                    self.selected.remove(&id);
                }
                self.colors_dirty = true;
            }
            Action::Show(ids) => {
                for id in ids {
                    self.hidden.remove(&id);
                }
                self.colors_dirty = true;
            }
            Action::Isolate(ids) => {
                let keep: HashSet<ElemId> = ids.into_iter().collect();
                self.hidden = self.all_ids().into_iter().filter(|i| !keep.contains(i)).collect();
                for k in &keep {
                    self.hidden.remove(k);
                }
                self.colors_dirty = true;
            }
            Action::ShowAll => {
                self.hidden.clear();
                self.colors_dirty = true;
            }
            Action::Focus(ids) => {
                let bb = if ids.is_empty() { self.scene_aabb() } else { self.ids_aabb(&ids) };
                let mut t = self.cam;
                t.fit(&bb);
                self.fly_to(t);
            }
            Action::RemoveModel(slot) => {
                let name = self.models.get(slot).and_then(|m| m.as_ref()).map(|m| m.name.clone()).unwrap_or_default();
                self.remove_model(slot);
                self.refresh_caches();
                self.toast(format!("Removed from the project: {name} (the file is not deleted)."));
            }
            Action::FocusClash(t, r) => self.apply(Action::FocusClashes(t, vec![r])),
            Action::FocusClashes(t, rs) => {
                let Some(test) = self.clash_tests.get(t) else { return };
                let res: Vec<ClashResult> = rs.iter().filter_map(|r| test.results.get(*r).cloned()).collect();
                if res.is_empty() {
                    return;
                }
                if self.cam_before_clash.is_none() {
                    self.cam_before_clash = Some(self.cam);
                }
                self.cur_result = rs.last().copied();
                let mut f = ClashFocus::default();
                self.selected.clear();
                for r in &res {
                    f.a.push(r.a);
                    f.b.push(r.b);
                    f.points.push(Vec3::from(r.point));
                    self.selected.insert(r.a);
                    self.selected.insert(r.b);
                    self.hidden.remove(&r.a);
                    self.hidden.remove(&r.b);
                }
                self.primary = Some(res[0].a);
                if self.clash_auto_zoom {
                    let bb = if res.len() == 1 {
                        // apropie camera de punctul de clash (elementele mari ar indeparta prea mult camera)
                        let bb = self.ids_aabb(&[res[0].a, res[0].b]);
                        let p = f.points[0];
                        let r0 = (bb.size().length() * 0.5).min(4.0).max(0.6);
                        Aabb { min: p - Vec3::splat(r0), max: p + Vec3::splat(r0) }
                    } else {
                        let mut bb = Aabb::EMPTY;
                        for p in &f.points {
                            bb.grow(*p - Vec3::splat(0.8));
                            bb.grow(*p + Vec3::splat(0.8));
                        }
                        bb
                    };
                    self.cam.fit(&bb);
                }
                self.clash_focus = Some(f);
                self.colors_dirty = true;
            }
        }
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.primary = None;
        self.clash_focus = None;
        self.colors_dirty = true;
    }

    // ------------------------------------------------------------------ incarcare
    pub fn open_path(&mut self, path: &str, append: bool) {
        let lower = path.to_lowercase();
        if lower.ends_with(".bimr") {
            self.load_project(path);
            return;
        }
        if lower.ends_with(".bimd") {
            self.open_bimd(path);
            return;
        }
        if lower.ends_with(".csv") {
            // graficul se importa dupa ce modelele s-au incarcat (pentru reguli)
            self.pending_csv.push(path.to_string());
            self.open_panel(Panel::TimeLiner);
            return;
        }
        if !append {
            self.reset();
        }
        let slot = self.models.len();
        self.push_slot();
        self.settings.add_recent(path);
        self.start_load(slot, path.to_string());
    }

    fn start_load(&mut self, slot: usize, path: String) {
        // data fisierului la momentul citirii -> Refresh stie daca s-a modificat ulterior
        match file_mtime(&path) {
            Some(t) => {
                self.file_stamps.insert(slot, t);
            }
            None => {
                self.file_stamps.remove(&slot);
            }
        }
        self.changed_files.remove(&slot);
        let (tx, rx): (Sender<LoadMsg>, Receiver<LoadMsg>) = channel();
        let origin = self.origin.clone();
        let p2 = path.clone();
        let (use_cache, auto_cache) = (self.settings.use_cache, self.settings.auto_cache);
        std::thread::spawn(move || {
            let txp = tx.clone();
            let prog = move |f: f32, s: &str| {
                let _ = txp.send(LoadMsg::Progress(f, s.to_string()));
            };
            let lower = p2.to_lowercase();
            // cache .bimc pentru tot ce nu e deja .bimc (conversiile DWG / STEP / Navisworks sunt lente)
            // norii de puncte nu au cache .bimc (punctele nu incap in formatul cache)
            let is_e57 = lower.ends_with(".e57") || lower.ends_with(".las");
            let is_ifc = !lower.ends_with(".bimc") && !is_e57;
            let mut note = None;
            let mut loaded = None;
            let mut cloud_raw: Option<crate::e57::RawCloud> = None;
            // like Navisworks NWC: reuse an up-to-date cache next to the IFC
            if use_cache && is_ifc {
                if let Some(c) = crate::bimc::fresh_cache(&p2) {
                    prog(0.1, "Reading cache (.bimc)");
                    match crate::bimc::read(&c) {
                        Ok(mut raw) => {
                            raw.info.push(("Loaded from".into(), crate::file_name(&c)));
                            note = Some(format!("{}: opened from cache {}", crate::file_name(&p2), crate::file_name(&c)));
                            loaded = Some(Ok(raw));
                        }
                        Err(e) => note = Some(format!("Cache ignored ({e}), reading IFC")),
                    }
                }
            }
            // incarcarea ruleaza protejat: o eroare interna devine mesaj de eroare, nu inchide aplicatia
            let loaded = loaded.unwrap_or_else(|| {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if is_e57 {
                        if lower.ends_with(".las") { crate::e57::read_las(&p2, &prog) } else { crate::e57::read(&p2, &prog) }.map(|(raw, c)| {
                            cloud_raw = Some(c);
                            raw
                        })
                    } else {
                        crate::load_any(&p2, &prog)
                    }
                }))
                .unwrap_or_else(|e| Err(format!("Internal error while reading the file: {}", panic_text(&e))))
            });
            let res = loaded.map(|raw| {
                if auto_cache && is_ifc && note.is_none() {
                    prog(0.95, "Writing cache (.bimc)");
                    let cp = crate::bimc::cache_path(&p2);
                    note = Some(match crate::bimc::write(&raw, &cp) {
                        Ok(n) => {
                            let src = std::fs::metadata(&p2).map(|m| m.len()).unwrap_or(0);
                            format!("Cache written: {} ({:.2} MB, {:.1}x smaller than the source)", crate::file_name(&cp), n as f64 / 1e6, src as f64 / n.max(1) as f64)
                        }
                        Err(e) => format!("Could not write cache: {e}"),
                    });
                }
                prog(0.99, "Building spatial index");
                let o = {
                    let mut g = origin.lock().unwrap_or_else(|p| p.into_inner());
                    *g.get_or_insert_with(|| match &cloud_raw {
                        Some(c) => DVec3::new(c.base.x.round(), c.base.y.round(), if c.base.z.abs() > 5000.0 { c.base.z.round() } else { 0.0 }),
                        None => raw.center(),
                    })
                };
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut m = crate::build_model(raw, &p2, o);
                    if let Some(c) = cloud_raw.take() {
                        m.attach_cloud(c, o);
                    }
                    Arc::new(m)
                }))
                    .map(|m| (m, note))
                    .map_err(|e| format!("Internal error while building the model: {}", panic_text(&e)))
            }).and_then(|r| r);
            let _ = tx.send(LoadMsg::Done(res));
        });
        self.loads.push(LoadJob { slot, path, progress: 0.0, stage: "Starting".into(), rx });
    }

    pub fn push_slot(&mut self) {
        self.models.push(None);
        self.trees.push(None);
        self.base_models.push(None);
        self.model_xf.push(ModelXf::default());
    }

    pub fn origin_f64(&self) -> DVec3 {
        self.origin.lock().unwrap().unwrap_or(DVec3::ZERO)
    }

    /// local (render) coordinates -> real world coordinates
    pub fn world(&self, p: Vec3) -> DVec3 {
        p.as_dvec3() + self.origin_f64()
    }

    pub fn xf_matrix(&self, base: &Model, xf: &ModelXf) -> Mat4 {
        let pivot = if xf.pivot_origin { (-self.origin_f64()).as_vec3() } else { base.aabb.center() };
        let t = Vec3::new(xf.translate[0] as f32, xf.translate[1] as f32, xf.translate[2] as f32);
        Mat4::from_translation(pivot + t) * Mat4::from_rotation_z((xf.rot_z as f32).to_radians()) * Mat4::from_scale(Vec3::splat(xf.scale.max(1e-6) as f32)) * Mat4::from_translation(-pivot)
    }

    /// Rebuilds a model from its loaded geometry + model transform + element offsets.
    pub fn apply_transform(&mut self, slot: usize) {
        let Some(base) = self.base_models.get(slot).cloned().flatten() else { return };
        let xf = self.model_xf.get(slot).copied().unwrap_or_default();
        let offs: HashMap<u32, Vec3> = self.elem_offsets.iter().filter(|(k, _)| k.m as usize == slot).map(|(k, v)| (k.e, Vec3::from(*v))).collect();
        let m = if xf.is_identity() && offs.is_empty() { base } else { Arc::new(base.transformed(self.xf_matrix(&base, &xf), &offs)) };
        let (pos, nrm) = render::mesh_data(&m);
        let col = vec![200u8; pos.len() / 3 * 4];
        {
            let mut r = self.renderer.lock().unwrap();
            r.pending.push(Upload::Mesh { slot: slot as u32, pos, nrm, col });
            r.pending.push(Upload::Points { slot: slot as u32, cloud: m.cloud.clone() });
        }
        // bufferul nou de pe GPU are culori implicite -> la urmatorul rebuild_colors trimitem tot
        self.looks.remove(&slot);
        if slot < self.models.len() {
            self.models[slot] = Some(m);
        }
        self.min_dist_result = None;
        self.colors_dirty = true;
    }

    pub fn set_model_xf(&mut self, slot: usize, xf: ModelXf) {
        while self.model_xf.len() <= slot {
            self.model_xf.push(ModelXf::default());
        }
        self.model_xf[slot] = xf;
        self.apply_transform(slot);
    }

    pub fn move_selected(&mut self, d: Vec3) {
        let mut slots = BTreeSet::new();
        for id in self.selected.clone() {
            let o = self.elem_offsets.entry(id).or_insert([0.0; 3]);
            o[0] += d.x;
            o[1] += d.y;
            o[2] += d.z;
            if o.iter().all(|v| v.abs() < 1e-7) {
                self.elem_offsets.remove(&id);
            }
            slots.insert(id.m as usize);
        }
        for s in slots {
            self.apply_transform(s);
        }
    }

    pub fn reset_items(&mut self, ids: &[ElemId]) {
        let mut slots = BTreeSet::new();
        for id in ids {
            if self.elem_offsets.remove(id).is_some() {
                slots.insert(id.m as usize);
            }
        }
        for s in slots {
            self.apply_transform(s);
        }
    }

    pub fn reset_all_transforms(&mut self) {
        self.elem_offsets.clear();
        for x in self.model_xf.iter_mut() {
            *x = ModelXf::default();
        }
        self.xf_draft.clear();
        for i in 0..self.models.len() {
            self.apply_transform(i);
        }
    }

    pub fn refresh_caches(&mut self) {
        self.cache_types = self.types();
        self.mat_cache = None;
        self.profile_map = None;
        self.qto_result = None;
        // modelele s-au schimbat (incarcare / Refresh / eliminare) -> rezultatul Compare nu mai e valabil
        if self.compare.take().is_some() {
            self.compare_on = false;
            self.log.push("Compare results cleared because a model changed - run Compare again.".into());
        }
        self.prop_index = self.build_prop_index();
        let mut names: BTreeSet<String> = BTreeSet::new();
        for (c, ps) in &self.prop_index.cats {
            if c != ITEM_CAT {
                names.extend(ps.iter().map(|p| p.name.clone()));
            }
        }
        self.cache_props = names.into_iter().collect();
    }

    // ------------------------------------------------------------------ Quantification
    /// Semnatura starii de care depinde rezultatul (configuratie + selectie / elemente ascunse).
    pub fn qto_signature(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_string(&self.qto).unwrap_or_default().hash(&mut h);
        match self.qto.scope {
            crate::qto::QtoScope::All => {}
            crate::qto::QtoScope::Visible => {
                let mut v: Vec<&ElemId> = self.hidden.iter().collect();
                v.sort();
                v.hash(&mut h);
            }
            crate::qto::QtoScope::Selection => self.selected.hash(&mut h),
        }
        self.models.iter().map(|m| m.as_ref().map(|m| Arc::as_ptr(m) as usize).unwrap_or(0)).collect::<Vec<_>>().hash(&mut h);
        h.finish()
    }

    /// Recalculeaza cantitatile daca s-a schimbat ceva (configuratie, modele, selectie...).
    pub fn qto_update(&mut self) {
        let sig = self.qto_signature();
        if self.qto_result.is_none() || sig != self.qto_sig {
            self.qto_result = Some(self.qto_compute(&self.qto));
            self.qto_sig = sig;
        }
    }

    pub fn qto_compute(&self, setup: &crate::qto::QtoSetup) -> crate::qto::QtoResult {
        use crate::qto::{geo_qty, GeoQty, QtoResult, QtoRow, QtoScope, QtyColumn};
        use rayon::prelude::*;
        // elementele luate in calcul
        let ids: Vec<ElemId> = match setup.scope {
            QtoScope::All => self.all_ids(),
            QtoScope::Visible => self.all_ids().into_iter().filter(|i| self.is_visible(*i)).collect(),
            QtoScope::Selection => self.selected.iter().copied().collect(),
        };
        let ids: Vec<ElemId> = ids.into_iter().filter(|i| self.elem(*i).is_some()).collect();
        // geometria doar daca o cere vreo coloana; in paralel
        let need_geo = setup.columns.iter().any(|c| matches!(c, QtyColumn::Volume | QtyColumn::Surface | QtyColumn::PlanArea | QtyColumn::Length | QtyColumn::Height));
        let models = &self.models;
        let geo: Vec<GeoQty> = if need_geo {
            ids.par_iter()
                .map(|id| {
                    let m = models[id.m as usize].as_ref().unwrap();
                    geo_qty(m, &m.elements[id.e as usize])
                })
                .collect()
        } else {
            vec![GeoQty::default(); ids.len()]
        };
        let ncol = setup.columns.len();
        let mut units = vec![String::new(); ncol];
        let mut groups: HashMap<Vec<String>, QtoRow> = HashMap::new();
        for (k, id) in ids.iter().enumerate() {
            let (m, e) = self.elem(*id).unwrap();
            let keys: Vec<String> = setup
                .group_by
                .iter()
                .map(|lv| elem_values(m, e, &lv.cat, &lv.prop).into_iter().map(str::trim).find(|v| !v.is_empty()).map(String::from).unwrap_or_else(|| format!("(no {})", lv.prop.to_lowercase())))
                .collect();
            let g = &geo[k];
            let vals: Vec<Option<f64>> = setup
                .columns
                .iter()
                .enumerate()
                .map(|(ci, c)| match c {
                    QtyColumn::Count => Some(1.0),
                    QtyColumn::Volume => Some(g.volume),
                    QtyColumn::Surface => Some(g.surface),
                    QtyColumn::PlanArea => Some(g.plan),
                    QtyColumn::Length => Some(g.length),
                    QtyColumn::Height => Some(g.height),
                    QtyColumn::Prop { cat, prop } => elem_values(m, e, cat, prop).into_iter().find_map(|v| {
                        let x = leading_number(v)?;
                        if units[ci].is_empty() {
                            units[ci] = crate::profiler::unit_of(v);
                        }
                        Some(x)
                    }),
                })
                .collect();
            let row = groups.entry(keys.clone()).or_insert_with(|| QtoRow { keys, ids: Vec::new(), item_values: Vec::new(), values: vec![None; ncol], missing: vec![0; ncol] });
            for (ci, v) in vals.iter().enumerate() {
                match v {
                    Some(x) => row.values[ci] = Some(row.values[ci].unwrap_or(0.0) + x),
                    None => row.missing[ci] += 1,
                }
            }
            row.ids.push(*id);
            row.item_values.push(vals);
        }
        let mut rows: Vec<QtoRow> = groups.into_values().collect();
        rows.sort_by(|a, b| {
            for (x, y) in a.keys.iter().zip(&b.keys) {
                let (nx, ny) = (x.starts_with("(no "), y.starts_with("(no "));
                let o = nx.cmp(&ny).then_with(|| crate::profiler::natural_cmp(x, y));
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
            std::cmp::Ordering::Equal
        });
        let mut totals = vec![None; ncol];
        let mut missing = vec![0u32; ncol];
        for r in &rows {
            for ci in 0..ncol {
                if let Some(v) = r.values[ci] {
                    totals[ci] = Some(totals[ci].unwrap_or(0.0) + v);
                }
                missing[ci] += r.missing[ci];
            }
        }
        QtoResult { setup: setup.clone(), rows, totals, missing, count: ids.len(), units }
    }

    /// (model, nume, tip IFC, nivel, GlobalId) pentru foaia detaliata din Excel
    pub fn qto_item_info(&self, id: ElemId) -> [String; 5] {
        match self.elem(id) {
            Some((m, e)) => [m.name.clone(), e.name.clone(), e.ifc_type.clone(), e.storey.clone(), e.guid.clone()],
            None => Default::default(),
        }
    }

    // ------------------------------------------------------------------ Selection Tree
    /// Reconstruieste arborii tuturor modelelor dupa gruparea curenta si o salveaza in setari.
    pub fn set_tree_levels(&mut self, levels: Vec<TreeLevel>) {
        self.settings.tree_levels = levels;
        self.settings.save();
        self.rebuild_trees();
    }

    pub fn rebuild_trees(&mut self) {
        let levels = self.settings.tree_levels.clone();
        for i in 0..self.models.len() {
            let t = self.models[i].as_ref().map(|m| ModelTree::build(m, &levels));
            if i < self.trees.len() {
                self.trees[i] = t;
            }
        }
    }

    // ------------------------------------------------------------------ Appearance Profiler
    /// Valorile proprietatii profilului in toate modelele: (valoare -> nr. elemente, valori numerice, unitate).
    fn profile_values(&self) -> (HashMap<String, u32>, Vec<f64>, String) {
        let (cat, prop) = (&self.profile.cat, &self.profile.prop);
        let mut counts: HashMap<String, u32> = HashMap::new();
        let mut nums: Vec<f64> = Vec::new();
        let mut unit = String::new();
        for (_, m) in self.loaded_models() {
            for e in &m.elements {
                let vals = elem_values(m, e, cat, prop);
                if let Some(v) = vals.iter().map(|v| v.trim()).find(|v| !v.is_empty()) {
                    *counts.entry(v.to_string()).or_insert(0) += 1;
                }
                if let Some((v, x)) = vals.iter().find_map(|v| leading_number(v).map(|x| (*v, x))) {
                    if unit.is_empty() {
                        unit = crate::profiler::unit_of(v);
                    }
                    nums.push(x);
                }
            }
        }
        (counts, nums, unit)
    }

    /// Reface harta element -> regula si numaratorile (dupa import sau editarea regulilor).
    pub fn profile_recount(&mut self) {
        self.profile_map = None;
        let mut counts = vec![0u32; self.profile.rules.len()];
        for r in self.profile_map().values() {
            if let Some(c) = counts.get_mut(*r as usize) {
                *c += 1;
            }
        }
        for (r, c) in self.profile.rules.iter_mut().zip(counts) {
            r.count = c;
        }
        self.colors_dirty = true;
    }

    /// Dupa import: adauga culori pentru valorile din modelele curente care nu sunt in profil.
    pub fn profile_add_missing(&mut self) {
        let (counts, _, _) = self.profile_values();
        let n = self.profile.add_missing(counts.into_iter().collect());
        self.profile_recount();
        self.toast(if n == 0 { "Profiler: every value already has a colour.".to_string() } else { format!("Profiler: {n} new values added.") });
    }

    /// Valori din modele fara regula (doar modul By value) - pentru avertizarea dupa import.
    pub fn profile_uncovered(&self) -> usize {
        if self.profile.mode != crate::profiler::ProfileMode::Values {
            return 0;
        }
        let (counts, _, _) = self.profile_values();
        counts.keys().filter(|v| !self.profile.rules.iter().any(|r| r.label.eq_ignore_ascii_case(v))).count()
    }

    /// Genereaza regulile profilului din valorile proprietatii alese, pe toate modelele.
    pub fn profile_generate(&mut self) {
        use crate::profiler::ProfileMode;
        let prop = self.profile.prop.clone();
        let (counts, nums, unit) = self.profile_values();
        match self.profile.mode {
            ProfileMode::Values => {
                let dropped = self.profile.build_values(counts.into_iter().collect());
                if dropped > 0 {
                    self.toast(format!("Profiler: {dropped} rare values left out (max {} colours).", crate::profiler::MAX_RULES));
                }
            }
            ProfileMode::Ranges => {
                if nums.is_empty() {
                    self.error(format!("Profiler: \"{prop}\" has no numeric values. Use \"By value\" instead."));
                }
                self.profile.build_ranges(&nums, &unit);
            }
        }
        if self.profile.rules.is_empty() {
            self.toast(format!("Profiler: no element has \"{prop}\"."));
        }
        self.profile.active = !self.profile.rules.is_empty();
        self.profile_map = None;
        self.colors_dirty = true;
    }

    /// element -> indexul regulii (doar elementele care intra intr-o regula)
    pub fn profile_map(&mut self) -> &HashMap<ElemId, u16> {
        if self.profile_map.is_none() {
            let mut map = HashMap::new();
            for (mi, m) in self.loaded_models() {
                for (ei, e) in m.elements.iter().enumerate() {
                    let vals = elem_values(m, e, &self.profile.cat, &self.profile.prop);
                    if let Some(r) = self.profile.rule_for(&vals) {
                        map.insert(ElemId { m: mi as u32, e: ei as u32 }, r as u16);
                    }
                }
            }
            self.profile_map = Some(map);
        }
        self.profile_map.as_ref().unwrap()
    }

    /// Selecteaza elementele unei reguli (click pe o linie din legenda).
    pub fn profile_select(&mut self, rule: usize, add: bool) {
        let ids: Vec<ElemId> = self.profile_map().iter().filter(|(_, r)| **r as usize == rule).map(|(id, _)| *id).collect();
        self.apply(Action::Select(ids, add));
    }

    // ------------------------------------------------------------------ Compare
    pub fn run_compare(&mut self) {
        let (Some(a), Some(b)) = self.compare_pick else {
            return self.error("Compare: choose model A (old version) and model B (new version).");
        };
        if a == b {
            return self.error("Compare: A and B must be two different models.");
        }
        let (Some(ma), Some(mb)) = (self.models.get(a).cloned().flatten(), self.models.get(b).cloned().flatten()) else {
            return self.error("Compare: one of the models is not loaded.");
        };
        let t = Instant::now();
        let r = crate::compare::compare(&ma, a, &mb, b, self.compare_tol);
        let c = r.counts;
        self.toast(format!(
            "Compare {} → {}: {} removed, {} added, {} geometry, {} properties changed, {} unchanged ({:.0} ms)",
            ma.name, mb.name, c[0], c[1], c[2], c[3], c[4], t.elapsed().as_secs_f64() * 1e3
        ));
        self.compare = Some(r);
        self.compare_on = true;
        self.colors_dirty = true;
    }

    pub fn compare_clear(&mut self) {
        self.compare = None;
        self.compare_on = false;
        self.colors_dirty = true;
    }

    pub fn profile_clear(&mut self) {
        self.profile.rules.clear();
        self.profile.active = false;
        self.profile_map = None;
        self.colors_dirty = true;
    }

    pub fn reload_model(&mut self, slot: usize, path: String) {
        // deja in curs de incarcare -> nu porni a doua oara
        if self.loads.iter().any(|j| j.slot == slot) {
            return;
        }
        // pastram modelul vechi: ramane afisat pana vine cel nou, apoi remapam selectia etc.
        if let Some(old) = self.base_models.get(slot).cloned().flatten() {
            self.reloading.insert(slot, old);
        }
        self.start_load(slot, path);
    }

    /// Verifica (cel mult o data la 1.5 s) ce fisiere sursa s-au modificat pe disc.
    pub fn check_file_changes(&mut self) {
        if self.last_file_check.elapsed().as_secs_f32() < 1.5 {
            return;
        }
        self.scan_file_changes();
    }

    fn scan_file_changes(&mut self) {
        self.last_file_check = Instant::now();
        let mut changed = BTreeSet::new();
        for (i, m) in self.loaded_models() {
            if let (Some(old), Some(now)) = (self.file_stamps.get(&i), file_mtime(&m.path)) {
                if now != *old {
                    changed.insert(i);
                }
            }
        }
        self.changed_files = changed;
        self.scan_inbox();
        self.write_issue_outbox();
    }

    /// Issue-urile (viewpoints) pentru add-in-urile Revit / Navisworks ("ManageBIM Issues"), in
    /// %LOCALAPPDATA%\ManageBIM\Outbox\issues.txt - text simplu, coordonate WORLD in metri:
    ///   MBISSUES 1
    ///   VIEW <tab> nume <tab> stare <tab> guid <tab> prioritate <tab> responsabil
    ///   CAM <tab> ex ey ez <tab> fx fy fz <tab> ux uy uz <tab> fov_grade <tab> inaltime_ortho (0 = perspectiva)
    ///   BOX <tab> minx miny minz <tab> maxx maxy maxz        (daca are section box)
    ///   SEL <tab> guid <tab> guid ...                        (elementele selectate)
    ///   TXT <tab> text (\n codat ca \\n)                     (descriere + comentarii)
    ///   END
    /// Se rescrie doar cand se schimba ceva (scriere atomica).
    pub fn write_issue_outbox(&mut self) {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_string(&self.viewpoints).unwrap_or_default().hash(&mut h);
        self.origin_f64().to_array().iter().for_each(|v| v.to_bits().hash(&mut h));
        let sig = h.finish() | 1;
        if sig == self.outbox_sig {
            return;
        }
        self.outbox_sig = sig;
        let Some(dir) = std::env::var_os("LOCALAPPDATA").map(|d| std::path::PathBuf::from(d).join("ManageBIM").join("Outbox")) else { return };
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let clean = |s: &str| s.replace('\\', "\\\\").replace('\t', " ").replace("\r\n", "\\n").replace('\n', "\\n");
        let v3 = |p: DVec3| format!("{:.6} {:.6} {:.6}", p.x, p.y, p.z);
        let mut s = String::from("MBISSUES 1\n");
        for v in &self.viewpoints {
            let c = v.cam;
            s.push_str(&format!("VIEW\t{}\t{}\t{}\t{}\t{}\n", clean(&v.name), clean(&v.issue.status), clean(&v.issue.guid), clean(&v.issue.priority), clean(&v.issue.assigned)));
            s.push_str(&format!(
                "CAM\t{}\t{}\t{}\t{:.4}\t{:.6}\n",
                v3(self.world(c.eye())),
                v3(c.fwd().as_dvec3()),
                v3(c.up().as_dvec3()),
                c.fov_deg,
                if c.ortho { 2.0 * c.ortho_half() as f64 } else { 0.0 }
            ));
            if v.section.enabled && v.section.mode == SectionMode::Box {
                s.push_str(&format!("BOX\t{}\t{}\n", v3(self.world(Vec3::from(v.section.bmin))), v3(self.world(Vec3::from(v.section.bmax)))));
            }
            let sel: Vec<String> = v.selected.iter().filter_map(|id| self.elem(*id).map(|(_, e)| e.guid.clone())).filter(|g| !g.is_empty()).take(2000).collect();
            if !sel.is_empty() {
                s.push_str(&format!("SEL\t{}\n", sel.iter().map(|g| clean(g)).collect::<Vec<_>>().join("\t")));
            }
            let mut text = v.comment.clone();
            for k in &v.comments {
                text.push_str(&format!("\n{} ({}): {}", k.author, crate::markup::pretty_date(&k.date), k.text));
            }
            if !text.trim().is_empty() {
                s.push_str(&format!("TXT\t{}\n", clean(text.trim())));
            }
            // markup-uri ca polilinii (RED) si texte (RTX): culoare, grosime, puncte fata de centrul vederii /
            // inaltimea ei (y in jos) - add-in-urile le pun pe un plan in fata camerei
            for r in &v.redlines {
                let col = format!("{} {} {}", r.color[0], r.color[1], r.color[2]);
                if r.kind == crate::markup::RedKind::Text {
                    if let Some(p) = r.pts.first() {
                        s.push_str(&format!("RTX\t{col}\t{:.1}\t{:.5} {:.5}\t{}\n", r.width, p[0], p[1], clean(&r.text)));
                    }
                    continue;
                }
                for pl in crate::markup::redline_polylines(r) {
                    if pl.len() >= 2 {
                        let pts: Vec<String> = pl.iter().map(|p| format!("{:.5} {:.5}", p[0], p[1])).collect();
                        s.push_str(&format!("RED\t{col}\t{:.1}\t{}\n", r.width, pts.join(" ")));
                    }
                }
            }
            s.push_str("END\n");
        }
        let path = dir.join("issues.txt");
        let tmp = dir.join("issues.txt.tmp");
        if std::fs::write(&tmp, s).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }

    /// "Send to ManageBIM" din Revit / AutoCAD / Plant 3D / Navisworks: add-in-ul scrie modelul in
    /// %LOCALAPPDATA%\ManageBIM\Inbox\<sursa>.bimc (scriere atomica: .tmp + redenumire).
    /// Un fisier nou se adauga in scena; unul deja deschis si rescris se reincarca singur (live),
    /// pastrand selectia, ascunderile si culorile (potrivire pe GUID).
    fn scan_inbox(&mut self) {
        let Some(dir) = inbox_dir() else { return };
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        static STARTED: std::sync::OnceLock<std::time::SystemTime> = std::sync::OnceLock::new();
        let started = *STARTED.get_or_init(|| std::time::SystemTime::now() - std::time::Duration::from_secs(5));
        let loading: HashSet<String> = self.loads.iter().map(|j| j.path.to_lowercase()).collect();
        let loaded: HashMap<String, usize> = self.loaded_models().map(|(i, m)| (m.path.to_lowercase(), i)).collect();
        let mut new_files = Vec::new();
        for e in rd.flatten() {
            let p = e.path();
            if crate::formats::ext_of(&p.to_string_lossy()) != "bimc" {
                continue;
            }
            let ps = p.to_string_lossy().to_string();
            let key = ps.to_lowercase();
            if loading.contains(&key) {
                continue;
            }
            if let Some(&slot) = loaded.get(&key) {
                if self.changed_files.contains(&slot) {
                    self.changed_files.remove(&slot);
                    self.toast(format!("Live update: {}", file_stem(&ps)));
                    self.reload_model(slot, ps);
                }
            } else if file_mtime(&ps).map(|t| t > started).unwrap_or(false) && !self.inbox_seen.contains(&key) {
                // doar ce a sosit dupa pornire (nu redeschidem trimiterile vechi la fiecare start)
                self.inbox_seen.insert(key);
                new_files.push(ps);
            }
        }
        for p in new_files {
            self.toast(format!("Received: {}", file_stem(&p)));
            let append = self.loaded_models().next().is_some() || !self.loads.is_empty();
            self.open_path(&p, append);
        }
    }

    /// Butonul Refresh (F5): reincarca modelele ale caror fisiere s-au modificat.
    /// `force` (Ctrl+F5 / Shift+click) reincarca toate modelele, modificate sau nu.
    pub fn refresh_models(&mut self, force: bool) {
        self.scan_file_changes();
        let mut todo: Vec<(usize, String)> = Vec::new();
        let mut missing = Vec::new();
        for (i, m) in self.loaded_models() {
            if !std::path::Path::new(&m.path).exists() {
                missing.push(m.name.clone());
            } else if force || self.changed_files.contains(&i) {
                todo.push((i, m.path.clone()));
            }
        }
        for n in missing {
            self.error(format!("Refresh: file not found for {n}"));
        }
        if todo.is_empty() {
            if self.loaded_models().next().is_none() {
                self.toast("Refresh: no models loaded.");
            } else {
                self.toast("Refresh: no file has changed since it was loaded (Ctrl+F5 reloads everything).");
            }
            return;
        }
        let names: Vec<String> = todo.iter().map(|(_, p)| crate::file_name(p)).collect();
        self.toast(format!("Refreshing {} model(s): {}", todo.len(), names.join(", ")));
        for (i, p) in todo {
            self.reload_model(i, p);
        }
    }

    /// Dupa reincarcare: muta selectia, ascunderile, culorile, deplasarile, seturile
    /// si rezultatele de clash de pe modelul vechi pe cel nou, potrivind elementele dupa GUID.
    fn remap_reloaded(&mut self, slot: usize, old: &Model, new: &Model) {
        let by_guid: HashMap<&str, u32> = new.elements.iter().enumerate().filter(|(_, e)| !e.guid.is_empty()).map(|(i, e)| (e.guid.as_str(), i as u32)).collect();
        let n_new = new.elements.len() as u32;
        let s = slot as u32;
        // element vechi -> element nou (None = elementul a disparut din fisier)
        let map_e = |e: u32| -> Option<u32> {
            let oe = old.elements.get(e as usize)?;
            if !oe.guid.is_empty() {
                return by_guid.get(oe.guid.as_str()).copied();
            }
            // fara GUID (glTF / DXF / OBJ...): pastram indexul daca inca exista
            (e < n_new).then_some(e)
        };
        let rid = |id: ElemId| -> Option<ElemId> {
            if id.m != s {
                return Some(id);
            }
            map_e(id.e).map(|e| ElemId { m: s, e })
        };

        self.selected = self.selected.iter().filter_map(|i| rid(*i)).collect();
        self.primary = self.primary.and_then(|i| rid(i));

        // ascunse: remapate; elementele noi ascunse implicit (ex. IfcSpace) raman ascunse
        let mapped: HashSet<u32> = (0..old.elements.len() as u32).filter_map(|e| map_e(e)).collect();
        let mut hidden: HashSet<ElemId> = self.hidden.iter().filter_map(|i| rid(*i)).collect();
        for (k, e) in new.elements.iter().enumerate() {
            if e.default_hidden && !mapped.contains(&(k as u32)) {
                hidden.insert(ElemId { m: s, e: k as u32 });
            }
        }
        self.hidden = hidden;
        self.overrides = self.overrides.iter().filter_map(|(i, c)| rid(*i).map(|n| (n, *c))).collect();
        self.elem_offsets = self.elem_offsets.iter().filter_map(|(i, o)| rid(*i).map(|n| (n, *o))).collect();
        for set in self.sets.iter_mut() {
            if let SetKind::Explicit(ids) = &mut set.kind {
                *ids = ids.iter().filter_map(|i| rid(*i)).collect();
            }
        }
        let mut stale = 0;
        for t in self.clash_tests.iter_mut() {
            for r in t.results.iter_mut() {
                match (rid(r.a), rid(r.b)) {
                    (Some(a), Some(b)) => {
                        r.a = a;
                        r.b = b;
                    }
                    _ => stale += 1,
                }
            }
        }
        let gone = old.elements.len().saturating_sub(mapped.len());
        let added = new.elements.len().saturating_sub(mapped.len());
        self.log.push(format!("Refresh {}: {} elements kept, {} new, {} removed", new.name, mapped.len(), added, gone));
        if stale > 0 {
            self.log.push(format!("Refresh {}: {stale} clash result(s) refer to removed elements - run the clash tests again", new.name));
        }
        self.clash_focus = None;
        self.min_dist_result = None;
        self.sim_cache.clear();
        self.sim_cache_day = i32::MIN;
        self.mat_cache = None;
    }

    pub fn reset(&mut self) {
        self.bimd_path = None;
        self.blank_project = false;
        {
            let mut r = self.renderer.lock().unwrap();
            for i in 0..self.models.len() {
                r.pending.push(Upload::Remove { slot: i as u32 });
            }
        }
        self.models.clear();
        self.walk_path.clear();
        self.path_run = None;
        self.trees.clear();
        self.base_models.clear();
        self.model_xf.clear();
        self.elem_offsets.clear();
        self.xf_draft.clear();
        self.animations.clear();
        self.mat_assign.clear();
        self.materials = crate::materials::library();
        self.home = None;
        self.mat_cache = None;
        self.cur_anim = 0;
        self.anim_play = None;
        self.anim_sim_active = false;
        self.loads.clear();
        *self.origin.lock().unwrap() = None;
        self.selected.clear();
        self.primary = None;
        self.hidden.clear();
        self.overrides.clear();
        self.measures.clear();
        self.cur_measure.clear();
        self.section = Section::default();
        self.viewpoints.clear();
        self.sets.clear();
        self.clash_tests = vec![ClashTest::new("Test 1".into())];
        self.cur_test = 0;
        self.cur_result = None;
        self.clash_focus = None;
        self.tasks.clear();
        self.cur_task = None;
        self.sim_active = false;
        self.project_path = None;
        self.colors_dirty = true;
        self.cache_types.clear();
        self.cache_props.clear();
        self.file_stamps.clear();
        self.reloading.clear();
        self.changed_files.clear();
        self.looks.clear();
        self.profile = Default::default();
        self.profile_map = None;
        self.compare = None;
        self.compare_on = false;
        self.compare_pick = (None, None);
        self.qto = Default::default();
        self.qto_result = None;
    }

    pub fn remove_model(&mut self, slot: usize) {
        self.file_stamps.remove(&slot);
        self.reloading.remove(&slot);
        self.changed_files.remove(&slot);
        self.looks.remove(&slot);
        if slot < self.models.len() {
            self.models[slot] = None;
            self.trees[slot] = None;
            if slot < self.base_models.len() {
                self.base_models[slot] = None;
            }
            self.elem_offsets.retain(|k, _| k.m as usize != slot);
            self.renderer.lock().unwrap().pending.push(Upload::Remove { slot: slot as u32 });
            self.selected.retain(|i| i.m as usize != slot);
            self.colors_dirty = true;
        }
    }

    pub fn poll_jobs(&mut self) {
        let mut done = Vec::new();
        for (i, j) in self.loads.iter_mut().enumerate() {
            while let Ok(m) = j.rx.try_recv() {
                match m {
                    LoadMsg::Progress(f, s) => {
                        j.progress = f;
                        j.stage = s;
                    }
                    LoadMsg::Done(r) => done.push((i, r)),
                }
            }
        }
        for (i, r) in done.into_iter().rev() {
            let job = self.loads.remove(i);
            match r {
                Ok((m, note)) => {
                    if let Some(n) = note {
                        self.log.push(n);
                    }
                    let first = self.loaded_models().next().is_none();
                    while self.base_models.len() <= job.slot {
                        self.base_models.push(None);
                    }
                    while self.model_xf.len() <= job.slot {
                        self.model_xf.push(ModelXf::default());
                    }
                    self.base_models[job.slot] = Some(m.clone());
                    if let Some(old) = self.reloading.remove(&job.slot) {
                        // Refresh: pastram starea utilizatorului (selectie, ascunse, culori...)
                        self.remap_reloaded(job.slot, &old, &m);
                    } else {
                        for (k, e) in m.elements.iter().enumerate() {
                            if e.default_hidden {
                                self.hidden.insert(ElemId { m: job.slot as u32, e: k as u32 });
                            }
                        }
                    }
                    self.trees[job.slot] = Some(ModelTree::build(&m, &self.settings.tree_levels));
                    let warn = m.warnings.clone();
                    self.toast(format!("Loaded {}: {} elements, {} triangles", m.name, m.elements.len(), m.tri_total()));
                    for w in warn {
                        self.log.push(format!("{}: {w}", m.name));
                    }
                    self.models[job.slot] = Some(m);
                    self.apply_transform(job.slot);
                    if first && self.pending_project.is_none() {
                        let bb = self.scene_aabb();
                        self.cam.yaw = std::f32::consts::FRAC_PI_4;
                        self.cam.pitch = -0.55;
                        self.cam.fit(&bb);
                    }
                    self.colors_dirty = true;
                    self.refresh_caches();
                }
                Err(e) => {
                    // la Refresh esuat (ex. fisier inca in curs de scriere) ramane modelul vechi pe ecran
                    if self.reloading.remove(&job.slot).is_some() {
                        self.changed_files.insert(job.slot);
                        self.error(format!("Refresh failed, keeping the previous version of {}: {e}", crate::file_name(&job.path)));
                    } else {
                        self.error(format!("{}: {e}", crate::file_name(&job.path)));
                    }
                }
            }
        }
        if self.loads.is_empty() {
            if let Some(p) = self.pending_project.take() {
                self.apply_project_state(p);
            }
            for p in std::mem::take(&mut self.pending_csv) {
                self.import_tasks_csv(&p);
            }
        }
        // clash
        if let Some(job) = &self.clash_job {
            let msg = job.rx.try_recv();
            // firul de clash s-a oprit fara rezultat (eroare interna): nu ramane blocat pe "ruleaza"
            if let Err(std::sync::mpsc::TryRecvError::Disconnected) = msg {
                self.clash_job = None;
                self.clash_queue.clear();
                self.error("Internal error while running the clash test; the test was stopped.".to_string());
                return;
            }
            if let Ok(found) = msg {
                let t = job.test;
                let secs = job.started.elapsed().as_secs_f32();
                let cancelled = job.prog.cancel.load(std::sync::atomic::Ordering::Relaxed);
                self.clash_job = None;
                if cancelled {
                    self.clash_queue.clear();
                    self.toast("Clash test cancelled.");
                } else {
                    self.merge_clash(t, found, secs);
                    if !self.clash_queue.is_empty() {
                        let n = self.clash_queue.remove(0);
                        self.cur_test = n;
                        self.run_clash(n);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------ clash
    /// Runs a test on the calling thread (batch use / tests).
    pub fn run_clash_blocking(&mut self, t: usize) {
        let test = self.clash_tests[t].clone();
        let excluded = self.excluded_items(&test.rules);
        let a: Vec<ElemId> = self.resolve_selector(&test.a).into_iter().filter(|i| !excluded.contains(i)).collect();
        let b: Vec<ElemId> = self.resolve_selector(&test.b).into_iter().filter(|i| !excluded.contains(i)).collect();
        let found = clash::run(&self.models, &a, &b, test.ty, test.tol_mm / 1000.0, test.ignore_same_file, &clash::Progress::new());
        self.merge_clash(t, found, 0.0);
    }

    pub fn run_clash(&mut self, t: usize) {
        if self.clash_job.is_some() {
            return;
        }
        let Some(test) = self.clash_tests.get(t) else { return };
        let excluded = self.excluded_items(&test.rules);
        let a: Vec<ElemId> = self.resolve_selector(&test.a).into_iter().filter(|i| !excluded.contains(i)).collect();
        let b: Vec<ElemId> = self.resolve_selector(&test.b).into_iter().filter(|i| !excluded.contains(i)).collect();
        if a.is_empty() || b.is_empty() {
            self.error("Selection A or B contains no elements.");
            return;
        }
        let models: Vec<Option<Arc<Model>>> = self.models.clone();
        let (ty, tol, ign) = (test.ty, test.tol_mm / 1000.0, test.ignore_same_file);
        let prog = clash::Progress::new();
        let p2 = prog.clone();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            // la o eroare interna tx se inchide fara mesaj -> poll_jobs opreste testul curat
            if let Ok(r) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| clash::run(&models, &a, &b, ty, tol, ign, &p2))) {
                let _ = tx.send(r);
            }
        });
        self.clash_job = Some(ClashJob { test: t, prog, rx, started: Instant::now() });
    }

    /// Items that a test must never use (exclude sets + exclude rules).
    pub fn excluded_items(&self, rules: &ClashRules) -> HashSet<ElemId> {
        let mut ex = HashSet::new();
        for n in &rules.exclude_sets {
            if let Some(set) = self.sets.iter().find(|s| &s.name == n) {
                ex.extend(self.resolve_set(set));
            }
        }
        let active: Vec<&SearchRule> = rules.exclude_rules.iter().filter(|r| !r.op.needs_value() || !r.value.trim().is_empty()).collect();
        if !active.is_empty() {
            for id in self.all_ids() {
                if active.iter().any(|r| self.rule_match(id, r)) {
                    ex.insert(id);
                }
            }
        }
        ex
    }

    /// Pair rules: same file / level / type / property value / set.
    pub fn pair_ignored(&self, test: &ClashTest, set_members: &[HashSet<ElemId>], a: ElemId, b: ElemId) -> bool {
        let r = &test.rules;
        if test.ignore_same_file && a.m == b.m {
            return true;
        }
        let (Some((_, ea)), Some((_, eb))) = (self.elem(a), self.elem(b)) else { return false };
        if r.same_level && !ea.storey.is_empty() && ea.storey == eb.storey {
            return true;
        }
        if r.same_type && ea.ifc_type == eb.ifc_type {
            return true;
        }
        for pn in &r.same_props {
            let va = ea.props.iter().find(|p| p.name.eq_ignore_ascii_case(pn)).map(|p| p.value.as_str());
            let vb = eb.props.iter().find(|p| p.name.eq_ignore_ascii_case(pn)).map(|p| p.value.as_str());
            if let (Some(x), Some(y)) = (va, vb) {
                if !x.is_empty() && x.eq_ignore_ascii_case(y) {
                    return true;
                }
            }
        }
        set_members.iter().any(|s| s.contains(&a) && s.contains(&b))
    }

    pub fn same_set_members(&self, rules: &ClashRules) -> Vec<HashSet<ElemId>> {
        rules.same_sets.iter().filter_map(|n| self.sets.iter().find(|s| &s.name == n)).map(|s| self.resolve_set(s).into_iter().collect()).collect()
    }

    pub fn merge_clash(&mut self, t: usize, found: Vec<clash::Found>, secs: f32) {
        let now = now_string();
        // reguli: perechi ignorate + elemente excluse
        let test_cfg = self.clash_tests[t].clone();
        let excluded = self.excluded_items(&test_cfg.rules);
        let set_members = self.same_set_members(&test_cfg.rules);
        let before = found.len();
        let found: Vec<clash::Found> = found.into_iter().filter(|f| !self.pair_ignored(&test_cfg, &set_members, f.a, f.b)).collect();
        let n_ignored = before - found.len();
        let stale: HashSet<(String, String)> = test_cfg
            .results
            .iter()
            .filter(|r| excluded.contains(&r.a) || excluded.contains(&r.b) || self.pair_ignored(&test_cfg, &set_members, r.a, r.b))
            .map(|r| (r.ga.clone(), r.gb.clone()))
            .collect();
        let guid = |s: &App, id: ElemId| s.elem(id).map(|(m, e)| format!("{}|{}", m.name, if e.guid.is_empty() { e.name.clone() } else { e.guid.clone() })).unwrap_or_default();
        let new_items: Vec<(clash::Found, String, String)> = found.into_iter().map(|f| {
            let ga = guid(self, f.a);
            let gb = guid(self, f.b);
            (f, ga, gb)
        }).collect();
        let test = &mut self.clash_tests[t];
        let mut old: HashMap<(String, String), ClashResult> = test.results.drain(..).map(|r| ((r.ga.clone(), r.gb.clone()), r)).collect();
        let mut next = old.values().filter_map(|r| r.name.strip_prefix("Clash").and_then(|n| n.parse::<u32>().ok())).max().unwrap_or(0) + 1;
        let mut out = Vec::new();
        let (mut n_new, mut n_act) = (0, 0);
        for (f, ga, gb) in new_items {
            let key = (ga.clone(), gb.clone());
            let prev = old.remove(&key).or_else(|| old.remove(&(gb.clone(), ga.clone())));
            let r = match prev {
                Some(mut p) => {
                    p.status = match p.status {
                        ClashStatus::New | ClashStatus::Active => ClashStatus::Active,
                        ClashStatus::Resolved => ClashStatus::New,
                        s => s,
                    };
                    p.a = f.a;
                    p.b = f.b;
                    p.point = f.point.into();
                    p.dist = f.dist;
                    p
                }
                None => {
                    let n = format!("Clash{next}");
                    next += 1;
                    ClashResult { name: n, a: f.a, b: f.b, ga, gb, point: f.point.into(), dist: f.dist, status: ClashStatus::New, assigned: String::new(), comment: String::new(), found: now.clone(), group: String::new() }
                }
            };
            if r.status == ClashStatus::New {
                n_new += 1;
            } else {
                n_act += 1;
            }
            out.push(r);
        }
        // rezultatele vechi excluse acum de reguli dispar; restul devin Resolved
        old.retain(|k, _| !stale.contains(k));
        let n_res = old.len();
        let mut res_old: Vec<ClashResult> = old.into_values().collect();
        res_old.sort_by(|x, y| x.name.cmp(&y.name));
        for mut r in res_old {
            r.status = ClashStatus::Resolved;
            out.push(r);
        }
        test.results = out;
        test.last_run = now;
        let name = test.name.clone();
        self.cur_result = None;
        self.sel_results.clear();
        let ign = if n_ignored > 0 { format!(", {n_ignored} ignored by rules") } else { String::new() };
        self.toast(format!("{name}: {n_new} new, {n_act} active, {n_res} resolved{ign} ({secs:.1} s)"));
        self.colors_dirty = true;
    }

    // ------------------------------------------------------------------ clash groups
    pub fn group_names(&self, t: usize) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for r in &self.clash_tests[t].results {
            if !r.group.is_empty() && !v.contains(&r.group) {
                v.push(r.group.clone());
            }
        }
        v
    }

    pub fn new_group(&mut self, t: usize, idx: &[usize]) {
        if idx.is_empty() {
            return;
        }
        let names = self.group_names(t);
        let mut n = names.len() + 1;
        while names.contains(&format!("Group {n}")) {
            n += 1;
        }
        let name = format!("Group {n}");
        for i in idx {
            if let Some(r) = self.clash_tests[t].results.get_mut(*i) {
                r.group = name.clone();
            }
        }
        self.open_groups.insert(name.clone());
        self.toast(format!("{} clashes grouped as \"{name}\".", idx.len()));
    }

    pub fn ungroup(&mut self, t: usize, idx: &[usize]) {
        for i in idx {
            if let Some(r) = self.clash_tests[t].results.get_mut(*i) {
                r.group.clear();
            }
        }
    }

    pub fn auto_group(&mut self, t: usize, by: GroupBy, cell: f32) {
        let keys: Vec<String> = self.clash_tests[t]
            .results
            .iter()
            .map(|r| {
                let ea = self.elem(r.a).map(|x| x.1);
                let eb = self.elem(r.b).map(|x| x.1);
                match by {
                    GroupBy::Level => format!("Level: {}", ea.map(|e| if e.storey.is_empty() { "(no level)".to_string() } else { e.storey.clone() }).unwrap_or_default()),
                    GroupBy::ItemA => format!("A: {}", ea.map(|e| e.name.clone()).unwrap_or_default()),
                    GroupBy::ItemB => format!("B: {}", eb.map(|e| e.name.clone()).unwrap_or_default()),
                    GroupBy::TypePair => format!("{} × {}", ea.map(|e| e.ifc_type.clone()).unwrap_or_default(), eb.map(|e| e.ifc_type.clone()).unwrap_or_default()),
                    GroupBy::Grid => format!("Grid {}", self.grid_location(Vec3::from(r.point))),
                    GroupBy::Proximity => {
                        let c = cell.max(0.1);
                        let p = r.point;
                        format!("Area {:+.0}/{:+.0}/{:+.0}", (p[0] / c).floor(), (p[1] / c).floor(), (p[2] / c).floor())
                    }
                }
            })
            .collect();
        let mut count: HashMap<&String, usize> = HashMap::new();
        for k in &keys {
            *count.entry(k).or_default() += 1;
        }
        let mut n = 0;
        let mut groups = HashSet::new();
        for (r, k) in self.clash_tests[t].results.iter_mut().zip(keys.iter()) {
            // un singur clash nu formeaza grup
            if count[k] > 1 {
                r.group = k.clone();
                groups.insert(k.clone());
                n += 1;
            } else {
                r.group.clear();
            }
        }
        self.toast(format!("Auto-grouped by {}: {} groups, {n} clashes.", by.label().to_lowercase(), groups.len()));
    }

    /// Combined status of a group: the "most open" status of its members.
    pub fn group_status(&self, t: usize, members: &[usize]) -> ClashStatus {
        let res = &self.clash_tests[t].results;
        for s in ClashStatus::ALL {
            if members.iter().any(|i| res[*i].status == s) {
                return s;
            }
        }
        ClashStatus::Resolved
    }

    pub fn elem_label(&self, id: ElemId) -> String {
        self.elem(id).map(|(_, e)| e.name.clone()).unwrap_or_else(|| "(missing)".into())
    }

    pub fn clash_report_html(&self, t: usize) -> String {
        let test = &self.clash_tests[t];
        let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let mut h = String::new();
        h.push_str("<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Clash report</title><style>");
        h.push_str("body{font-family:Segoe UI,Arial,sans-serif;margin:24px;color:#222}table{border-collapse:collapse;width:100%;font-size:13px}th,td{border:1px solid #ccc;padding:5px 8px;text-align:left}th{background:#2d3e50;color:#fff}tr:nth-child(even){background:#f4f6f8}.s{font-weight:600}.sum span{display:inline-block;margin-right:18px}");
        h.push_str("</style></head><body>");
        h.push_str(&format!("<h1>Clash Detective Report — {}</h1>", esc(&test.name)));
        h.push_str(&format!(
            "<p>Generated by ManageBIM on {}. Type: <b>{}</b>, tolerance {} mm. Selection A: {} · Selection B: {}. Last run: {}.</p>",
            now_string(),
            test.ty.label(),
            test.tol_mm,
            esc(&test.a.label()),
            esc(&test.b.label()),
            esc(&test.last_run)
        ));
        let rc = test.rules.count(test.ignore_same_file);
        if rc > 0 {
            h.push_str(&format!("<p>Rules applied: {rc}.</p>"));
        }
        h.push_str("<p class=\"sum\">");
        for s in ClashStatus::ALL {
            h.push_str(&format!("<span>{}: <b>{}</b></span>", s.label(), test.count(s)));
        }
        h.push_str(&format!("<span>Total: <b>{}</b></span></p>", test.results.len()));
        h.push_str("<table><tr><th>Group</th><th>Name</th><th>Status</th><th>Distance (m)</th><th>Item A</th><th>Type A</th><th>Item B</th><th>Type B</th><th>Level</th><th>Location (grid / level)</th><th>Point X,Y,Z (local)</th><th>Found</th><th>Assigned to</th><th>Comment</th></tr>");
        let grids = self.scene_grids();
        let mut order: Vec<&ClashResult> = test.results.iter().collect();
        order.sort_by_key(|r| (r.group.is_empty(), r.group.clone()));
        for r in order {
            let gl = grid_location_in(&grids, Vec3::from(r.point));
            let (ta, la, sa) = self.elem(r.a).map(|(m, e)| (e.ifc_type.clone(), format!("{} [{}]", e.name, m.name), e.storey.clone())).unwrap_or_default();
            let (tb, lb) = self.elem(r.b).map(|(m, e)| (e.ifc_type.clone(), format!("{} [{}]", e.name, m.name))).unwrap_or_default();
            let c = r.status.color();
            h.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"s\" style=\"color:rgb({},{},{})\">{}</td><td>{:.3}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.2}, {:.2}, {:.2}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(&r.group), esc(&r.name), c[0], c[1], c[2], r.status.label(), r.dist, esc(&la), esc(&ta), esc(&lb), esc(&tb), esc(&sa), esc(&gl),
                r.point[0], r.point[1], r.point[2], esc(&r.found), esc(&r.assigned), esc(&r.comment)
            ));
        }
        h.push_str("</table></body></html>");
        h
    }

    pub fn clash_report_csv(&self, t: usize) -> String {
        let test = &self.clash_tests[t];
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut s = String::from("\u{feff}Group;Name;Status;Distance_m;ItemA;TypeA;ModelA;GuidA;ItemB;TypeB;ModelB;GuidB;Level;Location;X;Y;Z;Found;AssignedTo;Comment\n");
        let grids = self.scene_grids();
        let mut order: Vec<&ClashResult> = test.results.iter().collect();
        order.sort_by_key(|r| (r.group.is_empty(), r.group.clone()));
        for r in order {
            let a = self.elem(r.a);
            let b = self.elem(r.b);
            s.push_str(&format!(
                "{};{};{};{:.4};{};{};{};{};{};{};{};{};{};{};{:.3};{:.3};{:.3};{};{};{}\n",
                q(&r.group),
                q(&r.name),
                r.status.label(),
                r.dist,
                q(&a.map(|x| x.1.name.clone()).unwrap_or_default()),
                q(&a.map(|x| x.1.ifc_type.clone()).unwrap_or_default()),
                q(&a.map(|x| x.0.name.clone()).unwrap_or_default()),
                q(&a.map(|x| x.1.guid.clone()).unwrap_or_default()),
                q(&b.map(|x| x.1.name.clone()).unwrap_or_default()),
                q(&b.map(|x| x.1.ifc_type.clone()).unwrap_or_default()),
                q(&b.map(|x| x.0.name.clone()).unwrap_or_default()),
                q(&b.map(|x| x.1.guid.clone()).unwrap_or_default()),
                q(&a.map(|x| x.1.storey.clone()).unwrap_or_default()),
                q(&grid_location_in(&grids, Vec3::from(r.point))),
                r.point[0],
                r.point[1],
                r.point[2],
                q(&r.found),
                q(&r.assigned),
                q(&r.comment)
            ));
        }
        s
    }

    /// distanta minima intre primele doua elemente selectate
    pub fn min_distance_selected(&mut self) {
        let ids: Vec<ElemId> = self.selected.iter().copied().take(2).collect();
        if ids.len() != 2 {
            self.error("Select exactly two elements.");
            return;
        }
        let (Some((ma, ea)), Some((mb, eb))) = (self.elem(ids[0]), self.elem(ids[1])) else { return };
        let r = clash::bvh_min_dist(&ea.bvh, ma.tris(ea), &eb.bvh, mb.tris(eb), f32::INFINITY);
        self.min_dist_result = r;
        match r {
            Some((d, _)) => self.toast(format!("Shortest distance between the elements: {}", fmt_len(d as f64))),
            None => self.error("The distance could not be computed."),
        }
    }

    // ------------------------------------------------------------------ timeliner
    pub fn sim_range(&self) -> Option<(i32, i32)> {
        timeline::range(&self.tasks)
    }

    pub fn update_sim(&mut self, dt: f32) {
        if !self.sim_view() {
            self.sim_play = false;
            if !self.sim_cache.is_empty() {
                self.sim_cache.clear();
                self.sim_cache_day = i32::MIN;
                self.colors_dirty = true;
            }
            return;
        }
        let Some((s, e)) = self.sim_range() else { return };
        if self.sim_play {
            self.sim_day += dt * self.sim_speed;
            if self.sim_day >= e as f32 + 1.0 {
                self.sim_day = e as f32 + 1.0;
                self.sim_play = false;
            }
        }
        self.sim_day = self.sim_day.clamp(s as f32 - 1.0, e as f32 + 1.0);
        let d = self.sim_day.floor() as i32;
        if d != self.sim_cache_day {
            self.sim_cache = timeline::state_at(&self.tasks, d);
            self.sim_cache_day = d;
            self.colors_dirty = true;
        }
    }

    pub fn invalidate_sim(&mut self) {
        self.sim_cache_day = i32::MIN;
    }

    // ------------------------------------------------------------------ culori
    // ------------------------------------------------------------------ materials
    /// element -> material index, resolved from the assignment rules (later rules win)
    pub fn material_map(&mut self) -> &HashMap<ElemId, u16> {
        if self.mat_cache.is_none() {
            let mut map: HashMap<ElemId, u16> = HashMap::new();
            for a in self.mat_assign.iter().filter(|a| a.enabled) {
                let Some(mi) = self.materials.iter().position(|m| m.name == a.material) else { continue };
                let ids: Vec<ElemId> = match &a.target {
                    MatTarget::Set(n) => self.sets.iter().find(|s| &s.name == n).map(|s| self.resolve_set(s)).unwrap_or_default(),
                    MatTarget::Items(v) => v.iter().copied().filter(|i| self.elem(*i).is_some()).collect(),
                    MatTarget::Type(t) => {
                        let t = t.to_lowercase();
                        self.all_ids().into_iter().filter(|i| self.elem(*i).map_or(false, |(_, e)| e.ifc_type.to_lowercase().contains(&t))).collect()
                    }
                };
                for id in ids {
                    map.insert(id, mi as u16);
                }
            }
            self.mat_cache = Some(map);
        }
        self.mat_cache.as_ref().unwrap()
    }

    /// ViewCube Home: saved home view, otherwise the whole model in isometric
    pub fn go_home(&mut self) {
        match self.home {
            Some(h) => {
                let mut t = h;
                t.ortho = self.cam.ortho;
                self.fly_to(t);
            }
            None => {
                let bb = self.scene_aabb();
                let mut t = self.cam;
                t.set_view(std::f32::consts::FRAC_PI_4, -0.6, &bb);
                self.fly_to(t);
            }
        }
    }

    pub fn open_panel(&mut self, p: Panel) {
        self.settings.layout.open(p);
    }

    pub fn panel_shown(&self, p: Panel) -> bool {
        self.settings.layout.shown(p)
    }

    /// GPU layer of an image texture (loaded on first use).
    pub fn texture_layer(&mut self, path: &str) -> Option<u8> {
        if path.is_empty() || self.tex_failed.contains(path) {
            return None;
        }
        if let Some(i) = self.tex_layers.iter().position(|t| t.0 == path) {
            return Some(i as u8);
        }
        if self.tex_layers.len() >= crate::materials::MAX_TEXTURES {
            self.error("Too many image textures (max 64).");
            self.tex_failed.insert(path.to_string());
            return None;
        }
        // cauta si langa proiect daca fisierul a fost mutat
        let mut real = path.to_string();
        if !std::path::Path::new(&real).exists() {
            if let Some(d) = self.project_path.as_ref().and_then(|p| std::path::Path::new(p).parent().map(|d| d.to_path_buf())) {
                let alt = d.join(crate::file_name(path));
                if alt.exists() {
                    real = alt.to_string_lossy().to_string();
                }
            }
        }
        match crate::materials::load_texture(&real) {
            Ok(px) => {
                self.tex_layers.push((path.to_string(), px));
                let all: Vec<Vec<u8>> = self.tex_layers.iter().map(|t| t.1.clone()).collect();
                self.renderer.lock().unwrap().pending.push(Upload::Textures(all));
                Some((self.tex_layers.len() - 1) as u8)
            }
            Err(e) => {
                self.error(e);
                self.tex_failed.insert(path.to_string());
                None
            }
        }
    }

    /// One material per image file (named after the file).
    pub fn import_texture_materials(&mut self, paths: &[String]) {
        let mut n = 0;
        for p in paths {
            if crate::materials::load_texture(p).is_err() {
                self.error(format!("Not a PNG / JPG image: {}", crate::file_name(p)));
                continue;
            }
            let name = std::path::Path::new(p).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Texture".into());
            let mut m = crate::materials::library()[4].clone();
            m.name = name;
            m.color = [255, 255, 255];
            m.texture = p.clone();
            m.scale = 1.0;
            self.materials.push(m);
            n += 1;
        }
        if n > 0 {
            self.mat_sel = self.materials.len() - 1;
            self.toast(format!("{n} texture materials added - set the real size of the image in \"Texture size\"."));
        }
    }

    pub fn materials_changed(&mut self) {
        self.mat_cache = None;
        self.colors_dirty = true;
    }

    pub fn assign_material(&mut self, target: MatTarget, material: &str) {
        // aceeasi tinta: inlocuieste materialul
        if let Some(a) = self.mat_assign.iter_mut().find(|a| a.target == target) {
            a.material = material.to_string();
        } else {
            self.mat_assign.push(MatAssign { target, material: material.to_string(), enabled: true });
        }
        self.materials_on = true;
        self.materials_changed();
    }

    pub fn auto_materials(&mut self) {
        let mut n = 0;
        for (kw, m) in crate::materials::AUTO_RULES {
            let present = self.cache_types.iter().any(|t| t.to_lowercase().contains(&kw.to_lowercase()));
            if present && self.materials.iter().any(|x| x.name == m) && !self.mat_assign.iter().any(|a| a.target == MatTarget::Type(kw.to_string())) {
                self.mat_assign.insert(n, MatAssign { target: MatTarget::Type(kw.to_string()), material: m.to_string(), enabled: true });
                n += 1;
            }
        }
        self.materials_on = true;
        self.materials_changed();
        self.toast(format!("{n} material rules added by IFC type (selection / set rules below them still win)."));
    }

    pub fn rebuild_colors(&mut self) {
        if !self.colors_dirty {
            return;
        }
        self.colors_dirty = false;
        if self.materials_on {
            self.material_map(); // umple mat_cache daca e nevoie; mai jos doar il imprumutam
        }
        let profile_on = self.profile.active && !self.profile.rules.is_empty();
        if profile_on {
            self.profile_map();
        }
        let mat_gpu: Vec<[u8; 4]> = (0..self.materials.len())
            .map(|i| {
                let t = self.materials[i].texture.clone();
                let layer = if t.is_empty() { None } else { self.texture_layer(&t) };
                self.materials[i].gpu(layer)
            })
            .collect();
        let roles = self.clash_roles();
        let ghost = !roles.is_empty();
        let others = self.clash_others;
        let sim = self.sim_view();
        let empty = HashMap::new();
        let empty_p: HashMap<ElemId, u16> = HashMap::new();
        let matmap = if self.materials_on { self.mat_cache.as_ref().unwrap_or(&empty) } else { &empty };
        let (hidden_set, overrides, sim_cache, selected, materials) = (&self.hidden, &self.overrides, &self.sim_cache, &self.selected, &self.materials);
        let (pmap, prules, punmatched) = (self.profile_map.as_ref().unwrap_or(&empty_p), &self.profile.rules, self.profile.unmatched);
        let cmp = self.compare.as_ref().filter(|_| self.compare_on);
        // aspectul unui element: override manual > profil > material; simularea 4D si clash-ul au prioritate
        let look_of = |id: ElemId| -> Look {
            let mut hidden = hidden_set.contains(&id);
            let mut ovr: Option<[u8; 4]> = overrides.get(&id).copied();
            if profile_on && ovr.is_none() {
                match pmap.get(&id).and_then(|r| prules.get(*r as usize)) {
                    Some(r) if r.visible => ovr = Some(r.color),
                    Some(_) => hidden = true,
                    None => match punmatched {
                        crate::profiler::Unmatched::Keep => {}
                        crate::profiler::Unmatched::Ghost => ovr = Some(crate::profiler::GHOST),
                        crate::profiler::Unmatched::Hide => hidden = true,
                    },
                }
            }
            // Compare: culoarea dupa tipul schimbarii; perechea veche (A) a unui element potrivit se ascunde
            if let Some(c) = cmp {
                if c.matched_a.contains_key(&id) {
                    hidden = true;
                } else {
                    ovr = Some(c.map.get(&id).map(|d| d.color()).unwrap_or(crate::profiler::GHOST));
                }
            }
            if sim {
                match sim_cache.get(&id) {
                    Some(SimState::Hidden) => hidden = true,
                    Some(SimState::Highlight(c)) => ovr = Some(*c),
                    _ => {}
                }
            }
            if ghost {
                match roles.get(&id) {
                    Some(1) => ovr = Some([230, 45, 40, 255]),
                    Some(2) => ovr = Some([40, 200, 70, 255]),
                    Some(3) => ovr = Some([235, 140, 120, 235]),
                    Some(4) => ovr = Some([140, 215, 150, 235]),
                    _ => match others {
                        Others::Dim => {
                            if !hidden {
                                ovr = Some([150, 150, 155, 28]);
                            }
                        }
                        Others::Hide => hidden = true,
                        Others::Normal => {}
                    },
                }
            }
            let sel = selected.contains(&id) && !ghost;
            let (col, gpu) = match (ovr, matmap.get(&id)) {
                (None, Some(k)) => (Some(materials[*k as usize].rgba()), mat_gpu[*k as usize]),
                _ => (ovr, [0u8; 4]),
            };
            Look { col, gpu, sel, hidden }
        };

        let mut ups = Vec::new();
        let mut new_looks: HashMap<usize, Vec<Look>> = HashMap::new();
        for (mi, m) in self.loaded_models() {
            let looks: Vec<Look> = (0..m.elements.len() as u32).map(|e| look_of(ElemId { m: mi as u32, e })).collect();
            // culorile de pe GPU corespund ultimelor "looks" trimise; None = trebuie trimis tot
            let prev = self.looks.get(&mi).filter(|p| p.len() == looks.len());
            let changed: Vec<usize> = match prev {
                Some(p) => (0..looks.len()).filter(|&i| p[i] != looks[i]).collect(),
                None => (0..looks.len()).collect(),
            };
            let total_tris = m.tri_color.len();
            let changed_tris: usize = changed.iter().map(|&i| m.elements[i].tri_count as usize).sum();
            if prev.is_none() || changed_tris * 3 > total_tris {
                // multe schimbari (sau prima data): tot bufferul, umplut pe toate nucleele
                ups.push(full_colors(mi as u32, m, &looks));
            } else {
                // putine schimbari (ex. un click de selectie): doar elementele afectate, grupate in intervale continue
                let mut k = 0;
                while k < changed.len() {
                    let mut j = k;
                    while j + 1 < changed.len() && changed[j + 1] == changed[j] + 1 {
                        j += 1;
                    }
                    let (a, b) = (&m.elements[changed[k]], &m.elements[changed[j]]);
                    let (t0, t1) = (a.tri_start as usize, (b.tri_start + b.tri_count) as usize);
                    let mut col = vec![0u8; (t1 - t0) * 12];
                    let mut mat = vec![0u8; (t1 - t0) * 12];
                    for &ei in &changed[k..=j] {
                        let e = &m.elements[ei];
                        let (s, n) = (e.tri_start as usize - t0, e.tri_count as usize);
                        fill_look(&looks[ei], &m.tri_color[e.tri_start as usize..e.tri_start as usize + n], &mut col[s * 12..(s + n) * 12], &mut mat[s * 12..(s + n) * 12]);
                    }
                    ups.push(Upload::ColorRange { slot: mi as u32, first: (t0 * 3) as u32, col, mat });
                    k = j + 1;
                }
            }
            new_looks.insert(mi, looks);
        }
        self.looks = new_looks;
        if !ups.is_empty() {
            self.renderer.lock().unwrap().pending.extend(ups);
        }
    }

    // ------------------------------------------------------------------ picking
    pub fn pick(&self, o: Vec3, d: Vec3) -> Option<(ElemId, f32, Vec3, [Vec3; 3])> {
        let planes = self.section.gpu_planes();
        let accept = |p: Vec3| planes.iter().all(|pl| pl.truncate().dot(p) <= pl.w + 1e-4);
        let inv = Vec3::new(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
        let mut best: Option<(ElemId, f32, Vec3, [Vec3; 3])> = None;
        // in Clash Detective (dim / hide other) only the clashing items can be picked
        let roles = if self.clash_others != Others::Normal { Some(self.clash_roles()).filter(|r| !r.is_empty()) } else { None };
        for (mi, m) in self.loaded_models() {
            if !m.aabb.ray(o, inv, f32::INFINITY) {
                continue;
            }
            for (ei, e) in m.elements.iter().enumerate() {
                let tmax = best.as_ref().map_or(f32::INFINITY, |b| b.1);
                if !e.aabb.ray(o, inv, tmax) {
                    continue;
                }
                let id = ElemId { m: mi as u32, e: ei as u32 };
                if !self.is_visible(id) {
                    continue;
                }
                if let Some(r) = roles.as_ref() {
                    if !r.contains_key(&id) {
                        continue;
                    }
                }
                let tris = m.tris(e);
                if let Some((t, ti)) = e.bvh.raycast(tris, o, d, tmax, &accept) {
                    let k = ti as usize * 3;
                    best = Some((id, t, o + d * t, [tris[k], tris[k + 1], tris[k + 2]]));
                }
            }
        }
        best
    }

    /// Ce se deseneaza in cadrul curent, pe model: elementele din campul vizual, neascunse, in section box;
    /// in timpul miscarii camerei se sar elementele mai mici de ~2 pixeli (cand camera sta, se deseneaza tot).
    /// Pasul transparent primeste doar elementele cu ceva transparent. Intervalele apropiate se unesc
    /// (varfurile in plus sunt oricum taiate / ascunse de shader), deci rezultatul vizual nu se schimba.
    pub fn draw_lists(&mut self, p: &FrameParams, vp_h: f32) -> HashMap<u32, crate::render::DrawList> {
        // miscarea camerei (pentru simplificarea temporara)
        let c = self.cam;
        let moved = c.eye != self.lod_cam.eye || c.yaw != self.lod_cam.yaw || c.pitch != self.lod_cam.pitch || c.focus != self.lod_cam.focus || c.fov_deg != self.lod_cam.fov_deg;
        if moved {
            self.lod_cam = c;
            self.lod_moved = Instant::now();
        }
        let moving = self.lod_moved.elapsed().as_secs_f32() < 0.3;
        // planurile frustumului (Gribb-Hartmann) din matricea vedere-proiectie
        let m = p.view_proj;
        let (r0, r1, r2, r3) = (m.row(0), m.row(1), m.row(2), m.row(3));
        let planes = [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r3 + r2, r3 - r2];
        let inside = |bb: &Aabb| -> bool {
            planes.iter().all(|pl| {
                let n = pl.truncate();
                let v = Vec3::new(if n.x >= 0.0 { bb.max.x } else { bb.min.x }, if n.y >= 0.0 { bb.max.y } else { bb.min.y }, if n.z >= 0.0 { bb.max.z } else { bb.min.z });
                n.dot(v) + pl.w >= 0.0
            })
        };
        let section_box = (self.section.enabled && self.section.mode == SectionMode::Box).then(|| Aabb { min: Vec3::from(self.section.bmin), max: Vec3::from(self.section.bmax) });
        let tanh = (c.fov() * 0.5).tan().max(1e-3);
        let min_px = if moving { 2.0 } else { 0.0 };
        let eye = c.eye();
        let mut out = HashMap::new();
        let slots: Vec<usize> = self.loaded_models().map(|(i, _)| i).collect();
        for slot in slots {
            let Some(model) = self.models[slot].clone() else { continue };
            // elementele cu triunghiuri transparente in culorile din fisier (calculat o data per model)
            let key = Arc::as_ptr(&model) as usize;
            let need = self.alpha_cache.get(&slot).map(|(k, _)| *k != key).unwrap_or(true);
            if need {
                let flags: Vec<bool> = model.elements.iter().map(|e| model.tri_color[e.tri_start as usize..(e.tri_start + e.tri_count) as usize].iter().any(|c| c[3] < 255)).collect();
                self.alpha_cache.insert(slot, (key, flags));
            }
            let base_alpha = &self.alpha_cache[&slot].1;
            let looks = self.looks.get(&slot);
            let mut all: Vec<(i32, i32)> = Vec::new();
            let mut transp: Vec<(i32, i32)> = Vec::new();
            let push = |v: &mut Vec<(i32, i32)>, first: i32, count: i32| match v.last_mut() {
                Some(l) if first - (l.0 + l.1) <= 600 => l.1 = first + count - l.0,
                _ => v.push((first, count)),
            };
            for (ei, e) in model.elements.iter().enumerate() {
                if e.tri_count == 0 {
                    continue;
                }
                let look = looks.and_then(|l| l.get(ei));
                if look.map(|l| l.hidden).unwrap_or(false) {
                    continue;
                }
                let bb = &e.aabb;
                if !bb.is_valid() || !inside(bb) {
                    continue;
                }
                if let Some(sb) = &section_box {
                    if !bb.overlaps(sb, 0.0) {
                        continue;
                    }
                }
                if min_px > 0.0 {
                    let r = bb.size().length() * 0.5;
                    let px = if c.ortho { r / c.ortho_half().max(1e-3) * vp_h * 0.5 } else { r / ((bb.center() - eye).length().max(1e-3) * tanh) * vp_h * 0.5 };
                    if px < min_px {
                        continue;
                    }
                }
                let (first, count) = (e.tri_start as i32 * 3, e.tri_count as i32 * 3);
                push(&mut all, first, count);
                let t = match look.and_then(|l| l.col) {
                    Some(col) => col[3] < 255,
                    None => base_alpha.get(ei).copied().unwrap_or(false),
                };
                if t {
                    push(&mut transp, first, count);
                }
            }
            // prea multe intervale -> le unim mai larg (apelurile de desenare costa mai mult decat varfurile in plus)
            for v in [&mut all, &mut transp] {
                let mut gap = 4000;
                while v.len() > 3000 {
                    let mut merged: Vec<(i32, i32)> = Vec::with_capacity(v.len() / 2);
                    for &(f, n) in v.iter() {
                        match merged.last_mut() {
                            Some(l) if f - (l.0 + l.1) <= gap => l.1 = f + n - l.0,
                            _ => merged.push((f, n)),
                        }
                    }
                    *v = merged;
                    gap *= 4;
                }
            }
            // nor de puncte: scanarile vizibile, primele N% puncte (amestecate la citire); in miscare mai putine
            let mut points = Vec::new();
            if let Some(cl) = &model.cloud {
                let dens = self.cloud_density.clamp(0.01, 1.0) * if moving { 0.35 } else { 1.0 };
                for &(ei, start, n) in &cl.scans {
                    if looks.and_then(|l| l.get(ei as usize)).map(|l| l.hidden).unwrap_or(false) {
                        continue;
                    }
                    if let Some(e) = model.elements.get(ei as usize) {
                        if e.aabb.is_valid() && (!inside(&e.aabb) || section_box.as_ref().map_or(false, |sb| !e.aabb.overlaps(sb, 0.0))) {
                            continue;
                        }
                    }
                    let k = ((n as f32 * dens).ceil() as i32).clamp(0, n as i32);
                    if k > 0 {
                        points.push((start as i32, k));
                    }
                }
            }
            out.insert(slot as u32, crate::render::DrawList { all, transp, points });
        }
        out
    }

    /// camera se misca acum (desenarea simplificata e activa) - pentru redesenarea completa dupa oprire
    pub fn lod_moving(&self) -> bool {
        self.lod_moved.elapsed().as_secs_f32() < 0.35
    }

    pub fn frame_params(&self, aspect: f32) -> FrameParams {
        let bb = self.scene_aabb();
        let proj = self.cam.proj(aspect, &bb);
        let vp = proj * self.cam.view();
        let pl = self.section.gpu_planes();
        let mut planes = [Vec4::ZERO; 6];
        for (i, p) in pl.iter().take(6).enumerate() {
            planes[i] = *p;
        }
        FrameParams { view_proj: vp, eye: self.cam.eye(), planes, nplanes: pl.len().min(6) as i32, sun: self.render.sun_dir(), light: self.render.light(), point_size: self.point_size, caps: self.section_caps, ao: self.render.ao, depth_k: [proj.z_axis.z, proj.w_axis.z, if self.cam.ortho { 1.0 } else { 0.0 }, proj.y_axis.y] }
    }

    // ------------------------------------------------------------------ viewpoints
    pub fn save_viewpoint(&mut self) {
        let n = self.viewpoints.len() + 1;
        let mut hidden: Vec<ElemId> = self.hidden.iter().copied().collect();
        hidden.sort();
        let selected: Vec<ElemId> = self.selected.iter().copied().collect();
        self.viewpoints.push(Viewpoint {
            name: format!("Viewpoint {n}"),
            cam: self.cam,
            section: self.section.clone(),
            hidden,
            comment: String::new(),
            redlines: Vec::new(),
            issue: Default::default(),
            comments: Vec::new(),
            selected,
            measures: self.measures.clone(),
        });
        self.active_vp = Some(self.viewpoints.len() - 1);
        self.vp_sel = self.viewpoints.len() - 1;
    }

    /// redlines are shown only while the camera is still at the viewpoint they belong to
    pub fn vp_matches(&self, i: usize) -> bool {
        match self.viewpoints.get(i) {
            Some(v) => {
                let a = &v.cam;
                let b = &self.cam;
                (a.eye() - b.eye()).length() < 1e-3 * (1.0 + a.focus) && (a.yaw - b.yaw).abs() < 1e-4 && (a.pitch - b.pitch).abs() < 1e-4 && a.ortho == b.ortho && (a.focus - b.focus).abs() < 1e-3 * (1.0 + a.focus)
            }
            None => false,
        }
    }

    pub fn guid_of(&self, id: ElemId) -> Option<String> {
        self.elem(id).map(|(_, e)| e.guid.clone()).filter(|g| g.len() == 22)
    }

    pub fn report_item(&self, i: usize, image: Vec<u8>, w: u32, h: u32) -> crate::markup::ReportItem {
        let v = &self.viewpoints[i];
        let c = v.cam;
        let eye = self.world(c.eye());
        let d = c.fwd();
        let u = c.up();
        crate::markup::ReportItem {
            index: i + 1,
            name: v.name.clone(),
            description: v.comment.clone(),
            issue: v.issue.clone(),
            comments: v.comments.clone(),
            redlines: v.redlines.len(),
            eye: eye.to_array(),
            dir: [d.x as f64, d.y as f64, d.z as f64],
            up: [u.x as f64, u.y as f64, u.z as f64],
            fov_deg: c.fov_deg,
            ortho_height: if c.ortho { Some(2.0 * c.ortho_half() as f64) } else { None },
            selected_guids: v.selected.iter().filter_map(|id| self.guid_of(*id)).collect(),
            hidden_guids: v.hidden.iter().filter_map(|id| self.guid_of(*id)).take(5000).collect(),
            image,
            image_w: w,
            image_h: h,
        }
    }

    pub fn start_report(&mut self, kind: u8, path: &str) {
        if self.viewpoints.is_empty() {
            self.error("No viewpoints to export - save a viewpoint (with markups) first.");
            return;
        }
        let hidden = self.hidden.clone();
        self.report_job = Some(ReportJob { kind, path: path.to_string(), idx: 0, phase: 0, wait: 0, items: Vec::new(), saved_cam: self.cam, saved_section: self.section.clone(), saved_hidden: hidden, saved_vp: self.active_vp });
        self.toast("Capturing viewpoints for the report…");
    }

    pub fn finish_report(&mut self) {
        let Some(job) = self.report_job.take() else { return };
        self.cam = job.saved_cam;
        self.section = job.saved_section.clone();
        self.hidden = job.saved_hidden.clone();
        self.active_vp = job.saved_vp;
        self.colors_dirty = true;
        let project = self.project_path.as_ref().map(|p| crate::file_name(p)).unwrap_or_else(|| "ManageBIM project".into());
        let mut path = job.path.clone();
        let ext = match job.kind {
            0 => ".html",
            1 => ".xlsx",
            _ => ".bcf",
        };
        if !path.to_lowercase().ends_with(ext) && !(job.kind == 2 && path.to_lowercase().ends_with(".bcfzip")) {
            path.push_str(ext);
        }
        let data: Vec<u8> = match job.kind {
            0 => crate::markup::html_report(&project, &job.items).into_bytes(),
            1 => {
                let thumbs: Vec<(Vec<u8>, u32, u32)> = job
                    .items
                    .iter()
                    .map(|it| match png_rgb(&it.image) {
                        Some((rgb, w, h)) => {
                            let (t, tw, th) = crate::markup::downscale_rgb(&rgb, w, h, 240);
                            (crate::markup::encode_png(&t, tw, th), tw, th)
                        }
                        None => (Vec::new(), 0, 0),
                    })
                    .collect();
                crate::markup::xlsx_report(&project, &job.items, &thumbs)
            }
            _ => crate::markup::bcf_zip(&project, &job.items),
        };
        match std::fs::write(&path, data) {
            Ok(()) => self.toast(format!("{} viewpoints exported to {}", job.items.len(), crate::file_name(&path))),
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }

    /// Unitatea in care scriem XML-ul Navisworks: din setari, sau (auto) unitatea documentului Navisworks din care
    /// vine modelul ("Source units" in .bimc-ul exportat de add-in), altfel metri.
    pub fn navis_units(&self) -> String {
        let s = self.settings.navis_units.trim().to_lowercase();
        if s != "auto" && !s.is_empty() && crate::markup::NW_UNITS.iter().any(|u| u.0 == s) {
            return s;
        }
        for (_, m) in self.loaded_models() {
            if let Some((_, v)) = m.info.iter().find(|(k, _)| k == "Source units") {
                if let Some(u) = crate::markup::nw_unit_code(v) {
                    return u.to_string();
                }
            }
        }
        "m".into()
    }

    /// Viewpoints + markups + comentarii + section box -> XML Navisworks (Viewpoints > Import Viewpoints).
    /// Nu are nevoie de capturi de ecran: se scrie direct.
    pub fn export_navis_xml(&mut self, path: &str) {
        if self.viewpoints.is_empty() {
            self.error("No viewpoints to export - save a viewpoint (with markups) first.");
            return;
        }
        let mut path = path.to_string();
        if !path.to_lowercase().ends_with(".xml") {
            path.push_str(".xml");
        }
        let bb = self.scene_aabb();
        let (smin, smax) = if bb.is_valid() { (self.world(bb.min).to_array(), self.world(bb.max).to_array()) } else { ([-1.0; 3], [1.0; 3]) };
        let aspect = if self.vp_rect[3] > 1.0 { self.vp_rect[2] / self.vp_rect[3] } else { 16.0 / 9.0 };
        let views: Vec<crate::markup::NwView> = self
            .viewpoints
            .iter()
            .map(|v| {
                let c = v.cam;
                let (near, far) = c.clip(&bb);
                let st = crate::markup::nw_status(&v.issue.status);
                let mut comments = Vec::new();
                if !v.comment.trim().is_empty() {
                    comments.push((v.issue.author.clone(), v.comment.clone(), v.issue.created.clone(), st));
                }
                let mut issue = format!("Issue: {} | Priority: {}", v.issue.status, v.issue.priority);
                if !v.issue.assigned.is_empty() {
                    issue.push_str(&format!(" | Assigned to: {}", v.issue.assigned));
                }
                comments.push((v.issue.author.clone(), issue, v.issue.created.clone(), st));
                for k in &v.comments {
                    comments.push((k.author.clone(), k.text.clone(), k.date.clone(), st));
                }
                let section_box = (v.section.enabled && v.section.mode == SectionMode::Box)
                    .then(|| (self.world(Vec3::from(v.section.bmin)).to_array(), self.world(Vec3::from(v.section.bmax)).to_array()));
                crate::markup::NwView {
                    name: v.name.clone(),
                    guid: v.issue.guid.clone(),
                    eye: self.world(c.eye()).to_array(),
                    fwd: c.fwd().as_dvec3().to_array(),
                    up: c.up().as_dvec3().to_array(),
                    fov_deg: c.fov_deg,
                    ortho_height: c.ortho.then(|| 2.0 * c.ortho_half() as f64),
                    aspect,
                    focal: c.focus as f64,
                    near: near as f64,
                    far: far as f64,
                    scene_min: smin,
                    scene_max: smax,
                    section_box,
                    comments,
                    redlines: v.redlines.clone(),
                }
            })
            .collect();
        let n_red: usize = views.iter().map(|v| v.redlines.len()).sum();
        let units = self.navis_units();
        match std::fs::write(&path, crate::markup::navis_viewpoints_xml(&views, &units)) {
            Ok(()) => self.toast(format!("{} viewpoints ({n_red} markups) exported for Navisworks in {units}: {} - in Navisworks: Saved Viewpoints > right-click > Import Viewpoints.", views.len(), crate::file_name(&path))),
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }

    /// Seturile ca `<selectionsets>` Navisworks: search sets -> conditii findspec (Item / Name, Type, GUID,
    /// sau categoria / proprietatea din model), seturile explicite -> Item / GUID = ... (SAU intre ele).
    /// Intoarce (xml, avertismente).
    pub fn navis_selectionsets_xml(&self) -> (String, Vec<String>) {
        use crate::markup::esc;
        let mut warn = Vec::new();
        let cond = |cat: &str, prop: &str, test: &str, value: &str, or: bool| -> String {
            // nume interne Navisworks pentru categoria Item (restul se potrivesc dupa numele afisat)
            let (ci, pi) = match (cat, prop) {
                ("Item", "Name") => (" internal=\"LcOaNode\"", " internal=\"LcOaSceneBaseUserName\""),
                ("Item", "Type") => (" internal=\"LcOaNode\"", " internal=\"LcOaSceneBaseClassUserName\""),
                ("Item", "GUID") => (" internal=\"LcOaNode\"", " internal=\"LcOaNodeGuid\""),
                ("Item", "Layer") => (" internal=\"LcOaNode\"", " internal=\"LcOaNodeLayer\""),
                ("Item", "Source File") => (" internal=\"LcOaNode\"", " internal=\"LcOaNodeSourceFile\""),
                _ => ("", ""),
            };
            let flags = if or { 74 } else { 10 };
            let val = if test == "prop" || test == "no_prop" { String::new() } else { format!("<data type=\"wstring\">{}</data>", esc(value)) };
            format!("<condition test=\"{test}\" flags=\"{flags}\"><category><name{ci}>{}</name></category><property><name{pi}>{}</name></property><value>{val}</value></condition>", esc(cat), esc(prop))
        };
        let mut s = String::from("<selectionsets>\n");
        for set in &self.sets {
            let mut conds = String::new();
            match &set.kind {
                SetKind::Search(rules) => {
                    for (i, r) in rules.iter().enumerate() {
                        let test = match r.op {
                            SearchOp::Equals => "equals",
                            SearchOp::NotEquals => "not_equals",
                            SearchOp::Contains => "contains",
                            SearchOp::Greater => "greater_than",
                            SearchOp::Less => "less_than",
                            SearchOp::Defined => "prop",
                            SearchOp::Undefined => "no_prop",
                            SearchOp::NotContains => {
                                warn.push(format!("{}: 'does not contain' has no Navisworks equivalent - condition skipped", set.name));
                                continue;
                            }
                        };
                        // campurile ManageBIM -> Navisworks
                        let (cat, prop) = match (r.group.as_str(), r.field.as_str()) {
                            ("", "Name") | (ITEM_CAT, "Name") => ("Item", "Name"),
                            ("", "IFC Type") | (ITEM_CAT, "IFC Type") => ("Item", "Type"),
                            ("", "GlobalId") | (ITEM_CAT, "GlobalId") => ("Item", "GUID"),
                            ("", "Level") | (ITEM_CAT, "Level") => ("Item", "Layer"),
                            ("", "Model") | (ITEM_CAT, "Model") => ("Item", "Source File"),
                            ("", "Any") => {
                                warn.push(format!("{}: 'Any property' search exported as Item / Name", set.name));
                                ("Item", "Name")
                            }
                            (g, f) => (g, f),
                        };
                        conds.push_str(&cond(cat, prop, test, &r.value, r.or && i > 0));
                    }
                }
                SetKind::Explicit(ids) => {
                    let guids: Vec<String> = ids.iter().filter_map(|id| self.elem(*id).map(|(_, e)| e.guid.clone())).filter(|g| !g.is_empty()).collect();
                    if guids.len() > 2000 {
                        warn.push(format!("{}: {} elements - only the first 2000 exported", set.name, guids.len()));
                    }
                    for (i, g) in guids.iter().take(2000).enumerate() {
                        // GUID-urile din .bimc / NWC (Navisworks) se potrivesc cu Item / GUID
                        let g = g.rsplit(':').next().unwrap_or(g);
                        conds.push_str(&cond("Item", "GUID", "equals", g, i > 0));
                    }
                }
            }
            s.push_str(&format!(
                "<selectionset name=\"{}\" guid=\"{}\"><findspec mode=\"all\" disjoint=\"0\"><conditions>{conds}</conditions><locator>/</locator></findspec></selectionset>\n",
                esc(&set.name),
                crate::markup::new_guid()
            ));
        }
        s.push_str("</selectionsets>\n");
        (s, warn)
    }

    const NW_HEAD: &'static str = "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<exchange xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:noNamespaceSchemaLocation=\"http://download.autodesk.com/us/navisworks/schemas/nw-exchange-12.0.xsd\" units=\"m\" filename=\"\" filepath=\"\">\n";

    /// Seturile de selectie -> XML Navisworks (Sets > Import Search Sets).
    pub fn export_navis_sets(&mut self, path: &str) {
        if self.sets.is_empty() {
            return self.error("No selection or search sets to export.");
        }
        let (sets, warn) = self.navis_selectionsets_xml();
        let path = if path.to_lowercase().ends_with(".xml") { path.to_string() } else { format!("{path}.xml") };
        match std::fs::write(&path, format!("{}{sets}</exchange>\n", Self::NW_HEAD)) {
            Ok(()) => {
                self.toast(format!("{} sets exported for Navisworks: {} - in Navisworks: Sets > Import Search Sets.", self.sets.len(), crate::file_name(&path)));
                for w in warn.iter().take(5) {
                    self.toast(w.clone());
                }
            }
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }

    /// Testele de clash (+ seturile folosite) -> XML Navisworks (Clash Detective > Import Clash Tests).
    pub fn export_navis_clash(&mut self, path: &str) {
        use crate::markup::esc;
        if self.clash_tests.is_empty() {
            return self.error("No clash tests to export.");
        }
        let locator = |s: &Selector| -> String {
            match s.kind {
                SelKind::Set => format!("lcop_selection_set_tree/{}", s.value),
                SelKind::Model => format!("/{}", s.value),
                _ => "/".to_string(),
            }
        };
        let mut tests = String::new();
        for t in &self.clash_tests {
            let ty = match t.ty {
                ClashType::Hard => "hard",
                ClashType::Clearance => "clearance",
                ClashType::Duplicates => "duplicate",
            };
            let side = |tag: &str, s: &Selector| format!("<{tag}><clashselection selfintersect=\"0\" primtypes=\"1\"><locator>{}</locator></clashselection></{tag}>", esc(&locator(s)));
            tests.push_str(&format!(
                "<clashtest name=\"{}\" guid=\"{}\" test_type=\"{ty}\" status=\"new\" tolerance=\"{:.6}\" merge_composites=\"0\"><linkage mode=\"none\"/>{}{}</clashtest>\n",
                esc(&t.name),
                crate::markup::new_guid(),
                t.tol_mm as f64 / 1000.0,
                side("left", &t.a),
                side("right", &t.b)
            ));
        }
        let (sets, _) = self.navis_selectionsets_xml();
        let path = if path.to_lowercase().ends_with(".xml") { path.to_string() } else { format!("{path}.xml") };
        let x = format!("{}<batchtest name=\"ManageBIM\" internal_name=\"ManageBIM\" units=\"m\"><clashtests>\n{tests}</clashtests>\n{sets}</batchtest>\n</exchange>\n", Self::NW_HEAD);
        match std::fs::write(&path, x) {
            Ok(()) => self.toast(format!("{} clash tests exported for Navisworks: {} - in Navisworks: Clash Detective > Import / Export > Import Clash Tests.", self.clash_tests.len(), crate::file_name(&path))),
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }

    /// Rezultatele unui test de clash ca "vederi": camera izometrica pe clash, cutia celor doua elemente
    /// (pentru section box), GUID-urile lor si descrierea. Coordonate WORLD, metri.
    pub fn clash_views(&self, ti: usize) -> Vec<(String, String, [f64; 3], [f64; 3], [f64; 3], ([f64; 3], [f64; 3]), Vec<String>, String)> {
        let Some(t) = self.clash_tests.get(ti) else { return Vec::new() };
        t.results
            .iter()
            .filter(|r| r.status != ClashStatus::Resolved && r.status != ClashStatus::Approved)
            .map(|r| {
                let bb = self.ids_aabb(&[r.a, r.b]);
                let p = Vec3::from(r.point);
                let size = if bb.is_valid() { bb.size().length() } else { 2.0 };
                let mut c = self.cam;
                c.yaw = std::f32::consts::FRAC_PI_4;
                c.pitch = -0.5;
                c.ortho = false;
                let dist = (size * 0.9).clamp(1.5, 60.0) / (c.fov().max(0.3) * 0.5).tan();
                let eye = p - c.fwd() * dist;
                let pad = Vec3::splat((size * 0.15).max(0.3));
                let (mn, mx) = if bb.is_valid() { (bb.min - pad, bb.max + pad) } else { (p - Vec3::splat(1.0), p + Vec3::splat(1.0)) };
                let na = self.elem(r.a).map(|(_, e)| e.name.clone()).unwrap_or_default();
                let nb = self.elem(r.b).map(|(_, e)| e.name.clone()).unwrap_or_default();
                let mut desc = format!("{} | {} <-> {} | {} {}", t.name, na, nb, if r.dist < 0.0 { "penetration" } else { "distance" }, fmt_len(r.dist.abs() as f64));
                if !r.comment.is_empty() {
                    desc.push_str(&format!(" | {}", r.comment));
                }
                // GUID-ul real al elementului (r.ga / r.gb sunt "model|guid", interne testului de clash)
                let guids = [r.a, r.b].iter().filter_map(|id| self.elem(*id).map(|(_, e)| e.guid.clone())).filter(|g| !g.is_empty()).collect();
                (
                    r.name.clone(),
                    format!("{:?}", r.status),
                    self.world(eye).to_array(),
                    c.fwd().as_dvec3().to_array(),
                    c.up().as_dvec3().to_array(),
                    (self.world(mn).to_array(), self.world(mx).to_array()),
                    guids,
                    desc,
                )
            })
            .collect()
    }

    /// Clash-urile testului curent -> viewpoints Navisworks (camera + section box + comentariu), importabile
    /// cu Saved Viewpoints > Import Viewpoints in orice model Navisworks cu aceleasi coordonate.
    pub fn export_clash_navis(&mut self, path: &str) {
        let views = self.clash_views(self.cur_test);
        if views.is_empty() {
            return self.error("No open clash results in the current test - run the test first.");
        }
        let bb = self.scene_aabb();
        let (smin, smax) = if bb.is_valid() { (self.world(bb.min).to_array(), self.world(bb.max).to_array()) } else { ([-1.0; 3], [1.0; 3]) };
        let aspect = if self.vp_rect[3] > 1.0 { self.vp_rect[2] / self.vp_rect[3] } else { 16.0 / 9.0 };
        let user = crate::markup::user_name();
        let now = crate::markup::now_iso();
        let nv: Vec<crate::markup::NwView> = views
            .into_iter()
            .map(|(name, status, eye, fwd, up, sbox, _, desc)| crate::markup::NwView {
                name,
                guid: crate::markup::new_guid(),
                eye,
                fwd,
                up,
                fov_deg: self.cam.fov_deg,
                ortho_height: None,
                aspect,
                focal: 5.0,
                near: 0.05,
                far: 2000.0,
                scene_min: smin,
                scene_max: smax,
                section_box: Some(sbox),
                comments: vec![(user.clone(), desc, now.clone(), if status == "New" { "new" } else { "active" })],
                redlines: Vec::new(),
            })
            .collect();
        let path = if path.to_lowercase().ends_with(".xml") { path.to_string() } else { format!("{path}.xml") };
        let units = self.navis_units();
        match std::fs::write(&path, crate::markup::navis_viewpoints_xml(&nv, &units)) {
            Ok(()) => self.toast(format!("{} clashes exported as Navisworks viewpoints: {} - Saved Viewpoints > right-click > Import Viewpoints.", nv.len(), crate::file_name(&path))),
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }

    /// Export IFC4: selectia, sau (fara selectie) toate elementele vizibile din proiect.
    pub fn export_ifc(&mut self, path: &str) {
        let path = with_ext(path, "ifc");
        let models: Vec<(usize, Arc<Model>)> = self.loaded_models().map(|(i, m)| (i, m.clone())).collect();
        let use_sel = self.ifc_selection && !self.selected.is_empty();
        let mut items: Vec<(&Model, &crate::model::Element)> = Vec::new();
        for (mi, m) in &models {
            for (ei, e) in m.elements.iter().enumerate() {
                let id = ElemId { m: *mi as u32, e: ei as u32 };
                let take = if use_sel { self.selected.contains(&id) } else { !self.hidden.contains(&id) && !e.default_hidden };
                if take && e.tri_count > 0 {
                    items.push((m.as_ref(), e));
                }
            }
        }
        if items.is_empty() {
            return self.error("Nothing to export: no visible items with geometry.");
        }
        let project = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "ManageBIM".into());
        let (text, st) = crate::ifc_export::write_ifc(&items, self.origin_f64(), &project, &now_string());
        match std::fs::write(&path, text) {
            Ok(()) => self.toast(format!(
                "IFC4 exported: {} items ({} closed solids, {} reused geometries), {} properties → {}",
                st.elements,
                st.closed,
                st.instances,
                st.properties,
                crate::file_name(&path)
            )),
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }

    /// Clash-urile testului curent -> BCF 2.1 (Revit, ACC, Solibri, BIMcollab, Navisworks cu BCF): un topic pe clash,
    /// cu camera si cele doua elemente selectate.
    pub fn export_clash_bcf(&mut self, path: &str) {
        let views = self.clash_views(self.cur_test);
        if views.is_empty() {
            return self.error("No open clash results in the current test - run the test first.");
        }
        let user = crate::markup::user_name();
        let now = crate::markup::now_iso();
        let items: Vec<crate::markup::ReportItem> = views
            .into_iter()
            .enumerate()
            .map(|(i, (name, status, eye, fwd, up, _, guids, desc))| crate::markup::ReportItem {
                index: i + 1,
                name: format!("Clash {name}"),
                description: desc,
                issue: crate::markup::Issue {
                    guid: crate::markup::new_guid(),
                    status: if status == "New" { "Open".into() } else { "In Progress".into() },
                    priority: "Normal".into(),
                    assigned: String::new(),
                    author: user.clone(),
                    created: now.clone(),
                },
                comments: Vec::new(),
                redlines: 0,
                eye,
                dir: fwd,
                up,
                fov_deg: self.cam.fov_deg,
                ortho_height: None,
                selected_guids: guids,
                hidden_guids: Vec::new(),
                image: Vec::new(),
                image_w: 0,
                image_h: 0,
            })
            .collect();
        let path = if path.to_lowercase().ends_with(".bcf") || path.to_lowercase().ends_with(".bcfzip") { path.to_string() } else { format!("{path}.bcf") };
        let project = self.project_path.as_ref().map(|p| crate::file_name(p)).unwrap_or_else(|| "ManageBIM clashes".into());
        match std::fs::write(&path, crate::markup::bcf_zip(&project, &items)) {
            Ok(()) => self.toast(format!("{} clashes exported as BCF topics: {}", items.len(), crate::file_name(&path))),
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }

    /// Issue-uri BCF (2.1 / 3.0) din Revit, ACC, Solibri, BIMcollab... -> viewpoints ManageBIM: camera, titlu,
    /// stare, prioritate, responsabil, descriere, comentarii si elementele selectate (dupa GUID).
    pub fn import_bcf(&mut self, path: &str) {
        let topics = match std::fs::read(path).map_err(|e| e.to_string()).and_then(crate::markup::parse_bcf) {
            Ok(t) => t,
            Err(e) => return self.error(format!("{}: {e}", crate::file_name(path))),
        };
        // GUID -> element (GlobalId IFC, Revit UniqueId, GUID Navisworks...; si dupa partea de dupa ':')
        let mut by_guid: HashMap<String, ElemId> = HashMap::new();
        for (mi, m) in self.loaded_models() {
            for (ei, e) in m.elements.iter().enumerate() {
                if !e.guid.is_empty() {
                    let id = ElemId { m: mi as u32, e: ei as u32 };
                    by_guid.insert(e.guid.clone(), id);
                    if let Some(tail) = e.guid.rsplit(':').next() {
                        by_guid.entry(tail.to_string()).or_insert(id);
                    }
                }
            }
        }
        let origin = self.origin_f64();
        let (mut n, mut found, mut wanted) = (0, 0, 0);
        for t in topics {
            let mut cam = self.cam;
            if let Some((e, d, _u, fov, h)) = t.camera {
                let f = DVec3::from_array(d).normalize_or(DVec3::X);
                cam.eye = (DVec3::from_array(e) - origin).as_vec3().into();
                cam.yaw = f.y.atan2(f.x) as f32;
                cam.pitch = (f.z.clamp(-1.0, 1.0).asin() as f32).clamp(-1.55, 1.55);
                match fov {
                    Some(fd) => {
                        cam.ortho = false;
                        cam.fov_deg = fd.clamp(10.0, 120.0);
                        cam.focus = 10.0;
                    }
                    None => {
                        cam.ortho = true;
                        cam.focus = (h as f32 * 0.5 / (cam.fov_deg.to_radians() * 0.5).tan()).max(0.05);
                    }
                }
            }
            wanted += t.selected.len();
            let selected: Vec<ElemId> = t.selected.iter().filter_map(|g| by_guid.get(g).copied()).collect();
            found += selected.len();
            let mut issue = crate::markup::Issue::default();
            issue.guid = t.guid.clone();
            if !t.status.is_empty() {
                issue.status = t.status.clone();
            }
            if !t.priority.is_empty() {
                issue.priority = t.priority.clone();
            }
            issue.assigned = t.assigned.clone();
            if !t.author.is_empty() {
                issue.author = t.author.clone();
            }
            if !t.created.is_empty() {
                issue.created = t.created.clone();
            }
            self.viewpoints.push(Viewpoint {
                name: if t.title.is_empty() { format!("BCF {}", n + 1) } else { t.title },
                cam,
                section: Section::default(),
                hidden: Vec::new(),
                comment: t.description,
                redlines: Vec::new(),
                issue,
                comments: t.comments,
                selected,
                measures: Vec::new(),
            });
            n += 1;
        }
        self.open_panel(crate::layout::Panel::Viewpoints);
        let sel = if wanted > 0 { format!(", {found} of {wanted} referenced elements found in the open models") } else { String::new() };
        self.toast(format!("{n} BCF issues imported from {}{sel}.", crate::file_name(path)));
    }

    /// Viewpoints dintr-un XML Navisworks (Saved Viewpoints > Export Viewpoints, sau exportul ManageBIM):
    /// camera, section box, comentarii si markups se adauga la viewpoints-urile existente.
    pub fn import_navis_xml(&mut self, path: &str) {
        let text = match std::fs::read(path) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(e) => return self.error(format!("Cannot read {path}: {e}")),
        };
        let aspect = if self.vp_rect[3] > 1.0 { self.vp_rect[2] / self.vp_rect[3] } else { 16.0 / 9.0 };
        let views = match crate::markup::parse_navis_viewpoints(&text, aspect) {
            Ok(v) => v,
            Err(e) => return self.error(format!("{}: {e}", crate::file_name(path))),
        };
        let origin = self.origin_f64();
        let local = |p: [f64; 3]| (DVec3::from_array(p) - origin).as_vec3();
        let (mut n, mut nred) = (0, 0);
        for v in views {
            let f = DVec3::from_array(v.fwd).normalize_or(DVec3::X);
            let mut cam = self.cam;
            cam.eye = local(v.eye).into();
            cam.yaw = f.y.atan2(f.x) as f32;
            cam.pitch = (f.z.clamp(-1.0, 1.0).asin() as f32).clamp(-1.55, 1.55);
            cam.fov_deg = v.fov_deg.clamp(10.0, 120.0);
            cam.ortho = v.ortho_height.is_some();
            cam.focus = match v.ortho_height {
                // ortho: inaltimea vederii = 2 * focus * tan(fov / 2)
                Some(h) => (h as f32 * 0.5 / (cam.fov_deg.to_radians() * 0.5).tan()).max(0.05),
                None => (v.focal as f32).max(0.05),
            };
            let mut section = Section::default();
            if let Some((mn, mx)) = v.section_box {
                let (a, b) = (local(mn), local(mx));
                section.enabled = true;
                section.mode = SectionMode::Box;
                section.bmin = a.min(b).to_array();
                section.bmax = a.max(b).to_array();
            }
            let mut issue = crate::markup::Issue::default();
            let mut comment = String::new();
            let mut comments = Vec::new();
            for (user, body, date, st) in &v.comments {
                if let Some(rest) = body.strip_prefix("Issue: ") {
                    // comentariul de stare scris de exportul ManageBIM
                    for part in rest.split(['|', '·']).map(str::trim) {
                        if let Some(p) = part.strip_prefix("Priority: ") {
                            issue.priority = p.to_string();
                        } else if let Some(a) = part.strip_prefix("Assigned to: ") {
                            issue.assigned = a.to_string();
                        } else if !part.contains(':') {
                            issue.status = part.to_string();
                        }
                    }
                    continue;
                }
                if comments.is_empty() && comment.is_empty() {
                    issue.status = match *st {
                        "active" => "In Progress",
                        "resolved" => "Resolved",
                        "approved" => "Closed",
                        _ => "Open",
                    }
                    .into();
                    if !user.is_empty() {
                        issue.author = user.clone();
                    }
                    issue.created = date.clone();
                }
                comments.push(crate::markup::VpComment { author: user.clone(), date: date.clone(), text: body.clone() });
            }
            if let Some(first) = comments.first() {
                if comments.len() == 1 {
                    comment = first.text.clone();
                    comments.clear();
                }
            }
            if !v.guid.is_empty() {
                issue.guid = v.guid.clone();
            }
            nred += v.redlines.len();
            self.viewpoints.push(Viewpoint { name: v.name, cam, section, hidden: Vec::new(), comment, redlines: v.redlines, issue, comments, selected: Vec::new(), measures: Vec::new() });
            n += 1;
        }
        self.open_panel(crate::layout::Panel::Viewpoints);
        let warn = if self.loaded_models().next().is_none() { " Open the model first so the cameras land in the right place." } else { "" };
        self.toast(format!("{n} viewpoints imported from {} ({nred} markups).{warn}", crate::file_name(path)));
    }

    /// smooth camera move (ViewCube, Home, standard views, focus)
    pub fn fly_to(&mut self, to: Camera) {
        self.cam_anim = Some((self.cam, to, 0.0));
    }

    pub fn cam_anim_tick(&mut self, dt: f32) {
        if let Some((a, b, t)) = self.cam_anim {
            let nt = (t + dt / 0.45).min(1.0);
            let k = nt * nt * (3.0 - 2.0 * nt);
            let mut dy = b.yaw - a.yaw;
            while dy > std::f32::consts::PI {
                dy -= std::f32::consts::TAU;
            }
            while dy < -std::f32::consts::PI {
                dy += std::f32::consts::TAU;
            }
            let mut c = b;
            c.eye = (a.eye() + (b.eye() - a.eye()) * k).into();
            c.yaw = a.yaw + dy * k;
            c.pitch = a.pitch + (b.pitch - a.pitch) * k;
            c.focus = a.focus + (b.focus - a.focus) * k;
            self.cam = c;
            self.cam_anim = if nt >= 1.0 { None } else { Some((a, b, nt)) };
        }
    }

    /// walking with optional collision (walls) and gravity (floors)
    pub fn walk_move(&mut self, local: Vec3, dist: f32, dt: f32) {
        let before = self.cam.eye();
        let mut probe = self.cam;
        probe.walk(local, dist);
        let after = probe.eye();
        let mv = after - before;
        let mut blocked = false;
        if self.settings.collision && mv.length() > 1e-6 {
            let dir = mv.normalize();
            let h = self.settings.eye_height;
            for z in [0.0, -h * 0.45, -h * 0.75] {
                let o = before + Vec3::new(0.0, 0.0, z);
                if let Some((_, t, _, _)) = self.pick(o, dir) {
                    if t < mv.length() + 0.3 {
                        blocked = true;
                        break;
                    }
                }
            }
        }
        if !blocked {
            self.cam.eye = after.into();
        }
        let _ = dt;
    }

    /// Mers lin: tastele si joystick-ul de mouse seteaza o viteza TINTA (`walk_target`, `walk_turn_target`)
    /// in fiecare cadru; aici viteza reala se apropie de ea cu accelerare / franare (fara salturi,
    /// independent de FPS), apoi tintele se reseteaza pentru cadrul urmator.
    pub fn walk_tick(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, 0.05);
        if self.path_run.is_some() {
            self.walk_path_tick(dt);
            self.walk_target = Vec3::ZERO;
            self.walk_turn_target = 0.0;
            self.walk_vel = Vec3::ZERO;
            self.walk_turn = 0.0;
            return;
        }
        let speed = self.settings.walk_speed.clamp(0.05, 50.0);
        self.walk_vel = walk::smooth(self.walk_vel, self.walk_target * speed, dt);
        self.walk_turn = walk::smooth1(self.walk_turn, self.walk_turn_target, dt);
        self.walk_target = Vec3::ZERO;
        self.walk_turn_target = 0.0;
        if self.walk_turn.abs() > 1e-4 {
            self.cam.look(self.walk_turn * dt, 0.0);
        } else {
            self.walk_turn = 0.0;
        }
        let v = self.walk_vel;
        if v.length() > 1e-3 {
            self.cam_anim = None;
            let d = v.length() * dt;
            self.walk_move(v / v.length(), d, dt);
        } else {
            self.walk_vel = Vec3::ZERO;
        }
    }

    /// se misca (sau incetineste) camera in modul mers - pentru repaint continuu
    pub fn walk_active(&self) -> bool {
        self.walk_vel != Vec3::ZERO || self.walk_turn != 0.0 || self.path_run.as_ref().map(|r| !r.paused).unwrap_or(false)
    }

    fn walk_path_tick(&mut self, dt: f32) {
        let (speed, eye) = (self.settings.walk_speed.clamp(0.05, 50.0), self.settings.eye_height);
        let Some(run) = self.path_run.as_mut() else { return };
        self.cam_anim = None;
        if run.step(&mut self.cam, speed, eye, dt) && !run.paused {
            run.paused = true;
            self.toast("End of the walk path.");
        }
    }

    /// Porneste / opreste (pauza) mersul pe traseu.
    pub fn walk_path_play(&mut self) {
        if let Some(r) = self.path_run.as_mut() {
            if r.dist >= r.length() {
                r.dist = 0.0;
            }
            r.paused = !r.paused;
            return;
        }
        match walk::PathRun::new(&self.walk_path) {
            Some(r) => {
                self.tool = Tool::Select;
                self.nav = NavMode::Walk;
                self.path_run = Some(r);
            }
            None => self.toast("Draw a walk path first: Walk Path > Draw, then click at least 2 points on the floor."),
        }
    }

    pub fn walk_path_stop(&mut self) {
        self.path_run = None;
    }

    /// Traseul ca animatie (Animator): se poate edita, reda si exporta video.
    pub fn walk_path_to_animation(&mut self) {
        let Some(r) = walk::PathRun::new(&self.walk_path) else {
            self.toast("Draw a walk path first (at least 2 points).");
            return;
        };
        let name = format!("Walk path {}", self.animations.len() + 1);
        let a = r.to_animation(name.clone(), self.settings.walk_speed, self.settings.eye_height, self.cam.fov_deg);
        let secs = a.duration();
        self.animations.push(a);
        self.toast(format!("Animation \"{name}\" created ({:.0} m, {:.0} s) - see Render > Animator.", r.length(), secs));
    }

    pub fn gravity_tick(&mut self, dt: f32) {
        if !(self.settings.gravity && self.nav == NavMode::Walk) {
            self.fall_speed = 0.0;
            return;
        }
        let eye = self.cam.eye();
        let h = self.settings.eye_height;
        if let Some((_, t, _, _)) = self.pick(eye + Vec3::new(0.0, 0.0, -0.05), Vec3::NEG_Z) {
            let ground = eye.z - 0.05 - t;
            let target = ground + h;
            if eye.z > target + 0.01 {
                self.fall_speed = (self.fall_speed + 9.81 * dt).min(30.0);
                let nz = (eye.z - self.fall_speed * dt).max(target);
                self.cam.eye[2] = nz;
            } else if eye.z < target - 0.01 && target - eye.z < 0.6 {
                // trepte / rampe: urca lin
                self.cam.eye[2] = eye.z + (target - eye.z).min(4.0 * dt);
                self.fall_speed = 0.0;
            } else {
                self.fall_speed = 0.0;
            }
        }
    }

    pub fn restore_viewpoint(&mut self, i: usize) {
        self.cam_anim = None;
        self.active_vp = Some(i);
        self.vp_sel = i;
        if let Some(v) = self.viewpoints.get(i).cloned() {
            self.cam = v.cam;
            self.section = v.section;
            self.hidden = v.hidden.into_iter().collect();
            // cotele salvate cu viewpoint-ul (viewpoint-urile vechi, fara cote, pastreaza masuratorile curente)
            if !v.measures.is_empty() {
                self.measures = v.measures;
            }
            self.clash_focus = None;
            self.colors_dirty = true;
        }
    }

    // ------------------------------------------------------------------ proiect
    pub fn project_file(&self) -> ProjectFile {
        let mut hidden: Vec<ElemId> = self.hidden.iter().copied().collect();
        hidden.sort();
        let mut overrides: Vec<(ElemId, [u8; 4])> = self.overrides.iter().map(|(k, v)| (*k, *v)).collect();
        overrides.sort_by_key(|x| x.0);
        ProjectFile {
            app: "ManageBIM".into(),
            version: 1,
            models: self.models.iter().map(|m| m.as_ref().map(|m| m.path.clone()).unwrap_or_default()).collect(),
            origin: self.origin.lock().unwrap().map(|o| o.to_array()),
            camera: self.cam,
            viewpoints: self.viewpoints.clone(),
            sets: self.sets.clone(),
            clash_tests: self.clash_tests.clone(),
            tasks: self.tasks.clone(),
            hidden,
            overrides,
            section: self.section.clone(),
            measures: self.measures.clone(),
            models_rel: self
                .models
                .iter()
                .map(|m| {
                    m.as_ref()
                        .map(|m| {
                            let proj_dir = self.project_path.as_ref().and_then(|p| std::path::Path::new(p).parent().map(|d| d.to_path_buf()));
                            match proj_dir {
                                Some(d) => std::path::Path::new(&m.path).strip_prefix(&d).map(|r| r.to_string_lossy().to_string()).unwrap_or_default(),
                                None => String::new(),
                            }
                        })
                        .unwrap_or_default()
                })
                .collect(),
            model_xf: self.model_xf.clone(),
            elem_offsets: {
                let mut v: Vec<(ElemId, [f32; 3])> = self.elem_offsets.iter().map(|(k, v)| (*k, *v)).collect();
                v.sort_by_key(|x| x.0);
                v
            },
            animations: self.animations.clone(),
            render: self.render.clone(),
            user_levels: self.user_levels.clone(),
            level_zero: self.level_zero,
            use_user_levels: self.use_user_levels,
            show_grids: self.show_grids,
            show_levels: self.show_levels,
            shared_dir: self.shared_dir.clone(),
            user_grids: self.user_grids.clone(),
            use_user_grids: self.use_user_grids,
            grid_bubble: self.grid_bubble,
            grid_ext: self.grid_ext,
            materials: self.materials.clone(),
            mat_assign: self.mat_assign.clone(),
            materials_on: Some(self.materials_on),
            home: self.home,
            walk_path: self.walk_path.iter().map(|p| p.to_array()).collect(),
            profile: if self.profile.rules.is_empty() { None } else { Some(self.profile.clone()) },
            qto: Some(self.qto.clone()),
        }
    }

    pub fn save_project(&mut self, path: &str) {
        let mut path = path.to_string();
        if !path.to_lowercase().ends_with(".bimr") {
            path.push_str(".bimr");
        }
        let pf = self.project_file();
        match serde_json::to_string_pretty(&pf).map_err(|e| e.to_string()).and_then(|s| std::fs::write(&path, s).map_err(|e| e.to_string())) {
            Ok(()) => {
                self.toast(format!("Project saved: {}", crate::file_name(&path)));
                self.settings.add_recent(&path);
                self.project_path = Some(path);
            }
            Err(e) => self.error(format!("Cannot save the project: {e}")),
        }
    }

    pub fn load_project(&mut self, path: &str) {
        let pf: ProjectFile = match std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string())) {
            Ok(p) => p,
            Err(e) => {
                self.error(format!("Invalid project: {e}"));
                return;
            }
        };
        self.reset();
        *self.origin.lock().unwrap() = pf.origin.map(DVec3::from_array);
        let base = std::path::Path::new(path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
        // transformarile se aplica imediat ce fiecare model termina incarcarea
        self.elem_offsets = pf.elem_offsets.iter().copied().collect();
        for (i, mp) in pf.models.iter().enumerate() {
            self.push_slot();
            if let Some(x) = pf.model_xf.get(i) {
                self.model_xf[i] = *x;
            }
            if mp.is_empty() {
                continue;
            }
            let mut real = mp.clone();
            if !std::path::Path::new(&real).exists() {
                let rel = pf.models_rel.get(i).filter(|r| !r.is_empty()).map(|r| base.join(r));
                let alt = rel.filter(|p| p.exists()).unwrap_or_else(|| base.join(crate::file_name(mp)));
                if alt.exists() {
                    real = alt.to_string_lossy().to_string();
                } else {
                    self.error(format!("Model not found: {mp}"));
                }
            }
            self.start_load(i, real);
        }
        self.project_path = Some(path.to_string());
        // proiectul salvat automat / despachetat din .bimd nu intra in lista Recent
        let internal = autosave_dir().map_or(false, |d| std::path::Path::new(path).starts_with(&d)) || published_dir().map_or(false, |d| std::path::Path::new(path).starts_with(&d));
        if !internal {
            self.settings.add_recent(path);
        }
        self.pending_project = Some(pf);
        if self.loads.is_empty() {
            if let Some(p) = self.pending_project.take() {
                self.apply_project_state(p);
            }
        }
    }

    fn apply_project_state(&mut self, pf: ProjectFile) {
        self.cam = pf.camera;
        self.home = pf.home;
        self.walk_path = pf.walk_path.iter().map(|p| Vec3::from(*p)).collect();
        self.path_run = None;
        self.animations = pf.animations;
        self.render = pf.render;
        self.user_levels = pf.user_levels;
        self.level_zero = pf.level_zero;
        self.use_user_levels = pf.use_user_levels;
        self.show_grids = pf.show_grids;
        self.show_levels = pf.show_levels;
        self.shared_dir = pf.shared_dir;
        self.user_grids = pf.user_grids;
        self.use_user_grids = pf.use_user_grids;
        self.grid_bubble = pf.grid_bubble;
        self.grid_ext = pf.grid_ext;
        self.shared = Default::default();
        if !pf.materials.is_empty() {
            self.materials = pf.materials;
        }
        self.mat_assign = pf.mat_assign;
        self.materials_on = pf.materials_on.unwrap_or(true);
        self.mat_cache = None;
        self.viewpoints = pf.viewpoints;
        self.sets = pf.sets;
        self.clash_tests = if pf.clash_tests.is_empty() { vec![ClashTest::new("Test 1".into())] } else { pf.clash_tests };
        self.cur_test = 0;
        self.tasks = pf.tasks;
        self.hidden = pf.hidden.into_iter().collect();
        self.overrides = pf.overrides.into_iter().collect();
        self.section = pf.section;
        self.measures = pf.measures;
        self.profile = pf.profile.unwrap_or_default();
        self.profile_map = None;
        self.qto = pf.qto.unwrap_or_default();
        self.qto_result = None;
        if let Some((s, _)) = self.sim_range() {
            self.sim_day = s as f32;
        }
        self.colors_dirty = true;
        self.toast("Project opened.");
    }

    /// Registers .bimc / .bimr (and "Open with" for .ifc) for the current user - no admin rights needed.
    #[cfg(windows)]
    pub fn register_file_types(&mut self) {
        use std::os::windows::process::CommandExt;
        let exe = match std::env::current_exe() {
            Ok(e) => e.to_string_lossy().to_string(),
            Err(e) => return self.error(format!("Cannot locate ManageBIM.exe: {e}")),
        };
        let open = format!("\"{exe}\" \"%1\"");
        let icon = format!("\"{exe}\",0");
        let mut keys: Vec<(String, Option<String>, String)> = Vec::new();
        for (ext, prog, desc) in [(".bimc", "ManageBIM.Cache", "ManageBIM model cache"), (".bimr", "ManageBIM.Project", "ManageBIM project"), ("", "ManageBIM.Ifc", "IFC model")] {
            if !ext.is_empty() {
                keys.push((format!("HKCU\\Software\\Classes\\{ext}"), None, prog.to_string()));
            }
            keys.push((format!("HKCU\\Software\\Classes\\{prog}"), None, desc.to_string()));
            keys.push((format!("HKCU\\Software\\Classes\\{prog}\\DefaultIcon"), None, icon.clone()));
            keys.push((format!("HKCU\\Software\\Classes\\{prog}\\shell\\open\\command"), None, open.clone()));
        }
        keys.push(("HKCU\\Software\\Classes\\.ifc\\OpenWithProgids".into(), Some("ManageBIM.Ifc".into()), String::new()));
        let mut failed = 0;
        for (key, name, data) in keys {
            let mut c = std::process::Command::new("reg");
            c.arg("add").arg(&key);
            match name {
                Some(n) => {
                    c.args(["/v", &n, "/t", "REG_SZ", "/d", ""]);
                }
                None => {
                    c.args(["/ve", "/d", &data]);
                }
            }
            c.arg("/f").creation_flags(0x0800_0000);
            if !c.status().map(|s| s.success()).unwrap_or(false) {
                failed += 1;
            }
        }
        if failed == 0 {
            self.toast(".bimc and .bimr files now open with ManageBIM (ManageBIM also appears in \"Open with\" for .ifc).");
        } else {
            self.error(format!("{failed} registry entries could not be written."));
        }
    }

    // ------------------------------------------------------------------ animation / render
    pub fn sim_sync(&mut self, anim: usize, t: f32) {
        let Some(a) = self.animations.get(anim) else { return };
        if a.sim_4d {
            if let Some((s, e)) = self.sim_range() {
                let dur = a.duration().max(1e-3);
                self.anim_sim_active = true;
                self.sim_day = s as f32 + (e - s + 1) as f32 * (t / dur).clamp(0.0, 1.0);
                return;
            }
        }
        if self.anim_sim_active {
            self.anim_sim_active = false;
            self.colors_dirty = true;
        }
    }

    pub fn goto_anim_time(&mut self, t: f32) {
        self.anim_time = t;
        if let Some(c) = self.animations.get(self.cur_anim).and_then(|a| a.sample(t)) {
            self.cam = c;
        }
        self.sim_sync(self.cur_anim, t);
    }

    /// Real-time preview in the viewport.
    pub fn anim_tick(&mut self, dt: f32) {
        let Some(t) = self.anim_play else { return };
        let dur = self.animations.get(self.cur_anim).map(|a| a.duration()).unwrap_or(0.0);
        let mut t = t + dt;
        if t > dur {
            if self.anim_loop && dur > 0.0 {
                t = 0.0;
            } else {
                self.anim_play = None;
                self.goto_anim_time(dur);
                return;
            }
        }
        self.anim_play = Some(t);
        self.goto_anim_time(t);
    }

    pub fn stop_anim(&mut self) {
        self.anim_play = None;
        if self.anim_sim_active {
            self.anim_sim_active = false;
            self.colors_dirty = true;
        }
    }

    pub fn render_bg(&self) -> ([u8; 3], [u8; 3]) {
        let pal = crate::theme::palette(self.settings.theme);
        let c = |c: eframe::egui::Color32| [c.r(), c.g(), c.b()];
        match self.render.background {
            Background::Theme => (c(pal.vp_top), c(pal.vp_bottom)),
            Background::White => ([255, 255, 255], [236, 239, 243]),
            Background::Dark => ([28, 32, 40], [72, 80, 94]),
        }
    }

    pub fn start_render_job(&mut self, path: &str) {
        if self.render_job.is_some() {
            return;
        }
        let Some(anim) = self.animations.get(self.cur_anim) else { return self.error("Create an animation first.") };
        if anim.keys.len() < 2 {
            return self.error("The animation needs at least two keyframes.");
        }
        let fmt = self.render.format;
        if fmt == VideoFormat::Mp4 && !self.ffmpeg.unwrap_or(false) {
            return self.error("MP4 needs ffmpeg (https://ffmpeg.org) on the PATH - choose AVI or install ffmpeg.");
        }
        let mut path = std::path::PathBuf::from(path);
        if path.extension().map(|e| e.to_string_lossy().to_lowercase()) != Some(fmt.ext().to_string()) {
            path.set_extension(fmt.ext());
        }
        let (w, h, fps, q) = (self.render.width & !1, self.render.height & !1, self.render.fps.max(1), self.render.quality);
        let ss = if self.render.supersample && fmt != VideoFormat::Gif { 2 } else { 1 };
        let frames = (anim.duration() * fps as f32).ceil() as u32 + 1;
        let sink = match crate::video::open_sink(fmt, &path, w, h, fps, q) {
            Ok(s) => s,
            Err(e) => return self.error(e),
        };
        let (tx, rx) = channel::<Frame>();
        let (dtx, drx) = channel();
        let p2 = path.clone();
        std::thread::spawn(move || {
            let mut sink = sink;
            let mut res = Ok(());
            while let Ok(f) = rx.recv() {
                let f = if ss > 1 { f.downscale(ss) } else { f };
                if let Err(e) = sink.add(&f) {
                    res = Err(e);
                    break;
                }
            }
            if res.is_ok() {
                res = sink.finish();
            }
            if res.is_ok() && fmt == VideoFormat::Mp4 {
                res = crate::video::convert_mp4(&p2);
            }
            let _ = dtx.send(res);
        });
        self.stop_anim();
        let saved_sel = std::mem::take(&mut self.selected);
        self.colors_dirty = true;
        self.render_job = Some(RenderJob { anim: self.cur_anim, frame: 0, frames, fps, ss, w: w * ss, h: h * ss, tx: Some(tx), done: drx, path, waiting: false, started: Instant::now(), saved_sel });
    }

    pub fn cancel_render(&mut self) {
        if let Some(j) = self.render_job.as_mut() {
            j.frames = j.frame;
            j.tx = None;
        }
    }

    /// Drives the export: one captured frame per UI frame. Called before the viewport is drawn.
    pub fn render_step(&mut self) {
        let captured = self.renderer.lock().unwrap().captured.take();
        if let Some(r) = captured {
            if let Some(p) = self.still_path.take() {
                match r {
                    Ok(f) => {
                        let f = if self.render.supersample { f.downscale(2) } else { f };
                        match crate::video::save_png(std::path::Path::new(&p), &f) {
                            Ok(()) => self.toast(format!("Image saved: {} ({}×{})", crate::file_name(&p), f.w, f.h)),
                            Err(e) => self.error(e),
                        }
                    }
                    Err(e) => self.error(e),
                }
            } else if let Some(j) = self.render_job.as_mut() {
                j.waiting = false;
                match r {
                    Ok(f) => {
                        if let Some(tx) = &j.tx {
                            let _ = tx.send(f);
                        }
                        j.frame += 1;
                    }
                    Err(e) => {
                        j.frames = j.frame;
                        j.tx = None;
                        self.error(e);
                    }
                }
            }
        }
        let Some(j) = self.render_job.as_ref() else { return };
        if j.frame >= j.frames {
            // toate cadrele trimise: inchide encoderul si asteapta rezultatul
            if let Some(j) = self.render_job.as_mut() {
                j.tx = None;
            }
            let res = self.render_job.as_ref().unwrap().done.try_recv();
            if let Ok(r) = res {
                let j = self.render_job.take().unwrap();
                self.selected = j.saved_sel;
                self.anim_sim_active = false;
                self.colors_dirty = true;
                match r {
                    Ok(()) => {
                        let secs = j.started.elapsed().as_secs_f32();
                        let target = if self.render.format == VideoFormat::PngSequence { crate::video::png_dir(&j.path) } else { j.path.clone() };
                        self.toast(format!("Animation exported: {} ({} frames in {:.0} s)", target.display(), j.frame, secs));
                    }
                    Err(e) => self.error(e),
                }
            }
            return;
        }
        if !j.waiting {
            let (anim, t, w, h) = (j.anim, j.frame as f32 / j.fps as f32, j.w, j.h);
            if let Some(c) = self.animations.get(anim).and_then(|a| a.sample(t)) {
                self.cam = c;
            }
            self.anim_time = t;
            self.sim_sync(anim, t);
            self.pending_capture = Some((w, h));
            if let Some(j) = self.render_job.as_mut() {
                j.waiting = true;
            }
        }
    }

    /// Writes a loaded model to the native .bimc format.
    pub fn export_cache(&mut self, slot: usize, path: &str) {
        let Some(m) = self.models.get(slot).and_then(|m| m.clone()) else { return };
        let mut path = path.to_string();
        if !path.to_lowercase().ends_with(".bimc") {
            path.push_str(".bimc");
        }
        let origin = self.origin.lock().unwrap().unwrap_or(DVec3::ZERO);
        let raw = crate::bimc::from_model(&m, origin);
        match crate::bimc::write(&raw, &path) {
            Ok(n) => {
                let src = std::fs::metadata(&m.path).map(|x| x.len()).unwrap_or(0);
                let ratio = if src > 0 { format!(", {:.1}x smaller than the source", src as f64 / n as f64) } else { String::new() };
                self.toast(format!("Exported {} ({:.2} MB{ratio})", crate::file_name(&path), n as f64 / 1e6));
            }
            Err(e) => self.error(e),
        }
    }

    /// Exports every loaded model as `<source>.bimc` next to its source file.
    pub fn export_all_caches(&mut self) {
        let slots: Vec<(usize, String)> = self.loaded_models().filter(|(_, m)| !m.path.to_lowercase().ends_with(".bimc")).map(|(i, m)| (i, crate::bimc::cache_path(&m.path))).collect();
        if slots.is_empty() {
            self.error("No IFC / glTF models loaded.");
        }
        for (i, p) in slots {
            self.export_cache(i, &p);
        }
    }

    pub fn import_tasks_csv(&mut self, path: &str) {
        match std::fs::read(path) {
            Ok(b) => {
                let text = String::from_utf8_lossy(&b).to_string();
                let (mut tasks, errs) = timeline::parse_csv(&text);
                let n = tasks.len();
                for t in tasks.iter_mut() {
                    t.elems = self.rule_elems(&t.rule);
                }
                self.tasks.extend(tasks);
                if let Some((s, _)) = self.sim_range() {
                    self.sim_day = s as f32;
                }
                self.invalidate_sim();
                self.toast(format!("Imported {n} tasks from CSV."));
                for e in errs {
                    self.log.push(e);
                }
            }
            Err(e) => self.error(format!("Cannot read the CSV: {e}")),
        }
    }

    pub fn export_tasks_csv(&self) -> String {
        let mut s = String::from("Name;Start;End;Type;Rule\n");
        for t in &self.tasks {
            s.push_str(&format!("{};{};{};{};{}\n", t.name, timeline::fmt_date(t.start), timeline::fmt_date(t.end), t.kind.label(), t.rule));
        }
        s
    }

    /// Fisierul ales intr-un dialog: exporturile / importurile ruleaza protejat - o eroare interna devine mesaj,
    /// nu inchide aplicatia (proiectul ramane deschis).
    pub fn handle_path(&mut self, purpose: Purpose, path: String) {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.handle_path_inner(purpose, path)));
        if let Err(e) = r {
            self.error(format!("Internal error ({}) - the operation was stopped, the project is unchanged on disk.", panic_text(&e)));
        }
    }

    fn handle_path_inner(&mut self, purpose: Purpose, path: String) {
        let path = path.trim().trim_matches('"').to_string();
        if path.is_empty() {
            return;
        }
        let write = |s: &mut App, content: String, what: &str| match std::fs::write(&path, content) {
            Ok(()) => s.toast(format!("{what} saved: {}", crate::file_name(&path))),
            Err(e) => s.error(format!("Cannot write the file: {e}")),
        };
        match purpose {
            Purpose::OpenModel { append } => self.open_path(&path, append),
            Purpose::OpenProject => self.open_path(&path, false),
            Purpose::PublishBimd => match self.publish_bimd(&path) {
                Ok(m) => {
                    self.bimd_path = Some(with_ext(&path, "bimd"));
                    self.toast(m);
                }
                Err(e) => self.error(e),
            },
            Purpose::SaveProject => self.save_project(&path),
            Purpose::ExportClashHtml => {
                let c = self.clash_report_html(self.cur_test);
                write(self, c, "HTML report");
            }
            Purpose::ExportClashCsv => {
                let c = self.clash_report_csv(self.cur_test);
                write(self, c, "CSV report");
            }
            Purpose::ExportClashBcf => self.export_clash_bcf(&path),
            Purpose::ExportIfc => self.export_ifc(&path),
            Purpose::ExportGrids => self.export_grids(&path),
            Purpose::ImportGridsCad => self.import_grids_cad(&path, None),
            Purpose::ExportReference => self.export_reference(&path),
            Purpose::ExportClashMatrix => self.export_clash_matrix(&path),
            Purpose::SharedFolder => {
                self.shared_dir = Some(path.clone());
                self.shared = Default::default();
                self.shared_sync(true);
                self.toast(format!("Issues are shared in {path}. Colleagues: open the same models and choose the same folder."));
            }
            Purpose::ExportClashNavis => self.export_clash_navis(&path),
            Purpose::ImportBcf => self.import_bcf(&path),
            Purpose::ImportTasksCsv => self.import_tasks_csv(&path),
            Purpose::ExportTasksCsv => {
                let c = self.export_tasks_csv();
                write(self, c, "Schedule CSV");
            }
            Purpose::ExportCache(slot) => self.export_cache(slot, &path),
            Purpose::ExportVideo => self.start_render_job(&path),
            Purpose::ImportTextures => self.import_texture_materials(&[path.clone()]),
            Purpose::ExportVpHtml => self.start_report(0, &path),
            Purpose::ExportVpXlsx => self.start_report(1, &path),
            Purpose::ExportBcf => self.start_report(2, &path),
            Purpose::ExportNavisXml => self.export_navis_xml(&path),
            Purpose::ImportNavisXml => self.import_navis_xml(&path),
            Purpose::ExportNavisSets => self.export_navis_sets(&path),
            Purpose::ExportNavisClash => self.export_navis_clash(&path),
            Purpose::MaterialTexture => {
                if crate::materials::load_texture(&path).is_ok() {
                    if let Some(m) = self.materials.get_mut(self.mat_sel) {
                        m.texture = path.clone();
                        m.color = [255, 255, 255];
                    }
                    self.tex_failed.remove(&path);
                    self.materials_changed();
                } else {
                    self.error("Choose a PNG or JPG image.");
                }
            }
            Purpose::ExportMaterials if path.to_lowercase().ends_with(".csv") || path.to_lowercase().ends_with(".xlsx") => {
                let res = if path.to_lowercase().ends_with(".csv") { std::fs::write(&path, crate::tables::materials_csv(&self.materials)) } else { std::fs::write(&path, crate::tables::materials_xlsx(&self.materials)) };
                match res {
                    Ok(()) => self.toast(format!("{} materials exported to {}", self.materials.len(), crate::file_name(&path))),
                    Err(e) => self.error(format!("Cannot write the file: {e}")),
                }
            }
            Purpose::ImportMaterials if path.to_lowercase().ends_with(".csv") || path.to_lowercase().ends_with(".xlsx") => match crate::tables::read_rows(&path).and_then(|r| crate::tables::materials_from_rows(&r)) {
                Ok(v) => {
                    let n = v.len();
                    for m in v {
                        if let Some(x) = self.materials.iter_mut().find(|x| x.name == m.name) {
                            *x = m;
                        } else {
                            self.materials.push(m);
                        }
                    }
                    self.materials_changed();
                    self.toast(format!("Imported {n} materials from {}.", crate::file_name(&path)));
                }
                Err(e) => self.error(format!("Material table: {e}")),
            },
            Purpose::ExportMaterials => {
                let mut p = path.clone();
                if !p.to_lowercase().ends_with(".bimmat") {
                    p.push_str(".bimmat");
                }
                match serde_json::to_string_pretty(&self.materials).map_err(|e| e.to_string()).and_then(|j| std::fs::write(&p, j).map_err(|e| e.to_string())) {
                    Ok(()) => self.toast(format!("{} materials exported to {}", self.materials.len(), crate::file_name(&p))),
                    Err(e) => self.error(e),
                }
            }
            Purpose::ImportMaterials => match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Vec<Material>>(&t).map_err(|e| e.to_string())) {
                Ok(v) => {
                    let n = v.len();
                    for m in v {
                        if let Some(x) = self.materials.iter_mut().find(|x| x.name == m.name) {
                            *x = m;
                        } else {
                            self.materials.push(m);
                        }
                    }
                    self.materials_changed();
                    self.toast(format!("Imported {n} materials."));
                }
                Err(e) => self.error(format!("Invalid material library: {e}")),
            },
            Purpose::RenderImage => {
                let mut p = path.clone();
                if !p.to_lowercase().ends_with(".png") {
                    p.push_str(".png");
                }
                self.still_path = Some(p);
                let ss = if self.render.supersample { 2 } else { 1 };
                self.pending_capture = Some((self.render.width * ss, self.render.height * ss));
            }
            Purpose::ExportSets => {
                let mut path = path.clone();
                if !path.to_lowercase().ends_with(".bimsets") {
                    path.push_str(".bimsets");
                }
                match serde_json::to_string_pretty(&self.sets) {
                    Ok(j) => match std::fs::write(&path, j) {
                        Ok(()) => self.toast(format!("{} sets exported to {}", self.sets.len(), crate::file_name(&path))),
                        Err(e) => self.error(format!("Cannot write the file: {e}")),
                    },
                    Err(e) => self.error(e.to_string()),
                }
            }
            Purpose::ImportSets => match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Vec<SelSet>>(&t).map_err(|e| e.to_string())) {
                Ok(v) => {
                    let n = v.len();
                    for mut s in v {
                        while self.sets.iter().any(|x| x.name == s.name) {
                            s.name.push_str(" (2)");
                        }
                        self.sets.push(s);
                    }
                    self.toast(format!("Imported {n} sets."));
                }
                Err(e) => self.error(format!("Invalid sets file: {e}")),
            },
            Purpose::ExportProfile if path.to_lowercase().ends_with(".csv") || path.to_lowercase().ends_with(".xlsx") => {
                let res = if path.to_lowercase().ends_with(".csv") { std::fs::write(&path, crate::tables::profile_csv(&self.profile)) } else { std::fs::write(&path, crate::tables::profile_xlsx(&self.profile)) };
                match res {
                    Ok(()) => self.toast(format!("Colour profile exported ({} colours): {}", self.profile.rules.len(), crate::file_name(&path))),
                    Err(e) => self.error(format!("Cannot write the file: {e}")),
                }
            }
            Purpose::ExportProfile => {
                let p = with_ext(&path, "bimprofile");
                let j = self.profile.to_file_json();
                match std::fs::write(&p, j) {
                    Ok(()) => self.toast(format!("Profile exported ({} colours): {}", self.profile.rules.len(), crate::file_name(&p))),
                    Err(e) => self.error(format!("Cannot write the file: {e}")),
                }
            }
            Purpose::ImportProfile => match if path.to_lowercase().ends_with(".csv") || path.to_lowercase().ends_with(".xlsx") {
                crate::tables::read_rows(&path).and_then(|r| crate::tables::profile_from_rows(&r))
            } else {
                std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| crate::profiler::Profile::from_file_json(&t))
            } {
                Ok(p) => {
                    self.profile = p;
                    self.profile.active = true;
                    self.profile_recount();
                    let matched: u32 = self.profile.rules.iter().map(|r| r.count).sum();
                    let unc = self.profile_uncovered();
                    let extra = if unc > 0 { format!(" {unc} values in these models have no colour yet - use \"Add missing values\".") } else { String::new() };
                    self.toast(format!("Profile imported: {} / {}, {} colours, {matched} items coloured.{extra}", self.profile.cat, self.profile.prop, self.profile.rules.len()));
                    self.open_panel(Panel::Profiler);
                }
                Err(e) => self.error(e),
            },
            Purpose::ExportCompare | Purpose::ExportCompareCsv => {
                let Some(r) = &self.compare else { return self.error("Run Compare first.") };
                let (Some(ma), Some(mb)) = (self.models.get(r.a).cloned().flatten(), self.models.get(r.b).cloned().flatten()) else { return self.error("Compare: the models are no longer loaded.") };
                let (p, text, what) = if purpose == Purpose::ExportCompare {
                    (with_ext(&path, "bimcmp"), serde_json::to_string_pretty(&r.to_file(&ma, &mb, now_string())).unwrap_or_default(), "Comparison")
                } else {
                    (with_ext(&path, "csv"), r.to_csv(&ma, &mb), "Comparison report")
                };
                match std::fs::write(&p, text) {
                    Ok(()) => self.toast(format!("{what} saved: {}", crate::file_name(&p))),
                    Err(e) => self.error(format!("Cannot write the file: {e}")),
                }
            }
            Purpose::ImportCompare => {
                let f = match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| crate::compare::CompareFile::parse(&t)) {
                    Ok(f) => f,
                    Err(e) => return self.error(e),
                };
                // modelele: cele alese in panou, altfel cele cu aceleasi nume ca in fisier
                let by_name = |n: &str| self.loaded_models().find(|(_, m)| m.name == n).map(|(i, _)| i);
                let a = by_name(&f.a_name).or(self.compare_pick.0);
                let b = by_name(&f.b_name).or(self.compare_pick.1);
                let (Some(a), Some(b)) = (a, b) else { return self.error("Import comparison: load both versions (A and B) first.") };
                let (Some(ma), Some(mb)) = (self.models.get(a).cloned().flatten(), self.models.get(b).cloned().flatten()) else { return self.error("Import comparison: load both versions (A and B) first.") };
                if a == b {
                    return self.error("Import comparison: A and B must be two different models.");
                }
                let (r, missing) = f.apply(&ma, a, &mb, b);
                self.compare_pick = (Some(a), Some(b));
                self.compare_tol = f.tol_mm;
                let note = if missing > 0 { format!(" {missing} items from the file were not found in these models.") } else { String::new() };
                self.toast(format!("Comparison imported ({} → {}, {}).{note}", f.a_name, f.b_name, f.created));
                self.compare = Some(r);
                self.compare_on = true;
                self.colors_dirty = true;
                self.open_panel(Panel::Compare);
            }
            Purpose::ExportTree => {
                let p = with_ext(&path, "bimtree");
                let f = TreeFile { app: "ManageBIM".into(), kind: TREE_FILE_KIND.into(), version: 1, levels: self.settings.tree_levels.clone() };
                match std::fs::write(&p, serde_json::to_string_pretty(&f).unwrap_or_default()) {
                    Ok(()) => self.toast(format!("Tree layout saved: {}", crate::file_name(&p))),
                    Err(e) => self.error(format!("Cannot write the file: {e}")),
                }
            }
            Purpose::ImportTree => match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<TreeFile>(&t).map_err(|e| e.to_string())) {
                Ok(f) if f.kind == TREE_FILE_KIND && !f.levels.is_empty() => {
                    let desc = f.levels.iter().map(|l| l.prop.clone()).collect::<Vec<_>>().join(" › ");
                    self.set_tree_levels(f.levels);
                    self.toast(format!("Tree layout imported: {desc}"));
                }
                Ok(_) => self.error("This is not a ManageBIM tree layout file."),
                Err(e) => self.error(format!("Invalid tree layout file: {e}")),
            },
            Purpose::ExportQtoXlsx | Purpose::ExportQtoCsv => {
                self.qto_update();
                let Some(r) = &self.qto_result else { return };
                let title = self.project_path.as_deref().map(crate::file_name).unwrap_or_else(|| "ManageBIM".into());
                let (p, bytes) = if purpose == Purpose::ExportQtoXlsx {
                    (with_ext(&path, "xlsx"), r.to_xlsx(&title, &|id| self.qto_item_info(id)))
                } else {
                    (with_ext(&path, "csv"), r.summary_csv().into_bytes())
                };
                let n = r.rows.len();
                match std::fs::write(&p, bytes) {
                    Ok(()) => self.toast(format!("Quantities exported ({n} groups): {}", crate::file_name(&p))),
                    Err(e) => self.error(format!("Cannot write the file: {e}")),
                }
            }
            Purpose::ExportQtoTemplate => {
                let p = with_ext(&path, "bimqto");
                match std::fs::write(&p, self.qto.to_file_json()) {
                    Ok(()) => self.toast(format!("Quantification template saved: {}", crate::file_name(&p))),
                    Err(e) => self.error(format!("Cannot write the file: {e}")),
                }
            }
            Purpose::ImportQtoTemplate => match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| crate::qto::QtoSetup::from_file_json(&t)) {
                Ok(s) => {
                    self.toast(format!("Quantification template loaded: {}", s.name));
                    self.qto = s;
                    self.qto_result = None;
                    self.open_panel(Panel::Quantities);
                }
                Err(e) => self.error(e),
            },
        }
    }

    /// Deschide dialogul de fisiere (Windows) sau un camp text (alte sisteme).
    pub fn request_path(&mut self, purpose: Purpose) {
        #[cfg(windows)]
        {
            let d = rfd::FileDialog::new();
            let r = match purpose {
                Purpose::OpenModel { .. } => d.set_title("Open Model").add_filter("All supported models", &crate::SUPPORTED[..crate::SUPPORTED_MODELS])
                    .add_filter("Navisworks / Revit (through Navisworks)", &crate::formats::NAVIS_EXT)
                    .add_filter("IFC / ManageBIM", &["ifc", "bimc"])
                    .add_filter("CAD (DWG, DXF, STEP, IGES, BREP, FCStd)", &["dwg", "dxf", "step", "stp", "iges", "igs", "brep", "brp", "fcstd"])
                    .add_filter("3D meshes (glTF, OBJ, STL, PLY)", &["glb", "gltf", "obj", "stl", "ply"]).add_filter("All files", &["*"]).pick_files().map(|v| v.into_iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>()),
                Purpose::PublishBimd => d.set_title("Publish project + models (.bimd)").add_filter("ManageBIM published project", &["bimd"]).set_file_name(self.project_path.as_ref().map(|p| format!("{}.bimd", std::path::Path::new(p).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())).unwrap_or_else(|| "project.bimd".into())).save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::OpenProject => d.set_title("Open Project").add_filter("ManageBIM project (.bimr / .bimd)", &["bimr", "bimd"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::SaveProject => d.set_title("Save Project").add_filter("ManageBIM project", &["bimr"]).set_file_name("project.bimr").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportClashHtml => d.set_title("Clash report (HTML)").add_filter("HTML", &["html"]).set_file_name("clash_report.html").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportClashCsv => d.set_title("Clash report (CSV)").add_filter("CSV", &["csv"]).set_file_name("clash_report.csv").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportReference => {
                    let dir = outbox_dir().map(|d| { let _ = std::fs::create_dir_all(&d); d }).unwrap_or_default();
                    let name = self.bimd_path.as_ref().or(self.project_path.as_ref()).and_then(|p| std::path::Path::new(p).file_stem().map(|s| s.to_string_lossy().to_string())).unwrap_or_else(|| "reference".into());
                    d.set_title("Reference for Revit / AutoCAD / Plant 3D").add_filter("ManageBIM reference", &["mbref"]).set_directory(dir).set_file_name(format!("{name}.mbref")).save_file().map(|p| vec![p.to_string_lossy().to_string()])
                }
                Purpose::ExportClashMatrix => d.set_title("Clash matrix (Excel)").add_filter("Excel workbook", &["xlsx"]).set_file_name("clash_matrix.xlsx").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportGridsCad => d.set_title("Import grid axes from DWG / DXF").add_filter("DWG / DXF", &["dwg", "dxf"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportGrids => d.set_title("Export grid axes").add_filter("DXF (AutoCAD, any CAD)", &["dxf"]).add_filter("DWG (needs ODA File Converter)", &["dwg"]).set_file_name("grids.dxf").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::SharedFolder => d.set_title("Shared issues folder (network / SharePoint / OneDrive)").pick_folder().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportIfc => d.set_title("Export to IFC4").add_filter("IFC", &["ifc"]).set_file_name("export.ifc").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportClashBcf => d.set_title("Export clashes as BCF").add_filter("BIM Collaboration Format", &["bcf", "bcfzip"]).set_file_name("clashes.bcf").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportClashNavis => d.set_title("Export clashes as Navisworks viewpoints").add_filter("Navisworks viewpoints XML", &["xml"]).set_file_name("clashes_viewpoints.xml").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportBcf => d.set_title("Import BCF issues").add_filter("BIM Collaboration Format", &["bcf", "bcfzip"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportTasksCsv => d.set_title("Import CSV Schedule").add_filter("CSV", &["csv", "txt"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportTasksCsv => d.set_title("Export CSV Schedule").add_filter("CSV", &["csv"]).set_file_name("schedule.csv").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportVideo => {
                    let ext = self.render.format.ext();
                    let name = format!("{}.{ext}", self.animations.get(self.cur_anim).map(|a| a.name.clone()).unwrap_or_else(|| "animation".into()));
                    d.set_title("Export animation").add_filter(ext.to_uppercase(), &[ext]).set_file_name(name).save_file().map(|p| vec![p.to_string_lossy().to_string()])
                }
                Purpose::RenderImage => d.set_title("Render image").add_filter("PNG image", &["png"]).set_file_name("render.png").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportTextures => {
                    if let Some(v) = d.clone().set_title("Import images as materials").add_filter("Images", &["png", "jpg", "jpeg"]).pick_files() {
                        let v: Vec<String> = v.into_iter().map(|p| p.to_string_lossy().to_string()).collect();
                        self.import_texture_materials(&v);
                    }
                    None
                }
                Purpose::MaterialTexture => d.set_title("Choose texture image").add_filter("Images", &["png", "jpg", "jpeg"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportMaterials => d.set_title("Export material library").add_filter("ManageBIM materials", &["bimmat"]).add_filter("Excel workbook", &["xlsx"]).add_filter("CSV (Excel)", &["csv"]).set_file_name("materials.bimmat").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportMaterials => d.set_title("Import material library").add_filter("Materials / Excel / CSV", &["bimmat", "json", "xlsx", "csv"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportVpHtml => d.set_title("Viewpoints report (HTML)").add_filter("HTML report", &["html"]).set_file_name("viewpoints_report.html").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportVpXlsx => d.set_title("Viewpoints report (Excel)").add_filter("Excel workbook", &["xlsx"]).set_file_name("viewpoints_report.xlsx").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportBcf => d.set_title("Export BCF 2.1").add_filter("BIM Collaboration Format", &["bcf", "bcfzip"]).set_file_name("issues.bcf").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportNavisXml => d.set_title("Export viewpoints for Navisworks").add_filter("Navisworks viewpoints XML", &["xml"]).set_file_name("viewpoints.xml").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportNavisXml => d.set_title("Import Navisworks viewpoints").add_filter("Navisworks viewpoints XML", &["xml"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportNavisSets => d.set_title("Export sets for Navisworks").add_filter("Navisworks search sets XML", &["xml"]).set_file_name("sets.xml").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportNavisClash => d.set_title("Export clash tests for Navisworks").add_filter("Navisworks clash tests XML", &["xml"]).set_file_name("clash_tests.xml").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportSets => d.set_title("Export sets").add_filter("ManageBIM sets", &["bimsets"]).set_file_name("sets.bimsets").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportSets => d.set_title("Import sets").add_filter("ManageBIM sets", &["bimsets", "json"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportProfile => {
                    let name = format!("{}.bimprofile", self.profile.prop.replace(['/', '\\', ':'], "_"));
                    d.set_title("Export Appearance Profile").add_filter("ManageBIM appearance profile", &["bimprofile"]).add_filter("Excel workbook", &["xlsx"]).add_filter("CSV (Excel)", &["csv"]).set_file_name(name).save_file().map(|p| vec![p.to_string_lossy().to_string()])
                }
                Purpose::ImportProfile => d.set_title("Import Appearance Profile").add_filter("Profile / Excel / CSV", &["bimprofile", "json", "xlsx", "csv"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportCompare => d.set_title("Save comparison").add_filter("ManageBIM comparison", &["bimcmp"]).set_file_name("comparison.bimcmp").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportCompare => d.set_title("Open comparison").add_filter("ManageBIM comparison", &["bimcmp", "json"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportCompareCsv => d.set_title("Comparison report (CSV)").add_filter("CSV", &["csv"]).set_file_name("comparison.csv").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportTree => d.set_title("Export tree layout").add_filter("ManageBIM tree layout", &["bimtree"]).set_file_name("tree.bimtree").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ImportTree => d.set_title("Import tree layout").add_filter("ManageBIM tree layout", &["bimtree", "json"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportQtoXlsx => {
                    let name = format!("{}.xlsx", self.qto.name.replace(['/', '\\', ':'], "_"));
                    d.set_title("Export quantities (Excel)").add_filter("Excel workbook", &["xlsx"]).set_file_name(name).save_file().map(|p| vec![p.to_string_lossy().to_string()])
                }
                Purpose::ExportQtoCsv => d.set_title("Export quantities (CSV)").add_filter("CSV", &["csv"]).set_file_name("quantities.csv").save_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportQtoTemplate => {
                    let name = format!("{}.bimqto", self.qto.name.replace(['/', '\\', ':'], "_"));
                    d.set_title("Save quantification template").add_filter("ManageBIM quantification template", &["bimqto"]).set_file_name(name).save_file().map(|p| vec![p.to_string_lossy().to_string()])
                }
                Purpose::ImportQtoTemplate => d.set_title("Open quantification template").add_filter("ManageBIM quantification template", &["bimqto", "json"]).pick_file().map(|p| vec![p.to_string_lossy().to_string()]),
                Purpose::ExportCache(slot) => {
                    let name = self.models.get(slot).and_then(|m| m.as_ref()).map(|m| format!("{}.bimc", m.name)).unwrap_or_else(|| "model.bimc".into());
                    d.set_title("Export ManageBIM cache").add_filter("ManageBIM cache", &["bimc"]).set_file_name(name).save_file().map(|p| vec![p.to_string_lossy().to_string()])
                }
            };
            if let Some(paths) = r {
                for (i, p) in paths.into_iter().enumerate() {
                    let pur = match purpose {
                        Purpose::OpenModel { append } => Purpose::OpenModel { append: append || i > 0 },
                        o => o,
                    };
                    self.handle_path(pur, p);
                }
            }
        }
        #[cfg(not(windows))]
        {
            let text = match purpose {
                Purpose::SaveProject => self.project_path.clone().unwrap_or_else(|| "project.bimr".into()),
                Purpose::ExportClashHtml => "clash_report.html".into(),
                Purpose::ExportClashCsv => "clash_report.csv".into(),
                Purpose::ExportClashBcf => "clashes.bcf".into(),
                Purpose::ExportIfc => "export.ifc".into(),
                Purpose::SharedFolder => String::new(),
                Purpose::PublishBimd => "project.bimd".into(),
                Purpose::ExportGrids => "grids.dxf".into(),
                Purpose::ImportGridsCad => String::new(),
                Purpose::ExportReference => "reference.mbref".into(),
                Purpose::ExportClashMatrix => "clash_matrix.xlsx".into(),
                Purpose::ExportClashNavis => "clashes_viewpoints.xml".into(),
                Purpose::ExportTasksCsv => "schedule.csv".into(),
                Purpose::ExportSets => "sets.bimsets".into(),
                Purpose::ExportMaterials => "materials.bimmat".into(),
                Purpose::ExportVpHtml => "viewpoints_report.html".into(),
                Purpose::ExportVpXlsx => "viewpoints_report.xlsx".into(),
                Purpose::ExportBcf => "issues.bcf".into(),
                Purpose::ExportNavisXml => "viewpoints.xml".into(),
                Purpose::ExportNavisSets => "sets.xml".into(),
                Purpose::ExportNavisClash => "clash_tests.xml".into(),
                Purpose::ExportVideo => format!("animation.{}", self.render.format.ext()),
                Purpose::RenderImage => "render.png".into(),
                Purpose::ExportProfile => "profile.bimprofile".into(),
                Purpose::ExportCompare => "comparison.bimcmp".into(),
                Purpose::ExportCompareCsv => "comparison.csv".into(),
                Purpose::ExportTree => "tree.bimtree".into(),
                Purpose::ExportQtoXlsx => "quantities.xlsx".into(),
                Purpose::ExportQtoCsv => "quantities.csv".into(),
                Purpose::ExportQtoTemplate => "quantities.bimqto".into(),
                Purpose::ExportCache(slot) => self.models.get(slot).and_then(|m| m.as_ref()).map(|m| crate::bimc::cache_path(&m.path)).unwrap_or_default(),
                _ => String::new(),
            };
            self.prompt = Some(PathPrompt { purpose, text });
        }
    }
}

pub fn now_string() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (y, m, d) = timeline::civil_from_days((secs / 86400) as i32);
    let s = secs % 86400;
    format!("{y}-{m:02}-{d:02} {:02}:{:02} UTC", s / 3600, (s % 3600) / 60)
}

/// Textul unei erori interne (panic) prinse cu catch_unwind.
pub fn panic_text(e: &(dyn std::any::Any + Send)) -> String {
    e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "unknown error".into())
}

/// Natural ordering: "Level 2" < "Level 10", numbers compared by value.
///
/// Must be a TOTAL order (Rust's sort panics otherwise - the old version compared some pairs as
/// numbers and others as text, e.g. "10" < "2 m" < "ab" < "10", and crashed on Plant 3D values).
/// Order: values starting with a number first (by value, then naturally), then the others
/// naturally (text chunks case-insensitive, digit runs by value), then exact text as tie-break.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (leading_number(a), leading_number(b)) {
        (Some(x), Some(y)) => x.total_cmp(&y).then_with(|| natural_chunks_cmp(a, b)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => natural_chunks_cmp(a, b),
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Chunk {
    /// digit run: (length without leading zeros, the digits) = numeric order of any length
    Num(usize, String),
    Text(String),
}

fn chunks(s: &str) -> Vec<Chunk> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut digits = false;
    let push = |out: &mut Vec<Chunk>, cur: &mut String, digits: bool| {
        if cur.is_empty() {
            return;
        }
        let c = std::mem::take(cur);
        out.push(if digits {
            let t = c.trim_start_matches('0').to_string();
            Chunk::Num(t.len(), t)
        } else {
            Chunk::Text(c.to_lowercase())
        });
    };
    for ch in s.chars() {
        let d = ch.is_ascii_digit();
        if d != digits {
            push(&mut out, &mut cur, digits);
            digits = d;
        }
        cur.push(ch);
    }
    push(&mut out, &mut cur, digits);
    out
}

fn natural_chunks_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    chunks(a).cmp(&chunks(b)).then_with(|| a.cmp(b))
}

/// Axele si nivelurile unui model, in coordonate locale (ale scenei).
#[derive(Default, Clone)]
pub struct GridData {
    /// (eticheta, capat 1, capat 2)
    pub grids: Vec<(String, Vec3, Vec3)>,
    /// (nume, cota Z locala), sortate dupa cota
    pub levels: Vec<(String, f32)>,
}

impl App {
    /// Axele / nivelurile modelului din slotul dat: din IFC (IfcGrid, IfcBuildingStorey) sau, fara niveluri in
    /// fisier, deduse din elemente (cota minima a elementelor fiecarui nivel). Calculate o data per model.
    pub fn grid_data(&self, slot: usize) -> Arc<GridData> {
        let Some(m) = self.models.get(slot).cloned().flatten() else { return Arc::new(GridData::default()) };
        let key = Arc::as_ptr(&m) as usize;
        if let Some((k, g)) = self.grid_cache.borrow().get(&slot) {
            if *k == key {
                return g.clone();
            }
        }
        let o = self.origin_f64();
        let xf = self.base_models.get(slot).cloned().flatten().map(|b| {
            let x = self.model_xf.get(slot).copied().unwrap_or_default();
            if x.is_identity() { Mat4::IDENTITY } else { self.xf_matrix(&b, &x) }
        });
        let xf = xf.unwrap_or(Mat4::IDENTITY);
        let loc = |p: [f64; 3]| xf.transform_point3((DVec3::from_array(p) - o).as_vec3());
        let mut g = GridData::default();
        if let Some((_, v)) = m.info.iter().find(|(k, _)| k == "~grids") {
            if let Ok(list) = serde_json::from_str::<Vec<(String, [f64; 3], [f64; 3])>>(v) {
                g.grids = list.into_iter().map(|(t, a, b)| (t, loc(a), loc(b))).collect();
            }
        }
        if let Some((_, v)) = m.info.iter().find(|(k, _)| k == "~levels") {
            if let Ok(list) = serde_json::from_str::<Vec<(String, f64)>>(v) {
                g.levels = list.into_iter().map(|(n, z)| (n, loc([o.x, o.y, z]).z)).collect();
            }
        }
        if g.levels.is_empty() {
            let mut by: HashMap<&str, f32> = HashMap::new();
            for e in &m.elements {
                if !e.storey.is_empty() && e.aabb.is_valid() {
                    let z = by.entry(e.storey.as_str()).or_insert(f32::INFINITY);
                    *z = z.min(e.aabb.min.z);
                }
            }
            g.levels = by.into_iter().filter(|(_, z)| z.is_finite()).map(|(n, z)| (n.to_string(), z)).collect();
        }
        g.levels.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        let g = Arc::new(g);
        self.grid_cache.borrow_mut().insert(slot, (key, g.clone()));
        g
    }

    /// Axele si nivelurile tuturor modelelor (nivelurile cu acelasi nume o singura data, axele identice o data).
    pub fn scene_grids(&self) -> GridData {
        let mut all = GridData::default();
        let custom = self.use_user_levels && !self.user_levels.is_empty();
        let my_grids = self.use_user_grids && !self.user_grids.is_empty();
        if my_grids {
            let o = self.origin_f64();
            all.grids = self.user_grids.iter().map(|g| (g.name.clone(), Vec3::new((g.a[0] - o.x) as f32, (g.a[1] - o.y) as f32, 0.0), Vec3::new((g.b[0] - o.x) as f32, (g.b[1] - o.y) as f32, 0.0))).collect();
        }
        if custom {
            let oz = self.origin_f64().z;
            all.levels = self.user_levels.iter().map(|l| (l.name.clone(), (self.level_zero + l.elev - oz) as f32)).collect();
        }
        for (slot, _) in self.loaded_models() {
            let g = self.grid_data(slot);
            for x in g.grids.iter().filter(|_| !my_grids) {
                if !all.grids.iter().any(|y| y.0 == x.0 && (y.1 - x.1).length() < 0.05 && (y.2 - x.2).length() < 0.05) {
                    all.grids.push(x.clone());
                }
            }
            for l in g.levels.iter().filter(|_| !custom) {
                if !all.levels.iter().any(|y| y.0 == l.0) {
                    all.levels.push(l.clone());
                }
            }
        }
        all.levels.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        all
    }

    /// Locatia unui punct (coordonate locale): intersectia de axe cea mai apropiata si nivelul, ex. "C-4 / Level 2".
    pub fn grid_location(&self, p: Vec3) -> String {
        let g = self.scene_grids();
        grid_location_in(&g, p)
    }
}

/// Locatia in axe (doua axe neparalele cele mai apropiate in plan) + nivelul de sub punct.
pub fn grid_location_in(g: &GridData, p: Vec3) -> String {
    let dist = |a: Vec3, b: Vec3| {
        let (a2, b2, p2) = (a.truncate(), b.truncate(), p.truncate());
        let d = b2 - a2;
        let l = d.length().max(1e-6);
        ((p2 - a2).perp_dot(d) / l).abs()
    };
    let mut lines: Vec<(f32, &String, glam::Vec2)> = g.grids.iter().map(|(t, a, b)| (dist(*a, *b), t, (*b - *a).truncate().normalize_or_zero())).collect();
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut grid = String::new();
    if let Some(first) = lines.first() {
        // a doua axa: cea mai apropiata care nu e (aproape) paralela cu prima
        match lines.iter().skip(1).find(|l| l.2.dot(first.2).abs() < 0.87) {
            Some(second) => {
                // litera inaintea cifrei, ca pe planse (C-4, nu 4-C)
                let (x, y) = if first.1.chars().next().map_or(false, |c| c.is_ascii_digit()) && !second.1.chars().next().map_or(false, |c| c.is_ascii_digit()) { (second.1, first.1) } else { (first.1, second.1) };
                grid = format!("{x}-{y}");
            }
            None => grid = first.1.clone(),
        }
    }
    let level = g.levels.iter().rev().find(|l| l.1 <= p.z + 0.05).or(g.levels.first()).map(|l| l.0.clone()).unwrap_or_default();
    match (grid.is_empty(), level.is_empty()) {
        (false, false) => format!("{grid} / {level}"),
        (false, true) => grid,
        (true, false) => level,
        (true, true) => "-".into(),
    }
}

/// Nivel definit de utilizator: nume si cota (m) fata de ±0.00 al proiectului.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserLevel {
    pub name: String,
    pub elev: f64,
}

pub fn yes() -> bool {
    true
}
impl App {
    /// Axele si nivelurile din modele (fara nivelurile utilizatorului).
    pub fn scene_grids_models(&self) -> GridData {
        let mut all = GridData::default();
        for (slot, _) in self.loaded_models() {
            let g = self.grid_data(slot);
            for l in &g.levels {
                if !all.levels.iter().any(|y| y.0 == l.0) {
                    all.levels.push(l.clone());
                }
            }
            all.grids.extend(g.grids.iter().cloned());
        }
        all.levels.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        all
    }
}

// ------------------------------------------------------------------ salvare automata
/// %LOCALAPPDATA%\ManageBIM\Autosave
pub fn autosave_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| std::path::PathBuf::from(d).join("ManageBIM").join("Autosave"))
}

/// Starea salvarii automate: lock-ul sesiunii (tinut deschis cat ruleaza aplicatia) si ce se poate recupera.
#[derive(Default)]
pub struct Autosave {
    pub lock: Option<std::fs::File>,
    pub last: Option<Instant>,
    pub sig: u64,
    /// (fisierul salvat automat, proiectul original daca exista, data)
    pub recover: Option<(String, Option<String>, String)>,
}

impl Autosave {
    /// La pornire: un lock ramas de la o sesiune care nu s-a inchis normal = acea sesiune s-a oprit brusc.
    /// Lock-urile sesiunilor care inca ruleaza nu se pot sterge (fisierul e deschis), deci raman neatinse.
    pub fn start() -> Autosave {
        let mut a = Autosave::default();
        let Some(dir) = autosave_dir() else { return a };
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                let Some(pid) = n.strip_prefix("session-").and_then(|s| s.strip_suffix(".lock")) else { continue };
                if std::fs::remove_file(e.path()).is_ok() {
                    let save = dir.join(format!("autosave-{pid}.bimr"));
                    if save.is_file() && a.recover.is_none() {
                        let orig = std::fs::read_to_string(dir.join(format!("autosave-{pid}.txt"))).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                        let when = std::fs::metadata(&save).and_then(|m| m.modified()).map(crate::app::fmt_time).unwrap_or_default();
                        a.recover = Some((save.to_string_lossy().to_string(), orig, when));
                    }
                }
            }
        }
        a.lock = std::fs::File::create(dir.join(format!("session-{}.lock", std::process::id()))).ok();
        a
    }

    /// La iesirea normala: lock-ul si salvarea automata a sesiunii se sterg.
    pub fn finish(&mut self) {
        self.lock = None;
        if let Some(dir) = autosave_dir() {
            let pid = std::process::id();
            for f in [format!("session-{pid}.lock"), format!("autosave-{pid}.bimr"), format!("autosave-{pid}.txt")] {
                let _ = std::fs::remove_file(dir.join(f));
            }
        }
    }
}

/// Data locala scurta (pentru mesaje).
pub fn fmt_time(t: std::time::SystemTime) -> String {
    let secs = t.elapsed().map(|d| d.as_secs()).unwrap_or(0);
    match secs {
        0..=89 => "just now".into(),
        90..=5399 => format!("{} minutes ago", (secs + 30) / 60),
        5400..=172_799 => format!("{} hours ago", (secs + 1800) / 3600),
        _ => format!("{} days ago", secs / 86_400),
    }
}

impl App {
    /// La fiecare 3 minute, daca proiectul s-a schimbat: copie in Autosave (fisierul .bimr al utilizatorului ramane neatins).
    pub fn autosave_tick(&mut self) {
        if self.loaded_models().next().is_none() || !self.loads.is_empty() {
            return;
        }
        let due = self.autosave.last.map_or(true, |t| t.elapsed().as_secs() >= 180);
        if !due {
            return;
        }
        self.autosave.last = Some(Instant::now());
        let Ok(json) = serde_json::to_string(&self.project_file()) else { return };
        let sig = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            json.hash(&mut h);
            h.finish() | 1
        };
        if sig == self.autosave.sig {
            return;
        }
        let Some(dir) = autosave_dir() else { return };
        let pid = std::process::id();
        let (f, tmp) = (dir.join(format!("autosave-{pid}.bimr")), dir.join(format!("autosave-{pid}.bimr.tmp")));
        if std::fs::write(&tmp, json).is_ok() && std::fs::rename(&tmp, &f).is_ok() {
            let _ = std::fs::write(dir.join(format!("autosave-{pid}.txt")), self.project_path.clone().unwrap_or_default());
            self.autosave.sig = sig;
        }
    }

    /// Deschide proiectul salvat automat; salvarea normala merge apoi in proiectul original (daca exista).
    pub fn autosave_recover(&mut self) {
        let Some((file, orig, _)) = self.autosave.recover.take() else { return };
        self.load_project(&file);
        self.project_path = orig.clone();
        let _ = std::fs::remove_file(&file);
        self.toast(match orig {
            Some(p) => format!("Recovered. Save (Ctrl+S) to keep the changes in {}.", crate::file_name(&p)),
            None => "Recovered. Use Save As to keep the project.".to_string(),
        });
    }

    pub fn autosave_discard(&mut self) {
        if let Some((file, _, _)) = self.autosave.recover.take() {
            let _ = std::fs::remove_file(&file);
            let _ = std::fs::remove_file(std::path::Path::new(&file).with_extension("txt"));
        }
    }
}

/// Axa creata in ManageBIM: eticheta si capetele in coordonate reale (metri, XY).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserGrid {
    pub name: String,
    pub a: [f64; 2],
    pub b: [f64; 2],
}

pub fn grid_bubble_default() -> f32 {
    11.0
}
pub fn grid_ext_default() -> f32 {
    2.0
}

/// Eticheta numarul `i` (0..) in stilul A, B, … Z, AA, AB … sau 1, 2, 3 …
pub fn grid_label(first: &str, i: usize) -> String {
    let f = first.trim();
    if let Ok(n) = f.parse::<i64>() {
        return (n + i as i64).to_string();
    }
    let start = f.chars().next().filter(|c| c.is_ascii_alphabetic()).map(|c| c.to_ascii_uppercase() as usize - 'A' as usize).unwrap_or(0);
    let mut k = start + i;
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (k % 26) as u8) as char);
        if k < 26 {
            break;
        }
        k = k / 26 - 1;
    }
    s
}

/// Retea de axe: `nx` axe verticale (de-a lungul lui Y) la distanta `dx`, etichete de la `fx`; `ny` axe orizontale
/// (de-a lungul lui X) la `dy`, etichete de la `fy`. Originea = intersectia primelor axe; `ext` = cat ies liniile.
pub fn generate_grid(origin: [f64; 2], nx: usize, dx: f64, fx: &str, ny: usize, dy: f64, fy: &str, ext: f64) -> Vec<UserGrid> {
    let (lx, ly) = (dx * nx.saturating_sub(1) as f64, dy * ny.saturating_sub(1) as f64);
    let mut out = Vec::new();
    for i in 0..nx {
        let x = origin[0] + dx * i as f64;
        out.push(UserGrid { name: grid_label(fx, i), a: [x, origin[1] - ext], b: [x, origin[1] + ly + ext] });
    }
    for j in 0..ny {
        let y = origin[1] + dy * j as f64;
        out.push(UserGrid { name: grid_label(fy, j), a: [origin[0] - ext, y], b: [origin[0] + lx + ext, y] });
    }
    out
}

/// Axele ca DXF (R12, metri): linii pe stratul GRID, bulele (cerc + text) pe stratul GRID_TAGS.
pub fn grid_dxf(grids: &[UserGrid], bubble_r: f64) -> String {
    let mut s = String::from("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n6\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n2\n");
    for (name, col) in [("GRID", 1), ("GRID_TAGS", 7)] {
        s.push_str(&format!("0\nLAYER\n2\n{name}\n70\n0\n62\n{col}\n6\nCONTINUOUS\n"));
    }
    s.push_str("0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    for g in grids {
        let (a, b) = (glam::DVec2::from(g.a), glam::DVec2::from(g.b));
        let d = (b - a).normalize_or_zero();
        // ca pe plansa: linia intre capete, bulele in prelungire (tangente la capete)
        s.push_str(&format!("0\nLINE\n8\nGRID\n10\n{}\n20\n{}\n30\n0.0\n11\n{}\n21\n{}\n31\n0.0\n", a.x, a.y, b.x, b.y));
        for c in [a - d * bubble_r, b + d * bubble_r] {
            s.push_str(&format!("0\nCIRCLE\n8\nGRID_TAGS\n10\n{}\n20\n{}\n30\n0.0\n40\n{}\n", c.x, c.y, bubble_r));
            // text centrat in bula (72=1 centru, 73=2 mijloc)
            s.push_str(&format!("0\nTEXT\n8\nGRID_TAGS\n10\n{x}\n20\n{y}\n30\n0.0\n40\n{h}\n1\n{t}\n72\n1\n73\n2\n11\n{x}\n21\n{y}\n31\n0.0\n", x = c.x, y = c.y, h = bubble_r * 0.9, t = g.name));
        }
    }
    s.push_str("0\nENDSEC\n0\nEOF\n");
    s
}

impl App {
    /// Proiect nou, gol: aplicatia trece de pagina de start, Save (Ctrl+S) cere unde se salveaza.
    pub fn new_project(&mut self) {
        self.reset();
        self.project_path = None;
        self.blank_project = true;
        self.toast("New project - add models with Open / Append; save it with Save (Ctrl+S).");
    }

    /// Axele folosite acum (ale tale sau din modele), in coordonate reale - pentru editor / export.
    pub fn grids_world(&self) -> Vec<UserGrid> {
        if self.use_user_grids && !self.user_grids.is_empty() {
            return self.user_grids.clone();
        }
        let o = self.origin_f64();
        self.scene_grids_models().grids.iter().map(|(n, a, b)| UserGrid { name: n.clone(), a: [a.x as f64 + o.x, a.y as f64 + o.y], b: [b.x as f64 + o.x, b.y as f64 + o.y] }).collect()
    }

    /// Export DXF; daca extensia e .dwg si ODA File Converter e instalat, DXF-ul se converteste in DWG.
    pub fn export_grids(&mut self, path: &str) {
        let grids = self.grids_world();
        if grids.is_empty() {
            return self.error("No grid axes: create them in View > Edit Grids, or open a model with grids.");
        }
        let dwg = path.to_lowercase().ends_with(".dwg");
        let dxf_path = if dwg { std::path::Path::new(path).with_extension("dxf").to_string_lossy().to_string() } else { with_ext(path, "dxf") };
        if let Err(e) = std::fs::write(&dxf_path, grid_dxf(&grids, 0.5)) {
            return self.error(format!("Cannot write {dxf_path}: {e}"));
        }
        if !dwg {
            return self.toast(format!("{} grid axes exported: {}", grids.len(), crate::file_name(&dxf_path)));
        }
        match crate::formats::dxf_to_dwg(&dxf_path) {
            Ok(out) => self.toast(format!("{} grid axes exported: {}", grids.len(), crate::file_name(&out))),
            Err(e) => self.toast(format!("Saved as DXF ({}) - {e}", crate::file_name(&dxf_path))),
        }
    }

    /// Sectiune pe nivel: box de la nivel pana la nivelul urmator (tot planul XY al scenei).
    pub fn section_level_box(&mut self, li: usize) {
        let g = self.scene_grids();
        let bb = self.scene_aabb();
        let Some(l) = g.levels.get(li).cloned() else { return };
        if !bb.is_valid() {
            return;
        }
        let top = g.levels.get(li + 1).map(|n| n.1).unwrap_or(bb.max.z + 0.01);
        self.section.enabled = true;
        self.section.mode = SectionMode::Box;
        // marginile: sub nivel (ex. placa de sub) si sub nivelul urmator (ex. fara placa de deasupra)
        self.section.bmin = [bb.min.x - 0.5, bb.min.y - 0.5, l.1 - self.level_below.max(0.0) - 0.01];
        self.section.bmax = [bb.max.x + 0.5, bb.max.y + 0.5, top + self.level_above - 0.02];
        self.section_level = Some(li);
        self.colors_dirty = true;
        self.toast(format!("Section: {} (to {})", l.0, g.levels.get(li + 1).map(|n| n.0.clone()).unwrap_or_else(|| "the top".into())));
    }

    /// Nivelul de deasupra / dedesubt (PgUp / PgDn sau ◀ ▶), pastrand tipul sectiunii (box de nivel sau plan).
    pub fn section_level_step(&mut self, delta: i32) {
        let n = self.scene_grids().levels.len();
        if n == 0 {
            return;
        }
        let cur = self.section_level.unwrap_or(0) as i32;
        let li = (cur + delta).clamp(0, n as i32 - 1) as usize;
        if self.section.enabled && self.section.mode == SectionMode::Planes {
            self.section_plan_cut(li, self.plan_cut_h);
        } else {
            self.section_level_box(li);
        }
    }

    /// Doar elementele nivelului (dupa numele nivelului din model; altfel dupa inaltime), restul ascunse.
    pub fn isolate_level(&mut self, li: usize) {
        let g = self.scene_grids();
        let Some(l) = g.levels.get(li).cloned() else { return };
        let top = g.levels.get(li + 1).map(|n| n.1).unwrap_or(f32::INFINITY);
        let by_name = self.loaded_models().any(|(_, m)| m.elements.iter().any(|e| e.storey == l.0));
        let mut keep = Vec::new();
        for (mi, m) in self.loaded_models() {
            for (ei, e) in m.elements.iter().enumerate() {
                let ok = if by_name { e.storey == l.0 } else { e.aabb.is_valid() && { let z = (e.aabb.min.z + e.aabb.max.z) * 0.5; z >= l.1 - 0.01 && z < top } };
                if ok {
                    keep.push(ElemId { m: mi as u32, e: ei as u32 });
                }
            }
        }
        let n = keep.len();
        self.apply(Action::Isolate(keep));
        self.section_level = Some(li);
        self.toast(format!("{}: {n} items shown, the rest hidden (Unhide All to undo)", l.0));
    }

    /// Sectiune tip plan: totul de deasupra lui (nivel + h) e taiat.
    pub fn section_plan_cut(&mut self, li: usize, h: f32) {
        let g = self.scene_grids();
        let Some(l) = g.levels.get(li) else { return };
        self.section.enabled = true;
        self.section.mode = SectionMode::Planes;
        self.section.planes = vec![PlaneDef { enabled: true, dir: 0, offset: l.1 + h }];
        self.section_level = Some(li);
        self.plan_cut_h = h;
        self.colors_dirty = true;
        self.toast(format!("Plan cut: {} + {h:.2} m", l.0));
    }
}

/// Parametrii generatorului de axe: originea (real, m), rotatia, numarul / distanta / prima eticheta pe fiecare directie.
#[derive(Clone, Debug)]
pub struct GridGen {
    pub origin: [f64; 2],
    pub angle_deg: f64,
    pub nx: usize,
    pub dx: f64,
    pub fx: String,
    pub ny: usize,
    pub dy: f64,
    pub fy: String,
}

impl Default for GridGen {
    fn default() -> Self {
        GridGen { origin: [0.0, 0.0], angle_deg: 0.0, nx: 5, dx: 6.0, fx: "1".into(), ny: 4, dy: 6.0, fy: "A".into() }
    }
}

/// Roteste axele cu `deg` in jurul punctului `c` si le muta cu `d` (real, m).
pub fn transform_grids(grids: &mut [UserGrid], c: [f64; 2], deg: f64, d: [f64; 2]) {
    let (s, co) = deg.to_radians().sin_cos();
    let rot = |p: [f64; 2]| {
        let (x, y) = (p[0] - c[0], p[1] - c[1]);
        [c[0] + x * co - y * s + d[0], c[1] + x * s + y * co + d[1]]
    };
    for g in grids {
        g.a = rot(g.a);
        g.b = rot(g.b);
    }
}

impl App {
    /// Genereaza reteaua din `grid_gen` (cu rotatie) ca axele utilizatorului.
    pub fn generate_user_grid(&mut self) {
        let g = self.grid_gen.clone();
        let mut out = generate_grid(g.origin, g.nx, g.dx, &g.fx, g.ny, g.dy, &g.fy, self.grid_ext as f64);
        transform_grids(&mut out, g.origin, g.angle_deg, [0.0, 0.0]);
        self.user_grids = out;
        self.use_user_grids = true;
        self.show_grids = true;
    }

    /// Clic in 3D in timpul alegerii: 1 = originea (apoi directia), 2 = directia -> reteaua se regenereaza.
    pub fn grid_pick_point(&mut self, local: Vec3) {
        let o = self.origin_f64();
        let w = [local.x as f64 + o.x, local.y as f64 + o.y];
        match self.grid_pick {
            1 => {
                self.grid_gen.origin = [(w[0] * 1000.0).round() / 1000.0, (w[1] * 1000.0).round() / 1000.0];
                self.grid_pick = 2;
                self.toast("Grid origin set. Now click a point along the first axis direction (Esc: keep the current angle).");
            }
            2 => {
                let (dx, dy) = (w[0] - self.grid_gen.origin[0], w[1] - self.grid_gen.origin[1]);
                if dx.hypot(dy) > 0.01 {
                    self.grid_gen.angle_deg = (dy.atan2(dx).to_degrees() * 100.0).round() / 100.0;
                }
                self.grid_pick = 0;
                self.generate_user_grid();
                self.toast(format!("Grid aligned: origin {:.3}, {:.3} · angle {:.2}°", self.grid_gen.origin[0], self.grid_gen.origin[1], self.grid_gen.angle_deg));
            }
            _ => {}
        }
    }
}

// ------------------------------------------------------------------ .bimd (un singur fisier, ca .nwd)
/// Folderul in care se despacheteaza fisierele .bimd deschise.
pub fn published_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| std::path::PathBuf::from(d).join("ManageBIM").join("Published"))
}

impl App {
    /// Publica proiectul intr-un singur fisier .bimd (ZIP): project.bimr + fiecare model ca .bimc
    /// (norii de puncte cu fisierul original). Se deschide pe orice PC, fara fisierele sursa.
    pub fn publish_bimd(&mut self, path: &str) -> Result<String, String> {
        let path = with_ext(path, "bimd");
        let origin = self.origin_f64();
        let tmp = std::env::temp_dir().join(format!("mb_publish_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let mut zip = crate::markup::Zip::default();
        let mut pf = self.project_file();
        pf.models_rel = Vec::new();
        let mut total: u64 = 0;
        let mut n = 0;
        for slot in 0..self.models.len() {
            let Some(m) = self.models[slot].clone() else {
                if let Some(p) = pf.models.get_mut(slot) {
                    p.clear();
                }
                continue;
            };
            let stem: String = std::path::Path::new(&m.path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| format!("model{slot}")).chars().map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
            let (name, data) = if m.cloud.is_some() {
                // nor de puncte: fisierul original (E57 / LAS)
                let ext = std::path::Path::new(&m.path).extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_else(|| "e57".into());
                (format!("models/{slot}_{stem}.{ext}"), std::fs::read(&m.path).map_err(|e| format!("{}: {e}", m.name))?)
            } else {
                // geometria originala (fara transformarea modelului - ea ramane in proiect)
                let base = self.base_models.get(slot).cloned().flatten().unwrap_or(m.clone());
                let raw = crate::bimc::from_model(&base, origin);
                let f = tmp.join(format!("{slot}.bimc"));
                crate::bimc::write(&raw, &f.to_string_lossy()).map_err(|e| format!("{}: {e}", m.name))?;
                (format!("models/{slot}_{stem}.bimc"), std::fs::read(&f).map_err(|e| e.to_string())?)
            };
            total += data.len() as u64;
            if total > 3_900_000_000 {
                let _ = std::fs::remove_dir_all(&tmp);
                return Err("The models are larger than 4 GB together - .bimd cannot hold them (save a .bimr project instead).".into());
            }
            zip.add(&name, &data);
            if let Some(p) = pf.models.get_mut(slot) {
                *p = name;
            }
            n += 1;
        }
        let _ = std::fs::remove_dir_all(&tmp);
        let json = serde_json::to_string_pretty(&pf).map_err(|e| e.to_string())?;
        zip.add("project.bimr", json.as_bytes());
        let bytes = zip.finish();
        let part = format!("{path}.part");
        std::fs::write(&part, &bytes).map_err(|e| format!("Cannot write {path}: {e}"))?;
        std::fs::rename(&part, &path).map_err(|e| format!("Cannot write {path}: {e}"))?;
        self.settings.add_recent(&path);
        Ok(format!("Published {} ({n} models, {:.1} MB)", crate::file_name(&path), bytes.len() as f64 / 1e6))
    }

    /// Deschide un .bimd: se despacheteaza in %LOCALAPPDATA%\ManageBIM\Published\<nume>, apoi proiectul se incarca
    /// de acolo. Save scrie din nou in .bimd.
    pub fn open_bimd(&mut self, path: &str) {
        let r = (|| -> Result<std::path::PathBuf, String> {
            let data = std::fs::read(path).map_err(|e| format!("Cannot read {path}: {e}"))?;
            let zip = crate::fcstd::Zip::parse(data).map_err(|_| format!("{} is not a valid .bimd file.", crate::file_name(path)))?;
            let stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            let stem = std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "project".into());
            let dir = published_dir().ok_or("LOCALAPPDATA not set")?.join(format!("{stem}-{stamp}"));
            std::fs::create_dir_all(dir.join("models")).map_err(|e| e.to_string())?;
            let txt = zip.text("project.bimr").ok_or("project.bimr missing in the .bimd file")?;
            let mut pf: ProjectFile = serde_json::from_str(&txt).map_err(|e| format!("Invalid project in the .bimd file: {e}"))?;
            for m in pf.models.iter_mut().filter(|m| !m.is_empty()) {
                if m.contains("..") {
                    return Err(format!("invalid path in the .bimd file: {m}"));
                }
                let out = dir.join(m.replace('/', "\\"));
                // despachetat o singura data (acelasi .bimd = acelasi folder)
                if !out.is_file() {
                    let data = zip.read(m).ok_or_else(|| format!("{m} missing in the .bimd file"))?;
                    std::fs::write(&out, data).map_err(|e| e.to_string())?;
                }
                *m = out.to_string_lossy().to_string();
            }
            let bimr = dir.join("project.bimr");
            std::fs::write(&bimr, serde_json::to_string_pretty(&pf).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            Ok(bimr)
        })();
        match r {
            Ok(bimr) => {
                self.load_project(&bimr.to_string_lossy());
                self.project_path = None;
                self.bimd_path = Some(path.to_string());
                self.settings.add_recent(path);
            }
            Err(e) => self.error(e),
        }
    }
}

impl App {
    /// Inapoi la pagina de start: proiectul se inchide (salvarea automata pastreaza o copie).
    pub fn close_to_start(&mut self) {
        self.reset();
        self.project_path = None;
        self.blank_project = false;
        self.confirm_start = false;
    }
}

/// Actiune pornita din pagina de start cand un proiect e deja deschis (dupa intrebarea "salvezi?").
#[derive(Clone, Debug, PartialEq)]
pub enum StartAction {
    New,
    Open(String),
    OpenModelDialog,
    OpenProjectDialog,
    /// doar inchide proiectul (ramane pagina de start)
    Close,
}

impl App {
    /// E ceva deschis (modele, proiect nou sau issue-uri) care s-ar pierde?
    pub fn has_open_project(&self) -> bool {
        self.loaded_models().next().is_some() || !self.loads.is_empty() || self.blank_project || self.project_path.is_some() || self.bimd_path.is_some() || !self.viewpoints.is_empty()
    }

    /// Din pagina de start: cu un proiect deschis se intreaba intai daca se salveaza.
    pub fn start_action(&mut self, a: StartAction) {
        if self.has_open_project() {
            self.start_pending = Some(a);
            self.confirm_start = true;
        } else {
            self.run_start_action(a);
        }
    }

    pub fn run_start_action(&mut self, a: StartAction) {
        self.confirm_start = false;
        self.start_pending = None;
        match a {
            StartAction::New => {
                self.new_project();
                self.show_start = false;
            }
            StartAction::Open(p) => {
                self.close_to_start();
                self.open_path(&p, false);
                self.show_start = false;
            }
            StartAction::OpenModelDialog => {
                self.close_to_start();
                self.show_start = false;
                self.request_path(Purpose::OpenModel { append: false });
            }
            StartAction::OpenProjectDialog => {
                self.close_to_start();
                self.show_start = false;
                self.request_path(Purpose::OpenProject);
            }
            StartAction::Close => {
                self.close_to_start();
                self.show_start = true;
            }
        }
    }
}

// ------------------------------------------------------------------ relocare (ca Navisworks: masurare -> transformare)
/// Ce se muta: modelul punctului ales sau elementele selectate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum XformMode {
    /// punct -> punct (translatie)
    Move,
    /// pivot, directia de pornire, directia tinta (rotatie in jurul verticalei)
    Rotate,
    /// fata elementului de mutat -> fata tinta (rotatie in plan + lipire pe fata)
    Align,
}

impl XformMode {
    pub fn needed(self) -> usize {
        match self {
            XformMode::Move | XformMode::Align => 2,
            XformMode::Rotate => 3,
        }
    }
    pub fn hint(self, k: usize) -> &'static str {
        match (self, k) {
            (XformMode::Move, 0) => "Move: click the point to move FROM (snaps to corners)",
            (XformMode::Move, _) => "Move: click the point to move TO",
            (XformMode::Rotate, 0) => "Rotate: click the pivot point",
            (XformMode::Rotate, 1) => "Rotate: click a point giving the current direction",
            (XformMode::Rotate, _) => "Rotate: click a point giving the new direction",
            (XformMode::Align, 0) => "Align: click a face of the item to move",
            (XformMode::Align, _) => "Align: click the target face (the first face is placed against it)",
        }
    }
}

#[derive(Clone, Debug)]
pub struct XformPick {
    pub mode: XformMode,
    /// true = elementele selectate (doar translatie), false = modelul primului punct
    pub selection: bool,
    /// (punct, normala fetei, elementul atins)
    pub pts: Vec<(Vec3, Vec3, ElemId)>,
}

/// Rotatia in jurul verticalei care duce directia orizontala `a` peste `b` (grade).
pub fn z_angle(a: Vec3, b: Vec3) -> Option<f32> {
    let (a2, b2) = (a.truncate(), b.truncate());
    if a2.length() < 1e-4 || b2.length() < 1e-4 {
        return None;
    }
    Some(a2.angle_to(b2).to_degrees())
}

impl App {
    pub fn xform_start(&mut self, mode: XformMode, selection: bool) {
        self.xform = Some(XformPick { mode, selection, pts: Vec::new() });
        self.grid_pick = 0;
    }

    /// Un punct ales in 3D (cu normala fetei); la ultimul punct se aplica transformarea.
    pub fn xform_point(&mut self, p: Vec3, n: Vec3, id: ElemId) {
        let Some(mut x) = self.xform.take() else { return };
        x.pts.push((p, n, id));
        if x.pts.len() < x.mode.needed() {
            self.xform = Some(x);
            return;
        }
        let r = self.xform_apply(&x);
        match r {
            Ok(msg) => self.toast(msg),
            Err(e) => self.error(e),
        }
    }

    fn xform_apply(&mut self, x: &XformPick) -> Result<String, String> {
        let p = &x.pts;
        // (unghi in grade in jurul verticalei prin pivot, translatie)
        let (angle, pivot, delta) = match x.mode {
            XformMode::Move => (0.0, p[0].0, p[1].0 - p[0].0),
            XformMode::Rotate => {
                let a = z_angle(p[1].0 - p[0].0, p[2].0 - p[0].0).ok_or("Rotate: the direction points are too close to the pivot.")?;
                (a, p[0].0, Vec3::ZERO)
            }
            XformMode::Align => {
                let (pa, na) = (p[0].0, p[0].1.normalize_or_zero());
                let (pb, nb) = (p[1].0, p[1].1.normalize_or_zero());
                // fata de mutat ajunge cu spatele pe fata tinta: normala ei = -normala tintei (rotatie doar in plan)
                let ang = if na.truncate().length() > 0.3 && nb.truncate().length() > 0.3 { z_angle(na, -nb).unwrap_or(0.0) } else { 0.0 };
                let rot = Mat4::from_rotation_z(ang.to_radians());
                let pa_rot = pivot_rotate(pa, pa, &rot);
                let na_rot = rot.transform_vector3(na);
                // dupa rotatie (in jurul lui pa), lipim fata: miscam pe normala tintei pana pe planul ei
                let dist = (pb - pa_rot).dot(nb);
                let d = if na_rot.dot(nb).abs() > 0.5 { nb * dist } else { pb - pa_rot };
                (ang, pa, d)
            }
        };
        if x.selection {
            if angle.abs() > 1e-4 {
                return Err("Selected items can only be moved - rotate / align whole models (choose \"Model\").".into());
            }
            if self.selected.is_empty() {
                return Err("Nothing selected.".into());
            }
            self.move_selected(delta);
            return Ok(format!("Moved {} items by {}", self.selected.len(), fmt_vec_len(delta)));
        }
        let slot = p[0].2.m as usize;
        let Some(base) = self.base_models.get(slot).cloned().flatten() else { return Err("The model is not loaded.".into()) };
        let mut xf = self.model_xf.get(slot).copied().unwrap_or_default();
        let piv_m = if xf.pivot_origin { (-self.origin_f64()).as_vec3() } else { base.aabb.center() };
        // M' = T(delta) * T(P) Rz(a) T(-P) * M  ->  rot_z += a, (pivot + t') = P + Rz(a)(pivot + t - P) + delta
        let r = Mat4::from_rotation_z(angle.to_radians());
        let t = Vec3::new(xf.translate[0] as f32, xf.translate[1] as f32, xf.translate[2] as f32);
        let new_pt = pivot + r.transform_vector3(piv_m + t - pivot) + delta;
        let t2 = new_pt - piv_m;
        xf.translate = [t2.x as f64, t2.y as f64, t2.z as f64];
        xf.rot_z = ((xf.rot_z + angle as f64) * 1e6).round() / 1e6;
        let name = base.name.clone();
        self.set_model_xf(slot, xf);
        Ok(match x.mode {
            XformMode::Move => format!("{name} moved by {}", fmt_vec_len(delta)),
            XformMode::Rotate => format!("{name} rotated by {angle:.3}°"),
            XformMode::Align => format!("{name} aligned: rotated {angle:.3}°, moved {}", fmt_vec_len(delta)),
        })
    }
}

fn pivot_rotate(p: Vec3, pivot: Vec3, rot: &Mat4) -> Vec3 {
    pivot + rot.transform_vector3(p - pivot)
}

fn fmt_vec_len(d: Vec3) -> String {
    format!("{} (ΔX {} ΔY {} ΔZ {})", fmt_len(d.length() as f64), fmt_num(d.x as f64), fmt_num(d.y as f64), fmt_num(d.z as f64))
}

impl App {
    /// Axe din DWG / DXF: devin axele mele (se pot muta / roti / edita apoi). `layer` = alt strat decat cel ghicit.
    pub fn import_grids_cad(&mut self, path: &str, layer: Option<&str>) {
        match crate::formats::grids_from_cad(path, layer) {
            Ok(g) if g.grids.is_empty() => {
                // stratul axelor nu s-a putut ghici: se alege din lista in fereastra Grids
                self.grid_import = Some((path.to_string(), g.layers, String::new()));
                self.show_grids_editor = true;
                self.toast("No layer named GRID / AXE / AX in the drawing - choose the layer with the grid lines in the Grids window.");
            }
            Ok(g) => {
                let n = g.grids.len();
                self.user_grids = g.grids.into_iter().map(|(name, a, b)| UserGrid { name, a, b }).collect();
                self.use_user_grids = true;
                self.show_grids = true;
                self.grid_import = Some((path.to_string(), g.layers, g.layer.clone()));
                self.toast(format!("{n} grid axes imported from {} (layer {}, units x{})", crate::file_name(path), g.layer, g.scale));
            }
            Err(e) => self.error(e),
        }
    }
}

// ------------------------------------------------------------------ referinta pentru Revit / AutoCAD (.mbref)
/// Format simplu, citit de add-in-uri: "MBREF1", originea (f64 x3, metri reali), numarul de elemente, apoi pe element:
/// nume, GUID, tip, nivel (u16 lungime + UTF-8), culoare RGBA, numar de triunghiuri, triunghiurile (f32 x9, metri fata de origine).
pub fn write_mbref(items: &[(&Model, &crate::model::Element)], origin: DVec3) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"MBREF1");
    for v in origin.to_array() {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&(items.len() as u32).to_le_bytes());
    let s = |b: &mut Vec<u8>, t: &str| {
        let t: String = t.chars().take(2000).collect();
        b.extend_from_slice(&(t.len() as u16).to_le_bytes());
        b.extend_from_slice(t.as_bytes());
    };
    for (m, e) in items {
        s(&mut b, &e.name);
        s(&mut b, &e.guid);
        s(&mut b, &e.ifc_type);
        s(&mut b, &e.storey);
        let cols = &m.tri_color[e.tri_start as usize..(e.tri_start + e.tri_count) as usize];
        let col = cols.first().copied().unwrap_or([200, 200, 200, 255]);
        b.extend_from_slice(&col);
        let tris = m.tris(e);
        b.extend_from_slice(&((tris.len() / 3) as u32).to_le_bytes());
        for v in tris {
            for c in [v.x, v.y, v.z] {
                b.extend_from_slice(&c.to_le_bytes());
            }
        }
    }
    b
}

impl App {
    /// Referinta pentru Revit / AutoCAD / Plant 3D: selectia, sau tot ce e vizibil.
    pub fn export_reference(&mut self, path: &str) {
        let path = with_ext(path, "mbref");
        let use_sel = !self.selected.is_empty();
        let models: Vec<(usize, Arc<Model>)> = self.loaded_models().map(|(i, m)| (i, m.clone())).collect();
        let mut items: Vec<(&Model, &crate::model::Element)> = Vec::new();
        for (mi, m) in &models {
            for (ei, e) in m.elements.iter().enumerate() {
                let id = ElemId { m: *mi as u32, e: ei as u32 };
                let take = if use_sel { self.selected.contains(&id) } else { !self.hidden.contains(&id) && !e.default_hidden };
                if take && e.tri_count > 0 {
                    items.push((m.as_ref(), e));
                }
            }
        }
        if items.is_empty() {
            return self.error("Nothing to export: no visible items with geometry.");
        }
        let n = items.len();
        let data = write_mbref(&items, self.origin_f64());
        match std::fs::write(&path, &data) {
            Ok(()) => self.toast(format!("Reference saved: {} ({n} items, {:.1} MB). Revit: ManageBIM › Reference; AutoCAD / Plant 3D: command MBREFERENCE.", crate::file_name(&path), data.len() as f64 / 1e6)),
            Err(e) => self.error(format!("Cannot write {path}: {e}")),
        }
    }
}

/// %LOCALAPPDATA%\ManageBIM\Outbox (referinte, issue-uri pentru add-in-uri)
pub fn outbox_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| std::path::PathBuf::from(d).join("ManageBIM").join("Outbox"))
}

// ------------------------------------------------------------------ matricea de clash (discipline / modele)
/// Clash-urile tuturor testelor, numarate pe perechi de modele: (modele, perechea (i <= j) -> [deschise, revizuite / aprobate, rezolvate]).
pub struct ClashMatrix {
    pub models: Vec<String>,
    pub cells: HashMap<(usize, usize), [usize; 3]>,
}

impl ClashMatrix {
    pub fn get(&self, i: usize, j: usize) -> [usize; 3] {
        self.cells.get(&(i.min(j), i.max(j))).copied().unwrap_or([0; 3])
    }
}

impl App {
    pub fn clash_matrix(&self) -> ClashMatrix {
        let mut models: Vec<String> = self.loaded_models().map(|(_, m)| crate::app::model_label(&m.name)).collect();
        models.dedup();
        let slot_ix: HashMap<u32, usize> = self.loaded_models().map(|(s, m)| (s as u32, models.iter().position(|n| *n == crate::app::model_label(&m.name)).unwrap_or(0))).collect();
        let mut cells: HashMap<(usize, usize), [usize; 3]> = HashMap::new();
        for t in &self.clash_tests {
            for r in &t.results {
                let (Some(&i), Some(&j)) = (slot_ix.get(&r.a.m), slot_ix.get(&r.b.m)) else { continue };
                let k = match r.status {
                    ClashStatus::New | ClashStatus::Active => 0,
                    ClashStatus::Reviewed | ClashStatus::Approved => 1,
                    ClashStatus::Resolved => 2,
                };
                cells.entry((i.min(j), i.max(j))).or_insert([0; 3])[k] += 1;
            }
        }
        ClashMatrix { models, cells }
    }

    /// Matricea in Excel: foaia "Open" (deschise) si foaia "All" (toate), modele pe randuri si coloane.
    pub fn export_clash_matrix(&mut self, path: &str) {
        use crate::qto::{Cell, Sheet};
        let m = self.clash_matrix();
        if m.cells.is_empty() {
            return self.error("No clash results yet - run the clash tests first.");
        }
        let mut headers = vec!["Model".to_string()];
        headers.extend(m.models.iter().cloned());
        headers.push("Total".into());
        let sheet = |name: &str, f: &dyn Fn([usize; 3]) -> usize| {
            let rows: Vec<Vec<Cell>> = (0..m.models.len())
                .map(|i| {
                    let mut r = vec![Cell::S(m.models[i].clone())];
                    let mut tot = 0;
                    for j in 0..m.models.len() {
                        let v = if j < i { 0 } else { f(m.get(i, j)) };
                        tot += v;
                        r.push(if j < i { Cell::Empty } else { Cell::N(v as f64) });
                    }
                    r.push(Cell::N(tot as f64));
                    r
                })
                .collect();
            Sheet { name: name.into(), title: format!("Clash matrix - {name} clashes (all tests)"), headers: headers.clone(), widths: std::iter::once(30.0).chain(std::iter::repeat(16.0)).take(headers.len()).collect(), rows, total_from: Some(1) }
        };
        let data = crate::qto::xlsx(&[sheet("Open", &|c| c[0]), sheet("All", &|c| c[0] + c[1] + c[2])]);
        let p = with_ext(path, "xlsx");
        match std::fs::write(&p, data) {
            Ok(()) => self.toast(format!("Clash matrix saved: {}", crate::file_name(&p))),
            Err(e) => self.error(format!("Cannot write {p}: {e}")),
        }
    }
}

/// Numele modelului fara extensie (pentru matrice / rapoarte).
pub fn model_label(name: &str) -> String {
    std::path::Path::new(name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| name.to_string())
}
