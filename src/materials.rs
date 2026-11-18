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
        Material::new("Concrete", [178, 178, 172], 0, 10, 0, Stone, 1.6),
        Material::new("Exposed concrete", [150, 150, 147], 0, 6, 0, Stone, 0.8),
        Material::new("Brick red", [158, 72, 50], 0, 5, 0, Brick, 0.5),
        Material::new("Brick yellow", [205, 170, 110], 0, 5, 0, Brick, 0.5),
        Material::new("Plaster white", [238, 236, 230], 0, 5, 0, None, 1.0),
        Material::new("Paint grey", [128, 132, 138], 0, 25, 0, None, 1.0),
        Material::new("Glass clear", [175, 210, 232], 70, 92, 0, None, 1.0),
        Material::new("Glass tinted", [85, 125, 145], 55, 92, 0, None, 1.0),
        Material::new("Steel", [150, 155, 162], 0, 70, 80, None, 1.0),
        Material::new("Aluminium", [200, 203, 208], 0, 60, 70, None, 1.0),
        Material::new("Galvanized steel", [178, 182, 185], 0, 45, 60, Stone, 0.3),
        Material::new("Copper", [184, 115, 70], 0, 70, 85, None, 1.0),
        Material::new("Wood oak", [172, 126, 80], 0, 25, 0, Wood, 1.0),
        Material::new("Wood dark", [98, 64, 40], 0, 30, 0, Wood, 1.0),
        Material::new("Ceramic tiles", [226, 226, 220], 0, 55, 0, Tiles, 0.3),
        Material::new("Roof tiles", [152, 62, 45], 0, 10, 0, Tiles, 0.35),
        Material::new("Facade panels", [210, 212, 215], 0, 40, 20, Panels, 1.2),
        Material::new("Asphalt", [62, 64, 67], 0, 5, 0, Stone, 0.5),
        Material::new("Grass", [86, 140, 62], 0, 0, 0, Grass, 1.0),
        Material::new("Water", [60, 110, 150], 40, 95, 0, None, 1.0),
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
