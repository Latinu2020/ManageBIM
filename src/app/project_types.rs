//! Sections, model transforms, viewpoints, the project file (.bimr) and the selection tree.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum SectionMode {
    Planes,
    Box,
}

/// The direction of the section exported to DWG / DXF. `Current` uses the active
/// cut plane; the others cut the model along a direction, at the chosen position.
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

    /// the axis of the cut plane: 0 = constant X, 1 = constant Y, 2 = constant Z (plan)
    pub fn axis(self) -> Option<usize> {
        match self {
            CadView::Front | CadView::Back => Some(1),
            CadView::Left | CadView::Right => Some(0),
            CadView::Top | CadView::Bottom => Some(2),
            CadView::Current => None,
        }
    }

    /// from the back / right the horizontal axis is flipped (as in the view from that direction)
    pub fn mirrored(self) -> bool {
        matches!(self, CadView::Back | CadView::Right)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PlaneDef {
    pub enabled: bool,
    /// 0 Top(+Z) 1 Bottom(-Z) 2 +X 3 -X 4 +Y 5 -Y
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
    /// planes (n, w): keeps the points with n·p <= w
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
    /// the chosen cut plane (Cut View) at the time of saving
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
    /// the name of the View Template active at save time ( the display style is applied on restore )
    #[serde(default)]
    pub template: String,
    /// the measurements (dimensions) visible when the viewpoint was saved
    #[serde(default)]
    pub measures: Vec<Measure>,
    /// the folder (and subfolders, "Nivel 2/Instalatii"); empty = root
    #[serde(default)]
    pub folder: String,
    /// the view look at save time (visual style, grids, levels, caps, colours) - the viewpoint comes back
    /// exactly as it was saved, regardless of which View Template is applied in the meantime
    #[serde(default)]
    pub look: Option<VpLook>,
}


/// The computed Multi view sheet: segments (layer, x0 y0 x1 y1 in m), texts, the frame of each view.
#[derive(Clone, Default)]
pub struct MultiViewSheet {
    pub segs: Vec<(String, [f64; 4])>,
    pub texts: Vec<CadText>,
    pub frames: Vec<(String, [f64; 4])>,
    pub gap: f64,
}

/// The look saved with a viewpoint (independent of View Templates).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VpLook {
    pub visual_style: crate::render::VisualStyle,
    pub show_grids: bool,
    pub show_levels: bool,
    pub section_caps: bool,
    #[serde(default)]
    pub overrides: Vec<(ElemId, [u8; 4])>,
}
#[derive(Serialize, Deserialize)]
pub struct ProjectFile {
    pub app: String,
    pub version: u32,
    pub models: Vec<String>,
    pub origin: Option<[f64; 3]>,
    pub camera: Camera,
    /// point lights (lamps) in the model
    #[serde(default)]
    pub lights: Vec<crate::lights::SceneLight>,
    #[serde(default)]
    pub coord_objects: Vec<crate::coord_objects::CoordObject>,
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
    /// per-element scale (SketchUp style), non-destructive
    #[serde(default)]
    pub elem_scale: Vec<(ElemId, [f32; 3])>,
    #[serde(default)]
    pub animations: Vec<Animation>,
    /// View Templates saved in the project (visibility + section + style)
    #[serde(default)]
    pub view_templates: Vec<ViewTemplate>,
    #[serde(default)]
    pub render: RenderSettings,
    /// user-defined levels (elevation relative to ±0.00) and the real elevation of ±0.00
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
    /// the walk path (points on the floor, metres)
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
    /// the parameters added by the user on elements
    #[serde(default)]
    pub user_params: Vec<UserParam>,
    #[serde(default)]
    pub title_block: Option<crate::pdf::TitleBlock>,
    #[serde(default)]
    pub vp_folders: Vec<String>,
    #[serde(default)]
    pub overlays: Vec<CadOverlay>,
}

/// A grouping level of the selection tree: the property (category, name) to group by.
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

/// The default tree (as before): Level > IFC Type > elements.
pub fn default_tree_levels() -> Vec<TreeLevel> {
    vec![TreeLevel::new(ITEM_CAT, "Level"), TreeLevel::new(ITEM_CAT, "IFC Type")]
}

/// .bimtree file: the tree configuration, reusable in other projects.
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
    /// all elements under this node
    pub ids: Vec<u32>,
    /// empty = the last level (the elements are listed under it)
    pub children: Vec<TreeNode>,
}

/// the selection tree of a model, grouped by the levels chosen by the user
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
    // natural order ("Level 2" < "Level 10"), the "(no ...)" group at the end
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
