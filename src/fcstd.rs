//! FreeCAD (.FCStd) without FreeCAD: the file is a ZIP archive with `Document.xml` (the objects, the visibility,
//! the BREP file of each shape), `GuiDocument.xml` (the colors) and the shapes in OpenCascade BREP format
//! (`*.brp`). We read the archive, take the VISIBLE objects that have a shape and pass them to the OCCT reader
//! (`crate::occt`). One FreeCAD object = one ManageBIM element, with its color.
use crate::model::{Prop, RawElement, RawModel};
use std::collections::HashMap;

/// The entries of a ZIP archive (only what we need: stored + deflate, no ZIP64 / encryption).
pub struct Zip {
    data: Vec<u8>,
    entries: HashMap<String, (u16, usize, usize)>, // name -> (method, data offset, compressed size)
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
        let mut count = u16_at(&data, eocd + 10).ok_or_else(bad)?;
        let mut p = u32_at(&data, eocd + 16).ok_or_else(bad)?;
        let u64_at = |b: &[u8], o: usize| b.get(o..o + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap()) as usize);
        // ZIP64 (large archives, or written by some programs even when small): EOCD64 through the locator
        if (count == 0xFFFF || p == 0xFFFF_FFFF) && eocd >= 20 && u32_at(&data, eocd - 20) == Some(0x0706_4b50) {
            if let Some(e64) = u64_at(&data, eocd - 20 + 8) {
                if u32_at(&data, e64) == Some(0x0606_4b50) {
                    count = u64_at(&data, e64 + 32).ok_or_else(bad)?;
                    p = u64_at(&data, e64 + 48).ok_or_else(bad)?;
                }
            }
        }
        let mut entries = HashMap::new();
        for _ in 0..count {
            if u32_at(&data, p) != Some(0x0201_4b50) {
                return Err(bad());
            }
            let method = u16_at(&data, p + 10).ok_or_else(bad)? as u16;
            let mut csize = u32_at(&data, p + 20).ok_or_else(bad)?;
            let usize32 = u32_at(&data, p + 24).ok_or_else(bad)?;
            let (nl, el, cl) = (u16_at(&data, p + 28).ok_or_else(bad)?, u16_at(&data, p + 30).ok_or_else(bad)?, u16_at(&data, p + 32).ok_or_else(bad)?);
            let mut local = u32_at(&data, p + 42).ok_or_else(bad)?;
            let name = String::from_utf8_lossy(data.get(p + 46..p + 46 + nl).ok_or_else(bad)?).to_string();
            // the ZIP64 extra field (0x0001): the 8-byte sizes / offset, only the ones marked 0xFFFFFFFF
            if csize == 0xFFFF_FFFF || local == 0xFFFF_FFFF || usize32 == 0xFFFF_FFFF {
                let mut x = p + 46 + nl;
                let end = x + el;
                while x + 4 <= end {
                    let (id, len) = (u16_at(&data, x).unwrap_or(0), u16_at(&data, x + 2).unwrap_or(0));
                    if id == 1 {
                        let mut q = x + 4;
                        if usize32 == 0xFFFF_FFFF {
                            q += 8;
                        }
                        if csize == 0xFFFF_FFFF {
                            csize = u64_at(&data, q).ok_or_else(bad)?;
                            q += 8;
                        }
                        if local == 0xFFFF_FFFF {
                            local = u64_at(&data, q).ok_or_else(bad)?;
                        }
                        break;
                    }
                    x += 4 + len;
                }
            }
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

/// The value of attribute `attr="..."` from the first tag that starts in `s` (no exotic XML entities).
fn attr(s: &str, attr: &str) -> Option<String> {
    let k = format!("{attr}=\"");
    let i = s.find(&k)? + k.len();
    let j = s[i..].find('"')? + i;
    Some(unescape(&s[i..j]))
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// The `<tag name="X" ...> ... </tag>` blocks in a section: (name, content).
fn blocks<'a>(s: &'a str, tag: &str) -> Vec<(String, &'a str)> {
    let open = format!("<{tag} name=\"");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find(&open) {
        let r = &rest[i..];
        let name = attr(r, "name").unwrap_or_default();
        let end = r.find(&close).map(|e| e + close.len()).unwrap_or(r.len());
        // empty tag `<tag name="X" ... />`: no content
        let head_end = r.find('>').unwrap_or(0);
        let body = if r[..head_end].ends_with('/') { "" } else { &r[..end] };
        out.push((name, body));
        rest = &r[end.max(1)..];
    }
    out
}

/// The content of property `<Property name="N" ...> ... </Property>` in an object.
fn property<'a>(obj: &'a str, name: &str) -> Option<&'a str> {
    let k = format!("<Property name=\"{name}\"");
    let i = obj.find(&k)?;
    let r = &obj[i..];
    let e = r.find("</Property>").unwrap_or(r.len());
    Some(&r[..e])
}

/// Packed FreeCAD color (r<<24 | g<<16 | b<<8 | a) -> RGB.
fn packed(v: u32) -> [u8; 3] {
    [(v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8]
}

/// The color and transparency (0..1) of a view provider from GuiDocument.xml.
fn gui_color(zip: &Zip, vp: &str) -> Option<([u8; 3], f32)> {
    // FreeCAD 1.x: ShapeAppearance = list of materials (binary): u32 n, then per material
    // ambient, diffuse, specular, emissive (packed u32, LE), shininess, transparency (f32)...
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
    // old versions: ShapeColor + Transparency (0..100)
    let c = property(vp, "ShapeColor").and_then(|p| attr(p, "value")).and_then(|v| v.parse::<u32>().ok())?;
    let t = property(vp, "Transparency").and_then(|p| attr(p, "value")).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
    Some((packed(c), (t / 100.0).clamp(0.0, 1.0)))
}

/// A FreeCAD object with a shape to display.
#[derive(Debug, Clone)]
pub struct FcObject {
    pub name: String,
    pub label: String,
    pub kind: String,
    pub shape_file: String,
    pub color: Option<([u8; 3], f32)>,
}

/// The visible objects with a 3D shape (no sketches, axes, origin planes).
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
    // a PartDesign Body displays the shape of the last operation (Tip): the operations (Pad, Pocket...) would duplicate the geometry
    let has_body = types.values().any(|t| t == "PartDesign::Body");
    let mut out = Vec::new();
    for (name, body) in blocks(data, "Object") {
        let kind = types.get(&name).cloned().unwrap_or_default();
        // App::* = containers (App::Part, groups, BuildingPart Arch) - their shape repeats the children
        if kind.starts_with("Sketcher::") || kind.starts_with("App::") || (has_body && kind.starts_with("PartDesign::") && kind != "PartDesign::Body") {
            continue;
        }
        let Some(file) = property(body, "Shape").and_then(|p| attr(p, "file")) else { continue };
        // visibility: in Document.xml (FreeCAD 0.19+) or in GuiDocument.xml (old versions)
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
                no_surface += 1; // lines / points (e.g. Draft wires) - nothing to display in 3D
                continue;
            }
        };
        // all solids of the object = a single element (as you see it in the FreeCAD tree)
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
