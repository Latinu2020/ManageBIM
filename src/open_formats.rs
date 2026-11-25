//! Formate deschise cu proprietati per obiect: 3MF, CityJSON / CityJSONSeq, ifcJSON / ifcXML,
//! 3D Tiles 1.1 (tileset.json) si USD ASCII (.usda). Doar citire; geometria e triangulata, proprietatile
//! fiecarui obiect ajung in Properties (si in exportul IFC).
use crate::e57::{parse_xml, Node};
use crate::model::{Prop, RawElement, RawModel};
use glam::{DMat4, DVec3};
use std::collections::HashMap;
use std::path::Path;

pub(crate) fn prop(g: &str, n: &str, v: impl Into<String>) -> Prop {
    Prop { group: g.into(), name: n.into(), value: v.into() }
}


fn valid(t: &[DVec3; 3]) -> bool {
    t.iter().all(|p| p.is_finite()) && (t[1] - t[0]).cross(t[2] - t[0]).length_squared() > 1e-18
}


/// numele local al unui tag / atribut XML (fara prefixul de namespace)
fn local(n: &str) -> &str {
    n.rsplit(':').next().unwrap_or(n)
}

fn kids<'a>(n: &'a Node, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
    n.kids.iter().filter(move |k| local(&k.name) == name)
}

fn kid<'a>(n: &'a Node, name: &str) -> Option<&'a Node> {
    n.kids.iter().find(|k| local(&k.name) == name)
}

fn attr<'a>(n: &'a Node, name: &str) -> Option<&'a str> {
    n.attrs.iter().find(|(k, _)| local(k) == name).map(|(_, v)| v.as_str())
}

/// "#RRGGBB" sau "#RRGGBBAA"
fn hex_color(s: &str) -> Option<[u8; 4]> {
    let h = s.trim().trim_start_matches('#');
    let p = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    match h.len() {
        6 => Some([p(0)?, p(2)?, p(4)?, 255]),
        8 => Some([p(0)?, p(2)?, p(4)?, p(6)?.max(40)]),
        _ => None,
    }
}

// ====================================================================== 3MF

/// Matricea 3MF "m00 m01 m02 m10 m11 m12 m20 m21 m22 m30 m31 m32" (vectori-rand) -> DMat4 (coloane)
fn mf_transform(s: Option<&str>) -> DMat4 {
    let v: Vec<f64> = s.unwrap_or("").split_whitespace().filter_map(|x| x.parse().ok()).collect();
    if v.len() != 12 {
        return DMat4::IDENTITY;
    }
    DMat4::from_cols_array(&[v[0], v[1], v[2], 0.0, v[3], v[4], v[5], 0.0, v[6], v[7], v[8], 0.0, v[9], v[10], v[11], 1.0])
}

struct MfObject {
    name: String,
    ty: String,
    part: String,
    uuid: String,
    meta: Vec<(String, String)>,
    /// triunghiuri locale cu culoarea fiecaruia
    tris: Vec<([DVec3; 3], [u8; 4])>,
    /// (cheia "fisier#id" a componentei, transformare)
    comps: Vec<(String, DMat4)>,
    materials: Vec<String>,
}

pub fn load_3mf(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    progress(0.05, "Reading 3MF");
    let zip = crate::fcstd::Zip::parse(std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?).map_err(|_| "Not a valid 3MF file (ZIP).".to_string())?;
    // modelul principal: din _rels/.rels (tinta 3dmodel), altfel 3D/3dmodel.model
    let rels = zip.text("_rels/.rels").unwrap_or_default();
    let main = parse_xml(&rels)
        .ok()
        .and_then(|r| kids(&r, "Relationship").find(|x| attr(x, "Type").map_or(false, |t| t.ends_with("/3dmodel"))).and_then(|x| attr(x, "Target")).map(|t| t.trim_start_matches('/').to_string()))
        .unwrap_or_else(|| "3D/3dmodel.model".into());
    let mut objects: HashMap<String, MfObject> = HashMap::new();
    let mut unit = 0.001;
    let mut model_meta: Vec<(String, String)> = Vec::new();
    let mut build: Vec<(String, DMat4, String, String)> = Vec::new();
    // toate partile .model (Production extension: componentele pot fi in alte fisiere)
    let mut parts: Vec<String> = zip.names().filter(|n| n.to_lowercase().ends_with(".model")).cloned().collect();
    parts.sort_by_key(|n| (n.trim_start_matches('/') != main, n.clone()));
    for (pi, part) in parts.iter().enumerate() {
        progress(0.1 + 0.6 * pi as f32 / parts.len().max(1) as f32, "Reading 3MF objects");
        let Some(text) = zip.text(part) else { continue };
        let root = parse_xml(&text).map_err(|e| format!("3MF: {e}"))?;
        let pkey = format!("/{}", part.trim_start_matches('/'));
        let is_main = part.trim_start_matches('/') == main;
        if is_main {
            unit = match attr(&root, "unit").unwrap_or("millimeter") {
                "micron" => 1e-6,
                "centimeter" => 0.01,
                "inch" => 0.0254,
                "foot" => 0.3048,
                "meter" => 1.0,
                _ => 0.001,
            };
            for m in kids(&root, "metadata") {
                if let Some(n) = attr(m, "name") {
                    if !m.text.trim().is_empty() {
                        model_meta.push((n.to_string(), m.text.trim().to_string()));
                    }
                }
            }
        }
        let Some(res) = kid(&root, "resources") else { continue };
        // materiale: basematerials (nume + culoare) si colorgroup (culori)
        let mut groups: HashMap<String, Vec<([u8; 4], String)>> = HashMap::new();
        for bm in kids(res, "basematerials") {
            let id = attr(bm, "id").unwrap_or("").to_string();
            groups.insert(id, kids(bm, "base").map(|b| (attr(b, "displaycolor").and_then(hex_color).unwrap_or([200, 200, 200, 255]), attr(b, "name").unwrap_or("").to_string())).collect());
        }
        for cg in kids(res, "colorgroup") {
            let id = attr(cg, "id").unwrap_or("").to_string();
            groups.insert(id, kids(cg, "color").map(|c| (attr(c, "color").and_then(hex_color).unwrap_or([200, 200, 200, 255]), String::new())).collect());
        }
        for o in kids(res, "object") {
            let id = attr(o, "id").unwrap_or("").to_string();
            let (dpid, dpi) = (attr(o, "pid").unwrap_or("").to_string(), attr(o, "pindex").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0));
            let pick = |pid: &str, idx: usize| groups.get(pid).and_then(|g| g.get(idx)).cloned();
            let mut ob = MfObject {
                name: attr(o, "name").unwrap_or("").to_string(),
                ty: attr(o, "type").unwrap_or("model").to_string(),
                part: attr(o, "partnumber").unwrap_or("").to_string(),
                uuid: attr(o, "UUID").unwrap_or("").to_string(),
                meta: Vec::new(),
                tris: Vec::new(),
                comps: Vec::new(),
                materials: Vec::new(),
            };
            if let Some(mg) = kid(o, "metadatagroup") {
                for m in kids(mg, "metadata") {
                    if let Some(n) = attr(m, "name") {
                        ob.meta.push((n.to_string(), m.text.trim().to_string()));
                    }
                }
            }
            if let Some((_, n)) = pick(&dpid, dpi).filter(|(_, n)| !n.is_empty()) {
                ob.materials.push(n);
            }
            if let Some(mesh) = kid(o, "mesh") {
                let verts: Vec<DVec3> = kid(mesh, "vertices")
                    .map(|v| kids(v, "vertex").map(|x| DVec3::new(attr(x, "x").and_then(|v| v.parse().ok()).unwrap_or(0.0), attr(x, "y").and_then(|v| v.parse().ok()).unwrap_or(0.0), attr(x, "z").and_then(|v| v.parse().ok()).unwrap_or(0.0))).collect())
                    .unwrap_or_default();
                if let Some(ts) = kid(mesh, "triangles") {
                    for t in kids(ts, "triangle") {
                        let ix = |k: &str| attr(t, k).and_then(|v| v.parse::<usize>().ok());
                        let (Some(a), Some(b), Some(c)) = (ix("v1"), ix("v2"), ix("v3")) else { continue };
                        let (Some(a), Some(b), Some(c)) = (verts.get(a), verts.get(b), verts.get(c)) else { continue };
                        let pid = attr(t, "pid").unwrap_or(&dpid).to_string();
                        let pi = ix("p1").unwrap_or(dpi);
                        let (col, mname) = pick(&pid, pi).unwrap_or(([200, 200, 200, 255], String::new()));
                        if !mname.is_empty() && !ob.materials.contains(&mname) {
                            ob.materials.push(mname);
                        }
                        ob.tris.push(([*a, *b, *c], col));
                    }
                }
            }
            if let Some(cs) = kid(o, "components") {
                for c in kids(cs, "component") {
                    let file = attr(c, "path").map(|p| format!("/{}", p.trim_start_matches('/'))).unwrap_or_else(|| pkey.clone());
                    ob.comps.push((format!("{file}#{}", attr(c, "objectid").unwrap_or("")), mf_transform(attr(c, "transform"))));
                }
            }
            objects.insert(format!("{pkey}#{id}"), ob);
        }
        if is_main {
            if let Some(b) = kid(&root, "build") {
                for it in kids(b, "item") {
                    let file = attr(it, "path").map(|p| format!("/{}", p.trim_start_matches('/'))).unwrap_or_else(|| pkey.clone());
                    build.push((format!("{file}#{}", attr(it, "objectid").unwrap_or("")), mf_transform(attr(it, "transform")), attr(it, "partnumber").unwrap_or("").to_string(), attr(it, "UUID").unwrap_or("").to_string()));
                }
            }
        }
    }
    progress(0.8, "Building 3MF items");
    // geometria unui obiect, cu componentele lui (recursiv, max 16 nivele)
    fn collect(objects: &HashMap<String, MfObject>, key: &str, m: DMat4, depth: usize, out: &mut Vec<([DVec3; 3], [u8; 4])>, mats: &mut Vec<String>) {
        let Some(o) = objects.get(key) else { return };
        if depth > 16 {
            return;
        }
        for (t, c) in &o.tris {
            out.push((t.map(|p| m.transform_point3(p)), *c));
        }
        for m2 in &o.materials {
            if !mats.contains(m2) {
                mats.push(m2.clone());
            }
        }
        for (k, cm) in &o.comps {
            collect(objects, k, m * *cm, depth + 1, out, mats);
        }
    }
    let scale = DMat4::from_scale(DVec3::splat(unit));
    let mut elements = Vec::new();
    for (k, (key, m, part, uuid)) in build.iter().enumerate() {
        let Some(o) = objects.get(key) else { continue };
        let mut tris = Vec::new();
        let mut mats = Vec::new();
        collect(&objects, key, scale * *m, 0, &mut tris, &mut mats);
        tris.retain(|t| valid(&t.0));
        if tris.is_empty() {
            continue;
        }
        let name = if !o.name.is_empty() { o.name.clone() } else { format!("Object {}", key.rsplit('#').next().unwrap_or("")) };
        let mut props = vec![prop("Element", "Name", name.clone()), prop("3MF", "Object type", o.ty.clone()), prop("3MF", "Item", (k + 1).to_string())];
        let pn = if !part.is_empty() { part } else { &o.part };
        if !pn.is_empty() {
            props.push(prop("3MF", "Part number", pn.clone()));
        }
        if !mats.is_empty() {
            props.push(prop("Material", "Material", mats.join(", ")));
        }
        for (n, v) in &o.meta {
            props.push(prop("3MF metadata", n, v.clone()));
        }
        for (n, v) in &model_meta {
            props.push(prop("3MF model", n, v.clone()));
        }
        let guid = if !uuid.is_empty() { uuid.clone() } else { o.uuid.clone() };
        let (t, c): (Vec<[DVec3; 3]>, Vec<[u8; 4]>) = tris.into_iter().unzip();
        elements.push(RawElement { name, ifc_type: format!("3MF {}", o.ty), guid, storey: String::new(), props, tris: t, colors: c, default_hidden: o.ty == "support" || o.ty == "solidsupport" });
    }
    if elements.is_empty() {
        return Err(format!("{}: no printable objects in the 3MF build.", crate::file_name(path)));
    }
    let mut info = vec![("Format".to_string(), "3MF".to_string()), ("Units".into(), format!("{unit} m"))];
    info.extend(model_meta.iter().filter(|(n, _)| ["Title", "Designer", "Application", "CreationDate"].contains(&n.as_str())).cloned());
    Ok(RawModel { elements, info, warnings: Vec::new() })
}

// ====================================================================== poligoane

/// Poligon 3D (contur + goluri) -> triunghiuri (earcut in planul lui, normala Newell).
pub(crate) fn poly_tris(outer: &[DVec3], holes: &[Vec<DVec3>]) -> Vec<[DVec3; 3]> {
    let mut out = Vec::new();
    if outer.len() < 3 {
        return out;
    }
    if outer.len() == 3 && holes.is_empty() {
        out.push([outer[0], outer[1], outer[2]]);
        return out;
    }
    let mut n = DVec3::ZERO;
    for i in 0..outer.len() {
        let (a, b) = (outer[i], outer[(i + 1) % outer.len()]);
        n += DVec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
    }
    if n.length_squared() < 1e-24 {
        return out;
    }
    let n = n.normalize();
    let u = if n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let u = (u - n * u.dot(n)).normalize();
    let v = n.cross(u);
    let o = outer[0];
    let mut flat = Vec::new();
    let mut all = Vec::new();
    let mut hole_idx = Vec::new();
    for p in outer {
        flat.push((*p - o).dot(u));
        flat.push((*p - o).dot(v));
        all.push(*p);
    }
    for h in holes.iter().filter(|h| h.len() >= 3) {
        hole_idx.push(all.len());
        for p in h {
            flat.push((*p - o).dot(u));
            flat.push((*p - o).dot(v));
            all.push(*p);
        }
    }
    if let Ok(idx) = earcutr::earcut(&flat, &hole_idx, 2) {
        for t in idx.chunks_exact(3) {
            out.push([all[t[0]], all[t[1]], all[t[2]]]);
        }
    }
    out
}

/// Valoare JSON -> text (obiectele imbricate se aplatizeaza "a.b").
pub(crate) fn flatten_json(prefix: &str, v: &serde_json::Value, out: &mut Vec<(String, String)>) {
    use serde_json::Value;
    if out.len() > 300 {
        return;
    }
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                flatten_json(&key, x, out);
            }
        }
        Value::Null => {}
        Value::String(s) => out.push((prefix.to_string(), s.clone())),
        Value::Array(a) if a.iter().all(|x| !x.is_object() && !x.is_array()) => {
            out.push((prefix.to_string(), a.iter().map(|x| x.as_str().map(|s| s.to_string()).unwrap_or_else(|| x.to_string())).collect::<Vec<_>>().join(", ")))
        }
        other => out.push((prefix.to_string(), other.to_string())),
    }
}

// ====================================================================== .json / .jsonl

/// Fisierele .json / .jsonl: CityJSON, CityJSONSeq, ifcJSON sau 3D Tiles (tileset.json), dupa continut.
pub fn load_json(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
    let text = String::from_utf8_lossy(&bytes);
    let mut ls = text.lines().filter(|l| !l.trim().is_empty());
    let (l1, l2) = (ls.next().unwrap_or(""), ls.next().unwrap_or(""));
    if l1.contains("\"CityJSON\"") && l2.contains("CityJSONFeature") {
        return load_cityjson_seq(&bytes, path, progress);
    }
    let v: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| format!("Invalid JSON: {e}"))?;
    if v.get("type").and_then(|t| t.as_str()) == Some("CityJSON") {
        return load_cityjson(&v, path, progress);
    }
    if v.get("asset").is_some() && v.get("root").is_some() {
        return load_3dtiles(path, &v, progress);
    }
    let is_ifc = v.get("type").and_then(|t| t.as_str()).map_or(false, |t| t.eq_ignore_ascii_case("ifcJSON"))
        || v.get("data").and_then(|d| d.as_array()).map_or(false, |a| a.iter().any(|x| x.get("type").and_then(|t| t.as_str()).map_or(false, |t| t.starts_with("Ifc"))));
    if is_ifc {
        return load_ifcjson(&v, path, progress);
    }
    Err(format!("{}: JSON file not recognised (expected CityJSON, CityJSONSeq, ifcJSON or a 3D Tiles tileset.json).", crate::file_name(path)))
}

// ====================================================================== CityJSON

fn city_color(obj_type: &str, surface: Option<&str>) -> [u8; 4] {
    match surface {
        Some("RoofSurface") => return [178, 84, 64, 255],
        Some("WallSurface") | Some("OuterWallSurface") => return [228, 222, 208, 255],
        Some("GroundSurface") | Some("OuterFloorSurface") => return [140, 140, 140, 255],
        Some("Window") => return [120, 170, 220, 150],
        Some("Door") => return [130, 90, 60, 255],
        Some("ClosureSurface") => return [200, 200, 200, 90],
        Some("TrafficArea") => return [110, 110, 115, 255],
        Some("AuxiliaryTrafficArea") => return [150, 150, 140, 255],
        Some("WaterSurface") => return [70, 130, 200, 220],
        _ => {}
    }
    match obj_type {
        t if t.starts_with("Building") => [225, 215, 195, 255],
        "TINRelief" => [150, 140, 105, 255],
        "WaterBody" => [70, 130, 200, 220],
        "PlantCover" | "SolitaryVegetationObject" => [90, 150, 70, 255],
        "LandUse" => [170, 190, 130, 255],
        "Road" | "Railway" | "TransportSquare" | "Waterway" => [120, 120, 125, 255],
        t if t.starts_with("Bridge") => [180, 175, 165, 255],
        t if t.starts_with("Tunnel") => [150, 145, 140, 255],
        _ => [200, 200, 200, 255],
    }
}

struct CityCtx<'a> {
    verts: Vec<DVec3>,
    templates: Option<(&'a serde_json::Value, Vec<DVec3>)>,
    materials: Vec<(String, [u8; 4])>,
}

type ColTri = ([DVec3; 3], [u8; 4]);

/// o suprafata CityJSON: [contur, gol, gol...] -> triunghiuri cu culoarea semanticii / materialului
fn city_surface(ctx: &CityCtx, s: &serde_json::Value, stype: Option<&str>, mat: Option<usize>, obj_type: &str, out: &mut Vec<ColTri>) {
    let Some(rings) = s.as_array() else { return };
    let ring = |r: &serde_json::Value| -> Vec<DVec3> { r.as_array().map(|a| a.iter().filter_map(|i| i.as_u64()).filter_map(|i| ctx.verts.get(i as usize).copied()).collect()).unwrap_or_default() };
    let Some(first) = rings.first() else { return };
    let outer = ring(first);
    let holes: Vec<Vec<DVec3>> = rings.iter().skip(1).map(ring).collect();
    let col = mat.and_then(|i| ctx.materials.get(i)).map(|m| m.1).unwrap_or_else(|| city_color(obj_type, stype));
    for t in poly_tris(&outer, &holes) {
        if valid(&t) {
            out.push((t, col));
        }
    }
}

/// boundaries -> triunghiuri colorate (semantica / materialul suprafetei, daca exista)
fn city_geometry(ctx: &CityCtx, g: &serde_json::Value, obj_type: &str, out: &mut Vec<ColTri>, sem_names: &mut Vec<String>) {
    let ty = g.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if ty == "GeometryInstance" {
        // sablon: varfurile sablonului transformate cu matricea + punctul de referinta
        let Some((tpl, tv)) = &ctx.templates else { return };
        let Some(t) = g.get("template").and_then(|t| t.as_u64()).and_then(|i| tpl.get("templates")?.get(i as usize)) else { return };
        let m: Vec<f64> = g.get("transformationMatrix").and_then(|m| m.as_array()).map(|a| a.iter().filter_map(|x| x.as_f64()).collect()).unwrap_or_default();
        let refp = g.get("boundaries").and_then(|b| b.get(0)).and_then(|i| i.as_u64()).and_then(|i| ctx.verts.get(i as usize)).copied().unwrap_or(DVec3::ZERO);
        let mat = if m.len() == 16 { DMat4::from_cols_array(&m[..16].try_into().unwrap()).transpose() } else { DMat4::IDENTITY };
        let sub = CityCtx { verts: tv.iter().map(|p| mat.transform_point3(*p) + refp).collect(), templates: None, materials: ctx.materials.clone() };
        city_geometry(&sub, t, obj_type, out, sem_names);
        return;
    }
    let b = g.get("boundaries");
    let sem = g.get("semantics");
    let surfaces = sem.and_then(|s| s.get("surfaces")).and_then(|s| s.as_array());
    let sem_vals = sem.and_then(|s| s.get("values"));
    let mat_vals = g.get("material").and_then(|m| m.as_object()).and_then(|m| m.values().next()).and_then(|m| m.get("values").or_else(|| m.get("value")));
    let mut stype_of = |v: Option<&serde_json::Value>| -> Option<String> {
        let st = v.and_then(|v| v.as_u64()).and_then(|i| surfaces.and_then(|s| s.get(i as usize))).and_then(|s| s.get("type")).and_then(|t| t.as_str())?.to_string();
        if !sem_names.contains(&st) {
            sem_names.push(st.clone());
        }
        Some(st)
    };
    fn at(v: Option<&serde_json::Value>, i: usize) -> Option<&serde_json::Value> {
        v.and_then(|x| x.get(i))
    }
    // valoarea materialului poate fi un singur numar pentru toata geometria
    let mat_of = |v: Option<&serde_json::Value>| v.and_then(|x| x.as_u64()).or_else(|| mat_vals.and_then(|m| m.as_u64())).map(|x| x as usize);
    let list = |v: Option<&serde_json::Value>| v.and_then(|x| x.as_array()).cloned().unwrap_or_default();
    match ty {
        "MultiSurface" | "CompositeSurface" => {
            for (i, s) in list(b).iter().enumerate() {
                let st = stype_of(at(sem_vals, i));
                city_surface(ctx, s, st.as_deref(), mat_of(at(mat_vals, i)), obj_type, out);
            }
        }
        "Solid" => {
            for (si, shell) in list(b).iter().enumerate() {
                for (i, s) in list(Some(shell)).iter().enumerate() {
                    let st = stype_of(at(at(sem_vals, si), i));
                    city_surface(ctx, s, st.as_deref(), mat_of(at(at(mat_vals, si), i)), obj_type, out);
                }
            }
        }
        "MultiSolid" | "CompositeSolid" => {
            for (di, solid) in list(b).iter().enumerate() {
                for (si, shell) in list(Some(solid)).iter().enumerate() {
                    for (i, s) in list(Some(shell)).iter().enumerate() {
                        let st = stype_of(at(at(at(sem_vals, di), si), i));
                        city_surface(ctx, s, st.as_deref(), mat_of(at(at(at(mat_vals, di), si), i)), obj_type, out);
                    }
                }
            }
        }
        _ => {}
    }
}

fn city_verts(v: &serde_json::Value, tr: Option<&serde_json::Value>) -> Vec<DVec3> {
    let (s, t) = match tr {
        Some(tr) => {
            let a = |k: &str, d: f64| -> DVec3 {
                let x: Vec<f64> = tr.get(k).and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_f64()).collect()).unwrap_or_default();
                if x.len() == 3 { DVec3::new(x[0], x[1], x[2]) } else { DVec3::splat(d) }
            };
            (a("scale", 1.0), a("translate", 0.0))
        }
        None => (DVec3::ONE, DVec3::ZERO),
    };
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|p| {
                    let c: Vec<f64> = p.as_array().map(|x| x.iter().filter_map(|x| x.as_f64()).collect()).unwrap_or_default();
                    if c.len() >= 3 { DVec3::new(c[0], c[1], c[2]) * s + t } else { DVec3::ZERO }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Obiectele unui document (sau feature) CityJSON -> elemente.
fn city_objects(objs: &serde_json::Map<String, serde_json::Value>, ctx: &CityCtx, out: &mut Vec<RawElement>) {
    // radacina fiecarui obiect (Building pentru BuildingPart / BuildingInstallation...) = "nivelul" din arbore
    let root_of = |id: &str| -> String {
        let mut cur = id.to_string();
        for _ in 0..8 {
            match objs.get(&cur).and_then(|o| o.get("parents")).and_then(|p| p.get(0)).and_then(|p| p.as_str()) {
                Some(p) => cur = p.to_string(),
                None => break,
            }
        }
        cur
    };
    let lod = |g: &serde_json::Value| g.get("lod").map(|l| l.as_str().map(|s| s.to_string()).unwrap_or_else(|| l.to_string())).unwrap_or_default();
    for (id, o) in objs {
        let ty = o.get("type").and_then(|t| t.as_str()).unwrap_or("CityObject");
        let geoms = o.get("geometry").and_then(|g| g.as_array()).cloned().unwrap_or_default();
        // doar cel mai detaliat LOD (altfel acelasi obiect apare de doua ori)
        let best = geoms.iter().map(lod).max_by(|a, b| a.parse::<f64>().unwrap_or(0.0).total_cmp(&b.parse::<f64>().unwrap_or(0.0))).unwrap_or_default();
        let mut tris = Vec::new();
        let mut sem = Vec::new();
        for g in geoms.iter().filter(|g| lod(g) == best) {
            city_geometry(ctx, g, ty, &mut tris, &mut sem);
        }
        if tris.is_empty() {
            continue;
        }
        let mut attrs = Vec::new();
        if let Some(a) = o.get("attributes") {
            flatten_json("", a, &mut attrs);
        }
        let name = attrs.iter().find(|(k, _)| ["name", "Name", "naam", "bezeichnung"].contains(&k.as_str())).map(|(_, v)| format!("{ty} {v}")).unwrap_or_else(|| format!("{ty} {id}"));
        let mut props = vec![prop("Element", "Name", name.clone()), prop("CityJSON", "Type", ty), prop("CityJSON", "Id", id.clone()), prop("CityJSON", "LoD", best.clone())];
        if !sem.is_empty() {
            props.push(prop("CityJSON", "Surfaces", sem.join(", ")));
        }
        for (k, label) in [("parents", "Parent"), ("children", "Children")] {
            if let Some(a) = o.get(k).and_then(|a| a.as_array()) {
                props.push(prop("CityJSON", label, a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", ")));
            }
        }
        for (k, v) in attrs {
            props.push(prop("Attributes", &k, v));
        }
        let (t, c): (Vec<[DVec3; 3]>, Vec<[u8; 4]>) = tris.into_iter().unzip();
        // radacina (cladirea unei parti) ca proprietate, nu ca nivel: altfel fiecare cladire devine un "nivel"
        let root = root_of(id);
        if root != *id {
            props.push(prop("CityJSON", "Root object", root));
        }
        out.push(RawElement { name, ifc_type: ty.to_string(), guid: id.clone(), storey: String::new(), props, tris: t, colors: c, default_hidden: false });
    }
}

fn city_materials(doc: &serde_json::Value) -> Vec<(String, [u8; 4])> {
    doc.get("appearance")
        .and_then(|a| a.get("materials"))
        .and_then(|m| m.as_array())
        .map(|a| {
            a.iter()
                .map(|m| {
                    let c: Vec<f64> = m.get("diffuseColor").and_then(|c| c.as_array()).map(|x| x.iter().filter_map(|v| v.as_f64()).collect()).unwrap_or_default();
                    let tr = m.get("transparency").and_then(|t| t.as_f64()).unwrap_or(0.0);
                    let col = if c.len() == 3 { [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, ((1.0 - tr) * 255.0).max(40.0) as u8] } else { [200, 200, 200, 255] };
                    (m.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(), col)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn city_info(doc: &serde_json::Value, fmt: &str) -> Vec<(String, String)> {
    let mut info = vec![("Format".to_string(), format!("{fmt} {}", doc.get("version").and_then(|v| v.as_str()).unwrap_or("")))];
    if let Some(m) = doc.get("metadata") {
        let mut kv = Vec::new();
        flatten_json("", m, &mut kv);
        for (k, v) in kv.into_iter().take(20) {
            info.push((format!("Metadata {k}"), v));
        }
    }
    info
}

pub fn load_cityjson(doc: &serde_json::Value, path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    progress(0.2, "Reading CityJSON");
    let tv = doc.get("geometry-templates").map(|t| city_verts(t.get("vertices-templates").unwrap_or(&serde_json::Value::Null), None));
    let ctx = CityCtx { verts: city_verts(doc.get("vertices").unwrap_or(&serde_json::Value::Null), doc.get("transform")), templates: doc.get("geometry-templates").zip(tv), materials: city_materials(doc) };
    let mut els = Vec::new();
    if let Some(objs) = doc.get("CityObjects").and_then(|o| o.as_object()) {
        progress(0.5, "Triangulating city objects");
        city_objects(objs, &ctx, &mut els);
    }
    if els.is_empty() {
        return Err(format!("{}: no city objects with geometry.", crate::file_name(path)));
    }
    Ok(RawModel { elements: els, info: city_info(doc, "CityJSON"), warnings: Vec::new() })
}

/// CityJSONSeq (JSON Lines): prima linie = antetul (transform, metadate), apoi cate un CityJSONFeature pe linie,
/// fiecare cu varfurile lui.
pub fn load_cityjson_seq(bytes: &[u8], path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    let text = String::from_utf8_lossy(bytes);
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let head: serde_json::Value = serde_json::from_str(lines.next().unwrap_or("{}")).map_err(|e| format!("CityJSONSeq header: {e}"))?;
    let tr = head.get("transform").cloned();
    let tv = head.get("geometry-templates").map(|t| city_verts(t.get("vertices-templates").unwrap_or(&serde_json::Value::Null), None));
    let mats = city_materials(&head);
    let mut els = Vec::new();
    let mut warnings = Vec::new();
    let total = text.lines().count().max(1);
    for (i, l) in lines.enumerate() {
        if i % 500 == 0 {
            progress(0.1 + 0.8 * i as f32 / total as f32, "Reading CityJSON features");
        }
        let f: serde_json::Value = match serde_json::from_str(l) {
            Ok(v) => v,
            Err(e) => {
                warnings.push(format!("Line {}: {e}", i + 2));
                continue;
            }
        };
        let ctx = CityCtx { verts: city_verts(f.get("vertices").unwrap_or(&serde_json::Value::Null), tr.as_ref()), templates: head.get("geometry-templates").zip(tv.clone()), materials: mats.clone() };
        if let Some(objs) = f.get("CityObjects").and_then(|o| o.as_object()) {
            city_objects(objs, &ctx, &mut els);
        }
    }
    if els.is_empty() {
        return Err(format!("{}: no city objects with geometry.", crate::file_name(path)));
    }
    warnings.truncate(20);
    Ok(RawModel { elements: els, info: city_info(&head, "CityJSONSeq"), warnings })
}

// ====================================================================== 3D Tiles

struct TilesCtx<'a> {
    elements: Vec<RawElement>,
    warnings: Vec<String>,
    contents: usize,
    tris: usize,
    progress: &'a dyn Fn(f32, &str),
}

const MAX_CONTENTS: usize = 4000;
const MAX_TRIS: usize = 25_000_000;

fn json_mat(v: Option<&serde_json::Value>) -> DMat4 {
    let a: Vec<f64> = v.and_then(|m| m.as_array()).map(|a| a.iter().filter_map(|x| x.as_f64()).collect()).unwrap_or_default();
    if a.len() == 16 {
        DMat4::from_cols_array(&a[..16].try_into().unwrap())
    } else {
        DMat4::IDENTITY
    }
}

/// Continutul unei dale: .glb / .gltf (1.1), .b3dm (1.0), .cmpt (compus), tileset extern (.json).
fn tile_content(ctx: &mut TilesCtx, bytes: &[u8], path: &str, m: DMat4, label: &str, depth: usize) {
    if bytes.len() < 4 || depth > 8 {
        return;
    }
    let magic = &bytes[..4];
    let u32_at = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize).unwrap_or(0);
    let model = match magic {
        b"glTF" => crate::gltf_load::load_gltf_bytes(bytes, path, &|_, _| {}, None),
        b"b3dm" => {
            // antet 28 octeti: tabelul de obiecte (JSON + binar), tabelul de lot (JSON + binar), apoi GLB
            let (ftj, ftb, btj, btb) = (u32_at(12), u32_at(16), u32_at(20), u32_at(24));
            let mut o = 28;
            let ft: serde_json::Value = bytes.get(o..o + ftj).and_then(|b| serde_json::from_slice(b).ok()).unwrap_or_default();
            o += ftj + ftb;
            let bt: serde_json::Value = bytes.get(o..o + btj).and_then(|b| serde_json::from_slice(b).ok()).unwrap_or_default();
            let bt_bin = bytes.get(o + btj..o + btj + btb).unwrap_or(&[]);
            o += btj + btb;
            let n = ft.get("BATCH_LENGTH").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let rows = (n > 0 && btj > 0).then(|| crate::gltf_meta::batch_table_rows(&bt, bt_bin, n));
            // RTC_CENTER: deplasarea dalei (dupa conversia Y-sus -> Z-sus a glTF-ului)
            let rtc: Vec<f64> = ft.get("RTC_CENTER").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_f64()).collect()).unwrap_or_default();
            let r = crate::gltf_load::load_gltf_bytes(bytes.get(o..).unwrap_or(&[]), path, &|_, _| {}, rows);
            if rtc.len() == 3 {
                let t = DVec3::new(rtc[0], rtc[1], rtc[2]);
                r.map(|mut mm| {
                    for e in &mut mm.elements {
                        for tri in &mut e.tris {
                            for p in tri.iter_mut() {
                                *p += t;
                            }
                        }
                    }
                    mm
                })
            } else {
                r
            }
        }
        b"cmpt" => {
            let n = u32_at(12);
            let mut o = 16;
            for _ in 0..n {
                let len = u32_at(o + 8);
                if len == 0 || o + len > bytes.len() {
                    break;
                }
                tile_content(ctx, &bytes[o..o + len], path, m, label, depth + 1);
                o += len;
            }
            return;
        }
        b"i3dm" | b"pnts" => {
            ctx.warnings.push(format!("{} content (instanced models / point clouds in 3D Tiles) is not read yet.", String::from_utf8_lossy(magic)));
            return;
        }
        _ => {
            // tileset extern sau .gltf text
            match serde_json::from_slice::<serde_json::Value>(bytes) {
                Ok(v) if v.get("root").is_some() => {
                    let dir = Path::new(path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
                    tile_walk(ctx, &v["root"], m, &dir, "REPLACE", depth + 1);
                    return;
                }
                Ok(_) => crate::gltf_load::load_gltf_bytes(bytes, path, &|_, _| {}, None),
                Err(_) => return,
            }
        }
    };
    match model {
        Ok(mm) => {
            for mut e in mm.elements {
                for tri in &mut e.tris {
                    for p in tri.iter_mut() {
                        *p = m.transform_point3(*p);
                    }
                }
                ctx.tris += e.tris.len();
                if e.storey.is_empty() {
                    e.storey = label.to_string();
                }
                e.props.push(prop("3D Tiles", "Tile content", label));
                ctx.elements.push(e);
            }
        }
        Err(e) => ctx.warnings.push(format!("{label}: {e}")),
    }
}

/// Parcurge arborele de dale: REPLACE = doar frunzele (copiii inlocuiesc parintele), ADD = toate.
fn tile_walk(ctx: &mut TilesCtx, tile: &serde_json::Value, parent: DMat4, dir: &Path, inherited: &str, depth: usize) {
    if depth > 64 || ctx.contents >= MAX_CONTENTS || ctx.tris >= MAX_TRIS {
        return;
    }
    let m = parent * json_mat(tile.get("transform"));
    let refine = tile.get("refine").and_then(|r| r.as_str()).unwrap_or(inherited).to_ascii_uppercase();
    let children = tile.get("children").and_then(|c| c.as_array()).cloned().unwrap_or_default();
    let use_own = refine == "ADD" || children.is_empty();
    if use_own {
        let mut uris: Vec<String> = Vec::new();
        for c in tile.get("content").into_iter().chain(tile.get("contents").and_then(|c| c.as_array()).into_iter().flatten()) {
            if let Some(u) = c.get("uri").or_else(|| c.get("url")).and_then(|u| u.as_str()) {
                uris.push(u.to_string());
            }
        }
        for u in uris {
            if u.contains("{") {
                ctx.warnings.push("Implicit tiling (template URIs) is not supported yet.".into());
                continue;
            }
            let p = dir.join(u.split(['?', '#']).next().unwrap_or(&u).replace('/', std::path::MAIN_SEPARATOR_STR));
            ctx.contents += 1;
            if ctx.contents % 25 == 0 {
                (ctx.progress)(0.1 + 0.8 * (ctx.contents as f32 / MAX_CONTENTS as f32).min(1.0), "Reading 3D Tiles");
            }
            match std::fs::read(&p) {
                Ok(b) => {
                    let label = p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                    tile_content(ctx, &b, &p.to_string_lossy(), m, &label, depth);
                }
                Err(e) => ctx.warnings.push(format!("{}: {e}", p.display())),
            }
        }
    }
    for c in &children {
        tile_walk(ctx, c, m, dir, &refine, depth + 1);
    }
}

/// WGS84: ECEF -> (latitudine, longitudine, inaltime) geodezice
fn ecef_to_geodetic(p: DVec3) -> (f64, f64, f64) {
    let a = 6_378_137.0;
    let e2 = 6.694_379_990_14e-3;
    let lon = p.y.atan2(p.x);
    let r = (p.x * p.x + p.y * p.y).sqrt();
    let mut lat = p.z.atan2(r * (1.0 - e2));
    let mut h = 0.0;
    for _ in 0..6 {
        let n = a / (1.0 - e2 * lat.sin().powi(2)).sqrt();
        h = r / lat.cos() - n;
        lat = p.z.atan2(r * (1.0 - e2 * n / (n + h)));
    }
    (lat, lon, h)
}

pub fn load_3dtiles(path: &str, ts: &serde_json::Value, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    progress(0.05, "Reading 3D Tiles tileset");
    let dir = Path::new(path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let mut ctx = TilesCtx { elements: Vec::new(), warnings: Vec::new(), contents: 0, tris: 0, progress };
    tile_walk(&mut ctx, &ts["root"], DMat4::IDENTITY, &dir, "REPLACE", 0);
    if ctx.contents >= MAX_CONTENTS || ctx.tris >= MAX_TRIS {
        ctx.warnings.push(format!("Very large tileset: stopped after {} tiles / {} triangles.", ctx.contents, ctx.tris));
    }
    if ctx.elements.is_empty() {
        let mut w = ctx.warnings.clone();
        w.truncate(5);
        return Err(format!("{}: no geometry in the tileset. {}", crate::file_name(path), w.join(" ")));
    }
    let mut info = vec![
        ("Format".to_string(), format!("3D Tiles {}", ts.get("asset").and_then(|a| a.get("version")).and_then(|v| v.as_str()).unwrap_or(""))),
        ("Tiles read".into(), ctx.contents.to_string()),
    ];
    // ECEF (coordonate geocentrice): mutat intr-un sistem local Est-Nord-Sus, ca modelul sa stea drept
    let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for e in &ctx.elements {
        for t in &e.tris {
            for p in t {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
        }
    }
    let c = (lo + hi) * 0.5;
    if (6.0e6..6.6e6).contains(&c.length()) {
        let (lat, lon, h) = ecef_to_geodetic(c);
        let east = DVec3::new(-lon.sin(), lon.cos(), 0.0);
        let north = DVec3::new(-lat.sin() * lon.cos(), -lat.sin() * lon.sin(), lat.cos());
        let up = DVec3::new(lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin());
        for e in &mut ctx.elements {
            for t in &mut e.tris {
                for p in t.iter_mut() {
                    let d = *p - c;
                    *p = DVec3::new(d.dot(east), d.dot(north), d.dot(up) + h);
                }
            }
        }
        info.push(("Georeference".into(), format!("WGS84 {:.7}°, {:.7}°, {:.2} m (shown in a local East-North-Up frame)", lat.to_degrees(), lon.to_degrees(), h)));
    }
    ctx.warnings.sort();
    ctx.warnings.dedup();
    ctx.warnings.truncate(30);
    Ok(RawModel { elements: ctx.elements, info, warnings: ctx.warnings })
}

/// ifcJSON (completat mai jos)
pub fn load_ifcjson(v: &serde_json::Value, _path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    crate::ifc_convert::load_ifcjson(v, progress)
}

/// O singura dala 3D Tiles 1.0 (.b3dm / .cmpt) deschisa direct.
pub fn load_tile_file(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
    let mut ctx = TilesCtx { elements: Vec::new(), warnings: Vec::new(), contents: 1, tris: 0, progress };
    let label = crate::file_name(path);
    tile_content(&mut ctx, &bytes, path, DMat4::IDENTITY, &label, 0);
    if ctx.elements.is_empty() {
        return Err(format!("{label}: no geometry. {}", ctx.warnings.join(" ")));
    }
    Ok(RawModel { elements: ctx.elements, info: vec![("Format".into(), "3D Tiles content".into())], warnings: ctx.warnings })
}
