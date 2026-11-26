//! Clash Detective data: tests, results, rules, grouping and severity.
use super::*;

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
    /// viewpoint-ul salvat al rezultatului (ca in Navisworks): la selectare camera merge aici
    #[serde(default)]
    pub view: Option<crate::camera::Camera>,
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
    /// test special: inaltime libera (headroom) sau distanta de evacuare (egress), cu parametrii lui;
    /// rulat de crate::egress in locul detectiei de clash obisnuite
    #[serde(default)]
    pub special: Option<(crate::egress::Check, crate::egress::Params)>,
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
        let special = |name: &str, check: crate::egress::Check, sev: [f32; 2]| {
            let mut t = ClashTest::new(name.to_string());
            t.special = Some((check, crate::egress::Params::default()));
            t.sev_mm = sev;
            t
        };
        vec![
            ("Headroom (clear height 2.10 m)", "Beams, ducts, pipes, trays and stairs lower than 2.10 m above the floor slabs. Change the height in the test", special("Headroom 2.10 m", crate::egress::Check::Headroom, [50.0, 200.0])),
            ("Egress distance (45 m)", "Walking distance to the nearest exit (exterior doors on the ground floor, stairs on upper floors) longer than 45 m. Change the limit in the test", special("Egress 45 m", crate::egress::Check::Egress, [1000.0, 5000.0])),
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
            special: None,
        }
    }
    pub fn count(&self, s: ClashStatus) -> usize {
        self.results.iter().filter(|r| r.status == s).count()
    }
}
