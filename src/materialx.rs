//! MaterialX / OpenPBR: import si export de materiale cu imagini.
//! Citim formatul standard (MaterialX 1.38 / 1.39: standard_surface, open_pbr_surface, UsdPreviewSurface,
//! gltf_pbr, legaturi nodename / nodegraph spre noduri image / tiledimage, fileprefix) - fara a evalua
//! calcule din graf. Scriem MaterialX 1.39 standard (open_pbr_surface + image + surfacematerial).

use crate::materials::{default_ior, Material, Pattern};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
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
///
/// Formatul standard (MaterialX 1.38 / 1.39, ca la ambientCG, Poly Haven, Substance, AMD GPUOpen,
/// Blender, Houdini, Omniverse): noduri de suprafata `standard_surface`, `open_pbr_surface`,
/// `UsdPreviewSurface`, `gltf_pbr` legate de `surfacematerial`; intrarile sunt valori sau legaturi
/// (`nodename`, `nodegraph` + `output`) spre noduri `image` / `tiledimage` / `gltf_image` /
/// `UsdUVTexture`, eventual prin `normalmap`, `displacement` sau noduri de calcul. `fileprefix` se aplica.
/// Formatul vechi scris de ManageBIM (`<material><node type="OpenPBR Surface"><input><image/>`) merge si el.
pub fn read(path: &str) -> Result<Vec<Material>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("Cannot read {path}: {e}"))?;
    let root = crate::e57::parse_xml(&text).map_err(|e| format!("{}: not a valid MaterialX XML file ({}).", crate::file_name(path), e.replace("E57: ", "")))?;
    if !root.name.eq_ignore_ascii_case("materialx") {
        return Err(format!("{} is not a MaterialX file (root element <{}>).", crate::file_name(path), root.name));
    }
    let base = std::path::Path::new(path).parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
    let doc = Doc::new(&root);
    let mut out: Vec<Material> = Vec::new();
    // 1. materialele standard: <surfacematerial> -> shader
    for m in root.kids.iter().filter(|k| k.name == "surfacematerial" || k.name == "material" && k.kids.iter().any(|i| i.name == "shaderref")) {
        let shader = m
            .kids
            .iter()
            .find(|i| i.name == "input" && i.attr("name") == Some("surfaceshader"))
            .and_then(|i| i.attr("nodename"))
            .or_else(|| m.kids.iter().find(|i| i.name == "shaderref").and_then(|i| i.attr("node")))
            .and_then(|n| doc.node(None, n));
        let Some((shader, scope)) = shader else { continue };
        let name = m.attr("uiname").or(m.attr("name")).unwrap_or("MaterialX").to_string();
        let mut mat = from_shader(&doc, shader, scope, &base, name);
        // deplasarea: <input name="displacementshader" nodename="..."/> -> displacement -> imagine
        if let Some(d) = m.kids.iter().find(|i| i.name == "input" && i.attr("name") == Some("displacementshader")).and_then(|i| i.attr("nodename")).and_then(|n| doc.node(None, n)) {
            if let Some(f) = d.0.kids.iter().find(|i| i.name == "input" && i.attr("name") == Some("displacement")).and_then(|i| doc.image_of(i, d.1, &base, 0)) {
                mat.height_map = f;
            }
        }
        out.push(mat);
    }
    // 2. noduri de suprafata fara surfacematerial (fisiere simple)
    if out.is_empty() {
        for k in &root.kids {
            if is_shader(&k.name) {
                let name = k.attr("name").unwrap_or("MaterialX").to_string();
                out.push(from_shader(&doc, k, None, &base, name));
            }
        }
    }
    // 3. formatul vechi ManageBIM: <material name><node type="OpenPBR Surface"> <input name value> <image file/>
    if out.is_empty() {
        for m in root.kids.iter().filter(|k| k.name == "material") {
            for n in m.kids.iter().filter(|k| k.name == "node") {
                let ty = n.attr("type").unwrap_or("").to_ascii_lowercase();
                if !(ty.contains("pbr") || ty.contains("surface") || ty.contains("principled")) {
                    continue;
                }
                let name = m.attr("name").or(n.attr("name")).unwrap_or("MaterialX").to_string();
                out.push(from_shader(&doc, n, None, &base, name));
            }
        }
    }
    if out.is_empty() {
        return Err(format!("{}: no surface shader found (standard_surface, open_pbr_surface, UsdPreviewSurface or gltf_pbr).", crate::file_name(path)));
    }
    for m in &mut out {
        fill_sibling_maps(m);
    }
    Ok(out)
}

fn is_shader(n: &str) -> bool {
    matches!(n, "standard_surface" | "open_pbr_surface" | "UsdPreviewSurface" | "gltf_pbr" | "disney_principled" | "node")
}

/// Indexul nodurilor documentului (si al grafurilor) dupa nume.
struct Doc<'a> {
    root: &'a crate::e57::Node,
    graphs: std::collections::HashMap<&'a str, &'a crate::e57::Node>,
}

impl<'a> Doc<'a> {
    fn new(root: &'a crate::e57::Node) -> Self {
        let graphs = root.kids.iter().filter(|k| k.name == "nodegraph").filter_map(|g| g.attr("name").map(|n| (n, g))).collect();
        Doc { root, graphs }
    }

    /// nodul `name` in graful `scope` (sau la radacina)
    fn node(&self, scope: Option<&'a crate::e57::Node>, name: &str) -> Option<(&'a crate::e57::Node, Option<&'a crate::e57::Node>)> {
        let parent = scope.unwrap_or(self.root);
        parent.kids.iter().find(|k| k.attr("name") == Some(name) && k.name != "input" && k.name != "output").map(|n| (n, scope))
    }

    /// prefixul de fisier cumulat (radacina + graf + nod + intrare)
    fn prefix(&self, scope: Option<&crate::e57::Node>, node: &crate::e57::Node, input: &crate::e57::Node) -> String {
        let mut p = self.root.attr("fileprefix").unwrap_or("").to_string();
        for x in [scope.and_then(|s| s.attr("fileprefix")), node.attr("fileprefix"), input.attr("fileprefix")].into_iter().flatten() {
            p.push_str(x);
        }
        p
    }

    /// Imaginea (fisierul) din spatele unei intrari: valoare `filename`, legatura `nodename` spre un nod de
    /// imagine (prin normalmap / calcule, max 8 niveluri) sau `nodegraph` + `output`.
    fn image_of(&self, input: &'a crate::e57::Node, scope: Option<&'a crate::e57::Node>, base: &std::path::Path, depth: usize) -> Option<String> {
        if depth > 8 {
            return None;
        }
        let target = if let Some(n) = input.attr("nodename") {
            self.node(scope, n)
        } else if let Some(g) = input.attr("nodegraph").and_then(|g| self.graphs.get(g)) {
            // iesirea grafului: <output name="..." nodename="..."/>
            let outs: Vec<&crate::e57::Node> = g.kids.iter().filter(|k| k.name == "output").collect();
            let o = input.attr("output").and_then(|on| outs.iter().find(|o| o.attr("name") == Some(on)).copied()).or(outs.first().copied())?;
            self.node(Some(g), o.attr("nodename")?)
        } else {
            None
        }?;
        let (node, sc) = target;
        // nod de imagine: intrarea "file"
        if matches!(node.name.as_str(), "image" | "tiledimage" | "gltf_image" | "UsdUVTexture" | "hextiledimage") {
            let f = node.kids.iter().find(|k| k.name == "input" && k.attr("name") == Some("file"))?;
            let v = f.attr("value")?;
            let full = format!("{}{}", self.prefix(sc, node, f), v);
            return Some(resolve(base, &full));
        }
        // altfel urmam prima intrare legata (normalmap.in, multiply.in1, mix.fg ...)
        for i in node.kids.iter().filter(|k| k.name == "input") {
            if i.attr("nodename").is_some() || i.attr("nodegraph").is_some() {
                if let Some(f) = self.image_of(i, sc, base, depth + 1) {
                    return Some(f);
                }
            }
        }
        None
    }
}

fn resolve(base: &std::path::Path, f: &str) -> String {
    let f = f.trim_start_matches("~/");
    if std::path::Path::new(f).is_absolute() {
        return f.to_string();
    }
    base.join(f).to_string_lossy().to_string()
}

/// Materialul ManageBIM dintr-un nod de suprafata (parametrii + imaginile legate).
fn from_shader(doc: &Doc, shader: &crate::e57::Node, scope: Option<&crate::e57::Node>, base: &std::path::Path, name: String) -> Material {
    let mut m = Material {
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
    };
    let mut coat_from_map = false;
    for i in shader.kids.iter().filter(|k| k.name == "input") {
        let key = i.attr("name").unwrap_or("").to_ascii_lowercase();
        let key = key.as_str();
        let pct = |v: f32| (v.clamp(0.0, 1.0) * 100.0).round() as u8;
        if let Some(v) = i.attr("value") {
            match key {
                "base_color" | "diffusecolor" | "base color" | "albedo" | "color" => m.color = color(v, m.color),
                "metalness" | "metallic" | "base_metalness" => m.metal = pct(num(v).unwrap_or(0.0)),
                "specular_roughness" | "roughness" => m.gloss = 100 - pct(num(v).unwrap_or(0.5)),
                "coat" | "clearcoat" | "coat_weight" | "coat weight" => m.coat = pct(num(v).unwrap_or(0.0)),
                "coat_roughness" | "clearcoatroughness" | "coat roughness" => m.coat_roughness = pct(num(v).unwrap_or(0.05)),
                "sheen" | "fuzz_weight" | "sheen weight" => m.sheen = pct(num(v).unwrap_or(0.0)),
                "specular_ior" | "ior" => m.ior = num(v).unwrap_or(1.5).clamp(1.0, 2.5),
                "transmission" | "transmission_weight" => m.transparency = (num(v).unwrap_or(0.0).clamp(0.0, 0.9) * 100.0) as u8,
                "opacity" | "geometry_opacity" => {
                    let o = num(v).unwrap_or(1.0);
                    if o < 0.999 {
                        m.transparency = ((1.0 - o).clamp(0.0, 0.9) * 100.0) as u8;
                    }
                }
                _ => {}
            }
        }
        // imagine legata: noul format (nodename / nodegraph) sau vechiul <input><image file/></input>
        let img = doc.image_of(i, scope, base, 0).or_else(|| i.kids.iter().find(|k| k.name == "image").and_then(|k| k.attr("file")).map(|f| resolve(base, f)));
        if let Some(f) = img {
            match key {
                "base_color" | "diffusecolor" | "base color" | "albedo" | "color" => {
                    m.texture = f;
                    m.color = [255, 255, 255];
                }
                "normal" | "geometry_normal" | "normalmap" | "normal map" | "coat_normal" => {
                    if m.normal_map.is_empty() {
                        m.normal_map = f;
                    }
                }
                "specular_roughness" | "roughness" => m.roughness_map = f,
                "coat_roughness" | "clearcoatroughness" => {
                    coat_from_map = true;
                    if m.roughness_map.is_empty() {
                        m.roughness_map = f;
                    }
                }
                "metalness" | "metallic" | "base_metalness" => m.metallic_map = f,
                "occlusion" | "ao" | "ambientocclusion" => m.ao_map = f,
                "displacement" | "height" => m.height_map = f,
                _ => {}
            }
        }
    }
    // ambientCG pune coat = 1 cu rugozitatea din aceeasi harta: e de fapt luciul obisnuit, nu un lac
    if coat_from_map {
        m.coat = 0;
    }
    m
}

/// Hartile lipsa din fisierele vecine cu acelasi prefix (ex. ambientCG livreaza *_AmbientOcclusion.jpg
/// fara sa-l lege in .mtlx).
fn fill_sibling_maps(m: &mut Material) {
    let tex = std::path::Path::new(&m.texture);
    let (Some(dir), Some(stem)) = (tex.parent(), tex.file_stem().map(|s| s.to_string_lossy().to_string())) else { return };
    if !tex.exists() {
        return;
    }
    // prefixul comun: numele fara ultimul cuvant ("Bricks104_1K-JPG_Color" -> "Bricks104_1K-JPG")
    let prefix = stem.rsplit_once(['_', '-', ' ']).map(|(a, _)| a.to_ascii_lowercase()).unwrap_or_default();
    if prefix.is_empty() {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        let l = n.to_ascii_lowercase();
        if !l.starts_with(&prefix) || !(l.ends_with(".png") || l.ends_with(".jpg") || l.ends_with(".jpeg")) {
            continue;
        }
        let p = e.path().to_string_lossy().to_string();
        match crate::materials::pbr_role(&n) {
            Some("ao") if m.ao_map.is_empty() => m.ao_map = p,
            Some("height") if m.height_map.is_empty() => m.height_map = p,
            Some("roughness") if m.roughness_map.is_empty() => m.roughness_map = p,
            Some("metallic") if m.metallic_map.is_empty() => m.metallic_map = p,
            Some("normal") if m.normal_map.is_empty() && !l.contains("dx") => m.normal_map = p,
            _ => {}
        }
    }
}

/// Scrie materialele in MaterialX standard 1.39: `open_pbr_surface` + noduri `image` + `surfacematerial`
/// (citit de Blender, Houdini, Maya, Omniverse, MaterialX Viewer si inapoi de ManageBIM).
pub fn write(mats: Vec<Material>, sel: Vec<crate::model::ElemId>, path: &str) -> Result<usize, String> {
    let _ = sel;
    let mut xml = String::from("<?xml version=\"1.0\"?>\n<materialx version=\"1.39\">\n");
    let mut n = 0;
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    for m in &mats {
        // nume MaterialX valid si unic
        let mut safe: String = m.name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        if safe.is_empty() || safe.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            safe = format!("M_{safe}");
        }
        while !used.insert(safe.clone()) {
            safe.push('_');
        }
        let c = m.color;
        let sh = format!("{safe}_surface");
        let mut nodes = String::new();
        let mut inputs = String::new();
        let val = |name: &str, ty: &str, v: String| format!("    <input name=\"{name}\" type=\"{ty}\" value=\"{v}\" />\n");
        let maps: [(&str, &str, &str, &str); 4] = [
            ("base_color", "color3", "srgb_texture", m.texture.as_str()),
            ("specular_roughness", "float", "", m.roughness_map.as_str()),
            ("base_metalness", "float", "", m.metallic_map.as_str()),
            ("geometry_normal", "vector3", "", m.normal_map.as_str()),
        ];
        for (input, ty, cs, file) in maps {
            if file.is_empty() {
                continue;
            }
            let img = format!("{safe}_{input}_img");
            let cs_attr = if cs.is_empty() { String::new() } else { format!(" colorspace=\"{cs}\"") };
            if input == "geometry_normal" {
                nodes.push_str(&format!("  <image name=\"{img}\" type=\"vector3\">\n    <input name=\"file\" type=\"filename\" value=\"{}\" />\n  </image>\n", esc(file)));
                nodes.push_str(&format!("  <normalmap name=\"{safe}_normalmap\" type=\"vector3\">\n    <input name=\"in\" type=\"vector3\" nodename=\"{img}\" />\n  </normalmap>\n"));
                inputs.push_str(&format!("    <input name=\"{input}\" type=\"{ty}\" nodename=\"{safe}_normalmap\" />\n"));
            } else {
                nodes.push_str(&format!("  <image name=\"{img}\" type=\"{ty}\">\n    <input name=\"file\" type=\"filename\" value=\"{}\"{cs_attr} />\n  </image>\n", esc(file)));
                inputs.push_str(&format!("    <input name=\"{input}\" type=\"{ty}\" nodename=\"{img}\" />\n"));
            }
        }
        if m.texture.is_empty() {
            inputs.push_str(&val("base_color", "color3", format!("{:.4}, {:.4}, {:.4}", c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0)));
        }
        if m.metallic_map.is_empty() {
            inputs.push_str(&val("base_metalness", "float", format!("{:.3}", m.metal as f32 / 100.0)));
        }
        if m.roughness_map.is_empty() {
            inputs.push_str(&val("specular_roughness", "float", format!("{:.3}", 1.0 - m.gloss as f32 / 100.0)));
        }
        inputs.push_str(&val("coat_weight", "float", format!("{:.3}", m.coat as f32 / 100.0)));
        inputs.push_str(&val("coat_roughness", "float", format!("{:.3}", m.coat_roughness as f32 / 100.0)));
        inputs.push_str(&val("fuzz_weight", "float", format!("{:.3}", m.sheen as f32 / 100.0)));
        inputs.push_str(&val("specular_ior", "float", format!("{:.3}", m.ior)));
        if m.transparency > 0 {
            inputs.push_str(&val("transmission_weight", "float", format!("{:.3}", m.transparency as f32 / 100.0)));
        }
        xml.push_str(&nodes);
        xml.push_str(&format!("  <open_pbr_surface name=\"{sh}\" type=\"surfaceshader\">\n{inputs}  </open_pbr_surface>\n"));
        // inaltimea si AO: ca intrari ale materialului (citite inapoi de ManageBIM)
        let mut extra = String::new();
        if !m.height_map.is_empty() {
            let img = format!("{safe}_height_img");
            xml.push_str(&format!("  <image name=\"{img}\" type=\"float\">\n    <input name=\"file\" type=\"filename\" value=\"{}\" />\n  </image>\n", esc(&m.height_map)));
            xml.push_str(&format!("  <displacement name=\"{safe}_displacement\" type=\"displacementshader\">\n    <input name=\"displacement\" type=\"float\" nodename=\"{img}\" />\n    <input name=\"scale\" type=\"float\" value=\"0.02\" />\n  </displacement>\n"));
            extra.push_str(&format!("    <input name=\"displacementshader\" type=\"displacementshader\" nodename=\"{safe}_displacement\" />\n"));
        }
        // id MaterialX valid + numele afisat (cu spatii) in `uiname`
        xml.push_str(&format!("  <surfacematerial name=\"{safe}\" uiname=\"{}\" type=\"material\">\n    <input name=\"surfaceshader\" type=\"surfaceshader\" nodename=\"{sh}\" />\n{extra}  </surfacematerial>\n", esc(&m.name)));
        n += 1;
    }
    xml.push_str("</materialx>\n");
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
        // fisierul scris e MaterialX standard
        let x = std::fs::read_to_string(&p).unwrap();
        assert!(x.contains("<open_pbr_surface") && x.contains("<surfacematerial") && x.contains("<image"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Fisierul .mtlx din arhivele ambientCG (standard_surface + tiledimage + normalmap + displacement);
    /// AO-ul nu e legat in .mtlx, dar e luat din fisierul vecin.
    #[test]
    fn ambientcg_mtlx() {
        let dir = std::env::temp_dir().join(format!("mb_mtlx_acg_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["Bricks104_1K-JPG_Color.jpg", "Bricks104_1K-JPG_NormalGL.jpg", "Bricks104_1K-JPG_Roughness.jpg", "Bricks104_1K-JPG_Displacement.jpg", "Bricks104_1K-JPG_AmbientOcclusion.jpg"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        let x = r#"<?xml version="1.0"?>
<materialx version="1.38" fileprefix="./">
  <standard_surface xpos="6.1" name="Bricks104_1K_JPG_StandardSurface" type="surfaceshader" ypos="-1.8">
    <input name="specular" type="float" value="0" />
    <input name="coat" type="float" value="1" />
    <input name="coat_color" type="color3" value="1, 1, 1" />
    <input name="base" type="float" value="1" />
    <input name="base_color" type="color3" nodename="Bricks104_1K_JPG_Color" />
    <input name="normal" type="vector3" nodename="normalmap" />
    <input name="coat_normal" type="vector3" nodename="normalmap" />
    <input name="specular_roughness" type="float" nodename="Bricks104_1K_JPG_Roughness" />
    <input name="coat_roughness" type="float" nodename="Bricks104_1K_JPG_Roughness" />
  </standard_surface>
  <surfacematerial xpos="8.6" name="Bricks104_1K_JPG" type="material" ypos="0.0">
    <input name="surfaceshader" type="surfaceshader" nodename="Bricks104_1K_JPG_StandardSurface" />
    <input name="displacementshader" type="displacementshader" nodename="displacement" />
  </surfacematerial>
  <tiledimage xpos="3.6" name="Bricks104_1K_JPG_Color" type="color3" ypos="-3.1">
    <input name="file" type="filename" value="Bricks104_1K-JPG_Color.jpg" colorspace="srgb_texture" />
    <input name="uvtiling" type="vector2" value="1.0, 1.0" />
  </tiledimage>
  <tiledimage xpos="3.6" name="Bricks104_1K_JPG_Displacement" type="float" ypos="5.2">
    <input name="file" type="filename" value="Bricks104_1K-JPG_Displacement.jpg" />
  </tiledimage>
  <displacement xpos="6.1" name="displacement" type="displacementshader" ypos="1.5">
    <input name="displacement" type="float" nodename="Bricks104_1K_JPG_Displacement" />
    <input name="scale" type="float" value="1.0" />
  </displacement>
  <tiledimage xpos="1.1" name="Bricks104_1K_JPG_NormalGL" type="vector3" ypos="0.1">
    <input name="file" type="filename" value="Bricks104_1K-JPG_NormalGL.jpg" />
  </tiledimage>
  <normalmap xpos="3.6" name="normalmap" type="vector3" ypos="3.5">
    <input name="in" type="vector3" nodename="Bricks104_1K_JPG_NormalGL" />
    <input name="scale" type="float" value="1.0" />
  </normalmap>
  <tiledimage xpos="3.6" name="Bricks104_1K_JPG_Roughness" type="float" ypos="-0.5">
    <input name="file" type="filename" value="Bricks104_1K-JPG_Roughness.jpg" />
  </tiledimage>
</materialx>"#;
        let p = dir.join("Bricks104_1K-JPG.mtlx");
        std::fs::write(&p, x).unwrap();
        let v = super::read(p.to_str().unwrap()).expect("read");
        assert_eq!(v.len(), 1);
        let m = &v[0];
        assert_eq!(m.name, "Bricks104_1K_JPG");
        assert!(m.texture.ends_with("Bricks104_1K-JPG_Color.jpg"), "{}", m.texture);
        assert!(m.normal_map.ends_with("_NormalGL.jpg") && m.roughness_map.ends_with("_Roughness.jpg") && m.height_map.ends_with("_Displacement.jpg"), "{m:?}");
        assert!(m.ao_map.ends_with("_AmbientOcclusion.jpg"), "AO from the sibling file");
        assert_eq!(m.coat, 0, "ambientCG coat driven by the roughness map is not a lacquer");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// nodegraph + output + fileprefix (Substance / GPUOpen) si formatul vechi ManageBIM
    #[test]
    fn nodegraph_and_legacy_formats() {
        let dir = std::env::temp_dir().join(format!("mb_mtlx_ng_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ng = r#"<?xml version="1.0"?>
<materialx version="1.38">
  <nodegraph name="NG_wood" fileprefix="textures/">
    <image name="img_col" type="color3"><input name="file" type="filename" value="wood_basecolor.png" /></image>
    <multiply name="mul" type="color3"><input name="in1" type="color3" nodename="img_col" /><input name="in2" type="float" value="0.9" /></multiply>
    <output name="out_color" type="color3" nodename="mul" />
  </nodegraph>
  <open_pbr_surface name="SR_wood" type="surfaceshader">
    <input name="base_color" type="color3" nodegraph="NG_wood" output="out_color" />
    <input name="base_metalness" type="float" value="0" />
    <input name="specular_roughness" type="float" value="0.6" />
  </open_pbr_surface>
  <surfacematerial name="Wood" type="material"><input name="surfaceshader" type="surfaceshader" nodename="SR_wood" /></surfacematerial>
</materialx>"#;
        let p = dir.join("wood.mtlx");
        std::fs::write(&p, ng).unwrap();
        let v = super::read(p.to_str().unwrap()).expect("nodegraph");
        assert_eq!(v[0].name, "Wood");
        assert!(v[0].texture.replace('\\', "/").ends_with("textures/wood_basecolor.png"), "{}", v[0].texture);
        assert_eq!(v[0].gloss, 40);
        let old = r#"<?xml version="1.0"?>
<materialx version="1.39">
  <material name="Old red">
    <node name="Old_red_surface" type="OpenPBR Surface">
      <input name="base_color" value="0.8, 0.1, 0.1" />
      <input name="roughness" value="0.2" />
    </node>
  </material>
</materialx>"#;
        let p2 = dir.join("old.mtlx");
        std::fs::write(&p2, old).unwrap();
        let v2 = super::read(p2.to_str().unwrap()).expect("legacy");
        assert_eq!(v2[0].name, "Old red");
        assert_eq!(v2[0].color, [204, 26, 26]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
