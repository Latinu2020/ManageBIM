//! Linii vizibile / ascunse (Hidden Line Removal) pentru desenul 2D din cubul de sectiune,
//! ca vederile de desen din Inventor: muchiile reale ale formei (nu diagonalele plasei de
//! triunghiuri), cele vazute cu linie plina, cele acoperite cu linie intrerupta.
//!
//! Metoda: (1) muchii caracteristice per element - margini libere, muchii unde fetele vecine fac
//! un unghi > `CREASE_DEG` si siluete (o fata spre privitor, cealalta nu); (2) buffer de adancime
//! ortografic pe fata cubului, rasterizat din triunghiurile de dupa planul de taiere; (3) fiecare
//! muchie e esantionata pe buffer si impartita in bucati vizibile / ascunse.
use glam::Vec3;
use std::collections::HashMap;

/// unghiul minim intre fetele vecine pentru o muchie desenata (sub el, suprafata e "neteda")
pub const CREASE_DEG: f32 = 28.0;

#[derive(Clone, Copy, Debug)]
pub struct HlrView {
    /// axa de privire (0 = X, 1 = Y, 2 = Z)
    pub axis: usize,
    /// pozitia planului de taiere pe axa
    pub plane: f32,
    /// +1: ce e "in spatele" planului are coordonata mai mare; -1: mai mica
    pub sign: f32,
    /// zona desenata (cubul de sectiune sau scena)
    pub bmin: Vec3,
    pub bmax: Vec3,
    /// rezolutia bufferului de adancime pe latura lunga (pixeli)
    pub res: usize,
}

impl HlrView {
    fn uv(&self, p: Vec3) -> (f32, f32) {
        match self.axis {
            0 => (p.y, p.z),
            1 => (p.x, p.z),
            _ => (p.x, p.y),
        }
    }
    fn uv_range(&self) -> ((f32, f32), (f32, f32)) {
        (self.uv(self.bmin), self.uv(self.bmax))
    }
    /// adancimea de la planul de taiere, in directia privirii (>= 0 = dupa plan)
    fn depth(&self, p: Vec3) -> f32 {
        self.sign * (p[self.axis] - self.plane)
    }
    fn far(&self) -> f32 {
        let f = if self.sign > 0.0 { self.bmax[self.axis] } else { self.bmin[self.axis] };
        self.depth(Vec3::splat(f)).max(0.0) + 1e-3
    }
    fn view_dir(&self) -> Vec3 {
        let mut v = Vec3::ZERO;
        v[self.axis] = self.sign;
        v
    }
}

struct ZBuf {
    w: usize,
    h: usize,
    u0: f32,
    v0: f32,
    cell: f32,
    z: Vec<f32>,
}

impl ZBuf {
    fn new(view: &HlrView) -> Self {
        let ((u0, v0), (u1, v1)) = view.uv_range();
        let (du, dv) = ((u1 - u0).max(1e-3), (v1 - v0).max(1e-3));
        let cell = du.max(dv) / view.res.max(64) as f32;
        let w = ((du / cell).ceil() as usize + 1).min(8192);
        let h = ((dv / cell).ceil() as usize + 1).min(8192);
        ZBuf { w, h, u0, v0, cell, z: vec![f32::INFINITY; w * h] }
    }
    fn raster(&mut self, view: &HlrView, t: [Vec3; 3]) {
        let far = view.far();
        let p: Vec<(f32, f32, f32)> = t
            .iter()
            .map(|q| {
                let (u, v) = view.uv(*q);
                ((u - self.u0) / self.cell, (v - self.v0) / self.cell, view.depth(*q))
            })
            .collect();
        // complet in fata planului sau dupa zona: nu acopera nimic
        if p.iter().all(|q| q.2 < 0.0) || p.iter().all(|q| q.2 > far) {
            return;
        }
        let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
        if area.abs() < 1e-6 {
            return; // vazut din profil
        }
        let x0 = p.iter().map(|q| q.0).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
        let x1 = (p.iter().map(|q| q.0).fold(f32::MIN, f32::max).ceil() as isize).min(self.w as isize - 1);
        let y0 = p.iter().map(|q| q.1).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
        let y1 = (p.iter().map(|q| q.1).fold(f32::MIN, f32::max).ceil() as isize).min(self.h as isize - 1);
        if x1 < 0 || y1 < 0 {
            return;
        }
        let inv = 1.0 / area;
        for y in y0..=y1 as usize {
            let py = y as f32 + 0.5;
            for x in x0..=x1 as usize {
                let px = x as f32 + 0.5;
                let w0 = ((p[1].0 - px) * (p[2].1 - py) - (p[2].0 - px) * (p[1].1 - py)) * inv;
                let w1 = ((p[2].0 - px) * (p[0].1 - py) - (p[0].0 - px) * (p[2].1 - py)) * inv;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                    continue;
                }
                let d = w0 * p[0].2 + w1 * p[1].2 + w2 * p[2].2;
                // partea de dinaintea planului e taiata (sectiune); ce e dupa zona nu conteaza
                if !(0.0..=far).contains(&d) {
                    continue;
                }
                let k = y * self.w + x;
                if d < self.z[k] {
                    self.z[k] = d;
                }
            }
        }
    }
    /// adancimea minima in jurul punctului (3x3), ca muchiile de pe marginea fetelor sa nu clipeasca
    fn at(&self, u: f32, v: f32) -> f32 {
        let x = ((u - self.u0) / self.cell) as isize;
        let y = ((v - self.v0) / self.cell) as isize;
        let mut best = f32::NEG_INFINITY;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (xx, yy) = (x + dx, y + dy);
                if xx >= 0 && yy >= 0 && (xx as usize) < self.w && (yy as usize) < self.h {
                    best = best.max(self.z[yy as usize * self.w + xx as usize]);
                }
            }
        }
        // maximul vecinilor: o muchie e ascunsa doar daca TOT vecinatatea e mai aproape de privitor
        best
    }
}

/// Muchiile caracteristice ale unei plase (triunghiuri ale unui singur element).
pub fn feature_edges(tris: &[[Vec3; 3]], view_dir: Vec3) -> Vec<(Vec3, Vec3)> {
    let key = |p: Vec3| [(p.x * 2000.0).round() as i32, (p.y * 2000.0).round() as i32, (p.z * 2000.0).round() as i32];
    // muchie -> (capete, normala primei fete, normala celei de-a doua, numar de fete)
    let mut map: HashMap<([i32; 3], [i32; 3]), (Vec3, Vec3, Vec3, Option<Vec3>, u32)> = HashMap::new();
    for t in tris {
        let n = (t[1] - t[0]).cross(t[2] - t[0]);
        if n.length_squared() < 1e-14 {
            continue;
        }
        let n = n.normalize();
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (ka, kb) = (key(a), key(b));
            if ka == kb {
                continue;
            }
            let k = if ka < kb { (ka, kb) } else { (kb, ka) };
            map.entry(k)
                .and_modify(|e| {
                    if e.3.is_none() {
                        e.3 = Some(n);
                    }
                    e.4 += 1;
                })
                .or_insert((a, b, n, None, 1));
        }
    }
    let cos_crease = CREASE_DEG.to_radians().cos();
    let mut out = Vec::new();
    for (_, (a, b, n1, n2, count)) in map {
        let keep = match (count, n2) {
            (1, _) => true, // margine libera
            (_, Some(n2)) => {
                // muchie (unghi intre fete) sau silueta (o fata spre privitor, cealalta nu)
                n1.dot(n2) < cos_crease || (n1.dot(view_dir) * n2.dot(view_dir)) < -1e-6
            }
            _ => false,
        };
        if keep {
            out.push((a, b));
        }
    }
    out
}

/// taie segmentul la cutia [bmin, bmax] si la semispatiul de dupa plan (slab method)
fn clip(view: &HlrView, p: Vec3, q: Vec3) -> Option<(Vec3, Vec3)> {
    let d = q - p;
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for ax in 0..3 {
        let (a, b) = (view.bmin[ax] - 1e-3, view.bmax[ax] + 1e-3);
        if d[ax].abs() < 1e-9 {
            if p[ax] < a || p[ax] > b {
                return None;
            }
        } else {
            let (mut lo, mut hi) = ((a - p[ax]) / d[ax], (b - p[ax]) / d[ax]);
            if lo > hi {
                std::mem::swap(&mut lo, &mut hi);
            }
            t0 = t0.max(lo);
            t1 = t1.min(hi);
        }
    }
    // dupa planul de taiere: depth(p + d t) >= 0
    let (dp, dd) = (view.depth(p), view.sign * d[view.axis]);
    if dd.abs() < 1e-9 {
        if dp < -1e-4 {
            return None;
        }
    } else {
        let t = -dp / dd;
        if dd > 0.0 {
            t0 = t0.max(t);
        } else {
            t1 = t1.min(t);
        }
    }
    (t0 < t1).then(|| (p + d * t0, p + d * t1))
}

/// Rezultatul: segmente 3D vizibile si ascunse (coordonatele scenei).
pub struct HlrResult {
    pub visible: Vec<(Vec3, Vec3)>,
    pub hidden: Vec<(Vec3, Vec3)>,
}

/// `elements`: triunghiurile fiecarui element (o lista per element, ca muchiile sa nu se amestece).
pub fn compute(view: &HlrView, elements: &[Vec<[Vec3; 3]>], want_hidden: bool, max_segs: usize) -> HlrResult {
    let mut zb = ZBuf::new(view);
    for tris in elements {
        for t in tris {
            zb.raster(view, *t);
        }
    }
    let vd = view.view_dir();
    let eps_base = zb.cell * 2.5 + 0.004;
    let mut res = HlrResult { visible: Vec::new(), hidden: Vec::new() };
    for tris in elements {
        for (a, b) in feature_edges(tris, vd) {
            let Some((p, q)) = clip(view, a, b) else { continue };
            let (pu, pv) = view.uv(p);
            let (qu, qv) = view.uv(q);
            let len_px = ((qu - pu).hypot(qv - pv)) / zb.cell;
            if len_px < 0.5 {
                continue; // muchie paralela cu privirea: se proiecteaza intr-un punct
            }
            // panta in adancime: muchiile inclinate au nevoie de toleranta mai mare
            let slope = (view.depth(q) - view.depth(p)).abs() / len_px.max(1.0);
            let eps = eps_base + slope * 1.5;
            let steps = (len_px.ceil() as usize).clamp(2, 4000);
            let mut run_start = 0usize;
            let mut run_vis: Option<bool> = None;
            let at = |k: usize| p + (q - p) * (k as f32 / steps as f32);
            for k in 0..=steps {
                let s = at(k);
                let (u, v) = view.uv(s);
                let vis = view.depth(s) <= zb.at(u, v) + eps;
                match run_vis {
                    None => {
                        run_vis = Some(vis);
                        run_start = k;
                    }
                    Some(rv) if rv != vis => {
                        let seg = (at(run_start), at(k));
                        if rv {
                            res.visible.push(seg);
                        } else if want_hidden {
                            res.hidden.push(seg);
                        }
                        run_vis = Some(vis);
                        run_start = k;
                    }
                    _ => {}
                }
            }
            if let Some(rv) = run_vis {
                if run_start < steps {
                    let seg = (at(run_start), q);
                    if rv {
                        res.visible.push(seg);
                    } else if want_hidden {
                        res.hidden.push(seg);
                    }
                }
            }
            if res.visible.len() + res.hidden.len() >= max_segs {
                return res;
            }
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    /// cub unitate [0,1]^3 ca 12 triunghiuri
    fn cube(o: Vec3) -> Vec<[Vec3; 3]> {
        let c = |x: f32, y: f32, z: f32| o + Vec3::new(x, y, z);
        let q = |a: Vec3, b: Vec3, cc: Vec3, d: Vec3| [[a, b, cc], [a, cc, d]];
        let mut t = Vec::new();
        t.extend(q(c(0., 0., 0.), c(1., 0., 0.), c(1., 1., 0.), c(0., 1., 0.)));
        t.extend(q(c(0., 0., 1.), c(1., 0., 1.), c(1., 1., 1.), c(0., 1., 1.)));
        t.extend(q(c(0., 0., 0.), c(1., 0., 0.), c(1., 0., 1.), c(0., 0., 1.)));
        t.extend(q(c(0., 1., 0.), c(1., 1., 0.), c(1., 1., 1.), c(0., 1., 1.)));
        t.extend(q(c(0., 0., 0.), c(0., 1., 0.), c(0., 1., 1.), c(0., 0., 1.)));
        t.extend(q(c(1., 0., 0.), c(1., 1., 0.), c(1., 1., 1.), c(1., 0., 1.)));
        t
    }

    #[test]
    fn cube_has_12_feature_edges_and_no_diagonals() {
        let e = feature_edges(&cube(Vec3::ZERO), -Vec3::Z);
        assert_eq!(e.len(), 12, "{e:?}");
    }

    #[test]
    fn top_view_hides_the_lower_cube() {
        // cub mic sub un cub mare, privit de sus: cubul mic e complet ascuns
        let big = cube(Vec3::new(0.0, 0.0, 2.0)).iter().map(|t| t.map(|p| Vec3::new(p.x * 4.0 - 1.5, p.y * 4.0 - 1.5, p.z))).collect::<Vec<_>>();
        let small = cube(Vec3::ZERO);
        let view = HlrView { axis: 2, plane: 5.0, sign: -1.0, bmin: Vec3::splat(-3.0), bmax: Vec3::splat(5.0), res: 400 };
        let r = compute(&view, &[big, small], true, 100_000);
        let vis_len: f32 = r.visible.iter().map(|(a, b)| (*a - *b).length()).sum();
        let hid_len: f32 = r.hidden.iter().map(|(a, b)| (*a - *b).length()).sum();
        // conturul cubului mare (4 x 4 m) e vizibil, cubul mic (4 x 1 m de contur) e ascuns
        assert!(vis_len > 14.0 && vis_len < 40.0, "visible {vis_len}");
        assert!(hid_len > 3.0, "hidden {hid_len}");
    }

    #[test]
    fn cut_plane_removes_geometry_in_front() {
        let view = HlrView { axis: 2, plane: 0.5, sign: -1.0, bmin: Vec3::splat(-1.0), bmax: Vec3::splat(2.0), res: 200 };
        let r = compute(&view, &[cube(Vec3::ZERO)], true, 10_000);
        // nicio muchie nu e deasupra planului de taiere
        assert!(r.visible.iter().chain(r.hidden.iter()).all(|(a, b)| a.z <= 0.5 + 1e-3 && b.z <= 0.5 + 1e-3));
        assert!(!r.visible.is_empty());
    }
}
