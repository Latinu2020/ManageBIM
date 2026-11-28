//! Application state and logic (without the panel drawing, which lives in ui.rs).
use crate::camera::Camera;
use crate::clash::{self, ClashStatus, ClashType};
use crate::model::{Aabb, CadSegs, ElemId, Element, Model};
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
mod coord;
mod mcp_tools;
mod measure_types;
mod search_types;
mod clash_types;
mod project_types;
mod raytrace;
pub use raytrace::RtState;
pub use clash_types::*;
pub use measure_types::*;
pub use project_types::*;
pub use project::{available_memory, estimated_memory};
pub use search_types::*;

#[path = "walk.rs"]
pub mod walk;

/// How an element looks on screen. We keep the last look sent to the GPU so that we update
/// only the elements that changed (a selection click no longer resends the whole model).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Look {
    /// forced color (override / clash / 4D / material); None = the color from the file
    pub col: Option<[u8; 4]>,
    /// material parameters for the shader
    pub gpu: [u8; 4],
    pub sel: bool,
    pub hidden: bool,
}

/// The colors + materials (4 bytes / vertex, 3 vertices / triangle) for the triangles of one element.
/// `tint` = the selection color and how much it covers the element color (Settings › Selection).
fn fill_look(l: &Look, tri_color: &[[u8; 4]], col: &mut [u8], mat: &mut [u8], tint: ([u8; 3], f32)) {
    let ([sr, sg, sb], k) = tint;
    let keep = 1.0 - k;
    for (t, tc) in tri_color.iter().enumerate() {
        let mut c = l.col.unwrap_or(*tc);
        if l.sel {
            c = [(c[0] as f32 * keep + sr as f32 * k) as u8, (c[1] as f32 * keep + sg as f32 * k) as u8, (c[2] as f32 * keep + sb as f32 * k) as u8, c[3].max(200)];
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

/// The whole color buffer of a model, filled in parallel (each element has its own range).
fn full_colors(slot: u32, m: &Model, looks: &[Look], tint: ([u8; 3], f32)) -> Upload {
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
            fill_look(&looks[ei], &m.tri_color[e.tri_start as usize..(e.tri_start + e.tri_count) as usize], c, k, tint);
        });
    }
    Upload::Colors { slot, col, mat }
}

/// Adds the extension if missing ("raport" -> "raport.csv").
fn with_ext(path: &str, ext: &str) -> String {
    if path.to_lowercase().ends_with(&format!(".{ext}")) {
        path.to_string()
    } else {
        format!("{path}.{ext}")
    }
}

/// The property values (category, name) of an element. Category "Item" = the standard fields.
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

/// The last modification date of a file (None if it does not exist / cannot be read).
fn file_mtime(path: &str) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn file_stem(path: &str) -> String {
    std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// %LOCALAPPDATA%\ManageBIM\Inbox - the add-ins write here when they send the model directly to ManageBIM.
pub fn inbox_dir() -> Option<std::path::PathBuf> {
    let d = crate::formats::data_root()?.join("ManageBIM").join("Inbox");
    std::fs::create_dir_all(&d).ok()?;
    Some(d)
}

// ====================================================================== types

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
    /// the exported viewpoints (all of them or only those in one folder)
    pub list: Vec<usize>,
    /// frames waited after the requested screenshot (stop if it does not come)
    pub shot_wait: u32,
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
        png::ColorType::Rgba => px.as_chunks::<4>().0.iter().flat_map(|c| [c[0], c[1], c[2]]).collect(),
        _ => return None,
    };
    Some((rgb, info.width, info.height))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NavMode {
    Orbit,
    Walk,
    /// left click + drag = lateral movement (like Pan in Navisworks)
    Pan,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Purpose {
    OpenModel { append: bool },
    OpenProject,
    SaveProject,
    ExportClashHtml,
    ExportClashPdf,
    ExportClashCsv,
    ExportClashBcf,
    ExportIfc,
    ExportGlb,
    SharedFolder,
    SelectScriptsDir,
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
    /// the sheet with several views of the section box (elevations + top view)
    ExportMultiView,
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
    /// Batch export: destination folder, 3 reports (PDF / HTML / Excel) + STATUS
    ExportBatch,
    /// Shareable settings profile (.mbprofile)
    ExportSettingsProfile,
    ImportSettingsProfile,
    ExportBcf,
    ExportNavisXml,
    ImportNavisXml,
    ExportNavisSets,
    ExportNavisClash,
    ExportMaterials,
    ImportMaterials,
    MaterialTexture,
    MaterialNormal,
    MaterialRoughness,
    MaterialMetallic,
    MaterialAo,
    MaterialHeight,
    ExportMaterialX,
    ExportMaterialOne,
    ExportDashboardPdf,
    ExportDashboardDocx,
    ImportMaterialX,
    LoadHdri,
    ExportIfcMap,
    ImportIfcMap,
    ImportClassifications,
    ExportClassifications,
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
    ImportPriceCatalog,
    ExportCostXlsx,
    ExportCostCsv,
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
    /// the Cancel button on the progress bar
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
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
    pub tx: Option<std::sync::mpsc::SyncSender<Frame>>,
    pub done: Receiver<Result<(), String>>,
    pub path: std::path::PathBuf,
    pub waiting: bool,
    pub started: Instant,
    pub saved_sel: BTreeSet<ElemId>,
    /// Render with materials: the previous view style and materials (restored at the end)
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
    /// removes the model (the slot) from the project; the file on disk stays untouched
    RemoveModel(usize),
}

/// A grid import waiting for confirmation: (file, (layer, count) rows, chosen layer).
pub type GridImport = (String, Vec<(String, usize)>, String);

pub struct App {
    pub models: Vec<Option<Arc<Model>>>,
    pub trees: Vec<Option<ModelTree>>,
    pub origin: Arc<Mutex<Option<DVec3>>>,
    pub renderer: Arc<Mutex<Renderer>>,
    pub cam: Camera,
    /// point lights placed in the model (lamps)
    pub lights: Vec<crate::lights::SceneLight>,
    /// the light selected in the Lighting panel / viewport
    pub light_sel: Option<usize>,
    /// UCS axis (0=X, 1=Y, 2=Z) grabbed for moving the light
    pub light_axis: Option<usize>,
    /// rotating the light direction through the gizmo circle: (rotation axis 0=X 1=Y 2=Z, the angle from the
    /// previous frame, true = angle measured on screen - the circle plane is seen edge-on)
    pub light_rot: Option<(usize, f32, bool)>,
    /// the light gizmo mode: false = Move (the arrows), true = Rotate (the circles)
    pub light_rotate_mode: bool,
    /// the search filter in the classification catalog
    pub class_search: String,
    /// the coordination objects (clearance / provision for void / placeholder) and their virtual model
    pub coord_objects: Vec<crate::coord_objects::CoordObject>,
    pub coord_slot: Option<usize>,
    pub coord_sel: Option<usize>,
    pub coord_open: bool,
    /// the type chosen from the ribbon, waits for the user confirmation in the window (not created automatically)
    pub coord_pending: Option<crate::coord_objects::CoordKind>,
    /// the face of the coordination object dragged with the mouse (0..5 = -X +X -Y +Y -Z +Z, 6 = move in plane)
    pub coord_drag: Option<usize>,
    /// the coordination objects gizmo: 0 = Resize (faces), 1 = Move (X/Y/Z), 2 = Rotate (vertical)
    pub coord_mode: u8,
    /// the cursor angle at the previous step of the rotation
    pub coord_rot_last: Option<f32>,
    /// the margin of the maintenance zones around the selection and of the automatic voids (m)
    pub coord_margin: f32,
    pub void_margin: f32,
    /// the Classification window (local catalog + online bSDD)
    pub class_open: bool,
    /// 0 = local catalog, 1 = online bSDD
    pub class_tab: usize,
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
    /// Clash Detective: the tests, the running job and the display of the results.
    pub clash: ClashState,
    /// Render materials: the assignment rules, the library window and its thumbnails.
    pub mats: MaterialsState,
    /// The MCP server for AI assistants: the server, its window and status, a capture waiting for its image.
    pub mcp: McpState,
    pub cur_test: usize,
    pub cur_result: Option<usize>,
    pub sel_results: BTreeSet<usize>,
    pub open_groups: HashSet<String>,
    pub find_in_selection: bool,
    pub group_cell: f32,
    /// Model focus: ribbon and panels hidden, the properties stay floating (maximum space for the model)
    pub immersive_mode: bool,
    /// the layout saved before Model focus, so it can be restored exactly
    pub saved_layout: Option<crate::layout::UiLayout>,
    /// the maximized floating windows (the ⤢ button in the header)
    pub float_max: std::collections::HashSet<crate::layout::Panel>,
    pub tasks: Vec<Task>,
    pub cur_task: Option<usize>,
    pub loads: Vec<LoadJob>,
    pub toasts: Vec<(String, Instant, bool)>,
    pub log: Vec<String>,
    pub project_path: Option<String>,
    pub prompt: Option<PathPrompt>,
    pub pending_project: Option<ProjectFile>,
    pub pivot: Option<Vec3>,
    pub last_frame: Instant,
    pub fps: f32,
    /// what the last frame drew after culling (Performance panel / MCP `get_performance`)
    pub perf: crate::render::DrawStats,
    pub show_help: bool,
    pub show_about: bool,
    /// the close confirmation (window X / Alt+F4 / File › Exit) and the "already confirmed" flag
    pub exit_ask: bool,
    pub exit_ok: bool,
    /// the Lighting window (View › Scene)
    pub show_light: bool,
    /// physical pixels per logical point (for the point size)
    pub ppp: f32,
    /// grids and levels on the model (computed once per model)
    pub grid_cache: std::cell::RefCell<HashMap<usize, (usize, Arc<GridData>)>>,
    /// levels: the user's own levels, the project zero and the display switches
    pub level: LevelState,
    pub show_grids: bool,
    /// autosave + recovery after an unexpected close
    pub autosave: Autosave,
    /// the shared folder for issues (network / SharePoint); None = not shared
    pub shared_dir: Option<String>,
    pub shared: crate::shared::SharedState,
    /// new, empty project (New): no start page, with Save available
    pub blank_project: bool,
    /// grids created in ManageBIM (real coordinates, meters)
    pub user_grids: Vec<UserGrid>,
    pub use_user_grids: bool,
    /// the size of the grid bubbles (px) and the extension of the lines beyond the ends (m)
    pub grid_bubble: f32,
    pub grid_ext: f32,
    pub show_grids_editor: bool,
    /// the panel opened from an auto-hide area and the last time the mouse was over it
    pub flyout: Option<(crate::layout::Dock, crate::layout::Panel)>,
    pub flyout_seen: Instant,
    /// relocation in progress (move / rotate / align to face)
    pub xform: Option<XformPick>,
    pub relocate_selection: bool,
    /// selection window in progress (the start point, on screen)
    pub box_start: Option<[f32; 2]>,
    /// the IFC export: only the selection (true) or everything visible
    pub ifc_selection: bool,
    /// the web export: only the selection
    pub web_selection: bool,
    /// Live Link (selection synchronized with Revit / Navisworks)
    pub sync: crate::sync::SyncState,
    /// as-built: the result, the parameters, the window, the elements colored by us
    pub asbuilt: Option<Vec<crate::asbuilt::ElemResult>>,
    pub asbuilt_params: crate::asbuilt::Params,
    pub asbuilt_open: bool,
    pub asbuilt_show: bool,
    pub asbuilt_painted: Vec<ElemId>,
    pub asbuilt_filter: Option<crate::asbuilt::Status>,
    /// the command palette (Ctrl+K)
    pub palette_open: bool,
    pub palette_q: String,
    pub palette_sel: usize,
    /// the built-in manual
    pub manual_open: bool,
    pub manual_page: usize,
    pub manual_q: String,
    /// Cut to DWG: on an A3 sheet with title block (otherwise 2D in Model space)
    pub cad_sheet: bool,
    /// Cut to DWG: full plan (hatch, labels, grids with dimensions, levels on sections)
    pub cad_annotate: bool,
    /// Cut to DWG: include the chain dimensions in the exported drawing
    pub cad_dims: bool,
    /// Cut to DWG: include the level lines + their labels in the exported drawing
    pub cad_levels: bool,
    /// Cut to DWG: hatch on the cut elements (off = clean drawing, outline only)
    pub cad_hatch: bool,
    /// the 2D drawing: the visible edges of the shape behind the cut plane (like the views in Inventor)
    pub cad_visible_edges: bool,
    /// the 2D drawing: the hidden edges, with a dashed line
    pub cad_hidden_edges: bool,
    /// the cache of the visible / hidden lines (key = the view + the models), the computation is expensive
    pub hlr_cache: std::sync::Mutex<Option<(String, CadSegs)>>,
    /// picking a face of the section box with the mouse, for the 2D drawing
    pub face_pick: bool,
    /// the view history (Undo / Redo): the states, the current position, the signature of the last state
    pub hist: Vec<ViewSnap>,
    /// view templates saved in the project, applied with one click
    pub view_templates: Vec<ViewTemplate>,
    /// the manage window for the view templates
    pub view_templates_open: bool,
    /// the index of the current template (for ◀ ▶ navigation next to the cube)
    pub vt_current: usize,
    /// a View Template was applied explicitly (otherwise the new viewpoints are not linked to any)
    pub vt_applied: bool,

    pub hist_pos: usize,
    pub hist_sig: u64,
    /// 2D drawings overlaid on the model and their window
    pub overlays: Vec<CadOverlay>,
    pub overlay_open: bool,
    /// the Multi view window: the ticked views (Front, Right, Back, Left, Top, Bottom), distance (m, 0 = auto), titles
    pub mv_open: bool,
    pub mv_views: [bool; 6],
    pub mv_gap: f32,
    pub mv_titles: bool,
    pub mv_cache: Option<(String, Arc<MultiViewSheet>)>,
    /// the 2D preview: zoom (1 = fits the window) and pan (pixels)
    pub prev_zoom: f32,
    pub prev_pan: [f32; 2],
    /// the title block of the sheets (PDF / DWG) and its editing window
    pub title_block: crate::pdf::TitleBlock,
    pub title_block_open: bool,
    /// the section box face being dragged now (0..5: -X +X -Y +Y -Z +Z)
    pub box_drag: Option<usize>,
    pub show_clash_matrix: bool,
    /// the last grid import from DWG / DXF: the file, the layers with lines, the layer used
    pub grid_import: Option<GridImport>,
    /// the grid generator (the origin and direction can be picked with a click in the model)
    pub grid_gen: GridGen,
    /// picking in 3D: 0 nothing, 1 the origin, 2 the direction of the first grid
    pub grid_pick: u8,
    /// OSNAP active (F3 toggles); when off, snapping to geometry is disabled
    pub osnap_on: bool,
    /// Split-view: the second viewport (comparison). its own camera/section.
    pub split_view: bool,
    /// the splitter position (0.05..0.95) - dragged with the mouse
    pub split_ratio: f32,
    /// cameras synchronized between the two halves
    pub split_link: bool,
    /// batch export queue (kind, path) - runs sequentially
    pub batch_queue: Vec<(u8, String)>,
    /// the camera and section of the right half
    pub cam_b: crate::camera::Camera,
    pub section_b: crate::app::Section,
    /// the project opened from a .bimd: Save publishes it there again
    pub bimd_path: Option<String>,
    /// the "save?" question before returning to the start page
    pub confirm_start: bool,
    /// the start page shown over the open project (like Navisworks: back arrow to the project)
    pub show_start: bool,
    /// what to do after the "save?" question (open / new project from the start page)
    pub start_pending: Option<StartAction>,
    /// start page: sorting (0 last opened, 1 name, 2 modification date, 3 type), search, list / grid
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
    /// scale per-element (stil SketchUp) - non-distructiv
    pub elem_scale: HashMap<ElemId, [f32; 3]>,
    pub xf_draft: HashMap<usize, ModelXf>,
    pub move_delta: [f64; 3],
    pub move_target: Option<[f64; 3]>,
    pub animations: Vec<Animation>,
    pub cur_anim: usize,
    pub render: RenderSettings,
    pub anim_play: Option<f32>,
    pub anim_time: f32,
    pub anim_loop: bool,
    /// ping-pong (P.P. in Navisworks): at the end, the animation goes backwards
    pub anim_pp: bool,
    /// Import 4D: after importing the schedule the 4D animation is created
    pub anim_4d_pending: bool,
    /// a model was loaded: the TimeLiner tasks without elements are linked again
    pub relink_tasks: bool,
    /// the playback direction: 1 forward, -1 backward
    pub anim_dir: f32,
    /// the state of the animated objects at the current time: element -> (rgba color, visible)
    pub anim_obj: HashMap<ElemId, ([u8; 4], bool)>,
    /// Record: recording the navigation in real time (start, keyframes)
    pub anim_rec: Option<(Instant, Vec<crate::anim::Keyframe>)>,
    pub anim_sim_active: bool,
    pub render_job: Option<RenderJob>,
    /// the final render (Render › Path Trace): job, window, picture, settings
    pub pt: PathTraceState,
    /// the last font applied (so we reinstall the fonts only on change)
    pub font_family_cur: String,
    /// the cut plane is dragged with the mouse in 3D (the part from the cube)
    pub cut_drag: bool,
    /// the script console window (the Rhai engine is enabled in 0.28)
    pub script_open: bool,
    pub script_help_open: bool,
    /// the current scenario of the visual script editor (steps that generate Rhai code)
    pub scenario: crate::script_builder::Scenario,
    /// the model properties for the visual editor (key = the signature of the loaded models)
    pub scenario_index: Option<(u64, std::sync::Arc<crate::script_builder::PropIndex>)>,
    /// the application settings window (View › Settings / the ⚙ button)
    pub settings_open: bool,
    /// interactive Ray Trace view (GPU path tracing in the viewport, like Navisworks)
    pub rt: RtState,
    /// the IFC Mapping window (BIM › Map classes)
    pub ifc_map_open: bool,
    pub still_path: Option<String>,
    /// (width, height, capture the live Ray Trace view instead of rendering again)
    pub pending_capture: Option<(u32, u32, bool)>,
    /// the image (Render Image): the previous view (style, materials, selection), restored after the capture
    pub still_restore: Option<(crate::render::VisualStyle, bool, BTreeSet<ElemId>)>,
    /// frames to wait before the capture (the new colors / materials reach the GPU)
    pub still_wait: u8,
    pub ffmpeg: Option<bool>,
    pub orbit_secs: f32,
    pub key_step: f32,
    pub proj_sel: Option<usize>,
    pub materials: Vec<Material>,
    pub materials_on: bool,
    /// the material names from the user library (cache; None = to be reread)
    pub user_lib_cache: Option<std::collections::HashSet<String>>,
    pub render_subtab: u8,
    pub show_tools: bool,
    pub sets_sig: u64,
    pub tex_layers: Vec<(String, Vec<u8>)>,
    /// PBR material key (all maps + intensities + resolution) -> the first layer of the albedo/normal pair
    pub pbr_layers: HashMap<String, u8>,
    /// the resolution with which tex_layers were loaded (on change everything is reloaded)
    pub tex_loaded_res: u32,
    /// recently imported PBR materials, waiting for an assignment choice
    pub pending_mat_assign: Vec<String>,
    /// the download of the ambientCG professional library (CC0) in progress
    pub pro_lib_job: Option<std::sync::Arc<std::sync::Mutex<crate::pro_materials::Progress>>>,
    /// the notification history (the bell in the status bar): (when, error, message) and how many were seen
    pub notes: Vec<(Instant, bool, String)>,
    pub notes_seen: usize,
    /// the window with the keyboard shortcuts (F1)
    pub keys_open: bool,
    /// the "Smart set" window (search sets from values, suggestions, word search)
    pub smart: crate::smart_sets::Builder,
    /// the Dashboard window and the displayed summary (rebuilt on open / Refresh / export)
    pub dash_open: bool,
    pub dash: Option<crate::dashboard::Summary>,
    /// the confirmed content of each search set (GUIDs): after a model update the Sets panel shows +/-
    pub set_snap: HashMap<String, std::collections::HashSet<String>>,
    /// the Dashboard export with the clash images (one capture per frame)
    pub dash_job: Option<crate::dashboard::CaptureJob>,
    pub tex_failed: HashSet<String>,
    pub pending_csv: Vec<String>,
    pub settings: crate::theme::Settings,
    pub theme_dirty: bool,
    pub last_mode: (bool, bool),
    /// Update All: the previous states (for the summary) and the summary at the end
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
    /// target velocity in this frame (local: x right, y forward, z up), in multiples of the walking speed
    pub walk_target: Vec3,
    /// target rotation in this frame (rad/s)
    pub walk_turn_target: f32,
    /// current velocity / rotation, smoothed (smooth acceleration and braking)
    pub walk_vel: Vec3,
    pub walk_turn: f32,
    /// the drawn walk path (points on the floor)
    pub walk_path: Vec<Vec3>,
    /// the path point dragged with the mouse (the Draw tool)
    pub wp_drag: Option<usize>,
    /// automatic walking along the path, when running
    pub path_run: Option<walk::PathRun>,
    /// the Inbox files already opened in this session (Send to ManageBIM)
    pub inbox_seen: HashSet<String>,
    /// the section box follows the selection (the Follow button)
    pub section_follow: bool,
    pub section_sel_sig: u64,
    /// the signature of the last write of Outbox\issues.txt (the Revit / Navisworks add-ins)
    pub outbox_sig: u64,
    /// simplified drawing: the camera from the last frame, the time of the last movement, the elements with transparency
    pub lod_cam: Camera,
    pub lod_moved: Instant,
    /// the LOD threshold while moving (pixels), adapted by FPS
    pub lod_px: f32,
    pub alpha_cache: HashMap<usize, (usize, Vec<bool>)>,
    /// Refresh: the modification date of the source file at load time (per slot)
    pub file_stamps: HashMap<usize, std::time::SystemTime>,
    /// Refresh: the old model, kept until the reload finishes (for remapping the selection by GUID)
    pub reloading: HashMap<usize, Arc<Model>>,
    /// Refresh: the slots whose files changed on disk since loading
    pub changed_files: BTreeSet<usize>,
    pub last_file_check: Instant,
    /// the look last sent to the GPU, per slot (incremental color update)
    pub looks: HashMap<usize, Vec<Look>>,
    /// Appearance Profiler: rules + element -> rule map (computed when needed)
    pub profile: crate::profiler::Profile,
    pub profile_map: Option<HashMap<ElemId, u16>>,
    /// Compare: result + 3D display + the chosen models (A = old, B = new) + tolerance (mm)
    pub compare: Option<crate::compare::CompareResult>,
    /// the changed properties (compare by parameters) and the list filter
    pub compare_params: Vec<crate::compare::ParamChange>,
    /// the quick checks (without IDS)
    pub quick_checks: Vec<crate::check::QuickCheck>,
    /// the legend of the coloring by parameter in the 3D view
    pub profile_legend: bool,
    /// the parameters added by the user (saved in the project) and the fields in the Properties panel
    pub user_params: Vec<UserParam>,
    pub up_name: String,
    pub prop_import: Option<PropImport>,
    /// the user properties: what is theirs on each element + the original values replaced
    pub user_shadow: Mutex<UserShadow>,
    /// the Property editor window (free properties or IFC standard sets on the selection)
    pub pe: crate::ui::PropEditor,
    /// the user data tab displayed / edited in Properties
    pub user_tab: String,
    /// empty tabs created with Add User Data Tab (still without properties)
    pub user_tabs_new: Vec<String>,
    /// renaming the current tab (the text in the edit field)
    pub user_tab_rename: Option<String>,
    pub up_value: String,
    pub compare_param_pick: Option<String>,
    pub compare_on: bool,
    /// the map of geometric deviations on the changed elements; scale in mm (0 = automatic)
    pub compare_heat: bool,
    pub compare_heat_max_mm: f32,
    pub compare_pick: (Option<usize>, Option<usize>),
    pub compare_tol: f32,
    /// Model Checker: the rules, the last result, the chosen rule and the IDS import warnings
    pub check_rules: Vec<crate::check::Rule>,
    pub check_results: Option<Vec<crate::check::RuleResult>>,
    pub check_sel: Option<usize>,
    pub check_warn: Vec<String>,
    /// the grouping editor of the selection tree is open
    pub tree_edit: bool,
    /// Quantification: the configuration (saved in the project / .bimqto template) and the last result
    pub qto: crate::qto::QtoSetup,
    pub qto_result: Option<crate::qto::QtoResult>,
    /// the signature of the state for which the result was computed (setup + selection / hidden)
    pub qto_sig: u64,
    /// 5D cost: the price catalog and the quantity column used for the priced take-off
    pub cost: crate::cost::CostSetup,
    /// 4D simulation: the day shown, playback and the element states of that day
    pub sim: Sim4dState,
    /// point cloud display: density, point size and shape
    pub cloud: CloudState,
    /// the update check and download (Help › Check for updates)
    pub upd: UpdateState,
    /// the bSDD search (Classification window)
    pub bsdd: BsddState,
    /// the viewpoint folders shown, filter and pending rename
    pub vp: ViewpointUi,
    /// section options: caps, plane display, level cuts and the 2D preview
    pub sec: SectionUi,
}

impl App {
    pub fn new(files: Vec<String>) -> App {
        let mut app = App {
            sec: SectionUi::default(),
            vp: ViewpointUi::default(),
            bsdd: BsddState::default(),
            upd: UpdateState::default(),
            cloud: CloudState::default(),
            sim: Sim4dState::default(),
            models: Vec::new(),
            trees: Vec::new(),
            origin: Arc::new(Mutex::new(None)),
            renderer: Arc::new(Mutex::new(Renderer::new())),
            cam: Camera::default(),
            lights: Vec::new(),
            light_sel: None,
            light_axis: None,
            light_rot: None,
            light_rotate_mode: false,
            class_search: String::new(),
            coord_objects: Vec::new(),
            coord_slot: None,
            coord_sel: None,
            coord_open: false,
            coord_pending: None,
            coord_drag: None,
            coord_mode: 0,
            coord_rot_last: None,
            coord_margin: 0.6,
            void_margin: 0.05,
            class_open: false,
            class_tab: 0,
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
            clash: ClashState { tests: vec![ClashTest::new("Test 1".into())], job: None, focus: None, others: Others::Dim, highlight_all: false, auto_zoom: true, subtab: 0, filter: None, filter_sel: 0, focus_mode: false, queue: Vec::new() },
            mats: MaterialsState { assign: Vec::new(), sel: 0, cache: None, preview: None, filter: String::new(), window: false, scroll: false, thumbs: HashMap::new(), thumb_job: None },
            mcp: McpState { server: None, open: false, status: String::new(), capture: None },
            cur_test: 0,
            cur_result: None,
            sel_results: BTreeSet::new(),
            open_groups: HashSet::new(),
            find_in_selection: false,
            group_cell: 5.0,
            immersive_mode: false,
            saved_layout: None,
            float_max: std::collections::HashSet::new(),
            tasks: Vec::new(),
            cur_task: None,
            loads: Vec::new(),
            toasts: Vec::new(),
            log: Vec::new(),
            project_path: None,
            prompt: None,
            pending_project: None,
            pivot: None,
            last_frame: Instant::now(),
            fps: 0.0,
            perf: Default::default(),
            show_help: false,
            show_about: false,
            exit_ask: false,
            exit_ok: false,
            show_light: false,
            ppp: 1.0,
            grid_cache: Default::default(),
            level: LevelState { show_lines: true, ..Default::default() },
            show_grids: true,
            autosave: Autosave::default(),
            shared_dir: None,
            shared: Default::default(),
            blank_project: false,
            user_grids: Vec::new(),
            use_user_grids: false,
            grid_bubble: 11.0,
            grid_ext: 2.0,
            show_grids_editor: false,
            flyout: None,
            flyout_seen: Instant::now(),
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
            cad_dims: true,
            cad_levels: true,
            cad_hatch: true,
            cad_visible_edges: true,
            cad_hidden_edges: false,
            hlr_cache: std::sync::Mutex::new(None),
            face_pick: false,
            hist: Vec::new(),
            view_templates: Vec::new(),
            view_templates_open: false,
            vt_current: 0,
            vt_applied: false,
            hist_pos: 0,
            hist_sig: 0,
            overlays: Vec::new(),
            overlay_open: false,
            mv_open: false,
            mv_views: [true, true, true, true, true, false],
            mv_gap: 0.0,
            mv_titles: true,
            mv_cache: None,
            prev_zoom: 1.0,
            prev_pan: [0.0, 0.0],
            title_block: Default::default(),
            title_block_open: false,
            box_drag: None,
            show_clash_matrix: false,
            grid_import: None,
            grid_gen: GridGen::default(),
            grid_pick: 0,
            osnap_on: true,
            split_view: false,
            split_ratio: 0.5,
            split_link: true,
            batch_queue: Vec::new(),
            cam_b: crate::camera::Camera::default(),
            section_b: crate::app::Section::default(),
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
            elem_scale: HashMap::new(),
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
            pt: PathTraceState::default(),
            font_family_cur: "Inter".into(),
            cut_drag: false,
            script_open: false,
            scenario: crate::script_builder::Scenario { name: "My script".into(), steps: Vec::new() },
            scenario_index: None,
            script_help_open: false,
            settings_open: false,
            rt: RtState::default(),
            ifc_map_open: false,
            still_path: None,
            pending_capture: None,
            still_restore: None,
            still_wait: 0,
            ffmpeg: None,
            orbit_secs: 20.0,
            key_step: 3.0,
            proj_sel: None,
            materials: crate::materials::full_library(),
            materials_on: true,
            user_lib_cache: None,
            render_subtab: 0,
            show_tools: false,
            sets_sig: 0,
            tex_layers: Vec::new(),
            pbr_layers: HashMap::new(),
            tex_loaded_res: crate::materials::tex_size(),
            pending_mat_assign: Vec::new(),
            pro_lib_job: None,
            notes: Vec::new(),
            notes_seen: 0,
            keys_open: false,
            smart: Default::default(),
            dash_open: false,
            dash: None,
            set_snap: HashMap::new(),
            dash_job: None,
            tex_failed: HashSet::new(),
            pending_csv: Vec::new(),
            settings: crate::theme::Settings::load(),
            theme_dirty: true,
            last_mode: (false, false),
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
            wp_drag: None,
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
            user_shadow: Mutex::new(UserShadow::default()),
            pe: Default::default(),
            user_tab: String::new(),
            user_tabs_new: Vec::new(),
            user_tab_rename: None,
            up_value: String::new(),
            compare_param_pick: None,
            compare_on: false,
            compare_heat: true,
            compare_heat_max_mm: 0.0,
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
            cost: Default::default(),
        };
        {
            let p = &app.settings.clash_ui;
            app.clash.subtab = p.subtab;
            app.clash.filter = p.filter;
            app.clash.filter_sel = p.filter_sel;
            app.clash.others = p.others;
            app.clash.auto_zoom = p.auto_zoom;
            app.clash.highlight_all = p.highlight_all;
            app.group_cell = p.group_cell;
            app.sim.only_linked = p.only_linked;
        }
        // external converters: detected at the first start (or when they are missing)
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

    // ------------------------------------------------------------------ messages
    /// Saves the UI preferences (Clash / 4D) when they change, without writing the file on every frame.
    pub fn sync_ui_prefs(&mut self) {
        let p = crate::theme::ClashUiPrefs {
            subtab: self.clash.subtab,
            filter: self.clash.filter,
            filter_sel: self.clash.filter_sel,
            others: self.clash.others,
            auto_zoom: self.clash.auto_zoom,
            highlight_all: self.clash.highlight_all,
            group_cell: self.group_cell,
            only_linked: self.sim.only_linked,
            // the rest (sorting, colors, panels) is edited directly in settings.clash_ui
            ..self.settings.clash_ui.clone()
        };
        if p != self.settings.clash_ui {
            self.settings.clash_ui = p;
            self.settings.save();
        }
    }

    /// Model focus: maximum space for the model, the properties stay floating.
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

    /// the notification history (the last 200)
    fn note(&mut self, err: bool, m: &str) {
        self.notes.push((Instant::now(), err, m.to_string()));
        if self.notes.len() > 200 {
            let extra = self.notes.len() - 200;
            self.notes.drain(..extra);
            self.notes_seen = self.notes_seen.saturating_sub(extra);
        }
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        let m = msg.into();
        self.log.push(m.clone());
        self.note(false, &m);
        self.toasts.push((m, Instant::now(), false));
    }
    pub fn error(&mut self, msg: impl Into<String>) {
        let m = msg.into();
        self.log.push(format!("ERROR: {m}"));
        self.note(true, &m);
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

/// The text of an internal error (panic) caught with catch_unwind.
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

/// The grids and levels of a model, in local (scene) coordinates.
#[derive(Default, Clone)]
pub struct GridData {
    /// (label, end 1, end 2)
    pub grids: Vec<(String, Vec3, Vec3)>,
    /// (name, local Z elevation), sorted by elevation
    pub levels: Vec<(String, f32)>,
}

impl App {

}

/// The grid location (the two closest non-parallel grids in plan) + the level below the point.
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
        // the second grid: the closest one that is not (nearly) parallel to the first
        match lines.iter().skip(1).find(|l| l.2.dot(first.2).abs() < 0.87) {
            Some(second) => {
                // letter before the digit, as on the drawings (C-4, not 4-C)
                let (x, y) = if first.1.chars().next().is_some_and(|c| c.is_ascii_digit()) && !second.1.chars().next().is_some_and(|c| c.is_ascii_digit()) { (second.1, first.1) } else { (first.1, second.1) };
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

/// User-defined level: name and elevation (m) relative to the project ±0.00.
/// Clash Detective: the tests, the running job and the display of the results.
pub struct ClashState {
    pub tests: Vec<ClashTest>,
    pub job: Option<ClashJob>,
    pub focus: Option<ClashFocus>,
    pub others: Others,
    pub highlight_all: bool,
    pub auto_zoom: bool,
    pub subtab: u8,
    pub filter: Option<ClashStatus>,
    /// Filter by Selection: 0 = off, 1 = inclusive, 2 = exclusive (like in Navisworks)
    pub filter_sel: u8,
    /// Clash Detective in Focus mode: maximized window, with all the space for tests and results
    pub focus_mode: bool,
    pub queue: Vec<usize>,
}

/// The material thumbnail worker: requests (material name) and results (name, thumbnail).
pub type ThumbJob = (std::sync::mpsc::Sender<String>, std::sync::mpsc::Receiver<(String, Option<crate::materials::Thumb>)>);

/// Render materials: the assignment rules, the library window and its thumbnails.
pub struct MaterialsState {
    pub assign: Vec<MatAssign>,
    pub sel: usize,
    pub cache: Option<HashMap<ElemId, u16>>,
    /// the preview of the color map of the selected material
    pub preview: Option<(String, eframe::egui::TextureHandle)>,
    /// the material list filter and the texture thumbnails (path -> image; None = in progress / failed)
    pub filter: String,
    /// the large Materials window (over the full height of the application window)
    pub window: bool,
    /// the list scrolls to the selected material (after an import)
    pub scroll: bool,
    pub thumbs: HashMap<String, Option<eframe::egui::TextureHandle>>,
    pub thumb_job: Option<ThumbJob>,
}

/// The MCP server for AI assistants: the server, its window and status, a capture waiting for its image.
pub struct McpState {
    /// the MCP server started (AI assistants) and its connection window
    pub server: Option<crate::mcp::Server>,
    pub open: bool,
    pub status: String,
    /// the response to an MCP capture_view request, sent when the image is ready (after the next frame)
    pub capture: Option<std::sync::mpsc::Sender<Result<serde_json::Value, String>>>,
}

/// State of the 4D simulation (Time tab): the day shown, playback and the cached element states.
pub struct Sim4dState {
    pub active: bool,
    pub day: f32,
    pub play: bool,
    pub speed: f32,
    pub cache: HashMap<ElemId, SimState>,
    pub cache_day: i32,
    /// 4D: shows only the elements linked to tasks (the rest of the model is hidden)
    pub only_linked: bool,
}

impl Default for Sim4dState {
    fn default() -> Self {
        Sim4dState {
            active: false,
            day: 0.0,
            play: false,
            speed: 7.0,
            cache: HashMap::new(),
            cache_day: i32::MIN,
            only_linked: true,
        }
    }
}

/// How point clouds are drawn (View › Point Cloud).
pub struct CloudState {
    /// point clouds: displayed density (0..1), point size (px), control window
    pub density: f32,
    /// point clouds: 0 = scan colors, 1 = by elevation, 2 = gray
    pub mode: u8,
    /// round points with relief (otherwise square, as before)
    pub round: bool,
    /// the real point size (mm) - close to the cloud the points grow and fill the gaps; 0 = pixels only
    pub world_mm: f32,
    pub point_size: f32,
    pub show: bool,
}

impl Default for CloudState {
    fn default() -> Self {
        CloudState {
            density: 0.5,
            mode: 0,
            round: true,
            world_mm: 6.0,
            point_size: 2.0,
            show: false,
        }
    }
}

/// The check for a newer ManageBIM on GitHub and its download.
#[derive(Default)]
pub struct UpdateState {
    /// update check (GitHub): in progress, the result, the installer download
    pub rx: Option<std::sync::mpsc::Receiver<crate::update::CheckResult>>,
    pub info: Option<crate::update::UpdateInfo>,
    pub dl_rx: Option<std::sync::mpsc::Receiver<Result<std::path::PathBuf, String>>>,
    pub checked: bool,
    pub manual: bool,
    pub open: bool,
}


/// The buildingSMART Data Dictionary search: query, chosen dictionary, results and the requests in flight.
#[derive(Default)]
pub struct BsddState {
    pub query: String,
    /// URI of the chosen bSDD dictionary (empty = all)
    pub dict: String,
    pub results: Vec<crate::bsdd::BsddClass>,
    pub total: usize,
    pub status: String,
    pub rx: Option<std::sync::mpsc::Receiver<crate::bsdd::SearchResult>>,
    pub dicts: Vec<crate::bsdd::BsddDictionary>,
    pub dicts_rx: Option<std::sync::mpsc::Receiver<Result<Vec<crate::bsdd::BsddDictionary>, String>>>,
}


/// How the Saved Viewpoints panel is browsed: current folder, folder list, filter and a rename waiting to be applied.
#[derive(Default)]
pub struct ViewpointUi {
    /// the viewpoint folders: the current one (where new viewpoints go), the ones created empty, the export filter
    pub folder_cur: String,
    pub folders: Vec<String>,
    pub filter: Option<String>,
    pub rename_pending: Option<(String, String)>,
}


/// Options of the section tools: caps, the visible plane, the plan cut at a level and the 2D preview window.
pub struct SectionUi {
    /// section: the cut faces filled (hatched caps)
    pub caps: bool,
    /// the section plane drawn as a transparent green surface
    pub show_plane: bool,
    /// the current level section (for PgUp / PgDn), the level box margins (m) and the plan cut height
    pub level: Option<usize>,
    pub level_below: f32,
    pub level_above: f32,
    pub plan_cut_h: f32,
    /// the small 2D preview window of the section (before export), closable
    pub preview_open: bool,
}

impl Default for SectionUi {
    fn default() -> Self {
        SectionUi {
            caps: true,
            show_plane: true,
            level: None,
            level_below: 0.0,
            level_above: 0.0,
            plan_cut_h: 1.2,
            preview_open: false,
        }
    }
}

/// The final path-traced render (Render › Path Trace).
pub struct PathTraceState {
    /// the render running or finished (CPU or GPU)
    pub job: Option<std::sync::Arc<crate::pathtrace::PathTraceJob>>,
    /// the Path Trace window is open
    pub open: bool,
    /// the picture shown in the window
    pub tex: Option<eframe::egui::TextureHandle>,
    /// width, height, samples per pixel, time limit (s)
    pub params: (u32, u32, u32, f32),
    /// the "finished" notification was shown
    pub notified: bool,
    /// seconds elapsed when it finished (the timer stops at "Ready")
    pub done_at: Option<f32>,
}

impl Default for PathTraceState {
    fn default() -> Self {
        PathTraceState { job: None, open: false, tex: None, params: (256, 256, 32, 30.0), notified: false, done_at: None }
    }
}

/// Level state of the project (Level & Storey Manager).
#[derive(Default)]
pub struct LevelState {
    /// the user's own levels (elevations from ±0.00, in metres)
    pub user: Vec<UserLevel>,
    /// real Z of ±0.00 (metres, model coordinates)
    pub zero: f64,
    /// use `user` instead of the levels found in the models
    pub custom: bool,
    /// level lines and labels in the 3D view
    pub show_lines: bool,
    /// the Level & Storey Manager window is open
    pub show_editor: bool,
}

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

// ------------------------------------------------------------------ autosave
/// %LOCALAPPDATA%\ManageBIM\Autosave
pub fn autosave_dir() -> Option<std::path::PathBuf> {
    crate::formats::data_root().map(|d| d.join("ManageBIM").join("Autosave"))
}

/// The autosave state: the session lock (kept open while the application runs) and what can be recovered.
#[derive(Default)]
pub struct Autosave {
    pub lock: Option<std::fs::File>,
    pub last: Option<Instant>,
    pub sig: u64,
    /// (the autosaved file, the original project if any, the date)
    pub recover: Option<(String, Option<String>, String)>,
}

impl Autosave {
    /// At startup: a lock left by a session that did not close normally = that session stopped abruptly.
    /// The locks of sessions still running cannot be deleted (the file is open), so they stay untouched.
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

    /// On a normal exit: the session lock and autosave are deleted.
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

/// Short local date (for messages).
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

/// A grid created in ManageBIM: the label and the ends in real coordinates (meters, XY).
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

/// Labels number `i` (0..) in the style A, B, … Z, AA, AB … or 1, 2, 3 …
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

/// Grid system: `nx` vertical grids (along Y) at spacing `dx`, labels from `fx`; `ny` horizontal grids
/// (along X) at `dy`, labels from `fy`. Origin = intersection of the first grids; `ext` = how far the lines extend.
#[allow(clippy::too_many_arguments)]
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

/// The grids as DXF (R12, meters): lines on the GRID layer, the bubbles (circle + text) on the GRID_TAGS layer.
pub fn grid_dxf(grids: &[UserGrid], bubble_r: f64) -> String {
    let mut s = String::from("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n6\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n2\n");
    for (name, col) in [("GRID", 1), ("GRID_TAGS", 7)] {
        s.push_str(&format!("0\nLAYER\n2\n{name}\n70\n0\n62\n{col}\n6\nCONTINUOUS\n"));
    }
    s.push_str("0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    for g in grids {
        let (a, b) = (glam::DVec2::from(g.a), glam::DVec2::from(g.b));
        let d = (b - a).normalize_or_zero();
        // as on the drawing: the line between the ends, the bubbles in extension (tangent at the ends)
        s.push_str(&format!("0\nLINE\n8\nGRID\n10\n{}\n20\n{}\n30\n0.0\n11\n{}\n21\n{}\n31\n0.0\n", a.x, a.y, b.x, b.y));
        for c in [a - d * bubble_r, b + d * bubble_r] {
            s.push_str(&format!("0\nCIRCLE\n8\nGRID_TAGS\n10\n{}\n20\n{}\n30\n0.0\n40\n{}\n", c.x, c.y, bubble_r));
            // text centered in the bubble (72=1 center, 73=2 middle)
            s.push_str(&format!("0\nTEXT\n8\nGRID_TAGS\n10\n{x}\n20\n{y}\n30\n0.0\n40\n{h}\n1\n{t}\n72\n1\n73\n2\n11\n{x}\n21\n{y}\n31\n0.0\n", x = c.x, y = c.y, h = bubble_r * 0.9, t = g.name));
        }
    }
    s.push_str("0\nENDSEC\n0\nEOF\n");
    s
}

impl App {
    /// RO / EN interface (saved in the settings).
    pub fn toggle_language(&mut self) {
        self.settings.romanian = !self.settings.romanian;
        crate::i18n::set_romanian(self.settings.romanian);
        self.settings.save();
    }

}

/// The grid generator parameters: origin (real, m), rotation, count / spacing / first label in each direction.
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

/// Rotates the grids by `deg` around point `c` and moves them by `d` (real, m).
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

// ------------------------------------------------------------------ .bimd (a single file, like .nwd)
/// The folder where the opened .bimd files are unpacked.
pub fn published_dir() -> Option<std::path::PathBuf> {
    crate::formats::data_root().map(|d| d.join("ManageBIM").join("Published"))
}

impl App {

}

impl App {
}

/// Action started from the start page when a project is already open (after the "save?" question).
#[derive(Clone, Debug, PartialEq)]
pub enum StartAction {
    New,
    Open(String),
    OpenModelDialog,
    OpenProjectDialog,
    /// only closes the project (the start page stays)
    Close,
}

impl App {

}

// ------------------------------------------------------------------ relocation (like Navisworks: measure -> transform)
/// What gets moved: the model of the picked point or the selected elements.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum XformMode {
    /// point -> point (translation)
    Move,
    /// pivot, start direction, target direction (rotation around the vertical)
    Rotate,
    /// face of the element to move -> target face (in-plane rotation + snapping onto the face)
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
    /// true = the selected elements (translation only), false = the model of the first point
    pub selection: bool,
    /// (point, face normal, the element hit)
    pub pts: Vec<(Vec3, Vec3, ElemId)>,
}

/// The rotation around the vertical that brings horizontal direction `a` onto `b` (degrees).
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

// ------------------------------------------------------------------ reference for Revit / AutoCAD (.mbref)
/// Simple format, read by the add-ins: "MBREF1", the origin (f64 x3, real meters), the element count, then per element:
/// name, GUID, type, level (u16 length + UTF-8), RGBA color, triangle count, the triangles (f32 x9, meters from the origin).
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

    // ------------------------------------------------------------ DWG / DXF 2D overlay over the model

    // ------------------------------------------------------------ View Undo / Redo

    // ------------------------------------------------------------ display style, demo project

    /// The demo project (Block A: architecture + structure, MEP, 4D schedule): the files are in the
    /// application, are written to %LOCALAPPDATA%\ManageBIM\Demo and are opened like a regular project.
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
            // we do not rewrite the demo files (the user may be working on them)
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
    // ------------------------------------------------------------ As-built: model against the point cloud

    // ------------------------------------------------------------ Live Link: synchronized selection

}

/// %LOCALAPPDATA%\ManageBIM\Outbox (references, issues for the add-ins)
pub fn outbox_dir() -> Option<std::path::PathBuf> {
    crate::formats::data_root().map(|d| d.join("ManageBIM").join("Outbox"))
}

// ------------------------------------------------------------------ the clash matrix (disciplines / models)
/// The clashes of all tests, counted per model pair: (models, the pair (i <= j) -> [open, reviewed / approved, resolved]).
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

    // ------------------------------------------------------------ Model Checker (rules + IDS)

}

/// The model name without extension (for the matrix / reports).
pub fn model_label(name: &str) -> String {
    std::path::Path::new(name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| name.to_string())
}

/// 12345 -> "12 345"
pub fn thousands_app(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// the DXF layer name for an element type (without characters forbidden in AutoCAD)
pub fn cad_layer(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' }).collect();
    let t = t.trim().to_string();
    if t.is_empty() { "Model".into() } else { t }
}

/// DXF R12 with lines on layers (a different AutoCAD color per layer), units: meters
pub fn cut_dxf(segs: &[(String, [f64; 4])]) -> String {
    cut_dxf_layers(segs, &[])
}

/// like `cut_dxf`, with extra layers defined in the table (e.g. for texts added afterwards)
pub fn cut_dxf_layers(segs: &[(String, [f64; 4])], extra: &[&str]) -> String {
    let mut layers: Vec<&str> = segs.iter().map(|s| s.0.as_str()).chain(extra.iter().copied()).collect();
    layers.sort();
    layers.dedup();
    // LTYPE table: solid +Hidden (dashed) for the „BEHIND" layer
    let mut s = String::from("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n6\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n");
    s.push_str("0\nTABLE\n2\nLTYPE\n70\n2\n0\nLTYPE\n2\nCONTINUOUS\n70\n0\n3\nSolid line\n72\n65\n73\n0\n40\n0.0\n0\nLTYPE\n2\nHIDDEN\n70\n0\n3\nHidden line __ __\n72\n65\n73\n2\n40\n1.0\n49\n0.5\n74\n0\n49\n-0.5\n74\n0\n0\nENDTAB\n");
    s.push_str("0\nTABLE\n2\nLAYER\n");
    s.push_str(&format!("70\n{}\n", layers.len()));
    for l in layers.iter() {
        let behind = l.eq_ignore_ascii_case("BEHIND") || l.eq_ignore_ascii_case("HIDDEN");
        let ltype = if behind { "HIDDEN" } else { "CONTINUOUS" };
        // 27.11: technical drawing palette (Inventor style), not random colors:
        //   7 = white/black (context), 8 = gray, 5 = blue, 1 = red (accents).
        let lower = l.to_ascii_lowercase();
        let color = if behind {
            8
        } else {
            match lower.as_str() {
                "dims" | "dims_text" => 5, // dimensions: blue
                "sheet" | "sheet_text" => 7, // border / title block: black
                "tags" => 1, // labels: subtle red
                "hatch" | "levels" | "levels_text" | "grids" | "grid_text" => 8, // hatch, levels, grids: gray
                _ => 7, // the cut and the elements: black
            }
        };
        s.push_str(&format!("0\nLAYER\n2\n{l}\n70\n0\n62\n{color}\n6\n{ltype}\n"));
    }
    s.push_str("0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    for (l, p) in segs {
        s.push_str(&format!("0\nLINE\n8\n{l}\n10\n{:.4}\n20\n{:.4}\n30\n0.0\n11\n{:.4}\n21\n{:.4}\n31\n0.0\n", p[0], p[1], p[2], p[3]));
    }
    s.push_str("0\nENDSEC\n0\nEOF\n");
    s
}

/// The cut lines placed on a sheet (A3 / A2 / A1 / A0, millimeters) at the chosen or automatic scale
/// (the largest standard scale at which it fits), with border and the editable title block.
/// `segs` are in meters (real coordinates). Returns the DXF and the scale (1:N).
pub fn cut_sheet_dxf(segs: &[(String, [f64; 4])], tb: &crate::pdf::TitleBlock, project: &str, title: &str, date: &str) -> (String, u32) {
    cut_sheet_dxf_annot(segs, &[], tb, project, title, date)
}

/// the sheet scale: the one from the title block or the largest standard scale at which the w x h (m) drawing fits in aw x ah (mm)
fn fit_scale(tb: &crate::pdf::TitleBlock, w: f64, h: f64, aw: f64, ah: f64) -> u32 {
    if tb.scale > 0 {
        return tb.scale;
    }
    crate::pdf::SCALES.into_iter().find(|n| w * 1000.0 / *n as f64 <= aw && h * 1000.0 / *n as f64 <= ah).unwrap_or(10000)
}

/// The scale at which `cut_sheet_dxf` would put the lines on the sheet (to size the texts beforehand).
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

/// like `cut_sheet_dxf`, plus the plan texts (labels, dimensions, grids, levels) moved onto the sheet (height in paper mm).
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
    // drawing area: the sheet without margins and without the title block strip
    let (aw, ah) = (sw - 2.0 * MARGIN - 10.0, sh - 2.0 * MARGIN - TB_H - 10.0);
    let scale = fit_scale(tb, w, h, aw, ah);
    // 27.11 (Inventor style): the drawing STAYS 1:1 (real coordinates, meters) - no scaling.
    // Only a translation takes place so it is centered in the drawing area; the border and
    // the title block stay in paper mm, and the drawing texts stay 1:1.
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (ox, oy) = (MARGIN + 5.0 + aw / 2.0, MARGIN + TB_H + 5.0 + ah / 2.0);
    let mut out: CadSegs = segs.iter().map(|(l, p)| (l.clone(), [ox + p[0] - cx, oy + p[1] - cy, ox + p[2] - cx, oy + p[3] - cy])).collect();
    let mut line = |a: f64, b: f64, c: f64, d: f64| out.push(("SHEET".to_string(), [a, b, c, d]));
    let rect = |l: &mut dyn FnMut(f64, f64, f64, f64), a: f64, b: f64, c: f64, d: f64| {
        l(a, b, c, b);
        l(c, b, c, d);
        l(c, d, a, d);
        l(a, d, a, b);
    };
    rect(&mut line, 0.0, 0.0, sw, sh);
    rect(&mut line, MARGIN, MARGIN, sw - MARGIN, sh - MARGIN);
    // the title block: bottom right, 185 x 48 mm - company / project | title | drawn, checked | date, rev | scale | no
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
    let plan_txt: String = texts.iter().map(|t| CadText { x: t.x - cx + ox, y: t.y - cy + oy, ..t.clone() }.dxf(1.0)).collect();
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
    // the sheet is in millimeters
    let dxf = dxf.replacen("$INSUNITS\n70\n6\n", "$INSUNITS\n70\n4\n", 1);
    (dxf, scale)
}

/// A text of the 2D plan (label, dimension, grid, level). `mm` = the height on paper;
/// in Model space it is multiplied by `u` (meters per paper mm). `rot` in degrees.
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
    /// the TEXT entity; `k` = drawing units per paper mm
    pub fn dxf(&self, k: f64) -> String {
        let s: String = self.s.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect();
        format!("0\nTEXT\n8\n{}\n10\n{:.4}\n20\n{:.4}\n30\n0.0\n40\n{:.4}\n1\n{}\n50\n{:.2}\n", self.layer, self.x, self.y, self.mm * k, s, self.rot)
    }
}

/// DXF in Model space (meters) with the lines and the plan texts - `u` = meters per paper mm (0.1 = 1:100).
pub fn cut_dxf_annot(segs: &[(String, [f64; 4])], texts: &[CadText], u: f64) -> String {
    let extra: Vec<&str> = texts.iter().map(|t| t.layer.as_str()).collect();
    let mut dxf = cut_dxf_layers(segs, &extra);
    let t: String = texts.iter().map(|t| t.dxf(u)).collect();
    if let Some(p) = dxf.rfind("0\nENDSEC\n0\nEOF") {
        dxf.insert_str(p, &t);
    }
    dxf
}

/// ANSI31 hatch (lines at 45 degrees, step `spacing`) inside the cut outline of an element.
/// The outline comes from cutting the triangles of a closed surface, so the even / odd rule delimits the interior.
pub fn hatch_lines(contour: &[[f64; 4]], spacing: f64) -> Vec<[f64; 4]> {
    let mut out = Vec::new();
    if contour.len() < 3 || spacing <= 0.0 {
        return out;
    }
    let r = std::f64::consts::FRAC_1_SQRT_2;
    // u along the hatch, v perpendicular
    let uv = |x: f64, y: f64| ((x + y) * r, (y - x) * r);
    let xy = |u: f64, v: f64| ((u - v) * r, (u + v) * r);
    let e: Vec<((f64, f64), (f64, f64))> = contour.iter().map(|s| (uv(s[0], s[1]), uv(s[2], s[3]))).collect();
    let (vmin, vmax) = e.iter().fold((f64::MAX, f64::MIN), |(a, b), (p, q)| (a.min(p.1).min(q.1), b.max(p.1).max(q.1)));
    if (vmax - vmin) / spacing > 4000.0 {
        return out;
    }
    // the lines on a global grid: the hatches of neighboring elements continue
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
        for w in xs.as_chunks::<2>().0 {
            if w[1] - w[0] > 1e-6 {
                let (a, b) = (xy(w[0], v), xy(w[1], v));
                out.push([a.0, a.1, b.0, b.1]);
            }
        }
        v += spacing;
    }
    out
}

/// The elements hatched in plan / section (structure and walls).
pub fn cut_is_hatched(ifc_type: &str) -> bool {
    let t = ifc_type.to_lowercase();
    ["wall", "column", "slab", "beam", "footing", "pile", "stair", "ramp", "roof"].iter().any(|k| t.contains(k)) && !t.contains("opening")
}

/// The label of a cut element: doors / windows -> the number (Mark / Number / Tag or the last part of the name),
/// walls -> the type. None for the rest.
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

fn circle_segs(layer: &str, c: [f64; 2], r: f64, out: &mut CadSegs) {
    const N: usize = 20;
    for i in 0..N {
        let (a, b) = (i as f64 / N as f64 * std::f64::consts::TAU, (i + 1) as f64 / N as f64 * std::f64::consts::TAU);
        out.push((layer.into(), [c[0] + r * a.cos(), c[1] + r * a.sin(), c[0] + r * b.cos(), c[1] + r * b.sin()]));
    }
}

/// the angle of a text readable from the bottom / from the right: (-90, 90]
pub fn read_angle(dx: f64, dy: f64) -> f64 {
    let mut a = dy.atan2(dx).to_degrees();
    if a > 90.0 + 1e-6 {
        a -= 180.0;
    } else if a <= -90.0 + 1e-6 {
        a += 180.0;
    }
    a
}

/// the approximate width of a text (meters), `u` = meters per paper mm
fn text_w(s: &str, mm: f64, u: f64) -> f64 {
    s.chars().count() as f64 * mm * u * 0.8
}

/// The grids in plan: the grid line, the bubble with the label at one end and dimension chains between the parallel grids
/// (plus the overall dimension) at the other end. Coordinates in meters.
pub fn plan_grid_annotations(grids: &[UserGrid], u: f64, segs: &mut CadSegs, texts: &mut Vec<CadText>) {
    let r_b = 4.5 * u;
    type DimItem<'a> = ([f64; 2], [f64; 2], [f64; 2], &'a str);
    let mut items: Vec<DimItem<'_>> = Vec::new();
    for g in grids {
        let (dx, dy) = (g.b[0] - g.a[0], g.b[1] - g.a[1]);
        let l = (dx * dx + dy * dy).sqrt();
        if l < 1e-6 {
            continue;
        }
        items.push((g.a, g.b, [dx / l, dy / l], g.name.as_str()));
    }
    // groups of parallel grids
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
        // each grid: (offset along the normal, min t, max t, label) in the system of the first grid in the group
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
        // the dimensions: at the end opposite to the bubbles
        let tmax = rows.iter().map(|r| r.2).fold(f64::MIN, f64::max);
        let rot = read_angle(n[0], n[1]);
        let (rc, rs) = (rot.to_radians().cos(), rot.to_radians().sin());
        let chain = |t: f64, idx: &[usize], segs: &mut CadSegs, texts: &mut Vec<CadText>| {
            let (a, b) = (p(rows[idx[0]].0, t), p(rows[idx[idx.len() - 1]].0, t));
            segs.push(("DIMS".into(), [a[0], a[1], b[0], b[1]]));
            for &k in idx {
                let o = rows[k].0;
                // extension line from the grid end and the 45 degree tick
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
                // above the dimension line, in the reading direction of the text
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

/// On a vertical section: the level lines with name and elevation (m) and the grids cut by the plane (with the bubble on top).
/// `axis` = 0 (plane at X = value, horizontal = Y) or 1 (plane at Y = value, horizontal = X);
/// `bbox` = [x0, y0, x1, y1] of the cut in the drawing coordinates.
#[allow(clippy::too_many_arguments)]
pub fn section_annotations(levels: &[(String, f64)], grids: &[UserGrid], axis: usize, value: f64, bbox: [f64; 4], u: f64, dims: bool, levels_on: bool, segs: &mut CadSegs, texts: &mut Vec<CadText>) {
    let [x0, y0, x1, y1] = bbox;
    if levels_on { for (name, z) in levels {
        if *z < y0 - 5.0 || *z > y1 + 5.0 {
            continue;
        }
        segs.push(("LEVELS".into(), [x0 - 10.0 * u, *z, x1 + 10.0 * u, *z]));
        // the elevation marker (triangle with its tip on the line)
        let tx = x1 + 10.0 * u;
        segs.push(("LEVELS".into(), [tx, *z, tx + 2.0 * u, *z + 2.5 * u]));
        segs.push(("LEVELS".into(), [tx + 2.0 * u, *z + 2.5 * u, tx - 2.0 * u, *z + 2.5 * u]));
        segs.push(("LEVELS".into(), [tx - 2.0 * u, *z + 2.5 * u, tx, *z]));
        texts.push(CadText::new("LEVELS_TEXT", tx + 3.5 * u, *z + 0.5 * u, 2.5, 0.0, format!("{name}  {}{:.3}", if *z >= 0.0 { "+" } else { "" }, z)));
    } }
    let mut grid_x: Vec<f64> = Vec::new();
    if levels_on { for g in grids {
        // the intersection of the grid (2D) with the section plane
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
        grid_x.push(h);
        segs.push(("GRIDS".into(), [h, y0 - 5.0 * u, h, y1 + 10.0 * u]));
        circle_segs("GRIDS", [h, y1 + 14.5 * u], 4.5 * u, segs);
        texts.push(CadText::new("GRID_TEXT", h - text_w(&g.name, 2.5, u) / 2.0, y1 + 13.25 * u, 2.5, 0.0, g.name.clone()));
    } }
    // ---- CHAIN DIMENSIONS (technical drawing): dimension line + extension lines + 45° tick + text.
    // Vertical: between consecutive levels; Horizontal: between the grid lines cut by the plane.
    if !dims { return; }
    let tick = |x: f64, y: f64, segs: &mut CadSegs| {
        segs.push(("DIMS".into(), [x - 1.2 * u, y - 1.2 * u, x + 1.2 * u, y + 1.2 * u]));
    };
    // vertical (left of the drawing)
    let mut zs: Vec<f64> = levels.iter().map(|(_, z)| *z).filter(|z| *z > y0 - 5.0 && *z < y1 + 5.0).collect();
    zs.sort_by(|a, b| a.total_cmp(b));
    zs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    if zs.len() >= 2 {
        let dx = x0 - 14.0 * u;
        segs.push(("DIMS".into(), [dx, zs[0], dx, zs[zs.len() - 1]]));
        for &z in &zs {
            segs.push(("DIMS".into(), [dx - 2.0 * u, z, x0 - 1.0 * u, z]));
            tick(dx, z, segs);
        }
        for w in zs.windows(2) {
            let d = (w[1] - w[0]).abs();
            if d < 0.01 { continue; }
            let label = format!("{:.2}", d);
            texts.push(CadText::new("DIMS_TEXT", dx - 1.5 * u, (w[0] + w[1]) * 0.5, 2.5, 90.0, label));
        }
    }
    // horizontal (below the drawing)
    grid_x.sort_by(|a, b| a.total_cmp(b));
    grid_x.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
    if grid_x.len() >= 2 {
        let dy = y0 - 16.0 * u;
        segs.push(("DIMS".into(), [grid_x[0], dy, grid_x[grid_x.len() - 1], dy]));
        for &x in &grid_x {
            segs.push(("DIMS".into(), [x, dy - 2.0 * u, x, y0 - 2.0 * u]));
            tick(x, dy, segs);
        }
        for w in grid_x.windows(2) {
            let d = (w[1] - w[0]).abs();
            if d < 0.01 { continue; }
            let label = format!("{:.2}", d);
            let half = text_w(&label, 2.5, u) / 2.0;
            texts.push(CadText::new("DIMS_TEXT", (w[0] + w[1]) * 0.5 - half, dy - 4.0 * u, 2.5, 0.0, label));
        }
    }
}

/// A view template saved in the project (Mitcad / Revit View Graphics style).
#[derive(Clone, Serialize, Deserialize)]
pub struct ViewTemplate {
    pub name: String,
    pub hidden: Vec<ElemId>,
    pub section: Section,
    /// the 3D position (camera) before saving - so you return to exactly the same view
    pub cam: Camera,
    pub visual_style: crate::render::VisualStyle,
    pub show_grids: bool,
    pub show_levels: bool,
    pub section_caps: bool,
    /// the colors/transparencies assigned to the elements
    #[serde(default)]
    pub overrides: Vec<(ElemId, [u8; 4])>,
    /// the elements selected at that moment
    #[serde(default)]
    pub selected: Vec<ElemId>,
    /// the measurements in the viewport
    #[serde(default)]
    pub measures: Vec<Measure>,
    /// the active viewpoint filter
    #[serde(default)]
    pub vp_filter: Option<String>,
    /// the template applied automatically when the project opens
    #[serde(default)]
    pub is_default: bool,
    /// short note (what you see in this template)
    #[serde(default)]
    pub note: String,
    /// what the template applies (like „Include" in Revit View Templates): camera, visibility + selection,
    /// section, visual style (+ grids / levels / caps), colors; everything by default
    #[serde(default = "vt_include_all")]
    pub include: [bool; 5],
}

fn vt_include_all() -> [bool; 5] {
    [true; 5]
}

/// the labels of the `ViewTemplate::include` flags
pub const VT_INCLUDE: [&str; 5] = ["Camera", "Visibility", "Section", "Style", "Colours"];

/// A view state in the Undo / Redo history.
#[derive(Clone)]
pub struct ViewSnap {
    pub selected: BTreeSet<ElemId>,
    pub hidden: Arc<HashSet<ElemId>>,
    pub section: Section,
    pub cam: Camera,
    /// the model data changed by the user (properties, colors, materials) - Undo restores them too;
    /// shared (Arc) with the previous snapshot when they did not change
    pub model: Arc<ModelSnap>,
}

/// The user changes to the model, in the Undo / Redo history.
#[derive(Clone, Default)]
pub struct ModelSnap {
    pub sig: u64,
    pub user_params: Vec<UserParam>,
    pub overrides: HashMap<ElemId, [u8; 4]>,
    pub materials: Vec<crate::materials::Material>,
    pub mat_assign: Vec<MatAssign>,
}

/// A 2D drawing (DWG / DXF) placed over the model at an elevation, with offset and rotation.
#[derive(Clone, Serialize, Deserialize)]
pub struct CadOverlay {
    pub name: String,
    pub path: String,
    /// elevation (m, local scene coordinates)
    pub z: f32,
    /// offset (m) and rotation (degrees, around the scene origin)
    #[serde(default)]
    pub dx: f32,
    #[serde(default)]
    pub dy: f32,
    #[serde(default)]
    pub rot_deg: f32,
    /// None = horizontal plane at Z; Some(0) = vertical YZ plane at Y=z (Front/Back);
    /// Some(1) = vertical XZ plane at X=z (Left/Right)
    #[serde(default)]
    pub vert: Option<u8>,
    /// the drawing was exported mirrored (Back / Right); on the way back it is mirrored again
    #[serde(default)]
    pub mirror: bool,
    #[serde(default = "yes_bool")]
    pub visible: bool,
    #[serde(default = "overlay_color")]
    pub color: [u8; 3],
    #[serde(default = "overlay_opacity")]
    pub opacity: f32,
    /// the lines (x1, y1, x2, y2) in scene coordinates; reread from the file when the project opens
    #[serde(skip)]
    pub segs: Arc<Vec<[f32; 4]>>,
}

/// default value `true` for the settings fields added later (serde)
pub fn yes_default() -> bool {
    true
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

// ====================================================================== parameters added by the user

/// A parameter added in ManageBIM to an element (e.g. elements without data from DWG / OBJ / STEP),
/// saved in the project and applied on every load: model (the file name) + GUID (or #index).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserParam {
    pub model: String,
    pub key: String,
    pub name: String,
    pub value: String,
    /// the user data tab (Navisworks: User Data Tab); old projects: "User"
    #[serde(default = "user_group_default")]
    pub group: String,
}

fn user_group_default() -> String {
    USER_GROUP.to_string()
}

/// the default group of the properties added by the user (old projects)
pub const USER_GROUP: &str = "User";

/// Property import from Excel / CSV: the file read and the choices in the import window.
#[derive(Clone, Debug, Default)]
pub struct PropImport {
    pub path: String,
    pub rows: Vec<Vec<String>>,
    /// the header row
    pub header: usize,
    /// the column with the element key
    pub key_col: usize,
    /// how the key is matched: "GlobalId", "Name" or the name of a property
    pub match_by: String,
    /// the tab the properties are written to
    pub tab: String,
    /// only the selected elements
    pub only_selected: bool,
}

/// the key of an element: the GUID or "#index" when it has no GUID
pub fn user_key(e: &Element, idx: usize) -> String {
    if e.guid.trim().is_empty() { format!("#{idx}") } else { e.guid.clone() }
}

/// What the user wrote over the model: (model, element key, tab, lowercase property).
pub type UserPropKey = (String, String, String, String);

/// The state of the user properties: which values are theirs on each element and the
/// ORIGINAL values they replaced (so they come back if the override is deleted - nothing is lost).
#[derive(Default)]
pub struct UserShadow {
    pub applied: HashSet<UserPropKey>,
    /// the original value (exact name + value) before the first override
    pub orig: HashMap<UserPropKey, (String, String)>,
}

/// Writes the user parameters into the model properties (the user tabs are rebuilt from scratch).
pub fn apply_user_params(m: &mut Model, params: &[UserParam]) {
    let mut sh = UserShadow::default();
    let prev = HashSet::new();
    apply_user_params_shadow(m, params, &prev, &mut sh);
}

/// Applies the user parameters to a model, element by element:
/// 1. removes the values previously set by the user (`prev` = (key, tab, property) of the model) and
///    the "User" tab, putting back the original values they had replaced;
/// 2. writes the current parameters; if the element already had the property (from IFC / Revit), the original value
///    is kept in `sh.orig` and comes back when the override is deleted.
///
/// Only the elements with parameters are touched - the properties of the others stay unchanged.
pub fn apply_user_params_shadow(m: &mut Model, params: &[UserParam], prev: &HashSet<(String, String, String)>, sh: &mut UserShadow) {
    let model = m.name.clone();
    let mine: Vec<&UserParam> = params.iter().filter(|p| p.model == model).collect();
    let had_user_tab = m.elements.iter().any(|e| e.props.iter().any(|p| p.group == USER_GROUP));
    if mine.is_empty() && prev.is_empty() && !had_user_tab {
        return;
    }
    let mut by_key: HashMap<&str, Vec<&UserParam>> = HashMap::new();
    for p in &mine {
        by_key.entry(p.key.as_str()).or_default().push(p);
    }
    let prev_keys: HashSet<&str> = prev.iter().map(|k| k.0.as_str()).collect();
    for (i, e) in m.elements.iter_mut().enumerate() {
        let k = user_key(e, i);
        let here = by_key.get(k.as_str());
        if here.is_none() && !prev_keys.contains(k.as_str()) && !e.props.iter().any(|p| p.group == USER_GROUP) {
            continue;
        }
        // 1. the old user values out, the originals back
        let mut restored: Vec<crate::model::Prop> = Vec::new();
        e.props.retain(|p| {
            let nl = p.name.to_lowercase();
            let mine_before = prev.contains(&(k.clone(), p.group.clone(), nl.clone()));
            if p.group == USER_GROUP || mine_before {
                if mine_before {
                    if let Some((n, v)) = sh.orig.get(&(model.clone(), k.clone(), p.group.clone(), nl)) {
                        if !restored.iter().any(|r| r.group == p.group && r.name.eq_ignore_ascii_case(n)) {
                            restored.push(crate::model::Prop { group: p.group.clone(), name: n.clone(), value: v.clone() });
                        }
                    }
                }
                false
            } else {
                true
            }
        });
        e.props.extend(restored);
        // 2. the current values
        if let Some(v) = here {
            for p in v {
                let nl = p.name.to_lowercase();
                let key = (model.clone(), k.clone(), p.group.clone(), nl.clone());
                if let Some(pos) = e.props.iter().position(|q| q.group == p.group && q.name.to_lowercase() == nl) {
                    let q = e.props.remove(pos);
                    sh.orig.entry(key.clone()).or_insert((q.name, q.value));
                    e.props.retain(|q| !(q.group == p.group && q.name.to_lowercase() == nl));
                }
                e.props.push(crate::model::Prop { group: p.group.clone(), name: p.name.clone(), value: p.value.clone() });
                sh.applied.insert(key);
            }
        }
    }
}

/// the displayed name of a property
pub fn prop_display_name(n: &str) -> &str {
    n
}

impl App {

    // ------------------------------------------------------------------ property import / export

}

/// the normalized key for matching (no leading/trailing spaces, lowercase GUIDs)
fn norm_key(s: &str) -> String {
    let t = s.trim();
    if t.len() == 36 && t.matches('-').count() == 4 { t.to_ascii_lowercase() } else { t.to_string() }
}
