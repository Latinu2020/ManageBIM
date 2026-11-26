//! Render materials (Navisworks Presenter-style): library + assignment rules.
use crate::model::ElemId;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, Ordering};

/// Rezolutia texturilor pe GPU: toate straturile array-ului au aceeasi marime.
/// 512 = 1 MB/strat, 1024 = 4 MB, 2048 = 16 MB (la 2048 limitam numarul de straturi).
pub const DEFAULT_TEX_SIZE: u32 = 512;
static TEX_RES: AtomicU32 = AtomicU32::new(DEFAULT_TEX_SIZE);

pub fn tex_size() -> u32 {
    TEX_RES.load(Ordering::Relaxed)
}

/// 0 = 512, 1 = 1024, 2 = 2048. Intoarce valoarea actuala.
pub fn set_tex_tier(tier: u32) -> u32 {
    let s = match tier { 1 => 1024, 2 => 2048, _ => 512 };
    TEX_RES.store(s, Ordering::Relaxed);
    s
}

pub fn tex_tier() -> u32 {
    match tex_size() { 1024 => 1, 2048 => 2, _ => 0 }
}

/// Numarul maxim de straturi din array, ca sa nu depasim ~256 MB de memorie GPU.
pub fn max_textures() -> usize {
    // 512 px = 1 MB/strat, 1024 = 4 MB, 2048 = 16 MB
    match tex_size() {
        2048 => 16,
        1024 => 64,
        _ => 192,
    }
}

/// codul special pentru materialele PBR impachetate; stratul e in octetul urmator (0..255)
pub const PBR_CODE: u8 = 200;

fn one() -> f32 { 1.0 }

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
    /// PBR (freepbr.com, ambientCG, Poly Haven...): toate hartile se pastreaza si se folosesc in 3D / randare
    #[serde(default)]
    pub normal_map: String,
    #[serde(default)]
    pub roughness_map: String,
    #[serde(default)]
    pub metallic_map: String,
    #[serde(default)]
    pub ao_map: String,
    #[serde(default)]
    pub height_map: String,
    /// intensitatile ajustabile in interfata (0..2); se aplica la incarcarea texturilor pe GPU
    #[serde(default = "one")]
    pub normal_strength: f32,
    #[serde(default = "one")]
    pub rough_mult: f32,
    #[serde(default = "one")]
    pub metal_mult: f32,
    /// OpenPBR Surface: strat de luciu (clearcoat) 0..100 si rugozitatea lui 0..100
    #[serde(default)]
    pub coat: u8,
    #[serde(default)]
    pub coat_roughness: u8,
    /// OpenPBR Surface: efect de tesatura (sheen) 0..100
    #[serde(default)]
    pub sheen: u8,
    /// indicent de refractie (1.0..2.5); 1.5 = sticla
    #[serde(default = "default_ior")]
    pub ior: f32,
}

pub fn default_ior() -> f32 { 1.5 }

impl Material {
    fn new(name: &str, color: [u8; 3], transparency: u8, gloss: u8, metal: u8, pattern: Pattern, scale: f32) -> Material {
        Material {
            name: name.into(), color, transparency, gloss, metal, pattern, scale, texture: String::new(),
            normal_map: String::new(), roughness_map: String::new(), metallic_map: String::new(),
            ao_map: String::new(), height_map: String::new(),
            normal_strength: 1.0, rough_mult: 1.0, metal_mult: 1.0,
            coat: 0, coat_roughness: 5, sheen: 0, ior: default_ior(),
        }
    }

    /// Are harti PBR dincolo de albedo?
    pub fn is_pbr(&self) -> bool {
        !self.normal_map.is_empty() || !self.roughness_map.is_empty() || !self.metallic_map.is_empty() || !self.ao_map.is_empty()
    }

    /// Toate imaginile materialului (pentru gasirea fisierelor mutate).
    pub fn maps(&self) -> Vec<&str> {
        [self.texture.as_str(), self.normal_map.as_str(), self.roughness_map.as_str(), self.metallic_map.as_str(), self.ao_map.as_str(), self.height_map.as_str()]
            .into_iter().filter(|s| !s.is_empty()).collect()
    }
    pub fn rgba(&self) -> [u8; 4] {
        let a = (255.0 * (1.0 - self.transparency.min(90) as f32 / 100.0)).round() as u8;
        [self.color[0], self.color[1], self.color[2], a]
    }
    /// per-vertex material data for the shader: pattern, gloss, metal, scale
    /// `layer` = index in the GPU texture array when an image texture is loaded
    pub fn gpu(&self, layer: Option<u8>) -> [u8; 4] {
        let s = ((self.scale.clamp(0.05, 4.05) - 0.05) / 4.0 * 255.0).round() as u8;
        match layer {
            // stratul PBR e in octetul 1 (0..255); proprietatile vin din harti, deci il putem folosi
            Some(l) if self.is_pbr() => [PBR_CODE, l, 0, s],
            Some(l) => [16 + l.min(181), (self.gloss.min(100) as f32 * 2.55) as u8, (self.metal.min(100) as f32 * 2.55) as u8, s],
            None => [self.pattern.code(), (self.gloss.min(100) as f32 * 2.55) as u8, (self.metal.min(100) as f32 * 2.55) as u8, s],
        }
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

pub const MAX_TEXTURES: usize = 64;

/// Miniatura unei texturi pentru lista de materiale (RGBA, `size` x `size`).
#[derive(Clone)]
pub struct Thumb {
    pub size: usize,
    pub rgba: Vec<u8>,
}

pub const THUMB_SIZE: u32 = 64;

/// Miniatura unei imagini, din cache-ul de pe disc (`<biblioteca>/thumbs`) sau calculata si salvata acolo.
/// Cheia cache-ului contine calea, marimea si data fisierului: o textura inlocuita primeste miniatura noua.
pub fn thumbnail(path: &str) -> Result<Thumb, String> {
    use std::hash::{Hash, Hasher};
    let meta = std::fs::metadata(path).map_err(|e| format!("{path}: {e}"))?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    meta.len().hash(&mut h);
    meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).hash(&mut h);
    let cache = user_library_dir().map(|d| d.join("thumbs").join(format!("{:016x}.png", h.finish())));
    let n = THUMB_SIZE as usize;
    if let Some(c) = &cache {
        if let Ok((w, hh, px)) = decode_file(&c.to_string_lossy()) {
            if w as usize == n && hh as usize == n {
                return Ok(Thumb { size: n, rgba: px });
            }
        }
    }
    let (w, hh, px) = decode_file(path)?;
    let rgba = resample(w, hh, &px, THUMB_SIZE);
    if let Some(c) = &cache {
        if let Some(d) = c.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(f) = std::fs::File::create(c) {
            let mut e = png::Encoder::new(std::io::BufWriter::new(f), THUMB_SIZE, THUMB_SIZE);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            if let Ok(mut wr) = e.write_header() {
                let _ = wr.write_image_data(&rgba);
            }
        }
    }
    Ok(Thumb { size: n, rgba })
}

/// Loads a PNG / JPG and resamples it to TEX_SIZE x TEX_SIZE RGBA (bilinear).
pub fn load_texture(path: &str) -> Result<Vec<u8>, String> {
    let (w, h, rgba) = decode_file(path)?;
    // randul 0 al imaginii (sus) ajunge sus pe perete
    Ok(resample_impl(w, h, &rgba, tex_size(), true))
}

/// PNG / JPG -> (latime, inaltime, RGBA)
pub fn decode_file(path: &str) -> Result<(u32, u32, Vec<u8>), String> {
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
    Ok((w, h, rgba))
}

/// Redimensionare biliniara la n x n; `flip` pune randul de sus al imaginii jos (texturile GPU).
fn resample_impl(w: u32, h: u32, rgba: &[u8], n: u32, flip: bool) -> Vec<u8> {
    let mut out = vec![0u8; (n * n * 4) as usize];
    for y in 0..n {
        let fy = (y as f32 + 0.5) * h as f32 / n as f32 - 0.5;
        let y0 = fy.floor().clamp(0.0, (h - 1) as f32) as u32;
        let y1 = (y0 + 1).min(h - 1);
        let ty = (fy - y0 as f32).clamp(0.0, 1.0);
        let oy = if flip { n - 1 - y } else { y };
        for x in 0..n {
            let fx = (x as f32 + 0.5) * w as f32 / n as f32 - 0.5;
            let x0 = fx.floor().clamp(0.0, (w - 1) as f32) as u32;
            let x1 = (x0 + 1).min(w - 1);
            let tx = (fx - x0 as f32).clamp(0.0, 1.0);
            for c in 0..4 {
                let g = |xx: u32, yy: u32| rgba[((yy * w + xx) * 4 + c) as usize] as f32;
                let v = (g(x0, y0) * (1.0 - tx) + g(x1, y0) * tx) * (1.0 - ty) + (g(x0, y1) * (1.0 - tx) + g(x1, y1) * tx) * ty;
                out[((oy * n + x) * 4 + c as u32) as usize] = v.round() as u8;
            }
        }
    }
    out
}

/// miniatura: fara intoarcere
pub fn resample(w: u32, h: u32, rgba: &[u8], n: u32) -> Vec<u8> {
    resample_impl(w, h, rgba, n, false)
}

/// Imparte hartile PBR in 2 straturi GPU de aceeasi marime:
/// - strat A: RGB = albedo * AO, A = rugozitate * rough_mult
/// - strat B: RGB = normala tangenta (xy * normal_strength), A = metalicitate * metal_mult
/// Shader-ul si path trace-ul decodeaza ambele straturi; modificarile de intensitate se vad imediat.
pub fn pack_pbr(
    albedo: &[u8], normal: Option<&[u8]>, rough: Option<&[u8]>, metal: Option<&[u8]>, ao: Option<&[u8]>,
    normal_strength: f32, rough_mult: f32, metal_mult: f32,
) -> (Vec<u8>, Vec<u8>) {
    let n = tex_size() as usize;
    let len = n * n * 4;
    let mut a = vec![0u8; len];
    let mut b = vec![0u8; len];
    let px = |src: Option<&[u8]>, i: usize, ch: usize, dflt: u8| src.map_or(dflt, |v| v.get(i + ch).copied().unwrap_or(dflt));
    for i in (0..len).step_by(4) {
        let ao_v = ao.map_or(1.0, |v| v.get(i).copied().unwrap_or(255) as f32 / 255.0);
        for c in 0..3 {
            a[i + c] = (albedo.get(i + c).copied().unwrap_or(200) as f32 * ao_v).round() as u8;
        }
        // rugozitate: 0 = luciu oglinda, 1 = mat; valoarea din harta e deja 0..255
        a[i + 3] = (px(rough, i, 0, 128) as f32 * rough_mult.clamp(0.0, 2.0)).round().clamp(0.0, 255.0) as u8;
        // normala OpenGL: (0.5, 0.5, 1) = plana; harta DirectX (normal-dx) are canalul Y inversat
        let nx = (px(normal, i, 0, 128) as f32 / 255.0 * 2.0 - 1.0) * normal_strength.clamp(0.0, 2.0);
        let ny = (px(normal, i, 1, 128) as f32 / 255.0 * 2.0 - 1.0) * normal_strength.clamp(0.0, 2.0);
        let nz = (px(normal, i, 2, 255) as f32 / 255.0 * 2.0 - 1.0).max(0.05);
        let inv = 1.0 / nx.hypot(ny).hypot(nz).max(1e-4);
        b[i] = ((nx * inv + 1.0) * 127.5).round().clamp(0.0, 255.0) as u8;
        b[i + 1] = ((ny * inv + 1.0) * 127.5).round().clamp(0.0, 255.0) as u8;
        b[i + 2] = ((nz * inv + 1.0) * 127.5).round().clamp(0.0, 255.0) as u8;
        b[i + 3] = (px(metal, i, 0, 0) as f32 * metal_mult.clamp(0.0, 2.0)).round().clamp(0.0, 255.0) as u8;
    }
    (a, b)
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

/// Folderul bibliotecii PBR livrate cu aplicatia: `<exe>\materials` (installer) sau, la rularea
/// din sursele proiectului, `third_party\pbr-library` (descarcat de `fetch_pro_library`).
pub fn bundled_library_dir() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.to_path_buf();
    let mut cands = vec![dir.join("materials")];
    // target\release\ManageBIM.exe -> <repo>\third_party\pbr-library
    if let Some(repo) = dir.parent().and_then(|p| p.parent()) {
        cands.push(repo.join("third_party").join("pbr-library"));
    }
    cands.into_iter().find(|d| d.join("library.bimmat").is_file())
}

/// Biblioteca PBR livrata cu aplicatia (materiale scanate ambientCG, CC0); caile din fisier sunt
/// relative la folderul ei. Gol daca nu e instalata.
pub fn bundled_library() -> Vec<Material> {
    let Some(dir) = bundled_library_dir() else { return Vec::new() };
    let Some(mut mats) = std::fs::read_to_string(dir.join("library.bimmat")).ok().and_then(|s| serde_json::from_str::<Vec<Material>>(&s).ok()) else { return Vec::new() };
    for m in &mut mats {
        for p in [&mut m.texture, &mut m.normal_map, &mut m.roughness_map, &mut m.metallic_map, &mut m.ao_map, &mut m.height_map] {
            if !p.is_empty() && !std::path::Path::new(p.as_str()).is_absolute() {
                *p = dir.join(p.as_str()).to_string_lossy().to_string();
            }
        }
    }
    // doar materialele ale caror imagini exista (installer partial / fisiere sterse)
    mats.retain(|m| m.texture.is_empty() || std::path::Path::new(&m.texture).is_file());
    mats
}

/// Material din biblioteca procedurala veche (gen_pbr_library): imagini in `Materials\PBR Library\<nume>\albedo.png`.
pub fn is_generated(m: &Material) -> bool {
    let t = m.texture.replace('\\', "/").to_ascii_lowercase();
    t.contains("/materials/pbr library/") && t.ends_with("/albedo.png")
}

/// Biblioteca completa: materialele incluse, inlocuite de biblioteca PBR livrata cu aplicatia,
/// apoi de ale utilizatorului (cele cu acelasi nume le inlocuiesc).
pub fn full_library() -> Vec<Material> {
    let mut lib = library();
    let bundled = bundled_library();
    for m in &bundled {
        match lib.iter_mut().find(|x| x.name == m.name) {
            Some(x) => *x = m.clone(),
            None => lib.push(m.clone()),
        }
    }
    for m in user_library() {
        // texturile procedurale generate de gen_pbr_library (0.27.21) nu mai inlocuiesc
        // materialele scanate livrate cu aplicatia; materialele proprii ale utilizatorului da
        if is_generated(&m) && bundled.iter().any(|b| b.name == m.name) {
            continue;
        }
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
/// Rolul e in Ultimul cuvant din nume, nu oriunde in el: "Metal063_2K_Color.png" e albedo,
/// nu metallic (desi numele materialului contine "metal").
pub fn pbr_role(file: &str) -> Option<&'static str> {
    let l = file.to_ascii_lowercase();
    let base = l.rsplit(['/', '\\']).next().unwrap_or(&l);
    if ["preview", "thumb", "sphere", "usdc", "blend", "mtlx", "tres"].iter().any(|k| base.contains(k)) {
        return None;
    }
    let stem = base.rsplit_once('.').map(|(a, _)| a).unwrap_or(base);
    let words: Vec<&str> = stem.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    // rolurile se cauta in ultimele doua cuvinte (ex. "normal dx", "base color")
    for w in words.iter().rev().take(2) {
        if w.contains("normal") || *w == "nrm" || *w == "nor" || w.contains("nor_gl") || w.contains("norgl") || w.contains("normdx") || w.contains("normgl") {
            return Some("normal");
        }
        if w.contains("rough") {
            return Some("roughness");
        }
        if w.contains("metal") || w.contains("metallic") || *w == "mtl" {
            return Some("metallic");
        }
        if w.contains("occlusion") || *w == "ao" {
            return Some("ao");
        }
        if w.contains("height") || w.contains("displace") || w.contains("disp") || w.contains("bump") {
            return Some("height");
        }
        if w.contains("gloss") {
            return Some("gloss");
        }
        if w.contains("albedo") || w.contains("basecolor") || *w == "base" || w.contains("diffuse") || *w == "diff" || w.contains("color") || w.contains("colour") || *w == "col" {
            return Some("albedo");
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
    while words.len() > 1 {
        let last = words.last().map(|w| w.to_ascii_lowercase()).unwrap_or_default();
        // sufixe cunoscute + hash-uri de genul "xqucc9" (freepbr.com)
        let hash = last.len() >= 5 && last.chars().any(|c| c.is_ascii_digit()) && last.chars().all(|c| c.is_ascii_alphanumeric());
        if matches!(last.as_str(), "bl" | "ue" | "unity" | "png" | "jpg" | "1k" | "2k" | "4k" | "8k" | "pbr" | "freepbr" | "material" | "diffuse") || hash {
            words.pop();
        } else {
            break;
        }
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
        let zip = crate::fcstd::Zip::parse(std::fs::read(path).map_err(|e| format!("Cannot read {path}: {e}"))?).map_err(|_| format!("{} is not a ZIP archive (or it is damaged).", crate::file_name(path)))?;
        let names: Vec<String> = zip.names().filter(|n| is_image(n) && !n.starts_with("__MACOSX")).cloned().collect();
        let mut unsupported = 0;
        for n in names {
            match zip.read(&n) {
                Some(b) => files.push((n.rsplit(['/', '\\']).next().unwrap_or(&n).to_string(), b)),
                None => unsupported += 1,
            }
        }
        if files.is_empty() && unsupported > 0 {
            return Err(format!(
                "{}: the images are compressed with a method ManageBIM cannot unpack (Deflate64 / LZMA / BZIP2). Unzip the archive and use Library › Import PBR folder… on the folder.",
                crate::file_name(path)
            ));
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
    // alegerea fisierului pentru fiecare rol: se prefera normala OpenGL fata de DirectX
    let get = |role: &str| -> String {
        let mut best: Option<(u8, String)> = None;
        for (r, f) in &paths {
            if *r != Some(role) {
                continue;
            }
            let l = f.to_string_lossy().to_ascii_lowercase();
            let score: u8 = match role {
                "normal" if l.contains("gl") => 2,
                "normal" if l.contains("dx") => 1,
                "albedo" if l.contains("albedo") || l.contains("basecolor") => 2,
                _ => 0,
            };
            if best.as_ref().is_none_or(|(s, _)| score > *s) {
                best = Some((score, f.to_string_lossy().to_string()));
            }
        }
        best.map(|(_, p)| p).unwrap_or_default()
    };
    let tex = paths[albedo].1.to_string_lossy().to_string();
    // validare: imaginea de culoare trebuie sa fie citibila la rezolutia curenta
    load_texture(&tex)?;
    let map = |role: &str| {
        let p = get(role);
        if p.is_empty() { None } else { load_texture(&p).ok() }
    };
    let gloss = match (map("roughness"), map("gloss")) {
        (Some(r), _) => (100.0 - avg(&r, 0) / 2.55).clamp(2.0, 95.0) as u8,
        (None, Some(g)) => (avg(&g, 0) / 2.55).clamp(2.0, 95.0) as u8,
        _ => 20,
    };
    let metal = map("metallic").map(|m| (avg(&m, 0) / 2.55).clamp(0.0, 100.0) as u8).unwrap_or(0);
    Ok(Material {
        name,
        color: [255, 255, 255], transparency: 0, gloss, metal,
        pattern: Pattern::None, scale: 1.0,
        texture: tex,
        normal_map: get("normal"),
        roughness_map: get("roughness"),
        metallic_map: get("metallic"),
        ao_map: get("ao"),
        height_map: get("height"),
        normal_strength: 1.0, rough_mult: 1.0, metal_mult: 1.0,
        coat: 0, coat_roughness: 5, sheen: 0, ior: default_ior(),
    })
}

#[cfg(test)]
mod bundled_tests {
    use super::*;

    #[test]
    fn generated_procedural_textures_are_recognised() {
        let mut m = library()[0].clone();
        m.texture = r"C:\Users\X\AppData\Roaming\ManageBIM\Materials\PBR Library\Brick_red\albedo.png".into();
        assert!(is_generated(&m));
        m.texture = r"C:\Users\X\AppData\Roaming\ManageBIM\Materials\Pro Library\Bricks104\Bricks104_1K-JPG_Color.jpg".into();
        assert!(!is_generated(&m));
        m.texture = r"D:\textures\my_brick.png".into();
        assert!(!is_generated(&m));
    }
}

/// Copiaza hartile unui material importat (MaterialX, .bimmat) in biblioteca ta
/// (`<biblioteca>/<nume>/`) si schimba caile: materialul ramane intreg daca folderul descarcat e sters.
/// Hartile deja din biblioteca sau inexistente raman cum sunt. Intoarce numarul de fisiere copiate.
pub fn adopt_maps(m: &mut Material) -> usize {
    let Some(root) = user_library_dir() else { return 0 };
    let safe: String = m.name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
    let dir = root.join(safe.trim());
    let root_c = std::fs::canonicalize(&root).unwrap_or(root.clone());
    let mut n = 0;
    for slot in [&mut m.texture, &mut m.normal_map, &mut m.roughness_map, &mut m.metallic_map, &mut m.ao_map, &mut m.height_map] {
        if slot.is_empty() {
            continue;
        }
        let src = std::path::Path::new(slot.as_str());
        let Ok(src_c) = std::fs::canonicalize(src) else { continue };
        if !src_c.is_file() || src_c.starts_with(&root_c) {
            continue;
        }
        let Some(fname) = src.file_name() else { continue };
        if std::fs::create_dir_all(&dir).is_err() {
            return n;
        }
        let dst = dir.join(fname);
        if std::fs::copy(&src_c, &dst).is_ok() {
            *slot = dst.to_string_lossy().to_string();
            n += 1;
        }
    }
    n
}
