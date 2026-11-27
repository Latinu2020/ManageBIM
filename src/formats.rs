//! Additional 3D design formats.
//!
//! Native readers: OBJ (+MTL), STL (ascii / binary), PLY (ascii / binary little endian), DXF (3D faces,
//! polyface / polygon meshes, MESH, extruded lines and polylines, blocks).
//! STEP / IGES / BREP: built-in OpenCascade reader (`crate::occt`, DLLs in `occt\` next to the exe).
//! FreeCAD .FCStd: built-in reader (`crate::fcstd`, ZIP + BREP through OpenCascade) - FreeCAD is not needed.
//! Converted through free tools when installed: DWG (ODA File Converter -> DXF, optional).
//! (OBJ comments `#@...` carry element metadata written by ManageBIM exporters.)
use crate::model::{Prop, RawElement, RawModel};
use glam::{DMat4, DVec3};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

type Tri = ([DVec3; 3], [u8; 4]);

fn stem(path: &str) -> String {
    Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Model".into())
}

fn elem(name: String, ty: &str, level: &str, props: Vec<Prop>, tris: Vec<Tri>) -> RawElement {
    let (t, c): (Vec<[DVec3; 3]>, Vec<[u8; 4]>) = tris.into_iter().unzip();
    RawElement { name, ifc_type: ty.to_string(), guid: String::new(), storey: level.to_string(), props, tris: t, colors: c, default_hidden: false }
}

fn prop(g: &str, n: &str, v: impl Into<String>) -> Prop {
    Prop { group: g.into(), name: n.into(), value: v.into() }
}

fn valid(t: &[DVec3; 3]) -> bool {
    t.iter().all(|p| p.is_finite()) && (t[1] - t[0]).cross(t[2] - t[0]).length_squared() > 1e-18
}

/// Scale guess for unitless meshes: very large extents are almost always millimetres.
fn unit_guess(tris: &[[DVec3; 3]], warnings: &mut Vec<String>) -> f64 {
    let mut mn = DVec3::splat(f64::INFINITY);
    let mut mx = DVec3::splat(f64::NEG_INFINITY);
    for t in tris {
        for p in t {
            mn = mn.min(*p);
            mx = mx.max(*p);
        }
    }
    let size = (mx - mn).max_element();
    if size.is_finite() && size > 2000.0 {
        warnings.push(format!("No units in the file - assumed millimetres (model is {size:.0} units wide)."));
        0.001
    } else {
        1.0
    }
}

fn finish(path: &str, mut elements: Vec<RawElement>, mut info: Vec<(String, String)>, warnings: Vec<String>, format: &str) -> Result<RawModel, String> {
    elements.retain(|e| !e.tris.is_empty());
    if elements.is_empty() {
        return Err(format!("{}: no 3D geometry found (only 2D or unsupported objects?).", crate::file_name(path)));
    }
    info.insert(0, ("Format".into(), format.into()));
    Ok(RawModel { elements, info, warnings })
}

// ====================================================================== OBJ
fn parse_mtl(path: &Path) -> HashMap<String, [u8; 4]> {
    let mut out = HashMap::new();
    let Ok(text) = std::fs::read(path) else { return out };
    let text = String::from_utf8_lossy(&text);
    let mut cur = String::new();
    let mut col = [200u8, 200, 200, 255];
    for l in text.lines() {
        let l = l.trim();
        let mut it = l.split_whitespace();
        match it.next() {
            Some("newmtl") => {
                if !cur.is_empty() {
                    out.insert(cur.clone(), col);
                }
                cur = l[6..].trim().to_string();
                col = [200, 200, 200, 255];
            }
            Some("Kd") => {
                let v: Vec<f64> = it.filter_map(|x| x.parse().ok()).collect();
                if v.len() >= 3 {
                    col[0] = (v[0].clamp(0.0, 1.0) * 255.0) as u8;
                    col[1] = (v[1].clamp(0.0, 1.0) * 255.0) as u8;
                    col[2] = (v[2].clamp(0.0, 1.0) * 255.0) as u8;
                }
            }
            Some("d") => {
                if let Some(a) = it.next().and_then(|x| x.parse::<f64>().ok()) {
                    col[3] = (a.clamp(0.15, 1.0) * 255.0) as u8;
                }
            }
            Some("Tr") => {
                if let Some(a) = it.next().and_then(|x| x.parse::<f64>().ok()) {
                    col[3] = ((1.0 - a).clamp(0.15, 1.0) * 255.0) as u8;
                }
            }
            _ => {}
        }
    }
    if !cur.is_empty() {
        out.insert(cur, col);
    }
    out
}

/// OBJ reader. ManageBIM extensions in comments (written by ManageBIM exporters / scripts):
/// `#@bimrust zup`, `#@scale s`, `#@type t`, `#@level l`, `#@guid g`, `#@prop group|name|value`, `#@rgba r g b a`.
pub fn load_obj(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    progress(0.05, "Reading OBJ");
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
    let text = String::from_utf8_lossy(&bytes);
    let dir = Path::new(path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let mut zup = false;
    let mut scale: Option<f64> = None;
    let mut source = String::new();
    let mut verts: Vec<DVec3> = Vec::new();
    let mut mats: HashMap<String, [u8; 4]> = HashMap::new();
    let mut color = [200u8, 200, 200, 255];
    let mut elements: Vec<RawElement> = Vec::new();
    let mut cur = elem(stem(path), "OBJ Object", "", Vec::new(), Vec::new());
    let mut cur_tris: Vec<Tri> = Vec::new();
    let mut named = false;
    let mut faces = 0usize;
    let total = text.len().max(1);
    let mut done = 0usize;
    for line in text.lines() {
        done += line.len() + 1;
        if faces.is_multiple_of(200_000) {
            progress(0.05 + 0.85 * done as f32 / total as f32, "Reading OBJ");
        }
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        if let Some(rest) = l.strip_prefix("#@") {
            let (k, v) = rest.split_once(' ').unwrap_or((rest, ""));
            let v = v.trim();
            match k {
                "bimrust" => zup = v.contains("zup"),
                "scale" => scale = v.parse().ok(),
                "source" => source = v.to_string(),
                "type" => cur.ifc_type = v.to_string(),
                "level" => cur.storey = v.to_string(),
                "guid" => cur.guid = v.to_string(),
                "hidden" => cur.default_hidden = true,
                "prop" => {
                    let p: Vec<&str> = v.splitn(3, '|').collect();
                    if p.len() == 3 && !p[2].is_empty() {
                        cur.props.push(prop(p[0], p[1], p[2]));
                    }
                }
                "rgba" => {
                    let c: Vec<u8> = v.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                    if c.len() == 4 {
                        color = [c[0], c[1], c[2], c[3].max(38)];
                    }
                }
                _ => {}
            }
            continue;
        }
        if l.starts_with('#') {
            continue;
        }
        let mut it = l.split_whitespace();
        match it.next() {
            Some("v") => {
                let c: Vec<f64> = it.take(3).filter_map(|x| x.parse().ok()).collect();
                if c.len() == 3 {
                    verts.push(DVec3::new(c[0], c[1], c[2]));
                } else {
                    verts.push(DVec3::ZERO);
                }
            }
            Some("f") => {
                let n = verts.len() as i64;
                let idx: Vec<usize> = it
                    .filter_map(|tok| {
                        let i: i64 = tok.split('/').next()?.parse().ok()?;
                        let k = if i < 0 { n + i } else { i - 1 };
                        (k >= 0 && k < n).then_some(k as usize)
                    })
                    .collect();
                for k in 1..idx.len().saturating_sub(1) {
                    cur_tris.push(([verts[idx[0]], verts[idx[k]], verts[idx[k + 1]]], color));
                }
                faces += 1;
            }
            Some("o") | Some("g") => {
                let name = l[1..].trim().to_string();
                if !cur_tris.is_empty() {
                    let mut e = std::mem::replace(&mut cur, elem(name.clone(), "OBJ Object", "", Vec::new(), Vec::new()));
                    let (t, c): (Vec<_>, Vec<_>) = std::mem::take(&mut cur_tris).into_iter().unzip();
                    e.tris = t;
                    e.colors = c;
                    elements.push(e);
                } else if !name.is_empty() {
                    cur.name = name;
                }
                named = true;
            }
            Some("usemtl") => {
                let n = l[6..].trim();
                if let Some(c) = mats.get(n) {
                    color = *c;
                }
            }
            Some("mtllib") => {
                let f = l[6..].trim();
                mats.extend(parse_mtl(&dir.join(f)));
            }
            _ => {}
        }
    }
    if !cur_tris.is_empty() {
        let (t, c): (Vec<_>, Vec<_>) = cur_tris.into_iter().unzip();
        cur.tris = t;
        cur.colors = c;
        elements.push(cur);
    }
    let mut warnings = Vec::new();
    if !zup {
        warnings.push("OBJ assumed Y-up (standard) and converted to Z-up.".into());
    }
    let all: Vec<[DVec3; 3]> = elements.iter().flat_map(|e| e.tris.iter().copied()).collect();
    let s = match scale {
        Some(s) => s,
        None => unit_guess(&all, &mut warnings),
    };
    for e in elements.iter_mut() {
        let mut keep_t = Vec::with_capacity(e.tris.len());
        let mut keep_c = Vec::with_capacity(e.tris.len());
        for (t, c) in e.tris.iter().zip(e.colors.iter()) {
            let t2 = t.map(|p| {
                let p = if zup { p } else { DVec3::new(p.x, -p.z, p.y) };
                p * s
            });
            if valid(&t2) {
                keep_t.push(t2);
                keep_c.push(*c);
            }
        }
        e.tris = keep_t;
        e.colors = keep_c;
        if e.ifc_type == "OBJ Object" && !named {
            e.ifc_type = "Mesh".into();
        }
        let mut p = vec![prop("Element", "Name", e.name.clone()), prop("Element", "Type", e.ifc_type.clone())];
        p.append(&mut e.props);
        e.props = p;
    }
    let mut info = vec![("Vertices".to_string(), verts.len().to_string()), ("Faces".to_string(), faces.to_string())];
    if !source.is_empty() {
        info.push(("Converted from".into(), source));
    }
    finish(path, elements, info, warnings, if zup { "OBJ (ManageBIM export)" } else { "Wavefront OBJ" })
}

// ====================================================================== STL
pub fn load_stl(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    progress(0.05, "Reading STL");
    let d = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
    let gray = [190u8, 190, 195, 255];
    let mut solids: Vec<(String, Vec<[DVec3; 3]>)> = Vec::new();
    let binary = d.len() >= 84 && {
        let n = u32::from_le_bytes([d[80], d[81], d[82], d[83]]) as usize;
        84 + n * 50 == d.len()
    };
    if binary {
        let n = u32::from_le_bytes([d[80], d[81], d[82], d[83]]) as usize;
        let f = |o: usize| f32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) as f64;
        let mut tris = Vec::with_capacity(n);
        for i in 0..n {
            let o = 84 + i * 50 + 12;
            let p = |k: usize| DVec3::new(f(o + k * 12), f(o + k * 12 + 4), f(o + k * 12 + 8));
            tris.push([p(0), p(1), p(2)]);
        }
        solids.push((stem(path), tris));
    } else {
        let text = String::from_utf8_lossy(&d);
        let mut cur: Option<(String, Vec<[DVec3; 3]>)> = None;
        let mut pts: Vec<DVec3> = Vec::new();
        for l in text.lines() {
            let l = l.trim();
            if let Some(n) = l.strip_prefix("solid") {
                let name = n.trim();
                cur = Some((if name.is_empty() { stem(path) } else { name.to_string() }, Vec::new()));
            } else if let Some(v) = l.strip_prefix("vertex") {
                let c: Vec<f64> = v.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                if c.len() == 3 {
                    pts.push(DVec3::new(c[0], c[1], c[2]));
                }
                if pts.len() == 3 {
                    let t = [pts[0], pts[1], pts[2]];
                    pts.clear();
                    cur.get_or_insert_with(|| (stem(path), Vec::new())).1.push(t);
                }
            } else if l.starts_with("endsolid") {
                if let Some(s) = cur.take() {
                    solids.push(s);
                }
            }
        }
        if let Some(s) = cur.take() {
            solids.push(s);
        }
    }
    let mut warnings = Vec::new();
    let all: Vec<[DVec3; 3]> = solids.iter().flat_map(|s| s.1.iter().copied()).collect();
    let s = unit_guess(&all, &mut warnings);
    let elements = solids
        .into_iter()
        .map(|(n, t)| {
            let tris: Vec<Tri> = t.into_iter().map(|t| (t.map(|p| p * s), gray)).filter(|t| valid(&t.0)).collect();
            elem(n.clone(), "STL Solid", "", vec![prop("Element", "Name", n)], tris)
        })
        .collect();
    finish(path, elements, vec![("Encoding".into(), if binary { "binary".into() } else { "ASCII".into() })], warnings, "STL")
}

// ====================================================================== PLY
pub fn load_ply(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    progress(0.05, "Reading PLY");
    let d = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
    let hend = d.windows(10).position(|w| w == b"end_header").ok_or("Not a PLY file (no end_header).")?;
    let mut body = hend + 10;
    while body < d.len() && (d[body] == b'\r' || d[body] == b'\n') {
        body += 1;
        if d[body - 1] == b'\n' {
            break;
        }
    }
    let header = String::from_utf8_lossy(&d[..hend]);
    let mut fmt = "ascii".to_string();
    // (element name, count, properties: (name, type, list count type))
    let mut elems: Vec<(String, usize, Vec<(String, String, Option<String>)>)> = Vec::new();
    for l in header.lines() {
        let t: Vec<&str> = l.split_whitespace().collect();
        match t.first().copied() {
            Some("format") => fmt = t.get(1).unwrap_or(&"ascii").to_string(),
            Some("element") if t.len() >= 3 => elems.push((t[1].to_string(), t[2].parse().unwrap_or(0), Vec::new())),
            Some("property") if t.len() >= 3 => {
                if let Some(e) = elems.last_mut() {
                    if t[1] == "list" && t.len() >= 5 {
                        e.2.push((t[4].to_string(), t[3].to_string(), Some(t[2].to_string())));
                    } else {
                        e.2.push((t[2].to_string(), t[1].to_string(), None));
                    }
                }
            }
            _ => {}
        }
    }
    if fmt == "binary_big_endian" {
        return Err("Big-endian binary PLY is not supported - re-save as ASCII or little endian.".into());
    }
    let size = |ty: &str| match ty {
        "char" | "uchar" | "int8" | "uint8" => 1,
        "short" | "ushort" | "int16" | "uint16" => 2,
        "int" | "uint" | "int32" | "uint32" | "float" | "float32" => 4,
        "double" | "float64" => 8,
        _ => 4,
    };
    let rd = |ty: &str, b: &[u8]| -> f64 {
        match ty {
            "char" | "int8" => b[0] as i8 as f64,
            "uchar" | "uint8" => b[0] as f64,
            "short" | "int16" => i16::from_le_bytes([b[0], b[1]]) as f64,
            "ushort" | "uint16" => u16::from_le_bytes([b[0], b[1]]) as f64,
            "int" | "int32" => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
            "uint" | "uint32" => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
            "double" | "float64" => f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
            _ => f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
        }
    };
    let mut verts: Vec<DVec3> = Vec::new();
    let mut vcol: Vec<[u8; 4]> = Vec::new();
    let mut faces: Vec<Vec<usize>> = Vec::new();
    // the scalar properties of faces (segment, label, object_id, material_index, colour...) and of vertices
    let mut face_vals: Vec<Vec<(String, f64)>> = Vec::new();
    let mut vstats: std::collections::BTreeMap<String, (f64, f64, f64, usize)> = std::collections::BTreeMap::new();
    let mut other: Vec<(String, usize)> = Vec::new();
    let ascii = fmt == "ascii";
    let text = if ascii { String::from_utf8_lossy(&d[body..]).to_string() } else { String::new() };
    let mut toks = text.split_ascii_whitespace();
    let mut pos = body;
    for (name, count, props) in &elems {
        for _ in 0..*count {
            let mut vals: HashMap<&str, f64> = HashMap::new();
            let mut list: Vec<usize> = Vec::new();
            for (pn, ty, lt) in props {
                if let Some(lt) = lt {
                    let n = if ascii {
                        toks.next().and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0) as usize
                    } else {
                        let s = size(lt);
                        let v = rd(lt, d.get(pos..pos + s).ok_or("PLY truncated")?) as usize;
                        pos += s;
                        v
                    };
                    for _ in 0..n {
                        let v = if ascii {
                            toks.next().and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0)
                        } else {
                            let s = size(ty);
                            let v = rd(ty, d.get(pos..pos + s).ok_or("PLY truncated")?);
                            pos += s;
                            v
                        };
                        if pn.contains("vertex") {
                            list.push(v as usize);
                        }
                    }
                } else {
                    let v = if ascii {
                        toks.next().and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0)
                    } else {
                        let s = size(ty);
                        let v = rd(ty, d.get(pos..pos + s).ok_or("PLY truncated")?);
                        pos += s;
                        v
                    };
                    vals.insert(pn.as_str(), v);
                }
            }
            if name == "vertex" {
                verts.push(DVec3::new(*vals.get("x").unwrap_or(&0.0), *vals.get("y").unwrap_or(&0.0), *vals.get("z").unwrap_or(&0.0)));
                let c = |k: &str, k2: &str| vals.get(k).or(vals.get(k2)).copied();
                vcol.push(match (c("red", "r"), c("green", "g"), c("blue", "b")) {
                    (Some(r), Some(g), Some(b)) => [r as u8, g as u8, b as u8, 255],
                    _ => [190, 190, 195, 255],
                });
                for (k, v) in &vals {
                    if !["x", "y", "z", "nx", "ny", "nz", "red", "green", "blue", "alpha", "r", "g", "b", "a", "s", "t", "u", "v", "texture_u", "texture_v"].contains(k) {
                        let e = vstats.entry(k.to_string()).or_insert((f64::INFINITY, f64::NEG_INFINITY, 0.0, 0));
                        e.0 = e.0.min(*v);
                        e.1 = e.1.max(*v);
                        e.2 += *v;
                        e.3 += 1;
                    }
                }
            } else if name == "face" && list.len() >= 3 {
                faces.push(list);
                let mut fv: Vec<(String, f64)> = vals.iter().map(|(k, v)| (k.to_string(), *v)).collect();
                fv.sort_by(|a, b| a.0.cmp(&b.0));
                face_vals.push(fv);
            }
        }
        if name == "vertex" {
            progress(0.4, "Reading PLY faces");
        } else if name != "face" {
            other.push((name.clone(), *count));
        }
    }
    let mut warnings = Vec::new();
    if faces.is_empty() && !verts.is_empty() {
        return Err("This PLY is a point cloud (no faces). Point clouds are not supported yet.".into());
    }
    // faces are split into elements by the first identifying attribute found (as in segmented scans,
    // classified photogrammetry models or exports with objects)
    const IDS: [&str; 10] = ["object_id", "objectid", "instance", "instance_id", "segment", "segment_id", "label", "class", "category", "material_index"];
    let id_key = IDS.iter().find(|k| face_vals.first().is_some_and(|fv| fv.iter().any(|(n, _)| n.eq_ignore_ascii_case(k)))).map(|s| s.to_string());
    let fval = |fv: &[(String, f64)], k: &str| fv.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|x| x.1);
    let mut groups: std::collections::BTreeMap<i64, (Vec<Tri>, std::collections::BTreeMap<String, (f64, f64, usize)>)> = std::collections::BTreeMap::new();
    for (fi, f) in faces.iter().enumerate() {
        let fv = face_vals.get(fi).map(|v| v.as_slice()).unwrap_or(&[]);
        let gid = id_key.as_deref().and_then(|k| fval(fv, k)).map(|v| v as i64).unwrap_or(i64::MIN);
        // the face colour (if present) takes priority over the vertex colour
        let fcol = match (fval(fv, "red"), fval(fv, "green"), fval(fv, "blue")) {
            (Some(r), Some(g), Some(b)) => Some([r as u8, g as u8, b as u8, 255]),
            _ => None,
        };
        let g = groups.entry(gid).or_default();
        for k in 1..f.len() - 1 {
            let (a, b, c) = (f[0], f[k], f[k + 1]);
            if a < verts.len() && b < verts.len() && c < verts.len() {
                g.0.push(([verts[a], verts[b], verts[c]], fcol.unwrap_or(vcol[a])));
            }
        }
        for (n, v) in fv {
            if ["red", "green", "blue", "alpha"].contains(&n.as_str()) || Some(n) == id_key.as_ref() {
                continue;
            }
            let e = g.1.entry(n.clone()).or_insert((f64::INFINITY, f64::NEG_INFINITY, 0));
            e.0 = e.0.min(*v);
            e.1 = e.1.max(*v);
            e.2 += 1;
        }
    }
    let all: Vec<[DVec3; 3]> = groups.values().flat_map(|g| g.0.iter().map(|t| t.0)).collect();
    let s = unit_guess(&all, &mut warnings);
    let num = |v: f64| if v.fract() == 0.0 && v.abs() < 1e15 { format!("{}", v as i64) } else { format!("{v:.4}") };
    let n = stem(path);
    let mut els = Vec::new();
    for (gid, (tris, st)) in groups {
        let tris: Vec<Tri> = tris.into_iter().map(|(t, c)| (t.map(|p| p * s), c)).filter(|t| valid(&t.0)).collect();
        if tris.is_empty() {
            continue;
        }
        let name = match (&id_key, gid) {
            (Some(k), g) if g != i64::MIN => format!("{n} {k} {g}"),
            _ => n.clone(),
        };
        let mut props = vec![prop("Element", "Name", name.clone()), prop("Element", "Faces", (tris.len()).to_string())];
        if let (Some(k), true) = (&id_key, gid != i64::MIN) {
            props.push(prop("PLY", k, gid.to_string()));
        }
        for (k, (lo, hi, _)) in &st {
            props.push(prop("PLY face", k, if lo == hi { num(*lo) } else { format!("{} … {}", num(*lo), num(*hi)) }));
        }
        els.push(elem(name, "PLY Mesh", "", props, tris));
    }
    // vertex attributes (quality, intensity, confidence...) and the header comments - on all elements
    let mut shared: Vec<Prop> = vstats.iter().map(|(k, (lo, hi, sum, c))| prop("PLY vertex", k, format!("{} … {} (mean {})", num(*lo), num(*hi), num(sum / (*c).max(1) as f64)))).collect();
    for l in header.lines() {
        let l = l.trim();
        if let Some(c) = l.strip_prefix("comment ").or_else(|| l.strip_prefix("obj_info ")) {
            let c = c.trim();
            match c.split_once(' ').filter(|(k, _)| k.len() <= 30 && !k.contains(':')) {
                Some((k, v)) if ["author", "generated", "created", "software", "unit", "units", "textureFile", "date", "source"].iter().any(|x| k.eq_ignore_ascii_case(x)) => shared.push(prop("PLY header", k, v.trim().to_string())),
                _ => shared.push(prop("PLY header", &format!("Comment {}", shared.iter().filter(|p| p.group == "PLY header").count() + 1), c.to_string())),
            }
        }
    }
    for (en, c) in &other {
        shared.push(prop("PLY header", &format!("Element {en}"), c.to_string()));
    }
    for e in &mut els {
        e.props.extend(shared.iter().cloned());
    }
    finish(path, els, vec![("Encoding".into(), fmt)], warnings, "PLY")
}

// ====================================================================== DXF
#[derive(Clone, Debug, Default)]
struct Ent {
    ty: String,
    codes: Vec<(i32, String)>,
    children: Vec<Ent>,
}

impl Ent {
    fn s(&self, c: i32) -> Option<&str> {
        self.codes.iter().find(|x| x.0 == c).map(|x| x.1.as_str())
    }
    fn f(&self, c: i32) -> Option<f64> {
        self.s(c).and_then(|v| v.trim().parse().ok())
    }
    fn i(&self, c: i32) -> Option<i64> {
        self.s(c).and_then(|v| v.trim().parse::<f64>().ok()).map(|v| v as i64)
    }
    fn p(&self, c: i32) -> DVec3 {
        DVec3::new(self.f(c).unwrap_or(0.0), self.f(c + 10).unwrap_or(0.0), self.f(c + 20).unwrap_or(0.0))
    }
    fn all_f(&self, c: i32) -> Vec<f64> {
        self.codes.iter().filter(|x| x.0 == c).filter_map(|x| x.1.trim().parse().ok()).collect()
    }
}

/// AutoCAD Color Index -> RGB (standard palette, approximated for 10..249).
fn aci(i: i64) -> [u8; 4] {
    let c: [u8; 3] = match i {
        1 => [230, 50, 50],
        2 => [235, 220, 60],
        3 => [70, 190, 80],
        4 => [70, 200, 210],
        5 => [60, 90, 220],
        6 => [210, 70, 200],
        7 | 0 | 256 => [215, 215, 215],
        8 => [128, 128, 128],
        9 => [192, 192, 192],
        250..=255 => {
            let g = [51u8, 80, 105, 130, 190, 255][(i - 250) as usize];
            [g, g, g]
        }
        10..=249 => {
            let h = ((i - 10) / 10) as f32 * 15.0;
            let shade = (i % 10) as usize;
            let v = [1.0, 1.0, 0.8, 0.8, 0.6, 0.6, 0.5, 0.5, 0.3, 0.3][shade];
            let s = if shade.is_multiple_of(2) { 1.0 } else { 0.5 };
            let c = v * s;
            let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
            let m = v - c;
            let (r, g, b) = match (h / 60.0) as i32 {
                0 => (c, x, 0.0),
                1 => (x, c, 0.0),
                2 => (0.0, c, x),
                3 => (0.0, x, c),
                4 => (x, 0.0, c),
                _ => (c, 0.0, x),
            };
            [((r + m) * 255.0) as u8, ((g + m) * 255.0) as u8, ((b + m) * 255.0) as u8]
        }
        _ => [200, 200, 200],
    };
    [c[0], c[1], c[2], 255]
}

/// The object coordinate system (OCS) for normal `n` - the AutoCAD "arbitrary axis" algorithm.
fn ocs_matrix(n: DVec3) -> DMat4 {
    let n = n.normalize_or(DVec3::Z);
    if n.abs_diff_eq(DVec3::Z, 1e-12) {
        return DMat4::IDENTITY;
    }
    let ax = if n.x.abs() < 1.0 / 64.0 && n.y.abs() < 1.0 / 64.0 { DVec3::Y.cross(n) } else { DVec3::Z.cross(n) }.normalize();
    let ay = n.cross(ax).normalize();
    DMat4::from_cols(ax.extend(0.0), ay.extend(0.0), n.extend(0.0), glam::DVec4::W)
}

struct DxfCtx {
    blocks: HashMap<String, (DVec3, Vec<Ent>)>,
    layer_col: HashMap<String, [u8; 4]>,
    unsupported: HashMap<String, usize>,
    /// triangulated ACIS solids (crate::acis), by entity handle
    acis: crate::acis::AcisMeshes,
}

impl DxfCtx {
    fn color(&self, e: &Ent, layer: &str, inherited: [u8; 4], on_layer0_in_block: bool) -> [u8; 4] {
        if let Some(tc) = e.i(420) {
            return [((tc >> 16) & 255) as u8, ((tc >> 8) & 255) as u8, (tc & 255) as u8, 255];
        }
        match e.i(62) {
            Some(0) => inherited,
            // layer "0" inside a block takes the properties of the insert (AutoCAD rule)
            Some(256) | None if on_layer0_in_block => inherited,
            Some(256) | None => self.layer_col.get(layer).copied().unwrap_or(inherited),
            Some(c) => aci(c.abs()),
        }
    }

    fn tris(&mut self, e: &Ent, xf: &DMat4, inh_col: [u8; 4], inh_layer: &str, depth: u32, out: &mut Vec<Tri>) {
        let mut layer = e.s(8).unwrap_or("0").to_string();
        let l0 = layer == "0" && depth > 0;
        if l0 {
            layer = inh_layer.to_string();
        }
        let col = self.color(e, &layer, inh_col, l0);
        let mut push = |a: DVec3, b: DVec3, c: DVec3, out: &mut Vec<Tri>| {
            let t = [xf.transform_point3(a), xf.transform_point3(b), xf.transform_point3(c)];
            if valid(&t) {
                out.push((t, col));
            }
        };
        let thick = e.f(39).unwrap_or(0.0);
        let ext = DVec3::new(e.f(210).unwrap_or(0.0), e.f(220).unwrap_or(0.0), e.f(230).unwrap_or(1.0)).normalize_or(DVec3::Z);
        let wall = |a: DVec3, b: DVec3, out: &mut Vec<Tri>, push: &mut dyn FnMut(DVec3, DVec3, DVec3, &mut Vec<Tri>)| {
            let up = ext * thick;
            push(a, b, b + up, out);
            push(a, b + up, a + up, out);
        };
        match e.ty.as_str() {
            "3DFACE" => {
                let (a, b, c, d) = (e.p(10), e.p(11), e.p(12), e.p(13));
                push(a, b, c, out);
                if (d - c).length_squared() > 1e-12 {
                    push(a, c, d, out);
                }
            }
            "SOLID" | "TRACE" => {
                let z = e.f(30).unwrap_or(0.0);
                let q = |c: i32| DVec3::new(e.f(c).unwrap_or(0.0), e.f(c + 10).unwrap_or(0.0), z);
                let (a, b, c, d) = (q(10), q(11), q(12), q(13));
                push(a, b, c, out);
                push(b, d, c, out);
            }
            "LINE" if thick.abs() > 1e-9 => wall(e.p(10), e.p(11), out, &mut push),
            "LWPOLYLINE" if thick.abs() > 1e-9 => {
                let z = e.f(38).unwrap_or(0.0);
                let xs = e.all_f(10);
                let ys = e.all_f(20);
                let pts: Vec<DVec3> = xs.iter().zip(ys.iter()).map(|(x, y)| DVec3::new(*x, *y, z)).collect();
                let closed = e.i(70).unwrap_or(0) & 1 == 1;
                let n = pts.len();
                for i in 0..n.saturating_sub(if closed { 0 } else { 1 }) {
                    wall(pts[i], pts[(i + 1) % n], out, &mut push);
                }
            }
            "CIRCLE" if thick.abs() > 1e-9 => {
                let (c, r) = (e.p(10), e.f(40).unwrap_or(0.0));
                let n = 24;
                for i in 0..n {
                    let a0 = std::f64::consts::TAU * i as f64 / n as f64;
                    let a1 = std::f64::consts::TAU * (i + 1) as f64 / n as f64;
                    wall(c + DVec3::new(r * a0.cos(), r * a0.sin(), 0.0), c + DVec3::new(r * a1.cos(), r * a1.sin(), 0.0), out, &mut push);
                }
            }
            "POLYLINE" => {
                let flag = e.i(70).unwrap_or(0);
                if flag & 64 != 0 {
                    // polyface mesh
                    let vs: Vec<DVec3> = e.children.iter().filter(|v| v.i(70).unwrap_or(0) & 64 != 0).map(|v| v.p(10)).collect();
                    for f in e.children.iter().filter(|v| v.i(70).unwrap_or(0) & 128 != 0 && v.i(70).unwrap_or(0) & 64 == 0) {
                        let idx: Vec<usize> = [71, 72, 73, 74].iter().filter_map(|c| f.i(*c)).filter(|i| *i != 0).map(|i| i.unsigned_abs() as usize - 1).filter(|i| *i < vs.len()).collect();
                        for k in 1..idx.len().saturating_sub(1) {
                            push(vs[idx[0]], vs[idx[k]], vs[idx[k + 1]], out);
                        }
                    }
                } else if flag & 16 != 0 {
                    // polygon M x N mesh
                    let (m, n) = (e.i(71).unwrap_or(0) as usize, e.i(72).unwrap_or(0) as usize);
                    let vs: Vec<DVec3> = e.children.iter().map(|v| v.p(10)).collect();
                    if m * n <= vs.len() {
                        for i in 0..m.saturating_sub(1) {
                            for j in 0..n.saturating_sub(1) {
                                let (a, b, c, d) = (vs[i * n + j], vs[i * n + j + 1], vs[(i + 1) * n + j + 1], vs[(i + 1) * n + j]);
                                push(a, b, c, out);
                                push(a, c, d, out);
                            }
                        }
                    }
                } else if thick.abs() > 1e-9 {
                    let vs: Vec<DVec3> = e.children.iter().map(|v| v.p(10)).collect();
                    let closed = flag & 1 == 1;
                    let n = vs.len();
                    for i in 0..n.saturating_sub(if closed { 0 } else { 1 }) {
                        wall(vs[i], vs[(i + 1) % n], out, &mut push);
                    }
                }
            }
            "MESH" => {
                let xs = e.all_f(10);
                let ys = e.all_f(20);
                let zs = e.all_f(30);
                let vs: Vec<DVec3> = (0..xs.len().min(ys.len()).min(zs.len())).map(|i| DVec3::new(xs[i], ys[i], zs[i])).collect();
                let list: Vec<i64> = e.codes.iter().filter(|x| x.0 == 90).filter_map(|x| x.1.trim().parse().ok()).collect();
                // the first 90 values are counts of other lists in some files - faces follow code 93
                let start = e.codes.iter().position(|x| x.0 == 93).map(|p| e.codes[..p].iter().filter(|x| x.0 == 90).count()).unwrap_or(0);
                let mut i = start;
                while i < list.len() {
                    let n = list[i] as usize;
                    if n < 3 || i + n >= list.len() + 1 {
                        break;
                    }
                    let f: Vec<usize> = list[i + 1..(i + 1 + n).min(list.len())].iter().map(|v| *v as usize).filter(|v| *v < vs.len()).collect();
                    for k in 1..f.len().saturating_sub(1) {
                        push(vs[f[0]], vs[f[k]], vs[f[k + 1]], out);
                    }
                    i += n + 1;
                }
            }
            "INSERT" => {
                if depth > 16 {
                    return;
                }
                let name = e.s(2).unwrap_or("").to_string();
                let Some((base, ents)) = self.blocks.get(&name).cloned() else { return };
                let s = DVec3::new(e.f(41).unwrap_or(1.0), e.f(42).unwrap_or(1.0), e.f(43).unwrap_or(1.0));
                let rot = e.f(50).unwrap_or(0.0).to_radians();
                // the insertion point is in the OCS of the normal (210/220/230) - e.g. lying Plant 3D nozzles
                let m = *xf * ocs_matrix(ext) * DMat4::from_translation(e.p(10)) * DMat4::from_rotation_z(rot) * DMat4::from_scale(s) * DMat4::from_translation(-base);
                for c in &ents {
                    self.tris(c, &m, col, &layer, depth + 1, out);
                }
            }
            t if crate::acis::DXF_TYPES.contains(&t) => {
                let h = e.s(5).unwrap_or("").trim().to_uppercase();
                match self.acis.meshes.get(&h) {
                    Some(m) => {
                        // mirrored insert: we keep the normals pointing outwards
                        let flip = xf.determinant() < 0.0;
                        for t in &m.tris {
                            if flip {
                                push(t[0], t[2], t[1], out);
                            } else {
                                push(t[0], t[1], t[2], out);
                            }
                        }
                    }
                    None => *self.unsupported.entry(e.ty.clone()).or_default() += 1,
                }
            }
            _ => {}
        }
    }
}

/// XDATA (extended data of other applications): group "XData <application>", values in order; those of
/// the form "Name=Value" / "Name: Value" become named properties.
fn xdata_props(e: &Ent) -> Vec<Prop> {
    let mut out = Vec::new();
    let mut app: Option<String> = None;
    let mut k = 0;
    let mut i = 0;
    while i < e.codes.len() {
        let (c, v) = (e.codes[i].0, e.codes[i].1.trim());
        match c {
            1001 => {
                app = Some(v.to_string());
                k = 0;
            }
            _ if app.is_none() => {}
            1002 => {} // { } lists - ignored as structure, the values inside them are kept
            1000 | 1003 | 1005 | 1040 | 1041 | 1042 | 1070 | 1071 => {
                if !v.is_empty() && out.len() < 200 {
                    let g = format!("XData {}", app.as_deref().unwrap_or(""));
                    if c == 1000 {
                        if let Some((n, val)) = v.split_once('=').or_else(|| v.split_once(": ")).filter(|(n, _)| !n.trim().is_empty() && n.len() <= 40) {
                            out.push(prop(&g, n.trim(), val.trim().to_string()));
                            i += 1;
                            continue;
                        }
                    }
                    k += 1;
                    out.push(prop(&g, &format!("Value {k}"), v.to_string()));
                }
            }
            1010 | 1011 | 1012 | 1013 => {
                // point: 10xx / 10xx+10 / 10xx+20
                let y = e.codes.get(i + 1).filter(|x| x.0 == c + 10).map(|x| x.1.trim().to_string()).unwrap_or_default();
                let z = e.codes.get(i + 2).filter(|x| x.0 == c + 20).map(|x| x.1.trim().to_string()).unwrap_or_default();
                k += 1;
                out.push(prop(&format!("XData {}", app.as_deref().unwrap_or("")), &format!("Point {k}"), format!("{v}, {y}, {z}")));
            }
            _ => {}
        }
        i += 1;
    }
    out
}

fn dxf_entities(pairs: &[(i32, String)], start: usize) -> (Vec<Ent>, usize) {
    // reads entities until ENDSEC / ENDBLK; returns index after
    let mut out: Vec<Ent> = Vec::new();
    let mut i = start;
    let mut in_poly = false;
    while i < pairs.len() {
        let (c, v) = (&pairs[i].0, pairs[i].1.as_str());
        if *c == 0 {
            if v == "ENDSEC" || v == "ENDBLK" {
                break;
            }
            let mut e = Ent { ty: v.to_string(), ..Default::default() };
            i += 1;
            while i < pairs.len() && pairs[i].0 != 0 {
                e.codes.push(pairs[i].clone());
                i += 1;
            }
            if e.ty == "VERTEX" && in_poly {
                if let Some(p) = out.last_mut() {
                    p.children.push(e);
                }
            } else if e.ty == "SEQEND" {
                in_poly = false;
            } else {
                in_poly = e.ty == "POLYLINE";
                out.push(e);
            }
            continue;
        }
        i += 1;
    }
    (out, i)
}

/// Reads the metadata left by „Cut to DWG”: the plane the 2D drawing was extracted
/// from. Looks first for the `.mbplane` sidecar, then for the comment in the DXF.
pub fn read_cut_plane_meta(path: &str) -> Option<(usize, f32, bool, glam::DVec3)> {
    let parse = |line: &str| -> Option<(usize, f32, bool, glam::DVec3)> {
        if !line.starts_with("MB_CUT_PLANE") { return None; }
        let (mut axis, mut value, mut mirror, mut origin) = (None, None, false, glam::DVec3::ZERO);
        for part in line.split_whitespace().skip(1) {
            if let Some(v) = part.strip_prefix("axis=") { axis = v.parse::<usize>().ok(); }
            else if let Some(v) = part.strip_prefix("value=") { value = v.parse::<f32>().ok(); }
            else if let Some(v) = part.strip_prefix("mirror=") { mirror = v == "1"; }
            else if let Some(v) = part.strip_prefix("origin=") {
                let c: Vec<f64> = v.split(',').filter_map(|x| x.parse().ok()).collect();
                if c.len() == 3 { origin = glam::DVec3::new(c[0], c[1], c[2]); }
            }
        }
        Some((axis?, value?, mirror, origin))
    };
    if let Ok(s) = std::fs::read_to_string(format!("{path}.mbplane")) {
        if let Some(m) = parse(s.lines().next().unwrap_or("")) { return Some(m); }
    }
    // the hidden text in the drawing (layer MB_CUT_PLANE) or the DXF comment; the DWG is converted to DXF text
    let text = dxf_text_of(path).ok().or_else(|| std::fs::read(path).ok().map(|b| String::from_utf8_lossy(&b).to_string()))?;
    text.lines().map(|l| l.trim()).find_map(parse)
}

pub fn load_dxf(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    load_dxf_with(path, progress, None, Vec::new())
}

/// `acis`: the already triangulated ACIS solids (DWG). Without them, if the DXF has ACIS entities, we read them
/// with acadrust (SAT in groups 1 / 3 or SAB in the ACDSDATA section) and translate them here.
/// `plant`: the Plant 3D objects / custom entities from the DWG (`crate::plant3d`), one element each.
fn load_dxf_with(path: &str, progress: &dyn Fn(f32, &str), acis: Option<crate::acis::AcisMeshes>, plant: Vec<crate::plant3d::PlantObject>) -> Result<RawModel, String> {
    progress(0.05, "Reading DXF");
    let d = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
    if d.starts_with(b"AutoCAD Binary DXF") {
        return Err("Binary DXF is not supported - save as ASCII DXF.".into());
    }
    let text = String::from_utf8_lossy(&d);
    let lines: Vec<&str> = text.lines().collect();
    let mut pairs: Vec<(i32, String)> = Vec::with_capacity(lines.len() / 2);
    for k in (0..lines.len().saturating_sub(1)).step_by(2) {
        if let Ok(c) = lines[k].trim().parse::<i32>() {
            pairs.push((c, lines[k + 1].trim_end().to_string()));
        }
    }
    progress(0.3, "Reading DXF sections");
    let has_acis = acis.is_none() && pairs.iter().any(|(c, v)| *c == 0 && crate::acis::DXF_TYPES.contains(&v.as_str()));
    let mut ctx = DxfCtx { blocks: HashMap::new(), layer_col: HashMap::new(), unsupported: HashMap::new(), acis: acis.unwrap_or_default() };
    if has_acis {
        progress(0.1, "Reading ACIS solids");
        match acadrust::DxfReader::from_file(path).and_then(|r| r.read()) {
            Ok(doc) => ctx.acis = crate::acis::mesh_document(&doc, &|f, s| progress(0.1 + f * 0.2, s)),
            Err(e) => {
                ctx.acis.failed.insert(format!("DXF could not be read for ACIS data: {e}"), 1);
            }
        }
    }
    let mut insunits = 0i64;
    let mut entities: Vec<Ent> = Vec::new();
    let mut block_by_handle: HashMap<String, String> = HashMap::new();
    let mut i = 0;
    while i < pairs.len() {
        if pairs[i] == (0, "SECTION".to_string()) && i + 1 < pairs.len() {
            let name = pairs[i + 1].1.clone();
            i += 2;
            match name.as_str() {
                "HEADER" => {
                    while i < pairs.len() && pairs[i] != (0, "ENDSEC".to_string()) {
                        if pairs[i] == (9, "$INSUNITS".to_string()) && i + 1 < pairs.len() {
                            insunits = pairs[i + 1].1.trim().parse().unwrap_or(0);
                        }
                        i += 1;
                    }
                }
                "TABLES" => {
                    let (ents, j) = dxf_entities(&pairs, i);
                    for e in ents.iter().filter(|e| e.ty == "LAYER") {
                        if let Some(n) = e.s(2) {
                            ctx.layer_col.insert(n.to_string(), e.i(420).map(|tc| [((tc >> 16) & 255) as u8, ((tc >> 8) & 255) as u8, (tc & 255) as u8, 255]).unwrap_or_else(|| aci(e.i(62).unwrap_or(7).abs())));
                        }
                    }
                    i = j;
                }
                "BLOCKS" => {
                    while i < pairs.len() && pairs[i] != (0, "ENDSEC".to_string()) {
                        if pairs[i] == (0, "BLOCK".to_string()) {
                            let mut hdr = Ent::default();
                            i += 1;
                            while i < pairs.len() && pairs[i].0 != 0 {
                                hdr.codes.push(pairs[i].clone());
                                i += 1;
                            }
                            let (ents, j) = dxf_entities(&pairs, i);
                            i = j;
                            if let Some(n) = hdr.s(2) {
                                // BLOCK: 330 = the owner BLOCK_RECORD (referenced by Plant 3D objects)
                                if let Some(h) = hdr.s(330) {
                                    block_by_handle.insert(h.trim().to_uppercase(), n.to_string());
                                }
                                ctx.blocks.insert(n.to_string(), (hdr.p(10), ents));
                            }
                        }
                        i += 1;
                    }
                }
                "ENTITIES" => {
                    let (ents, j) = dxf_entities(&pairs, i);
                    entities = ents;
                    i = j;
                }
                _ => {}
            }
        }
        i += 1;
    }
    progress(0.5, "Generating DXF geometry");
    let scale = match insunits {
        1 => 0.0254,
        2 => 0.3048,
        4 => 0.001,
        5 => 0.01,
        6 => 1.0,
        _ => 0.0,
    };
    let xf = DMat4::IDENTITY;
    let mut layers: HashMap<String, Vec<Tri>> = HashMap::new();
    let mut elements: Vec<RawElement> = Vec::new();
    let n = entities.len().max(1);
    for (k, e) in entities.iter().enumerate() {
        if k % 5000 == 0 {
            progress(0.5 + 0.45 * k as f32 / n as f32, "Generating DXF geometry");
        }
        let layer = e.s(8).unwrap_or("0").to_string();
        let mut out = Vec::new();
        ctx.tris(e, &xf, [215, 215, 215, 255], &layer, 0, &mut out);
        if out.is_empty() {
            continue;
        }
        if e.ty == "INSERT" {
            let bn = e.s(2).unwrap_or("").to_string();
            let p = e.p(10);
            let mut props = vec![
                prop("Element", "Name", bn.clone()),
                prop("Element", "Block", bn.clone()),
                prop("Element", "Layer", layer.clone()),
                prop("Element", "Handle", e.s(5).unwrap_or("").to_string()),
                prop("Element", "Insertion point", format!("{:.3}, {:.3}, {:.3}", p.x, p.y, p.z)),
                prop("Element", "Rotation", format!("{}°", e.f(50).unwrap_or(0.0))),
            ];
            // block attributes (ATTRIB after INSERT, until SEQEND): tag = value
            let mut title: Option<String> = None;
            for a in entities[k + 1..].iter().take_while(|a| a.ty == "ATTRIB") {
                let (Some(tag), Some(val)) = (a.s(2), a.s(1)) else { continue };
                let (tag, val) = (tag.trim(), val.trim());
                if tag.is_empty() {
                    continue;
                }
                if title.is_none() && !val.is_empty() && ["NAME", "TAG", "NUME", "ID", "MARK", "NUMBER"].contains(&tag.to_ascii_uppercase().as_str()) {
                    title = Some(val.to_string());
                }
                props.push(prop("Attributes", tag, val.to_string()));
            }
            props.extend(xdata_props(e));
            let label = title.map(|t| format!("{bn} {t}")).unwrap_or_else(|| bn.clone());
            let mut el = elem(label, "Block", &layer, props, out);
            el.guid = e.s(5).unwrap_or("").to_string();
            elements.push(el);
        } else if crate::acis::DXF_TYPES.contains(&e.ty.as_str()) {
            // one element per ACIS solid: storey = the layer, the entity colour
            let h = e.s(5).unwrap_or("").trim().to_uppercase();
            let kind = match e.ty.as_str() {
                "3DSOLID" => "3D Solid",
                "REGION" => "Region",
                "BODY" => "Body",
                _ => "Surface",
            };
            let name = format!("{kind} {h}");
            let mut props = vec![
                prop("Element", "Name", name.clone()),
                prop("Element", "Layer", layer.clone()),
                prop("Element", "Handle", h.clone()),
                prop("Element", "Entity", e.ty.clone()),
            ];
            props.extend(xdata_props(e));
            if let Some(m) = ctx.acis.meshes.get(&h) {
                props.push(prop("ACIS", "Faces", m.faces.to_string()));
                props.push(prop("ACIS", "Closed solids", m.solids.to_string()));
                if m.faces_failed > 0 {
                    props.push(prop("ACIS", "Faces not translated", m.faces_failed.to_string()));
                }
            }
            let mut el = elem(name, kind, &layer, props, out);
            el.guid = h;
            elements.push(el);
        } else {
            layers.entry(layer).or_default().extend(out);
        }
    }
    // Plant 3D objects: the catalog blocks (Plant3DCatalogItem_* / *U*) placed with the transforms
    // from the proxy graphics + the proxy faces (pipe, beams); one element per object
    let (mut plant_shown, mut plant_missing, mut plant_unplaced) = (0usize, 0usize, 0usize);
    let plant_total = plant.len();
    for o in plant {
        let mut e = Ent { ty: "PLANT3D".into(), ..Default::default() };
        e.codes.push((8, o.layer.clone()));
        e.codes.push((5, o.handle.clone()));
        match o.color {
            acadrust::types::Color::Index(i) => e.codes.push((62, i.to_string())),
            acadrust::types::Color::ByBlock => e.codes.push((62, "0".into())),
            acadrust::types::Color::Rgb { r, g, b } => e.codes.push((420, ((r as i64) << 16 | (g as i64) << 8 | b as i64).to_string())),
            _ => {}
        }
        let col = ctx.color(&e, &o.layer, [215, 215, 215, 255], false);
        let mut out: Vec<Tri> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        for pb in &o.blocks {
            // by handle (the *U* names can change when the DXF is written), then by name
            let bn = block_by_handle.get(&pb.record).unwrap_or(&pb.name);
            let Some((base, ents)) = ctx.blocks.get(bn).cloned() else {
                plant_missing += 1;
                continue;
            };
            let m = pb.xf * DMat4::from_translation(-base);
            for c in &ents {
                ctx.tris(c, &m, col, &o.layer, 1, &mut out);
            }
            if !names.contains(&pb.name) {
                names.push(pb.name.clone());
            }
        }
        out.extend(o.tris.iter().filter(|t| valid(t)).map(|t| (*t, col)));
        plant_unplaced += o.unplaced;
        if out.is_empty() {
            continue;
        }
        plant_shown += 1;
        let label = crate::plant3d::class_label(&o.class);
        let name = format!("{label} {}", o.handle);
        let mut props = vec![
            prop("Element", "Name", name.clone()),
            prop("Element", "Layer", o.layer.clone()),
            prop("Element", "Handle", o.handle.clone()),
            prop("Element", "Entity", o.class.clone()),
        ];
        if o.blocks.is_empty() && crate::plant3d::is_line_structure(&o.class) {
            props.push(prop("Plant 3D", "Geometry", "Approximated: tubes along the proxy-graphics lines".to_string()));
        }
        if !names.is_empty() {
            props.push(prop("Plant 3D", "Catalog blocks", names.join(", ")));
            props.push(prop("Plant 3D", "Placed parts", o.blocks.len().to_string()));
        }
        let mut el = elem(name, &label, &o.layer, props, out);
        el.guid = o.handle;
        elements.push(el);
    }
    let mut lk: Vec<String> = layers.keys().cloned().collect();
    lk.sort();
    for l in lk {
        let t = layers.remove(&l).unwrap();
        elements.push(elem(format!("Layer {l}"), "Layer geometry", &l, vec![prop("Element", "Name", format!("Layer {l}")), prop("Element", "Layer", l.clone())], t));
    }
    let mut warnings = Vec::new();
    // only the solids that really could not be translated (translated ones no longer generate warnings)
    let mut skipped: Vec<(&String, &usize)> = ctx.unsupported.iter().collect();
    skipped.sort_by(|a, b| a.0.cmp(b.0));
    let why = ctx.acis.failure_summary();
    for (t, c) in skipped {
        warnings.push(format!(
            "{c} × {t} (ACIS) could not be translated{} - in AutoCAD use EXPORT to IFC or convert them to meshes (MESHSMOOTH).",
            if why.is_empty() { String::new() } else { format!(": {why}") }
        ));
    }
    if ctx.acis.partial > 0 {
        let types: Vec<&str> = ctx.acis.unsupported.keys().map(|s| s.as_str()).collect();
        warnings.push(format!(
            "{} ACIS solid(s) shown partially: {} face(s) not translated{}.",
            ctx.acis.partial,
            ctx.acis.faces_failed,
            if types.is_empty() { String::new() } else { format!(" ({})", types.join(", ")) }
        ));
    }
    let s = if scale > 0.0 {
        scale
    } else {
        let all: Vec<[DVec3; 3]> = elements.iter().flat_map(|e| e.tris.iter().copied()).collect();
        unit_guess(&all, &mut warnings)
    };
    if s != 1.0 {
        for e in elements.iter_mut() {
            for t in e.tris.iter_mut() {
                *t = t.map(|p| p * s);
            }
        }
    }
    let mut info = vec![("Units".into(), format!("{s} m")), ("Blocks".into(), ctx.blocks.len().to_string()), ("Layers".into(), ctx.layer_col.len().to_string())];
    if ctx.acis.total > 0 {
        info.push(("ACIS solids".into(), format!("{} of {} translated (built-in reader, OpenCascade)", ctx.acis.translated_count(), ctx.acis.total)));
    }
    if plant_total > 0 {
        info.push(("Plant 3D objects".into(), format!("{plant_shown} shown (catalog blocks placed from proxy graphics)")));
    }
    if plant_missing + plant_unplaced > 0 {
        warnings.push(format!("{} Plant 3D part(s) could not be placed (catalog block missing or without a transform).", plant_missing + plant_unplaced));
    }
    let r = finish(path, elements, info, warnings, "DXF");
    match r {
        Err(_) if !ctx.unsupported.is_empty() => Err(format!(
            "The drawing only contains ACIS solids ({}) that could not be translated{}. Export to IFC from AutoCAD / Civil 3D, or convert them to meshes.",
            ctx.unsupported.keys().cloned().collect::<Vec<_>>().join(", "),
            if why.is_empty() { String::new() } else { format!(" ({why})") }
        )),
        o => o,
    }
}

// ====================================================================== external converters
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tools {
    #[serde(default)]
    pub oda: String,
    /// nwd2bimc.exe (ManageBIM for Navisworks): opens NWD / NWC / NWF / RVT through the installed Navisworks
    #[serde(default)]
    pub navisworks: String,
    /// preferred Navisworks version for opening NWD / NWC / RVT (0 = automatic: the newest supported)
    #[serde(default)]
    pub navis_year: u32,
}

static TOOLS: RwLock<Option<Tools>> = RwLock::new(None);

pub fn set_tools(t: Tools) {
    *TOOLS.write().unwrap_or_else(|e| e.into_inner()) = Some(t);
}

fn tools() -> Tools {
    TOOLS.read().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

fn on_path(names: &[&str]) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for n in names {
            let p = dir.join(n);
            if p.is_file() {
                return Some(p.to_string_lossy().to_string());
            }
        }
    }
    None
}

/// Newest match of `<root>\<dir prefix>*\<sub>`
fn find_in(roots: &[PathBuf], prefix: &str, sub: &[&str]) -> Option<String> {
    let mut found: Vec<PathBuf> = Vec::new();
    for r in roots {
        if let Ok(rd) = std::fs::read_dir(r) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_lowercase();
                if n.starts_with(&prefix.to_lowercase()) {
                    for s in sub {
                        let p = e.path().join(s);
                        if p.is_file() {
                            found.push(p);
                        }
                    }
                }
            }
        }
    }
    found.sort();
    found.pop().map(|p| p.to_string_lossy().to_string())
}

pub fn detect_tools() -> Tools {
    let pf: Vec<PathBuf> = ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)", "LOCALAPPDATA"].iter().filter_map(|v| std::env::var_os(v).map(PathBuf::from)).collect();
    let pf_oda: Vec<PathBuf> = pf.iter().map(|p| p.join("ODA")).collect();
    Tools {
        oda: find_in(&pf_oda, "ODAFileConverter", &["ODAFileConverter.exe"]).or_else(|| on_path(&["ODAFileConverter", "ODAFileConverter.exe"])).unwrap_or_default(),
        navisworks: find_nwd2bimc().unwrap_or_default(),
        navis_year: 0,
    }
}

/// nwd2bimc.exe next to ManageBIM.exe (AddIns\Navisworks\, AddIns\ or the same folder) - only if Navisworks is installed.
pub fn find_nwd2bimc() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let navis_installed = ["ProgramFiles", "ProgramW6432"].iter().filter_map(std::env::var_os).any(|pf| {
        std::fs::read_dir(PathBuf::from(pf).join("Autodesk"))
            .map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().starts_with("Navisworks") && e.path().join("Autodesk.Navisworks.Automation.dll").is_file()))
            .unwrap_or(false)
    });
    if !navis_installed {
        return None;
    }
    [dir.join("AddIns").join("Navisworks").join("nwd2bimc.exe"), dir.join("AddIns").join("nwd2bimc.exe"), dir.join("nwd2bimc.exe")]
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().to_string())
}

pub const NAVIS_EXT: [&str; 4] = ["nwd", "nwc", "nwf", "rvt"];

/// NWD / NWC / NWF / RVT: closed Autodesk formats. We open them through the installed Navisworks
/// (the official API, the ManageBIM add-in) - converted to .bimc and read from there.
pub fn convert_navisworks(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    let ext = ext_of(path).to_uppercase();
    let mut exe = tools().navisworks;
    if exe.is_empty() || !Path::new(&exe).is_file() {
        exe = find_nwd2bimc().unwrap_or_default();
    }
    if exe.is_empty() {
        return Err(format!(
            "{ext} is a closed Autodesk format. ManageBIM opens it through Autodesk Navisworks Manage / Simulate installed on this PC, with the ManageBIM add-in \
             (run AddIns\\ManageBIM-Navisworks-AddIn.exe). Without Navisworks: export the model to IFC or, in Navisworks, Tool add-ins > Export to ManageBIM (.bimc)."
        ));
    }
    // the installed versions, in trial order; if one fails (e.g. 2027, not yet supported) we move on to the next
    let years = navis_years();
    let mut errors: Vec<String> = Vec::new();
    let tries: Vec<Option<u32>> = if years.is_empty() { vec![None] } else { years.iter().map(|y| Some(*y)).collect() };
    for y in tries {
        match run_nwd2bimc(&exe, path, y, progress) {
            Ok(m) => {
                if let Some(y) = y {
                    *NAVIS_GOOD.write().unwrap_or_else(|e| e.into_inner()) = Some(y);
                }
                return Ok(m);
            }
            Err(e) => errors.push(match y {
                Some(y) => format!("Navisworks {y}: {e}"),
                None => e,
            }),
        }
    }
    Err(format!("Navisworks could not convert {}: {}", file_name_of(path), errors.join(" | ")))
}

/// the Navisworks version that worked last time (tried first)
static NAVIS_GOOD: RwLock<Option<u32>> = RwLock::new(None);

/// The installed Navisworks Manage / Simulate versions, in trial order: the one chosen in settings,
/// the one that worked last time, the supported ones (2026 -> 2022), then the newer ones (2027+).
pub fn navis_years() -> Vec<u32> {
    let mut found: Vec<u32> = Vec::new();
    for pf in ["ProgramFiles", "ProgramW6432"].iter().filter_map(std::env::var_os) {
        if let Ok(rd) = std::fs::read_dir(PathBuf::from(pf).join("Autodesk")) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if let Some(y) = n.strip_prefix("Navisworks Manage ").or_else(|| n.strip_prefix("Navisworks Simulate ")).and_then(|y| y.trim().parse::<u32>().ok()) {
                    if e.path().join("Autodesk.Navisworks.Automation.dll").is_file() && !found.contains(&y) {
                        found.push(y);
                    }
                }
            }
        }
    }
    order_years(&found, tools().navis_year, *NAVIS_GOOD.read().unwrap_or_else(|e| e.into_inner()))
}

/// the trial order (separate, for tests)
pub fn order_years(found: &[u32], preferred: u32, good: Option<u32>) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    for y in [Some(preferred).filter(|y| *y > 0), good].into_iter().flatten() {
        if found.contains(&y) && !out.contains(&y) {
            out.push(y);
        }
    }
    let mut sup: Vec<u32> = found.iter().copied().filter(|y| *y <= LAST_SUPPORTED_NAVIS && !out.contains(y)).collect();
    sup.sort_unstable_by(|a, b| b.cmp(a));
    let mut newer: Vec<u32> = found.iter().copied().filter(|y| *y > LAST_SUPPORTED_NAVIS && !out.contains(y)).collect();
    newer.sort_unstable();
    out.extend(sup);
    out.extend(newer);
    out
}

/// the latest Navisworks version for which the ManageBIM add-in exists
pub const LAST_SUPPORTED_NAVIS: u32 = 2026;

fn run_nwd2bimc(exe: &str, path: &str, year: Option<u32>, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    let ext = ext_of(path).to_uppercase();
    progress(0.02, &match year {
        Some(y) => format!("Starting Navisworks {y} (background)"),
        None => "Starting Navisworks (background)".to_string(),
    });
    let dir = temp_dir("navis")?;
    let out = dir.join("model.bimc");
    let mut c = cmd(exe);
    if let Some(y) = year {
        c.arg("--year").arg(y.to_string());
    }
    c.arg(path).arg(&out).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let mut child = c.spawn().map_err(|e| format!("Navisworks converter could not be started: {e}"))?;
    // errors on a separate thread (otherwise a full stderr buffer can block the process)
    let err_pipe = child.stderr.take();
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut e) = err_pipe {
            use std::io::Read;
            let _ = e.read_to_string(&mut s);
        }
        s
    });
    // real progress: "PROGRESS <0..1>\t<text>" from nwd2bimc (read from the Navisworks add-in)
    let mut log = String::new();
    if let Some(o) = child.stdout.take() {
        use std::io::BufRead;
        for line in std::io::BufReader::new(o).lines().map_while(Result::ok) {
            if let Some(rest) = line.strip_prefix("PROGRESS ") {
                let (f, msg) = rest.split_once('\t').unwrap_or((rest, ""));
                let f: f32 = f.trim().parse().unwrap_or(0.0);
                let msg = if msg.is_empty() { "Converting in Navisworks".to_string() } else { format!("Navisworks: {msg}") };
                progress(0.05 + 0.8 * f.clamp(0.0, 1.0), &msg);
            } else {
                log.push_str(&line);
                log.push('\n');
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    log.push_str(&err_thread.join().unwrap_or_default());
    if !out.is_file() || !status.success() {
        let _ = std::fs::remove_dir_all(&dir);
        let msg: Vec<&str> = log.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("Opening") && !l.starts_with("Starting")).collect();
        let detail = if msg.is_empty() { format!("exit code {:?}", status.code()) } else { msg.join(" ") };
        return Err(detail);
    }
    progress(0.86, "Reading converted model");
    let r = crate::bimc::read(&out.to_string_lossy());
    let _ = std::fs::remove_dir_all(&dir);
    r.map(|mut m| {
        m.info.insert(0, ("Converted from".into(), format!("{ext} (Autodesk Navisworks)")));
        m
    })
}

fn cmd(exe: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c
}

fn temp_dir(tag: &str) -> Result<PathBuf, String> {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let d = std::env::temp_dir().join(format!("bimrust_{tag}_{n}"));
    std::fs::create_dir_all(&d).map_err(|e| format!("Cannot create a temp folder: {e}"))?;
    Ok(d)
}

/// STEP / IGES / BREP: the built-in OpenCascade reader (no external programs).
pub fn load_cad(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    let ext = ext_of(path);
    let kind = crate::occt::CadKind::from_ext(&ext).ok_or_else(|| format!("Not a CAD exchange format: .{ext}"))?;
    crate::occt::load(path, kind, crate::occt::MeshQuality::default(), progress).map_err(|e| format!("{}: {e}", file_name_of(path)))
}

pub fn ext_of(path: &str) -> String {
    Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn run(mut c: std::process::Command, what: &str) -> Result<String, String> {
    let out = c.output().map_err(|e| format!("{what} could not be started: {e}"))?;
    let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if let Some(l) = log.lines().find(|l| l.contains("BIMRUST_ERROR")) {
        return Err(format!("{what}: {}", l.replace("BIMRUST_ERROR", "").trim()));
    }
    Ok(log)
}

fn file_name_of(path: &str) -> String {
    Path::new(path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}

pub fn convert_dwg(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    let t = tools();
    // order: the built-in reader -> Navisworks (object enablers; also when the built-in model has non-Plant 3D
    // custom entities without geometry, the model with more triangles is kept) -> ODA File Converter
    let navis = if !t.navisworks.is_empty() && Path::new(&t.navisworks).is_file() { Some(t.navisworks.clone()) } else { find_nwd2bimc() };
    let mut notes = Vec::new();
    // 1. the built-in reader: ACIS solids + Plant 3D objects (the catalog blocks) - more complete than
    //    Navisworks on Plant 3D models (which shows only the pipes). Navisworks / ODA remain the fallback.
    let tris = |m: &RawModel| m.elements.iter().map(|e| e.tris.len()).sum::<usize>();
    let native = convert_dwg_native(path, progress);
    match &native {
        Ok(m) => {
            // non-Plant 3D custom entities without geometry (e.g. AutoCAD Architecture / MEP objects): Navisworks
            // can draw them through object enablers -> we try it and keep the more complete model
            let foreign = m.info.iter().any(|(k, v)| k == "Custom objects without 3D geometry" && v.split(", ").any(|c| !c.split_once(' ').map(|(_, n)| n.starts_with("ACPP")).unwrap_or(true)));
            if !(foreign && navis.is_some()) {
                return native;
            }
        }
        Err(e) => notes.push(e.clone()),
    }
    let with_note = |mut m: RawModel, notes: &[String]| {
        for n in notes {
            m.warnings.insert(0, format!("The built-in DWG reader could not open it: {n}"));
        }
        m
    };
    if navis.is_some() {
        match convert_navisworks(path, progress) {
            Ok(n) => {
                return Ok(match native {
                    Ok(m) if tris(&m) >= tris(&n) => m,
                    Ok(_) => n,
                    Err(_) => with_note(n, &notes),
                })
            }
            Err(_) => {
                if let Ok(m) = native {
                    return Ok(m);
                }
            }
        }
    }
    if !t.oda.is_empty() && Path::new(&t.oda).is_file() {
        if let Ok(m) = convert_dwg_oda(&t.oda, path, progress) {
            return Ok(with_note(m, &notes));
        }
    }
    native
}

/// The built-in DWG reader (acadrust, pure Rust, MPL-2.0): DWG -> DXF in memory -> the DXF loader.
/// 3D faces, meshes, polyface, blocks, layers. ACIS solids (3DSOLID / REGION / BODY / surfaces)
/// are translated to OpenCascade B-rep and triangulated (`crate::acis`); only those that fail are reported.
/// Autodesk Plant 3D objects (custom entities, "UNKNOWN" in acadrust) are placed from their proxy graphics
/// and from the references to the catalog blocks (`crate::plant3d`).
pub fn convert_dwg_native(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    use acadrust::DwgReader;
    progress(0.05, "Reading DWG (built-in reader)");
    let mut reader = DwgReader::from_mmap(path).map_err(|e| format!("Cannot open the DWG: {e}"))?;
    let (doc, recovered) = match reader.read() {
        Ok(d) => (d, None),
        // corrupt / unknown records (e.g. some large Plant 3D DWGs): we re-read in recovery mode,
        // which skips the objects that cannot be decoded instead of abandoning the whole file
        Err(e) => {
            progress(0.1, "Reading DWG (recovery mode)");
            let mut r = DwgReader::from_mmap(path).map_err(|e| format!("Cannot open the DWG: {e}"))?;
            r.options.failsafe = true;
            (r.read().map_err(|e2| format!("Cannot read the DWG: {e} (recovery: {e2})"))?, Some(e.to_string()))
        }
    };
    let mut m = convert_dwg_doc(doc, &file_name_of(path), progress)?;
    attach_plant_db(&mut m, Path::new(path), progress);
    if let Some(e) = recovered {
        m.warnings.insert(0, format!("The DWG has damaged or unsupported records and was read in recovery mode; objects that could not be decoded are missing ({e})."));
    }
    Ok(m)
}

/// The Plant 3D engineering properties (Line Number, Size, Spec, Tag...) from the project database (Piping.dcf,
/// found by walking up from the DWG folder), read only. Without the database: the model stays as it is.
pub fn attach_plant_db(m: &mut RawModel, dwg: &Path, progress: &dyn Fn(f32, &str)) {
    if !m.elements.iter().any(|e| e.props.iter().any(|p| p.name == "Entity" && p.value.starts_with("ACPP"))) {
        return;
    }
    progress(0.96, "Reading Plant 3D project properties");
    match crate::plant3d_db::attach(m, dwg) {
        Ok(Some((db, hit, total))) => m.info.push(("Plant 3D project".into(), format!("{}: properties for {hit} of {total} objects", db.display()))),
        Ok(None) => {}
        Err(e) => m.warnings.push(format!("Plant 3D project database could not be read ({e}) - objects are shown without engineering properties.")),
    }
}

/// Custom entities (Plant 3D and others) in model space that did not become elements: (total, per class).
fn custom_not_shown(doc: &acadrust::CadDocument, shown: &std::collections::HashSet<String>) -> (usize, Vec<(String, usize)>) {
    let mut by: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for e in doc.model_space_entities() {
        if let acadrust::EntityType::Unknown(u) = e {
            if !shown.contains(&format!("{:X}", u.common.handle.value())) {
                *by.entry(u.dxf_name.clone()).or_default() += 1;
            }
        }
    }
    (by.values().sum(), by.into_iter().collect())
}

/// Like `convert_dwg_native`, for an acadrust document already read (or built in memory, in tests).
/// `name` = the file name, used in messages.
pub fn convert_dwg_doc(doc: acadrust::CadDocument, name: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    use acadrust::DxfWriter;
    let acis = crate::acis::mesh_document(&doc, &|f, s| progress(0.2 + f * 0.2, s));
    let plant = crate::plant3d::collect(&doc);
    let shown: std::collections::HashSet<String> = plant.iter().map(|o| o.handle.clone()).collect();
    let (hidden, hidden_by) = custom_not_shown(&doc, &shown);
    progress(0.4, "Converting DWG geometry");
    let dxf = DxfWriter::new(&doc).write_to_vec().map_err(|e| format!("Cannot convert the DWG: {e}"))?;
    drop(doc);
    let dir = temp_dir("dwgnative")?;
    let tmp = dir.join("model.dxf");
    std::fs::write(&tmp, &dxf).map_err(|e| e.to_string())?;
    drop(dxf);
    let r = load_dxf_with(&tmp.to_string_lossy(), &|f, s| progress(0.45 + f * 0.5, s), Some(acis), plant);
    let _ = std::fs::remove_dir_all(&dir);
    // the DXF loader messages refer to the temporary file: we put them under the DWG name
    let mut m = r.map_err(|e| {
        let e = e.replace("model.dxf", name);
        if e.contains("no 3D geometry") {
            format!("{name}: no 3D model in this DWG (2D drawing). ManageBIM shows 3D models - open the 3D DWG / NWC of the project.")
        } else {
            e
        }
    })?;
    m.info.insert(0, ("Converted from".into(), "DWG (built-in reader)".into()));
    if hidden > 0 {
        // e.g. welds (ACPPCONNECTOR without a block), grids, isometric symbols: no 3D geometry in the proxy
        let list: Vec<String> = hidden_by.iter().map(|(c, n)| format!("{n} {c}")).collect();
        m.info.push(("Custom objects without 3D geometry".into(), list.join(", ")));
    }
    Ok(m)
}

fn convert_dwg_oda(oda: &str, path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    let t = Tools { oda: oda.to_string(), ..Default::default() };
    progress(0.1, "Converting DWG with ODA File Converter");
    let dir = temp_dir("dwg")?;
    let (ind, outd) = (dir.join("in"), dir.join("out"));
    std::fs::create_dir_all(&ind).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&outd).map_err(|e| e.to_string())?;
    let name = Path::new(path).file_name().ok_or("bad path")?;
    std::fs::copy(path, ind.join(name)).map_err(|e| format!("Cannot copy the DWG: {e}"))?;
    let mut c = cmd(&t.oda);
    c.arg(&ind).arg(&outd).arg("ACAD2018").arg("DXF").arg("0").arg("1").arg("*.DWG");
    run(c, "ODA File Converter")?;
    let dxf = std::fs::read_dir(&outd).ok().and_then(|rd| rd.flatten().map(|e| e.path()).find(|p| ext_of(&p.to_string_lossy()) == "dxf")).ok_or("ODA File Converter did not produce a DXF.")?;
    let r = load_dxf(&dxf.to_string_lossy(), &|f, s| progress(0.3 + f * 0.65, s));
    let _ = std::fs::remove_dir_all(&dir);
    r.map(|mut m| {
        m.info.insert(0, ("Converted from".into(), "DWG (ODA File Converter)".into()));
        m
    })
}

/// DXF -> DWG with ODA File Converter (free, installed separately). Returns the DWG path.
/// DXF -> DWG. By default with the built-in DWG writer (acadrust, AutoCAD 2018 / AC1032), no other programs;
/// ODA File Converter (if installed) remains only as a fallback. The written DWG is re-read as a check.
pub fn dxf_to_dwg(dxf: &str) -> Result<String, String> {
    match dxf_to_dwg_native(dxf) {
        Ok(p) => Ok(p),
        Err(native) => dxf_to_dwg_oda(dxf).map_err(|oda| format!("{native}; {oda}")),
    }
}

/// the built-in DWG writer; checks the result by re-reading it (the same number of entities)
pub fn dxf_to_dwg_native(dxf: &str) -> Result<String, String> {
    let mut doc = acadrust::DxfReader::from_file(dxf).and_then(|r| r.read()).map_err(|e| format!("cannot read the DXF ({e})"))?;
    let n = doc.entities().count();
    doc.version = acadrust::types::DxfVersion::AC1032;
    let out = Path::new(dxf).with_extension("dwg");
    acadrust::DwgWriter::write_to_file(&out, &doc).map_err(|e| format!("built-in DWG writer failed ({e})"))?;
    let back = acadrust::DwgReader::from_mmap(&out).and_then(|mut r| r.read()).map_err(|e| format!("the written DWG could not be read back ({e})"))?;
    let m = back.entities().count();
    if m < n {
        return Err(format!("the written DWG has {m} of {n} entities"));
    }
    Ok(out.to_string_lossy().to_string())
}

fn dxf_to_dwg_oda(dxf: &str) -> Result<String, String> {
    let oda = tools().oda;
    if oda.is_empty() || !Path::new(&oda).is_file() {
        return Err("ODA File Converter is not installed".into());
    }
    let src = Path::new(dxf);
    let dir = src.parent().ok_or("bad path")?;
    let tmp_in = std::env::temp_dir().join(format!("mb_dxf_in_{}", std::process::id()));
    let tmp_out = std::env::temp_dir().join(format!("mb_dxf_out_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_in);
    let _ = std::fs::remove_dir_all(&tmp_out);
    std::fs::create_dir_all(&tmp_in).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&tmp_out).map_err(|e| e.to_string())?;
    let name = src.file_name().ok_or("bad path")?;
    std::fs::copy(src, tmp_in.join(name)).map_err(|e| e.to_string())?;
    let mut c = std::process::Command::new(&oda);
    c.arg(&tmp_in).arg(&tmp_out).args(["ACAD2018", "DWG", "0", "1", "*.DXF"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let st = c.status().map_err(|e| format!("ODA File Converter could not start: {e}"))?;
    let out = tmp_out.join(Path::new(name).with_extension("dwg"));
    let res = if st.success() && out.is_file() {
        let dst = dir.join(Path::new(name).with_extension("dwg"));
        std::fs::copy(&out, &dst).map_err(|e| e.to_string()).map(|_| dst.to_string_lossy().to_string())
    } else {
        Err("ODA File Converter did not produce a DWG.".into())
    };
    let _ = std::fs::remove_dir_all(&tmp_in);
    let _ = std::fs::remove_dir_all(&tmp_out);
    res
}

// ====================================================================== grids from DWG / DXF
/// Grids found in a drawing: (label, end 1, end 2) in metres, plus the layers with lines (to choose from).
pub struct CadGrids {
    pub grids: Vec<(String, [f64; 2], [f64; 2])>,
    pub layer: String,
    /// (layer, number of lines), the most likely ones for grids first
    pub layers: Vec<(String, usize)>,
    pub scale: f64,
}

fn dxf_text_of(path: &str) -> Result<String, String> {
    if path.to_lowercase().ends_with(".dwg") {
        use acadrust::{DwgReader, DxfWriter};
        let mut reader = DwgReader::from_mmap(path).map_err(|e| format!("Cannot open the DWG: {e}"))?;
        let doc = match reader.read() {
            Ok(d) => d,
            Err(_) => {
                let mut r = DwgReader::from_mmap(path).map_err(|e| format!("Cannot open the DWG: {e}"))?;
                r.options.failsafe = true;
                r.read().map_err(|e| format!("Cannot read the DWG: {e}"))?
            }
        };
        let v = DxfWriter::new(&doc).write_to_vec().map_err(|e| format!("Cannot convert the DWG: {e}"))?;
        Ok(String::from_utf8_lossy(&v).into_owned())
    } else {
        let d = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
        if d.starts_with(b"AutoCAD Binary DXF") {
            return Err("Binary DXF is not supported - save as ASCII DXF.".into());
        }
        Ok(String::from_utf8_lossy(&d).into_owned())
    }
}

/// The MTEXT text without formatting codes (\P, {\f...;}, \A1; ...).
fn mtext_plain(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '{' | '}' => {}
            '\\' => match it.next() {
                Some('P') | Some('~') => out.push(' '),
                Some('\\') => out.push('\\'),
                Some(_) => {
                    // code with a parameter up to ';'
                    for d in it.by_ref() {
                        if d == ';' {
                            break;
                        }
                    }
                }
                None => {}
            },
            _ => out.push(c),
        }
    }
    out.trim().to_string()
}

/// Reads the grids from a DWG / DXF: the lines of the grid layer (guessed by name, or `layer`) and the label at
/// the end of each line (the text / attribute in the nearest bubble). Segments with the same label are joined.
pub fn grids_from_cad(path: &str, layer: Option<&str>) -> Result<CadGrids, String> {
    let (insunits, entities, blocks) = dxf_doc(path)?;
    // lines (LINE, LWPOLYLINE with 2 vertices) on the layer; labels: TEXT / MTEXT / ATTRIB and TEXT from blocks (bubbles)
    let mut segs: Vec<(String, glam::DVec2, glam::DVec2)> = Vec::new();
    let mut tags: Vec<(String, glam::DVec2)> = Vec::new();
    let mut circles: Vec<f64> = Vec::new();
    for e in &entities {
        let lay = e.s(8).unwrap_or("0").to_string();
        match e.ty.as_str() {
            "LINE" => segs.push((lay, e.p(10).truncate(), e.p(11).truncate())),
            "LWPOLYLINE" => {
                let (xs, ys) = (e.all_f(10), e.all_f(20));
                if xs.len() == 2 && ys.len() == 2 {
                    segs.push((lay, glam::DVec2::new(xs[0], ys[0]), glam::DVec2::new(xs[1], ys[1])));
                }
            }
            "TEXT" | "ATTRIB" => {
                if let Some(t) = e.s(1).map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) {
                    // aligned (72/73/74 != 0): alignment point 11 is the text centre
                    let p = if e.i(72).unwrap_or(0) != 0 || e.i(73).unwrap_or(0) != 0 || e.i(74).unwrap_or(0) != 0 { e.p(11) } else { e.p(10) };
                    tags.push((t, p.truncate()));
                }
            }
            "MTEXT" => {
                let raw: String = e.codes.iter().filter(|c| c.0 == 3 || c.0 == 1).map(|c| c.1.as_str()).collect();
                let t = mtext_plain(&raw);
                if !t.is_empty() {
                    tags.push((t, e.p(10).truncate()));
                }
            }
            "CIRCLE" => circles.push(e.f(40).unwrap_or(0.0)),
            "INSERT" => {
                if let Some((base, ents)) = e.s(2).and_then(|n| blocks.get(n)) {
                    let ip = e.p(10);
                    let (sx, sy) = (e.f(41).unwrap_or(1.0), e.f(42).unwrap_or(1.0));
                    let rot = e.f(50).unwrap_or(0.0).to_radians();
                    let xf = |p: DVec3| {
                        let q = glam::DVec2::new((p.x - base.x) * sx, (p.y - base.y) * sy);
                        let (s, c) = rot.sin_cos();
                        glam::DVec2::new(ip.x + q.x * c - q.y * s, ip.y + q.x * s + q.y * c)
                    };
                    for b in ents {
                        match b.ty.as_str() {
                            "TEXT" => {
                                if let Some(t) = b.s(1).map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) {
                                    let p = if b.i(72).unwrap_or(0) != 0 || b.i(73).unwrap_or(0) != 0 { b.p(11) } else { b.p(10) };
                                    tags.push((t, xf(p)));
                                }
                            }
                            "CIRCLE" => circles.push(b.f(40).unwrap_or(0.0) * sx.abs()),
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    // the layers with lines; those with grid names first
    let mut count: HashMap<String, usize> = HashMap::new();
    for s in &segs {
        *count.entry(s.0.clone()).or_default() += 1;
    }
    let looks = |l: &str| {
        let u = l.to_uppercase();
        ["GRID", "AXE", "AXA", "AXIS", "AXES", "-AX", "AX_", "RASTER"].iter().any(|k| u.contains(k)) || u == "AX"
    };
    let mut layers: Vec<(String, usize)> = count.into_iter().collect();
    layers.sort_by(|a, b| looks(&b.0).cmp(&looks(&a.0)).then(b.1.cmp(&a.1)).then(a.0.cmp(&b.0)));
    let chosen = match layer {
        Some(l) => l.to_string(),
        // no layer with grid names: the list of layers, so the user can pick one
        None => match layers.iter().find(|(l, _)| looks(l)) {
            Some((l, _)) => l.clone(),
            None => return Ok(CadGrids { grids: Vec::new(), layer: String::new(), layers, scale: 1.0 }),
        },
    };
    let scale = match insunits {
        1 => 0.0254,
        2 => 0.3048,
        4 => 0.001,
        5 => 0.01,
        6 => 1.0,
        _ => {
            // unknown: architectural drawings are usually in mm
            let ext = segs.iter().map(|s| (s.2 - s.1).length()).fold(0.0, f64::max);
            if ext > 1000.0 { 0.001 } else { 1.0 }
        }
    };
    let gsegs: Vec<(glam::DVec2, glam::DVec2)> = segs.iter().filter(|s| s.0 == chosen).map(|s| (s.1, s.2)).filter(|(a, b)| (*b - *a).length() > 1e-6).collect();
    if gsegs.is_empty() {
        return Err(format!("No lines on the layer {chosen}."));
    }
    // search distance of the label from the end: ~2 x the bubble radius, or 3% of the longest grid line
    let mut rs: Vec<f64> = circles.iter().copied().filter(|r| *r > 0.0).collect();
    rs.sort_by(|a, b| a.total_cmp(b));
    let longest = gsegs.iter().map(|(a, b)| (*b - *a).length()).fold(0.0, f64::max);
    let reach = rs.get(rs.len() / 2).map(|r| r * 2.2).unwrap_or(0.0).max(longest * 0.03);
    let mut by_tag: Vec<(String, glam::DVec2, glam::DVec2)> = Vec::new();
    let mut untagged = 0;
    for (a, b) in gsegs {
        let near = |p: glam::DVec2| tags.iter().map(|(t, q)| (t, (*q - p).length())).filter(|x| x.1 <= reach).min_by(|x, y| x.1.total_cmp(&y.1));
        let tag = match (near(a), near(b)) {
            (Some(x), Some(y)) => Some(if x.1 <= y.1 { x.0.clone() } else { y.0.clone() }),
            (Some(x), None) | (None, Some(x)) => Some(x.0.clone()),
            _ => None,
        };
        let tag = tag.unwrap_or_else(|| {
            untagged += 1;
            format!("G{untagged}")
        });
        // same label and same direction: the segments are joined (the farthest ends)
        let d = (b - a).normalize();
        if let Some(g) = by_tag.iter_mut().find(|g| g.0 == tag && ((g.2 - g.1).normalize().perp_dot(d)).abs() < 0.02) {
            let pts = [g.1, g.2, a, b];
            let mut best = (g.1, g.2, (g.2 - g.1).length());
            for i in 0..4 {
                for j in i + 1..4 {
                    let l = (pts[j] - pts[i]).length();
                    if l > best.2 {
                        best = (pts[i], pts[j], l);
                    }
                }
            }
            g.1 = best.0;
            g.2 = best.1;
        } else {
            by_tag.push((tag, a, b));
        }
    }
    let grids = by_tag.into_iter().map(|(t, a, b)| (t, (a * scale).to_array(), (b * scale).to_array())).collect();
    Ok(CadGrids { grids, layer: chosen, layers, scale })
}

// ====================================================================== portability (Windows / Linux / FreeBSD)

/// Native file dialogs: always on Windows; on Linux / FreeBSD through xdg-desktop-portal (D-Bus).
pub fn native_dialogs() -> bool {
    if cfg!(windows) {
        return true;
    }
    std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
}

/// Opens a file / folder with the default program (Explorer, xdg-open).
pub fn open_with_system(path: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

/// The settings folder: %APPDATA% (Windows) or $XDG_CONFIG_HOME / ~/.config (Linux, FreeBSD).
pub fn config_root() -> Option<std::path::PathBuf> {
    if let Some(a) = std::env::var_os("APPDATA") {
        return Some(std::path::PathBuf::from(a));
    }
    std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
}

/// The local data folder: %LOCALAPPDATA% (Windows) or $XDG_DATA_HOME / ~/.local/share (Linux, FreeBSD).
pub fn data_root() -> Option<std::path::PathBuf> {
    if let Some(a) = std::env::var_os("LOCALAPPDATA") {
        return Some(std::path::PathBuf::from(a));
    }
    std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local").join("share")))
}

/// Safe write: the content goes to a temporary file (synced to disk), the old version becomes
/// `<fisier>.bak`, then the temporary file takes the place of the file. A crash / power cut during saving leaves
/// either the old file or the new one intact - never a half-written one.
pub fn write_atomic(path: &Path, data: &[u8], backup: bool) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = path.with_extension(format!("{}.tmp", path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default()));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    if backup && path.is_file() {
        let bak = backup_path(path);
        let _ = std::fs::remove_file(&bak);
        if std::fs::rename(path, &bak).is_err() {
            let _ = std::fs::copy(path, &bak);
        }
    }
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// `proiect.bimr` -> `proiect.bimr.bak`
pub fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".bak");
    PathBuf::from(s)
}

/// Reads a DXF (or a converted DWG): the units ($INSUNITS), the entities and the blocks.
fn dxf_doc(path: &str) -> Result<(i64, Vec<Ent>, HashMap<String, (DVec3, Vec<Ent>)>), String> {
    let text = dxf_text_of(path)?;
    let lines: Vec<&str> = text.lines().collect();
    let mut pairs: Vec<(i32, String)> = Vec::with_capacity(lines.len() / 2);
    for k in (0..lines.len().saturating_sub(1)).step_by(2) {
        if let Ok(c) = lines[k].trim().parse::<i32>() {
            pairs.push((c, lines[k + 1].trim_end().to_string()));
        }
    }
    let mut insunits = 0i64;
    let mut entities: Vec<Ent> = Vec::new();
    let mut blocks: HashMap<String, (DVec3, Vec<Ent>)> = HashMap::new();
    let mut i = 0;
    while i < pairs.len() {
        if pairs[i] == (0, "SECTION".to_string()) && i + 1 < pairs.len() {
            let name = pairs[i + 1].1.clone();
            i += 2;
            match name.as_str() {
                "HEADER" => {
                    while i < pairs.len() && pairs[i] != (0, "ENDSEC".to_string()) {
                        if pairs[i] == (9, "$INSUNITS".to_string()) && i + 1 < pairs.len() {
                            insunits = pairs[i + 1].1.trim().parse().unwrap_or(0);
                        }
                        i += 1;
                    }
                }
                "BLOCKS" => {
                    while i < pairs.len() && pairs[i] != (0, "ENDSEC".to_string()) {
                        if pairs[i] == (0, "BLOCK".to_string()) {
                            let mut hdr = Ent::default();
                            i += 1;
                            while i < pairs.len() && pairs[i].0 != 0 {
                                hdr.codes.push(pairs[i].clone());
                                i += 1;
                            }
                            let (ents, j) = dxf_entities(&pairs, i);
                            i = j;
                            if let Some(n) = hdr.s(2) {
                                blocks.insert(n.to_string(), (hdr.p(10), ents));
                            }
                        }
                        i += 1;
                    }
                }
                "ENTITIES" => {
                    let (ents, j) = dxf_entities(&pairs, i);
                    entities = ents;
                    i = j;
                }
                _ => {}
            }
        }
        i += 1;
    }
    Ok((insunits, entities, blocks))
}

/// 2D drawing (DWG / DXF) as lines in metres: LINE, LWPOLYLINE / POLYLINE (closed too), ARC, CIRCLE, ELLIPSE,
/// blocks (INSERT, nested too). Returns (layer, x1, y1, x2, y2) and the applied scale (units -> m).
pub fn cad_lines(path: &str) -> Result<(Vec<(String, [f64; 4])>, f64), String> {
    let (insunits, entities, blocks) = dxf_doc(path)?;
    let mut out: Vec<(String, [f64; 4])> = Vec::new();
    fn arc(c: glam::DVec2, r: f64, a0: f64, a1: f64) -> Vec<glam::DVec2> {
        // invalid angles / radii from a damaged drawing: no arc (a „while” loop would be infinite at -inf)
        if !(a0.is_finite() && a1.is_finite() && r.is_finite() && c.is_finite()) {
            return Vec::new();
        }
        let mut sweep = (a1 - a0).rem_euclid(std::f64::consts::TAU);
        if sweep <= 1e-12 {
            sweep = std::f64::consts::TAU;
        }
        let n = ((sweep / std::f64::consts::TAU) * 48.0).ceil().clamp(4.0, 48.0) as usize;
        (0..=n).map(|k| {
            let a = a0 + sweep * k as f64 / n as f64;
            c + glam::DVec2::new(a.cos(), a.sin()) * r
        }).collect()
    }
    fn emit(ents: &[Ent], blocks: &HashMap<String, (DVec3, Vec<Ent>)>, xf: &dyn Fn(glam::DVec2) -> glam::DVec2, layer_in: Option<&str>, out: &mut Vec<(String, [f64; 4])>, depth: u32) {
        if depth > 6 || out.len() > 2_000_000 {
            return;
        }
        fn seg(out: &mut Vec<(String, [f64; 4])>, xf: &dyn Fn(glam::DVec2) -> glam::DVec2, lay: &str, pts: &[glam::DVec2], closed: bool) {
            for w in pts.windows(2) {
                let (a, b) = (xf(w[0]), xf(w[1]));
                out.push((lay.to_string(), [a.x, a.y, b.x, b.y]));
            }
            if closed && pts.len() > 2 {
                let (a, b) = (xf(pts[pts.len() - 1]), xf(pts[0]));
                out.push((lay.to_string(), [a.x, a.y, b.x, b.y]));
            }
        }
        let push = |out: &mut Vec<(String, [f64; 4])>, lay: &str, pts: &[glam::DVec2], closed: bool| seg(out, xf, lay, pts, closed);
        for e in ents {
            // layer "0" in blocks takes the layer of the INSERT
            let own = e.s(8).unwrap_or("0");
            let lay = if own == "0" { layer_in.unwrap_or("0") } else { own };
            match e.ty.as_str() {
                "LINE" => push(out, lay, &[e.p(10).truncate(), e.p(11).truncate()], false),
                "LWPOLYLINE" => {
                    let (xs, ys) = (e.all_f(10), e.all_f(20));
                    let pts: Vec<glam::DVec2> = xs.iter().zip(ys.iter()).map(|(x, y)| glam::DVec2::new(*x, *y)).collect();
                    push(out, lay, &pts, e.i(70).unwrap_or(0) & 1 == 1);
                }
                "CIRCLE" => {
                    let pts = arc(e.p(10).truncate(), e.f(40).unwrap_or(0.0), 0.0, std::f64::consts::TAU);
                    push(out, lay, &pts, false);
                }
                "ARC" => {
                    let pts = arc(e.p(10).truncate(), e.f(40).unwrap_or(0.0), e.f(50).unwrap_or(0.0).to_radians(), e.f(51).unwrap_or(360.0).to_radians());
                    push(out, lay, &pts, false);
                }
                "ELLIPSE" => {
                    let c = e.p(10).truncate();
                    let maj = e.p(11).truncate();
                    let ratio = e.f(40).unwrap_or(1.0);
                    let (t0, mut t1) = (e.f(41).unwrap_or(0.0), e.f(42).unwrap_or(std::f64::consts::TAU));
                    if t1 <= t0 {
                        t1 += std::f64::consts::TAU;
                    }
                    let min = glam::DVec2::new(-maj.y, maj.x) * ratio;
                    let n = 48;
                    let pts: Vec<glam::DVec2> = (0..=n).map(|k| {
                        let t = t0 + (t1 - t0) * k as f64 / n as f64;
                        c + maj * t.cos() + min * t.sin()
                    }).collect();
                    push(out, lay, &pts, false);
                }
                "INSERT" => {
                    if let Some((base, sub)) = e.s(2).and_then(|n| blocks.get(n)) {
                        let ip = e.p(10);
                        let (sx, sy) = (e.f(41).unwrap_or(1.0), e.f(42).unwrap_or(1.0));
                        let rot = e.f(50).unwrap_or(0.0).to_radians();
                        let base = *base;
                        let (s, c) = rot.sin_cos();
                        let inner = move |p: glam::DVec2| {
                            let q = glam::DVec2::new((p.x - base.x) * sx, (p.y - base.y) * sy);
                            glam::DVec2::new(ip.x + q.x * c - q.y * s, ip.y + q.x * s + q.y * c)
                        };
                        let chained = |p: glam::DVec2| xf(inner(p));
                        emit(sub, blocks, &chained, Some(lay), out, depth + 1);
                    }
                }
                _ => {}
            }
        }
    }
    emit(&entities, &blocks, &|p| p, None, &mut out, 0);
    let scale = match insunits {
        1 => 0.0254,
        2 => 0.3048,
        4 => 0.001,
        5 => 0.01,
        6 => 1.0,
        _ => {
            let ext = out.iter().map(|s| (s.1[0] - s.1[2]).abs().max((s.1[1] - s.1[3]).abs())).fold(0.0, f64::max);
            if ext > 1000.0 { 0.001 } else { 1.0 }
        }
    };
    for s in &mut out {
        for v in &mut s.1 {
            *v *= scale;
        }
    }
    Ok((out, scale))
}

#[cfg(test)]
mod cut_plane_meta_tests {
    #[test]
    fn meta_is_read_from_the_hidden_text_inside_the_drawing() {
        // without the .mbplane sidecar file and without the comment in the HEADER: only the text on layer MB_CUT_PLANE
        let dir = std::env::temp_dir().join(format!("mb_meta_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("cut.dxf");
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nWALLS\n10\n0.0\n20\n0.0\n30\n0.0\n11\n1.0\n21\n0.0\n31\n0.0\n0\nTEXT\n8\nMB_CUT_PLANE\n10\n0.0\n20\n0.0\n30\n0.0\n40\n0.002\n1\nMB_CUT_PLANE axis=1 value=12.5000 mirror=1 origin=100.0000,200.0000,0.0000\n0\nENDSEC\n0\nEOF\n";
        std::fs::write(&p, dxf).unwrap();
        let (ax, v, mir, o) = super::read_cut_plane_meta(&p.to_string_lossy()).expect("meta");
        assert_eq!(ax, 1);
        assert!((v - 12.5).abs() < 1e-4);
        assert!(mir);
        assert_eq!(o, glam::DVec3::new(100.0, 200.0, 0.0));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
