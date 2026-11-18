// Schema unei baze Plant 3D (.dcf, SQLite), doar citire: dcfschema <fisier.dcf> [tabel...]
use rusqlite::{Connection, OpenFlags};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let c = Connection::open_with_flags(&a[0], OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).unwrap();
    let mut st = c.prepare("select name from sqlite_master where type='table' order by name").unwrap();
    let tables: Vec<String> = st.query_map([], |r| r.get(0)).unwrap().flatten().collect();
    if a.len() == 1 {
        for t in &tables {
            let n: i64 = c.query_row(&format!("select count(*) from \"{t}\""), [], |r| r.get(0)).unwrap_or(-1);
            let cols: Vec<String> = c.prepare(&format!("pragma table_info(\"{t}\")")).unwrap().query_map([], |r| r.get::<_, String>(1)).unwrap().flatten().collect();
            println!("{t} ({n}): {}", cols.join(", "));
        }
        return;
    }
    if a[1].to_lowercase().starts_with("select") {
        let mut st = c.prepare(&a[1]).unwrap();
        let n = st.column_count();
        let mut rows = st.query([]).unwrap();
        while let Some(r) = rows.next().unwrap() {
            let v: Vec<String> = (0..n).map(|i| match r.get_ref(i).unwrap() {
                rusqlite::types::ValueRef::Text(t) => String::from_utf8_lossy(t).to_string(),
                x => format!("{x:?}"),
            }).collect();
            println!("{}", v.join(" | "));
        }
        return;
    }
    for t in &a[1..] {
        println!("== {t}");
        let mut st = c.prepare(&format!("select * from \"{t}\" limit 4")).unwrap();
        let names: Vec<String> = st.column_names().iter().map(|s| s.to_string()).collect();
        let mut rows = st.query([]).unwrap();
        while let Some(r) = rows.next().unwrap() {
            let v: Vec<String> = (0..names.len()).map(|i| format!("{}={:?}", names[i], r.get_ref(i).unwrap())).collect();
            println!("  {}", v.join(" | "));
        }
    }
}
