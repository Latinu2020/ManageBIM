//! Subset MaterialX / OpenPBR: import și export de parametri + imagini.
//! Nu evaluăm grafuri MaterialX (ar cere biblioteca C++); citim nodurile de suprafață simple
//! (`OpenPBR Surface`, `UsdPreviewSurface`, `Principled BSDF`, `Standard Surface`) și imaginile
//! atașate intrărilor. Scriem noduri `OpenPBR Surface` cu aceiași parametri.

use crate::materials::{default_ior, Material, Pattern};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

fn unesc(s: &str) -> String {
    s.replace("&apos;", "'").replace("&quot;", "\"").replace("&gt;", ">").replace("&lt;", "<").replace("&amp;", "&")
}

/// atributele unui tag `<...>`, fără validare XML completă (fără panic pe fișiere stricate)
fn attrs(tag: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let b = tag.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let ks = i;
        while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'-' || b[i] == b':') {
            i += 1;
        }
        if i == ks {
            i += 1;
            continue;
        }
        let key = &tag[ks..i];
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] != b'=' {
            continue;
        }
        i += 1;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || (b[i] != b'"' && b[i] != b'\'') {
            continue;
        }
        let q = b[i];
        i += 1;
        let vs = i;
        while i < b.len() && b[i] != q {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        out.insert(key.to_string(), unesc(&tag[vs..i]));
        i += 1;
    }
    out
}

fn num(v: &str) -> Option<f32> {
    v.split([',', ' ']).find(|s| !s.is_empty()).and_then(|s| s.parse::<f32>().ok())
}

fn color(v: &str, default: [u8; 3]) -> [u8; 3] {
    let p: Vec<f32> = v.split([',', ' ']).filter_map(|s| s.parse::<f32>().ok()).collect();
    if p.len() < 3 {
        return default;
    }
    let to8 = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    [to8(p[0]), to8(p[1]), to8(p[2])]
}

/// Citește fișierul MaterialX (XML) și intoarce materialele găsite.
pub fn read(path: &str) -> Result<Vec<Material>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("Cannot read {path}: {e}"))?;
    let lower = text.to_ascii_lowercase();
    if !lower.contains("<materialx") && !lower.contains("<node") {
        return Err(format!("{path} is not a MaterialX XML file."));
    }
    // imaginile declarate global: <image name="map1" file="albedo.png"/>
    let mut images: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for line in text.lines() {
        if let Some(p) = line.find("<image") {
            let end = line[p..].find('>').map(|x| p + x).unwrap_or(line.len());
            let a = attrs(&line[p..end]);
            if let (Some(n), Some(f)) = (a.get("name"), a.get("file")) {
                images.insert(n.clone(), f.clone());
            }
        }
    }
    let base = std::path::Path::new(path).parent().unwrap_or(std::path::Path::new("."));
    let resolve = |f: String| -> String {
        if std::path::Path::new(&f).is_absolute() {
            return f;
        }
        // MaterialX folosește adesea "~/" pentru folderul documentului
        let f = f.trim_start_matches("~/").to_string();
        base.join(&f).to_string_lossy().to_string()
    };

    let mut out: Vec<Material> = Vec::new();
    let mut cur: Option<Material> = None;
    let mut last_input: Option<String> = None;
    let mut mat_name: Option<String> = None;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("</node") {
            if let Some(m) = cur.take() {
                out.push(m);
            }
            continue;
        }
        if t.starts_with("<material") {
            mat_name = attrs(t).get("name").cloned();
            continue;
        }
        if t.starts_with("<node") {
            if let Some(m) = cur.take() {
                out.push(m);
            }
            let a = attrs(t);
            let ty = a.get("type").map(|s| s.as_str()).unwrap_or("").to_ascii_lowercase();
            if !ty.contains("pbr") && !ty.contains("surface") && !ty.contains("principled") {
                continue;
            }
            let name = mat_name.take().or_else(|| a.get("name").cloned()).unwrap_or_else(|| format!("MaterialX {}", out.len() + 1));
            cur = Some(Material {
                name,
                color: [200, 200, 200],
                transparency: 0,
                gloss: 20,
                metal: 0,
                pattern: Pattern::None,
                scale: 1.0,
                texture: String::new(),
                normal_map: String::new(),
                roughness_map: String::new(),
                metallic_map: String::new(),
                ao_map: String::new(),
                height_map: String::new(),
                normal_strength: 1.0,
                rough_mult: 1.0,
                metal_mult: 1.0,
                coat: 0,
                coat_roughness: 5,
                sheen: 0,
                ior: default_ior(),
            });
            continue;
        }
        let Some(m) = cur.as_mut() else { continue };
        if t.starts_with("<input") {
            let a = attrs(t);
            let name = a.get("name").map(|s| s.to_ascii_lowercase()).unwrap_or_default();
            if let Some(v) = a.get("value") {
                match name.as_str() {
                    "base_color" | "diffusecolor" | "base color" | "albedo" | "color" => m.color = color(v, m.color),
                    "metallic" => m.metal = (num(v).unwrap_or(0.0).clamp(0.0, 1.0) * 100.0) as u8,
                    "roughness" => m.gloss = (100.0 - num(v).unwrap_or(0.5).clamp(0.0, 1.0) * 100.0) as u8,
                    "coat" | "clearcoat" | "coat weight" => m.coat = (num(v).unwrap_or(0.0).clamp(0.0, 1.0) * 100.0) as u8,
                    "coat_roughness" | "coat roughness" => m.coat_roughness = (num(v).unwrap_or(0.05).clamp(0.0, 1.0) * 100.0) as u8,
                    "sheen" | "sheen weight" => m.sheen = (num(v).unwrap_or(0.0).clamp(0.0, 1.0) * 100.0) as u8,
                    "ior" | "specular_ior" => m.ior = num(v).unwrap_or(1.5).clamp(1.0, 2.5),
                    "transmission" => m.transparency = (num(v).unwrap_or(0.0).clamp(0.0, 0.9) * 100.0) as u8,
                    _ => {}
                }
            }
            last_input = Some(name);
            // imagine legată direct pe același rând: <input ...><image file="..."/></input>
            if let Some(p) = t.find("<image") {
                let end = t[p..].find('>').map(|x| p + x).unwrap_or(t.len());
                let ia = attrs(&t[p..end]);
                if let Some(f) = ia.get("file") {
                    if let Some(inp) = last_input.as_deref() {
                        apply_image(m, inp, resolve(f.clone()));
                    }
                }
            }
        } else if t.starts_with("<image") {
            let a = attrs(t);
            if let (Some(f), Some(inp)) = (a.get("file"), last_input.as_deref()) {
                apply_image(m, inp, resolve(f.clone()));
                last_input = None;
            }
        }
    }
    if let Some(m) = cur {
        out.push(m);
    }
    if out.is_empty() {
        return Err("No OpenPBR / Preview Surface node found in the MaterialX file.".into());
    }
    Ok(out)
}

fn apply_image(m: &mut Material, input: &str, file: String) {
    match input {
        "base_color" | "diffusecolor" | "base color" | "albedo" | "color" => m.texture = file,
        "normal" | "normalmap" | "normal map" => m.normal_map = file,
        "roughness" => m.roughness_map = file,
        "metallic" | "metalness" => m.metallic_map = file,
        "ao" | "ambientocclusion" => m.ao_map = file,
        "height" | "displacement" => m.height_map = file,
        "coat" | "coat_roughness" | "sheen" => {}
        _ => {}
    }
}

/// Scrie materialele ca noduri OpenPBR Surface în XML MaterialX.
pub fn write(mats: Vec<Material>, sel: Vec<crate::model::ElemId>, path: &str) -> Result<usize, String> {
    let mut xml = String::from("<?xml version=\"1.0\"?>\n<materialx version=\"1.39\">\n");
    let mut n = 0;
    for m in &mats {
        let safe: String = m.name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        let c = m.color;
        xml.push_str(&format!("  <material name=\"{}\">\n", esc(&m.name)));
        xml.push_str(&format!("    <node name=\"{}_surface\" type=\"OpenPBR Surface\">\n", safe));
        let inp = |name: &str, value: String| format!("      <input name=\"{}\" value=\"{}\" />\n", name, value);
        xml.push_str(&inp("base_color", format!("{:.4}, {:.4}, {:.4}", c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0)));
        xml.push_str(&inp("metallic", format!("{:.3}", m.metal as f32 / 100.0)));
        xml.push_str(&inp("roughness", format!("{:.3}", 1.0 - m.gloss as f32 / 100.0)));
        xml.push_str(&inp("coat", format!("{:.3}", m.coat as f32 / 100.0)));
        xml.push_str(&inp("coat_roughness", format!("{:.3}", m.coat_roughness as f32 / 100.0)));
        xml.push_str(&inp("sheen", format!("{:.3}", m.sheen as f32 / 100.0)));
        xml.push_str(&inp("ior", format!("{:.3}", m.ior)));
        let maps = [
            ("base_color", m.texture.as_str()), ("normal", m.normal_map.as_str()),
            ("roughness", m.roughness_map.as_str()), ("metallic", m.metallic_map.as_str()),
            ("ao", m.ao_map.as_str()), ("height", m.height_map.as_str()),
        ];
        for (role, file) in maps {
            if !file.is_empty() {
                xml.push_str(&format!("      <input name=\"{}\">\n        <image file=\"{}\" />\n      </input>\n", role, esc(file)));
            }
        }
        xml.push_str("    </node>\n  </material>\n");
        n += 1;
    }
    xml.push_str("</materialx>\n");
    let _ = sel;
    std::fs::write(path, xml).map_err(|e| format!("Cannot write {path}: {e}"))?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    #[test]
    fn roundtrip_materialx() {
        // o cale absoluta pe fiecare sistem (pe Linux / FreeBSD "C:/..." e relativa si e rezolvata fata de fisier)
        #[cfg(windows)]
        const ROOT: &str = "C:/";
        #[cfg(not(windows))]
        const ROOT: &str = "/";
        let dir = std::env::temp_dir().join("mb_mtlx_test");
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("lib.mtlx");
        let m = crate::materials::Material {
            name: "Clear coat red".into(), color: [200, 40, 30], transparency: 0,
            gloss: 70, metal: 10, pattern: crate::materials::Pattern::None, scale: 1.0,
            texture: format!("{ROOT}tex/albedo.png"), normal_map: format!("{ROOT}tex/normal.png"),
            roughness_map: String::new(), metallic_map: String::new(), ao_map: String::new(), height_map: String::new(),
            normal_strength: 1.0, rough_mult: 1.0, metal_mult: 1.0,
            coat: 80, coat_roughness: 10, sheen: 30, ior: 1.6,
        };
        super::write(vec![m], vec![], p.to_str().unwrap()).unwrap();
        let v = super::read(p.to_str().unwrap()).expect("read");
        assert_eq!(v.len(), 1);
        let r = &v[0];
        assert_eq!(r.name, "Clear coat red");
        assert_eq!(r.coat, 80);
        assert_eq!(r.sheen, 30);
        assert!((r.ior - 1.6).abs() < 1e-3);
        assert_eq!(r.texture, format!("{ROOT}tex/albedo.png"));
        assert_eq!(r.normal_map, format!("{ROOT}tex/normal.png"));
        assert!((r.gloss as i32 - 70).abs() <= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
