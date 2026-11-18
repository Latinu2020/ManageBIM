use bimrust::*;
use std::time::Instant;
fn main() {
    let path = std::env::args().nth(1).expect("path");
    let p = |_: f32, _: &str| {};
    let t = Instant::now();
    let raw = load_any(&path, &p).unwrap();
    let t_parse = t.elapsed();
    let ntri: usize = raw.elements.iter().map(|e| e.tris.len()).sum();
    let nprop: usize = raw.elements.iter().map(|e| e.props.len()).sum();
    let o = raw.center();
    let t = Instant::now();
    let bytes = bimc::encode(&raw);
    let t_enc = t.elapsed();
    let t = Instant::now();
    let _m = build_model(raw, &path, o);
    let t_build = t.elapsed();
    let t = Instant::now();
    let raw2 = bimc::decode(&bytes).unwrap();
    let t_dec = t.elapsed();
    let t = Instant::now();
    let _m2 = build_model(raw2, &path, o);
    let t_build2 = t.elapsed();
    let src = std::fs::metadata(&path).unwrap().len();
    println!("{}: {} elements, {} triangles, {} properties", file_name(&path), _m.elements.len(), ntri, nprop);
    println!("  IFC  {:>9.2} MB  parse {:>6.0} ms + index {:>5.0} ms", src as f64 / 1e6, t_parse.as_secs_f64() * 1e3, t_build.as_secs_f64() * 1e3);
    println!("  BIMC {:>9.2} MB  read  {:>6.0} ms + index {:>5.0} ms   (encode {:.0} ms)  -> {:.1}x smaller", bytes.len() as f64 / 1e6, t_dec.as_secs_f64() * 1e3, t_build2.as_secs_f64() * 1e3, t_enc.as_secs_f64() * 1e3, src as f64 / bytes.len() as f64);
}
