// Proprietatile Plant 3D pe tipuri si acoperirea: p3dprops <dwg>... [--dump]
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let dump = a.iter().any(|x| x == "--dump");
    for path in a.iter().filter(|x| !x.starts_with("--")) {
        let m = match bimrust::formats::convert_dwg_native(path, &|_, _| {}) {
            Ok(m) => m,
            Err(e) => {
                println!("{path}: {e}");
                continue;
            }
        };
        let plant: Vec<_> = m.elements.iter().filter(|e| e.props.iter().any(|p| p.name == "Entity" && p.value.starts_with("ACPP"))).collect();
        let with = plant.iter().filter(|e| e.props.iter().any(|p| p.group.starts_with("Plant 3D") && p.name == "Class")).count();
        let lines = plant.iter().filter(|e| e.storey.starts_with("Line ")).count();
        println!("{}: {with} / {} Plant 3D elements with project properties, {lines} grouped by line", path.rsplit('\\').next().unwrap_or(path), plant.len());
        if dump {
            let mut seen = std::collections::HashSet::new();
            for e in &plant {
                if seen.insert(e.ifc_type.clone()) {
                    println!("  [{}] {} | storey {}", e.ifc_type, e.name, e.storey);
                    for p in e.props.iter().filter(|p| p.group.starts_with("Plant 3D")) {
                        println!("     {} / {} = {}", p.group, p.name, p.value);
                    }
                }
            }
        }
    }
}
