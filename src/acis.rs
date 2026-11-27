//! ACIS solids from DWG / DXF (3DSOLID, REGION, BODY, surfaces) -> triangles, without ODA / Navisworks.
//!
//! acadrust (MPL-2.0) parses the ACIS data (text SAT and binary SAB) into typed records. Here
//! we walk the topology (face -> loop -> coedge -> edge -> vertex) and the geometry (planar surfaces,
//! cone / cylinder, sphere, torus, B-spline; line, ellipse, B-spline curves) and send it as a neutral
//! B-rep description to the OpenCascade shim (`mbo_brep_from_desc` in src/occt/mbocct.cpp), which builds
//! the faces, repairs them (ShapeFix: pcurves, seams, tolerances, orientation) and triangulates them.
//! The description format is documented in mbocct.cpp.
use crate::occt::{self, MeshQuality};
use acadrust::entities::acis::{
    SatConeSurface, SatCoedge, SatDocument, SatEdge, SatEllipseCurve, SatFace, SatIntCurve, SatLoop, SatPlaneSurface, SatPoint, SatPointer, SatRecord, SatSphereSurface,
    SatSplineSurface, SatStraightCurve, SatTorusSurface, SatVertex, Sense,
};
use acadrust::entities::solid3d::AcisData;
use acadrust::{CadDocument, EntityType};
use glam::{DMat4, DVec3, DVec4};
use std::collections::{BTreeMap, HashMap};

/// A triangulated ACIS solid (drawing coordinates, before unit scaling).
#[derive(Clone, Debug, Default)]
pub struct SolidMesh {
    pub tris: Vec<[DVec3; 3]>,
    /// ACIS faces in total / not translated (unsupported geometry or failure in construction / triangulation)
    pub faces: usize,
    pub faces_failed: usize,
    /// closed shells turned into solids (volume oriented outward)
    pub solids: usize,
    /// untranslated ACIS geometries (type -> count), for the report
    pub unsupported: BTreeMap<String, usize>,
    /// translated ACIS geometries (type -> count), for the report
    pub translated: BTreeMap<String, usize>,
}

/// All ACIS solids of a drawing, indexed by the entity handle (hex, uppercase - as in DXF).
#[derive(Debug, Default)]
pub struct AcisMeshes {
    pub meshes: HashMap<String, SolidMesh>,
    /// ACIS entities with data
    pub total: usize,
    /// reason -> number of untranslated entities
    pub failed: BTreeMap<String, usize>,
    /// entities translated only partially (some faces are missing)
    pub partial: usize,
    pub faces_failed: usize,
    pub unsupported: BTreeMap<String, usize>,
    pub translated: BTreeMap<String, usize>,
}

impl AcisMeshes {
    pub fn translated_count(&self) -> usize {
        self.meshes.len()
    }
    pub fn failed_count(&self) -> usize {
        self.failed.values().sum()
    }
    /// "reason (n), reason (n)"
    pub fn failure_summary(&self) -> String {
        self.failed.iter().map(|(k, v)| format!("{k} ({v})")).collect::<Vec<_>>().join(", ")
    }
}

/// The DXF name of the ACIS-based entities (the DXF loader recognises them).
pub const DXF_TYPES: [&str; 10] = ["3DSOLID", "BODY", "REGION", "SURFACE", "PLANESURFACE", "EXTRUDEDSURFACE", "LOFTEDSURFACE", "REVOLVEDSURFACE", "SWEPTSURFACE", "NURBSURFACE"];

/// The entity handle, in the DXF form (code 5).
pub fn handle_key(e: &EntityType) -> String {
    format!("{:X}", e.common().handle.value())
}

fn acis_of(e: &EntityType) -> Option<&AcisData> {
    match e {
        EntityType::Solid3D(s) => Some(&s.acis_data),
        EntityType::Region(s) => Some(&s.acis_data),
        EntityType::Body(s) => Some(&s.acis_data),
        EntityType::Surface(s) => Some(&s.acis_data),
        _ => None,
    }
}

/// Triangulates all ACIS solids in the document (including those in blocks), in parallel.
pub fn mesh_document(doc: &CadDocument, progress: &dyn Fn(f32, &str)) -> AcisMeshes {
    use rayon::prelude::*;
    let jobs: Vec<(String, &AcisData)> = doc.entities().filter_map(|e| acis_of(e).filter(|a| a.has_data()).map(|a| (handle_key(e), a))).collect();
    let mut out = AcisMeshes { total: jobs.len(), ..Default::default() };
    if jobs.is_empty() {
        return out;
    }
    let q = MeshQuality::default();
    let n = jobs.len();
    // all solids at once on all cores (the batches waited for the slowest solid); the progress from the current thread
    let done = std::sync::atomic::AtomicUsize::new(0);
    let all: Vec<(String, Result<SolidMesh, String>)> = std::thread::scope(|s| {
        let h = s.spawn(|| {
            jobs.par_iter()
                .map(|(h, a)| {
                    let r = (h.clone(), mesh_acis(a, q));
                    done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    r
                })
                .collect::<Vec<_>>()
        });
        while !h.is_finished() {
            let d = done.load(std::sync::atomic::Ordering::Relaxed);
            progress(d as f32 / n as f32, &format!("Translating ACIS solids ({d} / {n})"));
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        h.join().unwrap_or_default()
    });
    for (h, r) in all {
        match r {
            Ok(m) => {
                if m.faces_failed > 0 {
                    out.partial += 1;
                    out.faces_failed += m.faces_failed;
                }
                for (k, v) in &m.unsupported {
                    *out.unsupported.entry(k.clone()).or_default() += v;
                }
                for (k, v) in &m.translated {
                    *out.translated.entry(k.clone()).or_default() += v;
                }
                out.meshes.insert(h, m);
            }
            Err(e) => *out.failed.entry(e).or_default() += 1,
        }
    }
    progress(1.0, "ACIS solids translated");
    out
}

/// The ACIS data of an entity (SAT or SAB) -> triangles.
pub fn mesh_acis(data: &AcisData, q: MeshQuality) -> Result<SolidMesh, String> {
    let doc = data.parse().ok_or_else(|| "ACIS data could not be parsed".to_string())?;
    mesh_sat(&doc, q)
}

/// A SAT document -> triangles (through OpenCascade).
pub fn mesh_sat(doc: &SatDocument, q: MeshQuality) -> Result<SolidMesh, String> {
    let mut d = Describer::new(doc);
    d.run();
    if d.nf == 0 {
        return Err("no faces in the ACIS body".into());
    }
    let (unsupported, translated) = (std::mem::take(&mut d.unsupported), std::mem::take(&mut d.translated));
    let (ints, reals) = d.finish();
    let b = occt::brep_from_desc(&ints, &reals, q).map_err(|e| if e.contains("not found") || e.contains("Cannot load") { "OpenCascade runtime not available".to_string() } else { e })?;
    let xf = placement(doc);
    let mut m = SolidMesh { faces: b.faces, faces_failed: b.faces_failed, solids: b.solids, unsupported, translated, tris: Vec::new() };
    if b.curve_fallbacks > 0 {
        *m.unsupported.entry("edge curve (approximated by a line)".into()).or_default() += b.curve_fallbacks;
    }
    let flip = xf.determinant() < 0.0;
    m.tris = b
        .tris
        .into_iter()
        .map(|t| {
            let t = t.map(|p| xf.transform_point3(p));
            if flip {
                [t[0], t[2], t[1]]
            } else {
                t
            }
        })
        .collect();
    if m.tris.is_empty() {
        let why = if m.unsupported.is_empty() { "the faces could not be built".to_string() } else { format!("unsupported geometry: {}", m.unsupported.keys().cloned().collect::<Vec<_>>().join(", ")) };
        return Err(why);
    }
    Ok(m)
}

/// The body transform: world = scale * (p * M) + T (the SAT convention, row vector).
fn placement(doc: &SatDocument) -> DMat4 {
    let (m, t, s) = doc.placement();
    let s = if s.is_finite() && s.abs() > 1e-12 { s } else { 1.0 };
    let col = |r: [f64; 3]| DVec4::new(r[0] * s, r[1] * s, r[2] * s, 0.0);
    let x = DMat4::from_cols(col(m[0]), col(m[1]), col(m[2]), DVec4::new(t[0], t[1], t[2], 1.0));
    if x.is_finite() && x.determinant().abs() > 1e-18 {
        x
    } else {
        DMat4::IDENTITY
    }
}

fn v3(t: (f64, f64, f64)) -> [f64; 3] {
    [t.0, t.1, t.2]
}

fn finite(v: &[f64]) -> bool {
    v.iter().all(|x| x.is_finite())
}

/// The ACIS topology + geometry -> the integer / real streams read by mbo_brep_from_desc.
struct Describer<'a> {
    doc: &'a SatDocument,
    vr: Vec<f64>,
    nv: i32,
    vmap: HashMap<i32, Option<i32>>,
    ci: Vec<i32>,
    cr: Vec<f64>,
    nc: i32,
    cmap: HashMap<i32, i32>,
    ei: Vec<i32>,
    ne: i32,
    emap: HashMap<i32, Option<i32>>,
    si: Vec<i32>,
    sr: Vec<f64>,
    ns: i32,
    smap: HashMap<i32, i32>,
    fi: Vec<i32>,
    nf: i32,
    shells: HashMap<i32, i32>,
    unsupported: BTreeMap<String, usize>,
    translated: BTreeMap<String, usize>,
}

impl<'a> Describer<'a> {
    fn new(doc: &'a SatDocument) -> Self {
        Describer {
            doc,
            vr: Vec::new(),
            nv: 0,
            vmap: HashMap::new(),
            ci: Vec::new(),
            cr: Vec::new(),
            nc: 0,
            cmap: HashMap::new(),
            ei: Vec::new(),
            ne: 0,
            emap: HashMap::new(),
            si: Vec::new(),
            sr: Vec::new(),
            ns: 0,
            smap: HashMap::new(),
            fi: Vec::new(),
            nf: 0,
            shells: HashMap::new(),
            unsupported: BTreeMap::new(),
            translated: BTreeMap::new(),
        }
    }

    fn rec(&self, p: SatPointer) -> Option<&'a SatRecord> {
        self.doc.resolve(p)
    }

    fn ok(&mut self, what: &str) {
        *self.translated.entry(what.to_string()).or_default() += 1;
    }

    fn missing(&mut self, what: String) {
        *self.unsupported.entry(what).or_default() += 1;
    }

    fn run(&mut self) {
        for r in &self.doc.records {
            if let Some(f) = SatFace::from_record(r) {
                self.face(&f);
            }
        }
    }

    fn finish(self) -> (Vec<i32>, Vec<f64>) {
        let mut ints = vec![1, self.nv, self.nc, self.ne, self.ns, self.nf];
        ints.extend(self.ci);
        ints.extend(self.ei);
        ints.extend(self.si);
        ints.extend(self.fi);
        let res = self.doc.header.spatial_resolution;
        let tol = if res.is_finite() && (1e-9..=1e-2).contains(&res) { res } else { 1e-6 };
        let mut reals = vec![tol];
        reals.extend(self.vr);
        reals.extend(self.cr);
        reals.extend(self.sr);
        (ints, reals)
    }

    fn vertex(&mut self, p: SatPointer) -> Option<i32> {
        if let Some(v) = self.vmap.get(&p.0) {
            return *v;
        }
        let pos = self.rec(p).and_then(SatVertex::from_record).and_then(|v| self.rec(v.point())).and_then(SatPoint::from_record).map(|pt| v3(pt.position())).filter(|x| finite(x));
        let idx = pos.map(|x| {
            self.vr.extend_from_slice(&x);
            self.nv += 1;
            self.nv - 1
        });
        self.vmap.insert(p.0, idx);
        idx
    }

    /// -1 = no curve (degenerate edge or segment between vertices)
    fn curve(&mut self, p: SatPointer) -> i32 {
        if p.is_null() {
            return -1;
        }
        if let Some(c) = self.cmap.get(&p.0) {
            return *c;
        }
        let Some(r) = self.rec(p) else { return -1 };
        if let Some(c) = SatStraightCurve::from_record(r) {
            let (o, d) = (v3(c.root_point()), v3(c.direction()));
            if finite(&o) && finite(&d) {
                self.ci.push(0);
                self.cr.extend_from_slice(&o);
                self.cr.extend_from_slice(&d);
                self.ok("curve: straight");
            } else {
                self.ci.push(99);
            }
        } else if let Some(c) = SatEllipseCurve::from_record(r) {
            let (o, n, m, k) = (v3(c.center()), v3(c.normal()), v3(c.major_axis()), c.ratio());
            if finite(&o) && finite(&n) && finite(&m) && k.is_finite() {
                self.ci.push(1);
                self.cr.extend_from_slice(&o);
                self.cr.extend_from_slice(&n);
                self.cr.extend_from_slice(&m);
                self.cr.push(k);
                self.ok(if (k.abs() - 1.0).abs() < 1e-9 { "curve: circle" } else { "curve: ellipse" });
            } else {
                self.ci.push(99);
            }
        } else if let Some((deg, knots, ctrl)) = SatIntCurve::from_record(r).and_then(|c| c.bspline_in(self.doc)).filter(|(deg, k, c)| *deg >= 1 && k.len() == c.len() + deg + 1 && finite(k) && c.iter().all(|p| finite(p) && p[3] > 0.0)) {
            let rational = ctrl.iter().any(|p| (p[3] - 1.0).abs() > 1e-12);
            self.ci.extend_from_slice(&[2, deg as i32, knots.len() as i32, ctrl.len() as i32, rational as i32]);
            self.cr.extend_from_slice(&knots);
            for p in &ctrl {
                self.cr.extend_from_slice(&[p[0] / p[3], p[1] / p[3], p[2] / p[3], p[3]]);
            }
            self.ok("curve: B-spline (intcurve / bs3)");
        } else {
            self.ci.push(99);
            self.missing(format!("curve: {}", r.entity_type));
        }
        self.nc += 1;
        let idx = self.nc - 1;
        self.cmap.insert(p.0, idx);
        idx
    }

    fn edge(&mut self, p: SatPointer) -> Option<i32> {
        if let Some(e) = self.emap.get(&p.0) {
            return *e;
        }
        let idx = (|| {
            let e = SatEdge::from_record(self.rec(p)?)?;
            let (sv, ev) = (self.vertex(e.start_vertex())?, self.vertex(e.end_vertex())?);
            let c = self.curve(e.curve());
            self.ei.extend_from_slice(&[sv, ev, c, (e.sense() == Sense::Reversed) as i32]);
            self.ne += 1;
            Some(self.ne - 1)
        })();
        self.emap.insert(p.0, idx);
        idx
    }

    /// -1 = missing / unsupported surface
    fn surface(&mut self, p: SatPointer) -> i32 {
        if let Some(s) = self.smap.get(&p.0) {
            return *s;
        }
        let Some(r) = self.rec(p) else { return -1 };
        let start = (self.si.len(), self.sr.len());
        let ok = if let Some(s) = SatPlaneSurface::from_record(r) {
            self.si.extend_from_slice(&[0, 0]);
            self.sr.extend_from_slice(&v3(s.root_point()));
            self.sr.extend_from_slice(&v3(s.normal()));
            self.sr.extend_from_slice(&v3(s.u_direction()));
            Some("surface: plane")
        } else if let Some(s) = SatConeSurface::from_record(r) {
            let (ratio, sin, cos) = (s.ratio(), s.sin_half_angle(), s.cos_half_angle());
            let elliptic = (ratio.abs() - 1.0).abs() > 1e-9;
            self.si.extend_from_slice(&[1, 0]);
            self.sr.extend_from_slice(&v3(s.center()));
            self.sr.extend_from_slice(&v3(s.axis()));
            self.sr.extend_from_slice(&v3(s.major_axis()));
            self.sr.extend_from_slice(&[ratio, sin, cos]);
            // elliptic: the cylinder = the extrusion of the ellipse, the cone = an affinely scaled circular cone (B-spline, in the shim)
            Some(match (sin.abs() < 1e-12, elliptic) {
                (true, false) => "surface: cylinder",
                (true, true) => "surface: elliptic cylinder",
                (false, false) => "surface: cone",
                (false, true) => "surface: elliptic cone",
            })
        } else if let Some(s) = SatSphereSurface::from_record(r) {
            self.si.extend_from_slice(&[2, 0]);
            self.sr.extend_from_slice(&v3(s.center()));
            self.sr.push(s.radius());
            self.sr.extend_from_slice(&v3(s.u_direction()));
            self.sr.extend_from_slice(&v3(s.pole()));
            Some("surface: sphere")
        } else if let Some(s) = SatTorusSurface::from_record(r) {
            if s.major_radius() <= 0.0 {
                None // "lemon" torus (negative major radius)
            } else {
                self.si.extend_from_slice(&[3, 0]);
                self.sr.extend_from_slice(&v3(s.center()));
                self.sr.extend_from_slice(&v3(s.normal()));
                self.sr.extend_from_slice(&[s.major_radius(), s.minor_radius()]);
                self.sr.extend_from_slice(&v3(s.u_direction()));
                Some("surface: torus")
            }
        } else if let Some(s) = SatSplineSurface::from_record(r) {
            match s.bspline(self.doc).filter(|b| {
                b.degree_u >= 1
                    && b.degree_v >= 1
                    && b.control_count_u >= 2
                    && b.control_count_v >= 2
                    && b.u_knots.len() == b.control_count_u + b.degree_u + 1
                    && b.v_knots.len() == b.control_count_v + b.degree_v + 1
                    && b.control_points.len() == b.control_count_u * b.control_count_v
                    && finite(&b.u_knots)
                    && finite(&b.v_knots)
                    && b.control_points.iter().all(|p| finite(p) && p[3] > 0.0)
            }) {
                Some(b) => {
                    let flip = s.sense() == Sense::Reversed;
                    self.si.extend_from_slice(&[
                        4,
                        flip as i32,
                        b.degree_u as i32,
                        b.degree_v as i32,
                        b.u_knots.len() as i32,
                        b.v_knots.len() as i32,
                        b.control_count_u as i32,
                        b.control_count_v as i32,
                        b.rational as i32,
                    ]);
                    self.sr.extend_from_slice(&b.u_knots);
                    self.sr.extend_from_slice(&b.v_knots);
                    for p in &b.control_points {
                        self.sr.extend_from_slice(&[p[0] / p[3], p[1] / p[3], p[2] / p[3], p[3]]);
                    }
                    Some("surface: B-spline")
                }
                None => None,
            }
        } else {
            None
        };
        let idx = match ok {
            Some(what) if finite(&self.sr[start.1..]) => {
                self.ok(what);
                self.ns += 1;
                self.ns - 1
            }
            _ => {
                self.si.truncate(start.0);
                self.sr.truncate(start.1);
                let name = match SatSplineSurface::from_record(r) {
                    Some(s) => format!("surface: {} ({})", r.entity_type, spline_kind(&s, self.doc)),
                    None => format!("surface: {}", r.entity_type),
                };
                self.missing(name);
                -1
            }
        };
        self.smap.insert(p.0, idx);
        idx
    }

    fn face(&mut self, f: &SatFace<'a>) {
        let surf = self.surface(f.surface());
        let shell_ptr = f.shell().0;
        let next_shell = self.shells.len() as i32;
        let shell = *self.shells.entry(shell_ptr).or_insert(next_shell);
        let mut loops: Vec<Vec<(i32, i32)>> = Vec::new();
        let mut lp = f.first_loop();
        let mut guard = 0;
        while let Some(l) = self.rec(lp).and_then(SatLoop::from_record) {
            guard += 1;
            if guard > 100_000 {
                break;
            }
            let mut ces = Vec::new();
            let first = l.first_coedge();
            let mut cp = first;
            let mut n = 0;
            while let Some(ce) = self.rec(cp).and_then(SatCoedge::from_record) {
                let e = self.edge(ce.edge()).unwrap_or(-1);
                ces.push((e, (ce.sense() == Sense::Reversed) as i32));
                n += 1;
                cp = ce.next();
                if cp.is_null() || cp.0 == first.0 || n > 1_000_000 {
                    break;
                }
            }
            loops.push(ces);
            lp = l.next_loop();
        }
        self.fi.extend_from_slice(&[surf, (f.sense() == Sense::Reversed) as i32, shell, loops.len() as i32]);
        for l in loops {
            self.fi.push(l.len() as i32);
            for (e, r) in l {
                self.fi.extend_from_slice(&[e, r]);
            }
        }
        self.nf += 1;
    }
}

/// The subtype of an undecodable spline surface (e.g. "rb_blend_spl_sur") - for the report.
fn spline_kind(s: &SatSplineSurface, doc: &SatDocument) -> String {
    s.definition_tokens(doc)
        .and_then(|t| t.iter().filter_map(|x| x.as_ident()).find(|x| !matches!(*x, "{" | "}" | "forward" | "reversed" | "forward_v" | "reversed_v" | "ref")))
        .unwrap_or("?")
        .to_string()
}
