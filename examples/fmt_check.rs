fn main() {
    for f in std::env::args().skip(1) {
        match bimrust::load_any(&f, &|_, _| {}) {
            Ok(m) => {
                println!("{}: {} elements, warnings {:?}", bimrust::file_name(&f), m.elements.len(), m.warnings);
                for e in m.elements.iter().take(8) {
                    let mut mn = glam::DVec3::splat(f64::INFINITY);
                    let mut mx = -mn;
                    for t in &e.tris { for p in t { mn = mn.min(*p); mx = mx.max(*p); } }
                    println!("   {:28} type={:16} level={:10} tris={:4} col={:?} size={:.3?} props={}", e.name, e.ifc_type, e.storey, e.tris.len(), e.colors.first(), (mx - mn).to_array(), e.props.len());
                }
            }
            Err(e) => println!("{}: ERROR {e}", bimrust::file_name(&f)),
        }
    }
}
