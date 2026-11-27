//! ACIS solids (3DSOLID) from DWG / DXF through the built-in reader: acadrust (SAT / SAB) -> B-rep description
//! -> OpenCascade (mbo_brep_from_desc) -> triangles. The fixtures are generated in the test with the
//! acadrust writer: ACIS primitives (box, cylinder, sphere, torus, cone) and bodies built record by
//! record (box with a hole = boolean subtraction, box with a rounded edge = fillet, B-spline face),
//! exactly as AutoCAD saves them (plane / cone-surface / ellipse-curve / spline-surface).
use acadrust::entities::acis::{primitives, SatDocument, SatPointer, SatToken, Sense, Sidedness};
use acadrust::entities::solid3d::{AcisData, Solid3D};
use acadrust::tables::Layer;
use acadrust::types::Color;
use acadrust::{CadDocument, DwgWriter, DxfVersion, DxfWriter, EntityType, SabWriter};
use bimrust::model::{RawElement, RawModel};
use bimrust::*;
use glam::DVec3;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::path::PathBuf;

// ------------------------------------------------------------------ minimal SAT builder
fn ptr(i: i32) -> SatPointer {
    SatPointer::new(i)
}

struct Brep {
    sat: SatDocument,
    pts: Vec<[f64; 3]>,
    vrec: Vec<i32>,
    lines: HashMap<(usize, usize), i32>,
    faces: Vec<i32>,
    coedges: Vec<(i32, i32)>,
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

impl Brep {
    fn new() -> Self {
        Brep { sat: SatDocument::new_body(), pts: Vec::new(), vrec: Vec::new(), lines: HashMap::new(), faces: Vec::new(), coedges: Vec::new() }
    }
    fn v(&mut self, p: [f64; 3]) -> usize {
        if let Some(i) = self.pts.iter().position(|q| (0..3).all(|k| (q[k] - p[k]).abs() < 1e-9)) {
            return i;
        }
        let pr = self.sat.add_point(p[0], p[1], p[2]);
        let vr = self.sat.add_vertex(SatPointer::NULL, ptr(pr));
        self.pts.push(p);
        self.vrec.push(vr);
        self.pts.len() - 1
    }
    fn line(&mut self, a: [f64; 3], b: [f64; 3]) -> (i32, bool) {
        let (ia, ib) = (self.v(a), self.v(b));
        if let Some(&e) = self.lines.get(&(ia, ib)) {
            return (e, false);
        }
        if let Some(&e) = self.lines.get(&(ib, ia)) {
            return (e, true);
        }
        let d = sub(b, a);
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let c = self.sat.add_straight_curve(a, [d[0] / len, d[1] / len, d[2] / len]);
        let e = self.sat.add_edge(ptr(self.vrec[ia]), 0.0, ptr(self.vrec[ib]), len, SatPointer::NULL, ptr(c), Sense::Forward);
        self.lines.insert((ia, ib), e);
        (e, false)
    }
    /// arc (or full circle if a == b) on the circle (center, normal, major axis = radius)
    #[allow(clippy::too_many_arguments)]
    fn arc(&mut self, a: [f64; 3], b: [f64; 3], center: [f64; 3], normal: [f64; 3], major: [f64; 3], t0: f64, t1: f64) -> i32 {
        let (ia, ib) = (self.v(a), self.v(b));
        let c = self.sat.add_ellipse_curve(center, normal, major, 1.0);
        self.sat.add_edge(ptr(self.vrec[ia]), t0, ptr(self.vrec[ib]), t1, SatPointer::NULL, ptr(c), Sense::Forward)
    }
    /// full ellipse (one vertex, on the major axis)
    fn ellipse(&mut self, center: [f64; 3], major: [f64; 3], ratio: f64) -> i32 {
        let a = [center[0] + major[0], center[1] + major[1], center[2] + major[2]];
        let ia = self.v(a);
        let c = self.sat.add_ellipse_curve(center, [0.0, 0.0, 1.0], major, ratio);
        self.sat.add_edge(ptr(self.vrec[ia]), 0.0, ptr(self.vrec[ia]), 2.0 * PI, SatPointer::NULL, ptr(c), Sense::Forward)
    }
    fn poly(&mut self, p: &[[f64; 3]]) -> Vec<(i32, bool)> {
        (0..p.len()).map(|i| self.line(p[i], p[(i + 1) % p.len()])).collect()
    }
    fn plane(&mut self, root: [f64; 3], normal: [f64; 3], u: [f64; 3]) -> i32 {
        self.sat.add_plane_surface(root, normal, u)
    }
    fn face(&mut self, surf: i32, sense: Sense, loops: Vec<Vec<(i32, bool)>>) {
        let first = self.sat.records.len() as i32;
        let mut starts = Vec::new();
        let mut idx = first;
        for l in &loops {
            starts.push(idx);
            idx += l.len() as i32 + 1;
        }
        let face_idx = idx;
        for (li, l) in loops.iter().enumerate() {
            let (base, n) = (starts[li], l.len() as i32);
            for (i, &(e, rev)) in l.iter().enumerate() {
                let i = i as i32;
                let c = self.sat.add_coedge(ptr(base + (i + 1) % n), ptr(base + (i + n - 1) % n), SatPointer::NULL, ptr(e), if rev { Sense::Reversed } else { Sense::Forward }, ptr(base + n));
                assert_eq!(c, base + i);
                self.coedges.push((c, e));
            }
            let next = if li + 1 < loops.len() { ptr(starts[li + 1] + loops[li + 1].len() as i32) } else { SatPointer::NULL };
            assert_eq!(self.sat.add_loop(next, ptr(base), ptr(face_idx)), base + n);
        }
        let first_loop = if loops.is_empty() { SatPointer::NULL } else { ptr(starts[0] + loops[0].len() as i32) };
        let f = self.sat.add_face(SatPointer::NULL, first_loop, SatPointer::NULL, ptr(surf), sense, Sidedness::Single);
        assert_eq!(f, face_idx);
        self.faces.push(f);
    }
    fn set(&mut self, rec: i32, tok: usize, p: i32) {
        assert_eq!(self.sat.records[rec as usize].index, rec);
        self.sat.records[rec as usize].tokens[tok] = SatToken::Pointer(ptr(p));
    }
    fn finish(mut self) -> SatDocument {
        let shell = self.sat.add_shell(ptr(self.faces[0]), SatPointer::NULL);
        let lump = self.sat.add_lump(ptr(shell), ptr(0));
        self.set(shell, 5, lump);
        self.set(0, 1, lump);
        for i in 0..self.faces.len() {
            let f = self.faces[i];
            if i + 1 < self.faces.len() {
                self.set(f, 1, self.faces[i + 1]);
            }
            self.set(f, 3, shell);
        }
        // the coedge partners (ring on the edge) and the edge -> coedge, vertex -> edge links
        let mut by_edge: HashMap<i32, Vec<i32>> = HashMap::new();
        for &(c, e) in &self.coedges {
            by_edge.entry(e).or_default().push(c);
        }
        for (e, cs) in by_edge {
            for k in 0..cs.len() {
                if cs.len() > 1 {
                    self.set(cs[k], 3, cs[(k + 1) % cs.len()]);
                }
            }
            self.set(e, 5, cs[0]);
            for t in [1usize, 3] {
                if let Some(SatToken::Pointer(v)) = self.sat.records[e as usize].tokens.get(t).cloned() {
                    self.set(v.0, 1, e);
                }
            }
        }
        self.sat
    }
}

/// Box [0,100]x[0,100]x[0,20] mm minus the r 20 cylinder at (50,50): the hole face is a cone-surface
/// (cylinder) with "reversed" sense, the top / bottom loops each have a hole circle.
fn box_with_hole() -> SatDocument {
    let mut b = Brep::new();
    let (z0, z1) = (0.0, 20.0);
    let p = |x: f64, y: f64, z: f64| [x, y, z];
    let top = b.plane(p(0.0, 0.0, z1), [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let bot = b.plane(p(0.0, 0.0, z0), [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]);
    let c_top = b.arc(p(70.0, 50.0, z1), p(70.0, 50.0, z1), p(50.0, 50.0, z1), [0.0, 0.0, 1.0], [20.0, 0.0, 0.0], 0.0, 2.0 * PI);
    let c_bot = b.arc(p(70.0, 50.0, z0), p(70.0, 50.0, z0), p(50.0, 50.0, z0), [0.0, 0.0, 1.0], [20.0, 0.0, 0.0], 0.0, 2.0 * PI);
    let outer = b.poly(&[p(0.0, 0.0, z1), p(100.0, 0.0, z1), p(100.0, 100.0, z1), p(0.0, 100.0, z1)]);
    b.face(top, Sense::Forward, vec![outer, vec![(c_top, true)]]);
    let outer = b.poly(&[p(0.0, 0.0, z0), p(0.0, 100.0, z0), p(100.0, 100.0, z0), p(100.0, 0.0, z0)]);
    b.face(bot, Sense::Forward, vec![outer, vec![(c_bot, false)]]);
    for (q, n) in [
        ([p(0.0, 0.0, z0), p(100.0, 0.0, z0), p(100.0, 0.0, z1), p(0.0, 0.0, z1)], [0.0, -1.0, 0.0]),
        ([p(100.0, 0.0, z0), p(100.0, 100.0, z0), p(100.0, 100.0, z1), p(100.0, 0.0, z1)], [1.0, 0.0, 0.0]),
        ([p(100.0, 100.0, z0), p(0.0, 100.0, z0), p(0.0, 100.0, z1), p(100.0, 100.0, z1)], [0.0, 1.0, 0.0]),
        ([p(0.0, 100.0, z0), p(0.0, 0.0, z0), p(0.0, 0.0, z1), p(0.0, 100.0, z1)], [-1.0, 0.0, 0.0]),
    ] {
        let s = b.plane(q[0], n, sub(q[1], q[0]).map(|x| x / 100.0));
        let l = b.poly(&q);
        b.face(s, Sense::Forward, vec![l]);
    }
    // hole: cylinder with the normal pointing outward from the axis -> "reversed" face (normal toward the void)
    let cyl = b.sat.add_cone_surface([50.0, 50.0, 0.0], [0.0, 0.0, 1.0], [20.0, 0.0, 0.0], 1.0, 1.0, 0.0);
    b.face(cyl, Sense::Reversed, vec![vec![(c_top, false)], vec![(c_bot, true)]]);
    b.finish()
}

/// Box [0,100]x[0,50]x[0,20] mm with the vertical edge at (100,50) rounded with r 10 (fillet):
/// the fillet face is a cylinder (cone-surface), its boundaries are arcs (ellipse-curve).
fn filleted_box() -> SatDocument {
    let mut b = Brep::new();
    let (z0, z1, r) = (0.0, 20.0, 10.0);
    let p = |x: f64, y: f64, z: f64| [x, y, z];
    let arc_b = b.arc(p(100.0, 40.0, z0), p(90.0, 50.0, z0), p(90.0, 40.0, z0), [0.0, 0.0, 1.0], [r, 0.0, 0.0], 0.0, PI / 2.0);
    let arc_t = b.arc(p(100.0, 40.0, z1), p(90.0, 50.0, z1), p(90.0, 40.0, z1), [0.0, 0.0, 1.0], [r, 0.0, 0.0], 0.0, PI / 2.0);
    // top (+z): (0,0) (100,0) (100,40) ~arc~ (90,50) (0,50)
    let top = b.plane(p(0.0, 0.0, z1), [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let mut l = vec![b.line(p(0.0, 0.0, z1), p(100.0, 0.0, z1)), b.line(p(100.0, 0.0, z1), p(100.0, 40.0, z1)), (arc_t, false)];
    l.push(b.line(p(90.0, 50.0, z1), p(0.0, 50.0, z1)));
    l.push(b.line(p(0.0, 50.0, z1), p(0.0, 0.0, z1)));
    b.face(top, Sense::Forward, vec![l]);
    // bottom (-z): reverse direction
    let bot = b.plane(p(0.0, 0.0, z0), [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]);
    let mut l = vec![b.line(p(0.0, 0.0, z0), p(0.0, 50.0, z0)), b.line(p(0.0, 50.0, z0), p(90.0, 50.0, z0)), (arc_b, true)];
    l.push(b.line(p(100.0, 40.0, z0), p(100.0, 0.0, z0)));
    l.push(b.line(p(100.0, 0.0, z0), p(0.0, 0.0, z0)));
    b.face(bot, Sense::Forward, vec![l]);
    for (q, n) in [
        (vec![p(0.0, 0.0, z0), p(100.0, 0.0, z0), p(100.0, 0.0, z1), p(0.0, 0.0, z1)], [0.0, -1.0, 0.0]),
        (vec![p(100.0, 0.0, z0), p(100.0, 40.0, z0), p(100.0, 40.0, z1), p(100.0, 0.0, z1)], [1.0, 0.0, 0.0]),
        (vec![p(90.0, 50.0, z0), p(0.0, 50.0, z0), p(0.0, 50.0, z1), p(90.0, 50.0, z1)], [0.0, 1.0, 0.0]),
        (vec![p(0.0, 50.0, z0), p(0.0, 0.0, z0), p(0.0, 0.0, z1), p(0.0, 50.0, z1)], [-1.0, 0.0, 0.0]),
    ] {
        let d = sub(q[1], q[0]);
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
        let s = b.plane(q[0], n, d.map(|x| x / len));
        let l = b.poly(&q);
        b.face(s, Sense::Forward, vec![l]);
    }
    let cyl = b.sat.add_cone_surface([90.0, 40.0, 0.0], [0.0, 0.0, 1.0], [r, 0.0, 0.0], 1.0, 1.0, 0.0);
    let l = vec![(arc_b, false), b.line(p(90.0, 50.0, z0), p(90.0, 50.0, z1)), (arc_t, true), b.line(p(100.0, 40.0, z1), p(100.0, 40.0, z0))];
    b.face(cyl, Sense::Forward, vec![l]);
    b.finish()
}

/// Elliptic frustum (ratio 0.5) along Z, from z 0 (major axis r0) to z h (major axis r1); r0 == r1 = elliptic cylinder.
fn elliptic_frustum(x: f64, r0: f64, r1: f64, h: f64) -> SatDocument {
    let mut b = Brep::new();
    let e_bot = b.ellipse([x, 0.0, 0.0], [r0, 0.0, 0.0], 0.5);
    let e_top = b.ellipse([x, 0.0, h], [r1, 0.0, 0.0], 0.5);
    let top = b.plane([x, 0.0, h], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    b.face(top, Sense::Forward, vec![vec![(e_top, false)]]);
    let bot = b.plane([x, 0.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]);
    b.face(bot, Sense::Forward, vec![vec![(e_bot, true)]]);
    let slope = (r1 - r0) / h;
    let n = (1.0 + slope * slope).sqrt();
    let side = b.sat.add_cone_surface([x, 0.0, 0.0], [0.0, 0.0, 1.0], [r0, 0.0, 0.0], 0.5, 1.0 / n, slope / n);
    b.face(side, Sense::Forward, vec![vec![(e_bot, false)], vec![(e_top, true)]]);
    b.finish()
}

/// The primitive box with the top face replaced by a bilinear B-spline surface (exactsur) in the same plane.
fn box_with_spline_top() -> SatDocument {
    let mut sat = primitives::build_box([50.0, 25.0, 10.0], 100.0, 50.0, 20.0);
    let ctrl = [[-10.0, -10.0, 20.0], [110.0, -10.0, 20.0], [-10.0, 60.0, 20.0], [110.0, 60.0, 20.0]];
    let s = sat.add_spline_surface(false, false, 1, 1, false, false, &[(0.0, 2), (1.0, 2)], &[(0.0, 2), (1.0, 2)], &ctrl, None, 0.0);
    let top_plane = sat.records.iter().find(|r| r.entity_type == "plane-surface" && r.token_float(6) == Some(1.0) && r.token_float(3) == Some(20.0)).map(|r| r.index).expect("top plane");
    let face = sat.records.iter().position(|r| r.entity_type == "face" && matches!(r.tokens.get(5), Some(SatToken::Pointer(p)) if p.0 == top_plane)).expect("top face");
    sat.records[face].tokens[5] = SatToken::Pointer(ptr(s));
    sat
}

// ------------------------------------------------------------------ DWG / DXF files
enum Fmt {
    Dwg(DxfVersion),
    Dxf(DxfVersion),
}

fn write(name: &str, solids: Vec<(SatDocument, &str, i16)>, fmt: Fmt, sab: bool) -> PathBuf {
    let version = match fmt {
        Fmt::Dwg(v) | Fmt::Dxf(v) => v,
    };
    let mut doc = CadDocument::with_version(version);
    doc.header.insertion_units = 4; // mm
    for (sat, layer, color) in solids {
        if doc.layers.get(layer).is_none() {
            doc.layers.add(Layer::new(layer)).unwrap();
        }
        let mut s = Solid3D::new();
        s.acis_data = if sab { AcisData::from_sab(SabWriter::write(&sat)) } else { AcisData::from_sat(&sat.to_sat_string()) };
        s.common.layer = layer.to_string();
        s.common.color = Color::from_index(color);
        doc.add_entity(EntityType::Solid3D(s)).unwrap();
    }
    let dir = std::env::temp_dir().join(format!("mb_acis_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (bytes, ext) = match fmt {
        Fmt::Dwg(_) => (DwgWriter::write_to_vec(&doc).unwrap(), "dwg"),
        Fmt::Dxf(_) => (DxfWriter::new(&doc).write_to_vec().unwrap(), "dxf"),
    };
    let p = dir.join(format!("{name}.{ext}"));
    std::fs::write(&p, bytes).unwrap();
    p
}

/// OCCT has global state in the C++ library: concurrent loads from tests
/// may return another fixture's model. We serialize them (as in smoke.rs).
static ACIS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// None if ManageBIM was compiled without OCCT - the tests are then skipped.
fn load(p: &std::path::Path) -> Option<RawModel> {
    let _guard = ACIS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !occt::BUILT_WITH_OCCT {
        eprintln!("OpenCascade not built - skipping {}", p.display());
        return None;
    }
    let s = p.to_string_lossy();
    let m = if s.ends_with(".dwg") { formats::convert_dwg_native(&s, &|_, _| {}) } else { formats::load_dxf(&s, &|_, _| {}) };
    Some(m.unwrap_or_else(|e| panic!("{}: {e}", p.display())))
}

fn bbox(tris: &[[DVec3; 3]]) -> (DVec3, DVec3) {
    let mut mn = DVec3::splat(f64::INFINITY);
    let mut mx = -mn;
    for t in tris {
        for p in t {
            mn = mn.min(*p);
            mx = mx.max(*p);
        }
    }
    (mn, mx)
}

/// signed volume, in mm3 (positive = outward normals)
fn volume_mm3(e: &RawElement) -> f64 {
    e.tris.iter().map(|t| t[0].dot(t[1].cross(t[2])) / 6.0).sum::<f64>() * 1e9
}

fn solid<'a>(m: &'a RawModel, layer: &str) -> &'a RawElement {
    let v: Vec<&RawElement> = m.elements.iter().filter(|e| e.ifc_type == "3D Solid" && e.storey == layer).collect();
    assert_eq!(v.len(), 1, "one element per 3DSOLID on layer {layer}: {:?}", m.elements.iter().map(|e| (&e.name, &e.ifc_type, &e.storey)).collect::<Vec<_>>());
    v[0]
}

fn assert_box(e: &RawElement, mn: [f64; 3], mx: [f64; 3], vol: f64, rel: f64) {
    let (a, b) = bbox(&e.tris);
    for k in 0..3 {
        assert!((a[k] * 1000.0 - mn[k]).abs() < 0.05 && (b[k] * 1000.0 - mx[k]).abs() < 0.05, "{}: bbox {a:?} - {b:?} (m), want {mn:?} - {mx:?} (mm)", e.name);
    }
    let v = volume_mm3(e);
    assert!(v > 0.0, "{}: outward orientation (signed volume {v})", e.name);
    assert!((v - vol).abs() <= vol * rel, "{}: volume {v:.1} mm3, want {vol:.1}", e.name);
}

fn no_acis_warnings(m: &RawModel) {
    assert!(!m.warnings.iter().any(|w| w.contains("ACIS")), "unexpected ACIS warnings: {:?}", m.warnings);
}

// ------------------------------------------------------------------ tests
#[test]
fn dwg_box_and_cylinder_sat() {
    let p = write(
        "box_cyl",
        vec![(primitives::build_box([50.0, 25.0, 10.0], 100.0, 50.0, 20.0), "Steel", 1), (primitives::build_cylinder([200.0, 0.0, 0.0], 10.0, 40.0), "Pipes", 5)],
        Fmt::Dwg(DxfVersion::AC1015),
        false,
    );
    let Some(m) = load(&p) else { return };
    no_acis_warnings(&m);
    assert!(m.info.iter().any(|(k, v)| k == "ACIS solids" && v.starts_with("2 of 2")), "{:?}", m.info);

    let b = solid(&m, "Steel");
    assert_box(b, [0.0, 0.0, 0.0], [100.0, 50.0, 20.0], 100_000.0, 1e-6);
    // the entity color (ACI 1 = red) and the properties
    assert!(b.colors.iter().all(|c| c[0] > 200 && c[1] < 80), "entity colour: {:?}", b.colors.first());
    assert!(b.props.iter().any(|p| p.name == "Faces" && p.value == "6"));
    assert!(!b.guid.is_empty());

    let c = solid(&m, "Pipes");
    let (a, z) = bbox(&c.tris);
    let s = (z - a) * 1000.0;
    assert!((s.x - 20.0).abs() < 0.05 && (s.y - 20.0).abs() < 0.05 && (s.z - 40.0).abs() < 0.05, "cylinder 20 x 20 x 40 mm: {s:?}");
    assert!(((a.x + z.x) * 500.0 - 200.0).abs() < 0.05, "cylinder centred on x = 200 mm");
    let v = volume_mm3(c);
    assert!(v > 0.0 && (v - PI * 100.0 * 40.0).abs() < PI * 100.0 * 40.0 * 0.01, "cylinder volume {v}");
}

#[test]
fn dwg_boolean_subtraction_hole() {
    let p = write("hole", vec![(box_with_hole(), "Plate", 3)], Fmt::Dwg(DxfVersion::AC1018), false);
    let Some(m) = load(&p) else { return };
    no_acis_warnings(&m);
    let e = solid(&m, "Plate");
    assert_box(e, [0.0, 0.0, 0.0], [100.0, 100.0, 20.0], 200_000.0 - PI * 400.0 * 20.0, 0.005);
    // nothing inside the hole (r < 19.9 mm around the axis)
    let inside = e.tris.iter().flatten().any(|q| ((q.x * 1000.0 - 50.0).powi(2) + (q.y * 1000.0 - 50.0).powi(2)).sqrt() < 19.9);
    assert!(!inside, "triangles inside the hole");
}

#[test]
fn dwg_fillet() {
    let p = write("fillet", vec![(filleted_box(), "Fillet", 2)], Fmt::Dwg(DxfVersion::AC1024), false);
    let Some(m) = load(&p) else { return };
    no_acis_warnings(&m);
    let e = solid(&m, "Fillet");
    let want = 100_000.0 - (100.0 - PI * 25.0) * 20.0;
    assert_box(e, [0.0, 0.0, 0.0], [100.0, 50.0, 20.0], want, 0.002);
    // the corner (100, 50) is rounded: no point farther than 10 mm from the fillet center in that quadrant
    let corner = e.tris.iter().flatten().any(|q| q.x * 1000.0 > 90.0 && q.y * 1000.0 > 40.0 && ((q.x * 1000.0 - 90.0).powi(2) + (q.y * 1000.0 - 40.0).powi(2)).sqrt() > 10.05);
    assert!(!corner, "sharp corner left after the fillet");
}

#[test]
fn dwg_spline_face_and_sab() {
    // AC1032 (2018): binary ACIS (SAB)
    let p = write("spline_sab", vec![(box_with_spline_top(), "Spline", 4), (primitives::build_box([0.0, 0.0, 0.0], 10.0, 10.0, 10.0), "Sab", 6)], Fmt::Dwg(DxfVersion::AC1032), true);
    let Some(m) = load(&p) else { return };
    no_acis_warnings(&m);
    assert_box(solid(&m, "Spline"), [0.0, 0.0, 0.0], [100.0, 50.0, 20.0], 100_000.0, 1e-4);
    assert_box(solid(&m, "Sab"), [-5.0, -5.0, -5.0], [5.0, 5.0, 5.0], 1000.0, 1e-6);
}

#[test]
fn dwg_sphere_torus_cone() {
    let p = write(
        "curved",
        vec![
            (primitives::build_sphere([0.0, 0.0, 0.0], 10.0), "Sphere", 1),
            (primitives::build_torus([100.0, 0.0, 0.0], 20.0, 5.0), "Torus", 2),
            (primitives::build_cone([200.0, 0.0, 0.0], 10.0, 30.0), "Cone", 3),
        ],
        Fmt::Dwg(DxfVersion::AC1015),
        false,
    );
    let Some(m) = load(&p) else { return };
    no_acis_warnings(&m);
    let s = solid(&m, "Sphere");
    let (v, want) = (volume_mm3(s), 4.0 / 3.0 * PI * 1000.0);
    assert!(v > 0.0 && (v - want).abs() < want * 0.02, "sphere volume {v} / {want}");
    let t = solid(&m, "Torus");
    let (v, want) = (volume_mm3(t), 2.0 * PI * PI * 20.0 * 25.0);
    assert!(v > 0.0 && (v - want).abs() < want * 0.02, "torus volume {v} / {want}");
    let c = solid(&m, "Cone");
    let (v, want) = (volume_mm3(c), PI * 100.0 * 30.0 / 3.0);
    assert!(v > 0.0 && (v - want).abs() < want * 0.02, "cone volume {v} / {want}");
}

#[test]
fn dwg_elliptic_cylinder_and_cone() {
    let p = write("elliptic", vec![(elliptic_frustum(0.0, 20.0, 20.0, 30.0), "ECyl", 1), (elliptic_frustum(100.0, 20.0, 14.0, 30.0), "ECone", 2)], Fmt::Dwg(DxfVersion::AC1015), false);
    let Some(m) = load(&p) else { return };
    no_acis_warnings(&m);
    let want = PI * 20.0 * 10.0 * 30.0;
    assert_box(solid(&m, "ECyl"), [-20.0, -10.0, 0.0], [20.0, 10.0, 30.0], want, 0.01);
    // elliptic frustum = 0.5 x the circular frustum
    let want = 0.5 * PI * 30.0 / 3.0 * (400.0 + 280.0 + 196.0);
    assert_box(solid(&m, "ECone"), [80.0, -10.0, 0.0], [120.0, 10.0, 30.0], want, 0.01);
}

#[test]
fn dxf_file_with_acis_and_body_transform() {
    // body with an ACIS transform: 90 degree rotation around Z + translation (1000, 0, 0)
    let mut sat = primitives::build_box([50.0, 25.0, 10.0], 100.0, 50.0, 20.0);
    let t = sat.add_transform([[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]], [1000.0, 0.0, 0.0], 1.0);
    sat.records[0].tokens[3] = SatToken::Pointer(ptr(t));
    let p = write("moved", vec![(sat, "Moved", 1), (filleted_box(), "Fillet", 2)], Fmt::Dxf(DxfVersion::AC1015), false);
    let Some(m) = load(&p) else { return };
    no_acis_warnings(&m);
    // (x, y) -> (-y, x) + (1000, 0): x in [950, 1000], y in [0, 100]
    assert_box(solid(&m, "Moved"), [950.0, 0.0, 0.0], [1000.0, 100.0, 20.0], 100_000.0, 1e-6);
    assert_box(solid(&m, "Fillet"), [0.0, 0.0, 0.0], [100.0, 50.0, 20.0], 100_000.0 - (100.0 - PI * 25.0) * 20.0, 0.002);
}

#[test]
fn untranslatable_solid_is_reported_not_fatal() {
    // empty ACIS body (no faces) next to a valid solid: only the empty one is reported
    let p = write("empty", vec![(SatDocument::new_body(), "Empty", 1), (primitives::build_box([0.0, 0.0, 0.0], 10.0, 10.0, 10.0), "Ok", 2)], Fmt::Dwg(DxfVersion::AC1015), false);
    let Some(m) = load(&p) else { return };
    // With native OCCT the element may get another name / layer; what matters is that the valid solid exists.
    let solids: Vec<&bimrust::model::RawElement> = m.elements.iter().filter(|e| e.ifc_type == "3D Solid").collect();
    assert_eq!(solids.len(), 1, "{:?}", m.elements.iter().map(|e| (&e.name, &e.ifc_type, &e.storey)).collect::<Vec<_>>());
    assert!(!solids[0].tris.is_empty(), "the valid solid must have triangles");
    let w: Vec<&String> = m.warnings.iter().filter(|w| w.contains("ACIS")).collect();
    assert_eq!(w.len(), 1, "{:?}", m.warnings);
    assert!(w[0].starts_with("1 × 3DSOLID"), "{w:?}");
}
