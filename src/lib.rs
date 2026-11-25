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
pub mod ifc_map;
pub mod classification;
pub mod bsdd;
pub mod hlr;
pub mod update;
pub mod coord_objects;
pub mod pro_materials;
pub mod hdri;
pub mod materialx;
pub mod pdf;
pub mod markup;
pub mod gltf_export;
pub mod gltf_load;
pub mod ifc;
pub mod ifc_export;
pub mod model;
pub mod occt;
pub mod oidn;
pub mod pathtrace;
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
    // gratarele Plant 3D sunt salvate in DWG doar ca un dreptunghi plat (barele se genereaza la afisare):
    // le dam grosime, ca sa se vada din orice unghi - pentru toate sursele (DWG nativ, add-in .bimc, NWC)
    r.map(|mut m| {
        // Daca fisierul e o taietura exportata din ManageBIM, repunem desenul 2D
        // in planul 3D exact din care a fost extras (metadatele sint in DXF / sidecar).
        if matches!(ext.as_str(), "dxf" | "dwg") {
            if let Some((axis, value, mirror, origin)) = formats::read_cut_plane_meta(path) {
                m.place_on_cut_plane(axis, value, mirror, origin);
                m.info.insert(0, ("Cut plane".into(), format!("Restored on axis {axis} at {value:.3} m")));
            }
        }
        m.thicken_flat_gratings(0.03);
        m
    })
}

fn load_by_ext(path: &str, ext: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    match ext {
        "bimc" => bimc::read(path).map(|mut m| {
            // trimis din AutoCAD / Plant 3D: proprietatile complete din baza proiectului (Piping.dcf)
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
        "dgn" => Err("DGN (MicroStation) is a closed Bentley format. In MicroStation / OpenRoads use File > Export > IFC (or DWG) and open that file.".into()),
        e if formats::NAVIS_EXT.contains(&e) => formats::convert_navisworks(path, progress),
        "rfa" => Err("RFA is a Revit family. Load it into a project and export IFC / .bimc (ManageBIM add-in for Revit).".into()),
        "blend" => Err("Blender files (.blend) are not read directly. In Blender use File > Export > glTF 2.0 (.glb) and tick Include > Custom Properties: object names, the collection hierarchy, colours and your Custom Properties open in ManageBIM. FBX works too (names and colours, no properties). With Bonsai (BlenderBIM) you can also save IFC.".into()),
        "3dm" => Err("Rhino files (.3dm) are not read directly. In Rhino use File > Export: STEP (.stp - names, assemblies and colours, millimetres converted) or glTF (.glb - names and colours) or FBX. For IFC use VisualARQ / Rhino.Inside.Revit.".into()),
        "dae" | "usd" | "usda" | "usdc" | "usdz" | "abc" | "3ds" | "max" => Err("Export this model to glTF / GLB (best: keeps materials and object names) or FBX and open that file.".into()),
        "skp" => Err("SketchUp: export to IFC (best: keeps the BIM data), FBX or OBJ and open that file.".into()),
        "ifczip" => load_ifczip(path, progress),
        "e57" | "las" => Err("Point clouds (E57 / LAS) open directly in ManageBIM (they are not converted to .bimc).".into()),
        "laz" => Err("LAZ is compressed LAS: convert it to .las (e.g. with CloudCompare: File > Save As > LAS) and open that.".into()),
        "ifcxml" => Err("Convert the .ifcxml file to .ifc (STEP) before opening it (e.g. with the authoring tool's IFC export).".into()),
        _ => Err(format!("Unknown format .{ext}. Supported: {}", SUPPORTED.join(", "))),
    }
}

/// .ifczip = arhiva ZIP cu un .ifc (de obicei unul singur, cel mai mare): il despachetam temporar si il citim.
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
/// Modelele sunt primele `SUPPORTED_MODELS` intrari (filtrul din dialogul Open); .bimr / .csv la final.
pub const SUPPORTED: [&str; 27] = ["ifc", "ifczip", "bimc", "glb", "gltf", "fbx", "obj", "stl", "ply", "dxf", "dwg", "fcstd", "step", "stp", "iges", "igs", "brep", "brp", "nwd", "nwc", "nwf", "rvt", "e57", "las", "laz", "bimr", "csv"];
pub const SUPPORTED_MODELS: usize = 25;

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
