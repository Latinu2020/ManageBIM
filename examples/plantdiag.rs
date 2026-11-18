fn main() {
    for path in std::env::args().skip(1) {
        let m = bimrust::bimc::read(&path).unwrap();
        println!("===== {} : {} elements", path.rsplit('\\').next().unwrap(), m.elements.len());
        for w in m.warnings.iter().take(8) { println!("  {w}"); }
        if let Some(e) = m.elements.iter().find(|e| matches!(e.ifc_type.as_str(), "Tank" | "Pump" | "Vessel" | "HeatExchanger")) {
            println!("  equipment {:?} [{}] tris={} props:", e.name, e.ifc_type, e.tris.len());
            for p in &e.props { println!("    {} / {} = {}", p.group, p.name, &p.value[..p.value.len().min(70)]); }
        }
    }
}
