pub mod acis;
pub mod addin_install;
pub mod anim;
pub mod asbuilt;
pub mod bimc;
pub mod check;
pub mod clash;
pub mod clash_smart;
pub mod compare;
pub mod e57;
pub mod egress;
pub mod fbx;
pub mod fcstd;
pub mod formats;
pub mod i18n;
pub mod keybind;
pub mod script_builder;
pub mod script_guide;
pub mod materials;
pub mod lights;
pub mod ifc_mvd;
pub mod ifc_psets;
pub mod ifc_map;
pub mod classification;
pub mod bsdd;
pub mod hlr;
pub mod update;
pub mod coord_objects;
pub mod cost;
pub mod pro_materials;
pub mod hdri;
pub mod materialx;
pub mod dashboard;
pub mod smart_sets;
pub mod mcp;
pub mod pdf;
pub mod markup;
pub mod gltf_export;
pub mod gltf_load;
pub mod gltf_meta;
pub mod open_formats;
pub mod usda;
pub mod ifc_schema;
pub mod ifc_convert;
pub mod ifc;
pub mod ifc_export;
pub mod model;
pub mod occt;
pub mod oidn;
pub mod pathtrace;
pub mod gpu_pt;
pub mod softgl;
pub mod plant3d;
pub mod plant3d_db;
pub mod profiler;
pub mod qto;
pub mod shared;
pub mod scripting;
pub mod step;
pub mod sync;
pub mod tables;
pub mod timeline;
pub mod web;

use model::{Model, RawModel};

/// Loads a model by file extension.
pub fn load_any(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    let ext = formats::ext_of(path);
    let r = load_by_ext(path, &ext, progress);
    // Plant 3D gratings are saved in DWG only as a flat rectangle (the bars are generated on display):
    // we give them thickness, so they are visible from any angle - for all sources (native DWG, .bimc add-in, NWC)
    r.map(|mut m| {
        // If the file is a cut exported from ManageBIM, we put the 2D drawing back
        // in the exact 3D plane it was extracted from (the metadata is in the DXF / sidecar).
        if matches!(ext.as_str(), "dxf" | "dwg") {
            if let Some((axis, value, mirror, origin)) = formats::read_cut_plane_meta(path) {
                m.place_on_cut_plane(axis, value, mirror, origin);
                m.info.insert(0, ("Cut plane".into(), format!("Restored on axis {axis} at {value:.3} m")));
            }
        }
        // drawing from a Plant 3D project: the objects on layer "0" get the area (e.g. "Area_2") as level,
        // as in the AutoCAD add-in (Plant 3D has no levels)
        if ext == "dwg" {
            if let Some(area) = plant3d::project_area(path) {
                for e in &mut m.elements {
                    if e.storey.is_empty() || e.storey == "0" {
                        e.storey = area.clone();
                    }
                }
            }
        }
        m.thicken_flat_gratings(0.03);
        m
    })
}

fn load_by_ext(path: &str, ext: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    match ext {
        "bimc" => bimc::read(path).map(|mut m| {
            // sent from AutoCAD / Plant 3D: the complete properties from the project database (Piping.dcf)
            match plant3d_db::attach_sent(&mut m) {
                Ok(Some((db, n))) => m.info.push(("Plant 3D project".into(), format!("{}: properties completed for {n} objects", db.display()))),
                Ok(None) => {}
                Err(e) => m.warnings.push(format!("Plant 3D project database could not be read ({e}).")),
            }
            m
        }),
        "ifc" => ifc::load_ifc(path, progress),
        "gltf" | "glb" => gltf_load::load_gltf(path, progress),
        "fbx" => fbx::load_fbx(path, progress),
        "obj" => formats::load_obj(path, progress),
        "stl" => formats::load_stl(path, progress),
        "ply" => formats::load_ply(path, progress),
        "dxf" => formats::load_dxf(path, progress),
        "dwg" => formats::convert_dwg(path, progress),
        "step" | "stp" | "iges" | "igs" | "brep" | "brp" => formats::load_cad(path, progress),
        "fcstd" => fcstd::load_fcstd(path, progress),
        "3mf" => open_formats::load_3mf(path, progress),
        "ifcxml" => ifc_convert::load_ifcxml(path, progress),
        "json" | "jsonl" | "cjson" | "cityjson" | "cjseq" => open_formats::load_json(path, progress),
        "b3dm" | "cmpt" => open_formats::load_tile_file(path, progress),
        "dgn" => Err("DGN (MicroStation) is a closed Bentley format. In MicroStation / OpenRoads use File > Export > IFC (or DWG) and open that file.".into()),
        e if formats::NAVIS_EXT.contains(&e) => formats::convert_navisworks(path, progress),
        "rfa" => Err("RFA is a Revit family. Load it into a project and export IFC / .bimc (ManageBIM add-in for Revit).".into()),
        "blend" => Err("Blender files (.blend) are not read directly. In Blender use File > Export > glTF 2.0 (.glb) and tick Include > Custom Properties: object names, the collection hierarchy, colours and your Custom Properties open in ManageBIM. FBX works too (names and colours, no properties). With Bonsai (BlenderBIM) you can also save IFC.".into()),
        "3dm" => Err("Rhino files (.3dm) are not read directly. In Rhino use File > Export: STEP (.stp - names, assemblies and colours, millimetres converted) or glTF (.glb - names and colours) or FBX. For IFC use VisualARQ / Rhino.Inside.Revit.".into()),
        "usda" | "usd" | "usdc" | "usdz" => usda::load_usda(path, progress),
        "dae" | "abc" | "3ds" | "max" => Err("Export this model to glTF / GLB (best: keeps materials and object names) or FBX and open that file.".into()),
        "skp" => Err("SketchUp: export to IFC (best: keeps the BIM data), FBX or OBJ and open that file.".into()),
        "ifczip" => load_ifczip(path, progress),
        "e57" | "las" | "laz" => Err("Point clouds (E57 / LAS / LAZ) open directly in ManageBIM (they are not converted to .bimc).".into()),

        _ => Err(format!("Unknown format .{ext}. Supported: {}", SUPPORTED.join(", "))),
    }
}

/// .ifczip = a ZIP archive with an .ifc (usually a single one, the largest): we unpack it temporarily and read it.
fn load_ifczip(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    progress(0.01, "Unzipping IFC");
    let zip = fcstd::Zip::parse(std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", file_name(path)))?).map_err(|_| format!("{} is not a valid .ifczip archive.", file_name(path)))?;
    let name = zip.names().filter(|n| n.to_lowercase().ends_with(".ifc")).max_by_key(|n| zip.read(n).map(|b| b.len()).unwrap_or(0)).cloned().ok_or_else(|| format!("{}: no .ifc file inside the archive.", file_name(path)))?;
    let data = zip.read(&name).ok_or("Cannot unzip the IFC file (unsupported compression).")?;
    let dir = std::env::temp_dir().join(format!("bimrust_ifczip_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(file_name(&name.replace('/', "\\")));
    std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
    let r = ifc::load_ifc(&tmp.to_string_lossy(), progress);
    let _ = std::fs::remove_dir_all(&dir);
    r.map(|mut m| {
        m.info.insert(0, ("Archive".into(), format!("{} > {}", file_name(path), name)));
        m
    })
}

/// Every extension ManageBIM can open (natively or through a converter).
/// The models are the first `SUPPORTED_MODELS` entries (the filter in the Open dialog); .bimr / .csv at the end.
pub const SUPPORTED: [&str; 36] = ["ifc", "ifczip", "ifcxml", "bimc", "glb", "gltf", "fbx", "obj", "stl", "ply", "3mf", "dxf", "dwg", "fcstd", "step", "stp", "iges", "igs", "brep", "brp", "nwd", "nwc", "nwf", "rvt", "e57", "las", "laz", "json", "jsonl", "usda", "usd", "b3dm", "cmpt", "bimd", "bimr", "csv"];
pub const SUPPORTED_MODELS: usize = 33;

pub fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}

pub fn build_model(raw: RawModel, path: &str, origin: glam::DVec3) -> Model {
    Model::from_raw(raw, file_name(path), path.to_string(), origin)
}

pub mod app;
pub mod clash_io;
pub mod cobie;
pub mod camera;
pub mod layout;
pub mod render;
pub mod theme;
pub mod video;
mod ui;
