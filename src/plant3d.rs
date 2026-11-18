//! Obiecte Autodesk Plant 3D (si alte entitati custom) din DWG, fara object enabler.
//!
//! acadrust citeste entitatile custom (ACPPPIPE, ACPPPIPEINLINEASSET, ACPPCONNECTOR, ACPPSUPPORT,
//! ACPPEQUIPMENT, ACPPSTRUCTURE*...) ca `UNKNOWN`, cu doua lucruri pastrate:
//! - `graphic_data` = grafica proxy (formatul AcGi: [marime][nr. inregistrari], apoi inregistrari
//!   [marime u32][tip u32][date]). Plant 3D scrie acolo, pentru fiecare piesa din catalog, o
//!   transformare (tip 29 PUSH_MODEL_XFORM, matrice 4x4 pe linii) urmata de conturul ei in coordonate
//!   locale (cercuri, polilinii) si POP (31). Teava dreapta e un MESH (8) 2 x N, grinzile SHELL (9),
//!   gratarele POLYGON (7).
//! - `raw_dwg_data` + `dwg_handle_bits` = inregistrarea DWG bruta; fluxul de handle-uri (ultimii
//!   `dwg_handle_bits` biti, R2010+) contine, dupa reactor / layer, referintele la blocurile cu geometria
//!   reala (solide ACIS): `Plant3DCatalogItem_*` (fitinguri, flanse, armaturi, stuturi) si `*U*` (corpul
//!   echipamentului, suporturi), blocuri pe care nu le insereaza niciun INSERT.
//!
//! Regula de plasare (verificata pe SampleProject: 2-P-001 si 6-B-1000_Equip, bbox-ul local al
//! conturului proxy == bbox-ul solidelor din bloc): a k-a referinta de bloc din fluxul de handle-uri
//! se plaseaza cu a k-a transformare de nivel 0 din grafica proxy. Push-urile imbricate (nivel >= 1)
//! descriu sub-piesele blocului (ex. INSERT-urile din `*U14`) si nu sunt folosite.
use acadrust::entities::EntityType;
use acadrust::types::Color;
use acadrust::CadDocument;
use glam::{DMat4, DVec3, DVec4};
use std::collections::HashMap;

/// Un obiect Plant 3D (entitate custom) gata de afisat.
#[derive(Clone, Debug, Default)]
pub struct PlantObject {
    /// handle hex (majuscule), ca grupul 5 din DXF
    pub handle: String,
    /// numele clasei DXF (ex. ACPPPIPE)
    pub class: String,
    pub layer: String,
    pub color: Color,
    /// blocurile cu geometrie si transformarea lor in coordonate lume (unitati desen)
    pub blocks: Vec<PlacedBlock>,
    /// triunghiuri din grafica proxy (teava, grinzi, gratare), in coordonate lume
    pub tris: Vec<[DVec3; 3]>,
    /// bbox-ul lume al intregii grafici proxy (contur inclus) - pentru verificari
    pub proxy_bbox: Option<(DVec3, DVec3)>,
    /// bbox-ul lume al conturului proxy din interiorul transformarilor (piesele plasate)
    pub placed_bbox: Option<(DVec3, DVec3)>,
    /// referinte de bloc fara transformare corespunzatoare (nu pot fi plasate)
    pub unplaced: usize,
}

/// O piesa din catalog plasata: blocul (nume + handle-ul BLOCK_RECORD, hex) si transformarea lui.
#[derive(Clone, Debug)]
pub struct PlacedBlock {
    pub name: String,
    /// handle-ul inregistrarii de bloc (hex); numele blocurilor anonime `*U*` se pot schimba la scrierea DXF
    pub record: String,
    pub xf: DMat4,
}

/// Numele afisat al clasei: ACPPPIPEINLINEASSET -> "Pipe Inline Asset".
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
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => class.to_string(),
    }
}

// ------------------------------------------------------------------ fluxul de handle-uri DWG
/// Handle-urile referite de o inregistrare DWG R2010+ (fluxul de handle-uri = ultimii `handle_bits`
/// biti). Referintele relative (coduri 6, 8, A, C) sunt rezolvate fata de `own`.
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

// ------------------------------------------------------------------ grafica proxy
/// Ce extragem din grafica proxy.
#[derive(Clone, Debug, Default)]
pub struct ProxyGeometry {
    /// transformarile de nivel 0 (PUSH_MODEL_XFORM fara alt push activ), in ordine
    pub placements: Vec<DMat4>,
    /// fete (MESH / SHELL / POLYGON) in coordonate lume
    pub tris: Vec<[DVec3; 3]>,
    /// bbox-ul lume al tuturor primitivelor (si contur)
    pub bbox: Option<(DVec3, DVec3)>,
    /// bbox-ul lume doar al primitivelor din interiorul transformarilor (= piesele din blocuri)
    pub placed_bbox: Option<(DVec3, DVec3)>,
    /// poliliniile (tip 6) in coordonate lume - conturul; scarile / balustradele Plant 3D au doar atat
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

/// Tub neted din inelele unui MESH 2 x N care descrie un cilindru (teava Plant 3D vine cu 8 laturi);
/// None daca reteaua nu e un cilindru circular drept.
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
        // normale spre exterior (x, y, z dreapta: a0 -> a1 -> b1 e anti-orar privit din afara)
        out.push([a0, a1, b1]);
        out.push([a0, b1, b0]);
    }
    Some(out)
}

/// SHELL de prisma "turtita": 2 inele cu aceleasi coordonate (prima jumatate a varfurilor == a doua),
/// intr-un plan. Asa scrie Plant 3D profilul grinzilor in grafica proxy.
fn collapsed_prism(w: &[DVec3]) -> bool {
    let n = w.len();
    if n < 6 || n % 2 != 0 {
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

/// Intinde prisma turtita de-a lungul celei mai lungi axe (segment) perpendiculare pe profil:
/// primul inel in planul capatului de start, al doilea in planul capatului de final.
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

/// Citeste grafica proxy (formatul AcGi). Ignora tipurile necunoscute; nu panicheaza pe date corupte.
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
            // CIRCLE: centru, raza, normala
            2 => {
                if let (Some(c), Some(r)) = (rd_p3(b, 0), rd_f64(b, 24)) {
                    pts.extend([c - DVec3::splat(r.abs()), c + DVec3::splat(r.abs())]);
                }
            }
            // CIRCLE3P / CIRCULARARC3P: 3 puncte
            3 | 5 => pts.extend(rd_pts(b, 0, 3).unwrap_or_default()),
            // CIRCULARARC: centru, raza, normala, ...
            4 => {
                if let (Some(c), Some(r)) = (rd_p3(b, 0), rd_f64(b, 24)) {
                    pts.extend([c - DVec3::splat(r.abs()), c + DVec3::splat(r.abs())]);
                }
            }
            // POLYLINE: contur (segmentele drepte = posibile axe de extrudare pentru profilele din SHELL)
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
            // POLYLINE_WITH_NORMALS (32): Plant 3D deseneaza asa porturile (segmente de lungime 0) - ignorat
            // POLYGON: fata plana (evantai)
            7 => {
                if let Some(k) = rd_u32(b, 0) {
                    let p = rd_pts(b, 4, k as usize).unwrap_or_default();
                    for i in 1..p.len().saturating_sub(1) {
                        faces.push([p[0], p[i], p[i + 1]]);
                    }
                    pts.extend(p);
                }
            }
            // MESH: rows x cols varfuri
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
            // SHELL: varfuri, apoi lista de fete [n, i1..in] (n < 0 = gaura, sarita)
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
                            // profil de grinda Plant 3D: inelele de jos / sus au aceleasi coordonate,
                            // lungimea vine din axa (polilinia cu 2 puncte) - rezolvat la final
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
            // PUSH_MODEL_XFORM: matrice 4x4 pe linii (translatia in coloana 4)
            29 => {
                let v: Option<Vec<f64>> = (0..16).map(|k| rd_f64(b, k * 8)).collect();
                if let Some(v) = v.filter(|v| v.iter().all(|x| x.is_finite())) {
                    let m = DMat4::from_cols(DVec4::new(v[0], v[4], v[8], v[12]), DVec4::new(v[1], v[5], v[9], v[13]), DVec4::new(v[2], v[6], v[10], v[14]), DVec4::new(v[3], v[7], v[11], v[15]));
                    if stack.is_empty() {
                        out.placements.push(m);
                    }
                    stack.push(cur);
                    cur = cur * m;
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

// ------------------------------------------------------------------ obiectele din document
/// Clasele Plant 3D desenate in grafica proxy doar din linii, dar care sunt obiecte fizice din tevi /
/// profile subtiri: le aratam ca tuburi pe linii (scara, balustrada, scara de trepte).
pub fn is_line_structure(class: &str) -> bool {
    let c = class.to_ascii_uppercase();
    c.starts_with("ACPPSTRUCTURE") && ["LADDER", "RAILING", "STAIR"].iter().any(|k| c.ends_with(k))
}

/// Unitatea desenului in metri ($INSUNITS); None daca nu e precizata.
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

/// Tuburi hexagonale de raza `r` pe segmentele poliliniilor (segmentele repetate o singura data).
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

/// Toate entitatile custom din model space care au geometrie: blocurile referite (plasate cu
/// transformarile din grafica proxy) si fetele din grafica proxy. Scarile / balustradele (doar linii)
/// devin tuburi subtiri; restul entitatilor doar cu contur (suduri, simboluri izometrice, grid) sunt omise.
pub fn collect(doc: &CadDocument) -> Vec<PlantObject> {
    // raza tuburilor: 20 mm in unitatile desenului (Plant 3D: de obicei inch)
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
            // fara unitati: ~ 1/400 din marimea obiectului
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

/// Grafica proxy sintetica (pentru teste / fixture-uri): inregistrari (tip, date) -> octeti AcGi.
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

/// Matrice -> date pentru inregistrarea PUSH_MODEL_XFORM (29), 16 double pe linii.
pub fn encode_xform(m: &DMat4) -> Vec<u8> {
    let t = m.transpose().to_cols_array();
    t.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// Flux de handle-uri DWG (referinte absolute, cod 5 = soft pointer) pentru teste / fixture-uri.
pub fn encode_handles(handles: &[u64]) -> Vec<u8> {
    let mut out = Vec::new();
    for &h in handles {
        let bytes: Vec<u8> = h.to_be_bytes().iter().copied().skip_while(|b| *b == 0).collect();
        out.push(0x50 | bytes.len() as u8);
        out.extend(bytes);
    }
    out
}
