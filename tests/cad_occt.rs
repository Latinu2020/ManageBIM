//! Cititorul nativ OpenCascade: STEP / IGES / BREP (tests/fixtures/cad, generate cu tools/occt-fixtures.cpp).
use bimrust::model::{RawElement, RawModel};
use bimrust::*;
use glam::DVec3;

/// None daca ManageBIM a fost compilat fara OCCT (third_party\occt lipsa) - testele se sar atunci.
fn load(f: &str) -> Option<RawModel> {
    if !occt::BUILT_WITH_OCCT {
        eprintln!("OpenCascade not built - skipping {f}");
        return None;
    }
    assert!(occt::available(), "OCCT built but not loadable: {:?}", occt::status());
    Some(load_any(&format!("tests/fixtures/cad/{f}"), &|_, _| {}).unwrap_or_else(|e| panic!("{f}: {e}")))
}

fn bbox(tris: &[[DVec3; 3]]) -> (DVec3, DVec3) {
    let mut mn = DVec3::splat(f64::INFINITY);
    let mut mx = -mn;
    for t in tris {
        for p in t {
            mn = mn.min(*p);
            mx = mx.max(*p);
        }
    }
    (mn, mx)
}

fn size(e: &RawElement) -> DVec3 {
    let (a, b) = bbox(&e.tris);
    b - a
}

fn prop<'a>(e: &'a RawElement, n: &str) -> &'a str {
    e.props.iter().find(|p| p.name == n).map(|p| p.value.as_str()).unwrap_or("")
}

/// volum cu semn (pozitiv = normale spre exterior)
fn volume(e: &RawElement) -> f64 {
    e.tris.iter().map(|t| t[0].dot(t[1].cross(t[2])) / 6.0).sum()
}

/// culoare sRGB cu toleranta de rotunjire (+-2)
fn col(c: &[u8; 4], want: [u8; 3]) -> bool {
    (0..3).all(|i| (c[i] as i32 - want[i] as i32).abs() <= 2)
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn step_assembly_names_paths_colours() {
    let Some(m) = load("asm.step") else { return };
    let names: Vec<&str> = m.elements.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["Base plate", "Pin:1", "Pin:2", "Cone:1"], "one element per leaf instance");

    let plate = &m.elements[0];
    let s = size(plate);
    assert!(near(s.x, 0.2, 1e-6) && near(s.y, 0.1, 1e-6) && near(s.z, 0.01, 1e-6), "mm -> m: {s:?}");
    assert!(near(volume(plate), 0.2 * 0.1 * 0.01, 1e-9), "closed, outward oriented: {}", volume(plate));
    assert_eq!(plate.storey, "Bracket assembly");
    assert_eq!(plate.ifc_type, "Part");
    // placa rosie, fata de sus galbena (culoare pe fata, XCAF)
    let yellow = plate.colors.iter().filter(|c| col(c, [255, 217, 0])).count();
    let red = plate.colors.iter().filter(|c| col(c, [204, 26, 26])).count();
    assert_eq!(yellow, 2, "top face colour: {:?}", plate.colors);
    assert_eq!(red, plate.colors.len() - 2);
    assert_eq!(plate.colors.len(), plate.tris.len());

    // doua instante ale aceleiasi piese, la pozitii diferite
    let (p1, p2) = (&m.elements[1], &m.elements[2]);
    assert_eq!(prop(p1, "Part name"), "Pin");
    assert!(p1.colors.iter().all(|c| col(c, [26, 77, 230])), "pin colour");
    let (c1, c2) = (bbox(&p1.tris), bbox(&p2.tris));
    assert!(near((c1.0.x + c1.1.x) / 2.0, 0.05, 1e-4) && near((c2.0.x + c2.1.x) / 2.0, 0.15, 1e-4));
    assert!(near(c1.0.z, 0.01, 1e-6) && near(c1.1.z, 0.06, 1e-6), "placed on the plate");
    assert_ne!(p1.guid, p2.guid);

    let cone = &m.elements[3];
    assert_eq!(cone.storey, "Cone sub-assembly");
    assert_eq!(prop(cone, "Assembly path"), "Bracket assembly / Cone sub-assembly");
    assert!(cone.colors.iter().all(|c| col(c, [26, 179, 51])));
    assert!(m.info.iter().any(|(k, v)| k == "File units" && !v.is_empty()), "{:?}", m.info);
    assert!(m.info.iter().any(|(k, v)| k == "Converted from" && v.contains("OpenCascade")));
}

#[test]
fn step_units_metres() {
    let Some(m) = load("asm_m.step") else { return };
    assert_eq!(m.elements.len(), 1);
    assert_eq!(m.elements[0].name, "Metre box");
    let s = size(&m.elements[0]);
    assert!(near(s.x, 2.0, 1e-6) && near(s.y, 1.0, 1e-6) && near(s.z, 0.5, 1e-6), "metres stay metres: {s:?}");
    assert!(near(volume(&m.elements[0]), 1.0, 1e-6));
}

#[test]
fn iges_geometry_and_colours() {
    let Some(m) = load("asm.igs") else { return };
    let all: Vec<[DVec3; 3]> = m.elements.iter().flat_map(|e| e.tris.iter().copied()).collect();
    let (a, b) = bbox(&all);
    let s = b - a;
    assert!(near(s.x, 0.2, 1e-5) && near(s.y, 0.1, 1e-5) && near(s.z, 0.06, 1e-5), "IGES mm -> m: {s:?}");
    let cols: std::collections::BTreeSet<[u8; 3]> = m.elements.iter().flat_map(|e| e.colors.iter().map(|c| [c[0], c[1], c[2]])).collect();
    // (culoarea conului din sub-ansamblu nu e scrisa de IGESCAFControl_Writer - IGES nu are ansambluri)
    for want in [[204, 26, 26], [26, 77, 230], [255, 217, 0]] {
        assert!(cols.iter().any(|c| col(&[c[0], c[1], c[2], 255], want)), "IGES colour {want:?} in {cols:?}");
    }
}

#[test]
fn brep_compound_splits_into_parts() {
    let Some(m) = load("parts.brep") else { return };
    assert_eq!(m.elements.len(), 2, "box + sphere");
    let s = size(&m.elements[1]);
    assert!(near(s.x, 0.08, 1e-3) && near(s.z, 0.08, 1e-3), "sphere r 40 mm: {s:?}");
    assert!(near(volume(&m.elements[0]), 0.1 * 0.05 * 0.02, 1e-9));
}

#[test]
fn freecad_fcstd_without_freecad() {
    let Some(m) = load("PartDesignExample.FCStd") else { return };
    // doar Body (forma finala); schitele, axele si operatiile Pad / Pocket nu dubleaza geometria
    let names: Vec<&str> = m.elements.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["Body"], "{names:?}");
    let b = &m.elements[0];
    assert_eq!(b.ifc_type, "Body");
    assert!(prop(b, "Type") == "PartDesign::Body");
    assert!(b.tris.len() > 100);
    let s = size(b);
    assert!(s.x > 0.01 && s.x < 1.0, "mm -> m: {s:?}");
    assert!(volume(b) > 0.0, "closed, outward oriented");
    // culoarea din GuiDocument.xml (ShapeAppearance): gri FreeCAD implicit 204
    assert!(b.colors.iter().all(|c| col(c, [204, 204, 204]) && c[3] == 255), "{:?}", &b.colors[..1]);
    assert!(m.info.iter().any(|(k, v)| k == "Document" && v == "PartDesignExample"));
}

#[test]
fn damaged_cad_file_is_an_error_not_a_crash() {
    if !occt::BUILT_WITH_OCCT {
        return;
    }
    let dir = std::env::temp_dir().join(format!("mb_occt_bad_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (n, body) in [("bad.step", "ISO-10303-21;\nHEADER;\nnot a step file"), ("bad.igs", "garbage"), ("bad.brep", "garbage")] {
        let p = dir.join(n);
        std::fs::write(&p, body).unwrap();
        let r = occt::load(&p.to_string_lossy(), occt::CadKind::from_ext(&formats::ext_of(n)).unwrap(), Default::default(), &|_, _| {});
        assert!(r.is_err(), "{n} should fail");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unicode_path() {
    if !occt::BUILT_WITH_OCCT {
        return;
    }
    let dir = std::env::temp_dir().join(format!("mb_occt_ÄƒÃ®È™È›_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("piesÄƒ.stp");
    std::fs::copy("tests/fixtures/cad/asm_m.step", &p).unwrap();
    let m = load_any(&p.to_string_lossy(), &|_, _| {}).unwrap();
    assert_eq!(m.elements.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
