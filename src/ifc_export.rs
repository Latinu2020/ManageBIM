//! Export IFC4 (STEP) din elementele din proiect: geometria (IfcTriangulatedFaceSet), culoarea,
//! nivelul (IfcBuildingStorey), GlobalId-ul original si TOATE proprietatile (un IfcPropertySet pe grup).
//! Elementele pastreaza tipul IFC cand e o clasa IFC4 cunoscuta, altfel IfcBuildingElementProxy
//! (cu tipul original in ObjectType). Coordonatele: reale (IfcSite asezat in originea scenei).
use crate::model::{Element, Model};
use glam::DVec3;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

/// Text STEP: apostrof dublat, backslash dublat, caracterele non-ASCII ca \X2\hhhh\X0\.
pub fn step_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('\'');
    let mut wide = String::new();
    let flush = |o: &mut String, wide: &mut String| {
        if !wide.is_empty() {
            o.push_str("\\X2\\");
            o.push_str(wide);
            o.push_str("\\X0\\");
            wide.clear();
        }
    };
    for c in s.chars() {
        if c.is_ascii() && !c.is_ascii_control() {
            flush(&mut o, &mut wide);
            match c {
                '\'' => o.push_str("''"),
                '\\' => o.push_str("\\\\"),
                _ => o.push(c),
            }
        } else if c.is_ascii_control() {
            flush(&mut o, &mut wide);
            o.push(' ');
        } else {
            let mut buf = [0u16; 2];
            for u in c.encode_utf16(&mut buf) {
                let _ = write!(wide, "{:04X}", u);
            }
        }
    }
    flush(&mut o, &mut wide);
    o.push('\'');
    o
}

const B64: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

/// GlobalId IFC (22 caractere) dintr-un text oarecare (determinist: acelasi text -> acelasi GUID).
pub fn guid_from(seed: &str) -> String {
    // doua hash-uri FNV-1a pe 64 biti -> 128 biti
    let h = |salt: u64| {
        let mut x = 0xcbf2_9ce4_8422_2325u64 ^ salt;
        for b in seed.bytes() {
            x ^= b as u64;
            x = x.wrapping_mul(0x0000_0100_0000_01b3);
        }
        x
    };
    let n = ((h(0) as u128) << 64) | h(0x9e37_79b9_7f4a_7c15) as u128;
    let mut s = String::with_capacity(22);
    // primul caracter: 2 biti, restul cate 6 biti
    s.push(B64[((n >> 126) & 0x3) as usize] as char);
    for k in (0..21).rev() {
        s.push(B64[((n >> (k * 6)) & 0x3f) as usize] as char);
    }
    s
}

fn is_ifc_guid(g: &str) -> bool {
    g.len() == 22 && g.bytes().all(|b| B64.contains(&b))
}

/// Clase IFC4 acceptate direct + cate atribute au dupa `Representation` (le scriem `$`).
fn ifc_class(t: &str) -> Option<(&'static str, usize)> {
    const NINE: &[&str] = &[
        "IfcWall", "IfcWallStandardCase", "IfcSlab", "IfcBeam", "IfcColumn", "IfcMember", "IfcPlate", "IfcRoof", "IfcStair", "IfcRamp", "IfcRampFlight", "IfcRailing", "IfcCovering",
        "IfcFooting", "IfcCurtainWall", "IfcChimney", "IfcShadingDevice", "IfcBuildingElementProxy", "IfcOpeningElement", "IfcFurniture", "IfcPipeSegment", "IfcPipeFitting", "IfcDuctSegment",
        "IfcDuctFitting", "IfcCableCarrierSegment", "IfcCableCarrierFitting", "IfcCableSegment", "IfcValve", "IfcPump", "IfcFan", "IfcAirTerminal", "IfcLightFixture", "IfcSanitaryTerminal",
        "IfcFlowMeter", "IfcTank", "IfcElectricAppliance", "IfcOutlet", "IfcDamper", "IfcBoiler",
    ];
    const EIGHT: &[&str] = &["IfcFlowSegment", "IfcFlowFitting", "IfcFlowTerminal", "IfcFlowController", "IfcFlowMovingDevice", "IfcFlowStorageDevice", "IfcFurnishingElement", "IfcDistributionElement"];
    let low = t.trim().to_lowercase();
    if let Some(c) = NINE.iter().find(|c| c.to_lowercase() == low) {
        return Some((c, 2));
    }
    if let Some(c) = EIGHT.iter().find(|c| c.to_lowercase() == low) {
        return Some((c, 1));
    }
    match low.as_str() {
        "ifcdoor" => Some(("IfcDoor", 6)),
        "ifcwindow" => Some(("IfcWindow", 6)),
        "ifcspace" => Some(("IfcSpace", 4)),
        _ => None,
    }
}

struct W {
    s: String,
    n: u64,
}

impl W {
    fn add(&mut self, body: &str) -> u64 {
        self.n += 1;
        let _ = writeln!(self.s, "#{}={};", self.n, body);
        self.n
    }
}

fn f(v: f64) -> String {
    // reale STEP: intotdeauna cu punct zecimal
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0');
    if s.ends_with('.') { format!("{s}0") } else { s.to_string() }
}

pub struct ExportStats {
    pub elements: usize,
    pub triangles: usize,
    pub properties: usize,
    /// suprafete inchise (solide etanse) si elemente scrise ca instante ale unei geometrii deja scrise
    pub closed: usize,
    pub instances: usize,
}

/// Scrie fisierul IFC. `items`: (model, element) de exportat; `origin`: coordonatele reale ale originii scenei.
pub fn write_ifc(items: &[(&Model, &Element)], origin: DVec3, project: &str, now: &str) -> (String, ExportStats) {
    write_ifc_mvd(items, origin, project, now, crate::ifc_mvd::Mvd::Reference)
}

/// Scrie fisierul IFC conform unui Model View Definition.
pub fn write_ifc_mvd(items: &[(&Model, &Element)], origin: DVec3, project: &str, now: &str, mvd: crate::ifc_mvd::Mvd) -> (String, ExportStats) {
    write_ifc_full(items, origin, project, now, mvd, &[])
}

/// Export cu mapare categorie -> clasa IFC.
pub fn write_ifc_full(items: &[(&Model, &Element)], origin: DVec3, project: &str, now: &str, mvd: crate::ifc_mvd::Mvd, map: &[crate::ifc_map::MapRule]) -> (String, ExportStats) {
    let mut w = W { s: String::new(), n: 0 };
    let mut st = ExportStats { elements: 0, triangles: 0, properties: 0, closed: 0, instances: 0 };
    let person = w.add("IFCPERSON($,$,'',$,$,$,$,$)");
    let org = w.add("IFCORGANIZATION($,'ManageBIM',$,$,$)");
    let po = w.add(&format!("IFCPERSONANDORGANIZATION(#{person},#{org},$)"));
    let app = w.add(&format!("IFCAPPLICATION(#{org},{},'ManageBIM','ManageBIM')", step_str(env!("CARGO_PKG_VERSION"))));
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let oh = w.add(&format!("IFCOWNERHISTORY(#{po},#{app},$,.ADDED.,{ts},#{po},#{app},{ts})"));
    let o3 = w.add("IFCCARTESIANPOINT((0.,0.,0.))");
    let zdir = w.add("IFCDIRECTION((0.,0.,1.))");
    let xdir = w.add("IFCDIRECTION((1.,0.,0.))");
    let ax = w.add(&format!("IFCAXIS2PLACEMENT3D(#{o3},#{zdir},#{xdir})"));
    let ctx = w.add(&format!("IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-05,#{ax},$)"));
    let body_ctx = w.add(&format!("IFCGEOMETRICREPRESENTATIONSUBCONTEXT('Body','Model',*,*,*,*,#{ctx},$,.MODEL_VIEW.,$)"));
    let u1 = w.add("IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.)");
    let u2 = w.add("IFCSIUNIT(*,.AREAUNIT.,$,.SQUARE_METRE.)");
    let u3 = w.add("IFCSIUNIT(*,.VOLUMEUNIT.,$,.CUBIC_METRE.)");
    let u4 = w.add("IFCSIUNIT(*,.PLANEANGLEUNIT.,$,.RADIAN.)");
    let units = w.add(&format!("IFCUNITASSIGNMENT((#{u1},#{u2},#{u3},#{u4}))"));
    let proj = w.add(&format!("IFCPROJECT({},#{oh},{},$,$,$,$,(#{ctx}),#{units})", step_str(&guid_from(&format!("project|{project}|{now}"))), step_str(project)));

    // IfcSite in originea scenei (coordonatele reale mari raman in plasarea site-ului, geometria e locala)
    let so = w.add(&format!("IFCCARTESIANPOINT(({},{},{}))", f(origin.x), f(origin.y), f(origin.z)));
    let sax = w.add(&format!("IFCAXIS2PLACEMENT3D(#{so},#{zdir},#{xdir})"));
    let site_pl = w.add(&format!("IFCLOCALPLACEMENT($,#{sax})"));
    let site = w.add(&format!("IFCSITE({},#{oh},'Site',$,$,#{site_pl},$,$,.ELEMENT.,$,$,$,$,$)", step_str(&guid_from(&format!("site|{project}|{now}")))));
    let bpl = w.add(&format!("IFCLOCALPLACEMENT(#{site_pl},#{ax})"));
    let bld = w.add(&format!("IFCBUILDING({},#{oh},{},$,$,#{bpl},$,$,.ELEMENT.,$,$,$)", step_str(&guid_from(&format!("building|{project}|{now}"))), step_str(project)));
    w.add(&format!("IFCRELAGGREGATES({},#{oh},$,$,#{proj},(#{site}))", step_str(&guid_from(&format!("agg1|{now}")))));
    w.add(&format!("IFCRELAGGREGATES({},#{oh},$,$,#{site},(#{bld}))", step_str(&guid_from(&format!("agg2|{now}")))));

    // niveluri: dupa numele nivelului, cota = minimul Z al elementelor lui
    let mut levels: BTreeMap<String, f64> = BTreeMap::new();
    for (m, e) in items {
        let _ = m;
        let z = if e.aabb.is_valid() { e.aabb.min.z as f64 } else { 0.0 };
        let k = if e.storey.trim().is_empty() { "(no level)".to_string() } else { e.storey.clone() };
        let v = levels.entry(k).or_insert(f64::INFINITY);
        *v = v.min(z);
    }
    let mut storeys: HashMap<String, (u64, u64)> = HashMap::new();
    let mut st_ids = Vec::new();
    for (name, z) in &levels {
        let z = if z.is_finite() { *z } else { 0.0 };
        let p = w.add(&format!("IFCLOCALPLACEMENT(#{bpl},#{ax})"));
        let s = w.add(&format!("IFCBUILDINGSTOREY({},#{oh},{},$,$,#{p},$,$,.ELEMENT.,{})", step_str(&guid_from(&format!("storey|{name}|{now}"))), step_str(name), f(z + origin.z)));
        storeys.insert(name.clone(), (s, p));
        st_ids.push(s);
    }
    if !st_ids.is_empty() {
        let list = st_ids.iter().map(|i| format!("#{i}")).collect::<Vec<_>>().join(",");
        w.add(&format!("IFCRELAGGREGATES({},#{oh},$,$,#{bld},({list}))", step_str(&guid_from(&format!("agg3|{now}")))));
    }

    let mut contained: HashMap<u64, Vec<u64>> = HashMap::new();
    let mut styles: HashMap<[u8; 4], u64> = HashMap::new();
    let mut rep_maps: HashMap<u64, u64> = HashMap::new();
    let mut used_guids: std::collections::HashSet<String> = std::collections::HashSet::new();
    // clasificari -> IfcClassificationReference: (sistem, cod, nume, URI) -> elementele
    let mut class_refs: BTreeMap<(String, String, String, String), Vec<u64>> = BTreeMap::new();
    for (k, (m, e)) in items.iter().enumerate() {
        let tris = m.tris(e);
        if tris.len() < 3 {
            continue;
        }
        // geometria fata de coltul elementului (coordonate mici = precizie buna), pe culori: o suprafata pe culoare
        let base = tris.iter().fold(glam::Vec3::splat(f32::INFINITY), |a, v| a.min(*v));
        let cols = &m.tri_color[e.tri_start as usize..(e.tri_start + e.tri_count) as usize];
        let mut by_col: BTreeMap<[u8; 4], Vec<usize>> = BTreeMap::new();
        for (ti, c) in cols.iter().enumerate().take(tris.len() / 3) {
            by_col.entry(*c).or_default().push(ti);
        }
        // fiecare grup de culoare: varfuri sudate (toleranta 0.1 mm), triunghiuri degenerate scoase, inchis / deschis
        let mut parts: Vec<([u8; 4], Vec<[f32; 3]>, Vec<[usize; 3]>, bool)> = Vec::new();
        for (c, list) in &by_col {
            let mut map: HashMap<[i64; 3], usize> = HashMap::new();
            let mut pts: Vec<[f32; 3]> = Vec::new();
            let mut idx: Vec<[usize; 3]> = Vec::with_capacity(list.len());
            for &ti in list {
                let mut tri = [0usize; 3];
                for j in 0..3 {
                    let v = tris[ti * 3 + j] - base;
                    let key = [(v.x as f64 * 1e4).round() as i64, (v.y as f64 * 1e4).round() as i64, (v.z as f64 * 1e4).round() as i64];
                    tri[j] = *map.entry(key).or_insert_with(|| {
                        pts.push([key[0] as f32 * 1e-4, key[1] as f32 * 1e-4, key[2] as f32 * 1e-4]);
                        pts.len()
                    });
                }
                if tri[0] != tri[1] && tri[1] != tri[2] && tri[0] != tri[2] {
                    idx.push(tri);
                }
            }
            if idx.is_empty() {
                continue;
            }
            // inchis = fiecare muchie folosita exact de doua triunghiuri (solid etans)
            let mut edges: HashMap<(usize, usize), u32> = HashMap::new();
            for t in &idx {
                for k in 0..3 {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    *edges.entry((a.min(b), a.max(b))).or_default() += 1;
                }
            }
            let closed = edges.values().all(|n| *n == 2);
            parts.push((*c, pts, idx, closed));
        }
        if parts.is_empty() {
            continue;
        }
        // geometrie identica deja scrisa (fitinguri, flanse...): instanta a aceleiasi reprezentari (IfcMappedItem)
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            for (c, pts, idx, _) in &parts {
                c.hash(&mut h);
                for p in pts {
                    for v in p {
                        v.to_bits().hash(&mut h);
                    }
                }
                idx.hash(&mut h);
            }
            h.finish()
        };
        let rep = match rep_maps.get(&key) {
            Some(map) => {
                let op = w.add(&format!("IFCCARTESIANTRANSFORMATIONOPERATOR3D($,$,#{o3},1.,$)"));
                let mi = w.add(&format!("IFCMAPPEDITEM(#{map},#{op})"));
                st.instances += 1;
                w.add(&format!("IFCSHAPEREPRESENTATION(#{body_ctx},'Body','MappedRepresentation',(#{mi}))"))
            }
            None => {
                let mut items_ids = Vec::new();
                for (col, pts, idx, closed) in &parts {
                    let mut pl = String::with_capacity(pts.len() * 30);
                    for (i, p) in pts.iter().enumerate() {
                        if i > 0 {
                            pl.push(',');
                        }
                        let _ = write!(pl, "({},{},{})", f(p[0] as f64), f(p[1] as f64), f(p[2] as f64));
                    }
                    let plist = w.add(&format!("IFCCARTESIANPOINTLIST3D(({pl}),$)"));
                    let mut il = String::with_capacity(idx.len() * 16);
                    for (i, t) in idx.iter().enumerate() {
                        if i > 0 {
                            il.push(',');
                        }
                        let _ = write!(il, "({},{},{})", t[0], t[1], t[2]);
                    }
                    let fs = w.add(&format!("IFCTRIANGULATEDFACESET(#{plist},$,{},({il}),$)", if *closed { ".T." } else { ".F." }));
                    st.triangles += idx.len();
                    if *closed {
                        st.closed += 1;
                    }
                    let style = match styles.get(col) {
                        Some(s) => *s,
                        None => {
                            let rgb = w.add(&format!("IFCCOLOURRGB($,{},{},{})", f(col[0] as f64 / 255.0), f(col[1] as f64 / 255.0), f(col[2] as f64 / 255.0)));
                            let sh = w.add(&format!("IFCSURFACESTYLESHADING(#{rgb},{})", f(1.0 - col[3] as f64 / 255.0)));
                            let s = w.add(&format!("IFCSURFACESTYLE($,.BOTH.,(#{sh}))"));
                            styles.insert(*col, s);
                            s
                        }
                    };
                    w.add(&format!("IFCSTYLEDITEM(#{fs},(#{style}),$)"));
                    items_ids.push(fs);
                }
                let list = items_ids.iter().map(|i| format!("#{i}")).collect::<Vec<_>>().join(",");
                let shape = w.add(&format!("IFCSHAPEREPRESENTATION(#{body_ctx},'Body','Tessellation',({list}))"));
                // prima aparitie: reprezentarea devine si harta pentru urmatoarele elemente identice
                let map = w.add(&format!("IFCREPRESENTATIONMAP(#{ax},#{shape})"));
                rep_maps.insert(key, map);
                let op = w.add(&format!("IFCCARTESIANTRANSFORMATIONOPERATOR3D($,$,#{o3},1.,$)"));
                let mi = w.add(&format!("IFCMAPPEDITEM(#{map},#{op})"));
                w.add(&format!("IFCSHAPEREPRESENTATION(#{body_ctx},'Body','MappedRepresentation',(#{mi}))"))
            }
        };
        let pds = w.add(&format!("IFCPRODUCTDEFINITIONSHAPE($,$,(#{rep}))"));
        let lvl = if e.storey.trim().is_empty() { "(no level)".to_string() } else { e.storey.clone() };
        let (sid, spl) = storeys[&lvl];
        let bp = w.add(&format!("IFCCARTESIANPOINT(({},{},{}))", f(base.x as f64), f(base.y as f64), f(base.z as f64)));
        let bax = w.add(&format!("IFCAXIS2PLACEMENT3D(#{bp},#{zdir},#{xdir})"));
        let epl = w.add(&format!("IFCLOCALPLACEMENT(#{spl},#{bax})"));
        let mut mapped: Option<(&str, &str)> = None;
        if let Some(r) = crate::ifc_map::resolve(map, &e.ifc_type).or_else(|| crate::ifc_map::resolve(map, &e.name)) {
            if r.class.eq_ignore_ascii_case("Not Exported") || r.class.trim().is_empty() {
                st.elements = st.elements.saturating_sub(0); // elementul nu se scrie
                continue;
            }
            mapped = Some((r.class.as_str(), r.ifc_type.as_str()));
        }
        let (class, extra, otype) = if let Some((c, ty)) = mapped {
            let cl = ifc_class(c).map(|(x, _)| x).unwrap_or(c);
            let ex = ifc_class(c).map(|(_, x)| x).unwrap_or(2);
            (cl, ex, if ty.trim().is_empty() { "$".into() } else { step_str(ty) })
        } else {
            match ifc_class(&e.ifc_type) {
                Some((c, x)) => (c, x, "$".to_string()),
                None => ("IfcBuildingElementProxy", 2, if e.ifc_type.trim().is_empty() { "$".into() } else { step_str(&e.ifc_type) }),
            }
        };
        let mut gid = if is_ifc_guid(&e.guid) { e.guid.clone() } else { guid_from(&format!("{}|{}|{}", m.path, e.guid, k)) };
        if !used_guids.insert(gid.clone()) {
            gid = guid_from(&format!("dup|{gid}|{k}"));
            used_guids.insert(gid.clone());
        }
        let tail = vec!["$"; extra].join(",");
        let el = w.add(&format!(
            "{}({},#{oh},{},$,{},#{epl},#{pds},{tail})",
            class.to_uppercase(),
            step_str(&gid),
            step_str(&e.name),
            otype
        ));
        contained.entry(sid).or_default().push(el);
        st.elements += 1;
        for key in element_classes(&e.props) {
            class_refs.entry(key).or_default().push(el);
        }

        // proprietati: un IfcPropertySet pe grup (fara cele calculate de ManageBIM pentru arbore)
        let mut groups: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
        for p in &e.props {
            if p.name.trim().is_empty() {
                continue;
            }
            let g = if p.group.trim().is_empty() { "Properties" } else { p.group.as_str() };
            if !mvd.keep_prop(g, &p.name) {
                continue;
            }
            groups.entry(g).or_default().push((p.name.as_str(), p.value.as_str()));
        }
        if (mvd != crate::ifc_mvd::Mvd::Site) && (!e.ifc_type.is_empty() || !e.guid.is_empty()) {
            let g = "ManageBIM";
            let extra = [
                ("Source IFC type", e.ifc_type.as_str()),
                ("Source GUID", e.guid.as_str()),
                ("Source model", m.name.as_str()),
            ];
            let kept: Vec<_> = extra.into_iter().filter(|(n, _)| mvd.keep_prop(g, n)).collect();
            if !kept.is_empty() {
                groups.entry(g).or_default().extend(kept);
            }
        }
        for (g, props) in groups {
            let mut seen = std::collections::HashSet::new();
            let mut ids = Vec::new();
            for (n, v) in props {
                if !seen.insert(n) {
                    continue;
                }
                // seturile standard (Pset_WallCommon...) cu tipul corect; restul text
                let val = crate::ifc_psets::step_value(g, n, v, &step_str).unwrap_or_else(|| if v.chars().count() <= 255 { format!("IFCLABEL({})", step_str(v)) } else { format!("IFCTEXT({})", step_str(v)) });
                ids.push(w.add(&format!("IFCPROPERTYSINGLEVALUE({},$,{val},$)", step_str(n))));
                st.properties += 1;
            }
            if ids.is_empty() {
                continue;
            }
            let list = ids.iter().map(|i| format!("#{i}")).collect::<Vec<_>>().join(",");
            let ps = w.add(&format!("IFCPROPERTYSET({},#{oh},{},$,({list}))", step_str(&guid_from(&format!("pset|{gid}|{g}"))), step_str(g)));
            w.add(&format!("IFCRELDEFINESBYPROPERTIES({},#{oh},$,$,(#{el}),#{ps})", step_str(&guid_from(&format!("rdp|{gid}|{g}")))));
        }
    }
    // IfcClassification (un sistem) + IfcClassificationReference (un cod) + legatura cu elementele
    let mut systems: BTreeMap<String, u64> = BTreeMap::new();
    for ((system, code, name, uri), els) in &class_refs {
        let sys = *systems.entry(system.clone()).or_insert_with(|| {
            let loc = if system.eq_ignore_ascii_case("IFC") { "'https://identifier.buildingsmart.org/uri/buildingsmart/ifc/4.3'".to_string() } else { "$".to_string() };
            w.add(&format!("IFCCLASSIFICATION($,$,$,{},$,{loc},$)", step_str(system)))
        });
        let loc = if uri.is_empty() { "$".to_string() } else { step_str(uri) };
        let nm = if name.is_empty() { "$".to_string() } else { step_str(name) };
        let r = w.add(&format!("IFCCLASSIFICATIONREFERENCE({loc},{},{nm},#{sys},$,$)", step_str(code)));
        for chunk in els.chunks(5000) {
            let list = chunk.iter().map(|i| format!("#{i}")).collect::<Vec<_>>().join(",");
            w.add(&format!("IFCRELASSOCIATESCLASSIFICATION({},#{oh},$,$,({list}),#{r})", step_str(&guid_from(&format!("rac|{system}|{code}|{}|{now}", chunk[0])))));
        }
    }
    for (sid, els) in contained {
        for chunk in els.chunks(5000) {
            let list = chunk.iter().map(|i| format!("#{i}")).collect::<Vec<_>>().join(",");
            w.add(&format!("IFCRELCONTAINEDINSPATIALSTRUCTURE({},#{oh},$,$,({list}),#{sid})", step_str(&guid_from(&format!("rcss|{sid}|{}|{now}", chunk[0])))));
        }
    }
    let header = format!(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('ViewDefinition [{}]'),'2;1');\nFILE_NAME({},{},(''),('ManageBIM'),{},'ManageBIM','');\nFILE_SCHEMA(('IFC4'));\nENDSEC;\nDATA;\n",
        mvd.view_name().replace([' ', '.'], ""),
        step_str(&format!("{project}.ifc")),
        step_str(now),
        step_str(&format!("ManageBIM {}", env!("CARGO_PKG_VERSION")))
    );
    (format!("{header}{}ENDSEC;\nEND-ISO-10303-21;\n", w.s), st)
}

/// Clasificarile unui element, din cele doua forme de proprietati:
/// - atribuite in ManageBIM (bSDD / catalog): grup = sistemul, "Classification" = "cod — nume",
///   optional "Classification URI";
/// - citite din IFC: grup "Classification", nume = sistemul, valoare = codul, "<sistem> name" = denumirea.
fn element_classes(props: &[crate::model::Prop]) -> Vec<(String, String, String, String)> {
    let find = |g: &str, n: &str| props.iter().find(|p| p.group == g && p.name.eq_ignore_ascii_case(n)).map(|p| p.value.trim().to_string()).unwrap_or_default();
    let mut out = Vec::new();
    for p in props {
        let v = p.value.trim();
        if v.is_empty() {
            continue;
        }
        if p.name.eq_ignore_ascii_case("Classification") && !p.group.eq_ignore_ascii_case("Classification") {
            let (code, name) = match v.split_once(" — ").or_else(|| v.split_once(" - ")) {
                Some((c, n)) => (c.trim().to_string(), n.trim().to_string()),
                None => (v.to_string(), String::new()),
            };
            out.push((p.group.trim().to_string(), code, name, find(&p.group, "Classification URI")));
        } else if p.group.eq_ignore_ascii_case("Classification") && !p.name.ends_with(" name") && !p.name.eq_ignore_ascii_case("Classification URI") {
            out.push((p.name.trim().to_string(), v.to_string(), find(&p.group, &format!("{} name", p.name)), String::new()));
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_props_become_references() {
        let p = |g: &str, n: &str, v: &str| crate::model::Prop { group: g.into(), name: n.into(), value: v.into() };
        let props = vec![
            p("IFC", "Classification", "IfcWall — Wall"),
            p("IFC", "Classification URI", "https://identifier.buildingsmart.org/uri/buildingsmart/ifc/4.3/class/IfcWall"),
            p("Classification", "Uniclass 2015", "EF_20_10_20"),
            p("Classification", "Uniclass 2015 name", "Walls"),
        ];
        let c = element_classes(&props);
        assert_eq!(c.len(), 2);
        assert!(c.contains(&("IFC".into(), "IfcWall".into(), "Wall".into(), "https://identifier.buildingsmart.org/uri/buildingsmart/ifc/4.3/class/IfcWall".into())));
        assert!(c.contains(&("Uniclass 2015".into(), "EF_20_10_20".into(), "Walls".into(), String::new())));
    }

    #[test]
    fn step_strings_and_guids() {
        assert_eq!(step_str("it's"), "'it''s'");
        assert_eq!(step_str("ă"), "'\\X2\\0103\\X0\\'");
        let g = guid_from("abc");
        assert_eq!(g.len(), 22);
        assert!(is_ifc_guid(&g));
        assert!(matches!(g.as_bytes()[0], b'0'..=b'3'));
        assert_eq!(g, guid_from("abc"));
        assert_ne!(g, guid_from("abd"));
    }
}
