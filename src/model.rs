//! Structurile de date comune: model federat, elemente, BVH.
use glam::{DVec3, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub const EMPTY: Aabb = Aabb { min: Vec3::splat(f32::INFINITY), max: Vec3::splat(f32::NEG_INFINITY) };
    pub fn grow(&mut self, p: Vec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }
    pub fn union(&mut self, o: &Aabb) {
        self.min = self.min.min(o.min);
        self.max = self.max.max(o.max);
    }
    pub fn is_valid(&self) -> bool {
        self.min.x <= self.max.x
    }
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }
    pub fn overlaps(&self, o: &Aabb, tol: f32) -> bool {
        self.min.x <= o.max.x + tol
            && self.max.x + tol >= o.min.x
            && self.min.y <= o.max.y + tol
            && self.max.y + tol >= o.min.y
            && self.min.z <= o.max.z + tol
            && self.max.z + tol >= o.min.z
    }
    /// distanta minima intre doua cutii (0 daca se suprapun)
    pub fn dist(&self, o: &Aabb) -> f32 {
        let d = (self.min - o.max).max(o.min - self.max).max(Vec3::ZERO);
        d.length()
    }
    pub fn ray(&self, o: Vec3, inv_d: Vec3, tmax: f32) -> bool {
        let t1 = (self.min - o) * inv_d;
        let t2 = (self.max - o) * inv_d;
        let tn = t1.min(t2);
        let tf = t1.max(t2);
        let near = tn.x.max(tn.y).max(tn.z).max(0.0);
        let far = tf.x.min(tf.y).min(tf.z).min(tmax);
        near <= far
    }
}

#[derive(Clone, Debug)]
pub struct Prop {
    pub group: String,
    pub name: String,
    pub value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct ElemId {
    pub m: u32,
    pub e: u32,
}

/// Element incarcat, in coordonate f64 (inainte de mutarea in originea locala).
pub struct RawElement {
    pub name: String,
    pub ifc_type: String,
    pub guid: String,
    pub storey: String,
    pub props: Vec<Prop>,
    pub tris: Vec<[DVec3; 3]>,
    pub colors: Vec<[u8; 4]>,
    pub default_hidden: bool,
}

pub struct RawModel {
    pub elements: Vec<RawElement>,
    pub info: Vec<(String, String)>,
    pub warnings: Vec<String>,
}

impl RawModel {
    /// Repune un desen 2D (extras prin „Cut to DWG”) in planul 3D de taiere.
    /// Coordonatele 2D sint in metri, in spatiul lumii; aplicatia le pastreaza
    /// locale, deci scadem origin-ul (ca in tot restul modelului).
    pub fn place_on_cut_plane(&mut self, axis: usize, value: f32, mirror: bool, origin: glam::DVec3) {
        let v = value as f64;
        for e in &mut self.elements {
            for t in &mut e.tris {
                for p in t {
                    let (mut a, mut b) = (p.x, p.y);
                    if mirror { a = -a; }
                    let local = match axis {
                        // 2D-ul exportat e in coordonatele LUMII (local + origin).
                        // `value` e deja local, deci nu i se scade origin-ul.
                        0 => glam::DVec3::new(v, a - origin.y, b - origin.z),
                        1 => glam::DVec3::new(a - origin.x, v, b - origin.z),
                        _ => glam::DVec3::new(a - origin.x, b - origin.y, v),
                    };
                    *p = local;
                }
            }
        }
    }

    /// Gratar plat (toate triunghiurile in acelasi plan, grosime < 1 mm): devine placa de `thick` metri,
    /// in jos fata de suprafata de calcare: fata de sus, cea de jos (normale inversate) si laturile pe
    /// conturul exterior (muchiile folosite o singura data).
    pub fn thicken_flat_gratings(&mut self, thick: f64) {
        for e in &mut self.elements {
            let t = e.ifc_type.to_lowercase();
            let is_grating = t.contains("grating") || e.props.iter().any(|p| p.value == "ACPPSTRUCTUREGRATING");
            if !is_grating || e.tris.is_empty() || e.tris.len() > 2000 {
                continue;
            }
            // normala planului (din cel mai mare triunghi) si planeitatea
            let n = e
                .tris
                .iter()
                .map(|t| (t[1] - t[0]).cross(t[2] - t[0]))
                .max_by(|a, b| a.length().total_cmp(&b.length()))
                .unwrap_or(DVec3::ZERO);
            if n.length() < 1e-12 {
                continue;
            }
            let mut n = n.normalize();
            let d0 = n.dot(e.tris[0][0]);
            let flat = e.tris.iter().all(|t| t.iter().all(|p| (n.dot(*p) - d0).abs() < 1e-3));
            if !flat {
                continue;
            }
            // grosimea in jos (gratarul se calca pe fata de sus); pentru planuri verticale, oricare parte
            if n.z < 0.0 {
                n = -n;
            }
            let off = -n * thick;
            let c0 = e.colors.first().copied().unwrap_or([160, 160, 160, 255]);
            // fata de sus orientata in sus (graficele proxy amesteca orientarile)
            let top: Vec<[DVec3; 3]> = std::mem::take(&mut e.tris).into_iter().map(|t| if (t[1] - t[0]).cross(t[2] - t[0]).dot(n) >= 0.0 { t } else { [t[0], t[2], t[1]] }).collect();
            let cols = std::mem::take(&mut e.colors);
            // muchiile de contur: perechi de varfuri (rotunjite la 0.1 mm) folosite o singura data
            let key = |p: DVec3| ((p.x * 1e4).round() as i64, (p.y * 1e4).round() as i64, (p.z * 1e4).round() as i64);
            let mut count: std::collections::HashMap<((i64, i64, i64), (i64, i64, i64)), (usize, DVec3, DVec3)> = std::collections::HashMap::new();
            for t in &top {
                for k in 0..3 {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    let (ka, kb) = (key(a), key(b));
                    let k2 = if ka < kb { (ka, kb) } else { (kb, ka) };
                    let ent = count.entry(k2).or_insert((0, a, b));
                    ent.0 += 1;
                }
            }
            let _ = (&cols, off);
            let edges: Vec<(DVec3, DVec3)> = count.into_values().filter(|v| v.0 == 1).map(|v| (v.1, v.2)).collect();
            // dimensiunile din baza de date Plant 3D ("L x W x grosime", in unitatile desenului)
            let size_hint = e.props.iter().find_map(|p| {
                let v: Vec<f64> = p.value.split(['x', 'X', '×']).map(|s| s.trim().parse::<f64>()).collect::<Result<_, _>>().ok()?;
                (v.len() == 3 && v.iter().all(|x| *x > 0.0)).then(|| (v[0], v[1], v[2]))
            });
            let (tris, info) = grating_bars(&edges, n, thick, size_hint);
            if tris.is_empty() {
                continue;
            }
            e.colors = vec![c0; tris.len()];
            e.tris = tris;
            e.props.push(Prop { group: "Geometry".into(), name: "Grating".into(), value: info });
        }
    }

    pub fn center(&self) -> DVec3 {
        let mut mn = DVec3::splat(f64::INFINITY);
        let mut mx = DVec3::splat(f64::NEG_INFINITY);
        for e in &self.elements {
            for t in &e.tris {
                for p in t {
                    mn = mn.min(*p);
                    mx = mx.max(*p);
                }
            }
        }
        if mn.x > mx.x {
            DVec3::ZERO
        } else {
            // X/Y centrate (coordonate mari de georeferentiere), Z pastrat real pentru cote
            let c = ((mn + mx) * 0.5).round();
            DVec3::new(c.x, c.y, if c.z.abs() > 5000.0 { c.z } else { 0.0 })
        }
    }
}

/// Cutie inchisa (12 triunghiuri, orientate spre exterior) dintr-un colt si trei axe.
fn push_box(out: &mut Vec<[DVec3; 3]>, o: DVec3, ax: DVec3, ay: DVec3, az: DVec3) {
    let c = o + (ax + ay + az) * 0.5;
    let p = |i: u8| o + if i & 1 != 0 { ax } else { DVec3::ZERO } + if i & 2 != 0 { ay } else { DVec3::ZERO } + if i & 4 != 0 { az } else { DVec3::ZERO };
    for f in [[0u8, 1, 3, 2], [4, 5, 7, 6], [0, 1, 5, 4], [2, 3, 7, 6], [0, 2, 6, 4], [1, 3, 7, 5]] {
        let (a, b, cc, d) = (p(f[0]), p(f[1]), p(f[2]), p(f[3]));
        let fc = (a + b + cc + d) * 0.25;
        let outward = (b - a).cross(cc - a).dot(fc - c) >= 0.0;
        if outward {
            out.push([a, b, cc]);
            out.push([a, cc, d]);
        } else {
            out.push([a, cc, b]);
            out.push([a, d, cc]);
        }
    }
}

/// Gratar presat ca in Plant 3D: rama pe contur, bare portante dese pe deschiderea scurta (pas 30 mm,
/// 30 x 3 mm) si bare transversale pe lungime (pas 100 mm, 6 x 6 mm), sub suprafata de calcare.
/// `edges` = conturul (in planul gratarului), `n` = normala in sus, `h` = inaltimea barelor.
fn grating_bars(edges: &[(DVec3, DVec3)], n: DVec3, h_default: f64, size_hint: Option<(f64, f64, f64)>) -> (Vec<[DVec3; 3]>, String) {
    let mut h = h_default;
    let mut out = Vec::new();
    let Some(longest) = edges.iter().max_by(|a, b| (a.1 - a.0).length().total_cmp(&(b.1 - b.0).length())) else { return (out, String::new()) };
    let o = longest.0;
    let mut u = (longest.1 - longest.0).reject_from(n).normalize_or_zero();
    if u == DVec3::ZERO {
        return (out, String::new());
    }
    let mut v = n.cross(u);
    let to2 = |p: DVec3| ((p - o).dot(u), (p - o).dot(v));
    let e2: Vec<((f64, f64), (f64, f64))> = edges.iter().map(|(a, b)| (to2(*a), to2(*b))).collect();
    let (mut umin, mut umax, mut vmin, mut vmax) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for (a, b) in &e2 {
        for p in [a, b] {
            umin = umin.min(p.0);
            umax = umax.max(p.0);
            vmin = vmin.min(p.1);
            vmax = vmax.max(p.1);
        }
    }
    // barele portante acopera deschiderea SCURTA: asezate de-a lungul laturii lungi, cu pasul de 30 mm
    let swap = (vmax - vmin) > (umax - umin);
    if swap {
        std::mem::swap(&mut u, &mut v);
    }
    let e2: Vec<((f64, f64), (f64, f64))> = if swap { e2.iter().map(|(a, b)| ((a.1, a.0), (b.1, b.0))).collect() } else { e2 };
    let (lo_u, hi_u, lo_v, hi_v) = if swap { (vmin, vmax, umin, umax) } else { (umin, umax, vmin, vmax) };
    let (lu, lv) = (hi_u - lo_u, hi_v - lo_v);
    if lu < 0.01 || lv < 0.01 {
        return (out, String::new());
    }
    // grosimea reala: unitatea desenului se deduce din L x W comparate cu conturul (inch, mm... - oricare)
    let mut from_db = false;
    if let Some((a, b, c)) = size_hint {
        let (big, small) = (a.max(b), a.min(b));
        let scale = lu.max(lv) / big;
        if (lu.min(lv) - small * scale).abs() <= 0.1 * lu.min(lv) + 0.01 {
            let t = c * scale;
            if (0.005..=0.2).contains(&t) {
                h = t;
                from_db = true;
            }
        }
    }
    // intervalul conturului pe o linie u = s (sau v = s): intersectii cu muchiile
    let span = |s: f64, along_v: bool| -> Option<(f64, f64)> {
        let mut lo = f64::MAX;
        let mut hi = f64::MIN;
        for (a, b) in &e2 {
            let (a_s, a_t, b_s, b_t) = if along_v { (a.0, a.1, b.0, b.1) } else { (a.1, a.0, b.1, b.0) };
            if (a_s - s) * (b_s - s) > 0.0 || (a_s - b_s).abs() < 1e-12 {
                continue;
            }
            let t = a_t + (b_t - a_t) * (s - a_s) / (b_s - a_s);
            lo = lo.min(t);
            hi = hi.max(t);
        }
        (hi > lo + 1e-3).then_some((lo, hi))
    };
    let pt = |s: f64, t: f64, along_v: bool| if along_v { o + u * s + v * t } else { o + u * t + v * s };
    let down = -n * h;
    // pasul barelor portante: 30 mm (mai rar doar la gratare foarte mari, ca sa ramana usor de afisat)
    let mut pitch = 0.03;
    while lu / pitch > 1500.0 {
        pitch *= 2.0;
    }
    let tb = 0.003;
    let nb = (lu / pitch).floor() as usize;
    let first = lo_u + (lu - nb as f64 * pitch) * 0.5;
    for i in 0..=nb {
        let s = first + i as f64 * pitch;
        if let Some((a, b)) = span(s, true) {
            let p0 = pt(s, a, true) - u * (tb * 0.5);
            push_box(&mut out, p0 + down, u * tb, v * (b - a), n * h);
        }
    }
    // bare transversale (sus, 6 x 6 mm), pas 100 mm
    let tc = 0.006;
    let mut cpitch = 0.1;
    while lv / cpitch > 300.0 {
        cpitch *= 2.0;
    }
    let nc = (lv / cpitch).floor() as usize;
    let firstc = lo_v + (lv - nc as f64 * cpitch) * 0.5;
    for i in 0..=nc {
        let s = firstc + i as f64 * cpitch;
        if let Some((a, b)) = span(s, false) {
            let p0 = pt(s, a, false) - v * (tc * 0.5);
            push_box(&mut out, p0 - n * tc, u * (b - a), v * tc, n * tc);
        }
    }
    // rama (bara de margine) pe tot conturul
    for (a, b) in edges {
        let d = (*b - *a).reject_from(n);
        let len = d.length();
        if len < 1e-4 {
            continue;
        }
        let dir = d / len;
        let side = n.cross(dir) * 0.005;
        push_box(&mut out, *a - side * 0.5 + down, d, side, n * h);
    }
    let info = format!(
        "{:.0} x {:.0} mm, bearing bars {:.0} x {:.0} mm at {:.0} mm, cross bars every {:.0} mm ({})",
        lv.max(lu) * 1000.0,
        lv.min(lu) * 1000.0,
        h * 1000.0,
        tb * 1000.0,
        pitch * 1000.0,
        cpitch * 1000.0,
        if from_db { "thickness from the Plant 3D project; bars drawn" } else { "standard thickness; the drawing stores only the outline" }
    );
    (out, info)
}

#[derive(Clone)]
pub struct Element {
    pub name: String,
    pub ifc_type: String,
    pub guid: String,
    pub storey: String,
    pub props: Vec<Prop>,
    pub tri_start: u32,
    pub tri_count: u32,
    pub aabb: Aabb,
    pub default_hidden: bool,
    pub bvh: Bvh,
}

pub struct Model {
    pub name: String,
    pub path: String,
    pub elements: Vec<Element>,
    /// 3 varfuri per triunghi
    pub pos: Vec<Vec3>,
    pub tri_color: Vec<[u8; 4]>,
    pub aabb: Aabb,
    pub info: Vec<(String, String)>,
    pub warnings: Vec<String>,
    /// nor de puncte (E57): punctele scanarilor, in coordonate locale
    pub cloud: Option<std::sync::Arc<PointCloud>>,
}

/// Punctele unui nor, amestecate aleator in fiecare scanare (densitate = primele N% din scanare).
pub struct PointCloud {
    pub pos: Vec<Vec3>,
    pub col: Vec<[u8; 4]>,
    /// (index element, primul punct, numar de puncte)
    pub scans: Vec<(u32, u32, u32)>,
}

impl Model {
    /// Copie completa (geometria inclusa) - doar cand modelul e folosit si in alta parte si trebuie modificat.
    pub fn clone_model(&self) -> Model {
        Model { name: self.name.clone(), path: self.path.clone(), elements: self.elements.clone(), pos: self.pos.clone(), tri_color: self.tri_color.clone(), aabb: self.aabb, info: self.info.clone(), warnings: self.warnings.clone(), cloud: self.cloud.clone() }
    }
}

impl Model {
    pub fn from_raw(raw: RawModel, name: String, path: String, origin: DVec3) -> Model {
        use rayon::prelude::*;
        // conversie + BVH pe element, in paralel
        let parts: Vec<(RawElement, Vec<Vec3>, Aabb, Bvh)> = raw
            .elements
            .into_par_iter()
            .map(|mut e| {
                let mut bb = Aabb::EMPTY;
                let mut p = Vec::with_capacity(e.tris.len() * 3);
                for t in &e.tris {
                    for v in t {
                        let l = (*v - origin).as_vec3();
                        bb.grow(l);
                        p.push(l);
                    }
                }
                e.tris = Vec::new();
                let bvh = Bvh::build(&p);
                (e, p, bb, bvh)
            })
            .collect();
        let ntri: usize = parts.iter().map(|x| x.1.len() / 3).sum();
        let mut pos = Vec::with_capacity(ntri * 3);
        let mut tri_color = Vec::with_capacity(ntri);
        let mut elements = Vec::with_capacity(parts.len());
        let mut aabb = Aabb::EMPTY;
        for (e, p, bb, bvh) in parts {
            let start = (pos.len() / 3) as u32;
            let count = (p.len() / 3) as u32;
            pos.extend_from_slice(&p);
            for i in 0..count as usize {
                tri_color.push(e.colors.get(i).copied().unwrap_or([200, 200, 200, 255]));
            }
            if bb.is_valid() {
                aabb.union(&bb);
            }
            elements.push(Element {
                name: e.name,
                ifc_type: e.ifc_type,
                guid: e.guid,
                storey: e.storey,
                props: e.props,
                tri_start: start,
                tri_count: count,
                aabb: bb,
                default_hidden: e.default_hidden,
                bvh,
            });
        }
        Model { name, path, elements, pos, tri_color, aabb, info: raw.info, warnings: raw.warnings, cloud: None }
    }

    /// Ataseaza punctele unui nor citit (E57): coordonate locale fata de `origin`, cutiile scanarilor si a modelului.
    pub fn attach_cloud(&mut self, c: crate::e57::RawCloud, origin: DVec3) {
        let shift = (c.base - origin).as_vec3();
        let pos: Vec<Vec3> = c.pos.iter().map(|p| Vec3::from_array(*p) + shift).collect();
        for &(ei, start, n) in &c.scans {
            let mut bb = Aabb::EMPTY;
            for p in &pos[start as usize..(start + n) as usize] {
                bb.grow(*p);
            }
            if bb.is_valid() {
                if let Some(e) = self.elements.get_mut(ei as usize) {
                    e.aabb = bb;
                }
                self.aabb.union(&bb);
            }
        }
        self.cloud = Some(std::sync::Arc::new(PointCloud { pos, col: c.col, scans: c.scans }));
    }

    pub fn tris(&self, e: &Element) -> &[Vec3] {
        &self.pos[e.tri_start as usize * 3..(e.tri_start + e.tri_count) as usize * 3]
    }

    /// Copy of the model with a transform (local coordinates) and per-element offsets applied.
    pub fn transformed(&self, m: glam::Mat4, offsets: &std::collections::HashMap<u32, Vec3>) -> Model {
        use rayon::prelude::*;
        let parts: Vec<(Vec<Vec3>, Aabb, Bvh)> = self
            .elements
            .par_iter()
            .enumerate()
            .map(|(i, e)| {
                let off = offsets.get(&(i as u32)).copied().unwrap_or(Vec3::ZERO);
                let mut bb = Aabb::EMPTY;
                let p: Vec<Vec3> = self
                    .tris(e)
                    .iter()
                    .map(|v| {
                        let q = m.transform_point3(*v) + off;
                        bb.grow(q);
                        q
                    })
                    .collect();
                let bvh = Bvh::build(&p);
                (p, bb, bvh)
            })
            .collect();
        let mut pos = Vec::with_capacity(self.pos.len());
        let mut elements = Vec::with_capacity(self.elements.len());
        let mut aabb = Aabb::EMPTY;
        for (e, (p, bb, bvh)) in self.elements.iter().zip(parts) {
            pos.extend_from_slice(&p);
            if bb.is_valid() {
                aabb.union(&bb);
            }
            let mut ne = e.clone();
            ne.aabb = bb;
            ne.bvh = bvh;
            elements.push(ne);
        }
        let cloud = self.cloud.as_ref().map(|c| {
            let mut pos = c.pos.clone();
            for &(ei, start, n) in &c.scans {
                let off = offsets.get(&ei).copied().unwrap_or(Vec3::ZERO);
                let mut bb = Aabb::EMPTY;
                for p in &mut pos[start as usize..(start + n) as usize] {
                    *p = m.transform_point3(*p) + off;
                    bb.grow(*p);
                }
                if bb.is_valid() {
                    if let Some(e) = elements.get_mut(ei as usize) {
                        e.aabb = bb;
                    }
                    aabb.union(&bb);
                }
            }
            std::sync::Arc::new(PointCloud { pos, col: c.col.clone(), scans: c.scans.clone() })
        });
        Model { name: self.name.clone(), path: self.path.clone(), elements, pos, tri_color: self.tri_color.clone(), aabb, info: self.info.clone(), warnings: self.warnings.clone(), cloud }
    }

    pub fn tri_total(&self) -> usize {
        self.pos.len() / 3
    }
}

// ---------------------------------------------------------------- BVH

#[derive(Clone)]
pub struct BvhNode {
    pub bb: Aabb,
    /// frunza: (start, count) in `order`; nod intern: (left, right) cu count == 0
    pub a: u32,
    pub b: u32,
    pub leaf: bool,
}

#[derive(Clone, Default)]
pub struct Bvh {
    pub nodes: Vec<BvhNode>,
    /// indici de triunghiuri (locali elementului)
    pub order: Vec<u32>,
}

impl Bvh {
    pub fn build(tris: &[Vec3]) -> Bvh {
        let n = tris.len() / 3;
        let mut bvh = Bvh { nodes: Vec::new(), order: (0..n as u32).collect() };
        if n == 0 {
            return bvh;
        }
        let cent: Vec<Vec3> = (0..n).map(|i| (tris[i * 3] + tris[i * 3 + 1] + tris[i * 3 + 2]) / 3.0).collect();
        let mut order = std::mem::take(&mut bvh.order);
        bvh.build_rec(tris, &cent, &mut order, 0, n);
        bvh.order = order;
        bvh
    }

    fn build_rec(&mut self, tris: &[Vec3], cent: &[Vec3], order: &mut [u32], start: usize, end: usize) -> u32 {
        let mut bb = Aabb::EMPTY;
        let mut cb = Aabb::EMPTY;
        for &t in &order[start..end] {
            let t = t as usize;
            bb.grow(tris[t * 3]);
            bb.grow(tris[t * 3 + 1]);
            bb.grow(tris[t * 3 + 2]);
            cb.grow(cent[t]);
        }
        let idx = self.nodes.len() as u32;
        self.nodes.push(BvhNode { bb, a: start as u32, b: (end - start) as u32, leaf: true });
        if end - start <= 4 {
            return idx;
        }
        let s = cb.size();
        let axis = if s.x >= s.y && s.x >= s.z {
            0
        } else if s.y >= s.z {
            1
        } else {
            2
        };
        if s[axis] <= 1e-7 {
            return idx;
        }
        let mid = (start + end) / 2;
        order[start..end].select_nth_unstable_by(mid - start, |a, b| {
            cent[*a as usize][axis].total_cmp(&cent[*b as usize][axis])
        });
        let l = self.build_rec(tris, cent, order, start, mid);
        let r = self.build_rec(tris, cent, order, mid, end);
        let nd = &mut self.nodes[idx as usize];
        nd.leaf = false;
        nd.a = l;
        nd.b = r;
        idx
    }

    /// Cea mai apropiata intersectie raza-triunghi. `accept` filtreaza punctele (ex. plan de sectiune).
    pub fn raycast(&self, tris: &[Vec3], o: Vec3, d: Vec3, mut tmax: f32, accept: &dyn Fn(Vec3) -> bool) -> Option<(f32, u32)> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv = Vec3::new(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
        let mut best = None;
        let mut stack = vec![0u32];
        while let Some(ni) = stack.pop() {
            let nd = &self.nodes[ni as usize];
            if !nd.bb.ray(o, inv, tmax) {
                continue;
            }
            if nd.leaf {
                for &t in &self.order[nd.a as usize..(nd.a + nd.b) as usize] {
                    let i = t as usize * 3;
                    if let Some(tt) = ray_tri(o, d, tris[i], tris[i + 1], tris[i + 2]) {
                        if tt < tmax && accept(o + d * tt) {
                            tmax = tt;
                            best = Some((tt, t));
                        }
                    }
                }
            } else {
                stack.push(nd.a);
                stack.push(nd.b);
            }
        }
        best
    }
}

pub fn ray_tri(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    if t > 1e-5 {
        Some(t)
    } else {
        None
    }
}
