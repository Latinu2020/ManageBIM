//! Clash Detective: Hard, Clearance and Duplicate tests between two sets of elements.
use crate::model::{Aabb, Bvh, ElemId, Model};
use glam::Vec3;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ClashType {
    Hard,
    Clearance,
    Duplicates,
}

impl ClashType {
    pub fn label(&self) -> &'static str {
        match self {
            ClashType::Hard => "Hard",
            ClashType::Clearance => "Clearance",
            ClashType::Duplicates => "Duplicates",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ClashStatus {
    New,
    Active,
    Reviewed,
    Approved,
    Resolved,
}

impl ClashStatus {
    pub const ALL: [ClashStatus; 5] = [ClashStatus::New, ClashStatus::Active, ClashStatus::Reviewed, ClashStatus::Approved, ClashStatus::Resolved];
    pub fn label(&self) -> &'static str {
        match self {
            ClashStatus::New => "New",
            ClashStatus::Active => "Active",
            ClashStatus::Reviewed => "Reviewed",
            ClashStatus::Approved => "Approved",
            ClashStatus::Resolved => "Resolved",
        }
    }
    pub fn color(&self) -> [u8; 3] {
        match self {
            ClashStatus::New => [230, 70, 60],
            ClashStatus::Active => [240, 150, 40],
            ClashStatus::Reviewed => [80, 150, 230],
            ClashStatus::Approved => [70, 190, 90],
            ClashStatus::Resolved => [150, 150, 150],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Found {
    pub a: ElemId,
    pub b: ElemId,
    pub point: Vec3,
    pub dist: f32,
}

pub struct Progress {
    pub done: AtomicUsize,
    pub total: AtomicUsize,
    pub cancel: AtomicBool,
}

impl Progress {
    pub fn new() -> Arc<Progress> {
        Arc::new(Progress { done: AtomicUsize::new(0), total: AtomicUsize::new(1), cancel: AtomicBool::new(false) })
    }
    pub fn frac(&self) -> f32 {
        self.done.load(Ordering::Relaxed) as f32 / self.total.load(Ordering::Relaxed).max(1) as f32
    }
}

// ------------------------------------------------------------- AABB tree over elements
struct Node {
    bb: Aabb,
    a: u32,
    b: u32,
    leaf: bool,
}

struct ElemTree {
    nodes: Vec<Node>,
    items: Vec<(ElemId, Aabb)>,
}

impl ElemTree {
    fn build(mut items: Vec<(ElemId, Aabb)>) -> ElemTree {
        let mut t = ElemTree { nodes: Vec::new(), items: Vec::new() };
        if !items.is_empty() {
            let n = items.len();
            t.rec(&mut items, 0, n);
        }
        t.items = items;
        t
    }
    fn rec(&mut self, items: &mut [(ElemId, Aabb)], s: usize, e: usize) -> u32 {
        let mut bb = Aabb::EMPTY;
        let mut cb = Aabb::EMPTY;
        for it in &items[s..e] {
            bb.union(&it.1);
            cb.grow(it.1.center());
        }
        let idx = self.nodes.len() as u32;
        self.nodes.push(Node { bb, a: s as u32, b: (e - s) as u32, leaf: true });
        if e - s <= 4 {
            return idx;
        }
        let sz = cb.size();
        let ax = if sz.x >= sz.y && sz.x >= sz.z { 0 } else if sz.y >= sz.z { 1 } else { 2 };
        let mid = (s + e) / 2;
        items[s..e].select_nth_unstable_by(mid - s, |x, y| x.1.center()[ax].total_cmp(&y.1.center()[ax]));
        let l = self.rec(items, s, mid);
        let r = self.rec(items, mid, e);
        let nd = &mut self.nodes[idx as usize];
        nd.leaf = false;
        nd.a = l;
        nd.b = r;
        idx
    }
    fn query(&self, q: &Aabb, tol: f32, out: &mut Vec<ElemId>) {
        if self.nodes.is_empty() {
            return;
        }
        let mut st = vec![0u32];
        while let Some(i) = st.pop() {
            let n = &self.nodes[i as usize];
            if !n.bb.overlaps(q, tol) {
                continue;
            }
            if n.leaf {
                for it in &self.items[n.a as usize..(n.a + n.b) as usize] {
                    if it.1.overlaps(q, tol) {
                        out.push(it.0);
                    }
                }
            } else {
                st.push(n.a);
                st.push(n.b);
            }
        }
    }
}

pub fn run(models: &[Option<Arc<Model>>], set_a: &[ElemId], set_b: &[ElemId], ty: ClashType, tol: f32, ignore_same_file: bool, prog: &Progress) -> Vec<Found> {
    let get = |id: ElemId| models.get(id.m as usize).and_then(|m| m.as_ref()).and_then(|m| m.elements.get(id.e as usize).map(|e| (m, e)));
    let b_items: Vec<(ElemId, Aabb)> = set_b.iter().filter_map(|id| get(*id).filter(|(_, e)| e.aabb.is_valid()).map(|(_, e)| (*id, e.aabb))).collect();
    let tree = ElemTree::build(b_items);
    let a_set: HashSet<ElemId> = set_a.iter().copied().collect();
    let b_set: HashSet<ElemId> = set_b.iter().copied().collect();
    prog.total.store(set_a.len().max(1), Ordering::Relaxed);
    prog.done.store(0, Ordering::Relaxed);
    let qtol = match ty {
        ClashType::Clearance => tol,
        ClashType::Duplicates => tol.max(1e-4),
        ClashType::Hard => 1e-5,
    };
    let mut res: Vec<Found> = set_a
        .par_iter()
        .flat_map_iter(|&ia| {
            let mut out = Vec::new();
            prog.done.fetch_add(1, Ordering::Relaxed);
            if prog.cancel.load(Ordering::Relaxed) {
                return out.into_iter();
            }
            let Some((ma, ea)) = get(ia) else { return out.into_iter() };
            if !ea.aabb.is_valid() {
                return out.into_iter();
            }
            let mut cand = Vec::new();
            tree.query(&ea.aabb, qtol, &mut cand);
            for ib in cand {
                if ib == ia || (ignore_same_file && ib.m == ia.m) {
                    continue;
                }
                // pair counted only once when the sets overlap
                if a_set.contains(&ib) && b_set.contains(&ia) && ib < ia {
                    continue;
                }
                let Some((mb, eb)) = get(ib) else { continue };
                let ta = ma.tris(ea);
                let tb = mb.tris(eb);
                match ty {
                    ClashType::Hard => {
                        if let Some(p) = bvh_intersect(&ea.bvh, ta, &eb.bvh, tb) {
                            let ov_min = ea.aabb.max.min(eb.aabb.max) - ea.aabb.min.max(eb.aabb.min);
                            let pen = ov_min.x.min(ov_min.y).min(ov_min.z).max(0.0);
                            if pen > tol {
                                out.push(Found { a: ia, b: ib, point: p, dist: -pen });
                            }
                        }
                    }
                    ClashType::Clearance => {
                        if let Some((d, p)) = bvh_min_dist(&ea.bvh, ta, &eb.bvh, tb, tol) {
                            out.push(Found { a: ia, b: ib, point: p, dist: d });
                        }
                    }
                    ClashType::Duplicates => {
                        if ea.tri_count == eb.tri_count
                            && (ea.aabb.min - eb.aabb.min).abs().max_element() <= tol.max(1e-4)
                            && (ea.aabb.max - eb.aabb.max).abs().max_element() <= tol.max(1e-4)
                        {
                            out.push(Found { a: ia, b: ib, point: ea.aabb.center(), dist: 0.0 });
                        }
                    }
                }
            }
            out.into_iter()
        })
        .collect();
    res.sort_by(|x, y| x.dist.total_cmp(&y.dist).then(x.a.cmp(&y.a)).then(x.b.cmp(&y.b)));
    res
}

fn tri_bb(t: &[Vec3], i: u32) -> Aabb {
    let i = i as usize * 3;
    Aabb { min: t[i].min(t[i + 1]).min(t[i + 2]), max: t[i].max(t[i + 1]).max(t[i + 2]) }
}

/// The first intersection point between two triangle meshes.
pub fn bvh_intersect(ba: &Bvh, ta: &[Vec3], bb: &Bvh, tb: &[Vec3]) -> Option<Vec3> {
    if ba.nodes.is_empty() || bb.nodes.is_empty() {
        return None;
    }
    let mut st = vec![(0u32, 0u32)];
    while let Some((i, j)) = st.pop() {
        let na = &ba.nodes[i as usize];
        let nb = &bb.nodes[j as usize];
        if !na.bb.overlaps(&nb.bb, 1e-6) {
            continue;
        }
        match (na.leaf, nb.leaf) {
            (true, true) => {
                for &x in &ba.order[na.a as usize..(na.a + na.b) as usize] {
                    let bx = tri_bb(ta, x);
                    if !bx.overlaps(&nb.bb, 1e-6) {
                        continue;
                    }
                    let xi = x as usize * 3;
                    for &y in &bb.order[nb.a as usize..(nb.a + nb.b) as usize] {
                        if !bx.overlaps(&tri_bb(tb, y), 1e-6) {
                            continue;
                        }
                        let yi = y as usize * 3;
                        if let Some(p) = tri_tri(&ta[xi..xi + 3], &tb[yi..yi + 3]) {
                            return Some(p);
                        }
                    }
                }
            }
            (false, true) => {
                st.push((na.a, j));
                st.push((na.b, j));
            }
            (true, false) => {
                st.push((i, nb.a));
                st.push((i, nb.b));
            }
            (false, false) => {
                if na.bb.size().length_squared() >= nb.bb.size().length_squared() {
                    st.push((na.a, j));
                    st.push((na.b, j));
                } else {
                    st.push((i, nb.a));
                    st.push((i, nb.b));
                }
            }
        }
    }
    None
}

/// The minimum distance (< limit) between two meshes; returns (dist, midpoint).
pub fn bvh_min_dist(ba: &Bvh, ta: &[Vec3], bb: &Bvh, tb: &[Vec3], limit: f32) -> Option<(f32, Vec3)> {
    if ba.nodes.is_empty() || bb.nodes.is_empty() {
        return None;
    }
    let mut best = limit;
    let mut bp = None;
    let mut st = vec![(0u32, 0u32)];
    while let Some((i, j)) = st.pop() {
        let na = &ba.nodes[i as usize];
        let nb = &bb.nodes[j as usize];
        if na.bb.dist(&nb.bb) >= best {
            continue;
        }
        match (na.leaf, nb.leaf) {
            (true, true) => {
                for &x in &ba.order[na.a as usize..(na.a + na.b) as usize] {
                    let xi = x as usize * 3;
                    let bx = tri_bb(ta, x);
                    for &y in &bb.order[nb.a as usize..(nb.a + nb.b) as usize] {
                        if bx.dist(&tri_bb(tb, y)) >= best {
                            continue;
                        }
                        let yi = y as usize * 3;
                        let (d, p, q) = tri_tri_dist(&ta[xi..xi + 3], &tb[yi..yi + 3]);
                        if d < best {
                            best = d;
                            let mid = (p + q) * 0.5;
                            bp = Some(mid);
                            if d <= 0.0 {
                                return Some((0.0, mid));
                            }
                        }
                    }
                }
            }
            (false, true) => {
                st.push((na.a, j));
                st.push((na.b, j));
            }
            (true, false) => {
                st.push((i, nb.a));
                st.push((i, nb.b));
            }
            (false, false) => {
                if na.bb.size().length_squared() >= nb.bb.size().length_squared() {
                    st.push((na.a, j));
                    st.push((na.b, j));
                } else {
                    st.push((i, nb.a));
                    st.push((i, nb.b));
                }
            }
        }
    }
    bp.map(|p| (best, p))
}

/// Does segment [p,q] strictly intersect the interior of the triangle?
fn seg_tri(p: Vec3, q: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<Vec3> {
    let d = q - p;
    let e1 = b - a;
    let e2 = c - a;
    let h = d.cross(e2);
    let det = e1.dot(h);
    let scale = e1.length() * e2.length() * d.length();
    if det.abs() <= 1e-7 * scale.max(1e-20) {
        return None;
    }
    let inv = 1.0 / det;
    let s = p - a;
    let u = s.dot(h) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let qq = s.cross(e1);
    let v = d.dot(qq) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(qq) * inv;
    if t > 1e-4 && t < 1.0 - 1e-4 {
        Some(p + d * t)
    } else {
        None
    }
}

pub fn tri_tri(a: &[Vec3], b: &[Vec3]) -> Option<Vec3> {
    for i in 0..3 {
        if let Some(p) = seg_tri(a[i], a[(i + 1) % 3], b[0], b[1], b[2]) {
            return Some(p);
        }
        if let Some(p) = seg_tri(b[i], b[(i + 1) % 3], a[0], a[1], a[2]) {
            return Some(p);
        }
    }
    None
}

pub fn closest_pt_tri(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let den = 1.0 / (va + vb + vc);
    a + ab * (vb * den) + ac * (vc * den)
}

pub fn seg_seg(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> (Vec3, Vec3) {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    let (s, t);
    if a <= 1e-12 && e <= 1e-12 {
        return (p1, p2);
    }
    if a <= 1e-12 {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= 1e-12 {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let den = a * e - b * b;
            let mut ss = if den.abs() > 1e-12 { ((b * f - c * e) / den).clamp(0.0, 1.0) } else { 0.0 };
            let mut tt = (b * ss + f) / e;
            if tt < 0.0 {
                tt = 0.0;
                ss = (-c / a).clamp(0.0, 1.0);
            } else if tt > 1.0 {
                tt = 1.0;
                ss = ((b - c) / a).clamp(0.0, 1.0);
            }
            s = ss;
            t = tt;
        }
    }
    (p1 + d1 * s, p2 + d2 * t)
}

pub fn tri_tri_dist(a: &[Vec3], b: &[Vec3]) -> (f32, Vec3, Vec3) {
    if let Some(p) = tri_tri(a, b) {
        return (0.0, p, p);
    }
    let mut best = f32::INFINITY;
    let mut bp = (a[0], b[0]);
    for i in 0..3 {
        let q = closest_pt_tri(a[i], b[0], b[1], b[2]);
        let d = (q - a[i]).length();
        if d < best {
            best = d;
            bp = (a[i], q);
        }
        let q = closest_pt_tri(b[i], a[0], a[1], a[2]);
        let d = (q - b[i]).length();
        if d < best {
            best = d;
            bp = (q, b[i]);
        }
        for j in 0..3 {
            let (p, q) = seg_seg(a[i], a[(i + 1) % 3], b[j], b[(j + 1) % 3]);
            let d = (p - q).length();
            if d < best {
                best = d;
                bp = (p, q);
            }
        }
    }
    (best, bp.0, bp.1)
}
