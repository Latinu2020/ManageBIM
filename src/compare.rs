//! Compare (like in Navisworks): two versions of the same model, matched by GUID.
//! Result: elements deleted, added, with changed geometry, with changed properties, identical.
use crate::model::{ElemId, Element, Model};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Diff {
    /// only in the old version (A) = deleted
    Removed,
    /// only in the new version (B) = added
    Added,
    /// same GUID, different geometry (moved / resized / remodeled)
    Geometry,
    /// same GUID and geometry, different properties
    Properties,
    Same,
}

impl Diff {
    pub const ALL: [Diff; 5] = [Diff::Removed, Diff::Added, Diff::Geometry, Diff::Properties, Diff::Same];
    pub fn label(self) -> &'static str {
        match self {
            Diff::Removed => "Removed (only in A)",
            Diff::Added => "Added (only in B)",
            Diff::Geometry => "Geometry changed",
            Diff::Properties => "Properties changed",
            Diff::Same => "Unchanged",
        }
    }
    pub fn color(self) -> [u8; 4] {
        match self {
            Diff::Removed => [225, 50, 45, 255],
            Diff::Added => [45, 190, 75, 255],
            Diff::Geometry => [245, 150, 30, 255],
            Diff::Properties => [70, 140, 255, 255],
            Diff::Same => [150, 150, 155, 40],
        }
    }
}

/// Properties that change on every export without the element having changed.
const VOLATILE: [&str; 1] = ["STEP Id"];

pub struct CompareResult {
    pub a: usize,
    pub b: usize,
    /// each element from A and from B with its state (the matched elements from A appear as Same/Geometry/Properties)
    pub map: HashMap<ElemId, Diff>,
    /// element from A that has a pair in B (hidden in the view, so they do not overlap)
    pub matched_a: HashMap<ElemId, ElemId>,
    /// for the changed elements from B: what changed, briefly
    pub changes: BTreeMap<ElemId, Vec<String>>,
    pub counts: [usize; 5],
    pub tol_mm: f32,
    /// the geometric deviation of the changed elements (side B): how far each surface moved
    pub dev: HashMap<ElemId, ElemDev>,
}

/// The geometric deviation of an element between versions (meters).
#[derive(Clone, Debug, Default)]
pub struct ElemDev {
    /// the largest distance between the surfaces A and B (in both directions)
    pub max: f32,
    /// the average over the triangles of B
    pub mean: f32,
    /// for each triangle of B: the largest distance of its vertices to the surface of A
    pub tri: Vec<f32>,
    /// the displacement of the center (B - A)
    pub moved: [f32; 3],
}

/// The deviation map color: 0 = green (in place), 0.5 = yellow, 1 = red (at or above the scale).
pub fn heat_color(t: f32) -> [u8; 4] {
    let t = t.clamp(0.0, 1.0);
    let (g, y, r) = ([40.0, 175.0, 90.0], [245.0, 210.0, 50.0], [225.0, 45.0, 40.0]);
    let mix = |a: [f32; 3], b: [f32; 3], k: f32| [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k];
    let c = if t < 0.5 { mix(g, y, t * 2.0) } else { mix(y, r, (t - 0.5) * 2.0) };
    [c[0] as u8, c[1] as u8, c[2] as u8, 255]
}

/// How far each triangle of `eb` is from the surface of `ea` (and vice versa, for the deleted parts).
pub fn deviation(ma: &Model, ea: &Element, mb: &Model, eb: &Element) -> ElemDev {
    let (ta, tb) = (ma.tris(ea), mb.tris(eb));
    let cap = 1000.0;
    let mut tri = Vec::with_capacity(tb.len() / 3);
    for t in tb.as_chunks::<3>().0 {
        let d = t.iter().map(|p| ea.bvh.nearest(ta, *p, cap)).fold(0.0f32, f32::max);
        tri.push(d);
    }
    // the reverse direction: the vertices of A that no longer have a surface in B (part cut / moved)
    let mut back = 0.0f32;
    let step = (ta.len() / 3 / 20_000).max(1);
    for t in ta.as_chunks::<3>().0.iter().step_by(step) {
        for p in t {
            back = back.max(eb.bvh.nearest(tb, *p, cap));
        }
    }
    let max = tri.iter().copied().fold(back, f32::max);
    let mean = if tri.is_empty() { 0.0 } else { tri.iter().sum::<f32>() / tri.len() as f32 };
    let mv = eb.aabb.center() - ea.aabb.center();
    ElemDev { max, mean, tri, moved: mv.to_array() }
}

impl CompareResult {
    /// we count once per pair (side B) + the deleted elements (side A)
    fn recount(&mut self) {
        self.counts = [0; 5];
        for (id, d) in &self.map {
            if *d == Diff::Removed || id.m as usize == self.b {
                self.counts[Diff::ALL.iter().position(|x| x == d).unwrap()] += 1;
            }
        }
    }

    pub fn ids(&self, d: Diff) -> Vec<ElemId> {
        let mut v: Vec<ElemId> = self.map.iter().filter(|(id, x)| **x == d && (d == Diff::Removed || id.m as usize == self.b)).map(|(id, _)| *id).collect();
        v.sort();
        v
    }
}

fn geometry_changed(a: &Element, b: &Element, tol: f32) -> Option<String> {
    if a.tri_count != b.tri_count {
        return Some(format!("Geometry: {} → {} triangles", a.tri_count, b.tri_count));
    }
    let dmin = (a.aabb.min - b.aabb.min).abs().max_element();
    let dmax = (a.aabb.max - b.aabb.max).abs().max_element();
    if dmin > tol || dmax > tol {
        let mv = b.aabb.center() - a.aabb.center();
        let (sa, sb) = (a.aabb.size(), b.aabb.size());
        if (sa - sb).abs().max_element() <= tol {
            return Some(format!("Moved {:.0} mm (dx {:.0}, dy {:.0}, dz {:.0})", mv.length() * 1000.0, mv.x * 1000.0, mv.y * 1000.0, mv.z * 1000.0));
        }
        return Some(format!("Size {:.2}×{:.2}×{:.2} → {:.2}×{:.2}×{:.2} m", sa.x, sa.y, sa.z, sb.x, sb.y, sb.z));
    }
    None
}

fn prop_changes(a: &Element, b: &Element) -> Vec<String> {
    let key = |e: &Element| -> BTreeMap<(String, String), String> {
        e.props.iter().filter(|p| !VOLATILE.contains(&p.name.as_str())).map(|p| ((p.group.clone(), p.name.clone()), p.value.clone())).collect()
    };
    let (pa, pb) = (key(a), key(b));
    let mut out = Vec::new();
    for (k, va) in &pa {
        match pb.get(k) {
            Some(vb) if vb != va => out.push(format!("{} / {}: {va} → {vb}", k.0, k.1)),
            None => out.push(format!("{} / {}: {va} → (removed)", k.0, k.1)),
            _ => {}
        }
    }
    for (k, vb) in &pb {
        if !pa.contains_key(k) {
            out.push(format!("{} / {}: (new) {vb}", k.0, k.1));
        }
    }
    if a.name != b.name {
        out.insert(0, format!("Name: {} → {}", a.name, b.name));
    }
    out
}

/// Compares model `ma` (slot a, the old version) with `mb` (slot b, the new version).
/// Both must be in the same local coordinates (the same scene origin).
pub fn compare(ma: &Model, a: usize, mb: &Model, b: usize, tol_mm: f32) -> CompareResult {
    let tol = tol_mm.max(0.0) / 1000.0;
    fn by_guid(m: &Model) -> HashMap<&str, usize> {
        m.elements.iter().enumerate().filter(|(_, e)| !e.guid.is_empty()).map(|(i, e)| (e.guid.as_str(), i)).collect()
    }
    fn put(r: &mut CompareResult, id: ElemId, d: Diff) {
        r.map.insert(id, d);
    }
    let (ga, gb) = (by_guid(ma), by_guid(mb));
    let mut r = CompareResult { a, b, map: HashMap::new(), matched_a: HashMap::new(), changes: BTreeMap::new(), counts: [0; 5], tol_mm, dev: HashMap::new() };
    for (ia, ea) in ma.elements.iter().enumerate() {
        let id_a = ElemId { m: a as u32, e: ia as u32 };
        match gb.get(ea.guid.as_str()).filter(|_| !ea.guid.is_empty()) {
            None => put(&mut r, id_a, Diff::Removed),
            Some(&ib) => {
                let eb = &mb.elements[ib];
                let id_b = ElemId { m: b as u32, e: ib as u32 };
                let mut why = Vec::new();
                // same box and same triangle count: the vertices may still be moved (remodeled inside)
                let reshaped = || -> Option<String> {
                    if ea.tri_count != eb.tri_count {
                        return None;
                    }
                    let (pa, pb) = (ma.tris(ea), mb.tris(eb));
                    let d = pa.iter().zip(pb).map(|(x, y)| (*x - *y).length()).fold(0.0f32, f32::max);
                    (d > tol).then(|| format!("Reshaped: vertices moved up to {:.0} mm", d * 1000.0))
                };
                let d = if let Some(g) = geometry_changed(ea, eb, tol).or_else(reshaped) {
                    why.push(g);
                    // how far each surface moved (for the deviation map)
                    let dv = deviation(ma, ea, mb, eb);
                    why.push(format!("Max deviation {:.0} mm (mean {:.0} mm)", dv.max * 1000.0, dv.mean * 1000.0));
                    r.dev.insert(id_b, dv);
                    why.extend(prop_changes(ea, eb));
                    Diff::Geometry
                } else {
                    why = prop_changes(ea, eb);
                    if why.is_empty() { Diff::Same } else { Diff::Properties }
                };
                put(&mut r, id_a, d);
                put(&mut r, id_b, d);
                r.matched_a.insert(id_a, id_b);
                if !why.is_empty() {
                    r.changes.insert(id_b, why);
                }
            }
        }
    }
    for (ib, eb) in mb.elements.iter().enumerate() {
        if eb.guid.is_empty() || !ga.contains_key(eb.guid.as_str()) {
            put(&mut r, ElemId { m: b as u32, e: ib as u32 }, Diff::Added);
        }
    }
    r.recount();
    r
}

// ====================================================================== export / import

/// A changed element, identified by GUID (independent of the order in the file).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompareItem {
    pub guid: String,
    /// "A" (the old version: deleted elements) or "B" (the new version)
    pub side: String,
    pub diff: Diff,
    pub name: String,
    pub ifc_type: String,
    pub level: String,
    #[serde(default)]
    pub changes: Vec<String>,
}

/// .bimcmp file: the result of a comparison, reusable on the same two versions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompareFile {
    pub app: String,
    pub kind: String,
    pub version: u32,
    pub a_name: String,
    pub b_name: String,
    pub tol_mm: f32,
    pub created: String,
    pub counts: [usize; 5],
    /// only the changed elements; the rest of the elements from B are "Unchanged"
    pub items: Vec<CompareItem>,
}

pub const FILE_KIND: &str = "compare";

fn item(e: &Element, side: &str, d: Diff, changes: Vec<String>) -> CompareItem {
    CompareItem { guid: e.guid.clone(), side: side.into(), diff: d, name: e.name.clone(), ifc_type: e.ifc_type.clone(), level: e.storey.clone(), changes }
}

impl CompareResult {
    pub fn to_file(&self, ma: &Model, mb: &Model, created: String) -> CompareFile {
        let mut items = Vec::new();
        for d in [Diff::Removed, Diff::Added, Diff::Geometry, Diff::Properties] {
            for id in self.ids(d) {
                let m = if id.m as usize == self.a { ma } else { mb };
                let Some(e) = m.elements.get(id.e as usize) else { continue };
                let side = if id.m as usize == self.a { "A" } else { "B" };
                items.push(item(e, side, d, self.changes.get(&id).cloned().unwrap_or_default()));
            }
        }
        CompareFile { app: "ManageBIM".into(), kind: FILE_KIND.into(), version: 1, a_name: ma.name.clone(), b_name: mb.name.clone(), tol_mm: self.tol_mm, created, counts: self.counts, items }
    }

    /// CSV report for Excel (UTF-8 BOM, separator ';').
    pub fn to_csv(&self, ma: &Model, mb: &Model) -> String {
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut s = String::from("\u{feff}Status;GlobalId;Name;IFC Type;Level;Model;Changes\n");
        for it in self.to_file(ma, mb, String::new()).items {
            let model = if it.side == "A" { &ma.name } else { &mb.name };
            s.push_str(&format!("{};{};{};{};{};{};{}\n", q(it.diff.label()), q(&it.guid), q(&it.name), q(&it.ifc_type), q(&it.level), q(model), q(&it.changes.join(" | "))));
        }
        s
    }
}

impl CompareFile {
    pub fn parse(text: &str) -> Result<CompareFile, String> {
        let f: CompareFile = serde_json::from_str(text).map_err(|e| format!("Invalid comparison file: {e}"))?;
        if f.kind != FILE_KIND {
            return Err(format!("This is not a ManageBIM comparison file (kind \"{}\").", f.kind));
        }
        Ok(f)
    }

    /// Rebuilds the result on the loaded models (matching by GUID).
    /// Returns the result and how many elements from the file were not found in the models.
    pub fn apply(&self, ma: &Model, a: usize, mb: &Model, b: usize) -> (CompareResult, usize) {
        fn by_guid(m: &Model) -> HashMap<&str, u32> {
            m.elements.iter().enumerate().filter(|(_, e)| !e.guid.is_empty()).map(|(i, e)| (e.guid.as_str(), i as u32)).collect()
        }
        let (ga, gb) = (by_guid(ma), by_guid(mb));
        let mut r = CompareResult { a, b, map: HashMap::new(), matched_a: HashMap::new(), changes: BTreeMap::new(), counts: [0; 5], tol_mm: self.tol_mm, dev: HashMap::new() };
        let mut missing = 0;
        for it in &self.items {
            let (side, slot, g) = if it.side == "A" { ("A", a, &ga) } else { ("B", b, &gb) };
            let Some(&e) = g.get(it.guid.as_str()) else {
                missing += 1;
                continue;
            };
            let id = ElemId { m: slot as u32, e };
            r.map.insert(id, it.diff);
            if side == "B" && !it.changes.is_empty() {
                r.changes.insert(id, it.changes.clone());
            }
        }
        // the A <-> B pairs (we hide the old side) and the rest of the elements from B = unchanged
        for (ib, eb) in mb.elements.iter().enumerate() {
            let id_b = ElemId { m: b as u32, e: ib as u32 };
            let d = *r.map.entry(id_b).or_insert(Diff::Same);
            if d != Diff::Added {
                if let Some(&ea) = ga.get(eb.guid.as_str()).filter(|_| !eb.guid.is_empty()) {
                    let id_a = ElemId { m: a as u32, e: ea };
                    r.map.insert(id_a, d);
                    r.matched_a.insert(id_a, id_b);
                }
            }
        }
        // the deviation map also for a comparison loaded from a file
        let pairs: Vec<(ElemId, ElemId)> = r.matched_a.iter().map(|(a, b)| (*a, *b)).collect();
        for (ia, ib) in pairs {
            if r.map.get(&ib) == Some(&Diff::Geometry) {
                if let (Some(ea), Some(eb)) = (ma.elements.get(ia.e as usize), mb.elements.get(ib.e as usize)) {
                    r.dev.insert(ib, deviation(ma, ea, mb, eb));
                }
            }
        }
        r.recount();
        (r, missing)
    }
}

// ====================================================================== by parameters

/// A property changed on an element (side B); empty `old` / `new` = added / deleted.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamChange {
    pub id: ElemId,
    /// "Group / Name" (or "Name" for the element name)
    pub param: String,
    pub old: String,
    pub new: String,
}

/// All the properties changed between the matched pairs (A -> B), one per row.
pub fn param_changes(ma: &Model, mb: &Model, r: &CompareResult) -> Vec<ParamChange> {
    let mut out = Vec::new();
    let mut pairs: Vec<(&ElemId, &ElemId)> = r.matched_a.iter().collect();
    pairs.sort();
    for (ia, ib) in pairs {
        let (Some(a), Some(b)) = (ma.elements.get(ia.e as usize), mb.elements.get(ib.e as usize)) else { continue };
        if a.name != b.name {
            out.push(ParamChange { id: *ib, param: "Name".into(), old: a.name.clone(), new: b.name.clone() });
        }
        let key = |e: &Element| -> BTreeMap<String, String> { e.props.iter().filter(|p| !VOLATILE.contains(&p.name.as_str())).map(|p| (format!("{} / {}", p.group, p.name), p.value.clone())).collect() };
        let (pa, pb) = (key(a), key(b));
        for (k, va) in &pa {
            match pb.get(k) {
                Some(vb) if vb != va => out.push(ParamChange { id: *ib, param: k.clone(), old: va.clone(), new: vb.clone() }),
                None => out.push(ParamChange { id: *ib, param: k.clone(), old: va.clone(), new: String::new() }),
                _ => {}
            }
        }
        for (k, vb) in &pb {
            if !pa.contains_key(k) {
                out.push(ParamChange { id: *ib, param: k.clone(), old: String::new(), new: vb.clone() });
            }
        }
    }
    out
}

/// The changed parameters, with the number of affected elements (the most frequent first).
pub fn param_summary(ch: &[ParamChange]) -> Vec<(String, usize)> {
    let mut m: BTreeMap<&str, std::collections::BTreeSet<ElemId>> = BTreeMap::new();
    for c in ch {
        m.entry(c.param.as_str()).or_default().insert(c.id);
    }
    let mut v: Vec<(String, usize)> = m.into_iter().map(|(k, s)| (k.to_string(), s.len())).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

/// CSV report by parameters (Excel): GUID; Element; Type; Parameter; Old value; New value.
pub fn param_csv(ch: &[ParamChange], mb: &Model) -> String {
    let q = |s: &str| if s.contains(';') || s.contains('"') || s.contains('\n') { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() };
    let mut s = String::from("\u{feff}GUID;Element;Type;Parameter;Old value;New value\n");
    for c in ch {
        let (g, n, t) = mb.elements.get(c.id.e as usize).map(|e| (e.guid.as_str(), e.name.as_str(), e.ifc_type.as_str())).unwrap_or(("", "", ""));
        s.push_str(&format!("{};{};{};{};{};{}\n", q(g), q(n), q(t), q(&c.param), q(&c.old), q(&c.new)));
    }
    s
}
