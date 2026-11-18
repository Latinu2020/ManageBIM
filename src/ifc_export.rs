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
}

/// Scrie fisierul IFC. `items`: (model, element) de exportat; `origin`: coordonatele reale ale originii scenei.
pub fn write_ifc(items: &[(&Model, &Element)], origin: DVec3, project: &str, now: &str) -> (String, ExportStats) {
    let mut w = W { s: String::new(), n: 0 };
    let mut st = ExportStats { elements: 0, triangles: 0, properties: 0 };
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
    let mut used_guids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (k, (m, e)) in items.iter().enumerate() {
        let tris = m.tris(e);
        if tris.len() < 3 {
            continue;
        }
        // varfuri sudate (aceeasi pozitie -> acelasi index)
        let mut map: HashMap<[u32; 3], usize> = HashMap::new();
        let mut pts: Vec<[f32; 3]> = Vec::new();
        let mut idx: Vec<[usize; 3]> = Vec::with_capacity(tris.len() / 3);
        for t in tris.chunks_exact(3) {
            let mut tri = [0usize; 3];
            for (j, v) in t.iter().enumerate() {
                let key = [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()];
                tri[j] = *map.entry(key).or_insert_with(|| {
                    pts.push([v.x, v.y, v.z]);
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
        let fs = w.add(&format!("IFCTRIANGULATEDFACESET(#{plist},$,$,({il}),$)"));
        st.triangles += idx.len();
        // culoarea cea mai folosita a elementului
        let col = {
            let cols = &m.tri_color[e.tri_start as usize..(e.tri_start + e.tri_count) as usize];
            let mut cnt: HashMap<[u8; 4], usize> = HashMap::new();
            for c in cols {
                *cnt.entry(*c).or_default() += 1;
            }
            cnt.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0))).map(|x| x.0).unwrap_or([200, 200, 200, 255])
        };
        let style = match styles.get(&col) {
            Some(s) => *s,
            None => {
                let rgb = w.add(&format!("IFCCOLOURRGB($,{},{},{})", f(col[0] as f64 / 255.0), f(col[1] as f64 / 255.0), f(col[2] as f64 / 255.0)));
                let sh = w.add(&format!("IFCSURFACESTYLESHADING(#{rgb},{})", f(1.0 - col[3] as f64 / 255.0)));
                let s = w.add(&format!("IFCSURFACESTYLE($,.BOTH.,(#{sh}))"));
                styles.insert(col, s);
                s
            }
        };
        w.add(&format!("IFCSTYLEDITEM(#{fs},(#{style}),$)"));
        let rep = w.add(&format!("IFCSHAPEREPRESENTATION(#{body_ctx},'Body','Tessellation',(#{fs}))"));
        let pds = w.add(&format!("IFCPRODUCTDEFINITIONSHAPE($,$,(#{rep}))"));
        let lvl = if e.storey.trim().is_empty() { "(no level)".to_string() } else { e.storey.clone() };
        let (sid, spl) = storeys[&lvl];
        let epl = w.add(&format!("IFCLOCALPLACEMENT(#{spl},#{ax})"));
        let (class, extra, otype) = match ifc_class(&e.ifc_type) {
            Some((c, x)) => (c, x, "$".to_string()),
            None => ("IfcBuildingElementProxy", 2, if e.ifc_type.trim().is_empty() { "$".into() } else { step_str(&e.ifc_type) }),
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

        // proprietati: un IfcPropertySet pe grup (fara cele calculate de ManageBIM pentru arbore)
        let mut groups: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
        for p in &e.props {
            if p.name.trim().is_empty() {
                continue;
            }
            groups.entry(if p.group.trim().is_empty() { "Properties" } else { p.group.as_str() }).or_default().push((p.name.as_str(), p.value.as_str()));
        }
        if !e.ifc_type.is_empty() || !e.guid.is_empty() {
            groups.entry("ManageBIM").or_default().extend([("Source IFC type", e.ifc_type.as_str()), ("Source GUID", e.guid.as_str()), ("Source model", m.name.as_str())]);
        }
        for (g, props) in groups {
            let mut seen = std::collections::HashSet::new();
            let mut ids = Vec::new();
            for (n, v) in props {
                if !seen.insert(n) {
                    continue;
                }
                let val = if v.chars().count() <= 255 { format!("IFCLABEL({})", step_str(v)) } else { format!("IFCTEXT({})", step_str(v)) };
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
    for (sid, els) in contained {
        for chunk in els.chunks(5000) {
            let list = chunk.iter().map(|i| format!("#{i}")).collect::<Vec<_>>().join(",");
            w.add(&format!("IFCRELCONTAINEDINSPATIALSTRUCTURE({},#{oh},$,$,({list}),#{sid})", step_str(&guid_from(&format!("rcss|{sid}|{}|{now}", chunk[0])))));
        }
    }
    let header = format!(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('ViewDefinition [ReferenceView_V1.2]'),'2;1');\nFILE_NAME({},{},(''),('ManageBIM'),{},'ManageBIM','');\nFILE_SCHEMA(('IFC4'));\nENDSEC;\nDATA;\n",
        step_str(&format!("{project}.ifc")),
        step_str(now),
        step_str(&format!("ManageBIM {}", env!("CARGO_PKG_VERSION")))
    );
    (format!("{header}{}ENDSEC;\nEND-ISO-10303-21;\n", w.s), st)
}

#[cfg(test)]
mod tests {
    use super::*;

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
