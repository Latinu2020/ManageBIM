//! Lumini punctuale plasate in model (lampi), folosite de randarea offline.
//! Pozitia e in coordonatele locale ale scenei (aceleasi ca vertecii / camera).
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneLight {
    pub name: String,
    pub pos: [f32; 3],
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

impl Default for SceneLight {
    fn default() -> Self {
        Self { name: "Lamp".into(), pos: [0.0; 3], color: [255, 236, 200], power: 2.0, radius: 12.0, enabled: true }
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
