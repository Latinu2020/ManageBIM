//! Genereaza iconita PNG pentru pachetele Linux / FreeBSD (rulat manual: cargo test --test gen_icon -- --ignored).
#![allow(dead_code)]
include!("../tools/icon_gen.rs");

#[test]
#[ignore]
fn write_linux_icon() {
    let n = 256u32;
    let rgba = icon_rgba(n);
    let f = std::fs::File::create("packaging/linux/managebim.png").unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), n, n);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&rgba).unwrap();
}
