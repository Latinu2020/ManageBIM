//! Render materials (Navisworks Presenter-style): library + assignment rules.
use crate::model::ElemId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Pattern {
    None,
    Brick,
    Tiles,
    Wood,
    Stone,
    Grass,
    Panels,
}

impl Pattern {
    pub const ALL: [Pattern; 7] = [Pattern::None, Pattern::Brick, Pattern::Tiles, Pattern::Wood, Pattern::Stone, Pattern::Grass, Pattern::Panels];
    pub fn label(&self) -> &'static str {
        match self {
            Pattern::None => "Plain",
            Pattern::Brick => "Brick",
            Pattern::Tiles => "Tiles",
            Pattern::Wood => "Wood grain",
            Pattern::Stone => "Stone / concrete",
            Pattern::Grass => "Grass",
            Pattern::Panels => "Panels / cladding",
        }
    }
    pub fn code(&self) -> u8 {
        match self {
            Pattern::None => 0,
            Pattern::Brick => 1,
            Pattern::Tiles => 2,
            Pattern::Wood => 3,
            Pattern::Stone => 4,
            Pattern::Grass => 5,
            Pattern::Panels => 6,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Material {
    pub name: String,
    pub color: [u8; 3],
    /// 0..90 %
    pub transparency: u8,
    /// 0..100
    pub gloss: u8,
    /// 0..100
    pub metal: u8,
    pub pattern: Pattern,
    /// pattern / texture size in metres
    pub scale: f32,
    /// external image texture (PNG / JPG); empty = procedural pattern
    #[serde(default)]
    pub texture: String,
}

impl Material {
    fn new(name: &str, color: [u8; 3], transparency: u8, gloss: u8, metal: u8, pattern: Pattern, scale: f32) -> Material {
        Material { name: name.into(), color, transparency, gloss, metal, pattern, scale, texture: String::new() }
    }
    pub fn rgba(&self) -> [u8; 4] {
        let a = (255.0 * (1.0 - self.transparency.min(90) as f32 / 100.0)).round() as u8;
        [self.color[0], self.color[1], self.color[2], a]
    }
    /// per-vertex material data for the shader: pattern, gloss, metal, scale
    /// `layer` = index in the GPU texture array when an image texture is loaded
    pub fn gpu(&self, layer: Option<u8>) -> [u8; 4] {
        let s = ((self.scale.clamp(0.05, 4.05) - 0.05) / 4.0 * 255.0).round() as u8;
        let code = match layer {
            Some(l) => 16 + l.min(200),
            None => self.pattern.code(),
        };
        [code, (self.gloss.min(100) as f32 * 2.55) as u8, (self.metal.min(100) as f32 * 2.55) as u8, s]
    }
}

pub fn library() -> Vec<Material> {
    use Pattern::*;
    vec![
        // Materiale cu valori PBR realiste (luciu / metalicitate 0..255) pentru randare profesională.
        Material::new("Concrete", [186, 186, 180], 0, 8, 0, Stone, 1.6),
        Material::new("Exposed concrete", [156, 156, 152], 0, 7, 0, Stone, 0.8),
        Material::new("Brick red", [162, 78, 56], 0, 6, 0, Brick, 0.5),
        Material::new("Brick yellow", [208, 174, 116], 0, 6, 0, Brick, 0.5),
        Material::new("Plaster white", [242, 240, 234], 0, 6, 0, None, 1.0),
        Material::new("Paint grey", [132, 136, 142], 0, 32, 0, None, 1.0),
        Material::new("Glass clear", [196, 224, 240], 78, 96, 0, None, 1.0),
        Material::new("Glass tinted", [92, 132, 152], 62, 96, 0, None, 1.0),
        Material::new("Steel", [158, 163, 170], 0, 88, 92, None, 1.0),
        Material::new("Aluminium", [208, 212, 218], 0, 78, 82, None, 1.0),
        Material::new("Galvanized steel", [186, 190, 194], 0, 58, 68, Stone, 0.3),
        Material::new("Copper", [192, 122, 76], 0, 80, 90, None, 1.0),
        Material::new("Wood oak", [178, 130, 84], 0, 28, 0, Wood, 1.0),
        Material::new("Wood dark", [102, 68, 44], 0, 32, 0, Wood, 1.0),
        Material::new("Ceramic tiles", [232, 232, 226], 0, 62, 0, Tiles, 0.3),
        Material::new("Roof tiles", [158, 68, 50], 0, 12, 0, Tiles, 0.35),
        Material::new("Facade panels", [214, 217, 220], 0, 48, 24, Panels, 1.2),
        Material::new("Asphalt", [66, 68, 72], 0, 7, 0, Stone, 0.5),
        Material::new("Grass", [92, 146, 68], 0, 4, 0, Grass, 1.0),
        Material::new("Water", [68, 120, 160], 38, 98, 0, None, 1.0),
        Material::new("Paint red", [200, 52, 42], 0, 45, 0, None, 1.0),
        Material::new("Paint yellow", [230, 190, 45], 0, 45, 0, None, 1.0),
        Material::new("Paint blue", [52, 100, 200], 0, 45, 0, None, 1.0),
        Material::new("Paint green", [62, 160, 82], 0, 45, 0, None, 1.0),
    ]
}

/// IFC type keyword -> library material (used by "Auto by IFC type").
pub const AUTO_RULES: [(&str, &str); 19] = [
    ("Wall", "Plaster white"),
    ("Slab", "Concrete"),
    ("Roof", "Roof tiles"),
    ("Column", "Exposed concrete"),
    ("Beam", "Exposed concrete"),
    ("Footing", "Concrete"),
    ("Pile", "Concrete"),
    ("Window", "Glass clear"),
    ("Plate", "Glass clear"),
    ("CurtainWall", "Glass clear"),
    ("Door", "Wood oak"),
    ("Stair", "Concrete"),
    ("Railing", "Steel"),
    ("Duct", "Galvanized steel"),
    ("AirTerminal", "Aluminium"),
    ("Pipe", "Copper"),
    ("CableCarrier", "Aluminium"),
    ("Covering", "Ceramic tiles"),
    ("Furni", "Wood dark"),
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MatTarget {
    /// a selection set or search set (search sets stay dynamic)
    Set(String),
    /// fixed items
    Items(Vec<ElemId>),
    /// every item whose IFC type contains this text
    Type(String),
}

impl MatTarget {
    pub fn label(&self) -> String {
        match self {
            MatTarget::Set(s) => format!("Set: {s}"),
            MatTarget::Items(v) => format!("{} selected items", v.len()),
            MatTarget::Type(t) => format!("Type: *{t}*"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatAssign {
    pub target: MatTarget,
    pub material: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

pub const TEX_SIZE: u32 = 512;
pub const MAX_TEXTURES: usize = 64;

/// Loads a PNG / JPG and resamples it to TEX_SIZE x TEX_SIZE RGBA (bilinear).
pub fn load_texture(path: &str) -> Result<Vec<u8>, String> {
    let data = std::fs::read(path).map_err(|e| format!("Cannot read texture {path}: {e}"))?;
    let (w, h, rgba): (u32, u32, Vec<u8>) = if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        let mut dec = png::Decoder::new(std::io::Cursor::new(&data));
        dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut r = dec.read_info().map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; r.output_buffer_size()];
        let info = r.next_frame(&mut buf).map_err(|e| e.to_string())?;
        let px = &buf[..info.buffer_size()];
        let rgba: Vec<u8> = match info.color_type {
            png::ColorType::Rgba => px.to_vec(),
            png::ColorType::Rgb => px.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
            png::ColorType::GrayscaleAlpha => px.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
            _ => px.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        };
        (info.width, info.height, rgba)
    } else if data.starts_with(&[0xFF, 0xD8]) {
        let mut dec = jpeg_decoder::Decoder::new(std::io::Cursor::new(&data));
        let px = dec.decode().map_err(|e| format!("JPEG: {e}"))?;
        let info = dec.info().ok_or("JPEG without header")?;
        let rgba: Vec<u8> = match info.pixel_format {
            jpeg_decoder::PixelFormat::RGB24 => px.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
            jpeg_decoder::PixelFormat::L8 => px.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
            jpeg_decoder::PixelFormat::CMYK32 => px.chunks_exact(4).flat_map(|c| {
                let k = 255 - c[3] as u32;
                [((255 - c[0] as u32) * k / 255) as u8, ((255 - c[1] as u32) * k / 255) as u8, ((255 - c[2] as u32) * k / 255) as u8, 255]
            }).collect(),
            _ => return Err("Unsupported JPEG pixel format".into()),
        };
        (info.width as u32, info.height as u32, rgba)
    } else {
        return Err(format!("{path}: only PNG and JPG textures are supported."));
    };
    if w == 0 || h == 0 {
        return Err("Empty image".into());
    }
    let n = TEX_SIZE;
    let mut out = vec![0u8; (n * n * 4) as usize];
    for y in 0..n {
        let fy = (y as f32 + 0.5) * h as f32 / n as f32 - 0.5;
        let y0 = fy.floor().clamp(0.0, (h - 1) as f32) as u32;
        let y1 = (y0 + 1).min(h - 1);
        let ty = (fy - y0 as f32).clamp(0.0, 1.0);
        for x in 0..n {
            let fx = (x as f32 + 0.5) * w as f32 / n as f32 - 0.5;
            let x0 = fx.floor().clamp(0.0, (w - 1) as f32) as u32;
            let x1 = (x0 + 1).min(w - 1);
            let tx = (fx - x0 as f32).clamp(0.0, 1.0);
            for c in 0..4 {
                let g = |xx: u32, yy: u32| rgba[((yy * w + xx) * 4 + c) as usize] as f32;
                let v = (g(x0, y0) * (1.0 - tx) + g(x1, y0) * tx) * (1.0 - ty) + (g(x0, y1) * (1.0 - tx) + g(x1, y1) * tx) * ty;
                // randul 0 al imaginii (sus) ajunge sus pe perete
                out[(((n - 1 - y) * n + x) * 4 + c as u32) as usize] = v.round() as u8;
            }
        }
    }
    Ok(out)
}

// ===================================================================== biblioteca utilizatorului

/// %APPDATA%\ManageBIM\Materials - aici stau texturile importate si `library.bimmat` (materialele tale,
/// disponibile in orice proiect).
pub fn user_library_dir() -> Option<std::path::PathBuf> {
    crate::formats::config_root().map(|d| d.join("ManageBIM").join("Materials"))
}

/// Materialele salvate de utilizator (gol daca nu exista inca).
pub fn user_library() -> Vec<Material> {
    let Some(f) = user_library_dir().map(|d| d.join("library.bimmat")) else { return Vec::new() };
    std::fs::read_to_string(f).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Adauga / inlocuieste (dupa nume) materialele in biblioteca utilizatorului.
pub fn save_to_user_library(mats: &[Material]) -> Result<std::path::PathBuf, String> {
    let dir = user_library_dir().ok_or("APPDATA is not set")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut lib = user_library();
    for m in mats {
        match lib.iter_mut().find(|x| x.name == m.name) {
            Some(x) => *x = m.clone(),
            None => lib.push(m.clone()),
        }
    }
    let f = dir.join("library.bimmat");
    let tmp = dir.join("library.bimmat.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&lib).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &f).map_err(|e| e.to_string())?;
    Ok(f)
}

/// Biblioteca completa: materialele incluse + ale utilizatorului (cele cu acelasi nume le inlocuiesc).
pub fn full_library() -> Vec<Material> {
    let mut lib = library();
    for m in user_library() {
        match lib.iter_mut().find(|x| x.name == m.name) {
            Some(x) => *x = m,
            None => lib.push(m),
        }
    }
    lib
}

fn is_image(n: &str) -> bool {
    let l = n.to_ascii_lowercase();
    l.ends_with(".png") || l.ends_with(".jpg") || l.ends_with(".jpeg")
}

/// rolul unei imagini dintr-un pachet PBR, dupa nume (freepbr, ambientCG, Poly Haven, textures.com...)
fn pbr_role(file: &str) -> Option<&'static str> {
    let l = file.to_ascii_lowercase();
    let l = l.rsplit(['/', '\\']).next().unwrap_or(&l);
    if l.contains("preview") || l.contains("thumb") || l.contains("sphere") {
        return None;
    }
    for (keys, role) in [
        (&["normal", "nrm", "_nor"][..], "normal"),
        (&["rough"][..], "roughness"),
        (&["metal"][..], "metallic"),
        (&["_ao", "-ao", "ambientocclusion", "ambient_occlusion", "occlusion"][..], "ao"),
        (&["height", "displace", "_disp", "bump"][..], "height"),
        (&["gloss"][..], "gloss"),
        (&["albedo", "basecolor", "base_color", "base-color", "diffuse", "diff", "color", "colour", "_col", "-col"][..], "albedo"),
    ] {
        if keys.iter().any(|k| l.contains(k)) {
            return Some(role);
        }
    }
    None
}

/// "fabric-pattern-07-bl.zip" -> "Fabric pattern 07"
pub fn pbr_name(path: &str) -> String {
    // numele fisierului fara extensie, cu / sau \ ca separator (cai Windows si Linux deopotriva)
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = file.rsplit_once('.').map(|(a, _)| a).unwrap_or(file).to_string();
    let mut words: Vec<String> = stem.split(['-', '_', ' ', '.']).filter(|w| !w.is_empty()).map(|w| w.to_string()).collect();
    while words.len() > 1 && matches!(words.last().map(|w| w.to_ascii_lowercase()).as_deref(), Some("bl" | "ue" | "unity" | "png" | "jpg" | "1k" | "2k" | "4k" | "8k" | "pbr")) {
        words.pop();
    }
    let s = words.join(" ");
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => "PBR material".into(),
    }
}

/// media unui canal (0..255) pe imaginea 512x512 RGBA
fn avg(rgba: &[u8], ch: usize) -> f32 {
    let n = rgba.len() / 4;
    if n == 0 {
        return 0.0;
    }
    rgba.chunks_exact(4).map(|p| p[ch] as f64).sum::<f64>() as f32 / n as f32
}

/// Importa un pachet PBR (arhiva .zip descarcata sau folderul dezarhivat) in biblioteca utilizatorului:
/// imaginile se copiaza in `dest\<nume>\`, iar materialul primeste textura de culoare (albedo),
/// luciul din harta de rugozitate si metalicitatea din harta metallic.
pub fn import_pbr(path: &str, dest_root: &std::path::Path) -> Result<Material, String> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let p = std::path::Path::new(path);
    if p.is_dir() {
        for e in std::fs::read_dir(p).map_err(|e| e.to_string())?.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if is_image(&n) {
                files.push((n, std::fs::read(e.path()).map_err(|e| e.to_string())?));
            }
        }
    } else {
        let zip = crate::fcstd::Zip::parse(std::fs::read(path).map_err(|e| format!("Cannot read {path}: {e}"))?).map_err(|_| format!("{} is not a ZIP archive.", crate::file_name(path)))?;
        let names: Vec<String> = zip.names().filter(|n| is_image(n) && !n.starts_with("__MACOSX")).cloned().collect();
        for n in names {
            if let Some(b) = zip.read(&n) {
                files.push((n.rsplit(['/', '\\']).next().unwrap_or(&n).to_string(), b));
            }
        }
    }
    if files.is_empty() {
        return Err(format!("{}: no PNG / JPG images inside.", crate::file_name(path)));
    }
    let albedo = files.iter().position(|(n, _)| pbr_role(n) == Some("albedo")).ok_or_else(|| {
        format!("{}: no colour map found (a file named *albedo*, *basecolor*, *diffuse* or *color*).", crate::file_name(path))
    })?;
    let name = pbr_name(path);
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
    let dir = dest_root.join(safe.trim());
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    let mut paths: Vec<(Option<&'static str>, std::path::PathBuf)> = Vec::new();
    for (n, b) in &files {
        let f = dir.join(n);
        std::fs::write(&f, b).map_err(|e| format!("Cannot write {}: {e}", f.display()))?;
        paths.push((pbr_role(n), f));
    }
    let tex = paths[albedo].1.to_string_lossy().to_string();
    load_texture(&tex)?;
    let map = |role: &str| paths.iter().find(|(r, _)| *r == Some(role)).and_then(|(_, f)| load_texture(&f.to_string_lossy()).ok());
    let gloss = match (map("roughness"), map("gloss")) {
        (Some(r), _) => (100.0 - avg(&r, 0) / 2.55).clamp(2.0, 95.0) as u8,
        (None, Some(g)) => (avg(&g, 0) / 2.55).clamp(2.0, 95.0) as u8,
        _ => 20,
    };
    let metal = map("metallic").map(|m| (avg(&m, 0) / 2.55).clamp(0.0, 100.0) as u8).unwrap_or(0);
    Ok(Material { name, color: [255, 255, 255], transparency: 0, gloss, metal, pattern: Pattern::None, scale: 1.0, texture: tex })
}
