fn main() {
    for path in std::env::args().skip(1) {
        let m = bimrust::bimc::read(&path).unwrap();
        let valves: Vec<_> = m.elements.iter().filter(|e| e.ifc_type.to_lowercase().contains("valve")).collect();
        let types: std::collections::BTreeSet<&str> = valves.iter().map(|e| e.ifc_type.as_str()).collect();
        let no_geo = valves.iter().filter(|e| e.tris.is_empty()).count();
        let min_t = valves.iter().map(|e| e.tris.len()).min().unwrap_or(0);
        println!("{}: {} valves {:?}, without geometry {}, min triangles {}", path.rsplit('\\').next().unwrap(), valves.len(), types, no_geo, min_t);
        if let Some(v) = valves.first() {
            println!("  example {:?} [{}] tris={}", v.name, v.ifc_type, v.tris.len());
            for k in ["Tag", "Size", "Spec", "LineNumberTag", "PartFamilyLongDesc", "ValveBody", "ActuatorType", "EndType", "PressureClass", "Manufacturer", "Status"] {
                if let Some(p) = v.props.iter().find(|p| p.group == "Plant 3D" && p.name == k) { println!("    {k} = {}", p.value); }
            }
            println!("    ({} Plant 3D properties in total)", v.props.iter().filter(|p| p.group == "Plant 3D").count());
        }
    }
}
