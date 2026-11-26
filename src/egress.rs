//! Verificari de spatiu: **inaltimea libera** (headroom) si **distanta de evacuare** (egress).
//!
//! Ambele lucreaza pe geometria incarcata (Z in sus, metri) si intorc `clash::Found`, ca rezultatele sa
//! intre intr-un test din Clash Detective (vedere, status, responsabil, BCF, MCP):
//! - headroom: puncte pe fata de sus a placilor de pardoseala; raza in sus pana la primul obstacol
//!   (ignorand ce incepe sub `low_cut` - mobilier, picioarele peretilor). Sub minim = rezultat
//!   (obstacol x placa, `dist` = inaltime - minim, negativ = cat lipseste).
//! - egress: pe fiecare nivel, harta 2D a zonelor de mers (pereti / stalpi taiati la `cut` deasupra
//!   pardoselii; usile trec), iesirile (usi exterioare la nivelul cel mai de jos, scari / rampe la
//!   celelalte), distanta de parcurs (Dijkstra pe 8 vecini). Peste limita = rezultat
//!   (spatiul / placa x iesirea cea mai apropiata, `dist` = limita - distanta).

use crate::clash::Found;
use crate::model::{ElemId, Model};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Check {
    Headroom,
    Egress,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Params {
    /// inaltimea libera minima (m)
    pub min_headroom: f32,
    /// distanta maxima de evacuare (m)
    pub max_travel: f32,
    /// pasul gridului (m)
    pub step: f32,
}

impl Default for Params {
    fn default() -> Self {
        Params { min_headroom: 2.10, max_travel: 45.0, step: 0.25 }
    }
}

/// ce se ignora la headroom: obstacole care pornesc sub aceasta inaltime deasupra pardoselii
const LOW_CUT: f32 = 1.2;
/// inaltimea la care se taie peretii pentru harta de mers (egress)
const CUT: f32 = 1.0;

fn lower(e: &crate::model::Element) -> String {
    e.ifc_type.to_ascii_lowercase()
}

/// placa de pardoseala (nu acoperis)
pub fn is_floor(e: &crate::model::Element) -> bool {
    let t = lower(e);
    let roof = t.contains("roof") || e.name.to_ascii_lowercase().contains("roof") || e.props.iter().any(|p| p.name.eq_ignore_ascii_case("PredefinedType") && p.value.eq_ignore_ascii_case("ROOF"));
    (t.contains("slab") || t == "floors" || t.contains("ifcfloor")) && !roof
}

fn not_physical(t: &str) -> bool {
    t.contains("space") || t.contains("opening") || t.contains("annotation") || t.contains("site") || t.contains("grid") || t.contains("zone") || t.contains("virtual")
}

fn is_door(t: &str) -> bool {
    t.contains("door")
}

fn is_stair(t: &str) -> bool {
    t.contains("stair") || t.contains("ramp")
}

/// varf al fetei de sus a unui element, in punctul (x, y): raza de sus in jos
fn top_at(m: &Model, e: &crate::model::Element, x: f32, y: f32) -> Option<f32> {
    let o = Vec3::new(x, y, e.aabb.max.z + 1.0);
    let d = Vec3::new(1e-6, 1e-6, -1.0);
    e.bvh.raycast(m.tris(e), o, d, e.aabb.max.z - e.aabb.min.z + 2.0, &|_| true).map(|(t, _)| o.z - t)
}

struct Sample {
    p: Vec3,
    floor: ElemId,
}

/// punctele de pe fata de sus a placilor (grid cu pasul `step`)
fn floor_samples(models: &[Option<Arc<Model>>], items: &[ElemId], step: f32) -> Vec<Sample> {
    let mut out = Vec::new();
    for id in items {
        let Some(Some(m)) = models.get(id.m as usize) else { continue };
        let Some(e) = m.elements.get(id.e as usize) else { continue };
        if !is_floor(e) {
            continue;
        }
        let bb = e.aabb;
        let nx = ((bb.max.x - bb.min.x) / step).ceil().max(1.0) as i32;
        let ny = ((bb.max.y - bb.min.y) / step).ceil().max(1.0) as i32;
        for i in 0..nx {
            for j in 0..ny {
                let x = bb.min.x + (i as f32 + 0.5) * step;
                let y = bb.min.y + (j as f32 + 0.5) * step;
                if let Some(z) = top_at(m, e, x, y) {
                    out.push(Sample { p: Vec3::new(x, y, z), floor: *id });
                }
            }
        }
    }
    out
}

/// index 2D simplu al cutiilor (celule de 2 m) pentru elementele candidate
struct Grid2 {
    cell: f32,
    map: HashMap<(i32, i32), Vec<ElemId>>,
}

impl Grid2 {
    fn new(models: &[Option<Arc<Model>>], ids: &[ElemId]) -> Grid2 {
        let cell = 2.0;
        let mut map: HashMap<(i32, i32), Vec<ElemId>> = HashMap::new();
        for id in ids {
            let Some(Some(m)) = models.get(id.m as usize) else { continue };
            let Some(e) = m.elements.get(id.e as usize) else { continue };
            let bb = e.aabb;
            for i in (bb.min.x / cell).floor() as i32..=(bb.max.x / cell).floor() as i32 {
                for j in (bb.min.y / cell).floor() as i32..=(bb.max.y / cell).floor() as i32 {
                    map.entry((i, j)).or_default().push(*id);
                }
            }
        }
        Grid2 { cell, map }
    }
    fn at(&self, x: f32, y: f32) -> &[ElemId] {
        self.map.get(&((x / self.cell).floor() as i32, (y / self.cell).floor() as i32)).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// Inaltimea libera: rezultate obstacol x placa, cu punctul cel mai jos.
pub fn headroom(models: &[Option<Arc<Model>>], items: &[ElemId], p: &Params) -> Vec<Found> {
    let samples = floor_samples(models, items, p.step);
    let obstacles: Vec<ElemId> = items
        .iter()
        .copied()
        .filter(|id| {
            models.get(id.m as usize).and_then(|m| m.as_ref()).and_then(|m| m.elements.get(id.e as usize)).map(|e| {
                let t = lower(e);
                !not_physical(&t) && !is_door(&t) && !t.contains("window") && e.tri_count > 0
            }).unwrap_or(false)
        })
        .collect();
    let grid = Grid2::new(models, &obstacles);
    // (obstacol, placa) -> (inaltimea minima, punctul)
    let mut worst: HashMap<(ElemId, ElemId), (f32, Vec3)> = HashMap::new();
    let d = Vec3::new(1e-6, 1e-6, 1.0);
    for s in &samples {
        let o = s.p + Vec3::new(0.0, 0.0, 0.005);
        let floor_z = s.p.z;
        let mut best: Option<(f32, ElemId)> = None;
        for id in grid.at(o.x, o.y) {
            if *id == s.floor {
                continue;
            }
            let Some(Some(m)) = models.get(id.m as usize) else { continue };
            let e = &m.elements[id.e as usize];
            let bb = e.aabb;
            if o.x < bb.min.x || o.x > bb.max.x || o.y < bb.min.y || o.y > bb.max.y || bb.max.z < floor_z + LOW_CUT || bb.min.z > floor_z + p.min_headroom {
                continue;
            }
            // un element care incepe de pe pardoseala (perete, stalp, mobilier) nu e un obstacol deasupra capului;
            // scara da (sub rampa urcatoare se loveste capul), partea ei joasa e ignorata de filtrul razei
            if bb.min.z < floor_z + LOW_CUT && !is_stair(&lower(e)) {
                continue;
            }
            let tmax = best.map(|b| b.0).unwrap_or(p.min_headroom);
            if let Some((t, _)) = e.bvh.raycast(m.tris(e), o, d, tmax, &|h| h.z >= floor_z + LOW_CUT) {
                if best.is_none_or(|b| t < b.0) {
                    best = Some((t, *id));
                }
            }
        }
        if let Some((h, ob)) = best {
            if h < p.min_headroom {
                let w = worst.entry((ob, s.floor)).or_insert((f32::MAX, s.p));
                if h < w.0 {
                    *w = (h, s.p + Vec3::new(0.0, 0.0, h));
                }
            }
        }
    }
    let mut out: Vec<Found> = worst.into_iter().map(|((a, b), (h, pt))| Found { a, b, point: pt, dist: h - p.min_headroom }).collect();
    out.sort_by(|x, y| x.dist.total_cmp(&y.dist));
    out
}

// ---------------------------------------------------------------- egress

/// Distanta de evacuare: pe fiecare nivel (dupa cota pardoselii), punctele mai departe decat limita.
pub fn egress(models: &[Option<Arc<Model>>], items: &[ElemId], p: &Params) -> Vec<Found> {
    let step = p.step.max(0.1);
    let samples = floor_samples(models, items, step);
    if samples.is_empty() {
        return Vec::new();
    }
    let elem = |id: &ElemId| models.get(id.m as usize).and_then(|m| m.as_ref()).and_then(|m| m.elements.get(id.e as usize).map(|e| (m, e)));
    // niveluri: cote de pardoseala grupate (toleranta 0.6 m)
    let mut levels: Vec<f32> = samples.iter().map(|s| s.p.z).collect();
    levels.sort_by(|a, b| a.total_cmp(b));
    let mut lv: Vec<f32> = Vec::new();
    for z in levels {
        if lv.last().is_none_or(|l| z - l > 0.6) {
            lv.push(z);
        }
    }
    let lowest = lv[0];
    let mut out = Vec::new();
    for &lz in &lv {
        let pts: Vec<&Sample> = samples.iter().filter(|s| (s.p.z - lz).abs() <= 0.6).collect();
        if pts.is_empty() {
            continue;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for s in &pts {
            x0 = x0.min(s.p.x);
            y0 = y0.min(s.p.y);
            x1 = x1.max(s.p.x);
            y1 = y1.max(s.p.y);
        }
        // o margine de exterior de ~2 m in jurul planului: usile exterioare il ating, peretii nu il inchid
        let margin = (2.0f32).max(2.0 * step);
        x0 -= margin;
        y0 -= margin;
        let nx = ((x1 + margin - x0) / step).ceil() as usize + 1;
        let ny = ((y1 + margin - y0) / step).ceil() as usize + 1;
        if nx * ny > 6_000_000 {
            continue;
        }
        let cell = |x: f32, y: f32| -> Option<usize> {
            let i = ((x - x0) / step).floor();
            let j = ((y - y0) / step).floor();
            if i < 0.0 || j < 0.0 || i as usize >= nx || j as usize >= ny {
                None
            } else {
                Some(j as usize * nx + i as usize)
            }
        };
        let center = |k: usize| Vec3::new(x0 + ((k % nx) as f32 + 0.5) * step, y0 + ((k / nx) as f32 + 0.5) * step, lz);
        // 0 = in afara planului, 1 = pardoseala, 2 = blocat (perete / stalp la 1 m)
        let mut g = vec![0u8; nx * ny];
        let mut floor_of: Vec<Option<ElemId>> = vec![None; nx * ny];
        for s in &pts {
            if let Some(k) = cell(s.p.x, s.p.y) {
                g[k] = 1;
                floor_of[k] = Some(s.floor);
            }
        }
        // obstacole: tot ce taie planul la `CUT` deasupra pardoselii (fara usi, spatii, deschideri)
        let zc = lz + CUT;
        let mut exits: Vec<(usize, ElemId)> = Vec::new();
        let mut spaces: Vec<ElemId> = Vec::new();
        for id in items {
            let Some((m, e)) = elem(id) else { continue };
            let t = lower(e);
            let bb = e.aabb;
            if t.contains("space") && bb.min.z <= zc && bb.max.z >= zc {
                spaces.push(*id);
            }
            if not_physical(&t) || is_floor(e) {
                continue;
            }
            // iesiri: usile de la nivelul cel mai de jos, scarile / rampele de la celelalte niveluri
            let door = is_door(&t) && bb.min.z <= zc && bb.max.z >= zc;
            let stair = is_stair(&t) && bb.max.z > lz + 0.3 && bb.min.z < lz + 3.5 && (lz - lowest).abs() > 0.6;
            if door || stair {
                // usa: trecerea prin toata grosimea peretelui (0.4 m de fiecare parte, pe directia subtire a usii)
                let mut pb = bb;
                if door {
                    if bb.max.x - bb.min.x < bb.max.y - bb.min.y {
                        pb.min.x -= 0.4;
                        pb.max.x += 0.4;
                    } else {
                        pb.min.y -= 0.4;
                        pb.max.y += 0.4;
                    }
                }
                for k in cells_in(&pb, x0, y0, step, nx, ny) {
                    exits.push((k, *id));
                }
                continue;
            }
            if bb.min.z > zc || bb.max.z < zc {
                continue;
            }
            // celulele in care elementul taie planul orizontal
            let tris = m.tris(e);
            for tri in tris.as_chunks::<3>().0 {
                let (a, b, c) = (tri[0], tri[1], tri[2]);
                let (zmin, zmax) = (a.z.min(b.z).min(c.z), a.z.max(b.z).max(c.z));
                if zmin > zc || zmax < zc {
                    continue;
                }
                let mut seg = Vec::with_capacity(2);
                for (u, v) in [(a, b), (b, c), (c, a)] {
                    if (u.z - zc) * (v.z - zc) <= 0.0 && (u.z - v.z).abs() > 1e-6 {
                        let t = (zc - u.z) / (v.z - u.z);
                        seg.push(u + (v - u) * t);
                    }
                }
                if seg.len() >= 2 {
                    let (s0, s1) = (seg[0], seg[1]);
                    let n = ((s1 - s0).length() / (step * 0.5)).ceil().max(1.0) as usize;
                    for q in 0..=n {
                        let pt = s0 + (s1 - s0) * (q as f32 / n as f32);
                        if let Some(k) = cell(pt.x, pt.y) {
                            g[k] = 2;
                        }
                    }
                }
            }
        }
        // usile si scarile sunt mereu de trecut (si cand placa se opreste la fata interioara a peretelui)
        for (k, _) in &exits {
            g[*k] = 1;
        }
        // exteriorul: celulele "in afara planului" legate de margine
        let mut outside = vec![false; nx * ny];
        let mut q: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
        for k in 0..nx * ny {
            let (i, j) = (k % nx, k / nx);
            if (i == 0 || j == 0 || i == nx - 1 || j == ny - 1) && g[k] == 0 {
                outside[k] = true;
                q.push_back(k);
            }
        }
        while let Some(k) = q.pop_front() {
            let (i, j) = ((k % nx) as i64, (k / nx) as i64);
            for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (a, b) = (i + di, j + dj);
                if a < 0 || b < 0 || a >= nx as i64 || b >= ny as i64 {
                    continue;
                }
                let kk = b as usize * nx + a as usize;
                if !outside[kk] && g[kk] == 0 {
                    outside[kk] = true;
                    q.push_back(kk);
                }
            }
        }
        let ground = (lz - lowest).abs() <= 0.6;
        // celulele de iesire folosite: la parter doar usile care ating exteriorul; altfel scarile
        // usa exterioara: exteriorul la cel mult ~1.5 m (perete gros, placa ce iese putin din fatada)
        let rr = (1.5 / step).ceil() as i64;
        let near_outside = |k: usize| {
            let (i, j) = ((k % nx) as i64, (k / nx) as i64);
            for dj in -rr..=rr {
                for di in -rr..=rr {
                    let (a, b) = (i + di, j + dj);
                    if a >= 0 && b >= 0 && a < nx as i64 && b < ny as i64 && outside[b as usize * nx + a as usize] {
                        return true;
                    }
                }
            }
            false
        };
        let exit_ids: Vec<ElemId> = {
            let mut v: Vec<ElemId> = exits.iter().map(|(_, id)| *id).collect();
            v.sort();
            v.dedup();
            v
        };
        let mut sources: Vec<(usize, ElemId)> = Vec::new();
        for id in exit_ids {
            let Some((_, e)) = elem(&id) else { continue };
            let t = lower(e);
            let cells: Vec<usize> = exits.iter().filter(|(_, x)| *x == id).map(|(k, _)| *k).collect();
            let ok = if is_door(&t) { ground && cells.iter().any(|k| near_outside(*k)) } else { !ground };
            if ok {
                for k in cells {
                    if g[k] == 1 {
                        sources.push((k, id));
                    }
                }
            }
        }
        if std::env::var_os("MB_EGRESS_DEBUG").is_some() {
            eprintln!("level {lz}: grid {nx}x{ny}, exits {}, sources {}, floor cells {}, blocked {}", exits.len(), sources.len(), g.iter().filter(|v| **v == 1).count(), g.iter().filter(|v| **v == 2).count());
        }
        if sources.is_empty() {
            // nicio iesire recunoscuta pe acest nivel: un singur rezultat, pe placa cea mai mare
            let (sx, sy) = pts.iter().fold((0.0f32, 0.0f32), |a, s| (a.0 + s.p.x, a.1 + s.p.y));
            let (cx, cy) = (sx / pts.len() as f32, sy / pts.len() as f32);
            if let Some(s) = pts.iter().min_by(|a, b| ((a.p.x - cx).powi(2) + (a.p.y - cy).powi(2)).total_cmp(&((b.p.x - cx).powi(2) + (b.p.y - cy).powi(2)))) {
                out.push(Found { a: s.floor, b: s.floor, point: s.p + Vec3::new(0.0, 0.0, 0.1), dist: -p.max_travel });
            }
            continue;
        }
        // Dijkstra pe 8 vecini din toate iesirile
        let mut dist = vec![f32::INFINITY; nx * ny];
        let mut via: Vec<Option<ElemId>> = vec![None; nx * ny];
        let mut heap = std::collections::BinaryHeap::new();
        #[derive(PartialEq)]
        struct Q(f32, usize);
        impl Eq for Q {}
        impl PartialOrd for Q {
            fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for Q {
            fn cmp(&self, o: &Self) -> std::cmp::Ordering {
                o.0.total_cmp(&self.0)
            }
        }
        for (k, id) in &sources {
            dist[*k] = 0.0;
            via[*k] = Some(*id);
            heap.push(Q(0.0, *k));
        }
        let diag = step * std::f32::consts::SQRT_2;
        while let Some(Q(dk, k)) = heap.pop() {
            if dk > dist[k] {
                continue;
            }
            let (i, j) = ((k % nx) as i64, (k / nx) as i64);
            for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let (a, b) = (i + di, j + dj);
                if a < 0 || b < 0 || a >= nx as i64 || b >= ny as i64 {
                    continue;
                }
                let kk = b as usize * nx + a as usize;
                if g[kk] != 1 {
                    continue;
                }
                // pe diagonala nu se trece printre doua celule blocate
                if di != 0 && dj != 0 && (g[j as usize * nx + a as usize] != 1 || g[b as usize * nx + i as usize] != 1) {
                    continue;
                }
                let nd = dk + if di != 0 && dj != 0 { diag } else { step };
                if nd < dist[kk] {
                    dist[kk] = nd;
                    via[kk] = via[k];
                    heap.push(Q(nd, kk));
                }
            }
        }
        // rezultat: pe fiecare spatiu (sau placa) punctul cel mai departat, daca trece de limita
        let owner_of = |k: usize| -> ElemId {
            let c = center(k);
            spaces
                .iter()
                .find(|s| elem(s).map(|(_, e)| c.x >= e.aabb.min.x && c.x <= e.aabb.max.x && c.y >= e.aabb.min.y && c.y <= e.aabb.max.y).unwrap_or(false))
                .copied()
                .or(floor_of[k])
                .unwrap_or(ElemId { m: 0, e: 0 })
        };
        let mut worst: HashMap<ElemId, (f32, usize, Option<ElemId>)> = HashMap::new();
        for k in 0..nx * ny {
            if g[k] != 1 || floor_of[k].is_none() || !dist[k].is_finite() {
                continue;
            }
            let w = worst.entry(owner_of(k)).or_insert((0.0, k, None));
            if dist[k] > w.0 {
                *w = (dist[k], k, via[k]);
            }
        }
        for (owner, (d, k, exit)) in worst {
            if d > p.max_travel {
                out.push(Found { a: owner, b: exit.unwrap_or(owner), point: center(k) + Vec3::new(0.0, 0.0, 0.1), dist: p.max_travel - d });
            }
        }
        // zone de pardoseala fara niciun drum spre o iesire: doar cele mari (>= 4 m2); bucatile de placa
        // inchise intre pereti sau sub ei nu conteaza
        let min_cells = (4.0 / (step * step)).ceil() as usize;
        let mut seen = vec![false; nx * ny];
        for start in 0..nx * ny {
            if seen[start] || g[start] != 1 || floor_of[start].is_none() || dist[start].is_finite() {
                continue;
            }
            let mut comp = vec![start];
            seen[start] = true;
            let mut qi = 0;
            while qi < comp.len() {
                let k = comp[qi];
                qi += 1;
                let (i, j) = ((k % nx) as i64, (k / nx) as i64);
                for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (a, b) = (i + di, j + dj);
                    if a < 0 || b < 0 || a >= nx as i64 || b >= ny as i64 {
                        continue;
                    }
                    let kk = b as usize * nx + a as usize;
                    if !seen[kk] && g[kk] == 1 && floor_of[kk].is_some() && !dist[kk].is_finite() {
                        seen[kk] = true;
                        comp.push(kk);
                    }
                }
            }
            if comp.len() >= min_cells {
                // punctul reprezentativ: celula zonei cea mai apropiata de centrul ei
                let (sx, sy) = comp.iter().fold((0.0, 0.0), |a, k| (a.0 + center(*k).x, a.1 + center(*k).y));
                let (cx, cy) = (sx / comp.len() as f32, sy / comp.len() as f32);
                let k = *comp.iter().min_by(|a, b| {
                    let (pa, pb) = (center(**a), center(**b));
                    ((pa.x - cx).powi(2) + (pa.y - cy).powi(2)).total_cmp(&((pb.x - cx).powi(2) + (pb.y - cy).powi(2)))
                }).unwrap_or(&start);
                let owner = owner_of(k);
                out.push(Found { a: owner, b: owner, point: center(k) + Vec3::new(0.0, 0.0, 0.1), dist: -p.max_travel });
            }
        }
    }
    out.sort_by(|x, y| x.dist.total_cmp(&y.dist));
    out
}

fn cells_in(bb: &crate::model::Aabb, x0: f32, y0: f32, step: f32, nx: usize, ny: usize) -> Vec<usize> {
    let mut v = Vec::new();
    let i0 = (((bb.min.x - x0) / step).floor().max(0.0)) as usize;
    let j0 = (((bb.min.y - y0) / step).floor().max(0.0)) as usize;
    let i1 = (((bb.max.x - x0) / step).floor().max(0.0) as usize).min(nx - 1);
    let j1 = (((bb.max.y - y0) / step).floor().max(0.0) as usize).min(ny - 1);
    for j in j0..=j1 {
        for i in i0..=i1 {
            v.push(j * nx + i);
        }
    }
    v
}
