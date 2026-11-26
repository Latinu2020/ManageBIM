//! The .mbref fixture used by tools/test-addins.ps1 (AutoCAD MBREFERENCE): written from a small test IFC with
//! the same writer as Share › Reference, so the add-in test reads a real file. Run with
//! `cargo test --test mbref_fixture -- --ignored` after the format changes.
use bimrust::*;

#[test]
#[ignore]
fn write_reference_fixture() {
    let raw = load_any("tests/data/wall_opening.ifc", &|_, _| {}).expect("test IFC");
    let model = build_model(raw, "tests/data/wall_opening.ifc", glam::DVec3::ZERO);
    let items: Vec<_> = model.elements.iter().map(|e| (&model, e)).collect();
    let bytes = app::write_mbref(&items, glam::DVec3::ZERO);
    std::fs::write("tests/fixtures/addins/wall_opening.mbref", &bytes).unwrap();
    assert!(bytes.starts_with(b"MBREF1"));
}
