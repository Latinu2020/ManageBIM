//! .bimc v2 (pe bucati, scris de add-in-ul Navisworks pentru modele foarte mari) == v1 la citire.
use bimrust::*;

#[test]
fn v2_chunks_read_exactly_like_v1() {
    let p = |_: f32, _: &str| {};
    let raw = load_any("samples/Block_A_Architecture_Structure.ifc", &p).unwrap();
    let v1 = bimc::decode(&bimc::encode(&raw)).unwrap();
    for chunk in [1, 7, 1000] {
        let bytes = bimc::encode_v2(&raw, chunk);
        assert_eq!(&bytes[4..8], &[2, 0, 2, 0], "version 2, flags 2");
        let v2 = bimc::decode(&bytes).unwrap();
        assert_eq!(v2.elements.len(), v1.elements.len(), "chunk {chunk}");
        assert_eq!(v2.info, v1.info);
        assert_eq!(v2.warnings, v1.warnings);
        for (a, b) in v1.elements.iter().zip(&v2.elements) {
            assert_eq!((&a.name, &a.ifc_type, &a.guid, &a.storey, a.default_hidden), (&b.name, &b.ifc_type, &b.guid, &b.storey, b.default_hidden));
            assert_eq!(a.props.len(), b.props.len());
            assert!(a.props.iter().zip(&b.props).all(|(x, y)| x.group == y.group && x.name == y.name && x.value == y.value));
            assert_eq!(a.tris, b.tris);
            assert_eq!(a.colors, b.colors);
        }
    }
}

#[test]
fn v2_truncated_file_gives_a_clear_error() {
    let p = |_: f32, _: &str| {};
    let raw = load_any("samples/Block_A_MEP.ifc", &p).unwrap();
    let bytes = bimc::encode_v2(&raw, 5);
    let cut = &bytes[..bytes.len() * 2 / 3];
    let e = match bimc::decode(cut) {
        Ok(_) => panic!("a truncated file must not load"),
        Err(e) => e,
    };
    assert!(e.contains("Incomplete") || e.contains("chunk"), "{e}");
}
