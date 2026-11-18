// Ce obiecte custom NU devin elemente: pe clase, cu histograma inregistrarilor proxy.
use acadrust::entities::EntityType;
use std::collections::{BTreeMap, HashSet};

fn u(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap())).unwrap_or(0)
}

fn main() {
    for path in std::env::args().skip(1) {
        let doc = match acadrust::DwgReader::from_mmap(&path).and_then(|mut r| { r.options.failsafe = true; r.read() }) {
            Ok(d) => d,
            Err(e) => {
                println!("{path}: UNREADABLE {e}");
                continue;
            }
        };
        let shown: HashSet<String> = bimrust::plant3d::collect(&doc).into_iter().map(|o| o.handle).collect();
        let mut st: BTreeMap<String, (usize, usize, usize, BTreeMap<u32, usize>)> = BTreeMap::new();
        for e in doc.model_space_entities() {
            let EntityType::Unknown(un) = e else { continue };
            let s = st.entry(un.dxf_name.clone()).or_default();
            s.0 += 1;
            if shown.contains(&format!("{:X}", un.common.handle.value())) {
                s.1 += 1;
                continue;
            }
            let Some(g) = un.common.graphic_data.as_deref() else {
                s.2 += 1;
                continue;
            };
            let mut o = 8;
            for _ in 0..u(g, 4) {
                let sz = u(g, o) as usize;
                if sz < 8 {
                    break;
                }
                *s.3.entry(u(g, o + 4)).or_default() += 1;
                o += sz;
            }
        }
        println!("===== {path}");
        for (k, (n, ok, nog, h)) in st {
            println!("  {k}: total {n}, shown {ok}, no proxy {nog}, missing-records {h:?}");
        }
    }
}
