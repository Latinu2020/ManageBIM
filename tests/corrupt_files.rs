//! Robustness: truncated files or files with damaged bytes must never stop the app (panic).
//! For every test file: cuts at several lengths and copies with changed bytes
//! (deterministic generator, the same variants on every run). Error = ok; panic = bug.
use bimrust::*;

/// simple pseudo-random generator (xorshift), so the results are reproducible
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn variants(data: &[u8], seed: u64) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for pct in [0, 1, 5, 10, 25, 50, 75, 90, 99] {
        let n = data.len() * pct / 100;
        out.push((format!("cut{pct}"), data[..n].to_vec()));
    }
    let mut r = Rng(seed | 1);
    // MB_FUZZ_N=500 cargo test --test corrupt_files: a longer search
    let n: usize = std::env::var("MB_FUZZ_N").ok().and_then(|v| v.parse().ok()).unwrap_or(24);
    for k in 0..n {
        let mut d = data.to_vec();
        if d.is_empty() {
            break;
        }
        let flips = 1 + (r.next() % 16) as usize;
        for _ in 0..flips {
            let i = (r.next() as usize) % d.len();
            d[i] = match r.next() % 4 {
                0 => 0,
                1 => 0xFF,
                2 => d[i] ^ (1 << (r.next() % 8)),
                _ => b"0123456789-.,;()'#=$*eE\n"[(r.next() % 24) as usize],
            };
        }
        out.push((format!("flip{k}"), d));
    }
    // huge numbers in place of a digit (indices, vertex counts, lengths)
    let mut d = data.to_vec();
    if let Some(i) = d.iter().position(|c| c.is_ascii_digit()) {
        d.splice(i..i + 1, b"99999999999".iter().copied());
        out.push(("bignum".into(), d));
    }
    out
}

#[test]
fn corrupt_files_never_panic() {
    let files = [
        "tests/fixtures/demo_arhitectura.ifc",
        "tests/fixtures/demo_instalatii.ifc",
        "tests/data/wall_opening.ifc",
        "tests/fixtures/fmt/attribs.dxf",
        "tests/fixtures/fmt/from_blender.obj",
        "tests/fixtures/fmt/test.obj",
        "tests/fixtures/fmt/segmented.ply",
        "tests/fixtures/fmt/test.ply",
        "tests/fixtures/fmt/test.fbx",
        "tests/fixtures/fmt/test_ascii.stl",
        "tests/fixtures/fmt/test_bin.stl",
        "samples/Block_A_MEP.ifc.bimc",
        "samples/Block_A_Architecture_Structure.ifc.bimc",
    ];
    let dir = std::env::temp_dir().join(format!("mb_corrupt_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // panic messages do not flood the test output; they are collected in the list
    std::panic::set_hook(Box::new(|_| {}));
    let mut panics = Vec::new();
    let mut runs = 0;
    for (fi, f) in files.iter().enumerate() {
        let Ok(data) = std::fs::read(f) else { continue };
        let ext = std::path::Path::new(f).extension().unwrap().to_string_lossy().to_string();
        for (name, bytes) in variants(&data, 0x9E37_79B9_7F4A_7C15 ^ fi as u64) {
            let p = dir.join(format!("{fi}_{name}.{ext}"));
            std::fs::write(&p, &bytes).unwrap();
            let path = p.to_string_lossy().to_string();
            runs += 1;
            let r = std::panic::catch_unwind(|| {
                // the .bimc cache has its own reader; a model that was read must also build (indices, colours)
                let raw = if path.ends_with(".bimc") { bimc::read(&path) } else { load_any(&path, &|_, _| {}) };
                if let Ok(raw) = raw {
                    let _ = build_model(raw, &path, glam::DVec3::ZERO);
                }
            });
            if let Err(e) = r {
                let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                panics.push(format!("{f} [{name}]: {msg}"));
            } else {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
    let _ = std::panic::take_hook();
    assert!(runs > 100, "{runs}");
    assert!(panics.is_empty(), "{} of {runs} corrupt files panicked (kept in {}):\n{}", panics.len(), dir.display(), panics.join("\n"));
    let _ = std::fs::remove_dir_all(&dir);
}
