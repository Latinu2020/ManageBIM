//! FBX nativ (Autodesk FBX, binar si ASCII) prin ufbx (MIT, legat static): fiecare nod cu mesh = un element,
//! cu numele nodului, calea in ierarhie si culorile materialelor. Coordonate convertite in metri, Z in sus.
use crate::model::{Prop, RawElement, RawModel};
use glam::DVec3;
use std::ffi::{c_char, c_int, CStr, CString};

#[repr(C)]
struct MbFbx {
    _p: [u8; 0],
}

extern "C" {
    fn mbfbx_load(path: *const c_char) -> *mut MbFbx;
    fn mbfbx_error(r: *const MbFbx) -> *const c_char;
    fn mbfbx_unit(r: *const MbFbx) -> f64;
    fn mbfbx_app(r: *const MbFbx) -> *const c_char;
    fn mbfbx_count(r: *const MbFbx) -> c_int;
    fn mbfbx_name(r: *const MbFbx, i: c_int) -> *const c_char;
    fn mbfbx_path(r: *const MbFbx, i: c_int) -> *const c_char;
    fn mbfbx_ntri(r: *const MbFbx, i: c_int) -> c_int;
    fn mbfbx_xyz(r: *const MbFbx, i: c_int) -> *const f64;
    fn mbfbx_rgba(r: *const MbFbx, i: c_int) -> *const u8;
    fn mbfbx_free(r: *mut MbFbx);
}

fn s(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

pub fn load_fbx(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    let file = crate::file_name(path);
    progress(0.05, "Reading FBX");
    let c = CString::new(path).map_err(|_| "Invalid file path".to_string())?;
    let r = unsafe { mbfbx_load(c.as_ptr()) };
    if r.is_null() {
        return Err("Out of memory reading the FBX file.".into());
    }
    struct Guard(*mut MbFbx);
    impl Drop for Guard {
        fn drop(&mut self) {
            unsafe { mbfbx_free(self.0) }
        }
    }
    let g = Guard(r);
    let r = g.0 as *const MbFbx;
    let n = unsafe { mbfbx_count(r) };
    let err = s(unsafe { mbfbx_error(r) });
    if n == 0 {
        return Err(format!("{file}: {}", if err.is_empty() { "no 3D meshes".into() } else { err }));
    }
    progress(0.6, "Building elements");
    let stem = std::path::Path::new(path).file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
    let mut elements = Vec::with_capacity(n as usize);
    for i in 0..n {
        let nt = unsafe { mbfbx_ntri(r, i) }.max(0) as usize;
        let (xp, cp) = unsafe { (mbfbx_xyz(r, i), mbfbx_rgba(r, i)) };
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
            if tri.iter().all(|p| p.is_finite()) {
                tris.push(tri);
                colors.push([rgba[t * 4], rgba[t * 4 + 1], rgba[t * 4 + 2], rgba[t * 4 + 3]]);
            }
        }
        if tris.is_empty() {
            continue;
        }
        let name = s(unsafe { mbfbx_name(r, i) });
        let path_s = s(unsafe { mbfbx_path(r, i) });
        let parent = path_s.rsplit(" / ").next().unwrap_or("").to_string();
        let p = |n: &str, v: String| Prop { group: "FBX".into(), name: n.into(), value: v };
        let props = vec![p("Node", name.clone()), p("Hierarchy", if path_s.is_empty() { stem.clone() } else { path_s.clone() })];
        elements.push(RawElement {
            name: if name.is_empty() { format!("Mesh {}", i + 1) } else { name },
            ifc_type: "Mesh".into(),
            guid: format!("FBX:{}:{}", path_s, i),
            storey: if parent.is_empty() { stem.clone() } else { parent },
            props,
            tris,
            colors,
            default_hidden: false,
        });
    }
    let unit = unsafe { mbfbx_unit(r) };
    let app = s(unsafe { mbfbx_app(r) });
    drop(g);
    let mut info = vec![("Converted from".to_string(), "FBX (built-in reader, ufbx)".to_string())];
    if unit > 0.0 {
        info.push(("File units".into(), format!("{} m per unit -> metres", unit)));
    }
    if !app.is_empty() {
        info.push(("Created with".into(), app));
    }
    progress(1.0, "Done");
    Ok(RawModel { elements, info, warnings: Vec::new() })
}
