// Verificare plasare Plant 3D: bbox-ul fiecarui element (geometria blocurilor din catalog) vs bbox-ul
// graficii proxy a obiectului (conturul desenat de Plant 3D in coordonate lume).
// cargo run --release --example p3ddiag -- <fisier.dwg>...
use std::collections::{BTreeMap, HashMap};

fn main() {
    for path in std::env::args().skip(1) {
        let t = std::time::Instant::now();
        let doc = match acadrust::DwgReader::from_mmap(&path).and_then(|mut r| { r.options.failsafe = true; r.read() }) {
            Ok(d) => d,
            Err(e) => {
                println!("{path}: acadrust cannot read it: {e}");
                continue;
            }
        };
        let objs = bimrust::plant3d::collect(&doc);
        let by_h: HashMap<String, &bimrust::plant3d::PlantObject> = objs.iter().map(|o| (o.handle.clone(), o)).collect();
        let m = match bimrust::formats::convert_dwg_native(&path, &|_, _| {}) {
            Ok(m) => m,
            Err(e) => {
                println!("{path}: ERROR {e}");
                continue;
            }
        };
        let units: f64 = m.info.iter().find(|(k, _)| k == "Units").and_then(|(_, v)| v.trim_end_matches(" m").parse().ok()).unwrap_or(1.0);
        println!("===== {path} ({:.2} s) elements={} plant objects={}", t.elapsed().as_secs_f64(), m.elements.len(), objs.len());
        for (k, v) in &m.info {
            println!("  info {k}: {v}");
        }
        for w in &m.warnings {
            println!("  warn {w}");
        }
        // abatere maxima (unitati desen) intre bbox-ul elementului si bbox-ul proxy, pe clase
        let mut stat: BTreeMap<String, (usize, usize, f64)> = BTreeMap::new();
        let mut worst: Vec<(f64, String)> = Vec::new();
        for e in &m.elements {
            let Some(o) = by_h.get(&e.guid) else { continue };
            let Some((plo, phi)) = (if o.blocks.is_empty() { o.proxy_bbox } else { o.placed_bbox }) else { continue };
            let mut lo = glam::DVec3::splat(f64::INFINITY);
            let mut hi = -lo;
            for t in &e.tris {
                for p in t {
                    lo = lo.min(*p / units);
                    hi = hi.max(*p / units);
                }
            }
            let d = (lo - plo).abs().max_element().max((hi - phi).abs().max_element());
            let s = stat.entry(o.class.clone()).or_default();
            s.0 += 1;
            let size = (phi - plo).max_element().max(1.0);
            if d <= 0.05 * size + 0.5 {
                s.1 += 1;
            }
            s.2 = s.2.max(d);
            worst.push((d / size, format!("{} {} d={d:.2} size={size:.1} el={lo:.1?}..{hi:.1?} proxy={plo:.1?}..{phi:.1?}", o.class, o.handle)));
        }
        println!("  class: (elements, bbox matches proxy, max deviation)");
        for (k, v) in &stat {
            println!("    {k}: {v:?}");
        }
        worst.sort_by(|a, b| b.0.total_cmp(&a.0));
        for w in worst.iter().take(6) {
            println!("    worst {:.3} {}", w.0, w.1);
        }
    }
}
