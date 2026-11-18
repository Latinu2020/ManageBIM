//! FreeCAD (.FCStd) fara FreeCAD: fisierul e o arhiva ZIP cu `Document.xml` (obiectele, vizibilitatea,
//! fisierul BREP al fiecarei forme), `GuiDocument.xml` (culorile) si formele in format BREP OpenCascade
//! (`*.brp`). Citim arhiva, luam obiectele VIZIBILE care au o forma si le trimitem cititorului OCCT
//! (`crate::occt`). Un obiect FreeCAD = un element ManageBIM, cu culoarea lui.
use crate::model::{Prop, RawElement, RawModel};
use std::collections::HashMap;

/// Intrarile unei arhive ZIP (doar ce ne trebuie: stored + deflate, fara ZIP64 / criptare).
pub struct Zip {
    data: Vec<u8>,
    entries: HashMap<String, (u16, usize, usize)>, // nume -> (metoda, offset date, marime comprimata)
}

fn u16_at(b: &[u8], o: usize) -> Option<usize> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]) as usize)
}
fn u32_at(b: &[u8], o: usize) -> Option<usize> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize)
}

impl Zip {
    pub fn parse(data: Vec<u8>) -> Result<Zip, String> {
        let bad = || "Not a valid FreeCAD file (ZIP archive expected).".to_string();
        // End Of Central Directory: ultimii 22 + comentariu (max 64 KB)
        let min = data.len().saturating_sub(22 + 65535);
        let eocd = (min..data.len().saturating_sub(21)).rev().find(|&i| data[i..i + 4] == [0x50, 0x4b, 0x05, 0x06]).ok_or_else(bad)?;
        let count = u16_at(&data, eocd + 10).ok_or_else(bad)?;
        let mut p = u32_at(&data, eocd + 16).ok_or_else(bad)?;
        let mut entries = HashMap::new();
        for _ in 0..count {
            if u32_at(&data, p) != Some(0x0201_4b50) {
                return Err(bad());
            }
            let method = u16_at(&data, p + 10).ok_or_else(bad)? as u16;
            let csize = u32_at(&data, p + 20).ok_or_else(bad)?;
            let (nl, el, cl) = (u16_at(&data, p + 28).ok_or_else(bad)?, u16_at(&data, p + 30).ok_or_else(bad)?, u16_at(&data, p + 32).ok_or_else(bad)?);
            let local = u32_at(&data, p + 42).ok_or_else(bad)?;
            let name = String::from_utf8_lossy(data.get(p + 46..p + 46 + nl).ok_or_else(bad)?).to_string();
            if u32_at(&data, local) == Some(0x0403_4b50) {
                let (lnl, lel) = (u16_at(&data, local + 26).ok_or_else(bad)?, u16_at(&data, local + 28).ok_or_else(bad)?);
                entries.insert(name, (method, local + 30 + lnl + lel, csize));
            }
            p += 46 + nl + el + cl;
        }
        Ok(Zip { data, entries })
    }

    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }

    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        let &(method, off, size) = self.entries.get(name)?;
        let raw = self.data.get(off..off + size)?;
        match method {
            0 => Some(raw.to_vec()),
            8 => miniz_oxide::inflate::decompress_to_vec(raw).ok(),
            _ => None,
        }
    }

    pub fn text(&self, name: &str) -> Option<String> {
        self.read(name).map(|b| String::from_utf8_lossy(&b).into_owned())
    }
}

/// Valoarea atributului `attr="..."` din primul tag care incepe in `s` (fara entitati XML exotice).
fn attr(s: &str, attr: &str) -> Option<String> {
    let k = format!("{attr}=\"");
    let i = s.find(&k)? + k.len();
    let j = s[i..].find('"')? + i;
    Some(unescape(&s[i..j]))
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// Blocurile `<tag name="X" ...> ... </tag>` dintr-o sectiune: (nume, continut).
fn blocks<'a>(s: &'a str, tag: &str) -> Vec<(String, &'a str)> {
    let open = format!("<{tag} name=\"");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find(&open) {
        let r = &rest[i..];
        let name = attr(r, "name").unwrap_or_default();
        let end = r.find(&close).map(|e| e + close.len()).unwrap_or(r.len());
        // tag gol `<tag name="X" ... />`: fara continut
        let head_end = r.find('>').unwrap_or(0);
        let body = if r[..head_end].ends_with('/') { "" } else { &r[..end] };
        out.push((name, body));
        rest = &r[end.max(1)..];
    }
    out
}

/// Continutul proprietatii `<Property name="N" ...> ... </Property>` dintr-un obiect.
fn property<'a>(obj: &'a str, name: &str) -> Option<&'a str> {
    let k = format!("<Property name=\"{name}\"");
    let i = obj.find(&k)?;
    let r = &obj[i..];
    let e = r.find("</Property>").unwrap_or(r.len());
    Some(&r[..e])
}

/// Culoare FreeCAD impachetata (r<<24 | g<<16 | b<<8 | a) -> RGB.
fn packed(v: u32) -> [u8; 3] {
    [(v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8]
}

/// Culoarea si transparenta (0..1) unui view provider din GuiDocument.xml.
fn gui_color(zip: &Zip, vp: &str) -> Option<([u8; 3], f32)> {
    // FreeCAD 1.x: ShapeAppearance = lista de materiale (binar): u32 n, apoi per material
    // ambient, diffuse, specular, emissive (u32 impachetate, LE), shininess, transparency (f32)...
    if let Some(f) = property(vp, "ShapeAppearance").and_then(|p| attr(p, "file")) {
        if let Some(b) = zip.read(&f) {
            if let (Some(n), Some(d)) = (u32_at(&b, 0), u32_at(&b, 8)) {
                if n >= 1 {
                    let tr = b.get(24..28).map(|s| f32::from_le_bytes([s[0], s[1], s[2], s[3]])).unwrap_or(0.0);
                    return Some((packed(d as u32), if tr.is_finite() { tr.clamp(0.0, 1.0) } else { 0.0 }));
                }
            }
        }
    }
    // versiunile vechi: ShapeColor + Transparency (0..100)
    let c = property(vp, "ShapeColor").and_then(|p| attr(p, "value")).and_then(|v| v.parse::<u32>().ok())?;
    let t = property(vp, "Transparency").and_then(|p| attr(p, "value")).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
    Some((packed(c), (t / 100.0).clamp(0.0, 1.0)))
}

/// Un obiect FreeCAD cu forma de afisat.
#[derive(Debug, Clone)]
pub struct FcObject {
    pub name: String,
    pub label: String,
    pub kind: String,
    pub shape_file: String,
    pub color: Option<([u8; 3], f32)>,
}

/// Obiectele vizibile cu forma 3D (fara schite, axe, plane de origine).
pub fn visible_objects(zip: &Zip) -> Result<Vec<FcObject>, String> {
    let doc = zip.text("Document.xml").ok_or("Document.xml is missing - not a FreeCAD document.")?;
    let types: HashMap<String, String> = {
        let objs = doc.find("<Objects").map(|i| &doc[i..]).unwrap_or("");
        let objs = &objs[..objs.find("</Objects>").unwrap_or(objs.len())];
        objs.split("<Object ").skip(1).filter_map(|o| Some((attr(o, "name")?, attr(o, "type").unwrap_or_default()))).collect()
    };
    let gui = zip.text("GuiDocument.xml").unwrap_or_default();
    let vps: HashMap<String, &str> = blocks(&gui, "ViewProvider").into_iter().collect();
    let data = doc.find("<ObjectData").map(|i| &doc[i..]).unwrap_or("");
    // un Body PartDesign afiseaza forma ultimei operatii (Tip): operatiile (Pad, Pocket...) ar dubla geometria
    let has_body = types.values().any(|t| t == "PartDesign::Body");
    let mut out = Vec::new();
    for (name, body) in blocks(data, "Object") {
        let kind = types.get(&name).cloned().unwrap_or_default();
        // App::* = containere (App::Part, grupuri, BuildingPart Arch) - forma lor repeta copiii
        if kind.starts_with("Sketcher::") || kind.starts_with("App::") || (has_body && kind.starts_with("PartDesign::") && kind != "PartDesign::Body") {
            continue;
        }
        let Some(file) = property(body, "Shape").and_then(|p| attr(p, "file")) else { continue };
        // vizibilitatea: in Document.xml (FreeCAD 0.19+) sau in GuiDocument.xml (versiuni vechi)
        let vis = property(body, "Visibility").or_else(|| vps.get(&name).and_then(|v| property(v, "Visibility"))).and_then(|p| attr(p, "value"));
        if vis.as_deref() == Some("false") {
            continue;
        }
        let label = property(body, "Label").and_then(|p| attr(p, "value")).filter(|l| !l.is_empty()).unwrap_or_else(|| name.clone());
        let color = vps.get(&name).and_then(|v| gui_color(zip, v));
        out.push(FcObject { name, label, kind, shape_file: file, color });
    }
    Ok(out)
}

pub fn load_fcstd(path: &str, progress: &dyn Fn(f32, &str)) -> Result<RawModel, String> {
    let file = crate::file_name(path);
    progress(0.02, "Reading FreeCAD document");
    let zip = Zip::parse(std::fs::read(path).map_err(|e| format!("Cannot read {file}: {e}"))?)?;
    let objs = visible_objects(&zip)?;
    if objs.is_empty() {
        return Err(format!("{file}: no visible 3D objects in this FreeCAD document."));
    }
    let doc_label = zip
        .text("Document.xml")
        .and_then(|d| property(&d, "Label").and_then(|p| attr(p, "value")))
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
    let dir = std::env::temp_dir().join(format!("bimrust_fcstd_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create a temp folder: {e}"))?;
    let mut elements = Vec::new();
    let mut warnings = Vec::new();
    let (mut no_surface, mut binary) = (0, 0);
    let n = objs.len();
    for (k, o) in objs.iter().enumerate() {
        progress(0.05 + 0.9 * k as f32 / n as f32, &format!("Tessellating {} ({}/{})", o.label, k + 1, n));
        if !o.shape_file.to_lowercase().ends_with(".brp") {
            binary += 1;
            continue;
        }
        let Some(bytes) = zip.read(&o.shape_file).filter(|b| !b.is_empty()) else { continue };
        let tmp = dir.join(format!("{k}.brep"));
        if std::fs::write(&tmp, &bytes).is_err() {
            continue;
        }
        let r = crate::occt::load(&tmp.to_string_lossy(), crate::occt::CadKind::Brep, Default::default(), &|_, _| {});
        let _ = std::fs::remove_file(&tmp);
        let m = match r {
            Ok(m) => m,
            Err(e) if e.contains("not found") || e.contains("Cannot load") => {
                let _ = std::fs::remove_dir_all(&dir);
                return Err(e);
            }
            Err(_) => {
                no_surface += 1; // linii / puncte (ex. Draft wires) - nimic de afisat in 3D
                continue;
            }
        };
        // toate solidele obiectului = un singur element (cum il vezi in arborele FreeCAD)
        let (mut tris, mut colors) = (Vec::new(), Vec::new());
        let mut solids = 0usize;
        for e in m.elements {
            solids += e.props.iter().find(|p| p.name == "Solids").and_then(|p| p.value.parse::<usize>().ok()).unwrap_or(0);
            let c = o.color.map(|(c, t)| [c[0], c[1], c[2], ((1.0 - t) * 255.0).round() as u8]);
            colors.extend(e.colors.iter().map(|x| c.unwrap_or(*x)));
            tris.extend(e.tris);
        }
        if tris.is_empty() {
            no_surface += 1;
            continue;
        }
        let short = o.kind.rsplit("::").next().unwrap_or(&o.kind).to_string();
        let p = |n: &str, v: String| Prop { group: "FreeCAD".into(), name: n.into(), value: v };
        let props = vec![p("Label", o.label.clone()), p("Name", o.name.clone()), p("Type", o.kind.clone()), p("Solids", solids.to_string())];
        elements.push(RawElement { name: o.label.clone(), ifc_type: if short.is_empty() { "Part".into() } else { short }, guid: format!("FCStd:{}", o.name), storey: doc_label.clone(), props, tris, colors, default_hidden: false });
    }
    let _ = std::fs::remove_dir_all(&dir);
    if binary > 0 {
        warnings.push(format!("{binary} objects saved as binary BREP were skipped (in FreeCAD: Preferences > General > Document, untick binary shape saving, then save again)."));
    }
    if no_surface > 0 {
        warnings.push(format!("{no_surface} visible objects have no surfaces (lines / points) and are not shown."));
    }
    if elements.is_empty() {
        return Err(format!("{file}: no visible 3D solids or surfaces in this FreeCAD document."));
    }
    let info = vec![
        ("Converted from".to_string(), "FreeCAD .FCStd (built-in reader, OpenCascade)".to_string()),
        ("Document".to_string(), doc_label),
        ("Objects shown".to_string(), elements.len().to_string()),
    ];
    progress(1.0, "Done");
    Ok(RawModel { elements, info, warnings })
}
