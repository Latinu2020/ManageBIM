//! Obiecte de coordonare (ca in Navisworks / Solibri / BIMcollab Zoom):
//! - Clearance zone: zona de mentenanta / acces in jurul unui echipament (usa de vizitare, spatiu de
//!   demontare, zona de operare). Orice element care intra in ea e un conflict "soft".
//! - Provision for void: propunere de gol / trecere prin pereti si placi pentru instalatii, exportata ca
//!   IfcBuildingElementProxy cu PredefinedType PROVISIONFORVOID (fluxul cel mai cerut in coordonarea MEP).
//! - Placeholder: volum rezervat pentru echipamente inca nemodelate sau coridoare de trasee.
//!
//! Obiectele sunt cutii orientate pe axe, salvate in proiect si transformate intr-un model virtual
//! "Coordination objects": apar in Selection Tree, Properties, Find, Clash Detective si exportul IFC.
use crate::model::{Prop, RawElement, RawModel};
use glam::{DVec3, Vec3};
use serde::{Deserialize, Serialize};

/// calea "fisierului" modelului virtual (nu exista pe disc; nu se reincarca)
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
    /// culoarea implicita (sRGB + alfa): verde transparent / portocaliu / violet
    pub fn color(self) -> [u8; 4] {
        match self {
            Self::Clearance => [70, 200, 120, 90],
            Self::ProvisionForVoid => [255, 140, 40, 150],
            Self::Placeholder => [150, 110, 230, 110],
        }
    }
    /// valoarea PredefinedType / ObjectType in IFC
    /// tipul afisat in Selection Tree; la exportul IFC devine IfcBuildingElementProxy cu acest ObjectType
    /// (PredefinedType e scris ca proprietate)
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
    /// identificator stabil (GUID-ul IFC se deriva din el)
    pub id: u64,
    pub name: String,
    pub kind: CoordKind,
    /// cutia in coordonatele LOCALE ale scenei (ca vertecsii modelelor)
    pub min: [f32; 3],
    pub max: [f32; 3],
    #[serde(default)]
    pub discipline: String,
    /// Proposed / Approved / Rejected / Built
    #[serde(default = "proposed")]
    pub status: String,
    /// pentru goluri: elementul gazda (perete / placa), nume sau GUID
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub note: String,
    #[serde(default = "yes")]
    pub visible: bool,
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
        }
    }
    pub fn size(&self) -> Vec3 {
        Vec3::from(self.max) - Vec3::from(self.min)
    }
    pub fn center(&self) -> Vec3 {
        (Vec3::from(self.max) + Vec3::from(self.min)) * 0.5
    }
    /// seteaza dimensiunile pastrand centrul (minim 1 cm)
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

/// cutie -> 12 triunghiuri cu fetele orientate spre exterior
fn box_tris(a: DVec3, b: DVec3) -> Vec<[DVec3; 3]> {
    let c = |x: bool, y: bool, z: bool| DVec3::new(if x { b.x } else { a.x }, if y { b.y } else { a.y }, if z { b.z } else { a.z });
    let quads = [
        [c(false, false, false), c(false, true, false), c(true, true, false), c(true, false, false)], // jos
        [c(false, false, true), c(true, false, true), c(true, true, true), c(false, true, true)],     // sus
        [c(false, false, false), c(true, false, false), c(true, false, true), c(false, false, true)], // fata
        [c(false, true, false), c(false, true, true), c(true, true, true), c(true, true, false)],     // spate
        [c(false, false, false), c(false, false, true), c(false, true, true), c(false, true, false)], // stanga
        [c(true, false, false), c(true, true, false), c(true, true, true), c(true, false, true)],     // dreapta
    ];
    quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect()
}

/// Modelul virtual cu obiectele vizibile (coordonate LUME = locale + origine).
pub fn to_raw(objs: &[CoordObject], origin: DVec3) -> RawModel {
    let mut elements = Vec::new();
    for o in objs {
        let a = Vec3::from(o.min).as_dvec3() + origin;
        let b = Vec3::from(o.max).as_dvec3() + origin;
        let tris = box_tris(a, b);
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

/// Cutia unui gol propus la intersectia unei instalatii (cutia ei) cu un perete / o placa (cutia gazdei),
/// cu `margin` metri in plus pe fiecare parte, pe directiile perpendiculare pe trecere.
pub fn void_at(service: (Vec3, Vec3), host: (Vec3, Vec3), margin: f32) -> Option<(Vec3, Vec3)> {
    let lo = service.0.max(host.0);
    let hi = service.1.min(host.1);
    if (0..3).any(|d| lo[d] > hi[d] + 1e-4) {
        return None;
    }
    // axa de trecere = cea mai subtire dimensiune a gazdei (grosimea peretelui / placii)
    let hs = host.1 - host.0;
    let thin = if hs.x <= hs.y && hs.x <= hs.z { 0 } else if hs.y <= hs.z { 1 } else { 2 };
    let (mut a, mut b) = (lo, hi);
    for d in 0..3 {
        if d == thin {
            // golul traverseaza toata grosimea gazdei
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
        // fetele spre exterior: volumul semnat (teorema divergentei) e pozitiv si egal cu volumul cutiei
        let vol: f64 = e.tris.iter().map(|t| t[0].dot(t[1].cross(t[2])) / 6.0).sum();
        assert!((vol - 0.4 * 0.2 * 0.3).abs() < 1e-6, "{vol}");
    }

    #[test]
    fn void_spans_the_wall_thickness_with_margin() {
        // conducta de-a lungul lui X prin peretele gros de 0.25 m (perpendicular pe X)
        let pipe = (Vec3::new(-2.0, -0.05, 1.0), Vec3::new(2.0, 0.05, 1.1));
        let wall = (Vec3::new(0.0, -3.0, 0.0), Vec3::new(0.25, 3.0, 3.0));
        let (a, b) = void_at(pipe, wall, 0.05).unwrap();
        assert!((a.x - 0.0).abs() < 1e-6 && (b.x - 0.25).abs() < 1e-6);
        assert!((a.y + 0.10).abs() < 1e-5 && (b.y - 0.10).abs() < 1e-5);
        assert!((a.z - 0.95).abs() < 1e-5 && (b.z - 1.15).abs() < 1e-5);
        assert!(void_at(pipe, (Vec3::new(5.0, 5.0, 5.0), Vec3::new(6.0, 6.0, 6.0)), 0.05).is_none());
    }
}
