//! Lumini punctuale plasate in model (lampi), folosite de randarea offline.
//! Pozitia e in coordonatele locale ale scenei (aceleasi ca vertecii / camera).
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum LightKind {
    #[default]
    Point,
    Spot,
    Distant,
}

impl LightKind {
    pub const ALL: [LightKind; 3] = [LightKind::Point, LightKind::Spot, LightKind::Distant];
    pub fn label(self) -> &'static str {
        match self { Self::Point => "Point light", Self::Spot => "Spot light", Self::Distant => "Distant light" }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneLight {
    pub name: String,
    #[serde(default)]
    pub kind: LightKind,
    pub pos: [f32; 3],
    /// directia / tinta pentru Spot si Distant
    #[serde(default)]
    pub target: [f32; 3],
    /// unghiul cone-ului pentru Spot (grade, 5..=120)
    #[serde(default = "default_angle")]
    pub angle: f32,
    /// culoarea luminii (sRGB 0..255)
    pub color: [u8; 3],
    /// putere relativa (0..10)
    pub power: f32,
    /// raza de actiune in metri (0 = nelimitata)
    pub radius: f32,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool { true }
fn default_angle() -> f32 { 45.0 }

impl Default for SceneLight {
    fn default() -> Self {
        Self {
            name: "Lamp".into(), kind: LightKind::Point, pos: [0.0; 3], target: [0.0, 0.0, -1.0],
            angle: default_angle(), color: [255, 236, 200], power: 2.0, radius: 12.0, enabled: true,
        }
    }
}

impl SceneLight {
    /// culoarea liniara (pentru calculul luminii)
    pub fn linear(&self) -> [f32; 3] {
        let f = |c: u8| {
            let x = c as f32 / 255.0;
            if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
        };
        [f(self.color[0]), f(self.color[1]), f(self.color[2])]
    }
}
