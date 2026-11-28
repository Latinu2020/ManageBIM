//! IFC loader (IFC2x3 / IFC4): geometry, colours, properties, spatial structure.
use crate::model::{Prop, RawElement, RawModel};
use crate::step::{StepFile, V};
use glam::{DMat4, DQuat, DVec2, DVec3, DVec4};
use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::f64::consts::PI;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

type Col = Option<[u8; 4]>;
type Tri = ([DVec3; 3], Col);
type Profile = (Vec<DVec2>, Vec<Vec<DVec2>>);

pub fn load_ifc(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    progress(0.02, "Reading file");
    let data = std::fs::read(path).map_err(|e| format!("Cannot read the file: {e}"))?;
    load_ifc_bytes(data, progress)
}

pub fn load_ifc_bytes(data: Vec<u8>, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    progress(0.05, "Parsing STEP");
    let f = StepFile::parse(data)?;
    let mut c = Ctx {
        f: &f,
        placements: HashMap::new(),
        styles: Cow::Owned(HashMap::new()),
        rep_cache: HashMap::new(),
        angle: 1.0,
        qscale: [1.0, 1.0, 1.0],
        warn: BTreeSet::new(),
    };
    let scale = c.units();
    progress(0.15, "Styles and relationships");
    c.collect_styles();

    // --- relations
    let mut parent: HashMap<u32, u32> = HashMap::new();
    for &r in f.of_type("IFCRELAGGREGATES").iter().chain(f.of_type("IFCRELNESTS")) {
        let a = f.args(r);
        if let (Some(p), Some(ch)) = (a.get(4).and_then(|v| v.as_ref()), a.get(5)) {
            for k in ch.as_list() {
                if let Some(k) = k.as_ref() {
                    parent.insert(k, p);
                }
            }
        }
    }
    for &r in f.of_type("IFCRELCONTAINEDINSPATIALSTRUCTURE") {
        let a = f.args(r);
        if let (Some(ch), Some(p)) = (a.get(4), a.get(5).and_then(|v| v.as_ref())) {
            for k in ch.as_list() {
                if let Some(k) = k.as_ref() {
                    parent.insert(k, p);
                }
            }
        }
    }
    // openings/fillings: window -> opening -> wall (for the level)
    for &r in f.of_type("IFCRELFILLSELEMENT") {
        let a = f.args(r);
        if let (Some(o), Some(e)) = (a.get(4).and_then(|v| v.as_ref()), a.get(5).and_then(|v| v.as_ref())) {
            parent.entry(e).or_insert(o);
        }
    }
    // systems / zones / groups (IfcRelAssignsToGroup): element -> (kind, name)
    let mut groups: HashMap<u32, Vec<(String, String)>> = HashMap::new();
    for &r in f.of_type("IFCRELASSIGNSTOGROUP").iter().chain(f.of_type("IFCRELASSIGNSTOGROUPBYFACTOR").iter()) {
        let a = f.args(r);
        let Some(g) = a.get(6).and_then(|v| v.as_ref()) else { continue };
        let gname = f.args(g).get(2).and_then(|v| v.as_str()).unwrap_or("").to_string();
        if gname.is_empty() {
            continue;
        }
        let lt = f.ty(g).to_ascii_lowercase();
        let gty = if lt.contains("system") { "System" } else if lt.contains("zone") { "Zone" } else { "Group" }.to_string();
        for o in a.get(4).map(|v| v.as_list()).unwrap_or(&[]) {
            if let Some(o) = o.as_ref() {
                groups.entry(o).or_default().push((gty.clone(), gname.clone()));
            }
        }
    }
    // classifications (Uniclass, OmniClass...): element -> (system, code, name)
    let mut classes: HashMap<u32, Vec<(String, String, String)>> = HashMap::new();
    for &r in f.of_type("IFCRELASSOCIATESCLASSIFICATION") {
        let a = f.args(r);
        let Some(cr) = a.get(5).and_then(|v| v.as_ref()) else { continue };
        if f.ty(cr) != "IFCCLASSIFICATIONREFERENCE" {
            continue;
        }
        let ca = f.args(cr);
        let code = ca.get(1).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let cname = ca.get(2).and_then(|v| v.as_str()).unwrap_or("").to_string();
        // the system: we climb through ReferencedSource up to IfcClassification
        let mut system = String::new();
        let mut src = ca.get(3).and_then(|v| v.as_ref());
        for _ in 0..8 {
            let Some(s) = src else { break };
            let sa = f.args(s);
            if f.ty(s) == "IFCCLASSIFICATION" {
                system = sa.get(3).and_then(|v| v.as_str()).unwrap_or("").to_string();
                break;
            }
            src = sa.get(3).and_then(|v| v.as_ref());
        }
        if system.is_empty() {
            system = "Classification".into();
        }
        for o in a.get(4).map(|v| v.as_list()).unwrap_or(&[]) {
            if let Some(o) = o.as_ref() {
                classes.entry(o).or_default().push((system.clone(), code.clone(), cname.clone()));
            }
        }
    }
    // the openings of each host element (walls, slabs...): subtracted from its geometry
    let mut voids: HashMap<u32, Vec<u32>> = HashMap::new();
    for &r in f.of_type("IFCRELVOIDSELEMENT") {
        let a = f.args(r);
        if let (Some(e), Some(o)) = (a.get(4).and_then(|v| v.as_ref()), a.get(5).and_then(|v| v.as_ref())) {
            parent.entry(o).or_insert(e);
            voids.entry(e).or_default().push(o);
        }
    }
    let mut psets: HashMap<u32, Vec<u32>> = HashMap::new();
    for &r in f.of_type("IFCRELDEFINESBYPROPERTIES") {
        let a = f.args(r);
        if let (Some(objs), Some(pd)) = (a.get(4), a.get(5).and_then(|v| v.as_ref())) {
            for o in objs.as_list() {
                if let Some(o) = o.as_ref() {
                    psets.entry(o).or_default().push(pd);
                }
            }
        }
    }
    let mut types: HashMap<u32, u32> = HashMap::new();
    for &r in f.of_type("IFCRELDEFINESBYTYPE") {
        let a = f.args(r);
        if let (Some(objs), Some(t)) = (a.get(4), a.get(5).and_then(|v| v.as_ref())) {
            for o in objs.as_list() {
                if let Some(o) = o.as_ref() {
                    types.insert(o, t);
                }
            }
        }
    }
    let mut materials: HashMap<u32, u32> = HashMap::new();
    for &r in f.of_type("IFCRELASSOCIATESMATERIAL") {
        let a = f.args(r);
        if let (Some(objs), Some(m)) = (a.get(4), a.get(5).and_then(|v| v.as_ref())) {
            for o in objs.as_list() {
                if let Some(o) = o.as_ref() {
                    materials.insert(o, m);
                }
            }
        }
    }
    let mut mat_colors: HashMap<u32, [u8; 4]> = HashMap::new();
    for &r in f.of_type("IFCMATERIALDEFINITIONREPRESENTATION") {
        let a = f.args(r);
        let Some(mat) = a.get(3).and_then(|v| v.as_ref()) else { continue };
        for rep in a.get(2).map(|v| v.as_list()).unwrap_or(&[]) {
            let Some(rep) = rep.as_ref() else { continue };
            let ra = f.args(rep);
            for it in ra.get(3).map(|v| v.as_list()).unwrap_or(&[]) {
                if let Some(it) = it.as_ref() {
                    let sa = f.args(it);
                    if let Some(col) = sa.get(1).and_then(|s| c.style_color(s, 0)) {
                        mat_colors.insert(mat, col);
                    }
                }
            }
        }
    }

    // --- product types
    let mut product_ids: Vec<u32> = Vec::new();
    for (ti, ids) in f.by_type.iter().enumerate() {
        let tn = &f.type_names[ti];
        if tn == "IFCOPENINGELEMENT" || tn == "IFCOPENINGSTANDARDCASE" || tn == "IFCVIRTUALELEMENT" || tn == "IFCGRID" {
            continue;
        }
        let mut is_product = false;
        for &id in ids.iter().take(8) {
            let a = f.args(id);
            if a.len() >= 7 && matches!(a[0], V::Str(_)) {
                if let Some(r) = a[6].as_ref() {
                    if f.ty(r) == "IFCPRODUCTDEFINITIONSHAPE" {
                        is_product = true;
                        break;
                    }
                }
            }
        }
        if is_product {
            product_ids.extend_from_slice(ids);
        }
    }
    product_ids.sort_unstable();

    let scale_m = DMat4::from_scale(DVec3::splat(scale));
    // --- geometry in parallel: ordered chunks of elements, each with its own Ctx (local caches),
    //     the results are joined in the same order -> the element indices stay identical to the serial version
    let styles = std::mem::take(c.styles.to_mut());
    let sh = Shared { parent: &parent, psets: &psets, types: &types, materials: &materials, mat_colors: &mat_colors, voids: &voids, groups: &groups, classes: &classes, scale, scale_m };
    let total = product_ids.len().max(1);
    let done = AtomicUsize::new(0);
    let chunk = (product_ids.len() / (rayon::current_num_threads() * 8).max(1)).max(16);
    let angle = c.angle;
    let qscale = c.qscale;
    let parts: Vec<(Vec<RawElement>, BTreeSet<String>)> = std::thread::scope(|s| {
        let h = s.spawn(|| {
            use rayon::prelude::*;
            product_ids
                .par_chunks(chunk)
                .map(|ids| {
                    let mut wc = Ctx { f: &f, placements: HashMap::new(), styles: Cow::Borrowed(&styles), rep_cache: HashMap::new(), angle, qscale, warn: BTreeSet::new() };
                    let mut out = Vec::with_capacity(ids.len());
                    for &id in ids {
                        if let Some(e) = make_element(&mut wc, &sh, id) {
                            out.push(e);
                        }
                        done.fetch_add(1, Ordering::Relaxed);
                    }
                    (out, wc.warn)
                })
                .collect()
        });
        // progress is reported from the current thread (the callback is not Sync)
        while !h.is_finished() {
            progress(0.2 + 0.78 * done.load(Ordering::Relaxed) as f32 / total as f32, "Generating geometry");
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        h.join().map_err(|_| "Geometry generation failed (internal error).".to_string())
    })?;
    let mut elements = Vec::with_capacity(product_ids.len());
    for (els, w) in parts {
        elements.extend(els);
        c.warn.extend(w);
    }
    progress(0.99, "Finishing");

    let mut info = vec![("Schema".to_string(), f.schema.clone())];
    if let Some(&p) = f.of_type("IFCPROJECT").first() {
        let a = f.args(p);
        info.push(("Project".into(), a.get(2).and_then(|v| v.as_str()).unwrap_or("").to_string()));
    }
    if let Some(&p) = f.of_type("IFCAPPLICATION").first() {
        let a = f.args(p);
        info.push(("Source application".into(), a.get(2).and_then(|v| v.as_str()).unwrap_or("").to_string()));
    }
    info.push(("STEP entities".into(), f.ents.len().to_string()));
    // IFC4 georeferencing (IfcMapConversion): the project coordinates in the map coordinate system
    if let Some(&mc) = f.of_type("IFCMAPCONVERSION").first() {
        let a = f.args(mc);
        let num = |i: usize| a.get(i).and_then(|v| v.as_f64());
        let (e, n, h) = (num(2).unwrap_or(0.0), num(3).unwrap_or(0.0), num(4).unwrap_or(0.0));
        let rot = match (num(5), num(6)) {
            (Some(x), Some(y)) => y.atan2(x).to_degrees(),
            _ => 0.0,
        };
        let crs = a.get(1).and_then(|v| v.as_ref()).and_then(|c| f.args(c).first().and_then(|v| v.as_str()).map(|s| s.to_string())).unwrap_or_default();
        info.push(("Georeference".into(), format!("E {e:.3}  N {n:.3}  H {h:.3}  rotation {rot:.4}°{}", if crs.is_empty() { String::new() } else { format!("  ({crs})") })));
    }
    let n_sys: usize = groups.values().map(|v| v.len()).sum();
    if n_sys > 0 {
        info.push(("Systems / groups".into(), format!("{} assignments", n_sys)));
    }
    if !classes.is_empty() {
        info.push(("Classified elements".into(), classes.len().to_string()));
    }
    // grids (IfcGrid) and levels (IfcBuildingStorey) in real coordinates (metres): 3D display and clash location.
    // Kept in `info` (keys with "~" are not displayed) so that they also reach the .bimc cache.
    let mut grids: Vec<(String, [f64; 3], [f64; 3])> = Vec::new();
    for &g in f.of_type("IFCGRID") {
        let a = f.args(g);
        let place = a.get(5).and_then(|v| v.as_ref()).map(|p| c.placement(p, 0)).unwrap_or(DMat4::IDENTITY);
        let xf = scale_m * place;
        for k in 7..=9 {
            let axes: Vec<u32> = a.get(k).map(|v| v.as_list().iter().filter_map(|x| x.as_ref()).collect()).unwrap_or_default();
            for ax in axes {
                let aa = f.args(ax);
                let tag = aa.first().and_then(|v| v.as_str()).unwrap_or("").to_string();
                let Some(cv) = aa.get(1).and_then(|v| v.as_ref()) else { continue };
                let pts = c.curve(cv, 0);
                if let (Some(p0), Some(p1)) = (pts.first(), pts.last()) {
                    let (p0, p1) = (xf.transform_point3(*p0), xf.transform_point3(*p1));
                    if p0.is_finite() && p1.is_finite() && (p1 - p0).length() > 1e-3 && !tag.is_empty() {
                        grids.push((tag, p0.to_array(), p1.to_array()));
                    }
                }
            }
        }
    }
    let mut levels: Vec<(String, f64)> = Vec::new();
    for &s in f.of_type("IFCBUILDINGSTOREY") {
        let a = f.args(s);
        let name = a.get(2).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let z = a.get(5).and_then(|v| v.as_ref()).map(|p| (scale_m * c.placement(p, 0)).transform_point3(DVec3::ZERO).z).or_else(|| a.get(9).and_then(|v| v.as_f64()).map(|e| e * scale));
        if let (false, Some(z)) = (name.is_empty(), z) {
            if z.is_finite() {
                levels.push((name, z));
            }
        }
    }
    if !grids.is_empty() {
        info.push(("~grids".into(), serde_json::to_string(&grids).unwrap_or_default()));
    }
    if !levels.is_empty() {
        info.push(("~levels".into(), serde_json::to_string(&levels).unwrap_or_default()));
    }
    info.push(("Length unit".into(), format!("{scale} m")));
    if elements.is_empty() {
        return Err("The IFC file contains no geometry that ManageBIM can display.".into());
    }
    Ok(RawModel { elements, info, warnings: c.warn.into_iter().collect() })
}


/// A number with at most 4 decimals, without useless zeros (3.2000 -> "3.2").
fn fmt_qty(x: f64) -> String {
    let s = format!("{x:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" { "0".into() } else { s.to_string() }
}

/// Shared (read-only) data for generating the elements in parallel.
struct Shared<'a> {
    parent: &'a HashMap<u32, u32>,
    psets: &'a HashMap<u32, Vec<u32>>,
    types: &'a HashMap<u32, u32>,
    materials: &'a HashMap<u32, u32>,
    mat_colors: &'a HashMap<u32, [u8; 4]>,
    voids: &'a HashMap<u32, Vec<u32>>,
    groups: &'a HashMap<u32, Vec<(String, String)>>,
    classes: &'a HashMap<u32, Vec<(String, String, String)>>,
    scale: f64,
    scale_m: DMat4,
}

/// An IFC product -> an element with geometry and properties (None if it has no displayable geometry).
fn make_element(c: &mut Ctx, sh: &Shared, id: u32) -> Option<RawElement> {
    let f = c.f;
    let a = f.args(id);
    if a.len() < 7 {
        return None;
    }
    let rep = a[6].as_ref()?;
    let ty = f.ty(id).to_string();
    let place = a[5].as_ref().map(|p| c.placement(p, 0)).unwrap_or(DMat4::IDENTITY);
    let xf = sh.scale_m * place;
    // default colour: material -> type
    let mat_info = sh.materials.get(&id).map(|m| c.material_info(*m, sh.mat_colors));
    let def_col = mat_info.as_ref().and_then(|m| m.1).unwrap_or_else(|| default_color(&ty));
    let local = c.product_tris(rep);
    if local.is_empty() {
        return None;
    }
    let mut tris = Vec::with_capacity(local.len());
    let mut colors = Vec::with_capacity(local.len());
    for (t, col) in local.iter() {
        let p = [xf.transform_point3(t[0]), xf.transform_point3(t[1]), xf.transform_point3(t[2])];
        if !(p[0].is_finite() && p[1].is_finite() && p[2].is_finite()) {
            continue;
        }
        if (p[1] - p[0]).cross(p[2] - p[0]).length_squared() < 1e-18 {
            return None;
        }
        tris.push(p);
        colors.push(col.unwrap_or(def_col));
    }
    // --- the openings (doors, windows, penetrations): subtracted from the host geometry
    if let Some(ops) = sh.voids.get(&id) {
        for &op in ops {
            let oa = f.args(op);
            let (Some(orep), Some(opl)) = (oa.get(6).and_then(|v| v.as_ref()), oa.get(5).and_then(|v| v.as_ref())) else { continue };
            let oxf = sh.scale_m * c.placement(opl, 0);
            let ot: Vec<[DVec3; 3]> = c.product_tris(orep).iter().map(|(t, _)| [oxf.transform_point3(t[0]), oxf.transform_point3(t[1]), oxf.transform_point3(t[2])]).collect();
            if !subtract_convex(&mut tris, &mut colors, &ot) {
                c.warn.insert("Some openings are not convex and were not cut from their walls.".into());
            }
        }
        // the very small pieces left after cutting
        let keep: Vec<bool> = tris.iter().map(|p| (p[1] - p[0]).cross(p[2] - p[0]).length_squared() >= 1e-18).collect();
        let mut k = 0;
        tris.retain(|_| {
            k += 1;
            keep[k - 1]
        });
        let mut k = 0;
        colors.retain(|_| {
            k += 1;
            keep[k - 1]
        });
    }
    if tris.is_empty() {
        return None;
    }
    // --- metadata
    let guid = a[0].as_str().unwrap_or("").to_string();
    let mut name = a[2].as_str().unwrap_or("").to_string();
    if name.is_empty() {
        name = format!("{} #{id}", pretty_type(&ty));
    }
    let mut props: Vec<Prop> = Vec::new();
    let mut push = |g: &str, n: &str, v: String| {
        if !v.is_empty() {
            props.push(Prop { group: g.to_string(), name: n.to_string(), value: v });
        }
    };
    push("Element", "Name", name.clone());
    push("Element", "IFC Type", pretty_type(&ty));
    push("Element", "GlobalId", guid.clone());
    push("Element", "STEP Id", format!("#{id}"));
    push("Element", "Description", a[3].as_str().unwrap_or("").to_string());
    push("Element", "ObjectType", a[4].as_str().unwrap_or("").to_string());
    push("Element", "Tag", a.get(7).and_then(|v| v.as_str()).unwrap_or("").to_string());
    if let Some(pt) = a.get(8).and_then(|v| v.as_enum()) {
        push("Element", "PredefinedType", pt.to_string());
    }
    // level
    let mut storey = String::new();
    let mut cur = id;
    for _ in 0..12 {
        let Some(p) = sh.parent.get(&cur) else { break };
        cur = *p;
        let pt = f.ty(cur);
        if pt == "IFCBUILDINGSTOREY" {
            let pa = f.args(cur);
            storey = pa.get(2).and_then(|v| v.as_str()).unwrap_or("").to_string();
            if storey.is_empty() {
                storey = format!("Level #{cur}");
            }
            if let Some(el) = pa.get(9).and_then(|v| v.as_f64()) {
                push("Element", "Level elevation", format!("{:.3} m", el * sh.scale));
            }
            break;
        }
    }
    if storey.is_empty() {
        // uses the first spatial container found
        if let Some(p) = sh.parent.get(&id) {
            let pa = f.args(*p);
            storey = pa.get(2).and_then(|v| v.as_str()).unwrap_or("").to_string();
        }
    }
    push("Element", "Level", storey.clone());
    if let Some((names, _)) = &mat_info {
        if !names.is_empty() {
            push("Material", "Material", names.join(", "));
        }
    }
    if let Some(t) = sh.types.get(&id) {
        let ta = f.args(*t);
        push("Type", "Type Name", ta.get(2).and_then(|v| v.as_str()).unwrap_or("").to_string());
        push("Type", "Type Class", pretty_type(f.ty(*t)));
        for ps in ta.get(5).map(|v| v.as_list()).unwrap_or(&[]) {
            if let Some(ps) = ps.as_ref() {
                c.read_pset(ps, &mut props, " (type)");
            }
        }
    }
    if let Some(list) = sh.psets.get(&id) {
        for ps in list {
            c.read_pset(*ps, &mut props, "");
        }
    }
    // systems / zones: "System" = the first system (used for grouping clashes and in Model Checker)
    if let Some(gs) = sh.groups.get(&id) {
        for (gty, gname) in gs {
            props.push(Prop { group: "Systems".into(), name: gty.clone(), value: gname.clone() });
        }
    }
    // classifications: the "Classification" group, name = the system, value = the code
    if let Some(cs) = sh.classes.get(&id) {
        for (system, code, cname) in cs {
            if !code.is_empty() {
                props.push(Prop { group: "Classification".into(), name: system.clone(), value: code.clone() });
            }
            if !cname.is_empty() {
                props.push(Prop { group: "Classification".into(), name: format!("{system} name"), value: cname.clone() });
            }
        }
    }
    Some(RawElement {
        name,
        ifc_type: pretty_type(&ty),
        guid,
        storey,
        props,
        tris,
        colors,
        default_hidden: ty == "IFCSPACE",
    })
}

struct Ctx<'a> {
    f: &'a StepFile,
    placements: HashMap<u32, DMat4>,
    /// the styles are collected once (Owned), then the geometry threads borrow them (Borrowed)
    styles: Cow<'a, HashMap<u32, [u8; 4]>>,
    rep_cache: HashMap<u32, Rc<Vec<Tri>>>,
    /// conversion factor angle -> radians
    angle: f64,
    /// conversion of IFC quantities (IfcQuantityLength / Area / Volume) to m, m², m³
    qscale: [f64; 3],
    warn: BTreeSet<String>,
}

impl<'a> Ctx<'a> {
    // ------------------------------------------------------------ units
    fn units(&mut self) -> f64 {
        let f = self.f;
        let mut scale = 1.0;
        let (mut area, mut vol) = (None, None);
        let Some(&ua) = f.of_type("IFCUNITASSIGNMENT").first() else { return 1.0 };
        let a = f.args(ua);
        for u in a.first().map(|v| v.as_list()).unwrap_or(&[]) {
            let Some(u) = u.as_ref() else { continue };
            let ua = f.args(u);
            match f.ty(u) {
                "IFCSIUNIT" => match ua.get(1).and_then(|v| v.as_enum()) {
                    Some("LENGTHUNIT") => scale = prefix(ua.get(2)),
                    // SQUARE_METRE / CUBIC_METRE: the prefix applies to the metre, hence squared / cubed
                    Some("AREAUNIT") => area = Some(prefix(ua.get(2)).powi(2)),
                    Some("VOLUMEUNIT") => vol = Some(prefix(ua.get(2)).powi(3)),
                    _ => {}
                },
                "IFCCONVERSIONBASEDUNIT" | "IFCCONVERSIONBASEDUNITWITHOFFSET" => {
                    let kind = ua.get(1).and_then(|v| v.as_enum()).unwrap_or("").to_string();
                    let Some(mw) = ua.get(3).and_then(|v| v.as_ref()) else { continue };
                    let ma = f.args(mw);
                    let factor = ma.first().and_then(|v| v.as_f64()).unwrap_or(1.0);
                    let base = ma.get(1).and_then(|v| v.as_ref()).map(|b| prefix(f.args(b).get(2))).unwrap_or(1.0);
                    match kind.as_str() {
                        "LENGTHUNIT" => scale = factor * base,
                        "PLANEANGLEUNIT" => self.angle = factor,
                        // e.g. square foot / cubic foot
                        "AREAUNIT" => area = Some(factor * base * base),
                        "VOLUMEUNIT" => vol = Some(factor * base * base * base),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        // no area / volume unit declared: derived from the length unit
        self.qscale = [scale, area.unwrap_or(scale * scale), vol.unwrap_or(scale * scale * scale)];
        scale
    }

    // ------------------------------------------------------------ styles
    fn collect_styles(&mut self) {
        let f = self.f;
        for &s in f.of_type("IFCSTYLEDITEM") {
            let a = f.args(s);
            let Some(item) = a.first().and_then(|v| v.as_ref()) else { continue };
            if let Some(col) = a.get(1).and_then(|st| self.style_color(st, 0)) {
                self.styles.to_mut().insert(item, col);
            }
        }
    }

    fn style_color(&self, v: &V, depth: u32) -> Col {
        if depth > 6 {
            return None;
        }
        match v {
            V::List(l) => l.iter().find_map(|x| self.style_color(x, depth + 1)),
            V::Ref(id) => {
                let f = self.f;
                let a = f.args(*id);
                match f.ty(*id) {
                    "IFCPRESENTATIONSTYLEASSIGNMENT" => a.first().and_then(|x| self.style_color(x, depth + 1)),
                    "IFCSURFACESTYLE" => a.get(2).and_then(|x| self.style_color(x, depth + 1)),
                    "IFCSURFACESTYLERENDERING" | "IFCSURFACESTYLESHADING" => {
                        let c = a.first().and_then(|x| x.as_ref())?;
                        let ca = f.args(c);
                        if ca.len() < 4 {
                            return None;
                        }
                        let r = ca[1].as_f64()?;
                        let g = ca[2].as_f64()?;
                        let b = ca[3].as_f64()?;
                        let t = a.get(1).and_then(|x| x.as_f64()).unwrap_or(0.0).clamp(0.0, 1.0);
                        let al = (1.0 - t).max(0.15);
                        Some([to8(r), to8(g), to8(b), to8(al)])
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// (material names, colour)
    fn material_info(&self, m: u32, mat_colors: &HashMap<u32, [u8; 4]>) -> (Vec<String>, Col) {
        let mut names = Vec::new();
        let mut col = None;
        self.mat_rec(m, mat_colors, &mut names, &mut col, 0);
        (names, col)
    }

    fn mat_rec(&self, id: u32, mc: &HashMap<u32, [u8; 4]>, names: &mut Vec<String>, col: &mut Col, depth: u32) {
        if depth > 5 || names.len() > 12 {
            return;
        }
        let f = self.f;
        let ty = f.ty(id);
        if !ty.starts_with("IFCMATERIAL") {
            return;
        }
        let a = f.args(id);
        if ty == "IFCMATERIAL" {
            if let Some(n) = a.first().and_then(|v| v.as_str()) {
                if !names.iter().any(|x| x == n) {
                    names.push(n.to_string());
                }
            }
            if col.is_none() {
                *col = mc.get(&id).copied();
            }
            return;
        }
        fn walk(c: &Ctx, v: &V, mc: &HashMap<u32, [u8; 4]>, names: &mut Vec<String>, col: &mut Col, depth: u32) {
            match v {
                V::Ref(r) => c.mat_rec(*r, mc, names, col, depth + 1),
                V::List(l) => l.iter().for_each(|x| walk(c, x, mc, names, col, depth)),
                _ => {}
            }
        }
        for v in &a {
            walk(self, v, mc, names, col, depth);
        }
    }

    // ------------------------------------------------------------ properties
    fn read_pset(&self, id: u32, out: &mut Vec<Prop>, suffix: &str) {
        let f = self.f;
        let a = f.args(id);
        let ty = f.ty(id);
        let gname = format!("{}{}", a.get(2).and_then(|v| v.as_str()).unwrap_or(ty), suffix);
        let list = match ty {
            "IFCPROPERTYSET" => a.get(4),
            "IFCELEMENTQUANTITY" => a.get(5),
            _ => None,
        };
        for p in list.map(|v| v.as_list()).unwrap_or(&[]) {
            let Some(p) = p.as_ref() else { continue };
            let pa = f.args(p);
            let pname = pa.first().and_then(|v| v.as_str()).unwrap_or("").to_string();
            let value = match f.ty(p) {
                "IFCPROPERTYSINGLEVALUE" => pa.get(2).map(|v| v.display()).unwrap_or_default(),
                "IFCPROPERTYENUMERATEDVALUE" | "IFCPROPERTYLISTVALUE" => pa.get(2).map(|v| v.display()).unwrap_or_default(),
                "IFCPROPERTYBOUNDEDVALUE" => format!(
                    "{} .. {}",
                    pa.get(3).map(|v| v.display()).unwrap_or_default(),
                    pa.get(2).map(|v| v.display()).unwrap_or_default()
                ),
                // quantities in SI units, with the unit written out (for Quantification and Profiler)
                t if t.starts_with("IFCQUANTITY") => {
                    let raw = pa.get(3);
                    let (k, unit) = match t {
                        "IFCQUANTITYLENGTH" => (self.qscale[0], "m"),
                        "IFCQUANTITYAREA" => (self.qscale[1], "m²"),
                        "IFCQUANTITYVOLUME" => (self.qscale[2], "m³"),
                        "IFCQUANTITYWEIGHT" => (1.0, "kg"),
                        _ => (1.0, ""),
                    };
                    match raw.and_then(|v| v.as_f64()) {
                        Some(x) => {
                            let s = fmt_qty(x * k);
                            if unit.is_empty() { s } else { format!("{s} {unit}") }
                        }
                        None => raw.map(|v| v.display()).unwrap_or_default(),
                    }
                }
                _ => continue,
            };
            if !pname.is_empty() && !value.is_empty() {
                out.push(Prop { group: gname.clone(), name: pname, value });
            }
        }
    }

    // ------------------------------------------------------------ placement
    fn point(&self, id: u32) -> DVec3 {
        let a = self.f.args(id);
        let l = a.first().map(|v| v.as_list()).unwrap_or(&[]);
        DVec3::new(
            l.first().and_then(|v| v.as_f64()).unwrap_or(0.0),
            l.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0),
            l.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0),
        )
    }

    fn opt_dir(&self, v: Option<&V>) -> Option<DVec3> {
        let id = v?.as_ref()?;
        let d = self.point(id);
        if d.length_squared() > 1e-20 {
            Some(d.normalize())
        } else {
            None
        }
    }

    fn axis2(&self, id: u32) -> DMat4 {
        let f = self.f;
        let a = f.args(id);
        let loc = a.first().and_then(|v| v.as_ref()).map(|p| self.point(p)).unwrap_or(DVec3::ZERO);
        match f.ty(id) {
            "IFCAXIS2PLACEMENT3D" => {
                let z = self.opt_dir(a.get(1)).unwrap_or(DVec3::Z);
                let xr = self.opt_dir(a.get(2)).unwrap_or(DVec3::X);
                frame(loc, z, xr)
            }
            "IFCAXIS2PLACEMENT2D" => {
                let x = self.opt_dir(a.get(1)).unwrap_or(DVec3::X);
                let x = DVec3::new(x.x, x.y, 0.0).normalize_or(DVec3::X);
                let y = DVec3::new(-x.y, x.x, 0.0);
                DMat4::from_cols(x.extend(0.0), y.extend(0.0), DVec4::Z, loc.extend(1.0))
            }
            "IFCAXIS1PLACEMENT" => {
                let z = self.opt_dir(a.get(1)).unwrap_or(DVec3::Z);
                frame(loc, z, DVec3::X)
            }
            _ => DMat4::IDENTITY,
        }
    }

    fn placement(&mut self, id: u32, depth: u32) -> DMat4 {
        if let Some(m) = self.placements.get(&id) {
            return *m;
        }
        if depth > 64 {
            return DMat4::IDENTITY;
        }
        let f = self.f;
        let a = f.args(id);
        let m = match f.ty(id) {
            "IFCLOCALPLACEMENT" => {
                let parent = a.first().and_then(|v| v.as_ref()).map(|p| self.placement(p, depth + 1)).unwrap_or(DMat4::IDENTITY);
                let rel = a.get(1).and_then(|v| v.as_ref()).map(|p| self.axis2(p)).unwrap_or(DMat4::IDENTITY);
                parent * rel
            }
            _ => DMat4::IDENTITY,
        };
        self.placements.insert(id, m);
        m
    }

    fn cart_op(&self, id: u32) -> DMat4 {
        let f = self.f;
        let a = f.args(id);
        let ty = f.ty(id);
        let origin = a.get(2).and_then(|v| v.as_ref()).map(|p| self.point(p)).unwrap_or(DVec3::ZERO);
        let s = a.get(3).and_then(|v| v.as_f64()).unwrap_or(1.0);
        if ty.starts_with("IFCCARTESIANTRANSFORMATIONOPERATOR2D") {
            let x = self.opt_dir(a.first()).unwrap_or(DVec3::X);
            let mut y = DVec3::new(-x.y, x.x, 0.0);
            if let Some(a2) = self.opt_dir(a.get(1)) {
                if a2.dot(y) < 0.0 {
                    y = -y;
                }
            }
            let s2 = a.get(4).and_then(|v| v.as_f64()).unwrap_or(s);
            DMat4::from_cols((x * s).extend(0.0), (y * s2).extend(0.0), DVec4::Z, origin.extend(1.0))
        } else {
            let z = self.opt_dir(a.get(4)).unwrap_or(DVec3::Z);
            let xr = self.opt_dir(a.first()).unwrap_or(DVec3::X);
            let m = frame(DVec3::ZERO, z, xr);
            let x = m.x_axis.truncate();
            let mut y = m.y_axis.truncate();
            if let Some(a2) = self.opt_dir(a.get(1)) {
                if a2.dot(y) < 0.0 {
                    y = -y;
                }
            }
            let s2 = a.get(5).and_then(|v| v.as_f64()).unwrap_or(s);
            let s3 = a.get(6).and_then(|v| v.as_f64()).unwrap_or(s);
            DMat4::from_cols((x * s).extend(0.0), (y * s2).extend(0.0), (z * s3).extend(0.0), origin.extend(1.0))
        }
    }

    // ------------------------------------------------------------ representations
    fn product_tris(&mut self, pds: u32) -> Rc<Vec<Tri>> {
        let f = self.f;
        let a = f.args(pds);
        let reps: Vec<u32> = a.get(2).map(|v| v.as_list()).unwrap_or(&[]).iter().filter_map(|v| v.as_ref()).collect();
        let mut body: Vec<u32> = Vec::new();
        let mut other: Vec<u32> = Vec::new();
        for r in reps {
            let ra = f.args(r);
            let ident = ra.get(1).and_then(|v| v.as_str()).unwrap_or("").to_ascii_lowercase();
            let rtype = ra.get(2).and_then(|v| v.as_str()).unwrap_or("").to_ascii_lowercase();
            if ident == "body" || ident == "facetation" {
                body.push(r);
            } else if matches!(ident.as_str(), "axis" | "footprint" | "box" | "annotation" | "profile" | "clearance" | "plan" | "cog" | "reference" | "lighting")
                || matches!(rtype.as_str(), "curve2d" | "curve3d" | "boundingbox" | "geometriccurveset" | "annotation2d" | "point")
            {
            } else {
                other.push(r);
            }
        }
        let use_reps = if body.is_empty() { other } else { body };
        if use_reps.len() == 1 {
            return self.rep_tris(use_reps[0], 0);
        }
        let mut out = Vec::new();
        for r in use_reps {
            out.extend(self.rep_tris(r, 0).iter().cloned());
        }
        Rc::new(out)
    }

    fn rep_tris(&mut self, rep: u32, depth: u32) -> Rc<Vec<Tri>> {
        if let Some(c) = self.rep_cache.get(&rep) {
            return c.clone();
        }
        let a = self.f.args(rep);
        let mut out = Vec::new();
        for it in a.get(3).map(|v| v.as_list()).unwrap_or(&[]) {
            if let Some(it) = it.as_ref() {
                self.item(it, None, depth, &mut out);
            }
        }
        let rc = Rc::new(out);
        self.rep_cache.insert(rep, rc.clone());
        rc
    }

    fn item(&mut self, id: u32, inherited: Col, depth: u32, out: &mut Vec<Tri>) {
        if depth > 24 {
            return;
        }
        let f = self.f;
        let col = self.styles.get(&id).copied().or(inherited);
        let a = f.args(id);
        let ty = f.ty(id);
        match ty {
            "IFCMAPPEDITEM" => {
                let (Some(src), Some(tgt)) = (a.first().and_then(|v| v.as_ref()), a.get(1).and_then(|v| v.as_ref())) else { return };
                let sa = f.args(src);
                let origin = sa.first().and_then(|v| v.as_ref()).map(|p| self.axis2(p)).unwrap_or(DMat4::IDENTITY);
                let Some(mrep) = sa.get(1).and_then(|v| v.as_ref()) else { return };
                let xf = self.cart_op(tgt) * origin;
                let inner = self.rep_tris(mrep, depth + 1);
                out.reserve(inner.len());
                for (t, c) in inner.iter() {
                    out.push(([xf.transform_point3(t[0]), xf.transform_point3(t[1]), xf.transform_point3(t[2])], c.or(col)));
                }
            }
            "IFCEXTRUDEDAREASOLID" | "IFCEXTRUDEDAREASOLIDTAPERED" => {
                let Some(prof) = a.first().and_then(|v| v.as_ref()) else { return };
                let pos = a.get(1).and_then(|v| v.as_ref()).map(|p| self.axis2(p)).unwrap_or(DMat4::IDENTITY);
                let dir = self.opt_dir(a.get(2)).unwrap_or(DVec3::Z);
                let depth_v = a.get(3).and_then(|v| v.as_f64()).unwrap_or(0.0);
                let ext = dir * depth_v;
                for (outer, holes) in self.profile(prof, 0) {
                    extrude(&outer, &holes, ext, &pos, col, out);
                }
            }
            "IFCREVOLVEDAREASOLID" | "IFCREVOLVEDAREASOLIDTAPERED" => {
                let Some(prof) = a.first().and_then(|v| v.as_ref()) else { return };
                let pos = a.get(1).and_then(|v| v.as_ref()).map(|p| self.axis2(p)).unwrap_or(DMat4::IDENTITY);
                let Some(ax) = a.get(2).and_then(|v| v.as_ref()) else { return };
                let axa = f.args(ax);
                let aloc = axa.first().and_then(|v| v.as_ref()).map(|p| self.point(p)).unwrap_or(DVec3::ZERO);
                let adir = self.opt_dir(axa.get(1)).unwrap_or(DVec3::Z);
                let ang = (a.get(3).and_then(|v| v.as_f64()).unwrap_or(0.0) * self.angle).clamp(-2.0 * PI, 2.0 * PI);
                for (outer, holes) in self.profile(prof, 0) {
                    revolve(&outer, &holes, aloc, adir, ang, &pos, col, out);
                }
            }
            "IFCSWEPTDISKSOLID" | "IFCSWEPTDISKSOLIDPOLYGONAL" => {
                let Some(dx) = a.first().and_then(|v| v.as_ref()) else { return };
                let r = a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                let path = self.curve(dx, 0);
                tube(&path, r, col, out);
            }
            "IFCFACETEDBREP" | "IFCFACETEDBREPWITHVOIDS" | "IFCADVANCEDBREP" | "IFCADVANCEDBREPWITHVOIDS" | "IFCMANIFOLDSOLIDBREP" => {
                if ty.starts_with("IFCADVANCED") {
                    self.warn.insert("IfcAdvancedBrep: curved surfaces are approximated by planar facets.".into());
                }
                if let Some(sh) = a.first().and_then(|v| v.as_ref()) {
                    self.shell(sh, col, out);
                }
            }
            "IFCFACEBASEDSURFACEMODEL" | "IFCSHELLBASEDSURFACEMODEL" => {
                for s in a.first().map(|v| v.as_list()).unwrap_or(&[]) {
                    if let Some(s) = s.as_ref() {
                        self.shell(s, col, out);
                    }
                }
            }
            "IFCCLOSEDSHELL" | "IFCOPENSHELL" | "IFCCONNECTEDFACESET" => self.shell(id, col, out),
            "IFCTRIANGULATEDFACESET" | "IFCTRIANGULATEDIRREGULARNETWORK" => {
                let Some(cl) = a.first().and_then(|v| v.as_ref()) else { return };
                let pts = self.point_list(cl);
                let pn: Vec<usize> = a.get(4).map(|v| v.as_list()).unwrap_or(&[]).iter().filter_map(|v| v.as_i64()).map(|i| i as usize).collect();
                let idx = |i: i64| -> Option<DVec3> {
                    let mut k = i as usize;
                    if !pn.is_empty() {
                        k = *pn.get(k.checked_sub(1)?)?;
                    }
                    pts.get(k.checked_sub(1)?).copied()
                };
                for t in a.get(3).map(|v| v.as_list()).unwrap_or(&[]) {
                    let l = t.as_list();
                    if l.len() >= 3 {
                        if let (Some(p0), Some(p1), Some(p2)) = (
                            l[0].as_i64().and_then(&idx),
                            l[1].as_i64().and_then(&idx),
                            l[2].as_i64().and_then(&idx),
                        ) {
                            out.push(([p0, p1, p2], col));
                        }
                    }
                }
            }
            "IFCPOLYGONALFACESET" => {
                let Some(cl) = a.first().and_then(|v| v.as_ref()) else { return };
                let pts = self.point_list(cl);
                let pn: Vec<usize> = a.get(3).map(|v| v.as_list()).unwrap_or(&[]).iter().filter_map(|v| v.as_i64()).map(|i| i as usize).collect();
                let ring = |v: &V| -> Vec<DVec3> {
                    v.as_list()
                        .iter()
                        .filter_map(|i| {
                            let mut k = i.as_i64()? as usize;
                            if !pn.is_empty() {
                                k = *pn.get(k.checked_sub(1)?)?;
                            }
                            pts.get(k.checked_sub(1)?).copied()
                        })
                        .collect()
                };
                for fc in a.get(2).map(|v| v.as_list()).unwrap_or(&[]) {
                    let Some(fc) = fc.as_ref() else { continue };
                    let fa = f.args(fc);
                    let outer = fa.first().map(&ring).unwrap_or_default();
                    let holes: Vec<Vec<DVec3>> = fa.get(1).map(|v| v.as_list()).unwrap_or(&[]).iter().map(&ring).collect();
                    tri_poly(&outer, &holes, col, out);
                }
            }
            "IFCBOOLEANCLIPPINGRESULT" | "IFCBOOLEANRESULT" => {
                let op = a.first().and_then(|v| v.as_enum()).unwrap_or("").to_string();
                let Some(first) = a.get(1).and_then(|v| v.as_ref()) else { return };
                let mut tmp = Vec::new();
                self.item(first, col, depth + 1, &mut tmp);
                if let Some(second) = a.get(2).and_then(|v| v.as_ref()) {
                    if op == "DIFFERENCE" && f.ty(second) == "IFCHALFSPACESOLID" {
                        let sa = f.args(second);
                        if let Some(surf) = sa.first().and_then(|v| v.as_ref()) {
                            if f.ty(surf) == "IFCPLANE" {
                                let pa = f.args(surf);
                                let pm = pa.first().and_then(|v| v.as_ref()).map(|p| self.axis2(p)).unwrap_or(DMat4::IDENTITY);
                                let o = pm.w_axis.truncate();
                                let mut n = pm.z_axis.truncate();
                                if sa.get(1).and_then(|v| v.as_enum()) == Some("F") {
                                    n = -n;
                                }
                                tmp = clip_tris(tmp, o, n);
                            }
                        }
                    } else if op == "UNION" {
                        self.item(second, col, depth + 1, &mut tmp);
                    } else if op == "DIFFERENCE" {
                        self.warn.insert("Complex boolean operations (cuts) are shown without subtraction.".into());
                    }
                }
                out.extend(tmp);
            }
            "IFCCSGSOLID" => {
                if let Some(t) = a.first().and_then(|v| v.as_ref()) {
                    self.item(t, col, depth + 1, out);
                }
            }
            "IFCBLOCK" => {
                let pos = a.first().and_then(|v| v.as_ref()).map(|p| self.axis2(p)).unwrap_or(DMat4::IDENTITY);
                let x = a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                let y = a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0);
                let z = a.get(3).and_then(|v| v.as_f64()).unwrap_or(0.0);
                let outer = vec![DVec2::ZERO, DVec2::new(x, 0.0), DVec2::new(x, y), DVec2::new(0.0, y)];
                extrude(&outer, &[], DVec3::Z * z, &pos, col, out);
            }
            "IFCRIGHTCIRCULARCYLINDER" => {
                let pos = a.first().and_then(|v| v.as_ref()).map(|p| self.axis2(p)).unwrap_or(DMat4::IDENTITY);
                let h = a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                let r = a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0);
                extrude(&circle(r, 24), &[], DVec3::Z * h, &pos, col, out);
            }
            "IFCBOUNDINGBOX" | "IFCPOLYLINE" | "IFCGEOMETRICSET" | "IFCGEOMETRICCURVESET" | "IFCTRIMMEDCURVE" | "IFCCOMPOSITECURVE"
            | "IFCINDEXEDPOLYCURVE" | "IFCCARTESIANPOINT" | "IFCLINE" | "IFCCIRCLE" | "IFCTEXTLITERALWITHEXTENT" | "IFCANNOTATIONFILLAREA" => {}
            "" => {}
            other => {
                self.warn.insert(format!("Unsupported geometry type (ignored): {}", pretty_type(other)));
            }
        }
    }

    fn point_list(&self, id: u32) -> Vec<DVec3> {
        let a = self.f.args(id);
        a.first()
            .map(|v| v.as_list())
            .unwrap_or(&[])
            .iter()
            .map(|p| {
                let l = p.as_list();
                DVec3::new(
                    l.first().and_then(|v| v.as_f64()).unwrap_or(0.0),
                    l.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0),
                    l.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0),
                )
            })
            .collect()
    }

    fn shell(&mut self, id: u32, col: Col, out: &mut Vec<Tri>) {
        let f = self.f;
        let a = f.args(id);
        for fc in a.first().map(|v| v.as_list()).unwrap_or(&[]) {
            let Some(fc) = fc.as_ref() else { continue };
            let fcol = self.styles.get(&fc).copied().or(col);
            let fa = f.args(fc);
            let mut outer: Vec<DVec3> = Vec::new();
            let mut holes: Vec<Vec<DVec3>> = Vec::new();
            for b in fa.first().map(|v| v.as_list()).unwrap_or(&[]) {
                let Some(b) = b.as_ref() else { continue };
                let ba = f.args(b);
                let Some(lp) = ba.first().and_then(|v| v.as_ref()) else { continue };
                let pts = self.loop_pts(lp);
                if pts.len() < 3 {
                    continue;
                }
                if f.ty(b) == "IFCFACEOUTERBOUND" && outer.is_empty() {
                    outer = pts;
                } else {
                    holes.push(pts);
                }
            }
            if outer.is_empty() && !holes.is_empty() {
                // no outer boundary marked: the largest one becomes the outer one
                let mut bi = 0;
                let mut ba = -1.0;
                for (i, h) in holes.iter().enumerate() {
                    let ar = newell(h).length();
                    if ar > ba {
                        ba = ar;
                        bi = i;
                    }
                }
                outer = holes.remove(bi);
            }
            tri_poly(&outer, &holes, fcol, out);
        }
    }

    fn loop_pts(&mut self, id: u32) -> Vec<DVec3> {
        let f = self.f;
        let a = f.args(id);
        let mut pts: Vec<DVec3> = match f.ty(id) {
            "IFCPOLYLOOP" => a.first().map(|v| v.as_list()).unwrap_or(&[]).iter().filter_map(|v| v.as_ref()).map(|p| self.point(p)).collect(),
            "IFCEDGELOOP" => {
                let mut v = Vec::new();
                for e in a.first().map(|v| v.as_list()).unwrap_or(&[]) {
                    let Some(e) = e.as_ref() else { continue };
                    let ea = f.args(e);
                    let (edge, sense) = if f.ty(e) == "IFCORIENTEDEDGE" {
                        (ea.get(2).and_then(|v| v.as_ref()), ea.get(3).and_then(|v| v.as_enum()) != Some("F"))
                    } else {
                        (Some(e), true)
                    };
                    let Some(edge) = edge else { continue };
                    let ga = f.args(edge);
                    let vp = |v: Option<&V>| -> Option<DVec3> {
                        let vid = v?.as_ref()?;
                        let va = f.args(vid);
                        Some(self.point(va.first()?.as_ref()?))
                    };
                    let (Some(s), Some(en)) = (vp(ga.first()), vp(ga.get(1))) else { continue };
                    // curved edge: samples the edge geometry
                    let mut seg: Vec<DVec3> = Vec::new();
                    if let Some(g) = ga.get(2).and_then(|v| v.as_ref()) {
                        let gt = f.ty(g);
                        if gt == "IFCCIRCLE" || gt == "IFCELLIPSE" {
                            let same = ga.get(3).and_then(|v| v.as_enum()) != Some("F");
                            seg = self.conic_between(g, s, en, same);
                        } else if gt != "IFCLINE" && gt != "IFCPOLYLINE" {
                            let c = self.curve(g, 0);
                            if c.len() > 2 {
                                let same = ga.get(3).and_then(|v| v.as_enum()) != Some("F");
                                seg = c;
                                if !same {
                                    seg.reverse();
                                }
                            }
                        }
                    }
                    if seg.len() < 2 {
                        seg = vec![s, en];
                    }
                    if !sense {
                        seg.reverse();
                    }
                    seg.pop();
                    v.extend(seg);
                }
                v
            }
            _ => Vec::new(),
        };
        dedup_ring(&mut pts);
        pts
    }

    /// arc on a circle/ellipse between two points (counter-clockwise if `ccw`)
    fn conic_between(&self, conic: u32, s: DVec3, e: DVec3, ccw: bool) -> Vec<DVec3> {
        let a = self.f.args(conic);
        let pos = a.first().and_then(|v| v.as_ref()).map(|p| self.axis2(p)).unwrap_or(DMat4::IDENTITY);
        let r1 = a.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0);
        let r2 = a.get(2).and_then(|v| v.as_f64()).unwrap_or(r1);
        let inv = pos.inverse();
        let ls = inv.transform_point3(s);
        let le = inv.transform_point3(e);
        let t1 = (ls.y / r2).atan2(ls.x / r1);
        let mut t2 = (le.y / r2).atan2(le.x / r1);
        if ccw {
            if t2 <= t1 + 1e-9 {
                t2 += 2.0 * PI;
            }
        } else if t2 >= t1 - 1e-9 {
            t2 -= 2.0 * PI;
        }
        let n = (((t2 - t1).abs() / (PI / 12.0)).ceil() as usize).max(1);
        (0..=n)
            .map(|i| {
                let t = t1 + (t2 - t1) * i as f64 / n as f64;
                pos.transform_point3(DVec3::new(r1 * t.cos(), r2 * t.sin(), 0.0))
            })
            .collect()
    }

    // ------------------------------------------------------------ curves
    fn curve(&mut self, id: u32, depth: u32) -> Vec<DVec3> {
        if depth > 12 {
            return Vec::new();
        }
        let f = self.f;
        let a = f.args(id);
        match f.ty(id) {
            "IFCPOLYLINE" => a.first().map(|v| v.as_list()).unwrap_or(&[]).iter().filter_map(|v| v.as_ref()).map(|p| self.point(p)).collect(),
            "IFCINDEXEDPOLYCURVE" => {
                let Some(pl) = a.first().and_then(|v| v.as_ref()) else { return Vec::new() };
                let pts = self.point_list(pl);
                let segs = a.get(1).map(|v| v.as_list()).unwrap_or(&[]);
                if segs.is_empty() {
                    return pts;
                }
                let mut out: Vec<DVec3> = Vec::new();
                for s in segs {
                    let (name, inner) = match s {
                        V::Typed(n, v) => (n.as_str(), v.as_ref()),
                        _ => continue,
                    };
                    let idx: Vec<DVec3> = inner.as_list().iter().filter_map(|i| pts.get((i.as_i64()? as usize).checked_sub(1)?).copied()).collect();
                    let seg = if name == "IFCARCINDEX" && idx.len() == 3 { arc3(idx[0], idx[1], idx[2]) } else { idx };
                    append_curve(&mut out, seg);
                }
                out
            }
            "IFCCOMPOSITECURVE" | "IFCCOMPOSITECURVEONSURFACE" | "IFCBOUNDARYCURVE" | "IFCOUTERBOUNDARYCURVE" => {
                let mut out: Vec<DVec3> = Vec::new();
                for s in a.first().map(|v| v.as_list()).unwrap_or(&[]) {
                    let Some(s) = s.as_ref() else { continue };
                    let sa = f.args(s);
                    let Some(pc) = sa.get(2).and_then(|v| v.as_ref()) else { continue };
                    let mut seg = self.curve(pc, depth + 1);
                    if sa.get(1).and_then(|v| v.as_enum()) == Some("F") {
                        seg.reverse();
                    }
                    append_curve(&mut out, seg);
                }
                out
            }
            "IFCTRIMMEDCURVE" => {
                let Some(basis) = a.first().and_then(|v| v.as_ref()) else { return Vec::new() };
                let sense = a.get(3).and_then(|v| v.as_enum()) != Some("F");
                let trim = |v: Option<&V>| -> (Option<f64>, Option<DVec3>) {
                    let mut par = None;
                    let mut pt = None;
                    for x in v.map(|v| v.as_list()).unwrap_or(&[]) {
                        match x {
                            V::Ref(r) => pt = Some(self.point(*r)),
                            o => par = o.as_f64().or(par),
                        }
                    }
                    (par, pt)
                };
                let (p1, c1) = trim(a.get(1));
                let (p2, c2) = trim(a.get(2));
                let ba = f.args(basis);
                match f.ty(basis) {
                    "IFCCIRCLE" | "IFCELLIPSE" => {
                        let pos = ba.first().and_then(|v| v.as_ref()).map(|p| self.axis2(p)).unwrap_or(DMat4::IDENTITY);
                        let r1 = ba.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0);
                        let r2 = ba.get(2).and_then(|v| v.as_f64()).unwrap_or(r1);
                        let inv = pos.inverse();
                        let ang = |p: Option<f64>, c: Option<DVec3>| -> f64 {
                            if let Some(c) = c {
                                let l = inv.transform_point3(c);
                                (l.y / r2).atan2(l.x / r1)
                            } else {
                                p.unwrap_or(0.0) * self.angle
                            }
                        };
                        let t1 = ang(p1, c1);
                        let mut t2 = ang(p2, c2);
                        if sense {
                            while t2 <= t1 + 1e-9 {
                                t2 += 2.0 * PI;
                            }
                        } else {
                            while t2 >= t1 - 1e-9 {
                                t2 -= 2.0 * PI;
                            }
                        }
                        let n = (((t2 - t1).abs() / (PI / 12.0)).ceil() as usize).max(1);
                        (0..=n)
                            .map(|i| {
                                let t = t1 + (t2 - t1) * i as f64 / n as f64;
                                pos.transform_point3(DVec3::new(r1 * t.cos(), r2 * t.sin(), 0.0))
                            })
                            .collect()
                    }
                    "IFCLINE" => {
                        let o = ba.first().and_then(|v| v.as_ref()).map(|p| self.point(p)).unwrap_or(DVec3::ZERO);
                        let (d, mag) = ba
                            .get(1)
                            .and_then(|v| v.as_ref())
                            .map(|v| {
                                let va = f.args(v);
                                (self.opt_dir(va.first()).unwrap_or(DVec3::X), va.get(1).and_then(|m| m.as_f64()).unwrap_or(1.0))
                            })
                            .unwrap_or((DVec3::X, 1.0));
                        let a0 = c1.unwrap_or_else(|| o + d * mag * p1.unwrap_or(0.0));
                        let b0 = c2.unwrap_or_else(|| o + d * mag * p2.unwrap_or(1.0));
                        vec![a0, b0]
                    }
                    _ => {
                        let mut c = self.curve(basis, depth + 1);
                        if !sense {
                            c.reverse();
                        }
                        c
                    }
                }
            }
            "IFCCIRCLE" | "IFCELLIPSE" => {
                let pos = a.first().and_then(|v| v.as_ref()).map(|p| self.axis2(p)).unwrap_or(DMat4::IDENTITY);
                let r1 = a.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0);
                let r2 = a.get(2).and_then(|v| v.as_f64()).unwrap_or(r1);
                (0..24)
                    .map(|i| {
                        let t = 2.0 * PI * i as f64 / 24.0;
                        pos.transform_point3(DVec3::new(r1 * t.cos(), r2 * t.sin(), 0.0))
                    })
                    .collect()
            }
            "IFCBSPLINECURVEWITHKNOTS" | "IFCBSPLINECURVE" | "IFCRATIONALBSPLINECURVEWITHKNOTS" | "IFCBEZIERCURVE" => {
                self.warn.insert("B-spline curves approximated by their control polygon.".into());
                a.get(1).map(|v| v.as_list()).unwrap_or(&[]).iter().filter_map(|v| v.as_ref()).map(|p| self.point(p)).collect()
            }
            other => {
                if !other.is_empty() {
                    self.warn.insert(format!("Unsupported curve type: {}", pretty_type(other)));
                }
                Vec::new()
            }
        }
    }

    fn curve2(&mut self, id: u32) -> Vec<DVec2> {
        let mut pts = self.curve(id, 0);
        dedup_ring(&mut pts);
        pts.into_iter().map(|p| p.truncate()).collect()
    }

    // ------------------------------------------------------------ profiles
    fn profile(&mut self, id: u32, depth: u32) -> Vec<Profile> {
        if depth > 8 {
            return Vec::new();
        }
        let f = self.f;
        let a = f.args(id);
        let ty = f.ty(id);
        let d = |i: usize| a.get(i).and_then(|v| v.as_f64()).unwrap_or(0.0);
        let mut res: Vec<Profile> = match ty {
            "IFCARBITRARYCLOSEDPROFILEDEF" => {
                let outer = a.get(2).and_then(|v| v.as_ref()).map(|c| self.curve2(c)).unwrap_or_default();
                return vec![(outer, Vec::new())];
            }
            "IFCARBITRARYPROFILEDEFWITHVOIDS" => {
                let outer = a.get(2).and_then(|v| v.as_ref()).map(|c| self.curve2(c)).unwrap_or_default();
                let holes: Vec<Vec<DVec2>> = a.get(3).map(|v| v.as_list()).unwrap_or(&[]).iter().filter_map(|v| v.as_ref()).map(|c| self.curve2(c)).filter(|h| h.len() >= 3).collect();
                return vec![(outer, holes)];
            }
            "IFCDERIVEDPROFILEDEF" | "IFCMIRROREDPROFILEDEF" => {
                let Some(par) = a.get(2).and_then(|v| v.as_ref()) else { return Vec::new() };
                let mut inner = self.profile(par, depth + 1);
                let m = a.get(3).and_then(|v| v.as_ref()).map(|o| self.cart_op(o)).unwrap_or_else(|| DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0)));
                for (o, hs) in inner.iter_mut() {
                    xf2(o, &m);
                    hs.iter_mut().for_each(|h| xf2(h, &m));
                }
                return inner;
            }
            "IFCCOMPOSITEPROFILEDEF" => {
                let mut all = Vec::new();
                for p in a.get(2).map(|v| v.as_list()).unwrap_or(&[]) {
                    if let Some(p) = p.as_ref() {
                        all.extend(self.profile(p, depth + 1));
                    }
                }
                return all;
            }
            "IFCRECTANGLEPROFILEDEF" | "IFCROUNDEDRECTANGLEPROFILEDEF" => vec![(rect(d(3), d(4)), Vec::new())],
            "IFCRECTANGLEHOLLOWPROFILEDEF" => {
                let t = d(5);
                let (x, y) = (d(3), d(4));
                let holes = if t > 0.0 && x > 2.0 * t && y > 2.0 * t { vec![rect(x - 2.0 * t, y - 2.0 * t)] } else { Vec::new() };
                vec![(rect(x, y), holes)]
            }
            "IFCCIRCLEPROFILEDEF" => vec![(circle(d(3), 24), Vec::new())],
            "IFCCIRCLEHOLLOWPROFILEDEF" => {
                let (r, t) = (d(3), d(4));
                let holes = if t > 0.0 && r > t { vec![circle(r - t, 24)] } else { Vec::new() };
                vec![(circle(r, 24), holes)]
            }
            "IFCELLIPSEPROFILEDEF" => {
                let (r1, r2) = (d(3), d(4));
                vec![(circle(1.0, 24).into_iter().map(|p| DVec2::new(p.x * r1, p.y * r2)).collect(), Vec::new())]
            }
            "IFCISHAPEPROFILEDEF" | "IFCASYMMETRICISHAPEPROFILEDEF" => {
                let (w, h, tw, tf) = (d(3) / 2.0, d(4) / 2.0, d(5) / 2.0, d(6));
                vec![(
                    vec![
                        DVec2::new(-w, -h),
                        DVec2::new(w, -h),
                        DVec2::new(w, -h + tf),
                        DVec2::new(tw, -h + tf),
                        DVec2::new(tw, h - tf),
                        DVec2::new(w, h - tf),
                        DVec2::new(w, h),
                        DVec2::new(-w, h),
                        DVec2::new(-w, h - tf),
                        DVec2::new(-tw, h - tf),
                        DVec2::new(-tw, -h + tf),
                        DVec2::new(-w, -h + tf),
                    ],
                    Vec::new(),
                )]
            }
            "IFCLSHAPEPROFILEDEF" => {
                let h = d(3);
                let w = a.get(4).and_then(|v| v.as_f64()).unwrap_or(h);
                let t = d(5);
                let (w2, h2) = (w / 2.0, h / 2.0);
                vec![(
                    vec![
                        DVec2::new(-w2, -h2),
                        DVec2::new(w2, -h2),
                        DVec2::new(w2, -h2 + t),
                        DVec2::new(-w2 + t, -h2 + t),
                        DVec2::new(-w2 + t, h2),
                        DVec2::new(-w2, h2),
                    ],
                    Vec::new(),
                )]
            }
            "IFCUSHAPEPROFILEDEF" | "IFCCSHAPEPROFILEDEF" => {
                let (h2, b2) = (d(3) / 2.0, d(4) / 2.0);
                let (tw, tf) = if ty == "IFCCSHAPEPROFILEDEF" { (d(5), d(5)) } else { (d(5), d(6)) };
                vec![(
                    vec![
                        DVec2::new(-b2, -h2),
                        DVec2::new(b2, -h2),
                        DVec2::new(b2, -h2 + tf),
                        DVec2::new(-b2 + tw, -h2 + tf),
                        DVec2::new(-b2 + tw, h2 - tf),
                        DVec2::new(b2, h2 - tf),
                        DVec2::new(b2, h2),
                        DVec2::new(-b2, h2),
                    ],
                    Vec::new(),
                )]
            }
            "IFCTSHAPEPROFILEDEF" => {
                let (h2, b2, tw, tf) = (d(3) / 2.0, d(4) / 2.0, d(5) / 2.0, d(6));
                vec![(
                    vec![
                        DVec2::new(-b2, h2),
                        DVec2::new(-b2, h2 - tf),
                        DVec2::new(-tw, h2 - tf),
                        DVec2::new(-tw, -h2),
                        DVec2::new(tw, -h2),
                        DVec2::new(tw, h2 - tf),
                        DVec2::new(b2, h2 - tf),
                        DVec2::new(b2, h2),
                    ],
                    Vec::new(),
                )]
            }
            "IFCZSHAPEPROFILEDEF" => {
                let (h2, b, tw, tf) = (d(3) / 2.0, d(4), d(5) / 2.0, d(6));
                vec![(
                    vec![
                        DVec2::new(-tw, -h2),
                        DVec2::new(b - tw, -h2),
                        DVec2::new(b - tw, -h2 + tf),
                        DVec2::new(tw, -h2 + tf),
                        DVec2::new(tw, h2),
                        DVec2::new(-b + tw, h2),
                        DVec2::new(-b + tw, h2 - tf),
                        DVec2::new(-tw, h2 - tf),
                    ],
                    Vec::new(),
                )]
            }
            other => {
                if !other.is_empty() {
                    self.warn.insert(format!("Unsupported profile type: {}", pretty_type(other)));
                }
                return Vec::new();
            }
        };
        // parameterised profiles: apply Position (arg 2)
        if let Some(p) = a.get(2).and_then(|v| v.as_ref()) {
            let m = self.axis2(p);
            for (o, hs) in res.iter_mut() {
                xf2(o, &m);
                hs.iter_mut().for_each(|h| xf2(h, &m));
            }
        }
        res
    }
}

// ================================================================ geometry utilities

fn to8(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn prefix(v: Option<&V>) -> f64 {
    match v.and_then(|v| v.as_enum()) {
        Some("MILLI") => 1e-3,
        Some("CENTI") => 1e-2,
        Some("DECI") => 1e-1,
        Some("KILO") => 1e3,
        Some("MICRO") => 1e-6,
        _ => 1.0,
    }
}

fn frame(loc: DVec3, z: DVec3, xref: DVec3) -> DMat4 {
    let z = z.normalize_or(DVec3::Z);
    let mut x = xref - z * xref.dot(z);
    if x.length_squared() < 1e-12 {
        let alt = if z.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
        x = alt - z * alt.dot(z);
    }
    let x = x.normalize();
    let y = z.cross(x);
    DMat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), loc.extend(1.0))
}

fn xf2(pts: &mut [DVec2], m: &DMat4) {
    for p in pts.iter_mut() {
        *p = m.transform_point3(p.extend(0.0)).truncate();
    }
}

fn rect(x: f64, y: f64) -> Vec<DVec2> {
    let (x, y) = (x / 2.0, y / 2.0);
    vec![DVec2::new(-x, -y), DVec2::new(x, -y), DVec2::new(x, y), DVec2::new(-x, y)]
}

fn circle(r: f64, n: usize) -> Vec<DVec2> {
    (0..n)
        .map(|i| {
            let t = 2.0 * PI * i as f64 / n as f64;
            DVec2::new(r * t.cos(), r * t.sin())
        })
        .collect()
}

fn append_curve(out: &mut Vec<DVec3>, seg: Vec<DVec3>) {
    for p in seg {
        if out.last().is_none_or(|l| (*l - p).length_squared() > 1e-16) {
            out.push(p);
        }
    }
}

fn dedup_ring(pts: &mut Vec<DVec3>) {
    pts.dedup_by(|a, b| (*a - *b).length_squared() < 1e-16);
    while pts.len() > 1 && (pts[0] - pts[pts.len() - 1]).length_squared() < 1e-16 {
        pts.pop();
    }
}

fn arc3(a: DVec3, b: DVec3, c: DVec3) -> Vec<DVec3> {
    let ab = b - a;
    let ac = c - a;
    let n = ab.cross(ac);
    let n2 = n.length_squared();
    if n2 < 1e-18 {
        return vec![a, b, c];
    }
    let o = a + (ab.length_squared() * ac.cross(n) + ac.length_squared() * n.cross(ab)) / (2.0 * n2);
    let r = (a - o).length();
    let e1 = (a - o) / r;
    let e2 = n.normalize().cross(e1);
    let mut th = (c - o).dot(e2).atan2((c - o).dot(e1));
    if th <= 0.0 {
        th += 2.0 * PI;
    }
    let k = ((th / (PI / 12.0)).ceil() as usize).max(2);
    (0..=k)
        .map(|i| {
            let t = th * i as f64 / k as f64;
            o + r * (t.cos() * e1 + t.sin() * e2)
        })
        .collect()
}

fn newell(p: &[DVec3]) -> DVec3 {
    let mut n = DVec3::ZERO;
    for i in 0..p.len() {
        let a = p[i];
        let b = p[(i + 1) % p.len()];
        n.x += (a.y - b.y) * (a.z + b.z);
        n.y += (a.z - b.z) * (a.x + b.x);
        n.z += (a.x - b.x) * (a.y + b.y);
    }
    n
}

/// Triangulates a planar 3D polygon with holes.
fn tri_poly(outer: &[DVec3], holes: &[Vec<DVec3>], col: Col, out: &mut Vec<Tri>) {
    if outer.len() < 3 {
        return;
    }
    if outer.len() == 3 && holes.is_empty() {
        out.push(([outer[0], outer[1], outer[2]], col));
        return;
    }
    let n = newell(outer);
    if n.length_squared() < 1e-24 {
        return;
    }
    let n = n.normalize();
    if outer.len() == 4 && holes.is_empty() {
        // convex quadrilateral: fast
        let ok = (0..4).all(|i| {
            let a = outer[i];
            let b = outer[(i + 1) % 4];
            let c = outer[(i + 2) % 4];
            (b - a).cross(c - b).dot(n) > 0.0
        });
        if ok {
            out.push(([outer[0], outer[1], outer[2]], col));
            out.push(([outer[0], outer[2], outer[3]], col));
            return;
        }
    }
    let u = if n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let u = (u - n * u.dot(n)).normalize();
    let v = n.cross(u);
    let o = outer[0];
    let mut flat: Vec<f64> = Vec::with_capacity((outer.len() + holes.iter().map(|h| h.len()).sum::<usize>()) * 2);
    let mut all: Vec<DVec3> = Vec::with_capacity(flat.capacity() / 2);
    let mut hole_idx = Vec::new();
    for p in outer {
        flat.push((*p - o).dot(u));
        flat.push((*p - o).dot(v));
        all.push(*p);
    }
    for h in holes {
        if h.len() < 3 {
            continue;
        }
        hole_idx.push(all.len());
        for p in h {
            flat.push((*p - o).dot(u));
            flat.push((*p - o).dot(v));
            all.push(*p);
        }
    }
    if let Ok(idx) = earcutr::earcut(&flat, &hole_idx, 2) {
        for t in idx.as_chunks::<3>().0 {
            out.push(([all[t[0]], all[t[1]], all[t[2]]], col));
        }
    }
}

fn extrude(outer: &[DVec2], holes: &[Vec<DVec2>], ext: DVec3, pos: &DMat4, col: Col, out: &mut Vec<Tri>) {
    if outer.len() < 3 {
        return;
    }
    let start = out.len();
    let o3: Vec<DVec3> = outer.iter().map(|p| p.extend(0.0)).collect();
    let h3: Vec<Vec<DVec3>> = holes.iter().map(|h| h.iter().map(|p| p.extend(0.0)).collect()).collect();
    tri_poly(&o3, &h3, col, out);
    let ncap = out.len() - start;
    for i in 0..ncap {
        let (t, c) = out[start + i];
        out.push(([t[0] + ext, t[2] + ext, t[1] + ext], c));
    }
    if ext.length_squared() > 1e-20 {
        for ring in std::iter::once(&o3).chain(h3.iter()) {
            for i in 0..ring.len() {
                let a = ring[i];
                let b = ring[(i + 1) % ring.len()];
                out.push(([a, b, b + ext], col));
                out.push(([a, b + ext, a + ext], col));
            }
        }
    }
    for t in out[start..].iter_mut() {
        for p in t.0.iter_mut() {
            *p = pos.transform_point3(*p);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn revolve(outer: &[DVec2], holes: &[Vec<DVec2>], aloc: DVec3, adir: DVec3, ang: f64, pos: &DMat4, col: Col, out: &mut Vec<Tri>) {
    if outer.len() < 3 || ang.abs() < 1e-9 {
        return;
    }
    let start = out.len();
    let steps = ((ang.abs() / (PI / 12.0)).ceil() as usize).max(2);
    let rot = |p: DVec3, t: f64| aloc + DQuat::from_axis_angle(adir, t) * (p - aloc);
    let o3: Vec<DVec3> = outer.iter().map(|p| p.extend(0.0)).collect();
    let h3: Vec<Vec<DVec3>> = holes.iter().map(|h| h.iter().map(|p| p.extend(0.0)).collect()).collect();
    for ring in std::iter::once(&o3).chain(h3.iter()) {
        for s in 0..steps {
            let t0 = ang * s as f64 / steps as f64;
            let t1 = ang * (s + 1) as f64 / steps as f64;
            for i in 0..ring.len() {
                let a = ring[i];
                let b = ring[(i + 1) % ring.len()];
                let (a0, b0, a1, b1) = (rot(a, t0), rot(b, t0), rot(a, t1), rot(b, t1));
                out.push(([a0, b0, b1], col));
                out.push(([a0, b1, a1], col));
            }
        }
    }
    if ang.abs() < 2.0 * PI - 1e-6 {
        let mut caps = Vec::new();
        tri_poly(&o3, &h3, col, &mut caps);
        for (t, c) in caps {
            out.push((t, c));
            out.push(([rot(t[0], ang), rot(t[1], ang), rot(t[2], ang)], c));
        }
    }
    for t in out[start..].iter_mut() {
        for p in t.0.iter_mut() {
            *p = pos.transform_point3(*p);
        }
    }
}

fn tube(path: &[DVec3], r: f64, col: Col, out: &mut Vec<Tri>) {
    if path.len() < 2 || r <= 0.0 {
        return;
    }
    const N: usize = 10;
    let mut rings: Vec<[DVec3; N]> = Vec::with_capacity(path.len());
    let mut nrm = DVec3::ZERO;
    for i in 0..path.len() {
        let t = if i == 0 {
            path[1] - path[0]
        } else if i + 1 == path.len() {
            path[i] - path[i - 1]
        } else {
            (path[i + 1] - path[i]).normalize_or_zero() + (path[i] - path[i - 1]).normalize_or_zero()
        };
        let t = t.normalize_or(DVec3::Z);
        if i == 0 {
            let alt = if t.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
            nrm = (alt - t * alt.dot(t)).normalize();
        } else {
            nrm = (nrm - t * nrm.dot(t)).normalize_or(nrm);
        }
        let b = t.cross(nrm);
        let mut ring = [DVec3::ZERO; N];
        for (k, p) in ring.iter_mut().enumerate() {
            let a = 2.0 * PI * k as f64 / N as f64;
            *p = path[i] + r * (a.cos() * nrm + a.sin() * b);
        }
        rings.push(ring);
    }
    for w in rings.windows(2) {
        for k in 0..N {
            let k2 = (k + 1) % N;
            out.push(([w[0][k], w[0][k2], w[1][k2]], col));
            out.push(([w[0][k], w[1][k2], w[1][k]], col));
        }
    }
    for (ring, c) in [(&rings[0], path[0]), (&rings[rings.len() - 1], path[path.len() - 1])] {
        for k in 0..N {
            out.push(([c, ring[k], ring[(k + 1) % N]], col));
        }
    }
}

/// Keeps the part with (p - o)·n >= 0.
fn clip_tris(tris: Vec<Tri>, o: DVec3, n: DVec3) -> Vec<Tri> {
    let mut out = Vec::with_capacity(tris.len());
    for (t, c) in tris {
        let d = [(t[0] - o).dot(n), (t[1] - o).dot(n), (t[2] - o).dot(n)];
        if d.iter().all(|x| *x >= -1e-9) {
            out.push((t, c));
            continue;
        }
        if d.iter().all(|x| *x <= 1e-9) {
            continue;
        }
        let mut poly: Vec<DVec3> = Vec::with_capacity(4);
        for i in 0..3 {
            let j = (i + 1) % 3;
            if d[i] >= 0.0 {
                poly.push(t[i]);
            }
            if (d[i] > 0.0 && d[j] < 0.0) || (d[i] < 0.0 && d[j] > 0.0) {
                let k = d[i] / (d[i] - d[j]);
                poly.push(t[i] + (t[j] - t[i]) * k);
            }
        }
        for i in 1..poly.len().saturating_sub(1) {
            out.push(([poly[0], poly[i], poly[i + 1]], c));
        }
    }
    out
}

fn default_color(ty: &str) -> [u8; 4] {
    let t = ty;
    let c: [u8; 4] = if t.contains("WINDOW") {
        [150, 200, 235, 110]
    } else if t.contains("PLATE") {
        [160, 205, 230, 130]
    } else if t.contains("SPACE") {
        [130, 200, 140, 70]
    } else if t.contains("WALL") {
        [222, 218, 205, 255]
    } else if t.contains("SLAB") || t.contains("FOOTING") || t.contains("PILE") {
        [178, 178, 178, 255]
    } else if t.contains("ROOF") {
        [170, 85, 70, 255]
    } else if t.contains("COLUMN") || t.contains("BEAM") || t.contains("MEMBER") {
        [150, 165, 190, 255]
    } else if t.contains("DOOR") {
        [140, 95, 60, 255]
    } else if t.contains("STAIR") || t.contains("RAMP") || t.contains("RAILING") {
        [130, 130, 135, 255]
    } else if t.contains("FURNI") {
        [185, 145, 105, 255]
    } else if t.contains("DUCT") || t.contains("AIRTERMINAL") {
        [110, 170, 215, 255]
    } else if t.contains("PIPE") || t.contains("FLOW") || t.contains("VALVE") || t.contains("PUMP") || t.contains("TANK") || t.contains("SANITARY") {
        [70, 160, 110, 255]
    } else if t.contains("CABLE") || t.contains("ELECTRIC") || t.contains("LIGHT") {
        [225, 170, 60, 255]
    } else if t.contains("SITE") || t.contains("GEOGRAPHIC") {
        [120, 155, 95, 255]
    } else if t.contains("COVERING") {
        [232, 230, 220, 255]
    } else if t.contains("REINFORC") || t.contains("TENDON") {
        [120, 90, 80, 255]
    } else {
        [195, 195, 195, 255]
    };
    c
}

const KNOWN_TYPES: &[&str] = &[
    "IfcWall", "IfcWallStandardCase", "IfcSlab", "IfcRoof", "IfcColumn", "IfcBeam", "IfcMember", "IfcPlate", "IfcDoor", "IfcWindow",
    "IfcStair", "IfcStairFlight", "IfcRamp", "IfcRampFlight", "IfcRailing", "IfcCovering", "IfcCurtainWall", "IfcFooting", "IfcPile",
    "IfcBuildingElementProxy", "IfcFurnishingElement", "IfcFurniture", "IfcSpace", "IfcSite", "IfcBuilding", "IfcBuildingStorey",
    "IfcFlowSegment", "IfcFlowFitting", "IfcFlowTerminal", "IfcFlowController", "IfcFlowMovingDevice", "IfcFlowStorageDevice",
    "IfcFlowTreatmentDevice", "IfcEnergyConversionDevice", "IfcDistributionChamberElement", "IfcDistributionControlElement",
    "IfcDuctSegment", "IfcDuctFitting", "IfcPipeSegment", "IfcPipeFitting", "IfcCableCarrierSegment", "IfcCableCarrierFitting",
    "IfcCableSegment", "IfcAirTerminal", "IfcSanitaryTerminal", "IfcLightFixture", "IfcValve", "IfcPump", "IfcTank", "IfcBoiler",
    "IfcReinforcingBar", "IfcReinforcingMesh", "IfcElementAssembly", "IfcDiscreteAccessory", "IfcMechanicalFastener", "IfcFastener",
    "IfcTransportElement", "IfcSystemFurnitureElement", "IfcPropertySet", "IfcElementQuantity", "IfcWallType", "IfcSlabType",
    "IfcColumnType", "IfcBeamType", "IfcDoorStyle", "IfcWindowStyle", "IfcDoorType", "IfcWindowType", "IfcMemberType", "IfcPlateType",
    "IfcCoveringType", "IfcRailingType", "IfcFurnitureType", "IfcBuildingElementProxyType", "IfcDuctSegmentType", "IfcPipeSegmentType",
    "IfcDuctFittingType", "IfcPipeFittingType", "IfcAdvancedBrep", "IfcOpeningElement", "IfcGeographicElement", "IfcCivilElement",
    "IfcUnitaryEquipment", "IfcElectricAppliance", "IfcElectricDistributionBoard", "IfcOutlet", "IfcSwitchingDevice", "IfcSensor",
    "IfcFireSuppressionTerminal", "IfcWasteTerminal", "IfcStackTerminal", "IfcChimney", "IfcShadingDevice", "IfcBuildingElementPart",
];

pub fn pretty_type(upper: &str) -> String {
    for k in KNOWN_TYPES {
        if k.eq_ignore_ascii_case(upper) {
            return k.to_string();
        }
    }
    if let Some(rest) = upper.strip_prefix("IFC") {
        let mut s = String::from("Ifc");
        let mut ch = rest.chars();
        if let Some(f) = ch.next() {
            s.push(f);
            s.push_str(&ch.as_str().to_ascii_lowercase());
        }
        s
    } else {
        upper.to_string()
    }
}

/// Subtracts a convex opening (given as triangles in real coordinates) from the host triangles: each triangle
/// is cut by the opening's planes (the part outside each plane stays, what is inside all of them disappears).
/// Returns false if the opening is not convex (then the host stays untouched).
pub fn subtract_convex(tris: &mut Vec<[DVec3; 3]>, colors: &mut Vec<[u8; 4]>, opening: &[[DVec3; 3]]) -> bool {
    if opening.len() < 4 {
        return true;
    }
    let verts: Vec<DVec3> = opening.iter().flat_map(|t| t.iter().copied()).collect();
    let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
    for v in &verts {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    let size = (hi - lo).length().max(1e-6);
    let eps = size * 1e-6;
    let c = verts.iter().copied().sum::<DVec3>() / verts.len() as f64;
    // the opening's planes, oriented outwards (the centre is inside)
    let mut planes: Vec<(DVec3, f64)> = Vec::new();
    for t in opening {
        let n = (t[1] - t[0]).cross(t[2] - t[0]);
        if n.length_squared() < 1e-24 {
            continue;
        }
        let mut n = n.normalize();
        let mut d = n.dot(t[0]);
        if n.dot(c) - d > 0.0 {
            n = -n;
            d = -d;
        }
        if !planes.iter().any(|(m, e)| m.dot(n) > 0.99999 && (e - d).abs() < eps * 10.0) {
            planes.push((n, d));
        }
    }
    if planes.len() < 4 || planes.len() > 64 {
        return false;
    }
    // convex: all the opening's vertices are inside (or on) each plane
    if verts.iter().any(|v| planes.iter().any(|(n, d)| n.dot(*v) - d > eps * 100.0 + size * 1e-4)) {
        return false;
    }
    let (blo, bhi) = (lo - DVec3::splat(eps), hi + DVec3::splat(eps));
    let mut out_t = Vec::with_capacity(tris.len());
    let mut out_c = Vec::with_capacity(colors.len());
    for (t, col) in tris.iter().zip(colors.iter()) {
        let (tlo, thi) = (t[0].min(t[1]).min(t[2]), t[0].max(t[1]).max(t[2]));
        if thi.x < blo.x || tlo.x > bhi.x || thi.y < blo.y || tlo.y > bhi.y || thi.z < blo.z || tlo.z > bhi.z {
            out_t.push(*t);
            out_c.push(*col);
            continue;
        }
        // what is outside plane k (and inside the previous planes) stays
        let mut remaining: Vec<Tri> = vec![(*t, Some(*col))];
        for (n, d) in &planes {
            let o = *n * *d;
            for (p, cc) in clip_tris(remaining.clone(), o, *n) {
                out_t.push(p);
                out_c.push(cc.unwrap_or(*col));
            }
            remaining = clip_tris(remaining, o, -*n);
            if remaining.is_empty() {
                break;
            }
        }
    }
    *tris = out_t;
    *colors = out_c;
    true
}
