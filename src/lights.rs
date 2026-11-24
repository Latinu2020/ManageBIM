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
    /// raza de actiune in metri (0 = nelimitata, lumina nu scade cu distanta)
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
    /// directia luminii (de la pozitie spre tinta), normalizata; implicit in jos
    pub fn direction(&self) -> glam::Vec3 {
        let d = glam::Vec3::from(self.target) - glam::Vec3::from(self.pos);
        let n = d.normalize_or_zero();
        if n == glam::Vec3::ZERO { -glam::Vec3::Z } else { n }
    }
    /// distanta pozitie -> tinta (minim 1 m, ca tinta sa nu cada peste lumina)
    fn aim_dist(&self) -> f32 {
        (glam::Vec3::from(self.target) - glam::Vec3::from(self.pos)).length().max(1.0)
    }
    /// seteaza directia pastrand distanta pana la tinta
    pub fn set_direction(&mut self, dir: glam::Vec3) {
        let d = dir.normalize_or_zero();
        if d == glam::Vec3::ZERO || !d.is_finite() {
            return;
        }
        let p = glam::Vec3::from(self.pos);
        self.target = (p + d * self.aim_dist()).to_array();
    }
    /// roteste directia in jurul unei axe (lume) cu unghiul dat (radiani)
    pub fn rotate_direction(&mut self, axis: glam::Vec3, angle: f32) {
        if !angle.is_finite() || angle.abs() < 1e-7 {
            return;
        }
        let d = glam::Quat::from_axis_angle(axis.normalize(), angle).mul_vec3(self.direction());
        self.set_direction(d);
    }
    /// (azimut de la +X in sens trigonometric, inclinare fata de orizontala; negativ = in jos), grade
    pub fn aim_angles(&self) -> (f32, f32) {
        let d = self.direction();
        (d.y.atan2(d.x).to_degrees(), d.z.clamp(-1.0, 1.0).asin().to_degrees())
    }
    pub fn set_aim_angles(&mut self, az_deg: f32, tilt_deg: f32) {
        let (a, t) = (az_deg.to_radians(), tilt_deg.clamp(-90.0, 90.0).to_radians());
        self.set_direction(glam::Vec3::new(t.cos() * a.cos(), t.cos() * a.sin(), t.sin()));
    }
    /// culoarea liniara (pentru calculul luminii)
    pub fn linear(&self) -> [f32; 3] {
        let f = |c: u8| {
            let x = c as f32 / 255.0;
            if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
        };
        [f(self.color[0]), f(self.color[1]), f(self.color[2])]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn rotate_keeps_distance_and_turns_direction() {
        let mut l = SceneLight { kind: LightKind::Spot, pos: [0.0, 0.0, 3.0], target: [5.0, 0.0, 3.0], ..Default::default() };
        l.rotate_direction(Vec3::Z, std::f32::consts::FRAC_PI_2);
        let t = Vec3::from(l.target);
        assert!((t - Vec3::new(0.0, 5.0, 3.0)).length() < 1e-4, "{t:?}");
    }

    #[test]
    fn aim_angles_roundtrip() {
        let mut l = SceneLight { kind: LightKind::Spot, pos: [1.0, 2.0, 3.0], target: [1.0, 2.0, 0.0], ..Default::default() };
        l.set_aim_angles(30.0, -45.0);
        let (az, tilt) = l.aim_angles();
        assert!((az - 30.0).abs() < 1e-3 && (tilt + 45.0).abs() < 1e-3, "{az} {tilt}");
    }

    #[test]
    fn degenerate_target_points_down() {
        let l = SceneLight { pos: [1.0, 1.0, 1.0], target: [1.0, 1.0, 1.0], ..Default::default() };
        assert_eq!(l.direction(), -Vec3::Z);
    }
}
