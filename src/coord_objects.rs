//! Coordination objects (as in Navisworks / Solibri / BIMcollab Zoom):
//! - Clearance zone: the maintenance / access zone around a piece of equipment (access door, space for
//!   dismantling, operating zone). Any element that enters it is a "soft" conflict.
//! - Provision for void: a proposed opening / penetration through walls and slabs for services, exported as
//!   IfcBuildingElementProxy with PredefinedType PROVISIONFORVOID (the most requested flow in MEP coordination).
//! - Placeholder: a volume reserved for equipment not modelled yet or for routing corridors.
//!
//! The objects are axis-aligned boxes, saved in the project and turned into a virtual model
//! "Coordination objects": they appear in Selection Tree, Properties, Find, Clash Detective and the IFC export.
use crate::model::{Prop, RawElement, RawModel};
use glam::{DVec3, Vec3};
use serde::{Deserialize, Serialize};

/// the "file" path of the virtual model (it does not exist on disk; it is not reloaded)
pub const COORD_PATH: &str = "<coordination objects>";
pub const COORD_NAME: &str = "Coordination objects";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CoordKind {
    #[default]
    Clearance,
    ProvisionForVoid,
    Placeholder,
}

impl CoordKind {
    pub const ALL: [CoordKind; 3] = [CoordKind::Clearance, CoordKind::ProvisionForVoid, CoordKind::Placeholder];
    pub fn label(self) -> &'static str {
        match self {
            Self::Clearance => "Clearance zone",
            Self::ProvisionForVoid => "Provision for void",
            Self::Placeholder => "Placeholder volume",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Self::Clearance => "🛡",
            Self::ProvisionForVoid => "⬚",
            Self::Placeholder => "📦",
        }
    }
    pub fn tip(self) -> &'static str {
        match self {
            Self::Clearance => "Maintenance / access space around equipment (door swing, filter removal, valve operation). Clashes with it are soft clashes.",
            Self::ProvisionForVoid => "Proposed opening for services through a wall or slab, sent to the structural / architectural designer. Exported to IFC as IfcBuildingElementProxy PROVISIONFORVOID.",
            Self::Placeholder => "Space reserved for equipment that is not modelled yet, or a corridor reserved for a route.",
        }
    }
    /// the default colour (sRGB + alpha): transparent green / orange / violet
    pub fn color(self) -> [u8; 4] {
        match self {
            Self::Clearance => [70, 200, 120, 90],
            Self::ProvisionForVoid => [255, 140, 40, 150],
            Self::Placeholder => [150, 110, 230, 110],
        }
    }
    /// the PredefinedType / ObjectType value in IFC
    /// the type shown in Selection Tree; on IFC export it becomes IfcBuildingElementProxy with this ObjectType
    /// (PredefinedType is written as a property)
    pub fn ifc_type(self) -> &'static str {
        self.label()
    }
    pub fn predefined(self) -> &'static str {
        match self {
            Self::Clearance => "CLEARANCE",
            Self::ProvisionForVoid => "PROVISIONFORVOID",
            Self::Placeholder => "PROVISIONFORSPACE",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CoordObject {
    /// stable identifier (the IFC GUID is derived from it)
    pub id: u64,
    pub name: String,
    pub kind: CoordKind,
    /// the box in the LOCAL coordinates of the scene (like the model vertices)
    pub min: [f32; 3],
    pub max: [f32; 3],
    #[serde(default)]
    pub discipline: String,
    /// Proposed / Approved / Rejected / Built
    #[serde(default = "proposed")]
    pub status: String,
    /// for voids: the host element (wall / slab), name or GUID
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub note: String,
    #[serde(default = "yes")]
    pub visible: bool,
    /// the rotation around the vertical axis through the centre (degrees); `min` / `max` are the UNROTATED box
    #[serde(default)]
    pub rot_deg: f32,
}

fn proposed() -> String {
    "Proposed".into()
}
fn yes() -> bool {
    true
}

pub const STATUSES: [&str; 4] = ["Proposed", "Approved", "Rejected", "Built"];

impl CoordObject {
    pub fn new(id: u64, kind: CoordKind, min: Vec3, max: Vec3, n: usize) -> Self {
        let (a, b) = (min.min(max), min.max(max));
        CoordObject {
            id,
            name: format!("{} {}", kind.label(), n),
            kind,
            min: a.to_array(),
            max: b.to_array(),
            discipline: String::new(),
            status: proposed(),
            host: String::new(),
            note: String::new(),
            visible: true,
            rot_deg: 0.0,
        }
    }
    pub fn size(&self) -> Vec3 {
        Vec3::from(self.max) - Vec3::from(self.min)
    }
    pub fn center(&self) -> Vec3 {
        (Vec3::from(self.max) + Vec3::from(self.min)) * 0.5
    }
    /// sets the dimensions keeping the centre (minimum 1 cm)
    pub fn set_size(&mut self, s: Vec3) {
        let c = self.center();
        let h = s.max(Vec3::splat(0.01)) * 0.5;
        self.min = (c - h).to_array();
        self.max = (c + h).to_array();
    }
    pub fn set_center(&mut self, c: Vec3) {
        let h = self.size() * 0.5;
        self.min = (c - h).to_array();
        self.max = (c + h).to_array();
    }
    pub fn guid_seed(&self) -> String {
        format!("coord|{}", self.id)
    }
}

/// box -> 12 triangles with the faces oriented outwards
fn box_tris(a: DVec3, b: DVec3) -> Vec<[DVec3; 3]> {
    let c = |x: bool, y: bool, z: bool| DVec3::new(if x { b.x } else { a.x }, if y { b.y } else { a.y }, if z { b.z } else { a.z });
    let quads = [
        [c(false, false, false), c(false, true, false), c(true, true, false), c(true, false, false)], // bottom
        [c(false, false, true), c(true, false, true), c(true, true, true), c(false, true, true)],     // top
        [c(false, false, false), c(true, false, false), c(true, false, true), c(false, false, true)], // front
        [c(false, true, false), c(false, true, true), c(true, true, true), c(true, true, false)],     // back
        [c(false, false, false), c(false, false, true), c(false, true, true), c(false, true, false)], // left
        [c(true, false, false), c(true, true, false), c(true, true, true), c(true, false, true)],     // right
    ];
    quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect()
}

/// The virtual model with the visible objects (WORLD coordinates = local + origin).
pub fn to_raw(objs: &[CoordObject], origin: DVec3) -> RawModel {
    let mut elements = Vec::new();
    for o in objs {
        let a = Vec3::from(o.min).as_dvec3() + origin;
        let b = Vec3::from(o.max).as_dvec3() + origin;
        // the rotation around the vertical through the centre
        let c = (a + b) * 0.5;
        let (s, co) = (o.rot_deg as f64).to_radians().sin_cos();
        let rot = |p: DVec3| {
            let (x, y) = (p.x - c.x, p.y - c.y);
            DVec3::new(c.x + x * co - y * s, c.y + x * s + y * co, p.z)
        };
        let tris: Vec<[DVec3; 3]> = box_tris(a, b).into_iter().map(|t| t.map(rot)).collect();
        let col = o.kind.color();
        let s = o.size();
        let p = |g: &str, n: &str, v: String| Prop { group: g.into(), name: n.into(), value: v };
        let mut props = vec![
            p("Coordination", "Kind", o.kind.label().into()),
            p("Coordination", "Status", o.status.clone()),
            p("Coordination", "PredefinedType", o.kind.predefined().into()),
            p("Coordination", "Width (X)", format!("{:.3} m", s.x)),
            p("Coordination", "Depth (Y)", format!("{:.3} m", s.y)),
            p("Coordination", "Height (Z)", format!("{:.3} m", s.z)),
            p("Coordination", "Bottom elevation", format!("{:.3} m", a.z)),
            p("Coordination", "Volume", format!("{:.3} m³", s.x * s.y * s.z)),
            p("Coordination", "Rotation", format!("{:.1}°", o.rot_deg)),
        ];
        if !o.discipline.is_empty() {
            props.push(p("Coordination", "Discipline", o.discipline.clone()));
        }
        if !o.host.is_empty() {
            props.push(p("Coordination", "Host", o.host.clone()));
        }
        if !o.note.is_empty() {
            props.push(p("Coordination", "Note", o.note.clone()));
        }
        elements.push(RawElement {
            name: o.name.clone(),
            ifc_type: o.kind.ifc_type().into(),
            guid: crate::ifc_export::guid_from(&o.guid_seed()),
            storey: String::new(),
            props,
            colors: vec![col; tris.len()],
            tris,
            default_hidden: !o.visible,
        });
    }
    RawModel { elements, info: vec![("Source".into(), "ManageBIM coordination objects".into())], warnings: Vec::new() }
}

/// The box of a proposed void at the intersection of a service (its box) with a wall / slab (the host box),
/// with `margin` extra metres on each side, in the directions perpendicular to the penetration.
pub fn void_at(service: (Vec3, Vec3), host: (Vec3, Vec3), margin: f32) -> Option<(Vec3, Vec3)> {
    let lo = service.0.max(host.0);
    let hi = service.1.min(host.1);
    if (0..3).any(|d| lo[d] > hi[d] + 1e-4) {
        return None;
    }
    // the penetration axis = the thinnest dimension of the host (the wall / slab thickness)
    let hs = host.1 - host.0;
    let thin = if hs.x <= hs.y && hs.x <= hs.z { 0 } else if hs.y <= hs.z { 1 } else { 2 };
    let (mut a, mut b) = (lo, hi);
    for d in 0..3 {
        if d == thin {
            // the void crosses the whole thickness of the host
            a[d] = host.0[d];
            b[d] = host.1[d];
        } else {
            a[d] -= margin;
            b[d] += margin;
        }
    }
    Some((a, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_model_has_one_closed_box_per_object() {
        let o = CoordObject::new(7, CoordKind::ProvisionForVoid, Vec3::ZERO, Vec3::new(0.4, 0.2, 0.3), 1);
        let raw = to_raw(&[o], DVec3::new(100.0, 200.0, 0.0));
        assert_eq!(raw.elements.len(), 1);
        let e = &raw.elements[0];
        assert_eq!(e.tris.len(), 12);
        assert_eq!(e.ifc_type, "Provision for void");
        assert!(e.props.iter().any(|p| p.value == "PROVISIONFORVOID"));
        // faces outwards: the signed volume (divergence theorem) is positive and equal to the box volume
        let vol: f64 = e.tris.iter().map(|t| t[0].dot(t[1].cross(t[2])) / 6.0).sum();
        assert!((vol - 0.4 * 0.2 * 0.3).abs() < 1e-6, "{vol}");
    }

    #[test]
    fn void_spans_the_wall_thickness_with_margin() {
        // a pipe along X through a 0.25 m thick wall (perpendicular to X)
        let pipe = (Vec3::new(-2.0, -0.05, 1.0), Vec3::new(2.0, 0.05, 1.1));
        let wall = (Vec3::new(0.0, -3.0, 0.0), Vec3::new(0.25, 3.0, 3.0));
        let (a, b) = void_at(pipe, wall, 0.05).unwrap();
        assert!((a.x - 0.0).abs() < 1e-6 && (b.x - 0.25).abs() < 1e-6);
        assert!((a.y + 0.10).abs() < 1e-5 && (b.y - 0.10).abs() < 1e-5);
        assert!((a.z - 0.95).abs() < 1e-5 && (b.z - 1.15).abs() < 1e-5);
        assert!(void_at(pipe, (Vec3::new(5.0, 5.0, 5.0), Vec3::new(6.0, 6.0, 6.0)), 0.05).is_none());
    }
}
