//! Regresie: sortarea "naturala" a valorilor de proprietati trebuie sa fie o ordine totala.
//! Varianta veche compara unele perechi ca numere si altele ca text ("10" < "2 m" < "ab" < "10"),
//! iar Rust opreste programul la o astfel de sortare - ManageBIM se inchidea la deschiderea
//! exporturilor Plant 3D (valori ca 2", DN50, 100 mm, 1/2", 0.5, Sch 40...).
use bimrust::app::natural_cmp;
use std::cmp::Ordering;

fn values() -> Vec<String> {
    let mut v: Vec<String> = [
        "10", "2 m", "ab", "AB", "2\"", "1/2\"", "DN50", "DN100", "100 mm", "0.5", ".5", "-3", "+3", "1e3", "e", "", " ", "Sch 40", "Sch 80", "Level 2",
        "Level 10", "level 2", "00", "0", "007", "7", "x7", "x07", "1.0", "1", "NaN", "inf", "∞", "Ø50", "2-P-001", "2-P-01", "2.5\"", "3/4\"", "CS", "SS316",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    // valori pseudo-aleatoare amestecate (determinist)
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let parts = ["", "DN", "Sch ", "Level ", "-", ".", "\"", " mm", "x", "0", "1", "2", "10", "a", "B"];
    for _ in 0..400 {
        let mut s = String::new();
        for _ in 0..3 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            s.push_str(parts[(x % parts.len() as u64) as usize]);
        }
        v.push(s);
    }
    v
}

#[test]
fn natural_order_is_total_and_sorting_does_not_panic() {
    let v = values();
    // antisimetrie si tranzitivitate pe toate perechile / un esantion mare de tripleti
    for a in &v {
        assert_eq!(natural_cmp(a, a), Ordering::Equal, "{a:?}");
        for b in &v {
            assert_eq!(natural_cmp(a, b), natural_cmp(b, a).reverse(), "{a:?} {b:?}");
        }
    }
    let n = v.len();
    for i in (0..n).step_by(3) {
        for j in (0..n).step_by(5) {
            for k in (0..n).step_by(7) {
                let (a, b, c) = (&v[i], &v[j], &v[k]);
                if natural_cmp(a, b) != Ordering::Greater && natural_cmp(b, c) != Ordering::Greater {
                    assert_ne!(natural_cmp(a, c), Ordering::Greater, "not transitive: {a:?} <= {b:?} <= {c:?}");
                }
            }
        }
    }
    let mut s = v.clone();
    s.sort_by(|a, b| natural_cmp(a, b)); // nu trebuie sa opreasca programul
    // ordinea naturala asteptata
    assert_eq!(natural_cmp("Level 2", "Level 10"), Ordering::Less);
    assert_eq!(natural_cmp("2 m", "10"), Ordering::Less);
    assert_eq!(natural_cmp("DN50", "DN100"), Ordering::Less);
}
