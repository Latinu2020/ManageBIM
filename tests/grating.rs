//! Gratarele Plant 3D (dreptunghi plat in DWG) primesc grosime la incarcare.
use bimrust::model::{Prop, RawElement, RawModel};
use glam::DVec3;

fn grating(tris: Vec<[DVec3; 3]>) -> RawModel {
    let n = tris.len();
    RawModel {
        elements: vec![RawElement { name: "G".into(), ifc_type: "StructureGrating".into(), guid: "g".into(), storey: String::new(), props: vec![Prop { group: "Item".into(), name: "x".into(), value: "y".into() }], tris, colors: vec![[0, 120, 160, 255]; n], default_hidden: false }],
        info: vec![],
        warnings: vec![],
    }
}

#[test]
fn flat_grating_becomes_a_closed_slab() {
    let (a, b, c, d) = (DVec3::new(0.0, 0.0, 3.0), DVec3::new(2.0, 0.0, 3.0), DVec3::new(2.0, 1.0, 3.0), DVec3::new(0.0, 1.0, 3.0));
    // al doilea triunghi intors (orientare amestecata, ca in grafica proxy)
    let mut m = grating(vec![[a, b, c], [a, d, c]]);
    m.thicken_flat_gratings(0.03);
    let e = &m.elements[0];
    assert_eq!(e.colors.len(), e.tris.len());
    // 2 m x 1 m: bare portante pe deschiderea scurta (1 m), la 30 mm de-a lungul celor 2 m (~67 bare),
    // bare transversale la 100 mm (~11), rama (4): fiecare bara = cutie de 12 triunghiuri
    let boxes = e.tris.len() / 12;
    assert_eq!(e.tris.len() % 12, 0);
    assert!((75..=90).contains(&boxes), "{boxes} bars");
    let zmin = e.tris.iter().flatten().map(|p| p.z).fold(f64::INFINITY, f64::min);
    let zmax = e.tris.iter().flatten().map(|p| p.z).fold(f64::NEG_INFINITY, f64::max);
    assert!((zmax - 3.0).abs() < 1e-9 && (zmin - 2.97).abs() < 1e-9, "bars 30 mm under the walking surface");
    let xmax = e.tris.iter().flatten().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let ymax = e.tris.iter().flatten().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    assert!((xmax - 2.0).abs() < 0.01 && (ymax - 1.0).abs() < 0.01, "stays on the outline");
    // cutii inchise, orientate spre exterior: volumul = suma barelor (mult sub o placa plina de 2 x 1 x 0.03)
    let vol: f64 = e.tris.iter().map(|t| t[0].dot(t[1].cross(t[2])) / 6.0).sum();
    assert!(vol > 0.002 && vol < 0.03, "volume {vol}");
    // barele portante pe directia scurta (Y): o bara are lungimea ~1 m pe Y
    assert!(e.props.iter().any(|p| p.name == "Grating" && p.value.contains("2000 x 1000")));
}

#[test]
fn thickness_from_plant3d_size_in_inches() {
    // contur 2.2606 x 1.3462 m = 89 x 53 in; descrierea din proiect "89x53x1.375" (inch) -> bare de 34.9 mm
    let (a, b, c, d) = (DVec3::new(0.0, 0.0, 5.0), DVec3::new(2.2606, 0.0, 5.0), DVec3::new(2.2606, 1.3462, 5.0), DVec3::new(0.0, 1.3462, 5.0));
    let mut m = grating(vec![[a, b, c], [a, c, d]]);
    m.elements[0].props.push(Prop { group: "Plant 3D".into(), name: "Part Size Long Description".into(), value: "89x53x1.375".into() });
    m.thicken_flat_gratings(0.03);
    let e = &m.elements[0];
    let zmin = e.tris.iter().flatten().map(|p| p.z).fold(f64::INFINITY, f64::min);
    assert!((5.0 - zmin - 1.375 * 0.0254).abs() < 1e-4, "thickness from the database: {}", 5.0 - zmin);
    assert!(e.props.iter().any(|p| p.name == "Grating" && p.value.contains("from the Plant 3D project")));
}

#[test]
fn non_flat_or_other_elements_untouched() {
    let mut m = grating(vec![[DVec3::ZERO, DVec3::X, DVec3::Z], [DVec3::ZERO, DVec3::Y, DVec3::new(0.0, 0.0, 1.0)]]);
    m.elements[0].tris[1] = [DVec3::ZERO, DVec3::X, DVec3::new(0.0, 1.0, 1.0)];
    let before = m.elements[0].tris.len();
    m.thicken_flat_gratings(0.03);
    assert_eq!(m.elements[0].tris.len(), before, "not planar -> unchanged");
    let mut m = grating(vec![[DVec3::ZERO, DVec3::X, DVec3::Y]]);
    m.elements[0].ifc_type = "StructurePlate".into();
    m.thicken_flat_gratings(0.03);
    assert_eq!(m.elements[0].tris.len(), 1);
}
