//! As-built check: the model against the point cloud (E57 / LAS scan).
//!
//! The cloud becomes an occupancy grid (voxels with points). Each model element is sampled on its
//! surface; a sample is "confirmed" if there are points at most ~tolerance away from it. The percentage of
//! confirmed samples (the coverage) gives the status: built / partial / missing; the elements outside
//! the scanned area stay "not scanned".

use crate::model::{Aabb, ElemId, Model, PointCloud};
use glam::Vec3;
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Status {
    Built,
    Partial,
    Missing,
    NotScanned,
}

impl Status {
    pub const ALL: [Status; 4] = [Status::Built, Status::Partial, Status::Missing, Status::NotScanned];
    pub fn label(self) -> &'static str {
        match self {
            Status::Built => "Built",
            Status::Partial => "Partial",
            Status::Missing => "Missing",
            Status::NotScanned => "Not scanned",
        }
    }
    pub fn color(self) -> [u8; 4] {
        match self {
            Status::Built => [70, 190, 110, 255],
            Status::Partial => [235, 180, 60, 255],
            Status::Missing => [230, 75, 75, 255],
            Status::NotScanned => [140, 145, 155, 255],
        }
    }
}

#[derive(Clone, Debug)]
pub struct ElemResult {
    pub id: ElemId,
    /// 0..1: samples confirmed by points
    pub coverage: f32,
    pub samples: u32,
    pub status: Status,
}

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// maximum point - surface distance (m)
    pub tol: f32,
    /// minimum coverage for "built" / "partial"
    pub built: f32,
    pub partial: f32,
}

impl Default for Params {
    fn default() -> Self {
        // on a real scan the hidden faces (back, interior) have no points: the thresholds are low
        Params { tol: 0.05, built: 0.30, partial: 0.08 }
    }
}

const BIAS: i64 = 1 << 20;

fn key(c: [i64; 3]) -> u64 {
    (((c[0] + BIAS) as u64) << 42) | (((c[1] + BIAS) as u64) << 21) | ((c[2] + BIAS) as u64)
}

pub struct Occupancy {
    cells: HashSet<u64>,
    cell: f32,
    pub bounds: Aabb,
    pub points: usize,
}

impl Occupancy {
    pub fn build(clouds: &[&PointCloud], cell: f32) -> Occupancy {
        let cell = cell.max(0.005);
        let mut cells = HashSet::new();
        let mut bounds = Aabb::EMPTY;
        let mut points = 0;
        for c in clouds {
            for p in &c.pos {
                let k = [(p.x / cell).floor() as i64, (p.y / cell).floor() as i64, (p.z / cell).floor() as i64];
                cells.insert(key(k));
                bounds.grow(*p);
                points += 1;
            }
        }
        Occupancy { cells, cell, bounds, points }
    }
    /// are there points in the cell of the point or in its neighbourhood (radius `r` cells)?
    fn near(&self, p: Vec3, r: i64) -> bool {
        let c = [(p.x / self.cell).floor() as i64, (p.y / self.cell).floor() as i64, (p.z / self.cell).floor() as i64];
        for dx in -r..=r {
            for dy in -r..=r {
                for dz in -r..=r {
                    if self.cells.contains(&key([c[0] + dx, c[1] + dy, c[2] + dz])) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// Samples on the surface of an element: at most `max` points, distributed by triangle area
/// (R2 sequence - uniform and deterministic).
fn samples(m: &Model, start: u32, count: u32, spacing: f32, max: usize) -> Vec<Vec3> {
    let tris = start as usize..(start + count) as usize;
    let area: f32 = tris.clone().map(|t| tri_area(m, t)).sum();
    if area <= 0.0 {
        return Vec::new();
    }
    let want = ((area / (spacing * spacing)).ceil() as usize).clamp(4, max);
    let per_area = want as f32 / area;
    let mut out = Vec::with_capacity(want + 8);
    let (a1, a2) = (0.754_877_7_f32, 0.569_840_3_f32);
    let mut k = 0u32;
    for t in tris {
        let (Some(p0), Some(p1), Some(p2)) = (m.pos.get(t * 3).copied(), m.pos.get(t * 3 + 1).copied(), m.pos.get(t * 3 + 2).copied()) else { continue };
        let n = (tri_area(m, t) * per_area).round().max(1.0) as u32;
        for _ in 0..n {
            k += 1;
            let mut u = (0.5 + a1 * k as f32).fract();
            let mut v = (0.5 + a2 * k as f32).fract();
            if u + v > 1.0 {
                u = 1.0 - u;
                v = 1.0 - v;
            }
            out.push(p0 + (p1 - p0) * u + (p2 - p0) * v);
            if out.len() >= max {
                return out;
            }
        }
    }
    out
}

fn tri_area(m: &Model, t: usize) -> f32 {
    match (m.pos.get(t * 3), m.pos.get(t * 3 + 1), m.pos.get(t * 3 + 2)) {
        (Some(a), Some(b), Some(c)) => (*b - *a).cross(*c - *a).length() * 0.5,
        _ => 0.0,
    }
}

/// Checks the given elements (model, element) against the occupancy grid.
pub fn check(items: &[(usize, &Model, u32)], occ: &Occupancy, p: Params) -> Vec<ElemResult> {
    use rayon::prelude::*;
    // the searched neighbourhood ~ tolerance (cell = tolerance / 2 -> radius 2 cells)
    let r = ((p.tol / occ.cell).ceil() as i64).clamp(1, 4);
    let scan = {
        let mut b = occ.bounds;
        if b.is_valid() {
            b.min -= Vec3::splat(p.tol);
            b.max += Vec3::splat(p.tol);
        }
        b
    };
    items
        .par_iter()
        .map(|(mi, m, ei)| {
            let e = &m.elements[*ei as usize];
            let id = ElemId { m: *mi as u32, e: *ei };
            let bb = e.aabb;
            let inside = scan.is_valid() && bb.max.x >= scan.min.x && bb.min.x <= scan.max.x && bb.max.y >= scan.min.y && bb.min.y <= scan.max.y && bb.max.z >= scan.min.z && bb.min.z <= scan.max.z;
            if !inside || e.tri_count == 0 {
                return ElemResult { id, coverage: 0.0, samples: 0, status: Status::NotScanned };
            }
            let size = bb.size().length();
            let spacing = (size / 30.0).clamp(p.tol * 2.0, 0.5);
            let s = samples(m, e.tri_start, e.tri_count, spacing, 1500);
            // only the samples in the scanned area count
            let s: Vec<Vec3> = s.into_iter().filter(|q| q.cmpge(scan.min).all() && q.cmple(scan.max).all()).collect();
            if s.is_empty() {
                return ElemResult { id, coverage: 0.0, samples: 0, status: Status::NotScanned };
            }
            let hits = s.iter().filter(|q| occ.near(**q, r)).count();
            let cov = hits as f32 / s.len() as f32;
            let status = if cov >= p.built {
                Status::Built
            } else if cov >= p.partial {
                Status::Partial
            } else {
                Status::Missing
            };
            ElemResult { id, coverage: cov, samples: s.len() as u32, status }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_unique_for_neighbours() {
        assert_ne!(key([0, 0, 1]), key([0, 1, 0]));
        assert_ne!(key([-1, 0, 0]), key([0, 0, 0]));
    }
}
