fn main() {
    let path = std::env::args().nth(1).unwrap();
    let m = bimrust::bimc::read(&path).unwrap();
    let with_plant = m.elements.iter().filter(|e| e.props.iter().any(|p| p.group == "Plant 3D")).count();
    println!("{} elements, {} with Plant 3D properties; {} warnings", m.elements.len(), with_plant, m.warnings.len());
    for w in m.warnings.iter().take(4) { println!("  warning: {w}"); }
    let mut types: std::collections::BTreeMap<&str, (usize, usize)> = Default::default();
    for e in &m.elements { let t = types.entry(e.ifc_type.as_str()).or_insert((0, 0)); t.0 += 1; if e.props.iter().any(|p| p.group == "Plant 3D") { t.1 += 1; } }
    println!("type: (elements, with Plant data)");
    for (t, (n, p)) in types.iter().take(20) { println!("  {t:<28} {n:>5} {p:>5}"); }
    if let Some(e) = m.elements.iter().find(|e| e.props.iter().any(|p| p.group == "Plant 3D")) {
        println!("example {:?} [{}]:", e.name, e.ifc_type);
        for p in e.props.iter().filter(|p| p.group == "Plant 3D").take(12) { println!("    {} = {}", p.name, p.value); }
    }
}
