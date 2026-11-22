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

mod selection;
mod search;
mod project;
mod transforms;
mod analysis;
mod issues;
pub(crate) mod clash_logic;
mod anim;
mod materials;
mod history;
mod export;
mod walk_actions;
mod grids;
mod autosave;
mod sync;
mod userprops;

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
    let d = crate::formats::data_root()?.join("ManageBIM").join("Inbox");
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
    /// o disciplina (Structure, Architecture, MEP - HVAC...), recunoscuta din tipul IFC si numele modelului
    Discipline,
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
            SelKind::Discipline => format!("Discipline: {}", self.value),
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
    #[serde(default)]
    pub approved_by: String,
    #[serde(default)]
    pub approved_at: String,
    #[serde(default)]
    pub resolved_by: String,
    #[serde(default)]
    pub resolved_at: String,
    /// al doilea "element" e norul de puncte (Scan vs Model)
    #[serde(default)]
    pub b_is_scan: bool,
}

/// Navisworks-style rules: what the test must ignore.
#[derive(Clone, Debug, Serialize, Deserialize)]
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
    /// ignora conexiunile normale (perete-placa, grinda-stalp, teava-suport, izolatie...) cu patrundere mica
    #[serde(default)]
    pub normal_joints: bool,
    /// patrunderea maxima pentru o conexiune normala (mm)
    #[serde(default = "joint_default")]
    pub joint_mm: f32,
    /// responsabilul fiecarui clash nou = disciplina care se muta (structura > arhitectura > instalatii)
    #[serde(default)]
    pub auto_assign: bool,
}

fn joint_default() -> f32 {
    50.0
}

impl Default for ClashRules {
    fn default() -> Self {
        ClashRules { same_level: false, same_type: false, same_props: Vec::new(), same_sets: Vec::new(), exclude_sets: Vec::new(), exclude_rules: Vec::new(), normal_joints: true, joint_mm: joint_default(), auto_assign: true }
    }
}

impl ClashRules {
    pub fn count(&self, same_file: bool) -> usize {
        same_file as usize + self.same_level as usize + self.same_type as usize + self.normal_joints as usize + self.same_props.len() + self.same_sets.len() + self.exclude_sets.len() + self.exclude_rules.len()
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
    /// sistemul instalatiei (System Name / Classification)
    System,
    ModelA,
    ModelB,
    Assigned,
    Status,
}

impl GroupBy {
    pub const ALL: [GroupBy; 11] = [
        GroupBy::Grid, GroupBy::Level, GroupBy::System, GroupBy::ItemA, GroupBy::ItemB,
        GroupBy::ModelA, GroupBy::ModelB, GroupBy::Assigned, GroupBy::Status,
        GroupBy::TypePair, GroupBy::Proximity,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            GroupBy::Level => "Level",
            GroupBy::System => "System",
            GroupBy::ItemA => "Item A",
            GroupBy::ItemB => "Item B",
            GroupBy::TypePair => "Type A × Type B",
            GroupBy::Proximity => "Proximity (grid cell)",
            GroupBy::Grid => "Grid location (axes / level)",
            GroupBy::ModelA => "Model A",
            GroupBy::ModelB => "Model B",
            GroupBy::Assigned => "Assigned to",
            GroupBy::Status => "Status",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
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
    /// pragurile de gravitate (mm): sub primul = Minor, intre = Major, peste al doilea = Critical
    #[serde(default = "sev_default")]
    pub sev_mm: [f32; 2],
    /// amprenta configuratiei la ultima rulare (Status Old cand s-a schimbat ceva de atunci)
    #[serde(default)]
    pub run_key: u64,
}

fn sev_default() -> [f32; 2] {
    [10.0, 50.0]
}

/// Gravitatea unui clash, dupa patrundere (hard) sau cat de aproape sunt elementele (clearance).
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub enum Severity {
    Critical,
    Major,
    Minor,
}

impl Severity {
    pub const ALL: [Severity; 3] = [Severity::Critical, Severity::Major, Severity::Minor];
    pub fn label(self) -> &'static str {
        match self {
            Severity::Critical => "Critical",
            Severity::Major => "Major",
            Severity::Minor => "Minor",
        }
    }
    pub fn color(self) -> [u8; 3] {
        match self {
            Severity::Critical => [230, 60, 60],
            Severity::Major => [245, 150, 35],
            Severity::Minor => [235, 205, 70],
        }
    }
}

impl ClashTest {
    /// Gravitatea unui rezultat: hard = patrunderea (mm) fata de praguri; clearance = cat din distanta
    /// ceruta lipseste (atingere = Critical); duplicate = Critical.
    pub fn severity(&self, dist: f32) -> Severity {
        severity_of(self.ty, self.tol_mm, self.sev_mm, dist)
    }
}

/// ca `ClashTest::severity`, fara testul intreg (tipul, toleranta si pragurile).
pub fn severity_of(ty: ClashType, tol_mm: f32, sev_mm: [f32; 2], dist: f32) -> Severity {
    {
        let [lo, hi] = sev_mm;
        match ty {
            ClashType::Duplicates => Severity::Critical,
            ClashType::Hard => {
                let pen = (-dist * 1000.0).max(0.0);
                if pen >= hi {
                    Severity::Critical
                } else if pen >= lo {
                    Severity::Major
                } else {
                    Severity::Minor
                }
            }
            ClashType::Clearance => {
                let gap = dist * 1000.0;
                if gap <= 0.0 {
                    Severity::Critical
                } else if gap < tol_mm * 0.5 {
                    Severity::Major
                } else {
                    Severity::Minor
                }
            }
        }
    }
}

impl ClashTest {
    /// Teste gata facute (sabloane): (nume, descriere, test). Disciplinele se recunosc automat.
    pub fn templates() -> Vec<(&'static str, &'static str, ClashTest)> {
        let d = |v: &str| Selector { kind: SelKind::Discipline, value: v.to_string() };
        let all = || Selector { kind: SelKind::All, value: String::new() };
        let mk = |name: &str, a: Selector, b: Selector, ty: ClashType, tol: f32, sev: [f32; 2]| {
            let mut t = ClashTest::new(name.to_string());
            (t.a, t.b, t.ty, t.tol_mm, t.sev_mm) = (a, b, ty, tol, sev);
            t
        };
        vec![
            ("Structure vs HVAC (hard)", "Ducts through beams, columns, slabs - 10 mm tolerance; Critical over 50 mm", mk("Structure vs HVAC", d("Structure"), d("MEP - HVAC"), ClashType::Hard, 10.0, [10.0, 50.0])),
            ("Structure vs Plumbing (hard)", "Pipes through the structure - 10 mm tolerance", mk("Structure vs Plumbing", d("Structure"), d("MEP - Plumbing"), ClashType::Hard, 10.0, [10.0, 50.0])),
            ("Structure vs Electrical (hard)", "Cable trays through the structure - 10 mm tolerance", mk("Structure vs Electrical", d("Structure"), d("MEP - Electrical"), ClashType::Hard, 10.0, [10.0, 50.0])),
            ("Architecture vs HVAC (hard)", "Walls, doors, ceilings vs ducts - 25 mm tolerance (small touches ignored)", mk("Architecture vs HVAC", d("Architecture"), d("MEP - HVAC"), ClashType::Hard, 25.0, [25.0, 100.0])),
            ("Structure vs Architecture (hard)", "Walls and doors cutting into beams and columns - 20 mm", mk("Structure vs Architecture", d("Structure"), d("Architecture"), ClashType::Hard, 20.0, [20.0, 80.0])),
            ("HVAC vs Plumbing (clearance 50 mm)", "Ducts and pipes need 50 mm for insulation and installation", mk("HVAC vs Plumbing clearance", d("MEP - HVAC"), d("MEP - Plumbing"), ClashType::Clearance, 50.0, [10.0, 50.0])),
            ("Electrical vs Plumbing (clearance 100 mm)", "Cables and trays kept 100 mm away from water pipes", mk("Electrical vs Plumbing clearance", d("MEP - Electrical"), d("MEP - Plumbing"), ClashType::Clearance, 100.0, [10.0, 50.0])),
            ("Duplicates (all models)", "The same element modelled twice", mk("Duplicates", all(), all(), ClashType::Duplicates, 1.0, [10.0, 50.0])),
        ]
    }

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
            sev_mm: sev_default(),
            run_key: 0,
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

/// Directia sectiunii exportate in DWG / DXF. `Current` foloseste planul de
/// taiere activ; celelalte taie modelul dupa o directie, la pozitia aleasa.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum CadView {
    #[default]
    Current,
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
}

impl CadView {
    pub const ALL: [CadView; 7] = [CadView::Current, CadView::Front, CadView::Back, CadView::Left, CadView::Right, CadView::Top, CadView::Bottom];

    pub fn label(self) -> &'static str {
        match self {
            CadView::Current => "Current section",
            CadView::Front => "Front",
            CadView::Back => "Back",
            CadView::Left => "Left",
            CadView::Right => "Right",
            CadView::Top => "Top",
            CadView::Bottom => "Bottom",
        }
    }

    /// axa planului de taiere: 0 = X constant, 1 = Y constant, 2 = Z constant (plan)
    pub fn axis(self) -> Option<usize> {
        match self {
            CadView::Front | CadView::Back => Some(1),
            CadView::Left | CadView::Right => Some(0),
            CadView::Top | CadView::Bottom => Some(2),
            CadView::Current => None,
        }
    }

    /// din spate / dreapta axa orizontala e inversata (ca in vederea din acea directie)
    pub fn mirrored(self) -> bool {
        matches!(self, CadView::Back | CadView::Right)
    }
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
    /// planul de taiere ales (Cut View) la momentul salvarii
    #[serde(default)]
    pub cad_view: CadView,
    #[serde(default)]
    pub cad_view_pos: f32,
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
    /// folderul (si subfolderele, "Nivel 2/Instalatii"); gol = radacina
    #[serde(default)]
    pub folder: String,
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
    #[serde(default)]
    pub check_rules: Vec<crate::check::Rule>,
    /// parametrii adaugati de utilizator pe elemente
    #[serde(default)]
    pub user_params: Vec<UserParam>,
    #[serde(default)]
    pub title_block: Option<crate::pdf::TitleBlock>,
    #[serde(default)]
    pub vp_folders: Vec<String>,
    #[serde(default)]
    pub overlays: Vec<CadOverlay>,
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
    /// viewpoint-urile exportate (toate sau doar cele dintr-un folder)
    pub list: Vec<usize>,
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
    ImportClashTests,
    ImportUserProps,
    ExportUserProps,
    ExportClashTestsCsv,
    ExportWeb,
    ExportAsBuilt,
    ExportSectionCad,
    ExportSectionPdf,
    ExportCobie,
    ImportOverlay,
    ImportCheckRules,
    ExportCheckRules,
    ExportIds,
    ExportCheckXlsx,
    ExportCheckBcf,
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
    ExportPathTrace,
    ImportTextures,
    ImportPbr,
    ImportPbrFolder,
    ExportVpHtml,
    ExportVpXlsx,
    ExportVpPdf,
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
    ExportCompareParamsCsv,
    ExportQuickChecksCsv,
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
    /// Render with materials: stilul vederii si materialele de dinainte (refacute la final)
    pub saved_style: Option<(crate::render::VisualStyle, bool)>,
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
    /// Filter by Selection: 0 = off, 1 = inclusive, 2 = exclusive (ca in Navisworks)
    pub clash_filter_sel: u8,
    /// Clash Detective in mod Focus: fereastra maximizata, cu tot spatiul pentru teste si rezultate
    pub clash_focus_mode: bool,
    /// Model focus: ribon si panouri ascunse, proprietatile raman plutitoare (maximum de spatiu pentru model)
    pub immersive_mode: bool,
    /// layout-ul salvat inainte de Model focus, ca sa se poata restaura exact
    pub saved_layout: Option<crate::layout::UiLayout>,
    /// ferestrele plutitoare maximizate (butonul ⤢ din header)
    pub float_max: std::collections::HashSet<crate::layout::Panel>,
    pub tasks: Vec<Task>,
    pub cur_task: Option<usize>,
    pub sim_active: bool,
    pub sim_day: f32,
    pub sim_play: bool,
    pub sim_speed: f32,
    pub sim_cache: HashMap<ElemId, SimState>,
    pub sim_cache_day: i32,
    /// 4D: afiseaza doar elementele legate de sarcini (restul modelului se ascunde)
    pub sim_only_linked: bool,
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
    /// exportul web: doar selectia
    pub web_selection: bool,
    /// Live Link (selectie sincronizata cu Revit / Navisworks)
    pub sync: crate::sync::SyncState,
    /// as-built: rezultatul, parametrii, fereastra, elementele colorate de noi
    pub asbuilt: Option<Vec<crate::asbuilt::ElemResult>>,
    pub asbuilt_params: crate::asbuilt::Params,
    pub asbuilt_open: bool,
    pub asbuilt_show: bool,
    pub asbuilt_painted: Vec<ElemId>,
    pub asbuilt_filter: Option<crate::asbuilt::Status>,
    /// paleta de comenzi (Ctrl+K)
    pub palette_open: bool,
    pub palette_q: String,
    pub palette_sel: usize,
    /// manualul integrat
    pub manual_open: bool,
    pub manual_page: usize,
    pub manual_q: String,
    /// Cut to DWG: pe plansa A3 cu cartus (altfel 2D in Model space)
    pub cad_sheet: bool,
    /// Cut to DWG: plan complet (hasura, etichete, axe cu cote, niveluri pe sectiuni)
    pub cad_annotate: bool,
    /// istoricul vizualizarii (Undo / Redo): starile, pozitia curenta, semnatura ultimei stari
    pub hist: Vec<ViewSnap>,
    pub hist_pos: usize,
    pub hist_sig: u64,
    /// folderele de viewpoints: cel curent (unde intra viewpoint-urile noi), cele create goale, filtrul de export
    pub vp_folder_cur: String,
    pub vp_folders: Vec<String>,
    pub vp_filter: Option<String>,
    pub vp_rename_pending: Option<(String, String)>,
    /// desene 2D suprapuse peste model si fereastra lor
    pub overlays: Vec<CadOverlay>,
    pub overlay_open: bool,
    /// cartusul planselor (PDF / DWG) si fereastra lui de editare
    pub title_block: crate::pdf::TitleBlock,
    pub title_block_open: bool,
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
    /// ping-pong (P.P. in Navisworks): la capat, animatia merge inapoi
    pub anim_pp: bool,
    /// Import 4D: dupa importul planificarii se creeaza animatia 4D
    pub anim_4d_pending: bool,
    /// un model s-a incarcat: sarcinile TimeLiner fara elemente se leaga din nou
    pub relink_tasks: bool,
    /// directia redarii: 1 inainte, -1 inapoi
    pub anim_dir: f32,
    /// starea obiectelor animate la timpul curent: element -> (culoare rgba, vizibil)
    pub anim_obj: HashMap<ElemId, ([u8; 4], bool)>,
    /// Record: inregistrarea navigarii in timp real (inceput, keyframe-uri)
    pub anim_rec: Option<(Instant, Vec<crate::anim::Keyframe>)>,
    pub anim_sim_active: bool,
    pub render_job: Option<RenderJob>,
    pub pathtrace: Option<std::sync::Arc<crate::pathtrace::PathTraceJob>>,
    pub pathtrace_open: bool,
    pub pathtrace_tex: Option<eframe::egui::TextureHandle>,
    pub pathtrace_params: (u32, u32, u32, f32),
    pub pathtrace_notified: bool,
    /// secundele scurse la momentul terminarii (ca timerul sa nu mai creasca dupa „Ready")
    pub pathtrace_done_at: Option<f32>,
    /// ultimul font aplicat (ca sa reinstalam fonturile doar la schimbare)
    pub font_family_cur: String,
    /// planul de taiere se trage cu mouse-ul in 3D (partea din cub)
    pub cut_drag: bool,
    /// fereastra consolei de scripturi (motorul Rhai se activeaza in 0.28)
    pub script_open: bool,
    pub script_help_open: bool,
    /// fereastra de setari a aplicatiei (View › Settings / butonul ⚙)
    pub settings_open: bool,
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
    /// Update All: starile dinainte (pentru sumar) si sumarul de la final
    pub update_all_before: Option<Vec<(String, [usize; 5])>>,
    pub update_summary: Option<crate::clash_io::UpdateSummary>,
    pub update_all_cur: Option<usize>,
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
    /// pragul LOD in miscare (pixeli), adaptat dupa FPS
    pub lod_px: f32,
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
    /// proprietatile schimbate (compare pe parametri) si filtrul listei
    pub compare_params: Vec<crate::compare::ParamChange>,
    /// verificarile rapide (fara IDS)
    pub quick_checks: Vec<crate::check::QuickCheck>,
    /// legenda colorarii dupa parametru in vederea 3D
    pub profile_legend: bool,
    /// parametrii adaugati de utilizator (salvati in proiect) si campurile din panoul Properties
    pub user_params: Vec<UserParam>,
    pub up_name: String,
    pub prop_import: Option<PropImport>,
    /// (tab, proprietate) scrise vreodata in aceasta sesiune - se sterg corect dupa redenumire / stergere
    pub user_applied: HashSet<(String, String)>,
    /// tab-ul de date utilizator afisat / editat in Properties
    pub user_tab: String,
    /// tab-uri goale create cu Add User Data Tab (inca fara proprietati)
    pub user_tabs_new: Vec<String>,
    /// redenumirea tab-ului curent (textul din campul de editare)
    pub user_tab_rename: Option<String>,
    pub up_value: String,
    pub compare_param_pick: Option<String>,
    pub compare_on: bool,
    pub compare_pick: (Option<usize>, Option<usize>),
    pub compare_tol: f32,
    /// Model Checker: regulile, ultimul rezultat, regula aleasa si avertismentele importului IDS
    pub check_rules: Vec<crate::check::Rule>,
    pub check_results: Option<Vec<crate::check::RuleResult>>,
    pub check_sel: Option<usize>,
    pub check_warn: Vec<String>,
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
            clash_filter_sel: 0,
            clash_focus_mode: false,
            immersive_mode: false,
            saved_layout: None,
            float_max: std::collections::HashSet::new(),
            tasks: Vec::new(),
            cur_task: None,
            sim_active: false,
            sim_day: 0.0,
            sim_play: false,
            sim_speed: 7.0,
            sim_cache: HashMap::new(),
            sim_cache_day: i32::MIN,
            sim_only_linked: true,
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
            web_selection: false,
            sync: Default::default(),
            asbuilt: None,
            asbuilt_params: Default::default(),
            asbuilt_open: false,
            asbuilt_show: false,
            asbuilt_painted: Vec::new(),
            asbuilt_filter: None,
            palette_open: false,
            palette_q: String::new(),
            palette_sel: 0,
            manual_open: false,
            manual_page: 0,
            manual_q: String::new(),
            cad_sheet: false,
            cad_annotate: true,
            hist: Vec::new(),
            hist_pos: 0,
            hist_sig: 0,
            vp_folder_cur: String::new(),
            vp_folders: Vec::new(),
            vp_filter: None,
            vp_rename_pending: None,
            overlays: Vec::new(),
            overlay_open: false,
            title_block: Default::default(),
            title_block_open: false,
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
            anim_pp: false,
            anim_4d_pending: false,
            relink_tasks: false,
            anim_dir: 1.0,
            anim_obj: HashMap::new(),
            anim_rec: None,
            anim_time: 0.0,
            anim_loop: false,
            anim_sim_active: false,
            render_job: None,
            pathtrace: None,
            pathtrace_open: false,
            pathtrace_tex: None,
            pathtrace_params: (256, 256, 32, 30.0),
            pathtrace_notified: false,
            pathtrace_done_at: None,
            font_family_cur: "Inter".into(),
            cut_drag: false,
            script_open: false,
            script_help_open: false,
            settings_open: false,
            still_path: None,
            pending_capture: None,
            ffmpeg: None,
            orbit_secs: 20.0,
            key_step: 3.0,
            proj_sel: None,
            materials: crate::materials::full_library(),
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
            update_all_before: None,
            update_summary: None,
            update_all_cur: None,
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
            lod_px: 2.0,
            alpha_cache: HashMap::new(),
            file_stamps: HashMap::new(),
            reloading: HashMap::new(),
            changed_files: BTreeSet::new(),
            last_file_check: Instant::now(),
            looks: HashMap::new(),
            profile: Default::default(),
            profile_map: None,
            compare: None,
            compare_params: Vec::new(),
            quick_checks: Vec::new(),
            profile_legend: true,
            user_params: Vec::new(),
            up_name: String::new(),
            prop_import: None,
            user_applied: HashSet::new(),
            user_tab: String::new(),
            user_tabs_new: Vec::new(),
            user_tab_rename: None,
            up_value: String::new(),
            compare_param_pick: None,
            compare_on: false,
            compare_pick: (None, None),
            compare_tol: 1.0,
            check_rules: Vec::new(),
            check_results: None,
            check_sel: None,
            check_warn: Vec::new(),
            tree_edit: false,
            qto: Default::default(),
            qto_result: None,
            qto_sig: 0,
        };
        {
            let p = &app.settings.clash_ui;
            app.clash_subtab = p.subtab;
            app.clash_filter = p.filter;
            app.clash_filter_sel = p.filter_sel;
            app.clash_others = p.others;
            app.clash_auto_zoom = p.auto_zoom;
            app.clash_highlight_all = p.highlight_all;
            app.group_cell = p.group_cell;
            app.sim_only_linked = p.only_linked;
        }
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
        crate::i18n::set_romanian(app.settings.romanian);
        app
    }

    // ------------------------------------------------------------------ mesaje
    /// Salveaza preferintele UI (Clash / 4D) cand se schimba, fara a scrie fisierul la fiecare cadru.
    pub fn sync_ui_prefs(&mut self) {
        let p = crate::theme::ClashUiPrefs {
            subtab: self.clash_subtab,
            filter: self.clash_filter,
            filter_sel: self.clash_filter_sel,
            others: self.clash_others,
            auto_zoom: self.clash_auto_zoom,
            highlight_all: self.clash_highlight_all,
            group_cell: self.group_cell,
            only_linked: self.sim_only_linked,
        };
        if p != self.settings.clash_ui {
            self.settings.clash_ui = p;
            self.settings.save();
        }
    }

    /// Model focus: maximum de spatiu pentru model, proprietatile raman plutitoare.
    pub fn toggle_immersive(&mut self) {
        if self.immersive_mode {
            if let Some(l) = self.saved_layout.take() {
                self.settings.layout = l;
            }
            self.immersive_mode = false;
        } else {
            self.saved_layout = Some(self.settings.layout.clone());
            self.settings.layout.hide_all = true;
            self.settings.layout.bottom_open = false;
            self.settings.layout.move_to(crate::layout::Panel::Properties, crate::layout::Dock::Float);
            self.immersive_mode = true;
        }
        self.settings.save();
    }

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

    const NW_HEAD: &'static str = "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<exchange xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:noNamespaceSchemaLocation=\"http://download.autodesk.com/us/navisworks/schemas/nw-exchange-12.0.xsd\" units=\"m\" filename=\"\" filepath=\"\">\n";

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
}

// ------------------------------------------------------------------ salvare automata
/// %LOCALAPPDATA%\ManageBIM\Autosave
pub fn autosave_dir() -> Option<std::path::PathBuf> {
    crate::formats::data_root().map(|d| d.join("ManageBIM").join("Autosave"))
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
    /// Interfata RO / EN (salvata in setari).
    pub fn toggle_language(&mut self) {
        self.settings.romanian = !self.settings.romanian;
        crate::i18n::set_romanian(self.settings.romanian);
        self.settings.save();
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

}

// ------------------------------------------------------------------ .bimd (un singur fisier, ca .nwd)
/// Folderul in care se despacheteaza fisierele .bimd deschise.
pub fn published_dir() -> Option<std::path::PathBuf> {
    crate::formats::data_root().map(|d| d.join("ManageBIM").join("Published"))
}

impl App {

}

impl App {
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

}

fn pivot_rotate(p: Vec3, pivot: Vec3, rot: &Mat4) -> Vec3 {
    pivot + rot.transform_vector3(p - pivot)
}

fn fmt_vec_len(d: Vec3) -> String {
    format!("{} (ΔX {} ΔY {} ΔZ {})", fmt_len(d.length() as f64), fmt_num(d.x as f64), fmt_num(d.y as f64), fmt_num(d.z as f64))
}

impl App {
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

    // ------------------------------------------------------------ suprapunere DWG / DXF 2D peste model

    // ------------------------------------------------------------ Undo / Redo al vizualizarii

    // ------------------------------------------------------------ stil de afisare, proiect demo

    /// Proiectul demo (Block A: arhitectura + structura, instalatii, planificare 4D): fisierele sunt in
    /// aplicatie, se scriu in %LOCALAPPDATA%\ManageBIM\Demo si se deschid ca un proiect obisnuit.
    pub fn open_demo(&mut self) {
        const FILES: [(&str, &[u8]); 3] = [
            ("Block_A_Architecture_Structure.ifc", include_bytes!("../samples/Block_A_Architecture_Structure.ifc")),
            ("Block_A_MEP.ifc", include_bytes!("../samples/Block_A_MEP.ifc")),
            ("Block_A_schedule_4D.csv", include_bytes!("../samples/Block_A_schedule_4D.csv")),
        ];
        let Some(dir) = crate::formats::data_root().map(|d| d.join("ManageBIM").join("Demo")) else {
            return self.error("No local data folder (LOCALAPPDATA / HOME is not set).");
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return self.error(format!("Cannot create {}: {e}", dir.display()));
        }
        for (n, b) in FILES {
            let p = dir.join(n);
            // nu rescriem fisierele demo (utilizatorul poate lucra pe ele)
            if !p.exists() {
                if let Err(e) = std::fs::write(&p, b) {
                    return self.error(format!("Cannot write {}: {e}", p.display()));
                }
            }
        }
        self.show_start = false;
        self.open_path(&dir.join(FILES[0].0).to_string_lossy(), false);
        self.open_path(&dir.join(FILES[1].0).to_string_lossy(), true);
        self.handle_path(Purpose::ImportTasksCsv, dir.join(FILES[2].0).to_string_lossy().to_string());
        self.manual_open = true;
        self.manual_page = 0;
        self.toast("Demo project: Block A (architecture + structure, MEP, 4D schedule). Try Clash Detective, TimeLiner, Model Checker and the command palette (Ctrl+K).");
    }
    // ------------------------------------------------------------ As-built: model fata de norul de puncte

    // ------------------------------------------------------------ Live Link: selectie sincronizata

}

/// %LOCALAPPDATA%\ManageBIM\Outbox (referinte, issue-uri pentru add-in-uri)
pub fn outbox_dir() -> Option<std::path::PathBuf> {
    crate::formats::data_root().map(|d| d.join("ManageBIM").join("Outbox"))
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

    // ------------------------------------------------------------ Model Checker (reguli + IDS)

}

/// Numele modelului fara extensie (pentru matrice / rapoarte).
pub fn model_label(name: &str) -> String {
    std::path::Path::new(name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| name.to_string())
}

/// 12345 -> "12 345"
pub fn thousands_app(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// numele layer-ului DXF pentru un tip de element (fara caractere interzise in AutoCAD)
pub fn cad_layer(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' }).collect();
    let t = t.trim().to_string();
    if t.is_empty() { "Model".into() } else { t }
}

/// DXF R12 cu linii pe layere (culoare AutoCAD diferita pe layer), unitati: metri
pub fn cut_dxf(segs: &[(String, [f64; 4])]) -> String {
    cut_dxf_layers(segs, &[])
}

/// ca `cut_dxf`, cu layere suplimentare definite in tabel (ex. pentru texte adaugate dupa)
pub fn cut_dxf_layers(segs: &[(String, [f64; 4])], extra: &[&str]) -> String {
    let mut layers: Vec<&str> = segs.iter().map(|s| s.0.as_str()).chain(extra.iter().copied()).collect();
    layers.sort();
    layers.dedup();
    // tabel LTYPE: solid +Hidden (punctat) pentru stratul „BEHIND"
    let mut s = String::from("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n6\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n");
    s.push_str("0\nTABLE\n2\nLTYPE\n70\n2\n0\nLTYPE\n2\nCONTINUOUS\n70\n0\n3\nSolid line\n72\n65\n73\n0\n40\n0.0\n0\nLTYPE\n2\nHIDDEN\n70\n0\n3\nHidden line __ __\n72\n65\n73\n2\n40\n1.0\n49\n0.5\n74\n0\n49\n-0.5\n74\n0\n0\nENDTAB\n");
    s.push_str("0\nTABLE\n2\nLAYER\n");
    s.push_str(&format!("70\n{}\n", layers.len()));
    for (i, l) in layers.iter().enumerate() {
        let behind = l.eq_ignore_ascii_case("BEHIND");
        let ltype = if behind { "HIDDEN" } else { "CONTINUOUS" };
        let color = if behind { 8 } else { [1, 2, 3, 4, 5, 6, 30, 140, 200, 40][i % 10] };
        s.push_str(&format!("0\nLAYER\n2\n{l}\n70\n0\n62\n{color}\n6\n{ltype}\n"));
    }
    s.push_str("0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    for (l, p) in segs {
        s.push_str(&format!("0\nLINE\n8\n{l}\n10\n{:.4}\n20\n{:.4}\n30\n0.0\n11\n{:.4}\n21\n{:.4}\n31\n0.0\n", p[0], p[1], p[2], p[3]));
    }
    s.push_str("0\nENDSEC\n0\nEOF\n");
    s
}

/// Liniile taieturii puse pe o plansa (A3 / A2 / A1 / A0, milimetri) la scara aleasa sau automata
/// (cea mai mare scara standard la care incape), cu chenar si cartusul editabil.
/// `segs` sunt in metri (coordonate reale). Intoarce DXF-ul si scara (1:N).
pub fn cut_sheet_dxf(segs: &[(String, [f64; 4])], tb: &crate::pdf::TitleBlock, project: &str, title: &str, date: &str) -> (String, u32) {
    cut_sheet_dxf_annot(segs, &[], tb, project, title, date)
}

/// scara plansei: cea din cartus sau cea mai mare scara standard la care incape desenul w x h (m) in aw x ah (mm)
fn fit_scale(tb: &crate::pdf::TitleBlock, w: f64, h: f64, aw: f64, ah: f64) -> u32 {
    if tb.scale > 0 {
        return tb.scale;
    }
    crate::pdf::SCALES.into_iter().find(|n| w * 1000.0 / *n as f64 <= aw && h * 1000.0 / *n as f64 <= ah).unwrap_or(10000)
}

/// Scara la care `cut_sheet_dxf` ar pune liniile pe plansa (pentru a dimensiona textele inainte).
pub fn sheet_scale(segs: &[(String, [f64; 4])], tb: &crate::pdf::TitleBlock) -> u32 {
    let (sw, sh) = crate::pdf::paper_mm(&tb.paper);
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (_, p) in segs {
        x0 = x0.min(p[0]).min(p[2]);
        y0 = y0.min(p[1]).min(p[3]);
        x1 = x1.max(p[0]).max(p[2]);
        y1 = y1.max(p[1]).max(p[3]);
    }
    fit_scale(tb, (x1 - x0).max(0.01), (y1 - y0).max(0.01), sw - 30.0, sh - 20.0 - 48.0 - 10.0)
}

/// ca `cut_sheet_dxf`, plus textele planului (etichete, cote, axe, niveluri) mutate pe plansa (inaltimea in mm de hartie).
pub fn cut_sheet_dxf_annot(segs: &[(String, [f64; 4])], texts: &[CadText], tb: &crate::pdf::TitleBlock, project: &str, title: &str, date: &str) -> (String, u32) {
    let (sw, sh) = crate::pdf::paper_mm(&tb.paper);
    const MARGIN: f64 = 10.0;
    const TB_W: f64 = 185.0;
    const TB_H: f64 = 48.0;
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (_, p) in segs {
        x0 = x0.min(p[0]).min(p[2]);
        y0 = y0.min(p[1]).min(p[3]);
        x1 = x1.max(p[0]).max(p[2]);
        y1 = y1.max(p[1]).max(p[3]);
    }
    let (w, h) = ((x1 - x0).max(0.01), (y1 - y0).max(0.01));
    // zona de desen: foaia fara margini si fara banda cartusului
    let (aw, ah) = (sw - 2.0 * MARGIN - 10.0, sh - 2.0 * MARGIN - TB_H - 10.0);
    let scale = fit_scale(tb, w, h, aw, ah);
    let k = 1000.0 / scale as f64;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (ox, oy) = (MARGIN + 5.0 + aw / 2.0, MARGIN + TB_H + 5.0 + ah / 2.0);
    let mut out: Vec<(String, [f64; 4])> = segs.iter().map(|(l, p)| (l.clone(), [ox + (p[0] - cx) * k, oy + (p[1] - cy) * k, ox + (p[2] - cx) * k, oy + (p[3] - cy) * k])).collect();
    let mut line = |a: f64, b: f64, c: f64, d: f64| out.push(("SHEET".to_string(), [a, b, c, d]));
    let rect = |l: &mut dyn FnMut(f64, f64, f64, f64), a: f64, b: f64, c: f64, d: f64| {
        l(a, b, c, b);
        l(c, b, c, d);
        l(c, d, a, d);
        l(a, d, a, b);
    };
    rect(&mut line, 0.0, 0.0, sw, sh);
    rect(&mut line, MARGIN, MARGIN, sw - MARGIN, sh - MARGIN);
    // cartusul: dreapta jos, 185 x 48 mm - firma / proiect | titlu | desenat, verificat | data, rev | scara | nr
    let (tx, ty) = (sw - MARGIN - TB_W, MARGIN);
    let r = sw - MARGIN;
    rect(&mut line, tx, ty, r, ty + TB_H);
    line(tx, ty + 34.0, r, ty + 34.0);
    line(tx, ty + 22.0, r, ty + 22.0);
    line(tx, ty + 11.0, r, ty + 11.0);
    line(tx + 120.0, ty, tx + 120.0, ty + 22.0);
    line(tx + 150.0, ty, tx + 150.0, ty + 22.0);
    let mut extra: Vec<&str> = vec!["SHEET_TEXT"];
    extra.extend(texts.iter().map(|t| t.layer.as_str()));
    let mut dxf = cut_dxf_layers(&out, &extra);
    let plan_txt: String = texts.iter().map(|t| CadText { x: ox + (t.x - cx) * k, y: oy + (t.y - cy) * k, ..t.clone() }.dxf(1.0)).collect();
    if let Some(p) = dxf.rfind("0\nENDSEC\n0\nEOF") {
        dxf.insert_str(p, &plan_txt);
    }
    let txt =|x: f64, y: f64, hgt: f64, s: &str| format!("0\nTEXT\n8\nSHEET_TEXT\n10\n{x:.2}\n20\n{y:.2}\n30\n0.0\n40\n{hgt:.2}\n1\n{}\n", s.replace('\n', " "));
    let or = |s: &str, d: &str| if s.trim().is_empty() { d.to_string() } else { s.trim().to_string() };
    let mut t = String::new();
    if !tb.company.trim().is_empty() {
        t.push_str(&txt(tx + 3.0, ty + 43.0, 2.5, &tb.company.to_uppercase()));
    }
    t.push_str(&txt(tx + 3.0, ty + 37.0, 4.5, tb.project_or(project)));
    if !tb.client.trim().is_empty() {
        t.push_str(&txt(tx + 120.0, ty + 43.0, 2.5, &format!("Client: {}", tb.client)));
    }
    t.push_str(&txt(tx + 3.0, ty + 26.0, 4.0, title));
    t.push_str(&txt(tx + 3.0, ty + 15.0, 2.5, &format!("Drawn: {}    Checked: {}", or(&tb.drawn, "-"), or(&tb.checked, "-"))));
    t.push_str(&txt(tx + 3.0, ty + 4.0, 2.5, &format!("Date: {}    {}", date.get(..10).unwrap_or(date), or(&tb.notes, "ManageBIM"))));
    t.push_str(&txt(tx + 122.0, ty + 15.0, 2.2, "SCALE"));
    t.push_str(&txt(tx + 122.0, ty + 4.0, 3.5, &format!("1:{scale}")));
    t.push_str(&txt(tx + 152.0, ty + 15.0, 2.2, &format!("REV {}", or(&tb.revision, "-"))));
    t.push_str(&txt(tx + 152.0, ty + 3.5, 4.5, &format!("{}01", tb.number_prefix)));
    t.push_str(&txt(MARGIN + 3.0, MARGIN + 3.0, 2.2, &format!("{} - 1:{scale}", tb.paper)));
    if let Some(p) = dxf.rfind("0\nENDSEC\n0\nEOF") {
        dxf.insert_str(p, &t);
    }
    // plansa e in milimetri
    let dxf = dxf.replacen("$INSUNITS\n70\n6\n", "$INSUNITS\n70\n4\n", 1);
    (dxf, scale)
}

/// Un text al planului 2D (eticheta, cota, axa, nivel). `mm` = inaltimea pe hartie;
/// in Model space se inmulteste cu `u` (metri pe mm de hartie). `rot` in grade.
#[derive(Clone, Debug)]
pub struct CadText {
    pub layer: String,
    pub x: f64,
    pub y: f64,
    pub mm: f64,
    pub rot: f64,
    pub s: String,
}

impl CadText {
    pub fn new(layer: &str, x: f64, y: f64, mm: f64, rot: f64, s: impl Into<String>) -> Self {
        CadText { layer: layer.into(), x, y, mm, rot, s: s.into() }
    }
    /// entitatea TEXT; `k` = unitati de desen pe mm de hartie
    pub fn dxf(&self, k: f64) -> String {
        let s: String = self.s.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect();
        format!("0\nTEXT\n8\n{}\n10\n{:.4}\n20\n{:.4}\n30\n0.0\n40\n{:.4}\n1\n{}\n50\n{:.2}\n", self.layer, self.x, self.y, self.mm * k, s, self.rot)
    }
}

/// DXF in Model space (metri) cu linii si textele planului - `u` = metri pe mm de hartie (0.1 = 1:100).
pub fn cut_dxf_annot(segs: &[(String, [f64; 4])], texts: &[CadText], u: f64) -> String {
    let extra: Vec<&str> = texts.iter().map(|t| t.layer.as_str()).collect();
    let mut dxf = cut_dxf_layers(segs, &extra);
    let t: String = texts.iter().map(|t| t.dxf(u)).collect();
    if let Some(p) = dxf.rfind("0\nENDSEC\n0\nEOF") {
        dxf.insert_str(p, &t);
    }
    dxf
}

/// Hasura ANSI31 (linii la 45 grade, pas `spacing`) in interiorul conturului taiat al unui element.
/// Conturul vine din taierea triunghiurilor unei suprafete inchise, deci regula par / impar delimiteaza interiorul.
pub fn hatch_lines(contour: &[[f64; 4]], spacing: f64) -> Vec<[f64; 4]> {
    let mut out = Vec::new();
    if contour.len() < 3 || spacing <= 0.0 {
        return out;
    }
    let r = std::f64::consts::FRAC_1_SQRT_2;
    // u de-a lungul hasurii, v perpendicular
    let uv = |x: f64, y: f64| ((x + y) * r, (y - x) * r);
    let xy = |u: f64, v: f64| ((u - v) * r, (u + v) * r);
    let e: Vec<((f64, f64), (f64, f64))> = contour.iter().map(|s| (uv(s[0], s[1]), uv(s[2], s[3]))).collect();
    let (vmin, vmax) = e.iter().fold((f64::MAX, f64::MIN), |(a, b), (p, q)| (a.min(p.1).min(q.1), b.max(p.1).max(q.1)));
    if (vmax - vmin) / spacing > 4000.0 {
        return out;
    }
    // liniile pe o grila globala: hasurile elementelor vecine se continua
    let mut v = (vmin / spacing).ceil() * spacing;
    let mut xs: Vec<f64> = Vec::new();
    while v <= vmax {
        xs.clear();
        for (p, q) in &e {
            if (p.1 < v) != (q.1 < v) {
                xs.push(p.0 + (q.0 - p.0) * (v - p.1) / (q.1 - p.1));
            }
        }
        xs.sort_by(|a, b| a.total_cmp(b));
        for w in xs.chunks_exact(2) {
            if w[1] - w[0] > 1e-6 {
                let (a, b) = (xy(w[0], v), xy(w[1], v));
                out.push([a.0, a.1, b.0, b.1]);
            }
        }
        v += spacing;
    }
    out
}

/// Elementele care se hasureaza in plan / sectiune (structura si pereti).
pub fn cut_is_hatched(ifc_type: &str) -> bool {
    let t = ifc_type.to_lowercase();
    ["wall", "column", "slab", "beam", "footing", "pile", "stair", "ramp", "roof"].iter().any(|k| t.contains(k)) && !t.contains("opening")
}

/// Eticheta unui element taiat: usi / ferestre -> numarul (Mark / Number / Tag sau ultima parte din nume),
/// pereti -> tipul. None pentru restul.
pub fn cut_tag(e: &crate::model::Element) -> Option<String> {
    let t = e.ifc_type.to_lowercase();
    let prop = |names: &[&str]| e.props.iter().find(|p| names.iter().any(|n| p.name.eq_ignore_ascii_case(n)) && !p.value.trim().is_empty()).map(|p| p.value.trim().to_string());
    let parts: Vec<&str> = e.name.split(':').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if t.contains("door") || t.contains("window") {
        let v = prop(&["Mark", "Number", "Nr", "Tag"]).or_else(|| parts.last().map(|s| s.to_string()))?;
        return Some(v.chars().take(24).collect());
    }
    if t.contains("wall") {
        let v = prop(&["Type Name", "Type", "ObjectType"]).or_else(|| if parts.len() >= 2 { Some(parts[1].to_string()) } else { parts.first().map(|s| s.to_string()) })?;
        return Some(v.chars().take(32).collect());
    }
    None
}

fn circle_segs(layer: &str, c: [f64; 2], r: f64, out: &mut Vec<(String, [f64; 4])>) {
    const N: usize = 20;
    for i in 0..N {
        let (a, b) = (i as f64 / N as f64 * std::f64::consts::TAU, (i + 1) as f64 / N as f64 * std::f64::consts::TAU);
        out.push((layer.into(), [c[0] + r * a.cos(), c[1] + r * a.sin(), c[0] + r * b.cos(), c[1] + r * b.sin()]));
    }
}

/// unghiul unui text citibil de jos / din dreapta: (-90, 90]
pub fn read_angle(dx: f64, dy: f64) -> f64 {
    let mut a = dy.atan2(dx).to_degrees();
    if a > 90.0 + 1e-6 {
        a -= 180.0;
    } else if a <= -90.0 + 1e-6 {
        a += 180.0;
    }
    a
}

/// latimea aproximativa a unui text (metri), `u` = metri pe mm de hartie
fn text_w(s: &str, mm: f64, u: f64) -> f64 {
    s.chars().count() as f64 * mm * u * 0.8
}

/// Axele in plan: linia axei, bula cu eticheta la un capat si lanturi de cote intre axele paralele
/// (plus cota totala) la celalalt capat. Coordonate in metri.
pub fn plan_grid_annotations(grids: &[UserGrid], u: f64, segs: &mut Vec<(String, [f64; 4])>, texts: &mut Vec<CadText>) {
    let r_b = 4.5 * u;
    let mut items: Vec<([f64; 2], [f64; 2], [f64; 2], &str)> = Vec::new();
    for g in grids {
        let (dx, dy) = (g.b[0] - g.a[0], g.b[1] - g.a[1]);
        let l = (dx * dx + dy * dy).sqrt();
        if l < 1e-6 {
            continue;
        }
        items.push((g.a, g.b, [dx / l, dy / l], g.name.as_str()));
    }
    // grupuri de axe paralele
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for i in 0..items.len() {
        let d = items[i].2;
        match groups.iter_mut().find(|gr| {
            let e = items[gr[0]].2;
            (d[0] * e[0] + d[1] * e[1]).abs() > 0.9998
        }) {
            Some(gr) => gr.push(i),
            None => groups.push(vec![i]),
        }
    }
    for gr in &groups {
        let d = items[gr[0]].2;
        let n = [-d[1], d[0]];
        // fiecare axa: (offset pe normala, t minim, t maxim, eticheta) in sistemul primei axe din grup
        let mut rows: Vec<(f64, f64, f64, &str)> = gr
            .iter()
            .map(|&i| {
                let (a, b, _, name) = items[i];
                let (ta, tb) = (a[0] * d[0] + a[1] * d[1], b[0] * d[0] + b[1] * d[1]);
                (a[0] * n[0] + a[1] * n[1], ta.min(tb), ta.max(tb), name)
            })
            .collect();
        rows.sort_by(|a, b| a.0.total_cmp(&b.0));
        let p = |o: f64, t: f64| [n[0] * o + d[0] * t, n[1] * o + d[1] * t];
        for &(o, t0, t1, name) in &rows {
            let (a, b) = (p(o, t0), p(o, t1));
            segs.push(("GRIDS".into(), [a[0], a[1], b[0], b[1]]));
            let c = p(o, t0 - r_b);
            circle_segs("GRIDS", c, r_b, segs);
            texts.push(CadText::new("GRID_TEXT", c[0] - text_w(name, 2.5, u) / 2.0, c[1] - 1.25 * u, 2.5, 0.0, name));
        }
        if rows.len() < 2 {
            continue;
        }
        // cotele: la capatul opus bulelor
        let tmax = rows.iter().map(|r| r.2).fold(f64::MIN, f64::max);
        let rot = read_angle(n[0], n[1]);
        let (rc, rs) = (rot.to_radians().cos(), rot.to_radians().sin());
        let chain = |t: f64, idx: &[usize], segs: &mut Vec<(String, [f64; 4])>, texts: &mut Vec<CadText>| {
            let (a, b) = (p(rows[idx[0]].0, t), p(rows[idx[idx.len() - 1]].0, t));
            segs.push(("DIMS".into(), [a[0], a[1], b[0], b[1]]));
            for &k in idx {
                let o = rows[k].0;
                // linie ajutatoare de la capatul axei si bifa la 45 grade
                let (e0, e1) = (p(o, rows[k].2 + 2.0 * u), p(o, t + 1.5 * u));
                segs.push(("DIMS".into(), [e0[0], e0[1], e1[0], e1[1]]));
                let q = p(o, t);
                let s = [(d[0] + n[0]) * u, (d[1] + n[1]) * u];
                segs.push(("DIMS".into(), [q[0] - s[0], q[1] - s[1], q[0] + s[0], q[1] + s[1]]));
            }
            for w in idx.windows(2) {
                let (oa, ob) = (rows[w[0]].0, rows[w[1]].0);
                let mm = ((ob - oa) * 1000.0).round();
                if mm < 1.0 {
                    continue;
                }
                let label = format!("{mm:.0}");
                let half = text_w(&label, 2.5, u) / 2.0;
                let m = p((oa + ob) / 2.0, t);
                // deasupra liniei de cota, in sensul de citire al textului
                let m = [m[0] - rs * 0.8 * u, m[1] + rc * 0.8 * u];
                texts.push(CadText::new("DIMS_TEXT", m[0] - half * rc, m[1] - half * rs, 2.5, rot, label));
            }
        };
        let all: Vec<usize> = (0..rows.len()).collect();
        chain(tmax + 12.0 * u, &all, segs, texts);
        if rows.len() > 2 {
            chain(tmax + 20.0 * u, &[0, rows.len() - 1], segs, texts);
        }
    }
}

/// Pe o sectiune verticala: liniile de nivel cu nume si cota (m) si axele taiate de plan (cu bula sus).
/// `axis` = 0 (plan la X = value, orizontala = Y) sau 1 (plan la Y = value, orizontala = X);
/// `bbox` = [x0, y0, x1, y1] al taieturii in coordonatele desenului.
pub fn section_annotations(levels: &[(String, f64)], grids: &[UserGrid], axis: usize, value: f64, bbox: [f64; 4], u: f64, segs: &mut Vec<(String, [f64; 4])>, texts: &mut Vec<CadText>) {
    let [x0, y0, x1, y1] = bbox;
    for (name, z) in levels {
        if *z < y0 - 5.0 || *z > y1 + 5.0 {
            continue;
        }
        segs.push(("LEVELS".into(), [x0 - 10.0 * u, *z, x1 + 10.0 * u, *z]));
        // marcajul de cota (triunghi cu varful pe linie)
        let tx = x1 + 10.0 * u;
        segs.push(("LEVELS".into(), [tx, *z, tx + 2.0 * u, *z + 2.5 * u]));
        segs.push(("LEVELS".into(), [tx + 2.0 * u, *z + 2.5 * u, tx - 2.0 * u, *z + 2.5 * u]));
        segs.push(("LEVELS".into(), [tx - 2.0 * u, *z + 2.5 * u, tx, *z]));
        texts.push(CadText::new("LEVELS_TEXT", tx + 3.5 * u, *z + 0.5 * u, 2.5, 0.0, format!("{name}  {}{:.3}", if *z >= 0.0 { "+" } else { "" }, z)));
    }
    for g in grids {
        // intersectia axei (2D) cu planul sectiunii
        let (a, b) = (g.a[axis] - value, g.b[axis] - value);
        if (a > 1e-9 && b > 1e-9) || (a < -1e-9 && b < -1e-9) || (b - a).abs() < 1e-9 {
            continue;
        }
        let t = -a / (b - a);
        let o = 1 - axis;
        let h = g.a[o] + (g.b[o] - g.a[o]) * t;
        if h < x0 - 2.0 || h > x1 + 2.0 {
            continue;
        }
        segs.push(("GRIDS".into(), [h, y0 - 5.0 * u, h, y1 + 10.0 * u]));
        circle_segs("GRIDS", [h, y1 + 14.5 * u], 4.5 * u, segs);
        texts.push(CadText::new("GRID_TEXT", h - text_w(&g.name, 2.5, u) / 2.0, y1 + 13.25 * u, 2.5, 0.0, g.name.clone()));
    }
}

/// O stare a vizualizarii in istoricul Undo / Redo.
#[derive(Clone)]
pub struct ViewSnap {
    pub selected: BTreeSet<ElemId>,
    pub hidden: Arc<HashSet<ElemId>>,
    pub section: Section,
    pub cam: Camera,
}

/// Un desen 2D (DWG / DXF) asezat peste model la o cota, cu deplasare si rotatie.
#[derive(Clone, Serialize, Deserialize)]
pub struct CadOverlay {
    pub name: String,
    pub path: String,
    /// cota (m, coordonate locale ale scenei)
    pub z: f32,
    /// deplasare (m) si rotatie (grade, in jurul originii scenei)
    #[serde(default)]
    pub dx: f32,
    #[serde(default)]
    pub dy: f32,
    #[serde(default)]
    pub rot_deg: f32,
    /// None = plan orizontal la Z; Some(0) = plan vertical YZ la Y=z (Front/Back);
    /// Some(1) = plan vertical XZ la X=z (Left/Right)
    #[serde(default)]
    pub vert: Option<u8>,
    /// desenul a fost exportat oglindit (Back / Right) la intoarcere se oglindeste inapoi
    #[serde(default)]
    pub mirror: bool,
    #[serde(default = "yes_bool")]
    pub visible: bool,
    #[serde(default = "overlay_color")]
    pub color: [u8; 3],
    #[serde(default = "overlay_opacity")]
    pub opacity: f32,
    /// liniile (x1, y1, x2, y2) in coordonatele scenei; recitite din fisier la deschiderea proiectului
    #[serde(skip)]
    pub segs: Arc<Vec<[f32; 4]>>,
}

fn yes_bool() -> bool {
    true
}
fn overlay_color() -> [u8; 3] {
    [255, 120, 60]
}
fn overlay_opacity() -> f32 {
    0.85
}

// ====================================================================== parametri adaugati de utilizator

/// Un parametru adaugat in ManageBIM unui element (ex. elemente fara date din DWG / OBJ / STEP),
/// salvat in proiect si aplicat la fiecare incarcare: model (numele fisierului) + GUID (sau #index).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserParam {
    pub model: String,
    pub key: String,
    pub name: String,
    pub value: String,
    /// tab-ul de date utilizator (Navisworks: User Data Tab); proiectele vechi: "User"
    #[serde(default = "user_group_default")]
    pub group: String,
}

fn user_group_default() -> String {
    USER_GROUP.to_string()
}

/// grupul implicit al proprietatilor adaugate de utilizator (proiectele vechi)
pub const USER_GROUP: &str = "User";

/// Import de proprietati din Excel / CSV: fisierul citit si alegerile din fereastra de import.
#[derive(Clone, Debug, Default)]
pub struct PropImport {
    pub path: String,
    pub rows: Vec<Vec<String>>,
    /// randul de antet
    pub header: usize,
    /// coloana cu cheia elementului
    pub key_col: usize,
    /// cum se potriveste cheia: "GlobalId", "Name" sau numele unei proprietati
    pub match_by: String,
    /// tab-ul in care se scriu proprietatile
    pub tab: String,
    /// doar elementele selectate
    pub only_selected: bool,
}

/// cheia unui element: GUID-ul sau "#index" cand nu are GUID
pub fn user_key(e: &Element, idx: usize) -> String {
    if e.guid.trim().is_empty() { format!("#{idx}") } else { e.guid.clone() }
}

/// Scrie parametrii utilizatorului in proprietatile modelului (tab-urile utilizatorului se refac de la zero).
pub fn apply_user_params(m: &mut Model, params: &[UserParam]) {
    apply_user_params_ext(m, params, &HashSet::new());
}

/// ca `apply_user_params`; `stale` = perechi (tab, proprietate) aplicate anterior (redenumite / sterse) care trebuie scoase.
pub fn apply_user_params_ext(m: &mut Model, params: &[UserParam], stale: &HashSet<(String, String)>) {
    let mine: Vec<&UserParam> = params.iter().filter(|p| p.model == m.name).collect();
    let mut strip: HashSet<(String, String)> = params.iter().map(|p| (p.group.clone(), p.name.to_lowercase())).collect();
    strip.extend(stale.iter().map(|(g, n)| (g.clone(), n.to_lowercase())));
    let is_user = |p: &crate::model::Prop| p.group == USER_GROUP || strip.contains(&(p.group.clone(), p.name.to_lowercase()));
    let had = m.elements.iter().any(|e| e.props.iter().any(|p| is_user(p)));
    if mine.is_empty() && !had {
        return;
    }
    let mut by_key: HashMap<&str, Vec<&UserParam>> = HashMap::new();
    for p in &mine {
        by_key.entry(p.key.as_str()).or_default().push(p);
    }
    for (i, e) in m.elements.iter_mut().enumerate() {
        e.props.retain(|p| !is_user(p));
        let k = user_key(e, i);
        if let Some(v) = by_key.get(k.as_str()) {
            for p in v {
                e.props.push(crate::model::Prop { group: p.group.clone(), name: p.name.clone(), value: p.value.clone() });
            }
        }
    }
}

/// numele afisat al unei proprietati
pub fn prop_display_name(n: &str) -> &str {
    n
}

impl App {

    // ------------------------------------------------------------------ import / export de proprietati

}

/// cheia normalizata pentru potrivire (fara spatii la capete, GUID-uri cu litere mici)
fn norm_key(s: &str) -> String {
    let t = s.trim();
    if t.len() == 36 && t.matches('-').count() == 4 { t.to_ascii_lowercase() } else { t.to_string() }
}
