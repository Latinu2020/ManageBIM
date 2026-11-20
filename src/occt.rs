//! Cititoare CAD native prin OpenCascade (OCCT, LGPL-2.1 + exceptie): STEP, IGES, BREP.
//!
//! OCCT e legat DINAMIC: `mbocct.dll` (shim C peste OCCT, `src/occt/mbocct.cpp`) si DLL-urile OCCT
//! stau in subfolderul `occt\` de langa ManageBIM.exe si se incarca abia cand deschizi un astfel de
//! fisier. Fara ele ManageBIM merge normal, doar STEP / IGES / BREP / FCStd nu se pot deschide.
//!
//! Fiecare piesa frunza din ansamblu devine un element ManageBIM: nume (instanta / piesa), calea
//! de ansamblu ca proprietate, culori pe fata (XCAF), coordonate in metri.
#[cfg(windows)]
use crate::model::{Prop, RawElement};
use crate::model::RawModel;
use glam::DVec3;
use std::path::PathBuf;

/// Formatul citit de OCCT
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CadKind {
    Step = 0,
    Iges = 1,
    Brep = 2,
}

impl CadKind {
    pub fn from_ext(ext: &str) -> Option<CadKind> {
        match ext {
            "step" | "stp" | "p21" => Some(CadKind::Step),
            "iges" | "igs" => Some(CadKind::Iges),
            "brep" | "brp" => Some(CadKind::Brep),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            CadKind::Step => "STEP",
            CadKind::Iges => "IGES",
            CadKind::Brep => "BREP",
        }
    }
}

/// Calitatea triangularii: deflexia liniara relativa la marimea piesei si cea unghiulara (radiani).
#[derive(Clone, Copy, Debug)]
pub struct MeshQuality {
    pub rel_deflection: f64,
    pub ang_deflection: f64,
}

impl Default for MeshQuality {
    fn default() -> Self {
        MeshQuality { rel_deflection: 0.002, ang_deflection: 0.35 }
    }
}

/// ManageBIM a fost compilat cu OCCT (build.rs a gasit third_party\occt si a facut mbocct.dll)?
pub const BUILT_WITH_OCCT: bool = option_env!("MBOCCT_BUILD_DIR").is_some();

/// Folderele in care se cauta `mbocct.dll`, in ordine.
pub fn search_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(d) = std::env::var_os("MANAGEBIM_OCCT_DIR") {
        v.push(PathBuf::from(d));
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        v.push(dir.join("occt"));
        // testele / exemplele ruleaza din target\<profil>\deps sau \examples
        if let Some(up) = dir.parent() {
            v.push(up.join("occt"));
        }
    }
    if let Some(d) = option_env!("MBOCCT_BUILD_DIR") {
        v.push(PathBuf::from(d));
    }
    v
}

#[cfg(windows)]
mod ffi {
    use std::ffi::{c_char, c_int, c_void, CStr};
    use std::sync::OnceLock;

    pub type ProgressFn = extern "C" fn(*mut c_void, f64, *const c_char);
    pub enum MboModel {}

    #[allow(clippy::type_complexity)]
    pub struct Api {
        pub dir: String,
        pub version: c_int,
        pub load: unsafe extern "C" fn(*const c_char, c_int, f64, f64, Option<ProgressFn>, *mut c_void) -> *mut MboModel,
        pub error: unsafe extern "C" fn(*const MboModel) -> *const c_char,
        pub info_count: unsafe extern "C" fn(*const MboModel) -> c_int,
        pub info: unsafe extern "C" fn(*const MboModel, c_int, c_int) -> *const c_char,
        pub warning_count: unsafe extern "C" fn(*const MboModel) -> c_int,
        pub warning: unsafe extern "C" fn(*const MboModel, c_int) -> *const c_char,
        pub part_count: unsafe extern "C" fn(*const MboModel) -> c_int,
        pub part_str: unsafe extern "C" fn(*const MboModel, c_int, c_int) -> *const c_char,
        pub part_int: unsafe extern "C" fn(*const MboModel, c_int, c_int) -> c_int,
        pub part_tri_count: unsafe extern "C" fn(*const MboModel, c_int) -> c_int,
        pub part_xyz: unsafe extern "C" fn(*const MboModel, c_int) -> *const f64,
        pub part_rgba: unsafe extern "C" fn(*const MboModel, c_int) -> *const u8,
        pub free: unsafe extern "C" fn(*mut MboModel),
        /// traducatorul ACIS (mbocct.dll mai vechi nu il are)
        pub brep_from_desc: Option<unsafe extern "C" fn(*const i32, c_int, *const f64, c_int, f64, f64) -> *mut MboModel>,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryExW(name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
        fn GetLastError() -> u32;
    }
    const LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR: u32 = 0x0000_0100;
    const LOAD_LIBRARY_SEARCH_DEFAULT_DIRS: u32 = 0x0000_1000;

    static API: OnceLock<Result<Api, String>> = OnceLock::new();

    pub fn api() -> Result<&'static Api, String> {
        API.get_or_init(load_api).as_ref().map_err(|e| e.clone())
    }

    fn load_api() -> Result<Api, String> {
        let dirs = super::search_dirs();
        let Some(dll) = dirs.iter().map(|d| d.join("mbocct.dll")).find(|p| p.is_file()) else {
            return Err("OpenCascade runtime (occt\\mbocct.dll) not found next to ManageBIM.exe.".into());
        };
        let dll = std::fs::canonicalize(&dll).unwrap_or(dll);
        // \\?\ din canonicalize nu deranjeaza LoadLibraryExW, dar il scoatem pentru mesaje
        let shown = dll.to_string_lossy().trim_start_matches(r"\\?\").to_string();
        let wide: Vec<u16> = shown.encode_utf16().chain(std::iter::once(0)).collect();
        // dependintele (TK*.dll, msvcp140.dll) se cauta intai in folderul lui mbocct.dll
        let h = unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS) };
        if h.is_null() {
            let code = unsafe { GetLastError() };
            let why = match code {
                126 => "a dependent OpenCascade DLL is missing".to_string(),
                193 => "wrong architecture (needs 64-bit)".to_string(),
                1260 | 4551 | 4556 => "blocked by Windows (Smart App Control / policy)".to_string(),
                c => format!("Windows error {c}"),
            };
            return Err(format!("Cannot load {shown}: {why}."));
        }
        macro_rules! sym {
            ($n:literal) => {{
                let p = unsafe { GetProcAddress(h, concat!($n, "\0").as_ptr() as *const c_char) };
                if p.is_null() {
                    return Err(format!("{shown} is outdated (missing {}).", $n));
                }
                unsafe { std::mem::transmute::<*mut c_void, _>(p) }
            }};
        }
        let version: unsafe extern "C" fn() -> c_int = sym!("mbo_version");
        let dir = dll.parent().map(|p| p.to_string_lossy().trim_start_matches(r"\\?\").to_string()).unwrap_or_default();
        Ok(Api {
            dir,
            version: unsafe { version() },
            load: sym!("mbo_load"),
            error: sym!("mbo_error"),
            info_count: sym!("mbo_info_count"),
            info: sym!("mbo_info"),
            warning_count: sym!("mbo_warning_count"),
            warning: sym!("mbo_warning"),
            part_count: sym!("mbo_part_count"),
            part_str: sym!("mbo_part_str"),
            part_int: sym!("mbo_part_int"),
            part_tri_count: sym!("mbo_part_tri_count"),
            part_xyz: sym!("mbo_part_xyz"),
            part_rgba: sym!("mbo_part_rgba"),
            free: sym!("mbo_free"),
            brep_from_desc: {
                let p = unsafe { GetProcAddress(h, c"mbo_brep_from_desc".as_ptr()) };
                (!p.is_null()).then(|| unsafe { std::mem::transmute::<*mut c_void, _>(p) })
            },
        })
    }

    pub fn s(p: *const c_char) -> String {
        if p.is_null() {
            return String::new();
        }
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

/// OCCT e disponibil (DLL-urile gasite si incarcate)?
pub fn available() -> bool {
    status().is_ok()
}

/// Versiunea OCCT si folderul DLL-urilor, sau motivul pentru care lipseste.
pub fn status() -> Result<String, String> {
    #[cfg(windows)]
    {
        let a = ffi::api()?;
        let v = a.version;
        Ok(format!("Open CASCADE {}.{}.{} ({})", (v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff, a.dir))
    }
    #[cfg(not(windows))]
    {
        Err("The native OpenCascade reader is only built for Windows.".into())
    }
}

/// Un corp B-rep construit si triangulat de OCCT din descrierea neutra (vezi `crate::acis`).
#[derive(Clone, Debug, Default)]
pub struct BrepMesh {
    /// triunghiuri in unitatile descrierii, orientate spre exterior
    pub tris: Vec<[DVec3; 3]>,
    pub faces: usize,
    pub faces_failed: usize,
    pub solids: usize,
    pub open_shells: usize,
    pub curve_fallbacks: usize,
    pub warnings: Vec<String>,
}

/// Construieste fetele OCCT din descrierea B-rep (`ints` / `reals`, formatul din mbocct.cpp), le repara
/// (ShapeFix) si le trianguleaza. Fir-sigur: se poate apela in paralel.
pub fn brep_from_desc(ints: &[i32], reals: &[f64], q: MeshQuality) -> Result<BrepMesh, String> {
    #[cfg(not(windows))]
    {
        let _ = (ints, reals, q);
        Err("The native OpenCascade reader is only built for Windows.".into())
    }
    #[cfg(windows)]
    {
        let api = ffi::api()?;
        let f = api.brep_from_desc.ok_or("occt\\mbocct.dll is outdated (no ACIS support).")?;
        let m = unsafe { f(ints.as_ptr(), ints.len() as i32, reals.as_ptr(), reals.len() as i32, q.rel_deflection, q.ang_deflection) };
        if m.is_null() {
            return Err("OpenCascade returned no result.".into());
        }
        let res = (|| {
            let err = ffi::s(unsafe { (api.error)(m) });
            if !err.is_empty() {
                return Err(err);
            }
            let mut out = BrepMesh::default();
            for i in 0..unsafe { (api.info_count)(m) } {
                let (k, v) = (ffi::s(unsafe { (api.info)(m, i, 0) }), ffi::s(unsafe { (api.info)(m, i, 1) }));
                let n: usize = v.parse().unwrap_or(0);
                match k.as_str() {
                    "faces" => out.faces = n,
                    "faces_failed" => out.faces_failed = n,
                    "solids" => out.solids = n,
                    "open_shells" => out.open_shells = n,
                    "curve_fallbacks" => out.curve_fallbacks = n,
                    _ => {}
                }
            }
            out.warnings = (0..unsafe { (api.warning_count)(m) }).map(|i| ffi::s(unsafe { (api.warning)(m, i) })).collect();
            if unsafe { (api.part_count)(m) } > 0 {
                let nt = unsafe { (api.part_tri_count)(m, 0) }.max(0) as usize;
                let xp = unsafe { (api.part_xyz)(m, 0) };
                if nt > 0 && !xp.is_null() {
                    let xyz = unsafe { std::slice::from_raw_parts(xp, nt * 9) };
                    out.tris = xyz
                        .chunks_exact(9)
                        .map(|v| [DVec3::new(v[0], v[1], v[2]), DVec3::new(v[3], v[4], v[5]), DVec3::new(v[6], v[7], v[8])])
                        .filter(|t| t.iter().all(|p| p.is_finite()))
                        .collect();
                }
            }
            Ok(out)
        })();
        unsafe { (api.free)(m) };
        res
    }
}

/// Citeste un fisier STEP / IGES / BREP prin OCCT.
pub fn load(path: &str, kind: CadKind, q: MeshQuality, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    #[cfg(not(windows))]
    {
        let _ = (path, kind, q, progress);
        Err("The native OpenCascade reader is only built for Windows.".into())
    }
    #[cfg(windows)]
    {
        use std::ffi::{c_char, c_void, CString};
        let api = ffi::api()?;
        let t0 = std::time::Instant::now();
        let cpath = CString::new(path).map_err(|_| "Invalid file path".to_string())?;
        // callback de progres: ctx = &&dyn Fn (pointer subtire catre pointerul gros)
        extern "C" fn tramp(ctx: *mut c_void, f: f64, msg: *const c_char) {
            let cb = unsafe { &*(ctx as *const &dyn Fn(f32, &str)) };
            let m = ffi::s(msg);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cb(f as f32, &m)));
        }
        let pref: &dyn Fn(f32, &str) = progress;
        let ctx = &pref as *const &dyn Fn(f32, &str) as *mut c_void;
        let m = unsafe { (api.load)(cpath.as_ptr(), kind as i32, q.rel_deflection, q.ang_deflection, Some(tramp), ctx) };
        if m.is_null() {
            return Err("OpenCascade returned no result.".into());
        }
        // eliberare garantata, si la erori
        struct Guard(&'static ffi::Api, *mut ffi::MboModel);
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe { (self.0.free)(self.1) }
            }
        }
        let g = Guard(api, m);
        let m = g.1 as *const ffi::MboModel;
        let err = ffi::s(unsafe { (api.error)(m) });
        if !err.is_empty() {
            return Err(err);
        }
        progress(0.95, "Building elements");
        let file = crate::file_name(path);
        let stem = std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| file.clone());
        let n = unsafe { (api.part_count)(m) };
        let mut elements = Vec::with_capacity(n.max(0) as usize);
        let mut tri_total = 0usize;
        for i in 0..n {
            let st = |f: i32| ffi::s(unsafe { (api.part_str)(m, i, f) });
            let (name, part, apath, id, parent, shape) = (st(0), st(1), st(2), st(3), st(4), st(5));
            let nt = unsafe { (api.part_tri_count)(m, i) }.max(0) as usize;
            let (xp, cp) = unsafe { ((api.part_xyz)(m, i), (api.part_rgba)(m, i)) };
            if nt == 0 || xp.is_null() || cp.is_null() {
                continue;
            }
            let xyz = unsafe { std::slice::from_raw_parts(xp, nt * 9) };
            let rgba = unsafe { std::slice::from_raw_parts(cp, nt * 4) };
            let mut tris = Vec::with_capacity(nt);
            let mut colors = Vec::with_capacity(nt);
            for t in 0..nt {
                let v = &xyz[t * 9..t * 9 + 9];
                let tri = [DVec3::new(v[0], v[1], v[2]), DVec3::new(v[3], v[4], v[5]), DVec3::new(v[6], v[7], v[8])];
                if tri.iter().any(|p| !p.is_finite()) {
                    continue;
                }
                tris.push(tri);
                let c = &rgba[t * 4..t * 4 + 4];
                colors.push([c[0], c[1], c[2], c[3]]);
            }
            if tris.is_empty() {
                continue;
            }
            tri_total += tris.len();
            let (solids, faces) = unsafe { ((api.part_int)(m, i, 0), (api.part_int)(m, i, 1)) };
            let p = |n: &str, v: String| Prop { group: "CAD".into(), name: n.into(), value: v };
            let mut props = vec![p("Assembly path", if apath.is_empty() { stem.clone() } else { apath.clone() })];
            if !part.is_empty() {
                props.push(p("Part name", part.clone()));
            }
            if !name.is_empty() && name != part {
                props.push(p("Instance name", name.clone()));
            }
            props.push(p("Shape type", shape));
            props.push(p("Solids", solids.to_string()));
            props.push(p("Faces", faces.to_string()));
            props.push(p("Source format", kind.label().into()));
            props.push(p("XCAF id", id.clone()));
            elements.push(RawElement {
                name,
                ifc_type: "Part".into(),
                // id stabil (calea XCAF a instantei): Refresh / Compare potrivesc elementele intre versiuni
                guid: format!("{}:{}", kind.label(), id),
                // "storey" = ansamblul parinte: arborele grupeaza piesele pe sub-ansamblu
                storey: if parent.is_empty() { stem.clone() } else { parent },
                props,
                tris,
                colors,
                default_hidden: false,
            });
        }
        let mut info = vec![("Converted from".to_string(), format!("{} (built-in OpenCascade reader)", kind.label()))];
        for i in 0..unsafe { (api.info_count)(m) } {
            info.push((ffi::s(unsafe { (api.info)(m, i, 0) }), ffi::s(unsafe { (api.info)(m, i, 1) })));
        }
        info.push(("Parts".into(), elements.len().to_string()));
        info.push(("Triangles".into(), tri_total.to_string()));
        info.push(("Load time".into(), format!("{:.2} s", t0.elapsed().as_secs_f64())));
        let warnings = (0..unsafe { (api.warning_count)(m) }).map(|i| ffi::s(unsafe { (api.warning)(m, i) })).collect();
        drop(g);
        if elements.is_empty() {
            return Err(format!("{file}: no 3D surfaces could be read."));
        }
        progress(1.0, "Done");
        Ok(RawModel { elements, info, warnings })
    }
}
