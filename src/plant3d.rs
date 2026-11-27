//! Autodesk Plant 3D objects (and other custom entities) from DWG, without an object enabler.
//!
//! acadrust reads the custom entities (ACPPPIPE, ACPPPIPEINLINEASSET, ACPPCONNECTOR, ACPPSUPPORT,
//! ACPPEQUIPMENT, ACPPSTRUCTURE*...) as `UNKNOWN`, with two things kept:
//! - `graphic_data` = proxy graphics (the AcGi format: [size][no. of records], then records
//!   [size u32][type u32][data]). Plant 3D writes there, for each catalog part, a
//!   transform (type 29 PUSH_MODEL_XFORM, 4x4 row-major matrix) followed by its outline in local
//!   coordinates (circles, polylines) and POP (31). Straight pipe is a 2 x N MESH (8), beams SHELL (9),
//!   gratings POLYGON (7).
//! - `raw_dwg_data` + `dwg_handle_bits` = the raw DWG record; the handle stream (the last
//!   `dwg_handle_bits` bits, R2010+) contains, after reactor / layer, the references to the blocks with the
//!   real geometry (ACIS solids): `Plant3DCatalogItem_*` (fittings, flanges, valves, nozzles) and `*U*` (the
//!   equipment body, supports), blocks that no INSERT inserts.
//!
//! Placement rule (verified on SampleProject: 2-P-001 and 6-B-1000_Equip, the local bbox of the
//! proxy outline == the bbox of the solids in the block): the k-th block reference in the handle stream
//! is placed with the k-th level-0 transform in the proxy graphics. Nested pushes (level >= 1)
//! describe the sub-parts of the block (e.g. the INSERTs in `*U14`) and are not used.
use acadrust::entities::EntityType;
use acadrust::types::Color;
use acadrust::CadDocument;
use glam::{DMat4, DVec3, DVec4};
use std::collections::HashMap;

/// A Plant 3D object (custom entity) ready to display.
#[derive(Clone, Debug, Default)]
pub struct PlantObject {
    /// hex handle (uppercase), like group 5 in DXF
    pub handle: String,
    /// the DXF class name (e.g. ACPPPIPE)
    pub class: String,
    pub layer: String,
    pub color: Color,
    /// the blocks with geometry and their transform in world coordinates (drawing units)
    pub blocks: Vec<PlacedBlock>,
    /// triangles from the proxy graphics (pipe, beams, gratings), in world coordinates
    pub tris: Vec<[DVec3; 3]>,
    /// world bbox of the whole proxy graphics (outline included) - for checks
    pub proxy_bbox: Option<(DVec3, DVec3)>,
    /// world bbox of the proxy outline inside the transforms (the placed parts)
    pub placed_bbox: Option<(DVec3, DVec3)>,
    /// block references without a matching transform (cannot be placed)
    pub unplaced: usize,
}

/// A placed catalog part: the block (name + BLOCK_RECORD handle, hex) and its transform.
#[derive(Clone, Debug)]
pub struct PlacedBlock {
    pub name: String,
    /// handle of the block record (hex); the names of anonymous `*U*` blocks can change when writing DXF
    pub record: String,
    pub xf: DMat4,
}

/// Display name of the class: ACPPPIPEINLINEASSET -> "Pipe Inline Asset".
pub fn class_label(class: &str) -> String {
    let known = [
        ("ACPPPIPE", "Pipe"),
        ("ACPPPIPEINLINEASSET", "Pipe Inline Asset"),
        ("ACPPCONNECTOR", "Pipe Connector"),
        ("ACPPSUPPORT", "Pipe Support"),
        ("ACPPEQUIPMENT", "Equipment"),
        ("ACPPSTRUCTUREBEAM", "Structure Beam"),
        ("ACPPSTRUCTURECOLUMN", "Structure Column"),
        ("ACPPSTRUCTUREGRATING", "Structure Grating"),
        ("ACPPSTRUCTUREPLATE", "Structure Plate"),
        ("ACPPSTRUCTURERAILING", "Structure Railing"),
        ("ACPPSTRUCTURELADDER", "Structure Ladder"),
        ("ACPPSTRUCTURESTAIR", "Structure Stair"),
        ("ACPPSTRUCTUREFOOTING", "Structure Footing"),
    ];
    if let Some((_, l)) = known.iter().find(|(c, _)| c.eq_ignore_ascii_case(class)) {
        return l.to_string();
    }
    let s = class.strip_prefix("ACPP").unwrap_or(class);
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str().to_lowercase().as_str(),
        None => class.to_string(),
    }
}

// ------------------------------------------------------------------ DWG handle stream
/// The handles referenced by a DWG R2010+ record (the handle stream = the last `handle_bits`
/// bits). Relative references (codes 6, 8, A, C) are resolved against `own`.
pub fn handle_refs(raw: &[u8], handle_bits: i64, own: u64) -> Vec<u64> {
    let total = raw.len() * 8;
    if handle_bits <= 0 || handle_bits as usize > total {
        return Vec::new();
    }
    let mut p = total - handle_bits as usize;
    let bit = |p: &mut usize| -> u8 {
        let v = (raw[*p / 8] >> (7 - *p % 8)) & 1;
        *p += 1;
        v
    };
    let byte = |p: &mut usize| -> u8 {
        let mut v = 0u8;
        for _ in 0..8 {
            v = (v << 1) | bit(p);
        }
        v
    };
    let mut out = Vec::new();
    while p + 8 <= total {
        let x = byte(&mut p);
        let (code, n) = (x >> 4, (x & 15) as usize);
        if p + 8 * n > total {
            break;
        }
        let mut v = 0u64;
        for _ in 0..n {
            v = (v << 8) | byte(&mut p) as u64;
        }
        let abs = match code {
            6 => own.wrapping_add(1),
            8 => own.wrapping_sub(1),
            0xA => own.wrapping_add(v),
            0xC => own.wrapping_sub(v),
            _ => v,
        };
        if abs != 0 {
            out.push(abs);
        }
    }
    out
}

// ------------------------------------------------------------------ proxy graphics
/// What we extract from the proxy graphics.
#[derive(Clone, Debug, Default)]
pub struct ProxyGeometry {
    /// level-0 transforms (PUSH_MODEL_XFORM with no other active push), in order
    pub placements: Vec<DMat4>,
    /// faces (MESH / SHELL / POLYGON) in world coordinates
    pub tris: Vec<[DVec3; 3]>,
    /// world bbox of all primitives (outline too)
    pub bbox: Option<(DVec3, DVec3)>,
    /// world bbox only of the primitives inside the transforms (= the parts in blocks)
    pub placed_bbox: Option<(DVec3, DVec3)>,
    /// the polylines (type 6) in world coordinates - the outline; Plant 3D stairs / railings have only that
    pub polylines: Vec<Vec<DVec3>>,
}

fn rd_u32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn rd_i32(b: &[u8], o: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn rd_f64(b: &[u8], o: usize) -> Option<f64> {
    Some(f64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
}
fn rd_p3(b: &[u8], o: usize) -> Option<DVec3> {
    let p = DVec3::new(rd_f64(b, o)?, rd_f64(b, o + 8)?, rd_f64(b, o + 16)?);
    p.is_finite().then_some(p)
}
fn rd_pts(b: &[u8], o: usize, n: usize) -> Option<Vec<DVec3>> {
    if n > b.len() / 24 + 1 {
        return None;
    }
    (0..n).map(|i| rd_p3(b, o + i * 24)).collect()
}

/// Smooth tube from the rings of a 2 x N MESH that describes a cylinder (Plant 3D pipe comes with 8 sides);
/// None if the mesh is not a straight circular cylinder.
fn smooth_tube(r0: &[DVec3], r1: &[DVec3]) -> Option<Vec<[DVec3; 3]>> {
    let ring = |r: &[DVec3]| -> Option<(DVec3, f64, Vec<DVec3>)> {
        let mut v: Vec<DVec3> = r.to_vec();
        if v.len() > 3 && v[0].distance(v[v.len() - 1]) < 1e-9 * (1.0 + v[0].length()) {
            v.pop();
        }
        if v.len() < 5 {
            return None;
        }
        let c = v.iter().copied().sum::<DVec3>() / v.len() as f64;
        let rad = v.iter().map(|p| p.distance(c)).sum::<f64>() / v.len() as f64;
        (rad > 0.0 && v.iter().all(|p| (p.distance(c) - rad).abs() <= 1e-3 * rad)).then_some((c, rad, v))
    };
    let (c0, ra, v0) = ring(r0)?;
    let (c1, rb, _) = ring(r1)?;
    let axis = c1 - c0;
    let len = axis.length();
    if len <= 1e-9 || (ra - rb).abs() > 1e-3 * ra {
        return None;
    }
    let z = axis / len;
    if v0.iter().any(|p| (*p - c0).dot(z).abs() > 1e-3 * ra) {
        return None;
    }
    let x = (v0[0] - c0).normalize();
    let y = z.cross(x);
    let n = 32;
    let pt = |c: DVec3, i: usize| {
        let a = std::f64::consts::TAU * i as f64 / n as f64;
        c + (x * a.cos() + y * a.sin()) * ra
    };
    let mut out = Vec::with_capacity(2 * n);
    for i in 0..n {
        let (a0, a1, b0, b1) = (pt(c0, i), pt(c0, i + 1), pt(c1, i), pt(c1, i + 1));
        // outward normals (right-handed x, y, z: a0 -> a1 -> b1 is counter-clockwise seen from outside)
        out.push([a0, a1, b1]);
        out.push([a0, b1, b0]);
    }
    Some(out)
}

/// "Flattened" prism SHELL: 2 rings with the same coordinates (the first half of the vertices == the second),
/// in one plane. This is how Plant 3D writes the beam profile in the proxy graphics.
fn collapsed_prism(w: &[DVec3]) -> bool {
    let n = w.len();
    if n < 6 || !n.is_multiple_of(2) {
        return false;
    }
    let h = n / 2;
    let size = w.iter().fold(0.0f64, |m, p| m.max((*p - w[0]).length()));
    let eps = 1e-6 * (1.0 + size);
    size > 0.0 && (0..h).all(|i| w[i].distance(w[i + h]) <= eps) && newell(&w[..h]).length() > 0.0 && {
        let nrm = newell(&w[..h]).normalize();
        w.iter().all(|p| (*p - w[0]).dot(nrm).abs() <= eps)
    }
}

fn newell(p: &[DVec3]) -> DVec3 {
    let mut n = DVec3::ZERO;
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        n += DVec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
    }
    n
}

/// Stretches the flattened prism along the longest axis (segment) perpendicular to the profile:
/// the first ring in the plane of the start end, the second in the plane of the end.
fn extrude_collapsed(w: &[DVec3], lines: &[(DVec3, DVec3)]) -> Option<Vec<DVec3>> {
    let h = w.len() / 2;
    let nrm = newell(&w[..h]).normalize();
    let (a, c) = lines
        .iter()
        .filter(|(a, c)| (*c - *a).normalize().dot(nrm).abs() > 0.999)
        .max_by(|x, y| x.0.distance(x.1).total_cmp(&y.0.distance(y.1)))
        .copied()?;
    let d = (c - a).normalize();
    let len = a.distance(c);
    Some(w.iter().enumerate().map(|(i, p)| *p - d * (*p - a).dot(d) + if i < h { DVec3::ZERO } else { d * len }).collect())
}

/// Reads the proxy graphics (the AcGi format). Ignores unknown types; does not panic on corrupt data.
pub fn parse_proxy(g: &[u8]) -> ProxyGeometry {
    let mut out = ProxyGeometry::default();
    let Some(n) = rd_u32(g, 4) else { return out };
    let mut stack: Vec<DMat4> = Vec::new();
    let mut cur = DMat4::IDENTITY;
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    let (mut plo, mut phi) = (lo, hi);
    let mut lines: Vec<(DVec3, DVec3)> = Vec::new();
    let mut flat_shells: Vec<(Vec<DVec3>, Vec<Vec<usize>>)> = Vec::new();
    let mut o = 8usize;
    for _ in 0..n {
        let (Some(sz), Some(ty)) = (rd_u32(g, o), rd_u32(g, o + 4)) else { break };
        let sz = sz as usize;
        if sz < 8 || o + sz > g.len() {
            break;
        }
        let b = &g[o + 8..o + sz];
        o += sz;
        let mut pts: Vec<DVec3> = Vec::new();
        let mut faces: Vec<[DVec3; 3]> = Vec::new();
        match ty {
            // CIRCLE: center, radius, normal
            2 => {
                if let (Some(c), Some(r)) = (rd_p3(b, 0), rd_f64(b, 24)) {
                    pts.extend([c - DVec3::splat(r.abs()), c + DVec3::splat(r.abs())]);
                }
            }
            // CIRCLE3P / CIRCULARARC3P: 3 points
            3 | 5 => pts.extend(rd_pts(b, 0, 3).unwrap_or_default()),
            // CIRCULARARC: center, radius, normal, ...
            4 => {
                if let (Some(c), Some(r)) = (rd_p3(b, 0), rd_f64(b, 24)) {
                    pts.extend([c - DVec3::splat(r.abs()), c + DVec3::splat(r.abs())]);
                }
            }
            // POLYLINE: outline (the straight segments = possible extrusion axes for the SHELL profiles)
            6 => {
                if let Some(k) = rd_u32(b, 0) {
                    let p = rd_pts(b, 4, k as usize).unwrap_or_default();
                    if p.len() >= 2 {
                        out.polylines.push(p.iter().map(|q| cur.transform_point3(*q)).collect());
                    }
                    if p.len() == 2 {
                        let (a, c) = (cur.transform_point3(p[0]), cur.transform_point3(p[1]));
                        if a.distance(c) > 1e-9 {
                            lines.push((a, c));
                        }
                    }
                    pts.extend(p);
                }
            }
            // POLYLINE_WITH_NORMALS (32): Plant 3D draws the ports this way (zero-length segments) - ignored
            // POLYGON: planar face (fan)
            7 => {
                if let Some(k) = rd_u32(b, 0) {
                    let p = rd_pts(b, 4, k as usize).unwrap_or_default();
                    for i in 1..p.len().saturating_sub(1) {
                        faces.push([p[0], p[i], p[i + 1]]);
                    }
                    pts.extend(p);
                }
            }
            // MESH: rows x cols vertices
            8 => {
                if let (Some(r), Some(c)) = (rd_u32(b, 0), rd_u32(b, 4)) {
                    let (r, c) = (r as usize, c as usize);
                    if let Some(p) = r.checked_mul(c).and_then(|k| rd_pts(b, 8, k)) {
                        match (r == 2).then(|| smooth_tube(&p[..c], &p[c..])).flatten() {
                            Some(t) => faces.extend(t),
                            None => {
                                for i in 0..r.saturating_sub(1) {
                                    for j in 0..c.saturating_sub(1) {
                                        let (a, bb, cc, d) = (p[i * c + j], p[i * c + j + 1], p[(i + 1) * c + j + 1], p[(i + 1) * c + j]);
                                        faces.push([a, bb, cc]);
                                        faces.push([a, cc, d]);
                                    }
                                }
                            }
                        }
                        pts.extend(p);
                    }
                }
            }
            // SHELL: vertices, then the face list [n, i1..in] (n < 0 = hole, skipped)
            9 => {
                if let Some(k) = rd_u32(b, 0) {
                    let k = k as usize;
                    if let Some(p) = rd_pts(b, 4, k) {
                        let mut q = 4 + k * 24;
                        let mut polys: Vec<Vec<usize>> = Vec::new();
                        if let Some(m) = rd_u32(b, q) {
                            q += 4;
                            let end = q + 4 * m as usize;
                            while q + 4 <= end.min(b.len()) {
                                let Some(c) = rd_i32(b, q) else { break };
                                q += 4;
                                let cnt = c.unsigned_abs() as usize;
                                let idx: Vec<usize> = (0..cnt).filter_map(|i| rd_i32(b, q + 4 * i)).map(|i| i as usize).collect();
                                q += 4 * cnt;
                                if c > 0 && idx.len() >= 3 && idx.iter().all(|&i| i < p.len()) {
                                    polys.push(idx);
                                }
                            }
                        }
                        let w: Vec<DVec3> = p.iter().map(|v| cur.transform_point3(*v)).collect();
                        if collapsed_prism(&w) {
                            // Plant 3D beam profile: the bottom / top rings have the same coordinates,
                            // the length comes from the axis (the 2-point polyline) - resolved at the end
                            flat_shells.push((w, polys));
                        } else {
                            for idx in &polys {
                                for i in 1..idx.len() - 1 {
                                    faces.push([p[idx[0]], p[idx[i]], p[idx[i + 1]]]);
                                }
                            }
                        }
                        pts.extend(p);
                    }
                }
            }
            // PUSH_MODEL_XFORM: 4x4 row-major matrix (the translation in column 4)
            29 => {
                let v: Option<Vec<f64>> = (0..16).map(|k| rd_f64(b, k * 8)).collect();
                if let Some(v) = v.filter(|v| v.iter().all(|x| x.is_finite())) {
                    let m = DMat4::from_cols(DVec4::new(v[0], v[4], v[8], v[12]), DVec4::new(v[1], v[5], v[9], v[13]), DVec4::new(v[2], v[6], v[10], v[14]), DVec4::new(v[3], v[7], v[11], v[15]));
                    if stack.is_empty() {
                        out.placements.push(m);
                    }
                    stack.push(cur);
                    cur *= m;
                }
            }
            // POP_MODEL_XFORM
            31 => {
                if let Some(m) = stack.pop() {
                    cur = m;
                }
            }
            _ => {}
        }
        for p in pts {
            let w = cur.transform_point3(p);
            lo = lo.min(w);
            hi = hi.max(w);
            if !stack.is_empty() {
                plo = plo.min(w);
                phi = phi.max(w);
            }
        }
        out.tris.extend(faces.into_iter().map(|t| t.map(|p| cur.transform_point3(p))));
    }
    for (w, polys) in flat_shells {
        let w = extrude_collapsed(&w, &lines).unwrap_or(w);
        for idx in &polys {
            for i in 1..idx.len() - 1 {
                let t = [w[idx[0]], w[idx[i]], w[idx[i + 1]]];
                for p in t {
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
                out.tris.push(t);
            }
        }
    }
    if lo.x <= hi.x {
        out.bbox = Some((lo, hi));
    }
    if plo.x <= phi.x {
        out.placed_bbox = Some((plo, phi));
    }
    out
}

// ------------------------------------------------------------------ the objects in the document
/// Plant 3D classes drawn in the proxy graphics only from lines, but which are physical objects made of pipes /
/// thin profiles: we show them as tubes along the lines (ladder, railing, stair).
pub fn is_line_structure(class: &str) -> bool {
    let c = class.to_ascii_uppercase();
    c.starts_with("ACPPSTRUCTURE") && ["LADDER", "RAILING", "STAIR"].iter().any(|k| c.ends_with(k))
}

/// The drawing unit in meters ($INSUNITS); None if not specified.
fn unit_m(insunits: i16) -> Option<f64> {
    match insunits {
        1 => Some(0.0254),
        2 => Some(0.3048),
        4 => Some(0.001),
        5 => Some(0.01),
        6 => Some(1.0),
        _ => None,
    }
}

/// Hexagonal tubes of radius `r` on the polyline segments (repeated segments only once).
pub fn line_tubes(polylines: &[Vec<DVec3>], r: f64) -> Vec<[DVec3; 3]> {
    let key = |p: DVec3| ((p.x / r * 4.0).round() as i64, (p.y / r * 4.0).round() as i64, (p.z / r * 4.0).round() as i64);
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for pl in polylines {
        for w in pl.windows(2) {
            let (a, b) = (w[0], w[1]);
            let d = b - a;
            if d.length() <= r * 0.5 {
                continue;
            }
            let (ka, kb) = (key(a), key(b));
            if !seen.insert(if ka <= kb { (ka, kb) } else { (kb, ka) }) {
                continue;
            }
            let z = d.normalize();
            let x = z.any_orthonormal_vector();
            let y = z.cross(x);
            let ring = |c: DVec3, i: usize| {
                let t = std::f64::consts::TAU * i as f64 / 6.0;
                c + (x * t.cos() + y * t.sin()) * r
            };
            for i in 0..6 {
                let (a0, a1, b0, b1) = (ring(a, i), ring(a, i + 1), ring(b, i), ring(b, i + 1));
                out.push([a0, a1, b1]);
                out.push([a0, b1, b0]);
            }
        }
    }
    out
}

/// All custom entities in model space that have geometry: the referenced blocks (placed with
/// the transforms from the proxy graphics) and the faces from the proxy graphics. Stairs / railings (lines only)
/// become thin tubes; the remaining outline-only entities (welds, isometric symbols, grid) are skipped.
pub fn collect(doc: &CadDocument) -> Vec<PlantObject> {
    // tube radius: 20 mm in drawing units (Plant 3D: usually inches)
    let tube_r = unit_m(doc.header.insertion_units).map(|u| 0.02 / u);
    let blocks: HashMap<u64, &str> = doc
        .block_records
        .iter()
        .filter(|b| !b.name.to_ascii_lowercase().starts_with("*model_space") && !b.name.to_ascii_lowercase().starts_with("*paper_space"))
        .map(|b| (b.handle.value(), b.name.as_str()))
        .collect();
    let mut out = Vec::new();
    for e in doc.model_space_entities() {
        let EntityType::Unknown(u) = e else { continue };
        let Some(g) = u.common.graphic_data.as_deref() else { continue };
        let px = parse_proxy(g);
        let own = u.common.handle.value();
        let refs: Vec<(u64, &str)> = u.raw_dwg_data.as_deref().map(|raw| handle_refs(raw, u.dwg_handle_bits, own)).unwrap_or_default().iter().filter_map(|h| blocks.get(h).map(|n| (*h, *n))).collect();
        let placed = refs.len().min(px.placements.len());
        let mut tris = px.tris;
        if refs.is_empty() && tris.is_empty() && is_line_structure(&u.dxf_name) {
            // no units: ~ 1/400 of the object size
            let r = tube_r.or_else(|| px.bbox.map(|(lo, hi)| (hi - lo).max_element() / 400.0)).filter(|r| *r > 0.0);
            if let Some(r) = r {
                tris = line_tubes(&px.polylines, r);
            }
        }
        let o = PlantObject {
            handle: format!("{own:X}"),
            class: u.dxf_name.clone(),
            layer: if u.common.layer.is_empty() { "0".into() } else { u.common.layer.clone() },
            color: u.common.color,
            blocks: refs.iter().zip(px.placements.iter()).map(|((h, n), m)| PlacedBlock { name: n.to_string(), record: format!("{h:X}"), xf: *m }).collect(),
            tris,
            proxy_bbox: px.bbox,
            placed_bbox: px.placed_bbox,
            unplaced: refs.len() - placed,
        };
        if !o.blocks.is_empty() || !o.tris.is_empty() {
            out.push(o);
        }
    }
    out
}

/// Synthetic proxy graphics (for tests / fixtures): records (type, data) -> AcGi bytes.
pub fn encode_proxy(records: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut body = Vec::new();
    for (ty, d) in records {
        body.extend(((d.len() + 8) as u32).to_le_bytes());
        body.extend(ty.to_le_bytes());
        body.extend(d);
    }
    let mut g = ((body.len() + 8) as u32).to_le_bytes().to_vec();
    g.extend((records.len() as u32).to_le_bytes());
    g.extend(body);
    g
}

/// Matrix -> data for the PUSH_MODEL_XFORM record (29), 16 doubles row-major.
pub fn encode_xform(m: &DMat4) -> Vec<u8> {
    let t = m.transpose().to_cols_array();
    t.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// DWG handle stream (absolute references, code 5 = soft pointer) for tests / fixtures.
pub fn encode_handles(handles: &[u64]) -> Vec<u8> {
    let mut out = Vec::new();
    for &h in handles {
        let bytes: Vec<u8> = h.to_be_bytes().iter().copied().skip_while(|b| *b == 0).collect();
        out.push(0x50 | bytes.len() as u8);
        out.extend(bytes);
    }
    out
}

/// The area of a drawing in a Plant 3D project: its folders under the project root (the one with Project.xml),
/// without "Plant 3D Models" (e.g. "Area_2"). None outside a project or for drawings in the models root.
pub fn project_area(dwg: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut d = std::path::Path::new(dwg).parent();
    while let Some(dir) = d {
        if parts.len() > 8 {
            return None;
        }
        if dir.join("Project.xml").is_file() {
            parts.reverse();
            let keep: Vec<String> = parts.into_iter().filter(|p| !p.replace(' ', "_").eq_ignore_ascii_case("Plant_3D_Models")).collect();
            return if keep.is_empty() { None } else { Some(keep.join(" / ")) };
        }
        parts.push(dir.file_name()?.to_string_lossy().to_string());
        d = dir.parent();
    }
    None
}

#[cfg(test)]
mod area_tests {
    #[test]
    fn plant_area_from_project_folders() {
        let root = std::env::temp_dir().join(format!("mb_p3d_{}", std::process::id()));
        let area = root.join("Plant_3D_Models").join("Area_2");
        std::fs::create_dir_all(&area).unwrap();
        std::fs::write(root.join("Project.xml"), "<x/>").unwrap();
        let dwg = area.join("2-P-001.dwg");
        assert_eq!(super::project_area(dwg.to_str().unwrap()).as_deref(), Some("Area_2"));
        assert_eq!(super::project_area(root.join("Plant_3D_Models").join("a.dwg").to_str().unwrap()), None);
        assert_eq!(super::project_area(std::env::temp_dir().join("x.dwg").to_str().unwrap()), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
